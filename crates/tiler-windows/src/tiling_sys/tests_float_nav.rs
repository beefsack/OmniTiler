#![forbid(unsafe_code)]
// Items 10 + 18 adapter seams through a real `TileLoop` against the real
// retained Engine (no HWNDs, hooks, or live effects): float-subject
// classification reads the Engine exception lane, candidate assembly
// honors domain/sticky/automatic boundaries, and the shared selection
// picks through the assembled layer. Native clear/actuation timing stays
// user-owned live acceptance.
use super::rmax03_adapter_tests::test_state;
use super::{
    ObservedWindow, RetainedRow, engine_is_automatic_fixed, engine_is_float, float_focus_candidates,
};

fn key(hwnd: u64) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid: 1000 + hwnd as u32,
        creation: format!("c{hwnd:016x}"),
    }
}

fn mk_observed(hwnd: u64, token: &str, x: i32, y: i32) -> ObservedWindow {
    let rect = tiler_core::geometry::Rect {
        x,
        y,
        w: 300,
        h: 200,
    };
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd,
            pid: 1000 + hwnd as u32,
            process_creation: format!("c{hwnd:016x}"),
            exe_path: "app.exe".to_owned(),
            user_sid: "sid".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }
}

fn seed_domain(
    state: &mut super::TileLoop,
    output: &str,
    workspace: &str,
    bounds: tiler_core::geometry::Rect,
    members: &[(&str, bool)],
    focused: &str,
) -> (
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
) {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::directional::WindowId;
    use tiler_core::ids::CorrelationId;
    let domain = crate::workspace_owner::workspace_domain(output, workspace, bounds, 8);
    let carried: Vec<(
        WindowId,
        tiler_core::geometry::Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = members
        .iter()
        .map(|(token, floating)| {
            (
                WindowId((*token).to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                *floating,
            )
        })
        .collect();
    let revision = state
        .engine
        .session(&domain.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("float-nav-seed").expect("correlation");
    let fp = crate::tiling::fingerprint(
        &members
            .iter()
            .map(|(t, _)| ((*t).to_owned(), bounds))
            .collect::<Vec<_>>(),
    );
    let event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focused.to_owned())),
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed must converge, got {reply:?}"
    );
    let _ = CoreCommand::Reconcile;
    domain
}

fn float_token(
    state: &mut super::TileLoop,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    bounds: tiler_core::geometry::Rect,
    members: &[(&str, bool)],
    focused: &str,
    window: &str,
) {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::directional::WindowId;
    use tiler_core::ids::CorrelationId;
    let carried: Vec<(
        WindowId,
        tiler_core::geometry::Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = members
        .iter()
        .map(|(token, floating)| {
            (
                WindowId((*token).to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                *floating,
            )
        })
        .collect();
    let revision = state
        .engine
        .session(&domain.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("float-nav-float").expect("correlation");
    let fp = crate::tiling::fingerprint(
        &members
            .iter()
            .map(|(t, _)| ((*t).to_owned(), bounds))
            .collect::<Vec<_>>(),
    );
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focused.to_owned())),
    );
    event.command = CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect: None,
    };
    let reply = state.engine.handle(&event);
    assert!(matches!(reply, CoreReply::Tiled(_)), "float must commit");
}

#[test]
fn float_subject_gate_reads_the_engine_exception_lane() {
    // Intentional floats hold no tile slot: the subject gate keys off the
    // Engine exception, not geometry or membership. Automatic fixed marks
    // stay absent for intentional floats.
    let mut state = test_state();
    let bounds = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let domain = seed_domain(
        &mut state,
        &output,
        &ws,
        bounds,
        &[("w1", false), ("w2", false)],
        "w1",
    );
    assert!(!engine_is_float(&state, &output, &ws, "w1"));
    float_token(
        &mut state,
        &domain,
        bounds,
        &[("w1", false), ("w2", false)],
        "w1",
        "w1",
    );
    assert!(engine_is_float(&state, &output, &ws, "w1"));
    assert!(!engine_is_float(&state, &output, &ws, "w2"));
    assert!(!engine_is_automatic_fixed(&state, &output, &ws, "w1"));
    assert!(!engine_is_automatic_fixed(&state, &output, &ws, "w2"));
    // Unknown domain/session reads false, never guesses.
    assert!(!engine_is_float(&state, "mon-9", &ws, "w1"));
}

