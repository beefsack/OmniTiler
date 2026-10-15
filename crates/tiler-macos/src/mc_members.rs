#![forbid(unsafe_code)]

//! Read-only Space-members inspection. Policy and shapes live in
//! [`crate::mc_probe`]; the live enumeration lives in
//! [`crate::mc_lifecycle`] (shared here).
//!
//! No launch, no writes, no synthetic posts, no companion process. One
//! call carries the complete bounded metadata the cleanup predicate needs:
//! every live member of the Space (shell chrome included) with identity,
//! geometry, and membership, plus the read-only focus-oracle legs and the
//! protected Ghostty snapshot for the same instant. IDs and geometry only —
//! never titles, content, or paths.

use crate::mc_lifecycle::{
    capture_focus_legs, enumerate_window_members, live, snapshot_ghostty_windows,
};
use crate::mc_probe::{McMembersReport, McSpaceLocation, ResidualMember};

/// Read-only members inspection for one Space SID. The session lock is
/// read fresh first (recorded, never changed); every leg after that is a
/// read. Unreadable topology still reports session, trust, focus, and
/// Ghostty state with the layout error named — reads never claim.
pub fn run_mc_members(space: u64) -> Result<String, (i32, String)> {
    let session = crate::ffi::session_state();
    let ax_trusted = crate::ffi::ax_is_process_trusted();
    let snapshot = live::fresh_snapshot();
    let (layout, layout_error, location) = match &snapshot {
        Ok(s) => {
            let location = s.locate(space).map(|(display, index)| {
                let display_current = s.displays.get(display).and_then(|d| d.current);
                McSpaceLocation {
                    display,
                    index,
                    display_current,
                    is_current: display_current == Some(space),
                    space_type: crate::ffi::sls_space_type(space)
                        .ok()
                        .map(|t| format!("{t:?}")),
                }
            });
            (Some(s.sanitized()), None, location)
        }
        Err(e) => (None, Some(e.to_string()), None),
    };
    let members = match enumerate_window_members() {
        Ok(all) => {
            let mut members: Vec<ResidualMember> = all
                .into_iter()
                .filter(|m| matches!(m.spaces, Ok(ref s) if s.contains(&space)))
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
                })
                .collect();
            members.sort_by_key(|m| m.wid);
            members
        }
        Err(e) => return Err((1, format!("members enumeration unreadable: {e}"))),
    };
    let focus = capture_focus_legs();
    let (ghostty, ghostty_error) = match snapshot_ghostty_windows() {
        Ok(g) => (g, None),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    let report = McMembersReport {
        space,
        session,
        ax_trusted,
        layout,
        layout_error,
        location,
        members,
        focus,
        ghostty,
        ghostty_error,
        synthetic_events_posted: false,
        mutates: false,
    };
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}
