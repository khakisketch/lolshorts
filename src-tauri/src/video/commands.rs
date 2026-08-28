use crate::auth::command_policy::require_command_access;
use crate::auth::{AuthManager, SubscriptionTier};
use crate::error::{AppError, AppResult};
use crate::storage::models::ClipMetadata;
use crate::storage::{
    CanvasTemplateInfo, MediaJobKind, MediaJobSnapshot, MediaJobStatus, PlatformExportMetadata,
    StorageError,
};
use crate::utils::security;
use crate::video::auto_composer::{
    AutoEditJobReceipt, AutoEditOutput, AutoEditOutputIntent, AutoEditOutputKind, AutoEditPlan,
    AutoEditStatus, StoryboardClip,
};
use crate::video::processor::types::{
    ChainedEffects, ClipSpec, ColorGrading, ComposeOptions, TextPosition, TextStyle,
    VerticalFraming,
};
use crate::video::{AutoEditConfig, AutoEditProgress, VideoProcessor};
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
    // The free public edition requires an account for editing/export, but does
    // not require a paid entitlement. Keep this aligned with command_policy.
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
        // 수동 편집기 내보내기 — 자막은 사용자가 캔버스로 직접 얹는다.
        // 훅 자막은 자동 편집이 "왜 이 장면인지" 를 아는 경로에만 붙는다.
        captions: None,
        framing: VerticalFraming::CenterCrop,
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
) -> AppResult<AutoEditJobReceipt> {
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

    if !is_pro {
        reconcile_pending_quota(&state.auth, &state.storage, &user_id).await;
    }

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

    let job_id = format!("auto_edit_{}", uuid::Uuid::new_v4());
    let cancellation = state
        .auto_composer
        .begin_job(&job_id)
        .await
        .map_err(|error| AppError::Validation(error.to_string()))?;

    let config_json = serde_json::to_string(&config)
        .map_err(|error| AppError::Validation(format!("Invalid auto-edit snapshot: {error}")))?;
    if let Err(error) = state.storage.create_media_job(
        &job_id,
        &user_id,
        crate::storage::MediaJobKind::AutoEdit,
        &config_json,
    ) {
        state.auto_composer.finish_job(&job_id).await;
        return Err(AppError::Database(error.to_string()));
    }

    tracing::info!(
        "Starting auto-edit job: {} with target duration: {}s",
        job_id,
        config.target_duration
    );

    let composer = state.auto_composer.clone();
    let storage = state.storage.clone();
    let auth = state.auth.clone();
    let executor = state.media_job_executor.clone();
    let spawned_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let completed = run_durable_auto_edit_job(
            composer.clone(),
            storage,
            auth,
            spawned_job_id.clone(),
            user_id,
            is_pro,
            config,
            cancellation,
            executor,
        )
        .await;
        if completed {
            composer.cleanup_job_artifacts(&spawned_job_id).await;
        }
        composer.finish_job(&spawned_job_id).await;
    });

    Ok(AutoEditJobReceipt {
        job_id,
        status: AutoEditStatus::Queued,
    })
}

#[derive(Clone)]
struct DurablePartOutput {
    result_id: String,
    config: AutoEditConfig,
    validated_path: PathBuf,
    validation: crate::video::OutputValidationReport,
    clip_count: usize,
    clip_keys: Vec<(String, String)>,
    output_kind: AutoEditOutputKind,
    part_index: usize,
    part_count: usize,
}

