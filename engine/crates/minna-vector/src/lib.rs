use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex, Once};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use libsqlite3_sys::sqlite3_auto_extension;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, SqlitePool};
use tokio::task;
use tracing::{instrument, warn};

use sqlite_vec::sqlite3_vec_init;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEmbedding {
    pub doc_id: i64,
    pub embedding: Vec<f32>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    pub label: String,
    pub doc_ids: Vec<i64>,
}

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

#[derive(Clone)]
pub struct FastEmbedder {
    model: Arc<Mutex<TextEmbedding>>,
}

impl FastEmbedder {
    pub fn new(model: EmbeddingModel, cache_dir: Option<PathBuf>) -> Result<Self> {
        let mut options = TextInitOptions::new(model);
        if let Some(dir) = cache_dir {
            options = options.with_cache_dir(dir);
        }
        let model = TextEmbedding::try_new(options)?;
        Ok(Self {
            model: Arc::new(Mutex::new(model)),
        })
    }
}

#[async_trait]
impl Embedder for FastEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let text = text.to_string();
        let model = self.model.clone();
        let embedding = task::spawn_blocking(move || {
            let mut guard = model
                .lock()
                .map_err(|_| anyhow!("embedding model lock poisoned"))?;
            let mut embeddings = guard.embed(vec![text], None)?;
            if embeddings.is_empty() {
                return Err(anyhow!("embedding model returned empty result"));
            }
            Ok::<Vec<f32>, anyhow::Error>(embeddings.remove(0))
        })
        .await??;
        Ok(embedding)
    }
}

#[derive(Debug, Clone)]
pub struct HashEmbedder {
    pub dims: usize,
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self { dims: 256 }
    }
}

#[async_trait]
impl Embedder for HashEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut vec = vec![0.0f32; self.dims];
        for token in text.split_whitespace() {
            let mut hash = 5381u64;
            for b in token.as_bytes() {
                hash = ((hash << 5).wrapping_add(hash)) ^ u64::from(*b);
            }
            let idx = (hash as usize) % self.dims;
            vec[idx] += 1.0;
        }
        normalize(&mut vec);
        Ok(vec)
    }
}

#[derive(Clone)]
pub struct VectorStore {
    pool: SqlitePool,
    sqlite_vec_available: bool,
}

impl VectorStore {
    pub async fn new(db_path: &Path) -> Result<Self> {
        register_sqlite_vec();
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
        let mut store = Self {
            pool,
            sqlite_vec_available: false,
        };
        store.init_schema().await?;
        store.sqlite_vec_available = store.detect_sqlite_vec().await.unwrap_or(false);
        Ok(store)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[instrument(skip_all)]
    async fn init_schema(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS vectors (\
                doc_id INTEGER PRIMARY KEY,\
                embedding TEXT NOT NULL,\
                updated_at TEXT NOT NULL\
            )",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_embedding(&self, doc_id: i64, embedding: &[f32]) -> Result<()> {
        let payload = serde_json::to_string(embedding)?;
        sqlx::query(
            "INSERT INTO vectors (doc_id, embedding, updated_at) VALUES (?1, ?2, ?3)\
            ON CONFLICT(doc_id) DO UPDATE SET embedding=excluded.embedding, updated_at=excluded.updated_at",
        )
        .bind(doc_id)
        .bind(payload)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn scrub_orphaned_embeddings(&self) -> Result<()> {
        sqlx::query("DELETE FROM vectors WHERE doc_id NOT IN (SELECT id FROM documents)")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_embedding(&self, doc_id: i64) -> Result<Option<Vec<f32>>> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT embedding FROM vectors WHERE doc_id = ?1",
        )
        .bind(doc_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|(payload,)| serde_json::from_str(&payload).ok()))
    }

    /// Get total vector count
    pub async fn count(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vectors")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    pub async fn list_embeddings(&self) -> Result<Vec<StoredEmbedding>> {
        let rows = sqlx::query_as::<_, (i64, String, String)>(
            "SELECT doc_id, embedding, updated_at FROM vectors",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(doc_id, embedding, updated_at)| {
                let embedding: Vec<f32> = serde_json::from_str(&embedding).ok()?;
                let updated_at = DateTime::parse_from_rfc3339(&updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                Some(StoredEmbedding {
                    doc_id,
                    embedding,
                    updated_at,
                })
            })
            .collect())
    }

