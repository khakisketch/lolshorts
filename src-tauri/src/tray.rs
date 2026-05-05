use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tracing::{error, info};

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
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<crate::AppState>();
                    if let Err(e) = state.cleanup_manager.cleanup_on_shutdown().await {
                        tracing::error!("Shutdown cleanup failed: {}", e);
                    }
                    handle.exit(0);
                });
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

/// 창 닫기 시 트레이로 최소화 처리
pub fn setup_close_to_tray(app: &AppHandle, minimize_to_tray: bool) {
    if !minimize_to_tray {
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        let window_clone = window.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 창 닫기 대신 숨기기
                api.prevent_close();
                if let Err(e) = window_clone.hide() {
                    error!("창 숨기기 실패: {}", e);
                }
                info!("창이 트레이로 최소화됨");
            }
        });
    }
}
