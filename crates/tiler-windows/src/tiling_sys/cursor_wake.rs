#![forbid(unsafe_code)]
//! Event-driven cursor-motion wake for focus-follows-cursor.
//!
//! The owner wait drains thread messages (delivering hook callbacks) before
//! and after every wait, so an owner-thread `WH_MOUSE_LL` motion tap wakes
//! the loop on pointer motion with no timer and no poll. The tap is
//! observation-only (never consumes, never synthesizes) and ignores
//! injected-flagged motion; it is installed only while focus-follows-cursor
//! is armed and no fullscreen game holds the session. Suspend entry parks
//! it; a failed release retains the live handle and retries with backoff,
//! reported through bounded diagnostics. Motion bursts collapse to one wake
//! per turn.
//!
//! Cost: while installed, every system mouse message pays one extra
//! hook-chain link on the owner thread, and every motion burst runs one
//! full owner turn. Game input between game start and the next tick still
//! traverses the installed tap (detection interval, unmeasured); parking
//! is physical from suspend entry on. Install/uninstall failures are
//! reported and retried with backoff, never spun on.

use std::time::{Duration, Instant};

use super::TileLoop;

/// Install/uninstall function pair behind the tap. Production uses the real
/// low-level hook; tests inject scripted fns to exercise the full `Some`
/// handle lifecycle (install success/refusal, unhook success/failure)
/// without touching global hook state.
pub(super) struct HookFns {
    pub install: fn() -> Option<crate::ffi::MouseHook>,
    pub uninstall: fn(&mut crate::ffi::MouseHook) -> bool,
}

fn real_hook_fns() -> HookFns {
    HookFns {
        install: crate::ffi::install_mouse_hook,
        uninstall: crate::ffi::uninstall_mouse_hook,
    }
}

/// Retry backoff for failed install/uninstall attempts: eligible ticks
/// reconcile, but never faster than this (no per-move hammering).
const WAKE_RETRY: Duration = Duration::from_secs(5);

/// One bounded install/uninstall outcome for the owner trace.
pub(super) struct WakeDiag {
    pub op: &'static str,
    pub ok: bool,
}

/// FFC cursor-motion tap lifecycle for one owner run.
pub(super) struct CursorWake {
    hook: Option<crate::ffi::MouseHook>,
    fns: HookFns,
    armed: bool,
    suspended: bool,
    seen: u64,
    next_retry_at: Option<Instant>,
    last_ok: Option<bool>,
    pending: Option<WakeDiag>,
}

impl CursorWake {
    /// New tap: physically off. The first active tick arms it from live
    /// settings, after the suspend decision; startup under a game never
    /// installs it.
    pub(super) fn new() -> Self {
        Self {
            hook: None,
            fns: real_hook_fns(),
            armed: false,
            suspended: false,
            seen: crate::ffi::mouse_move_seq(),
            next_retry_at: None,
            last_ok: None,
            pending: None,
        }
    }

    /// Test seam: scripted install/uninstall without global hook state.
    /// The lifecycle (stored `Some` handle, retained failed release,
    /// backoff retry, drain) runs exactly as in production.
    #[cfg(test)]
    pub(super) fn with_fns(fns: HookFns) -> Self {
        let mut wake = Self::new();
        wake.fns = fns;
        wake
    }

    /// Whether the tap should be installed (armed and not suspended).
    /// The tick consults this before consuming a wake.
    pub(super) fn wants_hook(&self) -> bool {
        self.armed && !self.suspended
    }

    /// Whether the tap is currently installed.
    #[cfg(test)]
    pub(super) fn is_active(&self) -> bool {
        self.hook.is_some()
    }

    /// Focus-follows-cursor setting edge: reconcile immediately on change.
    /// No-op on steady state.
    pub(super) fn set_armed(&mut self, armed: bool, now: Instant) {
        if self.armed != armed {
            self.armed = armed;
            self.reconcile(now, true);
        }
    }

