use crate::auth::command_policy::require_command_access;
use crate::auth::middleware::require_auth;
use crate::auth::SubscriptionTier;
use crate::error::{AppError, AppResult};
use crate::storage::{
    thumbnail_offset_secs, thumbnail_output_path, ClipMetadata, ClipVaultPage, ClipVaultPageInput,
    EventData, GameMetadata, StorageError, StorageStats,
};
use crate::utils::security::{self, SafeDeleteOutcome, SecurityError};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

/// Run synchronous SQLite/filesystem work on Tokio's blocking pool.
///
/// The storage layer intentionally owns a single SQLite connection, but Tauri
/// commands are async. Keeping read-heavy commands on the async worker can
/// delay recording/control IPC when a library is large or the disk is busy.
async fn run_storage_blocking<T, F>(
    storage: Arc<crate::storage::Storage>,
    operation: F,
) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&crate::storage::Storage) -> crate::storage::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || operation(&storage))
        .await
        .map_err(|error| AppError::Internal(format!("Storage task panicked: {error}")))?
        .map_err(|error| AppError::Database(error.to_string()))
}

async fn run_storage_app_blocking<T, F>(
    storage: Arc<crate::storage::Storage>,
    operation: F,
) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&crate::storage::Storage) -> AppResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || operation(&storage))
        .await
        .map_err(|error| AppError::Internal(format!("Storage task panicked: {error}")))?
}

/// List all games (sorted by most recent)
#[tauri::command]
pub async fn list_games(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    // FREE tier feature - no authentication required
    run_storage_blocking(state.storage.clone(), |storage| storage.list_games()).await
}

/// Get metadata for a specific game
#[tauri::command]
pub async fn get_game_metadata(
    state: State<'_, AppState>,
    game_id: String,
) -> AppResult<GameMetadata> {
    // FREE tier feature - no authentication required
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.load_game_metadata(&game_id)
    })
    .await
}

/// Save game metadata
#[tauri::command]
pub async fn save_game_metadata(
    state: State<'_, AppState>,
    game_id: String,
    metadata: GameMetadata,
) -> AppResult<()> {
    require_command_access(&state.auth, "save_game_metadata")
        .map_err(|e| AppError::Auth(e.to_string()))?;
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    if metadata.game_id != game_id {
        return Err(AppError::Validation(
            "metadata.game_id must match the command game_id".to_string(),
        ));
    }
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.save_game_metadata(&game_id, &metadata)
    })
    .await
}

/// Load events for a game
#[tauri::command]
pub async fn get_game_events(
    state: State<'_, AppState>,
    game_id: String,
) -> AppResult<Vec<EventData>> {
    // FREE tier feature - no authentication required
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.load_events(&game_id)
    })
    .await
}

/// Save events for a game
#[tauri::command]
pub async fn save_game_events(
    state: State<'_, AppState>,
    game_id: String,
    events: Vec<EventData>,
) -> AppResult<()> {
    require_command_access(&state.auth, "save_game_events")
        .map_err(|e| AppError::Auth(e.to_string()))?;
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.save_events(&game_id, &events)
    })
    .await
}

/// Save clip metadata
#[tauri::command]
pub async fn save_clip_metadata(
    state: State<'_, AppState>,
    game_id: String,
    clip: ClipMetadata,
) -> AppResult<()> {
    require_command_access(&state.auth, "save_clip_metadata")
        .map_err(|e| AppError::Auth(e.to_string()))?;
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.save_clip_metadata(&game_id, &clip)
    })
    .await
}

/// Delete a game and all its data (including video files)
#[tauri::command]
pub async fn delete_game(state: State<'_, AppState>, game_id: String) -> AppResult<()> {
    require_command_access(&state.auth, "delete_game")
        .map_err(|e| AppError::Auth(e.to_string()))?;
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    run_storage_app_blocking(state.storage.clone(), move |storage| {
        // Keep the ownership-checked file deletion and SQLite transaction off
        // the async IPC worker. Large games can contain many clips/thumbnails.
        let clips = storage
            .load_clip_metadata(&game_id)
            .map_err(|e| AppError::Database(e.to_string()))?;

        for clip in &clips {
            safe_delete_game_file(storage, &clip.file_path, "clip")?;
        }
        for clip in &clips {
            if let Some(ref thumb_path) = clip.thumbnail_path {
                safe_delete_game_file(storage, thumb_path, "thumbnail")?;
            }
        }

        storage
            .delete_game(&game_id)
            .map_err(|e| AppError::Database(e.to_string()))?;
        tracing::info!("Successfully deleted game {} and associated files", game_id);
        Ok(())
    })
    .await
}

