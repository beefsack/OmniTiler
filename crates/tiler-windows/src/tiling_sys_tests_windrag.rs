// Deterministic gaps (need a live desktop, verified later): the positive
// press actuation itself, the suspend/elevation precheck, and the
// already-foreground skip all read/actuate the real foreground.
use super::{clear_windrag_gesture, clear_windrag_gesture_scoped, windrag_down, windrag_up};
use crate::model::ProcessIdentity;
use crate::win_mouse::{WinDragEdge, WinDragKind, WinDragSnapshot};

const HWND_A: u64 = 0xA001;
const HWND_B: u64 = 0xB002;
const DEAD_HWND: u64 = 0x00BEEF42;

fn fake_me() -> ProcessIdentity {
    ProcessIdentity {
        pid: 4242,
        process_creation: "create-me".to_owned(),
        user_sid: "S-1-5-21-me".to_owned(),
        session_id: 1,
        exe_path: "C:\\bin\\tiler-windows.exe".to_owned(),
    }
}

fn origin(hwnd: u64, token: &str) -> crate::snapkey::SnapOrigin {
    crate::snapkey::SnapOrigin {
        hwnd,
        token: token.to_owned(),
        pid: 100,
        creation: "create-1".to_owned(),
    }
}

fn member_key(hwnd: u64) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid: 100,
        creation: "create-1".to_owned(),
    }
}

fn down_edge(hwnd: u64, token: &str, tag: Option<&str>) -> WinDragEdge {
    WinDragEdge {
        hwnd,
        kind: WinDragKind::Down,
        x: 10,
        y: 10,
        esc_seq: 1,
        snapshot: Some(WinDragSnapshot {
            origin: origin(hwnd, token),
            tag: tag.map(str::to_owned),
        }),
    }
}

fn publish_tiled(state: &mut super::TileLoop, hwnd: u64, token: &str, tag: Option<&str>) {
    state.managed.insert(hwnd);
    state.windrag_origins.insert(
        hwnd,
        crate::win_mouse::sys::WinDragPublished {
            origin: origin(hwnd, token),
            tag: tag.map(str::to_owned),
        },
    );
    state
        .member_tokens
        .insert(member_key(hwnd), token.to_owned());
    state.stable.insert(
        hwnd,
        tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        },
    );
}

fn assert_no_arm(state: &super::TileLoop, hwnd: u64) {
    assert!(!state.active.contains(&hwnd), "no active capture");
    assert!(!state.gesture_producer.contains_key(&hwnd), "no producer");
    assert!(!state.windrag_bound.contains_key(&hwnd), "no bound");
    assert!(
        !state.windrag_start_cursor.contains_key(&hwnd),
        "no start cursor"
    );
    assert!(!state.move_kind.contains_key(&hwnd), "no move arm");
}

