#![forbid(unsafe_code)]

//! Bounded live-proof fixture for the menu-bar workspace indicator.
//!
//! Offline contract (portable, unit-tested below): proof bounds, harness
//! preconditions, bundle-layout gate, segment hit-identity, frame
//! freshness, click-point planning, the post-down release guard,
//! focus/teardown readback verdicts, retained-domain predicates, the
//! unified cleanup plan, and the leg report. Every live-affecting decision
//! in the runner goes through these pure gates first, so the refusal
//! matrix below IS the product-lane contract (same
//! [`crate::workspace_indicator::resolve_click`], same
//! [`crate::workspace_select::drain_claimed_select`], same
//! [`crate::synth::mouse_point_verdict`] the live legs call — no weakened
//! semantic oracle).
//!
//! Live runner (macOS-only [`run_workspace_indicator_proof`], never executed
//! without an explicit `--allow-untested` invocation from the signed
//! LaunchServices bundle): hosts the PRODUCTION
//! [`crate::tile::IndicatorGuard`] (never a rebuilt lookalike) over the
//! signed Helper.app launched through LaunchServices (ordinary product
//! identity — bundle plus executable plus launch date — that the scoped
//! backend binds; no `Child` is held, currency is the spawn record plus
//! live-exe plus CURRENT window-nonce title), proves the independent
//! public self-join restore route before any Space-affecting leg, runs a
//! bounded fixture runtime scoped exclusively to the helper bundle
//! (publishing real native topology and helper observations), creates
//! exactly one temporary Space through the existing Mission Control
//! fixtures, and proves: real item appear/current readbacks from live
//! AppKit state over a revision-matched retained report (off-current
//! Occupied asserted, never parked), native Space-switch updates through
//! the engine tap lane, one fresh physical synthetic mouse down/up pair on
//! our own status segment with dual hit identity plus release on every
//! post-down exit, verified arrival through the full engine click lane
//! (resolve, submit, claim, guarded drain — no hand-crafted claim bypass),
//! observed (not inferred) teardown absence, and exact original
//! one-Space/current restoration. Every mutation/failure exit routes
//! through the same unified cleanup (goto original, helper re-home,
//! temp-only removal under the strict gate, verifications, teardown,
//! runtime/helper/caffeinate stops). No transfer experimentation: transfer
//! stays parked.
//!
//! Safety contract (enforced per leg, never relaxed for progress): original
//! one-Space baseline (other layouts refuse unless the exact safe shape is
//! proven), unlocked session plus AX trust plus post access, single display,
//! no Secure Input, no lock screen, protected Ghostty frame/membership
//! baseline never written/focused/input. Coordinates compute freshly from
//! the live segment frame and current topology before EVERY synthetic edge;
//! both the fresh AX positional owner and the WindowServer hit must bind
//! exactly our status window and PID (a system-hosted menu bar refuses —
//! never an assumed same PID); Ghostty ownership refuses explicitly.
//! `performClick` is never a substitute: the proof posts a real pair and
//! releases on every post-down exit, restores the pointer eventlessly, and
//! verifies the pointer preimage. The status item never steals focus
//! (before/after capture, never a synthetic-input or focus-write restore).
//! Bounded finite execution with a per-leg lock recheck and a bounded
//! reaped `caffeinate -d` companion held by exact handle; the helper stops
//! only by its verified exact PID through `stop_session` (record plus
//! live-exe plus CURRENT window-nonce re-proof — never a name-based kill,
//! never a `Child` handle that LaunchServices does not give). No TCC
//! grant/reset, no Screen Recording changes, no installs, no core
//! redesign; all `unsafe` stays confined to [`crate::ffi`].

use crate::runtime_types::InputSemanticAction;
use crate::workspace_indicator::{IndicatorClick, SpaceOccupancy};

// -- Proof bounds ------------------------------------------------------------

/// Minimum proof lifetime: the leg battery (spawn, restore-route proof,
/// temp lifecycle, appear/switch/click/arrival/teardown/restore) needs the
/// same work window as the sibling owned batteries, whose 60 s floor this
/// mirrors.
pub const PROOF_MIN_SECONDS: u64 = 60;

/// Validate the proof bounds before any live touch: finite probe lifetime
/// with the 60 s work floor, exactly display 0 (the product single-display
/// rule — any other display refuses rather than guessing attribution), and
/// the explicit `--allow-untested` opt-in (the runner refuses without it;
/// parse still succeeds so the refusal carries a named prerequisite).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProofConfig {
    pub seconds: u64,
    pub display: usize,
    pub allow_untested: bool,
}

/// Validate proof arguments (no OS touch).
pub fn validate_proof_args(
    seconds: u64,
    display: usize,
    allow_untested: bool,
) -> Result<ProofConfig, String> {
    crate::config::ProbeConfig::validate_probe_seconds(seconds)?;
    if seconds < PROOF_MIN_SECONDS {
        return Err(format!(
            "workspace-indicator-proof needs --seconds >= {PROOF_MIN_SECONDS} (leg battery plus cleanup reserve); refusing before any mutation"
        ));
    }
    if display != 0 {
        return Err(
            "workspace-indicator-proof refuses nonzero --display (product single-display rule: exactly one native row, never a guess)"
                .to_owned(),
        );
    }
    if !allow_untested {
        return Err(
            "workspace-indicator-proof requires --allow-untested (visible Mission Control automation plus synthetic menu-bar clicks with a later-unit launch)"
                .to_owned(),
        );
    }
    Ok(ProofConfig {
        seconds,
        display,
        allow_untested,
    })
}

// -- Harness preconditions (pure) --------------------------------------------

/// Plain-data harness preconditions sampled fresh by the runner before
/// every leg. Every denial refuses narrowly with a static reason instead
/// of running under an unsafe context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProofPreconditions {
    /// Window-server session reads unlocked (login done, on console, no
    /// screen-lock marker).
    pub session_unlocked: bool,
    /// `ax_is_process_trusted` reads exactly true.
    pub ax_trusted: bool,
    /// Window-server post access reads exactly true (posts would drop).
    pub post_access: bool,
    /// Fresh native display-row count (must read exactly 1).
    pub display_count: usize,
    /// Fresh user-Space count on display 0 (must read exactly 1 at
    /// baseline: the original one-Space shape).
    pub space_count: usize,
    /// Secure Input reads off.
    pub secure_input_off: bool,
    /// Lock-screen frontmost reads off.
    pub lock_screen_off: bool,
    /// Protected Ghostty baseline captured (frame/membership preimage).
    pub ghostty_baselined: bool,
    /// Focus preimage captured (frontmost readback held for the
    /// never-steals-focus leg).
    pub focus_captured: bool,
    /// Pointer preimage captured (cursor position held for eventless
    /// restoration).
    pub pointer_captured: bool,
}

/// Refuse the leg unless every harness precondition holds. Static reasons
/// only (IDs and counts ride the report, never titles or paths).
pub fn check_preconditions(pre: &ProofPreconditions) -> Result<(), String> {
    if !pre.session_unlocked {
        return Err("proof gated: session not unlocked".to_owned());
    }
    if !pre.ax_trusted {
        return Err("proof gated: AX trust not observed true".to_owned());
    }
    if !pre.post_access {
        return Err("proof gated: window-server post access not granted".to_owned());
    }
    if pre.display_count != 1 {
        return Err(format!(
            "proof refuses: expected exactly one native display, found {}",
            pre.display_count
        ));
    }
    if pre.space_count != 1 {
        return Err(format!(
            "proof refuses: baseline is not the original one-Space shape (found {} user Spaces)",
            pre.space_count
        ));
    }
    if !pre.secure_input_off {
        return Err("proof gated: Secure Input active".to_owned());
    }
    if !pre.lock_screen_off {
        return Err("proof gated: lock screen frontmost".to_owned());
    }
    if !pre.ghostty_baselined {
        return Err("proof gated: protected Ghostty baseline uncaptured".to_owned());
    }
    if !pre.focus_captured {
        return Err("proof gated: focus preimage uncaptured".to_owned());
    }
    if !pre.pointer_captured {
        return Err("proof gated: pointer preimage uncaptured".to_owned());
    }
    Ok(())
}

// -- Segment hit identity (pure) ---------------------------------------------

/// Fresh dual-hit proof for one synthetic segment edge (plain data, no OS
/// touch): our PID (the fixture hosts the status item in-process, so the
/// AX owner of our segment IS us), the fresh AX positional owner read at
/// the exact post point, the fresh WindowServer hit window at that point,
/// our live status-window WIDs from the CG list, and the live bundle of
/// the AX owner PID.
///
/// This deliberately differs from the keyboard [`crate::synth`] boundary:
/// that boundary posts INTO an owned window and refuses our own probe
/// bundle; here the click target IS our own menu-bar UI, so our PID plus
/// the probe bundle is the EXPECTED owner and anything else (Ghostty, a
/// protected terminal, a system-hosted menu bar, an unreadable leg)
/// refuses. A system-hosted menu bar never assumes our PID: without an
/// exact bind to our status window the edge refuses and reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentHitProof {
    pub our_pid: u32,
    pub ax_owner: Option<(u32, u32)>,
    pub cg_hit_wid: Option<u32>,
    pub our_window_wids: Vec<u32>,
    pub hit_bundle: Option<String>,
}

/// Verify one segment edge binds exactly our status item and PID before
/// any post. Refusals name the leg that failed, never a guess.
pub fn verify_segment_hit_identity(proof: &SegmentHitProof) -> Result<(), String> {
    let bundle = proof.hit_bundle.as_deref().unwrap_or_default();
    if bundle.is_empty() {
        return Err("refusing segment click with unknown owner bundle".to_owned());
    }
    if crate::scope::is_ghostty_bundle(bundle) {
        return Err("refusing segment click over Ghostty (never a target)".to_owned());
    }
    if crate::ownership::is_protected_bundle(bundle) {
        return Err("refusing segment click over a protected bundle".to_owned());
    }
    let Some((ax_pid, _)) = proof.ax_owner else {
        return Err("refusing segment click: AX positional owner unreadable".to_owned());
    };
    if ax_pid != proof.our_pid {
        return Err(
            "refusing segment click: AX owner is a foreign pid (system-hosted menu bar or drift); never assuming our PID"
                .to_owned(),
        );
    }
    let Some(hit) = proof.cg_hit_wid else {
        return Err("refusing segment click: WindowServer hit unreadable".to_owned());
    };
    if hit == 0 || !proof.our_window_wids.contains(&hit) {
        return Err("refusing segment click: WindowServer hit is not our status window".to_owned());
    }
    // Both legs name us: the AX owner PID equals ours AND the
    // WindowServer hit is one of our live status-window WIDs. The WID
    // cross-check runs through the shared point verdict so a stale point
    // (segment moved since the frame was read) refuses identically.
    crate::synth::mouse_point_verdict(proof.our_pid, hit, proof.ax_owner, proof.cg_hit_wid)
        .map_err(|reason| format!("refusing segment click: {reason}"))?;
    Ok(())
}

// -- Frame freshness and click-point planning (pure) -------------------------

/// Refuse when the live segment frame or the topology revision drifted
/// between the observation that planned the point and the edge. Exact
/// equality: menu-bar geometry never legitimately moves mid-leg.
pub fn verify_frame_fresh(
    planned_frame: (f64, f64, f64, f64),
    fresh_frame: (f64, f64, f64, f64),
    planned_revision: u64,
    fresh_revision: u64,
) -> Result<(), String> {
    if planned_frame != fresh_frame {
        return Err("refusing segment click: live segment frame drifted since planning".to_owned());
    }
    if planned_revision != fresh_revision {
        return Err("refusing segment click: topology revision moved since planning".to_owned());
    }
    Ok(())
}

/// Plan the synthetic click point from one live AppKit segment frame plus
/// the single display frame (both AppKit origin, bottom-left): the segment
/// centre with the Y axis flipped into CG/AX top-left points. Rejects
/// non-finite, negative-origin, and zero-area frames before any live
/// touch. Real display geometry is an explicit input, never a constant.
pub fn segment_click_point_cg(
    frame_appkit: (f64, f64, f64, f64),
    display_frame_appkit: (f64, f64, f64, f64),
) -> Result<(f64, f64), String> {
    let (x, y, w, h) = frame_appkit;
    let (dx, dy, _, dh) = display_frame_appkit;
    for v in [x, y, w, h, dx, dy, dh] {
        if !v.is_finite() {
            return Err("refusing segment click at a non-finite frame".to_owned());
        }
    }
    if w <= 0.0 || h <= 0.0 || dh <= 0.0 {
        return Err("refusing segment click at a zero-area frame".to_owned());
    }
    if x < 0.0 || y < 0.0 {
        return Err("refusing segment click at a negative frame origin".to_owned());
    }
    let cx = x + w / 2.0;
    // AppKit bottom-left to CG top-left over the display that hosts the
    // segment (single-display precondition guarantees the attribution).
    let cy = (dy + dh) - (y + h / 2.0);
    crate::synth::validate_click_point((cx, cy))?;
    Ok((cx, cy))
}

// -- Post-down release guard (pure) -------------------------------------------

/// Balanced-contract state for one synthetic down/up pair: a posted down
/// MUST be followed by its up on every exit path (no re-verification that
/// could refuse the release and strand the button system-wide). The live
/// runner pairs every `note_down` with exactly one release; dropping the
/// guard while armed is a programming error the report names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PostDownRelease {
    /// No down outstanding.
    #[default]
    Idle,
    /// A down was posted; its up is owed unconditionally.
    DownPosted,
}

impl PostDownRelease {
    /// Record a posted down (refuses a second down before the release).
    pub fn note_down(&mut self) -> Result<(), String> {
        match self {
            Self::Idle => {
                *self = Self::DownPosted;
                Ok(())
            }
            Self::DownPosted => {
                Err("unbalanced mouse pair: down while a release is owed".to_owned())
            }
        }
    }

    /// Record the owed release.
    pub fn note_up(&mut self) -> Result<(), String> {
        match self {
            Self::DownPosted => {
                *self = Self::Idle;
                Ok(())
            }
            Self::Idle => Err("unbalanced mouse pair: up with no down posted".to_owned()),
        }
    }

    /// Whether the exit path owes a release right now.
    #[must_use]
    pub const fn requires_release(self) -> bool {
        matches!(self, Self::DownPosted)
    }
}

