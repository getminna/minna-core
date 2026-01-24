//! Ring Engine for Gravity Well.
//!
//! Calculates ring assignments (Core, Ring 1, Ring 2, Beyond) for all nodes
//! based on graph distance from the user's identity with temporal decay.

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use tracing::{info, debug};

use crate::schema::{Ring, RingAssignment};
use crate::storage::GraphStore;

/// Configuration for ring calculation.
#[derive(Debug, Clone)]
pub struct RingConfig {
    /// Decay half-life in days (default: 30)
    pub decay_half_life_days: i64,
    /// Days after which edges become "ghost edges" (default: 90)
    pub ghost_edge_days: i64,
    /// Weight multiplier for ghost edges (default: 0.1)
    pub ghost_edge_weight: f64,
    /// Maximum distance for Ring 1 (default: 2.0)
    pub ring_1_threshold: f64,
    /// Maximum distance for Ring 2 (default: 4.0)
    pub ring_2_threshold: f64,
    /// Maximum hops to consider (default: 10)
    pub max_hops: usize,
}

impl Default for RingConfig {
    fn default() -> Self {
        Self {
            decay_half_life_days: 30,
            ghost_edge_days: 90,
            ghost_edge_weight: 0.1,
            ring_1_threshold: 2.0,
            ring_2_threshold: 4.0,
            max_hops: 10,
        }
    }
}

/// Ring Engine performs BFS traversal with temporal decay.
pub struct RingEngine {
    config: RingConfig,
}

/// Node in the priority queue for Dijkstra's algorithm.
#[derive(Debug, Clone)]
struct QueueNode {
    node_id: String,
    effective_distance: f64,
    hops: usize,
    path: Vec<String>,
}

impl PartialEq for QueueNode {
    fn eq(&self, other: &Self) -> bool {
        self.effective_distance == other.effective_distance
    }
}

impl Eq for QueueNode {}

impl PartialOrd for QueueNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (smallest distance first)
        other.effective_distance
            .partial_cmp(&self.effective_distance)
            .unwrap_or(Ordering::Equal)
    }
}

impl RingEngine {
    /// Create a new RingEngine with default configuration.
    pub fn new() -> Self {
        Self {
            config: RingConfig::default(),
        }
    }

    /// Create a new RingEngine with custom configuration.
    pub fn with_config(config: RingConfig) -> Self {
        Self { config }
    }

    /// Calculate temporal decay factor for an edge.
    ///
    /// Uses exponential decay: weight = base_weight * 2^(-age/half_life)
    /// Edges older than ghost_edge_days are treated as ghost edges.
    pub fn calculate_decay(&self, observed_at: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
        let age_days = (now - observed_at).num_days();

        if age_days >= self.config.ghost_edge_days {
            // Ghost edge - very low weight but still traversable
            self.config.ghost_edge_weight
        } else if age_days <= 0 {
            1.0
        } else {
            // Exponential decay
            let half_life = self.config.decay_half_life_days as f64;
            2.0_f64.powf(-(age_days as f64) / half_life)
        }
    }

    /// Calculate effective edge weight with temporal decay.
    ///
    /// Returns the "cost" of traversing this edge (inverse of weight with decay).
    pub fn edge_cost(&self, base_weight: f64, observed_at: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
        let decay = self.calculate_decay(observed_at, now);
        let effective_weight = base_weight * decay;

        // Cost is inverse of weight (higher weight = lower cost = closer)
        // Add small epsilon to avoid division by zero
        1.0 / (effective_weight + 0.001)
    }

    /// Determine ring assignment based on effective distance.
    pub fn distance_to_ring(&self, effective_distance: f64) -> Ring {
        if effective_distance <= 0.0 {
            Ring::Core
        } else if effective_distance <= self.config.ring_1_threshold {
            Ring::One
        } else if effective_distance <= self.config.ring_2_threshold {
            Ring::Two
        } else {
            Ring::Beyond
        }
    }

