use crate::lcu::{LcuClient, LcuError};
use crate::recording::auto_clip_manager::AutoClipManager;
use crate::recording::live_client::{
    check_live_client_basic, EventTrigger, GameEvent, LiveClientBasicInfo, LiveClientMonitor,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Game recording mode
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameMode {
    Live,
    TFT,
    Replay(Option<String>), // Option<String> = Target Summoner Name
}

/// Unified game status exposed to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedGameStatus {
    /// Whether LCU (League Client) is connected
    pub lcu_connected: bool,
    /// Whether currently in a game (detected via Live Client API - most reliable)
    pub in_game: bool,
    /// Current game mode
    pub game_mode: GameMode,
    /// Player's summoner name (if in game)
    pub summoner_name: Option<String>,
    /// Current champion name (if in game)
    pub champion_name: Option<String>,
    /// Current game time in seconds
    pub game_time: Option<f32>,
    /// Whether monitoring is active
    pub is_monitoring: bool,
    /// Whether recording is active
    pub is_recording: bool,
    /// Number of clips saved during the current game session. Populated from
    /// AutoClipManager so the game-end notification can report a real count
    /// instead of a hardcoded 0.
    #[serde(default)]
    pub session_clip_count: usize,
}

/// Game state monitor for automatic recording
pub struct GameStateMonitor {
    lcu_client: Arc<RwLock<LcuClient>>,
    live_client: Arc<RwLock<Option<LiveClientMonitor>>>,
    auto_clip_manager: Arc<AutoClipManager>,
    is_monitoring: Arc<RwLock<bool>>,
    last_game_state: Arc<RwLock<bool>>, // true if in game, false if not
    game_mode: Arc<RwLock<GameMode>>,
    /// Unified game status for frontend consumption
    unified_status: Arc<RwLock<UnifiedGameStatus>>,
    /// Handle for the live client monitoring task (spawned per game session)
    live_monitor_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

fn live_client_mode_hint(data: Option<&LiveClientBasicInfo>) -> Option<GameMode> {
    data.filter(|info| info.game_mode.contains("TFT"))
        .map(|_| GameMode::TFT)
}

impl GameStateMonitor {
    pub fn new(auto_clip_manager: Arc<AutoClipManager>) -> Self {
        Self {
            lcu_client: Arc::new(RwLock::new(LcuClient::new())),
            live_client: Arc::new(RwLock::new(None)),
            auto_clip_manager,
            is_monitoring: Arc::new(RwLock::new(false)),
            last_game_state: Arc::new(RwLock::new(false)),
            game_mode: Arc::new(RwLock::new(GameMode::Live)),
            live_monitor_handle: Arc::new(RwLock::new(None)),
            unified_status: Arc::new(RwLock::new(UnifiedGameStatus {
                lcu_connected: false,
                in_game: false,
                game_mode: GameMode::Live,
                summoner_name: None,
                champion_name: None,
                game_time: None,
                is_monitoring: false,
                is_recording: false,
                session_clip_count: 0,
            })),
        }
    }

    /// Get current unified game status for frontend
    pub async fn get_unified_status(&self) -> UnifiedGameStatus {
        self.unified_status.read().await.clone()
    }

    /// Set target summoner for replay recording
    pub async fn set_replay_target(&self, summoner_name: Option<String>) {
        let mut mode = self.game_mode.write().await;
        if let GameMode::Replay(_) = *mode {
            *mode = GameMode::Replay(summoner_name.clone());
            info!("Replay target set to: {:?}", summoner_name);
        } else {
            // Force switch to replay mode if setting target
            // Ideally this should only happen when game flow confirms replay
            warn!("Setting replay target while not in Replay mode (switching to Replay mode)");
            *mode = GameMode::Replay(summoner_name.clone());
        }
    }

    /// Get the currently selected replay target, if replay mode is active.
    pub async fn get_replay_target(&self) -> Option<String> {
        match &*self.game_mode.read().await {
            GameMode::Replay(target) => target.clone(),
            _ => None,
        }
    }

    /// Start monitoring game state
    pub async fn start_monitoring<F1, F2, Fut1, Fut2>(
        &self,
        on_game_start: F1,
        on_game_end: F2,
    ) -> Result<(), LcuError>
    where
        F1: Fn() -> Fut1 + Send + Sync + 'static + Clone,
        F2: Fn() -> Fut2 + Send + Sync + 'static + Clone,
        Fut1: std::future::Future<Output = Result<(), String>> + Send + 'static,
        Fut2: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        let mut is_monitoring = self.is_monitoring.write().await;

        if *is_monitoring {
            warn!("Game state monitoring is already running");
            return Ok(());
        }

        *is_monitoring = true;

