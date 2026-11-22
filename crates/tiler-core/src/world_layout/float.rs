//! Typed synchronous intentional float/unfloat over `WorldLayout`.
//!
//! The caller owns raw command decoding, string validation, convergence and
//! fresh-domain routing. This operation acts on one retained domain candidate,
//! validates topology, command/observed shapes, homing and binding semantics in
//! source order, and installs the resulting domain atomically. No correlation,
//! identity, capability or pending-plan state lives here.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::session::{DomainKey, ExceptionFlags, ObservedWindow, RefusalKind};
use crate::size_hints::{FixedSizePredicate, WindowSizeHints};

use super::focus::{domain_focus_resolves, settle_domain_focus_tail};
use super::observation::{domain_observed_known_match, valid_observed_shapes};
use super::outcome::{FloatOutcome, LayoutFailure};
use super::projection::project_borrowed_geometry;
use super::state::{DomainException, DomainLayout, WorldLayout};
use super::topology::{collect_leaves, validate_domain_topology};
use super::tree_edit::{admission_placement_for, remove_leaf_from_tree};

impl WorldLayout {
    pub(crate) fn is_ordinary_float(&self, key: &DomainKey, subject: &WindowId) -> bool {
        self.domain(key)
            .and_then(|domain| domain.exceptions.get(subject))
            .is_some_and(|record| record.flags == floating_exception_flags())
    }

    /// Toggle intentional float/unfloat for `subject` in the converged domain
    /// `key`.
    ///
    /// - When `subject` is currently an ordinary floating exception in `key`,
    ///   unfloats it back into tiling via [`Self::build_admission_candidate`]
    ///   with `suppress_fixed_float = true` and `allow_fullscreen_subject = true`,
    ///   recording any valid `float_geometry` as durable retained float
    ///   geometry. Capacity no-space keeps the window floated with no tile
    ///   override or frame authority (`float_window: Some(subject)`,
    ///   `float_rect: None`).
    /// - When `subject` is currently tiled in `key`, removes it from the tree,
    ///   records an ordinary floating exception with effective placement
    ///   (`float_geometry` > valid retained float geometry > centered 60%
    ///   bounds), clears any automatic-fixed or tile-override markers, and
    ///   settles focus and provenance.
    ///
    /// Observed focus is intentionally not synchronized here. On refusal,
    /// retained domain state is untouched while any generation-shared
    /// group-allocator burns from unfloat admission trials survive.
    #[cfg(test)]
    pub(crate) fn toggle_float(
        &mut self,
        key: &DomainKey,
        subject: &WindowId,
        float_geometry: Option<Rect>,
        observed: &[ObservedWindow],
    ) -> Result<FloatOutcome, LayoutFailure> {
        let (candidate, new_counter, outcome) = self.toggle_float_on_candidate(
            key,
            self.domain(key),
            self.admission_seq_next,
            subject,
            float_geometry,
            observed,
            self.config.fixed_size_admission,
            self.config.fixed_size_predicate,
        )?;
        *self.domains.get_mut(key).expect("domain validated above") = candidate;
        self.admission_seq_next = new_counter;
        Ok(outcome)
    }

