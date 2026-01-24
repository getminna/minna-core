use console::{style, Term};
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input, Password};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Print success message
pub fn success(msg: &str) {
    println!("{} {}", style("✔").green(), msg);
}

/// Print error message
pub fn error(msg: &str) {
    println!("{} {}", style("✖").red(), msg);
}

/// Print info message (indented)
pub fn info(msg: &str) {
    println!("  {}", msg);
}

/// Print a header/title
pub fn header(msg: &str) {
    println!();
    println!("  {}", msg);
    println!();
}

/// Print numbered steps
pub fn steps(items: &[&str]) {
    for (i, item) in items.iter().enumerate() {
        println!("  {}. {}", i + 1, item);
    }
    println!();
}

/// Prompt for a password/token (masked input)
pub fn prompt_password(prompt: &str) -> anyhow::Result<String> {
    let value = Password::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .interact()?;
    Ok(value)
}

/// Prompt for regular text input
pub fn prompt_input(prompt: &str) -> anyhow::Result<String> {
    let value = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .interact_text()?;
    Ok(value)
}

/// Prompt for a selection from a list
pub fn prompt_select(prompt: &str, items: &[&str]) -> anyhow::Result<usize> {
    let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .default(0)
        .interact()?;
    Ok(selection)
}

/// Prompt for a boolean confirmation
pub fn prompt_confirm(prompt: &str) -> anyhow::Result<bool> {
    use dialoguer::Confirm;
    let result = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(true)
        .interact()?;
    Ok(result)
}

/// Create a progress bar
pub fn progress_bar(total: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("⚡ {msg}...  {bar:20.cyan/dim} {pos}/{len}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Create a spinner for indeterminate progress
pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Print the final "Ready" box
#[allow(dead_code)]
pub fn ready_box(clipboard_text: &str) {
    let _term = Term::stdout();
    let width = 56;
    let border: String = "─".repeat(width);

    println!();
    println!("{}", border);
    println!("  {} Ready.", style("✔").green());
    println!();
    println!("  Copied to clipboard:");
    println!();
    println!("    {}", clipboard_text);
    println!();
    println!("  Paste into chat (⌘V) and hit Enter.");
    println!("{}", border);
    println!();
}

/// Print background task notice
pub fn background_notice(msg: &str, hint: &str) {
    println!("{} {}", style("💤").dim(), msg);
    println!("   {}", style(hint).dim());
}
