#![forbid(unsafe_code)]

//! Bounded same-display Space SWITCH comparison: system arrows vs yabai
//! Dock-swipe gesture (no SA, no SLS moves, no Space create/remove).
//!
//! Two owned regular-policy helpers (`O` on the original Space, `T` on the
//! existing temp Space, both main-held `Child` + nonce/exe/CG) bracket each
//! route leg. Proven Mission Control AX goto legs
//! ([`crate::mc_lifecycle::goto_sid_verified`]) provide setup and recovery;
//! the compared routes themselves are:
//! - arrow: one enabled `Ctrl+Right` down/up (O→T) then one `Ctrl+Left`
//!   down/up (T→O) through the session stream ([`crate::synth`],
//!   truthfully synthetic, exact-helper oracle before EACH edge);
//! - gesture: one same-display yabai Dock-swipe began/ended pair (fields
//!   55/110/123/124/129/132, no cursor warp/click) through the session
//!   tap, with the exact-helper oracle before EACH post including `ended`.
//!
//! Eligibility (fail closed, never a settings write):
//! - arrow legs need the read-only effective-table corroboration
//!   ([`crate::symbolic_hotkeys`]): a disabled/unresolved chord blocks its
//!   leg. Overrides `79/81 enabled-without-value` are a candidate hypothesis
//!   only; the actual switch experiment proves the mapping.
//! - digit/drag routes are ineligible (Desktop-N unresolved in incomplete
//!   overrides, no tag preimage, no shortcut enabling).
//! - gesture legs need same-display adjacency plus macOS < 27 (the rift
//!   ≥27 augmented payload needs an exact observed family select; the yabai
//!   pinned 6-field branch is the only candidate here, never a cause claim).
//!
//! Effect semantics: current-SID readback within 3 s is the only switch
//! evidence; helper memberships/frames must stay exactly unchanged (a
//! switch moves the current, never windows). Per-leg `*-post` latencies
//! cover owned-focus plus the synthetic post only — NOT end-to-end switch
//! time: the subsequent current-readback settle poll is excluded and no
//! elapsed-readback total is reported. A no-effect leg is never
//! repeated and its inverse is never sent as a no-op. A dispatched-but-
//! unverified goto is never replayed. Ghostty is read-only throughout
//! (natural return observed, never forced).
//!
//! Prior art (local checkouts first; pinned upstream URLs only):
//! gesture fields + began/ended session posts + same-display scope follow
//! yabai `space_manager_focus_space_using_gesture`
//! (https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L927-L1035);
//! the ≥27 coupling follows rift `src/sys/space_switch.rs`
//! (https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/space_switch.rs#L170-L223),
//! whose augmented payload is NOT used here.

use serde::Serialize;

use crate::mc_probe::{FocusLegs, GhosttyWindow, LegLatency, McError, TopologySnapshot};
use crate::space_switch_edges::{
    PairOutcome, PendingRelease, Slot, edge_synth_gates, gesture_payload_allowed, gesture_sign,
    post_arrow_held, post_gesture_held, post_release_once, release_allowed,
};
use crate::symbolic_hotkeys::{KEY_LEFT_ARROW, KEY_RIGHT_ARROW};

/// Minimum run budget: setup gotos plus two route pairs plus cleanup.
pub const MIN_SWITCH_SECS: u64 = 60;
/// Maximum run budget: the helper lifetime cap.
pub const MAX_SWITCH_SECS: u64 = crate::config::MAX_PROBE_SECONDS;
/// Current-SID effect settle after each synthetic leg (poll, never replay).
pub const SWITCH_SETTLE_MS: u64 = 3_000;
/// Settle poll slice.
pub const SWITCH_POLL_MS: u64 = 200;

/// Pure argument gate: explicit opt-in, two distinct nonzero SIDs, budget
/// inside `60..=120`. No live touch.
pub fn validate_switch_args(
    original: u64,
    temp: u64,
    seconds: u64,
    allow_untested: bool,
) -> Result<u64, String> {
    if !allow_untested {
        return Err(
            "spaces-switch-probe requires --allow-untested (synthetic arrows/gesture plus visible Space switches; recovery is observer-verified per run, never assumed)"
                .to_owned(),
        );
    }
    if original == 0 || temp == 0 {
        return Err("spaces-switch-probe needs nonzero --original and --temp SIDs".to_owned());
    }
    if original == temp {
        return Err("spaces-switch-probe needs distinct --original and --temp SIDs".to_owned());
    }
    crate::config::ProbeConfig::validate_probe_seconds(seconds)?;
    if seconds < MIN_SWITCH_SECS {
        return Err(format!(
            "spaces-switch-probe needs --seconds >= {MIN_SWITCH_SECS} (setup gotos plus two route pairs plus cleanup); refusing before any mutation"
        ));
    }
    Ok(seconds)
}

/// Pure post-leg verdict: `NoEffect` (stable `[current]` after a verified
/// dispatch) stops the route — never repeated, never inverted as a no-op.
/// `Switched` advances; `Ambiguous` recovers via MC AX.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegVerdict {
    Switched,
    NoEffect,
    Ambiguous,
}

#[must_use]
pub const fn classify_leg(current: Option<u64>, goal: u64, before: Option<u64>) -> LegVerdict {
    match (current, before) {
        (Some(c), _) if c == goal => LegVerdict::Switched,
        (Some(c), Some(b)) if c == b => LegVerdict::NoEffect,
        _ => LegVerdict::Ambiguous,
    }
}

/// Pure terminal outcome: any unresolved synthetic edge forces residue
/// even when currents, helpers, and Ghostty otherwise verify. Never a
/// pretend clean.
#[must_use]
pub const fn probe_outcome(
    currents_restored: bool,
    helpers_absent: bool,
    ghostty_ok: bool,
    unresolved_synthetic: bool,
) -> &'static str {
    if currents_restored && helpers_absent && ghostty_ok && !unresolved_synthetic {
        "switch-probe-complete"
    } else {
        "switch-probe-residue"
    }
}

