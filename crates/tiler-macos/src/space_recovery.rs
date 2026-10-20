#![forbid(unsafe_code)]

//! Bounded owned public Space-recovery battery: O→T→O with no SLS writes.
//!
//! One command owns the whole roundtrip: it spawns a regular-policy helper
//! with the `--space-join` self-action poll (exact held `Child` + per-run
//! nonce, finite lifetime), captures read-only baselines (topology, owned
//! membership + frame, Ghostty snapshot, focus legs), switches current
//! O→T→O through the existing Mission Control goto legs (never a replay),
//! triggers the helper's public `MoveToActiveSpace` self-action once per leg
//! through the same [`crate::space_join`] library the standalone
//! `owned-space-join` command uses, and proves each leg with exact
//! single-Space SLS membership plus exact behavior-restore evidence (and an
//! exact frame match on the return leg). No Space is created or destroyed,
//! no private SLS move is attempted on any path, no synthetic events are
//! posted, and Ghostty is read-only throughout (no focus writes to it — a
//! missing natural return is reported, never forced).
//!
//! Failure at any point after the spawn funnels through structured cleanup:
//! best-effort goto back to the original current, MC close, caffeinate stop
//! (reaped last), exact-Child helper stop, then post-cleanup verification
//! (currents, helper absence, Ghostty stability, focus legs) inside the
//! error report with residue named. A dispatched-but-unverified goto is
//! never replayed; the trigger is consumed once per action by the helper.
//!
//! Holder authentication: this battery owns the exact unreaped helper Child,
//! so each leg authenticates through the held-Child entrypoint
//! ([`crate::space_join::prove_held_child`]: live handle, expected exe,
//! fresh CG owner join — no window-title read, which cannot observe an
//! off-current window). The standalone `owned-space-join` command keeps the
//! cross-command title proof and is unaffected. Holder exit is NOT
//! independence: the gated `--probe-direct` experiment runs the direct move
//! in a separate short-lived mover child while this holder still owns the
//! Child ([`crate::space_direct_probe`]), then proves recovery with the
//! public legs after the mover exits.
//!
//! Prior art (local checkouts only; pinned upstream URLs): Mission Control
//! traversal follows Hammerspoon's public AX shape (`mc` → `mc.display` on
//! `AXDisplayID` → `mc.spaces.list[i]` press, 0.3 s stabilize, 2 s group
//! poll) with the private toggle replaced by a public app launch:
//! https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L84-L152
//! Per-window SLS membership reads use the pinned yabai shape:
//! https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705
//! The public `MoveToActiveSpace` self-action itself is new fixture
//! behavior (no surveyed non-SA mover uses it as a recovery); each leg's
//! effect is proven by exact membership readback, never the flag name.

use serde::Serialize;

use crate::mc_probe::{
    FocusLegs, GhosttyWindow, LegLatency, McError, TopologySnapshot, focus_ghostty_returned,
    focus_restored, ghostty_stable_across_switch,
};

/// Minimum run budget: two goto legs plus two bounded self-action polls
/// plus the 15 s cleanup reserve must fit; smaller budgets refuse upfront
/// instead of starting unrestorable.
pub const MIN_RECOVERY_SECS: u64 = 60;
/// Maximum run budget: the helper lifetime cap (`MAX_PROBE_SECONDS`).
pub const MAX_RECOVERY_SECS: u64 = crate::config::MAX_PROBE_SECONDS;
/// Membership effect settle after each self-action: the AppKit move lands
/// asynchronously, so an immediate non-exact readback polls fresh (never a
/// re-trigger) within this window. Parity with the MC effect settle.
pub const MEMBERSHIP_SETTLE_MS: u64 = crate::mc_probe::MC_EFFECT_SETTLE_MS;
/// Membership poll slice.
pub const MEMBERSHIP_POLL_MS: u64 = 200;

/// Pure argument gate: explicit `--allow-untested` opt-in per invocation,
/// two distinct nonzero SIDs, and a budget inside `60..=120` (the helper
/// lifetime cap). No live touch — unit-tested.
pub fn validate_recovery_args(
    original: u64,
    temp: u64,
    seconds: u64,
    allow_untested: bool,
) -> Result<u64, String> {
    if !allow_untested {
        return Err(
            "spaces-owned-recovery requires --allow-untested (visible Mission Control automation plus owned self-activation; recovery is observer-verified per run, never assumed)"
                .to_owned(),
        );
    }
    if original == 0 || temp == 0 {
        return Err("spaces-owned-recovery needs nonzero --original and --temp SIDs".to_owned());
    }
    if original == temp {
        return Err("spaces-owned-recovery needs distinct --original and --temp SIDs".to_owned());
    }
    crate::config::ProbeConfig::validate_probe_seconds(seconds)?;
    if seconds < MIN_RECOVERY_SECS {
        return Err(format!(
            "spaces-owned-recovery needs --seconds >= {MIN_RECOVERY_SECS} (two goto legs plus two bounded self-action polls plus the 15s cleanup reserve); refusing before any mutation"
        ));
    }
    Ok(seconds)
}

