//! Local Git repository extractor for Gravity Well.
//!
//! Scans git history to extract:
//! - User → File (EditedFile) edges
//! - User → Project/Repo (CommittedTo) edges
//! - Commit → File relationships
//!
//! Uses a 90-day cutoff for commit history to match ghost edge threshold.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, TimeZone, Utc};
use git2::{DiffOptions, Repository, Sort};
use tracing::{debug, info, warn};

use crate::schema::{ExtractedEdge, NodeRef, NodeType, Relation};

/// Configuration for local git extraction.
#[derive(Debug, Clone)]
pub struct LocalGitConfig {
    /// Maximum days of history to scan (default: 90)
    pub history_days: i64,
    /// Minimum commits to a file to create edge (default: 1)
    pub min_commits: usize,
    /// Ignore files matching these patterns
    pub ignore_patterns: Vec<String>,
    /// Maximum files to process per run (default: 10000)
    pub max_files: usize,
}

impl Default for LocalGitConfig {
    fn default() -> Self {
        Self {
            history_days: 90,
            min_commits: 1,
            ignore_patterns: vec![
                "*.lock".to_string(),
                "package-lock.json".to_string(),
                "yarn.lock".to_string(),
                "Cargo.lock".to_string(),
                "*.min.js".to_string(),
                "*.min.css".to_string(),
            ],
            max_files: 10000,
        }
    }
}

/// Result of local git extraction.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// Number of commits scanned
    pub commits_scanned: usize,
    /// Number of files processed
    pub files_processed: usize,
    /// Number of edges extracted
    pub edges_extracted: usize,
    /// Unique authors found
    pub unique_authors: usize,
    /// Time taken in milliseconds
    pub duration_ms: u64,
}

/// Extractor for local git repositories.
pub struct LocalGitExtractor {
    config: LocalGitConfig,
}

/// File edit statistics for an author.
#[derive(Debug, Default)]
struct AuthorFileStats {
    commits: usize,
    last_edit: Option<DateTime<Utc>>,
    // Reserved for future use: line-level statistics
    #[allow(dead_code)]
    lines_added: usize,
    #[allow(dead_code)]
    lines_removed: usize,
}

impl LocalGitExtractor {
    /// Create a new LocalGitExtractor with default configuration.
    pub fn new() -> Self {
        Self {
            config: LocalGitConfig::default(),
        }
    }

    /// Create a new LocalGitExtractor with custom configuration.
    pub fn with_config(config: LocalGitConfig) -> Self {
        Self { config }
    }

