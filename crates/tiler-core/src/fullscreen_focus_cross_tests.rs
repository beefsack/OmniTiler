#![forbid(unsafe_code)]
//! R-MAX-02: while the focused window is fullscreen, directional focus
//! skips all same-output siblings and continues to the adjacent output in
//! that direction; without one, focus stays. Explicit focus-by-window,
//! workspace switches and directional move/send are unaffected.
//!
//! Failing-first regressions: fullscreen tiled/floating subjects must not
//! take same-output siblings (`WorldLayout::local_focus` refuses `Unchanged`
//! in every direction; `run_directional_pair_focus` skips the local-exhaustion
//! gate; the shared `WorldCommand` entry skips the float lattice and marks
//! the cross command float-origin when the subject floats).

use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{Direction, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ObservedWindow, OutputDomain, RefusalKind};
use crate::settings_catalog::{FIELD_INNER_GAP, FIELD_OUTER_GAP};
use crate::settings_state::EffectiveSetting;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_layout::{LayoutFailure, PairOperationContext, PairOperationFailure, WorldLayout};
use crate::world_report::{
    PresentationIngressV4, PresentationRowV4, ScreenReport, WORLD_CONTRACT_VERSION_V4, WindowFact,
    WorldCommand, WorldReportV4,
};
use std::collections::BTreeMap;

// ---- WorldLayout fixtures (mirrors maximized_focus_send_tests.rs). ----

