//! Reusable observation-convergence phase helpers shared by single-domain
//! and canonical-pair convergence.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Node, NodeId, WindowId};
use crate::geometry::Rect;
use crate::policy::LayoutPolicy;
use crate::session::{DomainKey, ExceptionFlags, ObservedWindow, RefusalKind};

use super::group_alloc::GroupAllocator;
use crate::size_hints::{FixedSizePredicate, WindowSizeHints, is_fixed_size_with};

use super::admission::plan_admission_leg;
use super::focus::domain_focus_resolves;
use super::outcome::LayoutFailure;
use super::restore::best_effort_rejoin;
use super::state::{DomainException, DomainLayout, DomainRestoreSlot, LayoutConfig};
use super::tree_edit::{admission_placement_for, generate_group_id, remove_leaf_from_tree};

/// Target domain selector for shared classification without duplicating logic
/// between single-domain and pair convergence.
pub(super) enum ClassificationDomains<'a> {
    Single(&'a mut DomainLayout),
    Pair {
        source_key: &'a DomainKey,
        source: &'a mut DomainLayout,
        target_key: &'a DomainKey,
        target: &'a mut DomainLayout,
    },
}

impl ClassificationDomains<'_> {
    fn contains_exception(&self, id: &WindowId) -> bool {
        match self {
            Self::Single(domain) => domain.exceptions.contains_key(id),
            Self::Pair { source, target, .. } => {
                source.exceptions.contains_key(id) || target.exceptions.contains_key(id)
            }
        }
    }

    fn remove_exception(&mut self, id: &WindowId) {
        match self {
            Self::Single(domain) => {
                domain.exceptions.remove(id);
            }
            Self::Pair { source, target, .. } => {
                source.exceptions.remove(id);
                target.exceptions.remove(id);
            }
        }
    }

    fn insert_exception(&mut self, entry: &ObservedWindow, exception: DomainException) {
        match self {
            Self::Single(domain) => {
                domain.exceptions.insert(entry.window.clone(), exception);
            }
            Self::Pair {
                source_key,
                source,
                target_key,
                target,
            } => {
                if entry.output == source_key.output && entry.workspace == source_key.workspace {
                    source.exceptions.insert(entry.window.clone(), exception);
                } else if entry.output == target_key.output
                    && entry.workspace == target_key.workspace
                {
                    target.exceptions.insert(entry.window.clone(), exception);
                }
            }
        }
    }
}

/// Floating exception with every overlay flag clear; geometry kept only where
/// already held, never fabricated.
pub(super) fn floating_exception(geometry: Option<Rect>) -> DomainException {
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

/// Remove a missing known tiled window (storing its pre-removal restore slot)
/// or missing exception from `domain`.
pub(super) fn remove_missing_known_window(
    domain: &mut DomainLayout,
    id: &WindowId,
    policy: &dyn LayoutPolicy,
) -> Result<(), LayoutFailure> {
    if let Some(leaf) = domain.leaves.remove(id) {
        let current = domain.tree.clone();
        let stored = current.clone();
        domain.tree = remove_leaf_from_tree(policy, current, &leaf);
        if let Some(tree) = stored {
            domain
                .minimized_slots
                .insert(id.clone(), DomainRestoreSlot { leaf, tree });
        }
        Ok(())
    } else if domain.exceptions.remove(id).is_some() {
        Ok(())
    } else {
        Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology))
    }
}

/// Demote a tiled window observed floating into a floating exception, keeping
/// any retained float geometry (`retained_geometry` overrides `domain` when
/// provided for global pair lookups).
pub(super) fn demote_tiled_to_floating(
    domain: &mut DomainLayout,
    id: &WindowId,
    retained_geometry: Option<&BTreeMap<WindowId, Rect>>,
    policy: &dyn LayoutPolicy,
) {
    let leaf = domain.leaves.remove(id).expect("checked by caller");
    let current = domain.tree.clone();
    domain.tree = remove_leaf_from_tree(policy, current, &leaf);
    let geometry = retained_geometry
        .unwrap_or(&domain.retained_float_geometry)
        .get(id)
        .copied();
    domain
        .exceptions
        .insert(id.clone(), floating_exception(geometry));
}

