//! Graph storage operations for SQLite.
//!
//! This module provides the `GraphStore` struct for persisting and querying
//! the relationship graph in SQLite.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tracing::instrument;

use crate::schema::{
    ExtractedEdge, GraphEdge, GraphNode, NodeRef, NodeType, Relation, Ring, RingAssignment,
};

/// Graph storage backed by SQLite.
#[derive(Clone)]
pub struct GraphStore {
    pool: SqlitePool,
}

impl GraphStore {
    /// Create a new GraphStore with an existing connection pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Initialize the graph schema (called during DB setup).
    #[instrument(skip_all)]
    pub async fn init_schema(pool: &SqlitePool) -> Result<()> {
        // Graph nodes
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS graph_nodes (
                id TEXT PRIMARY KEY,
                node_type TEXT NOT NULL,
                provider TEXT NOT NULL,
                external_id TEXT NOT NULL,
                display_name TEXT,
                metadata JSON,
                first_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(provider, external_id)
            )",
        )
        .execute(pool)
        .await?;

        // Graph edges
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS graph_edges (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                from_node TEXT NOT NULL REFERENCES graph_nodes(id),
                to_node TEXT NOT NULL REFERENCES graph_nodes(id),
                relation TEXT NOT NULL,
                provider TEXT NOT NULL,
                observed_at TEXT NOT NULL,
                weight REAL NOT NULL DEFAULT 1.0,
                metadata JSON,
                UNIQUE(from_node, to_node, relation, provider)
            )",
        )
        .execute(pool)
        .await?;

        // Indexes for traversal
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_edges_from ON graph_edges(from_node)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_edges_to ON graph_edges(to_node)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_edges_relation ON graph_edges(relation)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_edges_observed ON graph_edges(observed_at)")
            .execute(pool)
            .await?;

        // User identity linking
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS user_identities (
                canonical_id TEXT PRIMARY KEY,
                email TEXT UNIQUE,
                display_name TEXT
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS user_identity_links (
                canonical_id TEXT REFERENCES user_identities(canonical_id),
                provider TEXT NOT NULL,
                provider_user_id TEXT NOT NULL,
                PRIMARY KEY (provider, provider_user_id)
            )",
        )
        .execute(pool)
        .await?;

        // Ring cache
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ring_assignments (
                node_id TEXT PRIMARY KEY REFERENCES graph_nodes(id),
                ring INTEGER NOT NULL,
                distance INTEGER NOT NULL,
                effective_distance REAL NOT NULL,
                path JSON,
                computed_at TEXT NOT NULL
            )",
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Upsert a node into the graph.
    #[instrument(skip(self))]
    pub async fn upsert_node(&self, node_ref: &NodeRef) -> Result<String> {
        let id = node_ref.canonical_id();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO graph_nodes (id, node_type, provider, external_id, display_name, first_seen_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET
                display_name = COALESCE(excluded.display_name, graph_nodes.display_name),
                last_seen_at = excluded.last_seen_at",
        )
        .bind(&id)
        .bind(node_ref.node_type.as_str())
        .bind(&node_ref.provider)
        .bind(&node_ref.external_id)
        .bind(&node_ref.display_name)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Upsert an edge into the graph (creates nodes if needed).
    #[instrument(skip(self))]
    pub async fn upsert_edge(&self, edge: &ExtractedEdge) -> Result<i64> {
        // Ensure both nodes exist
        let from_id = self.upsert_node(&edge.from).await?;
        let to_id = self.upsert_node(&edge.to).await?;

        // Upsert edge
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO graph_edges (from_node, to_node, relation, provider, observed_at, weight, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, 1.0, ?6)
             ON CONFLICT(from_node, to_node, relation, provider) DO UPDATE SET
                observed_at = excluded.observed_at,
                metadata = COALESCE(excluded.metadata, graph_edges.metadata)
             RETURNING id",
        )
        .bind(&from_id)
        .bind(&to_id)
        .bind(edge.relation.as_str())
        .bind(&edge.from.provider) // Use from node's provider as edge provider
        .bind(edge.observed_at.to_rfc3339())
        .bind(edge.metadata.as_ref().map(|m| m.to_string()))
        .fetch_one(&self.pool)
        .await?;

        Ok(id)
    }

    /// Get a node by its canonical ID.
    pub async fn get_node(&self, id: &str) -> Result<Option<GraphNode>> {
        let row = sqlx::query_as::<_, (String, String, String, String, Option<String>, Option<String>, String, String)>(
            "SELECT id, node_type, provider, external_id, display_name, metadata, first_seen_at, last_seen_at
             FROM graph_nodes WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(id, node_type, provider, external_id, display_name, metadata, first_seen_at, last_seen_at)| {
            GraphNode {
                id,
                node_type: NodeType::parse(&node_type).unwrap_or(NodeType::User),
                provider,
                external_id,
                display_name,
                metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
                first_seen_at: DateTime::parse_from_rfc3339(&first_seen_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                last_seen_at: DateTime::parse_from_rfc3339(&last_seen_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            }
        }))
    }

    /// Get all edges originating from a node.
    pub async fn edges_from(&self, node_id: &str) -> Result<Vec<GraphEdge>> {
        let rows = sqlx::query_as::<_, (i64, String, String, String, String, String, f64, Option<String>)>(
            "SELECT id, from_node, to_node, relation, provider, observed_at, weight, metadata
             FROM graph_edges WHERE from_node = ?1",
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, from_node, to_node, relation, provider, observed_at, weight, metadata)| {
                GraphEdge {
                    id,
                    from_node,
                    to_node,
                    relation: Relation::parse(&relation).unwrap_or(Relation::References),
                    provider,
                    observed_at: DateTime::parse_from_rfc3339(&observed_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    weight: weight as f32,
                    metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
                }
            })
            .collect())
    }

    /// Get all edges pointing to a node.
    pub async fn edges_to(&self, node_id: &str) -> Result<Vec<GraphEdge>> {
        let rows = sqlx::query_as::<_, (i64, String, String, String, String, String, f64, Option<String>)>(
            "SELECT id, from_node, to_node, relation, provider, observed_at, weight, metadata
             FROM graph_edges WHERE to_node = ?1",
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, from_node, to_node, relation, provider, observed_at, weight, metadata)| {
                GraphEdge {
                    id,
                    from_node,
                    to_node,
                    relation: Relation::parse(&relation).unwrap_or(Relation::References),
                    provider,
                    observed_at: DateTime::parse_from_rfc3339(&observed_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    weight: weight as f32,
                    metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
                }
            })
            .collect())
    }

    /// Get total node count.
    pub async fn node_count(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    /// Get total edge count.
    pub async fn edge_count(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    /// Get node count by type.
    pub async fn node_count_by_type(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT node_type, COUNT(*) as count FROM graph_nodes GROUP BY node_type ORDER BY count DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Save a ring assignment.
    pub async fn save_ring_assignment(&self, assignment: &RingAssignment) -> Result<()> {
        let path_json = serde_json::to_string(&assignment.path)?;

        sqlx::query(
            "INSERT INTO ring_assignments (node_id, ring, distance, effective_distance, path, computed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(node_id) DO UPDATE SET
                ring = excluded.ring,
                distance = excluded.distance,
                effective_distance = excluded.effective_distance,
                path = excluded.path,
                computed_at = excluded.computed_at",
        )
        .bind(&assignment.node_id)
        .bind(assignment.ring.as_int())
        .bind(assignment.distance)
        .bind(assignment.effective_distance)
        .bind(&path_json)
        .bind(assignment.computed_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get ring assignment for a node.
    pub async fn get_ring_assignment(&self, node_id: &str) -> Result<Option<RingAssignment>> {
        let row = sqlx::query_as::<_, (String, i32, i32, f64, String, String)>(
            "SELECT node_id, ring, distance, effective_distance, path, computed_at
             FROM ring_assignments WHERE node_id = ?1",
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(node_id, ring, distance, effective_distance, path, computed_at)| {
            RingAssignment {
                node_id,
                ring: Ring::from_int(ring),
                distance,
                effective_distance: effective_distance as f32,
                path: serde_json::from_str(&path).unwrap_or_default(),
                computed_at: DateTime::parse_from_rfc3339(&computed_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            }
        }))
    }

    /// Get all nodes in a specific ring.
    pub async fn nodes_in_ring(&self, ring: Ring) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT node_id FROM ring_assignments WHERE ring = ?1",
        )
        .bind(ring.as_int())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Clear all ring assignments (before recomputation).
    pub async fn clear_ring_assignments(&self) -> Result<()> {
        sqlx::query("DELETE FROM ring_assignments")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get ring distribution (count per ring).
    pub async fn ring_distribution(&self) -> Result<Vec<(Ring, i64)>> {
        let rows = sqlx::query_as::<_, (i32, i64)>(
            "SELECT ring, COUNT(*) as count FROM ring_assignments GROUP BY ring ORDER BY ring",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(ring, count)| (Ring::from_int(ring), count))
            .collect())
    }

    /// Get all user nodes.
    pub async fn get_user_nodes(&self) -> Result<Vec<GraphNode>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>, Option<String>, String, String)>(
            "SELECT id, node_type, provider, external_id, display_name, metadata, first_seen_at, last_seen_at
             FROM graph_nodes WHERE node_type = 'user'",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, node_type, provider, external_id, display_name, metadata, first_seen_at, last_seen_at)| {
                GraphNode {
                    id,
                    node_type: NodeType::parse(&node_type).unwrap_or(NodeType::User),
                    provider,
                    external_id,
                    display_name,
                    metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
                    first_seen_at: DateTime::parse_from_rfc3339(&first_seen_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    last_seen_at: DateTime::parse_from_rfc3339(&last_seen_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                }
            })
            .collect())
    }

    /// Link a user identity across providers.
    pub async fn link_user_identity(
        &self,
        canonical_id: &str,
        email: Option<&str>,
        display_name: Option<&str>,
        provider: &str,
        provider_user_id: &str,
    ) -> Result<()> {
        // Upsert canonical identity
        sqlx::query(
            "INSERT INTO user_identities (canonical_id, email, display_name)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(canonical_id) DO UPDATE SET
                email = COALESCE(excluded.email, user_identities.email),
                display_name = COALESCE(excluded.display_name, user_identities.display_name)",
        )
        .bind(canonical_id)
        .bind(email)
        .bind(display_name)
        .execute(&self.pool)
        .await?;

        // Link provider identity
        sqlx::query(
            "INSERT INTO user_identity_links (canonical_id, provider, provider_user_id)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(provider, provider_user_id) DO UPDATE SET
                canonical_id = excluded.canonical_id",
        )
        .bind(canonical_id)
        .bind(provider)
        .bind(provider_user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get canonical ID for a provider user.
    pub async fn get_canonical_user_id(
        &self,
        provider: &str,
        provider_user_id: &str,
    ) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT canonical_id FROM user_identity_links WHERE provider = ?1 AND provider_user_id = ?2",
        )
        .bind(provider)
        .bind(provider_user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(id,)| id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        GraphStore::init_schema(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_upsert_node() {
        let pool = setup_test_db().await;
        let store = GraphStore::new(pool);

        let node_ref = NodeRef::user("slack", "U123");
        let id = store.upsert_node(&node_ref).await.unwrap();
        assert_eq!(id, "user:slack:U123");

        let node = store.get_node(&id).await.unwrap().unwrap();
        assert_eq!(node.node_type, NodeType::User);
        assert_eq!(node.provider, "slack");
        assert_eq!(node.external_id, "U123");
    }

    #[tokio::test]
    async fn test_upsert_edge() {
        let pool = setup_test_db().await;
        let store = GraphStore::new(pool);

        let from = NodeRef::user("slack", "U123");
        let to = NodeRef::message("slack", "1234567890.123456");
        let edge = ExtractedEdge::new(from, to, Relation::AuthorOf, Utc::now());

        let edge_id = store.upsert_edge(&edge).await.unwrap();
        assert!(edge_id > 0);

        // Verify nodes were created
        assert_eq!(store.node_count().await.unwrap(), 2);
        assert_eq!(store.edge_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_edges_from_to() {
        let pool = setup_test_db().await;
        let store = GraphStore::new(pool);

        let user = NodeRef::user("slack", "U123");
        let msg1 = NodeRef::message("slack", "msg1");
        let msg2 = NodeRef::message("slack", "msg2");

        store
            .upsert_edge(&ExtractedEdge::new(
                user.clone(),
                msg1.clone(),
                Relation::AuthorOf,
                Utc::now(),
            ))
            .await
            .unwrap();
        store
            .upsert_edge(&ExtractedEdge::new(
                user.clone(),
                msg2.clone(),
                Relation::AuthorOf,
                Utc::now(),
            ))
            .await
            .unwrap();

        let from_edges = store.edges_from(&user.canonical_id()).await.unwrap();
        assert_eq!(from_edges.len(), 2);

        let to_edges = store.edges_to(&msg1.canonical_id()).await.unwrap();
        assert_eq!(to_edges.len(), 1);
    }

    #[tokio::test]
    async fn test_ring_assignment() {
        let pool = setup_test_db().await;
        let store = GraphStore::new(pool);

        // First create the node (ring_assignments has FK to graph_nodes)
        let node = NodeRef::user("slack", "U123");
        store.upsert_node(&node).await.unwrap();

        let assignment = RingAssignment {
            node_id: node.canonical_id(),
            ring: Ring::One,
            distance: 1,
            effective_distance: 1.05,
            path: vec!["user:self".to_string()],
            computed_at: Utc::now(),
        };

        store.save_ring_assignment(&assignment).await.unwrap();

        let loaded = store
            .get_ring_assignment(&node.canonical_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.ring, Ring::One);
        assert_eq!(loaded.distance, 1);
    }
}
