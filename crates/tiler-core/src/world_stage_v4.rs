//! Parallel core-internal v4 staging intake (unit 6 sub-increment 3).
//!
//! [`WorldEngine::stage_facts_v4`] fills the existing [`StagedReport`] from a
//! [`WorldReportV4`](crate::world_report::WorldReportV4) without going through any legacy report shape. The
//! per-window classification loop, held-window scope resolution, and
//! unsupported state-attempt scope are the shared helpers in
//! [`crate::world_observe`] and [`crate::world_state`], so the staging helpers and v4 intake
//! cannot drift. Nothing calls this from [`WorldEngine::handle_report`];
//! the runtime is version 4.
//!
//! The v4 intake stages screens, held screens, resolved held windows,
//! windows/closes, the gesture barrier, held native rows, held attempt
//! rows, the verbatim presentation row, and the verbatim renderer input.
//! It stages no echoes (v4 carries no views). Capability conflicts grade to
//! narrow `conflicting-fact` holds before staging; unsupported state attempts
//! keep their raw rows and emit `state_offroute` attempt diagnostics.

use crate::world_classification::{
    StageClassificationContext, StageClassificationInput, StateAttemptSource, classify_stage,
};
use crate::world_engine::{StagedObservation, StagedReport, WorldEngine};
use crate::world_report::{
    PresentationRowV4, RendererInput, RowDiagnostic, StateProperty, WorldReportV4,
};
use crate::world_report_v4_checks::graded_effective_v4;
use crate::world_state::state_property_supported;
use crate::world_transaction::WorldTransaction;

/// Read-only staged census for protocol tests: counts and verbatim rows
/// staged from a [`WorldReportV4`] without mutating the engine.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedV4Census {
    pub screens: usize,
    pub held_screens: usize,
    pub facts: usize,
    pub excluded: usize,
    pub held_windows: usize,
    pub closes: usize,
    pub held_native: usize,
    pub held_attempts: usize,
    pub presentation_held: bool,
    pub renderer_input: Option<RendererInput>,
    pub echoes: usize,
    pub diagnostics: Vec<RowDiagnostic>,
}

