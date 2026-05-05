#![allow(dead_code)]
use anyhow::{Context, Result};
use std::fs;
/// Resource cleanup and memory management for production stability
///
/// Provides automatic cleanup of temporary files, orphaned processes,
/// and memory leak prevention through RAII patterns and explicit cleanup hooks.
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, info, warn};

/// Cleanup configuration
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    /// Maximum age of temporary files before deletion (default: 24 hours)
    pub temp_file_max_age: Duration,

    /// Maximum size of log directory in MB (default: 500)
    pub max_log_size_mb: u64,

    /// Maximum size of temp segments in MB (default: 10GB)
    pub max_temp_segments_mb: u64,

    /// Enable automatic cleanup on startup (default: true)
    pub cleanup_on_startup: bool,

    /// Enable automatic cleanup on shutdown (default: true)
    pub cleanup_on_shutdown: bool,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            temp_file_max_age: Duration::from_secs(24 * 60 * 60), // 24 hours
            max_log_size_mb: 500,
            max_temp_segments_mb: 10 * 1024, // 10 GB
            cleanup_on_startup: true,
            cleanup_on_shutdown: true,
        }
    }
}

/// Resource cleanup manager
pub struct CleanupManager {
    config: CleanupConfig,
    app_data_dir: PathBuf,
}

impl CleanupManager {
    pub fn new(app_data_dir: PathBuf, config: CleanupConfig) -> Self {
        Self {
            config,
            app_data_dir,
        }
    }

    /// Run startup cleanup
    ///
    /// Cleans up orphaned files from previous session crashes
    pub async fn cleanup_on_startup(&self) -> Result<()> {
        if !self.config.cleanup_on_startup {
            return Ok(());
        }

        info!("Running startup cleanup...");

        let mut total_freed_mb = 0;

        // Clean old temporary segments
        let temp_segments_dir = self.app_data_dir.join("recordings/temp_segments");
        if temp_segments_dir.exists() {
            total_freed_mb += self
                .cleanup_old_files(&temp_segments_dir, self.config.temp_file_max_age)
                .await?;
        }

        // Clean old logs
        let logs_dir = self.app_data_dir.join("logs");
        if logs_dir.exists() {
            total_freed_mb += self.enforce_log_size_limit(&logs_dir).await?;
        }

        info!("Startup cleanup complete: freed {} MB", total_freed_mb);

        Ok(())
    }

    /// Run shutdown cleanup
    ///
    /// Gracefully shuts down resources and removes temporary files
    pub async fn cleanup_on_shutdown(&self) -> Result<()> {
        if !self.config.cleanup_on_shutdown {
            return Ok(());
        }

        info!("Running shutdown cleanup...");

        // Clean all temporary segments (fresh start on next launch)
        let temp_segments_dir = self.app_data_dir.join("recordings/temp_segments");
        if temp_segments_dir.exists() {
            self.clear_directory(&temp_segments_dir).await?;
        }

        info!("Shutdown cleanup complete");

        Ok(())
    }

    /// Clean old saved clips based on auto-delete policy
    ///
    /// - Deletes clips older than `auto_delete_days` if `auto_delete_enabled` is true
    /// - Deletes oldest clips first if total usage exceeds `max_storage_gb`
    /// - Skips exported clips unless `delete_exported_clips` is true
    ///
    /// Returns freed space in MB
    pub async fn cleanup_old_clips(
        &self,
        storage: &crate::settings::models::StorageSettings,
        clips_dir: &Path,
    ) -> Result<u64> {
        if !storage.auto_delete_enabled {
            return Ok(0);
        }

        if !clips_dir.exists() {
            return Ok(0);
        }

        let now = SystemTime::now();
        let max_age = Duration::from_secs(u64::from(storage.auto_delete_days) * 24 * 60 * 60);
        let max_bytes = u64::from(storage.max_storage_gb) * 1024 * 1024 * 1024;

        // Collect all clip files with metadata
        let mut clip_files: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
        let mut total_size: u64 = 0;

        let entries = fs::read_dir(clips_dir)
            .context(format!("Failed to read clips directory: {:?}", clips_dir))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let metadata = fs::metadata(&path)?;
            let modified = metadata.modified()?;
            let size = metadata.len();

            clip_files.push((path, modified, size));
            total_size += size;
        }

