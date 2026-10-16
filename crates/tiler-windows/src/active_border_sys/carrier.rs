use super::*;

/// Lowest window in `EnumWindows` top-to-bottom order among `hwnds`: the
/// renderable anchor the underlay sorts directly beneath. `None` when no
/// candidate is enumerated (closed, or the order is unreadable).
pub(crate) fn lowest_in_z(hwnds: &[u64]) -> Option<u64> {
    let order = top_to_bottom_hwnds()?;
    order
        .iter()
        .rev()
        .find(|hwnd| hwnds.contains(hwnd))
        .copied()
}

/// True only when the cached overlay needs a same-geometry reassert: the
/// surface is gone, hidden, misplaced, or displaced below the fresh target.
/// Adjacent-below is the achievable correct placement for a background
/// owner's surface under its foreground target, so only true-below (or an
/// unreadable probe, which recovers by reasserting) reasserts for the border.
/// The underlay (`for_underlay`) additionally reasserts when above its anchor:
/// its anchor is the lowest renderable group member, so above-anchor is never
/// an acceptable cached placement. The reassert is a `place_overlay` in the
/// target's own band directly beneath the fresh target HWND, never a blind
/// raise over unrelated topmost or shell UI.
pub(super) fn overlay_needs_reassert(
    overlay_u64: u64,
    outer: Rect,
    target_hwnd: u64,
    for_underlay: bool,
) -> bool {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindowVisible};
    if overlay_u64 == 0 {
        return true;
    }
    let overlay = overlay_u64 as usize as HWND;
    if !is_overlay_window(overlay_u64) {
        return true;
    }
    if unsafe { IsWindowVisible(overlay) } == 0 {
        return true;
    }
    let mut actual: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(overlay, &mut actual) } == 0 {
        return true;
    }
    let w = actual.right - actual.left;
    let h = actual.bottom - actual.top;
    if actual.left != outer.x || actual.top != outer.y || w != outer.w || h != outer.h {
        return true;
    }
    if target_hwnd == 0 || target_hwnd == overlay_u64 {
        return false;
    }
    // Behind means displaced (a foreign window between, or far below):
    // adjacent-below is the achievable correct placement for a background
    // owner's surface under its foreground target, so only true-below
    // reasserts for the border. The underlay anchor is the lowest renderable
    // member, so an above-anchor cached surface is equally displaced and must
    // also reassert. Unknown order (endpoint vanished mid-probe, or
    // enumeration failed) recovers by reasserting: a needless same-geometry
    // move is harmless, a missed displacement is not.
    z_needs_reassert(z_relation(overlay_u64, target_hwnd), for_underlay)
}

/// Pure z verdict shared by both carriers so the underlay Above repair is
/// unit-covered without a live window. Border preserves its foreground-below
/// assumption (`Above` stays acceptable); the underlay never accepts an
/// above-anchor cached placement.
pub(super) fn z_needs_reassert(relation: Option<ZRelation>, for_underlay: bool) -> bool {
    match relation {
        Some(ZRelation::AdjacentBelow) => false,
        Some(ZRelation::Above) => for_underlay,
        Some(ZRelation::Below) | None => true,
    }
}

pub(super) fn is_overlay_window(hwnd_u64: u64) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
    if hwnd_u64 == 0 {
        return false;
    }
    unsafe { IsWindow(hwnd_u64 as usize as HWND) != 0 }
}

/// Creator-thread liveness read for one owned carrier window: the shared
/// readback behind renderer-side own-anchor validation and pre-present
/// ensure. Read-only, never a write.
pub(crate) fn overlay_window_alive(hwnd_u64: u64) -> bool {
    is_overlay_window(hwnd_u64)
}

