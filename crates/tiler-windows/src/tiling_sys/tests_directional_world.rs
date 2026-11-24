//! Stage-5 directional seam (offline only): float origins dispatch world
//! commands and the core owns every logical decision. The adapter only
//! captures truthful normalized facts (lanes, tags, frames) and would
//! mechanically actuate the reply; no float branch bypasses the world.
//! No HWNDs, hooks, or live effects: the sequencer plus the core decide,
//! and the canonical reply encoding round-trips the new directional result.

use super::rmax03_adapter_tests::test_state;
use super::world_sequencer::WorldSequencer;
use super::{MonitorArea, ObservedTarget, ObservedWindow, RetainedRow};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report::{CommandResult, WorldCommand};

fn area() -> MonitorArea {
    MonitorArea {
        device: "DEV1".to_owned(),
        work: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        },
        full: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        dpi: 96,
    }
}

fn plain_facts() -> crate::tiling::WindowFacts {
    crate::tiling::WindowFacts {
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
    }
}

fn key(hwnd: u64) -> crate::native_guards::WindowKey {
    crate::native_guards::WindowKey {
        hwnd,
        pid: 200 + hwnd as u32,
        creation: format!("proc-{hwnd}"),
    }
}

fn observed(hwnd: u64, token: &str, x: i32, y: i32) -> ObservedWindow {
    let visible = Rect {
        x,
        y,
        w: 200,
        h: 200,
    };
    let outer = Rect {
        x: x - 8,
        y: y - 8,
        w: 216,
        h: 216,
    };
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer,
        visible,
        insets: crate::tiling::FrameInsets::measure(outer, visible),
        identity: ObservedTarget {
            hwnd,
            pid: 200 + hwnd as u32,
            process_creation: format!("proc-{hwnd}"),
            exe_path: "C:\\Apps\\app.exe".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            tag: format!("tag-{hwnd}"),
        },
        facts: plain_facts(),
        member_tag: Some(format!("tag-{hwnd}")),
    }
}

fn seed_member(state: &mut super::TileLoop, hwnd: u64, token: &str) {
    let member = key(hwnd);
    state.member_tokens.insert(member.clone(), token.to_owned());
    state
        .member_tags
        .insert(member.clone(), format!("tag-{hwnd}"));
    state.member_identity.insert(
        member.clone(),
        crate::model::ProcessIdentity {
            pid: 200 + hwnd as u32,
            process_creation: format!("proc-{hwnd}"),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    // v4 derives fixed-size intent from truthful raw hints: the automatic
    // float origin is a fixed hint pair the core can prove. The adapter
    // float table is no longer a classification input.
    state.hint_logged.insert(
        token.to_owned(),
        tiler_core::size_hints::WindowSizeHints {
            min_w: Some(200),
            max_w: Some(200),
            min_h: Some(200),
            max_h: Some(200),
        },
    );
    state.floated.insert(member);
}

/// Bootstrap one output with two intentional floats and deliver both
/// observations, so the core homes them before any command.
fn bootstrap_floats(state: &mut super::TileLoop, seq: &mut WorldSequencer) -> Vec<ObservedWindow> {
    state.ensure_device("DEV1");
    seed_member(state, 101, "w1");
    seed_member(state, 102, "w2");
    let observed = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    for _ in 0..2 {
        seq.invalidate();
        let report = seq
            .try_dispatch(
                &state.inputs(),
                Some(&observed),
                &retained,
                &areas,
                0,
                0,
                None,
            )
            .expect("observation dispatches");
        match seq.deliver(&report) {
            WorldOutcome::Accepted(reply) => match seq.ack(&reply) {
                super::world_sequencer::AckOutcome::Applied(_) => {}
                other => panic!("expected applied, got {other:?}"),
            },
            other => panic!("expected accepted, got {other:?}"),
        }
    }
    observed
}

fn dispatch(
    state: &mut super::TileLoop,
    seq: &mut WorldSequencer,
    observed: &[ObservedWindow],
    command: WorldCommand,
) -> tiler_core::world_report::WorldReplyV4 {
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(observed),
            &retained,
            &areas,
            0,
            0,
            Some(command),
        )
        .expect("command dispatches");
    // Truthful capture: both managed windows ride with membership evidence
    // and no invented frames; the command names the window only.
    assert_eq!(report.windows.len(), 2);
    match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => {
            match seq.ack(&reply) {
                super::world_sequencer::AckOutcome::Applied(_) => {}
                other => panic!("expected applied, got {other:?}"),
            }
            reply
        }
        other => panic!("expected accepted, got {other:?}"),
    }
}

