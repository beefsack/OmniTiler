//! Typed synchronous single-domain window admission over `WorldLayout`.
//!
//! The caller owns raw command decoding and convergence/seeding/focus sync.
//! This operation takes a validated, converged domain and a complete
//! observation, clones the local domain, validates the staged candidate and
//! installs it atomically. A refusal leaves retained state untouched except
//! the generation-shared allocator, whose group serials burn on trial. No
//! handshake, capability, pending-plan, identity or fingerprint state lives
//! here.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::session::{DesiredGeometry, DomainKey, ExceptionFlags, ObservedWindow, RefusalKind};
use crate::size_hints::{FixedSizePredicate, WindowSizeHints, is_fixed_size_with};

use super::admission::plan_admission_leg;
use super::focus::{domain_focus_resolves, settle_domain_focus_tail};
use super::observation::{domain_observed_known_match, valid_observed_shapes};
use super::outcome::{AdmissionOutcome, LayoutFailure};
use super::projection::project_borrowed_geometry;
use super::state::{DomainException, DomainLayout, WorldLayout};
use super::topology::{collect_node_ids, validate_domain_topology};

#[cfg(test)]
#[path = "admit_tests.rs"]
mod tests;

impl WorldLayout {
    /// Admit `window` into the converged domain `key` from a complete
    /// observation. `observed` must cover exactly the known tiled-plus-
    /// exception set plus the admitted window.
    ///
    /// `suppress_fixed_float` carries an explicit user tile win, and
    /// `allow_fullscreen_subject` tolerates a fullscreen-only subject for an
    /// explicit unfloat. On success the per-domain accepted revision advances
    /// by exactly one and no native acceptance is claimed.
    #[cfg(test)]
    pub(crate) fn admit_window(
        &mut self,
        key: &DomainKey,
        window: &WindowId,
        placement_bounds: Rect,
        suppress_fixed_float: bool,
        allow_fullscreen_subject: bool,
        observed: &[ObservedWindow],
    ) -> Result<AdmissionOutcome, LayoutFailure> {
        let (candidate, new_counter, outcome) = self.build_admission_candidate(
            key,
            self.domain(key),
            self.admission_seq_next,
            window,
            placement_bounds,
            suppress_fixed_float,
            allow_fullscreen_subject,
            observed,
            self.config.fixed_size_admission,
            self.config.fixed_size_predicate,
        )?;

        *self.domains.get_mut(key).expect("domain validated above") = candidate;
        if outcome.admitted_leaf.is_some() {
            self.admission_seq_next = new_counter;
        }

        Ok(outcome)
    }

    /// Test-only state seeder for preexisting deferred exceptions: inserts
    /// exact `flags` with no floating geometry, leaves tree/leaves/focus/
    /// admission counter untouched, and advances `accepted_revision` by one.
    #[cfg(test)]
    pub(crate) fn seed_exception_for_test(
        &mut self,
        key: &DomainKey,
        window: &WindowId,
        flags: ExceptionFlags,
    ) {
        let domain = self
            .domains
            .get_mut(key)
            .expect("domain must exist before seeding exception");
        domain.exceptions.insert(
            window.clone(),
            DomainException {
                flags,
                floating_geometry: None,
            },
        );
        domain.accepted_revision += 1;
    }