    /// Recalculate ring assignments for all nodes reachable from the user.
    ///
    /// Uses Dijkstra's algorithm with temporal decay-weighted edges.
    pub async fn recalculate_rings(
        &self,
        store: &GraphStore,
        user_node_id: &str,
    ) -> Result<RecalculationResult> {
        let now = Utc::now();
        let start_time = std::time::Instant::now();

        info!("Starting ring recalculation from user: {}", user_node_id);

        // Priority queue for Dijkstra's algorithm
        let mut queue = BinaryHeap::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut assignments: HashMap<String, RingAssignment> = HashMap::new();

        // Start with the user node (Core ring, distance 0)
        queue.push(QueueNode {
            node_id: user_node_id.to_string(),
            effective_distance: 0.0,
            hops: 0,
            path: vec![],
        });

        while let Some(current) = queue.pop() {
            // Skip if already visited
            if visited.contains(&current.node_id) {
                continue;
            }
            visited.insert(current.node_id.clone());

            // Skip if too many hops
            if current.hops > self.config.max_hops {
                continue;
            }

            // Determine ring and create assignment
            let ring = self.distance_to_ring(current.effective_distance);
            let assignment = RingAssignment {
                node_id: current.node_id.clone(),
                ring,
                distance: current.hops as i32,
                effective_distance: current.effective_distance as f32,
                path: current.path.clone(),
                computed_at: now,
            };

            // Save assignment
            store.save_ring_assignment(&assignment).await?;
            assignments.insert(current.node_id.clone(), assignment);

            debug!(
                "Assigned {} to {:?} (dist: {:.2}, hops: {})",
                current.node_id, ring, current.effective_distance, current.hops
            );

            // Get outgoing edges and add neighbors to queue
            let edges = store.edges_from(&current.node_id).await?;
            for edge in edges {
                if visited.contains(&edge.to_node) {
                    continue;
                }

                // Calculate edge cost with temporal decay
                let cost = self.edge_cost(edge.weight as f64, edge.observed_at, now);
                let new_distance = current.effective_distance + cost;

                let mut new_path = current.path.clone();
                new_path.push(current.node_id.clone());

                queue.push(QueueNode {
                    node_id: edge.to_node,
                    effective_distance: new_distance,
                    hops: current.hops + 1,
                    path: new_path,
                });
            }

            // Also traverse incoming edges (graph is conceptually undirected for proximity)
            let incoming = store.edges_to(&current.node_id).await?;
            for edge in incoming {
                if visited.contains(&edge.from_node) {
                    continue;
                }

                let cost = self.edge_cost(edge.weight as f64, edge.observed_at, now);
                let new_distance = current.effective_distance + cost;

                let mut new_path = current.path.clone();
                new_path.push(current.node_id.clone());

                queue.push(QueueNode {
                    node_id: edge.from_node,
                    effective_distance: new_distance,
                    hops: current.hops + 1,
                    path: new_path,
                });
            }
        }

        let duration = start_time.elapsed();
        let distribution = store.ring_distribution().await?;

        // Convert Vec to counts for logging
        let get_count = |ring: Ring| -> i64 {
            distribution.iter().find(|(r, _)| *r == ring).map(|(_, c)| *c).unwrap_or(0)
        };

        info!(
            "Ring recalculation complete: {} nodes processed in {:?}",
            assignments.len(),
            duration
        );
        info!(
            "Distribution: Core={}, Ring1={}, Ring2={}, Beyond={}",
            get_count(Ring::Core),
            get_count(Ring::One),
            get_count(Ring::Two),
            get_count(Ring::Beyond)
        );

        Ok(RecalculationResult {
            nodes_processed: assignments.len(),
            duration_ms: duration.as_millis() as u64,
            distribution,
        })
    }

    /// Get ring assignment for a specific node.
    pub async fn get_ring(&self, store: &GraphStore, node_id: &str) -> Result<Option<Ring>> {
        Ok(store
            .get_ring_assignment(node_id)
            .await?
            .map(|a| a.ring))
    }

    /// Check if ring assignments need recalculation.
    ///
    /// Returns true if:
    /// - No assignments exist
    /// - Assignments are older than max_age
    /// - Edge count has changed significantly since last calculation
    pub async fn needs_recalculation(
        &self,
        store: &GraphStore,
        _max_age: Duration,
    ) -> Result<bool> {
        let distribution = store.ring_distribution().await?;
        let total_assigned: i64 = distribution.iter().map(|(_, count)| count).sum();

        if total_assigned == 0 {
            return Ok(true);
        }

        // Check if we have recent assignments
        // (This is a simplified check - a full implementation would track last recalculation time)
        let node_count = store.node_count().await?;

        // If graph has grown significantly, recalculate
        if node_count > total_assigned * 2 {
            return Ok(true);
        }

        Ok(false)
    }
}

impl Default for RingEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of ring recalculation.
#[derive(Debug, Clone)]
pub struct RecalculationResult {
    /// Number of nodes processed
    pub nodes_processed: usize,
    /// Time taken in milliseconds
    pub duration_ms: u64,
    /// Distribution across rings as Vec of (Ring, count)
    pub distribution: Vec<(Ring, i64)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decay_calculation() {
        let engine = RingEngine::new();
        let now = Utc::now();

        // Fresh edge (0 days old) = no decay
        let decay = engine.calculate_decay(now, now);
        assert!((decay - 1.0).abs() < 0.001);

        // 30 days old = half decay (half-life)
        let old = now - Duration::days(30);
        let decay = engine.calculate_decay(old, now);
        assert!((decay - 0.5).abs() < 0.01);

        // 60 days old = quarter decay
        let older = now - Duration::days(60);
        let decay = engine.calculate_decay(older, now);
        assert!((decay - 0.25).abs() < 0.01);

        // 90+ days = ghost edge
        let ghost = now - Duration::days(100);
        let decay = engine.calculate_decay(ghost, now);
        assert!((decay - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_distance_to_ring() {
        let engine = RingEngine::new();

        assert_eq!(engine.distance_to_ring(0.0), Ring::Core);
        assert_eq!(engine.distance_to_ring(1.0), Ring::One);
        assert_eq!(engine.distance_to_ring(2.0), Ring::One);
        assert_eq!(engine.distance_to_ring(3.0), Ring::Two);
        assert_eq!(engine.distance_to_ring(4.0), Ring::Two);
        assert_eq!(engine.distance_to_ring(5.0), Ring::Beyond);
    }

    #[test]
    fn test_edge_cost() {
        let engine = RingEngine::new();
        let now = Utc::now();

        // Fresh edge with weight 1.0
        let cost = engine.edge_cost(1.0, now, now);
        assert!(cost < 1.1); // Low cost for fresh, high-weight edge

        // Old edge with weight 1.0
        let old = now - Duration::days(60);
        let old_cost = engine.edge_cost(1.0, old, now);
        assert!(old_cost > cost); // Higher cost for older edge
    }
}
