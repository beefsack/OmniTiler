#![forbid(unsafe_code)]

//! Bounded Mission Control lifecycle run. Policy and shapes live in
//! [`crate::mc_probe`]; this module owns live dispatch, scans, and cleanup.
//!
//! Leg contract per action: gates → fresh SLS join → budgeted toggle (gates
//! immediately pre-toggle) → stabilize → gates → fresh re-join with drift
//! rejection → AX tree join with exact list-count match → budgeted press with
//! gates immediately pre-press → effect readback → close attempt. A sent
//! press always yields an outcome carrying the post-press snapshot and MC
//! presence, even when the close fails, so a press is never lost to a close
//! error. Error paths close MC only when this leg opened it and gates still
//! pass. A dispatched-but-unverified remove is never replayed in cleanup
//! (indices shift under a missing Space); goto restoration re-targets by
//! fresh SID join, which is safe.

use crate::mc_probe::{
    ActionBudget, FocusLegs, GhosttyWindow, LegLatency, MC_CLEANUP_RESERVE_SECS,
    MC_EFFECT_SETTLE_MS, McAction, McError, McErrorReport, McFailEvidence, McInspectReport,
    McLayout, McLifecycleReport, TopologySnapshot, error_json, focus_restored, removal_eligibility,
    require_dock_bundle, validate_create_effect, validate_goto_effect, validate_removal_effect,
};
// macOS-only live dispatch names (used via `live` on macOS; off-mac the stub
// never touches them, so gate them to keep strict Clippy clean).
#[cfg(target_os = "macos")]
use crate::mc_probe::{
    DOCK_BUNDLE_ID, MC_ACTION_PRESS, MC_ACTION_REMOVE_DESKTOP, MC_ACTION_TIMEOUT_MS,
    MC_AX_TIMEOUT_SECS, MC_GROUP_POLL_MS, MC_ID_DISPLAY, MC_ID_ROOT, MC_ID_SPACES,
    MC_ID_SPACES_ADD, MC_ID_SPACES_LIST, MC_STABILIZE_MS, require_ax_trust,
    require_unlocked_session,
};
// Read-only observation helpers live in `mc_observe`; re-exported here so
// existing `mc_lifecycle::…` paths keep working with no interface change.
pub(crate) use crate::mc_observe::{
    capture_focus_legs, enumerate_window_members, snapshot_ghostty_windows, verify_space_empty,
};

/// Budget spend for one dispatch: main-phase legs spend inside the work
/// window; cleanup legs spend inside the reserve up to the deadline.
/// macOS-only live helper (called only from the macOS `live` module).
#[cfg(target_os = "macos")]
fn spend_leg(budget: &mut ActionBudget, cleanup: bool, what: &str) -> Result<(), McError> {
    if cleanup {
        budget.spend_cleanup(what)
    } else {
        budget.spend_main(what)
    }
}

/// Pinned display join for effect-thread selectors: the caller's
/// main-thread-published `(cgid, managed UUID)` verified against the
/// fresh SLS snapshot's exact in-row identifier - never a positional
/// guess, never an AppKit read, so this runs on any thread without
/// touching or blocking the main thread. Refuses a zero CG ID, an
/// unknown display, or an identifier mismatch (stale publication or a
/// display swap mid-leg). Returns (cgid, Space count).
#[cfg(any(target_os = "macos", test))]
pub(crate) fn display_cgid_of_pinned(
    snapshot: &TopologySnapshot,
    display: usize,
    pinned_cgid: u32,
    pinned_uuid: &str,
) -> Result<(u32, usize), McError> {
    let row = snapshot.displays.get(display).ok_or_else(|| {
        McError::Blocked(format!(
            "display {display} unknown ({} displays)",
            snapshot.displays.len()
        ))
    })?;
    if pinned_cgid == 0 {
        return Err(McError::Blocked(
            "pinned display has no CG display ID; refusing without an exact mapping".to_owned(),
        ));
    }
    match row.identifier.as_deref() {
        Some(id) if id == pinned_uuid => Ok((pinned_cgid, row.spaces.len())),
        _ => Err(McError::Blocked(
            "pinned display UUID no longer names the SLS row; refusing stale dispatch".to_owned(),
        )),
    }
}

#[cfg(target_os = "macos")]
pub(crate) mod live {
    use super::*;

    pub(crate) use super::display_cgid_of_pinned;

    /// Exact-handle `caffeinate -d -t <seconds>` companion, self-bounded via
    /// `-t`. Signals go only to the held handle; `wait()` runs only after an
    /// observed exit. `Drop` is a backstop. Stopped only after cleanup (or
    /// when no temp exists), never before it.
    pub struct CaffeinateGuard {
        child: Option<std::process::Child>,
        pub reaped: bool,
    }

    impl CaffeinateGuard {
        pub fn spawn(seconds: u64) -> Result<Self, McError> {
            let child = std::process::Command::new("caffeinate")
                .args(["-d", "-t", &seconds.to_string()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| McError::System(format!("caffeinate spawn failed: {e}")))?;
            Ok(Self {
                child: Some(child),
                reaped: false,
            })
        }

        pub fn stop(&mut self) -> bool {
            let Some(child) = self.child.as_mut() else {
                self.reaped = true;
                return true;
            };
            match child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => {
                    let _ = child.kill();
                    let deadline =
                        std::time::Instant::now() + std::time::Duration::from_millis(5_000);
                    loop {
                        match child.try_wait() {
                            Ok(Some(_)) => break,
                            Ok(None) => {
                                if std::time::Instant::now() >= deadline {
                                    return false;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            }
                            Err(_) => return false,
                        }
                    }
                }
                Err(_) => return false,
            }
            self.reaped = child.wait().is_ok();
            if self.reaped {
                // Cache the successful reap: a second stop on the already
                // reaped handle must report the cached truth, never flip it
                // by waiting on a reaped child.
                self.child = None;
            }
            self.reaped
        }
    }

    impl Drop for CaffeinateGuard {
        fn drop(&mut self) {
            let _ = self.stop();
        }
    }

    /// Live Dock PID: exactly one `com.apple.dock` entry (never guessed).
    pub fn dock_pid() -> Result<i32, McError> {
        let mut found = Vec::new();
        for (pid, bundle) in crate::ffi::running_apps() {
            if bundle == DOCK_BUNDLE_ID {
                found.push(pid);
            }
        }
        match found.len() {
            1 if found[0] > 1 => Ok(found[0]),
            1 => Err(McError::Ownership("refusing reserved Dock pid".to_owned())),
            0 => Err(McError::Unsupported(
                "no live com.apple.dock entry in the running-application table".to_owned(),
            )),
            _ => Err(McError::Blocked(format!(
                "ambiguous Dock identity ({} entries); refusing to guess",
                found.len()
            ))),
        }
    }

