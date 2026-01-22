//! `minna link` command - Review and link user identities across sources.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Result;
use minna_graph::{GraphStore, IdentityService, MatchType};
use sqlx::sqlite::SqlitePoolOptions;

/// Run the link command - review and confirm identity matches.
pub async fn run() -> Result<()> {
    let db_path = get_db_path()?;

    if !db_path.exists() {
        println!("No Minna database found. Run 'minna sync' first to populate data.");
        return Ok(());
    }

    // Connect to database
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}", db_path.display()))
        .await?;

    let graph = GraphStore::new(pool);

    // First, run auto-linking for exact email matches
    println!("Checking for exact email matches...");
    let auto_linked = IdentityService::auto_link_by_email(&graph).await?;
    if auto_linked > 0 {
        println!("  Auto-linked {} users with matching emails.", auto_linked);
    } else {
        println!("  No new exact email matches found.");
    }

    // Find fuzzy matches for review
    println!("\nLooking for potential identity matches...");
    let matches = IdentityService::find_fuzzy_matches(&graph).await?;

    if matches.is_empty() {
        println!("  No additional matches found for review.");
        println!("\nAll identities are linked!");
        return Ok(());
    }

    println!("\nFound {} potential matches for review:\n", matches.len());

    for (i, m) in matches.iter().enumerate() {
        let confidence_pct = (m.confidence * 100.0) as u32;
        let match_type = match m.match_type {
            MatchType::ExactEmail => "exact email",
            MatchType::SimilarName => "similar name",
            MatchType::Manual => "manual",
        };

        println!("{}. {} ({}% confidence, {})", i + 1,
            m.users.iter()
                .map(|u| format!("{}:{}", u.provider, u.display_name.as_deref().unwrap_or(&u.provider_user_id)))
                .collect::<Vec<_>>()
                .join(" <-> "),
            confidence_pct,
            match_type
        );

        for user in &m.users {
            let email = user.email.as_deref().unwrap_or("(no email)");
            println!("   - {}: {} <{}>", user.provider,
                user.display_name.as_deref().unwrap_or("(no name)"),
                email);
        }
        println!();
    }

    // Interactive confirmation
    print!("Link these accounts? [y/N/number to skip specific]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input == "y" || input == "yes" {
        // Link all matches
        for m in &matches {
            if m.users.len() >= 2 {
                let u1 = &m.users[0];
                let u2 = &m.users[1];
                IdentityService::link_users(
                    &graph,
                    &u1.provider,
                    &u1.provider_user_id,
                    &u2.provider,
                    &u2.provider_user_id,
                ).await?;
            }
        }
        println!("\nLinked {} identity matches.", matches.len());
    } else if input.is_empty() || input == "n" || input == "no" {
        println!("\nNo changes made.");
    } else {
        println!("\nSkipped. Run 'minna link' again to review.");
    }

    Ok(())
}

/// Get the path to the Minna database.
fn get_db_path() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("MINNA_DATA_DIR") {
        return Ok(PathBuf::from(dir).join("minna.db"));
    }

    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Minna")
            .join("minna.db"));
    }

    Ok(PathBuf::from(".minna").join("minna.db"))
}
