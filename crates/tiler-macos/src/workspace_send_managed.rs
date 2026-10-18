#![forbid(unsafe_code)]

//! Managed numbered send-follow effect seams plus the dedicated async
//! effect thread.
//!
//! Numbered send-follow 1..9 is armed; reaching the live path happens
//! through engine-admitted product claims (tests inject claims the same
//! way). Nothing here enables a holder or
//! posts input: the tile ensures Authentic holders once at startup
//! (Compatible stays zero-touch), and posts run only on the dedicated
//! owner through [`LiveManagedSendBackend`](crate::workspace_send_native::LiveManagedSendBackend)
//! under consumed per-edge proof permits.
//!
//! What this module holds over [`crate::workspace_send_effect`]:
//!
//! - [`ManagedSendBackend`]: the injectable seam the dedicated effect owner
//!   runs. Fresh reads (gates, row, holder, per-edge proof
//!   evidence) plus the release-first drag posts plus the bounded
//!   post-release readback. Fakes script every leg; the live backend
//!   ([`crate::workspace_send_native`]) serves fresh engine facts, native
//!   reads, and guarded HID posts here — never on the engine, AppKit, or
//!   per-app observation owners.
//! - [`ManagedPostPermit`]: the single-use per-edge product-post authority.
//!   Built only by [`authorize_managed_edge`] over [`prove_managed_send`]:
//!   runtime scope/current membership, ordinary visible non-fullscreen
//!   state, unchanged lifetime/topology, pinned source homing, fresh focus
//!   naming the exact target with SLPS/direct-AX/NSWorkspace agreement,
//!   fresh CG ownership/geometry, the backend-selected fresh grab point,
//!   and both positional AX and WindowServer hits at that exact point.
//!   Each permit binds the exact grab point plus the exact resolved holder
//!   chord for one edge and is consumed by value at the post boundary —
//!   no `Copy`/`Clone` reuse, no free point/chord arguments after proof.
//!   Down-edges (`begin_leg`, grab move/down/dragged, chord-down) require
//!   this permit; release edges (mouse-up/key-up) bypass it and always
//!   attempt. [`crate::synth::OwnedPostTarget`] is fixture
//!   ONLY and is never constructed, forged, or bypassed here (see the
//!   `managed_never_touches_fixture_post_target` test).
//! - [`ProvingDragAdapter`](ProvingWithChords): re-proves the applicable
//!   identity/focus/point plus the exact resolved holder chord immediately
//!   before every down-edge (`begin_leg`, move, down, dragged, chord-down)
//!   and always attempts the release edges (`MouseUp`, key-up) regardless
//!   of a stop request. Cancellation ([`SendEffectHandoff::cancel_requested`])
//!   stops further down-edges but never skips a held-input release.
//! - [`execute_managed_send`]: one pinned-target effect on the calling
//!   (effect-owner) thread: gates, single-display rule, per-edge proof, fresh
//!   holder resolve, one [`run_drag_leg`](crate::space_drag_transfer::run_drag_leg)
//!   with bounded post-release settle polling, exact membership-plus-current
//!   readback. Pre-actuation refusals (gates, display, proof, holder, setup,
//!   grab, chord-down, cancel-before-chord) report
//!   [`SendEffectOutcome::RefusedProven`]; completed chords without exact
//!   readback, unreadable arrival, release residue, or cancel-after-chord
//!   report [`SendEffectOutcome::PossiblyExecuted`] without replay. Only
//!   exact arrival reports [`SendEffectOutcome::Applied`].
//! - [`ManagedSendThread`]: the dedicated scoped effect thread. Owns its
//!   backend (built on the effect thread via the spawn factory, so fresh
//!   reads and posts never run on the engine/AppKit/observer owners).
//!   Submission and stop are event-driven (channel wake, no idle sleep);
//!   the engine/main threads only submit nonblocking
//!   ([`ManagedSendThread::submit`]) and poll the shared lane nonblocking
//!   ([`SendEffectHandoff::poll_completion_for`]); no fresh proof polling
//!   or waits happen on main. The existing bounded post-release settle poll
//!   stays inside `run_drag_leg` on this thread and is not a proof cache:
//!   no periodic proof query is added. Cancellation is request-to-stop;
//!   [`ManagedSendThread::request_shutdown`] signals stop, cancels the lane,
//!   and wakes the owner without joining, while
//!   [`ManagedSendThread::try_reap`] joins only once the owner already
//!   finished and only then releases the lane - the caller can never release
//!   while a live owner may still actuate. Never join per-cadence on the
//!   main loop, never abandon a held input (the release-first executor plus
//!   unconditional UP attempts guarantee the mouse releases before the
//!   terminal completion). Terminal ownership of an unfinished owner hands
//!   to [`spawn_terminal_reaper`](crate::workspace_send_native::spawn_terminal_reaper),
//!   which joins it on a dedicated thread and only then releases the lane:
//!   no blocking of the engine/AppKit, no panic, no silent detach as
//!   product policy.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use crate::desktop_shortcuts::{ShortcutEntry, resolve_numbered_chord};
use crate::space_drag_transfer::{DragBackend, DragStep, LegError, LegOk, PostCounts};
use crate::workspace_select::SelectionSnapshot;
use crate::workspace_send::SendGates;
use crate::workspace_send_effect::{
    ManagedSendTarget, SendCompletion, SendEffectHandoff, SendEffectOutcome, SendFreshEvidence,
    SendProofRefusal, prove_managed_send,
};