/// Z-order relation between the owned overlay and the fresh target in
/// `EnumWindows` top-to-bottom order. A background owner cannot place its
/// surface above the foreground window (live probe: steady-state overlay
/// adjacent-below the focused helper despite `HWND_TOP`), so `AdjacentBelow`
/// is the achievable correct placement for an eligible foreground target:
/// directly beneath it, above everything else, ring pixels uncovered.
/// `GetWindow` handle walks are invalid evidence (they skip the toolwindow
/// overlay against normal targets). Read-only enumeration, no writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ZRelation {
    Above,
    AdjacentBelow,
    Below,
}

/// Z-order probe: relation of the overlay to the fresh target, `None` when
/// either endpoint is gone or the order is unreadable.
fn z_relation(overlay_u64: u64, target_hwnd: u64) -> Option<ZRelation> {
    let order = top_to_bottom_hwnds()?;
    let mut upper_idx: Option<usize> = None;
    let mut lower_idx: Option<usize> = None;
    for (i, hwnd) in order.iter().enumerate() {
        if *hwnd == overlay_u64 && upper_idx.is_none() {
            upper_idx = Some(i);
        }
        if *hwnd == target_hwnd && lower_idx.is_none() {
            lower_idx = Some(i);
        }
        if upper_idx.is_some() && lower_idx.is_some() {
            break;
        }
    }
    match (upper_idx, lower_idx) {
        (Some(u), Some(l)) if u < l => Some(ZRelation::Above),
        (Some(u), Some(l)) if u == l + 1 => Some(ZRelation::AdjacentBelow),
        (Some(_), Some(_)) => Some(ZRelation::Below),
        _ => None,
    }
}

/// Shared `EnumWindows` top-to-bottom snapshot for z-order evidence.
fn top_to_bottom_hwnds() -> Option<Vec<u64>> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::EnumWindows;
    unsafe extern "system" fn enum_proc(hwnd: HWND, state: LPARAM) -> i32 {
        let out = unsafe { &mut *(state as *mut Vec<u64>) };
        out.push(hwnd as usize as u64);
        1
    }
    let mut out: Vec<u64> = Vec::new();
    let ok = unsafe { EnumWindows(Some(enum_proc), &mut out as *mut Vec<u64> as LPARAM) };
    if ok == 0 { None } else { Some(out) }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

pub(super) fn create_overlay_window() -> Result<u64, DynError> {
    create_overlay_window_in(OVERLAY_CLASS)
}

static BORDER_CLASS_REGISTERED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static UNDERLAY_CLASS_REGISTERED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static PREVIEW_CLASS_REGISTERED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn overlay_class_registered(class: &str) -> bool {
    use std::sync::atomic::Ordering;
    if class == UNDERLAY_CLASS {
        UNDERLAY_CLASS_REGISTERED.load(Ordering::SeqCst)
    } else if class == PREVIEW_CLASS {
        PREVIEW_CLASS_REGISTERED.load(Ordering::SeqCst)
    } else {
        BORDER_CLASS_REGISTERED.load(Ordering::SeqCst)
    }
}

fn mark_overlay_class_registered(class: &str) {
    use std::sync::atomic::Ordering;
    if class == UNDERLAY_CLASS {
        UNDERLAY_CLASS_REGISTERED.store(true, Ordering::SeqCst);
    } else if class == PREVIEW_CLASS {
        PREVIEW_CLASS_REGISTERED.store(true, Ordering::SeqCst);
    } else {
        BORDER_CLASS_REGISTERED.store(true, Ordering::SeqCst);
    }
}

