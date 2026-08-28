use super::{GameInfo, LcuMetrics, MatchInfo, PlayerInfo};
use crate::error::{AppError, AppResult};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn connect_lcu(state: State<'_, AppState>) -> AppResult<bool> {
    // No authentication required - this is a system check
    let mut client = state.lcu_client.lock().await;

    match client.connect().await {
        Ok(()) => Ok(true),
        Err(e) => {
            tracing::debug!(
                "LCU connection attempt failed (League client may not be running): {}",
                e
            );
            // We return false (success with 'false' value) rather than an Error
            // because failing to connect to LCU is a valid state (game not running)
            Ok(false)
        }
    }
}

#[tauri::command]
pub async fn check_lcu_status(state: State<'_, AppState>) -> AppResult<bool> {
    // No authentication required - this is a system check
    let client = state.lcu_client.lock().await;
    Ok(client.is_connected())
}

#[tauri::command]
pub async fn get_current_game(state: State<'_, AppState>) -> AppResult<Option<GameInfo>> {
    // FREE tier feature - no authentication required
    let client = state.lcu_client.lock().await;

    if !client.is_connected() {
        return Err(AppError::Lcu(
            "LCU not connected. Call connect_lcu first.".to_string(),
        ));
    }

    client
        .get_current_game()
        .await
        .map_err(|e| AppError::Lcu(e.to_string()))
}

#[tauri::command]
pub async fn is_in_game(state: State<'_, AppState>) -> AppResult<bool> {
    // FREE tier feature - no authentication required
    let client = state.lcu_client.lock().await;

    if !client.is_connected() {
        return Ok(false);
    }

    client
        .is_in_game()
        .await
        .map_err(|e| AppError::Lcu(e.to_string()))
}

/// Get LCU performance metrics for monitoring
#[tauri::command]
pub async fn get_lcu_metrics(state: State<'_, AppState>) -> AppResult<LcuMetrics> {
    let client = state.lcu_client.lock().await;
    Ok(client.get_performance_metrics().await)
}

/// Force refresh LCU caches (useful for debugging or when external changes detected)
#[tauri::command]
pub async fn refresh_lcu_caches(state: State<'_, AppState>) -> AppResult<()> {
    let client = state.lcu_client.lock().await;
    client
        .refresh_caches()
        .await
        .map_err(|e| AppError::Lcu(e.to_string()))
}

// ========================================================================
// New Commands for Replay & Match History
// ========================================================================

#[tauri::command]
pub async fn list_match_history(
    state: State<'_, AppState>,
    begin_index: u32,
    end_index: u32,
) -> AppResult<Vec<MatchInfo>> {
    if begin_index > end_index || end_index.saturating_sub(begin_index) > 100 {
        return Err(AppError::Validation(
            "Match-history range must be ordered and contain at most 100 matches".to_string(),
        ));
    }
    let client = state.lcu_client.lock().await;
    client
        .list_match_history(begin_index, end_index)
        .await
        .map_err(|e| AppError::Lcu(e.to_string()))
}

#[tauri::command]
pub async fn download_replay(state: State<'_, AppState>, game_id: i64) -> AppResult<()> {
    validate_game_id(game_id)?;
    let client = state.lcu_client.lock().await;
    client
        .download_replay(game_id)
        .await
        .map_err(|e| AppError::Lcu(e.to_string()))
}

#[tauri::command]
pub async fn get_replay_status(state: State<'_, AppState>, game_id: i64) -> AppResult<String> {
    validate_game_id(game_id)?;
    let client = state.lcu_client.lock().await;
    client
        .get_replay_status(game_id)
        .await
        .map_err(|e| AppError::Lcu(e.to_string()))
}

#[tauri::command]
pub async fn launch_replay(state: State<'_, AppState>, game_id: i64) -> AppResult<()> {
    validate_game_id(game_id)?;
    let client = state.lcu_client.lock().await;
    client
        .launch_replay(game_id)
        .await
        .map_err(|e| AppError::Lcu(e.to_string()))
}

fn validate_game_id(game_id: i64) -> AppResult<()> {
    if game_id <= 0 {
        return Err(AppError::Validation(
            "Replay game_id must be a positive integer".to_string(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn get_game_participants(state: State<'_, AppState>) -> AppResult<Vec<PlayerInfo>> {
    let client = state.lcu_client.lock().await;
    client
        .get_game_participants()
        .await
        .map_err(|e| AppError::Lcu(e.to_string()))
}