/// Shared known-float-observed-tiled, sticky adoption, fixed_auto origin, and
/// brand-new floating/fullscreen/fixed classification. Returns `(to_tile, flags_adopted)`.
#[allow(clippy::too_many_arguments)]
pub(super) fn classify_observed_windows(
    domains: &mut ClassificationDomains<'_>,
    observed_by_id: &BTreeMap<&WindowId, &ObservedWindow>,
    originally_known: &BTreeSet<WindowId>,
    original_exceptions: &BTreeSet<WindowId>,
    original_override: &BTreeSet<WindowId>,
    automatic_fixed: &mut BTreeSet<WindowId>,
    fixed_tile_override: &mut BTreeSet<WindowId>,
    fixed_admission: bool,
    fixed_predicate: FixedSizePredicate,
) -> (Vec<WindowId>, usize) {
    let mut flags_adopted = 0usize;

    // Known floating observed tiled: automatic/fullscreen holds and
    // sticky/suppress origin rules. Hint changes never reclassify.
    let mut to_tile: Vec<WindowId> = Vec::new();
    for id in original_exceptions {
        if observed_by_id
            .get(id)
            .is_some_and(|entry| !entry.floating && (!fixed_admission || !entry.fullscreen))
            && domains.contains_exception(id)
        {
            to_tile.push(id.clone());
        }
    }
    for id in &to_tile {
        let entry = observed_by_id.get(id).expect("to_tile observed");
        let suppressed = entry.fixed_suppress;
        let overridden = original_override.contains(id);
        let was_automatic = automatic_fixed.contains(id);
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
            automatic_fixed.insert(id.clone());
            flags_adopted += 1;
            continue;
        }
        domains.remove_exception(id);
        automatic_fixed.remove(id);
        if fixed_admission && suppressed {
            fixed_tile_override.insert(id.clone());
        }
        flags_adopted += 1;
    }

    // Sticky observations adopt automatics as intentional.
    for (id, entry) in observed_by_id {
        if entry.sticky && automatic_fixed.contains(*id) {
            automatic_fixed.remove(*id);
        }
    }

    // Adapter-asserted origin on a known floating exception adopts the
    // marker unless a tile win competes.
    for (id, entry) in observed_by_id {
        if !entry.floating || !entry.fixed_auto || entry.fixed_suppress || entry.sticky {
            continue;
        }
        if !domains.contains_exception(id) || automatic_fixed.contains(*id) {
            continue;
        }
        if original_override.contains(*id) || fixed_tile_override.contains(*id) {
            continue;
        }
        if !fixed_admission {
            continue;
        }
        automatic_fixed.insert((*id).clone());
        flags_adopted += 1;
    }

    // Brand-new floating windows become exceptions directly.
    for (id, entry) in observed_by_id {
        if entry.floating && !originally_known.contains(*id) {
            domains.insert_exception(entry, floating_exception(None));
            if fixed_admission
                && entry.fixed_auto
                && !entry.fullscreen
                && !entry.sticky
                && !entry.fixed_suppress
                && !original_override.contains(*id)
            {
                automatic_fixed.insert((*id).clone());
            }
            flags_adopted += 1;
        }
    }

    // Brand-new fullscreen holds slotless without a marker; born maximized
    // floats under its overlay with no reserved tile.
    for (id, entry) in observed_by_id {
        if !fixed_admission || entry.sticky {
            continue;
        }
        if originally_known.contains(*id) || domains.contains_exception(id) {
            continue;
        }
        if !entry.fullscreen || entry.floating {
            continue;
        }
        domains.insert_exception(entry, floating_exception(None));
        flags_adopted += 1;
    }

    // Brand-new fixed windows float with an automatic marker.
    for (id, entry) in observed_by_id {
        if originally_known.contains(*id) || domains.contains_exception(id) {
            continue;
        }
        if fixed_admission
            && !entry.floating
            && !entry.fullscreen
            && !entry.sticky
            && !entry.fixed_suppress
            && !fixed_tile_override.contains(*id)
            && is_fixed_size_with(entry.hints, fixed_predicate)
        {
            domains.insert_exception(entry, floating_exception(None));
            automatic_fixed.insert((*id).clone());
            flags_adopted += 1;
        }
    }

    (to_tile, flags_adopted)
}

