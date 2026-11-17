//! Typed synchronous retained reconciliation over [`WorldLayout`].
//!
//! One caller-selected domain is reconciled against requested work-area
//! bounds/gaps and a converged observation. When the target is absent, a
//! unique same-workspace source on another output may be relocated into it;
//! the relocation prechecks fail closed and produce `Ok(None)` so the caller
//! uses fresh routing. Candidates are built locally and installed atomically
//! only after every gate passes: a refusal mutates no map, and no whole world
//! or source backup is cloned. A retained target rides the caller's convergence;
//! only the relocated target keeps the exact-set fence. No `Session`, engine,
//! event or reply exists here. The outer layer owns identity/pending/divergence
//! fencing plus the exact `domain-mismatch` messages for the two gap mismatches.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::{LayoutFailure, RetainedProjectionOutcome, RetainedReconcileFailure};
use super::projection::project_retained_candidate;
use super::retained::repair_retained_local_candidate;
use super::state::{DomainLayout, WorldLayout};
use super::topology::validate_domain_topology;

impl WorldLayout {
    /// Reconcile one retained (or relocatable) domain against the caller's
    /// requested geometry and converged observation.
    ///
    /// `Ok(None)` means only that the target was absent with no eligible
    /// unique relocation source, so the caller must use fresh routing. Every
    /// other refusal is a typed [`RetainedReconcileFailure`] and leaves this
    /// layout exactly as it was. The relocated candidate adopts the requested
    /// geometry before reconciliation; source removal and target install (or
    /// retirement) happen together only after all gates pass.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reconcile_retained(
        &mut self,
        key: &DomainKey,
        bounds: Rect,
        gap: i32,
        outer_gap: i32,
        focused_window: &WindowId,
        observed: &[EngineWindow],
        closed_windows: &[WindowId],
    ) -> Result<Option<RetainedProjectionOutcome>, RetainedReconcileFailure> {
        // Absent target: select the unique relocation candidate without
        // touching the map. Empty or ineligible candidates still count toward
        // uniqueness, and an ineligible source yields the fresh-routing signal.
        let relocation = if self.domain(key).is_none() {
            match relocation_candidate(self, key, bounds, gap, outer_gap, observed) {
                Some(candidate) => Some(candidate),
                None => return Ok(None),
            }
        } else {
            None
        };

        // Reuse the relocated candidate as the reconciliation base; a retained
        // domain needs one owned clone. No source backup or world clone exists.
        let (source_key, original) = match relocation {
            Some((source_key, moved)) => (Some(source_key), moved),
            None => (
                None,
                self.domain(key).expect("selected domain present").clone(),
            ),
        };
        let relocated = source_key.is_some();

        let (outcome, accepted) = reconcile_selected(
            self,
            key,
            original,
            bounds,
            gap,
            outer_gap,
            focused_window,
            observed,
            closed_windows,
            relocated,
            source_key.as_ref(),
        )?;

        // Atomic install: the only map mutations in this operation happen here,
        // after every gate passed. `None` retires the target; `Some` installs
        // the complete accepted candidate.
        if let Some(source_key) = source_key {
            self.domains.remove(&source_key);
        }
        match accepted {
            Some(candidate) => {
                self.domains.insert(key.clone(), candidate);
            }
            None => {
                self.domains.remove(key);
            }
        }
        Ok(Some(outcome))
    }
}

/// Build the relocated target candidate without inserting it. The unique
/// source shares the workspace on a different output; its outer gap must
/// match the request and at least one observed id must intersect a tiled or
/// exception lifetime (not a restore slot). The source must be a usable,
/// nonempty topology, and the requested relocation gap/bounds must be valid.
/// The candidate adopts the requested geometry here, before any
/// `bounds_changed` decision in reconciliation.
fn relocation_candidate(
    layout: &WorldLayout,
    key: &DomainKey,
    bounds: Rect,
    gap: i32,
    outer_gap: i32,
    observed: &[EngineWindow],
) -> Option<(DomainKey, DomainLayout)> {
    if !(0..=64).contains(&outer_gap) {
        return None;
    }
    let mut source_key: Option<DomainKey> = None;
    for candidate_key in layout.domains.keys() {
        if candidate_key.workspace == key.workspace && candidate_key.output != key.output {
            if source_key.is_some() {
                return None;
            }
            source_key = Some(candidate_key.clone());
        }
    }
    let source_key = source_key?;
    let source = layout.domain(&source_key)?;
    if source.outer_gap != outer_gap {
        return None;
    }
    let intersects = observed.iter().any(|entry| {
        source.leaves.contains_key(&entry.window) || source.exceptions.contains_key(&entry.window)
    });
    if !intersects {
        return None;
    }
    if !validate_domain_topology(&source_key, source) {
        return None;
    }
    if source.leaves.is_empty() && source.exceptions.is_empty() && source.minimized_slots.is_empty()
    {
        return None;
    }
    if !(0..=64).contains(&gap) || bounds.w <= 0 || bounds.h <= 0 {
        return None;
    }
    // Moving the map key derives every home automatically: retained layout
    // data stores no independent output/workspace, so only geometry moves.
    let mut moved = source.clone();
    moved.bounds = bounds;
    moved.gap = gap;
    moved.outer_gap = outer_gap;
    if !validate_domain_topology(key, &moved) {
        return None;
    }
    if moved.leaves.is_empty() && moved.exceptions.is_empty() && moved.minimized_slots.is_empty() {
        return None;
    }
    Some((source_key, moved))
}

