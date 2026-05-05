#![allow(clippy::unnecessary_cast)]
use anyhow::{Context as AnyhowContext, Result};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::error::AppError;

use super::integration_backend::WindowsCaptureRecorder;
use super::live_client::{EventTrigger, GameEvent, LiveClientMonitor};
use crate::settings::models::RecordingSettings;
use crate::storage::{
    models::{ClipMetadata, EventData, EventType},
    Storage,
};

/// Queued event with timestamp for merging logic
#[derive(Debug, Clone)]
struct QueuedEvent {
    trigger: EventTrigger,
    event: GameEvent,
    received_at: Instant,
}

/// Event window after merging consecutive events
#[derive(Debug, Clone)]
struct EventWindow {
    primary_trigger: EventTrigger,
    events: Vec<GameEvent>,
    start_time: f32, // Game time in seconds
    end_time: f32,   // Game time in seconds
    priority: u8,    // Highest priority in window
}

/// Auto Clip Manager - Bridges event detection with automatic clip saving
///
/// Architecture:
/// LiveClientMonitor → AutoClipManager → WindowsRecorder + Storage
///                           ↓
///                      Settings (filter)
///
/// Responsibilities:
/// 1. Event Queue: Receive events from LiveClientMonitor
/// 2. Event Filtering: Apply settings filters (event types, priority, game modes)
/// 3. Event Merging: Combine consecutive events within threshold
/// 4. Clip Window Calculation: Calculate pre/post durations from settings or defaults
/// 5. Event Processing: Handle events and prepare for clip extraction
/// 6. Metadata Generation: Create rich metadata for each saved clip
pub struct AutoClipManager {
    /// Recording backend reference
    recorder: Arc<TokioRwLock<WindowsCaptureRecorder>>,

    /// Storage reference
    storage: Arc<Storage>,

    /// Settings reference
    settings: Arc<TokioRwLock<RecordingSettings>>,

    /// Event queue for merging
    event_queue: Arc<TokioMutex<VecDeque<QueuedEvent>>>,

    /// Current game ID for clip organization
    current_game_id: Arc<TokioRwLock<Option<String>>>,

    /// Processing lock to prevent concurrent clip saves
    processing_lock: Arc<TokioMutex<()>>,

    /// Event monitoring task handle
    monitor_task: Arc<TokioMutex<Option<JoinHandle<()>>>>,

    /// Cancellation token for stopping the monitoring task.
    /// Wrapped in a mutex so we can replace it with a fresh token after cancellation.
    cancel_token: Arc<TokioMutex<CancellationToken>>,

    /// Current game mode string (e.g. "CLASSIC", "ARAM")
    current_game_mode: Arc<TokioRwLock<String>>,

    /// Current queue ID (e.g. 420 = ranked solo, 440 = ranked flex)
    current_queue_id: Arc<TokioRwLock<Option<u32>>>,
}

impl AutoClipManager {
    /// Create a new Auto Clip Manager
    pub fn new(
        recorder: Arc<TokioRwLock<WindowsCaptureRecorder>>,
        storage: Arc<Storage>,
        settings: Arc<TokioRwLock<RecordingSettings>>,
    ) -> Self {
        Self {
            recorder,
            storage,
            settings,
            event_queue: Arc::new(TokioMutex::new(VecDeque::new())),
            current_game_id: Arc::new(TokioRwLock::new(None)),
            processing_lock: Arc::new(TokioMutex::new(())),
            monitor_task: Arc::new(TokioMutex::new(None)),
            cancel_token: Arc::new(TokioMutex::new(CancellationToken::new())),
            current_game_mode: Arc::new(TokioRwLock::new(String::new())),
            current_queue_id: Arc::new(TokioRwLock::new(None)),
        }
    }