/// Injectable managed-send backend for the dedicated effect owner.
///
/// Every method runs on the effect thread only. Fakes script results;
/// the future live backend would perform fresh reads/posts there.
/// Down-edge posts consume a single-use [`ManagedPostPermit`] by value and
/// must actuate exactly its bound point/chord; release edges are
/// permit-free and unconditional. The live topology revision is fenced only
/// through [`prove_managed_send`] over fresh evidence — no separate
/// `revision()` accessor duplicates that fence.
pub trait ManagedSendBackend: Send {
    /// Fresh session gates sampled on the effect thread (plain booleans).
    fn read_gates(&mut self) -> SendGates;
    /// Fresh single-display user-Space row.
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String>;
    /// Fresh native display-row count (product single-display rule).
    fn display_count(&mut self) -> Result<usize, String>;
    /// Fresh Desktop-N holder read (never a settings write here).
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String>;
    /// Fresh proof evidence for one pinned target (single fresh read set,
    /// never a poll, never cached across edges by the caller — the adapter
    /// calls this before every down-edge). Carries the backend-selected
    /// fresh grab point both hit-tests named.
    fn fresh_evidence(&mut self, target: &ManagedSendTarget) -> SendFreshEvidence;
    /// Select this leg's goal before any post (refusing here posts nothing).
    /// Consumes one proof permit binding the exact point/chord.
    fn begin_leg(&mut self, permit: ManagedPostPermit, goal: u64) -> Result<(), String>;
    /// One grab down-edge at exactly the permit's bound point. Only
    /// `MouseMove`/`MouseDown`/`MouseDragged` are accepted; `MouseUp`
    /// releases through [`ManagedSendBackend::release_mouse`] instead.
    fn post_mouse(&mut self, permit: ManagedPostPermit, step: DragStep) -> Result<(), String>;
    /// One chord-down at exactly the permit's bound chord. Key-up releases
    /// through [`ManagedSendBackend::release_key`] instead.
    fn post_key_down(&mut self, permit: ManagedPostPermit) -> Result<(), String>;
    /// Unconditional mouse release: always attempted, never proof-gated.
    fn release_mouse(&mut self) -> Result<(), String>;
    /// Unconditional key release: always attempted, never proof-gated.
    fn release_key(&mut self) -> Result<(), String>;
    /// Grab settle (50 ms live; fakes override to log-only).
    fn settle_grab(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(
            crate::space_drag_transfer::GRAB_SETTLE_MS,
        ));
    }
    /// Animation hold (400 ms live; fakes override to log-only).
    fn settle_hold(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(
            crate::space_drag_transfer::ANIMATION_HOLD_MS,
        ));
    }
    fn read_membership(&mut self) -> Option<Vec<u64>>;
    fn read_current(&mut self) -> Option<u64>;
    /// Post-release poll window (live 3 s; fakes fast).
    fn effect_deadline_ms(&self) -> u64 {
        crate::space_drag_transfer::EFFECT_SETTLE_MS
    }
    /// Post-release poll slice (live 200 ms; fakes fast).
    fn effect_poll_ms(&self) -> u64 {
        crate::space_drag_transfer::EFFECT_POLL_MS
    }
    fn counts(&self) -> PostCounts;
}

