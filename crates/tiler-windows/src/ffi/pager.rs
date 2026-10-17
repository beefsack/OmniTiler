//! Owned per-output taskbar pager overlays.
//!
//! The overlay window class, every `extern` window procedure, the
//! per-window painted frame, the GDI paint path, the taskbar reads, and the
//! calibration flags all live here. Windows are opaque owning
//! [`PagerWindow`] values: thread-confined (`!Send`/`!Sync` via the raw
//! handle, never `unsafe impl`), non-cloneable, destroyed exactly once by
//! `Drop`. Safe callers never handle numeric ids, raw context pointers, or
//! state pointers: frames go in as owned [`PagerFrame`] snapshots (artwork
//! arrives as shared `Arc<IconImage>`, never re-buffered), and clicks come
//! out through a caller sink.
//!
//! Window-procedure state access is validated, never trusted: each live
//! window registers its state pointer under its id, and the procedure only
//! dereferences a `GWLP_USERDATA` value that still equals the registered
//! pointer for that id. Anything else (cleared, foreign, or stale) falls
//! through to `DefWindowProcW`. Callbacks never run under a state borrow:
//! the procedure clones the click plus its sink, releases every borrow,
//! and only then calls out (inside `catch_unwind`, so a panicking sink is
//! swallowed instead of unwinding across the boundary).
//!
//! Threading: pager windows are confined to the creating (loop) thread;
//! the procedure runs there. The registry lock guards pointer bookkeeping
//! only and is never held across paint or callbacks. The click and
//! calibration sinks run on that thread; implementations must never call
//! back into the pager window functions (reentrancy) and must never hold
//! owner locks across a pump.
//!
//! Ownership: each window's frame box is created with the window and owned
//! by its [`PagerWindow`]; the registry entry is removed before
//! `DestroyWindow`, so exactly one owner exists either way. A failed
//! `CreateWindowExW` frees the box inline, including its font.

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use tiler_core::geometry::Rect;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, CreateFontW, CreatePen, CreateSolidBrush,
    DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint,
    FillRect, HDC, HFONT, InvalidateRect, LineTo, MoveToEx, PAINTSTRUCT, PS_SOLID, SelectObject,
    SetBkMode, SetTextColor, StretchDIBits, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Shell::{ABM_GETSTATE, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
    EnumWindows, FindWindowExW, FindWindowW, GWLP_USERDATA, GetClassNameW, GetClientRect,
    GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, IDC_ARROW, IsWindowVisible, LoadCursorW,
    MA_NOACTIVATE, RegisterClassW, RegisterWindowMessageW, SW_HIDE, SW_SHOWNA, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_CREATE,
    WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_PAINT,
    WM_SETTINGCHANGE, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WS_VISIBLE,
};

use crate::pager_icons::IconImage;

/// Owned overlay window class (distinct from tray/border/underlay/preview so
/// residue audits count the carrier exactly). The class string is owned
/// here so creation can never pair an unrelated existing window procedure
/// with this carrier: only this name is ever registered, with the internal
/// procedure below.
pub const PAGER_WINDOW_CLASS: &str = "OmniTilerTaskbarPager";

const CLASS_BUSY: u32 = 1410;
/// 64 wide chars: window class names are short (`Shell_SecondaryTrayWnd`
/// is 22), so this always holds the full name for comparison.
const CLASS_NAME_CAP: usize = 64;
const SECONDARY_TRAY_CLASS: &str = "Shell_SecondaryTrayWnd";
const PRIMARY_TRAY_CLASS: &str = "Shell_TrayWnd";
const TRAY_NOTIFY_CLASS: &str = "TrayNotifyWnd";
const SRCCOPY: u32 = 0x00CC0020;
/// 16384px sanity bound: rejects corrupt/off-screen rects before layout.
const RECT_LIMIT_PX: i32 = 16384;

