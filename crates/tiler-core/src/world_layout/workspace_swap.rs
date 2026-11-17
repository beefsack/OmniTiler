//! Typed global-unique two-workspace swap over [`WorldLayout`].
//!
//! Swaps two workspace domains across two outputs while keeping sticky
//! exception floats on their physical outputs. Both sides are prepared and
//! validated before any domain map mutation; any refusal leaves `WorldLayout`
//! untouched.

use std::collections::BTreeSet;

use crate::bounds::is_gap;
use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::session::DomainKey;

use super::outcome::{WorkspaceSwapFailure, WorkspaceSwapOutcome};
use super::relocation::build_relocated_candidate;
use super::state::{DomainException, DomainLayout, WorldLayout};
use super::topology::validate_domain_topology;

/// One directional leg of a global-unique workspace swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceSwapSide {
    pub(crate) source: DomainKey,
    pub(crate) target: DomainKey,
    pub(crate) bounds: Rect,
    pub(crate) gap: i32,
    pub(crate) outer_gap: i32,
}

struct ExtractedException {
    window: WindowId,
    exception: DomainException,
    geometry: Option<Rect>,
    automatic_fixed: bool,
    fixed_tile_override: bool,
    capacity_float: bool,
    admission_order: Option<u64>,
}

struct PreparedSide {
    sticky_ids: BTreeSet<WindowId>,
    sticky: Vec<ExtractedException>,
    moved: Option<DomainLayout>,
}

impl WorldLayout {
    /// Atomically swap two workspace domains across two outputs (`side_a` and
    /// `side_b`), keeping exception windows in `sticky_windows` on their
    /// physical output.
    ///
    /// Preserves exact refusal and state semantics:
    /// - A tiled or minimized sticky window refuses with
    ///   [`WorkspaceSwapFailure::StickyTiledOrMinimized`].
    /// - A stripped side that is empty and had no sticky exceptions produces no
    ///   moved domain and leaves any stale empty source slot untouched.
    /// - A stripped side that is rootless empty after extracting sticky
    ///   exceptions still relocates so opposite physical sticky windows can
    ///   attach to it.
    /// - When one side produces no moved domain and the opposite side has
    ///   sticky exceptions, a focus-cleared rootless stay domain is derived
    ///   from the opposite source onto this side's target key.
    /// - Both relocated/stay candidates are validated before any map mutation.
    pub(crate) fn swap_workspaces(
        &mut self,
        side_a: &WorkspaceSwapSide,
        side_b: &WorkspaceSwapSide,
        sticky_windows: &BTreeSet<WindowId>,
    ) -> Result<WorkspaceSwapOutcome, WorkspaceSwapFailure> {
        if !valid_swap_keys(side_a, side_b) {
            return Err(WorkspaceSwapFailure::InvalidKeys);
        }
        if self.domains.contains_key(&side_a.target) || self.domains.contains_key(&side_b.target) {
            return Err(WorkspaceSwapFailure::TargetOccupied);
        }
        if !valid_swap_geometry(side_a) || !valid_swap_geometry(side_b) {
            return Err(WorkspaceSwapFailure::InvalidGeometry);
        }

        let mut prep_a = prepare_side(self.domain(&side_a.source), side_a, sticky_windows)?;
        let mut prep_b = prepare_side(self.domain(&side_b.source), side_b, sticky_windows)?;

        let mut stay_a = None;
        if prep_a.moved.is_none() && !prep_b.sticky.is_empty() {
            let Some(source_b) = self.domain(&side_b.source) else {
                return Err(WorkspaceSwapFailure::StayFailed);
            };
            let stay = split_sticky_stay(
                &side_b.source,
                source_b,
                &prep_b.sticky_ids,
                &side_a.target,
                side_a.bounds,
                side_a.gap,
                side_a.outer_gap,
            )
            .ok_or(WorkspaceSwapFailure::StayFailed)?;
            stay_a = Some(stay);
        }

        let mut stay_b = None;
        if prep_b.moved.is_none() && !prep_a.sticky.is_empty() {
            let Some(source_a) = self.domain(&side_a.source) else {
                return Err(WorkspaceSwapFailure::StayFailed);
            };
            let stay = split_sticky_stay(
                &side_a.source,
                source_a,
                &prep_a.sticky_ids,
                &side_b.target,
                side_b.bounds,
                side_b.gap,
                side_b.outer_gap,
            )
            .ok_or(WorkspaceSwapFailure::StayFailed)?;
            stay_b = Some(stay);
        }

        if let Some(moved) = prep_a.moved.as_mut()
            && !prep_b.sticky.is_empty()
        {
            insert_exception_floats(moved, prep_b.sticky);
            if !validate_domain_topology(&side_a.target, moved) {
                return Err(WorkspaceSwapFailure::MalformedTopology);
            }
        }
        if let Some(moved) = prep_b.moved.as_mut()
            && !prep_a.sticky.is_empty()
        {
            insert_exception_floats(moved, prep_a.sticky);
            if !validate_domain_topology(&side_b.target, moved) {
                return Err(WorkspaceSwapFailure::MalformedTopology);
            }
        }

        let outcome = WorkspaceSwapOutcome {
            moved_a: prep_a.moved.is_some(),
            moved_b: prep_b.moved.is_some(),
            stay_a: stay_a.is_some(),
            stay_b: stay_b.is_some(),
        };

        if let Some(moved) = prep_a.moved {
            self.domains.remove(&side_a.source);
            self.domains.insert(side_a.target.clone(), moved);
        } else if let Some(stay) = stay_a {
            self.domains.insert(side_a.target.clone(), stay);
        }
        if let Some(moved) = prep_b.moved {
            self.domains.remove(&side_b.source);
            self.domains.insert(side_b.target.clone(), moved);
        } else if let Some(stay) = stay_b {
            self.domains.insert(side_b.target.clone(), stay);
        }

        Ok(outcome)
    }
}

