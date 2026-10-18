#![forbid(unsafe_code)]
//! Portable dedicated-input-thread handoff (stage 1, unit 1).
//!
//! One ordered FIFO carries owner-built immutable payloads to the input pump;
//! one single-outstanding bounded batch carries drained intents back. Esc
//! sequences bypass transport on shared atomics. The input pump applies the
//! FIFO between message pumps into local classifier state; callbacks only
//! read that local state. No latest-wins config, no second lossy queue, no
//! blocking while the other side stalls.

use std::collections::{HashMap, VecDeque};
use std::sync::{
    Arc, Mutex, OnceLock, TryLockError,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
use std::time::Instant;

use crate::snapkey::{ChordDisable, ChordRemap, KeyboardConfig, SnapOrigin};

/// Bound for the owner -> input payload FIFO. Full is an explicit fault with
/// the item returned for retention/retry, never a silent drop.
pub const INBOX_CAP: usize = 8;

/// Bound for one input -> owner batch. Matches the existing per-tick drain
/// cap (`MAX_DISPATCH_PER_TICK`).
pub const OUTBOX_BATCH_CAP: usize = 8;

/// START/END Esc comparison. Held gestures compare the live clock against
/// START; ended gestures compare the END-sampled sequence against START, so a
/// tap landing after END (before the owner drain) cannot cancel the completed
/// drop while a tap bundled with END still latches.
#[must_use]
pub const fn esc_edge_cancels(start_seq: u64, observed_seq: u64) -> bool {
    observed_seq != start_seq
}

/// Monotonic Esc clock bumped by input callbacks and sampled by the owner.
/// Inequality comparison only, so wrapping needs no fence.
#[derive(Debug, Default)]
pub struct EscClock {
    seq: AtomicU64,
}

impl EscClock {
    /// New zeroed clock.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
        }
    }

    /// Record one Esc down edge; returns the new sequence value.
    pub fn bump(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    }

    /// Current sequence for START/END snapshots.
    #[must_use]
    pub fn load(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

/// Process-global native Esc clock. Callbacks bump it at Esc-down time and
/// the owner WinEvent proc samples it directly at START/END, so a delayed
/// batch sequence can never reorder the comparison. A singleton (never
/// re-registered per handle) so pump restarts in one process cannot bump a
/// stale clock while the owner samples a fresh one.
static GLOBAL_ESC: OnceLock<EscClock> = OnceLock::new();
/// Process-global native Esc edge (consume-once). Set directly in the hook
/// callback alongside the clock bump, so an Esc arriving just before a
/// WinEvent END is visible to the owner settle even before the pump forwards
/// anything. Sequence comparison still decides cancellation; the flag alone
/// never cancels.
static GLOBAL_ESC_EDGE: AtomicBool = AtomicBool::new(false);
/// Process-global trace-only delivery-gap accumulator: max native event
/// timestamp (`KBDLLHOOKSTRUCT.time`) to callback
/// entry gap in milliseconds observed since the last pump publish. The pump
/// reads and resets it per quantum; the owner logs it with explicit limits
/// (tick-count domain only, never end-to-end latency).
static GLOBAL_DELIVERY_GAP_MS: AtomicU32 = AtomicU32::new(0);

/// Process-global native Esc clock shared by callbacks and WinEvent START/END.
#[must_use]
pub fn global_esc_clock() -> &'static EscClock {
    GLOBAL_ESC.get_or_init(EscClock::new)
}

/// Process-global native Esc edge flag. Set in callbacks, consumed
/// owner-side with `swap(false)`; cleared by ordered `ClearEsc` application.
pub fn global_esc_edge() -> &'static AtomicBool {
    &GLOBAL_ESC_EDGE
}

/// Record one trace-only native-to-callback delivery gap sample (keeps the
/// max). Cheap max accumulator for the pump's per-quantum publish.
pub fn record_delivery_gap_ms(gap_ms: u32) {
    GLOBAL_DELIVERY_GAP_MS.fetch_max(gap_ms, Ordering::Relaxed);
}

/// Take and reset the delivery-gap accumulator (input pump, per quantum).
#[must_use]
pub fn take_delivery_gap_ms() -> u32 {
    GLOBAL_DELIVERY_GAP_MS.swap(0, Ordering::Relaxed)
}

/// Complete owner-built input policy snapshot. Applied whole and in FIFO
/// order; newer snapshots never leapfrog pending invalidations.
///
/// Snapshots cross threads as immutable [`SharedSnapshot`] (`Arc`): the input
/// pump swaps the `Arc` outside callbacks and callbacks only read one entry.
/// Never clone these maps inside a hook callback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSnapshot {
    /// Policy generation: bumped by the owner only when the classifier
    /// policy (`keyboard`, `remap`, `disabled`, `active`) actually changes.
    /// Map-only republishes keep the generation. The pump tags every drained
    /// batch with the generation it classified under; the owner drops
    /// batches older than the latest published policy as stale (mask events
    /// still resolve) instead of dispatching under a replaced policy.
    pub generation: u64,
    /// Keyboard takeover policy.
    pub keyboard: KeyboardConfig,
    /// Owner-published rebound table.
    pub remap: Vec<ChordRemap>,
    /// Owner-published suppression table.
    pub disabled: Vec<ChordDisable>,
    /// Managed chord origins for callback binding.
    pub snap_origins: HashMap<u64, SnapOrigin>,
    /// Shortcut-handling gate (takeover plus pause sources).
    pub active: bool,
    /// Trace-only callback collection flag.
    pub trace: bool,
}

/// Immutable owner-built snapshot shared across the handoff. The owner builds
/// this off the input thread and moves the `Arc` into the [`Inbox`]; the
/// input pump applies it outside callbacks. Callbacks never clone the maps.
pub type SharedSnapshot = Arc<InputSnapshot>;

