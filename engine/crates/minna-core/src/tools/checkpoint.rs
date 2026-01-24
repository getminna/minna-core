use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// A checkpoint captures the state of a Claude Code session for lossless restoration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Human-readable title for this checkpoint
    pub title: String,
    /// Brief summary of what was being worked on
    pub summary: String,
    /// The task that was in progress
    pub current_task: String,
    /// Next steps to continue the work
    pub next_steps: String,
    /// List of active file paths
    pub files: Vec<String>,
    /// What triggered this checkpoint (e.g., "auto-compact", "auto-close", "manual")
    pub trigger: String,
    /// Version number (auto-incremented per title slug)
    #[serde(default)]
    pub version: u32,
    /// Timestamp when checkpoint was created
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
}

impl Checkpoint {
    /// Create a new checkpoint with the given parameters.
    pub fn new(
        title: impl Into<String>,
        summary: impl Into<String>,
        current_task: impl Into<String>,
        next_steps: impl Into<String>,
        files: Vec<String>,
        trigger: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            summary: summary.into(),
            current_task: current_task.into(),
            next_steps: next_steps.into(),
            files,
            trigger: trigger.into(),
            version: 0,
            created_at: Utc::now(),
        }
    }

    /// Generate the slug for this checkpoint's title.
    pub fn slug(&self) -> String {
        slug::slugify(&self.title)
    }

    /// Serialize this checkpoint to markdown format.
    pub fn to_markdown(&self) -> String {
        let files_list = if self.files.is_empty() {
            "- (none)".to_string()
        } else {
            self.files
                .iter()
                .map(|f| format!("- {}", f))
                .collect::<Vec<_>>()
                .join("\n")
        };

        format!(
            r#"---
title: {}
version: {}
created: {}
trigger: {}
---

## Summary
{}

## Current Task
{}

## Next Steps
{}

## Active Files
{}
"#,
            self.title,
            self.version,
            self.created_at.to_rfc3339(),
            self.trigger,
            self.summary,
            self.current_task,
            self.next_steps,
            files_list
        )
    }

    /// Parse a checkpoint from markdown format.
    pub fn from_markdown(content: &str) -> Result<Self> {
        // Parse frontmatter
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(anyhow!("invalid checkpoint format: missing frontmatter"));
        }

        let frontmatter = parts[1].trim();
        let body = parts[2].trim();

        // Parse frontmatter fields
        let mut title = String::new();
        let mut version = 0u32;
        let mut created_at = Utc::now();
        let mut trigger = String::new();

        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("title:") {
                title = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("version:") {
                version = value.trim().parse().unwrap_or(0);
            } else if let Some(value) = line.strip_prefix("created:") {
                created_at = DateTime::parse_from_rfc3339(value.trim())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
            } else if let Some(value) = line.strip_prefix("trigger:") {
                trigger = value.trim().to_string();
            }
        }

        // Parse body sections
        let mut summary = String::new();
        let mut current_task = String::new();
        let mut next_steps = String::new();
        let mut files = Vec::new();

        let mut current_section = "";
        for line in body.lines() {
            let line_trimmed = line.trim();
            if line_trimmed.starts_with("## ") {
                current_section = line_trimmed.strip_prefix("## ").unwrap_or("");
                continue;
            }

            match current_section {
                "Summary" => {
                    if !summary.is_empty() {
                        summary.push('\n');
                    }
                    summary.push_str(line);
                }
                "Current Task" => {
                    if !current_task.is_empty() {
                        current_task.push('\n');
                    }
                    current_task.push_str(line);
                }
                "Next Steps" => {
                    if !next_steps.is_empty() {
                        next_steps.push('\n');
                    }
                    next_steps.push_str(line);
                }
                "Active Files" => {
                    if let Some(file) = line_trimmed.strip_prefix("- ") {
                        if file != "(none)" {
                            files.push(file.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(Self {
            title,
            summary: summary.trim().to_string(),
            current_task: current_task.trim().to_string(),
            next_steps: next_steps.trim().to_string(),
            files,
            trigger,
            version,
            created_at,
        })
    }
}

/// Query options for loading checkpoints.
#[derive(Debug, Clone, Default)]
pub struct LoadQuery {
    /// Filter by title (optional)
    pub title: Option<String>,
    /// Load specific version (optional, defaults to latest)
    pub version: Option<u32>,
}

impl LoadQuery {
    pub fn latest() -> Self {
        Self::default()
    }

    pub fn by_title(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            version: None,
        }
    }

    pub fn exact(title: impl Into<String>, version: u32) -> Self {
        Self {
            title: Some(title.into()),
            version: Some(version),
        }
    }
}

/// Manages checkpoint storage and retrieval.
pub struct CheckpointStore {
    /// Base directory for checkpoint storage (e.g., ~/.minna/vault/checkpoints/)
    base_dir: PathBuf,
}

impl CheckpointStore {
    /// Create a new CheckpointStore with the given base directory.
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Create a CheckpointStore using the default path (~/.minna/vault/checkpoints/).
    pub fn default_path() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let base_dir = PathBuf::from(home)
            .join(".minna")
            .join("vault")
            .join("checkpoints");
        Self::new(base_dir)
    }

    /// Ensure the checkpoint directory exists.
    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.base_dir)
            .with_context(|| format!("failed to create checkpoint directory: {:?}", self.base_dir))
    }

    /// Get the next version number for a given slug.
    fn next_version(&self, slug: &str) -> Result<u32> {
        let pattern = format!("{}_v", slug);
        let mut max_version = 0u32;

        if !self.base_dir.exists() {
            return Ok(1);
        }

        let entries = fs::read_dir(&self.base_dir)
            .with_context(|| format!("failed to read checkpoint directory: {:?}", self.base_dir))?;

        for entry in entries.flatten() {
            let filename = entry.file_name();
            let name = filename.to_string_lossy();

            if name.starts_with(&pattern) && name.ends_with(".md") {
                // Extract version number: "slug_v3.md" -> 3
                if let Some(version_str) = name
                    .strip_prefix(&pattern)
                    .and_then(|s| s.strip_suffix(".md"))
                {
                    if let Ok(v) = version_str.parse::<u32>() {
                        max_version = max_version.max(v);
                    }
                }
            }
        }

        Ok(max_version + 1)
    }

    /// Save a checkpoint to disk.
    ///
    /// Returns the path where the checkpoint was saved.
    pub fn save(&self, mut checkpoint: Checkpoint) -> Result<PathBuf> {
        self.ensure_dir()?;

        let slug = checkpoint.slug();
        let version = self.next_version(&slug)?;
        checkpoint.version = version;

        let filename = format!("{}_v{}.md", slug, version);
        let path = self.base_dir.join(&filename);

        let content = checkpoint.to_markdown();
        fs::write(&path, &content)
            .with_context(|| format!("failed to write checkpoint: {:?}", path))?;

        debug!("Saved checkpoint: {:?}", path);
        Ok(path)
    }

    /// Load a checkpoint based on the query.
    pub fn load(&self, query: LoadQuery) -> Result<Option<Checkpoint>> {
        if !self.base_dir.exists() {
            return Ok(None);
        }

        let entries: Vec<_> = fs::read_dir(&self.base_dir)
            .with_context(|| format!("failed to read checkpoint directory: {:?}", self.base_dir))?
            .flatten()
            .collect();

        // If we have a specific title and version, load directly
        if let (Some(title), Some(version)) = (&query.title, query.version) {
            let slug = slug::slugify(title);
            let filename = format!("{}_v{}.md", slug, version);
            let path = self.base_dir.join(&filename);

            if path.exists() {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read checkpoint: {:?}", path))?;
                return Checkpoint::from_markdown(&content).map(Some);
            }
            return Ok(None);
        }

        // Find matching checkpoints
        let mut candidates: Vec<(PathBuf, DateTime<Utc>, u32)> = Vec::new();

        for entry in entries {
            let path = entry.path();
            if !path.extension().map_or(false, |e| e == "md") {
                continue;
            }

            let filename = path.file_name().unwrap_or_default().to_string_lossy();

            // If title filter is specified, check if filename matches
            if let Some(title) = &query.title {
                let slug = slug::slugify(title);
                if !filename.starts_with(&format!("{}_v", slug)) {
                    continue;
                }
            }

            // Extract version from filename
            let version = filename
                .rsplit("_v")
                .next()
                .and_then(|s| s.strip_suffix(".md"))
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);

            // Get modification time as fallback for sorting
            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| DateTime::<Utc>::from(t))
                .unwrap_or_else(Utc::now);

            candidates.push((path, mtime, version));
        }

        if candidates.is_empty() {
            return Ok(None);
        }

        // Sort by modification time (newest first), then by version (highest first)
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));

        // Load the most recent one
        let (path, _, _) = &candidates[0];
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read checkpoint: {:?}", path))?;

        match Checkpoint::from_markdown(&content) {
            Ok(checkpoint) => Ok(Some(checkpoint)),
            Err(e) => {
                warn!("Failed to parse checkpoint {:?}: {}", path, e);
                // Try the next candidate if parsing fails
                if candidates.len() > 1 {
                    let (path2, _, _) = &candidates[1];
                    let content2 = fs::read_to_string(path2)?;
                    Checkpoint::from_markdown(&content2).map(Some)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// List all checkpoints, optionally filtered by title.
    pub fn list(&self, title_filter: Option<&str>) -> Result<Vec<Checkpoint>> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }

        let slug_filter = title_filter.map(slug::slugify);

        let mut checkpoints = Vec::new();

        for entry in fs::read_dir(&self.base_dir)?.flatten() {
            let path = entry.path();
            if !path.extension().map_or(false, |e| e == "md") {
                continue;
            }

            let filename = path.file_name().unwrap_or_default().to_string_lossy();

            // Apply slug filter if specified
            if let Some(slug) = &slug_filter {
                if !filename.starts_with(&format!("{}_v", slug)) {
                    continue;
                }
            }

            match fs::read_to_string(&path) {
                Ok(content) => match Checkpoint::from_markdown(&content) {
                    Ok(checkpoint) => checkpoints.push(checkpoint),
                    Err(e) => warn!("Failed to parse checkpoint {:?}: {}", path, e),
                },
                Err(e) => warn!("Failed to read checkpoint {:?}: {}", path, e),
            }
        }

        // Sort by creation time (newest first)
        checkpoints.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(checkpoints)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_slug_generation() {
        let checkpoint = Checkpoint::new(
            "Auth Refactor (V2!!)",
            "Working on auth",
            "Implementing OAuth",
            "- Add tests",
            vec!["src/auth.rs".to_string()],
            "manual",
        );
        assert_eq!(checkpoint.slug(), "auth-refactor-v2");
    }

    #[test]
    fn test_markdown_roundtrip() {
        let original = Checkpoint::new(
            "Test Checkpoint",
            "This is a test summary",
            "Working on tests",
            "- Step 1\n- Step 2",
            vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            "auto-compact",
        );

        let markdown = original.to_markdown();
        let parsed = Checkpoint::from_markdown(&markdown).unwrap();

        assert_eq!(parsed.title, original.title);
        assert_eq!(parsed.summary, original.summary);
        assert_eq!(parsed.current_task, original.current_task);
        assert_eq!(parsed.files, original.files);
        assert_eq!(parsed.trigger, original.trigger);
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let store = CheckpointStore::new(temp_dir.path());

        let checkpoint = Checkpoint::new(
            "My Task",
            "Summary here",
            "Current task",
            "Next steps",
            vec!["file1.rs".to_string()],
            "manual",
        );

        let path = store.save(checkpoint.clone()).unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("my-task_v1.md"));

        // Load it back
        let loaded = store.load(LoadQuery::latest()).unwrap().unwrap();
        assert_eq!(loaded.title, "My Task");
        assert_eq!(loaded.version, 1);
    }

    #[test]
    fn test_version_increment() {
        let temp_dir = TempDir::new().unwrap();
        let store = CheckpointStore::new(temp_dir.path());

        for i in 1..=3 {
            let checkpoint = Checkpoint::new(
                "Same Title",
                format!("Version {}", i),
                "Task",
                "Steps",
                vec![],
                "manual",
            );
            let path = store.save(checkpoint).unwrap();
            assert!(path.to_string_lossy().contains(&format!("same-title_v{}.md", i)));
        }

        // Load the latest version
        let loaded = store
            .load(LoadQuery::by_title("Same Title"))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.version, 3);
        assert_eq!(loaded.summary, "Version 3");
    }

    #[test]
    fn test_load_specific_version() {
        let temp_dir = TempDir::new().unwrap();
        let store = CheckpointStore::new(temp_dir.path());

        for i in 1..=3 {
            let checkpoint = Checkpoint::new(
                "Versioned",
                format!("Version {}", i),
                "Task",
                "Steps",
                vec![],
                "manual",
            );
            store.save(checkpoint).unwrap();
        }

        let loaded = store
            .load(LoadQuery::exact("Versioned", 2))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.summary, "Version 2");
    }
}