/// Create one owned carrier window in `class`. Registration is attempted on
/// every creation and `ERROR_CLASS_ALREADY_EXISTS` (1410) is tolerated, so
/// the border and underlay classes each register exactly once via the two
/// per-class `AtomicBool`s above (`BORDER_CLASS_REGISTERED` /
/// `UNDERLAY_CLASS_REGISTERED`). Creation is rare (at most once per carrier
/// per process).
pub(crate) fn create_overlay_window_in(class: &str) -> Result<u64, DynError> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, RegisterClassW, WNDCLASSW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    };
    if !overlay_class_registered(class) {
        let class_w = wide(class);
        let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
        let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
        cls.style = CS_HREDRAW | CS_VREDRAW;
        cls.lpfnWndProc = Some(overlay_wnd_proc);
        cls.hInstance = hinst;
        cls.lpszClassName = class_w.as_ptr();
        let atom = unsafe { RegisterClassW(&cls) };
        if atom == 0 {
            // Tolerate a lost race / prior registration in this process:
            // ERROR_CLASS_ALREADY_EXISTS (1410) means the class is usable.
            let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if code != 1410 {
                return Err(err("error: overlay RegisterClass failed"));
            }
        }
        mark_overlay_class_registered(class);
    }
    let class_w = wide(class);
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
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
        return Err(err("error: active-border CreateWindow failed"));
    }
    Ok(hwnd as usize as u64)
}

pub(super) fn hide_window(hwnd_u64: u64) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
    unsafe {
        ShowWindow(hwnd_u64 as usize as HWND, SW_HIDE);
    }
    window_hidden(hwnd_u64)
}

/// True when the owned window reads hidden (or is already gone): the
/// creator-side readback behind every verified hide acknowledgement.
fn window_hidden(hwnd_u64: u64) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    if !is_overlay_window(hwnd_u64) {
        return true;
    }
    unsafe { IsWindowVisible(hwnd_u64 as usize as HWND) == 0 }
}

pub(super) fn destroy_window(hwnd_u64: u64) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
    unsafe {
        DestroyWindow(hwnd_u64 as usize as HWND);
    }
    // Verified destroyed readback: a surviving HWND reads live so the
    // caller preserves the handle for a bounded retry instead of claiming
    // success over a live window.
    !is_overlay_window(hwnd_u64)
}

/// Z-order anchors: ordinary targets stay in the normal band and sit directly
/// beneath the target; topmost targets use the topmost band the same way.
/// Live probes proved `HWND_TOP` a silent no-op for z-order here (a direct
/// `SetWindowPos(TOP)` from the background owner succeeds yet never
/// reorders), while passing the fresh target HWND as `hWndInsertAfter`
/// places the overlay immediately below it. The band still follows the
/// target every paint/move, never permanently over unrelated topmost/shell.
fn insert_after(topmost: bool) -> windows_sys::Win32::Foundation::HWND {
    if topmost {
        windows_sys::Win32::UI::WindowsAndMessaging::HWND_TOPMOST as usize
            as windows_sys::Win32::Foundation::HWND
    } else {
        windows_sys::Win32::UI::WindowsAndMessaging::HWND_TOP as usize
            as windows_sys::Win32::Foundation::HWND
    }
}

