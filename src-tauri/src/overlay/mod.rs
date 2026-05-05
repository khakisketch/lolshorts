pub mod click_through;

use tauri::{AppHandle, Manager};

pub fn show_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.show();
        #[cfg(target_os = "windows")]
        if let Ok(hwnd) = window.hwnd() {
            click_through::make_click_through(hwnd.0 as isize);
        }
        tracing::info!("Overlay shown");
    }
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
        tracing::info!("Overlay hidden");
    }
}
