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
    BlindRoot, ManagedSendTarget, SendCompletion, SendEffectHandoff, SendEffectOutcome,
    SendFreshEvidence, SendProofRefusal, SendRequest, TrailingBindingProof,
    TrailingCompletionProof, TrailingPrepPins, TrailingPrepProof, TrailingSendRequest,
    prove_managed_send,
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
    /// Blind-evidence root for the most recent [`ManagedSendBackend::fresh_evidence`]
    /// call: which single unreadable read collapsed the evidence to
    /// unknown (`None` when the last call was fully readable, or when the
    /// backend does not report a root). Diagnostic only — the proof
    /// verdict is unchanged. Defaults to unreported so a backend that
    /// never sets a root is never mislabeled as any specific root.
    fn blind_root(&self) -> Option<BlindRoot> {
        None
    }
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
    /// Stay-only ordinary select-back to the source SID after a released
    /// forward leg (the follow path never calls this). The production
    /// backend runs the proven SID-accepting native select with
    /// exact-current readback; the offline fake scripts the return.
    /// Numbered stays stay unarmed in
    /// [`crate::workspace_input::is_armed_send`], so no product caller
    /// reaches here until a reviewed arming change.
    fn select_back(&mut self, source_sid: u64) -> Result<(), String>;
    /// Stay-only fresh focused-window read (`"<pid>_<wid>"`) for the
    /// return-leg source-focus fallback. `None` is unreadable: an empty
    /// source (`source_focus` of `None` in [`execute_managed_send_stay`])
    /// makes no focus claim, while a claimed source focus that reads
    /// unknown holds.
    fn read_focused(&mut self) -> Option<String>;
    /// Ensure the trailing send-0 target natively on the effect thread
    /// (reuse proven-empty last, else exactly one append) and return the
    /// verified target plus its proof rows. Runs entirely on the owner:
    /// the main thread never waits for preparation - it only submits the
    /// request nonblocking and polls the completion. Default refuses (the
    /// numbered-only fakes never serve trailing); the live backend and
    /// trailing-capable fakes override. Errors name the refusing leg;
    /// anything past a possibly-sent press classifies ambiguous (see
    /// [`crate::workspace_send::classify_trailing_prep_error`]).
    fn prepare_trailing_target(
        &mut self,
        pins: &TrailingPrepPins,
    ) -> Result<TrailingPrepProof, String> {
        let _ = pins;
        Err("trailing preparation unimplemented on this backend; refusing".to_owned())
    }
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
        let root = self.inner.blind_root();
        let permit = match authorize_managed_edge(self.target, &fresh, self.chord) {
            Ok(permit) => permit,
            Err(refusal) => return Err(format_proof_refusal_reason(edge, refusal, root)),
        };
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

/// Char bound for the sanitized reason on one effect-owner refusal line.
pub(crate) const EFFECT_REFUSAL_DETAIL_CHARS: usize = 200;

/// Char bound for the operation-identity echo on one effect-owner line.
pub(crate) const EFFECT_REFUSAL_OP_CHARS: usize = 64;

/// Char bound for the stage token on one effect-owner line (stages are
/// short internal literals; the bound only defends against misuse).
pub(crate) const EFFECT_REFUSAL_STAGE_CHARS: usize = 32;

/// Sanitize free-form refusal detail into one bounded line fragment:
/// control chars (including newlines) become spaces, whitespace runs
/// collapse, empty becomes `unspecified`, char-truncated to `bound`.
/// Callers pass tokens only — never posted values, locations, bindings,
/// or window content.
#[must_use]
pub(crate) fn sanitize_refusal_detail(detail: &str, bound: usize) -> String {
    let flat: String = detail
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let collapsed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "unspecified".to_owned();
    }
    if collapsed.chars().count() > bound {
        collapsed.chars().take(bound).collect()
    } else {
        collapsed
    }
}

/// Token-only classification of one holder-chord resolve error: the raw
/// resolver errors carry entry debug and keycode numbers, so the owner
/// line carries only this token. Matches the resolver's own static
/// strings in [`crate::desktop_shortcuts::resolve_numbered_chord`].
#[must_use]
pub(crate) fn classify_holder_chord_error(err: &str) -> &'static str {
    if err.contains("disabled or unbound") {
        "holder-disabled"
    } else if err.contains("out of range") {
        "holder-unrepresentable"
    } else if err.contains("not the default") {
        "holder-customized"
    } else if err.contains("no default digit chord") {
        "holder-unmappable"
    } else {
        "holder-chord-refused"
    }
}