    /// Fullscreen-game suspension edge: reconcile immediately on change so
    /// a game never waits a tick for the park. A failed release retains
    /// the live handle and retries on eligible ticks.
    pub(super) fn set_suspended(&mut self, suspended: bool, now: Instant) {
        if self.suspended != suspended {
            self.suspended = suspended;
            self.reconcile(now, true);
        }
    }

    /// Eligible-tick reconcile: retry whatever the edges left undone (a
    /// refused install, a retained failed-release handle), at most once
    /// per backoff window. Call only on active ticks, after the suspend
    /// decision.
    pub(super) fn service(&mut self, now: Instant) {
        self.reconcile(now, false);
    }

    /// Graceful teardown: park now (called at loop exit; [`Drop`] is the
    /// backstop for every other exit path).
    pub(super) fn shutdown(&mut self, now: Instant) {
        self.armed = false;
        self.reconcile(now, true);
    }

    /// Coalesced motion wake: true once when genuine motion arrived since
    /// the last take, and only while the tap should be installed. The
    /// caller runs a normal full turn (same sampling as every owner wake);
    /// nothing here reads the cursor.
    pub(super) fn take_wake(&mut self) -> bool {
        if !self.wants_hook() {
            return false;
        }
        let seq = crate::ffi::mouse_move_seq();
        if seq != self.seen {
            self.seen = seq;
            true
        } else {
            false
        }
    }

    /// Drain one bounded diagnostic, if any. Failures report at most once
    /// per backoff window; success reports on first install and on
    /// recovery; steady success stays quiet.
    pub(super) fn take_diag(&mut self) -> Option<WakeDiag> {
        self.pending.take()
    }

    fn reconcile(&mut self, now: Instant, immediate: bool) {
        let due = immediate || self.next_retry_at.is_none_or(|at| now >= at);
        if self.wants_hook() {
            if self.hook.is_none()
                && due
                && let Some(hook) = self.try_install(now)
            {
                self.hook = Some(hook);
            }
        } else if self.hook.is_some() && due {
            let mut hook = self.hook.take().expect("hook guarded above");
            if (self.fns.uninstall)(&mut hook) {
                self.next_retry_at = None;
                self.note(true, "uninstall");
            } else {
                // Failed release retains the live handle (never drop a
                // handle the OS still owns): retry on a later eligible
                // tick, and drain parked motion meanwhile.
                self.hook = Some(hook);
                self.next_retry_at = Some(now + WAKE_RETRY);
                self.note(false, "uninstall");
            }
            self.seen = crate::ffi::mouse_move_seq();
        }
    }

    /// One install attempt: `None` on refusal (backoff + report) or when
    /// no attempt is due.
    fn try_install(&mut self, now: Instant) -> Option<crate::ffi::MouseHook> {
        match (self.fns.install)() {
            Some(hook) => {
                self.next_retry_at = None;
                self.note(true, "install");
                // Motion from the uninstalled window is stale.
                self.seen = crate::ffi::mouse_move_seq();
                Some(hook)
            }
            None => {
                self.next_retry_at = Some(now + WAKE_RETRY);
                self.note(false, "install");
                None
            }
        }
    }

    fn note(&mut self, ok: bool, op: &'static str) {
        // Bounded: first outcome, every failure episode, and recovery.
        // Steady success stays quiet.
        if self.last_ok != Some(ok) || !ok {
            self.pending = Some(WakeDiag { op, ok });
        }
        self.last_ok = Some(ok);
    }
}

impl Drop for CursorWake {
    /// Final backstop for every owner exit path (normal, error, bounded
    /// run): release the tap if still held. Best-effort by construction:
    /// process exit releases the hook regardless, and teardown already
    /// parks deterministically on the graceful path.
    fn drop(&mut self) {
        if let Some(mut hook) = self.hook.take() {
            let _ = (self.fns.uninstall)(&mut hook);
        }
    }
}