    /// Set the current game mode and queue ID for filtering
    pub async fn set_game_mode(&self, game_mode: String, queue_id: Option<u32>) {
        let mut mode = self.current_game_mode.write().await;
        *mode = game_mode.clone();
        let mut qid = self.current_queue_id.write().await;
        *qid = queue_id;
        info!(
            "Auto Clip Manager: game mode set to {} (queue: {:?})",
            game_mode, queue_id
        );
    }

    /// Set the current game ID for clip organization
    pub async fn set_current_game(&self, game_id: Option<String>) {
        let mut current = self.current_game_id.write().await;
        *current = game_id.clone();

        if let Some(ref id) = game_id {
            info!("Auto Clip Manager: tracking game {}", id);
        } else {
            info!("Auto Clip Manager: game ended, clearing queue");
            // Clear event queue when game ends
            let mut queue = self.event_queue.lock().await;
            queue.clear();
        }
    }

    /// Check if event monitoring is active
    pub async fn is_monitoring(&self) -> bool {
        let task_guard = self.monitor_task.lock().await;
        task_guard.is_some()
    }

    /// Start event monitoring from Live Client API
    ///
    /// This spawns a background task that continuously polls the Live Client API
    /// for game events and automatically processes them through the clip pipeline.
    pub async fn start_event_monitoring(&self) -> Result<()> {
        // Check if already monitoring
        let mut task_guard = self.monitor_task.lock().await;
        if task_guard.is_some() {
            info!("Event monitoring already running");
            return Ok(());
        }

        info!("Starting event monitoring...");

        // Create a new LiveClientMonitor
        let mut monitor = LiveClientMonitor::new().context("Failed to create LiveClientMonitor")?;

        // Clone Arc references for the monitoring task
        let event_queue = Arc::clone(&self.event_queue);
        let settings = Arc::clone(&self.settings);
        let recorder = Arc::clone(&self.recorder);
        let storage = Arc::clone(&self.storage);
        let current_game_id = Arc::clone(&self.current_game_id);
        let processing_lock = Arc::clone(&self.processing_lock);
        let current_game_mode = Arc::clone(&self.current_game_mode);
        let current_queue_id = Arc::clone(&self.current_queue_id);
        // FIX #6: Create a fresh cancellation token for each monitoring session
        // so that a previous cancel() doesn't keep the new session cancelled.
        let cancel_token = {
            let mut token_guard = self.cancel_token.lock().await;
            *token_guard = CancellationToken::new();
            token_guard.clone()
        };

        // Spawn monitoring task
        let handle = tokio::spawn(async move {
            info!("Event monitoring task started");

            // Create callback closure that processes events
            let callback =
                move |trigger: EventTrigger, live_event: super::live_client::GameEvent| {
                    // Convert live_client::GameEvent to recording::GameEvent
                    let event = convert_live_event(live_event, &trigger);

                    // Clone Arc references for the async block
                    let event_queue = Arc::clone(&event_queue);
                    let settings = Arc::clone(&settings);
                    let recorder = Arc::clone(&recorder);
                    let storage = Arc::clone(&storage);
                    let current_game_id = Arc::clone(&current_game_id);
                    let processing_lock = Arc::clone(&processing_lock);
                    let current_game_mode = Arc::clone(&current_game_mode);
                    let current_queue_id = Arc::clone(&current_queue_id);

                    // Spawn a task to process the event asynchronously
                    tokio::spawn(async move {
                        // Create a temporary AutoClipManager instance for processing
                        let temp_manager = AutoClipManager {
                            recorder,
                            storage,
                            settings,
                            event_queue,
                            current_game_id,
                            processing_lock,
                            monitor_task: Arc::new(TokioMutex::new(None)),
                            cancel_token: Arc::new(TokioMutex::new(CancellationToken::new())),
                            current_game_mode,
                            current_queue_id,
                        };

                        let trigger_display = format!("{:?}", trigger);
                        if let Err(e) = temp_manager.process_event(trigger, event).await {
                            error!("Failed to process event {}: {}", trigger_display, e);
                        }
                    });
                };

            // Run the monitor until cancelled
            let monitoring = monitor.start_monitoring(callback);
            tokio::select! {
                result = monitoring => {
                    if let Err(e) = result {
                        error!("Event monitoring error: {}", e);
                    }
                }
                _ = cancel_token.cancelled() => {
                    info!("Event monitoring cancelled");
                }
            }

            info!("Event monitoring task stopped");
        });

        *task_guard = Some(handle);
        info!("Event monitoring started successfully");

        Ok(())
    }

