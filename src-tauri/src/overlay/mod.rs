pub mod click_through;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
use tauri::{AppHandle, Manager};

const EXCLUSION_UNVERIFIED: u8 = 0;
const EXCLUSION_APPLIED: u8 = 1;
const EXCLUSION_FAILED: u8 = 2;

static CAPTURE_EXCLUSION_STATE: AtomicU8 = AtomicU8::new(EXCLUSION_UNVERIFIED);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureExclusionStatus {
    Unverified,
    Applied,
    Failed,
}

pub fn capture_exclusion_status() -> CaptureExclusionStatus {
    match CAPTURE_EXCLUSION_STATE.load(Ordering::Acquire) {
        EXCLUSION_APPLIED => CaptureExclusionStatus::Applied,
        EXCLUSION_FAILED => CaptureExclusionStatus::Failed,
        _ => CaptureExclusionStatus::Unverified,
    }
}

/// Apply Windows capture exclusion to the still-hidden overlay window.
///
/// This is safe to run during setup: it verifies the policy without making the
/// overlay visible. Callers can then expose the result through readiness while
/// `show_overlay` retains the fail-closed behavior.
pub fn verify_capture_exclusion(app: &AppHandle) -> CaptureExclusionStatus {
    let Some(window) = app.get_webview_window("overlay") else {
        CAPTURE_EXCLUSION_STATE.store(EXCLUSION_FAILED, Ordering::Release);
        return CaptureExclusionStatus::Failed;
    };

    #[cfg(target_os = "windows")]
    {
        let status = match window.hwnd() {
            Ok(hwnd) => match click_through::exclude_from_capture(hwnd.0 as isize) {
                Ok(()) => CaptureExclusionStatus::Applied,
                Err(error) => {
                    let _ = window.hide();
                    tracing::warn!(
                        %error,
                        "Overlay capture exclusion failed; keeping overlay hidden"
                    );
                    CaptureExclusionStatus::Failed
                }
            },
            Err(error) => {
                let _ = window.hide();
                tracing::warn!(
                    %error,
                    "Overlay HWND unavailable; keeping overlay hidden for capture safety"
                );
                CaptureExclusionStatus::Failed
            }
        };
        CAPTURE_EXCLUSION_STATE.store(
            match status {
                CaptureExclusionStatus::Applied => EXCLUSION_APPLIED,
                CaptureExclusionStatus::Failed => EXCLUSION_FAILED,
                CaptureExclusionStatus::Unverified => EXCLUSION_UNVERIFIED,
            },
            Ordering::Release,
        );
        status
    }

    #[cfg(not(target_os = "windows"))]
    {
        CAPTURE_EXCLUSION_STATE.store(EXCLUSION_FAILED, Ordering::Release);
        CaptureExclusionStatus::Failed
    }
}

pub fn show_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        if verify_capture_exclusion(app) != CaptureExclusionStatus::Applied {
            let _ = window.hide();
            return;
        }

        #[cfg(target_os = "windows")]
        if let Ok(hwnd) = window.hwnd() {
            click_through::make_click_through(hwnd.0 as isize);
        }
        let _ = window.show();
        tracing::info!("Overlay shown");
    }
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
        tracing::info!("Overlay hidden");
    }
}
