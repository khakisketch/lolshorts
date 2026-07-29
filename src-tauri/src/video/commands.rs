use crate::auth::command_policy::require_command_access;
use crate::auth::{AuthManager, SubscriptionTier};
use crate::error::{AppError, AppResult};
use crate::storage::models::ClipMetadata;
use crate::storage::{CanvasTemplateInfo, StorageError};
use crate::utils::security;
use crate::video::processor::types::{
    ChainedEffects, ClipSpec, ColorGrading, ComposeOptions, TextPosition, TextStyle,
};
use crate::video::{AutoEditConfig, AutoEditProgress, AutoEditResult, VideoProcessor};
use crate::AppState;
use std::path::PathBuf;
use std::time::Duration;
use tauri::State;

const GIF_EXPORT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Read the configured LUFS target for final-export loudness normalization,
/// or `None` if the user disabled it. Sourced from the recording settings
/// (`AudioSettings.audio_normalize` / `audio_target_lufs`).
async fn export_normalize_lufs(state: &AppState) -> Option<f64> {
    let settings = state.recording_settings.read().await;
    if settings.audio.audio_normalize {
        Some(settings.audio.audio_target_lufs)
    } else {
        None
    }
}

fn emit_export_progress(app: &tauri::AppHandle, pct: f64) {
    use tauri::Emitter;
    let _ = app.emit("export-progress", serde_json::json!({ "progress": pct }));
}

fn emit_export_complete(app: &tauri::AppHandle, output_path: &str) {
    use tauri::Emitter;
    let _ = app.emit(
        "export-complete",
        serde_json::json!({ "output_path": output_path }),
    );
}

fn emit_export_error(app: &tauri::AppHandle, message: &str) {
    use tauri::Emitter;
    let _ = app.emit("export-error", message.to_string());
}

#[tauri::command]
pub async fn get_clips(
    state: State<'_, AppState>,
    game_id: String,
) -> AppResult<Vec<ClipMetadata>> {
    require_command_access(&state.auth, "get_clips").map_err(|e| AppError::Auth(e.to_string()))?;

    // Validate game_id (prevent SQL injection)
    let validated_game_id =
        security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .storage
        .load_clip_metadata_with_duration_backfill(&validated_game_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))
}

/// Extract a clip from a video file (PRO feature)
#[tauri::command]
pub async fn extract_clip(
    state: State<'_, AppState>,
    input_path: String,
    output_path: String,
    start_time: f64,
    duration: f64,
) -> AppResult<String> {
    require_command_access(&state.auth, "extract_clip")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    // Security validation
    let validated_input = security::validate_video_input_path(&input_path)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let validated_output = security::validate_video_output_path(&output_path)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let validated_start_time = security::validate_time_offset(start_time)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let validated_duration =
        security::validate_duration(duration).map_err(|e| AppError::Validation(e.to_string()))?;

    let processor = VideoProcessor::new_with_fallback();

    let result_path = processor
        .extract_clip(
            validated_input,
            validated_output,
            validated_start_time,
            validated_duration,
        )
        .await
        .map_err(|e| AppError::Video(e.to_string()))?;

    Ok(result_path.to_string_lossy().to_string())
}

/// Compose multiple clips into a YouTube Short (9:16 aspect ratio) (PRO feature)
#[tauri::command]
pub async fn compose_shorts(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    clip_paths: Vec<String>,
    output_path: String,
) -> AppResult<String> {
    require_command_access(&state.auth, "compose_shorts")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    emit_export_progress(&app, 5.0);

    // Security validation
    let validated_clips: Result<Vec<PathBuf>, AppError> = clip_paths
        .iter()
        .map(|p| {
            security::validate_video_input_path(p).map_err(|e| AppError::Validation(e.to_string()))
        })
        .collect();
    let validated_clips = validated_clips?;

    let validated_output = security::validate_video_output_path(&output_path)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let processor = VideoProcessor::new_with_fallback();

    emit_export_progress(&app, 30.0);

    // Standard YouTube Shorts resolution: 1080x1920 (9:16)
    let result_path = match processor
        .compose_shorts(&validated_clips, validated_output, 1080, 1920)
        .await
    {
        Ok(path) => path,
        Err(e) => {
            let msg = e.to_string();
            emit_export_error(&app, &msg);
            return Err(AppError::Video(msg));
        }
    };

    let output_str = result_path.to_string_lossy().to_string();
    emit_export_complete(&app, &output_str);
    Ok(output_str)
}

/// Generate a thumbnail from a video file (PRO feature)
#[tauri::command]
pub async fn generate_thumbnail(
    state: State<'_, AppState>,
    input_path: String,
    output_path: String,
    time_offset: f64,
) -> AppResult<String> {
    require_command_access(&state.auth, "generate_thumbnail")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    // Security validation
    let validated_input = security::validate_video_input_path(&input_path)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let validated_output = security::validate_thumbnail_path(&output_path)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let validated_time_offset = security::validate_time_offset(time_offset)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let processor = VideoProcessor::new_with_fallback();

    let result_path = processor
        .generate_thumbnail(validated_input, validated_output, validated_time_offset)
        .await
        .map_err(|e| AppError::Video(e.to_string()))?;

    Ok(result_path.to_string_lossy().to_string())
}