fn dom(o: &str, w: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(o.to_owned()),
        workspace: WorkspaceId(w.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn dom_adjacent(o: &str, w: &str, dir: Direction, target: &str) -> OutputDomain {
    let mut d = dom(o, w);
    d.adjacent.insert(dir, OutputId(target.to_owned()));
    d
}
fn tiled(w: &str, o: &str, ws: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}
fn engine_row(w: &str, o: &str, ws: &str, rect: Rect) -> EngineWindow {
    EngineWindow {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect,
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}
fn fullscreen_flag(base: &ObservedWindow) -> ObservedWindow {
    let mut entry = base.clone();
    entry.fullscreen = true;
    entry
}
fn fullscreen_maximized_flag(base: &ObservedWindow) -> ObservedWindow {
    let mut entry = base.clone();
    entry.fullscreen = true;
    entry.maximized = true;
    entry
}
fn obs_for(
    layout: &WorldLayout,
    domain: &DomainKey,
    overrides: &BTreeMap<String, ObservedWindow>,
) -> Vec<ObservedWindow> {
    let mut wins: Vec<ObservedWindow> = layout
        .tiled_windows(domain)
        .map(|w| {
            if let Some(entry) = overrides.get(&w.0) {
                entry.clone()
            } else {
                tiled(&w.0, &domain.output.0, &domain.workspace.0)
            }
        })
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(domain) {
        let mut entry = tiled(&w.0, &domain.output.0, &domain.workspace.0);
        entry.floating = flags.floating;
        entry.fullscreen = flags.fullscreen;
        entry.maximized = flags.maximized;
        entry.sticky = flags.sticky;
        entry.fixed_auto = layout.is_automatic_fixed_float(domain, &w);
        wins.push(entry);
    }
    wins
}
fn engine_obs_for_pair(
    layout: &WorldLayout,
    k1: &DomainKey,
    k2: &DomainKey,
    overrides: &BTreeMap<String, ObservedWindow>,
) -> Vec<EngineWindow> {
    let mut wins = Vec::new();
    for k in [k1, k2] {
        for o in obs_for(layout, k, overrides) {
            let mut row = engine_row(
                &o.window.0,
                &o.output.0,
                &o.workspace.0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
            );
            row.floating = o.floating;
            row.fullscreen = o.fullscreen;
            row.maximized = o.maximized;
            row.sticky = o.sticky;
            row.fixed_auto = o.fixed_auto;
            wins.push(row);
        }
    }
    wins
}
fn admit(layout: &mut WorldLayout, d: &OutputDomain, w: &str) {
    let k = d.key();
    let win_id = WindowId(w.to_owned());
    let placement = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    };
    if !layout.contains_domain(&k) {
        layout
            .reconcile_fresh(
                d,
                0,
                &win_id,
                &[engine_row(w, &d.id.0, &d.workspace.0, placement)],
            )
            .expect("fresh admit");
        return;
    }
    let mut o0 = obs_for(layout, &k, &BTreeMap::new());
    o0.push(tiled(w, &d.id.0, &d.workspace.0));
    layout
        .admit_window(
            &k,
            &win_id,
            crate::session::ExceptionFlags::none(),
            None,
            placement,
            false,
            false,
            &o0,
        )
        .expect("admit");
}
fn focused_window(layout: &WorldLayout, domain: &DomainKey) -> WindowId {
    let l = layout.focus_leaf(domain).expect("focused leaf");
    layout.window_for_leaf(domain, l).expect("focused link")
}

/// Two tiles on one output, focused on win-2 (win-1 left of win-2).
/// Fixed-size admission is on so the fullscreen overlay observation binds
/// (same lane as the maximized-fence fixtures).
fn two_tile() -> (WorldLayout, OutputDomain, DomainKey) {
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    let d = dom("out-1", "ws-1");
    admit(&mut layout, &d, "win-1");
    admit(&mut layout, &d, "win-2");
    let domain = d.key();
    assert_eq!(focused_window(&layout, &domain).0, "win-2");
    (layout, d, domain)
}

/// Two adjacent outputs. Fixed-size admission is on for the fullscreen
/// overlay binding (see `two_tile`).
fn two_output() -> (WorldLayout, OutputDomain, OutputDomain) {
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    let o1 = dom_adjacent("out-1", "ws-1", Direction::Right, "out-2");
    let o2 = dom_adjacent("out-2", "ws-1", Direction::Left, "out-1");
    (layout, o1, o2)
}

#[test]
fn fullscreen_tiled_subject_propose_focus_refuses_unchanged_all_directions() {
    let (mut layout, _, domain) = two_tile();
    let focused = focused_window(&layout, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        fullscreen_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs_for(&layout, &domain, &flags);
    // A local sibling waits in Left; Up/Down have no local target either.
    // Every direction must refuse as Unchanged (no plan, no mutation) so the
    // caller continues to the adjacent output or stays.
    let rev_before = layout.accepted_revision(&domain);
    for direction in [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ] {
        let err = layout
            .local_focus(&domain, &focused, direction, &focused, &o0)
            .expect_err("fullscreen focus must skip same-output siblings");
        assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
        assert_eq!(layout.accepted_revision(&domain), rev_before);
    }
}

#[test]
fn non_fullscreen_control_still_plans_local_sibling() {
    let (mut layout, _, domain) = two_tile();
    let focused = focused_window(&layout, &domain);
    let o0 = obs_for(&layout, &domain, &BTreeMap::new());
    layout
        .local_focus(&domain, &focused, Direction::Left, &focused, &o0)
        .expect("ordinary focus still plans the local sibling");
}

fn maximized_flag(base: &ObservedWindow) -> ObservedWindow {
    let mut entry = base.clone();
    entry.maximized = true;
    entry
}

#[test]
fn maximized_tiled_subject_propose_focus_refuses_unchanged_all_directions() {
    // Nov 1 rule (2026-11-01): the maximized origin skips same-output
    // siblings in every direction with no fence profile.
    let (mut layout, _, domain) = two_tile();
    let focused = focused_window(&layout, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        maximized_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = obs_for(&layout, &domain, &flags);
    let rev_before = layout.accepted_revision(&domain);
    for direction in [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ] {
        let err = layout
            .local_focus(&domain, &focused, direction, &focused, &o0)
            .expect_err("maximized focus must skip same-output siblings");
        assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
        assert_eq!(layout.accepted_revision(&domain), rev_before);
    }
}

#[test]
fn maximized_tiled_subject_crosses_to_adjacent_output() {
    // Nov 1 rule: like the fullscreen subject above, the maximized subject
    // skips the local-exhaustion gate and crosses despite the sibling.
    for (dir, opp) in [
        (Direction::Right, Direction::Left),
        (Direction::Up, Direction::Down),
        (Direction::Down, Direction::Up),
    ] {
        let mut out_2 = dom_adjacent("out-2", "ws-1", opp, "out-1");
        out_2.bounds.y = match dir {
            Direction::Up => -600,
            Direction::Down => 600,
            _ => 0,
        };
        let out_1 = dom_adjacent("out-1", "ws-1", dir, "out-2");
        let mut layout = WorldLayout::new();
        layout.set_fixed_size_admission(true);
        admit(&mut layout, &out_2, "win-t");
        admit(&mut layout, &out_1, "win-a");
        admit(&mut layout, &out_1, "win-b");
        let source = out_1.key();
        let target = out_2.key();
        assert_eq!(focused_window(&layout, &source).0, "win-b");
        let plain = obs_for(&layout, &source, &BTreeMap::new());
        layout
            .local_focus(
                &source,
                &WindowId("win-b".to_owned()),
                Direction::Left,
                &WindowId("win-b".to_owned()),
                &plain,
            )
            .expect("seed focus plans");
        assert_eq!(focused_window(&layout, &source).0, "win-a");
        // Maximized win-a pressing dir must skip win-b and cross to win-t.
        let mut flags = BTreeMap::new();
        flags.insert(
            "win-a".to_owned(),
            maximized_flag(&tiled("win-a", "out-1", "ws-1")),
        );
        let o1 = engine_obs_for_pair(&layout, &source, &target, &flags);
        let subject = WindowId("win-a".to_owned());
        let ctx = PairOperationContext {
            source_domain: &out_1,
            target_domain: &out_2,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &subject,
            observed: &o1,
            closed: &[],
        };
        let outcome = layout
            .run_directional_pair_focus(&ctx, &subject, dir, false)
            .expect("maximized subject crosses despite the local sibling");
        assert!(outcome.crossed);
        assert_eq!(outcome.target_output, OutputId("out-2".to_owned()));
        assert_eq!(outcome.target_workspace, WorkspaceId("ws-1".to_owned()));
    }
}

#[test]
fn fullscreen_tiled_subject_crosses_to_adjacent_output() {
    let (mut layout, out_1, out_2) = two_output();
    admit(&mut layout, &out_2, "win-t");
    admit(&mut layout, &out_1, "win-a");
    admit(&mut layout, &out_1, "win-b");
    let source = out_1.key();
    let target = out_2.key();
    assert_eq!(focused_window(&layout, &source).0, "win-b");
    // Move logical focus onto win-a so a same-output sibling (win-b) waits
    // in the press direction.
    let plain = obs_for(&layout, &source, &BTreeMap::new());
    layout
        .local_focus(
            &source,
            &WindowId("win-b".to_owned()),
            Direction::Left,
            &WindowId("win-b".to_owned()),
            &plain,
        )
        .expect("seed focus plans");
    assert_eq!(focused_window(&layout, &source).0, "win-a");
    // Fullscreen win-a pressing Right must skip win-b and cross to win-t.
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-a".to_owned(),
        fullscreen_flag(&tiled("win-a", "out-1", "ws-1")),
    );
    let o1 = engine_obs_for_pair(&layout, &source, &target, &flags);
    let subject = WindowId("win-a".to_owned());
    let ctx = PairOperationContext {
        source_domain: &out_1,
        target_domain: &out_2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &o1,
        closed: &[],
    };
    let outcome = layout
        .run_directional_pair_focus(&ctx, &subject, Direction::Right, false)
        .expect("fullscreen subject crosses despite the local sibling");
    assert!(outcome.crossed);
    assert_eq!(outcome.target_output, OutputId("out-2".to_owned()));
    assert_eq!(outcome.target_workspace, WorkspaceId("ws-1".to_owned()));
    assert_eq!(outcome.to_window.0, "win-t");
}

#[test]
fn fullscreen_single_output_cross_refuses_without_pending() {
    let (mut layout, _, domain) = two_tile();
    // Focus win-1 so the press direction holds a sibling; there is still no
    // adjacent output to continue to.
    let plain = obs_for(&layout, &domain, &BTreeMap::new());
    let focused = focused_window(&layout, &domain);
    layout
        .local_focus(&domain, &focused, Direction::Left, &focused, &plain)
        .expect("seed focus plans");
    assert_eq!(focused_window(&layout, &domain).0, "win-1");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-1".to_owned(),
        fullscreen_flag(&tiled("win-1", "out-1", "ws-1")),
    );
    let o1 = obs_for(&layout, &domain, &flags);
    let rev_before = layout.accepted_revision(&domain);
    let err = layout
        .local_focus(
            &domain,
            &WindowId("win-1".to_owned()),
            Direction::Right,
            &WindowId("win-1".to_owned()),
            &o1,
        )
        .expect_err("no adjacent output must retain");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
    assert_eq!(layout.accepted_revision(&domain), rev_before);
}

#[test]
fn vertical_fullscreen_cross_refuses() {
    let (mut layout, out_1, out_2) = two_output();
    admit(&mut layout, &out_1, "win-1");
    admit(&mut layout, &out_1, "win-2");
    admit(&mut layout, &out_2, "win-3");
    let domain = out_1.key();
    let target = out_2.key();
    let focused = focused_window(&layout, &domain);
    let mut flags = BTreeMap::new();
    flags.insert(
        focused.0.clone(),
        fullscreen_flag(&tiled(&focused.0, "out-1", "ws-1")),
    );
    let o0 = engine_obs_for_pair(&layout, &domain, &target, &flags);
    // Left/Right-only crossings stay: Up/Down fullscreen is a no-op.
    for direction in [Direction::Up, Direction::Down] {
        let ctx = PairOperationContext {
            source_domain: &out_1,
            target_domain: &out_2,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &focused,
            observed: &o0,
            closed: &[],
        };
        let err = layout
            .run_directional_pair_focus(&ctx, &focused, direction, false)
            .expect_err("vertical fullscreen must not cross");
        assert_eq!(
            err,
            PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged))
        );
    }
}