fn valid_swap_keys(side_a: &WorkspaceSwapSide, side_b: &WorkspaceSwapSide) -> bool {
    let keys = [
        &side_a.source,
        &side_a.target,
        &side_b.source,
        &side_b.target,
    ];
    if keys
        .iter()
        .any(|key| key.output.0.is_empty() || key.workspace.0.is_empty())
    {
        return false;
    }
    side_a.source.output != side_a.target.output
        && side_b.source.output != side_b.target.output
        && side_a.source.workspace == side_a.target.workspace
        && side_b.source.workspace == side_b.target.workspace
        && side_a.source.output == side_b.target.output
        && side_b.source.output == side_a.target.output
        && side_a.source.workspace != side_b.source.workspace
}

fn valid_swap_geometry(side: &WorkspaceSwapSide) -> bool {
    side.bounds.w > 0
        && side.bounds.h > 0
        && side.bounds.x.checked_add(side.bounds.w).is_some()
        && side.bounds.y.checked_add(side.bounds.h).is_some()
        && is_gap(side.gap)
        && side.outer_gap >= 0
}

fn prepare_side(
    source: Option<&DomainLayout>,
    side: &WorkspaceSwapSide,
    sticky_windows: &BTreeSet<WindowId>,
) -> Result<PreparedSide, WorkspaceSwapFailure> {
    let Some(source) = source else {
        return Ok(PreparedSide {
            sticky_ids: BTreeSet::new(),
            sticky: Vec::new(),
            moved: None,
        });
    };

    let tiled_sticky = source.leaves.keys().any(|w| sticky_windows.contains(w));
    let minimized_sticky = source
        .minimized_slots
        .keys()
        .any(|w| sticky_windows.contains(w));
    if tiled_sticky || minimized_sticky {
        return Err(WorkspaceSwapFailure::StickyTiledOrMinimized);
    }

    let sticky_ids: BTreeSet<WindowId> = source
        .exceptions
        .keys()
        .filter(|w| sticky_windows.contains(*w))
        .cloned()
        .collect();

    let mut stripped = source.clone();
    let sticky = extract_exception_floats(&mut stripped, &sticky_ids);
    let stripped_empty = stripped.leaves.is_empty()
        && stripped.exceptions.is_empty()
        && stripped.minimized_slots.is_empty();

    let moved = if stripped_empty && sticky.is_empty() {
        None
    } else {
        if !validate_domain_topology(&side.source, &stripped) {
            return Err(WorkspaceSwapFailure::MalformedTopology);
        }
        let relocated = build_relocated_candidate(
            &stripped,
            &side.target,
            side.bounds,
            side.gap,
            side.outer_gap,
        )
        .ok_or(WorkspaceSwapFailure::MalformedTopology)?;
        Some(relocated)
    };

    Ok(PreparedSide {
        sticky_ids,
        sticky,
        moved,
    })
}

