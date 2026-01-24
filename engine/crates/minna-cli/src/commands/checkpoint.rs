use std::io::{self, Read};

use anyhow::{Context, Result};
use serde::Deserialize;

use minna_core::{Checkpoint, CheckpointStore};

/// Input from Claude Code hooks (via stdin).
#[derive(Debug, Deserialize)]
pub struct HookInput {
    /// Path to the transcript file (JSONL format)
    pub transcript_path: Option<String>,
    /// What triggered this checkpoint
    #[serde(default = "default_trigger")]
    pub trigger: String,
}

fn default_trigger() -> String {
    "manual".to_string()
}

/// A single entry in the Claude Code transcript.
#[derive(Debug, Deserialize)]
struct TranscriptEntry {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    tool: Option<String>,
    tool_input: Option<serde_json::Value>,
    message: Option<TranscriptMessage>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessage {
    content: Option<serde_json::Value>,
}

/// Extracted context from parsing a transcript.
#[derive(Debug, Default)]
struct ExtractedContext {
    summary: String,
    current_task: String,
    next_steps: String,
    files: Vec<String>,
    title: String,
}

/// Parse a transcript file and extract relevant context.
fn parse_transcript(path: &str) -> Result<ExtractedContext> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read transcript: {}", path))?;

    let mut ctx = ExtractedContext::default();
    let mut seen_files = std::collections::HashSet::new();

    // Parse JSONL (one JSON object per line)
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<TranscriptEntry>(line) {
            // Extract files from tool calls
            if let Some(tool) = &entry.tool {
                if let Some(input) = &entry.tool_input {
                    // Look for file paths in tool inputs
                    if tool == "Read" || tool == "Edit" || tool == "Write" {
                        if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                            if !seen_files.contains(path) {
                                seen_files.insert(path.to_string());
                                ctx.files.push(path.to_string());
                            }
                        }
                    }
                }
            }

            // Try to extract summary from assistant messages
            if entry.entry_type.as_deref() == Some("assistant") {
                if let Some(msg) = &entry.message {
                    if let Some(content) = &msg.content {
                        // Use the last substantial assistant message as summary basis
                        if let Some(text) = content.as_str() {
                            if text.len() > 50 && ctx.summary.is_empty() {
                                // Take first 200 chars as summary
                                ctx.summary = text.chars().take(200).collect::<String>();
                                if text.len() > 200 {
                                    ctx.summary.push_str("...");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Set defaults if extraction failed
    if ctx.summary.is_empty() {
        ctx.summary = "Manual checkpoint".to_string();
    }
    if ctx.title.is_empty() {
        ctx.title = format!(
            "Session Checkpoint {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M")
        );
    }
    if ctx.current_task.is_empty() {
        ctx.current_task = "Task in progress".to_string();
    }
    if ctx.next_steps.is_empty() {
        ctx.next_steps = "- Continue from checkpoint".to_string();
    }

    // Limit files to most recent 10
    if ctx.files.len() > 10 {
        ctx.files = ctx.files.into_iter().rev().take(10).collect();
        ctx.files.reverse();
    }

    Ok(ctx)
}

/// Run the checkpoint-and-clear command.
///
/// Reads HookInput from stdin, parses transcript, saves checkpoint,
/// and outputs instructions for the user.
pub async fn run(trigger: Option<String>) -> Result<()> {
    // Read hook input from stdin
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("failed to read from stdin")?;

    // Parse hook input (or use defaults if empty)
    let hook_input: HookInput = if input.trim().is_empty() {
        HookInput {
            transcript_path: None,
            trigger: trigger.unwrap_or_else(|| "manual".to_string()),
        }
    } else {
        serde_json::from_str(&input).unwrap_or(HookInput {
            transcript_path: None,
            trigger: trigger.unwrap_or_else(|| "manual".to_string()),
        })
    };

    // Extract context from transcript if available
    let ctx = if let Some(path) = &hook_input.transcript_path {
        parse_transcript(path).unwrap_or_default()
    } else {
        ExtractedContext::default()
    };

    // Build and save checkpoint
    let checkpoint = Checkpoint::new(
        if ctx.title.is_empty() {
            format!(
                "Session Checkpoint {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M")
            )
        } else {
            ctx.title
        },
        ctx.summary,
        ctx.current_task,
        ctx.next_steps,
        ctx.files,
        hook_input.trigger,
    );

    let store = CheckpointStore::default_path();
    let path = store.save(checkpoint)?;

    // Output instructions
    println!();
    println!("✅ Checkpoint saved to: {}", path.display());
    println!();
    println!("To restore your session:");
    println!("  1. Run /clear to reset the conversation");
    println!("  2. Run /minna load state to restore context");
    println!();

    Ok(())
}
