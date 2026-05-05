use super::game_monitor::{GameMode, UnifiedGameStatus};
use super::RecordingStatus;
use crate::auth::middleware::require_auth;
use crate::error::{AppError, AppResult};
use crate::lcu::PlayerInfo;
use crate::recording::live_client::check_live_client_basic;
use crate::utils::ffmpeg::get_ffmpeg_path;
use crate::utils::process::{command_output_with_timeout, command_status_success_with_timeout};
use crate::utils::security;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::State;

#[derive(Serialize, Clone)]
pub struct RecordingStatusInfo {
    pub status: String,
    pub is_monitoring: bool,
    pub buffer_duration_secs: u32,
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

    if probe.audio_configured {
        warnings.push(issue(
            "audio_hardware_unverified",
            "audio",
            "Audio capture is configured, but hardware/device capture was not verified in readiness",
            "Use the audio device list if recording has no sound",
        ));
        components.push(component(
            "audio",
            "unknown",
            "Audio hardware not probed by readiness",
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

fn find_ffprobe_for_ffmpeg(ffmpeg_path: &Path) -> bool {
    let ffprobe_name = if cfg!(target_os = "windows") {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };

    if ffmpeg_path
        .parent()
        .map(|parent| parent.join(ffprobe_name).exists())
        .unwrap_or(false)
    {
        return true;
    }

    let path_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let mut command = std::process::Command::new(path_cmd);
    command.arg("ffprobe");
    command_status_success_with_timeout(command, RECORDING_PROBE_TIMEOUT, "ffprobe PATH lookup")
        .unwrap_or(false)
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

pub(crate) async fn collect_recording_readiness(state: &AppState) -> RecordingReadiness {
    let (recording_status, config) = {
        let manager = state.recording_manager.read().await;
        (manager.get_status().await, manager.get_config().clone())
    };

    let output_dir_error = std::fs::create_dir_all(&config.output_dir)
        .err()
        .map(|error| error.to_string());
    let free_space_bytes = if output_dir_error.is_none() {
        get_free_disk_space(&config.output_dir)
    } else {
        None
    };

    let ffmpeg_lookup = get_ffmpeg_path();
    let ffmpeg_path = ffmpeg_lookup.as_ref().ok().cloned();
    let ffmpeg_error = ffmpeg_lookup.err().map(|error| error.to_string());
    let ffprobe_available = ffmpeg_path
        .as_deref()
        .map(find_ffprobe_for_ffmpeg)
        .unwrap_or(false);
    let encoder_name = config
        .encoder
        .to_ffmpeg_name_with_hw(config.hw_accel)
        .to_string();
    let encoder_available = ffmpeg_path
        .as_deref()
        .and_then(|path| ffmpeg_encoder_available(path, &encoder_name));
    let software_fallback = config.encoder.to_software_fallback().to_string();
    let software_fallback_available = ffmpeg_path
        .as_deref()
        .and_then(|path| ffmpeg_encoder_available(path, &software_fallback));
    let audio_configured = config.audio_config.is_some();
    let lcu_connected = state.lcu_client.lock().await.is_connected();
    let live_client_available = check_live_client_basic().await.is_some();

    build_recording_readiness(RecordingReadinessProbe {
        recording_status,
        output_dir: config.output_dir,
        output_dir_error,
        free_space_bytes,
        ffmpeg_path,
        ffmpeg_error,
        ffprobe_available,
        encoder_name,
        encoder_available,
        software_fallback_available,
        audio_configured,
        lcu_connected,
        live_client_available,
    })
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

    let readiness = collect_recording_readiness(&state).await;
    if let Some(message) = readiness_blocker_message(&readiness) {
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
        .map_err(|e| AppError::Recording(e.to_string()))?;

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
pub async fn stop_recording(state: State<'_, AppState>) -> AppResult<()> {
    let current_status = state.recording_manager.read().await.get_status().await;
    if matches!(current_status, RecordingStatus::Idle) {
        tracing::warn!(
            "stop_recording called but not recording (status: {:?}), ignoring",
            current_status
        );
        stop_recording_disk_monitor(&state).await;
        return Ok(());
    }
    let stop_result = state
        .recording_manager
        .write()
        .await
        .stop_recording()
        .await
        .map(|_| ())
        .map_err(|e| AppError::Recording(e.to_string()));
    stop_recording_disk_monitor(&state).await;
    stop_result
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
    let readiness = collect_recording_readiness(&state).await;
    if let Some(message) = readiness_blocker_message(&readiness) {
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
        .map_err(|e| AppError::Recording(e.to_string()))?;

    state
        .clip_manager
        .start_event_monitoring()
        .await
        .map_err(|e| AppError::Recording(e.to_string()))?;

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
pub async fn stop_auto_capture(state: State<'_, AppState>) -> AppResult<()> {
    let clip_stop_result = state
        .clip_manager
        .stop_event_monitoring()
        .await
        .map_err(|e| AppError::Recording(e.to_string()));

    let recording_stop_result = state
        .recording_manager
        .write()
        .await
        .stop_recording()
        .await
        .map_err(|e| AppError::Recording(e.to_string()));

    stop_recording_disk_monitor(&state).await;
    clip_stop_result?;
    recording_stop_result.map(|_| ())
}

#[tauri::command]
pub async fn save_replay(state: State<'_, AppState>, duration_secs: u32) -> AppResult<PathBuf> {
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    let recorder = state.recording_manager.read().await;

    let clip_path = recorder
        .save_last_seconds(duration_secs)
        .await
        .map_err(|e| AppError::Recording(format!("Failed to save replay: {}", e)))?;

    tracing::info!(
        "Replay saved: {} ({} seconds)",
        clip_path.display(),
        duration_secs
    );
    Ok(clip_path)
}

#[tauri::command]
pub async fn get_saved_clips(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::storage::models::ClipMetadata>> {
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

    // Get all games
    let games = state
        .storage
        .list_games()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Collect all clips from all games
    let mut all_clips = Vec::new();
    for game_id in games {
        let clips = state
            .storage
            .load_clip_metadata(&game_id)
            .map_err(|e| AppError::Database(e.to_string()))?;
        all_clips.extend(clips);
    }

    Ok(all_clips)
}

#[tauri::command]
pub async fn clear_saved_clips(state: State<'_, AppState>) -> AppResult<()> {
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

    // Get all games and delete them
    let games = state
        .storage
        .list_games()
        .map_err(|e| AppError::Database(e.to_string()))?;

    for game_id in games {
        state
            .storage
            .delete_game(&game_id)
            .map_err(|e| AppError::Database(e.to_string()))?;
    }

    Ok(())
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
        // Use Windows audio enumeration with fallbacks
        match list_audio_devices() {
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

    let is_monitoring = state.clip_manager.is_monitoring().await;
    let buffer_duration = manager.get_config().buffer_duration_secs as u32;

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
    })
}

/// Get recording quality info (encoder, bitrate, resolution)
#[tauri::command]
pub async fn get_recording_quality_info(
    state: State<'_, AppState>,
) -> AppResult<serde_json::Value> {
    use serde_json::json;

    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
        "resolution": format!("{}x{}", config.resolution.0, config.resolution.1),
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
pub async fn get_disk_usage_info() -> AppResult<serde_json::Value> {
    use serde_json::json;
    use std::fs;

    // Use actual app data directory (same as main.rs)
    let app_data = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lolshorts");

    let recordings_dir = app_data.join("recordings");
    let temp_dir = app_data.join("recordings").join("temp_segments");
    let logs_dir = app_data.join("logs");

    // Calculate directory sizes
    let get_dir_size = |dir: &Path| -> u64 {
        if !dir.exists() {
            return 0;
        }

        fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    };

    let recordings_size = get_dir_size(&recordings_dir);
    let temp_size = get_dir_size(&temp_dir);
    let logs_size = get_dir_size(&logs_dir);

    // Get total disk space (returns None if the OS query fails)
    let total_space = get_total_disk_space(&app_data);
    let free_space = get_free_disk_space(&app_data);
    let disk_known = total_space.is_some() && free_space.is_some();
    let total_space = total_space.unwrap_or(0);
    let free_space = free_space.unwrap_or(0);
    let used_space = total_space.saturating_sub(free_space);

    Ok(json!({
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
            "low_disk_space": free_space < (1024 * 1024 * 1024 * 10) // < 10GB free
        }
    }))
}

#[tauri::command]
pub async fn cleanup_temp_files(state: State<'_, AppState>) -> AppResult<u64> {
    let temp_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lolshorts")
        .join("recordings")
        .join("temp_segments");

    let size_before = if temp_dir.exists() {
        std::fs::read_dir(&temp_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum::<u64>()
            })
            .unwrap_or(0)
    } else {
        0
    };

    state
        .cleanup_manager
        .cleanup_on_startup()
        .await
        .map_err(|e| AppError::Io(format!("Cleanup failed: {}", e)))?;

    let size_after = if temp_dir.exists() {
        std::fs::read_dir(&temp_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum::<u64>()
            })
            .unwrap_or(0)
    } else {
        0
    };

    Ok(size_before.saturating_sub(size_after))
}

fn get_total_disk_space(path: &Path) -> Option<u64> {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        // Get the root path (e.g., "C:\")
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

            let result = GetDiskFreeSpaceExW(
                wide_path.as_ptr(),
                &mut free_bytes_available,
                &mut total_bytes,
                &mut total_free_bytes,
            );

            if result != 0 {
                return Some(total_bytes);
            }
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Use statvfs on Unix-like systems
        use std::mem::MaybeUninit;

        unsafe {
            let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
            let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes()).ok();

            if let Some(cstr) = path_cstr {
                if libc::statvfs(cstr.as_ptr(), stat.as_mut_ptr()) == 0 {
                    let stat = stat.assume_init();
                    return Some(stat.f_blocks as u64 * stat.f_frsize as u64);
                }
            }
        }
        None
    }
}

fn get_free_disk_space(path: &Path) -> Option<u64> {
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

            let result = GetDiskFreeSpaceExW(
                wide_path.as_ptr(),
                &mut free_bytes_available,
                &mut total_bytes,
                &mut total_free_bytes,
            );

            if result != 0 {
                return Some(free_bytes_available);
            }
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::mem::MaybeUninit;

        unsafe {
            let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
            let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes()).ok();

            if let Some(cstr) = path_cstr {
                if libc::statvfs(cstr.as_ptr(), stat.as_mut_ptr()) == 0 {
                    let stat = stat.assume_init();
                    return Some(stat.f_bavail as u64 * stat.f_frsize as u64);
                }
            }
        }
        None
    }
}

#[tauri::command]
pub async fn get_memory_pool_stats() -> AppResult<serde_json::Value> {
    use serde_json::json;

    // Memory pool optimization removed for production simplification
    let stats_json = json!({
        "message": "Memory pool optimization temporarily disabled for production stability",
        "pools": []
    });

    Ok(stats_json)
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
            "current_fps": stats.current_fps
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
            lcu_connected: true,
            live_client_available: false,
        });

        assert!(readiness.ready);
        assert!(readiness
            .warnings
            .iter()
            .any(|warning| warning.code == "capture_hardware_unverified"));
    }
}