/// Decide the release point for the up edge from FRESH up-leg reads (pure,
/// no OS touch): the live runner re-reads the segment frame, the topology
/// revision, recomputes the up point, and re-proves dual hit identity at
/// that recomputed point AFTER the down and BEFORE the up. The down point
/// is never reused blindly: frame/revision drift or an identity change
/// refuses the proof here (the caller still posts the owed balanced
/// release at the down point as safety cleanup, labeled cleanup — never
/// a successful identity proof — and never falls back to a foreign or
/// Ghostty point).
pub fn decide_up_point(
    down_frame: (f64, f64, f64, f64),
    down_revision: u64,
    up_frame: (f64, f64, f64, f64),
    up_revision: u64,
    up_point: (f64, f64),
    hit: &SegmentHitProof,
) -> Result<(f64, f64), String> {
    verify_frame_fresh(down_frame, up_frame, down_revision, up_revision).map_err(|reason| {
        format!(
            "refusing up edge without fresh identity ({reason}); owed release is cleanup, not proof"
        )
    })?;
    verify_segment_hit_identity(hit)
        .map_err(|reason| format!("{reason}; owed release is cleanup, not proof"))?;
    Ok(up_point)
}

// -- Focus and teardown readback verdicts (pure) -------------------------------

/// The status item must not steal focus: the frontmost PID after an
/// indicator leg must equal the preimage. Unreadable legs refuse (never
/// guessed); restoration via synthetic input or focus writes is forbidden
/// — equality is the only passing shape.
pub fn verify_focus_held(before_pid: Option<u32>, after_pid: Option<u32>) -> Result<(), String> {
    match (before_pid, after_pid) {
        (Some(before), Some(after)) if before == after => Ok(()),
        (Some(_), Some(_)) => Err("indicator stole focus: frontmost PID moved".to_owned()),
        _ => Err("focus hold unprovable: frontmost unreadable".to_owned()),
    }
}

/// Teardown must observe removal, never infer it from process exit: every
/// status-window WID recorded before destroy must be absent from the live
/// CG list after destroy. An empty baseline refuses (the appear leg must
/// have proven the item first).
pub fn verify_teardown_absent(before_wids: &[u32], after_wids: &[u32]) -> Result<(), String> {
    if before_wids.is_empty() {
        return Err("teardown unproven: no status-window baseline recorded".to_owned());
    }
    for wid in before_wids {
        if after_wids.contains(wid) {
            return Err("teardown unproven: status window still listed".to_owned());
        }
    }
    Ok(())
}

// -- Retained-occupancy assertion (pure) ---------------------------------------

/// Assert the off-current Space reads `Occupied` over a COMPLETE retained
/// report mapped to occupancy words. A complete report comes from the
/// engine's retained domains (managed windows only — the same homing the
/// engine plans from, current and hidden alike); any unproven frame (no
/// report, revision drift) renders every row `Unknown` and this assertion
/// is skipped by the caller (recorded unproven, never weakened into an
/// `Empty` verdict — `Unknown` is never presented as empty).
pub fn require_occupied_off_current(
    temp_position: usize,
    rows: &[(usize, SpaceOccupancy)],
) -> Result<(), String> {
    if temp_position == 0 {
        return Err("occupancy assertion needs a nonzero temp position".to_owned());
    }
    match rows.iter().find(|(position, _)| *position == temp_position) {
        Some((_, SpaceOccupancy::Occupied)) => Ok(()),
        Some((_, SpaceOccupancy::Empty)) => Err(
            "off-current occupancy reads Empty (helper homing lost); refusing".to_owned(),
        ),
        Some((_, SpaceOccupancy::Unknown)) => Err(
            "off-current occupancy reads Unknown (retained report incomplete); unproven, never empty"
                .to_owned(),
        ),
        None => Err("off-current occupancy: temp position not rendered".to_owned()),
    }
}

/// Whether one retained domain names `sid` as occupied: the domain key
/// must read exactly `space-{sid}` with a non-empty exactly-homed
/// membership. Pure over the production report shape, so the offline
/// matrix pins the same predicate the live await loop uses — never a
/// parallel reimplementation.
#[must_use]
pub fn domain_occupied(report: &crate::runtime::WorkspaceOccupancyReport, sid: u64) -> bool {
    let key = format!("space-{sid}");
    report
        .domains
        .iter()
        .any(|domain| domain.workspace == key && !domain.members.is_empty())
}

/// Whether the report is a complete frame for `revision`: the engine
/// answered with a present context homed under exactly this revision. Any
/// other frame renders every row `Unknown` (never `Empty`).
#[must_use]
pub fn report_complete_for(
    report: &crate::runtime::WorkspaceOccupancyReport,
    revision: u64,
) -> bool {
    report.context_present && report.context_revision == revision
}

/// Fail-closed bundle-layout precondition (pure path math, no FS reads):
/// the fixture must run from inside the signed probe `.app` bundle with
/// the sibling `OmniTilerProbeHelper.app` beside it — never a plain `cargo`
/// executable, which has no LaunchServices identity to scope. The live
/// runner refuses before any mutation when this is false.
#[must_use]
pub fn bundle_layout_ok(current_exe: &std::path::Path) -> bool {
    crate::probe_target::helper_app_for_exe(current_exe).is_some()
}

// -- Unified cleanup obligations (pure) ----------------------------------------

// Every mutation/failure exit after the caffeinate companion spawns routes
// through the same ordered cleanup, pinned here and executed by the live
// runner step-for-step. Steps that find nothing to do complete trivially
// (a goto already home, a helper already homed, no temp adopted); steps
// that cannot prove their effect record residue instead of forcing it.

/// One ordered cleanup obligation. Tokens ride the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupStep {
    /// Recheck session/trust/post-access/Secure Input/lock screen. When
    /// this fails, the mutation steps below are skipped WITH residue
    /// notes (never actuated under failed gates) while every
    /// non-mutating step still runs.
    CheckGates,
    /// A synthetic down was posted without its up: post the owed release
    /// unconditionally (no re-verification that could strand the button).
    ReleaseClickOwed,
    /// Return current to the original Space (product tap lane through the
    /// engine; skipped when already home).
    GotoOriginal,
    /// Re-home the owned helper to the original Space through the proven
    /// public self-join route (skipped when already homed or helper gone).
    /// Runs BEFORE any helper stop: restoration needs the helper alive.
    JoinHelperOriginal,
    /// Remove ONLY the adopted temp under the strict emptiness gate. An
    /// occupied temp, a dispatched-but-unverified remove, or any doubt
    /// records residue — never a replay, never a broader cleanup.
    RemoveTempIfEmpty,
    /// Verify the exact one-Space/current preimage.
    VerifyLayout,
    /// Verify protected Ghostty frame/membership equality.
    VerifyGhostty,
    /// Verify the pointer preimage (eventless restore when displaced) and
    /// read the frontmost (never a focus write; Ghostty/protected frontmost
    /// refuses).
    VerifyPointer,
    /// Explicit production-guard teardown plus observed CG absence
    /// (idempotent: safe when the item never appeared).
    TeardownIndicator,
    /// Stop the bounded fixture runtime (observation/worker owners).
    ShutdownRuntime,
    /// Stop exactly the spawned helper by its verified PID
    /// (record plus live-exe plus CURRENT window-nonce re-proof inside
    /// `stop_session`; a helper already gone only clears its stale record).
    /// Always after restoration: never kill before a restore that needs it.
    StopHelperExact,
    /// Stop and reap the owned caffeinate companion by its held handle.
    StopCaffeinate,
}

/// Stable step token for the report.
#[must_use]
pub const fn cleanup_step_token(step: CleanupStep) -> &'static str {
    match step {
        CleanupStep::CheckGates => "check-gates",
        CleanupStep::ReleaseClickOwed => "release-click-owed",
        CleanupStep::GotoOriginal => "goto-original",
        CleanupStep::JoinHelperOriginal => "join-helper-original",
        CleanupStep::RemoveTempIfEmpty => "remove-temp-if-empty",
        CleanupStep::VerifyLayout => "verify-layout",
        CleanupStep::VerifyGhostty => "verify-ghostty",
        CleanupStep::VerifyPointer => "verify-pointer",
        CleanupStep::TeardownIndicator => "teardown-indicator",
        CleanupStep::ShutdownRuntime => "shutdown-runtime",
        CleanupStep::StopHelperExact => "stop-helper-exact",
        CleanupStep::StopCaffeinate => "stop-caffeinate",
    }
}

/// What exists at a failure point: the cleanup plan follows from this
/// alone, so failure-injection tests pin every post-mutation stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CleanupState {
    /// An owned helper was spawned and may still be alive.
    pub helper_spawned: bool,
    /// A temp Space was adopted and may still exist.
    pub temp_created: bool,
    /// A synthetic down was posted without its up yet.
    pub release_owed: bool,
    /// The bounded fixture runtime was started.
    pub runtime_started: bool,
}

/// Ordered cleanup obligations for one failure state. The mutation steps
/// ride only when something exists that could need them; the verification
/// and stop steps always ride (each completes trivially when there is
/// nothing to do). The executor additionally consults
/// [`step_needs_gates`]: gates-failed runs skip the mutation steps with
/// residue notes but still run every other step.
#[must_use]
pub fn cleanup_plan(state: &CleanupState) -> Vec<CleanupStep> {
    use CleanupStep::{
        CheckGates, GotoOriginal, JoinHelperOriginal, ReleaseClickOwed, RemoveTempIfEmpty,
        ShutdownRuntime, StopCaffeinate, StopHelperExact, TeardownIndicator, VerifyGhostty,
        VerifyLayout, VerifyPointer,
    };
    let mut steps = vec![CheckGates];
    if state.release_owed {
        steps.push(ReleaseClickOwed);
    }
    if state.helper_spawned {
        steps.push(GotoOriginal);
        steps.push(JoinHelperOriginal);
    }
    if state.temp_created {
        steps.push(RemoveTempIfEmpty);
    }
    steps.push(VerifyLayout);
    steps.push(VerifyGhostty);
    steps.push(VerifyPointer);
    steps.push(TeardownIndicator);
    if state.runtime_started {
        steps.push(ShutdownRuntime);
    }
    if state.helper_spawned {
        steps.push(StopHelperExact);
    }
    steps.push(StopCaffeinate);
    steps
}

/// Whether a cleanup step actuates Mission Control / helper / input state
/// and therefore requires passing gates. Gate-independent steps (reads,
/// own-UI teardown, own-process stops) run even when gates fail; gated
/// steps record residue instead.
#[must_use]
pub const fn step_needs_gates(step: CleanupStep) -> bool {
    match step {
        CleanupStep::GotoOriginal
        | CleanupStep::JoinHelperOriginal
        | CleanupStep::RemoveTempIfEmpty => true,
        CleanupStep::CheckGates
        | CleanupStep::ReleaseClickOwed
        | CleanupStep::VerifyLayout
        | CleanupStep::VerifyGhostty
        | CleanupStep::VerifyPointer
        | CleanupStep::TeardownIndicator
        | CleanupStep::ShutdownRuntime
        | CleanupStep::StopHelperExact
        | CleanupStep::StopCaffeinate => false,
    }
}

/// Frontmost verdict for cleanup verification (read-only): the frontmost
/// bundle must be readable and must never be Ghostty or a protected
/// terminal. The owned helper bundle passes (it legitimately self-activates
/// on its public joins); anything else readable passes with its identity
/// noted. No focus is ever written here.
pub fn cleanup_focus_verdict(bundle: Option<&str>) -> Result<(), String> {
    match bundle {
        Some(id) if !id.is_empty() => {
            if crate::scope::is_ghostty_bundle(id) {
                return Err("cleanup focus: frontmost is Ghostty".to_owned());
            }
            if crate::ownership::is_protected_bundle(id) {
                return Err("cleanup focus: frontmost is a protected bundle".to_owned());
            }
            Ok(())
        }
        _ => Err("cleanup focus: frontmost unreadable".to_owned()),
    }
}

/// Expected live titles for one rendered row: `segment_title` per
/// position/occupancy. The live appear/occupancy legs compare the actual
/// AppKit button and menu readbacks against exactly this — the offline
/// matrix pins `'?'` if and only if Unknown, and the live legs assert the
/// readbacks equal it (never a cached model, never a re-derived verdict).
#[must_use]
pub fn expected_titles(rows: &[(usize, SpaceOccupancy)]) -> Vec<String> {
    rows.iter()
        .map(|(position, occupancy)| {
            crate::workspace_indicator::segment_title(*position, *occupancy)
        })
        .collect()
}

// -- Leg sequence and report (pure) --------------------------------------------

/// Ordered proof legs. Tokens ride the JSON report; positions and outcome
/// tokens only, never titles or paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofLeg {
    Baseline,
    SpawnHelper,
    ProveRestoreRoute,
    CreateTemp,
    IndicatorAppear,
    SwitchUpdate,
    OffCurrentOccupancy,
    SegmentClick,
    VerifiedArrival,
    TeardownAbsent,
    CleanupRestore,
}

/// Stable leg token for the report.
#[must_use]
pub const fn leg_token(leg: ProofLeg) -> &'static str {
    match leg {
        ProofLeg::Baseline => "baseline",
        ProofLeg::SpawnHelper => "spawn-helper",
        ProofLeg::ProveRestoreRoute => "prove-restore-route",
        ProofLeg::CreateTemp => "create-temp",
        ProofLeg::IndicatorAppear => "indicator-appear",
        ProofLeg::SwitchUpdate => "switch-update",
        ProofLeg::OffCurrentOccupancy => "off-current-occupancy",
        ProofLeg::SegmentClick => "segment-click",
        ProofLeg::VerifiedArrival => "verified-arrival",
        ProofLeg::TeardownAbsent => "teardown-absent",
        ProofLeg::CleanupRestore => "cleanup-restore",
    }
}

/// One leg outcome for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofLegRecord {
    pub leg: &'static str,
    pub complete: bool,
    pub detail: String,
}

impl ProofLegRecord {
    /// New leg record. Detail carries positions, counts, generations, and
    /// static tokens only — never titles, bundles, or paths.
    #[must_use]
    pub fn new(leg: ProofLeg, complete: bool, detail: String) -> Self {
        Self {
            leg: leg_token(leg),
            complete,
            detail,
        }
    }
}

