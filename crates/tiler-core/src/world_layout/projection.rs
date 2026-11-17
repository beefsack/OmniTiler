//! Hints-aware geometry projection over retained layout storage.
//!
//! A temporary borrowed leaf-to-window index is built once per call from the
//! retained window-to-leaf bindings; geometry and hinted projection run
//! through `geometry::project` and `size_hints::project_with_hints` unchanged.
//! Three distinct gates share one projector implementation:
//!
//! - advisory: hinted geometry in child order (`project_domain`);
//! - affected-domain geometry: hinted geometry plus a nonempty check
//!   (`project_domain_geometry`, the gate orientation uses);
//! - admission/repair fit: complete binding plus plain and hinted projection
//!   (`project_domain_fit`), kept separate and never used for orientation.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Node, NodeId, WindowId};
use crate::geometry::Rect;
use crate::session::{DesiredGeometry, DomainKey};
use crate::size_hints::WindowSizeHints;

use super::state::WorldLayout;
use super::topology::collect_leaves;

/// Typed advisory projection failure. An unknown domain or a domain with no
/// tree projects to an empty vector, while an unprojectable tree, unbound
/// projected leaf or non-positive rectangle fails closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionFailure {
    Unprojectable,
    UnboundLeaf,
    NonPositiveRect,
}

type LeafWindowIndex<'a> = BTreeMap<&'a NodeId, &'a WindowId>;

fn leaf_window_index(leaves: &BTreeMap<WindowId, NodeId>) -> LeafWindowIndex<'_> {
    leaves.iter().map(|(window, leaf)| (leaf, window)).collect()
}

fn project_with_index(
    key: &DomainKey,
    tree: &Node,
    bounds: Rect,
    gap: i32,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    leaf_to_window: &LeafWindowIndex<'_>,
) -> Result<Vec<DesiredGeometry>, ProjectionFailure> {
    let resolve = |leaf: &NodeId| {
        leaf_to_window
            .get(leaf)
            .and_then(|window| hints.get(*window))
            .copied()
            .unwrap_or_else(WindowSizeHints::none)
    };
    let hinted = crate::size_hints::project_with_hints(tree, bounds, gap, &resolve)
        .map_err(|_| ProjectionFailure::Unprojectable)?;
    let over: BTreeSet<&NodeId> = hinted.overconstrained.iter().collect();
    let mut out = Vec::with_capacity(hinted.leaves.len());
    for leaf in hinted.leaves {
        let Some(window) = leaf_to_window.get(&leaf.leaf) else {
            return Err(ProjectionFailure::UnboundLeaf);
        };
        if leaf.rect.w <= 0 || leaf.rect.h <= 0 {
            return Err(ProjectionFailure::NonPositiveRect);
        }
        out.push(DesiredGeometry {
            window: (*window).clone(),
            leaf: leaf.leaf.clone(),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            rect: leaf.rect,
            overconstrained: over.contains(&leaf.leaf),
            client_clamped: false,
        });
    }
    Ok(out)
}

fn project_borrowed(
    key: &DomainKey,
    tree: &Node,
    leaves: &BTreeMap<WindowId, NodeId>,
    bounds: Rect,
    gap: i32,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
) -> Result<Vec<DesiredGeometry>, ProjectionFailure> {
    let index = leaf_window_index(leaves);
    project_with_index(key, tree, bounds, gap, hints, &index)
}

/// Affected-domain geometry gate: hinted projection plus a nonempty check for
/// a non-empty tree. Complete leaf binding and topology are the caller's
/// validation duty; minimum overconstraint is reported per window and never
/// disqualifies. Child order preserved.
pub(super) fn project_borrowed_geometry(
    key: &DomainKey,
    tree: &Node,
    leaves: &BTreeMap<WindowId, NodeId>,
    bounds: Rect,
    gap: i32,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
) -> Option<Vec<DesiredGeometry>> {
    let geometry = project_borrowed(key, tree, leaves, bounds, gap, hints).ok()?;
    if geometry.is_empty() && !collect_leaves(tree).is_empty() {
        return None;
    }
    Some(geometry)
}

/// Admission/repair fit proof: complete, non-duplicated leaf binding plus
/// plain geometric fit and successful hinted projection. Kept separate from
/// the affected-domain geometry gate; minimum overconstraint never
/// disqualifies here either.
#[must_use]
pub(super) fn project_borrowed_fit(
    key: &DomainKey,
    tree: &Node,
    leaves: &BTreeMap<WindowId, NodeId>,
    bounds: Rect,
    gap: i32,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
) -> bool {
    let index = leaf_window_index(leaves);
    if index.len() != collect_leaves(tree).len() {
        return false;
    }
    if crate::geometry::project(tree, bounds, gap).is_err() {
        return false;
    }
    project_with_index(key, tree, bounds, gap, hints, &index).is_ok()
}

/// Order affected-domain geometry by `(output, workspace, leaf)`, matching
/// the affected projector so single-domain orientation stays byte-identical.
pub(super) fn sort_geometry(geometry: &mut [DesiredGeometry]) {
    geometry.sort_by(|a, b| {
        a.output
            .0
            .cmp(&b.output.0)
            .then(a.workspace.0.cmp(&b.workspace.0))
            .then(a.leaf.0.cmp(&b.leaf.0))
    });
}

impl WorldLayout {
    /// Advisory hints-aware projection of one retained domain in source child
    /// order. An unknown domain or a domain with no tree yields an empty vector.
    pub(crate) fn project_domain(
        &self,
        key: &DomainKey,
        hints: &BTreeMap<WindowId, WindowSizeHints>,
    ) -> Result<Vec<DesiredGeometry>, ProjectionFailure> {
        let Some(domain) = self.domain(key) else {
            return Ok(Vec::new());
        };
        let Some(tree) = &domain.tree else {
            return Ok(Vec::new());
        };
        project_borrowed(key, tree, &domain.leaves, domain.bounds, domain.gap, hints)
    }

    /// Affected-domain geometry gate: hinted projection plus a nonempty check
    /// for a non-empty tree, in source child order. `None` means the domain is
    /// unknown, has no tree, or fails the hinted/nonempty gate. Complete
    /// binding and topology are validated separately by the caller.
    pub(crate) fn project_domain_geometry(
        &self,
        key: &DomainKey,
        hints: &BTreeMap<WindowId, WindowSizeHints>,
    ) -> Option<Vec<DesiredGeometry>> {
        let domain = self.domain(key)?;
        let tree = domain.tree.as_ref()?;
        project_borrowed_geometry(key, tree, &domain.leaves, domain.bounds, domain.gap, hints)
    }

    /// Admission/repair fit proof for one retained domain: complete binding
    /// plus plain geometric fit and successful hinted projection. Distinct
    /// from the affected-domain geometry gate; orientation does not use it.
    pub(crate) fn project_domain_fit(
        &self,
        key: &DomainKey,
        hints: &BTreeMap<WindowId, WindowSizeHints>,
    ) -> bool {
        let Some(domain) = self.domain(key) else {
            return false;
        };
        let Some(tree) = &domain.tree else {
            return false;
        };
        project_borrowed_fit(key, tree, &domain.leaves, domain.bounds, domain.gap, hints)
    }

    /// Per-domain accepted layout revision (zero for an unknown domain).
    #[must_use]
    pub(crate) fn accepted_revision(&self, key: &DomainKey) -> u64 {
        self.domain(key)
            .map_or(0, |domain| domain.accepted_revision)
    }
}
