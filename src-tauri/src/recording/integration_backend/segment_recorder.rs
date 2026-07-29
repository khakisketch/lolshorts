#![allow(dead_code)]
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Output;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{timeout, Instant};
use tracing::{error, info, warn};

#[cfg(target_os = "windows")]
use super::types::get_window_rect;
use super::types::RecordingConfig;
use crate::utils::ffmpeg::{get_ffmpeg_path, get_ffprobe_path};

/// Monitor geometry list: (x, y, width, height) per display.
type MonitorList = Arc<Mutex<Vec<(i32, i32, u32, u32)>>>;
const CLIP_EXPORT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SEGMENT_VERIFY_TIMEOUT: Duration = Duration::from_secs(20);
/// ffprobe only reads the container header here, so this is generous on purpose.
const SEGMENT_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// How long to let FFmpeg run before deciding the launch actually succeeded.
/// Argument/device errors surface within a few hundred ms.
const START_HEALTH_PROBE_DELAY: Duration = Duration::from_millis(1200);
/// Number of trailing stderr lines kept for start-up failure diagnostics.
const STDERR_TAIL_LINES: usize = 20;
/// Polling interval while waiting for the rolling buffer to cover a clip window.
const COVERAGE_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Shortest window that still counts as a clip.
///
/// When the requested window falls outside the rolling buffer `compute_clip_window`
/// returns `duration == 0`, and the export used to be forced to `-t 0.1`: FFmpeg then
/// exits 0, the save "succeeds", and the caller writes a perfectly normal-looking
/// metadata row for a file that is not watchable. Anything below this threshold is
/// therefore rejected BEFORE a file is created.
const MIN_CLIP_DURATION_SECS: f64 = 1.0;
/// Length differences below this are keyframe-boundary noise (`-c copy` can only cut on
/// a keyframe) or float rounding, not a real shortfall.
const CLIP_LENGTH_TOLERANCE_SECS: f64 = 0.25;
/// A produced clip shorter than this fraction of the request is reported as a SEVERE
/// shortfall (error level) instead of an ordinary buffer clamp.
///
/// It stays a warning rather than a hard failure on purpose: a legitimately short buffer
/// (F9 "save last 60s" pressed 20s into a session) must still produce the clip the user
/// asked for. The caller persists the MEASURED length returned by `save_clip_anchored`,
/// so the shortfall reaches metadata either way — this only decides how loudly it is
/// logged.
const SEVERE_SHORTFALL_RATIO: f64 = 0.5;

