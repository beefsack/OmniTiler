#![forbid(unsafe_code)]

//! Shared runtime lifecycle state (ownership, watermark, logging).
//!
//! Split from [`crate::runtime`]: the single shared [`Shared`] cell plus
//! its lock helpers, arrival-clock advance, ring logging, and admission
//! liveness — the state every submitter leg, worker rebind path, and engine
//! fence reads. [`crate::runtime`] keeps the client façade
//! ([`crate::runtime::RuntimeHandle`]); per-app lifetime admission lives in
//! [`crate::runtime_apps`]. No threads are spawned here; no behavior changes
//! with the move (call sites keep working through `runtime` re-exports).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex, atomic::AtomicBool};
use std::time::Instant;

use crate::runtime_lifecycle::{
    EngineExit, InputExit, LiveHandle, LoggerExit, ShutdownReport, WorkerExit,
};
use crate::runtime_log::LogEvent;
use crate::runtime_thread::{FocusSlot, FrameSlot};
use crate::runtime_types::{
    ClaimedSelect, DesiredFrame, FocusEvidence, FocusRequest, LifecycleLogEntry, NativeContext,
    RuntimeConfig, RuntimeDiagnostics,
};
use crate::topology::TopologySnapshot;
use crate::world_sequencer::ObservationClock;

/// Cap for the structured lifecycle log; oldest entries rotate out.
const LOG_CAP: usize = 512;

pub(crate) struct AppRecord {
    pub(crate) generation: u64,
    pub(crate) terminated: bool,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) frames: Arc<FrameSlot>,
    /// Latest-wins focus slot for the owning AX worker (the SAME owner
    /// that observed the window dispatches focus). Stopped alongside
    /// `frames` on terminate/shutdown/drop so queued focus never outlives
    /// its lifetime.
    pub(crate) focus: Arc<FocusSlot>,
    /// Backend readiness set by the worker inside its owning thread: `None`
    /// while the backend is still being created, `Ok(())` once it exists,
    /// `Err` when creation was refused. Polled by [`RuntimeHandle::launch_app`]
    /// (bounded) and [`RuntimeHandle::worker_ready`].
    pub(crate) ready: Arc<Mutex<Option<Result<(), String>>>>,
    pub(crate) worker: Option<std::thread::JoinHandle<WorkerExit>>,
    pub(crate) worker_name: String,
}

/// Latest-wins AppKit display legs for the send effect owner; published
/// at startup, on screen-parameter notification, and to recover a cleared
/// slot — never on a quiet populated poll. Cleared on invalidation.
#[derive(Debug, Clone, Default)]
pub struct PublishedDisplayInputs {
    pub displays: Vec<crate::topology::DisplayGeometry>,
    pub separate: Option<bool>,
}

/// Live native-context cell: the single shared source every setter leg,
// the worker rebind path, and the engine read on every fence. Replaced
// wholesale per snapshot, never mutated in place, so readers always see
// one consistent revision. Plain data only.
//
// The cell also carries a coalesced invalidation generation: every
// revision-changing publication (a new fingerprint, a clear, or a
// clear-then-republish even at an identical revision) plus every verified
// arrival request bumps it exactly once. Bursts coalesce (many bumps, one
// pending mark). Each per-app owner consumes the generation once per pump:
// a bump requests one whole-owner reread on the next owner pump even when
// the backstop is silent and the revision is unchanged (arrival). The
// generation is consumed even when the reread fails (no stale retry
// loops); the backstop recovers and only a new bump rereads again.
#[derive(Debug, Clone, Default)]
pub struct NativeContextCell(Arc<Mutex<CellState>>);

#[derive(Debug, Clone, Default)]
struct CellState {
    ctx: Option<Arc<NativeContext>>,
    generation: u64,
}

