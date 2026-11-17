//! Conservative unchanged-domain proof for a single-domain `Reconcile`
//! sweep, over retained [`WorldLayout`] state.
//!
//! This is the typed successor to the session/world live-state skip. Every
//! guard below refuses a false negative to the ordinary per-domain
//! `converge_domain` path; only a proven no-op settles the shared focus tail.
//! All obsolete per-session identity/lifecycle state (owner, generation,
//! revision, fingerprint, divergence, pending, drag) and the per-domain
//! admission counter are deliberately absent here: the layout owns a sole
//! generation-shared high-water mark and no counter needs synchronizing, so a
//! hit is revision-neutral and high-water-neutral by construction. No
//! dispatcher, no full-world scan, no state snapshots and no candidate clone:
//! every refusal precedes the single in-place focus-tail mutation.

use std::collections::BTreeSet;

use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::DomainKey;

use super::state::WorldLayout;
use super::topology::{self, validate_domain_topology};

impl WorldLayout {
    /// Conservative proof that a single-domain `Reconcile` observation is an
    /// exact no-op for `key`, so the caller's unchanged-domain sweep may skip
    /// `converge_domain` without changing retained state or its outcome.
    ///
    /// `bounds`/`gap`/`outer_gap` and every advisory observation flag must
    /// exactly match retained geometry and the all-empty-hints assembly that
    /// convergence would see; any admissible adoption, prune or repair makes
    /// this return `false`. On a hit the exact convergence focus tail is run
    /// against the original stack/last-active, including non-fixed-point MRU
    /// normalization and accepted-history pruning, while revision and the
    /// shared high-water mark stay untouched. False negatives fall through to
    /// the ordinary full convergence path.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_unchanged_reconcile(
        &mut self,
        key: &DomainKey,
        bounds: Rect,
        gap: i32,
        outer_gap: i32,
        focused_window: &WindowId,
        observed: &[EngineWindow],
        closed_windows: &[WindowId],
    ) -> bool {
        if !closed_windows.is_empty() {
            return false;
        }
        let Some(retained) = self.domain(key) else {
            return false;
        };
        if retained.bounds != bounds || retained.gap != gap || retained.outer_gap != outer_gap {
            return false;
        }
        // Provenance convergence could adopt, mirror or prune must be absent.
        if !retained.exceptions.is_empty()
            || !retained.minimized_slots.is_empty()
            || !retained.automatic_fixed.is_empty()
            || !retained.fixed_tile_override.is_empty()
            || !retained.capacity_floats.is_empty()
        {
            return false;
        }
        // Known lifetimes may lack a sequence (convergence never inserts one
        // for an already-known window), but stale ids would be pruned.
        if retained
            .admission_order
            .keys()
            .any(|id| !retained.leaves.contains_key(id))
        {
            return false;
        }
        // One valid retained tree with exact leaf-to-link coverage.
        let Some(tree) = retained.tree.as_ref() else {
            return false;
        };
        if !validate_domain_topology(key, retained) {
            return false;
        }
        let leaves: BTreeSet<NodeId> = topology::collect_leaves(tree).into_iter().collect();
        if leaves.len() != retained.leaves.len()
            || retained.leaves.values().any(|leaf| !leaves.contains(leaf))
        {
            return false;
        }
        // Observation: exact unique ids/homing, every advisory flag clear and
        // no size hints, each id bound to exactly the retained tiled link.
        let mut seen: BTreeSet<&WindowId> = BTreeSet::new();
        for entry in observed {
            if entry.window.0.is_empty()
                || entry.output.0.is_empty()
                || entry.workspace.0.is_empty()
                || !seen.insert(&entry.window)
                || entry.output != key.output
                || entry.workspace != key.workspace
                || !retained.leaves.contains_key(&entry.window)
                || entry.floating
                || entry.fit_excluded
                || entry.fullscreen
                || entry.maximized
                || entry.sticky
                || entry.fixed_auto
                || entry.fixed_suppress
                || !entry.hints.is_empty()
            {
                return false;
            }
        }
        if seen.len() != retained.leaves.len() {
            return false;
        }
        // Only a live, locally resolving tiled focus may skip. An empty event
        // focus is allowed (convergence retains focus); a nonempty one must
        // name that exact retained tiled window.
        let Some(focus_leaf) = retained.focused_leaf.as_ref() else {
            return false;
        };
        if !self.focus_resolves(key, focus_leaf) {
            return false;
        }
        let Some(focus_window) = self.window_for_leaf(key, focus_leaf) else {
            return false;
        };
        if !focused_window.0.is_empty() && focused_window != &focus_window {
            return false;
        }
        // A strict plain projection proves both the D7 repair early-return
        // and the discarded reconcile projection succeed under the all-empty
        // hint map assembled from this observation.
        if crate::geometry::project(tree, retained.bounds, retained.gap).is_err() {
            return false;
        }
        // No seeded admissions/restores happen on an exact match, so the
        // original stack/last-active are the convergence `stack_base` tail
        // inputs. All refusals are above, so this is the sole mutation.
        let stack_base = retained.focus_stack.clone();
        let focused_leaf = retained.focused_leaf.clone();
        self.settle_focus_bookkeeping(key, &stack_base, focused_leaf);
        true
    }
}
