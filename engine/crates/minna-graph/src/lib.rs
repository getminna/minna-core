//! Minna Graph - Relationship graph for the Gravity Well.
//!
//! This crate provides the core graph infrastructure for Minna's proximity-aware
//! sync and retrieval system. It includes:
//!
//! - **Schema**: Node and relation types for the collaboration graph
//! - **Storage**: SQLite-backed persistence for nodes and edges
//! - **Ring Engine**: (Coming soon) BFS-based ring calculation with temporal decay
//!
//! # Example
//!
//! ```ignore
//! use minna_graph::{GraphStore, NodeRef, ExtractedEdge, Relation};
//! use chrono::Utc;
//!
//! // Create graph store
//! let store = GraphStore::new(pool);
//!
//! // Extract and store an edge
//! let user = NodeRef::user("slack", "U123");
//! let message = NodeRef::message("slack", "1234567890.123456");
//! let edge = ExtractedEdge::new(user, message, Relation::AuthorOf, Utc::now());
//! store.upsert_edge(&edge).await?;
//! ```

pub mod schema;
pub mod storage;

// Re-export commonly used types
pub use schema::{
    ExtractedEdge, GraphEdge, GraphNode, NodeRef, NodeType, Relation, Ring, RingAssignment,
};
pub use storage::GraphStore;