#[test]
fn automatic_float_focus_refuses_from_core_without_adapter_branches() {
    // No adapter bypass exists: the core refuses a directional focus from
    // an automatic fixed float with the floating vocabulary and no effect.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let observed = bootstrap_floats(&mut state, &mut seq);
    let reply = dispatch(
        &mut state,
        &mut seq,
        &observed,
        WorldCommand::FocusDirectional {
            window: tiler_core::world_report::FocusDirectionalSubject::Window(WindowId(
                "w1".to_owned(),
            )),
            direction: tiler_core::directional::Direction::Right,
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional {
            outcome,
            target,
            rect,
            cause,
            ..
        }) => {
            assert_eq!(
                outcome.as_str(),
                "focus-refused-floating",
                "cause={cause:?} target={target:?}"
            );
            assert!(target.is_none());
            assert!(rect.is_none());
        }
        other => panic!("world-owned automatic focus refusal, got {other:?}"),
    }
    assert!(
        reply.focus_request.is_none() && reply.transfers.is_empty(),
        "refusals stay atomic: no effects, no transfers"
    );
}

#[test]
fn automatic_floats_refuse_from_core_without_adapter_branches() {
    // The D4 latch rides the report as `fixed_auto`; the core refuses
    // with the floating vocabulary and the adapter writes nothing.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let observed = bootstrap_floats(&mut state, &mut seq);
    state.fixed_auto_seen.insert(key(101));
    let reply = dispatch(
        &mut state,
        &mut seq,
        &observed,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-refused-floating");
        }
        other => panic!("world-owned automatic refusal, got {other:?}"),
    }
    assert!(
        reply.focus_request.is_none() && reply.transfers.is_empty(),
        "refusals stay atomic: no effects, no transfers"
    );
}

#[test]
fn unknown_windows_refuse_stale_with_preserved_cause() {
    // A vanished subject is stale lifetime evidence, not a bare rejection:
    // focus and move share one vocabulary with the preserved cause, and
    // the refusal stays atomic.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let observed = bootstrap_floats(&mut state, &mut seq);
    for (command, token) in [
        (
            WorldCommand::FocusDirectional {
                window: tiler_core::world_report::FocusDirectionalSubject::Window(WindowId(
                    "w9".to_owned(),
                )),
                direction: tiler_core::directional::Direction::Right,
            },
            "focus-float-refused-stale",
        ),
        (
            WorldCommand::MoveDirectional {
                window: tiler_core::directional::WindowSubject::Window(WindowId("w9".to_owned())),
                direction: tiler_core::directional::Direction::Right,
                same_axis: tiler_core::directional::SameAxisMove::default(),
            },
            "move-float-refused-stale",
        ),
    ] {
        let reply = dispatch(&mut state, &mut seq, &observed, command);
        match seq.last_command_result() {
            Some(CommandResult::Directional {
                outcome,
                target,
                cause,
                ..
            }) => {
                assert_eq!(outcome.as_str(), token);
                assert_eq!(target, &Some(WindowId("w9".to_owned())));
                assert_eq!(cause, &Some("unknown-window".to_owned()));
            }
            other => panic!("world-owned stale refusal, got {other:?}"),
        }
        assert!(
            reply.focus_request.is_none() && reply.transfers.is_empty(),
            "stale refusals stay atomic"
        );
    }
}

/// No-focused-window directional focus (User 2026-10-26): the hook route
/// with no chord origin dispatches a subject-free FocusDirectional; the
/// core starts at the neutral action context and crosses by the existing
/// selection. Hermetic through the pending seams (injected observation,
/// suspend, elevation, foreground). The only native reads left are the
/// real foreground HWND (never matches injected members, so the global
/// focus stays honestly empty) and the real cursor sample (single-output
/// routes resolve through the registry fallback; two-output dispatches
/// pass explicit cursors).
fn nofocus_log_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tiler-dir-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

