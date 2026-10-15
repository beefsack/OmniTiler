#![forbid(unsafe_code)]

//! Pure scoped-tile topology: display geometry, coordinate conversion, and
//! current-user-Space domain selection.
//!
//! No live state, no threads, no titles: every input is caller-supplied plain
//! data (an [`crate::ffi::NsDisplay`] snapshot from the main thread plus the
//! read-only SLS display rows) and every output is plain data. Live callers
//! snapshot on the main thread and re-select when the
//! [`crate::ffi::DisplayChangeFlag`] reports a change or [`TopologyPoll`]
//! reports a revision drift; the observer itself lives in [`crate::ffi`].
//!
//! Coordinates: AppKit frames arrive in global bottom-left points and CG/AX
//! consume top-left points. Conversion uses the PRIMARY display height (the
//! display whose AppKit frame origin is `(0, 0)`, height in points — the same
//! value in both systems, so mixed scales need no pixel math), mirroring
//! glide's `CoordinateConverter` (CG main height minus rect max-y):
//! https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/sys/screen.rs
//! Multi-display above/below/negative origins fall out naturally (a display
//! above the primary converts to a negative CG y; one below exceeds the
//! primary height). A missing primary refuses conversion (`None`), never a
//! guessed origin.
//!
//! Homing (display↔Space rows) is UUID-exact: display UUIDs come from the
//! public `CGDisplayCreateUUIDFromDisplayID` and SLS rows carry their own
//! `"Display Identifier"` (yabai `display_space_list` matches with
//! `CFEqual(uuid, identifier)`:
//! https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/display.c).
//! Missing or duplicate row maps refuse the display; positions are never
//! used as identity. Unified sessions (`separate_spaces == false`, the
//! Mission Control setting off — public `NSScreen.screensHaveSeparateSpaces`,
//! the AppKit reading rift probes as `SLSGetSpaceManagementMode(cid) == 1`)
//! share one Space set across displays: a single SLS row shares its current
//! Space across every display (documented spanning reading); multiple rows
//! in unified mode refuse (homing unprovable — no surveyed source shows
//! that shape, and guessing would home windows to the wrong display).
//!
//! Selection: exactly one Space is eligible per display — the known CURRENT
//! Space, and only when its read-only type is user (type 0 per the pinned
//! yabai `SLSSpaceGetType` ids). Fullscreen/system/other/unreadable current
//! Spaces refuse the display; windows whose membership is not exactly
//! `[current]` (sticky multi-Space, other-Space, empty/unknown) are excluded
//! with per-window reasons and counts — omission, never a close report.
//! When the current Space is shared by several displays (unified span, or
//! duplicate currents mid-transition), membership alone cannot home a
//! window: greatest-overlap arbitration over the native frame (top-left
//! points, same space as the converted bounds) picks the unique max; ties,
//! zero overlap, and missing frames exclude with named reasons.

use serde::{Deserialize, Serialize};

use crate::ffi::{NsDisplay, SlsDisplayRow, SlsSpaceType};

/// CG/AX top-left points rect (converted; scale carried separately).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointsRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl PointsRect {
    /// Integer adapter rect for core planning: rounds points 1:1 to whole
    /// units (never floors a minimum inward or ceils a bound outward —
    /// callers that need directed rounding do it themselves). `None` on any
    /// non-finite component, non-positive size, or out-of-`i32` component:
    /// refused, never saturated into a guessed rect (saturation would plan
    /// outside the true work area).
    #[must_use]
    pub fn to_rect(&self) -> Option<tiler_core::geometry::Rect> {
        for v in [self.x, self.y, self.w, self.h] {
            if !v.is_finite() {
                return None;
            }
        }
        if self.w <= 0.0 || self.h <= 0.0 {
            return None;
        }
        let (x, y, w, h) = (
            self.x.round(),
            self.y.round(),
            self.w.round(),
            self.h.round(),
        );
        if w < 1.0 || h < 1.0 {
            return None;
        }
        const LIM: f64 = i32::MAX as f64;
        for v in [x, y, w, h] {
            if v < -LIM || v > LIM {
                return None;
            }
        }
        Some(tiler_core::geometry::Rect {
            x: x as i32,
            y: y as i32,
            w: w as i32,
            h: h as i32,
        })
    }

    /// Intersection area with display bounds in the same top-left points
    /// space. Non-finite or degenerate inputs overlap nothing (`0.0`).
    #[must_use]
    pub fn overlap_area(&self, bounds: &PointsRect) -> f64 {
        let (ax0, ay0, ax1, ay1) = (self.x, self.y, self.x + self.w, self.y + self.h);
        let (bx0, by0, bx1, by1) = (bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h);
        for v in [ax0, ay0, ax1, ay1, bx0, by0, bx1, by1] {
            if !v.is_finite() {
                return 0.0;
            }
        }
        let w = (ax1.min(bx1) - ax0.max(bx0)).max(0.0);
        let h = (ay1.min(by1) - ay0.max(by0)).max(0.0);
        if !w.is_finite() || !h.is_finite() {
            0.0
        } else {
            w * h
        }
    }
}

/// One display's geometry: CG ID, exact managed UUID, converted bounds plus
/// the menu/Dock-excluded visible frame, and backing scale. Converted rects
/// are `None` while the primary height is unknown (refused, never guessed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayGeometry {
    pub cg_id: u32,
    pub uuid: Option<String>,
    pub cg_bounds: Option<PointsRect>,
    pub visible: Option<PointsRect>,
    pub scale: f64,
}

