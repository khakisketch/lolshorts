#![allow(clippy::unnecessary_cast)]
use anyhow::{Context as AnyhowContext, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::error::AppError;

use super::highlight_score::HighlightKind;
use super::integration_backend::segment_recorder::now_wall_secs;
use super::integration_backend::{RecordingStatus, WindowsCaptureRecorder};
use super::live_client::{
    EventStreamConfig, EventTrigger, GameEvent, LiveClientMonitor, PlayerSummary,
};
use crate::settings::models::{EventFilterSettings, RecordingSettings};
use crate::storage::{
    models::{ClipMetadata, EventData, EventType},
    Storage,
};

/// How long a save waits for the extraction slot (`processing_lock`) before giving up on
/// the clip.
///
/// Clips are anchored to an explicit wall-clock instant, so a save that queues behind
/// another export still extracts the RIGHT footage as long as the rolling buffer (90s by
/// default) still holds it. Waiting is therefore strictly better than the old 5s timeout,
/// which dropped whole event windows — events included — whenever the previous export was
/// still running.
const PROCESSING_LOCK_TIMEOUT: Duration = Duration::from_secs(60);

/// Shorter budget for the game-end flush path.
///
/// At game end the user is waiting on the stop sequence (`stop_event_monitoring` runs
/// before `stop_recording`), and the footage is already final — so a flush that cannot get
/// the extraction slot quickly gives up on the clip rather than stalling the UI behind a
/// long export. The window's event data has been persisted either way.
///
/// Deliberately much shorter than `FLUSH_EXTRACTION_BUDGET`: a lock wait that ate the
/// budget would leave no time to actually extract the last highlight — the very clip this
/// path exists for. The in-flight drain
/// runs first, so by the time the flush asks for the slot it is normally already free.
const FLUSH_PROCESSING_LOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// Game-end barrier, part 1: how long `stop_event_monitoring` waits for DETACHED clip work
/// (per-event saves and merge-flush timers) to finish.
///
/// The caller stops the recorder as soon as this returns, and `save_event_clip` bails with
/// "녹화가 진행 중이 아닙니다" the instant the recorder leaves Recording/Buffering — so an
/// extraction still running here loses its clip outright. That clip is typically the last
/// teamfight of the game, so we wait; bounded, because a stuck export must never hang the
/// stop sequence.
const INFLIGHT_DRAIN_TIMEOUT: Duration = Duration::from_secs(15);

/// Game-end barrier, part 2: how long `stop_event_monitoring` gives the final
/// `flush_pending_events` extraction.
///
/// On overrun the flush is abandoned (FFmpeg children are spawned with `kill_on_drop`, so
/// nothing is left running) and the stop sequence continues — a warn, never a hang.
///
/// 60s, not 15s: the budget has to be larger than the operation it wraps or it aborts
/// exactly the extraction it exists to protect. One flush extraction pays, in the worst
/// case, `FLUSH_PROCESSING_LOCK_TIMEOUT` (5s) plus a per-segment ffprobe
/// (`SEGMENT_PROBE_TIMEOUT`, 15s) and decode check (`SEGMENT_VERIFY_TIMEOUT`, 20s) before
/// FFmpeg even starts. A typical 9-segment buffer measures in a couple of seconds, so 60s
/// clears the real path with room to spare while still bounding a pathological one.
///
/// This is a ceiling on how long game-end can stall, not a target — the drain below
/// normally leaves nothing for the flush to do.
const FLUSH_EXTRACTION_BUDGET: Duration = Duration::from_secs(60);
/// Preserve the final on-screen result moment without making shutdown feel hung.
const GAME_END_POST_ROLL_CAP: Duration = Duration::from_secs(3);

/// Extra delay added to the merge-flush timer.
///
/// The drain condition compares whole seconds (`received_at.elapsed().as_secs()`), so a
/// timer that fires at exactly the threshold can land a few microseconds early, truncate
/// to `threshold - 1` and no-op. The margin makes the first check deterministic.
const MERGE_FLUSH_MARGIN: Duration = Duration::from_millis(500);

/// Upper bound for the configured merge threshold when arming the flush timer. A window
/// longer than the rolling buffer can never yield a clip, and the value comes from
/// user-editable settings, so it is clamped rather than trusted.
const MAX_MERGE_THRESHOLD_SECS: f64 = 120.0;

/// Normalize the user-configurable merge threshold.
///
/// Settings come from user-editable JSON, and `Duration::from_secs_f64` PANICS on
/// NaN/infinity — so the value is clamped into a range where a merge window still
/// makes sense (one longer than the rolling buffer could never produce a clip).
/// Both the timer that closes the window and the check that decides whether it is
/// closed MUST use this, or a setting above the cap leaves the queue with no
/// pending timer.
fn clamp_merge_threshold_secs(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, MAX_MERGE_THRESHOLD_SECS)
    } else {
        0.0
    }
}

/// Queued event with timestamp for merging logic
#[derive(Debug, Clone)]
struct QueuedEvent {
    trigger: EventTrigger,
    event: GameEvent,
    received_at: Instant,
    /// Wall-clock instant (seconds since the UNIX epoch) at which this event was
    /// detected. This — not `event.event_time` (in-game seconds) — is what the clip
    /// window is anchored to, because the rolling video buffer is indexed by wall
    /// clock. The Live Client is polled every 250ms, so the detection instant is
    /// within a quarter second of the play; translating in-game `EventTime` into wall
    /// clock would need a game-start reference we do not reliably have.
    received_wall_secs: f64,
}

/// Event window after merging consecutive events
#[derive(Debug, Clone)]
struct EventWindow {
    primary_trigger: EventTrigger,
    events: Vec<GameEvent>,
    start_time: f32, // Game time in seconds
    end_time: f32,   // Game time in seconds
    priority: u8,    // Highest priority in window
    /// Wall-clock detection instant of the FIRST event in the window (clip start anchor).
    first_event_wall: f64,
    /// Wall-clock detection instant of the LAST event in the window (clip end anchor).
    last_event_wall: f64,
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

    /// `GameEnd` 순간에 찍힌 내 전적(챔피언·KDA·승패).
    ///
    /// 게임이 끝나면 Live Client API 는 응답을 멈추므로 그 뒤에 물어봐서는 알 수
    /// 없다. 감시 모니터가 여기에 담아 두고, 세션을 마무리하는 쪽이 꺼내 간다.
    last_game_summary: Arc<TokioRwLock<Option<PlayerSummary>>>,

    /// Current game ID for clip organization
    current_game_id: Arc<TokioRwLock<Option<String>>>,

    /// Processing lock to prevent concurrent clip saves
    processing_lock: Arc<TokioMutex<()>>,

    /// Low-priority, single-slot thumbnail lane. Clip publication and metadata never
    /// wait for JPEG extraction, and burst events cannot launch thumbnail FFmpeg jobs
    /// concurrently during gameplay.
    thumbnail_lock: Arc<TokioMutex<()>>,

    /// Event monitoring task handle
    monitor_task: Arc<TokioMutex<Option<JoinHandle<()>>>>,

    /// Cancellation token for stopping the monitoring task.
    /// Wrapped in a mutex so we can replace it with a fresh token after cancellation.
    cancel_token: Arc<TokioMutex<CancellationToken>>,

    /// Cancellation token handed to DETACHED background work (the merge-flush timer and
    /// the per-event processing tasks the Live Client callback spawns).
    ///
    /// It is a clone of the current monitoring session's token — cancelling the session
    /// wakes every detached task — but `Drop` never touches it. That distinction matters:
    /// the short-lived manager clones those tasks run on would otherwise cancel the whole
    /// session the moment they are dropped (`Drop` cancels `cancel_token`).
    task_cancel: Arc<TokioMutex<CancellationToken>>,

    /// Number of detached clip tasks currently in flight — everything that can reach
    /// `save_event_clip` without the stop sequence awaiting it directly:
    /// * the per-event tasks the manual path's Live Client callback spawns,
    /// * `handle_game_event` calls (the auto-detect path spawns one task per event in
    ///   `game_monitor`),
    /// * the merge-flush timers, from the moment they are armed.
    ///
    /// `stop_event_monitoring` drains this (bounded) before flushing, so an extraction can
    /// neither be cut off by the recorder stopping under it nor land after the session has
    /// been finalized and the current game id cleared.
    inflight_clip_tasks: Arc<AtomicUsize>,

    /// Serializes the read-modify-write in `persist_events`. `Storage::save_events`
    /// REPLACES the whole per-game event blob, so two concurrent appends would silently
    /// drop one side's events.
    events_write_lock: Arc<TokioMutex<()>>,

    /// Current game mode string (e.g. "CLASSIC", "ARAM")
    current_game_mode: Arc<TokioRwLock<String>>,

    /// Current queue ID (e.g. 420 = ranked solo, 440 = ranked flex)
    current_queue_id: Arc<TokioRwLock<Option<u32>>>,

    /// Number of clips saved during the current game session. Reset when a new game
    /// starts (`set_current_game(Some(..))`) and read at game-end to report an
    /// accurate clip count to the desktop notification.
    saved_clip_count: Arc<AtomicUsize>,

    /// Whether a merge-window flush timer is already in flight (dedup guard so a burst
    /// of events arms exactly one timer per window).
    merge_flush_armed: Arc<AtomicBool>,

