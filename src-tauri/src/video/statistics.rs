use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info};

/// Simple counter-based statistics - remove over-engineering
pub struct ClipStatistics {
    pub total_clips: AtomicU64,
    pub successful_clips: AtomicU64,
    pub failed_clips: AtomicU64,
}

impl Default for ClipStatistics {
    fn default() -> Self {
        Self {
            total_clips: AtomicU64::new(0),
            successful_clips: AtomicU64::new(0),
            failed_clips: AtomicU64::new(0),
        }
    }
}

impl ClipStatistics {
    /// Record successful clip
    pub fn record_success(&self) {
        self.total_clips.fetch_add(1, Ordering::Relaxed);
        self.successful_clips.fetch_add(1, Ordering::Relaxed);
        debug!("Clip success recorded");
    }

    /// Record failed clip
    pub fn record_failure(&self) {
        self.total_clips.fetch_add(1, Ordering::Relaxed);
        self.failed_clips.fetch_add(1, Ordering::Relaxed);
        debug!("Clip failure recorded");
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.total_clips.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let successful = self.successful_clips.load(Ordering::Relaxed);
        (successful as f64 / total as f64) * 100.0
    }

    /// Get current counts
    pub fn get_counts(&self) -> (u64, u64, u64) {
        (
            self.total_clips.load(Ordering::Relaxed),
            self.successful_clips.load(Ordering::Relaxed),
            self.failed_clips.load(Ordering::Relaxed),
        )
    }

    /// Reset all statistics
    pub fn reset(&self) {
        self.total_clips.store(0, Ordering::Relaxed);
        self.successful_clips.store(0, Ordering::Relaxed);
        self.failed_clips.store(0, Ordering::Relaxed);
        info!("Statistics reset");
    }
}

/// Global statistics instance
static GLOBAL_STATS: std::sync::OnceLock<ClipStatistics> = std::sync::OnceLock::new();

pub fn get_global_stats() -> &'static ClipStatistics {
    GLOBAL_STATS.get_or_init(ClipStatistics::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_statistics() {
        let stats = ClipStatistics::default();

        // Initial state
        assert_eq!(stats.get_counts(), (0, 0, 0));
        assert_eq!(stats.success_rate(), 0.0);

        // Add successes
        stats.record_success();
        stats.record_success();
        let (total, successful, failed) = stats.get_counts();
        assert_eq!(total, 2);
        assert_eq!(successful, 2);
        assert_eq!(failed, 0);
        assert_eq!(stats.success_rate(), 100.0);

        // Add failure
        stats.record_failure();
        let (total, successful, failed) = stats.get_counts();
        assert_eq!(total, 3);
        assert_eq!(successful, 2);
        assert_eq!(failed, 1);
        assert!((stats.success_rate() - 66.666666).abs() < 0.001);

        // Reset
        stats.reset();
        assert_eq!(stats.get_counts(), (0, 0, 0));
    }

    #[test]
    fn test_global_stats() {
        let stats1 = get_global_stats();
        let stats2 = get_global_stats();

        // Should be the same instance
        assert!(std::ptr::eq(stats1, stats2));

        // Test operations
        stats1.record_success();
        let (total, successful, _) = stats2.get_counts();
        assert_eq!(total, 1);
        assert_eq!(successful, 1);
    }
}