static REDISCOVER: AtomicBool = AtomicBool::new(false);
static TASKBAR_MSG: AtomicU32 = AtomicU32::new(0);
/// Set when a display/DPI/settings broadcast lands: the loop thread hides
/// every strip at once and revalidates from fresh facts before showing again,
/// so no stale pixels survive a calibration change.
static STALE_HIDE: AtomicBool = AtomicBool::new(false);

/// Whether Explorer (or display/DPI calibration) asked for rediscovery.
pub fn take_rediscover() -> bool {
    REDISCOVER.swap(false, Ordering::SeqCst)
}

/// Whether a calibration broadcast asked for an immediate hide plus
/// revalidation (consumed once by the host).
pub fn take_stale_hide() -> bool {
    STALE_HIDE.swap(false, Ordering::SeqCst)
}

fn want_rediscover() {
    REDISCOVER.store(true, Ordering::SeqCst);
}

fn want_stale_hide() {
    STALE_HIDE.store(true, Ordering::SeqCst);
    want_rediscover();
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

/// One miniature slot: its rect plus optional precomputed icon blit. The
/// icon destination rect is computed by the caller (the canonical
/// miniature/icon sizing stays with portable policy); paint blits blindly.
/// Artwork is shared (`Arc`), never re-buffered across the boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniSlot {
    pub rect: Rect,
    pub icon: Option<IconBlit>,
}

/// One opaque icon blit into its centered slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconBlit {
    pub image: Arc<IconImage>,
    pub dest: Rect,
}

/// One window's painted frame: workspace ids in cell order, the output key,
/// cell rects, the active index, and per-cell miniature slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerFrame {
    pub ids: Vec<String>,
    pub output: String,
    pub cells: Vec<Rect>,
    pub active: usize,
    pub minis: Vec<Vec<MiniSlot>>,
}

/// One rendered click: the output key, workspace id, and 1-based ordinal
/// (logging only, never a positional fallback).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerClick {
    pub output: String,
    pub workspace: String,
    pub ordinal: u32,
}

/// Receiver for rendered clicks. Runs on the creating (loop) thread inside
/// the window procedure, after every state borrow is released; must never
/// call back into the pager window functions and must never hold owner
/// locks across a pump. Panics are swallowed at the boundary.
pub type PagerClickSink = Arc<dyn Fn(PagerClick) + Send + Sync>;

/// Calibration hook for display/DPI/settings broadcasts. Runs on the
/// creating thread inside the window procedure at broadcast time (the exact
/// timing the monitor-dirty mark always had), after every state borrow is
/// released; same reentrancy discipline as the click sink.
pub type PagerCalibrationSink = Arc<dyn Fn() + Send + Sync>;

/// Per-window painted frame plus its sinks. Owned by [`PagerWindow`];
/// the procedure reaches it only through validated registration below.
struct PagerWindowState {
    frame: PagerFrame,
    font: HFONT,
    sink: PagerClickSink,
    calibration: PagerCalibrationSink,
}

impl Drop for PagerWindowState {
    fn drop(&mut self) {
        if !self.font.is_null() {
            unsafe {
                DeleteObject(self.font as _);
            }
        }
    }
}

/// Live-window registry: window id to the state-pointer address owned by
/// its [`PagerWindow`]. Integer addresses keep the table unconditionally
/// `Send + Sync`; the cast back to a pointer happens only after validation,
/// inside this module. An entry exists exactly while its owner is alive; the
/// procedure dereferences a `USERDATA` value only while it still equals the
/// registered address for that id. The lock guards bookkeeping only and is
/// never held across paint or sink calls. Entries are added and removed on
/// the creating thread, where the procedure also runs.
fn slots() -> &'static Mutex<HashMap<u64, usize>> {
    static SLOTS: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hwnd_key(hwnd: HWND) -> u64 {
    hwnd as usize as u64
}

