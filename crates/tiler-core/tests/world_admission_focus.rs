//! Fresh-admission focus intent at the full-world boundary (D4).
//!
//! A genuinely new lifetime admitted by a retained domain's sweep carries
//! exactly one Focus effect (`af-<window>`), tiled or capacity-floated
//! alike, even when the incoming native report still focuses the previous
//! tile. The intent is at-most-once per lifetime with the standard
//! one-shot watermark; re-observation, closes, gap/bounds edits, shrink
//! evictions, settings Apply, explicit tile retries, restarts, and
//! disable/enable re-adoptions mint nothing. An observed float focus never
//! wedges reconcile: the tiled remainder projects while the retained tiled
//! MRU stays authoritative internally. Ordinary user floats behave exactly
//! like capacity floats; the diagnostic origin never branches.

use tiler_core::boundary::CoreCommand;
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, EffectKind, FloatOrigin, HoldReason, OutputTopology, ScopeAuthority,
    WORLD_CONTRACT_VERSION, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win(id: &str, floating: bool) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        rect: rect(0, 0, 50, 50),
        floating,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn id(n: usize) -> String {
    format!("win-{n:06}")
}

/// Report with `count` windows (`win-000000..`), observed focus `focused`,
/// and explicit floating flags for `floating`.
fn tiny_report(
    request_seq: u64,
    observation_seq: u64,
    count: usize,
    focused: usize,
    floating: &[usize],
    bounds: Rect,
) -> WorldReport {
    let windows: Vec<EngineWindow> = (0..count)
        .map(|i| win(&id(i), floating.contains(&i)))
        .collect();
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        workspace_mode: WorkspaceMode::PerOutputLocal,
        topology: vec![OutputTopology {
            output: OutputId("out".to_owned()),
            native_order: vec!["ws".to_owned()],
            current_native: "ws".to_owned(),
            complete: true,
            geometry: None,
            primary: true,
        }],
        bindings: Vec::new(),
        domains: vec![DomainReport {
            domain: OutputDomain {
                id: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                bounds,
                gap: 8,
                adjacent: Default::default(),
            },
            outer_gap: 0,
            focused: WindowId(id(focused)),
            windows,
            closed: Vec::new(),
            readable: true,
            hold: None,
        }],
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
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

fn float_ids(reply: &tiler_core::world_report::WorldReply) -> Vec<String> {
    let mut ids: Vec<String> = reply
        .windows
        .iter()
        .filter(|w| w.floating)
        .map(|w| w.lifetime.0.clone())
        .collect();
    ids.sort();
    ids
}

fn row_of<'a>(
    reply: &'a tiler_core::world_report::WorldReply,
    id: &str,
) -> &'a tiler_core::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == id)
        .unwrap_or_else(|| panic!("expected row for {id}"))
}

fn assert_controlled(reply: &tiler_core::world_report::WorldReply) {
    assert!(
        reply
            .scopes
            .iter()
            .all(|s| s.authority == ScopeAuthority::Controlled),
        "reply stays controlled, got {:?}",
        reply.scopes
    );
}

