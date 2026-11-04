//! Parallel core-internal v4 staging intake (unit 6 sub-increment 3).
//!
//! [`WorldEngine::stage_facts_v4`] fills the existing [`StagedReport`] from a
//! [`WorldReportV4`] without going through the legacy [`WorldReport`]. The
//! per-window classification loop, held-window scope resolution, and
//! unsupported state-attempt scope are the shared helpers in
//! [`crate::world_observe`] and [`crate::world_state`], so the legacy and v4
//! intakes cannot drift. Nothing calls this from [`WorldEngine::handle_report`];
//! the runtime stays on version 3.
//!
//! The v4 intake stages screens, held screens, resolved held windows,
//! windows/closes, the gesture barrier, held native rows, held attempt
//! rows, the verbatim presentation row, and the verbatim renderer input.
//! It stages no echoes (v4 carries no views). Capability conflicts grade to
//! narrow `conflicting-fact` holds before staging; unsupported state attempts
//! keep their raw rows and emit `state_offroute` attempt diagnostics.

use crate::world_engine::{StagedReport, WorldEngine};
use crate::world_report::{
    PresentationRowV4, RendererInput, RowDiagnostic, StateProperty, WorldReportV4,
};
use crate::world_report_v4_checks::grade_conflicting_windows_v4;
use crate::world_state::state_property_supported;

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

impl WorldEngine {
    /// Stage a v4 report into [`StagedReport`]: verbatim screens,
    /// held screens, capability-graded windows and explicit closes via the
    /// shared helpers, the gesture barrier, held native rows, held attempt rows, the
    /// verbatim presentation row, and the verbatim renderer input. Echoes stay
    /// empty (v4 carries no views). Diagnostics follow report lane order:
    /// held screens, resolved held windows (including newly graded capability
    /// conflicts), held native rows, held attempt rows carrying `Some`
    /// diagnostic (rows with `None` emit nothing and never get a fabricated
    /// scope), then unsupported state-attempt `ConflictingFact` diagnostics
    /// in input order (raw attempts stay intact, never a held row), then the
    /// held presentation's diagnostic when it carries `Some`.
    pub(crate) fn stage_facts_v4(&mut self, report: &WorldReportV4) {
        let unavailable: &[crate::connection::FactName] = self
            .capabilities()
            .map(|capabilities| capabilities.unavailable_facts.as_slice())
            .unwrap_or(&[]);
        let native_sticky_supported =
            state_property_supported(self.capabilities(), StateProperty::Sticky);
        let maximize_supported =
            state_property_supported(self.capabilities(), StateProperty::Maximized);
        let graded = grade_conflicting_windows_v4(
            report,
            unavailable,
            native_sticky_supported,
            maximize_supported,
        );
        let effective = graded.as_ref().unwrap_or(report);
        let prior_homing = self.staged.homing.clone();
        let resolved_held = self.resolve_held_window_scopes(&effective.held_windows, &prior_homing);
        let mut diagnostics =
            Self::staged_screen_window_diagnostics(&effective.held_screens, &resolved_held);
        diagnostics.extend(
            effective
                .held_native
                .iter()
                .map(|held| held.diagnostic.clone()),
        );
        diagnostics.extend(
            effective
                .held_attempts
                .iter()
                .filter_map(|held| held.diagnostic.clone()),
        );
        let state_offroute: Vec<RowDiagnostic> = effective
            .state_attempts
            .iter()
            .filter(|attempt| !state_property_supported(self.capabilities(), attempt.property))
            .filter_map(|attempt| {
                self.state_offroute_diagnostic_for(
                    &effective.windows,
                    &effective.held_windows,
                    &attempt.window,
                )
            })
            .collect();
        diagnostics.extend(state_offroute);
        if let PresentationRowV4::Held(held) = &effective.presentation
            && let Some(diagnostic) = &held.diagnostic
        {
            diagnostics.push(diagnostic.clone());
        }
        let mut staged = StagedReport {
            screens: effective.screens.clone(),
            held_screens: effective.held_screens.clone(),
            held_windows: resolved_held,
            diagnostics,
            gesture_barrier: effective.gesture_barrier,
            held_native: effective.held_native.clone(),
            held_attempts: effective.held_attempts.clone(),
            presentation_v4: Some(effective.presentation.clone()),
            renderer_input: effective.renderer_input.clone(),
            ..StagedReport::default()
        };
        self.stage_window_facts(&effective.windows, &effective.closed, &mut staged);
        self.staged = staged;
    }

    /// Stage `report` on a clone and read back the census: `self` is not
    /// mutated. Protocol tests use this to check conservation over every
    /// accepted v4 report fixture and scenario report without running the
    /// engine.
    #[doc(hidden)]
    pub fn stage_report_v4_census(&self, report: &WorldReportV4) -> StagedV4Census {
        let mut staged_engine = self.clone();
        staged_engine.stage_facts_v4(report);
        let staged = &staged_engine.staged;
        StagedV4Census {
            screens: staged.screens.len(),
            held_screens: staged.held_screens.len(),
            facts: staged.facts.len(),
            excluded: staged.excluded.len(),
            held_windows: staged.held_windows.len(),
            closes: staged.closes.len(),
            held_native: staged.held_native.len(),
            held_attempts: staged.held_attempts.len(),
            presentation_held: matches!(staged.presentation_v4, Some(PresentationRowV4::Held(_))),
            renderer_input: staged.renderer_input.clone(),
            echoes: staged.echoes.len(),
            diagnostics: staged.diagnostics.clone(),
        }
    }
}
