//! Graph schema definitions for Minna's Gravity Well.
//!
//! This module defines the core types for the relationship graph:
//! - `NodeType`: Types of entities in the graph (users, issues, projects, etc.)
//! - `Relation`: Types of relationships between entities
//! - `NodeRef`: A reference to a node (for creating edges)
//! - `ExtractedEdge`: An edge extracted from a provider during sync

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Types of nodes in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// A person (you, collaborators)
    User,
    /// Linear, Jira, GitHub issue
    Issue,
    /// Linear project, Jira project, GitHub repo
    Project,
    /// Notion page, Confluence page, Google Doc
    Document,
    /// Slack channel, Discord channel
    Channel,
    /// Slack message, Discord message
    Message,
    /// GitHub PR
    PullRequest,
    /// Slack thread, email thread
    Thread,
    /// Git commit (local)
    Commit,
    /// Source file (local git)
    File,
}

impl NodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeType::User => "user",
            NodeType::Issue => "issue",
            NodeType::Project => "project",
            NodeType::Document => "document",
            NodeType::Channel => "channel",
            NodeType::Message => "message",
            NodeType::PullRequest => "pull_request",
            NodeType::Thread => "thread",
            NodeType::Commit => "commit",
            NodeType::File => "file",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(NodeType::User),
            "issue" => Some(NodeType::Issue),
            "project" => Some(NodeType::Project),
            "document" => Some(NodeType::Document),
            "channel" => Some(NodeType::Channel),
            "message" => Some(NodeType::Message),
            "pull_request" => Some(NodeType::PullRequest),
            "thread" => Some(NodeType::Thread),
            "commit" => Some(NodeType::Commit),
            "file" => Some(NodeType::File),
            _ => None,
        }
    }
}

/// Types of relationships between nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    // User ↔ Object
    /// User is assigned to Issue/PR
    AssignedTo,
    /// User authored Document/Message/Issue
    AuthorOf,
    /// User @mentioned in Object
    MentionedIn,
    /// User is reviewer on PR
    ReviewerOf,

    // User ↔ Container
    /// User is member of Channel/Project
    MemberOf,

    // Object ↔ Container
    /// Issue belongs to Project
    BelongsTo,
    /// Message posted in Channel
    PostedIn,

    // Object ↔ Object
    /// Page is child of Page
    ChildOf,
    /// Issue depends on Issue
    DependsOn,
    /// Issue blocks Issue
    Blocks,
    /// Document references Document
    References,
    /// Message is reply in Thread
    ThreadOf,

    // Local Git
    /// User edited File (via commit)
    EditedFile,
    /// Commit belongs to Project/Repo
    CommittedTo,

    // LSP (Future: Phase 2)
    /// File imports/references another File
    Imports,
}

impl Relation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Relation::AssignedTo => "assigned_to",
            Relation::AuthorOf => "author_of",
            Relation::MentionedIn => "mentioned_in",
            Relation::ReviewerOf => "reviewer_of",
            Relation::MemberOf => "member_of",
            Relation::BelongsTo => "belongs_to",
            Relation::PostedIn => "posted_in",
            Relation::ChildOf => "child_of",
            Relation::DependsOn => "depends_on",
            Relation::Blocks => "blocks",
            Relation::References => "references",
            Relation::ThreadOf => "thread_of",
            Relation::EditedFile => "edited_file",
            Relation::CommittedTo => "committed_to",
            Relation::Imports => "imports",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "assigned_to" => Some(Relation::AssignedTo),
            "author_of" => Some(Relation::AuthorOf),
            "mentioned_in" => Some(Relation::MentionedIn),
            "reviewer_of" => Some(Relation::ReviewerOf),
            "member_of" => Some(Relation::MemberOf),
            "belongs_to" => Some(Relation::BelongsTo),
            "posted_in" => Some(Relation::PostedIn),
            "child_of" => Some(Relation::ChildOf),
            "depends_on" => Some(Relation::DependsOn),
            "blocks" => Some(Relation::Blocks),
            "references" => Some(Relation::References),
            "thread_of" => Some(Relation::ThreadOf),
            "edited_file" => Some(Relation::EditedFile),
            "committed_to" => Some(Relation::CommittedTo),
            "imports" => Some(Relation::Imports),
            _ => None,
        }
    }
}

/// A reference to a node, used when creating edges.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRef {
    pub node_type: NodeType,
    pub provider: String,
    pub external_id: String,
    pub display_name: Option<String>,
}

impl NodeRef {
    /// Create a new node reference.
    pub fn new(
        node_type: NodeType,
        provider: impl Into<String>,
        external_id: impl Into<String>,
    ) -> Self {
        Self {
            node_type,
            provider: provider.into(),
            external_id: external_id.into(),
            display_name: None,
        }
    }