/// Read-only topology snapshot: joined display geometry plus the raw SLS
/// rows plus the separate-Spaces mode plus a fingerprint revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologySnapshot {
    pub displays: Vec<DisplayGeometry>,
    pub rows: Vec<SlsDisplayRow>,
    pub separate_spaces: Option<bool>,
    pub revision: u64,
}

/// Primary display height in AppKit points: the display whose frame origin
/// is `(0, 0)`. `None` when missing or degenerate (conversion refuses).
#[must_use]
pub fn primary_height(ns: &[NsDisplay]) -> Option<f64> {
    ns.iter()
        .find(|d| d.frame.x == 0.0 && d.frame.y == 0.0)
        .map(|d| d.frame.h)
        .filter(|h| h.is_finite() && *h > 0.0)
}

/// AppKit bottom-left y to CG/AX top-left y for a rect of height `h`.
/// `None` on any non-finite input (refused, never guessed).
#[must_use]
pub fn appkit_to_cg_y(y_app: f64, h: f64, primary_h: f64) -> Option<f64> {
    if y_app.is_finite() && h.is_finite() && primary_h.is_finite() {
        Some(primary_h - (y_app + h))
    } else {
        None
    }
}

/// Convert one AppKit bottom-left rect to CG/AX top-left points. Width,
/// height, and x pass through untouched (points, never pixels).
#[must_use]
pub fn convert_appkit_rect(x: f64, y: f64, w: f64, h: f64, primary_h: f64) -> Option<PointsRect> {
    appkit_to_cg_y(y, h, primary_h).map(|cy| PointsRect { x, y: cy, w, h })
}

/// Join display geometry from the main-thread snapshot. UUIDs ride straight
/// from the snapshot (exact public lookup); converted rects need the primary
/// height, without which they are `None`. No SLS row is consulted here —
/// homing is exact-UUID at selection time, never positional.
#[must_use]
pub fn join_displays(ns: &[NsDisplay]) -> Vec<DisplayGeometry> {
    let primary = primary_height(ns);
    ns.iter()
        .map(|d| {
            let convert = |r: crate::ffi::DisplayRect| {
                primary.and_then(|h| convert_appkit_rect(r.x, r.y, r.w, r.h, h))
            };
            DisplayGeometry {
                cg_id: d.cg_id,
                uuid: d.uuid.clone(),
                cg_bounds: convert(d.frame),
                visible: convert(d.visible),
                scale: d.scale,
            }
        })
        .collect()
}

/// Deterministic FNV-1a fingerprint over sorted display rows plus raw SLS
/// rows plus the separate-Spaces mode. Geometry, Space, UUID, or mode drift
/// changes the revision; identical snapshots repeat it.
#[must_use]
pub fn fingerprint_snapshot(
    displays: &[DisplayGeometry],
    rows: &[SlsDisplayRow],
    separate_spaces: Option<bool>,
) -> u64 {
    let mut sorted: Vec<&DisplayGeometry> = displays.iter().collect();
    sorted.sort_by_key(|d| d.cg_id);
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    for d in sorted {
        mix(&d.cg_id.to_le_bytes());
        match &d.uuid {
            Some(uuid) => {
                mix(&[1]);
                mix(uuid.as_bytes());
            }
            None => mix(&[0]),
        }
        for rect in [d.cg_bounds, d.visible] {
            match rect {
                Some(r) => {
                    mix(&[1]);
                    for v in [r.x, r.y, r.w, r.h] {
                        mix(&v.to_bits().to_le_bytes());
                    }
                }
                None => mix(&[0]),
            }
        }
        mix(&d.scale.to_bits().to_le_bytes());
    }
    for r in rows {
        match &r.identifier {
            Some(id) => {
                mix(&[1]);
                mix(id.as_bytes());
            }
            None => mix(&[0]),
        }
        for sid in &r.spaces {
            mix(&sid.to_le_bytes());
        }
        mix(&[0xff]);
        match r.current {
            Some(c) => {
                mix(&[1]);
                mix(&c.to_le_bytes());
            }
            None => mix(&[0]),
        }
    }
    match separate_spaces {
        Some(true) => mix(&[1]),
        Some(false) => mix(&[2]),
        None => mix(&[0]),
    }
    hash
}

/// Build a snapshot from a main-thread display list plus the raw SLS rows
/// plus the separate-Spaces mode (`None` when unreadable: legacy exact-1:1
/// positional homing, else refusal).
#[must_use]
pub fn build_snapshot(
    ns: &[NsDisplay],
    rows: &[SlsDisplayRow],
    separate_spaces: Option<bool>,
) -> TopologySnapshot {
    let displays = join_displays(ns);
    let revision = fingerprint_snapshot(&displays, rows, separate_spaces);
    TopologySnapshot {
        displays,
        rows: rows.to_vec(),
        separate_spaces,
        revision,
    }
}

/// One SLS row homed to a display: its Space list plus current Space.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HomedRow {
    spaces: Vec<u64>,
    current: Option<u64>,
}