impl TileLoop {
    /// Whether the cursor-motion tap is currently installed (tests).
    #[cfg(test)]
    pub(super) fn cursor_wake_active(&self) -> bool {
        self.cursor_wake.is_active()
    }
}

#[cfg(test)]
mod tests {
    use super::super::rmax03_adapter_tests::test_state;
    use std::sync::atomic::{AtomicBool, Ordering};

    const MOVE: u32 = 0x0200;

    static UNINSTALLED: AtomicBool = AtomicBool::new(false);

    fn some_hook() -> Option<crate::ffi::MouseHook> {
        Some(crate::ffi::test_hook())
    }

    fn no_hook() -> Option<crate::ffi::MouseHook> {
        None
    }

    fn ok_unhook(_: &mut crate::ffi::MouseHook) -> bool {
        true
    }

    fn fail_unhook(_: &mut crate::ffi::MouseHook) -> bool {
        false
    }

    fn record_unhook(_: &mut crate::ffi::MouseHook) -> bool {
        UNINSTALLED.store(true, Ordering::SeqCst);
        true
    }

    fn fns(
        install: fn() -> Option<crate::ffi::MouseHook>,
        uninstall: fn(&mut crate::ffi::MouseHook) -> bool,
    ) -> super::HookFns {
        super::HookFns { install, uninstall }
    }

    fn wake_with(
        state: &mut super::super::TileLoop,
        install: fn() -> Option<crate::ffi::MouseHook>,
        uninstall: fn(&mut crate::ffi::MouseHook) -> bool,
    ) {
        state.cursor_wake = super::CursorWake::with_fns(fns(install, uninstall));
    }

    #[test]
    fn wake_coalesces_one_burst_to_one_turn() {
        let _guard = crate::ffi::MOUSE_TEST_LOCK.lock().expect("test lock");
        let mut state = test_state();
        wake_with(&mut state, some_hook, ok_unhook);
        assert!(!state.cursor_wake.wants_hook());
        // Unarmed: motion never wakes.
        crate::ffi::drive_mouse_move(MOVE, 1, 1, 0);
        assert!(!state.cursor_wake.take_wake());
        // Armed: the stub installs for real lifecycle coverage, one wake
        // per burst, bursts collapse.
        let now = std::time::Instant::now();
        state.cursor_wake.set_armed(true, now);
        assert!(state.cursor_wake.wants_hook());
        assert!(state.cursor_wake_active());
        crate::ffi::drive_mouse_move(MOVE, 2, 2, 0);
        crate::ffi::drive_mouse_move(MOVE, 3, 3, 0);
        assert!(state.cursor_wake.take_wake());
        assert!(!state.cursor_wake.take_wake());
    }

    #[test]
    fn suspend_parks_the_tap_and_drains_game_motion() {
        let _guard = crate::ffi::MOUSE_TEST_LOCK.lock().expect("test lock");
        let mut state = test_state();
        wake_with(&mut state, some_hook, ok_unhook);
        let now = std::time::Instant::now();
        state.cursor_wake.set_armed(true, now);
        state.cursor_wake.set_suspended(true, now);
        assert!(!state.cursor_wake.wants_hook());
        assert!(!state.cursor_wake_active());
        // Game motion while suspended never wakes, including the resume.
        crate::ffi::drive_mouse_move(MOVE, 4, 4, 0);
        assert!(!state.cursor_wake.take_wake());
        state.cursor_wake.set_suspended(false, now);
        assert!(state.cursor_wake.wants_hook());
        assert!(!state.cursor_wake.take_wake());
        // Fresh motion after resume wakes again.
        crate::ffi::drive_mouse_move(MOVE, 5, 5, 0);
        assert!(state.cursor_wake.take_wake());
    }