#[test]
fn capacity_float_newcomer_focuses_once_then_stays() {
    // 10x10 gap 8 holds four tiles: the seed is startup (no intent) and
    // the fifth window capacity-floats with one focus intent.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = accepted(
        &mut engine,
        &tiny_report(1, 1, 4, 0, &[], rect(0, 0, 10, 10)),
    );
    assert!(seed.effects.is_empty(), "startup seeds mint no intent");
    assert_controlled(&seed);

    // The native report still focuses the previous tile: the newcomer
    // intent fires anyway, exactly once.
    let arrival = accepted(
        &mut engine,
        &tiny_report(2, 2, 5, 0, &[], rect(0, 0, 10, 10)),
    );
    assert_controlled(&arrival);
    assert_eq!(float_ids(&arrival), vec![id(4)], "newcomer capacity-floats");
    assert_eq!(
        row_of(&arrival, &id(4)).float_origin,
        Some(FloatOrigin::Capacity)
    );
    assert!(row_of(&arrival, &id(0)).allocation.is_some());
    assert_eq!(arrival.effects.len(), 1, "one admission intent");
    let effect = &arrival.effects[0];
    assert_eq!(effect.id, format!("af-{}", id(4)));
    assert_eq!(effect.kind, EffectKind::Focus);
    assert_eq!(effect.target.0, id(4));
    assert_eq!(
        effect.issued_revision, arrival.state_revision,
        "one-shot watermark rides the reply revision"
    );

    // Full repeated snapshots never refocus.
    let echo = accepted(
        &mut engine,
        &tiny_report(3, 3, 5, 0, &[4], rect(0, 0, 10, 10)),
    );
    assert_controlled(&echo);
    assert!(echo.effects.is_empty(), "re-observation mints no intent");
    assert_eq!(float_ids(&echo), vec![id(4)]);

    // The native side follows the intent: the floated active window stays
    // controlled, the previous tile keeps its slot, still no new intent.
    let followed = accepted(
        &mut engine,
        &tiny_report(4, 4, 5, 4, &[4], rect(0, 0, 10, 10)),
    );
    assert_controlled(&followed);
    assert!(followed.effects.is_empty());
    assert_eq!(float_ids(&followed), vec![id(4)]);
    assert!(row_of(&followed, &id(0)).allocation.is_some());

    // Closing the newcomer mints no intent and drops the row.
    let mut closed = tiny_report(5, 5, 5, 0, &[4], rect(0, 0, 10, 10));
    closed.domains[0].windows.retain(|w| w.window.0 != id(4));
    closed.domains[0].closed = vec![WindowId(id(4))];
    let closed_reply = accepted(&mut engine, &closed);
    assert_controlled(&closed_reply);
    assert!(closed_reply.effects.is_empty(), "close mints no intent");
    assert!(closed_reply.windows.iter().all(|w| w.lifetime.0 != id(4)));

    // Growth alone mints no intent.
    let grown = accepted(
        &mut engine,
        &tiny_report(6, 6, 4, 0, &[], rect(0, 0, 800, 600)),
    );
    assert_controlled(&grown);
    assert!(grown.effects.is_empty(), "growth mints no intent");

    // A settings Apply alone mints no intent and reflows nothing.
    let mut applied = tiny_report(7, 7, 4, 0, &[], rect(0, 0, 800, 600));
    applied.anchor_fallback = tiler_core::admission::AnchorFallback::ClosestRelative;
    applied.no_space_result = tiler_core::admission::NoSpaceResult::AllowOverlap;
    let applied_reply = accepted(&mut engine, &applied);
    assert_controlled(&applied_reply);
    assert!(applied_reply.effects.is_empty(), "Apply mints no intent");
    assert_eq!(float_ids(&applied_reply), Vec::<String>::new());
}

#[test]
fn tiled_newcomer_focuses_once_and_shrink_eviction_mints_none() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = accepted(
        &mut engine,
        &tiny_report(1, 1, 4, 0, &[], rect(0, 0, 800, 600)),
    );
    assert!(seed.effects.is_empty(), "startup seeds mint no intent");

    // Roomy tiled newcomer: still always focused, exactly once.
    let arrival = accepted(
        &mut engine,
        &tiny_report(2, 2, 5, 0, &[], rect(0, 0, 800, 600)),
    );
    assert_controlled(&arrival);
    assert_eq!(arrival.effects.len(), 1);
    assert_eq!(arrival.effects[0].id, format!("af-{}", id(4)));
    assert_eq!(arrival.effects[0].kind, EffectKind::Focus);
    assert!(
        row_of(&arrival, &id(4)).allocation.is_some(),
        "newcomer tiles"
    );

    // Following the intent mints nothing further.
    let followed = accepted(
        &mut engine,
        &tiny_report(3, 3, 5, 4, &[], rect(0, 0, 800, 600)),
    );
    assert_controlled(&followed);
    assert!(followed.effects.is_empty());

    // Shrink below capacity: newest victims float with no setter. The
    // startup seed admitted the native focus (win-000000) last under
    // focus-last carried order, so after the newcomer win-000004 the
    // next-newest victim is win-000000 itself (first-successful-tiled-
    // admission order, never window-id order).
    let shrunk = accepted(
        &mut engine,
        &tiny_report(4, 4, 5, 4, &[], rect(0, 0, 10, 10)),
    );
    assert_controlled(&shrunk);
    assert!(shrunk.effects.is_empty(), "shrink eviction mints no intent");
    let shrunk_floats = float_ids(&shrunk);
    assert_eq!(
        shrunk_floats,
        vec![id(0), id(4)],
        "actual newest victims float first, got {shrunk_floats:?}"
    );
    for n in 1..4 {
        assert!(
            row_of(&shrunk, &id(n)).allocation.is_some(),
            "survivor win-{n:06} keeps its slot"
        );
    }

    // The evicted floats focused stay controlled with no new intent.
    let echo_floats: Vec<usize> = shrunk_floats
        .iter()
        .map(|s| s.split('-').next_back().expect("id").parse().expect("n"))
        .collect();
    let echo = accepted(
        &mut engine,
        &tiny_report(5, 5, 5, 4, &echo_floats, rect(0, 0, 10, 10)),
    );
    assert_controlled(&echo);
    assert!(echo.effects.is_empty());
    assert_eq!(float_ids(&echo), shrunk_floats);
}

