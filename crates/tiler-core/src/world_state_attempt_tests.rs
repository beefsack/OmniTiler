//! Internal state-attempt settlement.

use crate::directional::{OutputId, WindowId};
use crate::world_directional::tests::{accepted, fact, report_full, screen};
use crate::world_engine::WorldEngine;
use crate::world_report::{
    AttemptOutcome, FrameSetter, RowDiagnosticKind, RowDiagnosticLane, StateAttempt, StateProperty,
    WorldReport,
};
use crate::world_state::{RetainedStateBinding, RetainedStateTarget};

fn caps_all() -> crate::connection::HostCapabilities {
    crate::connection::HostCapabilities {
        fullscreen: true,
        maximize: true,
        native_sticky: true,
        above: true,
        ..Default::default()
    }
}

fn caps_none() -> crate::connection::HostCapabilities {
    crate::connection::HostCapabilities::default()
}

fn engine_with(caps: crate::connection::HostCapabilities) -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps).expect("generation")
}

#[allow(clippy::too_many_arguments)]
fn attempt(
    window: &str,
    property: StateProperty,
    target: bool,
    revision: u64,
    setter: FrameSetter,
    pre: Option<bool>,
    post: Option<bool>,
    ms: u64,
) -> StateAttempt {
    StateAttempt {
        window: WindowId(window.to_owned()),
        property,
        target,
        revision,
        setter,
        pre,
        post,
        attempt_ms: ms,
    }
}

fn insert(engine: &mut WorldEngine, window: &str, property: StateProperty, value: bool, rev: u64) {
    engine.retained_states.insert(
        (WindowId(window.to_owned()), property),
        RetainedStateBinding {
            target: RetainedStateTarget {
                window: WindowId(window.to_owned()),
                property,
                value,
                revision: rev,
            },
            pending: None,
            spent: false,
            disposition: AttemptOutcome::Unattempted,
            sticky_meta: None,
            fullscreen_meta: None,
            maximize_meta: None,
            above_meta: None,
        },
    );
}

fn binding(
    engine: &WorldEngine,
    window: &str,
    property: StateProperty,
) -> Option<RetainedStateBinding> {
    engine
        .retained_states
        .get(&(WindowId(window.to_owned()), property))
        .cloned()
}

fn with_attempts(mut report: WorldReport, attempts: Vec<StateAttempt>) -> WorldReport {
    report.state_attempts = attempts;
    report
}

fn base_fact_above(
    window: &str,
    output: &str,
    above: Option<bool>,
) -> crate::world_report::WindowFact {
    let mut fact = fact(window, output, 0);
    fact.above = above;
    fact
}

fn property_fact(
    window: &str,
    output: &str,
    property: StateProperty,
    value: Option<bool>,
) -> crate::world_report::WindowFact {
    let mut row = fact(window, output, 0);
    match property {
        StateProperty::Fullscreen => row.fullscreen = value,
        StateProperty::Maximized => row.maximized = value,
        StateProperty::Sticky => row.sticky = value,
        StateProperty::Above => row.above = value,
    }
    row
}

#[test]
fn exact_match_success_confirms_sticky() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Sticky, true, rev);
    let row = attempt(
        "win-a",
        StateProperty::Sticky,
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
        7,
    );
    let mut sticky_fact = fact("win-a", "out-a", 0);
    sticky_fact.sticky = Some(true);
    let reply = accepted(
        &mut engine,
        &with_attempts(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![sticky_fact],
                vec![],
                "win-a",
                None,
                None,
            ),
            vec![row.clone()],
        ),
    );
    let bound = binding(&engine, "win-a", StateProperty::Sticky).expect("binding retained");
    assert!(bound.spent);
    assert_eq!(bound.pending.as_ref(), Some(&row));
    assert_eq!(bound.pending.as_ref().map(|row| row.attempt_ms), Some(7));
    assert_eq!(bound.disposition, AttemptOutcome::Applied);
    assert!(reply.diagnostics.is_empty());
    assert!(reply.command_result.is_none());
    assert!(reply.operation_settled.is_empty());
    assert!(reply.effects.is_empty());
    assert!(reply.windows.iter().all(|row| row.state.is_empty()));
    assert!(reply.windows.iter().all(|row| row.visibility.is_none()));
}