    #[test]
    fn failed_unhook_retains_and_retries_with_backoff() {
        let _guard = crate::ffi::MOUSE_TEST_LOCK.lock().expect("test lock");
        let mut state = test_state();
        wake_with(&mut state, some_hook, fail_unhook);
        let now = std::time::Instant::now();
        state.cursor_wake.set_armed(true, now);
        assert!(state.cursor_wake_active());
        // Failed release retains the live handle (never dropped) and
        // reports boundedly.
        state.cursor_wake.set_suspended(true, now);
        assert!(!state.cursor_wake.wants_hook());
        assert!(state.cursor_wake_active());
        let diag = state.cursor_wake.take_diag().expect("failure reports");
        assert_eq!((diag.op, diag.ok), ("uninstall", false));
        // Eligible ticks retry, at most once per backoff window.
        state.cursor_wake.service(now);
        assert!(state.cursor_wake_active());
        assert!(state.cursor_wake.take_diag().is_none());
        state.cursor_wake.service(now + super::WAKE_RETRY);
        assert!(state.cursor_wake_active());
        let diag = state.cursor_wake.take_diag().expect("retry reports");
        assert_eq!((diag.op, diag.ok), ("uninstall", false));
        // Resume keeps the retained handle live without reinstall spam.
        state.cursor_wake.set_suspended(false, now);
        assert!(state.cursor_wake.wants_hook());
        assert!(state.cursor_wake_active());
        assert!(state.cursor_wake.take_diag().is_none());
    }

    #[test]
    fn refused_install_retries_with_backoff() {
        let _guard = crate::ffi::MOUSE_TEST_LOCK.lock().expect("test lock");
        let mut state = test_state();
        wake_with(&mut state, no_hook, ok_unhook);
        let now = std::time::Instant::now();
        state.cursor_wake.set_armed(true, now);
        assert!(state.cursor_wake.wants_hook());
        assert!(!state.cursor_wake_active());
        let diag = state.cursor_wake.take_diag().expect("refusal reports");
        assert_eq!((diag.op, diag.ok), ("install", false));
        // No per-tick hammering: only the backoff window retries.
        state.cursor_wake.service(now);
        assert!(!state.cursor_wake_active());
        assert!(state.cursor_wake.take_diag().is_none());
        state.cursor_wake.service(now + super::WAKE_RETRY);
        assert!(!state.cursor_wake_active());
        let diag = state.cursor_wake.take_diag().expect("retry reports");
        assert_eq!((diag.op, diag.ok), ("install", false));
    }

    #[test]
    fn injected_flagged_motion_never_wakes() {
        let _guard = crate::ffi::MOUSE_TEST_LOCK.lock().expect("test lock");
        let mut state = test_state();
        wake_with(&mut state, some_hook, ok_unhook);
        let now = std::time::Instant::now();
        state.cursor_wake.set_armed(true, now);
        crate::ffi::drive_mouse_move(MOVE, 6, 6, 1);
        assert!(!state.cursor_wake.take_wake());
    }

    #[test]
    fn shutdown_parks_the_tap() {
        let _guard = crate::ffi::MOUSE_TEST_LOCK.lock().expect("test lock");
        let mut state = test_state();
        wake_with(&mut state, some_hook, ok_unhook);
        assert!(!state.cursor_wake_active());
        let now = std::time::Instant::now();
        state.cursor_wake.set_armed(true, now);
        assert!(state.cursor_wake_active());
        state.cursor_wake.shutdown(now);
        assert!(!state.cursor_wake.wants_hook());
        assert!(!state.cursor_wake_active());
        crate::ffi::drive_mouse_move(MOVE, 7, 7, 0);
        assert!(!state.cursor_wake.take_wake());
    }

    #[test]
    fn drop_releases_the_tap() {
        let _guard = crate::ffi::MOUSE_TEST_LOCK.lock().expect("test lock");
        UNINSTALLED.store(false, Ordering::SeqCst);
        let now = std::time::Instant::now();
        {
            let mut wake = super::CursorWake::with_fns(fns(some_hook, record_unhook));
            wake.set_armed(true, now);
        }
        assert!(UNINSTALLED.load(Ordering::SeqCst));
    }
}
