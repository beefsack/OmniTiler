use super::rmax03_adapter_tests::test_state;
use super::{
    MonitorArea, dispatch_orientation_intent, keyboard_tick, orientation_overlay_refusal,
    orientation_plan, resolve_chord_target, workspace_domain_for, writable_tokens,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

/// Null/disposable-handle harness: every HWND below is fake (101/102 are
/// never live windows), `me` is fabricated so every pre-write identity
/// gate fails closed on real windows, and logs redirect to a temp dir
/// that is removed afterwards. Only OS reads plus in-memory state run
/// here; no hooks, no setters on foreign windows, no live apps.
const FAKE_HWND: u64 = 101;
const FAKE_HWND_2: u64 = 102;

fn log_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tiler-ori-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp log dir");
    dir
}

fn fake_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn fake_origin() -> crate::snapkey::SnapOrigin {
    crate::snapkey::SnapOrigin {
        hwnd: FAKE_HWND,
        token: "w1".to_owned(),
        pid: 7,
        creation: "creation-ori".to_owned(),
    }
}

fn fake_key() -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd: FAKE_HWND,
        pid: 7,
        creation: "creation-ori".to_owned(),
    }
}

fn ori_intent(
    origin: Option<crate::snapkey::SnapOrigin>,
) -> crate::snapkey::QueuedOrientationIntent {
    crate::snapkey::QueuedOrientationIntent {
        edge: crate::snapkey::SnapEdge::Down,
        origin,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    }
}

fn test_areas() -> Vec<MonitorArea> {
    let rect = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    vec![MonitorArea {
        device: "mon-9".to_owned(),
        work: rect,
        full: rect,
    }]
}

/// Seed a tiled w1+w2 pair on the fabricated output through direct Engine
/// handles (no native calls) and return the Engine domain key the
/// production route re-derives from `areas`.
fn seed_pair(state: &mut super::TileLoop) -> tiler_core::session::DomainKey {
    use tiler_core::directional::WindowId;
    let areas = test_areas();
    let (domain, key) =
        workspace_domain_for("mon-9", "ws-9", &areas, state.inner_gap, state.outer_gap)
            .expect("fabricated domain");
    let none = tiler_core::size_hints::WindowSizeHints::none();
    let bounds = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    for (correlation, tokens) in [
        ("ori-rt-seed-1", vec!["w1"]),
        ("ori-rt-seed-2", vec!["w1", "w2"]),
    ] {
        let rows: Vec<(
            WindowId,
            tiler_core::geometry::Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = tokens
            .iter()
            .map(|token| (WindowId((*token).to_owned()), bounds, none, false))
            .collect();
        let correlation_id =
            tiler_core::ids::CorrelationId::parse(correlation).expect("correlation");
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let revision = state
            .engine
            .session(&key)
            .map(|session| session.accepted_revision())
            .unwrap_or(0);
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation_id,
            revision,
            fp,
            &domain,
            &key,
            state.outer_gap,
            &rows,
            Some(&WindowId("w1".to_owned())),
        );
        let reply = state.engine.handle(&event);
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed {correlation} converges, got {reply:?}"
        );
    }
    key
}

fn engine_snapshot(
    state: &super::TileLoop,
    key: &tiler_core::session::DomainKey,
) -> Option<tiler_core::session::SessionSnapshot> {
    state.engine.session(key).map(|session| session.snapshot())
}

fn log_text(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("owner.log")).unwrap_or_default()
}

fn assert_outcome(dir: &std::path::Path, outcome: &str) {
    let text = log_text(dir);
    assert!(
        text.contains(&format!("\"outcome\":\"{outcome}\"")),
        "expected outcome {outcome} in log, got: {text}"
    );
}

