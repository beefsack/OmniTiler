//! Capacity-float diagnostic and explicit float-placement lifecycle at the
//! world boundary: exhaustion marks capacity floats with a diagnostic-only
//! origin, ordinary floats behave identically, and ToggleFloat placement is
//! an explicit one-shot operation with observation/supersession/refusal
//! settlement. Logical-first: screens plus window facts only.

use tiler_core::boundary::CoreCommand;
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, FloatOrigin, ScopeAuthority, ScreenReport,
    WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_sized(output: &str, x: i32, w: i32, h: i32, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, w, h)),
        work_area: Some(rect(x, 0, w, h)),
        gap: 8,
        outer_gap: 0,
        complete: true,
        primary,
    }
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(rect(x, 0, 40, 90)),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
        minimized: false,
        observed_workspace: None,
    }
}

fn report(
    request_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    feedback: Vec<tiler_core::world_report::AttemptFeedback>,
) -> WorldReport {
    WorldReport {
        restored: None,
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: Vec::new(),
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: focused.to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback,
    }
}

fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReport,
) -> tiler_core::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn row_of<'a>(
    reply: &'a tiler_core::world_report::WorldReply,
    id: &str,
) -> &'a tiler_core::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == id)
        .unwrap_or_else(|| panic!("missing row {id}"))
}

fn tiny_screens() -> Vec<ScreenReport> {
    vec![screen_sized("out-a", 0, 10, 10, true)]
}

#[test]
fn exhaustion_marks_capacity_floats_with_diagnostic_origin_only() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows: Vec<WindowFact> = (0..6)
        .map(|i| fact(&format!("win-{i}"), "out-a", 0))
        .collect();
    let reply = accepted(
        &mut engine,
        &report(1, tiny_screens(), windows, "", None, None, vec![]),
    );
    let floats: Vec<&str> = reply
        .windows
        .iter()
        .filter(|row| row.floating)
        .map(|row| row.lifetime.0.as_str())
        .collect();
    assert!(
        !floats.is_empty(),
        "tiny 10x10 gap-8 exhausts: {:?}",
        reply.windows
    );
    for id in &floats {
        let row = row_of(&reply, id);
        assert_eq!(row.float_origin, Some(FloatOrigin::Capacity), "{id}");
        assert_eq!(row.allocation, None, "{id} takes no tile slot");
        assert_eq!(row.frame_authority, ScopeAuthority::Untouched, "{id}");
        assert_eq!(row.membership_authority, ScopeAuthority::Untouched, "{id}");
    }
    // Tiled survivors carry neither the marker nor the origin.
    let tiled: Vec<_> = reply.windows.iter().filter(|row| !row.floating).collect();
    assert!(!tiled.is_empty(), "some tiles project");
    for row in tiled {
        assert_eq!(row.float_origin, None, "{}", row.lifetime.0);
        assert!(row.allocation.is_some(), "{}", row.lifetime.0);
        assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    }
}

#[test]
fn ordinary_float_matches_capacity_float_except_origin() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    // Roomy screen: one ordinary float fact plus tiled siblings never exhaust.
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    let mut ordinary = fact("win-float", "out-a", 100);
    ordinary.floating = true;
    let reply = accepted(
        &mut engine,
        &report(
            1,
            roomy,
            vec![fact("win-a", "out-a", 0), ordinary],
            "",
            None,
            None,
            vec![],
        ),
    );
    let row = row_of(&reply, "win-float");
    assert!(row.floating, "ordinary float converges: {row:?}");
    assert_eq!(row.float_origin, None, "ordinary carries no origin");
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert_eq!(row.membership_authority, ScopeAuthority::Untouched);
    assert!(row.allocation.is_none());

    // Exhausted screen: capacity floats share the exact authorities.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows: Vec<WindowFact> = (0..6)
        .map(|i| fact(&format!("win-{i}"), "out-a", 0))
        .collect();
    let reply = accepted(
        &mut engine,
        &report(1, tiny_screens(), windows, "", None, None, vec![]),
    );
    let capacity = reply
        .windows
        .iter()
        .find(|row| row.floating)
        .expect("capacity float");
    assert_eq!(capacity.float_origin, Some(FloatOrigin::Capacity));
    assert_eq!(capacity.frame_authority, row.frame_authority);
    assert_eq!(capacity.membership_authority, row.membership_authority);
    assert_eq!(capacity.allocation, None);
}

