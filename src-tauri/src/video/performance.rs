#![allow(dead_code)]

// ========================================================================
// Performance Profiling Utilities
// ========================================================================
//
// Production-ready performance monitoring for auto-edit operations.
//
// Performance Targets (from AUTO_EDIT_GUIDE.md):
// - <30 seconds per minute of output video
// - 60s video: target <30s total
// - 120s video: target <60s total
// - 180s video: target <90s total

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Performance metrics for a single auto-edit operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Total elapsed time
    pub total_duration: Duration,

    /// Time spent on each stage
    pub stage_durations: HashMap<String, Duration>,

    /// Number of clips processed
    pub clips_processed: usize,

    /// Target video duration in seconds
    pub target_duration_seconds: u32,

    /// Output video file size in bytes
    pub output_file_size: u64,

    /// Performance rating (1-5 stars)
    pub rating: PerformanceRating,

    /// Whether performance target was met
    pub target_met: bool,

    /// Additional metadata
    pub metadata: PerformanceMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetadata {
    /// FFmpeg version
    pub ffmpeg_version: Option<String>,

    /// CPU model
    pub cpu_model: Option<String>,

    /// Number of CPU cores
    pub cpu_cores: usize,

    /// Total system memory in MB
    pub total_memory_mb: u64,

    /// Available disk space in MB
    pub available_disk_mb: u64,

    /// Hardware acceleration used
    pub hardware_accel: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PerformanceRating {
    /// Excellent (<50% of target)
    Excellent,
    /// Good (50-75% of target)
    Good,
    /// Acceptable (75-100% of target)
    Acceptable,
    /// Slow (100-150% of target)
    Slow,
    /// Poor (>150% of target)
    Poor,
}

impl PerformanceRating {
    /// Get star rating (1-5)
    pub fn stars(&self) -> u8 {
        match self {
            Self::Excellent => 5,
            Self::Good => 4,
            Self::Acceptable => 3,
            Self::Slow => 2,
            Self::Poor => 1,
        }
    }

    /// Get emoji representation
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Excellent => "⚡",
            Self::Good => "✅",
            Self::Acceptable => "👍",
            Self::Slow => "⚠️",
            Self::Poor => "🐌",
        }
    }
}

/// Performance profiler for tracking stage durations
pub struct PerformanceProfiler {
    start_time: Instant,
    stage_timers: HashMap<String, Instant>,
    stage_durations: HashMap<String, Duration>,
    target_duration_seconds: u32,
    clips_processed: usize,
}

impl PerformanceProfiler {
    /// Create a new profiler
    pub fn new(target_duration_seconds: u32) -> Self {
        info!(
            "Starting performance profiling for {}s video",
            target_duration_seconds
        );

        Self {
            start_time: Instant::now(),
            stage_timers: HashMap::new(),
            stage_durations: HashMap::new(),
            target_duration_seconds,
            clips_processed: 0,
        }
    }

    /// Start timing a stage
    pub fn start_stage(&mut self, stage_name: impl Into<String>) {
        let stage_name = stage_name.into();
        info!("Starting stage: {}", stage_name);
        self.stage_timers.insert(stage_name, Instant::now());
    }

    /// End timing a stage
    pub fn end_stage(&mut self, stage_name: impl Into<String>) {
        let stage_name = stage_name.into();

        if let Some(start_time) = self.stage_timers.remove(&stage_name) {
            let duration = start_time.elapsed();
            info!(
                "Stage {} completed in {:.2}s",
                stage_name,
                duration.as_secs_f64()
            );
            self.stage_durations.insert(stage_name, duration);
        } else {
            warn!("Stage {} was never started", stage_name);
        }
    }

    /// Set number of clips processed
    pub fn set_clips_processed(&mut self, count: usize) {
        self.clips_processed = count;
    }

    /// Calculate performance rating
    fn calculate_rating(actual_seconds: f64, target_seconds: f64) -> PerformanceRating {
        let ratio = actual_seconds / target_seconds;

        if ratio <= 0.5 {
            PerformanceRating::Excellent
        } else if ratio <= 0.75 {
            PerformanceRating::Good
        } else if ratio <= 1.0 {
            PerformanceRating::Acceptable
        } else if ratio <= 1.5 {
            PerformanceRating::Slow
        } else {
            PerformanceRating::Poor
        }
    }

