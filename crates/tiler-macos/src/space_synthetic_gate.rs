//! Plain-data protection shared by native Space gestures and title-bar drags.
//! Callers prove the focused window and the target display's current Space
//! independently: the pointer display and another display's current Space
//! do not identify either one.

use crate::ffi::{PreimageRoute, SlsDisplayRow, SlsSpaceType};

/// Pure exact display-row join: a pinned native UUID must name exactly one
/// fresh SLS row (several is ambiguous), while an unpinned read joins the
/// caller-owned topology ordinal. Empty rows never name a display.
fn pinned_display_row<'a>(
    rows: &'a [SlsDisplayRow],
    display_index: usize,
    pinned_uuid: Option<&str>,
) -> Result<&'a SlsDisplayRow, String> {
    match pinned_uuid {
        Some(uuid) => {
            let mut named = rows
                .iter()
                .filter(|row| row.identifier.as_deref() == Some(uuid));
            let row = named
                .next()
                .ok_or_else(|| format!("pinned display UUID {uuid} has no fresh SLS row"))?;
            if named.next().is_some() {
                return Err(format!(
                    "pinned display UUID {uuid} maps to several fresh SLS rows; refusing"
                ));
            }
            Ok(row)
        }
        None => rows.get(display_index).ok_or_else(|| {
            format!(
                "display index {display_index} unknown ({} fresh SLS rows)",
                rows.len()
            )
        }),
    }
}

pub(crate) fn synthetic_allowed(
    focused_ax_fullscreen: Option<bool>,
    focused_native_fullscreen: Option<bool>,
    target_current: Option<SlsSpaceType>,
) -> Result<(), &'static str> {
    match (focused_ax_fullscreen, focused_native_fullscreen) {
        (Some(true), _) | (_, Some(true)) => return Err("focused window is fullscreen"),
        (Some(false), Some(false)) => {}
        _ => return Err("focused window, application, or fullscreen state is unproven"),
    }
    match target_current {
        Some(SlsSpaceType::User) => Ok(()),
        Some(SlsSpaceType::Fullscreen) => Err("target display is currently fullscreen"),
        _ => Err("target display's current user Space is unproven"),
    }
}

/// Mission Control presents every display. Unlike a targeted swipe, the AX
/// fallback may open only once every current Space is proven ordinary.
pub(crate) fn mc_fallback_allowed(
    currents: impl IntoIterator<Item = Option<SlsSpaceType>>,
) -> bool {
    let mut currents = currents.into_iter();
    currents.next() == Some(Some(SlsSpaceType::User))
        && currents.all(|kind| kind == Some(SlsSpaceType::User))
}

/// Live focus proof shared by the gesture and drag fences: the selected
/// oracle must be the effective [`PreimageRoute::SlpsFrontDirectFocused`]
/// leg with a nonzero window (never the main-window fallback, system-wide
/// read, or a windowless app), and `expected` must name exactly that
/// app/window when the caller owns the target. Returns `(pid, wid)`.
fn live_focused_window(expected: Option<(i32, u32)>) -> Result<(i32, u32), String> {
    let evidence = crate::ffi::ax_selected_focus_evidence()
        .map_err(|e| format!("focused window unproven ({e})"))?;
    if evidence.route != PreimageRoute::SlpsFrontDirectFocused {
        return Err(format!(
            "focused window route '{}' is not the direct focused leg; refusing",
            evidence.route.as_str()
        ));
    }
    if evidence.pid <= 1 {
        return Err(format!(
            "focused window pid {} is reserved; refusing",
            evidence.pid
        ));
    }
    let wid = match evidence.wid {
        Some(wid) if wid != 0 => wid,
        Some(_) => return Err("focused window id is 0; refusing".to_owned()),
        None => return Err("focused window is windowless; refusing".to_owned()),
    };
    if let Some((want_pid, want_wid)) = expected
        && (evidence.pid != want_pid || wid != want_wid)
    {
        return Err(format!(
            "focused window {}:{wid} is not the exact required {want_pid}:{want_wid}; refusing",
            evidence.pid
        ));
    }
    Ok((evidence.pid, wid))
}