    /// Dock AX element with a FRESH exact bundle verification on every
    /// traversal (never the inspect-time PID alone), plus a finite messaging
    /// timeout before any read.
    fn dock_element(pid: i32) -> Result<crate::ffi::AxElement, McError> {
        let bundle = crate::ffi::bundle_id_for_pid(pid).unwrap_or_default();
        super::require_dock_bundle(&bundle)?;
        let element = crate::ffi::AxElement::application(pid)
            .ok_or_else(|| McError::System(format!("Dock AX element unavailable for pid {pid}")))?;
        let status = crate::ffi::ax_set_messaging_timeout(&element, MC_AX_TIMEOUT_SECS);
        if status != 0 {
            return Err(McError::System(format!(
                "Dock AX messaging timeout unset: {}",
                crate::ffi::ax_error_name(status)
            )));
        }
        Ok(element)
    }

    fn child_with_id(
        element: &crate::ffi::AxElement,
        want: &str,
    ) -> Result<Option<crate::ffi::AxElement>, McError> {
        let children = crate::ffi::ax_children(element).map_err(|code| {
            McError::System(format!(
                "AXChildren unreadable: {}",
                crate::ffi::ax_error_name(code)
            ))
        })?;
        for child in children {
            match crate::ffi::ax_identifier(&child) {
                Ok(Some(id)) if id == want => return Ok(Some(child)),
                Ok(_) => {}
                Err(code) => {
                    return Err(McError::System(format!(
                        "AXIdentifier unreadable: {}",
                        crate::ffi::ax_error_name(code)
                    )));
                }
            }
        }
        Ok(None)
    }

    /// Read-only MC-group presence (Dock → `AXIdentifier == "mc"`).
    pub fn mc_group_present() -> Result<bool, McError> {
        Ok(child_with_id(&dock_element(dock_pid()?)?, MC_ID_ROOT)?.is_some())
    }

