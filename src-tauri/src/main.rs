// 릴리스 빌드에서 Windows의 추가 콘솔 창 방지, 제거하지 마세요!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;
// Import everything from the library crate
use lolshorts::*;

#[tauri::command]
async fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let startup_start = std::time::Instant::now();

    // .env 파일에서 환경 변수 로드 (개발용)
    dotenvy::dotenv().ok();

    // Sentry 크래시 리포팅 초기화 (opt-in, SENTRY_DSN 환경변수 필요)
    let _sentry_guard = std::env::var("SENTRY_DSN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse().ok())
        .map(|dsn| {
            sentry::init(sentry::ClientOptions {
                dsn: Some(dsn),
                release: sentry::release_name!(),
                auto_session_tracking: true,
                ..Default::default()
            })
        });

    // 애플리케이션 데이터 디렉토리 가져오기 (로깅 초기화 전에 수행)
    let app_data_dir = match dirs::data_dir() {
        Some(dir) => dir.join("lolshorts"),
        None => {
            eprintln!("Error: Cannot determine application data directory. Please ensure your system has a valid user data folder.");
            std::process::exit(1);
        }
    };

    // 로그 디렉토리 생성
    let log_dir = app_data_dir.join("logs");
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("Warning: Failed to create log directory: {}", e);
    }

    // 파일 로거 설정 (일별 회전)
    let file_appender = tracing_appender::rolling::daily(&log_dir, "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // 로깅 초기화 (파일 + 표준 출력)
    // 개발 모드에서는 표준 출력도 활성화, 프로덕션에서는 파일 위주 (원한다면 둘 다 가능)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(non_blocking) // 파일에 기록
        .with_ansi(false) // 파일에는 ANSI 색상 코드 제거
        .init();

    tracing::info!("LoLShorts 애플리케이션 시작 중...");
    tracing::info!("로그 디렉토리: {:?}", log_dir);
    tracing::info!(
        startup_ms = startup_start.elapsed().as_millis(),
        "Application started"
    );

    // 환경변수 유효성 검사
    let env_check = utils::env_validation::validate_env();
    if !env_check.required_missing.is_empty() {
        tracing::warn!("필수 환경변수 누락: {:?}", env_check.required_missing);
    }
    if !env_check.optional_missing.is_empty() {
        tracing::info!(
            "선택적 환경변수 미설정 (기능이 제한될 수 있음): {:?}",
            env_check.optional_missing
        );
    }
    let startup_issues = Arc::new(RwLock::new(Vec::<String>::new()));
    let recording_disk_monitor = Arc::new(RwLock::new(None::<tokio::sync::watch::Sender<bool>>));

    // 저장소(Storage) 초기화
    let storage = match storage::Storage::new(&app_data_dir) {
        Ok(s) => Arc::new(s),
        Err(primary_err) => {
            tracing::error!(
                "Failed to initialize storage at {:?}: {}",
                app_data_dir,
                primary_err
            );
            let fallback_dir = std::env::temp_dir().join("lolshorts-recovery");
            match storage::Storage::new(&fallback_dir) {
                Ok(s) => {
                    let message = format!(
                        "Primary storage failed at {:?}: {}. Running with recovery storage at {:?}.",
                        app_data_dir, primary_err, fallback_dir
                    );
                    tracing::warn!("{}", message);
                    startup_issues.write().await.push(message);
                    Arc::new(s)
                }
                Err(fallback_err) => {
                    tracing::error!(
                        "Recovery storage initialization failed at {:?}: {}",
                        fallback_dir,
                        fallback_err
                    );
                    eprintln!(
                        "Error: Storage initialization failed: {}. Recovery storage also failed: {}. Check disk space and permissions.",
                        primary_err, fallback_err
                    );
                    std::process::exit(1);
                }
            }
        }
    };
    let runtime_data_dir = storage.base_path().to_path_buf();

    // 인증 관리자(Auth Manager) 초기화
    let auth = Arc::new(auth::AuthManager::new());

    // 녹화 디렉토리 초기화
    let mut recordings_dir = runtime_data_dir.join("recordings");
    if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
        tracing::error!(
            "Failed to create recordings directory at {:?}: {}",
            recordings_dir,
            e
        );
        let fallback_recordings_dir = std::env::temp_dir().join("lolshorts-recordings");
        match std::fs::create_dir_all(&fallback_recordings_dir) {
            Ok(()) => {
                let message = format!(
                    "Primary recordings directory failed at {:?}: {}. Using recovery recordings directory {:?}.",
                    recordings_dir, e, fallback_recordings_dir
                );
                tracing::warn!("{}", message);
                startup_issues.write().await.push(message);
                recordings_dir = fallback_recordings_dir;
            }
            Err(fallback_err) => {
                tracing::error!(
                    "Recovery recordings directory failed at {:?}: {}",
                    fallback_recordings_dir,
                    fallback_err
                );
                eprintln!(
                    "Error: Cannot create recordings folder: {}. Recovery folder also failed: {}. Check disk space and permissions.",
                    e, fallback_err
                );
                std::process::exit(1);
            }
        }
    }

    // 플랫폼 최적화 및 마이그레이션을 포함한 녹화 설정 로드
    let recording_settings =
        match settings::models::RecordingSettings::load_with_platform_optimization().await {
            Ok(settings) => {
                tracing::info!("플랫폼 최적화된 설정이 성공적으로 로드되었습니다");
                settings
            }
            Err(e) => {
                tracing::warn!("최적화된 설정 로드 실패, 마이그레이션 시도: {}", e);

                // 마이그레이션 시도 (폴백)
                match settings::models::RecordingSettings::migrate_settings().await {
                    Ok(migrated_settings) => {
                        tracing::info!("설정이 성공적으로 마이그레이션되었습니다");
                        migrated_settings
                    }
                    Err(migration_error) => {
                        tracing::error!("설정 마이그레이션 실패: {}, 기본값 사용", migration_error);
                        settings::models::RecordingSettings::default()
                    }
                }
            }
        };
    let recording_settings = Arc::new(RwLock::new(recording_settings));

    tracing::info!("녹화 설정이 로드되고 플랫폼에 최적화되었습니다");

    // 비디오 및 오디오 구성을 포함한 녹화 관리자(플랫폼별 백엔드) 초기화
    let settings_read = recording_settings.read().await;
    let audio_config = Some(settings_read.audio.to_audio_config());
    let encoder_pref = match settings_read.video.encoder {
        settings::models::EncoderPreference::Auto => "auto",
        settings::models::EncoderPreference::Nvenc => "nvenc",
        settings::models::EncoderPreference::Qsv => "qsv",
        settings::models::EncoderPreference::Amf => "amf",
        settings::models::EncoderPreference::Software => "software",
    };
    let video_config = Some(recording::VideoSettingsConfig {
        resolution: settings_read.video.get_resolution(),
        fps: settings_read.video.get_fps(),
        bitrate: settings_read.video.get_bitrate(),
        use_h265: settings_read.video.is_h265(),
        encoder_preference: encoder_pref.to_string(),
    });
    drop(settings_read);

    let recording_manager: Arc<RwLock<recording::RecordingManager>> =
        match recording::initialize_recording_backend_full(
            recordings_dir.clone(),
            audio_config.clone(),
            video_config.clone(),
        )
        .await
        {
            Ok(manager) => Arc::new(RwLock::new(manager)),
            Err(e) => {
                tracing::error!("Failed to initialize recording backend: {}", e);
                let fallback_recordings_dir = std::env::temp_dir().join("lolshorts-recordings");
                if fallback_recordings_dir != recordings_dir {
                    match recording::initialize_recording_backend_full(
                        fallback_recordings_dir.clone(),
                        audio_config.clone(),
                        video_config,
                    )
                    .await
                    {
                        Ok(manager) => {
                            let message = format!(
                                "Recording backend failed for {:?}: {}. Using recovery recordings directory {:?}.",
                                recordings_dir, e, fallback_recordings_dir
                            );
                            tracing::warn!("{}", message);
                            startup_issues.write().await.push(message);
                            recordings_dir = fallback_recordings_dir;
                            Arc::new(RwLock::new(manager))
                        }
                        Err(fallback_err) => {
                            tracing::error!(
                                "Recovery recording backend initialization failed: {}",
                                fallback_err
                            );
                            eprintln!("Error: Recording system initialization failed: {}. Recovery backend also failed: {}. Check if FFmpeg is available and audio devices are accessible.", e, fallback_err);
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Error: Recording system initialization failed: {}. Check if FFmpeg is available and audio devices are accessible.", e);
                    std::process::exit(1);
                }
            }
        };

    tracing::info!(
        "{}용 녹화 백엔드 초기화 완료 (오디오 지원 포함)",
        recording::Platform::current().name()
    );

    tracing::info!("크로스 플랫폼 녹화 백엔드 초기화됨");

    // 자동 클립 관리자(Auto Clip Manager) 초기화
    let auto_clip_manager = Arc::new(recording::auto_clip_manager::AutoClipManager::new(
        Arc::clone(&recording_manager),
        Arc::clone(&storage),
        Arc::clone(&recording_settings),
    ));

    tracing::info!("자동 클립 관리자 초기화됨");

    // 핫키 관리자(Hotkey Manager) 초기화
    let hotkey_manager = Arc::new(hotkey::HotkeyManager::new());

    tracing::info!("핫키 관리자 초기화됨");

    // 메트릭 수집기(Metrics Collector) 초기화
    let metrics_collector = Arc::new(utils::metrics::MetricsCollector::new(
        utils::metrics::HealthThresholds::default(),
        recordings_dir.clone(),
    ));

    tracing::info!("메트릭 수집기 초기화됨");

    // 정리 관리자(Cleanup Manager) 초기화
    let cleanup_config = utils::cleanup::CleanupConfig::default();
    let cleanup_manager = Arc::new(utils::cleanup::CleanupManager::new(
        app_data_dir.clone(),
        cleanup_config,
    ));

    // 시작 시 정리 실행
    if let Err(e) = cleanup_manager.cleanup_on_startup().await {
        tracing::error!("시작 시 정리 실패: {}", e);
    }

    tracing::info!("정리 관리자 초기화됨");

    // 저장 공간 보존 정책(자동 삭제/최대 용량) 적용: 기동 1회 + 6시간 간격 반복.
    // 매 사이클마다 recording_settings를 다시 읽어 사용자가 설정 화면에서
    // 바꾼 값을 재시작 없이 반영한다(캐시된 스냅샷을 쓰지 않음).
    {
        let retention_storage = Arc::clone(&storage);
        let retention_settings = Arc::clone(&recording_settings);
        let retention_cleanup_manager = Arc::clone(&cleanup_manager);

        let startup_settings_snapshot = retention_settings.read().await.storage.clone();
        if let Err(e) = retention_cleanup_manager
            .run_retention_cycle(&retention_storage, &startup_settings_snapshot)
            .await
        {
            tracing::error!("시작 시 저장 공간 보존 정책 적용 실패: {}", e);
        }

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 60 * 60));
            interval.tick().await; // 첫 tick은 즉시 완료됨 — 위의 기동 1회 실행과 중복 방지
            loop {
                interval.tick().await;
                let settings_snapshot = retention_settings.read().await.storage.clone();
                if let Err(e) = retention_cleanup_manager
                    .run_retention_cycle(&retention_storage, &settings_snapshot)
                    .await
                {
                    tracing::error!("저장 공간 보존 정책 적용 실패: {}", e);
                }
            }
        });
    }

    // 자동 편집(Auto-edit) 기능을 위한 Auto Composer 초기화
    let video_processor = match video::VideoProcessor::new() {
        Ok(processor) => {
            tracing::info!("✅ VideoProcessor가 성공적으로 초기화되었습니다");
            Arc::new(processor)
        }
        Err(e) => {
            tracing::warn!(
                "⚠️ 최적 설정으로 VideoProcessor 초기화 실패: {}. 폴백을 사용합니다.",
                e
            );
            Arc::new(video::VideoProcessor::new_with_fallback())
        }
    };

    let auto_composer = {
        let mut composer =
            video::AutoComposer::new(Arc::clone(&video_processor), Arc::clone(&storage));
        // 산출물을 %TEMP% 대신 앱 관리 디렉토리에 보존 (중간 산출물은 자동 정리)
        composer.set_output_root(runtime_data_dir.join("exports").join("auto_edit"));
        let audio = recording_settings.read().await.audio.clone();
        composer.set_normalize_audio(if audio.audio_normalize {
            Some(audio.audio_target_lufs)
        } else {
            None
        });
        Arc::new(composer)
    };

    tracing::info!("Auto Composer 초기화됨");

    // YouTube 관리자 초기화
    let youtube_manager = init_youtube_manager(Arc::clone(&storage)).await;
    tracing::info!("YouTube 관리자 초기화됨");

    // 자동 녹화를 위한 게임 상태 모니터(Game State Monitor) 초기화
    let game_monitor = Arc::new(recording::game_monitor::GameStateMonitor::new(Arc::clone(
        &auto_clip_manager,
    )));

    // LCU 클라이언트 초기화
    let lcu_client = Arc::new(tokio::sync::Mutex::new(lcu::LcuClient::new()));

    // 게임 모니터 콜백을 위한 참조 복제
    let recording_manager_for_monitor = Arc::clone(&recording_manager);
    let clip_manager_for_monitor = Arc::clone(&auto_clip_manager);
    let game_monitor_for_callbacks = Arc::clone(&game_monitor);

    // Overlay: shared handle set during setup, read by game callbacks
    let overlay_app_handle: Arc<tokio::sync::Mutex<Option<tauri::AppHandle>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    let overlay_handle_for_setup = Arc::clone(&overlay_app_handle);
    let overlay_handle_for_start = Arc::clone(&overlay_app_handle);
    let overlay_handle_for_end = Arc::clone(&overlay_app_handle);
    let overlay_settings = Arc::clone(&recording_settings);
    let recording_disk_monitor_for_start = Arc::clone(&recording_disk_monitor);
    let recording_disk_monitor_for_end = Arc::clone(&recording_disk_monitor);
    let storage_for_game_lifecycle = Arc::clone(&storage);

    let app_state = AppState {
        storage,
        auth,
        recording_manager: Arc::clone(&recording_manager),
        clip_manager: Arc::clone(&auto_clip_manager),
        game_monitor: Arc::clone(&game_monitor),
        recording_settings,
        hotkey_manager: Arc::clone(&hotkey_manager),
        metrics_collector: Arc::clone(&metrics_collector),
        cleanup_manager: Arc::clone(&cleanup_manager),
        auto_composer,
        video_processor,
        youtube_manager,
        lcu_client,
        startup_issues: Arc::clone(&startup_issues),
        recording_disk_monitor: Arc::clone(&recording_disk_monitor),
    };

    // 콜백과 함께 핫키 시스템 시작 (설정에서 핫키 읽기)
    let recording_manager_hotkey = Arc::clone(&recording_manager);
    let auto_clip_manager_hotkey = Arc::clone(&auto_clip_manager);
    let startup_issues_hotkey = Arc::clone(&startup_issues);
    // B4 fix: F8/F9는 게임 자동 감지(game_start_callback/game_end_callback)와 별개
    // 진입점이라 오버레이 show/hide가 없었다 — 아래에서 같은 handle/settings로
    // 대칭 적용한다(apply_overlay_show_for_hotkey/apply_overlay_hide_for_hotkey).
    let overlay_handle_for_hotkey = Arc::clone(&overlay_app_handle);
    let recording_settings_for_hotkey = Arc::clone(&app_state.recording_settings);
    let hotkey_settings = app_state.recording_settings.read().await.hotkeys.clone();
    let hotkey_config = hotkey::HotkeyConfig {
        manual_save_clip: hotkey_settings.manual_save_clip,
        toggle_recording: hotkey_settings.toggle_recording,
        delete_last_clip: hotkey_settings.delete_last_clip,
    };

    // TODO: Replace with spawn_monitored once the inner closure types support UnwindSafe
    tokio::spawn(async move {
        let storage_hotkey = Arc::clone(&storage_for_game_lifecycle);
        let hotkey_result = hotkey_manager
            .start_with_config(
                move |event| {
                    let rm = Arc::clone(&recording_manager_hotkey);
                    let acm = Arc::clone(&auto_clip_manager_hotkey);
                    let storage = Arc::clone(&storage_hotkey);
                    let overlay_handle = Arc::clone(&overlay_handle_for_hotkey);
                    let overlay_cfg = Arc::clone(&recording_settings_for_hotkey);

                    tokio::spawn(async move {
                        use hotkey::HotkeyEvent;

                        match event {
                            HotkeyEvent::ToggleAutoCapture => {
                                // 자동 캡처가 실행 중인지 확인
                                let is_monitoring = acm.is_monitoring().await
                                    || rm.read().await.get_current_game().await.is_some();

                                if is_monitoring {
                                    // 자동 캡처 중지 — 순서/롤백은 stop_capture_pipeline 단일 소스
                                    tracing::info!("핫키 F8: 자동 캡처 중지");
                                    let outcome =
                                        recording::game_lifecycle::stop_capture_pipeline(
                                            &storage, &rm, &acm,
                                        )
                                        .await;
                                    if let Err(e) = outcome.event_monitoring {
                                        tracing::error!("자동 캡처 중지 실패: {}", e);
                                    }
                                    if let Err(e) = outcome.recording_stopped {
                                        tracing::error!("리플레이 버퍼 중지 실패: {}", e);
                                    }
                                    if let Err(e) = outcome.finalized {
                                        tracing::error!("핫키 자동 캡처 게임 종료 처리 실패: {}", e);
                                    }
                                    // B4 fix: game_end_callback과 대칭으로 오버레이를 숨긴다
                                    // (개별 outcome 필드 실패 여부와 무관하게 무조건 — hide는
                                    // 멱등이라 안전하다).
                                    apply_overlay_hide_for_hotkey(&overlay_handle).await;
                                } else {
                                    // 자동 캡처 시작 — 순서/롤백은 start_capture_pipeline 단일 소스
                                    tracing::info!("핫키 F8: 자동 캡처 시작");
                                    let session_context =
                                        recording::game_lifecycle::GameSessionContext::from_live_client(
                                            recording::live_client::check_live_client_basic().await,
                                        );
                                    match recording::game_lifecycle::start_capture_pipeline(
                                        &storage,
                                        &rm,
                                        &acm,
                                        session_context,
                                        recording::game_lifecycle::CaptureStartMode::Manual,
                                    )
                                    .await
                                    {
                                        Ok(_) => {
                                            // B4 fix: game_start_callback과 대칭으로, 캡처가
                                            // 실제로 시작됐을 때만 오버레이를 띄운다.
                                            apply_overlay_show_for_hotkey(
                                                &overlay_handle,
                                                &overlay_cfg,
                                            )
                                            .await;
                                        }
                                        Err(e) => {
                                            tracing::error!("핫키 자동 캡처 시작 실패: {}", e);
                                        }
                                    }
                                }
                            }
                            HotkeyEvent::SaveReplay60 => {
                                // 최근 60초 저장 - 녹화 중단 없이 클립 추출
                                tracing::info!("핫키 F9: 60초 리플레이 저장");
                                match rm.read().await.save_last_seconds(60).await {
                                    Ok((path, measured_secs)) => {
                                        tracing::info!("60초 리플레이 저장됨: {:?}", path);
                                        // 메타데이터 저장 — 없으면 라이브러리/편집기에
                                        // 나타나지 않는 고아 파일이 된다
                                        if let Err(e) =
                                            recording::commands::persist_manual_replay_metadata(
                                                &storage,
                                                &acm,
                                                &path,
                                                measured_secs,
                                            )
                                            .await
                                        {
                                            tracing::warn!(
                                                "수동 리플레이 메타데이터 저장 실패: {}",
                                                e
                                            );
                                        }
                                    }
                                    Err(e) => tracing::error!("60초 리플레이 저장 실패: {}", e),
                                }
                            }
                            HotkeyEvent::DeleteLastClip => {
                                // 가장 최근에 저장된 클립 삭제
                                tracing::info!("핫키 F10: 마지막 클립 삭제");
                                match storage.delete_last_clip() {
                                    Ok(Some(path)) => {
                                        tracing::info!("마지막 클립 삭제됨: {}", path);
                                    }
                                    Ok(None) => tracing::info!("삭제할 클립이 없습니다"),
                                    Err(e) => tracing::error!("마지막 클립 삭제 실패: {}", e),
                                }
                            }
                        }
                    });
                },
                hotkey_config,
            )
            .await;

        if let Err(e) = hotkey_result {
            tracing::error!("핫키 시스템 시작 실패: {}", e);
            startup_issues_hotkey
                .write()
                .await
                .push(format!("Hotkey system unavailable: {}", e));
        }

        // 자동 녹화를 위한 게임 모니터링 시작
        let recording_manager_start = Arc::clone(&recording_manager_for_monitor);
        let clip_manager_start = Arc::clone(&clip_manager_for_monitor);
        let storage_for_game_start = Arc::clone(&storage_for_game_lifecycle);
        let storage_for_game_end = Arc::clone(&storage_for_game_lifecycle);
        let clip_manager_for_end = Arc::clone(&clip_manager_for_monitor);

        // 게임 시작 콜백 정의
        let game_start_callback = move || {
            let recording_mgr = Arc::clone(&recording_manager_start);
            let clip_mgr = Arc::clone(&clip_manager_start);
            let storage = Arc::clone(&storage_for_game_start);
            let overlay_handle = Arc::clone(&overlay_handle_for_start);
            let overlay_cfg = Arc::clone(&overlay_settings);
            let disk_monitor = Arc::clone(&recording_disk_monitor_for_start);

            async move {
                tracing::info!("🎮 게임 감지됨! 자동 녹화 시작...");

                let live_client_info = recording::live_client::check_live_client_basic().await;
                let session_context =
                    recording::game_lifecycle::GameSessionContext::from_live_client(
                        live_client_info,
                    );
                // 세션 begin → 녹화 시작 순서·에러 처리·수동 선점(adoption) 분기는
                // game_lifecycle::start_capture_pipeline이 단일 소스로 관리한다.
                match recording::game_lifecycle::start_capture_pipeline(
                    &storage,
                    &recording_mgr,
                    &clip_mgr,
                    session_context,
                    recording::game_lifecycle::CaptureStartMode::AutoDetect,
                )
                .await
                {
                    Ok(recording::game_lifecycle::CaptureStartOutcome::Started) => {}
                    Ok(recording::game_lifecycle::CaptureStartOutcome::AlreadyRecording) => {
                        // 수동(F8) 캡처 선점 — 세션을 finalize하지 않고 게임 모니터의
                        // 채택(adoption) 경로에 맡긴다.
                        tracing::info!("수동 캡처 선점 감지 — 세션 채택 경로로 위임");
                        return Err(
                            "recording already active (manual capture preemption)".to_string()
                        );
                    }
                    Err(e) => return Err(e),
                }

                if let Some(handle) = overlay_handle.lock().await.as_ref().cloned() {
                    let recordings_dir = recording_mgr.read().await.get_config().output_dir.clone();
                    recording::commands::start_recording_disk_monitor_with_sender(
                        handle,
                        Arc::clone(&disk_monitor),
                        recordings_dir,
                    )
                    .await;
                }

                // Show overlay if enabled
                let overlay_enabled = overlay_cfg.read().await.overlay_enabled;
                if overlay_enabled {
                    if let Some(handle) = overlay_handle.lock().await.as_ref() {
                        overlay::show_overlay(handle);
                    }
                }

                tracing::info!("✅ 자동 녹화가 성공적으로 시작되었습니다");
                Ok(())
            }
        };

        // 게임 종료 콜백 정의
        let game_end_callback = move || {
            let recording_mgr = Arc::clone(&recording_manager_for_monitor);
            let clip_mgr = Arc::clone(&clip_manager_for_end);
            let storage = Arc::clone(&storage_for_game_end);
            let overlay_handle = Arc::clone(&overlay_handle_for_end);
            let disk_monitor = Arc::clone(&recording_disk_monitor_for_end);

            async move {
                tracing::info!("⏹️ 게임 종료. 자동 녹화 중지...");

                recording::commands::stop_recording_disk_monitor_with_sender(disk_monitor).await;

                // Hide overlay
                if let Some(handle) = overlay_handle.lock().await.as_ref() {
                    overlay::hide_overlay(handle);
                }

                // 종료 시퀀스(이벤트 flush → 녹화 중지 → finalize)는
                // game_lifecycle::stop_capture_pipeline이 단일 소스로 관리한다.
                let outcome = recording::game_lifecycle::stop_capture_pipeline(
                    &storage,
                    &recording_mgr,
                    &clip_mgr,
                )
                .await;
                if let Err(e) = &outcome.event_monitoring {
                    tracing::warn!("이벤트 모니터링 중지 중 오류: {}", e);
                }
                if let Err(e) = &outcome.recording_stopped {
                    tracing::error!("리플레이 버퍼 중지 실패: {}", e);
                }
                if let Err(e) = &outcome.finalized {
                    tracing::error!("게임 메타데이터 종료 처리 실패: {}", e);
                }
                outcome.recording_stopped?;
                outcome.finalized?;

                tracing::info!("✅ 자동 녹화 중지됨");
                Ok(())
            }
        };

        // 게임 모니터링 시작
        if let Err(e) = game_monitor_for_callbacks
            .start_monitoring(game_start_callback, game_end_callback)
            .await
        {
            tracing::error!("게임 모니터링 시작 실패: {}", e);
        } else {
            tracing::info!(
                "🔍 게임 상태 모니터링 시작됨 - League of Legends가 감지되면 자동으로 녹화합니다"
            );
        }
    });

    let auto_clip_manager_for_setup = Arc::clone(&auto_clip_manager);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(app_state)
        .setup(move |app| {
            // Store app handle for overlay (used by game monitor callbacks) and wire
            // it into AutoClipManager so it can emit `recording-status` / `clip-saved`
            // / `clip-save-failed` / `game-event` to the overlay + dashboard — an
            // AppHandle only exists once the app has started, so this is the earliest
            // point either can be wired in.
            {
                let handle = app.handle().clone();
                let acm = Arc::clone(&auto_clip_manager_for_setup);
                tauri::async_runtime::spawn(async move {
                    acm.set_app_handle(handle.clone()).await;
                    *overlay_handle_for_setup.lock().await = Some(handle);
                    tracing::info!("Overlay app handle stored for game callbacks");
                });
            }

            // 시스템 트레이 설정
            if let Err(e) = tray::setup_tray(app.handle()) {
                tracing::error!("시스템 트레이 설정 실패: {}", e);
            }

            // minimize_to_tray 설정 적용 (비동기 설정 읽기를 위해 blocking spawn)
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // 설정에서 minimize_to_tray 값 확인
                let settings = settings::models::RecordingSettings::load_with_platform_optimization().await;
                let minimize_to_tray = settings.map(|s| s.minimize_to_tray).unwrap_or(true);
                tray::setup_close_to_tray(&handle, minimize_to_tray);
            });

            // Auto-updater initialization
            // The plugin's dialog:true config in tauri.conf.json handles the update UI automatically.
            match utils::health::configured_updater_pubkey() {
                Some(pubkey) => {
                    app.handle().plugin(
                        tauri_plugin_updater::Builder::new()
                            .pubkey(pubkey)
                            .build(),
                    )?;
                    tracing::info!("Auto-updater plugin initialized with configured public key");
                }
                None => {
                    tracing::warn!(
                        "Auto-updater disabled: TAURI_UPDATER_PUBKEY is not configured for this build"
                    );
                }
            }

            // Start YouTube scheduled upload background executor (with panic catching)
            let scheduler_handle = app.handle().clone();
            spawn_monitored("youtube_scheduler", async move {
                crate::youtube::commands::start_upload_scheduler(scheduler_handle).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 인증(Auth) 명령
            auth::commands::login,
            auth::commands::signup,
            auth::commands::logout,
            auth::commands::get_user_status,
            auth::commands::get_license_info,
            auth::commands::get_user_license,
            auth::commands::get_current_entitlement,
            auth::commands::refresh_token,
            auth::commands::set_session,
            // 결제(Payment) 명령
            auth::commands::get_subscription_details,
            auth::commands::cancel_subscription,
            auth::commands::open_payment_page,
            auth::commands::confirm_payment,
            // 녹화(Recording) 명령
            recording::commands::get_unified_game_status,
            recording::commands::set_recording_target,
            recording::commands::get_replay_target_candidates,
            recording::commands::notify_replay_launched,
            recording::commands::get_recording_readiness,
            recording::commands::start_recording,
            recording::commands::stop_recording,
            recording::commands::get_recording_status,
            recording::commands::get_detailed_recording_status,
            recording::commands::start_auto_capture,
            recording::commands::stop_auto_capture,
            recording::commands::save_replay,
            recording::commands::get_saved_clips,
            recording::commands::clear_saved_clips,
            recording::commands::list_audio_devices,
            recording::commands::list_system_audio_devices,
            recording::commands::list_microphone_devices,
            recording::commands::refresh_audio_devices,
            recording::commands::get_audio_devices_with_cache_info,
            recording::commands::get_recording_quality_info,
            recording::commands::detect_available_encoders,
            recording::commands::get_disk_usage_info,
            recording::commands::cleanup_temp_files,
            recording::commands::get_memory_pool_stats,
            recording::commands::get_performance_stats,
            recording::commands::get_recording_backend_info,
            // 비디오(Video) 명령
            video::commands::get_clips,
            video::commands::extract_clip,
            video::commands::compose_shorts,
            video::commands::compose_shorts_v2,
            video::commands::create_longform_video,
            video::commands::generate_thumbnail,
            video::commands::generate_clip_thumbnail,
            video::commands::get_video_duration,
            video::commands::delete_clip,
            // 자동 편집(Auto-edit) 명령
            video::commands::start_auto_edit,
            video::commands::get_auto_edit_progress,
            // 캔버스 템플릿 명령
            video::commands::save_canvas_template,
            video::commands::load_canvas_template,
            video::commands::list_canvas_templates,
            video::commands::delete_canvas_template,
            // LCU 명령
            lcu::commands::connect_lcu,
            lcu::commands::check_lcu_status,
            lcu::commands::get_current_game,
            lcu::commands::is_in_game,
            lcu::commands::get_lcu_metrics,
            lcu::commands::refresh_lcu_caches,
            lcu::commands::list_match_history,
            lcu::commands::download_replay,
            lcu::commands::get_replay_status,
            lcu::commands::launch_replay,
            lcu::commands::get_game_participants,
            // 저장소(Storage) 명령
            storage::commands::list_games,
            storage::commands::get_game_metadata,
            storage::commands::save_game_metadata,
            storage::commands::get_game_events,
            storage::commands::save_game_events,
            storage::commands::save_clip_metadata,
            storage::commands::delete_game,
            storage::commands::get_storage_stats,
            storage::commands::get_dashboard_stats,
            storage::commands::list_clips,
            storage::commands::get_auto_edit_quota,
            storage::commands::get_auto_edit_results,
            storage::commands::get_auto_edit_result,
            storage::commands::delete_auto_edit_result,
            storage::commands::update_auto_edit_youtube_status,
            // 설정(Settings) 명령
            settings::commands::get_recording_settings,
            settings::commands::save_recording_settings,
            settings::commands::reset_settings_to_default,
            // 플랫폼 구성 명령
            settings::commands::detect_platform_config,
            settings::commands::get_recommended_settings,
            settings::commands::validate_settings_for_platform,
            settings::commands::optimize_settings_for_platform,
            settings::commands::check_settings_migration_needed,
            settings::commands::migrate_settings,
            settings::commands::load_settings_optimized,
            settings::commands::export_settings_backup,
            settings::commands::import_settings_backup,
            settings::commands::get_settings_diagnostics,
            // 유틸리티(Utils) 명령
            utils::commands::get_recording_metrics,
            utils::commands::get_system_metrics,
            utils::commands::get_health_status,
            utils::commands::get_diagnostics_status,
            utils::commands::export_diagnostics_bundle,
            utils::commands::get_app_version,
            utils::commands::force_cleanup,
            utils::commands::get_disk_space_info,
            utils::commands::show_in_folder,
            utils::commands::open_file_with_default_app,
            utils::commands::check_file_exists,
            utils::commands::get_ffmpeg_info,
            utils::commands::get_hardware_encoders,
            utils::commands::get_video_encoders,
            // 통계 명령
            video::commands::get_clip_statistics,
            video::commands::reset_clip_statistics,
            // YouTube 명령
            youtube::commands::youtube_start_auth,
            youtube::commands::youtube_start_auth_with_server,
            youtube::commands::youtube_complete_auth,
            youtube::commands::youtube_get_auth_status,
            youtube::commands::youtube_upload_video,
            youtube::commands::youtube_get_upload_progress,
            youtube::commands::youtube_get_video_details,
            youtube::commands::youtube_get_upload_history,
            youtube::commands::youtube_add_to_history,
            youtube::commands::youtube_get_quota_info,
            youtube::commands::youtube_logout,
            youtube::commands::youtube_schedule_upload,
            youtube::commands::youtube_get_upload_queue,
            youtube::commands::youtube_cancel_scheduled_upload,
            // 비디오 효과(Video Effects) 명령
            video::commands::apply_slow_motion_cmd,
            video::commands::apply_color_grading_cmd,
            video::commands::apply_text_overlay_cmd,
            video::commands::apply_chained_effects_cmd,
            // GIF 내보내기
            video::commands::export_as_gif,
            // 비디오 내보내기(Export) 명령
            video::commands::export_video,
            // Autostart 명령
            set_autostart,
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| {
            tracing::error!("Failed to run Tauri application: {}", e);
            eprintln!("Fatal Error: LoLShorts failed to start: {}", e);
            std::process::exit(1);
        });

    // RunEvent 핸들러: 마지막 창을 닫거나 트레이에서 종료할 때(둘 다 결국
    // AppHandle::exit → ExitRequested를 거친다) 진행 중인 녹화를 먼저 정지하고,
    // 그 다음 세그먼트 정리(cleanup_on_shutdown)를 수행한다. managed state
    // (recording_manager)의 Drop은 이 종료 경로들에서 실행되지 않으므로, 여기서
    // 직접 정지하지 않으면 창 없는 FFmpeg가 좀비로 남아 15Mbps로 세그먼트를
    // 계속 기록한다.
    //
    // B7 fix: 이 종료 시퀀스는 이제 여기 한 곳에서만 실행된다. 트레이 "종료"
    // 메뉴와 main 창 닫기(minimize_to_tray=false)는 각자 정리를 수행하지 않고
    // `exit(0)`만 호출해 이 핸들러로 위임한다(tray.rs 참고) — 예전에는 트레이
    // 경로가 직접 정리를 수행한 뒤 `handle.exit(0)`이 이 핸들러를 다시 트리거해
    // 정지 시도가 2번 실행됐고, 반대로 main 창 닫기 단독 종료에서는
    // cleanup_on_shutdown이 전혀 실행되지 않는 비대칭이 있었다.
    app.run(move |app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            let app_handle = app_handle.clone();
            let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();

            // RunEvent 콜백은 동기(non-async)로 호출되므로, 이 스레드에서 직접
            // block_on하지 않고 별도 태스크에 위임한 뒤 채널로 결과를 기다린다
            // (같은 런타임에서 이 스레드를 block_on으로 점유하면, 다른 태스크가
            // 필요한 락을 쥔 채 양보하지 않을 경우 데드락 위험이 있다).
            tauri::async_runtime::spawn(async move {
                // 세그먼트 파일 정리(cleanup_on_shutdown)보다 먼저 녹화를 정지해야
                // 한다 — 그렇지 않으면 FFmpeg가 아직 쓰고 있는 세그먼트 파일을
                // 정리 로직이 지우려 드는 경쟁 상태가 생긴다.
                tray::stop_recording_before_exit(&app_handle).await;

                let state = app_handle.state::<AppState>();
                if let Err(e) = state.cleanup_manager.cleanup_on_shutdown().await {
                    tracing::error!("Shutdown cleanup failed: {}", e);
                }
                let _ = done_tx.send(());
            });

            // B3 fix: stop_recording_before_exit 내부 상한은 이제
            // STOP_TIMEOUT(3s) + FFMPEG_PROBE_TIMEOUT(2s) + pid당
            // FFMPEG_KILL_TIMEOUT(2s)이다(tray.rs 참고) — 예전에는 내부 폴백
            // (probe 5s + pid당 kill 5s, stop 5s 이후에야 시작)의 합이 이 바깥
            // 대기(8s)보다 커서, 폴백이 taskkill을 실행하기 전에 바깥 대기가
            // 먼저 끝나 종료가 진행돼 버릴 수 있었다(예산 역전). 여기 대기를
            // 15s로 늘려 내부 상한 합 + cleanup_on_shutdown 시간을 항상
            // 넉넉히 감당하게 한다.
            if done_rx
                .recv_timeout(std::time::Duration::from_secs(15))
                .is_err()
            {
                tracing::error!(
                    "앱 종료 전 녹화 정리가 15초 내에 끝나지 않았습니다 - 종료를 계속 진행합니다"
                );
            }
        }
    });
}