    /// Build a staged admission candidate without installing it. The caller
    /// supplies the `original` domain view (absent for a missing target) and a
    /// local `admission_counter` seed, which may differ from the retained
    /// high-water for paired or freshly seeded admission paths. `fixed_admission`
    /// and `fixed_predicate` are passed explicitly instead of being read from
    /// shared config so a legacy seeded final leg can use the old session's
    /// default (false/default) policy without mutating this layout. All
    /// validation, geometry, allocator burns and the focus/provenance commit
    /// tail run here; only the caller installs the returned candidate and
    /// counter.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_admission_candidate(
        &self,
        key: &DomainKey,
        original: Option<&DomainLayout>,
        admission_counter: u64,
        window: &WindowId,
        placement_bounds: Rect,
        suppress_fixed_float: bool,
        allow_fullscreen_subject: bool,
        observed: &[ObservedWindow],
        fixed_admission: bool,
        fixed_predicate: FixedSizePredicate,
    ) -> Result<(DomainLayout, u64, AdmissionOutcome), LayoutFailure> {
        let refuse = LayoutFailure::Semantics;

        // Affected-domain pretopology is checked first only when a domain is
        // already retained; no other domain is read. The lookup stays optional
        // so a missing domain cannot jump ahead of the shape gates below.
        if let Some(existing) = original
            && !validate_domain_topology(key, existing)
        {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        // All command shapes before scope, duplicate or completeness, matching
        // the old valid_command_shapes gate (nonempty window/output/workspace
        // and a valid placement rectangle).
        if window.0.is_empty()
            || key.output.0.is_empty()
            || key.workspace.0.is_empty()
            || !valid_rect_shape(placement_bounds)
        {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        if !valid_observed_shapes(observed) {
            return Err(refuse(RefusalKind::MalformedInput));
        }

        // Unknown valid-shaped target, then observed homing scope, both before
        // duplicate and completeness checks.
        let Some(original) = original else {
            return Err(refuse(RefusalKind::CrossDomainMismatch));
        };
        for observed_entry in observed {
            if observed_entry.output != key.output || observed_entry.workspace != key.workspace {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
        }

        // Duplicate known window before completeness.
        if original.leaves.contains_key(window) || original.exceptions.contains_key(window) {
            return Err(refuse(RefusalKind::DuplicateWindow));
        }

        let mut known: BTreeSet<&WindowId> = original
            .leaves
            .keys()
            .chain(original.exceptions.keys())
            .collect();
        known.insert(window);
        let observed_ids: BTreeSet<&WindowId> = observed.iter().map(|w| &w.window).collect();
        if observed_ids != known {
            return Err(refuse(RefusalKind::PartialObservation));
        }
        let Some(entry) = observed.iter().find(|w| &w.window == window) else {
            return Err(refuse(RefusalKind::PartialObservation));
        };

        // Subject homing and full-flag binding. A maximized-only birth rides as
        // an ordinary tile; a fullscreen-only subject is tolerated only for an
        // explicit unfloat.
        let entry_flags = entry.flags();
        let max_only_tile_carrier = entry_flags
            == ExceptionFlags {
                floating: false,
                fullscreen: false,
                maximized: true,
                sticky: false,
            };
        let observed_binding = if max_only_tile_carrier {
            ExceptionFlags {
                maximized: false,
                ..entry_flags
            }
        } else {
            entry_flags
        };
        let fullscreen_only_subject = entry_flags
            == ExceptionFlags {
                floating: false,
                fullscreen: true,
                maximized: false,
                sticky: false,
            };
        let binding_holds =
            !observed_binding.any() || (allow_fullscreen_subject && fullscreen_only_subject);
        if !binding_holds {
            return Err(refuse(RefusalKind::PartialObservation));
        }

        // Known bindings match retained state. The subject is not yet known,
        // so no admit_new exclusion is needed at this boundary.
        if !domain_observed_known_match(key, original, observed, fixed_admission) {
            return Err(refuse(RefusalKind::PartialObservation));
        }

        let base = original.accepted_revision;
        let original_stack = original.focus_stack.clone();
        let original_last_active = original.last_active.clone();
        let mut candidate = original.clone();

        let policy = self.policy.as_ref();
        let hints: BTreeMap<WindowId, WindowSizeHints> = observed
            .iter()
            .map(|observed_entry| (observed_entry.window.clone(), observed_entry.hints))
            .collect();

        let mut admitted_leaf: Option<NodeId> = None;
        let mut geometry: Vec<DesiredGeometry> = Vec::new();
        let mut new_counter = admission_counter;

        let override_wins = candidate.fixed_tile_override.contains(window) || suppress_fixed_float;
        if fixed_admission
            && !override_wins
            && !entry.floating
            && !entry.fullscreen
            && !entry.sticky
            && !entry.fixed_suppress
            && is_fixed_size_with(entry.hints, fixed_predicate)
        {
            // Automatic fixed membership float: no tile, no geometry, no
            // focus change and no admission sequence number.
            candidate.exceptions.insert(
                window.clone(),
                DomainException {
                    flags: floating_exception_flags(),
                    floating_geometry: None,
                },
            );
            candidate.automatic_fixed.insert(window.clone());
        } else {
            // Normal tiled admission under best effort, fail-closed on
            // overlays except the two tolerated carriers above.
            if entry.floating
                || entry.sticky
                || (entry.fullscreen && !(allow_fullscreen_subject && fullscreen_only_subject))
            {
                return Err(refuse(RefusalKind::PartialObservation));
            }

            let orientation = policy.admission_axis_for_rect(&placement_bounds);
            let mut node_ids = BTreeSet::new();
            if let Some(tree) = candidate.tree.as_ref() {
                collect_node_ids(tree, &mut node_ids);
            }
            let eligible = candidate
                .focused_leaf
                .as_ref()
                .filter(|leaf| domain_focus_resolves(&candidate, leaf))
                .cloned();
            let leaf_links: BTreeMap<NodeId, WindowId> = candidate
                .leaves
                .iter()
                .map(|(bound_window, leaf)| (leaf.clone(), bound_window.clone()))
                .collect();
            let Some(leg) = plan_admission_leg(
                policy,
                &self.group_alloc,
                candidate.tree.clone(),
                candidate.bounds,
                candidate.gap,
                eligible,
                orientation,
                window,
                &mut node_ids,
                &hints,
                &leaf_links,
            ) else {
                return Err(refuse(RefusalKind::MalformedTopology));
            };
            // A missing ordinary tree with a failed selection is allocator
            // exhaustion or anchor mismatch, never an empty-domain mint.
            if leg.ordinary_tree.is_none() && leg.selected.is_err() {
                return Err(refuse(RefusalKind::MalformedTopology));
            }

            match leg.selected {
                Ok(target) => {
                    candidate.tree = Some(target.tree);
                    candidate.leaves.insert(window.clone(), leg.leaf_id.clone());
                    if !validate_domain_topology(key, &candidate) {
                        return Err(refuse(RefusalKind::MalformedTopology));
                    }
                    let mut projected = project_borrowed_geometry(
                        key,
                        candidate.tree.as_ref().expect("tree set above"),
                        &candidate.leaves,
                        candidate.bounds,
                        candidate.gap,
                        &hints,
                    )
                    .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
                    if projected.is_empty() {
                        return Err(refuse(RefusalKind::MalformedTopology));
                    }
                    if allow_fullscreen_subject && fullscreen_only_subject {
                        projected.retain(|entry| &entry.window != window);
                    }

                    // An explicit suppress origin wins a tiled slot and
                    // drops the automatic marker transactionally.
                    if suppress_fixed_float || (fixed_admission && entry.fixed_suppress) {
                        candidate.fixed_tile_override.insert(window.clone());
                        candidate.automatic_fixed.remove(window);
                    }
                    candidate.capacity_floats.remove(window);
                    if !candidate.admission_order.contains_key(window) {
                        candidate
                            .admission_order
                            .insert(window.clone(), new_counter);
                        new_counter = new_counter.saturating_add(1);
                    }

                    admitted_leaf = Some(leg.leaf_id);
                    geometry = projected;
                }
                Err(()) => {
                    // Capacity no-space float: slotless membership with a
                    // diagnostic capacity marker, no allocation or frame
                    // authority and no change to tiled focus or the
                    // automatic marker.
                    candidate.exceptions.insert(
                        window.clone(),
                        DomainException {
                            flags: floating_exception_flags(),
                            floating_geometry: None,
                        },
                    );
                    candidate.capacity_floats.insert(window.clone());
                    let fixed_now = is_fixed_size_with(entry.hints, fixed_predicate);
                    if !candidate.fixed_tile_override.contains(window) {
                        if (suppress_fixed_float || entry.fixed_suppress) && fixed_now {
                            candidate.fixed_tile_override.insert(window.clone());
                        } else {
                            candidate.fixed_tile_override.remove(window);
                        }
                    }
                }
            }
        }

        // Revision limit is a nonterminal refusal after every semantic and
        // geometric gate. The candidate is discarded without install while
        // allocator serials stay burned.
        if base >= MAX_REVISION {
            return Err(LayoutFailure::RevisionLimit {
                base,
                limit: MAX_REVISION,
            });
        }
        let revision = base + 1;

        // Exact verify_lifecycle commit tail: settle focus from the captured
        // original stack and last-active fallback, prune fixed-size and
        // capacity provenance, then advance this domain's accepted revision.
        let settled_focus = admitted_leaf
            .clone()
            .or_else(|| candidate.focused_leaf.clone());
        settle_domain_focus_tail(
            &mut candidate,
            &original_stack,
            settled_focus,
            original_last_active.as_ref(),
        );
        WorldLayout::prune_domain_fixed_size_state(&mut candidate);
        WorldLayout::prune_domain_capacity_state(&mut candidate);
        candidate.accepted_revision = revision;

        Ok((
            candidate,
            new_counter,
            AdmissionOutcome {
                base_revision: base,
                revision,
                admitted_leaf,
                geometry,
            },
        ))
    }
}

fn floating_exception_flags() -> ExceptionFlags {
    ExceptionFlags {
        floating: true,
        fullscreen: false,
        maximized: false,
        sticky: false,
    }
}

fn valid_rect_shape(rect: Rect) -> bool {
    rect.w > 0
        && rect.h > 0
        && rect.x.checked_add(rect.w).is_some()
        && rect.y.checked_add(rect.h).is_some()
}
