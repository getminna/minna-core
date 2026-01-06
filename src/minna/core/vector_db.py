from typing import List
import numpy as np
import sqlite3
import json
import uuid
import os
from pathlib import Path
from fastembed import TextEmbedding

from minna.utils.paths import get_data_path, get_resource_path

# Try to import sqlite_vec to load extension if available
try:
    import sqlite_vec
    HAS_SQLITE_VEC = True
except ImportError:
    HAS_SQLITE_VEC = False

class VectorManager:
    def __init__(self, db_path: str = None):
        # Initialize Nomic v1.5 model
        self.model = TextEmbedding(model_name="nomic-ai/nomic-embed-text-v1.5")
        self.target_dim = 512
        
        if db_path:
            self.db_path = db_path
        else:
            self.db_path = str(get_data_path("minna.db"))
            
        print(f"Initialised VectorDB at: {self.db_path}")
        self.setup_db()

    def _get_connection(self):
        conn = sqlite3.connect(self.db_path)
        
        # CRITICAL: Enable WAL mode for concurrent read/write access
        # This allows the UI to read while workers are writing
        conn.execute("PRAGMA journal_mode=WAL;")
        
        # CRITICAL: Wait up to 5 seconds for lock release instead of crashing
        # This prevents "database is locked" errors when multiple workers run
        conn.execute("PRAGMA busy_timeout=5000;")
        
        conn.enable_load_extension(True)
        # Assuming sqlite-vec might be needed
        if HAS_SQLITE_VEC:
            sqlite_vec.load(conn)
        return conn

    def setup_db(self):
        """Initializes the database with schema."""
        # schema.sql is in src/minna/core/schema.sql, so relative path is "core/schema.sql"
        schema_path = get_resource_path("core/schema.sql")
        with open(schema_path, "r") as f:
            schema_sql = f.read()
            
        with self._get_connection() as conn:
            conn.executescript(schema_sql)

    def embed_text(self, text: str) -> List[float]:
        """
        Generates an embedding using Matryoshka Representation Learning.

        We use Nomic Embed Text v1.5, which outputs 768 dimensions.
        We truncate to the first 512 dimensions (Matryoshka) and apply
        L2 Normalization to maintain cosine similarity accuracy while
        reducing storage by 33%.

        The math:
        1. Generate: E = model.embed(text)  # shape (768,)
        2. Truncate: E' = E[:512]           # Matryoshka slice
        3. Normalize: E'' = E' / ||E'||_2   # L2 norm for cosine similarity

        Args:
            text: The input text to embed.

        Returns:
            A list of 512 floats representing the normalized embedding.
        """
        # fastembed returns a generator of numpy arrays
        embedding_gen = self.model.embed([text])
        embedding = next(embedding_gen) # This is a numpy array of shape (768,)

        # 1. Truncate to 512 dimensions (Matryoshka)
        truncated_embedding = embedding[:self.target_dim]

        # 2. Apply L2 Normalization
        norm = np.linalg.norm(truncated_embedding)
        
        if norm == 0:
            normalized_embedding = truncated_embedding
        else:
            normalized_embedding = truncated_embedding / norm

        return normalized_embedding.tolist()

    def add_documents(self, documents: List):
        """
        Embeds and saves a list of Documents to the database.
        """
        if not documents:
            return

        conn = self._get_connection()
        try:
            cursor = conn.cursor()
            inserted_count = 0
            
            for doc in documents:
                # 0. Quality Filter
                clean_content = doc.content.strip()
                if len(clean_content) < 10:
                    channel = doc.metadata.get("channel_name", "Unknown")
                    print(f"⚠️ Dropped message from #{channel}: '{clean_content[:20]}...' (Reason: Too short/Empty)")
                    continue
                
                if doc.source == "Unknown":
                    print(f"⚠️ Dropped message. Reason: Unknown source.")
                    continue

                # 1. Generate Embedding
                embedding = self.embed_text(doc.content)
                
                # 2. Prepare Data
                doc_id = str(uuid.uuid4())
                metadata_json = json.dumps(doc.metadata)
                
                # 3. Insert into documents (Metadata)
                cursor.execute(
                    "INSERT INTO documents (id, source, content, metadata) VALUES (?, ?, ?, ?)",
                    (doc_id, doc.source, doc.content, metadata_json)
                )
                
                # Get the internal rowid of the inserted document
                # We query it back using the primary key ID
                cursor.execute("SELECT rowid FROM documents WHERE id = ?", (doc_id,))
                row = cursor.fetchone()
                if row:
                    rowid = row[0]
                    
                    # 4. Insert into vec_documents (Vector)
                    # vec0 expects embedding as float array/list compatible with sqlite-vec
                    cursor.execute(
                        "INSERT INTO vec_documents (rowid, embedding) VALUES (?, ?)",
                        (rowid, json.dumps(embedding))
                    )
                    inserted_count += 1
            
            conn.commit()
            print(f"Successfully added {inserted_count} documents.")
            
        except Exception as e:
            conn.rollback()
            print(f"Error adding documents: {e}")
            raise e
        finally:
            conn.close()

    def search_keyword(self, query: str, limit: int = 5) -> List[dict]:
        """
        Performs a simple keyword search using SQL LIKE.
        """
        conn = self._get_connection()
        try:
            cursor = conn.cursor()
            # Use % wildcards for partial matching
            like_query = f"%{query}%"
            
            cursor.execute(
                "SELECT source, content, metadata FROM documents WHERE content LIKE ? AND length(content) > 10 AND source != 'Unknown' LIMIT ?",
                (like_query, limit)
            )
            results = cursor.fetchall()
            
            formatted_results = []
            for row in results:
                formatted_results.append({
                    "source": row[0],
                    "content": row[1],
                    "metadata": json.loads(row[2]) if row[2] else {}
                })
            return formatted_results
            
        except Exception as e:
            print(f"Error executing keyword search: {e}")
            return []
        finally:
            conn.close()

    def search(self, query: str, limit: int = 5) -> dict:
        """
        Searches the vector database using a hybrid strategy (Vector + Keyword Fallback).
        Returns a dictionary:
        {
            "results": List[dict],
            "search_strategy": str  # "strong_match", "keyword", "weak_match", "no_results"
        }
        """
        # 1. Generate Query Vector (Truncated & Normalized)
        query_vector = self.embed_text(query)
        
        # 2. Execute Vector Search
        conn = self._get_connection()
        vector_results = []
        try:
            cursor = conn.cursor()
            
            # sqlite-vec search query
            query_sql = """
                WITH matches AS (
                  SELECT
                    rowid,
                    distance
                  FROM vec_documents
                  WHERE embedding MATCH ?
                  ORDER BY distance
                  LIMIT ?
                )
                    SELECT DISTINCT
                        d.source,
                        d.content,
                        d.metadata,
                        m.distance
                    FROM matches m
                    JOIN documents d ON m.rowid = d.rowid
                    WHERE d.content IS NOT NULL 
                      AND length(d.content) > 10
                      AND d.source != 'Unknown'
                    ORDER BY m.distance
                """
                
            cursor.execute(query_sql, (json.dumps(query_vector), limit))
            results = cursor.fetchall()
            
            # Format Vector Results
            seen_content = set()
            for row in results:
                content = row[1]
                if content not in seen_content:
                     vector_results.append({
                        "source": row[0],
                        "content": content,
                        "metadata": json.loads(row[2]) if row[2] else {},
                        "distance": row[3]
                    })
                     seen_content.add(content)
            
            vector_results = vector_results[:limit]
            
        except Exception as e:
            print(f"Error executing vector search: {e}")
            vector_results = []
        finally:
            conn.close()

        # 3. Hybrid Logic Application
        if not vector_results:
            return {"results": [], "search_strategy": "no_results"}

        top_distance = vector_results[0]["distance"]
        
        # Threshold Check: Distance < 0.65 means Strong Match
        if top_distance < 0.65:
            return {
                "results": vector_results,
                "search_strategy": "strong_match"
            }
        
        # If Weak Match (Distance > 0.65), try Keyword Search
        keyword_results = self.search_keyword(query, limit)
        
        if keyword_results:
            return {
                "results": keyword_results,
                "search_strategy": "keyword"
            }
        else:
            # Fallback to weak vector results
            return {
                "results": vector_results,
                "search_strategy": "weak_match"
            }