#[test]
fn both_flags_follow_fullscreen_local_skip_and_cross() {
    // Fullscreen (even with maximize also set) follows the R-MAX-02
    // skip-local and cross rule.
    let (mut layout, out_1, out_2) = two_output();
    admit(&mut layout, &out_2, "win-t");
    admit(&mut layout, &out_1, "win-a");
    admit(&mut layout, &out_1, "win-b");
    let source = out_1.key();
    let target = out_2.key();
    let plain = obs_for(&layout, &source, &BTreeMap::new());
    layout
        .local_focus(
            &source,
            &WindowId("win-b".to_owned()),
            Direction::Left,
            &WindowId("win-b".to_owned()),
            &plain,
        )
        .expect("seed focus plans");
    let mut flags = BTreeMap::new();
    flags.insert(
        "win-a".to_owned(),
        fullscreen_maximized_flag(&tiled("win-a", "out-1", "ws-1")),
    );
    let o_local = obs_for(&layout, &source, &flags);
    // Local focus refuses Unchanged (fullscreen rule, all directions)...
    let err = layout
        .local_focus(
            &source,
            &WindowId("win-a".to_owned()),
            Direction::Right,
            &WindowId("win-a".to_owned()),
            &o_local,
        )
        .expect_err("both flags must not take the local sibling");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
    // ...while the cross leg proceeds.
    let o_pair = engine_obs_for_pair(&layout, &source, &target, &flags);
    let subject = WindowId("win-a".to_owned());
    let ctx = PairOperationContext {
        source_domain: &out_1,
        target_domain: &out_2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &o_pair,
        closed: &[],
    };
    let outcome = layout
        .run_directional_pair_focus(&ctx, &subject, Direction::Right, false)
        .expect("both flags cross like fullscreen");
    assert!(outcome.crossed);
    assert_eq!(outcome.target_output, OutputId("out-2".to_owned()));
    assert_eq!(outcome.target_workspace, WorkspaceId("ws-1".to_owned()));
}

