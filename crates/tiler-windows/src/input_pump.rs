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
/// Input pump exchange cadence: inbox apply plus outbox offer between native
/// message pumps. Short enough to keep intents flowing, long enough to avoid
/// a busy loop. Hook callbacks deliver through the native message queue
/// regardless of this timeout.
pub const PUMP_QUANTUM_MS: u32 = 10;

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
    /// Message-pump turns requested.
    pub pump_turns: u32,
    /// Keyboard config seen at each install attempt (stale-init evidence).
    pub keyboard_configs: Vec<KeyboardConfig>,
    /// Retry backoffs recorded (evidence).
    pub retry_secs: Vec<u64>,
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
            keyboard_configs: Vec::new(),
            retry_secs: Vec::new(),
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
    use super::{InputChannels, PUMP_QUANTUM_MS, install_backoff_secs, snap_batch_age_us};
    use crate::input_handoff::{
        InboxItem, InputDiag, InputSnapshot, OUTBOX_BATCH_CAP, PumpLifecycle, SharedSnapshot,
        StopReport, TaggedBatch, note_policy_applied, snapshot_policy_eq, take_delivery_gap_ms,
    };
    use crate::snapkey::{KeyboardConfig, QueuedSnapEvent};
    use crate::win_mouse::WinDragEdge;
    use std::collections::VecDeque;
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
        /// Deliver one message turn on the creator thread.
        fn pump_messages(&mut self, timeout_ms: u32);
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

        fn pump_messages(&mut self, timeout_ms: u32) {
            pump_messages(timeout_ms);
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

        fn pump_messages(&mut self, _timeout_ms: u32) {
            self.pump_turns += 1;
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
            Ok(())
        }

        /// Bounded stop wait. The pump records its stop acknowledgement as
        /// its terminal act, so joining after an ack is safe. With no ack
        /// the thread is joined only when already finished; a still-running
        /// thread keeps its handle (no detach) and the call returns unclean
        /// while retaining `Stopping`, so no overlapping worker can start.
        /// Only a clean stop (successful unhook acknowledgement plus exit)
        /// releases the process pump slot for a later spawn; every unclean
        /// path keeps the reserve, including a dropped handle. Never waits
        /// indefinitely and never holds channel locks while joining.
        pub fn wait_stopped(&mut self, timeout: Duration) -> StopReport {
            let deadline = Instant::now() + timeout;
            loop {
                if let Some(unhook_ok) = self.signals.stopped() {
                    let thread = self.thread.take().expect("stop joins once");
                    let _ = thread.join();
                    return match self.lifecycle.note_unhooked(unhook_ok) {
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
                    };
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
                    return match self.lifecycle.note_unhooked(unhook_ok) {
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
                    };
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
    }

    /// One pump message turn on the creator thread: delivers LL callbacks for
    /// hooks installed here, then waits briefly for more. The owner focus
    /// settle pump must not call this: it would not deliver this thread's
    /// callbacks anyway, and LL delivery stays isolated here.
    fn pump_messages(timeout_ms: u32) {
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
        unsafe {
            MsgWaitForMultipleObjectsEx(0, std::ptr::null(), timeout_ms, QS_ALLINPUT, 0);
        }
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

    /// Production pump step shared by the real thread and fake-backend
    /// tests. All install / retry / apply / drain / offer / stop logic lives
    /// here; only hook install, uninstall and message pumping go through the
    /// [`NativeHooks`] backend. Fake backends perform no native effects, and
    /// every `sys` helper used here is a no-op without installed hook state,
    /// so step tests exercise the production logic with zero native calls.
    struct PumpCore<B: NativeHooks> {
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
        cb_outbox_dropped: u32,
    }

    impl<B: NativeHooks> PumpCore<B> {
        /// New core over shared channels; installs on the first [`Self::step`].
        fn new(
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
                latest: None,
                applied: None,
                last_mouse_gate: None,
                pending_snap: None,
                pending_windrag: None,
                diag: InputDiag::default(),
                turns: 0,
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
        /// generation and retry before any newer drain.
        fn offer_batches(&mut self) {
            let generation = self.applied_generation();
            let trace = self.latest.as_ref().is_some_and(|snap| snap.trace);
            if let Some(pending) = self.pending_snap.take() {
                match self.channels.snap_outbox.try_put(pending) {
                    Ok(_) => {}
                    Err(batch) => self.pending_snap = Some(batch),
                }
            } else if self.channels.snap_outbox.try_occupied() != Some(true)
                && self.keyboard_hook.is_some()
            {
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
                        }
                        Err(batch) => self.pending_snap = Some(batch),
                    }
                }
            }
            if let Some(pending) = self.pending_windrag.take() {
                match self.channels.windrag_outbox.try_put(pending) {
                    Ok(_) => {}
                    Err(batch) => self.pending_windrag = Some(batch),
                }
            } else if self.channels.windrag_outbox.try_occupied() != Some(true)
                && self.mouse_hook.is_some()
            {
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
                        }
                        Err(batch) => self.pending_windrag = Some(batch),
                    }
                }
            }
            if self.channels.cb_outbox.try_occupied() != Some(true) {
                let diags = crate::snapkey::sys::drain_callback_diag();
                if !diags.is_empty() && self.channels.cb_outbox.try_put(diags).is_err() {
                    self.cb_outbox_dropped += 1;
                }
            }
        }

        /// Publish bounded diagnostics without waiting. Queue age and the
        /// per-turn dispatch span carry explicit non-latency limits; the
        /// heartbeat turn count proves liveness only.
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
            self.diag.dispatch_us = if trace { dispatch_us } else { 0 };
            self.diag.delivery_gap_ms = if trace { take_delivery_gap_ms() } else { 0 };
            self.diag.pending_snap =
                self.pending_snap.is_some() as usize + self.pending_windrag.is_some() as usize;
            self.diag.cb_outbox_dropped = self.cb_outbox_dropped;
            let _ = self.channels.diag.try_publish(&self.diag);
        }

        /// One pump turn: install/retry, FIFO apply, message delivery,
        /// batch offers, diagnostics. Returns true when a stop was
        /// requested.
        fn step(&mut self) -> bool {
            if self.channels.stop_flag.load(Ordering::SeqCst) {
                return true;
            }
            // FIFO apply before install/retry so a policy snapshot that
            // arrived while hooks were down is the config of the next
            // install attempt, never a stale spawn init.
            if let Some(items) = self.channels.inbox.try_drain() {
                for item in &items {
                    self.apply_item(item);
                }
            }
            let now = Instant::now();
            self.install_due(now);
            self.refresh_mouse_gate();
            let dispatch_start = Instant::now();
            self.backend.pump_messages(PUMP_QUANTUM_MS);
            let dispatch_us = dispatch_start
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64;
            self.turns += 1;
            self.offer_batches();
            self.publish_diag(dispatch_us);
            self.channels.stop_flag.load(Ordering::SeqCst)
        }

        /// Creator-side shutdown with bounded unhook retries: consumed pairs
        /// stay handled until the release; no wait for physical release and
        /// no synthetic key cleanup. Hook state is retained on failure (the
        /// `sys` uninstalls only mutate on success), so a failed release
        /// never strands a swallowed pair as a passed-through Up.
        fn shutdown(&mut self) -> StopReport {
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
    /// The stop acknowledgement is the terminal act, so the owner's bounded
    /// join after an ack is safe.
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
    }
}

#[cfg(windows)]
pub use native::{FailEvent, InputHandle, NativeHooks, PumpSignals};

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
