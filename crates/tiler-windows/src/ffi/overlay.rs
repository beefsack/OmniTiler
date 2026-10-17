//! Owned layered-overlay carrier (`UpdateLayeredWindow` path).
//!
//! Every Win32 call, raw pointer, DIB lifetime, and the single `extern`
//! window procedure for the active-border / group-underlay / drop-preview
//! surfaces lives here. Safe callers pass numeric window ids (`u64`), plain
//! `Rect` values, a static label for error text, and a painter closure that
//! only sees an owned `&mut [u8]` slice: the DIB section, its backing store,
//! and its presentation lifetime never leave this module.
//!
//! Threading: overlay windows are confined to the creating thread (the
//! procedure runs there). These functions perform no locking and must be
//! called on that thread. The accent-dirty flag is a process-wide sticky
//! `AtomicBool`: broadcasts coalesce, the owner consumes exactly once.
//! The procedure performs no allocation and calls no caller code, so it
//! cannot unwind.

use std::sync::atomic::{AtomicBool, Ordering};

use tiler_core::geometry::Rect;
use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmGetColorizationColor;
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC, SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_NOTOPMOST,
    HWND_TOP, HWND_TOPMOST, IsWindow, IsWindowVisible, RegisterClassW, SW_HIDE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
};

use super::hwnd_of;

const WM_SETTINGCHANGE: u32 = 0x001A;
const WM_DWMCOLORIZATIONCOLORCHANGED: u32 = 0x0320;
const CLASS_BUSY: u32 = 1410;
const SURFACE_LIMIT_PX: i32 = 16384;

/// Owned overlay kind. Only these three classes are ever registered (with
/// the internal procedure below) or instantiated: a caller cannot pair an
/// unrelated existing window procedure with this carrier by naming its
/// class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayClass {
    Border,
    Underlay,
    Preview,
}

fn class_name(class: OverlayClass) -> &'static str {
    match class {
        OverlayClass::Border => "OmniTilerActiveBorder",
        OverlayClass::Underlay => "OmniTilerGroupUnderlay",
        OverlayClass::Preview => "OmniTilerDropPreview",
    }
}

