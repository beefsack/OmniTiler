#![forbid(unsafe_code)]
use super::rmax03_adapter_tests::test_state;
use super::{
    LaunchBackend, SystemLaunchBackend, dispatch_launch_intent, dispatch_workspace_tiling_intent,
    keyboard_tick,
};
use crate::snapkey::{
    LaunchKind, QueuedLaunchIntent, QueuedSnapEvent, QueuedWorkspaceTilingIntent, SnapEdge,
};

/// Mocked spawn backend: records requested kinds, never touches the OS.
/// No test in this module launches a real browser or terminal.
struct Recorder {
    kinds: std::cell::RefCell<Vec<LaunchKind>>,
    fail: bool,
}

impl Recorder {
    fn ok() -> Self {
        Self {
            kinds: std::cell::RefCell::new(Vec::new()),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            kinds: std::cell::RefCell::new(Vec::new()),
            fail: true,
        }
    }
}

impl LaunchBackend for Recorder {
    fn launch(&self, kind: LaunchKind) -> std::result::Result<(), String> {
        self.kinds.borrow_mut().push(kind);
        if self.fail {
            return Err("mock spawn failed".to_owned());
        }
        Ok(())
    }
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

fn launch_intent(kind: LaunchKind) -> QueuedLaunchIntent {
    QueuedLaunchIntent {
        kind,
        edge: SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    }
}

#[test]
fn launch_dispatch_spawns_each_kind_through_backend() {
    // Down-only dispatch: browser and terminal downs each spawn exactly
    // once through the injected backend; the chord-time origin is ignored
    // (launchers act on no window). The fence is injected clear, so the
    // test never depends on ambient foreground state (headless runners
    // report an unopenable foreground process, which the live fence
    // would refuse).
    let mut state = test_state();
    state.keyboard.takeover = true;
    let backend = Recorder::ok();
    let tick = state.tick;
    dispatch_launch_intent(
        &mut state,
        launch_intent(LaunchKind::Browser),
        &backend,
        None,
    );
    dispatch_launch_intent(
        &mut state,
        launch_intent(LaunchKind::Terminal),
        &backend,
        None,
    );
    assert_eq!(
        *backend.kinds.borrow(),
        vec![LaunchKind::Browser, LaunchKind::Terminal]
    );
    assert_eq!(state.tick, tick + 2);
}

#[test]
fn launch_dispatch_ignores_ups_repeats_and_passed_chords() {
    // Key-ups, repeats, and passed chords never spawn and never advance
    // the tick.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let backend = Recorder::ok();
    let tick = state.tick;
    for edge in [SnapEdge::Up, SnapEdge::Repeat] {
        dispatch_launch_intent(
            &mut state,
            QueuedLaunchIntent {
                kind: LaunchKind::Browser,
                edge,
                origin: None,
                consumed: true,
                announce: false,
                tick: std::time::Instant::now(),
            },
            &backend,
            None,
        );
    }
    dispatch_launch_intent(
        &mut state,
        QueuedLaunchIntent {
            kind: LaunchKind::Terminal,
            edge: SnapEdge::Down,
            origin: None,
            consumed: false,
            announce: false,
            tick: std::time::Instant::now(),
        },
        &backend,
        None,
    );
    assert!(backend.kinds.borrow().is_empty());
    assert_eq!(state.tick, tick);
}

#[test]
fn launch_dispatch_refuses_without_takeover() {
    // `test_state` ships with takeover off: the down settles without
    // spawning, mirroring the classifier's pass-through.
    let mut state = test_state();
    assert!(!state.keyboard.takeover);
    let backend = Recorder::ok();
    dispatch_launch_intent(
        &mut state,
        launch_intent(LaunchKind::Browser),
        &backend,
        None,
    );
    assert!(backend.kinds.borrow().is_empty());
}

#[test]
fn launch_dispatch_refuses_on_injected_fence() {
    // A blocked suspend/elevated fence settles without spawning: the
    // injected outcome pins the refusal deterministically instead of
    // reading ambient foreground state.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let backend = Recorder::ok();
    let tick = state.tick;
    for gate in [Some("suspended"), Some("elevated-foreground")] {
        dispatch_launch_intent(
            &mut state,
            launch_intent(LaunchKind::Browser),
            &backend,
            gate,
        );
    }
    assert!(backend.kinds.borrow().is_empty());
    assert_eq!(state.tick, tick + 2);
}

#[test]
fn launch_backend_failure_keeps_loop_functioning() {
    // A failed spawn (e.g. missing wt.exe) settles narrowly: the tick
    // still advances and the loop keeps functioning with no crash and no
    // fallback launch.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let backend = Recorder::failing();
    let tick = state.tick;
    dispatch_launch_intent(
        &mut state,
        launch_intent(LaunchKind::Terminal),
        &backend,
        None,
    );
    assert_eq!(*backend.kinds.borrow(), vec![LaunchKind::Terminal]);
    assert_eq!(state.tick, tick + 1);
}

#[test]
fn keyboard_tick_routes_launch_without_spawning_for_settled_events() {
    // The `keyboard_tick` launch arm routes through the production
    // dispatch; a settled (non-announced) event returns before touching
    // the production backend, so this exercises the routing arm with no
    // real spawn.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let me = fake_me();
    let tick = state.tick;
    keyboard_tick(
        &mut state,
        &me,
        &[],
        &[],
        vec![QueuedSnapEvent::Launch(QueuedLaunchIntent {
            kind: LaunchKind::Browser,
            edge: SnapEdge::Up,
            origin: None,
            consumed: true,
            announce: false,
            tick: std::time::Instant::now(),
        })],
        None,
    );
    assert_eq!(state.tick, tick);
    // The production backend selects one spawn path per kind without
    // sharing handles or elevation; assert the selection contract, not
    // the spawn itself (never launch here).
    let _ = SystemLaunchBackend;
    assert_eq!(LaunchKind::Browser.as_str(), "browser");
    assert_eq!(LaunchKind::Terminal.as_str(), "terminal");
}

fn tiling_intent() -> QueuedWorkspaceTilingIntent {
    QueuedWorkspaceTilingIntent {
        edge: SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    }
}

#[test]
fn workspace_tiling_dispatch_ignores_ups_repeats_and_passed_chords() {
    // Key-ups, repeats, and passed chords never toggle and never advance
    // the tick. The chord-time origin is ignored either way: the toggle
    // acts on the current workspace, never a focused window.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let tick = state.tick;
    for edge in [SnapEdge::Up, SnapEdge::Repeat] {
        dispatch_workspace_tiling_intent(
            &mut state,
            &fake_me(),
            &[],
            &[],
            QueuedWorkspaceTilingIntent {
                edge,
                origin: None,
                consumed: true,
                announce: false,
                tick: std::time::Instant::now(),
            },
            None,
        );
    }
    dispatch_workspace_tiling_intent(
        &mut state,
        &fake_me(),
        &[],
        &[],
        QueuedWorkspaceTilingIntent {
            edge: SnapEdge::Down,
            origin: None,
            consumed: false,
            announce: false,
            tick: std::time::Instant::now(),
        },
        None,
    );
    assert_eq!(state.tick, tick);
}

#[test]
fn workspace_tiling_dispatch_refuses_without_takeover() {
    // `test_state` ships with takeover off: the down settles without
    // toggling, mirroring the classifier's pass-through.
    let mut state = test_state();
    assert!(!state.keyboard.takeover);
    let tick = state.tick;
    dispatch_workspace_tiling_intent(&mut state, &fake_me(), &[], &[], tiling_intent(), None);
    assert_eq!(state.tick, tick + 1);
}

#[test]
fn workspace_tiling_dispatch_refuses_on_injected_fence() {
    // A blocked suspend/elevated fence settles without toggling: the
    // injected outcome pins the refusal deterministically instead of
    // reading ambient foreground state.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let tick = state.tick;
    for gate in [Some("suspended"), Some("elevated-foreground")] {
        dispatch_workspace_tiling_intent(&mut state, &fake_me(), &[], &[], tiling_intent(), gate);
    }
    assert_eq!(state.tick, tick + 2);
}

#[test]
fn workspace_tiling_dispatch_refuses_without_scope() {
    // `test_state` ships with no active output: with takeover on and a
    // clear fence the down still settles with no side effects (never a
    // guessed fallback), before any observation or Engine work.
    let mut state = test_state();
    state.keyboard.takeover = true;
    assert!(state.active_output.is_empty());
    let tick = state.tick;
    dispatch_workspace_tiling_intent(&mut state, &fake_me(), &[], &[], tiling_intent(), None);
    assert_eq!(state.tick, tick + 1);
    assert!(state.active_output.is_empty());
}

#[test]
fn keyboard_tick_routes_workspace_tiling_without_toggling_for_settled_events() {
    // The `keyboard_tick` workspace-tiling arm routes through the
    // production dispatch; a settled (non-announced) event returns before
    // touching the toggle path, so this exercises the routing arm with no
    // observation, no Engine work, and no ambient foreground reads.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let me = fake_me();
    let tick = state.tick;
    keyboard_tick(
        &mut state,
        &me,
        &[],
        &[],
        vec![QueuedSnapEvent::WorkspaceTiling(
            QueuedWorkspaceTilingIntent {
                edge: SnapEdge::Up,
                origin: None,
                consumed: true,
                announce: false,
                tick: std::time::Instant::now(),
            },
        )],
        None,
    );
    assert_eq!(state.tick, tick);
}
