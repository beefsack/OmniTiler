//! Combined canonical two-domain observation convergence on [`PairCandidate`].
//!
//! Converges both domains of an assembled pair in one pass over the combined
//! observation: global sorted missing/classify/restore phases, carried-order
//! fresh admission with a global evolving anchor and shared tree-node/sequence
//! counters, source-then-target D7 repair, global focus selection, and global
//! retention/revision accounting.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::cursor_targeting::FreshPlacement;
use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::session::{DomainKey, ObservationConvergence, ObservedWindow, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::capacity::repair_domain_candidate;
use super::convergence_phases::{
    ClassificationDomains, admit_observed_window_leg, append_seeded_order,
    candidate_focus_stack_fallback, classify_observed_windows, collect_protected_tiled,
    demote_tiled_to_floating, fresh_admit_anchor, remove_missing_known_window,
    restore_slot_into_domain, retain_convergence_provenance,
};
use super::focus::{domain_focus_resolves, settle_domain_focus_tail};
use super::observation::valid_observed_shapes;
use super::outcome::LayoutFailure;
use super::pair::{
    PairCandidate, repartition_admission_order, repartition_retained_geometry,
    repartition_window_set,
};
use super::state::WorldLayout;
use super::topology::collect_node_ids;

impl WorldLayout {
    /// Converge an assembled canonical [`PairCandidate`] against a combined
    /// two-domain observation without mutating `self` or `pair` on failure.
    ///
    /// Preserves canonical target IDs on the same working view so callers can
    /// follow with close cleanup, nonconsuming [`Self::install_pair`], and
    /// two-domain planning. Allocator burns from discarded candidates survive.
    pub(super) fn converge_pair(
        &self,
        pair: &mut PairCandidate,
        observed: &[ObservedWindow],
        observed_focus: Option<&WindowId>,
        hidden: &BTreeSet<WindowId>,
    ) -> Result<ObservationConvergence, LayoutFailure> {
        let refuse = LayoutFailure::Semantics;
        let source_key = pair.source_key();
        let target_key = pair.target_key();

        // Pre-candidate gates: pair topology -> observed shape/duplicates ->
        // scope + retained homing (known source cannot transplant to target).
        if !pair.validate_pair_topology() {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if !valid_observed_shapes(observed) {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        let mut observed_by_id: BTreeMap<&WindowId, &ObservedWindow> = BTreeMap::new();
        for entry in observed {
            let entry_key = DomainKey {
                output: entry.output.clone(),
                workspace: entry.workspace.clone(),
            };
            if entry_key != source_key && entry_key != target_key {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
            if (pair.source.leaves.contains_key(&entry.window)
                || pair.source.exceptions.contains_key(&entry.window))
                && entry_key != source_key
            {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
            if (pair.target.leaves.contains_key(&entry.window)
                || pair.target.exceptions.contains_key(&entry.window))
                && entry_key != target_key
            {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
            observed_by_id.insert(&entry.window, entry);
        }

        let base_revision = pair
            .pair_revision
            .max(pair.source.accepted_revision)
            .max(pair.target.accepted_revision);
        let orig_counter = pair.admission_seq_next;
        let original_focus_domain = pair.focused_domain.clone();
        let original_focus_leaf = pair.focused_leaf.clone();
        let original_source_stack = pair.source.focus_stack.clone();
        let original_target_stack = pair.target.focus_stack.clone();
        let original_source_last_active = pair.source.last_active.clone();
        let original_target_last_active = pair.target.last_active.clone();
        let source_remembered = original_source_last_active
            .as_ref()
            .filter(|leaf| domain_focus_resolves(&pair.source, leaf))
            .cloned();
        let target_remembered = original_target_last_active
            .as_ref()
            .filter(|leaf| domain_focus_resolves(&pair.target, leaf))
            .cloned();

        // Global provenance unions with source priority.
        let mut orig_retained = pair.source.retained_float_geometry.clone();
        for (id, rect) in &pair.target.retained_float_geometry {
            orig_retained.entry(id.clone()).or_insert(*rect);
        }
        let mut orig_automatic = pair.source.automatic_fixed.clone();
        orig_automatic.extend(pair.target.automatic_fixed.iter().cloned());
        let mut orig_override = pair.source.fixed_tile_override.clone();
        orig_override.extend(pair.target.fixed_tile_override.iter().cloned());
        let mut orig_capacity = pair.source.capacity_floats.clone();
        orig_capacity.extend(pair.target.capacity_floats.iter().cloned());
        let mut orig_order = pair.source.admission_order.clone();
        for (id, seq) in &pair.target.admission_order {
            orig_order.entry(id.clone()).or_insert(*seq);
        }

        let mut candidate = pair.clone();
        let mut global_retained = orig_retained.clone();
        let mut global_automatic = orig_automatic.clone();
        let mut global_override = orig_override.clone();
        let mut global_capacity = orig_capacity.clone();
        let mut global_order = orig_order.clone();
        let mut new_counter = orig_counter;

        let policy = self.policy.as_ref();
        let mut removed = 0usize;
        let mut admitted = 0usize;
        let mut flags_adopted = 0usize;

        // Missing known windows in global sorted order: tiled first, then
        // exceptions.
        let known_tiled: BTreeSet<WindowId> = pair
            .source
            .leaves
            .keys()
            .chain(pair.target.leaves.keys())
            .cloned()
            .collect();
        let known_exceptions: BTreeSet<WindowId> = pair
            .source
            .exceptions
            .keys()
            .chain(pair.target.exceptions.keys())
            .cloned()
            .collect();
        for id in known_tiled.iter().chain(known_exceptions.iter()) {
            if observed_by_id.contains_key(id) {
                continue;
            }
            if pair.source.leaves.contains_key(id) || pair.source.exceptions.contains_key(id) {
                remove_missing_known_window(&mut candidate.source, id, policy)?;
            } else {
                remove_missing_known_window(&mut candidate.target, id, policy)?;
            }
            removed += 1;
        }

        // Tiled observed floating in global sorted order.
        for (id, entry) in &observed_by_id {
            if !entry.floating {
                continue;
            }
            if candidate.source.leaves.contains_key(*id) {
                demote_tiled_to_floating(&mut candidate.source, id, Some(&global_retained), policy);
                flags_adopted += 1;
            } else if candidate.target.leaves.contains_key(*id) {
                demote_tiled_to_floating(&mut candidate.target, id, Some(&global_retained), policy);
                flags_adopted += 1;
            }
        }

        // Known floating observed tiled, sticky adoption, fixed_auto origin,
        // and brand-new floating/fullscreen/fixed classification.
        let originally_known: BTreeSet<WindowId> = known_tiled
            .iter()
            .chain(known_exceptions.iter())
            .cloned()
            .collect();
        let (to_tile, classify_adopted) = classify_observed_windows(
            &mut ClassificationDomains::Pair {
                source_key: &source_key,
                source: &mut candidate.source,
                target_key: &target_key,
                target: &mut candidate.target,
            },
            &observed_by_id,
            &originally_known,
            &known_exceptions,
            &orig_override,
            &mut global_automatic,
            &mut global_override,
            self.config.fixed_size_admission,
            self.config.fixed_size_predicate,
        );
        flags_adopted += classify_adopted;

        // Slots for windows present anywhere in the pair are stale.
        let present_ids: BTreeSet<WindowId> = candidate
            .source
            .leaves
            .keys()
            .chain(candidate.source.exceptions.keys())
            .chain(candidate.target.leaves.keys())
            .chain(candidate.target.exceptions.keys())
            .cloned()
            .collect();
        candidate
            .source
            .minimized_slots
            .retain(|id, _| !present_ids.contains(id));
        candidate
            .target
            .minimized_slots
            .retain(|id, _| !present_ids.contains(id));

        let is_present_in = |source: &super::state::DomainLayout,
                             target: &super::state::DomainLayout,
                             id: &WindowId| {
            source.leaves.contains_key(id)
                || source.exceptions.contains_key(id)
                || target.leaves.contains_key(id)
                || target.exceptions.contains_key(id)
        };

        // Shared existing tree node IDs across both domains, then global sorted
        // restore attempt.
        let mut existing_ids: BTreeSet<NodeId> = BTreeSet::new();
        if let Some(tree) = candidate.source.tree.as_ref() {
            collect_node_ids(tree, &mut existing_ids);
        }
        if let Some(tree) = candidate.target.tree.as_ref() {
            collect_node_ids(tree, &mut existing_ids);
        }
        let mut restored: BTreeSet<WindowId> = BTreeSet::new();
        let mut seeded_source: Vec<NodeId> = Vec::new();
        let mut seeded_target: Vec<NodeId> = Vec::new();
        for (id, entry) in &observed_by_id {
            if entry.floating
                || entry.fullscreen
                || entry.maximized
                || entry.sticky
                || is_present_in(&candidate.source, &candidate.target, id)
            {
                continue;
            }
            let (slot, slot_is_source) =
                if let Some(slot) = candidate.source.minimized_slots.remove(*id) {
                    (slot, true)
                } else if let Some(slot) = candidate.target.minimized_slots.remove(*id) {
                    (slot, false)
                } else {
                    continue;
                };
            let entry_is_source =
                entry.output == source_key.output && entry.workspace == source_key.workspace;
            if entry_is_source != slot_is_source {
                continue;
            }
            let domain = if slot_is_source {
                &mut candidate.source
            } else {
                &mut candidate.target
            };
            if restore_slot_into_domain(
                domain,
                id,
                &slot,
                policy,
                &self.group_alloc,
                &mut existing_ids,
            ) {
                restored.insert((*id).clone());
                if slot_is_source {
                    seeded_source.push(slot.leaf);
                } else {
                    seeded_target.push(slot.leaf);
                }
                admitted += 1;
            }
        }

        // Carried-order fresh admissions with global evolving anchor
        // `(adomain, aleaf)` and shared node IDs / counter.
        let hints_all: BTreeMap<WindowId, WindowSizeHints> = observed
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect();
        let mut adomain = original_focus_domain.clone();
        let mut aleaf = original_focus_leaf.clone();
        let hypr = self.config.fresh_placement == FreshPlacement::CursorOutputCursorClosest;
        for entry in observed {
            let id = &entry.window;
            if entry.floating || is_present_in(&candidate.source, &candidate.target, id) {
                continue;
            }
            let is_source =
                entry.output == source_key.output && entry.workspace == source_key.workspace;
            let entry_key = if is_source { &source_key } else { &target_key };
            let original_remembered = if is_source {
                source_remembered.clone()
            } else {
                target_remembered.clone()
            };
            let domain = if is_source {
                &mut candidate.source
            } else {
                &mut candidate.target
            };
            let observed_leaf =
                observed_focus.and_then(|focused| domain.leaves.get(focused).cloned());
            let evolving = match (&adomain, &aleaf) {
                (Some(d), Some(l)) if d == entry_key => {
                    domain_focus_resolves(domain, l).then(|| l.clone())
                }
                _ => None,
            };
            let eligible = if hypr {
                observed_leaf.or_else(|| {
                    fresh_admit_anchor(
                        self.config,
                        domain.tree.as_ref(),
                        original_remembered,
                        domain,
                        hypr,
                    )
                })
            } else {
                evolving.or_else(|| {
                    fresh_admit_anchor(
                        self.config,
                        domain.tree.as_ref(),
                        original_remembered,
                        domain,
                        hypr,
                    )
                })
            };
            if let Some(leaf_id) = admit_observed_window_leg(
                domain,
                entry,
                eligible,
                policy,
                &self.group_alloc,
                self.config,
                &mut existing_ids,
                &hints_all,
                Some(&global_retained),
                &mut global_automatic,
                &mut global_override,
                &mut global_capacity,
                &mut global_order,
                &mut new_counter,
            )? {
                adomain = Some(entry_key.clone());
                aleaf = Some(leaf_id.clone());
                if is_source {
                    seeded_source.push(leaf_id);
                } else {
                    seeded_target.push(leaf_id);
                }
            }
            if !to_tile.contains(id) {
                admitted += 1;
            }
        }
        global_capacity.retain(|id| is_present_in(&candidate.source, &candidate.target, id));

        // Sync global overrides, admission order, retained geometry, and
        // capacity floats onto each domain for D7 repair, then repair source
        // then target.
        sync_repair_inputs(
            &mut candidate.source,
            &global_override,
            &global_order,
            &global_retained,
            &global_capacity,
        );
        sync_repair_inputs(
            &mut candidate.target,
            &global_override,
            &global_order,
            &global_retained,
            &global_capacity,
        );
        let protected_source = collect_protected_tiled(&observed_by_id, &candidate.source);
        let protected_target = collect_protected_tiled(&observed_by_id, &candidate.target);
        let empty_frames: BTreeMap<WindowId, Rect> = BTreeMap::new();
        let source_repair_focus = (original_focus_domain.as_ref() == Some(&source_key))
            .then_some(original_focus_leaf.as_ref())
            .flatten();
        let target_repair_focus = (original_focus_domain.as_ref() == Some(&target_key))
            .then_some(original_focus_leaf.as_ref())
            .flatten();
        if repair_domain_candidate(
            &mut candidate.source,
            policy,
            &source_key,
            &hints_all,
            &protected_source,
            &empty_frames,
            &original_source_stack,
            source_repair_focus,
        )
        .is_none()
        {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if repair_domain_candidate(
            &mut candidate.target,
            policy,
            &target_key,
            &hints_all,
            &protected_target,
            &empty_frames,
            &original_target_stack,
            target_repair_focus,
        )
        .is_none()
        {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        for (id, rect) in candidate
            .source
            .retained_float_geometry
            .iter()
            .chain(candidate.target.retained_float_geometry.iter())
        {
            global_retained.insert(id.clone(), *rect);
        }
        global_capacity.extend(candidate.source.capacity_floats.iter().cloned());
        global_capacity.extend(candidate.target.capacity_floats.iter().cloned());

        // Seeded MRU bases and global focus selection.
        let mut source_stack_base = original_source_stack;
        append_seeded_order(&mut source_stack_base, &seeded_source);
        let mut target_stack_base = original_target_stack;
        append_seeded_order(&mut target_stack_base, &seeded_target);

        let (mut next_focus_domain, mut next_focus_leaf): (Option<DomainKey>, Option<NodeId>) =
            (None, None);
        let observed_is_restored = observed_focus.is_some_and(|focused| restored.contains(focused));
        if !observed_is_restored
            && let Some(focused) = observed_focus
            && let Some((domain_key, leaf)) = candidate
                .source
                .leaves
                .get(focused)
                .map(|leaf| (source_key.clone(), leaf.clone()))
                .or_else(|| {
                    candidate
                        .target
                        .leaves
                        .get(focused)
                        .map(|leaf| (target_key.clone(), leaf.clone()))
                })
        {
            next_focus_domain = Some(domain_key);
            next_focus_leaf = Some(leaf);
        } else if let Some(domain_key) = original_focus_domain.as_ref()
            && let Some(leaf) = original_focus_leaf.as_ref()
            && ((domain_key == &source_key && domain_focus_resolves(&candidate.source, leaf))
                || (domain_key == &target_key && domain_focus_resolves(&candidate.target, leaf)))
        {
            next_focus_domain = Some(domain_key.clone());
            next_focus_leaf = Some(leaf.clone());
        } else if let Some(domain_key) = original_focus_domain.as_ref() {
            let fallback = if domain_key == &source_key {
                candidate_focus_stack_fallback(&candidate.source, &source_stack_base)
            } else if domain_key == &target_key {
                candidate_focus_stack_fallback(&candidate.target, &target_stack_base)
            } else {
                None
            };
            if let Some(leaf) = fallback {
                next_focus_domain = Some(domain_key.clone());
                next_focus_leaf = Some(leaf);
            }
        }

        candidate.focused_domain = next_focus_domain.clone();
        candidate.focused_leaf = next_focus_leaf.clone();
        if !candidate.validate_pair_topology() {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        // Global retention and changed predicate (never comparing per-domain
        // ownership repartition as a global change).
        retain_convergence_provenance(
            &mut global_automatic,
            &mut global_override,
            &mut global_capacity,
            &mut global_order,
            hidden,
            |id| is_present_in(&candidate.source, &candidate.target, id),
            |id| {
                candidate.source.minimized_slots.contains_key(id)
                    || candidate.target.minimized_slots.contains_key(id)
            },
        );
        let changed = candidate.source.tree != pair.source.tree
            || candidate.target.tree != pair.target.tree
            || candidate.source.leaves != pair.source.leaves
            || candidate.target.leaves != pair.target.leaves
            || candidate.source.exceptions != pair.source.exceptions
            || candidate.target.exceptions != pair.target.exceptions
            || candidate.source.minimized_slots != pair.source.minimized_slots
            || candidate.target.minimized_slots != pair.target.minimized_slots
            || global_retained != orig_retained
            || global_automatic != orig_automatic
            || global_override != orig_override
            || global_capacity != orig_capacity
            || global_order != orig_order
            || new_counter != orig_counter;
        if changed && base_revision >= MAX_REVISION {
            return Err(LayoutFailure::RevisionLimit {
                base: base_revision,
                limit: MAX_REVISION,
            });
        }

        // Repartition global provenance back onto source/target with source
        // priority for orphans.
        clear_provenance(&mut candidate.source);
        clear_provenance(&mut candidate.target);
        repartition_retained_geometry(
            &global_retained,
            &BTreeMap::new(),
            &mut candidate.source,
            &mut candidate.target,
        );
        let source_leaves = candidate.source.leaves.clone();
        let source_exceptions = candidate.source.exceptions.clone();
        let target_leaves = candidate.target.leaves.clone();
        let target_exceptions = candidate.target.exceptions.clone();
        repartition_window_set(
            &global_automatic,
            &BTreeSet::new(),
            &source_leaves,
            &source_exceptions,
            &target_leaves,
            &target_exceptions,
            &mut candidate.source.automatic_fixed,
            &mut candidate.target.automatic_fixed,
        );
        repartition_window_set(
            &global_override,
            &BTreeSet::new(),
            &source_leaves,
            &source_exceptions,
            &target_leaves,
            &target_exceptions,
            &mut candidate.source.fixed_tile_override,
            &mut candidate.target.fixed_tile_override,
        );
        repartition_window_set(
            &global_capacity,
            &BTreeSet::new(),
            &source_leaves,
            &source_exceptions,
            &target_leaves,
            &target_exceptions,
            &mut candidate.source.capacity_floats,
            &mut candidate.target.capacity_floats,
        );
        repartition_admission_order(
            &global_order,
            &BTreeMap::new(),
            &mut candidate.source,
            &mut candidate.target,
        );

        let source_focus_leaf = (next_focus_domain.as_ref() == Some(&source_key))
            .then(|| next_focus_leaf.clone())
            .flatten();
        let target_focus_leaf = (next_focus_domain.as_ref() == Some(&target_key))
            .then(|| next_focus_leaf.clone())
            .flatten();
        settle_domain_focus_tail(
            &mut candidate.source,
            &source_stack_base,
            source_focus_leaf,
            original_source_last_active.as_ref(),
        );
        settle_domain_focus_tail(
            &mut candidate.target,
            &target_stack_base,
            target_focus_leaf,
            original_target_last_active.as_ref(),
        );

        if changed {
            let next_revision = base_revision + 1;
            candidate.pair_revision = next_revision;
            candidate.source.accepted_revision = next_revision;
            candidate.target.accepted_revision = next_revision;
            candidate.admission_seq_next = new_counter;
        }
        *pair = candidate;

        let counts = if changed {
            ObservationConvergence {
                removed,
                admitted,
                flags_adopted,
            }
        } else {
            ObservationConvergence {
                removed: 0,
                admitted: 0,
                flags_adopted: 0,
            }
        };
        Ok(counts)
    }
}

fn sync_repair_inputs(
    domain: &mut super::state::DomainLayout,
    global_override: &BTreeSet<WindowId>,
    global_order: &BTreeMap<WindowId, u64>,
    global_retained: &BTreeMap<WindowId, Rect>,
    global_capacity: &BTreeSet<WindowId>,
) {
    domain.fixed_tile_override = global_override
        .iter()
        .filter(|id| domain.leaves.contains_key(*id))
        .cloned()
        .collect();
    domain.admission_order = global_order
        .iter()
        .filter(|(id, _)| domain.leaves.contains_key(*id))
        .map(|(id, seq)| (id.clone(), *seq))
        .collect();
    domain.retained_float_geometry = global_retained
        .iter()
        .filter(|(id, _)| domain.leaves.contains_key(*id))
        .map(|(id, rect)| (id.clone(), *rect))
        .collect();
    domain.capacity_floats = global_capacity
        .iter()
        .filter(|id| domain.leaves.contains_key(*id) || domain.exceptions.contains_key(*id))
        .cloned()
        .collect();
}

fn clear_provenance(domain: &mut super::state::DomainLayout) {
    domain.retained_float_geometry.clear();
    domain.automatic_fixed.clear();
    domain.fixed_tile_override.clear();
    domain.capacity_floats.clear();
    domain.admission_order.clear();
}