static ACCENT_DIRTY: AtomicBool = AtomicBool::new(false);

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Consume one pending system-accent refresh (sticky: coalesced broadcasts
/// refresh exactly once).
pub fn take_accent_dirty() -> bool {
    ACCENT_DIRTY.swap(false, Ordering::SeqCst)
}

fn mark_accent_dirty() {
    ACCENT_DIRTY.store(true, Ordering::SeqCst);
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_DWMCOLORIZATIONCOLORCHANGED || msg == WM_SETTINGCHANGE {
        mark_accent_dirty();
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Live DWM colorization colour (`0xAARRGGBB`); `None` when the query fails
/// (the caller falls back to its configured colour).
pub fn dwm_colorization() -> Option<u32> {
    let mut dword: u32 = 0;
    let mut opaque: i32 = 0;
    let hr = unsafe { DwmGetColorizationColor(&mut dword, &mut opaque) };
    if hr == 0 { Some(dword) } else { None }
}

/// Register the class for `class` (tolerating a previous registration) and
/// create one hidden layered popup; the numeric id on success. Error text
/// preserves the previous carrier vocabulary.
pub fn create_overlay_window(class: OverlayClass) -> Result<u64, String> {
    let class_w = wide(class_name(class));
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
    cls.style = CS_HREDRAW | CS_VREDRAW;
    cls.lpfnWndProc = Some(overlay_wnd_proc);
    cls.hInstance = hinst;
    cls.lpszClassName = class_w.as_ptr();
    let atom = unsafe { RegisterClassW(&cls) };
    if atom == 0 {
        let code = unsafe { GetLastError() };
        if code != CLASS_BUSY {
            return Err("error: overlay RegisterClass failed".to_owned());
        }
    }
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_w.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err("error: active-border CreateWindow failed".to_owned());
    }
    Ok(hwnd as usize as u64)
}

/// Creator-thread liveness read for one owned carrier window.
pub fn overlay_alive(hwnd: u64) -> bool {
    if hwnd == 0 {
        return false;
    }
    unsafe { IsWindow(hwnd_of(hwnd)) != 0 }
}

/// Hide one owned surface; true only when it reads hidden afterwards (a
/// destroyed window reads hidden, so this doubles as the verified-hide
/// readback).
pub fn hide_overlay(hwnd: u64) -> bool {
    unsafe {
        ShowWindow(hwnd_of(hwnd), SW_HIDE);
    }
    if !overlay_alive(hwnd) {
        return true;
    }
    unsafe { IsWindowVisible(hwnd_of(hwnd)) == 0 }
}

/// Destroy one owned surface; true only when the id reads destroyed
/// afterwards (a surviving window keeps its live handle for a bounded
/// caller retry).
pub fn destroy_overlay(hwnd: u64) -> bool {
    unsafe {
        DestroyWindow(hwnd_of(hwnd));
    }
    !overlay_alive(hwnd)
}

fn place_band(hwnd: u64, outer: Rect, after: HWND) -> bool {
    unsafe {
        SetWindowPos(
            hwnd_of(hwnd),
            after,
            outer.x,
            outer.y,
            outer.w,
            outer.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        ) != 0
    }
}

/// Position one owned overlay directly beneath its fresh anchor without
/// leaving its band: a normal-band surface for ordinary anchors, a topmost
/// surface for topmost anchors, then adjacency below the anchor itself.
/// A zero anchor (or the surface itself) falls back to the band anchor.
pub fn place_below(hwnd: u64, outer: Rect, topmost: bool, anchor: u64) -> Result<(), String> {
    if anchor != 0 && anchor != hwnd {
        if topmost {
            if !place_band(hwnd, outer, HWND_TOPMOST) {
                return Err("error: active-border position failed".to_owned());
            }
        } else if !place_band(hwnd, outer, HWND_NOTOPMOST) {
            return Err("error: active-border position failed".to_owned());
        }
        let below_target = unsafe {
            SetWindowPos(
                hwnd_of(hwnd),
                hwnd_of(anchor),
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        };
        if below_target == 0 {
            return Err("error: active-border position failed".to_owned());
        }
        return Ok(());
    }
    let band = if topmost { HWND_TOPMOST } else { HWND_TOP };
    if !place_band(hwnd, outer, band) {
        return Err("error: active-border position failed".to_owned());
    }
    Ok(())
}

/// Position one preview surface above windows in the topmost band (never
/// below an anchor: the filled target slot must stay above app windows).
pub fn place_above(hwnd: u64, outer: Rect) -> Result<(), String> {
    if !place_band(hwnd, outer, HWND_TOPMOST) {
        return Err("error: drop-preview position failed".to_owned());
    }
    Ok(())
}

fn surface_len(outer: Rect, label: &str) -> Result<usize, String> {
    if outer.w <= 0 || outer.h <= 0 {
        return Err(format!("error: {label} bad paint geometry"));
    }
    if outer.w > SURFACE_LIMIT_PX || outer.h > SURFACE_LIMIT_PX {
        return Err(format!("error: {label} surface too large"));
    }
    Ok(outer.w as usize * 4 * outer.h as usize)
}

/// RAII owner for the paint device context: deleted on drop (a `paint`
/// panic or the partial-DIB early return), disarmed on the success path so
/// the success call sequence stays exactly as before.
struct MemDc(Option<HDC>);

impl MemDc {
    fn get(&self) -> HDC {
        self.0.unwrap_or(std::ptr::null_mut())
    }

    fn disarm(&mut self) -> HDC {
        self.0.take().unwrap_or(std::ptr::null_mut())
    }
}

impl Drop for MemDc {
    fn drop(&mut self) {
        if let Some(dc) = self.0.take()
            && !dc.is_null()
        {
            unsafe {
                DeleteDC(dc);
            }
        }
    }
}

/// RAII owner for the DIB section bitmap: same contract as [`MemDc`].
struct Dib(Option<HBITMAP>);

impl Dib {
    fn get(&self) -> HBITMAP {
        self.0.unwrap_or(std::ptr::null_mut())
    }

    fn disarm(&mut self) -> HBITMAP {
        self.0.take().unwrap_or(std::ptr::null_mut())
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        if let Some(bitmap) = self.0.take()
            && !bitmap.is_null()
        {
            unsafe {
                DeleteObject(bitmap);
            }
        }
    }
}

/// Allocate one 32bpp top-down DIB, run `paint` over the owned bytes (it
/// returns the caller checksum), present with `UpdateLayeredWindow`
/// (`ULW_ALPHA`), free everything, then run `anchor` for band placement.
/// The pixel slice never outlives the call. A `paint` panic propagates to
/// the caller after the device context and bitmap are freed.
fn present_with(
    hwnd: u64,
    outer: Rect,
    label: &str,
    paint: impl FnOnce(&mut [u8]) -> u64,
    anchor: impl FnOnce() -> Result<(), String>,
) -> Result<u64, String> {
    let total = surface_len(outer, label)?;
    let mut memdc = MemDc(Some(unsafe { CreateCompatibleDC(std::ptr::null_mut()) }));
    if memdc.get().is_null() {
        return Err(format!("error: {label} DC failed"));
    }
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = outer.w;
    bmi.bmiHeader.biHeight = -outer.h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbmp = unsafe {
        CreateDIBSection(
            memdc.get(),
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    let mut dib = Dib(Some(hbmp));
    if hbmp.is_null() || bits.is_null() {
        return Err(format!("error: {label} DIB failed"));
    }
    let checksum = unsafe {
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, total);
        paint(pixels)
    };
    let old = unsafe { SelectObject(memdc.get(), dib.get()) };
    let dst = POINT {
        x: outer.x,
        y: outer.y,
    };
    let size = SIZE {
        cx: outer.w,
        cy: outer.h,
    };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: 0,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let presented = unsafe {
        UpdateLayeredWindow(
            hwnd_of(hwnd),
            std::ptr::null_mut(),
            &dst,
            &size,
            memdc.get(),
            &src,
            0,
            &blend,
            ULW_ALPHA,
        )
    };
    unsafe {
        SelectObject(memdc.get(), old);
    }
    let hbmp_raw = dib.disarm();
    let memdc_raw = memdc.disarm();
    unsafe {
        DeleteObject(hbmp_raw);
        DeleteDC(memdc_raw);
    }
    if presented == 0 {
        return Err(format!("error: {label} present failed"));
    }
    anchor()?;
    Ok(checksum)
}

/// Paint one ring/fill surface and anchor it below its target in its band.
pub fn present_layered(
    hwnd: u64,
    outer: Rect,
    topmost: bool,
    anchor: u64,
    label: &str,
    paint: impl FnOnce(&mut [u8]) -> u64,
) -> Result<u64, String> {
    present_with(hwnd, outer, label, paint, || {
        place_below(hwnd, outer, topmost, anchor)
    })
}

/// Paint one preview fill and assert the topmost band above windows.
pub fn present_preview(
    hwnd: u64,
    outer: Rect,
    label: &str,
    paint: impl FnOnce(&mut [u8]) -> u64,
) -> Result<u64, String> {
    present_with(hwnd, outer, label, paint, || place_above(hwnd, outer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_class_names_match_policy() {
        // The carrier and the policy lifecycles must name the same three
        // surfaces, or residue audits miscount.
        assert_eq!(
            class_name(OverlayClass::Border),
            crate::active_border_sys::OVERLAY_CLASS
        );
        assert_eq!(
            class_name(OverlayClass::Underlay),
            crate::active_border_sys::UNDERLAY_CLASS
        );
        assert_eq!(
            class_name(OverlayClass::Preview),
            crate::active_border_sys::PREVIEW_CLASS
        );
    }

    #[test]
    fn accent_dirty_flag_is_sticky_until_consumed() {
        assert!(!take_accent_dirty());
        mark_accent_dirty();
        mark_accent_dirty();
        assert!(take_accent_dirty());
        assert!(!take_accent_dirty());
    }

    #[test]
    fn surface_guard_rejects_degenerate_and_absurd() {
        let rect = |w: i32, h: i32| Rect { x: 0, y: 0, w, h };
        assert!(surface_len(rect(100, 80), "active-border").is_ok());
        assert!(surface_len(rect(0, 80), "active-border").is_err());
        assert!(surface_len(rect(100, 0), "active-border").is_err());
        assert!(surface_len(rect(-5, 80), "active-border").is_err());
        assert!(surface_len(rect(16385, 80), "active-border").is_err());
        // Error vocabulary stays caller-labelled.
        assert_eq!(
            surface_len(rect(0, 1), "group-underlay").unwrap_err(),
            "error: group-underlay bad paint geometry"
        );
        assert_eq!(
            surface_len(rect(16385, 1), "drop-preview").unwrap_err(),
            "error: drop-preview surface too large"
        );
    }
}