#[test]
fn normalized_float_echo_persists_without_retile() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows: Vec<WindowFact> = (0..6)
        .map(|i| fact(&format!("win-{i}"), "out-a", 0))
        .collect();
    let first = accepted(
        &mut engine,
        &report(1, tiny_screens(), windows.clone(), "", None, None, vec![]),
    );
    let floats: Vec<String> = first
        .windows
        .iter()
        .filter(|row| row.floating)
        .map(|row| row.lifetime.0.clone())
        .collect();
    assert!(!floats.is_empty());
    // Adapter echoes the classification back floating:true. The observed
    // focus names a tile (reconcile refuses focus-none against a floated
    // observed focus, so the echo path needs a tiled focus).
    let mut echo = windows;
    for window in &mut echo {
        if floats.contains(&window.window.0) {
            window.floating = true;
        }
    }
    let tiled_focus = first
        .windows
        .iter()
        .find(|row| !row.floating)
        .map(|row| row.lifetime.0.clone())
        .expect("tiled survivor");
    let second = accepted(
        &mut engine,
        &report(2, tiny_screens(), echo, &tiled_focus, None, None, vec![]),
    );
    let again: Vec<String> = second
        .windows
        .iter()
        .filter(|row| row.floating)
        .map(|row| row.lifetime.0.clone())
        .collect();
    assert_eq!(floats, again, "echo persists without retile");
}

fn place_float(
    engine: &mut WorldEngine,
    seq: u64,
    windows: Vec<WindowFact>,
    window: &str,
    target: Option<Rect>,
) -> tiler_core::world_report::WorldReply {
    accepted(
        engine,
        &report(
            seq,
            vec![screen_sized("out-a", 0, 800, 560, true)],
            windows,
            "",
            Some(10 + seq),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: window.to_owned(),
                float_rect: target,
            })),
            vec![],
        ),
    )
}

fn two_tiled() -> Vec<WindowFact> {
    vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)]
}

#[test]
fn float_placement_carries_operation_until_observed() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None, vec![]),
    );
    let target = rect(10, 10, 200, 150);
    // A tiled sibling stays behind and keeps its allocation while the
    // mover is excepted (the sole-float case is covered separately).
    let reply = place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    let row = row_of(&reply, "win-a");
    assert!(row.floating, "placement floats: {row:?}");
    assert_eq!(row.frame_operation.as_deref(), Some("op-12"));
    assert_eq!(row.effective_frame, Some(target));
    assert!(matches!(
        reply.command_result,
        Some(tiler_core::world_report::CommandResult::Float { .. })
    ));
    // The tiled sibling keeps its allocation while the mover is excepted.
    let sibling = row_of(&reply, "win-b");
    assert!(sibling.allocation.is_some(), "{sibling:?}");
    // Native confirmation (observed frame equals the target) clears the token.
    let mut confirmed = fact("win-a", "out-a", 0);
    confirmed.rect = Some(target);
    confirmed.floating = true;
    let settled = accepted(
        &mut engine,
        &report(
            3,
            roomy,
            vec![confirmed, fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let row = row_of(&settled, "win-a");
    assert_eq!(row.frame_operation, None, "confirmed: {row:?}");
    assert!(row.floating, "stays floated: {row:?}");
}

#[test]
fn float_toggle_unfloat_withdraws_and_replace_reissues() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None, vec![]),
    );
    let first_target = rect(10, 10, 200, 150);
    let placed = place_float(&mut engine, 2, two_tiled(), "win-a", Some(first_target));
    assert_eq!(
        row_of(&placed, "win-a").frame_operation.as_deref(),
        Some("op-12")
    );
    // ToggleFloat is a toggle: while the placement is outstanding the same
    // command unfloats, withdrawing the token and retiling the window.
    let unfloat = place_float(&mut engine, 3, two_tiled(), "win-a", None);
    let row = row_of(&unfloat, "win-a");
    assert_eq!(row.frame_operation, None, "withdrawn: {row:?}");
    assert!(!row.floating, "retiled: {row:?}");
    assert!(row.allocation.is_some(), "tiled again: {row:?}");
    // A fresh placement afterwards issues under its own operation id.
    let second_target = rect(30, 30, 220, 160);
    let reply = place_float(&mut engine, 4, two_tiled(), "win-a", Some(second_target));
    let row = row_of(&reply, "win-a");
    assert_eq!(row.frame_operation.as_deref(), Some("op-14"));
    assert_eq!(row.effective_frame, Some(second_target));
}