impl WorldTransaction<'_> {
    /// Stage a v4 report into [`StagedReport`]: the raw observation capture
    /// from the original typed rows, capability-graded windows and explicit
    /// closes via the shared helpers, the gesture barrier, held native rows,
    /// held attempt rows, the verbatim presentation row, and the verbatim
    /// renderer input. Echoes stay empty (v4 carries no views). The capture
    /// precedes grading so the observation retains the true original facts
    /// while classification consumes the effective rows. Initial intake must
    /// ingest intent from the raw capture before classification; this method
    /// never ingests, and v4 accepted-run assembly is not wired yet.
    /// Diagnostics follow report lane order:
    /// held screens, resolved held windows (including newly graded capability
    /// conflicts), held native rows, held attempt rows carrying `Some`
    /// diagnostic (rows with `None` emit nothing and never get a fabricated
    /// scope), then unsupported state-attempt `ConflictingFact` diagnostics
    /// in input order (raw attempts stay intact, never a held row), then the
    /// held presentation's diagnostic when it carries `Some`.
    #[cfg(test)]
    pub(crate) fn stage_facts_v4(&mut self, report: &WorldReportV4) {
        // Classification-only restage from the original typed rows:
        // invalid and capability grading may move rows into held rows
        // while the observation keeps the original facts. Like the legacy
        // restage, this never ingests intent.
        let observation = WorldEngine::capture_observation_v4(report);
        let unavailable: &[crate::connection::FactName] = self
            .connection
            .capabilities()
            .map(|capabilities| capabilities.unavailable_facts.as_slice())
            .unwrap_or(&[]);
        let native_sticky_supported =
            state_property_supported(self.connection.capabilities(), StateProperty::Sticky);
        let maximize_supported =
            state_property_supported(self.connection.capabilities(), StateProperty::Maximized);
        let graded = graded_effective_v4(
            report,
            unavailable,
            native_sticky_supported,
            maximize_supported,
        );
        let effective = graded.as_ref().unwrap_or(report);
        self.stage_captured_v4(effective, observation);
    }

    /// Stage v4 classification from an already-captured raw observation
    /// and the capability-graded effective rows: held scope resolution
    /// against prior staged homing, lane-ordered diagnostics, verbatim
    /// v4-only rows, and the shared per-window classification loop. The
    /// raw capture moves into the new staged view unchanged. Takes no
    /// ledger action; callers own exactly-once ingestion.
    pub(crate) fn stage_captured_v4(
        &mut self,
        effective: &WorldReportV4,
        mut observation: StagedObservation,
    ) {
        let derived = {
            let ctx = StageClassificationContext::new(self.connection, self.world);
            let input = StageClassificationInput {
                windows: &effective.windows,
                closed: &effective.closed,
                held_screens: &effective.held_screens,
                held_windows: &effective.held_windows,
                held_native: &effective.held_native,
                held_attempts: &effective.held_attempts,
                presentation: Some(&effective.presentation),
                state_attempts: StateAttemptSource::Report(&effective.state_attempts),
            };
            classify_stage(&ctx, input)
        };
        let mut held_windows = effective.held_windows.clone();
        for (held, diag) in held_windows.iter_mut().zip(derived.held_window_diagnostics) {
            held.diagnostic = diag;
        }
        observation.normalize_visibility_attempts(
            &effective.visibility_attempts,
            &derived.facts,
            &held_windows,
        );
        let staged = StagedReport {
            observation: std::sync::Arc::new(observation),
            screens: effective.screens.clone(),
            held_screens: effective.held_screens.clone(),
            held_windows,
            diagnostics: derived.diagnostics,
            gesture_barrier: effective.gesture_barrier,
            held_native: effective.held_native.clone(),
            held_attempts: effective.held_attempts.clone(),
            presentation_v4: Some(effective.presentation.clone()),
            renderer_input: effective.renderer_input.clone(),
            effective_windows: std::sync::Arc::new(effective.windows.clone()),
            excluded: derived.excluded,
            fact_output: derived.fact_output,
            facts: derived.facts,
            derived: derived.derived,
            closes: derived.closes,
            ..StagedReport::default()
        };
        self.world.staged = staged;
    }
}

impl WorldEngine {
    /// Stage `report` on the shared read-only classifier and read back the
    /// census: `self` is not mutated and `self.world` is not cloned.
    /// Protocol tests use this to check conservation over every accepted v4
    /// report fixture and scenario report without running the engine.
    #[doc(hidden)]
    pub fn stage_report_v4_census(&self, report: &WorldReportV4) -> StagedV4Census {
        let unavailable: &[crate::connection::FactName] = self
            .connection
            .capabilities()
            .map(|capabilities| capabilities.unavailable_facts.as_slice())
            .unwrap_or(&[]);
        let native_sticky_supported =
            state_property_supported(self.connection.capabilities(), StateProperty::Sticky);
        let maximize_supported =
            state_property_supported(self.connection.capabilities(), StateProperty::Maximized);
        let graded = graded_effective_v4(
            report,
            unavailable,
            native_sticky_supported,
            maximize_supported,
        );
        let effective = graded.as_ref().unwrap_or(report);
        let ctx = StageClassificationContext::new(&self.connection, &self.world);
        let input = StageClassificationInput {
            windows: &effective.windows,
            closed: &effective.closed,
            held_screens: &effective.held_screens,
            held_windows: &effective.held_windows,
            held_native: &effective.held_native,
            held_attempts: &effective.held_attempts,
            presentation: Some(&effective.presentation),
            state_attempts: StateAttemptSource::Report(&effective.state_attempts),
        };
        let derived = classify_stage(&ctx, input);
        StagedV4Census {
            screens: effective.screens.len(),
            held_screens: effective.held_screens.len(),
            facts: derived.facts.len(),
            excluded: derived.excluded.len(),
            held_windows: effective.held_windows.len(),
            closes: derived.closes.len(),
            held_native: effective.held_native.len(),
            held_attempts: effective.held_attempts.len(),
            presentation_held: matches!(effective.presentation, PresentationRowV4::Held(_)),
            renderer_input: effective.renderer_input.clone(),
            echoes: 0,
            diagnostics: derived.diagnostics,
        }
    }
}
