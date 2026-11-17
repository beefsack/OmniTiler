//! World-owned fresh-domain convergence and legacy tiled seed construction.
//!
//! Both entries build a local [`DomainLayout`] candidate with the shared
//! policy, allocator and admission high-water. Fresh convergence installs only
//! after success; the tiled seed returns its candidate. No session, pending
//! plan, identity, fingerprint
//! or synthetic observation is constructed here. Group serials burn on trial
//! through the shared allocator; a discarded candidate never touches retained
//! layout or the high-water.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::directional::{NodeId, WindowId};
use crate::seed::EngineWindow;
use crate::session::{
    ExceptionFlags, ObservationConvergence, ObservedWindow, OutputDomain, RefusalKind,
};
use crate::size_hints::WindowSizeHints;

use super::admission::plan_admission_leg;
use super::focus::{domain_focus_resolves, settle_domain_focus_tail};
use super::outcome::LayoutFailure;
use super::projection::project_borrowed_geometry;
use super::state::{DomainException, DomainLayout, WorldLayout};
use super::topology::{collect_node_ids, validate_domain_topology};
use super::tree_edit::admission_placement_for;

#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;

impl WorldLayout {
    /// Converge a brand-new domain against one complete observation.
    ///
    /// The descriptor is the already-inset canonical component geometry; its
    /// bounds and gap are used verbatim. The domain must not already be
    /// retained. A fully empty candidate retires instead of consuming a slot,
    /// while exception-only and minimize-slot-only domains are retained.
    pub(crate) fn converge_fresh_domain(
        &mut self,
        domain: &OutputDomain,
        outer_gap: i32,
        observed: &[ObservedWindow],
        observed_focus: Option<&WindowId>,
    ) -> Result<ObservationConvergence, LayoutFailure> {
        let refuse = LayoutFailure::Semantics;
        let key = domain.key();
        if self.domains.contains_key(&key) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if !domain.validate() || outer_gap < 0 {
            return Err(refuse(RefusalKind::MalformedInput));
        }

        let original = DomainLayout::new(domain.bounds, domain.gap, outer_gap);
        let (candidate, counter, counts) =
            self.converge_candidate(&key, &original, observed, observed_focus, &BTreeSet::new())?;
        if candidate.leaves.is_empty()
            && candidate.exceptions.is_empty()
            && candidate.minimized_slots.is_empty()
        {
            return Ok(counts);
        }
        self.domains.insert(key, candidate);
        self.admission_seq_next = counter;
        Ok(counts)
    }