#[test]
fn float_placement_drops_on_newer_native_input_and_proven_refusal() {
    use tiler_core::world_report::{AttemptFeedback, AttemptKind, AttemptOutcome};
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None, vec![]),
    );
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // Native frame matching neither source nor target wins without a fight:
    // the token drops while the float exception itself is retained.
    let mut foreign = fact("win-a", "out-a", 0);
    foreign.rect = Some(rect(400, 300, 210, 160));
    foreign.floating = true;
    let dropped = accepted(
        &mut engine,
        &report(
            3,
            roomy.clone(),
            vec![foreign, fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let row = row_of(&dropped, "win-a");
    assert_eq!(
        row.frame_operation, None,
        "foreign input supersedes: {row:?}"
    );
    assert!(row.floating, "exception retained: {row:?}");
    // Proven refusal settles the replacement too.
    place_float(&mut engine, 4, two_tiled(), "win-a", Some(target));
    let settled = accepted(
        &mut engine,
        &report(
            5,
            roomy,
            two_tiled(),
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-14".to_owned(),
                kind: AttemptKind::Property,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
        ),
    );
    assert_eq!(
        row_of(&settled, "win-a").frame_operation,
        None,
        "refusal settles"
    );
}

fn sole_view(reply: &tiler_core::world_report::WorldReply) -> String {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("logical view")
}

fn domain_key(view: &str) -> tiler_core::session::DomainKey {
    tiler_core::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(view.to_owned()),
    }
}

#[test]
fn sole_toggle_float_publishes_controlled_exception_row() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(
            1,
            roomy.clone(),
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
            vec![],
        ),
    );
    // Floating the sole tiled window leaves an exception-only session: no
    // tree is a valid logical state, so the scope stays Controlled with
    // the explicit float row instead of a constraint-clamp hold.
    let target = rect(10, 10, 200, 150);
    let reply = place_float(
        &mut engine,
        2,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some(target),
    );
    assert!(matches!(
        reply.command_result,
        Some(tiler_core::world_report::CommandResult::Float { .. })
    ));
    let view = sole_view(&reply);
    let scope = reply
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{view}"))
        .expect("scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled, "{scope:?}");
    let row = row_of(&reply, "win-a");
    assert!(row.floating, "{row:?}");
    assert_eq!(row.float_origin, None, "{row:?}");
    assert_eq!(row.frame_operation.as_deref(), Some("op-12"), "{row:?}");
    assert_eq!(row.effective_frame, Some(target), "{row:?}");
    assert_eq!(row.allocation, None, "{row:?}");
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched, "{row:?}");
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Untouched,
        "{row:?}"
    );
    // No tree is invented by publishing: the session stays exception-only.
    let session = engine
        .engine()
        .session(&domain_key(&view))
        .expect("retained session");
    assert!(
        session.tree_for(&domain_key(&view)).is_none(),
        "exception-only session keeps no tree"
    );
}

#[test]
fn sole_float_confirms_and_clears_once() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(
            1,
            roomy.clone(),
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let target = rect(10, 10, 200, 150);
    place_float(
        &mut engine,
        2,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some(target),
    );
    // Native confirmation (observed frame equals the target) clears the
    // token exactly once; the window stays floated under Controlled scope.
    let mut confirmed = fact("win-a", "out-a", 0);
    confirmed.rect = Some(target);
    confirmed.floating = true;
    let settled = accepted(
        &mut engine,
        &report(3, roomy, vec![confirmed], "", None, None, vec![]),
    );
    let view = sole_view(&settled);
    let scope = settled
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{view}"))
        .expect("scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled, "{scope:?}");
    let row = row_of(&settled, "win-a");
    assert_eq!(row.frame_operation, None, "confirmed: {row:?}");
    assert!(row.floating, "stays floated: {row:?}");
    assert_eq!(row.effective_frame, Some(target), "{row:?}");
}

#[test]
fn all_float_scope_stays_controlled() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    // Fresh seed of only ordinary floats (plus one automatic fixed float):
    // the session is exception-only from birth, never a hold.
    let mut ordinary = fact("win-u", "out-a", 100);
    ordinary.floating = true;
    let mut fixed = fact("win-x", "out-a", 200);
    fixed.floating = true;
    fixed.fixed_auto = true;
    fixed.hints = WindowSizeHints {
        min_w: Some(40),
        min_h: Some(90),
        max_w: Some(40),
        max_h: Some(90),
    };
    let reply = accepted(
        &mut engine,
        &report(1, roomy, vec![ordinary, fixed], "", None, None, vec![]),
    );
    let view = sole_view(&reply);
    let scope = reply
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{view}"))
        .expect("scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled, "{scope:?}");
    for win in ["win-u", "win-x"] {
        let row = row_of(&reply, win);
        assert!(row.floating, "{win}: {row:?}");
        assert_eq!(row.allocation, None, "{win}: {row:?}");
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Untouched,
            "{win}: {row:?}"
        );
        assert_eq!(row.frame_operation, None, "{win}: no authorized write");
    }
}

