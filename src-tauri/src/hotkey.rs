use anyhow::Result;
/// Global hotkey system for LoLShorts
///
/// Registers system-wide hotkeys for recording control:
/// - F8: Save last 60 seconds (manual instant replay)
/// - F9: Toggle auto-capture (start/stop)
/// - F10: Delete the most recently saved clip
///
/// Uses Windows RegisterHotKey API for global hotkey registration
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};

// UTF-16 string macro for Windows API - MUST be defined before use
#[cfg(target_os = "windows")]
macro_rules! w {
    ($s:expr) => {{
        const STR: &str = concat!($s, "\0");
        const UTF16: &[u16] = &{
            let bytes = STR.as_bytes();
            let mut utf16 = [0u16; STR.len()];
            let mut i = 0;
            while i < bytes.len() {
                utf16[i] = bytes[i] as u16;
                i += 1;
            }
            utf16
        };
        windows::core::PCWSTR::from_raw(UTF16.as_ptr())
    }};
}

#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
        MOD_SHIFT, VIRTUAL_KEY, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6,
        VK_F7, VK_F8, VK_F9,
    },
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW,
        PostQuitMessage, RegisterClassW, TranslateMessage, CS_HREDRAW, CS_VREDRAW, MSG,
        WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_HOTKEY, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    },
};

/// Hotkey identifiers
const HOTKEY_F8: i32 = 1; // Save 60s (manual clip)
const HOTKEY_F9: i32 = 2; // Toggle auto-capture
const HOTKEY_F10: i32 = 3; // Delete last clip

/// Hotkey event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    ToggleAutoCapture, // F9
    SaveReplay60,      // F8
    DeleteLastClip,    // F10
}

/// Configuration for dynamic hotkey registration
#[derive(Debug, Clone)]
pub struct HotkeyConfig {
    pub manual_save_clip: String,
    pub toggle_recording: String,
    pub delete_last_clip: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            manual_save_clip: "F8".to_string(),
            toggle_recording: "F9".to_string(),
            delete_last_clip: "F10".to_string(),
        }
    }
}

/// Parse a hotkey string like "F8", "Ctrl+F9", "Alt+Shift+F10" into modifiers and virtual key
#[cfg(target_os = "windows")]
fn parse_hotkey(key_str: &str) -> Option<(HOT_KEY_MODIFIERS, VIRTUAL_KEY)> {
    let parts: Vec<&str> = key_str.split('+').map(|s| s.trim()).collect();
    let mut modifiers = HOT_KEY_MODIFIERS(0);
    let mut vk = None;

    for part in &parts {
        match part.to_uppercase().as_str() {
            "CTRL" | "CONTROL" => modifiers |= MOD_CONTROL,
            "ALT" => modifiers |= MOD_ALT,
            "SHIFT" => modifiers |= MOD_SHIFT,
            key => {
                vk = match key {
                    "F1" => Some(VK_F1),
                    "F2" => Some(VK_F2),
                    "F3" => Some(VK_F3),
                    "F4" => Some(VK_F4),
                    "F5" => Some(VK_F5),
                    "F6" => Some(VK_F6),
                    "F7" => Some(VK_F7),
                    "F8" => Some(VK_F8),
                    "F9" => Some(VK_F9),
                    "F10" => Some(VK_F10),
                    "F11" => Some(VK_F11),
                    "F12" => Some(VK_F12),
                    s if s.len() == 1 => Some(VIRTUAL_KEY(s.as_bytes()[0] as u16)),
                    _ => None,
                };
            }
        }
    }

    modifiers |= MOD_NOREPEAT;
    vk.map(|k| (modifiers, k))
}

/// Hotkey manager
pub struct HotkeyManager {
    enabled: Arc<RwLock<bool>>,
    hwnd: Arc<RwLock<Option<isize>>>,
}

