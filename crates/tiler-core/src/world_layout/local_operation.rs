//! Typed local-operation entry points composing retained lookup, unique-source
//! relocation, and legacy tiled-seed fallback over [`WorldLayout`].
//!
//! Callers own boundary command decoding and any pre-operation convergence.
//! Retained and relocated candidates run with the layout's current
//! fixed-size policy and admission high-water; legacy tiled-seed fallback
//! builds a fresh candidate with `false` / `FixedSizePredicate::default()` and
//! the seed's returned admission counter. Only successful outcomes install
//! domain state and commit `admission_seq_next`, while generation-shared group
//! allocator burns survive every refusal.

use crate::contract::ResizeMode;
use crate::directional::{Direction, SameAxisMove, WindowId};
use crate::geometry::Rect;
use crate::seed::{EngineWindow, observed_window_from_engine, order_spatial_with_focus_last};
use crate::session::{ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::FixedSizePredicate;

use super::adoption::domain_is_empty;
use super::outcome::{
    FloatOutcome, LayoutFailure, LocalFocusOutcome, LocalMoveOutcome, LocalOperationFailure,
    OrientationOutcome, PointerCornerResizeOutcome, ResizeOutcome,
};
use super::state::{DomainLayout, WorldLayout};
use super::topology::validate_domain_topology;

/// Borrowed geometry and observation context for one composed local operation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalOperationContext<'a> {
    /// Already-inset canonical component descriptor for the target domain.
    pub(crate) domain: &'a OutputDomain,
    pub(crate) outer_gap: i32,
    pub(crate) observed_focus: &'a WindowId,
    pub(crate) observed: &'a [EngineWindow],
    pub(crate) preconverged: bool,
}

impl WorldLayout {
    /// Compose retained / relocated / legacy-seed parent-axis orientation flip.
    pub(crate) fn run_toggle_orientation(
        &mut self,
        ctx: &LocalOperationContext<'_>,
        subject: &WindowId,
    ) -> Result<OrientationOutcome, LocalOperationFailure> {
        let key = ctx.domain.key();
        let semantic = observed_semantic(ctx.observed);
        self.run_local_operation(
            ctx,
            ctx.preconverged,
            |this, original, counter, fixed_admission, _| {
                this.toggle_orientation_on_candidate(
                    &key,
                    original,
                    subject,
                    ctx.observed_focus,
                    &semantic,
                    fixed_admission,
                )
                .map(|(candidate, outcome)| (candidate, counter, outcome))
            },
        )
    }

    /// Compose retained / relocated / legacy-seed intentional float/unfloat
    /// toggle, including the fresh-floating convergence branch when the target
    /// is absent, floating windows are observed, and no unique relocation
    /// source exists.
    pub(crate) fn run_toggle_float(
        &mut self,
        ctx: &LocalOperationContext<'_>,
        subject: &WindowId,
        float_geometry: Option<Rect>,
    ) -> Result<FloatOutcome, LocalOperationFailure> {
        let key = ctx.domain.key();
        let semantic = observed_semantic(ctx.observed);
        let mut converged = ctx.preconverged;

        if self.domain(&key).is_none()
            && ctx.observed.iter().any(|w| w.floating)
            && self.unique_source_key(&key).is_none()
            && ctx.domain.validate()
            && ctx.outer_gap >= 0
        {
            let focus = (!ctx.observed_focus.0.is_empty()).then_some(ctx.observed_focus);
            self.converge_fresh_domain(ctx.domain, ctx.outer_gap, &semantic, focus)?;
            converged = self.domain(&key).is_some();
        }

        self.run_local_operation(
            ctx,
            converged,
            |this, original, counter, fixed_admission, fixed_predicate| {
                this.toggle_float_on_candidate(
                    &key,
                    Some(original),
                    counter,
                    subject,
                    float_geometry,
                    &semantic,
                    fixed_admission,
                    fixed_predicate,
                )
            },
        )
    }