        // Update unified status
        {
            let mut status = self.unified_status.write().await;
            status.is_monitoring = true;
        }

        info!("Starting game state monitoring");

        let lcu_client = Arc::clone(&self.lcu_client);
        let live_client = Arc::clone(&self.live_client);
        let is_monitoring_arc = Arc::clone(&self.is_monitoring);
        let last_game_state_arc = Arc::clone(&self.last_game_state);
        let auto_clip_manager = Arc::clone(&self.auto_clip_manager);
        let game_mode_arc = Arc::clone(&self.game_mode);
        let unified_status_arc = Arc::clone(&self.unified_status);
        let live_monitor_handle_arc = Arc::clone(&self.live_monitor_handle);

        // Start monitoring task
        tokio::spawn(async move {
            let mut retry_count = 0;
            const MAX_RETRIES: u32 = 5;
            const BASE_RETRY_DELAY: Duration = Duration::from_secs(2);
            const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
            const CHECK_INTERVAL: Duration = Duration::from_secs(1); // Check every 1 second for faster detection

            // Helper function for exponential backoff
            fn calculate_retry_delay(retry_count: u32) -> Duration {
                let base_delay_ms = BASE_RETRY_DELAY.as_millis() as u64;
                let max_delay_ms = MAX_RETRY_DELAY.as_millis() as u64;
                let exponential_delay = base_delay_ms * (2_u64.pow(retry_count.min(6)));
                let delay = exponential_delay.min(max_delay_ms);

                Duration::from_millis(delay)
            }

            loop {
                // Check if monitoring should stop
                {
                    let monitoring = is_monitoring_arc.read().await;
                    if !*monitoring {
                        info!("Game state monitoring stopped");
                        break;
                    }
                }

                // Try to connect to LCU if not connected
                // FIX #5: Drop write lock before network calls to avoid holding it too long
                let lcu_connected = {
                    let client = lcu_client.read().await;
                    client.is_connected()
                };

                if !lcu_connected {
                    let mut client = lcu_client.write().await;
                    #[allow(unused_assignments)]
                    match client.connect().await {
                        Ok(()) => {
                            info!("Successfully connected to League client");
                            retry_count = 0; // Reset for next potential failure
                        }
                        Err(e) => {
                            drop(client); // Drop write lock before sleeping
                            retry_count += 1;
                            debug!(
                                "Failed to connect to League client (attempt {}): {}",
                                retry_count, e
                            );

                            if retry_count >= MAX_RETRIES {
                                let delay = calculate_retry_delay(retry_count);
                                warn!(
                                    "Max retries reached, waiting {:.1} seconds before retry",
                                    delay.as_secs_f64()
                                );
                                tokio::time::sleep(delay).await;
                                retry_count = 0; // Reset after max retries
                            } else {
                                let delay = calculate_retry_delay(retry_count);
                                debug!("Waiting {:.1} seconds before retry", delay.as_secs_f64());
                                tokio::time::sleep(delay).await;
                            }
                            continue;
                        }
                    }
                }

                // Update LCU connection status
                {
                    let mut status = unified_status_arc.write().await;
                    let client = lcu_client.read().await;
                    status.lcu_connected = client.is_connected();
                }

                // Check game state using HYBRID detection:
                // 1. Try Live Client API first (most reliable - works for practice mode, custom games)
                // 2. Fall back to LCU gameflow-phase

                let (in_game, live_client_data) = {
                    // Try Live Client API (port 2999) - this is the most reliable method
                    let live_api_check = check_live_client_basic().await;

                    if let Some(data) = live_api_check {
                        debug!(
                            "✅ Live Client API detected game: player={}",
                            data.summoner_name
                        );
                        (true, Some(data))
                    } else {
                        // Fall back to LCU gameflow-phase (use read lock for query)
                        let client = lcu_client.read().await;
                        match client.get_game_session().await {
                            Ok(session) => {
                                use crate::lcu::GameFlowPhase;
                                let lcu_in_game = matches!(
                                    session.phase,
                                    GameFlowPhase::InProgress | GameFlowPhase::Reconnect
                                );
                                debug!("LCU API returned: in_game = {}", lcu_in_game);
                                (lcu_in_game, None)
                            }
                            Err(_) => (false, None),
                        }
                    }
                };

                // Update unified status with game info
                {
                    let mut status = unified_status_arc.write().await;
                    status.in_game = in_game;
                    if let Some(ref data) = live_client_data {
                        status.summoner_name = Some(data.summoner_name.clone());
                        status.champion_name = Some(data.champion_name.clone());
                        status.game_time = Some(data.game_time);
                    } else if !in_game {
                        status.summoner_name = None;
                        status.champion_name = None;
                        status.game_time = None;
                    }
                }

                let mut last_state = last_game_state_arc.write().await;

                if in_game && !*last_state {
                    // Game started - initialize Live Client Monitor
                    info!("🎮 Game detected! Starting automatic recording...");

                    // Start Live Client Monitor for event collection
                    let mut live_client_guard = live_client.write().await;
                    let auto_clip_manager_clone = Arc::clone(&auto_clip_manager);
                    let game_mode_clone = Arc::clone(&game_mode_arc);
                    let unified_status_clone = Arc::clone(&unified_status_arc);

                    // Task 30: build the monitor from user settings so contest_window_secs applies.
                    let event_config = auto_clip_manager.event_stream_config().await;
                    match LiveClientMonitor::with_config(
                        event_config,
                        auto_clip_manager.summary_slot(),
                    ) {
                        Ok(monitor) => {
                            let detected_mode =
                                match live_client_mode_hint(live_client_data.as_ref()) {
                                    Some(GameMode::TFT) => {
                                        info!("🎯 TFT (팀파이트 택틱스) 모드 감지됨");
                                        GameMode::TFT
                                    }
                                    _ => {
                                        // Auto-detect if this is a replay or live game.
                                        match monitor.detect_replay_mode().await {
                                            Some(true) => {
                                                info!("🎬 Replay mode detected automatically");
                                                GameMode::Replay(None)
                                            }
                                            Some(false) => {
                                                info!("🎮 Live game mode detected");
                                                GameMode::Live
                                            }
                                            None => {
                                                info!(
                                                "⚠️ Could not detect game mode, defaulting to Live"
                                            );
                                                GameMode::Live
                                            }
                                        }
                                    }
                                };

                            // Update game mode
                            {
                                let mut mode = game_mode_arc.write().await;
                                *mode = detected_mode.clone();
                            }

                            // Update unified status
                            {
                                let mut status = unified_status_arc.write().await;
                                status.game_mode = detected_mode.clone();
                                status.is_recording = false;
                            }

                            // Set game mode on clip manager for filtering
                            if let Some(ref data) = live_client_data {
                                auto_clip_manager
                                    .set_game_mode(data.game_mode.clone(), None)
                                    .await;
                            }

                            // Start FFmpeg recording BEFORE event monitoring
                            if let Err(e) = on_game_start().await {
                                // FIX: distinguish manual-capture preemption from a real failure.
                                // If recording is already active, the user started capture manually
                                // (F8 / start command) before auto-detect fired. Adopt that session
                                // instead of retrying start_recording every second (log spam) and
                                // never running end-of-game cleanup.
                                if auto_clip_manager.is_recording().await {
                                    info!(
                                        "Recording already active (manual capture) — adopting session ({})",
                                        e
                                    );
                                    {
                                        let mut status = unified_status_arc.write().await;
                                        status.is_recording = true;
                                    }
                                    // Ensure event monitoring runs exactly once. If the manual
                                    // path already started it, this is a no-op (guarded in
                                    // start_event_monitoring), preventing duplicate event streams.
                                    if !auto_clip_manager.is_monitoring().await {
                                        if let Err(mon_err) =
                                            auto_clip_manager.start_event_monitoring().await
                                        {
                                            warn!(
                                                "Failed to start event monitoring while adopting manual session: {}",
                                                mon_err
                                            );
                                        }
                                    }
                                    // Transition to in-game so the end edge runs cleanup once.
                                    *last_state = true;
                                    retry_count = 0;
                                    tokio::time::sleep(CHECK_INTERVAL).await;
                                    continue;
                                }

                                error!("Failed to start recording on game start: {}", e);
                                auto_clip_manager.set_game_mode(String::new(), None).await;
                                {
                                    let mut status = unified_status_arc.write().await;
                                    status.is_recording = false;
                                }
                                *last_state = false;
                                retry_count = 0;
                                tokio::time::sleep(CHECK_INTERVAL).await;
                                continue;
                            }

                            {
                                let mut status = unified_status_arc.write().await;
                                status.is_recording = true;
                            }

                            // FIX #1: Spawn monitoring in tokio::spawn so it doesn't block
                            // the polling loop. Store the JoinHandle to abort on game end.
                            let mut monitor = monitor;
                            let monitor_handle: JoinHandle<()> = tokio::spawn(async move {
                                if let Err(e) = monitor.start_monitoring(
                                    move |trigger: EventTrigger, event: GameEvent| {
                                        let current_mode = game_mode_clone.clone();

                                        info!("🎯 Game event detected: {:?}", trigger);

                                        let auto_clip_manager = Arc::clone(&auto_clip_manager_clone);
                                        let unified_status = Arc::clone(&unified_status_clone);
                                        tokio::spawn(async move {
                                            let mode = current_mode.read().await;
                                            let should_record = match &*mode {
                                                GameMode::Live => true,
                                                GameMode::TFT => false, // TFT: no event-based auto-clip
                                                GameMode::Replay(target) => {
                                                    if target.is_some() {
                                                        true
                                                    } else {
                                                        warn!("Replay event ignored: No target selected");
                                                        false
                                                    }
                                                }
                                            };

                                            if should_record {
                                                info!("🎥 Recording event for target");
                                                if let Err(e) = auto_clip_manager.handle_game_event(trigger, event).await {
                                                    error!("Failed to handle game event: {}", e);
                                                }
                                            }

                                            // Keep unified status updated with recording state
                                            let mut status = unified_status.write().await;
                                            status.is_recording = true;
                                        });
                                    }
                                ).await {
                                    warn!("Live Client monitoring ended: {}", e);
                                }
                            });

                            // Store the handle so we can abort it on game end
                            {
                                let mut handle_guard = live_monitor_handle_arc.write().await;
                                *handle_guard = Some(monitor_handle);
                            }
                            info!("✅ Live Client API monitoring spawned successfully");
                            *live_client_guard = None; // monitor moved into spawn
                        }
                        Err(e) => {
                            warn!("Failed to initialize Live Client Monitor: {}", e);
                        }
                    }
                } else if !in_game && *last_state {
                    // Game ended - stop Live Client Monitor
                    info!("⏹️ Game ended. Stopping automatic recording...");

                    // FIX #1: Abort the spawned monitoring task before game end cleanup
                    {
                        let mut handle_guard = live_monitor_handle_arc.write().await;
                        if let Some(handle) = handle_guard.take() {
                            handle.abort();
                            info!("Live Client monitoring task aborted");
                        }
                    }

                    // Stop Live Client Monitor
                    let mut live_client_guard = live_client.write().await;
                    *live_client_guard = None;
                    info!("Live Client API disconnected");

                    // Reset Game Mode to Live default
                    {
                        let mut mode = game_mode_arc.write().await;
                        *mode = GameMode::Live;
                    }

                    // Reset game mode on clip manager
                    auto_clip_manager.set_game_mode(String::new(), None).await;

                    // Reset unified status
                    {
                        let mut status = unified_status_arc.write().await;
                        status.in_game = false;
                        status.game_mode = GameMode::Live;
                        status.summoner_name = None;
                        status.champion_name = None;
                        status.game_time = None;
                        status.is_recording = false;
                    }

                    if let Err(e) = on_game_end().await {
                        error!("Failed to stop recording on game end: {}", e);
                    }
                }

                *last_state = in_game;
                retry_count = 0;

                // Wait before next check
                tokio::time::sleep(CHECK_INTERVAL).await;
            }
        });