/// Generate a basic thumbnail for clip preview (available to all authenticated users)
#[tauri::command]
pub async fn generate_clip_thumbnail(
    state: State<'_, AppState>,
    clip_file_path: String,
) -> AppResult<String> {
    require_command_access(&state.auth, "generate_clip_thumbnail")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    // Security validation
    let validated_input = security::validate_video_input_path(&clip_file_path)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // Generate thumbnail path in same directory as clip
    let clip_path = std::path::Path::new(&clip_file_path);
    let clip_name = clip_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("clip");

    let thumbnail_name = format!("{}_preview.jpg", clip_name);
    let thumbnail_path = clip_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(thumbnail_name);

    // Validate thumbnail path
    let _validated_output = security::validate_thumbnail_path(&thumbnail_path.to_string_lossy())
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // Use thumbnail helper function to generate at midpoint
    let result_path = crate::video::thumbnail::auto_generate_thumbnail(
        validated_input,
        thumbnail_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(".")),
    )
    .await
    .map_err(|e| AppError::Video(e.to_string()))?;

    Ok(result_path.to_string_lossy().to_string())
}

/// Get video duration in seconds
#[tauri::command]
pub async fn get_video_duration(state: State<'_, AppState>, input_path: String) -> AppResult<f64> {
    require_command_access(&state.auth, "get_video_duration")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    // Security validation
    let validated_input = security::validate_video_input_path(&input_path)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let processor = VideoProcessor::new_with_fallback();

    let duration = processor
        .get_duration(validated_input)
        .await
        .map_err(|e| AppError::Video(e.to_string()))?;

    Ok(duration)
}

/// Delete a clip from storage
#[tauri::command]
pub async fn delete_clip(
    state: State<'_, AppState>,
    clip_file_path: String,
    game_id: String,
) -> AppResult<()> {
    require_command_access(&state.auth, "delete_clip")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    // Security validation
    let validated_game_id =
        security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;

    // Delete the video file only if it resolves inside app-owned media roots.
    let deleted_path = match state.storage.safe_delete_media_file(&clip_file_path) {
        Ok(security::SafeDeleteOutcome::Deleted(path)) => {
            tracing::info!("Deleted clip file: {:?}", path);
            path
        }
        Ok(security::SafeDeleteOutcome::Missing(path)) => {
            tracing::warn!("Clip file already missing: {:?}", path);
            path
        }
        Err(StorageError::Security(err)) => {
            tracing::warn!(
                "Rejected unsafe clip deletion path {:?}: {}",
                clip_file_path,
                err
            );
            return Err(AppError::Validation(format!(
                "Unsafe clip deletion path: {}",
                err
            )));
        }
        Err(err) => return Err(AppError::Io(err.to_string())),
    };

    // Delete from JSON storage
    state
        .storage
        .delete_clip_metadata(&validated_game_id, &clip_file_path)
        .map_err(|e| AppError::Database(format!("Failed to delete clip metadata: {}", e)))?;

    tracing::info!("Successfully deleted clip and metadata: {:?}", deleted_path);
    Ok(())
}

/// Create a long-form montage video (16:9) from selected clips
#[tauri::command]
pub async fn create_longform_video(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    clip_paths: Vec<String>,
    output_path: String,
) -> AppResult<String> {
    // Montage export is a paid (PRO) feature: it produces an un-watermarked
    // multi-clip composite, so it is gated identically to compose_shorts.
    require_command_access(&state.auth, "create_longform_video")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    emit_export_progress(&app, 5.0);

    let validated_clips: Result<Vec<PathBuf>, AppError> = clip_paths
        .iter()
        .map(|p| {
            security::validate_video_input_path(p).map_err(|e| AppError::Validation(e.to_string()))
        })
        .collect();
    let validated_clips = validated_clips?;

    let validated_output = security::validate_video_output_path(&output_path)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // Use the optimized VideoProcessor
    // Note: compose_montage is newly added to Processor
    let processor = VideoProcessor::new_with_fallback();

    emit_export_progress(&app, 30.0);

    let result_path = match processor
        .compose_montage(&validated_clips, validated_output, false)
        .await
    {
        Ok(path) => path,
        Err(e) => {
            let msg = e.to_string();
            emit_export_error(&app, &msg);
            return Err(AppError::Video(msg));
        }
    };

    let output_str = result_path.to_string_lossy().to_string();
    emit_export_complete(&app, &output_str);
    Ok(output_str)
}

/// A single clip from the editor timeline for `compose_shorts_v2`.
///
/// `trim_end` is the number of seconds to cut from the **end** of the clip
/// (backend converts it to an absolute duration via ffprobe).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ClipSpecInput {
    pub path: String,
    #[serde(default)]
    pub trim_start: Option<f64>,
    #[serde(default)]
    pub trim_end: Option<f64>,
}

