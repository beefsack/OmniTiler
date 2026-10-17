//! Best-effort capacity policy owned by [`super::Session`].
//!
//! Responsibility: Session-side D2/D3/D4/D5/D7/D8/D10 mechanics that do not
//! fit the per-operation families. Operation families (lifecycle admission,
//! observation convergence, float retry) call into these helpers; world
//! state, projection/validation helpers, and transaction mechanics stay in
//! the parent session module.
//!
//! Contents:
//! - [`super::Session`] policy accessors (`anchor-fallback`, `no-space`)
//!   with next-decision-only timing (D10): setters never touch topology,
//!   membership, focus, revision, or classifications.
//! - Diagnostic capacity-float origin accessors (D4, distinct from the
//!   fixed-size `automatic_fixed` marker) and first-successful-tiled-
//!   admission order accessors (D7 victim order).
//! - Pure per-leg admission selection ([`select_admission_target`]) and
//!   D7 retained-tree repair ([`Session::repair_infeasible_domain`]).
//! - Overlap-gated projection ([`project_domain_geometry`]) so accepted
//!   overlap trees stay publishable until a real capacity decision.

use std::collections::{BTreeMap, BTreeSet};

use crate::admission::{
    AnchorFallback, NoSpaceResult, closest_relative_order, dfs_leaves, rank_largest_candidates,
    split_candidate_at, tree_admits_candidate,
};
use crate::directional::{Axis, Node, NodeId, WindowId, WindowLink};
use crate::geometry::{Rect, project, project_overlap};
use crate::size_hints::{WindowSizeHints, project_overlap_with_hints, project_with_hints};

