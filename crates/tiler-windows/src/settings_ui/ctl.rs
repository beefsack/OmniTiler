//! Win32 control helpers for the settings window.
//!
//! Small wrappers around `windows-sys` text/check/enable calls, the `Ctl`
//! layout descriptor, DPI scaling, control creation, input caps, abort
//! teardown, and the message font. No settings policy lives here.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::HFONT;

use super::App;
use super::{
    DynError, ID_BIND_CHORD, ID_BORDER_COLOR, ID_BORDER_GAP, ID_BORDER_RADIUS, ID_BORDER_WIDTH,
    ID_INNER_GAP, ID_OUTER_GAP, ID_UNDERLAY_COLOR, ID_UNDERLAY_EXTENSION, err,
};

pub(super) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub(super) fn set_text(hwnd: HWND, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;
    let text_w = wide(text);
    unsafe {
        SetWindowTextW(hwnd, text_w.as_ptr());
    }
}

pub(super) fn get_text(hwnd: HWND) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let capped = len.min(512) as usize;
    let mut buf = vec![0u16; capped + 1];
    let read = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), capped as i32 + 1) };
    if read <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..read as usize])
}

pub(super) fn checked(hwnd: HWND) -> bool {
    use windows_sys::Win32::UI::Controls::BST_CHECKED;
    use windows_sys::Win32::UI::WindowsAndMessaging::{BM_GETCHECK, SendMessageW};
    unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == isize::try_from(BST_CHECKED).unwrap_or(1) }
}

pub(super) fn set_checked(hwnd: HWND, value: bool) {
    use windows_sys::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED};
    use windows_sys::Win32::UI::WindowsAndMessaging::{BM_SETCHECK, SendMessageW};
    let flag = if value { BST_CHECKED } else { BST_UNCHECKED };
    unsafe {
        SendMessageW(hwnd, BM_SETCHECK, flag as usize, 0);
    }
}

pub(super) fn enable(hwnd: HWND, value: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    unsafe {
        EnableWindow(hwnd, if value { 1 } else { 0 });
    }
}

pub(super) struct Ctl {
    pub(super) id: u32,
    pub(super) class: &'static str,
    pub(super) text: String,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) w: i32,
    pub(super) h: i32,
    pub(super) style: u32,
}

pub(super) fn px(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi) + 48) / 96) as i32
}

/// Create one child control with the DPI-scaled message font.
/// `WS_CLIPSIBLINGS` plus the parent's `WS_CLIPCHILDREN` keep the groupbox
/// frames from smearing over their children; the groupboxes are additionally
/// pinned to the bottom of the sibling order after creation (see below),
/// because creation order alone left the first-created groupboxes topmost and
/// each opaque groupbox then covered every control inside it. Returns the new
/// control handle.
#[allow(clippy::too_many_arguments)]
pub(super) fn create_ctl(
    parent: HWND,
    hinst: windows_sys::Win32::Foundation::HINSTANCE,
    ctl: &Ctl,
    dpi: u32,
    font: HFONT,
    out: &mut Vec<(u32, HWND)>,
) -> Result<HWND, DynError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, WM_SETFONT, WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
    };
    let class_w = wide(ctl.class);
    let text_w = wide(&ctl.text);
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_w.as_ptr(),
            text_w.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | ctl.style,
            px(ctl.x, dpi),
            px(ctl.y, dpi),
            px(ctl.w, dpi),
            px(ctl.h, dpi),
            parent,
            ctl.id as isize as windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
            hinst,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err(err(format!("error: settings control {} failed", ctl.id)));
    }
    if !font.is_null() {
        use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, font as usize, 1);
        }
    }
    out.push((ctl.id, hwnd));
    Ok(hwnd)
}

/// Bounded native input length per edit control (the `get_text` 512 cap stays
/// as backstop). Group/label ids carry no limit.
pub(super) fn edit_limit(id: u32) -> Option<usize> {
    match id {
        ID_INNER_GAP | ID_OUTER_GAP => Some(2),
        ID_BORDER_COLOR => Some(7),
        ID_BORDER_WIDTH | ID_BORDER_GAP | ID_BORDER_RADIUS => Some(5),
        ID_UNDERLAY_COLOR => Some(9),
        ID_UNDERLAY_EXTENSION => Some(5),
        ID_BIND_CHORD => Some(24),
        _ => None,
    }
}

/// Tear down a half-built window after a control-creation failure. `WM_DESTROY`
/// owns the box through `GWLP_USERDATA`, so detach first: the synchronous
/// `DestroyWindow` below then reclaims nothing, and this drop is the single
/// owner. Exactly one free either way.
pub(super) fn abort_create(hwnd: HWND, raw: *mut App) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW,
    };
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        DestroyWindow(hwnd);
        drop(Box::from_raw(raw));
    }
}

pub(super) fn message_font(dpi: u32) -> HFONT {
    use windows_sys::Win32::Graphics::Gdi::{CreateFontW, DEFAULT_GUI_FONT, GetStockObject};
    let height = -((9 * dpi as i32 + 36) / 72);
    let face = wide("Segoe UI");
    // Weight/style/charset defaults; the stock fallback covers failure.
    let font = unsafe { CreateFontW(height, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr()) };
    if font.is_null() {
        unsafe { GetStockObject(DEFAULT_GUI_FONT) }
    } else {
        font
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dialog_fits_1380_work_area_at_dpi120() {
        // Lowest control edge is the status line (y1028 h60); it must sit
        // inside the 1088-high client area, which itself must fit a
        // 1380-high work area at DPI 120 (px(1088, 120) == 1360, leaving room
        // for the non-client frame on top).
        assert_eq!(super::px(1088, 120), 1360);
        assert!(super::px(1028 + 60, 120) <= super::px(1088, 120));
        assert!(super::px(1088, 120) <= 1380);
    }
}
