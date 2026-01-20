//! TUI view for `minna add` command
//!
//! Two modes:
//! 1. Interactive picker - select sources with Sunset Pink highlight
//! 2. Sync progress - progress bar with rolling artifact count

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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::time::{Duration, Instant};

use super::theme;

/// Available sources for selection
const SOURCES: &[(&str, &str)] = &[
    ("slack", "Slack"),
    ("linear", "Linear"),
    ("github", "GitHub"),
    ("notion", "Notion"),
    ("google", "Google Workspace"),
    ("atlassian", "Atlassian (Jira/Confluence)"),
];

struct PickerState {
    selected: usize,
}

struct SyncState {
    source: String,
    progress: f64,
    artifacts: u64,
    phase: SyncPhase,
    start_time: Instant,
}

enum SyncPhase {
    Connecting,
    SprintSync,
    DeepSync,
    Complete,
}

/// Run the add TUI in test mode
pub async fn run_test(sources: Vec<String>) -> Result<()> {
    if sources.is_empty() {
        // Interactive picker mode
        run_picker().await
    } else {
        // Sync progress mode
        run_sync(&sources[0]).await
    }
}

async fn run_picker() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = PickerState { selected: 0 };

    loop {
        terminal.draw(|f| render_picker(f, &state))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Up | KeyCode::Char('k') => {
                            state.selected = state.selected.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            state.selected = (state.selected + 1).min(SOURCES.len() - 1);
                        }
                        KeyCode::Enter => {
                            // Transition to sync view
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                            return run_sync(SOURCES[state.selected].0).await;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn render_picker(frame: &mut Frame, state: &PickerState) {
    let area = frame.area();

    // Dark background
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

    // Header
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  ▓▓ ", theme::accent()),
            Span::styled("CONNECT A SOURCE", theme::title()),
        ]),
    ]);
    frame.render_widget(header, chunks[0]);

    // Source list
    let items: Vec<ListItem> = SOURCES
        .iter()
        .enumerate()
        .map(|(i, (_, display))| {
            let content = if i == state.selected {
                Line::from(vec![
                    Span::styled(" → ", theme::accent()),
                    Span::styled(*display, theme::highlight()),
                ])
            } else {
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(*display, Style::default().fg(theme::SIGNAL_GREEN)),
                ])
            };
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE));

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // Footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" [↑↓] ", theme::accent()),
        Span::styled("Navigate", theme::muted()),
        Span::raw("  "),
        Span::styled("[Enter] ", theme::accent()),
        Span::styled("Select", theme::muted()),
        Span::raw("  "),
        Span::styled("[q] ", theme::accent()),
        Span::styled("Cancel", theme::muted()),
    ]));
    frame.render_widget(footer, chunks[2]);
}