/// One bounded refusal reason for a refused actuation edge: the edge plus
/// the refusing proof token, with the token-only blind root appended for
/// [`SendProofRefusal::UnknownManaged`] only. A missing root reports
/// `unreported` — never a guessed root — so a backend that never sets a
/// blind root (e.g. an engine-facts `None` flowing through a backend
/// without root reporting) can never be silently mislabeled as topology
/// or membership. Pure (no logging).
#[must_use]
pub(crate) fn format_proof_refusal_reason(
    edge: &str,
    refusal: SendProofRefusal,
    blind: Option<BlindRoot>,
) -> String {
    if refusal == SendProofRefusal::UnknownManaged {
        let root = blind.map_or("unreported", BlindRoot::token);
        format!(
            "{edge} managed proof refused: {} (blind-root={root})",
            refusal.token()
        )
    } else {
        format!("{edge} managed proof refused: {}", refusal.token())
    }
}

/// One bounded single-line rendering of a non-applied effect settlement
/// for the dedicated owner log: outcome token, pinned target SID,
/// operation identity, refusing stage, sanitized reason, and
/// per-operation post deltas (counts only). Pure (no logging):
/// [`log_effect_refusal`] is the only emitter.
#[must_use]
pub(crate) fn format_effect_refusal_line(
    target_sid: u64,
    operation_id: &str,
    outcome: SendEffectOutcome,
    stage: &str,
    reason: &str,
    start: PostCounts,
    end: PostCounts,
) -> String {
    let outcome_token = match outcome {
        SendEffectOutcome::Applied => "applied",
        SendEffectOutcome::RefusedProven => "refused",
        SendEffectOutcome::PossiblyExecuted => "possibly-executed",
    };
    format!(
        "tiler-macos send effect {outcome_token} target={target_sid} op={} stage={} reason={} dposts=md:{} mu:{} kd:{} ku:{}",
        sanitize_refusal_detail(operation_id, EFFECT_REFUSAL_OP_CHARS),
        sanitize_refusal_detail(stage, EFFECT_REFUSAL_STAGE_CHARS),
        sanitize_refusal_detail(reason, EFFECT_REFUSAL_DETAIL_CHARS),
        end.mouse_down.saturating_sub(start.mouse_down),
        end.mouse_up.saturating_sub(start.mouse_up),
        end.key_down.saturating_sub(start.key_down),
        end.key_up.saturating_sub(start.key_up),
    )
}

/// The single effect-owner refusal emitter: one bounded token-only line
/// on the dedicated owner (never the engine/AppKit), only for non-applied
/// terminal outcomes. No-op on `Applied`.
fn log_effect_refusal(
    target: &ManagedSendTarget,
    outcome: SendEffectOutcome,
    stage: &str,
    reason: &str,
    start: PostCounts,
    end: PostCounts,
) {
    if outcome == SendEffectOutcome::Applied {
        return;
    }
    let line = format_effect_refusal_line(
        target.target_sid(),
        target.operation_id(),
        outcome,
        stage,
        reason,
        start,
        end,
    );
    emit_owner_line(&line);
}

/// Single owner emission site (pinned by
/// `effect_owner_emits_exactly_one_bounded_refusal_line_site`): every
/// owner log line funnels through here so the refusal surface stays one
/// bounded token-only emitter. Lines carry IDs and static tokens only -
/// never titles, content, or paths.
fn emit_owner_line(line: &str) {
    eprintln!("{line}");
}

/// Proven-refusal terminal for [`execute_managed_send`]: logs the single
/// bounded token-only owner line, then returns the outcome. `end` is the
/// backend counts at refusal time (deltas against `start` prove how much
/// posted this operation — zero on every pre-actuation path below).
fn refuse_proven(
    target: &ManagedSendTarget,
    stage: &'static str,
    reason: &str,
    start: PostCounts,
    end: PostCounts,
) -> (SendEffectOutcome, Option<LegOk>) {
    log_effect_refusal(
        target,
        SendEffectOutcome::RefusedProven,
        stage,
        reason,
        start,
        end,
    );
    (SendEffectOutcome::RefusedProven, None)
}