/// Build the terminal JSON report: action, legs, restoration verdicts, and
/// notes. Counts, ordinals, positions, generations, and static tokens
/// only.
#[must_use]
pub fn proof_report(
    legs: &[ProofLegRecord],
    restored: bool,
    caffeinate_reaped: bool,
    notes: &[String],
) -> String {
    let complete = legs.iter().all(|leg| leg.complete) && restored && caffeinate_reaped;
    serde_json::json!({
        "action": "workspace-indicator-proof",
        "legs": legs.iter().map(|leg| serde_json::json!({
            "leg": leg.leg,
            "complete": leg.complete,
            "detail": leg.detail,
        })).collect::<Vec<_>>(),
        "restored": restored,
        "caffeinate_reaped": caffeinate_reaped,
        "complete": complete,
        "notes": notes,
    })
    .to_string()
}

/// Resolve one production segment binding against the CURRENT render
/// generation plus a FRESH native row and revision: the exact pre-dispatch
/// gate the live click leg runs before any post. Thin alias over the
/// product mapping so the offline matrix pins the live contract.
pub fn resolve_proof_click(
    click: IndicatorClick,
    current_generation: u64,
    fresh_spaces: &[u64],
    fresh_revision: u64,
) -> Result<InputSemanticAction, String> {
    crate::workspace_indicator::resolve_indicator_click(
        click,
        current_generation,
        fresh_spaces,
        fresh_revision,
        false,
    )
}

// -- Live runner (macOS-only; never executed without --allow-untested) --------

/// Live proof entry (macOS-only). The full harness from the module docs:
/// preconditions per leg, owned LS helper via `spawn_verified_ls` (no
/// `Child` is held — currency is the spawn record plus live-exe plus
/// CURRENT window-nonce title), proven restore
/// route, one temp Space, production indicator hosting with live AppKit
/// readbacks, product-lane switch/click/arrival, observed teardown, and
/// exact restoration. Returns the terminal JSON report on success; every
/// failure carries its stage plus the restoration readbacks. Off-macOS the
/// stub below refuses without touching anything.
#[cfg(target_os = "macos")]
pub fn run_workspace_indicator_proof(
    seconds: u64,
    display: usize,
    allow_untested: bool,
) -> Result<String, (i32, String)> {
    live::run(seconds, display, allow_untested)
}

/// Off-macOS stub: never launches, never touches UI.
#[cfg(not(target_os = "macos"))]
pub fn run_workspace_indicator_proof(
    _seconds: u64,
    _display: usize,
    _allow_untested: bool,
) -> Result<String, (i32, String)> {
    Err((1, "workspace-indicator-proof is macOS-only".to_owned()))
}

#[cfg(target_os = "macos")]
mod live {
    use std::sync::Arc;
    use std::time::Duration;

    use super::{
        CleanupState, CleanupStep, PostDownRelease, ProofLeg, ProofLegRecord, SegmentHitProof,
        bundle_layout_ok, cleanup_focus_verdict, cleanup_plan, cleanup_step_token, decide_up_point,
        domain_occupied, expected_titles, leg_token, proof_report, report_complete_for,
        require_occupied_off_current, resolve_proof_click, segment_click_point_cg,
        step_needs_gates, validate_proof_args, verify_focus_held, verify_frame_fresh,
        verify_segment_hit_identity, verify_teardown_absent,
    };
    use crate::ffi::{self, SessionLock};
    use crate::mc_probe::{ActionBudget, GhosttyWindow};
    use crate::runtime::{
        Runtime, RuntimeConfig, RuntimeHandle, SubmitOutcome, TopologyMode,
        WorkspaceOccupancyReport,
    };
    use crate::workspace_indicator::{IndicatorMode, IndicatorOptions, SpaceOccupancy};

    const LEG_SETTLE_MS: u64 = 400;
    const CLICK_DELIVERY_MS: u64 = 1_500;
    const OCCUPANCY_AWAIT_SECS: u64 = 15;
    const RUNTIME_SHUTDOWN_SECS: u64 = 5;
    const ENGINE_FLUSH_MS: u64 = 2_000;

    /// LS holder-seconds bound: the helper needs its slack inside the probe
    /// maximum, so the holder is capped below it.
    fn ls_holder_seconds(seconds: u64) -> u64 {
        seconds.min(crate::config::MAX_PROBE_SECONDS - crate::probe_target::LS_HELPER_SLACK_SECS)
    }

    /// One running proof: legs/notes plus every owned handle and baseline.
    /// Every exit after the caffeinate companion spawns funnels through
    /// [`Live::cleanup_fail`], which runs the unified [`cleanup_plan`]
    /// executor first — no mutation/failure path skips restoration.
    struct Live {
        legs: Vec<ProofLegRecord>,
        notes: Vec<String>,
        caffeinate_reaped: bool,
        display: usize,
        original: u64,
        temp: Option<u64>,
        helper_pid: u32,
        helper_spawned: bool,
        nonce: String,
        runtime: Option<RuntimeHandle>,
        indicator: crate::tile::IndicatorGuard,
        select_state: crate::workspace_select::SelectionState,
        budget: ActionBudget,
        awake: crate::mc_lifecycle::live::CaffeinateGuard,
        pointer_before: (f64, f64),
        ghostty_before: Vec<GhosttyWindow>,
        release: PostDownRelease,
        last_point: Option<(f64, f64)>,
        our_pid: u32,
    }

    impl Live {
        fn leg(&mut self, leg: ProofLeg, complete: bool, detail: String) {
            self.legs.push(ProofLegRecord::new(leg, complete, detail));
        }

        fn note(&mut self, note: String) {
            self.notes.push(note);
        }

        fn cleanup_state(&self) -> CleanupState {
            CleanupState {
                helper_spawned: self.helper_spawned,
                temp_created: self.temp.is_some(),
                release_owed: self.release.requires_release(),
                runtime_started: self.runtime.is_some(),
            }
        }

        /// Terminal failure: run the unified cleanup first, then report
        /// the stage plus the restoration readbacks. The ONLY failure
        /// exit after the companion spawns.
        fn cleanup_fail(&mut self, stage: &str, error: String) -> (i32, String) {
            let restored = self.run_cleanup();
            let layout_after: Option<serde_json::Value> =
                crate::mc_lifecycle::live::fresh_snapshot()
                    .map(|s| serde_json::to_value(s.sanitized()).unwrap_or(serde_json::Value::Null))
                    .ok();
            self.note(format!("{stage}: {error}"));
            (
                1,
                serde_json::json!({
                    "action": "workspace-indicator-proof",
                    "stage": stage,
                    "error": error,
                    "legs": self.legs.iter().map(|leg| serde_json::json!({
                        "leg": leg.leg, "complete": leg.complete, "detail": leg.detail,
                    })).collect::<Vec<_>>(),
                    "layout_after": layout_after,
                    "restored": restored,
                    "caffeinate_reaped": self.caffeinate_reaped,
                    "complete": false,
                    "notes": self.notes,
                })
                .to_string(),
            )
        }

        /// Publish the fresh native topology snapshot to the fixture
        /// runtime (the admission gate for select claims) and return its
        /// revision. Mirrors the tile controller cadence read.
        fn publish_topology(&self) -> Result<u64, String> {
            let handle = self
                .runtime
                .as_ref()
                .ok_or_else(|| "topology publish needs the fixture runtime".to_owned())?;
            let ns = ffi::ns_display_snapshot().map_err(|e| format!("display snapshot: {e}"))?;
            let rows = ffi::sls_display_rows().map_err(|e| format!("space rows: {e}"))?;
            let separate = ffi::ns_separate_spaces();
            Ok(handle.update_topology(crate::topology::build_snapshot(&ns, &rows, separate)))
        }

        /// Fresh single-display user row plus the live revision it was
        /// read under (one consistent read for mapping and fencing).
        fn fresh_row(&self) -> Result<(crate::workspace_select::SelectionSnapshot, u64), String> {
            let rows = ffi::sls_display_rows().map_err(|e| format!("space rows: {e}"))?;
            let ns = ffi::ns_display_snapshot().map_err(|e| format!("display snapshot: {e}"))?;
            let revision =
                crate::topology::build_snapshot(&ns, &rows, ffi::ns_separate_spaces()).revision;
            let row = crate::workspace_select::user_row_from_rows(&rows, self.display)?;
            Ok((row, revision))
        }