// ---- Raw WorldLayout: local fullscreen focus refuses. ----

fn engine_window(id: &str, x: i32, fullscreen: bool) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: Rect {
            x,
            y: 0,
            w: 100,
            h: 100,
        },
        floating: false,
        fit_excluded: false,
        fullscreen,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn seed_layout() -> (WorldLayout, OutputDomain) {
    let mut layout = WorldLayout::new();
    // Fixed-size admission so the fullscreen overlay observation binds
    // (same lane as the maximized-fence engine fixtures).
    layout.set_fixed_size_admission(true);
    let d = dom("out-1", "ws-1");
    layout
        .reconcile_fresh(
            &d,
            0,
            &WindowId("win-2".to_owned()),
            &[
                engine_window("win-1", 0, false),
                engine_window("win-2", 200, false),
            ],
        )
        .expect("seed must project");
    (layout, d)
}

#[test]
fn engine_fullscreen_local_focus_refuses_unchanged() {
    let (mut layout, d) = seed_layout();
    let domain = d.key();
    let obs = vec![
        tiled("win-1", "out-1", "ws-1"),
        fullscreen_flag(&tiled("win-2", "out-1", "ws-1")),
    ];
    let err = layout
        .local_focus(
            &domain,
            &WindowId("win-2".to_owned()),
            Direction::Left,
            &WindowId("win-2".to_owned()),
            &obs,
        )
        .expect_err("fullscreen focus must not plan locally");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
}

// ---- Shared WorldCommand entry (WorldEngine). ----

fn wrect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(wrect(x, 0, 800, 600)),
        work_area: Some(wrect(x, 0, 800, 560)),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(wrect(x, 0, 40, 90)),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: crate::world_report::NativeWorkspaceFact::Omitted,
        host_hidden: Some(false),
        privileged: Some(false),
        managed: Some(true),
        accepts_input: Some(true),
        activation_inhibited: Some(false),
        tool_window: Some(false),
        owned: Some(false),
        transient: Some(false),
        modal: Some(false),
        resizable: Some(true),
        above: Some(false),
        below: Some(false),
        owner: None,
        parent: None,
        role: None,
        app_id: None,
        executable: Some("synthetic.exe".to_owned()),
        process_id: Some("foreign-pid".to_owned()),
        class: Some("Synthetic".to_owned()),
        title: None,
        window_type: Some(crate::world_report::WindowType::Normal),
        activation_policy: Some(crate::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: crate::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

fn fullscreen_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        fullscreen: Some(true),
        ..fact(window, output, x)
    }
}

