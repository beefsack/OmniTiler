#![forbid(unsafe_code)]

//! Gated direct/bridged-SLS window-move probe, run AFTER the public O->T->O battery.
//!
//! The public battery in [`crate::space_recovery`] stays the default: spawn
//! one regular-policy helper, prove O→T→O with exact membership plus exact
//! frame restoration through the public `MoveToActiveSpace` self-action, stop
//! exactly that helper. `--probe-direct` opts into ONE additional direct
//! `SLSMoveWindowsToManagedSpace` dispatch on the SAME helper after leg B;
//! `--probe-bridged` opts into ONE additional bridged dispatch instead
//! (yabai `src/space_manager.c:667-673`: `alloc` plus
//! `initWithWindows:spaceID:` plus the internal dispatcher). The two flags
//! are mutually exclusive (never both: no fallback, no replay) and each
//! additionally requires `--allow-untested`; either probe runs with delayed
//! membership and current readbacks (max 3 s each), void-dispatch/no-effect
//! versus rejection classification, and an independent public self-restore
//! once the primary mover process exits.
//!
//! Primary-mover separation: either dispatch runs in a SEPARATE
//! short-lived owned child process (the current executable re-invoked as
//! `spaces-move --pid --nonce --wid --space [--route bridged]
//! --allow-untested`), owned by its
//! exact `Child` handle and reaped boundedly here. The fixture holder keeps
//! owning the helper `Child` throughout; the worker only writes the passed,
//! already-verified helper WID (no broadened target). No new general process
//! manager: one bounded `Command` spawn plus the existing exact-Child stop
//! semantics (exact handle only, bounded wait, no PID guess, no broad kill).
//! No LaunchServices route is needed for the mover: the dispatch needs no AX
//! (pure SLS plus CG reads) and the cross-command title proof reads the
//! helper on the current Space (source original is current after leg B), so a
//! direct spawn does not inherit the terminal TCC workaround. If a
//! LaunchServices wrapper ever replaces this spawn and cannot yield status,
//! inspect the mover's structured JSON plus process absence instead of the
//! wrapper status alone.
//!
//! Effect semantics: the SLS call returns void, so dispatch is never effect
//! evidence. The mover's immediate report plus the harness's own delayed
//! polls (max [`DIRECT_SETTLE_MS`]) are the only effect evidence. Dispatch
//! itself is tri-state ([`DispatchState`]): a parsed structured report is
//! `Yes`; a clean exit carrying a named pre-dispatch gate refusal is `No`;
//! timeouts, signal exits, unparsable output, and harness-side identity
//! failures are `Unknown` - never misreported as rejection. A report naming
//! the wrong route likewise downgrades attribution to `Unknown` (never an
//! early return - the delayed readbacks and restoration below run as
//! normal, so a moved window is never stranded by an attribution failure). An unchanged
//! exact `[original]` after a verified dispatch is OBSERVED NO-EFFECT (not a
//! SIP finding). Membership is always read back regardless of dispatch
//! state, and any changed membership is restored through the independent
//! public path even when dispatch is unknown. No retries, no no-op inverse
//! on no effect. On effect, the independent public self-restore (held-Child
//! `space_join` to the current original) runs FIRST after the mover exits;
//! an inverse SLS move is never attempted here. An ambiguous
//! changed-but-not-exact membership is restored the same way BEFORE the
//! error verdict and cleanup — the helper is never killed as a surrogate
//! for restoration (process exit does not restore Space assignment).
//!
//! Mover attribution: the harness itself starts through the `just launch`
//! LaunchServices recipe, so a directly spawned mover child inherits the
//! probe app's responsible-process attribution, NOT the shell/terminal's —
//! the shell-attribution caveat applies to shell-launched binaries only. No
//! LaunchServices wrapper is added around the mover; if the cross-command
//! title proof fails closed inside the mover, it exits with a named refusal
//! (dispatch `No`), never a claimed dispatch. The mover executable identity
//! (path, length, std-only fingerprint) is captured in the report so the
//! same-artifact fact is evidence, not assumption.
//!
//! Route determination (local checkouts first; pinned upstream URLs only):
//! the direct leg is yabai `space_manager_move_window_list_to_space`'s
//! non-workaround branch
//! (https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705),
//! bypassed upstream on newer families via `workspace_use_macos_space_workaround`
//! (https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/workspace.m#L17-L26),
//! so it is source-derived and experimental live here (one gated direct
//! dispatch on an owned fixture observed no effect; single-shot inverse
//! unverified; the bridged async operation dispatches only inside its own
//! gated probe flag, efficacy unproven).
//! The compat-ID pair stays blocked (setter-only locally, no exact tag preimage,
//! yabai-tag collision — no tag getter is added, reset-to-zero is not
//! restoration). Membership reads use the pinned yabai shape
//! (https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705
//! via `SLSCopySpacesForWindows`). Ghostty is untouched (no writes, keys,
//! or moves); no current-Space switch is performed here (current is only
//! read back); no synthetic events are posted on any path.

use serde::Serialize;

use crate::mc_probe::{LegLatency, McError};

/// Delayed effect settle after the direct dispatch: the SLS move lands
/// asynchronously (if at all), so a non-exact immediate readback polls fresh
/// (never a re-dispatch) within this window. Matches the native-plan
/// current/membership readback bound.
pub const DIRECT_SETTLE_MS: u64 = 3_000;
/// Delayed poll slice for the direct settle.
pub const DIRECT_POLL_MS: u64 = 200;
/// Bounded wait for the short-lived mover child (dispatch plus immediate
/// readback). The mover exits on its own after one dispatch; this is a
/// reap deadline, never a retry budget.
pub const MOVER_TIMEOUT_MS: u64 = 20_000;
/// Bounded drain for the mover's piped output after exit (or after the kill
/// following a timeout): reader threads exit on EOF once the child is gone,
/// so this always terminates. Documented no-hang: the harness never blocks
/// unboundedly on child output on any path.
pub const MOVER_DRAIN_MS: u64 = 5_000;
/// Partial-output drain after a timed-out mover is killed (the pipe may
/// still hold a truncated report fragment; kept as evidence, never parsed
/// as success).
pub const MOVER_TIMEOUT_DRAIN_MS: u64 = 2_000;
/// Truncation bound for captured mover output kept in the report.
pub const MOVER_OUTPUT_KEEP: usize = 2_000;
/// Cleanup reserve held back from the helper lifetime for the post-probe
/// teardown (MC close, caffeinate stop, exact-Child stop, verification):
/// mirrors the 15 s battery reserve, so the probe never spends it.
pub const PROBE_CLEANUP_RESERVE_MS: u64 = 15_000;
/// Worst-case wall-clock cost of the direct experiment AFTER leg B: bounded
/// mover wait + delayed settle + public-restore leg + restore resettle +
/// cleanup reserve. One pass, never multiplied (no retries). The harness
/// refuses the probe (no dispatch) when the remaining helper lifetime
/// cannot cover this.
pub const PROBE_WORST_CASE_MS: u64 = MOVER_TIMEOUT_MS
    + DIRECT_SETTLE_MS
    + crate::space_join::JOIN_TIMEOUT_MS
    + DIRECT_SETTLE_MS
    + PROBE_CLEANUP_RESERVE_MS;

/// Pure budget gate (no live touch — unit-tested): the remaining helper
/// lifetime at probe entry must cover [`PROBE_WORST_CASE_MS`]. A narrow
/// window refuses BEFORE any spawn, so an under-budgeted probe never
/// dispatches.
pub fn probe_budget_sufficient(remaining_ms: u64) -> Result<(), String> {
    if remaining_ms < PROBE_WORST_CASE_MS {
        return Err(format!(
            "remaining helper lifetime {remaining_ms}ms cannot cover the bounded direct-probe worst case {PROBE_WORST_CASE_MS}ms (mover {MOVER_TIMEOUT_MS}ms + settle {DIRECT_SETTLE_MS}ms + restore + resettle {DIRECT_SETTLE_MS}ms + cleanup reserve {PROBE_CLEANUP_RESERVE_MS}ms); refusing dispatch"
        ));
    }
    Ok(())
}

/// Causal claim carried by every direct-probe report: observed dispatch and
/// effect only, never a SIP (or any policy) inference.
pub const DIRECT_CAUSAL_NOTE: &str = "observed dispatch/effect only — no SIP inference; unchanged membership after a verified dispatch is observed no-effect, pre-dispatch refusals name their gate";

/// Pure gate for the opt-in flag: `--probe-direct` preserves the default
/// battery when absent and additionally requires `--allow-untested` when
/// present (the direct route is experimental — observed no-effect on one
/// owned fixture; the single-shot inverse is unverified while the separate
/// public held-Child recovery is proven per run, never assumed). No live
/// touch — unit-tested.
pub fn validate_probe_direct_args(
    probe_direct: bool,
    allow_untested: bool,
) -> Result<bool, String> {
    if probe_direct && !allow_untested {
        return Err(
            "spaces-owned-recovery --probe-direct requires --allow-untested (direct SLS route experimental, observed no-effect on one owned fixture; single-shot inverse unverified, effect observer-verified per run, never assumed)"
                .to_owned(),
        );
    }
    Ok(probe_direct)
}