        /// Await a revision-matched retained report with `sid` occupied.
        /// Fails closed on the deadline or on failed gates — never a
        /// parked Unknown claimed as proof.
        fn await_occupied(
            &self,
            revision: u64,
            sid: u64,
            what: &str,
        ) -> Result<WorkspaceOccupancyReport, String> {
            let handle = self
                .runtime
                .as_ref()
                .ok_or_else(|| "occupancy await needs the fixture runtime".to_owned())?;
            let deadline = std::time::Instant::now() + Duration::from_secs(OCCUPANCY_AWAIT_SECS);
            loop {
                if let Some(report) = handle.query_workspace_occupancy(Duration::from_millis(500))
                    && report_complete_for(&report, revision)
                    && domain_occupied(&report, sid)
                {
                    return Ok(report);
                }
                leg_gates()?;
                if std::time::Instant::now() >= deadline {
                    return Err(format!(
                        "retained occupancy for {what} never completed revision-matched and occupied"
                    ));
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }

        /// Product tap lane through the engine: submit, flush, claim, drain
        /// with immediately pre-dispatch gates, release, verified effect.
        /// No hand-crafted claim bypass: the engine admits and the pending
        /// slot carries the claim to the same drain the tile controller
        /// runs.
        fn drain_tap_via_engine(
            &mut self,
            action: crate::runtime_types::InputSemanticAction,
        ) -> Result<crate::workspace_select::SelectionEffect, String> {
            let claim = {
                let handle = self
                    .runtime
                    .as_ref()
                    .ok_or_else(|| "tap lane needs the fixture runtime".to_owned())?;
                match handle.submit_input_action(action) {
                    SubmitOutcome::Accepted => {}
                    SubmitOutcome::EngineQueueFull => {
                        return Err("tap submit refused: engine queue full".to_owned());
                    }
                    SubmitOutcome::AdmissionClosed => {
                        return Err("tap submit refused: admission closed".to_owned());
                    }
                    _ => {
                        return Err("tap submit refused by the engine lane".to_owned());
                    }
                }
                if !handle.flush(Duration::from_millis(ENGINE_FLUSH_MS)) {
                    return Err("tap submit unprocessed: engine flush timed out".to_owned());
                }
                handle
                    .claim_select_for_drain()
                    .ok_or_else(|| "tap claim missing after engine admission".to_owned())?
            };
            let outcome = crate::workspace_select::drain_live_claimed(
                &mut self.select_state,
                claim,
                &select_gates(),
            );
            if let Some(handle) = self.runtime.as_ref() {
                handle.release_select_after_drain();
            }
            let (success, _) =
                outcome.map_err(|failure| format!("tap drain refused: {}", failure.reason))?;
            Ok(success.effect)
        }

        /// Product indicator-click lane through the engine: resolve the
        /// delivered immutable binding fresh, submit it with its binding,
        /// flush, claim, drain, release, verified effect. The binding is
        /// fenced fresh both here and inside the drain — an ordinal is
        /// never interpreted against a different generation.
        fn drain_click_via_engine(
            &mut self,
            binding: crate::workspace_indicator::IndicatorClick,
            current_generation: u64,
            fresh_spaces: &[u64],
            fresh_revision: u64,
        ) -> Result<crate::workspace_select::SelectionEffect, String> {
            let action =
                resolve_proof_click(binding, current_generation, fresh_spaces, fresh_revision)
                    .map_err(|reason| format!("click admission refused: {reason}"))?;
            let claim = {
                let handle = self
                    .runtime
                    .as_ref()
                    .ok_or_else(|| "click lane needs the fixture runtime".to_owned())?;
                match handle.submit_indicator_select(
                    action,
                    binding.expected_sid,
                    binding.ui_generation,
                    binding.topology_revision,
                ) {
                    SubmitOutcome::Accepted => {}
                    SubmitOutcome::EngineQueueFull => {
                        return Err("click submit refused: engine queue full".to_owned());
                    }
                    SubmitOutcome::AdmissionClosed => {
                        return Err("click submit refused: admission closed".to_owned());
                    }
                    _ => {
                        return Err("click submit refused by the engine lane".to_owned());
                    }
                }
                if !handle.flush(Duration::from_millis(ENGINE_FLUSH_MS)) {
                    return Err("click submit unprocessed: engine flush timed out".to_owned());
                }
                handle
                    .claim_select_for_drain()
                    .ok_or_else(|| "click claim missing after engine admission".to_owned())?
            };
            let outcome = crate::workspace_select::drain_live_claimed(
                &mut self.select_state,
                claim,
                &select_gates(),
            );
            if let Some(handle) = self.runtime.as_ref() {
                handle.release_select_after_drain();
            }
            let (success, _) =
                outcome.map_err(|failure| format!("click drain refused: {}", failure.reason))?;
            Ok(success.effect)
        }

        /// Re-render the production guard from scratch (teardown plus tick
        /// always refreshes, retiring old bindings) and pump AppKit so the
        /// item installs. Returns the fresh render generation and the
        /// revision it was built under.
        fn render(
            &mut self,
            report: Option<WorkspaceOccupancyReport>,
        ) -> Result<(u64, u64), String> {
            let (_, revision) = self.fresh_row()?;
            let observations = self
                .runtime
                .as_ref()
                .map(|handle| handle.diagnostics().observations_published)
                .unwrap_or(0);
            let rows = ffi::sls_display_rows().map_err(|e| format!("space rows: {e}"))?;
            self.indicator.teardown();
            let frontmost = ffi::frontmost_app()
                .and_then(|(pid, _)| u32::try_from(pid).ok())
                .filter(|pid| *pid > 1);
            // Production pager inputs: the last applied reply presentation
            // plus its workspace bindings with the default highlight style
            // (this harness owns no settings file) and the live accent.
            let presentation = self
                .runtime
                .as_ref()
                .map(|handle| handle.last_presentation())
                .unwrap_or_default();
            let bindings = self
                .runtime
                .as_ref()
                .map(|handle| handle.last_pager_bindings())
                .unwrap_or_default();
            self.indicator.tick(
                &rows,
                revision,
                observations,
                report,
                frontmost,
                &presentation,
                &bindings,
                &crate::pager_highlight::PagerHighlightStyle::default(),
                ffi::border_system_accent(),
            );
            pump(LEG_SETTLE_MS);
            let generation = self
                .indicator
                .segment_targets()
                .first()
                .map(|(_, _, generation, _, _)| *generation)
                .unwrap_or(0);
            Ok((generation, revision))
        }

        /// Live segment occupancy words in render order, parsed back out
        /// of the actual button tooltips (never a cached model).
        fn live_words(&self) -> Vec<(usize, SpaceOccupancy)> {
            self.indicator
                .segment_snapshot()
                .into_iter()
                .map(|s| {
                    let occupancy = match s.occupancy.as_str() {
                        "occupied" => SpaceOccupancy::Occupied,
                        "empty" => SpaceOccupancy::Empty,
                        _ => SpaceOccupancy::Unknown,
                    };
                    (s.position, occupancy)
                })
                .collect()
        }

        /// Whether the owned helper process is still alive.
        fn helper_alive(&self) -> bool {
            self.helper_spawned && crate::ffi::process_exists(self.helper_pid as i32)
        }

        /// Whether the helper's live membership homes it exactly to `sid`.
        fn helper_homed_to(&self, sid: u64) -> bool {
            crate::mc_observe::enumerate_window_members()
                .map(|members| {
                    members
                        .iter()
                        .filter(|m| m.pid == self.helper_pid)
                        .filter_map(|m| m.spaces.as_ref().ok())
                        .any(|spaces| spaces.contains(&sid))
                })
                .unwrap_or(false)
        }

        /// The unified cleanup executor: runs [`cleanup_plan`] step for
        /// step. Gate-dependent mutation steps skip WITH residue notes when
        /// gates fail; every other step runs regardless. Each step records
        /// its token plus outcome; anything unproven sets `restored` false
        /// instead of forcing it. Returns whether exact restoration holds.
        fn run_cleanup(&mut self) -> bool {
            let mut restored = true;
            let mut gates_ok = true;
            let residue = |this: &mut Self, step: CleanupStep, reason: String| {
                this.note(format!(
                    "cleanup {}: residue ({reason})",
                    cleanup_step_token(step)
                ));
            };
            for step in cleanup_plan(&self.cleanup_state()) {
                if step_needs_gates(step) && !gates_ok {
                    let reason = "gates failed; mutation skipped, never actuated blind".to_owned();
                    residue(self, step, reason);
                    restored = false;
                    continue;
                }
                match step {
                    CleanupStep::CheckGates => {
                        if let Err(reason) = leg_gates() {
                            gates_ok = false;
                            self.note(format!("cleanup check-gates: {reason}"));
                        } else {
                            self.note("cleanup check-gates: pass".to_owned());
                        }
                    }
                    CleanupStep::ReleaseClickOwed => {
                        match self.last_point {
                            Some(point) => match post_tagged_edge(point, false) {
                                Ok(_) => self
                                    .note("cleanup release-click-owed: owed up posted".to_owned()),
                                Err(reason) => {
                                    restored = false;
                                    residue(self, step, reason);
                                }
                            },
                            None => {
                                restored = false;
                                residue(self, step, "down owed but no point recorded".to_owned());
                            }
                        }
                        self.release = PostDownRelease::Idle;
                    }
                    CleanupStep::GotoOriginal => {
                        if self.runtime.is_none() {
                            restored = false;
                            residue(
                                self,
                                step,
                                "no fixture runtime for the goto lane".to_owned(),
                            );
                            continue;
                        }
                        let at_home = self
                            .fresh_row()
                            .map(|(row, _)| row.current == self.original)
                            .unwrap_or(false);
                        if at_home {
                            self.note("cleanup goto-original: already home".to_owned());
                            continue;
                        }
                        if let Err(publish) = self.publish_topology() {
                            restored = false;
                            residue(self, step, publish);
                            continue;
                        }
                        match self.drain_tap_via_engine(
                            crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 1 },
                        ) {
                            Ok(effect) if effect.changed => {
                                // Verified below in VerifyLayout; the goto
                                // itself only dispatches here.
                                self.note("cleanup goto-original: dispatched".to_owned());
                            }
                            Ok(_) => {
                                self.note(
                                    "cleanup goto-original: no change (already home)".to_owned(),
                                );
                            }
                            Err(reason) => {
                                restored = false;
                                residue(self, step, reason);
                            }
                        }
                    }
                    CleanupStep::JoinHelperOriginal => {
                        if !self.helper_alive() {
                            self.note(
                                "cleanup join-helper-original: helper gone, nothing to home"
                                    .to_owned(),
                            );
                            continue;
                        }
                        if self.helper_homed_to(self.original) {
                            self.note("cleanup join-helper-original: already homed".to_owned());
                            continue;
                        }
                        match crate::space_join::run_owned_space_join(self.helper_pid, &self.nonce)
                        {
                            Ok(_) if self.helper_homed_to(self.original) => {
                                self.note(
                                    "cleanup join-helper-original: re-homed with readback"
                                        .to_owned(),
                                );
                            }
                            Ok(_) => {
                                restored = false;
                                residue(
                                    self,
                                    step,
                                    "self-join ran but homing readback disagrees".to_owned(),
                                );
                            }
                            Err((_, reason)) => {
                                restored = false;
                                residue(self, step, reason);
                            }
                        }
                    }
                    CleanupStep::RemoveTempIfEmpty => {
                        let Some(temp) = self.temp else {
                            self.note("cleanup remove-temp-if-empty: no temp adopted".to_owned());
                            continue;
                        };
                        let mut dispatched = false;
                        match crate::mc_lifecycle::remove_sid_verified(
                            temp,
                            &mut self.budget,
                            true,
                            &mut dispatched,
                        ) {
                            Ok(_) => {
                                self.temp = None;
                                self.note(
                                    "cleanup remove-temp-if-empty: removed verified".to_owned(),
                                );
                            }
                            Err(reason) => {
                                restored = false;
                                residue(
                                    self,
                                    step,
                                    format!("{reason}; never replayed, never broadened"),
                                );
                            }
                        }
                    }
                    CleanupStep::VerifyLayout => {
                        match crate::mc_lifecycle::live::fresh_snapshot() {
                            Ok(snapshot) => {
                                let exact = snapshot.displays.len() == 1
                                    && snapshot.displays.first().is_some_and(|row| {
                                        row.spaces == vec![self.original]
                                            && row.current == Some(self.original)
                                    });
                                if exact {
                                    self.note(
                                        "cleanup verify-layout: one-Space/current preimage exact"
                                            .to_owned(),
                                    );
                                } else {
                                    restored = false;
                                    residue(
                                        self,
                                        step,
                                        "layout is not the exact preimage".to_owned(),
                                    );
                                }
                            }
                            Err(reason) => {
                                restored = false;
                                residue(self, step, reason.to_string());
                            }
                        }
                    }
                    CleanupStep::VerifyGhostty => {
                        match crate::mc_lifecycle::snapshot_ghostty_windows() {
                            Ok(after) if after == self.ghostty_before => {
                                self.note(
                                    "cleanup verify-ghostty: frame/membership equal".to_owned(),
                                );
                            }
                            Ok(after) => {
                                restored = false;
                                residue(
                                    self,
                                    step,
                                    format!(
                                        "protected Ghostty moved (before-windows={} after-windows={})",
                                        self.ghostty_before.len(),
                                        after.len()
                                    ),
                                );
                            }
                            Err(reason) => {
                                restored = false;
                                residue(self, step, reason.to_string());
                            }
                        }
                    }
                    CleanupStep::VerifyPointer => {
                        match crate::synth::cursor_position() {
                            Some(after) if after == self.pointer_before => {
                                self.note("cleanup verify-pointer: preimage held".to_owned());
                            }
                            Some(_) => {
                                // One eventless restore attempt, then a
                                // second read decides (never synthetic
                                // motion, never a fight with the user).
                                let _ = ffi::warp_cursor_position(
                                    self.pointer_before.0,
                                    self.pointer_before.1,
                                );
                                match crate::synth::cursor_position() {
                                    Some(home) if home == self.pointer_before => {
                                        self.note(
                                            "cleanup verify-pointer: restored eventlessly"
                                                .to_owned(),
                                        );
                                    }
                                    _ => {
                                        restored = false;
                                        residue(
                                            self,
                                            step,
                                            "pointer preimage not restored".to_owned(),
                                        );
                                    }
                                }
                            }
                            None => {
                                restored = false;
                                residue(self, step, "pointer readback unreadable".to_owned());
                            }
                        }
                        match ffi::frontmost_app().map(|(_, bundle)| bundle) {
                            Some(bundle) => {
                                match cleanup_focus_verdict(Some(bundle.as_str())) {
                                    Ok(()) => self.note(
                                        "cleanup verify-pointer: frontmost readable, never Ghostty/protected"
                                            .to_owned(),
                                    ),
                                    Err(reason) => {
                                        restored = false;
                                        residue(self, step, reason);
                                    }
                                }
                            }
                            None => {
                                restored = false;
                                residue(self, step, "frontmost unreadable".to_owned());
                            }
                        }
                    }
                    CleanupStep::TeardownIndicator => {
                        let before = our_window_wids(self.our_pid);
                        self.indicator.teardown();
                        pump(LEG_SETTLE_MS);
                        let after = our_window_wids(self.our_pid);
                        match verify_teardown_absent(&before, &after) {
                            Ok(()) => self.note(
                                "cleanup teardown-indicator: status windows absent".to_owned(),
                            ),
                            Err(reason) if before.is_empty() => {
                                self.note(format!(
                                    "cleanup teardown-indicator: no status windows to remove ({reason})"
                                ));
                            }
                            Err(reason) => {
                                restored = false;
                                residue(self, step, reason);
                            }
                        }
                    }
                    CleanupStep::ShutdownRuntime => match self.runtime.take() {
                        Some(handle) => {
                            let report =
                                handle.shutdown(Duration::from_secs(RUNTIME_SHUTDOWN_SECS));
                            let clean = matches!(
                                report.outcome,
                                crate::runtime_lifecycle::ShutdownOutcome::Clean
                            );
                            self.note(format!(
                                    "cleanup shutdown-runtime: outcome-clean={clean} joined={} retained={} panics={} unclean={}",
                                    report.joined,
                                    report.retained,
                                    report.panics,
                                    report.unclean
                                ));
                            if !clean {
                                restored = false;
                            }
                        }
                        None => {
                            self.note("cleanup shutdown-runtime: never started".to_owned());
                        }
                    },
                    CleanupStep::StopHelperExact => {
                        if crate::ffi::process_exists(self.helper_pid as i32) {
                            match crate::probe_target::stop_session(self.helper_pid, &self.nonce) {
                                Ok(()) => self.note(
                                    "cleanup stop-helper-exact: exact PID stopped, record removed"
                                        .to_owned(),
                                ),
                                Err(reason) => {
                                    restored = false;
                                    residue(self, step, reason);
                                }
                            }
                        } else {
                            let cleared = crate::probe_target::remove_ls_record_if_gone(
                                self.helper_pid,
                                &self.nonce,
                            );
                            self.note(format!(
                                "cleanup stop-helper-exact: already gone, stale record cleared={cleared}"
                            ));
                        }
                    }
                    CleanupStep::StopCaffeinate => {
                        let reaped = self.awake.stop();
                        self.caffeinate_reaped = reaped;
                        self.note(format!("cleanup stop-caffeinate: reaped={reaped}"));
                    }
                }
            }
            restored
        }
    }

    fn err_stage(stage: &str, error: String) -> (i32, String) {
        (
            1,
            serde_json::json!({"action": "workspace-indicator-proof", "stage": stage, "error": error})
                .to_string(),
        )
    }

    /// Per-leg gates: fresh unlocked-session verdict, AX trust, post
    /// access, Secure Input off, lock screen off. Runs at every leg
    /// boundary (the window-server session read carries the
    /// screen-lock marker, so a lock between legs stops the battery
    /// before the next effect); any denial stops before the next effect.
    fn leg_gates() -> Result<(), String> {
        match ffi::session_state().lock {
            SessionLock::Unlocked => {}
            SessionLock::Locked => return Err("leg gated: session locked".to_owned()),
            SessionLock::Unknown => return Err("leg gated: session unknown".to_owned()),
        }
        if ffi::ax_is_process_trusted() != Some(true) {
            return Err("leg gated: AX trust not observed true".to_owned());
        }
        if ffi::cg_post_event_access_known() != Some(true) {
            return Err("leg gated: window-server post access not granted".to_owned());
        }
        if ffi::secure_input_known() == Some(true) {
            return Err("leg gated: Secure Input active".to_owned());
        }
        if ffi::frontmost_app()
            .is_some_and(|(_, bundle)| crate::focus::is_loginwindow_bundle(&bundle))
        {
            return Err("leg gated: lock screen frontmost".to_owned());
        }
        Ok(())
    }

    /// Product select gates, mirroring the tile controller drain exactly
    /// (see `tile.rs` product select drain): session lock, Secure Input,
    /// and lock-screen frontmost sampled immediately pre-dispatch.
    fn select_gates() -> crate::workspace_select::SelectGates {
        crate::workspace_select::SelectGates {
            session_unlocked: matches!(ffi::session_state().lock, SessionLock::Unlocked),
            secure_active: matches!(ffi::secure_input_known(), Some(true)),
            lock_screen: ffi::frontmost_app()
                .map(|(_, bundle)| crate::focus::is_loginwindow_bundle(&bundle))
                .unwrap_or(true),
        }
    }

