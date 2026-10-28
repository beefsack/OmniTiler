//! Command-owned linked/tiled focus memory and existing ownership boundaries.
//! New choices record before native writes when session binding gates allow it;
//! fail-closed residue and non-leaf storage remain outside this change. Retained
//! choices and convergence-owned paths have characterization coverage.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::world_commands::CommandApplied;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    EffectKind, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        gap: 0,
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
        native_workspace: None,
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
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: Vec::new(),
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        born_maximized: crate::born_maximized::BornMaximized::default(),
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
        creation_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true), screen("out-b", 800, false)]
}

fn focus_targets(reply: &crate::world_report::WorldReply) -> Vec<WindowId> {
    reply
        .effects
        .iter()
        .filter(|e| e.kind == EffectKind::Focus)
        .map(|e| e.target.clone())
        .collect()
}

/// Home domain of a linked window: the single session whose snapshot links it.
fn home_of(engine: &WorldEngine, window: &WindowId) -> Option<DomainKey> {
    for key in engine.engine.domain_keys() {
        if let Some(session) = engine.engine.session(&key)
            && session
                .snapshot()
                .windows
                .iter()
                .any(|l| &l.window == window)
        {
            return Some(key);
        }
    }
    None
}

/// Window currently named by session memory (`remembered_leaf` -> window).
fn remembered_window(engine: &WorldEngine, key: &DomainKey) -> Option<WindowId> {
    let session = engine.engine.session(key)?;
    let leaf = session.remembered_leaf(key)?;
    session.focused_window_for(&leaf, key)
}

fn focused_window(engine: &WorldEngine, key: &DomainKey) -> Option<WindowId> {
    let session = engine.engine.session(key)?;
    let (_, leaf) = session.focus();
    let leaf = leaf?;
    session.focused_window_for(&leaf, key)
}

fn bootstrap_two_tiled() -> (WorldEngine, DomainKey) {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            None,
            None,
        ),
    );
    let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    (engine, key)
}

/// Three tiled windows on out-a: lets a stale `last_active` name a window
/// that is neither the command subject nor the effect target.
fn bootstrap_three_tiled() -> (WorldEngine, DomainKey) {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-a", 400),
            ],
            "win-a",
            None,
            None,
        ),
    );
    let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    (engine, key)
}

/// Stale-memory driver: direct-focus `window`, so `last_active` names it.
/// The next command can then choose a target different from both this memory
/// and its reported foreground, isolating the command's own memory update.
fn stale_memory_to(engine: &mut WorldEngine, seq: u64, window: &str, focused: &str) {
    let reply = accepted(
        engine,
        &report(
            seq,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-a", 400),
            ],
            focused,
            Some(10),
            Some(WorldCommand::Focus {
                window: WindowId(window.to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId(window.to_owned())]);
}

fn no_focus_report(
    seq: u64,
    windows: Vec<WindowFact>,
    command_seq: u64,
    direction: &str,
) -> WorldReport {
    let mut out = report(
        seq,
        two_screens(),
        windows,
        "",
        Some(command_seq),
        Some(WorldCommand::FocusDirectional {
            window: WindowId(String::new()),
            direction: direction.to_owned(),
        }),
    );
    out.action_foreground = Some(OutputId("out-a".to_owned()));
    out.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    out
}

#[test]
fn direct_linked_focus_records_before_native_writes() {
    let (mut engine, key) = bootstrap_two_tiled();
    // Precondition: memory names win-a, win-b is linked but unremembered.
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-a".to_owned()))
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-b".to_owned())],
        "direct focus emits one effect"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "accepted linked focus records last_active before native writes"
    );
    assert_eq!(
        focused_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "session focus follows the recorded effect target"
    );
}

#[test]
fn view_return_select_records_linked_target_before_native_writes() {
    // Real view-return path (`retain_view`), never the direct focus lane:
    // send the remembered window away without follow, then select its
    // empty trailing workspace. The view return must name the moved
    // window (the only linked candidate there) and record it in target
    // memory before native writes, then the test leaves early.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            None,
            None,
        ),
    );
    let mut ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    ring.sort();
    assert!(ring.len() >= 2, "select needs a second workspace: {ring:?}");
    let current = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let target_ws = ring
        .iter()
        .find(|id| **id != current.workspace.0)
        .expect("other ws")
        .clone();
    // Leg 0: focus win-b through the direct lane (memory flips to it).
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-b".to_owned())]);
    // Leg 1: send the focused window away without follow. Stay names the
    // source survivor win-a and source memory must flip back to it
    // immediately, before native writes.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-b",
            Some(12),
            Some(WorldCommand::Send {
                window: WindowId("win-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                follow: false,
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-a".to_owned())],
        "send stay names the source survivor"
    );
    let source_key = home_of(&engine, &WindowId("win-a".to_owned())).expect("survivor linked");
    assert_eq!(
        remembered_window(&engine, &source_key),
        Some(WindowId("win-a".to_owned())),
        "send stay records the survivor before native writes"
    );
    // Leg 2: select the mover's trailing workspace. The view return must
    // name the moved window and record it in the target domain before
    // native writes. Native focus is still win-b (nothing actuated).
    let reply = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "win-b",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: target_ws.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-b".to_owned())],
        "view return names the moved window"
    );
    let target_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(target_ws.clone()),
    };
    assert_eq!(
        remembered_window(&engine, &target_key),
        Some(WindowId("win-b".to_owned())),
        "view return records the linked target before native writes"
    );
    // Leave early: no further legs. Real duplicate check: replaying the
    // consumed sequence reconciles only, never re-executes.
    let outcome = engine.handle_report(&report(
        5,
        two_screens(),
        windows(),
        "win-b",
        Some(13),
        Some(WorldCommand::SwitchView {
            output: OutputId("out-a".to_owned()),
            workspace: target_ws.clone(),
        }),
    ));
    assert!(
        matches!(outcome, WorldOutcome::CommandConsumed(_)),
        "replayed select stays consumed: {outcome:?}"
    );
    assert_eq!(
        remembered_window(&engine, &target_key),
        Some(WindowId("win-b".to_owned())),
        "consumed replay records nothing extra"
    );
}

