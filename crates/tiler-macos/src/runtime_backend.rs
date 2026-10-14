#![forbid(unsafe_code)]

//! Phase 1 backend traits and the native macOS adapter.
//!
//! Thread-affinity rule: factories are `Send + Sync` and shared, but the
//! backends they create are used only inside their owning thread. [`AxBackend`]
//! deliberately has no `Send` bound, so a native backend holding the
//! `!Send` [`crate::ffi::AxElement`] (and the `!Send`
//! [`crate::ffi::Phase1Observer`]) needs no `unsafe impl`. Creation,
//! driving, and destruction all happen on the owner (`app_worker_main` for
//! AX, `input_main` for input; see [`crate::runtime`]).
//!
//! Honesty rule: [`AxCompletion`] reports dispatch truth only (accepted,
//! refused, timed out). It never substitutes observed truth, which arrives
//! exclusively through [`crate::runtime::AxObservation`]. Backends must not
//! place titles, keystrokes, or other user content in reasons.
//!
//! Observation seam (precise):
//!
//! - `snapshot()` returns `Result`: a failed read is an error (preserving
//!   retained engine state), never an empty window list masquerading as
//!   truth. Partial failures name requested vs observed vs failed counts in
//!   the cause; the worker logs the failure and keeps the last good state.
//! - `poll_observation()` is the autonomous path, with a default of `None`
//!   (no autonomous observations: manually driven offline tests keep working
//!   without claiming streaming). The native backend overrides it with the
//!   thread-affine [`crate::ffi::Phase1Observer`]: the notification callback
//!   does no layout/log/AX work (atomic bump plus non-blocking wake only);
//!   the owner thread pumps a bounded runloop slice and re-reads. Portable
//!   fakes override it with a wake-driven channel (`recv_timeout`: wakes
//!   immediately on send, the bound only caps stop latency).
//! - Sequence and generation are stamped by the owning worker at publish
//!   time, never by the backend, so a backend cannot forge a lifetime.
//!
//! Native seams (precise):
//!
//! - Actuation and polling snapshots use the existing safe [`crate::ffi`]
//!   wrappers (`AxElement::application`, `windows`, `ax_set_messaging_timeout`,
//!   `ax_set_frame`/`ax_get_frame`, `ax_window_id`) with the shared finite
//!   [`crate::runtime::AX_CALL_TIMEOUT`]. The timeout is checked BEFORE each
//!   AX join/read/write; a setup refusal aborts the narrow operation with a
//!   timeout outcome instead of running unbounded. No new live launch CLI was
//!   added.
//! - Notification streaming is wired per-app through `Phase1Observer`
//!   (app-level session notifications plus per-window session notifications,
//!   re-synced when newly observed windows appear). Command wake uses a
//!   bounded runloop slice: there is deliberately NO cross-thread runloop
//!   wake handle, because the maintained `objc2-core-foundation` runloop
//!   types are not `Send` and this crate forbids custom `unsafe impl
//!   Send/Sync` (the rift/glide `WakeupHandle` pattern needs exactly such an
//!   impl). Worst-case extra latency on either direction is one
//!   [`NATIVE_PUMP_SLICE`]; correctness never depends on it.
//! - Native tap installation is NOT performed here: tap activation stays an
//!   explicit separate live acceptance. [`NativeInputStub`] parks honestly
//!   (see [`native_tap_status`]); installing a tap is never needed to test
//!   the offline skeleton.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::runtime::{AX_CALL_TIMEOUT, AxCompletion, FrameCommand, ObservedWindow};

pub use crate::runtime_observe::BackendSnapshot;

/// Bound for one native observer pump slice. Wake-driven delivery still
/// applies (notifications interrupt the slice); the bound only caps command/
/// stop latency in the absence of a cross-thread wake primitive (see module
/// docs: no `unsafe impl Send/Sync` for a wake handle).
#[cfg(target_os = "macos")]
const NATIVE_PUMP_SLICE: std::time::Duration = std::time::Duration::from_millis(20);

/// Raw key event from an input source. `flags` carries the raw OS modifier
/// bits (`CGEventFlags` space; see [`crate::input::cg_flag_bits`]) so the
/// input owner can apply the existing exact-mask compare
/// ([`crate::input_match::TapTuning`] /
/// [`crate::input_match::decide_tap_event`]): extra modifiers must fail the
/// match, which a single [`ProbeModifier`] cannot represent. Matching (exact
/// chord, key down only) happens in the input owner, never in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKeyEvent {
    pub kind: TapKeyKind,
    pub flags: u64,
    pub key_code: u32,
}

use crate::input_match::TapKeyKind;

