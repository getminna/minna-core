//! Linear provider implementation.
//!
//! Syncs issues from Linear and extracts relationship edges for Gravity Well.

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

/// Linear provider for syncing issues.
pub struct LinearProvider;

#[async_trait]
impl SyncProvider for LinearProvider {
    fn name(&self) -> &'static str {
        "linear"
    }

    fn display_name(&self) -> &'static str {
        "Linear"
    }

    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        let is_full_sync = mode == Some("full");
        info!(
            "Starting Linear sync (since_days: {:?}, mode: {:?})",
            since_days, mode
        );

        // Load OAuth token from TokenStore
        let token_store = TokenStore::load(ctx.auth_path)?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Linear)
            .ok_or_else(|| anyhow::anyhow!("missing linear token"))?;

        // Calculate since timestamp
        let cursor_str = ctx.get_sync_cursor("linear").await?;
        let since = if is_full_sync {
            let days = since_days.unwrap_or(90);
            info!("Linear: performing full sync (last {} days)", days);
            Utc::now() - chrono::Duration::days(days)
        } else {
            calculate_since(since_days, mode, cursor_str.as_deref())
        };
        let since_str = since.to_rfc3339();

        info!("Linear sync window starting from: {}", since_str);

        let limit = if is_full_sync {
            std::env::var("MINNA_LINEAR_ISSUE_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500usize)
        } else {
            std::env::var("MINNA_LINEAR_ISSUE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50usize)
        };

        emit_progress("linear", "syncing", "Searching for issues...", Some(0));

        let mut after: Option<String> = None;
        let mut docs_indexed = 0usize;
        let mut edges_extracted = 0usize;
        let mut max_updated = since_str.clone();

        loop {
            // Enhanced GraphQL query with data needed for edge extraction
            let query = r#"
                query Issues($since: DateTime!, $after: String, $first: Int!) {
                    issues(filter: { updatedAt: { gte: $since } }, first: $first, after: $after) {
                        nodes {
                            id
                            identifier
                            title
                            description
                            updatedAt
                            url
                            state { name }
                            assignee { id name email }
                            creator { id name email }
                            project { id name }
                            team { id name }
                        }
                        pageInfo { hasNextPage endCursor }
                    }
                }
            "#;

            let payload = serde_json::json!({
                "query": query,
                "variables": {
                    "since": since_str,
                    "after": after,
                    "first": limit as i64
                }
            });

            let response = call_with_backoff("linear", || {
                ctx.http_client
                    .post("https://api.linear.app/graphql")
                    .header("Authorization", token.access_token.clone())
                    .json(&payload)
            })
            .await?;

            let body: LinearResponse = response.json().await?;

            if let Some(errors) = body.errors {
                return Err(anyhow::anyhow!("Linear API error: {}", errors[0].message));
            }

            let data = body
                .data
                .ok_or_else(|| anyhow::anyhow!("Linear response missing data"))?;

            for issue in data.issues.nodes {
                let updated_at = DateTime::parse_from_rfc3339(&issue.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                if issue.updated_at > max_updated {
                    max_updated = issue.updated_at.clone();
                }

                // Build document
                let doc = Document {
                    id: None,
                    uri: issue.url.clone(),
                    source: "linear".to_string(),
                    title: Some(format!("{} {}", issue.identifier, issue.title)),
                    body: format!(
                        "# {}\n\n- State: {}\n- Assignee: {}\n- Updated: {}\n- URL: {}\n\n{}",
                        issue.title,
                        issue
                            .state
                            .as_ref()
                            .map(|s| s.name.as_str())
                            .unwrap_or("Unknown"),
                        issue
                            .assignee
                            .as_ref()
                            .map(|a| a.name.as_str())
                            .unwrap_or("Unassigned"),
                        issue.updated_at,
                        issue.url,
                        issue.description.as_deref().unwrap_or("")
                    ),
                    updated_at,
                };

                ctx.index_document(doc).await?;
                docs_indexed += 1;

                // Extract and store edges for Gravity Well
                let edges = self.extract_edges_from_issue(&issue, updated_at);
                if !edges.is_empty() {
                    ctx.index_edges(&edges).await?;
                    edges_extracted += edges.len();
                }

                if docs_indexed.is_multiple_of(10) {
                    emit_progress(
                        "linear",
                        "syncing",
                        &format!("Indexing: {} issues", docs_indexed),
                        Some(docs_indexed),
                    );
                }
            }

            if data.issues.page_info.has_next_page {
                after = data.issues.page_info.end_cursor;
            } else {
                break;
            }
        }

        // Update sync cursor
        ctx.set_sync_cursor("linear", &max_updated).await?;

        info!(
            "Linear sync complete: {} docs indexed, {} edges extracted",
            docs_indexed, edges_extracted
        );

        Ok(SyncSummary {
            provider: "linear".to_string(),
            items_scanned: 1,
            documents_processed: docs_indexed,
            updated_at: max_updated,
        })
    }
}

impl LinearProvider {
    /// Extract relationship edges from a Linear issue.
    fn extract_edges_from_issue(
        &self,
        issue: &LinearIssue,
        observed_at: DateTime<Utc>,
    ) -> Vec<ExtractedEdge> {
        let mut edges = Vec::new();

        // Create issue node reference
        let issue_node = NodeRef::with_name(
            NodeType::Issue,
            "linear",
            &issue.id,
            &issue.identifier,
        );

        // Edge: Assignee → Issue (AssignedTo)
        if let Some(ref assignee) = issue.assignee {
            let user_node = NodeRef::with_name(
                NodeType::User,
                "linear",
                &assignee.id,
                &assignee.name,
            );
            edges.push(ExtractedEdge::new(
                user_node,
                issue_node.clone(),
                Relation::AssignedTo,
                observed_at,
            ));
        }

        // Edge: Creator → Issue (AuthorOf)
        if let Some(ref creator) = issue.creator {
            let user_node = NodeRef::with_name(
                NodeType::User,
                "linear",
                &creator.id,
                &creator.name,
            );
            edges.push(ExtractedEdge::new(
                user_node,
                issue_node.clone(),
                Relation::AuthorOf,
                observed_at,
            ));
        }

        // Edge: Issue → Project (BelongsTo)
        if let Some(ref project) = issue.project {
            let project_node = NodeRef::with_name(
                NodeType::Project,
                "linear",
                &project.id,
                &project.name,
            );
            edges.push(ExtractedEdge::new(
                issue_node.clone(),
                project_node,
                Relation::BelongsTo,
                observed_at,
            ));
        }

        // Edge: Issue → Team (BelongsTo) - teams are like containers
        if let Some(ref team) = issue.team {
            let team_node = NodeRef::with_name(
                NodeType::Project, // Teams are project-like containers
                "linear",
                &team.id,
                &team.name,
            );
            edges.push(ExtractedEdge::new(
                issue_node,
                team_node,
                Relation::BelongsTo,
                observed_at,
            ));
        }

        edges
    }
}

// --- Linear API Response Types ---

#[derive(Debug, Clone, Deserialize)]
struct LinearResponse {
    data: Option<LinearData>,
    errors: Option<Vec<LinearError>>,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearError {
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearData {
    issues: LinearIssues,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearIssues {
    nodes: Vec<LinearIssue>,
    #[serde(rename = "pageInfo")]
    page_info: LinearPageInfo,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearIssue {
    id: String,
    identifier: String,
    title: String,
    description: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    url: String,
    state: Option<LinearState>,
    assignee: Option<LinearUser>,
    creator: Option<LinearUser>,
    project: Option<LinearProject>,
    team: Option<LinearTeam>,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearState {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearUser {
    id: String,
    name: String,
    #[allow(dead_code)]
    email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearProject {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearTeam {
    id: String,
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_edges_from_issue() {
        let provider = LinearProvider;
        let issue = LinearIssue {
            id: "issue-123".to_string(),
            identifier: "ENG-42".to_string(),
            title: "Fix the bug".to_string(),
            description: Some("It's broken".to_string()),
            updated_at: "2024-01-15T10:00:00Z".to_string(),
            url: "https://linear.app/team/issue/ENG-42".to_string(),
            state: Some(LinearState {
                name: "In Progress".to_string(),
            }),
            assignee: Some(LinearUser {
                id: "user-456".to_string(),
                name: "Alice".to_string(),
                email: Some("alice@example.com".to_string()),
            }),
            creator: Some(LinearUser {
                id: "user-789".to_string(),
                name: "Bob".to_string(),
                email: Some("bob@example.com".to_string()),
            }),
            project: Some(LinearProject {
                id: "proj-abc".to_string(),
                name: "Backend".to_string(),
            }),
            team: Some(LinearTeam {
                id: "team-xyz".to_string(),
                name: "Engineering".to_string(),
            }),
        };

        let edges = provider.extract_edges_from_issue(&issue, Utc::now());

        // Should have 4 edges: assignee, creator, project, team
        assert_eq!(edges.len(), 4);

        // Check assignee edge
        let assignee_edge = edges.iter().find(|e| e.relation == Relation::AssignedTo);
        assert!(assignee_edge.is_some());
        let ae = assignee_edge.unwrap();
        assert_eq!(ae.from.external_id, "user-456");
        assert_eq!(ae.to.external_id, "issue-123");

        // Check creator edge
        let creator_edge = edges.iter().find(|e| e.relation == Relation::AuthorOf);
        assert!(creator_edge.is_some());
        let ce = creator_edge.unwrap();
        assert_eq!(ce.from.external_id, "user-789");

        // Check project edge
        let project_edges: Vec<_> = edges
            .iter()
            .filter(|e| e.relation == Relation::BelongsTo)
            .collect();
        assert_eq!(project_edges.len(), 2); // project + team
    }
}
