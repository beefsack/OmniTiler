//! Background icon lookup primitives.
//!
//! Borrowed `WM_GETICON`/class handles are copied with `CopyIcon` and the
//! originals are never destroyed; extracted handles are owned and destroyed
//! exactly once. Every icon handle is RAII-owned here as an opaque
//! [`AppIcon`]; every DC and DIB section is released inline. Drawing
//! failures return a miss (`None`), never a gray hit.
//!
//! Alpha semantics, stated honestly: `DrawIconEx(DI_NORMAL)` blends the
//! artwork over a caller-supplied gray-filled 32-bit top-down DIB at lookup
//! time. The returned buffer is opaque (alpha bytes forced to `0xFF`); the
//! painter blits it with `StretchDIBits(SRCCOPY)`, which ignores alpha.
//!
//! The lookup chain policy (identity verification, source order, miss
//! vocabulary) stays with the caller; this module only resolves one window
//! or executable to owned pixels. Icon handles are confined to the calling
//! thread (no `Send`/`Sync` is implemented).

use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GdiFlush, HBITMAP, HDC, SelectObject,
};
use windows_sys::Win32::UI::Shell::ExtractIconExW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CopyIcon, DestroyIcon, DrawIconEx, GetClassLongPtrW, HICON, SendMessageTimeoutW,
};

use super::hwnd_of;

const WM_GETICON: u32 = 0x007F;
const ICON_SMALL: usize = 0;
const ICON_BIG: usize = 1;
const ICON_SMALL2: usize = 2;
const SMTO_ABORTIFHUNG: u32 = 0x0002;
const ICON_MSG_TIMEOUT_MS: u32 = 50;
const GCLP_HICONSM: i32 = -34;
const GCLP_HICON: i32 = -14;
const DI_NORMAL: u32 = 0x0003;
const ICON_SIDE_CAP_PX: u32 = 256;

/// Owned icon handle: destroyed exactly once. The raw handle never leaves
/// this module; callers only render it to owned pixels.
pub struct AppIcon(HICON);

impl Drop for AppIcon {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                DestroyIcon(self.0);
            }
        }
    }
}

struct DcGuard(HDC);

impl Drop for DcGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                DeleteDC(self.0);
            }
        }
    }
}

struct BitmapGuard(HBITMAP);

impl Drop for BitmapGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                DeleteObject(self.0 as _);
            }
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn copy_borrowed(raw: HICON) -> Option<AppIcon> {
    if raw.is_null() {
        return None;
    }
    let owned = unsafe { CopyIcon(raw) };
    if owned.is_null() {
        None
    } else {
        Some(AppIcon(owned))
    }
}

/// Classic `WM_GETICON` chain: `ICON_SMALL2`, `ICON_SMALL`, `ICON_BIG`, each
/// timeout-bounded (`SMTO_ABORTIFHUNG`, 50ms) so hung clients fail fast and
/// never stall the tile loop.
pub fn message_icon(hwnd: u64) -> Option<AppIcon> {
    if hwnd == 0 {
        return None;
    }
    for kind in [ICON_SMALL2, ICON_SMALL, ICON_BIG] {
        let mut result: usize = 0;
        let sent = unsafe {
            SendMessageTimeoutW(
                hwnd_of(hwnd),
                WM_GETICON,
                kind,
                0,
                SMTO_ABORTIFHUNG,
                ICON_MSG_TIMEOUT_MS,
                &mut result,
            )
        };
        if sent != 0
            && let Some(icon) = copy_borrowed(result as HICON)
        {
            return Some(icon);
        }
    }
    None
}

/// Class icons (`GCLP_HICONSM` / `GCLP_HICON`), copied, never destroyed.
pub fn class_icon(hwnd: u64) -> Option<AppIcon> {
    if hwnd == 0 {
        return None;
    }
    for index in [GCLP_HICONSM, GCLP_HICON] {
        let raw = unsafe { GetClassLongPtrW(hwnd_of(hwnd), index) } as HICON;
        if let Some(icon) = copy_borrowed(raw) {
            return Some(icon);
        }
    }
    None
}