/// Home one display's UUID to its SLS row:
/// - separate mode: exact identifier match (missing UUID or row →
///   `MissingMap`; duplicates → `DuplicateMap`; positions never consulted).
/// - unified mode: the single row spans every display (`Shared`); a row
///   whose known identifier contradicts the display's known UUID is not that
///   display's row (`MissingMap`); multiple rows refuse (`AmbiguousDisplaySpace`).
/// - unknown mode: legacy exact single-display single-row positional homing,
///   else `AmbiguousDisplaySpace`.
fn home_display(
    uuid: Option<&str>,
    rows: &[SlsDisplayRow],
    separate_spaces: Option<bool>,
    single_positional: bool,
) -> Result<HomedRow, SelectError> {
    match separate_spaces {
        Some(true) => {
            let uuid = uuid.ok_or(SelectError::MissingMap)?;
            let mut matched: Option<&SlsDisplayRow> = None;
            let mut duplicates = 0u32;
            for row in rows {
                if row.identifier.as_deref() == Some(uuid) {
                    duplicates += 1;
                    matched = Some(row);
                }
            }
            if duplicates > 1 {
                return Err(SelectError::DuplicateMap);
            }
            matched
                .map(|row| HomedRow {
                    spaces: row.spaces.clone(),
                    current: row.current,
                })
                .ok_or(SelectError::MissingMap)
        }
        Some(false) => {
            if rows.len() != 1 {
                return Err(SelectError::AmbiguousDisplaySpace(
                    "unified Spaces with several SLS rows; homing unprovable",
                ));
            }
            let row = &rows[0];
            // A known row identifier contradicting the display's known UUID
            // is evidence AGAINST sharing that row here.
            if let (Some(want), Some(have)) = (uuid, row.identifier.as_deref())
                && want != have
            {
                return Err(SelectError::MissingMap);
            }
            Ok(HomedRow {
                spaces: row.spaces.clone(),
                current: row.current,
            })
        }
        None => {
            if single_positional && rows.len() == 1 {
                Ok(HomedRow {
                    spaces: rows[0].spaces.clone(),
                    current: rows[0].current,
                })
            } else {
                Err(SelectError::AmbiguousDisplaySpace(
                    "unknown separate-Spaces mode without 1:1 rows; homing ambiguous",
                ))
            }
        }
    }
}

/// One window's Space-membership evidence for selection: the live membership
/// list for its window ID (via the existing `SLSCopySpacesForWindows` read
/// path) plus its carried fullscreen flag plus its native frame in CG/AX
/// top-left points (AX frame or CG bounds — both top-left points — for the
/// shared-Space greatest-overlap arbitration).
#[derive(Debug, Clone, PartialEq)]
pub struct WindowMembership {
    pub wid: u32,
    pub pid: u32,
    pub fullscreen: bool,
    /// Live Space IDs owning this window. Empty means unreadable (unknown),
    /// never "no Space".
    pub spaces: Vec<u64>,
    pub frame: Option<PointsRect>,
}

/// A window selected into the scoped domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainedWindow {
    pub wid: u32,
    pub pid: u32,
    pub fullscreen: bool,
}

/// Why a window missed the domain (excluded, never destroyed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeReason {
    /// Membership is not exactly `[current]`: sticky multi-Space.
    Sticky,
    /// Single membership on another Space (alive elsewhere).
    OtherSpace,
    /// Empty/unreadable membership (unknown home Space).
    UnknownSpace,
    /// No native frame while shared-Space arbitration needs one.
    UnknownGeometry,
    /// Shared Space but no overlap with this display.
    NoOverlap,
    /// Shared Space but greatest overlap is on another display.
    OtherDisplay,
    /// Shared Space with a tied greatest overlap (homing unprovable).
    SharedTie,
    /// Shared Space but a co-homed display lacks geometry (uniqueness
    /// unprovable).
    AmbiguousDisplay,
}

/// Per-window exclusion evidence (IDs plus reasons only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowExclusion {
    pub wid: u32,
    pub reason: ScopeReason,
}

/// The scoped tile domain for one display: revision plus domain keys plus
/// exactly-homed windows plus exclusion evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileDomain {
    pub revision: u64,
    pub output: String,
    pub workspace: String,
    pub display: u32,
    pub space: u64,
    pub windows: Vec<DomainedWindow>,
    pub excluded: Vec<WindowExclusion>,
}

/// Why domain selection refused (nothing publishes on any of these).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectError {
    /// No display row for the requested CG ID.
    MissingDisplay(u32),
    /// Display geometry never converted (missing primary).
    MissingGeometry(u32),
    /// No SLS row carries this display's UUID (missing map, or a unified
    /// row contradicting it).
    MissingMap,
    /// Several SLS rows claim this display's UUID (duplicate map).
    DuplicateMap,
    /// Display↔Space rows cannot be homed: legacy unknown mode without 1:1
    /// rows, or unified mode with several rows (reason carried).
    AmbiguousDisplaySpace(&'static str),
    /// The homed row reports no current Space.
    MissingCurrentSpace,
    /// The current Space type is unreadable (unknown home).
    UnknownSpaceType(u64),
    /// The current Space is not a user Space (kind named, never executed).
    NonUserSpace { sid: u64, kind: String },
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDisplay(id) => write!(f, "no display row for CG ID {id}"),
            Self::MissingGeometry(id) => write!(f, "display {id} has no converted geometry"),
            Self::MissingMap => write!(f, "no SLS row for this display UUID; refusing"),
            Self::DuplicateMap => write!(f, "duplicate SLS rows for this display UUID; refusing"),
            Self::AmbiguousDisplaySpace(reason) => {
                write!(f, "display/Space homing ambiguous ({reason}); refusing")
            }
            Self::MissingCurrentSpace => write!(f, "homed row reports no current Space"),
            Self::UnknownSpaceType(sid) => {
                write!(
                    f,
                    "Space {sid} type unreadable; refusing without a known user Space"
                )
            }
            Self::NonUserSpace { sid, kind } => write!(
                f,
                "Space {sid} is {kind} (not a user Space); refusing without a proven restore"
            ),
        }
    }
}