    /// Compose retained / relocated / legacy-seed local directional move
    /// (R1-R3).
    pub(crate) fn run_local_move(
        &mut self,
        ctx: &LocalOperationContext<'_>,
        subject: &WindowId,
        direction: Direction,
        same_axis_move: SameAxisMove,
    ) -> Result<LocalMoveOutcome, LocalOperationFailure> {
        let key = ctx.domain.key();
        let semantic = observed_semantic(ctx.observed);
        self.run_local_operation(
            ctx,
            ctx.preconverged,
            |this, original, counter, fixed_admission, _| {
                this.local_move_on_candidate(
                    &key,
                    original,
                    subject,
                    direction,
                    same_axis_move,
                    ctx.observed_focus,
                    &semantic,
                    fixed_admission,
                )
                .map(|(candidate, outcome)| (candidate, counter, outcome))
            },
        )
    }

    /// Compose retained / relocated / legacy-seed local directional focus.
    pub(crate) fn run_local_focus(
        &mut self,
        ctx: &LocalOperationContext<'_>,
        subject: &WindowId,
        direction: Direction,
    ) -> Result<LocalFocusOutcome, LocalOperationFailure> {
        let key = ctx.domain.key();
        let semantic = observed_semantic(ctx.observed);
        self.run_local_operation(
            ctx,
            ctx.preconverged,
            |this, original, counter, fixed_admission, _| {
                this.local_focus_on_candidate(
                    &key,
                    original,
                    subject,
                    direction,
                    ctx.observed_focus,
                    &semantic,
                    fixed_admission,
                )
                .map(|(candidate, outcome)| (candidate, counter, outcome))
            },
        )
    }

    /// Compose retained / relocated / legacy-seed keyboard pixel resize.
    pub(crate) fn run_keyboard_resize(
        &mut self,
        ctx: &LocalOperationContext<'_>,
        subject: &WindowId,
        direction: Direction,
        mode: ResizeMode,
        press_index: u32,
    ) -> Result<ResizeOutcome, LocalOperationFailure> {
        let key = ctx.domain.key();
        let semantic = observed_semantic(ctx.observed);
        self.run_local_operation(
            ctx,
            ctx.preconverged,
            |this, original, counter, fixed_admission, _| {
                this.keyboard_resize_on_candidate(
                    &key,
                    original,
                    subject,
                    direction,
                    mode,
                    press_index,
                    ctx.observed_focus,
                    &semantic,
                    fixed_admission,
                )
                .map(|(candidate, outcome)| (candidate, counter, outcome))
            },
        )
    }

    /// Compose retained / relocated / legacy-seed pointer edge pixel resize.
    pub(crate) fn run_pointer_resize(
        &mut self,
        ctx: &LocalOperationContext<'_>,
        subject: &WindowId,
        direction: Direction,
        proposed_boundary: i32,
    ) -> Result<ResizeOutcome, LocalOperationFailure> {
        let key = ctx.domain.key();
        let semantic = observed_semantic(ctx.observed);
        self.run_local_operation(ctx, ctx.preconverged, |this, original, counter, _, _| {
            this.pointer_resize_on_candidate(
                &key,
                original,
                subject,
                direction,
                proposed_boundary,
                ctx.observed_focus,
                &semantic,
            )
            .map(|(candidate, outcome)| (candidate, counter, outcome))
        })
    }

    /// Compose retained / relocated / legacy-seed atomic dual-axis (corner)
    /// pointer pixel resize.
    pub(crate) fn run_pointer_resize_corner(
        &mut self,
        ctx: &LocalOperationContext<'_>,
        subject: &WindowId,
        direction_1: Direction,
        boundary_1: i32,
        direction_2: Direction,
        boundary_2: i32,
    ) -> Result<PointerCornerResizeOutcome, LocalOperationFailure> {
        let key = ctx.domain.key();
        let semantic = observed_semantic(ctx.observed);
        self.run_local_operation(ctx, ctx.preconverged, |this, original, counter, _, _| {
            this.pointer_resize_corner_on_candidate(
                &key,
                original,
                subject,
                direction_1,
                boundary_1,
                direction_2,
                boundary_2,
                ctx.observed_focus,
                &semantic,
            )
            .map(|(candidate, outcome)| (candidate, counter, outcome))
        })
    }

