//! Dedicated input pump (stage 1, unit 2).
//!
//! Both low-level hooks and their synchronous consume/pass classifiers share
//! one dedicated input thread. The owner keeps observation, Engine, foreign
//! actuation, overlays, tray, settings, trace writing, Mouse Snap SPI and the
//! single recovery ledger.
//!
//! Transport contract (from [`crate::input_handoff`]):
//! - Owner builds immutable snapshots off the input thread and moves them
//!   into the bounded [`Inbox`] FIFO whole and in order. No latest-wins, no
//!   unbounded batch channel.
//! - The input pump applies FIFO items between message pumps into local
//!   classifier state; callbacks only read that local state plus one
//!   single-entry origin clone. Large maps are never cloned in callbacks.
//! - All input-side exchanges use `try_lock` variants: callbacks and the
//!   pump never wait on an owner-held mutex. Full is an explicit fault with
//!   the item preserved for retry; fresh admission closes immediately and no
//!   unbounded owner backlog accumulates.
//! - Input drains the real bounded classifier queues into single-outstanding
//!   [`Outbox`] batches outside callbacks. While an outbox is occupied the
//!   pump skips draining so items wait in the original queues under their own
//!   cap+priority rules; a refused offer retries from a thread-local pending
//!   slot, never a second lossy queue. Saturated new Downs pass through and
//!   terminal Up/Cancel edges keep their guaranteed delivery end to end.
//! - Esc ordering uses shared atomic [`EscClock`]s sampled directly at
//!   WinEvent START/END (owner) and mouse Down/Up plus Esc edges (input).
//!   Delayed batch sequences never decide cancellation.
//! - Both hooks stay together on this pump for the synchronous mouse-Down
//!   `arm_windrag_mask` plus the E8 reserve/send/stamp reentry path.
//! - Moves pass through; no motion intent stream.
//! - Logging stays owner-side: the pump publishes bounded [`InputDiag`]
//!   samples; queue age and heartbeat gaps carry explicit non-latency limits.
//!
//! Proof hook code (`install_proof`) is preserved untouched for the separate
//! `shortcut-proof` command; the product run uses this pump.

use std::collections::VecDeque;
#[cfg(windows)]
use std::os::windows::io::{AsHandle as _, AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use crate::input_handoff::{
    DiagSlot, INBOX_CAP, Inbox, InboxItem, InputSnapshot, Outbox, SharedSnapshot, TaggedBatch,
};
use crate::snapkey::{KeyboardConfig, QueuedSnapEvent};
use crate::win_mouse::WinDragEdge;

/// Bounded install-retry backoff in seconds: 5s doubling, 60s cap. Mirrors
/// the existing owner keyboard retry so recovery timing does not change.
#[must_use]
pub fn install_backoff_secs(failures: u32) -> u64 {
    (5u64 << failures.min(4)).min(60)
}

/// How long the owner waits for input readiness / stop acknowledgement
/// before reporting failure without claiming success.
pub const READY_WAIT: Duration = Duration::from_secs(2);
/// Bounded stop acknowledgement wait: a missing ack returns failure and
/// retains `Stopping` (never claims clean, never starts an overlapping
/// worker).
pub const STOP_WAIT: Duration = Duration::from_secs(5);
/// Bounded transient-contention retry: one-shot wait while a try-lock
/// exchange reported WouldBlock (contended, possibly free). Lock holds are
/// sub-millisecond by construction (try-locks only, never held across
/// waits), so this fires at most once per contention sighting; steady state
/// returns to deadline/indefinite waits. Never a fixed idle poll.
pub const CONTENTION_RETRY: Duration = Duration::from_millis(5);

/// Native pump wait return reason: exactly one per wait. Counts native
/// `MsgWaitForMultipleObjectsEx` returns only, never thread context switches.
/// `Timeout` is the genuine deadline expiry (`WAIT_TIMEOUT`, 258) only;
/// `Failed` is `WAIT_FAILED` or any unrecognized code, counted separately and
/// never merged into timeout. Classification is telemetry only: the loop keeps
/// its existing fallthrough to periodic work either way, with no new recovery
/// and no timing claim (a failed wait returns immediately; not proven delayed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputWaitReason {
    Work,
    Message,
    Timeout,
    Failed,
}

/// Classify one native input-wait return code. Pure (no native calls) so
/// tests prove the work/message/timeout/failed split without a desktop.
#[cfg(windows)]
#[must_use]
pub(crate) fn classify_input_wait(result: u32, has_work: bool) -> InputWaitReason {
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;
    if result == WAIT_TIMEOUT {
        return InputWaitReason::Timeout;
    }
    if has_work {
        if result == WAIT_OBJECT_0 {
            return InputWaitReason::Work;
        }
        if result == WAIT_OBJECT_0 + 1 {
            return InputWaitReason::Message;
        }
    } else if result == WAIT_OBJECT_0 {
        return InputWaitReason::Message;
    }
    InputWaitReason::Failed
}

/// Fixed cap on distinct stalled-transport invalidated HWNDs held in the
/// safety slots. Overflow escalates conservatively to a full windrag clear
/// (see [`InboxSender`]): total outstanding safety state stays fixed at this
/// cap plus two clear flags, regardless of owner window churn.
pub const SAFETY_INVALIDATES_CAP: usize = 8;

/// Bounded owner -> input sender. Full is an explicit fault: snapshots are
/// refused for rebuild-and-retry later (never queued here), while every
/// invalidation/suspend-clear obligation merges into fixed-bound
/// safety-state slots (never refused, never dropped). Fresh admission closes
/// on the first fault and reopens once everything drains; no unbounded owner
/// backlog accumulates.
///
/// Slot bounds: at most [`SAFETY_INVALIDATES_CAP`] distinct invalidated HWNDs
/// plus one windrag-clear flag plus one Esc-clear flag, all fixed. Overflow
/// escalates to `ClearWindrag`, which is a proven superset of every
/// invalidation: it drops all origins on both hooks and disarms any armed
/// gesture with a Cancel while retaining the swallow-until-up obligation
/// (the suspend path already applies exactly this pair). Escalation only
/// over-disarms: fresh gestures fail closed until the next snapshot
/// republishes, and no stranded pair or lost suspend-clear is possible.
/// Exact duplicates collapse (counted). Slot order on expansion is
/// `ClearWindrag`, invalidates in arrival order, then `ClearEsc`: a reordered
/// invalidate-after-clear is a harmless redundant disarm (invalidations never
/// rearm; snapshots ahead in the FIFO applied first in order), never a lost
/// obligation.
#[derive(Debug)]
pub struct InboxSender {
    inbox: Arc<Inbox>,
    pending: VecDeque<InboxItem>,
    /// Stalled-transport invalidated HWNDs, arrival order, deduplicated.
    invalidates: Vec<u64>,
    /// Stalled-transport suspend/teardown windrag clear.
    clear_windrag: bool,
    /// Stalled-transport suspend/teardown Esc clear.
    clear_esc: bool,
    /// Fresh snapshots/controls must not be admitted while true (inbox fault
    /// outstanding). Ordered accepted controls/config already in the inbox or
    /// pending are preserved and retried.
    pub admission_closed: bool,
    /// Explicit inbox-full faults observed (trace evidence).
    pub faults: u64,
    /// Exact-duplicate offers collapsed while pending (trace evidence).
    pub deduped: u64,
    /// Accepted `ClearWindrag` obligations observed (monotonic). The owner
    /// watches this to force a full snapshot republish after clears: a clear
    /// drops both hook origin maps while the owner's `last_snapshot` still
    /// carries the old origins, so without a forced republish the correct
    /// origins stay absent indefinitely under change-gated publication.
    /// Incremented once per non-duplicate accepted clear (direct or slotted,
    /// including overflow escalation); duplicate collapses never increment.
    windrag_cleared: u64,
}

impl InboxSender {
    /// New sender over a shared inbox.
    #[must_use]
    pub fn new(inbox: Arc<Inbox>) -> Self {
        Self {
            inbox,
            pending: VecDeque::with_capacity(INBOX_CAP),
            invalidates: Vec::new(),
            clear_windrag: false,
            clear_esc: false,
            admission_closed: false,
            faults: 0,
            deduped: 0,
            windrag_cleared: 0,
        }
    }

    /// Accepted `ClearWindrag` obligations observed so far (monotonic owner
    /// republish trigger; see the field docs).
    #[must_use]
    pub fn windrag_cleared(&self) -> u64 {
        self.windrag_cleared
    }

    /// Slotted safety obligations not yet expanded into the FIFO.
    #[must_use]
    pub fn safety_len(&self) -> usize {
        self.invalidates.len() + usize::from(self.clear_windrag) + usize::from(self.clear_esc)
    }

    /// Pop one slotted obligation in expansion order.
    fn next_slot_item(&mut self) -> Option<InboxItem> {
        if self.clear_windrag {
            self.clear_windrag = false;
            return Some(InboxItem::ClearWindrag);
        }
        if !self.invalidates.is_empty() {
            return Some(InboxItem::InvalidateWindrag(self.invalidates.remove(0)));
        }
        if self.clear_esc {
            self.clear_esc = false;
            return Some(InboxItem::ClearEsc);
        }
        None
    }

    /// Retain one safety obligation while stalled. Snapshots rebuild later
    /// (`Err`); every invalidation/suspend-clear merges into the fixed-bound
    /// slots (`Ok`, never dropped, never refused).
    fn offer_slotted(&mut self, item: InboxItem) -> Result<(), InboxItem> {
        match item {
            InboxItem::Snapshot(_) => Err(item),
            InboxItem::ClearWindrag => {
                self.clear_windrag = true;
                self.windrag_cleared = self.windrag_cleared.saturating_add(1);
                // Subsumed: older slotted invalidates add nothing to a clear
                // that disarms every gesture and drops every origin.
                self.invalidates.clear();
                Ok(())
            }
            InboxItem::ClearEsc => {
                self.clear_esc = true;
                Ok(())
            }
            InboxItem::InvalidateWindrag(hwnd) => {
                // Slot duplicates collapsed before admission/capacity checks,
                // so reaching here means a new distinct obligation.
                if self.invalidates.len() >= SAFETY_INVALIDATES_CAP {
                    // Fixed-bound escalation: a full windrag clear is a proven
                    // superset of every invalidation (all origins dropped,
                    // any armed gesture disarmed with Cancel, swallow
                    // retained). Conservative over-disarm only; the bound
                    // holds regardless of window churn.
                    self.clear_windrag = true;
                    self.windrag_cleared = self.windrag_cleared.saturating_add(1);
                    self.invalidates.clear();
                } else {
                    self.invalidates.push(hwnd);
                }
                Ok(())
            }
        }
    }

    /// True for whole-payload republishes (never subsume safety controls).
    fn is_snapshot(item: &InboxItem) -> bool {
        matches!(item, InboxItem::Snapshot(_))
    }

    /// True for safety controls (invalidations and suspend/teardown clears).
    fn is_safety(item: &InboxItem) -> bool {
        matches!(
            item,
            InboxItem::InvalidateWindrag(_) | InboxItem::ClearWindrag | InboxItem::ClearEsc
        )
    }