#[test]
fn view_return_fallback_records_effect_target_not_remembered() {
    // Direct `retain_view` fallback: session memory names win-a, but the
    // command index marks win-a unexplained fit-excluded (ineligible), so
    // the resolved effect must name win-b AND session memory must follow
    // it before native writes. RED on the old seam: effect win-b while
    // memory stays win-a. No sweep runs between the legs, so convergence
    // cannot move memory first.
    let (mut engine, key) = bootstrap_two_tiled();
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-a".to_owned()))
    );
    let ring = engine
        .mapping
        .get(&key.output.0)
        .cloned()
        .expect("home ring");
    let other = ring
        .iter()
        .find(|id| **id != key.workspace.0)
        .cloned()
        .expect("second workspace");
    let probe = report(
        2,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
        "win-a",
        None,
        None,
    );
    let held = BTreeSet::new();
    // Leg 1: retain some other view so the return below actually changes
    // the view. The empty index resolves no candidate there; the logical
    // view still flips.
    let empty_by_id: BTreeMap<&WindowId, &EngineWindow> = BTreeMap::new();
    let _ = engine.retain_view(&probe, 11, &key.output, &other, &held, &empty_by_id);
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "view detour leaves home memory unchanged"
    );
    // Leg 2: crafted index over the SAME linked session state (untouched):
    // win-a unexplained fit-excluded falls through, win-b stays eligible.
    let owned = vec![
        EngineWindow {
            window: WindowId("win-a".to_owned()),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            rect: rect(0, 0, 40, 90),
            floating: false,
            fit_excluded: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        },
        EngineWindow {
            window: WindowId("win-b".to_owned()),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            rect: rect(200, 0, 40, 90),
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        },
    ];
    let mut by_id: BTreeMap<&WindowId, &EngineWindow> = BTreeMap::new();
    for window in &owned {
        by_id.insert(&window.window, window);
    }
    let applied = engine.retain_view(&probe, 12, &key.output, &key.workspace.0, &held, &by_id);
    let target = match applied {
        CommandApplied::Effect(effect) => effect.target,
        CommandApplied::NoEffect => panic!("fallback return must stage a focus effect"),
        CommandApplied::Rejected { kind, message } => {
            panic!("fallback return must not refuse: {kind} {message}")
        }
    };
    assert_eq!(
        target,
        WindowId("win-b".to_owned()),
        "ineligible remembered top falls through to win-b"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "view return records the effect target before native writes"
    );
    assert_eq!(
        focused_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "session focus follows the recorded effect target"
    );
}

#[test]
fn rejected_unknown_focus_records_nothing() {
    let (mut engine, key) = bootstrap_two_tiled();
    let before = remembered_window(&engine, &key);
    let outcome = engine.handle_report(&report(
        2,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
        "win-a",
        Some(11),
        Some(WorldCommand::Focus {
            window: WindowId("win-gone".to_owned()),
            direction: "right".to_owned(),
        }),
    ));
    assert!(
        matches!(outcome, WorldOutcome::Rejected { .. }),
        "unknown window rejects: {outcome:?}"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        before,
        "rejection records no extra decision"
    );
}

#[test]
fn redundant_view_select_records_nothing_extra() {
    let (mut engine, key) = bootstrap_two_tiled();
    let view = engine.logical_view("out-a").expect("view");
    let before = remembered_window(&engine, &key);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: view,
            }),
        ),
    );
    assert!(
        focus_targets(&reply).is_empty(),
        "redundant select stays NoEffect: {:?}",
        reply.effects
    );
    assert_eq!(
        remembered_window(&engine, &key),
        before,
        "duplicate select records no extra decision"
    );
}

#[test]
fn exception_focus_effect_leaves_memory_unchanged() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_fact = fact("win-f", "out-a", 0);
    float_fact.floating = true;
    accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), float_fact.clone()],
            "win-a",
            None,
            None,
        ),
    );
    let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let before = remembered_window(&engine, &key);
    assert!(
        engine
            .engine
            .session(&key)
            .expect("session")
            .is_exception(&WindowId("win-f".to_owned())),
        "win-f is an exception lifetime"
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), float_fact],
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-f".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-f".to_owned())],
        "exception focus still emits (existing contract)"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        before,
        "unrecordable exception target leaves memory unchanged"
    );
}

#[test]
fn held_scope_blocks_recording_but_keeps_contract() {
    let (mut engine, key) = bootstrap_two_tiled();
    let before = remembered_window(&engine, &key);
    // The seam itself must skip held domains with no new refusal: the
    // session validator alone would accept win-b, but the held gate holds.
    let mut held = BTreeSet::new();
    held.insert(key.clone());
    assert!(
        !engine.record_command_focus(&WindowId("win-b".to_owned()), &held),
        "held scope reports unrecorded through the seam"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        before,
        "held scope records nothing through the seam"
    );
    assert!(
        engine.record_command_focus(&WindowId("win-b".to_owned()), &BTreeSet::new()),
        "unheld retry records through the same seam"
    );
    assert!(
        engine.record_command_focus(&WindowId("win-b".to_owned()), &BTreeSet::new()),
        "unheld retry records through the same seam"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "unheld retry records through the same seam"
    );
}

/// Observation matching the live session state: tiled links plus
/// exception lifetimes, keyed to the session's own owner/generation/
/// revision so proposal gates validate instead of failing input.
fn session_observation_for(
    engine: &WorldEngine,
    key: &DomainKey,
) -> crate::session::SessionObservation {
    use crate::contract::Observation;
    use crate::ids::{GenerationId, OwnerId};
    let session = engine.engine.session(key).expect("session");
    let mut windows: Vec<crate::session::ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|link| crate::session::ObservedWindow {
            window: link.window.clone(),
            output: link.output.clone(),
            workspace: link.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        })
        .collect();
    windows.extend(session.exception_observed());
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    crate::session::SessionObservation {
        observation: Observation::new(
            OwnerId::parse("owner-a").expect("owner"),
            GenerationId::parse("gen-1").expect("generation"),
            session.accepted_revision(),
            100 + session.accepted_revision(),
        ),
        windows,
    }
}

fn staged_focus_of(
    engine: &WorldEngine,
    key: &DomainKey,
) -> (Option<DomainKey>, Option<crate::directional::NodeId>) {
    let session = engine.engine.session(key).expect("session");
    session.focus()
}

fn report_global_unique(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    use crate::workspace_scope::WorkspaceMode;
    let mut out = report(seq, two_screens(), windows, focused, command_seq, command);
    out.workspace_mode = WorkspaceMode::GlobalUnique;
    out
}

