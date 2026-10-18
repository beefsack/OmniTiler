#![forbid(unsafe_code)]

//! Managed numbered send-follow effect backend plus the dedicated async
//! effect thread (OFFLINE PREPARATION — not product behavior).
//!
//! Nothing here arms a send, enables a holder, or posts input in product:
//! every send catalogue row stays `implemented: false`, the engine refuses
//! every send at its unsupported guard, the product tap table carries no
//! send chord, no startup path calls any holder ensure, and the tile drain
//! still calls [`crate::workspace_send::drain_live_claimed`] (which refuses).
//! Reaching this path happens only through offline injection in tests until
//! a deliberate follow-up unit wires it.
//!
//! What this unit adds over [`crate::workspace_send_effect`]:
//!
//! - [`ManagedSendBackend`]: the injectable seam the dedicated effect owner
//!   runs. Fresh reads (gates, row, revision, holder, per-edge proof
//!   evidence) plus the release-first drag posts plus the bounded
//!   post-release readback. Fakes script every leg; a future live backend
//!   would run fresh create/use/drop AX reads, settles, OS posts, and
//!   readbacks here — never on the engine, AppKit, or per-app observation
//!   owners.
//! - [`ManagedPostPermit`]: the unforgeable per-edge product-post authority.
//!   Built only by [`authorize_managed_edge`] over [`prove_managed_send`]:
//!   runtime scope/current membership, ordinary visible non-fullscreen
//!   state, unchanged lifetime/topology, pinned source homing, fresh focus
//!   naming the exact target with SLPS/direct-AX/NSWorkspace agreement,
//!   fresh CG ownership/geometry, and both positional AX and WindowServer
//!   hits at the grab point. [`crate::synth::OwnedPostTarget`] is fixture
//!   ONLY and is never constructed, forged, or bypassed here (see the
//!   `never_touches_fixture_post_target` test).
//! - [`managed_hid_mouse_post`] / [`managed_hid_key_post`]: the managed
//!   guarded boundary. The ONLY product-send callers of the maintained
//!   [`crate::synth::hid_mouse_post`] / [`crate::synth::hid_key_post`]
//!   wrappers (themselves safe `objc2` construction, no new `unsafe`).
//!   Future live posts route through these helpers with a permit (down
//!   edges) or as an unconditional release attempt (up edges). No raw global
//!   HID access exists outside this boundary for product sends.
//! - [`ProvingDragAdapter`]: a [`DragBackend`] that re-proves the applicable
//!   identity/focus/point immediately before every down-edge
//!   (`begin_leg`, move, down, dragged, chord-down) and always attempts the
//!   release edges (`MouseUp`, key-up) regardless of stale gates or a stop
//!   request. Cancellation ([`SendEffectHandoff::cancel_requested`]) stops
//!   further down-edges but never skips a held-input release.
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
//!   AX create/use/drop never runs on the engine/AppKit/observer owners).
//!   Submission and stop are event-driven (channel wake, no idle sleep);
//!   the engine/main threads only submit nonblocking
//!   ([`ManagedSendThread::submit`]) and poll the shared lane nonblocking
//!   ([`SendEffectHandoff::poll_completion_for`]); no fresh AX polling or
//!   waits happen on main. The existing bounded post-release settle poll
//!   stays inside `run_drag_leg` on this thread and is not a proof cache:
//!   no periodic proof query is added. Cancellation is request-to-stop;
//!   [`ManagedSendThread::request_shutdown`] signals stop, cancels the lane,
//!   and wakes the owner without joining, while
//!   [`ManagedSendThread::try_reap`] joins only once the owner already
//!   finished and only then releases the lane - the caller can never release
//!   while a live owner may still actuate. Never join per-cadence on the
//!   main loop, never abandon a held input (the release-first executor plus
//!   unconditional UP attempts guarantee the mouse releases before the
//!   terminal completion). `Drop` panics when the owner was not reaped: an
//!   active held-input thread is never silently detached and `Drop` never
//!   blocks.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use crate::desktop_shortcuts::{ShortcutEntry, resolve_numbered_chord};
use crate::space_drag_transfer::{
    DragBackend, DragStep, LegError, LegOk, PostCounts, SwitchChords,
};
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
pub trait ManagedSendBackend: Send {
    /// Fresh session gates sampled on the effect thread (plain booleans).
    fn read_gates(&mut self) -> SendGates;
    /// Fresh single-display user-Space row.
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String>;
    /// Fresh native display-row count (product single-display rule).
    fn display_count(&mut self) -> Result<usize, String>;
    /// Live topology revision backing the snapshot (`None` = unreadable).
    fn revision(&mut self) -> Option<u64> {
        None
    }
    /// Fresh Desktop-N holder read (never a settings write here).
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String>;
    /// Fresh proof evidence for one pinned target (single fresh read set,
    /// never a poll, never cached across edges by the caller — the adapter
    /// calls this before every down-edge).
    fn fresh_evidence(&mut self, target: &ManagedSendTarget) -> SendFreshEvidence;
    /// Select this leg's goal before any post (refusing here posts nothing).
    fn begin_leg(&mut self, goal: u64, chords: SwitchChords) -> Result<(), String>;
    fn post_mouse(&mut self, step: DragStep) -> Result<(), String>;
    fn post_key(&mut self, down: bool) -> Result<(), String>;
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

/// Unforgeable per-edge managed product-post authority.
///
/// Constructible only by [`authorize_managed_edge`], which runs the full
/// [`prove_managed_send`] verdict over fresh evidence. Down-edges
/// (move/down/dragged/chord-down) require this permit; release edges
/// (mouse-up/key-up) bypass it and always attempt. Private fields force
/// construction through proof — a stale focus window, relaunched lifetime,
/// moved domain, or disagreeing hit-test can never forge a permit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedPostPermit {
    pid: u32,
    wid: u32,
}