/// Execute one pinned-target managed send on the calling (effect-owner)
/// thread. Returns the terminal outcome plus the exact leg readback on
/// success. Never submits, finishes, polls, or releases the handoff — the
/// threaded owner below does that — but honors its stop request between
/// edges. History is NOT advanced here: the main thread notes arrival after
/// polling an `Applied` completion.
///
/// Every non-applied terminal logs one bounded token-only line on this
/// owner via [`log_effect_refusal`] (outcome, stage, sanitized reason,
/// per-operation post deltas) so a settled refusal names its refusing leg
/// without touching key material, titles, or completion plumbing.
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
        return refuse_proven(
            target,
            "cancel",
            "managed send cancel requested before further posts",
            start_counts,
            backend.counts(),
        );
    }
    let gates = backend.read_gates();
    if !gates.session_unlocked {
        return refuse_proven(
            target,
            "gates",
            "send gated: session-locked",
            start_counts,
            backend.counts(),
        );
    }
    if gates.secure_active {
        return refuse_proven(
            target,
            "gates",
            "send gated: secure-active",
            start_counts,
            backend.counts(),
        );
    }
    if gates.lock_screen {
        return refuse_proven(
            target,
            "gates",
            "send gated: lock-screen",
            start_counts,
            backend.counts(),
        );
    }
    match backend.display_count() {
        Ok(1) => {}
        Ok(other) => {
            return refuse_proven(
                target,
                "display",
                &format!("single-display rule: {other} displays"),
                start_counts,
                backend.counts(),
            );
        }
        Err(detail) => {
            return refuse_proven(
                target,
                "display",
                &format!("display count unreadable ({detail})"),
                start_counts,
                backend.counts(),
            );
        }
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
        Err(detail) => {
            return refuse_proven(
                target,
                "snapshot",
                &format!("fresh row unreadable ({detail})"),
                start_counts,
                backend.counts(),
            );
        }
    };
    if fresh_row.validate().is_err() {
        return refuse_proven(
            target,
            "row",
            "workspace row/current identity unprovable",
            start_counts,
            backend.counts(),
        );
    }
    // SID-pinned ordinal: the position of the pinned SID in the fresh row
    // names the Desktop-N holder. A missing SID refuses (never guess); a
    // past-nine position refuses (unmappable, never guessed).
    let Some(position) = fresh_row
        .spaces
        .iter()
        .position(|s| *s == target.target_sid())
    else {
        return refuse_proven(
            target,
            "target",
            "pinned target SID absent from the fresh row; refusing",
            start_counts,
            backend.counts(),
        );
    };
    let desktop = match u32::try_from(position + 1) {
        Ok(d) if (1..=9).contains(&d) => d,
        _ => {
            return refuse_proven(
                target,
                "target",
                "pinned target ordinal past Desktop-9; unmappable",
                start_counts,
                backend.counts(),
            );
        }
    };
    let symbolic = match crate::desktop_shortcuts::desktop_symbolic_id(desktop) {
        Some(id) => id,
        None => {
            return refuse_proven(
                target,
                "target",
                "desktop symbolic unresolvable",
                start_counts,
                backend.counts(),
            );
        }
    };
    let entry = match backend.read_holder(symbolic) {
        Ok(entry) => entry,
        // Never echo the store error: it may carry paths. The stage plus
        // the fixed token already discriminate the leg.
        Err(_) => {
            return refuse_proven(
                target,
                "holder",
                "holder-unreadable",
                start_counts,
                backend.counts(),
            );
        }
    };
    let (keycode, flags) = match resolve_numbered_chord(desktop, entry) {
        Ok(pair) => pair,
        // Never echo the resolver error: it carries entry keycodes.
        Err(detail) => {
            return refuse_proven(
                target,
                "holder-chord",
                classify_holder_chord_error(&detail),
                start_counts,
                backend.counts(),
            );
        }
    };
    let goal = target.target_sid();
    let mut adapter = ProvingWithChords::with_chord(backend, target, handoff, (keycode, flags));
    let mut notes = Vec::new();
    match crate::space_drag_transfer::run_drag_leg(&mut adapter, goal, goal, "send", &mut notes) {
        Ok(leg) => (SendEffectOutcome::Applied, Some(leg)),
        Err(LegError::Refused { stage, reason }) if stage != "readback" => {
            // A stop or proof refusal after the chord started is uncertain:
            // the switch may have fired before the refusal. Only a refusal
            // with zero key-down posts *this operation* is proven
            // pre-actuation (delta from the entry baseline, never the
            // cumulative backend totals).
            let end = adapter.inner.counts();
            let outcome = if end.key_down.saturating_sub(start_counts.key_down) > 0 {
                SendEffectOutcome::PossiblyExecuted
            } else {
                SendEffectOutcome::RefusedProven
            };
            log_effect_refusal(target, outcome, stage, &reason, start_counts, end);
            (outcome, None)
        }
        Err(other) => {
            let (stage, reason) = match &other {
                LegError::Refused { stage, reason } => (*stage, reason.clone()),
                LegError::NoEffect { .. } => ("readback", other.describe()),
                LegError::UnresolvedPending { .. } => ("pending", other.describe()),
                LegError::ReleaseFailed { .. } => ("release", other.describe()),
            };
            log_effect_refusal(
                target,
                SendEffectOutcome::PossiblyExecuted,
                stage,
                &reason,
                start_counts,
                adapter.inner.counts(),
            );
            (SendEffectOutcome::PossiblyExecuted, None)
        }
    }
}