#[test]
fn global_swap_records_effect_target_in_moved_domain() {
    // Global-unique nonempty swap where the incoming side remembers an
    // ineligible window (unexplained fit-excluded keeps its link but
    // view-return skips it): the pre-move capture falls through to the
    // eligible win-b2, the view flight cannot prove (post-commit lookup
    // misses the stale staged domain, so retain_view returns NoEffect),
    // and the fallback emits the effect plus command-owned memory in the
    // moved domain before native writes. Relocate alone would preserve
    // the ineligible win-b1 leaf, so disabling the fallback seam must
    // turn the memory assertion red while the effect still names win-b2.
    use crate::directional::{OutputId, WorkspaceId};
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report_global_unique(1, vec![], "", None, None),
    );
    let mut ring_a: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    ring_a.sort();
    let mut ring_b: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
        .map(|row| row.id.clone())
        .collect();
    ring_b.sort();
    let (view_a, view_b) = (ring_a[0].clone(), ring_b[0].clone());
    assert_ne!(view_a, view_b, "outputs start on distinct workspaces");
    let windows = || {
        vec![
            fact("win-a1", "out-a", 0),
            fact("win-a2", "out-a", 200),
            fact("win-b1", "out-b", 810),
            fact("win-b2", "out-b", 920),
        ]
    };
    // Focus win-b1 so the incoming side remembers it, then mark it
    // unexplained fit-excluded on the swap leg: still linked, but
    // ineligible for view return.
    accepted(
        &mut engine,
        &report_global_unique(2, windows(), "win-b1", None, None),
    );
    let src_b = home_of(&engine, &WindowId("win-b1".to_owned())).expect("win-b1 linked");
    assert_eq!(
        remembered_window(&engine, &src_b),
        Some(WindowId("win-b1".to_owned())),
        "precondition: incoming side remembers win-b1"
    );
    let mut excluded = windows();
    excluded
        .iter_mut()
        .find(|row| row.window == WindowId("win-b1".to_owned()))
        .expect("win-b1 fact")
        .fit_excluded = true;
    // Swap: select out-b's workspace on out-a.
    let reply = accepted(
        &mut engine,
        &report_global_unique(
            3,
            excluded,
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: view_b.clone(),
            }),
        ),
    );
    let targets: Vec<WindowId> = reply
        .effects
        .iter()
        .filter(|e| e.is_explicit_focus_for(11))
        .map(|e| e.target.clone())
        .collect();
    assert_eq!(
        targets,
        vec![WindowId("win-b2".to_owned())],
        "swap fallback names the eligible incoming-side window"
    );
    let home = home_of(&engine, &WindowId("win-b2".to_owned())).expect("win-b2 linked");
    assert_eq!(
        home,
        DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(view_b.clone()),
        },
        "effect target rides the moved domain"
    );
    assert_eq!(
        remembered_window(&engine, &home),
        Some(WindowId("win-b2".to_owned())),
        "swap fallback records the effect target before native writes"
    );
}

#[test]
fn pending_plan_refusal_leaves_staged_focus_uncorrupted() {
    // A staged but unverified focus plan owns focus: the seam must report
    // unrecorded with no staged-focus or memory mutation, never pretending
    // the accepted target was recorded.
    use crate::contract::FocusCapabilities;
    use crate::directional::Direction;
    use crate::ids::CorrelationId;
    let (mut engine, key) = bootstrap_two_tiled();
    let obs = session_observation_for(&engine, &key);
    let before_mem = remembered_window(&engine, &key);
    let before_focus = staged_focus_of(&engine, &key);
    {
        let session = engine.engine.session_mut(&key).expect("session");
        session
            .propose_focus(
                &key,
                &WindowId("win-a".to_owned()),
                Direction::Right,
                &obs,
                &CorrelationId::parse("c-focus-1").expect("correlation"),
                &FocusCapabilities::full(),
            )
            .expect("pending focus plan stages");
        assert!(session.has_pending_desired(), "pending owns focus");
    }
    assert!(
        !engine.record_command_focus(&WindowId("win-b".to_owned()), &BTreeSet::new()),
        "pending plan reports unrecorded"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        before_mem,
        "pending refusal records no memory"
    );
    assert_eq!(
        staged_focus_of(&engine, &key),
        before_focus,
        "pending refusal corrupts no staged focus"
    );
}

#[test]
fn drag_capture_refusal_leaves_staged_focus_uncorrupted() {
    // An active drag capture owns focus the same way: fail-closed skip
    // with no mutation and a false outcome, never a pretended record.
    let (mut engine, key) = bootstrap_two_tiled();
    let obs = session_observation_for(&engine, &key);
    let before_mem = remembered_window(&engine, &key);
    let before_focus = staged_focus_of(&engine, &key);
    {
        let session = engine.engine.session_mut(&key).expect("session");
        session
            .begin_drag(&WindowId("win-a".to_owned()), &obs)
            .expect("drag captures the focused window");
        assert!(session.has_drag(), "drag owns focus");
    }
    assert!(
        !engine.record_command_focus(&WindowId("win-b".to_owned()), &BTreeSet::new()),
        "drag capture reports unrecorded"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        before_mem,
        "drag refusal records no memory"
    );
    assert_eq!(
        staged_focus_of(&engine, &key),
        before_focus,
        "drag refusal corrupts no staged focus"
    );
}

#[test]
fn divergence_refusal_leaves_staged_focus_uncorrupted() {
    // Terminal divergence fails every rebind closed: no mutation, false
    // outcome, staged focus untouched.
    let (mut engine, key) = bootstrap_two_tiled();
    let before_mem = remembered_window(&engine, &key);
    let before_focus = staged_focus_of(&engine, &key);
    engine
        .engine
        .session_mut(&key)
        .expect("session")
        .note_adapter_loss();
    assert!(
        !engine.record_command_focus(&WindowId("win-b".to_owned()), &BTreeSet::new()),
        "divergence reports unrecorded"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        before_mem,
        "divergence records no memory"
    );
    assert_eq!(
        staged_focus_of(&engine, &key),
        before_focus,
        "divergence corrupts no staged focus"
    );
}

#[test]
fn binding_refusal_reports_unrecorded_without_mutation() {
    // No linked home (unknown window): the seam reports unrecorded with
    // no mutation anywhere, never a pretended record.
    let (mut engine, key) = bootstrap_two_tiled();
    let before_mem = remembered_window(&engine, &key);
    let before_focus = staged_focus_of(&engine, &key);
    assert!(
        !engine.record_command_focus(&WindowId("win-gone".to_owned()), &BTreeSet::new()),
        "unknown window reports unrecorded"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        before_mem,
        "binding refusal records no memory"
    );
    assert_eq!(
        staged_focus_of(&engine, &key),
        before_focus,
        "binding refusal corrupts no staged focus"
    );
}

#[test]
fn no_focus_cross_preserves_remembered_target() {
    // Characterization, not failing-first: the no-focus cross target IS the
    // destination remembered selection by construction
    // (`remembered_leaf`), so memory already names it and any recording
    // would be a no-op. The test pins the effect/memory agreement.
    // Seed remembered selections on both outputs, lose host focus, then
    // cross with no subject: the effect must name the destination's
    // already-remembered selection.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = || {
        vec![
            fact("win-a1", "out-a", 10),
            fact("win-a2", "out-a", 100),
            fact("win-b1", "out-b", 810),
            fact("win-b2", "out-b", 900),
        ]
    };
    accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a2", None, None),
    );
    accepted(
        &mut engine,
        &report(2, two_screens(), windows(), "win-b2", None, None),
    );
    accepted(
        &mut engine,
        &report(3, two_screens(), windows(), "", None, None),
    );
    let reply = accepted(&mut engine, &no_focus_report(4, windows(), 11, "right"));
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-b2".to_owned())],
        "no-focus crossing names the destination selection"
    );
    let target_key = home_of(&engine, &WindowId("win-b2".to_owned())).expect("win-b2 linked");
    assert_eq!(
        remembered_window(&engine, &target_key),
        Some(WindowId("win-b2".to_owned())),
        "no-focus cross preserves the remembered target"
    );
}

#[test]
fn local_move_preserves_mover_memory() {
    // Characterization, not failing-first: the move gate requires the
    // mover to be the observed focus, so the pre-command sweep convergence
    // already names the mover in session memory (Engine-off plus
    // seam-off stays green). The test pins the effect/memory agreement.
    let (mut engine, key) = bootstrap_two_tiled();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-b",
            Some(11),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-b".to_owned()),
                direction: "left".to_owned(),
                same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-b".to_owned())],
        "local move keeps focus on the mover"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "linked move records the mover before native writes"
    );
}