#[test]
fn floated_anchor_focus_never_wedges_and_never_refocuses() {
    // The 5-seed floats its own focused anchor (win-000000), leaving
    // session focus none: the next report naming that float must stay
    // controlled instead of freezing on focus-mismatch.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = accepted(
        &mut engine,
        &tiny_report(1, 1, 5, 0, &[], rect(0, 0, 10, 10)),
    );
    assert_eq!(
        float_ids(&seed),
        vec![id(0)],
        "focused anchor capacity-floats"
    );
    assert!(seed.effects.is_empty(), "startup seeds mint no intent");

    let echo = accepted(
        &mut engine,
        &tiny_report(2, 2, 5, 0, &[0], rect(0, 0, 10, 10)),
    );
    assert_controlled(&echo);
    assert!(echo.effects.is_empty(), "re-observation mints no intent");
    assert_eq!(float_ids(&echo), vec![id(0)]);
    assert_eq!(
        row_of(&echo, &id(0)).float_origin,
        Some(FloatOrigin::Capacity)
    );
    assert_eq!(echo.windows.len(), 5, "full roster still publishes");
}

#[test]
fn restart_and_enable_keep_floats_without_new_intent() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = accepted(
        &mut engine,
        &tiny_report(1, 1, 6, 0, &[], rect(0, 0, 10, 10)),
    );
    assert_eq!(float_ids(&seed), vec![id(0), id(5)]);

    // Restart (fresh core, same generation): the persisted floats echo
    // back as ordinary floats, stay focused as observed, mint no intent.
    let mut restarted = WorldEngine::new("gen-1").expect("valid");
    let adopt = accepted(
        &mut restarted,
        &tiny_report(1, 1, 6, 0, &[0, 5], rect(0, 0, 10, 10)),
    );
    assert_controlled(&adopt);
    assert!(adopt.effects.is_empty(), "restart adoption mints no intent");
    assert_eq!(float_ids(&adopt), vec![id(0), id(5)]);
    assert!(
        adopt
            .windows
            .iter()
            .filter(|w| w.floating)
            .all(|w| w.float_origin.is_none()),
        "fresh core carries no diagnostic origin"
    );

    // Disable releases the session; re-enable re-adopts fresh with the
    // same no-intent shape.
    let mut disable = tiny_report(2, 2, 6, 0, &[0, 5], rect(0, 0, 10, 10));
    disable.command_seq = Some(1);
    disable.command = Some(WorldCommand::SetWorkspaceTiling {
        output: OutputId("out".to_owned()),
        workspace: "ws".to_owned(),
        tiled: false,
    });
    let disabled = accepted(&mut restarted, &disable);
    assert!(disabled.windows.iter().all(|w| w.allocation.is_none()));
    let mut enable = tiny_report(3, 3, 6, 0, &[0, 5], rect(0, 0, 10, 10));
    enable.command_seq = Some(2);
    enable.command = Some(WorldCommand::SetWorkspaceTiling {
        output: OutputId("out".to_owned()),
        workspace: "ws".to_owned(),
        tiled: true,
    });
    let enabled = accepted(&mut restarted, &enable);
    assert_controlled(&enabled);
    assert!(
        enabled.effects.is_empty(),
        "enable re-adoption mints no intent"
    );
    assert_eq!(float_ids(&enabled), vec![id(0), id(5)]);
}