fn register_window(hwnd: HWND, state: *mut PagerWindowState) {
    if let Ok(mut guard) = slots().lock() {
        guard.insert(hwnd_key(hwnd), state as usize);
    }
}

fn unregister_window(hwnd: HWND) {
    if let Ok(mut guard) = slots().lock() {
        guard.remove(&hwnd_key(hwnd));
    }
}

/// Run `f` over validated window state: the `USERDATA` value must still
/// equal the registered address for `hwnd`, otherwise `None` (fail closed
/// to `DefWindowProcW`). The borrow ends before this returns; only owned
/// clones cross into the caller, so no alias outlives validation.
fn with_validated<R>(hwnd: HWND, f: impl FnOnce(&PagerWindowState) -> R) -> Option<R> {
    let user = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as usize;
    if user == 0 {
        return None;
    }
    let guard = slots().lock().ok()?;
    let registered = *guard.get(&hwnd_key(hwnd))?;
    if registered != user {
        return None;
    }
    Some(f(unsafe { &*(registered as *mut PagerWindowState) }))
}

/// Owned pager overlay window: thread-confined, non-cloneable, destroyed
/// exactly once by `Drop`. The raw handle field keeps it `!Send`/`!Sync`
/// structurally (no `unsafe impl` anywhere); the state box is owned here
/// and shared with the procedure only through validated registration.
pub struct PagerWindow {
    hwnd: HWND,
    dpi: u32,
    state: Box<PagerWindowState>,
}

impl PagerWindow {
    /// Create one overlay window owning `frame`, showing `strip` topmost
    /// without activation; `None` when creation fails. The frame box
    /// (including its font) is freed inline on failure.
    pub fn create(
        strip: Rect,
        frame: PagerFrame,
        dpi: u32,
        sink: &PagerClickSink,
        calibration: &PagerCalibrationSink,
    ) -> Option<Self> {
        let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class_w = wide(PAGER_WINDOW_CLASS);
        let title_w = wide("OmniTiler taskbar pager");
        let mut state = Box::new(PagerWindowState {
            frame,
            font: pager_font(dpi),
            sink: Arc::clone(sink),
            calibration: Arc::clone(calibration),
        });
        let raw: *mut PagerWindowState = &mut *state;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class_w.as_ptr(),
                title_w.as_ptr(),
                WS_POPUP | WS_VISIBLE,
                strip.x,
                strip.y,
                strip.w,
                strip.h,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                raw.cast(),
            )
        };
        if hwnd.is_null() {
            // Creation failed: the frame box owns a live HFONT here (it is
            // normally freed with the window), so drop it (which releases
            // the font) before returning.
            drop(state);
            return None;
        }
        register_window(hwnd, raw);
        Some(Self { hwnd, dpi, state })
    }

    /// Overlay DPI the frame was laid out for (a change recreates the
    /// window so font and geometry match the new facts).
    pub fn dpi(&self) -> u32 {
        self.dpi
    }

    /// Replace the painted frame. The caller change-gates (steady state
    /// never calls).
    pub fn set_frame(&mut self, frame: PagerFrame) {
        self.state.frame = frame;
    }

    /// Move, topmost-show, and repaint at the strip.
    pub fn move_show(&self, strip: Rect) {
        unsafe {
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                strip.x,
                strip.y,
                strip.w,
                strip.h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            InvalidateRect(self.hwnd, std::ptr::null(), 1);
        }
    }

    /// Show without activation when hidden.
    pub fn ensure_shown(&self) {
        if unsafe { IsWindowVisible(self.hwnd) } == 0 {
            unsafe {
                ShowWindow(self.hwnd, SW_SHOWNA);
            }
        }
    }

    /// Hide (suppression turns): the window persists for the next safe
    /// turn, so no destroy/create churn hides information.
    pub fn hide(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// Reassert topmost without moving, sizing, or activating.
    pub fn reassert_topmost(&self) {
        unsafe {
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }
    }
}

