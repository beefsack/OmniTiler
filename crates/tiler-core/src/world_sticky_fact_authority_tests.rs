//! Capability-gated sticky fact authority (W3): when `nativeSticky=false`
//! the raw `sticky` fact must be null; any non-null value is a narrow
//! conflicting-fact hold. Composed-route `sticky:null` admits ordinarily.
//! When `nativeSticky=true` the existing known-bool semantics hold and a
//! null holds sticky-dependent classification.

use crate::directional::WindowId;
use crate::intent_data::{IntentData, encode_intent_data};
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{IntentRecord, RowDiagnosticKind, RowDiagnosticLane};
use crate::world_v4_support_tests::{fact, report_full, screen};

fn caps(native_sticky: bool) -> crate::connection::HostCapabilities {
    crate::connection::HostCapabilities {
        native_sticky,
        // Fixtures carry native maximize facts; the lane under test is
        // sticky, so the orthogonal native maximize route stays valid.
        maximize: true,
        ..Default::default()
    }
}

fn engine_with(native_sticky: bool) -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps(native_sticky))
        .expect("generation")
}

fn accepted_reply(
    engine: &mut WorldEngine,
    report: &crate::world_report::WorldReportV4,
) -> crate::world_report::WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn controlled(reply: &crate::world_report::WorldReplyV4) -> Vec<String> {
    reply
        .windows
        .iter()
        .filter(|row| {
            row.membership_authority == crate::world_report::ScopeAuthority::Controlled
                && row.frame_authority == crate::world_report::ScopeAuthority::Controlled
        })
        .map(|row| row.lifetime.0.clone())
        .collect()
}

#[test]
fn composed_null_admits_while_nonnull_conflicts_narrowly() {
    let mut engine = engine_with(false);
    let mut composed = fact("win-composed", "out-a", 0);
    composed.sticky = None;
    let mut known_false = fact("win-false", "out-a", 200);
    known_false.sticky = Some(false);
    let mut known_true = fact("win-true", "out-a", 400);
    known_true.sticky = Some(true);
    let mut report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![composed, known_false, known_true],
        vec![],
        "",
        None,
        None,
    );
    report.intent_records = vec![IntentRecord {
        win: WindowId("win-false".to_owned()),
        data: encode_intent_data(&IntentData::new(Some(true), None)),
    }];
    let reply = accepted_reply(&mut engine, &report);
    assert!(reply.intent_writes.is_empty(), "{:?}", reply.intent_writes);
    let controlled = controlled(&reply);
    assert!(
        controlled.contains(&"win-composed".to_owned()),
        "composed null tiles ordinarily: {controlled:?}"
    );
    assert!(
        !controlled.contains(&"win-false".to_owned()),
        "unsupported non-null false holds: {controlled:?}"
    );
    assert!(
        !controlled.contains(&"win-true".to_owned()),
        "unsupported non-null true holds: {controlled:?}"
    );
    assert!(
        !reply
            .windows
            .iter()
            .any(|row| row.lifetime.0 == "win-false"),
        "conflicted row takes no slot: {:?}",
        reply.windows
    );
    assert!(
        !reply.windows.iter().any(|row| row.lifetime.0 == "win-true"),
        "conflicted row takes no slot: {:?}",
        reply.windows
    );
    for win in ["win-false", "win-true"] {
        assert!(
            reply
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.row == RowDiagnosticLane::Window
                    && diagnostic.id.as_deref() == Some(win)
                    && diagnostic.kind == RowDiagnosticKind::ConflictingFact),
            "conflicting-fact diagnostic for {win}: {:?}",
            reply.diagnostics
        );
    }
    assert!(
        !reply
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.row == RowDiagnosticLane::Window
                && diagnostic.id.as_deref() == Some("win-composed")),
        "healthy composed row mints no hold: {:?}",
        reply.diagnostics
    );
    let held: Vec<&str> = engine
        .staged
        .held_windows
        .iter()
        .map(|held| held.window.0.as_str())
        .collect();
    assert!(held.contains(&"win-false"), "raw retained: {held:?}");
    assert!(held.contains(&"win-true"), "raw retained: {held:?}");
    assert!(!held.contains(&"win-composed"), "{held:?}");
    for held_row in engine.staged.held_windows.iter() {
        if held_row.window.0 == "win-false" || held_row.window.0 == "win-true" {
            assert_eq!(held_row.kind, RowDiagnosticKind::ConflictingFact);
            let sticky = held_row
                .fields
                .iter()
                .find(|(key, _)| key == "sticky")
                .expect("sticky retained");
            assert!(
                matches!(sticky.1, crate::connection::RawSettingValue::Bool(_)),
                "raw non-null retained, got {:?}",
                sticky.1
            );
        }
    }
    let members = engine.logical_members_all();
    assert!(members.iter().any(|member| member.0 == "win-composed"));
    assert!(!members.iter().any(|member| member.0 == "win-false"));
    assert!(!members.iter().any(|member| member.0 == "win-true"));
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-false".to_owned())),
        "pending explicit intent retained, not consumed"
    );
    assert!(
        !engine
            .intent_admitted
            .contains(&WindowId("win-false".to_owned())),
        "conflicted placeholder never latches admission"
    );
    assert!(
        !engine
            .intent_admitted
            .contains(&WindowId("win-true".to_owned())),
        "conflicted placeholder never latches admission"
    );
}

