#![forbid(unsafe_code)]
use super::{InputChannels, install_backoff_secs, snap_batch_age_us};
use crate::input_handoff::{
    InboxItem, InputDiag, InputSnapshot, OUTBOX_BATCH_CAP, PumpLifecycle, SharedSnapshot,
    StopReport, TaggedBatch, note_policy_applied, snapshot_policy_eq, take_delivery_gap_ms,
};
use crate::snapkey::{KeyboardConfig, QueuedSnapEvent};
use crate::win_mouse::WinDragEdge;
use std::collections::VecDeque;
use std::os::windows::io::OwnedHandle;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc::{SyncSender, sync_channel},
};
use std::time::{Duration, Instant};

/// Native hook backend seam: real `sys` calls in production, scripted
/// fakes in tests. The same [`PumpCore`] step drives both, so install /
/// retry / stop behavior under test is the production logic, not a toy.
/// Implementations must never perform native effects in tests: fake
/// backends record calls and return scripted results with no hooks.
pub trait NativeHooks {
    /// Installed keyboard hook token (real ffi owner, fake unit).
    type KeyboardHook;
    /// Installed mouse hook token.
    type MouseHook;
    /// Install the keyboard hook with the latest policy (`Err` = retry).
    fn install_keyboard(&mut self, config: KeyboardConfig) -> Result<Self::KeyboardHook, String>;
    /// Install the mouse hook (`Err` = retry).
    fn install_mouse(&mut self) -> Result<Self::MouseHook, String>;
    /// Release the keyboard hook on the creator thread.
    fn uninstall_keyboard(&mut self, hook: &mut Self::KeyboardHook) -> bool;
    /// Release the mouse hook on the creator thread.
    fn uninstall_mouse(&mut self, hook: &mut Self::MouseHook) -> bool;
    /// Deliver pending messages (LL callbacks) on the creator thread
    /// without waiting. Callbacks queue classifier work that the core
    /// transfers before blocking, so the final callback never strands
    /// behind an idle wait.
    fn drain_messages(&mut self);
    /// Wait for owner work, queued hook-callback messages, or the
    /// deadline (`u32::MAX` while fully idle). The owner work wake and
    /// the message queue break the wait early; `None` keeps the legacy
    /// queue-only wait. Never transfers: the core drains and offers
    /// before and after. Returns the native wait reason (one per wait).
    fn wait_messages(
        &mut self,
        timeout_ms: u32,
        work_wake: Option<&OwnedHandle>,
    ) -> super::InputWaitReason;
}

/// Production backend: real hook install/uninstall plus the creator-side
/// message pump. Both hooks stay together on this thread.
pub struct RealHooks;
impl NativeHooks for RealHooks {
    type KeyboardHook = crate::ffi::SnapKeyHook;
    type MouseHook = crate::ffi::WinMouseHook;

    fn install_keyboard(&mut self, config: KeyboardConfig) -> Result<Self::KeyboardHook, String> {
        crate::snapkey::sys::install(config)
    }

    fn install_mouse(&mut self) -> Result<Self::MouseHook, String> {
        crate::win_mouse::sys::install()
    }

    fn uninstall_keyboard(&mut self, hook: &mut Self::KeyboardHook) -> bool {
        crate::snapkey::sys::uninstall(hook)
    }

    fn uninstall_mouse(&mut self, hook: &mut Self::MouseHook) -> bool {
        crate::win_mouse::sys::uninstall(hook)
    }

    fn drain_messages(&mut self) {
        drain_messages();
    }

    fn wait_messages(
        &mut self,
        timeout_ms: u32,
        work_wake: Option<&OwnedHandle>,
    ) -> super::InputWaitReason {
        wait_messages(timeout_ms, work_wake)
    }
}

impl super::FakeBackend {
    fn next_scripted(
        script: &mut VecDeque<bool>,
        attempts: &mut u32,
        what: &'static str,
    ) -> Result<(), String> {
        *attempts += 1;
        match script.pop_front() {
            Some(true) => Ok(()),
            Some(false) => Err(format!("error: fake {what} install failed")),
            None => Err(format!("error: fake {what} script exhausted")),
        }
    }
}

impl NativeHooks for super::FakeBackend {
    type KeyboardHook = ();
    type MouseHook = ();

    fn install_keyboard(&mut self, config: KeyboardConfig) -> Result<Self::KeyboardHook, String> {
        self.keyboard_configs.push(config);
        // Mirror production TLS init (empty origins/gate, machine from
        // the attempted config); cleared again below on failure, exactly
        // like the real install. Test-only: no native effects.
        #[cfg(test)]
        crate::snapkey::sys::test_ensure_state(config);
        let outcome = Self::next_scripted(
            &mut self.keyboard_script,
            &mut self.keyboard_attempts,
            "keyboard",
        );
        #[cfg(test)]
        if outcome.is_err() {
            crate::snapkey::sys::test_clear_state();
        }
        outcome
    }

    fn install_mouse(&mut self) -> Result<Self::MouseHook, String> {
        // Mirror production TLS init; cleared again below on failure.
        // Test-only: no native effects.
        #[cfg(test)]
        crate::win_mouse::sys::test_ensure_state();
        let outcome =
            Self::next_scripted(&mut self.mouse_script, &mut self.mouse_attempts, "mouse");
        #[cfg(test)]
        if outcome.is_err() {
            crate::win_mouse::sys::test_clear_state();
        }
        outcome
    }

    fn uninstall_keyboard(&mut self, _hook: &mut Self::KeyboardHook) -> bool {
        self.unhook_script.pop_front().unwrap_or(true)
    }