impl Drop for PagerWindow {
    /// Unregister first (later procedure traffic misses the registry and
    /// takes default handling), destroy the window, then free the frame box
    /// and its font. Process exit destroys windows implicitly (no residue).
    fn drop(&mut self) {
        unregister_window(self.hwnd);
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

/// Blit one worker-flattened icon into its centered slot. The buffer is
/// opaque by construction, so `SRCCOPY` is exact: no alpha channel travels
/// the paint path and the overlay stays a plain opaque GDI surface.
fn paint_icon(hdc: HDC, dest: RECT, image: &IconImage) {
    if !image.is_valid() {
        return;
    }
    let w = dest.right - dest.left;
    let h = dest.bottom - dest.top;
    if w <= 0 || h <= 0 {
        return;
    }
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = image.width as i32;
    bmi.bmiHeader.biHeight = -(image.height as i32);
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    unsafe {
        StretchDIBits(
            hdc,
            dest.left,
            dest.top,
            w,
            h,
            0,
            0,
            image.width as i32,
            image.height as i32,
            image.bgra.as_ptr().cast(),
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
}

fn paint_frame(hwnd: HWND, frame: &PagerFrame, font: HFONT) {
    let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    if hdc.is_null() {
        return;
    }
    let mut client: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        GetClientRect(hwnd, &mut client);
    }
    for (index, cell) in frame.cells.iter().enumerate() {
        let origin_x = frame.cells.first().map(|first| first.x).unwrap_or(0);
        let local = RECT {
            left: cell.x - origin_x,
            top: 0,
            right: cell.x - origin_x + cell.w,
            bottom: cell.h,
        };
        let active = index == frame.active;
        let (r, g, b) = if active { (42, 130, 218) } else { (58, 58, 58) };
        let brush = unsafe { CreateSolidBrush(rgb(r, g, b)) };
        if !brush.is_null() {
            unsafe {
                FillRect(hdc, &local, brush);
                DeleteObject(brush as _);
            }
        }
        // Miniature lighter window rectangles plus owned artwork blits.
        for slot in frame.minis.get(index).cloned().unwrap_or_default() {
            let mini = slot.rect;
            let local_mini = RECT {
                left: mini.x - origin_x,
                top: mini.y - cell.y,
                right: mini.x - origin_x + mini.w,
                bottom: mini.y - cell.y + mini.h,
            };
            let fill = unsafe { CreateSolidBrush(rgb(168, 168, 168)) };
            if !fill.is_null() {
                unsafe {
                    FillRect(hdc, &local_mini, fill);
                    DeleteObject(fill as _);
                }
            }
            let pen = unsafe { CreatePen(PS_SOLID, 1, rgb(215, 215, 215)) };
            if !pen.is_null() {
                unsafe {
                    let old = SelectObject(hdc, pen as _);
                    MoveToEx(hdc, local_mini.left, local_mini.top, std::ptr::null_mut());
                    LineTo(hdc, local_mini.right - 1, local_mini.top);
                    LineTo(hdc, local_mini.right - 1, local_mini.bottom - 1);
                    LineTo(hdc, local_mini.left, local_mini.bottom - 1);
                    LineTo(hdc, local_mini.left, local_mini.top);
                    SelectObject(hdc, old);
                    DeleteObject(pen as _);
                }
            }
            // Owned artwork blits opaquely into the centered slot (omitted
            // when the miniature cannot fit a legible icon: the caller
            // leaves such slots icon-less).
            if let Some(blit) = slot.icon {
                paint_icon(
                    hdc,
                    RECT {
                        left: blit.dest.x - origin_x,
                        top: blit.dest.y - cell.y,
                        right: blit.dest.x - origin_x + blit.dest.w,
                        bottom: blit.dest.y - cell.y + blit.dest.h,
                    },
                    &blit.image,
                );
            }
        }
        // Light 1px cell border.
        let pen = unsafe { CreatePen(PS_SOLID, 1, rgb(200, 200, 200)) };
        if !pen.is_null() {
            unsafe {
                let old = SelectObject(hdc, pen as _);
                MoveToEx(hdc, local.left, local.top, std::ptr::null_mut());
                LineTo(hdc, local.right - 1, local.top);
                LineTo(hdc, local.right - 1, local.bottom - 1);
                LineTo(hdc, local.left, local.bottom - 1);
                LineTo(hdc, local.left, local.top);
                SelectObject(hdc, old);
                DeleteObject(pen as _);
            }
        }
        // Active outline plus the 1-based number on every cell.
        if active {
            let pen = unsafe { CreatePen(PS_SOLID, 2, rgb(255, 255, 255)) };
            if !pen.is_null() {
                unsafe {
                    let old = SelectObject(hdc, pen as _);
                    MoveToEx(hdc, local.left + 1, local.top + 1, std::ptr::null_mut());
                    LineTo(hdc, local.right - 2, local.top + 1);
                    LineTo(hdc, local.right - 2, local.bottom - 2);
                    LineTo(hdc, local.left + 1, local.bottom - 2);
                    LineTo(hdc, local.left + 1, local.top + 1);
                    SelectObject(hdc, old);
                    DeleteObject(pen as _);
                }
            }
        }
        let label = wide(&(index + 1).to_string());
        let mut text_rect = local;
        unsafe {
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, rgb(255, 255, 255));
            // Scalable per-overlay font (created at the overlay DPI, deleted
            // with the window). A failed font falls back to the stock font:
            // numbers stay legible, never missing.
            let old_font = if font.is_null() {
                std::ptr::null_mut()
            } else {
                SelectObject(hdc, font as _)
            };
            DrawTextW(
                hdc,
                label.as_ptr(),
                -1,
                &mut text_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            if !old_font.is_null() {
                SelectObject(hdc, old_font);
            }
        }
    }
    unsafe {
        EndPaint(hwnd, &ps);
    }
}

/// Hit-test one click point against the stored cells: the rendered click,
/// or `None` when the point names no live cell. Pure over the owned frame
/// (no window reads), so it unit-tests without a desktop.
fn hit_test(frame: &PagerFrame, x: i32, y: i32) -> Option<PagerClick> {
    let origin = frame.cells.first().map(|first| (first.x, first.y));
    let (ox, oy) = origin?;
    let px = ox + x;
    let py = oy + y;
    let index = frame.cells.iter().position(|cell| {
        px >= cell.x && px < cell.x + cell.w && py >= cell.y && py < cell.y + cell.h
    })?;
    let workspace = frame.ids.get(index)?;
    Some(PagerClick {
        output: frame.output.clone(),
        workspace: workspace.clone(),
        ordinal: index as u32 + 1,
    })
}

unsafe extern "system" fn pager_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CREATE {
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return 0;
    }
    // Explorer restarts arrive as the registered broadcast, never WM_CREATE.
    if msg == TASKBAR_MSG.load(Ordering::SeqCst) && msg != 0 {
        want_rediscover();
        return 0;
    }
    if msg == WM_DISPLAYCHANGE || msg == WM_SETTINGCHANGE || msg == WM_DPICHANGED {
        // Geometry/DPI calibration may have moved the taskbar: run the
        // calibration hook at broadcast time (the monitor-dirty timing the
        // owner has always relied on), flag the hide plus revalidation for
        // the loop thread, then hide this overlay at once (app-local, no
        // foreign interaction) so no stale pixels survive the change.
        // Every borrow is released before calling out; a panicking hook is
        // swallowed (the flags still land below).
        if let Some(calibration) = with_validated(hwnd, |state| Arc::clone(&state.calibration)) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| calibration()));
        }
        want_stale_hide();
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
            InvalidateRect(hwnd, std::ptr::null(), 1);
        }
        return 0;
    }
    if msg == WM_MOUSEACTIVATE {
        return MA_NOACTIVATE as LRESULT;
    }
    if msg == WM_PAINT {
        // Clone the painted snapshot under validation, then paint with no
        // borrow held: nothing aliases the live state during GDI calls.
        if let Some((frame, font)) = with_validated(hwnd, |state| (state.frame.clone(), state.font))
        {
            paint_frame(hwnd, &frame, font);
        }
        return 0;
    }
    if msg == WM_LBUTTONDOWN {
        // Same borrow discipline: resolve the click plus its sink, release,
        // then deliver. Keep-latest overwrite stays with the sink.
        let x = (lparam & 0xffff) as i32;
        let y = ((lparam >> 16) & 0xffff) as i32;
        let ready = with_validated(hwnd, |state| {
            hit_test(&state.frame, x, y).map(|click| (Arc::clone(&state.sink), click))
        })
        .flatten();
        if let Some((sink, click)) = ready {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink(click)));
        }
        return 0;
    }
    if msg == WM_DESTROY {
        // Owned by the window value (unregister-then-destroy there): never
        // touch state here, so exactly one owner exists either way.
        let _ = wparam;
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Scalable per-overlay number font at `dpi` (9pt Segoe UI, like the
/// first-run dialog). Null when creation fails: paint then falls back to the
/// stock font instead of skipping numbers.
fn pager_font(dpi: u32) -> HFONT {
    let height = -((9 * dpi.max(1) as i32 + 36) / 72);
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            0,
            wide("Segoe UI").as_ptr(),
        )
    }
}