#[test]
fn all_properties_confirm_on_readable_target() {
    for property in [
        StateProperty::Fullscreen,
        StateProperty::Maximized,
        StateProperty::Sticky,
        StateProperty::Above,
    ] {
        let mut engine = engine_with(caps_all());
        let rev = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![property_fact("win-a", "out-a", property, Some(false))],
                vec![],
                "win-a",
                None,
                None,
            ),
        )
        .state_revision;
        insert(&mut engine, "win-a", property, true, rev);
        accepted(
            &mut engine,
            &with_attempts(
                report_full(
                    2,
                    2,
                    vec![screen("out-a", 0)],
                    vec![property_fact("win-a", "out-a", property, Some(true))],
                    vec![],
                    "win-a",
                    None,
                    None,
                ),
                vec![attempt(
                    "win-a",
                    property,
                    true,
                    rev,
                    FrameSetter::Accepted,
                    Some(false),
                    Some(true),
                    7,
                )],
            ),
        );
        assert_eq!(
            binding(&engine, "win-a", property)
                .expect("binding")
                .disposition,
            AttemptOutcome::Applied,
            "{property:?}",
        );
    }
}

#[test]
fn refusal_needs_unchanged_preimage_and_current_fact() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Above, true, rev);
    let reply = accepted(
        &mut engine,
        &with_attempts(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![base_fact_above("win-a", "out-a", Some(false))],
                vec![],
                "win-a",
                None,
                None,
            ),
            vec![attempt(
                "win-a",
                StateProperty::Above,
                true,
                rev,
                FrameSetter::Refused,
                Some(false),
                Some(false),
                7,
            )],
        ),
    );
    let bound = binding(&engine, "win-a", StateProperty::Above).expect("binding retained");
    assert!(bound.spent);
    assert_eq!(bound.disposition, AttemptOutcome::RefusedProven);
    assert!(reply.diagnostics.is_empty());
}

#[test]
fn accepted_setter_alone_never_confirms() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Above, true, rev);
    accepted(
        &mut engine,
        &with_attempts(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![base_fact_above("win-a", "out-a", Some(false))],
                vec![],
                "win-a",
                None,
                None,
            ),
            vec![attempt(
                "win-a",
                StateProperty::Above,
                true,
                rev,
                FrameSetter::Accepted,
                Some(false),
                Some(true),
                7,
            )],
        ),
    );
    let bound = binding(&engine, "win-a", StateProperty::Above).expect("binding retained");
    assert!(bound.spent);
    assert_eq!(bound.disposition, AttemptOutcome::PossiblyExecuted);
}

#[test]
fn threw_and_null_proof_stay_ambiguous() {
    for (setter, pre, post) in [
        (FrameSetter::Threw, Some(false), Some(true)),
        (FrameSetter::Accepted, Some(false), None),
        (FrameSetter::Accepted, None, None),
    ] {
        let mut engine = engine_with(caps_all());
        let rev = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![base_fact_above("win-a", "out-a", Some(false))],
                vec![],
                "win-a",
                None,
                None,
            ),
        )
        .state_revision;
        insert(&mut engine, "win-a", StateProperty::Above, true, rev);
        accepted(
            &mut engine,
            &with_attempts(
                report_full(
                    2,
                    2,
                    vec![screen("out-a", 0)],
                    vec![base_fact_above("win-a", "out-a", Some(true))],
                    vec![],
                    "win-a",
                    None,
                    None,
                ),
                vec![attempt(
                    "win-a",
                    StateProperty::Above,
                    true,
                    rev,
                    setter,
                    pre,
                    post,
                    7,
                )],
            ),
        );
        let bound = binding(&engine, "win-a", StateProperty::Above).expect("binding retained");
        assert!(bound.spent, "{setter:?}");
        assert_eq!(
            bound.disposition,
            AttemptOutcome::PossiblyExecuted,
            "{setter:?} {pre:?} {post:?}",
        );
    }
}