fn safe_delete_game_file(
    storage: &crate::storage::Storage,
    file_path: &str,
    label: &str,
) -> AppResult<()> {
    match storage.safe_delete_media_file(file_path) {
        Ok(SafeDeleteOutcome::Deleted(path)) => {
            tracing::info!("Deleted game {} file: {:?}", label, path);
            Ok(())
        }
        Ok(SafeDeleteOutcome::Missing(path)) => {
            tracing::warn!("Game {} file already missing: {:?}", label, path);
            Ok(())
        }
        Err(StorageError::Security(SecurityError::DeleteFailed { path, reason })) => {
            // Preserve existing delete_game behavior for transient filesystem
            // failures while still rejecting unsafe metadata paths below.
            tracing::warn!("Failed to delete game {} file {}: {}", label, path, reason);
            Ok(())
        }
        Err(StorageError::Security(err)) => {
            tracing::warn!(
                "Rejected unsafe game {} path from metadata {:?}: {}",
                label,
                file_path,
                err
            );
            Err(AppError::Validation(format!(
                "Unsafe {} path in game metadata: {}",
                label, err
            )))
        }
        Err(err) => {
            tracing::warn!(
                "Failed to delete game {} file {:?}: {}",
                label,
                file_path,
                err
            );
            Ok(())
        }
    }
}

/// Get storage statistics
#[tauri::command]
pub async fn get_storage_stats(state: State<'_, AppState>) -> AppResult<StorageStats> {
    // FREE tier feature - no authentication required
    run_storage_blocking(state.storage.clone(), |storage| storage.get_stats()).await
}

/// List all clips for a specific game
#[tauri::command]
pub async fn list_clips(
    state: State<'_, AppState>,
    game_id: String,
) -> AppResult<Vec<ClipMetadata>> {
    // FREE tier feature - no authentication required
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.load_clip_metadata(&game_id)
    })
    .await
}

/// Return one cursor-paginated page of games that contain usable clip metadata.
#[tauri::command]
pub async fn list_clip_vault_page(
    state: State<'_, AppState>,
    input: ClipVaultPageInput,
) -> AppResult<ClipVaultPage> {
    let game_limit = input.game_limit.unwrap_or(6);
    if !(1..=12).contains(&game_limit) {
        return Err(AppError::Validation(
            "game_limit must be between 1 and 12".to_string(),
        ));
    }
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.list_clip_vault_page(
            input.sort,
            input.cursor.as_deref(),
            game_limit,
            input.query.as_deref(),
            input.game_mode.as_deref(),
        )
    })
    .await
}

/// Generate and persist an app-owned thumbnail for the exact clip row owned by `game_id`.
#[tauri::command]
pub async fn ensure_clip_thumbnail(
    state: State<'_, AppState>,
    game_id: String,
    clip_file_path: String,
) -> AppResult<String> {
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    let input = security::validate_video_input_path(&clip_file_path)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    // The exact `(game_id, file_path)` lookup establishes ownership before ffmpeg writes anything.
    // Keep the synchronous SQLite read off the async runtime worker.
    let clip = run_storage_blocking(state.storage.clone(), {
        let game_id = game_id.clone();
        let clip_file_path = clip_file_path.clone();
        move |storage| storage.load_owned_clip_metadata(&game_id, &clip_file_path)
    })
    .await?;
    let output = thumbnail_output_path(&input).map_err(|e| AppError::Validation(e.to_string()))?;
    let output_string = output.to_string_lossy().to_string();
    if let Err(error) = state
        .video_processor
        .generate_thumbnail(&input, &output, thumbnail_offset_secs(&clip))
        .await
    {
        return Err(AppError::Internal(error.to_string()));
    }
    let persisted_output = output_string.clone();
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.update_owned_clip_thumbnail(&game_id, &clip_file_path, &persisted_output)
    })
    .await?;
    Ok(output_string)
}

// ============================================================================
// Auto-Edit Quota Commands
// ============================================================================