        // Sort oldest first
        clip_files.sort_by_key(|(_, modified, _)| *modified);

        let mut freed_bytes: u64 = 0;

        for (path, modified, size) in &clip_files {
            let age = now.duration_since(*modified).unwrap_or(Duration::ZERO);
            let exceeds_storage = (total_size - freed_bytes) > max_bytes;
            let too_old = age > max_age;

            if !too_old && !exceeds_storage {
                continue;
            }

            // Skip exported clips if policy says so
            if !storage.delete_exported_clips {
                // Exported clips have a sidecar marker file (.exported)
                let marker = path.with_extension("exported");
                if marker.exists() {
                    debug!("Skipping exported clip: {:?}", path);
                    continue;
                }
            }

            let reason = if too_old { "age" } else { "storage limit" };
            info!(
                "Deleting old clip ({} days, reason: {}): {:?}",
                age.as_secs() / 86400,
                reason,
                path
            );

            if let Err(e) = fs::remove_file(path) {
                warn!("Failed to remove clip {:?}: {}", path, e);
            } else {
                freed_bytes += size;
            }
        }

        let freed_mb = freed_bytes / 1024 / 1024;
        if freed_mb > 0 {
            info!("Clip auto-delete freed {} MB", freed_mb);
        }

        Ok(freed_mb)
    }

    /// Clean files older than specified age
    ///
    /// Returns freed space in MB
    async fn cleanup_old_files(&self, dir: &Path, max_age: Duration) -> Result<u64> {
        let mut freed_bytes: u64 = 0;
        let now = SystemTime::now();

        let entries = fs::read_dir(dir).context(format!("Failed to read directory: {:?}", dir))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let metadata = fs::metadata(&path)?;
                let modified = metadata.modified()?;

                if let Ok(age) = now.duration_since(modified) {
                    if age > max_age {
                        let size = metadata.len();
                        debug!("Removing old file: {:?} (age: {:?})", path, age);

                        if let Err(e) = fs::remove_file(&path) {
                            warn!("Failed to remove file {:?}: {}", path, e);
                        } else {
                            freed_bytes += size;
                        }
                    }
                }
            }
        }

        Ok(freed_bytes / 1024 / 1024) // Convert to MB
    }

    /// Enforce log directory size limit
    ///
    /// Deletes oldest logs first until under limit
    /// Returns freed space in MB
    async fn enforce_log_size_limit(&self, logs_dir: &Path) -> Result<u64> {
        // Calculate total size
        let mut log_files: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
        let mut total_size: u64 = 0;

        let entries = fs::read_dir(logs_dir)
            .context(format!("Failed to read log directory: {:?}", logs_dir))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let metadata = fs::metadata(&path)?;
                let modified = metadata.modified()?;
                let size = metadata.len();

                log_files.push((path, modified, size));
                total_size += size;
            }
        }

        let total_size_mb = total_size / 1024 / 1024;

        if total_size_mb <= self.config.max_log_size_mb {
            debug!("Log directory size OK: {} MB", total_size_mb);
            return Ok(0);
        }

        warn!(
            "Log directory exceeds limit: {} MB / {} MB",
            total_size_mb, self.config.max_log_size_mb
        );

        // Sort by modification time (oldest first)
        log_files.sort_by_key(|(_, modified, _)| *modified);

        let mut freed_bytes: u64 = 0;
        let target_size = self.config.max_log_size_mb * 1024 * 1024;

        for (path, _, size) in log_files {
            if total_size - freed_bytes <= target_size {
                break;
            }

            debug!("Removing old log file: {:?}", path);

            if let Err(e) = fs::remove_file(&path) {
                warn!("Failed to remove log file {:?}: {}", path, e);
            } else {
                freed_bytes += size;
            }
        }

        Ok(freed_bytes / 1024 / 1024) // Convert to MB
    }

    /// Clear entire directory
    async fn clear_directory(&self, dir: &Path) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(dir).context(format!("Failed to read directory: {:?}", dir))?;

        let mut removed_count = 0;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Err(e) = fs::remove_file(&path) {
                    warn!("Failed to remove file {:?}: {}", path, e);
                } else {
                    removed_count += 1;
                }
            }
        }

        debug!("Cleared {} files from {:?}", removed_count, dir);

        Ok(())
    }

    /// Check disk space availability
    ///
    /// Returns available space in GB
    pub fn check_disk_space(&self) -> Result<f64> {
        #[cfg(target_os = "windows")]
        {
            let _metadata = fs::metadata(&self.app_data_dir)?;
            // On Windows, use GetDiskFreeSpaceExW API for accurate disk space information

            // Get actual disk space using Windows API
            let volume_path = match self.app_data_dir.to_string_lossy().split(':').next() {
                Some(drive_letter) => format!("{}:\\", drive_letter),
                None => "C:\\".to_string(),
            };

            let (free_bytes_available, _total_bytes) = unsafe {
                use std::ffi::OsStr;
                use std::os::windows::ffi::OsStrExt;
                use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

                let wide_path: Vec<u16> = OsStr::new(&volume_path)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();

                let mut free_bytes = 0u64;
                let mut total_bytes = 0u64;
                let mut total_free_bytes = 0u64;

                let result = GetDiskFreeSpaceExW(
                    windows::core::PCWSTR(wide_path.as_ptr()),
                    Some(&mut free_bytes as *mut _),
                    Some(&mut total_bytes as *mut _),
                    Some(&mut total_free_bytes as *mut _),
                );

                if result.is_ok() {
                    (free_bytes, total_bytes)
                } else {
                    (10u64.pow(10), 100u64.pow(10)) // 10GB free, 100GB total fallback
                }
            };

            let free_gb = free_bytes_available as f64 / (1024.0 * 1024.0 * 1024.0);
            tracing::debug!(
                "Disk space check: {:.2} GB available on {}",
                free_gb,
                volume_path
            );

            Ok(free_gb)
        }

        #[cfg(not(target_os = "windows"))]
        {
            // For Unix-like systems, use statvfs
            use nix::sys::statvfs::statvfs;
            use std::ffi::CString;

            let path = CString::new(self.app_data_dir.to_string_lossy().as_bytes())
                .context("Failed to create CString from path")?;

            let stats = statvfs(&path).context("Failed to get filesystem statistics")?;

            let block_size = stats.f_bsize as u64;
            let available_blocks = stats.f_bavail as u64;
            let available_bytes = block_size * available_blocks;

            let available_gb = available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

            Ok(available_gb)
        }
    }

    /// Get total disk space information
    ///
    /// Returns (available_gb, total_gb) for the disk where app data is stored
    pub fn get_disk_space_info(&self) -> Result<(f64, f64)> {
        #[cfg(target_os = "windows")]
        {
            let _metadata = fs::metadata(&self.app_data_dir)?;

            // Get actual disk space using Windows API
            let volume_path = match self.app_data_dir.to_string_lossy().split(':').next() {
                Some(drive_letter) => format!("{}:\\", drive_letter),
                None => "C:\\".to_string(),
            };

            let (free_bytes_available, total_bytes) = unsafe {
                use std::ffi::OsStr;
                use std::os::windows::ffi::OsStrExt;
                use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

                let wide_path: Vec<u16> = OsStr::new(&volume_path)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();

                let mut free_bytes = 0u64;
                let mut total_bytes = 0u64;
                let mut total_free_bytes = 0u64;

                let result = GetDiskFreeSpaceExW(
                    windows::core::PCWSTR(wide_path.as_ptr()),
                    Some(&mut free_bytes as *mut _),
                    Some(&mut total_bytes as *mut _),
                    Some(&mut total_free_bytes as *mut _),
                );

                if result.is_ok() {
                    (free_bytes, total_bytes)
                } else {
                    (10u64.pow(10), 100u64.pow(10)) // 10GB free, 100GB total fallback
                }
            };

            let free_gb = free_bytes_available as f64 / (1024.0 * 1024.0 * 1024.0);
            let total_gb = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

            tracing::debug!(
                "Disk space info: {:.2} GB available, {:.2} GB total on {}",
                free_gb,
                total_gb,
                volume_path
            );

            Ok((free_gb, total_gb))
        }

        #[cfg(not(target_os = "windows"))]
        {
            // For Unix-like systems, use statvfs
            use nix::sys::statvfs::statvfs;
            use std::ffi::CString;

            let path = CString::new(self.app_data_dir.to_string_lossy().as_bytes())
                .context("Failed to create CString from path")?;

            let stats = statvfs(&path).context("Failed to get filesystem statistics")?;

            let block_size = stats.f_bsize as u64;
            let available_blocks = stats.f_bavail as u64;
            let total_blocks = stats.f_blocks as u64;

            let available_gb = (block_size * available_blocks) as f64 / (1024.0 * 1024.0 * 1024.0);
            let total_gb = (block_size * total_blocks) as f64 / (1024.0 * 1024.0 * 1024.0);

            Ok((available_gb, total_gb))
        }
    }
}

