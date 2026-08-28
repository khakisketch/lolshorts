use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use tracing::{info, warn};

const DEFAULT_MAX_CONCURRENT: usize = 3;

/// Concurrency cap for the process-wide offline FFmpeg pool. Kept low because
/// consumer hardware encoders (notably NVENC) limit simultaneous sessions.
const DEFAULT_GLOBAL_MAX_CONCURRENT: usize = 2;

/// Acquire timeout for the global pool. Deliberately longer than any single
/// offline encode's process timeout so a queued encode waits its turn instead
/// of spuriously failing while another encode holds the slot.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(45 * 60);

static GLOBAL_POOL: OnceLock<FfmpegPool> = OnceLock::new();

/// Process-wide FFmpeg concurrency limiter for **offline** video processing
/// (export / compose / effects). NOT used by the realtime segment recorder,
/// which owns its own capture process lifecycle.
pub fn global_ffmpeg_pool() -> &'static FfmpegPool {
    GLOBAL_POOL.get_or_init(|| FfmpegPool::new(DEFAULT_GLOBAL_MAX_CONCURRENT))
}

pub struct FfmpegPool {
    semaphore: Arc<Semaphore>,
    active_processes: Arc<Mutex<Vec<u32>>>, // PIDs
    max_concurrent: usize,
}

impl FfmpegPool {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            active_processes: Arc::new(Mutex::new(Vec::new())),
            max_concurrent,
        }
    }

    pub async fn acquire(&self) -> Result<FfmpegPermit, crate::error::AppError> {
        let permit = tokio::time::timeout(ACQUIRE_TIMEOUT, self.semaphore.clone().acquire_owned())
            .await
            .map_err(|_| {
                crate::error::AppError::ProcessTimeout(format!(
                    "FFmpeg pool full ({} concurrent). Timed out waiting for slot.",
                    self.max_concurrent
                ))
            })?
            .map_err(|_| crate::error::AppError::Internal("Semaphore closed".into()))?;

        Ok(FfmpegPermit {
            _permit: permit,
            active_processes: self.active_processes.clone(),
            pid: None,
        })
    }

    pub async fn active_count(&self) -> usize {
        self.active_processes.lock().await.len()
    }

    pub async fn kill_all(&self) {
        let pids = self.active_processes.lock().await.clone();
        for pid in pids {
            #[cfg(windows)]
            {
                let mut command = tokio::process::Command::new("taskkill");
                command
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .kill_on_drop(true);
                let _ = tokio::time::timeout(Duration::from_secs(5), command.output()).await;
            }
            warn!(pid = pid, "Force-killed FFmpeg process from pool");
        }
        self.active_processes.lock().await.clear();
    }
}

impl Default for FfmpegPool {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CONCURRENT)
    }
}

impl Drop for FfmpegPool {
    fn drop(&mut self) {
        let pids: Vec<u32> = self
            .active_processes
            .try_lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        for pid in pids {
            #[cfg(windows)]
            {
                let mut command = std::process::Command::new("taskkill");
                command.args(["/F", "/T", "/PID", &pid.to_string()]);
                let _ = crate::utils::process::command_output_with_timeout(
                    command,
                    Duration::from_secs(5),
                    "taskkill cleanup",
                );
            }
        }
    }
}

pub struct FfmpegPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    active_processes: Arc<Mutex<Vec<u32>>>,
    pid: Option<u32>,
}

impl FfmpegPermit {
    pub async fn register_process(&mut self, child: &Child) {
        if let Some(pid) = child.id() {
            self.pid = Some(pid);
            self.active_processes.lock().await.push(pid);
            info!(pid = pid, "Registered FFmpeg process in pool");
        }
    }
}

impl Drop for FfmpegPermit {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            if let Ok(mut procs) = self.active_processes.try_lock() {
                procs.retain(|&p| p != pid);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_allows_up_to_max() {
        let pool = FfmpegPool::new(2);
        let _p1 = pool.acquire().await.unwrap();
        let _p2 = pool.acquire().await.unwrap();
        assert_eq!(pool.semaphore.available_permits(), 0);
    }

    #[tokio::test]
    async fn test_pool_releases_on_drop() {
        let pool = FfmpegPool::new(1);
        {
            let _p = pool.acquire().await.unwrap();
            assert_eq!(pool.semaphore.available_permits(), 0);
        }
        assert_eq!(pool.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn test_active_count() {
        let pool = FfmpegPool::new(3);
        assert_eq!(pool.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_default_pool_has_3_slots() {
        let pool = FfmpegPool::default();
        assert_eq!(pool.max_concurrent, 3);
    }
}