/// Get auto-edit usage and quota information
///
/// Returns current month's usage and remaining quota based on user tier.
#[tauri::command]
pub async fn get_auto_edit_quota(state: State<'_, AppState>) -> AppResult<AutoEditQuotaInfo> {
    // Require authentication to check tier
    let user = require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

    let tier = state
        .auth
        .get_tier()
        .map_err(|e| AppError::Auth(e.to_string()))?;
    let is_pro = matches!(tier, SubscriptionTier::Pro);

    // Load current usage (scoped to this user; see storage::Storage doc on
    // auto_edit_usage_by_user for why this is local-only, non-authoritative)
    let usage = run_storage_blocking(state.storage.clone(), {
        let user_id = user.id.clone();
        move |storage| storage.load_auto_edit_usage(&user_id)
    })
    .await?;

    // Calculate remaining quota
    let limit = if is_pro { u32::MAX } else { 5 };
    let remaining = if is_pro {
        u32::MAX
    } else {
        limit.saturating_sub(usage.usage_count)
    };

    Ok(AutoEditQuotaInfo {
        tier: format!("{:?}", tier),
        is_pro,
        usage: usage.usage_count,
        limit,
        remaining,
        month: usage.month,
    })
}

/// Auto-edit quota information for frontend display
#[derive(Debug, Serialize, Deserialize)]
pub struct AutoEditQuotaInfo {
    /// User's subscription tier (FREE or PRO)
    pub tier: String,

    /// Whether user is PRO tier
    pub is_pro: bool,

    /// Number of auto-edits used this month
    pub usage: u32,

    /// Monthly limit (5 for FREE, u32::MAX for PRO)
    pub limit: u32,

    /// Remaining auto-edits this month
    pub remaining: u32,

    /// Current month (YYYY-MM)
    pub month: String,
}

// ============================================================================
// Auto-Edit Results Commands
// ============================================================================

/// Get all auto-edit results
#[tauri::command]
pub async fn get_auto_edit_results(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::storage::AutoEditResultMetadata>> {
    run_storage_blocking(state.storage.clone(), |storage| {
        storage.load_auto_edit_results()
    })
    .await
}

#[tauri::command]
pub async fn get_auto_edit_result_groups(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::storage::AutoEditResultGroup>> {
    run_storage_blocking(state.storage.clone(), |storage| {
        storage.load_auto_edit_result_groups()
    })
    .await
}

#[tauri::command]
pub async fn delete_auto_edit_result_group(
    state: State<'_, AppState>,
    series_id: String,
    delete_files: bool,
) -> AppResult<()> {
    require_auth(&state.auth).map_err(|error| AppError::Auth(error.to_string()))?;
    security::validate_id(&series_id, 160)
        .map_err(|error| AppError::Validation(error.to_string()))?;
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.delete_auto_edit_result_group(&series_id, delete_files)
    })
    .await
}

/// Get a specific auto-edit result by ID
#[tauri::command]
pub async fn get_auto_edit_result(
    state: State<'_, AppState>,
    result_id: String,
) -> AppResult<crate::storage::AutoEditResultMetadata> {
    security::validate_id(&result_id, 100).map_err(|e| AppError::Validation(e.to_string()))?;
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.load_auto_edit_result(&result_id)
    })
    .await
    .map_err(|error| match error {
        AppError::Database(message) => AppError::NotFound(message),
        other => other,
    })
}

/// Delete an auto-edit result
///
/// If delete_file is true, also deletes the video file and thumbnail.
#[tauri::command]
pub async fn delete_auto_edit_result(
    state: State<'_, AppState>,
    result_id: String,
    delete_file: bool,
) -> AppResult<()> {
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

    security::validate_id(&result_id, 100).map_err(|e| AppError::Validation(e.to_string()))?;
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.delete_auto_edit_result(&result_id, delete_file)
    })
    .await
}

/// Update YouTube upload status for an auto-edit result
#[tauri::command]
pub async fn update_auto_edit_youtube_status(
    state: State<'_, AppState>,
    result_id: String,
    status: crate::storage::YouTubeUploadStatus,
) -> AppResult<()> {
    // Require authentication
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

    security::validate_id(&result_id, 100).map_err(|e| AppError::Validation(e.to_string()))?;
    run_storage_blocking(state.storage.clone(), move |storage| {
        storage.update_auto_edit_youtube_status(&result_id, status)
    })
    .await
}

/// Get dashboard statistics (total games, clips, storage used)
#[tauri::command]
pub async fn get_dashboard_stats(state: State<'_, AppState>) -> AppResult<StorageStats> {
    // FREE tier feature - no authentication required
    //
    // `Storage::get_stats` recursively walks the recordings/exports
    // directory trees on disk (`dir_size_bytes`) synchronously; on a large
    // library that can take long enough to stall the async runtime worker
    // thread it runs on. Move it to a blocking-pool thread instead.
    run_storage_blocking(state.storage.clone(), |storage| storage.get_stats()).await
}
