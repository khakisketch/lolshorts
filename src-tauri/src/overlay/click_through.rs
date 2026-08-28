#[cfg(target_os = "windows")]
pub fn make_click_through(raw_hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let hwnd = HWND(raw_hwnd as *mut _);
    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            ex_style | WS_EX_LAYERED.0 as i32 | WS_EX_TRANSPARENT.0 as i32,
        );
    }
}

/// Ask Windows to omit this top-level window from screen capture.
///
/// Kept separate from click-through because either Win32 policy can fail
/// independently. The caller hides the overlay when this one fails so REC/toast UI
/// can never leak into desktop-fallback footage.
#[cfg(target_os = "windows")]
pub fn exclude_from_capture(raw_hwnd: isize) -> Result<(), String> {
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x0000_0011;

    #[link(name = "user32")]
    extern "system" {
        fn SetWindowDisplayAffinity(hwnd: isize, affinity: u32) -> i32;
    }

    let applied = unsafe { SetWindowDisplayAffinity(raw_hwnd, WDA_EXCLUDEFROMCAPTURE) };
    if applied == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn make_click_through(_raw_hwnd: isize) {
    // Click-through is only supported on Windows
    tracing::debug!("Click-through overlay not supported on this platform");
}

#[cfg(not(target_os = "windows"))]
pub fn exclude_from_capture(_raw_hwnd: isize) -> Result<(), String> {
    Ok(())
}