/// Content equality for change detection: every field except `generation`.
/// The owner offers a snapshot only when this differs from the last offered
/// one, so unchanged ticks send nothing and map-only republishes never flush
/// classifier queues.
#[must_use]
pub fn snapshot_content_eq(a: &InputSnapshot, b: &InputSnapshot) -> bool {
    a.keyboard == b.keyboard
        && a.remap == b.remap
        && a.disabled == b.disabled
        && a.snap_origins == b.snap_origins
        && a.active == b.active
        && a.trace == b.trace
}

/// Policy equality: the classifier-behavior subset (`keyboard`, `remap`,
/// `disabled`, `active`). The pump drains stale queued actions only when this
/// differs; origins/gate republishes with equal policy never flush the queue,
/// so held pairs and legitimate queued actions survive map updates.
#[must_use]
pub fn snapshot_policy_eq(a: &InputSnapshot, b: &InputSnapshot) -> bool {
    a.keyboard == b.keyboard
        && a.remap == b.remap
        && a.disabled == b.disabled
        && a.active == b.active
}

/// One generation-tagged input -> owner batch. The tag is the policy
/// generation the pump had applied when it drained the batch. The owner drops
/// batches older than its latest published policy as stale (mask events still
/// resolve through the normal mask path) and dispatches the rest unchanged.
#[derive(Debug, Clone)]
pub struct TaggedBatch<T> {
    /// Policy generation at drain time.
    pub generation: u64,
    /// Drained records, oldest first.
    pub items: Vec<T>,
}

impl<T> TaggedBatch<T> {
    /// New tagged batch.
    #[must_use]
    pub fn new(generation: u64, items: Vec<T>) -> Self {
        Self { generation, items }
    }

    /// True when classified under a superseded policy: the owner must not
    /// dispatch these under the current policy.
    #[must_use]
    pub fn is_stale(&self, published_generation: u64) -> bool {
        self.generation < published_generation
    }

    /// Record count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when no record is carried.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Callback-visible policy publication fence (freshness correction).
///
/// The owner bumps the published policy generation when it offers a
/// policy-changing snapshot; the pump records the applied generation when it
/// applies one. Between the two, pump-applied classifier state is stale: a
/// callback that consumed under the old policy would have its action
/// discarded owner-side as stale. Callbacks therefore refuse *fresh*
/// consumption while the fence is pending (fresh chords pass through, fresh
/// mouse Downs pass through). In-flight state is untouched: tracked pass
/// verdicts, consumed held pairs/repeats and the Win-up mask obligation ride
/// their stored down-time verdicts exactly like a closed shortcut gate.
///
/// Both counters are monotonic (`fetch_max`): map-only republishes keep the
/// generation and never arm the fence. Pending means strictly
/// `published > applied`; an applied-ahead reading (test-only ordering) is
/// not pending.
static PUBLISHED_POLICY_GEN: AtomicU64 = AtomicU64::new(0);
static APPLIED_POLICY_GEN: AtomicU64 = AtomicU64::new(0);

/// Owner-side: record a newly offered policy generation. Call before the
/// inbox offer so callbacks refuse fresh consumption fail-closed even while
/// the snapshot is still queued.
pub fn note_policy_published(generation: u64) {
    PUBLISHED_POLICY_GEN.fetch_max(generation, Ordering::SeqCst);
}

/// Pump-side: record a newly applied policy generation.
pub fn note_policy_applied(generation: u64) {
    APPLIED_POLICY_GEN.fetch_max(generation, Ordering::SeqCst);
}

/// True while a newer policy is published but not yet applied: callbacks
/// must refuse fresh consumption.
///
/// Fail-closed paired read: the applied generation loads before the
/// published one, so a concurrent publish between the loads reports
/// pending (safe) rather than a concurrent apply hiding a publish.
#[must_use]
pub fn policy_fence_pending() -> bool {
    let applied = APPLIED_POLICY_GEN.load(Ordering::SeqCst);
    let published = PUBLISHED_POLICY_GEN.load(Ordering::SeqCst);
    published > applied
}

/// Base generation for a new owner run in this process: the maximum of the
/// process-global published/applied generations. A second same-process owner
/// run starts its local `published_gen` here so its first policy bump lands
/// above every earlier allocation and re-arms the fence; starting from zero
/// would leave `fetch_max` pinned at the old maximum and silently disable
/// the fence for the new run. Never resets the globals (a reset would race
/// an active pump); a leftover pending fence from an unclean run stays
/// pending fail-closed until the new pump applies.
#[must_use]
pub fn fence_base_generation() -> u64 {
    PUBLISHED_POLICY_GEN
        .load(Ordering::SeqCst)
        .max(APPLIED_POLICY_GEN.load(Ordering::SeqCst))
}

/// Process-level input-pump slot: exactly one pump worker per process.
/// Reserved at spawn, released only after creator-side successful
/// unhook-plus-exit. A hung or unclean stop (including a dropped
/// `InputBridge`/`JoinHandle`) keeps the reserve so no overlapping worker
/// can start; recovery is process exit, never a duplicate worker. Never
/// released on `Drop`: only an explicit clean-stop or spawn-failure release
/// frees the slot.
static PUMP_SLOT_RESERVED: AtomicBool = AtomicBool::new(false);

/// Reserve the process pump slot. True when newly reserved, false when a
/// worker already holds it (the caller must refuse to spawn).
pub fn pump_slot_try_reserve() -> bool {
    PUMP_SLOT_RESERVED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

/// Release the process pump slot after a clean stop or a spawn failure.
pub fn pump_slot_release() {
    PUMP_SLOT_RESERVED.store(false, Ordering::SeqCst);
}

/// True while a pump worker holds the process slot.
#[must_use]
pub fn pump_slot_reserved() -> bool {
    PUMP_SLOT_RESERVED.load(Ordering::SeqCst)
}

#[cfg(test)]
static PUMP_SLOT_TEST_SERIAL: Mutex<()> = Mutex::new(());

/// Serialize tests that mutate the process-global pump slot.
#[cfg(test)]
pub fn pump_slot_test_serial() -> std::sync::MutexGuard<'static, ()> {
    PUMP_SLOT_TEST_SERIAL.lock().expect("pump slot test serial")
}

/// Effective fresh-chord gate for keyboard callbacks: the stored
/// pump-applied gate closed further while the fence is pending.
#[must_use]
pub fn keyboard_fresh_gate(stored_active: bool) -> bool {
    stored_active && !policy_fence_pending()
}

#[cfg(test)]
static FENCE_TEST_SERIAL: Mutex<()> = Mutex::new(());

/// Serialize tests that mutate the process-global fence generations.
/// Step-seam tests apply small fixed generations without publishing, so they
/// cannot disturb fence tests allocating above the live maxima; fence tests
/// must still serialize among themselves, allocate via
/// [`fence_test_allocate`], and apply everything they publish before
/// releasing the guard, leaving the fence clear for unrelated callback
/// tests (which never consult the gate on their paths, but must never
/// observe a stale pending fence either).
#[cfg(test)]
pub fn fence_test_serial() -> std::sync::MutexGuard<'static, ()> {
    FENCE_TEST_SERIAL.lock().expect("fence test serial")
}