/// Register the overlay class (tolerating a previous registration); false
/// when registration genuinely fails.
pub fn ensure_pager_class() -> bool {
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class_w = wide(PAGER_WINDOW_CLASS);
    let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
    cls.style = CS_HREDRAW | CS_VREDRAW;
    cls.lpfnWndProc = Some(pager_wnd_proc);
    cls.hInstance = hinst;
    cls.hCursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
    cls.lpszClassName = class_w.as_ptr();
    let atom = unsafe { RegisterClassW(&cls) };
    if atom == 0 {
        let code = super::process::last_error();
        if code != CLASS_BUSY {
            return false;
        }
    }
    true
}

/// Register the Explorer-restart broadcast id; false when registration
/// fails (the caller degrades, as before).
pub fn register_taskbar_message(name: &str) -> bool {
    let id = unsafe { RegisterWindowMessageW(wide(name).as_ptr()) };
    if id == 0 {
        return false;
    }
    TASKBAR_MSG.store(id, Ordering::SeqCst);
    want_rediscover();
    true
}

fn rect_from_raw(raw: RECT) -> Option<Rect> {
    if raw.right <= raw.left || raw.bottom <= raw.top {
        return None;
    }
    let w = raw.right.saturating_sub(raw.left);
    let h = raw.bottom.saturating_sub(raw.top);
    if w <= 0 || h <= 0 || w > RECT_LIMIT_PX || h > RECT_LIMIT_PX {
        return None;
    }
    Some(Rect {
        x: raw.left,
        y: raw.top,
        w,
        h,
    })
}