/// Single-use per-edge managed product-post authority.
///
/// Constructible only by [`authorize_managed_edge`], which runs the full
/// [`prove_managed_send`] verdict over fresh evidence and binds the
/// backend-selected fresh grab point plus the exact resolved holder chord.
/// Consumed by value at the post boundary: no `Copy`/`Clone`, so one proof
/// authorizes exactly one edge and can never be replayed. Release edges
/// never take a permit — they always attempt. Private fields force
/// construction through proof — a stale focus window, relaunched lifetime,
/// moved domain, disagreeing hit-test, or missing grab point can never
/// forge a permit.
#[derive(Debug, PartialEq)]
pub struct ManagedPostPermit {
    pid: u32,
    wid: u32,
    point: (f64, f64),
    keycode: u32,
    flags: u64,
}

impl ManagedPostPermit {
    #[must_use]
    pub const fn pid(&self) -> u32 {
        self.pid
    }
    #[must_use]
    pub const fn wid(&self) -> u32 {
        self.wid
    }
    #[must_use]
    pub const fn point(&self) -> (f64, f64) {
        self.point
    }
    #[must_use]
    pub const fn keycode(&self) -> u32 {
        self.keycode
    }
    #[must_use]
    pub const fn flags(&self) -> u64 {
        self.flags
    }
}

/// Prove one actuation edge: the full fresh-proof verdict must name the
/// pinned target at the backend-selected grab point, and the resulting
/// permit binds that exact point plus the caller-supplied exact resolved
/// holder chord for exactly one edge. Release edges never call this.
/// Sequential proof is not atomic: call before every down-edge.
pub fn authorize_managed_edge(
    target: &ManagedSendTarget,
    fresh: &SendFreshEvidence,
    chord: (u32, u64),
) -> Result<ManagedPostPermit, SendProofRefusal> {
    prove_managed_send(target, fresh)?;
    let point = match fresh.grab_point {
        Some((x, y)) if x.is_finite() && y.is_finite() => (x, y),
        _ => return Err(SendProofRefusal::UnknownGrabPoint),
    };
    Ok(ManagedPostPermit {
        pid: target.pid(),
        wid: target.wid(),
        point,
        keycode: chord.0,
        flags: chord.1,
    })
}

/// [`DragBackend`] plumbing for the managed leg: re-proves the pinned target
/// at the backend-selected grab point plus the exact resolved holder chord
/// immediately before every down-edge and always attempts release edges.
///
/// One mouse point throughout a drag: the first `begin_leg` permit pins the
/// selected grab point for this leg, and every later down-edge must prove
/// that same point — a shifted fresh point refuses before the next post
/// instead of silently drifting the held drag. Releases still always run.
///
/// Proof source is the backend's [`ManagedSendBackend::fresh_evidence`]
/// (fresh per edge, never cached). Each down-edge authorizes a single-use
/// [`ManagedPostPermit`] binding that exact point/chord and consumes it by
/// value at the post boundary. A stop request
/// ([`SendEffectHandoff::cancel_requested`]) refuses further down-edges but
/// never skips `MouseUp`/key-up. Key/mouse UP bypass fresh target gates by
/// construction here.
struct ProvingWithChords<'a, 'h, B: ManagedSendBackend> {
    inner: &'a mut B,
    target: &'a ManagedSendTarget,
    handoff: &'h SendEffectHandoff,
    chord: (u32, u64),
    pinned_point: Option<(f64, f64)>,
}

