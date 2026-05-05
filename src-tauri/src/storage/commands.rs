use crate::auth::middleware::require_auth;
use crate::auth::SubscriptionTier;
use crate::error::{AppError, AppResult};
use crate::storage::{ClipMetadata, EventData, GameMetadata, StorageError, StorageStats};
use crate::utils::security::{self, SafeDeleteOutcome, SecurityError};
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

/// List all games (sorted by most recent)
#[tauri::command]
pub async fn list_games(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    // FREE tier feature - no authentication required
    state
        .storage
        .list_games()
        .map_err(|e| AppError::Database(e.to_string()))
}

/// Get metadata for a specific game
#[tauri::command]
pub async fn get_game_metadata(
    state: State<'_, AppState>,
    game_id: String,
) -> AppResult<GameMetadata> {
    // FREE tier feature - no authentication required
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .storage
        .load_game_metadata(&game_id)
        .map_err(|e| AppError::Database(e.to_string()))
}

/// Save game metadata
#[tauri::command]
pub async fn save_game_metadata(
    state: State<'_, AppState>,
    game_id: String,
    metadata: GameMetadata,
) -> AppResult<()> {
    // FREE tier feature - no authentication required
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .storage
        .save_game_metadata(&game_id, &metadata)
        .map_err(|e| AppError::Database(e.to_string()))
}

/// Load events for a game
#[tauri::command]
pub async fn get_game_events(
    state: State<'_, AppState>,
    game_id: String,
) -> AppResult<Vec<EventData>> {
    // FREE tier feature - no authentication required
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .storage
        .load_events(&game_id)
        .map_err(|e| AppError::Database(e.to_string()))
}

/// Save events for a game
#[tauri::command]
pub async fn save_game_events(
    state: State<'_, AppState>,
    game_id: String,
    events: Vec<EventData>,
) -> AppResult<()> {
    // FREE tier feature - no authentication required
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .storage
        .save_events(&game_id, &events)
        .map_err(|e| AppError::Database(e.to_string()))
}

/// Save clip metadata
#[tauri::command]
pub async fn save_clip_metadata(
    state: State<'_, AppState>,
    game_id: String,
    clip: ClipMetadata,
) -> AppResult<()> {
    // FREE tier feature - no authentication required
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .storage
        .save_clip_metadata(&game_id, &clip)
        .map_err(|e| AppError::Database(e.to_string()))
}

/// Delete a game and all its data (including video files)
#[tauri::command]
pub async fn delete_game(state: State<'_, AppState>, game_id: String) -> AppResult<()> {
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    // 1. Load all clip metadata for this game to find physical files
    let clips = state
        .storage
        .load_clip_metadata(&game_id)
        .map_err(|e| AppError::Database(e.to_string()))?;

    // 2. Delete physical video files
    for clip in &clips {
        safe_delete_game_file(&state.storage, &clip.file_path, "clip")?;
    }

    // 3. Delete thumbnail files if they exist
    for clip in &clips {
        if let Some(ref thumb_path) = clip.thumbnail_path {
            safe_delete_game_file(&state.storage, thumb_path, "thumbnail")?;
        }
    }

    // 4. Delete metadata (Game and associated Clips)
    state
        .storage
        .delete_game(&game_id)
        .map_err(|e| AppError::Database(e.to_string()))?;

    tracing::info!("Successfully deleted game {} and associated files", game_id);
    Ok(())
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
    state
        .storage
        .get_stats()
        .map_err(|e| AppError::Database(e.to_string()))
}

/// List all clips for a specific game
#[tauri::command]
pub async fn list_clips(
    state: State<'_, AppState>,
    game_id: String,
) -> AppResult<Vec<ClipMetadata>> {
    // FREE tier feature - no authentication required
    security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .storage
        .load_clip_metadata(&game_id)
        .map_err(|e| AppError::Database(e.to_string()))
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
    require_auth(&state.auth).map_err(|e| AppError::Auth(e.to_string()))?;

    let tier = state
        .auth
        .get_tier()
        .map_err(|e| AppError::Auth(e.to_string()))?;
    let is_pro = matches!(tier, SubscriptionTier::Pro);

    // Load current usage
    let usage = state
        .storage
        .load_auto_edit_usage()
        .map_err(|e| AppError::Database(e.to_string()))?;

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
    state
        .storage
        .load_auto_edit_results()
        .map_err(|e| AppError::Database(e.to_string()))
}

/// Get a specific auto-edit result by ID
#[tauri::command]
pub async fn get_auto_edit_result(
    state: State<'_, AppState>,
    result_id: String,
) -> AppResult<crate::storage::AutoEditResultMetadata> {
    security::validate_id(&result_id, 100).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .storage
        .load_auto_edit_result(&result_id)
        .map_err(|e| AppError::NotFound(e.to_string()))
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
    state
        .storage
        .delete_auto_edit_result(&result_id, delete_file)
        .map_err(|e| AppError::Database(e.to_string()))
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
    state
        .storage
        .update_auto_edit_youtube_status(&result_id, status)
        .map_err(|e| AppError::Database(e.to_string()))
}

/// Get dashboard statistics (total games, clips, storage used)
#[tauri::command]
pub async fn get_dashboard_stats(state: State<'_, AppState>) -> AppResult<StorageStats> {
    // FREE tier feature - no authentication required
    state
        .storage
        .get_stats()
        .map_err(|e| AppError::Database(e.to_string()))
}