    /// AppHandle used to emit `recording-status` / `clip-saved` / `game-event` to the
    /// frontend (overlay + dashboard). `None` until `set_app_handle` is called from
    /// `main.rs`'s Tauri `setup` hook (an AppHandle only exists once the app has
    /// started), and always `None` in unit tests — `emit_event` no-ops silently in
    /// that case so call sites never need to special-case it.
    app_handle: Arc<TokioMutex<Option<tauri::AppHandle>>>,
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
            last_game_summary: Arc::new(TokioRwLock::new(None)),
            current_game_id: Arc::new(TokioRwLock::new(None)),
            processing_lock: Arc::new(TokioMutex::new(())),
            thumbnail_lock: Arc::new(TokioMutex::new(())),
            monitor_task: Arc::new(TokioMutex::new(None)),
            cancel_token: Arc::new(TokioMutex::new(CancellationToken::new())),
            task_cancel: Arc::new(TokioMutex::new(CancellationToken::new())),
            inflight_clip_tasks: Arc::new(AtomicUsize::new(0)),
            events_write_lock: Arc::new(TokioMutex::new(())),
            current_game_mode: Arc::new(TokioRwLock::new(String::new())),
            current_queue_id: Arc::new(TokioRwLock::new(None)),
            saved_clip_count: Arc::new(AtomicUsize::new(0)),
            merge_flush_armed: Arc::new(AtomicBool::new(false)),
            app_handle: Arc::new(TokioMutex::new(None)),
        }
    }

    /// Clone the shared state handles for a detached background task.
    ///
    /// `monitor_task`/`cancel_token` are deliberately fresh: they belong to the
    /// long-lived manager, and a detached task must never cancel or abort it (its `Drop`
    /// cancels whatever token it holds). Cancellation still reaches the task through
    /// `task_cancel`, which is shared and never cancelled by `Drop`.
    fn detached_handle(&self) -> Self {
        Self {
            recorder: Arc::clone(&self.recorder),
            storage: Arc::clone(&self.storage),
            settings: Arc::clone(&self.settings),
            event_queue: Arc::clone(&self.event_queue),
            last_game_summary: Arc::clone(&self.last_game_summary),
            current_game_id: Arc::clone(&self.current_game_id),
            processing_lock: Arc::clone(&self.processing_lock),
            thumbnail_lock: Arc::clone(&self.thumbnail_lock),
            monitor_task: Arc::new(TokioMutex::new(None)),
            cancel_token: Arc::new(TokioMutex::new(CancellationToken::new())),
            task_cancel: Arc::clone(&self.task_cancel),
            inflight_clip_tasks: Arc::clone(&self.inflight_clip_tasks),
            events_write_lock: Arc::clone(&self.events_write_lock),
            current_game_mode: Arc::clone(&self.current_game_mode),
            current_queue_id: Arc::clone(&self.current_queue_id),
            saved_clip_count: Arc::clone(&self.saved_clip_count),
            merge_flush_armed: Arc::clone(&self.merge_flush_armed),
            app_handle: Arc::clone(&self.app_handle),
        }
    }

    /// Wire in the AppHandle used to emit frontend events (`recording-status`,
    /// `clip-saved`, `clip-save-failed`, `game-event`). Called once from `main.rs`'s
    /// Tauri `setup` hook, since an AppHandle only exists once the app has started.
    pub async fn set_app_handle(&self, handle: tauri::AppHandle) {
        *self.app_handle.lock().await = Some(handle);
    }

    /// Emit a Tauri event to the frontend (overlay + dashboard) if an AppHandle has
    /// been wired in via `set_app_handle`. Silently no-ops otherwise (unit tests,
    /// or a call that races app startup) so call sites never need to special-case
    /// a missing handle. `pub(crate)` so `game_lifecycle` can reuse the same handle
    /// for `recording-status` transitions instead of threading a second AppHandle
    /// through every start/stop call site.
    pub(crate) async fn emit_event(&self, event: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        if let Some(handle) = self.app_handle.lock().await.as_ref() {
            if let Err(e) = handle.emit(event, payload) {
                warn!("Failed to emit '{}' event: {}", event, e);
            }
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
            // New session — reset the per-session saved-clip counter.
            self.saved_clip_count.store(0, Ordering::SeqCst);
            // 지난 판의 전적이 남아 있으면 그게 이 판의 요약으로 저장된다.
            // 마무리가 세션 없이 지나간 경우(`finish_auto_capture_session` 의
            // "active game session 없음" 경로) 값이 슬롯에 잔류할 수 있다.
            *self.last_game_summary.write().await = None;
            // Arm a fresh token for this session's detached work.
            //
            // `start_event_monitoring` also publishes its session token here, but it
            // only runs on the MANUAL path: auto-detect drives events through
            // `game_monitor`'s own LiveClientMonitor straight into
            // `handle_game_event`, so on the default path nothing else would ever
            // arm (or cancel) `task_cancel` and every post-event wait would be
            // uncancellable. `set_current_game` is the one call both paths make —
            // `begin_auto_capture_session` for start, `finish_auto_capture_session`
            // for end — so the token's lifetime is tied to it.
            {
                let mut token = self.task_cancel.lock().await;
                if token.is_cancelled() {
                    *token = CancellationToken::new();
                }
            }
            info!("Auto Clip Manager: tracking game {}", id);
        } else {
            // Session over: wake every detached wait so post-event sleeps collapse
            // instead of running on past game end.
            self.task_cancel.lock().await.cancel();
            info!("Auto Clip Manager: game ended, clearing queue");
            // Clear event queue when game ends. Callers are expected to run
            // flush_pending_events() (via stop_event_monitoring) beforehand so
            // that end-of-game highlights are not lost. The saved-clip counter
            // is intentionally NOT reset here so it can be read during game-end.
            let mut queue = self.event_queue.lock().await;
            queue.clear();
        }
    }

    /// Current game ID (if a session is active).
    pub async fn current_game_id(&self) -> Option<String> {
        self.current_game_id.read().await.clone()
    }

    #[cfg(test)]
    pub(crate) async fn current_game_id_for_tests(&self) -> Option<String> {
        self.current_game_id.read().await.clone()
    }

    /// Number of events currently sitting in the merge queue (test-only).
    #[cfg(test)]
    pub(crate) async fn queued_event_count(&self) -> usize {
        self.event_queue.lock().await.len()
    }

    /// Number of clips saved during the current game session.
    ///
    /// Read at game-end so the frontend desktop notification can report the real
    /// count instead of a hardcoded `0`.
    pub fn saved_clip_count(&self) -> usize {
        self.saved_clip_count.load(Ordering::SeqCst)
    }

    /// Count a clip that was saved outside the automatic pipeline.
    ///
    /// The manual replay path (`save_replay` / the F8+F9 hotkeys) writes its
    /// metadata in `recording::commands`, not through `save_clip_metadata`, so it
    /// never reached the counter above — the end-of-game notification silently
    /// under-reported every manually saved clip.
    pub(crate) fn record_externally_saved_clip(&self) {
        self.saved_clip_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Whether the underlying recorder is currently recording/buffering.
    ///
    /// `GameEnd` 에 찍힌 전적을 꺼내 간다(꺼내면 비워진다).
    pub async fn take_game_summary(&self) -> Option<PlayerSummary> {
        self.last_game_summary.write().await.take()
    }

    /// 비우지 않고 들여다본다. 저장에 성공한 뒤에야 `take` 로 비운다.
    pub async fn peek_game_summary(&self) -> Option<PlayerSummary> {
        self.last_game_summary.read().await.clone()
    }

    /// 요약을 받을 그릇. 이벤트 모니터를 만드는 쪽이 이걸 넘겨야 한다.
    ///
    /// 모니터 생성 경로가 둘(수동 = 여기, 자동 감지 = `game_monitor`)이라 두
    /// 경로가 **같은** 그릇을 봐야 한다. 이 접근자가 그 유일한 출처다.
    pub fn summary_slot(&self) -> Arc<TokioRwLock<Option<PlayerSummary>>> {
        Arc::clone(&self.last_game_summary)
    }

    /// Used by the game-state monitor to detect manual-capture preemption (the
    /// user started recording via F8/command before auto-detect fired) so it can
    /// adopt the session instead of retrying `start_recording` every second.
    pub async fn is_recording(&self) -> bool {
        matches!(
            self.recorder.read().await.get_status().await,
            RecordingStatus::Recording | RecordingStatus::Buffering
        )
    }

    /// Build the event-stream config from current user settings so that
    /// `contest_window_secs` (Task 30) actually reaches the detection logic.
    pub async fn event_stream_config(&self) -> EventStreamConfig {
        let settings = self.settings.read().await;
        EventStreamConfig::from_settings(settings.event_filter.contest_window_secs)
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

        // Create a LiveClientMonitor using the user's event-filter settings so that
        // contest_window_secs (Task 30) actually reaches steal detection.
        let mut monitor =
            LiveClientMonitor::with_config(self.event_stream_config().await, self.summary_slot())
                .context("Failed to create LiveClientMonitor")?;

        // Clone Arc references for the monitoring task
        let event_queue = Arc::clone(&self.event_queue);
        let settings = Arc::clone(&self.settings);
        let recorder = Arc::clone(&self.recorder);
        let storage = Arc::clone(&self.storage);
        let current_game_id = Arc::clone(&self.current_game_id);
        let processing_lock = Arc::clone(&self.processing_lock);
        let thumbnail_lock = Arc::clone(&self.thumbnail_lock);
        let current_game_mode = Arc::clone(&self.current_game_mode);
        let current_queue_id = Arc::clone(&self.current_queue_id);
        let saved_clip_count = Arc::clone(&self.saved_clip_count);
        let merge_flush_armed = Arc::clone(&self.merge_flush_armed);
        let app_handle = Arc::clone(&self.app_handle);
        let task_cancel = Arc::clone(&self.task_cancel);
        let inflight_clip_tasks = Arc::clone(&self.inflight_clip_tasks);
        let last_game_summary = Arc::clone(&self.last_game_summary);
        let events_write_lock = Arc::clone(&self.events_write_lock);
        // FIX #6: Create a fresh cancellation token for each monitoring session
        // so that a previous cancel() doesn't keep the new session cancelled.
        let cancel_token = {
            let mut token_guard = self.cancel_token.lock().await;
            *token_guard = CancellationToken::new();
            token_guard.clone()
        };
        // Publish the session token to detached work (merge-flush timer, per-event
        // tasks). It is the SAME token `stop_event_monitoring` cancels, so a stop wakes
        // every detached task instead of leaving it to finish long after game end.
        {
            let mut task_token = self.task_cancel.lock().await;
            *task_token = cancel_token.clone();
        }

        // Spawn monitoring task
        let handle = tokio::spawn(async move {
            info!("Event monitoring task started");

            // Create callback closure that processes events
            let callback = move |trigger: EventTrigger, event: super::live_client::GameEvent| {
                // 이벤트를 그대로 쓴다 — 같은 타입인데 필드를 옮겨 적던 변환이
                // `moment`·`result` 를 매번 버리고 있었다.

                // Clone Arc references for the async block
                let event_queue = Arc::clone(&event_queue);
                let settings = Arc::clone(&settings);
                let recorder = Arc::clone(&recorder);
                let storage = Arc::clone(&storage);
                let current_game_id = Arc::clone(&current_game_id);
                let processing_lock = Arc::clone(&processing_lock);
                let thumbnail_lock = Arc::clone(&thumbnail_lock);
                let current_game_mode = Arc::clone(&current_game_mode);
                let current_queue_id = Arc::clone(&current_queue_id);
                let saved_clip_count = Arc::clone(&saved_clip_count);
                let merge_flush_armed = Arc::clone(&merge_flush_armed);
                let app_handle = Arc::clone(&app_handle);
                let task_cancel = Arc::clone(&task_cancel);
                let events_write_lock = Arc::clone(&events_write_lock);
                let inflight_clip_tasks = Arc::clone(&inflight_clip_tasks);
                let last_game_summary = Arc::clone(&last_game_summary);
                // Counted BEFORE the spawn so `stop_event_monitoring` can never
                // observe zero for a task that has been created but not yet polled.
                let inflight = InflightGuard::new(Arc::clone(&inflight_clip_tasks));

                // Spawn a task to process the event asynchronously
                tokio::spawn(async move {
                    let _inflight = inflight;
                    // Create a temporary AutoClipManager instance for processing.
                    // `cancel_token` is fresh (its Drop must not kill the session);
                    // `task_cancel` is the shared session token so the post-event
                    // waits inside actually observe a stop.
                    let temp_manager = AutoClipManager {
                        recorder,
                        storage,
                        settings,
                        event_queue,
                        last_game_summary,
                        current_game_id,
                        processing_lock,
                        thumbnail_lock,
                        monitor_task: Arc::new(TokioMutex::new(None)),
                        cancel_token: Arc::new(TokioMutex::new(CancellationToken::new())),
                        task_cancel,
                        inflight_clip_tasks,
                        events_write_lock,
                        current_game_mode,
                        current_queue_id,
                        saved_clip_count,
                        merge_flush_armed,
                        app_handle,
                    };

                    let trigger_display = format!("{:?}", trigger);

                    // Already stopping: do not open a new save that would write to a
                    // session the caller is about to finalize.
                    if temp_manager.session_cancelled().await {
                        debug!(
                            "Dropping event {} — monitoring is stopping",
                            trigger_display
                        );
                        return;
                    }

                    // NOTE: the processing future itself is deliberately NOT wrapped
                    // in a `select!` against the cancel token. Dropping it mid-save
                    // would throw away an extraction that is often nearly finished —
                    // typically the last fight of the game. Instead the waits inside
                    // are cancellation-aware (`sleep_or_cancelled`) so a stop makes
                    // this task finish promptly, and `stop_event_monitoring` waits
                    // (bounded) for the in-flight count to drain.
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
    /// Cancels the background monitoring task, waits for it to finish, and then runs the
    /// GAME-END BARRIER: every detached clip extraction is drained and the queue is
    /// flushed — all while the recorder is still alive, because `save_event_clip` refuses
    /// to run once the recorder has stopped ("녹화가 진행 중이 아닙니다"). Callers must
    /// therefore invoke this BEFORE stopping the recorder;
    /// `game_lifecycle::stop_capture_pipeline` owns that ordering.
    ///
    /// The barrier is bounded (`INFLIGHT_DRAIN_TIMEOUT` + `FLUSH_EXTRACTION_BUDGET`):
    /// anything unfinished by then is warned about and abandoned so the stop sequence
    /// always proceeds.
    pub async fn stop_event_monitoring(&self) -> Result<()> {
        self.stop_event_monitoring_with_budget(INFLIGHT_DRAIN_TIMEOUT, FLUSH_EXTRACTION_BUDGET)
            .await
    }

    /// `stop_event_monitoring` with explicit barrier budgets.
    ///
    /// The budgets are a parameter purely so the overrun path can be exercised in a test
    /// without spending the real 30s; production always goes through the wrapper above.
    async fn stop_event_monitoring_with_budget(
        &self,
        drain_budget: Duration,
        flush_budget: Duration,
    ) -> Result<()> {
        info!("Stopping event monitoring...");

        // The GameEnd signal arrives before the result screen has visually settled.
        // Keep the recorder alive for only the remaining configured post-roll, capped at
        // three seconds. The previous stop path cancelled waits immediately and produced
        // a visibly abrupt ~2s tail in the field recording.
        self.wait_for_game_end_post_roll().await;

        // Cancel the monitoring task so no new events arrive during the flush.
        {
            let token = self.cancel_token.lock().await;
            token.cancel();
        }

        // Also cancel the detached-work token. On the manual path these are the same
        // token, so this is a no-op; on the auto-detect path `task_cancel` is armed by
        // `set_current_game` and would otherwise stay live until the session is
        // finalized — which happens AFTER the in-flight drain below, making the drain
        // wait out its full timeout for tasks nobody has asked to stop.
        self.task_cancel.lock().await.cancel();

        // Get and wait for the task to finish
        {
            let mut task_guard = self.monitor_task.lock().await;
            if let Some(handle) = task_guard.take() {
                handle.await.context("Failed to join monitoring task")?;
                info!("Event monitoring stopped successfully");
            } else {
                info!("Event monitoring was not running");
            }
        }

        // ---- Game-end barrier ------------------------------------------------------
        // Everything below MUST finish before the caller stops the recorder. An
        // extraction that is still running when the recorder stops fails outright with
        // "녹화가 진행 중이 아닙니다" and the clip — typically the last teamfight of the
        // game — is lost, which is exactly the bug this barrier exists to prevent.

        // 1. Detached clip work: per-event saves AND merge-flush timers (which own a
        //    window they have already drained out of the queue, so nothing else can save
        //    it for them). Their waits are cancellation-aware, so the cancel above has
        //    already collapsed the long post-event sleeps; what is left is an extraction
        //    of footage that ALREADY exists — waiting for new frames is never part of
        //    this, the game is over. This is the single drain point: the flush below runs
        //    after it, so the extraction slot is normally free by then.
        self.wait_for_inflight_clip_tasks(drain_budget).await;

        // 2. Save whatever is still queued (the merge window may never have closed)
        //    before the game session clears the queue. Bounded for the same reason as the
        //    drain; FFmpeg children are spawned with `kill_on_drop`, so abandoning this
        //    future cannot leave a process behind.
        match tokio::time::timeout(flush_budget, self.flush_pending_events()).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => warn!("Failed to flush pending events on stop: {}", e),
            Err(_) => warn!(
                "Game-end flush did not finish within {:.0}s; continuing with stop \
                 (the clip is lost, its event data was already persisted)",
                flush_budget.as_secs_f64()
            ),
        }

        Ok(())
    }

    async fn wait_for_game_end_post_roll(&self) {
        let detected_at = {
            let queue = self.event_queue.lock().await;
            queue
                .iter()
                .rev()
                .find(|queued| matches!(queued.trigger, EventTrigger::GameEnd))
                .map(|queued| queued.received_wall_secs)
        };
        let Some(detected_at) = detected_at else {
            return;
        };

        let configured_post = {
            let settings = self.settings.read().await;
            self.calculate_clip_window(&EventTrigger::GameEnd, &settings)
                .post_duration as f64
        };
        let remaining = (detected_at + configured_post - now_wall_secs())
            .max(0.0)
            .min(GAME_END_POST_ROLL_CAP.as_secs_f64());
        if remaining > 0.0 {
            info!(
                remaining_secs = remaining,
                configured_post_secs = configured_post,
                "Waiting for final GameEnd post-roll before stopping capture"
            );
            tokio::time::sleep(Duration::from_secs_f64(remaining)).await;
        }
    }

    /// Whether the current monitoring session has been cancelled.
    async fn session_cancelled(&self) -> bool {
        self.task_cancel.lock().await.is_cancelled()
    }

    /// Sleep, but wake immediately if the monitoring session is cancelled.
    ///
    /// Returns `true` when the full duration elapsed, `false` when a stop cut it short.
    /// A cut-short wait does NOT abort the save: the footage that already exists is still
    /// worth extracting, and the caller anchors the window to "now" in that case instead
    /// of waiting out the coverage timeout for frames that will never be recorded.
    async fn sleep_or_cancelled(&self, duration: Duration) -> bool {
        let cancel = self.task_cancel.lock().await.clone();
        tokio::select! {
            _ = tokio::time::sleep(duration) => true,
            _ = cancel.cancelled() => {
                info!("Post-event wait cut short: event monitoring was stopped");
                false
            }
        }
    }

    /// Wait (bounded) until no detached clip task is running.
    ///
    /// The single drain point of the game-end barrier — see `inflight_clip_tasks` for what
    /// it covers. Returns early on `budget` expiry with a warn so the stop sequence can
    /// never hang on a stuck export.
    async fn wait_for_inflight_clip_tasks(&self, budget: Duration) {
        let deadline = tokio::time::Instant::now() + budget;
        loop {
            let inflight = self.inflight_clip_tasks.load(Ordering::SeqCst);
            if inflight == 0 {
                return;
            }
            if tokio::time::Instant::now() >= deadline {
                warn!(
                    "{} event task(s) still in flight after {:.0}s; continuing with stop",
                    inflight,
                    budget.as_secs_f64()
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Arm the merge-window flush timer.
    ///
    /// `try_process_merged_events` only ever ran from `process_event`, so a merge window
    /// that received no further event stayed in the queue until the next event or game
    /// end. With the defaults (merge on, 15s threshold) a single kill therefore sat there
    /// for minutes; by the time it flushed, the 90s rolling buffer had rotated past the
    /// play and the "clip" was an empty window. This timer is what actually closes a merge
    /// window: exactly one is in flight per window (`merge_flush_armed` is the dedup
    /// guard), it wakes just after the threshold and drains whatever the window collected.
    async fn arm_merge_flush(&self, merge_threshold_secs: f64) {
        if self.merge_flush_armed.swap(true, Ordering::SeqCst) {
            // A timer for the currently open window is already pending.
            return;
        }

        let threshold = clamp_merge_threshold_secs(merge_threshold_secs);
        let delay = Duration::from_secs_f64(threshold) + MERGE_FLUSH_MARGIN;
        let cancel = self.task_cancel.lock().await.clone();
        let manager = self.detached_handle();
        // Counted from ARM time, not from the moment the timer fires: between "timer
        // fires" and "extraction finishes" this task OWNS the window (it drains it out of
        // the queue), so `stop_event_monitoring` slipping through that gap stopped the
        // recorder from under a save that was already logging "Saving merged clip" — the
        // last highlight of the game vanished with a "녹화가 진행 중이 아닙니다" error.
        // A pending timer costs the drain nothing: a cancel wakes it immediately.
        let inflight = InflightGuard::new(Arc::clone(&self.inflight_clip_tasks));

        tokio::spawn(async move {
            let _inflight = inflight;
            tokio::select! {
                _ = cancel.cancelled() => {
                    // Stopping: `stop_event_monitoring` flushes the queue itself, so
                    // waking up later to save would be a zombie write into a finalized
                    // session. Disarm so a later session can arm again.
                    manager.merge_flush_armed.store(false, Ordering::SeqCst);
                    debug!("Merge flush timer cancelled before firing");
                    return;
                }
                _ = tokio::time::sleep(delay) => {}
            }

            // Disarm BEFORE draining: the drain (and the export behind it) can take tens
            // of seconds, and an event arriving in that gap must be able to open a fresh
            // window instead of waiting for yet another event to arrive.
            manager.merge_flush_armed.store(false, Ordering::SeqCst);

            if let Err(e) = manager.try_process_merged_events().await {
                error!("Merge flush timer failed to process window: {}", e);
            }
        });
    }

    /// Flush any events still sitting in the queue, saving them immediately.
    ///
    /// Called during game-end cleanup (before `set_current_game(None)` clears the
    /// queue) so the final highlights of a game — the plays most worth clipping —
    /// aren't systematically lost when the merge window hasn't closed yet.
    ///
    /// Unlike the normal merge path this does NOT wait for `post_duration`; it saves
    /// whatever footage is already available. In immediate (non-merge) mode the queued
    /// entries are only leftovers of the queue bookkeeping — `save_single_event` already
    /// wrote both the clip metadata AND the `EventData` for each of them — so they are
    /// discarded here to avoid duplicates.
    pub async fn flush_pending_events(&self) -> Result<()> {
        let merge_enabled = {
            let settings = self.settings.read().await;
            settings.clip_timing.merge_consecutive_events
        };

        let pending: Vec<QueuedEvent> = {
            let mut queue = self.event_queue.lock().await;
            queue.drain(..).collect()
        };

        if pending.is_empty() {
            return Ok(());
        }

        if !merge_enabled {
            debug!(
                "flush_pending_events: {} event(s) already saved (immediate mode), discarding",
                pending.len()
            );
            return Ok(());
        }

        info!(
            "Flushing {} pending event(s) before game end",
            pending.len()
        );

        let window = match self.merge_events(&pending) {
            Some(w) => w,
            None => {
                warn!(
                    "flush_pending_events: failed to merge {} events",
                    pending.len()
                );
                return Ok(());
            }
        };

        // Save immediately without waiting for post-event footage (game is ending).
        self.save_event_window_inner(window, false).await
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
        // The auto-detect path — the DEFAULT one — spawns a task per event in
        // `game_monitor`, so this whole call is detached work the game-end barrier has to
        // wait for; in immediate mode it runs the post-event sleep and the extraction
        // itself. Without this guard `stop_event_monitoring` saw zero tasks in flight and
        // let the caller stop the recorder mid-save. (The manual path counts its tasks
        // BEFORE spawning them in `start_event_monitoring` and reaches `process_event`
        // directly, so it is not double-counted here.)
        let _inflight = InflightGuard::new(Arc::clone(&self.inflight_clip_tasks));

        // Already stopping: opening a new save now would either race the barrier or land
        // after the recorder has stopped. The manual path drops late events the same way.
        if self.session_cancelled().await {
            debug!(
                "Dropping event {} — monitoring is stopping",
                event.event_name
            );
            return Ok(());
        }

        debug!(
            "Auto Clip Manager: handling live event {} (priority: {})",
            event.event_name,
            trigger.priority()
        );

        // 이벤트를 **그대로** 넘긴다. 예전에는 `convert_live_event` 로 필드를 손수
        // 옮겨 적었는데, 같은 타입인데도 그렇게 하는 바람에 `moment`(그 순간의
        // 체력·생존 인원)와 `result`(승/패)가 매번 조용히 버려졌다.
        self.process_event(trigger, event).await
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

        // 담을지, 그리고 **어떤 이름으로** 담을지. 하위 상황의 토글이 꺼져 있으면
        // 여기서 부모 이름으로 강등되어 돌아오므로, 아래 모든 단계(우선순위·클립
        // 길이·저장 이름·점수)가 강등된 이름을 일관되게 쓴다.
        let Some(trigger) = self.resolve_recordable_trigger(&trigger, &event).await? else {
            debug!(
                "Event filtered out by settings: {} (priority: {})",
                event.event_name,
                trigger.priority()
            );
            return Ok(());
        };

        // Notify the overlay's event feed that a clip-worthy trigger fired. This is
        // detection, not save confirmation — the clip itself is still queued/merged
        // and extracted asynchronously below (see `clip-saved` / `clip-save-failed`).
        self.emit_event(
            "game-event",
            serde_json::json!({
                "name": event.event_name,
                "priority": trigger.priority(),
            }),
        )
        .await;

        // Stamp the detection instant ONCE and carry it through the whole chain
        // (queue -> merge window -> save_event_clip -> save_clip_anchored) so the clip
        // is anchored to when the play happened, not to when we got around to saving it.
        let received_wall_secs = now_wall_secs();

        // Add event to queue
        let queued = QueuedEvent {
            trigger: trigger.clone(),
            event: event.clone(),
            received_at: Instant::now(),
            received_wall_secs,
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
        let merge_enabled = settings.clip_timing.merge_consecutive_events;
        let merge_threshold = settings.clip_timing.merge_time_threshold;
        drop(settings);

        if merge_enabled {
            // Arm the timer that CLOSES this merge window. Without it the queue is only
            // ever drained by the arrival of another event, so a lone kill never became a
            // clip. Arming first means the guarantee holds even if the drain below throws.
            self.arm_merge_flush(merge_threshold).await;
            // Also drain right away if the window that was already open has aged out.
            self.try_process_merged_events().await?;
        } else {
            // Save immediately without merging, anchored to the detection instant.
            self.save_single_event(trigger, event, received_wall_secs)
                .await?;
        }

        Ok(())
    }

    /// 이 이벤트를 **어떤 이름으로** 저장할지 정한다. 담지 않기로 하면 `None`.
    ///
    /// # 왜 bool 이 아닌가
    ///
    /// 감지는 킬 하나에 가장 특별한 이름 하나만 붙인다(`detect_trigger`). 그래서
    /// 셧다운을 끈 사용자의 셧다운 킬은 "킬"이 아니라 "셧다운"으로 도착하고,
    /// 예전 코드는 그걸 그대로 버렸다 — 킬을 담겠다고 켜 둔 사용자가 가장 좋은
    /// 킬을 잃었다(기본 프리셋이 정확히 그 조합이었다).
    ///
    /// 이제는 버리는 대신 **부모로 한 단계 내려 다시 묻는다**(`EventTrigger::parent`).
    /// 셧다운이 꺼져 있으면 그 순간은 평범한 킬로 취급되어 `record_kills` 에
    /// 걸리고, 킬마저 꺼져 있으면 그때 비로소 버려진다. 강등된 이름이 그대로
    /// 저장되므로(반환값) 클립 목록·점수·우선순위가 전부 한 이야기를 한다 —
    /// "셧다운은 안 담겠다"고 한 사용자의 라이브러리에 셧다운 클립이 남지 않는다.
    async fn resolve_recordable_trigger(
        &self,
        trigger: &EventTrigger,
        event: &GameEvent,
    ) -> Result<Option<EventTrigger>> {
        let settings = self.settings.read().await;

        // Task 29: exclude the end-of-game highlight for games shorter than the
        // configured minimum (remakes / very short games aren't worth a clip).
        // event.event_time carries the in-game time at GameEnd.
        if matches!(trigger, EventTrigger::GameEnd)
            && (event.event_time as u32) < settings.event_filter.min_game_duration_secs
        {
            debug!(
                "GameEnd ignored: game duration {:.0}s below minimum {}s",
                event.event_time, settings.event_filter.min_game_duration_secs
            );
            return Ok(None);
        }

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
                return Ok(None);
            }
        }
        drop(game_mode);
        drop(queue_id);

        // 감지된 이름에서 시작해 통과할 때까지 부모로 내려간다.
        //
        // 우선순위 문턱도 매 단계 다시 본다 — 강등하면 우선순위가 함께 내려가므로
        // (셧다운 3 -> 킬 1) 한 번만 재는 것은 거짓말이 된다. 반대로 문턱 때문에
        // 막힌 것을 강등으로 우회하지도 않는다: 부모는 언제나 우선순위가 같거나
        // 낮으므로, 문턱에 막힌 트리거는 강등해도 계속 막힌다(의도한 성질이다).
        let mut candidate = trigger.clone();
        // 실제 부모 사슬은 최장 3단(스틸 -> 장로 -> 드래곤)이다. 상한은 나중에
        // 누가 사슬을 늘리다 고리를 만들었을 때 폴링 루프가 멈추지 않게 하는 것.
        const MAX_DEMOTION_STEPS: usize = 8;

        for _ in 0..MAX_DEMOTION_STEPS {
            let passes_priority = candidate.priority() >= settings.event_filter.min_priority;
            let enabled = trigger_enabled(&candidate, &settings.event_filter);

            if passes_priority && enabled {
                if candidate != *trigger {
                    debug!(
                        "Trigger demoted: {:?} -> {:?} (원래 이름의 토글이 꺼져 있음)",
                        trigger, candidate
                    );
                }
                return Ok(Some(candidate));
            }

            match candidate.parent(event) {
                Some(parent) => candidate = parent,
                None => return Ok(None),
            }
        }

        warn!(
            "Trigger demotion chain did not terminate for {:?} — dropping the event",
            trigger
        );
        Ok(None)
    }

    /// Try to process merged events if merge window has closed
    async fn try_process_merged_events(&self) -> Result<()> {
        let settings = self.settings.read().await;
        // Clamp with the SAME bound `arm_merge_flush` uses. If the two disagree, a
        // setting above the cap arms a timer that fires at the cap, finds the window
        // still "open" by this (larger) comparison, disarms, and leaves the queue with
        // no pending timer at all — reintroducing the never-closing merge window the
        // timer exists to prevent.
        let merge_threshold = clamp_merge_threshold_secs(settings.clip_timing.merge_time_threshold);
        drop(settings);
        let merge_threshold = merge_threshold as u64;

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

        // Save the merged clip (normal path waits for post-event footage)
        self.save_event_window_inner(window, true).await?;

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

        // 대표 이벤트를 고른다.
        //
        // 예전에는 `max_by_key(priority)` 로 트리거만 고르고, 정작 메타데이터는
        // `window.events[0]`(= 시간순 첫 이벤트)에서 뽑았다. 둘이 다른 이벤트를
        // 가리키는 것이다. 게다가 `max_by_key` 는 동점일 때 **마지막** 원소를
        // 돌려주는데 킬·포탑·데스가 전부 우선순위 1이라 동점이 흔하다.
        //
        // 실게임 한 판에서 실제로 이렇게 깨졌다:
        //   내 킬 + 포탑  -> `turret_kill` (15점, 정상은 킬 25점)
        //   내 킬 + 데스  -> `Death`       (10점)
        // 훅 자막도 「포탑」·「죽는 장면」으로 나갔다.
        //
        // 이제 우선순위가 같으면 **하이라이트 기본점**으로 가른다(킬 25 > 포탑 15 >
        // 데스 10). 진짜 동점이면 먼저 일어난 쪽이 이긴다 — 그게 창의 앵커다.
        let primary_index = {
            let mut best_index = 0usize;
            let mut best_rank = window_rank(&events[0]);
            for (index, queued) in events.iter().enumerate().skip(1) {
                let rank = window_rank(queued);
                if rank > best_rank {
                    best_rank = rank;
                    best_index = index;
                }
            }
            best_index
        };
        let primary_event = &events[primary_index];
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

        // Wall-clock span of the window. Kept separate from the in-game start/end times
        // above: the rolling video buffer is indexed by wall clock, so that — not game
        // time — is what the clip window must be anchored to.
        let first_event_wall = events
            .iter()
            .map(|e| e.received_wall_secs)
            .fold(f64::INFINITY, f64::min);
        let last_event_wall = events
            .iter()
            .map(|e| e.received_wall_secs)
            .fold(f64::NEG_INFINITY, f64::max);

        // 대표 이벤트를 맨 앞에 둔다 — 소비하는 쪽(`save_merged_clip`)이 `events[0]` 을
        // 대표로 읽으므로, 여기서 순서를 맞춰 두면 그 계약이 구조적으로 성립한다.
        // 나머지는 원래(시간) 순서를 유지한다.
        let mut ordered_events = Vec::with_capacity(events.len());
        ordered_events.push(primary_event.event.clone());
        for (index, queued) in events.iter().enumerate() {
            if index != primary_index {
                ordered_events.push(queued.event.clone());
            }
        }

        Some(EventWindow {
            primary_trigger: primary_event.trigger.clone(),
            events: ordered_events,
            start_time: start_time as f32,
            end_time: end_time as f32,
            priority,
            first_event_wall,
            last_event_wall,
        })
    }

    /// Save a single event without merging.
    ///
    /// `event_wall_secs` is the wall-clock instant the event was detected; the clip is
    /// anchored to it so it stays centred on the play no matter how long the post-event
    /// wait and the extraction itself take.
    async fn save_single_event(
        &self,
        trigger: EventTrigger,
        event: GameEvent,
        event_wall_secs: f64,
    ) -> Result<()> {
        let settings = self.settings.read().await;

        // Calculate clip window duration
        let clip_window = self.calculate_clip_window(&trigger, &settings);
        drop(settings);

        let pre_duration = clip_window.pre_duration as f64;
        let post_duration = clip_window.post_duration as f64;

        // The event really happened, so record it BEFORE anything that can fail or be
        // interrupted (the post-event wait, the extraction slot, the export). Immediate
        // mode used to persist nothing but `ClipMetadata`: `storage.save_events` was
        // called from the merge path only, so a whole game recorded in immediate mode
        // ended up with zero stored events.
        let event_data = build_event_data(&trigger, &event, trigger.priority());
        if let Err(e) = self.persist_events(vec![event_data]).await {
            warn!(
                "Failed to persist event data for {}: {}",
                event.event_name, e
            );
        }

        info!(
            "Event detected: {} (Waiting {}s for post-event capture...)",
            event.event_name, post_duration
        );

        // CRITICAL FIX: Wait for the post-event action to actually happen in the
        // game/recorder. If we save immediately, we cut off the future.
        //
        // The wait holds NO lock: it used to run inside `processing_lock`, so a second
        // event arriving during it timed out after 5s and was dropped. The recorder guard
        // is likewise taken later, inside `save_event_clip`.
        let waited_fully = self
            .sleep_or_cancelled(Duration::from_secs_f64(post_duration))
            .await;

        // A stop cut the wait short: no further footage will be recorded, so anchor the
        // window end at "now" instead of waiting out the coverage timeout for frames that
        // will never exist.
        let effective_post = if waited_fully {
            post_duration
        } else {
            (now_wall_secs() - event_wall_secs)
                .max(0.0)
                .min(post_duration)
        };
        let requested_duration = pre_duration + effective_post;

        info!(
            "Saving clip for event: {} (priority: {}, duration: {:.1}s)",
            event.event_name,
            trigger.priority(),
            requested_duration
        );

        // Generate clip ID
        let clip_id = format!("{}_{}", event.event_name, event.event_time as u32);

        // Serialize clip extraction (one FFmpeg export at a time).
        let _lock = match tokio::time::timeout(PROCESSING_LOCK_TIMEOUT, self.processing_lock.lock())
            .await
        {
            Ok(guard) => guard,
            Err(_) => {
                crate::utils::telemetry::capture_operational_error("clip", "clip_save_timeout");
                error!(
                    "processing_lock acquisition timed out after {:.0}s in save_single_event; \
                     skipping the clip (event data was already persisted)",
                    PROCESSING_LOCK_TIMEOUT.as_secs_f64()
                );
                self.emit_event(
                    "clip-save-failed",
                    serde_json::json!({
                        "event_name": event.event_name,
                        "reason": "clip extraction slot was busy",
                    }),
                )
                .await;
                return Err(anyhow::anyhow!(AppError::ProcessTimeout(
                    "processing_lock acquisition timed out".into()
                )));
            }
        };

        // Get recorder and save clip. The recorder guard is taken AFTER the post-event
        // sleep above — holding a read guard across a multi-second sleep starves every
        // status poll, because tokio's RwLock is write-preferring.
        let recorder = self.recorder.read().await;

        let saved = match recorder
            .save_event_clip(event_wall_secs, pre_duration, effective_post, &clip_id)
            .await
        {
            Ok((path, actual_duration)) => {
                info!(
                    "Clip saved successfully: {:?} ({:.2}s)",
                    path, actual_duration
                );
                Some((path, actual_duration))
            }
            Err(e) => {
                crate::utils::telemetry::capture_operational_error("clip", "clip_save_failed");
                error!("Failed to save clip for event {}: {}", event.event_name, e);
                self.emit_event(
                    "clip-save-failed",
                    serde_json::json!({
                        "event_name": event.event_name,
                        "reason": e.to_string(),
                    }),
                )
                .await;
                None
            }
        };
        drop(recorder);

        // Ghost-metadata guard: a failed extraction used to persist a row pointing at a
        // relative `pending/<id>.mp4` placeholder that nothing ever creates, so the
        // library filled up with entries that could not be played, edited or swept.
        let Some((clip_path, actual_duration)) = saved else {
            return Ok(());
        };

        // Metadata carries the MEASURED clip length, not the requested window: the export
        // is clamped whenever the buffer could not cover the request.
        self.save_clip_metadata(
            &clip_id,
            &trigger,
            &event,
            trigger.priority(),
            &clip_path,
            actual_duration,
            pre_duration,
        )
        .await?;

        Ok(())
    }

    /// Save an event window (merged events).
    ///
    /// `wait_for_post` controls whether we sleep for the post-event duration before
    /// extracting the clip. Normal processing waits so the aftermath is captured; the
    /// game-end flush path passes `false` to save whatever footage already exists.
    async fn save_event_window_inner(
        &self,
        window: EventWindow,
        wait_for_post: bool,
    ) -> Result<()> {
        // The window's events have ALREADY been drained from the queue by the caller, so
        // from here on nothing else can save them: every early return below would erase
        // them. Persist the `EventData` first — the plays really did happen, and they are
        // what the timeline/auto-edit reads — and treat the clip itself as best-effort.
        // (The old code wrote the events last, after a lock acquisition that timed out
        // after 5s whenever a previous window was still sleeping on the same lock: those
        // events vanished without a clip and without a row.)
        if let Err(e) = self.persist_window_events(&window).await {
            warn!("Failed to persist event window data: {}", e);
        }

        let settings = self.settings.read().await;

        // Calculate clip window for primary event
        let clip_window = self.calculate_clip_window(&window.primary_trigger, &settings);
        drop(settings);

        // Extend duration to cover the full event window, capped to prevent
        // absurdly long clips (e.g., when app restarts mid-game and replays all events).
        // The span is measured on the WALL CLOCK (detection instants), not on in-game
        // event times: the clip window is anchored to wall clock, so mixing the two
        // origins would skew it whenever they drift apart.
        const MAX_EVENT_WINDOW_SECS: f64 = 30.0;
        let raw_span = window.last_event_wall - window.first_event_wall;
        // NaN-safe on purpose: a non-finite span degrades to 0 instead of reaching
        // `Duration::from_secs_f64`, which PANICS on NaN. (`clamp` alone would propagate
        // it, so the finiteness check has to come first.)
        let event_window_duration = if raw_span.is_finite() {
            raw_span.clamp(0.0, MAX_EVENT_WINDOW_SECS)
        } else {
            0.0
        };
        let pre_duration = clip_window.pre_duration as f64;
        // Merged window is [first_event - pre, last_event + post], so the post side has
        // to cover the span between the first and last event plus the normal tail.
        let post_duration = event_window_duration + clip_window.post_duration as f64;

        info!(
            "Saving merged clip: {:?} ({} events, priority: {}, duration: {:.1}s)",
            window.primary_trigger,
            window.events.len(),
            window.priority,
            pre_duration + post_duration
        );

        // Use primary event for clip generation
        let primary_event = &window.events[0];
        let clip_id = format!(
            "merged_{}_{}",
            window.start_time as u32, window.end_time as u32
        );

        let waited_fully = if wait_for_post {
            info!(
                "Merged Event Window: Waiting {:.1}s for post-event capture...",
                post_duration
            );
            // The sleep MUST hold no lock. Holding the recorder read guard across a sleep
            // of up to ~33s blocked every status poll behind it (tokio's RwLock is
            // write-preferring), freezing the UI at game end; holding `processing_lock`
            // across it made every window that arrived during the sleep time out and be
            // lost. Both guards are now taken after the wait.
            self.sleep_or_cancelled(Duration::from_secs_f64(post_duration))
                .await
        } else {
            info!("Merged Event Window: flushing without waiting for post-event capture");
            false
        };

        // Flush path (the game is already ending) or a stop that cut the wait short: do
        // NOT anchor the window end in the future. No further footage will arrive, so end
        // it at "now" instead of waiting out the coverage timeout for frames that will
        // never exist.
        let effective_post = if waited_fully {
            post_duration
        } else {
            (now_wall_secs() - window.first_event_wall)
                .max(0.0)
                .min(post_duration)
        };

        // Serialize clip extraction (one FFmpeg export at a time). The flush path runs
        // inside the user-visible stop sequence, so it waits for the slot far less
        // patiently than a normal in-game save.
        let lock_budget = if wait_for_post {
            PROCESSING_LOCK_TIMEOUT
        } else {
            FLUSH_PROCESSING_LOCK_TIMEOUT
        };
        let _lock = match tokio::time::timeout(lock_budget, self.processing_lock.lock()).await {
            Ok(guard) => guard,
            Err(_) => {
                error!(
                    "processing_lock acquisition timed out after {:.0}s in save_event_window; \
                     skipping the clip (the window's event data was already persisted)",
                    lock_budget.as_secs_f64()
                );
                self.emit_event(
                    "clip-save-failed",
                    serde_json::json!({
                        "event_name": primary_event.event_name,
                        "reason": "clip extraction slot was busy",
                    }),
                )
                .await;
                return Err(anyhow::anyhow!(AppError::ProcessTimeout(
                    "processing_lock acquisition timed out".into()
                )));
            }
        };

        // Get recorder and save merged clip using save_event_clip method
        let recorder = self.recorder.read().await;

        let saved = match recorder
            .save_event_clip(
                window.first_event_wall,
                pre_duration,
                effective_post,
                &clip_id,
            )
            .await
        {
            Ok((path, actual_duration)) => {
                info!(
                    "Merged clip saved successfully: {:?} ({:.2}s)",
                    path, actual_duration
                );
                Some((path, actual_duration))
            }
            Err(e) => {
                error!(
                    "Failed to save merged clip for window {:?}: {}",
                    window.primary_trigger, e
                );
                self.emit_event(
                    "clip-save-failed",
                    serde_json::json!({
                        "event_name": primary_event.event_name,
                        "reason": e.to_string(),
                    }),
                )
                .await;
                None
            }
        };
        drop(recorder);

        // Ghost-metadata guard: see save_single_event. A failed extraction must leave no
        // clip row behind — the window's event data was already persisted above, because
        // the events themselves really did happen.
        if let Some((ref clip_path, actual_duration)) = saved {
            // Metadata carries the MEASURED clip length, not the requested window.
            self.save_clip_metadata(
                &clip_id,
                &window.primary_trigger,
                primary_event,
                window.priority,
                clip_path,
                actual_duration,
                pre_duration,
            )
            .await?;
        }

        Ok(())
    }

    /// Persist every event in a merged window (append, never replace).
    async fn persist_window_events(&self, window: &EventWindow) -> Result<()> {
        let event_data: Vec<EventData> = window
            .events
            .iter()
            // The whole window is stored under its PRIMARY trigger — that is what the clip
            // was cut for — matching the previous behaviour.
            .map(|e| build_event_data(&window.primary_trigger, e, window.priority))
            .collect();

        self.persist_events(event_data).await
    }

    /// Append `new_events` to the game's stored event list.
    ///
    /// `Storage::save_events` REPLACES the whole per-game blob (one row per game, upserted
    /// wholesale), so writing only the current window would erase every earlier window's
    /// events. We therefore read-modify-write under `events_write_lock`, skipping entries
    /// that are already stored so a retry (or a flush that races an in-flight save) cannot
    /// double-count.
    async fn persist_events(&self, new_events: Vec<EventData>) -> Result<()> {
        if new_events.is_empty() {
            return Ok(());
        }

        let game_id = match self.current_game_id.read().await.clone() {
            Some(id) => id,
            None => {
                warn!("No current game ID set - event data not saved");
                return Ok(());
            }
        };

        let _write_guard = self.events_write_lock.lock().await;

        let mut stored = self
            .storage
            .load_events(&game_id)
            .context("Failed to load existing event data")?;

        let mut added = 0usize;
        for event in new_events {
            let duplicate = stored.iter().any(|existing| {
                existing.event_id == event.event_id
                    && existing.timestamp.to_bits() == event.timestamp.to_bits()
            });
            if !duplicate {
                stored.push(event);
                added += 1;
            }
        }

        if added == 0 {
            return Ok(());
        }

        self.storage
            .save_events(&game_id, &stored)
            .context("Failed to save event data")?;

        debug!("Persisted {} new event(s) for game {}", added, game_id);
        Ok(())
    }

    /// Calculate clip window (pre/post durations) based on settings and event type
    fn calculate_clip_window(
        &self,
        trigger: &EventTrigger,
        settings: &RecordingSettings,
    ) -> ClipWindow {
        // 사용자가 설정에서 명시적으로 정한 값이 있으면 그것을 최우선으로 쓴다.
        // 설정 파일의 `event_timings` 는 이 이름들로만 키가 붙는다.
        let settings_key = match trigger {
            EventTrigger::Multikill(_) => Some("multikill"),
            EventTrigger::Steal => Some("steal"),
            EventTrigger::Death | EventTrigger::FirstBloodVictim => Some("death"),
            EventTrigger::Assist => Some("assist"),
            EventTrigger::TurretKill => Some("turret"),
            EventTrigger::Outplay1vX(_) | EventTrigger::LowHpOutplay => Some("outplay"),
            EventTrigger::DragonKill | EventTrigger::ElderDragonKill => Some("dragon"),
            EventTrigger::BaronKill => Some("baron"),
            EventTrigger::HeraldKill => Some("herald"),
            EventTrigger::InhibitorKill
            | EventTrigger::VoidgrubsKill
            | EventTrigger::AtakhanKill => Some("objective"),
            EventTrigger::Ace => Some("ace"),
            EventTrigger::GameEnd => Some("game_end"),
            EventTrigger::ChampionKill
            | EventTrigger::FirstBlood
            | EventTrigger::Shutdown
            | EventTrigger::TradeKill => Some("kill"),
        };

        if let Some(key) = settings_key {
            if let Some(timing) = settings.clip_timing.event_timings.get(key) {
                return ClipWindow {
                    pre_duration: timing.pre_duration,
                    post_duration: timing.post_duration,
                };
            }
        }

        // 설정에 없으면 이벤트가 스스로 권장하는 길이를 쓴다.
        //
        // `EventTrigger::pre_duration()/post_duration()` 에는 이벤트별로 조정된
        // 값이 이미 들어 있었는데(게임 끝 30+10, 1vX 15+5, 스틸 20+5 …)
        // **프로덕션 호출부가 하나도 없어서 죽은 코드였다.** 그래서 승리 순간이
        // 13초로 잘리고, 빌드업이 필요한 1v3 역전이 평범한 킬과 같은 길이였다.
        ClipWindow {
            pre_duration: trigger.pre_duration(),
            post_duration: trigger.post_duration(),
        }
    }

    /// Save clip metadata to storage.
    ///
    /// `duration` MUST be the length of the file that was actually produced (as measured
    /// and returned by `save_event_clip`), not the requested window: the export is clamped
    /// whenever the rolling buffer could not cover the request, and auto-edit trims against
    /// this number. It was previously hardcoded to `0.0`, and after that fix it carried the
    /// requested length — both broke clip selection/trimming and target-duration
    /// enforcement.
    // 인자가 여덟이다. 묶을 만한 응집된 덩어리가 없어서(식별자·트리거·이벤트·
    // 측정된 길이·경로는 서로 다른 출처에서 온다) 구조체로 감싸면 호출부가
    // 그 구조체를 만드는 코드로 바뀔 뿐 읽기가 나아지지 않는다.
    #[allow(clippy::too_many_arguments)]
    async fn save_clip_metadata(
        &self,
        clip_id: &str,
        // 강등까지 마친 **최종** 트리거. 저장되는 이름과 점수가 여기서 갈린다.
        trigger: &EventTrigger,
        event: &GameEvent,
        priority: u8,
        clip_path: &std::path::Path,
        duration: f64,
        // `event_offset_secs`: 클립 안에서 하이라이트가 일어나는 지점(= 요청한
        // pre-roll). 저장된 클립이 요청보다 짧으면 앞이 잘렸다는 뜻이므로, 이
        // 값이 길이를 넘지 않도록 호출부가 아니라 이 함수 안에서 조인다.
        event_offset_secs: f64,
    ) -> Result<()> {
        let game_id = self.current_game_id.read().await;

        if let Some(ref game_id) = *game_id {
            // Publish metadata first. Thumbnail FFmpeg work runs later in a dedicated
            // single-slot lane so a saved highlight is visible immediately and JPEG
            // decoding never extends the clip extraction critical section.
            let thumb_at = event_offset_secs.clamp(0.0, (duration - 0.1).max(0.0));

            // 앞이 잘린 클립에서는 이벤트가 그만큼 앞으로 당겨진다. 길이를 넘는
            // 오프셋은 썸네일 추출을 실패시키므로 클립 안으로 조인다.
            let event_offset = if duration > 0.0 {
                Some(event_offset_secs.clamp(0.0, duration))
            } else {
                None
            };

            // 점수는 이벤트에 실려 온 그 순간의 상황으로 낸다.
            //
            // **상황이 안 실려 오면 큰 소리로 알린다.** 실게임 한 판에서 클립 22개가
            // 전부 `score_reasons=[]` 였고 점수가 소수점까지 기본점과 일치했다 —
            // 즉 배수가 하나도 안 걸렸는데, 그게 조용해서 산출물을 열어보기 전까지
            // 아무도 몰랐다. 이 앱의 유일한 차별점("체력 8%에서 펜타킬")이 여기서
            // 죽으면 훅 자막도 제목만 남는다. 조용히 기본점으로 떨어지지 않는다.
            let moment = match event.moment.clone() {
                Some(moment) => moment,
                None => {
                    warn!(
                        "클립 {}: 그 순간의 상황이 전달되지 않았습니다 — 점수가 종류 기본점만 \
                         남고 훅 자막에 이유가 빠집니다 (trigger={:?}, event={})",
                        clip_id, trigger, event.event_name
                    );
                    Default::default()
                }
            };

            let score = crate::recording::highlight_score::score(
                trigger_to_highlight_kind(trigger, event),
                &moment,
            );

            if score.reasons.is_empty() {
                debug!(
                    "클립 {}: 상황 이유 없음 (체력={:?}, 어시={:?}, 생존={:?}v{:?})",
                    clip_id,
                    moment.my_health_ratio,
                    moment.assist_count,
                    moment.allies_alive,
                    moment.enemies_alive
                );
            } else {
                info!(
                    "클립 {} 점수 {:.1} — {:?}",
                    clip_id, score.value, score.reasons
                );
            }

            let metadata = ClipMetadata {
                file_path: clip_path.to_string_lossy().to_string(),
                thumbnail_path: None,
                event_offset_secs: event_offset,
                // 원시 이벤트 이름(`event.event_name`)이 아니라 트리거로 적는다.
                // 이름만 쓰던 동안 더블킬도 셧다운도 전부 `Custom("ChampionKill")`
                // 이라 저장된 뒤에는 서로 구분할 수 없었다.
                event_type: trigger_to_event_type(trigger),
                event_time: event.event_time as f64,
                priority,
                duration,
                created_at: chrono::Utc::now(),
                usage_count: 0,
                highlight_score: Some(score.value),
                score_reasons: score.reasons,
            };

            self.storage
                .save_clip_metadata(game_id, &metadata)
                .context("Failed to save clip metadata")?;

            // Count clips saved this session for the game-end notification.
            self.saved_clip_count.fetch_add(1, Ordering::SeqCst);

            info!(
                "Clip metadata saved: {} (game: {}, duration: {:.1}s)",
                clip_id, game_id, duration
            );

            self.emit_event(
                "clip-saved",
                serde_json::json!({
                    "clip_id": clip_id,
                    "game_id": game_id,
                    "event_type": event.event_name,
                    "event_time": metadata.event_time,
                    "priority": metadata.priority,
                    "duration": metadata.duration,
                    "file_path": metadata.file_path,
                    "created_at": metadata.created_at,
                }),
            )
            .await;

            let thumbnail_lock = Arc::clone(&self.thumbnail_lock);
            let storage = Arc::clone(&self.storage);
            let game_id = game_id.clone();
            let clip_id = clip_id.to_string();
            let clip_path = clip_path.to_path_buf();
            let thumbnail_dir = clip_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf();
            let mut thumbnail_metadata = metadata.clone();
            tokio::spawn(async move {
                let _thumbnail_slot = thumbnail_lock.lock().await;
                match crate::video::thumbnail::generate_event_thumbnail(
                    clip_path.clone(),
                    thumbnail_dir,
                    thumb_at,
                    &clip_id,
                )
                .await
                {
                    Ok(path) => {
                        thumbnail_metadata.thumbnail_path =
                            Some(path.to_string_lossy().to_string());
                        if let Err(e) = storage.save_clip_metadata(&game_id, &thumbnail_metadata) {
                            warn!(
                                "클립 썸네일 메타데이터 갱신 실패({}): {}",
                                clip_path.display(),
                                e
                            );
                        }
                    }
                    Err(e) => {
                        warn!("클립 썸네일 생성 실패({}): {}", clip_path.display(), e);
                    }
                }
            });
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

/// Keeps the in-flight clip-task count accurate no matter how the task ends.
///
/// Constructed BEFORE the work it guards can reach an extraction (on the callback thread
/// for spawned per-event tasks, at arm time for merge-flush timers, at entry for
/// `handle_game_event`) and dropped when that work finishes — including on panic — so the
/// game-end barrier can never wait forever, nor miss a task that was created but not yet
/// polled.
struct InflightGuard(Arc<AtomicUsize>);

impl InflightGuard {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter)
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Build the stored `EventData` record for one game event.
fn build_event_data(trigger: &EventTrigger, event: &GameEvent, priority: u8) -> EventData {
    // Collect participants (killer + assisters)
    let mut participants: Vec<String> = Vec::new();
    if let Some(ref killer) = event.killer_name {
        participants.push(killer.clone());
    }
    if let Some(ref assisters) = event.assisters {
        participants.extend_from_slice(assisters);
    }

    EventData {
        event_id: event.event_id as u64,
        event_type: trigger_to_event_type(trigger),
        timestamp: event.event_time as f64,
        priority,
        participants,
        details: None,
    }
}

/// Clip window timing configuration
#[derive(Debug, Clone)]
struct ClipWindow {
    pre_duration: u32,  // Seconds before event
    post_duration: u32, // Seconds after event
}

/// 병합 창의 **대표 이벤트**를 고르는 순위 — 큰 쪽이 이긴다.
///
/// 1순위는 `priority`(설정의 우선순위 문턱과 같은 눈금이라 이걸 먼저 봐야 한다).
/// 같으면 하이라이트 **기본점**으로 가른다 — 킬·포탑·데스가 전부 우선순위 1이라
/// 동점이 흔한데, 그 셋은 볼 가치가 전혀 다르다(25 / 15 / 10).
///
/// `base()` 는 `f64` 라 `Ord` 가 없으므로 0.01 단위 정수로 옮긴다. 표의 값이 전부
/// 소수점 없는 정수라 정보 손실이 없다.
fn window_rank(queued: &QueuedEvent) -> (u8, i64) {
    let base = trigger_to_highlight_kind(&queued.trigger, &queued.event).base();
    (queued.trigger.priority(), (base * 100.0) as i64)
}

/// 이 트리거를 담기로 되어 있는가 — 트리거 하나와 토글 하나의 대응표.
///
/// `resolve_recordable_trigger` 가 강등 사슬을 돌며 단계마다 이걸 묻는다. 대응이
/// 한 곳에 모여 있어야 "감지되는 트리거인데 대응 토글이 없다"는 빈칸이 컴파일
/// 단계에서 드러난다(`match` 는 전부 나열한다 — `_` 를 쓰지 않는 이유다).
fn trigger_enabled(trigger: &EventTrigger, filter: &EventFilterSettings) -> bool {
    match trigger {
        EventTrigger::ChampionKill => filter.record_kills,
        EventTrigger::Death => filter.record_deaths,
        EventTrigger::Assist => filter.record_assists,
        EventTrigger::FirstBlood => filter.record_first_blood,
        EventTrigger::FirstBloodVictim => filter.record_first_blood_victim,
        EventTrigger::Multikill(_) => filter.record_multikills,
        EventTrigger::DragonKill => filter.record_dragon,
        EventTrigger::BaronKill => filter.record_baron,
        EventTrigger::HeraldKill => filter.record_herald,
        EventTrigger::TurretKill => filter.record_turret,
        EventTrigger::InhibitorKill => filter.record_inhibitor,
        EventTrigger::Ace => filter.record_ace,
        EventTrigger::Steal => filter.record_steal,
        EventTrigger::GameEnd => filter.record_game_end,
        EventTrigger::ElderDragonKill => filter.record_elder,
        EventTrigger::VoidgrubsKill => filter.record_voidgrubs,
        EventTrigger::AtakhanKill => filter.record_atakhan,
        EventTrigger::Shutdown => filter.record_shutdown,
        EventTrigger::Outplay1vX(_) => filter.record_outplay,
        EventTrigger::TradeKill => filter.record_trade_kill,
        EventTrigger::LowHpOutplay => filter.record_low_hp,
    }
}

/// 감지된 트리거를 점수 모델의 종류로 옮긴다.
///
/// 1:1 이 아니다 — 점수는 "얼마나 볼 만한가"를 재므로 감지가 나눈 것을 합치기도
/// 하고(저체력 아웃플레이는 그냥 킬이다, 낮은 체력은 배수로 이미 반영된다),
/// 감지가 합친 것을 나누기도 한다(멀티킬은 단계마다 다른 장면이다).
fn trigger_to_highlight_kind(trigger: &EventTrigger, event: &GameEvent) -> HighlightKind {
    match trigger {
        EventTrigger::ChampionKill => HighlightKind::Kill,
        // 낮은 체력은 `MomentContext` 의 클러치 배수로 붙는다. 여기서 또 올리면
        // 같은 사실을 두 번 세는 셈이다.
        EventTrigger::LowHpOutplay => HighlightKind::Kill,
        EventTrigger::Death => HighlightKind::Death,
        EventTrigger::FirstBloodVictim => HighlightKind::FirstBloodVictim,
        EventTrigger::Assist => HighlightKind::Assist,
        EventTrigger::FirstBlood => HighlightKind::FirstBlood,
        EventTrigger::Multikill(2) => HighlightKind::Doublekill,
        EventTrigger::Multikill(3) => HighlightKind::Triplekill,
        EventTrigger::Multikill(4) => HighlightKind::Quadrakill,
        EventTrigger::Multikill(n) if *n >= 5 => HighlightKind::Pentakill,
        // 0·1 킬짜리 멀티킬은 감지가 만들지 않지만, 열거형이 막지 않으므로 남긴다.
        EventTrigger::Multikill(_) => HighlightKind::Kill,
        EventTrigger::Outplay1vX(n) => HighlightKind::Outplay((*n).min(u8::MAX as u32) as u8),
        EventTrigger::DragonKill => HighlightKind::Dragon,
        EventTrigger::BaronKill => HighlightKind::Baron,
        EventTrigger::HeraldKill => HighlightKind::Herald,
        EventTrigger::TurretKill => HighlightKind::Turret,
        EventTrigger::InhibitorKill => HighlightKind::Inhibitor,
        EventTrigger::Ace => HighlightKind::Ace,
        EventTrigger::Steal => HighlightKind::ObjectiveSteal,
        EventTrigger::ElderDragonKill => HighlightKind::ElderDragon,
        EventTrigger::VoidgrubsKill => HighlightKind::Voidgrubs,
        EventTrigger::AtakhanKill => HighlightKind::Atakhan,
        EventTrigger::Shutdown => HighlightKind::Shutdown,
        EventTrigger::TradeKill => HighlightKind::TradeKill,
        // 이긴 판의 마지막 장면과 진 판의 마지막 장면은 볼 이유가 다르다.
        // `Result` 가 안 오면 진 판으로 보수적으로 잡는다(더 낮은 점수).
        EventTrigger::GameEnd => HighlightKind::GameEnd {
            won: event
                .result
                .as_deref()
                .is_some_and(|r| r.eq_ignore_ascii_case("Win")),
        },
    }
}

/// Convert LiveClientMonitor's EventTrigger to storage's EventType
pub(crate) fn trigger_to_event_type(trigger: &EventTrigger) -> EventType {
    match trigger {
        EventTrigger::ChampionKill => EventType::ChampionKill,
        EventTrigger::Death => EventType::Custom("Death".to_string()),
        EventTrigger::Assist => EventType::Custom("Assist".to_string()),
        EventTrigger::FirstBlood => EventType::FirstBlood,
        EventTrigger::FirstBloodVictim => EventType::Custom("FirstBloodVictim".to_string()),
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

// `convert_live_event` 를 **삭제했다.**
//
// 이름은 "live_client::GameEvent 를 recording::GameEvent 로 변환" 이었지만 둘은
// 애초에 같은 타입이다(`use super::live_client::GameEvent`). 그래서 이 함수가 한
// 일은 필드 일곱 개를 손으로 옮겨 적고 나머지를 `..Default::default()` 로 덮는
// 것뿐이었다 — 즉 **새 필드가 생길 때마다 조용히 버리는 장치**였다.
//
// 실제로 두 개를 버리고 있었다:
//   `moment`  — 그 순간의 체력·생존 인원·어시스트. 하이라이트 점수의 배수가
//               전부 여기 걸려 있어서, 실게임 22개 클립이 전부 `score_reasons=[]`
//               였고 점수가 소수점까지 기본점과 일치했다. 훅 자막 둘째 줄도
//               통째로 비었다.
//   `result`  — 승/패. `GameEnd` 클립의 점수가 이긴 판과 진 판에서 갈리지 않았다.
//
// 같은 타입이므로 변환 자체가 필요 없다. 이벤트를 그대로 넘긴다.

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
            ..Default::default()
        }
    }

    /// Build a queued event with an explicit wall-clock detection instant so the
    /// anchoring maths can be asserted deterministically.
    fn queued_at(trigger: EventTrigger, event_time: f32, wall: f64) -> QueuedEvent {
        QueuedEvent {
            trigger,
            event: create_test_event("ChampionKill", event_time),
            received_at: Instant::now(),
            received_wall_secs: wall,
        }
    }

    #[tokio::test]
    async fn test_merge_events() {
        // Create test events at different times
        let events = vec![
            queued_at(EventTrigger::ChampionKill, 100.0, 1_000.0),
            queued_at(EventTrigger::Multikill(2), 105.0, 1_005.0),
            queued_at(EventTrigger::Multikill(3), 108.0, 1_008.0),
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

        let resolved = manager
            .resolve_recordable_trigger(&trigger, &event)
            .await
            .unwrap();
        assert_eq!(resolved, Some(EventTrigger::ChampionKill));

        // Cleanup
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn clip_metadata_is_saved_under_current_game_id() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");

        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());
        let settings = Arc::new(TokioRwLock::new(RecordingSettings::default()));
        let recorder_config = RecordingConfig {
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let recorder = WindowsCaptureRecorder::new(recorder_config).await.unwrap();
        let recorder_arc = Arc::new(TokioRwLock::new(recorder));
        let manager =
            AutoClipManager::new(Arc::clone(&recorder_arc), Arc::clone(&storage), settings);

        let game_id = "game_metadata_target";
        let metadata = crate::storage::models::GameMetadata {
            game_id: game_id.to_string(),
            champion: "Ahri".to_string(),
            game_mode: "CLASSIC".to_string(),
            start_time: chrono::Utc::now(),
            end_time: None,
            result: None,
            kda: None,
        };
        storage.create_game(game_id, &metadata).unwrap();

        manager.set_current_game(Some(game_id.to_string())).await;
        let clip_path = temp_dir.path().join("clip.mp4");
        manager
            .save_clip_metadata(
                "clip_1",
                &EventTrigger::ChampionKill,
                &create_test_event("ChampionKill", 42.0),
                3,
                &clip_path,
                18.0,
                10.0,
            )
            .await
            .expect("metadata save should succeed");

        let clips = storage.load_clip_metadata(game_id).unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].file_path, clip_path.to_string_lossy());
        assert_eq!(clips[0].priority, 3);
        // Duration must be the real value, not the old hardcoded 0.0
        assert_eq!(clips[0].duration, 18.0);
        assert_eq!(manager.saved_clip_count(), 1);

        manager.set_current_game(None).await;
        manager
            .save_clip_metadata(
                "clip_without_game",
                &EventTrigger::ChampionKill,
                &create_test_event("ChampionKill", 50.0),
                5,
                &temp_dir.path().join("unassigned.mp4"),
                12.0,
                10.0,
            )
            .await
            .expect("missing current game should not fail event processing");

        let clips_after_clear = storage.load_clip_metadata(game_id).unwrap();
        assert_eq!(clips_after_clear.len(), 1);
    }

    #[tokio::test]
    async fn flush_drains_queue_without_ghost_rows_when_extraction_fails() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());
        let settings = Arc::new(TokioRwLock::new(RecordingSettings::default()));
        let recorder_config = RecordingConfig {
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let recorder = WindowsCaptureRecorder::new(recorder_config).await.unwrap();
        let recorder_arc = Arc::new(TokioRwLock::new(recorder));
        let manager = AutoClipManager::new(recorder_arc, Arc::clone(&storage), settings);

        let game_id = "auto_flush_test";
        let metadata = crate::storage::models::GameMetadata {
            game_id: game_id.to_string(),
            champion: "Ahri".to_string(),
            game_mode: "CLASSIC".to_string(),
            start_time: chrono::Utc::now(),
            end_time: None,
            result: None,
            kda: None,
        };
        storage.create_game(game_id, &metadata).unwrap();
        manager.set_current_game(Some(game_id.to_string())).await;

        // Merge mode is ON by default (threshold 15s) so these stay queued, unsaved.
        manager
            .process_event(
                EventTrigger::Multikill(3),
                create_test_event("ChampionKill", 500.0),
            )
            .await
            .unwrap();
        manager
            .process_event(
                EventTrigger::ChampionKill,
                create_test_event("ChampionKill", 503.0),
            )
            .await
            .unwrap();

        assert_eq!(manager.saved_clip_count(), 0);
        assert!(storage.load_clip_metadata(game_id).unwrap().is_empty());

        // Game-end flush drains the queue and ATTEMPTS the save. In this test env the
        // recorder has no segments, so extraction fails — the ghost-metadata guard must
        // then leave NO clip row and NO count bump (the old behavior wrote a bogus
        // `pending/*.mp4` row that poisoned auto-edit and could never be deleted).
        manager.flush_pending_events().await.unwrap();

        assert!(
            storage.load_clip_metadata(game_id).unwrap().is_empty(),
            "failed extraction must not leave a ghost clip row"
        );
        assert_eq!(manager.saved_clip_count(), 0);

        // The events themselves really happened, so they are still persisted.
        let events = storage.load_events(game_id).unwrap();
        assert_eq!(
            events.len(),
            2,
            "flush must persist the window's event data even when extraction fails"
        );

        // A second flush is a no-op (queue already drained) — no duplicate events.
        manager.flush_pending_events().await.unwrap();
        assert_eq!(storage.load_events(game_id).unwrap().len(), 2);
        assert!(storage.load_clip_metadata(game_id).unwrap().is_empty());
    }

    /// Build a manager whose merge window closes after `merge_threshold` seconds.
    async fn manager_with_settings(
        temp_dir: &std::path::Path,
        storage: Arc<Storage>,
        settings: RecordingSettings,
    ) -> AutoClipManager {
        let recorder_config = RecordingConfig {
            output_dir: temp_dir.to_path_buf(),
            ..Default::default()
        };
        let recorder = WindowsCaptureRecorder::new(recorder_config).await.unwrap();
        AutoClipManager::new(
            Arc::new(TokioRwLock::new(recorder)),
            storage,
            Arc::new(TokioRwLock::new(settings)),
        )
    }

    /// **상황 정보가 저장 경로 끝까지 살아서 가는가.**
    ///
    /// 실게임에서 클립 22개가 전부 `score_reasons=[]` 였고 점수가 소수점까지
    /// 기본점과 일치했다. 원인은 `convert_live_event` — 이름은 "변환" 이었지만
    /// 같은 타입을 필드별로 옮겨 적으면서 `..Default::default()` 로 `moment` 와
    /// `result` 를 매번 버리는 장치였다. 새 필드가 생길 때마다 조용히 사라지는
    /// 구조였고, 실제로 두 개가 그렇게 사라졌다.
    ///
    /// 이 테스트는 "이벤트가 큐를 지나 창까지 가면서 `moment` 를 잃지 않는다" 를
    /// 고정한다. 다시 손으로 옮겨 적는 코드가 생기면 여기서 먼저 깨진다.
    #[tokio::test]
    async fn the_moment_survives_the_merge_window() {
        use crate::recording::highlight_score::MomentContext;

        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());
        let manager =
            manager_with_settings(temp_dir.path(), storage, RecordingSettings::default()).await;

        let mut queued = queued_at(EventTrigger::ChampionKill, 100.0, 1_100.0);
        queued.event.moment = Some(MomentContext {
            my_health_ratio: Some(0.08),
            assist_count: Some(0),
            ..Default::default()
        });

        let window = manager.merge_events(&[queued]).expect("window");
        let moment = window.events[0]
            .moment
            .as_ref()
            .expect("상황 정보가 창까지 살아 있어야 한다");

        assert_eq!(moment.my_health_ratio, Some(0.08));
        assert_eq!(moment.assist_count, Some(0));

        // 그리고 그 값이 실제로 점수를 움직여야 한다 — 전달만 되고 안 쓰이면 같은 결함이다.
        let scored = crate::recording::highlight_score::score(
            crate::recording::highlight_score::HighlightKind::Kill,
            moment,
        );
        assert!(
            !scored.reasons.is_empty(),
            "상황이 있는데 이유가 비었다: {:?}",
            scored
        );
    }

    /// 병합 라벨링 회귀 — 킬이 포탑·데스로 둔갑하면 안 된다.
    ///
    /// 실게임 한 판에서 실제로 이렇게 나왔다:
    ///   `merged_433_445` 내 킬 + 포탑 -> `turret_kill` 15점 (정상은 킬 25점)
    ///   `merged_745_750` 내 킬 + 포탑 -> `turret_kill` 15점
    ///   `merged_645_659` 내 킬 + 데스 -> `Death`       10점
    ///
    /// 원인은 둘이었다. ① `max_by_key` 가 동점에서 **마지막**을 돌려주는데 킬·포탑·
    /// 데스가 전부 우선순위 1이다. ② 트리거는 그렇게 고르고 메타데이터는
    /// `events[0]`(시간순 첫)에서 뽑아 서로 다른 이벤트를 가리켰다.
    #[tokio::test]
    async fn a_kill_merged_with_a_turret_stays_a_kill() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());
        let manager =
            manager_with_settings(temp_dir.path(), storage, RecordingSettings::default()).await;

        // 킬이 먼저, 포탑이 나중 — 예전에는 나중 것(포탑)이 대표가 됐다.
        let events = vec![
            queued_at(EventTrigger::ChampionKill, 433.0, 1_433.0),
            queued_at(EventTrigger::TurretKill, 445.0, 1_445.0),
        ];

        let window = manager.merge_events(&events).expect("window");
        assert_eq!(window.primary_trigger, EventTrigger::ChampionKill);
        // 대표 이벤트가 맨 앞에 와야 소비하는 쪽(`events[0]`)이 같은 것을 본다.
        assert_eq!(window.events[0].event_time, 433.0);
    }

    #[tokio::test]
    async fn a_kill_merged_with_a_death_stays_a_kill() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());
        let manager =
            manager_with_settings(temp_dir.path(), storage, RecordingSettings::default()).await;

        let events = vec![
            queued_at(EventTrigger::ChampionKill, 645.0, 1_645.0),
            queued_at(EventTrigger::Death, 659.0, 1_659.0),
        ];

        let window = manager.merge_events(&events).expect("window");
        assert_eq!(window.primary_trigger, EventTrigger::ChampionKill);
        assert_eq!(window.events[0].event_time, 645.0);
    }

    /// 우선순위가 다르면 그쪽이 먼저다 — 기본점 타이브레이크가 우선순위를 덮으면 안 된다.
    #[tokio::test]
    async fn a_higher_priority_trigger_still_wins_over_a_higher_base_score() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());
        let manager =
            manager_with_settings(temp_dir.path(), storage, RecordingSettings::default()).await;

        // 스틸(우선순위 4)이 킬(1)을 이겨야 한다.
        let events = vec![
            queued_at(EventTrigger::ChampionKill, 100.0, 1_100.0),
            queued_at(EventTrigger::Steal, 105.0, 1_105.0),
        ];

        let window = manager.merge_events(&events).expect("window");
        assert_eq!(window.primary_trigger, EventTrigger::Steal);
        assert_eq!(window.events[0].event_time, 105.0);
        assert_eq!(window.priority, 4);
    }

    /// 진짜 동점(같은 종류)이면 먼저 일어난 쪽이 창의 앵커다.
    #[tokio::test]
    async fn a_true_tie_keeps_the_earlier_event_as_primary() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());
        let manager =
            manager_with_settings(temp_dir.path(), storage, RecordingSettings::default()).await;

        let events = vec![
            queued_at(EventTrigger::ChampionKill, 200.0, 1_200.0),
            queued_at(EventTrigger::ChampionKill, 205.0, 1_205.0),
        ];

        let window = manager.merge_events(&events).expect("window");
        assert_eq!(window.events[0].event_time, 200.0);
        // 나머지도 잃지 않는다.
        assert_eq!(window.events.len(), 2);
    }

    /// 강등 규칙 회귀 — 하위 상황을 끈다고 그 순간이 사라지면 안 된다.
    ///
    /// 예전에는 감지가 붙인 이름 하나로만 판정했기 때문에, "킬은 담고 셧다운은
    /// 빼겠다"고 한 사용자의 **셧다운 킬이 통째로 사라졌다**. 기본 프리셋이 정확히
    /// 그 조합이었으므로 아무 설정도 건드리지 않은 사용자가 매 판 손해를 봤다.
    #[tokio::test]
    async fn disabled_sub_situation_demotes_to_its_parent_instead_of_vanishing() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        settings.event_filter.record_kills = true;
        settings.event_filter.record_shutdown = false;
        settings.event_filter.record_multikills = false;
        settings.event_filter.min_priority = 1;

        let manager = manager_with_settings(temp_dir.path(), storage, settings).await;
        let event = create_test_event("ChampionKill", 300.0);

        // 셧다운도 멀티킬도 꺼져 있지만 킬은 켜져 있다 → 평범한 킬로 남는다.
        for trigger in [EventTrigger::Shutdown, EventTrigger::Multikill(3)] {
            let resolved = manager
                .resolve_recordable_trigger(&trigger, &event)
                .await
                .unwrap();
            assert_eq!(
                resolved,
                Some(EventTrigger::ChampionKill),
                "{:?} 는 킬로 강등되어 살아남아야 한다",
                trigger
            );
        }
    }

    /// 강등은 부모까지만이다 — 부모도 꺼져 있으면 그때는 버린다.
    #[tokio::test]
    async fn demotion_stops_when_the_parent_is_also_disabled() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        settings.event_filter.record_kills = false;
        settings.event_filter.record_shutdown = false;
        settings.event_filter.min_priority = 1;

        let manager = manager_with_settings(temp_dir.path(), storage, settings).await;
        let event = create_test_event("ChampionKill", 300.0);

        let resolved = manager
            .resolve_recordable_trigger(&EventTrigger::Shutdown, &event)
            .await
            .unwrap();
        assert_eq!(resolved, None);
    }

    /// 데스를 끈 사용자의 클립에 죽는 장면이 섞이지 않아야 한다.
    ///
    /// `record_trade_kill` 은 기본이 켜짐인데 트레이드킬은 **내가 죽은** 이벤트다.
    /// 부모(데스)를 보지 않던 동안, 데스를 꺼 둔 사용자에게도 죽는 장면이 남았다.
    #[tokio::test]
    async fn trade_kill_respects_the_death_toggle() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        settings.event_filter.record_deaths = false;
        settings.event_filter.record_trade_kill = false;
        settings.event_filter.min_priority = 1;

        let manager = manager_with_settings(temp_dir.path(), storage, settings).await;
        let event = create_test_event("ChampionKill", 300.0);

        let resolved = manager
            .resolve_recordable_trigger(&EventTrigger::TradeKill, &event)
            .await
            .unwrap();
        assert_eq!(resolved, None, "데스를 껐으면 트레이드킬도 빠져야 한다");
    }

    /// "죽는 장면은 됐고 퍼블 당한 것만" — 사용자가 원한 예외 조합.
    #[tokio::test]
    async fn first_blood_victim_can_be_kept_while_deaths_are_off() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        settings.event_filter.record_deaths = false;
        settings.event_filter.record_first_blood_victim = true;
        settings.event_filter.min_priority = 1;

        let manager = manager_with_settings(temp_dir.path(), storage, settings).await;
        let event = create_test_event("ChampionKill", 90.0);

        assert_eq!(
            manager
                .resolve_recordable_trigger(&EventTrigger::FirstBloodVictim, &event)
                .await
                .unwrap(),
            Some(EventTrigger::FirstBloodVictim)
        );
        // 그냥 죽은 것은 여전히 빠진다.
        assert_eq!(
            manager
                .resolve_recordable_trigger(&EventTrigger::Death, &event)
                .await
                .unwrap(),
            None
        );
    }

    /// 스틸은 원본 이벤트에 따라 서로 다른 부모로 내려간다.
    #[tokio::test]
    async fn steal_demotes_along_the_objective_it_came_from() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        settings.event_filter.record_steal = false;
        settings.event_filter.record_baron = true;
        settings.event_filter.record_dragon = true;
        settings.event_filter.record_elder = false;
        settings.event_filter.min_priority = 1;

        let manager = manager_with_settings(temp_dir.path(), storage, settings).await;

        let baron = create_test_event("BaronKill", 1200.0);
        assert_eq!(
            manager
                .resolve_recordable_trigger(&EventTrigger::Steal, &baron)
                .await
                .unwrap(),
            Some(EventTrigger::BaronKill)
        );

        // 장로를 스틸했는데 장로도 꺼져 있으면 두 단계 내려가 일반 드래곤이 된다.
        let elder = GameEvent {
            event_name: "DragonKill".to_string(),
            dragon_type: Some("Elder".to_string()),
            ..create_test_event("DragonKill", 1500.0)
        };
        assert_eq!(
            manager
                .resolve_recordable_trigger(&EventTrigger::Steal, &elder)
                .await
                .unwrap(),
            Some(EventTrigger::DragonKill)
        );
    }

    /// 우선순위 문턱은 강등으로 우회되지 않는다.
    ///
    /// 부모는 언제나 우선순위가 같거나 낮으므로, 문턱에 막힌 트리거는 내려가도
    /// 계속 막혀야 한다 — 그러지 않으면 "중요한 것만" 설정이 조용히 새어 나간다.
    #[tokio::test]
    async fn priority_threshold_is_not_bypassed_by_demotion() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        settings.event_filter.record_kills = true;
        settings.event_filter.record_multikills = false;
        settings.event_filter.min_priority = 3;

        let manager = manager_with_settings(temp_dir.path(), storage, settings).await;
        let event = create_test_event("ChampionKill", 300.0);

        // 더블킬(2)은 문턱 3에 막히고, 킬(1)로 내려가도 여전히 막힌다.
        assert_eq!(
            manager
                .resolve_recordable_trigger(&EventTrigger::Multikill(2), &event)
                .await
                .unwrap(),
            None
        );
    }

    /// 두 계층 표가 같은 것을 말하는지 — 마이그레이션(`reconcile_hierarchy`)과
    /// 강등 사슬(`EventTrigger::parent`)은 서로 다른 파일에 손으로 적혀 있다.
    ///
    /// 어긋나면 증상이 조용하다: 화면은 부모가 켜졌다고 하위 스위치를 감추는데,
    /// 마이그레이션이 그 하위를 켜 주지 않으면 그 순간은 부모 이름으로 강등되어
    /// 저장된다 — 클립은 남지만 셧다운이 평범한 킬 점수를 받는다.
    #[tokio::test]
    async fn reconciled_settings_keep_every_sub_situation_under_its_own_name() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        // 부모만 켠다. 하위는 일부러 전부 끈 상태에서 시작한다.
        for flag in [
            &mut settings.event_filter.record_multikills,
            &mut settings.event_filter.record_shutdown,
            &mut settings.event_filter.record_outplay,
            &mut settings.event_filter.record_low_hp,
            &mut settings.event_filter.record_first_blood,
            &mut settings.event_filter.record_trade_kill,
            &mut settings.event_filter.record_first_blood_victim,
            &mut settings.event_filter.record_elder,
            &mut settings.event_filter.record_steal,
        ] {
            *flag = false;
        }
        settings.event_filter.record_kills = true;
        settings.event_filter.record_deaths = true;
        settings.event_filter.record_dragon = true;
        settings.event_filter.record_baron = true;
        settings.event_filter.min_priority = 1;

        settings.event_filter.reconcile_hierarchy();

        let manager = manager_with_settings(temp_dir.path(), storage, settings).await;

        let kill_event = create_test_event("ChampionKill", 300.0);
        let dragon_event = create_test_event("DragonKill", 900.0);

        let cases: [(EventTrigger, &GameEvent); 8] = [
            (EventTrigger::Shutdown, &kill_event),
            (EventTrigger::Multikill(3), &kill_event),
            (EventTrigger::Outplay1vX(2), &kill_event),
            (EventTrigger::LowHpOutplay, &kill_event),
            (EventTrigger::TradeKill, &kill_event),
            (EventTrigger::FirstBloodVictim, &kill_event),
            (EventTrigger::ElderDragonKill, &dragon_event),
            (EventTrigger::Steal, &dragon_event),
        ];

        for (trigger, event) in cases {
            let resolved = manager
                .resolve_recordable_trigger(&trigger, event)
                .await
                .unwrap();
            assert_eq!(
                resolved,
                Some(trigger.clone()),
                "{:?} 는 강등 없이 제 이름으로 남아야 한다 — reconcile_hierarchy 표에 빠졌는지 확인",
                trigger
            );
        }
    }

    fn seed_game(storage: &Storage, game_id: &str) {
        let metadata = crate::storage::models::GameMetadata {
            game_id: game_id.to_string(),
            champion: "Ahri".to_string(),
            game_mode: "CLASSIC".to_string(),
            start_time: chrono::Utc::now(),
            end_time: None,
            result: None,
            kda: None,
        };
        storage.create_game(game_id, &metadata).unwrap();
    }

    #[tokio::test]
    async fn merge_window_closes_on_its_own_without_a_second_event() {
        // A1 regression: `try_process_merged_events` only ever ran from `process_event`,
        // so a merge window that received exactly one event stayed queued until the next
        // event or game end. With the defaults (merge on, 15s) a lone kill therefore sat
        // there for minutes — by the time it flushed, the 90s rolling buffer had rotated
        // past the play and the "clip" was an empty window. Nothing but a timer can close
        // this window, so the test deliberately sends ONE event and then waits.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        // Keep the test short; the timer waits threshold + margin.
        settings.clip_timing.merge_time_threshold = 1.0;
        let manager = manager_with_settings(temp_dir.path(), Arc::clone(&storage), settings).await;

        let game_id = "merge_timer_test";
        seed_game(&storage, game_id);
        manager.set_current_game(Some(game_id.to_string())).await;

        manager
            .process_event(
                EventTrigger::ChampionKill,
                create_test_event("ChampionKill", 300.0),
            )
            .await
            .unwrap();

        assert_eq!(
            manager.queued_event_count().await,
            1,
            "a merged event must be queued, not saved immediately"
        );

        // No further event is sent: only the flush timer can drain this queue.
        tokio::time::sleep(Duration::from_millis(2_100)).await;

        assert_eq!(
            manager.queued_event_count().await,
            0,
            "merge window must close on its own once the threshold passes"
        );
        // Extraction fails in this environment (the recorder is Idle), so no clip row —
        // but the drain must have happened, which is what the persisted event proves.
        assert_eq!(
            storage.load_events(game_id).unwrap().len(),
            1,
            "the drained window must have been processed"
        );
        assert!(storage.load_clip_metadata(game_id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn stop_waits_for_the_detached_merge_flush_extraction() {
        // Regression (observed in a real ARAM game): the merge-flush timer drains its
        // window OUT of the queue and then owns it, but only the manual path's per-event
        // tasks were counted as in-flight. `stop_event_monitoring` therefore returned
        // while that detached save was still running, `stop_capture_pipeline` stopped the
        // recorder, and the extraction died —
        //   Saving merged clip: Ace (2 events, priority: 4, duration: 27.8s)
        //   Auto Clip Manager: game ended, clearing queue
        //   ERROR Failed to save merged clip for window Ace: 녹화가 진행 중이 아닙니다
        // — losing the last teamfight of the game.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        // Close the merge window quickly so the timer fires inside the test...
        settings.clip_timing.merge_time_threshold = 1.0;
        // ...and give the save a long post-event wait, so it is still pending when the
        // stop begins (the stop's cancel collapses it, exactly as at game end).
        settings.clip_timing.event_timings.insert(
            "kill".to_string(),
            crate::settings::models::EventTiming {
                pre_duration: 5,
                post_duration: 30,
            },
        );
        let manager = manager_with_settings(temp_dir.path(), Arc::clone(&storage), settings).await;

        let game_id = "stop_barrier_test";
        seed_game(&storage, game_id);
        manager.set_current_game(Some(game_id.to_string())).await;

        // Hold the extraction slot BEFORE the timer fires, so the detached save has to
        // queue behind it — that is what makes "did the stop wait?" measurable.
        let slot = Arc::clone(&manager.processing_lock).lock_owned().await;

        manager
            .process_event(
                EventTrigger::ChampionKill,
                create_test_event("ChampionKill", 300.0),
            )
            .await
            .unwrap();

        // Let the flush timer fire (threshold + MERGE_FLUSH_MARGIN) and take ownership of
        // the window.
        tokio::time::sleep(Duration::from_millis(1_900)).await;
        assert_eq!(
            manager.queued_event_count().await,
            0,
            "the merge-flush timer should own the window by now"
        );

        let releaser = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(700)).await;
            drop(slot);
        });

        let started = std::time::Instant::now();
        manager
            .stop_event_monitoring()
            .await
            .expect("stop succeeds");
        let elapsed = started.elapsed();
        releaser.await.unwrap();

        assert!(
            elapsed >= Duration::from_millis(400),
            "stop must wait for the detached merge-flush extraction; it returned after {:?}",
            elapsed
        );
        assert_eq!(
            manager.inflight_clip_tasks.load(Ordering::SeqCst),
            0,
            "no clip work may still be in flight when the caller stops the recorder"
        );
        // The window really was processed. Extraction fails here (the recorder is Idle in
        // tests) so the ghost-metadata guard leaves no clip row — the persisted event data
        // is what proves the detached save ran to completion.
        assert_eq!(storage.load_events(game_id).unwrap().len(), 1);
        assert!(storage.load_clip_metadata(game_id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn stop_proceeds_when_the_game_end_barrier_budgets_expire() {
        // The barrier must never become a hang: a stuck extraction (or a save that can
        // never get the slot) is warned about and abandoned so the stop sequence — and
        // with it `stop_recording` / finalize — always continues.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());
        let manager = manager_with_settings(
            temp_dir.path(),
            Arc::clone(&storage),
            RecordingSettings::default(),
        )
        .await;

        let game_id = "stop_budget_test";
        seed_game(&storage, game_id);
        manager.set_current_game(Some(game_id.to_string())).await;

        // Merging is on by default (15s window), so this event stays queued and the flush
        // has real work to do...
        manager
            .process_event(
                EventTrigger::ChampionKill,
                create_test_event("ChampionKill", 700.0),
            )
            .await
            .unwrap();

        // ...but the extraction slot is held for the whole test, so the flush can never
        // finish, and a task that never completes keeps the drain busy.
        let _slot = Arc::clone(&manager.processing_lock).lock_owned().await;
        let stuck = InflightGuard::new(Arc::clone(&manager.inflight_clip_tasks));

        let started = std::time::Instant::now();
        manager
            .stop_event_monitoring_with_budget(
                Duration::from_millis(300),
                Duration::from_millis(300),
            )
            .await
            .expect("stop must proceed even when both barrier budgets expire");
        let elapsed = started.elapsed();
        drop(stuck);

        assert!(
            elapsed >= Duration::from_millis(500),
            "both budgets should have been spent, got {:?}",
            elapsed
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "the barrier must give up at its budgets instead of waiting on a stuck \
             extraction, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn immediate_mode_persists_event_data() {
        // A7: `storage.save_events` was only ever called from the merge path, so a game
        // recorded with merging OFF ended up with clip files but zero stored events —
        // while `flush_pending_events` claimed those events were "already saved".
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).unwrap());

        let mut settings = RecordingSettings::default();
        settings.clip_timing.merge_consecutive_events = false;
        // post_duration 0 keeps the post-event wait out of the test's runtime.
        settings.clip_timing.event_timings.insert(
            "kill".to_string(),
            crate::settings::models::EventTiming {
                pre_duration: 5,
                post_duration: 0,
            },
        );
        let manager = manager_with_settings(temp_dir.path(), Arc::clone(&storage), settings).await;

        let game_id = "immediate_mode_test";
        seed_game(&storage, game_id);
        manager.set_current_game(Some(game_id.to_string())).await;

        manager
            .process_event(
                EventTrigger::ChampionKill,
                create_test_event("ChampionKill", 120.0),
            )
            .await
            .unwrap();

        let events = storage.load_events(game_id).unwrap();
        assert_eq!(
            events.len(),
            1,
            "immediate mode must persist the event itself, not only the clip"
        );
        assert_eq!(events[0].timestamp, 120.0);
        // The extraction fails (recorder Idle), so there must still be no clip row.
        assert!(storage.load_clip_metadata(game_id).unwrap().is_empty());

        // The flush path discards immediate-mode leftovers, so no duplicates appear.
        manager.flush_pending_events().await.unwrap();
        assert_eq!(storage.load_events(game_id).unwrap().len(), 1);
    }
}