#[derive(Debug, Clone, Serialize)]
struct SwitchLegReport {
    leg: String,
    route: String,
    dispatched: bool,
    posts: u32,
    current_before: Option<u64>,
    current_after: Option<u64>,
    verdict: String,
    memberships_unchanged: Option<bool>,
    /// `None` means the frame read was unavailable (named in
    /// `frame_error`) — never a false "changed".
    frames_unchanged: Option<bool>,
    frame_error: Option<String>,
    /// Present only when a posted edge could not be released in-pair:
    /// the pending-edge token (route, edge, exact target, refusal).
    /// `None` means no synthetic edge was left pending on this leg.
    pending_edge: Option<String>,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
struct SwitchProbeReport {
    outcome: String,
    original: u64,
    temp: u64,
    display: usize,
    pid_o: u32,
    wid_o: u32,
    pid_t: u32,
    wid_t: u32,
    bindings: crate::symbolic_hotkeys::ArrowCorroboration,
    legs: Vec<SwitchLegReport>,
    currents_restored: bool,
    helpers_stopped_exact: bool,
    helpers_absent: bool,
    ghostty_stable: bool,
    natural_focus_returned: bool,
    ghostty_before: Vec<GhosttyWindow>,
    ghostty_after: Vec<GhosttyWindow>,
    focus_before: FocusLegs,
    focus_after: FocusLegs,
    mc_open_after: Option<bool>,
    caffeinate_reaped: bool,
    synthetic_events_posted: bool,
    /// True when any posted edge could not be safely released: the report
    /// is residue by construction (never a pretend clean), and no further
    /// route ran after the pending edge.
    unresolved_synthetic: bool,
    live_evidence: String,
    notes: Vec<String>,
    latencies: Vec<LegLatency>,
}

/// Read the current SID for one display from a fresh snapshot.
fn current_of(display: usize) -> Option<u64> {
    crate::mc_lifecycle::live::fresh_snapshot()
        .ok()
        .and_then(|s| s.displays.get(display).and_then(|d| d.current))
}

/// Poll the current SID for exactly `goal` within the settle window.
/// Returns the last observed current (never a replay).
fn await_current(display: usize, goal: u64) -> Option<u64> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(SWITCH_SETTLE_MS);
    let mut last = current_of(display);
    if last == Some(goal) {
        return last;
    }
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(SWITCH_POLL_MS));
        last = current_of(display);
        if last == Some(goal) {
            return last;
        }
    }
    last
}

/// SLPS focus one exact owned helper with oracle agreement (established
/// focus module, never a systemwide AX gate).
fn focus_helper(pid: u32, wid: u32) -> Result<(), McError> {
    crate::focus::dispatch_focus(pid, wid)
        .map_err(|e| McError::Blocked(format!("focus dispatch refused: {e}")))?;
    crate::focus::await_agreement(pid, wid, std::time::Duration::from_secs(5))
        .map(|_| ())
        .map_err(|e| McError::Blocked(format!("focus agreement unproven: {e}")))?;
    crate::focus::read_agreement(pid, wid)
        .map(|_| ())
        .map_err(|e| McError::Blocked(format!("focus oracle disagrees before post: {e}")))
}

/// Setup-failure funnel: current restore, exact stops for all known
/// helpers, full absence + Ghostty + MC + caffeinate evidence. Expands to
/// an early `return Err(fail_early(..))` at each setup site.
macro_rules! setup_fail {
    ($stage:expr, $err:expr, $caff:expr, $helpers:expr, $display:expr, $original:expr, $snap0:expr, $ghost:expr, $budget:expr, $notes:expr) => {
        return Err(fail_early(
            $stage, $err, $caff, $helpers, $display, $original, $snap0, $ghost, $budget, $notes,
        ))
    };
}

/// Baseline membership check: exactly the one expected Space (a helper on
/// the wrong single Space is drift, never a baseline).
fn check_baseline_membership(pid: u32, wid: u32, expect: u64, name: &str) -> Result<(), String> {
    match membership_of(pid, wid) {
        Ok(m) if crate::space_recovery::membership_is_exact(&m, expect) => Ok(()),
        Ok(m) => Err(format!(
            "helper {name} membership {m:?}, not exactly [{expect}]"
        )),
        Err(e) => Err(e.to_string()),
    }
}

struct Helper {
    child: Option<std::process::Child>,
    pid: u32,
    nonce: String,
    helper_path: String,
    wid: u32,
    frame: Option<crate::ffi::AxFrame>,
}

/// Spawn one regular-policy helper and join its exact window (fresh CG
/// owner join; no title read). The caller holds the Child.
fn spawn_helper(seconds: u64) -> Result<Helper, McError> {
    let nonce =
        crate::probe_target::mint_nonce().map_err(|e| McError::System(format!("nonce: {e}")))?;
    let spec = crate::probe_target::ProbeTargetSpec::new(&nonce, 0, 0, seconds)
        .map(|s| s.with_regular_policy(true))
        .map_err(|e| McError::Blocked(format!("helper spec: {e}")))?;
    let mut raw = crate::probe_target::start_helper_process(&spec)
        .map_err(|e| McError::System(format!("helper spawn failed: {e}")))?;
    crate::probe_target::await_handshake(&mut raw, &spec)
        .map_err(|e| McError::System(format!("helper spawn failed: {e}")))?;
    if let Err(e) = crate::probe_target::record_spawn(raw.pid, &spec, &raw.helper_path) {
        let _ = raw.child.kill();
        let _ = raw.child.wait();
        return Err(McError::System(format!("helper spawn failed: {e}")));
    }
    let crate::probe_target::UnreadySpawn {
        mut child,
        pid,
        helper_path,
        lines,
        line_drops: _,
    } = raw;
    if let Err(e) = std::thread::Builder::new()
        .name("switch-probe-drain".to_owned())
        .spawn(move || while lines.recv().is_ok() {})
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(McError::System(format!("drain spawn: {e}")));
    }
    let owned = match crate::ax::authorize_owned_action(pid, &nonce) {
        Ok(o) => o,
        Err(e) => {
            // Exact handle owned here: kill it before reporting, never
            // orphan the helper process on an identity shortfall.
            let _ = child.kill();
            let _ = child.wait();
            return Err(McError::Ownership(format!("helper identity: {e}")));
        }
    };
    if crate::ownership::is_protected_bundle(&owned.bundle_id) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(McError::Ownership(
            "helper bound to a protected bundle; refusing".to_owned(),
        ));
    }
    let wid = match crate::focus::join_helper_wid(pid) {
        Ok(w) => w,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(McError::Ownership(format!("helper CG join: {e}")));
        }
    };
    let frame = match crate::ax::read_window_owned(&owned, pid, wid) {
        Ok(r) => r.ax_frame,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(McError::System(format!("helper frame: {e}")));
        }
    };
    Ok(Helper {
        child: Some(child),
        pid,
        nonce,
        helper_path,
        wid,
        frame,
    })
}