/// Pure membership goal: exactly the one target Space — merely containing
/// the target while also still on the preimage is NOT success.
pub fn membership_is_exact(spaces: &[u64], goal: u64) -> bool {
    spaces.len() == 1 && spaces[0] == goal
}

/// Pure settle poll over a scripted membership reader: first exact `[goal]`
/// wins; `None` (read error or persistent mismatch) at the deadline.
/// Tests pass scripted closures; live code passes an SLS reader.
pub fn await_membership_exact(
    read: &mut dyn FnMut() -> Option<Vec<u64>>,
    goal: u64,
    deadline_ms: u64,
    poll_ms: u64,
) -> Option<Vec<u64>> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(deadline_ms);
    loop {
        if let Some(spaces) = read()
            && membership_is_exact(&spaces, goal)
        {
            return Some(spaces);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_ms.min(500)));
    }
}

#[derive(Debug, Clone, Serialize)]
struct RecoveryLeg {
    leg: String,
    goto_ms: u64,
    current_after_goto: Option<u64>,
    membership: Vec<u64>,
    membership_exact: bool,
    preimage_bits: u64,
    restored_bits: u64,
    restored_verified: bool,
    frame_after_ax: Option<crate::ffi::AxFrame>,
    frame_matches_baseline: Option<bool>,
    self_action_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct SpacesOwnedRecoveryReport {
    outcome: String,
    original: u64,
    temp: u64,
    display: usize,
    pid: u32,
    wid: u32,
    legs: Vec<RecoveryLeg>,
    baseline_frame_ax: Option<crate::ffi::AxFrame>,
    /// Present only with `--probe-direct`: the gated direct-SLS experiment
    /// after O→T→O (separate mover child, delayed readbacks, independent
    /// public restore). Absent (`None`) preserves the default battery shape.
    direct_probe: Option<crate::space_direct_probe::DirectProbeReport>,
    /// Present only with `--probe-bridged`: the gated bridged-SLS experiment
    /// after O->T->O (same harness shape, `bridged` route). Never set together
    /// with `direct_probe` (exactly one probe route per run).
    bridged_probe: Option<crate::space_direct_probe::DirectProbeReport>,
    currents_restored: bool,
    helper_stopped_exact: bool,
    helper_windows_absent: bool,
    ghostty_stable: bool,
    natural_focus_returned: bool,
    focus_matches_baseline: bool,
    mc_open_after: Option<bool>,
    caffeinate_reaped: bool,
    synthetic_events_posted: bool,
    live_evidence: String,
    notes: Vec<String>,
    latencies: Vec<LegLatency>,
}

#[derive(Debug, Clone, Serialize)]
struct RecoveryErrorReport {
    stage: String,
    action: String,
    error: String,
    original: u64,
    temp: u64,
    display: Option<usize>,
    pid: Option<u32>,
    currents_restored: bool,
    helper_stopped_exact: bool,
    helper_windows_absent: Option<bool>,
    ghostty_stable: Option<bool>,
    natural_focus_returned: Option<bool>,
    mc_open_after: Option<bool>,
    caffeinate_reaped: bool,
    ghostty_before: Option<Vec<GhosttyWindow>>,
    ghostty_after: Option<Vec<GhosttyWindow>>,
    focus_before: Option<FocusLegs>,
    focus_after: Option<FocusLegs>,
    synthetic_events_posted: bool,
    mutates: bool,
    notes: Vec<String>,
    latencies: Vec<LegLatency>,
}

/// Live run context: owns the caffeinate companion, the exact helper Child,
/// preimages, baselines, budget, and evidence accumulators. Post-spawn
/// failures funnel through [`RunCtx::fail`]: ordered current restore, MC
/// close, caffeinate stop (reaped last), exact-Child stop, then
/// post-cleanup verification inside the error report.
struct RunCtx {
    caffeinate: crate::mc_lifecycle::live::CaffeinateGuard,
    child: Option<std::process::Child>,
    pid: u32,
    helper_path: String,
    wid: u32,
    display: usize,
    original: u64,
    temp: u64,
    before: TopologySnapshot,
    baseline_frame_ax: Option<crate::ffi::AxFrame>,
    budget: crate::mc_probe::ActionBudget,
    /// Wall-clock helper deadline tracked at spawn (helper lifetime
    /// `seconds`): the gated direct probe refuses (no dispatch) when the
    /// remaining lifetime cannot cover its worst case. Tracked here rather
    /// than through `ActionBudget` so no core budget semantics change.
    helper_deadline: std::time::Instant,
    notes: Vec<String>,
    latencies: Vec<LegLatency>,
    ghostty_before: Vec<GhosttyWindow>,
    focus_before: FocusLegs,
}

impl RunCtx {
    fn lat(&mut self, leg: &str, t0: std::time::Instant) {
        self.latencies.push(LegLatency {
            leg: leg.to_owned(),
            ms: t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        });
    }