    /// Collect system metadata
    fn collect_metadata() -> PerformanceMetadata {
        use crate::utils::ffmpeg;
        use sysinfo::{Disks, System};

        let mut sys = System::new_all();
        sys.refresh_all();

        let cpu_model = sys.cpus().first().map(|cpu| cpu.brand().to_string());

        let cpu_cores = num_cpus::get();
        let total_memory_mb = sys.total_memory() / 1024 / 1024;

        // Get available disk space on primary disk
        let disks = Disks::new_with_refreshed_list();
        let available_disk_mb = disks
            .first()
            .map(|disk| disk.available_space() / 1024 / 1024)
            .unwrap_or(0);

        // Query FFmpeg version and hardware acceleration
        let (ffmpeg_version, hardware_accel) = match ffmpeg::get_ffmpeg_info() {
            Ok(info) => {
                let hw_encoders: Vec<String> = info
                    .available_encoders
                    .iter()
                    .filter(|e| e.is_hardware_accelerated && e.can_encode)
                    .map(|e| e.name.clone())
                    .collect();

                let hw_accel = if !hw_encoders.is_empty() {
                    Some(hw_encoders.join(", "))
                } else {
                    None
                };

                (Some(info.version), hw_accel)
            }
            Err(_) => (None, None),
        };

        PerformanceMetadata {
            ffmpeg_version,
            cpu_model,
            cpu_cores,
            total_memory_mb,
            available_disk_mb,
            hardware_accel,
        }
    }

    /// Finalize profiling and generate metrics
    pub fn finalize(self, output_file_size: u64) -> PerformanceMetrics {
        let total_duration = self.start_time.elapsed();
        let actual_seconds = total_duration.as_secs_f64();

        // Performance target: 30 seconds per minute of output
        let target_seconds = (self.target_duration_seconds as f64 / 60.0) * 30.0;

        let rating = Self::calculate_rating(actual_seconds, target_seconds);
        let target_met = actual_seconds <= target_seconds;

        let metrics = PerformanceMetrics {
            total_duration,
            stage_durations: self.stage_durations,
            clips_processed: self.clips_processed,
            target_duration_seconds: self.target_duration_seconds,
            output_file_size,
            rating,
            target_met,
            metadata: Self::collect_metadata(),
        };

        // Log performance summary
        info!("===============================================");
        info!("Performance Summary");
        info!("===============================================");
        info!("Target Duration: {}s", self.target_duration_seconds);
        info!("Processing Time: {:.2}s", actual_seconds);
        info!("Performance Target: {:.2}s", target_seconds);
        info!(
            "Target Met: {}",
            if target_met { "YES ✅" } else { "NO ❌" }
        );
        info!(
            "Rating: {:?} {} ({} stars)",
            rating,
            rating.emoji(),
            rating.stars()
        );
        info!("Clips Processed: {}", self.clips_processed);
        info!(
            "Output Size: {:.2} MB",
            output_file_size as f64 / 1024.0 / 1024.0
        );
        info!("===============================================");

        if !target_met {
            warn!("Performance target was not met! Consider optimization.");
        }

        metrics
    }

    /// Get current elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get estimated total time based on current progress
    pub fn estimate_total(&self, progress_percent: f64) -> Duration {
        if progress_percent <= 0.0 {
            return Duration::from_secs(0);
        }

        let elapsed = self.elapsed().as_secs_f64();
        let estimated_total = elapsed / (progress_percent / 100.0);
        Duration::from_secs_f64(estimated_total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_profiler_basic_flow() {
        let mut profiler = PerformanceProfiler::new(60);

        profiler.start_stage("test_stage");
        thread::sleep(Duration::from_millis(100));
        profiler.end_stage("test_stage");

        profiler.set_clips_processed(5);

        let metrics = profiler.finalize(1024 * 1024 * 10); // 10 MB

        assert_eq!(metrics.clips_processed, 5);
        assert!(metrics.stage_durations.contains_key("test_stage"));
        assert!(metrics.total_duration >= Duration::from_millis(100));
    }

    #[test]
    fn test_performance_rating_calculation() {
        // Target: 30s for 60s video
        let target = 30.0;

        assert_eq!(
            PerformanceProfiler::calculate_rating(10.0, target),
            PerformanceRating::Excellent
        );

        assert_eq!(
            PerformanceProfiler::calculate_rating(20.0, target),
            PerformanceRating::Good
        );

        assert_eq!(
            PerformanceProfiler::calculate_rating(25.0, target),
            PerformanceRating::Acceptable
        );

        assert_eq!(
            PerformanceProfiler::calculate_rating(40.0, target),
            PerformanceRating::Slow
        );

        assert_eq!(
            PerformanceProfiler::calculate_rating(50.0, target),
            PerformanceRating::Poor
        );
    }

    #[test]
    fn test_rating_stars() {
        assert_eq!(PerformanceRating::Excellent.stars(), 5);
        assert_eq!(PerformanceRating::Good.stars(), 4);
        assert_eq!(PerformanceRating::Acceptable.stars(), 3);
        assert_eq!(PerformanceRating::Slow.stars(), 2);
        assert_eq!(PerformanceRating::Poor.stars(), 1);
    }

    #[test]
    fn test_estimate_total() {
        let profiler = PerformanceProfiler::new(60);
        thread::sleep(Duration::from_millis(100));

        // 50% progress -> expect ~200ms total
        let estimated = profiler.estimate_total(50.0);
        assert!(estimated >= Duration::from_millis(180));
        assert!(estimated <= Duration::from_millis(220));
    }
}