fn helper_child(helper: &mut Helper) -> Result<&mut std::process::Child, McError> {
    helper
        .child
        .as_mut()
        .ok_or_else(|| McError::System("helper Child already taken".to_owned()))
}

fn membership_of(pid: u32, wid: u32) -> Result<Vec<u64>, McError> {
    // Join to the owned PID first so a reused WID never authorizes a read.
    crate::cg::verify_wid_owned_by_pid(pid, wid)
        .map_err(|e| McError::Ownership(format!("CG join lost: {e}")))?;
    crate::spaces::window_spaces(wid).map_err(|e| McError::System(e.to_string()))
}

/// One leg report line: every leg — switched, refused, unsupported,
/// skipped, or pending — carries the same shape, so a missing edge is
/// never silent. `unchanged` is (memberships, frames, frame_error): frames
/// is `None` when unreadable (unknown, never a false changed).
#[allow(clippy::too_many_arguments)]
fn leg_report(
    leg: &str,
    route: &str,
    dispatched: bool,
    posts: u32,
    before: Option<u64>,
    after: Option<u64>,
    verdict: &str,
    unchanged: Option<(bool, Option<bool>, Option<String>)>,
    pending_edge: Option<String>,
    note: String,
) -> SwitchLegReport {
    SwitchLegReport {
        leg: leg.to_owned(),
        route: route.to_owned(),
        dispatched,
        posts,
        current_before: before,
        current_after: after,
        verdict: verdict.to_owned(),
        memberships_unchanged: unchanged.as_ref().map(|(m, _, _)| *m),
        frames_unchanged: unchanged.as_ref().and_then(|(_, f, _)| *f),
        frame_error: unchanged.as_ref().and_then(|(_, _, e)| e.clone()),
        pending_edge,
        note,
    }
}

/// An ineligible route leg: never dispatched, never posted.
fn unsupported_leg(display: usize, leg: &str, route: &str, note: String) -> SwitchLegReport {
    leg_report(
        leg,
        route,
        false,
        0,
        current_of(display),
        current_of(display),
        "unsupported",
        None,
        None,
        note,
    )
}

/// Which synthetic pair a leg posts.
#[derive(Debug, Clone, Copy)]
enum PairKind {
    Arrow { keycode: u32 },
    Gesture { sign: f64 },
}

/// Where a leg leaves the route: the next leg may run (`Advance`), this
/// route stops but the next route may run (`StopRoute`), or every further
/// route halts (`HaltAll` — pending edge, even when released).
enum LegNext {
    Advance,
    StopRoute,
    HaltAll,
}

/// A leg skipped because an earlier pending edge halted all routes.
/// Honest accounting: the leg never dispatched, never posted.
fn push_skipped(
    legs: &mut Vec<SwitchLegReport>,
    display: usize,
    leg: &str,
    route: &str,
    notes: &mut Vec<String>,
) {
    legs.push(leg_report(
        leg,
        route,
        false,
        0,
        current_of(display),
        current_of(display),
        "skipped",
        None,
        None,
        "skipped: an earlier pending edge halted all further routes".to_owned(),
    ));
    notes.push(format!("{leg}: skipped after pending-edge halt"));
}