impl<'a, 'h, B: ManagedSendBackend> ProvingWithChords<'a, 'h, B> {
    fn with_chord(
        inner: &'a mut B,
        target: &'a ManagedSendTarget,
        handoff: &'h SendEffectHandoff,
        chord: (u32, u64),
    ) -> Self {
        Self {
            inner,
            target,
            handoff,
            chord,
            pinned_point: None,
        }
    }

    fn authorize(&mut self, edge: &'static str) -> Result<ManagedPostPermit, String> {
        let fresh = self.inner.fresh_evidence(self.target);
        let permit = authorize_managed_edge(self.target, &fresh, self.chord)
            .map_err(|r| format!("{edge} managed proof refused: {}", r.token()))?;
        if let Some(pinned) = self.pinned_point
            && permit.point() != pinned
        {
            return Err(format!(
                "{edge} managed proof refused: refused-send-grab-drift"
            ));
        }
        Ok(permit)
    }
}

impl<B: ManagedSendBackend> DragBackend for ProvingWithChords<'_, '_, B> {
    fn begin_leg(&mut self, goal: u64) -> Result<(), String> {
        if goal != self.target.target_sid() {
            return Err("leg-setup refused: goal SID mismatches the pinned target".to_owned());
        }
        if self.handoff.cancel_requested() {
            return Err(
                "leg-setup stopped: managed send cancel requested before further posts".to_owned(),
            );
        }
        let permit = self.authorize("leg-setup")?;
        self.pinned_point = Some(permit.point());
        self.inner.begin_leg(permit, goal)
    }
    fn post_mouse(&mut self, step: DragStep) -> Result<(), String> {
        if step == DragStep::MouseUp {
            return self.inner.release_mouse();
        }
        let edge = match step {
            DragStep::MouseMove => "grab-move",
            DragStep::MouseDown => "grab-down",
            DragStep::MouseDragged => "grab-dragged",
            DragStep::MouseUp => unreachable!("release handled above"),
            _ => return Err("refusing non-mouse step on the managed mouse post".to_owned()),
        };
        if self.handoff.cancel_requested() {
            return Err(format!(
                "{edge} stopped: managed send cancel requested before further posts"
            ));
        }
        let permit = self.authorize(edge)?;
        self.inner.post_mouse(permit, step)
    }
    fn post_key(&mut self, down: bool) -> Result<(), String> {
        if !down {
            return self.inner.release_key();
        }
        if self.handoff.cancel_requested() {
            return Err(
                "chord-down stopped: managed send cancel requested before further posts".to_owned(),
            );
        }
        let permit = self.authorize("chord-down")?;
        self.inner.post_key_down(permit)
    }
    fn settle_grab(&mut self) {
        self.inner.settle_grab();
    }
    fn settle_hold(&mut self) {
        self.inner.settle_hold();
    }
    fn read_membership(&mut self) -> Option<Vec<u64>> {
        self.inner.read_membership()
    }
    fn read_current(&mut self) -> Option<u64> {
        self.inner.read_current()
    }
    fn effect_deadline_ms(&self) -> u64 {
        self.inner.effect_deadline_ms()
    }
    fn effect_poll_ms(&self) -> u64 {
        self.inner.effect_poll_ms()
    }
    fn counts(&self) -> PostCounts {
        self.inner.counts()
    }
}

