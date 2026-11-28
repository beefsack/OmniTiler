#![forbid(unsafe_code)]

//! Bounded Mission Control lifecycle run. Policy and shapes live in
//! [`crate::mc_probe`]; this module owns live dispatch, scans, and cleanup.
//!
//! Leg contract per action: gates → fresh SLS join → budgeted toggle (gates
//! immediately pre-toggle) → stabilize → gates → fresh re-join with drift
//! rejection → AX tree join with exact-list-count match → budgeted press with
//! gates immediately pre-press → effect readback → verified close. A sent
//! press always yields an outcome carrying the post-press snapshot and MC
//! presence, even when the close fails, so a press is never lost to a close
//! error. The close is verified against the AX presence group plus, only
//! when the tree was owned by WindowManager, its CG presentation windows
//! (the group disappears before the transform settles); Dock-owned and
//! not-yet-observed trees verify on presence alone. A goto press is never
//! toggled before its own dismissal is observed, and an absent surface is
//! never toggled back open.
//! AX traversal prefers `com.apple.WindowManager` (`mc.display` directly on
//! the application element) and falls back automatically to `com.apple.dock`
//! (`mc` → `mc.display`) when the WindowManager surface is absent, has no
//! exact running-application entry, or exposes no `mc.display` (macOS 15);
//! each surface's exact bundle is re-verified on every access. Error paths
//! close MC only when this leg opened it and gates still pass. A
//! dispatched-but-unverified remove is never replayed in cleanup (indices
//! shift under a missing Space); goto restoration re-targets by fresh SID
//! join, which is safe.

use crate::mc_probe::{
    ActionBudget, FocusLegs, GhosttyWindow, LegLatency, MC_CLEANUP_RESERVE_SECS,
    MC_EFFECT_SETTLE_MS, McAction, McError, McErrorReport, McFailEvidence, McInspectReport,
    McLayout, McLifecycleReport, McSurface, TopologySnapshot, error_json, focus_restored,
    removal_eligibility, require_dock_bundle, require_mc_surface_bundle, validate_create_effect,
    validate_goto_effect, validate_removal_effect,
};
// macOS-only live dispatch names (used via `live` on macOS; off-mac the stub
// never touches them, so gate them to keep strict Clippy clean).
#[cfg(target_os = "macos")]
use crate::mc_probe::{
    DOCK_BUNDLE_ID, MC_ACTION_PRESS, MC_ACTION_REMOVE_DESKTOP, MC_ACTION_TIMEOUT_MS,
    MC_AX_TIMEOUT_SECS, MC_GROUP_POLL_MS, MC_ID_DISPLAY, MC_ID_ROOT, MC_ID_SPACES,
    MC_ID_SPACES_ADD, MC_ID_SPACES_LIST, McSurfaceExposure, mc_owner, mc_presentation_cg_owner,
    record_mc_owner, require_ax_trust, require_unlocked_session, select_mc_surface,
};
// Read-only observation helpers live in `mc_observe`; re-exported here so
// existing `mc_lifecycle::…` paths keep working with no interface change.
pub(crate) use crate::mc_observe::{
    capture_focus_legs, enumerate_window_members, snapshot_ghostty_windows, verify_space_empty,
};

/// Which press this leg sends: only a goto press dismisses Mission Control
/// by itself (its expected end state is verified absence), while create and
/// remove presses leave it displayed. A close must never be toggled from a
/// presence read taken while an auto-dismissal is still in flight.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McDismissal {
    Auto,
    Manual,
}

/// Pure post-press close decision from the observed Mission Control state.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseDecision {
    /// Absence is verified (presence absent and no presentation windows);
    /// nothing to do and nothing to toggle.
    Done,
    /// Mission Control is demonstrably displayed: one presence-checked
    /// close is allowed.
    Toggle,
    /// No verified absence, but presence does not read true either: wait
    /// for the surface to settle. Toggling now could reopen a dismissal
    /// that is already in flight, and skipping the wait would probe an
    /// emptiness read against lingering presentation windows.
    Await,
}

/// The single close rule: only a verified-absent surface is a no-op, only a
/// presence-true surface may be toggled, and an auto-dismissing press is
/// never toggled before its own dismissal was observed (or ruled out).
#[cfg(any(target_os = "macos", test))]
fn decide_close(
    dismissal: McDismissal,
    verified_closed: bool,
    present: Option<bool>,
) -> CloseDecision {
    if verified_closed {
        CloseDecision::Done
    } else if dismissal == McDismissal::Auto {
        CloseDecision::Await
    } else if present == Some(true) {
        CloseDecision::Toggle
    } else {
        CloseDecision::Await
    }
}

/// Sampling cadence for the native detached dual-source Mission Control open.
#[cfg(any(target_os = "macos", test))]
const MC_OPEN_SAMPLE_MS: u64 = 250;

/// Bounded dual-source open wait: AX presence is the only readiness signal,
/// the exact WindowManager `layer >= 0` presentation is observed alongside,
/// and a CG-only positive never authorizes the press. The deadline cases
/// stay distinct (both absent, CG only, unreadable). Clock, sleep, and the
/// probe are injected so the contract is tested without real waiting; the
/// existing action bound is the only timeout.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn await_open_until_ax(
    timeout_ms: u64,
    sample_ms: u64,
    mut elapsed_ms: impl FnMut() -> u64,
    mut sleep_ms: impl FnMut(u64),
    mut probe: impl FnMut() -> (Result<bool, McError>, Result<bool, McError>),
) -> Result<(), McError> {
    let mut cg_seen = false;
    let mut last_ax: Result<bool, String> = Ok(false);
    let mut last_cg: Result<bool, String> = Ok(false);
    loop {
        let started = elapsed_ms();
        if started > timeout_ms {
            break;
        }
        let (ax, cg) = probe();
        last_ax = ax.map_err(|e| e.to_string());
        last_cg = cg.map_err(|e| e.to_string());
        if matches!(last_ax, Ok(true)) {
            return Ok(());
        }
        if matches!(last_cg, Ok(true)) {
            cg_seen = true;
        }
        let at = elapsed_ms();
        if at >= timeout_ms {
            break;
        }
        let next = started.saturating_add(sample_ms).min(timeout_ms);
        sleep_ms(next.saturating_sub(at));
    }
    match last_ax {
        Err(ax_error) => Err(McError::Blocked(format!(
            "Mission Control open signals were unreadable at the {timeout_ms}ms deadline (last AX error: {ax_error}; last WindowManager CG error: {}); refusing without a presence proof",
            last_cg.err().unwrap_or_else(|| "none".to_owned())
        ))),
        Ok(true) => Ok(()),
        Ok(false) if cg_seen => Err(McError::Blocked(format!(
            "Mission Control AX group never appeared within {timeout_ms}ms while the exact WindowManager CG presentation (layer>=0) did; CG presence alone cannot authorize the press"
        ))),
        Ok(false) => match last_cg {
            Err(cg_error) => Err(McError::Blocked(format!(
                "Mission Control WindowManager CG presentation (layer>=0) was unreadable at the {timeout_ms}ms deadline (last error: {cg_error}); refusing without a presence proof"
            ))),
            Ok(_) => Err(McError::Blocked(format!(
                "Mission Control AX group and WindowManager CG presentation (layer>=0) both read absent through {timeout_ms}ms"
            ))),
        },
    }
}

