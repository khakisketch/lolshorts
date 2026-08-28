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

pub const DEFAULT_SYSTEM_METRICS_INTERVAL: Duration = Duration::from_secs(15);
#[cfg(target_os = "windows")]
const GPU_QUERY_TIMEOUT: Duration = Duration::from_secs(3);

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

    /// GPU temperature in Celsius if reported by the supported NVIDIA path
    pub gpu_temperature_celsius: Option<f64>,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            total_cpu_percent: 0.0,
            available_ram_gb: 0.0,
            // Negative means "not measured yet". Treating an unknown disk as
            // zero produced a false critical warning during startup.
            available_disk_gb: -1.0,
            gpu_percent: None,
            gpu_memory_mb: None,
            gpu_temperature_celsius: None,
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
        let (cpu_usage, available_ram_gb) = {
            let mut sys = self.sysinfo.write().await;
            sys.refresh_cpu_all();
            sys.refresh_memory();
            let cpu_usage = if sys.cpus().is_empty() {
                0.0
            } else {
                sys.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32
            };
            (
                cpu_usage as f64,
                sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
            )
        };

        // Disk enumeration and nvidia-smi are blocking OS operations. Run them
        // outside Tokio's async workers and outside the metrics write lock.
        let recording_dir = self.recording_dir.clone();
        let disk_task = tokio::task::spawn_blocking(move || Self::disk_space(&recording_dir));
        #[cfg(target_os = "windows")]
        let gpu_task = tokio::task::spawn_blocking(Self::get_gpu_info);

        let available_disk_gb = disk_task.await.ok().flatten().unwrap_or(-1.0);
        #[cfg(target_os = "windows")]
        let gpu_info = gpu_task.await.ok().and_then(std::result::Result::ok);

        let mut metrics = self.system_metrics.write().await;
        metrics.total_cpu_percent = cpu_usage;
        metrics.available_ram_gb = available_ram_gb;
        metrics.available_disk_gb = available_disk_gb;
        metrics.gpu_percent = None;
        metrics.gpu_memory_mb = None;
        metrics.gpu_temperature_celsius = None;

        #[cfg(target_os = "windows")]
        if let Some(gpu_info) = gpu_info {
            metrics.gpu_percent = Some(gpu_info.utilization);
            metrics.gpu_memory_mb = Some(gpu_info.memory_usage_mb);
            metrics.gpu_temperature_celsius = Some(gpu_info.temperature_celsius);
        }
    }

    /// Read the primary NVIDIA adapter without a shell. Unsupported or
    /// temporarily unavailable adapters return `None` to the frontend instead
    /// of a fabricated 0% value.
    #[cfg(target_os = "windows")]
    fn get_gpu_info() -> anyhow::Result<GpuInfo> {
        let mut command = std::process::Command::new("nvidia-smi");
        command.args([
            "--query-gpu=utilization.gpu,memory.used,temperature.gpu",
            "--format=csv,noheader,nounits",
        ]);
        let output = crate::utils::process::command_output_with_timeout(
            command,
            GPU_QUERY_TIMEOUT,
            "NVIDIA performance metrics",
        )?;
        if !output.status.success() {
            anyhow::bail!("nvidia-smi exited with {}", output.status);
        }
        let line = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .ok_or_else(|| anyhow::anyhow!("nvidia-smi returned no GPU metrics"))?
            .to_string();
        let values = line
            .split(',')
            .map(str::trim)
            .map(str::parse::<f64>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if values.len() != 3 {
            anyhow::bail!("nvidia-smi returned an unexpected metric count");
        }
        Ok(GpuInfo {
            utilization: values[0].clamp(0.0, 100.0),
            memory_usage_mb: values[1].max(0.0),
            temperature_celsius: values[2],
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

        if sys_metrics.available_disk_gb >= 0.0 && sys_metrics.available_disk_gb < 1.0 {
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

        if sys_metrics.available_disk_gb < 0.0 {
            warn!("Recording disk availability is unknown");
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

    /// Run background metrics collection. The caller owns task supervision so
    /// a panic is reported alongside the other monitored runtime services.
    pub async fn run_background_collection(self: Arc<Self>, interval: Duration) {
        let mut interval_timer = tokio::time::interval(interval);

        loop {
            interval_timer.tick().await;
            self.update_system_metrics().await;
            let health = self.check_health().await;

            match health {
                HealthStatus::Healthy => {
                    let mut count = self.health_check_count.write().await;
                    *count += 1;
                    if *count % 10 == 0 {
                        info!("System health check #{}: all systems healthy", *count);
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
    }

    /// Get available disk space in GB for the exact recording volume.
    fn disk_space(path: &Path) -> Option<f64> {
        crate::utils::disk::query_disk_space(path)
            .ok()
            .map(|snapshot| snapshot.available_bytes as f64 / 1024.0 / 1024.0 / 1024.0)
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

    #[tokio::test]
    async fn unknown_disk_is_warning_instead_of_false_critical() {
        let collector =
            MetricsCollector::new(HealthThresholds::default(), PathBuf::from("C:\\test"));

        assert_eq!(SystemMetrics::default().available_disk_gb, -1.0);
        assert_eq!(collector.check_health().await, HealthStatus::Warning);
    }
}
