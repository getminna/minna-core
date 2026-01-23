//! TUI view for `minna mcp` command
//!
//! Shows tool detection and MCP config injection confirmation

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::time::Duration;

use super::theme;

const TOOLS: &[(&str, &str, &str)] = &[
    ("claude-code", "Claude Code", "~/.claude/claude_desktop_config.json"),
    ("cursor", "Cursor", "~/.cursor/mcp.json"),
    ("zed", "Zed", "~/.config/zed/settings.json"),
    ("antigravity", "Antigravity", "~/.config/antigravity/mcp_config.json"),
];

struct SetupState {
    tool: String,
    tool_display: String,
    config_path: String,
    phase: SetupPhase,
    confirmed: bool,
}

enum SetupPhase {
    Detected,
    Confirming,
    Injecting,
    Complete,
}

/// Run the setup TUI in test mode
pub async fn run_test(tool: Option<String>) -> Result<()> {
    let tool_id = tool.as_deref().unwrap_or("cursor");

    let (tool_display, config_path) = TOOLS
        .iter()
        .find(|(id, _, _)| *id == tool_id)
        .map(|(_, display, path)| (*display, *path))
        .unwrap_or(("Unknown", "~/.config/mcp.json"));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = SetupState {
        tool: tool_id.to_string(),
        tool_display: tool_display.to_string(),
        config_path: config_path.to_string(),
        phase: SetupPhase::Detected,
        confirmed: false,
    };

    loop {
        terminal.draw(|f| render(f, &state))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match (&state.phase, key.code) {
                        (_, KeyCode::Char('q') | KeyCode::Esc) => break,
                        (SetupPhase::Detected, KeyCode::Enter) => {
                            state.phase = SetupPhase::Confirming;
                        }
                        (SetupPhase::Confirming, KeyCode::Char('y') | KeyCode::Enter) => {
                            state.confirmed = true;
                            state.phase = SetupPhase::Injecting;
                        }
                        (SetupPhase::Confirming, KeyCode::Char('n')) => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Auto-advance from Injecting to Complete
        if matches!(state.phase, SetupPhase::Injecting) {
            tokio::time::sleep(Duration::from_millis(800)).await;
            state.phase = SetupPhase::Complete;
        }

        if matches!(state.phase, SetupPhase::Complete) {
            tokio::time::sleep(Duration::from_secs(2)).await;
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    if state.confirmed {
        print_complete_message(&state.tool_display);
    }

    Ok(())
}

fn render(frame: &mut Frame, state: &SetupState) {
    let area = frame.area();

    let block = Block::default().style(Style::default().bg(theme::DARK_GRAPHITE));
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);

    render_header(frame, chunks[0], state);
    render_body(frame, chunks[1], state);
    render_footer(frame, chunks[2], state);
}

fn render_header(frame: &mut Frame, area: Rect, state: &SetupState) {
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  ▓▓ ", theme::accent()),
            Span::styled(
                format!("SETUP {}", state.tool_display.to_uppercase()),
                theme::title(),
            ),
        ]),
    ]);
    frame.render_widget(header, area);
}