/// Position the owned overlay directly beneath its fresh target without
/// leaving its band. Ordinary targets: a `NOTOPMOST` demote-if-needed (no-op
/// when already normal) followed by placement with the target HWND as
/// `hWndInsertAfter` (immediately below the target). Topmost targets: a
/// `TOPMOST` promote-if-needed followed by the same below-target placement
/// inside the topmost band. A zero `target_hwnd` falls back to the band
/// anchor (best effort, no adjacency). Read-only band input, owned-window
/// writes only, never over unrelated topmost or shell UI.
fn place_overlay(
    hwnd_u64: u64,
    outer: Rect,
    topmost: bool,
    target_hwnd: u64,
) -> Result<(), DynError> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos,
    };
    let band = |after: HWND| unsafe {
        SetWindowPos(
            hwnd_u64 as usize as HWND,
            after,
            outer.x,
            outer.y,
            outer.w,
            outer.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    };
    if topmost {
        if band(insert_after(true)) == 0 {
            return Err(err("error: active-border position failed"));
        }
    } else if band(HWND_NOTOPMOST as usize as HWND) == 0 {
        return Err(err("error: active-border position failed"));
    }
    if target_hwnd != 0 && target_hwnd != hwnd_u64 {
        let below_target = unsafe {
            SetWindowPos(
                hwnd_u64 as usize as HWND,
                target_hwnd as usize as HWND,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        };
        if below_target == 0 {
            return Err(err("error: active-border position failed"));
        }
        return Ok(());
    }
    if band(insert_after(topmost)) == 0 {
        return Err(err("error: active-border position failed"));
    }
    Ok(())
}

pub(super) fn move_overlay(
    hwnd_u64: u64,
    outer: Rect,
    topmost: bool,
    target_hwnd: u64,
) -> Result<(), DynError> {
    place_overlay(hwnd_u64, outer, topmost, target_hwnd)
}

/// Paint the ring into a fresh 32bpp top-down DIB and present it with
/// `UpdateLayeredWindow` (`ULW_ALPHA`). Returns an FNV-1a checksum over the
/// owned DIB bytes (proof support without screen capture), computed in the
/// single paint pass (no second full-buffer hash walk).
pub(super) fn paint_and_present(
    hwnd_u64: u64,
    outer: Rect,
    width_px: i32,
    radius_px: i32,
    color: (u8, u8, u8),
    topmost: bool,
    target_hwnd: u64,
) -> Result<u64, DynError> {
    if width_px < 0 || radius_px < 0 {
        return Err(err("error: active-border bad paint geometry"));
    }
    if width_px == 0 {
        hide_window(hwnd_u64);
        return Ok(0);
    }
    present_pixels(
        hwnd_u64,
        outer,
        topmost,
        target_hwnd,
        "active-border",
        |pixels| paint_ring(pixels, outer.w, outer.h, width_px, radius_px, color),
    )
}

/// Paint the premultiplied-alpha group fill into a fresh 32bpp top-down DIB
/// and present it on the shared carrier pipeline. Same checksum/placement
/// contract as the ring path; only the painter differs.
pub(super) fn paint_and_present_fill(
    hwnd_u64: u64,
    outer: Rect,
    color: (u8, u8, u8),
    alpha: u8,
    topmost: bool,
    anchor_hwnd: u64,
) -> Result<u64, DynError> {
    present_pixels(
        hwnd_u64,
        outer,
        topmost,
        anchor_hwnd,
        "group-underlay",
        |pixels| crate::group_underlay::paint_fill_argb(pixels, outer.w, outer.h, color, alpha),
    )
}

/// Position the drop-preview surface ABOVE windows in the topmost band
/// (KWin overlay-item analogue). Unlike [`place_overlay`] there is no
/// below-target step: the filled target slot must stay above normal app
/// windows while shown. `SWP_NOACTIVATE` keeps it nonactivating; the
/// `WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` creation
/// flags keep it click-through and out of Alt+Tab. Callers hide immediately
/// on cancel/finish/suspend/teardown so the topmost band is never pinned
/// over unrelated shell UI.
pub(super) fn place_preview_above(hwnd_u64: u64, outer: Rect) -> Result<(), DynError> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos,
    };
    let ok = unsafe {
        SetWindowPos(
            hwnd_u64 as usize as HWND,
            HWND_TOPMOST as usize as HWND,
            outer.x,
            outer.y,
            outer.w,
            outer.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    };
    if ok == 0 {
        return Err(err("error: drop-preview position failed"));
    }
    Ok(())
}

