use std::sync::Arc;

/// 챔피언을 아직 모를 때 넣는 자리표시자.
///
/// 상수로 뽑은 이유: `finish_auto_capture_session` 이 "챔피언이 아직 미상인가"를
/// 이 값과 비교해 판정한다. 한쪽만 바뀌면 그 가드가 조용히 죽어서 챔피언이 영영
/// 갱신되지 않는다 — 실제로 한 번 그 상태로 통과했다.
const UNKNOWN_CHAMPION: &str = "Unknown";
/// 판 모드를 못 얻었을 때의 자리표시자. `finish_auto_capture_session` 이
/// `GameEnd` 요약으로 늦게 채울 수 있는지 판정하는 기준이기도 하다.
const UNKNOWN_GAME_MODE: &str = "UNKNOWN";

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::recording::auto_clip_manager::AutoClipManager;
use crate::recording::live_client::LiveClientBasicInfo;
use crate::recording::RecordingManager;
use crate::storage::{GameMetadata, Storage};

#[derive(Debug, Clone, Default)]
pub struct GameSessionContext {
    pub champion: Option<String>,
    pub game_mode: Option<String>,
}

impl GameSessionContext {
    pub fn from_live_client(info: Option<LiveClientBasicInfo>) -> Self {
        match info {
            Some(info) => Self {
                champion: non_empty(info.champion_name),
                game_mode: non_empty(info.game_mode),
            },
            None => Self::default(),
        }
    }
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn safe_id_part(value: &str) -> String {
    let part: String = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .take(24)
        .collect();

    if part.is_empty() {
        "unknown".to_string()
    } else {
        part
    }
}

pub fn build_game_id(started_at: DateTime<Utc>, context: &GameSessionContext) -> String {
    let timestamp = started_at.format("%Y%m%dT%H%M%SZ");
    let champion = safe_id_part(context.champion.as_deref().unwrap_or("unknown"));
    let game_mode = safe_id_part(context.game_mode.as_deref().unwrap_or("unknown"));
    format!("auto_{}_{}_{}", timestamp, champion, game_mode)
}

pub fn build_game_metadata(
    started_at: DateTime<Utc>,
    context: &GameSessionContext,
) -> GameMetadata {
    let game_id = build_game_id(started_at, context);

    GameMetadata {
        game_id,
        champion: context
            .champion
            .clone()
            .unwrap_or_else(|| UNKNOWN_CHAMPION.to_string()),
        game_mode: context
            .game_mode
            .clone()
            .unwrap_or_else(|| UNKNOWN_GAME_MODE.to_string()),
        start_time: started_at,
        end_time: None,
        result: None,
        kda: None,
    }
}

pub async fn begin_auto_capture_session(
    storage: &Storage,
    recorder: &Arc<RwLock<RecordingManager>>,
    clip_manager: &Arc<AutoClipManager>,
    context: GameSessionContext,
) -> Result<GameMetadata, String> {
    if let Some(existing) = recorder.read().await.get_current_game().await {
        clip_manager
            .set_current_game(Some(existing.game_id.clone()))
            .await;
        info!(
            "Resolved existing auto-capture game session: {}",
            existing.game_id
        );
        return Ok(existing);
    }

    let metadata = build_game_metadata(Utc::now(), &context);
    storage
        .create_game(&metadata.game_id, &metadata)
        .map_err(|error| format!("Failed to create game metadata: {}", error))?;

    recorder
        .read()
        .await
        .set_current_game(Some(metadata.clone()))
        .await;
    clip_manager
        .set_current_game(Some(metadata.game_id.clone()))
        .await;

    info!(
        "Started auto-capture game session {} (champion={}, mode={})",
        metadata.game_id, metadata.champion, metadata.game_mode
    );

    Ok(metadata)
}

pub async fn finish_auto_capture_session(
    storage: &Storage,
    recorder: &Arc<RwLock<RecordingManager>>,
    clip_manager: &Arc<AutoClipManager>,
) -> Result<Option<GameMetadata>, String> {
    let current = recorder.read().await.get_current_game().await;
    let mut finalized = None;
    let mut save_error = None;

    if let Some(mut metadata) = current {
        if metadata.end_time.is_none() {
            metadata.end_time = Some(Utc::now());
        }

        // `GameEnd` 순간에 찍어 둔 전적을 여기서 회수한다.
        //
        // 게임이 끝나면 Live Client API 는 응답을 멈추므로, 그 순간에 찍어 두지
        // 않으면 챔피언도 KDA 도 영영 알 수 없다. 예전에는 길이 추정만 했고
        // `result`/`kda` 는 모든 생성 지점에서 `None` 이었다.
        //
        // 승패는 `GameEnd` 이벤트의 `Result` 필드에서 온다 — **다만 이 필드가 항상
        // 온다고 확인된 바는 없다**(Riot 공식 샘플 페이로드에는 GameEnd 이벤트
        // 자체가 없고 스펙 문서는 접근이 막혀 있다). 그래서 "오면 쓰고, 안 오면
        // 아래 길이 추정으로 떨어진다"로만 설계했다.
        //
        // 회수는 하되 **저장에 성공할 때까지 슬롯을 비우지 않는다** — 저장이
        // 실패하면 이 함수는 current 포인터를 지우고 Err 를 내므로, 여기서 먼저
        // 비워 버리면 재시도할 값이 사라진다.
        let summary = clip_manager.peek_game_summary().await;
        if let Some(ref summary) = summary {
            // `champion` 은 비어 있는 대신 "Unknown" 으로 채워져 오므로
            // (`build_game_metadata`), `is_empty()` 만 보면 이 대입은 영영 일어나지
            // 않는다. 실제로 그 상태로 한 번 통과시킨 적이 있다.
            let champion_unset =
                metadata.champion.is_empty() || metadata.champion == UNKNOWN_CHAMPION;
            if champion_unset && !summary.champion.is_empty() {
                metadata.champion = summary.champion.clone();
            }

            // 판 모드도 같은 이유로 늦게 채운다.
            //
            // 세션은 로딩 화면에서 시작되는 일이 흔하고 그때 Live Client API 는
            // 아직 응답하지 않는다 — 그래서 결과 화면에 "트린다미어 - UNKNOWN"
            // 이 나갔다. 챔피언에는 이 길이 있었는데 모드에는 없었다.
            //
            // `game_id` 는 고치지 않는다. 이미 그 이름으로 디스크에 클립이 쌓였고
            // 저장 계층의 키다 — 화면에 나가는 값만 바로잡는다.
            let mode_unset =
                metadata.game_mode.is_empty() || metadata.game_mode == UNKNOWN_GAME_MODE;
            if mode_unset && !summary.game_mode.is_empty() {
                metadata.game_mode = summary.game_mode.clone();
            }
            metadata.kda = Some(crate::storage::models::KDA {
                kills: summary.kills,
                deaths: summary.deaths,
                assists: summary.assists,
            });
            if let Some(result) = match summary.result.as_deref() {
                Some("Win") => Some(crate::storage::models::GameResult::Win),
                Some("Lose") | Some("Loss") => Some(crate::storage::models::GameResult::Loss),
                _ => None,
            } {
                metadata.result = Some(result);
            }
            info!(
                "게임 요약 회수: {} {}/{}/{} result={:?}",
                metadata.champion, summary.kills, summary.deaths, summary.assists, metadata.result
            );
        }

        // 승패를 끝내 못 얻었을 때의 최소한의 표시: 5분 미만이면 리메이크로 본다.
        // 긴 판은 그냥 미상으로 남긴다 — 틀린 승패를 적느니 비워 두는 편이 낫다.
        if metadata.result.is_none() {
            if let Some(end) = metadata.end_time {
                let secs = (end - metadata.start_time).num_seconds();
                if (0..300).contains(&secs) {
                    metadata.result = Some(crate::storage::models::GameResult::Remake);
                }
            }
        }

        if let Err(error) = storage.save_game_metadata(&metadata.game_id, &metadata) {
            save_error = Some(format!(
                "Failed to finalize game metadata {}: {}",
                metadata.game_id, error
            ));
        } else {
            // 디스크에 남은 뒤에야 슬롯을 비운다.
            if summary.is_some() {
                clip_manager.take_game_summary().await;
            }
            info!("Finalized auto-capture game session {}", metadata.game_id);
        }
        finalized = Some(metadata);
    } else {
        warn!("finish_auto_capture_session called without an active game session");
    }

    clip_manager.set_current_game(None).await;
    recorder.read().await.set_current_game(None).await;

    if let Some(error) = save_error {
        return Err(error);
    }

    Ok(finalized)
}

/// Which caller is driving `start_capture_pipeline`, controlling two behaviors
/// that legitimately differ between the manual and automatic entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureStartMode {
    /// Manual capture: the F8 hotkey and the `start_auto_capture` command.
    ///
    /// This pipeline OWNS event monitoring — it starts the clip manager's own
    /// event-monitoring task after recording begins (and rolls it back on
    /// failure). An already-active recorder here is treated as a hard failure.
    Manual,
    /// Automatic capture: the game-state monitor's game-start callback.
    ///
    /// The game monitor owns event monitoring via its own `LiveClientMonitor`,
    /// so this pipeline does NOT start the clip manager's task. An already-active
    /// recorder means the user preempted auto-detect with manual capture, so the
    /// session is left intact for the adoption path (returns `AlreadyRecording`)
    /// instead of being finalized.
    AutoDetect,
}