/// Fresh bounded focus confirmation: `AXFocusedWindow` must still join the
/// proven window exactly, and the window's AX fullscreen state is returned
/// (`Ok(None)` is unreadable and refuses downstream).
fn fresh_focused_fullscreen(pid: i32, wid: u32) -> Result<Option<bool>, String> {
    crate::ffi::ax_focused_window_state(pid, wid)
}

/// Native fullscreen membership for exactly one window: every Space the
/// window belongs to must read a known type, an empty membership is
/// unknown, and any fullscreen membership is fullscreen. Non-user,
/// non-fullscreen memberships (system/other) are not ordinary user Spaces
/// and refuse rather than proving a plain windowed state.
fn native_fullscreen_membership(wid: u32) -> Result<bool, String> {
    let spaces = crate::ffi::sls_spaces_for_window(wid)
        .map_err(|e| format!("window {wid} Space membership unreadable ({e})"))?;
    if spaces.is_empty() {
        return Err(format!("window {wid} Space membership is empty; refusing"));
    }
    let mut fullscreen = false;
    for sid in spaces {
        match crate::ffi::sls_space_type(sid) {
            Ok(SlsSpaceType::Fullscreen) => fullscreen = true,
            Ok(SlsSpaceType::User) => {}
            Ok(other) => {
                return Err(format!(
                    "window {wid} Space {sid} type {other:?} is not an ordinary user Space; refusing"
                ));
            }
            Err(e) => {
                return Err(format!(
                    "window {wid} Space {sid} type unreadable ({e}); refusing"
                ));
            }
        }
    }
    Ok(fullscreen)
}

/// Fresh type of the target display's current Space, joined by the exact
/// pinned native UUID when the caller owns one (else the caller-owned
/// ordinal). An unreadable current or type refuses; no other display's
/// current Space participates in a targeted synthetic swipe.
fn target_current_type(
    display_index: usize,
    pinned_uuid: Option<&str>,
) -> Result<SlsSpaceType, String> {
    let rows =
        crate::ffi::sls_display_rows().map_err(|e| format!("SLS display rows unreadable ({e})"))?;
    let row = pinned_display_row(&rows, display_index, pinned_uuid)?;
    let sid = row
        .current
        .ok_or_else(|| "target display current Space unreadable".to_owned())?;
    crate::ffi::sls_space_type(sid)
        .map_err(|e| format!("target display current Space {sid} type unreadable ({e})"))
}

/// Live fence for one targeted synthetic Dock swipe (no owned window): the
/// exactly focused window must be proven nonfullscreen in AX and by its
/// native Space membership, and the target display's current Space must be
/// an ordinary user Space. `pinned_uuid` is the caller's exact native
/// display pin; `display_index` is its owned ordinal when unpinned.
pub(crate) fn require_swipe_target(
    display_index: usize,
    pinned_uuid: Option<&str>,
) -> Result<(), String> {
    require_synthetic_target(None, display_index, pinned_uuid)
}

/// Live fence for one synthetic title-bar drag of the owned `(pid, wid)`:
/// the same focused-window proof, but the exactly focused window must be
/// that dragged window (never a guess), plus the ordinary target display
/// current. Release edges stay ungated by their callers.
pub(crate) fn require_window_target(
    pid: u32,
    wid: u32,
    display_index: usize,
    pinned_uuid: Option<&str>,
) -> Result<(), String> {
    let pid =
        i32::try_from(pid).map_err(|_| format!("window pid {pid} is unrepresentable; refusing"))?;
    require_synthetic_target(Some((pid, wid)), display_index, pinned_uuid)
}

fn require_synthetic_target(
    expected: Option<(i32, u32)>,
    display_index: usize,
    pinned_uuid: Option<&str>,
) -> Result<(), String> {
    let (pid, wid) = live_focused_window(expected)?;
    let ax_fullscreen = fresh_focused_fullscreen(pid, wid)?;
    let native_fullscreen = native_fullscreen_membership(wid)?;
    let target_current = target_current_type(display_index, pinned_uuid)?;
    synthetic_allowed(ax_fullscreen, Some(native_fullscreen), Some(target_current))
        .map_err(|reason| format!("synthetic Space input refused: {reason}"))
}