    /// Stop event monitoring
    ///
    /// This cancels the background monitoring task and waits for it to finish.
    pub async fn stop_event_monitoring(&self) -> Result<()> {
        info!("Stopping event monitoring...");

        // Cancel the monitoring task
        {
            let token = self.cancel_token.lock().await;
            token.cancel();
        }

        // Get and wait for the task to finish
        let mut task_guard = self.monitor_task.lock().await;
        if let Some(handle) = task_guard.take() {
            handle.await.context("Failed to join monitoring task")?;
            info!("Event monitoring stopped successfully");
        } else {
            info!("Event monitoring was not running");
        }

        Ok(())
    }

    /// Handle a game event from Live Client API
    ///
    /// This is the public interface called by GameMonitor.
    /// Converts the event and processes it through the clip pipeline.
    pub async fn handle_game_event(
        &self,
        trigger: EventTrigger,
        event: super::live_client::GameEvent,
    ) -> Result<()> {
        debug!(
            "Auto Clip Manager: handling live event {} (priority: {})",
            event.event_name,
            trigger.priority()
        );

        // Convert live_client::GameEvent to recording::GameEvent
        let recording_event = convert_live_event(event, &trigger);

        // Process the converted event
        self.process_event(trigger, recording_event).await
    }

    /// Process an event from LiveClientMonitor
    ///
    /// This is the main entry point called by the event detection callback.
    /// Events are filtered, queued, merged, and automatically saved.
    pub async fn process_event(&self, trigger: EventTrigger, event: GameEvent) -> Result<()> {
        debug!(
            "Auto Clip Manager: processing event {} (priority: {})",
            event.event_name,
            trigger.priority()
        );

        // Check if we should record this event based on settings
        if !self.should_record_event(&trigger, &event).await? {
            debug!(
                "Event filtered out by settings: {} (priority: {})",
                event.event_name,
                trigger.priority()
            );
            return Ok(());
        }

        // Add event to queue
        let queued = QueuedEvent {
            trigger: trigger.clone(),
            event: event.clone(),
            received_at: Instant::now(),
        };

        {
            const MAX_QUEUE_SIZE: usize = 1000;
            let mut queue =
                tokio::time::timeout(std::time::Duration::from_secs(5), self.event_queue.lock())
                    .await
                    .map_err(|_| {
                        tracing::error!("event_queue lock acquisition timed out after 5s");
                        anyhow::anyhow!(AppError::ProcessTimeout(
                            "event_queue lock acquisition timed out".into()
                        ))
                    })?;

            // Enforce queue size limit to prevent memory growth
            // Use while loop to ensure we stay under limit even after push
            let overflow_count = queue.len().saturating_sub(MAX_QUEUE_SIZE - 1);
            if overflow_count > 0 {
                warn!(
                    "Event queue overflow ({} events), dropping {} oldest events",
                    queue.len(),
                    overflow_count
                );
                for _ in 0..overflow_count {
                    queue.pop_front();
                }
            }

            queue.push_back(queued);
            debug_assert!(
                queue.len() <= MAX_QUEUE_SIZE,
                "Queue size invariant violated"
            );
        }

        // Check if we should merge events or save immediately
        let settings = self.settings.read().await;

        if settings.clip_timing.merge_consecutive_events {
            // Wait for merge window to close before processing
            self.try_process_merged_events().await?;
        } else {
            // Save immediately without merging
            self.save_single_event(trigger, event).await?;
        }

        Ok(())
    }