impl std::error::Error for SelectError {}

/// Output domain key for a display (stable within the session).
#[must_use]
pub fn output_key(cg_id: u32) -> String {
    format!("macos-display-{cg_id}")
}

/// Workspace domain key for a Space ID.
#[must_use]
pub fn workspace_key(sid: u64) -> String {
    format!("space-{sid}")
}

/// Select the scoped domain for one display. `space_type` resolves a Space
/// ID to its read-only type (`None` = unreadable); the live side injects
/// the existing `sls_space_type` read, tests inject scripted types.
/// Fullscreen windows on the current user Space are INCLUDED with their flag
/// (the hold gate decides); fullscreen/system/other Space homing refuses the
/// display instead. When the current Space is homed by several displays,
/// membership-exact candidates arbitrate by greatest frame overlap with a
/// unique tie refusal (ties, zero overlap, and missing frames exclude).
pub fn select_domain(
    snapshot: &TopologySnapshot,
    cg_id: u32,
    memberships: &[WindowMembership],
    space_type: &dyn Fn(u64) -> Option<SlsSpaceType>,
) -> Result<TileDomain, SelectError> {
    let display = snapshot
        .displays
        .iter()
        .find(|d| d.cg_id == cg_id)
        .ok_or(SelectError::MissingDisplay(cg_id))?;
    let target_bounds = display
        .cg_bounds
        .ok_or(SelectError::MissingGeometry(cg_id))?;
    if display.visible.is_none() {
        return Err(SelectError::MissingGeometry(cg_id));
    }
    let single_positional = snapshot.displays.len() == 1 && snapshot.rows.len() == 1;
    let homed = home_display(
        display.uuid.as_deref(),
        &snapshot.rows,
        snapshot.separate_spaces,
        single_positional,
    )?;
    let current = homed.current.ok_or(SelectError::MissingCurrentSpace)?;
    match space_type(current) {
        Some(SlsSpaceType::User) => {}
        Some(kind) => {
            return Err(SelectError::NonUserSpace {
                sid: current,
                kind: format!("{kind:?}"),
            });
        }
        None => return Err(SelectError::UnknownSpaceType(current)),
    }
    // Co-homed displays sharing this exact current Space: membership alone
    // cannot home a window then, so candidates arbitrate by geometry.
    // Homing failures elsewhere simply do not share (they refused already).
    // Positions (not rects) identify the target: mirrored displays share
    // identical bounds, and rect equality would hide the twin.
    let mut sharer_rects: Vec<PointsRect> = Vec::new();
    let mut target_pos: Option<usize> = None;
    let mut sharers_complete = true;
    for other in &snapshot.displays {
        let Ok(other_homed) = home_display(
            other.uuid.as_deref(),
            &snapshot.rows,
            snapshot.separate_spaces,
            single_positional,
        ) else {
            continue;
        };
        if other_homed.current != Some(current) {
            continue;
        }
        match other.cg_bounds {
            Some(rect) => {
                if other.cg_id == cg_id && target_pos.is_none() {
                    target_pos = Some(sharer_rects.len());
                }
                sharer_rects.push(rect);
            }
            None => sharers_complete = false,
        }
    }
    let shared = sharer_rects.len() > 1;
    // The target homed successfully above over identical inputs, so it is
    // among the sharers; an unstable homing refuses instead of panicking.
    let Some(target_pos) = target_pos else {
        return Err(SelectError::AmbiguousDisplaySpace(
            "target display homing unstable",
        ));
    };
    let mut windows = Vec::new();
    let mut excluded = Vec::new();
    for m in memberships {
        if !(m.spaces.len() == 1 && m.spaces[0] == current) {
            let reason = if m.spaces.is_empty() {
                ScopeReason::UnknownSpace
            } else if m.spaces.len() > 1 {
                // Sticky even when it contains current: restoring one Space
                // out of several is not a proven inverse (same rule as the
                // Tier-2 move preimage).
                ScopeReason::Sticky
            } else {
                ScopeReason::OtherSpace
            };
            excluded.push(WindowExclusion { wid: m.wid, reason });
            continue;
        }
        if !shared {
            windows.push(DomainedWindow {
                wid: m.wid,
                pid: m.pid,
                fullscreen: m.fullscreen,
            });
            continue;
        }
        // Shared current Space: greatest-overlap arbitration. Uniqueness must
        // be provable over complete co-homed geometry.
        if !sharers_complete {
            excluded.push(WindowExclusion {
                wid: m.wid,
                reason: ScopeReason::AmbiguousDisplay,
            });
            continue;
        }
        let Some(frame) = m.frame else {
            excluded.push(WindowExclusion {
                wid: m.wid,
                reason: ScopeReason::UnknownGeometry,
            });
            continue;
        };
        let overlaps: Vec<f64> = sharer_rects
            .iter()
            .map(|rect| frame.overlap_area(rect))
            .collect();
        let target = frame.overlap_area(&target_bounds);
        let max = overlaps.iter().fold(0.0_f64, |a, b| f64::max(a, *b));
        if max <= 0.0 {
            excluded.push(WindowExclusion {
                wid: m.wid,
                reason: ScopeReason::NoOverlap,
            });
        } else if target < max {
            excluded.push(WindowExclusion {
                wid: m.wid,
                reason: ScopeReason::OtherDisplay,
            });
        } else if overlaps
            .iter()
            .enumerate()
            .any(|(pos, area)| pos != target_pos && *area == max)
        {
            excluded.push(WindowExclusion {
                wid: m.wid,
                reason: ScopeReason::SharedTie,
            });
        } else {
            windows.push(DomainedWindow {
                wid: m.wid,
                pid: m.pid,
                fullscreen: m.fullscreen,
            });
        }
    }
    Ok(TileDomain {
        revision: snapshot.revision,
        output: output_key(cg_id),
        workspace: workspace_key(current),
        display: cg_id,
        space: current,
        windows,
        excluded,
    })
}