/// Attempt exact-tree or best-effort rejoin of a consumed minimize restore
/// slot into `domain`.
pub(super) fn restore_slot_into_domain(
    domain: &mut DomainLayout,
    id: &WindowId,
    slot: &DomainRestoreSlot,
    policy: &dyn LayoutPolicy,
    group_alloc: &GroupAllocator,
    existing_ids: &mut BTreeSet<NodeId>,
) -> bool {
    if existing_ids.contains(&slot.leaf) {
        return false;
    }
    let current = domain.tree.clone();
    let expected = remove_leaf_from_tree(policy, Some(slot.tree.clone()), &slot.leaf);
    let lone_root = matches!(&slot.tree, Node::Leaf { id } if id == &slot.leaf);
    let rejoined = if current == expected {
        Some(slot.tree.clone())
    } else if lone_root {
        // The stored window was the sole resident before it hid. A later
        // arrival cannot be its old sibling anchor, but the hidden resident
        // still owns first place: restore it ahead of the current tree on
        // the workspace's long axis, keeping the newcomer second.
        let Some(current_tree) = current else {
            return false;
        };
        let Some(group_id) = generate_group_id(group_alloc, existing_ids) else {
            return false;
        };
        Some(Node::Group {
            id: group_id,
            axis: policy.admission_axis_for_rect(&domain.bounds),
            children: vec![
                Node::Leaf {
                    id: slot.leaf.clone(),
                },
                current_tree,
            ],
            shares: policy.new_group_shares().to_vec(),
        })
    } else {
        best_effort_rejoin(group_alloc, slot, &current, &domain.leaves, existing_ids)
    };
    let Some(tree) = rejoined else {
        return false;
    };
    domain.tree = Some(tree);
    domain.leaves.insert(id.clone(), slot.leaf.clone());
    existing_ids.insert(slot.leaf.clone());
    true
}

/// Newest resolving leaf from a candidate's MRU base.
pub(super) fn candidate_focus_stack_fallback(
    candidate: &DomainLayout,
    base: &[NodeId],
) -> Option<NodeId> {
    base.iter()
        .rev()
        .find_map(|leaf| domain_focus_resolves(candidate, leaf).then(|| leaf.clone()))
}

/// Cursor-first fresh-admission anchor fallback. `original_remembered` reads
/// the original domain's resolving `last_active` (with pair-remapped IDs when
/// applicable), while the MRU fallback reads `candidate`.
pub(super) fn fresh_admit_anchor(
    config: LayoutConfig,
    current: Option<&Node>,
    original_remembered: Option<NodeId>,
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
        && let Some(cursor) = config.fresh_cursor
        && let Some(closest) = crate::cursor_targeting::cursor_closest_leaf(&projected, cursor)
    {
        return Some(closest);
    }
    if let Some(leaf) = original_remembered
        .or_else(|| candidate_focus_stack_fallback(candidate, &candidate.focus_stack))
    {
        return Some(leaf);
    }
    if hypr {
        return projected.into_iter().next().map(|(leaf, _)| leaf);
    }
    None
}