fn window_rect_of(hwnd: HWND) -> Option<Rect> {
    let mut raw: RECT = unsafe { std::mem::zeroed() };
    if hwnd.is_null() || unsafe { GetWindowRect(hwnd, &mut raw) } == 0 {
        return None;
    }
    rect_from_raw(raw)
}

/// One taskbar probe: raw bar bounds, the proven tray-area left edge
/// (`None` when the tray child is unknown), live visibility, the raw
/// `ABM_GETSTATE` switch word, and the live taskbar DPI (0 when that query
/// itself fails: the caller substitutes its fallback, never an assumption).
#[derive(Debug, Clone, Copy)]
pub struct TaskbarProbe {
    pub bar: Option<Rect>,
    pub tray_left: Option<i32>,
    pub visible: bool,
    pub autohide_state: usize,
    pub dpi: u32,
}

/// Raw `ABM_GETSTATE` switch word (read-only: `cbSize`-only `APPBARDATA`,
/// no window handle, no callback, no registration, never `ABM_SETSTATE`).
/// Zero reads as not-auto-hiding (a failed query fails open on the switch
/// only; geometry/visibility refusals still fail closed).
pub fn query_appbar_state() -> usize {
    let mut data: APPBARDATA = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<APPBARDATA>() as u32;
    unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) }
}