    /// Offer one item. `Ok` for everything except snapshots refused while
    /// stalled (rebuildable): controls are infallibly absorbed (queued,
    /// buffered in order, collapsed as exact duplicates, or merged into the
    /// fixed-bound safety slots with clear escalation). `Err` returns a
    /// refused snapshot intact. Accepted order is always FIFO: pending drains
    /// before new items, slots expand after pending. Nothing
    /// safety-relevant is ever dropped inside.
    pub fn offer(&mut self, item: InboxItem) -> Result<(), InboxItem> {
        // Duplicate collapse is lossless only with no intervening
        // opposite-kind payload between the preserved duplicate and this
        // offer. A snapshot between two equal invalidates/clears rebinds
        // (repullishes) the origin the first one dropped, so the newer
        // control is a fresh obligation that must be preserved; symmetrically
        // a control between two equal snapshots means the newer snapshot
        // rebinds what the control dropped. Pending is sender-visible;
        // queued inbox payloads are probed without waiting (contention fails
        // closed to "present", preserving the newest work).
        let snapshot_between =
            self.pending.iter().any(Self::is_snapshot) || self.inbox.has_snapshot();
        let safety_between = self.pending.iter().any(Self::is_safety) || self.inbox.has_safety();
        let dedupe_safe = if Self::is_snapshot(&item) {
            !safety_between
        } else {
            !snapshot_between
        };
        // Exact duplicates of preserved work collapse first (counted): the
        // work is already ordered ahead, so collapsing is lossless even
        // while admission is closed.
        if dedupe_safe && self.pending.iter().any(|staged| *staged == item) {
            self.deduped += 1;
            return Ok(());
        }
        // Slot duplicates collapse the same way (counted).
        if dedupe_safe {
            match &item {
                InboxItem::ClearEsc if self.clear_esc => {
                    self.deduped += 1;
                    return Ok(());
                }
                InboxItem::ClearWindrag if self.clear_windrag => {
                    self.deduped += 1;
                    return Ok(());
                }
                InboxItem::InvalidateWindrag(hwnd) if self.invalidates.contains(hwnd) => {
                    self.deduped += 1;
                    return Ok(());
                }
                _ => {}
            }
        }
        // Every non-duplicate accepted clear is a fresh origin-drop
        // obligation the owner must republish after (see `windrag_cleared`).
        let accepted_clear = matches!(item, InboxItem::ClearWindrag);
        if self.admission_closed {
            let outcome = self.offer_slotted(item);
            // `offer_slotted` already counted slotted/escalated clears; a
            // direct clear can never reach here refused, so nothing further.
            return outcome;
        }
        if !self.pending.is_empty() {
            if self.pending.len() >= INBOX_CAP {
                self.faults += 1;
                self.admission_closed = true;
                return self.offer_slotted(item);
            }
            self.pending.push_back(item);
            if accepted_clear {
                self.windrag_cleared = self.windrag_cleared.saturating_add(1);
            }
            return Ok(());
        }
        let mut slot = None;
        if self.inbox.try_push(item, &mut slot) {
            if accepted_clear {
                self.windrag_cleared = self.windrag_cleared.saturating_add(1);
            }
            return Ok(());
        }
        let item = slot.expect("refused item preserved");
        if self.pending.len() >= INBOX_CAP {
            self.faults += 1;
            self.admission_closed = true;
            return self.offer_slotted(item);
        }
        let buffered_clear = matches!(item, InboxItem::ClearWindrag);
        self.pending.push_back(item);
        if buffered_clear {
            self.windrag_cleared = self.windrag_cleared.saturating_add(1);
        }
        self.faults += 1;
        self.admission_closed = true;
        Ok(())
    }

    /// Retry pending items in order, then expand slotted safety obligations
    /// behind them. Returns items still outstanding (pending plus slots).
    /// Admission reopens once fully drained.
    pub fn flush(&mut self) -> usize {
        loop {
            if self.pending.is_empty() {
                let Some(next) = self.next_slot_item() else {
                    break;
                };
                self.pending.push_back(next);
            }
            let item = self.pending.pop_front().expect("pending item");
            let mut slot = None;
            if self.inbox.try_push(item, &mut slot) {
                continue;
            }
            self.pending
                .push_front(slot.expect("refused item preserved"));
            self.faults += 1;
            return self.pending.len() + self.safety_len();
        }
        self.admission_closed = false;
        0
    }

    /// Pending item count.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

/// Shared input channels owned by the owner handle and the pump thread.
///
/// Esc clocks and edges are process-global singletons in
/// [`crate::input_handoff`] (callbacks bump them, WinEvent START/END and the
/// owner settle sample them), so handles can never mismatch clocks across
/// restarts. Generations pair exactly one owner handle with its pump thread.
#[derive(Debug)]
pub struct InputChannels {
    /// Owner -> input FIFO.
    pub inbox: Arc<Inbox>,
    /// Input -> owner snap batches, tagged with the applied policy
    /// generation at drain time.
    pub snap_outbox: Arc<Outbox<TaggedBatch<QueuedSnapEvent>>>,
    /// Input -> owner windrag batches, tagged like [`Self::snap_outbox`].
    pub windrag_outbox: Arc<Outbox<TaggedBatch<WinDragEdge>>>,
    /// Input -> owner trace callback summaries (closed vocabulary only).
    pub cb_outbox: Arc<Outbox<Vec<crate::snapkey::CallbackDiag>>>,
    /// Bounded diagnostics published by the pump.
    pub diag: Arc<DiagSlot>,
    /// Latest policy generation the pump applied, in FIFO order. The owner
    /// drops batches tagged older than its published policy as stale.
    pub applied_policy_gen: Arc<std::sync::atomic::AtomicU64>,
    /// Stale queued records dropped by the latest policy-changing apply
    /// (real classifier queue; outbox/pending records are dropped
    /// owner-side by generation). Read with [`Self::applied_policy_gen`].
    pub acked_stale: Arc<std::sync::atomic::AtomicU32>,
    /// Stop requested by the owner; pump unhooks on its own thread and acks.
    pub stop_flag: Arc<AtomicBool>,
    /// Owner-intent wake: auto-reset kernel event signaled by the pump after
    /// each accepted keyboard/mouse batch (fresh or pending-retry). Stays
    /// pending between offer and the owner outer wait; coalesces bursts.
    /// `None` only when creation failed (spawn refuses cleanly).
    #[cfg(windows)]
    pub intent_wake: Option<OwnedHandle>,
    /// Owner-to-input work wake: auto-reset kernel event signaled by the
    /// owner after every accepted inbox publish/control, retry flush with
    /// payload, outbox drain, and stop request. Breaks the pump wait early
    /// so no fixed idle quantum is needed; coalesces bursts. `None` only
    /// when creation failed (spawn refuses cleanly).
    #[cfg(windows)]
    pub work_wake: Option<OwnedHandle>,
}

/// Create the owner-intent wake: auto-reset, initially non-signaled.
#[cfg(windows)]
fn create_intent_event() -> Result<OwnedHandle, String> {
    let handle = unsafe {
        windows_sys::Win32::System::Threading::CreateEventW(
            std::ptr::null(),
            0,
            0,
            std::ptr::null(),
        )
    };
    if handle.is_null() {
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Err(format!("error: intent wake creation failed: win32-{code}"));
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

impl InputChannels {
    /// New channels with fresh clocks and empty queues.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inbox: Arc::new(Inbox::new()),
            snap_outbox: Arc::new(Outbox::new()),
            windrag_outbox: Arc::new(Outbox::new()),
            cb_outbox: Arc::new(Outbox::new()),
            diag: Arc::new(DiagSlot::new()),
            applied_policy_gen: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            acked_stale: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            #[cfg(windows)]
            intent_wake: create_intent_event().ok(),
            #[cfg(windows)]
            work_wake: create_intent_event().ok(),
        }
    }
}

#[cfg(windows)]
impl InputChannels {
    /// Borrow the owner-intent wake, if creation succeeded.
    pub fn intent_wake(&self) -> Option<&OwnedHandle> {
        self.intent_wake.as_ref()
    }

    /// Signal the owner-intent wake (best-effort, never waits, never logs).
    /// No-op when creation failed; the owner keeps its periodic fallback.
    pub fn notify_intent(&self) {
        if let Some(handle) = self.intent_wake.as_ref() {
            unsafe {
                windows_sys::Win32::System::Threading::SetEvent(handle.as_handle().as_raw_handle());
            }
        }
    }

    /// Borrow the owner-to-input work wake, if creation succeeded.
    pub fn work_wake(&self) -> Option<&OwnedHandle> {
        self.work_wake.as_ref()
    }

    /// Signal the owner-to-input work wake (best-effort, never waits, never
    /// logs). No-op when creation failed; the pump falls back to its retry
    /// deadlines and message-queue wake.
    pub fn notify_work(&self) {
        if let Some(handle) = self.work_wake.as_ref() {
            unsafe {
                windows_sys::Win32::System::Threading::SetEvent(handle.as_handle().as_raw_handle());
            }
        }
    }
}

impl Default for InputChannels {
    fn default() -> Self {
        Self::new()
    }
}

/// Oldest snap-batch queue age in microseconds. `u64::MAX` saturates the
/// `u128` conversion; empty batches report zero.
#[must_use]
pub fn snap_batch_age_us(batch: &[QueuedSnapEvent], now: Instant) -> u64 {
    let mut oldest: Option<Instant> = None;
    for event in batch {
        let tick = match event {
            QueuedSnapEvent::Intent(i) => i.tick,
            QueuedSnapEvent::Resize(i) => i.tick,
            QueuedSnapEvent::Workspace(i) => i.tick,
            QueuedSnapEvent::WorkspaceSend(i) => i.tick,
            QueuedSnapEvent::WorkspaceHistory(i) => i.tick,
            QueuedSnapEvent::Maximize(i) => i.tick,
            QueuedSnapEvent::Fullscreen(i) => i.tick,
            QueuedSnapEvent::Float(i) => i.tick,
            QueuedSnapEvent::Sticky(i) => i.tick,
            QueuedSnapEvent::Orientation(i) => i.tick,
            QueuedSnapEvent::WorkspaceTiling(i) => i.tick,
            QueuedSnapEvent::Launch(i) => i.tick,
            QueuedSnapEvent::Mask(i) => i.tick,
        };
        oldest = Some(match oldest {
            Some(cur) => cur.min(tick),
            None => tick,
        });
    }
    match oldest {
        Some(tick) => now
            .duration_since(tick)
            .as_micros()
            .min(u128::from(u64::MAX)) as u64,
        None => 0,
    }
}

/// Portable scripted backend for deterministic install/retry/stop tests. The
/// production pump step drives this through the `NativeHooks` seam under
/// `cfg(windows)`; the pure backoff/queue/ordering vocabulary stays portable
/// here. No threads, no native calls.
#[derive(Debug, Default)]
pub struct FakeBackend {
    /// Scripted keyboard install results (`true` = success), consumed in order.
    pub keyboard_script: VecDeque<bool>,
    /// Scripted mouse install results, consumed in order.
    pub mouse_script: VecDeque<bool>,
    /// Scripted unhook results, consumed in order (defaults to success).
    pub unhook_script: VecDeque<bool>,
    /// Keyboard install attempts so far.
    pub keyboard_attempts: u32,
    /// Mouse install attempts so far.
    pub mouse_attempts: u32,
    /// Message drains requested (two per completed pump step).
    pub pump_turns: u32,
    /// Message-pump timeout per turn in milliseconds (deadline evidence:
    /// install-retry deadline while hooks are down, `u32::MAX` while idle).
    pub pump_timeouts: Vec<u32>,
    /// Scripted classifier items enqueued during each message drain,
    /// mimicking LL callbacks that ran on the pump thread (test-only: no
    /// native effects). Drained by the pre-wait transfer on the same turn.
    pub snap_on_drain: Vec<QueuedSnapEvent>,
    /// Scripted windrag edges enqueued during each message drain, like
    /// [`Self::snap_on_drain`].
    pub windrag_on_drain: Vec<WinDragEdge>,
    /// When true (test-only), the wait asserts no classifier work remains
    /// queued: pre-wait callback work must transfer
    /// before blocking.
    pub assert_transferred_before_wait: bool,
    /// Keyboard config seen at each install attempt (stale-init evidence).
    pub keyboard_configs: Vec<KeyboardConfig>,
    /// Retry backoffs recorded (evidence).
    pub retry_secs: Vec<u64>,
    /// Scripted native wait reasons, consumed in order (defaults to timeout).
    pub wait_script: VecDeque<InputWaitReason>,
}

impl FakeBackend {
    /// New backend with scripted install outcomes (unhook succeeds).
    #[must_use]
    pub fn new(keyboard_script: Vec<bool>, mouse_script: Vec<bool>, unhook_ok: bool) -> Self {
        let mut unhook_script = VecDeque::new();
        if !unhook_ok {
            unhook_script.push_back(false);
        }
        Self {
            keyboard_script: keyboard_script.into(),
            mouse_script: mouse_script.into(),
            unhook_script,
            keyboard_attempts: 0,
            mouse_attempts: 0,
            pump_turns: 0,
            pump_timeouts: Vec::new(),
            snap_on_drain: Vec::new(),
            windrag_on_drain: Vec::new(),
            assert_transferred_before_wait: false,
            keyboard_configs: Vec::new(),
            retry_secs: Vec::new(),
            wait_script: VecDeque::new(),
        }
    }