/// Execute one pinned-target managed send on the calling (effect-owner)
/// thread. Returns the terminal outcome plus the exact leg readback on
/// success. Never submits, finishes, polls, or releases the handoff — the
/// threaded owner below does that — but honors its stop request between
/// edges. History is NOT advanced here: the main thread notes arrival after
/// polling an `Applied` completion.
pub(crate) fn execute_managed_send<B: ManagedSendBackend>(
    target: &ManagedSendTarget,
    backend: &mut B,
    handoff: &SendEffectHandoff,
) -> (SendEffectOutcome, Option<LegOk>) {
    // Per-operation post baseline: the backend is reused across requests, so
    // classification below must use deltas from this operation only - never
    // cumulative totals that a prior success would corrupt.
    let start_counts = backend.counts();
    // Stop before touching the desktop: a cancel that landed between take
    // and execute reports a proven refusal (nothing posted this operation).
    if handoff.cancel_requested() {
        return (SendEffectOutcome::RefusedProven, None);
    }
    let gates = backend.read_gates();
    if !gates.session_unlocked {
        return (SendEffectOutcome::RefusedProven, None);
    }
    if gates.secure_active {
        return (SendEffectOutcome::RefusedProven, None);
    }
    if gates.lock_screen {
        return (SendEffectOutcome::RefusedProven, None);
    }
    match backend.display_count() {
        Ok(1) => {}
        Ok(_) | Err(_) => return (SendEffectOutcome::RefusedProven, None),
    }
    // No standalone admission proof here: the per-edge adapter below
    // re-proves the pinned target before every down-edge (including
    // leg-setup), so a stale lifetime, moved source, focus swap, or
    // disagreeing hit refuses with zero posts. Holder resolution between
    // is read-only and posts nothing. The pinned revision is fenced only
    // by that per-edge `live_revision` proof — no duplicate row-revision
    // accessor.
    let fresh_row = match backend.snapshot() {
        Ok(row) => row,
        Err(_) => return (SendEffectOutcome::RefusedProven, None),
    };
    if fresh_row.validate().is_err() {
        return (SendEffectOutcome::RefusedProven, None);
    }
    // SID-pinned ordinal: the position of the pinned SID in the fresh row
    // names the Desktop-N holder. A missing SID refuses (never guess); a
    // past-nine position refuses (unmappable, never guessed).
    let Some(position) = fresh_row
        .spaces
        .iter()
        .position(|s| *s == target.target_sid())
    else {
        return (SendEffectOutcome::RefusedProven, None);
    };
    let desktop = match u32::try_from(position + 1) {
        Ok(d) if (1..=9).contains(&d) => d,
        _ => return (SendEffectOutcome::RefusedProven, None),
    };
    let symbolic = match crate::desktop_shortcuts::desktop_symbolic_id(desktop) {
        Some(id) => id,
        None => return (SendEffectOutcome::RefusedProven, None),
    };
    let entry = match backend.read_holder(symbolic) {
        Ok(entry) => entry,
        Err(_) => return (SendEffectOutcome::RefusedProven, None),
    };
    let (keycode, flags) = match resolve_numbered_chord(desktop, entry) {
        Ok(pair) => pair,
        Err(_) => return (SendEffectOutcome::RefusedProven, None),
    };
    let goal = target.target_sid();
    let mut adapter = ProvingWithChords::with_chord(backend, target, handoff, (keycode, flags));
    let mut notes = Vec::new();
    match crate::space_drag_transfer::run_drag_leg(&mut adapter, goal, goal, "send", &mut notes) {
        Ok(leg) => (SendEffectOutcome::Applied, Some(leg)),
        Err(LegError::Refused { stage, .. }) if stage != "readback" => {
            // A stop or proof refusal after the chord started is uncertain:
            // the switch may have fired before the refusal. Only a refusal
            // with zero key-down posts *this operation* is proven
            // pre-actuation (delta from the entry baseline, never the
            // cumulative backend totals).
            let key_down_delta = adapter
                .inner
                .counts()
                .key_down
                .saturating_sub(start_counts.key_down);
            if key_down_delta > 0 {
                (SendEffectOutcome::PossiblyExecuted, None)
            } else {
                (SendEffectOutcome::RefusedProven, None)
            }
        }
        Err(_) => (SendEffectOutcome::PossiblyExecuted, None),
    }
}

