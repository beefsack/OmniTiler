//! Stage 2c cursor-first boundary tests (offline only): capture projects
//! the sampled cursor plus the four retained targeting settings, the
//! sequencer envelopes them into the report, production selects resolve in
//! core (never from an adapter output), sends stay on the focused window's
//! source output, explicit outputs stay screen-specific, and the focused
//! fullscreen-game lane reports no cursor. No HWNDs, hooks, or live effects.

use super::rmax03_adapter_tests::test_state;
use super::world_capture::capture_world;
use super::world_sequencer::WorldSequencer;
use super::{MonitorArea, ObservedWindow};
use tiler_core::geometry::Rect;

fn area(device: &str, x: i32) -> MonitorArea {
    MonitorArea {
        device: device.to_owned(),
        work: Rect {
            x,
            y: 0,
            w: 800,
            h: 560,
        },
        full: Rect {
            x,
            y: 0,
            w: 800,
            h: 600,
        },
    }
}

fn areas() -> Vec<MonitorArea> {
    vec![area("DEV-A", 0), area("DEV-B", 800)]
}

fn fulls() -> Vec<Rect> {
    vec![
        Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        Rect {
            x: 800,
            y: 0,
            w: 800,
            h: 600,
        },
    ]
}

fn facts() -> crate::tiling::WindowFacts {
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

fn observed_window(hwnd: u64, token: &str, pid: u32, creation: &str, x: i32) -> ObservedWindow {
    let visible = Rect {
        x,
        y: 0,
        w: 40,
        h: 90,
    };
    let outer = Rect {
        x: x - 8,
        y: -8,
        w: 56,
        h: 106,
    };
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer,
        visible,
        insets: crate::tiling::FrameInsets::measure(outer, visible),
        identity: super::ObservedTarget {
            hwnd,
            pid,
            process_creation: creation.to_owned(),
            exe_path: "C:\\Apps\\app.exe".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            tag: format!("tag-{token}"),
        },
        facts: facts(),
        member_tag: Some(format!("tag-{token}")),
    }
}

fn seed_member(state: &mut super::TileLoop, hwnd: u64, token: &str, pid: u32, creation: &str) {
    let key = crate::native_guards::WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
    };
    state.member_tokens.insert(key.clone(), token.to_owned());
    state
        .member_tags
        .insert(key.clone(), format!("tag-{token}"));
    state.member_identity.insert(
        key,
        crate::model::ProcessIdentity {
            pid,
            process_creation: creation.to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
}

fn live_settings(
    switch_target: &str,
    fresh_placement: &str,
    focus_follows_cursor: bool,
    cursor_warp: bool,
) -> crate::settings::LiveSettings {
    let mut settings = crate::settings::Settings::default();
    settings.core.switch_target = switch_target.to_owned();
    settings.core.fresh_placement = fresh_placement.to_owned();
    settings.core.focus_follows_cursor = focus_follows_cursor;
    settings.core.cursor_warp = cursor_warp;
    crate::settings::LiveSettings::fresh(settings, None)
}

fn route_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 1,
        process_creation: "owner-1".to_owned(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: "C:\\Owner\\owner.exe".to_owned(),
    }
}

