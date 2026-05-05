#![allow(dead_code)]
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;
use tokio::time::Instant;
use tracing::info;

use super::segment_recorder::SegmentRecorder;
use super::types::{RecordingConfig, RecordingStats, RecordingStatus};
use crate::storage::GameMetadata;

/// Check whether the League of Legends game window is visible and not minimized.
///
/// Returns `true` if the window is found and visible, or if the check cannot be
/// performed (so recording is never blocked by a failed detection).
/// Returns `false` only when the window is definitively found but minimized/hidden.
#[cfg(target_os = "windows")]
fn is_game_window_visible() -> bool {
    // Win32 APIs used for window detection
    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(lp_class_name: *const u16, lp_window_name: *const u16) -> isize;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
    }

    fn to_wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn hwnd_is_visible(hwnd: isize) -> bool {
        unsafe { IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 }
    }

    // Try each known LoL window class name
    const WINDOW_CLASSES: &[&str] = &["RiotWindowClass", "RCLIENT"];
    for class in WINDOW_CLASSES {
        let wide = to_wide_null(class);
        let hwnd = unsafe { FindWindowW(wide.as_ptr(), std::ptr::null()) };
        if hwnd != 0 {
            let visible = hwnd_is_visible(hwnd);
            if visible {
                tracing::debug!("LoL window class '{}' found and visible", class);
            } else {
                tracing::warn!(
                    "LoL window class '{}' found but minimized/hidden — \
                     recording may capture a blank screen",
                    class
                );
            }
            return visible;
        }
    }

    // No window found by class — game may not be running yet; allow recording to proceed
    tracing::debug!("LoL window not found by class name; proceeding with recording");
    true
}

/// Non-Windows stub — always returns true so the check is a no-op on other platforms.
#[cfg(not(target_os = "windows"))]
fn is_game_window_visible() -> bool {
    true
}

/// 메인 녹화 매니저 (WindowsCaptureRecorder 호환)
pub struct WindowsCaptureRecorder {
    pub(super) config: RecordingConfig,
    pub(super) status: Arc<TokioRwLock<RecordingStatus>>,
    pub(super) segment_recorder: Arc<TokioRwLock<SegmentRecorder>>,
    pub(super) current_game: Arc<TokioRwLock<Option<GameMetadata>>>,
    pub(super) start_time: Arc<TokioRwLock<Option<Instant>>>,
    pub(super) total_frames: Arc<TokioRwLock<u64>>,
}

impl WindowsCaptureRecorder {
    pub async fn new(config: RecordingConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.output_dir)?;

        let segment_recorder = SegmentRecorder::new(config.clone())?;