/// Compose editor clips into a Short honoring per-clip trim, aspect ratio, and
/// transitions in a single re-encode pass (PRO feature). Backward-compatible
/// `compose_shorts` remains for the legacy path.
#[tauri::command]
pub async fn compose_shorts_v2(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    clips: Vec<ClipSpecInput>,
    aspect_ratio: String,
    transition_type: String,
    transition_duration: f64,
    output_path: String,
) -> Result<String, String> {
    require_command_access(&state.auth, "compose_shorts_v2")
        .map_err(|e| AppError::Auth(e.to_string()).to_string())?;

    emit_export_progress(&app, 5.0);

    if clips.is_empty() {
        let msg = "No clips provided".to_string();
        emit_export_error(&app, &msg);
        return Err(msg);
    }

    // Aspect ratio -> output resolution.
    let (width, height) = match aspect_ratio.as_str() {
        "16:9" => (1920u32, 1080u32),
        "1:1" => (1080u32, 1080u32),
        _ => (1080u32, 1920u32), // default 9:16
    };

    // Transition kind. "none" -> hard cut.
    let transition = match transition_type.as_str() {
        "none" | "" => None,
        "slide" => Some(("slide".to_string(), transition_duration.max(0.1))),
        _ => Some(("fade".to_string(), transition_duration.max(0.1))),
    };

    let processor = VideoProcessor::new_with_fallback();

    // Build ClipSpecs, converting `trim_end` (cut-from-end) -> absolute duration.
    let mut specs: Vec<ClipSpec> = Vec::with_capacity(clips.len());
    for clip in &clips {
        let path = security::validate_video_input_path(&clip.path).map_err(|e| {
            let msg = format!("Invalid clip path: {}", e);
            emit_export_error(&app, &msg);
            msg
        })?;

        let trim_start = clip.trim_start.filter(|s| *s > 0.0);
        let trim_duration = if let Some(end_cut) = clip.trim_end.filter(|e| *e > 0.0) {
            let full = processor.get_duration(&path).await.map_err(|e| {
                let msg = format!("Failed to probe clip duration: {}", e);
                emit_export_error(&app, &msg);
                msg
            })?;
            let dur = full - trim_start.unwrap_or(0.0) - end_cut;
            if dur <= 0.0 {
                let msg = format!(
                    "Trim removes the entire clip {}: full={:.2}s start={:.2}s end_cut={:.2}s",
                    clip.path,
                    full,
                    trim_start.unwrap_or(0.0),
                    end_cut
                );
                emit_export_error(&app, &msg);
                return Err(msg);
            }
            Some(dur)
        } else {
            None
        };

        specs.push(ClipSpec {
            path,
            trim_start,
            trim_duration,
        });
    }

    let validated_output = security::validate_video_output_path(&output_path).map_err(|e| {
        let msg = format!("Invalid output path: {}", e);
        emit_export_error(&app, &msg);
        msg
    })?;

    emit_export_progress(&app, 30.0);

    let opts = ComposeOptions {
        width,
        height,
        transition,
        event_times: None,
        fps: Some(60),
        normalize_audio: export_normalize_lufs(&state).await,
    };

    emit_export_progress(&app, 45.0);

    if let Err(e) = processor
        .compose_with_options(&specs, &validated_output, &opts)
        .await
    {
        let msg = e.to_string();
        emit_export_error(&app, &msg);
        return Err(msg);
    }

    let output_str = validated_output.to_string_lossy().to_string();
    emit_export_complete(&app, &output_str);
    Ok(output_str)
}

/// Start auto-edit composition for YouTube Shorts
#[tauri::command]
pub async fn start_auto_edit(
    state: State<'_, AppState>,
    config: AutoEditConfig,
) -> AppResult<AutoEditResult> {
    // Auth-gated here; the FREE monthly quota vs PRO-unlimited split is enforced below.
    let policy_user = require_command_access(&state.auth, "start_auto_edit")
        .map_err(|e| AppError::Auth(e.to_string()))?;
    // Quota is scoped per-user (see storage::Storage auto_edit_usage_by_user doc);
    // "anonymous" is a defensive fallback and should be unreachable since this
    // command is AuthRequired, but avoids a hard failure if that ever changes.
    let user_id = policy_user
        .map(|u| u.id)
        .unwrap_or_else(|| "anonymous".to_string());

    // Check tier and quota
    let tier = state
        .auth
        .get_tier()
        .map_err(|e| AppError::Auth(e.to_string()))?;
    let is_pro = matches!(tier, SubscriptionTier::Pro);

    // Quota gate (FREE only; PRO is unlimited on both server and local).
    //
    // Authority is the server-side `quota` edge function; the local SQLite
    // counter is only a cache / offline fallback. POLICY: a server outage
    // (offline/timeout) must NOT block a legitimate user, so when the server is
    // unreachable we fall back to the local counter rather than failing closed.
    if !is_pro {
        let server = server_quota_check(&state.auth).await;
        if matches!(server, ServerQuotaVerdict::Unavailable) {
            tracing::warn!(
                "Server quota check unavailable for user {}; falling back to local counter",
                user_id
            );
        }

        // `resolve_quota_gate` only consults the local counter when the server
        // is Unavailable (lazy closure), keeping the offline-fallback policy in
        // one testable place.
        resolve_quota_gate(&server, || {
            state
                .storage
                .check_auto_edit_quota(&user_id, is_pro)
                .map_err(|e| e.to_string())
        })
        .map_err(|msg| AppError::Validation(format!("Quota check failed: {}", msg)))?;

        tracing::info!(
            "Auto-edit quota gate passed: tier={:?}, authority={}",
            tier,
            match server {
                ServerQuotaVerdict::Allowed | ServerQuotaVerdict::Denied { .. } => "server",
                ServerQuotaVerdict::Unavailable => "local",
            }
        );
    } else {
        tracing::info!("Auto-edit quota check skipped: tier={:?} (unlimited)", tier);
    }

    // Generate unique job ID
    let job_id = format!("auto_edit_{}", chrono::Local::now().format("%Y%m%d_%H%M%S"));

    tracing::info!(
        "Starting auto-edit job: {} with target duration: {}s",
        job_id,
        config.target_duration
    );

    // Start auto-composition
    let result = state
        .auto_composer
        .compose(config.clone(), job_id.clone(), is_pro)
        .await
        .map_err(|e| {
            tracing::error!("Auto-edit failed for job {}: {}", job_id, e);
            AppError::Video(format!("Auto-edit failed: {}", e))
        })?;

    // 결과 메타데이터(썸네일 포함)는 AutoComposer::compose가 이미 저장한다 —
    // 여기서 다시 저장하면 thumbnail_path가 None으로 덮여 사라지므로 중복 저장 금지.

    // Increment usage counter on success (FREE only).
    if !is_pro {
        // Authoritative consume on the server. Best effort: the compose already
        // succeeded, so a server outage must not fail the user's export -- the
        // local cache counter below still advances to keep the offline fallback
        // warm and approximately correct.
        server_quota_consume(&state.auth).await;

        state
            .storage
            .increment_auto_edit_usage(&user_id)
            .map_err(|e| {
                tracing::error!("Failed to increment usage: {}", e);
                // Just log warning, don't fail operation
            })
            .ok();
    }

    tracing::info!(
        "Auto-edit completed successfully and metadata saved: {:?}",
        result.output_path
    );
    Ok(result)
}