fn mk_retained(
    hwnd: u64,
    token: &str,
    rect: Option<(i32, i32)>,
    maximized: bool,
    fullscreen: bool,
    minimized: bool,
) -> RetainedRow {
    RetainedRow {
        key: key(hwnd),
        token: token.to_owned(),
        rect: rect.map(|(x, y)| tiler_core::geometry::Rect {
            x,
            y,
            w: 300,
            h: 200,
        }),
        maximized,
        fullscreen,
        facts: Some(crate::tiling::WindowFacts {
            visible: true,
            minimized,
            maximized,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: fullscreen,
            no_activate: false,
            dialog: false,
        }),
    }
}

#[test]
fn float_focus_candidates_share_one_domain_layer() {
    // Observed encounter order, then verified retained maximized rows.
    // Tiles, other-output windows, unassigned windows, fullscreen and
    // stale rows never enter. Sticky rides without an Engine exception
    // and qualifies output-wide (even homed off-workspace); ordinary
    // floats ride the exception lane in the strict subject domain.
    let mut state = test_state();
    let bounds = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let ws_b = state
        .workspaces
        .workspace_ids(&output)
        .into_iter()
        .find(|id| *id != ws)
        .expect("sibling");
    let other_output = "mon-2".to_owned();
    state.workspaces.ensure_output(&other_output);
    let ws2 = state.workspaces.active_id(&other_output).expect("active");
    let domain = seed_domain(
        &mut state,
        &output,
        &ws,
        bounds,
        &[("w1", false), ("w2", false), ("w7", false)],
        "w1",
    );
    // Carried observations track prior floats: a stale all-tiled
    // observation would converge the earlier floats back into tiles.
    float_token(
        &mut state,
        &domain,
        bounds,
        &[("w1", false), ("w2", false), ("w7", false)],
        "w1",
        "w1",
    );
    float_token(
        &mut state,
        &domain,
        bounds,
        &[("w1", true), ("w2", false), ("w7", false)],
        "w2",
        "w2",
    );
    float_token(
        &mut state,
        &domain,
        bounds,
        &[("w1", true), ("w2", true), ("w7", false)],
        "w7",
        "w7",
    );
    let domain2 = seed_domain(
        &mut state,
        &other_output,
        &ws2,
        bounds,
        &[("w3", false)],
        "w3",
    );
    float_token(&mut state, &domain2, bounds, &[("w3", false)], "w3", "w3");
    // Membership: subject, ordinary float and tiled control home ws; w3
    // homes the other output; w5 stays unassigned. Sticky s1 homes ws
    // with no Engine exception (pruned-domain shape), s2 a sibling
    // workspace on the same output, s3 the other output (excluded).
    assert!(state.workspaces.assign(key(101), &output, &ws, false));
    assert!(state.workspaces.assign(key(102), &output, &ws, false));
    assert!(state.workspaces.assign(key(104), &output, &ws, false));
    assert!(
        state
            .workspaces
            .assign(key(103), &other_output, &ws2, false)
    );
    assert!(state.workspaces.assign(key(106), &output, &ws, false));
    assert!(state.workspaces.assign(key(107), &output, &ws_b, false));
    assert!(
        state
            .workspaces
            .assign(key(108), &other_output, &ws2, false)
    );
    for (hwnd, token, home_output, home_ws) in [
        (101, "w1", &output, &ws),
        (102, "w2", &output, &ws),
        (104, "w4", &output, &ws),
        (106, "s1", &output, &ws),
        (107, "s2", &output, &ws_b),
        (103, "w3", &other_output, &ws2),
        (108, "s3", &other_output, &ws2),
        (109, "w7", &output, &ws),
        (110, "w9", &output, &ws),
        (111, "w8", &output, &ws),
        // w10's live token disagrees with its retained row: recycled
        // identity must never ride the stale row into candidacy.
        (112, "wX", &output, &ws),
    ] {
        assert!(
            state
                .workspaces
                .assign(key(hwnd), home_output, home_ws, false),
            "assign {token}"
        );
        state.member_tokens.insert(key(hwnd), token.to_owned());
    }
    state.sticky.insert(key(106), false);
    state.sticky.insert(key(107), false);
    state.sticky.insert(key(108), false);
    let observed = vec![
        mk_observed(101, "w1", 500, 500),
        mk_observed(104, "w4", 100, 100),
        mk_observed(103, "w3", 700, 100),
        mk_observed(105, "w5", 800, 100),
        mk_observed(106, "s1", 900, 900),
        mk_observed(102, "w2", 600, 0),
        mk_observed(107, "s2", 700, 100),
        mk_observed(108, "s3", 750, 100),
    ];
    // Retained union: maximized float w7 qualifies; maximized
    // fullscreen w9, frameless minimized w8 and token-mismatched w10
    // refuse.
    let retained = vec![
        mk_retained(109, "w7", Some((400, 300)), true, false, false),
        mk_retained(110, "w9", Some((450, 320)), true, true, false),
        mk_retained(111, "w8", None, false, false, true),
        mk_retained(112, "w10", Some((420, 310)), true, false, false),
    ];
    let candidates = float_focus_candidates(&state, &observed, &retained, &output, &ws, "w1");
    let tokens: Vec<&str> = candidates.iter().map(|c| c.token.as_str()).collect();
    assert_eq!(
        tokens,
        vec!["s1", "w2", "s2", "w7"],
        "observed encounter order then retained maximized union"
    );
    assert!(
        candidates
            .iter()
            .find(|c| c.token == "s1")
            .expect("sticky")
            .sticky
    );
    assert!(
        candidates
            .iter()
            .find(|c| c.token == "s2")
            .expect("non-home sticky")
            .sticky
    );
    assert!(
        !candidates
            .iter()
            .find(|c| c.token == "w2")
            .expect("float")
            .sticky
    );
    assert!(
        !candidates
            .iter()
            .find(|c| c.token == "w7")
            .expect("retained maximized float")
            .sticky
    );
    // End to end through the shared race: nearer ordinary w2 wins right.
    assert_eq!(
        crate::tiling::select_float_focus_target(500, 500, "right", &candidates),
        Some("w2".to_owned())
    );
}