#[test]
fn each_wrong_correlation_field_reads_absent() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Above, true, rev);
    let cases = [
        attempt(
            "win-b",
            StateProperty::Above,
            true,
            rev,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            7,
        ),
        attempt(
            "win-a",
            StateProperty::Fullscreen,
            true,
            rev,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            7,
        ),
        attempt(
            "win-a",
            StateProperty::Above,
            false,
            rev,
            FrameSetter::Accepted,
            Some(true),
            Some(false),
            7,
        ),
        attempt(
            "win-a",
            StateProperty::Above,
            true,
            rev + 1,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            7,
        ),
    ];
    for (i, row) in cases.into_iter().enumerate() {
        let seq = 2 + i as u64;
        let reply = accepted(
            &mut engine,
            &with_attempts(
                report_full(
                    seq,
                    seq,
                    vec![screen("out-a", 0)],
                    vec![base_fact_above("win-a", "out-a", Some(true))],
                    vec![],
                    "win-a",
                    None,
                    None,
                ),
                vec![row],
            ),
        );
        assert!(reply.diagnostics.is_empty());
        let bound = binding(&engine, "win-a", StateProperty::Above).expect("binding retained");
        assert!(!bound.spent);
        assert_eq!(bound.disposition, AttemptOutcome::Unattempted);
        assert!(bound.pending.is_none());
    }
}

#[test]
fn gap_capture_survives_then_recordfree_recovery_confirms() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Above, true, rev);
    let row = attempt(
        "win-a",
        StateProperty::Above,
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
        7,
    );
    let mut gapped = with_attempts(
        report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", None)],
            vec![],
            "win-a",
            None,
            None,
        ),
        vec![row.clone()],
    );
    gapped.inventory_complete = false;
    accepted(&mut engine, &gapped);
    let bound = binding(&engine, "win-a", StateProperty::Above).expect("binding retained");
    assert!(bound.spent);
    assert_eq!(bound.disposition, AttemptOutcome::PossiblyExecuted);
    accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(true))],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let bound = binding(&engine, "win-a", StateProperty::Above).expect("binding retained");
    assert_eq!(bound.pending.as_ref(), Some(&row));
    assert_eq!(bound.disposition, AttemptOutcome::Applied);
}

#[test]
fn gesture_barrier_capture_then_recovery_confirms() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Above, true, rev);
    let mut barrier = with_attempts(
        report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(true))],
            vec![],
            "win-a",
            None,
            None,
        ),
        vec![attempt(
            "win-a",
            StateProperty::Above,
            true,
            rev,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            7,
        )],
    );
    barrier.gesture_barrier = true;
    accepted(&mut engine, &barrier);
    assert_eq!(
        binding(&engine, "win-a", StateProperty::Above)
            .expect("binding")
            .disposition,
        AttemptOutcome::PossiblyExecuted,
    );
    accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(true))],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert_eq!(
        binding(&engine, "win-a", StateProperty::Above)
            .expect("binding")
            .disposition,
        AttemptOutcome::Applied,
    );
}

#[test]
fn exact_redelivery_idempotent_conflicting_repeat_keeps_original() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Above, true, rev);
    let row = attempt(
        "win-a",
        StateProperty::Above,
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
        7,
    );
    accepted(
        &mut engine,
        &with_attempts(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![base_fact_above("win-a", "out-a", Some(true))],
                vec![],
                "win-a",
                None,
                None,
            ),
            vec![row.clone()],
        ),
    );
    accepted(
        &mut engine,
        &with_attempts(
            report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![base_fact_above("win-a", "out-a", Some(true))],
                vec![],
                "win-a",
                None,
                None,
            ),
            vec![row.clone()],
        ),
    );
    let bound = binding(&engine, "win-a", StateProperty::Above).expect("binding");
    assert_eq!(bound.pending.as_ref(), Some(&row));
    assert_eq!(bound.disposition, AttemptOutcome::Applied);
    accepted(
        &mut engine,
        &with_attempts(
            report_full(
                4,
                4,
                vec![screen("out-a", 0)],
                vec![base_fact_above("win-a", "out-a", Some(true))],
                vec![],
                "win-a",
                None,
                None,
            ),
            vec![attempt(
                "win-a",
                StateProperty::Above,
                true,
                rev,
                FrameSetter::Accepted,
                Some(false),
                Some(false),
                9,
            )],
        ),
    );
    let bound = binding(&engine, "win-a", StateProperty::Above).expect("binding");
    assert_eq!(
        bound.pending.as_ref(),
        Some(&row),
        "conflicting repeat keeps original",
    );
    assert_eq!(bound.disposition, AttemptOutcome::Applied);
}