use super::{DesiredGeometry, DomainKey, OutputDomain};

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
/// Every candidate (ordinary or fallback) must pass whole-tree strict plus
/// hinted projection with a positive rectangle for every tiled lifetime;
/// `overconstrained` minimum flags alone never disqualify (D1, B6 kept).
/// Fallback shapes wrap the candidate leaf old/new with equal shares on
/// its long edge (square ties vertical). `NoSearch` and an empty
/// candidate list mean policy exhaustion, never a tiling impossibility
/// proof.
#[allow(clippy::too_many_arguments)]
pub(super) fn select_admission_target(
    policy: AnchorFallback,
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
    if policy == AnchorFallback::NoSearch {
        return Err(());
    }
    let order: Vec<NodeId> = match policy {
        AnchorFallback::LargestSplittable => {
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
            rank_largest_candidates(&areas, &dfs_leaves(tree), leaf_to_window)
        }
        AnchorFallback::ClosestRelative => match ordinary_anchor {
            Some(anchor) => closest_relative_order(tree, anchor),
            None => dfs_leaves(tree),
        },
        AnchorFallback::NoSearch => return Err(()),
    };
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

impl super::Session {
    /// D2 anchor fallback for subsequent capacity decisions only (D10).
    /// Largest-splittable default.
    #[must_use]
    pub fn anchor_fallback(&self) -> AnchorFallback {
        self.anchor_fallback
    }

    /// Select the D2 anchor fallback for subsequent admissions only.
    /// Never touches topology, membership, focus, revision, or existing
    /// classifications (D10: no reflow or retry on Apply).
    pub fn set_anchor_fallback(&mut self, fallback: AnchorFallback) {
        self.anchor_fallback = fallback;
    }

    /// D3 no-space result for subsequent capacity decisions only (D10).
    /// Float default.
    #[must_use]
    pub fn no_space_result(&self) -> NoSpaceResult {
        self.no_space
    }

    /// Select the D3 no-space result for subsequent admission or actual
    /// capacity-repair decisions only. Never touches topology, membership,
    /// focus, revision, or existing classifications: retained overlap
    /// allocations stay valid until a real capacity decision (D10).
    pub fn set_no_space_result(&mut self, result: NoSpaceResult) {
        self.no_space = result;
    }

    /// Whether a window floats with diagnostic capacity origin (D4):
    /// no split was available and the no-space setting selected float.
    /// Functionally an ordinary float; distinct from automatic fixed.
    #[must_use]
    pub fn is_capacity_float(&self, window: &WindowId) -> bool {
        self.capacity_floats.contains(window)
    }

    /// Number of diagnostic capacity floats (bounded count only).
    #[must_use]
    pub fn capacity_float_count(&self) -> usize {
        self.capacity_floats.len()
    }

    /// First-successful-tiled-admission sequence number for a window, if
    /// tracked (D7 victim order input).
    #[must_use]
    pub fn admission_sequence(&self, window: &WindowId) -> Option<u64> {
        self.admission_order.get(window).copied()
    }

    /// Whether a domain's retained tree was accepted via overlap (D9).
    #[must_use]
    pub fn overlap_accepted(&self, key: &DomainKey) -> bool {
        self.overlap_accepted.contains(key)
    }

    /// Whether the staged pending plan capacity-floats `window` (explicit
    /// unfloat no-space report input for the float route).
    #[must_use]
    pub fn pending_stages_capacity_float(&self, window: &WindowId) -> bool {
        self.pending_desired
            .as_ref()
            .is_some_and(|desired| desired.capacity_floats.contains(window))
    }

    /// Drop capacity bookkeeping for windows no longer in membership.
    /// Close and identity replacement clear origin and order (D5); moves
    /// and re-tiles for the same lifetime preserve them. Overlap marks
    /// drop with their domain.
    pub(super) fn prune_capacity_state(&mut self) {
        self.capacity_floats
            .retain(|id| self.windows.contains_key(id) || self.exceptions.contains_key(id));
        self.admission_order.retain(|id, _| {
            self.windows.contains_key(id)
                || self.exceptions.contains_key(id)
                || self.minimized_slots.contains_key(id)
        });
        self.overlap_accepted
            .retain(|key| self.trees.contains_key(key));
        self.refresh_overlap_marks();
    }

    /// Drop overlap-accepted marks whose trees now project strictly or
    /// are gone: only genuinely overflowing retained allocations keep
    /// the mark, so a later strict success never shelters a changed
    /// tree from the next real capacity decision.
    pub(super) fn refresh_overlap_marks(&mut self) {
        let keys: Vec<DomainKey> = self.overlap_accepted.iter().cloned().collect();
        for key in keys {
            let strict = match (
                self.domains.iter().find(|d| d.key() == key),
                self.trees.get(&key).cloned().flatten(),
            ) {
                (Some(domain), Some(tree)) => {
                    crate::geometry::project(&tree, domain.bounds, domain.gap).is_ok()
                }
                _ => false,
            };
            if strict {
                self.overlap_accepted.remove(&key);
            }
        }
        self.overlap_accepted
            .retain(|key| self.trees.contains_key(key));
    }

    /// Repair one retained domain whose tree no longer projects (D7).
    ///
    /// Float mode evicts the shortest newest-first eligible victim
    /// prefix (first-successful-tiled-admission order, untracked oldest,
    /// identity descending for ties) until the retained tree
    /// projects strictly, contracting survivor branches with the
    /// existing removal rules. Protected windows never evict: explicit
    /// fixed-tile overrides plus the caller-supplied `protected` set
    /// (adapter-observed overlays, unsafe or excluded members). Evicted
    /// windows keep their last live frame (`live_frames`, else retained
    /// geometry, else none) and record diagnostic capacity origin.
    /// Overlap mode (D9) instead retains links when the positive
    /// overflow projector succeeds, with no eviction.
    ///
    /// Repair updates only internal tiled focus bookkeeping (a surviving
    /// prior MRU when the focused leaf evicts, else none): it issues no
    /// native focus setter and preserves the observed active window even
    /// when floated. Deferred during gestures: pending, desired, or drag
    /// residue refuses. A protected-only impossible tree, nonpositive
    /// usable area, or invalid topology remains a narrow truthful hold
    /// (`false`, no mutation).
    ///
    /// `geometry_changed` marks a real capacity-repair trigger (changed
    /// bounds/gaps re-evaluate under the currently selected setting).
    /// With unchanged geometry an accepted overlap mark shelters the
    /// tree with no reflow, so a bare settings Apply never evicts.
    pub fn repair_infeasible_domain(
        &mut self,
        key: &DomainKey,
        hints: &BTreeMap<WindowId, WindowSizeHints>,
        protected: &BTreeSet<WindowId>,
        live_frames: &BTreeMap<WindowId, Rect>,
        geometry_changed: bool,
    ) -> bool {
        if self.reconciler.divergence().is_some() {
            return false;
        }
        if self.has_pending() || self.has_pending_desired() || self.has_drag() {
            return false;
        }
        if self.domains.iter().find(|d| &d.key() == key).is_none() {
            return false;
        }
        if !self.validate_current_topology() {
            return false;
        }
        if self.trees.get(key).cloned().flatten().is_none() {
            return true;
        }
        let focus_stack = self.focus_stack.get(key).cloned().unwrap_or_default();
        let focused = (self.focused_domain.clone(), self.focused_leaf.clone());
        let mut repair = DomainRepair {
            policy: self.policy.clone(),
            no_space: self.no_space,
            domains: self.domains.clone(),
            trees: self.trees.clone(),
            windows: self.windows.clone(),
            exceptions: self.exceptions.clone(),
            retained: self.retained_float_geometry.clone(),
            capacity: self.capacity_floats.clone(),
            overlap: self.overlap_accepted.clone(),
            fixed_override: self.fixed_tile_override.clone(),
            admission_order: self.admission_order.clone(),
        };
        let Some(focus) = repair.repair(
            key,
            hints,
            protected,
            live_frames,
            geometry_changed,
            &focus_stack,
            &focused,
        ) else {
            return false;
        };
        if !super::validate_topology(
            &self.domains,
            &repair.trees,
            &repair.windows,
            &repair.exceptions,
            &focus.0,
            &focus.1,
        ) {
            return false;
        }
        self.trees = repair.trees;
        self.windows = repair.windows;
        self.exceptions = repair.exceptions;
        self.retained_float_geometry = repair.retained;
        self.capacity_floats = repair.capacity;
        self.overlap_accepted = repair.overlap;
        self.focused_domain = focus.0;
        self.focused_leaf = focus.1;
        self.focus_stack = self.updated_focus_stack(
            &self.focused_domain,
            &self.focused_leaf,
            &self.trees,
            &self.windows,
        );
        self.last_active = self.updated_last_active(
            &self.focused_domain,
            &self.focused_leaf,
            &self.trees,
            &self.windows,
        );
        self.prune_capacity_state();
        debug_assert!(self.validate_current_topology());
        true
    }
}

/// Working-copy D7 repair shared by direct session repair and observation
/// convergence. Maps move in and back out; the caller assigns them only on
/// `Some`. `focus_stack`/`focused` seed the surviving-MRU fallback for the
/// repaired domain; the returned focus names only surviving tiles (never a
/// floated victim) with no native effect.
pub(super) struct DomainRepair {
    pub policy: std::sync::Arc<dyn crate::policy::LayoutPolicy>,
    pub no_space: NoSpaceResult,
    pub domains: Vec<OutputDomain>,
    pub trees: BTreeMap<DomainKey, Option<Node>>,
    pub windows: BTreeMap<WindowId, WindowLink>,
    pub exceptions: BTreeMap<WindowId, super::ExceptionRecord>,
    pub retained: BTreeMap<WindowId, Rect>,
    pub capacity: BTreeSet<WindowId>,
    pub overlap: BTreeSet<DomainKey>,
    pub fixed_override: BTreeSet<WindowId>,
    pub admission_order: BTreeMap<WindowId, u64>,
}

impl DomainRepair {
    /// Repair one domain in the working maps. `None` is a narrow truthful
    /// hold; the caller discards the working maps on `None`.
    ///
    /// `geometry_changed` distinguishes a real capacity-repair trigger
    /// (changed bounds/gaps re-evaluate under the currently selected
    /// setting) from projection retention (an unchanged tree keeps its
    /// accepted overlap mark with no reflow, so Apply alone never
    /// evicts). D10 governs settings timing, not geometry triggers.
    #[allow(clippy::too_many_arguments)]
    pub fn repair(
        &mut self,
        key: &DomainKey,
        hints: &BTreeMap<WindowId, WindowSizeHints>,
        protected: &BTreeSet<WindowId>,
        live_frames: &BTreeMap<WindowId, Rect>,
        geometry_changed: bool,
        focus_stack: &[NodeId],
        focused: &(Option<DomainKey>, Option<NodeId>),
    ) -> Option<(Option<DomainKey>, Option<NodeId>)> {
        let domain = self.domains.iter().find(|d| &d.key() == key).cloned()?;
        if domain.bounds.w <= 0 || domain.bounds.h <= 0 || domain.gap < 0 {
            return None;
        }
        let Some(tree) = self.trees.get(key).cloned().flatten() else {
            return Some(focused.clone());
        };
        if domain_projects(&domain, &tree, &self.windows, key, hints) {
            self.overlap.remove(key);
            return Some(focused.clone());
        }
        if self.no_space == NoSpaceResult::AllowOverlap
            && domain_projects_overlap(&domain, &tree, &self.windows, key, hints)
        {
            self.overlap.insert(key.clone());
            return Some(focused.clone());
        }
        if !geometry_changed
            && self.overlap.contains(key)
            && domain_projects_overlap(&domain, &tree, &self.windows, key, hints)
        {
            // Unchanged geometry only: a retained overlap allocation
            // stays valid until a real capacity decision, even under
            // the float setting (D10: Apply alone never reflows). A
            // real bounds/gap change re-evaluates under the currently
            // selected setting instead, so the mark never shelters a
            // changed tree from newest-first float repair.
            return Some(focused.clone());
        }
        if self.no_space == NoSpaceResult::AllowOverlap {
            // Overlap mode without a projecting overlap tree is a
            // truthful hold, never a float eviction (D9).
            return None;
        }
        // Float mode: evict the shortest newest-first eligible prefix
        // (untracked lifetimes last, treated as oldest).
        let mut members: Vec<(Option<u64>, WindowId)> = self
            .windows
            .values()
            .filter(|link| link.output == key.output && link.workspace == key.workspace)
            .map(|link| {
                (
                    self.admission_order.get(&link.window).copied(),
                    link.window.clone(),
                )
            })
            .collect();
        sort_victims_newest_first(&mut members);
        let eligible: Vec<WindowId> = members
            .into_iter()
            .map(|(_, id)| id)
            .filter(|id| !protected.contains(id) && !self.fixed_override.contains(id))
            .collect();
        if eligible.is_empty() {
            return None;
        }
        for victim in &eligible {
            let Some(link) = self.windows.remove(victim) else {
                continue;
            };
            let current = self.trees.get(key).cloned().flatten();
            self.trees.insert(
                key.clone(),
                super::remove_leaf_from_tree(&*self.policy, current, &link.leaf),
            );
            let frame = live_frames
                .get(victim)
                .copied()
                .or_else(|| self.retained.get(victim).copied());
            if let Some(rect) = frame {
                self.retained.insert(victim.clone(), rect);
            }
            self.exceptions.insert(
                victim.clone(),
                super::ExceptionRecord {
                    window: victim.clone(),
                    output: key.output.clone(),
                    workspace: key.workspace.clone(),
                    flags: super::ExceptionFlags {
                        floating: true,
                        fullscreen: false,
                        maximized: false,
                        sticky: false,
                    },
                    floating_geometry: frame,
                },
            );
            self.capacity.insert(victim.clone());
            let candidate = self.trees.get(key).cloned().flatten();
            let projects = match candidate {
                None => true,
                Some(ref tree) => domain_projects(&domain, tree, &self.windows, key, hints),
            };
            if projects {
                break;
            }
        }
        let candidate = self.trees.get(key).cloned().flatten();
        let projects = match candidate {
            None => true,
            Some(ref tree) => domain_projects(&domain, tree, &self.windows, key, hints),
        };
        if !projects {
            return None;
        }
        self.overlap.remove(key);
        // Surviving-MRU focus only; never a floated victim, no native set.
        let (mut focus_domain, mut focus_leaf) = focused.clone();
        if !focus_resolves_in(&focus_domain, &focus_leaf, &self.trees, &self.windows) {
            let fallback = focus_stack.iter().rev().find_map(|leaf| {
                focus_resolves_in(
                    &Some(key.clone()),
                    &Some(leaf.clone()),
                    &self.trees,
                    &self.windows,
                )
                .then(|| leaf.clone())
            });
            match fallback {
                Some(leaf) => {
                    focus_domain = Some(key.clone());
                    focus_leaf = Some(leaf);
                }
                None => {
                    if focus_domain.as_ref() == Some(key) {
                        focus_domain = None;
                        focus_leaf = None;
                    }
                }
            }
        }
        Some((focus_domain, focus_leaf))
    }
}

fn focus_resolves_in(
    domain: &Option<DomainKey>,
    leaf: &Option<NodeId>,
    trees: &std::collections::BTreeMap<DomainKey, Option<Node>>,
    windows: &BTreeMap<WindowId, WindowLink>,
) -> bool {
    let (Some(domain), Some(leaf)) = (domain, leaf) else {
        return domain.is_none() && leaf.is_none();
    };
    let Some(tree) = trees.get(domain).cloned().flatten() else {
        return false;
    };
    if !super::collect_leaves(&tree).contains(leaf) {
        return false;
    }
    windows
        .values()
        .any(|l| &l.leaf == leaf && l.output == domain.output && l.workspace == domain.workspace)
}

/// Strict projectability of one domain tree with hints (B6 flags allowed).
pub(super) fn domain_projects(
    domain: &OutputDomain,
    tree: &Node,
    windows: &BTreeMap<WindowId, WindowLink>,
    key: &DomainKey,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
) -> bool {
    let leaf_to_window: BTreeMap<&NodeId, &WindowId> = windows
        .values()
        .filter(|l| l.output == key.output && l.workspace == key.workspace)
        .map(|l| (&l.leaf, &l.window))
        .collect();
    if leaf_to_window.len() != super::collect_leaves(tree).len() {
        return false;
    }
    let resolve = |leaf: &NodeId| {
        leaf_to_window
            .get(leaf)
            .and_then(|window| hints.get(*window))
            .copied()
            .unwrap_or_else(WindowSizeHints::none)
    };
    project(tree, domain.bounds, domain.gap).is_ok()
        && project_with_hints(tree, domain.bounds, domain.gap, &resolve).is_ok()
}

/// Overlap projectability of one domain tree with hints (D9).
pub(super) fn domain_projects_overlap(
    domain: &OutputDomain,
    tree: &Node,
    windows: &BTreeMap<WindowId, WindowLink>,
    key: &DomainKey,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
) -> bool {
    let leaf_to_window: BTreeMap<&NodeId, &WindowId> = windows
        .values()
        .filter(|l| l.output == key.output && l.workspace == key.workspace)
        .map(|l| (&l.leaf, &l.window))
        .collect();
    if leaf_to_window.len() != super::collect_leaves(tree).len() {
        return false;
    }
    if project_overlap(tree, domain.bounds, domain.gap).is_err() {
        return false;
    }
    let resolve = |leaf: &NodeId| {
        leaf_to_window
            .get(leaf)
            .and_then(|window| hints.get(*window))
            .copied()
            .unwrap_or_else(WindowSizeHints::none)
    };
    project_overlap_with_hints(tree, domain.bounds, domain.gap, &resolve).is_ok()
}

/// One planned admission leg: the fresh leaf id, the ordinary-anchor
/// post-insertion tree, and the selected target (ordinary, D2 fallback,
/// or policy exhaustion). Shared by single admission and observation
/// convergence so both run the identical ordinary-first geometric gate.
pub(super) struct PlannedLeg {
    pub leaf_id: NodeId,
    pub ordinary_tree: Option<Node>,
    pub selected: Result<AdmissionTarget, ()>,
}

/// Plan one tiled admission leg over a working tree.
///
/// `current` is the pre-insertion domain tree (`None` for an empty
/// domain); `eligible` is the caller-resolved ordinary anchor
/// (`None` root-wraps); `orientation` is the caller's ordinary split
/// axis (policy placement for fresh admission, preserved untouched).
/// `existing_ids` reserves the fresh leaf plus every tried fallback
/// group id. Every candidate passes whole-tree strict plus hinted
/// projection; minimum flags alone never disqualify (D1, B6 kept).
#[allow(clippy::too_many_arguments)]
pub(super) fn plan_admission_leg(
    policy: &dyn crate::policy::LayoutPolicy,
    anchor_fallback: AnchorFallback,
    current: Option<Node>,
    bounds: Rect,
    gap: i32,
    eligible: Option<NodeId>,
    orientation: Axis,
    window: &WindowId,
    base_revision: u64,
    existing_ids: &mut BTreeSet<NodeId>,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    leaf_links: &BTreeMap<NodeId, WindowId>,
) -> PlannedLeg {
    let leaf_id = super::ops::lifecycle::generate_leaf_id(window, existing_ids);
    existing_ids.insert(leaf_id.clone());
    let ordinary_tree = super::insert_tiled(
        policy,
        current.clone(),
        eligible.as_ref(),
        leaf_id.clone(),
        orientation,
        existing_ids,
        window,
        base_revision,
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
    let mut group_seq: u64 = 0;
    let mut group_ids = std::iter::from_fn(|| {
        group_seq += 1;
        let mut n = group_seq;
        loop {
            let candidate = NodeId(format!("cap-{}-g{n}", window.0));
            if !existing_ids.contains(&candidate) {
                existing_ids.insert(candidate.clone());
                return Some(candidate);
            }
            n += 1;
        }
    });
    // Whole-tree oracle input: every candidate tree already contains
    // the fresh leaf, so the newcomer mapping rides along (its hints
    // resolve from `hints`; an unknown id would otherwise silently
    // resolve to none).
    let mut oracle_links = leaf_links.clone();
    oracle_links.insert(leaf_id.clone(), window.clone());
    let selected = select_admission_target(
        anchor_fallback,
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
    PlannedLeg {
        leaf_id,
        ordinary_tree,
        selected,
    }
}

/// Project one domain tree, gating overlap: strict first; overlap only
/// when `allow_overlap` (D9 decision) or the domain was overlap-accepted
/// (retained allocations stay valid after switching to float until a
/// real capacity decision). Returns `None` for invalid topology or
/// unprojectable geometry under the gate.
#[must_use]
pub fn project_domain_geometry(
    domain: &OutputDomain,
    tree: &Node,
    windows: &BTreeMap<WindowId, WindowLink>,
    key: &DomainKey,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    allow_overlap: bool,
) -> Option<Vec<DesiredGeometry>> {
    if let Ok(strict) =
        super::project_output_geometry(Some(domain), Some(tree), windows, key, hints)
        && (!strict.is_empty() || super::collect_leaves(tree).is_empty())
    {
        return Some(strict);
    }
    if !allow_overlap {
        return None;
    }
    project_domain_overlap(domain, tree, windows, key, hints)
}

/// Project affected domains, gating overlap per domain: strict first;
/// overlap only when `allow(key)` (D9 decision or retained
/// overlap-accepted mark). Returns `None` for unknown domains or
/// unprojectable geometry under the gate. Sorted like the strict
/// affected projector so call sites stay byte-identical on success.
#[must_use]
pub fn project_affected_gated(
    domains: &[OutputDomain],
    trees: &BTreeMap<DomainKey, Option<Node>>,
    windows: &BTreeMap<WindowId, WindowLink>,
    affected: &[DomainKey],
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    allow: &dyn Fn(&DomainKey) -> bool,
) -> Option<Vec<DesiredGeometry>> {
    let mut out = Vec::new();
    for key in affected {
        let domain = domains.iter().find(|d| &d.key() == key)?;
        let tree = trees.get(key).cloned().flatten();
        let projected = match tree.as_ref() {
            None => Vec::new(),
            Some(tree) => project_domain_geometry(domain, tree, windows, key, hints, allow(key))?,
        };
        out.extend(projected);
    }
    out.sort_by(|a: &DesiredGeometry, b: &DesiredGeometry| {
        a.output
            .0
            .cmp(&b.output.0)
            .then(a.workspace.0.cmp(&b.workspace.0))
            .then(a.leaf.0.cmp(&b.leaf.0))
    });
    Some(out)
}

/// Overlap projection of one domain tree with B6 minimum flags (D9).
/// Positive overflowing leaf rectangles; `capacity_overflow` is a
/// projector-level diagnostic and does not ride [`DesiredGeometry`].
pub(super) fn project_domain_overlap(
    domain: &OutputDomain,
    tree: &Node,
    windows: &BTreeMap<WindowId, WindowLink>,
    key: &DomainKey,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
) -> Option<Vec<DesiredGeometry>> {
    let leaf_to_window: BTreeMap<&NodeId, &WindowId> = windows
        .values()
        .filter(|l| l.output == key.output && l.workspace == key.workspace)
        .map(|l| (&l.leaf, &l.window))
        .collect();
    let resolve = |leaf: &NodeId| {
        leaf_to_window
            .get(leaf)
            .and_then(|window| hints.get(*window))
            .copied()
            .unwrap_or_else(WindowSizeHints::none)
    };
    let hinted = project_overlap_with_hints(tree, domain.bounds, domain.gap, &resolve).ok()?;
    let over: BTreeSet<&NodeId> = hinted.overconstrained.iter().collect();
    let mut out = Vec::with_capacity(hinted.leaves.len());
    for leaf in hinted.leaves {
        let window = leaf_to_window.get(&leaf.leaf)?;
        if leaf.rect.w <= 0 || leaf.rect.h <= 0 {
            return None;
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
    Some(out)
}

/// Newest-first victim order: higher first-successful-tiled-admission
/// sequence first, untracked lifetimes last (treated as oldest),
/// stable window-identity descending for ties.
pub(super) fn sort_victims_newest_first(members: &mut [(Option<u64>, WindowId)]) {
    members.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => x.cmp(&y).reverse().then(b.1.0.cmp(&a.1.0)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.1.0.cmp(&a.1.0),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn victims_sort_newest_first_with_untracked_last() {
        let mut members = vec![
            (None, WindowId("untracked-b".to_owned())),
            (Some(0), WindowId("oldest".to_owned())),
            (Some(2), WindowId("newest".to_owned())),
            (None, WindowId("untracked-a".to_owned())),
            (Some(1), WindowId("middle".to_owned())),
        ];
        sort_victims_newest_first(&mut members);
        let ids: Vec<&str> = members.iter().map(|(_, id)| id.0.as_str()).collect();
        assert_eq!(
            ids,
            vec!["newest", "middle", "oldest", "untracked-b", "untracked-a"],
            "newest first, untracked last with identity descending"
        );
    }

    #[test]
    fn victims_break_sequence_ties_by_identity_descending() {
        // A clean-fit roster shares one sequence number, so identity
        // breaks those ties deterministically.
        let mut members = vec![
            (Some(7), WindowId("a".to_owned())),
            (Some(7), WindowId("c".to_owned())),
            (Some(7), WindowId("b".to_owned())),
        ];
        sort_victims_newest_first(&mut members);
        let ids: Vec<&str> = members.iter().map(|(_, id)| id.0.as_str()).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
    }
}
