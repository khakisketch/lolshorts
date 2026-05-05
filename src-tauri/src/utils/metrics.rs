use serde::{Deserialize, Serialize};
/// Production performance metrics collection and monitoring
///
/// Tracks system health, resource utilization, and recording performance
/// for production observability and alerting.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Performance metrics for FFmpeg recording process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMetrics {
    /// Current frames per second (target: 60)
    pub fps: f64,

    /// Frame drops in current segment
    pub frame_drops: u32,

    /// Current bitrate in Kbps
    pub bitrate_kbps: u32,

    /// FFmpeg process CPU usage (0.0-100.0)
    pub cpu_percent: f64,

    /// FFmpeg process memory usage in MB
    pub memory_mb: f64,

    /// Number of segments in buffer
    pub buffer_segments: u32,

    /// Total disk space used by buffer in MB
    pub buffer_size_mb: f64,

    /// Timestamp of last update (excluded from serialization)
    #[serde(skip, default = "Instant::now")]
    pub last_updated: Instant,
}

impl Default for RecordingMetrics {
    fn default() -> Self {
        Self {
            fps: 60.0,
            frame_drops: 0,
            bitrate_kbps: 0,
            cpu_percent: 0.0,
            memory_mb: 0.0,
            buffer_segments: 0,
            buffer_size_mb: 0.0,
            last_updated: Instant::now(),
        }
    }
}

/// System resource metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// Overall CPU usage (0.0-100.0)
    pub total_cpu_percent: f64,

    /// Available RAM in GB
    pub available_ram_gb: f64,

    /// Disk space available for recordings in GB
    pub available_disk_gb: f64,

    /// GPU utilization if available (0.0-100.0)
    pub gpu_percent: Option<f64>,

    /// GPU memory usage in MB if available
    pub gpu_memory_mb: Option<f64>,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            total_cpu_percent: 0.0,
            available_ram_gb: 0.0,
            available_disk_gb: 0.0,
            gpu_percent: None,
            gpu_memory_mb: None,
        }
    }
}

/// Health status thresholds
#[derive(Debug, Clone)]
pub struct HealthThresholds {
    /// Minimum FPS before warning (default: 55)
    pub min_fps: f32,

    /// Maximum frame drops per segment (default: 10)
    pub max_frame_drops: u64,

    /// Maximum CPU usage before warning (default: 80%)
    pub max_cpu_percent: f32,

    /// Maximum memory usage in MB (default: 2048)
    pub max_memory_mb: f32,

    /// Maximum buffer size in MB (default: 5000)
    pub max_buffer_mb: f32,

    /// Minimum available disk space in GB (default: 5)
    pub min_disk_gb: f32,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            min_fps: 55.0,
            max_frame_drops: 10,
            max_cpu_percent: 80.0,
            max_memory_mb: 2048.0,
            max_buffer_mb: 5000.0,
            min_disk_gb: 5.0,
        }
    }
}

/// Health status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// All metrics within normal range
    Healthy,

    /// Some metrics approaching thresholds
    Warning,

    /// Critical metrics exceeded
    Critical,
}

/// Basic GPU information structure
#[derive(Debug, Clone)]
struct GpuInfo {
    utilization: f64,
    memory_usage_mb: f64,
    #[allow(dead_code)]
    temperature_celsius: f64,
}

/// Metrics collector and health monitor
pub struct MetricsCollector {
    recording_metrics: Arc<RwLock<RecordingMetrics>>,
    system_metrics: Arc<RwLock<SystemMetrics>>,
    thresholds: HealthThresholds,
    sysinfo: Arc<RwLock<sysinfo::System>>,
    recording_dir: PathBuf,
    health_check_count: Arc<RwLock<u64>>,
}

