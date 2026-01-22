//! Ring-aware Sync Scheduler for Gravity Well.
//!
//! Manages sync scheduling based on ring proximity:
//! - Core/Ring 1: Hourly, full depth (all changes)
//! - Ring 2: Daily, head-only (check for updates, fetch if changed)
//! - Beyond: On-demand only (never automatic)
//!
//! This is the core value proposition of Gravity Well - keeping relevant
//! context fresh without burning API quota on distant content.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};

use minna_graph::{GraphStore, Ring};

/// Sync depth controls how much data to fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyncDepth {
    /// Full sync: fetch all changes within the time window.
    /// Used for Core and Ring 1 content.
    Full,

    /// Head-only sync: check if content changed, fetch only if updated.
    /// Used for Ring 2 content to reduce API calls.
    HeadOnly,

    /// On-demand: only sync when explicitly requested.
    /// Used for Beyond content.
    OnDemand,
}

impl SyncDepth {
    /// Get the sync depth for a ring.
    pub fn for_ring(ring: Ring) -> Self {
        match ring {
            Ring::Core | Ring::One => SyncDepth::Full,
            Ring::Two => SyncDepth::HeadOnly,
            Ring::Beyond => SyncDepth::OnDemand,
        }
    }

    /// Get the lookback window in days for this depth.
    pub fn lookback_days(&self) -> Option<i64> {
        match self {
            SyncDepth::Full => Some(7),      // 1 week for full syncs
            SyncDepth::HeadOnly => Some(1),  // 1 day for head-only
            SyncDepth::OnDemand => None,     // No automatic lookback
        }
    }
}

/// Configuration for the sync scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// How often to sync Core/Ring 1 content (default: 1 hour).
    pub ring1_interval: Duration,

    /// How often to sync Ring 2 content (default: 24 hours).
    pub ring2_interval: Duration,

    /// Maximum API calls per hour across all providers.
    pub hourly_budget: u32,

    /// Maximum concurrent syncs.
    pub max_concurrent: usize,

    /// Whether to enable automatic scheduling.
    pub enabled: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            ring1_interval: Duration::from_secs(60 * 60),      // 1 hour
            ring2_interval: Duration::from_secs(24 * 60 * 60), // 24 hours
            hourly_budget: 1000,
            max_concurrent: 3,
            enabled: true,
        }
    }
}

/// A scheduled sync task.
#[derive(Debug, Clone)]
pub struct ScheduledSync {
    /// Provider to sync (e.g., "slack", "linear").
    pub provider: String,

    /// Sync depth for this task.
    pub depth: SyncDepth,

    /// Ring that triggered this sync.
    pub ring: Ring,

    /// Entities to sync (node IDs). Empty means sync all for this provider.
    pub entity_ids: Vec<String>,

    /// When this sync was scheduled.
    pub scheduled_at: DateTime<Utc>,

    /// Priority (lower = higher priority). Core=0, Ring1=1, Ring2=2.
    pub priority: u8,
}

impl ScheduledSync {
    /// Create a new scheduled sync for a ring.
    pub fn for_ring(provider: &str, ring: Ring) -> Self {
        Self {
            provider: provider.to_string(),
            depth: SyncDepth::for_ring(ring),
            ring,
            entity_ids: Vec::new(),
            scheduled_at: Utc::now(),
            priority: match ring {
                Ring::Core => 0,
                Ring::One => 1,
                Ring::Two => 2,
                Ring::Beyond => 3,
            },
        }
    }

    /// Create an on-demand sync for specific entities.
    pub fn on_demand(provider: &str, entity_ids: Vec<String>) -> Self {
        Self {
            provider: provider.to_string(),
            depth: SyncDepth::Full,
            ring: Ring::Beyond,
            entity_ids,
            scheduled_at: Utc::now(),
            priority: 0, // On-demand is high priority (user requested)
        }
    }
}

/// Tracks API usage for budget management.
#[derive(Debug, Default)]
pub struct SyncBudget {
    /// Calls made per provider in the current hour.
    calls_this_hour: HashMap<String, u32>,

    /// When the current hour started.
    hour_start: Option<Instant>,

    /// Total calls made this hour across all providers.
    total_this_hour: u32,
}

impl SyncBudget {
    /// Create a new budget tracker.
    pub fn new() -> Self {
        Self {
            calls_this_hour: HashMap::new(),
            hour_start: Some(Instant::now()),
            total_this_hour: 0,
        }
    }