/// Live fence for a Mission Control fallback selection (opening MC and
/// pressing a Space): the fresh native topology must be nonempty and EVERY
/// display's current Space must read a positive user type. Applied before
/// Mission Control opens, never only before the press.
pub(crate) fn require_mc_fallback() -> Result<(), String> {
    let rows =
        crate::ffi::sls_display_rows().map_err(|e| format!("SLS display rows unreadable ({e})"))?;
    if rows.is_empty() {
        return Err(
            "fresh SLS display topology is empty; refusing Mission Control fallback".to_owned(),
        );
    }
    let mut currents = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let sid = row.current.ok_or_else(|| {
            format!("display {index} current Space unreadable; refusing Mission Control fallback")
        })?;
        let kind = crate::ffi::sls_space_type(sid)
            .map_err(|e| format!("display {index} current Space {sid} type unreadable ({e})"))?;
        currents.push(Some(kind));
    }
    if mc_fallback_allowed(currents) {
        Ok(())
    } else {
        Err(
            "Mission Control fallback refused: every display's current Space must be proven user"
                .to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SlsSpaceType::{Fullscreen, User};

    #[test]
    fn either_fullscreen_source_refuses() {
        assert!(synthetic_allowed(Some(true), Some(false), Some(User)).is_err());
        assert!(synthetic_allowed(Some(false), Some(true), Some(User)).is_err());
    }

    #[test]
    fn ambiguous_focus_or_missing_current_refuses() {
        assert!(synthetic_allowed(None, Some(false), Some(User)).is_err());
        assert!(synthetic_allowed(Some(false), None, Some(User)).is_err());
        assert!(synthetic_allowed(Some(false), Some(false), None).is_err());
    }

    #[test]
    fn target_fullscreen_refuses_but_fullscreen_elsewhere_is_targetable() {
        assert!(synthetic_allowed(Some(false), Some(false), Some(Fullscreen)).is_err());
        let currents = [Some(User), Some(Fullscreen)];
        assert!(synthetic_allowed(Some(false), Some(false), currents[0]).is_ok());
        assert!(!mc_fallback_allowed(currents));
    }

    #[test]
    fn mc_requires_all_current_spaces_known_user() {
        assert!(mc_fallback_allowed([Some(User), Some(User)]));
        assert!(!mc_fallback_allowed([Some(User), None]));
        assert!(!mc_fallback_allowed([None, Some(User)]));
        assert!(!mc_fallback_allowed([]));
    }

    #[test]
    fn pinned_display_row_joins_exactly_one_native_identity() {
        let rows = [
            SlsDisplayRow {
                identifier: Some("A".to_owned()),
                spaces: vec![1],
                current: Some(1),
            },
            SlsDisplayRow {
                identifier: Some("B".to_owned()),
                spaces: vec![2],
                current: Some(2),
            },
        ];
        assert_eq!(
            pinned_display_row(&rows, 0, Some("B"))
                .expect("uuid joins")
                .current,
            Some(2)
        );
        assert_eq!(
            pinned_display_row(&rows, 1, None)
                .expect("index joins")
                .current,
            Some(2)
        );
        assert!(pinned_display_row(&rows, 2, None).is_err());
        assert!(pinned_display_row(&rows, 0, Some("C")).is_err());
        assert!(pinned_display_row(&[], 0, None).is_err());
        let ambiguous = [
            SlsDisplayRow {
                identifier: Some("A".to_owned()),
                spaces: vec![],
                current: None,
            },
            SlsDisplayRow {
                identifier: Some("A".to_owned()),
                spaces: vec![],
                current: None,
            },
        ];
        assert!(pinned_display_row(&ambiguous, 0, Some("A")).is_err());
    }
}