#[test]
fn ordinary_user_float_focus_matches_capacity_handling() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &tiny_report(1, 1, 2, 0, &[], rect(0, 0, 800, 600)),
    );

    // The user floats win-000001 natively: convergence adopts the
    // ordinary float with no intent.
    let floated = accepted(
        &mut engine,
        &tiny_report(2, 2, 2, 0, &[1], rect(0, 0, 800, 600)),
    );
    assert_controlled(&floated);
    assert!(floated.effects.is_empty());
    assert_eq!(float_ids(&floated), vec![id(1)]);
    assert_eq!(row_of(&floated, &id(1)).float_origin, None);

    // Naming the ordinary float focused stays controlled, same as a
    // capacity float, with no new intent.
    let focused = accepted(
        &mut engine,
        &tiny_report(3, 3, 2, 1, &[1], rect(0, 0, 800, 600)),
    );
    assert_controlled(&focused);
    assert!(focused.effects.is_empty());
    assert_eq!(float_ids(&focused), vec![id(1)]);
    assert!(row_of(&focused, &id(0)).allocation.is_some());
}

#[test]
fn explicit_retry_failure_mints_no_focus_and_never_loops() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &tiny_report(1, 1, 5, 0, &[], rect(0, 0, 10, 10)),
    );

    for seq in [2u64, 3] {
        let mut attempt = tiny_report(seq, seq, 5, 1, &[0], rect(0, 0, 10, 10));
        attempt.command_seq = Some(seq - 1);
        attempt.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: id(0),
            float_rect: None,
        }));
        let reply = accepted(&mut engine, &attempt);
        assert_controlled(&reply);
        match &reply.command_result {
            Some(tiler_core::world_report::CommandResult::Float {
                window,
                rect,
                no_space,
            }) => {
                assert_eq!(window.0, id(0));
                assert_eq!(*rect, None);
                assert!(*no_space, "still exhausted");
            }
            other => panic!("expected float result, got {other:?}"),
        }
        assert!(reply.effects.is_empty(), "failed retry mints no focus");
        assert_eq!(float_ids(&reply), vec![id(0)], "retry stays floated");
    }
}

#[test]
fn burst_batch_pins_last_eligible_newcomer_once() {
    // MED-4: two newcomers in one report carry exactly one native focus
    // intent, pinned to the last eligible newcomer in carried order
    // (consistent with the session admission sequence), never one per
    // newcomer and never replayed.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = accepted(
        &mut engine,
        &tiny_report(1, 1, 2, 0, &[], rect(0, 0, 800, 600)),
    );
    assert!(seed.effects.is_empty(), "startup seeds mint no intent");
    let arrival = accepted(
        &mut engine,
        &tiny_report(2, 2, 4, 0, &[], rect(0, 0, 800, 600)),
    );
    assert_controlled(&arrival);
    assert_eq!(arrival.effects.len(), 1, "one burst intent");
    let effect = &arrival.effects[0];
    assert_eq!(effect.kind, EffectKind::Focus);
    assert_eq!(effect.target.0, id(3), "last newcomer in carried order");
    assert_eq!(effect.id, format!("af-{}", id(3)));
    // The burst never replays: the identical roster mints nothing.
    let echo = accepted(
        &mut engine,
        &tiny_report(3, 3, 4, 3, &[], rect(0, 0, 800, 600)),
    );
    assert_controlled(&echo);
    assert!(echo.effects.is_empty(), "burst never replays");
}