    /// Check if event should be recorded based on settings
    async fn should_record_event(
        &self,
        trigger: &EventTrigger,
        _event: &GameEvent,
    ) -> Result<bool> {
        let settings = self.settings.read().await;

        // Check game mode filtering
        let game_mode = self.current_game_mode.read().await;
        let queue_id = self.current_queue_id.read().await;
        if !game_mode.is_empty() {
            let mode_settings = &settings.game_mode;
            let mode_allowed = match game_mode.as_str() {
                "CLASSIC" => match *queue_id {
                    Some(420) => mode_settings.record_ranked_solo,
                    Some(440) => mode_settings.record_ranked_flex,
                    Some(430) => mode_settings.record_normal,
                    Some(400) => mode_settings.record_normal,
                    Some(490) => mode_settings.record_quick_play,
                    _ => true,
                },
                "ARAM" => mode_settings.record_aram,
                "URF" | "ARURF" => mode_settings.record_special,
                _ => true,
            };
            if !mode_allowed {
                return Ok(false);
            }
        }
        drop(game_mode);
        drop(queue_id);

        // Check priority threshold
        let event_priority = trigger.priority();
        if event_priority < settings.event_filter.min_priority {
            return Ok(false);
        }

        // Check event type filters
        let should_record = match trigger {
            EventTrigger::ChampionKill => settings.event_filter.record_kills,
            EventTrigger::Death => settings.event_filter.record_deaths,
            EventTrigger::Assist => settings.event_filter.record_assists,
            EventTrigger::FirstBlood => settings.event_filter.record_first_blood,
            EventTrigger::Multikill(_) => settings.event_filter.record_multikills,
            EventTrigger::DragonKill => settings.event_filter.record_dragon,
            EventTrigger::BaronKill => settings.event_filter.record_baron,
            EventTrigger::HeraldKill => settings.event_filter.record_herald,
            EventTrigger::TurretKill => settings.event_filter.record_turret,
            EventTrigger::InhibitorKill => settings.event_filter.record_inhibitor,
            EventTrigger::Ace => settings.event_filter.record_ace,
            EventTrigger::Steal => settings.event_filter.record_steal,
            EventTrigger::GameEnd => settings.event_filter.record_game_end,
            EventTrigger::ElderDragonKill => settings.event_filter.record_elder,
            EventTrigger::VoidgrubsKill => settings.event_filter.record_voidgrubs,
            EventTrigger::AtakhanKill => settings.event_filter.record_atakhan,
            EventTrigger::Shutdown => settings.event_filter.record_shutdown,
            EventTrigger::Outplay1vX(_) => settings.event_filter.record_outplay,
            EventTrigger::TradeKill => settings.event_filter.record_trade_kill,
            EventTrigger::LowHpOutplay => settings.event_filter.record_low_hp,
        };

        Ok(should_record)
    }

    /// Try to process merged events if merge window has closed
    async fn try_process_merged_events(&self) -> Result<()> {
        let settings = self.settings.read().await;
        let merge_threshold = settings.clip_timing.merge_time_threshold as u64;
        drop(settings);

        let mut queue = self.event_queue.lock().await;

        // Check if oldest event is outside merge window
        if let Some(oldest) = queue.front() {
            let age = oldest.received_at.elapsed().as_secs();

            if age >= merge_threshold {
                // Merge window closed - process events
                let events_to_merge: Vec<QueuedEvent> = queue.drain(..).collect();
                drop(queue);

                if !events_to_merge.is_empty() {
                    self.process_event_window(events_to_merge).await?;
                }
            }
        }

        Ok(())
    }

    /// Process a window of merged events
    async fn process_event_window(&self, events: Vec<QueuedEvent>) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        // Create event window (safely handles empty/invalid events)
        let window = match self.merge_events(&events) {
            Some(w) => w,
            None => {
                tracing::warn!("Failed to merge {} events - skipping window", events.len());
                return Ok(());
            }
        };

