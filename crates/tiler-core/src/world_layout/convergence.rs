//! Typed synchronous single-domain observation convergence over
//! [`WorldLayout`].
//!
//! One retained domain is converged against a complete per-domain
//! observation. A local [`DomainLayout`] candidate is validated and installed
//! atomically; the per-domain revision advances once iff retained layout,
//! provenance or the local admission counter changed. Focus-only bookkeeping
//! is settled on every success but never advances the revision. No identity,
//! pending/divergence state machine, synthetic ack or cross-domain creation
//! lives here.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::cursor_targeting::FreshPlacement;
use crate::directional::{Node, NodeId, WindowId};
use crate::geometry::Rect;
use crate::session::{
    DomainKey, ExceptionFlags, ObservationConvergence, ObservedWindow, RefusalKind,
};
use crate::size_hints::{WindowSizeHints, is_fixed_size_with};

use super::admission::plan_admission_leg;
use super::capacity::repair_domain_candidate;
use super::focus::{domain_focus_resolves, settle_domain_focus_tail};
use super::observation::valid_observed_shapes;
use super::outcome::LayoutFailure;
use super::restore::best_effort_rejoin;
use super::state::{DomainException, DomainLayout, DomainRestoreSlot, WorldLayout};
use super::topology::{collect_node_ids, validate_domain_topology};
use super::tree_edit::{admission_placement_for, remove_leaf_from_tree};

#[cfg(test)]
#[path = "convergence_tests.rs"]
mod tests;

