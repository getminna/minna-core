//! Identity linking service for Gravity Well.
//!
//! Automatically links user identities across providers based on email matching.
//! Provides fuzzy match suggestions for manual review.

use std::collections::HashMap;

use anyhow::Result;
use chrono::Utc;
use tracing::info;

use crate::storage::GraphStore;

/// A suggested identity link between two users.
#[derive(Debug, Clone)]
pub struct IdentityMatch {
    /// The canonical user ID (if one exists)
    pub canonical_id: Option<String>,
    /// Provider user IDs that might be the same person
    pub users: Vec<ProviderUser>,
    /// How the match was detected
    pub match_type: MatchType,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

/// A user from a specific provider.
#[derive(Debug, Clone)]
pub struct ProviderUser {
    pub provider: String,
    pub provider_user_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

/// How an identity match was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchType {
    /// Exact email match (high confidence)
    ExactEmail,
    /// Similar display names (lower confidence)
    SimilarName,
    /// Manual link by user
    Manual,
}

/// Service for managing user identity linking.
pub struct IdentityService;

impl IdentityService {
    /// Auto-link users with exact email matches.
    ///
    /// Scans all user nodes and links those with matching emails.
    /// Returns the number of users linked.
    pub async fn auto_link_by_email(graph: &GraphStore) -> Result<usize> {
        let users = graph.get_user_nodes().await?;

        // Group users by email
        let mut by_email: HashMap<String, Vec<(String, String, Option<String>)>> = HashMap::new();

        for user in &users {
            // Try to extract email from metadata
            let email = user.metadata
                .as_ref()
                .and_then(|m| m.get("email"))
                .and_then(|e| e.as_str())
                .map(|e| e.to_lowercase());

            if let Some(email) = email {
                by_email.entry(email).or_default().push((
                    user.provider.clone(),
                    user.external_id.clone(),
                    user.display_name.clone(),
                ));
            }
        }

        let mut linked = 0;

        // Link users with matching emails
        for (email, users) in by_email {
            if users.len() < 2 {
                continue; // Need at least 2 users to link
            }

            // Generate canonical ID (use email as the canonical ID)
            let canonical_id = format!("user:{}", email.replace('@', "_at_").replace('.', "_"));
            let display_name = users.iter()
                .find_map(|(_, _, name)| name.clone());

            info!(
                "Auto-linking {} users with email {}: {:?}",
                users.len(),
                email,
                users.iter().map(|(p, id, _)| format!("{}:{}", p, id)).collect::<Vec<_>>()
            );

            // Link each provider user to the canonical ID
            for (provider, provider_user_id, _) in &users {
                graph.link_user_identity(
                    &canonical_id,
                    Some(&email),
                    display_name.as_deref(),
                    provider,
                    provider_user_id,
                ).await?;
                linked += 1;
            }
        }

        Ok(linked)
    }

