#![forbid(unsafe_code)]

//! Bounded recovery for one previously adopted temp Space. Policy and
//! shapes live in [`crate::mc_probe`]; live dispatch, scans, and the
//! Mission Control traversal live in [`crate::mc_lifecycle`] (shared here).
//!
//! NEVER creates another Space: only `mc_goto_sid(original)` and the
//! conditional `mc_remove_sid(temp)` run. A dispatched-but-unverified press
//! is never replayed; failures report residue with the exact restoration
//! state and the protected evidence that names the mismatch.

use crate::mc_lifecycle::{
    capture_focus_legs, enumerate_window_members, fail_plain_inner, goto_sid_verified, live,
    remove_sid_verified, require_user_temp, safe_close_mc, snapshot_ghostty_windows,
    verify_space_empty,
};
use crate::mc_probe::{
    ActionBudget, FocusLegs, GhosttyWindow, LegLatency, MC_CLEANUP_RESERVE_SECS, McAction, McError,
    McErrorReport, McFailEvidence, McLayout, McRecoverReport, ResidualMember, error_json,
    focus_ghostty_returned, ghostty_stable_across_switch, removal_eligibility,
    validate_recover_topology,
};

/// Recovery run context: same companion/preimage discipline as the
/// lifecycle context but for one previously adopted temp. Failures report
/// residue with the exact restoration state plus the protected snapshots
/// and focus legs; nothing is replayed and nothing new is created.
struct RecoverCtx {
    caffeinate: live::CaffeinateGuard,
    layout_before: McLayout,
    temp: u64,
    budget: ActionBudget,
    remove_dispatched: bool,
    notes: Vec<String>,
    latencies: Vec<LegLatency>,
    ghostty_before: Vec<GhosttyWindow>,
    focus_before: FocusLegs,
}

impl RecoverCtx {
    fn lat(&mut self, leg: &str, t0: std::time::Instant) {
        self.latencies.push(LegLatency {
            leg: leg.to_owned(),
            ms: t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        });
    }

    /// Evidence captured for the error report: protected snapshots and
    /// focus legs from both sides, so a failed battery never drops the
    /// mismatch it observed.
    fn evidence(&self) -> McFailEvidence {
        (
            Some(self.ghostty_before.clone()),
            snapshot_ghostty_windows().ok(),
            Some(self.focus_before.clone()),
            Some(capture_focus_legs()),
        )
    }

    fn fail(
        &mut self,
        stage: &str,
        action: McAction,
        error: McError,
        currents_restored: bool,
    ) -> (i32, String) {
        self.notes.push(error.to_string());
        if self.remove_dispatched {
            self.notes.push(
                "a remove was already dispatched without verified effect; NOT replayed — manual residue"
                    .to_owned(),
            );
        }
        let reaped = self.caffeinate.stop();
        let (gb, ga, fb, fa) = self.evidence();
        error_json(&McErrorReport {
            stage: stage.to_owned(),
            action: action.as_str().to_owned(),
            error: self.notes.join("; "),
            temp_space: Some(self.temp),
            layout_before: Some(self.layout_before.clone()),
            layout_after: live::fresh_snapshot().ok().map(|s| s.sanitized()),
            currents_restored,
            mc_open_after: live::mc_group_present().ok(),
            caffeinate_reaped: reaped,
            ghostty_before: gb,
            ghostty_after: ga,
            focus_before: fb,
            focus_after: fa,
            notes: std::mem::take(&mut self.notes),
            latencies: std::mem::take(&mut self.latencies),
        })
    }
}

/// Original Space must read as a user Space (same bar as the temp: a
/// fullscreen/system original has no proven return target here); an
/// unreadable type fails closed.
fn require_user_original(original: u64) -> Result<(), McError> {
    match crate::ffi::sls_space_type(original) {
        Ok(crate::ffi::SlsSpaceType::User) => Ok(()),
        Ok(other) => Err(McError::Blocked(format!(
            "original Space {original} is not a user Space ({other:?}); refusing recovery without a proven return target — this is not a SIP verdict"
        ))),
        Err(cause) => Err(McError::Unsupported(format!(
            "original Space {original} type unreadable: {cause}"
        ))),
    }
}