/// Verdict from the authoritative server-side auto-edit quota.
#[derive(Debug, PartialEq, Eq)]
enum ServerQuotaVerdict {
    /// Server permits another auto-edit this month.
    Allowed,
    /// Server rejects: the FREE monthly quota is exhausted.
    Denied { used: u32, limit: u32 },
    /// Server could not be consulted (no session, offline, timeout, or a
    /// malformed response). The caller must fall back to the local counter.
    Unavailable,
}

/// Map a successful `quota` edge-function JSON body to a [`ServerQuotaVerdict`].
///
/// A body without a boolean `allowed` field is treated as `Unavailable` so the
/// caller fails open to the local fallback rather than mis-reading an error
/// envelope as a denial.
fn parse_quota_verdict(value: &serde_json::Value) -> ServerQuotaVerdict {
    let Some(allowed) = value.get("allowed").and_then(|v| v.as_bool()) else {
        return ServerQuotaVerdict::Unavailable;
    };
    if allowed {
        ServerQuotaVerdict::Allowed
    } else {
        let used = value.get("used").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let limit = value.get("limit").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        ServerQuotaVerdict::Denied { used, limit }
    }
}

/// Decide the FREE-tier auto-edit gate from the server verdict, consulting the
/// local counter (`local`) **only** when the server is `Unavailable`.
///
/// This is the single source of the offline-fallback policy: server is
/// authoritative when reachable; otherwise the local cache decides so an
/// offline user is not blocked. `local` is a closure so the local counter is
/// touched only on the fallback path.
fn resolve_quota_gate(
    server: &ServerQuotaVerdict,
    local: impl FnOnce() -> std::result::Result<u32, String>,
) -> std::result::Result<(), String> {
    match server {
        ServerQuotaVerdict::Allowed => Ok(()),
        ServerQuotaVerdict::Denied { used, limit } => Err(format!(
            "Monthly auto-edit quota exceeded ({}/{}). Upgrade to PRO for unlimited usage.",
            used, limit
        )),
        ServerQuotaVerdict::Unavailable => local().map(|_| ()),
    }
}

/// Invoke the `quota` edge function for the given `action` using the current
/// session's access token. Returns `None` (treated as server-unavailable) when
/// there is no authenticated session/token, no Supabase client, or the call
/// fails for any reason (offline, timeout, non-2xx).
async fn call_quota_action(auth: &AuthManager, action: &str) -> Option<serde_json::Value> {
    let user = auth.get_current_user().ok().flatten()?;
    if user.access_token.is_empty() {
        return None;
    }
    let client = auth.get_supabase_client().ok()?;
    match client
        .call_edge_function(
            "quota",
            &user.access_token,
            &serde_json::json!({ "action": action }),
        )
        .await
    {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!("quota edge function '{}' call failed: {}", action, e);
            None
        }
    }
}

/// Consult the authoritative server quota before composing.
async fn server_quota_check(auth: &AuthManager) -> ServerQuotaVerdict {
    match call_quota_action(auth, "check").await {
        Some(value) => parse_quota_verdict(&value),
        None => ServerQuotaVerdict::Unavailable,
    }
}

/// Record one authoritative consume after a successful compose. Best effort:
/// any failure is logged and swallowed because the export already succeeded.
async fn server_quota_consume(auth: &AuthManager) {
    match call_quota_action(auth, "consume").await {
        Some(value) => match parse_quota_verdict(&value) {
            ServerQuotaVerdict::Allowed => {
                tracing::info!("Server auto-edit quota consume recorded");
            }
            ServerQuotaVerdict::Denied { used, limit } => {
                tracing::warn!(
                    "Server quota consume reported over-limit ({}/{}) after a successful compose",
                    used,
                    limit
                );
            }
            ServerQuotaVerdict::Unavailable => {}
        },
        None => {
            tracing::warn!(
                "Server auto-edit quota consume unavailable; relying on the local counter cache"
            );
        }
    }
}

