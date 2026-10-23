//! Capacity-float diagnostic and explicit float-placement lifecycle at the
//! world boundary: exhaustion marks capacity floats with a diagnostic-only
//! origin, ordinary floats behave identically, and ToggleFloat placement is
//! an explicit one-shot operation with observation/supersession/refusal
//! settlement. Logical-first: screens plus window facts only.

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    FloatOrigin, ScopeAuthority, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand,
    WorldReport,
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
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: Vec::new(),
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        default_tiled: true,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn report_with_frames(
    request_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    frames: Vec<crate::world_report::FrameAttempt>,
) -> WorldReport {
    let mut out = report(request_seq, screens, windows, focused, command_seq, command);
    out.monotonic_ms = 1000 + request_seq * 1000;
    out.frames = frames;
    out
}

fn float_fact(window: &str, output: &str, rect: Option<Rect>) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect,
        floating: true,
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

fn float_attempt(
    operation: &str,
    window: &str,
    target: Rect,
    setter: crate::world_report::FrameSetter,
    prewrite: Option<Rect>,
    readback: Option<Rect>,
    ms: u64,
) -> crate::world_report::FrameAttempt {
    crate::world_report::FrameAttempt {
        window: WindowId(window.to_owned()),
        target,
        setter,
        readback,
        dpi_before: None,
        dpi_after: None,
        attempt_ms: ms,
        operation_id: Some(operation.to_owned()),
        prewrite,
    }
}

fn row_of<'a>(
    reply: &'a crate::world_report::WorldReply,
    id: &str,
) -> &'a crate::world_report::WindowDesire {
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
        &report(1, tiny_screens(), windows, "", None, None),
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
        &report(1, tiny_screens(), windows, "", None, None),
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
        &report(1, tiny_screens(), windows.clone(), "", None, None),
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
        &report(2, tiny_screens(), echo, &tiled_focus, None, None),
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
) -> crate::world_report::WorldReply {
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
        &report(1, roomy.clone(), two_tiled(), "", None, None),
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
        Some(crate::world_report::CommandResult::Float { .. })
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
        &report(1, roomy.clone(), two_tiled(), "", None, None),
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
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let source = rect(0, 0, 40, 90);
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
        ),
    );
    let row = row_of(&dropped, "win-a");
    assert_eq!(
        row.frame_operation, None,
        "foreign input supersedes: {row:?}"
    );
    assert!(row.floating, "exception retained: {row:?}");
    // A native refusal with an unchanged readable preimage settles the
    // replacement too: only exact operation-id plus window plus target
    // correlation counts.
    place_float(&mut engine, 4, two_tiled(), "win-a", Some(target));
    let settled = accepted(
        &mut engine,
        &report_with_frames(
            5,
            roomy,
            two_tiled(),
            "",
            None,
            None,
            vec![float_attempt(
                "op-14",
                "win-a",
                target,
                FrameSetter::Refused,
                Some(source),
                Some(source),
                6000,
            )],
        ),
    );
    assert_eq!(
        row_of(&settled, "win-a").frame_operation,
        None,
        "refusal settles"
    );
}

fn sole_view(reply: &crate::world_report::WorldReply) -> String {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("logical view")
}