    fn uninstall_mouse(&mut self, _hook: &mut Self::MouseHook) -> bool {
        self.unhook_script.pop_front().unwrap_or(true)
    }

    fn drain_messages(&mut self) {
        self.pump_turns += 1;
        #[cfg(test)]
        {
            for event in std::mem::take(&mut self.snap_on_drain) {
                crate::snapkey::sys::test_push_queued(event);
            }
            for edge in std::mem::take(&mut self.windrag_on_drain) {
                crate::win_mouse::sys::test_push_edge(edge);
            }
        }
    }

    fn wait_messages(
        &mut self,
        timeout_ms: u32,
        _work_wake: Option<&OwnedHandle>,
    ) -> super::InputWaitReason {
        self.pump_timeouts.push(timeout_ms);
        // S1 seam assertion (test builds only): classifier work queued
        // by callbacks during the pre-wait drain must already be
        // transferred before blocking. Drains at most one probe batch,
        // so a violation fails loudly instead of stranding silently.
        #[cfg(test)]
        if self.assert_transferred_before_wait {
            let snap_left =
                crate::snapkey::sys::drain_up_to(crate::input_handoff::OUTBOX_BATCH_CAP);
            let mouse_left =
                crate::win_mouse::sys::drain_up_to(crate::input_handoff::OUTBOX_BATCH_CAP);
            assert!(
                snap_left.is_empty() && mouse_left.is_empty(),
                "pre-wait drain queued work that never transferred before blocking"
            );
        }
        self.wait_script
            .pop_front()
            .unwrap_or(super::InputWaitReason::Timeout)
    }
}

/// Install/retry lifecycle event for the owner writer. Bounded: the pump
/// offers through a capacity-16 slot and drops (counted) under owner
/// stall; readiness and stop completion ride dedicated lossless slots.
/// The pump never logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailEvent {
    /// `"keyboard"` or `"mouse"`.
    pub hook: &'static str,
    /// Consecutive attempt number.
    pub attempt: u32,
    /// Backoff seconds until the next attempt.
    pub retry_secs: u64,
    /// True when a previous failure recovered.
    pub recovered: bool,
}

/// Bounded pump -> owner signals. `ready` and `stopped` are lossless
/// single slots; `failures` is a bounded FIFO (drops counted, never
/// blocking the pump).
pub struct PumpSignals {
    ready: AtomicBool,
    stopped: std::sync::Mutex<Option<bool>>,
    failures: SyncSender<FailEvent>,
    /// Failure events dropped under owner stall (bounded evidence).
    pub events_dropped: AtomicU32,
}

impl PumpSignals {
    /// New signals with an empty bounded failure FIFO.
    #[must_use]
    pub fn new(failures: SyncSender<FailEvent>) -> Self {
        Self {
            ready: AtomicBool::new(false),
            stopped: std::sync::Mutex::new(None),
            failures,
            events_dropped: AtomicU32::new(0),
        }
    }