#[test]
fn gesture_barrier_spends_newcomer_activation_without_replay() {
    // MED-4: a newcomer arriving mid-gesture drops its activation
    // honestly (no effect while the barrier holds) and never replays it
    // on a later report: safety priority with at-most-once preserved.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &tiny_report(1, 1, 2, 0, &[], rect(0, 0, 800, 600)),
    );
    let mut barrier = tiny_report(2, 2, 3, 0, &[], rect(0, 0, 800, 600));
    barrier.gesture_barrier = true;
    let held = accepted(&mut engine, &barrier);
    assert!(held.effects.is_empty(), "barrier spends the activation");
    assert!(
        row_of(&held, &id(2)).allocation.is_some(),
        "newcomer still converges under the barrier"
    );
    // The barrier lifts with the same roster: still no intent, the
    // activation was spent, never queued.
    let after = accepted(
        &mut engine,
        &tiny_report(3, 3, 3, 0, &[], rect(0, 0, 800, 600)),
    );
    assert_controlled(&after);
    assert!(
        after.effects.is_empty(),
        "spent activation never replays after the gesture"
    );
}

#[test]
fn unexplained_excluded_newcomer_takes_no_activation() {
    // MED-4: a game/overlay newcomer (fit-excluded with no floating,
    // fullscreen, or maximized origin) never takes automatic activation,
    // even though it converges. Safety priority over liveness.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &tiny_report(1, 1, 2, 0, &[], rect(0, 0, 800, 600)),
    );
    let mut game = tiny_report(2, 2, 3, 0, &[], rect(0, 0, 800, 600));
    game.domains[0].windows[2].fit_excluded = true;
    let reply = accepted(&mut engine, &game);
    assert_controlled(&reply);
    assert!(
        reply.effects.is_empty(),
        "unexplained excluded newcomer takes no activation"
    );
    assert!(
        row_of(&reply, &id(2)).allocation.is_some(),
        "the lifetime still converges honestly"
    );
    // A later clean report still mints nothing for it: no activation
    // was ever owed, and none is queued.
    let echo = accepted(
        &mut engine,
        &tiny_report(3, 3, 3, 0, &[], rect(0, 0, 800, 600)),
    );
    assert!(echo.effects.is_empty());
}

#[test]
fn normalized_ordinary_float_newcomer_focuses_like_capacity() {
    // A newcomer arriving as a normalized ordinary float (floating with
    // an origin, fit-excluded telling the same story) carries the same
    // one-shot newcomer intent as a fresh capacity float; only the
    // unexplained shape is unsafe.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &tiny_report(1, 1, 2, 0, &[], rect(0, 0, 800, 600)),
    );
    let arrival = accepted(
        &mut engine,
        &tiny_report(2, 2, 3, 0, &[2], rect(0, 0, 800, 600)),
    );
    assert_controlled(&arrival);
    assert_eq!(float_ids(&arrival), vec![id(2)], "newcomer floats");
    assert_eq!(arrival.effects.len(), 1, "one newcomer intent");
    assert_eq!(arrival.effects[0].target.0, id(2));
    assert_eq!(arrival.effects[0].id, format!("af-{}", id(2)));
}

#[test]
fn startup_seed_admits_focus_last_with_no_intent() {
    // Startup admits the spatial roster with the native focus last and
    // mints no intent: the observed native focus stays authoritative on
    // a fresh seed, whatever its spatial position.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = accepted(
        &mut engine,
        &tiny_report(1, 1, 3, 0, &[], rect(0, 0, 800, 600)),
    );
    assert_controlled(&seed);
    assert!(seed.effects.is_empty(), "startup mints no intent");
    assert_eq!(seed.windows.len(), 3, "full roster publishes");
    for n in 0..3 {
        assert!(
            row_of(&seed, &id(n)).allocation.is_some(),
            "startup tile win-{n:06} projects"
        );
    }
}

fn win_ws(id: &str, ws: &str) -> EngineWindow {
    let mut w = win(id, false);
    w.workspace = WorkspaceId(ws.to_owned());
    w.rect = rect(0, 0, 200, 200);
    w
}

fn ws_domain(ws: &str, windows: Vec<EngineWindow>, focused: &str, readable: bool) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            bounds: rect(0, 0, 800, 600),
            gap: 8,
            adjacent: Default::default(),
        },
        outer_gap: 0,
        focused: WindowId(focused.to_owned()),
        windows,
        closed: Vec::new(),
        readable,
        hold: (!readable).then_some(HoldReason::Unreadable),
    }
}

