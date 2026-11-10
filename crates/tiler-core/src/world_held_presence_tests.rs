//! Held-presence and safety-exclusion regressions (HIGH2+MED3):
//! narrowly held rows keep live membership/homes/slots with no writes,
//! exclusions never enter or keep membership, slots, or control.

use crate::boundary::CoreCommand;
use crate::connection::FactName;
use crate::directional::{OutputId, WindowId};
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    HeldWindowRow, RowDiagnosticKind, ScopeAuthority, WorldCommand, WorldReplyV4, WorldReportV4,
};
use crate::world_v4_support_tests::{accepted, fact, report_full, screen};

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn born_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            birth_clear: true,
            // Fixtures carry native sticky and maximize facts; the held
            // lane under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

fn title_unavailable_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            unavailable_facts: vec![FactName::Title],
            // Fixtures carry native sticky and maximize facts; the held
            // lane under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

fn overlong_title() -> String {
    "x".repeat(crate::world_report::MAX_METADATA_TEXT_BYTES + 1)
}

// The legacy lane echoed views [("out-a", "ws-1"), ("out-b", "ws-1")] here;
// that echo is inert (membership/hold/diagnostic rows carry the proof) and
// has no v4 slot, so the v4 port admits with an empty views lane.
fn two_output_report(seq: u64, windows: Vec<crate::world_report::WindowFact>) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0), screen("out-b", 800)],
        windows,
        Vec::new(),
        "",
        None,
        None,
    )
}

fn controlled_row(reply: &WorldReplyV4, window: &str) -> bool {
    reply.windows.iter().any(|row| {
        row.lifetime.0 == window
            && row.membership_authority == ScopeAuthority::Controlled
            && row.frame_authority == ScopeAuthority::Controlled
    })
}

fn hold_row(reply: &WorldReplyV4, window: &str) -> bool {
    reply.windows.iter().any(|row| {
        row.lifetime.0 == window
            && matches!(row.membership_authority, ScopeAuthority::Hold(_))
            && matches!(row.frame_authority, ScopeAuthority::Hold(_))
    })
}

fn scope_hold(reply: &WorldReplyV4, output: &str, workspace: &str) -> bool {
    reply.scopes.iter().any(|status| {
        status
            .scope
            .covers_workspace(&OutputId(output.to_owned()), workspace)
            && matches!(status.authority, ScopeAuthority::Hold(_))
    })
}

fn scope_controlled(reply: &WorldReplyV4, output: &str, workspace: &str) -> bool {
    reply.scopes.iter().any(|status| {
        status
            .scope
            .covers_workspace(&OutputId(output.to_owned()), workspace)
            && status.authority == ScopeAuthority::Controlled
    })
}

#[test]
fn held_member_keeps_home_sibling_scope_stays_controlled() {
    // Complete admit, then a malformed (overlong title) row for one member:
    // the held scope holds, the independent sibling scope stays controlled,
    // and recovery reuses the same home with no fresh admission focus.
    let mut engine = engine();
    let reply = accepted(
        &mut engine,
        &two_output_report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        ),
    );
    assert!(controlled_row(&reply, "win-a"));
    assert!(controlled_row(&reply, "win-b"));
    let home = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();
    let spent = engine.admission_focus_spent.clone();

    let mut held_fact = fact("win-a", "out-a", 0);
    held_fact.title = Some(overlong_title());
    let reply = accepted(
        &mut engine,
        &two_output_report(2, vec![held_fact, fact("win-b", "out-b", 800)]),
    );
    // Retained membership and home survive the complete held row.
    assert_eq!(
        engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("held keeps member")
            .home,
        home
    );
    assert!(scope_hold(&reply, &home.output.0, &home.workspace.0));
    assert!(hold_row(&reply, "win-a"));
    // The independent sibling scope stays controlled with a live row.
    assert!(scope_controlled(&reply, "out-b", "ws-1"));
    assert!(controlled_row(&reply, "win-b"));
    // Invalid-vs-conflicting kind survives scope resolution.
    assert!(reply.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == RowDiagnosticKind::InvalidFact
            && diagnostic.id.as_deref() == Some("win-a")
    }));
    assert!(reply.effects.is_empty());

    // Recovery reuses the same lifetime: same home, no fresh focus intent.
    let reply = accepted(
        &mut engine,
        &two_output_report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        ),
    );
    assert_eq!(
        engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("recovered")
            .home,
        home
    );
    assert!(controlled_row(&reply, "win-a"));
    assert!(controlled_row(&reply, "win-b"));
    assert_eq!(engine.admission_focus_spent, spent);
    assert!(reply.effects.is_empty());
}

