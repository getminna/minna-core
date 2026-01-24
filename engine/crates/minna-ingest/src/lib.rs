use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, SqlitePool};
use std::str::FromStr;
use tracing::instrument;

// Re-export graph types for convenience
pub use minna_graph::{GraphStore, ExtractedEdge, NodeRef, Relation, NodeType, Ring};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Option<i64>,
    pub uri: String,
    pub source: String,
    pub title: Option<String>,
    pub body: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterRecord {
    pub id: Option<i64>,
    pub label: String,
    pub doc_ids: Vec<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct IngestionEngine {
    pool: SqlitePool,
}

impl IngestionEngine {
    pub async fn new(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let options = SqliteConnectOptions::from_str("sqlite:")?
            .filename(db_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        let engine = Self { pool };
        engine.init_schema().await?;
        Ok(engine)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get a GraphStore instance backed by the same database.
    pub fn graph_store(&self) -> GraphStore {
        GraphStore::new(self.pool.clone())
    }

    #[instrument(skip_all)]
    async fn init_schema(&self) -> Result<()> {
        sqlx::query("PRAGMA journal_mode=WAL;")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS documents (\
                id INTEGER PRIMARY KEY AUTOINCREMENT,\
                uri TEXT NOT NULL UNIQUE,\
                source TEXT NOT NULL,\
                title TEXT,\
                body TEXT NOT NULL,\
                updated_at TEXT NOT NULL\
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(\
                uri, title, body,\
                content='documents',\
                content_rowid='id'\
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS documents_ai AFTER INSERT ON documents BEGIN\n\
                INSERT INTO documents_fts(rowid, uri, title, body) VALUES (new.id, new.uri, new.title, new.body);\n\
            END;",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS documents_ad AFTER DELETE ON documents BEGIN\n\
                INSERT INTO documents_fts(documents_fts, rowid, uri, title, body) VALUES('delete', old.id, old.uri, old.title, old.body);\n\
            END;",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS documents_au AFTER UPDATE ON documents BEGIN\n\
                INSERT INTO documents_fts(documents_fts, rowid, uri, title, body) VALUES('delete', old.id, old.uri, old.title, old.body);\n\
                INSERT INTO documents_fts(rowid, uri, title, body) VALUES (new.id, new.uri, new.title, new.body);\n\
            END;",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS clusters (\
                id INTEGER PRIMARY KEY AUTOINCREMENT,\
                label TEXT NOT NULL,\
                doc_ids TEXT NOT NULL,\
                created_at TEXT NOT NULL\
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sync_state (\
                provider TEXT PRIMARY KEY,\
                cursor TEXT,\
                updated_at TEXT NOT NULL\
            )",
        )
        .execute(&self.pool)
        .await?;

        // Initialize graph schema (Gravity Well)
        GraphStore::init_schema(&self.pool).await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn upsert_document(&self, doc: &Document) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO documents (uri, source, title, body, updated_at) \
            VALUES (?1, ?2, ?3, ?4, ?5) \
            ON CONFLICT(uri) DO UPDATE SET \
                source=excluded.source, \
                title=excluded.title, \
                body=excluded.body, \
                updated_at=excluded.updated_at \
            RETURNING id",
        )
        .bind(&doc.uri)
        .bind(&doc.source)
        .bind(&doc.title)
        .bind(&doc.body)
        .bind(doc.updated_at.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn get_document_by_uri(&self, uri: &str) -> Result<Option<Document>> {
        let row = sqlx::query_as::<_, (i64, String, String, Option<String>, String, String)>(
            "SELECT id, uri, source, title, body, updated_at FROM documents WHERE uri = ?1",
        )
        .bind(uri)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(id, uri, source, title, body, updated_at)| Document {
            id: Some(id),
            uri,
            source,
            title,
            body,
            updated_at: DateTime::parse_from_rfc3339(&updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }))
    }

    pub async fn fetch_documents_by_ids(&self, ids: &[i64]) -> Result<Vec<Document>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, uri, source, title, body, updated_at FROM documents WHERE id IN ({})",
            placeholders
        );
        let mut q = sqlx::query_as::<_, (i64, String, String, Option<String>, String, String)>(&query);
        for id in ids {
            q = q.bind(id);
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|(id, uri, source, title, body, updated_at)| Document {
                id: Some(id),
                uri,
                source,
                title,
                body,
                updated_at: DateTime::parse_from_rfc3339(&updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
            .collect())
    }

    pub async fn search_keyword(&self, query: &str, limit: usize) -> Result<Vec<Document>> {
        let rows = sqlx::query_as::<_, (i64, String, String, Option<String>, String, String)>(
            "SELECT d.id, d.uri, d.source, d.title, d.body, d.updated_at\
            FROM documents_fts f JOIN documents d ON d.id = f.rowid\
            WHERE documents_fts MATCH ?1\
            ORDER BY bm25(documents_fts)\
            LIMIT ?2",
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, uri, source, title, body, updated_at)| Document {
                id: Some(id),
                uri,
                source,
                title,
                body,
                updated_at: DateTime::parse_from_rfc3339(&updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
            .collect())
    }

    pub async fn delete_documents_by_source(&self, source: &str) -> Result<()> {
        sqlx::query("DELETE FROM documents WHERE source = ?1")
            .bind(source)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn store_clusters(&self, clusters: &[ClusterRecord]) -> Result<()> {
        for cluster in clusters {
            let doc_ids = serde_json::to_string(&cluster.doc_ids)?;
            sqlx::query(
                "INSERT INTO clusters (label, doc_ids, created_at) VALUES (?1, ?2, ?3)",
            )
            .bind(&cluster.label)
            .bind(doc_ids)
            .bind(cluster.created_at.to_rfc3339())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_cluster_doc_ids(&self, label: &str) -> Result<Vec<i64>> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT doc_ids FROM clusters WHERE label = ?1 ORDER BY id DESC LIMIT 1",
        )
        .bind(label)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((doc_ids,)) = row {
            let ids: Vec<i64> = serde_json::from_str(&doc_ids).unwrap_or_default();
            Ok(ids)
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn list_clusters(&self, limit: usize) -> Result<Vec<ClusterRecord>> {
        let rows = sqlx::query_as::<_, (i64, String, String, String)>(
            "SELECT id, label, doc_ids, created_at FROM clusters ORDER BY id DESC LIMIT ?1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, label, doc_ids, created_at)| ClusterRecord {
                id: Some(id),
                label,
                doc_ids: serde_json::from_str(&doc_ids).unwrap_or_default(),
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
            .collect())
    }

    pub async fn set_sync_cursor(&self, provider: &str, cursor: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO sync_state (provider, cursor, updated_at) VALUES (?1, ?2, ?3)\
            ON CONFLICT(provider) DO UPDATE SET cursor=excluded.cursor, updated_at=excluded.updated_at",
        )
        .bind(provider)
        .bind(cursor)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_sync_cursor(&self, provider: &str) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT cursor FROM sync_state WHERE provider = ?1",
        )
        .bind(provider)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|(cursor,)| cursor))
    }

    /// Get total document count
    pub async fn document_count(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    /// Get document count per source
    pub async fn document_counts_by_source(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT source, COUNT(*) FROM documents GROUP BY source",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get last sync time per provider
    pub async fn get_sync_times(&self) -> Result<Vec<(String, DateTime<Utc>)>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT provider, updated_at FROM sync_state",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(provider, updated_at)| {
                DateTime::parse_from_rfc3339(&updated_at)
                    .ok()
                    .map(|dt| (provider, dt.with_timezone(&Utc)))
            })
            .collect())
    }
}
