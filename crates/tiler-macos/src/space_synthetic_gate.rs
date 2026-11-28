//! Plain-data protection shared by native Space gestures and title-bar drags.
//! Callers prove the focused window and the target display's current Space
//! independently: the pointer display and another display's current Space
//! do not identify either one.

use crate::ffi::{PreimageRoute, SlsDisplayRow, SlsSpaceType};
use crate::runtime_types::SelectPhysical;

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

/// Exact fixture-owned surface proof. The fullscreen anchor must still be
/// live and report only this Space. Auxiliary rows may share the surface,
/// but a layer-0 on-screen row owned by another process may not.
pub(crate) fn detached_surface_exact(detached: u64, pid: u32, wid: u32) -> Result<bool, String> {
    if detached == 0 || pid <= 1 || wid == 0 {
        return Ok(false);
    }
    let kind = crate::ffi::sls_space_type(detached).map_err(|e| e.to_string())?;
    if kind != SlsSpaceType::Fullscreen {
        return Ok(false);
    }
    let rows = crate::ffi::cg_window_list_basic()?;
    detached_rows_exact(detached, pid, wid, &rows, &|wid| {
        crate::ffi::sls_spaces_for_window(wid).map_err(|e| e.to_string())
    })
}

fn detached_rows_exact(
    detached: u64,
    pid: u32,
    wid: u32,
    rows: &[crate::ffi::CgWindowBasic],
    spaces_of: &dyn Fn(u32) -> Result<Vec<u64>, String>,
) -> Result<bool, String> {
    if rows.len() >= crate::config::MAX_WINDOWS {
        return Err("CG snapshot reached the window bound; completeness unknown".to_owned());
    }
    let mut anchors = rows.iter().filter(|row| row.wid == wid);
    if !matches!(anchors.next(), Some(row) if row.pid == pid && row.layer == 0)
        || anchors.next().is_some()
        || spaces_of(wid)? != [detached]
    {
        return Ok(false);
    }
    let mut foreign = false;
    for row in rows {
        if row.wid == wid || row.layer < 0 {
            continue;
        }
        let foreign_candidate = row.layer == 0 && row.onscreen && row.pid != pid;
        match spaces_of(row.wid) {
            Ok(spaces) if spaces.contains(&detached) => {
                eprintln!(
                    "tiler-macos detached surface {detached}: extra wid={} pid={} layer={} onscreen={} foreign_candidate={foreign_candidate}",
                    row.wid, row.pid, row.layer, row.onscreen
                );
                foreign |= foreign_candidate;
            }
            Err(error) if foreign_candidate => {
                return Err(format!(
                    "window {} Space membership unreadable ({error})",
                    row.wid
                ));
            }
            _ => {}
        }
    }
    Ok(!foreign)
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

/// Typed detached-route admission for the pinned Mission Control select:
/// only the exact pinned source current and the exact requested target may
/// carry a detached fullscreen surface. The pinned row is the unique row for
/// `pinned_uuid`; both SIDs must occur exactly once across the fresh rows and
/// on that pinned row, with the exact class implied by the typed physical
/// read (Desktop => user, Detached => fullscreen). Every other display's
/// current must stay a known user Space, because Mission Control presents
/// every display. A route with two Desktop reads is never admitted here.
fn mc_detached_selection_allowed(
    displays: &[crate::mc_probe::DisplaySpaces],
    pinned_uuid: &str,
    source: SelectPhysical,
    target: SelectPhysical,
    space_type: &dyn Fn(u64) -> Result<SlsSpaceType, String>,
) -> Result<(), String> {
    if !matches!(source, SelectPhysical::Detached(_))
        && !matches!(target, SelectPhysical::Detached(_))
    {
        return Err(
            "neither the exact pinned source nor the exact target is a detached fullscreen surface"
                .to_owned(),
        );
    }
    let mut pinned = None;
    for (index, row) in displays.iter().enumerate() {
        if row.identifier.as_deref() != Some(pinned_uuid) {
            continue;
        }
        if pinned.replace(index).is_some() {
            return Err(format!(
                "pinned display UUID {pinned_uuid} maps to several fresh SLS rows; refusing"
            ));
        }
    }
    let Some(pinned) = pinned else {
        return Err(format!(
            "pinned display UUID {pinned_uuid} has no fresh SLS row; refusing"
        ));
    };
    let pinned_row = &displays[pinned];
    if pinned_row.current != Some(source.sid()) {
        return Err(format!(
            "pinned display current Space {:?} is not the exact typed source Space {}; refusing",
            pinned_row.current,
            source.sid()
        ));
    }
    for (index, row) in displays.iter().enumerate() {
        if index == pinned {
            continue;
        }
        let sid = row.current.ok_or_else(|| {
            format!("display {index} current Space unreadable; refusing detached selection")
        })?;
        let kind = space_type(sid).map_err(|e| {
            format!("display {index} current Space {sid} type unreadable ({e}); refusing")
        })?;
        if kind != SlsSpaceType::User {
            return Err(format!(
                "display {index} current Space {sid} type {kind:?} is not user; refusing detached selection"
            ));
        }
    }
    for physical in [source, target] {
        let sid = physical.sid();
        let expected = match physical {
            SelectPhysical::Desktop(_) => SlsSpaceType::User,
            SelectPhysical::Detached(_) => SlsSpaceType::Fullscreen,
        };
        let mut found = None;
        for (display_index, row) in displays.iter().enumerate() {
            for (space_index, space) in row.spaces.iter().enumerate() {
                if *space != sid {
                    continue;
                }
                if found.is_some() {
                    return Err(format!(
                        "Space {sid} is not unique across fresh SLS rows; refusing detached selection"
                    ));
                }
                found = Some((display_index, space_index));
            }
        }
        match found {
            Some((display_index, _)) if display_index == pinned => {}
            Some(_) => {
                return Err(format!(
                    "Space {sid} is not on the pinned display {pinned_uuid}; refusing detached selection"
                ));
            }
            None => {
                return Err(format!(
                    "Space {sid} is not in the pinned display {pinned_uuid}'s row; refusing detached selection"
                ));
            }
        }
        let kind = space_type(sid).map_err(|e| {
            format!("Space {sid} type unreadable ({e}); refusing detached selection")
        })?;
        if kind != expected {
            return Err(format!(
                "Space {sid} type {kind:?} is not the exact expected {expected:?} for the typed physical read; refusing detached selection"
            ));
        }
    }
    Ok(())
}

/// Live fence for the pinned Mission Control select when the exact pinned
/// source current or the exact requested target is a detached fullscreen
/// surface. Replaces the ordinary all-user fallback check for that route
/// only: the pinned row must own both exact SIDs with their typed classes and
/// every other display's current must still read user. Applied before
/// Mission Control opens and again at the press fence.
pub(crate) fn require_mc_detached_selection(
    displays: &[crate::mc_probe::DisplaySpaces],
    pinned_uuid: &str,
    source: SelectPhysical,
    target: SelectPhysical,
) -> Result<(), String> {
    mc_detached_selection_allowed(displays, pinned_uuid, source, target, &|sid| {
        crate::ffi::sls_space_type(sid)
    })
    .map_err(|reason| format!("Mission Control detached selection refused: {reason}"))
}

/// Press-fence recheck for the exact registered fixture detached member.
/// The typed pinned-row fence above still passes when the surface SID stays
/// in place, so it cannot see the registered window leaving that surface
/// during the asynchronous Mission Control open. This repeats the same
/// exact owner/member proof from fresh reads immediately before the AX
/// press. `None` is an unregistered route and adds no read or refusal.
pub(crate) fn require_registered_detached_member(
    registration: Option<(u64, u32, u32)>,
    exact: impl FnOnce(u64, u32, u32) -> Result<bool, String>,
) -> Result<(), String> {
    let Some((surface, pid, wid)) = registration else {
        return Ok(());
    };
    let still_exact = exact(surface, pid, wid).map_err(|e| {
        format!(
            "registered detached member {pid}_{wid} on surface {surface} unreadable at the press fence ({e}); refusing"
        )
    })?;
    if still_exact {
        Ok(())
    } else {
        Err(format!(
            "registered detached member {pid}_{wid} left the exact surface {surface} during Mission Control open; refusing the press"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SlsSpaceType::{Fullscreen, User};

    use crate::mc_probe::DisplaySpaces;
    use crate::runtime_types::SelectPhysical::{Desktop, Detached};

    fn row(identifier: &str, spaces: &[u64], current: Option<u64>) -> DisplaySpaces {
        DisplaySpaces {
            identifier: Some(identifier.to_owned()),
            spaces: spaces.to_vec(),
            current,
        }
    }

    fn types(pairs: &[(u64, SlsSpaceType)]) -> impl Fn(u64) -> Result<SlsSpaceType, String> {
        let map: std::collections::BTreeMap<u64, SlsSpaceType> = pairs.iter().copied().collect();
        move |sid| {
            map.get(&sid)
                .copied()
                .ok_or_else(|| format!("Space {sid} type unreadable"))
        }
    }

    #[test]
    fn detached_surface_entry_and_return_accept_only_same_owner_onscreen_members() {
        use crate::ffi::{CgBounds, CgWindowBasic};

        let owner = 12_168;
        let surface = 320;
        let anchor = 10_747;
        let row = |wid, pid, layer, onscreen, h| CgWindowBasic {
            wid,
            pid,
            layer,
            onscreen,
            bounds: CgBounds {
                x: 0.0,
                y: 0.0,
                w: 2240.0,
                h,
            },
        };
        let spaces = |wid| match wid {
            20..=22 => panic!("negative-layer row {wid} must be ignored"),
            10_808 => Ok(vec![319]),
            _ => Ok(vec![surface]),
        };
        for (onscreen, extra, height) in [(false, 10_815, 1260.0), (true, 10_816, 32.0)] {
            let mut rows = [
                row(anchor, owner, 0, onscreen, 1260.0),
                row(extra, owner, 0, true, height),
                row(10_808, owner, 0, false, 632.0),
                row(20, 81, -1, onscreen, 1260.0),
                row(21, 81, -2, onscreen, 1260.0),
                row(22, 88, -4, false, 1260.0),
                row(23, 88, 24, false, 1260.0),
            ];
            assert_eq!(
                detached_rows_exact(surface, owner, anchor, &rows, &spaces),
                Ok(true)
            );
            rows[1].pid = owner + 1;
            assert_eq!(
                detached_rows_exact(surface, owner, anchor, &rows, &spaces),
                Ok(false)
            );
            rows[1].onscreen = false;
            assert_eq!(
                detached_rows_exact(surface, owner, anchor, &rows, &spaces),
                Ok(true)
            );
            assert_eq!(
                detached_rows_exact(surface, owner, anchor, &rows, &|wid| {
                    if wid == anchor {
                        Ok(vec![317, surface])
                    } else {
                        spaces(wid)
                    }
                }),
                Ok(false)
            );
        }
    }

    #[test]
    fn pinned_detached_route_admits_only_exact_typed_pinned_source_and_target() {
        let displays = [
            row("A", &[10, 11, 900], Some(900)),
            row("B", &[20], Some(20)),
        ];
        let kinds = types(&[(10, User), (11, User), (900, Fullscreen), (20, User)]);
        assert!(
            mc_detached_selection_allowed(&displays, "A", Detached(900), Desktop(11), &kinds)
                .is_ok(),
            "the exact detached pinned source and its user target are admitted"
        );
        let desktop_source = [
            row("A", &[10, 11, 900], Some(10)),
            row("B", &[20], Some(20)),
        ];
        assert!(
            mc_detached_selection_allowed(&desktop_source, "A", Desktop(10), Detached(900), &kinds)
                .is_ok(),
            "the exact Desktop source and detached target are admitted"
        );
    }

    #[test]
    fn registered_member_drift_during_mc_open_blocks_press_with_unchanged_typed_row() {
        let displays = [
            row("A", &[10, 11, 900], Some(900)),
            row("B", &[20], Some(20)),
        ];
        let kinds = types(&[(10, User), (11, User), (900, Fullscreen), (20, User)]);
        // The press-fence typed proof still passes: the exact SIDs, their
        // classes, and the pinned index/current are unchanged by the open.
        mc_detached_selection_allowed(&displays, "A", Detached(900), Desktop(11), &kinds)
            .expect("the typed SID and pinned index are unchanged during open");
        // The registered member left the still-present fullscreen surface
        // while Mission Control was opening: only the exact member recheck
        // can block the press.
        let error =
            require_registered_detached_member(Some((900, 12_168, 10_747)), |surface, pid, wid| {
                assert_eq!((surface, pid, wid), (900, 12_168, 10_747));
                Ok(false)
            })
            .expect_err("member drift with an unchanged typed row blocks the press");
        assert!(error.contains("during Mission Control open"));
        assert!(error.contains("refusing the press"));
        require_registered_detached_member(Some((900, 12_168, 10_747)), |_, _, _| Ok(true))
            .expect("a still-exact member permits the press");
        require_registered_detached_member(None, |_, _, _| {
            panic!("an unregistered route must not add a member read")
        })
        .expect("unregistered routes are unchanged");
        assert!(
            require_registered_detached_member(Some((900, 12_168, 10_747)), |_, _, _| {
                Err("CG unreadable".to_owned())
            })
            .expect_err("an unreadable member proof refuses")
            .contains("unreadable at the press fence")
        );
    }

    #[test]
    fn pinned_detached_route_refuses_nonpinned_currents_that_are_not_user() {
        let displays = [
            row("A", &[10, 11, 900], Some(900)),
            row("B", &[20, 21], Some(21)),
        ];
        let kinds = types(&[
            (10, User),
            (11, User),
            (900, Fullscreen),
            (20, User),
            (21, Fullscreen),
        ]);
        assert!(
            mc_detached_selection_allowed(&displays, "A", Detached(900), Desktop(11), &kinds)
                .is_err(),
            "another display's fullscreen current refuses"
        );
        let unreadable = [row("A", &[10, 11, 900], Some(900)), row("B", &[20], None)];
        assert!(
            mc_detached_selection_allowed(&unreadable, "A", Detached(900), Desktop(11), &kinds)
                .is_err(),
            "another display's unreadable current refuses"
        );
    }

    #[test]
    fn pinned_detached_route_refuses_wrong_or_untyped_classes() {
        let displays = [
            row("A", &[10, 11, 900], Some(900)),
            row("B", &[20], Some(20)),
        ];
        for kinds in [
            types(&[(10, User), (11, User), (900, User), (20, User)]),
            types(&[(10, User), (11, Fullscreen), (900, Fullscreen), (20, User)]),
            types(&[(10, User), (11, User), (20, User)]),
        ] {
            assert!(
                mc_detached_selection_allowed(&displays, "A", Detached(900), Desktop(11), &kinds)
                    .is_err(),
                "a wrong or unreadable exact class refuses"
            );
        }
        assert!(
            mc_detached_selection_allowed(
                &displays,
                "A",
                Desktop(10),
                Desktop(11),
                &types(&[(10, User), (11, User), (900, Fullscreen), (20, User)])
            )
            .is_err(),
            "two Desktop reads never take the detached exemption"
        );
    }

    #[test]
    fn pinned_detached_route_refuses_moved_duplicated_or_offrow_sids() {
        let kinds = types(&[
            (10, User),
            (11, User),
            (900, Fullscreen),
            (20, User),
            (21, User),
        ]);
        let moved = [
            row("A", &[10, 11, 900], Some(901)),
            row("B", &[20], Some(20)),
        ];
        assert!(
            mc_detached_selection_allowed(&moved, "A", Detached(900), Desktop(11), &kinds).is_err(),
            "the pinned current must be exactly the typed source"
        );
        let duplicate = [
            row("A", &[10, 11, 11, 900], Some(900)),
            row("B", &[20], Some(20)),
        ];
        assert!(
            mc_detached_selection_allowed(&duplicate, "A", Detached(900), Desktop(11), &kinds)
                .is_err(),
            "a target listed twice on the pinned row refuses"
        );
        let source_elsewhere = [
            row("A", &[10, 11, 900], Some(900)),
            row("B", &[20, 900], Some(20)),
        ];
        assert!(
            mc_detached_selection_allowed(
                &source_elsewhere,
                "A",
                Detached(900),
                Desktop(11),
                &kinds
            )
            .is_err(),
            "a source listed on another row refuses"
        );
        let offrow_target = [
            row("A", &[10, 900], Some(900)),
            row("B", &[20, 11], Some(20)),
        ];
        assert!(
            mc_detached_selection_allowed(&offrow_target, "A", Detached(900), Desktop(11), &kinds)
                .is_err(),
            "a target on another row refuses"
        );
        let ambiguous_pin = [
            row("A", &[10, 11, 900], Some(900)),
            row("A", &[20], Some(20)),
        ];
        assert!(
            mc_detached_selection_allowed(&ambiguous_pin, "A", Detached(900), Desktop(11), &kinds)
                .is_err(),
            "an ambiguous pinned UUID refuses"
        );
        let missing_pin = [row("B", &[20], Some(20))];
        assert!(
            mc_detached_selection_allowed(&missing_pin, "A", Detached(900), Desktop(11), &kinds)
                .is_err(),
            "a missing pinned UUID refuses"
        );
    }

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
