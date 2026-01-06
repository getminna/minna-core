-- Standard metadata storage
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    content TEXT,
    metadata TEXT, -- JSON string
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Vector storage using sqlite-vec
-- We use float[512] to match our Matryoshka truncated embeddings
CREATE VIRTUAL TABLE IF NOT EXISTS vec_documents USING vec0(
    embedding float[512]
);
