use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tracing::{error, info, warn};

/// Request a full application exit from the main UI.
///
/// Closing the webview can intentionally minimize it to the system tray. This
/// command mirrors the tray's explicit "quit" action so the `ExitRequested`
/// handler remains the sole owner of recording and temporary-file cleanup.
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    info!("메인 UI에서 앱 종료 요청");
    app.exit(0);
}

/// 시스템 트레이 아이콘 및 메뉴 설정
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_i = MenuItem::with_id(app, "show", "LoLShorts 열기", true, None::<&str>)?;
    let hide_i = MenuItem::with_id(app, "hide", "숨기기", true, None::<&str>)?;
    let separator = MenuItem::with_id(app, "sep", "──────────", false, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_i, &hide_i, &separator, &quit_i])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().unwrap_or_else(|| {
            tracing::warn!("No default window icon available for tray");
            tauri::image::Image::new(&[], 0, 0)
        }))
        .menu(&menu)
        .tooltip("LoLShorts - 하이라이트 메이커")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => {
                info!("트레이 메뉴에서 앱 종료 요청");
                // B7 fix: 정리 시퀀스(stop_recording_before_exit + cleanup_on_shutdown)는
                // main.rs의 `RunEvent::ExitRequested` 핸들러 한 곳에서만 실행한다.
                // `app.exit(0)`은 결국 그 핸들러의 ExitRequested를 발생시키므로 — 이
                // arm이 예전처럼 직접 정리를 수행하면 ExitRequested에서 다시 실행되어
                // 정지 시도가 2번 일어난다(수정 전 버그). 여기서는 종료만 요청한다.
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    info!("시스템 트레이 초기화 완료");
    Ok(())
}

/// 메인 창 닫기(X) 처리: `minimize_to_tray`가 켜져 있으면 트레이로 숨기고,
/// 꺼져 있으면 앱을 명시적으로 종료한다.
///
/// B1 fix: 이전에는 `minimize_to_tray == false`일 때 핸들러를 아예 달지 않고
/// Tauri의 기본 닫기 동작(웹뷰 파괴)에 맡겼다. 그런데 `label="overlay"`,
/// `visible=false`인 보조 창이 tauri.conf.json에 의해 항상 함께 떠 있어서,
/// wry의 windows 맵이 "완전히 비어야만" 발생하는 `RunEvent::ExitRequested`가
/// 결코 발생하지 않았다 — main 창만 파괴된 채 앱은 창 없이 계속 살아 FFmpeg가
/// 녹화를 이어갔고, main 창이 이미 파괴돼 트레이 "show"로도 복구할 수 없었다.
/// 이제 `minimize_to_tray == false`에서도 CloseRequested를 가로채 `exit(0)`을
/// 직접 호출해, windows 맵 상태(오버레이 창 존재 여부)와 무관하게 종료
/// 시퀀스(`ExitRequested` → `stop_recording_before_exit` + `cleanup_on_shutdown`,
/// main.rs 참고)로 진입시킨다. 오버레이 창은 보조 창이므로 이 판단에 관여하지
/// 않는다(자체 CloseRequested 핸들러가 없고, 사용자가 직접 닫을 수도 없다).
pub fn setup_close_to_tray(app: &AppHandle, minimize_to_tray: bool) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let window_clone = window.clone();
    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            // 두 분기 모두 기본 파괴 동작을 막는다: minimize_to_tray=true는 숨기기 위해,
            // false는 main만 파괴되고 창 없는 좀비 앱이 남는 것을 막고 대신
            // 명시적 exit(0)으로 정식 종료 시퀀스를 타게 하기 위해서다.
            api.prevent_close();
            if minimize_to_tray {
                if let Err(e) = window_clone.hide() {
                    error!("창 숨기기 실패: {}", e);
                }
                info!("창이 트레이로 최소화됨");
            } else {
                info!("메인 창 닫힘 요청 (minimize_to_tray=false) - 앱 종료");
                app_handle.exit(0);
            }
        }
    });
}