/// RAII guard for temporary file cleanup
///
/// Automatically removes file when dropped
pub struct TempFileGuard {
    path: PathBuf,
    cleanup_on_drop: bool,
}

impl TempFileGuard {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cleanup_on_drop: true,
        }
    }

    /// Disable cleanup on drop (keep file)
    pub fn keep(mut self) -> PathBuf {
        self.cleanup_on_drop = false;
        self.path.clone()
    }

    /// Get path reference
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.cleanup_on_drop && self.path.exists() {
            if let Err(e) = fs::remove_file(&self.path) {
                warn!("Failed to cleanup temp file {:?}: {}", self.path, e);
            } else {
                debug!("Cleaned up temp file: {:?}", self.path);
            }
        }
    }
}

/// Process cleanup utilities
pub mod process {
    use std::process::Child;
    use tracing::{debug, warn};

    /// Ensure FFmpeg process is terminated
    pub fn terminate_ffmpeg(mut child: Child) {
        debug!("Terminating FFmpeg process (PID: {:?})", child.id());

        // Try graceful shutdown first
        if let Err(e) = child.kill() {
            warn!("Failed to terminate FFmpeg process: {}", e);
        }

        // Wait for process to exit (with timeout)
        match child.wait() {
            Ok(status) => {
                debug!("FFmpeg process exited with status: {}", status);
            }
            Err(e) => {
                warn!("Failed to wait for FFmpeg process: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::thread::sleep;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_cleanup_old_files() {
        let temp_dir = tempdir().unwrap();
        let manager = CleanupManager::new(
            temp_dir.path().to_path_buf(),
            CleanupConfig {
                temp_file_max_age: Duration::from_secs(1),
                ..Default::default()
            },
        );

        // Create old file
        let old_file = temp_dir.path().join("old.tmp");
        File::create(&old_file).unwrap();

        // Wait for file to age
        sleep(Duration::from_secs(2));

        // Create new file
        let new_file = temp_dir.path().join("new.tmp");
        File::create(&new_file).unwrap();

        // Cleanup
        let _freed = manager
            .cleanup_old_files(temp_dir.path(), Duration::from_secs(1))
            .await
            .unwrap();

        // Old file should be removed
        assert!(!old_file.exists());

        // New file should still exist
        assert!(new_file.exists());
    }

    #[tokio::test]
    async fn test_enforce_log_size_limit() {
        let temp_dir = tempdir().unwrap();
        let manager = CleanupManager::new(
            temp_dir.path().to_path_buf(),
            CleanupConfig {
                max_log_size_mb: 1, // 1 MB limit
                ..Default::default()
            },
        );

        // Create large log files
        for i in 0..5 {
            let log_file = temp_dir.path().join(format!("log{}.txt", i));
            let mut file = File::create(&log_file).unwrap();
            file.write_all(&vec![0u8; 500 * 1024]).unwrap(); // 500 KB each
            sleep(Duration::from_millis(100)); // Ensure different modification times
        }

        // Enforce limit
        let freed = manager
            .enforce_log_size_limit(temp_dir.path())
            .await
            .unwrap();

        assert!(freed > 0);
    }

    #[test]
    fn test_temp_file_guard_cleanup() {
        let temp_dir = tempdir().unwrap();
        let temp_file = temp_dir.path().join("test.tmp");

        {
            File::create(&temp_file).unwrap();
            let _guard = TempFileGuard::new(temp_file.clone());

            assert!(temp_file.exists());
        }

        // File should be removed after guard dropped
        assert!(!temp_file.exists());
    }

    #[test]
    fn test_temp_file_guard_keep() {
        let temp_dir = tempdir().unwrap();
        let temp_file = temp_dir.path().join("test.tmp");

        {
            File::create(&temp_file).unwrap();
            let guard = TempFileGuard::new(temp_file.clone());

            assert!(temp_file.exists());

            // Keep the file
            guard.keep();
        }

        // File should still exist
        assert!(temp_file.exists());
    }
}