/// Revision poll for the controller: `None` until the first snapshot, then
/// the new revision exactly when it drifts. Pure: the controller owns the
/// snapshot cadence alongside the [`crate::ffi::DisplayChangeFlag`]
/// observer; no native state crosses owner threads here — revisions only.
#[derive(Debug, Clone, Default)]
pub struct TopologyPoll {
    last: Option<u64>,
}

/// Poll outcome: the revision to record when changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollOutcome {
    Unchanged,
    Changed(u64),
}

impl TopologyPoll {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll(&mut self, snapshot: &TopologySnapshot) -> PollOutcome {
        if self.last == Some(snapshot.revision) {
            PollOutcome::Unchanged
        } else {
            self.last = Some(snapshot.revision);
            PollOutcome::Changed(snapshot.revision)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::DisplayRect;

    fn ns(cg_id: u32, uuid: &str, x: f64, y: f64, w: f64, h: f64) -> NsDisplay {
        NsDisplay {
            cg_id,
            uuid: Some(uuid.to_owned()),
            frame: DisplayRect { x, y, w, h },
            visible: DisplayRect { x, y, w, h },
            scale: 2.0,
        }
    }

    fn row(identifier: Option<&str>, spaces: &[u64], current: Option<u64>) -> SlsDisplayRow {
        SlsDisplayRow {
            identifier: identifier.map(str::to_owned),
            spaces: spaces.to_vec(),
            current,
        }
    }

    fn member(wid: u32, spaces: &[u64]) -> WindowMembership {
        WindowMembership {
            wid,
            pid: 4242,
            fullscreen: false,
            spaces: spaces.to_vec(),
            frame: None,
        }
    }

    fn user_space(_: u64) -> Option<SlsSpaceType> {
        Some(SlsSpaceType::User)
    }

    #[test]
    fn single_display_converts_with_primary_origin() {
        // Mirror of the glide converter test shape: AppKit bottom-left to
        // CG/AX top-left via the primary height.
        let displays = vec![ns(1, "uuid-1", 0.0, 0.0, 1512.0, 982.0)];
        let joined = join_displays(&displays);
        assert_eq!(joined.len(), 1);
        let bounds = joined[0].cg_bounds.expect("primary converts");
        assert_eq!(
            (bounds.x, bounds.y, bounds.w, bounds.h),
            (0.0, 0.0, 1512.0, 982.0)
        );
        assert_eq!(joined[0].uuid.as_deref(), Some("uuid-1"));
        // Exact UUID homing selects with domain keys.
        let snapshot = build_snapshot(
            &displays,
            &[row(Some("uuid-1"), &[11], Some(11))],
            Some(true),
        );
        let domain = select_domain(&snapshot, 1, &[member(7, &[11])], &user_space)
            .expect("single user Space selects");
        assert_eq!(domain.output, "macos-display-1");
        assert_eq!(domain.workspace, "space-11");
        assert_eq!(domain.windows.len(), 1);
        assert!(domain.excluded.is_empty());
    }

    #[test]
    fn multi_display_selects_each_domain_by_exact_uuid() {
        // Rows deliberately ordered differently from displays: homing is by
        // UUID equality, never positionally.
        let displays = vec![
            ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
            ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
        ];
        let rows = vec![
            row(Some("uuid-2"), &[21], Some(21)),
            row(Some("uuid-1"), &[11], Some(11)),
        ];
        let snapshot = build_snapshot(&displays, &rows, Some(true));
        let congestion = [
            WindowMembership {
                wid: 1,
                pid: 100,
                fullscreen: false,
                spaces: vec![11],
                frame: None,
            },
            WindowMembership {
                wid: 2,
                pid: 101,
                fullscreen: true,
                spaces: vec![21],
                frame: None,
            },
        ];
        let first = select_domain(&snapshot, 1, &congestion, &user_space).expect("display 1");
        assert_eq!((first.space, first.workspace.as_str()), (11, "space-11"));
        assert_eq!(
            first.windows.iter().map(|w| w.wid).collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            first
                .excluded
                .iter()
                .map(|e| (e.wid, e.reason))
                .collect::<Vec<_>>(),
            vec![(2, ScopeReason::OtherSpace)]
        );
        let second = select_domain(&snapshot, 2, &congestion, &user_space).expect("display 2");
        assert_eq!((second.space, second.workspace.as_str()), (21, "space-21"));
        assert_eq!(
            second
                .windows
                .iter()
                .map(|w| (w.wid, w.fullscreen))
                .collect::<Vec<_>>(),
            vec![(2, true)]
        );
    }

    #[test]
    fn missing_and_duplicate_uuid_maps_refuse() {
        let displays = vec![
            ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
            ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
        ];
        // Display 2 has no row: missing map refuses exactly that display.
        let snapshot = build_snapshot(
            &displays,
            &[row(Some("uuid-1"), &[11], Some(11))],
            Some(true),
        );
        assert!(select_domain(&snapshot, 1, &[], &user_space).is_ok());
        assert_eq!(
            select_domain(&snapshot, 2, &[], &user_space).unwrap_err(),
            SelectError::MissingMap
        );
        // Two rows claim one UUID: duplicate map refuses.
        let snapshot = build_snapshot(
            &displays,
            &[
                row(Some("uuid-1"), &[11], Some(11)),
                row(Some("uuid-1"), &[12], Some(12)),
            ],
            Some(true),
        );
        assert_eq!(
            select_domain(&snapshot, 1, &[], &user_space).unwrap_err(),
            SelectError::DuplicateMap
        );
        // A display without a UUID cannot home either.
        let mut displays = displays;
        displays[0].uuid = None;
        let snapshot = build_snapshot(
            &displays,
            &[row(Some("uuid-1"), &[11], Some(11))],
            Some(true),
        );
        assert_eq!(
            select_domain(&snapshot, 1, &[], &user_space).unwrap_err(),
            SelectError::MissingMap
        );
    }

    #[test]
    fn unified_single_row_shares_with_geometry_arbitration() {
        // Unified span: one row with no identifier (nothing contradicts
        // sharing), two displays, same current Space. Windows home by
        // greatest frame overlap; ties and misses exclude.
        let displays = vec![
            ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
            ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
        ];
        let snapshot = build_snapshot(&displays, &[row(None, &[11], Some(11))], Some(false));
        let memberships = vec![
            // Mostly on display 1 (CG top-left points).
            WindowMembership {
                wid: 1,
                pid: 100,
                fullscreen: false,
                spaces: vec![11],
                frame: Some(PointsRect {
                    x: 100.0,
                    y: 100.0,
                    w: 800.0,
                    h: 600.0,
                }),
            },
            // Mostly on display 2.
            WindowMembership {
                wid: 2,
                pid: 101,
                fullscreen: true,
                spaces: vec![11],
                frame: Some(PointsRect {
                    x: 1500.0,
                    y: 100.0,
                    w: 800.0,
                    h: 600.0,
                }),
            },
            // Perfectly straddling tie: excluded on both.
            WindowMembership {
                wid: 3,
                pid: 102,
                fullscreen: false,
                spaces: vec![11],
                frame: Some(PointsRect {
                    x: 1040.0,
                    y: 100.0,
                    w: 800.0,
                    h: 600.0,
                }),
            },
            // No frame: unknown geometry on a shared Space.
            WindowMembership {
                wid: 4,
                pid: 103,
                fullscreen: false,
                spaces: vec![11],
                frame: None,
            },
        ];
        let first = select_domain(&snapshot, 1, &memberships, &user_space).expect("display 1");
        assert_eq!(first.space, 11);
        assert_eq!(
            first
                .windows
                .iter()
                .map(|w| (w.wid, w.fullscreen))
                .collect::<Vec<_>>(),
            vec![(1, false)]
        );
        assert_eq!(
            first
                .excluded
                .iter()
                .map(|e| (e.wid, e.reason))
                .collect::<Vec<_>>(),
            vec![
                (2, ScopeReason::OtherDisplay),
                (3, ScopeReason::SharedTie),
                (4, ScopeReason::UnknownGeometry),
            ]
        );
        let second = select_domain(&snapshot, 2, &memberships, &user_space).expect("display 2");
        assert_eq!(
            second
                .windows
                .iter()
                .map(|w| (w.wid, w.fullscreen))
                .collect::<Vec<_>>(),
            vec![(2, true)]
        );
        assert_eq!(
            second
                .excluded
                .iter()
                .map(|e| (e.wid, e.reason))
                .collect::<Vec<_>>(),
            vec![
                (1, ScopeReason::OtherDisplay),
                (3, ScopeReason::SharedTie),
                (4, ScopeReason::UnknownGeometry),
            ]
        );
    }

    #[test]
    fn unified_multi_row_and_contradiction_refuse() {
        let displays = vec![
            ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
            ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
        ];
        // Several rows in unified mode: homing unprovable, documented refusal.
        let snapshot = build_snapshot(
            &displays,
            &[
                row(Some("a"), &[11], Some(11)),
                row(Some("b"), &[21], Some(21)),
            ],
            Some(false),
        );
        assert!(matches!(
            select_domain(&snapshot, 1, &[], &user_space).unwrap_err(),
            SelectError::AmbiguousDisplaySpace(_)
        ));
        // A single unified row contradicting the display's known UUID is not
        // that display's row.
        let snapshot = build_snapshot(
            &displays,
            &[row(Some("uuid-9"), &[11], Some(11))],
            Some(false),
        );
        assert_eq!(
            select_domain(&snapshot, 1, &[], &user_space).unwrap_err(),
            SelectError::MissingMap
        );
        // But a row naming exactly one display still shares with it (the
        // other display refuses on the contradiction).
        let snapshot = build_snapshot(
            &displays,
            &[row(Some("uuid-1"), &[11], Some(11))],
            Some(false),
        );
        assert!(select_domain(&snapshot, 1, &[], &user_space).is_ok());
        assert_eq!(
            select_domain(&snapshot, 2, &[], &user_space).unwrap_err(),
            SelectError::MissingMap
        );
    }

    #[test]
    fn legacy_unknown_mode_keeps_exact_single_homing() {
        // Unknown mode preserves the previous unit's contract: exact 1:1
        // positional homing, refusal otherwise.
        let displays = vec![ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0)];
        let snapshot = build_snapshot(&displays, &[row(Some("uuid-1"), &[11], Some(11))], None);
        assert!(select_domain(&snapshot, 1, &[member(7, &[11])], &user_space).is_ok());
        let displays = vec![
            ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
            ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
        ];
        let snapshot = build_snapshot(&displays, &[row(Some("uuid-1"), &[11], Some(11))], None);
        assert!(matches!(
            select_domain(&snapshot, 1, &[], &user_space).unwrap_err(),
            SelectError::AmbiguousDisplaySpace(_)
        ));
    }

