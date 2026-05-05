#![allow(dead_code)]
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Output;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{timeout, Instant};
use tracing::{error, info, warn};

#[cfg(target_os = "windows")]
use super::types::get_window_rect;
use super::types::RecordingConfig;
use crate::utils::ffmpeg::get_ffmpeg_path;

/// Monitor geometry list: (x, y, width, height) per display.
type MonitorList = Arc<Mutex<Vec<(i32, i32, u32, u32)>>>;
const CLIP_EXPORT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SEGMENT_VERIFY_TIMEOUT: Duration = Duration::from_secs(20);

#[cfg(target_os = "windows")]
use crate::recording::wasapi_audio::WasapiCapture;

/// Returns (offset_x, offset_y, width, height) for the given monitor index.
/// For index 0 (primary), returns (0, 0, 0, 0) meaning no explicit offset is needed.
/// For secondary monitors, returns the virtual-desktop offset obtained from the
/// Win32 MONITORINFO API. Falls back to primary if the index is out of range.
#[cfg(target_os = "windows")]
fn get_monitor_offset(index: u32) -> (i32, i32, u32, u32) {
    if index == 0 {
        return (0, 0, 0, 0);
    }

    // Collect all monitor rects via EnumDisplayMonitors callback.
    // The callback stores MONITORINFO for each monitor in insertion order
    // (which matches the EnumDisplayMonitors enumeration order on Windows).

    #[allow(non_snake_case)]
    extern "system" fn enum_proc(
        _hmonitor: isize,
        _hdc: isize,
        lprect: *mut [i32; 4],
        lparam: isize,
    ) -> i32 {
        let monitors = unsafe { &*(lparam as *const Mutex<Vec<(i32, i32, u32, u32)>>) };
        if let Ok(mut list) = monitors.lock() {
            let r = unsafe { &*lprect };
            let x = r[0];
            let y = r[1];
            let w = (r[2] - r[0]).unsigned_abs();
            let h = (r[3] - r[1]).unsigned_abs();
            list.push((x, y, w, h));
        }
        1 // continue enumeration
    }

    let monitors: MonitorList = Arc::new(Mutex::new(Vec::new()));
    let monitors_ptr = Arc::as_ptr(&monitors) as isize;

    // Safety: EnumDisplayMonitors is a standard Win32 API. The callback only
    // dereferences `lparam` which points to our Mutex on the stack — valid for
    // the entire duration of the call.
    #[link(name = "user32")]
    extern "system" {
        fn EnumDisplayMonitors(
            hdc: isize,
            lprcclip: isize,
            lpfnenumproc: extern "system" fn(isize, isize, *mut [i32; 4], isize) -> i32,
            dwdata: isize,
        ) -> i32;
    }

    unsafe {
        EnumDisplayMonitors(0, 0, enum_proc, monitors_ptr);
    }

    let list = monitors.lock().unwrap();
    if let Some(&(x, y, w, h)) = list.get(index as usize) {
        tracing::info!("Monitor {}: offset=({},{}) size={}x{}", index, x, y, w, h);
        (x, y, w, h)
    } else {
        tracing::warn!(
            "Monitor index {} out of range ({} monitors found), using primary",
            index,
            list.len()
        );
        (0, 0, 0, 0)
    }
}

/// FFmpeg 세그먼트 기반 녹화기
/// 세그먼트 muxer를 사용하여 순환 버퍼 구현
pub struct SegmentRecorder {
    pub(super) config: RecordingConfig,
    pub(super) ffmpeg_process: Option<tokio::process::Child>,
    pub(super) segment_dir: PathBuf,
    pub(super) segment_pattern: String,
    pub(super) start_time: Option<Instant>,
    /// WASAPI loopback audio capture (Windows only)
    #[cfg(target_os = "windows")]
    pub(super) wasapi: Option<WasapiCapture>,
    /// Task 28: Whether a crash-restart has already been attempted (prevent infinite loops)
    pub(super) restart_attempted: bool,
}