/// Numbered send-stay effect (product stays are unarmed in
/// [`crate::workspace_input::is_armed_send`] and refused at engine admission,
/// so no product caller reaches here until a reviewed arming change; the
/// world-command mapping already preserves the stay intent): the forward
/// native drag through [`execute_managed_send`], then a released ordinary
/// select-back to the exact admission source SID.
///
/// Stay protocol (single-flight through the caller's handoff lane with
/// release-first, session/lock gates, and every forward per-edge
/// identity/ordinal/revision check retained):
/// - Pre-forward (nothing posted yet): the source SID must be nonzero and
///   present in the pinned fresh row, else a proven refusal.
/// - Forward: one [`execute_managed_send`]. Anything but `Applied` returns
///   as-is: no return attempted, no history, no replay.
/// - Return (the forward chord posted, so every failure below holds as
///   `PossiblyExecuted`, never replayed): stop-request, session gates, and
///   the single-display rule re-fenced; one fresh row must carry the pinned
///   display identity plus exact `spaces` order/identity with the view still
///   on the forward arrival (revision legitimately moved, so only the SID
///   row plus arrival are fenced); the ordinary
///   [`ManagedSendBackend::select_back`] actuates; the readback must prove
///   membership exactly `[target]` with current exactly `source`, plus the
///   source-focus fallback (`source_focus` names `"<pid>_<wid>"`; `None`
///   is an empty source and makes no focus claim, never an AX raise).
///   Callers must supply the fallback explicitly: it has to be the
///   post-removal source-MRU resident (or a proven-empty source), and the
///   admission subject is on the target after the drag so it must never be
///   reused as the source pin. The claim path (`from_claim`) converts only
///   core-proven stay fallbacks pinned by the engine after dispatch (resident
///   or proven-empty, never unknown); explicit `bind_stay` targets serve
///   offline tests.
///
/// Never advances previous history: stays keep the source current, so a
/// future product caller must not `note_arrival` a stay settlement.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn execute_managed_send_stay<B: ManagedSendBackend>(
    target: &ManagedSendTarget,
    source_sid: u64,
    source_focus: Option<&str>,
    backend: &mut B,
    handoff: &SendEffectHandoff,
) -> (SendEffectOutcome, Option<LegOk>) {
    let start_counts = backend.counts();
    if source_sid == 0 {
        return refuse_proven(
            target,
            "stay-source",
            "stay source SID unpinned (zero); refusing",
            start_counts,
            backend.counts(),
        );
    }
    let forward_row = match backend.snapshot() {
        Ok(row) => row,
        Err(detail) => {
            return refuse_proven(
                target,
                "stay-row",
                &format!("stay source row unreadable ({detail})"),
                start_counts,
                backend.counts(),
            );
        }
    };
    if forward_row.validate().is_err() {
        return refuse_proven(
            target,
            "stay-row",
            "stay source row/current identity unprovable",
            start_counts,
            backend.counts(),
        );
    }
    if !forward_row.spaces.contains(&source_sid) {
        return refuse_proven(
            target,
            "stay-source",
            "stay source absent from the fresh row; refusing",
            start_counts,
            backend.counts(),
        );
    }
    let (outcome, leg) = execute_managed_send(target, backend, handoff);
    if outcome != SendEffectOutcome::Applied {
        return (outcome, leg);
    }
    let forward = match leg {
        Some(leg) => leg,
        None => {
            return hold_stay_return(
                target,
                "stay-return",
                "stay return unproven: forward applied without readback",
                start_counts,
                backend.counts(),
            );
        }
    };
    if !crate::space_recovery::membership_is_exact(&forward.membership, target.target_sid())
        || forward.current != target.target_sid()
    {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return unproven: forward readback names no exact target",
            start_counts,
            backend.counts(),
        );
    }
    // The forward chord posted: every path below holds (never a proven
    // refusal, never replayed).
    if handoff.cancel_requested() {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return stopped: managed send cancel requested before select-back",
            start_counts,
            backend.counts(),
        );
    }
    let gates = backend.read_gates();
    if !gates.session_unlocked {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return gated: session-locked",
            start_counts,
            backend.counts(),
        );
    }
    if gates.secure_active {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return gated: secure-active",
            start_counts,
            backend.counts(),
        );
    }
    if gates.lock_screen {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return gated: lock-screen",
            start_counts,
            backend.counts(),
        );
    }
    match backend.display_count() {
        Ok(1) => {}
        Ok(other) => {
            return hold_stay_return(
                target,
                "stay-return",
                &format!("stay return refused: single-display rule ({other} displays)"),
                start_counts,
                backend.counts(),
            );
        }
        Err(detail) => {
            return hold_stay_return(
                target,
                "stay-return",
                &format!("stay return refused: display count unreadable ({detail})"),
                start_counts,
                backend.counts(),
            );
        }
    }
    let fresh = match backend.snapshot() {
        Ok(row) => row,
        Err(detail) => {
            return hold_stay_return(
                target,
                "stay-return",
                &format!("stay return refused: fresh row unreadable ({detail})"),
                start_counts,
                backend.counts(),
            );
        }
    };
    if fresh.validate().is_err() {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return refused: fresh row/current identity unprovable",
            start_counts,
            backend.counts(),
        );
    }
    // Return-leg fence without the stale revision fingerprint: display
    // identity plus SID order/identity must equal the pinned pre-forward
    // row, and the view must still sit on the forward arrival (the current
    // legitimately moved source to target, so the revision moved too -
    // only the SID row plus arrival are fenced here). A display swap,
    // foreign SID drift, or a foreign current move holds rather than
    // selecting under drifted topology. Pure geometry drift is invisible
    // to this SID row, but the select itself is SID-accepting with a
    // same-shape plus exact-current readback, so it cannot misroute.
    if fresh.display != forward_row.display {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return refused: display identity moved mid-stay",
            start_counts,
            backend.counts(),
        );
    }
    if fresh.spaces != forward_row.spaces {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return refused: native row moved mid-stay",
            start_counts,
            backend.counts(),
        );
    }
    if fresh.current != target.target_sid() {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return refused: forward arrival moved before select-back",
            start_counts,
            backend.counts(),
        );
    }
    if let Err(detail) = backend.select_back(source_sid) {
        return hold_stay_return(
            target,
            "stay-return",
            &format!("stay return unproven: select-back refused ({detail})"),
            start_counts,
            backend.counts(),
        );
    }
    let (membership, current) = match (backend.read_membership(), backend.read_current()) {
        (Some(membership), Some(current)) => (membership, current),
        _ => {
            return hold_stay_return(
                target,
                "stay-return",
                "stay return unproven: membership/current unreadable",
                start_counts,
                backend.counts(),
            );
        }
    };
    if !crate::space_recovery::membership_is_exact(&membership, target.target_sid())
        || current != source_sid
    {
        return hold_stay_return(
            target,
            "stay-return",
            "stay return unproven: membership/current not exactly target-on-source",
            start_counts,
            backend.counts(),
        );
    }
    if let Some(expected) = source_focus {
        match backend.read_focused() {
            Some(actual) if actual == expected => {}
            _ => {
                return hold_stay_return(
                    target,
                    "stay-return",
                    "stay return unproven: source focus fallback unproven",
                    start_counts,
                    backend.counts(),
                );
            }
        }
    }
    (
        SendEffectOutcome::Applied,
        Some(LegOk {
            membership,
            current,
            pending_recovered: forward.pending_recovered,
            counts: backend.counts(),
        }),
    )
}