/// One route's outward + return legs. Runs the return leg only when the
/// outward leg switched; a pending edge on either leg halts every further
/// route (returns true), even when released — the completed pair is
/// evidence only, never a reason to run an extra route.
#[allow(clippy::too_many_arguments)]
fn run_route_pair(
    o: &mut Helper,
    t: &mut Helper,
    display: usize,
    original: u64,
    temp: u64,
    route: &str,
    outward_leg: &str,
    outward_slot: Slot,
    outward_goal: u64,
    outward_kind: PairKind,
    outward_describe: String,
    return_leg: &str,
    return_kind: PairKind,
    return_describe: String,
    budget: &mut crate::mc_probe::ActionBudget,
    notes: &mut Vec<String>,
    latencies: &mut Vec<LegLatency>,
    legs: &mut Vec<SwitchLegReport>,
    synthetic_posts: &mut u32,
    unresolved: &mut bool,
) -> bool {
    let outward = run_synthetic_leg(
        o,
        t,
        outward_slot,
        outward_goal,
        display,
        original,
        temp,
        outward_leg,
        route,
        outward_kind,
        outward_describe,
        budget,
        notes,
        latencies,
        legs,
        synthetic_posts,
        unresolved,
    );
    if !matches!(outward, LegNext::Advance) {
        return matches!(outward, LegNext::HaltAll);
    }
    matches!(
        run_synthetic_leg(
            o,
            t,
            Slot::T,
            original,
            display,
            original,
            temp,
            return_leg,
            route,
            return_kind,
            return_describe,
            budget,
            notes,
            latencies,
            legs,
            synthetic_posts,
            unresolved,
        ),
        LegNext::HaltAll
    )
}
/// One focused synthetic pair leg: focus the exact owned helper, post the
/// pair, read back the current, verify helper memberships/frames. Off-nominal
/// legs recover via MC AX and stop their route (never repeated/inverted);
/// a pending edge resolves explicitly and halts every further route.
#[allow(clippy::too_many_arguments)]
fn run_synthetic_leg(
    o: &mut Helper,
    t: &mut Helper,
    slot: Slot,
    goal: u64,
    display: usize,
    original: u64,
    temp: u64,
    leg: &str,
    route: &str,
    kind: PairKind,
    describe: String,
    budget: &mut crate::mc_probe::ActionBudget,
    notes: &mut Vec<String>,
    latencies: &mut Vec<LegLatency>,
    legs: &mut Vec<SwitchLegReport>,
    synthetic_posts: &mut u32,
    unresolved: &mut bool,
) -> LegNext {
    let before = current_of(display);
    let t0 = std::time::Instant::now();
    // Focus the exact owned helper first (proven SLPS+oracle method).
    let focused = {
        let helper = helper_for(o, t, slot);
        focus_helper(helper.pid, helper.wid)
    };
    if let Err(e) = focused {
        latencies.push(LegLatency {
            leg: format!("{leg}-post"),
            ms: t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        });
        let unchanged = unchanged_pair(o, t, original, temp);
        legs.push(leg_report(
            leg,
            route,
            false,
            0,
            before,
            current_of(display),
            "refused",
            Some(unchanged),
            None,
            format!("owned focus refused before any post: {e}"),
        ));
        recover_current(display, original, budget, notes);
        return LegNext::StopRoute;
    }
    let helper = helper_for(o, t, slot);
    let (pid, wid) = (helper.pid, helper.wid);
    let helper_path = helper.helper_path.clone();
    let outcome = match helper_child(helper) {
        Ok(child) => match kind {
            PairKind::Arrow { keycode } => {
                post_arrow_held(child, slot, pid, wid, &helper_path, keycode)
            }
            PairKind::Gesture { sign } => {
                post_gesture_held(child, slot, pid, wid, &helper_path, sign)
            }
        },
        Err(e) => PairOutcome::Refused(format!("helper Child unavailable: {e}")),
    };
    latencies.push(LegLatency {
        leg: format!("{leg}-post"),
        ms: t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    });
    match outcome {
        PairOutcome::Paired => {
            *synthetic_posts += 2;
            let after = await_current(display, goal);
            let verdict = match classify_leg(after, goal, before) {
                LegVerdict::Switched => "switched",
                LegVerdict::NoEffect => "noeffect",
                LegVerdict::Ambiguous => "ambiguous",
            };
            let unchanged = unchanged_pair(o, t, original, temp);
            legs.push(leg_report(
                leg,
                route,
                true,
                2,
                before,
                after,
                verdict,
                Some(unchanged),
                None,
                describe,
            ));
            if verdict == "switched" {
                LegNext::Advance
            } else {
                notes.push(format!(
                    "{leg} {verdict}: no repeat, no no-op inverse; MC AX recovery follows"
                ));
                recover_current(display, original, budget, notes);
                LegNext::StopRoute
            }
        }
        PairOutcome::Refused(error) => {
            let unchanged = unchanged_pair(o, t, original, temp);
            legs.push(leg_report(
                leg,
                route,
                false,
                0,
                before,
                current_of(display),
                "refused",
                Some(unchanged),
                None,
                error,
            ));
            recover_current(display, original, budget, notes);
            LegNext::StopRoute
        }
        PairOutcome::Pending {
            posted,
            pending,
            error,
        } => {
            *synthetic_posts += posted;
            let token = pending.token(&error);
            notes.push(token.clone());
            match resolve_pending_release(
                o, t, &pending, display, original, budget, notes, latencies,
            ) {
                PendingResolution::Released {
                    posts,
                    current_after,
                } => {
                    *synthetic_posts += posts;
                    let unchanged = unchanged_pair(o, t, original, temp);
                    legs.push(leg_report(
                        leg, route, true, posted + posts, before, current_after,
                        "pending-released", Some(unchanged), Some(token),
                        "pair completed once via explicit pending-edge recovery (evidence only — no extra route)".to_owned(),
                    ));
                    LegNext::HaltAll
                }
                PendingResolution::Unresolved(reason) => {
                    *unresolved = true;
                    let unchanged = unchanged_pair(o, t, original, temp);
                    legs.push(leg_report(
                        leg,
                        route,
                        true,
                        posted,
                        before,
                        current_of(display),
                        "pending-unresolved",
                        Some(unchanged),
                        Some(token),
                        reason,
                    ));
                    LegNext::HaltAll
                }
            }
        }
    }
}