fn route_dirs(name: &str) -> (std::path::PathBuf, crate::storage::LedgerStore) {
    let dir = std::env::temp_dir().join(format!(
        "omnitiler-cursor-targeting-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = crate::storage::LedgerStore::open(&dir).expect("ledger temp dir");
    (dir, store)
}

fn view_of(reply: &tiler_core::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

/// Seed win-a on DEV-A (foreground) plus win-b1/win-b2 on DEV-B through
/// one accepted observation tick. Bootstrap adopts native affinity, so the
/// homing is A/B regardless of the cursor.
fn seeded_disagreement(
    name: &str,
    switch_target: &str,
    fresh_placement: &str,
    cursor: Option<(i32, i32)>,
) -> (
    super::TileLoop,
    std::path::PathBuf,
    crate::storage::LedgerStore,
    Vec<ObservedWindow>,
) {
    let mut state = test_state();
    state.ensure_device("DEV-A");
    state.ensure_device("DEV-B");
    state.settings_live = Some(live_settings(switch_target, fresh_placement, false, false));
    seed_member(&mut state, 101, "win-a", 201, "proc-a");
    seed_member(&mut state, 102, "win-b1", 202, "proc-b1");
    seed_member(&mut state, 103, "win-b2", 203, "proc-b2");
    let observed = vec![
        observed_window(101, "win-a", 201, "proc-a", 0),
        observed_window(102, "win-b1", 202, "proc-b1", 800),
        observed_window(103, "win-b2", 203, "proc-b2", 1200),
    ];
    let (dir, store) = route_dirs(&format!("{name}-{switch_target}-{fresh_placement}"));
    let me = route_me();
    let reply = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas(),
        101,
        &fulls(),
        cursor,
        false,
    )
    .expect("seed converges");
    assert!(reply.ready);
    (state, dir, store, observed)
}

#[test]
fn normalize_cursor_drops_out_of_bounds_reads() {
    use super::observation::normalize_cursor;
    use tiler_core::bounds::GEOMETRY_BOUND;
    assert_eq!(normalize_cursor(None), None);
    assert_eq!(normalize_cursor(Some((0, 0))), Some((0, 0)));
    assert_eq!(normalize_cursor(Some((100, 300))), Some((100, 300)));
    assert_eq!(
        normalize_cursor(Some((GEOMETRY_BOUND, GEOMETRY_BOUND))),
        Some((GEOMETRY_BOUND, GEOMETRY_BOUND))
    );
    assert_eq!(
        normalize_cursor(Some((-GEOMETRY_BOUND, -GEOMETRY_BOUND))),
        Some((-GEOMETRY_BOUND, -GEOMETRY_BOUND))
    );
    // Outside the carried geometry bounds: fail-closed to no cursor, never
    // an invented position and never a report refusal.
    for raw in [
        (GEOMETRY_BOUND + 1, 0),
        (0, GEOMETRY_BOUND + 1),
        (-GEOMETRY_BOUND - 1, 0),
        (0, -GEOMETRY_BOUND - 1),
        (i32::MAX, 0),
        (0, i32::MIN),
    ] {
        assert_eq!(normalize_cursor(Some(raw)), None, "{raw:?}");
    }
}

#[test]
fn capture_projects_cursor_targeting_fields() {
    use tiler_core::cursor_targeting::{FreshPlacement, SwitchTarget};
    let mut state = test_state();
    state.ensure_device("DEV-A");
    state.ensure_device("DEV-B");
    state.settings_live = Some(live_settings(
        "focused-output",
        "focused-output-last-focused",
        true,
        true,
    ));
    let observed = vec![observed_window(101, "win-a", 201, "proc-a", 0)];
    let mut inputs = state.inputs();
    inputs.cursor = Some((100, 300));
    let captured = capture_world(&inputs, Some(&observed), &[], &areas(), 101);
    assert_eq!(captured.switch_target, SwitchTarget::Focused);
    assert_eq!(captured.fresh_placement, FreshPlacement::FocusedOutputMru);
    assert!(captured.focus_follows_cursor);
    assert!(captured.cursor_warp);
    assert_eq!(captured.cursor, Some((100, 300)));
    // No live settings (proof owners): defaults plus no cursor.
    let mut plain = test_state();
    plain.ensure_device("DEV-A");
    plain.ensure_device("DEV-B");
    let inputs = plain.inputs();
    let captured = capture_world(&inputs, Some(&observed), &[], &areas(), 101);
    assert_eq!(captured.switch_target, SwitchTarget::Cursor);
    assert_eq!(captured.fresh_placement, FreshPlacement::CursorOutputMru);
    assert!(!captured.focus_follows_cursor);
    assert!(!captured.cursor_warp);
    assert_eq!(captured.cursor, None);
}

#[test]
fn sequencer_envelopes_cursor_targeting_fields() {
    use tiler_core::cursor_targeting::{FreshPlacement, SwitchTarget};
    let mut state = test_state();
    state.ensure_device("DEV-A");
    state.ensure_device("DEV-B");
    state.settings_live = Some(live_settings(
        "focused-output",
        "cursor-output-cursor-closest",
        true,
        false,
    ));
    let observed = vec![observed_window(101, "win-a", 201, "proc-a", 0)];
    let mut inputs = state.inputs();
    inputs.cursor = Some((900, 100));
    let captured = capture_world(&inputs, Some(&observed), &[], &areas(), 101);
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let report = seq.assemble(&captured, 7, None, None, 0);
    assert_eq!(report.switch_target, SwitchTarget::Focused);
    assert_eq!(
        report.fresh_placement,
        FreshPlacement::CursorOutputCursorClosest
    );
    assert!(report.focus_follows_cursor);
    assert!(!report.cursor_warp);
    assert_eq!(report.cursor, Some((900, 100)));
}

#[test]
fn production_select_resolves_cursor_output_under_disagreement() {
    use tiler_core::world_report::WorldCommand;
    // Focus on DEV-A, cursor on DEV-B: the trailing selection lands on
    // DEV-B (cursor policy), never the focused output.
    let (mut state, dir, store, observed) = seeded_disagreement(
        "cursor-disagreement",
        "cursor-output",
        "cursor-output-last-focused",
        Some((1000, 300)),
    );
    let me = route_me();
    let wire_a = state.output_token("DEV-A");
    let wire_b = state.output_token("DEV-B");
    let before_a = state.world.engine().logical_view(&wire_a).expect("view a");
    let before_b = state.world.engine().logical_view(&wire_b).expect("view b");
    let reply = super::world_tick::run_world_command(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas(),
        101,
        &fulls(),
        WorldCommand::SelectTrailing { output: None },
        Some((1000, 300)),
    )
    .expect("select converges");
    assert_eq!(view_of(&reply, &wire_a).as_deref(), Some(before_a.as_str()));
    let after_b = view_of(&reply, &wire_b).expect("view b");
    assert_ne!(after_b, before_b, "trailing lands on the cursor output");
    assert!(
        reply.transfers.iter().any(|t| matches!(
            &t.kind,
            tiler_core::world_report::TransferKind::View
        ) && t.target_output.0 == wire_b),
        "view flight targets the cursor output: {:?}",
        reply.transfers
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn production_select_resolves_focused_output_when_configured() {
    use tiler_core::world_report::WorldCommand;
    // Focused switch target: the cursor on DEV-B is ignored, the trailing
    // selection lands on DEV-A (focused output).
    let (mut state, dir, store, observed) = seeded_disagreement(
        "focused-disagreement",
        "focused-output",
        "cursor-output-last-focused",
        Some((100, 300)),
    );
    let me = route_me();
    let wire_a = state.output_token("DEV-A");
    let wire_b = state.output_token("DEV-B");
    let before_a = state.world.engine().logical_view(&wire_a).expect("view a");
    let before_b = state.world.engine().logical_view(&wire_b).expect("view b");
    let reply = super::world_tick::run_world_command(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas(),
        101,
        &fulls(),
        WorldCommand::SelectTrailing { output: None },
        Some((1000, 300)),
    )
    .expect("select converges");
    assert_eq!(view_of(&reply, &wire_b).as_deref(), Some(before_b.as_str()));
    let after_a = view_of(&reply, &wire_a).expect("view a");
    assert_ne!(after_a, before_a, "trailing lands on the focused output");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn production_sends_use_focused_source_output_despite_cursor() {
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::world_report::{TransferKind, WorldCommand};
    // Cursor on DEV-B throughout: the focused mover win-a lives on DEV-A,
    // so numbered and relative sends resolve on the mover's output.
    let (mut state, dir, store, observed) = seeded_disagreement(
        "sends-disagreement",
        "cursor-output",
        "cursor-output-last-focused",
        Some((1000, 300)),
    );
    let me = route_me();
    let wire_a = state.output_token("DEV-A");
    let reply = super::world_tick::run_world_command(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas(),
        101,
        &fulls(),
        WorldCommand::SendNumbered {
            window: WindowId("win-a".to_owned()),
            source: tiler_core::world_report::SendSource::Explicit(OutputId(wire_a.clone())),
            index: 0,
            follow: false,
        },
        Some((1000, 300)),
    )
    .expect("send converges");
    assert!(
        reply.transfers.iter().any(
            |t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")
                && t.target_output.0 == wire_a
        ),
        "numbered send resolves on the mover output: {:?}",
        reply.transfers
    );
    let wire_b = state.output_token("DEV-B");
    // Give DEV-B a second workspace so the relative step has somewhere to
    // go (a single-workspace ring wraps to itself and refuses as unchanged).
    super::world_tick::run_world_command(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas(),
        101,
        &fulls(),
        WorldCommand::SelectTrailing { output: None },
        Some((1000, 300)),
    )
    .expect("trailing select converges");
    let reply = super::world_tick::run_world_command(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas(),
        // The mover is the focused window (production origin contract).
        102,
        &fulls(),
        WorldCommand::SendRelative {
            window: WindowId("win-b1".to_owned()),
            source: tiler_core::world_report::SendSource::Explicit(OutputId(wire_b.clone())),
            delta: 1,
            follow: false,
        },
        Some((100, 300)),
    );
    let reply = reply.expect("relative send converges");
    assert!(
        reply
            .transfers
            .iter()
            .filter(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-b1"))
            .all(|t| t.target_output.0 == wire_b),
        "relative send never leaves the mover output: {:?}",
        reply.transfers
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn explicit_output_select_stays_screen_specific() {
    use tiler_core::directional::OutputId;
    use tiler_core::world_report::WorldCommand;
    // Explicit outputs name a genuinely semantic target (pager click):
    // the cursor on DEV-A never diverts a DEV-B selection.
    let (mut state, dir, store, observed) = seeded_disagreement(
        "explicit-screen",
        "cursor-output",
        "cursor-output-last-focused",
        Some((100, 300)),
    );
    let me = route_me();
    let wire_a = state.output_token("DEV-A");
    let wire_b = state.output_token("DEV-B");
    let before_a = state.world.engine().logical_view(&wire_a).expect("view a");
    let before_b = state.world.engine().logical_view(&wire_b).expect("view b");
    let reply = super::world_tick::run_world_command(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas(),
        101,
        &fulls(),
        WorldCommand::SelectTrailing {
            output: Some(OutputId(wire_b.clone())),
        },
        Some((100, 300)),
    )
    .expect("explicit select converges");
    assert_eq!(view_of(&reply, &wire_a).as_deref(), Some(before_a.as_str()));
    assert_ne!(
        view_of(&reply, &wire_b).as_deref(),
        Some(before_b.as_str()),
        "explicit output keeps its screen"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gaming_lane_reports_no_cursor() {
    // The focused-fullscreen-game lane (`reconcile_limited_tick`) runs the
    // same observation tick with no cursor sampling: a previously known
    // cursor reads back as unknown instead of steering policy under the game.
    let (mut state, dir, store, observed) = seeded_disagreement(
        "gaming-guard",
        "cursor-output",
        "cursor-output-last-focused",
        Some((1000, 300)),
    );
    assert_eq!(state.world.engine().last_cursor(), Some((1000, 300)));
    let me = route_me();
    let reply = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas(),
        101,
        &fulls(),
        None,
        true,
    )
    .expect("limited tick converges");
    assert_eq!(state.world.engine().last_cursor(), None);
    // The paused game lane skips presentation work: hidden shapes and an
    // empty pager, never a projection under the game.
    assert!(reply.presentation.pager.is_empty());
    assert_eq!(
        reply.presentation.border.reason,
        Some(tiler_core::visual::BorderHideReason::Suspended)
    );
    assert!(matches!(
        reply.presentation.underlay,
        tiler_core::world_presentation::PresentationUnderlay::Hidden { .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