        Ok(())
    }

    /// Stop monitoring game state
    pub async fn stop_monitoring(&self) -> Result<(), LcuError> {
        let mut is_monitoring = self.is_monitoring.write().await;
        *is_monitoring = false;
        info!("Stopping game state monitoring");
        Ok(())
    }

    /// Check if monitoring is active
    pub async fn is_monitoring_active(&self) -> bool {
        *self.is_monitoring.read().await
    }

    /// Get current game state
    pub async fn get_current_game_state(&self) -> Result<bool, LcuError> {
        let client = self.lcu_client.read().await;
        client.is_in_game().await
    }

    /// Force refresh connection to League client
    pub async fn refresh_connection(&self) -> Result<(), LcuError> {
        let mut client = self.lcu_client.write().await;
        *client = LcuClient::new();
        client.connect().await
    }
}

// Default implementation removed since AutoClipManager is required

#[cfg(test)]
mod tests {
    use super::*;

    fn live_info(game_mode: &str) -> LiveClientBasicInfo {
        LiveClientBasicInfo {
            summoner_name: "tester".to_string(),
            champion_name: "Ahri".to_string(),
            game_time: 10.0,
            game_mode: game_mode.to_string(),
        }
    }

    #[test]
    fn live_client_mode_hint_detects_tft_without_extra_probe() {
        assert_eq!(
            live_client_mode_hint(Some(&live_info("TFT"))),
            Some(GameMode::TFT)
        );
        assert_eq!(
            live_client_mode_hint(Some(&live_info("TFT_DOUBLE_UP"))),
            Some(GameMode::TFT)
        );
    }

    #[test]
    fn live_client_mode_hint_leaves_non_tft_for_replay_detection() {
        assert_eq!(live_client_mode_hint(Some(&live_info("CLASSIC"))), None);
        assert_eq!(live_client_mode_hint(None), None);
    }
}