/// Read the primary taskbar fresh: `Shell_TrayWnd` rect and visibility plus
/// the `TrayNotifyWnd` tray edge and the live taskbar DPI. Cheap enough for
/// every pump (five reads, no enumeration); `None` when Explorer is gone or
/// any geometry read fails (the host treats that as unknown facts: hide,
/// never lay out from memory).
pub fn read_primary_taskbar() -> Option<TaskbarProbe> {
    let tray = unsafe { FindWindowW(wide(PRIMARY_TRAY_CLASS).as_ptr(), std::ptr::null()) };
    if tray.is_null() {
        return None;
    }
    let bar = window_rect_of(tray)?;
    let notify = unsafe {
        FindWindowExW(
            tray,
            std::ptr::null_mut(),
            wide(TRAY_NOTIFY_CLASS).as_ptr(),
            std::ptr::null(),
        )
    };
    let tray_left = (!notify.is_null())
        .then(|| window_rect_of(notify))
        .flatten()
        .map(|notify_rect| notify_rect.x);
    let dpi = unsafe { GetDpiForWindow(tray) };
    Some(TaskbarProbe {
        bar: Some(bar),
        tray_left,
        visible: unsafe { IsWindowVisible(tray) } != 0,
        autohide_state: query_appbar_state(),
        dpi,
    })
}

unsafe extern "system" fn secondary_enum(hwnd: HWND, state: LPARAM) -> i32 {
    let out = unsafe { &mut *(state as *mut Vec<HWND>) };
    let mut name = [0u16; CLASS_NAME_CAP];
    let len = unsafe { GetClassNameW(hwnd, name.as_mut_ptr(), name.len() as i32) };
    if len > 0 {
        let class = String::from_utf16_lossy(&name[..len as usize]);
        if class == SECONDARY_TRAY_CLASS {
            out.push(hwnd);
        }
    }
    1
}

/// Enumerate secondary taskbars (`Shell_SecondaryTrayWnd`). Heavier than the
/// primary read, so the host calls this only on the slow revalidate cadence.
/// Every entry refuses layout: the tray edge is unprovable there, so this
/// only feeds refusal logging, never positions.
pub fn discover_secondary_taskbars() -> Vec<TaskbarProbe> {
    let mut secondaries: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(
            Some(secondary_enum),
            &mut secondaries as *mut Vec<HWND> as LPARAM,
        );
    }
    let mut bars = Vec::new();
    for hwnd in secondaries {
        if let Some(bar) = window_rect_of(hwnd) {
            bars.push(TaskbarProbe {
                bar: Some(bar),
                tray_left: None,
                visible: unsafe { IsWindowVisible(hwnd) } != 0,
                autohide_state: 0,
                dpi: 0,
            });
        }
    }
    bars
}