/// Bounded recovery for one previously adopted temp: restore the original
/// current via the settled goto leg (poll only, never a replay), verify the
/// natural Ghostty return plus cross-Space Ghostty stability (PID/WID,
/// bounds, membership as sets — never `onscreen`, which legitimately flips
/// when the CURRENT Space moves), then read-only residual inspection. The
/// temp is removed ONLY when the pre-existing strict emptiness gate plus
/// eligibility plus exact removal-effect readback all pass; otherwise the
/// removal parks with residual members enumerated and nothing declared
/// clean. NEVER creates another Space: only `mc_goto_sid(original)` and
/// conditional `mc_remove_sid(temp)` run.
pub fn run_mc_recover(
    temp: u64,
    original: u64,
    seconds: u64,
    display: usize,
    allow_untested: bool,
) -> Result<String, (i32, String)> {
    crate::config::ProbeConfig::validate_probe_seconds(seconds).map_err(|e| (2, e))?;
    if !allow_untested {
        return Err((
            2,
            "mc-recover requires --allow-untested (visible Mission Control automation; recovery is observer-verified per run, never assumed)".to_owned(),
        ));
    }
    if seconds < crate::mc_probe::MC_MIN_LIFECYCLE_SECS {
        return Err((
            2,
            format!(
                "mc-recover needs --seconds >= {} ({}s cleanup reserve plus minimum work); refusing before any mutation",
                crate::mc_probe::MC_MIN_LIFECYCLE_SECS,
                MC_CLEANUP_RESERVE_SECS,
            ),
        ));
    }

    let t_gates = std::time::Instant::now();
    if let Err(e) = live::action_gates() {
        return Err(fail_plain_inner(
            "recover-gates",
            McAction::Gate,
            e,
            live::fresh_snapshot().ok().map(|s| s.sanitized()),
            live::mc_group_present().ok(),
            true,
            Vec::new(),
            Vec::new(),
        ));
    }
    let sep_before = crate::ffi::ns_separate_spaces();
    if sep_before.is_none() {
        return Err(fail_plain_inner(
            "recover-baseline-separate",
            McAction::Gate,
            McError::Unsupported("separate-Spaces state unreadable upfront; refusing".to_owned()),
            live::fresh_snapshot().ok().map(|s| s.sanitized()),
            live::mc_group_present().ok(),
            true,
            Vec::new(),
            Vec::new(),
        ));
    }
    let focus_before = capture_focus_legs();
    let ghostty_before = match snapshot_ghostty_windows() {
        Ok(g) => g,
        Err(e) => {
            return Err(fail_plain_inner(
                "recover-baseline-ghostty",
                McAction::Gate,
                e,
                live::fresh_snapshot().ok().map(|s| s.sanitized()),
                live::mc_group_present().ok(),
                true,
                Vec::new(),
                Vec::new(),
            ));
        }
    };
    let mut caffeinate = match live::CaffeinateGuard::spawn(seconds) {
        Ok(g) => g,
        Err(e) => {
            return Err(fail_plain_inner(
                "recover-caffeinate-spawn",
                McAction::Gate,
                e,
                live::fresh_snapshot().ok().map(|s| s.sanitized()),
                live::mc_group_present().ok(),
                false,
                Vec::new(),
                Vec::new(),
            ));
        }
    };
    let before = match live::fresh_snapshot() {
        Ok(s) => s,
        Err(e) => {
            let reaped = caffeinate.stop();
            return Err(fail_plain_inner(
                "recover-snapshot",
                McAction::Snapshot,
                e,
                None,
                live::mc_group_present().ok(),
                reaped,
                Vec::new(),
                Vec::new(),
            ));
        }
    };
    let mut ctx = RecoverCtx {
        caffeinate,
        layout_before: before.sanitized(),
        temp,
        budget: ActionBudget::new(seconds),
        remove_dispatched: false,
        notes: Vec::new(),
        latencies: Vec::new(),
        ghostty_before,
        focus_before,
    };
    ctx.lat("baselines", t_gates);
    // Strict adoption: exactly [original, temp] in SLS order on the display.
    if let Err(e) = validate_recover_topology(&before, display, original, temp) {
        return Err(ctx.fail("recover-topology", McAction::Snapshot, e, false));
    }
    if let Err(e) = require_user_original(original) {
        return Err(ctx.fail("recover-original-type", McAction::Snapshot, e, false));
    }
    if let Err(e) = require_user_temp(temp) {
        return Err(ctx.fail("recover-temp-type", McAction::Snapshot, e, false));
    }
    // MC must read absent before the goto leg; one safe close attempt,
    // then verdict — never dispatched through a visible overlay.
    match live::mc_group_present() {
        Ok(false) => ctx.notes.push("MC absent at recovery entry".to_owned()),
        Ok(true) => {
            safe_close_mc(&mut ctx.notes);
            if live::mc_group_present().unwrap_or(true) {
                return Err(ctx.fail(
                    "recover-mc",
                    McAction::Gate,
                    McError::Blocked(
                        "Mission Control present at recovery entry and the safe close did not clear it; refusing dispatch through an overlay"
                            .to_owned(),
                    ),
                    false,
                ));
            }
        }
        Err(e) => {
            return Err(ctx.fail("recover-mc", McAction::Gate, e, false));
        }
    }

    // Goto-original leg: the ONLY switch (never goto-temp, never create).
    // Already-there is a no-op readback, never a redundant press.
    let t = std::time::Instant::now();
    let goto_snapshot = if before.displays[display].current == Some(original) {
        ctx.notes.push(format!(
            "already on original Space {original}; no goto dispatch, readback only"
        ));
        before.clone()
    } else {
        match goto_sid_verified(&before, display, original, &mut ctx.budget, false) {
            Ok((s, close)) => {
                if let Some(c) = close {
                    ctx.notes.push(format!("recover-goto close shortfall: {c}"));
                }
                s
            }
            Err(e) => return Err(ctx.fail("recover-goto", McAction::GotoSpace, e, false)),
        }
    };
    ctx.lat("recover-goto", t);

    // Natural-return battery: Ghostty stable across the switch (PID/WID,
    // bounds, membership as sets — `onscreen` excluded, it legitimately
    // flips with the CURRENT Space), and the full focus oracle agrees on a
    // Ghostty return. Never a focus write here.
    let ghostty_after = match snapshot_ghostty_windows() {
        Ok(g) => g,
        Err(e) => return Err(ctx.fail("recover-ghostty", McAction::Snapshot, e, true)),
    };
    if !ghostty_stable_across_switch(&ctx.ghostty_before, &ghostty_after) {
        return Err(ctx.fail(
            "recover-ghostty",
            McAction::Snapshot,
            McError::Blocked(
                "Ghostty PID/WID, bounds, or membership changed across the recovery goto (onscreen excluded from this comparison); reporting, never writing"
                    .to_owned(),
            ),
            true,
        ));
    }
    let focus_after = capture_focus_legs();
    if !focus_ghostty_returned(&focus_after) {
        return Err(ctx.fail(
            "recover-focus",
            McAction::Snapshot,
            McError::Blocked(
                "post-goto focus lacks full Ghostty-return agreement (workspace+SLPS+readable direct on one Ghostty PID with a selected window); unknown is not restored"
                    .to_owned(),
            ),
            true,
        ));
    }
    let sep_after = crate::ffi::ns_separate_spaces();
    if sep_after != sep_before {
        return Err(ctx.fail(
            "recover-separate",
            McAction::Snapshot,
            McError::Blocked(format!(
                "separate-Spaces state changed (was {sep_before:?}, now {sep_after:?}); refusing success"
            )),
            true,
        ));
    }
    let mut mc_absent = !live::mc_group_present().unwrap_or(true);
    if !mc_absent {
        safe_close_mc(&mut ctx.notes);
        mc_absent = !live::mc_group_present().unwrap_or(true);
    }
    if !mc_absent {
        return Err(ctx.fail(
            "recover-mc-end",
            McAction::Snapshot,
            McError::Blocked(
                "Mission Control still present after the recovery goto; reporting, not claiming success"
                    .to_owned(),
            ),
            true,
        ));
    }

    // Read-only residual inspection: every live member of the temp, with
    // identity plus geometry plus membership. No titles, no content.
    let t = std::time::Instant::now();
    let live_members = match enumerate_window_members() {
        Ok(m) => m,
        Err(e) => return Err(ctx.fail("recover-inspect", McAction::Snapshot, e, true)),
    };
    let mut residual: Vec<ResidualMember> = live_members
        .into_iter()
        .filter(|m| matches!(m.spaces, Ok(ref s) if s.contains(&temp)))
        .map(|m| ResidualMember {
            wid: m.wid,
            pid: m.pid,
            bundle: m.bundle,
            layer: m.layer,
            onscreen: m.onscreen,
            x: m.x,
            y: m.y,
            w: m.w,
            h: m.h,
            spaces: m.spaces.ok(),
            exe: m.exe,
            exe_alias: m.exe_alias,
            ax_windows_absent: m.ax_windows_absent,
        })
        .collect();
    residual.sort_by_key(|m| m.wid);
    ctx.lat("inspect", t);
    ctx.notes.push(format!(
        "temp {temp} residual members after restore: {} ({})",
        residual.len(),
        residual
            .iter()
            .map(|m| format!("WID{} PID{} {}", m.wid, m.pid, m.bundle))
            .collect::<Vec<_>>()
            .join(", "),
    ));

    // Removal ONLY when the pre-existing strict gates already prove clear:
    // fresh eligibility on a fresh snapshot plus the unchanged emptiness
    // scan. Any block parks the temp with residue enumerated — never
    // weakened, never exempted, never declared clean.
    let t = std::time::Instant::now();
    let elig_snapshot = match live::fresh_snapshot() {
        Ok(s) => s,
        Err(e) => return Err(ctx.fail("recover-pre-remove", McAction::RemoveSpace, e, true)),
    };
    let blocked: Option<String> = match removal_eligibility(&elig_snapshot, temp) {
        Ok(_) => match verify_space_empty(temp) {
            Ok(scan) => {
                ctx.notes.push(format!(
                    "emptiness gate clear ({} candidates checked, {} shell skipped); dispatching one settled remove",
                    scan.checked, scan.shell_skipped
                ));
                match remove_sid_verified(temp, &mut ctx.budget, false, &mut ctx.remove_dispatched)
                {
                    Ok((_, close)) => {
                        if let Some(c) = close {
                            ctx.notes
                                .push(format!("recover-remove close shortfall: {c}"));
                        }
                        ctx.lat("remove", t);
                    }
                    Err(e) => {
                        return Err(ctx.fail("recover-remove", McAction::RemoveSpace, e, true));
                    }
                }
                None
            }
            Err(e) => Some(format!("cleanup parked — emptiness gate: {e}")),
        },
        Err(e) => Some(format!("cleanup parked — eligibility: {e}")),
    };
    // Re-resolve the emptiness counts for the report from a final read-only
    // scan (never a second dispatch decision): counts only when the gate
    // passed; parked reports carry None counts plus the blocking reason.
    let (empty_checked, empty_shell_skipped, removed) = match &blocked {
        None => {
            let after_remove = live::fresh_snapshot().ok();
            let still_there = after_remove
                .as_ref()
                .map(|s| s.all_sids().contains(&temp))
                .unwrap_or(true);
            if still_there {
                return Err(ctx.fail(
                    "recover-remove-readback",
                    McAction::RemoveSpace,
                    McError::Blocked(format!(
                        "remove dispatched but temp Space {temp} still present on readback; reporting residue, never replaying"
                    )),
                    true,
                ));
            }
            // Final scan counts for the record (read-only; the removal
            // already proved its own effect).
            match verify_space_empty(temp) {
                Ok(scan) => (Some(scan.checked), Some(scan.shell_skipped), true),
                Err(_) => (None, None, true),
            }
        }
        Some(_) => (None, None, false),
    };
    if let Some(reason) = &blocked {
        ctx.notes.push(reason.clone());
        ctx.lat("remove-parked", t);
    }

    let after = match live::fresh_snapshot() {
        Ok(s) => s,
        Err(e) => return Err(ctx.fail("recover-final", McAction::Snapshot, e, true)),
    };
    if removed
        && (after.displays.get(display).map(|d| &d.spaces) != Some(&vec![original])
            || after.displays[display].current != Some(original))
    {
        return Err(ctx.fail(
            "recover-final-topology",
            McAction::Snapshot,
            McError::Blocked(format!(
                "post-removal display {display} is not exactly [original={original}] current {original}; reporting residue"
            )),
            true,
        ));
    }
    let reaped = ctx.caffeinate.stop();
    let report = McRecoverReport {
        display,
        temp_space: temp,
        original_space: original,
        seconds,
        layout_before: ctx.layout_before.clone(),
        displays_before: before.ordered_layouts(),
        goto_layout: goto_snapshot.sanitized(),
        layout_after: after.sanitized(),
        displays_after: after.ordered_layouts(),
        currents_restored: after.displays.get(display).and_then(|d| d.current) == Some(original),
        separate_before: sep_before,
        separate_after: sep_after,
        mc_absent_at_end: mc_absent,
        focus_before: ctx.focus_before.clone(),
        focus_after,
        focus_ghostty_returned: true,
        ghostty_before: ctx.ghostty_before.clone(),
        ghostty_after,
        ghostty_equal: true,
        residual_members: residual,
        empty_checked,
        empty_shell_skipped,
        removal_blocked: blocked,
        removed,
        latencies: std::mem::take(&mut ctx.latencies),
        notes: std::mem::take(&mut ctx.notes),
        synthetic_events_posted: false,
        caffeinate_reaped: reaped,
    };
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}