/// Stay-return hold terminal for [`execute_managed_send_stay`]: the forward
/// chord already posted, so a failed or uncertain return is ambiguous
/// mid-actuation output - held without re-actuation, never a proven
/// refusal, never replayed. Logs the single bounded token-only owner line
/// like [`refuse_proven`].
#[cfg_attr(not(test), allow(dead_code))]
fn hold_stay_return(
    target: &ManagedSendTarget,
    stage: &'static str,
    reason: &str,
    start: PostCounts,
    end: PostCounts,
) -> (SendEffectOutcome, Option<LegOk>) {
    log_effect_refusal(
        target,
        SendEffectOutcome::PossiblyExecuted,
        stage,
        reason,
        start,
        end,
    );
    (SendEffectOutcome::PossiblyExecuted, None)
}

/// Completion for one executed target: fenced on the same operation,
/// lifetime, revision, and follow intent the handoff enforces. Trailing
/// executions attach their exact-append proof (`None` on reuse; tokenless
/// requests attach creation without binding) so the main thread adopts
/// owned creation and reports the binding back without re-reading
/// topology.
pub(crate) fn completion_for(
    target: &ManagedSendTarget,
    outcome: SendEffectOutcome,
    trailing: Option<TrailingCompletionProof>,
) -> SendCompletion {
    SendCompletion {
        operation_id: target.operation_id().to_owned(),
        app_generation: target.app_generation(),
        revision: target.revision(),
        follow: target.follow(),
        outcome,
        trailing,
    }
}