#[test]
fn snapped_float_reconciles_without_losing_exception_focus_or_frame() {
    // Post-snap steady state through the real Engine: a reconcile
    // carrying the moved float frame converges, keeps the float
    // exception, retains the moved frame, and leaves tiled focus
    // untouched, so the next tick never fights the snapped window.
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    use tiler_core::ids::CorrelationId;
    let mut state = test_state();
    let bounds = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let domain = seed_domain(
        &mut state,
        &output,
        &ws,
        bounds,
        &[("w1", false), ("w2", false)],
        "w1",
    );
    float_token(
        &mut state,
        &domain,
        bounds,
        &[("w1", false), ("w2", false)],
        "w1",
        "w1",
    );
    let moved = tiler_core::geometry::Rect {
        x: 1202,
        y: 104,
        w: 394,
        h: 592,
    };
    let focus_before = state.engine.session(&domain.1).expect("session").focus();
    let carried = vec![
        (
            WindowId("w1".to_owned()),
            moved,
            tiler_core::size_hints::WindowSizeHints::none(),
            true,
        ),
        (
            WindowId("w2".to_owned()),
            bounds,
            tiler_core::size_hints::WindowSizeHints::none(),
            false,
        ),
    ];
    let revision = state
        .engine
        .session(&domain.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("float-nav-reconcile").expect("correlation");
    let fp = crate::tiling::fingerprint(&[("w1".to_owned(), moved), ("w2".to_owned(), bounds)]);
    let event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId("w2".to_owned())),
    );
    let reply = state.engine.handle(&event);
    match &reply {
        CoreReply::Projection(plan) => assert!(
            !plan.geometry.iter().any(|g| g.window.0 == "w1"),
            "snapped float takes no correction write"
        ),
        CoreReply::Tiled(plan) => assert!(
            !plan.geometry.iter().any(|g| g.window.0 == "w1"),
            "snapped float takes no correction write"
        ),
        other => panic!("moved float must converge, got {other:?}"),
    }
    let session = state.engine.session(&domain.1).expect("session");
    assert!(
        session.is_exception(&WindowId("w1".to_owned())),
        "snapped window stays floating"
    );
    assert_eq!(session.focus(), focus_before, "tiled focus untouched");
}