impl NativeContextCell {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish a freshly derived context (or clear it with `None`). The
    /// generation bumps exactly when the effective publication changes: a
    /// `None`-to-`Some`/`Some`-to-`None` transition or a revision move. A
    /// same-revision republish (including a recovered snapshot after a
    /// clear, which transitions `None`-to-`Some` and therefore bumps)
    /// never double-counts steady state.
    pub fn store(&self, ctx: Option<Arc<NativeContext>>) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let changed = match (&state.ctx, &ctx) {
            (None, None) => false,
            (None, Some(_)) | (Some(_), None) => true,
            (Some(current), Some(next)) => current.revision != next.revision,
        };
        state.ctx = ctx;
        if changed {
            state.generation = state.generation.wrapping_add(1);
        }
    }

    /// Request one whole-owner reread without a topology change (verified
    /// native arrival): unconditionally bumps the generation once. Bursts
    /// coalesce into the single pending mark the owners consume once.
    pub fn request_reread(&self) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        state.generation = state.generation.wrapping_add(1);
    }

    /// Load the current context, if any.
    #[must_use]
    pub fn load(&self) -> Option<Arc<NativeContext>> {
        self.load_versioned().0
    }

    /// Load the current context plus its invalidation generation under one
    /// lock, so an owner never pairs a context with a newer generation
    /// than the one it consumed.
    #[must_use]
    pub fn load_versioned(&self) -> (Option<Arc<NativeContext>>, u64) {
        let state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        (state.ctx.clone(), state.generation)
    }

    /// Current invalidation generation (brief lock; owners prefer
    /// [`NativeContextCell::load_versioned`] to pair it with the context).
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .generation
    }
}

/// Deterministic-test rendezvous invoked by the engine thread once per
/// opened world flight (after the ticket stamps the current watermark,
/// before the report builds), with no locks held. A test uses it to
/// enqueue racing input mid-flight over channels — no sleeps, no live
/// desktop — proving the current-watermark fence refuses the stale reply
/// and the queued successor applies newest-only with no stale emission.
/// Production never sets this; unset with
/// [`RuntimeHandle::set_midflight_hook_for_test`]`(None)`.
pub type MidflightHook = Arc<dyn Fn() + Send + Sync>;

/// Deterministic-test rendezvous invoked by the engine thread once per
/// accepted world reply (after sequencer acceptance, before the
/// emission-coherence gate), with no locks held. A test uses it to advance
/// the watermark between acceptance and emission — no sleeps, no live
/// desktop — proving accepted-but-superseded replies never reach native
/// setters. Production never sets this; unset with
/// [`RuntimeHandle::set_post_accept_hook_for_test`]`(None)`.
pub type PostAcceptHook = Arc<dyn Fn() + Send + Sync>;

/// Bind-time focus baselines for one deferred selection intent: the owning
/// app generation, the engine's worker-gated observed global focus, and
/// the main-thread foreground PID — all read at dispatch. `None` in any
/// of them refuses the bind (no baseline, no intent): never guessed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusBaselines {
    pub app_generation: Option<u64>,
    pub prior_focus: Option<String>,
    pub prior_frontmost: Option<u32>,
}

/// Correlation-bound deferred selection focus: one explicit-focus outcome
/// of a planner-owned view switch (the target domain's remembered leaf),
/// held after the engine reply until the native Dock-swipe selection proves
/// arrival. Released only by the matching select completion's verified post
/// (the post names the bound native Space); dropped on refusal, on
/// supersession by a newer switch or explicit focus, on a stale app, on a
/// user focus move to another app, or when the post never proves arrival.
/// Never dispatched before verified arrival, never on the wrong Space,
/// never replayed (at-most-once: the slot is taken before enqueue).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeferredSelectionFocus {
    /// Stable world-command operation identity (`op-<command_seq>`).
    pub operation_id: String,
    pub command_seq: u64,
    /// Core-correlated focus target (`"<pid>_<wid>"`).
    pub window: tiler_core::directional::WindowId,
    pub route: crate::focus_route::FocusRoute,
    /// Native Space the selection must arrive on before focus releases.
    pub expected_sid: u64,
    /// Owning app generation at bind (`None` when the app was unknown
    /// then): release requires a live, unterminated record with this exact
    /// generation, so PID reuse or termination drops the intent.
    pub app_generation: Option<u64>,
    /// Observed global focus (`"<pid>_<wid>"`) at bind, from the engine's
    /// worker-gated observed truth: the owner worker refuses to dispatch
    /// when the agreed selected focus names neither this prior nor the
    /// target, closing the same-app window race the main thread cannot
    /// see. `None` is never bound (bind refuses without a baseline).
    pub prior_focus: Option<String>,
    /// Main-thread foreground PID at bind: release requires the same
    /// foreground at completion (a move in between proves user input and
    /// drops the intent). `None` is never bound (bind refuses without a
    /// stability baseline).
    pub prior_frontmost: Option<u32>,
}