    /// Extract edges from a git repository.
    ///
    /// Scans commit history within the configured window and extracts
    /// author → file relationships.
    pub fn extract(&self, repo_path: &Path) -> Result<(Vec<ExtractedEdge>, ExtractionResult)> {
        let start_time = std::time::Instant::now();

        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open git repository at {:?}", repo_path))?;

        let repo_name = self.get_repo_name(&repo, repo_path);
        info!("Scanning git history for: {}", repo_name);

        let cutoff = Utc::now() - Duration::days(self.config.history_days);
        let cutoff_ts = cutoff.timestamp();

        // Walk commits
        let mut revwalk = repo.revwalk()?;
        revwalk.set_sorting(Sort::TIME)?;
        revwalk.push_head()?;

        // Map: (author_email, file_path) → stats
        let mut author_files: HashMap<(String, String), AuthorFileStats> = HashMap::new();
        let mut commits_scanned = 0;
        let mut unique_authors: HashSet<String> = HashSet::new();

        for oid in revwalk {
            let oid = oid?;
            let commit = repo.find_commit(oid)?;

            // Stop at cutoff
            let commit_time = commit.time().seconds();
            if commit_time < cutoff_ts {
                break;
            }

            commits_scanned += 1;

            // Get author info
            let author = commit.author();
            let author_email = author.email().unwrap_or("unknown").to_lowercase();
            unique_authors.insert(author_email.clone());

            let commit_dt = Utc.timestamp_opt(commit_time, 0).single().unwrap_or_else(Utc::now);

            // Get diff
            let parent = commit.parent(0).ok();
            let parent_tree = parent.as_ref().and_then(|p| p.tree().ok());
            let commit_tree = commit.tree().ok();

            let mut diff_opts = DiffOptions::new();
            diff_opts.ignore_whitespace(true);

            if let Ok(diff) = repo.diff_tree_to_tree(
                parent_tree.as_ref(),
                commit_tree.as_ref(),
                Some(&mut diff_opts),
            ) {
                // Process each file in the diff
                for delta in diff.deltas() {
                    let file_path = delta
                        .new_file()
                        .path()
                        .or_else(|| delta.old_file().path())
                        .and_then(|p| p.to_str())
                        .map(|s| s.to_string());

                    if let Some(path) = file_path {
                        // Skip ignored patterns
                        if self.should_ignore(&path) {
                            continue;
                        }

                        let key = (author_email.clone(), path);
                        let stats = author_files.entry(key).or_default();
                        stats.commits += 1;
                        if stats.last_edit.is_none() || Some(commit_dt) > stats.last_edit {
                            stats.last_edit = Some(commit_dt);
                        }
                    }
                }
            }

            // Log progress periodically
            if commits_scanned % 100 == 0 {
                debug!("Scanned {} commits, {} file edits", commits_scanned, author_files.len());
            }
        }

        info!(
            "Scanned {} commits, found {} author-file relationships",
            commits_scanned,
            author_files.len()
        );

        // Build edges
        let mut edges = Vec::new();
        let mut files_processed: HashSet<String> = HashSet::new();

        for ((author_email, file_path), stats) in &author_files {
            if stats.commits < self.config.min_commits {
                continue;
            }

            if files_processed.len() >= self.config.max_files {
                warn!("Reached max files limit ({}), stopping", self.config.max_files);
                break;
            }

            files_processed.insert(file_path.clone());

            let observed_at = stats.last_edit.unwrap_or_else(Utc::now);

            // User → File (EditedFile)
            let user_node = NodeRef::with_name(
                NodeType::User,
                "local-git",
                author_email,
                author_email, // Use email as display name for now
            );
            let file_node = NodeRef::with_name(
                NodeType::File,
                "local-git",
                format!("{}:{}", repo_name, file_path),
                file_path,
            );

            edges.push(ExtractedEdge::new(
                user_node.clone(),
                file_node.clone(),
                Relation::EditedFile,
                observed_at,
            ));

            // User → Repo (CommittedTo) - deduplicated below
        }

        // Add User → Repo edges (one per author)
        let repo_node = NodeRef::with_name(
            NodeType::Project,
            "local-git",
            &repo_name,
            &repo_name,
        );

        for author_email in &unique_authors {
            let user_node = NodeRef::with_name(
                NodeType::User,
                "local-git",
                author_email,
                author_email,
            );

            // Find most recent commit for this author
            let last_commit = author_files
                .iter()
                .filter(|((email, _), _)| email == author_email)
                .filter_map(|(_, stats)| stats.last_edit)
                .max()
                .unwrap_or_else(Utc::now);

            edges.push(ExtractedEdge::new(
                user_node,
                repo_node.clone(),
                Relation::CommittedTo,
                last_commit,
            ));
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

        let result = ExtractionResult {
            commits_scanned,
            files_processed: files_processed.len(),
            edges_extracted: edges.len(),
            unique_authors: unique_authors.len(),
            duration_ms,
        };

        info!(
            "Extraction complete: {} commits, {} files, {} edges, {} authors in {}ms",
            result.commits_scanned,
            result.files_processed,
            result.edges_extracted,
            result.unique_authors,
            result.duration_ms
        );

        Ok((edges, result))
    }

    /// Get collaborators for a specific file.
    ///
    /// Returns a list of (author_email, commit_count, last_edit) for the file.
    pub fn get_file_collaborators(
        &self,
        repo_path: &Path,
        file_path: &str,
    ) -> Result<Vec<(String, usize, DateTime<Utc>)>> {
        let repo = Repository::open(repo_path)?;
        let cutoff = Utc::now() - Duration::days(self.config.history_days);
        let cutoff_ts = cutoff.timestamp();

        let mut revwalk = repo.revwalk()?;
        revwalk.set_sorting(Sort::TIME)?;
        revwalk.push_head()?;

        let mut author_stats: HashMap<String, (usize, DateTime<Utc>)> = HashMap::new();

        for oid in revwalk {
            let oid = oid?;
            let commit = repo.find_commit(oid)?;

            let commit_time = commit.time().seconds();
            if commit_time < cutoff_ts {
                break;
            }

            let commit_dt = Utc.timestamp_opt(commit_time, 0).single().unwrap_or_else(Utc::now);

            // Check if this commit touched the file
            let parent = commit.parent(0).ok();
            let parent_tree = parent.as_ref().and_then(|p| p.tree().ok());
            let commit_tree = commit.tree().ok();

            if let Ok(diff) = repo.diff_tree_to_tree(parent_tree.as_ref(), commit_tree.as_ref(), None) {
                let touched_file = diff.deltas().any(|delta| {
                    delta
                        .new_file()
                        .path()
                        .or_else(|| delta.old_file().path())
                        .and_then(|p| p.to_str())
                        .map(|p| p == file_path)
                        .unwrap_or(false)
                });

                if touched_file {
                    let author_email = commit
                        .author()
                        .email()
                        .unwrap_or("unknown")
                        .to_lowercase();

                    let entry = author_stats
                        .entry(author_email)
                        .or_insert((0, commit_dt));
                    entry.0 += 1;
                    if commit_dt > entry.1 {
                        entry.1 = commit_dt;
                    }
                }
            }
        }

        let mut result: Vec<_> = author_stats
            .into_iter()
            .map(|(email, (count, last))| (email, count, last))
            .collect();

        // Sort by commit count descending
        result.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(result)
    }

    /// Get the repository name from its origin remote or path.
    fn get_repo_name(&self, repo: &Repository, path: &Path) -> String {
        // Try to get from origin remote
        if let Ok(remote) = repo.find_remote("origin") {
            if let Some(url) = remote.url() {
                // Parse repo name from git URL
                if let Some(name) = self.parse_repo_name_from_url(url) {
                    return name;
                }
            }
        }

        // Fallback to directory name
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Parse repository name from git URL.
    fn parse_repo_name_from_url(&self, url: &str) -> Option<String> {
        // Handle SSH URLs: git@github.com:org/repo.git
        if url.contains('@') && url.contains(':') {
            let parts: Vec<&str> = url.split(':').collect();
            if parts.len() == 2 {
                return parts[1]
                    .strip_suffix(".git")
                    .map(|s| s.to_string())
                    .or_else(|| Some(parts[1].to_string()));
            }
        }

        // Handle HTTPS URLs: https://github.com/org/repo.git
        if let Some(path) = url.strip_prefix("https://") {
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() >= 3 {
                let name = parts[1..].join("/");
                return Some(name.strip_suffix(".git").unwrap_or(&name).to_string());
            }
        }

        None
    }

    /// Check if a file path should be ignored.
    fn should_ignore(&self, path: &str) -> bool {
        for pattern in &self.config.ignore_patterns {
            if pattern.starts_with('*') {
                // Suffix match
                let suffix = pattern.trim_start_matches('*');
                if path.ends_with(suffix) {
                    return true;
                }
            } else if path == pattern || path.ends_with(&format!("/{}", pattern)) {
                return true;
            }
        }
        false
    }
}

impl Default for LocalGitExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ssh_url() {
        let extractor = LocalGitExtractor::new();
        assert_eq!(
            extractor.parse_repo_name_from_url("git@github.com:getminna/minna-core.git"),
            Some("getminna/minna-core".to_string())
        );
    }

    #[test]
    fn test_parse_https_url() {
        let extractor = LocalGitExtractor::new();
        // HTTPS URLs extract org/repo from the path
        assert_eq!(
            extractor.parse_repo_name_from_url("https://github.com/getminna/minna-core.git"),
            Some("getminna/minna-core".to_string())
        );
    }

    #[test]
    fn test_should_ignore() {
        let extractor = LocalGitExtractor::new();
        assert!(extractor.should_ignore("package-lock.json"));
        assert!(extractor.should_ignore("yarn.lock"));
        assert!(extractor.should_ignore("Cargo.lock"));
        assert!(extractor.should_ignore("bundle.min.js"));
        assert!(!extractor.should_ignore("src/main.rs"));
        assert!(!extractor.should_ignore("package.json"));
    }

    #[test]
    fn test_config_defaults() {
        let config = LocalGitConfig::default();
        assert_eq!(config.history_days, 90);
        assert_eq!(config.min_commits, 1);
        assert_eq!(config.max_files, 10000);
    }
}