    /// Record API calls made.
    pub fn record_calls(&mut self, provider: &str, count: u32) {
        self.maybe_reset_hour();
        *self.calls_this_hour.entry(provider.to_string()).or_insert(0) += count;
        self.total_this_hour += count;
    }

    /// Check if we have budget remaining.
    pub fn has_budget(&mut self, limit: u32) -> bool {
        self.maybe_reset_hour();
        self.total_this_hour < limit
    }

    /// Get remaining budget for this hour.
    pub fn remaining(&mut self, limit: u32) -> u32 {
        self.maybe_reset_hour();
        limit.saturating_sub(self.total_this_hour)
    }

    /// Get calls made by a specific provider this hour.
    pub fn calls_for_provider(&self, provider: &str) -> u32 {
        *self.calls_this_hour.get(provider).unwrap_or(&0)
    }

    /// Reset counters if an hour has passed.
    fn maybe_reset_hour(&mut self) {
        if let Some(start) = self.hour_start {
            if start.elapsed() > Duration::from_secs(3600) {
                self.calls_this_hour.clear();
                self.total_this_hour = 0;
                self.hour_start = Some(Instant::now());
            }
        } else {
            self.hour_start = Some(Instant::now());
        }
    }
}

/// The main sync scheduler.
///
/// Coordinates ring-aware sync scheduling to keep relevant content fresh
/// while minimizing API usage for distant content.
pub struct SyncScheduler {
    config: SchedulerConfig,
    budget: SyncBudget,

    /// Last sync time per provider per ring.
    last_sync: HashMap<(String, Ring), Instant>,

    /// Pending syncs, ordered by priority.
    pending: Vec<ScheduledSync>,

    /// Providers that are currently syncing.
    in_progress: HashSet<String>,
}

impl SyncScheduler {
    /// Create a new scheduler with default configuration.
    pub fn new() -> Self {
        Self::with_config(SchedulerConfig::default())
    }

    /// Create a scheduler with custom configuration.
    pub fn with_config(config: SchedulerConfig) -> Self {
        Self {
            config,
            budget: SyncBudget::new(),
            last_sync: HashMap::new(),
            pending: Vec::new(),
            in_progress: HashSet::new(),
        }
    }

    /// Check if scheduling is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the scheduler configuration.
    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    /// Update scheduler configuration.
    pub fn set_config(&mut self, config: SchedulerConfig) {
        self.config = config;
    }

    /// Schedule syncs based on ring assignments.
    ///
    /// Examines the graph to determine which providers have content in each ring,
    /// then schedules syncs according to ring-based intervals.
    pub async fn schedule_from_rings(
        &mut self,
        graph: &GraphStore,
        providers: &[&str],
    ) -> Result<Vec<ScheduledSync>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let mut scheduled = Vec::new();
        let now = Instant::now();

        // Get ring distribution
        let distribution = graph.ring_distribution().await?;
        let has_ring_data = !distribution.is_empty();

        for &provider in providers {
            // Check Ring 1 (hourly)
            if self.should_sync(provider, Ring::One, now) {
                let sync = ScheduledSync::for_ring(provider, Ring::One);
                if !self.is_duplicate(&sync) {
                    info!("[SCHEDULER] Queueing Ring 1 sync for {}", provider);
                    scheduled.push(sync.clone());
                    self.pending.push(sync);
                }
            }

            // Check Ring 2 (daily) - only if we have ring data
            if has_ring_data && self.should_sync(provider, Ring::Two, now) {
                let sync = ScheduledSync::for_ring(provider, Ring::Two);
                if !self.is_duplicate(&sync) {
                    info!("[SCHEDULER] Queueing Ring 2 sync for {}", provider);
                    scheduled.push(sync.clone());
                    self.pending.push(sync);
                }
            }
        }

        // Sort pending by priority
        self.pending.sort_by_key(|s| s.priority);