fn two_ws_report(
    request_seq: u64,
    observation_seq: u64,
    ws1: Vec<EngineWindow>,
    ws1_focused: &str,
    ws2: Vec<EngineWindow>,
    ws2_focused: &str,
) -> WorldReport {
    let mut base = tiny_report(
        request_seq,
        observation_seq,
        0,
        0,
        &[],
        rect(0, 0, 800, 600),
    );
    base.topology[0].native_order = vec!["ws1".to_owned(), "ws2".to_owned()];
    base.domains = vec![
        ws_domain("ws1", ws1, ws1_focused, true),
        ws_domain("ws2", ws2, ws2_focused, true),
    ];
    base
}

#[test]
fn empty_domain_fresh_map_focuses_newcomer_once() {
    // A previously reported EMPTY readable controlled domain carries no
    // retained session, but the prior accepted report still proves the
    // empty roster: the first fresh map there focuses exactly once.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = accepted(
        &mut engine,
        &two_ws_report(1, 1, vec![win_ws(&id(0), "ws1")], &id(0), vec![], ""),
    );
    assert!(seed.effects.is_empty(), "startup seeds mint no intent");
    assert!(
        seed.scopes
            .iter()
            .any(|s| s.scope == "out/ws2" && s.authority == ScopeAuthority::Controlled),
        "prior reply proves ws2 readable controlled, got {:?}",
        seed.scopes
    );
    // Native focus stays on the other tile while the newcomer maps in.
    let arrival = accepted(
        &mut engine,
        &two_ws_report(
            2,
            2,
            vec![win_ws(&id(0), "ws1")],
            &id(0),
            vec![win_ws(&id(10), "ws2")],
            "",
        ),
    );
    assert_controlled(&arrival);
    assert_eq!(
        arrival.effects.len(),
        1,
        "fresh map into proven-empty domain focuses once"
    );
    assert_eq!(arrival.effects[0].id, format!("af-{}", id(10)));
    assert_eq!(arrival.effects[0].kind, EffectKind::Focus);
    assert_eq!(arrival.effects[0].target.0, id(10));
    assert!(
        row_of(&arrival, &id(10)).allocation.is_some(),
        "newcomer tiles"
    );
    let echo = accepted(
        &mut engine,
        &two_ws_report(
            3,
            3,
            vec![win_ws(&id(0), "ws1")],
            &id(0),
            vec![win_ws(&id(10), "ws2")],
            &id(10),
        ),
    );
    assert_controlled(&echo);
    assert!(echo.effects.is_empty(), "re-observation mints no intent");
}

#[test]
fn unknown_workspace_without_prior_roster_mints_none() {
    // A workspace with no prior roster at all never infers birth by
    // absence: no surprise auto-focus.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut seed = two_ws_report(1, 1, vec![win_ws(&id(0), "ws1")], &id(0), vec![], "");
    seed.topology[0].native_order = vec!["ws1".to_owned()];
    seed.domains.remove(1);
    accepted(&mut engine, &seed);
    let arrival = accepted(
        &mut engine,
        &two_ws_report(
            2,
            2,
            vec![win_ws(&id(0), "ws1")],
            &id(0),
            vec![win_ws(&id(10), "ws2")],
            "",
        ),
    );
    assert_controlled(&arrival);
    assert!(
        arrival.effects.is_empty(),
        "unknown prior roster mints no intent"
    );
}