/// 앱 종료 전 진행 중인 녹화를 정지한다.
///
/// `SegmentRecorder::Drop`은 FFmpeg 자식 프로세스를 kill하지만, Tauri의
/// `Builder::run` 종료 경로(마지막 창을 닫아 `ExitRequested`가 발생하는 경우)와
/// 트레이의 `handle.exit(0)` 경로 모두 managed state의 `Drop`을 실행하지 않는다.
/// 따라서 이 함수를 명시적으로 호출해 정지하지 않으면, 창 없는 FFmpeg가 좀비로
/// 남아 20Mbps로 세그먼트를 계속 기록한다.
///
/// `stop_recording` 내부(`SegmentRecorder::stop`)에 이미 5초 타임아웃 + 강제
/// kill이 있지만, RwLock 자체가 다른 곳에서 막혀 있는 극단적인 경우까지 대비해
/// 바깥에도 상한을 둔다. 그 상한을 넘기면 (락을 통해 자식 프로세스 pid를 얻을
/// 수 없으므로) 녹화 디렉터리 경로가 커맨드라인에 포함된 ffmpeg 프로세스만 pid로
/// 특정해 강제 종료한다.
///
/// B3 fix: 이 함수를 호출하는 바깥 대기(main.rs의 `ExitRequested` 핸들러, 8초)가
/// 예전에는 `STOP_TIMEOUT(5s)` 이후에야 시작되는 `force_kill_capture_ffmpeg`의
/// probe(5s) + pid당 taskkill(5s)까지 감당할 수 없어, 8초가 지나 이벤트루프가
/// `process::exit`로 진행하면서 강제 종료 스레드가 taskkill 실행 전에 죽을 수
/// 있었다(예산 역전). 내부 상한 합(3+2+2*N)이 바깥 대기보다 항상 작도록 아래
/// 상수들과 main.rs의 바깥 대기(15s)를 함께 축소/확대했다.
const STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
// Only the Windows force-kill fallback reads these; without the cfg gate they are
// dead code on macOS/Linux and the cross-platform CI job runs
// `cargo clippy --all-targets -- -D warnings`.
#[cfg(target_os = "windows")]
const FFMPEG_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
#[cfg(target_os = "windows")]
const FFMPEG_KILL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// 녹화 중이 아니면 지연 없이 즉시 반환한다.
pub async fn stop_recording_before_exit(app: &AppHandle) {
    let state = app.state::<crate::AppState>();
    let recording_manager = std::sync::Arc::clone(&state.recording_manager);
    // 락이 막혀 stop_recording의 결과를 통해 recordings 디렉터리를 알아낼 수 없는
    // 최악의 경우에 대비해, 락과 무관하게 얻을 수 있는 경로를 폴백 매칭용으로 미리 확보한다.
    let recordings_marker = state.storage.base_path().join("recordings");

    let stop_fut = async {
        let mut manager = recording_manager.write().await;
        let status = manager.get_status().await;
        if matches!(
            status,
            crate::recording::RecordingStatus::Recording
                | crate::recording::RecordingStatus::Buffering
        ) {
            info!("앱 종료 전 진행 중인 녹화를 정지합니다");
            if let Err(e) = manager.stop_recording().await {
                error!("종료 전 녹화 정지 실패: {}", e);
            }
        }
    };

    if tokio::time::timeout(STOP_TIMEOUT, stop_fut).await.is_err() {
        warn!(
            "종료 전 녹화 정지가 {}초 내에 끝나지 않음 — FFmpeg 프로세스 강제 종료를 시도합니다",
            STOP_TIMEOUT.as_secs()
        );
        force_kill_capture_ffmpeg(recordings_marker).await;
    }
}

