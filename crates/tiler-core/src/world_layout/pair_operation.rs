//! Shared two-domain pair convergence and geometry helpers for directional
//! pair operations and explicit cross-domain sends.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Direction, WindowId};
use crate::seed::{EngineWindow, observed_window_from_engine};
use crate::session::{DesiredGeometry, DomainKey, ObservedWindow, OutputDomain};
use crate::size_hints::WindowSizeHints;

use super::focus::{
    domain_focus_resolves, settle_domain_accepted_focus_tail, settle_domain_focus_tail,
    sync_domain_focus_from_window,
};
use super::local_move::geometry_covers_domain;
use super::observation::domain_observed_known_match;
use super::outcome::PairOperationFailure;
use super::pair::{PairAssemblyFailure, PairCandidate};
use super::projection::{project_borrowed_geometry, sort_geometry};
use super::state::WorldLayout;

/// Borrowed geometry and observation context for one two-domain pair operation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PairOperationContext<'a> {
    pub(crate) source_domain: &'a OutputDomain,
    pub(crate) target_domain: &'a OutputDomain,
    pub(crate) source_outer_gap: i32,
    pub(crate) target_outer_gap: Option<i32>,
    pub(crate) observed_focus: &'a WindowId,
    pub(crate) observed: &'a [EngineWindow],
    pub(crate) closed: &'a [WindowId],
}

impl WorldLayout {
    /// Assemble a canonical pair, converge both domains in one pass, run
    /// close-slot cleanup, install the converged pair nonconsumingly, and sync
    /// source focus from `ctx.observed_focus`.
    ///
    /// Returns the same canonical [`PairCandidate`] working view for follow-up
    /// command planning so target IDs are never reassembled or recanonicalized.
    pub(super) fn prepare_converged_pair(
        &mut self,
        ctx: &PairOperationContext<'_>,
        require_adjacency: bool,
    ) -> Result<(PairCandidate, Vec<ObservedWindow>), PairOperationFailure> {
        let pair = self.assemble_pair(ctx.source_domain, ctx.target_domain, require_adjacency)?;
        self.converge_assembled_pair(pair, ctx)
    }

    pub(super) fn prepare_converged_occupied_pair(
        &mut self,
        ctx: &PairOperationContext<'_>,
    ) -> Result<(PairCandidate, Vec<ObservedWindow>), PairOperationFailure> {
        let pair = self.assemble_pair_occupied(ctx.source_domain, ctx.target_domain)?;
        self.converge_assembled_pair(pair, ctx)
    }

    fn converge_assembled_pair(
        &mut self,
        mut pair: PairCandidate,
        ctx: &PairOperationContext<'_>,
    ) -> Result<(PairCandidate, Vec<ObservedWindow>), PairOperationFailure> {
        let observed: Vec<ObservedWindow> = ctx
            .observed
            .iter()
            .map(observed_window_from_engine)
            .collect();
        let hidden = hidden_for_pair(&pair, &observed, ctx.closed);
        let focus = (!ctx.observed_focus.0.is_empty()).then_some(ctx.observed_focus);

        self.converge_pair(&mut pair, &observed, focus, &hidden)?;

        for window in ctx.closed {
            pair.forget_minimized_slot(window);
        }

        self.install_pair(&pair, ctx.source_outer_gap, None)
            .map_err(|_| PairOperationFailure::Assembly(PairAssemblyFailure::CommitRejected))?;

        sync_pair_source_focus_from_window(&mut pair, ctx.observed_focus);

        Ok((pair, observed))
    }
}

pub(super) fn hidden_for_pair(
    pair: &PairCandidate,
    observed: &[ObservedWindow],
    closed: &[WindowId],
) -> BTreeSet<WindowId> {
    let observed_ids: BTreeSet<&WindowId> = observed.iter().map(|w| &w.window).collect();
    let closed_ids: BTreeSet<&WindowId> = closed.iter().collect();
    let mut hidden = BTreeSet::new();
    for id in pair
        .source
        .leaves
        .keys()
        .chain(pair.source.exceptions.keys())
        .chain(pair.source.minimized_slots.keys())
        .chain(pair.target.leaves.keys())
        .chain(pair.target.exceptions.keys())
        .chain(pair.target.minimized_slots.keys())
    {
        if !observed_ids.contains(id) && !closed_ids.contains(id) {
            hidden.insert(id.clone());
        }
    }
    hidden
}

pub(super) fn sync_pair_source_focus_from_window(pair: &mut PairCandidate, window: &WindowId) {
    if let Some(leaf) = sync_domain_focus_from_window(&mut pair.source, window)
        && domain_focus_resolves(&pair.source, &leaf)
    {
        pair.focused_domain = Some(pair.source_key());
        pair.focused_leaf = Some(leaf);
    }
}

pub(super) fn opposite_direction(direction: Direction) -> Direction {
    match direction {
        Direction::Left => Direction::Right,
        Direction::Right => Direction::Left,
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
    }
}

pub(super) fn pair_known_windows(pair: &PairCandidate) -> BTreeSet<&WindowId> {
    pair.source
        .leaves
        .keys()
        .chain(pair.source.exceptions.keys())
        .chain(pair.target.leaves.keys())
        .chain(pair.target.exceptions.keys())
        .collect()
}