#[test]
fn offroute_matrix_all_four_properties_narrow() {
    let mut engine = engine_with(caps_none());
    let mut fa = fact("win-a", "out-a", 0);
    fa.sticky = None;
    let mut fb = fact("win-b", "out-b", 800);
    fb.sticky = None;
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![fa.clone(), fb.clone()],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let rows = vec![
        attempt(
            "win-a",
            StateProperty::Fullscreen,
            true,
            99,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            2,
        ),
        attempt(
            "win-a",
            StateProperty::Maximized,
            true,
            99,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            2,
        ),
        attempt(
            "win-a",
            StateProperty::Sticky,
            true,
            99,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            2,
        ),
        attempt(
            "win-a",
            StateProperty::Above,
            true,
            99,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            2,
        ),
    ];
    let reply = accepted(
        &mut engine,
        &with_attempts(
            report_full(
                2,
                2,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![fa, fb],
                vec![],
                "win-a",
                None,
                None,
            ),
            rows.clone(),
        ),
    );
    let last = engine.last_report.as_ref().expect("last report");
    assert_eq!(last.state_attempts, rows);
    assert_eq!(reply.diagnostics.len(), 4);
    for diagnostic in &reply.diagnostics {
        assert_eq!(diagnostic.row, RowDiagnosticLane::Attempt);
        assert_eq!(diagnostic.kind, RowDiagnosticKind::ConflictingFact);
        assert_eq!(diagnostic.id.as_deref(), Some("win-a"));
    }
    assert!(reply.diagnostics.iter().all(|diagnostic| {
        matches!(
            diagnostic.scope,
            crate::world_report::AuthorityScope::Output(ref out) if out.0 == "out-a"
        )
    }));
    for row in &reply.windows {
        assert!(row.state.is_empty());
        assert!(row.frame_authority.hold_reason().is_none(), "{row:?}");
        assert!(row.membership_authority.hold_reason().is_none(), "{row:?}");
    }
}

#[test]
fn offroute_scope_falls_back_to_proven_home_then_no_invention() {
    let mut engine = engine_with(caps_none());
    let mut fa = fact("win-a", "out-a", 0);
    fa.sticky = None;
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fa],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let mut unknown = fact("win-a", "out-a", 0);
    unknown.output = None;
    unknown.sticky = None;
    let reply = accepted(
        &mut engine,
        &with_attempts(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![unknown],
                vec![],
                "win-a",
                None,
                None,
            ),
            vec![attempt(
                "win-a",
                StateProperty::Above,
                true,
                99,
                FrameSetter::Accepted,
                Some(false),
                Some(true),
                2,
            )],
        ),
    );
    assert_eq!(reply.diagnostics.len(), 1);
    assert!(matches!(
        reply.diagnostics[0].scope,
        crate::world_report::AuthorityScope::Output(ref out) if out.0 == "out-a"
    ));
    let reply = accepted(
        &mut engine,
        &with_attempts(
            report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![{
                    let mut keep = fact("win-a", "out-a", 0);
                    keep.sticky = None;
                    keep
                }],
                vec![],
                "win-a",
                None,
                None,
            ),
            vec![attempt(
                "ghost",
                StateProperty::Above,
                true,
                99,
                FrameSetter::Accepted,
                Some(false),
                Some(true),
                3,
            )],
        ),
    );
    assert!(reply.diagnostics.is_empty());
    assert_eq!(
        engine
            .last_report
            .as_ref()
            .expect("last")
            .state_attempts
            .len(),
        1,
    );
}