#[test]
fn ordinary_directional_focus_records_effect_target() {
    // Stale `last_active` names win-c; the leftward focus from win-b must
    // flip memory to win-a before native writes. The Engine focus commit
    // owns this (no world seam records here): disabling the commit leaves
    // memory on observed win-b while the effect still names win-a.
    let (mut engine, key) = bootstrap_three_tiled();
    stale_memory_to(&mut engine, 2, "win-c", "win-a");
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-c".to_owned())),
        "precondition: last_active names win-c"
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-a", 400),
            ],
            "win-b",
            Some(12),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-b".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-a".to_owned())],
        "leftward focus names win-a"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "directional focus flips last_active before native writes"
    );
}

#[test]
fn cross_output_move_records_mover_in_target() {
    // Pinned cross-output move through the pair path: the mover stands
    // alone on out-a (no local neighbor, so the pair plans R4 cross), and
    // stale `last_active` on out-b names win-c. The Engine move commit
    // owns target memory (no world seam records here).
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-b", 810),
            fact("win-c", "out-b", 920),
        ]
    };
    accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-c".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-c".to_owned())]);
    let source_key = home_of(&engine, &WindowId("win-b".to_owned())).expect("win-b linked");
    assert_eq!(
        remembered_window(&engine, &source_key),
        Some(WindowId("win-c".to_owned())),
        "precondition: stale last_active names win-c"
    );
    let pre_key = home_of(&engine, &WindowId("win-a".to_owned())).expect("mover linked");
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-a",
            Some(12),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-a".to_owned())],
        "cross move keeps focus on the mover"
    );
    let target_key = home_of(&engine, &WindowId("win-a".to_owned())).expect("mover linked");
    assert_ne!(target_key, pre_key, "mover actually crossed outputs");
    assert_eq!(
        remembered_window(&engine, &target_key),
        Some(WindowId("win-a".to_owned())),
        "cross move records the mover in the target before native writes"
    );
}

#[test]
fn swap_neighbor_move_preserves_mover_memory() {
    // Characterization, not failing-first: same convergence-owned reason
    // as the local move (observed focus must be the mover). The test pins
    // the effect/memory agreement.
    let (mut engine, key) = bootstrap_three_tiled();
    stale_memory_to(&mut engine, 2, "win-c", "win-a");
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-c".to_owned())),
        "precondition: last_active names win-c"
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 200),
                fact("win-c", "out-a", 400),
            ],
            "win-b",
            Some(12),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-b".to_owned()),
                direction: "left".to_owned(),
                same_axis: crate::directional::SameAxisMove::SwapWithNeighbor,
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-b".to_owned())],
        "swap keeps focus on the mover"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "swap records the mover before native writes"
    );
}

#[test]
fn float_lane_focus_leaves_memory_unchanged() {
    // Float-lane directional focus names an exception lifetime by
    // construction (candidates are floating/sticky only), so the command
    // emits its effect but leaves tiled memory untouched. Non-leaf memory
    // requires a separate model decision.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_b = fact("win-f1", "out-a", 0);
    float_b.floating = true;
    let mut float_c = fact("win-f2", "out-a", 200);
    float_c.floating = true;
    let windows = || {
        vec![
            fact("win-a", "out-a", 400),
            float_b.clone(),
            float_c.clone(),
        ]
    };
    accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let key = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    assert!(
        engine
            .engine
            .session(&key)
            .expect("session")
            .is_exception(&WindowId("win-f2".to_owned())),
        "float candidate is an exception lifetime"
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-f1",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-f1".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-f2".to_owned())],
        "float lane names the float candidate"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "unrecordable float target leaves tiled memory unchanged"
    );
}

#[test]
fn send_follow_records_mover_in_target_before_native_writes() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            None,
            None,
        ),
    );
    let mut ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    ring.sort();
    assert!(ring.len() >= 2, "send needs a second workspace: {ring:?}");
    let current = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let target_ws = ring
        .iter()
        .find(|id| **id != current.workspace.0)
        .expect("other ws");
    // Stale `last_active` names win-b; the follow names the mover win-a.
    // The Engine transfer commit owns target memory (no world seam records
    // here): disabling the commit leaves memory on win-b.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-b".to_owned())]);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)],
            "win-a",
            Some(12),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                follow: true,
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-a".to_owned())],
        "follow names the mover"
    );
    let target_key = home_of(&engine, &WindowId("win-a".to_owned())).expect("mover linked");
    assert_eq!(
        remembered_window(&engine, &target_key),
        Some(WindowId("win-a".to_owned())),
        "send follow records the mover in the target before native writes"
    );
}

#[test]
fn close_focused_falls_back_to_mru_survivor_without_command_effect() {
    // Close/remove successor is convergence-owned, never command-chosen:
    // no WorldCommand names a close, so no seam records here. Closing the
    // focused window with empty observed focus resolves session memory
    // (`last_active`) and focus to the MRU survivor through the convergence
    // fallback, with no explicit-focus effect. Disabling the convergence
    // `last_active` update turns the memory assertion red while the
    // effect assertion stays green.
    let (mut engine, key) = bootstrap_two_tiled();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 200)];
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-b".to_owned())]);
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "precondition: memory names the soon-closed window"
    );
    // Omission plus the explicit close; the survivor is never pre-reported
    // as focused, so only the MRU fallback can name it.
    let mut close = report(
        3,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        "",
        None,
        None,
    );
    close.closed = vec![WindowId("win-b".to_owned())];
    let reply = accepted(&mut engine, &close);
    assert!(
        focus_targets(&reply).is_empty(),
        "close emits no command-owned focus effect: {:?}",
        reply.effects
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "convergence MRU fallback records the survivor"
    );
    assert_eq!(
        focused_window(&engine, &key),
        Some(WindowId("win-a".to_owned())),
        "session focus follows the surviving leaf"
    );
}

