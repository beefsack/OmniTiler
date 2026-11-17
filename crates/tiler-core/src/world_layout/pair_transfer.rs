//! Shared two-domain transfer helpers for R4 directional moves and explicit
//! workspace/output sends over `WorldLayout`.

use std::collections::BTreeMap;

use crate::contract::MAX_REVISION;
use crate::directional::{Node, NodeId, WindowId};
use crate::geometry::Rect;
use crate::session::{DesiredGeometry, ExceptionFlags, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::convergence_phases::candidate_focus_stack_fallback;
use super::outcome::{LayoutFailure, PairOperationFailure};
use super::pair::PairCandidate;
use super::pair_operation::{
    pair_geometry_covers_domains, project_pair_geometry, settle_pair_focus_and_prune,
};
use super::projection::project_borrowed_geometry;
use super::state::DomainException;

/// Set `pair.focused_domain` and `pair.focused_leaf` for a transfer with
/// explicit `follow`/`stay` semantics.
///
/// When `follow` is true and `target_tiled_leaf` is `Some(leaf)`, focuses
/// that leaf in the target domain. When `follow` is true and
/// `target_tiled_leaf` is `None` (capacity-float or deferred exception send),
/// focuses the target MRU tiled fallback or first tiled leaf in window-id
/// order (`None` when target has no tiled leaves). When `follow` is false
/// (stay), focuses the source MRU tiled fallback (`None` when source is empty).
pub(super) fn follow_or_stay_pair_focus(
    pair: &mut PairCandidate,
    follow: bool,
    target_tiled_leaf: Option<&NodeId>,
) {
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    if follow {
        if let Some(leaf) = target_tiled_leaf {
            pair.focused_domain = Some(target_key);
            pair.focused_leaf = Some(leaf.clone());
        } else {
            let follow_leaf =
                candidate_focus_stack_fallback(&pair.target, &pair.target.focus_stack)
                    .or_else(|| pair.target.leaves.values().next().cloned());
            match follow_leaf {
                Some(leaf) => {
                    pair.focused_domain = Some(target_key);
                    pair.focused_leaf = Some(leaf);
                }
                None => {
                    pair.focused_domain = None;
                    pair.focused_leaf = None;
                }
            }
        }
    } else {
        match candidate_focus_stack_fallback(&pair.source, &pair.source.focus_stack) {
            Some(next) => {
                pair.focused_domain = Some(source_key);
                pair.focused_leaf = Some(next);
            }
            None => {
                pair.focused_domain = None;
                pair.focused_leaf = None;
            }
        }
    }
}

/// Check `MAX_REVISION`, advance `pair.pair_revision` by one, and settle focus
/// and global provenance pruning without local-pruning before split.
pub(super) fn commit_pair_revision_and_settle(
    pair: &mut PairCandidate,
) -> Result<(u64, u64), LayoutFailure> {
    let base = pair.pair_revision;
    if base >= MAX_REVISION {
        return Err(LayoutFailure::RevisionLimit {
            base,
            limit: MAX_REVISION,
        });
    }
    let revision = base.checked_add(1).ok_or(LayoutFailure::RevisionLimit {
        base,
        limit: MAX_REVISION,
    })?;
    pair.pair_revision = revision;
    settle_pair_focus_and_prune(pair, false);
    Ok((base, revision))
}

/// Stage a slotless capacity-float exception at `pair.target` for a tiled
/// `mover_window` whose destination tree exhausted geometric capacity.
///
/// Preserves the source removal already applied to `pair.source.tree`, restores
/// `pair.target.tree` to `orig_target_tree`, records `capacity_floats`
/// provenance, sets follow/stay focus, validates, projects, and advances/settles
/// `pair` without installing into `WorldLayout`.
#[allow(clippy::too_many_arguments)]
pub(super) fn stage_capacity_float_on_pair(
    pair: &mut PairCandidate,
    mover_window: &WindowId,
    mover_leaf: &NodeId,
    orig_source_tree: Option<&Node>,
    orig_source_leaves: &BTreeMap<WindowId, NodeId>,
    orig_target_tree: Option<Node>,
    mover_live_frame: Option<Rect>,
    allow_cross_output: bool,
    follow: bool,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
) -> Result<(u64, u64, Vec<DesiredGeometry>), PairOperationFailure> {
    let refuse = |kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind));
    let source_key = pair.source_key();

    for domain in [&pair.source_domain, &pair.target_domain] {
        if domain.bounds.w <= 0 || domain.bounds.h <= 0 || domain.gap < 0 {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
    }

    if allow_cross_output {
        let Some(orig_tree) = orig_source_tree else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        let pre_geo = project_borrowed_geometry(
            &source_key,
            orig_tree,
            orig_source_leaves,
            pair.source.bounds,
            pair.source.gap,
            hints,
        );
        if pre_geo.is_none_or(|geo| !geo.iter().any(|g| &g.leaf == mover_leaf)) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
    }

    let mover_frame: Option<Rect> = match (mover_live_frame, allow_cross_output) {
        (Some(live), false) => Some(live),
        (_, true) => None,
        (None, false) => {
            let orig_tree =
                orig_source_tree.ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
            let rect = project_borrowed_geometry(
                &source_key,
                orig_tree,
                orig_source_leaves,
                pair.source.bounds,
                pair.source.gap,
                hints,
            )
            .and_then(|geometry| {
                geometry
                    .into_iter()
                    .find(|g| &g.leaf == mover_leaf)
                    .map(|g| g.rect)
            })
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
            Some(rect)
        }
    };

    pair.target.tree = orig_target_tree;
    pair.source.leaves.remove(mover_window);
    pair.target.leaves.remove(mover_window);
    pair.target.exceptions.insert(
        mover_window.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
            },
            floating_geometry: mover_frame,
        },
    );
    let prior_durable = pair.source.retained_float_geometry.remove(mover_window);
    if let Some(frame) = mover_frame {
        pair.target
            .retained_float_geometry
            .insert(mover_window.clone(), frame);
    } else if let Some(prior) = prior_durable {
        pair.target
            .retained_float_geometry
            .entry(mover_window.clone())
            .or_insert(prior);
    }
    pair.target.capacity_floats.insert(mover_window.clone());

    follow_or_stay_pair_focus(pair, follow, None);

    if !pair.validate_pair_topology() {
        return Err(refuse(RefusalKind::MalformedTopology));
    }

    let geometry =
        project_pair_geometry(pair, hints).ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
    if !pair_geometry_covers_domains(&geometry, pair) {
        return Err(refuse(RefusalKind::MalformedTopology));
    }
    if geometry.iter().any(|g| &g.window == mover_window) {
        return Err(refuse(RefusalKind::MalformedTopology));
    }

    let (base, revision) = commit_pair_revision_and_settle(pair)?;
    Ok((base, revision, geometry))
}