/// State key grounding held-reply geometry retention: the current
/// (output, logical workspace) scopes, the reply's workspace bindings,
/// window homing, and the retained current native workareas plus full
/// bounds per current scope. A held reply under an identical key preserves
/// the last applied authorized geometry (same view, same bindings, same
/// lifetimes, same workareas); any change clears it. Workarea-only
/// topology movement during a hold clears the retention (stale geometry is
/// never kept across a dock/resolution change); the global topology
/// revision is deliberately NOT part of the key, so an own-append revision
/// bump with unchanged scopes and geometry keeps retention instead of
/// churning. Adapter-owned, plan-truth only — the renderers still
/// revalidate every shape against live native legs before painting, so
/// retention can never resurrect a stale overlay.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AppliedGeometryKey {
    pub(crate) current: BTreeSet<(String, String)>,
    pub(crate) bindings: Vec<(String, String, Option<String>)>,
    pub(crate) homing: BTreeMap<String, (String, String)>,
    /// Per-current-scope native geometry: `(visible work area, full
    /// bounds)` in integer CG top-left points, keyed by (output, logical
    /// workspace). Sourced from the retained current native domains at
    /// reply-consume time; empty while native context is absent (synthetic
    /// mode), where the other key legs already discriminate.
    pub(crate) workareas:
        BTreeMap<(String, String), (tiler_core::geometry::Rect, tiler_core::geometry::Rect)>,
}

