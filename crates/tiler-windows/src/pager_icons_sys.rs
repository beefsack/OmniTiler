#![forbid(unsafe_code)]
//! Background icon lookup (`cfg(windows)` only).
//!
//! One worker thread serves the tile loop's [`IconChannel`]: it pops one
//! [`WindowKey`](crate::native_guards::WindowKey) at a time, resolves the
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
//! Icon handles, DIB sections, and every Win32 call live in
//! [`crate::ffi`]: borrowed handles are copied there and the originals are
//! never destroyed, extracted handles arrive owned and are destroyed
//! exactly once there, and drawing failures return a miss, never a gray
//! hit. This module holds only the chain policy: identity verification,
//! source order, and the worker service.
//!
//! Alpha semantics, stated honestly: artwork blends over a
//! [`MINI_GRAY`](crate::pager_icons::MINI_GRAY)-filled 32-bit top-down DIB
//! at lookup time. The returned buffer is opaque (alpha bytes forced to
//! `0xFF`); the painter blits it with `StretchDIBits(SRCCOPY)`, which
//! ignores alpha. Icon translucency is therefore flattened onto the
//! miniature gray once, never composited per paint, and the opaque overlay
//! hosting is unchanged.
//!
//! Shutdown never joins the worker: `Drop` clears the queue, raises the
//! stop flag, and wakes the worker, which exits after at most the current
//! native call. Message lookups are timeout-bounded, but the exe fallback's
//! `ExtractIconExW` has no timeout: shutdown does not wait for it, so no
//! claim is made that all work is bounded. No foreign query or wait ever
//! runs on the tile loop or in the paint path.

use std::os::windows::io::OwnedHandle;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};

use crate::ffi::{self, AppIcon};
use crate::native::HeldProcess;
use crate::pager_icons::{
    ICON_RENDER_PX, IconChannel, IconImage, IconResult, IconSource, MINI_GRAY,
};
use crate::native_guards::WindowKey;

/// `ApplicationFrameHost.exe` owns UWP windows; its exe icon is not the app.
const FRAME_HOST_EXE: &str = "applicationframehost.exe";

fn is_frame_host(exe: &str) -> bool {
    exe.rsplit(['\\', '/'])
        .next()
        .is_some_and(|base| base.eq_ignore_ascii_case(FRAME_HOST_EXE))
}

/// Verified owner of one lookup: the HWND plus its held process (the open
/// handle pins the process against pid reuse for the whole lookup).
struct VerifiedOwner {
    hwnd: u64,
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
    if !ffi::is_window(key.hwnd) {
        return None;
    }
    if ffi::window_pid(key.hwnd) != key.pid {
        return None;
    }
    let process = HeldProcess::open(key.pid).ok()?;
    let identity = process.identity().ok()?;
    if identity.process_creation != key.creation {
        return None;
    }
    Some(VerifiedOwner {
        hwnd: key.hwnd,
        process,
        exe: identity.exe_path,
    })
}

/// Re-verify after the lookup: the HWND must still resolve to the key pid
/// and the held process must still be alive. Catches window close and HWND
/// reuse across processes mid-lookup.
fn verify_after(key: &WindowKey, owner: &VerifiedOwner) -> bool {
    ffi::window_pid(owner.hwnd) == key.pid && owner.process.is_alive()
}

/// Render one owned icon to worker pixels, or `None` on any draw failure
/// (a gray hit would lie about artwork presence).
fn render_icon(icon: &AppIcon) -> Option<IconImage> {
    let pixels = ffi::render_icon(icon, ICON_RENDER_PX, MINI_GRAY)?;
    let image = IconImage::new(pixels.size, pixels.size, pixels.bgra);
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
    if let Some(icon) = ffi::message_icon(owner.hwnd) {
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
    if let Some(icon) = ffi::class_icon(owner.hwnd) {
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
    // Executable fallback from the already-verified exe path: never presents
    // the UWP host exe as app artwork.
    if !is_frame_host(&owner.exe)
        && let Some(icon) = ffi::exe_icon(&owner.exe)
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

/// Shared worker state: the deduplicated channel plus its wake signal. The tile
/// loop only locks briefly (never waits); the worker waits while idle.
struct WorkerShared {
    channel: Mutex<IconChannel>,
    wake: Condvar,
    /// Owner completion hint: auto-reset event signaled after every pushed
    /// result so an event-driven owner drains promptly instead of stranding
    /// artwork behind an idle wait. `None` when event creation failed (the
    /// owner keeps draining on its other wakes). Best-effort, never waited
    /// on, never logged; bursts coalesce to one wake.
    completed: Option<OwnedHandle>,
}

fn signal_completed(shared: &WorkerShared) {
    if let Some(handle) = shared.completed.as_ref() {
        ffi::signal_event(handle);
    }
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
                // simply dropped here. Every finished lookup (hit or miss)
                // signals the owner completion hint so results never strand
                // behind an idle wait; the owner drains and filters by live
                // membership.
                lock_shared(&shared).push_result(result);
                signal_completed(&shared);
            }
            None => {
                // Spurious wake with an empty queue: loop back to the wait.
            }
        }
    }
}

/// Owner of the single background lookup worker and its deduplicated queues.
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
    /// host retries every five seconds and the pager stays icon-less).
    #[must_use]
    pub fn try_new() -> Option<Self> {
        let shared = Arc::new(WorkerShared {
            channel: Mutex::new(IconChannel::new()),
            wake: Condvar::new(),
            completed: ffi::create_auto_reset_event(),
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

    /// Queue lookup requests (deduped against queued and in-flight work) and
    /// wake the worker.
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

    /// Borrow the owner completion hint signaled after every finished
    /// lookup, if event creation succeeded. The owner includes it in its
    /// event-driven wait set alongside the input and render wakes.
    pub fn completion_handle(&self) -> Option<&OwnedHandle> {
        self.shared.completed.as_ref()
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