#[test]
fn local_move_direct_records_mover_without_sweep() {
    // Direct command-seam characterization (no-new-choice, not
    // failing-first against `verify_move`): stale `last_active` names
    // win-c while the staged report names win-b as observed focus and
    // mover. No `handle_report` sweep runs, but the Engine pre-command
    // convergence inside `execute_command` (`Engine::handle` ->
    // `converge_for_single_domain` for `CoreCommand::Move`) still names
    // win-b first: the `FocusMismatch` gate requires mover == observed
    // focus, so same-domain convergence necessarily records the mover
    // before the plan commits, and `verify_move` then writes the same
    // choice again. Disabling either write alone stays green
    // (redundant double-write); disabling both turns the memory
    // assertion red while the effect still names win-b.
    let (mut engine, key) = bootstrap_three_tiled();
    stale_memory_to(&mut engine, 2, "win-c", "win-a");
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-c".to_owned())),
        "precondition: last_active names win-c"
    );
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 200),
        fact("win-c", "out-a", 400),
    ];
    let command = WorldCommand::MoveDirectional {
        window: WindowId("win-b".to_owned()),
        direction: "left".to_owned(),
        same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
    };
    let probe = report(
        3,
        two_screens(),
        windows,
        "win-b",
        Some(12),
        Some(command.clone()),
    );
    engine.stage_facts(&probe);
    engine.build_staged_domains(&probe);
    let applied = engine.execute_command(&probe, 12, &command, &BTreeSet::new(), None, None);
    let target = match applied {
        CommandApplied::Effect(effect) => effect.target,
        CommandApplied::NoEffect => panic!("local move must stage a focus effect"),
        CommandApplied::Rejected { kind, message } => {
            panic!("local move must not refuse: {kind} {message}")
        }
    };
    assert_eq!(
        target,
        WindowId("win-b".to_owned()),
        "local move keeps focus on the mover"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "direct move records the mover before native writes"
    );
    assert_eq!(
        focused_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "session focus follows the recorded effect target"
    );
}

#[test]
fn swap_neighbor_move_direct_records_mover_without_sweep() {
    // Same direct seam as the local move, but the `SwapWithNeighbor`
    // same-axis variant. Same no-new-choice ownership: the Engine
    // pre-command convergence (`converge_for_single_domain` for
    // `CoreCommand::Move`) names win-b before the plan commits, since
    // the `FocusMismatch` gate requires mover == observed focus, and
    // `verify_move` rewrites the same choice. Disabling either write
    // alone stays green; disabling both turns memory red with the
    // effect still naming win-b.
    let (mut engine, key) = bootstrap_three_tiled();
    stale_memory_to(&mut engine, 2, "win-c", "win-a");
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-c".to_owned())),
        "precondition: last_active names win-c"
    );
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 200),
        fact("win-c", "out-a", 400),
    ];
    let command = WorldCommand::MoveDirectional {
        window: WindowId("win-b".to_owned()),
        direction: "left".to_owned(),
        same_axis: crate::directional::SameAxisMove::SwapWithNeighbor,
    };
    let probe = report(
        3,
        two_screens(),
        windows,
        "win-b",
        Some(12),
        Some(command.clone()),
    );
    engine.stage_facts(&probe);
    engine.build_staged_domains(&probe);
    let applied = engine.execute_command(&probe, 12, &command, &BTreeSet::new(), None, None);
    let target = match applied {
        CommandApplied::Effect(effect) => effect.target,
        CommandApplied::NoEffect => panic!("swap must stage a focus effect"),
        CommandApplied::Rejected { kind, message } => {
            panic!("swap must not refuse: {kind} {message}")
        }
    };
    assert_eq!(
        target,
        WindowId("win-b".to_owned()),
        "swap keeps focus on the mover"
    );
    assert_eq!(
        remembered_window(&engine, &key),
        Some(WindowId("win-b".to_owned())),
        "direct swap records the mover before native writes"
    );
}

#[test]
fn send_directional_follow_records_mover_in_target() {
    // Direction-only cross-output send (`SendDirectional` right from
    // out-a): the distinct `execute_output_send` path (not the
    // same-output `execute_send` path), same `verify_lifecycle` memory
    // owner. Target MRU names win-d; follow lands win-a there. The
    // sweep cannot mask the target: the mover is still observed in the
    // source, so only the lifecycle commit can name win-a in the
    // target. Disabling that write leaves target memory on win-d while
    // the effect still names win-a.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 200),
            fact("win-c", "out-b", 810),
            fact("win-d", "out-b", 920),
        ]
    };
    accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-d".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-d".to_owned())]);
    let target_pre = home_of(&engine, &WindowId("win-d".to_owned())).expect("win-d linked");
    assert_eq!(
        remembered_window(&engine, &target_pre),
        Some(WindowId("win-d".to_owned())),
        "precondition: target MRU names win-d"
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-a",
            Some(12),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: true,
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-a".to_owned())],
        "directional follow names the mover"
    );
    let target_key = home_of(&engine, &WindowId("win-a".to_owned())).expect("mover linked");
    assert_ne!(
        target_key.output.0, "out-a",
        "mover actually crossed outputs"
    );
    assert_eq!(
        remembered_window(&engine, &target_key),
        Some(WindowId("win-a".to_owned())),
        "directional follow records the mover in the target before native writes"
    );
}

#[test]
fn send_directional_stay_records_survivor_in_source() {
    // Direction-only cross-output send without follow: the same
    // `execute_output_send` path with `follow: false`. Stay keeps the
    // source selected with the focused-removal MRU fallback, so the
    // source stack needs depth below the mover: focus win-b, then
    // re-focus win-a, then stay-send win-a. The survivor win-b is a new
    // choice the sweep cannot make (it converges the source to the
    // observed mover win-a), so only the lifecycle commit can record
    // it. Disabling that write leaves no resolvable source memory (the
    // old win-a leaf moved away) while the effect still names win-b.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 200),
            fact("win-c", "out-b", 810),
            fact("win-d", "out-b", 920),
        ]
    };
    accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-b",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-a".to_owned())]);
    let source_pre = home_of(&engine, &WindowId("win-b".to_owned())).expect("win-b linked");
    assert_eq!(
        remembered_window(&engine, &source_pre),
        Some(WindowId("win-a".to_owned())),
        "precondition: source MRU names the mover with depth below"
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-a",
            Some(12),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-b".to_owned())],
        "directional stay names the source survivor"
    );
    let source_key = home_of(&engine, &WindowId("win-b".to_owned())).expect("survivor linked");
    assert_eq!(
        remembered_window(&engine, &source_key),
        Some(WindowId("win-b".to_owned())),
        "directional stay records the survivor before native writes"
    );
}

#[test]
fn observed_float_away_back_returns_float() {
    // Mouse path: ordinary float observed focused alongside a tile, then
    // leave and return. Before the fix, leaf-only return named the tile
    // instead of the observed float.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_f = fact("win-f", "out-a", 200);
    float_f.floating = true;
    let windows = || vec![fact("win-a", "out-a", 0), float_f.clone()];
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    // Mouse observation: native focus lands on the float, no command.
    let _ = accepted(
        &mut engine,
        &report(2, two_screens(), windows(), "win-f", None, None),
    );
    // Leave to the spare while the float is still natively focused.
    let _ = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-f",
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    // Return: approved behavior re-emits the observed float.
    let back = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "",
            Some(12),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-f".to_owned())],
        "observed float survives away/back"
    );
}

#[test]
fn keyboard_float_lane_leave_before_observe_returns_float() {
    // Keyboard path (`float_lane_focus_leaves_memory_unchanged` setup):
    // float-lane effect names win-f2, leave BEFORE native observes win-f2,
    // return must still name win-f2. Before the fix, leaf-only return
    // named the tile instead.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_b = fact("win-f1", "out-a", 0);
    float_b.floating = true;
    let mut float_c = fact("win-f2", "out-a", 200);
    float_c.floating = true;
    let windows = || {
        vec![
            fact("win-a", "out-a", 400),
            float_b.clone(),
            float_c.clone(),
        ]
    };
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-f1",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-f1".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-f2".to_owned())],
        "float lane names win-f2"
    );
    // Leave before the target is ever natively observed (focus still win-f1).
    let _ = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-f1",
            Some(12),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-f2".to_owned())],
        "unlanded keyboard float survives away/back"
    );
}

