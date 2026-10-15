#![forbid(unsafe_code)]

//! Guarded public-activation-only restoration of one captured original-
//! frontmost Ghostty window (PID/WID on the original Space).
//!
//! Scope: exactly one explicit target — the recorded original frontmost
//! `(pid, wid)` whose live bundle must read exactly
//! [`RESTORE_BUNDLE`]. The ONLY write is a single
//! [`crate::ffi::activate_app`] call (public `NSRunningApplication`
//! activation, no SLPS events, no AX raise, no synthetic input, no window
//! setters, no keystrokes). Arrival is verified by the full selected oracle
//! ([`crate::focus::await_agreement`]); geometry/membership/exe stability is
//! re-read after. Any shortfall reports residue — never a retry, never a
//! second activation, never a broader target. This is NOT a general
//! arbitrary-window API: non-Ghostty bundles always refuse.
//!
//! A `caffeinate -d` companion ([`crate::mc_lifecycle::live`]) is held for
//! the whole run and reaped on every post-spawn path (single funnel in
//! [`run_mc_restore_frontmost`]); the report carries the actual
//! `caffeinate_reaped` value plus actual post-run session/AX-trust reads.

use serde::Serialize;

/// The only bundle this restoration will ever activate.
pub const RESTORE_BUNDLE: &str = "com.mitchellh.ghostty";
/// Bounded agreement wait after the single activation.
const AGREEMENT_SECS: u64 = 5;
/// Bounded display-sleep companion for the run (activation + readbacks).
const GUARD_SECS: u64 = 60;

/// Pure target gate: nonzero PID, nonzero WID, nonzero Space. Coupled by
/// shape — a missing half never parses (see CLI).
pub fn validate_restore_target(pid: u32, wid: u32, space: u64) -> Result<(u32, u32, u64), String> {
    if pid <= 1 {
        return Err(format!("refusing reserved pid {pid}"));
    }
    if wid == 0 {
        return Err("refusing window id 0".to_owned());
    }
    if space == 0 {
        return Err("refusing space id 0".to_owned());
    }
    Ok((pid, wid, space))
}

/// Pure opt-in gate: explicit `--allow-untested` per invocation, refused
/// before any mutation (guard spawn included).
pub fn require_allow_untested(allow: bool) -> Result<(), String> {
    if allow {
        Ok(())
    } else {
        Err("mc-restore-frontmost requires --allow-untested (one public activation of exactly the captured Ghostty PID/WID; refusing before any mutation)".to_owned())
    }
}

/// Pure bundle gate: exact match only (no substring, no fallback).
#[must_use]
pub fn bundle_is_restore_target(bundle: &str) -> bool {
    bundle == RESTORE_BUNDLE
}

/// Pure geometry preimage comparison over [`crate::mc_probe::GhosttyWindow`]:
/// PID/WID/layer/bounds/membership equal; `onscreen` excluded (visibility
/// legitimately flips with the current Space).
#[must_use]
pub fn ghostty_geometry_equal(
    a: &crate::mc_probe::GhosttyWindow,
    b: &crate::mc_probe::GhosttyWindow,
) -> bool {
    fn norm(spaces: &Option<Vec<u64>>) -> Option<Vec<u64>> {
        spaces.as_ref().map(|s| {
            let mut v = s.clone();
            v.sort_unstable();
            v.dedup();
            v
        })
    }
    a.wid == b.wid
        && a.pid == b.pid
        && a.layer == b.layer
        && a.x == b.x
        && a.y == b.y
        && a.w == b.w
        && a.h == b.h
        && norm(&a.spaces) == norm(&b.spaces)
}

/// Terminal outcome of one guarded restoration attempt. `session_unlocked`
/// and `ax_trusted` are actual post-run reads (never assumed); `error` is
/// the residue reason (`None` only on clean).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontmostRestoreReport {
    pub pid: u32,
    pub wid: u32,
    pub space: u64,
    pub display: usize,
    pub bundle: String,
    pub exe_stable: bool,
    pub activation_sent: bool,
    pub agreement: bool,
    pub frame_unchanged: bool,
    pub spaces_unchanged: bool,
    pub currents_held: bool,
    pub session_unlocked: bool,
    pub ax_trusted: bool,
    pub caffeinate_reaped: bool,
    pub synthetic_events_posted: bool,
    pub error: Option<String>,
    pub notes: Vec<String>,
}