        Ok(scheduled)
    }

    /// Check if a provider/ring combination needs syncing.
    fn should_sync(&self, provider: &str, ring: Ring, now: Instant) -> bool {
        let key = (provider.to_string(), ring);
        let interval = match ring {
            Ring::Core | Ring::One => self.config.ring1_interval,
            Ring::Two => self.config.ring2_interval,
            Ring::Beyond => return false, // Never auto-sync Beyond
        };

        match self.last_sync.get(&key) {
            Some(last) => now.duration_since(*last) >= interval,
            None => true, // Never synced, should sync
        }
    }

    /// Check if a sync task is already pending or in progress.
    fn is_duplicate(&self, sync: &ScheduledSync) -> bool {
        if self.in_progress.contains(&sync.provider) {
            return true;
        }

        self.pending.iter().any(|p| {
            p.provider == sync.provider && p.ring == sync.ring
        })
    }

    /// Get the next sync task to execute.
    ///
    /// Returns None if:
    /// - No pending syncs
    /// - Budget exhausted
    /// - Max concurrent syncs reached
    pub fn next_sync(&mut self) -> Option<ScheduledSync> {
        if !self.config.enabled {
            return None;
        }

        if self.in_progress.len() >= self.config.max_concurrent {
            debug!("[SCHEDULER] Max concurrent syncs reached");
            return None;
        }

        if !self.budget.has_budget(self.config.hourly_budget) {
            warn!("[SCHEDULER] Hourly budget exhausted");
            return None;
        }

        // Find next sync that isn't already in progress
        let idx = self.pending.iter().position(|s| {
            !self.in_progress.contains(&s.provider)
        })?;

        let sync = self.pending.remove(idx);
        self.in_progress.insert(sync.provider.clone());

        Some(sync)
    }

    /// Mark a sync as complete and record API usage.
    pub fn complete_sync(&mut self, provider: &str, ring: Ring, api_calls: u32) {
        self.in_progress.remove(provider);
        self.last_sync.insert((provider.to_string(), ring), Instant::now());
        self.budget.record_calls(provider, api_calls);

        info!(
            "[SCHEDULER] Completed {} sync for {} ({} API calls, {} remaining)",
            format!("{:?}", ring),
            provider,
            api_calls,
            self.budget.remaining(self.config.hourly_budget)
        );
    }

    /// Mark a sync as failed.
    pub fn fail_sync(&mut self, provider: &str) {
        self.in_progress.remove(provider);
        warn!("[SCHEDULER] Sync failed for {}", provider);
    }

    /// Queue an on-demand sync (user requested).
    ///
    /// On-demand syncs bypass ring-based scheduling and have high priority.
    pub fn queue_on_demand(&mut self, provider: &str, entity_ids: Option<Vec<String>>) {
        let sync = match entity_ids {
            Some(ids) if !ids.is_empty() => ScheduledSync::on_demand(provider, ids),
            _ => {
                let mut sync = ScheduledSync::for_ring(provider, Ring::One);
                sync.priority = 0; // High priority for on-demand
                sync
            }
        };

        info!("[SCHEDULER] Queueing on-demand sync for {}", provider);
        self.pending.insert(0, sync); // Insert at front
    }

    /// Get pending sync count.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Get in-progress sync count.
    pub fn in_progress_count(&self) -> usize {
        self.in_progress.len()
    }

    /// Get budget status.
    pub fn budget_status(&mut self) -> (u32, u32) {
        let remaining = self.budget.remaining(self.config.hourly_budget);
        (self.config.hourly_budget - remaining, self.config.hourly_budget)
    }

    /// Clear all pending syncs.
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// Get statistics about sync scheduling.
    pub fn stats(&mut self) -> SchedulerStats {
        let (used, total) = self.budget_status();
        SchedulerStats {
            pending: self.pending.len(),
            in_progress: self.in_progress.len(),
            budget_used: used,
            budget_total: total,
            last_sync_times: self.last_sync.iter()
                .map(|((p, r), t)| {
                    (p.clone(), *r, t.elapsed().as_secs())
                })
                .collect(),
        }
    }
}

impl Default for SyncScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about scheduler state.
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    /// Number of pending sync tasks.
    pub pending: usize,
    /// Number of syncs currently in progress.
    pub in_progress: usize,
    /// API calls used this hour.
    pub budget_used: u32,
    /// Total hourly budget.
    pub budget_total: u32,
    /// Last sync times: (provider, ring, seconds_ago).
    pub last_sync_times: Vec<(String, Ring, u64)>,
}

/// Determines sync parameters based on entity's ring assignment.
pub struct SyncPlanner;

impl SyncPlanner {
    /// Plan sync for entities in a specific ring.
    ///
    /// Returns (lookback_days, mode) parameters for the sync call.
    pub fn plan_for_ring(ring: Ring) -> (Option<i64>, Option<&'static str>) {
        let depth = SyncDepth::for_ring(ring);
        let lookback = depth.lookback_days();
        let mode = match depth {
            SyncDepth::Full => Some("full"),
            SyncDepth::HeadOnly => Some("head"),
            SyncDepth::OnDemand => None,
        };
        (lookback, mode)
    }