    /// Record a backoff wait. Returns the waited seconds.
    pub fn note_retry(&mut self, failures: u32) -> u64 {
        let secs = install_backoff_secs(failures);
        self.retry_secs.push(secs);
        secs
    }
}

/// Build one whole owner snapshot behind an `Arc` without touching input
/// state. Pure constructor for the owner thread.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn shared_snapshot(
    generation: u64,
    keyboard: KeyboardConfig,
    remap: Vec<crate::snapkey::ChordRemap>,
    disabled: Vec<crate::snapkey::ChordDisable>,
    snap_origins: std::collections::HashMap<u64, crate::snapkey::SnapOrigin>,
    windrag_origins: std::collections::HashMap<u64, crate::input_handoff::WindragOrigin>,
    active: bool,
    trace: bool,
) -> SharedSnapshot {
    Arc::new(InputSnapshot {
        generation,
        keyboard,
        remap,
        disabled,
        snap_origins,
        windrag_origins,
        active,
        trace,
    })
}

#[cfg(windows)]
mod native {
    use super::{InputChannels, install_backoff_secs, snap_batch_age_us};
    use crate::input_handoff::{
        InboxItem, InputDiag, InputSnapshot, OUTBOX_BATCH_CAP, PumpLifecycle, SharedSnapshot,
        StopReport, TaggedBatch, note_policy_applied, snapshot_policy_eq, take_delivery_gap_ms,
    };
    use crate::snapkey::{KeyboardConfig, QueuedSnapEvent};
    use crate::win_mouse::WinDragEdge;
    use std::collections::VecDeque;
    use std::os::windows::io::{AsHandle as _, AsRawHandle as _, OwnedHandle};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{SyncSender, sync_channel},
    };
    use std::time::{Duration, Instant};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
        TranslateMessage,
    };

    /// Native hook backend seam: real `sys` calls in production, scripted
    /// fakes in tests. The same [`PumpCore`] step drives both, so install /
    /// retry / stop behavior under test is the production logic, not a toy.
    /// Implementations must never perform native effects in tests: fake
    /// backends record calls and return scripted results with no hooks.
    pub trait NativeHooks {
        /// Installed keyboard hook token (real `HHOOK`, fake unit).
        type KeyboardHook;
        /// Installed mouse hook token.
        type MouseHook;
        /// Install the keyboard hook with the latest policy (`Err` = retry).
        fn install_keyboard(
            &mut self,
            config: KeyboardConfig,
        ) -> Result<Self::KeyboardHook, String>;
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
        type KeyboardHook = windows_sys::Win32::UI::WindowsAndMessaging::HHOOK;
        type MouseHook = windows_sys::Win32::UI::WindowsAndMessaging::HHOOK;

        fn install_keyboard(
            &mut self,
            config: KeyboardConfig,
        ) -> Result<Self::KeyboardHook, String> {
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

        fn install_keyboard(
            &mut self,
            config: KeyboardConfig,
        ) -> Result<Self::KeyboardHook, String> {
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
                .spawn(move || {
                    run_pump(thread_channels, thread_signals, keyboard_config, RealHooks)
                })
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
        pub fn wait_ready(
            &mut self,
            timeout: Duration,
            on_event: &mut dyn FnMut(&FailEvent),
        ) -> bool {
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
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            let has = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
            if has == 0 {
                break;
            }
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    /// Wait for owner work, queued hook-callback messages, or the deadline.
    /// `u32::MAX` waits indefinitely: the work wake and the message queue
    /// still wake the pump; nothing polls. No locks are held across the wait
    /// (callers drain/apply outside callbacks with try-locks before and
    /// after, and transfer classifier work before blocking). Returns the
    /// native wait reason; failed/unrecognized codes classify as `Failed`
    /// with no behavior change (the step keeps its existing fallthrough).
    fn wait_messages(timeout_ms: u32, work_wake: Option<&OwnedHandle>) -> super::InputWaitReason {
        let has_work = work_wake.is_some();
        let result = unsafe {
            match work_wake {
                Some(wake) => {
                    let raw = wake.as_handle().as_raw_handle();
                    MsgWaitForMultipleObjectsEx(1, &raw, timeout_ms, QS_ALLINPUT, 0)
                }
                None => {
                    MsgWaitForMultipleObjectsEx(0, std::ptr::null(), timeout_ms, QS_ALLINPUT, 0)
                }
            }
        };
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
                }
                None => self.contention = true,
            }
            let now = Instant::now();
            self.install_due(now);
            self.refresh_mouse_gate();
            let pre_start = Instant::now();
            self.backend.drain_messages();
            self.offer_batches();
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
    mod core_tests {
        use super::*;
        use crate::input_handoff::{
            InboxItem, InputSnapshot, SharedSnapshot, TaggedBatch, WindragOrigin, global_esc_clock,
            global_windrag_esc_clock,
        };
        use crate::snapkey::{KeyboardConfig, SnapClassify, SnapOrigin};
        use std::collections::HashMap;

        fn test_channels() -> (Arc<InputChannels>, Arc<PumpSignals>) {
            let channels = Arc::new(InputChannels::new());
            let (tx, _rx) = sync_channel(16);
            (Arc::clone(&channels), Arc::new(PumpSignals::new(tx)))
        }

        fn snapshot_with(policy_generation: u64, takeover: bool) -> SharedSnapshot {
            Arc::new(InputSnapshot {
                generation: policy_generation,
                keyboard: KeyboardConfig {
                    takeover,
                    allow_win_l: false,
                },
                remap: Vec::new(),
                disabled: Vec::new(),
                snap_origins: HashMap::new(),
                windrag_origins: HashMap::new(),
                active: takeover,
                trace: false,
            })
        }

        #[test]
        fn install_retry_reports_then_readies_on_both_hooks() {
            // Keyboard fails twice, mouse succeeds: no readiness until both
            // install; every failure is reported with its backoff.
            let (channels, signals) = test_channels();
            let backend =
                super::super::FakeBackend::new(vec![false, false, true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            // First turn: both install attempts run (keyboard fails).
            assert!(!core.step());
            assert!(!signals.is_ready());
            assert_eq!(core.keyboard_failures, 1);
            assert!(core.mouse_hook.is_some());
            // Backoff (5s) blocks immediate retry: force due.
            core.keyboard_retry_at = Some(Instant::now());
            assert!(!core.step());
            assert_eq!(core.keyboard_failures, 2);
            core.keyboard_retry_at = Some(Instant::now());
            assert!(!core.step());
            assert!(core.keyboard_hook.is_some());
            assert!(signals.is_ready());
        }

        #[test]
        fn retry_uses_latest_snapshot_policy_not_spawn_config() {
            // A policy snapshot arriving while install keeps failing must be
            // the config of the eventual install, never the stale spawn init.
            // Serial guard: the step applies generations to the
            // process-global fence, which races guarded fence tests.
            let _guard = crate::input_handoff::fence_test_serial();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![false, true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig::disabled(),
            );
            assert!(!core.step());
            assert!(core.keyboard_hook.is_none());
            channels
                .inbox
                .push(InboxItem::Snapshot(snapshot_with(1, true)))
                .expect("fits");
            core.keyboard_retry_at = Some(Instant::now());
            assert!(!core.step());
            assert!(core.keyboard_hook.is_some());
            assert!(core.backend.keyboard_configs.contains(&KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            }));
        }

        #[test]
        fn inbox_controls_apply_in_fifo_order() {
            // Snapshot, invalidate, snapshot through the real step with
            // hook-free TLS fixtures (no hooks, no native calls): the middle
            // control must apply between the two policies. Serial guard:
            // the step applies generations to the process-global fence.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            assert!(!core.step());
            let mut first = snapshot_with(1, true);
            Arc::get_mut(&mut first)
                .expect("unique")
                .snap_origins
                .insert(
                    7,
                    crate::snapkey::SnapOrigin {
                        hwnd: 7,
                        token: "t".to_owned(),
                        pid: 1,
                        creation: "c".to_owned(),
                    },
                );
            channels
                .inbox
                .push(InboxItem::Snapshot(first))
                .expect("fits");
            channels
                .inbox
                .push(InboxItem::InvalidateWindrag(7))
                .expect("fits");
            channels
                .inbox
                .push(InboxItem::Snapshot(snapshot_with(2, false)))
                .expect("fits");
            assert!(!core.step());
            // Applied keyboard origins lost hwnd 7 while the second
            // snapshot's changed policy (takeover off) applied; the mouse
            // snapshot dropped it too.
            assert!(!crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
            assert!(
                !crate::win_mouse::sys::test_snapshot()
                    .windrag_origins
                    .contains_key(&7)
            );
            // Policy generations advanced in order; the invalidated origin
            // never rebound through the later snapshot.
            assert_eq!(
                channels
                    .applied_policy_gen
                    .load(std::sync::atomic::Ordering::SeqCst),
                2
            );
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn retained_snapshot_reapplies_after_initial_install() {
            // Snapshot applied while TLS is absent (install pending), then
            // both hooks install through the real step: the retained snapshot
            // must restore origins/gates onto the fresh empty hook state.
            // Without the reapply the installs would leave empty state with
            // change-gated publication never republishing it. Serial guard:
            // the step applies generations to the process-global fence.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig::disabled(),
            );
            let mut snap = snapshot_with(11, true);
            Arc::get_mut(&mut snap)
                .expect("unique")
                .snap_origins
                .insert(
                    7,
                    SnapOrigin {
                        hwnd: 7,
                        token: "t".to_owned(),
                        pid: 1,
                        creation: "c".to_owned(),
                    },
                );
            Arc::get_mut(&mut snap)
                .expect("unique")
                .windrag_origins
                .insert(
                    7,
                    WindragOrigin {
                        origin: SnapOrigin {
                            hwnd: 7,
                            token: "t".to_owned(),
                            pid: 1,
                            creation: "c".to_owned(),
                        },
                        tag: Some("tag-7".to_owned()),
                    },
                );
            channels
                .inbox
                .push(InboxItem::Snapshot(snap))
                .expect("fits");
            assert!(!core.step());
            assert!(core.keyboard_hook.is_some());
            assert!(core.mouse_hook.is_some());
            // The install used the retained policy, not the stale spawn init.
            assert!(core.backend.keyboard_configs.contains(&KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            }));
            assert!(crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
            let mouse = crate::win_mouse::sys::test_snapshot();
            assert!(mouse.windrag_origins.contains_key(&7));
            assert!(mouse.active);
            assert_eq!(
                channels
                    .applied_policy_gen
                    .load(std::sync::atomic::Ordering::SeqCst),
                11
            );
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn retained_snapshot_reapplies_after_partial_install_failure() {
            // Keyboard installs while mouse keeps failing; the later mouse
            // recovery through the real step must still restore the retained
            // mouse snapshot (origins plus gate). Serial guard: the step
            // applies generations to the process-global fence.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![false, true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig::disabled(),
            );
            let mut snap = snapshot_with(21, true);
            Arc::get_mut(&mut snap)
                .expect("unique")
                .windrag_origins
                .insert(
                    7,
                    WindragOrigin {
                        origin: SnapOrigin {
                            hwnd: 7,
                            token: "t".to_owned(),
                            pid: 1,
                            creation: "c".to_owned(),
                        },
                        tag: Some("tag-7".to_owned()),
                    },
                );
            channels
                .inbox
                .push(InboxItem::Snapshot(snap))
                .expect("fits");
            assert!(!core.step());
            assert!(core.keyboard_hook.is_some());
            assert!(core.mouse_hook.is_none());
            assert_eq!(core.mouse_failures, 1);
            // Mouse retry due now: recovery installs and reapplies.
            core.mouse_retry_at = Some(Instant::now());
            assert!(!core.step());
            assert!(core.mouse_hook.is_some());
            assert!(signals.is_ready());
            let mouse = crate::win_mouse::sys::test_snapshot();
            assert!(mouse.windrag_origins.contains_key(&7));
            assert!(mouse.active);
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn clear_forgets_retained_then_forced_same_snapshot_restores() {
            // Clear drops both hook origin maps and the retention; the
            // owner's forced republish of the UNCHANGED desired snapshot
            // (same content, same generation: no policy change, no bump)
            // restores origins through the real step. Without the force the
            // cleared origins would stay absent forever under change-gated
            // publication. Serial guard: the step applies generations to the
            // process-global fence.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            assert!(!core.step());
            let mut snap = snapshot_with(5, true);
            let origin = SnapOrigin {
                hwnd: 7,
                token: "t".to_owned(),
                pid: 1,
                creation: "c".to_owned(),
            };
            Arc::get_mut(&mut snap)
                .expect("unique")
                .snap_origins
                .insert(7, origin.clone());
            Arc::get_mut(&mut snap)
                .expect("unique")
                .windrag_origins
                .insert(
                    7,
                    WindragOrigin {
                        origin,
                        tag: Some("tag-7".to_owned()),
                    },
                );
            channels
                .inbox
                .push(InboxItem::Snapshot(Arc::clone(&snap)))
                .expect("fits");
            assert!(!core.step());
            assert!(crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
            assert!(
                crate::win_mouse::sys::test_snapshot()
                    .windrag_origins
                    .contains_key(&7)
            );
            channels.inbox.push(InboxItem::ClearWindrag).expect("fits");
            assert!(!core.step());
            assert!(crate::snapkey::sys::test_keyboard_origins().is_empty());
            assert!(
                crate::win_mouse::sys::test_snapshot()
                    .windrag_origins
                    .is_empty()
            );
            // Forced republish of the unchanged snapshot restores both maps.
            channels
                .inbox
                .push(InboxItem::Snapshot(snap))
                .expect("fits");
            assert!(!core.step());
            assert!(crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
            assert!(
                crate::win_mouse::sys::test_snapshot()
                    .windrag_origins
                    .contains_key(&7)
            );
            assert_eq!(
                channels
                    .applied_policy_gen
                    .load(std::sync::atomic::Ordering::SeqCst),
                5
            );
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn install_with_queued_invalidate_does_not_rearm() {
            // Snapshot and invalidation queued before the hooks exist: the
            // step applies to absent TLS, installs empty hook state, then
            // reapplies the retention. The retention must have forgotten the
            // invalidated origin, or the install would rearm it. Serial
            // guard: the step applies generations to the process-global
            // fence.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig::disabled(),
            );
            let mut snap = snapshot_with(40, true);
            let origin = SnapOrigin {
                hwnd: 7,
                token: "t".to_owned(),
                pid: 1,
                creation: "c".to_owned(),
            };
            Arc::get_mut(&mut snap)
                .expect("unique")
                .snap_origins
                .insert(7, origin.clone());
            Arc::get_mut(&mut snap)
                .expect("unique")
                .windrag_origins
                .insert(
                    7,
                    WindragOrigin {
                        origin,
                        tag: Some("tag-7".to_owned()),
                    },
                );
            channels
                .inbox
                .push(InboxItem::Snapshot(snap))
                .expect("fits");
            channels
                .inbox
                .push(InboxItem::InvalidateWindrag(7))
                .expect("fits");
            assert!(!core.step());
            assert!(core.keyboard_hook.is_some());
            assert!(core.mouse_hook.is_some());
            assert!(!crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
            assert!(
                !crate::win_mouse::sys::test_snapshot()
                    .windrag_origins
                    .contains_key(&7)
            );
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn keyboard_recovers_without_rearming_invalidated_origin() {
            // Keyboard fails while the mouse installs (gate closed, no mask
            // source), an invalidation lands, then the keyboard recovers:
            // the mouse gate must reinstall in the recovered direction
            // (closed -> open) from the retention without rearming the
            // invalidated origin on either hook. Serial guard: the step
            // applies generations to the process-global fence.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![false, true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig::disabled(),
            );
            let mut snap = snapshot_with(9, true);
            let origin = SnapOrigin {
                hwnd: 7,
                token: "t".to_owned(),
                pid: 1,
                creation: "c".to_owned(),
            };
            Arc::get_mut(&mut snap)
                .expect("unique")
                .snap_origins
                .insert(7, origin.clone());
            Arc::get_mut(&mut snap)
                .expect("unique")
                .windrag_origins
                .insert(
                    7,
                    WindragOrigin {
                        origin,
                        tag: Some("tag-7".to_owned()),
                    },
                );
            channels
                .inbox
                .push(InboxItem::Snapshot(snap))
                .expect("fits");
            assert!(!core.step());
            assert!(core.keyboard_hook.is_none());
            assert!(core.mouse_hook.is_some());
            // Mouse installed without the keyboard: origins present but the
            // fresh-Down gate closed.
            assert!(
                crate::win_mouse::sys::test_snapshot()
                    .windrag_origins
                    .contains_key(&7)
            );
            assert!(!crate::win_mouse::sys::test_keyboard_installed());
            channels
                .inbox
                .push(InboxItem::InvalidateWindrag(7))
                .expect("fits");
            core.keyboard_retry_at = Some(Instant::now());
            assert!(!core.step());
            assert!(core.keyboard_hook.is_some());
            // Recovery installed the retained policy, not the stale spawn
            // init, and neither hook rearmed the invalidated origin.
            assert!(core.backend.keyboard_configs.contains(&KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            }));
            assert!(!crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
            assert!(
                !crate::win_mouse::sys::test_snapshot()
                    .windrag_origins
                    .contains_key(&7)
            );
            // The mouse gate recovered with the keyboard (closed -> open).
            assert!(crate::win_mouse::sys::test_keyboard_installed());
            assert!(crate::win_mouse::sys::test_snapshot().active);
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn revert_snapshot_advances_ack_with_equal_content() {
            // Owner revert after a refused policy change reuses the armed
            // generation with content equal to the applied policy: the ack
            // must still advance, or the fence would stay pending forever
            // while the classifier is already back on the desired policy.
            // Serial guard: the step applies generations to the
            // process-global fence.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            assert!(!core.step());
            channels
                .inbox
                .push(InboxItem::Snapshot(snapshot_with(5, true)))
                .expect("fits");
            assert!(!core.step());
            assert_eq!(
                channels
                    .applied_policy_gen
                    .load(std::sync::atomic::Ordering::SeqCst),
                5
            );
            // Revert: same policy content as applied, reused armed gen 6.
            channels
                .inbox
                .push(InboxItem::Snapshot(snapshot_with(6, true)))
                .expect("fits");
            assert!(!core.step());
            assert_eq!(
                channels
                    .applied_policy_gen
                    .load(std::sync::atomic::Ordering::SeqCst),
                6
            );
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn stalled_overflow_escalates_and_resumes_through_pump_seam() {
            // Production sender plus production pump step: a stalled
            // transport absorbing far more distinct invalidations than the
            // fixed cap escalates to a clear the pump applies (origins
            // dropped on both hooks, latched suspend clears applied), then a
            // resume snapshot restores; the cycle repeats. Proves bounded
            // lossless delivery end to end: no rearm of obsolete origins, no
            // lost suspend clears, fixed slot state throughout. Serial
            // guard: the step applies generations (and `ClearEsc` global
            // edges) process-globally, racing fence/edge tests.
            let _guard = crate::input_handoff::fence_test_serial();
            use crate::input_handoff::INBOX_CAP;
            use crate::snapkey::VK_ESCAPE;
            use crate::snapkey::sys::{take_esc_edge, take_windrag_esc_edge, test_drive_llproc};
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            assert!(!core.step());
            assert!(signals.is_ready());
            let mut sender = super::super::InboxSender::new(Arc::clone(&channels.inbox));
            for cycle in 0..2u64 {
                // Publish origins and apply through the real step (the resume
                // path: in production the owner's change-gated flow rebuilds
                // after managed-set churn, which is also what produced the
                // invalidations, so fresher snapshots rearm only live
                // origins and never obsolete ones).
                let mut snap = snapshot_with(100 + cycle, true);
                for hwnd in [7u64, 8] {
                    let origin = SnapOrigin {
                        hwnd,
                        token: "t".to_owned(),
                        pid: 1,
                        creation: "c".to_owned(),
                    };
                    Arc::get_mut(&mut snap)
                        .expect("unique")
                        .snap_origins
                        .insert(hwnd, origin.clone());
                    Arc::get_mut(&mut snap)
                        .expect("unique")
                        .windrag_origins
                        .insert(
                            hwnd,
                            WindragOrigin {
                                origin,
                                tag: Some("tag".to_owned()),
                            },
                        );
                }
                channels
                    .inbox
                    .push(InboxItem::Snapshot(snap))
                    .expect("fits");
                assert!(!core.step());
                assert!(crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
                assert!(
                    crate::win_mouse::sys::test_snapshot()
                        .windrag_origins
                        .contains_key(&8)
                );
                // Latch suspend-clear edges through the real callback, prove
                // latched, re-latch for the cycle.
                unsafe {
                    test_drive_llproc(VK_ESCAPE, 0, 0, 0);
                }
                assert!(take_esc_edge());
                assert!(take_windrag_esc_edge());
                unsafe {
                    test_drive_llproc(VK_ESCAPE, 0, 0, 0);
                }
                // Stall with harmless idempotent filler (unknown HWNDs touch
                // nothing), then churn far past the fixed cap plus the
                // suspend-clear obligation.
                for n in 0..INBOX_CAP {
                    channels
                        .inbox
                        .push(InboxItem::InvalidateWindrag(900 + n as u64))
                        .expect("fits");
                }
                sender
                    .offer(InboxItem::InvalidateWindrag(500 + cycle))
                    .expect("buffered");
                for hwnd in 0..40u64 {
                    sender
                        .offer(InboxItem::InvalidateWindrag(2000 + cycle * 100 + hwnd))
                        .expect("absorbed");
                }
                sender.offer(InboxItem::ClearEsc).expect("absorbed");
                assert!(sender.safety_len() <= super::super::SAFETY_INVALIDATES_CAP + 2);
                // Drain through real pump steps interleaved with sender
                // retries; ends with a step so the inbox is fully applied.
                for _ in 0..16 {
                    sender.flush();
                    assert!(!core.step());
                }
                assert_eq!(sender.pending_len(), 0);
                assert_eq!(sender.safety_len(), 0);
                assert!(!sender.admission_closed);
                assert!(channels.inbox.is_empty());
                // Escalated clear applied: origins dropped on both hooks...
                assert!(crate::snapkey::sys::test_keyboard_origins().is_empty());
                assert!(
                    crate::win_mouse::sys::test_snapshot()
                        .windrag_origins
                        .is_empty()
                );
                // ...and the latched suspend clears applied, not lost.
                assert!(!take_esc_edge());
                assert!(!take_windrag_esc_edge());
            }
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn policy_change_drains_stale_once_with_ack() {
            // A real policy change through the step drains the classifier
            // queue exactly once and publishes the ack generation + count.
            // Serial guard: the step applies generations to the
            // process-global fence.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            assert!(!core.step());
            channels
                .inbox
                .push(InboxItem::Snapshot(snapshot_with(1, true)))
                .expect("fits");
            assert!(!core.step());
            assert_eq!(
                channels
                    .applied_policy_gen
                    .load(std::sync::atomic::Ordering::SeqCst),
                1
            );
            // Map-only republish (same policy): no drain, generation held.
            channels
                .inbox
                .push(InboxItem::Snapshot(snapshot_with(1, true)))
                .expect("fits");
            assert!(!core.step());
            assert_eq!(
                channels
                    .applied_policy_gen
                    .load(std::sync::atomic::Ordering::SeqCst),
                1
            );
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn pending_stall_preserves_tagged_terminal_order() {
            // Terminal Up queued behind an occupied outbox waits in the real
            // queue and arrives tagged after release; a parked batch keeps
            // its original generation when it finally offers. Serial guard:
            // the step applies generations to the process-global fence.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            assert!(!core.step());
            channels
                .inbox
                .push(InboxItem::Snapshot(snapshot_with(1, true)))
                .expect("fits");
            assert!(!core.step());
            channels
                .windrag_outbox
                .put(TaggedBatch::new(
                    1,
                    vec![crate::win_mouse::WinDragEdge {
                        hwnd: 1,
                        kind: crate::win_mouse::WinDragKind::Down,
                        x: 0,
                        y: 0,
                        esc_seq: 0,
                        snapshot: None,
                    }],
                ))
                .expect("occupy");
            crate::win_mouse::sys::test_push_edge(crate::win_mouse::WinDragEdge {
                hwnd: 7,
                kind: crate::win_mouse::WinDragKind::Up,
                x: 2,
                y: 3,
                esc_seq: 1,
                snapshot: None,
            });
            // Occupied: the terminal Up waits in the real queue.
            assert!(!core.step());
            assert!(core.pending_windrag.is_none());
            channels.windrag_outbox.take().expect("release");
            assert!(!core.step());
            let batch = channels.windrag_outbox.take().expect("offered");
            assert_eq!(batch.generation, 1);
            assert_eq!(batch.items.len(), 1);
            assert_eq!(batch.items[0].kind, crate::win_mouse::WinDragKind::Up);
            // Parked batches keep their drain-time generation on retry.
            core.pending_snap = Some(TaggedBatch::new(1, Vec::new()));
            channels
                .snap_outbox
                .put(TaggedBatch::new(1, Vec::new()))
                .expect("occupy");
            assert!(!core.step());
            assert!(core.pending_snap.is_some());
            channels.snap_outbox.take().expect("release");
            assert!(!core.step());
            assert!(core.pending_snap.is_none());
            let batch = channels.snap_outbox.take().expect("offered");
            assert_eq!(batch.generation, 1);
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn stop_unhook_failure_retries_bounded_then_reports_unclean() {
            // Permanent unhook failure: bounded creator-side retry, then an
            // unclean terminal report (never claims clean, never hangs).
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            assert!(!core.step());
            channels
                .stop_flag
                .store(true, std::sync::atomic::Ordering::SeqCst);
            core.backend.unhook_script.clear();
            for _ in 0..4 {
                core.backend.unhook_script.push_back(false);
            }
            let report = core.shutdown();
            assert!(!report.clean && !report.unhook_ok);
        }

        /// Test-only owner handle over a fake pump thread: the thread runs
        /// no hooks, installs nothing, and only returns a [`StopReport`]
        /// (or blocks/panics as scripted), so stop-seam behavior is
        /// exercised with zero native effects.
        fn test_handle(thread: Option<std::thread::JoinHandle<StopReport>>) -> InputHandle {
            let (fail_tx, fail_rx) = sync_channel(16);
            let mut handle = InputHandle {
                channels: Arc::new(InputChannels::new()),
                signals: Arc::new(PumpSignals::new(fail_tx)),
                failures: fail_rx,
                lifecycle: PumpLifecycle::new(),
                thread,
                stop_report: None,
                keyboard_config: KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            };
            handle.lifecycle.start().expect("fresh handle starts");
            handle.request_stop().expect("stop requested");
            handle
        }

        #[test]
        fn second_wait_returns_cached_report_without_panic() {
            // Terminal waits are idempotent: a second bounded wait returns
            // the stored report instead of joining a consumed handle.
            let _serial = crate::input_handoff::pump_slot_test_serial();
            let thread = std::thread::Builder::new()
                .name("fake-input-stop".to_owned())
                .spawn(|| StopReport {
                    unhook_ok: true,
                    clean: true,
                })
                .expect("fake thread spawns");
            let mut handle = test_handle(Some(thread));
            handle.signals.note_stopped(true);
            let first = handle.wait_stopped(Duration::from_secs(10));
            assert!(first.clean && first.unhook_ok);
            let second = handle.wait_stopped(Duration::from_secs(10));
            assert_eq!(second, first);
            crate::input_handoff::pump_slot_release();
        }

        #[test]
        fn ack_with_stalled_thread_stays_bounded_then_completes() {
            // An ack published before the thread finishes must not block
            // the owner past the bound: the handle is retained and the
            // call reports unclean; a later wait still joins cleanly.
            let _serial = crate::input_handoff::pump_slot_test_serial();
            let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
            let thread = std::thread::Builder::new()
                .name("fake-input-stall".to_owned())
                .spawn(move || {
                    let _ = gate_rx.recv();
                    StopReport {
                        unhook_ok: true,
                        clean: true,
                    }
                })
                .expect("fake thread spawns");
            let mut handle = test_handle(Some(thread));
            handle.signals.note_stopped(true);
            let started = Instant::now();
            let first = handle.wait_stopped(Duration::from_millis(100));
            let elapsed = started.elapsed();
            assert!(!first.clean, "stalled thread must not report clean");
            assert!(
                elapsed < Duration::from_secs(10),
                "stop wait stayed bounded, took {elapsed:?}"
            );
            assert!(
                handle.thread.is_some(),
                "stalled handle retained for a later wait"
            );
            drop(gate_tx);
            let second = handle.wait_stopped(Duration::from_secs(10));
            assert!(second.clean && second.unhook_ok);
            crate::input_handoff::pump_slot_release();
        }

        #[test]
        fn join_panic_reports_unclean_not_clean() {
            // A thread that acks then panics must not report clean from
            // the ack alone: the failed join folds into the report.
            let _serial = crate::input_handoff::pump_slot_test_serial();
            let (fail_tx, fail_rx) = sync_channel(16);
            let signals = Arc::new(PumpSignals::new(fail_tx));
            let thread_signals = Arc::clone(&signals);
            let thread = std::thread::Builder::new()
                .name("fake-input-panic".to_owned())
                .spawn(move || -> StopReport {
                    thread_signals.note_stopped(true);
                    panic!("simulated pump panic");
                })
                .expect("fake thread spawns");
            let mut handle = InputHandle {
                channels: Arc::new(InputChannels::new()),
                signals,
                failures: fail_rx,
                lifecycle: PumpLifecycle::new(),
                thread: Some(thread),
                stop_report: None,
                keyboard_config: KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            };
            handle.lifecycle.start().expect("fresh handle starts");
            handle.request_stop().expect("stop requested");
            let report = handle.wait_stopped(Duration::from_secs(10));
            assert!(
                !report.clean,
                "panicked thread must not report clean, got {report:?}"
            );
            assert!(
                handle.lifecycle.state() == crate::input_handoff::PumpState::Stopping,
                "unclean stop retains Stopping, never frees the slot"
            );
            crate::input_handoff::pump_slot_release();
        }

        #[test]
        fn failed_ack_retains_slot_and_refuses_overlap() {
            // A failed unhook ack reports unclean and keeps the process
            // slot, so no overlapping worker can start; a clean stop
            // releases it.
            let _serial = crate::input_handoff::pump_slot_test_serial();
            assert!(!crate::input_handoff::pump_slot_reserved());
            assert!(crate::input_handoff::pump_slot_try_reserve());
            let thread = std::thread::Builder::new()
                .name("fake-input-fail".to_owned())
                .spawn(|| StopReport {
                    unhook_ok: false,
                    clean: false,
                })
                .expect("fake thread spawns");
            let mut handle = test_handle(Some(thread));
            handle.signals.note_stopped(false);
            let report = handle.wait_stopped(Duration::from_secs(10));
            assert!(!report.clean && !report.unhook_ok);
            assert!(
                crate::input_handoff::pump_slot_reserved(),
                "unclean stop retains the slot"
            );
            assert!(
                !crate::input_handoff::pump_slot_try_reserve(),
                "no overlapping worker while retained"
            );
            crate::input_handoff::pump_slot_release();
            assert!(crate::input_handoff::pump_slot_try_reserve());
            let thread = std::thread::Builder::new()
                .name("fake-input-clean".to_owned())
                .spawn(|| StopReport {
                    unhook_ok: true,
                    clean: true,
                })
                .expect("fake thread spawns");
            let mut handle = test_handle(Some(thread));
            handle.signals.note_stopped(true);
            let report = handle.wait_stopped(Duration::from_secs(10));
            assert!(report.clean && report.unhook_ok);
            assert!(
                !crate::input_handoff::pump_slot_reserved(),
                "clean stop releases the slot"
            );
        }

        #[test]
        fn missing_ack_with_failed_thread_reports_unclean() {
            // No ack, already-finished thread: the bounded wait joins and
            // folds the thread's own unhook failure, never claiming clean.
            let _serial = crate::input_handoff::pump_slot_test_serial();
            let thread = std::thread::Builder::new()
                .name("fake-input-noack".to_owned())
                .spawn(|| StopReport {
                    unhook_ok: false,
                    clean: false,
                })
                .expect("fake thread spawns");
            let mut handle = test_handle(Some(thread));
            let report = handle.wait_stopped(Duration::from_secs(10));
            assert!(!report.clean && !report.unhook_ok);
            crate::input_handoff::pump_slot_release();
        }

        #[test]
        fn esc_clocks_are_process_singletons() {
            // No per-handle mismatch possible: callbacks and the owner share
            // these exact atomics for the process lifetime (monotonic, so
            // parallel tests can only advance them further).
            let before = global_esc_clock().load();
            global_esc_clock().bump();
            global_esc_clock().bump();
            assert!(global_esc_clock().load() >= before.wrapping_add(2));
            let _ = SnapClassify::new(KeyboardConfig::disabled());
            let _ = global_windrag_esc_clock().load();
        }

        fn wake_signaled(channels: &InputChannels) -> bool {
            use std::os::windows::io::{AsHandle as _, AsRawHandle as _};
            let Some(handle) = channels.intent_wake() else {
                panic!("intent wake missing");
            };
            let raw = handle.as_handle().as_raw_handle();
            unsafe {
                windows_sys::Win32::System::Threading::WaitForSingleObject(raw, 0)
                    == windows_sys::Win32::Foundation::WAIT_OBJECT_0
            }
        }

        fn dummy_snap() -> crate::snapkey::QueuedSnapEvent {
            use crate::snapkey::{QueuedIntent, QueuedSnapEvent, SnapEdge, SnapOp};
            use tiler_core::directional::Direction;
            QueuedSnapEvent::Intent(QueuedIntent {
                op: SnapOp::Focus,
                direction: Direction::Left,
                edge: SnapEdge::Down,
                origin: None,
                consumed: true,
                announce: true,
                tick: Instant::now(),
            })
        }

        fn dummy_windrag() -> crate::win_mouse::WinDragEdge {
            crate::win_mouse::WinDragEdge {
                hwnd: 7,
                kind: crate::win_mouse::WinDragKind::Down,
                x: 1,
                y: 2,
                esc_seq: 0,
                snapshot: None,
            }
        }

        fn installed_core(
            channels: &Arc<InputChannels>,
            signals: &Arc<PumpSignals>,
        ) -> PumpCore<super::super::FakeBackend> {
            let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(channels),
                Arc::clone(signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            assert!(!core.step());
            assert!(core.keyboard_hook.is_some());
            assert!(core.mouse_hook.is_some());
            assert!(!wake_signaled(channels));
            core
        }

        #[test]
        fn intent_wake_created_nonsignaled() {
            // Lifecycle happy path: fresh channels carry a non-signaled
            // auto-reset wake (no spurious owner wake).
            let (channels, _signals) = test_channels();
            assert!(channels.intent_wake().is_some());
            assert!(!wake_signaled(&channels));
        }

        #[test]
        fn snap_offer_signals_single_publication() {
            // Publication: one accepted snap batch leaves one pending wake
            // for the owner wait; the first probe consumes it (auto-reset)
            // and the second finds nothing.
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
            assert!(!core.step());
            assert!(channels.snap_outbox.occupied());
            assert!(wake_signaled(&channels));
            assert!(!wake_signaled(&channels));
            channels.snap_outbox.take().expect("batch");
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn snap_and_windrag_offers_share_one_pending_wake() {
            // Coalescing: one turn offering both a snap and a windrag batch
            // occupies both outboxes but leaves a single pending wake.
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
            crate::win_mouse::sys::test_push_edge(dummy_windrag());
            assert!(!core.step());
            assert!(channels.snap_outbox.occupied());
            assert!(channels.windrag_outbox.occupied());
            assert!(wake_signaled(&channels));
            assert!(!wake_signaled(&channels));
            assert_eq!(channels.snap_outbox.take().expect("batch").items.len(), 1);
            assert_eq!(
                channels.windrag_outbox.take().expect("batch").items.len(),
                1
            );
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn take_republish_interleaving_delivers_next_wake() {
            // Interleaving: offer/wake, consume wake, take batch, publish
            // another batch, and the next wake plus data arrive. A notify
            // landing after the take (owner already drained) is a harmless
            // extra wake: the following drain finds no data and the signal
            // does not stick.
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
            assert!(!core.step());
            assert!(wake_signaled(&channels));
            assert_eq!(channels.snap_outbox.take().expect("batch").items.len(), 1);
            assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
            assert!(!core.step());
            assert!(wake_signaled(&channels));
            assert_eq!(channels.snap_outbox.take().expect("batch").items.len(), 1);
            // Take-before-notify: extra wake with empty outboxes drains to
            // nothing and leaves no stuck signal.
            channels.notify_intent();
            assert!(wake_signaled(&channels));
            assert!(channels.snap_outbox.take().is_none());
            assert!(!wake_signaled(&channels));
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn occupied_snap_waits_without_wake_and_retry_signals() {
            // Occupied outboxes skip draining (items wait in the real queue,
            // no wake); the drain after release wakes, as does a parked
            // pending retry once space frees.
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            channels
                .snap_outbox
                .put(TaggedBatch::new(0, vec![dummy_snap()]))
                .expect("occupy");
            assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
            assert!(!core.step());
            assert!(!wake_signaled(&channels));
            let filler = channels.snap_outbox.take().expect("release");
            assert_eq!(filler.items.len(), 1);
            assert!(!core.step());
            assert!(channels.snap_outbox.occupied());
            assert!(wake_signaled(&channels));
            let batch = channels.snap_outbox.take().expect("batch");
            assert_eq!(batch.items.len(), 1);
            // Thread-local parked batch: refused retry stays silent, the
            // retry after release wakes.
            core.pending_snap = Some(TaggedBatch::new(1, vec![dummy_snap()]));
            channels
                .snap_outbox
                .put(TaggedBatch::new(1, vec![dummy_snap()]))
                .expect("occupy");
            assert!(!core.step());
            assert!(core.pending_snap.is_some());
            assert!(!wake_signaled(&channels));
            channels.snap_outbox.take().expect("release");
            assert!(!core.step());
            assert!(core.pending_snap.is_none());
            assert!(wake_signaled(&channels));
            channels.snap_outbox.take().expect("batch");
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn windrag_offer_signals_while_diag_only_step_does_not() {
            // Mouse batches wake; a diagnostics-only turn never does.
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            crate::win_mouse::sys::test_push_edge(dummy_windrag());
            assert!(!core.step());
            assert!(channels.windrag_outbox.occupied());
            assert!(wake_signaled(&channels));
            channels.windrag_outbox.take().expect("batch");
            // Diagnostics publish every turn; with empty queues nothing wakes.
            assert!(!core.step());
            assert!(!wake_signaled(&channels));
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        fn work_signaled(channels: &InputChannels) -> bool {
            use std::os::windows::io::{AsHandle as _, AsRawHandle as _};
            let Some(handle) = channels.work_wake() else {
                panic!("work wake missing");
            };
            let raw = handle.as_handle().as_raw_handle();
            unsafe {
                windows_sys::Win32::System::Threading::WaitForSingleObject(raw, 0)
                    == windows_sys::Win32::Foundation::WAIT_OBJECT_0
            }
        }

        #[test]
        fn work_wake_created_nonsignaled() {
            // Owner-to-input direction: fresh channels carry a non-signaled
            // auto-reset work wake (no spurious pump wake).
            let (channels, _signals) = test_channels();
            assert!(channels.work_wake().is_some());
            assert!(!work_signaled(&channels));
        }

        #[test]
        fn wait_reason_splits_work_message_and_retry_timeout() {
            // Native return-code split (no desktop): work wake, queued
            // messages, and the genuine deadline (`WAIT_TIMEOUT` 258) map
            // distinctly; `WAIT_FAILED` and any unrecognized code count as
            // `Failed`, never merged into timeout.
            use super::super::{InputWaitReason, classify_input_wait};
            assert_eq!(classify_input_wait(0, true), InputWaitReason::Work);
            assert_eq!(classify_input_wait(1, true), InputWaitReason::Message);
            assert_eq!(classify_input_wait(258, true), InputWaitReason::Timeout);
            assert_eq!(classify_input_wait(0, false), InputWaitReason::Message);
            assert_eq!(classify_input_wait(258, false), InputWaitReason::Timeout);
            assert_eq!(
                classify_input_wait(0xFFFF_FFFF, true),
                InputWaitReason::Failed
            );
            assert_eq!(
                classify_input_wait(0xFFFF_FFFF, false),
                InputWaitReason::Failed
            );
            assert_eq!(classify_input_wait(7, true), InputWaitReason::Failed);
        }

        #[test]
        fn step_counts_native_waits_by_reason_with_turns_equal() {
            // Wait-return evidence through the production step: scripted
            // work/message/timeout/failed returns accumulate by reason and the
            // completed-turn count stays equal (no stop short-circuit here).
            // Timeout holds the genuine deadline only; failures stay separate.
            use super::super::InputWaitReason;
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            let base_turns = channels.diag.snapshot().turns;
            core.backend.wait_script.push_back(InputWaitReason::Work);
            core.backend.wait_script.push_back(InputWaitReason::Message);
            core.backend.wait_script.push_back(InputWaitReason::Timeout);
            core.backend.wait_script.push_back(InputWaitReason::Failed);
            assert!(!core.step());
            assert!(!core.step());
            assert!(!core.step());
            assert!(!core.step());
            let diag = channels.diag.snapshot();
            assert_eq!(diag.turns, base_turns + 4);
            assert_eq!(diag.waits, base_turns + 4);
            assert_eq!(diag.wait_work, 1);
            assert_eq!(diag.wait_msg, 1);
            // installed_core's setup turn plus the four scripted turns: the
            // setup wait defaults to timeout.
            assert_eq!(diag.wait_timeout, base_turns + 1);
            assert_eq!(diag.wait_failed, 1);
        }

        #[test]
        fn next_timer_idles_without_deadline_and_retries_on_deadline() {
            // Deadline evidence through the production step: fully installed
            // idles with `u32::MAX` (INFINITE); a failed install schedules
            // its backoff as the wait deadline; recovery idles again.
            let (channels, signals) = test_channels();
            let backend = super::super::FakeBackend::new(vec![false], vec![true], true);
            let mut core = PumpCore::new(
                backend,
                Arc::clone(&channels),
                Arc::clone(&signals),
                KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
            );
            assert!(!core.step());
            assert!(core.keyboard_hook.is_none());
            assert!(core.mouse_hook.is_some());
            let due = core.next_timer().expect("keyboard retry pending");
            let secs = due.saturating_duration_since(Instant::now()).as_secs();
            assert!(
                (9..=10).contains(&secs),
                "keyboard backoff is 10s after the first failure, got {secs}s"
            );
            let waited = core
                .backend
                .pump_timeouts
                .last()
                .copied()
                .expect("wait recorded");
            assert!(
                (9900..=10000).contains(&waited),
                "wait matches the retry deadline, got {waited}ms"
            );
            // Recovery installs and the pump idles with no deadline.
            core.keyboard_retry_at = Some(Instant::now());
            core.backend.keyboard_script.push_back(true);
            assert!(!core.step());
            assert!(core.keyboard_hook.is_some());
            assert_eq!(core.next_timer(), None);
            assert_eq!(
                core.backend.pump_timeouts.last().copied(),
                Some(u32::MAX),
                "fully idle waits indefinitely"
            );
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn publish_after_wait_race_applies_without_loss() {
            // Publish-after-wait race: a work signal offered before the pump
            // wait stays pending (auto-reset), so the inbox control the owner
            // just published is never lost behind an idle wait.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            channels.inbox.push(InboxItem::ClearEsc).expect("fits");
            channels.notify_work();
            assert!(work_signaled(&channels), "publish wakes the idle pump");
            assert!(!core.step());
            assert!(channels.inbox.is_empty(), "published control applied");
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn drain_wakes_parked_retry_after_backpressure() {
            // Saturation/backpressure: an occupied outbox parks the retry
            // silently; the owner drain wakes the pump so the retained event
            // does not strand behind an idle wait.
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            channels
                .snap_outbox
                .put(TaggedBatch::new(0, vec![dummy_snap()]))
                .expect("occupy");
            assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
            assert!(!core.step());
            assert!(!work_signaled(&channels), "no spurious wake while stalled");
            channels.snap_outbox.take().expect("owner drain");
            channels.notify_work();
            assert!(work_signaled(&channels), "drain wakes the pump");
            assert!(!core.step());
            assert!(channels.snap_outbox.occupied());
            channels.snap_outbox.take().expect("batch");
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn stop_wakes_idle_pump() {
            // Stop path: the flag plus the work signal (what `request_stop`
            // publishes) makes the next turn report stop promptly.
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            assert_eq!(core.next_timer(), None, "installed pump idles");
            channels
                .stop_flag
                .store(true, std::sync::atomic::Ordering::SeqCst);
            channels.notify_work();
            assert!(work_signaled(&channels), "stop wakes the idle pump");
            assert!(core.step(), "stop reports promptly");
        }

        #[test]
        fn pre_wait_callback_work_transfers_before_blocking() {
            // S1: callbacks delivered during the pre-wait drain queue
            // classifier work; the pre-wait transfer must offer it before
            // the wait blocks, or the final callback strands behind an idle
            // wait with nothing left to wake the pump. The fake asserts the
            // queues are empty at wait time, so the old drain-before-INFINITE
            // ordering fails loudly here with no external wake.
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            core.backend.assert_transferred_before_wait = true;
            core.backend.snap_on_drain.push(dummy_snap());
            core.backend.windrag_on_drain.push(dummy_windrag());
            assert!(!core.step());
            assert!(channels.snap_outbox.occupied());
            assert!(channels.windrag_outbox.occupied());
            assert_eq!(
                core.backend.pump_timeouts.last().copied(),
                Some(u32::MAX),
                "installed pump still idles after transferring"
            );
            channels.snap_outbox.take().expect("batch");
            channels.windrag_outbox.take().expect("batch");
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn inbox_contention_arms_bounded_retry_without_loss() {
            // S2: inbox `try_drain` WouldBlock (owner mid-offer) arms one
            // short retry instead of stranding; the control applies on the
            // next turn once released. Bounded queues preserved throughout.
            let _guard = crate::input_handoff::fence_test_serial();
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            channels.inbox.push(InboxItem::ClearEsc).expect("fits");
            {
                let _held = channels.inbox.test_lock_held();
                assert!(channels.inbox.try_drain().is_none());
                assert!(!core.step());
                let due = core.next_timer().expect("contention retry armed");
                assert!(
                    due.saturating_duration_since(Instant::now())
                        <= super::super::CONTENTION_RETRY + Duration::from_millis(50),
                    "retry is bounded and short, got {due:?}"
                );
                let waited = core
                    .backend
                    .pump_timeouts
                    .last()
                    .copied()
                    .expect("wait recorded");
                assert!(
                    waited <= super::super::CONTENTION_RETRY.as_millis() as u32,
                    "wait matches the contention retry, got {waited}ms"
                );
            }
            assert!(!core.step());
            assert!(channels.inbox.is_empty(), "control applied, never lost");
            assert_eq!(core.next_timer(), None, "clean turn clears the retry");
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }

        #[test]
        fn outbox_contention_parks_and_retries_without_loss() {
            // S2: outbox `try_occupied` WouldBlock on a free outbox (owner
            // mid-take) skips the drain but arms the retry; queued records
            // wait in the real queue under their own bound and offer next
            // turn. A parked pending batch under contention re-parks and
            // then offers; terminals never strand.
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            crate::win_mouse::sys::test_ensure_state();
            let (channels, signals) = test_channels();
            let mut core = installed_core(&channels, &signals);
            assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
            channels.snap_outbox.test_with_lock_held(|| {
                assert!(!core.step());
            });
            assert!(
                core.pending_snap.is_none(),
                "nothing drained, nothing parked"
            );
            assert!(
                core.next_timer().is_some(),
                "unknown occupancy arms the retry"
            );
            assert!(!core.step());
            assert!(channels.snap_outbox.occupied(), "retained event offered");
            // Parked pending under contention: re-parks, then offers.
            core.pending_snap = Some(TaggedBatch::new(1, vec![dummy_snap()]));
            channels.snap_outbox.test_with_lock_held(|| {
                assert!(!core.step());
            });
            assert!(core.pending_snap.is_some(), "contended retry re-parks");
            assert!(core.next_timer().is_some());
            channels.snap_outbox.take().expect("owner drain");
            assert!(!core.step());
            assert!(core.pending_snap.is_none(), "parked batch offered");
            assert_eq!(core.next_timer(), None, "clean turn clears the retry");
            channels.snap_outbox.take().expect("batch");
            crate::snapkey::sys::test_clear_state();
            crate::win_mouse::sys::test_clear_state();
        }
    }
}

#[cfg(windows)]
pub use native::{FailEvent, InputHandle, NativeHooks, PumpSignals};

/// Test-only real-core seam for the stage-3 combined stress: no production
/// visibility change (the `native` module stays private).
#[cfg(all(windows, test))]
pub(crate) use native::PumpCore;

#[cfg(test)]
mod tests {
    use super::{
        FakeBackend, InboxSender, SAFETY_INVALIDATES_CAP, install_backoff_secs, snap_batch_age_us,
    };
    use crate::input_handoff::{
        EscClock, Inbox, InboxItem, InputSnapshot, OUTBOX_BATCH_CAP, Outbox, PumpLifecycle,
        TaggedBatch, snapshot_content_eq, snapshot_policy_eq,
    };
    use crate::snapkey::{
        INTENT_QUEUE_CAP, KeyboardConfig, QueuedIntent, QueuedSnapEvent, SnapClassify, SnapEdge,
        SnapOp, SnapQueue, VK_H, VK_LWIN, classify_and_queue, win_up_mask_reserve,
    };
    use crate::win_mouse::{
        UpOutcome, WINDRAG_QUEUE_CAP, WinDragEdge, WinDragKind, WinDragQueue, WinHookArm,
    };
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tiler_core::directional::Direction;

    #[test]
    fn install_backoff_doubles_to_sixty() {
        assert_eq!(install_backoff_secs(0), 5);
        assert_eq!(install_backoff_secs(1), 10);
        assert_eq!(install_backoff_secs(2), 20);
        assert_eq!(install_backoff_secs(3), 40);
        assert_eq!(install_backoff_secs(4), 60);
        assert_eq!(install_backoff_secs(9), 60);
    }

    #[test]
    fn fake_backend_scripts_install_outcomes() {
        // Scripted outcomes plus the shared backoff vocabulary; the
        // production step drives these through `NativeHooks` (covered in
        // `core_tests`).
        let mut backend = FakeBackend::new(vec![false, false, true], vec![true], true);
        assert_eq!(backend.keyboard_script.len(), 3);
        assert_eq!(backend.mouse_script.len(), 1);
        assert!(backend.unhook_script.is_empty());
        assert_eq!(backend.note_retry(1), 10);
        assert_eq!(backend.note_retry(2), 20);
        assert_eq!(backend.retry_secs, vec![10, 20]);
    }

    #[test]
    fn sender_fault_closes_admission_and_retries_in_order() {
        let inbox = Arc::new(Inbox::new());
        let mut sender = InboxSender::new(Arc::clone(&inbox));
        for _ in 0..crate::input_handoff::INBOX_CAP {
            sender.offer(InboxItem::ClearEsc).expect("fits");
        }
        sender.flush();
        // Inbox full now: the next offer faults closed but is preserved.
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("buffered");
        assert!(sender.admission_closed);
        assert_eq!(sender.pending_len(), 1);
        // Safety obligations still merge while closed (lossless stall);
        // snapshots refuse for rebuild-and-retry later.
        sender.offer(InboxItem::ClearEsc).expect("slotted");
        assert_eq!(sender.safety_len(), 1);
        let rebuild = InputSnapshot {
            generation: 0,
            keyboard: KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            },
            remap: Vec::new(),
            disabled: Vec::new(),
            snap_origins: HashMap::new(),
            windrag_origins: HashMap::new(),
            active: true,
            trace: false,
        };
        assert!(
            sender
                .offer(InboxItem::Snapshot(Arc::new(rebuild)))
                .is_err()
        );
        // Owner drains one slot, flush retries the preserved control first,
        // then the slotted suspend-clear behind it.
        assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
        assert_eq!(sender.flush(), 0);
        assert!(!sender.admission_closed);
        assert_eq!(
            inbox.drain(),
            vec![InboxItem::InvalidateWindrag(7), InboxItem::ClearEsc]
        );
    }

    #[test]
    fn sender_collapses_exact_duplicate_controls() {
        let inbox = Arc::new(Inbox::new());
        let mut sender = InboxSender::new(Arc::clone(&inbox));
        for _ in 0..crate::input_handoff::INBOX_CAP {
            sender.offer(InboxItem::ClearEsc).expect("fits");
        }
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("buffered");
        // Same control again collapses (preserved once, counted) even while
        // admission is closed.
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("deduped");
        assert_eq!(sender.pending_len(), 1);
        assert_eq!(sender.deduped, 1);
        // A different control while closed merges into the safety slots
        // (retained, never refused).
        sender
            .offer(InboxItem::InvalidateWindrag(9))
            .expect("slotted");
        assert_eq!(sender.pending_len(), 1);
        assert_eq!(sender.safety_len(), 1);
        // After the pump drains, the held control retries first and the
        // slotted one expands behind it in order.
        assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
        assert_eq!(sender.flush(), 0);
        assert!(!sender.admission_closed);
        assert_eq!(
            inbox.drain(),
            vec![
                InboxItem::InvalidateWindrag(7),
                InboxItem::InvalidateWindrag(9)
            ]
        );
    }

    #[test]
    fn sender_retains_distinct_safety_controls_under_stall() {
        // Stalled full transport keeps every distinct invalidation plus the
        // suspend-clear obligations: inbox full, pending held, slots merged,
        // all delivered in order once the pump drains.
        let inbox = Arc::new(Inbox::new());
        let mut sender = InboxSender::new(Arc::clone(&inbox));
        for hwnd in 100..108u64 {
            sender
                .offer(InboxItem::InvalidateWindrag(hwnd))
                .expect("fits inbox");
        }
        sender
            .offer(InboxItem::InvalidateWindrag(200))
            .expect("buffered pending");
        assert!(sender.admission_closed);
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("slotted");
        sender
            .offer(InboxItem::InvalidateWindrag(9))
            .expect("slotted");
        sender.offer(InboxItem::ClearEsc).expect("slotted");
        // Slot duplicates collapse without losing the obligation.
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("deduped");
        assert_eq!(sender.deduped, 1);
        assert_eq!(sender.safety_len(), 3);
        assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
        assert_eq!(sender.flush(), 0);
        assert!(!sender.admission_closed);
        assert_eq!(
            inbox.drain(),
            vec![
                InboxItem::InvalidateWindrag(200),
                InboxItem::InvalidateWindrag(7),
                InboxItem::InvalidateWindrag(9),
                InboxItem::ClearEsc,
            ]
        );
    }

    #[test]
    fn sender_escalates_to_clear_beyond_fixed_cap() {
        // Far more distinct invalidations than the fixed cap while stalled:
        // every offer absorbs (`Ok`), slot state stays bounded, and the
        // expansion carries the conservative clear (a proven superset of
        // every invalidation) instead of an unbounded HWND list.
        let inbox = Arc::new(Inbox::new());
        let mut sender = InboxSender::new(Arc::clone(&inbox));
        for hwnd in 100..108u64 {
            sender
                .offer(InboxItem::InvalidateWindrag(hwnd))
                .expect("fits inbox");
        }
        sender
            .offer(InboxItem::InvalidateWindrag(200))
            .expect("buffered pending");
        assert!(sender.admission_closed);
        for hwnd in 1000..1040u64 {
            sender
                .offer(InboxItem::InvalidateWindrag(hwnd))
                .expect("absorbed");
            assert!(sender.safety_len() <= SAFETY_INVALIDATES_CAP + 2);
        }
        assert!(sender.invalidates.len() <= SAFETY_INVALIDATES_CAP);
        assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
        assert_eq!(sender.flush(), 0);
        assert!(!sender.admission_closed);
        // Pending first, then the escalated clear, then at most a cap of
        // trailing invalidates queued after the last escalation. Exact
        // shape: 40 offers fill 8, escalate, fill 8, escalate, fill 8,
        // escalate, fill 8, escalate, then 4 remain.
        assert_eq!(
            inbox.drain(),
            vec![
                InboxItem::InvalidateWindrag(200),
                InboxItem::ClearWindrag,
                InboxItem::InvalidateWindrag(1036),
                InboxItem::InvalidateWindrag(1037),
                InboxItem::InvalidateWindrag(1038),
                InboxItem::InvalidateWindrag(1039),
            ]
        );
    }

    #[test]
    fn sender_clear_windrag_subsumes_older_slot_invalidates() {
        // A stalled `ClearWindrag` subsumes older slotted invalidates (every
        // gesture already disarmed with every origin dropped); a newer
        // invalidate behind the clear is still retained.
        let inbox = Arc::new(Inbox::new());
        let mut sender = InboxSender::new(Arc::clone(&inbox));
        for hwnd in 100..108u64 {
            sender
                .offer(InboxItem::InvalidateWindrag(hwnd))
                .expect("fits inbox");
        }
        sender
            .offer(InboxItem::InvalidateWindrag(200))
            .expect("buffered pending");
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("slotted");
        sender
            .offer(InboxItem::InvalidateWindrag(9))
            .expect("slotted");
        sender.offer(InboxItem::ClearWindrag).expect("slotted");
        sender
            .offer(InboxItem::InvalidateWindrag(11))
            .expect("slotted");
        // Duplicate clears collapse.
        sender.offer(InboxItem::ClearWindrag).expect("deduped");
        assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
        assert_eq!(sender.flush(), 0);
        assert_eq!(
            inbox.drain(),
            vec![
                InboxItem::InvalidateWindrag(200),
                InboxItem::ClearWindrag,
                InboxItem::InvalidateWindrag(11),
            ]
        );
    }

    #[test]
    fn snapshot_content_and_policy_compare_without_generation() {
        let base = InputSnapshot {
            generation: 1,
            keyboard: KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            },
            remap: Vec::new(),
            disabled: Vec::new(),
            snap_origins: HashMap::new(),
            windrag_origins: HashMap::new(),
            active: true,
            trace: false,
        };
        // Generation alone never counts as a change.
        let mut renumbered = base.clone();
        renumbered.generation = 2;
        assert!(snapshot_content_eq(&base, &renumbered));
        assert!(snapshot_policy_eq(&base, &renumbered));
        // Map-only drift: content differs, policy identical (no queue flush).
        let mut maps = base.clone();
        maps.windrag_origins.insert(
            7,
            crate::input_handoff::WindragOrigin {
                origin: crate::snapkey::SnapOrigin {
                    hwnd: 7,
                    token: "t".to_owned(),
                    pid: 1,
                    creation: "c".to_owned(),
                },
                tag: None,
            },
        );
        assert!(!snapshot_content_eq(&base, &maps));
        assert!(snapshot_policy_eq(&base, &maps));
        // Policy drift: takeover off changes both (stale drain required).
        let mut policy = base.clone();
        policy.keyboard.takeover = false;
        policy.active = false;
        assert!(!snapshot_content_eq(&base, &policy));
        assert!(!snapshot_policy_eq(&base, &policy));
    }

    #[test]
    fn sender_snapshot_invalidate_snapshot_stays_ordered() {
        let inbox = Arc::new(Inbox::new());
        let mut sender = InboxSender::new(Arc::clone(&inbox));
        let first = Arc::new(InputSnapshot {
            generation: 0,
            keyboard: KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            },
            remap: Vec::new(),
            disabled: Vec::new(),
            snap_origins: HashMap::new(),
            windrag_origins: HashMap::new(),
            active: true,
            trace: false,
        });
        let second = Arc::new(InputSnapshot {
            generation: 0,
            keyboard: KeyboardConfig::disabled(),
            remap: Vec::new(),
            disabled: Vec::new(),
            snap_origins: HashMap::new(),
            windrag_origins: HashMap::new(),
            active: false,
            trace: false,
        });
        assert!(
            sender
                .offer(InboxItem::Snapshot(Arc::clone(&first)))
                .is_ok()
        );
        assert!(sender.offer(InboxItem::InvalidateWindrag(7)).is_ok());
        assert!(
            sender
                .offer(InboxItem::Snapshot(Arc::clone(&second)))
                .is_ok()
        );
        sender.flush();
        let drained = inbox.drain();
        assert_eq!(drained.len(), 3);
        assert!(matches!(drained[0], InboxItem::Snapshot(_)));
        assert_eq!(drained[1], InboxItem::InvalidateWindrag(7));
        assert!(matches!(drained[2], InboxItem::Snapshot(_)));
    }

    #[test]
    fn failed_stop_never_frees_the_worker_slot() {
        let mut life = PumpLifecycle::new();
        life.start().expect("start");
        life.note_installed().expect("ready");
        life.request_stop().expect("stop");
        let report = life.note_unhooked(false).expect("ack");
        assert!(!report.clean);
        assert!(life.start().is_err());
    }

    #[test]
    fn mask_reentry_fires_after_config_change_without_rebind() {
        // Both hooks stay together: a consumed windrag Down arms the shared
        // keyboard mask; a config change between Down and Win-up must not
        // drop the mask obligation or rebind the held pair.
        let mut machine = SnapClassify::new(KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        });
        machine.set_gate_active(true);
        let mut queue = SnapQueue::new();
        assert_eq!(
            SnapClassify::push(&mut machine, VK_LWIN, false, true, false),
            None
        );
        machine.note_windrag_consumed();
        assert_eq!(
            classify_and_queue(
                &mut machine,
                &mut queue,
                VK_H,
                false,
                None,
                false,
                Instant::now(),
                true
            ),
            Some(true)
        );
        // Ordered config apply between Down and Win-up (input pump FIFO):
        // stale actions drain, held verdicts survive.
        machine.set_enabled(true);
        machine.set_remap(Vec::new());
        machine.set_disabled(Vec::new());
        queue.drain_stale();
        assert!(win_up_mask_reserve(
            &mut machine,
            &mut queue,
            VK_LWIN,
            true,
            Instant::now()
        ));
        assert_eq!(
            classify_and_queue(
                &mut machine,
                &mut queue,
                VK_H,
                true,
                None,
                false,
                Instant::now(),
                true
            ),
            Some(true)
        );
    }

    #[test]
    fn saturated_down_passes_and_terminal_up_survives_outbox_stall() {
        // Saturated new Down passes with nothing armed; the terminal Up of a
        // consumed gesture keeps guaranteed delivery end to end even while
        // the outbox is occupied.
        let queue = &mut WinDragQueue::new();
        for n in 0..WINDRAG_QUEUE_CAP {
            assert!(queue.push(WinDragEdge {
                hwnd: n as u64,
                kind: WinDragKind::Down,
                x: 0,
                y: 0,
                esc_seq: 0,
                snapshot: None,
            }));
        }
        // Saturated Down: pass-through, no arm, counted loss.
        assert!(!queue.push(WinDragEdge {
            hwnd: 999,
            kind: WinDragKind::Down,
            x: 0,
            y: 0,
            esc_seq: 0,
            snapshot: None,
        }));
        queue.note_drop();
        assert!(!WinHookArm::default().is_armed());
        // Consumed gesture elsewhere still closes with guaranteed delivery.
        let mut arm2 = WinHookArm::default();
        assert!(arm2.on_down(7, 1, 1, true, true));
        queue.push_terminal(WinDragEdge {
            hwnd: 7,
            kind: WinDragKind::Up,
            x: 2,
            y: 3,
            esc_seq: 1,
            snapshot: None,
        });
        let outbox: Outbox<TaggedBatch<WinDragEdge>> = Outbox::new();
        let mut first = Vec::new();
        while first.len() < OUTBOX_BATCH_CAP {
            first.push(queue.pop_front().expect("queued"));
        }
        outbox.put(TaggedBatch::new(2, first)).expect("offer");
        // Occupied: newer drains wait in the real queue; the terminal Up is
        // still queued behind, never stranded.
        let mut rest = Vec::new();
        while let Some(edge) = queue.pop_front() {
            rest.push(edge);
        }
        assert_eq!(
            rest.last(),
            Some(&WinDragEdge {
                hwnd: 7,
                kind: WinDragKind::Up,
                x: 2,
                y: 3,
                esc_seq: 1,
                snapshot: None,
            })
        );
        assert_eq!(
            arm2.on_up(2, 3),
            UpOutcome::Gesture(crate::win_mouse::WinBound {
                hwnd: 7,
                start_x: 1,
                start_y: 1,
                cur_x: 2,
                cur_y: 3,
            })
        );
    }

    #[test]
    fn esc_clock_orders_start_end_without_batch_delay() {
        let clock = EscClock::new();
        let start = clock.load();
        assert!(!crate::input_handoff::esc_edge_cancels(start, clock.load()));
        clock.bump();
        assert!(crate::input_handoff::esc_edge_cancels(start, clock.load()));
        let end_sample = clock.load();
        clock.bump();
        assert!(!crate::input_handoff::esc_edge_cancels(start, start));
        assert!(crate::input_handoff::esc_edge_cancels(start, end_sample));
    }

    #[test]
    fn snap_batch_age_uses_oldest_tick() {
        let now = Instant::now();
        let old = now - Duration::from_millis(120);
        let mk = |tick| {
            QueuedSnapEvent::Intent(QueuedIntent {
                op: SnapOp::Focus,
                direction: Direction::Left,
                edge: SnapEdge::Down,
                origin: None,
                consumed: true,
                announce: true,
                tick,
            })
        };
        let batch = vec![mk(now), mk(old)];
        let age = snap_batch_age_us(&batch, now);
        assert!((100_000..=500_000).contains(&age));
        assert_eq!(snap_batch_age_us(&[], now), 0);
    }

    #[test]
    fn snap_saturation_keeps_mask_end_to_end() {
        let mut machine = SnapClassify::new(KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        });
        machine.set_gate_active(true);
        let mut queue = SnapQueue::new();
        assert_eq!(
            SnapClassify::push(&mut machine, VK_LWIN, false, true, false),
            None
        );
        machine.note_windrag_consumed();
        for _ in 0..INTENT_QUEUE_CAP {
            assert!(queue.push(QueuedSnapEvent::Intent(QueuedIntent {
                op: SnapOp::Focus,
                direction: Direction::Left,
                edge: SnapEdge::Down,
                origin: None,
                consumed: true,
                announce: true,
                tick: Instant::now(),
            })));
        }
        assert!(win_up_mask_reserve(
            &mut machine,
            &mut queue,
            VK_LWIN,
            true,
            Instant::now()
        ));
        assert_eq!(queue.len(), INTENT_QUEUE_CAP);
        let outbox: Outbox<TaggedBatch<QueuedSnapEvent>> = Outbox::new();
        let mut batch = Vec::new();
        while batch.len() < OUTBOX_BATCH_CAP {
            batch.push(queue.pop_front().expect("queued"));
        }
        outbox.put(TaggedBatch::new(5, batch)).expect("offer");
        let taken = outbox.take().expect("take");
        assert_eq!(taken.generation, 5);
        assert!(!taken.is_stale(5));
        assert!(taken.is_stale(6));
    }

    #[test]
    fn sender_counts_accepted_clears_for_owner_republish() {
        // Every accepted clear (direct, buffered, slotted, escalated) ticks
        // the owner republish trigger; only duplicate collapses never tick
        // it. Direct re-accepts are idempotent clears that force again
        // (bounded, harmless); the owner clears its force on snapshot
        // accept.
        let inbox = Arc::new(Inbox::new());
        let mut sender = InboxSender::new(Arc::clone(&inbox));
        assert_eq!(sender.windrag_cleared(), 0);
        sender.offer(InboxItem::ClearWindrag).expect("direct");
        assert_eq!(sender.windrag_cleared(), 1);
        sender.offer(InboxItem::ClearWindrag).expect("re-accepted");
        assert_eq!(sender.windrag_cleared(), 2);
        // Stall, then slot a clear twice: the second collapses (counted,
        // never ticking) while the first ticks.
        for _ in 0..crate::input_handoff::INBOX_CAP - 2 {
            sender.offer(InboxItem::ClearEsc).expect("fits");
        }
        sender
            .offer(InboxItem::InvalidateWindrag(200))
            .expect("buffered");
        assert!(sender.admission_closed);
        let before = sender.windrag_cleared();
        sender.offer(InboxItem::ClearWindrag).expect("slotted");
        assert_eq!(sender.windrag_cleared(), before + 1);
        sender.offer(InboxItem::ClearWindrag).expect("deduped");
        assert_eq!(sender.windrag_cleared(), before + 1);
        assert_eq!(sender.deduped, 1);
        // Overflow past the fixed cap: escalation ticks again.
        for hwnd in 1000..1040u64 {
            sender
                .offer(InboxItem::InvalidateWindrag(hwnd))
                .expect("absorbed");
        }
        assert!(sender.windrag_cleared() > before + 1);
        assert!(sender.invalidates.len() <= SAFETY_INVALIDATES_CAP);
    }

    #[test]
    fn sender_preserves_newest_control_across_queued_snapshot() {
        // Pending [Inv7, Snap rearm7] plus a newer Inv7: the newer control
        // must be preserved (never collapsed), or the rearmed origin would
        // stay bound after its invalidation.
        let inbox = Arc::new(Inbox::new());
        let mut sender = InboxSender::new(Arc::clone(&inbox));
        for _ in 0..crate::input_handoff::INBOX_CAP {
            inbox.push(InboxItem::ClearEsc).expect("stall");
        }
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("buffered");
        // Admission is closed with pending held; reopen it in-test to stage
        // the intervening snapshot behind the held control (the ordering
        // the pump applies), then offer the newer invalidation.
        sender.admission_closed = false;
        let snap = Arc::new(InputSnapshot {
            generation: 0,
            keyboard: KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            },
            remap: Vec::new(),
            disabled: Vec::new(),
            snap_origins: HashMap::new(),
            windrag_origins: HashMap::new(),
            active: true,
            trace: false,
        });
        sender
            .offer(InboxItem::Snapshot(Arc::clone(&snap)))
            .expect("buffered behind");
        assert_eq!(sender.pending_len(), 2);
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("newest preserved");
        assert_eq!(sender.deduped, 0);
        assert_eq!(sender.pending_len(), 3);
        assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
        sender.flush();
        assert_eq!(
            inbox.drain(),
            vec![
                InboxItem::InvalidateWindrag(7),
                InboxItem::Snapshot(snap),
                InboxItem::InvalidateWindrag(7),
            ]
        );
    }

    #[test]
    fn sender_preserves_slotted_control_across_inbox_snapshot() {
        // Inbox [Snap rearm7] ahead plus slotted Inv7 behind must not
        // collapse a newer Inv7: the inbox snapshot rebinds between them.
        let inbox = Arc::new(Inbox::new());
        let mut sender = InboxSender::new(Arc::clone(&inbox));
        let snap = Arc::new(InputSnapshot {
            generation: 0,
            keyboard: KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            },
            remap: Vec::new(),
            disabled: Vec::new(),
            snap_origins: HashMap::new(),
            windrag_origins: HashMap::new(),
            active: true,
            trace: false,
        });
        sender
            .offer(InboxItem::Snapshot(Arc::clone(&snap)))
            .expect("queued");
        for _ in 0..crate::input_handoff::INBOX_CAP - 1 {
            sender.offer(InboxItem::ClearEsc).expect("fits");
        }
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("buffered");
        assert!(sender.admission_closed);
        sender
            .offer(InboxItem::InvalidateWindrag(9))
            .expect("slotted");
        // Newer Inv7 across the queued snapshot: preserved, not deduped.
        sender
            .offer(InboxItem::InvalidateWindrag(7))
            .expect("newest preserved");
        assert_eq!(sender.deduped, 0);
        assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
        assert_eq!(sender.flush(), 0);
        assert_eq!(
            inbox.drain(),
            vec![
                InboxItem::InvalidateWindrag(7),
                InboxItem::InvalidateWindrag(9),
                InboxItem::InvalidateWindrag(7),
            ]
        );
    }

    #[test]
    fn esc_interleave_callback_end_settle_orders_by_sequence() {
        // F8 interleaving fixture, portable: an Esc callback edge followed
        // by a WinEvent END before any pump forwarding must still cancel the
        // ended gesture (END sample newer than START), while a post-END Esc
        // must not rewrite the settled comparison.
        use crate::input_handoff::esc_edge_cancels;
        use std::sync::atomic::Ordering;
        // Serial guard: the global edge store/swap races a concurrent pump
        // `ClearEsc` apply (which stores false).
        let _guard = crate::input_handoff::fence_test_serial();
        let clock = EscClock::new();
        let start = clock.load();
        // Callback Esc: bump then END samples the new sequence.
        clock.bump();
        let end_sample = clock.load();
        assert!(esc_edge_cancels(start, end_sample));
        // Settle consumes the edge flag; a later Esc bumps again but the
        // ended comparison still uses the END sample, never the live clock.
        crate::input_handoff::global_esc_edge().store(true, Ordering::SeqCst);
        assert!(crate::input_handoff::global_esc_edge().swap(false, Ordering::SeqCst));
        clock.bump();
        assert!(!esc_edge_cancels(start, start));
        assert!(esc_edge_cancels(start, end_sample));
        assert!(!esc_edge_cancels(end_sample, end_sample));
    }
}