    /// Create a node reference with a display name.
    pub fn with_name(
        node_type: NodeType,
        provider: impl Into<String>,
        external_id: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            node_type,
            provider: provider.into(),
            external_id: external_id.into(),
            display_name: Some(display_name.into()),
        }
    }

    /// Convenience constructor for user nodes.
    pub fn user(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::User, provider, external_id)
    }

    /// Convenience constructor for issue nodes.
    pub fn issue(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::Issue, provider, external_id)
    }

    /// Convenience constructor for project nodes.
    pub fn project(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::Project, provider, external_id)
    }

    /// Convenience constructor for document nodes.
    pub fn document(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::Document, provider, external_id)
    }

    /// Convenience constructor for channel nodes.
    pub fn channel(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::Channel, provider, external_id)
    }

    /// Convenience constructor for message nodes.
    pub fn message(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::Message, provider, external_id)
    }

    /// Convenience constructor for pull request nodes.
    pub fn pull_request(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::PullRequest, provider, external_id)
    }

    /// Convenience constructor for thread nodes.
    pub fn thread(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::Thread, provider, external_id)
    }

    /// Convenience constructor for commit nodes.
    pub fn commit(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::Commit, provider, external_id)
    }

    /// Convenience constructor for file nodes.
    pub fn file(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self::new(NodeType::File, provider, external_id)
    }

    /// Generate the canonical node ID for storage.
    pub fn canonical_id(&self) -> String {
        format!(
            "{}:{}:{}",
            self.node_type.as_str(),
            self.provider,
            self.external_id
        )
    }
}

/// An edge extracted from a provider during sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEdge {
    pub from: NodeRef,
    pub to: NodeRef,
    pub relation: Relation,
    pub observed_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

impl ExtractedEdge {
    /// Create a new extracted edge.
    pub fn new(from: NodeRef, to: NodeRef, relation: Relation, observed_at: DateTime<Utc>) -> Self {
        Self {
            from,
            to,
            relation,
            observed_at,
            metadata: None,
        }
    }

    /// Create an edge with metadata.
    pub fn with_metadata(
        from: NodeRef,
        to: NodeRef,
        relation: Relation,
        observed_at: DateTime<Utc>,
        metadata: serde_json::Value,
    ) -> Self {
        Self {
            from,
            to,
            relation,
            observed_at,
            metadata: Some(metadata),
        }
    }
}

/// A stored node in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub node_type: NodeType,
    pub provider: String,
    pub external_id: String,
    pub display_name: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// A stored edge in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: i64,
    pub from_node: String,
    pub to_node: String,
    pub relation: Relation,
    pub provider: String,
    pub observed_at: DateTime<Utc>,
    pub weight: f32,
    pub metadata: Option<serde_json::Value>,
}

/// Ring assignment for proximity-based sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ring {
    /// The user themselves (distance 0)
    Core = 0,
    /// Direct connections (distance 1, fresh)
    One = 1,
    /// Extended network (distance 2, or decayed distance 1)
    Two = 2,
    /// Everything else (distance 3+)
    Beyond = 3,
}

impl Ring {
    pub fn from_int(i: i32) -> Self {
        match i {
            0 => Ring::Core,
            1 => Ring::One,
            2 => Ring::Two,
            _ => Ring::Beyond,
        }
    }

    pub fn as_int(&self) -> i32 {
        *self as i32
    }
}

/// A ring assignment for a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RingAssignment {
    pub node_id: String,
    pub ring: Ring,
    pub distance: i32,
    pub effective_distance: f32,
    pub path: Vec<String>,
    pub computed_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_ref_canonical_id() {
        let node = NodeRef::user("slack", "U123");
        assert_eq!(node.canonical_id(), "user:slack:U123");

        let node = NodeRef::issue("linear", "abc-123");
        assert_eq!(node.canonical_id(), "issue:linear:abc-123");
    }

    #[test]
    fn test_node_type_roundtrip() {
        for node_type in [
            NodeType::User,
            NodeType::Issue,
            NodeType::Project,
            NodeType::Document,
            NodeType::Channel,
            NodeType::Message,
            NodeType::PullRequest,
            NodeType::Thread,
            NodeType::Commit,
            NodeType::File,
        ] {
            let s = node_type.as_str();
            let parsed = NodeType::parse(s).unwrap();
            assert_eq!(node_type, parsed);
        }
    }

    #[test]
    fn test_relation_roundtrip() {
        for relation in [
            Relation::AssignedTo,
            Relation::AuthorOf,
            Relation::MentionedIn,
            Relation::ReviewerOf,
            Relation::MemberOf,
            Relation::BelongsTo,
            Relation::PostedIn,
            Relation::ChildOf,
            Relation::DependsOn,
            Relation::Blocks,
            Relation::References,
            Relation::ThreadOf,
            Relation::EditedFile,
            Relation::CommittedTo,
            Relation::Imports,
        ] {
            let s = relation.as_str();
            let parsed = Relation::parse(s).unwrap();
            assert_eq!(relation, parsed);
        }
    }
}