/// Private run evidence: the report minus `caffeinate_reaped` (filled by
/// the outer funnel after the single guard stop).
struct Inner {
    report: FrontmostRestoreReport,
}

/// Refusal before any activation: report-shaped, `activation_sent: false`.
fn refuse(
    pid: u32,
    wid: u32,
    space: u64,
    display: usize,
    notes: Vec<String>,
    reason: String,
) -> Inner {
    Inner {
        report: FrontmostRestoreReport {
            pid,
            wid,
            space,
            display,
            bundle: String::new(),
            exe_stable: false,
            activation_sent: false,
            agreement: false,
            frame_unchanged: false,
            spaces_unchanged: false,
            currents_held: false,
            session_unlocked: false,
            ax_trusted: false,
            caffeinate_reaped: false,
            synthetic_events_posted: false,
            error: Some(reason),
            notes,
        },
    }
}

/// Guarded restoration: verify everything live, send ONE public activation,
/// verify the full selected-oracle agreement plus geometry stability.
/// Read-only except the single [`crate::ffi::activate_app`] call. The guard
/// is spawned before this runs and stopped after it returns, so every path
/// below is reaped by construction.
fn run_inner(pid: u32, wid: u32, space: u64, display: usize) -> Inner {
    let mut notes: Vec<String> = Vec::new();
    let refused =
        |notes: Vec<String>, reason: String| refuse(pid, wid, space, display, notes, reason);

    // Actual lock + AX trust first (oracle readback needs both).
    let session = crate::ffi::session_state();
    if !session.unlocked() {
        return refused(
            notes,
            format!("restore refused: session not unlocked ({:?})", session.lock),
        );
    }
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return refused(
            notes,
            "restore refused: AX trust not observed true".to_owned(),
        );
    }

    // Exact live bundle identity (Ghostty only — never a general target).
    match crate::ffi::bundle_id_for_pid(pid as i32).filter(|b| !b.is_empty()) {
        Some(live) if bundle_is_restore_target(&live) => {}
        Some(live) => {
            return refused(
                notes,
                format!(
                    "restore refused: live bundle '{live}' != '{RESTORE_BUNDLE}' (possible PID reuse)"
                ),
            );
        }
        None => {
            return refused(
                notes,
                format!("restore refused: pid {pid} has no live bundle"),
            );
        }
    }
    let exe_before = match crate::ffi::process_executable_path(pid as i32) {
        Some(exe) => exe,
        None => {
            return refused(
                notes,
                format!("restore refused: pid {pid} executable path unreadable"),
            );
        }
    };
    // Fresh CG owner join + exact single-Space membership.
    if let Err(e) = crate::cg::verify_wid_owned_by_pid(pid, wid) {
        return refused(notes, format!("restore refused: CG join lost: {e}"));
    }
    let mem_before = match crate::spaces::window_spaces(wid) {
        Ok(m) => m,
        Err(e) => {
            return refused(
                notes,
                format!("restore refused: membership unreadable: {e}"),
            );
        }
    };
    if !crate::space_recovery::membership_is_exact(&mem_before, space) {
        return refused(
            notes,
            format!("restore refused: WID{wid} membership {mem_before:?} != [{space}]"),
        );
    }
    // Current-original guard: the space must be current on its display.
    match crate::mc_lifecycle::live::fresh_snapshot() {
        Ok(snap) => match snap.locate(space) {
            Some((di, _))
                if di == display
                    && snap.displays.get(display).and_then(|d| d.current) == Some(space) => {}
            other => {
                return refused(
                    notes,
                    format!(
                        "restore refused: Space {space} not current on display {display} (locate {other:?})"
                    ),
                );
            }
        },
        Err(e) => {
            return refused(notes, format!("restore refused: topology unreadable: {e}"));
        }
    }
    // Geometry preimage (read-only Ghostty snapshot).
    let pre = match crate::mc_lifecycle::snapshot_ghostty_windows() {
        Ok(ghost) => match ghost.iter().find(|g| g.wid == wid && g.pid == pid).cloned() {
            Some(p) => p,
            None => {
                return refused(
                    notes,
                    format!("restore refused: WID{wid} PID{pid} absent from Ghostty snapshot"),
                );
            }
        },
        Err(e) => {
            return refused(
                notes,
                format!("restore refused: Ghostty snapshot unreadable: {e}"),
            );
        }
    };
    notes.push(format!(
        "preimage WID{wid} PID{pid} {}x{}@{},{}, spaces {:?}",
        pre.w, pre.h, pre.x, pre.y, pre.spaces
    ));

    // Session recheck immediately pre-activation: the lock may have changed
    // during the reads above; a lock here sends nothing.
    let pre_session = crate::ffi::session_state();
    notes.push(format!(
        "pre-activation session lock={:?}",
        pre_session.lock
    ));
    if !pre_session.unlocked() {
        return refused(
            notes,
            format!(
                "restore refused: session locked at pre-activation recheck ({:?}); no activation sent",
                pre_session.lock
            ),
        );
    }

    // THE single write: public app activation only.
    if !crate::ffi::activate_app(pid as i32) {
        return refused(
            notes,
            "activation request refused by the OS; nothing else sent, no retry".to_owned(),
        );
    }
    // Bounded full selected-oracle agreement readback.
    let agreed =
        crate::focus::await_agreement(pid, wid, std::time::Duration::from_secs(AGREEMENT_SECS))
            .is_ok();
    // Post readbacks (reads only): identity, geometry, currents, session,
    // AX trust — actual values, never assumed.
    let bundle_after = crate::ffi::bundle_id_for_pid(pid as i32).unwrap_or_default();
    let exe_after = crate::ffi::process_executable_path(pid as i32).unwrap_or_default();
    let exe_stable = !exe_after.is_empty() && exe_after == exe_before;
    let cg_after_ok = crate::cg::verify_wid_owned_by_pid(pid, wid).is_ok();
    let spaces_unchanged = crate::spaces::window_spaces(wid)
        .ok()
        .is_some_and(|m| crate::space_recovery::membership_is_exact(&m, space));
    let frame_unchanged = crate::mc_lifecycle::snapshot_ghostty_windows()
        .ok()
        .and_then(|v| v.into_iter().find(|g| g.wid == wid && g.pid == pid))
        .is_some_and(|post| ghostty_geometry_equal(&pre, &post));
    let currents_held = crate::mc_lifecycle::live::fresh_snapshot()
        .ok()
        .and_then(|s| s.displays.get(display).and_then(|d| d.current))
        == Some(space);
    let session_post = crate::ffi::session_state();
    let session_post_unlocked = session_post.unlocked();
    let ax_post = crate::ffi::ax_is_process_trusted();
    notes.push(format!(
        "post session lock={:?} ax_trust={ax_post:?}",
        session_post.lock
    ));
    let error = if agreed
        && bundle_is_restore_target(&bundle_after)
        && exe_stable
        && cg_after_ok
        && frame_unchanged
        && spaces_unchanged
        && currents_held
        && session_post_unlocked
        && ax_post == Some(true)
    {
        None
    } else {
        Some(format!(
            "restore residue: sent=true agreed={agreed} bundle_ok={} exe_stable={exe_stable} cg_ok={cg_after_ok} frame_ok={frame_unchanged} spaces_ok={spaces_unchanged} current_ok={currents_held} session_ok={session_post_unlocked} ax_ok={}",
            bundle_is_restore_target(&bundle_after),
            ax_post == Some(true),
        ))
    };
    Inner {
        report: FrontmostRestoreReport {
            pid,
            wid,
            space,
            display,
            bundle: bundle_after,
            exe_stable,
            activation_sent: true,
            agreement: agreed,
            frame_unchanged,
            spaces_unchanged,
            currents_held,
            session_unlocked: session_post_unlocked,
            ax_trusted: ax_post == Some(true),
            caffeinate_reaped: false,
            synthetic_events_posted: false,
            error,
            notes,
        },
    }
}