impl SegmentRecorder {
    pub fn new(config: RecordingConfig) -> Result<Self> {
        let segment_dir = config.output_dir.join("segments");
        std::fs::create_dir_all(&segment_dir)?;

        let segment_pattern = segment_dir
            .join("segment_%03d.mp4")
            .to_string_lossy()
            .to_string();

        Ok(Self {
            config,
            ffmpeg_process: None,
            segment_dir,
            segment_pattern,
            start_time: None,
            #[cfg(target_os = "windows")]
            wasapi: None,
            restart_attempted: false,
        })
    }

    /// 세그먼트 기반 녹화 시작
    pub async fn start(&mut self) -> Result<()> {
        self.restart_attempted = false;
        self.start_internal(true).await
    }

    /// Internal FFmpeg launch shared by `start()` and crash-recovery restart.
    ///
    /// When `cleanup` is `true` the existing segments are deleted first (normal start).
    /// When `false` they are preserved so a crash-recovery restart can continue the buffer.
    async fn start_internal(&mut self, cleanup: bool) -> Result<()> {
        if self.ffmpeg_process.is_some() {
            anyhow::bail!("이미 녹화가 진행 중입니다");
        }

        if cleanup {
            self.cleanup_old_segments()?;
        }

        let ffmpeg_path = get_ffmpeg_path().context("FFmpeg를 찾을 수 없습니다")?;

        let mut cmd = tokio::process::Command::new(&ffmpeg_path);

        cmd.arg("-y");

        #[allow(unused_assignments)]
        let mut has_audio = false;

        // Record time before any capture backends start for audio-video sync analysis
        let video_start_instant = std::time::Instant::now();

        #[cfg(target_os = "windows")]
        {
            cmd.arg("-f").arg("gdigrab");
            cmd.arg("-framerate").arg(self.config.fps.to_string());

            // HWND-based capture: use `-i desktop` with window rect offsets.
            // This avoids the alt-tab bug where title-based capture grabs
            // whichever window has focus instead of the LoL window.
            if let Some(hwnd) = self.config.capture_hwnd {
                if let Some((x, y, w, h)) = get_window_rect(hwnd) {
                    cmd.arg("-offset_x").arg(x.to_string());
                    cmd.arg("-offset_y").arg(y.to_string());
                    cmd.arg("-video_size").arg(format!("{}x{}", w, h));
                    info!("HWND capture: 0x{:X} at ({},{}) {}x{}", hwnd, x, y, w, h);
                    cmd.arg("-i").arg("desktop");
                } else {
                    // HWND became invalid; fall back to title-based capture
                    warn!(
                        "HWND 0x{:X} is no longer valid, falling back to title capture",
                        hwnd
                    );
                    let title = super::types::get_league_capture_title();
                    cmd.arg("-i").arg(format!("title={}", title));
                }
            } else if let Some(ref target) = self.config.capture_target {
                let safe_target = if target.contains("://")
                    || target.starts_with("pipe:")
                    || target.starts_with("tcp:")
                    || target.starts_with("http:")
                    || target.starts_with("smb:")
                {
                    warn!("Unsafe capture_target rejected: {}", target);
                    super::types::get_league_capture_title()
                } else {
                    target.clone()
                };
                cmd.arg("-i").arg(format!("title={}", safe_target));
            } else {
                // Apply multi-monitor offset if a secondary monitor is requested
                let monitor_index = self.config.monitor_index.unwrap_or(0);
                if monitor_index > 0 {
                    let (offset_x, offset_y, mon_w, mon_h) = get_monitor_offset(monitor_index);
                    if mon_w > 0 && mon_h > 0 {
                        cmd.arg("-offset_x").arg(offset_x.to_string());
                        cmd.arg("-offset_y").arg(offset_y.to_string());
                        cmd.arg("-video_size").arg(format!("{}x{}", mon_w, mon_h));
                        info!(
                            "Capturing monitor {} at offset ({},{}) size {}x{}",
                            monitor_index, offset_x, offset_y, mon_w, mon_h
                        );
                    }
                }
                cmd.arg("-i").arg("desktop");
            }

            if let Some(ref audio_cfg) = self.config.audio_config {
                if audio_cfg.record_system_audio {
                    // Try WASAPI loopback first (works on all modern Windows PCs)
                    let mut wasapi_started = false;
                    let mut wasapi = WasapiCapture::new(&self.segment_dir).ok();
                    if let Some(ref mut w) = wasapi {
                        match w.start() {
                            Ok(()) => {
                                wasapi_started = true;
                                let audio_offset_ms = video_start_instant.elapsed().as_millis();
                                info!(
                                    "WASAPI loopback audio capture started successfully \
                                     ({}ms after FFmpeg video start)",
                                    audio_offset_ms
                                );
                            }
                            Err(e) => {
                                warn!("WASAPI loopback failed, falling back to DirectShow: {}", e);
                                wasapi = None;
                            }
                        }
                    }
                    self.wasapi = wasapi;

                    // Fall back to DirectShow if WASAPI failed
                    if !wasapi_started {
                        let device_name = audio_cfg.system_audio_device.clone().or_else(|| {
                            crate::recording::audio::list_audio_devices_ffmpeg()
                                .ok()
                                .and_then(|devices| {
                                    devices
                                        .into_iter()
                                        .find(|d| {
                                            d.device_type
                                                == crate::recording::audio::AudioDeviceType::SystemAudio
                                        })
                                        .map(|d| d.name)
                                })
                        });

                        if let Some(ref device) = device_name {
                            cmd.arg("-f").arg("dshow");
                            cmd.arg("-i").arg(format!("audio={}", device));
                            has_audio = true;
                            info!("DirectShow 오디오 디바이스 사용: {}", device);
                        } else {
                            warn!("사용 가능한 시스템 오디오 디바이스가 없습니다. 비디오만 녹화합니다.");
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            cmd.arg("-f").arg("avfoundation");
            cmd.arg("-framerate").arg(self.config.fps.to_string());
            cmd.arg("-i").arg("1:0");
            has_audio = self.config.audio_config.is_some();
        }

        #[cfg(target_os = "linux")]
        {
            cmd.arg("-f").arg("x11grab");
            cmd.arg("-framerate").arg(self.config.fps.to_string());
            cmd.arg("-video_size").arg(format!(
                "{}x{}",
                self.config.resolution.0, self.config.resolution.1
            ));
            cmd.arg("-i").arg(":0.0");
        }

        let encoder = self
            .config
            .encoder
            .to_ffmpeg_name_with_hw(self.config.hw_accel);
        cmd.arg("-c:v").arg(encoder);
        cmd.arg("-b:v")
            .arg(format!("{}k", self.config.bitrate / 1000));

        if encoder.contains("nvenc") {
            cmd.arg("-preset").arg("p1");
            cmd.arg("-tune").arg("ll");
        } else if encoder.contains("qsv") {
            cmd.arg("-preset").arg("veryfast");
        } else if encoder.contains("amf") {
            cmd.arg("-preset").arg("speed");
        } else {
            cmd.arg("-preset").arg("ultrafast");
            cmd.arg("-tune").arg("zerolatency");
        }
        cmd.arg("-g").arg((self.config.fps * 2).to_string());
        cmd.arg("-keyint_min").arg(self.config.fps.to_string());
        cmd.arg("-sc_threshold").arg("0");
        cmd.arg("-pix_fmt").arg("yuv420p");

        if has_audio {
            if let Some(ref audio_cfg) = self.config.audio_config {
                cmd.arg("-c:a").arg("aac");
                cmd.arg("-b:a").arg(format!("{}k", audio_cfg.bitrate));
                // WASAPI captures at the device's native sample rate (commonly 48000 or 44100 Hz).
                // Explicitly passing -ar 48000 ensures FFmpeg resamples to a consistent rate
                // regardless of the capture device configuration, preventing A/V sync drift
                // and downstream muxing issues.
                let target_sample_rate = if audio_cfg.sample_rate == 44100 {
                    tracing::info!(
                        "Audio sample rate: device reported 44100Hz, \
                         resampling to 48000Hz for consistent output"
                    );
                    48000
                } else {
                    tracing::info!(
                        "Audio sample rate: using FFmpeg resampling to {}Hz",
                        audio_cfg.sample_rate
                    );
                    audio_cfg.sample_rate
                };
                cmd.arg("-ar").arg(target_sample_rate.to_string());
            }
        }

        let max_segments =
            (self.config.buffer_duration_secs / self.config.segment_duration_secs) as i32;

        cmd.arg("-f").arg("segment");
        cmd.arg("-segment_time")
            .arg(self.config.segment_duration_secs.to_string());
        cmd.arg("-segment_wrap").arg(max_segments.to_string());
        cmd.arg("-segment_format").arg("mp4");
        cmd.arg("-reset_timestamps").arg("1");
        cmd.arg("-strftime").arg("0");
        cmd.arg(&self.segment_pattern);

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000);
        }

        info!("FFmpeg 녹화 시작: {:?}", cmd);

        let mut child = cmd.spawn().context("FFmpeg 프로세스 시작 실패")?;

        // Take stderr handle before storing child to prevent pipe buffer deadlock
        let stderr = child.stderr.take();
        self.ffmpeg_process = Some(child);
        self.start_time = Some(Instant::now());

        // Task 33: Spawn a bounded stderr reader.
        // Limits memory growth: stops after MAX_LINES lines.
        // Only logs lines containing "error"/"warning" at info level; everything else at trace.
        //
        // Task 28: When the stderr pipe closes before recording is intentionally stopped,
        // that signals an unexpected FFmpeg exit (crash). We emit a warning here so callers
        // can detect it. The `monitor_ffmpeg_health()` method provides the restart logic.
        let is_cleanup = cleanup;
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                const MAX_LINES: usize = 10_000;
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                let mut line_count: usize = 0;
                let mut saw_error = false;
                let mut frame_log_counter: u64 = 0;
                while let Ok(Some(line)) = lines.next_line().await {
                    line_count += 1;
                    if line_count > MAX_LINES {
                        warn!(
                            "FFmpeg stderr exceeded {} lines; stopping reader to prevent unbounded memory growth",
                            MAX_LINES
                        );
                        break;
                    }
                    // Task 61: Parse FFmpeg progress output for quality metrics
                    if line.contains("frame=") {
                        frame_log_counter += 1;
                        // Log every 1000 frames to avoid noise
                        if frame_log_counter.is_multiple_of(1000) {
                            if let Some(fps_str) = line.split("fps=").nth(1) {
                                if let Ok(fps) = fps_str
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or("0")
                                    .parse::<f64>()
                                {
                                    let expected_fps = 60.0_f64;
                                    if fps > 0.0 && fps < expected_fps * 0.9 {
                                        tracing::warn!(
                                            actual_fps = fps,
                                            expected_fps,
                                            "Frame rate below target"
                                        );
                                    } else if fps > 0.0 {
                                        tracing::info!(
                                            actual_fps = fps,
                                            frame_count = frame_log_counter,
                                            "Recording quality metrics"
                                        );
                                    }
                                }
                            }
                        }
                        tracing::trace!("FFmpeg progress: {}", line);
                    } else if line.contains("error")
                        || line.contains("Error")
                        || line.contains("warning")
                        || line.contains("Warning")
                    {
                        info!("FFmpeg: {}", line);
                        if line.to_ascii_lowercase().contains("error") {
                            saw_error = true;
                        }
                    } else {
                        tracing::trace!("FFmpeg: {}", line);
                    }
                }
                // Task 28: Pipe closed — FFmpeg has exited. If this was a crash-recovery
                // restart (cleanup=false) and we saw errors, the second attempt also failed.
                // Either way, warn so the health monitor (callers of monitor_ffmpeg_health)
                // can detect the exit and attempt recovery within the 5-second spec window.
                if saw_error || !is_cleanup {
                    warn!(
                        "FFmpeg stderr pipe closed unexpectedly (cleanup={}). \
                         Call monitor_ffmpeg_health() to attempt crash recovery.",
                        is_cleanup
                    );
                } else {
                    info!("FFmpeg stderr monitor exited");
                }
            });
        }

