use crate::auth::middleware::{require_auth, require_tier};
use crate::auth::SubscriptionTier;
use crate::error::{AppError, AppResult};
use crate::storage::models::ClipMetadata;
use crate::storage::{CanvasTemplateInfo, StorageError};
use crate::utils::security;
use crate::video::processor::types::{ChainedEffects, ColorGrading, TextPosition, TextStyle};
use crate::video::{AutoEditConfig, AutoEditProgress, AutoEditResult, VideoProcessor};
use crate::AppState;
use std::path::PathBuf;
use std::time::Duration;
use tauri::State;

const GIF_EXPORT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[tauri::command]
pub async fn get_clips(
    state: State<'_, AppState>,
    game_id: String,
) -> AppResult<Vec<ClipMetadata>> {
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

    // Validate game_id (prevent SQL injection)
    let validated_game_id =
        security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .storage
        .load_clip_metadata(&validated_game_id)
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
    // Require PRO tier for manual clip extraction
    require_tier(&state.auth, SubscriptionTier::Pro).map_err(|e| AppError::Auth(e.to_string()))?;

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
    state: State<'_, AppState>,
    clip_paths: Vec<String>,
    output_path: String,
) -> AppResult<String> {
    // Require PRO tier for YouTube Shorts composition
    require_tier(&state.auth, SubscriptionTier::Pro).map_err(|e| AppError::Auth(e.to_string()))?;

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

    // Standard YouTube Shorts resolution: 1080x1920 (9:16)
    let result_path = processor
        .compose_shorts(&validated_clips, validated_output, 1080, 1920)
        .await
        .map_err(|e| AppError::Video(e.to_string()))?;

    Ok(result_path.to_string_lossy().to_string())
}

/// Generate a thumbnail from a video file (PRO feature)
#[tauri::command]
pub async fn generate_thumbnail(
    state: State<'_, AppState>,
    input_path: String,
    output_path: String,
    time_offset: f64,
) -> AppResult<String> {
    // Require PRO tier for custom thumbnail generation
    require_tier(&state.auth, SubscriptionTier::Pro).map_err(|e| AppError::Auth(e.to_string()))?;

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
    // Require authentication (available to both FREE and PRO)
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    state: State<'_, AppState>,
    clip_paths: Vec<String>,
    output_path: String,
) -> AppResult<String> {
    // Require PRO tier? Maybe allow for free users with watermark later.
    // For now, basic merge is a core utility. Let's allow it for authenticated users.
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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

    let result_path = processor
        .compose_montage(&validated_clips, validated_output, false)
        .await
        .map_err(|e| AppError::Video(e.to_string()))?;

    Ok(result_path.to_string_lossy().to_string())
}

/// Start auto-edit composition for YouTube Shorts
#[tauri::command]
pub async fn start_auto_edit(
    state: State<'_, AppState>,
    config: AutoEditConfig,
) -> AppResult<AutoEditResult> {
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

    // Check tier and quota
    let tier = state
        .auth
        .get_tier()
        .map_err(|e| AppError::Auth(e.to_string()))?;
    let is_pro = matches!(tier, SubscriptionTier::Pro);

    // Check quota before starting
    let remaining = state
        .storage
        .check_auto_edit_quota(is_pro)
        .map_err(|e| AppError::Validation(format!("Quota check failed: {}", e)))?;

    tracing::info!(
        "Auto-edit quota check passed: tier={:?}, remaining={}",
        tier,
        if is_pro {
            "unlimited".to_string()
        } else {
            remaining.to_string()
        }
    );

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

    // Save result metadata to storage for the Results tab
    let file_size = std::fs::metadata(&result.output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    let metadata = crate::storage::models::AutoEditResultMetadata {
        result_id: job_id.clone(),
        job_id: job_id.clone(),
        output_path: result.output_path.clone(),
        thumbnail_path: None, // Thumbnail can be generated later or on demand
        created_at: chrono::Utc::now(),
        duration: result.total_duration,
        clip_count: result.clip_count,
        game_ids: config.game_ids.clone(),
        target_duration: config.target_duration,
        canvas_template_name: config.canvas_template.as_ref().map(|t| t.name.clone()),
        has_background_music: config.background_music.is_some(),
        youtube_status: None,
        file_size_bytes: file_size,
    };

    if let Err(e) = state.storage.save_auto_edit_result(&metadata) {
        tracing::error!("Failed to save auto-edit result metadata: {}", e);
        // We don't fail the whole operation if metadata save fails, as the video is already created
    }

    // Increment usage counter on success
    if !is_pro {
        state
            .storage
            .increment_auto_edit_usage()
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

/// Get progress of an auto-edit job
#[tauri::command]
pub async fn get_auto_edit_progress(
    state: State<'_, AppState>,
) -> AppResult<Option<AutoEditProgress>> {
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

    let stats = crate::video::get_global_stats();
    let (total, successful, failed) = stats.get_counts();
    let success_rate = stats.success_rate();

    Ok((total, successful, failed, success_rate))
}

/// Reset all statistics
#[tauri::command]
pub async fn reset_clip_statistics(state: State<'_, AppState>) -> AppResult<()> {
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

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
    state: State<'_, AppState>,
    input: String,
    output: String,
    format: String,
    width: u32,
    height: u32,
) -> Result<String, String> {
    use tokio::process::Command as TokioCommand;

    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()).to_string())?;

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
            let encoder = VideoProcessor::detect_optimal_encoder();
            let mut args: Vec<&str> = encoder.get_ffmpeg_args().to_vec();
            args.extend_from_slice(&["-crf", "23", "-c:a", "aac", "-b:a", "192k"]);
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

    let child = command
        .spawn()
        .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;
    let result = child
        .wait_with_output()
        .await
        .map_err(|e| format!("FFmpeg process error: {}", e))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        tracing::error!("Export failed: {}", stderr);
        return Err(format!(
            "Export failed: {}",
            stderr.chars().take(500).collect::<String>()
        ));
    }

    tracing::info!("Video export completed: {:?}", output_path);
    Ok(output_path.to_string_lossy().to_string())
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
    require_auth(&state.auth).map_err(|e| e.to_string())?;

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
    require_auth(&state.auth).map_err(|e| e.to_string())?;

    // Validate color grading ranges
    if !brightness.is_finite() || brightness < -1.0 || brightness > 1.0 {
        return Err(format!("brightness {} out of range -1.0..1.0", brightness));
    }
    if !contrast.is_finite() || contrast < 0.0 || contrast > 3.0 {
        return Err(format!("contrast {} out of range 0.0..3.0", contrast));
    }
    if !saturation.is_finite() || saturation < 0.0 || saturation > 3.0 {
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
    require_auth(&state.auth).map_err(|e| e.to_string())?;

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
    require_auth(&state.auth).map_err(|e| e.to_string())?;

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
    require_auth(&state.auth).map_err(|e| e.to_string())?;

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
