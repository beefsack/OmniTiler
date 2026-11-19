//! Per-report staged observation capture and borrowed raw routing context.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::world_report::{HoldReason, WorldCommand, WorldReportV4};
use crate::world_transaction::WorldTransaction;

/// One logical workspace projected into an Engine session: the stable core
/// id, its owning screen, and the per-report staged observation assigned by
/// [`WorldEngine::build_staged_domains`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StagedDomain {
    pub(crate) key: DomainKey,
    pub(crate) bounds: Rect,
    pub(crate) gap: i32,
    pub(crate) outer_gap: i32,
    /// Homed non-minimized facts with readable frames, as Engine carriers.
    pub(crate) windows: Vec<EngineWindow>,
    /// Adapter-observed focused lifetime homed here (empty when none):
    /// the staged focus proof for this domain.
    pub(crate) focused: WindowId,
    /// Homed minimized facts (kept out of the Engine carriers; the sweep
    /// retains their slots and publish emits explicit minimized rows).
    pub(crate) minimized: Vec<WindowId>,
    /// Homed facts with unreadable frames: the scope holds instead of
    /// authorizing geometry, never a silent absence.
    pub(crate) unreadable: Vec<WindowId>,
    /// Explicit closes homed here.
    pub(crate) closed: Vec<WindowId>,
    /// False when the owning screen is incomplete or carries an unreadable
    /// window fact: the scope becomes an explicit hold and its rows
    /// authorize nothing.
    pub(crate) readable: bool,
    pub(crate) hold: Option<HoldReason>,
}

/// Raw per-report observation capture: verbatim report lanes staged before
/// any classification. Both intakes populate the same boundary directly;
/// lane, command and lifetime consumers migrate to it in later increments.
/// No envelope admission, request/command correlation, or dispatch state
/// lives here: only the `observation_seq` watermark is retained for
/// observation ordering. Later helpers take explicit correlation arguments.
/// `raw_windows` keeps every typed row in source order, including
/// own-process and safety-excluded subjects; the classified `facts`/
/// `derived` carriers below stay the consume-lane view.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StagedObservation {
    pub(crate) inventory_complete: bool,
    pub(crate) observation_seq: u64,
    pub(crate) raw_windows: Vec<crate::world_report::WindowFact>,
    pub(crate) closed: Vec<WindowId>,
    pub(crate) native: Vec<crate::world_report::NativeObservation>,
    pub(crate) cursor: Option<(i32, i32)>,
    pub(crate) cursor_hit: Option<WindowId>,
    pub(crate) focused_global: String,
    pub(crate) action_foreground: Option<OutputId>,
    pub(crate) action_cached: Option<OutputId>,
    pub(crate) action_outputs: Vec<OutputId>,
    pub(crate) primary_output: Option<OutputId>,
    pub(crate) gesture: Option<crate::gesture::GestureIngress>,
    pub(crate) monotonic_ms: u64,
    pub(crate) creation_attempts: Vec<crate::world_report::NativeCreationAttempt>,
    pub(crate) property_attempts: Vec<crate::world_attempts::PropertyAttempt>,
    /// True once wire-bound visibility `LastWrite` evidence has been
    /// normalized into the canonical attempt lane for this staged report.
    /// Set after classified staging (even when the report carries zero
    /// visibility evidence) and cloned unchanged across classification
    /// restages, so normalization is exactly once per report regardless of
    /// whether any row produced an attempt.
    pub(crate) visibility_normalized: bool,
    pub(crate) focus_feedback: Vec<crate::world_report::FocusFeedback>,
    pub(crate) intent_records: Vec<crate::world_report::IntentRecord>,
}