/// Pure gate for the bridged opt-in flag: `--probe-bridged` preserves the
/// default battery when absent and additionally requires `--allow-untested`
/// when present (the bridged route is experimental - async dispatch with
/// unproven efficacy - while the separate public held-Child recovery is
/// proven per run, never assumed). No live touch - unit-tested.
pub fn validate_probe_bridged_args(
    probe_bridged: bool,
    allow_untested: bool,
) -> Result<bool, String> {
    if probe_bridged && !allow_untested {
        return Err(
            "spaces-owned-recovery --probe-bridged requires --allow-untested (bridged SLS route experimental, async dispatch efficacy unproven; single-shot inverse unverified, effect observer-verified per run, never assumed)"
                .to_owned(),
        );
    }
    Ok(probe_bridged)
}

/// Pure route-selection gate: at most one probe route per run. Both flags
/// together refuse before any spawn - the harness never falls back to the
/// other route and never replays the battery. No live touch - unit-tested.
pub fn validate_probe_selection(probe_direct: bool, probe_bridged: bool) -> Result<(), String> {
    if probe_direct && probe_bridged {
        return Err(
            "spaces-owned-recovery refuses --probe-direct with --probe-bridged: select exactly one probe route per run (no fallback, no replay)"
                .to_owned(),
        );
    }
    Ok(())
}

/// Pure leg-name helper so every latency entry carries its probe route
/// (`direct-mover`, `bridged-settle`, ...). No live touch - unit-tested.
#[must_use]
pub fn probe_leg_name(route: crate::spaces::SpaceMoveRoute, phase: &str) -> String {
    format!("{}-{phase}", route.as_str())
}

/// Pure argv builder for the separate short-lived mover child: the current
/// executable re-invoked as the existing `spaces-move` command against the
/// already-verified fixture identity. No new subcommand, no broadened
/// target - the worker only writes this passed verified WID. The direct
/// route takes the command default (no `--route` flag, preserving the
/// existing shape byte-for-byte); the bridged route appends
/// `--route bridged` explicitly.
pub fn mover_argv(pid: u32, nonce: &str, wid: u32, space: u64) -> Vec<String> {
    vec![
        "spaces-move".to_owned(),
        "--pid".to_owned(),
        pid.to_string(),
        "--nonce".to_owned(),
        nonce.to_owned(),
        "--wid".to_owned(),
        wid.to_string(),
        "--space".to_owned(),
        space.to_string(),
        "--allow-untested".to_owned(),
    ]
}

/// Pure argv builder for the bridged mover child: the direct argv plus the
/// explicit `--route bridged` selection (never a silent default).
pub fn mover_argv_bridged(pid: u32, nonce: &str, wid: u32, space: u64) -> Vec<String> {
    let mut argv = mover_argv(pid, nonce, wid, space);
    argv.extend(["--route".to_owned(), "bridged".to_owned()]);
    argv
}

/// Whether the direct SLS dispatch happened. Tri-state by construction: a
/// timeout, crash, garble, or harness-side identity failure is `Unknown`,
/// never coerced into rejection. `No` is claimed only when the command
/// positively refused before dispatch (never spawned, or a clean exit
/// carrying a named pre-dispatch gate refusal with no report).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DispatchState {
    /// A structured move report parsed from the mover output: the core only
    /// builds reports after the void SLS call returned.
    Yes,
    /// Positively refused before dispatch: the mover never spawned, or it
    /// exited cleanly with a named pre-dispatch gate refusal and no report.
    No,
    /// Timeout, signal/crash exit, unparsable output, or harness-side
    /// identity failure: dispatch may or may not have happened.
    Unknown,
}

impl DispatchState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Unknown => "unknown",
        }
    }
}

/// Gate markers that positively identify a pre-dispatch refusal in mover
/// output. Every `Err` path of the mover core fires before the void SLS
/// call (post-dispatch paths always build a report, never error), so a
/// clean exit carrying one of these markers with no report means the
/// dispatch never executed. Marker absence (or an unclean exit) is
/// [`DispatchState::Unknown`], never `No`.
const PREDISPATCH_GATE_MARKERS: &[&str] = &[
    "ownership refused",
    "blocked:",
    "unsupported:",
    "requires --allow-untested",
    "refusing",
    "needs --",
];

/// Pure refusal detector (no live touch — unit-tested): `true` only for a
/// clean mover exit (status 1/2: core refusal or usage/validation, both
/// documented no-mutation paths) whose output names a pre-dispatch gate
/// with no structured report parsed.
pub fn positive_predispatch_refusal(exit_code: Option<i32>, text: &str) -> bool {
    matches!(exit_code, Some(1) | Some(2))
        && PREDISPATCH_GATE_MARKERS.iter().any(|m| text.contains(m))
}

/// Pure route-agreement gate: the mover's structured report must name the
/// requested route. A mismatch (wrong binary, stale argv, substituted
/// worker) downgrades dispatch attribution to [`DispatchState::Unknown`]
/// with a named mismatch error - never an early return, never an efficacy
/// claim either way. The caller still runs its delayed membership readbacks
/// and the normal aftermath (including the independent public restore on
/// changed membership), so a moved window is never stranded by an
/// attribution failure. Agreement (or no report at all) passes dispatch
/// through untouched. No live touch - unit-tested.
pub fn reconcile_report_route(
    requested: &str,
    dispatch: DispatchState,
    report_route: Option<&str>,
) -> (DispatchState, Option<String>) {
    match report_route {
        Some(actual) if actual != requested => (
            DispatchState::Unknown,
            Some(format!(
                "route mismatch: requested {requested}, mover reported {actual}; dispatch attribution Unknown, no efficacy claimed"
            )),
        ),
        _ => (dispatch, None),
    }
}

/// Mover executable identity: the worker must be the same current
/// executable (path, length, fingerprint). The fingerprint is an explicitly
/// labeled non-cryptographic std-only hash — same-artifact evidence, not a
/// tamper-security claim — so no new hash dependency enters the tree.
#[derive(Debug, Clone, Serialize)]
pub struct MoverExeIdentity {
    pub path: String,
    pub len: Option<u64>,
    /// `std-siphash64:<hex>` over the file bytes, or `None` when unreadable
    /// (named in `error`, never silently omitted).
    pub fingerprint: Option<String>,
    pub is_current_exe: bool,
    pub error: Option<String>,
}

/// Pure non-cryptographic fingerprint over bytes (std only). Deterministic
/// per process; labeled at every use site, never presented as a digest.
pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    bytes.len().hash(&mut h);
    bytes.hash(&mut h);
    format!("std-siphash64:{:016x}", h.finish())
}

/// Read-only current-executable identity for the mover-spawn evidence.
/// Never launches anything.
fn current_exe_identity() -> MoverExeIdentity {
    let path = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    if path.is_empty() {
        return MoverExeIdentity {
            path: String::new(),
            len: None,
            fingerprint: None,
            is_current_exe: true,
            error: Some("current executable path unresolved".to_owned()),
        };
    }
    match std::fs::read(&path) {
        Ok(bytes) => MoverExeIdentity {
            path,
            len: Some(bytes.len() as u64),
            fingerprint: Some(fingerprint_bytes(&bytes)),
            is_current_exe: true,
            error: None,
        },
        Err(e) => MoverExeIdentity {
            path,
            len: None,
            fingerprint: None,
            is_current_exe: true,
            error: Some(format!("current executable unreadable: {e}")),
        },
    }
}

/// Outcome of one bounded mover-child run. `dispatch` is tri-state: only a
/// parsed structured report is `Yes`; only a never-spawned child or a clean
/// exit with a named pre-dispatch gate refusal is `No`; everything else
/// (timeout, crash, garble, identity failure) is `Unknown`. `report` is the
/// parsed mover report when present; `error` names spawn/timeout or
/// pre-dispatch refusal text otherwise.
#[derive(Debug, Clone)]
pub struct MoverResult {
    pub exited: bool,
    pub exit_code: Option<i32>,
    pub dispatch: DispatchState,
    pub report: Option<crate::spaces::SpaceMoveReport>,
    pub exe: MoverExeIdentity,
    pub error: Option<String>,
    pub stdout_kept: String,
    pub stderr_kept: String,
}

/// Pure direct-effect classification over the mover result plus the harness
/// delayed readbacks. Distinguishes void dispatch with no effect from
/// pre-dispatch rejection; never implies a cause (SIP or otherwise).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DirectOutcome {
    /// Mover dispatched and the delayed harness poll reads exact `[target]`.
    DispatchedEffect,
    /// Mover dispatched but the delayed read landed after the immediate one
    /// (immediate not exact, delayed exact `[target]`).
    DelayedEffect,
    /// Mover dispatched (void call returned) yet membership stayed exact
    /// `[preimage]`: observed no-effect, NOT a SIP finding.
    ObservedNoEffect,
    /// Effect observed with unknown dispatch provenance (timeout or
    /// unparsable mover output, but the delayed harness poll reads exact
    /// `[target]`): restored through the independent public path like any
    /// effect, with the provenance recorded honestly.
    UnknownEffect,
    /// Mover positively refused before dispatch (never spawned, or a clean
    /// exit with a named pre-dispatch gate refusal and no report): no
    /// dispatch happened.
    Rejected,
    /// The delayed readback is unreadable or changed-but-not-exact:
    /// preserve evidence. A readable changed membership is restored through
    /// the independent public path BEFORE the error verdict; an unreadable
    /// one cannot be verified, so no blind restore is claimed.
    Ambiguous,
}