    /// Find potential identity matches for manual review.
    ///
    /// Looks for:
    /// - Users with similar emails (typos, aliases)
    /// - Users with matching display names across providers
    pub async fn find_fuzzy_matches(graph: &GraphStore) -> Result<Vec<IdentityMatch>> {
        let users = graph.get_user_nodes().await?;
        let mut matches = Vec::new();

        // Group by provider
        let mut by_provider: HashMap<String, Vec<_>> = HashMap::new();
        for user in &users {
            by_provider.entry(user.provider.clone()).or_default().push(user);
        }

        // Skip if we only have one provider
        if by_provider.len() < 2 {
            return Ok(matches);
        }

        // Check for similar display names across providers
        let mut seen_pairs: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

        for (provider1, users1) in &by_provider {
            for (provider2, users2) in &by_provider {
                if provider1 >= provider2 {
                    continue; // Avoid duplicate comparisons
                }

                for u1 in users1 {
                    for u2 in users2 {
                        // Skip if already linked to same canonical
                        let id1 = u1.id.clone();
                        let id2 = u2.id.clone();

                        let pair_key = if id1 < id2 {
                            (id1.clone(), id2.clone())
                        } else {
                            (id2.clone(), id1.clone())
                        };

                        if seen_pairs.contains(&pair_key) {
                            continue;
                        }

                        // Check display name similarity
                        if let (Some(name1), Some(name2)) = (&u1.display_name, &u2.display_name) {
                            let similarity = name_similarity(name1, name2);
                            if similarity > 0.8 {
                                seen_pairs.insert(pair_key);

                                matches.push(IdentityMatch {
                                    canonical_id: None,
                                    users: vec![
                                        ProviderUser {
                                            provider: u1.provider.clone(),
                                            provider_user_id: u1.external_id.clone(),
                                            email: u1.metadata.as_ref()
                                                .and_then(|m| m.get("email"))
                                                .and_then(|e| e.as_str())
                                                .map(|s| s.to_string()),
                                            display_name: u1.display_name.clone(),
                                        },
                                        ProviderUser {
                                            provider: u2.provider.clone(),
                                            provider_user_id: u2.external_id.clone(),
                                            email: u2.metadata.as_ref()
                                                .and_then(|m| m.get("email"))
                                                .and_then(|e| e.as_str())
                                                .map(|s| s.to_string()),
                                            display_name: u2.display_name.clone(),
                                        },
                                    ],
                                    match_type: MatchType::SimilarName,
                                    confidence: similarity,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Sort by confidence descending
        matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        Ok(matches)
    }

    /// Manually link two users.
    pub async fn link_users(
        graph: &GraphStore,
        provider1: &str,
        user_id1: &str,
        provider2: &str,
        user_id2: &str,
    ) -> Result<String> {
        // Generate canonical ID
        let canonical_id = format!("user:linked:{}_{}",
            Utc::now().timestamp(),
            &user_id1[..user_id1.len().min(8)]
        );

        // Get emails/names if available
        let node1_id = format!("user:{}:{}", provider1, user_id1);
        let node2_id = format!("user:{}:{}", provider2, user_id2);

        let node1 = graph.get_node(&node1_id).await?;
        let node2 = graph.get_node(&node2_id).await?;

        let email = node1.as_ref()
            .and_then(|n| n.metadata.as_ref())
            .and_then(|m| m.get("email"))
            .and_then(|e| e.as_str())
            .or_else(|| node2.as_ref()
                .and_then(|n| n.metadata.as_ref())
                .and_then(|m| m.get("email"))
                .and_then(|e| e.as_str()));

        let display_name = node1.as_ref()
            .and_then(|n| n.display_name.as_deref())
            .or_else(|| node2.as_ref().and_then(|n| n.display_name.as_deref()));

        // Link both users
        graph.link_user_identity(&canonical_id, email, display_name, provider1, user_id1).await?;
        graph.link_user_identity(&canonical_id, email, display_name, provider2, user_id2).await?;

        info!("Manually linked users: {}:{} <-> {}:{} as {}",
              provider1, user_id1, provider2, user_id2, canonical_id);

        Ok(canonical_id)
    }

    /// Get the count of pending (unlinked) identity suggestions.
    pub async fn pending_suggestions_count(graph: &GraphStore) -> Result<usize> {
        let matches = Self::find_fuzzy_matches(graph).await?;
        Ok(matches.len())
    }
}

/// Calculate name similarity using Jaro-Winkler-like algorithm.
fn name_similarity(a: &str, b: &str) -> f32 {
    let a = a.to_lowercase();
    let b = b.to_lowercase();

    if a == b {
        return 1.0;
    }

    // Simple character overlap ratio
    let a_chars: std::collections::HashSet<char> = a.chars().collect();
    let b_chars: std::collections::HashSet<char> = b.chars().collect();

    let intersection = a_chars.intersection(&b_chars).count();
    let union = a_chars.union(&b_chars).count();

    if union == 0 {
        return 0.0;
    }

    let jaccard = intersection as f32 / union as f32;

    // Bonus for same prefix
    let prefix_len = a.chars()
        .zip(b.chars())
        .take_while(|(c1, c2)| c1 == c2)
        .count();

    let prefix_bonus = (prefix_len as f32 / a.len().max(b.len()) as f32) * 0.1;

    (jaccard + prefix_bonus).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_similarity() {
        assert!(name_similarity("John Doe", "john doe") > 0.99);
        assert!(name_similarity("John Doe", "John D.") > 0.7);
        assert!(name_similarity("John Doe", "Jane Smith") < 0.5);
        assert!(name_similarity("Alice", "Alice") == 1.0);
    }

    #[test]
    fn test_match_type_equality() {
        assert_eq!(MatchType::ExactEmail, MatchType::ExactEmail);
        assert_ne!(MatchType::ExactEmail, MatchType::SimilarName);
    }
}
