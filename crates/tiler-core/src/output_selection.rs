//! Position-based FULL-rectangle output candidate selection (2026-10-09).
//!
//! Pure, platform-neutral selector over adapter-normalized integer FULL
//! output rectangles. The world owner retains the FULL rectangles and
//! derives a strict unique-reciprocal adjacency map beside the per-use
//! ranked selectors below, so directional world commands resolve from one
//! shared rule. The protocol still carries the obsolete adapter-resolved
//! pair for codec compatibility, but the world never reads it as authority.
//!
//! - Candidates: exact edge-touch with positive perpendicular overlap on
//!   FULL rectangles (panel gaps never block). No wrap.
//! - Strict map (KDE world parity): one direction maps to a target only
//!   when exactly one candidate touches on that side AND exactly that
//!   source touches the target back on the opposite side. Multiple forward
//!   touches or non-unique reverse touches omit the direction, so callers
//!   refuse rather than guess.
//! - Windowed moves/sends (exhausted directional moves all four directions,
//!   explicit output sends): the candidate whose shared edge contains the
//!   moving window centre projection wins; else the largest overlap of the
//!   moving window span with the shared edge; final left/top (smallest `x`,
//!   then smallest `y`, then smallest output id for determinism).
//! - Whole-workspace migration: largest shared edge, then left/top. Never
//!   uses window position.
//! - Ordinary choices fixed here: horizontal projection uses `y`, vertical
//!   uses `x`; centre containment is half-open `[start, end)` evaluated in
//!   doubled coordinates so odd extents keep their exact `.5` centre; the
//!   tertiary tie is the stable output id.
//! - Reverse uniqueness is intentionally NOT required by the per-use ranked
//!   selectors: candidates only need forward edge-touch. A target touched by
//!   two sources on its opposite side is still a valid forward target (valid
//!   reverse ambiguity accepted); the adapter resolves the forward pair
//!   before presenting it.

use std::collections::BTreeMap;

use crate::directional::{Direction, OutputId, WorkspaceId};
use crate::geometry::Rect;

/// One FULL-rectangle output for selection. Callers must present validated
/// topology: every rect positive with representable edges. Unvalidated input
/// fails as [`SelectionError::UnreadableTopology`], never as silent skip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputEntry {
    pub id: OutputId,
    pub rect: Rect,
}

/// Validated candidate with its shared edge against the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: OutputId,
    pub rect: Rect,
    /// Shared edge interval along the edge axis: `[start, end)` in device
    /// units. For left/right this is a `y` interval, for up/down an `x`
    /// interval. Length is always positive.
    pub edge_start: i32,
    pub edge_end: i32,
}

/// Selection failure: some presented rectangle is unreadable
/// (non-positive extent or overflowing edge). Distinct from "no candidate",
/// which is `Ok(None)` / `Ok(vec![])` and a caller-side no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionError {
    UnreadableTopology,
}

fn valid_rect(rect: &Rect) -> bool {
    rect.w > 0
        && rect.h > 0
        && rect.x.checked_add(rect.w).is_some()
        && rect.y.checked_add(rect.h).is_some()
}

fn overlap(a_start: i32, a_len: i32, b_start: i32, b_len: i32) -> i64 {
    let a_end = i64::from(a_start) + i64::from(a_len);
    let b_end = i64::from(b_start) + i64::from(b_len);
    let start = (i64::from(a_start)).max(i64::from(b_start));
    let end = a_end.min(b_end);
    end - start
}