fn float_fact(window: &str, output: &str, x: i32, y: i32) -> WindowFact {
    WindowFact {
        rect: Some(wrect(x, y, 200, 200)),
        transient: Some(true),
        ..fact(window, output, x)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        screens,
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> crate::world_report::WorldReplyV4 {
    accepted_with_settlements(engine, report).0
}

fn accepted_with_settlements(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
) -> (
    crate::world_report::WorldReplyV4,
    Vec<crate::world_report::BatchSettlement>,
) {
    match engine.handle_report(report, &[]) {
        (WorldOutcome::Accepted(reply), settlements) => (reply, settlements),
        (other, _) => panic!("expected accepted, got {other:?}"),
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn screens_in(dir: &str) -> Vec<ScreenReport> {
    let mut out_b = screen("out-b", 800);
    let (x, y) = match dir {
        "right" => (800, 0),
        "up" => (0, -600),
        "down" => (0, 600),
        other => panic!("unexpected direction {other}"),
    };
    out_b.full_rect = Some(wrect(x, y, 800, 600));
    out_b.work_area = Some(wrect(x, y, 800, 560));
    vec![screen("out-a", 0), out_b]
}

fn target_xy(dir: &str) -> (i32, i32) {
    match dir {
        "right" => (900, 0),
        "up" => (100, -500),
        "down" => (100, 700),
        other => panic!("unexpected direction {other}"),
    }
}

fn fact_at(window: &str, output: &str, x: i32, y: i32) -> WindowFact {
    WindowFact {
        rect: Some(wrect(x, y, 40, 90)),
        ..fact(window, output, x)
    }
}

fn one_screen() -> Vec<ScreenReport> {
    vec![screen("out-a", 0)]
}

fn expect_explicit_focus(
    reply: &crate::world_report::WorldReplyV4,
    target: &str,
    cmd_seq: u64,
    rev: u64,
) {
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {:?}",
        reply.effects
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target, WindowId(target.to_owned()));
    assert_eq!(request.id, format!("focus-{rev}"));
    assert_eq!(request.issued_revision, rev);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(cmd_seq));
    assert_eq!(request.after_shown, None);
}

fn home_of(engine: &WorldEngine, window: &WindowId) -> Option<DomainKey> {
    engine
        .world
        .layout
        .domain_keys()
        .into_iter()
        .find(|key| engine.world.layout.is_tiled_window(key, window))
}

fn session_focused(engine: &WorldEngine, key: &DomainKey) -> Option<WindowId> {
    let leaf = engine.world.layout.focus_leaf(key)?;
    engine.world.layout.window_for_leaf(key, leaf)
}

#[test]
fn world_fullscreen_tiled_skips_sibling_and_crosses_output() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            None,
            None,
        ),
    );
    // Fullscreen win-a pressing Right must skip win-b and cross to win-c.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fullscreen_fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-a".to_owned()),
                direction: crate::directional::Direction::Right,
            }),
        ),
    );
    expect_explicit_focus(&reply, "win-c", 11, 2);
}

#[test]
fn world_non_fullscreen_control_plans_local_sibling() {
    // Genuine original geometry: inner gap 8 with outer gap 0 (the
    // pre-migration screen default), via the real wire keys `gap`/`outer`.
    // The effective-value guards below fail if the keys ever silently
    // default instead of applying.
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the lane
            // under test is the native route.
            native_sticky: true,
            maximize: true,
            ..HostCapabilities::default()
        },
        vec![
            ("gap".to_owned(), RawSettingValue::Number(8.0)),
            ("outer".to_owned(), RawSettingValue::Number(0.0)),
        ],
    )
    .expect("generation");
    assert_eq!(
        engine.settings().requested[FIELD_INNER_GAP],
        EffectiveSetting::Int(8)
    );
    assert_eq!(
        engine.settings().requested[FIELD_OUTER_GAP],
        EffectiveSetting::Int(0)
    );
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-a".to_owned()),
                direction: crate::directional::Direction::Right,
            }),
        ),
    );
    expect_explicit_focus(&reply, "win-b", 11, 2);
}

#[test]
fn world_fullscreen_single_output_retains_without_effect() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            one_screen(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            None,
            None,
        ),
    );
    let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let outcome = engine
        .handle_report(
            &report(
                2,
                one_screen(),
                vec![
                    fullscreen_fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 200),
                ],
                "win-a",
                Some(11),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-a".to_owned()),
                    direction: crate::directional::Direction::Right,
                }),
            ),
            &[],
        )
        .0;
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(
                kind, "unchanged",
                "single-output fullscreen refuses as unchanged, not {kind}"
            );
            assert_eq!(
                session_focused(&engine, &key),
                Some(WindowId("win-a".to_owned())),
                "single-output fullscreen retains focus"
            );
            return;
        }
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert!(
        reply.focus_request.is_none(),
        "no focus request without an adjacent output, got {:?}",
        reply.focus_request
    );
    assert_eq!(
        session_focused(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "single-output fullscreen retains focus"
    );
}

#[test]
fn world_fullscreen_vertical_is_noop() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            None,
            None,
        ),
    );
    let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let outcome = engine
        .handle_report(
            &report(
                2,
                two_screens(),
                vec![
                    fullscreen_fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 200),
                    fact("win-c", "out-b", 900),
                ],
                "win-a",
                Some(11),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-a".to_owned()),
                    direction: crate::directional::Direction::Up,
                }),
            ),
            &[],
        )
        .0;
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(
                kind, "unchanged",
                "vertical fullscreen refuses as unchanged, not {kind}"
            );
            assert_eq!(
                session_focused(&engine, &key),
                Some(WindowId("win-a".to_owned())),
                "vertical fullscreen retains focus"
            );
            return;
        }
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert!(
        reply.focus_request.is_none(),
        "vertical fullscreen never crosses, got {:?}",
        reply.focus_request
    );
    assert_eq!(
        session_focused(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "vertical fullscreen retains focus"
    );
}

