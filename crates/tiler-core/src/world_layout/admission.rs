//! Standalone admission selection for the sole world layout.
//!
//! The ordinary anchor is tried first; only a geometrically failing ordinary
//! anchor consults the largest-first D2 fallback. Every candidate must pass
//! whole-tree strict plus hinted projection with a positive rectangle for
//! every tiled lifetime; minimum flags alone never disqualify (D1, B6 kept).
//! Group ids draw from the generation-shared allocator and burn on trial, so
//! removed ids are never reissued.

use std::collections::{BTreeMap, BTreeSet};

use super::group_alloc::GroupAllocator;
use crate::admission::{
    dfs_leaves, rank_largest_candidates, split_candidate_at, tree_admits_candidate,
};
use crate::directional::{Axis, Node, NodeId, WindowId};
use crate::geometry::{Rect, project};
use crate::policy::LayoutPolicy;
use crate::size_hints::WindowSizeHints;

use super::tree_edit;

/// One selected admission target: the post-insertion tree plus the new leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AdmissionTarget {
    pub(super) tree: Node,
    pub(super) anchor: Option<NodeId>,
}

/// Select one tiled admission target or report policy exhaustion.
///
/// The ordinary anchor is tried first through the caller's ordinary
/// insertion (`ordinary` builds the post-insertion tree for one anchor);
/// only a geometrically failing ordinary anchor consults the D2 fallback.
/// Fallback shapes wrap the candidate leaf old/new with equal shares on
/// its long edge (square ties vertical). An empty candidate list means
/// policy exhaustion, never a tiling impossibility proof.
#[allow(clippy::too_many_arguments)]
pub(super) fn select_admission_target(
    tree: Option<&Node>,
    bounds: Rect,
    gap: i32,
    ordinary_anchor: Option<&NodeId>,
    ordinary_tree: Option<&Node>,
    fallback_anchors: BTreeMap<NodeId, Rect>,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    leaf_to_window: &BTreeMap<NodeId, WindowId>,
    new_leaf: &NodeId,
    group_ids: &mut dyn Iterator<Item = NodeId>,
) -> Result<AdmissionTarget, ()> {
    if let Some(candidate) = ordinary_tree
        && tree_admits_candidate(candidate, bounds, gap, hints, leaf_to_window)
    {
        return Ok(AdmissionTarget {
            tree: candidate.clone(),
            anchor: ordinary_anchor.cloned(),
        });
    }
    let Some(tree) = tree else {
        // Empty domain: the ordinary single leaf is the only shape; its
        // failure is invalid bounds, not search evidence.
        return Err(());
    };
    let Ok(projected) = project(tree, bounds, gap) else {
        return Err(());
    };
    let areas: BTreeMap<NodeId, i64> = projected
        .iter()
        .map(|leaf| {
            (
                leaf.leaf.clone(),
                i64::from(leaf.rect.w) * i64::from(leaf.rect.h),
            )
        })
        .collect();
    let order = rank_largest_candidates(&areas, &dfs_leaves(tree), leaf_to_window);
    for anchor in order {
        if Some(&anchor) == ordinary_anchor {
            continue;
        }
        let Some(rect) = fallback_anchors.get(&anchor) else {
            continue;
        };
        let Some(group_id) = group_ids.next() else {
            return Err(());
        };
        let Some(candidate) = split_candidate_at(tree, &anchor, *rect, new_leaf.clone(), group_id)
        else {
            continue;
        };
        if tree_admits_candidate(&candidate, bounds, gap, hints, leaf_to_window) {
            return Ok(AdmissionTarget {
                tree: candidate,
                anchor: Some(anchor),
            });
        }
    }
    Err(())
}

/// One planned admission leg: the fresh leaf id, the ordinary-anchor
/// post-insertion tree, and the selected target (ordinary, D2 fallback,
/// or policy exhaustion). Shared so every caller runs the identical
/// ordinary-first geometric gate.
pub(super) struct PlannedLeg {
    pub(super) leaf_id: NodeId,
    pub(super) ordinary_tree: Option<Node>,
    pub(super) selected: Result<AdmissionTarget, ()>,
}

/// Plan one tiled admission leg over a working tree.
///
/// `current` is the pre-insertion domain tree (`None` for an empty
/// domain); `eligible` is the caller-resolved ordinary anchor
/// (`None` root-wraps); `orientation` is the caller's ordinary split
/// axis. `existing_ids` reserves the fresh leaf plus every tried fallback
/// group id. Every candidate passes whole-tree strict plus hinted
/// projection; minimum flags alone never disqualify (D1, B6 kept).
/// Group ids draw from the generation-shared allocator: tried fallback
/// ids burn even when their candidate fails projection, and removed ids
/// are never reissued. Exhaustion fails the leg closed.
#[allow(clippy::too_many_arguments)]
pub(super) fn plan_admission_leg(
    policy: &dyn LayoutPolicy,
    alloc: &GroupAllocator,
    current: Option<Node>,
    bounds: Rect,
    gap: i32,
    eligible: Option<NodeId>,
    orientation: Axis,
    window: &WindowId,
    existing_ids: &mut BTreeSet<NodeId>,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    leaf_links: &BTreeMap<NodeId, WindowId>,
) -> Option<PlannedLeg> {
    let leaf_id = tree_edit::generate_leaf_id(alloc, existing_ids)?;
    let ordinary_tree = tree_edit::insert_tiled(
        policy,
        alloc,
        current.clone(),
        eligible.as_ref(),
        leaf_id.clone(),
        orientation,
        existing_ids,
    );
    // Plain-projected allocations rank the largest fallback (D2). An
    // unprojectable retained tree yields no ranking: D7 repair owns
    // that tree, and the newcomer simply exhausts to D3.
    let fallback_anchors: BTreeMap<NodeId, Rect> = match current.as_ref() {
        Some(tree) => project(tree, bounds, gap)
            .map(|leaves| {
                leaves
                    .into_iter()
                    .map(|leaf| (leaf.leaf, leaf.rect))
                    .collect()
            })
            .unwrap_or_default(),
        None => BTreeMap::new(),
    };
    let mut group_ids = std::iter::from_fn(|| {
        let minted = alloc.mint(existing_ids)?;
        existing_ids.insert(minted.clone());
        Some(minted)
    });
    // Whole-tree oracle input: every candidate tree already contains
    // the fresh leaf, so the newcomer mapping rides along (its hints
    // resolve from `hints`; an unknown id would otherwise silently
    // resolve to none).
    let mut oracle_links = leaf_links.clone();
    oracle_links.insert(leaf_id.clone(), window.clone());
    let selected = select_admission_target(
        current.as_ref(),
        bounds,
        gap,
        eligible.as_ref(),
        ordinary_tree.as_ref(),
        fallback_anchors,
        hints,
        &oracle_links,
        &leaf_id,
        &mut group_ids,
    );
    Some(PlannedLeg {
        leaf_id,
        ordinary_tree,
        selected,
    })
}