/// Per-report staged observation: screens as reported, one
/// [`StagedDomain`] per known logical workspace, adapter view echoes, and
/// the raw fact index the settlement path proves against.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StagedReport {
    /// Captured raw observation for this report, staged before
    /// classification and normalized once before wrapping in `Arc`.
    /// See [`StagedObservation`].
    pub(crate) observation: Arc<StagedObservation>,
    pub(crate) screens: Vec<crate::world_report::ScreenReport>,
    /// Identifiable bad screen rows held narrowly, in report order.
    pub(crate) held_screens: Vec<crate::world_report::HeldScreenRow>,
    /// Identifiable bad window rows held narrowly, in report order.
    pub(crate) held_windows: Vec<crate::world_report::HeldWindowRow>,
    /// Identifiable native rows held narrowly, in report order. The legacy
    /// intake never fills this lane (stays empty).
    pub(crate) held_native: Vec<crate::world_report::HeldNativeRow>,
    /// Identifiable attempt rows held narrowly, in report order. The
    /// legacy intake never fills this lane (stays empty).
    pub(crate) held_attempts: Vec<crate::world_report::HeldAttemptRow>,
    /// Staged v4 presentation outcome, verbatim from the report: the valid
    /// ingress or the held singleton row, never a defaulted ingress. `None`
    /// for the legacy intake (v3 carries its own presentation shape).
    pub(crate) presentation_v4: Option<crate::world_report::PresentationRowV4>,
    /// Staged v4 renderer input, verbatim from the report with no
    /// validation here. `None` for the legacy intake and for v4 reports
    /// without renderer input.
    pub(crate) renderer_input: Option<crate::world_report::RendererInput>,
    /// Safety/own-process exclusions observed on this report: known
    /// exclusions (staged facts classifying excluded) plus own-process
    /// windows (dropped before facts). Never members, slots, or control:
    /// converge removes them cleanly (never a minimize slot, never a
    /// native close), reconciles drop them complete and incomplete, and
    /// pending actions withhold writes/focus. Only an explicit native
    /// close resets lifetime identity.
    pub(crate) excluded: BTreeSet<WindowId>,
    /// Typed narrow-hold diagnostics emitted with this report's reply:
    /// one per held screen/window row plus one per use-time scale hold.
    pub(crate) diagnostics: Vec<crate::world_report::RowDiagnostic>,
    pub(crate) domains: Vec<StagedDomain>,
    /// Non-empty adapter view echoes by output.
    pub(crate) echoes: BTreeMap<String, String>,
    /// Raw observed screen per fact, before any logical assignment.
    /// `None` is unknown affinity, never a current-output guess.
    pub(crate) fact_output: BTreeMap<WindowId, Option<OutputId>>,
    /// Full raw facts by window id.
    pub(crate) facts: BTreeMap<WindowId, crate::world_report::WindowFact>,
    /// Core-derived per-window classification (see
    /// [`crate::window_classify`]): membership, hold, and exclusion
    /// carriers for the consume lanes. Raw facts stay the sole public
    /// input; these carriers never leave the core.
    pub(crate) derived: BTreeMap<WindowId, crate::window_classify::ClassifiedWindow>,
    /// Raw explicit closes.
    pub(crate) closes: BTreeSet<WindowId>,
    /// Effective classified window rows for this report: the
    /// capability-graded rows on the v4 intake, the verbatim rows on the
    /// legacy intake. Classification-only restages consume these, never
    /// the raw capture; the raw capture stays the ingestion boundary.
    pub(crate) effective_windows: Arc<Vec<crate::world_report::WindowFact>>,
    /// Staged logical homing per window.
    pub(crate) homing: BTreeMap<WindowId, DomainKey>,
    /// Carried gesture barrier for the staged run.
    pub(crate) gesture_barrier: bool,
    /// Issuing revision of the accepted run that staged this report (`0`
    /// for standalone staging probes outside `run_accepted_v4_batched`).
    pub(crate) revision: u64,
    /// True during `run_accepted_v4_batched` once a focus effect was
    /// emitted for this report before `drain_focus_candidates` updates
    /// `pending_focus`, preventing same-batch implicit fallback.
    pub(crate) batch_focus_issued: bool,
}

impl StagedReport {
    /// Install freshly computed derived classification, diagnostics, and index
    /// carriers in place while preserving retained stable capture inputs,
    /// adapter echoes, issuing revision, and same-batch focus state.
    pub(crate) fn apply_derived(&mut self, derived: crate::world_classification::StageDerived) {
        for (held, diag) in self
            .held_windows
            .iter_mut()
            .zip(derived.held_window_diagnostics)
        {
            held.diagnostic = diag;
        }
        self.diagnostics = derived.diagnostics;
        self.excluded = derived.excluded;
        self.fact_output = derived.fact_output;
        self.facts = derived.facts;
        self.derived = derived.derived;
        self.closes = derived.closes;
        self.domains.clear();
        self.homing.clear();
    }
}

impl StagedObservation {
    /// Normalize wire-bound visibility attempt evidence into the canonical
    /// attempt lane at most once per captured report. Only rows whose window
    /// matches a live or held window in this report are admitted; absent and
    /// close-only subjects are ignored with no authority or diagnostic.
    pub(crate) fn normalize_visibility_attempts(
        &mut self,
        visibility_attempts: &[crate::world_report::VisibilityAttempt],
        facts: &BTreeMap<WindowId, crate::world_report::WindowFact>,
        held_windows: &[crate::world_report::HeldWindowRow],
    ) {
        if self.visibility_normalized {
            return;
        }
        if visibility_attempts.is_empty() {
            self.visibility_normalized = true;
            return;
        }
        let held_set: Option<std::collections::BTreeSet<&WindowId>> = if held_windows.is_empty() {
            None
        } else {
            Some(held_windows.iter().map(|held| &held.window).collect())
        };
        for attempt in visibility_attempts {
            let present = facts.contains_key(&attempt.window)
                || held_set
                    .as_ref()
                    .is_some_and(|set| set.contains(&attempt.window));
            if present {
                self.property_attempts
                    .push(crate::world_attempts::PropertyAttempt::from_visibility_attempt(attempt));
            }
        }
        self.visibility_normalized = true;
    }
}

/// Explicit typed event identity for command-path event construction.
/// Threaded from the validated envelope instead of re-parsing a report.
#[derive(Debug, Clone)]
pub(crate) struct EventIssue {
    pub(crate) owner: OwnerId,
    pub(crate) correlation: CorrelationId,
}