    /// Build the legacy tiled seed candidate for one fresh domain without
    /// installing any retained state.
    ///
    /// Every entry is an ordinary tile with no hints, no overlay flags and no
    /// tile suppression, exactly as the old sequential tiled seed synthesized
    /// its admissions. Caller-slice order decides leg order; the descriptor is
    /// used verbatim. On any refusal the local candidate and counter are
    /// discarded while allocator burns survive.
    pub(super) fn build_tiled_seed(
        &self,
        domain: &OutputDomain,
        outer_gap: i32,
        seed_order: &[EngineWindow],
    ) -> Result<(DomainLayout, u64), LayoutFailure> {
        let refuse = LayoutFailure::Semantics;
        if !domain.validate() || outer_gap < 0 {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        let key = domain.key();
        let policy = self.policy.as_ref();
        let hints: BTreeMap<WindowId, WindowSizeHints> = BTreeMap::new();
        let mut candidate = DomainLayout::new(domain.bounds, domain.gap, outer_gap);
        let mut counter = self.admission_seq_next;
        let mut seen: BTreeSet<&WindowId> = BTreeSet::new();

        for entry in seed_order {
            // Entry ids, exact homing and duplicates gate every leg before
            // any allocator work. Flags and hints are deliberately ignored.
            if entry.window.0.is_empty()
                || entry.output.0.is_empty()
                || entry.workspace.0.is_empty()
            {
                return Err(refuse(RefusalKind::MalformedInput));
            }
            if entry.output != domain.id || entry.workspace != domain.workspace {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
            if !seen.insert(&entry.window)
                || candidate.leaves.contains_key(&entry.window)
                || candidate.exceptions.contains_key(&entry.window)
            {
                return Err(refuse(RefusalKind::DuplicateWindow));
            }

            let base = candidate.accepted_revision;
            let pre_stack = candidate.focus_stack.clone();
            let pre_last_active = candidate.last_active.clone();
            // Eligible focus is the retained resolving leaf of this domain.
            let eligible = candidate
                .focused_leaf
                .as_ref()
                .filter(|leaf| domain_focus_resolves(&candidate, leaf))
                .cloned();
            let placement = admission_placement_for(
                candidate.tree.as_ref(),
                candidate.bounds,
                candidate.gap,
                eligible.as_ref(),
            );
            let orientation = policy.admission_axis_for_rect(&placement);

            // Fresh actual-tree id set per leg: unused ids from a prior
            // failed or capacity leg are never reserved for later legs.
            let mut node_ids: BTreeSet<NodeId> = BTreeSet::new();
            if let Some(tree) = candidate.tree.as_ref() {
                collect_node_ids(tree, &mut node_ids);
            }
            let leaf_links: BTreeMap<NodeId, WindowId> = candidate
                .leaves
                .iter()
                .map(|(window, leaf)| (leaf.clone(), window.clone()))
                .collect();
            let leg = plan_admission_leg(
                policy,
                &self.group_alloc,
                candidate.tree.clone(),
                candidate.bounds,
                candidate.gap,
                eligible,
                orientation,
                &entry.window,
                &mut node_ids,
                &hints,
                &leaf_links,
            );
            if leg.ordinary_tree.is_none() && leg.selected.is_err() {
                return Err(refuse(RefusalKind::MalformedTopology));
            }

            let admitted_leaf: Option<NodeId> = match leg.selected {
                Ok(target) => {
                    candidate.tree = Some(target.tree);
                    candidate
                        .leaves
                        .insert(entry.window.clone(), leg.leaf_id.clone());
                    if !validate_domain_topology(&key, &candidate) {
                        return Err(refuse(RefusalKind::MalformedTopology));
                    }
                    let projected = project_borrowed_geometry(
                        &key,
                        candidate.tree.as_ref().expect("tree set above"),
                        &candidate.leaves,
                        candidate.bounds,
                        candidate.gap,
                        &hints,
                    )
                    .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
                    if projected.is_empty() {
                        return Err(refuse(RefusalKind::MalformedTopology));
                    }
                    if !candidate.admission_order.contains_key(&entry.window) {
                        candidate
                            .admission_order
                            .insert(entry.window.clone(), counter);
                        counter = counter.saturating_add(1);
                    }
                    candidate.capacity_floats.remove(&entry.window);
                    Some(leg.leaf_id)
                }
                Err(()) => {
                    // No-space capacity float: slotless floating membership,
                    // no geometry, no sequence and no fixed-size override.
                    candidate.exceptions.insert(
                        entry.window.clone(),
                        DomainException {
                            flags: ExceptionFlags {
                                floating: true,
                                fullscreen: false,
                                maximized: false,
                                sticky: false,
                            },
                            floating_geometry: None,
                        },
                    );
                    candidate.capacity_floats.insert(entry.window.clone());
                    None
                }
            };

            if base >= MAX_REVISION {
                return Err(LayoutFailure::RevisionLimit {
                    base,
                    limit: MAX_REVISION,
                });
            }

            // Exact verify_lifecycle tail: settle the admitted tile as focus
            // (a capacity leg keeps the existing tiled focus), prune fixed and
            // capacity provenance, then advance exactly one revision per leg.
            let settled_focus = admitted_leaf.or_else(|| candidate.focused_leaf.clone());
            settle_domain_focus_tail(
                &mut candidate,
                &pre_stack,
                settled_focus,
                pre_last_active.as_ref(),
            );
            WorldLayout::prune_domain_fixed_size_state(&mut candidate);
            WorldLayout::prune_domain_capacity_state(&mut candidate);
            candidate.accepted_revision = base + 1;
        }

        Ok((candidate, counter))
    }
}
