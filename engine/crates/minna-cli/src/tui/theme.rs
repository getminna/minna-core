//! City Pop / Sunny Brutalist theme for Minna TUI

use ratatui::style::{Color, Modifier, Style};

/// Signal Green - Primary color for success, active states
pub const SIGNAL_GREEN: Color = Color::Rgb(0x00, 0xFF, 0x41);

/// Sunset Pink - Accent color for highlights, selections
pub const SUNSET_PINK: Color = Color::Rgb(0xFF, 0x71, 0xCE);

/// Dark Graphite - Background color
pub const DARK_GRAPHITE: Color = Color::Rgb(0x1A, 0x1B, 0x26);

/// Muted text color
pub const MUTED: Color = Color::Rgb(0x6B, 0x6B, 0x6B);

/// Warning/syncing color
pub const AMBER: Color = Color::Rgb(0xFF, 0xB8, 0x6C);

/// Error color
pub const ERROR_RED: Color = Color::Rgb(0xFF, 0x55, 0x55);

// ─────────────────────────────────────────────────────────────
// Style helpers
// ─────────────────────────────────────────────────────────────

pub fn title() -> Style {
    Style::default()
        .fg(SIGNAL_GREEN)
        .add_modifier(Modifier::BOLD)
}

pub fn highlight() -> Style {
    Style::default()
        .bg(SUNSET_PINK)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD)
}

pub fn success() -> Style {
    Style::default().fg(SIGNAL_GREEN)
}

pub fn warning() -> Style {
    Style::default().fg(AMBER)
}

pub fn error() -> Style {
    Style::default().fg(ERROR_RED)
}

pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

pub fn accent() -> Style {
    Style::default().fg(SUNSET_PINK)
}

// ─────────────────────────────────────────────────────────────
// Box drawing characters (thick brutalist style)
// ─────────────────────────────────────────────────────────────

pub const BOX_TOP_LEFT: &str = "┏";
pub const BOX_TOP_RIGHT: &str = "┓";
pub const BOX_BOTTOM_LEFT: &str = "┗";
pub const BOX_BOTTOM_RIGHT: &str = "┛";
pub const BOX_HORIZONTAL: &str = "━";
pub const BOX_VERTICAL: &str = "┃";

/// Double-line box for "Ready" state
pub const DOUBLE_TOP_LEFT: &str = "╔";
pub const DOUBLE_TOP_RIGHT: &str = "╗";
pub const DOUBLE_BOTTOM_LEFT: &str = "╚";
pub const DOUBLE_BOTTOM_RIGHT: &str = "╝";
pub const DOUBLE_HORIZONTAL: &str = "═";
pub const DOUBLE_VERTICAL: &str = "║";

// ─────────────────────────────────────────────────────────────
// Progress bar characters
// ─────────────────────────────────────────────────────────────

pub const PROGRESS_FULL: &str = "█";
pub const PROGRESS_EMPTY: &str = "░";
pub const PROGRESS_PARTIAL: &str = "▓";

/// Build a progress bar string
pub fn progress_bar(progress: f64, width: usize) -> String {
    let filled = (progress * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!(
        "{}{}",
        PROGRESS_FULL.repeat(filled),
        PROGRESS_EMPTY.repeat(empty)
    )
}

// ─────────────────────────────────────────────────────────────
// ASCII art header
// ─────────────────────────────────────────────────────────────

pub const MINNA_HEADER: &str = r#"
  ███╗   ███╗██╗███╗   ██╗███╗   ██╗ █████╗
  ████╗ ████║██║████╗  ██║████╗  ██║██╔══██╗
  ██╔████╔██║██║██╔██╗ ██║██╔██╗ ██║███████║
  ██║╚██╔╝██║██║██║╚██╗██║██║╚██╗██║██╔══██║
  ██║ ╚═╝ ██║██║██║ ╚████║██║ ╚████║██║  ██║
  ╚═╝     ╚═╝╚═╝╚═╝  ╚═══╝╚═╝  ╚═══╝╚═╝  ╚═╝
"#;

pub const MINNA_SMALL: &str = "▓▓ MINNA";