    /// Get entities to sync for a provider based on ring assignments.
    ///
    /// Returns entities grouped by ring for prioritized syncing.
    pub async fn get_entities_by_ring(
        graph: &GraphStore,
        provider: &str,
    ) -> Result<HashMap<Ring, Vec<String>>> {
        let mut by_ring: HashMap<Ring, Vec<String>> = HashMap::new();

        // Get all nodes for this provider that have ring assignments
        // This is a simplified version - a full implementation would join
        // graph_nodes with ring_assignments
        for ring in [Ring::Core, Ring::One, Ring::Two, Ring::Beyond] {
            let nodes = graph.nodes_in_ring(ring).await?;
            let provider_nodes: Vec<String> = nodes.into_iter()
                .filter(|id| id.contains(&format!(":{}:", provider)))
                .collect();

            if !provider_nodes.is_empty() {
                by_ring.insert(ring, provider_nodes);
            }
        }

        Ok(by_ring)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_depth_for_ring() {
        assert_eq!(SyncDepth::for_ring(Ring::Core), SyncDepth::Full);
        assert_eq!(SyncDepth::for_ring(Ring::One), SyncDepth::Full);
        assert_eq!(SyncDepth::for_ring(Ring::Two), SyncDepth::HeadOnly);
        assert_eq!(SyncDepth::for_ring(Ring::Beyond), SyncDepth::OnDemand);
    }

    #[test]
    fn test_sync_budget() {
        let mut budget = SyncBudget::new();
        assert!(budget.has_budget(100));

        budget.record_calls("slack", 50);
        assert!(budget.has_budget(100));
        assert_eq!(budget.remaining(100), 50);

        budget.record_calls("linear", 50);
        assert!(!budget.has_budget(100));
        assert_eq!(budget.remaining(100), 0);
    }

    #[test]
    fn test_scheduled_sync_priority() {
        let core = ScheduledSync::for_ring("slack", Ring::Core);
        let ring1 = ScheduledSync::for_ring("slack", Ring::One);
        let ring2 = ScheduledSync::for_ring("slack", Ring::Two);
        let on_demand = ScheduledSync::on_demand("slack", vec!["id1".to_string()]);

        assert_eq!(core.priority, 0);
        assert_eq!(ring1.priority, 1);
        assert_eq!(ring2.priority, 2);
        assert_eq!(on_demand.priority, 0); // On-demand is high priority
    }

    #[test]
    fn test_scheduler_config_defaults() {
        let config = SchedulerConfig::default();
        assert_eq!(config.ring1_interval, Duration::from_secs(3600));
        assert_eq!(config.ring2_interval, Duration::from_secs(86400));
        assert_eq!(config.hourly_budget, 1000);
        assert!(config.enabled);
    }

    #[test]
    fn test_scheduler_next_sync() {
        let mut scheduler = SyncScheduler::new();

        // No pending syncs
        assert!(scheduler.next_sync().is_none());

        // Add a sync
        scheduler.queue_on_demand("slack", None);
        assert_eq!(scheduler.pending_count(), 1);

        // Get it
        let sync = scheduler.next_sync().unwrap();
        assert_eq!(sync.provider, "slack");
        assert_eq!(scheduler.in_progress_count(), 1);

        // Can't get another for same provider
        scheduler.queue_on_demand("slack", None);
        assert!(scheduler.next_sync().is_none()); // slack still in progress

        // Complete it
        scheduler.complete_sync("slack", Ring::One, 10);
        assert_eq!(scheduler.in_progress_count(), 0);

        // Now can get the next one
        assert!(scheduler.next_sync().is_some());
    }

    #[test]
    fn test_sync_planner() {
        let (days, mode) = SyncPlanner::plan_for_ring(Ring::One);
        assert_eq!(days, Some(7));
        assert_eq!(mode, Some("full"));

        let (days, mode) = SyncPlanner::plan_for_ring(Ring::Two);
        assert_eq!(days, Some(1));
        assert_eq!(mode, Some("head"));

        let (days, mode) = SyncPlanner::plan_for_ring(Ring::Beyond);
        assert_eq!(days, None);
        assert_eq!(mode, None);
    }
}
