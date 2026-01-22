//! Data extractors for the Gravity Well graph.
//!
//! This module provides extractors that scan local data sources
//! (like git repositories) and extract relationship edges.

#[cfg(feature = "local-git")]
pub mod local_git;

#[cfg(feature = "local-git")]
pub use local_git::LocalGitExtractor;