/// Reconcile one owned domain candidate at `key` without mutating `layout`.
/// Gap mismatches are checked in inner-then-outer order; the relocated target
/// alone keeps the exact-set fence. Returns the projection outcome plus the
/// complete accepted candidate (`None` for a successful retirement). The
/// caller owns the atomic install.
#[allow(clippy::too_many_arguments)]
fn reconcile_selected(
    layout: &WorldLayout,
    key: &DomainKey,
    original: DomainLayout,
    bounds: Rect,
    gap: i32,
    outer_gap: i32,
    focused_window: &WindowId,
    observed: &[EngineWindow],
    closed_windows: &[WindowId],
    relocated: bool,
    source_exclusion: Option<&DomainKey>,
) -> Result<(RetainedProjectionOutcome, Option<DomainLayout>), RetainedReconcileFailure> {
    let base_revision = original.accepted_revision;

    if original.gap != gap {
        return Err(RetainedReconcileFailure::InnerGapMismatch);
    }
    if original.outer_gap != outer_gap {
        return Err(RetainedReconcileFailure::OuterGapMismatch);
    }

    // The relocated target missed pre-request convergence, so it alone keeps
    // the exact-set fence against tiled links plus exceptions (not slots).
    if relocated {
        let known: BTreeSet<WindowId> = original
            .leaves
            .keys()
            .chain(original.exceptions.keys())
            .cloned()
            .collect();
        let observed_set: BTreeSet<WindowId> =
            observed.iter().map(|entry| entry.window.clone()).collect();
        if observed_set != known {
            return Err(LayoutFailure::Semantics(RefusalKind::PartialObservation).into());
        }
    }

    let malformed =
        || RetainedReconcileFailure::from(LayoutFailure::Semantics(RefusalKind::MalformedTopology));

    // No tree: a missing tree alongside tiled links is malformed. Otherwise a
    // fully empty committed domain retires (`None`). Exceptions/slots adopt
    // the new bounds revision-neutrally and return a complete candidate.
    if original.tree.is_none() {
        if !original.leaves.is_empty() {
            return Err(malformed());
        }
        let committed_empty = original.leaves.is_empty()
            && original.exceptions.is_empty()
            && original.minimized_slots.is_empty();
        if committed_empty {
            return Ok((
                RetainedProjectionOutcome {
                    base_revision,
                    geometry: Vec::new(),
                    focus_leaf: None,
                },
                None,
            ));
        }
        let mut candidate = original;
        candidate.bounds = bounds;
        return Ok((
            RetainedProjectionOutcome {
                base_revision,
                geometry: Vec::new(),
                focus_leaf: None,
            },
            Some(candidate),
        ));
    }

    // Focus: a retained bound leaf projects with that leaf. With no local
    // focus, each focus-none allowance converges to focus none instead of a
    // mismatch; a truly unknown focus refuses.
    let focus_leaf = match original.focused_leaf.as_ref() {
        Some(leaf) => Some(leaf.clone()),
        None if !focused_window.0.is_empty() && original.leaves.contains_key(focused_window) => {
            None
        }
        None if !focused_window.0.is_empty()
            && original
                .exceptions
                .get(focused_window)
                .is_some_and(|exception| exception.flags.floating) =>
        {
            None
        }
        None if !focused_window.0.is_empty()
            && focus_homed_on_other_domain(layout, key, focused_window, source_exclusion) =>
        {
            None
        }
        None if focused_window.0.is_empty() && closed_windows.is_empty() => None,
        _ => {
            return Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch).into());
        }
    };

    let hints: BTreeMap<WindowId, WindowSizeHints> = observed
        .iter()
        .map(|entry| (entry.window.clone(), entry.hints))
        .collect();

    if let Some(geometry) =
        project_retained_candidate(key, &original, bounds, gap, &hints, observed)
    {
        let mut candidate = original;
        candidate.bounds = bounds;
        return Ok((
            RetainedProjectionOutcome {
                base_revision,
                geometry,
                focus_leaf,
            },
            Some(candidate),
        ));
    }

    // Repair only on an actually changed geometry; an unchanged geometry that
    // fails to project is a narrow truthful hold. A relocated candidate already
    // adopted the requested bounds, so it never enters repair. The repaired
    // candidate is projected again even with focus none: a repair that empties
    // the tree is refused because the projector rejects a missing tree.
    if original.bounds == bounds {
        return Err(malformed());
    }
    let (mut candidate, repaired_focus) = repair_retained_local_candidate(
        &original,
        layout.policy.as_ref(),
        key,
        bounds,
        gap,
        &hints,
        observed,
    )
    .ok_or_else(malformed)?;
    let geometry = project_retained_candidate(key, &candidate, bounds, gap, &hints, observed)
        .ok_or_else(malformed)?;
    candidate.outer_gap = outer_gap;
    Ok((
        RetainedProjectionOutcome {
            base_revision,
            geometry,
            focus_leaf: repaired_focus,
        },
        Some(candidate),
    ))
}

/// True when `window` is a known lifetime homed on any other retained domain
/// as a tiled link, float exception or minimize restore slot: the observed
/// per-domain focus is the cross-domain global active, not an unknown window.
/// The relocated source key is excluded because it is not yet installed and
/// the original flow had already removed it at this point.
fn focus_homed_on_other_domain(
    layout: &WorldLayout,
    key: &DomainKey,
    window: &WindowId,
    source_exclusion: Option<&DomainKey>,
) -> bool {
    layout.domains.iter().any(|(other, domain)| {
        other != key
            && source_exclusion.is_none_or(|source| other != source)
            && (domain.leaves.contains_key(window)
                || domain.exceptions.contains_key(window)
                || domain.minimized_slots.contains_key(window))
    })
}