#[test]
fn held_newcomer_has_no_membership_or_row() {
    // A malformed newcomer beside a healthy sibling: no membership, no row,
    // and the shared scope stays controlled.
    let mut engine = engine();
    let reply = accepted(
        &mut engine,
        &two_output_report(1, vec![fact("win-healthy", "out-b", 800)]),
    );
    assert!(controlled_row(&reply, "win-healthy"));

    let mut newcomer = fact("win-new", "out-b", 810);
    newcomer.title = Some(overlong_title());
    let reply = accepted(
        &mut engine,
        &two_output_report(2, vec![fact("win-healthy", "out-b", 800), newcomer]),
    );
    assert!(!engine.members.contains_key(&WindowId("win-new".to_owned())));
    assert!(!reply.windows.iter().any(|row| row.lifetime.0 == "win-new"));
    assert!(scope_controlled(&reply, "out-b", "ws-1"));
    assert!(controlled_row(&reply, "win-healthy"));
}

#[test]
fn born_binding_survives_hold_and_recovers_same_operation() {
    // A maximized birth binds commandlessly; a held row keeps the binding
    // with no desire, and recovery republishes the same operation id.
    let mut engine = born_engine();
    let mut maximized = fact("win-born", "out-a", 0);
    maximized.maximized = Some(true);
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    assert_eq!(engine.retained_born.len(), 1);
    let operation = engine.retained_born[0].operation_id.clone();
    let desire = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-born")
        .and_then(|row| row.clear_operation.clone())
        .expect("birth desire");
    assert_eq!(desire, operation);

    // Held: binding kept, desire withheld, no remint.
    let mut held_fact = fact("win-born", "out-a", 0);
    held_fact.maximized = Some(true);
    held_fact.title = Some(overlong_title());
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![held_fact],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    assert_eq!(engine.retained_born.len(), 1);
    assert_eq!(engine.retained_born[0].operation_id, operation);
    assert!(
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-born")
            .and_then(|row| row.clear_operation.clone())
            .is_none()
    );

    // Recovery republishes the same operation, never a new binding.
    let mut recovered = fact("win-born", "out-a", 0);
    recovered.maximized = Some(true);
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![recovered],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    assert_eq!(engine.retained_born.len(), 1);
    assert_eq!(engine.retained_born[0].operation_id, operation);
    assert_eq!(
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-born")
            .and_then(|row| row.clear_operation.clone())
            .as_deref(),
        Some(operation.as_str())
    );
}

#[test]
fn explicit_close_after_hold_drops_member() {
    // Held retention never pins a lifetime past an explicit close.
    let mut engine = engine();
    accepted(
        &mut engine,
        &two_output_report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        ),
    );
    let mut held_fact = fact("win-a", "out-a", 0);
    held_fact.title = Some(overlong_title());
    accepted(
        &mut engine,
        &two_output_report(2, vec![held_fact, fact("win-b", "out-b", 800)]),
    );
    assert!(engine.members.contains_key(&WindowId("win-a".to_owned())));

    let mut closed = two_output_report(3, vec![fact("win-b", "out-b", 800)]);
    closed.closed = vec![WindowId("win-a".to_owned())];
    let reply = accepted(&mut engine, &closed);
    assert!(!engine.members.contains_key(&WindowId("win-a".to_owned())));
    assert!(!reply.windows.iter().any(|row| row.lifetime.0 == "win-a"));
    assert!(controlled_row(&reply, "win-b"));
}