/// Bounded switch comparison over two EXISTING user Spaces. Creates and
/// destroys no Space, performs no SLS move, posts no tap input: only the
/// arrow chords and the yabai gesture pair are synthesized, each gated by
/// the exact held helper plus the fresh selected oracle.
pub fn run_spaces_switch_probe(
    original: u64,
    temp: u64,
    seconds: u64,
    allow_untested: bool,
) -> Result<String, (i32, String)> {
    validate_switch_args(original, temp, seconds, allow_untested).map_err(|e| (2, e))?;

    if let Err(e) = crate::mc_lifecycle::live::action_gates() {
        return Err((1, serde_json::json!({"stage": "pre-gates", "action": "spaces-switch-probe", "error": e.to_string()}).to_string()));
    }
    // Original frontmost baseline FIRST (user-owned physical restoration
    // context; never an agent write to the terminal).
    let frontmost_before = crate::ffi::frontmost_app();
    let ghostty_before = crate::mc_lifecycle::snapshot_ghostty_windows().map_err(|e| {
        (
            1,
            serde_json::json!({"stage": "baseline-ghostty", "error": e.to_string()}).to_string(),
        )
    })?;
    let focus_before = crate::mc_lifecycle::capture_focus_legs();
    let snap0 = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| {
        (
            1,
            serde_json::json!({"stage": "baseline-topology", "error": e.to_string()}).to_string(),
        )
    })?;
    let (display, _) = snap0.locate(original).ok_or_else(|| {
        (
            1,
            format!("original Space {original} not uniquely located; refusing before any mutation"),
        )
    })?;
    let (temp_display, _) = snap0.locate(temp).ok_or_else(|| {
        (
            1,
            format!("temp Space {temp} not uniquely located; refusing before any mutation"),
        )
    })?;
    if temp_display != display {
        return Err((
            1,
            format!(
                "cross-display switch {original} -> {temp} refused: only same-display comparison is in plan — this is not a SIP failure"
            ),
        ));
    }
    // Read-only effective bindings BEFORE any synthetic leg: disabled or
    // unresolved arrow chords block their legs (no settings write).
    let bindings_report = crate::symbolic_hotkeys::read_bindings().map_err(|e| {
        (
            1,
            serde_json::json!({"stage": "bindings", "error": e}).to_string(),
        )
    })?;
    let bindings = bindings_report.corroboration;
    // Same-display adjacency from SLS order (no guessed desktop numbers).
    let sign = gesture_sign(&snap0.displays[display].spaces, original, temp);
    if let Err(e) = gesture_payload_allowed(crate::ffi::macos_major()) {
        return Err((
            1,
            serde_json::json!({"stage": "version-gate", "error": e}).to_string(),
        ));
    }

    let spawn_lock = crate::ffi::session_state();
    if !spawn_lock.unlocked() {
        return Err((
            1,
            serde_json::json!({"stage": "pre-gates", "error": format!("window-server session not unlocked ({:?})", spawn_lock.lock)}).to_string(),
        ));
    }
    let mut caffeinate = crate::mc_lifecycle::live::CaffeinateGuard::spawn(seconds)
        .map_err(|e| (1, format!("caffeinate spawn failed: {e}")))?;
    let mut budget = crate::mc_probe::ActionBudget::new(seconds);
    let mut notes = vec![format!(
        "frontmost at entry: {frontmost_before:?}; bindings ctrl_left={} ctrl_right={} over {} entries",
        bindings.ctrl_left_enabled, bindings.ctrl_right_enabled, bindings.entries_seen
    )];
    let mut latencies: Vec<LegLatency> = Vec::new();
    let mut legs: Vec<SwitchLegReport> = Vec::new();
    let mut synthetic_posts: u32 = 0;

    // Setup: helper O on the original (current), MC-goto T, helper T on T,
    // MC-goto O. Proven AX recovery legs only; zero SLS moves. Every
    // failure below funnels through fail_early: current restore, exact
    // stops for all known helpers, absence + Ghostty + MC evidence.
    let mut helper_o = match spawn_helper(seconds) {
        Ok(h) => h,
        Err(e) => setup_fail!(
            "spawn-o",
            e.to_string(),
            &mut caffeinate,
            &mut Vec::new(),
            display,
            original,
            &snap0,
            &ghostty_before,
            &mut budget,
            &mut notes
        ),
    };
    if let Err(msg) = check_baseline_membership(helper_o.pid, helper_o.wid, original, "O") {
        let mut known = vec![helper_o];
        setup_fail!(
            "baseline-o",
            msg,
            &mut caffeinate,
            &mut known,
            display,
            original,
            &snap0,
            &ghostty_before,
            &mut budget,
            &mut notes
        );
    }
    let t = std::time::Instant::now();
    let snap_t =
        match crate::mc_lifecycle::goto_sid_verified(&snap0, display, temp, &mut budget, false) {
            Ok((s, close)) => {
                latencies.push(LegLatency {
                    leg: "setup-goto-temp".to_owned(),
                    ms: t.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                });
                if let Some(c) = close {
                    notes.push(format!("setup goto close shortfall: {c}"));
                }
                s
            }
            Err(e) => {
                let mut known = vec![helper_o];
                setup_fail!(
                    "setup-goto-temp",
                    e.to_string(),
                    &mut caffeinate,
                    &mut known,
                    display,
                    original,
                    &snap0,
                    &ghostty_before,
                    &mut budget,
                    &mut notes
                );
            }
        };
    let mut helper_t = match spawn_helper(seconds) {
        Ok(h) => h,
        Err(e) => {
            let mut known = vec![helper_o];
            setup_fail!(
                "spawn-t",
                e.to_string(),
                &mut caffeinate,
                &mut known,
                display,
                original,
                &snap0,
                &ghostty_before,
                &mut budget,
                &mut notes
            );
        }
    };
    if let Err(msg) = check_baseline_membership(helper_t.pid, helper_t.wid, temp, "T") {
        let mut known = vec![helper_o, helper_t];
        setup_fail!(
            "baseline-t",
            msg,
            &mut caffeinate,
            &mut known,
            display,
            original,
            &snap0,
            &ghostty_before,
            &mut budget,
            &mut notes
        );
    }
    let t = std::time::Instant::now();
    let snap_o = match crate::mc_lifecycle::goto_sid_verified(
        &snap_t,
        display,
        original,
        &mut budget,
        false,
    ) {
        Ok((s, close)) => {
            latencies.push(LegLatency {
                leg: "setup-goto-original".to_owned(),
                ms: t.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            });
            if let Some(c) = close {
                notes.push(format!("setup return close shortfall: {c}"));
            }
            s
        }
        Err(e) => {
            let mut known = vec![helper_o, helper_t];
            setup_fail!(
                "setup-goto-original",
                e.to_string(),
                &mut caffeinate,
                &mut known,
                display,
                original,
                &snap0,
                &ghostty_before,
                &mut budget,
                &mut notes
            );
        }
    };
    let _ = snap_o;
    let frame_o_baseline = helper_o.frame;
    let frame_t_baseline = helper_t.frame;

    // --- Arrow route: O→T via Ctrl+Right, T→O via Ctrl+Left. ---
    // A pending edge halts ALL further routes (even when released: the
    // pair is evidence only, never a reason to run an extra route).
    let mut halted = false;
    let mut unresolved_synthetic = false;
    let arrow_eligible = bindings.both_enabled();
    if !arrow_eligible {
        legs.push(unsupported_leg(
            display,
            "arrow-o-t",
            "arrow",
            format!(
                "arrow legs ineligible: effective corroboration ctrl_left={} ctrl_right={}; unresolved or disabled chords block the route (no settings write) — this is not a SIP failure",
                bindings.ctrl_left_enabled, bindings.ctrl_right_enabled
            ),
        ));
        notes.push(
            "arrow route skipped as unsupported (bindings); gesture comparison continues"
                .to_owned(),
        );
    } else {
        halted = run_route_pair(
            &mut helper_o,
            &mut helper_t,
            display,
            original,
            temp,
            "arrow",
            "arrow-o-t",
            Slot::O,
            temp,
            PairKind::Arrow {
                keycode: KEY_RIGHT_ARROW,
            },
            "single Ctrl+Right down/up on exact O-helper; current readback only".to_owned(),
            "arrow-t-o",
            PairKind::Arrow {
                keycode: KEY_LEFT_ARROW,
            },
            "single Ctrl+Left down/up on exact T-helper; current readback only".to_owned(),
            &mut budget,
            &mut notes,
            &mut latencies,
            &mut legs,
            &mut synthetic_posts,
            &mut unresolved_synthetic,
        );
    }

    // --- Gesture route: same-display yabai began/ended pair. ---
    match sign {
        None => {
            legs.push(unsupported_leg(
                display,
                "gesture-o-t",
                "gesture",
                "gesture legs ineligible: O and T are not adjacent in SLS order (or unmapped); no multi-step loop here — this is not a SIP failure".to_owned(),
            ));
        }
        Some(s) => {
            if halted {
                push_skipped(&mut legs, display, "gesture-o-t", "gesture", &mut notes);
                push_skipped(&mut legs, display, "gesture-t-o", "gesture", &mut notes);
            } else {
                halted = run_route_pair(
                    &mut helper_o,
                    &mut helper_t,
                    display,
                    original,
                    temp,
                    "gesture",
                    "gesture-o-t",
                    Slot::O,
                    temp,
                    PairKind::Gesture { sign: s },
                    format!(
                        "yabai began/ended pair sign={s:+} on exact O-helper, same display, no warp/click"
                    ),
                    "gesture-t-o",
                    PairKind::Gesture { sign: -s },
                    format!(
                        "yabai began/ended pair sign={:+} on exact T-helper, same display, no warp/click",
                        -s
                    ),
                    &mut budget,
                    &mut notes,
                    &mut latencies,
                    &mut legs,
                    &mut synthetic_posts,
                    &mut unresolved_synthetic,
                );
            }
        }
    }
    if halted {
        notes.push(
            "routes halted after a pending edge: no extra route ran, even when the pair was completed"
                .to_owned(),
        );
    }

    // Success cleanup: MC close where safe, caffeinate reaped last, exact
    // stops for BOTH helpers (never a PID guess, never a broad kill), then
    // verification: currents, helper absence, Ghostty stability + natural
    // focus return (observed, never forced), frames exact.
    crate::mc_lifecycle::safe_close_mc(&mut notes);
    let reaped = caffeinate.stop();
    let stop_o = helper_o
        .child
        .as_mut()
        .map(|c| crate::session::stop_exact_child(c, helper_o.pid));
    let stop_t = helper_t
        .child
        .as_mut()
        .map(|c| crate::session::stop_exact_child(c, helper_t.pid));
    let stopped_exact =
        stop_o.as_ref().is_some_and(|s| s.exited) && stop_t.as_ref().is_some_and(|s| s.exited);
    notes.push(format!(
        "helper O exact-Child stop: {:?}; helper T exact-Child stop: {:?}",
        stop_o.map(|s| (s.exited, s.reaped, s.record_removed)),
        stop_t.map(|s| (s.exited, s.reaped, s.record_removed)),
    ));
    let helpers_absent = !crate::ffi::process_exists(helper_o.pid as i32)
        && !crate::ffi::process_exists(helper_t.pid as i32)
        && crate::ffi::cg_window_list_all()
            .map(|list| {
                list.iter()
                    .all(|w| w.pid != helper_o.pid && w.pid != helper_t.pid)
            })
            .unwrap_or(false);
    let snap_final = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| {
        (
            1,
            serde_json::json!({"stage": "verify-topology", "error": e.to_string()}).to_string(),
        )
    })?;
    let currents_restored = snap_final.displays.get(display).and_then(|d| d.current)
        == snap0.displays.get(display).and_then(|d| d.current);
    let ghostty_after = crate::mc_lifecycle::snapshot_ghostty_windows().map_err(|e| {
        (
            1,
            serde_json::json!({"stage": "verify-ghostty", "error": e.to_string()}).to_string(),
        )
    })?;
    let ghostty_ok = crate::mc_probe::ghostty_stable_across_switch(&ghostty_before, &ghostty_after);
    let focus_after = crate::mc_lifecycle::capture_focus_legs();
    let natural_return = crate::mc_probe::focus_ghostty_returned(&focus_after);
    if !natural_return {
        notes.push(
            "no natural Ghostty focus return observed after helper stops (reported, not claimed)"
                .to_owned(),
        );
    }
    let frames_exact = helper_o.frame == frame_o_baseline && helper_t.frame == frame_t_baseline;
    notes.push(format!(
        "helper frames exact across battery: {frames_exact} (switch moves the current, never windows)"
    ));
    let switched_legs = legs.iter().filter(|l| l.verdict == "switched").count();
    let pending_legs = legs
        .iter()
        .filter(|l| l.verdict == "pending-released" || l.verdict == "pending-unresolved")
        .count();
    let report = SwitchProbeReport {
        outcome: probe_outcome(
            currents_restored,
            helpers_absent,
            ghostty_ok,
            unresolved_synthetic,
        )
        .to_owned(),
        original,
        temp,
        display,
        pid_o: helper_o.pid,
        wid_o: helper_o.wid,
        pid_t: helper_t.pid,
        wid_t: helper_t.wid,
        bindings,
        legs,
        currents_restored,
        helpers_stopped_exact: stopped_exact,
        helpers_absent,
        ghostty_stable: ghostty_ok,
        natural_focus_returned: natural_return,
        ghostty_before,
        ghostty_after,
        focus_before,
        focus_after,
        mc_open_after: crate::mc_lifecycle::live::mc_group_present().ok(),
        caffeinate_reaped: reaped,
        synthetic_events_posted: synthetic_posts > 0,
        unresolved_synthetic,
        live_evidence: format!(
            "{switched_legs} switched legs by exact current-SID readback within {}ms; {pending_legs} pending-completed legs; helper memberships/frames required exactly unchanged; no retries, no no-op inverses, no extra route after a pending edge",
            SWITCH_SETTLE_MS
        ),
        notes,
        latencies,
    };
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}

