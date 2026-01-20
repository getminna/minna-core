use anyhow::{anyhow, Result};
use serde_json::json;
use std::path::PathBuf;

use crate::ui;

struct AiTool {
    name: &'static str,
    display_name: &'static str,
    config_paths: &'static [&'static str],
}

const AI_TOOLS: &[AiTool] = &[
    AiTool {
        name: "cursor",
        display_name: "Cursor",
        config_paths: &["~/.cursor/mcp.json"],
    },
    AiTool {
        name: "claude-code",
        display_name: "Claude Code",
        config_paths: &["~/.claude/claude_desktop_config.json"],
    },
    AiTool {
        name: "vscode",
        display_name: "VS Code + Continue",
        config_paths: &["~/.continue/config.json"],
    },
    AiTool {
        name: "windsurf",
        display_name: "Windsurf",
        config_paths: &["~/.windsurf/mcp.json"],
    },
];

pub async fn run(tool: Option<String>) -> Result<()> {
    let tool = match tool {
        Some(name) => {
            AI_TOOLS
                .iter()
                .find(|t| t.name == name)
                .ok_or_else(|| anyhow!("Unknown tool: {}. Valid: cursor, claude-code, vscode, windsurf", name))?
        }
        None => {
            // Auto-detect or ask
            let detected = detect_installed_tools();
            if detected.is_empty() {
                return show_manual_instructions();
            }

            if detected.len() == 1 {
                detected[0]
            } else {
                let items: Vec<&str> = detected.iter().map(|t| t.display_name).collect();
                let selection = ui::prompt_select("Which AI tool do you use?", &items)?;
                detected[selection]
            }
        }
    };

    setup_tool(tool).await
}

fn detect_installed_tools() -> Vec<&'static AiTool> {
    AI_TOOLS
        .iter()
        .filter(|tool| {
            tool.config_paths.iter().any(|path| {
                let expanded = expand_path(path);
                expanded.parent().map(|p| p.exists()).unwrap_or(false)
            })
        })
        .collect()
}

fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(&path[2..])
    } else {
        PathBuf::from(path)
    }
}

async fn setup_tool(tool: &AiTool) -> Result<()> {
    let config_path = tool
        .config_paths
        .first()
        .map(|p| expand_path(p))
        .ok_or_else(|| anyhow!("No config path for {}", tool.name))?;

    // Check if config file exists
    if config_path.exists() {
        ui::success(&format!("Found {}", config_path.display()));
    } else {
        ui::info(&format!("Will create {}", config_path.display()));
    }

    // Ask for confirmation
    let items = &["Yes, add Minna", "No, show manual instructions"];
    let selection = ui::prompt_select(&format!("Add Minna to {}?", tool.display_name), items)?;

    if selection == 1 {
        return show_manual_instructions();
    }

    // Read existing config or create new
    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Add Minna to mcpServers
    let socket_path = get_socket_path();

    let minna_config = json!({
        "command": "nc",
        "args": ["-U", socket_path.to_string_lossy()],
    });

    // For different tools, the config structure varies
    match tool.name {
        "cursor" | "windsurf" => {
            if config.get("mcpServers").is_none() {
                config["mcpServers"] = json!({});
            }
            config["mcpServers"]["minna"] = minna_config;
        }
        "claude-code" => {
            if config.get("mcpServers").is_none() {
                config["mcpServers"] = json!({});
            }
            config["mcpServers"]["minna"] = minna_config;
        }
        "vscode" => {
            // Continue uses a different format
            if config.get("models").is_none() {
                config["models"] = json!([]);
            }
            // Add context provider instead
            ui::info("VS Code + Continue requires manual configuration.");
            return show_manual_instructions();
        }
        _ => {}
    }

    // Write config
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

    ui::success(&format!("Done. Restart {} to activate.", tool.display_name));

    Ok(())
}

fn show_manual_instructions() -> Result<()> {
    let socket_path = get_socket_path();

    println!();
    println!("  Add this to your MCP configuration:");
    println!();
    println!("  {}", console::style("{").dim());
    println!("    {}\"mcpServers\"{}: {{", console::style("").cyan(), console::style("").dim());
    println!("      {}\"minna\"{}: {{", console::style("").cyan(), console::style("").dim());
    println!(
        "        {}\"command\"{}: {}\"nc\"{},",
        console::style("").cyan(),
        console::style("").dim(),
        console::style("").green(),
        console::style("").dim()
    );
    println!(
        "        {}\"args\"{}: [{}\"{}\"{}]",
        console::style("").cyan(),
        console::style("").dim(),
        console::style("").green(),
        socket_path.display(),
        console::style("").dim()
    );
    println!("      }}");
    println!("    }}");
    println!("  }}");
    println!();

    Ok(())
}

fn get_socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".minna/mcp.sock")
}