/// Per-app AX backend. Created, driven, and destroyed inside the owning app
/// worker thread. No `Send` bound: native state stays thread-affine without
/// `unsafe impl`.
pub trait AxBackend {
    /// Apply one desired frame with a finite messaging timeout. Returns
    /// dispatch truth only, never verified state.
    fn apply_frame(&mut self, cmd: &FrameCommand) -> AxCompletion;
    /// Poll the current window list with honest per-call bounds. `Ok` is a
    /// complete observation; `Err` names the cause (with requested vs
    /// observed vs failed counts for partial reads) and must preserve
    /// retained engine state — never an empty list standing in for failure.
    fn snapshot(&mut self) -> Result<Vec<ObservedWindow>, String>;
    /// Autonomous observation, if the backend supports it. `None` means no
    /// new observation is available right now (including the default: no
    /// autonomous source at all). `Some(Ok(_))` is a fresh payload to
    /// publish; `Some(Err(_))` is an observation failure that preserves
    /// retained state. Never blocks beyond a bounded slice; `stop` bounds
    /// fake backends the same way the native slice bounds the runloop pump.
    fn poll_observation(&mut self, stop: &AtomicBool) -> Option<Result<BackendSnapshot, String>> {
        let _ = stop;
        None
    }
    /// Tear down thread-affine state (observer unsubscribe, source
    /// invalidation). The worker logs the outcome; `Err` never becomes a
    /// silent clean. Default: nothing to tear down.
    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// Factory for per-app AX backends. Shared across threads; `create` is
/// always invoked inside the owning worker thread.
pub trait AxBackendFactory: Send + Sync {
    fn create(&self, pid: u32) -> Result<Box<dyn AxBackend>, String>;
}

/// Input event source. Created, driven, and destroyed inside the dedicated
/// input thread. `next` blocks until an event arrives or `stop` is set, and
/// must return promptly (within milliseconds) once `stop` is set. EOS
/// contract: `None` is unexpected end-of-stream (disconnect/exhaustion), not
/// an idle tick. The input owner treats a `None` without `stop` as an honest
/// unexpected exit (counted, logged, thread exits) and never busy-spins on
/// it.
pub trait InputSource {
    fn next(&mut self, stop: &AtomicBool) -> Option<RawKeyEvent>;
}

/// Factory for the input source. Shared; `create` runs inside the input
/// owner thread.
pub trait InputSourceFactory: Send + Sync {
    fn create(&self) -> Result<Box<dyn InputSource>, String>;
    fn describe(&self) -> &'static str;
}

/// Honest status of native tap installation in the Phase 1 skeleton.
#[must_use]
pub const fn native_tap_status() -> &'static str {
    "not-installed: native tap activation is explicit separate live acceptance; \
     install is not needed to test the offline skeleton"
}

/// Native input stub: installs nothing, intercepts nothing. Parks until
/// stop so the input owner stays wake-driven without a tap.
pub struct NativeInputStub;

impl InputSourceFactory for NativeInputStub {
    fn create(&self) -> Result<Box<dyn InputSource>, String> {
        Ok(Box::new(NativeInputSource))
    }

    fn describe(&self) -> &'static str {
        native_tap_status()
    }
}

struct NativeInputSource;

impl InputSource for NativeInputSource {
    fn next(&mut self, stop: &AtomicBool) -> Option<RawKeyEvent> {
        // No tap installed: nothing to block on. True park (no timeout
        // polling): stop/shutdown wakes this thread via `unpark`, and an
        // `unpark` before `park` grants a permit, so no wake is lost. The
        // stop flag re-check covers spurious wakeups and races the permit.
        while !stop.load(Ordering::SeqCst) {
            std::thread::park();
        }
        None
    }
}

/// Native per-app AX backend over the existing safe FFI wrappers.
/// Constructed with plain data only; every `AxElement` is created, used,
/// and dropped inside the owning worker thread on each call. The observer
/// guard (when created, lazily on first `poll_observation`) is likewise
/// owned here and torn down here.
#[cfg(target_os = "macos")]
pub struct NativeAxFactory;

#[cfg(target_os = "macos")]
impl AxBackendFactory for NativeAxFactory {
    fn create(&self, pid: u32) -> Result<Box<dyn AxBackend>, String> {
        // Validate before any FFI: reserved PIDs are refused, and anything
        // above `i32::MAX` would truncate on the `as i32` cast at the AX
        // boundary (a wrong app observed — or a refusal misattributed).
        if pid <= 1 || pid > i32::MAX as u32 {
            return Err(format!("refusing out-of-range pid {pid}"));
        }
        Ok(Box::new(NativeAxBackend {
            pid,
            observer: None,
        }))
    }
}

#[cfg(target_os = "macos")]
struct NativeAxBackend {
    pid: u32,
    observer: Option<crate::ffi::Phase1Observer>,
}

