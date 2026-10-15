//! Bounded background icon lookup (`cfg(windows)` only).
//!
//! One worker thread serves the tile loop's [`IconChannel`]: it pops one
//! [`WindowKey`](crate::workspace::WindowKey) at a time, resolves the
//! artwork with no shared lock held, and pushes one owned [`IconImage`]
//! (or a miss). The worker sleeps on a wake signal when no work is queued:
//! no periodic wakes while idle or suppressed. Lookup chain per window:
//!
//! 1. Full-identity verification before and after the lookup with the
//!    existing [`HeldProcess`](crate::native::HeldProcess) helper: the
//!    HWND's live pid must equal the key pid and the held identity's
//!    process creation must equal the key creation, or the result is a
//!    `refused` miss. Same-process HWND reuse (one process, two windows,
//!    one recycled number) shares pid and creation, so it is not fenced by
//!    this key model; the exe path and artwork of same-process windows
//!    normally coincide, and the periodic positive refresh bounds any
//!    staleness.
//! 2. `WM_GETICON` `ICON_SMALL2` / `ICON_SMALL` / `ICON_BIG` via
//!    `SendMessageTimeoutW` (`SMTO_ABORTIFHUNG`, 50ms each): hung clients
//!    fail fast and never stall the tile loop.
//! 3. Class icons (`GCLP_HICONSM` / `GCLP_HICON`).
//! 4. Executable fallback using the already-verified exe path, then
//!    `ExtractIconExW` (no `SHGetFileInfo`, no COM). A
//!    `ApplicationFrameHost.exe` owner is omitted (its exe artwork is not
//!    the packaged app's icon); exact AUMID shell resolution stays
//!    deferred, so such minis keep the plain rectangle.
//!
//! Borrowed `WM_GETICON`/class handles are copied with `CopyIcon` and the
//! originals are never destroyed; extracted handles are owned and destroyed
//! exactly once. Every DC, DIB section, and process handle is RAII-released;
//! drawing failures return a miss, never a gray hit.
//!
//! Alpha semantics, stated honestly: `DrawIconEx(DI_NORMAL)` blends the
//! artwork over a [`MINI_GRAY`](crate::pager_icons::MINI_GRAY)-filled 32-bit
//! top-down DIB at lookup time. The returned buffer is opaque (alpha bytes
//! forced to `0xFF`); the painter blits it with `StretchDIBits(SRCCOPY)`,
//! which ignores alpha. Icon translucency is therefore flattened onto the
//! miniature gray once, never composited per paint, and the opaque overlay
//! hosting is unchanged.
//!
//! Shutdown never joins the worker: `Drop` clears the queue, raises the
//! stop flag, and wakes the worker, which exits after at most the current
//! native call. Message lookups are timeout-bounded, but the exe fallback's
//! `ExtractIconExW` has no timeout: shutdown does not wait for it, so no
//! claim is made that all work is bounded. No foreign query or wait ever
//! runs on the tile loop or in the paint path.

use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GdiFlush, HBITMAP, HDC, SelectObject,
};
use windows_sys::Win32::UI::Shell::ExtractIconExW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CopyIcon, DestroyIcon, DrawIconEx, GetClassLongPtrW, GetWindowThreadProcessId, HICON, IsWindow,
    SendMessageTimeoutW,
};

use crate::native::HeldProcess;
use crate::pager_icons::{
    ICON_RENDER_PX, IconChannel, IconImage, IconResult, IconSource, MINI_GRAY,
};
use crate::workspace::WindowKey;

const WM_GETICON: u32 = 0x007F;
const ICON_SMALL: usize = 0;
const ICON_BIG: usize = 1;
const ICON_SMALL2: usize = 2;
const SMTO_ABORTIFHUNG: u32 = 0x0002;
const ICON_MSG_TIMEOUT_MS: u32 = 50;
const GCLP_HICONSM: i32 = -34;
const GCLP_HICON: i32 = -14;
const DI_NORMAL: u32 = 0x0003;
/// `ApplicationFrameHost.exe` owns UWP windows; its exe icon is not the app.
const FRAME_HOST_EXE: &str = "applicationframehost.exe";