#[test]
fn world_fullscreen_float_skips_lattice_and_crosses_remembered() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
                fact("win-t", "out-b", 900),
            ],
            "win-sub",
            None,
            None,
        ),
    );
    // Establish the remembered tiled target on out-b, then return explicit
    // focus to the float subject.
    accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
                fact("win-t", "out-b", 900),
            ],
            "win-t",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-t".to_owned()),
                direction: Some(crate::directional::Direction::Right),
            }),
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
                fact("win-t", "out-b", 900),
            ],
            "win-sub",
            Some(12),
            Some(WorldCommand::Focus {
                window: WindowId("win-sub".to_owned()),
                direction: Some(crate::directional::Direction::Right),
            }),
        ),
    );
    // Fullscreen subject pressing Right must skip win-east and cross to the
    // remembered win-t.
    let reply = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                WindowFact {
                    fullscreen: Some(true),
                    ..float_fact("win-sub", "out-a", 100, 100)
                },
                float_fact("win-east", "out-a", 300, 100),
                fact("win-t", "out-b", 900),
            ],
            "win-sub",
            Some(13),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-sub".to_owned()),
                direction: crate::directional::Direction::Right,
            }),
        ),
    );
    expect_explicit_focus(&reply, "win-t", 13, 4);
}

#[test]
fn world_fullscreen_float_vertical_retains() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            one_screen(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-above", "out-a", 100, 0),
            ],
            "win-sub",
            None,
            None,
        ),
    );
    // A same-output float waits above, but the fullscreen subject pressing
    // Up must retain without dispatch.
    let (reply, settlements) = accepted_with_settlements(
        &mut engine,
        &report(
            2,
            one_screen(),
            vec![
                WindowFact {
                    fullscreen: Some(true),
                    ..float_fact("win-sub", "out-a", 100, 100)
                },
                float_fact("win-above", "out-a", 100, 0),
            ],
            "win-sub",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-sub".to_owned()),
                direction: crate::directional::Direction::Up,
            }),
        ),
    );
    let outcome = settlements
        .first()
        .and_then(|s| s.result.as_ref())
        .map(|result| match result {
            crate::world_report::CommandResult::Directional { outcome, .. } => outcome.as_str(),
            _ => "other",
        });
    assert_eq!(outcome, Some("focus-float-retained"));
    assert!(
        reply.focus_request.is_none(),
        "vertical fullscreen float retains, got {:?}",
        reply.focus_request
    );
}

#[test]
fn world_fullscreen_float_single_output_retains() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            one_screen(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
            ],
            "win-sub",
            None,
            None,
        ),
    );
    // A same-output float waits to the right, but with no adjacent output
    // the fullscreen subject pressing Right must retain without dispatch.
    let (reply, settlements) = accepted_with_settlements(
        &mut engine,
        &report(
            2,
            one_screen(),
            vec![
                WindowFact {
                    fullscreen: Some(true),
                    ..float_fact("win-sub", "out-a", 100, 100)
                },
                float_fact("win-east", "out-a", 300, 100),
            ],
            "win-sub",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-sub".to_owned()),
                direction: crate::directional::Direction::Right,
            }),
        ),
    );
    let outcome = settlements
        .first()
        .and_then(|s| s.result.as_ref())
        .map(|result| match result {
            crate::world_report::CommandResult::Directional { outcome, .. } => outcome.as_str(),
            _ => "other",
        });
    assert_eq!(outcome, Some("focus-float-retained"));
    assert!(
        reply.focus_request.is_none(),
        "single-output fullscreen float retains, got {:?}",
        reply.focus_request
    );
}

#[test]
fn world_explicit_focus_by_window_works_under_fullscreen() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            None,
            None,
        ),
    );
    // Explicit focus-by-window is unaffected by the fullscreen subject.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fullscreen_fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: Some(crate::directional::Direction::Right),
            }),
        ),
    );
    expect_explicit_focus(&reply, "win-b", 11, 2);
}

// ---- Nov 1 rule (2026-11-01), native maximize route: a maximized origin
// skips same-output siblings and continues to the adjacent output in the
// requested direction; without one, focus stays. Destination overlays are
// eligible and preserved. ----

fn maximized_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        maximized: Some(true),
        ..fact(window, output, x)
    }
}

fn maximized_float_fact(window: &str, output: &str, x: i32, y: i32) -> WindowFact {
    WindowFact {
        maximized: Some(true),
        ..float_fact(window, output, x, y)
    }
}