/// Readiness wait selected for one pinned goto leg. The ordinary pinned
/// route keeps the AX-only 2 s poll, while the native detached select and
/// the signed detached fixture goto sample the WindowManager presentation
/// alongside AX and then wait up to the existing 8 s action bound for AX
/// alone to authorize. Keeping the choice a value makes the route itself
/// reviewable and regression-testable instead of a bare function pointer.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinnedOpenRoute {
    AxOnlyPoll,
    DualSourceSample,
}

/// The signed detached fixture goto route: dual-source sampling to the
/// 8 s action bound. The capture3 replay saw WindowManager presentation at
/// 60 ms and AX at 307 ms, so this route observes the early CG signal and
/// still requires the later AX presence.
#[cfg(any(target_os = "macos", test))]
pub(crate) const fn fixture_goto_open_route() -> PinnedOpenRoute {
    PinnedOpenRoute::DualSourceSample
}

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

/// Exact full-topology preflight for the pinned multi-display MC select:
/// before opening and again before the single press, every display row
/// must still have a readable identifier/current and the same count, order,
/// membership, and currents as the owner's prior pre-snapshot. Drift or an
/// unprovable non-target row refuses without a select press (never actuating
/// into a proof that cannot pass).
#[cfg(any(target_os = "macos", test))]
pub(crate) fn topology_preflight_matches(
    prior: &TopologySnapshot,
    fresh: &TopologySnapshot,
) -> Result<(), McError> {
    if prior.displays.len() != fresh.displays.len() {
        return Err(McError::Blocked(format!(
            "preflight display count changed (was {}, now {}); refusing before select press",
            prior.displays.len(),
            fresh.displays.len()
        )));
    }
    for (index, (before, now)) in prior.displays.iter().zip(&fresh.displays).enumerate() {
        let Some(before_id) = before.identifier.as_deref() else {
            return Err(McError::Blocked(format!(
                "preflight display {index} identity unreadable in the owner pre-snapshot; refusing before select press"
            )));
        };
        let Some(now_id) = now.identifier.as_deref() else {
            return Err(McError::Blocked(format!(
                "preflight display {index} identity unreadable; refusing before select press"
            )));
        };
        if before_id != now_id
            || before.spaces != now.spaces
            || before.current != now.current
            || before.current.is_none()
            || now.current.is_none()
        {
            return Err(McError::Blocked(format!(
                "preflight topology drifted for display {index}; refusing before select press"
            )));
        }
    }
    Ok(())
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

    /// Exact-verified PID for one MC surface: exactly one exact bundle
    /// entry in the running-application table (never guessed). `None` means
    /// the surface is not running; ambiguous or reserved identities refuse
    /// as ownership failures instead of falling through.
    fn exact_surface_pid(surface: McSurface) -> Result<Option<i32>, McError> {
        let mut found = Vec::new();
        for (pid, bundle) in crate::ffi::running_apps() {
            if bundle == surface.bundle_id() {
                found.push(pid);
            }
        }
        match found.len() {
            1 if found[0] > 1 => Ok(Some(found[0])),
            1 => Err(McError::Ownership(format!(
                "refusing reserved {} pid",
                surface.bundle_id()
            ))),
            0 => Ok(None),
            _ => Err(McError::Ownership(format!(
                "ambiguous {} identity ({} entries); refusing to guess",
                surface.bundle_id(),
                found.len()
            ))),
        }
    }

    /// Owning surface identity for inspection: the surface that actually
    /// owns a present `mc.display` tree, else a running surface when no
    /// tree is present. WindowManager merely running is never reported as
    /// the owner while the Dock owns the open tree.
    pub fn mc_surface_identity() -> Result<(McSurface, i32), McError> {
        if let Some((surface, _)) = surface_displays()?
            && let Some(pid) = exact_surface_pid(surface)?
        {
            return Ok((surface, pid));
        }
        for surface in McSurface::ALL {
            if let Some(pid) = exact_surface_pid(surface)? {
                return Ok((surface, pid));
            }
        }
        Err(McError::Unsupported(
            "no live com.apple.WindowManager or com.apple.dock entry in the running-application table"
                .to_owned(),
        ))
    }

    /// Surface AX element with a FRESH exact bundle verification on every
    /// traversal (never the inspect-time PID alone), plus a finite messaging
    /// timeout before any read. `None` only when the exact-verified process
    /// has no addressable application element (surface absent).
    fn surface_element(
        surface: McSurface,
        pid: i32,
    ) -> Result<Option<crate::ffi::AxElement>, McError> {
        let bundle = crate::ffi::bundle_id_for_pid(pid).unwrap_or_default();
        super::require_mc_surface_bundle(surface, &bundle)?;
        let Some(element) = crate::ffi::AxElement::application(pid) else {
            return Ok(None);
        };
        let status = crate::ffi::ax_set_messaging_timeout(&element, MC_AX_TIMEOUT_SECS);
        if status != 0 {
            return Err(McError::System(format!(
                "{} AX messaging timeout unset: {}",
                surface.bundle_id(),
                crate::ffi::ax_error_name(status)
            )));
        }
        Ok(Some(element))
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

    /// Every exact-identifier child (bounded by the surface's AXChildren).
    fn children_with_id(
        element: &crate::ffi::AxElement,
        want: &str,
    ) -> Result<Vec<crate::ffi::AxElement>, McError> {
        let mut found = Vec::new();
        for child in crate::ffi::ax_children(element).map_err(|code| {
            McError::System(format!(
                "AXChildren unreadable: {}",
                crate::ffi::ax_error_name(code)
            ))
        })? {
            match crate::ffi::ax_identifier(&child) {
                Ok(Some(id)) if id == want => found.push(child),
                Ok(_) => {}
                Err(code) => {
                    return Err(McError::System(format!(
                        "AXIdentifier unreadable: {}",
                        crate::ffi::ax_error_name(code)
                    )));
                }
            }
        }
        Ok(found)
    }

    /// Exact-bundle-verified `mc.display` nodes for one surface. `Absent`
    /// covers a surface that is not running, has no addressable application
    /// element, or exposes no `mc.display`; AX read failures stay errors so
    /// routing can classify them (WindowManager `System` is routing-inert,
    /// Dock errors and ownership refusals are fatal).
    fn probe_surface(
        surface: McSurface,
    ) -> Result<McSurfaceExposure<Vec<crate::ffi::AxElement>>, McError> {
        let Some(pid) = exact_surface_pid(surface)? else {
            return Ok(McSurfaceExposure::Absent);
        };
        let Some(app) = surface_element(surface, pid)? else {
            return Ok(McSurfaceExposure::Absent);
        };
        let owner = if surface.direct_display() {
            app
        } else {
            match child_with_id(&app, MC_ID_ROOT)? {
                Some(root) => root,
                None => return Ok(McSurfaceExposure::Absent),
            }
        };
        let displays = children_with_id(&owner, MC_ID_DISPLAY)?;
        if displays.is_empty() {
            Ok(McSurfaceExposure::Absent)
        } else {
            Ok(McSurfaceExposure::Present(displays))
        }
    }

    /// Fresh `mc.display` nodes and the exact surface that owns them.
    /// Selection prefers WindowManager and falls through to the Dock only
    /// when WindowManager is absent or its application/child AX reads are
    /// unavailable; the selected surface is recorded as the presentation
    /// owner.
    fn surface_displays() -> Result<Option<(McSurface, Vec<crate::ffi::AxElement>)>, McError> {
        let selected = select_mc_surface(probe_surface(McSurface::WindowManager), || {
            probe_surface(McSurface::Dock)
        })?;
        if let Some((surface, _)) = &selected {
            record_mc_owner(*surface);
        }
        Ok(selected)
    }

    /// Read-only MC-group presence: `mc.display` under the preferred surface
    /// (WindowManager direct, else the Dock `mc` root).
    pub fn mc_group_present() -> Result<bool, McError> {
        Ok(surface_displays()?.is_some())
    }

    /// Fresh `mc.display` nodes owned by the selected surface (empty when
    /// Mission Control is not displayed).
    fn mc_display_children() -> Result<Vec<crate::ffi::AxElement>, McError> {
        Ok(surface_displays()?
            .map(|(_, displays)| displays)
            .unwrap_or_default())
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

    /// MC-surface windows above the subnormal desktop seam: the presentation
    /// overlay, thumbnails, and buttons that exist only while Mission
    /// Control is displayed (observed on the WindowManager route: layers
    /// 14/19 overlay plus layer-0 controls; closed leaves only the subnormal
    /// wallpaper windows). The AX presence group disappears before this CG
    /// transform settles, and those windows can still be Space members when
    /// an emptiness read runs, so a close is not verified on presence
    /// absence alone.
    ///
    /// The CG read is keyed on the surface that actually owned the open MC
    /// AX tree ([`mc_owner`]), never on WindowManager merely running. The
    /// Dock owns persistent layer-0+ chrome (its bar), so a bare layer test
    /// cannot prove a Dock-owned overlay is gone: Dock-owned and not-yet-
    /// observed trees verify on the fresh AX presence group alone (the
    /// historical macOS 15 behavior), while a WindowManager-owned tree checks
    /// `layer >= 0` for that exact PID with a PID-reuse fence.
    fn mc_presentation_windows_present() -> Result<bool, McError> {
        let Some(owner) = mc_presentation_cg_owner(mc_owner()) else {
            return Ok(false);
        };
        surface_presentation_present(owner)
    }

    /// Independent, owner-cache-free WindowManager presentation read: the
    /// exact `com.apple.WindowManager` identity's own `layer >= 0` windows,
    /// PID-reuse fenced. Fixture teardown uses it to refuse removal when the
    /// AX group read absent but the CG presentation still shows.
    pub fn mc_windowmanager_presentation_present() -> Result<bool, McError> {
        surface_presentation_present(McSurface::WindowManager)
    }

    /// Exact-identity presentation read for one surface: at least one CG
    /// window of that surface's exact current PID at `layer >= 0`, fenced so
    /// an identity change during the read voids the proof. A not-running
    /// surface owns no window.
    fn surface_presentation_present(surface: McSurface) -> Result<bool, McError> {
        let Some(pid) = exact_surface_pid(surface)? else {
            // The surface is gone, so it owns no lingering windows.
            return Ok(false);
        };
        let pid_u32 = u32::try_from(pid).map_err(|_| {
            McError::System(format!(
                "{} pid {pid} out of range for the window list",
                surface.bundle_id()
            ))
        })?;
        let windows = crate::ffi::cg_window_list_basic()
            .map_err(|e| McError::Unsupported(format!("CG window list unreadable: {e}")))?;
        // PID-reuse fence: the identity that authorized this read must still
        // hold after it; any drift voids the verification.
        if exact_surface_pid(surface)? != Some(pid) {
            return Err(McError::Blocked(format!(
                "{} identity changed during the presentation read; refusing",
                surface.bundle_id()
            )));
        }
        Ok(windows.iter().any(|w| w.pid == pid_u32 && w.layer >= 0))
    }

    /// Verified Mission Control absence: the AX presence group is absent AND
    /// no MC-surface presentation window remains above the subnormal desktop
    /// seam on the WindowManager route (Dock fallback: presence alone; see
    /// [`Self::mc_presentation_windows_present`]). Read failures propagate
    /// (never read as closed).
    pub fn mc_closed_verified() -> Result<bool, McError> {
        if mc_group_present()? {
            return Ok(false);
        }
        Ok(!mc_presentation_windows_present()?)
    }

    /// Bounded poll for verified absence (same 2 s shape and slice as
    /// [`poll_group`]; read failures are poll data, the deadline names the
    /// verdict).
    fn await_mc_closed() -> bool {
        poll_until_ms(MC_GROUP_POLL_MS, 100, || {
            mc_closed_verified().ok().filter(|closed| *closed)
        })
        .unwrap_or(false)
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

    /// Dual-source open used only by the native detached select path: the
    /// same presence-gated, one-toggle open as [`mc_open`], but the
    /// readiness wait samples AX presence and the exact WindowManager
    /// `layer >= 0` presentation together to the existing action bound.
    /// Only AX presence authorizes readiness; a CG-only positive is held to
    /// the deadline and refused, and terminal unreadability is distinguished
    /// from proven absence. The pre-open read still decides whether any
    /// toggle is sent.
    fn mc_open_dual() -> Result<bool, McError> {
        let pre_ax = mc_group_present();
        if let Err(error) = pre_ax {
            return Err(McError::Blocked(format!(
                "Mission Control open refused: AX presence unreadable before the toggle ({error}); not toggling a surface that cannot be proven absent"
            )));
        }
        if pre_ax == Ok(true) {
            return Ok(false);
        }
        action_gates()?;
        let launched = std::time::Instant::now();
        if !crate::ffi::mc_launch_toggle() {
            return Err(McError::System(
                "Mission Control launch not sent (NSWorkspace refused)".to_owned(),
            ));
        }
        super::await_open_until_ax(
            MC_ACTION_TIMEOUT_MS,
            super::MC_OPEN_SAMPLE_MS,
            || u64::try_from(launched.elapsed().as_millis()).unwrap_or(u64::MAX),
            |ms| std::thread::sleep(std::time::Duration::from_millis(ms)),
            || (mc_group_present(), mc_windowmanager_presentation_present()),
        )?;
        Ok(true)
    }

    impl PinnedOpenRoute {
        /// The one open step this route dispatches. Ordinary pinned routes
        /// keep [`mc_open`]; the native detached select and the signed
        /// detached fixture goto use [`mc_open_dual`].
        fn open(self) -> fn() -> Result<bool, McError> {
            match self {
                Self::AxOnlyPoll => mc_open,
                Self::DualSourceSample => mc_open_dual,
            }
        }
    }

    /// Close iff displayed, verified against both MC signals before
    /// returning: presence absent AND no presentation windows left, bounded
    /// by the existing 2 s poll. Presence is always read first, so a close
    /// is never toggled into an absent surface (which would reopen it); an
    /// already-absent surface with a lingering transform is waited out, not
    /// toggled. Returns `true` when this call sent the close.
    pub fn mc_close() -> Result<bool, McError> {
        if mc_group_present()? {
            action_gates()?;
            if !crate::ffi::mc_launch_toggle() {
                return Err(McError::System(
                    "Mission Control close launch not sent (NSWorkspace refused)".to_owned(),
                ));
            }
            if await_mc_closed() {
                Ok(true)
            } else {
                Err(McError::System(format!(
                    "Mission Control close did not verify within {MC_GROUP_POLL_MS}ms (presence or presentation windows remained)"
                )))
            }
        } else if await_mc_closed() {
            Ok(false)
        } else {
            Err(McError::System(format!(
                "Mission Control presence is absent but its presentation windows remained for {MC_GROUP_POLL_MS}ms"
            )))
        }
    }

    /// Once MC is present, its exact action child can still be absent while
    /// AppKit is constructing the display strip. Read only until it is
    /// ready; never retry a press.
    fn await_mc_child(
        mut read: impl FnMut() -> Result<crate::ffi::AxElement, McError>,
    ) -> Result<crate::ffi::AxElement, McError> {
        use std::time::{Duration, Instant};
        action_gates()?;
        let deadline = Instant::now() + Duration::from_millis(MC_GROUP_POLL_MS);
        loop {
            let result = read();
            let remaining = deadline.saturating_duration_since(Instant::now());
            match result {
                Ok(child) if !remaining.is_zero() => return Ok(child),
                Err(e @ (McError::Ownership(_) | McError::Unsupported(_))) => return Err(e),
                result => {
                    if !remaining.is_zero() {
                        std::thread::sleep(remaining.min(Duration::from_millis(25)));
                        if Instant::now() < deadline {
                            continue;
                        }
                    }
                    let cause = result.err().map_or_else(String::new, |e| format!(": {e}"));
                    return Err(McError::Blocked(format!(
                        "Mission Control action child was not ready within {MC_GROUP_POLL_MS}ms{cause}"
                    )));
                }
            }
        }
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

    /// Fresh AX join of one display's `mc.spaces` group: the preferred
    /// surface's `mc.display` with `AXDisplayID == cgid` → `mc.spaces`
    /// (WindowManager direct, else Dock `mc` root).
    fn spaces_group(cgid: u32) -> Result<crate::ffi::AxElement, McError> {
        let displays = mc_display_children()?;
        if displays.is_empty() {
            return Err(McError::Blocked(format!(
                "no mc.display owned by {} or {}; refusing without an exact display join",
                McSurface::WindowManager.bundle_id(),
                McSurface::Dock.bundle_id()
            )));
        }
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

    fn await_add_child(cgid: u32) -> Result<crate::ffi::AxElement, McError> {
        await_mc_child(|| {
            child_with_id(&spaces_group(cgid)?, MC_ID_SPACES_ADD)?
                .ok_or_else(|| McError::System("mc.spaces.add missing for display".to_owned()))
        })
    }

    fn await_list_child(
        cgid: u32,
        index: usize,
        expected_count: usize,
    ) -> Result<crate::ffi::AxElement, McError> {
        await_mc_child(|| {
            let children = spaces_list_children(&spaces_group(cgid)?)?;
            if children.len() != expected_count {
                return Err(McError::Blocked(format!(
                    "AX list count {} != SLS row count {expected_count}; refusing index dispatch",
                    children.len()
                )));
            }
            children.into_iter().nth(index).ok_or_else(|| {
                McError::Blocked("Mission Control target Space child is missing".to_owned())
            })
        })
    }

    /// Fresh SLS rows → snapshot.
    pub fn fresh_snapshot() -> Result<TopologySnapshot, McError> {
        let rows = crate::ffi::sls_display_rows()
            .map_err(|e| McError::Unsupported(format!("SLS display rows unreadable: {e}")))?;
        Ok(TopologySnapshot::from_rows(&rows))
    }

    /// Fresh unlocked-session verdict plus observed AX trust before opening,
    /// during readiness traversal, and immediately before a press.
    pub fn action_gates() -> Result<(), McError> {
        super::require_unlocked_session(&crate::ffi::session_state())?;
        super::require_ax_trust(crate::ffi::ax_is_process_trusted())?;
        Ok(())
    }

    /// Close attempt folded into leg endings: the press kind decides whether
    /// a dismissal is already in flight (goto) or MC is expected displayed
    /// (create/remove), and a presence-checked close plus a bounded verified
    /// absence is recorded. Never turns a sent press into `Err`, and keeps
    /// the shortfall contract exact: `mc_open == false` iff `close_error ==
    /// None`, both meaning absence was verified.
    fn finish_press(
        mut outcome: super::PressOutcome,
        dismissal: super::McDismissal,
    ) -> super::PressOutcome {
        match close_after_press(dismissal) {
            Ok(()) => {
                outcome.mc_open = false;
                outcome.close_error = None;
            }
            Err(e) => {
                // Re-read once: the surface may have settled while the
                // bounded attempt was failing.
                outcome.mc_open = !mc_closed_verified().unwrap_or(false);
                outcome.close_error = outcome.mc_open.then(|| e.to_string());
            }
        }
        outcome
    }

    /// Post-press close: never toggles a verified-absent surface, never
    /// toggles an auto-dismissing press before its dismissal was observed,
    /// and waits out a presented-but-unsettled surface instead of reopening
    /// it. At most one presence-checked close is sent; the shortfall names
    /// what was still visible at the deadline.
    fn close_after_press(dismissal: super::McDismissal) -> Result<(), McError> {
        let verified = mc_closed_verified().unwrap_or(false);
        let present = if verified {
            None
        } else {
            mc_group_present().ok()
        };
        match super::decide_close(dismissal, verified, present) {
            super::CloseDecision::Done => Ok(()),
            super::CloseDecision::Toggle => mc_close().map(|_| ()),
            super::CloseDecision::Await => {
                if await_mc_closed() {
                    return Ok(());
                }
                // The post-press AX surface may have re-rendered after the
                // transient: only a presence-true read justifies one
                // presence-checked close from here.
                if mc_group_present()? {
                    mc_close().map(|_| ())
                } else {
                    Err(McError::System(format!(
                        "Mission Control presence is absent but its presentation windows remained for {MC_GROUP_POLL_MS}ms; not toggling (would reopen)"
                    )))
                }
            }
        }
    }

    /// Shared leg skeleton: budgeted open → caller waits for its exact child
    /// and presses → finish (close attempt folded into the outcome).
    /// Error paths close MC only when this leg opened it.
    fn run_mc_leg(
        open_spend: &str,
        dismissal: super::McDismissal,
        budget: &mut ActionBudget,
        cleanup: bool,
        press: impl FnOnce(&mut ActionBudget) -> Result<super::PressOutcome, McError>,
    ) -> Result<super::PressOutcome, McError> {
        run_mc_leg_opened(open_spend, dismissal, budget, cleanup, mc_open, press)
    }

    /// [`run_mc_leg`] with the open step supplied: the native detached route
    /// swaps only its readiness wait (dual-source sampling), while every
    /// ordinary route keeps [`mc_open`].
    fn run_mc_leg_opened(
        open_spend: &str,
        dismissal: super::McDismissal,
        budget: &mut ActionBudget,
        cleanup: bool,
        open: impl FnOnce() -> Result<bool, McError>,
        press: impl FnOnce(&mut ActionBudget) -> Result<super::PressOutcome, McError>,
    ) -> Result<super::PressOutcome, McError> {
        super::spend_leg(budget, cleanup, open_spend)?;
        let opened = open()?;
        match press(budget) {
            Ok(outcome) => Ok(finish_press(outcome, dismissal)),
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
        run_mc_leg(
            "add-open",
            super::McDismissal::Manual,
            budget,
            cleanup,
            |b| mc_add_pressed(&await_add_child(cgid)?, b, cleanup),
        )
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
        run_mc_leg(
            "add-open",
            super::McDismissal::Manual,
            budget,
            cleanup,
            |b| {
                let add = await_add_child(cgid)?;
                let fresh = fresh_snapshot()?;
                let (cgid_fresh, _) =
                    display_cgid_of_pinned(&fresh, display, pinned_cgid, pinned_uuid)?;
                if cgid_fresh != cgid {
                    return Err(McError::Blocked(format!(
                        "CG display ID changed during open (was {cgid}, now {cgid_fresh}); refusing stale dispatch"
                    )));
                }
                mc_add_pressed(&add, b, cleanup)
            },
        )
    }

    fn mc_add_pressed(
        add: &crate::ffi::AxElement,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        super::spend_leg(budget, cleanup, "add-press")?;
        action_gates()?;
        crate::ffi::ax_perform_action(add, MC_ACTION_PRESS).map_err(|code| {
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
        if !cleanup {
            crate::space_synthetic_gate::require_mc_fallback()
                .map_err(|e| McError::Blocked(format!("MC selection refused: {e}")))?;
        }
        let snap0 = fresh_snapshot()?;
        let (d0, _) = snap0.locate(sid).ok_or_else(|| {
            McError::Ownership(format!(
                "Space {sid} not uniquely located before goto; ownership uncertain — refusing"
            ))
        })?;
        let (cgid, _) = display_cgid_of(&snap0, d0)?;
        run_mc_leg(
            "goto-open",
            super::McDismissal::Auto,
            budget,
            cleanup,
            |b| mc_goto_pressed(sid, &snap0, cgid, b, cleanup),
        )
    }

    fn mc_goto_pressed(
        sid: u64,
        snap0: &TopologySnapshot,
        cgid: u32,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        let (d0, i0) = snap0.locate(sid).ok_or_else(|| {
            McError::Ownership(format!("Space {sid} lost its pre-open join; refusing"))
        })?;
        let target = await_list_child(cgid, i0, snap0.displays[d0].spaces.len())?;
        let snap1 = fresh_snapshot()?;
        let (d1, i1) = snap1.locate(sid).ok_or_else(|| {
            McError::Ownership(format!(
                "Space {sid} moved during open; ownership uncertain — refusing"
            ))
        })?;
        if (d1, i1) != (d0, i0) || snap1.displays[d1] != snap0.displays[d0] {
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
        super::spend_leg(budget, cleanup, "goto-press")?;
        action_gates()?;
        crate::ffi::ax_perform_action(&target, MC_ACTION_PRESS).map_err(|code| {
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

    /// Go to `sid` on a caller-pinned display WITHOUT the global Dock
    /// swipe: the same exact AX Space press as [`mc_goto_sid`], but the
    /// displayed Mission Control surface is joined by the pinned
    /// `(display_index, CG display id, managed UUID)` triple rather than a
    /// fresh SLS locate + AppKit read. The pinned row must still own the
    /// target before open and after the fresh re-read; any display move,
    /// membership drift, or CG/UUID mismatch refuses before the press.
    /// Used by the serialized select owner for grouped indicator clicks
    /// and for eligible macOS 27 zero-post fallbacks.
    pub fn mc_goto_sid_pinned(
        sid: u64,
        display_index: usize,
        pinned_cgid: u32,
        pinned_uuid: &str,
        prior: &TopologySnapshot,
        budget: &mut ActionBudget,
        cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        mc_goto_pinned_after(
            sid, (display_index, pinned_cgid, pinned_uuid), prior, budget, cleanup, || {
                if cleanup { Ok(()) } else { crate::space_synthetic_gate::require_mc_fallback() }
            },
            None,
        )
    }

    /// Go to the exact typed target on the caller-pinned display when the
    /// pinned source current or the requested target is a detached fullscreen
    /// surface. The ordinary all-user fallback fence is replaced only for
    /// this route: both exact SIDs must stay uniquely on the pinned row with
    /// their typed classes and every non-pinned display's current must read
    /// user, checked before Mission Control opens and again at the press
    /// fence. Any drift, duplicate, off-row SID, or non-user current on
    /// another display refuses without a press. When the fixture supplied an
    /// exact registered member, that same owner/member proof is repeated
    /// immediately before the press: typed SID/index checks alone cannot see
    /// the member leave while its surface SID remains.
    #[allow(clippy::too_many_arguments)] // ordered pinned leg inputs, one caller
    pub fn mc_goto_sid_pinned_detached(
        display_index: usize,
        pinned_cgid: u32,
        pinned_uuid: &str,
        prior: &TopologySnapshot,
        source_physical: crate::runtime_types::SelectPhysical,
        target_physical: crate::runtime_types::SelectPhysical,
        registered_member: Option<(u64, u32, u32)>,
        budget: &mut ActionBudget,
    ) -> Result<super::PressOutcome, McError> {
        pinned_goto_leg(
            target_physical.sid(),
            (display_index, pinned_cgid, pinned_uuid),
            prior,
            budget,
            false,
            || Ok(()),
            Some((source_physical, target_physical)),
            registered_member,
            PinnedOpenRoute::DualSourceSample,
        )
    }

    /// Ordinary fixture goto: keep the existing AX-only open policy.
    pub(crate) fn fixture_goto_pinned(
        sid: u64,
        pin: (usize, u32, &str),
        prior: &TopologySnapshot,
        budget: &mut ActionBudget,
        fixture_source: impl FnOnce() -> Result<(), String>,
    ) -> Result<super::PressOutcome, McError> {
        mc_goto_pinned_after(sid, pin, prior, budget, false, fixture_source, None)
    }

    /// Signed detached acceptance fixture goto with the same 8 s dual-source
    /// open as the native detached selection. AX alone authorizes progress.
    pub(crate) fn fixture_acceptance_goto_pinned(
        sid: u64,
        pin: (usize, u32, &str),
        prior: &TopologySnapshot,
        budget: &mut ActionBudget,
        fixture_source: impl FnOnce() -> Result<(), String>,
    ) -> Result<super::PressOutcome, McError> {
        pinned_goto_leg(
            sid,
            pin,
            prior,
            budget,
            false,
            fixture_source,
            None,
            None,
            fixture_goto_open_route(),
        )
    }

    fn mc_goto_pinned_after(
        sid: u64,
        pin: (usize, u32, &str),
        prior: &TopologySnapshot,
        budget: &mut ActionBudget,
        cleanup: bool,
        source: impl FnOnce() -> Result<(), String>,
        detached: Option<(
            crate::runtime_types::SelectPhysical,
            crate::runtime_types::SelectPhysical,
        )>,
    ) -> Result<super::PressOutcome, McError> {
        pinned_goto_leg(
            sid,
            pin,
            prior,
            budget,
            cleanup,
            source,
            detached,
            None,
            PinnedOpenRoute::AxOnlyPoll,
        )
    }

    /// One pinned goto leg: exact pinned preflight → the selected open →
    /// the exact pre-press fence. Every dispatch check is shared; the
    /// route value swaps only the readiness wait. A registered fixture
    /// detached member is re-proved from fresh reads immediately before
    /// the press because the asynchronous open gives it time to leave its
    /// surface SID.
    #[allow(clippy::too_many_arguments)] // one leg, explicit ordered inputs
    fn pinned_goto_leg(
        sid: u64,
        (display_index, pinned_cgid, pinned_uuid): (usize, u32, &str),
        prior: &TopologySnapshot,
        budget: &mut ActionBudget,
        cleanup: bool,
        source: impl FnOnce() -> Result<(), String>,
        detached: Option<(
            crate::runtime_types::SelectPhysical,
            crate::runtime_types::SelectPhysical,
        )>,
        registered_member: Option<(u64, u32, u32)>,
        route: PinnedOpenRoute,
    ) -> Result<super::PressOutcome, McError> {
        action_gates()?;
        if detached.is_none() {
            source().map_err(|e| McError::Blocked(format!("MC selection refused: {e}")))?;
        }
        let snap0 = fresh_snapshot()?;
        topology_preflight_matches(prior, &snap0)?;
        if let Some((source_physical, target_physical)) = detached {
            crate::space_synthetic_gate::require_mc_detached_selection(
                &snap0.displays,
                pinned_uuid,
                source_physical,
                target_physical,
            )
            .map_err(|e| McError::Blocked(format!("MC selection refused: {e}")))?;
        }
        let (cgid, _) = display_cgid_of_pinned(&snap0, display_index, pinned_cgid, pinned_uuid)?;
        let (d0, i0) = snap0.locate(sid).ok_or_else(|| {
            McError::Ownership(format!(
                "Space {sid} not uniquely located before pinned goto; ownership uncertain — refusing"
            ))
        })?;
        if d0 != display_index || snap0.displays[d0].identifier.as_deref() != Some(pinned_uuid) {
            return Err(McError::Blocked(format!(
                "Space {sid} is not on the pinned display {pinned_uuid}; refusing pinned goto"
            )));
        }
        run_mc_leg_opened(
            "goto-open",
            super::McDismissal::Auto,
            budget,
            cleanup,
            route.open(),
            |b| {
                let target = await_list_child(cgid, i0, snap0.displays[d0].spaces.len())?;
                let snap1 = fresh_snapshot()?;
                topology_preflight_matches(prior, &snap1)?;
                let (d1, i1) = snap1.locate(sid).ok_or_else(|| {
                    McError::Ownership(format!(
                        "Space {sid} moved during open; ownership uncertain — refusing"
                    ))
                })?;
                let (cgid_fresh, _) = display_cgid_of_pinned(&snap1, d1, pinned_cgid, pinned_uuid)?;
                if (d1, i1) != (d0, i0)
                    || snap1.displays[d1].identifier != snap0.displays[d0].identifier
                    || cgid_fresh != cgid
                {
                    return Err(McError::Blocked(format!(
                        "pinned topology drifted during open (was display {d0} index {i0}, now {d1}/{i1}); refusing stale dispatch"
                    )));
                }
                if let Some((source_physical, target_physical)) = detached {
                    crate::space_synthetic_gate::require_mc_detached_selection(
                        &snap1.displays,
                        pinned_uuid,
                        source_physical,
                        target_physical,
                    )
                    .map_err(|e| McError::Blocked(format!("MC selection refused: {e}")))?;
                }
                super::spend_leg(b, cleanup, "goto-press")?;
                action_gates()?;
                crate::space_synthetic_gate::require_registered_detached_member(
                    registered_member,
                    crate::space_synthetic_gate::detached_surface_exact,
                )
                .map_err(|e| McError::Blocked(format!("MC selection refused: {e}")))?;
                crate::ffi::ax_perform_action(&target, MC_ACTION_PRESS).map_err(|code| {
                    McError::Dispatch(format!(
                        "pinned goto press refused: {}",
                        crate::ffi::ax_error_name(code)
                    ))
                })?;
                Ok(super::PressOutcome {
                    after: fresh_snapshot().ok(),
                    mc_open: true,
                    close_error: None,
                })
            },
        )
    }

    /// Remove `sid` via `AXRemoveDesktop`. Same SID-accepting,
    /// drift-rejecting shape as goto.
    pub fn mc_remove_sid(
        sid: u64,
        budget: &mut ActionBudget,
        cleanup: bool,
        verify: impl FnOnce(&TopologySnapshot) -> Result<(), McError>,
    ) -> Result<super::PressOutcome, McError> {
        action_gates()?;
        let snap0 = fresh_snapshot()?;
        let (d0, _) = snap0.locate(sid).ok_or_else(|| {
            McError::Ownership(format!(
                "Space {sid} not uniquely located before remove; ownership uncertain — cleanup blocked"
            ))
        })?;
        let (cgid, _) = display_cgid_of(&snap0, d0)?;
        run_mc_leg(
            "remove-open",
            super::McDismissal::Manual,
            budget,
            cleanup,
            |b| mc_remove_pressed(sid, &snap0, cgid, b, cleanup, verify),
        )
    }

    fn mc_remove_pressed(
        sid: u64,
        snap0: &TopologySnapshot,
        cgid: u32,
        budget: &mut ActionBudget,
        cleanup: bool,
        verify: impl FnOnce(&TopologySnapshot) -> Result<(), McError>,
    ) -> Result<super::PressOutcome, McError> {
        let (d0, i0) = snap0.locate(sid).ok_or_else(|| {
            McError::Ownership(format!("Space {sid} lost its pre-open join; refusing"))
        })?;
        let target = await_list_child(cgid, i0, snap0.displays[d0].spaces.len())?;
        let snap1 = fresh_snapshot()?;
        let (d1, i1) = snap1.locate(sid).ok_or_else(|| {
            McError::Ownership(format!(
                "Space {sid} moved during open; ownership uncertain — cleanup blocked"
            ))
        })?;
        if (d1, i1) != (d0, i0) || snap1.displays[d1] != snap0.displays[d0] {
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
        verify(&snap1)?;
        super::spend_leg(budget, cleanup, "remove-press")?;
        action_gates()?;
        crate::ffi::ax_perform_action(&target, MC_ACTION_REMOVE_DESKTOP).map_err(|code| {
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

    pub fn mc_surface_identity() -> Result<(McSurface, i32), McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn mc_group_present() -> Result<bool, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn mc_closed_verified() -> Result<bool, McError> {
        unsupported("Mission Control lifecycle")
    }

    /// Non-macOS stub: the independent WindowManager presentation read has no
    /// live backend here.
    pub fn mc_windowmanager_presentation_present() -> Result<bool, McError> {
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

    pub fn mc_goto_sid_pinned(
        _sid: u64,
        _display_index: usize,
        _pinned_cgid: u32,
        _pinned_uuid: &str,
        _prior: &TopologySnapshot,
        _budget: &mut ActionBudget,
        _cleanup: bool,
    ) -> Result<super::PressOutcome, McError> {
        unsupported("Mission Control lifecycle")
    }

    #[allow(clippy::too_many_arguments)] // mirrors the macOS signature
    pub fn mc_goto_sid_pinned_detached(
        _display_index: usize,
        _pinned_cgid: u32,
        _pinned_uuid: &str,
        _prior: &TopologySnapshot,
        _source_physical: crate::runtime_types::SelectPhysical,
        _target_physical: crate::runtime_types::SelectPhysical,
        _registered_member: Option<(u64, u32, u32)>,
        _budget: &mut ActionBudget,
    ) -> Result<super::PressOutcome, McError> {
        unsupported("Mission Control lifecycle")
    }

    pub fn mc_remove_sid(
        _sid: u64,
        _budget: &mut ActionBudget,
        _cleanup: bool,
        _verify: impl FnOnce(&TopologySnapshot) -> Result<(), McError>,
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

/// Create effect helper: the append lands asynchronously (an observed add
/// press read unchanged immediately post-press), so poll fresh readbacks
/// within [`MC_EFFECT_SETTLE_MS`] until the exact-append validator accepts
/// one. Returns the accepted snapshot and new SID, or `None` at the
/// deadline. Poll only — the dispatched press is never replayed.
pub(crate) fn await_create_effect(
    before: &TopologySnapshot,
    display: usize,
) -> Option<(TopologySnapshot, u64)> {
    let t0 = std::time::Instant::now();
    loop {
        if let Ok(snapshot) = live::fresh_snapshot()
            && let Ok(sid) = validate_create_effect(before, &snapshot, display)
        {
            return Some((snapshot, sid));
        }
        if t0.elapsed() >= std::time::Duration::from_millis(MC_EFFECT_SETTLE_MS) {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Goto helper: SID-accepting dispatch plus exact-current readback against
/// the `from` baseline. A sent press settles asynchronously, so a failed
/// immediate readback polls fresh snapshots within [`MC_EFFECT_SETTLE_MS`]
/// (poll only — the dispatched press is never replayed) before the timeout
/// names the actual diff. Returns (post-goto snapshot, close shortfall);
/// `None` means the leg verified Mission Control absent (presence gone and
/// no presentation windows left), never merely a missing presence read.
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
/// dispatch, exact removal-effect readback. Returns (scan, close shortfall);
/// `None` means the leg verified Mission Control absent, never merely a
/// missing presence read.
/// Sets `remove_dispatched` once a press is sent, even when the effect stays
/// unverified — cleanup must never replay it.
pub(crate) fn remove_sid_verified(
    temp: u64,
    budget: &mut ActionBudget,
    cleanup: bool,
    remove_dispatched: &mut bool,
) -> Result<(crate::mc_probe::EmptySummary, Option<String>), McError> {
    let scan = verify_space_empty(temp)?;
    let close = remove_sid_dispatched(temp, budget, cleanup, remove_dispatched)?;
    Ok((scan, close))
}

/// Post-scan removal dispatch shared by the default emptiness path and the
/// receipt-bound owned recovery: fresh eligibility, SID-accepting press, and
/// exact removal-effect readback. The caller supplies the already-proven
/// emptiness authority (default scan or receipt-bound recovery proof), so
/// this core never re-derives or widens it. Returns the Mission Control close
/// shortfall; `None` means the leg verified Mission Control absent, never
/// merely a missing presence read.
/// Sets `remove_dispatched` once a press is sent, even when the effect stays
/// unverified — cleanup must never replay it.
pub(crate) fn remove_sid_dispatched(
    temp: u64,
    budget: &mut ActionBudget,
    cleanup: bool,
    remove_dispatched: &mut bool,
) -> Result<Option<String>, McError> {
    remove_sid_dispatched_checked(temp, budget, cleanup, remove_dispatched, |_| Ok(()))
}

/// Same dispatch with one caller-supplied pre-dispatch verification run
/// after the fresh eligibility/locate gates and immediately before the SID
/// press. The receipt-bound owned recovery uses it for its independent live
/// re-read; the default emptiness path passes the no-op above.
pub(crate) fn remove_sid_dispatched_checked<F>(
    temp: u64,
    budget: &mut ActionBudget,
    cleanup: bool,
    remove_dispatched: &mut bool,
    verify: F,
) -> Result<Option<String>, McError>
where
    F: FnOnce(&TopologySnapshot) -> Result<(), McError>,
{
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
    let outcome = live::mc_remove_sid(temp, budget, cleanup, verify)?;
    *remove_dispatched = true;
    let mut after = outcome.after.ok_or_else(|| {
        McError::Blocked(format!(
            "remove Space {temp}: post-press snapshot unreadable (MC open: {}); effect unknown — reporting residue, never replaying",
            outcome.mc_open
        ))
    })?;
    if validate_removal_effect(&pre, &after, temp).is_ok() {
        return Ok(outcome.close_error);
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
                    return Ok(outcome.close_error);
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
/// untrusted session is left alone). `mc_close` itself reads presence first
/// (never toggles an absent surface) and waits out a lingering presentation
/// transform, so every outcome here is a verified-absence verdict.
pub(crate) fn safe_close_mc(notes: &mut Vec<String>) {
    if live::action_gates().is_err() {
        notes.push(
            "MC close skipped: session gates fail (locked or untrusted); MC left as-is".to_owned(),
        );
        return;
    }
    match live::mc_close() {
        Ok(true) => notes.push("MC closed".to_owned()),
        Ok(false) => notes.push("MC already absent and verified".to_owned()),
        Err(e) => notes.push(format!(
            "MC close not verified: {e}; presence preserved in report"
        )),
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
    let (dock_pid, dock_bundle, dock_error) = match live::dock_pid() {
        Ok(pid) => {
            let bundle = crate::ffi::bundle_id_for_pid(pid).unwrap_or_default();
            match require_dock_bundle(&bundle) {
                Ok(()) => (Some(pid), Some(bundle), None),
                Err(e) => (Some(pid), Some(bundle), Some(e.to_string())),
            }
        }
        Err(e) => (None, None, Some(e.to_string())),
    };
    let (surface_pid, surface_bundle, presence, presence_error) = match live::mc_surface_identity()
    {
        Ok((surface, pid)) => {
            let bundle = crate::ffi::bundle_id_for_pid(pid).unwrap_or_default();
            let (present, err) = match require_mc_surface_bundle(surface, &bundle) {
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
        dock_error,
        mc_surface_pid: surface_pid,
        mc_surface_bundle: surface_bundle,
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
    // MC must read verified-absent at the end (presence AND no presentation
    // windows; the AX group disappears before the CG transform settles);
    // one safe close attempt, then verdict.
    let mut mc_absent = live::mc_closed_verified().unwrap_or(false);
    if !mc_absent {
        safe_close_mc(&mut ctx.notes);
        mc_absent = live::mc_closed_verified().unwrap_or(false);
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

#[cfg(test)]
mod tests {
    use super::{
        CloseDecision, MC_OPEN_SAMPLE_MS, McDismissal, PinnedOpenRoute, await_open_until_ax,
        decide_close, fixture_goto_open_route, topology_preflight_matches,
    };
    use crate::mc_probe::{
        DisplaySpaces, MC_ACTION_TIMEOUT_MS, MC_GROUP_POLL_MS, McError, TopologySnapshot,
    };

    fn topology(displays: Vec<DisplaySpaces>) -> TopologySnapshot {
        TopologySnapshot { displays }
    }

    fn display(uuid: &str, spaces: &[u64], current: Option<u64>) -> DisplaySpaces {
        DisplaySpaces {
            identifier: Some(uuid.to_owned()),
            spaces: spaces.to_vec(),
            current,
        }
    }

    #[test]
    fn pinned_preflight_rejects_drift_and_unprovable_non_targets() {
        let prior = topology(vec![
            display("display-a", &[1, 2], Some(1)),
            display("display-b", &[3, 4], Some(3)),
        ]);
        assert!(
            topology_preflight_matches(&prior, &prior.clone()).is_ok(),
            "the identical owner snapshot passes preflight"
        );
        // A non-target current move is refused before the select press.
        let moved = topology(vec![
            display("display-a", &[1, 2], Some(2)),
            display("display-b", &[3, 4], Some(3)),
        ]);
        assert!(topology_preflight_matches(&prior, &moved).is_err());
        // A reordered/renamed identity is refused.
        let reordered = topology(vec![
            display("display-b", &[3, 4], Some(3)),
            display("display-a", &[1, 2], Some(1)),
        ]);
        assert!(topology_preflight_matches(&prior, &reordered).is_err());
        // An unreadable non-target current or identity is not proof.
        let mut missing_current = prior.clone();
        missing_current.displays[0].current = None;
        assert!(topology_preflight_matches(&prior, &missing_current).is_err());
        let mut missing_identity = prior.clone();
        missing_identity.displays[0].identifier = None;
        assert!(topology_preflight_matches(&prior, &missing_identity).is_err());
        // A membership change is drift too.
        let mut membership = prior.clone();
        membership.displays[0].spaces.push(9);
        assert!(topology_preflight_matches(&prior, &membership).is_err());
    }

    #[test]
    fn close_only_toggles_a_demonstrably_present_surface() {
        // A create/remove press leaves MC displayed: one presence-checked
        // close is the only action.
        assert_eq!(
            decide_close(McDismissal::Manual, false, Some(true)),
            CloseDecision::Toggle
        );
        // Absent or unreadable presence is never a toggle signal; it waits
        // for a settle instead (a blind toggle can reopen a dismissal).
        assert_eq!(
            decide_close(McDismissal::Manual, false, Some(false)),
            CloseDecision::Await
        );
        assert_eq!(
            decide_close(McDismissal::Manual, false, None),
            CloseDecision::Await
        );
    }

    #[test]
    fn auto_dismissing_press_never_toggles_before_its_dismissal_verifies() {
        // A goto press dismisses MC by itself: even a presence-true read
        // taken during the dismissal must not toggle it back open.
        assert_eq!(
            decide_close(McDismissal::Auto, false, Some(true)),
            CloseDecision::Await
        );
        assert_eq!(
            decide_close(McDismissal::Auto, false, Some(false)),
            CloseDecision::Await
        );
    }

    #[test]
    fn verified_absence_is_done_even_if_presence_reads_stale() {
        // The verified predicate (presence + no presentation windows) wins
        // over a stale presence-true read, so a closed surface is never
        // toggled open.
        assert_eq!(
            decide_close(McDismissal::Manual, true, Some(true)),
            CloseDecision::Done
        );
        assert_eq!(
            decide_close(McDismissal::Auto, true, Some(false)),
            CloseDecision::Done
        );
    }

    /// Run the dual-source sampler with a clock that advances only by its
    /// own sleep requests, so no real time passes.
    fn run_dual(
        timeout_ms: u64,
        probe: impl FnMut() -> (Result<bool, McError>, Result<bool, McError>),
    ) -> (Result<(), McError>, u64, Vec<u64>) {
        let now = std::cell::Cell::new(0_u64);
        let sleeps = std::cell::RefCell::new(Vec::new());
        let outcome = await_open_until_ax(
            timeout_ms,
            MC_OPEN_SAMPLE_MS,
            || now.get(),
            |slice| {
                sleeps.borrow_mut().push(slice);
                now.set(now.get() + slice);
            },
            probe,
        );
        (outcome, now.get(), sleeps.into_inner())
    }

    #[test]
    fn dual_source_open_reaches_ax_after_earlier_cg() {
        // The CG presentation shows first; AX (the only readiness signal)
        // appears later and ends the wait.
        let mut reads = std::collections::VecDeque::from([
            (Ok(false), Ok(true)),
            (Ok(false), Ok(true)),
            (Ok(true), Ok(true)),
        ]);
        let (outcome, elapsed, sleeps) =
            run_dual(MC_ACTION_TIMEOUT_MS, || reads.pop_front().expect("scripted read"));
        outcome.expect("AX presence authorizes readiness");
        assert_eq!(elapsed, 500);
        assert_eq!(sleeps, [250, 250]);
    }

    #[test]
    fn dual_source_open_refuses_cg_only_and_read_errors_at_the_action_deadline() {
        assert_eq!(MC_ACTION_TIMEOUT_MS, 8_000);
        let (outcome, elapsed, sleeps) =
            run_dual(MC_ACTION_TIMEOUT_MS, || (Ok(false), Ok(true)));
        let error = outcome.expect_err("CG presence alone never authorizes the press");
        assert_eq!(elapsed, MC_ACTION_TIMEOUT_MS);
        assert_eq!(sleeps.len(), 32);
        assert!(
            error
                .to_string()
                .contains("CG presence alone cannot authorize the press")
        );

        // A CG positive seen earlier still cannot authorize a later unreadable.
        let mut cg = Some(Ok(true));
        let (outcome, ..) = run_dual(MC_ACTION_TIMEOUT_MS, || {
            (
                Ok(false),
                cg.take()
                    .unwrap_or(Err(McError::Blocked("CG read lost".to_owned()))),
            )
        });
        assert!(
            outcome
                .expect_err("a transient CG positive never authorizes")
                .to_string()
                .contains("CG presence alone")
        );

        // Unreadable AX is never absence and never a CG-only presentation.
        let (outcome, ..) = run_dual(MC_ACTION_TIMEOUT_MS, || {
            (
                Err(McError::Blocked("AX read refused".to_owned())),
                Ok(true),
            )
        });
        let message = outcome.expect_err("unreadable AX refuses").to_string();
        assert!(message.contains("unreadable"));
        assert!(message.contains("last AX error: blocked: AX read refused"));

        // Unreadable CG with AX proven absent is unreadable, not both-absent.
        let (outcome, ..) = run_dual(MC_ACTION_TIMEOUT_MS, || {
            (
                Ok(false),
                Err(McError::Unsupported("CG unreadable".to_owned())),
            )
        });
        let message = outcome.expect_err("unreadable CG refuses").to_string();
        assert!(message.contains("WindowManager CG presentation (layer>=0) was unreadable"));
        assert!(!message.contains("both read absent"));

        // Both readable-absent stays its own distinct refusal.
        let (outcome, ..) = run_dual(MC_ACTION_TIMEOUT_MS, || (Ok(false), Ok(false)));
        assert!(
            outcome
                .expect_err("both absent refuses")
                .to_string()
                .contains("both read absent")
        );
    }

    /// Run the dual-source sampler against first-positive timestamps on a
    /// virtual clock (no real waiting).
    fn replay_dual(ax_at_ms: u64, cg_at_ms: u64) -> (Result<(), McError>, u64) {
        let now = std::cell::Cell::new(0_u64);
        let outcome = await_open_until_ax(
            MC_ACTION_TIMEOUT_MS,
            MC_OPEN_SAMPLE_MS,
            || now.get(),
            |slice| now.set(now.get() + slice),
            || (Ok(now.get() >= ax_at_ms), Ok(now.get() >= cg_at_ms)),
        );
        (outcome, now.get())
    }

    /// Test mirror of the ordinary AX-only readiness poll (2 s bound, 100 ms
    /// slices) that the prior pinned route used, on the same virtual clock.
    fn replay_ax_only(ax_at_ms: u64) -> (Result<(), McError>, u64) {
        let mut now = 0_u64;
        loop {
            if now >= ax_at_ms {
                return (Ok(()), now);
            }
            if now >= MC_GROUP_POLL_MS {
                return (
                    Err(McError::Blocked(format!(
                        "Mission Control presence did not become true within {MC_GROUP_POLL_MS}ms"
                    ))),
                    now,
                );
            }
            now += 100;
        }
    }

    #[test]
    fn fixture_goto_route_samples_past_the_ax_only_bound_at_capture3_timings() {
        // The route chosen by the signed detached fixture goto is the
        // dual-source sampler, not the ordinary AX-only poll.
        assert_eq!(fixture_goto_open_route(), PinnedOpenRoute::DualSourceSample);

        // capture3: WindowManager presentation first positive at 60 ms, AX
        // group at 307 ms. The sampler sees the early CG signal but only AX
        // authorizes, at its next 250 ms sample.
        let (outcome, elapsed) = replay_dual(307, 60);
        outcome.expect("AX presence at 307ms authorizes the open");
        assert_eq!(elapsed, 500);
        let (prior, prior_elapsed) = replay_ax_only(307);
        prior.expect("the prior AX-only route also reaches a 307ms AX");
        assert_eq!(prior_elapsed, 400);

        // Stretch AX beyond the prior 2 s bound: the fixture sampler still
        // authorizes to its 8 s action bound, while the prior route times out.
        let (outcome, elapsed) = replay_dual(2_307, 60);
        outcome.expect("the 8 s sampler reaches a late AX");
        assert_eq!(elapsed, 2_500);
        let (prior, prior_elapsed) = replay_ax_only(2_307);
        prior.expect_err("the prior AX-only 2 s route cannot reach a 2307ms AX");
        assert_eq!(prior_elapsed, MC_GROUP_POLL_MS);
    }
}
