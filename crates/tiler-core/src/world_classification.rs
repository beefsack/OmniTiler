//! Shared read-only stage classifier for v4 census, initial accepted
//! staging, and classification-only restage.

use std::collections::{BTreeMap, BTreeSet};

use crate::connection::{FactName, HostCapabilities};
use crate::directional::{OutputId, WindowId};
use crate::intent_data::IntentData;
use crate::session::DomainKey;
use crate::size_hints::FixedSizePredicate;
use crate::window_classify::{ClassifiedWindow, ClassifyInput, classify_window};
use crate::workspace_scope::DisplacedEntry;
use crate::world_attempts::PropertyAttempt;
use crate::world_connection_state::WorldConnectionState;
use crate::world_engine::{ExcludedIntent, RetainedFloat, WorldState};
use crate::world_layout::WorldLayout;
use crate::world_members::MemberState;
use crate::world_report::{
    AuthorityScope, HeldAttemptRow, HeldNativeRow, HeldScreenRow, HeldWindowRow, PresentationRowV4,
    RowDiagnostic, RowDiagnosticLane, StateAttempt, StateProperty, WindowFact,
};
use crate::world_state::{state_offroute_diagnostic_in, state_property_supported};

/// Narrow immutable classification context borrowing only connection policy and
/// retained world ownership needed to classify staged rows.
pub(crate) struct StageClassificationContext<'a> {
    pub(crate) capabilities: Option<&'a HostCapabilities>,
    pub(crate) own_process_id: Option<&'a str>,
    pub(crate) fixed_predicate: FixedSizePredicate,
    pub(crate) layout: &'a WorldLayout,
    pub(crate) members: &'a BTreeMap<WindowId, MemberState>,
    pub(crate) mapping: &'a BTreeMap<String, Vec<String>>,
    pub(crate) displaced: &'a BTreeMap<String, DisplacedEntry>,
    pub(crate) excluded_intent: &'a BTreeMap<WindowId, ExcludedIntent>,
    pub(crate) pending_intent: &'a BTreeMap<WindowId, IntentData>,
    pub(crate) intent_conflicts: &'a BTreeSet<WindowId>,
    pub(crate) retained_floats: &'a [RetainedFloat],
    pub(crate) restored_floats: &'a BTreeSet<WindowId>,
    pub(crate) restored_sticky: &'a BTreeMap<WindowId, bool>,
    pub(crate) prior_homing: &'a BTreeMap<WindowId, DomainKey>,
}

impl<'a> StageClassificationContext<'a> {
    #[must_use]
    pub(crate) fn new(connection: &'a WorldConnectionState, world: &'a WorldState) -> Self {
        Self {
            capabilities: connection.capabilities(),
            own_process_id: connection
                .application_identity()
                .and_then(|identity| identity.process_id.as_deref()),
            fixed_predicate: connection.fixed_size_predicate_setting(),
            layout: &world.layout,
            members: &world.workspace.members,
            mapping: &world.workspace.mapping,
            displaced: &world.workspace.displaced,
            excluded_intent: &world.intent.excluded_intent,
            pending_intent: &world.intent.pending_intent,
            intent_conflicts: &world.intent.intent_conflicts,
            retained_floats: world.settlement.retained_floats.as_slice(),
            restored_floats: &world.intent.restored_floats,
            restored_sticky: &world.intent.restored_sticky,
            prior_homing: &world.staged.homing,
        }
    }
}

/// Source of state-attempt rows for offroute diagnostics: initial effective
/// report rows vs already-normalized observation attempts on restage.
pub(crate) enum StateAttemptSource<'a> {
    Report(&'a [StateAttempt]),
    Normalized(&'a [PropertyAttempt]),
}

/// Borrowed stable inputs for one read-only stage classification pass.
pub(crate) struct StageClassificationInput<'a> {
    pub(crate) windows: &'a [WindowFact],
    pub(crate) closed: &'a [WindowId],
    pub(crate) held_screens: &'a [HeldScreenRow],
    pub(crate) held_windows: &'a [HeldWindowRow],
    pub(crate) held_native: &'a [HeldNativeRow],
    pub(crate) held_attempts: &'a [HeldAttemptRow],
    pub(crate) presentation: Option<&'a PresentationRowV4>,
    pub(crate) state_attempts: StateAttemptSource<'a>,
}

/// Derived classification, diagnostics, and index carriers produced by
/// [`classify_stage`].
pub(crate) struct StageDerived {
    pub(crate) held_window_diagnostics: Vec<Option<RowDiagnostic>>,
    pub(crate) diagnostics: Vec<RowDiagnostic>,
    pub(crate) excluded: BTreeSet<WindowId>,
    pub(crate) fact_output: BTreeMap<WindowId, Option<OutputId>>,
    pub(crate) facts: BTreeMap<WindowId, WindowFact>,
    pub(crate) derived: BTreeMap<WindowId, ClassifiedWindow>,
    pub(crate) closes: BTreeSet<WindowId>,
}

fn resolve_held_window_diagnostic(
    ctx: &StageClassificationContext<'_>,
    held: &HeldWindowRow,
) -> Option<RowDiagnostic> {
    let scope = match &held.output {
        Some(out) => Some(AuthorityScope::Output(out.clone())),
        None => ctx
            .layout
            .retained_home_any(&held.window)
            .or_else(|| {
                ctx.prior_homing
                    .get(&held.window)
                    .map(|key| key.output.clone())
            })
            .map(AuthorityScope::Output),
    };
    scope.map(|scope| RowDiagnostic {
        scope,
        row: RowDiagnosticLane::Window,
        id: Some(held.window.0.clone()),
        kind: held.kind,
    })
}