/// Live entry: pure gates first (no side effects), explicit opt-in, then
/// the held-guard run. The guard is stopped exactly once on every
/// post-spawn path; the report carries the actual `caffeinate_reaped`.
pub fn run_mc_restore_frontmost(
    pid: u32,
    wid: u32,
    space: u64,
    display: usize,
    allow_untested: bool,
) -> Result<String, (i32, String)> {
    let (pid, wid, space) = validate_restore_target(pid, wid, space).map_err(|e| (2, e))?;
    require_allow_untested(allow_untested).map_err(|e| (2, e))?;
    let mut guard = match crate::mc_lifecycle::live::CaffeinateGuard::spawn(GUARD_SECS) {
        Ok(g) => g,
        Err(e) => {
            return Err((
                1,
                serde_json::json!({
                    "outcome": "restore-residue",
                    "pid": pid, "wid": wid, "space": space, "display": display,
                    "activation_sent": false,
                    "caffeinate_reaped": false,
                    "synthetic_events_posted": false,
                    "error": format!("caffeinate spawn failed: {e}"),
                })
                .to_string(),
            ));
        }
    };
    let mut inner = run_inner(pid, wid, space, display);
    let reaped = guard.stop();
    inner.report.caffeinate_reaped = reaped;
    if !reaped {
        inner.report.error = Some(match inner.report.error.take() {
            Some(error) => format!("{error}; caffeinate child exit/reap unverified"),
            None => "caffeinate child exit/reap unverified".to_owned(),
        });
    }
    let mut body = serde_json::to_value(&inner.report).map_err(|e| (1, e.to_string()))?;
    let clean = inner.report.error.is_none();
    body["outcome"] = serde_json::Value::String(if clean {
        "restore-complete".to_owned()
    } else {
        "restore-residue".to_owned()
    });
    if clean {
        Ok(body.to_string())
    } else if inner.report.activation_sent {
        Err((1, body.to_string()))
    } else {
        Err((2, body.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_gate_couples_and_refuses_reserved() {
        assert!(validate_restore_target(42, 7, 1).is_ok());
        assert!(validate_restore_target(1, 7, 1).is_err());
        assert!(validate_restore_target(42, 0, 1).is_err());
        assert!(validate_restore_target(42, 7, 0).is_err());
    }

    #[test]
    fn opt_in_refuses_missing_before_any_mutation() {
        assert!(require_allow_untested(true).is_ok());
        assert!(require_allow_untested(false).is_err());
    }

    #[test]
    fn bundle_gate_is_exact_only() {
        assert!(bundle_is_restore_target("com.mitchellh.ghostty"));
        assert!(!bundle_is_restore_target("com.mitchellh.ghostty.helper"));
        assert!(!bundle_is_restore_target("GHOSTTY"));
        assert!(!bundle_is_restore_target(""));
    }

    fn window(wid: u32, onscreen: bool) -> crate::mc_probe::GhosttyWindow {
        crate::mc_probe::GhosttyWindow {
            wid,
            pid: 42,
            layer: 0,
            onscreen,
            x: 100.0,
            y: 100.0,
            w: 800.0,
            h: 600.0,
            spaces: Some(vec![10]),
        }
    }

    #[test]
    fn geometry_ignores_onscreen_but_not_bounds_or_membership() {
        assert!(ghostty_geometry_equal(&window(7, true), &window(7, false)));
        let mut moved = window(7, true);
        moved.x = 101.0;
        assert!(!ghostty_geometry_equal(&window(7, true), &moved));
        let mut rehomed = window(7, true);
        rehomed.spaces = Some(vec![20]);
        assert!(!ghostty_geometry_equal(&window(7, true), &rehomed));
    }
}