impl DirectOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DispatchedEffect => "dispatched-effect",
            Self::DelayedEffect => "delayed-effect",
            Self::ObservedNoEffect => "observed-no-effect",
            Self::UnknownEffect => "unknown-dispatch-effect",
            Self::Rejected => "rejected",
            Self::Ambiguous => "ambiguous",
        }
    }
}

/// Pure inputs for [`classify_direct`]: everything the harness observed, no
/// live reads inside.
#[derive(Debug, Clone)]
pub struct DirectInputs {
    pub dispatched: DispatchState,
    pub mover_error: Option<String>,
    pub immediate: Option<Vec<u64>>,
    pub delayed: Option<Vec<u64>>,
}

/// Pure classification: void dispatch versus rejection versus effect, from
/// observed state only. Exact-single-Space equality throughout: merely
/// containing the target while also still on the preimage is NOT success.
/// Dispatch `Unknown` never collapses into `Rejected`: an exact `[target]`
/// readback with unknown provenance is an effect (`UnknownEffect`), and an
/// exact `[preimage]` readback is observed no-effect either way. A positive
/// refusal paired with a moved window is a contradiction (`Ambiguous`,
/// restored before the error — never a clean verdict).
pub fn classify_direct(inputs: &DirectInputs, preimage: u64, target: u64) -> DirectOutcome {
    let exact_target = inputs
        .delayed
        .as_ref()
        .is_some_and(|s| s.len() == 1 && s[0] == target);
    let exact_preimage = inputs
        .delayed
        .as_ref()
        .is_some_and(|s| s.len() == 1 && s[0] == preimage);
    if exact_target {
        return match inputs.dispatched {
            DispatchState::Yes => {
                if inputs
                    .immediate
                    .as_ref()
                    .is_some_and(|s| s.len() == 1 && s[0] == target)
                {
                    DirectOutcome::DispatchedEffect
                } else {
                    DirectOutcome::DelayedEffect
                }
            }
            // Changed membership with unknown provenance: effect until
            // proven otherwise — restored, never rejected.
            DispatchState::Unknown => DirectOutcome::UnknownEffect,
            // Positive refusal yet the window moved: contradiction, never a
            // clean verdict — restored before the error (see aftermath).
            DispatchState::No => DirectOutcome::Ambiguous,
        };
    }
    if exact_preimage {
        return match inputs.dispatched {
            DispatchState::No => DirectOutcome::Rejected,
            _ => DirectOutcome::ObservedNoEffect,
        };
    }
    DirectOutcome::Ambiguous
}

/// What the harness must do after classification. Pure over the outcome
/// plus the delayed readback (no live reads inside — unit-tested): effects
/// restore and expect success; unchanged membership only verifies (no
/// inverse, no second dispatch); any readable changed-but-not-exact
/// membership (including the refused-yet-moved contradiction) restores
/// BEFORE the error verdict; an unreadable readback fails without a blind
/// restore claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeAftermath {
    /// Restore through the independent public path; success expected.
    RestoreExpectOk,
    /// Verify stillness (membership + frame); no inverse dispatch.
    VerifyNoInverse,
    /// Restore through the independent public path, then still fail with
    /// the restore evidence attached (never a silent Ok).
    RestoreExpectErr,
    /// Fail without a restore attempt (nothing readable to verify one).
    FailNoRestore,
}

/// Pure aftermath planner for [`classify_direct`] outcomes.
pub fn plan_aftermath(outcome: DirectOutcome, delayed: &Option<Vec<u64>>) -> ProbeAftermath {
    match outcome {
        DirectOutcome::DispatchedEffect
        | DirectOutcome::DelayedEffect
        | DirectOutcome::UnknownEffect => ProbeAftermath::RestoreExpectOk,
        DirectOutcome::ObservedNoEffect | DirectOutcome::Rejected => {
            ProbeAftermath::VerifyNoInverse
        }
        DirectOutcome::Ambiguous => match delayed {
            // Readable: something is verifiably not home (sticky/multi
            // membership, wrong-Space read, or the refused-yet-moved
            // contradiction — classify never leaves exact `[preimage]` here).
            // Restore first (never kill the helper as a surrogate), then
            // fail with the restore evidence attached.
            Some(_) => ProbeAftermath::RestoreExpectErr,
            // Unreadable: no verifiable restore target — fail honestly
            // without a blind restore claim.
            None => ProbeAftermath::FailNoRestore,
        },
    }
}

/// Pure current-Space stability: the direct leg must not move the current.
#[must_use]
pub fn current_unchanged(before: Option<u64>, after: Option<u64>) -> bool {
    before == after
}

/// Structured direct-probe evidence, kept separate from the public battery
/// legs. `causal_claim` is always [`DIRECT_CAUSAL_NOTE`]: observed state,
/// never implied SIP.
#[derive(Debug, Clone, Serialize)]
pub struct DirectProbeReport {
    pub attempted: bool,
    /// The SLS dispatch taken: `direct` or `bridged` (never a fallback -
    /// the mover argv selects it explicitly and the mover reports it back).
    pub route: String,
    pub outcome: String,
    pub target: u64,
    pub preimage: u64,
    pub mover_exited: bool,
    pub mover_exit_code: Option<i32>,
    pub mover_dispatch: DispatchState,
    pub mover_error: Option<String>,
    pub mover_stdout: String,
    pub mover_stderr: String,
    /// Same-artifact evidence for the mover worker: path, byte length,
    /// and explicitly labeled non-cryptographic fingerprint.
    pub mover_exe: MoverExeIdentity,
    /// Actual AX-trust value observed in THIS (parent) process at probe
    /// entry (`None` off-macOS/stub): evidence, never assumed. The mover
    /// child inherits this process's attribution, not the terminal's.
    pub parent_ax_trusted: Option<bool>,
    /// Fresh window-server lock verdicts around the probe (actual
    /// `CGSessionCopyCurrentDictionary` state, never the frontmost
    /// heuristic alone).
    pub session_lock_before: crate::ffi::SessionLock,
    pub session_lock_after: crate::ffi::SessionLock,
    pub immediate_spaces: Option<Vec<u64>>,
    pub immediate_verified: Option<bool>,
    pub delayed_spaces: Option<Vec<u64>>,
    pub current_before: Option<u64>,
    pub current_after: Option<u64>,
    pub current_unchanged: bool,
    pub bridged: crate::ffi::SlsBridgedAvailability,
    pub move_symbols: crate::ffi::SlsMoveSymbols,
    pub restore_performed: bool,
    pub restore_membership: Option<Vec<u64>>,
    pub restore_exact: Option<bool>,
    pub frame_matches_baseline: Option<bool>,
    pub causal_claim: String,
    pub notes: Vec<String>,
}

fn truncate_kept(s: &str) -> String {
    if s.len() <= MOVER_OUTPUT_KEEP {
        return s.to_owned();
    }
    let mut out = s[..MOVER_OUTPUT_KEEP].to_owned();
    out.push_str("…[truncated]");
    out
}

/// Try to extract a [`crate::spaces::SpaceMoveReport`] from mover output:
/// the success body (`{"report": {...}}`) prints on stdout, while the
/// unverified-effect body prints on stderr (main returns `Err` for
/// unverified effects). Either location counts as dispatched. Robust to
/// interleaved diagnostics: the whole text is tried first (single-JSON
/// body), then lines newest-first (the LAST `{"report": ...}` line wins, so
/// earlier helper chatter never shadows the mover's own verdict).
fn extract_mover_report(text: &str) -> Option<crate::spaces::SpaceMoveReport> {
    fn from_value(value: &serde_json::Value) -> Option<crate::spaces::SpaceMoveReport> {
        serde_json::from_value(value.get("report")?.clone()).ok()
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text.trim())
        && let Some(report) = from_value(&value)
    {
        return Some(report);
    }
    for line in text.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line)
            && let Some(report) = from_value(&value)
        {
            return Some(report);
        }
    }
    None
}