/// Allocate a fresh test policy generation above every generation published
/// or applied so far. Call only while holding [`fence_test_serial`]:
/// hardcoded bases are unsound because `fetch_max` never moves backwards
/// (a later-allocated base running first would strand an earlier test's
/// apply below the live published maximum, leaving its "cleared"
/// assertions pending). The caller publishes and applies the allocation
/// before releasing the guard.
#[cfg(test)]
pub fn fence_test_allocate() -> u64 {
    PUBLISHED_POLICY_GEN
        .load(Ordering::SeqCst)
        .max(APPLIED_POLICY_GEN.load(Ordering::SeqCst))
        .wrapping_add(1)
}

/// One ordered owner -> input payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InboxItem {
    /// Apply the whole snapshot (policy, tables, origins, gates).
    Snapshot(SharedSnapshot),
    /// Drop a stale unlatched Esc edge (suspend/teardown only).
    ClearEsc,
}

/// Bounded FIFO for [`InboxItem`]. Lossless: [`Inbox::push`] returns a
/// refused item for retention/retry. Never touched by callbacks.
#[derive(Debug)]
pub struct Inbox {
    inner: Mutex<VecDeque<InboxItem>>,
}

impl Default for Inbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Inbox {
    /// New empty inbox.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::with_capacity(INBOX_CAP)),
        }
    }

    /// Queue one item; `Err` returns the item boxed for retention/retry
    /// when full.
    pub fn push(&self, item: InboxItem) -> Result<(), Box<InboxItem>> {
        let mut guard = self.inner.lock().expect("inbox lock");
        if guard.len() >= INBOX_CAP {
            return Err(Box::new(item));
        }
        guard.push_back(item);
        Ok(())
    }

    /// Nonblocking owner offer: `Ok(true)` when queued, `Ok(false)` when
    /// full (item returned through `slot`) or contended. Never waits on the
    /// input pump. Full is an explicit fault: the caller must close fresh
    /// admission immediately and retry the preserved ordered item later;
    /// never accumulate an unbounded owner backlog.
    pub fn try_push(&self, item: InboxItem, slot: &mut Option<InboxItem>) -> bool {
        match self.inner.try_lock() {
            Ok(mut guard) => {
                if guard.len() >= INBOX_CAP {
                    *slot = Some(item);
                    false
                } else {
                    guard.push_back(item);
                    true
                }
            }
            Err(TryLockError::WouldBlock) => {
                *slot = Some(item);
                false
            }
            Err(TryLockError::Poisoned(_)) => panic!("inbox lock"),
        }
    }

    /// Drain all pending items, oldest first.
    pub fn drain(&self) -> Vec<InboxItem> {
        self.inner.lock().expect("inbox lock").drain(..).collect()
    }

    /// Nonblocking input-pump drain outside callbacks: `None` on contention
    /// (pump retries next iteration), `Some(items)` (possibly empty) when the
    /// lock was acquired. Never waits on the owner.
    pub fn try_drain(&self) -> Option<Vec<InboxItem>> {
        match self.inner.try_lock() {
            Ok(mut guard) => Some(guard.drain(..).collect()),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(_)) => panic!("inbox lock"),
        }
    }

    /// True when a snapshot is queued (dedupe guard: a queued snapshot
    /// between two equal safety controls means the newer control must be
    /// preserved, never collapsed). Contention fails closed to true.
    #[must_use]
    pub fn has_snapshot(&self) -> bool {
        match self.inner.try_lock() {
            Ok(guard) => guard
                .iter()
                .any(|item| matches!(item, InboxItem::Snapshot(_))),
            Err(_) => true,
        }
    }

    /// True when a safety control (invalidate/clear) is queued (dedupe
    /// guard for re-offered snapshots). Contention fails closed to true.
    #[must_use]
    pub fn has_safety(&self) -> bool {
        match self.inner.try_lock() {
            Ok(guard) => guard.iter().any(|item| matches!(item, InboxItem::ClearEsc)),
            Err(_) => true,
        }
    }

    /// Pending item count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().expect("inbox lock").len()
    }

    /// Test-only: hold the FIFO lock across a pump turn, so `try_drain`
    /// reports WouldBlock and the contention retry is exercised over the
    /// real seam (no sleeps, no threads).
    #[cfg(test)]
    pub fn test_lock_held(&self) -> std::sync::MutexGuard<'_, VecDeque<InboxItem>> {
        self.inner.lock().expect("inbox lock")
    }

    /// True when no item is pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Single outstanding bounded batch, input -> owner. The pump offers a drain