/// Pure frame verdict over plain data: both reads present → exact compare;
/// either missing → unknown (`None`) with the legs named, never a false
/// changed (an off-current helper's AX window is often unreadable).
#[must_use]
pub fn classify_frames(
    read_o: Result<crate::ffi::AxFrame, String>,
    base_o: Option<crate::ffi::AxFrame>,
    read_t: Result<crate::ffi::AxFrame, String>,
    base_t: Option<crate::ffi::AxFrame>,
) -> (Option<bool>, Option<String>) {
    match (read_o, read_t) {
        (Ok(fo), Ok(ft)) => (Some(Some(fo) == base_o && Some(ft) == base_t), None),
        (o, t) => {
            let mut missing = Vec::new();
            if let Err(e) = o {
                missing.push(format!("O frame unreadable: {e}"));
            }
            if let Err(e) = t {
                missing.push(format!("T frame unreadable: {e}"));
            }
            (None, Some(missing.join("; ")))
        }
    }
}

/// One owned helper's live frame (owned Child + nonce + AX join held).
fn read_helper_frame(h: &mut Helper) -> Result<crate::ffi::AxFrame, String> {
    let child = helper_child(h).map_err(|e| format!("helper Child unavailable: {e}"))?;
    let _ = child;
    let owned =
        crate::ax::authorize_owned_action(h.pid, &h.nonce).map_err(|e| format!("identity: {e}"))?;
    crate::ax::read_window_owned(&owned, h.pid, h.wid)
        .map_err(|e| format!("AX read: {e}"))?
        .ax_frame
        .ok_or_else(|| "AX frame missing".to_owned())
}