#[test]
fn supported_native_keeps_bool_semantics_and_null_holds() {
    let mut engine = engine_with(true);
    let mut known_false = fact("win-false", "out-a", 0);
    known_false.sticky = Some(false);
    let mut known_true = fact("win-true", "out-a", 200);
    known_true.sticky = Some(true);
    let mut unknown = fact("win-null", "out-a", 400);
    unknown.sticky = None;
    let report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![known_false, known_true, unknown],
        vec![],
        "",
        None,
        None,
    );
    let reply = accepted_reply(&mut engine, &report);
    assert!(reply.intent_writes.is_empty(), "{:?}", reply.intent_writes);
    let controlled = controlled(&reply);
    assert!(
        controlled.contains(&"win-false".to_owned()),
        "supported known-false tiles: {controlled:?}"
    );
    assert!(
        !reply
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.row == RowDiagnosticLane::Window
                && matches!(diagnostic.id.as_deref(), Some("win-false" | "win-true"))
                && diagnostic.kind == RowDiagnosticKind::ConflictingFact),
        "supported known bools never conflict: {:?}",
        reply.diagnostics
    );
    assert!(
        !controlled.contains(&"win-null".to_owned()),
        "supported null holds sticky-dependent classification: {controlled:?}"
    );
    assert!(
        !reply.windows.iter().any(|row| row.lifetime.0 == "win-null"),
        "held null takes no slot: {:?}",
        reply.windows
    );
    let members = engine.logical_members_all();
    assert!(members.iter().any(|member| member.0 == "win-false"));
}