#[test]
fn world_native_maximized_tiled_skips_sibling_and_crosses_output() {
    for dir in ["right", "up", "down"] {
        let screens = screens_in(dir);
        let (tx, ty) = target_xy(dir);
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 200),
                    fact_at("win-c", "out-b", tx, ty),
                ],
                "win-a",
                None,
                None,
            ),
        );
        // Maximized win-a pressing dir must skip win-b and cross to win-c.
        let reply = accepted(
            &mut engine,
            &report(
                2,
                screens,
                vec![
                    maximized_fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 200),
                    fact_at("win-c", "out-b", tx, ty),
                ],
                "win-a",
                Some(11),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-a".to_owned()),
                    direction: crate::directional::Direction::parse_wire(dir).expect("direction"),
                }),
            ),
        );
        expect_explicit_focus(&reply, "win-c", 11, 2);
    }
}

#[test]
fn world_native_maximized_tiled_single_output_stays_unchanged() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            one_screen(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            None,
            None,
        ),
    );
    let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let outcome = engine
        .handle_report(
            &report(
                2,
                one_screen(),
                vec![
                    maximized_fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 200),
                ],
                "win-a",
                Some(11),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-a".to_owned()),
                    direction: crate::directional::Direction::Right,
                }),
            ),
            &[],
        )
        .0;
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(
                kind, "unchanged",
                "single-output maximized refuses as unchanged, not {kind}"
            );
            assert_eq!(
                session_focused(&engine, &key),
                Some(WindowId("win-a".to_owned())),
                "single-output maximized retains focus"
            );
            return;
        }
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert!(
        reply.focus_request.is_none(),
        "no focus request without an adjacent output, got {:?}",
        reply.focus_request
    );
    assert_eq!(
        session_focused(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "single-output maximized retains focus"
    );
}

#[test]
fn world_native_maximized_tiled_no_vertical_neighbor_retains() {
    for dir in ["up", "down"] {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                two_screens(),
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 200),
                    fact("win-c", "out-b", 900),
                ],
                "win-a",
                None,
                None,
            ),
        );
        let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
        let outcome = engine
            .handle_report(
                &report(
                    2,
                    two_screens(),
                    vec![
                        maximized_fact("win-a", "out-a", 0),
                        fact("win-b", "out-a", 200),
                        fact("win-c", "out-b", 900),
                    ],
                    "win-a",
                    Some(11),
                    Some(WorldCommand::FocusDirectional {
                        window: WindowId("win-a".to_owned()),
                        direction: crate::directional::Direction::parse_wire(dir)
                            .expect("direction"),
                    }),
                ),
                &[],
            )
            .0;
        let reply = match outcome {
            WorldOutcome::Accepted(reply) => reply,
            WorldOutcome::Rejected { kind, .. } => {
                assert_eq!(
                    kind, "unchanged",
                    "no-vertical-neighbor maximized refuses as unchanged, not {kind}"
                );
                assert_eq!(
                    session_focused(&engine, &key),
                    Some(WindowId("win-a".to_owned())),
                    "no-vertical-neighbor maximized retains focus"
                );
                continue;
            }
            other => panic!("unexpected outcome: {other:?}"),
        };
        assert!(
            reply.focus_request.is_none(),
            "maximized without vertical neighbor retains, got {:?}",
            reply.focus_request
        );
        assert_eq!(
            session_focused(&engine, &key),
            Some(WindowId("win-a".to_owned())),
            "no-vertical-neighbor maximized retains focus"
        );
    }
}

#[test]
fn world_native_maximized_cross_preserves_destination_overlay() {
    for dir in ["right", "up", "down"] {
        let screens = screens_in(dir);
        let (tx, ty) = target_xy(dir);
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 200),
                    fact_at("win-c", "out-b", tx, ty),
                ],
                "win-a",
                None,
                None,
            ),
        );
        // Both ends maximized: the cross must reach the maximized destination
        // and disturb neither overlay.
        let reply = accepted(
            &mut engine,
            &report(
                2,
                screens,
                vec![
                    maximized_fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 200),
                    WindowFact {
                        maximized: Some(true),
                        ..fact_at("win-c", "out-b", tx, ty)
                    },
                ],
                "win-a",
                Some(11),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-a".to_owned()),
                    direction: crate::directional::Direction::parse_wire(dir).expect("direction"),
                }),
            ),
        );
        expect_explicit_focus(&reply, "win-c", 11, 2);
        let staged_maximized = |lifetime: &str| {
            engine
                .world
                .staged
                .facts
                .get(&WindowId(lifetime.to_owned()))
                .and_then(|fact| fact.maximized)
        };
        assert_eq!(
            staged_maximized("win-a"),
            Some(true),
            "source overlay preserved ({dir})"
        );
        assert_eq!(
            staged_maximized("win-c"),
            Some(true),
            "destination overlay preserved ({dir})"
        );
    }
}

// Float-origin maximized focus below is an explicitly requested ASSUMPTION
// for later reversal: ordinary floats stay local, while a maximized float
// origin skips the lattice and crosses (or retains) like the tiled rule.
// These cases are isolated here so the assumption can be revisited alone.

