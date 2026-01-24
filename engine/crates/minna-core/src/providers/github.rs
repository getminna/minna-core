//! GitHub provider implementation.
//!
//! Syncs pull requests and issues from GitHub, extracting relationship edges for Gravity Well.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::info;

use crate::Document;
use crate::progress::emit_progress;
use minna_auth_bridge::TokenStore;

use super::{
    call_with_backoff, calculate_since, ExtractedEdge, NodeRef, NodeType, Relation,
    SyncContext, SyncProvider, SyncSummary,
};

/// GitHub provider for syncing PRs and issues.
pub struct GithubProvider;

#[async_trait]
impl SyncProvider for GithubProvider {
    fn name(&self) -> &'static str {
        "github"
    }

    fn display_name(&self) -> &'static str {
        "GitHub"
    }

    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        let is_full_sync = mode == Some("full");
        info!(
            "Starting GitHub sync (since_days: {:?}, mode: {:?})",
            since_days, mode
        );

        // Load OAuth token
        let token_store = TokenStore::load(ctx.auth_path)?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Github)
            .ok_or_else(|| anyhow::anyhow!("missing github token"))?;

        // Calculate since timestamp
        let cursor_str = ctx.get_sync_cursor("github_cursor").await?;
        let since = if is_full_sync {
            let days = since_days.unwrap_or(90);
            info!("GitHub: performing full sync (last {} days)", days);
            Utc::now() - chrono::Duration::days(days)
        } else {
            calculate_since(since_days, mode, cursor_str.as_deref())
        };
        let since_str = since.to_rfc3339();

        info!("GitHub sync window starting from: {}", since_str);

        let repo_limit = self.get_repo_limit(is_full_sync);
        let issue_limit = self.get_issue_limit(is_full_sync);

        // Fetch repositories
        let repos = self.fetch_repos(ctx, &token.access_token, repo_limit).await?;
        info!("Found {} GitHub repositories", repos.len());
        emit_progress(
            "github",
            "syncing",
            &format!("Found {} repositories", repos.len()),
            Some(0),
        );

        let mut docs_indexed = 0usize;
        let mut edges_extracted = 0usize;
        let mut repos_scanned = 0usize;

        for repo in repos.into_iter().take(repo_limit) {
            repos_scanned += 1;

            // Fetch issues/PRs for this repo
            let issues = self
                .fetch_issues(ctx, &token.access_token, &repo, &since_str, issue_limit)
                .await?;

            for issue in issues {
                // Only index PRs (issues with pull_request field)
                if issue.pull_request.is_none() {
                    continue;
                }

                let updated_at = DateTime::parse_from_rfc3339(&issue.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                let body = issue.body.as_deref().unwrap_or("");

                let doc = Document {
                    id: None,
                    uri: issue.html_url.clone(),
                    source: "github".to_string(),
                    title: Some(issue.title.clone()),
                    body: format!(
                        "# {}\n\n- Repo: {}/{}\n- Number: #{}\n- State: {}\n- Updated: {}\n- URL: {}\n\n{}",
                        issue.title,
                        repo.owner.login,
                        repo.name,
                        issue.number,
                        issue.state.as_deref().unwrap_or("unknown"),
                        issue.updated_at,
                        issue.html_url,
                        body
                    ),
                    updated_at,
                };

                ctx.index_document(doc).await?;
                docs_indexed += 1;

                // Extract and store edges
                let edges = self.extract_edges_from_issue(&repo, &issue, updated_at);
                if !edges.is_empty() {
                    ctx.index_edges(&edges).await?;
                    edges_extracted += edges.len();
                }

                if docs_indexed.is_multiple_of(5) {
                    emit_progress(
                        "github",
                        "syncing",
                        &format!("Indexing: {} PRs", docs_indexed),
                        Some(docs_indexed),
                    );
                }
            }
        }

        // Update sync cursor
        let cursor = Utc::now().to_rfc3339();
        ctx.set_sync_cursor("github_cursor", &cursor).await?;

        info!(
            "GitHub sync complete: {} repos, {} docs, {} edges",
            repos_scanned, docs_indexed, edges_extracted
        );

        Ok(SyncSummary {
            provider: "github".to_string(),
            items_scanned: repos_scanned,
            documents_processed: docs_indexed,
            updated_at: cursor,
        })
    }
}