fn scratch_log(state: &mut super::TileLoop, tag: &str) {
    // Preview logging must never touch the repo tree.
    state.log_path = std::env::temp_dir().join(format!(
        "tiler-windrag-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
}

fn arm_pair(state: &mut super::TileLoop) {
    // Two fully armed windrag gestures with owned preview bindings.
    let rect = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    };
    for (hwnd, token) in [(HWND_A, "w1"), (HWND_B, "w2")] {
        publish_tiled(state, hwnd, token, Some("tag-1"));
        state.active.insert(hwnd);
        state.gesture_before.insert(hwnd, rect);
        state.gesture_esc_seq.insert(hwnd, 1);
        state.gesture_start_key.insert(hwnd, member_key(hwnd));
        state
            .gesture_start_tag
            .insert(hwnd, Some("tag-1".to_owned()));
        state.gesture_preview_start.insert(
            hwnd,
            super::PreviewStartBinding {
                token: token.to_owned(),
                output: "mon-9".to_owned(),
                workspace: "ws-9".to_owned(),
                revision: 0,
            },
        );
        state.windrag_start_cursor.insert(hwnd, (10, 10));
        state.windrag_bound.insert(
            hwnd,
            WinDragSnapshot {
                origin: origin(hwnd, token),
                tag: Some("tag-1".to_owned()),
            },
        );
        state
            .move_kind
            .insert(hwnd, crate::group_underlay::MoveSizeKind::Move);
        state.gesture_producer.insert(hwnd, "windrag");
        state
            .preview_bound
            .insert(hwnd, super::PreviewBound { hover_prior: None });
    }
    state.member_rects.insert("w1".to_owned(), rect);
}

fn assert_no_maps(state: &super::TileLoop, hwnd: u64) {
    for map_has in [
        state.active.contains(&hwnd),
        state.gesture_producer.contains_key(&hwnd),
        state.windrag_bound.contains_key(&hwnd),
        state.windrag_start_cursor.contains_key(&hwnd),
        state.move_kind.contains_key(&hwnd),
        state.gesture_before.contains_key(&hwnd),
        state.gesture_end_cursor.contains_key(&hwnd),
        state.gesture_start_key.contains_key(&hwnd),
        state.gesture_start_tag.contains_key(&hwnd),
        state.gesture_esc_seq.contains_key(&hwnd),
        state.gesture_end_seq.contains_key(&hwnd),
        state.esc_latched.contains(&hwnd),
        state.preview_bound.contains_key(&hwnd),
        state.gesture_preview_start.contains_key(&hwnd),
    ] {
        assert!(!map_has, "every arm/preview map cleared");
    }
}

#[test]
fn down_refuses_before_any_probe_with_no_arm() {
    // Every pre-probe fence refuses the production `windrag_down` with
    // its exact reason and arms nothing (no active capture, producer,
    // bound, cursor, or move arm). No native call runs on the pre-probe
    // paths; the closing identity case performs one bounded
    // thread/pid read of a dead HWND. (Tag/creation/token drift past
    // the probe is pinned portably by `validate_windrag_down`.)
    let me = fake_me();
    // No snapshot at all.
    let mut state = super::rmax03_adapter_tests::test_state();
    let edge = WinDragEdge {
        hwnd: HWND_A,
        kind: WinDragKind::Down,
        x: 10,
        y: 10,
        esc_seq: 1,
        snapshot: None,
    };
    assert_eq!(
        windrag_down(&mut state, &me, &edge, &[], &[], &[]),
        Err("no-snapshot")
    );
    assert_no_arm(&state, HWND_A);
    // Known snapshot but unmanaged subject.
    let mut state = super::rmax03_adapter_tests::test_state();
    assert_eq!(
        windrag_down(
            &mut state,
            &me,
            &down_edge(HWND_A, "w1", Some("tag-1")),
            &[],
            &[],
            &[]
        ),
        Err("unmanaged")
    );
    assert_no_arm(&state, HWND_A);
    // Managed but never published (floating/sticky/unknown pass through
    // hook-side; the owner still refuses).
    let mut state = super::rmax03_adapter_tests::test_state();
    state.managed.insert(HWND_A);
    assert_eq!(
        windrag_down(
            &mut state,
            &me,
            &down_edge(HWND_A, "w1", Some("tag-1")),
            &[],
            &[],
            &[]
        ),
        Err("unknown-subject")
    );
    assert_no_arm(&state, HWND_A);
    // Busy: an open gesture, a held PRE frame, or a stored bound each
    // refuses without rebinding.
    for seed in ["active", "before", "bound"] {
        let mut state = super::rmax03_adapter_tests::test_state();
        publish_tiled(&mut state, HWND_A, "w1", Some("tag-1"));
        match seed {
            "active" => {
                state.active.insert(HWND_A);
            }
            "before" => {
                state.gesture_before.insert(
                    HWND_A,
                    tiler_core::geometry::Rect {
                        x: 0,
                        y: 0,
                        w: 400,
                        h: 300,
                    },
                );
            }
            _ => {
                state.windrag_bound.insert(
                    HWND_A,
                    WinDragSnapshot {
                        origin: origin(HWND_A, "w1"),
                        tag: Some("tag-1".to_owned()),
                    },
                );
            }
        }
        assert_eq!(
            windrag_down(
                &mut state,
                &me,
                &down_edge(HWND_A, "w1", Some("tag-1")),
                &[],
                &[],
                &[]
            ),
            Err("busy"),
            "seed {seed} refuses"
        );
        assert!(
            !state.gesture_producer.contains_key(&HWND_A),
            "seed {seed} never arms a producer"
        );
    }
    // Live-probe gate through the production path: a published member
    // whose HWND no longer resolves refuses `identity-changed` with no
    // arm. One bounded thread/pid read of a dead HWND: no setter, no
    // plan, no geometry.
    let mut state = super::rmax03_adapter_tests::test_state();
    publish_tiled(&mut state, DEAD_HWND, "w1", Some("tag-1"));
    assert_eq!(
        windrag_down(
            &mut state,
            &me,
            &down_edge(DEAD_HWND, "w1", Some("tag-1")),
            &[],
            &[],
            &[]
        ),
        Err("identity-changed")
    );
    assert_no_arm(&state, DEAD_HWND);
}

#[test]
fn press_failure_clear_keeps_neighbour_preview_and_maps() {
    // B has only just armed at press, before acquiring a preview binding.
    // Its failed focus must not invoke hide_preview on A's preview.
    let mut state = super::rmax03_adapter_tests::test_state();
    scratch_log(&mut state, "press-own");
    arm_pair(&mut state);
    state.preview_bound.remove(&HWND_B);
    state.preview_last = Some("neighbour-preview".to_owned());
    assert_eq!(
        clear_windrag_gesture_scoped(&mut state, HWND_B, false),
        Some("w2".to_owned())
    );
    assert_no_maps(&state, HWND_B);
    assert!(state.active.contains(&HWND_A));
    assert!(state.windrag_bound.contains_key(&HWND_A));
    assert!(state.gesture_producer.contains_key(&HWND_A));
    assert!(state.preview_bound.contains_key(&HWND_A));
    assert!(state.gesture_preview_start.contains_key(&HWND_A));
    assert_eq!(state.preview_last.as_deref(), Some("neighbour-preview"));
    // B's journey is dead: a later Up settles nothing.
    windrag_up(&mut state, HWND_B, 50, 60, 2);
    assert!(!state.gesture_end_cursor.contains_key(&HWND_B));
}

#[test]
fn cancel_clears_arm_preview_and_leaves_source_stationary() {
    // Owner-side Cancel (hook disarmed, paired Up swallowed hook-side):
    // the legacy clear drops every gesture map for the HWND with no
    // plan and no geometry change. Stable source and member rects are
    // identical after, and a later Up settles nothing.
    let mut state = super::rmax03_adapter_tests::test_state();
    scratch_log(&mut state, "cancel");
    arm_pair(&mut state);
    let stable_before = state.stable.clone();
    let rects_before = state.member_rects.clone();
    assert_eq!(
        clear_windrag_gesture(&mut state, HWND_A),
        Some("w1".to_owned())
    );
    assert_no_maps(&state, HWND_A);
    assert_eq!(state.stable, stable_before, "source stays stationary");
    assert_eq!(state.member_rects, rects_before, "no geometry writes");
    windrag_up(&mut state, HWND_A, 50, 60, 2);
    assert!(!state.gesture_end_cursor.contains_key(&HWND_A));
    assert_eq!(clear_windrag_gesture(&mut state, 0xDEAD), None);
}

#[test]
fn same_output_fence_keeps_cross_output_release() {
    // Same-output gate through the production predicate: inside the
    // source domain may proceed to the Engine drop; taskbar/outside or
    // cross-output points refuse with no transfer.
    assert!(crate::tiling::drop_point_in_domain(
        0, 0, 1920, 1040, 100, 100
    ));
    assert!(crate::tiling::drop_point_in_domain(
        0, 0, 1920, 1040, 1919, 1039
    ));
    assert!(!crate::tiling::drop_point_in_domain(
        0, 0, 1920, 1040, 100, 1100
    ));
    assert!(!crate::tiling::drop_point_in_domain(
        0, 0, 1920, 1040, 2000, 100
    ));
    assert!(!crate::tiling::drop_point_in_domain(
        0, 0, 1920, 1040, -10, 100
    ));
}

#[test]
fn shared_drop_contract_still_lands_on_preview() {
    // Drop contract press-focus leaves unchanged: preview a window-edge
    // target, carry the exact hover prior into the drop, and the mover
    // lands exactly on the preview rect (R-DRAG-07 split reads from
    // this agreement). Real Engine drop through the production command
    // shapes with `source: None`.
    use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
    use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
    use tiler_core::engine::Engine;
    use tiler_core::geometry::Rect;
    use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
    use tiler_core::session::OutputDomain;

    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("windrag-press").expect("generation");
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let domain = OutputDomain {
        id: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds,
        gap: 8,
        adjacent: std::collections::BTreeMap::new(),
    };
    let window = |token: &str| tiler_core::seed::EngineWindow {
        window: WindowId(token.to_owned()),
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: bounds,
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    };
    let event =
        |command: CoreCommand, correlation: &str, windows: Vec<tiler_core::seed::EngineWindow>| {
            CoreEvent {
                owner: owner.clone(),
                generation: generation.clone(),
                correlation: CorrelationId::parse(correlation).expect("correlation"),
                revision: 0,
                fingerprint: 7,
                domain: domain.clone(),
                domain_key: domain.key(),
                outer_gap: 8,
                focused_window: WindowId("w1".to_owned()),
                windows,
                directional: None,
                directional_target_outer_gap: None,
                target_domain: None,
                target_windows: Vec::new(),
                closed_windows: Vec::new(),
                command,
            }
        };
    let rows = || vec![window("w1"), window("w2")];
    let mut engine = Engine::new();
    let geometry = match engine.handle(&event(CoreCommand::Reconcile, "corr-press-seed", rows())) {
        CoreReply::Projection(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect::<Vec<_>>(),
        CoreReply::Tiled(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect::<Vec<_>>(),
        other => panic!("seed must converge, got {other:?}"),
    };
    let target = geometry
        .iter()
        .find(|(token, _)| token == "w2")
        .map(|(_, rect)| *rect)
        .expect("sibling geometry");
    let at = (target.x + 2, target.y + target.h / 2);
    let (proposed, prior) = match engine.handle(&event(
        CoreCommand::DragPreview {
            window: "w1".to_owned(),
            x: at.0,
            y: at.1,
            hover_prior: None,
            source: None,
        },
        "corr-press-preview",
        rows(),
    )) {
        CoreReply::DragPreview(plan) => (plan.preview.proposed_rect, plan.preview.hover_prior()),
        other => panic!("edge preview must resolve, got {other:?}"),
    };
    match engine.handle(&event(
        CoreCommand::DragDrop {
            window: "w1".to_owned(),
            x: at.0,
            y: at.1,
            hover_prior: Some(prior),
            source: None,
        },
        "corr-press-drop",
        rows(),
    )) {
        CoreReply::Tiled(plan) => {
            let placed = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w1")
                .expect("mover placement");
            assert_eq!(placed.rect, proposed, "drop must equal preview");
        }
        other => panic!("carried drop must plan, got {other:?}"),
    }
}