/// Verify both helpers still hold their EXACT expected single-Space
/// memberships plus exact baseline frames. A switch moves the current
/// Space, never windows: a helper on the wrong single Space (even still
/// single) is drift, never unchanged. Unreadable MEMBERSHIP counts as
/// changed; unreadable FRAMES count as unknown (`None` + named errors).
/// Infallible by design: every leg reports the booleans, never an exit.
fn unchanged_pair(
    o: &mut Helper,
    t: &mut Helper,
    expect_o: u64,
    expect_t: u64,
) -> (bool, Option<bool>, Option<String>) {
    let mem_ok = membership_of(o.pid, o.wid)
        .map(|m| crate::space_recovery::membership_is_exact(&m, expect_o))
        .unwrap_or(false)
        && membership_of(t.pid, t.wid)
            .map(|m| crate::space_recovery::membership_is_exact(&m, expect_t))
            .unwrap_or(false);
    let (frames, error) =
        classify_frames(read_helper_frame(o), o.frame, read_helper_frame(t), t.frame);
    (mem_ok, frames, error)
}

/// Mutable access to one helper slot. Both helpers stay main-held exact
/// `Child` handles for the whole run; cleanup always owns them.
fn helper_for<'a>(o: &'a mut Helper, t: &'a mut Helper, slot: Slot) -> &'a mut Helper {
    match slot {
        Slot::O => o,
        Slot::T => t,
    }
}

/// Resolution of one pending release: `Released` (the pair is complete —
/// evidence only, no further route runs) or `Unresolved` (parked dirty).
enum PendingResolution {
    Released {
        posts: u32,
        current_after: Option<u64>,
    },
    Unresolved(String),
}

/// Explicit pending-edge recovery: complete the posted pair exactly once
/// (MC-restore current, refocus the intended OWNED helper only, recheck
/// gates/holder/exe/CG/oracle, post ONLY the pending release, MC-restore
/// when the release moved Space). Any shortfall parks as unresolved:
/// no blind release, no re-send, no retry, no pretend clean.
#[allow(clippy::too_many_arguments)]
fn resolve_pending_release(
    o: &mut Helper,
    t: &mut Helper,
    pending: &PendingRelease,
    display: usize,
    original: u64,
    budget: &mut crate::mc_probe::ActionBudget,
    notes: &mut Vec<String>,
    latencies: &mut Vec<LegLatency>,
) -> PendingResolution {
    let t0 = std::time::Instant::now();
    notes.push(pending.token("explicit pending-edge recovery; completing the pair once"));
    // (1) Current side first, through the independent MC route.
    recover_current(display, original, budget, notes);
    // (2) Intended OWNED helper focus only (proven method; never Ghostty).
    {
        let helper = helper_for(o, t, pending.slot);
        if let Err(e) = focus_helper(helper.pid, helper.wid) {
            return PendingResolution::Unresolved(format!(
                "pending release parked: owned focus restore refused ({e}); unresolved synthetic state"
            ));
        }
    }
    // (3) Every recheck, no exceptions: gates, holder/exe/CG, exact oracle.
    let helper = helper_for(o, t, pending.slot);
    let (pid, wid) = (helper.pid, helper.wid);
    let helper_path = helper.helper_path.clone();
    let gates_ok = edge_synth_gates("pending-release").is_ok();
    let child = match helper_child(helper) {
        Ok(c) => c,
        Err(e) => {
            return PendingResolution::Unresolved(format!(
                "pending release parked: helper Child unavailable ({e}); unresolved synthetic state"
            ));
        }
    };
    let target = match crate::synth::OwnedPostTarget::from_held_child(child, pid, wid, &helper_path)
    {
        Ok(t) => t,
        Err(e) => {
            return PendingResolution::Unresolved(format!(
                "pending release parked: holder/exe/CG reproof refused ({e}); unresolved synthetic state"
            ));
        }
    };
    let oracle_exact = crate::focus::read_agreement(pid, wid).is_ok();
    if !release_allowed(gates_ok, true, oracle_exact) {
        return PendingResolution::Unresolved(format!(
            "pending release parked: rechecks fail (gates_ok={gates_ok} oracle_exact={oracle_exact}); unresolved synthetic state — no blind release"
        ));
    }
    // (4) The pending release edge ONLY, posted once.
    let posted = match post_release_once(&target, pending) {
        Ok(n) => n,
        Err(why) => {
            return PendingResolution::Unresolved(format!(
                "pending release parked: {why}; unresolved synthetic state"
            ));
        }
    };
    latencies.push(LegLatency {
        leg: "pending-release".to_owned(),
        ms: t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    });
    // (5) The release may move the current: restore via MC AX when needed.
    let current_after = current_of(display);
    if current_after != Some(original) {
        notes.push("pending release changed the current side; MC AX restore follows".to_owned());
        recover_current(display, original, budget, notes);
    }
    notes.push(
        "pending release posted once (pair complete, evidence only — no further route)".to_owned(),
    );
    PendingResolution::Released {
        posts: posted,
        current_after: current_of(display),
    }
}