/// Get progress of an auto-edit job
#[tauri::command]
pub async fn get_auto_edit_progress(
    state: State<'_, AppState>,
) -> AppResult<Option<AutoEditProgress>> {
    require_command_access(&state.auth, "get_auto_edit_progress")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let progress = state.auto_composer.get_progress().await;
    Ok(progress)
}

// ========================================================================
// Canvas Template Management
// ========================================================================

/// Save a canvas template to the library for reuse
#[tauri::command]
pub async fn save_canvas_template(
    state: State<'_, AppState>,
    template: crate::video::CanvasTemplate,
) -> AppResult<()> {
    require_command_access(&state.auth, "save_canvas_template")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    state
        .storage
        .save_canvas_template(&template)
        .map_err(|e| AppError::Database(format!("Failed to save canvas template: {}", e)))?;

    Ok(())
}

/// Load a canvas template by ID
#[tauri::command]
pub async fn load_canvas_template(
    state: State<'_, AppState>,
    template_id: String,
) -> AppResult<crate::video::CanvasTemplate> {
    require_command_access(&state.auth, "load_canvas_template")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    // Security validation
    let validated_template_id = security::validate_template_id(&template_id)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let template = state
        .storage
        .load_canvas_template(&validated_template_id)
        .map_err(|e| AppError::Database(format!("Failed to load canvas template: {}", e)))?;

    Ok(template)
}

/// List all available canvas templates
#[tauri::command]
pub async fn list_canvas_templates(
    state: State<'_, AppState>,
) -> AppResult<Vec<CanvasTemplateInfo>> {
    require_command_access(&state.auth, "list_canvas_templates")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let templates = state
        .storage
        .list_canvas_templates()
        .map_err(|e| AppError::Database(format!("Failed to list canvas templates: {}", e)))?;

    Ok(templates)
}

/// Delete a canvas template
#[tauri::command]
pub async fn delete_canvas_template(
    state: State<'_, AppState>,
    template_id: String,
) -> AppResult<()> {
    require_command_access(&state.auth, "delete_canvas_template")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    // Security validation
    let validated_template_id = security::validate_template_id(&template_id)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .storage
        .delete_canvas_template(&validated_template_id)
        .map_err(|e| AppError::Database(format!("Failed to delete canvas template: {}", e)))?;

    Ok(())
}

// ============================================================================
// Statistics Commands
// ============================================================================

/// Get simple clip statistics
#[tauri::command]
pub async fn get_clip_statistics(state: State<'_, AppState>) -> AppResult<(u64, u64, u64, f64)> {
    require_command_access(&state.auth, "get_clip_statistics")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let stats = crate::video::get_global_stats();
    let (total, successful, failed) = stats.get_counts();
    let success_rate = stats.success_rate();

    Ok((total, successful, failed, success_rate))
}

/// Reset all statistics
#[tauri::command]
pub async fn reset_clip_statistics(state: State<'_, AppState>) -> AppResult<()> {
    require_command_access(&state.auth, "reset_clip_statistics")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    crate::video::get_global_stats().reset();
    Ok(())
}

// ============================================================================
// Export Commands
// ============================================================================