#[cfg(target_os = "macos")]
fn messaging_secs() -> f32 {
    AX_CALL_TIMEOUT.as_secs_f32().max(0.5)
}

#[cfg(target_os = "macos")]
impl AxBackend for NativeAxBackend {
    fn apply_frame(&mut self, cmd: &FrameCommand) -> AxCompletion {
        use crate::ffi::{AxElement, ax_set_frame, ax_set_messaging_timeout, ax_window_id};
        use std::time::Instant;
        let start = Instant::now();
        if cmd.pid != self.pid {
            return AxCompletion::Failed {
                reason: "pid mismatch (stale command)".to_owned(),
            };
        }
        let pid_i32 = self.pid as i32;
        let Some(app) = AxElement::application(pid_i32) else {
            return AxCompletion::Failed {
                reason: "no AX element for pid".to_owned(),
            };
        };
        // Finite timeout BEFORE any join/read/write; refusal aborts.
        if ax_set_messaging_timeout(&app, messaging_secs()) != 0 {
            return AxCompletion::Timeout {
                timeout_ms: AX_CALL_TIMEOUT.as_millis().min(u128::from(u64::MAX)) as u64,
            };
        }
        let windows = match app.windows() {
            Ok(windows) => windows,
            Err(code) => {
                return AxCompletion::Failed {
                    reason: format!("windows: {}", crate::ffi::ax_error_name(code)),
                };
            }
        };
        // Identity join per candidate with its own finite bound: a candidate
        // whose timeout refuses is skipped (narrow abort), never read
        // unbounded. When every candidate refuses, the operation times out.
        let mut refused_joins = 0usize;
        let mut joined: Option<&AxElement> = None;
        for window in windows.iter() {
            if ax_set_messaging_timeout(window, messaging_secs()) != 0 {
                refused_joins += 1;
                continue;
            }
            if ax_window_id(window) == Some(cmd.wid) {
                joined = Some(window);
                break;
            }
        }
        let Some(window) = joined else {
            if refused_joins > 0 && refused_joins == windows.len() {
                return AxCompletion::Timeout {
                    timeout_ms: AX_CALL_TIMEOUT.as_millis().min(u128::from(u64::MAX)) as u64,
                };
            }
            return AxCompletion::Failed {
                reason: "wid not joined to owned pid".to_owned(),
            };
        };
        if ax_set_messaging_timeout(window, messaging_secs()) != 0 {
            return AxCompletion::Timeout {
                timeout_ms: AX_CALL_TIMEOUT.as_millis().min(u128::from(u64::MAX)) as u64,
            };
        }
        let frame = crate::ffi::AxFrame {
            x: f64::from(cmd.x),
            y: f64::from(cmd.y),
            w: f64::from(cmd.w),
            h: f64::from(cmd.h),
        };
        if !frame.valid_for_set() {
            return AxCompletion::Failed {
                reason: "degenerate frame".to_owned(),
            };
        }
        match ax_set_frame(window, &frame) {
            Ok(()) => AxCompletion::Applied {
                elapsed_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            },
            Err(code) => AxCompletion::Failed {
                reason: format!("set: {}", crate::ffi::ax_error_name(code)),
            },
        }
    }

    fn snapshot(&mut self) -> Result<Vec<ObservedWindow>, String> {
        snapshot_windows(self.pid)
    }

    fn poll_observation(&mut self, stop: &AtomicBool) -> Option<Result<BackendSnapshot, String>> {
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        // Lazy observer creation inside the owner thread (thread-affine).
        // The factory already validated `pid <= i32::MAX`, so this cast is
        // exact.
        if self.observer.is_none() {
            match crate::ffi::Phase1Observer::create(self.pid as i32, messaging_secs()) {
                Ok(observer) => {
                    self.observer = Some(observer);
                    // Initial sync regardless of notification: preexisting
                    // startup windows need move/resize subscriptions now, not
                    // on the next app-level event (which may never come for
                    // a static window set). A sync refusal is reported
                    // honestly — a failed subscription is never assumed
                    // success — and publishes nothing, preserving retained
                    // geometry for the next pump.
                    return Some(self.read_observed());
                }
                Err(reason) => return Some(Err(reason)),
            }
        }
        let observer = self.observer.as_mut().expect("created above");
        observer.pump(NATIVE_PUMP_SLICE);
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        if observer.take_notified() == 0 {
            return None;
        }
        Some(self.read_observed())
    }

    fn shutdown(&mut self) -> Result<(), String> {
        if let Some(observer) = self.observer.take() {
            observer.shutdown()
        } else {
            Ok(())
        }
    }
}