pub(crate) struct Shared {
    pub(crate) config: RuntimeConfig,
    /// Live [`RuntimeHandle`] count, held solely by handles (clones bump,
    /// drops release; supervised threads never touch it). The final owning
    /// handle drop tears down; borrowed clone drops are no-ops — unlike a
    /// `shared` strong-count, which threads also hold.
    pub(crate) handles_live: usize,
    pub(crate) generation_counter: u64,
    pub(crate) log_counter: u64,
    pub(crate) start: Instant,
    pub(crate) apps: BTreeMap<u32, AppRecord>,
    /// Shared world input watermark: every invalidating input submitter
    /// (AX workers, input thread, main-thread controller) advances this
    /// clock when its input is actually enqueued — including while a world
    /// request is outstanding — so an in-flight reply whose stamped
    /// observation no longer matches refuses as stale instead of actuating
    /// over newer input. Never advanced per dispatched job (a dispatch with
    /// no new input must not move the mark); reports stamp only actually
    /// submitted input.
    pub(crate) clock: Arc<ObservationClock>,
    /// Deterministic-test rendezvous: when set, the engine thread invokes
    /// the hook once per opened flight (after the ticket stamps the current
    /// watermark, before the report builds) with no locks held, so a test
    /// can enqueue racing input mid-flight over channels. Production never
    /// sets this; see `set_midflight_hook_for_test`.
    pub(crate) midflight_hook: Mutex<Option<MidflightHook>>,
    /// Deterministic-test rendezvous: when set, the engine thread invokes
    /// the hook once per accepted world reply (after sequencer acceptance,
    /// before the emission-coherence gate spends or emits anything) with
    /// no locks held, so a test can advance the watermark between
    /// acceptance and emission over channels. Production never sets this;
    /// see `set_post_accept_hook_for_test`.
    pub(crate) post_accept_hook: Mutex<Option<PostAcceptHook>>,
    /// Current native topology snapshot (the main-thread controller owns
    /// the cadence). `None` until the first
    /// [`RuntimeHandle::update_topology`]; synthetic mode never sets it.
    pub(crate) topology: Option<TopologySnapshot>,
    /// Latest-wins AppKit display legs; see [`PublishedDisplayInputs`].
    pub(crate) display_inputs: Option<PublishedDisplayInputs>,
    /// Latest-wins active-Space wake shared with off-main effect owners
    /// (select settlement and the send-stay return wait on its generation).
    /// Published once after the main-thread display guard installs;
    /// `None` until then (both paths refuse honestly, never guessed).
    pub(crate) select_wake: Option<crate::ffi::SpaceSelectWake>,
    /// Current display/Space revision fence. Synthetic mode holds 0 so every
    /// legacy command (revision 0) passes; native mode tracks the latest
    /// snapshot fingerprint.
    pub(crate) topology_revision: u64,
    /// Derived native tile context (per-display current-user-Space domains
    /// in integer work areas), published by the main-thread supplier
    /// alongside the snapshot. Retired by [`NativeContextCell`] below;
    /// kept here so the engine can fence without an extra hop.
    /// `None` until the first successful derivation; scoped observations
    /// and effects fence while it is absent.
    pub(crate) native_context: Option<Arc<NativeContext>>,
    /// Live cell handle cloned into per-app backends at bind time. The
    /// single shared source for every setter-leg fence read; written
    /// synchronously by [`RuntimeHandle::update_topology`] (and cleared by
    /// [`RuntimeHandle::invalidate_topology`]), so a topology update
    /// mid-plan fences the very next leg — not just the next command.
    pub(crate) native_cell: NativeContextCell,
    /// One pending product select claim (armed select-only actions): set by
    /// the engine without blocking, claimed and drained by the main-thread
    /// tile controller. `select_busy` marks a drain in progress so
    /// concurrent presses serialize/refuse instead of overlapping; both are
    /// short critical sections only (never held across the blocking effect).
    /// Indicator clicks ride as full [`ClaimedSelect`] bindings (expected
    /// SID plus UI generation plus topology revision); tap chords ride
    /// binding-free. The drain refuses a bound claim whose ordinal no
    /// longer names its SID or whose revision drifted.
    pub(crate) select_pending: Option<ClaimedSelect>,
    pub(crate) select_busy: bool,
    /// Main-thread wake hint for the pending claim above: set once when the
    /// engine publishes an actionable claim, consumed once by the main pump
    /// (`take_select_claim_pending`) to run the cadence body early instead
    /// of waiting for the next 250 ms tick. Coalesced (bursts merge into one
    /// wake); cleared whenever the slot is claimed, dropped, or settled
    /// away, so a reconciled claim never causes a spurious extra cadence
    /// run. Never a timer, never a new thread: one bool checked on the
    /// existing 50 ms pump under the same short lock as the slot itself.
    pub(crate) select_claim_pending: bool,
    /// Deferred selection focus bound at engine-reply dispatch (see
    /// [`DeferredSelectionFocus`]): at most one, latest-wins. Keyed by
    /// operation identity so only the matching select completion can
    /// release it; anything else supersedes or drops it. Shared between
    /// the engine thread (bind/drop) and the main-thread tile controller
    /// (release on the verified completion post).
    pub(crate) selection_focus_pending: Option<DeferredSelectionFocus>,
    /// Monotonic supersession floor for deferred selection focus: bumped
    /// by every selection bind and every explicit-focus dispatch. A select
    /// completion releases focus only when its command seq reaches the
    /// floor; anything below was superseded by newer input and drops
    /// counted instead of ever reapplying.
    pub(crate) selection_focus_floor_seq: u64,
    /// Main-thread-supplied global focus evidence (workspace-frontmost PID
    /// plus bundle, plain IDs — never AX reads: the main thread performs no
    /// AX under any cadence). The AX focused leg (`ax_focus`) is NOT read on
    /// the main thread: each owning per-app AX worker reads the bounded
    /// systemwide leg on its own thread (`sys_focus`) and stamps it over
    /// `ax_focus` at send time, so a hung app can never stall the
    /// controller. The engine promotes an app-local focused window to
    /// observed truth only on full agreement; `None` (no evidence yet)
    /// withholds promotion, never guesses.
    pub(crate) focus_evidence: Option<FocusEvidence>,
    pub(crate) zombies: Vec<LiveHandle>,
    pub(crate) engine_handle: Option<std::thread::JoinHandle<EngineExit>>,
    pub(crate) input_handle: Option<std::thread::JoinHandle<InputExit>>,
    pub(crate) logger_handle: Option<std::thread::JoinHandle<LoggerExit>>,
    /// Parked input thread for prompt stop wake (`unpark` after `stop_input`
    /// is set; an `unpark` before `park` grants a permit, so no wake is
    /// lost).
    pub(crate) input_thread: Option<std::thread::Thread>,
    /// Cached terminal shutdown report: a repeated stop replays the exact
    /// terminal report instead of hiding a panic/timeout/unclean behind
    /// `AlreadyStopped`.
    pub(crate) terminal: Option<ShutdownReport>,
    pub(crate) admission_closed: bool,
    pub(crate) stop_engine: Arc<AtomicBool>,
    pub(crate) stop_input: Arc<AtomicBool>,
    pub(crate) stop_logger: Arc<AtomicBool>,
    pub(crate) log_tx: Option<std::sync::mpsc::SyncSender<LogEvent>>,
    pub(crate) diagnostics: RuntimeDiagnostics,
    pub(crate) log: VecDeque<LifecycleLogEntry>,
    /// Test-only wake channel for [`crate::runtime::RuntimeHandle`]
    /// condition waits (`wait_diagnostics_for_test` and siblings): `None`
    /// in production (every mutation site pays only this `None` check, no
    /// timers, no threads, no periodic work). The first `wait_*_for_test`
    /// call arms it lazily under the shared lock; every relevant state
    /// mutation then bumps [`Shared::test_seq`] and `notify_all`s under
    /// the same lock, so a waiter that snapshots `(state, seq)` and
    /// rechecks before parking can never miss a wake. Predicates evaluate
    /// on owned snapshots with the lock released (they may call back into
    /// handle snapshot readers - never a blocking wait); the pre-park
    /// sequence recheck closes the evaluate-then-park race.
    pub(crate) test_wake: Option<Arc<Condvar>>,
    /// Monotonic mutation sequence for test-only condition waits: bumped
    /// under the shared lock by [`Shared::notify_test_waiters`] on every
    /// notifying mutation. A waiter re-locks after evaluating its
    /// predicate and parks only when `seq` is unchanged - a changed `seq`
    /// means a mutation landed in between, so the waiter re-snapshots
    /// instead of parking past it.
    pub(crate) test_seq: u64,
    pub(crate) last_desired: Vec<DesiredFrame>,
    /// State key for the last applied reply's geometry (current scopes,
    /// workspace bindings, window homing): a same-key held reply retains
    /// the last applied authorized geometry instead of publishing empty
    /// shapes (see [`crate::world_dispatch::consume_world_reply`]). Any
    /// view, binding, or lifetime change moves the key and clears the
    /// retention — never a stale overlay, never an off-current write.
    pub(crate) last_geometry_key: Option<AppliedGeometryKey>,
    /// Latest applied reply presentation (border/underlay/pager shapes plus
    /// the tick-time foreground token). The engine publishes exactly the
    /// applied reply's presentation here; the tile main thread renders only
    /// these shapes through the existing native carriers, revalidating the
    /// live foreground against the token before painting.
    pub(crate) last_presentation: tiler_core::world_presentation::WorldPresentation,
    /// Latest applied reply workspace bindings for the menu-bar pager: the
    /// stable core logical workspace id per native desktop, translated
    /// through the reply's `native_binding` rows (current AND hidden
    /// domains — the indicator shows every Space). Published next to the
    /// presentation so the tick maps native SIDs to pager content with a
    /// current-reply live lookup, never a durable host table.
    pub(crate) last_pager_bindings: Vec<crate::pager::PagerWorkspaceBinding>,
    /// Startup-resolved presentation style numbers for the report ingress
    /// (whole-point rounded width/gap plus the ring switch). Published
    /// once by the tile main thread at startup; the engine copies it into
    /// every report so core shapes use the configured outset.
    pub(crate) presentation_style: crate::active_border::PresentationStylePx,
}