    fn error_report(
        &mut self,
        stage: &str,
        error: McError,
        currents_restored: bool,
        helper_stopped: bool,
    ) -> (i32, String) {
        self.notes.push(error.to_string());
        let reaped = self.caffeinate.stop();
        let ghostty_after = crate::mc_lifecycle::snapshot_ghostty_windows().ok();
        let focus_after = crate::mc_lifecycle::capture_focus_legs();
        let ghostty_stable = ghostty_after
            .as_ref()
            .map(|after| ghostty_stable_across_switch(&self.ghostty_before, after));
        let helper_absent = if helper_stopped {
            Some(
                !crate::ffi::process_exists(self.pid as i32)
                    && crate::ffi::cg_window_list_all()
                        .map(|list| list.iter().all(|w| w.pid != self.pid))
                        .unwrap_or(false),
            )
        } else {
            None
        };
        let body = RecoveryErrorReport {
            stage: stage.to_owned(),
            action: "spaces-owned-recovery".to_owned(),
            error: self.notes.join("; "),
            original: self.original,
            temp: self.temp,
            display: Some(self.display),
            pid: Some(self.pid),
            currents_restored,
            helper_stopped_exact: helper_stopped,
            helper_windows_absent: helper_absent,
            ghostty_stable,
            natural_focus_returned: Some(focus_ghostty_returned(&focus_after)),
            mc_open_after: crate::mc_lifecycle::live::mc_group_present().ok(),
            caffeinate_reaped: reaped,
            ghostty_before: Some(self.ghostty_before.clone()),
            ghostty_after,
            focus_before: Some(self.focus_before.clone()),
            focus_after: Some(focus_after),
            synthetic_events_posted: false,
            mutates: true,
            notes: std::mem::take(&mut self.notes),
            latencies: std::mem::take(&mut self.latencies),
        };
        (
            1,
            serde_json::to_string(&body).unwrap_or_else(|_| self.notes.join("; ")),
        )
    }