#[test]
fn unreadable_recovery_mints_none() {
    // A recovered unreadable scope proves no prior roster either: the
    // first readable map there stays quiet.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut seed = two_ws_report(1, 1, vec![win_ws(&id(0), "ws1")], &id(0), vec![], "");
    seed.domains[1] = ws_domain("ws2", vec![], "", false);
    let seed_reply = accepted(&mut engine, &seed);
    assert!(seed_reply.effects.is_empty());
    let arrival = accepted(
        &mut engine,
        &two_ws_report(
            2,
            2,
            vec![win_ws(&id(0), "ws1")],
            &id(0),
            vec![win_ws(&id(10), "ws2")],
            "",
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "recovered unreadable scope mints no intent"
    );
}

fn overlay_report(
    request_seq: u64,
    observation_seq: u64,
    fullscreen: bool,
    maximized: bool,
    count: usize,
    focused: usize,
) -> WorldReport {
    let mut report = tiny_report(
        request_seq,
        observation_seq,
        count,
        focused,
        &[],
        rect(0, 0, 800, 600),
    );
    report.domains[0].windows[1].fullscreen = fullscreen;
    report.domains[0].windows[1].maximized = maximized;
    if fullscreen || maximized {
        report.domains[0].windows[1].fit_excluded = true;
    }
    report
}

#[test]
fn admission_behind_fullscreen_sibling_mints_none_without_retry() {
    // D01 native activation ownership: a new ordinary C admitted behind
    // a fullscreen B never takes automatic activation, with no later
    // retry once the overlay leaves.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &tiny_report(1, 1, 2, 0, &[], rect(0, 0, 800, 600)),
    );
    let overlaid = accepted(&mut engine, &overlay_report(2, 2, true, false, 2, 1));
    assert_controlled(&overlaid);
    assert!(overlaid.effects.is_empty());
    let arrival = accepted(&mut engine, &overlay_report(3, 3, true, false, 3, 1));
    assert_controlled(&arrival);
    assert!(
        arrival.effects.is_empty(),
        "admission behind fullscreen B mints no intent"
    );
    assert!(
        row_of(&arrival, &id(2)).allocation.is_some(),
        "newcomer still converges honestly"
    );
    let echo = accepted(&mut engine, &overlay_report(4, 4, true, false, 3, 1));
    assert!(echo.effects.is_empty());
    // The overlay leaves with the same roster: still nothing, the
    // suppressed activation was spent, never queued.
    let cleared = accepted(
        &mut engine,
        &tiny_report(5, 5, 3, 1, &[], rect(0, 0, 800, 600)),
    );
    assert_controlled(&cleared);
    assert!(
        cleared.effects.is_empty(),
        "suppressed activation never retries after the overlay"
    );
}

#[test]
fn admission_behind_maximized_sibling_mints_none() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &tiny_report(1, 1, 2, 0, &[], rect(0, 0, 800, 600)),
    );
    let arrival = accepted(&mut engine, &overlay_report(2, 2, false, true, 3, 1));
    assert_controlled(&arrival);
    assert!(
        arrival.effects.is_empty(),
        "admission behind maximized B mints no intent"
    );
    assert!(
        row_of(&arrival, &id(2)).allocation.is_some(),
        "newcomer still converges honestly"
    );
}

#[test]
fn capacity_float_behind_fullscreen_sibling_mints_none() {
    // No capacity-origin branch: a floated newcomer behind a fullscreen
    // sibling is suppressed exactly like a tiled one.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &tiny_report(1, 1, 4, 0, &[], rect(0, 0, 10, 10)),
    );
    let arrival = accepted(&mut engine, &overlay_report(2, 2, true, false, 5, 1));
    assert_controlled(&arrival);
    assert!(
        arrival.effects.is_empty(),
        "floated admission behind fullscreen B mints no intent"
    );
    assert_eq!(
        float_ids(&arrival),
        vec![id(4)],
        "newcomer still capacity-floats honestly"
    );
}

#[test]
fn ordinary_float_behind_fullscreen_sibling_mints_none() {
    // Same ordinary-float path: a newcomer arriving already floating
    // behind a fullscreen sibling is suppressed alike.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &tiny_report(1, 1, 2, 0, &[], rect(0, 0, 800, 600)),
    );
    let mut arrival_report = overlay_report(2, 2, true, false, 3, 1);
    arrival_report.domains[0].windows[2].floating = true;
    arrival_report.domains[0].windows[2].fit_excluded = true;
    let arrival = accepted(&mut engine, &arrival_report);
    assert_controlled(&arrival);
    assert!(
        arrival.effects.is_empty(),
        "ordinary float behind fullscreen B mints no intent"
    );
    assert_eq!(float_ids(&arrival), vec![id(2)]);
}