#[test]
fn pure_classify_bypass_grants_no_unsupported_authority() {
    use crate::size_hints::FixedSizePredicate;
    use crate::window_classify::{ClassifyInput, classify_window};
    let unavailable: &[crate::connection::FactName] = &[];
    let mut supported_true = fact("win-t", "out-a", 0);
    supported_true.sticky = Some(true);
    let mut supported_false = fact("win-f", "out-a", 0);
    supported_false.sticky = Some(false);
    let mut composed_null = fact("win-n", "out-a", 0);
    composed_null.sticky = None;
    for (name, probe) in [("true", &supported_true), ("false", &supported_false)] {
        let classified = classify_window(ClassifyInput {
            fact: probe,
            unavailable,
            fixed_predicate: FixedSizePredicate::BothAxes,
            maximize_supported: true,
            native_sticky_supported: false,
            retained_tiled: false,
            automatic_fixed: false,
            tile_override: false,
            explicit_float: false,
            semantic_sticky: false,
            own_process_id: Some("test-own-pid"),
        });
        assert!(!classified.sticky, "{name}: {classified:?}");
        assert!(!classified.floating, "{name}: {classified:?}");
        assert!(
            classified.hold,
            "unsupported non-null {name} fails closed: {classified:?}"
        );
    }
    let classified = classify_window(ClassifyInput {
        fact: &composed_null,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: false,
        retained_tiled: false,
        automatic_fixed: false,
        tile_override: false,
        explicit_float: false,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(!classified.sticky, "{classified:?}");
    assert!(
        !classified.hold,
        "composed null admits ordinarily: {classified:?}"
    );
    let classified = classify_window(ClassifyInput {
        fact: &supported_true,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: true,
        retained_tiled: false,
        automatic_fixed: false,
        tile_override: false,
        explicit_float: false,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(classified.sticky, "{classified:?}");
    assert!(!classified.hold, "{classified:?}");
    let classified = classify_window(ClassifyInput {
        fact: &composed_null,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: true,
        retained_tiled: false,
        automatic_fixed: false,
        tile_override: false,
        explicit_float: false,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(
        classified.hold,
        "supported null holds sticky-dependent classification: {classified:?}"
    );
}

#[test]
fn mixed_outputs_healthy_siblings_keep_authority() {
    let mut engine = engine_with(false);
    let mut healthy_a = fact("win-healthy-a", "out-a", 0);
    healthy_a.sticky = None;
    let mut healthy_b = fact("win-healthy-b", "out-b", 800);
    healthy_b.sticky = None;
    let mut conflict_a = fact("win-conflict-a", "out-a", 200);
    conflict_a.sticky = Some(false);
    let mut conflict_b = fact("win-conflict-b", "out-b", 1000);
    conflict_b.sticky = Some(true);
    let report = report_full(
        1,
        1,
        vec![screen("out-a", 0), screen("out-b", 800)],
        vec![healthy_a, conflict_a, healthy_b, conflict_b],
        vec![],
        "",
        None,
        None,
    );
    let reply = accepted_reply(&mut engine, &report);
    let controlled = controlled(&reply);
    assert!(
        controlled.contains(&"win-healthy-a".to_owned()),
        "same-output healthy sibling keeps authority: {controlled:?}"
    );
    assert!(
        controlled.contains(&"win-healthy-b".to_owned()),
        "other-output healthy sibling keeps authority: {controlled:?}"
    );
    assert!(
        !controlled.contains(&"win-conflict-a".to_owned()),
        "same-output conflict holds: {controlled:?}"
    );
    assert!(
        !controlled.contains(&"win-conflict-b".to_owned()),
        "other-output conflict holds: {controlled:?}"
    );
    for win in ["win-conflict-a", "win-conflict-b"] {
        assert!(
            reply
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.row == RowDiagnosticLane::Window
                    && diagnostic.id.as_deref() == Some(win)
                    && diagnostic.kind == RowDiagnosticKind::ConflictingFact),
            "conflicting-fact diagnostic for {win}: {:?}",
            reply.diagnostics
        );
    }
    for win in ["win-healthy-a", "win-healthy-b"] {
        assert!(
            !reply
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.row == RowDiagnosticLane::Window
                    && diagnostic.id.as_deref() == Some(win)),
            "healthy sibling mints no hold: {win}: {:?}",
            reply.diagnostics
        );
    }
    let members = engine.logical_members_all();
    assert!(members.iter().any(|member| member.0 == "win-healthy-a"));
    assert!(members.iter().any(|member| member.0 == "win-healthy-b"));
    assert!(!members.iter().any(|member| member.0 == "win-conflict-a"));
    assert!(!members.iter().any(|member| member.0 == "win-conflict-b"));
}

#[test]
fn conflicting_sticky_never_masks_structure_or_invalid_precedence() {
    use crate::world_report::{MAX_METADATA_TEXT_BYTES, NativeWorkspaceFact};
    use crate::world_report_v4_checks::{grade_conflicting_windows_v4, grade_invalid_windows_v4};
    // Structural violations refuse instead of grading around the conflict.
    let mut dup_a = fact("win-dup", "out-a", 0);
    dup_a.sticky = Some(true);
    let mut dup_b = fact("win-dup", "out-a", 200);
    dup_b.sticky = Some(false);
    let dup_report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![dup_a, dup_b],
        vec![],
        "",
        None,
        None,
    );
    assert!(
        grade_conflicting_windows_v4(&dup_report, &[], false, true).is_none(),
        "duplicate ids never grade to a hold"
    );
    for (label, mut bad) in [
        ("owner", {
            let mut row = fact("win-bad-owner", "out-a", 0);
            row.sticky = Some(false);
            row.owner = Some(WindowId("has space".to_owned()));
            row
        }),
        ("parent", {
            let mut row = fact("win-bad-parent", "out-a", 0);
            row.sticky = Some(true);
            row.parent = Some(WindowId("has space".to_owned()));
            row
        }),
        ("native", {
            let mut row = fact("win-bad-native", "out-a", 0);
            row.sticky = Some(false);
            row.native_workspace = NativeWorkspaceFact::Value("has space".to_owned());
            row
        }),
    ] {
        let report = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![bad.clone()],
            vec![],
            "",
            None,
            None,
        );
        assert!(
            grade_conflicting_windows_v4(&report, &[], false, true).is_none(),
            "{label} violation never grades to a hold"
        );
        bad.window = WindowId(format!("win-bad-{label}-engine"));
        let engine_report = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![bad],
            vec![],
            "",
            None,
            None,
        );
        let mut engine = engine_with(false);
        assert!(
            matches!(
                engine.handle_report(&engine_report, &[]).0,
                WorldOutcome::Rejected { .. }
            ),
            "{label} violation refuses through the engine"
        );
    }
    // Invalid-metadata precedence: an overbound row carrying a conflicting
    // sticky belongs to the invalid grader, never the conflicting grader.
    let mut overbound = fact("win-over", "out-a", 0);
    overbound.sticky = Some(true);
    overbound.title = Some("x".repeat(MAX_METADATA_TEXT_BYTES + 1));
    let raw = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![overbound],
        vec![],
        "",
        None,
        None,
    );
    assert!(
        grade_conflicting_windows_v4(&raw, &[], false, true).is_none(),
        "gradable invalid row never double-holds as conflicting"
    );
    let graded = grade_invalid_windows_v4(&raw).expect("overbound grades invalid");
    assert_eq!(graded.windows.len(), 0);
    assert_eq!(graded.held_windows.len(), 1);
    assert_eq!(graded.held_windows[0].kind, RowDiagnosticKind::InvalidFact);
    let sticky = graded.held_windows[0]
        .fields
        .iter()
        .find(|(key, _)| key == "sticky")
        .expect("sticky retained");
    assert_eq!(
        sticky.1,
        crate::connection::RawSettingValue::Bool(true),
        "raw true retained exactly, got {:?}",
        sticky.1
    );
    assert!(
        grade_conflicting_windows_v4(&graded, &[], false, true).is_none(),
        "already-held invalid row never re-grades"
    );
    // Ordinal preservation across successive conflicts: the first held row
    // keeps ordinal 0 with its exact Bool(false), the second takes the next
    // free slot with its exact Bool(true).
    let mut first = fact("win-first", "out-a", 0);
    first.sticky = Some(false);
    let base = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![first],
        vec![],
        "",
        None,
        None,
    );
    let mut staged = grade_conflicting_windows_v4(&base, &[], false, true).expect("first grades");
    assert_eq!(staged.held_windows.len(), 1);
    assert_eq!(staged.held_windows[0].ordinal, 0);
    let mut second = fact("win-second", "out-a", 200);
    second.sticky = Some(true);
    staged.windows.push(second);
    let restaged = grade_conflicting_windows_v4(&staged, &[], false, true).expect("second grades");
    assert_eq!(restaged.held_windows.len(), 2);
    assert_eq!(restaged.held_windows[0].ordinal, 0);
    assert_eq!(restaged.held_windows[1].ordinal, 1);
    for held in &restaged.held_windows {
        assert_eq!(held.kind, RowDiagnosticKind::ConflictingFact);
        let sticky = held
            .fields
            .iter()
            .find(|(key, _)| key == "sticky")
            .expect("sticky retained");
        let expected = if held.window.0 == "win-first" {
            crate::connection::RawSettingValue::Bool(false)
        } else {
            crate::connection::RawSettingValue::Bool(true)
        };
        assert_eq!(sticky.1, expected, "exact Bool for {}", held.window.0);
    }
}