/// Snapshot plus subscription sync plus focused read for one poll: the full
/// observed truth for the owning worker to stage. Any leg failing reports
/// the error (retained geometry preserved); a failed window subscription is
/// never silently assumed success.
#[cfg(target_os = "macos")]
impl NativeAxBackend {
    fn read_observed(&mut self) -> Result<BackendSnapshot, String> {
        let windows = snapshot_windows(self.pid)?;
        let wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
        let observer = self
            .observer
            .as_mut()
            .expect("observer created before read");
        observer
            .sync_windows(&wids, messaging_secs())
            .map_err(|reason| format!("subscription sync pid {}: {reason}", self.pid))?;
        let focused_wid = observer.focused_wid(messaging_secs());
        Ok(BackendSnapshot {
            windows,
            focused_wid,
        })
    }
}

/// Complete window-list read for one pid: finite timeout before every join
/// and read; any failure is `Err` (requested vs observed vs failed counts
/// named), never a silent empty list.
#[cfg(target_os = "macos")]
fn snapshot_windows(pid: u32) -> Result<Vec<ObservedWindow>, String> {
    use crate::ffi::{AxElement, ax_get_frame, ax_set_messaging_timeout, ax_window_id};
    let Some(app) = AxElement::application(pid as i32) else {
        return Err(format!("snapshot pid {pid}: no AX element for pid"));
    };
    if ax_set_messaging_timeout(&app, messaging_secs()) != 0 {
        return Err(format!(
            "snapshot pid {pid}: messaging timeout unset ({}ms)",
            AX_CALL_TIMEOUT.as_millis()
        ));
    }
    let windows = match app.windows() {
        Ok(windows) => windows,
        Err(code) => {
            return Err(format!(
                "snapshot pid {pid}: windows: {}",
                crate::ffi::ax_error_name(code)
            ));
        }
    };
    let requested = windows.len();
    let mut out = Vec::new();
    let mut failed = 0usize;
    let mut first_cause = String::new();
    for window in windows.iter() {
        if ax_set_messaging_timeout(window, messaging_secs()) != 0 {
            failed += 1;
            if first_cause.is_empty() {
                first_cause = "messaging timeout unset".to_owned();
            }
            continue;
        }
        let Some(wid) = ax_window_id(window) else {
            failed += 1;
            if first_cause.is_empty() {
                first_cause = "window identity join failed".to_owned();
            }
            continue;
        };
        match ax_get_frame(window) {
            Ok(frame) => out.push(ObservedWindow {
                wid,
                x: frame.x as i32,
                y: frame.y as i32,
                w: frame.w as i32,
                h: frame.h as i32,
                floating: false,
            }),
            Err(code) => {
                failed += 1;
                if first_cause.is_empty() {
                    first_cause = crate::ffi::ax_error_name(code);
                }
            }
        }
        if out.len() >= crate::config::MAX_WINDOWS {
            break;
        }
    }
    if failed > 0 {
        return Err(format!(
            "snapshot pid {pid}: requested {requested} observed {} failed {failed}: {first_cause}",
            out.len()
        ));
    }
    Ok(out)
}

/// Off-macOS stub: AX backends are macOS-only. Offline fakes implement the
/// traits for portable tests; native tap activation stays separate.
#[cfg(not(target_os = "macos"))]
pub struct NativeAxFactory;

#[cfg(not(target_os = "macos"))]
impl AxBackendFactory for NativeAxFactory {
    fn create(&self, _pid: u32) -> Result<Box<dyn AxBackend>, String> {
        Err("AX backends are macOS-only; use offline fakes".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_factory_rejects_reserved_and_unrepresentable_pids() {
        // Offline validation before any FFI: reserved PIDs plus anything
        // above `i32::MAX` (which would truncate on the `as i32` cast at
        // the AX boundary). No live AX runs here — refusal happens first.
        let factory = NativeAxFactory;
        assert!(factory.create(0).is_err());
        assert!(factory.create(1).is_err());
        assert!(factory.create(u32::MAX).is_err());
        assert!(factory.create(i32::MAX as u32 + 1).is_err());
        #[cfg(target_os = "macos")]
        assert!(factory.create(1001).is_ok());
    }

    #[test]
    fn input_stub_parks_until_stop_without_polling() {
        // A pre-signalled stop must fall through immediately: the stub parks
        // (permit-based), it never quantizes shutdown to a poll interval.
        let stop = AtomicBool::new(true);
        let mut source = NativeInputSource;
        let start = std::time::Instant::now();
        assert_eq!(source.next(&stop), None);
        assert!(
            start.elapsed() < std::time::Duration::from_millis(50),
            "pre-stopped stub must not sleep: {:?}",
            start.elapsed()
        );
    }
}