/// Export a video with custom format and resolution
///
/// Supports mp4 (H.264), webm (VP9), and mov (ProRes) output formats.
#[tauri::command]
pub async fn export_video(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    input: String,
    output: String,
    format: String,
    width: u32,
    height: u32,
) -> Result<String, String> {
    use tokio::process::Command as TokioCommand;

    require_command_access(&state.auth, "export_video")
        .map_err(|e| AppError::Auth(e.to_string()).to_string())?;

    emit_export_progress(&app, 5.0);

    // Validate input path
    let validated_input = security::validate_video_input_path(&input)
        .map_err(|e| format!("Invalid input path: {}", e))?;

    if !validated_input.exists() {
        return Err("Input video file not found".to_string());
    }

    // Validate output path
    let validated_output = security::validate_video_output_path(&output)
        .map_err(|e| format!("Invalid output path: {}", e))?;

    // Validate dimensions
    if width == 0 || height == 0 || width > 7680 || height > 7680 {
        return Err("Invalid resolution. Width and height must be between 1 and 7680.".to_string());
    }

    // Map format to FFmpeg codec and container arguments
    let (codec_args, output_path) = match format.as_str() {
        "webm" => {
            let out = if validated_output.extension().and_then(|e| e.to_str()) != Some("webm") {
                validated_output.with_extension("webm")
            } else {
                validated_output
            };
            (
                vec![
                    "-c:v",
                    "libvpx-vp9",
                    "-crf",
                    "30",
                    "-b:v",
                    "0",
                    "-c:a",
                    "libopus",
                ],
                out,
            )
        }
        "mov" => {
            let out = if validated_output.extension().and_then(|e| e.to_str()) != Some("mov") {
                validated_output.with_extension("mov")
            } else {
                validated_output
            };
            (
                vec!["-c:v", "prores_ks", "-profile:v", "1", "-c:a", "pcm_s16le"],
                out,
            )
        }
        _ => {
            // Default to MP4 with detected hardware encoder (fallback: libx264)
            let out = if validated_output.extension().and_then(|e| e.to_str()) != Some("mp4") {
                validated_output.with_extension("mp4")
            } else {
                validated_output
            };
            // Quality lives in the encoder args (libx264 -crf / nvenc -cq /
            // amf -qp / qsv -global_quality); appending a blanket -crf here
            // would hard-fail hardware encoders. Add the compatibility pixel
            // format and a 1080p bitrate ceiling instead.
            let encoder = VideoProcessor::detect_optimal_encoder();
            let mut args: Vec<&str> = encoder.get_ffmpeg_args().to_vec();
            args.extend_from_slice(&[
                "-pix_fmt", "yuv420p", "-maxrate", "16M", "-bufsize", "32M", "-c:a", "aac", "-b:a",
                "192k",
            ]);
            (args, out)
        }
    };

    // Build FFmpeg command
    let ffmpeg_path =
        crate::utils::ffmpeg::get_ffmpeg_path().map_err(|e| format!("FFmpeg not found: {}", e))?;

    let scale_filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
        width, height, width, height
    );

    // Bound concurrent offline FFmpeg processes (hardware encoders limit sessions).
    let _pool_permit = crate::utils::ffmpeg_pool::global_ffmpeg_pool()
        .acquire()
        .await
        .map_err(|e| {
            let msg = format!("FFmpeg pool acquire failed: {}", e);
            emit_export_error(&app, &msg);
            msg
        })?;

    let mut command = TokioCommand::new(&ffmpeg_path);
    command.args(["-y", "-i"]);
    command.arg(validated_input.as_os_str());
    command.args(["-vf", &scale_filter]);

    for arg in &codec_args {
        command.arg(arg);
    }

    command.arg(output_path.as_os_str());
    command.stdout(std::process::Stdio::null());
    command.stderr(std::process::Stdio::piped());
    command.kill_on_drop(true);

    tracing::info!(
        "Exporting video: format={}, resolution={}x{}, output={:?}",
        format,
        width,
        height,
        output_path
    );

    emit_export_progress(&app, 40.0);

    let child = command.spawn().map_err(|e| {
        let msg = format!("Failed to start FFmpeg: {}", e);
        emit_export_error(&app, &msg);
        msg
    })?;
    let result = child.wait_with_output().await.map_err(|e| {
        let msg = format!("FFmpeg process error: {}", e);
        emit_export_error(&app, &msg);
        msg
    })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        tracing::error!("Export failed: {}", stderr);
        let msg = format!(
            "Export failed: {}",
            stderr.chars().take(500).collect::<String>()
        );
        emit_export_error(&app, &msg);
        return Err(msg);
    }

    // Release the pool slot before the normalization pass, which acquires its
    // own slot via the shared execute path (avoids a self-deadlock).
    drop(_pool_permit);

    emit_export_progress(&app, 85.0);

    // Optional final LUFS normalization (2-pass loudnorm; copies video so no
    // re-encode). Only for the H.264/MP4 path — loudnorm re-muxes to AAC/MP4,
    // which would be wrong for VP9/webm or ProRes/mov.
    let is_mp4 = !matches!(format.as_str(), "webm" | "mov");
    if is_mp4 {
        if let Some(target_lufs) = export_normalize_lufs(&state).await {
            let processor = VideoProcessor::new_with_fallback();
            let normalized = output_path.with_extension("loudnorm.mp4");
            match processor
                .normalize_audio(&output_path, &normalized, target_lufs)
                .await
            {
                Ok(_) => {
                    if let Err(e) = std::fs::rename(&normalized, &output_path) {
                        tracing::warn!("Failed to swap in normalized audio: {}", e);
                        let _ = std::fs::remove_file(&normalized);
                    }
                }
                Err(e) => {
                    tracing::warn!("Audio normalization skipped (non-fatal): {}", e);
                    let _ = std::fs::remove_file(&normalized);
                }
            }
        }
    }

    let output_str = output_path.to_string_lossy().to_string();
    tracing::info!("Video export completed: {:?}", output_path);
    emit_export_complete(&app, &output_str);
    Ok(output_str)
}

// ============================================================================
// Video Effects Commands (Task 20)
// ============================================================================

/// Apply slow-motion effect to a video (speed_factor must be < 1.0)
#[tauri::command]
pub async fn apply_slow_motion_cmd(
    state: State<'_, AppState>,
    input: String,
    output: String,
    speed_factor: f64,
) -> Result<String, String> {
    require_command_access(&state.auth, "apply_slow_motion_cmd").map_err(|e| e.to_string())?;

    if speed_factor >= 1.0 {
        return Err("Speed factor must be less than 1.0 for slow motion".to_string());
    }
    if speed_factor <= 0.0 {
        return Err("Speed factor must be greater than 0.0".to_string());
    }

    let validated_input = security::validate_video_input_path(&input)
        .map_err(|e| format!("Invalid input path: {}", e))?;
    let validated_output = security::validate_video_output_path(&output)
        .map_err(|e| format!("Invalid output path: {}", e))?;

    let processor = VideoProcessor::new_with_fallback();
    let result_path = processor
        .apply_slow_motion(&validated_input, &validated_output, speed_factor)
        .await
        .map_err(|e| e.to_string())?;

    Ok(result_path.to_string_lossy().to_string())
}

