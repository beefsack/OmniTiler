//! Per-domain D7 capacity repair for the sole world layout.
//!
//! A retained tree that no longer projects is repaired in place on the
//! caller's disposable [`DomainLayout`] candidate. The shortest newest-first
//! eligible victim prefix is evicted (first-successful-tiled-admission order,
//! untracked oldest, identity descending for ties) until the tree projects
//! strictly, contracting survivor branches with the shared removal rules.
//! Protected windows never evict: explicit fixed-tile overrides plus the
//! caller-supplied `protected` set. Evicted windows keep their last live
//! frame (live frames, else retained geometry, else none) and record
//! capacity provenance. The caller owns validate-then-install and any
//! revision advance.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::policy::LayoutPolicy;
use crate::session::{DomainKey, ExceptionFlags};
use crate::size_hints::WindowSizeHints;

use super::focus::domain_focus_resolves;
use super::projection::project_borrowed_fit;
use super::state::{DomainException, DomainLayout};
use super::tree_edit::remove_leaf_from_tree;

/// Repair one domain candidate in place. `None` is a narrow truthful hold;
/// the caller discards the candidate on `None`. `focus_stack` seeds the
/// surviving-MRU fallback and `focused` is this domain's current focus leaf:
/// it is kept when it still resolves after repair, otherwise replaced by the
/// newest surviving MRU leaf (or cleared when none resolves). The returned
/// leaf names only a surviving tile, never a floated victim, with no native
/// effect. No revision is advanced here.
#[allow(clippy::too_many_arguments)]
pub(super) fn repair_domain_candidate(
    domain: &mut DomainLayout,
    policy: &dyn LayoutPolicy,
    key: &DomainKey,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    protected: &BTreeSet<WindowId>,
    live_frames: &BTreeMap<WindowId, Rect>,
    focus_stack: &[NodeId],
    focused: Option<&NodeId>,
) -> Option<Option<NodeId>> {
    if domain.bounds.w <= 0 || domain.bounds.h <= 0 || domain.gap < 0 {
        return None;
    }
    let Some(tree) = domain.tree.as_ref() else {
        return Some(focused.cloned());
    };
    if project_borrowed_fit(key, tree, &domain.leaves, domain.bounds, domain.gap, hints) {
        return Some(focused.cloned());
    }
    // Evict the shortest newest-first eligible prefix
    // (untracked lifetimes last, treated as oldest).
    let mut members: Vec<(Option<u64>, WindowId)> = domain
        .leaves
        .keys()
        .map(|window| (domain.admission_order.get(window).copied(), window.clone()))
        .collect();
    sort_victims_newest_first(&mut members);
    let eligible: Vec<WindowId> = members
        .into_iter()
        .map(|(_, id)| id)
        .filter(|id| !protected.contains(id) && !domain.fixed_tile_override.contains(id))
        .collect();
    if eligible.is_empty() {
        return None;
    }
    for victim in &eligible {
        let Some(leaf) = domain.leaves.remove(victim) else {
            continue;
        };
        let current = domain.tree.take();
        domain.tree = remove_leaf_from_tree(policy, current, &leaf);
        let frame = live_frames
            .get(victim)
            .copied()
            .or_else(|| domain.retained_float_geometry.get(victim).copied());
        if let Some(rect) = frame {
            domain.retained_float_geometry.insert(victim.clone(), rect);
        }
        domain.exceptions.insert(
            victim.clone(),
            DomainException {
                flags: ExceptionFlags {
                    floating: true,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                },
                floating_geometry: frame,
            },
        );
        domain.capacity_floats.insert(victim.clone());
        let projects = match domain.tree.as_ref() {
            None => true,
            Some(tree) => {
                project_borrowed_fit(key, tree, &domain.leaves, domain.bounds, domain.gap, hints)
            }
        };
        if projects {
            break;
        }
    }
    let projects = match domain.tree.as_ref() {
        None => true,
        Some(tree) => {
            project_borrowed_fit(key, tree, &domain.leaves, domain.bounds, domain.gap, hints)
        }
    };
    if !projects {
        return None;
    }
    // Surviving-MRU focus only; never a floated victim, no native set.
    if focused.is_none_or(|leaf| domain_focus_resolves(domain, leaf)) {
        return Some(focused.cloned());
    }
    let fallback = focus_stack
        .iter()
        .rev()
        .find_map(|leaf| domain_focus_resolves(domain, leaf).then(|| leaf.clone()));
    Some(fallback)
}

/// Newest-first victim order: higher first-successful-tiled-admission
/// sequence first, untracked lifetimes last (treated as oldest),
/// stable window-identity descending for ties.
fn sort_victims_newest_first(members: &mut [(Option<u64>, WindowId)]) {
    members.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => x.cmp(&y).reverse().then(b.1.0.cmp(&a.1.0)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.1.0.cmp(&a.1.0),
    });
}