        info!("세그먼트 기반 녹화 시작됨: {}", self.segment_dir.display());
        Ok(())
    }

    /// 녹화 중지
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut process) = self.ffmpeg_process.take() {
            #[cfg(target_os = "windows")]
            {
                let _ = process.kill().await;
            }

            #[cfg(not(target_os = "windows"))]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                if let Some(pid) = process.id() {
                    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGINT);
                }
            }

            match tokio::time::timeout(std::time::Duration::from_secs(5), process.wait()).await {
                Ok(Ok(_)) => {
                    info!("FFmpeg 녹화 정상 중지됨");
                }
                Ok(Err(e)) => {
                    warn!("FFmpeg 프로세스 종료 오류: {}", e);
                }
                Err(_) => {
                    warn!("FFmpeg 프로세스 종료 타임아웃 (5초) - 강제 종료 시도");
                    let _ = process.kill().await;
                }
            }
        }

        // Stop WASAPI capture if active
        #[cfg(target_os = "windows")]
        {
            if let Some(ref mut wasapi) = self.wasapi {
                if let Some(wav_path) = wasapi.stop() {
                    info!("WASAPI loopback audio saved: {}", wav_path.display());
                }
            }
            self.wasapi = None;
        }

        self.start_time = None;
        Ok(())
    }

    /// Task 28: Poll whether FFmpeg has exited unexpectedly and attempt ONE restart.
    ///
    /// Existing segments are preserved on restart (no `cleanup_old_segments` call).
    /// Returns `true` if a restart was triggered, `false` if FFmpeg is still running
    /// or no action could be taken.
    pub async fn monitor_ffmpeg_health(&mut self) -> bool {
        let process = match self.ffmpeg_process.as_mut() {
            Some(p) => p,
            None => return false,
        };

        // try_wait: Ok(None) = still running, Ok(Some(status)) = exited
        match process.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    warn!(
                        "FFmpeg exited unexpectedly with status: {:?}. Segments preserved.",
                        status
                    );
                } else {
                    info!("FFmpeg exited (success) during monitoring — may indicate end of input.");
                }

                self.ffmpeg_process = None;

                if self.restart_attempted {
                    warn!("FFmpeg crash recovery: restart already attempted once, not retrying.");
                    return false;
                }

                // Attempt ONE restart; preserve existing segments (cleanup=false)
                self.restart_attempted = true;
                info!("FFmpeg crash recovery: attempting restart (attempt 1/1)...");
                match self.start_internal(false).await {
                    Ok(()) => {
                        info!("FFmpeg crash recovery: restart succeeded.");
                        true
                    }
                    Err(e) => {
                        error!("FFmpeg crash recovery: restart failed: {}", e);
                        false
                    }
                }
            }
            Ok(None) => false, // still running — nothing to do
            Err(e) => {
                warn!("FFmpeg health check error: {}", e);
                false
            }
        }
    }

    /// 특정 시간 범위의 클립 저장
    pub async fn save_clip(
        &self,
        output_path: &PathBuf,
        start_offset_secs: f64,
        duration_secs: f64,
    ) -> Result<PathBuf> {
        info!(
            "클립 저장 시작: {} (offset: {}s, duration: {}s)",
            output_path.display(),
            start_offset_secs,
            duration_secs
        );

        let segments = self.get_sorted_segments()?;

        if segments.is_empty() {
            anyhow::bail!("저장된 세그먼트가 없습니다");
        }

        let ffmpeg_path = get_ffmpeg_path()?;
        let mut verified_segments = Vec::with_capacity(segments.len());

        for segment in segments {
            match std::fs::metadata(&segment) {
                Ok(metadata) if metadata.len() == 0 => {
                    warn!("빈 세그먼트 파일 제외: {}", segment.display());
                    continue;
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(
                        "세그먼트 메타데이터 확인 실패로 제외: {} ({})",
                        segment.display(),
                        e
                    );
                    continue;
                }
            }

            if verify_segment(&segment, &ffmpeg_path).await {
                verified_segments.push(segment);
            } else {
                warn!("손상된 세그먼트 파일 제외: {}", segment.display());
            }
        }

        if verified_segments.is_empty() {
            anyhow::bail!("저장 가능한 세그먼트가 없습니다. 녹화 파일이 아직 준비되지 않았거나 손상되었습니다");
        }

        let concat_file = self.segment_dir.join("concat_list.txt");
        let mut concat_content = String::new();
        for segment in &verified_segments {
            concat_content.push_str(&format!("file '{}'\n", segment.display()));
        }
        tokio::fs::write(&concat_file, &concat_content).await?;

        let mut cmd = tokio::process::Command::new(&ffmpeg_path);

        cmd.arg("-y");
        cmd.arg("-f").arg("concat");
        cmd.arg("-safe").arg("0");
        cmd.arg("-i").arg(&concat_file);

        // Add WASAPI loopback audio if available
        #[cfg(target_os = "windows")]
        let has_wasapi_audio = {
            let wav_path = self.segment_dir.join("wasapi_loopback.wav");
            if wav_path.exists() {
                cmd.arg("-i").arg(&wav_path);
                info!("Muxing WASAPI loopback audio: {}", wav_path.display());
                true
            } else {
                false
            }
        };

        cmd.arg("-ss").arg(start_offset_secs.to_string());
        cmd.arg("-t").arg(duration_secs.to_string());

        #[cfg(target_os = "windows")]
        {
            if has_wasapi_audio {
                // Map video from concat input, audio from WAV input
                cmd.arg("-map").arg("0:v");
                cmd.arg("-map").arg("1:a");
                cmd.arg("-c:v").arg("copy");
                cmd.arg("-c:a").arg("aac");
                cmd.arg("-b:a").arg("192k");
            } else {
                cmd.arg("-c").arg("copy");
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            cmd.arg("-c").arg("copy");
        }

        cmd.arg("-movflags").arg("+faststart");
        cmd.arg(output_path);

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000);
        }

        let output =
            run_command_with_timeout(&mut cmd, CLIP_EXPORT_TIMEOUT, "FFmpeg clip export").await?;

        let _ = tokio::fs::remove_file(&concat_file).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("클립 저장 실패: {}", stderr);
            anyhow::bail!("클립 저장 실패: {}", stderr);
        }

        info!("클립 저장 완료: {}", output_path.display());
        Ok(output_path.clone())
    }

    /// 세그먼트 파일 정렬된 목록 반환
    /// FIX #7: Sort by file modification time instead of segment number.
    /// With segment_wrap, numeric order gives wrong temporal order because
    /// segment numbers wrap around (e.g., 5, 0, 1, 2 instead of 0, 1, 2, 5).
    fn get_sorted_segments(&self) -> Result<Vec<PathBuf>> {
        let mut segments: Vec<PathBuf> = std::fs::read_dir(&self.segment_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().map(|ext| ext == "mp4").unwrap_or(false))
            .collect();

        segments.sort_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });

        Ok(segments)
    }

    /// 기존 세그먼트 파일 정리
    fn cleanup_old_segments(&self) -> Result<()> {
        if self.segment_dir.exists() {
            for entry in std::fs::read_dir(&self.segment_dir)?.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "mp4").unwrap_or(false) {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        Ok(())
    }

    /// 녹화 진행 시간 반환
    pub fn get_elapsed_secs(&self) -> f64 {
        self.start_time
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }

    /// 녹화 중인지 확인
    #[allow(dead_code)]
    pub fn is_recording(&self) -> bool {
        self.ffmpeg_process.is_some()
    }
}