    /// Live CG window IDs owned by `pid` (read-only snapshot; IDs only).
    fn our_window_wids(pid: u32) -> Vec<u32> {
        ffi::cg_window_list_basic()
            .map(|list| {
                list.into_iter()
                    .filter(|w| w.pid == pid)
                    .map(|w| w.wid)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Post one tagged mouse edge through the SESSION tap (fixture
    /// provenance tag, inert for mouse delivery — the tap never consumes
    /// clicks — but marking our pair). Safe `objc2-core-graphics`
    /// wrappers only; no new `unsafe` (this file forbids it).
    fn post_tagged_edge(point: (f64, f64), down: bool) -> Result<u64, String> {
        use objc2_core_foundation::CGPoint;
        use objc2_core_graphics::{
            CGEvent, CGEventField, CGEventTapLocation, CGEventType, CGMouseButton,
        };
        let ty = if down {
            CGEventType::LeftMouseDown
        } else {
            CGEventType::LeftMouseUp
        };
        let event = CGEvent::new_mouse_event(
            None,
            ty,
            CGPoint {
                x: point.0,
                y: point.1,
            },
            CGMouseButton::Left,
        )
        .ok_or_else(|| "CGEventCreateMouseEvent returned null".to_owned())?;
        CGEvent::set_integer_value_field(
            Some(&event),
            CGEventField::EventSourceUserData,
            crate::input_match::OWN_SWITCH_USER_DATA,
        );
        let start = std::time::Instant::now();
        CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
        Ok(start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
    }

    /// Panic-after-down safety net: an armed guard posts the owed balanced
    /// release at the down point when dropped (including panic unwinding),
    /// so a post-mutation panic can never strand the button system-wide.
    /// The normal path disarms after posting its fresh-identity up edge;
    /// a guard-fired release is safety cleanup, never a successful
    /// identity proof. This covers the button only — full layout
    /// restoration on panic still relies on the existing exact cleanup
    /// (all non-panic exits) plus the helper holder self-exit and the
    /// documented out-of-tap user recovery; catching a panic across live
    /// AppKit/Runtime state to run the full cleanup risks a double panic
    /// and is deliberately not attempted here.
    struct PanicReleaseGuard {
        point: Option<(f64, f64)>,
        armed: bool,
    }

    impl PanicReleaseGuard {
        fn armed(point: (f64, f64)) -> Self {
            Self {
                point: Some(point),
                armed: true,
            }
        }

        fn disarm(&mut self) {
            self.armed = false;
            self.point = None;
        }
    }

    impl Drop for PanicReleaseGuard {
        fn drop(&mut self) {
            if self.armed
                && let Some(point) = self.point
            {
                let _ = post_tagged_edge(point, false);
            }
        }
    }

    /// Pump the main runloop so AppKit installs/delivers the status item
    /// and segment actions. Read-only sleeps plus the maintained pump —
    /// never a synthetic event.
    fn pump(ms: u64) {
        ffi::pump_runloop_once(ms);
    }

    /// Start the bounded fixture runtime scoped EXCLUSIVELY to the signed
    /// helper bundle: the LS-launched helper carries the ordinary product
    /// identity (bundle plus executable plus launch date) the scoped
    /// backend binds, so its observations publish and its retained domain
    /// homes. Input stays on the product off route (no tap): the proof
    /// posts exactly one synthetic segment pair itself. Engine actuation
    /// on the owned helper window is authorized for the live unit.
    fn start_fixture_runtime() -> Result<RuntimeHandle, String> {
        let scope = crate::scope::AppScope {
            allowed: vec![crate::HELPER_BUNDLE_ID.to_owned()],
        };
        let ax_factory = Arc::new(
            crate::native_tile_backend::ScopedAxFactory::new(scope)
                .map_err(|reason| format!("fixture scope refused: {reason}"))?,
        );
        let input_factory = Arc::new(crate::native_input::OffTileFactory);
        let config = RuntimeConfig {
            topology_mode: TopologyMode::Native,
            allowed_bundles: vec![crate::HELPER_BUNDLE_ID.to_owned()],
            ..RuntimeConfig::default()
        };
        Runtime::start(config, ax_factory, input_factory)
            .map_err(|failure| format!("fixture runtime start refused: {}", failure.error()))
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn run(
        seconds: u64,
        display: usize,
        allow_untested: bool,
    ) -> Result<String, (i32, String)> {
        let config =
            validate_proof_args(seconds, display, allow_untested).map_err(|reason| (2, reason))?;
        // Fail-closed bundle layout BEFORE anything: the fixture must run
        // from the signed LaunchServices bundle (sibling Helper.app beside
        // it) — never a plain `cargo` executable, which has no
        // LaunchServices identity for the scoped backend to bind. Later
        // units launch via `just launch -- workspace-indicator-proof ...`.
        let in_bundle = std::env::current_exe()
            .ok()
            .is_some_and(|exe| bundle_layout_ok(&exe));
        if !in_bundle {
            return Err((
                2,
                "workspace-indicator-proof must run from the signed LaunchServices bundle (no sibling Helper.app beside a cargo executable); launch via `just launch -- workspace-indicator-proof --seconds N --allow-untested`"
                    .to_owned(),
            ));
        }
        let our_pid = std::process::id();
        let t0 = std::time::Instant::now();
        let budget_total = Duration::from_secs(config.seconds);

        // Leg: baseline. Fresh gates, single display, original one-Space
        // shape, Ghostty/focus/pointer preimages, caffeinate companion.
        leg_gates().map_err(|e| err_stage("baseline-gates", e))?;
        let before_snapshot = crate::mc_lifecycle::live::fresh_snapshot()
            .map_err(|e| err_stage("baseline-snapshot", e.to_string()))?;
        if before_snapshot.displays.len() != 1 {
            return Err(err_stage(
                "baseline-topology",
                format!(
                    "expected exactly one native display, found {}",
                    before_snapshot.displays.len()
                ),
            ));
        }
        let before_row = before_snapshot.displays[display].clone();
        let original_current = before_row.current.ok_or_else(|| {
            err_stage(
                "baseline-topology",
                "chosen display has no current Space in the preimage".to_owned(),
            )
        })?;
        if before_row.spaces.len() != 1 {
            return Err(err_stage(
                "baseline-topology",
                format!(
                    "baseline is not the original one-Space shape (found {} Spaces)",
                    before_row.spaces.len()
                ),
            ));
        }
        let original = before_row.spaces[0];
        if original_current != original {
            return Err(err_stage(
                "baseline-topology",
                "one-Space baseline is not current on its own Space; refusing".to_owned(),
            ));
        }
        let ghostty_before: Vec<GhosttyWindow> = crate::mc_lifecycle::snapshot_ghostty_windows()
            .map_err(|e| err_stage("baseline-ghostty", e.to_string()))?;
        let focus_before = crate::ffi::frontmost_app().map(|(pid, _)| pid as u32);
        let pointer_before = crate::synth::cursor_position().ok_or_else(|| {
            err_stage("baseline-pointer", "cursor preimage unreadable".to_owned())
        })?;
        // The indicator owns AppKit state from here: accessory policy (no
        // Dock icon, no activation), matching the tile controller.
        let mtm = objc2::MainThreadMarker::new().ok_or_else(|| {
            err_stage("indicator-main-thread", "no main-thread marker".to_owned())
        })?;
        {
            use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            app.finishLaunching();
        }
        let awake = crate::mc_lifecycle::live::CaffeinateGuard::spawn(config.seconds)
            .map_err(|e| err_stage("baseline-caffeinate", e.to_string()))?;
        let mut live = Live {
            legs: Vec::new(),
            notes: Vec::new(),
            caffeinate_reaped: false,
            display,
            original,
            temp: None,
            helper_pid: 0,
            helper_spawned: false,
            nonce: String::new(),
            runtime: None,
            indicator: crate::tile::IndicatorGuard::new(
                mtm,
                IndicatorOptions {
                    mode: IndicatorMode::All,
                    icons: false,
                },
            ),
            select_state: crate::workspace_select::SelectionState::default(),
            budget: ActionBudget::new(config.seconds),
            awake,
            pointer_before,
            ghostty_before,
            release: PostDownRelease::Idle,
            last_point: None,
            our_pid,
        };
        // (The companion moves into `live` exactly once here; every exit
        // below funnels through `cleanup_fail`, which stops it last.)
        live.leg(
            ProofLeg::Baseline,
            true,
            format!(
                "one-Space baseline current-position=1 ghostty-windows={} frontmost-captured={} pointer-captured=true ls-bundle-layout=true",
                live.ghostty_before.len(),
                focus_before.is_some()
            ),
        );

        // Leg: spawn the owned helper through LaunchServices (the signed
        // bundled Helper.app via `open -n --args`): ordinary product
        // identity (bundle plus executable plus launch date) the scoped
        // backend binds. No Child is held — currency is the spawn record
        // plus live-exe plus CURRENT window-nonce title, re-proven by
        // `stop_session` at the exact PID. The holder bound leaves the
        // helper slack so the fixture always stops a live helper.
        leg_gates().map_err(|e| live.cleanup_fail("spawn-gates", e))?;
        let nonce =
            crate::probe_target::mint_nonce().map_err(|e| live.cleanup_fail("spawn-nonce", e))?;
        let holder_seconds = ls_holder_seconds(config.seconds);
        let spec = crate::probe_target::ProbeTargetSpec::new(&nonce, 0, 0, holder_seconds)
            .map(|s| s.with_regular_policy(true).with_space_join(true))
            .map_err(|e| live.cleanup_fail("spawn-spec", e))?;
        let session = crate::probe_target::spawn_verified_ls(&spec, holder_seconds)
            .map_err(|e| live.cleanup_fail("spawn-ls", e))?;
        if session.bundle != crate::HELPER_BUNDLE_ID {
            let _ = crate::probe_target::stop_session(session.pid, &session.nonce);
            return Err(live.cleanup_fail(
                "spawn-identity",
                "LS helper bundle is not the signed helper (failing closed, helper stopped)"
                    .to_owned(),
            ));
        }
        live.helper_pid = session.pid;
        live.helper_spawned = true;
        live.nonce = session.nonce.clone();
        live.leg(
            ProofLeg::SpawnHelper,
            true,
            format!(
                "ls helper spawned with product identity bundle-verified=true holder-seconds={holder_seconds}"
            ),
        );

        // Bounded fixture runtime, scoped exclusively to the helper
        // bundle, plus the first published native topology (the admission
        // gate for select claims) and the launched worker (its scoped
        // observations publish the retained domains the occupancy legs
        // assert over).
        leg_gates().map_err(|e| live.cleanup_fail("runtime-gates", e))?;
        let handle = start_fixture_runtime().map_err(|e| live.cleanup_fail("runtime-start", e))?;
        live.runtime = Some(handle);
        let revision = live
            .publish_topology()
            .map_err(|e| live.cleanup_fail("runtime-topology", e))?;
        live.note(format!("published native topology revision={revision}"));
        match live
            .runtime
            .as_ref()
            .expect("runtime just started")
            .launch_app(live.helper_pid)
        {
            Ok(generation) => live.note(format!("helper worker launched generation={generation}")),
            Err(reason) => {
                return Err(live.cleanup_fail(
                    "runtime-launch",
                    format!("helper worker refused: {reason:?}"),
                ));
            }
        }

        // Leg: prove the independent public self-join restore route BEFORE
        // any Space-affecting leg (the same route the cleanup uses after
        // the mover path is gone).
        leg_gates().map_err(|e| live.cleanup_fail("restore-route-gates", e))?;
        if let Err((_, e)) = crate::space_join::run_owned_space_join(live.helper_pid, &live.nonce) {
            return Err(live.cleanup_fail("restore-route", e));
        }
        if !live.helper_homed_to(live.original) {
            return Err(live.cleanup_fail(
                "restore-route",
                "helper not homed to the original after the proven join".to_owned(),
            ));
        }
        live.leg(
            ProofLeg::ProveRestoreRoute,
            true,
            "public self-join proven with homing readback".to_owned(),
        );

        // Leg: create exactly one temp Space on display 0 through the
        // existing Mission Control fixtures (adopted only through the
        // exactly-one-new-SID gate). Row becomes [original, temp].
        leg_gates().map_err(|e| live.cleanup_fail("create-gates", e))?;
        let create_outcome =
            crate::mc_lifecycle::live::mc_add_space(display, &mut live.budget, false)
                .map_err(|e| live.cleanup_fail("create-dispatch", e.to_string()))?;
        if let Some(c) = create_outcome.close_error {
            live.note(format!("create close shortfall: {c}"));
        }
        let created_snapshot = crate::mc_lifecycle::live::await_snapshot(&|s| {
            crate::mc_probe::validate_create_effect(&before_snapshot, s, display).is_ok()
        })
        .map_err(|e| live.cleanup_fail("create-readback", e.to_string()))?;
        let temp =
            crate::mc_probe::validate_create_effect(&before_snapshot, &created_snapshot, display)
                .map_err(|e| live.cleanup_fail("create-adopt", e.to_string()))?;
        if let Err(e) = crate::mc_lifecycle::require_user_temp(temp) {
            return Err(live.cleanup_fail("create-type", e.to_string()));
        }
        live.temp = Some(temp);
        let revision = live
            .publish_topology()
            .map_err(|e| live.cleanup_fail("create-topology", e))?;
        live.leg(
            ProofLeg::CreateTemp,
            true,
            format!("temp-adopted row-positions=2 user-typed=true revision={revision}"),
        );

        // Leg: indicator appear. The production guard renders the fresh
        // row over the REVISION-MATCHED retained report (required: the
        // helper is homed to the original, so it reads Occupied; the
        // fresh temp may read Empty or, without a named domain, Unknown
        // — never a lied Empty). Every field below reads live AppKit
        // state: titles must equal `expected_titles` ('?' iff Unknown),
        // current is exactly one segment, menu mirrors the bar.
        leg_gates().map_err(|e| live.cleanup_fail("appear-gates", e))?;
        let report = live
            .await_occupied(revision, original, "original")
            .map_err(|e| live.cleanup_fail("appear-occupancy", e))?;
        let (generation, _) = live
            .render(Some(report))
            .map_err(|e| live.cleanup_fail("appear-render", e))?;
        {
            let snapshot = live.indicator.segment_snapshot();
            let targets = live.indicator.segment_targets();
            let titles = live.indicator.row_titles();
            let frame = live.indicator.status_frame();
            let words = live.live_words();
            if snapshot.len() != 2 || targets.len() != 2 {
                return Err(live.cleanup_fail(
                    "appear",
                    format!(
                        "status item did not render the [original, temp] row (segments={} targets={})",
                        snapshot.len(),
                        targets.len()
                    ),
                ));
            }
            let expected = expected_titles(&words);
            let live_titles: Vec<String> = snapshot.iter().map(|s| s.title.clone()).collect();
            if live_titles != expected || titles != expected {
                return Err(live.cleanup_fail(
                    "appear",
                    "segment/menu titles do not equal the '?' iff Unknown contract".to_owned(),
                ));
            }
            if snapshot.iter().filter(|s| s.current).count() != 1 {
                return Err(live.cleanup_fail(
                    "appear",
                    "current highlight is not exactly one segment".to_owned(),
                ));
            }
            let occupied: Vec<usize> = words
                .iter()
                .filter(|(_, o)| *o == SpaceOccupancy::Occupied)
                .map(|(position, _)| *position)
                .collect();
            if occupied != vec![1] {
                return Err(live.cleanup_fail(
                    "appear",
                    "retained occupancy does not name the helper homing (position 1 must read Occupied)"
                        .to_owned(),
                ));
            }
            if frame.is_none() {
                return Err(live.cleanup_fail("appear", "status item frame unreadable".to_owned()));
            }
            live.leg(
                ProofLeg::IndicatorAppear,
                true,
                format!(
                    "segments=2 titles={} current-count=1 occupied=[1] generation={generation}",
                    expected.join(",")
                ),
            );
        }

        // Leg: native Space-switch update through the product tap lane
        // (engine submit, flush, claim, guarded drain, release): goto
        // temp with verified arrival, then a re-render proving the
        // current highlight followed the switch.
        leg_gates().map_err(|e| live.cleanup_fail("switch-gates", e))?;
        {
            let (fresh, _) = live
                .fresh_row()
                .map_err(|e| live.cleanup_fail("switch-row", e))?;
            if fresh.spaces != vec![original, temp] {
                return Err(live.cleanup_fail(
                    "switch-row",
                    "fresh row is not the exact [original, temp] shape".to_owned(),
                ));
            }
            // Fresh publish before dispatch (admission gates on a
            // present native context); the post-switch revision is
            // re-published after arrival below.
            live.publish_topology()
                .map_err(|e| live.cleanup_fail("switch-topology", e))?;
            let effect = live
                .drain_tap_via_engine(crate::runtime_types::InputSemanticAction::WorkspaceSelect {
                    index: 2,
                })
                .map_err(|e| live.cleanup_fail("switch-update", e))?;
            if !effect.changed {
                return Err(live.cleanup_fail(
                    "switch-update",
                    "switch dispatched but reported no change".to_owned(),
                ));
            }
            let (after, _) = live
                .fresh_row()
                .map_err(|e| live.cleanup_fail("switch-readback", e))?;
            if after.current != temp {
                return Err(live.cleanup_fail(
                    "switch-update",
                    "arrival unverified: current is not temp".to_owned(),
                ));
            }
            // Re-publish AFTER the switch (the current move advances the
            // revision), then await the matched report for the re-render.
            let revision = live
                .publish_topology()
                .map_err(|e| live.cleanup_fail("switch-topology", e))?;
            let report = live
                .await_occupied(revision, original, "original-after-switch")
                .map_err(|e| live.cleanup_fail("switch-occupancy", e))?;
            let (generation2, _) = live
                .render(Some(report))
                .map_err(|e| live.cleanup_fail("switch-rerender", e))?;
            let current_positions: Vec<usize> = live
                .indicator
                .segment_snapshot()
                .into_iter()
                .filter(|s| s.current)
                .map(|s| s.position)
                .collect();
            if current_positions != vec![2] {
                return Err(live.cleanup_fail(
                    "switch-update",
                    "current highlight did not follow the native switch".to_owned(),
                ));
            }
            live.leg(
                ProofLeg::SwitchUpdate,
                true,
                format!(
                    "engine tap-lane arrival verified current-position=2 generation={generation2}"
                ),
            );
        }

        // Helper follows to temp through the proven public route (current
        // is temp, so the self-join homes it there), then current returns
        // to the original through the engine tap lane: the helper stays
        // homed to temp while temp is OFF-current.
        leg_gates().map_err(|e| live.cleanup_fail("homing-gates", e))?;
        if let Err((_, e)) = crate::space_join::run_owned_space_join(live.helper_pid, &live.nonce) {
            return Err(live.cleanup_fail("homing-join", e));
        }
        if !live.helper_homed_to(temp) {
            return Err(live.cleanup_fail(
                "homing-join",
                "helper not homed to temp after the proven join".to_owned(),
            ));
        }
        live.note("helper homed to temp with live membership readback".to_owned());
        {
            // Fresh publish before dispatch (admission gates on a present
            // native context); the revision itself is re-read after.
            live.publish_topology()
                .map_err(|e| live.cleanup_fail("homing-topology", e))?;
            let effect = live
                .drain_tap_via_engine(crate::runtime_types::InputSemanticAction::WorkspaceSelect {
                    index: 1,
                })
                .map_err(|e| live.cleanup_fail("homing-back", e))?;
            if !effect.changed {
                return Err(live.cleanup_fail(
                    "homing-back",
                    "return dispatched but reported no change".to_owned(),
                ));
            }
            let (after, _) = live
                .fresh_row()
                .map_err(|e| live.cleanup_fail("homing-readback", e))?;
            if after.current != original {
                return Err(live.cleanup_fail(
                    "homing-back",
                    "return unverified: current is not the original".to_owned(),
                ));
            }
        }

        // Leg: off-current retained occupancy, REQUIRED (never parked):
        // current is the original, the helper is homed to temp, and the
        // revision-matched report must name temp Occupied. The pure
        // `require_occupied_off_current` gate pins the assertion shape.
        leg_gates().map_err(|e| live.cleanup_fail("occupancy-gates", e))?;
        let revision = live
            .publish_topology()
            .map_err(|e| live.cleanup_fail("occupancy-topology", e))?;
        let report = live
            .await_occupied(revision, temp, "temp-off-current")
            .map_err(|e| live.cleanup_fail("off-current-occupancy", e))?;
        {
            let (_, _) = live
                .render(Some(report))
                .map_err(|e| live.cleanup_fail("occupancy-render", e))?;
            let words = live.live_words();
            require_occupied_off_current(2, &words)
                .map_err(|e| live.cleanup_fail("off-current-occupancy", e))?;
            let snapshot = live.indicator.segment_snapshot();
            let live_titles: Vec<String> = snapshot.iter().map(|s| s.title.clone()).collect();
            if live_titles != expected_titles(&words)
                || live.indicator.row_titles() != expected_titles(&words)
            {
                return Err(live.cleanup_fail(
                    "off-current-occupancy",
                    "segment/menu titles do not equal the '?' iff Unknown contract".to_owned(),
                ));
            }
            live.leg(
                ProofLeg::OffCurrentOccupancy,
                true,
                format!(
                    "temp-position=2 retained-Occupied off-current titles={}",
                    expected_titles(&words).join(",")
                ),
            );
        }

        // Leg: fresh synthetic mouse down/up pair on OUR segment for
        // position 2 (temp). Coordinates compute freshly from the live
        // segment frame and current topology before EACH edge; BOTH the
        // fresh AX positional owner and the WindowServer hit must bind
        // exactly our status window and PID at each edge (Ghostty refuses
        // explicitly; a system-hosted menu bar refuses — never an assumed
        // PID); `performClick` is never a substitute. No AppKit select
        // runs before the release: delivery is drained only after the up.
        // The down's up posts unconditionally on every post-down exit
        // (balanced contract, enforced by the unified cleanup and the
        // panic-drop guard too); a refused up-identity still releases at
        // the down point as labeled cleanup, never as proof, and never
        // at a foreign/Ghostty point.
        leg_gates().map_err(|e| live.cleanup_fail("click-gates", e))?;
        let focus_before_click = crate::ffi::frontmost_app().map(|(pid, _)| pid as u32);
        let (click_binding, click_point, down_frame, down_revision) = {
            let (fresh, fresh_revision) = live
                .fresh_row()
                .map_err(|e| live.cleanup_fail("click-row", e))?;
            if fresh.spaces != vec![original, temp] {
                return Err(live.cleanup_fail(
                    "click-row",
                    "fresh row is not the exact [original, temp] shape".to_owned(),
                ));
            }
            let targets = live.indicator.segment_targets();
            let (position, sid, binding_generation, binding_revision, binding_kind) = targets
                .iter()
                .find(|(position, _, _, _, _)| *position == 2)
                .copied()
                .ok_or_else(|| {
                    live.cleanup_fail("click-target", "no live target for position 2".to_owned())
                })?;
            if sid != temp {
                return Err(live.cleanup_fail(
                    "click-target",
                    "position 2 does not name the temp SID".to_owned(),
                ));
            }
            if binding_kind != crate::workspace_indicator::IndicatorClickKind::Numbered {
                return Err(live.cleanup_fail(
                    "click-target",
                    "position 2 read back a non-numbered slot kind".to_owned(),
                ));
            }
            let binding = crate::workspace_indicator::IndicatorClick {
                kind: binding_kind,
                position,
                expected_sid: sid,
                ui_generation: binding_generation,
                topology_revision: binding_revision,
            };
            // Admission BEFORE any post: stale refuses here, never after
            // a down. The generation rides the latest render.
            let current_generation = targets
                .first()
                .map(|(_, _, generation, _, _)| *generation)
                .unwrap_or(binding_generation);
            let action =
                resolve_proof_click(binding, current_generation, &fresh.spaces, fresh_revision)
                    .map_err(|reason| live.cleanup_fail("click-admission", reason))?;
            let want_action =
                crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 2 };
            if action != want_action {
                return Err(live.cleanup_fail(
                    "click-admission",
                    "admission resolved the wrong action".to_owned(),
                ));
            }
            // Fresh frame for position 2 plus display geometry, then the
            // point. A re-read guards drift between observations.
            let frame_now = live
                .indicator
                .segment_snapshot()
                .into_iter()
                .find(|s| s.position == 2)
                .map(|s| s.frame)
                .ok_or_else(|| {
                    live.cleanup_fail("click-frame", "position 2 has no live frame".to_owned())
                })?;
            let displays =
                ffi::ns_display_snapshot().map_err(|e| live.cleanup_fail("click-geometry", e))?;
            if displays.len() != 1 {
                return Err(live.cleanup_fail(
                    "click-geometry",
                    "display geometry is not single-display fresh".to_owned(),
                ));
            }
            let d = &displays[0];
            let point =
                segment_click_point_cg(frame_now, (d.frame.x, d.frame.y, d.frame.w, d.frame.h))
                    .map_err(|reason| live.cleanup_fail("click-point", reason))?;
            let frame_fresh = live
                .indicator
                .segment_snapshot()
                .into_iter()
                .find(|s| s.position == 2)
                .map(|s| s.frame)
                .ok_or_else(|| {
                    live.cleanup_fail(
                        "click-frame",
                        "position 2 frame unreadable on re-read".to_owned(),
                    )
                })?;
            let (_, revision_fresh) = live
                .fresh_row()
                .map_err(|e| live.cleanup_fail("click-row", e))?;
            verify_frame_fresh(frame_now, frame_fresh, fresh_revision, revision_fresh)
                .map_err(|reason| live.cleanup_fail("click-drift", reason))?;
            // Dual hit identity, freshly read at the exact point.
            let ax_owner = ffi::ax_hit_test_owner(point.0, point.1).ok();
            let cg_hit = ffi::sls_window_at_point(point.0, point.1).ok();
            let hit_bundle = ax_owner.and_then(|(pid, _)| ffi::bundle_id_for_pid(pid as i32));
            verify_segment_hit_identity(&SegmentHitProof {
                our_pid,
                ax_owner,
                cg_hit_wid: cg_hit,
                our_window_wids: our_window_wids(our_pid),
                hit_bundle,
            })
            .map_err(|reason| live.cleanup_fail("click-identity", reason))?;
            (binding, point, frame_fresh, revision_fresh)
        };
        // The pair: down posts only after every gate above passed
        // (refuse-before-down). The up edge revalidates independently
        // below at recomputed coordinates — never a reuse of the down
        // point without fresh proof.
        let down_latency = post_tagged_edge(click_point, true)
            .map_err(|reason| live.cleanup_fail("click-down", reason))?;
        if let Err(reason) = live.release.note_down() {
            let _ = post_tagged_edge(click_point, false);
            return Err(live.cleanup_fail("click-down", reason));
        }
        live.last_point = Some(click_point);
        // Armed until the normal up posts: a panic after this point
        // still releases the button through the guard's Drop (cleanup,
        // never proof). The normal path disarms right after its up.
        let mut panic_guard = PanicReleaseGuard::armed(click_point);
        // Independent up revalidation: fresh geometry, recomputed point,
        // and fresh dual hit identity AFTER the down, BEFORE the up. A
        // refusal here still releases at the DOWN point as balanced
        // safety cleanup (labeled cleanup, not proof) — never at a
        // foreign or Ghostty point.
        let up_point = match (|| -> Result<(f64, f64), String> {
            let (row_up, revision_up) = live.fresh_row()?;
            if row_up.spaces != vec![original, temp] {
                return Err(
                    "up revalidation: fresh row is not the exact [original, temp] shape".to_owned(),
                );
            }
            let up_frame = live
                .indicator
                .segment_snapshot()
                .into_iter()
                .find(|s| s.position == 2)
                .map(|s| s.frame)
                .ok_or_else(|| "up revalidation: position 2 has no live frame".to_owned())?;
            let displays = ffi::ns_display_snapshot()?;
            if displays.len() != 1 {
                return Err(
                    "up revalidation: display geometry is not single-display fresh".to_owned(),
                );
            }
            let d = &displays[0];
            let recomputed =
                segment_click_point_cg(up_frame, (d.frame.x, d.frame.y, d.frame.w, d.frame.h))?;
            let ax_owner = ffi::ax_hit_test_owner(recomputed.0, recomputed.1).ok();
            let cg_hit = ffi::sls_window_at_point(recomputed.0, recomputed.1).ok();
            let hit_bundle = ax_owner.and_then(|(pid, _)| ffi::bundle_id_for_pid(pid as i32));
            decide_up_point(
                down_frame,
                down_revision,
                up_frame,
                revision_up,
                recomputed,
                &SegmentHitProof {
                    our_pid,
                    ax_owner,
                    cg_hit_wid: cg_hit,
                    our_window_wids: our_window_wids(our_pid),
                    hit_bundle,
                },
            )
        })() {
            Ok(point) => point,
            Err(reason) => {
                let _ = post_tagged_edge(click_point, false);
                let _ = live.release.note_up();
                live.last_point = None;
                panic_guard.disarm();
                live.note(
                    "cleanup release-click-owed: owed up posted after refused up identity (cleanup, not proof)"
                        .to_owned(),
                );
                return Err(live.cleanup_fail("click-up-identity", reason));
            }
        };
        // Fresh-identity up posts at the RECOMPUTED point; the owed
        // release is unconditional from here (any post failure still
        // clears the books via the unified cleanup).
        let up_latency = post_tagged_edge(up_point, false).map_err(|reason| {
            let _ = live.release.note_up();
            live.last_point = None;
            panic_guard.disarm();
            live.cleanup_fail("click-up", reason)
        })?;
        live.release
            .note_up()
            .map_err(|reason| live.cleanup_fail("click-up", reason))?;
        live.last_point = None;
        panic_guard.disarm();
        if ffi::warp_cursor_position(live.pointer_before.0, live.pointer_before.1).is_err() {
            return Err(live.cleanup_fail(
                "click-pointer-restore",
                "eventless pointer restore refused".to_owned(),
            ));
        }
        let pointer_after = crate::synth::cursor_position().ok_or_else(|| {
            live.cleanup_fail(
                "click-pointer-restore",
                "pointer restore readback unreadable".to_owned(),
            )
        })?;
        if pointer_after != live.pointer_before {
            return Err(live.cleanup_fail(
                "click-pointer-restore",
                "pointer preimage not restored".to_owned(),
            ));
        }
        // Drain the production channel only AFTER the release: the AppKit
        // target forwards the immutable render-time binding verbatim, and
        // nothing AppKit-selects while the button is held.
        pump(CLICK_DELIVERY_MS);
        let delivered = {
            let deadline = std::time::Instant::now() + Duration::from_millis(2_000);
            loop {
                if let Some(click) = live.indicator.try_recv_click() {
                    break Some(click);
                }
                if std::time::Instant::now() >= deadline {
                    break None;
                }
                pump(50);
            }
        };
        let delivered = delivered.ok_or_else(|| {
            live.cleanup_fail(
                "segment-click",
                "no click binding delivered after a real synthetic pair".to_owned(),
            )
        })?;
        if delivered != click_binding {
            return Err(live.cleanup_fail(
                "segment-click",
                "delivered binding is not the fresh render-time binding".to_owned(),
            ));
        }
        verify_focus_held(
            focus_before_click,
            crate::ffi::frontmost_app().map(|(pid, _)| pid as u32),
        )
        .map_err(|reason| live.cleanup_fail("segment-click", reason))?;
        live.leg(
            ProofLeg::SegmentClick,
            true,
            format!(
                "real pair delivered position=2 down-latency-us={down_latency} up-latency-us={up_latency} focus-held=true pointer-restored=true"
            ),
        );

        // Leg: verified arrival through the product guarded engine lane:
        // the delivered immutable binding submits with its binding (no
        // hand-crafted claim bypass), the engine admits to the pending
        // claim, the main thread claims and drains with immediately
        // pre-dispatch gates, and the current readback verifies arrival.
        // A stale replay of the same binding refuses (generation moved
        // on) — asserted without dispatch.
        leg_gates().map_err(|e| live.cleanup_fail("arrival-gates", e))?;
        {
            let (fresh, fresh_revision) = live
                .fresh_row()
                .map_err(|e| live.cleanup_fail("arrival-row", e))?;
            let current_generation = live
                .indicator
                .segment_targets()
                .first()
                .map(|(_, _, generation, _, _)| *generation)
                .unwrap_or(delivered.ui_generation);
            let effect = live
                .drain_click_via_engine(
                    delivered,
                    current_generation,
                    &fresh.spaces,
                    fresh_revision,
                )
                .map_err(|e| live.cleanup_fail("verified-arrival", e))?;
            if !effect.changed {
                return Err(live.cleanup_fail(
                    "verified-arrival",
                    "arrival dispatched but reported no change".to_owned(),
                ));
            }
            let (after, _) = live
                .fresh_row()
                .map_err(|e| live.cleanup_fail("arrival-readback", e))?;
            if after.current != temp {
                return Err(live.cleanup_fail(
                    "verified-arrival",
                    "arrival unverified: current is not temp".to_owned(),
                ));
            }
            // Stale-replay negative control: re-publish (the arrival
            // advanced the revision), await the matched report, re-render
            // (teardown plus tick retires the old generation), then the
            // same binding must refuse without dispatch.
            let post_published = live
                .publish_topology()
                .map_err(|e| live.cleanup_fail("arrival-topology", e))?;
            let report = live
                .await_occupied(post_published, temp, "temp-after-arrival")
                .map_err(|e| live.cleanup_fail("arrival-occupancy", e))?;
            let (post_generation, post_revision) = live
                .render(Some(report))
                .map_err(|e| live.cleanup_fail("arrival-rerender", e))?;
            let fresh_now = live
                .fresh_row()
                .map(|(row, _)| row)
                .map_err(|e| live.cleanup_fail("arrival-row", e))?;
            if post_generation != delivered.ui_generation
                && resolve_proof_click(delivered, post_generation, &fresh_now.spaces, post_revision)
                    .is_ok()
            {
                return Err(live.cleanup_fail(
                    "verified-arrival",
                    "stale binding re-resolved after a rebuild; refusal broken".to_owned(),
                ));
            }
        }
        live.leg(
            ProofLeg::VerifiedArrival,
            true,
            "engine-lane click claim drained, current=temp verified, stale-replay controlled"
                .to_owned(),
        );

        // Leg: teardown absence. Record our status-window WIDs, destroy
        // explicitly through the production guard, then prove every
        // recorded WID left the live CG list (never inferred from exit).
        leg_gates().map_err(|e| live.cleanup_fail("teardown-gates", e))?;
        let teardown_before = our_window_wids(our_pid);
        live.indicator.teardown();
        pump(LEG_SETTLE_MS);
        let teardown_after = our_window_wids(our_pid);
        match verify_teardown_absent(&teardown_before, &teardown_after) {
            Ok(()) => live.leg(
                ProofLeg::TeardownAbsent,
                true,
                format!(
                    "status windows before={} after-listed=0",
                    teardown_before.len()
                ),
            ),
            Err(reason) if teardown_before.is_empty() => live.leg(
                ProofLeg::TeardownAbsent,
                true,
                format!("no status windows to remove ({reason})"),
            ),
            Err(reason) => return Err(live.cleanup_fail("teardown-absent", reason)),
        }

        // Exact restoration runs through the SAME unified cleanup as every
        // failure exit (goto original, helper re-home, temp removal,
        // verifications, teardown, runtime/helper/caffeinate stops).
        // Success additionally requires the lifetime bound held.
        if t0.elapsed() > budget_total + Duration::from_secs(60) {
            live.note("proof overran its bounded lifetime; restoration still verified".to_owned());
        }
        let restored = live.run_cleanup();
        live.leg(
            ProofLeg::CleanupRestore,
            restored,
            "unified cleanup: goto-original, helper re-home, temp removal, verifications, stops"
                .to_owned(),
        );
        let reaped = live.caffeinate_reaped;
        let complete = restored && reaped && live.legs.iter().all(|leg| leg.complete);
        let report = proof_report(&live.legs, restored, reaped, &live.notes);
        if complete {
            Ok(report)
        } else {
            Err((1, report))
        }
    }

    // Silence dead-code for helpers shared across cfg shapes.
    #[allow(dead_code)]
    fn _shape_check(pre: &super::ProofPreconditions) -> Result<(), String> {
        use super::check_preconditions;
        check_preconditions(pre)?;
        let _ = leg_token(ProofLeg::Baseline);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_indicator::{IndicatorMode, resolve_click};

    fn open_preconditions() -> ProofPreconditions {
        ProofPreconditions {
            session_unlocked: true,
            ax_trusted: true,
            post_access: true,
            display_count: 1,
            space_count: 1,
            secure_input_off: true,
            lock_screen_off: true,
            ghostty_baselined: true,
            focus_captured: true,
            pointer_captured: true,
        }
    }

    #[test]
    fn proof_args_need_bounded_seconds_single_display_and_opt_in() {
        assert!(validate_proof_args(120, 0, true).is_ok());
        assert!(validate_proof_args(60, 0, true).is_ok());
        // Floor: the leg battery needs its work window.
        assert!(validate_proof_args(59, 0, true).is_err());
        assert!(validate_proof_args(0, 0, true).is_err());
        assert!(validate_proof_args(121, 0, true).is_err());
        // Single-display product rule: any other display refuses.
        assert!(validate_proof_args(120, 1, true).is_err());
        // Explicit opt-in per invocation, never a default mutation.
        assert!(validate_proof_args(120, 0, false).is_err());
        assert_eq!(IndicatorMode::default(), IndicatorMode::All);
    }

    #[test]
    fn preconditions_refuse_every_unsafe_shape() {
        assert!(check_preconditions(&open_preconditions()).is_ok());
        // Locks and trust shortfalls refuse first.
        for mutate in [
            |p: &mut ProofPreconditions| p.session_unlocked = false,
            |p: &mut ProofPreconditions| p.ax_trusted = false,
            |p: &mut ProofPreconditions| p.post_access = false,
            |p: &mut ProofPreconditions| p.secure_input_off = false,
            |p: &mut ProofPreconditions| p.lock_screen_off = false,
        ] {
            let mut pre = open_preconditions();
            mutate(&mut pre);
            assert!(check_preconditions(&pre).is_err());
        }
        // Topology: zero/multi display and non-one-Space baselines refuse
        // (other layouts need an explicit safe shape, never a guess).
        for (displays, spaces) in [(0, 1), (2, 1), (1, 0), (1, 2), (1, 9)] {
            let pre = ProofPreconditions {
                display_count: displays,
                space_count: spaces,
                ..open_preconditions()
            };
            assert!(check_preconditions(&pre).is_err(), "{displays}/{spaces}");
        }
        // Uncaptured preimages refuse (no restoration proof without them).
        for mutate in [
            |p: &mut ProofPreconditions| p.ghostty_baselined = false,
            |p: &mut ProofPreconditions| p.focus_captured = false,
            |p: &mut ProofPreconditions| p.pointer_captured = false,
        ] {
            let mut pre = open_preconditions();
            mutate(&mut pre);
            assert!(check_preconditions(&pre).is_err());
        }
    }

    fn hit_proof() -> SegmentHitProof {
        SegmentHitProof {
            our_pid: 4242,
            ax_owner: Some((4242, 7)),
            cg_hit_wid: Some(7),
            our_window_wids: vec![7, 9],
            hit_bundle: Some("io.github.omnitiler.macos.probe".to_owned()),
        }
    }

    #[test]
    fn hit_identity_binds_exact_owner_and_window() {
        // Our PID plus our status window: the only passing shape.
        assert!(verify_segment_hit_identity(&hit_proof()).is_ok());
        // Unknown ownership refuses (never a guessed bind).
        let mut proof = hit_proof();
        proof.hit_bundle = None;
        assert!(verify_segment_hit_identity(&proof).is_err());
        proof = hit_proof();
        proof.hit_bundle = Some(String::new());
        assert!(verify_segment_hit_identity(&proof).is_err());
        // Ghostty at the point refuses explicitly (never our target).
        for bundle in [
            "com.mitchellh.ghostty",
            "COM.MITCHELLH.GHOSTTY",
            "com.apple.Terminal",
        ] {
            let proof = SegmentHitProof {
                hit_bundle: Some(bundle.to_owned()),
                ..hit_proof()
            };
            assert!(verify_segment_hit_identity(&proof).is_err(), "{bundle}");
        }
        // System-hosted menu bar (foreign AX owner): refuse, never assume
        // our PID.
        let proof = SegmentHitProof {
            ax_owner: Some((1, 7)),
            ..hit_proof()
        };
        assert!(verify_segment_hit_identity(&proof).is_err());
        // WindowServer hit outside our status windows refuses (impostor or
        // drift under a system host).
        let proof = SegmentHitProof {
            cg_hit_wid: Some(4242),
            ..hit_proof()
        };
        assert!(verify_segment_hit_identity(&proof).is_err());
        // Unreadable legs refuse (never guessed).
        for proof in [
            SegmentHitProof {
                ax_owner: None,
                ..hit_proof()
            },
            SegmentHitProof {
                cg_hit_wid: None,
                ..hit_proof()
            },
        ] {
            assert!(verify_segment_hit_identity(&proof).is_err());
        }
        // Half-changed legs refuse (one leg drifted, the other stale).
        let proof = SegmentHitProof {
            ax_owner: Some((4242, 9)),
            cg_hit_wid: Some(7),
            ..hit_proof()
        };
        assert!(verify_segment_hit_identity(&proof).is_err());
    }

    #[test]
    fn frame_freshness_and_point_planning_refuse_drift() {
        let frame = (100.0, 4.0, 26.0, 22.0);
        assert!(verify_frame_fresh(frame, frame, 7, 7).is_ok());
        // Geometry or revision drift refuses.
        assert!(verify_frame_fresh(frame, (101.0, 4.0, 26.0, 22.0), 7, 7).is_err());
        assert!(verify_frame_fresh(frame, frame, 7, 8).is_err());
        // Centre plus AppKit-to-CG Y flip over the display frame.
        let display = (0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(
            segment_click_point_cg(frame, display),
            Ok((113.0, 1080.0 - 15.0))
        );
        // Degenerate frames refuse before any live touch.
        assert!(segment_click_point_cg((f64::NAN, 4.0, 26.0, 22.0), display).is_err());
        assert!(segment_click_point_cg((100.0, 4.0, 0.0, 22.0), display).is_err());
        assert!(segment_click_point_cg((-1.0, 4.0, 26.0, 22.0), display).is_err());
    }

    #[test]
    fn post_down_release_is_balanced() {
        // Idle up refuses (no release without a down); double down
        // refuses (no second down while a release is owed).
        let mut guard = PostDownRelease::Idle;
        assert!(!guard.requires_release());
        assert!(guard.note_up().is_err());
        assert!(guard.note_down().is_ok());
        assert!(guard.requires_release());
        assert!(guard.note_down().is_err());
        // The post-down exit owes exactly one unconditional release.
        assert!(guard.requires_release());
        assert!(guard.note_up().is_ok());
        assert!(!guard.requires_release());
    }

    #[test]
    fn up_edge_needs_fresh_identity_not_down_reuse() {
        // The up edge never reuses the down proof blindly: the same pure
        // gate the live leg runs refuses frame drift, revision drift, and
        // a changed hit identity — the caller then releases at the DOWN
        // point as labeled cleanup (balanced contract above), never at a
        // foreign/Ghostty point and never as proof.
        let frame = (100.0, 4.0, 26.0, 22.0);
        let point = (113.0, 1065.0);
        // Fresh up reads identical to down: passes, returning the
        // recomputed up point.
        assert_eq!(
            decide_up_point(frame, 7, frame, 7, point, &hit_proof()),
            Ok(point)
        );
        // Frame drift between down and up refuses.
        assert!(
            decide_up_point(frame, 7, (101.0, 4.0, 26.0, 22.0), 7, point, &hit_proof()).is_err()
        );
        // Revision drift between down and up refuses.
        assert!(decide_up_point(frame, 7, frame, 8, point, &hit_proof()).is_err());
        // Identity change after down refuses (foreign AX owner: the
        // system-hosted menu-bar shape — never an assumed PID).
        let foreign = SegmentHitProof {
            ax_owner: Some((1, 7)),
            ..hit_proof()
        };
        assert!(decide_up_point(frame, 7, frame, 7, point, &foreign).is_err());
        // WindowServer hit leaving our status windows refuses too.
        let drifted = SegmentHitProof {
            cg_hit_wid: Some(4242),
            ..hit_proof()
        };
        assert!(decide_up_point(frame, 7, frame, 7, point, &drifted).is_err());
        // Every refusal above still owes exactly one balanced release at
        // the down point (the live leg posts it as cleanup): the contract
        // pins the release, never a fallback point.
        let mut guard = PostDownRelease::Idle;
        assert!(guard.note_down().is_ok());
        assert!(guard.requires_release());
        assert!(guard.note_up().is_ok());
        assert!(!guard.requires_release());
    }

    #[test]
    fn focus_hold_and_teardown_need_exact_readbacks() {
        assert!(verify_focus_held(Some(9), Some(9)).is_ok());
        assert!(verify_focus_held(Some(9), Some(10)).is_err());
        assert!(verify_focus_held(None, Some(9)).is_err());
        assert!(verify_focus_held(Some(9), None).is_err());
        // Removal is observed (every baseline WID gone), never inferred.
        assert!(verify_teardown_absent(&[7], &[9]).is_ok());
        assert!(verify_teardown_absent(&[7], &[7, 9]).is_err());
        assert!(verify_teardown_absent(&[], &[]).is_err());
    }

    #[test]
    fn binding_matrix_matches_the_product_lane() {
        // Fresh binding resolves through the exact product mapping.
        let click = IndicatorClick {
            kind: crate::workspace_indicator::IndicatorClickKind::Numbered,
            position: 2,
            expected_sid: 3,
            ui_generation: 7,
            topology_revision: 9,
        };
        assert!(resolve_proof_click(click, 7, &[20, 3], 9).is_ok());
        // Stale generation, stale revision, moved row, vanished position,
        // and past-nine positions all refuse before any backend touch —
        // the same refusals the live click leg gates on.
        assert!(resolve_click(click, 8, &[20, 3], 9).is_err());
        assert!(resolve_click(click, 7, &[20, 3], 10).is_err());
        assert!(resolve_click(click, 7, &[20, 41], 9).is_err());
        assert!(resolve_click(click, 7, &[20], 9).is_err());
        let past_nine = IndicatorClick {
            position: 10,
            ..click
        };
        assert!(resolve_click(past_nine, 7, &[20, 3], 9).is_err());
    }

    #[test]
    fn occupancy_assertion_needs_proven_occupied() {
        use SpaceOccupancy::{Empty, Occupied, Unknown};
        // Proven off-current occupancy passes.
        assert!(require_occupied_off_current(2, &[(1, Empty), (2, Occupied)]).is_ok());
        // Empty is a hard refusal (homing lost); Unknown is unproven
        // (never weakened into empty); a missing temp refuses.
        assert!(require_occupied_off_current(2, &[(1, Occupied), (2, Empty)]).is_err());
        assert!(require_occupied_off_current(2, &[(1, Occupied), (2, Unknown)]).is_err());
        assert!(require_occupied_off_current(2, &[(1, Occupied)]).is_err());
        assert!(require_occupied_off_current(0, &[(1, Occupied)]).is_err());
    }

    #[test]
    fn leg_tokens_and_report_shape_are_stable() {
        assert_eq!(leg_token(ProofLeg::Baseline), "baseline");
        assert_eq!(leg_token(ProofLeg::CleanupRestore), "cleanup-restore");
        let legs = vec![
            ProofLegRecord::new(ProofLeg::Baseline, true, "detail-a".to_owned()),
            ProofLegRecord::new(ProofLeg::SegmentClick, false, "detail-b".to_owned()),
        ];
        let report = proof_report(&legs, false, true, &["note-a".to_owned()]);
        let parsed: serde_json::Value = serde_json::from_str(&report).expect("report is JSON");
        assert_eq!(parsed["action"], "workspace-indicator-proof");
        assert_eq!(parsed["complete"], false);
        assert_eq!(parsed["restored"], false);
        assert_eq!(parsed["legs"][0]["leg"], "baseline");
        assert_eq!(parsed["notes"][0], "note-a");
        // All-complete plus restored plus reaped completes.
        let legs = vec![ProofLegRecord::new(
            ProofLeg::Baseline,
            true,
            "ok".to_owned(),
        )];
        let parsed: serde_json::Value =
            serde_json::from_str(&proof_report(&legs, true, true, &[])).expect("report is JSON");
        assert_eq!(parsed["complete"], true);
    }

    #[test]
    fn mc_failure_is_a_named_refusal_not_a_replay() {
        // The cleanup contract: a dispatched-but-unverified remove is
        // never replayed — the drain reports residue instead. The shape
        // below pins the refusal token the runner records.
        let err = crate::mc_probe::McError::Blocked("remove shortfall".to_owned());
        assert!(err.to_string().contains("remove shortfall"));
    }

    fn complete_report(revision: u64) -> crate::runtime::WorkspaceOccupancyReport {
        crate::runtime::WorkspaceOccupancyReport {
            context_present: true,
            context_revision: revision,
            domains: vec![
                crate::runtime::WorkspaceOccupancyDomain {
                    workspace: "space-11".to_owned(),
                    current: true,
                    members: vec![(100, 1)],
                },
                crate::runtime::WorkspaceOccupancyDomain {
                    workspace: "space-12".to_owned(),
                    current: false,
                    members: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn retained_report_predicates_pin_completion_and_homing() {
        // Complete frame: present context homed under exactly the
        // published revision; occupied only on a non-empty exactly-homed
        // membership under the exact `space-{sid}` key.
        let report = complete_report(7);
        assert!(report_complete_for(&report, 7));
        assert!(domain_occupied(&report, 11));
        assert!(!domain_occupied(&report, 12));
        assert!(!domain_occupied(&report, 13));
        // Any drift uncompletes: missing context, wrong revision, or a
        // renamed key never reads occupied.
        let mut drifted = complete_report(7);
        drifted.context_revision = 8;
        assert!(!report_complete_for(&drifted, 7));
        let mut absent = complete_report(7);
        absent.context_present = false;
        assert!(!report_complete_for(&absent, 7));
        let mut renamed = complete_report(7);
        renamed.domains[0].workspace = "space-99".to_owned();
        assert!(!domain_occupied(&renamed, 11));
        let mut emptied = complete_report(7);
        emptied.domains[0].members.clear();
        assert!(!domain_occupied(&emptied, 11));
    }

    #[test]
    fn bundle_layout_gate_rejects_plain_executables() {
        // Signed bundle layout: the probe binary inside
        // `OmniTilerProbe.app/Contents/MacOS` with the sibling Helper.app
        // beside it passes; plain cargo layouts fail closed (no
        // LaunchServices identity to scope, no live launch).
        assert!(bundle_layout_ok(std::path::Path::new(
            "/Applications/OmniTiler/OmniTilerProbe.app/Contents/MacOS/tiler-macos"
        )));
        assert!(!bundle_layout_ok(std::path::Path::new(
            "/Users/dev/OmniTiler/target/debug/tiler-macos"
        )));
        assert!(!bundle_layout_ok(std::path::Path::new(
            "/Applications/OmniTiler/OmniTilerProbe.app/Contents/MacOS"
        )));
        assert!(!bundle_layout_ok(std::path::Path::new("tiler-macos")));
    }

    #[test]
    fn expected_titles_assert_question_iff_unknown() {
        // The live appear/occupancy legs compare actual AppKit readbacks
        // against exactly this: '?' if and only if Unknown.
        use SpaceOccupancy::{Empty, Occupied, Unknown};
        assert_eq!(
            expected_titles(&[(1, Occupied), (2, Empty)]),
            vec!["1".to_owned(), "2".to_owned()]
        );
        assert_eq!(
            expected_titles(&[(1, Occupied), (2, Unknown)]),
            vec!["1".to_owned(), "2?".to_owned()]
        );
        assert_eq!(
            expected_titles(&[(1, Unknown), (2, Occupied)]),
            vec!["1?".to_owned(), "2".to_owned()]
        );
    }

    #[test]
    fn cleanup_focus_verdict_reads_never_writes() {
        // Readable non-protected frontmost passes (the owned helper
        // legitimately self-activates on its public joins); Ghostty, a
        // protected terminal, and unreadable legs refuse.
        assert!(cleanup_focus_verdict(Some("io.github.omnitiler.macos.probe.helper")).is_ok());
        assert!(cleanup_focus_verdict(Some("com.apple.TextEdit")).is_ok());
        assert!(cleanup_focus_verdict(Some("com.mitchellh.ghostty")).is_err());
        assert!(cleanup_focus_verdict(Some("com.apple.Terminal")).is_err());
        assert!(cleanup_focus_verdict(None).is_err());
        assert!(cleanup_focus_verdict(Some("")).is_err());
    }

    fn step_tokens(plan: &[CleanupStep]) -> Vec<&'static str> {
        plan.iter().map(|step| cleanup_step_token(*step)).collect()
    }

    #[test]
    fn cleanup_step_tokens_are_stable() {
        assert_eq!(cleanup_step_token(CleanupStep::CheckGates), "check-gates");
        assert_eq!(
            cleanup_step_token(CleanupStep::RemoveTempIfEmpty),
            "remove-temp-if-empty"
        );
        assert_eq!(
            cleanup_step_token(CleanupStep::StopHelperExact),
            "stop-helper-exact"
        );
        assert_eq!(
            cleanup_step_token(CleanupStep::StopCaffeinate),
            "stop-caffeinate"
        );
    }

    #[test]
    fn cleanup_gates_cover_exactly_the_mutation_steps() {
        // Goto, helper re-home, and temp removal actuate Space/helper
        // state: gates required. Release, reads, own-UI teardown, and
        // own-process stops run even when gates fail (the owed release in
        // particular must never strand on a gate refusal).
        for step in [
            CleanupStep::GotoOriginal,
            CleanupStep::JoinHelperOriginal,
            CleanupStep::RemoveTempIfEmpty,
        ] {
            assert!(step_needs_gates(step), "{step:?}");
        }
        for step in [
            CleanupStep::CheckGates,
            CleanupStep::ReleaseClickOwed,
            CleanupStep::VerifyLayout,
            CleanupStep::VerifyGhostty,
            CleanupStep::VerifyPointer,
            CleanupStep::TeardownIndicator,
            CleanupStep::ShutdownRuntime,
            CleanupStep::StopHelperExact,
            CleanupStep::StopCaffeinate,
        ] {
            assert!(!step_needs_gates(step), "{step:?}");
        }
    }

    #[test]
    fn cleanup_plan_is_empty_when_nothing_exists() {
        // Pre-spawn failure: no helper, no temp, no release, no runtime —
        // only reads, idempotent teardown, and the companion stop ride.
        let plan = cleanup_plan(&CleanupState::default());
        assert_eq!(
            step_tokens(&plan),
            vec![
                "check-gates",
                "verify-layout",
                "verify-ghostty",
                "verify-pointer",
                "teardown-indicator",
                "stop-caffeinate",
            ]
        );
    }

    #[test]
    fn cleanup_plan_after_spawn_restores_before_stopping() {
        // Helper spawned, temp never adopted: goto plus helper re-home
        // ride BEFORE the exact stop (restoration needs the helper alive),
        // but no temp removal exists to run.
        let plan = cleanup_plan(&CleanupState {
            helper_spawned: true,
            ..CleanupState::default()
        });
        assert_eq!(
            step_tokens(&plan),
            vec![
                "check-gates",
                "goto-original",
                "join-helper-original",
                "verify-layout",
                "verify-ghostty",
                "verify-pointer",
                "teardown-indicator",
                "stop-helper-exact",
                "stop-caffeinate",
            ]
        );
        // Restoration precedes the stop: nothing kills the helper before
        // the steps that need it alive.
        let stop = step_tokens(&plan)
            .iter()
            .position(|token| *token == "stop-helper-exact")
            .expect("stop rides");
        for token in ["goto-original", "join-helper-original"] {
            let at = step_tokens(&plan)
                .iter()
                .position(|t| *t == token)
                .expect("restore rides");
            assert!(at < stop, "{token} must precede the helper stop");
        }
        assert!(!step_tokens(&plan).contains(&"remove-temp-if-empty"));
        assert!(!step_tokens(&plan).contains(&"shutdown-runtime"));
    }

    #[test]
    fn cleanup_plan_after_create_removes_only_the_temp() {
        // Helper plus adopted temp plus runtime: the full order — temp
        // removal is the ONLY destructive Space step (never broadened),
        // runtime stops before the helper stop, caffeinate reaps last.
        let plan = cleanup_plan(&CleanupState {
            helper_spawned: true,
            temp_created: true,
            runtime_started: true,
            ..CleanupState::default()
        });
        assert_eq!(
            step_tokens(&plan),
            vec![
                "check-gates",
                "goto-original",
                "join-helper-original",
                "remove-temp-if-empty",
                "verify-layout",
                "verify-ghostty",
                "verify-pointer",
                "teardown-indicator",
                "shutdown-runtime",
                "stop-helper-exact",
                "stop-caffeinate",
            ]
        );
        let tokens = step_tokens(&plan);
        let remove = tokens
            .iter()
            .position(|t| *t == "remove-temp-if-empty")
            .expect("remove rides");
        let join = tokens
            .iter()
            .position(|t| *t == "join-helper-original")
            .expect("join rides");
        assert!(join < remove, "helper re-homes before temp removal");
        assert!(
            tokens
                .iter()
                .position(|t| *t == "stop-caffeinate")
                .expect("reap rides")
                == tokens.len() - 1,
            "caffeinate reaps last"
        );
    }

    #[test]
    fn cleanup_plan_after_down_owes_exactly_one_release() {
        // Click-down posted, up not yet: the owed release rides second
        // (before any verification that could observe a stuck button),
        // and the plan still carries the full restoration tail.
        let plan = cleanup_plan(&CleanupState {
            helper_spawned: true,
            temp_created: true,
            release_owed: true,
            runtime_started: true,
        });
        let tokens = step_tokens(&plan);
        assert_eq!(tokens[0], "check-gates");
        assert_eq!(tokens[1], "release-click-owed");
        assert_eq!(tokens.len(), 12, "full tail preserved: {tokens:?}");
        assert_eq!(tokens[11], "stop-caffeinate");
    }
}