#[test]
fn offroute_held_first_appearance_uses_held_output() {
    let mut engine = engine_with(caps_none());
    let mut fa = fact("win-a", "out-a", 0);
    fa.sticky = None;
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fa],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let mut held_report = report_full(
        2,
        2,
        vec![screen("out-a", 0), screen("out-b", 800)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    held_report.held_windows = vec![crate::world_report::HeldWindowRow {
        window: WindowId("win-held".to_owned()),
        ordinal: 0,
        output: Some(OutputId("out-b".to_owned())),
        fields: Vec::new(),
        diagnostic: None,
        kind: crate::world_report::RowDiagnosticKind::ConflictingFact,
    }];
    let rows = vec![
        attempt(
            "win-held",
            StateProperty::Above,
            true,
            3,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            5,
        ),
        attempt(
            "win-held",
            StateProperty::Sticky,
            true,
            4,
            FrameSetter::Accepted,
            Some(false),
            Some(true),
            6,
        ),
    ];
    held_report.state_attempts = rows.clone();
    let reply = accepted(&mut engine, &held_report);
    let last = engine.last_report.as_ref().expect("last");
    assert_eq!(last.state_attempts, rows);
    let attempt_diags: Vec<_> = reply
        .diagnostics
        .iter()
        .filter(|diag| diag.row == RowDiagnosticLane::Attempt)
        .collect();
    assert_eq!(attempt_diags.len(), 2);
    for diag in attempt_diags {
        assert_eq!(diag.kind, RowDiagnosticKind::ConflictingFact);
        assert!(matches!(
            diag.scope,
            crate::world_report::AuthorityScope::Output(ref out) if out.0 == "out-b"
        ));
    }
}

#[test]
fn offroute_diagnostics_survive_command_restage() {
    let mut engine = engine_with(caps_none());
    let mut fa = fact("win-a", "out-a", 0);
    fa.sticky = None;
    let mut fb = fact("win-b", "out-a", 400);
    fb.sticky = None;
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fa.clone(), fb.clone()],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let rows = vec![attempt(
        "win-a",
        StateProperty::Above,
        true,
        77,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
        2,
    )];
    let mut commanded = with_attempts(
        report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fa, fb],
            vec![],
            "win-a",
            Some(1),
            Some(crate::world_report::WorldCommand::Focus {
                window: WindowId("win-a".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
        rows.clone(),
    );
    commanded.request_seq = 2;
    let reply = match engine.handle_report(&commanded) {
        crate::world_engine::WorldOutcome::Accepted(reply) => reply,
        crate::world_engine::WorldOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(
        engine.last_report.as_ref().expect("last").state_attempts,
        rows
    );
    assert_eq!(reply.diagnostics.len(), 1);
    assert_eq!(reply.diagnostics[0].row, RowDiagnosticLane::Attempt);
    assert_eq!(
        reply.diagnostics[0].kind,
        RowDiagnosticKind::ConflictingFact
    );
    assert_eq!(reply.diagnostics[0].id.as_deref(), Some("win-a"));
}

#[test]
fn terminal_proof_stable_against_later_flip() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Above, true, rev);
    accepted(
        &mut engine,
        &with_attempts(
            report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![base_fact_above("win-a", "out-a", Some(true))],
                vec![],
                "win-a",
                None,
                None,
            ),
            vec![attempt(
                "win-a",
                StateProperty::Above,
                true,
                rev,
                FrameSetter::Accepted,
                Some(false),
                Some(true),
                7,
            )],
        ),
    );
    assert_eq!(
        binding(&engine, "win-a", StateProperty::Above)
            .expect("binding")
            .disposition,
        AttemptOutcome::Applied,
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert_eq!(
        binding(&engine, "win-a", StateProperty::Above)
            .expect("binding")
            .disposition,
        AttemptOutcome::Applied,
    );
    assert!(reply.windows.iter().all(|row| row.state.is_empty()));
    assert!(reply.effects.is_empty());
}

#[test]
fn bounds_upsert_and_lifecycle_cleanup() {
    let mut engine = engine_with(caps_all());
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    for property in [
        StateProperty::Fullscreen,
        StateProperty::Maximized,
        StateProperty::Sticky,
        StateProperty::Above,
    ] {
        insert(&mut engine, "win-a", property, true, 1);
    }
    assert_eq!(engine.retained_states.len(), 4);
    insert(&mut engine, "win-a", StateProperty::Above, false, 2);
    assert_eq!(engine.retained_states.len(), 4);
    let above = binding(&engine, "win-a", StateProperty::Above).expect("above");
    assert!(!above.target.value);
    assert_eq!(above.target.revision, 2);
    insert(&mut engine, "ghost", StateProperty::Above, true, 9);
    assert_eq!(engine.retained_states.len(), 5);
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(binding(&engine, "ghost", StateProperty::Above).is_none());
    assert!(binding(&engine, "win-a", StateProperty::Fullscreen).is_some());
    let mut closed = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
    );
    closed.closed = vec![WindowId("win-a".to_owned())];
    accepted(&mut engine, &closed);
    assert!(engine.retained_states.is_empty());
    assert_eq!(
        crate::world_state::MAX_RETAINED_STATE_BINDINGS,
        crate::world_report::MAX_WORLD_FACTS * 4,
    );
    for i in 0..crate::world_state::MAX_RETAINED_STATE_BINDINGS {
        engine.retained_states.insert(
            (WindowId(format!("fill-{i}")), StateProperty::Above),
            RetainedStateBinding {
                target: RetainedStateTarget {
                    window: WindowId(format!("fill-{i}")),
                    property: StateProperty::Above,
                    value: true,
                    revision: 1,
                },
                pending: None,
                spent: false,
                disposition: AttemptOutcome::Unattempted,
                sticky_meta: None,
                fullscreen_meta: None,
                maximize_meta: None,
                above_meta: None,
            },
        );
    }
    let before = engine.retained_states.len();
    let mut incomplete = report_full(
        4,
        4,
        vec![screen("out-a", 0)],
        vec![base_fact_above("win-a", "out-a", Some(false))],
        vec![],
        "win-a",
        None,
        None,
    );
    incomplete.inventory_complete = false;
    accepted(&mut engine, &incomplete);
    assert_eq!(engine.retained_states.len(), before);
    assert_eq!(
        engine.retained_states.len(),
        crate::world_state::MAX_RETAINED_STATE_BINDINGS,
    );
    let probe = attempt(
        "fill-0",
        StateProperty::Above,
        true,
        1,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
        7,
    );
    let reply = accepted(
        &mut engine,
        &with_attempts(
            report_full(
                5,
                5,
                vec![screen("out-a", 0)],
                vec![base_fact_above("fill-0", "out-a", Some(true))],
                vec![],
                "fill-0",
                None,
                None,
            ),
            vec![probe.clone()],
        ),
    );
    assert!(reply.diagnostics.is_empty());
    let settled = engine
        .retained_states
        .get(&(WindowId("fill-0".to_owned()), StateProperty::Above))
        .expect("exact-cap binding settles");
    assert!(settled.spent);
    assert_eq!(settled.pending.as_ref(), Some(&probe));
    assert_eq!(settled.disposition, AttemptOutcome::Applied);
    for i in 0..crate::world_state::MAX_RETAINED_STATE_BINDINGS {
        engine
            .retained_states
            .entry((WindowId(format!("fill-{i}")), StateProperty::Above))
            .or_insert(RetainedStateBinding {
                target: RetainedStateTarget {
                    window: WindowId(format!("fill-{i}")),
                    property: StateProperty::Above,
                    value: true,
                    revision: 1,
                },
                pending: None,
                spent: false,
                disposition: AttemptOutcome::Unattempted,
                sticky_meta: None,
                fullscreen_meta: None,
                maximize_meta: None,
                above_meta: None,
            });
    }
    engine.retained_states.insert(
        (WindowId("fill-over".to_owned()), StateProperty::Above),
        RetainedStateBinding {
            target: RetainedStateTarget {
                window: WindowId("fill-over".to_owned()),
                property: StateProperty::Above,
                value: true,
                revision: 1,
            },
            pending: None,
            spent: false,
            disposition: AttemptOutcome::Unattempted,
            sticky_meta: None,
            fullscreen_meta: None,
            maximize_meta: None,
            above_meta: None,
        },
    );
    assert_eq!(
        engine.retained_states.len(),
        crate::world_state::MAX_RETAINED_STATE_BINDINGS + 1,
    );
    let over_probe = attempt(
        "fill-1",
        StateProperty::Above,
        true,
        1,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
        7,
    );
    let mut overfull = with_attempts(
        report_full(
            6,
            6,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
        vec![over_probe],
    );
    overfull.inventory_complete = false;
    let reply = accepted(&mut engine, &overfull);
    assert!(reply.diagnostics.is_empty());
    assert!(
        engine
            .retained_states
            .get(&(WindowId("fill-1".to_owned()), StateProperty::Above))
            .expect("over-cap binding kept")
            .pending
            .is_none()
    );
}

#[test]
fn own_process_prunes_even_when_incomplete() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Above, true, rev);
    let mut own_fact = base_fact_above("win-a", "out-a", Some(false));
    own_fact.process_id = Some("test-own-pid".to_owned());
    let mut incomplete = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![own_fact],
        vec![],
        "win-a",
        None,
        None,
    );
    incomplete.inventory_complete = false;
    accepted(&mut engine, &incomplete);
    assert!(
        binding(&engine, "win-a", StateProperty::Above).is_none(),
        "proven own-process prunes incomplete",
    );
}