impl ManagedPostPermit {
    #[must_use]
    pub const fn pid(self) -> u32 {
        self.pid
    }
    #[must_use]
    pub const fn wid(self) -> u32 {
        self.wid
    }
}

/// Prove one actuation edge: the full fresh-proof verdict must name the
/// pinned target, or the edge refuses without posting (except release edges,
/// which never call this). Sequential proof is not atomic: call before every
/// down-edge.
pub fn authorize_managed_edge(
    target: &ManagedSendTarget,
    fresh: &SendFreshEvidence,
) -> Result<ManagedPostPermit, SendProofRefusal> {
    prove_managed_send(target, fresh)?;
    Ok(ManagedPostPermit {
        pid: target.pid(),
        wid: target.wid(),
    })
}

/// Managed guarded HID mouse boundary (product-send path only).
///
/// The only product-send caller of the maintained
/// [`crate::synth::hid_mouse_post`] wrapper besides the fixture-owned live
/// runner (which proves fixture ownership separately). Takes a down-edge
/// permit; release-edge callers use [`managed_hid_mouse_release`] instead so
/// a held button is never stranded by a stale gate. No new `unsafe`.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) fn managed_hid_mouse_post(
    _permit: ManagedPostPermit,
    kind: crate::synth::HidMouseKind,
    point: (f64, f64),
    flags: u64,
) -> Result<u64, String> {
    crate::synth::hid_mouse_post(
        kind,
        point,
        flags,
        Some(crate::input_match::OWN_SWITCH_USER_DATA),
    )
}

/// Release-side HID mouse post: always attempts `Up` with no permit and no
/// proof gate (release-first guarantee). Same maintained wrapper underneath.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) fn managed_hid_mouse_release(point: (f64, f64)) -> Result<u64, String> {
    crate::synth::hid_mouse_post(
        crate::synth::HidMouseKind::Up,
        point,
        0,
        Some(crate::input_match::OWN_SWITCH_USER_DATA),
    )
}

/// Managed guarded HID key boundary (product-send path only).
///
/// Down-edges require a permit; up-edges use [`managed_hid_key_release`].
/// Same maintained [`crate::synth::hid_key_post`] wrapper, same own-switch
/// tag, no new `unsafe`.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) fn managed_hid_key_post(
    _permit: ManagedPostPermit,
    key_code: u16,
    flags: u64,
    down: bool,
) -> Result<u64, String> {
    crate::synth::hid_key_post(
        key_code,
        flags,
        down,
        Some(crate::input_match::OWN_SWITCH_USER_DATA),
    )
}