    #[test]
    fn multi_display_above_below_negative_origins() {
        // Primary 1440x900 at (0,0); one display above (AppKit y beyond the
        // primary height), one below (negative AppKit y), one left
        // (negative AppKit x). Points pass through; only y flips.
        let displays = vec![
            ns(1, "u1", 0.0, 0.0, 1440.0, 900.0),
            ns(2, "u2", 0.0, 900.0, 1440.0, 900.0),
            ns(3, "u3", 0.0, -900.0, 1440.0, 900.0),
            ns(4, "u4", -1440.0, 0.0, 1440.0, 900.0),
        ];
        let joined = join_displays(&displays);
        let ys: Vec<f64> = joined
            .iter()
            .map(|d| d.cg_bounds.expect("primary known").y)
            .collect();
        assert_eq!(ys, vec![0.0, -900.0, 900.0, 0.0]);
        let xs: Vec<f64> = joined
            .iter()
            .map(|d| d.cg_bounds.expect("primary known").x)
            .collect();
        assert_eq!(xs, vec![0.0, 0.0, 0.0, -1440.0]);
    }

    #[test]
    fn mixed_scale_needs_no_pixel_math() {
        // Points convert identically at either scale; scale rides alongside.
        let displays = vec![ns(1, "u1", 0.0, 0.0, 1512.0, 982.0)];
        let mut two_x = displays.clone();
        two_x[0].scale = 2.0;
        let mut one_x = displays;
        one_x[0].scale = 1.0;
        let a = join_displays(&two_x)[0].cg_bounds;
        let b = join_displays(&one_x)[0].cg_bounds;
        assert_eq!(a, b);
        assert_eq!(join_displays(&two_x)[0].scale, 2.0);
    }