/// Show the overlay window (if `overlay_enabled` in settings) for a capture
/// session started from the global F8 hotkey handler (B4 fix).
///
/// Mirrors the `game_start_callback` overlay logic above, but that closure only
/// fires for auto-detected games — the F8 hotkey path (`CaptureStartMode::Manual`)
/// previously showed no overlay at all despite emitting `recording-status`, so the
/// REC indicator/toast events went to a hidden webview. Settings are re-read on
/// every call (not cached) so a toggle in Settings applies to the very next
/// capture without a restart.
async fn apply_overlay_show_for_hotkey(
    overlay_handle: &Arc<tokio::sync::Mutex<Option<tauri::AppHandle>>>,
    overlay_settings: &Arc<RwLock<settings::models::RecordingSettings>>,
) {
    let overlay_enabled = overlay_settings.read().await.overlay_enabled;
    if overlay_enabled {
        if let Some(handle) = overlay_handle.lock().await.as_ref() {
            overlay::show_overlay(handle);
        }
    }
}

/// Hide the overlay window for a capture session stopped from the F8 hotkey
/// handler (symmetric counterpart to `apply_overlay_show_for_hotkey`, B4 fix).
/// Unconditional like `game_end_callback`'s hide call — `hide_overlay` no-ops
/// harmlessly if the window was never shown.
async fn apply_overlay_hide_for_hotkey(
    overlay_handle: &Arc<tokio::sync::Mutex<Option<tauri::AppHandle>>>,
) {
    if let Some(handle) = overlay_handle.lock().await.as_ref() {
        overlay::hide_overlay(handle);
    }
}