/// Owned icon handle: destroyed exactly once.
struct OwnedIcon(HICON);

impl Drop for OwnedIcon {
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

fn copy_borrowed(raw: HICON) -> Option<OwnedIcon> {
    if raw.is_null() {
        return None;
    }
    let owned = unsafe { CopyIcon(raw) };
    if owned.is_null() {
        None
    } else {
        Some(OwnedIcon(owned))
    }
}

/// Classic `WM_GETICON` chain: `ICON_SMALL2`, `ICON_SMALL`, `ICON_BIG`.
fn message_icon(hwnd: HWND) -> Option<OwnedIcon> {
    for kind in [ICON_SMALL2, ICON_SMALL, ICON_BIG] {
        let mut result: usize = 0;
        let sent = unsafe {
            SendMessageTimeoutW(
                hwnd,
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

fn class_icon(hwnd: HWND) -> Option<OwnedIcon> {
    for index in [GCLP_HICONSM, GCLP_HICON] {
        let raw = unsafe { GetClassLongPtrW(hwnd, index) } as HICON;
        if let Some(icon) = copy_borrowed(raw) {
            return Some(icon);
        }
    }
    None
}

fn is_frame_host(exe: &str) -> bool {
    exe.rsplit(['\\', '/'])
        .next()
        .is_some_and(|base| base.eq_ignore_ascii_case(FRAME_HOST_EXE))
}

/// Live pid behind one HWND, or `None` when the window is gone.
fn window_pid(hwnd: HWND) -> Option<u32> {
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    (pid != 0).then_some(pid)
}

/// Verified owner of one lookup: the HWND plus its held process (the open
/// handle pins the process against pid reuse for the whole lookup).
struct VerifiedOwner {
    hwnd: HWND,
    process: HeldProcess,
    exe: String,
}

/// Verify the full key identity before the lookup: the HWND must be live,
/// its current pid must equal the key pid, and the held process creation
/// must equal the key creation. Uses only the existing native identity
/// helper; no duplicate process machinery lives here.
fn verify_before(key: &WindowKey) -> Option<VerifiedOwner> {
    if key.hwnd == 0 {
        return None;
    }
    let hwnd = key.hwnd as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    if window_pid(hwnd) != Some(key.pid) {
        return None;
    }
    let process = HeldProcess::open(key.pid).ok()?;
    let identity = process.identity().ok()?;
    if identity.process_creation != key.creation {
        return None;
    }
    Some(VerifiedOwner {
        hwnd,
        process,
        exe: identity.exe_path,
    })
}

/// Re-verify after the lookup: the HWND must still resolve to the key pid
/// and the held process must still be alive. Catches window close and HWND
/// reuse across processes mid-lookup.
fn verify_after(key: &WindowKey, owner: &VerifiedOwner) -> bool {
    window_pid(owner.hwnd) == Some(key.pid) && owner.process.is_alive()
}

/// Executable fallback from the already-verified exe path: never presents
/// the UWP host exe as app artwork. `ExtractIconExW` reads the file only
/// (no `SHGetFileInfo`, no COM) and returns owned handles.
fn exe_icon(exe: &str) -> Option<OwnedIcon> {
    if is_frame_host(exe) {
        return None;
    }
    let path: Vec<u16> = exe.encode_utf16().chain([0]).collect();
    let mut large: HICON = std::ptr::null_mut();
    let mut small: HICON = std::ptr::null_mut();
    let count = unsafe { ExtractIconExW(path.as_ptr(), 0, &mut large, &mut small, 1) };
    if count == 0 || count == u32::MAX {
        return None;
    }
    // Small matches the miniature scale; the unused handle is destroyed.
    if !small.is_null() {
        if !large.is_null() {
            unsafe {
                DestroyIcon(large);
            }
        }
        Some(OwnedIcon(small))
    } else if !large.is_null() {
        Some(OwnedIcon(large))
    } else {
        None
    }
}

/// Render one owned icon onto a miniature-gray DIB and return the opaque
/// bytes. Alpha flattens here; paint only copies. Any failure (DIB, select,
/// draw) returns a miss: a gray hit would lie about artwork presence.
fn render_icon(icon: &OwnedIcon) -> Option<IconImage> {
    let size = ICON_RENDER_PX as i32;
    let hdc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
    if hdc.is_null() {
        return None;
    }
    let _dc = DcGuard(hdc);
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = size;
    bmi.bmiHeader.biHeight = -size;
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
    // Opaque miniature-gray backing: transparent artwork regions read gray.
    let stride = size as usize * 4;
    let total = stride * size as usize;
    let drawn = unsafe {
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, total);
        for i in 0..total / 4 {
            let px = &mut pixels[i * 4..i * 4 + 4];
            px[0] = MINI_GRAY.2;
            px[1] = MINI_GRAY.1;
            px[2] = MINI_GRAY.0;
            px[3] = 0xFF;
        }
        let drawn = DrawIconEx(
            hdc,
            0,
            0,
            icon.0,
            size,
            size,
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
    let bgra = unsafe {
        let mut bgra = vec![0u8; total];
        bgra.copy_from_slice(std::slice::from_raw_parts(bits as *const u8, total));
        // `SRCCOPY` ignores the alpha byte, but keep the buffer honestly
        // opaque: icon blending may leave the alpha lane untouched.
        for i in 0..total / 4 {
            bgra[i * 4 + 3] = 0xFF;
        }
        bgra
    };
    restore(hdc, old);
    let image = IconImage::new(ICON_RENDER_PX, ICON_RENDER_PX, bgra);
    image.is_valid().then_some(image)
}

/// One finished lookup: artwork plus its source for bounded outcome counts.
struct LookupOutcome {
    image: Option<IconImage>,
    source: IconSource,
}

fn lookup_icon(key: &WindowKey) -> LookupOutcome {
    let refused = || LookupOutcome {
        image: None,
        source: IconSource::Refused,
    };
    let miss = || LookupOutcome {
        image: None,
        source: IconSource::Miss,
    };
    let Some(owner) = verify_before(key) else {
        return refused();
    };
    // Borrowed window/class handles are copied inside; the originals are
    // never destroyed. Extracted handles arrive owned. Each stage verifies
    // identity before accepting pixels; a failed verification refuses at
    // once instead of trying a possibly-recycled next stage.
    if let Some(icon) = message_icon(owner.hwnd) {
        if !verify_after(key, &owner) {
            return refused();
        }
        if let Some(image) = render_icon(&icon)
            && verify_after(key, &owner)
        {
            return LookupOutcome {
                image: Some(image),
                source: IconSource::Window,
            };
        }
        if !verify_after(key, &owner) {
            return refused();
        }
    }
    if let Some(icon) = class_icon(owner.hwnd) {
        if !verify_after(key, &owner) {
            return refused();
        }
        if let Some(image) = render_icon(&icon)
            && verify_after(key, &owner)
        {
            return LookupOutcome {
                image: Some(image),
                source: IconSource::Class,
            };
        }
        if !verify_after(key, &owner) {
            return refused();
        }
    }
    if let Some(icon) = exe_icon(&owner.exe)
        && verify_after(key, &owner)
        && let Some(image) = render_icon(&icon)
        && verify_after(key, &owner)
    {
        return LookupOutcome {
            image: Some(image),
            source: IconSource::Exe,
        };
    }
    if !verify_after(key, &owner) {
        return refused();
    }
    miss()
}

/// Shared worker state: the bounded channel plus its wake signal. The tile
/// loop only locks briefly (never waits); the worker waits while idle.
struct WorkerShared {
    channel: Mutex<IconChannel>,
    wake: Condvar,
}

fn lock_shared(shared: &WorkerShared) -> std::sync::MutexGuard<'_, IconChannel> {
    shared
        .channel
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn worker_loop(shared: Arc<WorkerShared>, stop: Arc<AtomicBool>) {
    loop {
        // Stop is checked before every pop; the queue may hold requests at
        // shutdown only if `Drop` races a `request`, and those are dropped
        // with the service.
        let next = {
            let mut channel = lock_shared(&shared);
            while channel.pending_len() == 0 && !stop.load(Ordering::SeqCst) {
                channel = shared
                    .wake
                    .wait(channel)
                    .unwrap_or_else(|poison| poison.into_inner());
            }
            if stop.load(Ordering::SeqCst) {
                return;
            }
            channel.pop_request()
        };
        match next {
            Some((key, generation)) => {
                // No shared lock across the native lookup: pop and push are
                // the only critical sections.
                let outcome = lookup_icon(&key);
                let result = IconResult {
                    key,
                    image: outcome.image.map(Arc::new),
                    source: outcome.source,
                    generation,
                };
                // A stale (pre-suppression) result is refused at push and
                // simply dropped here.
                lock_shared(&shared).push_result(result);
            }
            None => {
                // Spurious wake with an empty queue: loop back to the wait.
            }
        }
    }
}

/// Owner of the single background lookup worker and its bounded queues.
/// `Drop` clears the queue, raises the stop flag, and wakes the worker, so
/// at most the current native call finishes; shutdown never waits for it
/// (and the exe fallback's `ExtractIconExW` is not timeout-bounded, so no
/// claim is made that all work is bounded).
pub struct PagerIconService {
    shared: Arc<WorkerShared>,
    stop: Arc<AtomicBool>,
}

impl PagerIconService {
    /// Spawn the worker; `None` when the thread itself cannot start (the
    /// host retries on a capped cadence and the pager stays icon-less).
    #[must_use]
    pub fn try_new() -> Option<Self> {
        let shared = Arc::new(WorkerShared {
            channel: Mutex::new(IconChannel::new()),
            wake: Condvar::new(),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let worker_shared = Arc::clone(&shared);
        let worker_stop = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("omntiler-pager-icons".to_owned())
            .spawn(move || worker_loop(worker_shared, worker_stop))
            .ok()?;
        Some(Self { shared, stop })
    }

    /// Queue lookup requests (deduped against queued and in-flight work,
    /// bounded; overflow drops the newest) and wake the worker.
    pub fn request(&self, keys: Vec<WindowKey>) {
        if keys.is_empty() {
            return;
        }
        let mut pushed = 0;
        {
            let mut channel = lock_shared(&self.shared);
            for key in keys {
                pushed += usize::from(channel.push_request(key));
            }
        }
        if pushed > 0 {
            self.shared.wake.notify_one();
        }
    }

    /// Drain finished lookups (hits and misses) for the cache.
    #[must_use]
    pub fn drain(&self) -> Vec<IconResult> {
        let mut channel = lock_shared(&self.shared);
        let mut out = Vec::with_capacity(channel.results_len());
        while let Some(result) = channel.pop_result() {
            out.push(result);
        }
        out
    }

    /// Drop queued (not yet started) requests for keys no longer drawable.
    pub fn prune_pending(&self, keep: &std::collections::HashSet<WindowKey>) -> usize {
        lock_shared(&self.shared).prune_pending(keep)
    }

    /// Suppression: clear queued work and stop future requests. An
    /// already-started lookup may still finish; its stale result is refused
    /// at push, and fresh results apply only to still-live keys. No wake:
    /// the worker stays asleep until the next request.
    pub fn clear(&self) {
        lock_shared(&self.shared).clear();
    }
}

impl Drop for PagerIconService {
    fn drop(&mut self) {
        // Clear first so nothing queued survives, then stop and wake: the
        // worker exits instead of sleeping through shutdown. No join, so
        // shutdown never blocks on `ExtractIconExW`.
        {
            let mut channel = lock_shared(&self.shared);
            channel.clear();
            self.stop.store(true, Ordering::SeqCst);
        }
        self.shared.wake.notify_all();
    }
}