/// Completion for one executed target: fenced on the same operation,
/// lifetime, and revision the handoff enforces.
pub(crate) fn completion_for(
    target: &ManagedSendTarget,
    outcome: SendEffectOutcome,
) -> SendCompletion {
    SendCompletion {
        operation_id: target.operation_id().to_owned(),
        app_generation: target.app_generation(),
        revision: target.revision(),
        outcome,
    }
}

/// Dedicated scoped managed-send effect thread.
///
/// Owns exactly one [`ManagedSendBackend`] built on the effect thread via
/// the spawn factory, so fresh AX create/use/drop reads, settles, OS posts,
/// and readbacks never run on the engine, AppKit, or per-app observation
/// owners.
///
/// The owner sleeps on an event-driven wake channel while idle: each
/// successful [`ManagedSendThread::submit`], [`ManagedSendThread::cancel`],
/// and [`ManagedSendThread::request_shutdown`] sends one wake (the lane
/// itself carries the work; the channel carries only the wakeup). No polling
/// sleep exists on the idle path. The bounded post-release settle poll stays
/// inside `run_drag_leg` on the owner thread and is not a proof cache.
///
/// Lifecycle is explicit and never blocks the caller: `spawn` starts the
/// owner, `request_shutdown` signals stop, cancels the lane, and wakes the
/// owner without joining, and `try_reap` joins only when the owner already
/// finished (`JoinHandle::is_finished`) and only then releases the lane.
/// The lane is never released while a live owner may still actuate; a
/// cancelled inflight therefore never admits a concurrent physical effect.
/// Never join per-cadence on the main loop, never abandon a held input (the
/// release-first executor plus unconditional UP attempts guarantee the mouse
/// releases before the terminal completion).
///
/// Ownership policy: shut down explicitly with [`request_shutdown`](
/// ManagedSendThread::request_shutdown) plus [`try_reap`](
/// ManagedSendThread::try_reap), or terminally with
/// [`shutdown_nonblocking`](ManagedSendThread::shutdown_nonblocking) (an
/// unfinished owner hands to [`spawn_terminal_reaper`], which joins it on
/// a dedicated thread and only then releases the lane). `Drop` is a
/// nonblocking, non-panicking backstop only: stop-signal plus wake, reap
/// when already finished, detach otherwise (a detached owner still
/// self-completes through its release-first executor).
pub struct ManagedSendThread {
    handoff: Arc<SendEffectHandoff>,
    stop: Arc<AtomicBool>,
    wake_tx: mpsc::Sender<()>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ManagedSendThread {
    /// Spawn the dedicated owner. The factory runs ON the effect thread, so
    /// backend construction (and every later AX read/post) stays off the
    /// engine/AppKit/observer owners. The owner blocks on the wake channel
    /// while idle - no sleep polling.
    pub fn spawn<B, F>(factory: F) -> Self
    where
        B: ManagedSendBackend + 'static,
        F: FnOnce() -> B + Send + 'static,
    {
        let handoff = Arc::new(SendEffectHandoff::new());
        let stop = Arc::new(AtomicBool::new(false));
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let thread_handoff = Arc::clone(&handoff);
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("managed-send-effect".to_owned())
            .spawn(move || {
                let mut backend = factory();
                loop {
                    if thread_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Some(target) = thread_handoff.take_for_owner() {
                        let (outcome, _) =
                            execute_managed_send(&target, &mut backend, &thread_handoff);
                        let _ = thread_handoff.finish(completion_for(&target, outcome));
                        continue;
                    }
                    // Idle: event-driven wait for submit/cancel/shutdown.
                    // The lane holds the work; this channel is wakeup only.
                    // A queued wake returns immediately, so no submission is
                    // ever lost between the take above and this wait.
                    match wake_rx.recv() {
                        Ok(()) => continue,
                        Err(_) => break,
                    }
                }
            })
            .expect("managed send effect thread spawns");
        Self {
            handoff,
            stop,
            wake_tx,
            handle: Some(handle),
        }
    }

    /// Nonblocking submit through the serialized lane: refuses busy while a
    /// request is pending, inflight, or completed-but-unpolled. Wakes the
    /// owner exactly once on success.
    pub fn submit(&self, target: ManagedSendTarget) -> Result<(), &'static str> {
        let result = self.handoff.submit(target);
        if result.is_ok() {
            let _ = self.wake_tx.send(());
        }
        result
    }

