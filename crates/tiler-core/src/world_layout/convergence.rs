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
use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::seed::{EngineWindow, observed_window_from_engine};
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
#[cfg(test)]
use super::state::{DomainException, DomainRestoreSlot};
use super::state::{DomainLayout, WorldLayout};
use super::topology::{collect_node_ids, validate_domain_topology};

#[cfg(test)]
#[path = "convergence_tests.rs"]
mod tests;

impl WorldLayout {
    /// Converge one retained domain from carried [`EngineWindow`] entries.
    ///
    /// Computes the hidden set from retained leaves, exceptions and minimize
    /// slots absent from both `windows` and `closed_windows`, maps empty
    /// `focused_window` to `None`, runs [`Self::converge_domain`] (which
    /// updates the shared admission high-water mark on success), and then
    /// forgets explicitly closed minimize restore slots revision-neutrally.
    /// Returns `Ok(None)` when `key` is absent without mutating state.
    pub(crate) fn converge_retained_observation(
        &mut self,
        key: &DomainKey,
        windows: &[EngineWindow],
        focused_window: &WindowId,
        closed_windows: &[WindowId],
    ) -> Result<Option<ObservationConvergence>, LayoutFailure> {
        let Some(domain) = self.domain(key) else {
            return Ok(None);
        };
        let observed_ids: BTreeSet<&WindowId> = windows.iter().map(|w| &w.window).collect();
        let closed_ids: BTreeSet<&WindowId> = closed_windows.iter().collect();
        let mut hidden = BTreeSet::new();
        for id in domain
            .leaves
            .keys()
            .chain(domain.exceptions.keys())
            .chain(domain.minimized_slots.keys())
        {
            if !observed_ids.contains(id) && !closed_ids.contains(id) {
                hidden.insert(id.clone());
            }
        }
        let observed: Vec<ObservedWindow> =
            windows.iter().map(observed_window_from_engine).collect();
        let focus = if focused_window.0.is_empty() {
            None
        } else {
            Some(focused_window)
        };
        let counts = self.converge_domain(key, &observed, focus, &hidden)?;
        if let Some(domain) = self.domains.get_mut(key) {
            for window in closed_windows {
                domain.minimized_slots.remove(window);
            }
        }
        Ok(Some(counts))
    }

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
        let Some(original) = self.domain(key) else {
            return Err(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch));
        };
        let (candidate, counter, counts) =
            self.converge_candidate(key, original, observed, observed_focus, hidden)?;
        *self.domains.get_mut(key).expect("domain validated above") = candidate;
        self.admission_seq_next = counter;
        Ok(counts)
    }

    /// Build one converged domain candidate from a borrowed original without
    /// any retained write. The caller installs the returned candidate and
    /// counter only after success. A fresh domain passes an empty original
    /// with no last-active leaf; the remembered-leaf anchor still reads only
    /// retained state, preserving the deliberate candidate-MRU distinction.
    pub(super) fn converge_candidate(
        &self,
        key: &DomainKey,
        original: &DomainLayout,
        observed: &[ObservedWindow],
        observed_focus: Option<&WindowId>,
        hidden: &BTreeSet<WindowId>,
    ) -> Result<(DomainLayout, u64, ObservationConvergence), LayoutFailure> {
        let refuse = LayoutFailure::Semantics;

        // Pre-candidate validation: retained topology, observed id shape and
        // exact single-domain homing, before any candidate work.
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
        let original_remembered = self.remembered_leaf(key);
        let original_focus_leaf = original.focused_leaf.clone();
        let original_stack = original.focus_stack.clone();
        let mut candidate = original.clone();

        let policy = self.policy.as_ref();
        let mut removed = 0usize;
        let mut admitted = 0usize;

        // Missing known tiled/exception windows first, in chain iteration
        // order. A removed tiled leaf stores the pre-removal tree and its leaf
        // for restore, regardless of the hidden set.
        for id in original.leaves.keys().chain(original.exceptions.keys()) {
            if observed_by_id.contains_key(id) {
                continue;
            }
            remove_missing_known_window(&mut candidate, id, policy)?;
            removed += 1;
        }

        // Tiled observed floating: remove the leaf and retain a floating
        // exception, keeping geometry already held.
        let mut flags_adopted = 0usize;
        for (id, entry) in &observed_by_id {
            if !entry.floating || !candidate.leaves.contains_key(*id) {
                continue;
            }
            demote_tiled_to_floating(&mut candidate, id, None, policy);
            flags_adopted += 1;
        }

        // Known floating observed tiled, sticky adoption, fixed_auto origin,
        // and brand-new floating/fullscreen/fixed classification.
        let originally_known: BTreeSet<WindowId> = original
            .leaves
            .keys()
            .chain(original.exceptions.keys())
            .cloned()
            .collect();
        let original_exceptions: BTreeSet<WindowId> = original.exceptions.keys().cloned().collect();
        let mut automatic_fixed = std::mem::take(&mut candidate.automatic_fixed);
        let mut fixed_tile_override = std::mem::take(&mut candidate.fixed_tile_override);
        let (to_tile, classify_adopted) = classify_observed_windows(
            &mut ClassificationDomains::Single(&mut candidate),
            &observed_by_id,
            &originally_known,
            &original_exceptions,
            &original.fixed_tile_override,
            &mut automatic_fixed,
            &mut fixed_tile_override,
            self.config.fixed_size_admission,
            self.config.fixed_size_predicate,
        );
        candidate.automatic_fixed = automatic_fixed;
        candidate.fixed_tile_override = fixed_tile_override;
        flags_adopted += classify_adopted;

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
            if restore_slot_into_domain(
                &mut candidate,
                id,
                &slot,
                policy,
                &self.group_alloc,
                &mut existing_ids,
            ) {
                restored.insert((*id).clone());
                seeded_order.push(slot.leaf);
                admitted += 1;
            }
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
        let mut automatic_fixed = std::mem::take(&mut candidate.automatic_fixed);
        let mut fixed_tile_override = std::mem::take(&mut candidate.fixed_tile_override);
        let mut capacity_floats = std::mem::take(&mut candidate.capacity_floats);
        let mut admission_order = std::mem::take(&mut candidate.admission_order);
        for entry in observed {
            let id = &entry.window;
            if entry.floating
                || candidate.leaves.contains_key(id)
                || candidate.exceptions.contains_key(id)
            {
                continue;
            }
            let current = candidate.tree.as_ref();
            let observed_leaf =
                observed_focus.and_then(|focused| candidate.leaves.get(focused).cloned());
            let evolving_leaf = evolving
                .as_ref()
                .filter(|leaf| domain_focus_resolves(&candidate, leaf))
                .cloned();
            let eligible = if hypr {
                observed_leaf.or_else(|| {
                    fresh_admit_anchor(
                        self.config,
                        current,
                        original_remembered.clone(),
                        &candidate,
                        hypr,
                    )
                })
            } else {
                evolving_leaf.or_else(|| {
                    fresh_admit_anchor(
                        self.config,
                        current,
                        original_remembered.clone(),
                        &candidate,
                        hypr,
                    )
                })
            };
            if let Some(leaf_id) = admit_observed_window_leg(
                &mut candidate,
                entry,
                eligible,
                policy,
                &self.group_alloc,
                self.config,
                &mut existing_ids,
                &hints_all,
                None,
                &mut automatic_fixed,
                &mut fixed_tile_override,
                &mut capacity_floats,
                &mut admission_order,
                &mut new_counter,
            )? {
                evolving = Some(leaf_id.clone());
                seeded_order.push(leaf_id);
            }
            if !to_tile.contains(id) {
                admitted += 1;
            }
        }
        candidate.automatic_fixed = automatic_fixed;
        candidate.fixed_tile_override = fixed_tile_override;
        candidate.capacity_floats = capacity_floats;
        candidate.admission_order = admission_order;
        candidate.capacity_floats.retain(|id| {
            candidate.leaves.contains_key(id) || candidate.exceptions.contains_key(id)
        });

        // D7 repair with observation hints, protected fullscreen/maximized/
        // sticky holds, empty live frames and the original stack/focus. The
        // returned focus is ignored; exact focus selection happens later.
        let protected = collect_protected_tiled(&observed_by_id, &candidate);
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
        append_seeded_order(&mut stack_base, &seeded_order);

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
        let mut automatic_fixed = std::mem::take(&mut candidate.automatic_fixed);
        let mut fixed_tile_override = std::mem::take(&mut candidate.fixed_tile_override);
        let mut capacity_floats = std::mem::take(&mut candidate.capacity_floats);
        let mut admission_order = std::mem::take(&mut candidate.admission_order);
        retain_convergence_provenance(
            &mut automatic_fixed,
            &mut fixed_tile_override,
            &mut capacity_floats,
            &mut admission_order,
            hidden,
            |id| candidate.leaves.contains_key(id) || candidate.exceptions.contains_key(id),
            |id| candidate.minimized_slots.contains_key(id),
        );
        candidate.automatic_fixed = automatic_fixed;
        candidate.fixed_tile_override = fixed_tile_override;
        candidate.capacity_floats = capacity_floats;
        candidate.admission_order = admission_order;

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
        Ok((candidate, new_counter, counts))
    }
}