impl GithubProvider {
    fn get_repo_limit(&self, is_full_sync: bool) -> usize {
        if is_full_sync {
            std::env::var("MINNA_GITHUB_REPO_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000)
        } else {
            std::env::var("MINNA_GITHUB_REPO_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(25)
        }
    }

    fn get_issue_limit(&self, is_full_sync: bool) -> usize {
        if is_full_sync {
            std::env::var("MINNA_GITHUB_ISSUE_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500)
        } else {
            std::env::var("MINNA_GITHUB_ISSUE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50)
        }
    }

    /// Fetch user's repositories.
    async fn fetch_repos(
        &self,
        ctx: &SyncContext<'_>,
        access_token: &str,
        limit: usize,
    ) -> Result<Vec<GithubRepo>> {
        let mut repos = Vec::new();
        let mut page = 1;

        while repos.len() < limit {
            let url = format!(
                "https://api.github.com/user/repos?per_page=100&page={}",
                page
            );

            let response = call_with_backoff("github", || {
                ctx.http_client
                    .get(&url)
                    .header("Authorization", format!("token {}", access_token))
            })
            .await?;

            let batch: Vec<GithubRepo> = response.json().await?;
            if batch.is_empty() {
                break;
            }

            repos.extend(batch);
            page += 1;
        }

        Ok(repos)
    }

    /// Fetch issues/PRs for a repository.
    async fn fetch_issues(
        &self,
        ctx: &SyncContext<'_>,
        access_token: &str,
        repo: &GithubRepo,
        since: &str,
        limit: usize,
    ) -> Result<Vec<GithubIssue>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/issues?state=all&since={}&per_page={}",
            repo.owner.login, repo.name, since, limit
        );

        let response = call_with_backoff("github", || {
            ctx.http_client
                .get(&url)
                .header("Authorization", format!("token {}", access_token))
        })
        .await?;

        let issues: Vec<GithubIssue> = response.json().await.unwrap_or_default();
        Ok(issues)
    }

    /// Extract relationship edges from a GitHub issue/PR.
    fn extract_edges_from_issue(
        &self,
        repo: &GithubRepo,
        issue: &GithubIssue,
        observed_at: DateTime<Utc>,
    ) -> Vec<ExtractedEdge> {
        let mut edges = Vec::new();

        // Determine if this is a PR or issue
        let is_pr = issue.pull_request.is_some();
        let node_type = if is_pr {
            NodeType::PullRequest
        } else {
            NodeType::Issue
        };

        // Issue/PR node
        let issue_node = NodeRef::with_name(
            node_type,
            "github",
            format!("{}/{}/#{}", repo.owner.login, repo.name, issue.number),
            &issue.title,
        );

        // Repository node (as project)
        let repo_node = NodeRef::with_name(
            NodeType::Project,
            "github",
            format!("{}/{}", repo.owner.login, repo.name),
            &repo.name,
        );

        // Edge: Issue/PR → Repo (BelongsTo)
        edges.push(ExtractedEdge::new(
            issue_node.clone(),
            repo_node,
            Relation::BelongsTo,
            observed_at,
        ));

        // Edge: Author → Issue/PR (AuthorOf)
        if let Some(ref user) = issue.user {
            let user_node = NodeRef::with_name(
                NodeType::User,
                "github",
                &user.login,
                &user.login,
            );
            edges.push(ExtractedEdge::new(
                user_node,
                issue_node.clone(),
                Relation::AuthorOf,
                observed_at,
            ));
        }

        // Edge: Assignees → Issue/PR (AssignedTo)
        if let Some(ref assignees) = issue.assignees {
            for assignee in assignees {
                let user_node = NodeRef::with_name(
                    NodeType::User,
                    "github",
                    &assignee.login,
                    &assignee.login,
                );
                edges.push(ExtractedEdge::new(
                    user_node,
                    issue_node.clone(),
                    Relation::AssignedTo,
                    observed_at,
                ));
            }
        }

        // Edge: Reviewers → PR (ReviewerOf) - for PRs only
        if is_pr {
            if let Some(ref reviewers) = issue.requested_reviewers {
                for reviewer in reviewers {
                    let user_node = NodeRef::with_name(
                        NodeType::User,
                        "github",
                        &reviewer.login,
                        &reviewer.login,
                    );
                    edges.push(ExtractedEdge::new(
                        user_node,
                        issue_node.clone(),
                        Relation::ReviewerOf,
                        observed_at,
                    ));
                }
            }
        }

        edges
    }
}

// --- GitHub API Response Types ---

#[derive(Debug, Clone, Deserialize)]
struct GithubRepo {
    name: String,
    owner: GithubOwner,
    #[allow(dead_code)]
    #[serde(default)]
    private: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubOwner {
    login: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubIssue {
    number: i64,
    title: String,
    body: Option<String>,
    html_url: String,
    updated_at: String,
    state: Option<String>,
    user: Option<GithubUser>,
    assignees: Option<Vec<GithubUser>>,
    requested_reviewers: Option<Vec<GithubUser>>,
    pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubUser {
    login: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_edges_from_pr() {
        let provider = GithubProvider;

        let repo = GithubRepo {
            name: "minna-core".to_string(),
            owner: GithubOwner {
                login: "getminna".to_string(),
            },
            private: Some(false),
        };

        let issue = GithubIssue {
            number: 42,
            title: "Add Gravity Well".to_string(),
            body: Some("Implementing graph-based prioritization".to_string()),
            html_url: "https://github.com/getminna/minna-core/pull/42".to_string(),
            updated_at: "2024-01-15T10:00:00Z".to_string(),
            state: Some("open".to_string()),
            user: Some(GithubUser {
                login: "alice".to_string(),
            }),
            assignees: Some(vec![GithubUser {
                login: "bob".to_string(),
            }]),
            requested_reviewers: Some(vec![GithubUser {
                login: "charlie".to_string(),
            }]),
            pull_request: Some(serde_json::json!({})),
        };

        let edges = provider.extract_edges_from_issue(&repo, &issue, Utc::now());

        // Should have: BelongsTo, AuthorOf, AssignedTo, ReviewerOf
        assert_eq!(edges.len(), 4);

        // Check BelongsTo
        let belongs_to = edges.iter().find(|e| e.relation == Relation::BelongsTo);
        assert!(belongs_to.is_some());

        // Check AuthorOf
        let author_of = edges.iter().find(|e| e.relation == Relation::AuthorOf);
        assert!(author_of.is_some());
        assert_eq!(author_of.unwrap().from.external_id, "alice");

        // Check AssignedTo
        let assigned_to = edges.iter().find(|e| e.relation == Relation::AssignedTo);
        assert!(assigned_to.is_some());
        assert_eq!(assigned_to.unwrap().from.external_id, "bob");

        // Check ReviewerOf
        let reviewer_of = edges.iter().find(|e| e.relation == Relation::ReviewerOf);
        assert!(reviewer_of.is_some());
        assert_eq!(reviewer_of.unwrap().from.external_id, "charlie");
    }
}