/// Pure early-report contract: every terminal setup failure carries exact
/// helper stops, absence, currents, Ghostty, MC state, and caffeinate.
/// `pub(crate)` for the offline test split (same crate, no behavior change).
#[allow(clippy::too_many_arguments)]
pub(crate) fn early_report(
    stage: &str,
    error_joined: &str,
    original: u64,
    display: usize,
    stops: Vec<serde_json::Value>,
    helpers_absent: bool,
    currents_restored: bool,
    ghostty_ok: bool,
    mc_open_after: Option<bool>,
    reaped: bool,
) -> String {
    serde_json::json!({
        "stage": stage,
        "action": "spaces-switch-probe",
        "error": error_joined,
        "original": original,
        "display": display,
        "helper_stops": stops,
        "helpers_absent": helpers_absent,
        "currents_restored": currents_restored,
        "ghostty_stable": ghostty_ok,
        "mc_open_after": mc_open_after,
        "caffeinate_reaped": reaped,
        "synthetic_events_posted": false,
        "mutates": true,
    })
    .to_string()
}
/// Structured early-failure cleanup: MC current restore, MC close, exact
/// stops for every known helper (exact `Child` handles only), then absence
/// + current + Ghostty + MC evidence with caffeinate reaped last.
#[allow(clippy::too_many_arguments)]
fn fail_early(
    stage: &str,
    error: String,
    caffeinate: &mut crate::mc_lifecycle::live::CaffeinateGuard,
    helpers: &mut [Helper],
    display: usize,
    original: u64,
    snap0: &TopologySnapshot,
    ghostty_before: &[GhosttyWindow],
    budget: &mut crate::mc_probe::ActionBudget,
    notes: &mut Vec<String>,
) -> (i32, String) {
    notes.push(format!("{stage}: {error}"));
    recover_current(display, original, budget, notes);
    crate::mc_lifecycle::safe_close_mc(notes);
    let mut stops = Vec::new();
    for h in helpers.iter_mut() {
        match h.child.as_mut() {
            Some(c) => {
                let s = crate::session::stop_exact_child(c, h.pid);
                stops.push(serde_json::json!({
                    "pid": h.pid,
                    "exited": s.exited,
                    "reaped": s.reaped,
                    "record_removed": s.record_removed,
                }));
            }
            None => stops.push(serde_json::json!({
                "pid": h.pid,
                "exited": false,
                "reaped": false,
                "record_removed": false,
                "note": "Child already taken; no signal sent",
            })),
        }
    }
    let pids: Vec<u32> = helpers.iter().map(|h| h.pid).collect();
    let helpers_absent = helpers
        .iter()
        .all(|h| !crate::ffi::process_exists(h.pid as i32))
        && crate::ffi::cg_window_list_all()
            .map(|list| list.iter().all(|w| !pids.contains(&w.pid)))
            .unwrap_or(false);
    let snap_final = crate::mc_lifecycle::live::fresh_snapshot().ok();
    let currents_restored = snap_final.as_ref().is_some_and(|s| {
        s.displays.get(display).and_then(|d| d.current)
            == snap0.displays.get(display).and_then(|d| d.current)
    });
    let ghostty_after = crate::mc_lifecycle::snapshot_ghostty_windows().ok();
    let ghostty_ok = ghostty_after
        .as_ref()
        .is_some_and(|after| crate::mc_probe::ghostty_stable_across_switch(ghostty_before, after));
    let reaped = caffeinate.stop();
    (
        1,
        early_report(
            stage,
            &notes.join("; "),
            original,
            display,
            stops,
            helpers_absent,
            currents_restored,
            ghostty_ok,
            crate::mc_lifecycle::live::mc_group_present().ok(),
            reaped,
        ),
    )
}

/// Best-effort current restore through the proven MC AX goto (never a
/// replay of a synthetic leg, never a synthetic inverse).
fn recover_current(
    display: usize,
    original: u64,
    budget: &mut crate::mc_probe::ActionBudget,
    notes: &mut Vec<String>,
) {
    match crate::mc_lifecycle::live::fresh_snapshot() {
        Ok(after) => {
            if after.displays.get(display).and_then(|d| d.current) == Some(original) {
                notes.push(format!("recovery: already on original {original}; no goto"));
            } else if after.locate(original).is_some_and(|(di, _)| di == display) {
                match crate::mc_lifecycle::goto_sid_verified(
                    &after, display, original, budget, true,
                ) {
                    Ok((_, close)) => {
                        notes.push(format!("recovery: MC AX goto original {original} sent"));
                        if let Some(c) = close {
                            notes.push(format!("recovery close shortfall: {c}"));
                        }
                    }
                    Err(e) => notes.push(format!("recovery goto refused: {e}")),
                }
            } else {
                notes.push(format!(
                    "recovery: original {original} not uniquely on display {display}; leaving currents as read back"
                ));
            }
        }
        Err(e) => notes.push(format!("recovery snapshot unreadable ({e}); goto blocked")),
    }
    crate::mc_lifecycle::safe_close_mc(notes);
}