    /// Shared lane: engine submits via [`ManagedSendThread::submit`];
    /// main polls completions nonblocking here.
    #[must_use]
    pub fn handoff(&self) -> Arc<SendEffectHandoff> {
        Arc::clone(&self.handoff)
    }

    /// Nonblocking request-to-stop (delegates to the lane) plus an owner
    /// wake so an idle owner re-checks immediately.
    pub fn cancel(&self) -> bool {
        let had = self.handoff.cancel();
        let _ = self.wake_tx.send(());
        had
    }

    /// Whether a request is pending, inflight, or completed-but-unpolled.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.handoff.is_busy()
    }

    /// Whether the owner thread already finished (join would not block).
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
    }

    /// Nonblocking shutdown request: signal stop, cancel the lane (a
    /// cancelled inflight stays busy until its terminal completion), and
    /// wake the owner. Never joins, never releases the lane - pair with
    /// [`ManagedSendThread::try_reap`].
    pub fn request_shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.handoff.cancel();
        let _ = self.wake_tx.send(());
    }

    /// Nonblocking reap: when the owner already finished, join it (which no
    /// longer blocks) and only then release the lane. Returns `None` while
    /// the owner still lives - the caller must retry later and must never
    /// release the lane itself while the owner is live. Returns whether
    /// anything was outstanding at release.
    pub fn try_reap(&mut self) -> Option<bool> {
        if !self.is_finished() {
            return None;
        }
        let handle = self.handle.take()?;
        let _ = handle.join();
        Some(self.handoff.release())
    }

    /// Nonblocking terminal shutdown consuming the thread: signal stop,
    /// cancel the lane, and wake the owner; reap and release the lane when
    /// already finished. Returns `None` when reaped (terminal, nothing to
    /// retain) or the unfinished owner when still live — the caller must
    /// hand it to [`spawn_terminal_reaper`](crate::workspace_send_native::spawn_terminal_reaper)
    /// and must never release its lane or drop it silently. Never blocks,
    /// never panics, on any thread (engine/AppKit included: no join happens
    /// here unless the owner already finished).
    pub fn shutdown_nonblocking(mut self) -> Option<Self> {
        self.request_shutdown();
        match self.try_reap() {
            Some(_) => None,
            None => Some(self),
        }
    }
    /// Terminal off-thread reap: consume the thread and block the CALLER
    /// until the owner finishes, then release the lane exactly once. Call
    /// only on a dedicated terminal reaper thread (see
    /// [`spawn_terminal_reaper`]) — never on the engine, AppKit/main, or
    /// any observation owner.
    pub(crate) fn join_and_release(mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.handoff.release();
    }
}

impl Drop for ManagedSendThread {
    fn drop(&mut self) {
        // Nonblocking best-effort backstop (never join, never panic):
        // signal stop, drop pending/completed lane state, wake the owner.
        // A finished owner is reaped here (the join cannot block); a live
        // owner is detached after the stop signal — it still runs its
        // release-first executor to a terminal completion on its own, and
        // the lane's `Arc` keeps the handoff alive for it. Product shutdown
        // never relies on this: unfinished owners hand to
        // [`spawn_terminal_reaper`], which joins them off-thread and only
        // then releases the lane.
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.handoff.cancel();
        let _ = self.wake_tx.send(());
        if self
            .handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
            && let Some(handle) = self.handle.take()
        {
            let _ = handle.join();
            self.handoff.release();
        }
    }
}