/// Edge-touch candidates in `direction` from `source` over FULL rects.
///
/// Returns candidates in deterministic left/top/id order. `Err` when the
/// source, or any entry in `all` (candidate or not), is unreadable, when
/// output ids repeat, or when the `all` entry matching the source id carries
/// a different rect: callers refuse fail-closed. Empty `Ok` means no
/// candidate (caller-side no-op).
pub fn candidates_in_direction(
    source: &OutputEntry,
    direction: Direction,
    all: &[OutputEntry],
) -> Result<Vec<Candidate>, SelectionError> {
    if !valid_rect(&source.rect) {
        return Err(SelectionError::UnreadableTopology);
    }
    let mut seen_source = false;
    let mut seen_ids = std::collections::HashSet::new();
    for entry in all {
        if !valid_rect(&entry.rect) {
            return Err(SelectionError::UnreadableTopology);
        }
        if !seen_ids.insert(entry.id.clone()) {
            return Err(SelectionError::UnreadableTopology);
        }
        if entry.id == source.id {
            if entry.rect != source.rect {
                return Err(SelectionError::UnreadableTopology);
            }
            seen_source = true;
        }
    }
    if !seen_source {
        return Err(SelectionError::UnreadableTopology);
    }
    let mut out = Vec::new();
    for entry in all {
        if entry.id == source.id {
            continue;
        }
        let touches = match direction {
            Direction::Left => entry.rect.x.checked_add(entry.rect.w) == Some(source.rect.x),
            Direction::Right => source.rect.x.checked_add(source.rect.w) == Some(entry.rect.x),
            Direction::Up => entry.rect.y.checked_add(entry.rect.h) == Some(source.rect.y),
            Direction::Down => source.rect.y.checked_add(source.rect.h) == Some(entry.rect.y),
        };
        if !touches {
            continue;
        }
        let (start, end): (i64, i64) = match direction {
            Direction::Left | Direction::Right => {
                let s = i64::from(source.rect.y.max(entry.rect.y));
                let e = (i64::from(source.rect.y) + i64::from(source.rect.h))
                    .min(i64::from(entry.rect.y) + i64::from(entry.rect.h));
                (s, e)
            }
            Direction::Up | Direction::Down => {
                let s = i64::from(source.rect.x.max(entry.rect.x));
                let e = (i64::from(source.rect.x) + i64::from(source.rect.w))
                    .min(i64::from(entry.rect.x) + i64::from(entry.rect.w));
                (s, e)
            }
        };
        if end - start <= 0 {
            continue;
        }
        let Ok(start_i32) = i32::try_from(start) else {
            return Err(SelectionError::UnreadableTopology);
        };
        let len = end - start;
        let Ok(len_i32) = i32::try_from(len) else {
            return Err(SelectionError::UnreadableTopology);
        };
        let Some(edge_end) = start_i32.checked_add(len_i32) else {
            return Err(SelectionError::UnreadableTopology);
        };
        out.push(Candidate {
            id: entry.id.clone(),
            rect: entry.rect,
            edge_start: start_i32,
            edge_end,
        });
    }
    out.sort_by(|a, b| {
        a.rect
            .x
            .cmp(&b.rect.x)
            .then(a.rect.y.cmp(&b.rect.y))
            .then(a.id.0.cmp(&b.id.0))
    });
    Ok(out)
}

fn centre_contained(window: &Rect, edge_start: i32, edge_end: i32, horizontal: bool) -> bool {
    // Doubled centre keeps odd-extent `.5` exact under half-open containment.
    let doubled = if horizontal {
        i64::from(window.y)
            .checked_mul(2)
            .and_then(|v| v.checked_add(i64::from(window.h)))
    } else {
        i64::from(window.x)
            .checked_mul(2)
            .and_then(|v| v.checked_add(i64::from(window.w)))
    };
    let Some(doubled) = doubled else {
        return false;
    };
    doubled >= i64::from(edge_start) * 2 && doubled < i64::from(edge_end) * 2
}

fn window_edge_overlap(window: &Rect, candidate: &Candidate, horizontal: bool) -> i64 {
    if horizontal {
        overlap(
            window.y,
            window.h,
            candidate.edge_start,
            candidate.edge_end - candidate.edge_start,
        )
        .max(0)
    } else {
        overlap(
            window.x,
            window.w,
            candidate.edge_start,
            candidate.edge_end - candidate.edge_start,
        )
        .max(0)
    }
}