/// Verify a segment file is not corrupt by running a null-output FFmpeg decode pass.
///
/// Uses FFmpeg's `-f null -` output to decode the file without writing anything.
/// Any decode errors are reported via FFmpeg's stderr exit status.
///
/// Returns `true` if the segment passes the integrity check, `false` otherwise.
///
/// Used before clip export so one corrupt or half-written segment does not poison
/// the whole concat job.
pub async fn verify_segment(segment_path: &std::path::Path, ffmpeg_path: &std::path::Path) -> bool {
    let mut command = tokio::process::Command::new(ffmpeg_path);
    command
        .args(["-v", "error", "-i"])
        .arg(segment_path)
        .args(["-f", "null", "-"])
        .kill_on_drop(true);

    let result = run_command_with_timeout(
        &mut command,
        SEGMENT_VERIFY_TIMEOUT,
        "FFmpeg segment verification",
    )
    .await;

    match result {
        Ok(output) => {
            if output.status.success() {
                tracing::debug!("Segment integrity check passed: {}", segment_path.display());
                true
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::warn!(
                    "Segment failed integrity check: {} — FFmpeg errors: {}",
                    segment_path.display(),
                    stderr.trim()
                );
                false
            }
        }
        Err(e) => {
            tracing::error!(
                "Segment verification could not run for {}: {}",
                segment_path.display(),
                e
            );
            false
        }
    }
}