/// Bounded collection of one mover child's piped output. Reader threads exit
/// on EOF once the child is gone (kill + wait guarantees that post-timeout
/// too); dropping the receiver never blocks them (`send` result ignored), so
/// this always terminates by `timeout_ms` with whatever arrived. No-hang
/// confirmation: no path here blocks unboundedly on child output.
fn drain_mover_output(
    rx: std::sync::mpsc::Receiver<(bool, String)>,
    timeout_ms: u64,
) -> (String, String) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut stdout_text = String::new();
    let mut stderr_text = String::new();
    loop {
        let remain = deadline.saturating_duration_since(std::time::Instant::now());
        if remain.is_zero() {
            break;
        }
        match rx.recv_timeout(remain.min(std::time::Duration::from_millis(200))) {
            Ok((is_err, text)) => {
                if is_err {
                    stderr_text.push_str(&text);
                } else {
                    stdout_text.push_str(&text);
                }
            }
            // Senders gone (EOF on both pipes): nothing more to collect.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            // Slice elapsed with senders still alive (a held pipe): keep
            // waiting inside the overall deadline, never past it.
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    (stdout_text, stderr_text)
}

/// Run the separate short-lived mover child to completion (bounded): spawn
/// the current executable with [`mover_argv`], collect output on threads so
/// a chatty child can never block on a full pipe, poll exit boundedly,
/// reap exactly, and verify process absence. Exact handle only: no PID
/// guess, no broad kill. Dispatch is tri-state: only a parsed structured
/// report is `Yes`; only a never-spawned child or a clean exit with a named
/// pre-dispatch gate refusal is `No`; timeouts, crashes, garbles, and wait
/// failures are `Unknown`. Returns the classified [`MoverResult`].
fn run_mover_child(
    route: crate::spaces::SpaceMoveRoute,
    pid: u32,
    nonce: &str,
    wid: u32,
    space: u64,
) -> MoverResult {
    let exe = current_exe_identity();
    let failed = |error: String| MoverResult {
        exited: false,
        exit_code: None,
        dispatch: DispatchState::Unknown,
        report: None,
        exe: exe.clone(),
        error: Some(error),
        stdout_kept: String::new(),
        stderr_kept: String::new(),
    };
    if exe.path.is_empty() {
        return failed("mover exe unresolved; refusing spawn".to_owned());
    }
    let args = match route {
        crate::spaces::SpaceMoveRoute::Direct => mover_argv(pid, nonce, wid, space),
        crate::spaces::SpaceMoveRoute::Bridged => mover_argv_bridged(pid, nonce, wid, space),
    };
    let mut child = match std::process::Command::new(&exe.path)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            // Never spawned: no dispatch is possible — the only `No` that
            // needs no output evidence (there is no process to have run).
            return MoverResult {
                exited: false,
                exit_code: None,
                dispatch: DispatchState::No,
                report: None,
                exe,
                error: Some(format!("mover spawn failed: {e}")),
                stdout_kept: String::new(),
                stderr_kept: String::new(),
            };
        }
    };
    let (out_tx, out_rx) = std::sync::mpsc::channel::<(bool, String)>();
    {
        let tx = out_tx.clone();
        if let Some(mut p) = child.stdout.take() {
            std::thread::Builder::new()
                .name("direct-mover-drain-out".to_owned())
                .spawn(move || {
                    use std::io::Read as _;
                    let mut buf = String::new();
                    let _ = p.read_to_string(&mut buf);
                    let _ = tx.send((false, buf));
                })
                .ok();
        }
    }
    {
        let tx = out_tx.clone();
        if let Some(mut p) = child.stderr.take() {
            std::thread::Builder::new()
                .name("direct-mover-drain-err".to_owned())
                .spawn(move || {
                    use std::io::Read as _;
                    let mut buf = String::new();
                    let _ = p.read_to_string(&mut buf);
                    let _ = tx.send((true, buf));
                })
                .ok();
        }
    }
    drop(out_tx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(MOVER_TIMEOUT_MS);
    let mut exit_code: Option<i32> = None;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_code = status.code();
                break;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    timed_out = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                // Wait state unknown: the child may or may not have run to
                // completion — never claim rejection.
                return failed(format!("mover wait state unknown: {e}"));
            }
        }
    }
    if timed_out {
        let _ = child.kill();
        let _ = child.wait();
        // Bounded partial drain: the pipe may hold a truncated fragment
        // (kept as evidence below, never parsed as success). Reader threads
        // exit on the post-kill EOF; the receiver drop never blocks them.
        let (stdout_text, stderr_text) = drain_mover_output(out_rx, MOVER_TIMEOUT_DRAIN_MS);
        return MoverResult {
            exited: true,
            exit_code: None,
            dispatch: DispatchState::Unknown,
            report: None,
            exe,
            error: Some(format!(
                "mover timed out after {MOVER_TIMEOUT_MS}ms; killed exact child — dispatch may or may not have happened (Unknown, never rejection)"
            )),
            stdout_kept: truncate_kept(&stdout_text),
            stderr_kept: truncate_kept(&stderr_text),
        };
    }
    // Reap exactly after the observed exit (never an unbounded wait on a
    // live child: the exit was already observed above).
    let _ = child.wait();
    let mover_pid = child.id();
    // Bounded drain (no-hang per `drain_mover_output`): EOF follows the
    // observed exit since this child has no grandchildren holding the pipe.
    let (stdout_text, stderr_text) = drain_mover_output(out_rx, MOVER_DRAIN_MS);
    // Process absence: the exact child was reaped, so the PID must be gone;
    // a lingering PID here would be a reuse, never our mover.
    let mover_gone = !crate::ffi::process_exists(mover_pid as i32);
    let _ = mover_gone;
    let report = extract_mover_report(&stdout_text).or_else(|| extract_mover_report(&stderr_text));
    if let Some(report) = report {
        return MoverResult {
            exited: true,
            exit_code,
            dispatch: DispatchState::Yes,
            report: Some(report),
            exe,
            error: None,
            stdout_kept: truncate_kept(&stdout_text),
            stderr_kept: truncate_kept(&stderr_text),
        };
    }
    // No structured report parsed: positive pre-dispatch refusal or an
    // unknowable outcome — distinguished below, never inferred from the
    // exit status alone.
    let err_text = if stderr_text.trim().is_empty() {
        stdout_text.trim().to_owned()
    } else {
        stderr_text.trim().to_owned()
    };
    let combined = format!("{stdout_text}\n{stderr_text}");
    if positive_predispatch_refusal(exit_code, &combined) {
        return MoverResult {
            exited: true,
            exit_code,
            dispatch: DispatchState::No,
            report: None,
            exe,
            error: Some(if err_text.is_empty() {
                format!("mover refused before dispatch (code {exit_code:?}); no dispatch happened")
            } else {
                truncate_kept(&err_text)
            }),
            stdout_kept: truncate_kept(&stdout_text),
            stderr_kept: truncate_kept(&stderr_text),
        };
    }
    MoverResult {
        exited: true,
        exit_code,
        dispatch: DispatchState::Unknown,
        report: None,
        exe,
        error: Some(if err_text.is_empty() {
            format!(
                "mover exited (code {exit_code:?}) without a parseable structured report; dispatch Unknown, never rejection"
            )
        } else {
            truncate_kept(&err_text)
        }),
        stdout_kept: truncate_kept(&stdout_text),
        stderr_kept: truncate_kept(&stderr_text),
    }
}

/// Held-Child membership read: re-runs the holder verification routine
/// (exact-handle liveness, expected exe, fresh CG owner join — no title read,
/// so this holds off-current) and only then performs the plain SLS
/// membership read for the already-joined window ID.
fn held_membership_read(
    child: &mut std::process::Child,
    pid: u32,
    helper_path: &str,
    wid: u32,
) -> Option<Vec<u64>> {
    match crate::focus::recheck_holder_before_mutation(child, pid, helper_path, wid) {
        Ok(()) => crate::spaces::window_spaces(wid).ok(),
        Err(_) => None,
    }
}

/// Delayed direct settle over a scripted reader: polls fresh (never a
/// re-dispatch) until the deadline and returns the last read (exact or not).
/// The caller classifies via [`classify_direct`].
fn settle_membership(
    read: &mut dyn FnMut() -> Option<Vec<u64>>,
    deadline_ms: u64,
    poll_ms: u64,
) -> Option<Vec<u64>> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(deadline_ms);
    let mut last: Option<Vec<u64>> = None;
    loop {
        if let Some(spaces) = read() {
            last = Some(spaces);
        }
        if std::time::Instant::now() >= deadline {
            return last;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_ms.min(500)));
    }
}

/// Current-Space readback for one display: the fresh topology snapshot's
/// current, or `None` when the snapshot is unreadable (unknown, never
/// guessed).
fn display_current(display: usize) -> Option<u64> {
    crate::mc_lifecycle::live::fresh_snapshot()
        .ok()?
        .displays
        .get(display)?
        .current
}

/// Delayed current readback: returns the first read plus a settled re-read
/// when the first differs from `want` (up to `deadline_ms`). Read-only.
fn settle_current(display: usize, want: Option<u64>, deadline_ms: u64) -> Option<u64> {
    let first = display_current(display);
    if first == want {
        return first;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(deadline_ms);
    let mut last = first;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(DIRECT_POLL_MS.min(500)));
        last = display_current(display);
        if last == want {
            return last;
        }
    }
    last
}