fn collect_state_offroute_diagnostics(
    ctx: &StageClassificationContext<'_>,
    input: &StageClassificationInput<'_>,
) -> Vec<RowDiagnostic> {
    let mut offroute = Vec::new();
    match input.state_attempts {
        StateAttemptSource::Report(attempts) => {
            for attempt in attempts {
                if !state_property_supported(ctx.capabilities, attempt.property)
                    && let Some(diag) = state_offroute_diagnostic_in(
                        input.windows,
                        input.held_windows,
                        ctx.members,
                        ctx.mapping,
                        ctx.displaced,
                        ctx.layout,
                        &attempt.window,
                    )
                {
                    offroute.push(diag);
                }
            }
        }
        StateAttemptSource::Normalized(attempts) => {
            for evidence in attempts.iter().filter_map(PropertyAttempt::as_state) {
                if !state_property_supported(ctx.capabilities, evidence.property)
                    && let Some(diag) = state_offroute_diagnostic_in(
                        input.windows,
                        input.held_windows,
                        ctx.members,
                        ctx.mapping,
                        ctx.displaced,
                        ctx.layout,
                        &evidence.window,
                    )
                {
                    offroute.push(diag);
                }
            }
        }
    }
    offroute
}

/// Run read-only stage classification over borrowed stable inputs without
/// mutating world state or copying raw/effective capture lanes.
#[must_use]
pub(crate) fn classify_stage(
    ctx: &StageClassificationContext<'_>,
    input: StageClassificationInput<'_>,
) -> StageDerived {
    let held_window_diagnostics: Vec<Option<RowDiagnostic>> = input
        .held_windows
        .iter()
        .map(|held| resolve_held_window_diagnostic(ctx, held))
        .collect();

    let mut diagnostics: Vec<RowDiagnostic> = input
        .held_screens
        .iter()
        .map(|held| held.diagnostic.clone())
        .chain(held_window_diagnostics.iter().flatten().cloned())
        .collect();
    diagnostics.extend(input.held_native.iter().map(|held| held.diagnostic.clone()));
    diagnostics.extend(
        input
            .held_attempts
            .iter()
            .filter_map(|held| held.diagnostic.clone()),
    );
    diagnostics.extend(collect_state_offroute_diagnostics(ctx, &input));
    if let Some(PresentationRowV4::Held(held)) = input.presentation
        && let Some(diagnostic) = &held.diagnostic
    {
        diagnostics.push(diagnostic.clone());
    }

    let unavailable: &[FactName] = ctx
        .capabilities
        .map(|capabilities| capabilities.unavailable_facts.as_slice())
        .unwrap_or(&[]);
    let maximize_supported = state_property_supported(ctx.capabilities, StateProperty::Maximized);
    let native_sticky_supported = state_property_supported(ctx.capabilities, StateProperty::Sticky);

    let mut excluded = BTreeSet::new();
    let mut fact_output = BTreeMap::new();
    let mut facts = BTreeMap::new();
    let mut derived = BTreeMap::new();

    for fact in input.windows {
        let member_tiled = ctx
            .members
            .get(&fact.window)
            .is_some_and(|state| !state.floating);
        let engine_exception = ctx.layout.is_exception_window_any(&fact.window);
        let retained_tiled = member_tiled && !engine_exception;
        let automatic_fixed = ctx.layout.is_automatic_fixed_float_any(&fact.window)
            || ctx
                .excluded_intent
                .get(&fact.window)
                .is_some_and(|intent| intent.automatic_fixed);
        let tile_override = ctx.layout.has_fixed_tile_override_any(&fact.window)
            || ctx
                .excluded_intent
                .get(&fact.window)
                .is_some_and(|intent| intent.tile_override)
            || ctx.pending_intent.get(&fact.window).is_some_and(|data| {
                data.float == Some(false)
                    && data.sticky_prior.is_none()
                    && !ctx.intent_conflicts.contains(&fact.window)
            });
        let explicit_float = ctx
            .retained_floats
            .iter()
            .any(|retained| retained.window == fact.window)
            || ctx.restored_floats.contains(&fact.window)
            || ctx.pending_intent.get(&fact.window).is_some_and(|data| {
                data.float == Some(true)
                    && data.sticky_prior.is_none()
                    && !ctx.intent_conflicts.contains(&fact.window)
            });
        let semantic_sticky = ctx.restored_sticky.contains_key(&fact.window)
            || ctx.pending_intent.get(&fact.window).is_some_and(|data| {
                data.sticky_prior.is_some() && !ctx.intent_conflicts.contains(&fact.window)
            });
        let classified = classify_window(ClassifyInput {
            fact,
            unavailable,
            fixed_predicate: ctx.fixed_predicate,
            maximize_supported,
            native_sticky_supported,
            retained_tiled,
            automatic_fixed,
            tile_override,
            explicit_float,
            semantic_sticky,
            own_process_id: ctx.own_process_id,
        });
        if classified.own_excluded {
            excluded.insert(fact.window.clone());
            continue;
        }
        if classified.excluded {
            excluded.insert(fact.window.clone());
        }
        fact_output.insert(fact.window.clone(), fact.output.clone());
        facts.insert(fact.window.clone(), fact.clone());
        derived.insert(fact.window.clone(), classified);
    }

    let closes: BTreeSet<WindowId> = input.closed.iter().cloned().collect();

    StageDerived {
        held_window_diagnostics,
        diagnostics,
        excluded,
        fact_output,
        facts,
        derived,
        closes,
    }
}