#[test]
fn held_plus_closed_same_id_refuses() {
    // One id cannot be both observed-held and reported closed.
    let mut engine = engine();
    let mut held_fact = fact("win-a", "out-a", 0);
    held_fact.title = Some(overlong_title());
    let mut report = two_output_report(1, vec![held_fact, fact("win-b", "out-b", 800)]);
    report.closed = vec![WindowId("win-a".to_owned())];
    assert!(matches!(
        engine.handle_report(&report),
        WorldOutcome::Rejected { .. }
    ));
}

#[test]
fn output_less_held_row_resolves_retained_home_hold() {
    // A hand-carried output-less held row (corpus shape with no authority)
    // resolves its scope from the retained home and holds it narrowly.
    let mut engine = engine();
    accepted(
        &mut engine,
        &two_output_report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        ),
    );
    let home = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();

    let mut report = two_output_report(2, vec![fact("win-b", "out-b", 800)]);
    report.held_windows = vec![HeldWindowRow {
        window: WindowId("win-a".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let reply = accepted(&mut engine, &report);
    assert_eq!(
        engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("held keeps member")
            .home,
        home
    );
    assert!(scope_hold(&reply, &home.output.0, &home.workspace.0));
    assert!(hold_row(&reply, "win-a"));
    // The rebuilt diagnostic keeps the invalid-vs-conflicting reason.
    assert!(reply.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == RowDiagnosticKind::InvalidFact
            && diagnostic.id.as_deref() == Some("win-a")
    }));
}

#[test]
fn conflicting_title_holds_member_and_recovers() {
    // A listed-unavailable fact carried non-null grades to ConflictingFact
    // and keeps the member like the invalid lane.
    let mut engine = title_unavailable_engine();
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 40)],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    assert!(controlled_row(&reply, "win-a"));
    let home = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();

    let mut conflict = fact("win-a", "out-a", 0);
    conflict.title = Some("v1".to_owned());
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![conflict, fact("win-b", "out-a", 40)],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    assert_eq!(
        engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("conflict keeps member")
            .home,
        home
    );
    assert!(scope_hold(&reply, &home.output.0, &home.workspace.0));
    assert!(reply.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == RowDiagnosticKind::ConflictingFact
            && diagnostic.id.as_deref() == Some("win-a")
    }));

    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 40)],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    assert!(controlled_row(&reply, "win-a"));
}

fn owned_fact(window: &str, output: &str, x: i32) -> crate::world_report::WindowFact {
    crate::world_report::WindowFact {
        owned: Some(true),
        ..fact(window, output, x)
    }
}

#[test]
fn known_exclusion_never_members_or_controls() {
    // Safety-excluded windows never enter membership, keep no slot, and
    // authorize no focus, flight, or tiling writes, complete or gapped.
    let mut engine = engine();
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), owned_fact("win-x", "out-a", 40)],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    assert!(engine.members.contains_key(&WindowId("win-t".to_owned())));
    assert!(!engine.members.contains_key(&WindowId("win-x".to_owned())));
    assert!(!reply.windows.iter().any(|row| row.lifetime.0 == "win-x"));
    assert!(reply.effects.is_empty());
    assert!(reply.transfers.is_empty());
    assert!(controlled_row(&reply, "win-t"));

    // An incomplete repeat drops nothing and still publishes nothing.
    let mut gapped = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-t", "out-a", 0), owned_fact("win-x", "out-a", 40)],
        Vec::new(),
        "",
        None,
        None,
    );
    gapped.inventory_complete = false;
    let reply = accepted(&mut engine, &gapped);
    assert!(!engine.members.contains_key(&WindowId("win-x".to_owned())));
    assert!(!reply.windows.iter().any(|row| row.lifetime.0 == "win-x"));
    assert!(reply.effects.is_empty());

    // Fixed-hint recovery floats untouched with no placement or clear ops.
    let mut fixed = fact("win-x", "out-a", 40);
    fixed.owned = Some(false);
    fixed.hints = Some(WindowSizeHints {
        min_w: Some(40),
        min_h: Some(90),
        max_w: Some(40),
        max_h: Some(90),
    });
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), fixed],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-x")
        .expect("float row");
    assert_eq!(row.membership_authority, ScopeAuthority::Untouched);
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert!(row.frame_operation.is_none());
    assert!(row.clear_operation.is_none());

    // Tiled-hint recovery tiles controlled with an allocation. A fresh
    // excluded window recovers straight to tiled: the fixed latch above
    // intentionally keeps win-x floating, so this step uses win-y.
    let mut tiled = fact("win-y", "out-a", 200);
    tiled.owned = Some(false);
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), tiled],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-y")
        .expect("tiled row");
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.allocation.is_some());
}