fn extract_exception_floats(
    domain: &mut DomainLayout,
    ids: &BTreeSet<WindowId>,
) -> Vec<ExtractedException> {
    let mut out = Vec::new();
    for id in ids {
        let Some(exception) = domain.exceptions.remove(id) else {
            continue;
        };
        out.push(ExtractedException {
            window: id.clone(),
            exception,
            geometry: domain.retained_float_geometry.remove(id),
            automatic_fixed: domain.automatic_fixed.remove(id),
            fixed_tile_override: domain.fixed_tile_override.remove(id),
            capacity_float: domain.capacity_floats.remove(id),
            admission_order: domain.admission_order.remove(id),
        });
    }
    out
}

fn insert_exception_floats(domain: &mut DomainLayout, carried: Vec<ExtractedException>) {
    for carry in carried {
        if domain.exceptions.contains_key(&carry.window)
            || domain.leaves.contains_key(&carry.window)
        {
            continue;
        }
        if let Some(geometry) = carry.geometry {
            domain
                .retained_float_geometry
                .insert(carry.window.clone(), geometry);
        }
        if carry.automatic_fixed {
            domain.automatic_fixed.insert(carry.window.clone());
        }
        if carry.fixed_tile_override {
            domain.fixed_tile_override.insert(carry.window.clone());
        }
        if carry.capacity_float {
            domain.capacity_floats.insert(carry.window.clone());
        }
        if let Some(order) = carry.admission_order {
            domain.admission_order.insert(carry.window.clone(), order);
        }
        domain.exceptions.insert(carry.window, carry.exception);
    }
}

fn split_sticky_stay(
    source_key: &DomainKey,
    source: &DomainLayout,
    sticky_ids: &BTreeSet<WindowId>,
    target: &DomainKey,
    bounds: Rect,
    gap: i32,
    outer_gap: i32,
) -> Option<DomainLayout> {
    if sticky_ids.is_empty() {
        return None;
    }
    if !sticky_ids
        .iter()
        .all(|id| source.exceptions.contains_key(id))
    {
        return None;
    }
    if source_key.output != target.output {
        return None;
    }
    if bounds.w <= 0 || bounds.h <= 0 || !is_gap(gap) || outer_gap < 0 {
        return None;
    }
    if !validate_domain_topology(source_key, source) {
        return None;
    }

    let mut stay = source.clone();
    stay.leaves.clear();
    stay.tree = None;
    stay.minimized_slots.clear();
    stay.focused_leaf = None;
    stay.focus_stack.clear();
    stay.focus_history.clear();
    stay.last_active = None;
    stay.exceptions.retain(|id, _| sticky_ids.contains(id));
    stay.retained_float_geometry
        .retain(|id, _| sticky_ids.contains(id));
    stay.automatic_fixed.retain(|id| sticky_ids.contains(id));
    stay.fixed_tile_override
        .retain(|id| sticky_ids.contains(id));
    stay.capacity_floats.retain(|id| sticky_ids.contains(id));
    stay.admission_order.retain(|id, _| sticky_ids.contains(id));
    stay.bounds = bounds;
    stay.gap = gap;
    stay.outer_gap = outer_gap;

    validate_domain_topology(target, &stay).then_some(stay)
}