/// Portable observed-rectangle grounding over FULL rectangles.
///
/// The rectangle centre decides first (half-open `[start, end)`
/// containment, first in slice order wins); otherwise the largest overlap
/// area wins with first-wins ties (the all-zero case included, so the first
/// entry wins there). `None` only when `all` is empty. This reports which
/// screen an observed frame sits on; it authorizes no write, focus, or
/// completion verdict. Native lifetime/freshness/scope vetoes stay with the
/// adapter caller.
#[must_use]
pub fn output_for_rect(all: &[OutputEntry], rect: &Rect) -> Option<OutputId> {
    let cx = i64::from(rect.x) + i64::from(rect.w) / 2;
    let cy = i64::from(rect.y) + i64::from(rect.h) / 2;
    for entry in all {
        let r = &entry.rect;
        let end_x = i64::from(r.x) + i64::from(r.w);
        let end_y = i64::from(r.y) + i64::from(r.h);
        if cx >= i64::from(r.x) && cx < end_x && cy >= i64::from(r.y) && cy < end_y {
            return Some(entry.id.clone());
        }
    }
    let mut best: Option<(&OutputId, i64)> = None;
    for entry in all {
        let r = &entry.rect;
        let ox = (i64::from(rect.x) + i64::from(rect.w)).min(i64::from(r.x) + i64::from(r.w))
            - i64::from(rect.x).max(i64::from(r.x));
        let oy = (i64::from(rect.y) + i64::from(rect.h)).min(i64::from(r.y) + i64::from(r.h))
            - i64::from(rect.y).max(i64::from(r.y));
        let overlap = ox.max(0) * oy.max(0);
        if best.as_ref().is_none_or(|(_, known)| overlap > *known) {
            best = Some((&entry.id, overlap));
        }
    }
    best.map(|(id, _)| id.clone())
}

/// Windowed selection for exhausted moves and explicit sends.
///
/// `window` is the moving window frame rect (FULL-coordinate space).
/// `Err` on any unreadable topology (including `window`); `Ok(None)` when
/// no candidate touches (caller-side no-op).
pub fn select_for_window(
    direction: Direction,
    source: &OutputEntry,
    window: &Rect,
    all: &[OutputEntry],
) -> Result<Option<Candidate>, SelectionError> {
    if !valid_rect(window) {
        return Err(SelectionError::UnreadableTopology);
    }
    let candidates = candidates_in_direction(source, direction, all)?;
    if candidates.is_empty() {
        return Ok(None);
    }
    let horizontal = matches!(direction, Direction::Left | Direction::Right);
    // Literal contract ranking: centre containment first; span overlap only
    // when neither shared edge contains the centre; then deterministic
    // left/top/id. Two containing edges (overlapping/mirrored rectangles)
    // skip overlap straight to left/top/id.
    let mut ranked = candidates;
    ranked.sort_by(|a, b| {
        let a_centred = centre_contained(window, a.edge_start, a.edge_end, horizontal);
        let b_centred = centre_contained(window, b.edge_start, b.edge_end, horizontal);
        b_centred
            .cmp(&a_centred)
            .then_with(|| {
                if a_centred && b_centred {
                    std::cmp::Ordering::Equal
                } else {
                    window_edge_overlap(window, b, horizontal)
                        .cmp(&window_edge_overlap(window, a, horizontal))
                }
            })
            .then(a.rect.x.cmp(&b.rect.x))
            .then(a.rect.y.cmp(&b.rect.y))
            .then(a.id.0.cmp(&b.id.0))
    });
    Ok(ranked.into_iter().next())
}

/// Migration selection for whole-workspace moves: largest shared edge,
/// then left/top/id. Never uses window position.
pub fn select_for_migration(
    direction: Direction,
    source: &OutputEntry,
    all: &[OutputEntry],
) -> Result<Option<Candidate>, SelectionError> {
    let mut candidates = candidates_in_direction(source, direction, all)?;
    if candidates.is_empty() {
        return Ok(None);
    }
    candidates.sort_by(|a, b| {
        let a_len = i64::from(a.edge_end) - i64::from(a.edge_start);
        let b_len = i64::from(b.edge_end) - i64::from(b.edge_start);
        b_len
            .cmp(&a_len)
            .then(a.rect.x.cmp(&b.rect.x))
            .then(a.rect.y.cmp(&b.rect.y))
            .then(a.id.0.cmp(&b.id.0))
    });
    Ok(candidates.into_iter().next())
}