/// `recordings_dir`을 커맨드라인에 포함한 ffmpeg 프로세스만 pid로 특정해 강제
/// 종료한다. 이미지 이름 전체를 무차별로 죽이면 사용자의 다른 FFmpeg 작업
/// (내보내기 등)까지 함께 죽을 수 있어 피한다.
///
/// B2 fix: 이전에는 `Name='ffmpeg.exe'`로 고정돼 있었으나, 이 repo의 캡처
/// 사이드카 실제 이미지 이름은 `ffmpeg-x86_64-pc-windows-msvc.exe`(dev 빌드는
/// 그 이름 그대로 실행됨)라 CIM 쿼리가 항상 0건을 반환하고 폴백이 조용히
/// no-op이 됐다. `Name LIKE 'ffmpeg%'`로 두 이름 모두 매칭하고, 0건 조회와
/// kill 성공/실패를 로그로 구분한다.
#[cfg(target_os = "windows")]
async fn force_kill_capture_ffmpeg(recordings_dir: std::path::PathBuf) {
    // PowerShell 단일 따옴표 문자열 리터럴 이스케이프.
    let marker = recordings_dir.to_string_lossy().replace('\'', "''");

    let probe_result = tokio::task::spawn_blocking(move || {
        let ps_command = format!(
            "Get-CimInstance Win32_Process -Filter \"Name LIKE 'ffmpeg%'\" | \
             Where-Object {{ $_.CommandLine -and $_.CommandLine.Contains('{}') }} | \
             Select-Object -ExpandProperty ProcessId",
            marker
        );
        let mut probe = std::process::Command::new("powershell");
        probe.args(["-NoProfile", "-NonInteractive", "-Command", &ps_command]);
        crate::utils::process::command_output_with_timeout(
            probe,
            FFMPEG_PROBE_TIMEOUT,
            "shutdown ffmpeg pid probe",
        )
    })
    .await;

    let output = match probe_result {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            error!("종료 시 FFmpeg 프로세스 조회 실패: {}", e);
            return;
        }
        Err(e) => {
            error!("종료 시 FFmpeg 프로세스 조회 작업이 취소됨: {}", e);
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let pids: Vec<u32> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.parse::<u32>().ok())
        .collect();

    if pids.is_empty() {
        info!("종료 시 강제 종료 대상 FFmpeg 프로세스 없음 (CIM 조회 결과 0건)");
        return;
    }

    for pid in pids {
        warn!(pid, "종료 전 남은 캡처 FFmpeg 프로세스를 강제 종료합니다");
        let kill_result = tokio::task::spawn_blocking(move || {
            let mut kill_cmd = std::process::Command::new("taskkill");
            kill_cmd.args(["/F", "/PID", &pid.to_string()]);
            crate::utils::process::command_output_with_timeout(
                kill_cmd,
                FFMPEG_KILL_TIMEOUT,
                "shutdown ffmpeg taskkill",
            )
        })
        .await;

        match kill_result {
            Ok(Ok(output)) if output.status.success() => {
                info!(pid, "FFmpeg 프로세스 강제 종료 성공");
            }
            Ok(Ok(output)) => {
                warn!(
                    pid,
                    code = ?output.status.code(),
                    "taskkill이 비정상 종료 코드를 반환했습니다"
                );
            }
            Ok(Err(e)) => {
                error!(pid, "taskkill 실행 실패: {}", e);
            }
            Err(e) => {
                error!(pid, "taskkill 작업이 취소됨: {}", e);
            }
        }
    }
}

/// 비-Windows 폴백: `SegmentRecorder::stop()`이 이미 SIGINT + 5초 대기 + kill을
/// 수행하므로 여기서 추가로 할 일이 없다(이 유닛은 Windows 좀비 프로세스 시나리오
/// 대응이 목적).
#[cfg(not(target_os = "windows"))]
async fn force_kill_capture_ffmpeg(_recordings_dir: std::path::PathBuf) {}