/// Release-side HID key post: always attempts key-up with no permit.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) fn managed_hid_key_release(key_code: u16, flags: u64) -> Result<u64, String> {
    crate::synth::hid_key_post(
        key_code,
        flags,
        false,
        Some(crate::input_match::OWN_SWITCH_USER_DATA),
    )
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
pub(crate) fn managed_hid_mouse_post(
    _permit: ManagedPostPermit,
    _kind: crate::synth::HidMouseKind,
    _point: (f64, f64),
    _flags: u64,
) -> Result<u64, String> {
    Err("managed HID mouse posts are macOS-only".to_owned())
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
pub(crate) fn managed_hid_mouse_release(_point: (f64, f64)) -> Result<u64, String> {
    Err("managed HID mouse posts are macOS-only".to_owned())
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
pub(crate) fn managed_hid_key_post(
    _permit: ManagedPostPermit,
    _key_code: u16,
    _flags: u64,
    _down: bool,
) -> Result<u64, String> {
    Err("managed HID key posts are macOS-only".to_owned())
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
pub(crate) fn managed_hid_key_release(_key_code: u16, _flags: u64) -> Result<u64, String> {
    Err("managed HID key posts are macOS-only".to_owned())
}

/// [`DragBackend`] plumbing for the managed leg: re-proves the pinned target
/// immediately before every down-edge and always attempts release edges.
///
/// Proof source is the backend's [`ManagedSendBackend::fresh_evidence`]
/// (fresh per edge, never cached). A stop request
/// ([`SendEffectHandoff::cancel_requested`]) refuses further down-edges but
/// never skips `MouseUp`/key-up. Key/mouse UP bypass fresh target gates by
/// construction here.
struct ProvingWithChords<'a, 'h, B: ManagedSendBackend> {
    inner: &'a mut B,
    target: &'a ManagedSendTarget,
    handoff: &'h SendEffectHandoff,
    chords: SwitchChords,
}

impl<'a, 'h, B: ManagedSendBackend> ProvingWithChords<'a, 'h, B> {
    fn with_chords(
        inner: &'a mut B,
        target: &'a ManagedSendTarget,
        handoff: &'h SendEffectHandoff,
        chords: SwitchChords,
    ) -> Self {
        Self {
            inner,
            target,
            handoff,
            chords,
        }
    }
}

impl<B: ManagedSendBackend> DragBackend for ProvingWithChords<'_, '_, B> {
    fn begin_leg(&mut self, goal: u64) -> Result<(), String> {
        if self.handoff.cancel_requested() {
            return Err(
                "leg-setup stopped: managed send cancel requested before further posts".to_owned(),
            );
        }
        let fresh = self.inner.fresh_evidence(self.target);
        let _permit = authorize_managed_edge(self.target, &fresh)
            .map_err(|r| format!("leg-setup managed proof refused: {}", r.token()))?;
        self.inner.begin_leg(goal, self.chords)
    }
    fn post_mouse(&mut self, step: DragStep) -> Result<(), String> {
        if step == DragStep::MouseUp {
            return self.inner.post_mouse(step);
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
        let fresh = self.inner.fresh_evidence(self.target);
        let _permit = authorize_managed_edge(self.target, &fresh)
            .map_err(|r| format!("{edge} managed proof refused: {}", r.token()))?;
        self.inner.post_mouse(step)
    }
    fn post_key(&mut self, down: bool) -> Result<(), String> {
        if !down {
            return self.inner.post_key(false);
        }
        if self.handoff.cancel_requested() {
            return Err(
                "chord-down stopped: managed send cancel requested before further posts".to_owned(),
            );
        }
        let fresh = self.inner.fresh_evidence(self.target);
        let _permit = authorize_managed_edge(self.target, &fresh)
            .map_err(|r| format!("chord-down managed proof refused: {}", r.token()))?;
        self.inner.post_key(true)
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
    // Admission proof before resolving anything else: stale lifetime,
    // moved source, focus swap, or disagreeing hits refuse with zero posts.
    let fresh = backend.fresh_evidence(target);
    if prove_managed_send(target, &fresh).is_err() {
        return (SendEffectOutcome::RefusedProven, None);
    }
    let fresh_row = match backend.snapshot() {
        Ok(row) => row,
        Err(_) => return (SendEffectOutcome::RefusedProven, None),
    };
    if fresh_row.validate().is_err() {
        return (SendEffectOutcome::RefusedProven, None);
    }
    // Revision fence: the pinned revision must still be live. The proof leg
    // above already fenced `live_revision`; the backend row revision must
    // agree too when the backend reports one.
    if let Some(live) = backend.revision()
        && live != target.revision()
    {
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
    let chords = SwitchChords::numbered((keycode, flags), (keycode, flags));
    let goal = target.target_sid();
    let mut adapter = ProvingWithChords::with_chords(backend, target, handoff, chords);
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
/// Ownership policy: the thread must be reaped with `try_reap` (after
/// `request_shutdown`) before drop. `Drop` panics on a live owner instead
/// of silently detaching a possibly held-input thread, and `Drop` never
/// joins, so it never blocks an AppKit thread.
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
}

impl Drop for ManagedSendThread {
    fn drop(&mut self) {
        // Best-effort nonblocking stop on every path (never join here, so
        // Drop never blocks an AppKit thread). A live owner is a caller
        // bug: panic instead of silently detaching a possibly held-input
        // thread or releasing its lane from under it.
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.handoff.cancel();
        let _ = self.wake_tx.send(());
        if self.handle.is_some() && !std::thread::panicking() {
            panic!(
                "ManagedSendThread must be reaped with try_reap after request_shutdown; refusing silent detach of a possibly held-input owner"
            );
        }
    }
}