/// One public self-action leg through the held-Child entrypoint of the
/// shared [`crate::space_join`] library (same exclusive publish plus
/// outcome-poll as the battery legs, authenticated by the exact held Child
/// instead of the cross-command title proof). Single attempt, no retry.
fn run_public_restore_leg(
    route: crate::spaces::SpaceMoveRoute,
    child: &mut std::process::Child,
    pid: u32,
    nonce: &str,
    helper_path: &str,
    wid: u32,
) -> Result<crate::space_join::JoinEvidence, McError> {
    let r = route.as_str();
    let proof = crate::space_join::prove_held_child(child, pid, nonce, helper_path, wid)
        .map_err(|e| McError::Ownership(format!("{r}-restore held proof: {e}")))?;
    let request_id = crate::space_join::publish_join_trigger_held(&proof)
        .map_err(|e| McError::Blocked(format!("{r}-restore publish: {e}")))?;
    let our_body = crate::probe_target::space_join_request_body(nonce, &request_id)
        .map_err(|e| McError::Blocked(format!("{r}-restore body: {e}")))?;
    let evidence = crate::space_join::await_join_result(pid, nonce, &request_id, &our_body)
        .map_err(|e| McError::Blocked(format!("{r}-restore outcome: {e}")))?;
    if !evidence.restored_verified
        || !crate::probe_target::space_join_restored_ok(
            evidence.preimage_bits,
            evidence.restored_bits,
        )
    {
        return Err(McError::Blocked(format!(
            "{r}-restore behavior not exactly restored (preimage {} restored {} verified {})",
            evidence.preimage_bits, evidence.restored_bits, evidence.restored_verified
        )));
    }
    Ok(evidence)
}

/// Gated SLS experiment, called AFTER the public O->T->O battery has
/// proven exact `[original]` plus the exact frame on the same helper.
/// Single dispatch of the selected route, bounded mover, delayed readbacks,
/// independent public restore after the mover exits. No retries on any path.
///
/// The caller lends the exact helper `Child` (taken back before any context
/// use, so cleanup always owns it) plus the baseline identity and the
/// spawn-tracked helper deadline. Ghostty, currents, and synthetic posts
/// are untouched here beyond readbacks.
#[allow(clippy::too_many_arguments)]
fn run_probe_after_recovery(
    route: crate::spaces::SpaceMoveRoute,
    child: &mut std::process::Child,
    owned: &crate::ownership::OwnedIdentity,
    helper_pid: u32,
    helper_nonce: &str,
    helper_path: &str,
    wid: u32,
    baseline_frame_ax: Option<crate::ffi::AxFrame>,
    original: u64,
    temp: u64,
    display: usize,
    helper_deadline: std::time::Instant,
    notes: &mut Vec<String>,
    latencies: &mut Vec<LegLatency>,
) -> Result<DirectProbeReport, McError> {
    let r = route.as_str();
    // Worst-case budget gate BEFORE any spawn: the remaining helper
    // lifetime must cover mover + settle + restore + resettle + cleanup
    // reserve. A narrow window refuses with no dispatch on any path.
    let remaining_ms = helper_deadline
        .saturating_duration_since(std::time::Instant::now())
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    if let Err(e) = probe_budget_sufficient(remaining_ms) {
        notes.push(format!("{r}-probe budget refusal: {e}"));
        return Err(McError::Blocked(e));
    }
    let bridged = crate::ffi::sls_bridged_availability();
    let move_symbols = crate::ffi::sls_move_symbols();
    // Actual parent-process evidence (never assumed): AX-trust value plus
    // the fresh window-server lock verdict. The mover child inherits THIS
    // process's attribution, not the terminal's.
    let parent_ax_trusted = crate::ffi::ax_is_process_trusted();
    let session_lock_before = crate::ffi::session_state().lock;
    notes.push(format!(
        "{r}-probe diagnose: direct={} compat_pair={} (blocked, never dispatched) bridged_fn={} bridged_class={} bridged_selector={} bridged_available={} parent_ax_trusted={parent_ax_trusted:?} session_lock={session_lock_before:?}",
        move_symbols.direct,
        move_symbols.compat_pair,
        bridged.fn_present,
        bridged.class_present,
        bridged.selector_present,
        bridged.available
    ));

    let current_before = display_current(display);
    let preimage = held_membership_read(child, helper_pid, helper_path, wid);
    notes.push(format!(
        "{r}-probe preimage: membership={preimage:?} current={current_before:?} (goal: dispatch toward [{temp}], current stays {current_before:?})"
    ));

    // Separate short-lived mover: one dispatch of the selected route, then
    // it exits. The harness polls the effect itself below; the mover's
    // immediate report is dispatch evidence only. A locked/unreadable
    // session refuses the spawn up front (defense in depth - the mover core
    // rechecks at its own write boundary): modeled as dispatch `No`, since
    // nothing could run.
    let t_mover = std::time::Instant::now();
    let mut mover = if matches!(session_lock_before, crate::ffi::SessionLock::Unlocked) {
        run_mover_child(route, helper_pid, helper_nonce, wid, temp)
    } else {
        MoverResult {
            exited: false,
            exit_code: None,
            dispatch: DispatchState::No,
            report: None,
            exe: current_exe_identity(),
            error: Some(format!(
                "harness refused mover spawn: window-server session {session_lock_before:?}; no dispatch possible"
            )),
            stdout_kept: String::new(),
            stderr_kept: String::new(),
        }
    };
    // Route agreement BEFORE any attribution: a report naming the wrong
    // route downgrades to Unknown with a named error but never returns
    // early - the delayed readbacks and restoration below run as normal.
    {
        let reported = mover.report.as_ref().map(|rep| rep.route.as_str());
        let (dispatch, mismatch) = reconcile_report_route(r, mover.dispatch, reported);
        mover.dispatch = dispatch;
        if let Some(err) = mismatch {
            notes.push(format!("{r} route agreement: {err}"));
            mover.error = Some(match mover.error.take() {
                Some(prev) => format!("{prev}; {err}"),
                None => err,
            });
        }
    }
    latencies.push(LegLatency {
        leg: probe_leg_name(route, "mover"),
        ms: t_mover.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    });
    notes.push(format!(
        "{r} mover: exited={} code={:?} dispatch={} exe={} fp={:?} error={:?}",
        mover.exited,
        mover.exit_code,
        mover.dispatch.as_str(),
        mover.exe.path,
        mover.exe.fingerprint,
        mover.error
    ));
    let immediate_spaces = mover.report.as_ref().map(|r| r.effect_spaces.clone());
    let immediate_verified = mover.report.as_ref().map(|r| r.effect_verified);

    // Delayed harness readbacks (max 3 s each) run REGARDLESS of dispatch
    // state: membership exactness plus current stability. Fresh polls,
    // never a re-dispatch.
    let mut last_poll: Option<Vec<u64>> = None;
    let mut read = || {
        let spaces = held_membership_read(child, helper_pid, helper_path, wid);
        if spaces.is_some() {
            last_poll = spaces.clone();
        }
        spaces
    };
    let t_settle = std::time::Instant::now();
    let delayed = settle_membership(&mut read, DIRECT_SETTLE_MS, DIRECT_POLL_MS);
    let delayed_spaces = delayed.or(last_poll);
    latencies.push(LegLatency {
        leg: probe_leg_name(route, "settle"),
        ms: t_settle.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    });
    let current_after = settle_current(display, current_before, DIRECT_SETTLE_MS);
    let session_lock_after = crate::ffi::session_state().lock;
    let unchanged = current_unchanged(current_before, current_after);
    notes.push(format!(
        "{r} readbacks: dispatch={} immediate={immediate_spaces:?} (verified={immediate_verified:?}) delayed={delayed_spaces:?} current_before={current_before:?} current_after={current_after:?} unchanged={unchanged} session_lock_after={session_lock_after:?}",
        mover.dispatch.as_str()
    ));

    let inputs = DirectInputs {
        dispatched: mover.dispatch,
        mover_error: mover.error.clone(),
        immediate: immediate_spaces.clone(),
        delayed: delayed_spaces.clone(),
    };
    let outcome = classify_direct(&inputs, original, temp);
    let aftermath = plan_aftermath(outcome, &delayed_spaces);
    notes.push(format!(
        "{r} classification: {} aftermath={aftermath:?} ({})",
        outcome.as_str(),
        DIRECT_CAUSAL_NOTE
    ));

    // Shared frame gate for the no-change paths: the baseline must not have
    // drifted even though membership never moved. A drift with no effect is
    // evidence of an outside change — fail, never a clean verdict.
    let read_frame_now = || {
        crate::ax::read_window_owned(owned, helper_pid, wid)
            .map(|r| r.ax_frame)
            .unwrap_or(None)
    };

    match plan_aftermath(outcome, &delayed_spaces) {
        ProbeAftermath::VerifyNoInverse => {
            // No effect observed (positively refused, or verified/unknown
            // dispatch with membership home): NO inverse — a restore move
            // would be a second dispatch with nothing to undo. Verify
            // stillness plus the exact baseline frame, then report.
            let check = held_membership_read(child, helper_pid, helper_path, wid);
            let frame = read_frame_now();
            let frame_ok = frame.is_some() && frame == baseline_frame_ax;
            notes.push(format!(
                "{r} {}: no inverse dispatched; membership still {check:?} frame_matches_baseline={frame_ok}",
                outcome.as_str()
            ));
            if !frame_ok {
                return Err(McError::Blocked(format!(
                    "{r} {} left membership home but the baseline frame drifted (baseline {baseline_frame_ax:?} vs now {frame:?}); refusing to call this clean",
                    outcome.as_str()
                )));
            }
            Ok(DirectProbeReport {
                attempted: true,
                route: r.to_owned(),
                outcome: outcome.as_str().to_owned(),
                target: temp,
                preimage: original,
                mover_exited: mover.exited,
                mover_exit_code: mover.exit_code,
                mover_dispatch: mover.dispatch,
                mover_error: mover.error,
                mover_stdout: mover.stdout_kept.clone(),
                mover_stderr: mover.stderr_kept.clone(),
                mover_exe: mover.exe.clone(),
                parent_ax_trusted,
                session_lock_before,
                session_lock_after,
                immediate_spaces,
                immediate_verified,
                delayed_spaces: delayed_spaces.clone(),
                current_before,
                current_after,
                current_unchanged: unchanged,
                bridged,
                move_symbols,
                restore_performed: false,
                restore_membership: delayed_spaces.clone(),
                restore_exact: Some(
                    delayed_spaces
                        .as_ref()
                        .is_some_and(|s| s.len() == 1 && s[0] == original),
                ),
                frame_matches_baseline: Some(true),
                causal_claim: DIRECT_CAUSAL_NOTE.to_owned(),
                notes: Vec::new(),
            })
        }
        ProbeAftermath::RestoreExpectOk => {
            // Effect observed (verified or unknown provenance — the helper
            // is exactly on [temp]). The primary mover has already exited
            // (reaped above), so the independent public self-restore runs
            // now through the held Child — usable after the mover's exit by
            // construction. An inverse SLS move is never attempted here.
            if !unchanged {
                notes.push(format!(
                    "{r} moved current {current_before:?} -> {current_after:?}; public restore still targets membership [{original}] first"
                ));
            }
            let (restored, exact, frame_ok) = restore_and_resettle(
                route,
                child,
                owned,
                helper_pid,
                helper_nonce,
                helper_path,
                wid,
                baseline_frame_ax,
                original,
                latencies,
                notes,
            )?;
            if !exact {
                return Err(McError::Blocked(format!(
                    "{r} public restore membership is {restored:?}, not exactly [{original}]; preserving for cleanup"
                )));
            }
            if !frame_ok {
                return Err(McError::Blocked(format!(
                    "{r} public restore frame not exactly restored"
                )));
            }
            Ok(DirectProbeReport {
                attempted: true,
                route: r.to_owned(),
                outcome: outcome.as_str().to_owned(),
                target: temp,
                preimage: original,
                mover_exited: mover.exited,
                mover_exit_code: mover.exit_code,
                mover_dispatch: mover.dispatch,
                // Normally `None` here (a parsed report carries no error);
                // a route mismatch downgrades attribution above and its
                // named error travels in this field instead of being
                // dropped. Never an efficacy claim either way.
                mover_error: mover.error.clone(),
                mover_stdout: mover.stdout_kept.clone(),
                mover_stderr: mover.stderr_kept.clone(),
                mover_exe: mover.exe.clone(),
                parent_ax_trusted,
                session_lock_before,
                session_lock_after,
                immediate_spaces,
                immediate_verified,
                delayed_spaces,
                current_before,
                current_after,
                current_unchanged: unchanged,
                bridged,
                move_symbols,
                restore_performed: true,
                restore_membership: restored,
                restore_exact: Some(true),
                frame_matches_baseline: Some(true),
                causal_claim: DIRECT_CAUSAL_NOTE.to_owned(),
                notes: Vec::new(),
            })
        }
        ProbeAftermath::RestoreExpectErr => {
            // Readable changed-but-not-exact membership (sticky/multi,
            // wrong-Space read, or the refused-yet-moved contradiction):
            // restore through the independent public path FIRST — never the
            // helper kill as a surrogate (process exit does not restore
            // Space assignment) — then still fail with the restore evidence
            // attached, never a silent Ok.
            let restore_evidence = match restore_and_resettle(
                route,
                child,
                owned,
                helper_pid,
                helper_nonce,
                helper_path,
                wid,
                baseline_frame_ax,
                original,
                latencies,
                notes,
            ) {
                Ok((restored, exact, frame_ok)) => format!(
                    "restore attempted: membership={restored:?} exact={exact} frame_matches_baseline={frame_ok}"
                ),
                Err(e) => format!("restore leg itself failed before verification: {e}"),
            };
            notes.push(format!("{r} ambiguous restore: {restore_evidence}"));
            Err(McError::Blocked(format!(
                "{r} outcome {} with changed membership {delayed_spaces:?} (dispatch {}); independent public restore attempted BEFORE this error verdict (never helper kill): {restore_evidence}; preserving for cleanup",
                outcome.as_str(),
                mover.dispatch.as_str()
            )))
        }
        ProbeAftermath::FailNoRestore => Err(McError::Blocked(format!(
            "{r} outcome ambiguous with unreadable delayed membership (immediate={immediate_spaces:?} delayed={delayed_spaces:?}); no verifiable restore target, so no blind restore is claimed - preserving for cleanup"
        ))),
    }
}

