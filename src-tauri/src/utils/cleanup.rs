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

use crate::settings::models::StorageSettings;
use crate::storage::Storage;

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

        // Clean old rolling-buffer segments left over from a previous session
        // crash/kill. The actual segment directory is recordings/segments
        // (segment mp4s + WASAPI loopback WAV + concat list) -- NOT
        // recordings/temp_segments, which nothing ever writes to.
        let segments_dir = self.app_data_dir.join("recordings").join("segments");
        if segments_dir.exists() {
            total_freed_mb += self
                .cleanup_old_files(&segments_dir, self.config.temp_file_max_age)
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

        // Clean all rolling-buffer segments (fresh start on next launch).
        // See cleanup_on_startup for why this targets recordings/segments.
        let segments_dir = self.app_data_dir.join("recordings").join("segments");
        if segments_dir.exists() {
            self.clear_directory(&segments_dir).await?;
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
        storage: &StorageSettings,
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

    /// Run one full storage-retention cycle: enforce the user's auto-delete /
    /// max-storage policy (`StorageSettings`), then sweep local DB metadata
    /// for rows whose backing file has disappeared (deleted by this policy
    /// itself, by hand in a file manager, or by an external tool).
    ///
    /// Intended to be called once at startup and then on a recurring timer
    /// (see `main.rs` integration) so that policy changes made in the
    /// Settings UI take effect on the next cycle without an app restart --
    /// callers should pass freshly-read `settings` each time rather than a
    /// cached snapshot.
    pub async fn run_retention_cycle(
        &self,
        storage: &Storage,
        settings: &StorageSettings,
    ) -> Result<RetentionCycleReport> {
        // (1) Age/size-based deletion of saved highlight clips. Reuses
        // cleanup_old_clips as-is: no-op unless auto_delete_enabled, and
        // already covers both the auto_delete_days and max_storage_gb caps.
        let clips_dir = storage.recordings_clips_dir();
        let freed_mb = self.cleanup_old_clips(settings, &clips_dir).await?;

        // (2) Orphan sweep: clip metadata (+ thumbnail) rows whose video file
        // is gone -- whether removed by step (1) above, by the user, or by
        // an external tool -- are removed from the DB so the library/editor
        // don't keep offering dead entries.
        let orphaned_clips_removed =
            sweep_orphaned_clips(storage).context("Failed to sweep orphaned clip metadata")?;

        // (3) Same idea for auto-edit results (exports/auto_edit renders).
        let orphaned_auto_edit_results_removed = sweep_orphaned_auto_edit_results(storage)
            .context("Failed to sweep orphaned auto-edit result metadata")?;

        let report = RetentionCycleReport {
            freed_mb,
            orphaned_clips_removed,
            orphaned_auto_edit_results_removed,
        };

        if report.freed_mb > 0
            || report.orphaned_clips_removed > 0
            || report.orphaned_auto_edit_results_removed > 0
        {
            info!(
                "Retention cycle complete: freed {} MB, removed {} orphaned clip row(s), {} orphaned auto-edit result row(s)",
                report.freed_mb, report.orphaned_clips_removed, report.orphaned_auto_edit_results_removed
            );
        } else {
            debug!("Retention cycle complete: nothing to clean up");
        }

        Ok(report)
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

/// Summary of one `CleanupManager::run_retention_cycle` pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetentionCycleReport {
    /// MB freed by the age/max-storage auto-delete policy.
    pub freed_mb: u64,
    /// Clip metadata rows removed because their video file no longer exists.
    pub orphaned_clips_removed: usize,
    /// Auto-edit result rows removed because their output file no longer exists.
    pub orphaned_auto_edit_results_removed: usize,
}

/// Whether `path`'s absence should be treated as a confirmed (permanent)
/// deletion rather than a transient one.
///
/// `Path::exists()` returning `false` is ambiguous: it's also what happens
/// when the *volume* the file lives on isn't currently reachable (an
/// unmounted drive letter, a disconnected external disk, an unplugged
/// network share) -- the file may well still exist once the volume comes
/// back. We only trust the miss when the file's parent directory is
/// verifiably present: if the parent is there but the file isn't, the file
/// was genuinely deleted; if the parent is *also* missing, the whole
/// volume is plausibly just not mounted right now, so callers should skip
/// the row this cycle and re-evaluate on the next one rather than
/// irreversibly deleting metadata for a file that may reappear.
fn is_confirmed_missing(path: &Path) -> bool {
    if path.exists() {
        return false;
    }

    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.exists(),
        // Bare relative filename with no directory component: there is no
        // separate volume to be unmounted, so trust the miss.
        _ => true,
    }
}

/// Remove clip metadata (+ thumbnail file, best-effort) rows whose video file
/// is no longer present on disk. Returns the number of rows removed.
fn sweep_orphaned_clips(storage: &Storage) -> Result<usize> {
    let mut removed = 0usize;

    for (game_id, clip) in storage
        .all_clip_metadata_with_game_id()
        .context("Failed to list clip metadata")?
    {
        let file_path = Path::new(&clip.file_path);
        if file_path.exists() {
            continue;
        }

        if !is_confirmed_missing(file_path) {
            warn!(
                "Retention sweep: skipping orphan check for clip {} -- its parent directory is also missing (possibly an unmounted drive); will re-evaluate next cycle",
                clip.file_path
            );
            continue;
        }

        if let Some(thumbnail_path) = &clip.thumbnail_path {
            if let Err(e) = storage.safe_delete_media_file(thumbnail_path) {
                warn!(
                    "Retention sweep: failed to delete orphaned clip thumbnail {}: {}",
                    thumbnail_path, e
                );
            }
        }

        match storage.delete_clip_metadata(&game_id, &clip.file_path) {
            Ok(()) => {
                debug!(
                    "Retention sweep: removed orphaned clip row {} (game {})",
                    clip.file_path, game_id
                );
                removed += 1;
            }
            Err(e) => warn!(
                "Retention sweep: failed to delete orphaned clip row {}: {}",
                clip.file_path, e
            ),
        }
    }

    Ok(removed)
}

/// Remove auto-edit result rows whose rendered output file is no longer
/// present on disk. Returns the number of rows removed.
fn sweep_orphaned_auto_edit_results(storage: &Storage) -> Result<usize> {
    let mut removed = 0usize;

    let results = storage
        .load_auto_edit_results()
        .context("Failed to list auto-edit results")?;

    for result in results {
        let output_path = Path::new(&result.output_path);
        if output_path.exists() {
            continue;
        }

        if !is_confirmed_missing(output_path) {
            warn!(
                "Retention sweep: skipping orphan check for auto-edit result {} -- its parent directory is also missing (possibly an unmounted drive); will re-evaluate next cycle",
                result.result_id
            );
            continue;
        }

        // delete_file=false: the backing file is already gone, so there is
        // nothing left for safe_delete_media_file to do -- just drop the row.
        match storage.delete_auto_edit_result(&result.result_id, false) {
            Ok(()) => {
                debug!(
                    "Retention sweep: removed orphaned auto-edit result row {}",
                    result.result_id
                );
                removed += 1;
            }
            Err(e) => warn!(
                "Retention sweep: failed to delete orphaned auto-edit result row {}: {}",
                result.result_id, e
            ),
        }
    }

    Ok(removed)
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

    #[tokio::test]
    async fn cleanup_on_startup_targets_recordings_segments_not_temp_segments() {
        // Regression test: startup cleanup used to scan the non-existent
        // recordings/temp_segments directory. The real rolling-buffer
        // segment directory is recordings/segments.
        let temp_dir = tempdir().unwrap();
        let manager = CleanupManager::new(
            temp_dir.path().to_path_buf(),
            CleanupConfig {
                temp_file_max_age: Duration::from_secs(0),
                ..Default::default()
            },
        );

        let segments_dir = temp_dir.path().join("recordings").join("segments");
        fs::create_dir_all(&segments_dir).unwrap();
        let stale_segment = segments_dir.join("segment_000.mp4");
        File::create(&stale_segment).unwrap();

        sleep(Duration::from_millis(50));

        manager.cleanup_on_startup().await.unwrap();

        assert!(
            !stale_segment.exists(),
            "stale rolling-buffer segment should be cleaned up on startup"
        );
    }

    #[tokio::test]
    async fn cleanup_on_shutdown_clears_recordings_segments() {
        let temp_dir = tempdir().unwrap();
        let manager = CleanupManager::new(temp_dir.path().to_path_buf(), CleanupConfig::default());

        let segments_dir = temp_dir.path().join("recordings").join("segments");
        fs::create_dir_all(&segments_dir).unwrap();
        let fresh_segment = segments_dir.join("segment_005.mp4");
        File::create(&fresh_segment).unwrap();

        manager.cleanup_on_shutdown().await.unwrap();

        assert!(
            !fresh_segment.exists(),
            "shutdown cleanup should clear the entire segment buffer, even fresh files"
        );
    }

    #[tokio::test]
    async fn cleanup_old_clips_deletes_files_past_the_auto_delete_policy() {
        let temp_dir = tempdir().unwrap();
        let manager = CleanupManager::new(temp_dir.path().to_path_buf(), CleanupConfig::default());
        let clips_dir = temp_dir.path().join("clips");
        fs::create_dir_all(&clips_dir).unwrap();

        let old_clip = clips_dir.join("old.mp4");
        File::create(&old_clip).unwrap();
        sleep(Duration::from_millis(50));

        // auto_delete_days=0 intentionally bypasses StorageSettings::validate()'s
        // UI-facing 1-365 range so the test doesn't need to sleep a full day --
        // any file that predates `now` counts as "too old".
        let settings = StorageSettings {
            auto_delete_enabled: true,
            auto_delete_days: 0,
            max_storage_gb: 50,
            delete_exported_clips: true,
        };

        manager
            .cleanup_old_clips(&settings, &clips_dir)
            .await
            .unwrap();

        assert!(!old_clip.exists());
    }

    #[tokio::test]
    async fn cleanup_old_clips_is_noop_when_auto_delete_disabled() {
        let temp_dir = tempdir().unwrap();
        let manager = CleanupManager::new(temp_dir.path().to_path_buf(), CleanupConfig::default());
        let clips_dir = temp_dir.path().join("clips");
        fs::create_dir_all(&clips_dir).unwrap();

        let clip = clips_dir.join("clip.mp4");
        File::create(&clip).unwrap();
        sleep(Duration::from_millis(50));

        let settings = StorageSettings {
            auto_delete_enabled: false,
            auto_delete_days: 0,
            max_storage_gb: 50,
            delete_exported_clips: true,
        };

        let freed_mb = manager
            .cleanup_old_clips(&settings, &clips_dir)
            .await
            .unwrap();

        assert_eq!(freed_mb, 0);
        assert!(clip.exists());
    }

    #[tokio::test]
    async fn cleanup_old_clips_preserves_exported_clips_by_default() {
        let temp_dir = tempdir().unwrap();
        let manager = CleanupManager::new(temp_dir.path().to_path_buf(), CleanupConfig::default());
        let clips_dir = temp_dir.path().join("clips");
        fs::create_dir_all(&clips_dir).unwrap();

        let exported_clip = clips_dir.join("exported.mp4");
        File::create(&exported_clip).unwrap();
        File::create(exported_clip.with_extension("exported")).unwrap();
        sleep(Duration::from_millis(50));

        let settings = StorageSettings {
            auto_delete_enabled: true,
            auto_delete_days: 0,
            max_storage_gb: 50,
            delete_exported_clips: false,
        };

        manager
            .cleanup_old_clips(&settings, &clips_dir)
            .await
            .unwrap();

        assert!(
            exported_clip.exists(),
            "exported clip should be preserved when delete_exported_clips=false"
        );
    }

    #[tokio::test]
    async fn run_retention_cycle_removes_orphaned_clip_and_auto_edit_metadata() {
        use crate::storage::models::{
            AutoEditResultMetadata, ClipMetadata, EventType, GameMetadata, UploadStatus,
            YouTubeUploadStatus,
        };

        let temp_dir = tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let manager = CleanupManager::new(temp_dir.path().to_path_buf(), CleanupConfig::default());

        let game_id = "game-orphan";
        storage
            .save_game_metadata(
                game_id,
                &GameMetadata {
                    game_id: game_id.to_string(),
                    champion: "Ahri".to_string(),
                    game_mode: "Ranked".to_string(),
                    start_time: chrono::Utc::now(),
                    end_time: None,
                    result: None,
                    kda: None,
                },
            )
            .unwrap();

        // Clip metadata row whose video file was never actually written
        // (e.g. deleted out-of-band, or by the auto-delete policy itself).
        // Create the parent directory first, as it would already exist in
        // real usage (some earlier recording wrote into it) -- the sweep
        // now distinguishes "file gone but its directory is still there"
        // (a real deletion) from "the directory itself is also gone" (a
        // plausibly-unmounted volume, see the dedicated test below).
        fs::create_dir_all(storage.recordings_clips_dir()).unwrap();
        let missing_clip_path = storage
            .recordings_clips_dir()
            .join("missing.mp4")
            .to_string_lossy()
            .to_string();
        storage
            .save_clip_metadata(
                game_id,
                &ClipMetadata {
                    file_path: missing_clip_path,
                    thumbnail_path: None,
                    event_type: EventType::ChampionKill,
                    event_time: 10.0,
                    priority: 1,
                    duration: 15.0,
                    event_offset_secs: None,
                    created_at: chrono::Utc::now(),
                    usage_count: 0,
                },
            )
            .unwrap();

        // Auto-edit result row whose rendered output was never written.
        // Same reasoning as above: the exports directory would already
        // exist in real usage.
        fs::create_dir_all(storage.exports_dir()).unwrap();
        let missing_result_path = storage
            .exports_dir()
            .join("missing_result.mp4")
            .to_string_lossy()
            .to_string();
        storage
            .save_auto_edit_result(&AutoEditResultMetadata {
                result_id: "result-orphan".to_string(),
                job_id: "job-orphan".to_string(),
                output_path: missing_result_path,
                thumbnail_path: None,
                created_at: chrono::Utc::now(),
                duration: 30.0,
                clip_count: 1,
                game_ids: vec![game_id.to_string()],
                target_duration: 60,
                canvas_template_name: None,
                has_background_music: false,
                youtube_status: Some(YouTubeUploadStatus {
                    video_id: None,
                    status: UploadStatus::NotUploaded,
                    upload_started_at: None,
                    upload_completed_at: None,
                    progress: 0.0,
                    error: None,
                }),
                file_size_bytes: 0,
            })
            .unwrap();

        // auto_delete_enabled=false: only the orphan sweep should act.
        let settings = StorageSettings::default();

        let report = manager
            .run_retention_cycle(&storage, &settings)
            .await
            .unwrap();

        assert_eq!(report.freed_mb, 0);
        assert_eq!(report.orphaned_clips_removed, 1);
        assert_eq!(report.orphaned_auto_edit_results_removed, 1);

        assert!(storage.load_clip_metadata(game_id).unwrap().is_empty());
        assert!(storage.load_auto_edit_result("result-orphan").is_err());
    }

    #[tokio::test]
    async fn run_retention_cycle_keeps_clip_metadata_when_file_exists() {
        use crate::storage::models::{ClipMetadata, EventType, GameMetadata};

        let temp_dir = tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let manager = CleanupManager::new(temp_dir.path().to_path_buf(), CleanupConfig::default());

        let game_id = "game-alive";
        storage
            .save_game_metadata(
                game_id,
                &GameMetadata {
                    game_id: game_id.to_string(),
                    champion: "Lux".to_string(),
                    game_mode: "Ranked".to_string(),
                    start_time: chrono::Utc::now(),
                    end_time: None,
                    result: None,
                    kda: None,
                },
            )
            .unwrap();

        let clips_dir = storage.recordings_clips_dir();
        fs::create_dir_all(&clips_dir).unwrap();
        let clip_path = clips_dir.join("alive.mp4");
        File::create(&clip_path).unwrap();

        storage
            .save_clip_metadata(
                game_id,
                &ClipMetadata {
                    file_path: clip_path.to_string_lossy().to_string(),
                    thumbnail_path: None,
                    event_type: EventType::Ace,
                    event_time: 5.0,
                    priority: 4,
                    duration: 12.0,
                    event_offset_secs: None,
                    created_at: chrono::Utc::now(),
                    usage_count: 0,
                },
            )
            .unwrap();

        let settings = StorageSettings::default();
        let report = manager
            .run_retention_cycle(&storage, &settings)
            .await
            .unwrap();

        assert_eq!(report.orphaned_clips_removed, 0);
        assert_eq!(storage.load_clip_metadata(game_id).unwrap().len(), 1);
    }

    // ---- is_confirmed_missing ----

    #[test]
    fn is_confirmed_missing_false_when_file_exists() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("present.mp4");
        File::create(&file_path).unwrap();

        assert!(!is_confirmed_missing(&file_path));
    }

    #[test]
    fn is_confirmed_missing_true_when_parent_dir_exists_but_file_does_not() {
        let temp_dir = tempdir().unwrap();
        // temp_dir itself exists; the file inside it does not.
        let file_path = temp_dir.path().join("gone.mp4");

        assert!(is_confirmed_missing(&file_path));
    }

    #[test]
    fn is_confirmed_missing_false_when_parent_dir_is_also_missing() {
        let temp_dir = tempdir().unwrap();
        // Neither the "unplugged_drive" directory nor the file inside it exist.
        let file_path = temp_dir.path().join("unplugged_drive").join("clip.mp4");

        assert!(!is_confirmed_missing(&file_path));
    }

    #[tokio::test]
    async fn run_retention_cycle_skips_clip_when_parent_directory_is_missing() {
        // Regression: a transiently-unreachable volume (e.g. an unmounted
        // drive) must not be mistaken for a genuinely deleted clip file --
        // the row should survive the sweep for re-evaluation next cycle.
        use crate::storage::models::{ClipMetadata, EventType, GameMetadata};

        let temp_dir = tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let manager = CleanupManager::new(temp_dir.path().to_path_buf(), CleanupConfig::default());

        let game_id = "game-unmounted";
        storage
            .save_game_metadata(
                game_id,
                &GameMetadata {
                    game_id: game_id.to_string(),
                    champion: "Zed".to_string(),
                    game_mode: "Ranked".to_string(),
                    start_time: chrono::Utc::now(),
                    end_time: None,
                    result: None,
                    kda: None,
                },
            )
            .unwrap();

        // Simulate an unmounted drive: the clip's parent directory does not
        // exist at all (not just the file itself).
        let missing_clip_path = temp_dir
            .path()
            .join("unplugged_drive")
            .join("clip.mp4")
            .to_string_lossy()
            .to_string();
        storage
            .save_clip_metadata(
                game_id,
                &ClipMetadata {
                    file_path: missing_clip_path,
                    thumbnail_path: None,
                    event_type: EventType::Ace,
                    event_time: 5.0,
                    priority: 4,
                    duration: 12.0,
                    event_offset_secs: None,
                    created_at: chrono::Utc::now(),
                    usage_count: 0,
                },
            )
            .unwrap();

        let settings = StorageSettings::default();
        let report = manager
            .run_retention_cycle(&storage, &settings)
            .await
            .unwrap();

        assert_eq!(
            report.orphaned_clips_removed, 0,
            "clip row must survive the sweep when its parent directory is also missing"
        );
        assert_eq!(storage.load_clip_metadata(game_id).unwrap().len(), 1);
    }
}