/// Apply color grading to a video
#[tauri::command]
pub async fn apply_color_grading_cmd(
    state: State<'_, AppState>,
    input: String,
    output: String,
    brightness: f64,
    contrast: f64,
    saturation: f64,
) -> Result<String, String> {
    require_command_access(&state.auth, "apply_color_grading_cmd").map_err(|e| e.to_string())?;

    // Validate color grading ranges
    if !brightness.is_finite() || !(-1.0..=1.0).contains(&brightness) {
        return Err(format!("brightness {} out of range -1.0..1.0", brightness));
    }
    if !contrast.is_finite() || !(0.0..=3.0).contains(&contrast) {
        return Err(format!("contrast {} out of range 0.0..3.0", contrast));
    }
    if !saturation.is_finite() || !(0.0..=3.0).contains(&saturation) {
        return Err(format!("saturation {} out of range 0.0..3.0", saturation));
    }

    let validated_input = security::validate_video_input_path(&input)
        .map_err(|e| format!("Invalid input path: {}", e))?;
    let validated_output = security::validate_video_output_path(&output)
        .map_err(|e| format!("Invalid output path: {}", e))?;

    let grading = ColorGrading {
        brightness,
        contrast,
        saturation,
        ..Default::default()
    };

    let processor = VideoProcessor::new_with_fallback();
    let result_path = processor
        .apply_color_grading(&validated_input, &validated_output, grading)
        .await
        .map_err(|e| e.to_string())?;

    Ok(result_path.to_string_lossy().to_string())
}