/// Spawn a monitored background task that logs panics instead of silently crashing.
///
/// Wraps the task in an outer `tokio::spawn` that awaits the inner `JoinHandle`.
/// If the inner task panics, tokio propagates the panic as a `JoinError`, which is
/// caught here and logged.
fn spawn_monitored<F>(task_name: &'static str, future: F) -> tokio::task::JoinHandle<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let handle = tokio::spawn(future);
        if let Err(e) = handle.await {
            if e.is_panic() {
                tracing::error!("Background task '{}' panicked: {:?}", task_name, e);
            } else {
                tracing::warn!("Background task '{}' was cancelled", task_name);
            }
        }
    })
}

/// Validate OAuth redirect URI for security: must be localhost only.
fn validate_redirect_uri(uri: &str, platform: &str) -> bool {
    if uri.is_empty() {
        tracing::info!("{} OAuth disabled (no redirect URI configured)", platform);
        return false;
    }
    let Some(rest) = uri.strip_prefix("http://") else {
        tracing::error!("{} redirect URI must be localhost, got: {}", platform, uri);
        return false;
    };

    let authority = rest.split('/').next().unwrap_or_default();
    let Some((host, port)) = authority.split_once(':') else {
        if matches!(authority, "localhost" | "127.0.0.1") {
            return true;
        }
        tracing::error!("{} redirect URI must be localhost, got: {}", platform, uri);
        return false;
    };

    let valid_host = matches!(host, "localhost" | "127.0.0.1");
    let valid_port = !port.is_empty() && port.chars().all(|character| character.is_ascii_digit());
    if !valid_host || !valid_port {
        tracing::error!("{} redirect URI must be localhost, got: {}", platform, uri);
        return false;
    }

    true
}