        info!(
            "Merged {} events into window: {:?} (priority: {}, duration: {:.1}s)",
            events.len(),
            window.primary_trigger,
            window.priority,
            window.end_time - window.start_time
        );

        // Save the merged clip
        self.save_event_window(window).await?;

        Ok(())
    }

    /// Merge consecutive events into a single window
    /// Returns None if events is empty or contains invalid data
    fn merge_events(&self, events: &[QueuedEvent]) -> Option<EventWindow> {
        // Guard against empty events
        if events.is_empty() {
            tracing::warn!("merge_events called with empty events list");
            return None;
        }

        // Find highest priority event safely
        let primary_event = events.iter().max_by_key(|e| e.trigger.priority())?;
        let priority = primary_event.trigger.priority();

        // Calculate time range with NaN-safe comparison
        let start_time = events
            .iter()
            .map(|e| e.event.event_time)
            .filter(|t| t.is_finite())
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;

        let end_time = events
            .iter()
            .map(|e| e.event.event_time)
            .filter(|t| t.is_finite())
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;

        Some(EventWindow {
            primary_trigger: primary_event.trigger.clone(),
            events: events.iter().map(|e| e.event.clone()).collect(),
            start_time: start_time as f32,
            end_time: end_time as f32,
            priority,
        })
    }

    /// Save a single event without merging
    async fn save_single_event(&self, trigger: EventTrigger, event: GameEvent) -> Result<()> {
        // Prevent concurrent saves
        let _lock = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.processing_lock.lock(),
        )
        .await
        .map_err(|_| {
            tracing::error!("processing_lock acquisition timed out after 5s in save_single_event");
            anyhow::anyhow!(AppError::ProcessTimeout(
                "processing_lock acquisition timed out".into()
            ))
        })?;

        let settings = self.settings.read().await;

        // Calculate clip window duration
        let clip_window = self.calculate_clip_window(&trigger, &settings);
        drop(settings);

        let pre_duration = clip_window.pre_duration as f64;
        let post_duration = clip_window.post_duration as f64;
        let total_duration = pre_duration + post_duration;

        info!(
            "Event detected: {} (Waiting {}s for post-event capture...)",
            event.event_name, post_duration
        );

        // CRITICAL FIX: Wait for the post-event action to actually happen in the game/recorder
        // If we save immediately, we cut off the future.
        tokio::time::sleep(tokio::time::Duration::from_secs_f64(post_duration)).await;

        info!(
            "Saving clip for event: {} (priority: {}, duration: {:.1}s)",
            event.event_name,
            trigger.priority(),
            total_duration
        );

        // Generate clip ID
        let clip_id = format!("{}_{}", event.event_name, event.event_time as u32);

        // Get recorder and save clip using the new save_event_clip method
        let recorder = self.recorder.read().await;

        // Event time is in game time (seconds from game start)
        // We need to calculate the offset from recording start
        let event_time_secs = event.event_time as f64;

        let clip_path = match recorder
            .save_event_clip(event_time_secs, pre_duration, post_duration, &clip_id)
            .await
        {
            Ok(path) => {
                info!("Clip saved successfully: {:?}", path);
                path
            }
            Err(e) => {
                error!("Failed to save clip for event {}: {}", event.event_name, e);
                // Still save metadata with placeholder path for retry later
                let placeholder_path = std::path::PathBuf::from(format!("pending/{}.mp4", clip_id));
                placeholder_path
            }
        };

        // Save metadata to storage
        self.save_clip_metadata(&clip_id, &event, trigger.priority(), &clip_path)
            .await?;

        Ok(())
    }

    /// Save an event window (merged events)
    async fn save_event_window(&self, window: EventWindow) -> Result<()> {
        // Prevent concurrent saves
        let _lock = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.processing_lock.lock(),
        )
        .await
        .map_err(|_| {
            tracing::error!("processing_lock acquisition timed out after 5s in save_event_window");
            anyhow::anyhow!(AppError::ProcessTimeout(
                "processing_lock acquisition timed out".into()
            ))
        })?;

        let settings = self.settings.read().await;

        // Calculate clip window for primary event
        let clip_window = self.calculate_clip_window(&window.primary_trigger, &settings);
        drop(settings);

        // Extend duration to cover the full event window, capped to prevent
        // absurdly long clips (e.g., when app restarts mid-game and replays all events)
        const MAX_EVENT_WINDOW_SECS: f32 = 30.0;
        let event_window_duration =
            (window.end_time - window.start_time).min(MAX_EVENT_WINDOW_SECS);
        let total_duration = clip_window.pre_duration as f64
            + event_window_duration as f64
            + clip_window.post_duration as f64;

        info!(
            "Saving merged clip: {:?} ({} events, priority: {}, duration: {:.1}s)",
            window.primary_trigger,
            window.events.len(),
            window.priority,
            total_duration
        );

        // Use primary event for clip generation
        let primary_event = &window.events[0];
        let clip_id = format!(
            "merged_{}_{}",
            window.start_time as u32, window.end_time as u32
        );

        // Get recorder and save merged clip using save_event_clip method
        let recorder = self.recorder.read().await;

        // For merged events, we use the start of the window as the event time
        // and extend the post_duration to cover the entire window plus normal post duration
        let event_time_secs = window.start_time as f64;
        let pre_duration = clip_window.pre_duration as f64;
        // Post duration includes: window duration + normal post duration
        let post_duration = event_window_duration as f64 + clip_window.post_duration as f64;

        info!(
            "Merged Event Window: Waiting {}s for post-event capture...",
            post_duration
        );
        tokio::time::sleep(tokio::time::Duration::from_secs_f64(post_duration)).await;

        let clip_path = match recorder
            .save_event_clip(event_time_secs, pre_duration, post_duration, &clip_id)
            .await
        {
            Ok(path) => {
                info!("Merged clip saved successfully: {:?}", path);
                path
            }
            Err(e) => {
                error!(
                    "Failed to save merged clip for window {:?}: {}",
                    window.primary_trigger, e
                );
                // Still save metadata with placeholder path for retry later
                let placeholder_path = std::path::PathBuf::from(format!("pending/{}.mp4", clip_id));
                placeholder_path
            }
        };

        // Save metadata to storage
        self.save_clip_metadata(&clip_id, primary_event, window.priority, &clip_path)
            .await?;

        // Save all events in the window to storage
        let game_id = self.current_game_id.read().await;
        if let Some(ref game_id) = *game_id {
            let event_data: Vec<EventData> = window
                .events
                .iter()
                .map(|e| {
                    // Collect participants (killer + assisters)
                    let mut participants: Vec<String> = Vec::new();
                    if let Some(ref killer) = e.killer_name {
                        participants.push(killer.clone());
                    }
                    if let Some(ref assisters) = e.assisters {
                        participants.extend_from_slice(assisters);
                    }

                    EventData {
                        event_id: e.event_id as u64,
                        event_type: trigger_to_event_type(&window.primary_trigger),
                        timestamp: e.event_time as f64,
                        priority: window.priority,
                        participants,
                        details: None,
                    }
                })
                .collect();

            self.storage
                .save_events(game_id, &event_data)
                .context("Failed to save event data")?;
        }

        Ok(())
    }

    /// Calculate clip window (pre/post durations) based on settings and event type
    fn calculate_clip_window(
        &self,
        trigger: &EventTrigger,
        settings: &RecordingSettings,
    ) -> ClipWindow {
        // Map EventTrigger to settings event type string
        let event_type = match trigger {
            EventTrigger::Multikill(_) => "multikill",
            EventTrigger::Steal => "steal",
            EventTrigger::Death => "death",
            EventTrigger::GameEnd => "game_end",
            _ => "kill", // Default for other events
        };

        // Get event-specific timing or use defaults
        let timing = settings.clip_timing.get_timing_for_event(event_type);

        ClipWindow {
            pre_duration: timing.pre_duration,
            post_duration: timing.post_duration,
        }
    }

    /// Save clip metadata to storage
    async fn save_clip_metadata(
        &self,
        clip_id: &str,
        event: &GameEvent,
        priority: u8,
        clip_path: &std::path::Path,
    ) -> Result<()> {
        let game_id = self.current_game_id.read().await;

        if let Some(ref game_id) = *game_id {
            let metadata = ClipMetadata {
                file_path: clip_path.to_string_lossy().to_string(),
                thumbnail_path: None,
                event_type: EventType::Custom(event.event_name.clone()),
                event_time: event.event_time as f64,
                priority,
                duration: 0.0, // Will be calculated by video processor
                created_at: chrono::Utc::now(),
                usage_count: 0,
            };

            self.storage
                .save_clip_metadata(game_id, &metadata)
                .context("Failed to save clip metadata")?;

            info!("Clip metadata saved: {} (game: {})", clip_id, game_id);
        } else {
            warn!("No current game ID set - clip metadata not saved");
        }

        Ok(())
    }
}