#[test]
fn own_process_windows_never_member() {
    // The connection's own process fact drops before closes: never a
    // member, never a row, complete or gapped.
    let mut engine = engine();
    let mut own = fact("win-self", "out-a", 40);
    own.process_id = Some("test-own-pid".to_owned());
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-t", "out-a", 0), own],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine
            .members
            .contains_key(&WindowId("win-self".to_owned()))
    );
    assert!(!reply.windows.iter().any(|row| row.lifetime.0 == "win-self"));
    assert!(reply.effects.is_empty());
    assert!(reply.transfers.is_empty());

    let mut own = fact("win-self", "out-a", 40);
    own.process_id = Some("test-own-pid".to_owned());
    let mut gapped = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-t", "out-a", 0), own],
        Vec::new(),
        "",
        None,
        None,
    );
    gapped.inventory_complete = false;
    let reply = accepted(&mut engine, &gapped);
    assert!(
        !engine
            .members
            .contains_key(&WindowId("win-self".to_owned()))
    );
    assert!(!reply.windows.iter().any(|row| row.lifetime.0 == "win-self"));
}

fn fixed_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(400),
        min_h: Some(300),
        max_w: Some(400),
        max_h: Some(300),
    }
}

fn fixed_fact(window: &str, output: &str, x: i32) -> crate::world_report::WindowFact {
    crate::world_report::WindowFact {
        hints: Some(fixed_hints()),
        ..fact(window, output, x)
    }
}

