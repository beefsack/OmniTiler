//! Typed RECONCILE adoption routing for a genuinely absent target domain.
//!
//! This is the narrow world-owned replacement for the old engine's fresh
//! reconcile dispatcher. It runs only after retained reconciliation has
//! returned `Ok(None)`: the target is absent and no unique relocation source
//! was eligible there. Routing mirrors the old `fresh_admit_shared` decide-
//! once/commit-once flow with all identity, pending, divergence, handshake,
//! correlation and reporting machinery dropped.
//!
//! The shared generation allocator burns group serials on every fit, seed and
//! admission trial; a discarded candidate never touches retained layout or
//! the admission high-water. Only the successful route installs a domain and
//! only the relocated legacy route removes a source key, after local
//! admission has already passed. No whole-world clone or rollback backup
//! exists here.

use std::collections::{BTreeMap, BTreeSet};

use crate::bounds::is_gap;
use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::seed::{EngineWindow, observed_window_from_engine, order_spatial_with_focus_last};
use crate::session::{DomainKey, ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::{FixedSizePredicate, WindowSizeHints};

use super::fit::is_fixed_candidate;
use super::focus::{domain_focus_resolves, sync_domain_focus_from_window};
use super::outcome::{
    FreshDomainFailure, FreshDomainOutcome, FreshOutcomeKind, LayoutFailure,
    RetainedProjectionOutcome,
};
use super::projection::project_retained_candidate;
use super::relocation::build_relocated_candidate;
use super::state::{DomainLayout, WorldLayout};
use super::topology::validate_domain_topology;
use super::tree_edit::admission_placement_for;

#[cfg(test)]
#[path = "adoption_tests.rs"]
mod tests;

impl WorldLayout {
    /// Adopt one fresh domain from a complete carried observation. The caller
    /// guarantees the target key is absent and that retained reconciliation
    /// already declined it (its unique relocation source, if any, was
    /// ineligible there). All policy decisions are local and source-exact.
    pub(crate) fn reconcile_fresh(
        &mut self,
        domain: &OutputDomain,
        outer_gap: i32,
        focused_window: &WindowId,
        observed: &[EngineWindow],
    ) -> Result<FreshDomainOutcome, FreshDomainFailure> {
        // Empty truly-fresh observation: revision zero, no retained slot.
        if observed.is_empty() {
            return Ok(FreshDomainOutcome {
                projection: empty_projection(0),
                native_capacity_float: None,
                kind: FreshOutcomeKind::Empty,
            });
        }

        let key = domain.key();
        let Some(anchor) = fresh_anchor(observed, focused_window) else {
            // All floating: no tile anchor exists. An invalid descriptor is a
            // constructor failure here and maps to `SeedFailed`.
            if !domain.validate() || outer_gap < 0 {
                return Err(FreshDomainFailure::SeedFailed);
            }
            // Converge one local empty domain and read the accepted revision
            // from the candidate BEFORE install/retire, matching the old
            // `converge_fresh_floating` base (taken before `store_committed`).
            let semantic = observed_semantic(observed);
            let original = DomainLayout::new(domain.bounds, domain.gap, outer_gap);
            let (candidate, counter, _counts) = match self.converge_candidate(
                &key,
                &original,
                &semantic,
                focus_opt(focused_window),
                &BTreeSet::new(),
            ) {
                Ok(result) => result,
                Err(failure) => return Err(FreshDomainFailure::Layout(failure)),
            };
            let base_revision = candidate.accepted_revision;
            // `store_committed` retires a fully empty candidate before
            // `note_working_counter`, so no high-water advance on retirement.
            if !domain_is_empty(&candidate) {
                self.domains.insert(key.clone(), candidate);
                self.admission_seq_next = counter;
            }
            return Ok(FreshDomainOutcome {
                projection: empty_projection(base_revision),
                native_capacity_float: None,
                kind: FreshOutcomeKind::Converged,
            });
        };

        let fixed_present = self.config.fixed_size_admission
            && observed
                .iter()
                .any(|entry| is_fixed_candidate(entry, self.config.fixed_size_predicate));

        // Clean fit: only the focused non-floating anchor qualifies, and only
        // with no fixed-size member to exclude. A decline falls through with
        // its allocator burns intact.
        if !fixed_present
            && anchor == *focused_window
            && let Ok(admission) = self.try_fitted_admission(domain, outer_gap, &anchor, observed)
        {
            return Ok(FreshDomainOutcome {
                projection: RetainedProjectionOutcome {
                    base_revision: admission.base_revision,
                    geometry: admission.geometry,
                    focus_leaf: admission.admitted_leaf,
                },
                native_capacity_float: None,
                kind: FreshOutcomeKind::Fit,
            });
        }

        let unique_source = self.unique_source_key(&key);
        let mixed = fixed_present
            || observed
                .iter()
                .any(|entry| entry.floating || entry.fullscreen);
        let mut converged = false;

        // An invalid constructor makes the old `converge_fresh_floating` fail
        // with no reply and fall through to the legacy/serial route; it must
        // not surface as a directly invented `Layout` MalformedInput here.
        if unique_source.is_none() && mixed && domain.validate() && outer_gap >= 0 {
            let semantic = observed_semantic(observed);
            match self.converge_fresh_domain(
                domain,
                outer_gap,
                &semantic,
                focus_opt(focused_window),
            ) {
                Ok(_) => converged = true,
                // Mixed convergence is an ordinary semantic/revision failure,
                // never a seed failure: it maps straight through `Layout`.
                Err(failure) => return Err(FreshDomainFailure::Layout(failure)),
            }
            if let Some(outcome) = self.mixed_anchor_projection(&key, domain, &anchor, observed) {
                return Ok(outcome);
            }
            // A failed projection falls through exactly as the source did;
            // the convergence commit is not rolled back.
        }

        // Relocation candidate or fit-excluded members keep the full legacy
        // fresh route. Without either, a genuinely fresh all-normal roster
        // takes the serial seed.
        if unique_source.is_some() || observed.iter().any(|entry| entry.fit_excluded) {
            return self.legacy_admit(
                domain,
                outer_gap,
                &anchor,
                focused_window,
                observed,
                converged,
            );
        }

        self.serial_seed(domain, outer_gap, &anchor, focused_window, observed)
    }

    /// Legacy fresh route: an already-converged stored domain, else a direct
    /// unique-source relocation candidate, else the deterministic tiled seed.
    /// The final anchor admit runs against the complete carried observation.
    #[allow(clippy::too_many_arguments)]
    fn legacy_admit(
        &mut self,
        domain: &OutputDomain,
        outer_gap: i32,
        anchor: &WindowId,
        focused_window: &WindowId,
        observed: &[EngineWindow],
        converged: bool,
    ) -> Result<FreshDomainOutcome, FreshDomainFailure> {
        let key = domain.key();

        if converged {
            // Never reseed after convergence. Old `take_usable_session`
            // checked domain identity (bounds/gap/adjacent here) before
            // emptiness; a mismatch or empty converged candidate fails closed
            // as PartialObservation with the converged state preserved.
            let Some(existing) = self
                .domain(&key)
                .filter(|candidate| validate_domain_topology(&key, candidate))
                .filter(|candidate| {
                    candidate.bounds == domain.bounds && candidate.gap == domain.gap
                })
                .filter(|candidate| !domain_is_empty(candidate))
                .cloned()
            else {
                return Err(partial_observation());
            };
            let counter = self.admission_seq_next;
            let fixed_admission = self.config.fixed_size_admission;
            let fixed_predicate = self.config.fixed_size_predicate;
            return self.admit_into_candidate(
                &key,
                anchor,
                observed,
                &existing,
                counter,
                None,
                fixed_admission,
                fixed_predicate,
            );
        }

        if let Some((source_key, moved)) = self.direct_relocation_candidate(&key, domain, outer_gap)
        {
            let counter = self.admission_seq_next;
            let fixed_admission = self.config.fixed_size_admission;
            let fixed_predicate = self.config.fixed_size_predicate;
            let admitted = self.admit_into_candidate(
                &key,
                anchor,
                observed,
                &moved,
                counter,
                Some(source_key.clone()),
                fixed_admission,
                fixed_predicate,
            );
            match admitted {
                Ok(outcome) => return Ok(outcome),
                // A relocation that missed convergence discards its candidate
                // and rebuilds the legacy seed, preserving the source.
                Err(FreshDomainFailure::Layout(LayoutFailure::Semantics(
                    RefusalKind::PartialObservation,
                )))
                | Err(FreshDomainFailure::Layout(LayoutFailure::RevisionLimit { .. })) => {
                    return self.legacy_seed_admit(
                        domain,
                        outer_gap,
                        anchor,
                        focused_window,
                        observed,
                    );
                }
                // Every other semantic refusal returns with the source intact.
                Err(failure) => return Err(failure),
            }
        }

        self.legacy_seed_admit(domain, outer_gap, anchor, focused_window, observed)
    }

    /// Build the legacy tiled seed over the roster without the anchor, then
    /// admit the anchor under the old session's default fixed-size policy.
    /// The seed leg uses `false`/default (not current config) so the final
    /// anchor proposal cannot be reclassified as a fixed float by today's
    /// setting. The source key is untouched; only the target installs.
    fn legacy_seed_admit(
        &mut self,
        domain: &OutputDomain,
        outer_gap: i32,
        anchor: &WindowId,
        focused_window: &WindowId,
        observed: &[EngineWindow],
    ) -> Result<FreshDomainOutcome, FreshDomainFailure> {
        let key = domain.key();
        let seed_order = order_spatial_with_focus_last(
            observed
                .iter()
                .filter(|entry| &entry.window != anchor)
                .cloned()
                .collect(),
            focused_window,
            true,
        )
        .ok_or(FreshDomainFailure::AmbiguousPlacement)?;
        let (seeded, counter) = self
            .build_tiled_seed(domain, outer_gap, &seed_order)
            .map_err(|_| FreshDomainFailure::SeedFailed)?;
        self.admit_into_candidate(
            &key,
            anchor,
            observed,
            &seeded,
            counter,
            None,
            false,
            FixedSizePredicate::default(),
        )
    }

    /// Propose the anchored admission from one local candidate and commit only
    /// after it passes. `source_to_remove` is set only for the direct legacy
    /// relocation path; source removal and target insertion happen together.
    #[allow(clippy::too_many_arguments)]
    fn admit_into_candidate(
        &mut self,
        key: &DomainKey,
        anchor: &WindowId,
        observed: &[EngineWindow],
        original: &DomainLayout,
        admission_counter: u64,
        source_to_remove: Option<DomainKey>,
        fixed_admission: bool,
        fixed_predicate: FixedSizePredicate,
    ) -> Result<FreshDomainOutcome, FreshDomainFailure> {
        let semantic = observed_semantic(observed);
        let placement = seed_placement(original);
        let (candidate, new_counter, admission) = self
            .build_admission_candidate(
                key,
                Some(original),
                admission_counter,
                anchor,
                placement,
                false,
                false,
                &semantic,
                fixed_admission,
                fixed_predicate,
            )
            .map_err(FreshDomainFailure::Layout)?;

        // The legacy route returns the admission plan verbatim: child-order
        // geometry with no client-clamp marks (empty on a capacity branch),
        // the plan's desired focus, and no native float target. It does not
        // reproject the committed candidate.
        let outcome = FreshDomainOutcome {
            projection: RetainedProjectionOutcome {
                base_revision: admission.base_revision,
                geometry: admission.geometry,
                focus_leaf: candidate.focused_leaf.clone(),
            },
            native_capacity_float: None,
            kind: FreshOutcomeKind::Legacy,
        };

        // The only map mutations in this route, after local admission passed.
        if let Some(source_key) = source_to_remove {
            self.domains.remove(&source_key);
        }
        if domain_is_empty(&candidate) {
            self.domains.remove(key);
        } else {
            self.domains.insert(key.clone(), candidate);
        }
        self.admission_seq_next = new_counter;
        Ok(outcome)
    }

    /// Serial spatial seed for a genuinely fresh all-normal roster: one
    /// convergence on an empty local domain, then the source-exact anchor
    /// tile/exception/capacity projection. Convergence errors other than the
    /// revision bound are seed failures here; the revision bound maps through
    /// `Layout` like any other ordinary failure.
    fn serial_seed(
        &mut self,
        domain: &OutputDomain,
        outer_gap: i32,
        anchor: &WindowId,
        focused_window: &WindowId,
        observed: &[EngineWindow],
    ) -> Result<FreshDomainOutcome, FreshDomainFailure> {
        let key = domain.key();
        // A failed `Session::new` was the old seed-failed shape.
        if !domain.validate() || outer_gap < 0 {
            return Err(FreshDomainFailure::SeedFailed);
        }
        let ordered = order_spatial_with_focus_last(observed.to_vec(), anchor, true)
            .unwrap_or_else(|| observed.to_vec());
        let semantic = observed_semantic(&ordered);
        let original = DomainLayout::new(domain.bounds, domain.gap, outer_gap);
        let (candidate, counter, _counts) = match self.converge_candidate(
            &key,
            &original,
            &semantic,
            focus_opt(focused_window),
            &BTreeSet::new(),
        ) {
            Ok(result) => result,
            Err(LayoutFailure::RevisionLimit { base, limit }) => {
                return Err(FreshDomainFailure::Layout(LayoutFailure::RevisionLimit {
                    base,
                    limit,
                }));
            }
            // Non-revision convergence failures are seed failures here.
            Err(_) => return Err(FreshDomainFailure::SeedFailed),
        };

        // `store_committed` retires a fully empty result before
        // `note_working_counter`: no slot and no high-water advance on retire.
        if domain_is_empty(&candidate) {
            self.domains.remove(&key);
        } else {
            self.domains.insert(key.clone(), candidate);
            self.admission_seq_next = counter;
        }

        self.serial_anchor_projection(&key, domain, anchor, observed)
    }

    /// Mixed-route anchor projection (old engine.rs 1998-2123). Both the tiled
    /// and exception attempts require the anchor to actually be a bound tile
    /// or exception, retained focus to be `Some`, and the projector to succeed.
    /// A committed-empty domain projects empty. Any other miss falls through
    /// to legacy/serial routing; there is no all-floated empty-success branch
    /// here, and the tile focus sync is installed only on projector success.
    fn mixed_anchor_projection(
        &mut self,
        key: &DomainKey,
        domain: &OutputDomain,
        anchor: &WindowId,
        observed: &[EngineWindow],
    ) -> Option<FreshDomainOutcome> {
        let installed = self.domain(key).cloned()?;
        let is_tiled = installed.leaves.contains_key(anchor);
        if !is_tiled && !installed.exceptions.contains_key(anchor) {
            return None;
        }
        let mut focused = installed;
        if is_tiled {
            let _ = sync_domain_focus_from_window(&mut focused, anchor);
        }
        if domain_is_empty(&focused) {
            return Some(FreshDomainOutcome {
                projection: empty_projection(focused.accepted_revision),
                native_capacity_float: None,
                kind: FreshOutcomeKind::Converged,
            });
        }
        let focus_leaf = focused.focused_leaf.clone()?;
        let outcome = project_anchor_result(
            key,
            domain,
            &focused,
            observed,
            Some(focus_leaf),
            None,
            FreshOutcomeKind::Converged,
        )?;
        if let Some(slot) = self.domains.get_mut(key) {
            *slot = focused;
        }
        Some(outcome)
    }

    /// Serial-seed anchor projection (old engine.rs 2268-2406). A tiled anchor
    /// needs retained focus `Some` and a successful projector else SeedFailed.
    /// An exception anchor tolerates focus `None`, returns the projected
    /// remainder (capacity target if marked) or, when the roster is all-floated
    /// / unprojectable, an empty projection with that same capacity target.
    /// An anchor that is neither tile nor exception is SeedFailed.
    fn serial_anchor_projection(
        &mut self,
        key: &DomainKey,
        domain: &OutputDomain,
        anchor: &WindowId,
        observed: &[EngineWindow],
    ) -> Result<FreshDomainOutcome, FreshDomainFailure> {
        let installed = self
            .domain(key)
            .cloned()
            .ok_or(FreshDomainFailure::SeedFailed)?;
        let is_tiled = installed.leaves.contains_key(anchor);
        let is_exception = installed.exceptions.contains_key(anchor);
        if !is_tiled && !is_exception {
            return Err(FreshDomainFailure::SeedFailed);
        }
        let is_capacity = installed.capacity_floats.contains(anchor);
        let capacity_target = || is_capacity.then(|| anchor.clone());
        let mut focused = installed;
        if is_tiled {
            let _ = sync_domain_focus_from_window(&mut focused, anchor);
        }
        if domain_is_empty(&focused) {
            return Ok(FreshDomainOutcome {
                projection: empty_projection(focused.accepted_revision),
                native_capacity_float: if is_tiled { None } else { capacity_target() },
                kind: FreshOutcomeKind::SerialSeed,
            });
        }
        if is_tiled {
            let focus_leaf = focused
                .focused_leaf
                .clone()
                .ok_or(FreshDomainFailure::SeedFailed)?;
            let outcome = project_anchor_result(
                key,
                domain,
                &focused,
                observed,
                Some(focus_leaf),
                None,
                FreshOutcomeKind::SerialSeed,
            )
            .ok_or(FreshDomainFailure::SeedFailed)?;
            if let Some(slot) = self.domains.get_mut(key) {
                *slot = focused;
            }
            return Ok(outcome);
        }
        let focus_leaf = focused.focused_leaf.clone();
        let target = capacity_target();
        if let Some(outcome) = project_anchor_result(
            key,
            domain,
            &focused,
            observed,
            focus_leaf,
            target.clone(),
            FreshOutcomeKind::SerialSeed,
        ) {
            if let Some(slot) = self.domains.get_mut(key) {
                *slot = focused;
            }
            Ok(outcome)
        } else {
            Ok(FreshDomainOutcome {
                projection: empty_projection(focused.accepted_revision),
                native_capacity_float: target,
                kind: FreshOutcomeKind::SerialSeed,
            })
        }
    }

    /// Single unique same-workspace, other-output source key. More than one
    /// candidate is ambiguous and behaves as no source, matching the old
    /// `find_unique_source_for_target` gate.
    pub(super) fn unique_source_key(&self, key: &DomainKey) -> Option<DomainKey> {
        let mut source: Option<DomainKey> = None;
        for candidate in self.domains.keys() {
            if candidate.workspace == key.workspace && candidate.output != key.output {
                if source.is_some() {
                    return None;
                }
                source = Some(candidate.clone());
            }
        }
        source
    }

    /// Direct legacy relocation candidate at this seam: a valid request outer
    /// gap, absent target and exactly one usable nonempty source whose
    /// geometry validates at the target. Unlike `reconcile_retained`'s
    /// relocation precheck this has no observed intersection or source
    /// outer-gap match, matching `try_relocate_for_target` exactly.
    pub(super) fn direct_relocation_candidate(
        &self,
        key: &DomainKey,
        domain: &OutputDomain,
        outer_gap: i32,
    ) -> Option<(DomainKey, DomainLayout)> {
        if !is_gap(outer_gap) || !is_gap(domain.gap) || self.domain(key).is_some() {
            return None;
        }
        let source_key = self.unique_source_key(key)?;
        let source = self.domain(&source_key)?;
        if !validate_domain_topology(&source_key, source) || domain_is_empty(source) {
            return None;
        }
        let moved = build_relocated_candidate(source, key, domain.bounds, domain.gap, outer_gap)?;
        if domain_is_empty(&moved) {
            return None;
        }
        Some((source_key, moved))
    }
}

/// Anchor for a fresh seed: the focused non-floating window, else the first
/// non-floating member in observation order.
fn fresh_anchor(observed: &[EngineWindow], focused: &WindowId) -> Option<WindowId> {
    if !focused.0.is_empty()
        && observed
            .iter()
            .any(|entry| &entry.window == focused && !entry.floating)
    {
        return Some(focused.clone());
    }
    observed
        .iter()
        .find(|entry| !entry.floating)
        .map(|entry| entry.window.clone())
}

fn focus_opt(focused: &WindowId) -> Option<&WindowId> {
    (!focused.0.is_empty()).then_some(focused)
}

fn observed_semantic(observed: &[EngineWindow]) -> Vec<ObservedWindow> {
    observed.iter().map(observed_window_from_engine).collect()
}

fn fresh_hints(observed: &[EngineWindow]) -> BTreeMap<WindowId, WindowSizeHints> {
    observed
        .iter()
        .map(|entry| (entry.window.clone(), entry.hints))
        .collect()
}

pub(super) fn domain_is_empty(domain: &DomainLayout) -> bool {
    domain.leaves.is_empty() && domain.exceptions.is_empty() && domain.minimized_slots.is_empty()
}

fn empty_projection(base_revision: u64) -> RetainedProjectionOutcome {
    RetainedProjectionOutcome {
        base_revision,
        geometry: Vec::new(),
        focus_leaf: None,
    }
}

fn partial_observation() -> FreshDomainFailure {
    FreshDomainFailure::Layout(LayoutFailure::Semantics(RefusalKind::PartialObservation))
}

/// Legacy seed-target placement: the resolving focused leaf's projected rect,
/// or the whole domain bounds for an empty tree. Mirrors `seed_target_bounds`.
fn seed_placement(domain: &DomainLayout) -> Rect {
    let eligible = domain
        .focused_leaf
        .as_ref()
        .filter(|leaf| domain_focus_resolves(domain, leaf))
        .cloned();
    admission_placement_for(
        domain.tree.as_ref(),
        domain.bounds,
        domain.gap,
        eligible.as_ref(),
    )
}

/// Project one convergence candidate for a mixed/serial anchor tail. The
/// caller supplies the exact focus the source would pass (the retained
/// `(focus_domain, focus_leaf)` pair, or `None` for a floored exception anchor)
/// and any capacity float target. `None` means the projector refused; the
/// caller decides whether that falls through or becomes a seed failure.
fn project_anchor_result(
    key: &DomainKey,
    domain: &OutputDomain,
    focused: &DomainLayout,
    observed: &[EngineWindow],
    focus_leaf: Option<NodeId>,
    native_capacity_float: Option<WindowId>,
    kind: FreshOutcomeKind,
) -> Option<FreshDomainOutcome> {
    let hints = fresh_hints(observed);
    let geometry =
        project_retained_candidate(key, focused, domain.bounds, domain.gap, &hints, observed)?;
    Some(FreshDomainOutcome {
        projection: RetainedProjectionOutcome {
            base_revision: focused.accepted_revision,
            geometry,
            focus_leaf,
        },
        native_capacity_float,
        kind,
    })
}