    /// Shared bounded poll: first `Some` wins; `None` at the deadline.
    fn poll_until_ms<T>(
        timeout_ms: u64,
        slice_ms: u64,
        mut f: impl FnMut() -> Option<T>,
    ) -> Option<T> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            if let Some(v) = f() {
                return Some(v);
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(slice_ms));
        }
    }

    /// Bounded poll for presence to equal `want`. Mid-transition read
    /// failures are poll data; the deadline names the verdict.
    pub fn poll_group(want: bool) -> Result<bool, McError> {
        let mut last_err: Option<McError> = None;
        let hit = poll_until_ms(MC_GROUP_POLL_MS, 100, || match mc_group_present() {
            Ok(present) if present == want => Some(true),
            Ok(_) => None,
            Err(e) => {
                last_err = Some(e);
                None
            }
        });
        match (hit, last_err) {
            (Some(_), _) => Ok(want),
            (None, Some(e)) => Err(e),
            (None, None) => Err(McError::System(format!(
                "Mission Control presence did not become {want} within {MC_GROUP_POLL_MS}ms"
            ))),
        }
    }

    /// Open iff absent: presence read, gates immediately pre-toggle, one
    /// public app-launch toggle at most, bounded presence poll as proof.
    pub fn mc_open() -> Result<bool, McError> {
        if mc_group_present()? {
            return Ok(false);
        }
        action_gates()?;
        if !crate::ffi::mc_launch_toggle() {
            return Err(McError::System(
                "Mission Control launch not sent (NSWorkspace refused)".to_owned(),
            ));
        }
        poll_group(true)?;
        Ok(true)
    }

    /// Close iff present (goto auto-closes; already-closed is a no-op).
    pub fn mc_close() -> Result<bool, McError> {
        if !mc_group_present()? {
            return Ok(false);
        }
        action_gates()?;
        if !crate::ffi::mc_launch_toggle() {
            return Err(McError::System(
                "Mission Control close launch not sent (NSWorkspace refused)".to_owned(),
            ));
        }
        poll_group(false)?;
        Ok(true)
    }

    fn stabilize() {
        std::thread::sleep(std::time::Duration::from_millis(MC_STABILIZE_MS));
    }

    /// Fresh display join from one snapshot: SLS in-row identifier → CG
    /// display ID via the main-thread AppKit snapshot (exact UUID match).
    /// Returns (cgid, Space count).
    fn display_cgid_of(
        snapshot: &TopologySnapshot,
        display: usize,
    ) -> Result<(u32, usize), McError> {
        let row = snapshot.displays.get(display).ok_or_else(|| {
            McError::Blocked(format!(
                "display {display} unknown ({} displays)",
                snapshot.displays.len()
            ))
        })?;
        let want = row.identifier.as_deref().ok_or_else(|| {
            McError::Blocked(
                "chosen display has no managed UUID; refusing positional fallback".to_owned(),
            )
        })?;
        let screens = crate::ffi::ns_display_snapshot()
            .map_err(|e| McError::Unsupported(format!("display snapshot unavailable: {e}")))?;
        for screen in screens {
            if screen.uuid.as_deref() == Some(want) && screen.cg_id != 0 {
                return Ok((screen.cg_id, row.spaces.len()));
            }
        }
        Err(McError::Blocked(
            "no CG display ID joins the chosen managed display; refusing without an exact mapping"
                .to_owned(),
        ))
    }

    /// Fresh AX join of one display's `mc.spaces` group: Dock → `mc` →
    /// `mc.display` with `AXDisplayID == cgid` → `mc.spaces`.
    fn spaces_group(cgid: u32) -> Result<crate::ffi::AxElement, McError> {
        let dock = dock_element(dock_pid()?)?;
        let mc = child_with_id(&dock, MC_ID_ROOT)?.ok_or_else(|| {
            McError::System("Mission Control AX group absent mid-lifecycle".to_owned())
        })?;
        let displays = crate::ffi::ax_children(&mc).map_err(|code| {
            McError::System(format!(
                "mc AXChildren unreadable: {}",
                crate::ffi::ax_error_name(code)
            ))
        })?;
        for candidate in displays {
            if !matches!(
                crate::ffi::ax_identifier(&candidate),
                Ok(Some(id)) if id == MC_ID_DISPLAY
            ) {
                continue;
            }
            match crate::ffi::ax_display_id(&candidate) {
                Ok(Some(id)) if id == i64::from(cgid) => {
                    return child_with_id(&candidate, MC_ID_SPACES)?.ok_or_else(|| {
                        McError::System("mc.spaces group missing for display".to_owned())
                    });
                }
                Ok(_) => {}
                Err(code) => {
                    return Err(McError::System(format!(
                        "AXDisplayID unreadable: {}",
                        crate::ffi::ax_error_name(code)
                    )));
                }
            }
        }
        Err(McError::Blocked(format!(
            "no mc.display with AXDisplayID {cgid}; refusing without an exact display join"
        )))
    }

    /// Fresh `mc.spaces.list` children (ordered Space buttons).
    fn spaces_list_children(
        spaces: &crate::ffi::AxElement,
    ) -> Result<Vec<crate::ffi::AxElement>, McError> {
        let list = child_with_id(spaces, MC_ID_SPACES_LIST)?
            .ok_or_else(|| McError::System("mc.spaces.list missing for display".to_owned()))?;
        crate::ffi::ax_children(&list).map_err(|code| {
            McError::System(format!(
                "mc.spaces.list children unreadable: {}",
                crate::ffi::ax_error_name(code)
            ))
        })
    }

    /// Fresh SLS rows → snapshot.
    pub fn fresh_snapshot() -> Result<TopologySnapshot, McError> {
        let rows = crate::ffi::sls_display_rows()
            .map_err(|e| McError::Unsupported(format!("SLS display rows unreadable: {e}")))?;
        Ok(TopologySnapshot::from_rows(&rows))
    }

    /// Per-action gates: fresh unlocked-session verdict plus observed AX
    /// trust. Runs at leg start, immediately pre-toggle, post-delay, and
    /// immediately pre-press.
    pub fn action_gates() -> Result<(), McError> {
        super::require_unlocked_session(&crate::ffi::session_state())?;
        super::require_ax_trust(crate::ffi::ax_is_process_trusted())?;
        Ok(())
    }

    /// Close attempt folded into leg endings: records the close shortfall (if
    /// any) and re-reads presence. Never turns a sent press into `Err`.
    fn finish_press(mut outcome: super::PressOutcome) -> super::PressOutcome {
        if let Err(e) = mc_close() {
            outcome.close_error = Some(e.to_string());
        }
        outcome.mc_open = mc_group_present().unwrap_or(true);
        outcome
    }

    /// Shared leg skeleton: budgeted open → stabilize → caller press →
    /// finish (close attempt folded into the outcome). Error paths close MC
    /// only when this leg opened it.
    fn run_mc_leg(
        open_spend: &str,
        budget: &mut ActionBudget,
        cleanup: bool,
        press: impl FnOnce(&mut ActionBudget) -> Result<super::PressOutcome, McError>,
    ) -> Result<super::PressOutcome, McError> {
        super::spend_leg(budget, cleanup, open_spend)?;
        let opened = mc_open()?;
        stabilize();
        match press(budget) {
            Ok(outcome) => Ok(finish_press(outcome)),
            Err(e) => {
                if opened && let Err(c) = mc_close() {
                    let reason = format!("{e}; MC close on error path also failed: {c}");
                    return Err(if matches!(e, McError::Dispatch(_)) {
                        McError::Dispatch(reason)
                    } else {
                        McError::System(reason)
                    });
                }
                Err(e)
            }
        }
    }

    /// Add one Space on `display`. A refused press is `Err` (nothing sent); a
    /// sent press always yields an outcome with the post-press snapshot and
    /// MC presence, even when the close fails.
    pub fn mc_add_space(
        display: usize,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        action_gates()?;
        let snapshot = fresh_snapshot()?;
        let (cgid, _) = display_cgid_of(&snapshot, display)?;
        run_mc_leg("add-open", budget, cleanup, |b| {
            mc_add_pressed(cgid, b, cleanup)
        })
    }

    /// Pinned add helper for the effect-thread trailing preparation: the
    /// same single-Space create as [`mc_add_space`], but the SLS-to-CG join
    /// rides the caller's main-thread-published `(cgid, managed UUID)`
    /// verified by [`display_cgid_of_pinned`] instead of the AppKit
    /// snapshot - no main thread touch, no marshal, no block. The pinned
    /// identity re-verifies post-open (a display swap mid-leg refuses
    /// before any press).
    pub fn mc_add_space_pinned(
        display: usize,
        pinned_cgid: u32,
        pinned_uuid: &str,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        action_gates()?;
        let snapshot = fresh_snapshot()?;
        let (cgid, _) = display_cgid_of_pinned(&snapshot, display, pinned_cgid, pinned_uuid)?;
        run_mc_leg("add-open", budget, cleanup, |b| {
            let fresh = fresh_snapshot()?;
            let (cgid_fresh, _) =
                display_cgid_of_pinned(&fresh, display, pinned_cgid, pinned_uuid)?;
            if cgid_fresh != cgid {
                return Err(McError::Blocked(format!(
                    "CG display ID changed during open (was {cgid}, now {cgid_fresh}); refusing stale dispatch"
                )));
            }
            mc_add_pressed(cgid, b, cleanup)
        })
    }

    fn mc_add_pressed(
        cgid: u32,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        action_gates()?;
        let spaces = spaces_group(cgid)?;
        let add = child_with_id(&spaces, MC_ID_SPACES_ADD)?
            .ok_or_else(|| McError::System("mc.spaces.add missing for display".to_owned()))?;
        super::spend_leg(budget, cleanup, "add-press")?;
        action_gates()?;
        crate::ffi::ax_perform_action(&add, MC_ACTION_PRESS).map_err(|code| {
            McError::Dispatch(format!(
                "mc.spaces.add press refused: {}",
                crate::ffi::ax_error_name(code)
            ))
        })?;
        Ok(super::PressOutcome {
            after: fresh_snapshot().ok(),
            mc_open: true,
            close_error: None,
        })
    }

    /// Go to `sid`: SID-accepting (never a cached index). Fresh locate →
    /// open → stabilize → fresh re-locate with drift rejection → exact AX
    /// list-count match → single press → outcome (never a lost press).
    pub fn mc_goto_sid(
        sid: u64,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        action_gates()?;
        let snap0 = fresh_snapshot()?;
        let (d0, _) = snap0.locate(sid).ok_or_else(|| {
            McError::Ownership(format!(
                "Space {sid} not uniquely located before goto; ownership uncertain — refusing"
            ))
        })?;
        let (cgid, _) = display_cgid_of(&snap0, d0)?;
        run_mc_leg("goto-open", budget, cleanup, |b| {
            mc_goto_pressed(sid, &snap0, cgid, b, cleanup)
        })
    }

    fn mc_goto_pressed(
        sid: u64,
        snap0: &TopologySnapshot,
        cgid: u32,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        action_gates()?;
        let snap1 = fresh_snapshot()?;
        let (d1, i1) = snap1.locate(sid).ok_or_else(|| {
            McError::Ownership(format!(
                "Space {sid} moved during open; ownership uncertain — refusing"
            ))
        })?;
        let (d0, i0) = snap0.locate(sid).ok_or_else(|| {
            McError::Ownership(format!("Space {sid} lost its pre-open join; refusing"))
        })?;
        if (d1, i1) != (d0, i0) || snap1.displays[d1].identifier != snap0.displays[d0].identifier {
            return Err(McError::Blocked(format!(
                "topology drifted during open (was display {d0} index {i0}, now {d1}/{i1}); refusing stale dispatch"
            )));
        }
        let (cgid_fresh, _) = display_cgid_of(&snap1, d1)?;
        if cgid_fresh != cgid {
            return Err(McError::Blocked(format!(
                "CG display ID changed during open (was {cgid}, now {cgid_fresh}); refusing stale dispatch"
            )));
        }
        let children = spaces_list_children(&spaces_group(cgid_fresh)?)?;
        if children.len() != snap1.displays[d1].spaces.len() {
            return Err(McError::Blocked(format!(
                "AX list count {} != SLS row count {} for display {d1}; refusing index dispatch",
                children.len(),
                snap1.displays[d1].spaces.len()
            )));
        }
        let target = &children[i1];
        super::spend_leg(budget, cleanup, "goto-press")?;
        action_gates()?;
        crate::ffi::ax_perform_action(target, MC_ACTION_PRESS).map_err(|code| {
            McError::Dispatch(format!(
                "goto press refused: {}",
                crate::ffi::ax_error_name(code)
            ))
        })?;
        Ok(super::PressOutcome {
            after: fresh_snapshot().ok(),
            mc_open: true,
            close_error: None,
        })
    }

    /// Remove `sid` via `AXRemoveDesktop`. Same SID-accepting,
    /// drift-rejecting shape as goto.
    pub fn mc_remove_sid(
        sid: u64,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        action_gates()?;
        let snap0 = fresh_snapshot()?;
        let (d0, _) = snap0.locate(sid).ok_or_else(|| {
            McError::Ownership(format!(
                "Space {sid} not uniquely located before remove; ownership uncertain — cleanup blocked"
            ))
        })?;
        let (cgid, _) = display_cgid_of(&snap0, d0)?;
        run_mc_leg("remove-open", budget, cleanup, |b| {
            mc_remove_pressed(sid, &snap0, cgid, b, cleanup)
        })
    }

    fn mc_remove_pressed(
        sid: u64,
        snap0: &TopologySnapshot,
        cgid: u32,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        action_gates()?;
        let snap1 = fresh_snapshot()?;
        let (d1, i1) = snap1.locate(sid).ok_or_else(|| {
            McError::Ownership(format!(
                "Space {sid} moved during open; ownership uncertain — cleanup blocked"
            ))
        })?;
        let (d0, i0) = snap0.locate(sid).ok_or_else(|| {
            McError::Ownership(format!("Space {sid} lost its pre-open join; refusing"))
        })?;
        if (d1, i1) != (d0, i0) || snap1.displays[d1].identifier != snap0.displays[d0].identifier {
            return Err(McError::Blocked(format!(
                "topology drifted during open (was display {d0} index {i0}, now {d1}/{i1}); refusing stale dispatch"
            )));
        }
        let (cgid_fresh, _) = display_cgid_of(&snap1, d1)?;
        if cgid_fresh != cgid {
            return Err(McError::Blocked(format!(
                "CG display ID changed during open (was {cgid}, now {cgid_fresh}); refusing stale dispatch"
            )));
        }
        let children = spaces_list_children(&spaces_group(cgid_fresh)?)?;
        if children.len() != snap1.displays[d1].spaces.len() {
            return Err(McError::Blocked(format!(
                "AX list count {} != SLS row count {} for display {d1}; refusing index dispatch",
                children.len(),
                snap1.displays[d1].spaces.len()
            )));
        }
        let target = &children[i1];
        super::spend_leg(budget, cleanup, "remove-press")?;
        action_gates()?;
        crate::ffi::ax_perform_action(target, MC_ACTION_REMOVE_DESKTOP).map_err(|code| {
            McError::System(format!(
                "remove-desktop refused: {}",
                crate::ffi::ax_error_name(code)
            ))
        })?;
        Ok(super::PressOutcome {
            after: fresh_snapshot().ok(),
            mc_open: true,
            close_error: None,
        })
    }

    /// Bounded wait for the SLS snapshot to satisfy `want`, polling fresh. A
    /// timeout returns the latest snapshot so the caller names the actual
    /// diff. Unreadable topology propagates (not a timeout).
    pub fn await_snapshot(
        want: &dyn Fn(&TopologySnapshot) -> bool,
    ) -> Result<TopologySnapshot, McError> {
        let mut latest = fresh_snapshot()?;
        if want(&latest) {
            return Ok(latest);
        }
        poll_until_ms(MC_ACTION_TIMEOUT_MS, 200, || match fresh_snapshot() {
            Ok(s) => {
                let done = want(&s);
                latest = s;
                done.then_some(())
            }
            Err(_) => None,
        });
        Ok(latest)
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) mod live {
    use super::*;

    #[cfg(test)]
    pub(crate) use super::display_cgid_of_pinned;

    pub struct CaffeinateGuard {
        pub reaped: bool,
    }

    impl CaffeinateGuard {
        pub fn spawn(_seconds: u64) -> Result<Self, McError> {
            Err(McError::Unsupported(
                "Mission Control lifecycle is macOS-only".to_owned(),
            ))
        }

        pub fn stop(&mut self) -> bool {
            self.reaped = false;
            self.reaped
        }
    }

    fn unsupported<T>(what: &str) -> Result<T, McError> {
        Err(McError::Unsupported(format!("{what} is macOS-only")))
    }

    pub fn dock_pid() -> Result<i32, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn mc_group_present() -> Result<bool, McError> {
        unsupported("Mission Control lifecycle")
    }

    // Live-only helpers (called only from the macOS `live` module); off-mac
    // the stub is never invoked, so silence dead_code there only.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn poll_group(_want: bool) -> Result<bool, McError> {
        unsupported("Mission Control lifecycle")
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn mc_open() -> Result<bool, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn mc_close() -> Result<bool, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn fresh_snapshot() -> Result<TopologySnapshot, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn action_gates() -> Result<(), McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn mc_add_space(
        _display: usize,
        _budget: &mut ActionBudget,
        _cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn mc_add_space_pinned(
        _display: usize,
        _pinned_cgid: u32,
        _pinned_uuid: &str,
        _budget: &mut ActionBudget,
        _cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn mc_goto_sid(
        _sid: u64,
        _budget: &mut ActionBudget,
        _cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn mc_remove_sid(
        _sid: u64,
        _budget: &mut ActionBudget,
        _cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn await_snapshot(
        _want: &dyn Fn(&TopologySnapshot) -> bool,
    ) -> Result<TopologySnapshot, McError> {
        unsupported("Mission Control lifecycle")
    }
}

/// Press outcome: a sent press always yields one, carrying the post-press
/// snapshot (for exact-diff adoption) and MC presence (for safe-close and
/// residue), even when the close failed. A refused press is `Err` (nothing
/// sent, nothing to adopt).
#[derive(Debug, Clone)]
pub struct PressOutcome {
    pub after: Option<TopologySnapshot>,
    pub mc_open: bool,
    pub close_error: Option<String>,
}

/// Upfront topology preflight, before ANY mutation: the chosen display must
/// have a readable current (the goto-back target), and every original Space
/// on it must read as a user Space. Type-symbol availability is proven here
/// too, so the post-create temp check cannot meet a missing symbol.
fn require_upfront_topology(snapshot: &TopologySnapshot, display: usize) -> Result<(), McError> {
    let row = snapshot.displays.get(display).ok_or_else(|| {
        McError::Blocked(format!(
            "display {display} unknown ({} displays); refusing before any mutation",
            snapshot.displays.len()
        ))
    })?;
    row.current.ok_or_else(|| {
        McError::Blocked(
            "chosen display has no current Space in the preimage; refusing before any mutation — the goto-back target must exist upfront"
                .to_owned(),
        )
    })?;
    for sid in &row.spaces {
        match crate::ffi::sls_space_type(*sid) {
            Ok(crate::ffi::SlsSpaceType::User) => {}
            Ok(other) => {
                return Err(McError::Blocked(format!(
                    "preexisting Space {sid} on display {display} is not a user Space ({other:?}); refusing before any mutation — this is not a SIP verdict"
                )));
            }
            Err(cause) => {
                return Err(McError::Unsupported(format!(
                    "preexisting Space {sid} type unreadable: {cause}; refusing before any mutation"
                )));
            }
        }
    }
    Ok(())
}

/// Temp must read as a user Space (fullscreen/system temps have no proven
/// removal); an unreadable type fails closed.
pub(crate) fn require_user_temp(temp: u64) -> Result<(), McError> {
    match crate::ffi::sls_space_type(temp) {
        Ok(crate::ffi::SlsSpaceType::User) => Ok(()),
        Ok(other) => Err(McError::Blocked(format!(
            "temp Space {temp} is not a user Space ({other:?}); refusing lifecycle without a proven removal — this is not a SIP verdict"
        ))),
        Err(cause) => Err(McError::Unsupported(format!(
            "temp Space {temp} type unreadable: {cause}"
        ))),
    }
}

/// Goto helper: SID-accepting dispatch plus exact-current readback against
/// the `from` baseline. A sent press settles asynchronously, so a failed
/// immediate readback polls fresh snapshots within [`MC_EFFECT_SETTLE_MS`]
/// (poll only — the dispatched press is never replayed) before the timeout
/// names the actual diff. Returns (post-goto snapshot, close shortfall).
pub(crate) fn goto_sid_verified(
    from: &TopologySnapshot,
    display: usize,
    target: u64,
    budget: &mut ActionBudget,
    cleanup: bool,
) -> Result<(TopologySnapshot, Option<String>), McError> {
    let outcome = live::mc_goto_sid(target, budget, cleanup)?;
    let mut after = outcome.after.ok_or_else(|| {
        McError::Blocked(format!(
            "goto Space {target}: post-press snapshot unreadable (MC open: {}); effect unknown — restoring",
            outcome.mc_open
        ))
    })?;
    if validate_goto_effect(from, &after, display, target).is_ok() {
        return Ok((after, outcome.close_error));
    }
    // Async settle: the press was sent; poll fresh readbacks (never a
    // replay) until the effect lands or the bounded window expires.
    let t0 = std::time::Instant::now();
    let mut last = validate_goto_effect(from, &after, display, target).unwrap_err();
    while t0.elapsed() < std::time::Duration::from_millis(MC_EFFECT_SETTLE_MS) {
        std::thread::sleep(std::time::Duration::from_millis(200));
        match live::fresh_snapshot() {
            Ok(s) => {
                if validate_goto_effect(from, &s, display, target).is_ok() {
                    return Ok((s, outcome.close_error));
                }
                last = validate_goto_effect(from, &s, display, target).unwrap_err();
                after = s;
            }
            Err(e) => {
                last = e;
            }
        }
    }
    let _ = after;
    Err(McError::Blocked(format!(
        "goto Space {target}: no effect within {}ms after a sent press ({last}); restoring",
        MC_EFFECT_SETTLE_MS,
    )))
}

/// Removal helper: fresh emptiness scan, fresh eligibility, SID-accepting
/// dispatch, exact removal-effect readback. Returns (scan, close shortfall).
/// Sets `remove_dispatched` once a press is sent, even when the effect stays
/// unverified — cleanup must never replay it.
pub(crate) fn remove_sid_verified(
    temp: u64,
    budget: &mut ActionBudget,
    cleanup: bool,
    remove_dispatched: &mut bool,
) -> Result<(crate::mc_probe::EmptySummary, Option<String>), McError> {
    let scan = verify_space_empty(temp)?;
    let pre = live::fresh_snapshot()?;
    let di = removal_eligibility(&pre, temp)?;
    let (ldi, _) = pre.locate(temp).ok_or_else(|| {
        McError::Ownership(format!(
            "temp Space {temp} moved between scan and dispatch; ownership uncertain — cleanup blocked"
        ))
    })?;
    if ldi != di {
        return Err(McError::Ownership(format!(
            "temp Space {temp} joined display {ldi}, eligibility said {di}; ownership uncertain — cleanup blocked"
        )));
    }
    let outcome = live::mc_remove_sid(temp, budget, cleanup)?;
    *remove_dispatched = true;
    let mut after = outcome.after.ok_or_else(|| {
        McError::Blocked(format!(
            "remove Space {temp}: post-press snapshot unreadable (MC open: {}); effect unknown — reporting residue, never replaying",
            outcome.mc_open
        ))
    })?;
    if validate_removal_effect(&pre, &after, temp).is_ok() {
        return Ok((scan, outcome.close_error));
    }
    // Async settle: the press was sent; poll fresh readbacks (never a
    // replay) until the removal lands or the bounded window expires.
    let t0 = std::time::Instant::now();
    let mut last = validate_removal_effect(&pre, &after, temp).unwrap_err();
    while t0.elapsed() < std::time::Duration::from_millis(MC_EFFECT_SETTLE_MS) {
        std::thread::sleep(std::time::Duration::from_millis(200));
        match live::fresh_snapshot() {
            Ok(s) => {
                if validate_removal_effect(&pre, &s, temp).is_ok() {
                    return Ok((scan, outcome.close_error));
                }
                last = validate_removal_effect(&pre, &s, temp).unwrap_err();
                after = s;
            }
            Err(e) => {
                last = e;
            }
        }
    }
    let _ = after;
    Err(McError::Blocked(format!(
        "remove Space {temp}: still present within {}ms after a sent press ({last}); reporting residue, never replaying",
        MC_EFFECT_SETTLE_MS,
    )))
}

/// Ordered per-display current restoration to the preimage currents, inside
/// the given budget phase. Drift with an unlocatable preimage is noted,
/// never forced. Returns (latest snapshot or None, all-restored, notes).
fn restore_currents_to(
    before: &TopologySnapshot,
    budget: &mut ActionBudget,
    cleanup: bool,
) -> (Option<TopologySnapshot>, bool, Vec<String>) {
    let mut notes = Vec::new();
    let mut after = match live::fresh_snapshot() {
        Ok(s) => s,
        Err(e) => {
            notes.push(format!(
                "restore snapshot unreadable ({e}); ownership uncertain"
            ));
            return (None, false, notes);
        }
    };
    let mut ok = true;
    for (di, pre) in before.displays.iter().enumerate() {
        let Some(want) = pre.current else { continue };
        if after.displays.get(di).and_then(|d| d.current) == Some(want) {
            continue;
        }
        match after.locate(want) {
            Some((ldi, _)) if ldi == di => {
                match goto_sid_verified(&after, di, want, budget, cleanup) {
                    Ok((s, close)) => {
                        after = s;
                        if let Some(c) = close {
                            notes.push(format!("restore close shortfall on display {di}: {c}"));
                        }
                    }
                    Err(e) => {
                        ok = false;
                        notes.push(format!("current restore on display {di} refused: {e}"));
                    }
                }
            }
            _ => {
                ok = false;
                notes.push(format!(
                    "original current {want} not uniquely located; display {di} left as read back"
                ));
            }
        }
    }
    (Some(after), ok, notes)
}

/// Best-effort MC close where safely possible: gates must pass (a locked or
/// untrusted session is left alone) and MC must read present. Every outcome
/// is noted; nothing is assumed.
pub(crate) fn safe_close_mc(notes: &mut Vec<String>) {
    if live::action_gates().is_err() {
        notes.push(
            "MC close skipped: session gates fail (locked or untrusted); MC left as-is".to_owned(),
        );
        return;
    }
    match live::mc_group_present() {
        Ok(false) => notes.push("MC already absent".to_owned()),
        Ok(true) => match live::mc_close() {
            Ok(_) => notes.push("MC closed".to_owned()),
            Err(e) => notes.push(format!(
                "MC close failed: {e}; presence preserved in report"
            )),
        },
        Err(e) => notes.push(format!("MC presence unreadable, close not attempted: {e}")),
    }
}

/// Lifecycle run context: owns the caffeinate companion, preimages,
/// baselines, budget, and evidence accumulators. Failures funnel through
/// [`RunCtx::fail`]: the temp-carrying path keeps caffeinate alive through
/// cleanup and stops it last.
struct RunCtx {
    caffeinate: live::CaffeinateGuard,
    before: TopologySnapshot,
    layout_before: McLayout,
    temp: Option<u64>,
    budget: ActionBudget,
    remove_dispatched: bool,
    notes: Vec<String>,
    latencies: Vec<LegLatency>,
    focus_before: FocusLegs,
    ghostty_before: Vec<GhosttyWindow>,
    sep_before: Option<bool>,
}

impl RunCtx {
    fn lat(&mut self, leg: &str, t0: std::time::Instant) {
        self.latencies.push(LegLatency {
            leg: leg.to_owned(),
            ms: t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        });
    }

    fn fail(&mut self, stage: &str, action: McAction, error: McError) -> (i32, String) {
        match self.temp {
            Some(temp) => cleanup_after_failure(self, stage, action, error, temp),
            None => {
                self.notes.push(error.to_string());
                let reaped = self.caffeinate.stop();
                let (gb, ga, fb, fa) = self.evidence();
                error_json(&McErrorReport {
                    stage: stage.to_owned(),
                    action: action.as_str().to_owned(),
                    error: self.notes.join("; "),
                    temp_space: None,
                    layout_before: Some(self.layout_before.clone()),
                    layout_after: live::fresh_snapshot().ok().map(|s| s.sanitized()),
                    currents_restored: false,
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
    }

    /// Evidence captured for the error report: protected snapshots and
    /// focus legs from both sides whenever the run captured them, so a
    /// failed battery never drops the mismatch it observed.
    fn evidence(&self) -> McFailEvidence {
        (
            Some(self.ghostty_before.clone()),
            snapshot_ghostty_windows().ok(),
            Some(self.focus_before.clone()),
            Some(capture_focus_legs()),
        )
    }
}

/// Post-failure cleanup with ownership certainty: ordered current restore,
/// then temp removal ONLY when freshly eligible AND no remove was already
/// dispatched without a verified effect. Caffeinate stays alive throughout
/// and stops last.
fn cleanup_after_failure(
    ctx: &mut RunCtx,
    stage: &str,
    action: McAction,
    error: McError,
    temp: u64,
) -> (i32, String) {
    ctx.notes.push(error.to_string());
    let (after, currents_restored, mut restore_notes) =
        restore_currents_to(&ctx.before, &mut ctx.budget, true);
    ctx.notes.append(&mut restore_notes);
    if let Some(snapshot) = after {
        if snapshot.all_sids().contains(&temp) {
            if ctx.remove_dispatched {
                ctx.notes.push(format!(
                    "temp Space {temp} still present but a remove was already dispatched without verified effect; NOT replayed — manual residue"
                ));
            } else {
                cleanup_remove_temp(ctx, temp);
            }
        } else {
            ctx.notes.push(format!(
                "temp Space {temp} already absent; nothing to remove"
            ));
        }
    } else {
        ctx.notes
            .push("cleanup snapshot unreadable; ownership uncertain — removal blocked".to_owned());
    }
    safe_close_mc(&mut ctx.notes);
    let reaped = ctx.caffeinate.stop();
    let (gb, ga, fb, fa) = ctx.evidence();
    error_json(&McErrorReport {
        stage: stage.to_owned(),
        action: action.as_str().to_owned(),
        error: ctx.notes.join("; "),
        temp_space: Some(temp),
        layout_before: Some(ctx.layout_before.clone()),
        layout_after: live::fresh_snapshot().ok().map(|s| s.sanitized()),
        currents_restored,
        mc_open_after: live::mc_group_present().ok(),
        caffeinate_reaped: reaped,
        ghostty_before: gb,
        ghostty_after: ga,
        focus_before: fb,
        focus_after: fa,
        notes: std::mem::take(&mut ctx.notes),
        latencies: std::mem::take(&mut ctx.latencies),
    })
}

/// Single ownership-certain temp removal for the error path (no replay: the
/// caller guarantees no prior unverified dispatch).
fn cleanup_remove_temp(ctx: &mut RunCtx, temp: u64) {
    match remove_sid_verified(temp, &mut ctx.budget, true, &mut ctx.remove_dispatched) {
        Ok((scan, close)) => {
            ctx.notes.push(format!(
                "temp Space {temp} removed during cleanup ({} candidates clear, {} shell skipped)",
                scan.checked, scan.shell_skipped
            ));
            if let Some(c) = close {
                ctx.notes
                    .push(format!("cleanup remove close shortfall: {c}"));
            }
        }
        Err(e) => ctx.notes.push(format!("cleanup removal: {e}")),
    }
}

/// Fixed-arity structured-error constructor (stable field order over
/// call-site brevity).
#[allow(clippy::too_many_arguments)]
pub(crate) fn fail_plain_inner(
    stage: &str,
    action: McAction,
    error: McError,
    layout_before: Option<McLayout>,
    mc_open_after: Option<bool>,
    caffeinate_reaped: bool,
    notes: Vec<String>,
    latencies: Vec<LegLatency>,
) -> (i32, String) {
    error_json(&McErrorReport {
        stage: stage.to_owned(),
        action: action.as_str().to_owned(),
        error: error.to_string(),
        temp_space: None,
        layout_before,
        layout_after: live::fresh_snapshot().ok().map(|s| s.sanitized()),
        currents_restored: false,
        mc_open_after,
        caffeinate_reaped,
        ghostty_before: None,
        ghostty_after: None,
        focus_before: None,
        focus_after: None,
        notes,
        latencies,
    })
}

/// Read-only Mission Control inspection. No launch, no writes, no posts.
pub fn run_mc_inspect() -> Result<String, (i32, String)> {
    let session = crate::ffi::session_state();
    let ax_trusted = crate::ffi::ax_is_process_trusted();
    let post_event_access = crate::ffi::cg_post_event_access_known();
    let (dock_pid, dock_bundle, presence, presence_error) = match live::dock_pid() {
        Ok(pid) => {
            let bundle = crate::ffi::bundle_id_for_pid(pid).unwrap_or_default();
            let (present, err) = match require_dock_bundle(&bundle) {
                Ok(()) => match live::mc_group_present() {
                    Ok(p) => (Some(p), None),
                    Err(e) => (None, Some(e.to_string())),
                },
                Err(e) => (None, Some(e.to_string())),
            };
            (Some(pid), Some(bundle), present, err)
        }
        Err(e) => (None, None, None, Some(e.to_string())),
    };
    let (layout, layout_error) = match live::fresh_snapshot() {
        Ok(s) => (Some(s.sanitized()), None),
        Err(e) => (None, Some(e.to_string())),
    };
    let report = McInspectReport {
        session,
        ax_trusted,
        post_event_access,
        dock_pid,
        dock_bundle,
        mc_group_present: presence,
        presence_error,
        layout,
        layout_error,
        synthetic_events_posted: false,
        mutates: false,
    };
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}

/// Bounded temporary-Space lifecycle. Success requires exact ordered
/// restoration, separate-Spaces equality, MC absence, full focus-oracle
/// agreement, and Ghostty frame/membership equality — with notes and
/// per-leg latencies retained either way.
pub fn run_mc_lifecycle(
    seconds: u64,
    display: usize,
    allow_untested: bool,
) -> Result<String, (i32, String)> {
    crate::config::ProbeConfig::validate_probe_seconds(seconds).map_err(|e| (2, e))?;
    if !allow_untested {
        return Err((
            2,
            "mc-lifecycle requires --allow-untested (visible Mission Control automation with unproven close-toggle semantics; recovery is observer-verified per run, never assumed)".to_owned(),
        ));
    }
    if seconds < crate::mc_probe::MC_MIN_LIFECYCLE_SECS {
        return Err((
            2,
            format!(
                "mc-lifecycle needs --seconds >= {} ({}s cleanup reserve plus minimum work); refusing before any mutation",
                crate::mc_probe::MC_MIN_LIFECYCLE_SECS,
                MC_CLEANUP_RESERVE_SECS,
            ),
        ));
    }

    let t_gates = std::time::Instant::now();
    if let Err(e) = live::action_gates() {
        return Err(fail_plain_inner(
            "pre-gates",
            McAction::Gate,
            e,
            live::fresh_snapshot().ok().map(|s| s.sanitized()),
            live::mc_group_present().ok(),
            true,
            Vec::new(),
            Vec::new(),
        ));
    }
    // Read-only baselines before spawning anything: separate-Spaces state,
    // full focus-oracle legs, Ghostty protection snapshot.
    let sep_before = crate::ffi::ns_separate_spaces();
    if sep_before.is_none() {
        return Err(fail_plain_inner(
            "baseline-separate",
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
                "baseline-ghostty",
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
                "caffeinate-spawn",
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
    let before_snapshot = match live::fresh_snapshot() {
        Ok(s) => s,
        Err(e) => {
            let reaped = caffeinate.stop();
            return Err(fail_plain_inner(
                "snapshot-before",
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
    // Upfront preflight before ANY mutation: display range, readable
    // goto-back target, all original Spaces user-typed.
    if let Err(e) = require_upfront_topology(&before_snapshot, display) {
        let reaped = caffeinate.stop();
        return Err(fail_plain_inner(
            "upfront-topology",
            McAction::Snapshot,
            e,
            Some(before_snapshot.sanitized()),
            live::mc_group_present().ok(),
            reaped,
            Vec::new(),
            Vec::new(),
        ));
    }
    let mut ctx = RunCtx {
        caffeinate,
        layout_before: before_snapshot.sanitized(),
        before: before_snapshot,
        temp: None,
        budget: ActionBudget::new(seconds),
        remove_dispatched: false,
        notes: Vec::new(),
        latencies: Vec::new(),
        focus_before,
        ghostty_before,
        sep_before,
    };
    ctx.lat("baselines", t_gates);
    let original_current = ctx.before.displays[display]
        .current
        .expect("upfront topology proved a current here");

    // Create leg: dispatch, then adopt the temp ONLY through the
    // exactly-one-new-SID gate.
    let t = std::time::Instant::now();
    let create_outcome = match live::mc_add_space(display, &mut ctx.budget, false) {
        Ok(outcome) => outcome,
        Err(e) => return Err(ctx.fail("create-dispatch", McAction::AddSpace, e)),
    };
    if let Some(c) = create_outcome.close_error {
        ctx.notes.push(format!("create close shortfall: {c}"));
    }
    let created_snapshot =
        match live::await_snapshot(&|s| validate_create_effect(&ctx.before, s, display).is_ok()) {
            Ok(s) => s,
            Err(e) => return Err(ctx.fail("create-readback", McAction::AddSpace, e)),
        };
    let temp = match validate_create_effect(&ctx.before, &created_snapshot, display) {
        Ok(sid) => sid,
        Err(e) => return Err(ctx.fail("create-adopt", McAction::AddSpace, e)),
    };
    ctx.temp = Some(temp);
    ctx.lat("add", t);
    if let Err(e) = require_user_temp(temp) {
        return Err(ctx.fail("create-type", McAction::AddSpace, e));
    }

    // Goto leg: fresh SID join for the adopted temp, exact-current readback.
    let t = std::time::Instant::now();
    let (goto_snapshot, goto_close) =
        match goto_sid_verified(&created_snapshot, display, temp, &mut ctx.budget, false) {
            Ok(v) => v,
            Err(e) => return Err(ctx.fail("goto-temp", McAction::GotoSpace, e)),
        };
    if let Some(c) = goto_close {
        ctx.notes.push(format!("goto close shortfall: {c}"));
    }
    ctx.lat("goto", t);

    // Back leg: return to the upfront-proven original current.
    if original_current == temp {
        // Unreachable when the create gate held (create never switches
        // current), closed explicitly rather than assumed.
        return Err(ctx.fail(
            "back-target",
            McAction::GotoSpace,
            McError::Blocked(
                "preimage current equals the adopted temp; nothing to return to — refusing rather than guessing"
                    .to_owned(),
            ),
        ));
    }
    let t = std::time::Instant::now();
    match goto_sid_verified(
        &goto_snapshot,
        display,
        original_current,
        &mut ctx.budget,
        false,
    ) {
        Ok((_, close)) => {
            if let Some(c) = close {
                ctx.notes.push(format!("back close shortfall: {c}"));
            }
        }
        Err(e) => return Err(ctx.fail("goto-back", McAction::GotoSpace, e)),
    }
    ctx.lat("back", t);

    // Removal leg: fresh snapshot for the report, then verified removal
    // (emptiness scan, eligibility, SID dispatch, readback).
    let t = std::time::Instant::now();
    let pre_remove = match live::fresh_snapshot() {
        Ok(s) => s,
        Err(e) => return Err(ctx.fail("pre-remove", McAction::RemoveSpace, e)),
    };
    let (scan, remove_close) =
        match remove_sid_verified(temp, &mut ctx.budget, false, &mut ctx.remove_dispatched) {
            Ok(v) => v,
            Err(e) => return Err(ctx.fail("remove-temp", McAction::RemoveSpace, e)),
        };
    if let Some(c) = remove_close {
        ctx.notes.push(format!("remove close shortfall: {c}"));
    }
    ctx.lat("remove", t);

    // Ordered restoration plus the full success battery, all still under
    // the live caffeinate companion.
    let t = std::time::Instant::now();
    let (after_opt, currents_restored, mut restore_notes) =
        restore_currents_to(&ctx.before, &mut ctx.budget, false);
    ctx.notes.append(&mut restore_notes);
    let after = match after_opt {
        Some(s) => s,
        None => {
            return Err(ctx.fail(
                "final-readback",
                McAction::Snapshot,
                McError::Unsupported("final topology unreadable".to_owned()),
            ));
        }
    };
    if !currents_restored {
        return Err(ctx.fail(
            "final-restore",
            McAction::Snapshot,
            McError::Blocked(
                "per-display currents not fully restored; residue reported".to_owned(),
            ),
        ));
    }
    if let Err(e) = crate::mc_probe::require_exact_restoration(&ctx.before, &after) {
        return Err(ctx.fail("final-topology", McAction::Snapshot, e));
    }
    let ghostty_after = match snapshot_ghostty_windows() {
        Ok(g) => g,
        Err(e) => return Err(ctx.fail("final-ghostty", McAction::Snapshot, e)),
    };
    if ghostty_after != ctx.ghostty_before {
        return Err(ctx.fail(
            "final-ghostty",
            McAction::Snapshot,
            McError::Blocked(
                "Ghostty frames/membership changed during the run; reporting, never writing"
                    .to_owned(),
            ),
        ));
    }
    let focus_after = capture_focus_legs();
    if !focus_restored(&ctx.focus_before, &focus_after) {
        return Err(ctx.fail(
            "final-focus",
            McAction::Snapshot,
            McError::Blocked(
                "natural focus return lacks full oracle agreement (workspace+SLPS+direct, all readable); unknown is not restored"
                    .to_owned(),
            ),
        ));
    }
    let sep_after = crate::ffi::ns_separate_spaces();
    if sep_after != ctx.sep_before {
        return Err(ctx.fail(
            "final-separate",
            McAction::Snapshot,
            McError::Blocked(format!(
                "separate-Spaces state changed (was {:?}, now {sep_after:?}); refusing success",
                ctx.sep_before
            )),
        ));
    }
    // MC must read absent at the end; one safe close attempt, then verdict.
    // No visual claim is made beyond the presence boolean.
    let mut mc_absent = !live::mc_group_present().unwrap_or(true);
    if !mc_absent {
        safe_close_mc(&mut ctx.notes);
        mc_absent = !live::mc_group_present().unwrap_or(true);
    }
    if !mc_absent {
        return Err(ctx.fail(
            "final-mc",
            McAction::Snapshot,
            McError::Blocked(
                "Mission Control still present at end of run; reporting, not claiming success"
                    .to_owned(),
            ),
        ));
    }
    ctx.lat("finalize", t);
    ctx.notes.push(format!(
        "temp {temp} adopted by exactly-one-new-SID diff; emptiness scan checked {} candidates",
        scan.checked
    ));
    let reaped = ctx.caffeinate.stop();
    let report = McLifecycleReport {
        display,
        temp_space: temp,
        seconds,
        layout_before: ctx.layout_before.clone(),
        displays_before: ctx.before.ordered_layouts(),
        created_layout: created_snapshot.sanitized(),
        goto_layout: goto_snapshot.sanitized(),
        back_layout: pre_remove.sanitized(),
        layout_after: after.sanitized(),
        displays_after: after.ordered_layouts(),
        currents_restored: true,
        separate_before: ctx.sep_before,
        separate_after: sep_after,
        mc_absent_at_end: true,
        focus_before: ctx.focus_before.clone(),
        focus_after,
        focus_restored: true,
        ghostty_before: ctx.ghostty_before.clone(),
        ghostty_after,
        ghostty_equal: true,
        empty_scan: scan,
        latencies: std::mem::take(&mut ctx.latencies),
        notes: std::mem::take(&mut ctx.notes),
        synthetic_events_posted: false,
        caffeinate_reaped: reaped,
    };
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}