/// Result of a successful `start_capture_pipeline` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureStartOutcome {
    /// A fresh recording (and, in `Manual` mode, event monitoring) was started.
    Started,
    /// `AutoDetect` only: the recorder was already active (manual preemption).
    /// The active session was intentionally left intact for the adoption path —
    /// callers must NOT finalize it and should defer to game-monitor adoption.
    AlreadyRecording,
}

/// The three ordered results of `stop_capture_pipeline`.
///
/// The pipeline always runs all three steps in the fixed order
/// (stop event monitoring → stop recording → finalize) so the end-of-game flush
/// happens while the recorder is still alive; each caller decides which errors to
/// propagate versus log.
pub struct CaptureStopOutcome {
    /// Result of stopping event monitoring, which also runs the game-end barrier:
    /// every detached clip extraction is drained and the queue is flushed before it
    /// returns (bounded at 30s total).
    pub event_monitoring: Result<(), String>,
    /// Result of stopping the replay-buffer recording.
    pub recording_stopped: Result<(), String>,
    /// Result of finalizing the game session metadata.
    pub finalized: Result<Option<GameMetadata>, String>,
}

/// Single source of truth for the auto-capture START sequence.
///
/// Order: begin/resolve the game session → start the replay-buffer recording →
/// (Manual mode only) start event monitoring. Each step rolls back the earlier
/// ones on failure so a partial start never leaves a dangling session or an
/// orphaned recording. The three previous copies of this sequence (the
/// `start_auto_capture` command, the F8 hotkey, and the game-start callback)
/// delegate here so the ordering and rollback live in exactly one place.
pub async fn start_capture_pipeline(
    storage: &Storage,
    recorder: &Arc<RwLock<RecordingManager>>,
    clip_manager: &Arc<AutoClipManager>,
    context: GameSessionContext,
    mode: CaptureStartMode,
) -> Result<CaptureStartOutcome, String> {
    // 1. Ensure a game session exists (resolves an already-active one instead of
    //    creating a duplicate).
    begin_auto_capture_session(storage, recorder, clip_manager, context).await?;

    // 2. Start the replay-buffer recording, rolling back the session on failure.
    if let Err(error) = recorder.write().await.start_recording().await {
        let already_recording = matches!(
            recorder.read().await.get_status().await,
            crate::recording::RecordingStatus::Recording
                | crate::recording::RecordingStatus::Buffering
        );
        if already_recording {
            // A LIVE session already owns the recorder. In BOTH modes this is a
            // preemption, not a failure — finalizing here would confirm an early
            // Remake result, clear current_game, and orphan every subsequent clip
            // of the game that is still being recorded.
            return match mode {
                CaptureStartMode::AutoDetect => {
                    // Manual capture (F8 / command) preempted auto-detect; leave
                    // the session for the game monitor's adoption path.
                    info!(
                        "Replay buffer already active (manual capture preemption); \
                         leaving session for the adoption path: {}",
                        error
                    );
                    Ok(CaptureStartOutcome::AlreadyRecording)
                }
                CaptureStartMode::Manual => {
                    // User asked to start while a session is already live (e.g.
                    // poll-lag mis-click during an auto-detect session). Preserve
                    // the old command semantics: report the error, but with NO
                    // side effects on the live session.
                    info!(
                        "Manual capture start requested while already recording; \
                         leaving the live session untouched: {}",
                        error
                    );
                    Err(error.to_string())
                }
            };
        }

        if let Err(cleanup_error) =
            finish_auto_capture_session(storage, recorder, clip_manager).await
        {
            warn!(
                "Failed to clear auto-capture game session after recording start failure: {}",
                cleanup_error
            );
        }
        return Err(error.to_string());
    }

    // 3. In manual mode this pipeline owns event monitoring; start it now and roll
    //    back both recording and the session if it fails. In auto-detect mode the
    //    game monitor owns event monitoring, so this step is skipped.
    if matches!(mode, CaptureStartMode::Manual) {
        if let Err(error) = clip_manager.start_event_monitoring().await {
            if let Err(stop_error) = recorder.write().await.stop_recording().await {
                warn!(
                    "Failed to stop recording after event monitoring start failure: {}",
                    stop_error
                );
            }
            if let Err(cleanup_error) =
                finish_auto_capture_session(storage, recorder, clip_manager).await
            {
                warn!(
                    "Failed to clear auto-capture game session after event monitoring start failure: {}",
                    cleanup_error
                );
            }
            return Err(error.to_string());
        }
    }

    // Emitted last, only once the whole pipeline (recording +, in Manual mode,
    // event monitoring) has genuinely succeeded — an earlier emit would tell the
    // overlay recording is live right before a rollback flips it back off.
    clip_manager
        .emit_event("recording-status", serde_json::json!({ "recording": true }))
        .await;

    Ok(CaptureStartOutcome::Started)
}