#[test]
fn float_only_workspace_returns_float() {
    // Tiled-mode workspace whose only resident is a float: send the tile
    // away without follow, leave, return. Before the fix, leaf-only
    // candidates found no tiled member and emitted no focus.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_f = fact("win-f", "out-a", 200);
    float_f.floating = true;
    let windows = || vec![fact("win-a", "out-a", 0), float_f.clone()];
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    // Move the only tile away without follow: home keeps the float.
    let _ = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: other.clone(),
                follow: false,
            }),
        ),
    );
    // Leave the now float-only home for the spare.
    let _ = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-f",
            Some(12),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-f".to_owned())],
        "float-only tiled workspace returns the float"
    );
}

#[test]
fn sticky_shown_away_from_home_returns_sticky() {
    // Sticky homed on the bootstrap workspace but command-focused while
    // the spare is shown (effective domain != home via `effective_domain`).
    // Return to the spare must name the sticky; before the fix, return
    // filtered sticky rows and stayed quiet (spare has no tile). Avoid
    // `observed_workspace` on the sticky fact so ingestion never rehomes
    // the sticky native home; shown-vs-home split comes only from the
    // retained logical view.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut sticky_s = fact("win-s", "out-a", 200);
    sticky_s.sticky = true;
    let windows = || vec![fact("win-a", "out-a", 0), sticky_s.clone()];
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    // Show the spare (no tile there yet).
    let _ = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    // Command-focus the sticky while the spare is shown: effective domain
    // is the spare, actual home stays the bootstrap workspace.
    let focused = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-a",
            Some(12),
            Some(WorldCommand::Focus {
                window: WindowId("win-s".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert!(
        !focus_targets(&focused).is_empty()
            && focus_targets(&focused)
                .iter()
                .all(|t| *t == WindowId("win-s".to_owned())),
        "sticky focuses from the shown spare: {:?}",
        focused.effects
    );
    // Detour to home, then return to the spare: sticky must come back.
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "win-s",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            windows(),
            "",
            Some(14),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-s".to_owned())],
        "sticky shown away from home survives away/back"
    );
}

#[test]
fn later_tile_outranks_stale_home_sticky_on_return() {
    // A sticky with a truthful native receipt stays linked at home while
    // shown away, so focusing it away records home[spare]=win-s. A tile
    // admitted and focused on the spare afterwards must win the later
    // return: live owner memory shadows the older foreign sticky
    // identity instead of letting it outrank the latest target tile.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut sticky_s = fact("win-s", "out-a", 200);
    sticky_s.sticky = true;
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), sticky_s.clone()],
            "win-a",
            None,
            None,
        ),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    // Pin the sticky home with a truthful receipt from here on: the link
    // never migrates, only the shown view moves.
    let mut sticky_home = sticky_s.clone();
    sticky_home.observed_workspace = Some(home.workspace.0.clone());
    let windows = || vec![fact("win-a", "out-a", 0), sticky_home.clone()];
    // Show the spare (no tile there yet).
    let _ = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    // Focus the sticky while the spare is shown: the home session records
    // the effective spare domain without moving the link.
    let focused = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-a",
            Some(12),
            Some(WorldCommand::Focus {
                window: WindowId("win-s".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert!(
        !focus_targets(&focused).is_empty()
            && focus_targets(&focused)
                .iter()
                .all(|t| *t == WindowId("win-s".to_owned())),
        "sticky focuses from the shown spare: {:?}",
        focused.effects
    );
    assert_eq!(
        home_of(&engine, &WindowId("win-s".to_owned())),
        Some(home.clone()),
        "receipt-pinned sticky stays linked at home"
    );
    // Detour home, then return: the foreign home memory answers while the
    // spare owns nothing.
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "win-s",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            windows(),
            "",
            Some(14),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-s".to_owned())],
        "foreign home sticky answers a tile-less spare"
    );
    // Admit a tile on the spare and focus it there.
    let tiled = || {
        let mut rows = windows();
        rows.push(fact("win-t", "out-a", 400));
        rows
    };
    let _ = accepted(
        &mut engine,
        &report(6, two_screens(), tiled(), "win-a", None, None),
    );
    let tile_focused = accepted(
        &mut engine,
        &report(
            7,
            two_screens(),
            tiled(),
            "win-a",
            Some(15),
            Some(WorldCommand::Focus {
                window: WindowId("win-t".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&tile_focused),
        vec![WindowId("win-t".to_owned())],
        "tile focuses on the spare"
    );
    // Detour home, then return: the latest target-session tile outranks
    // the stale home sticky identity.
    let _ = accepted(
        &mut engine,
        &report(
            8,
            two_screens(),
            tiled(),
            "win-t",
            Some(16),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            9,
            two_screens(),
            tiled(),
            "",
            Some(17),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-t".to_owned())],
        "later tile outranks stale home sticky on return"
    );
}

// Pinned last-active ordering coverage (all GREEN): per-workspace last
// active wins regardless of kind (tile / ordinary float / sticky). The
// S1 gap (tiled operation focus, send survivors, admission newcomers
// never recorded) is covered by the `last_active_*_supersedes_stale_*`
// tests below: the shared publish-time recorder backfills every accepted
// Focus effect onto the one canonical slot, latest wins.
// Each probe isolates one last-active ordering question against the
// no-preference contract.

#[test]
fn last_active_sticky_after_tile_same_home_returns_sticky() {
    // Tile focused, then sticky focused, both while home is shown.
    // Contract: last active (sticky) wins the away/back return.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut sticky_s = fact("win-s", "out-a", 200);
    sticky_s.sticky = true;
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            sticky_s.clone(),
        ]
    };
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-b".to_owned())]);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-b",
            Some(12),
            Some(WorldCommand::Focus {
                window: WindowId("win-s".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&reply),
        vec![WindowId("win-s".to_owned())],
        "sticky focus emits while home is shown"
    );
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "win-s",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            windows(),
            "",
            Some(14),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-s".to_owned())],
        "sticky focused after tile at home survives away/back"
    );
}

#[test]
fn last_active_sticky_after_tile_on_spare_returns_sticky() {
    // Tile admitted+focused on the spare, then the home-pinned sticky is
    // focused while the spare is shown. Contract: the later sticky wins.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut sticky_s = fact("win-s", "out-a", 200);
    sticky_s.sticky = true;
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), sticky_s.clone()],
            "win-a",
            None,
            None,
        ),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let mut sticky_home = sticky_s.clone();
    sticky_home.observed_workspace = Some(home.workspace.0.clone());
    let windows = || vec![fact("win-a", "out-a", 0), sticky_home.clone()];
    let _ = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let tiled = || {
        let mut rows = windows();
        rows.push(fact("win-t", "out-a", 400));
        rows
    };
    let _ = accepted(
        &mut engine,
        &report(3, two_screens(), tiled(), "win-a", None, None),
    );
    let tile_focused = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            tiled(),
            "win-a",
            Some(12),
            Some(WorldCommand::Focus {
                window: WindowId("win-t".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&tile_focused),
        vec![WindowId("win-t".to_owned())],
        "tile focuses on the spare"
    );
    let sticky_focused = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            tiled(),
            "win-t",
            Some(13),
            Some(WorldCommand::Focus {
                window: WindowId("win-s".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&sticky_focused),
        vec![WindowId("win-s".to_owned())],
        "sticky focuses from the shown spare"
    );
    let _ = accepted(
        &mut engine,
        &report(
            6,
            two_screens(),
            tiled(),
            "win-s",
            Some(14),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            7,
            two_screens(),
            tiled(),
            "",
            Some(15),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-s".to_owned())],
        "later sticky outranks earlier spare tile on return"
    );
}

#[test]
fn last_active_later_tile_after_float_returns_tile() {
    // Float recorded first, tile focused after, both at home.
    // Contract: last active (tile) wins. Control: must pass.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_f = fact("win-f", "out-a", 200);
    float_f.floating = true;
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            float_f.clone(),
        ]
    };
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-f".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-f".to_owned())]);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-f",
            Some(12),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-b".to_owned())]);
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "win-b",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            windows(),
            "",
            Some(14),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-b".to_owned())],
        "later tile outranks earlier float on return"
    );
}