    /// Post-failure cleanup with ownership certainty, then the structured
    /// error report. The current is restored to the original first (cleanup
    /// budget, never replayed), MC is closed where safely possible,
    /// caffeinate stops last, and the exact helper Child is stopped (never
    /// a PID guess, never a broad kill). No SLS writes on any path.
    fn fail(&mut self, stage: &str, error: McError) -> (i32, String) {
        self.notes.push(error.to_string());
        let mut currents_restored = false;
        match crate::mc_lifecycle::live::fresh_snapshot() {
            Ok(after) => {
                if after.displays.get(self.display).and_then(|d| d.current) == Some(self.original) {
                    currents_restored = true;
                } else if after
                    .locate(self.original)
                    .is_some_and(|(di, _)| di == self.display)
                {
                    let t0 = std::time::Instant::now();
                    match crate::mc_lifecycle::goto_sid_verified(
                        &after,
                        self.display,
                        self.original,
                        &mut self.budget,
                        true,
                    ) {
                        Ok((s, close)) => {
                            self.lat("cleanup-goto-original", t0);
                            currents_restored =
                                s.displays.get(self.display).and_then(|d| d.current)
                                    == Some(self.original);
                            if let Some(c) = close {
                                self.notes
                                    .push(format!("cleanup goto close shortfall: {c}"));
                            }
                        }
                        Err(e) => self.notes.push(format!("cleanup goto original: {e}")),
                    }
                } else {
                    self.notes.push(format!(
                        "original current {} not uniquely located on display {}; leaving currents as read back",
                        self.original, self.display
                    ));
                }
            }
            Err(e) => self.notes.push(format!(
                "cleanup snapshot unreadable ({e}); ownership uncertain — goto blocked"
            )),
        }
        crate::mc_lifecycle::safe_close_mc(&mut self.notes);
        let helper_stopped = match self.child.as_mut() {
            Some(child) => {
                let stop = crate::session::stop_exact_child(child, self.pid);
                self.notes.push(format!(
                    "helper exact-Child stop: exited={} reaped={} record_removed={}",
                    stop.exited, stop.reaped, stop.record_removed
                ));
                if let Some(e) = stop.wait_error {
                    self.notes.push(format!("helper stop wait state: {e}"));
                }
                stop.exited
            }
            None => {
                self.notes
                    .push("helper Child already taken; no signal sent".to_owned());
                false
            }
        };
        self.error_report(
            stage,
            McError::Blocked("cleanup complete".to_owned()),
            currents_restored,
            helper_stopped,
        )
    }
}

/// Live owned membership read over the held Child: every poll iteration
/// re-runs the holder verification routine (exact-handle liveness,
/// expected exe, fresh CG owner join — no title read, so this holds
/// off-current) and only then performs the plain SLS membership read for
/// the already-joined window ID. No foreign windows: the WID was joined to
/// the owned PID at baseline and re-joined on every poll.
pub(crate) fn poll_membership_held(
    child: &mut std::process::Child,
    pid: u32,
    helper_path: &str,
    wid: u32,
    goal: u64,
) -> Result<Vec<u64>, String> {
    let mut last: Result<Vec<u64>, String> = Err("membership unreadable at first read".to_owned());
    let mut read =
        || match crate::focus::recheck_holder_before_mutation(child, pid, helper_path, wid) {
            Ok(()) => match crate::spaces::window_spaces(wid) {
                Ok(spaces) => {
                    last = Ok(spaces.clone());
                    Some(spaces)
                }
                Err(e) => {
                    last = Err(e.to_string());
                    None
                }
            },
            Err(e) => {
                last = Err(format!("held-Child recheck refused: {e}"));
                None
            }
        };
    match await_membership_exact(&mut read, goal, MEMBERSHIP_SETTLE_MS, MEMBERSHIP_POLL_MS) {
        Some(spaces) => Ok(spaces),
        None => Err(match last {
            Ok(spaces) => format!(
                "membership stayed {spaces:?} within {MEMBERSHIP_SETTLE_MS}ms after a verified self-action (goal exact [{goal}]); restoring"
            ),
            Err(e) => format!("membership unreadable: {e}"),
        }),
    }
}

/// Run one self-action leg through the held-Child entrypoint of the shared
/// [`crate::space_join`] library (same exclusive publish + outcome-poll as
/// standalone `owned-space-join`, but authenticated by the exact held Child
/// instead of the cross-command title proof, which cannot observe an
/// off-current window). The proof is taken immediately before publishing;
/// exact behavior-restore is required. Returns behavior evidence + elapsed.
pub(crate) fn run_join_leg_held(
    child: &mut std::process::Child,
    pid: u32,
    nonce: &str,
    helper_path: &str,
    wid: u32,
) -> Result<(crate::space_join::JoinEvidence, u64), McError> {
    let t0 = std::time::Instant::now();
    let proof = crate::space_join::prove_held_child(child, pid, nonce, helper_path, wid)
        .map_err(|e| McError::Ownership(format!("join-leg held proof: {e}")))?;
    let request_id = crate::space_join::publish_join_trigger_held(&proof)
        .map_err(|e| McError::Blocked(format!("join-leg publish: {e}")))?;
    let our_body = crate::probe_target::space_join_request_body(nonce, &request_id)
        .map_err(|e| McError::Blocked(format!("join-leg body: {e}")))?;
    let evidence = crate::space_join::await_join_result(pid, nonce, &request_id, &our_body)
        .map_err(|e| McError::Blocked(format!("join-leg outcome: {e}")))?;
    if !evidence.restored_verified
        || !crate::probe_target::space_join_restored_ok(
            evidence.preimage_bits,
            evidence.restored_bits,
        )
    {
        return Err(McError::Blocked(format!(
            "join-leg behavior not exactly restored (preimage {} restored {} verified {}); restoring",
            evidence.preimage_bits, evidence.restored_bits, evidence.restored_verified
        )));
    }
    let ms = t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    Ok((evidence, ms))
}

/// Bounded owned public Space-recovery battery. Spawns one regular-policy
/// helper with the `--space-join` poll, proves O→T→O with exact membership
/// plus exact frame restoration, then stops exactly that helper. With
/// `probe_direct`, one additional gated direct-SLS move runs AFTER leg B on
/// the same helper (separate short-lived mover child, delayed readbacks,
/// independent public restore once the mover exits); with `probe_bridged`,
/// the same battery runs ONE gated bridged move instead. The two flags are
/// mutually exclusive. Exit
/// codes: 2 usage/validation (nothing mutated), 1 gate/effect/restoration
/// failure with structured evidence and residue. The default battery
/// (`probe_direct == false && probe_bridged == false`) never attempts a
/// private SLS move on any path.
pub fn run_spaces_owned_recovery(
    original: u64,
    temp: u64,
    seconds: u64,
    allow_untested: bool,
    probe_direct: bool,
    probe_bridged: bool,
) -> Result<String, (i32, String)> {
    validate_recovery_args(original, temp, seconds, allow_untested).map_err(|e| (2, e))?;
    crate::space_direct_probe::validate_probe_direct_args(probe_direct, allow_untested)
        .map_err(|e| (2, e))?;
    crate::space_direct_probe::validate_probe_bridged_args(probe_bridged, allow_untested)
        .map_err(|e| (2, e))?;
    crate::space_direct_probe::validate_probe_selection(probe_direct, probe_bridged)
        .map_err(|e| (2, e))?;

    if let Err(e) = crate::mc_lifecycle::live::action_gates() {
        return Err((
            1,
            serde_json::json!({"stage": "pre-gates", "action": "spaces-owned-recovery", "error": e.to_string()}).to_string(),
        ));
    }
    let sep_before = crate::ffi::ns_separate_spaces();
    let ghostty_before =
        crate::mc_lifecycle::snapshot_ghostty_windows().map_err(|e| {
            (
                1,
                serde_json::json!({"stage": "baseline-ghostty", "action": "spaces-owned-recovery", "error": e.to_string()}).to_string(),
            )
        })?;
    let focus_before = crate::mc_lifecycle::capture_focus_legs();
    let snap0 = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| {
        (
            1,
            serde_json::json!({"stage": "baseline-topology", "action": "spaces-owned-recovery", "error": e.to_string()}).to_string(),
        )
    })?;
    let (display, _) = snap0.locate(original).ok_or_else(|| {
        (
            1,
            format!("original Space {original} not uniquely located; ownership uncertain — refusing before any mutation"),
        )
    })?;
    snap0.locate(temp).ok_or_else(|| {
        (
            1,
            format!("temp Space {temp} not uniquely located; ownership uncertain — refusing before any mutation"),
        )
    })?;
    let temp_display = snap0.locate(temp).map(|(di, _)| di).unwrap_or(usize::MAX);
    if temp_display != display {
        return Err((
            1,
            format!(
                "cross-display recovery {original} -> {temp} refused: only same-display recovery is in plan — this is not a SIP failure"
            ),
        ));
    }
    if snap0
        .displays
        .get(display)
        .and_then(|d| d.current)
        .is_none()
    {
        return Err((
            1,
            "chosen display has no current Space in the preimage; refusing before any mutation"
                .to_owned(),
        ));
    }
    // Spawn the owned regular-policy helper with the self-action poll.
    // Exact held Child + per-run nonce; finite `seconds` lifetime.
    let helper_nonce = crate::probe_target::mint_nonce().map_err(|e| (1, e))?;
    let spec = crate::probe_target::ProbeTargetSpec::new(&helper_nonce, 0, 0, seconds)
        .map(|s| s.with_regular_policy(true).with_space_join(true))
        .map_err(|e| (2, e))?;
    // Fresh window-server lock verdict immediately pre-spawn
    // (`CGSessionCopyCurrentDictionary`, never the frontmost heuristic
    // alone): a locked or unreadable session refuses before any mutation.
    // The write boundary rechecks inside `spaces-move` itself.
    let spawn_lock = crate::ffi::session_state();
    if !spawn_lock.unlocked() {
        return Err((
            1,
            serde_json::json!({"stage": "pre-gates", "action": "spaces-owned-recovery", "error": format!("window-server session not unlocked ({:?}); refusing before any mutation", spawn_lock.lock)}).to_string(),
        ));
    }
    let mut raw = crate::probe_target::start_helper_process(&spec)
        .map_err(|e| (1, format!("helper spawn failed: {e}")))?;
    let bundle = crate::probe_target::await_handshake(&mut raw, &spec)
        .map_err(|e| (1, format!("helper spawn failed: {e}")))?;
    if let Err(e) = crate::probe_target::record_spawn(raw.pid, &spec, &raw.helper_path) {
        let _ = raw.child.kill();
        let _ = raw.child.wait();
        return Err((1, format!("helper spawn failed: {e}")));
    }
    // Hold the helper stdout pipe open for the run lifetime (a closed read
    // end would SIGPIPE-kill the helper right after `ready`); forward
    // helper lines verbatim so self-action markers stay observable.
    let crate::probe_target::UnreadySpawn {
        mut child,
        pid: helper_pid,
        helper_path: helper_canonical,
        lines,
        line_drops: _,
    } = raw;
    let _ = std::thread::Builder::new()
        .name("recovery-drain".to_owned())
        .spawn(move || {
            use std::io::Write as _;
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            while let Ok(line) = lines.recv() {
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
        });
    let _ = bundle;

    let mut ctx = RunCtx {
        caffeinate: crate::mc_lifecycle::live::CaffeinateGuard::spawn(seconds).map_err(|e| {
            let _ = child.kill();
            let _ = child.wait();
            (1, format!("caffeinate spawn failed: {e}"))
        })?,
        child: Some(child),
        pid: helper_pid,
        helper_path: helper_canonical,
        wid: 0,
        display,
        original,
        temp,
        before: snap0.clone(),
        baseline_frame_ax: None,
        budget: crate::mc_probe::ActionBudget::new(seconds),
        helper_deadline: std::time::Instant::now() + std::time::Duration::from_secs(seconds),
        notes: vec![format!(
            "helper spawned (regular-policy + space-join); separate-spaces={sep_before:?}"
        )],
        latencies: Vec::new(),
        ghostty_before,
        focus_before,
    };

    // Owned identity + baseline membership/frame BEFORE any mutation: the
    // helper must start exactly on the original Space.
    let owned = match crate::ax::authorize_owned_action(helper_pid, &helper_nonce) {
        Ok(o) => o,
        Err(e) => return Err(ctx.fail("baseline-identity", McError::Ownership(e.to_string()))),
    };
    if crate::ownership::is_protected_bundle(&owned.bundle_id) {
        return Err(ctx.fail(
            "baseline-identity",
            McError::Ownership("helper bound to a protected bundle; refusing".to_owned()),
        ));
    }
    let wid = match crate::focus::join_helper_wid(helper_pid) {
        Ok(w) => w,
        Err(e) => return Err(ctx.fail("baseline-join", McError::Ownership(e.to_string()))),
    };
    ctx.wid = wid;
    let baseline_members = match crate::spaces::window_spaces_owned(&owned, helper_pid, wid) {
        Ok(m) => m,
        Err(e) => return Err(ctx.fail("baseline-membership", McError::System(e.to_string()))),
    };
    if !membership_is_exact(&baseline_members, original) {
        return Err(ctx.fail(
            "baseline-membership",
            McError::Blocked(format!(
                "helper membership is {baseline_members:?}, not exactly [{original}]; refusing before any mutation"
            )),
        ));
    }
    let baseline_frame = match crate::ax::read_window_owned(&owned, helper_pid, wid) {
        Ok(r) => r,
        Err(e) => return Err(ctx.fail("baseline-frame", McError::System(e.to_string()))),
    };
    if baseline_frame.ax_frame.is_none() {
        return Err(ctx.fail(
            "baseline-frame",
            McError::System("helper AX frame unreadable in baseline; refusing".to_owned()),
        ));
    }
    ctx.baseline_frame_ax = baseline_frame.ax_frame;

    // Leg A: current O→T, public self-action join, exact [T].
    let t_goto_a = std::time::Instant::now();
    let snap_a =
        match crate::mc_lifecycle::goto_sid_verified(&snap0, display, temp, &mut ctx.budget, false)
        {
            Ok((s, close)) => {
                let goto_ms = t_goto_a.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                ctx.latencies.push(LegLatency {
                    leg: "goto-temp".to_owned(),
                    ms: goto_ms,
                });
                if let Some(c) = close {
                    ctx.notes.push(format!("goto-temp close shortfall: {c}"));
                }
                (s, goto_ms)
            }
            Err(e) => return Err(ctx.fail("goto-temp", e)),
        };
    let (snap_a, goto_ms_a) = snap_a;
    let current_a = snap_a.displays.get(display).and_then(|d| d.current);
    // Held-Child legs: the exact Child is lent out of the context for the
    // leg and restored before any context use, so cleanup always owns it.
    // Title proofs cannot observe the off-current helper; the held handle
    // (live + exe + fresh CG join, no title) is the authentication.
    // Locals avoid holding any context borrow across the leg.
    let helper_path = ctx.helper_path.clone();
    let mut child_a = ctx
        .child
        .take()
        .expect("harness holds the exact helper Child");
    let leg_a_join = run_join_leg_held(&mut child_a, helper_pid, &helper_nonce, &helper_path, wid);
    let leg_a_members = if leg_a_join.is_ok() {
        Some(poll_membership_held(
            &mut child_a,
            helper_pid,
            &helper_path,
            wid,
            temp,
        ))
    } else {
        None
    };
    ctx.child = Some(child_a);
    let (evidence_a, action_ms_a) = match leg_a_join {
        Ok(v) => v,
        Err(e) => return Err(ctx.fail("join-temp", e)),
    };
    ctx.latencies.push(LegLatency {
        leg: "join-temp".to_owned(),
        ms: action_ms_a,
    });
    let members_a =
        match leg_a_members.expect("membership polled only after a verified self-action") {
            Ok(m) => m,
            Err(e) => return Err(ctx.fail("membership-temp", McError::Blocked(e))),
        };
    let leg_a = RecoveryLeg {
        leg: "join-temp".to_owned(),
        goto_ms: goto_ms_a,
        current_after_goto: current_a,
        membership: members_a,
        membership_exact: true,
        preimage_bits: evidence_a.preimage_bits,
        restored_bits: evidence_a.restored_bits,
        restored_verified: evidence_a.restored_verified,
        frame_after_ax: None,
        frame_matches_baseline: None,
        self_action_ms: action_ms_a,
    };

    // Leg B: current T→O, public self-action join, exact [O] + exact frame.
    let t_goto_b = std::time::Instant::now();
    let snap_b = match crate::mc_lifecycle::goto_sid_verified(
        &snap_a,
        display,
        original,
        &mut ctx.budget,
        false,
    ) {
        Ok((s, close)) => {
            let goto_ms = t_goto_b.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            ctx.latencies.push(LegLatency {
                leg: "goto-original".to_owned(),
                ms: goto_ms,
            });
            if let Some(c) = close {
                ctx.notes
                    .push(format!("goto-original close shortfall: {c}"));
            }
            (s, goto_ms)
        }
        Err(e) => return Err(ctx.fail("goto-original", e)),
    };
    let (snap_b, goto_ms_b) = snap_b;
    let current_b = snap_b.displays.get(display).and_then(|d| d.current);
    let mut child_b = ctx
        .child
        .take()
        .expect("harness holds the exact helper Child");
    let leg_b_join = run_join_leg_held(&mut child_b, helper_pid, &helper_nonce, &helper_path, wid);
    let leg_b_members = if leg_b_join.is_ok() {
        Some(poll_membership_held(
            &mut child_b,
            helper_pid,
            &helper_path,
            wid,
            original,
        ))
    } else {
        None
    };
    ctx.child = Some(child_b);
    let (evidence_b, action_ms_b) = match leg_b_join {
        Ok(v) => v,
        Err(e) => return Err(ctx.fail("join-original", e)),
    };
    ctx.latencies.push(LegLatency {
        leg: "join-original".to_owned(),
        ms: action_ms_b,
    });
    let members_b =
        match leg_b_members.expect("membership polled only after a verified self-action") {
            Ok(m) => m,
            Err(e) => return Err(ctx.fail("membership-original", McError::Blocked(e))),
        };
    let frame_b = match crate::ax::read_window_owned(&owned, helper_pid, wid) {
        Ok(r) => r,
        Err(e) => return Err(ctx.fail("frame-original", McError::System(e.to_string()))),
    };
    let frame_ok = frame_b.ax_frame.is_some() && frame_b.ax_frame == ctx.baseline_frame_ax;
    if !frame_ok {
        ctx.notes.push(format!(
            "frame mismatch: baseline {:?} vs return {:?}",
            ctx.baseline_frame_ax, frame_b.ax_frame
        ));
        return Err(ctx.fail(
            "frame-original",
            McError::Blocked("helper frame not exactly restored on the return leg".to_owned()),
        ));
    }
    let leg_b = RecoveryLeg {
        leg: "join-original".to_owned(),
        goto_ms: goto_ms_b,
        current_after_goto: current_b,
        membership: members_b,
        membership_exact: true,
        preimage_bits: evidence_b.preimage_bits,
        restored_bits: evidence_b.restored_bits,
        restored_verified: evidence_b.restored_verified,
        frame_after_ax: frame_b.ax_frame,
        frame_matches_baseline: Some(true),
        self_action_ms: action_ms_b,
    };

    // Gated SLS experiment AFTER the public O->T->O is proven on this
    // same helper: one dispatch of the selected route in a separate
    // short-lived mover child (reaped boundedly inside), delayed harness
    // readbacks (max 3 s each), and the independent public self-restore
    // after the mover exits. The exact helper Child is lent out and restored
    // before any context use, so cleanup always owns it; failures funnel
    // through `fail` with the Child back in the context. No retries on any
    // path. At most one route runs: the selection gate above already refused
    // both flags together, so there is no fallback and no replay.
    let direct_probe = if probe_direct {
        let mut child_c = ctx
            .child
            .take()
            .expect("harness holds the exact helper Child");
        let probe_outcome = crate::space_direct_probe::run_direct_probe_after_recovery(
            &mut child_c,
            &owned,
            helper_pid,
            &helper_nonce,
            &helper_path,
            wid,
            ctx.baseline_frame_ax,
            original,
            temp,
            display,
            ctx.helper_deadline,
            &mut ctx.notes,
            &mut ctx.latencies,
        );
        ctx.child = Some(child_c);
        match probe_outcome {
            Ok(report) => Some(report),
            Err(e) => return Err(ctx.fail("direct-probe", e)),
        }
    } else {
        None
    };
    let bridged_probe = if probe_bridged {
        let mut child_c = ctx
            .child
            .take()
            .expect("harness holds the exact helper Child");
        let probe_outcome = crate::space_direct_probe::run_bridged_probe_after_recovery(
            &mut child_c,
            &owned,
            helper_pid,
            &helper_nonce,
            &helper_path,
            wid,
            ctx.baseline_frame_ax,
            original,
            temp,
            display,
            ctx.helper_deadline,
            &mut ctx.notes,
            &mut ctx.latencies,
        );
        ctx.child = Some(child_c);
        match probe_outcome {
            Ok(report) => Some(report),
            Err(e) => return Err(ctx.fail("bridged-probe", e)),
        }
    } else {
        None
    };

    // Success cleanup: MC close where safely possible, caffeinate reaped
    // last, exact-Child helper stop (never a PID guess), then verification.
    crate::mc_lifecycle::safe_close_mc(&mut ctx.notes);
    let reaped = ctx.caffeinate.stop();
    let helper_stopped = match ctx.child.as_mut() {
        Some(child) => {
            let stop = crate::session::stop_exact_child(child, helper_pid);
            ctx.notes.push(format!(
                "helper exact-Child stop: exited={} reaped={} record_removed={}",
                stop.exited, stop.reaped, stop.record_removed
            ));
            stop.exited
        }
        None => false,
    };
    if !helper_stopped {
        return Err(ctx.fail(
            "helper-stop",
            McError::System(
                "owned helper did not exit bounded; record retained for inspection".to_owned(),
            ),
        ));
    }
    let helper_absent = !crate::ffi::process_exists(helper_pid as i32)
        && crate::ffi::cg_window_list_all()
            .map(|list| list.iter().all(|w| w.pid != helper_pid))
            .unwrap_or(false);
    let snap_final = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| {
        ctx.fail(
            "verify-topology",
            McError::System(format!("final snapshot unreadable: {e}")),
        )
    })?;
    let currents_restored = snap_final.displays.len() == ctx.before.displays.len()
        && snap_final
            .displays
            .iter()
            .zip(ctx.before.displays.iter())
            .all(|(a, b)| a.current == b.current);
    let ghostty_after = crate::mc_lifecycle::snapshot_ghostty_windows().map_err(|e| {
        ctx.fail(
            "verify-ghostty",
            McError::System(format!("Ghostty snapshot unreadable: {e}")),
        )
    })?;
    let ghostty_ok = ghostty_stable_across_switch(&ctx.ghostty_before, &ghostty_after);
    if !ghostty_ok {
        ctx.notes
            .push("Ghostty snapshot differs across the battery".to_owned());
        return Err(ctx.fail(
            "verify-ghostty",
            McError::Blocked("Ghostty windows changed under a read-only battery".to_owned()),
        ));
    }
    if !helper_absent {
        return Err(ctx.fail(
            "verify-helper-absence",
            McError::System("helper process or windows linger after verified stop".to_owned()),
        ));
    }
    if !currents_restored {
        return Err(ctx.fail(
            "verify-currents",
            McError::Blocked(
                "per-display currents differ from the preimage after O->T->O".to_owned(),
            ),
        ));
    }
    let focus_after = crate::mc_lifecycle::capture_focus_legs();
    // Natural focus return is observed and reported, never forced: no
    // focus writes exist on any path here.
    let natural_return = focus_ghostty_returned(&focus_after);
    if !natural_return {
        ctx.notes.push(
            "no natural Ghostty focus return observed after helper stop (reported, not claimed)"
                .to_owned(),
        );
    }
    let report = SpacesOwnedRecoveryReport {
        outcome: "recovery-complete".to_owned(),
        original,
        temp,
        display,
        pid: helper_pid,
        wid,
        legs: vec![leg_a, leg_b],
        baseline_frame_ax: ctx.baseline_frame_ax,
        direct_probe,
        bridged_probe,
        currents_restored,
        helper_stopped_exact: helper_stopped,
        helper_windows_absent: helper_absent,
        ghostty_stable: ghostty_ok,
        natural_focus_returned: natural_return,
        focus_matches_baseline: focus_restored(&ctx.focus_before, &focus_after),
        mc_open_after: crate::mc_lifecycle::live::mc_group_present().ok(),
        caffeinate_reaped: reaped,
        synthetic_events_posted: false,
        live_evidence: "O->T->O proven by exact SLS membership [T] then [O], exact collectionBehavior preimage+restore on both self-actions, exact AX frame restoration, per-display current restoration, Ghostty read-only stability".to_owned(),
        notes: std::mem::take(&mut ctx.notes),
        latencies: std::mem::take(&mut ctx.latencies),
    };
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_args_require_opt_in_distinct_sids_and_bounded_budget() {
        // Opt-in missing refuses before any live touch.
        assert!(validate_recovery_args(1, 66, 120, false).is_err());
        // Zero or identical SIDs refuse.
        assert!(validate_recovery_args(0, 66, 120, true).is_err());
        assert!(validate_recovery_args(1, 0, 120, true).is_err());
        assert!(validate_recovery_args(1, 1, 120, true).is_err());
        // Budget below the 60 s floor or above the 120 s helper cap refuses.
        assert!(validate_recovery_args(1, 66, 45, true).is_err());
        assert!(validate_recovery_args(1, 66, 121, true).is_err());
        assert!(validate_recovery_args(1, 66, 0, true).is_err());
        // The brief's battery shape passes validation.
        assert_eq!(validate_recovery_args(1, 66, 120, true).unwrap(), 120);
        assert_eq!(validate_recovery_args(1, 66, 60, true).unwrap(), 60);
    }