/// Strict unique-reciprocal adjacency over FULL rectangles (KDE world
/// parity, extracted 2026-10-18).
///
/// One `(source, direction)` maps to a target only when exactly one
/// candidate touches the source on that side AND exactly that source
/// touches the candidate back on the opposite side. Multiple forward
/// touches (split edges) or non-unique reverse touches omit the direction,
/// so callers refuse rather than guess; gaps, corner-only contact, and
/// non-touching arrangements map to nothing. `Ok` maps are empty when no
/// direction resolves. `Err` on any unreadable rectangle (non-positive
/// extent or overflowing edge) or duplicate output id: callers refuse
/// fail-closed, never derive from the readable subset.
pub fn unique_reciprocal_adjacency(
    all: &[OutputEntry],
) -> Result<BTreeMap<(OutputId, Direction), OutputId>, SelectionError> {
    const DIRECTIONS: [Direction; 4] = [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ];
    const fn opposite(direction: Direction) -> Direction {
        match direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }
    let mut out = BTreeMap::new();
    for source in all {
        for direction in DIRECTIONS {
            let candidates = candidates_in_direction(source, direction, all)?;
            if candidates.len() != 1 {
                continue;
            }
            let target = &candidates[0];
            let back = OutputEntry {
                id: target.id.clone(),
                rect: target.rect,
            };
            let reverse = candidates_in_direction(&back, opposite(direction), all)?;
            if reverse.len() == 1 && reverse[0].id == source.id {
                out.insert((source.id.clone(), direction), target.id.clone());
            }
        }
    }
    Ok(out)
}

/// R-WS-12 G-37 migration source-refill setting (decisions 2026-10-10 D5).
///
/// One global setting with two validated values: `last-remaining-workspace`
/// (default, COSMIC: the last remaining scoped workspace refills the source)
/// and `most-recently-used-workspace` (bspwm/i3/awesome: the remembered
/// item-1.2 per-output previous workspace refills the source when eligible).
/// The wire tokens are `last-remaining-workspace` and
/// `most-recently-used-workspace`; a missing settings field decodes to the
/// default. Anything else refuses at the settings boundary, never here.
/// Destination insertion (D4) is unchanged by this setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MigrationSourceRefill {
    /// Last remaining workspace (default).
    #[default]
    LastRemaining,
    /// Most recently used remaining workspace (item-1.2 history with
    /// last-remaining fallback).
    MostRecentlyUsed,
}

impl MigrationSourceRefill {
    /// Wire token for this value (`last-remaining-workspace` /
    /// `most-recently-used-workspace`).
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::LastRemaining => "last-remaining-workspace",
            Self::MostRecentlyUsed => "most-recently-used-workspace",
        }
    }

    /// Validated parse of a wire token. `None` for anything else, including
    /// empty strings: callers refuse fail-closed.
    #[must_use]
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "last-remaining-workspace" => Some(Self::LastRemaining),
            "most-recently-used-workspace" => Some(Self::MostRecentlyUsed),
            _ => None,
        }
    }
}