fn nofocus_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn nofocus_areas() -> (Vec<super::MonitorArea>, Vec<Rect>) {
    let areas = vec![
        super::MonitorArea {
            device: "DEV1".to_owned(),
            work: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 560,
            },
            full: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            dpi: 96,
        },
        super::MonitorArea {
            device: "DEV2".to_owned(),
            work: Rect {
                x: 800,
                y: 0,
                w: 800,
                h: 560,
            },
            full: Rect {
                x: 800,
                y: 0,
                w: 800,
                h: 600,
            },
            dpi: 96,
        },
    ];
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    (areas, fulls)
}

fn nofocus_seed_member(
    state: &mut super::TileLoop,
    me: &crate::model::ProcessIdentity,
    hwnd: u64,
    token: &str,
    pid: u32,
    creation: &str,
) {
    use super::inject_pending_tag;
    let member = crate::native_guards::WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
    };
    state.member_tokens.insert(member.clone(), token.to_owned());
    state.member_identity.insert(
        member.clone(),
        crate::model::ProcessIdentity {
            pid,
            process_creation: creation.to_owned(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            exe_path: me.exe_path.clone(),
        },
    );
    state.member_tags.insert(member, format!("tag-{hwnd}"));
    inject_pending_tag(hwnd, Some(format!("tag-{hwnd}")));
}

fn nofocus_observed(
    me: &crate::model::ProcessIdentity,
    hwnd: u64,
    token: &str,
    pid: u32,
    creation: &str,
    x: i32,
) -> ObservedWindow {
    let visible = Rect {
        x,
        y: 10,
        w: 200,
        h: 200,
    };
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: visible,
        visible,
        insets: crate::tiling::FrameInsets::measure(visible, visible),
        identity: ObservedTarget {
            hwnd,
            pid,
            process_creation: creation.to_owned(),
            exe_path: me.exe_path.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            tag: format!("tag-{hwnd}"),
        },
        facts: plain_facts(),
        member_tag: Some(format!("tag-{hwnd}")),
    }
}

struct NoFocusFixture {
    state: super::TileLoop,
    me: crate::model::ProcessIdentity,
    store: crate::storage::LedgerStore,
    dir: std::path::PathBuf,
    observed: Vec<ObservedWindow>,
    areas: Vec<super::MonitorArea>,
    fulls: Vec<Rect>,
    wire1: String,
    wire2: String,
}

/// Seed two outputs with managed windows (plus an optional second
/// source window), converged with no foreground. Returns the fixture for
/// direct [`super::world_tick::run_world_command`] dispatches.
fn nofocus_seeded(name: &str, source_windows: usize, with_target: bool) -> NoFocusFixture {
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
        inject_pending_observe, inject_pending_suspend,
    };
    clear_pending_seams();
    let dir = nofocus_log_dir(name);
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = nofocus_me();
    let (areas, fulls) = nofocus_areas();
    state.ensure_device("DEV1");
    state.ensure_device("DEV2");
    let wire1 = state.wire_of("DEV1").expect("wire 1");
    let wire2 = state.wire_of("DEV2").expect("wire 2");
    let mut members = Vec::new();
    for (index, (hwnd, token, pid, creation, x)) in [
        (0x6101u64, "w-m1", 9101u32, "c-6101", 10),
        (0x6102u64, "w-m2", 9102u32, "c-6102", 300),
    ]
    .into_iter()
    .enumerate()
    {
        if index < source_windows {
            members.push((hwnd, token, pid, creation, x));
        }
    }
    if with_target {
        members.push((0x6201u64, "w-n1", 9201u32, "c-6201", 810));
    }
    for (hwnd, token, pid, creation, _) in &members {
        nofocus_seed_member(&mut state, &me, *hwnd, token, *pid, creation);
    }
    let observed: Vec<ObservedWindow> = members
        .iter()
        .map(|(hwnd, token, pid, creation, x)| {
            nofocus_observed(&me, *hwnd, token, *pid, creation, *x)
        })
        .collect();
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    let seed = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
        false,
    )
    .expect("seed converges");
    assert!(seed.ready, "seed converges");
    NoFocusFixture {
        state,
        me,
        store,
        dir,
        observed,
        areas,
        fulls,
        wire1,
        wire2,
    }
}