/// Stateless `ABS_AUTOHIDE` test for one `ABM_GETSTATE` result: only a
/// positive auto-hide bit hides. Zero (query failed or switch off) reads as
/// not-auto-hiding; unrelated state bits never hide.
pub fn autohide_from_state(state: usize) -> bool {
    (state as u32 & ABS_AUTOHIDE) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pager_icons::IconImage;

    fn frame_with_cells() -> PagerFrame {
        PagerFrame {
            ids: vec!["ws-a".to_owned(), "ws-b".to_owned()],
            output: "mon-a".to_owned(),
            cells: vec![
                Rect {
                    x: 100,
                    y: 1040,
                    w: 72,
                    h: 40,
                },
                Rect {
                    x: 172,
                    y: 1040,
                    w: 72,
                    h: 40,
                },
            ],
            active: 0,
            minis: vec![Vec::new(), Vec::new()],
        }
    }

    #[test]
    fn autohide_bit_is_the_only_hide_signal() {
        assert!(!autohide_from_state(0), "failed query never hides");
        assert!(autohide_from_state(ABS_AUTOHIDE as usize));
        assert!(autohide_from_state(u32::MAX as usize));
        assert!(
            !autohide_from_state(0x02),
            "unrelated state bits never hide"
        );
    }

    #[test]
    fn stale_hide_flag_hides_once_and_requests_rediscover() {
        // Drain any residue so the consume-once assertion is hermetic (no
        // other test in this module touches these flags).
        let _ = take_stale_hide();
        let _ = take_rediscover();
        // The calibration-broadcast path sets both: the host hides every
        // strip at once, then revalidates from fresh facts on the next pump.
        want_stale_hide();
        assert!(take_stale_hide(), "host must observe the hide once");
        assert!(!take_stale_hide(), "hide never replays a second pump");
        assert!(take_rediscover(), "revalidation still follows the hide");
    }

    #[test]
    fn hit_test_names_cells_without_a_window() {
        let frame = frame_with_cells();
        // Inside each cell (client coords relative to the strip origin).
        let first = hit_test(&frame, 5, 5).expect("first cell");
        assert_eq!(
            first,
            PagerClick {
                output: "mon-a".to_owned(),
                workspace: "ws-a".to_owned(),
                ordinal: 1,
            }
        );
        let second = hit_test(&frame, 77, 5).expect("second cell");
        assert_eq!(second.workspace, "ws-b");
        assert_eq!(second.ordinal, 2);
        // Right edge is exclusive (72 starts the next cell); past the
        // last cell, below the strip, or far outside is no click.
        assert!(hit_test(&frame, 144, 5).is_none());
        assert!(hit_test(&frame, 5, 50).is_none());
        assert!(hit_test(&frame, -200, -200).is_none());
        // No cells: no click, never a guess.
        let empty = PagerFrame {
            ids: Vec::new(),
            output: "mon-a".to_owned(),
            cells: Vec::new(),
            active: 0,
            minis: Vec::new(),
        };
        assert!(hit_test(&empty, 0, 0).is_none());
    }

    #[test]
    fn hit_test_needs_a_live_workspace_id() {
        // A cell without a parallel id refuses instead of guessing.
        let mut frame = frame_with_cells();
        frame.ids.pop();
        assert!(hit_test(&frame, 77, 5).is_none());
        assert!(hit_test(&frame, 5, 5).is_some());
    }

    #[test]
    fn icon_slots_share_artwork_without_copying() {
        // The boundary holds shared artwork: cloning a frame bumps the
        // `Arc`, never the pixels (the procedure clones per paint).
        let image = Arc::new(IconImage::new(32, 32, vec![7; 32 * 32 * 4]));
        let frame = PagerFrame {
            ids: vec!["ws-a".to_owned()],
            output: "mon-a".to_owned(),
            cells: vec![Rect {
                x: 0,
                y: 0,
                w: 72,
                h: 40,
            }],
            active: 0,
            minis: vec![vec![MiniSlot {
                rect: Rect {
                    x: 1,
                    y: 1,
                    w: 10,
                    h: 10,
                },
                icon: Some(IconBlit {
                    image: Arc::clone(&image),
                    dest: Rect {
                        x: 2,
                        y: 2,
                        w: 8,
                        h: 8,
                    },
                }),
            }]],
        };
        let painted = frame.clone();
        let slot = painted.minis[0][0].icon.as_ref().expect("slot");
        assert!(Arc::ptr_eq(&slot.image, &image));
    }
}
