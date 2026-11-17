//! Typed synchronous retained update-gaps reprojection over [`WorldLayout`].
//!
//! One retained domain adopts new projected work-area bounds, inner gap and
//! outer gap, reprojecting its tree with hint-aware client-clamp marking. A
//! local [`DomainLayout`] candidate is validated and installed atomically; no
//! revision is advanced and no full world is cloned. Projection failure
//! triggers one D7 geometric repair. The caller owns convergence plus any
//! identity, pending or divergence fencing; none of that state lives here.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::policy::LayoutPolicy;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::capacity::repair_domain_candidate;
use super::focus::settle_domain_focus_tail;
use super::outcome::{LayoutFailure, RetainedProjectionOutcome};
use super::projection::project_retained_candidate;
use super::state::{DomainLayout, WorldLayout};
use super::topology::validate_domain_topology;

#[cfg(test)]
#[path = "retained_tests.rs"]
mod tests;

impl WorldLayout {
    /// Adopt new bounds, inner gap and outer gap for one retained domain and
    /// reproject its tiled geometry. Converged input is a caller precondition;
    /// this method never runs observation convergence. Any refusal leaves the
    /// layout exactly as it was, including the accepted revision.
    pub(crate) fn update_domain_gaps(
        &mut self,
        key: &DomainKey,
        bounds: Rect,
        gap: i32,
        outer_gap: i32,
        focused_window: &WindowId,
        observed: &[EngineWindow],
    ) -> Result<RetainedProjectionOutcome, LayoutFailure> {
        let Some(original) = self.domain(key) else {
            return Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain));
        };
        let base_revision = original.accepted_revision;

        // No retained tree: an empty or exception/float-only slot adopts the
        // new gaps (or retires when fully empty). A tiled binding or a tiled
        // observation alongside a missing tree stays malformed.
        if original.tree.is_none() {
            let has_tiled_links = !original.leaves.is_empty();
            let has_tiled_observed = observed.iter().any(|entry| {
                !entry.floating && entry.output == key.output && entry.workspace == key.workspace
            });
            if has_tiled_links || has_tiled_observed {
                return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology));
            }
            let committed_empty = original.leaves.is_empty()
                && original.exceptions.is_empty()
                && original.minimized_slots.is_empty();
            if observed.is_empty() && committed_empty {
                self.domains.remove(key);
                return Ok(RetainedProjectionOutcome {
                    base_revision,
                    geometry: Vec::new(),
                    focus_leaf: None,
                });
            }
            let mut candidate = original.clone();
            if !adopt_gaps(&mut candidate, key, bounds, gap) {
                return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology));
            }
            candidate.outer_gap = outer_gap;
            *self.domains.get_mut(key).expect("domain validated") = candidate;
            return Ok(RetainedProjectionOutcome {
                base_revision,
                geometry: Vec::new(),
                focus_leaf: None,
            });
        }

        // Focus: a retained bound leaf in this domain projects with that leaf;
        // an ordinary float homed here (no bound leaf) projects with focus
        // none; anything else is a focus mismatch.
        let focus_leaf = match original.focused_leaf.as_ref() {
            Some(leaf) => Some(leaf.clone()),
            None if !focused_window.0.is_empty()
                && original
                    .exceptions
                    .get(focused_window)
                    .is_some_and(|exception| exception.flags.floating) =>
            {
                None
            }
            _ => return Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch)),
        };

        let hints: BTreeMap<WindowId, WindowSizeHints> = observed
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect();

        if let Some(geometry) =
            project_retained_candidate(key, original, bounds, gap, &hints, observed)
        {
            let mut candidate = original.clone();
            if !adopt_gaps(&mut candidate, key, bounds, gap) {
                return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology));
            }
            candidate.outer_gap = outer_gap;
            *self.domains.get_mut(key).expect("domain validated") = candidate;
            return Ok(RetainedProjectionOutcome {
                base_revision,
                geometry,
                focus_leaf,
            });
        }

        // D7 geometric repair runs only after a projection failure, on the
        // requested bounds/gap and with the caller's observed hints.
        let Some((mut candidate, repaired_focus)) = repair_retained_local_candidate(
            original,
            self.policy.as_ref(),
            key,
            bounds,
            gap,
            &hints,
            observed,
        ) else {
            return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology));
        };

        match repaired_focus {
            None => {
                // Repair emptied the tiled set without emptying membership
                // (floats remain): project empty and keep the slot, unless
                // nothing at all remains, in which case retire it.
                if candidate.leaves.is_empty()
                    && candidate.exceptions.is_empty()
                    && candidate.minimized_slots.is_empty()
                {
                    self.domains.remove(key);
                } else {
                    candidate.outer_gap = outer_gap;
                    *self.domains.get_mut(key).expect("domain validated") = candidate;
                }
                Ok(RetainedProjectionOutcome {
                    base_revision,
                    geometry: Vec::new(),
                    focus_leaf: None,
                })
            }
            Some(leaf) => {
                let Some(geometry) =
                    project_retained_candidate(key, &candidate, bounds, gap, &hints, observed)
                else {
                    return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology));
                };
                candidate.outer_gap = outer_gap;
                *self.domains.get_mut(key).expect("domain validated") = candidate;
                Ok(RetainedProjectionOutcome {
                    base_revision,
                    geometry,
                    focus_leaf: Some(leaf),
                })
            }
        }
    }
}