/// only when empty and the owner takes when ready; while occupied the pump
/// skips draining so items wait in the original classifier queues under
/// their own cap+priority rules. Never drops, never overwrites, never
/// blocks: [`Outbox::put`] refuses (returning the batch intact) while
/// occupied. Batch size bounds live with the caller: the pump only offers
/// drains capped at the matching per-tick limit (`OUTBOX_BATCH_CAP` for
/// intents/edges, `CALLBACK_DIAG_CAP` for summaries), and the single-slot
/// shape keeps at most one batch in flight.
#[derive(Debug, Default)]
pub struct Outbox<T> {
    inner: Mutex<OutboxInner<T>>,
}

#[derive(Debug, Default)]
struct OutboxInner<T> {
    batch: Option<T>,
    offered: u64,
    taken: u64,
}

impl<T> Outbox<T> {
    /// New empty outbox.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(OutboxInner {
                batch: None,
                offered: 0,
                taken: 0,
            }),
        }
    }

    /// True when an untaken batch is outstanding.
    #[must_use]
    pub fn occupied(&self) -> bool {
        self.inner.lock().expect("outbox lock").batch.is_some()
    }

    /// Offer one drain batch. Refuses (returning the batch) when occupied,
    /// so end-to-end saturation semantics stay with the classifier queues.
    pub fn put(&self, batch: T) -> Result<(), T> {
        let mut guard = self.inner.lock().expect("outbox lock");
        if guard.batch.is_some() {
            return Err(batch);
        }
        guard.batch = Some(batch);
        guard.offered += 1;
        Ok(())
    }

    /// Nonblocking input-pump offer outside callbacks: `Ok(true)` when the
    /// batch was accepted, `Err` with the batch returned intact when
    /// occupied or contended (items wait in the original classifier queues
    /// under their own cap+priority rules). Never waits on the owner and
    /// never drops.
    pub fn try_put(&self, batch: T) -> Result<bool, T> {
        match self.inner.try_lock() {
            Ok(mut guard) => {
                if guard.batch.is_some() {
                    return Err(batch);
                }
                guard.batch = Some(batch);
                guard.offered += 1;
                Ok(true)
            }
            Err(TryLockError::WouldBlock) => Err(batch),
            Err(TryLockError::Poisoned(_)) => panic!("outbox lock"),
        }
    }

    /// Nonblocking input-pump occupancy probe. `None` on contention (treat
    /// as occupied: skip draining this iteration).
    pub fn try_occupied(&self) -> Option<bool> {
        match self.inner.try_lock() {
            Ok(guard) => Some(guard.batch.is_some()),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(_)) => panic!("outbox lock"),
        }
    }

    /// Take the outstanding batch, if any.
    pub fn take(&self) -> Option<T> {
        let mut guard = self.inner.lock().expect("outbox lock");
        let batch = guard.batch.take()?;
        guard.taken += 1;
        Some(batch)
    }

    /// Batches offered so far (stall evidence).
    #[must_use]
    pub fn offered(&self) -> u64 {
        self.inner.lock().expect("outbox lock").offered
    }

    /// Batches taken so far.
    #[must_use]
    pub fn taken(&self) -> u64 {
        self.inner.lock().expect("outbox lock").taken
    }

    /// Test-only: hold the slot lock while `f` runs, so `try_occupied`
    /// and `try_put` report WouldBlock and the contention retry is exercised
    /// over the real seam (no sleeps, no threads). Closure style keeps the
    /// private slot type out of the signature.
    #[cfg(test)]
    pub fn test_with_lock_held<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = self.inner.lock().expect("outbox lock");
        f()
    }
}

/// Bounded input -> owner diagnostics published by the input pump outside
/// callbacks and consumed trace-only by the owner writer. Counts only, no
/// identity. `queue_age_us` is the oldest queued intent/edge age at offer
/// time; `heartbeat_us` is the gap since the previous offer. Neither measures
/// OS hook delivery latency: callback duration alone never covers the native
/// event timestamp vs delivery gap, and the heartbeat gap only proves the
/// pump is alive. Honest limits, logged with those exclusions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputDiag {
    /// Snap classifier drops observed on the input pump.
    pub snap_dropped: u32,
    /// Callback-summary drops/filtered plus mask-send aggregate.
    pub cb_dropped: u32,
    /// Callback-summary privacy-filtered count.
    pub cb_filtered: u32,
    /// Mask `SendInput` pairs sent.
    pub mask_sends: u32,
    /// Max observed mask-send microseconds.
    pub mask_send_max_us: u32,
    /// Oldest queued item age at the last offer, microseconds. Snap batches
    /// only; the owner measures take-time age itself for dispatch decisions.
    pub queue_age_us: u64,
    /// Gap since the previous offer, microseconds (liveness only).
    pub heartbeat_us: u64,
    /// Offers published so far.
    pub offers: u64,
    /// Pump message turns completed: one drain-wait-drain-publish iteration
    /// (heartbeat liveness, always counted). Equals `waits`; stop before
    /// the wait returns early with neither a wait nor a turn.
    pub turns: u64,
    /// Native wait returns since pump start (one per `MsgWaitForMultipleObjectsEx`
    /// return in the pump step). Counts native wait returns only, never thread
    /// context switches. The owner reads the whole struct under one diag-slot
    /// lock, so one `snapshot()` is coherent; cross-pump coherence with the
    /// renderer atomics is not claimed (see `RenderChannels::wait_counts`).
    pub waits: u64,
    /// Native wait returns by reason: owner work wake, queued callback
    /// messages, genuine deadline timeout (`WAIT_TIMEOUT` 258), or failed /
    /// unrecognized (`WAIT_FAILED` et al). Exactly one per `waits`; failures
    /// are counted separately and never merged into timeout.
    pub wait_work: u64,
    pub wait_msg: u64,
    pub wait_timeout: u64,
    pub wait_failed: u64,
    /// Last message-turn dispatch span in microseconds: message delivery plus
    /// batch transfer, excluding the native wait, trace-only. Covers
    /// neither OS scheduling before the native event timestamp nor the
    /// downstream hook chain; never end-to-end latency.
    pub dispatch_us: u64,
    /// Max native event timestamp to callback-entry gap in milliseconds
    /// since the last publish, trace-only. Tick-count domain only; excludes
    /// everything before the native timestamp and after callback entry.
    pub delivery_gap_ms: u32,
    /// Thread-local parked batches awaiting outbox space (snap).
    pub pending_snap: usize,
    /// Callback-summary batches dropped on a full `cb_outbox`.
    pub cb_outbox_dropped: u32,
    /// Last offer time for heartbeat computation.
    pub last_offer: Option<Instant>,
}