/// Implement Drop to ensure proper cleanup when AutoClipManager is destroyed
impl Drop for AutoClipManager {
    fn drop(&mut self) {
        // Cancel the monitoring task to ensure it stops
        // The spawned task has a clone of cancel_token and will stop when cancelled
        if let Ok(token) = self.cancel_token.try_lock() {
            token.cancel();
        }

        // Note: We can't await the task handle in Drop (sync context)
        // The task will stop on its own due to cancellation
        // This is safe because:
        // 1. The cancel_token is cancelled, so the monitoring loop will exit
        // 2. Any pending events will be dropped (acceptable on app shutdown)

        // Try to abort the task if still running (best-effort cleanup)
        if let Ok(mut guard) = self.monitor_task.try_lock() {
            if let Some(handle) = guard.take() {
                handle.abort();
                debug!("AutoClipManager: aborted monitoring task on drop");
            }
        }

        debug!("AutoClipManager dropped, cleanup initiated");
    }
}

/// Clip window timing configuration
#[derive(Debug, Clone)]
struct ClipWindow {
    pre_duration: u32,  // Seconds before event
    post_duration: u32, // Seconds after event
}

/// Convert LiveClientMonitor's EventTrigger to storage's EventType
fn trigger_to_event_type(trigger: &EventTrigger) -> EventType {
    match trigger {
        EventTrigger::ChampionKill => EventType::ChampionKill,
        EventTrigger::Death => EventType::Custom("Death".to_string()),
        EventTrigger::Assist => EventType::Custom("Assist".to_string()),
        EventTrigger::FirstBlood => EventType::FirstBlood,
        EventTrigger::Multikill(n) => EventType::Multikill(*n),
        EventTrigger::DragonKill => EventType::DragonKill,
        EventTrigger::BaronKill => EventType::BaronKill,
        EventTrigger::HeraldKill => EventType::Custom("HeraldKill".to_string()),
        EventTrigger::TurretKill => EventType::TurretKill,
        EventTrigger::InhibitorKill => EventType::InhibitorKill,
        EventTrigger::Ace => EventType::Ace,
        EventTrigger::Steal => EventType::Custom("Steal".to_string()),
        EventTrigger::GameEnd => EventType::Custom("GameEnd".to_string()),
        EventTrigger::ElderDragonKill => EventType::Custom("ElderDragonKill".to_string()),
        EventTrigger::VoidgrubsKill => EventType::Custom("VoidgrubsKill".to_string()),
        EventTrigger::AtakhanKill => EventType::Custom("AtakhanKill".to_string()),
        EventTrigger::Shutdown => EventType::Custom("Shutdown".to_string()),
        EventTrigger::Outplay1vX(n) => EventType::Custom(format!("Outplay1v{}", n)),
        EventTrigger::TradeKill => EventType::Custom("TradeKill".to_string()),
        EventTrigger::LowHpOutplay => EventType::Custom("LowHpOutplay".to_string()),
    }
}