/// Single source of truth for the auto-capture STOP sequence.
///
/// Order is load-bearing: `stop_event_monitoring` flushes any queued end-of-game
/// highlights and needs the recorder still alive, so it MUST run before
/// `stop_recording`; the session is finalized last. All three steps always run
/// (a failure in one does not skip the others) and their results are returned so
/// each caller can choose which to propagate versus merely log.
///
/// "Still alive" is not just about the flush this function awaits: clip extractions also
/// run DETACHED (the merge-flush timer, and the per-event task `game_monitor` spawns for
/// every Live Client event). `stop_event_monitoring` therefore drains them before
/// returning — awaiting it is what keeps step 2 from stopping the recorder out from under
/// an extraction that is still running. The whole barrier is bounded at 30s, so a stuck
/// export delays this pipeline but can never hang it.
pub async fn stop_capture_pipeline(
    storage: &Storage,
    recorder: &Arc<RwLock<RecordingManager>>,
    clip_manager: &Arc<AutoClipManager>,
) -> CaptureStopOutcome {
    // 1. Stop event monitoring first — this drains in-flight clip extractions and
    //    flushes queued highlights, all while the recorder is still alive.
    let event_monitoring = clip_manager
        .stop_event_monitoring()
        .await
        .map_err(|error| error.to_string());

    // 2. Stop the replay-buffer recording.
    let recording_stopped = recorder
        .write()
        .await
        .stop_recording()
        .await
        .map(|_| ())
        .map_err(|error| error.to_string());

    // Tell the overlay recording has ended, regardless of whether the recorder
    // itself was already idle — either way the post-condition is "not recording",
    // so this is an idempotent, always-correct signal.
    clip_manager
        .emit_event(
            "recording-status",
            serde_json::json!({ "recording": false }),
        )
        .await;

    // 3. Finalize the game session metadata (persist end_time / coarse result and
    //    clear the current-game pointers).
    let finalized = finish_auto_capture_session(storage, recorder, clip_manager).await;

    CaptureStopOutcome {
        event_monitoring,
        recording_stopped,
        finalized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::integration_backend::RecordingConfig;
    use crate::recording::live_client::PlayerSummary;
    use crate::settings::models::RecordingSettings;

    async fn test_recorder(
        output_dir: &std::path::Path,
    ) -> Arc<RwLock<crate::recording::RecordingManager>> {
        let config = RecordingConfig {
            output_dir: output_dir.to_path_buf(),
            ..Default::default()
        };
        Arc::new(RwLock::new(
            crate::recording::RecordingManager::new(config)
                .await
                .expect("test recorder should initialize"),
        ))
    }

    async fn test_clip_manager(
        recorder: Arc<RwLock<crate::recording::RecordingManager>>,
        storage: Arc<Storage>,
    ) -> Arc<AutoClipManager> {
        Arc::new(AutoClipManager::new(
            recorder,
            storage,
            Arc::new(RwLock::new(RecordingSettings::default())),
        ))
    }

    #[test]
    fn game_id_is_valid_for_storage_commands() {
        let started_at = DateTime::parse_from_rfc3339("2026-05-09T01:02:03Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        let context = GameSessionContext {
            champion: Some("Kai'Sa / Void".to_string()),
            game_mode: Some("CLASSIC".to_string()),
        };

        let game_id = build_game_id(started_at, &context);

        assert_eq!(game_id, "auto_20260509T010203Z_KaiSaVoid_CLASSIC");
        crate::utils::security::validate_game_id(&game_id).expect("game id should be valid");
    }

    #[tokio::test]
    async fn begin_creates_game_and_sets_current_game_id() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage"));
        let recorder = test_recorder(temp_dir.path()).await;
        let clip_manager = test_clip_manager(Arc::clone(&recorder), Arc::clone(&storage)).await;

        let metadata = begin_auto_capture_session(
            &storage,
            &recorder,
            &clip_manager,
            GameSessionContext {
                champion: Some("Ahri".to_string()),
                game_mode: Some("CLASSIC".to_string()),
            },
        )
        .await
        .expect("session starts");

        assert_eq!(
            recorder
                .read()
                .await
                .get_current_game()
                .await
                .expect("recorder current game")
                .game_id,
            metadata.game_id
        );
        assert_eq!(
            clip_manager.current_game_id_for_tests().await,
            Some(metadata.game_id.clone())
        );
        assert_eq!(
            storage
                .load_game_metadata(&metadata.game_id)
                .expect("saved metadata")
                .champion,
            "Ahri"
        );
    }

    #[tokio::test]
    async fn begin_resolves_existing_game_session() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage"));
        let recorder = test_recorder(temp_dir.path()).await;
        let clip_manager = test_clip_manager(Arc::clone(&recorder), Arc::clone(&storage)).await;

        let first = begin_auto_capture_session(
            &storage,
            &recorder,
            &clip_manager,
            GameSessionContext {
                champion: Some("Lux".to_string()),
                game_mode: Some("ARAM".to_string()),
            },
        )
        .await
        .expect("first session");
        let second = begin_auto_capture_session(
            &storage,
            &recorder,
            &clip_manager,
            GameSessionContext {
                champion: Some("Different".to_string()),
                game_mode: Some("CLASSIC".to_string()),
            },
        )
        .await
        .expect("existing session");

        assert_eq!(first.game_id, second.game_id);
        assert_eq!(
            clip_manager.current_game_id_for_tests().await,
            Some(first.game_id)
        );
    }

    #[tokio::test]
    async fn finish_finalizes_and_clears_current_game() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage"));
        let recorder = test_recorder(temp_dir.path()).await;
        let clip_manager = test_clip_manager(Arc::clone(&recorder), Arc::clone(&storage)).await;

        let metadata = begin_auto_capture_session(
            &storage,
            &recorder,
            &clip_manager,
            GameSessionContext {
                champion: Some("Jinx".to_string()),
                game_mode: Some("CLASSIC".to_string()),
            },
        )
        .await
        .expect("session starts");

        let finalized = finish_auto_capture_session(&storage, &recorder, &clip_manager)
            .await
            .expect("session finishes")
            .expect("finalized game");

        assert_eq!(finalized.game_id, metadata.game_id);
        assert!(finalized.end_time.is_some());
        assert!(recorder.read().await.get_current_game().await.is_none());
        assert!(clip_manager.current_game_id_for_tests().await.is_none());
        assert!(storage
            .load_game_metadata(&metadata.game_id)
            .expect("saved metadata")
            .end_time
            .is_some());
    }

    /// 로딩 중에 시작된 판은 챔피언도 모드도 모른 채로 굳는다.
    ///
    /// 실기기에서 결과 화면에 "트린다미어 - UNKNOWN" 으로 나갔다. 챔피언에는
    /// `GameEnd` 요약으로 늦게 채우는 길이 있었는데 모드에는 없었다.
    #[tokio::test]
    async fn game_mode_is_filled_in_from_the_game_end_summary() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage"));
        let recorder = test_recorder(temp_dir.path()).await;
        let clip_manager = test_clip_manager(Arc::clone(&recorder), Arc::clone(&storage)).await;

        // Live Client API 가 아직 응답하지 않는 시점 — 둘 다 못 얻은 채 시작한다.
        let metadata = begin_auto_capture_session(
            &storage,
            &recorder,
            &clip_manager,
            GameSessionContext::default(),
        )
        .await
        .expect("session starts");
        assert_eq!(metadata.game_mode, UNKNOWN_GAME_MODE);

        *clip_manager.summary_slot().write().await = Some(PlayerSummary {
            champion: "Tryndamere".to_string(),
            kills: 12,
            deaths: 2,
            assists: 4,
            result: Some("Win".to_string()),
            game_mode: "ARAM".to_string(),
        });

        let finalized = finish_auto_capture_session(&storage, &recorder, &clip_manager)
            .await
            .expect("session finishes")
            .expect("finalized game");

        assert_eq!(finalized.game_mode, "ARAM");
        assert_eq!(finalized.champion, "Tryndamere");
        // 저장된 것도 같아야 한다 — 화면은 디스크에서 읽는다.
        assert_eq!(
            storage
                .load_game_metadata(&metadata.game_id)
                .expect("saved metadata")
                .game_mode,
            "ARAM"
        );
        // `game_id` 는 그대로다. 이미 그 이름으로 클립이 쌓였고 저장 계층의 키다.
        assert_eq!(finalized.game_id, metadata.game_id);
    }

    /// 이미 아는 모드를 요약이 덮어쓰지 않는다.
    #[tokio::test]
    async fn a_known_game_mode_is_not_overwritten() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage"));
        let recorder = test_recorder(temp_dir.path()).await;
        let clip_manager = test_clip_manager(Arc::clone(&recorder), Arc::clone(&storage)).await;

        begin_auto_capture_session(
            &storage,
            &recorder,
            &clip_manager,
            GameSessionContext {
                champion: Some("Jinx".to_string()),
                game_mode: Some("CLASSIC".to_string()),
            },
        )
        .await
        .expect("session starts");

        *clip_manager.summary_slot().write().await = Some(PlayerSummary {
            champion: "Jinx".to_string(),
            kills: 1,
            deaths: 1,
            assists: 1,
            result: None,
            game_mode: "ARAM".to_string(),
        });

        let finalized = finish_auto_capture_session(&storage, &recorder, &clip_manager)
            .await
            .expect("session finishes")
            .expect("finalized game");

        assert_eq!(finalized.game_mode, "CLASSIC");
    }

    #[tokio::test]
    async fn stop_pipeline_finalizes_and_clears_session_even_when_idle() {
        // The recorder is idle (recording was never started), so `stop_recording`
        // is expected to fail — but event-monitoring stop and finalize must still
        // run, and the session must be cleared. This exercises the fixed ordering
        // and the "always finalize" guarantee without needing FFmpeg.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage"));
        let recorder = test_recorder(temp_dir.path()).await;
        let clip_manager = test_clip_manager(Arc::clone(&recorder), Arc::clone(&storage)).await;

        let metadata = begin_auto_capture_session(
            &storage,
            &recorder,
            &clip_manager,
            GameSessionContext {
                champion: Some("Sett".to_string()),
                game_mode: Some("CLASSIC".to_string()),
            },
        )
        .await
        .expect("session starts");

        let outcome = stop_capture_pipeline(&storage, &recorder, &clip_manager).await;

        // No monitoring task was ever started, so stopping it is a clean no-op.
        assert!(outcome.event_monitoring.is_ok());
        // Recorder was idle, so stopping the recording surfaces an error...
        assert!(outcome.recording_stopped.is_err());
        // ...but the session is still finalized regardless.
        let finalized = outcome
            .finalized
            .expect("finalize should succeed")
            .expect("a finalized game");
        assert_eq!(finalized.game_id, metadata.game_id);
        assert!(finalized.end_time.is_some());

        // Session pointers cleared.
        assert!(recorder.read().await.get_current_game().await.is_none());
        assert!(clip_manager.current_game_id_for_tests().await.is_none());
        assert!(storage
            .load_game_metadata(&metadata.game_id)
            .expect("saved metadata")
            .end_time
            .is_some());
    }

    #[tokio::test]
    async fn stop_pipeline_flushes_queued_highlights_inside_the_live_session() {
        // The end-of-game flush must run BEFORE `stop_recording` and BEFORE the session is
        // finalized. Both are observable without FFmpeg: `persist_events` silently drops
        // everything once `current_game_id` has been cleared (step 3), so persisted event
        // data proves the flush ran while the session was still live — the same window in
        // which the recorder is still alive and `save_event_clip` would be allowed to run.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage"));
        let recorder = test_recorder(temp_dir.path()).await;
        let clip_manager = test_clip_manager(Arc::clone(&recorder), Arc::clone(&storage)).await;

        let metadata = begin_auto_capture_session(
            &storage,
            &recorder,
            &clip_manager,
            GameSessionContext {
                champion: Some("Ahri".to_string()),
                game_mode: Some("ARAM".to_string()),
            },
        )
        .await
        .expect("session starts");

        // Merging is on by default (15s window), so this event is still queued when the
        // game ends — exactly the "last teamfight" case that used to be lost.
        clip_manager
            .process_event(
                crate::recording::live_client::EventTrigger::Ace,
                crate::recording::live_client::GameEvent {
                    event_id: 7,
                    event_name: "Ace".to_string(),
                    event_time: 1_337.0,
                    killer_name: Some("TestPlayer".to_string()),
                    victim_name: None,
                    assisters: Some(vec![]),
                    dragon_type: None,
                    ..Default::default()
                },
            )
            .await
            .expect("event is queued");
        assert_eq!(clip_manager.queued_event_count().await, 1);

        let outcome = stop_capture_pipeline(&storage, &recorder, &clip_manager).await;

        assert!(outcome.event_monitoring.is_ok());
        assert!(outcome
            .finalized
            .expect("finalize should succeed")
            .is_some());

        assert_eq!(
            clip_manager.queued_event_count().await,
            0,
            "the queue must be drained by the flush, not by the game-end clear"
        );
        assert_eq!(
            storage
                .load_events(&metadata.game_id)
                .expect("stored events")
                .len(),
            1,
            "the flush must run while the session is still live, or its event data is \
             dropped and the highlight is lost"
        );
    }

    #[tokio::test]
    async fn stop_pipeline_is_safe_with_no_active_session() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage"));
        let recorder = test_recorder(temp_dir.path()).await;
        let clip_manager = test_clip_manager(Arc::clone(&recorder), Arc::clone(&storage)).await;

        let outcome = stop_capture_pipeline(&storage, &recorder, &clip_manager).await;

        assert!(outcome.event_monitoring.is_ok());
        assert!(outcome.recording_stopped.is_err());
        // No active session — finalize succeeds with None rather than erroring.
        assert!(matches!(outcome.finalized, Ok(None)));
    }

    // NOTE: `start_capture_pipeline` drives the concrete recorder's
    // `start_recording`, which spawns a real FFmpeg capture when FFmpeg is
    // available (as it is on dev machines). There is no way to mock that from a
    // unit test, so the start pipeline's begin → record → monitor sequencing is
    // covered by the recorder integration tests
    // (`tests/integration/windows_capture_recorder_tests.rs`) plus the shared
    // logic exercised by the deterministic stop-pipeline tests above, rather than
    // by a flaky/hanging unit test here.
}