    #[test]
    fn membership_goal_is_exact_single_space() {
        assert!(membership_is_exact(&[66], 66));
        // Empty, multi-Space (sticky), or wrong-Space memberships refuse.
        assert!(!membership_is_exact(&[], 66));
        assert!(!membership_is_exact(&[1, 66], 66));
        assert!(!membership_is_exact(&[1], 66));
        assert!(!membership_is_exact(&[66, 66], 66));
    }

    #[test]
    fn membership_settle_poll_finds_exact_and_times_out_mismatch() {
        // Immediate exact hit: first read wins, no sleep.
        let mut read = || Some(vec![66u64]);
        assert_eq!(
            await_membership_exact(&mut read, 66, 1_000, 50),
            Some(vec![66u64])
        );
        // Async settle: mismatch then exact still resolves (poll, never a
        // re-trigger — the self-action is never replayed here).
        let mut n = 0;
        let mut read = || {
            n += 1;
            Some(if n < 3 { vec![1u64] } else { vec![66u64] })
        };
        assert_eq!(
            await_membership_exact(&mut read, 66, 5_000, 1),
            Some(vec![66u64])
        );
        // Persistent mismatch and unreadable reads time out empty.
        let mut read = || Some(vec![1u64]);
        assert!(await_membership_exact(&mut read, 66, 5, 1).is_none());
        let mut read = || None;
        assert!(await_membership_exact(&mut read, 66, 5, 1).is_none());
    }

    #[test]
    fn frame_equality_is_exact_ax_value() {
        // The return leg requires the exact AX frame value back: identical
        // reads pass, any drift (even subpoint) fails with evidence.
        let a = crate::ffi::AxFrame {
            x: 200.0,
            y: 200.0,
            w: 480.0,
            h: 320.0,
        };
        let b = crate::ffi::AxFrame {
            x: 200.0,
            y: 200.0,
            w: 480.0,
            h: 320.0,
        };
        let c = crate::ffi::AxFrame {
            x: 200.5,
            y: 200.0,
            w: 480.0,
            h: 320.0,
        };
        assert!(Some(a) == Some(b));
        assert!(Some(a) != Some(c));
    }
}