#[test]
fn last_active_unchanged_tile_observation_preserves_float_command() {
    // Float command records win-f while native still reports the old tile
    // focus (command before native actuation, native unchanged). Contract:
    // an unchanged tile re-report carries no new information and must not
    // clobber the recorded float.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_f = fact("win-f", "out-a", 200);
    float_f.floating = true;
    let windows = || vec![fact("win-a", "out-a", 0), float_f.clone()];
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-f".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-f".to_owned())]);
    // Native never actuated: same tile focus re-reported, no command.
    let _ = accepted(
        &mut engine,
        &report(3, two_screens(), windows(), "win-a", None, None),
    );
    assert_eq!(
        engine.canonical_memory(&home),
        Some(WindowId("win-f".to_owned())),
        "unchanged tile re-report preserves the recorded float"
    );
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "win-a",
            Some(12),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            windows(),
            "",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-f".to_owned())],
        "unlanded float command survives away/back"
    );
}

#[test]
fn last_active_closed_float_prunes_memory_no_ghost() {
    // Closing the recorded float prunes its identity; the return names the
    // surviving tile with no ghost focus. Control: must pass.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_f = fact("win-f", "out-a", 200);
    float_f.floating = true;
    let windows = || vec![fact("win-a", "out-a", 0), float_f.clone()];
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-f".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-f".to_owned())]);
    let mut close = report(
        3,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        "",
        None,
        None,
    );
    close.closed = vec![WindowId("win-f".to_owned())];
    let _ = accepted(&mut engine, &close);
    assert!(
        engine
            .canonical_memory(&home)
            .is_none_or(|w| w != WindowId("win-f".to_owned())),
        "closed float identity is pruned"
    );
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            Some(12),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            "",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-a".to_owned())],
        "closed float leaves the surviving tile"
    );
}

#[test]
fn last_active_no_focus_report_preserves_float_memory() {
    // An empty-focus report (unknown/lost foreground) records nothing and
    // preserves the recorded float. Control: must pass.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_f = fact("win-f", "out-a", 200);
    float_f.floating = true;
    let windows = || vec![fact("win-a", "out-a", 0), float_f.clone()];
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-f".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-f".to_owned())]);
    let _ = accepted(
        &mut engine,
        &report(3, two_screens(), windows(), "", None, None),
    );
    assert_eq!(
        engine.canonical_memory(&home),
        Some(WindowId("win-f".to_owned())),
        "empty focus preserves the recorded float"
    );
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "",
            Some(12),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            windows(),
            "",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-f".to_owned())],
        "float survives an empty-focus report across away/back"
    );
}

#[test]
fn last_active_no_focus_empty_target_with_visible_sticky_stays_quiet() {
    // Sticky recorded at home, then a no-focus switch to an empty spare
    // where only the sticky is visible: the spare names no canonical
    // target and has no tiled fallback, so the switch stays NoEffect and
    // records nothing; home memory survives for the return.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut sticky_s = fact("win-s", "out-a", 200);
    sticky_s.sticky = true;
    let windows = || vec![fact("win-a", "out-a", 0), sticky_s.clone()];
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-s".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-s".to_owned())]);
    let away = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "",
            Some(12),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    assert!(
        away.effects.is_empty(),
        "empty spare with only a visible sticky invents no focus"
    );
    assert_eq!(
        engine.canonical_memory(&home),
        Some(WindowId("win-s".to_owned())),
        "away switch preserves home sticky memory"
    );
    let spare = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(other.clone()),
    };
    assert_eq!(
        engine.canonical_memory(&spare),
        None,
        "sticky records only where it was selected, never the spare it shows on"
    );
    let back = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            "",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-s".to_owned())],
        "return restores the sticky recorded at home"
    );
}

#[test]
fn last_active_tiled_operation_focus_supersedes_stale_sticky() {
    // S1: sticky recorded first, then a tiled Operation Focus (local,
    // cross_output_transfer=false) names a tile before native observation
    // moves. The accepted tile must overwrite the sticky slot so
    // leave/return yields the tile.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut sticky_s = fact("win-s", "out-a", 200);
    sticky_s.sticky = true;
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            sticky_s.clone(),
        ]
    };
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-s".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-s".to_owned())]);
    assert_eq!(
        engine.canonical_memory(&home),
        Some(WindowId("win-s".to_owned())),
        "precondition: sticky recorded at home"
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-s",
            Some(12),
            Some(WorldCommand::Operation(
                crate::boundary::CoreCommand::Focus {
                    window: "win-s".to_owned(),
                    direction: "left".to_owned(),
                    cross_output_transfer: false,
                    float_subject: false,
                },
            )),
        ),
    );
    let targets = focus_targets(&reply);
    assert_eq!(targets.len(), 1, "tiled operation focus emits once");
    let tile = targets.into_iter().next().expect("tile target");
    assert!(
        tile != WindowId("win-s".to_owned()),
        "operation focus names a tile, got {tile:?}"
    );
    assert_eq!(
        engine.canonical_memory(&home),
        Some(tile.clone()),
        "tiled operation focus overwrites the stale sticky"
    );
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            tile.0.as_str(),
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            windows(),
            "",
            Some(14),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back).last(),
        Some(&tile),
        "leave/return yields the newer tile, not the stale sticky"
    );
}