/// Admit one fresh normal window leg into `domain`, updating tree/leaves or
/// falling back to a capacity float. Returns `Ok(Some(leaf_id))` when tiled,
/// `Ok(None)` when capacity-floated.
#[allow(clippy::too_many_arguments)]
pub(super) fn admit_observed_window_leg(
    domain: &mut DomainLayout,
    entry: &ObservedWindow,
    eligible: Option<NodeId>,
    policy: &dyn LayoutPolicy,
    group_alloc: &GroupAllocator,
    config: LayoutConfig,
    existing_ids: &mut BTreeSet<NodeId>,
    hints_all: &BTreeMap<WindowId, WindowSizeHints>,
    retained_geometry: Option<&BTreeMap<WindowId, Rect>>,
    automatic_fixed: &mut BTreeSet<WindowId>,
    fixed_tile_override: &mut BTreeSet<WindowId>,
    capacity_floats: &mut BTreeSet<WindowId>,
    admission_order: &mut BTreeMap<WindowId, u64>,
    admission_counter: &mut u64,
) -> Result<Option<NodeId>, LayoutFailure> {
    let id = &entry.window;
    let current = domain.tree.clone();
    let placement = admission_placement_for(
        current.as_ref(),
        domain.bounds,
        domain.gap,
        eligible.as_ref(),
    );
    let orientation = policy.admission_axis_for_rect(&placement);
    let leaf_links: BTreeMap<NodeId, WindowId> = domain
        .leaves
        .iter()
        .map(|(window, leaf)| (leaf.clone(), window.clone()))
        .collect();
    let Some(leg) = plan_admission_leg(
        policy,
        group_alloc,
        current,
        domain.bounds,
        domain.gap,
        eligible,
        orientation,
        id,
        existing_ids,
        hints_all,
        &leaf_links,
    ) else {
        return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology));
    };
    let leaf_id = leg.leaf_id;
    let ordinary = leg.ordinary_tree;
    // Exhaustion fail-closed: empty domains never mint.
    if ordinary.is_none() && leg.selected.is_err() {
        return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology));
    }
    let admitted_tree = match leg.selected {
        Ok(target) => Some(target.tree),
        Err(()) => None,
    };
    if let Some(next) = admitted_tree {
        domain.tree = Some(next);
        domain.leaves.insert(id.clone(), leaf_id.clone());
        // First-successful-tiled sequence only; same-lifetime first number
        // kept, restores and floats take no number.
        if !admission_order.contains_key(id) {
            admission_order.insert(id.clone(), *admission_counter);
            *admission_counter = admission_counter.saturating_add(1);
        }
        if config.fixed_size_admission && entry.fixed_suppress {
            automatic_fixed.remove(id);
            fixed_tile_override.insert(id.clone());
        }
        capacity_floats.remove(id);
        Ok(Some(leaf_id))
    } else {
        let geometry = retained_geometry
            .unwrap_or(&domain.retained_float_geometry)
            .get(id)
            .copied();
        domain
            .exceptions
            .insert(id.clone(), floating_exception(geometry));
        capacity_floats.insert(id.clone());
        let fixed_now = is_fixed_size_with(entry.hints, config.fixed_size_predicate);
        if !fixed_tile_override.contains(id) {
            if entry.fixed_suppress && fixed_now {
                automatic_fixed.remove(id);
                fixed_tile_override.insert(id.clone());
            } else {
                fixed_tile_override.remove(id);
            }
        }
        Ok(None)
    }
}

/// Collect tiled windows carrying protected overlay flags for D7 repair.
pub(super) fn collect_protected_tiled(
    observed_by_id: &BTreeMap<&WindowId, &ObservedWindow>,
    domain: &DomainLayout,
) -> BTreeSet<WindowId> {
    let mut protected = BTreeSet::new();
    for (id, entry) in observed_by_id {
        if (entry.fullscreen || entry.maximized || entry.sticky) && domain.leaves.contains_key(*id)
        {
            protected.insert((*id).clone());
        }
    }
    protected
}

/// Append newly seeded/restored leaves to an MRU stack base without
/// reordering existing entries.
pub(super) fn append_seeded_order(stack_base: &mut Vec<NodeId>, seeded_order: &[NodeId]) {
    for leaf in seeded_order {
        if !stack_base.contains(leaf) {
            stack_base.push(leaf.clone());
        }
    }
}

/// Prune automatic/override markers, capacity provenance, and admission order
/// against final presence, restore slots, and the hidden set.
pub(super) fn retain_convergence_provenance(
    automatic_fixed: &mut BTreeSet<WindowId>,
    fixed_tile_override: &mut BTreeSet<WindowId>,
    capacity_floats: &mut BTreeSet<WindowId>,
    admission_order: &mut BTreeMap<WindowId, u64>,
    hidden: &BTreeSet<WindowId>,
    is_present: impl Fn(&WindowId) -> bool,
    has_slot: impl Fn(&WindowId) -> bool,
) {
    automatic_fixed.retain(|id| is_present(id) || hidden.contains(id));
    fixed_tile_override.retain(|id| is_present(id) || hidden.contains(id));
    capacity_floats.retain(|id| is_present(id));
    admission_order.retain(|id, _| is_present(id) || has_slot(id));
}