    /// Build a staged float/unfloat candidate without installing it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn toggle_float_on_candidate(
        &self,
        key: &DomainKey,
        original: Option<&DomainLayout>,
        admission_counter: u64,
        subject: &WindowId,
        float_geometry: Option<Rect>,
        observed: &[ObservedWindow],
        fixed_admission: bool,
        fixed_predicate: FixedSizePredicate,
    ) -> Result<(DomainLayout, u64, FloatOutcome), LayoutFailure> {
        let refuse = LayoutFailure::Semantics;

        if let Some(existing) = original
            && !validate_domain_topology(key, existing)
        {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        if subject.0.is_empty() || !float_geometry.is_none_or(valid_rect_shape) {
            return Err(refuse(RefusalKind::MalformedInput));
        }

        if !valid_observed_shapes(observed) {
            return Err(refuse(RefusalKind::MalformedInput));
        }

        for entry in observed {
            if entry.output != key.output || entry.workspace != key.workspace {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
        }

        let Some(original) = original else {
            if !observed.is_empty() {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
            return Err(refuse(RefusalKind::PartialObservation));
        };

        if let Some(record) = original.exceptions.get(subject) {
            if record.flags != floating_exception_flags() {
                return Err(refuse(RefusalKind::NotTiled));
            }
            let target = observed
                .iter()
                .find(|entry| &entry.window == subject)
                .ok_or_else(|| refuse(RefusalKind::PartialObservation))?;

            let bindings_match = if fixed_admission {
                target.floating == record.flags.floating
            } else {
                let exact = target.flags() == record.flags;
                let fullscreen_tolerance =
                    target.floating && target.fullscreen && !target.maximized && !target.sticky;
                exact || fullscreen_tolerance
            };
            if !bindings_match || target.output != key.output || target.workspace != key.workspace {
                return Err(refuse(RefusalKind::PartialObservation));
            }

            let retained = match float_geometry {
                Some(rect) => Some(rect),
                None => record
                    .floating_geometry
                    .or_else(|| original.retained_float_geometry.get(subject).copied()),
            };

            if target.maximized {
                return Err(refuse(RefusalKind::PartialObservation));
            }

            let mut candidate = original.clone();
            candidate.exceptions.remove(subject);
            if let Some(rect) = retained {
                candidate
                    .retained_float_geometry
                    .insert(subject.clone(), rect);
            }

            let mut next_observed = observed.to_vec();
            if let Some(entry) = next_observed
                .iter_mut()
                .find(|entry| &entry.window == subject)
            {
                entry.floating = false;
            }

            let placement = admission_placement_for(
                candidate.tree.as_ref(),
                candidate.bounds,
                candidate.gap,
                candidate.focused_leaf.as_ref(),
            );

            let (installed_candidate, new_counter, admission) = self.build_admission_candidate(
                key,
                Some(&candidate),
                admission_counter,
                subject,
                placement,
                true,
                true,
                &next_observed,
                fixed_admission,
                fixed_predicate,
            )?;

            let focus_leaf = installed_candidate.focused_leaf.clone();
            let next_counter = if admission.admitted_leaf.is_some() {
                new_counter
            } else {
                admission_counter
            };

            let float_window = admission.admitted_leaf.is_none().then(|| subject.clone());

            return Ok((
                installed_candidate,
                next_counter,
                FloatOutcome {
                    base_revision: admission.base_revision,
                    revision: admission.revision,
                    geometry: admission.geometry,
                    focus_leaf,
                    float_window,
                    float_rect: None,
                },
            ));
        }

        let target = observed
            .iter()
            .find(|entry| &entry.window == subject)
            .ok_or_else(|| refuse(RefusalKind::PartialObservation))?;

        if target.floating
            || target.maximized
            || target.sticky
            || !original.leaves.contains_key(subject)
        {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let effective = match float_geometry {
            Some(rect) => rect,
            None => match original.retained_float_geometry.get(subject) {
                Some(rect) if valid_rect_shape(*rect) => *rect,
                _ => centered_float_rect(original.bounds),
            },
        };

        let known: BTreeSet<&WindowId> = original
            .leaves
            .keys()
            .chain(original.exceptions.keys())
            .collect();
        let observed_ids: BTreeSet<&WindowId> = observed.iter().map(|w| &w.window).collect();
        if observed_ids != known {
            return Err(refuse(RefusalKind::PartialObservation));
        }

        let mut adjusted_observed = observed.to_vec();
        let Some(subject_entry) = adjusted_observed
            .iter_mut()
            .find(|entry| &entry.window == subject)
        else {
            return Err(refuse(RefusalKind::PartialObservation));
        };
        subject_entry.fullscreen = false;
        if !domain_observed_known_match(key, original, &adjusted_observed, fixed_admission) {
            return Err(refuse(RefusalKind::PartialObservation));
        }

        let mut candidate = original.clone();
        let leaf = candidate
            .leaves
            .remove(subject)
            .expect("tiled subject checked above");
        let target_tree = candidate.tree.take();
        candidate.tree = remove_leaf_from_tree(self.policy.as_ref(), target_tree, &leaf);

        let was_focused = original.focused_leaf.as_ref() == Some(&leaf);
        let desired_focus_leaf = if was_focused {
            let stack_fallback = original
                .focus_stack
                .iter()
                .rev()
                .find(|l| domain_focus_resolves(&candidate, l))
                .cloned();
            match stack_fallback {
                Some(next) => Some(next),
                None => candidate.tree.as_ref().and_then(|tree| {
                    collect_leaves(tree)
                        .into_iter()
                        .find(|l| domain_focus_resolves(&candidate, l))
                }),
            }
        } else if original
            .focused_leaf
            .as_ref()
            .is_some_and(|l| domain_focus_resolves(&candidate, l))
        {
            original.focused_leaf.clone()
        } else {
            None
        };
        candidate.focused_leaf = desired_focus_leaf.clone();

        if !validate_domain_topology(key, &candidate) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if candidate.tree == original.tree
            && candidate.leaves == original.leaves
            && candidate.focused_leaf == original.focused_leaf
        {
            return Err(refuse(RefusalKind::Unchanged));
        }

        let hints: BTreeMap<WindowId, WindowSizeHints> = observed
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect();
        let desired_geometry = match candidate.tree.as_ref() {
            Some(tree) => project_borrowed_geometry(
                key,
                tree,
                &candidate.leaves,
                candidate.bounds,
                candidate.gap,
                &hints,
            )
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?,
            None => Vec::new(),
        };

        let base = original.accepted_revision;
        if base >= MAX_REVISION {
            return Err(LayoutFailure::RevisionLimit {
                base,
                limit: MAX_REVISION,
            });
        }
        let revision = base + 1;

        candidate.automatic_fixed.remove(subject);
        candidate.fixed_tile_override.remove(subject);
        candidate.exceptions.insert(
            subject.clone(),
            DomainException {
                flags: floating_exception_flags(),
                floating_geometry: Some(effective),
            },
        );
        candidate
            .retained_float_geometry
            .insert(subject.clone(), effective);

        let stack_base = original.focus_stack.clone();
        let current_last_active = original.last_active.clone();
        settle_domain_focus_tail(
            &mut candidate,
            &stack_base,
            desired_focus_leaf.clone(),
            current_last_active.as_ref(),
        );
        WorldLayout::prune_domain_fixed_size_state(&mut candidate);
        WorldLayout::prune_domain_capacity_state(&mut candidate);
        candidate.accepted_revision = revision;

        Ok((
            candidate,
            admission_counter,
            FloatOutcome {
                base_revision: base,
                revision,
                geometry: desired_geometry,
                focus_leaf: desired_focus_leaf,
                float_window: Some(subject.clone()),
                float_rect: Some(effective),
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

/// Centered 60% work-area rectangle, matching the adapter's previous static
/// placement: `max(1, floor(0.6 * bounds))` size centered on the bounds.
/// Shared with the sticky-on transition so tiled-origin sticky takes the
/// same intentional-float placement as the ordinary float toggle.
pub(crate) fn centered_float_rect(bounds: Rect) -> Rect {
    let w = ((i64::from(bounds.w) * 6) / 10).max(1) as i32;
    let h = ((i64::from(bounds.h) * 6) / 10).max(1) as i32;
    Rect {
        x: bounds.x + (bounds.w - w) / 2,
        y: bounds.y + (bounds.h - h) / 2,
        w,
        h,
    }
}