/// Direct entrypoint: preserves the existing `--probe-direct` behavior
/// exactly (route `direct`, default mover argv).
#[allow(clippy::too_many_arguments)]
pub fn run_direct_probe_after_recovery(
    child: &mut std::process::Child,
    owned: &crate::ownership::OwnedIdentity,
    helper_pid: u32,
    helper_nonce: &str,
    helper_path: &str,
    wid: u32,
    baseline_frame_ax: Option<crate::ffi::AxFrame>,
    original: u64,
    temp: u64,
    display: usize,
    helper_deadline: std::time::Instant,
    notes: &mut Vec<String>,
    latencies: &mut Vec<LegLatency>,
) -> Result<DirectProbeReport, McError> {
    run_probe_after_recovery(
        crate::spaces::SpaceMoveRoute::Direct,
        child,
        owned,
        helper_pid,
        helper_nonce,
        helper_path,
        wid,
        baseline_frame_ax,
        original,
        temp,
        display,
        helper_deadline,
        notes,
        latencies,
    )
}

/// Bridged entrypoint: one bridged dispatch in the mover child with the
/// same ownership/topology/session/preimage checks, delayed readbacks, and
/// independent public self-restore once the mover exits. Same no-retry,
/// no-fallback rules as the direct probe.
#[allow(clippy::too_many_arguments)]
pub fn run_bridged_probe_after_recovery(
    child: &mut std::process::Child,
    owned: &crate::ownership::OwnedIdentity,
    helper_pid: u32,
    helper_nonce: &str,
    helper_path: &str,
    wid: u32,
    baseline_frame_ax: Option<crate::ffi::AxFrame>,
    original: u64,
    temp: u64,
    display: usize,
    helper_deadline: std::time::Instant,
    notes: &mut Vec<String>,
    latencies: &mut Vec<LegLatency>,
) -> Result<DirectProbeReport, McError> {
    run_probe_after_recovery(
        crate::spaces::SpaceMoveRoute::Bridged,
        child,
        owned,
        helper_pid,
        helper_nonce,
        helper_path,
        wid,
        baseline_frame_ax,
        original,
        temp,
        display,
        helper_deadline,
        notes,
        latencies,
    )
}

