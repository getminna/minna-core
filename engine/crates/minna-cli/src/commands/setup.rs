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
        name: "claude-code",
        display_name: "Claude Code",
        config_paths: &["~/.claude/claude_desktop_config.json"],
    },
    AiTool {
        name: "cursor",
        display_name: "Cursor",
        config_paths: &["~/.cursor/mcp.json"],
    },
    AiTool {
        name: "zed",
        display_name: "Zed",
        config_paths: &["~/.config/zed/settings.json"],
    },
    AiTool {
        name: "antigravity",
        display_name: "Antigravity",
        config_paths: &["~/.config/antigravity/mcp_config.json"],
    },
];

pub async fn run(tool: Option<String>) -> Result<()> {
    // Handle explicit "manual" request
    if tool.as_deref() == Some("manual") {
        return show_manual_instructions();
    }

    let tool = match tool {
        Some(name) => {
            // Explicit tool specified
            AI_TOOLS
                .iter()
                .find(|t| t.name == name)
                .ok_or_else(|| {
                    anyhow!(
                        "Unknown tool: {}. Valid: claude-code, cursor, zed, antigravity, manual",
                        name
                    )
                })?
        }
        None => {
            // Try auto-detection first!
            if let Some(detected) = detect_current_ide() {
                // Auto-magic: configure silently and celebrate
                setup_tool_silent(detected).await?;
                show_magic_success(detected);
                return Ok(());
            }

            // Fall back to installed tools detection
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

/// Detect the current IDE based on environment variables
fn detect_current_ide() -> Option<&'static AiTool> {
    // Claude Code: CLAUDECODE=1
    if std::env::var("CLAUDECODE").is_ok() {
        return AI_TOOLS.iter().find(|t| t.name == "claude-code");
    }

    // Check VSCODE_* paths for Cursor/Antigravity
    let vscode_path = std::env::var("VSCODE_IPC_HOOK")
        .or_else(|_| std::env::var("VSCODE_CODE_CACHE_PATH"))
        .unwrap_or_default();

    if vscode_path.contains("Antigravity") {
        return AI_TOOLS.iter().find(|t| t.name == "antigravity");
    }
    if vscode_path.contains("Cursor") {
        return AI_TOOLS.iter().find(|t| t.name == "cursor");
    }

    // Zed: ZED_TERM or TERM_PROGRAM=Zed
    if std::env::var("ZED_TERM").is_ok()
        || std::env::var("TERM_PROGRAM")
            .map(|v| v == "Zed")
            .unwrap_or(false)
    {
        return AI_TOOLS.iter().find(|t| t.name == "zed");
    }

    None
}

/// Show a big celebration message after auto-magic setup
fn show_magic_success(tool: &AiTool) {
    println!();
    println!(
        "  {}",
        console::style("✨ MAGIC ✨").magenta().bold()
    );
    println!();
    println!(
        "  {} detected {} and configured Minna automatically!",
        console::style("Minna").cyan().bold(),
        console::style(tool.display_name).green().bold()
    );
    println!();
    println!("  {}", console::style("Your AI now has memory.").dim());
    println!();
    println!(
        "  {} Restart {} to activate.",
        console::style("→").yellow(),
        console::style(tool.display_name).white().bold()
    );
    println!();
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

/// Silent setup - no prompts, used for auto-magic detection
async fn setup_tool_silent(tool: &AiTool) -> Result<()> {
    let config_path = tool
        .config_paths
        .first()
        .map(|p| expand_path(p))
        .ok_or_else(|| anyhow!("No config path for {}", tool.name))?;

    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    let socket_path = get_socket_path();
    inject_mcp_config(&mut config, tool.name, &socket_path);

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

    Ok(())
}

/// Interactive setup with prompts
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

    let socket_path = get_socket_path();
    inject_mcp_config(&mut config, tool.name, &socket_path);

    // Write config
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

    ui::success(&format!("Done. Restart {} to activate.", tool.display_name));

    Ok(())
}

/// Inject MCP config into the appropriate location based on tool type
fn inject_mcp_config(config: &mut serde_json::Value, tool_name: &str, socket_path: &PathBuf) {
    let minna_config = json!({
        "command": "nc",
        "args": ["-U", socket_path.to_string_lossy()],
    });

    match tool_name {
        "cursor" | "claude-code" | "antigravity" => {
            if config.get("mcpServers").is_none() {
                config["mcpServers"] = json!({});
            }
            config["mcpServers"]["minna"] = minna_config;
        }
        "zed" => {
            // Zed uses 'context_servers' with a different structure
            if config.get("context_servers").is_none() {
                config["context_servers"] = json!({});
            }
            config["context_servers"]["minna"] = json!({
                "source": "custom",
                "command": "nc",
                "args": ["-U", socket_path.to_string_lossy()],
            });
        }
        _ => {}
    }
}

fn show_manual_instructions() -> Result<()> {
    let socket_path = get_socket_path();

    println!();
    println!("  Add this to your MCP configuration:");
    println!();
    println!("  {}", console::style("{").dim());
    println!(
        "    {}\"mcpServers\"{}: {{",
        console::style("").cyan(),
        console::style("").dim()
    );
    println!(
        "      {}\"minna\"{}: {{",
        console::style("").cyan(),
        console::style("").dim()
    );
    println!(
        "        {}\"command\"{}: {}\"nc\"{},",
        console::style("").cyan(),
        console::style("").dim(),
        console::style("").green(),
        console::style("").dim()
    );
    println!(
        "        {}\"args\"{}: [\"-U\", {}\"{}\"{}]",
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
    crate::paths::get_socket_path()
}