#[test]
fn excluded_fixed_tile_override_restores_after_safety_gap() {
    // Fixed admit floats automatic, ToggleFloat unfloat records the
    // explicit tile override, then an owned safety exclusion retires the
    // slot (complete and gapped): no membership, row, control, focus,
    // effect, or transfer while excluded. Lifting with current fixed
    // hints restores the same-lifetime tiled override with no fresh
    // focus; an explicit close resets it so the reused id floats again.
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            Vec::new(),
            "w0",
            None,
            None,
        ),
    );
    assert!(arrival.effects.is_empty());
    let arrival_row = arrival
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "wf")
        .expect("automatic float row");
    assert!(arrival_row.floating);
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("wf".to_owned()))
    );

    let mut floated = fact("wf", "out-a", 200);
    floated.transient = Some(true);
    floated.hints = Some(fixed_hints());
    let untiled = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), floated],
            Vec::new(),
            "w0",
            Some(10),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "wf".to_owned(),
                float_rect: None,
            })),
        ),
    );
    match &untiled.command_result {
        Some(crate::world_report::CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window.0, "wf");
            assert_eq!(*rect, None);
            assert!(!*no_space);
        }
        other => panic!("expected unfloat tile result, got {other:?}"),
    }
    assert!(untiled.effects.is_empty());
    assert!(
        !untiled
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "wf")
            .expect("tiled row")
            .floating
    );
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("wf".to_owned()))
    );

    // Owned safety exclusion, complete inventory: slot retires but the
    // neutral ledger keeps the approved intent.
    let mut excluded = fixed_fact("wf", "out-a", 200);
    excluded.owned = Some(true);
    let denied = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), excluded],
            Vec::new(),
            "w0",
            None,
            None,
        ),
    );
    assert!(!engine.members.contains_key(&WindowId("wf".to_owned())));
    assert!(!denied.windows.iter().any(|row| row.lifetime.0 == "wf"));
    assert!(denied.effects.is_empty());
    assert!(denied.transfers.is_empty());
    assert!(!denied.windows.iter().any(
        |row| row.lifetime.0 == "wf" && row.membership_authority == ScopeAuthority::Controlled
    ));
    assert!(
        engine
            .excluded_intent
            .get(&WindowId("wf".to_owned()))
            .is_some_and(|intent| intent.tile_override)
    );

    // Gapped repeat changes nothing and keeps the ledger.
    let mut gapped_excluded = fixed_fact("wf", "out-a", 200);
    gapped_excluded.owned = Some(true);
    let mut gapped = report_full(
        5,
        5,
        vec![screen("out-a", 0)],
        vec![
            fact("w0", "out-a", 0),
            fact("w1", "out-a", 100),
            gapped_excluded,
        ],
        Vec::new(),
        "w0",
        None,
        None,
    );
    gapped.inventory_complete = false;
    let denied_gapped = accepted(&mut engine, &gapped);
    assert!(!engine.members.contains_key(&WindowId("wf".to_owned())));
    assert!(
        !denied_gapped
            .windows
            .iter()
            .any(|row| row.lifetime.0 == "wf")
    );
    assert!(denied_gapped.effects.is_empty());
    assert!(
        engine
            .excluded_intent
            .get(&WindowId("wf".to_owned()))
            .is_some_and(|intent| intent.tile_override)
    );

    // Lift with current fixed hints: same lifetime tiles controlled
    // with the override restored and no fresh admission focus.
    let lifted = accepted(
        &mut engine,
        &report_full(
            6,
            6,
            vec![screen("out-a", 0)],
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            Vec::new(),
            "w0",
            None,
            None,
        ),
    );
    let row = lifted
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "wf")
        .expect("restored row");
    assert!(!row.floating);
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("wf".to_owned()))
    );
    assert!(engine.members.contains_key(&WindowId("wf".to_owned())));
    assert!(lifted.effects.is_empty());

    // Explicit native close ends the lifetime: the ledger clears and the
    // reused id with fixed hints floats automatic again.
    let mut closed = report_full(
        7,
        7,
        vec![screen("out-a", 0)],
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        Vec::new(),
        "w0",
        None,
        None,
    );
    closed.closed = vec![WindowId("wf".to_owned())];
    accepted(&mut engine, &closed);
    assert!(
        !engine
            .excluded_intent
            .contains_key(&WindowId("wf".to_owned()))
    );
    let readmitted = accepted(
        &mut engine,
        &report_full(
            8,
            8,
            vec![screen("out-a", 0)],
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            Vec::new(),
            "w0",
            None,
            None,
        ),
    );
    let row = readmitted
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "wf")
        .expect("reused row");
    assert!(row.floating);
    assert!(
        !engine
            .engine
            .has_fixed_tile_override_any(&WindowId("wf".to_owned()))
    );
}