impl MetricsCollector {
    pub fn new(thresholds: HealthThresholds, recording_dir: PathBuf) -> Self {
        Self {
            recording_metrics: Arc::new(RwLock::new(RecordingMetrics::default())),
            system_metrics: Arc::new(RwLock::new(SystemMetrics::default())),
            thresholds,
            sysinfo: Arc::new(RwLock::new(sysinfo::System::new_all())),
            recording_dir,
            health_check_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Get current recording metrics
    pub async fn get_recording_metrics(&self) -> RecordingMetrics {
        self.recording_metrics.read().await.clone()
    }

    /// Get current system metrics
    pub async fn get_system_metrics(&self) -> SystemMetrics {
        self.system_metrics.read().await.clone()
    }

    /// Update recording metrics
    pub async fn update_recording_metrics(&self, metrics: RecordingMetrics) {
        let mut current = self.recording_metrics.write().await;
        *current = metrics;
    }

    /// Update buffer metrics
    pub async fn update_buffer_metrics(&self, segments: usize, size_mb: f32) {
        let mut metrics = self.recording_metrics.write().await;
        metrics.buffer_segments = segments as u32;
        metrics.buffer_size_mb = size_mb as f64;
        metrics.last_updated = Instant::now();
    }

    /// Update system metrics from sysinfo
    pub async fn update_system_metrics(&self) {
        let mut sys = self.sysinfo.write().await;

        // Refresh system info
        sys.refresh_cpu_all();
        sys.refresh_memory();

        let mut metrics = self.system_metrics.write().await;

        // Calculate average CPU usage
        let cpu_usage: f32 =
            sys.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32;

        metrics.total_cpu_percent = cpu_usage as f64;
        metrics.available_ram_gb = sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;

        // Check disk space for recording directory
        if let Some(available_gb) = self.get_disk_space(&self.recording_dir) {
            metrics.available_disk_gb = available_gb;
        } else {
            metrics.available_disk_gb = -1.0; // Unknown — UI should display "unknown"
        }

        // Add basic GPU metrics if available (Windows DirectX)
        #[cfg(target_os = "windows")]
        {
            // Try to get GPU information using Windows APIs
            if let Ok(gpu_info) = self.get_gpu_info() {
                metrics.gpu_percent = Some(gpu_info.utilization);
                metrics.gpu_memory_mb = Some(gpu_info.memory_usage_mb);
            }
        }
    }

    /// Get GPU information using Windows APIs
    #[cfg(target_os = "windows")]
    fn get_gpu_info(&self) -> anyhow::Result<GpuInfo> {
        // For now, return default values
        // In a real implementation, you would use Windows Performance Counters
        // or DirectX APIs to get actual GPU metrics
        Ok(GpuInfo {
            utilization: 0.0,
            memory_usage_mb: 0.0,
            temperature_celsius: 0.0,
        })
    }

    /// Check health status against thresholds
    pub async fn check_health(&self) -> HealthStatus {
        let rec_metrics = self.recording_metrics.read().await;
        let sys_metrics = self.system_metrics.read().await;

        // Critical checks
        if rec_metrics.fps < (self.thresholds.min_fps - 10.0).into() {
            warn!("Critical: FPS too low: {:.1}", rec_metrics.fps);
            return HealthStatus::Critical;
        }

        if rec_metrics.cpu_percent > 95.0 {
            warn!(
                "Critical: CPU usage too high: {:.1}%",
                rec_metrics.cpu_percent
            );
            return HealthStatus::Critical;
        }

        if sys_metrics.available_disk_gb < 1.0 {
            warn!(
                "Critical: Disk space very low: {:.2} GB",
                sys_metrics.available_disk_gb
            );
            return HealthStatus::Critical;
        }

        // Warning checks
        if rec_metrics.fps < self.thresholds.min_fps.into() {
            warn!("Warning: FPS below threshold: {:.1}", rec_metrics.fps);
            return HealthStatus::Warning;
        }

        if rec_metrics.frame_drops as u64 > self.thresholds.max_frame_drops {
            warn!("Warning: Too many frame drops: {}", rec_metrics.frame_drops);
            return HealthStatus::Warning;
        }

        if rec_metrics.cpu_percent > self.thresholds.max_cpu_percent.into() {
            warn!("Warning: High CPU usage: {:.1}%", rec_metrics.cpu_percent);
            return HealthStatus::Warning;
        }

        if rec_metrics.memory_mb > self.thresholds.max_memory_mb.into() {
            warn!(
                "Warning: High memory usage: {:.1} MB",
                rec_metrics.memory_mb
            );
            return HealthStatus::Warning;
        }

        if rec_metrics.buffer_size_mb > self.thresholds.max_buffer_mb.into() {
            warn!(
                "Warning: Buffer size too large: {:.1} MB",
                rec_metrics.buffer_size_mb
            );
            return HealthStatus::Warning;
        }

        if sys_metrics.available_disk_gb < self.thresholds.min_disk_gb.into() {
            warn!(
                "Warning: Low disk space: {:.2} GB",
                sys_metrics.available_disk_gb
            );
            return HealthStatus::Warning;
        }

        HealthStatus::Healthy
    }

    /// Start background metrics collection
    ///
    /// Returns a handle to stop the collection task
    pub fn start_background_collection(
        self: Arc<Self>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                // Update system metrics
                self.update_system_metrics().await;

                // Check health and log warnings
                let health = self.check_health().await;

                match health {
                    HealthStatus::Healthy => {
                        // Log every 10 intervals (reduce log spam)
                        let mut count = self.health_check_count.write().await;
                        *count += 1;

                        if *count % 10 == 0 {
                            info!("✅ System health check #{}: All systems healthy", *count);
                        }
                    }
                    HealthStatus::Warning => {
                        let rec = self.get_recording_metrics().await;
                        let sys = self.get_system_metrics().await;
                        warn!(
                            "Performance warning - FPS: {:.1}, CPU: {:.1}%, Mem: {:.1}MB, Disk: {:.2}GB",
                            rec.fps, rec.cpu_percent, rec.memory_mb, sys.available_disk_gb
                        );
                    }
                    HealthStatus::Critical => {
                        let rec = self.get_recording_metrics().await;
                        let sys = self.get_system_metrics().await;
                        warn!(
                            "CRITICAL performance issue - FPS: {:.1}, CPU: {:.1}%, Mem: {:.1}MB, Disk: {:.2}GB",
                            rec.fps, rec.cpu_percent, rec.memory_mb, sys.available_disk_gb
                        );
                    }
                }
            }
        })
    }

    /// Get available disk space in GB for the given path using sysinfo.
    fn get_disk_space(&self, path: &Path) -> Option<f64> {
        use sysinfo::Disks;

        let disks = Disks::new_with_refreshed_list();
        let path_str = path.to_string_lossy().to_lowercase();

        // Find the disk whose mount point is the longest prefix of our path
        let best = disks
            .iter()
            .filter(|d| {
                let mount = d.mount_point().to_string_lossy().to_lowercase();
                path_str.starts_with(mount.as_str())
            })
            .max_by_key(|d| d.mount_point().to_string_lossy().len());

        best.map(|disk| disk.available_space() as f64 / 1024.0 / 1024.0 / 1024.0)
    }

    #[cfg(test)]
    pub async fn set_system_metrics_for_test(&self, metrics: SystemMetrics) {
        let mut sys_metrics = self.system_metrics.write().await;
        *sys_metrics = metrics;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_healthy() {
        let collector =
            MetricsCollector::new(HealthThresholds::default(), PathBuf::from("C:\\test"));

        let metrics = RecordingMetrics {
            fps: 60.0,
            frame_drops: 0,
            cpu_percent: 30.0,
            memory_mb: 512.0,
            buffer_size_mb: 1000.0,
            ..Default::default()
        };

        // Set system metrics with sufficient disk space
        let sys_metrics = SystemMetrics {
            available_disk_gb: 10.0,
            ..Default::default()
        };

        collector.update_recording_metrics(metrics).await;
        collector.set_system_metrics_for_test(sys_metrics).await;

        let health = collector.check_health().await;
        assert_eq!(health, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_health_check_warning() {
        let collector =
            MetricsCollector::new(HealthThresholds::default(), PathBuf::from("C:\\test"));

        let metrics = RecordingMetrics {
            fps: 50.0, // Below threshold (55)
            frame_drops: 0,
            cpu_percent: 30.0,
            memory_mb: 512.0,
            buffer_size_mb: 1000.0,
            ..Default::default()
        };

        // Set system metrics with sufficient disk space
        let sys_metrics = SystemMetrics {
            available_disk_gb: 10.0,
            ..Default::default()
        };

        collector.update_recording_metrics(metrics).await;
        collector.set_system_metrics_for_test(sys_metrics).await;

        let health = collector.check_health().await;
        assert_eq!(health, HealthStatus::Warning);
    }

    #[tokio::test]
    async fn test_health_check_critical() {
        let collector =
            MetricsCollector::new(HealthThresholds::default(), PathBuf::from("C:\\test"));

        let metrics = RecordingMetrics {
            fps: 40.0, // Very low (< 45)
            frame_drops: 0,
            cpu_percent: 30.0,
            memory_mb: 512.0,
            buffer_size_mb: 1000.0,
            ..Default::default()
        };

        collector.update_recording_metrics(metrics).await;

        let health = collector.check_health().await;
        assert_eq!(health, HealthStatus::Critical);
    }
}