    pub async fn search_semantic<E: Embedder + ?Sized>(
        &self,
        embedder: &E,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(i64, f32)>> {
        let query_embedding = embedder.embed(query).await?;
        self.search_with_embedding(&query_embedding, limit).await
    }

    pub async fn search_with_embedding(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(i64, f32)>> {
        if self.sqlite_vec_available {
            if let Ok(results) = self
                .search_with_embedding_sqlite_vec(query_embedding, limit)
                .await
            {
                return Ok(results);
            }
        }
        let embeddings = self.list_embeddings().await?;
        let mut scored: Vec<(i64, f32)> = embeddings
            .into_iter()
            .map(|row| {
                let score = cosine_similarity(query_embedding, &row.embedding);
                (row.doc_id, score)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }

    pub async fn cluster_documents(
        &self,
        min_similarity: f32,
        min_points: usize,
    ) -> Result<Vec<Cluster>> {
        let embeddings = self.list_embeddings().await?;
        let mut parent: HashMap<i64, i64> = HashMap::new();
        let ids: Vec<i64> = embeddings.iter().map(|e| e.doc_id).collect();
        for id in &ids {
            parent.insert(*id, *id);
        }

        for i in 0..embeddings.len() {
            for j in (i + 1)..embeddings.len() {
                let sim = cosine_similarity(&embeddings[i].embedding, &embeddings[j].embedding);
                if sim >= min_similarity {
                    union(&mut parent, embeddings[i].doc_id, embeddings[j].doc_id);
                }
            }
        }

        let mut clusters: HashMap<i64, Vec<i64>> = HashMap::new();
        for id in ids {
            let root = find(&mut parent, id);
            clusters.entry(root).or_default().push(id);
        }

        let mut results = Vec::new();
        for (idx, (_, doc_ids)) in clusters.into_iter().enumerate() {
            if doc_ids.len() >= min_points {
                results.push(Cluster {
                    label: format!("Cluster {}", idx + 1),
                    doc_ids,
                });
            }
        }
        Ok(results)
    }

    async fn detect_sqlite_vec(&self) -> Result<bool> {
        let version = sqlx::query_scalar::<_, String>("SELECT vec_version()")
            .fetch_one(&self.pool)
            .await;
        if let Ok(version) = version {
            let _ = version;
            return Ok(true);
        }
        Ok(false)
    }

    async fn search_with_embedding_sqlite_vec(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(i64, f32)>> {
        let payload = serde_json::to_string(query_embedding)?;
        let rows = sqlx::query_as::<_, (i64, f32)>(
            "SELECT doc_id, (1.0 - vec_distance_cosine(vec_f32(?1), vec_f32(embedding))) as score \
            FROM vectors ORDER BY score DESC LIMIT ?2",
        )
        .bind(payload)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

fn normalize(vec: &mut [f32]) {
    let norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vec.iter_mut() {
            *v /= norm;
        }
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for i in 0..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

fn find(parent: &mut HashMap<i64, i64>, x: i64) -> i64 {
    let parent_x = *parent.get(&x).unwrap_or(&x);
    if parent_x != x {
        let root = find(parent, parent_x);
        parent.insert(x, root);
    }
    *parent.get(&x).unwrap_or(&x)
}

fn union(parent: &mut HashMap<i64, i64>, a: i64, b: i64) {
    let root_a = find(parent, a);
    let root_b = find(parent, b);
    if root_a != root_b {
        parent.insert(root_b, root_a);
    }
}

pub fn embedder_from_env() -> Result<Arc<dyn Embedder>> {
    let backend = std::env::var("MINNA_EMBED_BACKEND").unwrap_or_else(|_| "fastembed".to_string());
    if backend.eq_ignore_ascii_case("hash") {
        return Ok(Arc::new(HashEmbedder::default()));
    }

    let model_name =
        std::env::var("MINNA_EMBED_MODEL").unwrap_or_else(|_| "nomic-embed-text-v1.5".to_string());
    let model = EmbeddingModel::from_str(&model_name)
        .unwrap_or(EmbeddingModel::NomicEmbedTextV15);
    let cache_dir = std::env::var("MINNA_EMBED_CACHE_DIR")
        .ok()
        .map(PathBuf::from);
    let embedder = FastEmbedder::new(model, cache_dir)?;
    Ok(Arc::new(embedder))
}

pub fn embedder_from_env_or_hash() -> Arc<dyn Embedder> {
    match embedder_from_env() {
        Ok(embedder) => embedder,
        Err(err) => {
            warn!("fast embedding unavailable: {}", err);
            Arc::new(HashEmbedder::default())
        }
    }
}

fn register_sqlite_vec() {
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        let _ = sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const (),
        )));
    });
}