/// Convert live_client::GameEvent to recording::GameEvent
fn convert_live_event(
    live_event: super::live_client::GameEvent,
    _trigger: &EventTrigger,
) -> GameEvent {
    GameEvent {
        event_id: live_event.event_id,
        event_name: live_event.event_name,
        event_time: live_event.event_time,
        killer_name: live_event.killer_name,
        victim_name: live_event.victim_name,
        assisters: live_event.assisters,
        dragon_type: live_event.dragon_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::integration_backend::{RecordingConfig, WindowsCaptureRecorder};
    use crate::settings::models::RecordingSettings;

    fn create_test_event(event_name: &str, event_time: f32) -> GameEvent {
        GameEvent {
            event_id: 1,
            event_name: event_name.to_string(),
            event_time,
            killer_name: Some("TestPlayer".to_string()),
            victim_name: Some("Enemy".to_string()),
            assisters: Some(vec![]),
            dragon_type: None,
        }
    }

    #[tokio::test]
    async fn test_merge_events() {
        // Create test events at different times
        let events = vec![
            QueuedEvent {
                trigger: EventTrigger::ChampionKill,
                event: create_test_event("ChampionKill", 100.0),
                received_at: Instant::now(),
            },
            QueuedEvent {
                trigger: EventTrigger::Multikill(2),
                event: create_test_event("ChampionKill", 105.0),
                received_at: Instant::now(),
            },
            QueuedEvent {
                trigger: EventTrigger::Multikill(3),
                event: create_test_event("ChampionKill", 108.0),
                received_at: Instant::now(),
            },
        ];

        // Create temporary directory
        let temp_dir = std::env::temp_dir().join("lolshorts_test_acm");
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Setup dependencies
        let storage = Arc::new(Storage::new(&temp_dir).unwrap());
        let settings = Arc::new(TokioRwLock::new(RecordingSettings::default()));

        // Setup Recorder config for test
        let recorder_config = RecordingConfig {
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        // Create Recorder
        let recorder = WindowsCaptureRecorder::new(recorder_config).await.unwrap();
        let recorder_arc = Arc::new(TokioRwLock::new(recorder));

        let manager = AutoClipManager::new(recorder_arc, storage, settings);

        // Test merging logic
        let window = manager
            .merge_events(&events)
            .expect("merge_events should return Some for valid events");

        assert_eq!(window.events.len(), 3);
        assert_eq!(window.start_time, 100.0);
        assert_eq!(window.end_time, 108.0);
        // Priority should be highest event (Multikill 3 -> priority 3)
        assert_eq!(window.priority, 3);

        // Cleanup
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_event_filtering() {
        // Create temporary directory
        let temp_dir = std::env::temp_dir().join("lolshorts_test_filtering");
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Setup dependencies
        let storage = Arc::new(Storage::new(&temp_dir).unwrap());
        let settings = Arc::new(TokioRwLock::new(RecordingSettings::default()));

        // Setup Recorder config for test
        let recorder_config = RecordingConfig {
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        // Create Recorder
        let recorder = WindowsCaptureRecorder::new(recorder_config).await.unwrap();
        let recorder_arc = Arc::new(TokioRwLock::new(recorder));

        let manager = AutoClipManager::new(recorder_arc, storage, settings);

        // Test filtering
        let trigger = EventTrigger::ChampionKill; // Default settings usually allow kills
        let event = create_test_event("ChampionKill", 100.0);

        let should_record = manager.should_record_event(&trigger, &event).await.unwrap();
        assert!(should_record);

        // Cleanup
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