/// Execute one trailing send request on the calling (effect-owner) thread:
/// prepare the exact native target (reuse proven-empty last, else exactly
/// one append), bind it to the request's original pins, run the single
/// forward drag, and return the terminal outcome plus the bound target
/// plus the optional binding proof. Never submits, finishes, polls, or
/// releases the handoff - the threaded owner below does that - but honors
/// its stop request between phases.
///
/// Phase contract:
/// - Cancel before preparation: proven refusal (nothing posted), no proof.
/// - Preparation refusal: pre-actuation reasons refuse proven; anything
///   past a possibly-sent press holds as possibly-executed, never
///   replayed (see [`crate::workspace_send::classify_trailing_prep_error`]).
///   No proof either way: an exact append that validated would have
///   succeeded, so anything else cannot bind.
/// - Preparation success: the target binds to the original pins through
///   the single pin contract (subject/lifetime/source/operation fences
///   intact; the post-preparation revision rides explicitly so the
///   own-append move is never bypassed). A bind failure holds as
///   possibly-executed with the proof when an append ran (the Space may
///   exist) - never a silent drop.
/// - Cancel after preparation but before the drag: the drag never runs;
///   possibly-executed with the proof when an append ran (creation is a
///   press), proven refusal otherwise.
/// - The drag outcome stands; the proof rides along whenever an append
///   ran (reuse proves no creation). A token pairs the rows into a
///   binding; tokenless appends still report `created` so the main
///   thread adopts the owned Space - only the binding is token-gated.
pub(crate) fn execute_trailing_send<B: ManagedSendBackend>(
    request: &TrailingSendRequest,
    backend: &mut B,
    handoff: &SendEffectHandoff,
) -> (
    SendEffectOutcome,
    Option<ManagedSendTarget>,
    Option<TrailingCompletionProof>,
) {
    if handoff.cancel_requested() {
        return (SendEffectOutcome::RefusedProven, None, None);
    }
    let prep = match backend.prepare_trailing_target(&request.pins) {
        Ok(prep) => prep,
        Err(reason) => {
            let outcome = crate::workspace_send::classify_trailing_prep_error(&reason);
            emit_owner_line(&format!(
                "tiler-macos send trailing prep op={} outcome={outcome:?} complete=false",
                request.operation_id,
            ));
            return (outcome, None, None);
        }
    };
    let proof = || -> Option<TrailingCompletionProof> {
        let created = prep.created;
        let binding = match (created, request.token.clone()) {
            (Some(created), Some(token)) => Some(TrailingBindingProof {
                token,
                display_id: prep.display_id.clone(),
                created,
                before: prep.before.clone(),
                after: prep.after.clone(),
            }),
            _ => None,
        };
        Some(TrailingCompletionProof {
            target_sid: prep.target_sid,
            revision: prep.revision,
            created,
            binding,
        })
    };
    // Bind through the single pin contract: the ephemeral claim carries
    // the admitted original pins verbatim (the trailing SID/revision legs
    // of the bind read the fresh preparation, never the claim).
    let claim = crate::runtime_types::ClaimedSelect {
        action: crate::runtime_types::InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        },
        expected_sid: None,
        ui_generation: 0,
        topology_revision: None,
        send_subject: Some(request.window.clone()),
        send_generation: Some(request.app_generation),
        send_source: Some(request.source.clone()),
        send_source_sid: None,
        send_source_focus: None,
        send_source_focus_empty: false,
        operation_id: Some(request.operation_id.clone()),
        trailing_token: request.token.clone(),
    };
    let send_prep = crate::workspace_send::TrailingSendPrep {
        target_sid: prep.target_sid,
        created: prep.created,
        revision: prep.revision,
    };
    let target = match crate::workspace_send::bind_trailing_send_target(&claim, send_prep) {
        Ok(target) => target,
        Err(reason) => {
            emit_owner_line(&format!(
                "tiler-macos send trailing bind op={} reason={reason} complete=false",
                request.operation_id,
            ));
            return (SendEffectOutcome::PossiblyExecuted, None, proof());
        }
    };
    if handoff.cancel_requested() {
        // The drag never runs; a ran append is still a press.
        let outcome = if prep.created.is_some() {
            SendEffectOutcome::PossiblyExecuted
        } else {
            SendEffectOutcome::RefusedProven
        };
        return (outcome, Some(target), proof());
    }
    let (outcome, _) = execute_managed_send(&target, backend, handoff);
    (outcome, Some(target), proof())
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
                    if let Some(request) = thread_handoff.take_for_owner() {
                        match request {
                            SendRequest::Pinned(target) => {
                                // Single-flight owner routes on the pinned intent:
                                // follows run the forward drag, stays run the
                                // forward drag plus the released return select.
                                // Stay source pins travel in the target (explicit
                                // `bind_stay` pins or core-proven `from_claim` pins,
                                // never re-sampled). A stay without a pinned source SID selects back to
                                // zero, which the stay path refuses proven before
                                // any post.
                                let (outcome, _) = if target.follow() {
                                    execute_managed_send(&target, &mut backend, &thread_handoff)
                                } else {
                                    let sid = target.source_sid().unwrap_or(0);
                                    execute_managed_send_stay(
                                        &target,
                                        sid,
                                        target.source_focus(),
                                        &mut backend,
                                        &thread_handoff,
                                    )
                                };
                                let _ =
                                    thread_handoff.finish(completion_for(&target, outcome, None));
                            }
                            SendRequest::Trailing(request) => {
                                // Trailing owner flow: prepare the exact
                                // native target on this thread (the main
                                // thread never waits for preparation), bind
                                // it to the admitted original pins, and run
                                // the single forward drag. No second
                                // dispatch, no duplicate effect: this drag
                                // is the only actuation; the core drains
                                // its pending send on the reported binding.
                                let (outcome, target, proof) =
                                    execute_trailing_send(&request, &mut backend, &thread_handoff);
                                let completion = match target {
                                    Some(target) => completion_for(&target, outcome, proof),
                                    None => SendCompletion {
                                        operation_id: request.operation_id.clone(),
                                        app_generation: request.app_generation,
                                        // Unknown until preparation binds:
                                        // the trailing poll fences operation
                                        // plus lifetime, never this.
                                        revision: 0,
                                        follow: true,
                                        outcome,
                                        trailing: proof,
                                    },
                                };
                                let _ = thread_handoff.finish(completion);
                            }
                        }
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

    /// Nonblocking trailing submit: the owner prepares the exact native
    /// target itself before binding and actuating. Same single-flight
    /// refusal and exactly-once wake as [`ManagedSendThread::submit`].
    pub fn submit_trailing(&self, request: TrailingSendRequest) -> Result<(), &'static str> {
        let result = self.handoff.submit_trailing(request);
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