/// Rolling tail of FFmpeg stderr lines shared with the stderr reader task.
type StderrTail = Arc<Mutex<VecDeque<String>>>;

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
    /// Microphone capture (Windows only). Independent of `wasapi`: recorded to a
    /// separate `mic_capture.wav` and muxed as a second audio input (amix) in
    /// `save_clip`. `Some` only while a mic capture is active this session.
    #[cfg(target_os = "windows")]
    pub(super) mic: Option<WasapiCapture>,
    /// Task 28: Whether a crash-restart has already been attempted (prevent infinite loops)
    pub(super) restart_attempted: bool,
    /// Wall-clock start of the WASAPI wav (seconds since UNIX epoch) for the CURRENT
    /// session. `Some` iff a WASAPI wav was produced this session — used both to seek
    /// the wav correctly in `save_clip` and to avoid muxing a previous session's stale
    /// wav. Reset to `None` on a fresh start; preserved across crash-recovery restarts.
    pub(super) audio_start_walltime: Option<f64>,
    /// Whether system audio (WASAPI loopback or DirectShow) is actually being captured
    /// this session. Surfaced to the UI so a silent recording can be flagged.
    pub(super) system_audio_active: bool,
    /// Wall-clock start of the microphone wav (seconds since UNIX epoch) for the
    /// CURRENT session. `Some` iff a mic wav was produced this session — used to seek
    /// the (continuous) mic wav in `save_clip` and to avoid muxing a previous
    /// session's stale mic wav. Mirrors `audio_start_walltime` for the mic input.
    pub(super) mic_start_walltime: Option<f64>,
    /// Whether the microphone is actually being captured this session. Surfaced to
    /// the UI alongside `system_audio_active`.
    pub(super) mic_active: bool,
    /// Latest cumulative encoded frame count parsed from FFmpeg progress output.
    /// Powers the real recording FPS stat instead of returning the configured value.
    pub(super) frame_count: Arc<AtomicU64>,
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
            #[cfg(target_os = "windows")]
            mic: None,
            restart_attempted: false,
            audio_start_walltime: None,
            system_audio_active: false,
            mic_start_walltime: None,
            mic_active: false,
            frame_count: Arc::new(AtomicU64::new(0)),
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

            // Remove any stale WASAPI wav left over from a previous session so
            // save_clip can never mux another session's audio, and reset all
            // per-session audio/frame state. (Crash-recovery restarts pass
            // cleanup=false and keep the running capture + its wav.)
            let stale_wav = self.segment_dir.join("wasapi_loopback.wav");
            if stale_wav.exists() {
                if let Err(e) = std::fs::remove_file(&stale_wav) {
                    warn!("Failed to remove stale WASAPI wav: {}", e);
                }
            }
            // Same session-contamination defense for the microphone wav.
            let stale_mic_wav = self.segment_dir.join("mic_capture.wav");
            if stale_mic_wav.exists() {
                if let Err(e) = std::fs::remove_file(&stale_mic_wav) {
                    warn!("Failed to remove stale microphone wav: {}", e);
                }
            }
            self.audio_start_walltime = None;
            self.system_audio_active = false;
            self.mic_start_walltime = None;
            self.mic_active = false;
            self.frame_count.store(0, Ordering::Relaxed);
        }

        let ffmpeg_path = get_ffmpeg_path().context("FFmpeg를 찾을 수 없습니다")?;

        // 창이 쓸 만한 크기가 될 때까지 기다린다.
        //
        // 실게임에서 관측된 실패다(2026-07-29):
        //
        // ```
        // [gdigrab] Found window League of Legends (TM) Client, capturing 1x1x32 at (0,0)
        // [h264_nvenc] InitializeEncoder failed: invalid param (8):
        //              Frame Dimension less than the minimum supported value.
        // ```
        //
        // 게임 프로세스가 뜨는 즉시 녹화를 시작하는데, 그 순간 창은 만들어지기만
        // 하고 크기가 아직 1x1 이다. 사슬은 이렇게 이어졌다: HWND 경로는
        // `even_dimensions(1, 1) == (0, 0)` 이라 **올바르게** 거부하고 title 캡처로
        // 폴백하는데, **폴백 경로에는 크기 검증이 없어서** gdigrab 이 1x1 을 그대로
        // 잡고 인코더가 즉사했다. 재시도가 없어 그 판은 통째로 녹화되지 않았다.
        //
        // 폴백 쪽에 가드를 더하는 대신 여기서 기다리는 이유: 크기를 모른 채
        // 시작할 이유가 없고, 기다리면 두 경로가 모두 안전해진다.
        #[cfg(target_os = "windows")]
        wait_for_capturable_window().await;

        let mut cmd = tokio::process::Command::new(&ffmpeg_path);

        cmd.arg("-y");

        // Emit machine-readable progress (LF-terminated `key=value`) to stdout so
        // the frame counter parses reliably. FFmpeg's stderr stats line is
        // '\r'-terminated (updated in place); tokio's line reader only splits on
        // '\n', so `frame=` never surfaced there and the recording-FPS stat could
        // stay pinned at 0. `-nostats` silences the now-redundant stderr stats to
        // keep the error log readable.
        cmd.arg("-nostats");
        cmd.arg("-progress").arg("pipe:1");

        #[allow(unused_assignments)]
        let mut has_audio = false;

        // Record time before any capture backends start for audio-video sync analysis
        let video_start_instant = std::time::Instant::now();

        #[cfg(target_os = "windows")]
        {
            cmd.arg("-f").arg("gdigrab");
            cmd.arg("-framerate").arg(self.config.fps.to_string());
            // 커서를 합성하지 않는다.
            //
            // gdigrab 의 기본값은 `draw_mouse=1` 이라 매 프레임 GDI 에서 커서를 읽어
            // 합성하는데, 60fps 로 이걸 하면 **실제 화면의 커서가 깜빡이는 것처럼
            // 보인다**(실사용 중 보고됨). 롤은 커서를 계속 움직이는 게임이라 특히
            // 두드러진다. 녹화가 게임 플레이를 방해하는 것은 클립에 커서가 안 남는
            // 것보다 나쁘다.
            cmd.arg("-draw_mouse").arg("0");

            // Re-resolve the League window at every recording start.
            //
            // `config.capture_hwnd` is filled in once, by `RecordingConfig::default()`
            // when the app boots. Launching LoLShorts from the launcher — the normal
            // order — therefore stores the CLIENT's handle ("League of Legends",
            // LeagueClientUx), and the client window stays alive (just hidden) once a
            // match starts. The stored handle keeps returning a valid rect, so capture
            // silently records the client's screen region instead of the game.
            // `find_league_hwnd` tries "League of Legends (TM) Client" first, so
            // resolving here picks the in-game window whenever a match is running and
            // falls back to the boot-time handle otherwise.
            let capture_hwnd = super::types::find_league_hwnd().or(self.config.capture_hwnd);

            if let Some(hwnd) = capture_hwnd {
                // `even_dimensions`: GetWindowRect happily reports an odd width/height
                // (a restored/DPI-scaled window is regularly 1919x1079), and h264 with
                // yuv420p then dies instantly with "width not divisible by 2" — the
                // spawn succeeds, so recording *looks* live while nothing is captured.
                // 인코더가 받아들일 수 있는 최소 크기. 이보다 작으면 title 폴백도
                // 소용이 없다 — 같은 창을 같은 크기로 잡을 뿐이다.
                //
                // 예전에는 `w == 0 || h == 0` 만 보고 title 캡처로 넘겼는데,
                // 폴백 경로에는 크기 검증이 아예 없어서 1x1 창을 그대로 gdigrab 에
                // 넘겼고 NVENC 가 즉사했다(실게임 관측). 재시도가 없어 그 판은
                // 통째로 녹화되지 않았다.
                const MIN_ENCODABLE: u32 = 64;

                let rect = get_window_rect(hwnd).and_then(|(x, y, w, h)| {
                    let (w, h) = even_dimensions(w, h);
                    if w < MIN_ENCODABLE || h < MIN_ENCODABLE {
                        warn!(
                            "HWND 0x{:X} 창이 아직 너무 작습니다({}x{}) — 인코더가 받아들일 수 없습니다.",
                            hwnd, w, h
                        );
                        None
                    } else {
                        Some((x, y, w, h))
                    }
                });

                // 창은 찾았는데 크기가 안 나오면, 제목으로 잡아도 같은 창을 같은
                // 크기로 잡을 뿐이다. 그대로 두면 인코더가 죽고 그 판이 사라진다.
                if rect.is_none() {
                    anyhow::bail!(
                        "League 창이 아직 캡처할 수 있는 크기가 아닙니다(게임 로딩 중일 수 있습니다)."
                    );
                }

                if let Some((x, y, w, h)) = rect {
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
                    // Same yuv420p constraint as the HWND path: a monitor rect can be
                    // odd (rotated/scaled displays), which kills the encoder on spawn.
                    let (mon_w, mon_h) = even_dimensions(mon_w, mon_h);
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
                    if self.wasapi.is_some() {
                        // Crash-recovery restart: the WASAPI capture thread is independent
                        // of the FFmpeg video process and is still running. Re-creating it
                        // would truncate the wav and lose audio, so leave it untouched and
                        // record video-only segments (save_clip re-muxes the preserved wav).
                        info!(
                            "Restart: preserving existing WASAPI capture \
                             (audio thread is independent of FFmpeg)"
                        );
                    } else {
                        // Try WASAPI loopback first (works on all modern Windows PCs).
                        // Honor the configured output device (falls back to default+warn).
                        // audio_device_id (explicit WASAPI device from enumerate_audio_devices())
                        // takes priority over the legacy system_audio_device hint.
                        let device_hint = audio_cfg
                            .audio_device_id
                            .clone()
                            .or_else(|| audio_cfg.system_audio_device.clone());
                        let mut wasapi_started = false;
                        let mut wasapi = WasapiCapture::new(&self.segment_dir, device_hint).ok();
                        if let Some(ref mut w) = wasapi {
                            match w.start() {
                                Ok(()) => {
                                    wasapi_started = true;
                                    // Record the wav's wall-clock start so save_clip can seek
                                    // the (continuous) wav independently of the video buffer.
                                    self.audio_start_walltime = Some(now_wall_secs());
                                    let audio_offset_ms = video_start_instant.elapsed().as_millis();
                                    info!(
                                        "WASAPI loopback audio capture started successfully \
                                         ({}ms after FFmpeg video start)",
                                        audio_offset_ms
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        "WASAPI loopback failed, falling back to DirectShow: {}",
                                        e
                                    );
                                    wasapi = None;
                                }
                            }
                        }
                        self.wasapi = wasapi;

                        if wasapi_started {
                            self.system_audio_active = true;
                        } else {
                            // Fall back to DirectShow if WASAPI failed
                            let device_name =
                                audio_cfg.system_audio_device.clone().or_else(|| {
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
                                self.system_audio_active = true;
                                info!("DirectShow 오디오 디바이스 사용: {}", device);
                            } else {
                                // System audio requested but no capture device available:
                                // surface this so the UI can warn about a silent recording.
                                self.system_audio_active = false;
                                warn!(
                                    "사용 가능한 시스템 오디오 디바이스가 없습니다. \
                                     오디오 없이 비디오만 녹화합니다."
                                );
                            }
                        }
                    }
                }

                // Microphone capture is INDEPENDENT of system audio: start it whenever
                // record_microphone is on, and mix it into the same clip track at save
                // time (amix). Failure NEVER blocks recording — we warn and continue with
                // whatever else is active (system audio only, or video only).
                if audio_cfg.record_microphone {
                    if self.mic.is_some() {
                        // Crash-recovery restart: the mic capture thread is independent of
                        // the FFmpeg video process and is still running. Re-creating it
                        // would truncate the wav, so leave it untouched (save_clip re-muxes
                        // the preserved mic wav).
                        info!("Restart: preserving existing microphone capture");
                    } else {
                        let mut mic = WasapiCapture::new_microphone(
                            &self.segment_dir,
                            audio_cfg.microphone_device.clone(),
                        )
                        .ok();
                        let mut mic_started = false;
                        if let Some(ref mut m) = mic {
                            match m.start() {
                                Ok(()) => {
                                    mic_started = true;
                                    // Anchor the mic wav's wall-clock start so save_clip can
                                    // seek it independently, exactly like the system wav.
                                    self.mic_start_walltime = Some(now_wall_secs());
                                    let mic_offset_ms = video_start_instant.elapsed().as_millis();
                                    info!(
                                        "Microphone capture started successfully \
                                         ({}ms after FFmpeg video start)",
                                        mic_offset_ms
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        "Microphone capture failed; continuing without mic: {}",
                                        e
                                    );
                                    mic = None;
                                }
                            }
                        }
                        self.mic = mic;
                        self.mic_active = mic_started;
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
            // avfoundation embeds the system audio into the segments, so report
            // it as active — otherwise get_stats() would show a false "silent
            // recording" warning on macOS.
            self.system_audio_active = has_audio;
        }

        #[cfg(target_os = "linux")]
        {
            cmd.arg("-f").arg("x11grab");
            cmd.arg("-framerate").arg(self.config.fps.to_string());
            // yuv420p needs even dimensions on every platform.
            let (grab_w, grab_h) =
                even_dimensions(self.config.resolution.0, self.config.resolution.1);
            cmd.arg("-video_size").arg(format!("{}x{}", grab_w, grab_h));
            cmd.arg("-i").arg(":0.0");
        }

        let encoder = self
            .config
            .encoder
            .to_ffmpeg_name_with_hw(self.config.hw_accel);
        cmd.arg("-c:v").arg(encoder);
        let bitrate_k = self.config.bitrate / 1000;
        cmd.arg("-b:v").arg(format!("{}k", bitrate_k));
        // 상한이 없으면 VBR 이 목표치를 한참 넘긴다 — 20Mbps 로 설정했는데 실측
        // 클립이 28Mbps 였고, 8초짜리가 27MB 였다. 한 판이면 수백 MB 가 쌓인다.
        // `save_clip` 이 `-c:v copy` 라 여기서 낭비한 용량은 뒤에서 회수되지 않는다.
        cmd.arg("-maxrate").arg(format!("{}k", bitrate_k * 3 / 2));
        cmd.arg("-bufsize").arg(format!("{}k", bitrate_k * 3));

        if encoder.contains("nvenc") {
            // p1(최속) 은 곧 최저 품질이고, `save_clip` 이 `-c:v copy` 라
            // 이 손실은 절대 회복되지 않는다 — 화질 상한이 여기서 정해진다.
            // 리플레이 버퍼는 실시간 송출이 아니라 지연 요구가 낮으므로 최속을
            // 쓸 이유가 없다.
            cmd.arg("-preset").arg("p4");
        } else if encoder.contains("qsv") {
            cmd.arg("-preset").arg("veryfast");
        } else if encoder.contains("amf") {
            cmd.arg("-preset").arg("speed");
        } else {
            // 위 NVENC 와 같은 이유. `zerolatency` 는 B프레임을 끄고 룩어헤드를
            // 없애 화질을 크게 떨어뜨리는데, 여기서 얻을 지연 이득이 없다.
            cmd.arg("-preset").arg("veryfast");
        }
        cmd.arg("-g").arg((self.config.fps * 2).to_string());
        cmd.arg("-keyint_min").arg(self.config.fps.to_string());
        cmd.arg("-sc_threshold").arg("0");
        cmd.arg("-pix_fmt").arg("yuv420p");

        if has_audio {
            if let Some(ref audio_cfg) = self.config.audio_config {
                // NOTE: this branch only runs when audio is muxed directly into the
                // segments by FFmpeg — i.e. the DirectShow fallback (Windows) or
                // avfoundation (macOS). The WASAPI loopback path sets has_audio=false
                // and writes a separate wav that save_clip muxes later, so this -ar/-c:a
                // block does NOT apply to WASAPI.
                cmd.arg("-c:a").arg("aac");
                cmd.arg("-b:a").arg(format!("{}k", audio_cfg.bitrate));
                // Explicitly passing -ar normalizes the DirectShow/avfoundation input to
                // a consistent sample rate, preventing A/V sync drift and muxing issues.
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
        // stdout carries `-progress pipe:1` output; it MUST be consumed by the
        // reader task below or FFmpeg blocks once the pipe buffer fills.
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000);
        }

        info!("FFmpeg 녹화 시작: {:?}", cmd);

        let mut child = cmd.spawn().context("FFmpeg 프로세스 시작 실패")?;

        // Take stderr + stdout handles before storing child to prevent pipe buffer deadlock
        let stderr = child.stderr.take();
        let stdout = child.stdout.take();
        self.ffmpeg_process = Some(child);
        self.start_time = Some(Instant::now());

        let is_cleanup = cleanup;

        // Task 61 (fix): parse the cumulative encoded frame count from the
        // machine-readable `-progress pipe:1` stream on stdout. Each field is its
        // own LF-terminated line (`frame=123`), so a plain line reader works —
        // unlike the '\r'-terminated stderr stats line that stalled the counter at
        // 0. This task MUST keep draining stdout for the whole session; if it
        // stops, FFmpeg blocks on a full pipe and recording halts.
        let frame_count_progress = Arc::clone(&self.frame_count);
        if let Some(stdout) = stdout {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                let mut fps_samples: u64 = 0;
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(frames) = parse_progress_frame(&line) {
                        frame_count_progress.store(frames, Ordering::Relaxed);
                    } else if let Some(rest) = line.strip_prefix("fps=") {
                        // `-progress` reports instantaneous fps (~1 block/sec). Sample
                        // it occasionally to surface sustained under-target capture
                        // without spamming the log.
                        if let Ok(fps) = rest.trim().parse::<f64>() {
                            if fps > 0.0 {
                                fps_samples += 1;
                                if fps_samples.is_multiple_of(60) {
                                    let expected_fps = 60.0_f64;
                                    if fps < expected_fps * 0.9 {
                                        tracing::warn!(
                                            actual_fps = fps,
                                            expected_fps,
                                            "Frame rate below target"
                                        );
                                    } else {
                                        tracing::info!(
                                            actual_fps = fps,
                                            "Recording quality metrics"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                tracing::debug!("FFmpeg progress (stdout) reader exited");
            });
        }

        // Task 33: Spawn a bounded stderr reader.
        // Limits memory growth: stops after MAX_LINES lines.
        // Now that `-progress` carries the frame counter on stdout, stderr is used
        // ONLY for error/warning logging and crash detection.
        //
        // Task 28: When the stderr pipe closes before recording is intentionally stopped,
        // that signals an unexpected FFmpeg exit (crash). We emit a warning here so callers
        // can detect it. The `monitor_ffmpeg_health()` method provides the restart logic.
        // Rolling stderr tail: the reader task owns the pipe, so a start-up failure can
        // only be explained to the user if the last few lines are kept somewhere shared.
        let stderr_tail: StderrTail =
            Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL_LINES)));
        let stderr_tail_writer = Arc::clone(&stderr_tail);

        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                const MAX_LINES: usize = 10_000;
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                let mut line_count: usize = 0;
                let mut saw_error = false;
                while let Ok(Some(line)) = lines.next_line().await {
                    line_count += 1;
                    if line_count > MAX_LINES {
                        warn!(
                            "FFmpeg stderr exceeded {} lines; stopping reader to prevent unbounded memory growth",
                            MAX_LINES
                        );
                        break;
                    }
                    if let Ok(mut tail) = stderr_tail_writer.lock() {
                        if tail.len() >= STDERR_TAIL_LINES {
                            tail.pop_front();
                        }
                        tail.push_back(line.clone());
                    }
                    if line.contains("error")
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

        // A successful `spawn()` proves only that the OS created the process. FFmpeg
        // still dies within a few hundred ms on a bad `-video_size`, a missing capture
        // device or an unusable encoder — and the health monitor only polls every 5s,
        // so the UI used to claim "recording" for 5-10 seconds of a session that never
        // produced a single frame. Probe once here and fail the start instead.
        tokio::time::sleep(START_HEALTH_PROBE_DELAY).await;
        let early_exit = match self.ffmpeg_process.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(status) => status,
                Err(e) => {
                    warn!("FFmpeg 시작 상태 확인 실패(계속 진행): {}", e);
                    None
                }
            },
            None => None,
        };

        if let Some(status) = early_exit {
            let tail = stderr_tail
                .lock()
                .map(|tail| tail.iter().cloned().collect::<Vec<_>>().join("\n"))
                .unwrap_or_default();
            self.ffmpeg_process = None;
            self.start_time = None;
            // A fresh start owns the audio capture threads it just created, so tear them
            // down. A crash-recovery restart (cleanup=false) must NOT: those threads are
            // independent of FFmpeg and still hold the session's continuous wav.
            if cleanup {
                self.abort_audio_captures();
            }
            error!("FFmpeg가 시작 직후 종료됨 (status: {:?})\n{}", status, tail);
            anyhow::bail!(
                "FFmpeg가 시작 직후 종료되었습니다 (종료 코드: {:?}). FFmpeg 출력: {}",
                status.code(),
                if tail.is_empty() {
                    "(출력 없음)".to_string()
                } else {
                    tail
                }
            );
        }

        info!("세그먼트 기반 녹화 시작됨: {}", self.segment_dir.display());
        Ok(())
    }

    /// Best-effort teardown of the audio capture threads started by this session.
    ///
    /// Used when the FFmpeg launch turns out to have failed, so the wav writers do not
    /// keep running (and keep growing their files) for a recording that never began.
    fn abort_audio_captures(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref mut wasapi) = self.wasapi {
                let _ = wasapi.stop();
            }
            self.wasapi = None;

            if let Some(ref mut mic) = self.mic {
                let _ = mic.stop();
            }
            self.mic = None;
        }

        self.audio_start_walltime = None;
        self.system_audio_active = false;
        self.mic_start_walltime = None;
        self.mic_active = false;
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

            if let Some(ref mut mic) = self.mic {
                if let Some(wav_path) = mic.stop() {
                    info!("Microphone audio saved: {}", wav_path.display());
                }
            }
            self.mic = None;
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

    /// Snapshot everything a clip extraction needs so the export can run WITHOUT holding
    /// the recorder lock (see [`ClipExtractionContext`]).
    pub fn extraction_context(&self) -> ClipExtractionContext {
        ClipExtractionContext {
            config: self.config.clone(),
            segment_dir: self.segment_dir.clone(),
            audio_start_walltime: self.audio_start_walltime,
            mic_start_walltime: self.mic_start_walltime,
            system_audio_active: self.system_audio_active,
            ffmpeg_running: self.ffmpeg_process.is_some(),
        }
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

    /// Latest cumulative encoded frame count parsed from FFmpeg progress output.
    pub fn frame_count(&self) -> u64 {
        self.frame_count.load(Ordering::Relaxed)
    }

    /// Whether system audio is actually being captured this session (WASAPI or DirectShow).
    pub fn system_audio_active(&self) -> bool {
        self.system_audio_active
    }

    /// Whether the microphone is actually being captured this session.
    pub fn mic_active(&self) -> bool {
        self.mic_active
    }

    /// 녹화 중인지 확인
    #[allow(dead_code)]
    pub fn is_recording(&self) -> bool {
        self.ffmpeg_process.is_some()
    }
}

/// Immutable snapshot of the recorder state a clip extraction needs.
///
/// Extraction (coverage wait → per-segment verify → FFmpeg export) routinely takes tens
/// of seconds and is bounded only by `CLIP_EXPORT_TIMEOUT` (10 minutes). The recorder
/// lives behind a `tokio::sync::RwLock` that the health monitor write-locks every 5s, and
/// that lock is WRITE-PREFERRING: a read guard held for a whole export parked the health
/// monitor's writer, and every later `get_status`/`get_stats` read then queued behind that
/// writer — the UI froze for the entire save. Callers therefore take this snapshot under
/// the guard, drop the guard, and run the extraction lock-free.
///
/// Segment rotation during extraction: the segment list is snapshotted only AFTER the
/// coverage wait, and every file is verified (full decode) and length-probed immediately
/// before the concat job, so a rotated-away file is dropped or re-measured instead of
/// being silently mixed in. The residual risk — a segment overwritten between its probe
/// and FFmpeg reading it — is bounded by the rolling buffer length (default 90s), which is
/// far longer than a verify+export pass; nothing in this path can extend the buffer, so it
/// is accepted rather than blocking the recorder for the duration of a save.
#[derive(Debug, Clone)]
pub struct ClipExtractionContext {
    config: RecordingConfig,
    segment_dir: PathBuf,
    audio_start_walltime: Option<f64>,
    mic_start_walltime: Option<f64>,
    system_audio_active: bool,
    /// Whether FFmpeg was still writing segments when the snapshot was taken. Only
    /// decides whether the newest file on disk counts as complete, so a recorder that
    /// stops mid-extraction merely makes the coverage wait more conservative.
    ffmpeg_running: bool,
}

impl ClipExtractionContext {
    /// Save the most recent `requested_secs` (ending "now") from the rolling buffer.
    ///
    /// The rolling segment buffer only keeps the last ~`buffer_duration_secs`, and the
    /// concatenated video has its own rebased timeline (each segment resets its PTS),
    /// whereas the WASAPI wav is a continuous recording from the session start. We
    /// therefore compute a SEPARATE input seek for each stream (see
    /// `compute_clip_window`) so both land on the same wall-clock instant. This fixes
    /// both the "clips are empty after ~1 minute of recording" bug (offset used to be
    /// absolute session-elapsed applied to a ~60s concat stream) and the fixed A/V
    /// startup offset (WASAPI starts before the first video frame).
    ///
    /// Returns the written path and the MEASURED length of the produced file.
    pub async fn save_clip(
        &self,
        output_path: &PathBuf,
        requested_secs: f64,
    ) -> Result<(PathBuf, f64)> {
        // Anchor at "now", but still wait one segment boundary: the segment being
        // written has no moov atom yet, so without the wait a manual "last N seconds"
        // silently loses the 0-10s that the user actually pressed the hotkey for.
        let coverage_timeout =
            Duration::from_secs_f64(self.config.segment_duration_secs as f64 + 2.0);
        // The anchor is resolved ONCE inside save_clip_anchored, before the wait — see
        // the comment there for why re-reading "now" afterwards always shortened the clip.
        self.save_clip_anchored(output_path, requested_secs, None, coverage_timeout)
            .await
    }

    /// Save `requested_secs` of buffered footage ending at an EXPLICIT wall-clock instant.
    ///
    /// `end_anchor_wall` is the absolute instant (seconds since the UNIX epoch) the clip
    /// should end at; `None` means "now". Anchoring explicitly is what makes an event
    /// clip land on the event: the caller knows when the event was detected, whereas
    /// "now" drifts by however long queueing, merging and the post-event wait took.
    ///
    /// `coverage_timeout` bounds how long we wait for the rolling buffer to actually
    /// reach `end_anchor_wall` before giving up and clamping the window to what exists.
    ///
    /// Returns the written path and the MEASURED length of the produced file — NOT the
    /// requested length. The window is clamped whenever the buffer could not cover it, so
    /// the caller must persist this value (metadata, auto-edit target length) instead of
    /// what it asked for.
    pub async fn save_clip_anchored(
        &self,
        output_path: &PathBuf,
        requested_secs: f64,
        end_anchor_wall: Option<f64>,
        coverage_timeout: Duration,
    ) -> Result<(PathBuf, f64)> {
        info!(
            "클립 저장 시작: {} (requested: {:.2}s, end_anchor: {:?})",
            output_path.display(),
            requested_secs,
            end_anchor_wall
        );

        // Resolve the window end ONCE, up front. For a manual save (`end_anchor_wall ==
        // None`) this used to be re-read as "now" AFTER the coverage wait, the segment
        // verification and the ffprobe pass — several seconds later — so the window end
        // always sat beyond the footage that actually exists: every manual save logged
        // "클립 창 끝이 가용 영상을 넘어섭니다" and returned a clip shorter than requested.
        let end_anchor = end_anchor_wall.unwrap_or_else(now_wall_secs);

        // Wait (bounded) for the buffer to cover the end of the requested window before
        // taking the snapshot of segments below.
        if !coverage_timeout.is_zero() {
            self.wait_for_segment_coverage(end_anchor, coverage_timeout)
                .await;
        }

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

        // ---- Measure each segment so the concat list can carry `duration` directives ----
        // Without them the concat demuxer cannot build a timeline for segments whose PTS
        // were reset, an input `-ss` fails with "could not seek", and the export silently
        // returns the wrong span (and the wrong length) of footage.
        //
        // A segment whose length cannot be measured is DROPPED rather than assumed to be a
        // full `segment_duration_secs` long — see `retain_measured_segments` for why that
        // assumption is what made identical 13s requests produce 8/14/18-second files.
        let mut probed = Vec::with_capacity(verified_segments.len());
        for segment in &verified_segments {
            probed.push(probe_duration_secs(segment).await);
        }

        // Captured BEFORE the unmeasurable segments are dropped, so the clamp below can
        // report how much footage the exclusion actually cost.
        let verified_end = mtime_secs(
            verified_segments
                .last()
                .expect("verified_segments is non-empty"),
        );
        let verified_count = verified_segments.len();

        let (concat_segments, durations) = retain_measured_segments(&verified_segments, &probed)?;

        // ---- Compute per-input seek offsets against the rolling buffer timeline ----
        // concat_segments is sorted oldest-first (by mtime), and a segment's mtime marks
        // its COMPLETION. Anchoring the concat timeline from its END (newest mtime, then
        // walking back the MEASURED durations) stays correct even when an older segment
        // was dropped by the integrity check — the old "oldest_mtime - segment_duration"
        // anchor silently shifted the whole window in that case.
        let now = now_wall_secs();
        let total_content: f64 = durations.iter().sum();
        let newest_mtime = mtime_secs(
            concat_segments
                .last()
                .expect("retain_measured_segments never returns an empty list"),
        );
        let content_end = if newest_mtime > 0.0 {
            newest_mtime.min(now)
        } else {
            now
        };
        let concat_start = content_end - total_content;

        // Dropping an unmeasurable TAIL segment shortens the usable footage: the window
        // must be clamped to what survived, not to what merely passed the integrity check.
        // `compute_clip_window` does the clamping (content_end is its `available_end`);
        // this only makes the cost visible instead of letting it surface as a mysteriously
        // short clip.
        if concat_segments.len() < verified_count {
            let verified_end = verified_end.min(now);
            if verified_end > content_end + CLIP_LENGTH_TOLERANCE_SECS {
                warn!(
                    "길이 측정 실패 세그먼트 제외로 가용 영상 끝이 {:.2}s 앞당겨졌습니다 \
                     — 클립 창을 가용 범위({:.2}s)로 잘라 저장합니다",
                    verified_end - content_end,
                    total_content
                );
            }
        }

        let window = compute_clip_window(
            requested_secs,
            end_anchor,
            content_end,
            concat_start,
            self.audio_start_walltime,
            self.mic_start_walltime,
        );

        if end_anchor > content_end + CLIP_LENGTH_TOLERANCE_SECS {
            warn!(
                "클립 창 끝이 가용 영상({:.2}s 부족)을 넘어섭니다 — 가용 범위로 잘라 저장합니다: {}",
                end_anchor - content_end,
                output_path.display()
            );
        }

        if window.duration + CLIP_LENGTH_TOLERANCE_SECS < requested_secs {
            warn!(
                "클립 요청 {:.1}s 중 {:.1}s만 확보됨(버퍼 부족): {}",
                requested_secs,
                window.duration,
                output_path.display()
            );
        }

        // Reject an unusable window BEFORE anything is written: no file, no metadata row.
        ensure_usable_clip_window(&window, requested_secs, output_path)?;

        let concat_file = self.segment_dir.join("concat_list.txt");
        let concat_content = build_concat_list(&concat_segments, &durations);
        tokio::fs::write(&concat_file, &concat_content).await?;

        // Mux each wav ONLY when it was produced THIS session (flag), never on mere
        // file existence — a leftover wav from a previous session must not leak in.
        let wav_path = self.segment_dir.join("wasapi_loopback.wav");
        let has_system_audio = self.audio_start_walltime.is_some() && wav_path.exists();

        let mic_wav_path = self.segment_dir.join("mic_capture.wav");
        let has_mic_audio = self.mic_start_walltime.is_some() && mic_wav_path.exists();

        let mut cmd = tokio::process::Command::new(&ffmpeg_path);
        cmd.arg("-y");

        // Video input (index 0): seek into the concatenated (rebased) segment timeline.
        cmd.arg("-ss").arg(format!("{:.3}", window.video_ss));
        cmd.arg("-f").arg("concat");
        cmd.arg("-safe").arg("0");
        cmd.arg("-i").arg(&concat_file);

        // Audio inputs: each continuous wav seeks independently (different origins) with
        // the same per-input anchoring so video, system audio and mic all land on the
        // same wall-clock instant. Input indices are assigned in the order added below.
        let mut sys_input_idx: Option<usize> = None;
        let mut mic_input_idx: Option<usize> = None;
        let mut next_input_idx = 1;
        if has_system_audio {
            cmd.arg("-ss").arg(format!("{:.3}", window.audio_ss));
            cmd.arg("-i").arg(&wav_path);
            sys_input_idx = Some(next_input_idx);
            next_input_idx += 1;
        }
        if has_mic_audio {
            cmd.arg("-ss").arg(format!("{:.3}", window.mic_ss));
            cmd.arg("-i").arg(&mic_wav_path);
            mic_input_idx = Some(next_input_idx);
        }

        // `ensure_usable_clip_window` above guarantees >= MIN_CLIP_DURATION_SECS, so this
        // no longer needs the `.max(0.1)` floor that turned an empty window into a 0.1s
        // file FFmpeg happily reported as a success.
        cmd.arg("-t").arg(format!("{:.3}", window.duration));
        // `-avoid_negative_ts make_zero` 는 **쓰지 않는다.**
        //
        // 클립을 0초부터 시작시키려고 넣었던 옵션인데, 실측해 보니 두 가지를 동시에
        // 망가뜨리고 있었다(실게임 세그먼트로 재현):
        //
        // ```
        //   -ss 42.21 -t 17.37 + make_zero -> 57.58s, 오디오 start_time 40.188s
        //   -ss 42.21 -t 17.37 (옵션 없음)  -> 17.37s, 오디오 start_time 0.000s
        // ```
        //
        // 1) `-t` 의 기준 시각이 어긋나 **오차가 `-ss` 에 비례해 커진다**
        //    (`-ss 5` 면 1초 초과, `-ss 42` 면 40초 초과). 실게임에서 13초를
        //    요청한 클립이 39초로 나왔다.
        // 2) 오디오 스트림의 start_time 이 40초로 밀려, **클립 앞부분이 통째로
        //    무음이고 들리는 소리는 다른 순간의 것**이 된다.
        //
        // mp4 머서가 이미 출력 타임스탬프를 0 기준으로 정규화하므로(위 실측의
        // start_time=0.000) 이 옵션 없이도 클립은 0초부터 시작한다.

        let audio_bitrate = self
            .config
            .audio_config
            .as_ref()
            .map(|a| a.bitrate)
            .unwrap_or(192);
        // Volumes are linear coefficients (0-200% -> 0.0-2.0). Defaults mirror
        // AudioConfig::default() so a missing audio_config never silences a track.
        let (sys_vol, mic_vol) = self
            .config
            .audio_config
            .as_ref()
            .map(|a| {
                (
                    a.system_audio_volume as f32 / 100.0,
                    a.microphone_volume as f32 / 100.0,
                )
            })
            .unwrap_or((1.0, 1.2));

        match (sys_input_idx, mic_input_idx) {
            (Some(sys), Some(mic)) => {
                // System audio + microphone mixed into ONE track (amix). Per-input
                // volume is applied before mixing.
                let filter = format!(
                    "[{sys}:a]volume={sys_vol}[a1];[{mic}:a]volume={mic_vol}[a2];\
                     [a1][a2]amix=inputs=2:duration=first[outa]"
                );
                info!(
                    "Muxing system audio + microphone (amix): video_ss={:.2}s, \
                     audio_ss={:.2}s, mic_ss={:.2}s, dur={:.2}s",
                    window.video_ss, window.audio_ss, window.mic_ss, window.duration
                );
                cmd.arg("-filter_complex").arg(filter);
                cmd.arg("-map").arg("0:v");
                cmd.arg("-map").arg("[outa]");
                cmd.arg("-c:v").arg("copy");
                cmd.arg("-c:a").arg("aac");
                cmd.arg("-b:a").arg(format!("{}k", audio_bitrate));
            }
            (Some(_sys), None) => {
                // System audio only — UNCHANGED legacy 2-input path (system is input 1).
                info!(
                    "Muxing WASAPI loopback audio: {} (video_ss={:.2}s, audio_ss={:.2}s, dur={:.2}s)",
                    wav_path.display(),
                    window.video_ss,
                    window.audio_ss,
                    window.duration
                );
                cmd.arg("-map").arg("0:v");
                cmd.arg("-map").arg("1:a");
                cmd.arg("-c:v").arg("copy");
                cmd.arg("-c:a").arg("aac");
                cmd.arg("-b:a").arg(format!("{}k", audio_bitrate));
            }
            (None, Some(mic)) => {
                // No separate system wav. Two sub-cases:
                // (a) DirectShow fallback embedded the system audio INSIDE the
                //     segments (stream 0:a, same timeline as the video, so the
                //     input-0 -ss already anchors it) — mix it with the mic so
                //     the system audio is not silently dropped.
                // (b) System audio genuinely off/unavailable — mic only.
                let filter = if self.system_audio_active {
                    info!(
                        "Muxing embedded (DirectShow) system audio + microphone (amix): \
                         video_ss={:.2}s, mic_ss={:.2}s, dur={:.2}s",
                        window.video_ss, window.mic_ss, window.duration
                    );
                    format!(
                        "[0:a]volume={sys_vol}[a1];[{mic}:a]volume={mic_vol}[a2];\
                         [a1][a2]amix=inputs=2:duration=first[outa]"
                    )
                } else {
                    info!(
                        "Muxing microphone only: {} (video_ss={:.2}s, mic_ss={:.2}s, dur={:.2}s)",
                        mic_wav_path.display(),
                        window.video_ss,
                        window.mic_ss,
                        window.duration
                    );
                    format!("[{mic}:a]volume={mic_vol}[outa]")
                };
                cmd.arg("-filter_complex").arg(filter);
                cmd.arg("-map").arg("0:v");
                cmd.arg("-map").arg("[outa]");
                cmd.arg("-c:v").arg("copy");
                cmd.arg("-c:a").arg("aac");
                cmd.arg("-b:a").arg(format!("{}k", audio_bitrate));
            }
            (None, None) => {
                cmd.arg("-c").arg("copy");
            }
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

        // Report the REAL length. `window.duration` is only an upper bound: it is clamped
        // whenever the buffer could not cover the request, and `-c copy` additionally cuts
        // on keyframe boundaries. ffprobe reads the container header only (cheap), and we
        // fall back to the computed window if it cannot be read.
        let actual_duration = match probe_duration_secs(output_path).await {
            Some(measured) => measured,
            None => {
                warn!(
                    "저장된 클립 길이 측정 실패, 계산값 {:.2}s를 사용합니다: {}",
                    window.duration,
                    output_path.display()
                );
                window.duration
            }
        };

        // A produced file that is materially shorter than the request must not hide in an
        // info line: `요청 13.00s, 창 13.00s, 실제 8.00s` was logged as an ordinary success
        // and stored as a normal clip. The MEASURED value is what the caller persists (see
        // the return contract above), so the shortfall reaches metadata; this classifies
        // how loudly it has to be reported.
        match classify_shortfall(requested_secs, actual_duration) {
            ClipShortfall::Severe => error!(
                "클립 길이 심각 부족: 요청 {:.2}s 중 {:.2}s만 저장됨({:.0}%, 계산된 창 {:.2}s): {}",
                requested_secs,
                actual_duration,
                actual_duration / requested_secs * 100.0,
                window.duration,
                output_path.display()
            ),
            ClipShortfall::Partial => warn!(
                "클립 길이 부족: 요청 {:.2}s 중 {:.2}s 저장됨(계산된 창 {:.2}s): {}",
                requested_secs,
                actual_duration,
                window.duration,
                output_path.display()
            ),
            ClipShortfall::None => {}
        }

        info!(
            "클립 저장 완료: {} (요청 {:.2}s, 창 {:.2}s, 실제 {:.2}s)",
            output_path.display(),
            requested_secs,
            window.duration,
            actual_duration
        );
        Ok((output_path.clone(), actual_duration))
    }

    /// Wait (bounded) until COMPLETED segments cover `target_wall`.
    ///
    /// The segment FFmpeg is currently writing has no moov atom yet, so it fails
    /// `verify_segment` (and, when it slips past that, the duration probe that drops it
    /// from the concat list) and can never be part of a clip: usable footage ends at the
    /// PREVIOUS segment's completion, i.e. 0..segment_duration seconds in the past.
    /// A clip whose window ends at "now" (or at an event that just happened) therefore
    /// loses its tail unless we first wait for the segment boundary.
    ///
    /// On timeout we return anyway and let the caller clamp the window — at game end no
    /// further segment will ever be produced, and losing a tail beats failing the save.
    async fn wait_for_segment_coverage(&self, target_wall: f64, timeout_after: Duration) {
        let deadline = tokio::time::Instant::now() + timeout_after;

        loop {
            let covered = self.completed_coverage_wall();
            if covered >= target_wall {
                return;
            }

            if tokio::time::Instant::now() >= deadline {
                warn!(
                    "세그먼트 커버리지 대기 타임아웃({:.1}s): {:.2}s 부족한 상태로 진행합니다",
                    timeout_after.as_secs_f64(),
                    target_wall - covered
                );
                return;
            }

            tokio::time::sleep(COVERAGE_POLL_INTERVAL).await;
        }
    }

    /// Wall-clock instant up to which COMPLETED segments cover the rolling buffer.
    ///
    /// While FFmpeg runs, the newest file by mtime is the one still being written, so the
    /// last completed segment is the second-newest. Once FFmpeg has exited every segment
    /// on disk is finalized, so the newest one counts. Returns 0.0 when nothing is
    /// complete yet (the caller then simply waits out its timeout).
    fn completed_coverage_wall(&self) -> f64 {
        let segments = match self.get_sorted_segments() {
            Ok(segments) => segments,
            Err(e) => {
                warn!("세그먼트 목록 조회 실패: {}", e);
                return 0.0;
            }
        };

        let index = if self.ffmpeg_running {
            match segments.len().checked_sub(2) {
                Some(index) => index,
                None => return 0.0,
            }
        } else {
            match segments.len().checked_sub(1) {
                Some(index) => index,
                None => return 0.0,
            }
        };

        mtime_secs(&segments[index])
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
}

/// Parse a `frame=N` field from FFmpeg's `-progress` output.
///
/// The `-progress` stream is machine-readable, unpadded `key=value` on
/// LF-terminated lines (e.g. `frame=123`), unlike the '\r'-terminated stderr
/// stats line (`frame=  123 fps= 60 ...`) that a line reader cannot split.
/// Returns the cumulative encoded frame count, or `None` for any other field.
fn parse_progress_frame(line: &str) -> Option<u64> {
    line.strip_prefix("frame=")?.trim().parse::<u64>().ok()
}

/// Round a capture size DOWN to even width/height.
///
/// h264 with `yuv420p` requires both dimensions to be divisible by 2. gdigrab reports
/// window/monitor rects verbatim, so an odd one (a restored or DPI-scaled window is
/// routinely 1919x1079) makes the encoder abort with "width not divisible by 2" the
/// instant it starts — after `spawn()` has already reported success.
/// 캡처할 수 있는 크기의 League 창이 나타날 때까지 (상한을 두고) 기다린다.
///
/// 게임이 막 뜬 순간의 창은 1x1 이고, 그 상태로 gdigrab 을 걸면 인코더가
/// "Frame Dimension less than the minimum supported value" 로 즉사한다.
/// 재시도 경로가 없으므로 그 판은 통째로 녹화되지 않는다 — 실게임에서 관측했다.
///
/// 창을 못 찾아도 그냥 진행한다. 여기서 실패로 끝내면 창 제목이 다른 지역/버전
/// 클라이언트에서 녹화가 아예 불가능해지는데, 그건 지금 고치려는 문제보다 나쁘다.
/// 기다리는 것은 "1x1 인 동안"이지 "창이 있어야 한다"가 아니다.
#[cfg(target_os = "windows")]
async fn wait_for_capturable_window() {
    /// 이보다 작으면 아직 초기화 중으로 본다. 실제 게임 창은 최소 640x480 이다.
    const MIN_CAPTURABLE: u32 = 320;
    const POLL: Duration = Duration::from_millis(200);
    const MAX_WAIT: Duration = Duration::from_secs(20);

    let deadline = tokio::time::Instant::now() + MAX_WAIT;
    let mut warned = false;

    loop {
        match super::types::find_league_hwnd().and_then(super::types::get_window_rect) {
            Some((_, _, w, h)) if w >= MIN_CAPTURABLE && h >= MIN_CAPTURABLE => {
                if warned {
                    info!("League 창이 캡처 가능한 크기가 되었습니다: {}x{}", w, h);
                }
                return;
            }
            Some((_, _, w, h)) => {
                if !warned {
                    info!(
                        "League 창이 아직 {}x{} 입니다. 캡처 가능한 크기가 될 때까지 기다립니다.",
                        w, h
                    );
                    warned = true;
                }
            }
            None => {
                // 창을 못 찾음 — 제목이 다르거나 아직 안 만들어졌다. 기다려는 보되
                // 이것 때문에 녹화를 막지는 않는다.
                if !warned {
                    warned = true;
                }
            }
        }

        if tokio::time::Instant::now() >= deadline {
            warn!(
                "League 창이 {}초 안에 캡처 가능한 크기가 되지 않았습니다. 그대로 진행합니다.",
                MAX_WAIT.as_secs()
            );
            return;
        }

        tokio::time::sleep(POLL).await;
    }
}

fn even_dimensions(width: u32, height: u32) -> (u32, u32) {
    (width & !1, height & !1)
}

/// Build the concat-demuxer list for `segments`, pairing every `file` line with an
/// explicit `duration` directive.
///
/// The `duration` lines are NOT optional here. Our segments are written with
/// `-reset_timestamps 1`, so the demuxer cannot derive the concatenated timeline on its
/// own; an INPUT seek (`-ss` before `-i`, which is what keeps the video aligned with the
/// per-input audio seeks) then fails with "could not seek", and together with
/// `-avoid_negative_ts make_zero` the export quietly produces a clip of the wrong length
/// from the wrong part of the buffer.
///
/// `durations[i]` is the measured length of `segments[i]`; a missing/implausible value is
/// simply omitted so ffmpeg falls back to its own (best-effort) probing for that entry.
fn build_concat_list(segments: &[PathBuf], durations: &[f64]) -> String {
    let mut content = String::new();

    for (index, segment) in segments.iter().enumerate() {
        // Single quotes delimit the path, so an apostrophe inside it (a Windows user
        // named e.g. "O'Brien") has to be escaped the way ffmpeg's parser expects.
        let escaped = segment.display().to_string().replace('\'', r"'\''");
        content.push_str(&format!("file '{}'\n", escaped));

        if let Some(duration) = durations.get(index) {
            if duration.is_finite() && *duration > 0.0 {
                content.push_str(&format!("duration {:.3}\n", duration));
            }
        }
    }

    content
}

/// Keep only the segments whose length could actually be MEASURED, paired with that
/// measurement.
///
/// `probed[i]` is the ffprobe result for `segments[i]`; `None` (or an implausible value)
/// means the measurement failed. In practice the only files that fail are partial ones:
/// the segment FFmpeg is still writing has no moov atom yet, and one that rotated away
/// mid-export is truncated.
///
/// Such a file used to be declared `segment_duration_secs` long, which is the worst
/// possible guess — a 3-second fragment entered the concat list as a 10-second entry, so
/// every `duration` directive from that point on described a timeline that does not exist
/// and the input `-ss`/`-t` pair cut the wrong length from the wrong place. One real match
/// produced 8.0 / 8.0 / 8.0 / 14.2 / 18.0-second files for five identical 13-second
/// requests. Dropping the entry is always the safer trade: the concat timeline stays
/// exact, and at most the last few seconds of footage are lost (the caller clamps the
/// window to the shortened `content_end`).
///
/// Returns the surviving `(segments, durations)` pair, still oldest-first and index
/// aligned. Errors when nothing survives — falling back to a guessed timeline for an
/// entire clip is exactly the behaviour this replaces.
fn retain_measured_segments(
    segments: &[PathBuf],
    probed: &[Option<f64>],
) -> Result<(Vec<PathBuf>, Vec<f64>)> {
    // 측정 실패 세그먼트를 그냥 빼면 목록에 "구멍"이 생기고, 그러면 이 함수의
    // 결과를 쓰는 쪽의 타임라인 산식(concat_start = 마지막 세그먼트 mtime -
    // 남은 길이 합)이 어긋난다. 구멍 이전 구간이 구멍 길이만큼 미래로 잘못
    // 매핑돼, 킬 순간에 앵커된 클립이 엉뚱한 영상을 자르게 된다 — 고치려던
    // 버그가 형태만 바꿔 되살아나는 셈이다.
    //
    // 그래서 마지막 연속 구간만 남긴다. 실사용에서 압도적으로 흔한 경우는
    // "기록 중인 꼬리 파일 하나"라 잃는 것이 없고, 중간이 빠진 드문 경우에는
    // 더 짧지만 시간축이 정확한 클립을 만든다.
    let measured: Vec<Option<f64>> = (0..segments.len())
        .map(|index| match probed.get(index).copied().flatten() {
            Some(duration) if duration.is_finite() && duration > 0.0 => Some(duration),
            _ => None,
        })
        .collect();

    // 쓸 수 있는 구간 = "마지막으로 측정된 세그먼트에서 끝나는 연속 구간".
    //   1. 측정 안 된 꼬리(기록 중인 파일)는 잘라낸다 — 어차피 못 쓴다.
    //   2. 거기서 뒤로 걸어오다 실패를 만나면 멈춘다 — 그 앞은 구멍 너머라 버린다.
    let end = match measured.iter().rposition(|d| d.is_some()) {
        Some(last) => last + 1,
        None => {
            anyhow::bail!(
                "길이를 측정할 수 있는 세그먼트가 없습니다({}개 모두 측정 실패).                  녹화 파일이 아직 준비되지 않았거나 손상되었습니다",
                segments.len()
            );
        }
    };
    let mut start = end;
    while start > 0 && measured[start - 1].is_some() {
        start -= 1;
    }
    let dropped = segments.len() - (end - start);

    for (index, segment) in segments.iter().enumerate() {
        if (start..end).contains(&index) {
            continue;
        }
        let reason = if measured[index].is_none() {
            "길이 측정 실패(기록 중인 부분 파일일 수 있습니다)"
        } else {
            "측정 실패 세그먼트보다 앞이라 시간축을 보장할 수 없음"
        };
        warn!(
            "세그먼트를 concat 목록에서 제외합니다 — {}: {}",
            reason,
            segment.display()
        );
    }

    let kept_segments: Vec<PathBuf> = segments[start..end].to_vec();
    let kept_durations: Vec<f64> = measured[start..end]
        .iter()
        .map(|d| d.expect("연속 구간은 전부 측정된 값이다"))
        .collect();

    if kept_segments.is_empty() {
        anyhow::bail!(
            "길이를 측정할 수 있는 세그먼트가 없습니다({}개 모두 측정 실패). \
             녹화 파일이 아직 준비되지 않았거나 손상되었습니다",
            dropped
        );
    }

    if dropped > 0 {
        warn!(
            "세그먼트 {}개를 제외하고 마지막 연속 구간 {}개로 클립을 만듭니다 (시간축 정확도를 위해 구멍 앞은 버립니다)",
            dropped,
            kept_segments.len()
        );
    }

    Ok((kept_segments, kept_durations))
}

/// How far the produced clip fell short of what the caller asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipShortfall {
    /// Requested length delivered (within keyframe-boundary noise).
    None,
    /// Shorter than requested, but still a usable clip — normally a buffer clamp.
    Partial,
    /// Less than [`SEVERE_SHORTFALL_RATIO`] of the request: the export did not deliver
    /// anything close to the window, which is what a broken concat timeline looks like.
    Severe,
}

/// Classify `actual_secs` against `requested_secs` for reporting.
///
/// A non-positive request has no shortfall to speak of (the caller asked for "whatever is
/// available"), and an overshoot — `-c copy` can only cut on a keyframe, so the file is
/// regularly a fraction of a second longer — is never a shortfall.
fn classify_shortfall(requested_secs: f64, actual_secs: f64) -> ClipShortfall {
    if !requested_secs.is_finite() || requested_secs <= 0.0 || !actual_secs.is_finite() {
        return ClipShortfall::None;
    }

    if actual_secs < requested_secs * SEVERE_SHORTFALL_RATIO {
        ClipShortfall::Severe
    } else if actual_secs + CLIP_LENGTH_TOLERANCE_SECS < requested_secs {
        ClipShortfall::Partial
    } else {
        ClipShortfall::None
    }
}

/// Measure a media file's container duration with ffprobe.
///
/// Reads the container header only (no decoding), so this is cheap enough to run for
/// every segment on each clip export. Returns `None` when ffprobe is unavailable, exits
/// non-zero, or prints something unparsable (e.g. `N/A` for a half-written segment).
async fn probe_duration_secs(path: &std::path::Path) -> Option<f64> {
    let ffprobe_path = match get_ffprobe_path() {
        Ok(path) => path,
        Err(e) => {
            warn!(
                "FFprobe를 찾을 수 없어 세그먼트 길이를 측정할 수 없습니다: {}",
                e
            );
            return None;
        }
    };

    let mut command = tokio::process::Command::new(&ffprobe_path);
    command
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path);
    command.stdin(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        command.creation_flags(0x08000000);
    }

    let output = run_command_with_timeout(
        &mut command,
        SEGMENT_PROBE_TIMEOUT,
        "FFprobe segment duration",
    )
    .await
    .ok()?;

    if !output.status.success() {
        return None;
    }

    parse_ffprobe_duration(&String::from_utf8_lossy(&output.stdout))
}

/// Parse ffprobe's `format=duration` output (`"12.345\n"`, or `"N/A"` when unknown).
fn parse_ffprobe_duration(stdout: &str) -> Option<f64> {
    let value: f64 = stdout.trim().lines().next()?.trim().parse().ok()?;
    if value.is_finite() && value > 0.0 {
        Some(value)
    } else {
        None
    }
}

/// Current wall-clock time as seconds since the UNIX epoch.
pub(crate) fn now_wall_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// File modification time as seconds since the UNIX epoch (0.0 on error).
fn mtime_secs(path: &std::path::Path) -> f64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Per-input seek offsets for extracting a clip from the rolling buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipWindow {
    /// Input `-ss` for the concatenated video stream (seconds, >= 0).
    pub video_ss: f64,
    /// Input `-ss` for the continuous WASAPI (system audio) wav (seconds, >= 0).
    pub audio_ss: f64,
    /// Input `-ss` for the continuous microphone wav (seconds, >= 0).
    pub mic_ss: f64,
    /// Output `-t` duration (seconds), clamped to what is actually available.
    pub duration: f64,
}

/// Compute per-input seek offsets for extracting `requested_secs` of footage that ENDS
/// at `end_anchor_secs`.
///
/// All time arguments are absolute wall-clock seconds (e.g. seconds since the UNIX
/// epoch). Video, system audio and microphone use DIFFERENT origins:
///   * the concatenated video stream begins at `concat_start_secs`
///     (= newest verified segment's mtime − the measured length of the concat list),
///   * the WASAPI (system audio) wav is a continuous recording that began at
///     `wav_start_secs`,
///   * the microphone wav is a continuous recording that began at `mic_start_secs`.
///
/// `end_anchor_secs` is the EXPLICIT instant the clip should end at — for an event clip
/// that is `event_detected_at + post_duration`, not "now" (which drifts by however long
/// queueing/merging/the post-event wait took). `available_end_secs` is the latest instant
/// the buffer actually covers (the last COMPLETED segment); the window end is clamped to
/// it so a request that outran the recorder degrades into a shorter clip instead of an
/// out-of-range seek.
///
/// Returning a separate offset per input keeps every stream aligned to the same
/// wall-clock instant, which also corrects the fixed A/V startup offset. When the
/// requested window is longer than what the buffer holds, the returned `duration` is
/// clamped to the available length (the offsets always stay within their streams).
/// `audio_ss`/`mic_ss` fall back to `video_ss` when their wav is absent (the value is
/// unused in that case since the input is not added).
fn compute_clip_window(
    requested_secs: f64,
    end_anchor_secs: f64,
    available_end_secs: f64,
    concat_start_secs: f64,
    wav_start_secs: Option<f64>,
    mic_start_secs: Option<f64>,
) -> ClipWindow {
    let requested = requested_secs.max(0.0);
    // The clip can never end later than the footage the buffer actually holds.
    let effective_end = end_anchor_secs.min(available_end_secs);
    // Desired window is [end - requested, end]; clamp the start to available video.
    let want_start = end_anchor_secs - requested;
    let effective_start = want_start.max(concat_start_secs).min(effective_end);
    let available = (effective_end - effective_start).max(0.0);
    let duration = if requested > 0.0 {
        available.min(requested)
    } else {
        available
    };

    let video_ss = (effective_start - concat_start_secs).max(0.0);
    let audio_ss = match wav_start_secs {
        Some(ws) => (effective_start - ws).max(0.0),
        None => video_ss,
    };
    let mic_ss = match mic_start_secs {
        Some(ms) => (effective_start - ms).max(0.0),
        None => video_ss,
    };

    ClipWindow {
        video_ss,
        audio_ss,
        mic_ss,
        duration,
    }
}

/// Reject a computed window that is too short to be a real clip.
///
/// `compute_clip_window` returns `duration == 0` whenever the requested window lies
/// outside the rolling buffer (a merge window that flushed late, an anchor older than the
/// buffer, a game that ended before the post-event footage existed). Exporting that window
/// wrote a ~0.1s file that FFmpeg reported as a success, so the caller stored a
/// normal-looking metadata row for something unplayable. Failing here keeps both the file
/// and the metadata row from ever being created.
fn ensure_usable_clip_window(
    window: &ClipWindow,
    requested_secs: f64,
    output_path: &std::path::Path,
) -> Result<()> {
    if window.duration < MIN_CLIP_DURATION_SECS {
        anyhow::bail!(
            "클립 창이 너무 짧습니다({:.2}s < {:.1}s, 요청 {:.1}s): \
             앵커가 롤링 버퍼 범위를 벗어나 저장하지 않습니다 ({})",
            window.duration,
            MIN_CLIP_DURATION_SECS,
            requested_secs,
            output_path.display()
        );
    }
    Ok(())
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
        // Stop WASAPI + microphone capture on drop
        #[cfg(target_os = "windows")]
        {
            if let Some(ref mut wasapi) = self.wasapi {
                let _ = wasapi.stop();
            }
            self.wasapi = None;

            if let Some(ref mut mic) = self.mic {
                let _ = mic.stop();
            }
            self.mic = None;
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

#[cfg(test)]
mod progress_parse_tests {
    use super::parse_progress_frame;

    #[test]
    fn parses_unpadded_progress_frame_line() {
        // `-progress pipe:1` emits exactly `frame=N` (no padding).
        assert_eq!(parse_progress_frame("frame=123"), Some(123));
        assert_eq!(parse_progress_frame("frame=0"), Some(0));
    }

    #[test]
    fn tolerates_stray_whitespace_around_value() {
        assert_eq!(parse_progress_frame("frame= 4567 "), Some(4567));
    }

    #[test]
    fn ignores_other_progress_fields() {
        assert_eq!(parse_progress_frame("fps=60.00"), None);
        assert_eq!(parse_progress_frame("out_time=00:00:02.050000"), None);
        assert_eq!(parse_progress_frame("progress=continue"), None);
        // A substring match must NOT trigger: only a leading `frame=` counts.
        assert_eq!(parse_progress_frame("dup_frames=0"), None);
    }

    #[test]
    fn returns_none_for_nonnumeric_frame_value() {
        assert_eq!(parse_progress_frame("frame=N/A"), None);
        assert_eq!(parse_progress_frame("frame="), None);
    }
}

#[cfg(test)]
mod clip_window_tests {
    use super::{compute_clip_window, ensure_usable_clip_window, MIN_CLIP_DURATION_SECS};

    const EPS: f64 = 1e-6;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn long_session_seeks_video_into_concat_and_audio_deep_into_wav() {
        // 20 min session (now=1200), buffer covers last 90s (concat_start=1110),
        // wav started at session start (0). Request the last 60s.
        let w = compute_clip_window(60.0, 1200.0, 1200.0, 1110.0, Some(0.0), None);
        // effective_start = max(1140, 1110) = 1140
        assert!(approx(w.video_ss, 30.0), "video_ss={}", w.video_ss); // 1140 - 1110
        assert!(approx(w.audio_ss, 1140.0), "audio_ss={}", w.audio_ss); // 1140 - 0
        assert!(approx(w.duration, 60.0), "duration={}", w.duration);
    }

    #[test]
    fn short_session_clamps_duration_to_available_and_seeks_from_zero() {
        // 40s session; oldest available content at concat_start=2. Request 60s.
        let w = compute_clip_window(60.0, 40.0, 40.0, 2.0, Some(0.0), None);
        // want_start = -20 -> clamped to concat_start=2 -> effective_start=2
        assert!(approx(w.video_ss, 0.0), "video_ss={}", w.video_ss);
        assert!(approx(w.audio_ss, 2.0), "audio_ss={}", w.audio_ss); // 2 - 0
        assert!(approx(w.duration, 38.0), "duration={}", w.duration); // 40 - 2
    }

    #[test]
    fn no_wav_makes_audio_offset_equal_video_offset() {
        let w = compute_clip_window(60.0, 1200.0, 1200.0, 1110.0, None, None);
        assert!(approx(w.audio_ss, w.video_ss));
        assert!(approx(w.mic_ss, w.video_ss));
        assert!(approx(w.video_ss, 30.0));
    }

    #[test]
    fn request_equal_to_buffer_seeks_from_concat_start() {
        // Request exactly the buffered length (90s) at now=1200, concat_start=1110.
        let w = compute_clip_window(90.0, 1200.0, 1200.0, 1110.0, Some(0.0), None);
        assert!(approx(w.video_ss, 0.0), "video_ss={}", w.video_ss);
        assert!(approx(w.audio_ss, 1110.0), "audio_ss={}", w.audio_ss);
        assert!(approx(w.duration, 90.0), "duration={}", w.duration);
    }

    #[test]
    fn offsets_never_go_negative() {
        // wav_start AFTER effective_start (unusual) must clamp audio_ss to 0, not negative.
        let w = compute_clip_window(60.0, 100.0, 100.0, 40.0, Some(1000.0), None);
        assert!(w.video_ss >= -EPS);
        assert!(w.audio_ss >= -EPS);
        assert!(approx(w.audio_ss, 0.0));
    }

    #[test]
    fn video_and_audio_target_same_wall_clock_instant() {
        // The invariant that fixes A/V sync: concat_start + video_ss == wav_start + audio_ss
        // == effective_start (both derived from the same wall-clock start).
        let now = 1500.0;
        let concat_start = 1400.0;
        let wav_start = 12.0;
        let w = compute_clip_window(75.0, now, now, concat_start, Some(wav_start), None);
        let video_instant = concat_start + w.video_ss;
        let audio_instant = wav_start + w.audio_ss;
        assert!(
            approx(video_instant, audio_instant),
            "video_instant={}, audio_instant={}",
            video_instant,
            audio_instant
        );
    }

    // ---- Microphone input cases ----

    #[test]
    fn long_session_seeks_mic_deep_into_mic_wav() {
        // System wav and mic wav both started at session start (0); mic must seek just
        // as deep as system audio into its continuous wav.
        let w = compute_clip_window(60.0, 1200.0, 1200.0, 1110.0, Some(0.0), Some(0.0));
        assert!(approx(w.video_ss, 30.0), "video_ss={}", w.video_ss);
        assert!(approx(w.audio_ss, 1140.0), "audio_ss={}", w.audio_ss);
        assert!(approx(w.mic_ss, 1140.0), "mic_ss={}", w.mic_ss);
        assert!(approx(w.duration, 60.0), "duration={}", w.duration);
    }

    #[test]
    fn mic_started_later_than_system_seeks_shallower() {
        // Mic capture began 100s after the system wav; its seek must be 100s smaller.
        let w = compute_clip_window(60.0, 1200.0, 1200.0, 1110.0, Some(0.0), Some(100.0));
        assert!(approx(w.audio_ss, 1140.0), "audio_ss={}", w.audio_ss); // 1140 - 0
        assert!(approx(w.mic_ss, 1040.0), "mic_ss={}", w.mic_ss); // 1140 - 100
    }

    #[test]
    fn mic_only_no_system_audio_still_anchors_mic() {
        // System audio off (wav_start=None): audio_ss falls back to video_ss (unused),
        // but the mic still gets its own independent, correct seek.
        let w = compute_clip_window(60.0, 1200.0, 1200.0, 1110.0, None, Some(0.0));
        assert!(approx(w.video_ss, 30.0), "video_ss={}", w.video_ss);
        assert!(approx(w.audio_ss, w.video_ss), "audio_ss={}", w.audio_ss);
        assert!(approx(w.mic_ss, 1140.0), "mic_ss={}", w.mic_ss); // 1140 - 0
    }

    #[test]
    fn mic_offset_never_goes_negative() {
        // mic_start AFTER effective_start must clamp mic_ss to 0, not negative.
        let w = compute_clip_window(60.0, 100.0, 100.0, 40.0, Some(0.0), Some(1000.0));
        assert!(w.mic_ss >= -EPS);
        assert!(approx(w.mic_ss, 0.0), "mic_ss={}", w.mic_ss);
    }

    #[test]
    fn video_system_and_mic_target_same_wall_clock_instant() {
        // Extends the A/V-sync invariant to three streams: every input maps to the same
        // absolute instant `effective_start`.
        let now = 1500.0;
        let concat_start = 1400.0;
        let wav_start = 12.0;
        let mic_start = 37.0;
        let w = compute_clip_window(
            75.0,
            now,
            now,
            concat_start,
            Some(wav_start),
            Some(mic_start),
        );
        let video_instant = concat_start + w.video_ss;
        let audio_instant = wav_start + w.audio_ss;
        let mic_instant = mic_start + w.mic_ss;
        assert!(
            approx(video_instant, audio_instant) && approx(audio_instant, mic_instant),
            "video={}, audio={}, mic={}",
            video_instant,
            audio_instant,
            mic_instant
        );
    }

    // ---- Explicit end anchor (event-anchored clips) ----

    #[test]
    fn past_end_anchor_cuts_the_window_around_the_event_not_around_now() {
        // The regression this guards: an event was detected at t=1150 with pre=5/post=3,
        // but the caller only reached save_clip at t=1200 (queueing + merge + post wait).
        // Anchoring at "now" would return the last 8s of the buffer (1192..1200) — the
        // wrong 8 seconds. The explicit anchor must yield [1145, 1153].
        let event_wall = 1150.0;
        let (pre, post) = (5.0, 3.0);
        let end_anchor = event_wall + post; // 1153
        let available_end = 1200.0; // buffer covers well past the event
        let concat_start = 1110.0;

        let w = compute_clip_window(
            pre + post,
            end_anchor,
            available_end,
            concat_start,
            None,
            None,
        );

        assert!(approx(w.duration, 8.0), "duration={}", w.duration);
        // video_ss is relative to concat_start: 1145 - 1110 = 35
        assert!(approx(w.video_ss, 35.0), "video_ss={}", w.video_ss);
        // The clip must START pre seconds before the event, not before "now".
        assert!(approx(concat_start + w.video_ss, event_wall - pre));
    }

    #[test]
    fn end_anchor_beyond_available_footage_is_clamped_and_keeps_the_start() {
        // Coverage wait timed out (game ended): the window end runs 4s past the last
        // completed segment. The clip must lose its TAIL, not slide backwards — the
        // event has to stay where the caller put it.
        let event_wall = 1190.0;
        let (pre, post) = (10.0, 6.0);
        let end_anchor = event_wall + post; // 1196
        let available_end = 1192.0; // only 2s of post-event footage exists
        let concat_start = 1100.0;

        let w = compute_clip_window(
            pre + post,
            end_anchor,
            available_end,
            concat_start,
            None,
            None,
        );

        // Start stays at event - pre = 1180 -> video_ss = 80
        assert!(approx(w.video_ss, 80.0), "video_ss={}", w.video_ss);
        // Duration is clamped to 1192 - 1180 = 12s (not the requested 16s).
        assert!(approx(w.duration, 12.0), "duration={}", w.duration);
    }

    #[test]
    fn anchor_older_than_the_buffer_never_produces_a_negative_duration() {
        // The whole window predates the rolling buffer (segments already wrapped away).
        let w = compute_clip_window(20.0, 500.0, 1200.0, 1110.0, Some(0.0), Some(0.0));
        assert!(w.duration >= 0.0, "duration={}", w.duration);
        assert!(w.video_ss >= 0.0, "video_ss={}", w.video_ss);
        assert!(w.audio_ss >= 0.0, "audio_ss={}", w.audio_ss);
        assert!(w.mic_ss >= 0.0, "mic_ss={}", w.mic_ss);
    }

    #[test]
    fn anchored_window_keeps_audio_and_video_on_the_same_instant() {
        // The per-input anchoring invariant must survive the explicit end anchor.
        let concat_start = 1400.0;
        let wav_start = 12.0;
        let mic_start = 37.0;
        let w = compute_clip_window(
            30.0,
            1470.0, // end anchor (event + post)
            1490.0, // buffer covers further than the anchor
            concat_start,
            Some(wav_start),
            Some(mic_start),
        );
        assert!(approx(w.duration, 30.0), "duration={}", w.duration);
        assert!(approx(concat_start + w.video_ss, 1440.0));
        assert!(approx(wav_start + w.audio_ss, 1440.0));
        assert!(approx(mic_start + w.mic_ss, 1440.0));
    }

    // ---- Minimum-length guard (no 0.1s "successful" clips) ----
    //
    // These drive the REAL pair of functions the export path uses, in the same order and
    // with the values the failure actually produces: `compute_clip_window` first, then the
    // guard that `save_clip_anchored` calls before it writes the concat list or spawns
    // FFmpeg. What they cannot cover is the FFmpeg-dependent prologue (segment verify +
    // ffprobe), which needs a real recording to exercise.

    #[test]
    fn window_that_fell_out_of_the_rolling_buffer_is_rejected_not_exported() {
        // The A1 failure mode end-to-end: a merge window flushed ~200s late, so the whole
        // requested window predates the 90s buffer. compute_clip_window can only return an
        // empty window, and the export must refuse it instead of writing a 0.1s file.
        let requested = 13.0;
        let w = compute_clip_window(requested, 1000.0, 1200.0, 1110.0, Some(0.0), None);
        assert!(
            w.duration < MIN_CLIP_DURATION_SECS,
            "precondition: duration={}",
            w.duration
        );

        let err = ensure_usable_clip_window(&w, requested, std::path::Path::new("clip.mp4"))
            .expect_err("an empty window must not be exported");
        let message = err.to_string();
        assert!(
            message.contains("너무 짧습니다"),
            "unexpected error: {}",
            message
        );
    }

    #[test]
    fn sub_second_tail_is_rejected_too() {
        // Coverage timed out with only ~0.4s of footage past the window start: still not a
        // clip, and FFmpeg would have exited 0 for it.
        let w = compute_clip_window(16.0, 1196.0, 1190.4, 1190.0, None, None);
        assert!(
            w.duration < MIN_CLIP_DURATION_SECS,
            "duration={}",
            w.duration
        );
        assert!(ensure_usable_clip_window(&w, 16.0, std::path::Path::new("clip.mp4")).is_err());
    }

    #[test]
    fn normal_window_passes_the_guard() {
        let w = compute_clip_window(13.0, 1200.0, 1200.0, 1110.0, Some(0.0), None);
        assert!(approx(w.duration, 13.0), "duration={}", w.duration);
        assert!(ensure_usable_clip_window(&w, 13.0, std::path::Path::new("clip.mp4")).is_ok());
    }
}

#[cfg(test)]
mod concat_list_tests {

    /// 중간이 빠지면 앞쪽을 통째로 버린다.
    ///
    /// 이 함수의 결과는 `concat_start = 마지막 세그먼트 mtime - 길이 합` 으로
    /// 시간축을 되짚는 데 쓰인다. 구멍을 남긴 채 앞 구간을 살려두면 그 구간이
    /// 구멍 길이만큼 미래로 매핑돼, 킬에 앵커된 클립이 엉뚱한 장면을 자른다.
    /// 짧고 정확한 편이 길고 어긋난 것보다 낫다.
    #[test]
    fn a_hole_in_the_middle_discards_everything_before_it() {
        let segments = vec![
            PathBuf::from("s0.mp4"),
            PathBuf::from("s1.mp4"),
            PathBuf::from("s2.mp4"),
            PathBuf::from("s3.mp4"),
        ];
        // s1 측정 실패 → s0·s1 을 버리고 s2·s3 만 남아야 한다.
        let probed = vec![Some(10.0), None, Some(10.0), Some(4.0)];

        let (kept, durations) =
            retain_measured_segments(&segments, &probed).expect("연속 구간이 남는다");

        assert_eq!(kept, vec![PathBuf::from("s2.mp4"), PathBuf::from("s3.mp4")]);
        assert_eq!(durations, vec![10.0, 4.0]);
    }

    /// 실사용에서 압도적으로 흔한 경우: 기록 중인 꼬리 파일 하나만 실패.
    /// 이때는 앞이 전부 살아남아야 한다(버릴 이유가 없다).
    #[test]
    fn only_the_unmeasurable_tail_is_dropped() {
        let segments = vec![
            PathBuf::from("s0.mp4"),
            PathBuf::from("s1.mp4"),
            PathBuf::from("s2.mp4"),
        ];
        let probed = vec![Some(10.0), Some(10.0), None];

        let (kept, durations) =
            retain_measured_segments(&segments, &probed).expect("꼬리만 빠지면 앞은 그대로 쓴다");

        assert_eq!(kept.len(), 2);
        assert_eq!(durations, vec![10.0, 10.0]);
    }

    /// 마지막 하나만 측정되면 그 하나로라도 정확한 클립을 만든다.
    #[test]
    fn a_single_trailing_survivor_is_still_usable() {
        let segments = vec![PathBuf::from("s0.mp4"), PathBuf::from("s1.mp4")];
        let probed = vec![None, Some(7.5)];

        let (kept, durations) =
            retain_measured_segments(&segments, &probed).expect("하나는 남는다");

        assert_eq!(kept, vec![PathBuf::from("s1.mp4")]);
        assert_eq!(durations, vec![7.5]);
    }

    use super::{
        build_concat_list, even_dimensions, parse_ffprobe_duration, retain_measured_segments,
    };
    use std::path::PathBuf;

    #[test]
    fn every_file_line_is_followed_by_a_duration_directive() {
        // Without the duration lines the concat demuxer cannot seek, which is exactly
        // how an 8s request turned into an 18s clip of the wrong footage.
        let segments = vec![
            PathBuf::from("/tmp/segments/segment_000.mp4"),
            PathBuf::from("/tmp/segments/segment_001.mp4"),
        ];
        let list = build_concat_list(&segments, &[10.016, 9.984]);

        let lines: Vec<&str> = list.lines().collect();
        assert_eq!(lines.len(), 4, "list was: {:?}", lines);
        assert_eq!(lines[0], "file '/tmp/segments/segment_000.mp4'");
        assert_eq!(lines[1], "duration 10.016");
        assert_eq!(lines[2], "file '/tmp/segments/segment_001.mp4'");
        assert_eq!(lines[3], "duration 9.984");
        assert!(list.ends_with('\n'));
    }

    #[test]
    fn implausible_durations_are_omitted_rather_than_written() {
        let segments = vec![
            PathBuf::from("a.mp4"),
            PathBuf::from("b.mp4"),
            PathBuf::from("c.mp4"),
        ];
        let list = build_concat_list(&segments, &[0.0, f64::NAN, 10.0]);

        let lines: Vec<&str> = list.lines().collect();
        assert_eq!(
            lines,
            vec![
                "file 'a.mp4'",
                "file 'b.mp4'",
                "file 'c.mp4'",
                "duration 10.000"
            ]
        );
    }

    #[test]
    fn missing_duration_entries_still_emit_the_file_lines() {
        let segments = vec![PathBuf::from("a.mp4"), PathBuf::from("b.mp4")];
        let list = build_concat_list(&segments, &[10.0]);
        assert_eq!(
            list.lines().collect::<Vec<_>>(),
            vec!["file 'a.mp4'", "duration 10.000", "file 'b.mp4'"]
        );
    }

    #[test]
    fn apostrophes_in_the_path_are_escaped_for_the_concat_parser() {
        // C:\Users\O'Brien\... would otherwise terminate the quoted path early.
        let segments = vec![PathBuf::from("/home/o'brien/seg.mp4")];
        let list = build_concat_list(&segments, &[10.0]);
        assert_eq!(
            list.lines().next().unwrap(),
            r"file '/home/o'\''brien/seg.mp4'"
        );
    }

    #[test]
    fn empty_segment_list_yields_empty_content() {
        assert_eq!(build_concat_list(&[], &[]), "");
    }

    // ---- ffprobe duration parsing ----

    #[test]
    fn parses_ffprobe_duration_output() {
        assert_eq!(parse_ffprobe_duration("10.016000\n"), Some(10.016));
        assert_eq!(parse_ffprobe_duration("  9.5  "), Some(9.5));
    }

    #[test]
    fn rejects_unusable_ffprobe_output() {
        assert_eq!(parse_ffprobe_duration("N/A\n"), None);
        assert_eq!(parse_ffprobe_duration(""), None);
        assert_eq!(parse_ffprobe_duration("0.000000\n"), None);
        assert_eq!(parse_ffprobe_duration("-3\n"), None);
    }

    // ---- unmeasurable segments are DROPPED, never assumed to be a full segment long ----

    #[test]
    fn unmeasurable_segment_is_excluded_from_the_concat_list() {
        // The segment FFmpeg is still writing has no moov atom, so ffprobe returns nothing
        // for it. Declaring it `segment_duration_secs` long (the old fallback) put a 10s
        // entry in the list for a partial file, which is how a 13s request produced an 8s
        // clip: every later `duration` directive described a timeline that does not exist.
        //
        // Dropping it is only half the answer though — the survivors must also be
        // CONTIGUOUS. The caller reconstructs wall-clock as
        // `last segment mtime - sum(durations)`, so keeping seg_000 across a hole at
        // seg_001 would place seg_000 ten seconds later than it really is.
        let segments = vec![
            PathBuf::from("seg_000.mp4"),
            PathBuf::from("seg_001.mp4"),
            PathBuf::from("seg_002.mp4"),
        ];
        let (kept, durations) =
            retain_measured_segments(&segments, &[Some(10.016), None, Some(9.984)])
                .expect("the trailing run is usable");

        assert_eq!(
            kept,
            vec![PathBuf::from("seg_002.mp4")],
            "seg_000 sits behind a hole; a shorter clip beats a misaligned one"
        );
        assert_eq!(durations, vec![9.984]);

        let list = build_concat_list(&kept, &durations);
        assert!(!list.contains("seg_001.mp4"), "list was: {}", list);
        assert!(!list.contains("seg_000.mp4"), "list was: {}", list);
        assert_eq!(
            list.lines().collect::<Vec<_>>(),
            vec!["file 'seg_002.mp4'", "duration 9.984"]
        );
    }

    #[test]
    fn every_segment_unmeasurable_is_an_error_not_a_guessed_timeline() {
        let segments = vec![PathBuf::from("a.mp4"), PathBuf::from("b.mp4")];
        let err = retain_measured_segments(&segments, &[None, None])
            .expect_err("a fully unmeasurable buffer must not be exported");
        let message = err.to_string();
        assert!(message.contains("측정"), "unexpected error: {}", message);
    }

    #[test]
    fn fully_measured_segments_pass_through_unchanged() {
        let segments = vec![PathBuf::from("a.mp4"), PathBuf::from("b.mp4")];
        let (kept, durations) =
            retain_measured_segments(&segments, &[Some(10.0), Some(4.5)]).expect("all measurable");
        assert_eq!(kept, segments);
        assert_eq!(durations, vec![10.0, 4.5]);
    }

    #[test]
    fn implausible_measurements_are_dropped_like_outright_failures() {
        // ffprobe can print `0` for a truncated file; a zero/NaN length must never become
        // a concat entry either (it would claim a file with no timeline at all).
        let segments = vec![
            PathBuf::from("a.mp4"),
            PathBuf::from("b.mp4"),
            PathBuf::from("c.mp4"),
        ];
        let (kept, durations) =
            retain_measured_segments(&segments, &[Some(0.0), Some(f64::NAN), Some(12.0)])
                .expect("one segment is measurable");
        assert_eq!(kept, vec![PathBuf::from("c.mp4")]);
        assert_eq!(durations, vec![12.0]);
    }

    #[test]
    fn missing_probe_entries_count_as_failures() {
        // Defensive: a short `probed` slice must drop the unpaired tail rather than index
        // out of bounds or silently keep an unmeasured file.
        let segments = vec![PathBuf::from("a.mp4"), PathBuf::from("b.mp4")];
        let (kept, durations) =
            retain_measured_segments(&segments, &[Some(10.0)]).expect("first is measurable");
        assert_eq!(kept, vec![PathBuf::from("a.mp4")]);
        assert_eq!(durations, vec![10.0]);
    }

    // ---- even_dimensions ----

    #[test]
    fn odd_capture_sizes_are_rounded_down_to_even() {
        // h264/yuv420p aborts on an odd dimension the moment it starts.
        assert_eq!(even_dimensions(1919, 1079), (1918, 1078));
        assert_eq!(even_dimensions(1920, 1080), (1920, 1080));
        assert_eq!(even_dimensions(1, 1), (0, 0));
        assert_eq!(even_dimensions(0, 0), (0, 0));
    }
}

#[cfg(test)]
mod shortfall_tests {
    use super::{classify_shortfall, ClipShortfall};

    #[test]
    fn a_clip_of_the_requested_length_reports_nothing() {
        assert_eq!(classify_shortfall(13.0, 13.0), ClipShortfall::None);
        // `-c copy` cuts on keyframes, so a fraction either way is not a shortfall.
        assert_eq!(classify_shortfall(13.0, 12.9), ClipShortfall::None);
        assert_eq!(classify_shortfall(13.0, 13.4), ClipShortfall::None);
    }

    #[test]
    fn the_observed_regression_is_reported_instead_of_passing_as_success() {
        // Field observation: `요청 13.00s, 창 13.00s, 실제 8.00s` was logged at info level
        // and stored as an ordinary clip.
        assert_eq!(classify_shortfall(13.0, 8.0), ClipShortfall::Partial);
    }

    #[test]
    fn less_than_half_the_request_is_severe() {
        assert_eq!(classify_shortfall(13.0, 6.0), ClipShortfall::Severe);
        assert_eq!(classify_shortfall(60.0, 5.0), ClipShortfall::Severe);
    }

    #[test]
    fn a_non_positive_or_unusable_request_reports_nothing() {
        assert_eq!(classify_shortfall(0.0, 0.0), ClipShortfall::None);
        assert_eq!(classify_shortfall(-1.0, 5.0), ClipShortfall::None);
        assert_eq!(classify_shortfall(f64::NAN, 5.0), ClipShortfall::None);
        assert_eq!(classify_shortfall(13.0, f64::NAN), ClipShortfall::None);
    }
}