#[test]
fn conflict_recovery_consumes_pending_and_seals_admission() {
    let mut engine = engine_with(false);
    let mut conflicted = fact("win-r", "out-a", 0);
    conflicted.sticky = Some(false);
    let mut conflict_report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![conflicted],
        vec![],
        "",
        None,
        None,
    );
    conflict_report.intent_records = vec![IntentRecord {
        win: WindowId("win-r".to_owned()),
        data: encode_intent_data(&IntentData::new(Some(false), None)),
    }];
    let reply = accepted_reply(&mut engine, &conflict_report);
    assert!(reply.intent_writes.is_empty(), "{:?}", reply.intent_writes);
    assert!(
        engine
            .pending_intent
            .contains_key(&WindowId("win-r".to_owned())),
        "conflict retains pending tile intent"
    );
    assert!(
        !engine
            .intent_admitted
            .contains(&WindowId("win-r".to_owned())),
        "conflict never latches admission"
    );
    // Composed-null recovery tiles healthily: no import save is minted, the
    // pending tile intent is consumed, and admission seals the lifetime.
    let mut recovered = fact("win-r", "out-a", 0);
    recovered.sticky = None;
    let recovery = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![recovered],
        vec![],
        "",
        None,
        None,
    );
    let reply = accepted_reply(&mut engine, &recovery);
    assert!(reply.intent_writes.is_empty(), "{:?}", reply.intent_writes);
    assert!(
        controlled(&reply).contains(&"win-r".to_owned()),
        "recovered null tiles: {:?}",
        controlled(&reply)
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-r".to_owned())),
        "recovery consumes pending tile intent"
    );
    assert!(
        engine
            .intent_admitted
            .contains(&WindowId("win-r".to_owned())),
        "recovery seals admission"
    );
    assert!(
        engine.staged.held_windows.is_empty(),
        "recovery holds nothing: {:?}",
        engine.staged.held_windows
    );
    // An exact duplicate of the recovery returns the cached reply.
    match engine.handle_report(&recovery, &[]).0 {
        WorldOutcome::Duplicate(cached) => assert_eq!(cached, reply),
        other => panic!("expected duplicate, got {other:?}"),
    }
    // A late changed record for the admitted lifetime never overwrites and
    // mints no save.
    let mut late_row = fact("win-r", "out-a", 0);
    late_row.sticky = None;
    let mut late = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![late_row],
        vec![],
        "",
        None,
        None,
    );
    late.intent_records = vec![IntentRecord {
        win: WindowId("win-r".to_owned()),
        data: encode_intent_data(&IntentData::new(Some(true), None)),
    }];
    let reply = accepted_reply(&mut engine, &late);
    assert!(reply.intent_writes.is_empty(), "{:?}", reply.intent_writes);
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-r".to_owned())),
        "admitted lifetime ignores late rows"
    );
    assert!(
        engine
            .intent_admitted
            .contains(&WindowId("win-r".to_owned())),
        "admission persists through late rows"
    );
    // Same request sequence with a changed raw sticky value is rejected,
    // even though the sequence itself is already consumed.
    let mut changed_row = fact("win-r", "out-a", 0);
    changed_row.sticky = Some(true);
    let mut changed = late.clone();
    changed.windows = vec![changed_row];
    match engine.handle_report(&changed, &[]).0 {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-reused"),
        other => panic!("expected reused rejection, got {other:?}"),
    }
}