fn nofocus_command(direction: &str) -> WorldCommand {
    WorldCommand::FocusDirectional {
        window: tiler_core::world_report::FocusDirectionalSubject::NoFocus,
        direction: tiler_core::directional::Direction::parse_wire(direction).expect("direction"),
    }
}

fn nofocus_dispatch(
    fx: &mut NoFocusFixture,
    cursor: Option<(i32, i32)>,
    direction: &str,
) -> Option<super::world_tick::WorldCommandOutcome> {
    super::world_tick::run_world_command(
        &mut fx.state,
        &fx.me,
        &fx.store,
        &fx.dir,
        &fx.observed,
        &[],
        &fx.areas,
        0,
        &fx.fulls,
        nofocus_command(direction),
        cursor,
    )
}

#[test]
fn no_focus_dispatch_crosses_occupied_source_to_existing_selection() {
    // Occupied source (two windows) with no focused window: the core
    // crosses right from the cursor-resolved output and effects the
    // destination's remembered selection, never a source window.
    let mut fx = nofocus_seeded("nofocus-cross", 2, true);
    let outcome = nofocus_dispatch(&mut fx, Some((10, 10)), "right").expect("accepted");
    let reply = &outcome.reply;
    assert!(outcome.result.is_none(), "success rides the focus request");
    let focus = reply.focus_request.as_ref().expect("one focus request");
    assert_eq!(focus.target.0, "w-n1", "existing destination selection");
    assert!(
        focus.id.starts_with("focus-"),
        "one-shot focus id: {}",
        focus.id
    );
    let _ = (fx.wire1, fx.wire2);
    super::clear_pending_seams();
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn no_focus_dispatch_empty_source_crosses() {
    // Empty source workspace: no local subject exists, yet the adjacent
    // output's selection is still reached.
    let mut fx = nofocus_seeded("nofocus-empty-src", 0, true);
    let outcome = nofocus_dispatch(&mut fx, Some((10, 10)), "right").expect("accepted");
    let focus = outcome
        .reply
        .focus_request
        .as_ref()
        .expect("empty source still crosses");
    assert_eq!(focus.target.0, "w-n1");
    super::clear_pending_seams();
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn no_focus_dispatch_empty_destination_retains() {
    // Adjacent output exists but is empty: no remembered selection, so
    // no effect rather than an invented target.
    let mut fx = nofocus_seeded("nofocus-empty-dst", 1, false);
    let outcome = nofocus_dispatch(&mut fx, Some((10, 10)), "right").expect("accepted");
    let reply = &outcome.reply;
    assert!(
        reply.focus_request.is_none(),
        "empty target: {:?}",
        reply.focus_request
    );
    match &outcome.result {
        Some(CommandResult::Directional { outcome, cause, .. }) => {
            assert_eq!(outcome.as_str(), "focus-unfocused-retained");
            assert_eq!(cause.as_deref(), Some("no-target"));
        }
        other => panic!("expected retained no-op, got {other:?}"),
    }
    super::clear_pending_seams();
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn no_focus_keyboard_route_without_origin_reaches_core() {
    // Production hook route with no chord origin on a single output:
    // the intent reaches the core (no origin-vanished settle) and the
    // core retains with no adjacent output, actuating nothing.
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
        inject_pending_observe, inject_pending_suspend, inject_pending_tag, keyboard_tick,
    };
    use crate::snapkey::{QueuedIntent, QueuedSnapEvent, SnapEdge, SnapOp};
    clear_pending_seams();
    let dir = nofocus_log_dir("nofocus-route");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = nofocus_me();
    let areas = vec![area()];
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.ensure_device("DEV1");
    nofocus_seed_member(&mut state, &me, 0x6301, "w-1", 9301, "c-6301");
    inject_pending_tag(0x6301, Some("tag-0x6301".to_owned()));
    let observed = vec![nofocus_observed(&me, 0x6301, "w-1", 9301, "c-6301", 10)];
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    let seed = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
        false,
    )
    .expect("seed converges");
    assert!(seed.ready, "seed converges");
    // The real cursor sample rides this tick; the single-output
    // registry fallback keeps source resolution deterministic.
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    inject_pending_tag(0x6301, Some("tag-0x6301".to_owned()));
    keyboard_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &fulls,
        &areas,
        vec![QueuedSnapEvent::Intent(QueuedIntent {
            op: SnapOp::Focus,
            direction: tiler_core::directional::Direction::Right,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        })],
        None,
    );
    assert_eq!(state.tick, 1, "production dispatch advances one tick");
    assert!(state.snap_advance.is_none());
    let text = std::fs::read_to_string(dir.join("owner.log")).expect("owner log");
    let lines: Vec<serde_json::Value> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("log line"))
        .collect();
    let snaps: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|line| line["event"] == serde_json::json!("snap"))
        .collect();
    assert_eq!(snaps.len(), 1, "one snap settle: {lines:?}");
    assert_eq!(snaps[0]["op"], serde_json::json!("focus"));
    assert_eq!(snaps[0]["direction"], serde_json::json!("right"));
    assert_eq!(
        snaps[0]["outcome"],
        serde_json::json!("focus-unfocused-retained"),
        "reaches core, not origin-vanished: {lines:?}"
    );
    assert_eq!(snaps[0]["cause"], serde_json::json!("no-adjacent-output"));
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_focus_keyboard_route_keeps_suspend_and_elevation_fences() {
    // Suspension and elevation still fence the no-origin route before any
    // core dispatch, exactly like the subject route.
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
        inject_pending_observe, inject_pending_suspend, inject_pending_tag, keyboard_tick,
    };
    use crate::snapkey::{QueuedIntent, QueuedSnapEvent, SnapEdge, SnapOp};
    for (name, suspend, elevated, outcome) in [
        ("nofocus-suspend", Some(true), Some(false), "suspended"),
        (
            "nofocus-elevated",
            Some(false),
            Some(true),
            "elevated-foreground",
        ),
    ] {
        clear_pending_seams();
        let dir = nofocus_log_dir(name);
        let mut state = test_state();
        state.log_path = dir.join("owner.log");
        let me = nofocus_me();
        let areas = vec![area()];
        let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
        state.ensure_device("DEV1");
        nofocus_seed_member(&mut state, &me, 0x6401, "w-1", 9401, "c-6401");
        inject_pending_tag(0x6401, Some("tag-0x6401".to_owned()));
        let observed = vec![nofocus_observed(&me, 0x6401, "w-1", 9401, "c-6401", 10)];
        let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
        inject_pending_observe(observed.clone(), Vec::new(), false);
        inject_pending_suspend(Some(false));
        inject_pending_elevated(Some(false));
        inject_pending_foreground(Some(0));
        super::world_tick::run_observation_tick(
            &mut state,
            &me,
            &store,
            &dir,
            &observed,
            &[],
            &areas,
            0,
            &fulls,
            None,
            false,
        )
        .expect("seed converges");
        inject_pending_observe(observed.clone(), Vec::new(), false);
        inject_pending_suspend(suspend);
        inject_pending_elevated(elevated);
        inject_pending_foreground(Some(0));
        inject_pending_tag(0x6401, Some("tag-0x6401".to_owned()));
        keyboard_tick(
            &mut state,
            &me,
            &store,
            &dir,
            &fulls,
            &areas,
            vec![QueuedSnapEvent::Intent(QueuedIntent {
                op: SnapOp::Focus,
                direction: tiler_core::directional::Direction::Right,
                edge: SnapEdge::Down,
                origin: None,
                consumed: true,
                announce: true,
                tick: std::time::Instant::now(),
            })],
            None,
        );
        assert!(state.snap_advance.is_none());
        let text = std::fs::read_to_string(dir.join("owner.log")).expect("owner log");
        let lines: Vec<serde_json::Value> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("log line"))
            .collect();
        let snaps: Vec<&serde_json::Value> = lines
            .iter()
            .filter(|line| line["event"] == serde_json::json!("snap"))
            .collect();
        assert_eq!(snaps.len(), 1, "one snap settle: {lines:?}");
        assert_eq!(
            snaps[0]["outcome"],
            serde_json::Value::from(outcome),
            "fence holds: {lines:?}"
        );
        clear_pending_seams();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