    #[test]
    fn visible_frame_excludes_menu_and_dock() {
        // A 25pt menu inset plus a 32pt Dock strip survive conversion with
        // identical insets (visible != full, insets preserved).
        let display = NsDisplay {
            cg_id: 1,
            uuid: Some("u1".to_owned()),
            frame: DisplayRect {
                x: 0.0,
                y: 0.0,
                w: 1512.0,
                h: 982.0,
            },
            visible: DisplayRect {
                x: 0.0,
                y: 32.0,
                w: 1512.0,
                h: 925.0,
            },
            scale: 2.0,
        };
        let joined = join_displays(&[display])[0].clone();
        let full = joined.cg_bounds.expect("converts");
        let visible = joined.visible.expect("converts");
        assert_eq!((full.x, full.y, full.w, full.h), (0.0, 0.0, 1512.0, 982.0));
        assert_eq!(
            (visible.x, visible.y, visible.w, visible.h),
            (0.0, 25.0, 1512.0, 925.0)
        );
    }

    #[test]
    fn missing_primary_refuses_conversion_and_selection() {
        // No (0,0) origin anywhere: rects stay `None`, selection names the
        // missing geometry instead of guessing an origin.
        let displays = vec![ns(9, "u9", 100.0, 100.0, 800.0, 600.0)];
        assert_eq!(primary_height(&displays), None);
        assert!(join_displays(&displays)[0].cg_bounds.is_none());
        let snapshot = build_snapshot(&displays, &[row(Some("u9"), &[11], Some(11))], Some(true));
        assert_eq!(
            select_domain(&snapshot, 9, &[], &user_space).unwrap_err(),
            SelectError::MissingGeometry(9)
        );
        assert_eq!(
            select_domain(&snapshot, 99, &[], &user_space).unwrap_err(),
            SelectError::MissingDisplay(99)
        );
    }

    #[test]
    fn non_user_and_unknown_current_spaces_refuse() {
        let displays = vec![ns(1, "u1", 0.0, 0.0, 1440.0, 900.0)];
        for (kind, name) in [
            (SlsSpaceType::Fullscreen, "Fullscreen"),
            (SlsSpaceType::System, "System"),
            (SlsSpaceType::Other(9), "Other(9)"),
        ] {
            let snapshot =
                build_snapshot(&displays, &[row(Some("u1"), &[11], Some(11))], Some(true));
            let err = select_domain(&snapshot, 1, &[], &|_| Some(kind)).unwrap_err();
            assert!(
                matches!(err, SelectError::NonUserSpace { .. }),
                "kind {kind:?}"
            );
            assert!(err.to_string().contains(name), "kind named: {err}");
        }
        // Unreadable type refuses distinctly (unknown home, never executed).
        let snapshot = build_snapshot(&displays, &[row(Some("u1"), &[11], Some(11))], Some(true));
        assert_eq!(
            select_domain(&snapshot, 1, &[], &|_| None).unwrap_err(),
            SelectError::UnknownSpaceType(11)
        );
        // Missing current refuses too.
        let snapshot = build_snapshot(&displays, &[row(Some("u1"), &[11], None)], Some(true));
        assert_eq!(
            select_domain(&snapshot, 1, &[], &user_space).unwrap_err(),
            SelectError::MissingCurrentSpace
        );
    }

