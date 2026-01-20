//! Welcome screen for `minna` command (no args)
//!
//! City Pop / Sunny Brutalist aesthetic welcome

use anyhow::Result;

use super::theme;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run the welcome screen (prints to stdout, no full-screen TUI)
pub async fn run() -> Result<()> {
    print_welcome();
    Ok(())
}

fn print_welcome() {
    let green = "\x1b[38;2;0;255;65m";   // Signal Green #00FF41
    let pink = "\x1b[38;2;255;113;206m"; // Sunset Pink #FF71CE
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    // ASCII art logo
    println!();
    println!(
        "{green}{bold}  ███╗   ███╗██╗███╗   ██╗███╗   ██╗ █████╗{reset}",
        green = green,
        bold = bold,
        reset = reset
    );
    println!(
        "{green}{bold}  ████╗ ████║██║████╗  ██║████╗  ██║██╔══██╗{reset}",
        green = green,
        bold = bold,
        reset = reset
    );
    println!(
        "{green}{bold}  ██╔████╔██║██║██╔██╗ ██║██╔██╗ ██║███████║{reset}",
        green = green,
        bold = bold,
        reset = reset
    );
    println!(
        "{green}{bold}  ██║╚██╔╝██║██║██║╚██╗██║██║╚██╗██║██╔══██║{reset}",
        green = green,
        bold = bold,
        reset = reset
    );
    println!(
        "{green}{bold}  ██║ ╚═╝ ██║██║██║ ╚████║██║ ╚████║██║  ██║{reset}",
        green = green,
        bold = bold,
        reset = reset
    );
    println!(
        "{green}{bold}  ╚═╝     ╚═╝╚═╝╚═╝  ╚═══╝╚═╝  ╚═══╝╚═╝  ╚═╝{reset}",
        green = green,
        bold = bold,
        reset = reset
    );
    println!();
    println!(
        "  {dim}Your AI's memory. Local-first. Zero config.{reset}",
        dim = dim,
        reset = reset
    );
    println!(
        "  {dim}v{version}{reset}",
        dim = dim,
        version = VERSION,
        reset = reset
    );
    println!();

    // Divider
    println!(
        "  {pink}{line}{reset}",
        pink = pink,
        line = theme::BOX_HORIZONTAL.repeat(44),
        reset = reset
    );
    println!();

    // Quick start
    println!("  {bold}Get started:{reset}", bold = bold, reset = reset);
    println!();
    println!(
        "    {pink}${reset} minna add slack      {dim}Connect Slack{reset}",
        pink = pink,
        dim = dim,
        reset = reset
    );
    println!(
        "    {pink}${reset} minna add linear     {dim}Connect Linear{reset}",
        pink = pink,
        dim = dim,
        reset = reset
    );
    println!(
        "    {pink}${reset} minna status         {dim}View dashboard{reset}",
        pink = pink,
        dim = dim,
        reset = reset
    );
    println!(
        "    {pink}${reset} minna setup cursor   {dim}Configure your AI{reset}",
        pink = pink,
        dim = dim,
        reset = reset
    );
    println!();

    // Divider
    println!(
        "  {pink}{line}{reset}",
        pink = pink,
        line = theme::BOX_HORIZONTAL.repeat(44),
        reset = reset
    );
    println!();

    // Status section - check if daemon is running
    let daemon_status = check_daemon_status();
    let sources_count = count_sources();

    println!("  {bold}Status:{reset}", bold = bold, reset = reset);
    println!();

    if daemon_status {
        println!(
            "    {green}●{reset} Daemon running",
            green = green,
            reset = reset
        );
    } else {
        println!(
            "    {dim}○{reset} Daemon stopped {dim}(starts automatically){reset}",
            dim = dim,
            reset = reset
        );
    }

    if sources_count > 0 {
        println!(
            "    {green}●{reset} {count} source{s} connected",
            green = green,
            count = sources_count,
            s = if sources_count == 1 { "" } else { "s" },
            reset = reset
        );
    } else {
        println!(
            "    {dim}○{reset} No sources connected",
            dim = dim,
            reset = reset
        );
    }

    println!();

    // Help hint
    println!(
        "  {dim}Run {reset}{pink}minna --help{reset}{dim} for all commands{reset}",
        pink = pink,
        dim = dim,
        reset = reset
    );
    println!();
}

fn check_daemon_status() -> bool {
    let pid_file = dirs::home_dir()
        .map(|h| h.join(".minna/daemon.pid"))
        .unwrap_or_default();

    if !pid_file.exists() {
        return false;
    }

    if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            // Check if process is running
            return std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
        }
    }

    false
}

fn count_sources() -> usize {
    // Check auth.json for configured sources
    let auth_path = directories::ProjectDirs::from("ai", "minna", "minna")
        .map(|d| d.data_dir().join("auth.json"))
        .or_else(|| {
            dirs::home_dir().map(|h| h.join(".local/share/minna/auth.json"))
        });

    if let Some(path) = auth_path {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(tokens) = json.get("tokens").and_then(|t| t.as_object()) {
                    return tokens.len();
                }
            }
        }
    }

    0
}