/// Borrowed raw routing context for observation-dependent routing leaves.
/// Built from the source report for pre-capture freezes (prior retained
/// homes/views) or from staged state for post-capture commands (current
/// observations). No cloning, no ingestion.
pub(crate) struct RawRouting<'a> {
    pub(crate) inventory_complete: bool,
    pub(crate) cursor: Option<(i32, i32)>,
    pub(crate) focused_global: &'a str,
    pub(crate) action_foreground: Option<&'a OutputId>,
    pub(crate) action_cached: Option<&'a OutputId>,
    pub(crate) action_outputs: &'a [OutputId],
    pub(crate) primary_output: Option<&'a OutputId>,
    pub(crate) screens: &'a [crate::world_report::ScreenReport],
    pub(crate) held_screens: &'a [crate::world_report::HeldScreenRow],
}

impl<'a> RawRouting<'a> {
    pub(crate) fn from_staged(staged: &'a StagedReport) -> Self {
        Self {
            inventory_complete: staged.observation.inventory_complete,
            cursor: staged.observation.cursor,
            focused_global: staged.observation.focused_global.as_str(),
            action_foreground: staged.observation.action_foreground.as_ref(),
            action_cached: staged.observation.action_cached.as_ref(),
            action_outputs: staged.observation.action_outputs.as_slice(),
            primary_output: staged.observation.primary_output.as_ref(),
            screens: staged.screens.as_slice(),
            held_screens: staged.held_screens.as_slice(),
        }
    }

    /// Genuine v4 routing constructor: the same borrowed raw lanes read
    /// directly from the typed v4 report, never through a v4-to-legacy
    /// conversion. The caller passes the original v4 rows so the freeze
    /// below captures against prior retained state.
    pub(crate) fn from_v4(report: &'a WorldReportV4) -> Self {
        Self {
            inventory_complete: report.inventory_complete,
            cursor: report.cursor,
            focused_global: report.focused_global.as_str(),
            action_foreground: report.action_foreground.as_ref(),
            action_cached: report.action_cached.as_ref(),
            action_outputs: report.action_outputs.as_slice(),
            primary_output: report.primary_output.as_ref(),
            screens: report.screens.as_slice(),
            held_screens: report.held_screens.as_slice(),
        }
    }
}

impl WorldTransaction<'_> {
    /// Freeze the pre-observation command sources against prior retained
    /// state: the implicit send source (retained home else the action
    /// context) and the direction-only migration source (action output plus
    /// its retained view). Takes the command explicitly and the borrowed
    /// raw routing built from the source report; reads retained homing and
    /// views only. Never ingests intent, never stages observations, and
    /// never touches downstream staged gates: call before capture/ingest
    /// so first-tick admissions, staged rehoming, and same-report view
    /// settlement cannot retarget the current intent. Explicit send
    /// sources and report-only commands freeze nothing.
    pub(crate) fn freeze_command_sources(
        &self,
        command: Option<&WorldCommand>,
        routing: &RawRouting<'_>,
    ) -> (Option<OutputId>, Option<(OutputId, String)>) {
        // Execution-time (Implicit-wrapped) sends carry an empty placeholder
        // window: resolve the retained subject from the pre-freeze slot
        // first, then look up its retained home. The slot vintage is the
        // caller's: pre-observation for the first batch command, post-prior
        // for later per-command refreezes. An unresolvable subject freezes
        // nothing, so execution refuses instead of falling back to action.
        let subject = self.execution_subject();
        let effective_send: Option<(&WindowId, &crate::world_report::SendSource)> = match command {
            Some(
                WorldCommand::SendRelative {
                    window: crate::directional::WindowSubject::CurrentFocus,
                    source,
                    ..
                }
                | WorldCommand::SendNumbered {
                    window: crate::directional::WindowSubject::CurrentFocus,
                    source,
                    ..
                },
            ) => subject.as_ref().map(|slot| (slot, source)),
            Some(
                WorldCommand::SendRelative {
                    window: crate::directional::WindowSubject::Window(window),
                    source,
                    ..
                }
                | WorldCommand::SendNumbered {
                    window: crate::directional::WindowSubject::Window(window),
                    source,
                    ..
                },
            ) => Some((window, source)),
            _ => None,
        };
        let frozen_send_source = match effective_send {
            Some((window, source)) => match source {
                crate::world_report::SendSource::Explicit(_) => None,
                crate::world_report::SendSource::HomeThenAction => {
                    self.resolve_home_then_action(routing, window)
                }
            },
            None => None,
        };
        let frozen_migration_source = match command {
            Some(WorldCommand::MigrateDirectional { .. }) => {
                let resolved = self.resolve_action_output(routing);
                match resolved {
                    Some(output) => self
                        .world
                        .workspace
                        .views
                        .get(&output.0)
                        .cloned()
                        .map(|view| (output, view)),
                    None => None,
                }
            }
            _ => None,
        };
        (frozen_send_source, frozen_migration_source)
    }
}