/// Executable fallback from a verified exe path (`ExtractIconExW` reads the
/// file only, no `SHGetFileInfo`, no COM) and returns owned handles. Small
/// matches the miniature scale; the unused handle is destroyed. The caller
/// owns exe allow-listing (e.g. omitting a host exe whose icon is not the
/// app's).
pub fn exe_icon(exe_path: &str) -> Option<AppIcon> {
    if exe_path.is_empty() {
        return None;
    }
    let path = wide(exe_path);
    let mut large: HICON = std::ptr::null_mut();
    let mut small: HICON = std::ptr::null_mut();
    let count = unsafe { ExtractIconExW(path.as_ptr(), 0, &mut large, &mut small, 1) };
    if count == 0 || count == u32::MAX {
        return None;
    }
    if !small.is_null() {
        if !large.is_null() {
            unsafe {
                DestroyIcon(large);
            }
        }
        Some(AppIcon(small))
    } else if !large.is_null() {
        Some(AppIcon(large))
    } else {
        None
    }
}

/// One rendered icon: opaque top-down BGRA bytes (`size * size * 4`),
/// artwork alpha already flattened onto `gray`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconPixels {
    pub size: u32,
    pub bgra: Vec<u8>,
}

/// Render one owned icon onto a gray DIB and return the opaque bytes. Any
/// failure (DIB, select, draw) returns a miss: a gray hit would lie about
/// artwork presence.
pub fn render_icon(icon: &AppIcon, size: u32, gray: (u8, u8, u8)) -> Option<IconPixels> {
    if size == 0 || size > ICON_SIDE_CAP_PX {
        return None;
    }
    let side = size as i32;
    let hdc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
    if hdc.is_null() {
        return None;
    }
    let _dc = DcGuard(hdc);
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = side;
    bmi.bmiHeader.biHeight = -side;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let bitmap = unsafe {
        CreateDIBSection(
            hdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    if bitmap.is_null() {
        return None;
    }
    // A null backing store with a live bitmap would leak the section on the
    // early return below: release it before bailing.
    if bits.is_null() {
        unsafe {
            DeleteObject(bitmap as _);
        }
        return None;
    }
    let _bitmap = BitmapGuard(bitmap);
    let old = unsafe { SelectObject(hdc, bitmap as _) };
    if old.is_null() {
        return None;
    }
    // From here the bitmap is selected in: restore before every return so
    // the guards never free a selected object.
    let restore = |hdc: HDC, old: windows_sys::Win32::Graphics::Gdi::HGDIOBJ| unsafe {
        SelectObject(hdc, old);
    };
    // Opaque gray backing: transparent artwork regions read gray.
    let stride = size as usize * 4;
    let total = stride * size as usize;
    let drawn = unsafe {
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, total);
        for i in 0..total / 4 {
            let px = &mut pixels[i * 4..i * 4 + 4];
            px[0] = gray.2;
            px[1] = gray.1;
            px[2] = gray.0;
            px[3] = 0xFF;
        }
        let drawn = DrawIconEx(
            hdc,
            0,
            0,
            icon.0,
            side,
            side,
            0,
            std::ptr::null_mut(),
            DI_NORMAL,
        );
        GdiFlush();
        drawn
    };
    if drawn == 0 {
        restore(hdc, old);
        return None;
    }
    let mut bgra = vec![0u8; total];
    unsafe {
        bgra.copy_from_slice(std::slice::from_raw_parts(bits as *const u8, total));
    }
    // `SRCCOPY` ignores the alpha byte, but keep the buffer honestly
    // opaque: icon blending may leave the alpha lane untouched.
    for i in 0..total / 4 {
        bgra[i * 4 + 3] = 0xFF;
    }
    restore(hdc, old);
    Some(IconPixels { size, bgra })
}