impl Default for HotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(RwLock::new(false)),
            hwnd: Arc::new(RwLock::new(None)),
        }
    }

    /// Start hotkey listener with default hotkey configuration
    #[cfg(target_os = "windows")]
    pub async fn start<F>(&self, callback: F) -> Result<()>
    where
        F: Fn(HotkeyEvent) + Send + Sync + 'static,
    {
        self.start_with_config(callback, HotkeyConfig::default())
            .await
    }

    /// Start hotkey listener with custom hotkey configuration (Windows implementation)
    #[cfg(target_os = "windows")]
    pub async fn start_with_config<F>(&self, callback: F, config: HotkeyConfig) -> Result<()>
    where
        F: Fn(HotkeyEvent) + Send + Sync + 'static,
    {
        let enabled = Arc::clone(&self.enabled);
        let hwnd_store = Arc::clone(&self.hwnd);
        let (setup_tx, setup_rx) = oneshot::channel::<std::result::Result<(), String>>();

        // Spawn hotkey listener thread and report initialization back to the caller.
        tokio::task::spawn_blocking(move || {
            let mut setup_tx = Some(setup_tx);
            let mut report_setup = |result: std::result::Result<(), String>| {
                if let Some(tx) = setup_tx.take() {
                    let _ = tx.send(result);
                }
            };

            unsafe {
                // Create invisible window for message processing
                let h_instance = match windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                {
                    Ok(h_instance) => h_instance,
                    Err(e) => {
                        let message = format!("Failed to get module handle: {:?}", e);
                        tracing::error!("{}", message);
                        report_setup(Err(message));
                        return;
                    }
                };

                let class_name = w!("LoLShortsHotkeyWindow");

                let wc = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(window_proc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: h_instance.into(),
                    hIcon: Default::default(),
                    hCursor: Default::default(),
                    hbrBackground: Default::default(),
                    lpszMenuName: windows::core::PCWSTR::null(),
                    lpszClassName: class_name,
                };

                let class_atom = RegisterClassW(&wc);
                if class_atom == 0 {
                    tracing::warn!(
                        "Hotkey window class registration returned 0; continuing because the class may already exist"
                    );
                }

                let hwnd = match CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    class_name,
                    w!("LoLShorts Hotkeys"),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    0,
                    0,
                    HWND::default(),
                    None,
                    h_instance,
                    None,
                ) {
                    Ok(hwnd) => hwnd,
                    Err(e) => {
                        let message = format!("Failed to create hotkey window: {:?}", e);
                        tracing::error!("{}", message);
                        report_setup(Err(message));
                        return;
                    }
                };

                // Store HWND for clean shutdown
                *hwnd_store.blocking_write() = Some(hwnd.0 as isize);
                let mut registered_count = 0usize;
                let mut registration_failures = Vec::new();

                let mut register_hotkey =
                    |id: i32, label: &str, modifiers: HOT_KEY_MODIFIERS, vk: VIRTUAL_KEY| {
                        match RegisterHotKey(hwnd, id, modifiers, vk.0 as u32) {
                            Ok(()) => {
                                registered_count += 1;
                                tracing::info!("Registered hotkey {} successfully", label);
                            }
                            Err(e) => {
                                registration_failures.push(format!("{} ({:?})", label, e));
                            }
                        }
                    };

                // Register hotkeys dynamically from settings
                // manual_save_clip -> SaveReplay60 (HOTKEY_F8)
                if let Some((modifiers, vk)) = parse_hotkey(&config.manual_save_clip) {
                    register_hotkey(HOTKEY_F8, "manual_save_clip", modifiers, vk);
                } else {
                    tracing::warn!(
                        "Invalid hotkey string for manual_save_clip: {}, falling back to F8",
                        config.manual_save_clip
                    );
                    register_hotkey(HOTKEY_F8, "manual_save_clip fallback", MOD_NOREPEAT, VK_F8);
                }

                // toggle_recording -> ToggleAutoCapture (HOTKEY_F9)
                if let Some((modifiers, vk)) = parse_hotkey(&config.toggle_recording) {
                    register_hotkey(HOTKEY_F9, "toggle_recording", modifiers, vk);
                } else {
                    tracing::warn!(
                        "Invalid hotkey string for toggle_recording: {}, falling back to F9",
                        config.toggle_recording
                    );
                    register_hotkey(HOTKEY_F9, "toggle_recording fallback", MOD_NOREPEAT, VK_F9);
                }

                // delete_last_clip -> DeleteLastClip (HOTKEY_F10)
                if let Some((modifiers, vk)) = parse_hotkey(&config.delete_last_clip) {
                    register_hotkey(HOTKEY_F10, "delete_last_clip", modifiers, vk);
                } else {
                    tracing::warn!(
                        "Invalid hotkey string for delete_last_clip: {}, falling back to F10",
                        config.delete_last_clip
                    );
                    register_hotkey(
                        HOTKEY_F10,
                        "delete_last_clip fallback",
                        MOD_NOREPEAT,
                        VK_F10,
                    );
                }

                if registered_count == 0 {
                    let message = format!(
                        "No global hotkeys could be registered: {}",
                        registration_failures.join("; ")
                    );
                    tracing::error!("{}", message);
                    *hwnd_store.blocking_write() = None;
                    report_setup(Err(message));
                    return;
                }

                if !registration_failures.is_empty() {
                    tracing::warn!(
                        "Some global hotkeys failed to register: {}",
                        registration_failures.join("; ")
                    );
                }

                tracing::info!(
                    "Global hotkeys registered: {} (save 60s), {} (toggle), {} (save 30s)",
                    config.manual_save_clip,
                    config.toggle_recording,
                    config.delete_last_clip
                );
                report_setup(Ok(()));

                // Message loop
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
                    if msg.message == WM_HOTKEY {
                        let hotkey_id = msg.wParam.0 as i32;
                        let event = match hotkey_id {
                            HOTKEY_F8 => Some(HotkeyEvent::SaveReplay60),
                            HOTKEY_F9 => Some(HotkeyEvent::ToggleAutoCapture),
                            HOTKEY_F10 => Some(HotkeyEvent::DeleteLastClip),
                            _ => None,
                        };

                        if let Some(event) = event {
                            tracing::debug!("Hotkey triggered: {:?}", event);
                            callback(event);
                        }
                    }

                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                // Cleanup
                UnregisterHotKey(hwnd, HOTKEY_F8).ok();
                UnregisterHotKey(hwnd, HOTKEY_F9).ok();
                UnregisterHotKey(hwnd, HOTKEY_F10).ok();
                *hwnd_store.blocking_write() = None;
            }
        });

        match tokio::time::timeout(std::time::Duration::from_secs(5), setup_rx).await {
            Ok(Ok(Ok(()))) => {
                *enabled.write().await = true;
                Ok(())
            }
            Ok(Ok(Err(message))) => {
                *enabled.write().await = false;
                Err(anyhow::anyhow!(message))
            }
            Ok(Err(_)) => {
                *enabled.write().await = false;
                Err(anyhow::anyhow!(
                    "Hotkey listener exited before reporting initialization"
                ))
            }
            Err(_) => {
                *enabled.write().await = false;
                Err(anyhow::anyhow!("Hotkey initialization timed out"))
            }
        }
    }

    /// Stop hotkey listener
    pub async fn stop(&self) -> Result<()> {
        let mut enabled = self.enabled.write().await;
        *enabled = false;

        #[cfg(target_os = "windows")]
        {
            let hwnd_val = *self.hwnd.read().await;
            if let Some(raw) = hwnd_val {
                unsafe {
                    let _ = PostMessageW(
                        HWND(raw as *mut core::ffi::c_void),
                        WM_CLOSE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
        }

        tracing::info!("Hotkey manager stopped");

        Ok(())
    }

    /// Check if hotkeys are enabled
    pub async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }
}

/// Window procedure for hotkey message handling
#[cfg(target_os = "windows")]
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// For non-Windows platforms
#[cfg(not(target_os = "windows"))]
impl HotkeyManager {
    pub async fn start<F>(&self, _callback: F) -> Result<()>
    where
        F: Fn(HotkeyEvent) + Send + Sync + 'static,
    {
        tracing::warn!("Global hotkeys not supported on this platform");
        Ok(())
    }

    pub async fn start_with_config<F>(&self, _callback: F, _config: HotkeyConfig) -> Result<()>
    where
        F: Fn(HotkeyEvent) + Send + Sync + 'static,
    {
        tracing::warn!("Global hotkeys not supported on this platform");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hotkey_manager_creation() {
        let manager = HotkeyManager::new();
        assert!(!manager.is_enabled().await);
    }

    #[tokio::test]
    async fn test_hotkey_event_equality() {
        assert_eq!(
            HotkeyEvent::ToggleAutoCapture,
            HotkeyEvent::ToggleAutoCapture
        );
        assert_ne!(HotkeyEvent::ToggleAutoCapture, HotkeyEvent::SaveReplay60);
    }

    #[test]
    fn test_hotkey_config_default() {
        let config = HotkeyConfig::default();
        assert_eq!(config.manual_save_clip, "F8");
        assert_eq!(config.toggle_recording, "F9");
        assert_eq!(config.delete_last_clip, "F10");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_hotkey_simple_f8() {
        let result = parse_hotkey("F8");
        assert!(result.is_some());
        let (modifiers, vk) = result.unwrap();
        assert_eq!(vk, VK_F8);
        assert!(modifiers.0 & MOD_NOREPEAT.0 != 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_hotkey_with_modifiers() {
        let result = parse_hotkey("Ctrl+Shift+F10");
        assert!(result.is_some());
        let (modifiers, vk) = result.unwrap();
        assert_eq!(vk, VK_F10);
        assert!(modifiers.0 & MOD_CONTROL.0 != 0);
        assert!(modifiers.0 & MOD_SHIFT.0 != 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_hotkey_invalid_returns_none() {
        let result = parse_hotkey("InvalidKey");
        assert!(result.is_none());
    }
}
