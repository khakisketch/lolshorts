// 릴리스 빌드에서 Windows의 추가 콘솔 창 방지, 제거하지 마세요!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
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

    // 기능 게이트(Feature Gate) 초기화
    let feature_gate = Arc::new(feature_gate::FeatureGate::new(auth.clone()));

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

    let auto_composer = Arc::new(video::AutoComposer::new(
        Arc::clone(&video_processor),
        Arc::clone(&storage),
    ));

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

    let app_state = AppState {
        storage,
        auth,
        feature_gate,
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
    let hotkey_settings = app_state.recording_settings.read().await.hotkeys.clone();
    let hotkey_config = hotkey::HotkeyConfig {
        manual_save_clip: hotkey_settings.manual_save_clip,
        toggle_recording: hotkey_settings.toggle_recording,
        delete_last_clip: hotkey_settings.delete_last_clip,
    };

    // TODO: Replace with spawn_monitored once the inner closure types support UnwindSafe
    tokio::spawn(async move {
        let hotkey_result = hotkey_manager
            .start_with_config(
                move |event| {
                    let rm = Arc::clone(&recording_manager_hotkey);
                    let acm = Arc::clone(&auto_clip_manager_hotkey);

                    tokio::spawn(async move {
                        use hotkey::HotkeyEvent;

                        match event {
                            HotkeyEvent::ToggleAutoCapture => {
                                // 자동 캡처가 실행 중인지 확인
                                let is_monitoring = acm.is_monitoring().await;

                                if is_monitoring {
                                    // 자동 캡처 중지
                                    tracing::info!("핫키 F8: 자동 캡처 중지");
                                    if let Err(e) = acm.stop_event_monitoring().await {
                                        tracing::error!("자동 캡처 중지 실패: {}", e);
                                    }
                                    if let Err(e) = rm.write().await.stop_recording().await {
                                        tracing::error!("리플레이 버퍼 중지 실패: {}", e);
                                    }
                                } else {
                                    // 자동 캡처 시작
                                    tracing::info!("핫키 F8: 자동 캡처 시작");
                                    if let Err(e) = rm.write().await.start_recording().await {
                                        tracing::error!("리플레이 버퍼 시작 실패: {}", e);
                                    }
                                    if let Err(e) = acm.start_event_monitoring().await {
                                        tracing::error!("이벤트 모니터링 시작 실패: {}", e);
                                    }
                                }
                            }
                            HotkeyEvent::SaveReplay60 => {
                                // 최근 60초 저장 - 녹화 중단 없이 클립 추출
                                tracing::info!("핫키 F9: 60초 리플레이 저장");
                                match rm.read().await.save_last_seconds(60).await {
                                    Ok(path) => {
                                        tracing::info!("60초 리플레이 저장됨: {:?}", path);
                                    }
                                    Err(e) => tracing::error!("60초 리플레이 저장 실패: {}", e),
                                }
                            }
                            HotkeyEvent::SaveReplay30 => {
                                // 최근 30초 저장 - 녹화 중단 없이 클립 추출
                                tracing::info!("핫키 F10: 30초 리플레이 저장");
                                match rm.read().await.save_last_seconds(30).await {
                                    Ok(path) => {
                                        tracing::info!("30초 리플레이 저장됨: {:?}", path);
                                    }
                                    Err(e) => tracing::error!("30초 리플레이 저장 실패: {}", e),
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

        // 게임 시작 콜백 정의
        let game_start_callback = move || {
            let recording_mgr = Arc::clone(&recording_manager_start);
            let clip_mgr = Arc::clone(&clip_manager_start);
            let overlay_handle = Arc::clone(&overlay_handle_for_start);
            let overlay_cfg = Arc::clone(&overlay_settings);
            let disk_monitor = Arc::clone(&recording_disk_monitor_for_start);

            async move {
                tracing::info!("🎮 게임 감지됨! 자동 녹화 시작...");

                // 녹화 시작
                if let Err(e) = recording_mgr.write().await.start_recording().await {
                    tracing::error!("리플레이 버퍼 시작 실패: {}", e);
                    return Err(e.to_string());
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

                // 하이라이트 이벤트 모니터링 시작
                if let Err(e) = clip_mgr.start_event_monitoring().await {
                    tracing::error!("이벤트 모니터링 시작 실패: {}", e);
                    recording::commands::stop_recording_disk_monitor_with_sender(disk_monitor)
                        .await;
                    return Err(e.to_string());
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
            let overlay_handle = Arc::clone(&overlay_handle_for_end);
            let disk_monitor = Arc::clone(&recording_disk_monitor_for_end);

            async move {
                tracing::info!("⏹️ 게임 종료. 자동 녹화 중지...");

                recording::commands::stop_recording_disk_monitor_with_sender(disk_monitor).await;

                // Hide overlay
                if let Some(handle) = overlay_handle.lock().await.as_ref() {
                    overlay::hide_overlay(handle);
                }

                // 녹화 중지
                if let Err(e) = recording_mgr.write().await.stop_recording().await {
                    tracing::error!("리플레이 버퍼 중지 실패: {}", e);
                    return Err(e.to_string());
                }

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

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(app_state)
        .setup(|app| {
            // Store app handle for overlay (used by game monitor callbacks)
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
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
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            tracing::error!("Failed to run Tauri application: {}", e);
            eprintln!("Fatal Error: LoLShorts failed to start: {}", e);
            std::process::exit(1);
        });
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
    if !uri.starts_with("http://localhost") && !uri.starts_with("http://127.0.0.1") {
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
        .unwrap_or_else(|_| "http://localhost:8080/oauth2/callback".to_string());

    let youtube_redirect_uri_valid = validate_redirect_uri(&youtube_redirect_uri, "YouTube");
    let youtube_disabled_uri = "http://localhost:8080/oauth2/callback".to_string();

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