#[test]
fn all_minimized_scope_stays_controlled() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    let minimized = WindowFact {
        minimized: true,
        rect: None,
        ..fact("win-m", "out-a", 0)
    };
    let reply = accepted(
        &mut engine,
        &report(1, roomy, vec![minimized], "", None, None, vec![]),
    );
    let view = sole_view(&reply);
    let scope = reply
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{view}"))
        .expect("scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled, "{scope:?}");
    let row = row_of(&reply, "win-m");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(tiler_core::world_report::HoldReason::Minimized),
        "{row:?}"
    );
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Controlled,
        "{row:?}"
    );
}

#[test]
fn float_ambiguous_third_frame_holds_across_ack() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None, vec![]),
    );
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // Clamp leaves a third frame (neither source nor target) under ambiguity.
    let mut third = fact("win-a", "out-a", 0);
    third.rect = Some(rect(400, 300, 210, 160));
    third.floating = true;
    let ambiguous = accepted(
        &mut engine,
        &report(
            3,
            roomy.clone(),
            vec![third.clone(), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-12".to_owned(),
                kind: AttemptKind::Property,
                outcome: AttemptOutcome::PossiblyExecuted,
                hold: None,
            }],
        ),
    );
    assert_eq!(
        row_of(&ambiguous, "win-a").frame_operation.as_deref(),
        Some("op-12"),
        "ambiguous holds"
    );
    // Ack disappears but the same third frame persists: never infer external
    // from ambiguity. The token holds with no re-actuation.
    let held = accepted(
        &mut engine,
        &report(
            4,
            roomy.clone(),
            vec![third, fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let row = row_of(&held, "win-a");
    assert_eq!(
        row.frame_operation.as_deref(),
        Some("op-12"),
        "ambiguous holds across ack: {row:?}"
    );
    assert_eq!(row.effective_frame, Some(target));
    assert_eq!(held.command_result, None, "no replay while held");
}

#[test]
fn float_explicit_supersession_settles_ambiguous() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None, vec![]),
    );
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    let mut third = fact("win-a", "out-a", 0);
    third.rect = Some(rect(400, 300, 210, 160));
    third.floating = true;
    let settled = accepted(
        &mut engine,
        &report(
            3,
            roomy,
            vec![third, fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-12".to_owned(),
                kind: AttemptKind::Property,
                outcome: AttemptOutcome::SupersededExternal,
                hold: None,
            }],
        ),
    );
    assert_eq!(
        row_of(&settled, "win-a").frame_operation,
        None,
        "explicit supersession settles"
    );
}

#[test]
fn float_lost_result_recovers_stale_never_replays() {
    use tiler_core::boundary::CoreCommand;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None, vec![]),
    );
    let target = rect(10, 10, 200, 150);
    let first = place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    assert!(first.command_result.is_some(), "one-shot result");
    // Lost reply: unrelated snapshot still carries the exact token.
    let next = accepted(
        &mut engine,
        &report(3, roomy.clone(), two_tiled(), "", None, None, vec![]),
    );
    assert_eq!(next.command_result, None);
    assert_eq!(
        row_of(&next, "win-a").frame_operation.as_deref(),
        Some("op-12")
    );
    assert_eq!(row_of(&next, "win-a").effective_frame, Some(target));
    // Consumed command sequence reconciles only, never re-executes.
    let replay = WorldReport {
        restored: None,
        command_seq: Some(12),
        command: Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: "win-a".to_owned(),
            float_rect: Some(target),
        })),
        ..report(4, roomy, two_tiled(), "", None, None, vec![])
    };
    match engine.handle_report(&replay) {
        WorldOutcome::CommandConsumed(reply) => {
            assert_eq!(reply.command_result, None);
            assert_eq!(
                row_of(&reply, "win-a").frame_operation.as_deref(),
                Some("op-12")
            );
        }
        other => panic!("expected consumed, got {other:?}"),
    }
}

#[test]
fn float_close_and_reset_clear_token() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None, vec![]),
    );
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // Explicit close settles the pending target.
    let mut closed = report(
        3,
        roomy.clone(),
        vec![fact("win-b", "out-a", 100)],
        "",
        None,
        None,
        vec![],
    );
    closed.closed = vec![tiler_core::directional::WindowId("win-a".to_owned())];
    let reply = accepted(&mut engine, &closed);
    assert!(
        reply.windows.iter().all(|w| w.frame_operation.is_none()),
        "close clears: {:?}",
        reply.windows
    );
    // Fresh placement then incarnation reset clears without touching windows.
    place_float(&mut engine, 4, two_tiled(), "win-a", Some(target));
    let mut reset = report(5, roomy, two_tiled(), "", None, None, vec![]);
    reset.adapter_incarnation = "adapter-2".to_owned();
    let reply = accepted(&mut engine, &reset);
    assert!(
        reply.windows.iter().all(|w| w.frame_operation.is_none()),
        "incarnation clears: {:?}",
        reply.windows
    );
}