/// Paint the premultiplied-alpha drop-preview fill and present it ABOVE
/// windows on the shared DIB pipeline, then assert the topmost band.
/// Same checksum contract as the underlay path; only the placement differs.
pub(super) fn paint_and_present_preview_fill(
    hwnd_u64: u64,
    outer: Rect,
    color: (u8, u8, u8),
    alpha: u8,
) -> Result<u64, DynError> {
    use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
    use windows_sys::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
    if outer.w <= 0 || outer.h <= 0 {
        return Err(err("error: drop-preview bad paint geometry"));
    }
    if outer.w > 16384 || outer.h > 16384 {
        return Err(err("error: drop-preview surface too large"));
    }
    let memdc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
    if memdc.is_null() {
        return Err(err("error: drop-preview DC failed"));
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
            memdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    if hbmp.is_null() || bits.is_null() {
        unsafe {
            DeleteDC(memdc);
        }
        return Err(err("error: drop-preview DIB failed"));
    }
    let stride = outer.w as usize * 4;
    let total = stride * outer.h as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, total) };
    let checksum = crate::group_underlay::paint_fill_argb(pixels, outer.w, outer.h, color, alpha);
    let old = unsafe { SelectObject(memdc, hbmp) };
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
            hwnd_u64 as usize as HWND,
            std::ptr::null_mut(),
            &dst,
            &size,
            memdc,
            &src,
            0,
            &blend,
            ULW_ALPHA,
        )
    };
    unsafe {
        SelectObject(memdc, old);
        DeleteObject(hbmp);
        DeleteDC(memdc);
    }
    if presented == 0 {
        return Err(err("error: drop-preview present failed"));
    }
    place_preview_above(hwnd_u64, outer)?;
    Ok(checksum)
}

/// True only when the cached preview needs a same-geometry reassert: the
/// surface is gone, hidden, or misplaced. Z-order against the mover is not
/// probed here (the preview presents topmost by construction); a hidden or
/// misplaced surface re-presents, anything else stays as-is.
pub(super) fn preview_needs_reassert(preview_u64: u64, outer: Rect) -> bool {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindowVisible};
    if preview_u64 == 0 || !is_overlay_window(preview_u64) {
        return true;
    }
    if unsafe { IsWindowVisible(preview_u64 as usize as HWND) } == 0 {
        return true;
    }
    let mut actual: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(preview_u64 as usize as HWND, &mut actual) } == 0 {
        return true;
    }
    let w = actual.right - actual.left;
    let h = actual.bottom - actual.top;
    actual.left != outer.x || actual.top != outer.y || w != outer.w || h != outer.h
}

/// Shared carrier presentation: allocate one 32bpp top-down DIB, run the
/// caller painter over the owned bytes (it returns the FNV-1a checksum),
/// present with `UpdateLayeredWindow` (`ULW_ALPHA`), then anchor below the
/// target in its band.
fn present_pixels(
    hwnd_u64: u64,
    outer: Rect,
    topmost: bool,
    target_hwnd: u64,
    what: &str,
    paint: impl FnOnce(&mut [u8]) -> u64,
) -> Result<u64, DynError> {
    use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
    use windows_sys::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
    if outer.w <= 0 || outer.h <= 0 {
        return Err(err(format!("error: {what} bad paint geometry")));
    }
    // Bound the surface: a full-monitor overlay plus ring is the largest
    // legitimate case; anything absurd fails closed instead of allocating.
    if outer.w > 16384 || outer.h > 16384 {
        return Err(err(format!("error: {what} surface too large")));
    }
    let memdc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
    if memdc.is_null() {
        return Err(err(format!("error: {what} DC failed")));
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
            memdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    if hbmp.is_null() || bits.is_null() {
        unsafe {
            DeleteDC(memdc);
        }
        return Err(err(format!("error: {what} DIB failed")));
    }
    let stride = outer.w as usize * 4;
    let total = stride * outer.h as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, total) };
    let checksum = paint(pixels);
    let old = unsafe { SelectObject(memdc, hbmp) };
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
            hwnd_u64 as usize as HWND,
            std::ptr::null_mut(),
            &dst,
            &size,
            memdc,
            &src,
            0,
            &blend,
            ULW_ALPHA,
        )
    };
    unsafe {
        SelectObject(memdc, old);
        DeleteObject(hbmp);
        DeleteDC(memdc);
    }
    if presented == 0 {
        return Err(err(format!("error: {what} present failed")));
    }
    // Target-relative anchor (not blindly permanent TOPMOST): above the
    // ordinary frame, topmost only while the target is topmost. Task
    // View/Alt+Tab interplay needs live evidence.
    place_overlay(hwnd_u64, outer, topmost, target_hwnd)?;
    Ok(checksum)
}