    #[test]
    fn membership_exact_current_plus_exclusion_evidence() {
        let displays = vec![ns(1, "u1", 0.0, 0.0, 1440.0, 900.0)];
        let snapshot = build_snapshot(
            &displays,
            &[row(Some("u1"), &[11, 12], Some(11))],
            Some(true),
        );
        let domain = select_domain(
            &snapshot,
            1,
            &[
                member(1, &[11]),
                // Fullscreen ON the current user Space stays homed with its
                // flag (the hold gate decides; the domain never drops it).
                WindowMembership {
                    wid: 2,
                    pid: 100,
                    fullscreen: true,
                    spaces: vec![11],
                    frame: None,
                },
                member(3, &[12]),
                member(4, &[11, 12]),
                member(5, &[]),
            ],
            &user_space,
        )
        .expect("user Space selects");
        assert_eq!(domain.space, 11);
        assert_eq!(
            domain
                .windows
                .iter()
                .map(|w| (w.wid, w.fullscreen))
                .collect::<Vec<_>>(),
            vec![(1, false), (2, true)]
        );
        assert_eq!(
            domain
                .excluded
                .iter()
                .map(|e| (e.wid, e.reason))
                .collect::<Vec<_>>(),
            vec![
                (3, ScopeReason::OtherSpace),
                (4, ScopeReason::Sticky),
                (5, ScopeReason::UnknownSpace),
            ]
        );
    }

    #[test]
    fn revision_tracks_geometry_space_and_mode_drift() {
        let displays = vec![ns(1, "u1", 0.0, 0.0, 1440.0, 900.0)];
        let rows = vec![row(Some("u1"), &[11], Some(11))];
        let a = build_snapshot(&displays, &rows, Some(true));
        let again = build_snapshot(&displays, &rows, Some(true));
        assert_eq!(a.revision, again.revision);
        // Geometry drift changes the revision.
        let mut moved = displays.clone();
        moved[0].frame.x = 1440.0;
        assert_ne!(
            build_snapshot(&moved, &rows, Some(true)).revision,
            a.revision
        );
        // Space drift changes the revision.
        assert_ne!(
            build_snapshot(&displays, &[row(Some("u1"), &[11], Some(12))], Some(true)).revision,
            a.revision
        );
        // Mode drift changes the revision.
        assert_ne!(
            build_snapshot(&displays, &rows, Some(false)).revision,
            a.revision
        );
        assert_ne!(build_snapshot(&displays, &rows, None).revision, a.revision);
        // Poll reports exactly the drift.
        let mut poll = TopologyPoll::new();
        assert!(matches!(poll.poll(&a), PollOutcome::Changed(_)));
        assert_eq!(poll.poll(&again), PollOutcome::Unchanged);
        assert!(matches!(
            poll.poll(&build_snapshot(&moved, &rows, Some(true))),
            PollOutcome::Changed(_)
        ));
    }

    #[test]
    fn poll_reset_republishes_after_invalidation() {
        // Recovery contract for the controller: after a failed snapshot
        // read invalidates the published context, the poll resets so the
        // recovered snapshot republishes even when its revision is
        // identical to the pre-failure one. Without the reset the identical
        // recovery reads `Unchanged` forever and the cleared context never
        // republishes.
        let displays = [ns(1, "u1", 0.0, 0.0, 1440.0, 900.0)];
        let rows = [row(Some("u1"), &[11], Some(11))];
        let a = build_snapshot(&displays, &rows, Some(true));
        let mut poll = TopologyPoll::new();
        assert!(matches!(poll.poll(&a), PollOutcome::Changed(_)));
        assert_eq!(poll.poll(&a), PollOutcome::Unchanged);
        // Simulated invalidation recovery: fresh poll, same snapshot.
        poll = TopologyPoll::new();
        assert!(matches!(poll.poll(&a), PollOutcome::Changed(_)));
    }

    #[test]
    fn to_rect_rounds_points_without_guessing() {
        use tiler_core::geometry::Rect;
        // 1:1 rounding (negative origins pass through for left/above
        // displays; only finiteness, positivity, and range gate).
        assert_eq!(
            PointsRect {
                x: 1440.0,
                y: -900.0,
                w: 1440.0,
                h: 900.0,
            }
            .to_rect(),
            Some(Rect {
                x: 1440,
                y: -900,
                w: 1440,
                h: 900,
            })
        );
        assert_eq!(
            PointsRect {
                x: 0.4,
                y: 0.5,
                w: 100.5,
                h: 99.5,
            }
            .to_rect()
            .map(|r| (r.x, r.y, r.w, r.h)),
            Some((0, 1, 101, 100))
        );
        // Refusals: non-finite, degenerate, and out-of-range never
        // saturate into a guessed rect.
        for bad in [
            PointsRect {
                x: f64::NAN,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            PointsRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 10.0,
            },
            PointsRect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: -1.0,
            },
            PointsRect {
                x: 1e30,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
        ] {
            assert_eq!(bad.to_rect(), None);
        }
    }
}
