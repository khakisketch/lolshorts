#![allow(dead_code)]
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock as TokioRwLock;
use tokio::time::Instant;
use tracing::info;

use super::segment_recorder::{now_wall_secs, SegmentRecorder};
use super::types::{CaptureMode, RecordingConfig, RecordingStats, RecordingStatus};
use crate::storage::GameMetadata;

/// Extra slack added to the buffer-coverage wait on top of one segment boundary.
const COVERAGE_WAIT_SLACK: f64 = 2.0;
/// Hard ceiling for the coverage wait so a pathological window can never hang a save.
const MAX_COVERAGE_WAIT_SECS: f64 = 120.0;

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
    /// Cancels the background FFmpeg health-monitor task when recording stops.
    pub(super) health_cancel_tx: Option<tokio::sync::watch::Sender<bool>>,
}

impl WindowsCaptureRecorder {
    pub async fn new(config: RecordingConfig) -> Result<Self> {
        tokio::fs::create_dir_all(&config.output_dir).await?;

        let segment_recorder = SegmentRecorder::new(config.clone())?;

        Ok(Self {
            config,
            status: Arc::new(TokioRwLock::new(RecordingStatus::Idle)),
            segment_recorder: Arc::new(TokioRwLock::new(segment_recorder)),
            current_game: Arc::new(TokioRwLock::new(None)),
            start_time: Arc::new(TokioRwLock::new(None)),
            total_frames: Arc::new(TokioRwLock::new(0)),
            health_cancel_tx: None,
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
        let free_bytes = get_free_disk_space(&self.config.output_dir).ok_or_else(|| {
            anyhow::anyhow!(
                "녹화 드라이브의 여유 공간을 확인할 수 없습니다. 저장 위치와 드라이브 연결 상태를 확인한 뒤 다시 시도하세요."
            )
        })?;
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

        // `start()` now verifies FFmpeg is still alive ~1.2s after spawn, so a bad
        // capture rect / encoder fails HERE instead of pretending to record. Roll the
        // status back to Idle on failure, otherwise it would stay stuck on Buffering
        // and every retry would bail with "이미 녹화가 진행 중입니다".
        {
            let mut recorder = self.segment_recorder.write().await;
            if let Err(e) = recorder.start().await {
                drop(recorder);
                *self.status.write().await = RecordingStatus::Idle;
                return Err(e);
            }
        }

        *self.start_time.write().await = Some(Instant::now());
        *self.total_frames.write().await = 0;
        *self.status.write().await = RecordingStatus::Recording;

        // Task 28: start the FFmpeg crash-recovery watchdog. Without this the recovery
        // logic in SegmentRecorder::monitor_ffmpeg_health was never called, so a mid-game
        // FFmpeg crash left status stuck on "Recording" while no new segments were written.
        self.spawn_health_monitor();

        info!("녹화가 성공적으로 시작되었습니다");
        Ok(())
    }

    /// Spawn a background task that periodically checks FFmpeg health and, on an
    /// unexpected exit, attempts ONE restart (preserving segments AND the running WASAPI
    /// audio thread). If FFmpeg is dead and cannot be restarted, the recording status is
    /// transitioned to `Error` so the UI can surface the failure.
    fn spawn_health_monitor(&mut self) {
        // Cancel any previous monitor before starting a new one.
        if let Some(tx) = self.health_cancel_tx.take() {
            let _ = tx.send(true);
        }
        let (tx, mut rx) = tokio::sync::watch::channel(false);
        self.health_cancel_tx = Some(tx);

        let segment_recorder = Arc::clone(&self.segment_recorder);
        let status = Arc::clone(&self.status);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = rx.changed() => {
                        if *rx.borrow() {
                            break;
                        }
                    }
                }
                if *rx.borrow() {
                    break;
                }

                // Only monitor while actively recording; stop once it ends elsewhere.
                let current = *status.read().await;
                match current {
                    RecordingStatus::Recording | RecordingStatus::Buffering => {}
                    RecordingStatus::Idle | RecordingStatus::Error => break,
                    RecordingStatus::Processing => continue,
                }

                let (restarted, still_alive) = {
                    let mut recorder = segment_recorder.write().await;
                    let restarted = recorder.monitor_ffmpeg_health().await;
                    (restarted, recorder.is_recording())
                };

                if restarted {
                    tracing::warn!(
                        "FFmpeg crash detected — recording restarted (segments & audio preserved)"
                    );
                }

                if !still_alive {
                    tracing::error!(
                        "FFmpeg process is dead and could not be restarted; marking recording as Error"
                    );
                    *status.write().await = RecordingStatus::Error;
                    break;
                }
            }
            tracing::debug!("FFmpeg health monitor task exiting");
        });
    }

    /// 녹화 중지
    pub async fn stop_recording(&mut self) -> Result<PathBuf> {
        // Stop the health watchdog first so it cannot restart FFmpeg mid-stop.
        if let Some(tx) = self.health_cancel_tx.take() {
            let _ = tx.send(true);
        }

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

    /// 마지막 N초 클립 저장.
    ///
    /// Returns the saved clip path **and its measured duration**. The measured
    /// value is what callers must persist: the rolling buffer may hold less than
    /// `secs` (short session, coverage timeout), in which case the produced file is
    /// shorter than requested and storing the request value would make auto-edit
    /// plan around footage that does not exist.
    pub async fn save_last_seconds(&self, secs: u64) -> Result<(PathBuf, f64), String> {
        let status = *self.status.read().await;
        if status != RecordingStatus::Recording && status != RecordingStatus::Buffering {
            return Err("녹화가 진행 중이 아닙니다".to_string());
        }

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("clip_{}_{}.mp4", timestamp, secs);
        let output_path = self.config.output_dir.join("clips").join(&filename);

        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }

        // Offset/duration are computed against the actual rolling-buffer timeline
        // inside save_clip; we only pass the requested window length.
        //
        // The recorder guard is released BEFORE the extraction runs: an export holds no
        // recorder state (it works off the snapshot) and can take minutes, during which
        // the health monitor's 5-second `write()` would otherwise park every status/stats
        // read behind it — tokio's RwLock is write-preferring.
        let ctx = {
            let recorder = self.segment_recorder.read().await;
            recorder.extraction_context()
        };
        ctx.save_clip(&output_path, secs as f64)
            .await
            .map_err(|e| e.to_string())
    }

    /// 특정 이벤트 시점 기준 클립 저장.
    ///
    /// `event_wall_secs` is the WALL-CLOCK instant (seconds since the UNIX epoch) at
    /// which the event was DETECTED — `AutoClipManager` stamps it the moment the Live
    /// Client poll surfaces the event. The clip window is therefore the explicit
    /// `[event − pre, event + post]` range.
    ///
    /// It used to be ignored (`_event_time_secs`) and every clip was simply "the last
    /// pre+post seconds ending now", so the clip drifted by however long queueing,
    /// merging, lock contention and the post-event wait had taken — for a merged window
    /// that is easily 30+ seconds past the play the user wanted.
    ///
    /// Returns `(clip_path, actual_duration_secs)`. The duration is MEASURED on the
    /// produced file, not `pre_secs + post_secs`: whenever the rolling buffer could not
    /// cover the window the export is clamped to a shorter clip, and storing the request
    /// instead of the result made auto-edit trim against footage that does not exist.
    pub async fn save_event_clip(
        &self,
        event_wall_secs: f64,
        pre_secs: f64,
        post_secs: f64,
        clip_id: &str,
    ) -> Result<(PathBuf, f64)> {
        let status = *self.status.read().await;
        if status != RecordingStatus::Recording && status != RecordingStatus::Buffering {
            anyhow::bail!("녹화가 진행 중이 아닙니다");
        }

        let total_duration = pre_secs + post_secs;
        let end_anchor = event_wall_secs + post_secs;

        let filename = format!("{}.mp4", clip_id);
        let output_path = self.config.output_dir.join("clips").join(&filename);

        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Budget for the buffer to reach the end of the window: whatever post-event time
        // is still in the future, plus one segment boundary (the in-progress segment has
        // no moov atom and is unusable), plus slack.
        let coverage_timeout = Duration::from_secs_f64(
            ((end_anchor - now_wall_secs()).max(0.0)
                + self.config.segment_duration_secs as f64
                + COVERAGE_WAIT_SLACK)
                .clamp(0.0, MAX_COVERAGE_WAIT_SECS),
        );

        // Snapshot under the guard, extract without it — the coverage wait (up to 120s),
        // the per-segment verify pass and the export itself must not keep the recorder
        // lock, or the health monitor's periodic `write()` freezes every status poll for
        // the whole save (see `ClipExtractionContext`).
        let ctx = {
            let recorder = self.segment_recorder.read().await;
            recorder.extraction_context()
        };
        ctx.save_clip_anchored(
            &output_path,
            total_duration,
            Some(end_anchor),
            coverage_timeout,
        )
        .await
    }

    pub async fn get_status(&self) -> RecordingStatus {
        *self.status.read().await
    }

    /// Runtime-only capture diagnostics for the settings status dashboard.
    pub async fn get_capture_diagnostics(
        &self,
    ) -> (
        Option<CaptureMode>,
        Option<super::types::CaptureBackend>,
        Option<String>,
    ) {
        self.segment_recorder.read().await.capture_diagnostics()
    }

    pub async fn get_stats(&self) -> RecordingStats {
        let start_time = *self.start_time.read().await;
        let uptime = start_time.map(|t| t.elapsed().as_secs_f64());

        // Real measured values from FFmpeg progress output — do NOT fabricate the
        // configured fps (the old code returned config.fps as if it were measured).
        let (total_frames, audio_active, mic_active) = {
            let recorder = self.segment_recorder.read().await;
            (
                recorder.frame_count(),
                recorder.system_audio_active(),
                recorder.mic_active(),
            )
        };

        RecordingStats {
            total_frames,
            uptime_seconds: uptime.unwrap_or(0.0),
            current_fps: match uptime {
                Some(u) if u > 0.0 => total_frames as f64 / u,
                _ => 0.0,
            },
            audio_active,
            mic_active,
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

fn get_free_disk_space(path: &Path) -> Option<u64> {
    crate::utils::disk::query_disk_space(path)
        .ok()
        .map(|snapshot| snapshot.available_bytes)
}