async fn run_command_with_timeout(
    command: &mut tokio::process::Command,
    timeout_after: Duration,
    operation: &str,
) -> Result<Output> {
    command.kill_on_drop(true);

    match timeout(timeout_after, command.output()).await {
        Ok(result) => result.with_context(|| format!("{} failed to run", operation)),
        Err(_) => anyhow::bail!(
            "{} timed out after {} seconds",
            operation,
            timeout_after.as_secs()
        ),
    }
}

/// Monitor disk space during a recording session.
/// Warns at < 1 GB free, emits a critical event at < 500 MB free.
/// Stops when `cancel` is set to `true`.
pub async fn monitor_disk_space(
    app_handle: tauri::AppHandle,
    recordings_dir: std::path::PathBuf,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) {
    use tauri::Emitter;

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    break;
                }
            }
        }

        if *cancel.borrow() {
            break;
        }

        // Use sysinfo to get available disk space for the recordings directory
        let available_mb = get_available_mb(&recordings_dir);

        const WARN_THRESHOLD_MB: u64 = 1024; // 1 GB
        const CRIT_THRESHOLD_MB: u64 = 512; // 500 MB

        if available_mb < CRIT_THRESHOLD_MB {
            tracing::error!(
                "Disk space critically low: {}MB available on recording volume",
                available_mb
            );
            let _ = app_handle.emit("disk-critical", available_mb);
        } else if available_mb < WARN_THRESHOLD_MB {
            warn!(
                "Disk space low: {}MB available on recording volume",
                available_mb
            );
            let _ = app_handle.emit("disk-warning", available_mb);
        } else {
            tracing::debug!("Disk space OK: {}MB available", available_mb);
        }
    }
}