#[test]
fn unknown_sticky_migration_rejects_without_partial_move() {
    // Raw unknown sticky (None) is never proven non-sticky: a tiled
    // directional migration refuses with the migration vocabulary and
    // no retained flight, mapping, or membership mutation. The
    // unreadable-hold fence refuses first here; the command gates below
    // cover the fact-less member path that never stages a hold.
    use crate::world_report::TransferKind;
    let mut eng = engine();
    accepted(
        &mut eng,
        &two_output_report(1, vec![fact("win-a", "out-a", 0)]),
    );
    let mapping_before = eng.mapping.clone();
    let mut unknown = fact("win-a", "out-a", 0);
    unknown.sticky = None;
    let mut report = two_output_report(2, vec![unknown]);
    report.focused_global = "win-a".to_owned();
    report.foreground_known = true;
    report.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    report.command_seq = Some(11);
    report.command = Some(WorldCommand::MigrateDirectional {
        direction: "right".to_owned(),
    });
    match eng.handle_report(&report) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(crate::world_report::CommandResult::Rejected {
                kind: crate::world_report::CommandRejectKind::MigrationRefused
            }),
            "unknown sticky must refuse"
        ),
        other => panic!("unknown sticky must refuse, got {other:?}"),
    }
    assert_eq!(eng.mapping, mapping_before);
    assert!(
        eng.retained
            .iter()
            .all(|retained| !matches!(retained.target.kind, TransferKind::Migration))
    );
    assert!(eng.members.contains_key(&WindowId("win-a".to_owned())));

    // Proven sticky stays physical under the existing stay policy:
    // the workspace migrates with an empty member set while the
    // sticky lifetime never moves. Unknown (above) refuses instead.
    let mut sticky_engine = engine();
    accepted(
        &mut sticky_engine,
        &two_output_report(1, vec![fact("win-a", "out-a", 0)]),
    );
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    let mut report = two_output_report(2, vec![sticky]);
    report.focused_global = "win-a".to_owned();
    report.foreground_known = true;
    report.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    report.command_seq = Some(11);
    report.command = Some(WorldCommand::MigrateDirectional {
        direction: "right".to_owned(),
    });
    match sticky_engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => match &reply.command_result {
            Some(crate::world_report::CommandResult::Migration { members, .. }) => {
                assert!(members.is_empty(), "sticky stays: {members:?}");
            }
            other => panic!("expected migration result, got {other:?}"),
        },
        other => panic!("proven sticky stays, got {other:?}"),
    }

    let mut proven_engine = engine();
    accepted(
        &mut proven_engine,
        &two_output_report(1, vec![fact("win-a", "out-a", 0)]),
    );
    let mut report = two_output_report(2, vec![fact("win-a", "out-a", 0)]);
    report.focused_global = "win-a".to_owned();
    report.foreground_known = true;
    report.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    report.command_seq = Some(11);
    report.command = Some(WorldCommand::MigrateDirectional {
        direction: "right".to_owned(),
    });
    match proven_engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => {
            assert!(reply.transfers.iter().any(|transfer| matches!(
                transfer.kind,
                crate::world_report::TransferKind::Migration
            )));
        }
        other => panic!("proven non-sticky must apply, got {other:?}"),
    }
}

#[test]
fn swap_source_unknown_sticky_flags_only_unproven() {
    // The swap/migration unknown gate: proven sticky and proven
    // non-sticky never flag (existing stay/move policy owns them);
    // a member with a null sticky fact or no staged fact at all flags,
    // so the command refuses instead of swapping a partial subset.
    use crate::directional::WorkspaceId;
    use crate::session::DomainKey;
    let key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let member = |window: &str| {
        (
            WindowId(window.to_owned()),
            crate::world_members::MemberState {
                home: key.clone(),
                floating: false,
                minimized: false,
                geometry: None,
            },
        )
    };
    // All proven non-sticky: no flag.
    let mut proven_engine = engine();
    let (id, state) = member("win-a");
    proven_engine.members.insert(id.clone(), state);
    let mut proven = fact("win-a", "out-a", 0);
    proven.sticky = Some(false);
    proven_engine.staged.facts.insert(id, proven);
    assert!(!WorldEngine::swap_source_unknown_sticky(
        &proven_engine,
        &key
    ));
    // Proven sticky: no flag (stay policy, not a refusal).
    let mut sticky_engine = engine();
    let (id, state) = member("win-a");
    sticky_engine.members.insert(id.clone(), state);
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    sticky_engine.staged.facts.insert(id, sticky);
    assert!(!WorldEngine::swap_source_unknown_sticky(
        &sticky_engine,
        &key
    ));
    // Null sticky fact: flags.
    let mut unknown_engine = engine();
    let (id, state) = member("win-a");
    unknown_engine.members.insert(id.clone(), state);
    let mut unknown = fact("win-a", "out-a", 0);
    unknown.sticky = None;
    unknown_engine.staged.facts.insert(id, unknown);
    assert!(WorldEngine::swap_source_unknown_sticky(
        &unknown_engine,
        &key
    ));
    // Member with no staged fact at all: flags (never silently moved).
    let mut missing_engine = engine();
    let (id, state) = member("win-a");
    missing_engine.members.insert(id, state);
    assert!(WorldEngine::swap_source_unknown_sticky(
        &missing_engine,
        &key
    ));
}