    fn run_local_operation<O>(
        &mut self,
        ctx: &LocalOperationContext<'_>,
        converged: bool,
        plan: impl Fn(
            &Self,
            &DomainLayout,
            u64,
            bool,
            FixedSizePredicate,
        ) -> Result<(DomainLayout, u64, O), LayoutFailure>,
    ) -> Result<O, LocalOperationFailure> {
        let key = ctx.domain.key();
        let target_existed = self.domains.contains_key(&key);

        if target_existed {
            let usable = self.domains.get(&key).is_some_and(|existing| {
                validate_domain_topology(&key, existing)
                    && existing.bounds == ctx.domain.bounds
                    && existing.gap == ctx.domain.gap
                    && !domain_is_empty(existing)
            });
            if usable {
                let existing = self.domains.get(&key).expect("checked above");
                match plan(
                    self,
                    existing,
                    self.admission_seq_next,
                    self.config.fixed_size_admission,
                    self.config.fixed_size_predicate,
                ) {
                    Ok((candidate, new_counter, outcome)) => {
                        self.install_local_candidate(key, ctx.outer_gap, candidate, new_counter);
                        return Ok(outcome);
                    }
                    Err(err) if needs_rebuild(&err) => {
                        if converged {
                            return Err(LocalOperationFailure::Layout(err));
                        }
                        self.domains.remove(&key);
                    }
                    Err(err) => {
                        return Err(LocalOperationFailure::Layout(err));
                    }
                }
            } else if !converged {
                self.domains.remove(&key);
            }
        } else if let Some((source_key, moved)) =
            self.direct_relocation_candidate(&key, ctx.domain, ctx.outer_gap)
        {
            match plan(
                self,
                &moved,
                self.admission_seq_next,
                self.config.fixed_size_admission,
                self.config.fixed_size_predicate,
            ) {
                Ok((candidate, new_counter, outcome)) => {
                    self.domains.remove(&source_key);
                    self.install_local_candidate(key, ctx.outer_gap, candidate, new_counter);
                    return Ok(outcome);
                }
                Err(err) if needs_rebuild(&err) => {}
                Err(err) => {
                    return Err(LocalOperationFailure::Layout(err));
                }
            }
        }

        let Some(order) =
            order_spatial_with_focus_last(ctx.observed.to_vec(), ctx.observed_focus, false)
        else {
            return Err(LocalOperationFailure::AmbiguousPlacement);
        };
        if converged {
            return Err(LocalOperationFailure::Layout(LayoutFailure::Semantics(
                RefusalKind::PartialObservation,
            )));
        }

        let (seeded, seed_counter) = self
            .build_tiled_seed(ctx.domain, ctx.outer_gap, &order)
            .map_err(|_| LocalOperationFailure::SeedFailed)?;
        let (candidate, new_counter, outcome) = plan(
            self,
            &seeded,
            seed_counter,
            false,
            FixedSizePredicate::default(),
        )
        .map_err(LocalOperationFailure::Layout)?;
        self.install_local_candidate(key, ctx.outer_gap, candidate, new_counter);
        Ok(outcome)
    }

    fn install_local_candidate(
        &mut self,
        key: crate::session::DomainKey,
        outer_gap: i32,
        mut candidate: DomainLayout,
        new_counter: u64,
    ) {
        if domain_is_empty(&candidate) {
            self.domains.remove(&key);
            return;
        }
        candidate.outer_gap = outer_gap;
        self.domains.insert(key, candidate);
        self.admission_seq_next = new_counter;
    }
}

fn observed_semantic(observed: &[EngineWindow]) -> Vec<ObservedWindow> {
    observed.iter().map(observed_window_from_engine).collect()
}

fn needs_rebuild(error: &LayoutFailure) -> bool {
    matches!(
        error,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
            | LayoutFailure::RevisionLimit { .. }
    )
}