async fn run_sync(source: &str) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = SyncState {
        source: source.to_string(),
        progress: 0.0,
        artifacts: 0,
        phase: SyncPhase::Connecting,
        start_time: Instant::now(),
    };

    loop {
        // Update state based on elapsed time (simulated progress)
        let elapsed = state.start_time.elapsed().as_secs_f64();

        state.phase = if elapsed < 1.0 {
            SyncPhase::Connecting
        } else if elapsed < 4.0 {
            state.progress = (elapsed - 1.0) / 3.0;
            state.artifacts = (state.progress * 142.0) as u64;
            SyncPhase::SprintSync
        } else if elapsed < 6.0 {
            state.progress = 1.0;
            state.artifacts = 142;
            SyncPhase::DeepSync
        } else {
            SyncPhase::Complete
        };

        terminal.draw(|f| render_sync(f, &state))?;

        if matches!(state.phase, SyncPhase::Complete) {
            // Wait a moment, then show ready box
            tokio::time::sleep(Duration::from_secs(1)).await;
            break;
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    // Print the "Ready" box to stdout (non-TUI)
    print_ready_box(&state.source);

    Ok(())
}

fn render_sync(frame: &mut Frame, state: &SyncState) {
    let area = frame.area();

    let block = Block::default().style(Style::default().bg(theme::DARK_GRAPHITE));
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(5),
        ])
        .split(area);

    // Header
    let source_display = SOURCES
        .iter()
        .find(|(id, _)| *id == state.source)
        .map(|(_, name)| *name)
        .unwrap_or(&state.source);

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  ▓▓ ", theme::accent()),
            Span::styled(format!("CONNECTING {}", source_display.to_uppercase()), theme::title()),
        ]),
    ]);
    frame.render_widget(header, chunks[0]);

    // Progress section
    let progress_area = chunks[1];

    match state.phase {
        SyncPhase::Connecting => {
            let connecting = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ◐ ", theme::accent()),
                    Span::styled("Opening browser...", theme::success()),
                ]),
            ]);
            frame.render_widget(connecting, progress_area);
        }
        SyncPhase::SprintSync => {
            let bar = theme::progress_bar(state.progress, 30);
            let progress = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ⚡ Sprint Sync...  ", Style::default()),
                    Span::styled(bar, theme::success()),
                    Span::styled(format!("  {} artifacts", state.artifacts), theme::accent()),
                ]),
            ]);
            frame.render_widget(progress, progress_area);
        }
        SyncPhase::DeepSync | SyncPhase::Complete => {
            let bar = theme::progress_bar(1.0, 30);
            let progress = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ✔ ", theme::success()),
                    Span::styled(format!("{} connected.", source_display), theme::success()),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ⚡ Sprint Sync...  ", Style::default()),
                    Span::styled(bar, theme::success()),
                    Span::styled("  142 artifacts", theme::accent()),
                ]),
            ]);
            frame.render_widget(progress, progress_area);
        }
    }

    // Deep sync notice
    if matches!(state.phase, SyncPhase::DeepSync | SyncPhase::Complete) {
        let notice = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ↗ ", theme::muted()),
                Span::styled("Deep sync running in background (90 days of history).", theme::muted()),
            ]),
            Line::from(vec![
                Span::styled("     Run ", theme::muted()),
                Span::styled("`minna status`", theme::accent()),
                Span::styled(" to check progress.", theme::muted()),
            ]),
        ]);
        frame.render_widget(notice, chunks[2]);
    }
}

fn print_ready_box(source: &str) {
    let source_display = SOURCES
        .iter()
        .find(|(id, _)| *id == source)
        .map(|(_, name)| *name)
        .unwrap_or(source);

    let width = 50;
    let h_line = theme::DOUBLE_HORIZONTAL.repeat(width);

    println!();
    println!(
        "  {}{}{}",
        theme::DOUBLE_TOP_LEFT, h_line, theme::DOUBLE_TOP_RIGHT
    );
    println!(
        "  {}  \x1b[32;1m✔ Ready.\x1b[0m{}",
        theme::DOUBLE_VERTICAL,
        " ".repeat(width - 10).to_string() + theme::DOUBLE_VERTICAL
    );
    println!(
        "  {}{}{}",
        theme::DOUBLE_VERTICAL,
        " ".repeat(width),
        theme::DOUBLE_VERTICAL
    );
    println!(
        "  {}  {} is now connected to Minna.{}{}",
        theme::DOUBLE_VERTICAL,
        source_display,
        " ".repeat(width - source_display.len() - 28),
        theme::DOUBLE_VERTICAL
    );
    println!(
        "  {}{}{}",
        theme::DOUBLE_VERTICAL,
        " ".repeat(width),
        theme::DOUBLE_VERTICAL
    );
    println!(
        "  {}  Next: Open your AI tool and ask about your {}{}{}",
        theme::DOUBLE_VERTICAL,
        source_display,
        " ".repeat(width.saturating_sub(source_display.len() + 42)),
        theme::DOUBLE_VERTICAL
    );
    println!(
        "  {}  data to verify the connection.{}{}",
        theme::DOUBLE_VERTICAL,
        " ".repeat(width - 34),
        theme::DOUBLE_VERTICAL
    );
    println!(
        "  {}{}{}",
        theme::DOUBLE_BOTTOM_LEFT, h_line, theme::DOUBLE_BOTTOM_RIGHT
    );
    println!();
}