#[test]
fn world_native_maximized_float_skips_lattice_and_crosses_remembered() {
    for (dir, (sx, sy), (lx, ly)) in [
        ("right", (100, 100), (320, 100)),
        ("up", (100, 220), (100, 0)),
        ("down", (100, 100), (100, 320)),
    ] {
        let screens = screens_in(dir);
        let (tx, ty) = target_xy(dir);
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![
                    float_fact("win-sub", "out-a", sx, sy),
                    float_fact("win-local", "out-a", lx, ly),
                    fact_at("win-t", "out-b", tx, ty),
                ],
                "win-sub",
                None,
                None,
            ),
        );
        // Establish the remembered tiled target on out-b, then return explicit
        // focus to the float subject.
        accepted(
            &mut engine,
            &report(
                2,
                screens.clone(),
                vec![
                    float_fact("win-sub", "out-a", sx, sy),
                    float_fact("win-local", "out-a", lx, ly),
                    fact_at("win-t", "out-b", tx, ty),
                ],
                "win-t",
                Some(11),
                Some(WorldCommand::Focus {
                    window: WindowId("win-t".to_owned()),
                    direction: Some(
                        crate::directional::Direction::parse_wire(dir).expect("direction"),
                    ),
                }),
            ),
        );
        accepted(
            &mut engine,
            &report(
                3,
                screens.clone(),
                vec![
                    float_fact("win-sub", "out-a", sx, sy),
                    float_fact("win-local", "out-a", lx, ly),
                    fact_at("win-t", "out-b", tx, ty),
                ],
                "win-sub",
                Some(12),
                Some(WorldCommand::Focus {
                    window: WindowId("win-sub".to_owned()),
                    direction: Some(
                        crate::directional::Direction::parse_wire(dir).expect("direction"),
                    ),
                }),
            ),
        );
        // Maximized float subject pressing dir must skip win-local and cross
        // to the remembered win-t.
        let reply = accepted(
            &mut engine,
            &report(
                4,
                screens,
                vec![
                    maximized_float_fact("win-sub", "out-a", sx, sy),
                    float_fact("win-local", "out-a", lx, ly),
                    fact_at("win-t", "out-b", tx, ty),
                ],
                "win-sub",
                Some(13),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-sub".to_owned()),
                    direction: crate::directional::Direction::parse_wire(dir).expect("direction"),
                }),
            ),
        );
        expect_explicit_focus(&reply, "win-t", 13, 4);
    }
}

#[test]
fn world_native_maximized_float_no_vertical_neighbor_retains() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                float_fact("win-sub", "out-a", 100, 220),
                float_fact("win-above", "out-a", 100, 0),
                fact("win-t", "out-b", 900),
            ],
            "win-sub",
            None,
            None,
        ),
    );
    // A same-output float waits above and a horizontal neighbor exists, but
    // with no vertical adjacent output pressing Up must retain without dispatch.
    let (reply, settlements) = accepted_with_settlements(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                maximized_float_fact("win-sub", "out-a", 100, 220),
                float_fact("win-above", "out-a", 100, 0),
                fact("win-t", "out-b", 900),
            ],
            "win-sub",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-sub".to_owned()),
                direction: crate::directional::Direction::Up,
            }),
        ),
    );
    let outcome = settlements
        .first()
        .and_then(|s| s.result.as_ref())
        .map(|result| match result {
            crate::world_report::CommandResult::Directional { outcome, .. } => outcome.as_str(),
            _ => "other",
        });
    assert_eq!(outcome, Some("focus-float-retained"));
    assert!(
        reply.focus_request.is_none(),
        "maximized float without vertical neighbor retains, got {:?}",
        reply.focus_request
    );
}

#[test]
fn world_native_maximized_float_single_output_retains() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            one_screen(),
            vec![
                float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
            ],
            "win-sub",
            None,
            None,
        ),
    );
    // A same-output float waits right, but with no adjacent output the
    // maximized subject must retain without dispatch.
    let (reply, settlements) = accepted_with_settlements(
        &mut engine,
        &report(
            2,
            one_screen(),
            vec![
                maximized_float_fact("win-sub", "out-a", 100, 100),
                float_fact("win-east", "out-a", 300, 100),
            ],
            "win-sub",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-sub".to_owned()),
                direction: crate::directional::Direction::Right,
            }),
        ),
    );
    let outcome = settlements
        .first()
        .and_then(|s| s.result.as_ref())
        .map(|result| match result {
            crate::world_report::CommandResult::Directional { outcome, .. } => outcome.as_str(),
            _ => "other",
        });
    assert_eq!(outcome, Some("focus-float-retained"));
    assert!(
        reply.focus_request.is_none(),
        "single-output maximized float retains, got {:?}",
        reply.focus_request
    );
}