        Ok(Self {
            config,
            status: Arc::new(TokioRwLock::new(RecordingStatus::Idle)),
            segment_recorder: Arc::new(TokioRwLock::new(segment_recorder)),
            current_game: Arc::new(TokioRwLock::new(None)),
            start_time: Arc::new(TokioRwLock::new(None)),
            total_frames: Arc::new(TokioRwLock::new(0)),
        })
    }

    /// 녹화 시작
    pub async fn start_recording(&mut self) -> Result<()> {
        let mut status = self.status.write().await;

        if *status != RecordingStatus::Idle {
            anyhow::bail!("이미 녹화가 진행 중입니다");
        }

        // 디스크 여유 공간 확인 (최소 2GB)
        const MIN_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
        let free_bytes = get_free_disk_space(&self.config.output_dir);
        if free_bytes < MIN_FREE_BYTES {
            let free_gb = free_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            anyhow::bail!(
                "디스크 공간이 부족합니다. 최소 2GB 이상의 여유 공간이 필요합니다. (현재: {:.1}GB)",
                free_gb
            );
        }

        *status = RecordingStatus::Buffering;
        drop(status);

        // Warn if the game window is not currently visible (minimized/hidden).
        // We do not block recording — gdigrab can still capture a minimized window
        // in some configurations, and the window may become visible shortly after.
        if !is_game_window_visible() {
            tracing::warn!(
                "Game window appears minimized or hidden; recording will start anyway \
                 but captured frames may be blank until the window is restored"
            );
        }

        {
            let mut recorder = self.segment_recorder.write().await;
            recorder.start().await?;
        }

        *self.start_time.write().await = Some(Instant::now());
        *self.total_frames.write().await = 0;
        *self.status.write().await = RecordingStatus::Recording;

        info!("녹화가 성공적으로 시작되었습니다");
        Ok(())
    }

    /// 녹화 중지
    pub async fn stop_recording(&mut self) -> Result<PathBuf> {
        {
            let mut status = self.status.write().await;
            match *status {
                RecordingStatus::Idle => {
                    anyhow::bail!("진행 중인 녹화가 없습니다");
                }
                RecordingStatus::Processing => {
                    anyhow::bail!("이미 녹화 중지가 진행 중입니다");
                }
                RecordingStatus::Recording | RecordingStatus::Buffering => {
                    *status = RecordingStatus::Processing;
                }
                RecordingStatus::Error => {
                    *status = RecordingStatus::Processing;
                }
            }
        }

        let stop_result = {
            let mut recorder = self.segment_recorder.write().await;
            recorder.stop().await
        };

        match stop_result {
            Ok(()) => {
                *self.start_time.write().await = None;
                *self.status.write().await = RecordingStatus::Idle;
            }
            Err(e) => {
                *self.status.write().await = RecordingStatus::Error;
                return Err(e);
            }
        }

        let output_path = self.config.output_dir.join("segments");
        info!("녹화 중지됨: {}", output_path.display());
        Ok(output_path)
    }

    /// 마지막 N초 클립 저장
    pub async fn save_last_seconds(&self, seconds: u32) -> Result<PathBuf> {
        let status = self.status.read().await;
        if *status != RecordingStatus::Recording && *status != RecordingStatus::Buffering {
            anyhow::bail!("녹화가 진행 중이 아닙니다");
        }
        drop(status);

        let recorder = self.segment_recorder.read().await;
        let elapsed = recorder.get_elapsed_secs();

        let start_offset = if elapsed > seconds as f64 {
            elapsed - seconds as f64
        } else {
            0.0
        };

        let duration = seconds as f64;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("clip_{}_{}.mp4", timestamp, seconds);
        let output_path = self.config.output_dir.join("clips").join(&filename);

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        recorder
            .save_clip(&output_path, start_offset, duration)
            .await
    }

    /// 특정 이벤트 시점 기준 클립 저장 (상대 시간 적용)
    pub async fn save_event_clip(
        &self,
        _event_time_secs: f64,
        pre_secs: f64,
        post_secs: f64,
        clip_id: &str,
    ) -> Result<PathBuf> {
        let status = self.status.read().await;
        if *status != RecordingStatus::Recording && *status != RecordingStatus::Buffering {
            anyhow::bail!("녹화가 진행 중이 아닙니다");
        }
        drop(status);

        let recorder = self.segment_recorder.read().await;
        let elapsed = recorder.get_elapsed_secs();
        let total_duration = pre_secs + post_secs;

        let start_offset = if elapsed > total_duration {
            elapsed - total_duration
        } else {
            0.0
        };

        let filename = format!("{}.mp4", clip_id);
        let output_path = self.config.output_dir.join("clips").join(&filename);

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        recorder
            .save_clip(&output_path, start_offset, total_duration)
            .await
    }

    pub async fn get_status(&self) -> RecordingStatus {
        *self.status.read().await
    }

    pub async fn get_stats(&self) -> RecordingStats {
        let total_frames = *self.total_frames.read().await;
        let start_time = *self.start_time.read().await;
        let uptime = start_time.map(|t| t.elapsed().as_secs_f64());

        RecordingStats {
            total_frames,
            uptime_seconds: uptime.unwrap_or(0.0),
            current_fps: match uptime {
                Some(u) if u > 0.0 => total_frames as f64 / u,
                _ => self.config.fps as f64,
            },
        }
    }

    pub async fn set_current_game(&self, game: Option<GameMetadata>) {
        *self.current_game.write().await = game;
    }

    pub async fn get_current_game(&self) -> Option<GameMetadata> {
        self.current_game.read().await.clone()
    }

    /// 캡처 대상 설정 (게임 창 제목)
    pub async fn set_capture_target(&mut self, target: Option<String>) {
        self.config.capture_target = target;
    }

    /// 현재 설정 반환
    pub fn get_config(&self) -> &RecordingConfig {
        &self.config
    }

    /// Idle 상태에서만 설정 업데이트 (녹화 중 변경 불가)
    pub async fn update_config(&mut self, config: RecordingConfig) -> Result<()> {
        let status = *self.status.read().await;
        if status != RecordingStatus::Idle {
            anyhow::bail!("녹화 중에는 설정을 변경할 수 없습니다");
        }

        let segment_recorder = SegmentRecorder::new(config.clone())?;
        self.config = config;
        *self.segment_recorder.write().await = segment_recorder;

        info!(
            "녹화 설정 업데이트: {}x{} @ {}fps, {} Mbps",
            self.config.resolution.0,
            self.config.resolution.1,
            self.config.fps,
            self.config.bitrate / 1_000_000
        );

        Ok(())
    }
}

fn get_free_disk_space(path: &Path) -> u64 {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let root = path
            .ancestors()
            .last()
            .unwrap_or(path)
            .to_str()
            .and_then(|s| s.chars().next())
            .map(|c| format!("{}:\\", c))
            .unwrap_or_else(|| "C:\\".to_string());

        let wide_path: Vec<u16> = OsStr::new(&root)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes_available: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free_bytes: u64 = 0;

        unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetDiskFreeSpaceExW(
                    lpDirectoryName: *const u16,
                    lpFreeBytesAvailableToCaller: *mut u64,
                    lpTotalNumberOfBytes: *mut u64,
                    lpTotalNumberOfFreeBytes: *mut u64,
                ) -> i32;
            }

            if GetDiskFreeSpaceExW(
                wide_path.as_ptr(),
                &mut free_bytes_available,
                &mut total_bytes,
                &mut total_free_bytes,
            ) != 0
            {
                return free_bytes_available;
            }
        }
        // fallback: assume enough space if call fails
        u64::MAX
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows fallback: assume enough space
        let _ = path;
        u64::MAX
    }
}