/// YouTube 관리자 초기화 (환경변수 기반 자격증명)
async fn init_youtube_manager(storage: Arc<storage::Storage>) -> Arc<youtube::YouTubeManager> {
    let youtube_client_id = std::env::var("YOUTUBE_CLIENT_ID").ok().filter(|v| {
        !v.is_empty() && !v.contains("your-client-id") && v.ends_with(".apps.googleusercontent.com")
    });
    let youtube_client_secret = std::env::var("YOUTUBE_CLIENT_SECRET")
        .ok()
        .filter(|v| !v.is_empty() && !v.contains("your-client-secret"));
    let youtube_redirect_uri = std::env::var("YOUTUBE_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:9090/oauth/callback".to_string());

    let youtube_redirect_uri_valid = validate_redirect_uri(&youtube_redirect_uri, "YouTube");
    let youtube_disabled_uri = "http://localhost:9090/oauth/callback".to_string();

    let manager = match (youtube_client_id, youtube_client_secret) {
        (Some(client_id), Some(client_secret)) if youtube_redirect_uri_valid => {
            match youtube::YouTubeManager::new(
                client_id,
                client_secret,
                youtube_redirect_uri,
                Arc::clone(&storage),
            ) {
                Ok(m) => Arc::new(m),
                Err(e) => {
                    tracing::warn!("YouTube manager init failed - platform disabled: {}", e);
                    Arc::new(
                        youtube::YouTubeManager::new(
                            String::new(),
                            String::new(),
                            youtube_disabled_uri,
                            Arc::clone(&storage),
                        )
                        .unwrap_or_else(|e2| {
                            tracing::warn!("YouTube fallback init failed: {}", e2);
                            unreachable!("valid localhost URI always succeeds")
                        }),
                    )
                }
            }
        }
        _ => {
            tracing::warn!(
                "YouTube API 자격증명이 설정되지 않았습니다. 업로드 기능이 비활성화됩니다."
            );
            Arc::new(
                youtube::YouTubeManager::new(
                    String::new(),
                    String::new(),
                    youtube_disabled_uri,
                    Arc::clone(&storage),
                )
                .unwrap_or_else(|e| {
                    tracing::warn!("YouTube fallback init failed: {}", e);
                    unreachable!("valid localhost URI always succeeds")
                }),
            )
        }
    };

    if let Err(e) = manager.load_credentials().await {
        tracing::warn!("YouTube 자격 증명 로드 실패: {}", e);
    }

    manager
}

#[cfg(test)]
mod redirect_uri_tests {
    use super::validate_redirect_uri;

    #[test]
    fn accepts_loopback_redirects() {
        assert!(validate_redirect_uri(
            "http://localhost:8080/oauth2/callback",
            "YouTube"
        ));
        assert!(validate_redirect_uri(
            "http://127.0.0.1:8080/oauth2/callback",
            "YouTube"
        ));
    }

    #[test]
    fn rejects_lookalike_or_non_http_redirects() {
        assert!(!validate_redirect_uri(
            "http://localhost.evil.test/oauth2/callback",
            "YouTube"
        ));
        assert!(!validate_redirect_uri(
            "https://localhost:8080/oauth2/callback",
            "YouTube"
        ));
        assert!(!validate_redirect_uri(
            "http://127.0.0.1.evil.test/oauth2/callback",
            "YouTube"
        ));
        assert!(!validate_redirect_uri(
            "http://localhost:abc/oauth2/callback",
            "YouTube"
        ));
    }
}
