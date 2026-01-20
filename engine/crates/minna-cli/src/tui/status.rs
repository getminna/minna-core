//! TUI view for `minna status` command
//!
//! Renders the "Base Station" dashboard with:
//! - Header: MINNA STATUS
//! - Body: Connected sources, sync progress, daemon status
//! - Footer: Keybindings

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame, Terminal,
};
use std::io;

use super::theme;

/// Mock data for UI testing
struct MockState {
    daemon_running: bool,
    daemon_version: String,
    sources: Vec<SourceState>,
    documents: u64,
    vectors: u64,
    db_size_mb: f64,
}

struct SourceState {
    name: String,
    status: SourceStatus,
    docs: u64,
    last_sync: String,
}

#[derive(Clone, Copy)]
enum SourceStatus {
    Ready,
    Syncing,
    Error,
    NotConfigured,
}

impl MockState {
    fn demo() -> Self {
        Self {
            daemon_running: true,
            daemon_version: "0.1.0".to_string(),
            sources: vec![
                SourceState {
                    name: "slack".to_string(),
                    status: SourceStatus::Ready,
                    docs: 1247,
                    last_sync: "2 min ago".to_string(),
                },
                SourceState {
                    name: "linear".to_string(),
                    status: SourceStatus::Syncing,
                    docs: 89,
                    last_sync: "syncing...".to_string(),
                },
                SourceState {
                    name: "github".to_string(),
                    status: SourceStatus::Ready,
                    docs: 342,
                    last_sync: "15 min ago".to_string(),
                },
                SourceState {
                    name: "notion".to_string(),
                    status: SourceStatus::NotConfigured,
                    docs: 0,
                    last_sync: "-".to_string(),
                },
            ],
            documents: 1678,
            vectors: 8420,
            db_size_mb: 24.7,
        }
    }
}

/// Run the status TUI in test mode with mock data
pub async fn run_test() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let state = MockState::demo();

    // Main loop
    loop {
        terminal.draw(|f| render(f, &state))?;

        // Handle input
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('r') => {
                            // Would restart daemon
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}

fn render(frame: &mut Frame, state: &MockState) {
    let area = frame.area();

    // Clear with dark background
    let block = Block::default().style(Style::default().bg(theme::DARK_GRAPHITE));
    frame.render_widget(block, area);

    // Layout: Header, Body, Footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(4),  // Header
            Constraint::Min(10),    // Body
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    render_header(frame, chunks[0]);
    render_body(frame, chunks[1], state);
    render_footer(frame, chunks[2]);
}

fn render_header(frame: &mut Frame, area: Rect) {
    let header_text = vec![
        Line::from(vec![
            Span::styled("  ▓▓ ", theme::accent()),
            Span::styled("MINNA STATUS", theme::title()),
        ]),
        Line::from(Span::styled(
            format!("  {}", theme::BOX_HORIZONTAL.repeat(40)),
            theme::accent(),
        )),
    ];

    let header = Paragraph::new(header_text);
    frame.render_widget(header, area);
}

fn render_body(frame: &mut Frame, area: Rect, state: &MockState) {
    // Split into two columns
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    render_sources(frame, columns[0], state);
    render_stats(frame, columns[1], state);
}

fn render_sources(frame: &mut Frame, area: Rect, state: &MockState) {
    let mut rows = vec![];

    for source in &state.sources {
        let (status_icon, status_style) = match source.status {
            SourceStatus::Ready => ("✔", theme::success()),
            SourceStatus::Syncing => ("⚡", theme::warning()),
            SourceStatus::Error => ("✖", theme::error()),
            SourceStatus::NotConfigured => ("○", theme::muted()),
        };

        let docs_str = if source.docs > 0 {
            format!("{:>6} docs", source.docs)
        } else {
            "         ".to_string()
        };

        rows.push(Row::new(vec![
            Span::styled(format!(" {} ", status_icon), status_style),
            Span::styled(format!("{:<12}", source.name), Style::default().fg(theme::SIGNAL_GREEN)),
            Span::styled(docs_str, theme::muted()),
            Span::styled(format!("{}", source.last_sync), theme::muted()),
        ]));
    }

    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Min(10),
        ],
    )
    .block(
        Block::default()
            .title(Span::styled(" SOURCES ", Style::default().add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_style(theme::muted()),
    );

    frame.render_widget(table, area);
}

fn render_stats(frame: &mut Frame, area: Rect, state: &MockState) {
    let daemon_status = if state.daemon_running {
        Span::styled("● running", theme::success())
    } else {
        Span::styled("○ stopped", theme::error())
    };

    let stats_text = vec![
        Line::from(vec![
            Span::styled(" daemon    ", theme::muted()),
            daemon_status,
        ]),
        Line::from(vec![
            Span::styled(" version   ", theme::muted()),
            Span::raw(format!("v{}", state.daemon_version)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" documents ", theme::muted()),
            Span::styled(format!("{}", state.documents), theme::success()),
        ]),
        Line::from(vec![
            Span::styled(" vectors   ", theme::muted()),
            Span::styled(format!("{}", state.vectors), theme::success()),
        ]),
        Line::from(vec![
            Span::styled(" db size   ", theme::muted()),
            Span::raw(format!("{:.1} MB", state.db_size_mb)),
        ]),
    ];

    let stats = Paragraph::new(stats_text).block(
        Block::default()
            .title(Span::styled(" STATS ", Style::default().add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_style(theme::muted()),
    );

    frame.render_widget(stats, area);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let footer_text = Line::from(vec![
        Span::styled(" [q] ", theme::accent()),
        Span::styled("Quit", theme::muted()),
        Span::raw("  "),
        Span::styled("[r] ", theme::accent()),
        Span::styled("Restart daemon", theme::muted()),
        Span::raw("  "),
        Span::styled("[l] ", theme::accent()),
        Span::styled("View logs", theme::muted()),
    ]);

    let footer = Paragraph::new(footer_text)
        .style(Style::default().bg(theme::DARK_GRAPHITE));
    frame.render_widget(footer, area);
}