fn domain_key(view: &str) -> crate::session::DomainKey {
    crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view.to_owned()),
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
        Some(crate::world_report::CommandResult::Float { .. })
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
        &report(3, roomy, vec![confirmed], "", None, None),
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
        &report(1, roomy, vec![ordinary, fixed], "", None, None),
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
        &report(1, roomy, vec![minimized], "", None, None),
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
        ScopeAuthority::Hold(crate::world_report::HoldReason::Minimized),
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
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // A clamping setter leaves a third frame (neither source nor target):
    // the raw accepted attempt with a mismatching readback latches
    // ambiguity under the operation identity. The same evidence also feeds
    // clamp suppression, so the token is withheld while the retained target
    // stays outstanding: no re-actuation, no verdict.
    let third = rect(400, 300, 210, 160);
    let ambiguous = accepted(
        &mut engine,
        &report_with_frames(
            3,
            roomy.clone(),
            vec![
                float_fact("win-a", "out-a", Some(third)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![float_attempt(
                "op-12",
                "win-a",
                target,
                FrameSetter::Accepted,
                Some(source),
                Some(third),
                4000,
            )],
        ),
    );
    assert_eq!(engine.retained_floats.len(), 1, "ambiguous retains");
    let row = row_of(&ambiguous, "win-a");
    assert_eq!(row.frame_operation, None, "clamp withholds while held");
    assert_eq!(row.effective_frame, Some(target));
    // Ack disappears but the same third frame persists: never infer external
    // input from ambiguity. The token holds with no re-actuation.
    let held = accepted(
        &mut engine,
        &report(
            4,
            roomy.clone(),
            vec![
                float_fact("win-a", "out-a", Some(third)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
        ),
    );
    let row = row_of(&held, "win-a");
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "ambiguous holds across ack"
    );
    assert_eq!(row.frame_operation, None, "still withheld: {row:?}");
    assert_eq!(row.effective_frame, Some(target));
    assert_eq!(held.command_result, None, "no replay while held");
}

#[test]
fn float_raw_accepted_attempt_confirms_at_target() {
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // Setter success alone proves nothing: the staged fact must name the
    // target too. The correlated readback corroborates the observation.
    let settled = accepted(
        &mut engine,
        &report_with_frames(
            3,
            roomy,
            vec![
                float_fact("win-a", "out-a", Some(target)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![float_attempt(
                "op-12",
                "win-a",
                target,
                FrameSetter::Accepted,
                Some(source),
                Some(target),
                4000,
            )],
        ),
    );
    let row = row_of(&settled, "win-a");
    assert_eq!(row.frame_operation, None, "confirmed: {row:?}");
    assert!(row.floating, "stays floated: {row:?}");
}

#[test]
fn float_dpi_change_blocks_confirmation_until_fresh_evidence() {
    use crate::world_report::FrameAttempt;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // A DPI transition under the write invalidates the placement: even a
    // matching readback plus a matching observation cannot confirm it on
    // this report. Unknown DPI on either side is not a transition.
    let dpi_flip = accepted(
        &mut engine,
        &report_with_frames(
            3,
            roomy.clone(),
            vec![
                float_fact("win-a", "out-a", Some(target)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![FrameAttempt {
                window: WindowId("win-a".to_owned()),
                target,
                setter: crate::world_report::FrameSetter::Accepted,
                readback: Some(target),
                dpi_before: Some(96),
                dpi_after: Some(120),
                attempt_ms: 4000,
                operation_id: Some("op-12".to_owned()),
                prewrite: Some(source),
            }],
        ),
    );
    assert_eq!(
        row_of(&dpi_flip, "win-a").frame_operation.as_deref(),
        Some("op-12"),
        "DPI-changing basis never confirms"
    );
    // Fresh stable evidence afterwards confirms through observation alone.
    let settled = accepted(
        &mut engine,
        &report(
            4,
            roomy,
            vec![
                float_fact("win-a", "out-a", Some(target)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
        ),
    );
    assert_eq!(
        row_of(&settled, "win-a").frame_operation,
        None,
        "fresh stable observation confirms"
    );
}

#[test]
fn float_changed_preimage_and_throw_latch_ambiguity() {
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // A refusal whose readback moved off the preimage proves nothing about
    // the write: latch ambiguity exactly like a clamp. The attempt also
    // feeds transient suppression, so the stranded token is withheld, never
    // re-actuated.
    let moved = rect(5, 5, 40, 90);
    let held = accepted(
        &mut engine,
        &report_with_frames(
            3,
            roomy.clone(),
            vec![
                float_fact("win-a", "out-a", Some(moved)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![float_attempt(
                "op-12",
                "win-a",
                target,
                FrameSetter::Refused,
                Some(source),
                Some(moved),
                4000,
            )],
        ),
    );
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "moved refusal latches ambiguity"
    );
    assert_eq!(row_of(&held, "win-a").effective_frame, Some(target));
    // A throwing setter with unreadable proof latches too, even when the
    // staged frame still names the source.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    let thrown = accepted(
        &mut engine,
        &report_with_frames(
            3,
            roomy,
            vec![
                float_fact("win-a", "out-a", Some(source)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![float_attempt(
                "op-12",
                "win-a",
                target,
                FrameSetter::Threw,
                Some(source),
                None,
                4000,
            )],
        ),
    );
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "throw with unreadable proof latches ambiguity"
    );
    assert_eq!(row_of(&thrown, "win-a").effective_frame, Some(target));
}

#[test]
fn float_forged_correlation_never_settles() {
    use crate::world_report::{FrameAttempt, FrameSetter};
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    // Each forged shape runs against a fresh placement: only exact
    // operation-id plus window plus target correlation settles, and an
    // ordinary (uncorrelated) attempt is never a placement proof.
    let forged: Vec<FrameAttempt> = vec![
        float_attempt(
            "op-9999",
            "win-a",
            target,
            FrameSetter::Accepted,
            Some(source),
            Some(target),
            4000,
        ),
        float_attempt(
            "op-12",
            "win-b",
            target,
            FrameSetter::Accepted,
            Some(source),
            Some(target),
            4000,
        ),
        float_attempt(
            "op-12",
            "win-a",
            rect(11, 11, 200, 150),
            FrameSetter::Accepted,
            Some(source),
            Some(target),
            4000,
        ),
        FrameAttempt {
            window: WindowId("win-a".to_owned()),
            target,
            setter: FrameSetter::Accepted,
            readback: Some(source),
            dpi_before: None,
            dpi_after: None,
            attempt_ms: 4000,
            operation_id: None,
            prewrite: None,
        },
    ];
    for (index, attempt) in forged.into_iter().enumerate() {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        accepted(
            &mut engine,
            &report(1, roomy.clone(), two_tiled(), "", None, None),
        );
        place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
        let reply = accepted(
            &mut engine,
            &report_with_frames(
                3,
                roomy.clone(),
                vec![
                    float_fact("win-a", "out-a", Some(source)),
                    fact("win-b", "out-a", 100),
                ],
                "",
                None,
                None,
                vec![attempt],
            ),
        );
        // Every forged shape leaves the placement outstanding: an ordinary
        // clamp may withhold the stranded token, but the retained target
        // never settles from window/geometry-only evidence.
        assert_eq!(
            engine.retained_floats.len(),
            1,
            "forged shape {index} never settles"
        );
        assert_eq!(row_of(&reply, "win-a").effective_frame, Some(target));
    }
}

#[test]
fn float_replay_of_consumed_attempt_never_resurrects() {
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // Latch ambiguity with a clamping attempt.
    let third = rect(400, 300, 210, 160);
    let clamp = float_attempt(
        "op-12",
        "win-a",
        target,
        FrameSetter::Accepted,
        Some(source),
        Some(third),
        4000,
    );
    accepted(
        &mut engine,
        &report_with_frames(
            3,
            roomy.clone(),
            vec![
                float_fact("win-a", "out-a", Some(third)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![clamp.clone()],
        ),
    );
    // Redelivering the identical stamp is a lost-ack replay: it changes
    // nothing and never clears the latched hold.
    let replayed = accepted(
        &mut engine,
        &report_with_frames(
            4,
            roomy.clone(),
            vec![
                float_fact("win-a", "out-a", Some(third)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![clamp],
        ),
    );
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "replay holds without renewal"
    );
    assert_eq!(row_of(&replayed, "win-a").effective_frame, Some(target));
    // Confirmation then redelivery: the settled placement stays settled.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    let confirm = float_attempt(
        "op-12",
        "win-a",
        target,
        FrameSetter::Accepted,
        Some(source),
        Some(target),
        4000,
    );
    let settled = accepted(
        &mut engine,
        &report_with_frames(
            3,
            roomy.clone(),
            vec![
                float_fact("win-a", "out-a", Some(target)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![confirm.clone()],
        ),
    );
    assert_eq!(row_of(&settled, "win-a").frame_operation, None);
    let again = accepted(
        &mut engine,
        &report_with_frames(
            4,
            roomy,
            vec![
                float_fact("win-a", "out-a", Some(target)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![confirm],
        ),
    );
    assert_eq!(row_of(&again, "win-a").frame_operation, None);
}

#[test]
fn float_incomplete_gesture_minimized_and_unreadable_hold() {
    use crate::world_report::FrameSetter;
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    let target = rect(10, 10, 200, 150);
    // Incomplete inventory never confirms, even at the target.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    let mut gap = report_with_frames(
        3,
        roomy.clone(),
        vec![
            float_fact("win-a", "out-a", Some(target)),
            fact("win-b", "out-a", 100),
        ],
        "",
        None,
        None,
        vec![],
    );
    gap.inventory_complete = false;
    let held = accepted(&mut engine, &gap);
    assert_eq!(
        row_of(&held, "win-a").frame_operation.as_deref(),
        Some("op-12"),
        "incomplete inventory holds"
    );
    // A gesture barrier holds the same way.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    let mut gesture = report(
        3,
        roomy.clone(),
        vec![
            float_fact("win-a", "out-a", Some(target)),
            fact("win-b", "out-a", 100),
        ],
        "",
        None,
        None,
    );
    gesture.gesture_barrier = true;
    let held = accepted(&mut engine, &gesture);
    assert_eq!(
        row_of(&held, "win-a").frame_operation.as_deref(),
        Some("op-12"),
        "gesture holds"
    );
    // Minimized and unreadable facts hold: there is no managed observation
    // to confirm or supersede from. A minimized row carries no token by
    // shape, but the placement stays retained; an unreadable fact keeps the
    // stranded token withheld under suppression.
    for (name, windows) in [
        (
            "minimized",
            vec![
                WindowFact {
                    minimized: true,
                    rect: None,
                    ..float_fact("win-a", "out-a", None)
                },
                fact("win-b", "out-a", 100),
            ],
        ),
        (
            "unreadable",
            vec![
                float_fact("win-a", "out-a", None),
                fact("win-b", "out-a", 100),
            ],
        ),
    ] {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        accepted(
            &mut engine,
            &report(1, roomy.clone(), two_tiled(), "", None, None),
        );
        place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
        let mut stale = report_with_frames(
            3,
            roomy.clone(),
            windows,
            "",
            None,
            None,
            vec![float_attempt(
                "op-12",
                "win-a",
                target,
                FrameSetter::Threw,
                None,
                None,
                4000,
            )],
        );
        stale.monotonic_ms = 4000;
        accepted(&mut engine, &stale);
        assert_eq!(engine.retained_floats.len(), 1, "{name} holds");
    }
}

#[test]
fn float_gap_clamp_recovers_without_repeated_row() {
    use crate::world_report::FrameSetter;
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    let third = rect(400, 300, 210, 160);
    // A clamping attempt received during a gap is staged under the
    // operation before the hold, so the acknowledgement-consuming recovery
    // report (no repeated attempt row) reconciles from retained evidence
    // instead of dropping the placement as external input. A minimize
    // excursion legitimately releases the committed exception geometry, so
    // that lane asserts retention and no replay rather than the row frame.
    for (name, gap_windows, incomplete, gesture, expect_effective) in [
        (
            "inventory",
            vec![
                float_fact("win-a", "out-a", Some(third)),
                fact("win-b", "out-a", 100),
            ],
            true,
            false,
            true,
        ),
        (
            "gesture",
            vec![
                float_fact("win-a", "out-a", Some(third)),
                fact("win-b", "out-a", 100),
            ],
            false,
            true,
            true,
        ),
        (
            "unreadable",
            vec![
                float_fact("win-a", "out-a", None),
                fact("win-b", "out-a", 100),
            ],
            false,
            false,
            true,
        ),
        (
            "minimized",
            vec![
                WindowFact {
                    minimized: true,
                    rect: None,
                    ..float_fact("win-a", "out-a", None)
                },
                fact("win-b", "out-a", 100),
            ],
            false,
            false,
            false,
        ),
    ] {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        accepted(
            &mut engine,
            &report(1, roomy.clone(), two_tiled(), "", None, None),
        );
        place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
        let mut gapped = report_with_frames(
            3,
            roomy.clone(),
            gap_windows,
            "",
            None,
            None,
            vec![float_attempt(
                "op-12",
                "win-a",
                target,
                FrameSetter::Accepted,
                Some(source),
                Some(third),
                4000,
            )],
        );
        if incomplete {
            gapped.inventory_complete = false;
        }
        if gesture {
            gapped.gesture_barrier = true;
        }
        accepted(&mut engine, &gapped);
        assert_eq!(
            engine.retained_floats.len(),
            1,
            "{name}: gap stages evidence and holds"
        );
        // Recovery carries no repeated attempt: the shared consumed
        // watermark already forgot it. Retained evidence still holds.
        let recovered = accepted(
            &mut engine,
            &report_with_frames(
                4,
                roomy.clone(),
                vec![
                    float_fact("win-a", "out-a", Some(third)),
                    fact("win-b", "out-a", 100),
                ],
                "",
                None,
                None,
                vec![],
            ),
        );
        assert_eq!(
            engine.retained_floats.len(),
            1,
            "{name}: recovery without repetition retains"
        );
        if expect_effective {
            assert_eq!(row_of(&recovered, "win-a").effective_frame, Some(target));
        }
        assert_eq!(recovered.command_result, None, "{name}: no replay");
    }
}

#[test]
fn float_gap_refusal_recovers_to_settle_after_ack() {
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // An explicit refusal with an unchanged readable preimage arrives
    // during an inventory gap: the staged fact cannot prove it yet, so the
    // evidence is preserved instead of settling or dropping.
    let mut gapped = report_with_frames(
        3,
        roomy.clone(),
        vec![
            float_fact("win-a", "out-a", Some(source)),
            fact("win-b", "out-a", 100),
        ],
        "",
        None,
        None,
        vec![float_attempt(
            "op-12",
            "win-a",
            target,
            FrameSetter::Refused,
            Some(source),
            Some(source),
            4000,
        )],
    );
    gapped.inventory_complete = false;
    accepted(&mut engine, &gapped);
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "gap preserves refusal evidence"
    );
    // After the acknowledgement the complete readable preimage reconciles
    // the retained refusal with no replay: the stale target drops.
    let settled = accepted(
        &mut engine,
        &report_with_frames(
            4,
            roomy,
            vec![
                float_fact("win-a", "out-a", Some(source)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert_eq!(
        engine.retained_floats.len(),
        0,
        "readable preimage settles refusal"
    );
    let row = row_of(&settled, "win-a");
    assert_eq!(row.frame_operation, None);
    assert!(row.floating, "exception retained: {row:?}");
}

#[test]
fn float_dpi_flip_holds_recovery_then_target_settles() {
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // A DPI-invalidated write latches no-reactuation ambiguity: it never
    // confirms and never retries.
    let third = rect(400, 300, 210, 160);
    accepted(
        &mut engine,
        &report_with_frames(
            3,
            roomy.clone(),
            vec![
                float_fact("win-a", "out-a", Some(third)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
            vec![crate::world_report::FrameAttempt {
                window: WindowId("win-a".to_owned()),
                target,
                setter: FrameSetter::Accepted,
                readback: Some(third),
                dpi_before: Some(96),
                dpi_after: Some(120),
                attempt_ms: 4000,
                operation_id: Some("op-12".to_owned()),
                prewrite: Some(source),
            }],
        ),
    );
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "DPI flip latches ambiguity"
    );
    // Recovery without a repeated row still holds: the third frame is not
    // newer external input while ambiguity is latched.
    let recovered = accepted(
        &mut engine,
        &report(
            4,
            roomy.clone(),
            vec![
                float_fact("win-a", "out-a", Some(third)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
        ),
    );
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "recovery holds without repetition"
    );
    assert_eq!(row_of(&recovered, "win-a").effective_frame, Some(target));
    // A later exact target settles from native observation.
    let settled = accepted(
        &mut engine,
        &report(
            5,
            roomy,
            vec![
                float_fact("win-a", "out-a", Some(target)),
                fact("win-b", "out-a", 100),
            ],
            "",
            None,
            None,
        ),
    );
    assert_eq!(
        engine.retained_floats.len(),
        0,
        "later exact target settles"
    );
    assert_eq!(row_of(&settled, "win-a").frame_operation, None);
}

#[test]
fn float_gap_close_still_settles() {
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let source = rect(0, 0, 40, 90);
    let target = rect(10, 10, 200, 150);
    place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    // An explicit close settles during an inventory gap even with a
    // correlated attempt riding the same report: close always settles.
    let mut gapped = report_with_frames(
        3,
        roomy,
        vec![fact("win-b", "out-a", 100)],
        "",
        None,
        None,
        vec![float_attempt(
            "op-12",
            "win-a",
            target,
            FrameSetter::Accepted,
            Some(source),
            Some(rect(400, 300, 210, 160)),
            4000,
        )],
    );
    gapped.inventory_complete = false;
    gapped.closed = vec![WindowId("win-a".to_owned())];
    accepted(&mut engine, &gapped);
    assert_eq!(
        engine.retained_floats.len(),
        0,
        "explicit close settles across the gap"
    );
}

#[test]
fn float_lost_result_recovers_stale_never_replays() {
    use crate::boundary::CoreCommand;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let roomy = vec![screen_sized("out-a", 0, 800, 560, true)];
    accepted(
        &mut engine,
        &report(1, roomy.clone(), two_tiled(), "", None, None),
    );
    let target = rect(10, 10, 200, 150);
    let first = place_float(&mut engine, 2, two_tiled(), "win-a", Some(target));
    assert!(first.command_result.is_some(), "one-shot result");
    // Lost reply: unrelated snapshot still carries the exact token.
    let next = accepted(
        &mut engine,
        &report(3, roomy.clone(), two_tiled(), "", None, None),
    );
    assert_eq!(next.command_result, None);
    assert_eq!(
        row_of(&next, "win-a").frame_operation.as_deref(),
        Some("op-12")
    );
    assert_eq!(row_of(&next, "win-a").effective_frame, Some(target));
    // Consumed command sequence reconciles only, never re-executes.
    let replay = WorldReport {
        command_seq: Some(12),
        command: Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: "win-a".to_owned(),
            float_rect: Some(target),
        })),
        ..report(4, roomy, two_tiled(), "", None, None)
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
        &report(1, roomy.clone(), two_tiled(), "", None, None),
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
    );
    closed.closed = vec![crate::directional::WindowId("win-a".to_owned())];
    let reply = accepted(&mut engine, &closed);
    assert!(
        reply.windows.iter().all(|w| w.frame_operation.is_none()),
        "close clears: {:?}",
        reply.windows
    );
    // Fresh placement then incarnation reset clears without touching windows.
    place_float(&mut engine, 4, two_tiled(), "win-a", Some(target));
    let mut reset = report(5, roomy, two_tiled(), "", None, None);
    reset.adapter_incarnation = "adapter-2".to_owned();
    let reply = accepted(&mut engine, &reset);
    assert!(
        reply.windows.iter().all(|w| w.frame_operation.is_none()),
        "incarnation clears: {:?}",
        reply.windows
    );
}