#[test]
fn native_false_null_toggle_float_save_preserved() {
    use crate::boundary::CoreCommand;
    use crate::world_report::WorldCommand;
    let mut engine = engine_with(false);
    let mut win_a = fact("win-a", "out-a", 0);
    win_a.sticky = None;
    let mut win_b = fact("win-b", "out-a", 400);
    win_b.sticky = None;
    accepted_reply(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![win_a.clone(), win_b.clone()],
            vec![],
            "",
            None,
            None,
        ),
    );
    let tiled = engine.engine.domain_keys().iter().any(|key| {
        engine.engine.session(key).is_some_and(|s| {
            s.snapshot()
                .windows
                .iter()
                .any(|l| l.window == WindowId("win-a".to_owned()))
        })
    });
    assert!(tiled, "composed-null win-a tiles before toggle");
    let toggled = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![win_a, win_b],
        vec![],
        "",
        Some(2),
        Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: "win-a".to_owned(),
            float_rect: None,
        })),
    );
    let reply = accepted_reply(&mut engine, &toggled);
    assert_eq!(reply.intent_writes.len(), 1, "{reply:?}");
    assert_eq!(reply.intent_writes[0].win, WindowId("win-a".to_owned()));
    assert_eq!(
        reply.intent_writes[0].data.as_deref(),
        Some(encode_intent_data(&IntentData::new(Some(true), None)).as_str()),
        "{:?}",
        reply.intent_writes
    );
    let floated = engine.engine.domain_keys().iter().any(|key| {
        engine
            .engine
            .session(key)
            .is_some_and(|s| s.is_exception(&WindowId("win-a".to_owned())))
    });
    assert!(floated, "toggle floats the composed-null window");
    match &reply.command_result {
        Some(crate::world_report::CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window.0, "win-a");
            assert!(rect.is_some(), "float carries a rect");
            assert!(!no_space);
        }
        other => panic!("expected float result, got {other:?}"),
    }
    // Native-supported null holds sticky-dependent classification without
    // blocking an unrelated healthy sibling.
    let mut supported = engine_with(true);
    let mut unknown = fact("win-null", "out-a", 0);
    unknown.sticky = None;
    let mut healthy = fact("win-healthy", "out-a", 400);
    healthy.sticky = Some(false);
    let reply = accepted_reply(
        &mut supported,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![unknown, healthy],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        controlled(&reply).contains(&"win-healthy".to_owned()),
        "supported known-bool sibling tiles: {:?}",
        controlled(&reply)
    );
    assert!(
        !controlled(&reply).contains(&"win-null".to_owned()),
        "supported null holds: {:?}",
        controlled(&reply)
    );
    assert!(
        !reply
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.row == RowDiagnosticLane::Window
                && diagnostic.id.as_deref() == Some("win-healthy")
                && diagnostic.kind == RowDiagnosticKind::ConflictingFact),
        "supported null hold never conflicts the sibling: {:?}",
        reply.diagnostics
    );
}