pub(super) fn pair_observed_known_match(
    pair: &PairCandidate,
    observed: &[ObservedWindow],
    fixed_admission: bool,
) -> bool {
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    let mut source_obs = Vec::new();
    let mut target_obs = Vec::new();
    for entry in observed {
        let key = DomainKey {
            output: entry.output.clone(),
            workspace: entry.workspace.clone(),
        };
        if key == source_key {
            source_obs.push(entry.clone());
        } else if key == target_key {
            target_obs.push(entry.clone());
        } else {
            return false;
        }
    }
    domain_observed_known_match(&source_key, &pair.source, &source_obs, fixed_admission)
        && domain_observed_known_match(&target_key, &pair.target, &target_obs, fixed_admission)
}

pub(super) fn hints_from_observed(
    observed: &[ObservedWindow],
) -> BTreeMap<WindowId, WindowSizeHints> {
    observed
        .iter()
        .map(|entry| (entry.window.clone(), entry.hints))
        .collect()
}

pub(super) fn project_pair_geometry(
    pair: &PairCandidate,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
) -> Option<Vec<DesiredGeometry>> {
    let mut geometry = Vec::new();
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    if let Some(tree) = pair.source.tree.as_ref() {
        let source_geo = project_borrowed_geometry(
            &source_key,
            tree,
            &pair.source.leaves,
            pair.source.bounds,
            pair.source.gap,
            hints,
        )?;
        geometry.extend(source_geo);
    } else if !pair.source.leaves.is_empty() {
        return None;
    }
    if let Some(tree) = pair.target.tree.as_ref() {
        let target_geo = project_borrowed_geometry(
            &target_key,
            tree,
            &pair.target.leaves,
            pair.target.bounds,
            pair.target.gap,
            hints,
        )?;
        geometry.extend(target_geo);
    } else if !pair.target.leaves.is_empty() {
        return None;
    }
    sort_geometry(&mut geometry);
    Some(geometry)
}

pub(super) fn pair_geometry_covers_domains(
    geometry: &[DesiredGeometry],
    pair: &PairCandidate,
) -> bool {
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    let mut source_geo = Vec::new();
    let mut target_geo = Vec::new();
    for entry in geometry {
        if entry.output == source_key.output && entry.workspace == source_key.workspace {
            source_geo.push(entry.clone());
        } else if entry.output == target_key.output && entry.workspace == target_key.workspace {
            target_geo.push(entry.clone());
        } else {
            return false;
        }
    }
    geometry_covers_domain(&source_geo, &pair.source.leaves, &source_key)
        && geometry_covers_domain(&target_geo, &pair.target.leaves, &target_key)
}

pub(super) fn settle_pair_focus_and_prune(pair: &mut PairCandidate, accepted_focus: bool) {
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    let source_focus = if pair.focused_domain.as_ref() == Some(&source_key) {
        pair.focused_leaf.clone()
    } else {
        None
    };
    let target_focus = if pair.focused_domain.as_ref() == Some(&target_key) {
        pair.focused_leaf.clone()
    } else {
        None
    };

    let source_stack = pair.source.focus_stack.clone();
    let source_last = pair.source.last_active.clone();
    if accepted_focus && source_focus.is_some() {
        settle_domain_accepted_focus_tail(
            &mut pair.source,
            &source_stack,
            source_focus,
            source_last.as_ref(),
        );
    } else {
        settle_domain_focus_tail(
            &mut pair.source,
            &source_stack,
            source_focus,
            source_last.as_ref(),
        );
    }

    let target_stack = pair.target.focus_stack.clone();
    let target_last = pair.target.last_active.clone();
    if accepted_focus && target_focus.is_some() {
        settle_domain_accepted_focus_tail(
            &mut pair.target,
            &target_stack,
            target_focus,
            target_last.as_ref(),
        );
    } else {
        settle_domain_focus_tail(
            &mut pair.target,
            &target_stack,
            target_focus,
            target_last.as_ref(),
        );
    }

    WorldLayout::prune_domain_focus_history(&mut pair.source);
    WorldLayout::prune_domain_focus_history(&mut pair.target);

    // Global pair membership: retain fixed-size and capacity provenance across
    // both domains so R4 / capacity-float transfers keep source-side markers
    // and admission order until `install_pair` -> `split_with_outer_gaps`
    // partitions them to the window's new domain.
    let live: BTreeSet<WindowId> = pair
        .source
        .leaves
        .keys()
        .chain(pair.source.exceptions.keys())
        .chain(pair.target.leaves.keys())
        .chain(pair.target.exceptions.keys())
        .cloned()
        .collect();
    let mut live_or_slotted = live.clone();
    live_or_slotted.extend(pair.source.minimized_slots.keys().cloned());
    live_or_slotted.extend(pair.target.minimized_slots.keys().cloned());

    pair.source
        .automatic_fixed
        .retain(|id| live_or_slotted.contains(id));
    pair.target
        .automatic_fixed
        .retain(|id| live_or_slotted.contains(id));
    pair.source
        .fixed_tile_override
        .retain(|id| live_or_slotted.contains(id));
    pair.target
        .fixed_tile_override
        .retain(|id| live_or_slotted.contains(id));
    if !accepted_focus {
        pair.source.capacity_floats.retain(|id| live.contains(id));
        pair.target.capacity_floats.retain(|id| live.contains(id));
        pair.source
            .admission_order
            .retain(|id, _| live_or_slotted.contains(id));
        pair.target
            .admission_order
            .retain(|id, _| live_or_slotted.contains(id));
    }
}
