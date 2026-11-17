//! Typed whole-workspace migration over [`WorldLayout`].
//!
//! Partitions carried observations into migrating (`!sticky`) and sticky
//! subsets, converges both independently from the same retained source and
//! `admission_seq_next` baseline, relocates the migrating candidate to the
//! target output with the source's carried outer gap, gates non-empty tiled
//! targets on hinted client-clamp projection, and installs both-or-neither
//! only after every gate passes.

use std::collections::{BTreeMap, BTreeSet};

use crate::bounds::is_gap;
use crate::directional::WindowId;
use crate::seed::{EngineWindow, observed_window_from_engine};
use crate::session::{OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::adoption::domain_is_empty;
use super::outcome::{LayoutFailure, WorkspaceMigrationFailure, WorkspaceMigrationOutcome};
use super::projection::project_retained_candidate;
use super::relocation::build_relocated_candidate;
use super::state::WorldLayout;

impl WorldLayout {
    /// Migrate one workspace domain from `source_domain` to an unoccupied
    /// `target_domain` on another output.
    ///
    /// Preserves exact refusal priority and atomicity: any failure leaves
    /// `domains` and `admission_seq_next` untouched while preserving
    /// generation-shared group allocator burns from convergence trials.
    pub(crate) fn migrate_workspace(
        &mut self,
        source_domain: &OutputDomain,
        source_outer_gap: i32,
        focused_window: &WindowId,
        observed: &[EngineWindow],
        target_domain: &OutputDomain,
    ) -> Result<WorkspaceMigrationOutcome, WorkspaceMigrationFailure> {
        let source_key = source_domain.key();
        let target_key = target_domain.key();
        if target_key.output == source_key.output {
            return Err(WorkspaceMigrationFailure::SameOutput);
        }
        for entry in observed {
            if entry.fit_excluded
                && !entry.floating
                && !entry.sticky
                && !entry.fullscreen
                && !entry.maximized
            {
                return Err(WorkspaceMigrationFailure::OverlayPresent);
            }
        }
        if self.domains.contains_key(&target_key) {
            return Err(LayoutFailure::Semantics(RefusalKind::Unchanged).into());
        }
        if !is_gap(source_outer_gap) {
            return Err(WorkspaceMigrationFailure::DomainMismatch);
        }

        let Some(original) = self.domain(&source_key).cloned() else {
            if observed.is_empty() && focused_window.0.is_empty() {
                return Ok(WorkspaceMigrationOutcome {
                    members: Vec::new(),
                    floats: 0,
                });
            }
            return Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain).into());
        };

        if original.bounds != source_domain.bounds
            || original.gap != source_domain.gap
            || original.outer_gap != source_outer_gap
        {
            return Err(WorkspaceMigrationFailure::DomainMismatch);
        }
        for entry in observed {
            if entry.output != source_key.output || entry.workspace != source_key.workspace {
                return Err(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch).into());
            }
        }
        let all_retained_observed = original
            .leaves
            .keys()
            .chain(original.exceptions.keys())
            .all(|id| observed.iter().any(|entry| &entry.window == id));
        if !all_retained_observed {
            return Err(LayoutFailure::Semantics(RefusalKind::PartialObservation).into());
        }
        if !focused_window.0.is_empty()
            && !observed.iter().any(|entry| &entry.window == focused_window)
        {
            return Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch).into());
        }
        if domain_is_empty(&original) && observed.is_empty() {
            self.domains.remove(&source_key);
            return Ok(WorkspaceMigrationOutcome {
                members: Vec::new(),
                floats: 0,
            });
        }

        let active = (!focused_window.0.is_empty()).then_some(focused_window);
        let mut moved_obs = Vec::new();
        let mut stay_obs = Vec::new();
        let mut moved_has_active = false;
        let mut stay_has_active = false;
        for entry in observed {
            let obs = observed_window_from_engine(entry);
            if entry.sticky {
                if active.is_some_and(|w| w == &entry.window) {
                    stay_has_active = true;
                }
                stay_obs.push(obs);
            } else {
                if active.is_some_and(|w| w == &entry.window) {
                    moved_has_active = true;
                }
                moved_obs.push(obs);
            }
        }
        let moved_focus = moved_has_active.then_some(focused_window);
        let stay_focus = stay_has_active.then_some(focused_window);

        let empty_hidden = BTreeSet::new();
        let (moved_converged, moved_counter, _) = self.converge_candidate(
            &source_key,
            &original,
            &moved_obs,
            moved_focus,
            &empty_hidden,
        )?;
        let stay = if stay_obs.is_empty() {
            None
        } else {
            let (stay_converged, stay_counter, _) = self.converge_candidate(
                &source_key,
                &original,
                &stay_obs,
                stay_focus,
                &empty_hidden,
            )?;
            Some((stay_converged, stay_counter))
        };

        if target_domain.bounds.w <= 0 || target_domain.bounds.h <= 0 || !is_gap(target_domain.gap)
        {
            return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology).into());
        }
        let Some(moved) = build_relocated_candidate(
            &moved_converged,
            &target_key,
            target_domain.bounds,
            target_domain.gap,
            source_outer_gap,
        ) else {
            return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology).into());
        };

        if !moved.leaves.is_empty() {
            let hints: BTreeMap<WindowId, WindowSizeHints> = observed
                .iter()
                .map(|entry| (entry.window.clone(), entry.hints))
                .collect();
            if project_retained_candidate(
                &target_key,
                &moved,
                target_domain.bounds,
                target_domain.gap,
                &hints,
                observed,
            )
            .is_none()
            {
                return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology).into());
            }
        }

        let mut next_counter = self.admission_seq_next.max(moved_counter);
        if let Some((_, stay_counter)) = &stay {
            next_counter = next_counter.max(*stay_counter);
        }
        let stay = stay.and_then(|(stay, _)| (!domain_is_empty(&stay)).then_some(stay));
        let moved_empty = domain_is_empty(&moved);

        let mut members: Vec<WindowId> = moved
            .leaves
            .keys()
            .chain(moved.exceptions.keys())
            .cloned()
            .collect();
        members.sort();
        let floats = moved.exceptions.len();

        self.admission_seq_next = next_counter;
        self.domains.remove(&source_key);
        if !moved_empty {
            self.domains.insert(target_key, moved);
        }
        if let Some(stay) = stay {
            self.domains.insert(source_key, stay);
        }

        Ok(WorkspaceMigrationOutcome { members, floats })
    }
}