/// Lock the shared state, recovering from poisoning so stop/reap/diagnostics
/// stay usable after a supervised thread panic. Panics never hold this lock
/// (workers panic inside backends, the engine panics outside the lock), so
/// recovery only inherits already-committed counters.
pub(crate) fn lock_shared(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Lock the focus outbox with the same poison recovery.
pub(crate) fn lock_focus(
    queue: &Mutex<Vec<FocusRequest>>,
) -> std::sync::MutexGuard<'_, Vec<FocusRequest>> {
    queue.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Advance the shared world input watermark. Call immediately BEFORE
/// enqueueing invalidating input (never after: the engine may dequeue the
/// moment the job lands, so the mark must already name it — otherwise the
/// job's own dispatch tickets a stale mark and refuses itself). Never on
/// refusal paths that enqueue nothing. On a saturated queue the mark may
/// over-advance for a dropped job; that direction is conservative (one
/// extra observation-only refusal, never stale actuation), and the
/// submitter is told to retry. Lock-free; safe mid-flight from any thread.
pub(crate) fn invalidate_clock(shared: &Mutex<Shared>) {
    lock_shared(shared).clock.invalidate();
}

impl Shared {
    /// Wake any armed test-only condition waiter. Call with the shared
    /// lock held: bumps the waiter sequence first so a waiter that
    /// evaluated its predicate on a pre-mutation snapshot re-snapshots
    /// instead of parking past the wake, then notifies. Production pays
    /// only the `None` check when unarmed.
    pub(crate) fn notify_test_waiters(&mut self) {
        self.test_seq += 1;
        if let Some(cv) = self.test_wake.clone() {
            cv.notify_all();
        }
    }

    /// Push one ring entry and forward its copy to the bounded async sink
    /// under the SAME lock. Sequence allocation, ring order, and channel
    /// enqueue order are therefore identical, so the sink observes the same
    /// monotonic correlation sequence as the ring. The enqueue is a single
    /// `try_send` does not wait for queue space or sink I/O: a full or gone
    /// queue drops the async copy counted. Only the dedicated logger thread
    /// calls the sink's `write`. The ring entry is always kept. See [`emit`].
    pub(crate) fn log(
        &mut self,
        pid: u32,
        generation: u64,
        ref_id: u64,
        kind: &'static str,
        outcome: &'static str,
        count: u64,
    ) {
        self.log_counter += 1;
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
            self.diagnostics.log_rotated += 1;
        }
        let timestamp_ms = self.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let entry = LifecycleLogEntry {
            seq: self.log_counter,
            timestamp_ms,
            pid,
            generation,
            ref_id,
            kind,
            outcome,
            count,
        };
        self.log.push_back(entry);
        let event = LogEvent {
            seq: self.log_counter,
            timestamp_ms,
            pid,
            generation,
            ref_id,
            kind,
            outcome,
            count,
        };
        if let Some(tx) = self.log_tx.clone()
            && tx.try_send(event).is_err()
        {
            self.diagnostics.logger_dropped += 1;
        }
        self.notify_test_waiters();
    }

    pub(crate) fn count_reply(&mut self, token: &str) {
        *self
            .diagnostics
            .core_replies
            .entry(token.to_owned())
            .or_insert(0) += 1;
        self.notify_test_waiters();
    }

    pub(crate) fn count_completion(&mut self, token: &str) {
        *self
            .diagnostics
            .completions
            .entry(token.to_owned())
            .or_insert(0) += 1;
        self.notify_test_waiters();
    }
}