#[test]
fn rehandshake_starts_with_no_bindings() {
    let mut engine = engine_with(caps_all());
    let rev = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![base_fact_above("win-a", "out-a", Some(false))],
            vec![],
            "win-a",
            None,
            None,
        ),
    )
    .state_revision;
    insert(&mut engine, "win-a", StateProperty::Above, true, rev);
    assert!(!engine.retained_states.is_empty());
    let reply = engine.handshake(&crate::connection::HandshakeRequest {
        adapter: "adapter-2".to_owned(),
        request: "handshake-2".to_owned(),
        application_identity: crate::connection::ApplicationIdentity {
            app_id: Some("example.app".to_owned()),
            executable: Some("example".to_owned()),
            process_id: Some("pid-own-1".to_owned()),
        },
        capabilities: caps_all(),
    });
    assert!(reply.kind.is_none());
    assert!(engine.retained_states.is_empty());
}

#[test]
fn reply_bound_state_term_rejected_without_mutation() {
    use crate::world_report::WORLD_MAX_REPLY_BYTES;
    use crate::world_sizing::{
        MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES, reply_upper_bound, report_estimate,
    };
    let mut engine = engine_with(crate::connection::HostCapabilities {
        native_sticky: true,
        ..Default::default()
    });
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let logical = engine.engine.domain_keys().len();
    let staged = engine.staged.facts.len() + engine.staged.held_windows.len();
    let retained = engine.retained_publish_footprint();
    let attempts = 400usize;
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    let mut report: WorldReport = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        windows.clone(),
        vec![],
        "",
        None,
        None,
    );
    report.state_attempts = (0..attempts)
        .map(|i| {
            attempt(
                &format!("win-pad-{i:05}"),
                StateProperty::Above,
                true,
                9,
                FrameSetter::Accepted,
                Some(false),
                Some(true),
                1,
            )
        })
        .collect();
    while reply_upper_bound(&report, 0, logical, 1, staged, retained) <= WORLD_MAX_REPLY_BYTES {
        let i = windows.len();
        windows.push(fact(&format!("win-pad-{i:05}"), "out-a", 0));
        assert!(windows.len() < crate::world_report::MAX_WORLD_FACTS);
        report = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            windows.clone(),
            vec![],
            "",
            None,
            None,
        );
        report.state_attempts = (0..attempts)
            .map(|i| {
                attempt(
                    &format!("win-pad-{i:05}"),
                    StateProperty::Above,
                    true,
                    9,
                    FrameSetter::Accepted,
                    Some(false),
                    Some(true),
                    1,
                )
            })
            .collect();
        assert!(report_estimate(&report) <= crate::world_report::WORLD_MAX_REQUEST_BYTES);
    }
    let state_term = attempts * MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES;
    let without_state = reply_upper_bound(&report, 0, logical, 1, staged, retained) - state_term;
    assert!(without_state <= WORLD_MAX_REPLY_BYTES, "{without_state}");
    assert!(without_state + state_term > WORLD_MAX_REPLY_BYTES);
    assert_eq!(crate::world_sizing::MAX_TARGET_LANES_PER_WINDOW, 626);
    let revision = engine.state_revision;
    match engine.handle_report(&report) {
        crate::world_engine::WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, crate::world_engine::KIND_REPLY_SIZE);
        }
        other => panic!("expected reply-oversize, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision);
    let retry = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        vec![],
        "",
        None,
        None,
    );
    match engine.handle_report(&retry) {
        crate::world_engine::WorldOutcome::Accepted(_)
        | crate::world_engine::WorldOutcome::CommandConsumed(_) => {}
        other => panic!("expected accepted retry, got {other:?}"),
    }
}