/// Adopt new bounds and inner gap on a local candidate and gate on retained
/// topology. The outer gap is set by the caller only after success, matching
/// the original commit order. A bad gap range, non-positive bounds or a
/// malformed topology returns `false` with the caller's world untouched (the
/// mutated value is a disposable local clone).
fn adopt_gaps(domain: &mut DomainLayout, key: &DomainKey, bounds: Rect, gap: i32) -> bool {
    if !(0..=64).contains(&gap) || bounds.w <= 0 || bounds.h <= 0 {
        return false;
    }
    domain.bounds = bounds;
    domain.gap = gap;
    validate_domain_topology(key, domain)
}

/// Shared local-candidate D7 repair used by both update-gaps and retained
/// reconciliation. Adopts the requested bounds/gap on a disposable clone,
/// evicts the shortest newest-first eligible victim prefix, settles focus to
/// the newest surviving MRU, prunes stale capacity/admission provenance and
/// validates topology. `None` is a narrow truthful hold and the caller
/// discards the candidate. Revision-neutral; the caller sets the outer gap
/// only after success, preserving the original commit order.
pub(super) fn repair_retained_local_candidate(
    original: &DomainLayout,
    policy: &dyn LayoutPolicy,
    key: &DomainKey,
    bounds: Rect,
    gap: i32,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    observed: &[EngineWindow],
) -> Option<(DomainLayout, Option<NodeId>)> {
    let mut candidate = original.clone();
    if !adopt_gaps(&mut candidate, key, bounds, gap) {
        return None;
    }
    let observed_by_window: BTreeMap<&WindowId, &EngineWindow> = observed
        .iter()
        .map(|entry| (&entry.window, entry))
        .collect();
    let mut protected: BTreeSet<WindowId> = BTreeSet::new();
    let mut live_frames: BTreeMap<WindowId, Rect> = BTreeMap::new();
    for window in candidate.leaves.keys() {
        let Some(entry) = observed_by_window.get(window) else {
            continue;
        };
        if !entry.floating {
            live_frames.insert(window.clone(), entry.rect);
        }
        if entry.fullscreen || entry.maximized || entry.sticky || entry.fit_excluded {
            protected.insert(window.clone());
        }
    }
    let repaired_focus = repair_domain_candidate(
        &mut candidate,
        policy,
        key,
        hints,
        &protected,
        &live_frames,
        &original.focus_stack,
        original.focused_leaf.as_ref(),
    )?;
    settle_domain_focus_tail(
        &mut candidate,
        &original.focus_stack,
        repaired_focus.clone(),
        original.last_active.as_ref(),
    );
    WorldLayout::prune_domain_capacity_state(&mut candidate);
    if !validate_domain_topology(key, &candidate) {
        return None;
    }
    Some((candidate, repaired_focus))
}