/// Append one ring entry and forward its copy to the bounded async sink
/// under the SAME shared lock. Sequence allocation, ring order, and channel
/// enqueue order are therefore identical, so the sink observes the same
/// monotonic correlation sequence as the ring. The enqueue is a single
/// `try_send` (does not wait for queue space: a full or gone queue drops the
/// async copy counted via `logger_dropped`); the ring entry is always kept.
/// Only the dedicated logger thread ever blocks, in `write` — never an
/// engine/input/app owner holding shared state.
pub(crate) fn emit(
    shared: &Arc<Mutex<Shared>>,
    pid: u32,
    generation: u64,
    ref_id: u64,
    kind: &'static str,
    outcome: &'static str,
    count: u64,
) {
    lock_shared(shared).log(pid, generation, ref_id, kind, outcome, count);
}

/// True while the engine owner is alive (spawned and not proven exited).
/// Admission consults this so work is refused — never queued — once the
/// engine is dead.
pub(crate) fn engine_alive(shared: &Mutex<Shared>) -> bool {
    let guard = lock_shared(shared);
    if guard.admission_closed {
        return false;
    }
    guard
        .engine_handle
        .as_ref()
        .is_none_or(|h| !h.is_finished())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(revision: u64) -> Arc<NativeContext> {
        Arc::new(NativeContext {
            revision,
            domains: Vec::new(),
            skipped: Vec::new(),
        })
    }

    #[test]
    fn generation_bumps_only_on_effective_publication_change() {
        let cell = NativeContextCell::new();
        assert_eq!(cell.generation(), 0);
        assert_eq!(cell.load_versioned().1, 0);
        // Clearing an empty cell never bumps (no spurious reread).
        cell.store(None);
        assert_eq!(cell.generation(), 0);
        // Initial publication bumps once.
        cell.store(Some(ctx(7)));
        assert_eq!(cell.generation(), 1);
        // A same-revision republish never bumps (steady state stays quiet).
        cell.store(Some(ctx(7)));
        assert_eq!(cell.generation(), 1);
        // A revision move bumps.
        cell.store(Some(ctx(8)));
        assert_eq!(cell.generation(), 2);
        // A clear bumps (Some-to-None): the loss is consumable once.
        cell.store(None);
        assert_eq!(cell.generation(), 3);
        // Clearing an already-cleared cell never bumps.
        cell.store(None);
        assert_eq!(cell.generation(), 3);
        // A recovered snapshot republishes even at the pre-failure
        // revision (None-to-Some): the owners reread the restored truth.
        cell.store(Some(ctx(8)));
        assert_eq!(cell.generation(), 4);
    }

    #[test]
    fn arrival_request_bumps_and_pairs_with_the_context() {
        let cell = NativeContextCell::new();
        cell.store(Some(ctx(7)));
        let base = cell.generation();
        // Bursts bump once each and the owner consumes the run as one
        // pending reread; the paired load never splits the context from
        // its generation.
        cell.request_reread();
        cell.request_reread();
        cell.request_reread();
        assert_eq!(cell.generation(), base + 3);
        let (loaded, generation) = cell.load_versioned();
        assert_eq!(loaded.map(|c| c.revision), Some(7));
        assert_eq!(generation, base + 3);
        assert_eq!(cell.load().map(|c| c.revision), Some(7));
    }
}