/// Pure G-37 source-refill selector over one source output scope.
///
/// `remaining_in_scope_order` is the remaining source-output scoped backing
/// ids in scoped order AFTER excluding the migrated id; `previous` is the
/// item-1.2 remembered previous stable id snapshotted before the migration
/// mutates mappings. Eligibility is the smallest rule: under
/// [`MigrationSourceRefill::MostRecentlyUsed`], the remembered id refills
/// when it is still a member of the remaining scope (live, still assigned
/// the source, surviving empties valid); otherwise the last remaining id
/// refills, exactly as under [`MigrationSourceRefill::LastRemaining`].
/// Never recreates or reinterprets ordinals: `None` only when nothing
/// remains. History invalidation (items 1.3/1.5) stays with the caller.
#[must_use]
pub fn select_migration_source_refill(
    refill: MigrationSourceRefill,
    previous: Option<&WorkspaceId>,
    remaining_in_scope_order: &[WorkspaceId],
) -> Option<WorkspaceId> {
    if refill == MigrationSourceRefill::MostRecentlyUsed
        && let Some(remembered) = previous
        && remaining_in_scope_order.contains(remembered)
    {
        return Some(remembered.clone());
    }
    remaining_in_scope_order.last().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, x: i32, y: i32, w: i32, h: i32) -> OutputEntry {
        OutputEntry {
            id: OutputId(id.to_owned()),
            rect: Rect { x, y, w, h },
        }
    }

    fn strict_map(all: &[OutputEntry]) -> BTreeMap<(OutputId, Direction), OutputId> {
        unique_reciprocal_adjacency(all).expect("readable topology")
    }

    fn lookup<'a>(
        map: &'a BTreeMap<(OutputId, Direction), OutputId>,
        source: &'a str,
        direction: Direction,
    ) -> Option<&'a str> {
        map.get(&(OutputId(source.to_owned()), direction))
            .map(|id| id.0.as_str())
    }

    #[test]
    fn no_candidate_without_touch() {
        let source = entry("s", 0, 0, 800, 600);
        let far = entry("f", 900, 0, 800, 600);
        assert!(
            candidates_in_direction(&source, Direction::Right, &[source.clone(), far])
                .expect("readable")
                .is_empty()
        );
    }

    #[test]
    fn unreadable_source_refuses() {
        let source = entry("s", 0, 0, 0, 600);
        let next = entry("n", 0, 0, 800, 600);
        let all = vec![source.clone(), next];
        assert_eq!(
            candidates_in_direction(&source, Direction::Right, &all),
            Err(SelectionError::UnreadableTopology)
        );
    }

    #[test]
    fn unreadable_non_source_refuses() {
        let source = entry("s", 0, 0, 800, 600);
        let bad = entry("bad", 800, 0, 0, 600);
        let all = vec![source.clone(), bad];
        assert_eq!(
            candidates_in_direction(&source, Direction::Right, &all),
            Err(SelectionError::UnreadableTopology)
        );
    }

    #[test]
    fn duplicate_ids_refuse() {
        let source = entry("s", 0, 0, 800, 600);
        let dup = entry("s", 800, 0, 800, 600);
        let all = vec![source.clone(), dup];
        assert_eq!(
            candidates_in_direction(&source, Direction::Right, &all),
            Err(SelectionError::UnreadableTopology)
        );
    }

    #[test]
    fn rect_grounds_centre_then_largest_overlap() {
        let all = vec![
            entry("left", 0, 0, 800, 600),
            entry("right", 800, 0, 800, 600),
        ];
        // Centre inside wins.
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: 700,
                    y: 100,
                    w: 400,
                    h: 200
                }
            )
            .map(|id| id.0),
            Some("right".to_owned())
        );
        // Centre outside every output but overlapping the right one: the
        // sliver grounds right, not the primary-first fallback.
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: 1500,
                    y: 100,
                    w: 400,
                    h: 200
                }
            )
            .map(|id| id.0),
            Some("right".to_owned())
        );
        // Disjoint from every output: all-zero overlaps keep first entry.
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: -500,
                    y: 100,
                    w: 100,
                    h: 100
                }
            )
            .map(|id| id.0),
            Some("left".to_owned())
        );
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: 2000,
                    y: 100,
                    w: 100,
                    h: 100
                }
            )
            .map(|id| id.0),
            Some("left".to_owned())
        );
        let empty: Vec<OutputEntry> = Vec::new();
        assert_eq!(
            output_for_rect(
                &empty,
                &Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100
                }
            ),
            None
        );
    }

    #[test]
    fn rect_grounds_half_open_edge_with_odd_extent_rounding() {
        let all = vec![
            entry("left", 0, 0, 800, 600),
            entry("right", 800, 0, 800, 600),
        ];
        // Integer-division centre (`x + w / 2`, matching the retired adapter
        // formula): width 1 keeps the centre at 799 (left), width 2 moves it
        // onto the shared edge at 800, which belongs to the right screen.
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: 799,
                    y: 100,
                    w: 1,
                    h: 100
                }
            )
            .map(|id| id.0),
            Some("left".to_owned())
        );
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: 799,
                    y: 100,
                    w: 2,
                    h: 100
                }
            )
            .map(|id| id.0),
            Some("right".to_owned())
        );
    }

    #[test]
    fn rect_grounds_centre_ahead_of_larger_overlap() {
        let all = vec![
            entry("left", 600, 0, 200, 600),
            entry("right", 800, 0, 800, 600),
        ];
        // Centre at 700 sits in left, whose overlap is 200 * 200 versus
        // right's 600 * 200: containment takes precedence over larger area.
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: 0,
                    y: 0,
                    w: 1400,
                    h: 200
                }
            )
            .map(|id| id.0),
            Some("left".to_owned())
        );
    }

    #[test]
    fn rect_grounds_negative_origins() {
        let all = vec![
            entry("west", -1600, 0, 800, 600),
            entry("east", -800, 0, 800, 600),
        ];
        // Centre inside east.
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: -750,
                    y: 100,
                    w: 100,
                    h: 100
                }
            )
            .map(|id| id.0),
            Some("east".to_owned())
        );
        // Centre at 50 is outside both, with a 100px sliver on the
        // non-first east output: grounds east, not the first entry.
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: -100,
                    y: 100,
                    w: 300,
                    h: 100
                }
            )
            .map(|id| id.0),
            Some("east".to_owned())
        );
        // Fully disjoint on the far side: first entry wins the all-zero tie.
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: -2000,
                    y: 100,
                    w: 100,
                    h: 100
                }
            )
            .map(|id| id.0),
            Some("west".to_owned())
        );
    }

    #[test]
    fn rect_grounds_without_i32_overflow() {
        let all = vec![
            entry("left", 0, 0, 800, 600),
            entry("right", 800, 0, 800, 600),
        ];
        // `x + w` overflows i32 here; i64 grounding stays exact and the
        // disjoint frame keeps the first entry without panicking.
        assert_eq!(
            output_for_rect(
                &all,
                &Rect {
                    x: i32::MAX - 100,
                    y: 0,
                    w: 200,
                    h: 100
                }
            )
            .map(|id| id.0),
            Some("left".to_owned())
        );
        // Extreme topology edges overflow i32 as well: containment on the
        // far output still resolves.
        let far = vec![
            entry("left", 0, 0, 800, 600),
            entry("far", i32::MAX - 800, 0, 1600, 600),
        ];
        assert_eq!(
            output_for_rect(
                &far,
                &Rect {
                    x: i32::MAX - 700,
                    y: 100,
                    w: 100,
                    h: 100
                }
            )
            .map(|id| id.0),
            Some("far".to_owned())
        );
    }

    fn workspace(id: &str) -> WorkspaceId {
        WorkspaceId(id.to_owned())
    }

    #[test]
    fn migration_refill_wire_roundtrips_with_last_remaining_default() {
        assert_eq!(
            MigrationSourceRefill::default(),
            MigrationSourceRefill::LastRemaining
        );
        assert_eq!(
            MigrationSourceRefill::LastRemaining.as_wire_str(),
            "last-remaining-workspace"
        );
        assert_eq!(
            MigrationSourceRefill::MostRecentlyUsed.as_wire_str(),
            "most-recently-used-workspace"
        );
        assert_eq!(
            MigrationSourceRefill::parse_wire("last-remaining-workspace"),
            Some(MigrationSourceRefill::LastRemaining)
        );
        assert_eq!(
            MigrationSourceRefill::parse_wire("most-recently-used-workspace"),
            Some(MigrationSourceRefill::MostRecentlyUsed)
        );
        for invalid in ["", "mru", "last-remaining", "most-recently-used", "COSMIC"] {
            assert_eq!(
                MigrationSourceRefill::parse_wire(invalid),
                None,
                "{invalid:?}"
            );
        }
    }

    #[test]
    fn migration_refill_mru_prefers_eligible_previous_over_trailing_empty() {
        let remaining = vec![workspace("ws-1"), workspace("ws-e")];
        assert_eq!(
            select_migration_source_refill(
                MigrationSourceRefill::MostRecentlyUsed,
                Some(&workspace("ws-1")),
                &remaining,
            ),
            Some(workspace("ws-1"))
        );
        assert_eq!(
            select_migration_source_refill(
                MigrationSourceRefill::LastRemaining,
                Some(&workspace("ws-1")),
                &remaining,
            ),
            Some(workspace("ws-e"))
        );
    }

    #[test]
    fn migration_refill_mru_falls_back_without_eligible_previous() {
        let remaining = vec![workspace("ws-1"), workspace("ws-e")];
        // No history.
        assert_eq!(
            select_migration_source_refill(
                MigrationSourceRefill::MostRecentlyUsed,
                None,
                &remaining
            ),
            Some(workspace("ws-e"))
        );
        // Remembered id is the migrated one (excluded from remaining).
        assert_eq!(
            select_migration_source_refill(
                MigrationSourceRefill::MostRecentlyUsed,
                Some(&workspace("ws-2")),
                &remaining,
            ),
            Some(workspace("ws-e"))
        );
        // Remembered id moved out of scope or was removed.
        assert_eq!(
            select_migration_source_refill(
                MigrationSourceRefill::MostRecentlyUsed,
                Some(&workspace("ws-gone")),
                &remaining,
            ),
            Some(workspace("ws-e"))
        );
        // Nothing remains: no recreation.
        assert_eq!(
            select_migration_source_refill(
                MigrationSourceRefill::MostRecentlyUsed,
                Some(&workspace("ws-1")),
                &[],
            ),
            None
        );
        assert_eq!(
            select_migration_source_refill(MigrationSourceRefill::LastRemaining, None, &[]),
            None
        );
    }

    #[test]
    fn migration_refill_mru_accepts_surviving_empty_previous() {
        // The remembered empty workspace survives in scope, so it refills
        // even though it holds no windows.
        let remaining = vec![workspace("ws-empty"), workspace("ws-e")];
        assert_eq!(
            select_migration_source_refill(
                MigrationSourceRefill::MostRecentlyUsed,
                Some(&workspace("ws-empty")),
                &remaining,
            ),
            Some(workspace("ws-empty"))
        );
    }

    #[test]
    fn strict_map_two_outputs_reciprocal() {
        let all = vec![
            entry("left", 0, 0, 800, 600),
            entry("right", 800, 0, 800, 600),
        ];
        let map = strict_map(&all);
        assert_eq!(lookup(&map, "left", Direction::Right), Some("right"));
        assert_eq!(lookup(&map, "right", Direction::Left), Some("left"));
        assert_eq!(lookup(&map, "left", Direction::Left), None);
        assert_eq!(lookup(&map, "left", Direction::Up), None);
        assert_eq!(lookup(&map, "right", Direction::Down), None);
    }

    #[test]
    fn strict_map_vertical_stack_and_negative_origins() {
        let all = vec![
            entry("top", -800, -600, 800, 600),
            entry("bottom", -800, 0, 800, 600),
        ];
        let map = strict_map(&all);
        assert_eq!(lookup(&map, "top", Direction::Down), Some("bottom"));
        assert_eq!(lookup(&map, "bottom", Direction::Up), Some("top"));
    }

    #[test]
    fn strict_map_three_in_row_middle_has_both() {
        let all = vec![
            entry("a", 0, 0, 800, 600),
            entry("b", 800, 0, 800, 600),
            entry("c", 1600, 0, 800, 600),
        ];
        let map = strict_map(&all);
        assert_eq!(lookup(&map, "a", Direction::Right), Some("b"));
        assert_eq!(lookup(&map, "b", Direction::Left), Some("a"));
        assert_eq!(lookup(&map, "b", Direction::Right), Some("c"));
        assert_eq!(lookup(&map, "c", Direction::Left), Some("b"));
        assert_eq!(lookup(&map, "a", Direction::Left), None);
        assert_eq!(lookup(&map, "c", Direction::Right), None);
    }

    #[test]
    fn strict_map_split_edge_omits_multi_but_keeps_sibling_row() {
        // Wide source under two outputs: the forward multi-touch omits the
        // upward direction (and, by reverse count, each upper output's
        // downward direction), while the upper row still resolves
        // horizontally against each other.
        let all = vec![
            entry("src", 0, 600, 1600, 600),
            entry("up-l", 0, 0, 800, 600),
            entry("up-r", 800, 0, 800, 600),
        ];
        let map = strict_map(&all);
        assert_eq!(lookup(&map, "src", Direction::Up), None);
        assert_eq!(lookup(&map, "up-l", Direction::Down), None);
        assert_eq!(lookup(&map, "up-r", Direction::Down), None);
        assert_eq!(lookup(&map, "up-l", Direction::Right), Some("up-r"));
        assert_eq!(lookup(&map, "up-r", Direction::Left), Some("up-l"));
    }

    #[test]
    fn strict_map_reverse_ambiguity_omits_vertical_keeps_row() {
        // Two sources above one wide target: each source's downward touch
        // is single, but the target's opposite side is touched twice, so
        // nothing vertical resolves; the top row still resolves
        // horizontally.
        let all = vec![
            entry("s1", 0, 0, 800, 600),
            entry("s2", 800, 0, 800, 600),
            entry("wide", 0, 600, 1600, 600),
        ];
        let map = strict_map(&all);
        assert_eq!(lookup(&map, "s1", Direction::Down), None);
        assert_eq!(lookup(&map, "s2", Direction::Down), None);
        assert_eq!(lookup(&map, "wide", Direction::Up), None);
        assert_eq!(lookup(&map, "s1", Direction::Right), Some("s2"));
        assert_eq!(lookup(&map, "s2", Direction::Left), Some("s1"));
    }

    #[test]
    fn strict_map_tall_against_stacked_pair_omits_sides() {
        // One tall output against two stacked halves: both side directions
        // are ambiguous, while the stacked pair still resolves vertically.
        let all = vec![
            entry("tall", 0, 0, 800, 1200),
            entry("top", 800, 0, 800, 600),
            entry("bottom", 800, 600, 800, 600),
        ];
        let map = strict_map(&all);
        assert_eq!(lookup(&map, "tall", Direction::Right), None);
        assert_eq!(lookup(&map, "top", Direction::Left), None);
        assert_eq!(lookup(&map, "bottom", Direction::Left), None);
        assert_eq!(lookup(&map, "top", Direction::Down), Some("bottom"));
        assert_eq!(lookup(&map, "bottom", Direction::Up), Some("top"));
    }

    #[test]
    fn strict_map_gaps_and_corners_omit() {
        let gap = vec![entry("a", 0, 0, 800, 600), entry("b", 900, 0, 800, 600)];
        assert!(strict_map(&gap).is_empty());
        // Corner-only contact overlaps zero: no candidate either way.
        let corner = vec![entry("a", 0, 0, 800, 600), entry("b", 800, 600, 800, 600)];
        assert!(strict_map(&corner).is_empty());
        // Mirrored overlaps above a wide source: forward and reverse are
        // both multi, so nothing resolves.
        let mirrored = vec![
            entry("src", 0, 600, 1600, 600),
            entry("a", 0, 0, 1200, 600),
            entry("b", 400, 0, 1200, 600),
        ];
        assert!(strict_map(&mirrored).is_empty());
    }

    #[test]
    fn strict_map_single_output_is_empty() {
        let all = vec![entry("solo", 0, 0, 800, 600)];
        assert!(strict_map(&all).is_empty());
        let empty: Vec<OutputEntry> = Vec::new();
        assert!(strict_map(&empty).is_empty());
    }

    #[test]
    fn strict_map_rejects_unreadable_and_duplicates() {
        let source = entry("s", 0, 0, 800, 600);
        let bad = entry("bad", 800, 0, 0, 600);
        assert_eq!(
            unique_reciprocal_adjacency(&[source.clone(), bad]),
            Err(SelectionError::UnreadableTopology)
        );
        let dup = entry("s", 800, 0, 800, 600);
        assert_eq!(
            unique_reciprocal_adjacency(&[source.clone(), dup]),
            Err(SelectionError::UnreadableTopology)
        );
        let overflow = entry("big", i32::MAX, 0, 1, 600);
        let neighbor = entry("n", 0, 0, 800, 600);
        assert_eq!(
            unique_reciprocal_adjacency(&[overflow, neighbor]),
            Err(SelectionError::UnreadableTopology)
        );
    }
}