    /// Note readiness (idempotent, lossless).
    pub fn note_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    /// Readiness snapshot.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Record one install/retry event without blocking; counts drops.
    pub fn note_failure(&self, event: FailEvent) {
        if self.failures.try_send(event).is_err() {
            self.events_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record stop completion (terminal, lossless).
    pub fn note_stopped(&self, unhook_ok: bool) {
        *self.stopped.lock().expect("stopped lock") = Some(unhook_ok);
    }

    /// Stop completion snapshot.
    #[must_use]
    pub fn stopped(&self) -> Option<bool> {
        *self.stopped.lock().expect("stopped lock")
    }
}

/// Owner handle for exactly one input pump thread. All logging stays on
/// the owner: the pump reports readiness, bounded [`FailEvent`]s,
/// [`InputDiag`] samples and generation acks for the owner writer.
pub struct InputHandle {
    channels: Arc<InputChannels>,
    signals: Arc<PumpSignals>,
    failures: std::sync::mpsc::Receiver<FailEvent>,
    lifecycle: PumpLifecycle,
    thread: Option<std::thread::JoinHandle<StopReport>>,
    /// Completed stop report: a second `wait_stopped` returns the
    /// stored report instead of joining a consumed handle.
    stop_report: Option<StopReport>,
    keyboard_config: KeyboardConfig,
}

impl InputHandle {
    /// Spawn the dedicated input thread. `Err` on thread-spawn failure
    /// (the owner logs and continues degraded with no hooks) or while
    /// another pump worker holds the process slot (a hung or unclean
    /// stop, including a dropped handle, keeps the reserve so no
    /// overlapping worker can start; recovery is process exit); otherwise
    /// the worker starts in `Starting` and the owner must
    /// [`InputHandle::wait_ready`] with a bounded wait. The slot is
    /// never released on `Drop`: only a clean stop or a spawn failure
    /// frees it.
    pub fn spawn(keyboard_config: KeyboardConfig) -> Result<Self, String> {
        if !crate::input_handoff::pump_slot_try_reserve() {
            return Err("error: input pump already active".to_owned());
        }
        let channels = Arc::new(InputChannels::new());
        if channels.intent_wake().is_none() || channels.work_wake().is_none() {
            crate::input_handoff::pump_slot_release();
            return Err("error: input wake creation failed".to_owned());
        }
        let (fail_tx, fail_rx) = sync_channel(16);
        let signals = Arc::new(PumpSignals::new(fail_tx));
        let thread_channels = Arc::clone(&channels);
        let thread_signals = Arc::clone(&signals);
        let thread = std::thread::Builder::new()
            .name("omnitiler-input".to_owned())
            .spawn(move || run_pump(thread_channels, thread_signals, keyboard_config, RealHooks))
            .map_err(|err| {
                crate::input_handoff::pump_slot_release();
                format!("error: input thread spawn failed: {err}")
            })?;
        let mut lifecycle = PumpLifecycle::new();
        lifecycle.start().expect("fresh handle starts");
        Ok(Self {
            channels,
            signals,
            failures: fail_rx,
            lifecycle,
            thread: Some(thread),
            stop_report: None,
            keyboard_config,
        })
    }

    /// Shared channels for owner publish/drain.
    #[must_use]
    pub fn channels(&self) -> &Arc<InputChannels> {
        &self.channels
    }

    /// Current lifecycle state.
    #[must_use]
    pub fn state(&self) -> crate::input_handoff::PumpState {
        self.lifecycle.state()
    }

    /// Keyboard config the pump installed with.
    #[must_use]
    pub const fn keyboard_config(&self) -> KeyboardConfig {
        self.keyboard_config
    }

    /// Bounded readiness wait. Drains install retry events for the owner
    /// writer while waiting. Returns true when the pump reported ready
    /// (lifecycle `Running`); false on timeout without claiming
    /// readiness. Install failures keep retrying on the pump; the owner
    /// keeps the handle and continues draining events on its tick.
    pub fn wait_ready(&mut self, timeout: Duration, on_event: &mut dyn FnMut(&FailEvent)) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            self.drain_failures(on_event);
            if self.signals.is_ready() {
                let _ = self.lifecycle.note_installed();
                return true;
            }
            if self.lifecycle.state() == crate::input_handoff::PumpState::Running {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Drain pending install/retry events without waiting (owner tick).
    pub fn drain_failures(&mut self, on_event: &mut dyn FnMut(&FailEvent)) {
        while let Ok(event) = self.failures.try_recv() {
            on_event(&event);
        }
        if self.signals.is_ready() {
            let _ = self.lifecycle.note_installed();
        }
    }

    /// Request graceful stop: closes admission (owner stops pushing) and
    /// signals the pump. Idempotent in `Stopping` so the owner can
    /// re-drive after a failure. The pump unhooks on its creator thread
    /// and acks; consumed pairs stay handled until that release.
    pub fn request_stop(&mut self) -> Result<(), crate::input_handoff::LifecycleError> {
        self.lifecycle.request_stop()?;
        self.channels.stop_flag.store(true, Ordering::SeqCst);
        // Wake the pump so a fully idle wait observes the stop promptly.
        self.channels.notify_work();
        Ok(())
    }

    /// Bounded stop wait. Clean requires the unhook acknowledgement
    /// *and* a finished thread whose join succeeds: a thread wedged
    /// between its ack and its return keeps its handle (no detach) and
    /// the call returns unclean, and a thread that panicked reports
    /// unclean even when the ack claimed success, so clean is never
    /// fabricated from an ack alone. A still-running thread keeps its
    /// handle for a later bounded wait; a second call returns the
    /// stored report instead of joining a consumed handle. Only a
    /// clean stop releases the process pump slot; every unclean path
    /// keeps the reserve, including a dropped handle. Never waits
    /// indefinitely and never holds channel locks while joining.
    pub fn wait_stopped(&mut self, timeout: Duration) -> StopReport {
        if let Some(report) = self.stop_report {
            return report;
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(unhook_ok) = self.signals.stopped() {
                // Ack arrived: complete only against a finished thread
                // whose join succeeds. A thread wedged between its ack
                // and its return, or one that panicked, keeps unclean:
                // the handle is retained for a later bounded wait (or
                // the panic is recorded) instead of blocking this call
                // past the deadline or fabricating clean.
                if let Some(thread) = self.thread.take() {
                    if thread.is_finished() {
                        let joined_ok = thread
                            .join()
                            .map(|report| report.unhook_ok)
                            .unwrap_or(false);
                        let report = self.complete_stop(unhook_ok && joined_ok);
                        self.stop_report = Some(report);
                        return report;
                    }
                    self.thread = Some(thread);
                } else {
                    let report = self.complete_stop(unhook_ok);
                    self.stop_report = Some(report);
                    return report;
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(thread) = self.thread.take() {
            if thread.is_finished() {
                let unhook_ok = thread
                    .join()
                    .map(|report| report.unhook_ok)
                    .unwrap_or(false);
                let report = self.complete_stop(unhook_ok);
                self.stop_report = Some(report);
                return report;
            }
            // Still running (hung native call): keep the handle so a
            // later bounded wait can still join; never detach.
            self.thread = Some(thread);
        }
        StopReport {
            unhook_ok: false,
            clean: false,
        }
    }

    /// Fold one unhook acknowledgement into the lifecycle, releasing
    /// the process slot only on a clean stop.
    fn complete_stop(&mut self, unhook_ok: bool) -> StopReport {
        match self.lifecycle.note_unhooked(unhook_ok) {
            Ok(report) => {
                if report.clean {
                    crate::input_handoff::pump_slot_release();
                }
                report
            }
            Err(_) => StopReport {
                unhook_ok,
                clean: false,
            },
        }
    }
}

/// Deliver pending LL callbacks for hooks installed here, without
/// waiting. The owner focus settle pump must not call this: it would
/// not deliver this thread's callbacks anyway, and LL delivery stays
/// isolated here.
fn drain_messages() {
    crate::ffi::drain_messages();
}

/// Wait for owner work, queued hook-callback messages, or the deadline.
/// `u32::MAX` waits indefinitely: the work wake and the message queue
/// still wake the pump; nothing polls. No locks are held across the wait
/// (callers drain/apply outside callbacks with try-locks before and
/// after, and transfer classifier work before blocking). Returns the
/// native wait reason; failed/unrecognized codes classify as `Failed`
/// with no behavior change (the step keeps its existing fallthrough).
fn wait_messages(timeout_ms: u32, work_wake: Option<&OwnedHandle>) -> super::InputWaitReason {
    use std::os::windows::io::{AsHandle as _, AsRawHandle as _, RawHandle};
    let has_work = work_wake.is_some();
    let raw = work_wake.map(|wake| wake.as_handle().as_raw_handle());
    let handles: &[RawHandle] = match &raw {
        Some(handle) => std::slice::from_ref(handle),
        None => &[],
    };
    let result = crate::ffi::msg_wait_raw(handles, timeout_ms);
    super::classify_input_wait(result, has_work)
}

/// Production pump step shared by the real thread and fake-backend
/// tests. All install / retry / apply / drain / offer / stop logic lives
/// here; only hook install, uninstall and message pumping go through the
/// [`NativeHooks`] backend. Fake backends perform no native effects, and
/// every `sys` helper used here is a no-op without installed hook state,
/// so step tests exercise the production logic with zero native calls.
/// `pub(crate)` (not public API) so the stage-3 combined stress can drive
/// the real core over the real transports; every member stays private
/// except the few marked `pub(crate)` below.
pub(crate) struct PumpCore<B: NativeHooks> {
    backend: B,
    channels: Arc<InputChannels>,
    signals: Arc<PumpSignals>,
    spawn_config: KeyboardConfig,
    keyboard_hook: Option<B::KeyboardHook>,
    mouse_hook: Option<B::MouseHook>,
    keyboard_failures: u32,
    mouse_failures: u32,
    keyboard_retry_at: Option<Instant>,
    mouse_retry_at: Option<Instant>,
    /// Transient try-lock contention observed on the last transfer
    /// (inbox drain, outbox probe/offer, callback summaries). Arms one
    /// bounded retry deadline; never a fixed poll.
    contention: bool,
    /// One-shot retry deadline while [`Self::contention`] is outstanding.
    contention_retry_at: Option<Instant>,
    /// Latest snapshot seen (install/apply source of truth).
    latest: Option<SharedSnapshot>,
    /// Last applied snapshot (policy baseline for change detection).
    applied: Option<SharedSnapshot>,
    /// Last mouse gate pushed (snapshot gate plus keyboard presence).
    last_mouse_gate: Option<bool>,
    /// Last sampled Win+Shift chord level for the transition-only owner
    /// wake. `None` until the first sample with the keyboard hook
    /// installed, so install itself never wakes. Only genuine
    /// transitions signal (steady hold and release stay quiet); the owner
    /// still samples the physical levels itself on the wake, so this is
    /// purely a wake hint with no policy content.
    last_chord: Option<bool>,
    pending_snap: Option<TaggedBatch<QueuedSnapEvent>>,
    pending_windrag: Option<TaggedBatch<WinDragEdge>>,
    diag: InputDiag,
    turns: u64,
    /// Cumulative native wait returns by reason (exactly one per wait;
    /// native returns only, never context switches). `wait_timeout` is the
    /// genuine deadline (`WAIT_TIMEOUT` 258) only; `WAIT_FAILED` and any
    /// unrecognized code count under `wait_failed`. Published into
    /// [`InputDiag`] each turn for the trace-only owner summary.
    wait_work: u64,
    wait_msg: u64,
    wait_timeout: u64,
    wait_failed: u64,
    cb_outbox_dropped: u32,
}

impl<B: NativeHooks> PumpCore<B> {
    /// New core over shared channels; installs on the first [`Self::step`].
    pub(crate) fn new(
        backend: B,
        channels: Arc<InputChannels>,
        signals: Arc<PumpSignals>,
        spawn_config: KeyboardConfig,
    ) -> Self {
        Self {
            backend,
            channels,
            signals,
            spawn_config,
            keyboard_hook: None,
            mouse_hook: None,
            keyboard_failures: 0,
            mouse_failures: 0,
            keyboard_retry_at: None,
            mouse_retry_at: None,
            contention: false,
            contention_retry_at: None,
            latest: None,
            applied: None,
            last_mouse_gate: None,
            last_chord: None,
            pending_snap: None,
            pending_windrag: None,
            diag: InputDiag::default(),
            turns: 0,
            wait_work: 0,
            wait_msg: 0,
            wait_timeout: 0,
            wait_failed: 0,
            cb_outbox_dropped: 0,
        }
    }

    /// Policy the next keyboard install must use: the latest snapshotted
    /// policy, falling back to the spawn config. Retries never reinstall
    /// a stale init after newer policy arrived.
    fn install_config(&self) -> KeyboardConfig {
        self.latest
            .as_ref()
            .map(|snap| snap.keyboard)
            .unwrap_or(self.spawn_config)
    }

    /// Currently applied policy generation (batch tag source).
    fn applied_generation(&self) -> u64 {
        self.applied
            .as_ref()
            .map(|snap| snap.generation)
            .unwrap_or(0)
    }

    /// One install attempt per missing hook; `None` until due. Each
    /// successful install reapplies the retained snapshot: install
    /// creates empty hook state, so without this a snapshot applied
    /// while TLS was absent would leave correct origins/gates missing
    /// indefinitely under change-gated publication.
    fn install_due(&mut self, now: Instant) {
        let mut keyboard_new = false;
        let mut mouse_new = false;
        if self.keyboard_hook.is_none() && self.keyboard_retry_at.is_none_or(|at| now >= at) {
            match self.backend.install_keyboard(self.install_config()) {
                Ok(hook) => {
                    self.keyboard_hook = Some(hook);
                    keyboard_new = true;
                    let recovered = self.keyboard_failures > 0;
                    self.keyboard_failures = 0;
                    self.keyboard_retry_at = None;
                    if recovered {
                        self.signals.note_failure(FailEvent {
                            hook: "keyboard",
                            attempt: 0,
                            retry_secs: 0,
                            recovered: true,
                        });
                    }
                }
                Err(_) => {
                    self.keyboard_failures += 1;
                    let backoff = install_backoff_secs(self.keyboard_failures);
                    self.keyboard_retry_at = Some(now + Duration::from_secs(backoff));
                    self.signals.note_failure(FailEvent {
                        hook: "keyboard",
                        attempt: self.keyboard_failures,
                        retry_secs: backoff,
                        recovered: false,
                    });
                }
            }
        }
        if self.mouse_hook.is_none() && self.mouse_retry_at.is_none_or(|at| now >= at) {
            match self.backend.install_mouse() {
                Ok(hook) => {
                    self.mouse_hook = Some(hook);
                    mouse_new = true;
                    let recovered = self.mouse_failures > 0;
                    self.mouse_failures = 0;
                    self.mouse_retry_at = None;
                    if recovered {
                        self.signals.note_failure(FailEvent {
                            hook: "mouse",
                            attempt: 0,
                            retry_secs: 0,
                            recovered: true,
                        });
                    }
                }
                Err(_) => {
                    self.mouse_failures += 1;
                    let backoff = install_backoff_secs(self.mouse_failures);
                    self.mouse_retry_at = Some(now + Duration::from_secs(backoff));
                    self.signals.note_failure(FailEvent {
                        hook: "mouse",
                        attempt: self.mouse_failures,
                        retry_secs: backoff,
                        recovered: false,
                    });
                }
            }
        }
        if self.keyboard_hook.is_some() && self.mouse_hook.is_some() {
            self.signals.note_ready();
        }
        if (keyboard_new || mouse_new)
            && let Some(snap) = self.latest.clone()
        {
            self.reapply_retained(&snap, keyboard_new, mouse_new);
        }
    }

    /// Reapply the retained snapshot onto freshly installed hook state.
    /// Policy tables restore whole; the stale-drain count is ignored
    /// (fresh queues are empty) and ack counters stay with the original
    /// apply, so this never double-counts.
    fn reapply_retained(&mut self, snap: &SharedSnapshot, keyboard: bool, mouse: bool) {
        if keyboard {
            let _ = crate::snapkey::sys::apply_snapshot(snap, true);
        }
        if mouse {
            crate::win_mouse::sys::apply_snapshot(snap, self.keyboard_hook.is_some());
            self.last_mouse_gate = Some(snap.active && self.keyboard_hook.is_some());
        }
    }

    /// Apply one FIFO item outside callbacks. Snapshots apply whole and
    /// in order; only a real policy change drains stale queued actions
    /// (origins/gate republishes never flush); invalidations drop origins
    /// without touching held-pair verdicts; terminal clears disarm
    /// without inventing synthetic key cleanup.
    /// Drop one origin from the retained snapshot so a later hook
    /// install (`reapply_retained`) or mouse-gate refresh
    /// (`refresh_mouse_gate`) reinstalls the invalidated state instead
    /// of rearming it. Held-pair verdicts and terminal swallow
    /// obligations are untouched (they live in the hook arm, not the
    /// retained maps).
    fn forget_retained_origin(&mut self, hwnd: u64) {
        if let Some(latest) = self.latest.as_mut() {
            let dot = |snap: &mut InputSnapshot| {
                snap.snap_origins.remove(&hwnd);
                snap.windrag_origins.remove(&hwnd);
            };
            if let Some(snap) = Arc::get_mut(latest) {
                dot(snap);
            } else {
                let mut clone = (**latest).clone();
                dot(&mut clone);
                *latest = Arc::new(clone);
            }
        }
    }

    /// Drop every origin from the retained snapshot after a clear, for
    /// the same no-rearm reason as [`Self::forget_retained_origin`].
    /// The owner republishes live origins with a forced snapshot; the
    /// cleared retention never rebinds obsolete ones first.
    fn forget_retained_all(&mut self) {
        if let Some(latest) = self.latest.as_mut() {
            let dot = |snap: &mut InputSnapshot| {
                snap.snap_origins.clear();
                snap.windrag_origins.clear();
            };
            if let Some(snap) = Arc::get_mut(latest) {
                dot(snap);
            } else {
                let mut clone = (**latest).clone();
                dot(&mut clone);
                *latest = Arc::new(clone);
            }
        }
    }

    fn apply_item(&mut self, item: &InboxItem) {
        match item {
            InboxItem::Snapshot(snap) => {
                let policy_changed = self
                    .applied
                    .as_ref()
                    .is_none_or(|cur| !snapshot_policy_eq(cur, snap));
                let stale = crate::snapkey::sys::apply_snapshot(snap, policy_changed);
                crate::win_mouse::sys::apply_snapshot(snap, self.keyboard_hook.is_some());
                self.last_mouse_gate = Some(snap.active && self.keyboard_hook.is_some());
                // Acknowledgements advance monotonically even when the
                // content equals the applied policy (owner revert after a
                // refused policy change reuses the armed generation):
                // without this the fence would stay pending forever while
                // the classifier is already back on the desired policy.
                // Map-only republishes keep the generation, so the max is
                // a no-op for them; stale drains still count only on real
                // policy changes.
                self.channels
                    .applied_policy_gen
                    .fetch_max(snap.generation, Ordering::SeqCst);
                note_policy_applied(snap.generation);
                if policy_changed {
                    self.channels
                        .acked_stale
                        .store(stale.min(u32::MAX as usize) as u32, Ordering::Relaxed);
                }
                self.applied = Some(Arc::clone(snap));
                self.latest = Some(Arc::clone(snap));
            }
            InboxItem::InvalidateWindrag(hwnd) => {
                crate::snapkey::sys::apply_invalidate_windrag(*hwnd);
                crate::win_mouse::sys::apply_invalidate(*hwnd);
                self.forget_retained_origin(*hwnd);
            }
            InboxItem::ClearWindrag => {
                crate::snapkey::sys::apply_clear_windrag_origins();
                crate::win_mouse::sys::apply_clear();
                self.forget_retained_all();
            }
            InboxItem::ClearEsc => {
                crate::snapkey::sys::clear_esc_edge();
                crate::snapkey::sys::clear_windrag_esc_edge();
                crate::input_handoff::global_esc_edge().store(false, Ordering::SeqCst);
                crate::input_handoff::global_windrag_esc_edge().store(false, Ordering::SeqCst);
            }
        }
    }

    /// Re-push the mouse gate when keyboard presence changed without a
    /// new snapshot (mouse consumes only with both hooks installed, since
    /// a missing keyboard cannot send the mask).
    fn refresh_mouse_gate(&mut self) {
        let gate = self
            .latest
            .as_ref()
            .map(|snap| snap.active && self.keyboard_hook.is_some())
            .unwrap_or(false);
        if self.last_mouse_gate != Some(gate)
            && self.mouse_hook.is_some()
            && let Some(snap) = self.latest.clone()
        {
            crate::win_mouse::sys::apply_snapshot(&snap, self.keyboard_hook.is_some());
            self.last_mouse_gate = Some(gate);
        }
    }

    /// Sample the tracked Win+Shift chord level and signal the owner
    /// wake on a genuine transition only. Runs on the pump thread
    /// between message drains (never in the hook callback, which stays
    /// non-blocking): bare modifier traffic queues no batch, so without
    /// this the owner would sleep through a chord press/release. Steady
    /// levels stay quiet; hookless sampling baselines silently. The owner
    /// still samples the physical levels itself on the wake, so this is
    /// purely a wake hint with no policy content.
    fn sample_chord_edge(&mut self) {
        if self.keyboard_hook.is_none() {
            return;
        }
        let (win, shift) = crate::snapkey::sys::chord_levels();
        let chord = tiler_core::visual::group_underlay_chord_held(win, shift);
        match self.last_chord {
            None => self.last_chord = Some(chord),
            Some(last) if last != chord => {
                self.last_chord = Some(chord);
                self.channels.notify_intent();
            }
            Some(_) => {}
        }
    }

    /// Drain the hook-state Esc-cancel edges on this thread and signal the
    /// owner wake when either was set. The Esc downs queue no batch, yet an
    /// open gesture must latch and hide its preview promptly (previously
    /// the fixed poll delivered this; an event-driven owner would otherwise
    /// wait for the END). Consumes only this thread's edges: the product
    /// owner latches through the process-global edges, so this steals
    /// nothing, and one edge fires exactly one wake instead of signaling
    /// every step while the global stays set pending the owner drain.
    fn sample_cancel_edge(&mut self) {
        if self.keyboard_hook.is_none() {
            return;
        }
        let (esc, windrag) = crate::snapkey::sys::take_pump_esc_edges();
        if esc || windrag {
            self.channels.notify_intent();
        }
    }

    /// Drain classifier queues into tagged outbox batches. Occupied
    /// outboxes skip draining so records wait in the real queues under
    /// their own bounds; refused offers park thread-locally with their
    /// generation and retry before any newer drain. A WouldBlock probe
    /// (contended, possibly free) sets [`Self::contention`] for one
    /// bounded retry instead of stranding behind an idle wait; a refused
    /// offer re-probes so true occupancy (owner wakes on take) never
    /// arms the retry.
    fn offer_batches(&mut self) {
        let generation = self.applied_generation();
        let trace = self.latest.as_ref().is_some_and(|snap| snap.trace);
        if let Some(pending) = self.pending_snap.take() {
            match self.channels.snap_outbox.try_put(pending) {
                Ok(_) => self.channels.notify_intent(),
                Err(batch) => {
                    if self.channels.snap_outbox.try_occupied() != Some(true) {
                        self.contention = true;
                    }
                    self.pending_snap = Some(batch);
                }
            }
        } else {
            match self.channels.snap_outbox.try_occupied() {
                Some(true) => {}
                Some(false) if self.keyboard_hook.is_some() => {
                    let items = crate::snapkey::sys::drain_up_to(OUTBOX_BATCH_CAP);
                    if !items.is_empty() {
                        let age = if trace {
                            snap_batch_age_us(&items, Instant::now())
                        } else {
                            0
                        };
                        match self
                            .channels
                            .snap_outbox
                            .try_put(TaggedBatch::new(generation, items))
                        {
                            Ok(_) => {
                                if trace {
                                    self.diag.note_offer(age, Instant::now());
                                }
                                self.channels.notify_intent();
                            }
                            Err(batch) => {
                                if self.channels.snap_outbox.try_occupied() != Some(true) {
                                    self.contention = true;
                                }
                                self.pending_snap = Some(batch);
                            }
                        }
                    }
                }
                Some(false) => {}
                None => self.contention = true,
            }
        }
        if let Some(pending) = self.pending_windrag.take() {
            match self.channels.windrag_outbox.try_put(pending) {
                Ok(_) => self.channels.notify_intent(),
                Err(batch) => {
                    if self.channels.windrag_outbox.try_occupied() != Some(true) {
                        self.contention = true;
                    }
                    self.pending_windrag = Some(batch);
                }
            }
        } else {
            match self.channels.windrag_outbox.try_occupied() {
                Some(true) => {}
                Some(false) if self.mouse_hook.is_some() => {
                    let items = crate::win_mouse::sys::drain_up_to(OUTBOX_BATCH_CAP);
                    if !items.is_empty() {
                        match self
                            .channels
                            .windrag_outbox
                            .try_put(TaggedBatch::new(generation, items))
                        {
                            Ok(_) => {
                                self.diag.windrag_offers += 1;
                                self.diag.windrag_last_offer = trace.then(Instant::now);
                                self.channels.notify_intent();
                            }
                            Err(batch) => {
                                if self.channels.windrag_outbox.try_occupied() != Some(true) {
                                    self.contention = true;
                                }
                                self.pending_windrag = Some(batch);
                            }
                        }
                    }
                }
                Some(false) => {}
                None => self.contention = true,
            }
        }
        match self.channels.cb_outbox.try_occupied() {
            Some(true) => {}
            Some(false) => {
                let diags = crate::snapkey::sys::drain_callback_diag();
                if !diags.is_empty() && self.channels.cb_outbox.try_put(diags).is_err() {
                    self.cb_outbox_dropped += 1;
                }
            }
            // Unknown occupancy: summaries stay queued under their own
            // bound for the contention retry instead of draining into a
            // counted drop.
            None => self.contention = true,
        }
    }

    /// Publish bounded diagnostics without waiting. Queue age and the
    /// per-turn dispatch span carry explicit non-latency limits; the
    /// heartbeat turn count proves liveness only. A contended publish
    /// skips the turn with no retry flag: the next turn republishes a
    /// fresher sample, so nothing strands.
    fn publish_diag(&mut self, dispatch_us: u64) {
        let trace = self.latest.as_ref().is_some_and(|snap| snap.trace);
        self.diag.snap_dropped = crate::snapkey::sys::queue_dropped();
        self.diag.windrag_dropped = crate::win_mouse::sys::queue_dropped();
        self.diag.cb_dropped = crate::snapkey::sys::callback_diag_dropped();
        self.diag.cb_filtered = crate::snapkey::sys::callback_diag_filtered();
        let (sends, max_us) = crate::snapkey::sys::mask_send_stats();
        self.diag.mask_sends = sends;
        self.diag.mask_send_max_us = max_us;
        self.diag.hook_stats = crate::win_mouse::sys::hook_stats();
        self.diag.turns = self.turns;
        self.diag.waits = self
            .wait_work
            .saturating_add(self.wait_msg)
            .saturating_add(self.wait_timeout)
            .saturating_add(self.wait_failed);
        self.diag.wait_work = self.wait_work;
        self.diag.wait_msg = self.wait_msg;
        self.diag.wait_timeout = self.wait_timeout;
        self.diag.wait_failed = self.wait_failed;
        self.diag.dispatch_us = if trace { dispatch_us } else { 0 };
        self.diag.delivery_gap_ms = if trace { take_delivery_gap_ms() } else { 0 };
        self.diag.pending_snap =
            self.pending_snap.is_some() as usize + self.pending_windrag.is_some() as usize;
        self.diag.cb_outbox_dropped = self.cb_outbox_dropped;
        let _ = self.channels.diag.try_publish(&self.diag);
    }

    /// Next wait deadline, if any. Install-retry dues plus one bounded
    /// transient-contention retry. `None` means fully idle: both hooks
    /// installed and no contention outstanding, so the pump may wait
    /// indefinitely. No wake is lost: owner inbox/control/flush/drain/
    /// stop signals break the wait through the work wake, hook-callback
    /// messages arrive through the message queue itself, and callback
    /// work queued during the pre-wait drain transfers before blocking.
    /// A missing hook with no backoff scheduled is due immediately.
    pub(crate) fn next_timer(&self) -> Option<Instant> {
        let mut earliest: Option<Instant> = None;
        if self.keyboard_hook.is_none() {
            match self.keyboard_retry_at {
                Some(due) => {
                    earliest = Some(earliest.map_or(due, |prev| prev.min(due)));
                }
                None => return Some(Instant::now()),
            }
        }
        if self.mouse_hook.is_none() {
            match self.mouse_retry_at {
                Some(due) => {
                    earliest = Some(earliest.map_or(due, |prev| prev.min(due)));
                }
                None => return Some(Instant::now()),
            }
        }
        if let Some(due) = self.contention_retry_at {
            earliest = Some(earliest.map_or(due, |prev| prev.min(due)));
        }
        earliest
    }

    /// Test-only: make pending install retries due now, so recovery runs
    /// through the real step without wall-clock backoff waits.
    #[cfg(test)]
    pub(crate) fn test_force_retry_due(&mut self) {
        let now = Instant::now();
        if self.keyboard_hook.is_none() {
            self.keyboard_retry_at = Some(now);
        }
        if self.mouse_hook.is_none() {
            self.mouse_retry_at = Some(now);
        }
    }

    /// One pump turn: install/retry, FIFO apply, pre-wait message
    /// delivery plus batch transfer, deadline wait, post-wait delivery
    /// plus transfer, diagnostics. Returns true when a stop was
    /// requested. The pre-wait transfer is load-bearing: callbacks
    /// delivered during the pre-wait drain queue classifier work that
    /// must offer before blocking, or the final callback strands behind
    /// an idle wait with nothing left to wake the pump.
    pub(crate) fn step(&mut self) -> bool {
        if self.channels.stop_flag.load(Ordering::SeqCst) {
            return true;
        }
        self.contention = false;
        // FIFO apply before install/retry so a policy snapshot that
        // arrived while hooks were down is the config of the next
        // install attempt, never a stale spawn init. A contended drain
        // (lock held by an owner offer) arms the bounded retry below.
        match self.channels.inbox.try_drain() {
            Some(items) => {
                for item in &items {
                    self.apply_item(item);
                }
                // Capacity freed: a stalled owner retry (refused snapshot
                // or outstanding sender payloads) gets exactly one wake to
                // re-offer. No feedback loop: the flag clears on signal and
                // re-arms only if the retry turn still finds no room.
                if self.channels.inbox_stalled.swap(false, Ordering::SeqCst) {
                    self.channels.notify_intent();
                }
            }
            None => self.contention = true,
        }
        let now = Instant::now();
        self.install_due(now);
        self.refresh_mouse_gate();
        let pre_start = Instant::now();
        self.backend.drain_messages();
        self.offer_batches();
        self.sample_chord_edge();
        self.sample_cancel_edge();
        let pre_us = pre_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        // Deadline-driven wait: install retries plus one bounded
        // transient-contention retry; fully idle waits indefinitely.
        // Owner signals and queued hook callbacks break the wait early,
        // so nothing polls.
        if self.contention {
            self.contention_retry_at = Some(Instant::now() + super::CONTENTION_RETRY);
        } else {
            self.contention_retry_at = None;
        }
        let timeout_ms = match self.next_timer() {
            Some(due) => due
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(u128::from(u32::MAX)) as u32,
            None => u32::MAX,
        };
        let reason = self
            .backend
            .wait_messages(timeout_ms, self.channels.work_wake());
        match reason {
            super::InputWaitReason::Work => {
                self.wait_work = self.wait_work.saturating_add(1);
            }
            super::InputWaitReason::Message => {
                self.wait_msg = self.wait_msg.saturating_add(1);
            }
            super::InputWaitReason::Timeout => {
                self.wait_timeout = self.wait_timeout.saturating_add(1);
            }
            super::InputWaitReason::Failed => {
                self.wait_failed = self.wait_failed.saturating_add(1);
            }
        }
        self.contention = false;
        let post_start = Instant::now();
        self.backend.drain_messages();
        self.offer_batches();
        self.sample_chord_edge();
        self.sample_cancel_edge();
        let post_us = post_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        if self.contention {
            self.contention_retry_at = Some(Instant::now() + super::CONTENTION_RETRY);
        }
        self.turns += 1;
        // Dispatch covers delivery plus transfer only, never the wait.
        self.publish_diag(pre_us.saturating_add(post_us));
        self.channels.stop_flag.load(Ordering::SeqCst)
    }

    /// Creator-side shutdown with bounded unhook retries: consumed pairs
    /// stay handled until the release; no wait for physical release and
    /// no synthetic key cleanup. Hook state is retained on failure (the
    /// `sys` uninstalls only mutate on success), so a failed release
    /// never strands a swallowed pair as a passed-through Up.
    /// `pub(crate)` for the stage-3 teardown matrix; the production path
    /// runs it on the pump thread via [`run_pump`].
    pub(crate) fn shutdown(&mut self) -> StopReport {
        const RETRY_DELAYS_MS: [u64; 3] = [100, 500, 1000];
        let mut unhook_ok = true;
        if let Some(mut hook) = self.keyboard_hook.take() {
            let mut ok = self.backend.uninstall_keyboard(&mut hook);
            for delay in RETRY_DELAYS_MS {
                if ok {
                    break;
                }
                std::thread::sleep(Duration::from_millis(delay));
                ok = self.backend.uninstall_keyboard(&mut hook);
            }
            unhook_ok &= ok;
        }
        if let Some(mut hook) = self.mouse_hook.take() {
            let mut ok = self.backend.uninstall_mouse(&mut hook);
            for delay in RETRY_DELAYS_MS {
                if ok {
                    break;
                }
                std::thread::sleep(Duration::from_millis(delay));
                ok = self.backend.uninstall_mouse(&mut hook);
            }
            unhook_ok &= ok;
        }
        StopReport {
            unhook_ok,
            clean: unhook_ok,
        }
    }
}

/// Pump thread body: installs both hooks on this thread, reports
/// readiness, exchanges batches, and unhooks here on stop. Never logs.
/// The stop acknowledgement is recorded as the terminal act before
/// returning; clean still requires a finished thread whose join succeeds
/// (`is_finished` first, so a wedged thread never blocks the owner past
/// the bound and a panicked thread never reports clean from an ack
/// alone).
fn run_pump(
    channels: Arc<InputChannels>,
    signals: Arc<PumpSignals>,
    keyboard_config: KeyboardConfig,
    backend: RealHooks,
) -> StopReport {
    let mut core = PumpCore::new(backend, Arc::clone(&channels), signals, keyboard_config);
    loop {
        if core.step() {
            break;
        }
    }
    let report = core.shutdown();
    core.signals.note_stopped(report.unhook_ok);
    report
}

#[cfg(test)]
mod core_tests;