impl InputDiag {
    /// Record one offer: stamps heartbeat gap from the previous offer and the
    /// oldest-item queue age supplied by the pump.
    pub fn note_offer(&mut self, queue_age_us: u64, now: Instant) {
        let gap = self
            .last_offer
            .map(|prev| {
                now.duration_since(prev)
                    .as_micros()
                    .min(u128::from(u64::MAX)) as u64
            })
            .unwrap_or(0);
        self.queue_age_us = queue_age_us;
        self.heartbeat_us = gap;
        self.last_offer = Some(now);
        self.offers = self.offers.saturating_add(1);
    }
}

/// Single-slot diagnostics exchange. The input pump publishes with
/// [`DiagSlot::try_publish`] (try-lock, never waits on the owner); the owner
/// reads with [`DiagSlot::snapshot`] (blocking is fine off the pump, falls
/// back to the last snapshot on contention via [`DiagSlot::try_snapshot`]).
#[derive(Debug, Default)]
pub struct DiagSlot {
    inner: Mutex<InputDiag>,
}

impl DiagSlot {
    /// New empty slot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(InputDiag::default()),
        }
    }

    /// Publish one sample. False on contention (pump retries next offer;
    /// never waits).
    pub fn try_publish(&self, diag: &InputDiag) -> bool {
        match self.inner.try_lock() {
            Ok(mut guard) => {
                *guard = *diag;
                true
            }
            Err(TryLockError::WouldBlock) => false,
            Err(TryLockError::Poisoned(_)) => panic!("diag lock"),
        }
    }

    /// Latest sample (owner side).
    #[must_use]
    pub fn snapshot(&self) -> InputDiag {
        *self.inner.lock().expect("diag lock")
    }

    /// Nonblocking read. `None` on contention.
    #[must_use]
    pub fn try_snapshot(&self) -> Option<InputDiag> {
        match self.inner.try_lock() {
            Ok(guard) => Some(*guard),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(_)) => panic!("diag lock"),
        }
    }
}

/// Input pump lifecycle: exactly one input worker at a time. A failed or
/// missing hook release retains `Stopping` so no overlapping worker can
/// start; only a successful unhook acknowledgement completes the stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpState {
    /// No input worker.
    Stopped,
    /// Worker requested, hook install pending.
    Starting,
    /// Hooks installed, callbacks live.
    Running,
    /// Stop requested, unhook acknowledgement pending.
    Stopping,
}

/// Lifecycle transition refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    /// Start requested while not stopped: no overlapping worker.
    AlreadyActive,
    /// Stop/ack with no matching pending step.
    NothingPending,
}

/// Truthful stop outcome; clean only with a successful acknowledgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopReport {
    /// The unhook call reported success.
    pub unhook_ok: bool,
    /// Clean stop: acknowledgement present and successful.
    pub clean: bool,
}

/// Single-owner input pump lifecycle.
#[derive(Debug)]
pub struct PumpLifecycle {
    state: PumpState,
}

impl Default for PumpLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl PumpLifecycle {
    /// New stopped lifecycle.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: PumpState::Stopped,
        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> PumpState {
        self.state
    }

    /// Request the worker. `Stopped -> Starting`; anything else refuses.
    pub fn start(&mut self) -> Result<(), LifecycleError> {
        if self.state != PumpState::Stopped {
            return Err(LifecycleError::AlreadyActive);
        }
        self.state = PumpState::Starting;
        Ok(())
    }

    /// Readiness: hooks installed. `Starting -> Running`.
    pub fn note_installed(&mut self) -> Result<(), LifecycleError> {
        if self.state != PumpState::Starting {
            return Err(LifecycleError::NothingPending);
        }
        self.state = PumpState::Running;
        Ok(())
    }

    /// Install failure: `Starting -> Stopped` (no worker holds resources, so
    /// a later start is safe).
    pub fn install_failed(&mut self) -> Result<(), LifecycleError> {
        if self.state != PumpState::Starting {
            return Err(LifecycleError::NothingPending);
        }
        self.state = PumpState::Stopped;
        Ok(())
    }

    /// Graceful stop request. Idempotent in `Stopping` so the owner can
    /// re-drive unhook attempts after a failure.
    pub fn request_stop(&mut self) -> Result<(), LifecycleError> {
        match self.state {
            PumpState::Running | PumpState::Starting | PumpState::Stopping => {
                self.state = PumpState::Stopping;
                Ok(())
            }
            PumpState::Stopped => Err(LifecycleError::NothingPending),
        }
    }