#[allow(clippy::too_many_arguments)]
async fn run_durable_auto_edit_job(
    composer: std::sync::Arc<crate::video::AutoComposer>,
    storage: std::sync::Arc<crate::storage::Storage>,
    auth: std::sync::Arc<AuthManager>,
    job_id: String,
    user_id: String,
    is_pro: bool,
    config: AutoEditConfig,
    cancellation: tokio_util::sync::CancellationToken,
    executor: std::sync::Arc<crate::video::media_job_executor::MediaJobExecutor>,
) -> bool {
    let started = std::time::Instant::now();
    let run = crate::video::with_auto_edit_context(job_id.clone(), cancellation, async {
        storage
            .update_media_job_status(
                &job_id,
                crate::storage::MediaJobStatus::Running,
                "planning",
                1.0,
                None,
                None,
            )
            .map_err(storage_video_error)?;
        let render_configs = build_render_configs(&composer, config).await?;
        let part_count = render_configs.len();
        let snapshot = storage
            .load_media_job(&job_id)
            .map_err(storage_video_error)?;
        if snapshot.parts.is_empty() {
            let trims = render_configs
                .iter()
                .map(|part| {
                    serde_json::to_string(&part.storyboard).unwrap_or_else(|_| "[]".to_string())
                })
                .collect::<Vec<_>>();
            storage
                .initialize_media_job_parts(&job_id, &trims)
                .map_err(storage_video_error)?;
        }

        let mut drafts = Vec::with_capacity(part_count);
        for (index, part_config) in render_configs.into_iter().enumerate() {
            let part_index = index + 1;
            let result_id = if part_count == 1 {
                job_id.clone()
            } else {
                format!("{}_part_{:02}", job_id, part_index)
            };
            let output_kind = match part_config.output_intent {
                AutoEditOutputIntent::VerticalVideo => AutoEditOutputKind::VerticalVideo,
                AutoEditOutputIntent::ShortsSeries if part_count > 1 => {
                    AutoEditOutputKind::ShortSeriesPart
                }
                _ => AutoEditOutputKind::Short,
            };
            let clip_keys = part_config
                .storyboard
                .as_ref()
                .map(|storyboard| {
                    storyboard
                        .iter()
                        .map(|clip| (clip.game_id.clone(), clip.file_path.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let planned_duration = part_config.storyboard.as_ref().map(|storyboard| {
                storyboard
                    .iter()
                    .map(|clip| clip.trim_end_secs - clip.trim_start_secs)
                    .sum::<f64>()
            });
            let mut persisted = storage
                .load_media_job(&job_id)
                .map_err(storage_video_error)?
                .parts
                .into_iter()
                .find(|part| part.part_index == part_index)
                .ok_or_else(|| crate::video::VideoError::ProcessingError {
                    message: format!("Missing durable part checkpoint {part_index}"),
                })?;

            let recovered = recover_existing_part(
                &executor,
                &composer,
                &storage,
                &job_id,
                &result_id,
                &part_config,
                &mut persisted,
            )
            .await?;

            let (validated_path, validation, clip_count) = if let Some((path, report)) = recovered {
                (path, report, clip_keys.len())
            } else {
                render_and_validate_part(
                    &executor,
                    &composer,
                    &storage,
                    &job_id,
                    result_id.clone(),
                    part_config.clone(),
                    is_pro,
                    persisted,
                    planned_duration,
                )
                .await?
            };

            drafts.push(DurablePartOutput {
                result_id,
                config: part_config,
                validated_path,
                validation,
                clip_count,
                clip_keys,
                output_kind,
                part_index,
                part_count,
            });
        }

        storage
            .update_media_job_status(
                &job_id,
                crate::storage::MediaJobStatus::Validating,
                "publishing",
                99.0,
                None,
                None,
            )
            .map_err(storage_video_error)?;
        let final_dir = composer.final_dir();
        tokio::fs::create_dir_all(&final_dir)
            .await
            .map_err(|error| crate::video::VideoError::ProcessingError {
                message: format!("Could not create final output directory: {error}"),
            })?;
        let mut results = Vec::with_capacity(drafts.len());
        let mut completed_outputs = Vec::with_capacity(drafts.len());
        let mut all_clip_keys = Vec::new();
        for draft in drafts {
            let final_path = final_dir.join(format!("{}.mp4", draft.result_id));
            move_media_file(&draft.validated_path, &final_path).await?;
            let fingerprint =
                crate::video::output_validation::file_fingerprint_async(final_path.clone())
                    .await
                    .map_err(|error| crate::video::VideoError::ProcessingError {
                        message: error.to_string(),
                    })?;
            let mut checkpoint = storage
                .load_media_job(&job_id)
                .map_err(storage_video_error)?
                .parts
                .into_iter()
                .find(|part| part.part_index == draft.part_index)
                .ok_or_else(|| crate::video::VideoError::ProcessingError {
                    message: "Missing publication checkpoint".to_string(),
                })?;
            checkpoint.output_path = Some(final_path.to_string_lossy().to_string());
            checkpoint.file_fingerprint = Some(fingerprint);
            storage
                .update_media_job_part(&job_id, &checkpoint)
                .map_err(storage_video_error)?;
            let thumbnail_path =
                crate::video::thumbnail::auto_generate_thumbnail(&final_path, &final_dir)
                    .await
                    .ok()
                    .map(|path| path.to_string_lossy().to_string());
            let file_size_bytes = tokio::fs::metadata(&final_path)
                .await
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            let output_kind = serde_json::to_value(draft.output_kind)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default();
            results.push(crate::storage::AutoEditResultMetadata {
                result_id: draft.result_id.clone(),
                job_id: job_id.clone(),
                output_path: final_path.to_string_lossy().to_string(),
                thumbnail_path,
                created_at: chrono::Utc::now(),
                duration: draft.validation.duration,
                clip_count: draft.clip_count,
                game_ids: draft.config.game_ids.clone(),
                target_duration: draft.config.target_duration,
                canvas_template_name: draft
                    .config
                    .canvas_template
                    .as_ref()
                    .map(|template| template.name.clone()),
                has_background_music: draft.config.background_music.is_some(),
                youtube_status: Some(crate::storage::YouTubeUploadStatus {
                    video_id: None,
                    status: crate::storage::UploadStatus::NotUploaded,
                    upload_started_at: None,
                    upload_completed_at: None,
                    progress: 0.0,
                    error: None,
                }),
                file_size_bytes,
                publish_title: draft.config.publish_metadata.title.clone(),
                publish_description: draft.config.publish_metadata.description.clone(),
                publish_tags: draft.config.publish_metadata.tags.clone(),
                publish_privacy_status: draft.config.publish_metadata.privacy_status.clone(),
                output_intent: enum_string(draft.config.output_intent),
                framing_mode: enum_string(draft.config.framing_mode),
                platform_preset: enum_string(draft.config.platform_preset),
                series_id: job_id.clone(),
                part_index: draft.part_index,
                part_count: draft.part_count,
                output_kind,
                validation: Some(draft.validation.clone()),
                platform_exports: Vec::new(),
            });
            completed_outputs.push(AutoEditOutput {
                result_id: draft.result_id,
                output_path: final_path.to_string_lossy().to_string(),
                duration: draft.validation.duration,
                clips_used: draft.clip_count,
                file_size_bytes,
                output_kind: draft.output_kind,
                part_index: (draft.part_count > 1).then_some(draft.part_index),
                part_count: (draft.part_count > 1).then_some(draft.part_count),
            });
            all_clip_keys.extend(draft.clip_keys);
        }
        all_clip_keys.sort();
        all_clip_keys.dedup();
        executor.before(crate::video::media_job_executor::MediaFailurePoint::Publish)?;
        let server_synced = is_pro
            || (executor
                .before(crate::video::media_job_executor::MediaFailurePoint::QuotaSync)
                .is_ok()
                && server_quota_consume(&auth, &job_id).await);
        storage
            .publish_auto_edit_series(
                &job_id,
                &user_id,
                is_pro,
                server_synced,
                &results,
                &all_clip_keys,
            )
            .map_err(storage_video_error)?;
        composer
            .update_progress_completed_outputs(
                &job_id,
                completed_outputs,
                started.elapsed().as_secs_f64(),
            )
            .await;
        Ok::<(), crate::video::VideoError>(())
    })
    .await;

    match run {
        Ok(()) => true,
        Err(error) => {
            let (status, code, stage) = match executor.classify(&error) {
                crate::video::media_job_executor::MediaFailureClass::Paused => (
                    crate::storage::MediaJobStatus::Paused,
                    "cancelled",
                    "paused",
                ),
                crate::video::media_job_executor::MediaFailureClass::Recoverable => (
                    crate::storage::MediaJobStatus::Recoverable,
                    "recoverable_media_error",
                    "recoverable",
                ),
                crate::video::media_job_executor::MediaFailureClass::Failed => (
                    crate::storage::MediaJobStatus::Failed,
                    "render_failed",
                    "failed",
                ),
            };
            if let Err(storage_error) = storage.update_media_job_status(
                &job_id,
                status,
                stage,
                0.0,
                Some(code),
                Some(&error.to_string()),
            ) {
                tracing::warn!("Could not persist failed media job: {}", storage_error);
            }
            if matches!(error, crate::video::VideoError::Cancelled) {
                let _ = composer.cancel_job(&job_id).await;
            } else {
                composer
                    .update_progress_failed(
                        &job_id,
                        error.to_string(),
                        started.elapsed().as_secs_f64(),
                    )
                    .await;
            }
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn render_and_validate_part(
    executor: &crate::video::media_job_executor::MediaJobExecutor,
    composer: &crate::video::AutoComposer,
    storage: &crate::storage::Storage,
    job_id: &str,
    result_id: String,
    config: AutoEditConfig,
    is_pro: bool,
    mut checkpoint: crate::storage::MediaJobPart,
    planned_duration: Option<f64>,
) -> crate::video::Result<(PathBuf, crate::video::OutputValidationReport, usize)> {
    executor.before(crate::video::media_job_executor::MediaFailurePoint::Process)?;
    storage
        .update_media_job_status(
            job_id,
            crate::storage::MediaJobStatus::Running,
            &format!("rendering_part_{}", checkpoint.part_index),
            ((checkpoint.part_index.saturating_sub(1)) as f64
                / checkpoint.part_count.max(1) as f64)
                * 90.0,
            None,
            None,
        )
        .map_err(storage_video_error)?;
    checkpoint.status = crate::storage::MediaJobStatus::Running;
    checkpoint.attempt_count = checkpoint.attempt_count.saturating_add(1);
    checkpoint.progress_percentage = 1.0;
    storage
        .update_media_job_part(job_id, &checkpoint)
        .map_err(storage_video_error)?;
    let result = composer.compose(config.clone(), result_id, is_pro).await?;
    let mut partial = PathBuf::from(&result.output_path);
    checkpoint.partial_path = Some(result.output_path.clone());
    checkpoint.status = crate::storage::MediaJobStatus::Validating;
    checkpoint.progress_percentage = 95.0;
    storage
        .update_media_job_part(job_id, &checkpoint)
        .map_err(storage_video_error)?;
    storage
        .update_media_job_status(
            job_id,
            crate::storage::MediaJobStatus::Validating,
            &format!("validating_part_{}", checkpoint.part_index),
            90.0,
            None,
            None,
        )
        .map_err(storage_video_error)?;
    executor.before(crate::video::media_job_executor::MediaFailurePoint::Validate)?;
    let mut report =
        crate::video::OutputValidator::validate(&partial, config.platform_preset, planned_duration)
            .await;
    if !report.is_delivery_ready() {
        // Composer inputs are allowed to be silent or use a non-delivery codec.
        // Normalize once through the shared contract path before declaring the
        // part invalid. Corrupt/truncated media still fails this transcode.
        let normalized = partial.with_file_name(format!(
            "{}.normalized.partial.mp4",
            partial
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("output")
        ));
        crate::video::OutputValidator::transcode_to_contract(&partial, &normalized)
            .await
            .map_err(|message| crate::video::VideoError::ProcessingError { message })?;
        let normalized_report = crate::video::OutputValidator::validate(
            &normalized,
            config.platform_preset,
            planned_duration,
        )
        .await;
        if normalized_report.is_delivery_ready() {
            tokio::fs::remove_file(&partial).await.map_err(|error| {
                crate::video::VideoError::ProcessingError {
                    message: error.to_string(),
                }
            })?;
            partial = normalized;
            checkpoint.partial_path = Some(partial.to_string_lossy().to_string());
        }
        report = normalized_report;
    }
    checkpoint.validation = Some(report.clone());
    if !report.is_delivery_ready() {
        checkpoint.status = crate::storage::MediaJobStatus::Failed;
        storage
            .update_media_job_part(job_id, &checkpoint)
            .map_err(storage_video_error)?;
        return Err(crate::video::VideoError::ProcessingError {
            message: format!(
                "Output validation failed: {}",
                report
                    .issues
                    .iter()
                    .map(|issue| issue.code.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    let validated = crate::video::output_validation::validated_path_for(&partial);
    move_media_file(&partial, &validated).await?;
    checkpoint.status = crate::storage::MediaJobStatus::Complete;
    checkpoint.progress_percentage = 100.0;
    checkpoint.output_path = Some(validated.to_string_lossy().to_string());
    checkpoint.file_fingerprint = Some(
        crate::video::output_validation::file_fingerprint_async(validated.clone())
            .await
            .map_err(|error| crate::video::VideoError::ProcessingError {
                message: error.to_string(),
            })?,
    );
    executor.before(crate::video::media_job_executor::MediaFailurePoint::PartCheckpoint)?;
    storage
        .update_media_job_part(job_id, &checkpoint)
        .map_err(storage_video_error)?;
    Ok((validated, report, result.clip_count))
}

async fn recover_existing_part(
    executor: &crate::video::media_job_executor::MediaJobExecutor,
    composer: &crate::video::AutoComposer,
    storage: &crate::storage::Storage,
    job_id: &str,
    result_id: &str,
    config: &AutoEditConfig,
    checkpoint: &mut crate::storage::MediaJobPart,
) -> crate::video::Result<Option<(PathBuf, crate::video::OutputValidationReport)>> {
    use crate::video::media_job_executor::MediaFailurePoint;

    let persisted_path = checkpoint.output_path.as_deref().map(std::path::Path::new);
    let candidates = executor.file_system.validated_candidates(
        &composer.job_stage_dir(job_id),
        &composer.final_dir(),
        result_id,
        persisted_path,
    );
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let fingerprint = match executor.file_system.fingerprint(&candidate).await {
            Ok(value) => value,
            Err(_) => continue,
        };
        if checkpoint.output_path.as_deref() == candidate.to_str()
            && checkpoint
                .file_fingerprint
                .as_ref()
                .is_some_and(|expected| expected != &fingerprint)
        {
            continue;
        }
        executor.before(MediaFailurePoint::Validate)?;
        let planned_duration = config.storyboard.as_ref().map(|storyboard| {
            storyboard
                .iter()
                .map(|clip| clip.trim_end_secs - clip.trim_start_secs)
                .sum::<f64>()
        });
        let report = crate::video::OutputValidator::validate(
            &candidate,
            config.platform_preset,
            planned_duration,
        )
        .await;
        if !report.is_delivery_ready() {
            continue;
        }
        checkpoint.status = crate::storage::MediaJobStatus::Complete;
        checkpoint.progress_percentage = 100.0;
        checkpoint.output_path = Some(candidate.to_string_lossy().to_string());
        checkpoint.file_fingerprint = Some(fingerprint);
        checkpoint.validation = Some(report.clone());
        executor.before(MediaFailurePoint::PartCheckpoint)?;
        storage
            .update_media_job_part(job_id, checkpoint)
            .map_err(storage_video_error)?;
        return Ok(Some((candidate, report)));
    }
    Ok(None)
}

async fn move_media_file(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> crate::video::Result<()> {
    if source == destination {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            crate::video::VideoError::ProcessingError {
                message: error.to_string(),
            }
        })?;
    }
    if destination.exists() {
        tokio::fs::remove_file(destination).await.map_err(|error| {
            crate::video::VideoError::ProcessingError {
                message: error.to_string(),
            }
        })?;
    }
    if tokio::fs::rename(source, destination).await.is_err() {
        tokio::fs::copy(source, destination)
            .await
            .map_err(|error| crate::video::VideoError::ProcessingError {
                message: error.to_string(),
            })?;
        tokio::fs::remove_file(source).await.map_err(|error| {
            crate::video::VideoError::ProcessingError {
                message: error.to_string(),
            }
        })?;
    }
    Ok(())
}

fn storage_video_error(error: crate::storage::StorageError) -> crate::video::VideoError {
    crate::video::VideoError::ProcessingError {
        message: error.to_string(),
    }
}

fn enum_string<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

async fn build_render_configs(
    composer: &crate::video::AutoComposer,
    mut config: AutoEditConfig,
) -> crate::video::Result<Vec<AutoEditConfig>> {
    if config.output_intent != AutoEditOutputIntent::ShortsSeries {
        return Ok(vec![config]);
    }
    if config.storyboard.is_none() {
        let plan = composer.plan(&config).await?;
        config.storyboard = Some(plan.clips.into_iter().map(|clip| clip.storyboard).collect());
        config.selected_clip_paths = None;
    }
    let parts = partition_storyboard(config.storyboard.take().unwrap_or_default(), 180.0)?;
    if parts.is_empty() {
        return Err(crate::video::VideoError::NoClipsFound);
    }
    Ok(parts
        .into_iter()
        .map(|part| {
            let mut part_config = config.clone();
            part_config.game_ids = part
                .iter()
                .map(|clip| clip.game_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            part_config.storyboard = Some(part);
            part_config
        })
        .collect())
}

fn partition_storyboard(
    mut storyboard: Vec<StoryboardClip>,
    max_duration: f64,
) -> crate::video::Result<Vec<Vec<StoryboardClip>>> {
    storyboard.sort_by_key(|clip| clip.order);
    let mut parts: Vec<Vec<StoryboardClip>> = Vec::new();
    let mut current: Vec<StoryboardClip> = Vec::new();
    let mut current_duration = 0.0;

    for clip in storyboard {
        let mut start = clip.trim_start_secs;
        let end = clip.trim_end_secs;
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
            return Err(crate::video::VideoError::ProcessingError {
                message: format!("invalid storyboard range for {}", clip.file_path),
            });
        }
        while start < end - 0.001 {
            let remaining = end - start;
            if remaining > max_duration {
                if !current.is_empty() {
                    renumber_storyboard(&mut current);
                    parts.push(std::mem::take(&mut current));
                    current_duration = 0.0;
                }
                let mut segment = clip.clone();
                segment.trim_start_secs = start;
                segment.trim_end_secs = start + max_duration;
                segment.order = 0;
                parts.push(vec![segment]);
                start += max_duration;
                continue;
            }
            if !current.is_empty() && current_duration + remaining > max_duration + 0.001 {
                renumber_storyboard(&mut current);
                parts.push(std::mem::take(&mut current));
                current_duration = 0.0;
            }
            let mut segment = clip.clone();
            segment.trim_start_secs = start;
            segment.trim_end_secs = end;
            current_duration += remaining;
            current.push(segment);
            start = end;
        }
    }
    if !current.is_empty() {
        renumber_storyboard(&mut current);
        parts.push(current);
    }
    Ok(parts)
}

fn renumber_storyboard(storyboard: &mut [StoryboardClip]) {
    for (index, clip) in storyboard.iter_mut().enumerate() {
        clip.order = index as u32;
    }
}

#[tauri::command]
pub async fn plan_auto_edit(
    state: State<'_, AppState>,
    config: AutoEditConfig,
) -> AppResult<AutoEditPlan> {
    require_command_access(&state.auth, "plan_auto_edit")
        .map_err(|error| AppError::Auth(error.to_string()))?;
    state
        .auto_composer
        .plan(&config)
        .await
        .map_err(|error| AppError::Video(error.to_string()))
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
async fn call_quota_action(
    auth: &AuthManager,
    action: &str,
    job_id: Option<&str>,
) -> Option<serde_json::Value> {
    let user = auth.get_current_user().ok().flatten()?;
    if user.access_token.is_empty() {
        return None;
    }
    let client = auth.get_supabase_client().ok()?;
    match client
        .call_edge_function(
            "quota",
            &user.access_token,
            &serde_json::json!({ "action": action, "job_id": job_id }),
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
    match call_quota_action(auth, "check", None).await {
        Some(value) => parse_quota_verdict(&value),
        None => ServerQuotaVerdict::Unavailable,
    }
}

/// Record one authoritative consume after a successful compose. Best effort:
/// any failure is logged and swallowed because the export already succeeded.
async fn server_quota_consume(auth: &AuthManager, job_id: &str) -> bool {
    match call_quota_action(auth, "consume", Some(job_id)).await {
        Some(value) => match parse_quota_verdict(&value) {
            ServerQuotaVerdict::Allowed => {
                tracing::info!("Server auto-edit quota consume recorded");
                true
            }
            ServerQuotaVerdict::Denied { used, limit } => {
                tracing::warn!(
                    "Server quota consume reported over-limit ({}/{}) after a successful compose",
                    used,
                    limit
                );
                false
            }
            ServerQuotaVerdict::Unavailable => false,
        },
        None => {
            tracing::warn!(
                "Server auto-edit quota consume unavailable; relying on the local counter cache"
            );
            false
        }
    }
}

async fn reconcile_pending_quota(
    auth: &AuthManager,
    storage: &crate::storage::Storage,
    user_id: &str,
) {
    let pending = match storage.pending_quota_job_ids(user_id) {
        Ok(pending) => pending,
        Err(error) => {
            tracing::warn!("Could not load pending quota sync records: {}", error);
            return;
        }
    };
    for job_id in pending {
        if !server_quota_consume(auth, &job_id).await {
            break;
        }
        if let Err(error) = storage.mark_quota_job_synced(user_id, &job_id) {
            tracing::warn!("Could not mark quota job {} synced: {}", job_id, error);
            break;
        }
    }
}

/// Get progress of an auto-edit job
#[tauri::command]
pub async fn get_auto_edit_progress(
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<Option<AutoEditProgress>> {
    require_command_access(&state.auth, "get_auto_edit_progress")
        .map_err(|e| AppError::Auth(e.to_string()))?;

    if let Some(progress) = state.auto_composer.get_progress(&job_id).await {
        return Ok(Some(progress));
    }
    let snapshot = match state.storage.load_media_job(&job_id) {
        Ok(snapshot) => snapshot,
        Err(_) => return Ok(None),
    };
    let status = match snapshot.status {
        MediaJobStatus::Queued => AutoEditStatus::Queued,
        MediaJobStatus::Complete => AutoEditStatus::Completed,
        MediaJobStatus::Paused | MediaJobStatus::Discarded => AutoEditStatus::Cancelled,
        MediaJobStatus::Failed => AutoEditStatus::Failed,
        _ => AutoEditStatus::Processing,
    };
    let outputs = state
        .storage
        .load_auto_edit_results()
        .unwrap_or_default()
        .into_iter()
        .filter(|result| result.job_id == job_id)
        .map(|result| AutoEditOutput {
            result_id: result.result_id,
            output_path: result.output_path,
            duration: result.duration,
            clips_used: result.clip_count,
            file_size_bytes: result.file_size_bytes,
            output_kind: match result.output_kind.as_str() {
                "short_series_part" => AutoEditOutputKind::ShortSeriesPart,
                "vertical_video" => AutoEditOutputKind::VerticalVideo,
                _ => AutoEditOutputKind::Short,
            },
            part_index: (result.part_count > 1).then_some(result.part_index),
            part_count: (result.part_count > 1).then_some(result.part_count),
        })
        .collect::<Vec<_>>();
    Ok(Some(AutoEditProgress {
        job_id,
        status,
        progress: snapshot.progress_percentage,
        current_step: snapshot.current_stage,
        elapsed_seconds: 0.0,
        estimated_seconds: 0.0,
        output_path: outputs.first().map(|output| output.output_path.clone()),
        error: snapshot.error_message,
        outputs,
    }))
}

#[tauri::command]
pub async fn cancel_auto_edit(
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<AutoEditProgress> {
    require_command_access(&state.auth, "cancel_auto_edit")
        .map_err(|error| AppError::Auth(error.to_string()))?;
    let progress = state
        .auto_composer
        .cancel_job(&job_id)
        .await
        .map_err(|error| AppError::Video(error.to_string()))?;
    if let Ok(snapshot) = state.storage.load_media_job(&job_id) {
        if matches!(
            snapshot.status,
            MediaJobStatus::Queued | MediaJobStatus::Running | MediaJobStatus::Validating
        ) {
            let _ = state.storage.update_media_job_status(
                &job_id,
                MediaJobStatus::Paused,
                "paused",
                snapshot.progress_percentage,
                None,
                None,
            );
        }
    }
    Ok(progress)
}

#[tauri::command]
pub async fn get_media_job(
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<MediaJobSnapshot> {
    let user = require_command_access(&state.auth, "get_media_job")
        .map_err(|error| AppError::Auth(error.to_string()))?
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;
    let snapshot = state
        .storage
        .load_media_job(&job_id)
        .map_err(|error| AppError::Database(error.to_string()))?;
    if snapshot.user_id != user.id {
        return Err(AppError::Auth(
            "Media job does not belong to the current user".to_string(),
        ));
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn list_recoverable_media_jobs(
    state: State<'_, AppState>,
) -> AppResult<Vec<MediaJobSnapshot>> {
    let user = require_command_access(&state.auth, "list_recoverable_media_jobs")
        .map_err(|error| AppError::Auth(error.to_string()))?
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;
    state
        .storage
        .list_recoverable_media_jobs(&user.id)
        .map_err(|error| AppError::Database(error.to_string()))
}

#[tauri::command]
pub async fn pause_media_job(
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<MediaJobSnapshot> {
    let _ = get_media_job(state.clone(), job_id.clone()).await?;
    let _ = state.auto_composer.cancel_job(&job_id).await;
    let snapshot = state
        .storage
        .load_media_job(&job_id)
        .map_err(|error| AppError::Database(error.to_string()))?;
    if matches!(
        snapshot.status,
        MediaJobStatus::Queued | MediaJobStatus::Running | MediaJobStatus::Validating
    ) {
        state
            .storage
            .update_media_job_status(
                &job_id,
                MediaJobStatus::Paused,
                "paused",
                snapshot.progress_percentage,
                None,
                None,
            )
            .map_err(|error| AppError::Database(error.to_string()))?;
    }
    state
        .storage
        .load_media_job(&job_id)
        .map_err(|error| AppError::Database(error.to_string()))
}

#[tauri::command]
pub async fn resume_media_job(
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<AutoEditJobReceipt> {
    let user = require_command_access(&state.auth, "resume_media_job")
        .map_err(|error| AppError::Auth(error.to_string()))?
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;
    let snapshot = state
        .storage
        .load_media_job(&job_id)
        .map_err(|error| AppError::Database(error.to_string()))?;
    if snapshot.user_id != user.id
        || snapshot.kind != MediaJobKind::AutoEdit
        || !snapshot.recoverable
    {
        return Err(AppError::Validation(
            "Media job cannot be resumed".to_string(),
        ));
    }
    let config: AutoEditConfig = serde_json::from_str(&snapshot.config_json)
        .map_err(|error| AppError::Validation(format!("Saved job snapshot is invalid: {error}")))?;
    let tier = state
        .auth
        .get_tier()
        .map_err(|error| AppError::Auth(error.to_string()))?;
    let is_pro = matches!(tier, SubscriptionTier::Pro);
    let cancellation = state
        .auto_composer
        .begin_job(&job_id)
        .await
        .map_err(|error| AppError::Validation(error.to_string()))?;
    state
        .storage
        .update_media_job_status(
            &job_id,
            MediaJobStatus::Queued,
            "queued",
            snapshot.progress_percentage,
            None,
            None,
        )
        .map_err(|error| AppError::Database(error.to_string()))?;
    let composer = state.auto_composer.clone();
    let storage = state.storage.clone();
    let auth = state.auth.clone();
    let executor = state.media_job_executor.clone();
    let spawned_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let completed = run_durable_auto_edit_job(
            composer.clone(),
            storage,
            auth,
            spawned_job_id.clone(),
            user.id,
            is_pro,
            config,
            cancellation,
            executor,
        )
        .await;
        if completed {
            composer.cleanup_job_artifacts(&spawned_job_id).await;
        }
        composer.finish_job(&spawned_job_id).await;
    });
    Ok(AutoEditJobReceipt {
        job_id,
        status: AutoEditStatus::Queued,
    })
}

#[tauri::command]
pub async fn discard_media_job(state: State<'_, AppState>, job_id: String) -> AppResult<()> {
    let snapshot = get_media_job(state.clone(), job_id.clone()).await?;
    if !snapshot.recoverable && snapshot.status != MediaJobStatus::Failed {
        return Err(AppError::Validation(
            "Only paused, recoverable, or failed media jobs can be discarded".to_string(),
        ));
    }
    let _ = state.auto_composer.cancel_job(&job_id).await;
    let roots = [
        state.auto_composer.job_stage_dir(&job_id),
        state.auto_composer.final_dir(),
    ];
    for part in &snapshot.parts {
        for value in [part.partial_path.as_ref(), part.output_path.as_ref()]
            .into_iter()
            .flatten()
        {
            let path = std::path::Path::new(value);
            if safe_unpublished_job_path(path, &roots, &job_id) && path.is_file() {
                tokio::fs::remove_file(path).await.map_err(|error| {
                    AppError::Video(format!("Could not discard media artifact: {error}"))
                })?;
            }
        }
    }
    let stage = state.auto_composer.job_stage_dir(&job_id);
    if stage.is_dir() {
        tokio::fs::remove_dir_all(&stage).await.map_err(|error| {
            AppError::Video(format!("Could not discard job directory: {error}"))
        })?;
    }
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || {
        storage.update_media_job_status(
            &job_id,
            MediaJobStatus::Discarded,
            "discarded",
            snapshot.progress_percentage,
            None,
            None,
        )
    })
    .await
    .map_err(|error| AppError::Internal(format!("Discard checkpoint task failed: {error}")))?
    .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(())
}

fn safe_unpublished_job_path(path: &std::path::Path, roots: &[PathBuf; 2], job_id: &str) -> bool {
    let file_matches = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with(job_id))
        .unwrap_or(false);
    if !file_matches {
        return false;
    }
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| path.starts_with(root))
}

#[tauri::command]
pub async fn export_auto_edit_for_platform(
    state: State<'_, AppState>,
    result_id: String,
    platform_preset: String,
) -> AppResult<String> {
    let receipt = begin_platform_export(
        &state,
        result_id,
        platform_preset,
        "export_auto_edit_for_platform",
    )
    .await?;
    Ok(receipt.job_id)
}

#[tauri::command]
pub async fn start_platform_export(
    state: State<'_, AppState>,
    result_id: String,
    platform_preset: String,
) -> AppResult<AutoEditJobReceipt> {
    begin_platform_export(&state, result_id, platform_preset, "start_platform_export").await
}

async fn begin_platform_export(
    state: &AppState,
    result_id: String,
    platform_preset: String,
    command_name: &str,
) -> AppResult<AutoEditJobReceipt> {
    let user = require_command_access(&state.auth, command_name)
        .map_err(|error| AppError::Auth(error.to_string()))?
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;
    let preset = match platform_preset.as_str() {
        "tiktok" => crate::video::auto_composer::PlatformPreset::Tiktok,
        "instagram_reels" => crate::video::auto_composer::PlatformPreset::InstagramReels,
        "youtube_shorts" => crate::video::auto_composer::PlatformPreset::YoutubeShorts,
        _ => {
            return Err(AppError::Validation(format!(
                "Unsupported platform preset: {platform_preset}"
            )))
        }
    };
    let result = state
        .storage
        .load_auto_edit_result(&result_id)
        .map_err(|error| AppError::Database(error.to_string()))?;
    let source = PathBuf::from(&result.output_path);
    if !source.is_file() {
        return Err(AppError::Validation(
            "Auto-edit output is missing".to_string(),
        ));
    }
    let job_id = format!("platform_export_{}", uuid::Uuid::new_v4());
    let cancellation = state
        .auto_composer
        .begin_job(&job_id)
        .await
        .map_err(|error| AppError::Validation(error.to_string()))?;
    let config = serde_json::json!({ "result_id": result_id, "platform_preset": platform_preset });
    if let Err(error) = state.storage.create_media_job(
        &job_id,
        &user.id,
        MediaJobKind::PlatformExport,
        &config.to_string(),
    ) {
        state.auto_composer.finish_job(&job_id).await;
        return Err(AppError::Database(error.to_string()));
    }
    state
        .storage
        .initialize_media_job_parts(&job_id, &[config.to_string()])
        .map_err(|error| AppError::Database(error.to_string()))?;
    let storage = state.storage.clone();
    let composer = state.auto_composer.clone();
    let executor = state.media_job_executor.clone();
    let spawned_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let run =
            crate::video::with_auto_edit_context(spawned_job_id.clone(), cancellation, async {
                storage
                    .update_media_job_status(
                        &spawned_job_id,
                        MediaJobStatus::Running,
                        "validating_source",
                        5.0,
                        None,
                        None,
                    )
                    .map_err(storage_video_error)?;
                let source_report =
                    crate::video::OutputValidator::validate(&source, preset, Some(result.duration))
                        .await;
                let (output_path, passthrough, owns_file, report) = if source_report
                    .is_delivery_ready()
                {
                    (source.clone(), true, false, source_report)
                } else {
                    executor
                        .before(crate::video::media_job_executor::MediaFailurePoint::Process)?;
                    let export_dir = composer.final_dir().join("platform_exports");
                    tokio::fs::create_dir_all(&export_dir)
                        .await
                        .map_err(|error| crate::video::VideoError::ProcessingError {
                            message: error.to_string(),
                        })?;
                    let partial = export_dir.join(format!("{}.partial.mp4", spawned_job_id));
                    crate::video::OutputValidator::transcode_to_contract(&source, &partial)
                        .await
                        .map_err(|message| crate::video::VideoError::ProcessingError { message })?;
                    storage
                        .update_media_job_status(
                            &spawned_job_id,
                            MediaJobStatus::Validating,
                            "validating_export",
                            90.0,
                            None,
                            None,
                        )
                        .map_err(storage_video_error)?;
                    let report =
                        crate::video::OutputValidator::validate(&partial, preset, None).await;
                    if !report.is_delivery_ready() {
                        return Err(crate::video::VideoError::ProcessingError {
                            message: format!(
                                "Platform export validation failed: {}",
                                report
                                    .issues
                                    .iter()
                                    .map(|issue| issue.code.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        });
                    }
                    let destination = export_dir.join(format!(
                        "{}_{}.mp4",
                        result.result_id,
                        enum_string(preset)
                    ));
                    move_media_file(&partial, &destination).await?;
                    (destination, false, true, report)
                };
                let export = PlatformExportMetadata {
                    export_id: format!("export_{}", uuid::Uuid::new_v4()),
                    job_id: spawned_job_id.clone(),
                    result_id: result.result_id.clone(),
                    preset,
                    output_path: output_path.to_string_lossy().to_string(),
                    passthrough,
                    owns_file,
                    created_at: chrono::Utc::now(),
                    validation: report.clone(),
                };
                storage
                    .update_media_job_status(
                        &spawned_job_id,
                        MediaJobStatus::Validating,
                        "committing_export",
                        95.0,
                        None,
                        None,
                    )
                    .map_err(storage_video_error)?;
                executor.before(crate::video::media_job_executor::MediaFailurePoint::Publish)?;
                storage
                    .save_platform_export(&export)
                    .map_err(storage_video_error)?;
                let mut part = storage
                    .load_media_job(&spawned_job_id)
                    .map_err(storage_video_error)?
                    .parts
                    .remove(0);
                part.status = MediaJobStatus::Complete;
                part.progress_percentage = 100.0;
                part.output_path = Some(export.output_path);
                part.validation = Some(report);
                part.file_fingerprint =
                    crate::video::output_validation::file_fingerprint_async(output_path.clone())
                        .await
                        .ok();
                storage
                    .update_media_job_part(&spawned_job_id, &part)
                    .map_err(storage_video_error)?;
                storage
                    .update_media_job_status(
                        &spawned_job_id,
                        MediaJobStatus::Complete,
                        "complete",
                        100.0,
                        None,
                        None,
                    )
                    .map_err(storage_video_error)?;
                Ok::<(), crate::video::VideoError>(())
            })
            .await;
        if let Err(error) = run {
            let status = match executor.classify(&error) {
                crate::video::media_job_executor::MediaFailureClass::Paused => {
                    MediaJobStatus::Paused
                }
                crate::video::media_job_executor::MediaFailureClass::Recoverable => {
                    MediaJobStatus::Recoverable
                }
                crate::video::media_job_executor::MediaFailureClass::Failed => {
                    MediaJobStatus::Failed
                }
            };
            let _ = storage.update_media_job_status(
                &spawned_job_id,
                status,
                if status == MediaJobStatus::Recoverable {
                    "recoverable"
                } else {
                    "failed"
                },
                0.0,
                Some("platform_export_failed"),
                Some(&error.to_string()),
            );
        }
        composer.finish_job(&spawned_job_id).await;
    });
    Ok(AutoEditJobReceipt {
        job_id,
        status: AutoEditStatus::Queued,
    })
}

#[tauri::command]
pub async fn revalidate_auto_edit_result(
    state: State<'_, AppState>,
    result_id: String,
) -> AppResult<crate::video::OutputValidationReport> {
    require_command_access(&state.auth, "revalidate_auto_edit_result")
        .map_err(|error| AppError::Auth(error.to_string()))?;
    let mut result = state
        .storage
        .load_auto_edit_result(&result_id)
        .map_err(|error| AppError::Database(error.to_string()))?;
    let preset = match result.platform_preset.as_str() {
        "tiktok" => crate::video::auto_composer::PlatformPreset::Tiktok,
        "instagram_reels" => crate::video::auto_composer::PlatformPreset::InstagramReels,
        _ => crate::video::auto_composer::PlatformPreset::YoutubeShorts,
    };
    let report = crate::video::OutputValidator::validate(
        std::path::Path::new(&result.output_path),
        preset,
        Some(result.duration),
    )
    .await;
    result.validation = Some(report.clone());
    state
        .storage
        .save_auto_edit_result(&result)
        .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(report)
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

    let mut command = TokioCommand::new(&ffmpeg_path);
    command.args(["-y", "-i"]);
    command.arg(validated_input.as_os_str());
    command.args(["-vf", &scale_filter]);

    for arg in &codec_args {
        command.arg(arg);
    }

    command.arg(output_path.as_os_str());
    tracing::info!(
        "Exporting video: format={}, resolution={}x{}, output={:?}",
        format,
        width,
        height,
        output_path
    );

    emit_export_progress(&app, 40.0);

    if let Err(error) = crate::video::execute_ffmpeg_command(&mut command).await {
        let msg = format!("Export failed: {error}");
        tracing::error!("{}", msg);
        emit_export_error(&app, &msg);
        return Err(msg);
    }

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
                    if let Err(e) = tokio::fs::rename(&normalized, &output_path).await {
                        tracing::warn!("Failed to swap in normalized audio: {}", e);
                        let _ = tokio::fs::remove_file(&normalized).await;
                    }
                }
                Err(e) => {
                    tracing::warn!("Audio normalization skipped (non-fatal): {}", e);
                    let _ = tokio::fs::remove_file(&normalized).await;
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

    fn storyboard_clip(path: &str, start: f64, end: f64, order: u32) -> StoryboardClip {
        StoryboardClip {
            game_id: "game-1".to_string(),
            file_path: path.to_string(),
            order,
            trim_start_secs: start,
            trim_end_secs: end,
        }
    }

    #[test]
    fn shorts_series_partition_preserves_every_selected_second_in_order() {
        let input = vec![
            storyboard_clip("a.mp4", 0.0, 100.0, 0),
            storyboard_clip("b.mp4", 5.0, 105.0, 1),
            storyboard_clip("c.mp4", 0.0, 50.0, 2),
        ];
        let parts = partition_storyboard(input, 180.0).unwrap();
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|part| {
            part.iter()
                .map(|clip| clip.trim_end_secs - clip.trim_start_secs)
                .sum::<f64>()
                <= 180.001
        }));
        let total: f64 = parts
            .iter()
            .flatten()
            .map(|clip| clip.trim_end_secs - clip.trim_start_secs)
            .sum();
        assert!((total - 250.0).abs() < 0.001);
        assert_eq!(parts[0][0].file_path, "a.mp4");
        assert_eq!(parts[1][0].file_path, "b.mp4");
    }

    #[test]
    fn shorts_series_splits_one_long_range_without_loss_or_overlap() {
        let parts =
            partition_storyboard(vec![storyboard_clip("long.mp4", 10.0, 410.0, 0)], 180.0).unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0][0].trim_start_secs, 10.0);
        assert_eq!(parts[0][0].trim_end_secs, 190.0);
        assert_eq!(parts[1][0].trim_start_secs, 190.0);
        assert_eq!(parts[2][0].trim_end_secs, 410.0);
    }

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