#[test]
fn last_active_send_stay_survivor_supersedes_stale_sticky() {
    // Stale float recorded at home, then a same-output send with stay=false
    // names the tiled source survivor. That tile must overwrite the
    // float slot so leave/return yields the tile.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut float_f = fact("win-f", "out-a", 200);
    float_f.floating = true;
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            float_f.clone(),
        ]
    };
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-f".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-f".to_owned())]);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-a",
            Some(12),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: other.clone(),
                follow: false,
            }),
        ),
    );
    let targets = focus_targets(&reply);
    assert_eq!(targets.len(), 1, "send-stay names the source survivor");
    let survivor = targets.into_iter().next().expect("survivor");
    assert!(
        survivor != WindowId("win-f".to_owned()),
        "send-stay names a tile, got {survivor:?}"
    );
    assert_eq!(
        engine.canonical_memory(&home),
        Some(survivor.clone()),
        "send-stay survivor overwrites the stale sticky at home"
    );
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            windows(),
            survivor.0.as_str(),
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            windows(),
            "",
            Some(14),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![survivor],
        "leave/return yields the send-stay tile, not the stale sticky"
    );
}

#[test]
fn last_active_admission_newcomer_supersedes_stale_sticky() {
    // Sticky recorded at home, then a fresh tiled newcomer is admitted
    // with automatic focus. The admission tile must overwrite the sticky
    // slot so leave/return yields the newcomer.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut sticky_s = fact("win-s", "out-a", 200);
    sticky_s.sticky = true;
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            sticky_s.clone(),
        ]
    };
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-s".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-s".to_owned())]);
    let mut arrived = windows();
    arrived.push(fact("win-n", "out-a", 300));
    let reply = accepted(
        &mut engine,
        &report(3, two_screens(), arrived.clone(), "win-s", None, None),
    );
    let targets = focus_targets(&reply);
    assert_eq!(
        targets,
        vec![WindowId("win-n".to_owned())],
        "admission names the newcomer"
    );
    assert_eq!(
        engine.canonical_memory(&home),
        Some(WindowId("win-n".to_owned())),
        "admitted newcomer overwrites the stale sticky"
    );
    let _ = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            arrived.clone(),
            "win-n",
            Some(12),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            arrived,
            "",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back).last(),
        Some(&WindowId("win-n".to_owned())),
        "leave/return yields the admitted newcomer, not the stale sticky"
    );
}

#[test]
fn last_active_close_after_tile_supersession_falls_back_to_tiled() {
    // Sticky recorded, then a tiled Operation focus overwrites the slot.
    // Closing that tile prunes the slot; the effect-free close/return
    // must use the eligible tiled fallback (win-a), never the stale
    // sticky exception.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut sticky_s = fact("win-s", "out-a", 200);
    sticky_s.sticky = true;
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            sticky_s.clone(),
        ]
    };
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "win-a", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-s".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(focus_targets(&reply), vec![WindowId("win-s".to_owned())]);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-s",
            Some(12),
            Some(WorldCommand::Operation(
                crate::boundary::CoreCommand::Focus {
                    window: "win-s".to_owned(),
                    direction: "left".to_owned(),
                    cross_output_transfer: false,
                    float_subject: false,
                },
            )),
        ),
    );
    let tile = focus_targets(&reply)
        .into_iter()
        .next()
        .expect("tile target");
    assert!(
        tile != WindowId("win-s".to_owned()),
        "operation focus names a tile, got {tile:?}"
    );
    assert_eq!(
        engine.canonical_memory(&home),
        Some(tile.clone()),
        "tiled operation focus overwrites the stale sticky"
    );
    let mut close = report(
        4,
        two_screens(),
        vec![fact("win-a", "out-a", 0), sticky_s.clone()],
        "win-a",
        None,
        None,
    );
    close.closed = vec![tile];
    let reply = accepted(&mut engine, &close);
    assert!(
        focus_targets(&reply).is_empty(),
        "close emits no focus effect, got {:?}",
        reply.effects
    );
    assert_eq!(
        engine.canonical_memory(&home),
        Some(WindowId("win-a".to_owned())),
        "closed tile pruned; observed tile survivor recorded, stale sticky never restored"
    );
    let _ = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            vec![fact("win-a", "out-a", 0), sticky_s.clone()],
            "win-a",
            Some(13),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            6,
            two_screens(),
            vec![fact("win-a", "out-a", 0), sticky_s.clone()],
            "",
            Some(14),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back).last(),
        Some(&WindowId("win-a".to_owned())),
        "return after close uses the tiled fallback, not the stale exception"
    );
}

#[test]
fn sticky_visibility_switch_mints_no_admission_and_no_spare_memory() {
    // An already-known sticky follows the current view (no members-map
    // homing), so switching to a never-focused empty spare stages it
    // there. That move is not a birth: with unchanged native focus and
    // no command, the observation must mint no admission focus and
    // record no canonical memory in the spare. A detour observation
    // stays quiet, and the return names the remembered tile.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut sticky_s = fact("win-s", "out-a", 200);
    sticky_s.sticky = true;
    let windows = || vec![fact("win-a", "out-a", 0), sticky_s.clone()];
    let first = accepted(
        &mut engine,
        &report(1, two_screens(), windows(), "", None, None),
    );
    let home = home_of(&engine, &WindowId("win-a".to_owned())).expect("win-a linked");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| **id != home.workspace.0)
        .expect("second workspace")
        .clone();
    let spare = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(other.clone()),
    };
    // Settle tile focus once; the sticky is visible throughout but never
    // intentionally focused.
    let _ = accepted(
        &mut engine,
        &report(2, two_screens(), windows(), "win-a", None, None),
    );
    assert_eq!(
        engine.canonical_memory(&home),
        Some(WindowId("win-a".to_owned())),
        "precondition: tile recorded at home"
    );
    // Switch to the never-focused empty spare.
    let _ = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            windows(),
            "win-a",
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    // Next observation: unchanged native focus, no command. The sticky
    // is merely visible on the spare now.
    let obs = accepted(
        &mut engine,
        &report(4, two_screens(), windows(), "win-a", None, None),
    );
    assert!(
        focus_targets(&obs).is_empty(),
        "visible sticky mints no admission focus, got {:?}",
        obs.effects
    );
    assert_eq!(
        engine.canonical_memory(&spare),
        None,
        "spare keeps no memory for a never-focused sticky"
    );
    // Detour observation stays quiet as well.
    let obs2 = accepted(
        &mut engine,
        &report(5, two_screens(), windows(), "win-a", None, None),
    );
    assert!(
        focus_targets(&obs2).is_empty(),
        "detour observation stays quiet, got {:?}",
        obs2.effects
    );
    assert_eq!(
        engine.canonical_memory(&spare),
        None,
        "detour keeps the spare slot absent"
    );
    // Return home: the remembered tile answers, never the sticky.
    let back = accepted(
        &mut engine,
        &report(
            6,
            two_screens(),
            windows(),
            "",
            Some(12),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: home.workspace.0.clone(),
            }),
        ),
    );
    assert_eq!(
        focus_targets(&back),
        vec![WindowId("win-a".to_owned())],
        "return names the remembered tile, never the visible sticky"
    );
    assert_eq!(
        engine.canonical_memory(&spare),
        None,
        "return leaves the spare slot absent"
    );
}