#[test]
fn orientation_missing_origin_settles_without_mutation() {
    // Missing chord origin through the real dispatch route: the `None`
    // branch runs before the suspension/elevation gate and before any
    // host read, so it settles `origin-vanished` deterministically on
    // every runner (including elevated ones) with zero Engine, focus, or
    // view mutation. The log outcome pins the fence.
    let dir = log_dir("ori-missing");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let key = seed_pair(&mut state);
    state.workspaces.ensure_output("mon-9");
    let before = engine_snapshot(&state, &key);
    assert!(before.is_some(), "seeded session exists");
    let active_before = state.workspaces.active_id("mon-9");
    let me = fake_me();
    let fulls: Vec<tiler_core::geometry::Rect> = Vec::new();
    let areas = test_areas();
    dispatch_orientation_intent(&mut state, &me, &fulls, &areas, ori_intent(None));
    assert_outcome(&dir, "origin-vanished");
    assert_eq!(
        engine_snapshot(&state, &key),
        before,
        "missing origin must not mutate the Engine"
    );
    assert_eq!(
        state.workspaces.active_id("mon-9"),
        active_before,
        "missing origin must not switch the view"
    );
    assert!(
        state.snap_advance.is_none(),
        "missing origin carries no focus advance"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn orientation_unknown_origin_refuses_without_mutation() {
    // Fabricated `Some` origin unknown to the owner tables, checked at
    // the production resolver (`resolve_chord_target`, called by every
    // toggle route): the missing-table check refuses `origin-vanished`
    // before member, lifetime, scope, or any live-eligibility state, so
    // it is deterministic on every runner. Engine, view, and advance
    // stay unchanged.
    let mut state = test_state();
    let key = seed_pair(&mut state);
    let output = "mon-9".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    let me = fake_me();
    let areas = test_areas();
    let before = engine_snapshot(&state, &key);
    assert!(before.is_some(), "seeded session exists");
    match resolve_chord_target(&mut state, &me, &areas, &fake_origin()) {
        Err(reject) => assert_eq!(reject.outcome, "origin-vanished"),
        Ok(_) => panic!("unknown origin must refuse"),
    }
    assert!(
        state.snap_advance.is_none(),
        "unknown origin carries no focus advance"
    );
    assert_eq!(
        engine_snapshot(&state, &key),
        before,
        "unknown origin must not mutate the Engine"
    );
    assert_eq!(
        state.workspaces.active_id(&output),
        Some(active),
        "unknown origin must not switch the view"
    );
}

#[test]
fn orientation_resolver_fences_settle_without_mutation() {
    // The production origin resolver (`resolve_chord_target`, called by
    // every toggle route) with fabricated table state and no publication
    // step: a live-but-unfocused origin settles `foreground-changed` and
    // clears any advance, while an owner-advanced continuation to a
    // lifetime-stale member settles `identity-changed` and drops the
    // stale membership. Neither mutates Engine state or the view. The
    // foreground HWND is only ever READ to mirror the continuation.
    let mut state = test_state();
    let key = seed_pair(&mut state);
    let output = "mon-9".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    let member = fake_key();
    assert!(
        state
            .workspaces
            .assign(member.clone(), &output, &active, false)
    );
    state.member_tokens.insert(member.clone(), "w1".to_owned());
    state
        .member_tags
        .insert(member.clone(), "tag-ori-1".to_owned());
    let me = fake_me();
    let areas = test_areas();
    let before = engine_snapshot(&state, &key);
    assert!(before.is_some(), "seeded session exists");

    // Unfocused origin: live in the table, foreground elsewhere.
    state.snap_advance = Some(fake_origin());
    state.snap_origins.insert(FAKE_HWND, fake_origin());
    match resolve_chord_target(&mut state, &me, &areas, &fake_origin()) {
        Err(reject) => assert_eq!(reject.outcome, "foreground-changed"),
        Ok(_) => panic!("unfocused origin must refuse"),
    }
    assert!(
        state.snap_advance.is_none(),
        "external focus clears the advance"
    );

    // Lifetime-stale member behind an owner-advanced continuation: mirror
    // the live foreground into the advance so only the lifetime fence
    // refuses.
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    let advance = crate::snapkey::SnapOrigin {
        hwnd: foreground,
        token: "w1".to_owned(),
        pid: 7,
        creation: "creation-ori".to_owned(),
    };
    state.snap_origins.insert(foreground, advance.clone());
    state.snap_advance = Some(advance);
    match resolve_chord_target(&mut state, &me, &areas, &fake_origin()) {
        Err(reject) => assert_eq!(reject.outcome, "identity-changed"),
        Ok(_) => panic!("lifetime-stale member must refuse"),
    }
    assert!(
        !state.member_tokens.contains_key(&member),
        "lifetime mismatch drops the stale membership"
    );
    assert_eq!(
        engine_snapshot(&state, &key),
        before,
        "resolver fences must not mutate the Engine"
    );
    assert_eq!(
        state.workspaces.active_id(&output),
        Some(active),
        "resolver fences must not switch the view"
    );
}

#[test]
fn orientation_suspended_queue_drops_without_mutation() {
    // The pump-level suspension route (`blocked`): a consumed orientation
    // intent drops with zero Engine, focus, or view mutation and no tick
    // advance. Fully hermetic: no Win32 reads at all.
    let dir = log_dir("ori-suspended");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let key = seed_pair(&mut state);
    state.workspaces.ensure_output("mon-9");
    let before = engine_snapshot(&state, &key);
    assert!(before.is_some(), "seeded session exists");
    let active_before = state.workspaces.active_id("mon-9");
    let me = fake_me();
    let fulls: Vec<tiler_core::geometry::Rect> = Vec::new();
    let areas = test_areas();
    keyboard_tick(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![crate::snapkey::QueuedSnapEvent::Orientation(ori_intent(
            Some(fake_origin()),
        ))],
        Some("test-suspend"),
    );
    assert_eq!(
        engine_snapshot(&state, &key),
        before,
        "suspended queue must not mutate the Engine"
    );
    assert_eq!(
        state.workspaces.active_id("mon-9"),
        active_before,
        "suspended queue must not switch the view"
    );
    assert!(state.snap_advance.is_none());
    assert_eq!(state.tick, 0, "suspended queue advances no tick");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn orientation_plan_accepts_only_matching_tiled() {
    // The extracted production seam against real Engine replies: a
    // ToggleOrientation plan accepts; a ToggleFloat plan and a refusal
    // do not.
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::directional::WindowId;
    let mut state = test_state();
    let _key = seed_pair(&mut state);
    let none = tiler_core::size_hints::WindowSizeHints::none();
    let bounds = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let rows = [
        (WindowId("w1".to_owned()), bounds, none, false),
        (WindowId("w2".to_owned()), bounds, none, false),
    ];
    let event_for =
        |state: &super::TileLoop, correlation: &str, focused: &str, command: CoreCommand| {
            let areas = test_areas();
            let (domain, key) =
                workspace_domain_for("mon-9", "ws-9", &areas, state.inner_gap, state.outer_gap)
                    .expect("fabricated domain");
            let correlation_id =
                tiler_core::ids::CorrelationId::parse(correlation).expect("correlation");
            let fp = crate::tiling::fingerprint(
                &rows
                    .iter()
                    .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                    .collect::<Vec<_>>(),
            );
            let revision = state
                .engine
                .session(&key)
                .map(|session| session.accepted_revision())
                .unwrap_or(0);
            let mut event = crate::tiling::build_reconcile_event_for_floating(
                &state.owner,
                &state.generation,
                &correlation_id,
                revision,
                fp,
                &domain,
                &key,
                state.outer_gap,
                &rows,
                Some(&WindowId(focused.to_owned())),
            );
            event.command = command;
            event
        };
    let reply = state.engine.handle(&event_for(
        &state,
        "ori-seam-toggle",
        "w1",
        CoreCommand::ToggleOrientation {
            window: "w1".to_owned(),
        },
    ));
    let plan = orientation_plan(&reply).expect("matching Tiled accepts");
    assert_eq!(
        plan.kind,
        tiler_core::boundary::TiledKind::ToggleOrientation
    );
    assert_eq!(plan.geometry.len(), 2);
    // Writes extract from the accepted plan: both tiles reproject.
    let writes = crate::workspace_owner::planned_writes(&reply).expect("plan extracts");
    assert_eq!(writes.len(), 2);
    let reply = state.engine.handle(&event_for(
        &state,
        "ori-seam-float",
        "w1",
        CoreCommand::ToggleFloat {
            window: "w1".to_owned(),
            float_rect: None,
        },
    ));
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "float commits, got {reply:?}"
    );
    assert!(
        orientation_plan(&reply).is_none(),
        "wrong Tiled kind refuses"
    );
    let reply = state.engine.handle(&event_for(
        &state,
        "ori-seam-unknown",
        "w1",
        CoreCommand::ToggleOrientation {
            window: "w9".to_owned(),
        },
    ));
    assert!(
        matches!(reply, CoreReply::Rejected { .. }),
        "unknown window refuses, got {reply:?}"
    );
    assert!(orientation_plan(&reply).is_none(), "refusal refuses");
}

#[test]
fn orientation_guard_seams_refuse_overlays_and_scope_writes() {
    // Production-called guard seams with fabricated native state (fake
    // HWND reads only): focused overlays refuse with orientation
    // vocabulary, retained-only siblings take no writes, and the
    // suspension/elevation gate vocabulary holds.
    let mut state = test_state();
    let member = fake_key();
    let member2 = crate::workspace::WindowKey {
        hwnd: FAKE_HWND_2,
        pid: 8,
        creation: "creation-ori-2".to_owned(),
    };
    let observed = |hwnd: u64, token: &str, fullscreen: bool| super::ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        visible: tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd,
            pid: 7,
            process_creation: "creation-ori".to_owned(),
            exe_path: "C:\\test\\app.exe".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
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
            captionless_fullscreen: fullscreen,
            no_activate: false,
            dialog: false,
        },
    };
    // Clean focus proceeds; live fullscreen refuses; retained-only
    // fullscreen refuses through the retained row.
    assert_eq!(
        orientation_overlay_refusal(&[observed(FAKE_HWND, "w1", false)], &[], &member),
        None
    );
    assert_eq!(
        orientation_overlay_refusal(&[observed(FAKE_HWND, "w1", true)], &[], &member),
        Some("orientation-refused-fullscreen")
    );
    let retained = vec![super::RetainedRow {
        key: member.clone(),
        token: "w1".to_owned(),
        rect: None,
        maximized: false,
        fullscreen: true,
        facts: None,
    }];
    assert_eq!(
        orientation_overlay_refusal(&[], &retained, &member),
        Some("orientation-refused-fullscreen")
    );
    // Write scoping: w2 assigned but never freshly observed takes no
    // writes while w1 does.
    let output = "mon-9".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    assert!(
        state
            .workspaces
            .assign(member.clone(), &output, &active, false)
    );
    assert!(
        state
            .workspaces
            .assign(member2.clone(), &output, &active, false)
    );
    state.member_tokens.insert(member.clone(), "w1".to_owned());
    state.member_tokens.insert(member2.clone(), "w2".to_owned());
    let observed = vec![observed(FAKE_HWND, "w1", false)];
    assert_eq!(
        writable_tokens(&state, &output, &active, &observed),
        std::collections::HashSet::from(["w1".to_owned()]),
        "retained-only sibling takes no writes"
    );
    // Suspension/elevation gate vocabulary: blocked or elevated settles,
    // quiet proceeds.
    assert!(crate::tiling::toggle_gate_outcome(true, false).is_some());
    assert!(crate::tiling::toggle_gate_outcome(false, true).is_some());
    assert!(crate::tiling::toggle_gate_outcome(false, false).is_none());
}