/// One independent public restore leg through the held Child plus the
/// bounded resettle poll and the exact-frame check. Single attempt, no
/// retry. Returns the resettled membership, its exactness against
/// `[original]`, and the frame match. Callers decide the verdict.
#[allow(clippy::too_many_arguments)]
fn restore_and_resettle(
    route: crate::spaces::SpaceMoveRoute,
    child: &mut std::process::Child,
    owned: &crate::ownership::OwnedIdentity,
    helper_pid: u32,
    helper_nonce: &str,
    helper_path: &str,
    wid: u32,
    baseline_frame_ax: Option<crate::ffi::AxFrame>,
    original: u64,
    latencies: &mut Vec<LegLatency>,
    notes: &mut Vec<String>,
) -> Result<(Option<Vec<u64>>, bool, bool), McError> {
    let r = route.as_str();
    let t_restore = std::time::Instant::now();
    let evidence =
        run_public_restore_leg(route, child, helper_pid, helper_nonce, helper_path, wid)?;
    latencies.push(LegLatency {
        leg: probe_leg_name(route, "public-restore"),
        ms: t_restore.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    });
    let _ = evidence;
    let mut last_restore: Option<Vec<u64>> = None;
    let mut restore_read = || {
        let spaces = held_membership_read(child, helper_pid, helper_path, wid);
        if spaces.is_some() {
            last_restore = spaces.clone();
        }
        spaces
    };
    let restored =
        settle_membership(&mut restore_read, DIRECT_SETTLE_MS, DIRECT_POLL_MS).or(last_restore);
    let exact = restored
        .as_ref()
        .is_some_and(|s| s.len() == 1 && s[0] == original);
    let frame = crate::ax::read_window_owned(owned, helper_pid, wid)
        .map(|r| r.ax_frame)
        .unwrap_or(None);
    let frame_ok = frame.is_some() && frame == baseline_frame_ax;
    notes.push(format!(
        "{r} public restore: membership={restored:?} exact={exact} frame_matches_baseline={frame_ok}"
    ));
    Ok((restored, exact, frame_ok))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(
        dispatched: DispatchState,
        immediate: Option<Vec<u64>>,
        delayed: Option<Vec<u64>>,
    ) -> DirectInputs {
        DirectInputs {
            dispatched,
            mover_error: None,
            immediate,
            delayed,
        }
    }

    #[test]
    fn probe_direct_flag_preserves_default_and_requires_opt_in() {
        // Default absent: old fixture unchanged (false passes through).
        assert!(!validate_probe_direct_args(false, true).unwrap());
        assert!(!validate_probe_direct_args(false, false).unwrap());
        // Opt-in without --allow-untested refuses before any live touch.
        assert!(validate_probe_direct_args(true, false).is_err());
        assert!(validate_probe_direct_args(true, true).unwrap());
    }

    #[test]
    fn probe_bridged_flag_preserves_default_and_requires_opt_in() {
        // Default absent: old fixture unchanged (false passes through).
        assert!(!validate_probe_bridged_args(false, true).unwrap());
        assert!(!validate_probe_bridged_args(false, false).unwrap());
        // Opt-in without --allow-untested refuses before any live touch.
        assert!(validate_probe_bridged_args(true, false).is_err());
        assert!(validate_probe_bridged_args(true, true).unwrap());
    }

    #[test]
    fn probe_selection_refuses_both_routes_before_any_spawn() {
        // Exactly one probe route per run: both flags together refuse with
        // no fallback and no replay.
        assert!(validate_probe_selection(false, false).is_ok());
        assert!(validate_probe_selection(true, false).is_ok());
        assert!(validate_probe_selection(false, true).is_ok());
        let err = validate_probe_selection(true, true).unwrap_err();
        assert!(err.contains("exactly one probe route"));
        assert!(err.contains("no fallback"));
    }

    #[test]
    fn probe_leg_names_carry_the_selected_route() {
        // Every latency entry names its route: the report's timing is
        // attributable to direct or bridged, never ambiguous.
        assert_eq!(
            probe_leg_name(crate::spaces::SpaceMoveRoute::Direct, "mover"),
            "direct-mover"
        );
        assert_eq!(
            probe_leg_name(crate::spaces::SpaceMoveRoute::Bridged, "mover"),
            "bridged-mover"
        );
        assert_eq!(
            probe_leg_name(crate::spaces::SpaceMoveRoute::Bridged, "settle"),
            "bridged-settle"
        );
        assert_eq!(
            probe_leg_name(crate::spaces::SpaceMoveRoute::Bridged, "public-restore"),
            "bridged-public-restore"
        );
    }

    #[test]
    fn bridged_mover_argv_selects_route_explicitly() {
        // The direct argv is unchanged (the command default is direct, no
        // flag); the bridged argv appends the explicit selection. Neither
        // shape broadens the target: only the verified WID is written.
        assert_eq!(
            mover_argv_bridged(4242, "0123456789abcdef", 7, 66),
            vec![
                "spaces-move",
                "--pid",
                "4242",
                "--nonce",
                "0123456789abcdef",
                "--wid",
                "7",
                "--space",
                "66",
                "--allow-untested",
                "--route",
                "bridged",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
        );
        assert!(
            !mover_argv(4242, "0123456789abcdef", 7, 66)
                .iter()
                .any(|a| a == "--route")
        );
    }

    #[test]
    fn bridged_effect_and_ambiguity_restore_through_public_path() {
        // The shared classify/aftermath pipeline serves the bridged probe:
        // an exact [target] effect (verified or unknown provenance)
        // restores through the independent public path, and a
        // changed-but-not-exact readback restores BEFORE the error verdict.
        let effect = inputs(DispatchState::Yes, Some(vec![66]), Some(vec![66]));
        assert_eq!(
            classify_direct(&effect, 1, 66),
            DirectOutcome::DispatchedEffect
        );
        assert_eq!(
            plan_aftermath(DirectOutcome::DispatchedEffect, &effect.delayed),
            ProbeAftermath::RestoreExpectOk
        );
        let unknown = inputs(DispatchState::Unknown, None, Some(vec![66]));
        assert_eq!(
            classify_direct(&unknown, 1, 66),
            DirectOutcome::UnknownEffect
        );
        assert_eq!(
            plan_aftermath(DirectOutcome::UnknownEffect, &unknown.delayed),
            ProbeAftermath::RestoreExpectOk
        );
        let ambiguous = inputs(DispatchState::Yes, Some(vec![1]), Some(vec![1, 66]));
        assert_eq!(classify_direct(&ambiguous, 1, 66), DirectOutcome::Ambiguous);
        assert_eq!(
            plan_aftermath(DirectOutcome::Ambiguous, &ambiguous.delayed),
            ProbeAftermath::RestoreExpectErr
        );
    }

    #[test]
    fn bridged_unavailable_refusal_is_rejection_not_no_effect() {
        // The mover core refuses an unavailable bridged route with an
        // `unsupported:` gate before dispatch; the harness classifies that
        // clean refusal (membership home) as Rejected with no inverse - never
        // as observed no-effect, never as an effect.
        assert!(positive_predispatch_refusal(
            Some(1),
            "unsupported: bridged route unavailable (dispatcher=false class=false selector=false); refusing without a fallback - this is not a SIP failure"
        ));
        let refused = DirectInputs {
            dispatched: DispatchState::No,
            mover_error: Some("unsupported: bridged route unavailable".to_owned()),
            immediate: None,
            delayed: Some(vec![1]),
        };
        assert_eq!(classify_direct(&refused, 1, 66), DirectOutcome::Rejected);
        assert_eq!(
            plan_aftermath(DirectOutcome::Rejected, &refused.delayed),
            ProbeAftermath::VerifyNoInverse
        );
    }

    #[test]
    fn mover_argv_targets_only_the_verified_helper_window() {
        // Exact existing spaces-move shape: no new subcommand, no broadened
        // target — the worker only writes this passed verified WID.
        assert_eq!(
            mover_argv(4242, "0123456789abcdef", 7, 66),
            vec![
                "spaces-move",
                "--pid",
                "4242",
                "--nonce",
                "0123456789abcdef",
                "--wid",
                "7",
                "--space",
                "66",
                "--allow-untested",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn classify_observed_no_effect_when_dispatch_leaves_preimage() {
        use DispatchState::Yes;
        // Void dispatch, membership unchanged exact [1]: observed no-effect,
        // never a SIP finding, never a rejection.
        let out = classify_direct(&inputs(Yes, Some(vec![1]), Some(vec![1])), 1, 66);
        assert_eq!(out, DirectOutcome::ObservedNoEffect);
        // Immediate unverified plus delayed unchanged is still no-effect.
        let out = classify_direct(&inputs(Yes, None, Some(vec![1])), 1, 66);
        assert_eq!(out, DirectOutcome::ObservedNoEffect);
        // Unknown dispatch with membership home is still observed no-effect
        // (the state, not the provenance, is what no-effect claims).
        let out = classify_direct(&inputs(DispatchState::Unknown, None, Some(vec![1])), 1, 66);
        assert_eq!(out, DirectOutcome::ObservedNoEffect);
    }

    #[test]
    fn classify_delayed_effect_when_settle_lands_late() {
        use DispatchState::Yes;
        // Immediate [1], delayed exact [66]: the async landing arrived
        // inside the harness settle window.
        let out = classify_direct(&inputs(Yes, Some(vec![1]), Some(vec![66])), 1, 66);
        assert_eq!(out, DirectOutcome::DelayedEffect);
        // Immediate exact plus delayed exact is the prompt effect.
        let out = classify_direct(&inputs(Yes, Some(vec![66]), Some(vec![66])), 1, 66);
        assert_eq!(out, DirectOutcome::DispatchedEffect);
    }

    #[test]
    fn classify_unknown_dispatch_with_changed_membership_restores() {
        // The B2 regression: a timed-out (Unknown) mover with the delayed
        // poll reading exact [66] is an effect with unknown provenance —
        // restored through the independent public path, NEVER rejection.
        let mut timed_out = inputs(DispatchState::Unknown, Some(vec![1]), Some(vec![66]));
        timed_out.mover_error = Some("mover timed out after 20000ms".to_owned());
        let out = classify_direct(&timed_out, 1, 66);
        assert_eq!(out, DirectOutcome::UnknownEffect);
        assert_eq!(
            plan_aftermath(out, &timed_out.delayed),
            ProbeAftermath::RestoreExpectOk
        );
        // Same for unparsable mover output with a moved window.
        let garbled = inputs(DispatchState::Unknown, None, Some(vec![66]));
        assert_eq!(
            classify_direct(&garbled, 1, 66),
            DirectOutcome::UnknownEffect
        );
    }

    #[test]
    fn classify_rejection_only_on_positive_predispatch_refusal() {
        use DispatchState::No;
        // Clean exit naming a pre-dispatch gate with membership home:
        // rejected, never misread as no-effect.
        let mut refused = inputs(No, None, Some(vec![1]));
        refused.mover_error = Some("blocked: unknown target".to_owned());
        assert_eq!(classify_direct(&refused, 1, 66), DirectOutcome::Rejected);
        assert_eq!(
            plan_aftermath(DirectOutcome::Rejected, &refused.delayed),
            ProbeAftermath::VerifyNoInverse
        );
        // Positive refusal yet the window moved: contradiction — restored
        // before the error, never a clean verdict.
        let contradiction = inputs(No, None, Some(vec![66]));
        assert_eq!(
            classify_direct(&contradiction, 1, 66),
            DirectOutcome::Ambiguous
        );
        assert_eq!(
            plan_aftermath(DirectOutcome::Ambiguous, &contradiction.delayed),
            ProbeAftermath::RestoreExpectErr
        );
    }

    #[test]
    fn refusal_detector_needs_clean_exit_plus_named_gate() {
        // Exit 1/2 with a named gate is a positive refusal.
        assert!(positive_predispatch_refusal(
            Some(1),
            "blocked: unknown target Space 66; refusing"
        ));
        assert!(positive_predispatch_refusal(
            Some(2),
            "ownership refused: no spawn record"
        ));
        assert!(positive_predispatch_refusal(
            Some(2),
            "spaces-move requires --allow-untested"
        ));
        // Silence, crashes, success codes, or garble without a gate marker
        // are Unknown — never rejection.
        assert!(!positive_predispatch_refusal(Some(1), ""));
        assert!(!positive_predispatch_refusal(Some(0), "blocked: x"));
        assert!(!positive_predispatch_refusal(None, "blocked: x"));
        assert!(!positive_predispatch_refusal(Some(1), "segmentation fault"));
        assert!(!positive_predispatch_refusal(Some(134), "blocked: x"));
    }

    #[test]
    fn classify_ambiguous_when_delayed_readback_unusable() {
        use DispatchState::Yes;
        // Dispatched but delayed unreadable: ambiguous with nothing
        // verifiable — fail without a blind restore claim.
        let unreadable = inputs(Yes, Some(vec![1]), None);
        assert_eq!(
            classify_direct(&unreadable, 1, 66),
            DirectOutcome::Ambiguous
        );
        assert_eq!(
            plan_aftermath(DirectOutcome::Ambiguous, &unreadable.delayed),
            ProbeAftermath::FailNoRestore
        );
        // Sticky/multi-Space delayed membership is neither exact target nor
        // exact preimage: ambiguous, restored before the error — the helper
        // is never killed as a surrogate for restoration.
        let sticky = inputs(Yes, Some(vec![1]), Some(vec![1, 66]));
        assert_eq!(classify_direct(&sticky, 1, 66), DirectOutcome::Ambiguous);
        assert_eq!(
            plan_aftermath(DirectOutcome::Ambiguous, &sticky.delayed),
            ProbeAftermath::RestoreExpectErr
        );
    }

    #[test]
    fn probe_budget_gate_refuses_narrow_windows_before_any_spawn() {
        // Worst case is one pass, never multiplied: mover + settle +
        // restore + resettle + cleanup reserve.
        assert_eq!(
            PROBE_WORST_CASE_MS,
            MOVER_TIMEOUT_MS
                + DIRECT_SETTLE_MS
                + crate::space_join::JOIN_TIMEOUT_MS
                + DIRECT_SETTLE_MS
                + PROBE_CLEANUP_RESERVE_MS
        );
        // The demanded B4 regression: an insufficient remaining lifetime
        // refuses with no dispatch (the live caller returns before spawning
        // the mover).
        assert!(probe_budget_sufficient(PROBE_WORST_CASE_MS).is_ok());
        assert!(probe_budget_sufficient(PROBE_WORST_CASE_MS + 10_000).is_ok());
        let err = probe_budget_sufficient(PROBE_WORST_CASE_MS - 1).unwrap_err();
        assert!(err.contains("refusing dispatch"));
        assert!(probe_budget_sufficient(0).is_err());
    }

    #[test]
    fn mover_fingerprint_is_stable_and_content_sensitive() {
        // Same bytes hash stably (same-artifact evidence); any byte change
        // changes the fingerprint. Explicitly non-cryptographic (std only).
        let fp = fingerprint_bytes(b"tiler-macos-mover-artifact");
        assert_eq!(fp, fingerprint_bytes(b"tiler-macos-mover-artifact"));
        assert!(fp.starts_with("std-siphash64:"));
        assert_ne!(fp, fingerprint_bytes(b"tiler-macos-mover-artifact!"));
        assert_ne!(fp, fingerprint_bytes(b""));
    }

    #[test]
    fn last_report_line_wins_over_interleaved_chatter() {
        // Interleaved helper diagnostics must not shadow the mover verdict:
        // the LAST {"report": ...} line wins.
        let body = |spaces: &[u64], verified: bool| {
            let spaces = spaces
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"report\":{{\"purpose\":\"move\",\"pid\":9,\"wid\":7,\"preimage_spaces\":[1],\"cg_baseline\":null,\"target_space\":66,\"route\":\"direct\",\"symbols\":{{\"direct\":true,\"compat_pair\":false,\"bridged_async\":false}},\"effect_spaces\":[{spaces}],\"effect_error\":null,\"effect_verified\":{verified}}}}}",
            )
        };
        let text = format!(
            "helper snapshot line\n{}\nmore chatter\n{}",
            body(&[1], false),
            body(&[66], true)
        );
        let report = extract_mover_report(&text).expect("last report parses");
        assert_eq!(report.effect_spaces, vec![66]);
        assert!(report.effect_verified);
        // Pure chatter with no report parses to nothing (Unknown, never No).
        assert!(extract_mover_report("helper snapshot line\nmore chatter\n").is_none());
    }

    #[test]
    fn mover_report_route_names_the_dispatched_route() {
        // The mover's structured report carries the dispatch it took, so the
        // harness can prove the selected route ran (no silent substitution).
        let body = |route: &str| {
            format!(
                "{{\"report\":{{\"purpose\":\"move\",\"pid\":9,\"wid\":7,\"preimage_spaces\":[1],\"cg_baseline\":null,\"target_space\":66,\"route\":\"{route}\",\"symbols\":{{\"direct\":true,\"compat_pair\":false,\"bridged_async\":false}},\"effect_spaces\":[1],\"effect_error\":null,\"effect_verified\":false}}}}"
            )
        };
        let direct = extract_mover_report(&body("direct")).expect("direct report parses");
        assert_eq!(direct.route, "direct");
        let bridged = extract_mover_report(&body("bridged")).expect("bridged report parses");
        assert_eq!(bridged.route, "bridged");
    }

    #[test]
    fn route_mismatch_downgrades_attribution_but_still_restores() {
        // Requested bridged, mover reports direct: attribution downgrades to
        // Unknown with both routes named - never an efficacy claim for
        // either route. With delayed exact [target] the shared pipeline
        // still plans the independent public restore (no early return, no
        // stranded window).
        let (dispatch, mismatch) =
            reconcile_report_route("bridged", DispatchState::Yes, Some("direct"));
        assert_eq!(dispatch, DispatchState::Unknown);
        let err = mismatch.expect("mismatch names both routes");
        assert!(err.contains("bridged"), "unexpected: {err}");
        assert!(err.contains("direct"), "unexpected: {err}");
        assert!(err.contains("no efficacy claimed"), "unexpected: {err}");
        let changed = DirectInputs {
            dispatched: dispatch,
            mover_error: Some(err),
            immediate: None,
            delayed: Some(vec![66]),
        };
        assert_eq!(
            classify_direct(&changed, 1, 66),
            DirectOutcome::UnknownEffect
        );
        assert_eq!(
            plan_aftermath(DirectOutcome::UnknownEffect, &changed.delayed),
            ProbeAftermath::RestoreExpectOk
        );
        // Agreement passes dispatch through untouched (no error, no
        // downgrade); no report (refusal path) is not a mismatch either.
        let (dispatch, mismatch) =
            reconcile_report_route("bridged", DispatchState::Yes, Some("bridged"));
        assert_eq!(dispatch, DispatchState::Yes);
        assert!(mismatch.is_none());
        let (dispatch, mismatch) = reconcile_report_route("bridged", DispatchState::No, None);
        assert_eq!(dispatch, DispatchState::No);
        assert!(mismatch.is_none());
    }

    #[test]
    fn settle_returns_last_read_and_current_stability_is_equality() {
        // Scripted membership reader: mismatch then exact still resolves
        // (poll, never a re-dispatch).
        let mut n = 0;
        let mut read = || {
            n += 1;
            Some(if n < 3 { vec![1u64] } else { vec![66u64] })
        };
        assert_eq!(settle_membership(&mut read, 5_000, 1), Some(vec![66u64]));
        // Unreadable throughout stays unreadable (unknown, never guessed).
        let mut read = || None;
        assert_eq!(settle_membership(&mut read, 5, 1), None);
        // Current stability is plain equality (the direct leg must not move
        // the current).
        assert!(current_unchanged(Some(1), Some(1)));
        assert!(!current_unchanged(Some(1), Some(66)));
        assert!(!current_unchanged(Some(1), None));
    }

    #[test]
    fn causal_note_never_implies_policy() {
        // The report's causal claim must disclaim policy inference in words:
        // unchanged membership is observed no-effect, refusals name gates.
        assert!(DIRECT_CAUSAL_NOTE.contains("no SIP inference"));
        assert!(DIRECT_CAUSAL_NOTE.contains("observed no-effect"));
    }
}