/// Apply text overlay to a video
#[tauri::command]
pub async fn apply_text_overlay_cmd(
    state: State<'_, AppState>,
    input: String,
    output: String,
    text: String,
    position: String,
    size: u32,
    color: String,
) -> Result<String, String> {
    require_command_access(&state.auth, "apply_text_overlay_cmd").map_err(|e| e.to_string())?;

    let validated_input = security::validate_video_input_path(&input)
        .map_err(|e| format!("Invalid input path: {}", e))?;
    let validated_output = security::validate_video_output_path(&output)
        .map_err(|e| format!("Invalid output path: {}", e))?;

    let text_position = match position.to_lowercase().as_str() {
        "topleft" | "top_left" => TextPosition::TopLeft,
        "topright" | "top_right" => TextPosition::TopRight,
        "bottomleft" | "bottom_left" => TextPosition::BottomLeft,
        "bottomright" | "bottom_right" => TextPosition::BottomRight,
        "center" => TextPosition::Center,
        _ => {
            return Err(format!(
            "Invalid text position: {}. Use: topleft, topright, bottomleft, bottomright, center",
            position
        ))
        }
    };

    let style = TextStyle { size, color };

    let processor = VideoProcessor::new_with_fallback();
    let result_path = processor
        .add_text_overlay(
            &validated_input,
            &validated_output,
            &text,
            text_position,
            style,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(result_path.to_string_lossy().to_string())
}

/// Apply multiple effects in a single FFmpeg pass (chained effects)
#[tauri::command]
pub async fn apply_chained_effects_cmd(
    state: State<'_, AppState>,
    input: String,
    output: String,
    effects: ChainedEffects,
) -> Result<String, String> {
    require_command_access(&state.auth, "apply_chained_effects_cmd").map_err(|e| e.to_string())?;

    let validated_input = security::validate_video_input_path(&input)
        .map_err(|e| format!("Invalid input path: {}", e))?;
    let validated_output = security::validate_video_output_path(&output)
        .map_err(|e| format!("Invalid output path: {}", e))?;

    // Validate slow_motion range if provided
    if let Some(speed) = effects.slow_motion {
        if speed <= 0.0 || speed >= 1.0 {
            return Err(
                "Speed factor must be between 0.0 (exclusive) and 1.0 (exclusive) for slow motion"
                    .to_string(),
            );
        }
    }

    // Validate color grading ranges if provided
    if let Some(ref grading) = effects.color_grading {
        if !grading.brightness.is_finite() || grading.brightness < -1.0 || grading.brightness > 1.0
        {
            return Err(format!(
                "brightness {} out of range -1.0..1.0",
                grading.brightness
            ));
        }
        if !grading.contrast.is_finite() || grading.contrast < 0.0 || grading.contrast > 3.0 {
            return Err(format!(
                "contrast {} out of range 0.0..3.0",
                grading.contrast
            ));
        }
        if !grading.saturation.is_finite() || grading.saturation < 0.0 || grading.saturation > 3.0 {
            return Err(format!(
                "saturation {} out of range 0.0..3.0",
                grading.saturation
            ));
        }
    }

    let processor = VideoProcessor::new_with_fallback();
    let result_path = processor
        .apply_chained_effects(&validated_input, &validated_output, effects)
        .await
        .map_err(|e| e.to_string())?;

    Ok(result_path.to_string_lossy().to_string())
}

// ============================================================================
// GIF Export Command (Task 21)
// ============================================================================

/// Export a video as an animated GIF (max 15 seconds)
#[tauri::command]
pub async fn export_as_gif(
    state: State<'_, AppState>,
    input: String,
    output: String,
    max_duration: f64,
) -> Result<String, String> {
    require_command_access(&state.auth, "export_as_gif").map_err(|e| e.to_string())?;

    let validated_input = security::validate_video_input_path(&input)
        .map_err(|e| format!("Invalid input path: {}", e))?;

    if !validated_input.exists() {
        return Err("Input video file not found".to_string());
    }

    // Ensure output has .gif extension
    let output_path = if output.ends_with(".gif") {
        PathBuf::from(&output)
    } else {
        PathBuf::from(&output).with_extension("gif")
    };

    // Clamp duration to 15 seconds max
    let duration = if max_duration > 15.0 {
        15.0
    } else if max_duration <= 0.0 {
        5.0
    } else {
        max_duration
    };

    let ffmpeg_path =
        crate::utils::ffmpeg::get_ffmpeg_path().map_err(|e| format!("FFmpeg not found: {}", e))?;

    let mut command = tokio::process::Command::new(&ffmpeg_path);
    command
        .args([
            "-i",
            validated_input.to_str().unwrap_or_default(),
            "-vf",
            "fps=15,scale=480:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
            "-t",
            &duration.to_string(),
            "-y",
            output_path.to_str().unwrap_or_default(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    command.kill_on_drop(true);

    let status = tokio::time::timeout(GIF_EXPORT_TIMEOUT, command.status())
        .await
        .map_err(|_| {
            format!(
                "GIF export timed out after {} seconds",
                GIF_EXPORT_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;

    if status.success() {
        tracing::info!("GIF export completed: {:?}", output_path);
        Ok(output_path.to_string_lossy().to_string())
    } else {
        Err("GIF export failed. The input video may be corrupted or unsupported.".to_string())
    }
}

#[cfg(test)]
mod quota_gate_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn parse_verdict_allowed() {
        let value = serde_json::json!({ "allowed": true, "used": 0, "limit": 5 });
        assert_eq!(parse_quota_verdict(&value), ServerQuotaVerdict::Allowed);
    }

    #[test]
    fn parse_verdict_denied_reads_used_and_limit() {
        let value = serde_json::json!({ "allowed": false, "used": 5, "limit": 5 });
        assert_eq!(
            parse_quota_verdict(&value),
            ServerQuotaVerdict::Denied { used: 5, limit: 5 }
        );
    }

    #[test]
    fn parse_verdict_denied_defaults_missing_counts_to_zero() {
        let value = serde_json::json!({ "allowed": false });
        assert_eq!(
            parse_quota_verdict(&value),
            ServerQuotaVerdict::Denied { used: 0, limit: 0 }
        );
    }

    #[test]
    fn parse_verdict_pro_unlimited_body_is_allowed() {
        // PRO responses carry `limit: null`; `allowed` still governs.
        let value = serde_json::json!({ "allowed": true, "limit": null, "unlimited": true });
        assert_eq!(parse_quota_verdict(&value), ServerQuotaVerdict::Allowed);
    }

    #[test]
    fn parse_verdict_malformed_body_is_unavailable() {
        // An error envelope (no boolean `allowed`) must fail open to local.
        let value = serde_json::json!({ "error": "AUTH_INVALID", "message": "nope" });
        assert_eq!(parse_quota_verdict(&value), ServerQuotaVerdict::Unavailable);
    }

    #[test]
    fn gate_allowed_does_not_touch_local() {
        let called = Cell::new(false);
        let result = resolve_quota_gate(&ServerQuotaVerdict::Allowed, || {
            called.set(true);
            Ok(3)
        });
        assert!(result.is_ok());
        assert!(
            !called.get(),
            "server Allowed must not consult local counter"
        );
    }

    #[test]
    fn gate_denied_errors_without_touching_local() {
        let called = Cell::new(false);
        let result = resolve_quota_gate(&ServerQuotaVerdict::Denied { used: 5, limit: 5 }, || {
            called.set(true);
            Ok(3)
        });
        let err = result.unwrap_err();
        assert!(err.contains("quota exceeded"), "got: {}", err);
        assert!(err.contains("5/5"), "got: {}", err);
        assert!(
            !called.get(),
            "server Denied must not consult local counter"
        );
    }

    #[test]
    fn gate_unavailable_falls_back_to_local_allow() {
        // Server offline but the local counter has room => allow.
        let called = Cell::new(false);
        let result = resolve_quota_gate(&ServerQuotaVerdict::Unavailable, || {
            called.set(true);
            Ok(2)
        });
        assert!(result.is_ok());
        assert!(
            called.get(),
            "server Unavailable must consult local counter"
        );
    }

    #[test]
    fn gate_unavailable_falls_back_to_local_deny() {
        // Server offline and the local counter is exhausted => deny with the
        // local error surfaced verbatim.
        let result = resolve_quota_gate(&ServerQuotaVerdict::Unavailable, || {
            Err(
                "Monthly auto-edit quota exceeded (5/5). Upgrade to PRO for unlimited usage."
                    .to_string(),
            )
        });
        let err = result.unwrap_err();
        assert!(err.contains("quota exceeded"), "got: {}", err);
    }
}