impl WorldLayout {
    /// Converge one retained domain against a complete observation.
    ///
    /// `observed` is the full carried slice for this domain; admission legs
    /// follow its order, not a sorted map. `hidden` names same-lifetime
    /// minimized windows whose fixed-size markers survive removal.
    pub(crate) fn converge_domain(
        &mut self,
        key: &DomainKey,
        observed: &[ObservedWindow],
        observed_focus: Option<&WindowId>,
        hidden: &BTreeSet<WindowId>,
    ) -> Result<ObservationConvergence, LayoutFailure> {
        let refuse = LayoutFailure::Semantics;

        // Pre-candidate validation: known domain, retained topology, observed
        // id shape and exact single-domain homing, before any candidate work.
        let Some(original) = self.domain(key) else {
            return Err(refuse(RefusalKind::CrossDomainMismatch));
        };
        if !validate_domain_topology(key, original) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if !valid_observed_shapes(observed) {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        let mut observed_by_id: BTreeMap<&WindowId, &ObservedWindow> = BTreeMap::new();
        for entry in observed {
            if entry.output != key.output || entry.workspace != key.workspace {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
            observed_by_id.insert(&entry.window, entry);
        }

        let base_revision = original.accepted_revision;
        let original_last_active = original.last_active.clone();
        let original_focus_leaf = original.focused_leaf.clone();
        let original_stack = original.focus_stack.clone();
        let mut candidate = original.clone();

        let fixed_admission = self.config.fixed_size_admission;
        let fixed_predicate = self.config.fixed_size_predicate;
        let policy = self.policy.as_ref();
        let mut removed = 0usize;
        let mut admitted = 0usize;
        let mut flags_adopted = 0usize;

        // Missing known tiled/exception windows first, in chain iteration
        // order. A removed tiled leaf stores the pre-removal tree and its leaf
        // for restore, regardless of the hidden set.
        for id in original.leaves.keys().chain(original.exceptions.keys()) {
            if observed_by_id.contains_key(id) {
                continue;
            }
            if let Some(leaf) = candidate.leaves.remove(id) {
                let current = candidate.tree.clone();
                let stored = current.clone();
                candidate.tree = remove_leaf_from_tree(policy, current, &leaf);
                if let Some(tree) = stored {
                    candidate
                        .minimized_slots
                        .insert(id.clone(), DomainRestoreSlot { leaf, tree });
                }
                removed += 1;
            } else if candidate.exceptions.remove(id).is_some() {
                removed += 1;
            } else {
                return Err(refuse(RefusalKind::MalformedTopology));
            }
        }

        // Tiled observed floating: remove the leaf and retain a floating
        // exception, keeping geometry already held.
        for (id, entry) in &observed_by_id {
            if !entry.floating || !candidate.leaves.contains_key(*id) {
                continue;
            }
            let leaf = candidate.leaves.remove(*id).expect("checked above");
            let current = candidate.tree.clone();
            candidate.tree = remove_leaf_from_tree(policy, current, &leaf);
            let geometry = candidate.retained_float_geometry.get(*id).copied();
            candidate
                .exceptions
                .insert((*id).clone(), floating_exception(geometry));
            flags_adopted += 1;
        }

        // Known floating observed tiled: automatic/fullscreen holds and
        // sticky/suppress origin rules. Hint changes never reclassify.
        let mut to_tile: Vec<WindowId> = Vec::new();
        for id in original.exceptions.keys() {
            if observed_by_id
                .get(id)
                .is_some_and(|entry| !entry.floating && (!fixed_admission || !entry.fullscreen))
                && candidate.exceptions.contains_key(id)
            {
                to_tile.push(id.clone());
            }
        }
        for id in &to_tile {
            let entry = observed_by_id.get(id).expect("to_tile observed");
            let suppressed = entry.fixed_suppress;
            let overridden = original.fixed_tile_override.contains(id);
            let was_automatic = candidate.automatic_fixed.contains(id);
            let fixed_now = is_fixed_size_with(entry.hints, fixed_predicate);
            if fixed_admission && was_automatic && !suppressed && !overridden {
                flags_adopted += 1;
                continue;
            }
            if fixed_admission
                && !was_automatic
                && !suppressed
                && !overridden
                && !entry.sticky
                && fixed_now
            {
                candidate.automatic_fixed.insert(id.clone());
                flags_adopted += 1;
                continue;
            }
            candidate.exceptions.remove(id);
            candidate.automatic_fixed.remove(id);
            if fixed_admission && suppressed {
                candidate.fixed_tile_override.insert(id.clone());
            }
            flags_adopted += 1;
        }

        // Sticky observations adopt automatics as intentional.
        for (id, entry) in &observed_by_id {
            if entry.sticky && candidate.automatic_fixed.contains(*id) {
                candidate.automatic_fixed.remove(*id);
            }
        }

        // Adapter-asserted origin on a known floating exception adopts the
        // marker unless a tile win competes.
        for (id, entry) in &observed_by_id {
            if !entry.floating || !entry.fixed_auto || entry.fixed_suppress || entry.sticky {
                continue;
            }
            if !candidate.exceptions.contains_key(*id) || candidate.automatic_fixed.contains(*id) {
                continue;
            }
            if original.fixed_tile_override.contains(*id)
                || candidate.fixed_tile_override.contains(*id)
            {
                continue;
            }
            if !fixed_admission {
                continue;
            }
            candidate.automatic_fixed.insert((*id).clone());
            flags_adopted += 1;
        }

        // Brand-new floating windows become exceptions directly.
        for (id, entry) in &observed_by_id {
            if entry.floating
                && !original.leaves.contains_key(*id)
                && !original.exceptions.contains_key(*id)
            {
                candidate
                    .exceptions
                    .insert((*id).clone(), floating_exception(None));
                if fixed_admission
                    && entry.fixed_auto
                    && !entry.fullscreen
                    && !entry.sticky
                    && !entry.fixed_suppress
                    && !original.fixed_tile_override.contains(*id)
                {
                    candidate.automatic_fixed.insert((*id).clone());
                }
                flags_adopted += 1;
            }
        }

        // Brand-new fullscreen holds slotless without a marker; born
        // maximized floats under its overlay with no reserved tile.
        for (id, entry) in &observed_by_id {
            if !fixed_admission || entry.sticky {
                continue;
            }
            if original.leaves.contains_key(*id)
                || original.exceptions.contains_key(*id)
                || candidate.exceptions.contains_key(*id)
            {
                continue;
            }
            if !entry.fullscreen || entry.floating {
                continue;
            }
            candidate
                .exceptions
                .insert((*id).clone(), floating_exception(None));
            flags_adopted += 1;
        }

        // Brand-new fixed windows float with an automatic marker.
        for (id, entry) in &observed_by_id {
            if original.leaves.contains_key(*id)
                || original.exceptions.contains_key(*id)
                || candidate.exceptions.contains_key(*id)
            {
                continue;
            }
            if fixed_admission
                && !entry.floating
                && !entry.fullscreen
                && !entry.sticky
                && !entry.fixed_suppress
                && !candidate.fixed_tile_override.contains(*id)
                && is_fixed_size_with(entry.hints, fixed_predicate)
            {
                candidate
                    .exceptions
                    .insert((*id).clone(), floating_exception(None));
                candidate.automatic_fixed.insert((*id).clone());
                flags_adopted += 1;
            }
        }

        // Slots for windows present again are stale, never restore sources.
        candidate.minimized_slots.retain(|id, _| {
            !candidate.leaves.contains_key(id) && !candidate.exceptions.contains_key(id)
        });

        // Restore attempt: exact stored-tree fast path, else best-effort
        // rejoin. Restored windows bypass fresh classification and cannot take
        // observed focus.
        let mut existing_ids: BTreeSet<NodeId> = BTreeSet::new();
        if let Some(tree) = candidate.tree.as_ref() {
            collect_node_ids(tree, &mut existing_ids);
        }
        let mut restored: BTreeSet<WindowId> = BTreeSet::new();
        let mut seeded_order: Vec<NodeId> = Vec::new();
        for (id, entry) in &observed_by_id {
            if entry.floating
                || entry.fullscreen
                || entry.maximized
                || entry.sticky
                || candidate.leaves.contains_key(*id)
                || candidate.exceptions.contains_key(*id)
            {
                continue;
            }
            let Some(slot) = candidate.minimized_slots.get(*id).cloned() else {
                continue;
            };
            // The slot is consumed either way.
            candidate.minimized_slots.remove(*id);
            if existing_ids.contains(&slot.leaf) {
                continue;
            }
            let current = candidate.tree.clone();
            let expected = remove_leaf_from_tree(policy, Some(slot.tree.clone()), &slot.leaf);
            let rejoined = if current == expected {
                Some(slot.tree.clone())
            } else {
                best_effort_rejoin(
                    &self.group_alloc,
                    &slot,
                    &current,
                    &candidate.leaves,
                    &mut existing_ids,
                )
            };
            let Some(tree) = rejoined else {
                continue;
            };
            candidate.tree = Some(tree);
            candidate.leaves.insert((*id).clone(), slot.leaf.clone());
            existing_ids.insert(slot.leaf.clone());
            restored.insert((*id).clone());
            seeded_order.push(slot.leaf.clone());
            admitted += 1;
        }

        // Fresh normal admissions in carried slice order. Anchor order uses
        // the evolving admission focus for non-Hypr placements and the
        // observed focus leaf for Hypr, then the shared fresh anchor.
        let hints_all: BTreeMap<WindowId, WindowSizeHints> = observed
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect();
        let mut evolving = original_focus_leaf.clone();
        let mut new_counter = self.admission_seq_next;
        let hypr = self.config.fresh_placement == FreshPlacement::CursorOutputCursorClosest;
        for entry in observed {
            let id = &entry.window;
            if entry.floating
                || candidate.leaves.contains_key(id)
                || candidate.exceptions.contains_key(id)
            {
                continue;
            }
            let current = candidate.tree.clone();
            let observed_leaf =
                observed_focus.and_then(|focused| candidate.leaves.get(focused).cloned());
            let evolving_leaf = evolving
                .as_ref()
                .filter(|leaf| domain_focus_resolves(&candidate, leaf))
                .cloned();
            let eligible = if hypr {
                observed_leaf
                    .or_else(|| fresh_admit_anchor(self, key, current.as_ref(), &candidate, hypr))
            } else {
                evolving_leaf
                    .or_else(|| fresh_admit_anchor(self, key, current.as_ref(), &candidate, hypr))
            };
            let placement = admission_placement_for(
                current.as_ref(),
                candidate.bounds,
                candidate.gap,
                eligible.as_ref(),
            );
            let orientation = policy.admission_axis_for_rect(&placement);
            let leaf_links: BTreeMap<NodeId, WindowId> = candidate
                .leaves
                .iter()
                .map(|(window, leaf)| (leaf.clone(), window.clone()))
                .collect();
            let leg = plan_admission_leg(
                policy,
                &self.group_alloc,
                current,
                candidate.bounds,
                candidate.gap,
                eligible,
                orientation,
                id,
                &mut existing_ids,
                &hints_all,
                &leaf_links,
            );
            let leaf_id = leg.leaf_id;
            let ordinary = leg.ordinary_tree;
            // Exhaustion fail-closed: empty domains never mint.
            if ordinary.is_none() && leg.selected.is_err() {
                return Err(refuse(RefusalKind::MalformedTopology));
            }
            let admitted_tree = match leg.selected {
                Ok(target) => Some(target.tree),
                Err(()) => None,
            };
            if let Some(next) = admitted_tree {
                candidate.tree = Some(next);
                candidate.leaves.insert(id.clone(), leaf_id.clone());
                // First-successful-tiled sequence only; same-lifetime first
                // number kept, restores and floats take no number.
                if !candidate.admission_order.contains_key(id) {
                    candidate.admission_order.insert(id.clone(), new_counter);
                    new_counter = new_counter.saturating_add(1);
                }
                evolving = Some(leaf_id.clone());
                seeded_order.push(leaf_id.clone());
                if fixed_admission && entry.fixed_suppress {
                    candidate.automatic_fixed.remove(id);
                    candidate.fixed_tile_override.insert(id.clone());
                }
                candidate.capacity_floats.remove(id);
            } else {
                let geometry = candidate.retained_float_geometry.get(id).copied();
                candidate
                    .exceptions
                    .insert(id.clone(), floating_exception(geometry));
                candidate.capacity_floats.insert(id.clone());
                let fixed_now = is_fixed_size_with(entry.hints, fixed_predicate);
                if !candidate.fixed_tile_override.contains(id) {
                    if entry.fixed_suppress && fixed_now {
                        candidate.automatic_fixed.remove(id);
                        candidate.fixed_tile_override.insert(id.clone());
                    } else {
                        candidate.fixed_tile_override.remove(id);
                    }
                }
            }
            if !to_tile.contains(id) {
                admitted += 1;
            }
        }
        candidate.capacity_floats.retain(|id| {
            candidate.leaves.contains_key(id) || candidate.exceptions.contains_key(id)
        });

        // D7 repair with observation hints, protected fullscreen/maximized/
        // sticky holds, empty live frames and the original stack/focus. The
        // returned focus is ignored; exact focus selection happens later.
        let mut protected: BTreeSet<WindowId> = BTreeSet::new();
        for (id, entry) in &observed_by_id {
            if (entry.fullscreen || entry.maximized || entry.sticky)
                && candidate.leaves.contains_key(*id)
            {
                protected.insert((*id).clone());
            }
        }
        let empty_frames: BTreeMap<WindowId, Rect> = BTreeMap::new();
        if repair_domain_candidate(
            &mut candidate,
            policy,
            key,
            &hints_all,
            &protected,
            &empty_frames,
            &original_stack,
            original_focus_leaf.as_ref(),
        )
        .is_none()
        {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        // Append seeded/restored leaf order to the retained MRU base.
        let mut stack_base = original_stack.clone();
        for leaf in &seeded_order {
            if !stack_base.contains(leaf) {
                stack_base.push(leaf.clone());
            }
        }

        // Focus: observed tiled focus (unless just restored), else retained
        // resolving focus, else original-focused-domain MRU fallback. A
        // standing focus leaf always names this single domain, so only the
        // leaf is tracked.
        let mut next_focus_leaf: Option<NodeId> = None;
        let observed_is_restored = observed_focus.is_some_and(|focused| restored.contains(focused));
        if !observed_is_restored
            && let Some(focused) = observed_focus
            && let Some(leaf) = candidate.leaves.get(focused)
        {
            next_focus_leaf = Some(leaf.clone());
        } else if original_focus_leaf
            .as_ref()
            .is_some_and(|leaf| domain_focus_resolves(&candidate, leaf))
        {
            next_focus_leaf = original_focus_leaf.clone();
        } else if original_focus_leaf.is_some()
            && let Some(leaf) = candidate_focus_stack_fallback(&candidate, &stack_base)
        {
            next_focus_leaf = Some(leaf);
        }

        // Validate the candidate with the selected focus installed.
        candidate.focused_leaf = next_focus_leaf.clone();
        if !validate_domain_topology(key, &candidate) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        // Retention: automatic/override markers survive an explicit hidden
        // gap; capacity and order follow leaves/exceptions (and slots).
        candidate.automatic_fixed.retain(|id| {
            candidate.leaves.contains_key(id)
                || candidate.exceptions.contains_key(id)
                || hidden.contains(id)
        });
        candidate.fixed_tile_override.retain(|id| {
            candidate.leaves.contains_key(id)
                || candidate.exceptions.contains_key(id)
                || hidden.contains(id)
        });
        candidate.capacity_floats.retain(|id| {
            candidate.leaves.contains_key(id) || candidate.exceptions.contains_key(id)
        });
        candidate.admission_order.retain(|id, _| {
            candidate.leaves.contains_key(id)
                || candidate.exceptions.contains_key(id)
                || candidate.minimized_slots.contains_key(id)
        });

        // Changed predicate excludes focus/MRU/history/last-active/revision.
        let changed = candidate.tree != original.tree
            || candidate.leaves != original.leaves
            || candidate.exceptions != original.exceptions
            || candidate.retained_float_geometry != original.retained_float_geometry
            || candidate.automatic_fixed != original.automatic_fixed
            || candidate.fixed_tile_override != original.fixed_tile_override
            || candidate.minimized_slots != original.minimized_slots
            || candidate.capacity_floats != original.capacity_floats
            || candidate.admission_order != original.admission_order
            || new_counter != self.admission_seq_next;
        if changed && base_revision >= MAX_REVISION {
            // Refuse without install or high-water mutation; allocator burns
            // from admitted candidates survive. No terminal divergence.
            return Err(LayoutFailure::RevisionLimit {
                base: base_revision,
                limit: MAX_REVISION,
            });
        }
        settle_domain_focus_tail(
            &mut candidate,
            &stack_base,
            next_focus_leaf,
            original_last_active.as_ref(),
        );
        if changed {
            candidate.accepted_revision = base_revision + 1;
        }
        *self.domains.get_mut(key).expect("domain validated above") = candidate;
        if changed {
            self.admission_seq_next = new_counter;
        }
        if changed {
            Ok(ObservationConvergence {
                removed,
                admitted,
                flags_adopted,
            })
        } else {
            Ok(ObservationConvergence {
                removed: 0,
                admitted: 0,
                flags_adopted: 0,
            })
        }
    }
}

/// Floating exception with every overlay flag clear; geometry kept only where
/// already held, never fabricated.
fn floating_exception(geometry: Option<Rect>) -> DomainException {
    DomainException {
        flags: ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
        floating_geometry: geometry,
    }
}

/// Newest resolving leaf from a candidate's MRU base.
fn candidate_focus_stack_fallback(candidate: &DomainLayout, base: &[NodeId]) -> Option<NodeId> {
    base.iter()
        .rev()
        .find_map(|leaf| domain_focus_resolves(candidate, leaf).then(|| leaf.clone()))
}

/// Cursor-first fresh-admission anchor fallback. The remembered leaf reads the
/// ORIGINAL retained state through `layout`, while the MRU fallback reads the
/// candidate; their distinction is deliberate.
fn fresh_admit_anchor(
    layout: &WorldLayout,
    key: &DomainKey,
    current: Option<&Node>,
    candidate: &DomainLayout,
    hypr: bool,
) -> Option<NodeId> {
    let projected: Vec<(NodeId, Rect)> = current
        .and_then(|tree| crate::geometry::project(tree, candidate.bounds, candidate.gap).ok())
        .map(|leaves| {
            leaves
                .into_iter()
                .map(|entry| (entry.leaf, entry.rect))
                .collect()
        })
        .unwrap_or_default();
    if hypr
        && let Some(cursor) = layout.config.fresh_cursor
        && let Some(closest) = crate::cursor_targeting::cursor_closest_leaf(&projected, cursor)
    {
        return Some(closest);
    }
    if let Some(leaf) = layout
        .remembered_leaf(key)
        .or_else(|| candidate_focus_stack_fallback(candidate, &candidate.focus_stack))
    {
        return Some(leaf);
    }
    if hypr {
        return projected.into_iter().next().map(|(leaf, _)| leaf);
    }
    None
}