/// Returns available disk space in MB for the given path, or u64::MAX on error.
fn get_available_mb(path: &std::path::Path) -> u64 {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();
    let path_str = path.to_string_lossy().to_lowercase();

    // Find the disk whose mount point is the longest prefix of our path
    // (most specific match)
    let best = disks
        .iter()
        .filter(|d| {
            let mount = d.mount_point().to_string_lossy().to_lowercase();
            path_str.starts_with(mount.as_str())
        })
        .max_by_key(|d| d.mount_point().to_string_lossy().len());

    if let Some(disk) = best {
        disk.available_space() / (1024 * 1024)
    } else {
        // Fallback: return the first disk's available space
        disks
            .iter()
            .next()
            .map(|d| d.available_space() / (1024 * 1024))
            .unwrap_or(u64::MAX)
    }
}

impl Drop for SegmentRecorder {
    fn drop(&mut self) {
        // Stop WASAPI capture on drop
        #[cfg(target_os = "windows")]
        {
            if let Some(ref mut wasapi) = self.wasapi {
                let _ = wasapi.stop();
            }
            self.wasapi = None;
        }

        if let Some(mut process) = self.ffmpeg_process.take() {
            #[cfg(target_os = "windows")]
            {
                let _ = process.start_kill();
            }
            #[cfg(not(target_os = "windows"))]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                if let Some(pid) = process.id() {
                    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGINT);
                }
            }
            info!("SegmentRecorder dropped, FFmpeg process killed");
        }
    }
}
