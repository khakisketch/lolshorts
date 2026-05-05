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

#[cfg(not(target_os = "windows"))]
pub fn make_click_through(_raw_hwnd: isize) {
    // Click-through is only supported on Windows
    tracing::debug!("Click-through overlay not supported on this platform");
}
