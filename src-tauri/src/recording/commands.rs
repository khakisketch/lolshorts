use super::game_monitor::{GameMode, UnifiedGameStatus};
use super::RecordingStatus;
use crate::error::{AppError, AppResult};
use crate::lcu::PlayerInfo;
use crate::recording::live_client::check_live_client_basic;
use crate::utils::ffmpeg::{get_ffmpeg_path, get_ffprobe_path};
use crate::utils::process::{command_output_with_timeout, command_status_success_with_timeout};
use crate::utils::security;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::State;

#[derive(Serialize, Clone)]
pub struct RecordingStatusInfo {
    pub status: String,
    pub is_monitoring: bool,
    pub buffer_duration_secs: u32,
    pub capture_mode: Option<crate::recording::CaptureMode>,
    pub capture_backend: Option<crate::recording::CaptureBackend>,
    pub capture_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReplayTargetCandidateStatus {
    Unavailable,
    Loading,
    Empty,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayTargetCandidate {
    pub summoner_name: String,
    pub champion_id: u32,
    pub team_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayTargetCandidates {
    pub status: ReplayTargetCandidateStatus,
    pub candidates: Vec<ReplayTargetCandidate>,
    pub selected_target: Option<String>,
    pub retryable: bool,
    pub error: Option<String>,
}

const MIN_RECORDING_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const RECORDING_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Environment checks start subprocesses, enumerate audio devices, and contact
/// the Live Client API. Cache them so status/diagnostic callers cannot create a
/// continuous background workload while the app is idle or in a game.
const RECORDING_READINESS_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordingReadinessCacheKey {
    output_dir: PathBuf,
    encoder_name: String,
    software_fallback_name: String,
    audio_configured: bool,
}

#[derive(Debug, Clone)]
struct RecordingReadinessEnvironmentProbe {
    output_dir_error: Option<String>,
    free_space_bytes: Option<u64>,
    ffmpeg_path: Option<PathBuf>,
    ffmpeg_error: Option<String>,
    ffprobe_available: bool,
    encoder_available: Option<bool>,
    software_fallback_available: Option<bool>,
    system_audio_device_available: bool,
    live_client_available: bool,
}

#[derive(Debug, Clone)]
struct CachedRecordingReadinessEnvironmentProbe {
    key: RecordingReadinessCacheKey,
    checked_at: Instant,
    probe: RecordingReadinessEnvironmentProbe,
}

static RECORDING_READINESS_ENVIRONMENT_CACHE: OnceLock<
    Mutex<Option<CachedRecordingReadinessEnvironmentProbe>>,
> = OnceLock::new();
static RECORDING_READINESS_PROBE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

/// Show the overlay window when a recording actually starts, if the user has it
/// enabled (B4 fix). Previously `overlay::show_overlay` was only called from the
/// auto-detect `game_start_callback` in main.rs, so the F8 hotkey and the
/// `start_recording`/`start_auto_capture` commands emitted `recording-status` to
/// a webview nobody was showing. Settings are re-read on every call (rather than
/// cached at startup) so a toggle in Settings takes effect on the very next
/// recording without an app restart.
async fn apply_overlay_for_recording_start(app_handle: &tauri::AppHandle, state: &AppState) {
    let overlay_enabled = state.recording_settings.read().await.overlay_enabled;
    if overlay_enabled {
        crate::overlay::show_overlay(app_handle);
    }
}

/// Hide the overlay window when a recording actually stops (symmetric counterpart
/// to `apply_overlay_for_recording_start`, B4 fix). Unconditional like
/// `game_end_callback`'s existing hide call — `hide_overlay` no-ops harmlessly if
/// the window was never shown.
fn apply_overlay_for_recording_stop(app_handle: &tauri::AppHandle) {
    crate::overlay::hide_overlay(app_handle);
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecordingReadinessSeverity {
    Blocker,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingReadinessIssue {
    pub code: String,
    pub component: String,
    pub message: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingReadinessComponent {
    pub component: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingReadiness {
    pub ready: bool,
    pub blockers: Vec<RecordingReadinessIssue>,
    pub warnings: Vec<RecordingReadinessIssue>,
    pub components: Vec<RecordingReadinessComponent>,
    #[serde(default)]
    pub public_services: Option<crate::public_service_config::PublicServiceStatus>,
    #[serde(default)]
    pub autostart: Option<crate::autostart::AutostartStatus>,
}

fn issue(code: &str, component: &str, message: &str, action: &str) -> RecordingReadinessIssue {
    RecordingReadinessIssue {
        code: code.to_string(),
        component: component.to_string(),
        message: message.to_string(),
        action: action.to_string(),
    }
}

fn component(component: &str, status: &str, message: &str) -> RecordingReadinessComponent {
    RecordingReadinessComponent {
        component: component.to_string(),
        status: status.to_string(),
        message: message.to_string(),
    }
}

#[derive(Debug, Clone)]
struct RecordingReadinessProbe {
    recording_status: RecordingStatus,
    output_dir: PathBuf,
    output_dir_error: Option<String>,
    free_space_bytes: Option<u64>,
    ffmpeg_path: Option<PathBuf>,
    ffmpeg_error: Option<String>,
    ffprobe_available: bool,
    encoder_name: String,
    encoder_available: Option<bool>,
    software_fallback_available: Option<bool>,
    audio_configured: bool,
    system_audio_device_available: bool,
    lcu_connected: bool,
    live_client_available: bool,
}

fn build_recording_readiness(probe: RecordingReadinessProbe) -> RecordingReadiness {
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    let mut components = Vec::new();

    match probe.recording_status {
        RecordingStatus::Error => {
            blockers.push(issue(
                "recording_status_error",
                "recording_manager",
                "Recording manager is in an error state",
                "Stop recording if needed, restart LoLShorts, then retry",
            ));
            components.push(component(
                "recording_manager",
                "blocker",
                "Recording manager error",
            ));
        }
        RecordingStatus::Processing => {
            blockers.push(issue(
                "recording_status_processing",
                "recording_manager",
                "Recording manager is processing a previous stop/save operation",
                "Wait for processing to finish before starting another recording",
            ));
            components.push(component(
                "recording_manager",
                "blocker",
                "Recording manager is busy processing",
            ));
        }
        RecordingStatus::Recording | RecordingStatus::Buffering => {
            warnings.push(issue(
                "recording_already_active",
                "recording_manager",
                "Recording is already active",
                "No action required unless you intended to restart recording",
            ));
            components.push(component(
                "recording_manager",
                "ok",
                "Recording is already active",
            ));
        }
        RecordingStatus::Idle => {
            components.push(component(
                "recording_manager",
                "ok",
                "Recording manager is idle",
            ));
        }
    }

    if let Some(error) = probe.ffmpeg_error {
        blockers.push(issue(
            "ffmpeg_missing",
            "ffmpeg",
            &format!("FFmpeg is not available: {}", error),
            "Install or bundle FFmpeg, then restart LoLShorts",
        ));
        components.push(component("ffmpeg", "blocker", "FFmpeg binary not found"));
    } else if let Some(path) = probe.ffmpeg_path {
        components.push(component(
            "ffmpeg",
            "ok",
            &format!("FFmpeg found at {}", path.display()),
        ));
    }

    if probe.ffprobe_available {
        components.push(component("ffprobe", "ok", "ffprobe is available"));
    } else {
        warnings.push(issue(
            "ffprobe_unknown",
            "ffprobe",
            "ffprobe is not available or could not be located",
            "Video probing features may be limited; bundle ffprobe beside FFmpeg if needed",
        ));
        components.push(component(
            "ffprobe",
            "warning",
            "ffprobe availability unknown",
        ));
    }

    if let Some(error) = probe.output_dir_error {
        blockers.push(issue(
            "recording_directory_unavailable",
            "storage",
            &format!("Recording directory is not writable: {}", error),
            "Choose a writable recording directory or fix folder permissions",
        ));
        components.push(component(
            "storage",
            "blocker",
            "Recording directory is not writable",
        ));
    } else if let Some(free_bytes) = probe.free_space_bytes {
        if free_bytes < MIN_RECORDING_FREE_BYTES {
            let free_gb = free_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            blockers.push(issue(
                "disk_space_low",
                "storage",
                &format!(
                    "Only {:.1}GB free; recording requires at least 2GB",
                    free_gb
                ),
                "Free disk space or change the recording directory",
            ));
            components.push(component(
                "storage",
                "blocker",
                "Insufficient free disk space",
            ));
        } else {
            components.push(component(
                "storage",
                "ok",
                &format!("Recording directory ready: {}", probe.output_dir.display()),
            ));
        }
    } else {
        warnings.push(issue(
            "disk_space_unknown",
            "storage",
            "Could not determine free disk space",
            "Verify that the recording drive has at least 2GB free",
        ));
        components.push(component(
            "storage",
            "unknown",
            "Free disk space could not be measured",
        ));
    }

    match probe.encoder_available {
        Some(true) => components.push(component(
            "encoder",
            "ok",
            &format!(
                "Configured encoder '{}' is advertised by FFmpeg",
                probe.encoder_name
            ),
        )),
        Some(false) => {
            let fallback_message = if probe.software_fallback_available == Some(true) {
                "Software fallback encoder is available"
            } else {
                "Software fallback encoder availability is unknown"
            };
            warnings.push(issue(
                "encoder_not_advertised",
                "encoder",
                &format!(
                    "Configured encoder '{}' was not found in FFmpeg's encoder list. {}.",
                    probe.encoder_name, fallback_message
                ),
                "Use software encoding or install the matching GPU driver/encoder support",
            ));
            components.push(component(
                "encoder",
                "warning",
                "Configured encoder not advertised",
            ));
        }
        None => {
            warnings.push(issue(
                "encoder_unknown",
                "encoder",
                "Encoder availability could not be probed safely",
                "Run encoder detection from settings or verify FFmpeg encoder support",
            ));
            components.push(component(
                "encoder",
                "unknown",
                "Encoder availability unknown",
            ));
        }
    }

    if probe.encoder_name.to_ascii_lowercase().contains("_nvenc")
        && probe.encoder_available == Some(true)
    {
        components.push(component(
            "support_matrix",
            "ok",
            "NVIDIA NVENC matches the official Windows 11 support target",
        ));
    } else {
        warnings.push(issue(
            "experimental_encoder_support",
            "support_matrix",
            "The configured encoder is outside the official Windows 11 + NVIDIA NVENC support target",
            "Use NVIDIA NVENC for the supported release path, or explicitly continue with the experimental fallback",
        ));
        components.push(component(
            "support_matrix",
            "warning",
            "AMD, Intel, and CPU encoders are experimental fallbacks",
        ));
    }

    if probe.audio_configured && probe.system_audio_device_available {
        components.push(component(
            "audio",
            "ok",
            "A Windows system-audio output device is available",
        ));
    } else if probe.audio_configured {
        warnings.push(issue(
            "audio_device_missing",
            "audio",
            "Audio capture is configured, but no Windows output device was detected",
            "Connect or enable an output device, then retry the readiness check",
        ));
        components.push(component(
            "audio",
            "warning",
            "No output audio device detected",
        ));
    } else {
        warnings.push(issue(
            "audio_disabled",
            "audio",
            "Audio capture is disabled in the recording configuration",
            "Enable audio in settings if clips should include sound",
        ));
        components.push(component("audio", "warning", "Audio capture disabled"));
    }

    warnings.push(issue(
        "capture_hardware_unverified",
        "capture",
        "Hardware/window capture cannot be fully verified until a game window is present",
        "Start or focus League of Legends if recordings are blank",
    ));
    components.push(component(
        "capture",
        "unknown",
        "Capture is runtime/hardware dependent",
    ));

    if probe.lcu_connected {
        components.push(component("lcu", "ok", "LCU connection is available"));
    } else {
        warnings.push(issue(
            "lcu_not_connected",
            "lcu",
            "League Client (LCU) is not connected",
            "Start the League client for automatic replay/live-game context",
        ));
        components.push(component("lcu", "warning", "LCU is not connected"));
    }

    if probe.live_client_available {
        components.push(component(
            "live_client",
            "ok",
            "Live Client API is reachable",
        ));
    } else {
        components.push(component(
            "live_client",
            "unknown",
            "Live Client API is unavailable until an active game/replay is loaded",
        ));
    }

    RecordingReadiness {
        ready: blockers.is_empty(),
        blockers,
        warnings,
        components,
        public_services: None,
        autostart: None,
    }
}

fn candidate_from_player(player: PlayerInfo) -> ReplayTargetCandidate {
    ReplayTargetCandidate {
        summoner_name: player.summoner_name,
        champion_id: player.champion_id,
        team_id: player.team_id,
    }
}

fn build_replay_target_candidates(
    lcu_connected: bool,
    participants: Result<Vec<PlayerInfo>, String>,
    selected_target: Option<String>,
) -> ReplayTargetCandidates {
    if !lcu_connected {
        return ReplayTargetCandidates {
            status: ReplayTargetCandidateStatus::Unavailable,
            candidates: Vec::new(),
            selected_target,
            retryable: true,
            error: Some("LCU is not connected; start or reconnect the League client".to_string()),
        };
    }

    match participants {
        Ok(players) => {
            let candidates: Vec<ReplayTargetCandidate> = players
                .into_iter()
                .filter(|player| !player.summoner_name.trim().is_empty())
                .map(candidate_from_player)
                .collect();

            let status = if candidates.is_empty() {
                ReplayTargetCandidateStatus::Empty
            } else {
                ReplayTargetCandidateStatus::Ready
            };

            ReplayTargetCandidates {
                status,
                candidates,
                selected_target,
                retryable: false,
                error: None,
            }
        }
        Err(error) => {
            let lower = error.to_lowercase();
            let loading = lower.contains("loading")
                || lower.contains("invalid team data")
                || lower.contains("game session");
            ReplayTargetCandidates {
                status: if loading {
                    ReplayTargetCandidateStatus::Loading
                } else {
                    ReplayTargetCandidateStatus::Failed
                },
                candidates: Vec::new(),
                selected_target,
                retryable: true,
                error: Some(error),
            }
        }
    }
}

fn validate_replay_target_selection(
    target: &str,
    readiness: &ReplayTargetCandidates,
) -> Result<(), String> {
    let target = target.trim();
    if target.is_empty() || target.len() > 32 {
        return Err("Summoner name must be 1-32 characters".to_string());
    }

    match readiness.status {
        ReplayTargetCandidateStatus::Ready => {
            let target_lower = target.to_lowercase();
            if readiness
                .candidates
                .iter()
                .any(|candidate| candidate.summoner_name.to_lowercase() == target_lower)
            {
                Ok(())
            } else {
                Err(format!(
                    "Selected replay target '{}' is stale or not in the current replay candidates",
                    target
                ))
            }
        }
        ReplayTargetCandidateStatus::Empty => Err(
            "No replay target candidates are available yet; wait for replay participants to load"
                .to_string(),
        ),
        _ => Ok(()),
    }
}

fn ffmpeg_encoder_available(ffmpeg_path: &Path, encoder_name: &str) -> Option<bool> {
    let mut command = std::process::Command::new(ffmpeg_path);
    command.args(["-hide_banner", "-encoders"]);
    let output =
        command_output_with_timeout(command, RECORDING_PROBE_TIMEOUT, "FFmpeg encoder list")
            .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(stdout.lines().any(|line| line.contains(encoder_name)))
}

fn readiness_cache_is_fresh(checked_at: Instant, now: Instant) -> bool {
    now.checked_duration_since(checked_at)
        .is_some_and(|age| age < RECORDING_READINESS_CACHE_TTL)
}

fn readiness_cache() -> &'static Mutex<Option<CachedRecordingReadinessEnvironmentProbe>> {
    RECORDING_READINESS_ENVIRONMENT_CACHE.get_or_init(|| Mutex::new(None))
}

fn readiness_probe_lock() -> &'static tokio::sync::Mutex<()> {
    RECORDING_READINESS_PROBE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn cached_environment_probe(
    key: &RecordingReadinessCacheKey,
) -> Option<RecordingReadinessEnvironmentProbe> {
    let cache = readiness_cache().lock().ok()?;
    let cached = cache.as_ref()?;
    if cached.key == *key && readiness_cache_is_fresh(cached.checked_at, Instant::now()) {
        Some(cached.probe.clone())
    } else {
        None
    }
}

fn cache_environment_probe(
    key: RecordingReadinessCacheKey,
    probe: RecordingReadinessEnvironmentProbe,
) {
    if let Ok(mut cache) = readiness_cache().lock() {
        *cache = Some(CachedRecordingReadinessEnvironmentProbe {
            key,
            checked_at: Instant::now(),
            probe,
        });
    }
}

fn probe_recording_environment(
    key: &RecordingReadinessCacheKey,
) -> RecordingReadinessEnvironmentProbe {
    let output_dir_error = std::fs::create_dir_all(&key.output_dir)
        .err()
        .map(|error| error.to_string());
    let free_space_bytes = if output_dir_error.is_none() {
        crate::utils::disk::query_disk_space(&key.output_dir)
            .ok()
            .map(|snapshot| snapshot.available_bytes)
    } else {
        None
    };

    let ffmpeg_lookup = get_ffmpeg_path();
    let ffmpeg_path = ffmpeg_lookup.as_ref().ok().cloned();
    let ffmpeg_error = ffmpeg_lookup.err().map(|error| error.to_string());
    let ffprobe_available = get_ffprobe_path().is_ok();
    let encoder_available = ffmpeg_path
        .as_deref()
        .and_then(|path| ffmpeg_encoder_available(path, &key.encoder_name));
    let software_fallback_available = ffmpeg_path
        .as_deref()
        .and_then(|path| ffmpeg_encoder_available(path, &key.software_fallback_name));
    let system_audio_device_available =
        !crate::recording::wasapi_audio::enumerate_system_audio_devices().is_empty();

    RecordingReadinessEnvironmentProbe {
        output_dir_error,
        free_space_bytes,
        ffmpeg_path,
        ffmpeg_error,
        ffprobe_available,
        encoder_available,
        software_fallback_available,
        system_audio_device_available,
        // The Live Client check is asynchronous and is populated by the caller.
        live_client_available: false,
    }
}

async fn collect_environment_probe(
    key: RecordingReadinessCacheKey,
    force_refresh: bool,
) -> RecordingReadinessEnvironmentProbe {
    if !force_refresh {
        if let Some(probe) = cached_environment_probe(&key) {
            return probe;
        }
    }

    // On startup the app, onboarding wizard, and diagnostics screen can all
    // request readiness at once. Serialize cache misses so that concurrent
    // callers do not each spawn FFmpeg probes and enumerate devices.
    let _probe_guard = readiness_probe_lock().lock().await;
    if !force_refresh {
        if let Some(probe) = cached_environment_probe(&key) {
            return probe;
        }
    }

    let probe_key = key.clone();
    let (environment_result, live_client_available) = tokio::join!(
        tokio::task::spawn_blocking(move || probe_recording_environment(&probe_key)),
        check_live_client_basic(),
    );
    let mut environment = environment_result.unwrap_or_else(|error| {
        tracing::warn!(%error, "recording readiness background probe failed");
        RecordingReadinessEnvironmentProbe {
            output_dir_error: None,
            free_space_bytes: None,
            ffmpeg_path: None,
            ffmpeg_error: Some("Readiness background probe did not complete".to_string()),
            ffprobe_available: false,
            encoder_available: None,
            software_fallback_available: None,
            system_audio_device_available: false,
            live_client_available: false,
        }
    });
    environment.live_client_available = live_client_available.is_some();
    cache_environment_probe(key, environment.clone());
    environment
}

pub(crate) async fn collect_recording_readiness(state: &AppState) -> RecordingReadiness {
    collect_recording_readiness_with_cache(state, false).await
}

async fn collect_fresh_recording_readiness(state: &AppState) -> RecordingReadiness {
    collect_recording_readiness_with_cache(state, true).await
}

async fn collect_recording_readiness_with_cache(
    state: &AppState,
    force_refresh: bool,
) -> RecordingReadiness {
    let (recording_status, config) = {
        let manager = state.recording_manager.read().await;
        (manager.get_status().await, manager.get_config().clone())
    };
    let encoder_name = config
        .encoder
        .to_ffmpeg_name_with_hw(config.hw_accel)
        .to_string();
    let software_fallback_name = config.encoder.to_software_fallback().to_string();
    let audio_configured = config.audio_config.is_some();
    let environment = collect_environment_probe(
        RecordingReadinessCacheKey {
            output_dir: config.output_dir.clone(),
            encoder_name: encoder_name.clone(),
            software_fallback_name,
            audio_configured,
        },
        force_refresh,
    )
    .await;
    let lcu_connected = state.lcu_client.lock().await.is_connected();

    let mut readiness = build_recording_readiness(RecordingReadinessProbe {
        recording_status,
        output_dir: config.output_dir,
        output_dir_error: environment.output_dir_error,
        free_space_bytes: environment.free_space_bytes,
        ffmpeg_path: environment.ffmpeg_path,
        ffmpeg_error: environment.ffmpeg_error,
        ffprobe_available: environment.ffprobe_available,
        encoder_name,
        encoder_available: environment.encoder_available,
        software_fallback_available: environment.software_fallback_available,
        audio_configured,
        system_audio_device_available: environment.system_audio_device_available,
        lcu_connected,
        live_client_available: environment.live_client_available,
    });
    let overlay_enabled = state.recording_settings.read().await.overlay_enabled;
    let overlay_status = crate::overlay::capture_exclusion_status();
    if !overlay_enabled {
        readiness.components.push(component(
            "overlay_exclusion",
            "ok",
            "Overlay is disabled, so it cannot enter recording frames",
        ));
    } else if overlay_status == crate::overlay::CaptureExclusionStatus::Applied {
        readiness.components.push(component(
            "overlay_exclusion",
            "ok",
            "Windows capture exclusion is applied to the overlay",
        ));
    } else {
        readiness.warnings.push(issue(
            "overlay_exclusion_unavailable",
            "overlay_exclusion",
            "Overlay capture exclusion is unavailable; the overlay will remain hidden",
            "Restart LoLShorts and review diagnostics before enabling the overlay",
        ));
        readiness.components.push(component(
            "overlay_exclusion",
            "warning",
            "Capture exclusion unavailable; fail-closed hiding is active",
        ));
    }
    readiness.public_services = Some(state.public_service_status.clone());
    readiness.autostart = Some(state.autostart_status.read().await.clone());
    readiness
}

fn readiness_blocker_message(readiness: &RecordingReadiness) -> Option<String> {
    if readiness.blockers.is_empty() {
        None
    } else {
        Some(
            readiness
                .blockers
                .iter()
                .map(|blocker| {
                    format!(
                        "{}: {} ({})",
                        blocker.component, blocker.message, blocker.action
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

pub async fn start_recording_disk_monitor_with_sender(
    app_handle: tauri::AppHandle,
    monitor_sender: std::sync::Arc<tokio::sync::RwLock<Option<tokio::sync::watch::Sender<bool>>>>,
    recordings_dir: PathBuf,
) {
    stop_recording_disk_monitor_with_sender(std::sync::Arc::clone(&monitor_sender)).await;

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    *monitor_sender.write().await = Some(cancel_tx);
    tauri::async_runtime::spawn(
        super::integration_backend::segment_recorder::monitor_disk_space(
            app_handle,
            recordings_dir,
            cancel_rx,
        ),
    );
}

pub async fn stop_recording_disk_monitor_with_sender(
    monitor_sender: std::sync::Arc<tokio::sync::RwLock<Option<tokio::sync::watch::Sender<bool>>>>,
) {
    if let Some(cancel_tx) = monitor_sender.write().await.take() {
        let _ = cancel_tx.send(true);
    }
}

pub async fn start_recording_disk_monitor(
    app_handle: tauri::AppHandle,
    state: &AppState,
    recordings_dir: PathBuf,
) {
    start_recording_disk_monitor_with_sender(
        app_handle,
        std::sync::Arc::clone(&state.recording_disk_monitor),
        recordings_dir,
    )
    .await;
}

pub async fn stop_recording_disk_monitor(state: &AppState) {
    stop_recording_disk_monitor_with_sender(std::sync::Arc::clone(&state.recording_disk_monitor))
        .await;
}

/// Get unified game status - checks Live Client API directly for real-time status
#[tauri::command]
pub async fn get_unified_game_status(state: State<'_, AppState>) -> AppResult<UnifiedGameStatus> {
    // Get base status from game monitor
    let mut status = state.game_monitor.get_unified_status().await;

    // Check Live Client API directly for real-time game detection
    if let Some(info) = check_live_client_basic().await {
        status.in_game = true;
        status.summoner_name = Some(info.summoner_name);
        status.game_time = Some(info.game_time);

        // Detect TFT mode from API game_mode field
        if info.game_mode.contains("TFT") {
            status.game_mode = GameMode::TFT;
            status.champion_name = None; // TFT has no single champion
        } else {
            status.champion_name = Some(info.champion_name);
        }
    } else {
        // No game detected via Live Client API
        status.in_game = false;
        status.summoner_name = None;
        status.champion_name = None;
        status.game_time = None;
        status.game_mode = GameMode::Live;
        // Don't overwrite is_recording - check actual RecordingManager status instead
    }

    // Always derive is_recording from actual RecordingManager state
    let rec_status = state.recording_manager.read().await.get_status().await;
    status.is_recording = matches!(
        rec_status,
        super::RecordingStatus::Recording | super::RecordingStatus::Buffering
    );

    // Expose the real per-session clip count for the game-end notification.
    status.session_clip_count = state.clip_manager.saved_clip_count();

    Ok(status)
}

#[tauri::command]
pub async fn set_recording_target(
    state: State<'_, AppState>,
    summoner_name: Option<String>,
) -> AppResult<()> {
    if let Some(ref name) = summoner_name {
        if name.trim().is_empty() || name.len() > 32 {
            return Err(AppError::Validation(
                "Summoner name must be 1-32 characters".into(),
            ));
        }

        let selected_target = state.game_monitor.get_replay_target().await;
        let readiness = {
            let client = state.lcu_client.lock().await;
            let lcu_connected = client.is_connected();
            let participants = if lcu_connected {
                client
                    .get_game_participants()
                    .await
                    .map_err(|e| e.to_string())
            } else {
                Ok(Vec::new())
            };
            build_replay_target_candidates(lcu_connected, participants, selected_target)
        };

        validate_replay_target_selection(name, &readiness).map_err(AppError::Validation)?;
    }
    state
        .game_monitor
        .set_replay_target(summoner_name.map(|name| name.trim().to_string()))
        .await;
    Ok(())
}

#[tauri::command]
pub async fn get_replay_target_candidates(
    state: State<'_, AppState>,
) -> AppResult<ReplayTargetCandidates> {
    let selected_target = state.game_monitor.get_replay_target().await;
    let client = state.lcu_client.lock().await;
    let lcu_connected = client.is_connected();
    let participants = if lcu_connected {
        client
            .get_game_participants()
            .await
            .map_err(|e| e.to_string())
    } else {
        Ok(Vec::new())
    };

    Ok(build_replay_target_candidates(
        lcu_connected,
        participants,
        selected_target,
    ))
}

/// Notify the backend that a replay has been launched
/// This switches the game mode to Replay for proper event filtering
#[tauri::command]
pub async fn notify_replay_launched(state: State<'_, AppState>) -> AppResult<()> {
    tracing::info!("Replay launched - switching to replay mode");
    // Set replay target to None initially (user will select target via modal)
    state.game_monitor.set_replay_target(None).await;
    Ok(())
}

#[tauri::command]
pub async fn get_recording_readiness(state: State<'_, AppState>) -> AppResult<RecordingReadiness> {
    Ok(collect_recording_readiness(&state).await)
}

#[tauri::command]
pub async fn start_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let current_status = state.recording_manager.read().await.get_status().await;
    if matches!(
        current_status,
        RecordingStatus::Recording | RecordingStatus::Buffering
    ) {
        tracing::warn!(
            "start_recording called but already recording (status: {:?}), ignoring",
            current_status
        );
        return Ok(());
    }

    // Starting capture is safety-critical, so it must not rely on a cached
    // device/disk/encoder result from a previous diagnostics refresh.
    let readiness = collect_fresh_recording_readiness(&state).await;
    if let Some(message) = readiness_blocker_message(&readiness) {
        crate::utils::telemetry::capture_operational_error("recording", "recording_start_blocked");
        return Err(AppError::Recording(format!(
            "Recording readiness blockers: {}",
            message
        )));
    }

    state
        .recording_manager
        .write()
        .await
        .start_recording()
        .await
        .map_err(|e| {
            crate::utils::telemetry::capture_operational_error(
                "recording",
                "recording_start_failed",
            );
            AppError::Recording(e.to_string())
        })?;

    {
        use tauri::Emitter;
        if let Err(e) =
            app_handle.emit("recording-status", serde_json::json!({ "recording": true }))
        {
            tracing::warn!("Failed to emit recording-status event: {}", e);
        }
    }
    apply_overlay_for_recording_start(&app_handle, &state).await;

    let output_dir = state
        .recording_manager
        .read()
        .await
        .get_config()
        .output_dir
        .clone();
    start_recording_disk_monitor(app_handle, &state, output_dir).await;
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    use tauri::Emitter;

    // A session created through start_auto_capture/game detection owns event
    // monitoring, metadata finalization, and the recorder as one pipeline. Raw stop
    // would strand the other two pieces; require the matching public command instead.
    let auto_capture_active = state.clip_manager.is_monitoring().await
        || state
            .recording_manager
            .read()
            .await
            .get_current_game()
            .await
            .is_some();
    reject_raw_stop_during_auto_capture(auto_capture_active)?;

    let current_status = state.recording_manager.read().await.get_status().await;
    if matches!(current_status, RecordingStatus::Idle) {
        tracing::warn!(
            "stop_recording called but not recording (status: {:?}), ignoring",
            current_status
        );
        stop_recording_disk_monitor(&state).await;
        if let Err(e) = app_handle.emit(
            "recording-status",
            serde_json::json!({ "recording": false }),
        ) {
            tracing::warn!("Failed to emit recording-status event: {}", e);
        }
        apply_overlay_for_recording_stop(&app_handle);
        return Ok(());
    }
    let stop_call_result = state.recording_manager.write().await.stop_recording().await;
    stop_recording_disk_monitor(&state).await;

    // B6 fix: `stop_recording()` failing does not guarantee the recorder actually
    // stopped (e.g. the FFmpeg child ignored the stop signal within its internal
    // timeout). Previously this unconditionally emitted `recording: false`, which
    // could turn off the overlay's REC indicator while capture was still running.
    // Re-query the real status on failure instead of assuming success.
    let still_recording = if stop_call_result.is_err() {
        matches!(
            state.recording_manager.read().await.get_status().await,
            RecordingStatus::Recording | RecordingStatus::Buffering
        )
    } else {
        false
    };

    if let Err(e) = app_handle.emit(
        "recording-status",
        serde_json::json!({ "recording": still_recording }),
    ) {
        tracing::warn!("Failed to emit recording-status event: {}", e);
    }
    if !still_recording {
        apply_overlay_for_recording_stop(&app_handle);
    }

    stop_call_result.map(|_| ()).map_err(|e| {
        crate::utils::telemetry::capture_operational_error("recording", "recording_stop_failed");
        AppError::Recording(e.to_string())
    })
}

#[tauri::command]
pub async fn get_recording_status(state: State<'_, AppState>) -> AppResult<String> {
    let status = state.recording_manager.read().await.get_status().await;

    let status_str = match status {
        RecordingStatus::Idle => "idle",
        RecordingStatus::Buffering => "buffering",
        RecordingStatus::Recording => "recording",
        RecordingStatus::Processing => "processing",
        RecordingStatus::Error => "error",
    };

    Ok(status_str.to_string())
}

#[tauri::command]
pub async fn start_auto_capture(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // Auto capture has the same safety requirement as manual start.
    let readiness = collect_fresh_recording_readiness(&state).await;
    if let Some(message) = readiness_blocker_message(&readiness) {
        crate::utils::telemetry::capture_operational_error(
            "recording",
            "auto_capture_start_blocked",
        );
        return Err(AppError::Recording(format!(
            "Recording readiness blockers: {}",
            message
        )));
    }

    // Delegate the begin → start recording → start event monitoring sequence
    // (with rollback) to the single shared pipeline. Manual mode owns event
    // monitoring here.
    let session_context = super::game_lifecycle::GameSessionContext::from_live_client(
        check_live_client_basic().await,
    );
    super::game_lifecycle::start_capture_pipeline(
        state.storage.as_ref(),
        &state.recording_manager,
        &state.clip_manager,
        session_context,
        super::game_lifecycle::CaptureStartMode::Manual,
    )
    .await
    .map_err(|error| {
        crate::utils::telemetry::capture_operational_error(
            "recording",
            "auto_capture_start_failed",
        );
        AppError::Recording(error)
    })?;
    apply_overlay_for_recording_start(&app_handle, &state).await;

    let output_dir = state
        .recording_manager
        .read()
        .await
        .get_config()
        .output_dir
        .clone();
    start_recording_disk_monitor(app_handle, &state, output_dir).await;
    Ok(())
}

#[tauri::command]
pub async fn stop_auto_capture(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // Delegate the stop_event_monitoring (flush) → stop_recording → finalize
    // sequence to the single shared pipeline, which pins the load-bearing order.
    let outcome = super::game_lifecycle::stop_capture_pipeline(
        state.storage.as_ref(),
        &state.recording_manager,
        &state.clip_manager,
    )
    .await;

    stop_recording_disk_monitor(&state).await;
    // B4 fix: hide the overlay symmetrically with the manual-start show above.
    // Unconditional (mirrors `game_end_callback`'s hide in main.rs) rather than
    // gated on outcome success, since a partially-failed stop still means capture
    // is no longer the manual-capture session the overlay was showing for.
    apply_overlay_for_recording_stop(&app_handle);

    // Preserve the original propagation order: event-monitoring stop first, then
    // recording stop, then finalize.
    outcome.event_monitoring.map_err(AppError::Recording)?;
    outcome.recording_stopped.map_err(AppError::Recording)?;
    outcome.finalized.map_err(AppError::Recording).map(|_| ())
}

/// List system audio output (render) devices available for WASAPI loopback
/// capture (contract 2). Returns cpal output-device names.
///
/// Follows the same access policy as `list_audio_devices` — no `require_auth`, so
/// the settings UI can populate the device dropdown before sign-in. On non-Windows
/// platforms WASAPI loopback does not apply, so an empty list is returned.
#[tauri::command]
pub async fn list_system_audio_devices() -> AppResult<Vec<String>> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(crate::recording::wasapi_audio::enumerate_system_audio_devices)
            .await
            .map_err(|error| AppError::Internal(format!("Audio device scan failed: {error}")))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

/// List microphone input (capture) devices (contract 2 sibling). Returns cpal
/// input-device names. Same access policy as `list_system_audio_devices`.
#[tauri::command]
pub async fn list_microphone_devices() -> AppResult<Vec<String>> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(crate::recording::wasapi_audio::enumerate_microphone_devices)
            .await
            .map_err(|error| AppError::Internal(format!("Audio device scan failed: {error}")))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

#[tauri::command]
pub async fn save_replay(state: State<'_, AppState>, duration_secs: u32) -> AppResult<PathBuf> {
    // Validate duration (1-300 seconds)
    security::validate_range(duration_secs as f64, 1.0, 300.0, "duration_secs")
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let current_status = state.recording_manager.read().await.get_status().await;
    if !matches!(
        current_status,
        RecordingStatus::Recording | RecordingStatus::Buffering
    ) {
        return Err(AppError::Recording(format!(
            "Cannot save replay while recorder is {:?}; start recording and wait for buffering first",
            current_status
        )));
    }

    // Actually save the last N seconds from the replay buffer
    // `actual_duration` is measured from the produced file — the buffer can hold
    // less than requested, and storing the request value would misreport the clip.
    let (clip_path, actual_duration) = {
        let recorder = state.recording_manager.read().await;
        recorder
            .save_last_seconds(duration_secs as u64)
            .await
            .map_err(|e| AppError::Recording(format!("Failed to save replay: {}", e)))?
    };

    tracing::info!(
        "Replay saved: {} ({} seconds)",
        clip_path.display(),
        duration_secs
    );

    // FIX: persist ClipMetadata so manual replays enter the library / editor /
    // auto-edit / upload pipeline instead of becoming orphan files on disk.
    // Attach to the active auto-capture session when one exists; otherwise use a
    // per-day manual bucket so the clip is still discoverable.
    if let Err(e) = persist_manual_replay_metadata(
        state.storage.as_ref(),
        state.clip_manager.as_ref(),
        &clip_path,
        actual_duration,
    )
    .await
    {
        tracing::warn!("Failed to persist manual replay metadata: {}", e);
    }

    Ok(clip_path)
}

/// Persist metadata for a manually captured replay clip so it is discoverable by
/// the rest of the app (library / editor / auto-edit / upload).
///
/// Takes `storage`/`clip_manager` directly (rather than a full
/// `State<'_, AppState>`) so it can also be called from non-command call
/// sites that don't have a `State` extractor available -- e.g. a global
/// hotkey handler wired up in `main.rs`, which only has an `AppHandle` and
/// pulls individual managed pieces out of it.
pub async fn persist_manual_replay_metadata(
    storage: &crate::storage::Storage,
    clip_manager: &crate::recording::auto_clip_manager::AutoClipManager,
    clip_path: &Path,
    // Measured length of the produced clip, not the requested one: the rolling
    // buffer may hold less than was asked for, and recording the request value
    // would make auto-edit plan around footage that does not exist.
    measured_duration_secs: f64,
) -> Result<(), String> {
    use crate::storage::models::{ClipMetadata, EventType};

    let game_id = match clip_manager.current_game_id().await {
        Some(id) => id,
        None => {
            // No active auto-capture session — use/create a per-day manual bucket.
            let manual_id = format!("manual_{}", chrono::Utc::now().format("%Y%m%d"));
            if storage.load_game_metadata(&manual_id).is_err() {
                let metadata = crate::storage::GameMetadata {
                    game_id: manual_id.clone(),
                    champion: "Manual".to_string(),
                    game_mode: "MANUAL".to_string(),
                    start_time: chrono::Utc::now(),
                    end_time: None,
                    result: None,
                    kda: None,
                };
                storage
                    .create_game(&manual_id, &metadata)
                    .map_err(|e| format!("Failed to create manual game bucket: {}", e))?;
            }
            manual_id
        }
    };

    let clip_id = format!("manual_{}", chrono::Utc::now().timestamp_millis());
    let clip_meta = ClipMetadata {
        file_path: clip_path.to_string_lossy().to_string(),
        thumbnail_path: None,
        // 수동 저장은 "방금 그거"를 담는 것이라 볼거리가 끝쪽에 있다.
        // 마지막 3초 앞을 대표 지점으로 본다.
        event_offset_secs: (measured_duration_secs - 3.0).max(0.0).into(),
        event_type: EventType::Custom("ManualReplay".to_string()),
        event_time: 0.0,
        priority: 3,
        duration: measured_duration_secs,
        created_at: chrono::Utc::now(),
        usage_count: 0,
        // 사람이 직접 고른 장면이라 중간 이상은 준다(`HighlightKind::ManualSave`).
        // 게임 상황을 찍을 수 없는 경로이므로 배수 없이 기본점 그대로.
        highlight_score: Some(crate::recording::highlight_score::HighlightKind::ManualSave.base()),
        score_reasons: Vec::new(),
    };

    // B5 fix: this used to call `storage.save_clip_metadata` directly, bypassing
    // every emit in `AutoClipManager::save_clip_metadata` — so `save_replay`
    // (and the F8/F9 hotkeys, both of which route through this function) never
    // produced a `clip-saved`/`clip-save-failed` toast. Emit the same events here
    // via `clip_manager.emit_event` (`pub(crate)`, meant for exactly this kind of
    // reuse) with the same payload shape as the auto-capture path, and count the
    // clip through `record_externally_saved_clip` so the end-of-game notification
    // stops under-reporting manual saves.
    if let Err(e) = storage.save_clip_metadata(&game_id, &clip_meta) {
        let reason = format!("Failed to save manual replay metadata: {}", e);
        clip_manager
            .emit_event(
                "clip-save-failed",
                serde_json::json!({
                    "event_name": "ManualReplay",
                    "reason": reason.clone(),
                }),
            )
            .await;
        return Err(reason);
    }

    tracing::info!("Manual replay metadata saved under game {}", game_id);

    clip_manager.record_externally_saved_clip();

    clip_manager
        .emit_event(
            "clip-saved",
            serde_json::json!({
                "clip_id": clip_id,
                "game_id": game_id,
                "event_type": "ManualReplay",
                "event_time": clip_meta.event_time,
                "priority": clip_meta.priority,
                "duration": clip_meta.duration,
                "file_path": clip_meta.file_path,
                "created_at": clip_meta.created_at,
            }),
        )
        .await;

    Ok(())
}

#[tauri::command]
pub async fn get_saved_clips(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::storage::models::ClipMetadata>> {
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || {
        let games = storage.list_games()?;
        let mut all_clips = Vec::new();
        for game_id in games {
            all_clips.extend(storage.load_clip_metadata(&game_id)?);
        }
        Ok::<_, crate::storage::StorageError>(all_clips)
    })
    .await
    .map_err(|error| AppError::Internal(format!("Clip library query failed: {error}")))?
    .map_err(|error| AppError::Database(error.to_string()))
}

#[tauri::command]
pub async fn clear_saved_clips(state: State<'_, AppState>) -> AppResult<()> {
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || {
        for game_id in storage.list_games()? {
            storage.delete_game(&game_id)?;
        }
        Ok::<_, crate::storage::StorageError>(())
    })
    .await
    .map_err(|error| AppError::Internal(format!("Clip library cleanup failed: {error}")))?
    .map_err(|error| AppError::Database(error.to_string()))
}

/// List available audio devices (Cross-platform)
#[tauri::command]
pub async fn list_audio_devices() -> AppResult<Vec<crate::recording::audio::AudioDevice>> {
    use crate::recording::audio::list_audio_devices;

    #[cfg(target_os = "macos")]
    {
        // Use macOS audio enumeration
        let mac_devices = crate::recording::mac_audio::list_audio_devices()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Convert MacAudioDevice to AudioDevice (common interface)
        let devices: Vec<crate::recording::audio::AudioDevice> = mac_devices
            .into_iter()
            .map(|mac_device| crate::recording::audio::AudioDevice {
                name: mac_device.name,
                device_type: if mac_device.is_microphone() {
                    crate::recording::audio::AudioDeviceType::Microphone
                } else if mac_device.is_speaker() {
                    crate::recording::audio::AudioDeviceType::SystemAudio
                } else {
                    crate::recording::audio::AudioDeviceType::SystemAudio // Default fallback
                },
            })
            .collect();

        Ok(devices)
    }

    #[cfg(not(target_os = "macos"))]
    {
        // `list_audio_devices()` now refreshes the cache itself (via
        // `AudioDeviceManager::refresh_if_needed`) before returning, so a
        // cold cache no longer silently reports an empty device list here.
        match list_audio_devices().await {
            Ok(devices) => Ok(devices),
            Err(e) => {
                // If primary enumeration fails, try FFmpeg fallback
                tracing::warn!(
                    "Primary audio device enumeration failed: {}, trying FFmpeg",
                    e
                );
                crate::recording::audio::list_audio_devices_ffmpeg()
                    .map_err(|e| AppError::Internal(format!("FFmpeg fallback also failed: {}", e)))
            }
        }
    }
}

/// Refresh audio device cache and return updated list
#[tauri::command]
pub async fn refresh_audio_devices() -> AppResult<Vec<crate::recording::audio::AudioDevice>> {
    use crate::recording::audio::get_audio_device_manager;

    // Force refresh of audio device cache
    let manager = get_audio_device_manager();

    // Collect devices before and after refresh
    let devices = {
        let mut manager_guard = manager.lock().await;

        manager_guard
            .force_refresh()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to refresh audio devices: {}", e)))?;

        manager_guard.get_devices().to_vec()
    };

    Ok(devices)
}

/// Get audio devices with cache status information
#[tauri::command]
pub async fn get_audio_devices_with_cache_info() -> AppResult<serde_json::Value> {
    use crate::recording::audio::get_audio_device_manager;
    use serde_json::json;

    let manager = get_audio_device_manager();

    // Collect all information within a single lock scope
    let (devices, cache_age, cache_ttl) = {
        let manager_guard = manager.lock().await;

        let devices = manager_guard.get_devices().to_vec();
        let cache_age = manager_guard.last_refresh.elapsed().as_secs();
        let cache_ttl = manager_guard.cache_ttl.as_secs();

        (devices, cache_age, cache_ttl)
    };

    Ok(json!({
        "devices": devices,
        "cache_age_seconds": cache_age,
        "cache_ttl_seconds": cache_ttl,
        "cache_valid": cache_age < cache_ttl,
        "total_devices": devices.len()
    }))
}

/// StatusDashboard용 실시간 상태 정보
#[tauri::command]
pub async fn get_detailed_recording_status(
    state: State<'_, AppState>,
) -> AppResult<RecordingStatusInfo> {
    let manager = state.recording_manager.read().await;
    let status = manager.get_status().await;

    let is_monitoring =
        state.clip_manager.is_monitoring().await || manager.get_current_game().await.is_some();
    let buffer_duration = manager.get_config().buffer_duration_secs as u32;
    let (capture_mode, capture_backend, capture_warning) = manager.get_capture_diagnostics().await;

    let status_str = match status {
        RecordingStatus::Idle => "idle",
        RecordingStatus::Buffering => "buffering",
        RecordingStatus::Recording => "recording",
        RecordingStatus::Processing => "processing",
        RecordingStatus::Error => "error",
    };

    Ok(RecordingStatusInfo {
        status: status_str.to_string(),
        is_monitoring,
        buffer_duration_secs: buffer_duration,
        capture_mode,
        capture_backend,
        capture_warning,
    })
}

fn reject_raw_stop_during_auto_capture(auto_capture_active: bool) -> AppResult<()> {
    if auto_capture_active {
        return Err(AppError::Recording(
            "stop_recording cannot stop an automatic capture session; use stop_auto_capture"
                .to_string(),
        ));
    }
    Ok(())
}

/// Label the *actual* capture resolution for `get_recording_quality_info`.
///
/// On Windows, the gdigrab capture command (see
/// `integration_backend::segment_recorder`) never passes `config.resolution`
/// as `-video_size` -- it captures at the real HWND window rect, the target
/// monitor's actual bounds, or (with neither set) the native desktop size,
/// unconditionally. Echoing `config.resolution` there would misreport a
/// configured-but-unused settings value as if it were the measured capture
/// resolution, so report it honestly as native instead. Other platforms
/// (Linux's x11grab) do apply `config.resolution` to the actual capture, so
/// echoing it remains honest there.
fn recording_quality_resolution_label(resolution: (u32, u32)) -> String {
    #[cfg(target_os = "windows")]
    {
        let _ = resolution; // unused on this platform, kept for signature parity
        "native (game window)".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("{}x{}", resolution.0, resolution.1)
    }
}

/// Get recording quality info (encoder, bitrate, resolution)
#[tauri::command]
pub async fn get_recording_quality_info(
    state: State<'_, AppState>,
) -> AppResult<serde_json::Value> {
    use serde_json::json;

    // Get actual configuration from the recording manager
    let recorder = state.recording_manager.read().await;
    let config = recorder.get_config();

    let encoder_name = config.encoder.to_ffmpeg_name();
    let codec_name = match config.encoder {
        super::VideoEncoder::H264 => "H.264/AVC",
        super::VideoEncoder::H265 => "H.265/HEVC",
    };

    Ok(json!({
        "encoder": encoder_name,
        "codec": codec_name,
        "resolution": recording_quality_resolution_label(config.resolution),
        "fps": config.fps,
        "bitrate_mbps": config.bitrate as f64 / 1_000_000.0,
        "audio_enabled": config.audio_config.is_some(),
    }))
}

/// Detect available hardware encoders on the system
/// Returns list of available encoders with their capabilities
#[tauri::command]
pub async fn detect_available_encoders() -> AppResult<serde_json::Value> {
    use serde_json::json;

    let available_encoders = tokio::task::spawn_blocking(|| {
        use std::process::Command;

        // Test function for encoder availability
        fn test_encoder(encoder_name: &str, _codec: &str) -> bool {
            let ffmpeg_path = match get_ffmpeg_path() {
                Ok(path) => path,
                Err(_) => return false,
            };
            let mut command = Command::new(ffmpeg_path);
            command.args([
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=256x256:d=0.1",
                "-c:v",
                encoder_name,
                "-f",
                "null",
                "-",
            ]);

            command_status_success_with_timeout(
                command,
                RECORDING_PROBE_TIMEOUT,
                "FFmpeg encoder smoke test",
            )
            .unwrap_or(false)
        }

        // Test all encoder types
        let mut encoders = Vec::new();

        // Test NVENC (NVIDIA)
        if test_encoder("hevc_nvenc", "h265") {
            encoders.push(json!({
                "id": "nvenc",
                "name": "NVIDIA NVENC",
                "type": "hardware",
                "vendor": "NVIDIA",
                "codecs": ["h264", "h265"],
                "h264_encoder": "h264_nvenc",
                "h265_encoder": "hevc_nvenc",
                "performance": "excellent",
                "quality": "high",
            }));
        }

        // Test QSV (Intel Quick Sync)
        if test_encoder("hevc_qsv", "h265") {
            encoders.push(json!({
                "id": "qsv",
                "name": "Intel Quick Sync",
                "type": "hardware",
                "vendor": "Intel",
                "codecs": ["h264", "h265"],
                "h264_encoder": "h264_qsv",
                "h265_encoder": "hevc_qsv",
                "performance": "excellent",
                "quality": "high",
            }));
        }

        // Test AMF (AMD)
        if test_encoder("hevc_amf", "h265") {
            encoders.push(json!({
                "id": "amf",
                "name": "AMD AMF",
                "type": "hardware",
                "vendor": "AMD",
                "codecs": ["h264", "h265"],
                "h264_encoder": "h264_amf",
                "h265_encoder": "hevc_amf",
                "performance": "excellent",
                "quality": "high",
            }));
        }

        // Software encoder is always available
        encoders.push(json!({
            "id": "software",
            "name": "Software (CPU)",
            "type": "software",
            "vendor": "FFmpeg",
            "codecs": ["h264", "h265", "av1"],
            "h264_encoder": "libx264",
            "h265_encoder": "libx265",
            "av1_encoder": "libsvtav1",
            "performance": "slow",
            "quality": "excellent",
        }));

        // Determine automatically detected encoder (first available hardware, or software)
        let auto_encoder = if encoders.len() > 1 {
            encoders[0]["id"].as_str().unwrap_or("software").to_string()
        } else {
            "software".to_string()
        };

        let total_count = encoders.len();
        json!({
            "available": encoders,
            "auto_detected": auto_encoder,
            "total_count": total_count,
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("Encoder detection failed: {}", e)))?;

    Ok(available_encoders)
}

#[tauri::command]
pub async fn get_disk_usage_info(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let recordings_dir = state.recordings_dir.clone();
    let logs_dir = state.storage.base_path().join("logs");
    tokio::task::spawn_blocking(move || collect_disk_usage_info(recordings_dir, logs_dir))
        .await
        .map_err(|error| AppError::Internal(format!("Disk usage scan failed: {error}")))
}

fn collect_disk_usage_info(recordings_dir: PathBuf, logs_dir: PathBuf) -> serde_json::Value {
    use serde_json::json;

    // The rolling-buffer segments (the actual temp footage + loopback WAV) live in
    // recordings/segments, not the non-existent recordings/temp_segments the UI used
    // to scan (which always reported ~0 bytes).
    let temp_dir = recordings_dir.join("segments");

    // The application directories contain nested per-game and per-job folders.
    // Reuse the recursive storage scanner; the previous one-level sum counted
    // directory entries as zero bytes and drastically under-reported usage.
    let recordings_size = crate::storage::dir_size_bytes(&recordings_dir);
    let temp_size = crate::storage::dir_size_bytes(&temp_dir);
    let logs_size = crate::storage::dir_size_bytes(&logs_dir);

    // One atomic volume probe keeps total/free values from different moments
    // from being combined and halves the WMI/Win32 call overhead.
    let disk_space = crate::utils::disk::query_disk_space(&recordings_dir).ok();
    let disk_known = disk_space.is_some();
    let (total_space, free_space) = disk_space
        .map(|snapshot| (snapshot.total_bytes, snapshot.available_bytes))
        .unwrap_or((0, 0));
    let used_space = total_space.saturating_sub(free_space);

    json!({
        "disk_space_known": disk_known,
        "total_space_gb": (total_space as f64) / (1024.0 * 1024.0 * 1024.0),
        "free_space_gb": (free_space as f64) / (1024.0 * 1024.0 * 1024.0),
        "used_space_gb": (used_space as f64) / (1024.0 * 1024.0 * 1024.0),
        "recordings_gb": (recordings_size as f64) / (1024.0 * 1024.0 * 1024.0),
        "temp_files_gb": (temp_size as f64) / (1024.0 * 1024.0 * 1024.0),
        "logs_gb": (logs_size as f64) / (1024.0 * 1024.0 * 1024.0),
        "cleanup_needed": temp_size > (1024 * 1024 * 1024 * 2), // > 2GB
        "recommendations": {
            "cleanup_temp": temp_size > 0,
            "archive_old_recordings": recordings_size > (1024 * 1024 * 1024 * 20), // > 20GB
            "low_disk_space": disk_known && free_space < (1024 * 1024 * 1024 * 10) // < 10GB free
        }
    })
}

#[tauri::command]
pub async fn cleanup_temp_files(state: State<'_, AppState>) -> AppResult<u64> {
    // Actual rolling-buffer footage lives in recordings/segments (not temp_segments).
    let segments_dir = state.recordings_dir.join("segments");

    // Always run the standard startup cleanup (logs + legacy temp dirs).
    state
        .cleanup_manager
        .cleanup_on_startup()
        .await
        .map_err(|e| AppError::Io(format!("Cleanup failed: {}", e)))?;

    // Never delete the live rolling buffer while a recording is in progress — those
    // segments are the active replay window.
    let rec_status = state.recording_manager.read().await.get_status().await;
    if matches!(
        rec_status,
        RecordingStatus::Recording | RecordingStatus::Buffering
    ) {
        tracing::info!("cleanup_temp_files: recording active, leaving segment buffer intact");
        return Ok(0);
    }

    // Remove stale rolling-buffer segments and the loopback WAV left over from the
    // last finished session (safe when idle; mirrors cleanup_on_shutdown). Directory
    // enumeration and deletes are blocking on Windows, so keep them off Tokio workers.
    let freed = tokio::task::spawn_blocking(move || {
        if !segments_dir.exists() {
            return 0;
        }
        let mut freed: u64 = 0;
        if let Ok(entries) = std::fs::read_dir(&segments_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let is_segment_artifact = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| {
                        matches!(
                            ext.to_ascii_lowercase().as_str(),
                            "mp4" | "ts" | "wav" | "txt"
                        )
                    })
                    .unwrap_or(false);
                if !is_segment_artifact {
                    continue;
                }
                let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                match std::fs::remove_file(&path) {
                    Ok(()) => freed += size,
                    Err(error) => {
                        tracing::warn!("Failed to remove segment file {:?}: {}", path, error)
                    }
                }
            }
        }
        freed
    })
    .await
    .map_err(|error| AppError::Internal(format!("Segment cleanup task failed: {error}")))?;

    tracing::info!(
        "cleanup_temp_files: freed {} bytes from segment buffer",
        freed
    );
    Ok(freed)
}

#[tauri::command]
pub async fn get_performance_stats(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    use serde_json::json;

    // Get actual recording stats from the manager
    let manager = state.recording_manager.read().await;
    let stats = manager.get_stats().await;

    Ok(json!({
        "recording": {
            "total_frames": stats.total_frames,
            "uptime_seconds": stats.uptime_seconds,
            "current_fps": stats.current_fps,
            "audio_active": stats.audio_active,
            "mic_active": stats.mic_active
        },
        "system": {
            "status": format!("{:?}", manager.get_status().await)
        }
    }))
}

/// Get recording backend information
#[tauri::command]
pub async fn get_recording_backend_info(
    _state: State<'_, AppState>,
) -> AppResult<serde_json::Value> {
    use serde_json::json;

    // Simplified backend info for production stability
    Ok(json!({
        "backend_type": "integration_backend",
        "platform": std::env::consts::OS,
        "version": "1.2.0",
        "status": "production_ready"
    }))
}

// Screenshot capture moved to screenshot::commands module

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_environment_cache_expires_after_ttl() {
        let now = Instant::now();
        assert!(readiness_cache_is_fresh(now, now));
        assert!(readiness_cache_is_fresh(
            now,
            now + RECORDING_READINESS_CACHE_TTL - Duration::from_millis(1)
        ));
        assert!(!readiness_cache_is_fresh(
            now,
            now + RECORDING_READINESS_CACHE_TTL
        ));
    }

    #[test]
    fn raw_stop_rejects_an_owned_auto_capture_session() {
        let error = reject_raw_stop_during_auto_capture(true)
            .expect_err("raw stop must not tear down only one part of the pipeline");
        let message = error.to_string();
        assert!(
            message.contains("stop_auto_capture"),
            "unexpected error: {message}"
        );
        assert!(reject_raw_stop_during_auto_capture(false).is_ok());
    }

    fn player(name: &str, champion_id: u32, team_id: u32) -> PlayerInfo {
        PlayerInfo {
            summoner_name: name.to_string(),
            champion_id,
            team_id,
        }
    }

    #[test]
    fn replay_candidates_ready_when_participants_exist() {
        let readiness = build_replay_target_candidates(
            true,
            Ok(vec![player("Faker", 1, 100), player("Gumayusi", 2, 100)]),
            Some("Faker".to_string()),
        );

        assert_eq!(readiness.status, ReplayTargetCandidateStatus::Ready);
        assert_eq!(readiness.candidates.len(), 2);
        assert_eq!(readiness.selected_target.as_deref(), Some("Faker"));
        assert!(!readiness.retryable);
    }

    #[test]
    fn replay_target_validation_rejects_stale_target_when_ready() {
        let readiness =
            build_replay_target_candidates(true, Ok(vec![player("Faker", 1, 100)]), None);

        let error = validate_replay_target_selection("Chovy", &readiness).unwrap_err();

        assert!(error.contains("stale"));
    }

    #[test]
    fn replay_target_validation_allows_target_when_candidates_unavailable() {
        let readiness = build_replay_target_candidates(false, Ok(Vec::new()), None);

        assert!(validate_replay_target_selection("Faker", &readiness).is_ok());
    }

    #[test]
    fn recording_readiness_blocks_missing_ffmpeg_and_low_disk() {
        let readiness = build_recording_readiness(RecordingReadinessProbe {
            recording_status: RecordingStatus::Idle,
            output_dir: PathBuf::from("C:/recordings"),
            output_dir_error: None,
            free_space_bytes: Some(1024),
            ffmpeg_path: None,
            ffmpeg_error: Some("not found".to_string()),
            ffprobe_available: false,
            encoder_name: "h264_nvenc".to_string(),
            encoder_available: None,
            software_fallback_available: None,
            audio_configured: true,
            system_audio_device_available: true,
            lcu_connected: false,
            live_client_available: false,
        });

        assert!(!readiness.ready);
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "ffmpeg_missing"));
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "disk_space_low"));
    }

    #[test]
    fn recording_readiness_warns_for_unverified_hardware_capture() {
        let readiness = build_recording_readiness(RecordingReadinessProbe {
            recording_status: RecordingStatus::Idle,
            output_dir: PathBuf::from("C:/recordings"),
            output_dir_error: None,
            free_space_bytes: Some(MIN_RECORDING_FREE_BYTES),
            ffmpeg_path: Some(PathBuf::from("C:/ffmpeg/bin/ffmpeg.exe")),
            ffmpeg_error: None,
            ffprobe_available: true,
            encoder_name: "libx264".to_string(),
            encoder_available: Some(true),
            software_fallback_available: Some(true),
            audio_configured: true,
            system_audio_device_available: true,
            lcu_connected: true,
            live_client_available: false,
        });

        assert!(readiness.ready);
        assert!(readiness
            .warnings
            .iter()
            .any(|warning| warning.code == "capture_hardware_unverified"));
    }

    // ---- recording_quality_resolution_label ----

    #[test]
    #[cfg(target_os = "windows")]
    fn recording_quality_resolution_label_reports_native_on_windows() {
        // Regression: Windows capture (gdigrab) never actually applies
        // `config.resolution` to the capture command -- it must not be
        // echoed back as if it were the measured resolution.
        assert_eq!(
            recording_quality_resolution_label((1920, 1080)),
            "native (game window)"
        );
        assert_eq!(
            recording_quality_resolution_label((2560, 1440)),
            "native (game window)"
        );
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn recording_quality_resolution_label_echoes_configured_value_elsewhere() {
        // Linux's x11grab does apply `config.resolution` to the actual
        // capture command, so echoing it there remains honest.
        assert_eq!(
            recording_quality_resolution_label((1920, 1080)),
            "1920x1080"
        );
    }

    // ---- persist_manual_replay_metadata ----
    //
    // These construct `AutoClipManager` directly (mirroring the helper in
    // `auto_clip_manager.rs`'s own tests) rather than going through
    // `AppState`/`State`, exercising the exact call shape a non-command
    // caller (e.g. a global hotkey handler) would use.

    async fn test_clip_manager(
        storage: std::sync::Arc<crate::storage::Storage>,
        output_dir: &std::path::Path,
    ) -> crate::recording::auto_clip_manager::AutoClipManager {
        use crate::recording::integration_backend::{RecordingConfig, WindowsCaptureRecorder};
        use crate::settings::models::RecordingSettings;
        use tokio::sync::RwLock as TokioRwLock;

        let recorder_config = RecordingConfig {
            output_dir: output_dir.to_path_buf(),
            ..Default::default()
        };
        let recorder = WindowsCaptureRecorder::new(recorder_config).await.unwrap();
        let recorder_arc = std::sync::Arc::new(TokioRwLock::new(recorder));
        let settings = std::sync::Arc::new(TokioRwLock::new(RecordingSettings::default()));

        crate::recording::auto_clip_manager::AutoClipManager::new(recorder_arc, storage, settings)
    }

    #[tokio::test]
    async fn persist_manual_replay_metadata_creates_manual_bucket_when_no_active_game() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = std::sync::Arc::new(crate::storage::Storage::new(temp_dir.path()).unwrap());
        let clip_manager = test_clip_manager(storage.clone(), temp_dir.path()).await;

        let clip_path = temp_dir.path().join("manual-replay.mp4");

        persist_manual_replay_metadata(&storage, &clip_manager, &clip_path, 30.0)
            .await
            .expect("should persist metadata under a manual bucket");

        let expected_game_id = format!("manual_{}", chrono::Utc::now().format("%Y%m%d"));
        let games = storage.list_games().unwrap();
        assert!(
            games.contains(&expected_game_id),
            "expected manual bucket {} to be created, got {:?}",
            expected_game_id,
            games
        );

        let clips = storage.load_clip_metadata(&expected_game_id).unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].duration, 30.0);
        assert_eq!(clips[0].file_path, clip_path.to_string_lossy().to_string());
    }

    #[tokio::test]
    async fn persist_manual_replay_metadata_attaches_to_active_auto_capture_session() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = std::sync::Arc::new(crate::storage::Storage::new(temp_dir.path()).unwrap());
        let clip_manager = test_clip_manager(storage.clone(), temp_dir.path()).await;

        clip_manager
            .set_current_game(Some("active-game".to_string()))
            .await;

        let clip_path = temp_dir.path().join("manual-replay-2.mp4");

        persist_manual_replay_metadata(&storage, &clip_manager, &clip_path, 45.0)
            .await
            .expect("should persist metadata under the active game session");

        let clips = storage.load_clip_metadata("active-game").unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].duration, 45.0);

        // No manual bucket should have been created when a session is active.
        let expected_manual_id = format!("manual_{}", chrono::Utc::now().format("%Y%m%d"));
        assert!(!storage.list_games().unwrap().contains(&expected_manual_id));
    }
}