    /// Unhook acknowledgement. Failure retains `Stopping` (never claims
    /// clean, never frees the worker slot); success completes the stop.
    pub fn note_unhooked(&mut self, unhook_ok: bool) -> Result<StopReport, LifecycleError> {
        if self.state != PumpState::Stopping {
            return Err(LifecycleError::NothingPending);
        }
        if !unhook_ok {
            return Ok(StopReport {
                unhook_ok: false,
                clean: false,
            });
        }
        self.state = PumpState::Stopped;
        Ok(StopReport {
            unhook_ok: true,
            clean: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        INBOX_CAP, Inbox, InboxItem, InputSnapshot, LifecycleError, OUTBOX_BATCH_CAP, Outbox,
        PumpLifecycle, PumpState, SharedSnapshot, TaggedBatch, esc_edge_cancels,
        fence_base_generation, fence_test_allocate, fence_test_serial, note_policy_applied,
        note_policy_published, policy_fence_pending, pump_slot_release, pump_slot_reserved,
        pump_slot_test_serial, pump_slot_try_reserve,
    };
    use crate::snapkey::{
        INTENT_QUEUE_CAP, KeyboardConfig, QueuedIntent, QueuedSnapEvent, SnapClassify, SnapEdge,
        SnapOp, SnapOrigin, SnapQueue, VK_H, VK_LWIN, classify_and_queue, win_up_mask_reserve,
    };
    use std::collections::HashMap;
    use std::time::Instant;
    use tiler_core::directional::Direction;

    fn origin_of(hwnd: u64) -> SnapOrigin {
        SnapOrigin {
            hwnd,
            token: format!("token-{hwnd}"),
            pid: 1000 + hwnd as u32,
            creation: format!("creation-{hwnd}"),
        }
    }

    fn snapshot_with(hwnds: &[u64], takeover: bool) -> SharedSnapshot {
        let mut snap = InputSnapshot {
            generation: 0,
            keyboard: KeyboardConfig {
                takeover,
                allow_win_l: false,
            },
            remap: Vec::new(),
            disabled: Vec::new(),
            snap_origins: HashMap::new(),
            active: takeover,
            trace: false,
        };
        for hwnd in hwnds {
            snap.snap_origins.insert(*hwnd, origin_of(*hwnd));
        }
        std::sync::Arc::new(snap)
    }

    struct Applied {
        machine: SnapClassify,
        snap_origins: HashMap<u64, SnapOrigin>,
    }

    impl Applied {
        fn new() -> Self {
            Self {
                machine: SnapClassify::new(KeyboardConfig::disabled()),
                snap_origins: HashMap::new(),
            }
        }

        // Production apply path in miniature: whole snapshots and controls in
        // FIFO order onto the real classifier plus the applied origin maps.
        fn apply(&mut self, item: &InboxItem) {
            match item {
                InboxItem::Snapshot(snap) => {
                    self.machine.set_enabled(snap.keyboard.takeover);
                    self.machine
                        .set_gate_active(snap.keyboard.takeover && snap.active);
                    self.machine.set_remap(snap.remap.clone());
                    self.machine.set_disabled(snap.disabled.clone());
                    self.snap_origins = snap.snap_origins.clone();
                }
                InboxItem::ClearEsc => {}
            }
        }
    }

    fn win_down(machine: &mut SnapClassify) {
        assert_eq!(
            SnapClassify::push(machine, VK_LWIN, false, true, false),
            None
        );
    }

    fn chord(
        machine: &mut SnapClassify,
        queue: &mut SnapQueue,
        vk: u32,
        gate: bool,
    ) -> Option<bool> {
        classify_and_queue(machine, queue, vk, false, None, false, Instant::now(), gate)
    }

    fn chord_up(
        machine: &mut SnapClassify,
        queue: &mut SnapQueue,
        vk: u32,
        gate: bool,
    ) -> Option<bool> {
        classify_and_queue(machine, queue, vk, true, None, false, Instant::now(), gate)
    }

    fn dummy_intent() -> QueuedSnapEvent {
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

    fn drain_queue(queue: &mut SnapQueue, limit: usize) -> Vec<QueuedSnapEvent> {
        let mut out = Vec::new();
        while out.len() < limit.max(1) {
            match queue.pop_front() {
                Some(event) => out.push(event),
                None => break,
            }
        }
        out
    }

    #[test]
    fn snapshot_snapshot_applies_strictly_in_order() {
        // Snapshots apply as queued payloads, never with the later policy
        // acting before the earlier one.
        let inbox = Inbox::new();
        let first = snapshot_with(&[7], true);
        let second = snapshot_with(&[], false);
        let pushed = vec![InboxItem::Snapshot(first), InboxItem::Snapshot(second)];
        for item in &pushed {
            inbox.push(item.clone()).expect("fits");
        }
        let drained = inbox.drain();
        assert_eq!(drained, pushed);
        let mut applied = Applied::new();
        applied.apply(&drained[0]);
        assert!(applied.snap_origins.contains_key(&7));
        assert!(applied.machine.enabled);
        win_down(&mut applied.machine);
        let mut queue = SnapQueue::new();
        assert_eq!(
            chord(&mut applied.machine, &mut queue, VK_H, true),
            Some(true)
        );
        assert_eq!(
            chord_up(&mut applied.machine, &mut queue, VK_H, true),
            Some(true)
        );
        applied.apply(&drained[1]);
        assert!(!applied.snap_origins.contains_key(&7));
        assert!(!applied.machine.enabled);
        let mut queue = SnapQueue::new();
        win_down(&mut applied.machine);
        // Takeover off now passes fresh chords through.
        assert_eq!(
            chord(&mut applied.machine, &mut queue, VK_H, true),
            Some(false)
        );
    }

    #[test]
    fn inbox_full_returns_item_for_retry_without_loss() {
        let inbox = Inbox::new();
        for _ in 0..INBOX_CAP {
            inbox.push(InboxItem::ClearEsc).expect("fits to capacity");
        }
        assert_eq!(inbox.len(), INBOX_CAP);
        let refused = inbox
            .push(InboxItem::ClearEsc)
            .expect_err("full retains the item");
        assert_eq!(*refused, InboxItem::ClearEsc);
        let drained = inbox.drain();
        assert_eq!(drained.len(), INBOX_CAP);
        assert!(inbox.is_empty());
        inbox.push(*refused).expect("retry succeeds");
        assert_eq!(inbox.drain(), vec![InboxItem::ClearEsc]);
    }

    #[test]
    fn gated_fresh_down_passes_and_remembers_verdict() {
        // Callbacks always route physical events into the classifier; the
        // applied gate only refuses fresh actions while the press verdict and
        // Win state are retained.
        let mut machine = SnapClassify::new(KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        });
        machine.set_gate_active(false);
        let mut queue = SnapQueue::new();
        win_down(&mut machine);
        assert!(machine.win_held());
        // Fresh chord while gated: passes through, tracked as a pass-origin
        // hold rather than bypassing the classifier.
        assert_eq!(chord(&mut machine, &mut queue, VK_H, false), Some(false));
        assert!(machine.key_is_down(VK_H));
        // Gate opens mid-hold: the repeat still passes (verdict remembered),
        // it is never converted into a swallow.
        machine.set_gate_active(true);
        assert_eq!(chord(&mut machine, &mut queue, VK_H, true), Some(false));
        assert!(machine.win_held());
        // Paired Up passes as well: never stolen.
        assert_eq!(chord_up(&mut machine, &mut queue, VK_H, true), Some(false));
        assert!(!machine.key_is_down(VK_H));
    }

    #[test]
    fn consumed_hold_survives_gate_close_with_mask() {
        let mut machine = SnapClassify::new(KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        });
        machine.set_gate_active(true);
        let mut queue = SnapQueue::new();
        win_down(&mut machine);
        assert_eq!(chord(&mut machine, &mut queue, VK_H, true), Some(true));
        // Gate closes mid-hold: repeat and Up still ride the stored consumed
        // verdict, and the Win-up mask obligation still fires.
        machine.set_gate_active(false);
        assert_eq!(chord(&mut machine, &mut queue, VK_H, false), Some(true));
        assert!(win_up_mask_reserve(
            &mut machine,
            &mut queue,
            VK_LWIN,
            true,
            Instant::now()
        ));
        assert_eq!(chord_up(&mut machine, &mut queue, VK_H, false), Some(true));
        assert!(!machine.key_is_down(VK_H));
    }

    #[test]
    fn tagged_batch_marks_superseded_policy_stale() {
        let fresh = TaggedBatch::new(4, vec![1u8, 2]);
        assert!(!fresh.is_stale(4));
        assert!(fresh.is_stale(5));
        let stale = TaggedBatch::new(3, vec![1u8]);
        assert!(stale.is_stale(4));
        assert!(!stale.is_stale(3));
        let empty = TaggedBatch::new(2, Vec::<u8>::new());
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn snap_mask_reservation_travels_outbox_end_to_end() {
        let mut machine = SnapClassify::new(KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        });
        machine.set_gate_active(true);
        let mut queue = SnapQueue::new();
        win_down(&mut machine);
        // Consumed chord arms the Win-up mask obligation for this hold.
        assert_eq!(chord(&mut machine, &mut queue, VK_H, true), Some(true));
        // Drain the chord event: only the mask obligation must survive.
        drain_queue(&mut queue, INTENT_QUEUE_CAP);
        for _ in 0..INTENT_QUEUE_CAP {
            assert!(queue.push(dummy_intent()));
        }
        // Saturation keeps its original semantics: the mask reservation
        // evicts the oldest slot and still fires.
        assert!(win_up_mask_reserve(
            &mut machine,
            &mut queue,
            VK_LWIN,
            true,
            Instant::now()
        ));
        assert_eq!(queue.len(), INTENT_QUEUE_CAP);
        let outbox: Outbox<TaggedBatch<QueuedSnapEvent>> = Outbox::new();
        let batch = drain_queue(&mut queue, OUTBOX_BATCH_CAP);
        assert_eq!(batch.len(), OUTBOX_BATCH_CAP);
        outbox.put(TaggedBatch::new(1, batch)).expect("offer");
        let taken = outbox.take().expect("take");
        assert_eq!(taken.items.len(), OUTBOX_BATCH_CAP);
        assert!(outbox.take().is_none());
    }

    #[test]
    fn esc_held_and_ended_compare_against_start() {
        use super::EscClock;
        let clock = EscClock::new();
        let start = clock.load();
        assert!(!esc_edge_cancels(start, clock.load()));
        let edge = clock.bump();
        // Held gesture: live clock moved past START, latches.
        assert!(esc_edge_cancels(start, clock.load()));
        assert!(esc_edge_cancels(start, edge));
        // Ended gesture: compares the END-sampled sequence, so only the END
        // sample decides; a later edge does not rewrite the comparison.
        let end_sample = clock.load();
        clock.bump();
        assert!(!esc_edge_cancels(start, start));
        assert!(esc_edge_cancels(start, end_sample));
        assert!(!esc_edge_cancels(end_sample, end_sample));
    }

    #[test]
    fn lifecycle_failed_release_retains_stopping() {
        let mut life = PumpLifecycle::new();
        assert_eq!(life.state(), PumpState::Stopped);
        life.start().expect("start");
        assert_eq!(
            life.start().expect_err("overlap"),
            LifecycleError::AlreadyActive
        );
        life.note_installed().expect("ready");
        life.request_stop().expect("stop");
        assert_eq!(life.state(), PumpState::Stopping);
        // Failed release: stays stopping, reports unclean, never frees the
        // worker slot.
        let report = life.note_unhooked(false).expect("ack completes");
        assert!(!report.clean && !report.unhook_ok);
        assert_eq!(life.state(), PumpState::Stopping);
        assert_eq!(
            life.start().expect_err("no overlap"),
            LifecycleError::AlreadyActive
        );
        // Missing release is the same state: still stopping, still guarded.
        // Retry stays idempotent, then the later ack completes the stop.
        life.request_stop().expect("re-drive");
        assert_eq!(life.state(), PumpState::Stopping);
        let report = life.note_unhooked(true).expect("ack");
        assert!(report.clean && report.unhook_ok);
        assert_eq!(life.state(), PumpState::Stopped);
        // Install failure before readiness frees nothing held: retry allowed.
        life.start().expect("start");
        life.install_failed().expect("failure");
        assert_eq!(life.state(), PumpState::Stopped);
        life.start().expect("retry after failure");
        assert_eq!(
            PumpLifecycle::new()
                .request_stop()
                .expect_err("nothing running"),
            LifecycleError::NothingPending
        );
    }

    #[test]
    fn try_exchange_never_waits_and_preserves_order() {
        let inbox = Inbox::new();
        let mut slot = None;
        assert!(inbox.try_push(InboxItem::ClearEsc, &mut slot));
        assert!(slot.is_none());
        let drained = inbox.try_drain().expect("acquired");
        assert_eq!(drained, vec![InboxItem::ClearEsc]);
        for _ in 0..INBOX_CAP {
            let mut s = None;
            assert!(inbox.try_push(InboxItem::ClearEsc, &mut s));
        }
        let mut full_slot = None;
        assert!(!inbox.try_push(InboxItem::ClearEsc, &mut full_slot));
        assert_eq!(full_slot, Some(InboxItem::ClearEsc));
        let outbox: Outbox<u8> = Outbox::new();
        assert!(outbox.try_put(7).expect("offer"));
        assert_eq!(outbox.try_occupied(), Some(true));
        assert_eq!(outbox.try_put(9).expect_err("occupied"), 9);
    }

    #[test]
    fn diag_slot_publishes_without_waiting() {
        use super::{DiagSlot, InputDiag};
        use std::time::Instant;
        let slot = DiagSlot::new();
        let mut diag = InputDiag {
            snap_dropped: 3,
            ..InputDiag::default()
        };
        diag.note_offer(150, Instant::now());
        assert!(slot.try_publish(&diag));
        let read = slot.try_snapshot().expect("read");
        assert_eq!(read.snap_dropped, 3);
        assert_eq!(read.offers, 1);
        assert_eq!(slot.snapshot().offers, 1);
    }

    #[test]
    fn policy_fence_refuses_fresh_but_keeps_held_pairs_and_mask() {
        // Production fence helpers under test (the real policy gates call
        // these): fresh refuses fail-closed while pending, held pairs ride
        // stored verdicts, and the mask obligation survives. Generation
        // allocated above the live maxima under the serial guard, published
        // and applied before release, so parallel tests cannot disturb the
        // window and no pending fence leaks out.
        use super::{
            fence_test_allocate, fence_test_serial, keyboard_fresh_gate, note_policy_applied,
            note_policy_published, policy_fence_pending,
        };
        let _guard = fence_test_serial();
        let base = fence_test_allocate();
        note_policy_applied(base);
        assert!(!policy_fence_pending());
        assert!(keyboard_fresh_gate(true));
        // Owner publishes a newer policy: fresh refuses until the pump
        // applies it.
        note_policy_published(base + 1);
        assert!(policy_fence_pending());
        assert!(!keyboard_fresh_gate(true));
        // A fresh chord under the fence passes through tracked (pass verdict
        // retained, never converted into a swallow).
        let mut machine = SnapClassify::new(KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        });
        machine.set_gate_active(true);
        let mut queue = SnapQueue::new();
        win_down(&mut machine);
        assert_eq!(
            chord(&mut machine, &mut queue, VK_H, keyboard_fresh_gate(true)),
            Some(false)
        );
        assert!(machine.key_is_down(VK_H));
        // A consumed hold from before the fence rides its stored verdict
        // through repeats and the paired Up, with the mask obligation.
        let mut held = SnapClassify::new(KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        });
        held.set_gate_active(true);
        let mut held_queue = SnapQueue::new();
        win_down(&mut held);
        assert_eq!(chord(&mut held, &mut held_queue, VK_H, true), Some(true));
        assert_eq!(
            chord(&mut held, &mut held_queue, VK_H, keyboard_fresh_gate(true)),
            Some(true)
        );
        assert!(win_up_mask_reserve(
            &mut held,
            &mut held_queue,
            VK_LWIN,
            true,
            Instant::now()
        ));
        assert_eq!(
            chord_up(&mut held, &mut held_queue, VK_H, keyboard_fresh_gate(true)),
            Some(true)
        );
        // Pump applies the policy: fresh consumption resumes.
        note_policy_applied(base + 1);
        assert!(!policy_fence_pending());
        assert!(keyboard_fresh_gate(true));
    }

    #[test]
    fn fence_base_restarts_rerun_above_global_maxima() {
        // A second same-process owner run starts its local generation at the
        // process-global maximum so its first policy bump re-arms the fence
        // instead of idling below a pinned `fetch_max` ceiling.
        let _guard = fence_test_serial();
        let base = fence_test_allocate();
        note_policy_applied(base);
        assert!(!policy_fence_pending());
        assert_eq!(fence_base_generation(), base);
        // The rerun's first bump lands above and pairs exactly once.
        note_policy_published(base + 1);
        assert!(policy_fence_pending());
        note_policy_applied(base + 1);
        assert!(!policy_fence_pending());
    }

    #[test]
    fn pump_slot_blocks_overlap_until_explicit_clean_release() {
        // Process-level no-overlap guard with a fake seam (no threads, no
        // hooks): reserve refuses while held (hung/unclean stop, including a
        // dropped handle, keeps the reserve since only an explicit release
        // frees it), and a clean stop or spawn failure releases for reuse.
        let _guard = pump_slot_test_serial();
        assert!(!pump_slot_reserved());
        assert!(pump_slot_try_reserve());
        assert!(pump_slot_reserved());
        // Overlapping spawn refuses; dropping the holder changes nothing
        // (no `Drop` release by design).
        assert!(!pump_slot_try_reserve());
        assert!(pump_slot_reserved());
        // Clean stop releases; the next run reserves fresh.
        pump_slot_release();
        assert!(!pump_slot_reserved());
        assert!(pump_slot_try_reserve());
        pump_slot_release();
        assert!(!pump_slot_reserved());
    }
}