fn render_body(frame: &mut Frame, area: Rect, state: &SetupState) {
    let content = match state.phase {
        SetupPhase::Detected => {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ✔ ", theme::success()),
                    Span::styled(format!("Found {}", state.tool_display), theme::success()),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("     Config: ", theme::muted()),
                    Span::styled(&state.config_path, theme::accent()),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("     Press ", theme::muted()),
                    Span::styled("[Enter]", theme::accent()),
                    Span::styled(" to continue", theme::muted()),
                ]),
            ]
        }
        SetupPhase::Confirming => {
            // Confirmation box with Sunset Pink border
            let box_width = 46;
            let h_line = theme::BOX_HORIZONTAL.repeat(box_width);

            vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("   {}{}{}", theme::BOX_TOP_LEFT, h_line, theme::BOX_TOP_RIGHT),
                    theme::accent(),
                )),
                Line::from(vec![
                    Span::styled(format!("   {} ", theme::BOX_VERTICAL), theme::accent()),
                    Span::styled("                                              ", Style::default()),
                    Span::styled(format!(" {}", theme::BOX_VERTICAL), theme::accent()),
                ]),
                Line::from(vec![
                    Span::styled(format!("   {} ", theme::BOX_VERTICAL), theme::accent()),
                    Span::styled("  Add Minna to ", Style::default()),
                    Span::styled(&state.tool_display, theme::success()),
                    Span::styled("?", Style::default()),
                    Span::raw(" ".repeat(box_width - state.tool_display.len() - 18)),
                    Span::styled(format!(" {}", theme::BOX_VERTICAL), theme::accent()),
                ]),
                Line::from(vec![
                    Span::styled(format!("   {} ", theme::BOX_VERTICAL), theme::accent()),
                    Span::styled("                                              ", Style::default()),
                    Span::styled(format!(" {}", theme::BOX_VERTICAL), theme::accent()),
                ]),
                Line::from(vec![
                    Span::styled(format!("   {} ", theme::BOX_VERTICAL), theme::accent()),
                    Span::styled("  This will update: ", theme::muted()),
                    Span::raw(" ".repeat(26)),
                    Span::styled(format!(" {}", theme::BOX_VERTICAL), theme::accent()),
                ]),
                Line::from(vec![
                    Span::styled(format!("   {} ", theme::BOX_VERTICAL), theme::accent()),
                    Span::styled(format!("    {}", state.config_path), theme::accent()),
                    Span::raw(" ".repeat(box_width - state.config_path.len() - 4)),
                    Span::styled(format!(" {}", theme::BOX_VERTICAL), theme::accent()),
                ]),
                Line::from(vec![
                    Span::styled(format!("   {} ", theme::BOX_VERTICAL), theme::accent()),
                    Span::styled("                                              ", Style::default()),
                    Span::styled(format!(" {}", theme::BOX_VERTICAL), theme::accent()),
                ]),
                Line::from(vec![
                    Span::styled(format!("   {} ", theme::BOX_VERTICAL), theme::accent()),
                    Span::styled("  ", Style::default()),
                    Span::styled("[Y]", theme::success()),
                    Span::styled("es  ", Style::default()),
                    Span::styled("[N]", theme::error()),
                    Span::styled("o", Style::default()),
                    Span::raw(" ".repeat(34)),
                    Span::styled(format!(" {}", theme::BOX_VERTICAL), theme::accent()),
                ]),
                Line::from(vec![
                    Span::styled(format!("   {} ", theme::BOX_VERTICAL), theme::accent()),
                    Span::styled("                                              ", Style::default()),
                    Span::styled(format!(" {}", theme::BOX_VERTICAL), theme::accent()),
                ]),
                Line::from(Span::styled(
                    format!("   {}{}{}", theme::BOX_BOTTOM_LEFT, h_line, theme::BOX_BOTTOM_RIGHT),
                    theme::accent(),
                )),
            ]
        }
        SetupPhase::Injecting => {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ◐ ", theme::accent()),
                    Span::styled("Updating config...", theme::success()),
                ]),
            ]
        }
        SetupPhase::Complete => {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ✔ ", theme::success()),
                    Span::styled("Config updated!", theme::success()),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ⟳ ", theme::accent()),
                    Span::styled(
                        format!("Restart {} to activate Minna.", state.tool_display),
                        Style::default(),
                    ),
                ]),
            ]
        }
    };

    let body = Paragraph::new(content);
    frame.render_widget(body, area);
}

fn render_footer(frame: &mut Frame, area: Rect, state: &SetupState) {
    let footer_text = match state.phase {
        SetupPhase::Confirming => Line::from(vec![
            Span::styled(" [y] ", theme::success()),
            Span::styled("Yes", theme::muted()),
            Span::raw("  "),
            Span::styled("[n] ", theme::error()),
            Span::styled("No", theme::muted()),
        ]),
        _ => Line::from(vec![
            Span::styled(" [q] ", theme::accent()),
            Span::styled("Quit", theme::muted()),
        ]),
    };

    let footer = Paragraph::new(footer_text);
    frame.render_widget(footer, area);
}

fn print_complete_message(tool: &str) {
    println!();
    println!("  \x1b[32;1m✔\x1b[0m {} is now configured to use Minna.", tool);
    println!();
    println!("  \x1b[33mRestart {} to activate.\x1b[0m", tool);
    println!();
}
