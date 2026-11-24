//! Cross-output send and R4 exhaustion: refusal preservation, overlap admit, no D2 redirect, focus/resize, drag agreement.
//!
//! Shared helpers live in the parent (`capacity_admission_tests.rs`); this module
//! holds only `#[test]` fns.

use super::*;
use crate::directional::{Direction, SameAxisMove};
use crate::world_layout::{PairMoveOutcome, PairOperationContext, PairOperationFailure};

#[test]
fn send_refused_when_target_unrepresentable_source_preserved() {
    use crate::size_hints::WindowSizeHints;
    use crate::world_engine::{WorldEngine, WorldOutcome};
    use crate::world_report::{
        ActivationPolicy, CommandRejectKind, CommandResult, NativeWorkspaceFact, OutputIdentity,
        PresentationIngressV4, PresentationRowV4, ScreenReport, VisibilityFact,
        WORLD_CONTRACT_VERSION_V4, WindowFact, WindowType, WorldCommand, WorldReportV4,
    };

    let make_screen = |output: &str, w: i32, h: i32| ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(Rect { x: 0, y: 0, w, h }),
        work_area: Some(Rect { x: 0, y: 0, w, h }),
        complete: true,
        scale: 1.0,
        identity: OutputIdentity {
            manufacturer: Some("SynthMfg".to_owned()),
            model: Some(format!("SynthModel-{output}")),
            serial: Some(format!("serial-{output}")),
            connector: None,
        },
    };
    let make_fact = |window: &str, output: &str, rect: Rect| WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(rect),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: NativeWorkspaceFact::Omitted,
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
        window_type: Some(WindowType::Normal),
        activation_policy: Some(ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
    };
    let make_report = |seq: u64,
                       screens: Vec<ScreenReport>,
                       windows: Vec<WindowFact>,
                       focused: &str,
                       cmd_seq: Option<u64>,
                       command: Option<WorldCommand>| WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: cmd_seq,
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
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        visibility_attempts: Vec::new(),
        focus_changes: None,
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
        pointer_activity: None,
        warp_spent: None,
    };

    // Source ws-1 roomy with a, b (b focused); the named target domain
    // does not exist. The send refuses SendTargetUnknown with the source
    // exactly preserved: same tree, same focus, same revision, mover
    // still tiled at home.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let screens = vec![make_screen("o", 120, 80)];
    let windows = vec![
        make_fact(
            "a",
            "o",
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 40,
            },
        ),
        make_fact(
            "b",
            "o",
            Rect {
                x: 0,
                y: 40,
                w: 120,
                h: 40,
            },
        ),
    ];
    let (outcome, _) = engine.handle_report(
        &make_report(1, screens.clone(), windows.clone(), "b", None, None),
        &[],
    );
    assert!(matches!(outcome, WorldOutcome::Accepted(_)));
    let key1 = engine
        .world
        .layout
        .domain_keys()
        .into_iter()
        .next()
        .expect("seeded domain");
    let seq_b = engine
        .world
        .layout
        .admission_sequence(&key1, &WindowId("b".to_owned()));
    let (tree_before, focus_before, rev_before) = (
        engine.world.layout.tree(&key1).cloned(),
        engine.world.layout.focus_leaf(&key1).cloned(),
        engine.world.layout.accepted_revision(&key1),
    );
    let (outcome, settlements) = engine.handle_report(
        &make_report(
            2,
            screens,
            windows,
            "b",
            Some(1),
            Some(WorldCommand::Send {
                window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                    "b".to_owned(),
                )),
                target_output: crate::directional::OutputId("o".to_owned()),
                target_workspace: crate::directional::WorkspaceId("ws-9".to_owned()),
                follow: true,
            }),
        ),
        &[],
    );
    assert!(matches!(outcome, WorldOutcome::Accepted(_)));
    assert_eq!(
        settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::SendTargetUnknown
        }),
        "honest refuse for an unknown/unrepresentable target"
    );
    assert_eq!(
        engine.world.layout.tree(&key1),
        tree_before.as_ref(),
        "source preserved"
    );
    assert_eq!(
        engine.world.layout.focus_leaf(&key1),
        focus_before.as_ref(),
        "focus preserved"
    );
    assert_eq!(engine.world.layout.accepted_revision(&key1), rev_before);
    assert_eq!(tiled_windows(&engine.world.layout), vec!["a", "b"]);
    assert_eq!(
        engine
            .world
            .layout
            .admission_sequence(&key1, &WindowId("b".to_owned())),
        seq_b,
        "refusal never re-sequences"
    );
}

#[test]
fn r4_exhausted_floats_at_chosen_output_with_no_d2_redirect() {
    // Adjacent outputs: source out-1/ws sole mover m, target out-2/ws 1x1
    // with t. The exact wrap plans first, proves geometric exhaustion,
    // and capacity-floats the mover at the CHOSEN output (CapacityFloat
    // under the explicit output-send intent, follow-only, untouched
    // frame, no D2 redirect).
    let mut d1 = domain("out-1", "ws", 120, 80, 0);
    let mut d2 = domain("out-2", "ws", 1, 1, 0);
    d1.adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    d2.adjacent
        .insert(Direction::Left, OutputId("out-1".to_owned()));
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &d1, "m", placement(120, 40));
    admit_into(&mut layout, &d2, "t", placement(1, 1));
    let src = d1.key();
    let target_key = d2.key();
    let dest_tree_before = layout.tree(&target_key).expect("target tree").clone();
    let seq_m = layout.admission_sequence(&src, &WindowId("m".to_owned()));
    assert!(layout.sync_focus_from_window(&src, &WindowId("m".to_owned())));
    let mover_leaf = leaf_of(&layout, &src, "m");
    let t_leaf = leaf_of(&layout, &target_key, "t");
    let obs = complete_engine_obs(&layout, &[&src, &target_key]);
    let ctx = PairOperationContext {
        source_domain: &d1,
        source_outer_gap: 0,
        target_domain: &d2,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("m".to_owned()),
        observed: &obs,
        closed: &[],
    };
    let outcome = layout
        .run_directional_pair_move(
            &ctx,
            &WindowId("m".to_owned()),
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("float R4 stages the capacity fallback");
    match outcome {
        PairMoveOutcome::CapacityFloat {
            window,
            leaf,
            source_output,
            source_workspace,
            target_output,
            target_workspace,
            follow,
            geometry,
            ..
        } => {
            assert_eq!(window.0, "m");
            assert_eq!(leaf, mover_leaf, "dropped leaf named for audit");
            assert_eq!(source_output.0, "out-1");
            assert_eq!(source_workspace.0, "ws");
            assert_eq!(target_output.0, "out-2", "chosen output kept");
            assert_eq!(target_workspace.0, "ws");
            assert!(follow, "R4 always follows the mover");
            assert!(
                geometry.iter().all(|g| g.window.0 != "m"),
                "slotless mover takes no tiled geometry: no native frame move"
            );
            assert!(
                geometry.iter().any(|g| g.window.0 == "t"),
                "survivor still projects"
            );
        }
        other => panic!("float R4 carries CapacityFloat, got {other:?}"),
    }
    assert_eq!(tiled_windows(&layout), vec!["t".to_string()]);
    let flags = layout
        .exception_flags(&target_key, &WindowId("m".to_owned()))
        .expect("mover floated at target");
    assert!(flags.floating && !flags.fullscreen && !flags.maximized && !flags.sticky);
    assert!(layout.is_capacity_float(&target_key, &WindowId("m".to_owned())));
    assert!(
        !layout.is_automatic_fixed_float(&target_key, &WindowId("m".to_owned())),
        "capacity origin stays distinct from fixed auto"
    );
    assert!(
        layout
            .exception_float_geometry(&target_key, &WindowId("m".to_owned()))
            .is_none(),
        "cross-output stages no source-coordinate claim: the frame \
         stays untouched for the native remap plus arrival proof"
    );
    assert_eq!(
        layout.tree(&target_key),
        Some(&dest_tree_before),
        "destination tree untouched: no tile slot taken"
    );
    assert_eq!(
        layout.admission_sequence(&target_key, &WindowId("m".to_owned())),
        seq_m,
        "moves never re-sequence"
    );
    assert_eq!(
        layout.focus_leaf(&target_key),
        Some(&t_leaf),
        "committed follow stays on the target bookkeeping"
    );
}

#[test]
fn r4_float_never_redirects_to_a_roomy_third_output() {
    // A third output with room does not divert the fallback: the float
    // homes at the R4-chosen output, never tiles elsewhere (no D2 even
    // when another leaf could fit).
    let mut d1 = domain("out-1", "ws", 120, 80, 0);
    let mut d2 = domain("out-2", "ws", 1, 1, 0);
    let d3 = domain("out-3", "ws", 120, 80, 0);
    d1.adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    d2.adjacent
        .insert(Direction::Left, OutputId("out-1".to_owned()));
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &d1, "m", placement(120, 40));
    admit_into(&mut layout, &d2, "t", placement(1, 1));
    let src = d1.key();
    let target_key = d2.key();
    let third = d3.key();
    assert!(layout.sync_focus_from_window(&src, &WindowId("m".to_owned())));
    let obs = complete_engine_obs(&layout, &[&src, &target_key]);
    let ctx = PairOperationContext {
        source_domain: &d1,
        source_outer_gap: 0,
        target_domain: &d2,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("m".to_owned()),
        observed: &obs,
        closed: &[],
    };
    let outcome = layout
        .run_directional_pair_move(
            &ctx,
            &WindowId("m".to_owned()),
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("float R4 stages the capacity fallback");
    match outcome {
        PairMoveOutcome::CapacityFloat {
            target_output,
            target_workspace,
            ..
        } => {
            assert_eq!(target_output.0, "out-2");
            assert_eq!(target_workspace.0, "ws");
        }
        other => panic!("float R4 carries CapacityFloat, got {other:?}"),
    }
    assert!(
        layout.is_exception(&target_key, &WindowId("m".to_owned())),
        "never redirected to the roomy third output"
    );
    assert_eq!(layout.tree(&third), None, "third output untouched");
    assert_eq!(tiled_windows(&layout), vec!["t".to_string()]);
}

#[test]
fn r4_unrepresentable_source_refuses_without_mutation() {
    // When a cross-output move context fails prechecks (e.g. mismatched
    // source bounds), the operation refuses truthfully with the source
    // preserved instead of staging a float (no fake success).
    let mut d1 = domain("out-1", "ws", 120, 80, 0);
    let mut d2 = domain("out-2", "ws", 1, 1, 0);
    d1.adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    d2.adjacent
        .insert(Direction::Left, OutputId("out-1".to_owned()));
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &d1, "a", placement(120, 40));
    admit_into(&mut layout, &d1, "m", placement(120, 40));
    admit_into(&mut layout, &d2, "t", placement(1, 1));
    let src = d1.key();
    let target_key = d2.key();
    assert!(layout.sync_focus_from_window(&src, &WindowId("m".to_owned())));
    let mover_leaf = leaf_of(&layout, &src, "m");
    let (tree_before, focus_before, rev_before) = (
        layout.tree(&src).cloned(),
        layout.focus_leaf(&src).cloned(),
        layout.accepted_revision(&src),
    );
    let mut bad_d1 = d1.clone();
    bad_d1.bounds = Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };
    let obs = complete_engine_obs(&layout, &[&src, &target_key]);
    let ctx = PairOperationContext {
        source_domain: &bad_d1,
        source_outer_gap: 0,
        target_domain: &d2,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("m".to_owned()),
        observed: &obs,
        closed: &[],
    };
    assert!(
        layout
            .run_directional_pair_move(
                &ctx,
                &WindowId("m".to_owned()),
                Direction::Right,
                true,
                SameAxisMove::GroupWithNeighbor,
            )
            .is_err(),
        "invalid source context refuses instead of floating blindly"
    );
    assert_eq!(layout.tree(&src), tree_before.as_ref(), "source preserved");
    assert_eq!(layout.focus_leaf(&src), focus_before.as_ref());
    assert_eq!(layout.accepted_revision(&src), rev_before);
    assert_eq!(
        layout.focus_leaf(&src),
        Some(&mover_leaf),
        "refusal preserves the mover focus"
    );
    assert!(!layout.is_capacity_float_any(&WindowId("m".to_owned())));
}

// Retired test: `r4_float_withdrawn_before_ack_preserves_source`
// Tested `Session::cancel_unacked_pending` on the deleted two-phase
// `propose`/`acknowledge` state machine; `WorldLayout` executes synchronously
// with candidate atomicity (covered by refusal preservation tests above).

#[test]
fn drag_preview_and_finish_agree_on_exhausted_cross_output() {
    // Source roomy with a, b; destination 1x1 with t. A cross-output drag
    // of b to the exhausted destination refuses the preview and
    // snap-backs or refuses the finish identically: same gate, source
    // preserved, no faked float.
    let mut d1 = domain("out-1", "ws", 120, 80, 0);
    let mut d2 = domain("out-2", "ws", 1, 1, 0);
    d1.adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    d2.adjacent
        .insert(Direction::Left, OutputId("out-1".to_owned()));
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &d1, "a", placement(120, 40));
    admit_into(&mut layout, &d1, "b", placement(120, 40));
    admit_into(&mut layout, &d2, "t", placement(1, 1));
    let src = d1.key();
    let dest = d2.key();
    assert!(layout.sync_focus_from_window(&src, &WindowId("b".to_owned())));
    let tree_before = layout.tree(&src).cloned();
    let obs = complete_engine_obs(&layout, &[&src, &dest]);
    let ctx = PairOperationContext {
        source_domain: &d1,
        source_outer_gap: 0,
        target_domain: &d2,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("b".to_owned()),
        observed: &obs,
        closed: &[],
    };
    let preview = layout.run_pair_drag_preview(
        &ctx,
        crate::policy::ScaledResize::unscaled(),
        &WindowId("b".to_owned()),
        0,
        0,
        None,
    );
    assert!(
        matches!(preview, Err(PairOperationFailure::Layout(_))),
        "preview refuses honestly (resolve or projection stage), got {preview:?}"
    );
    let release = layout.run_pair_drag_drop(
        &ctx,
        crate::policy::ScaledResize::unscaled(),
        &WindowId("b".to_owned()),
        0,
        0,
        None,
    );
    match release {
        Ok(out) if out.revision == out.base_revision => {}
        Err(PairOperationFailure::Layout(_)) => {}
        other => panic!("finish refuses honestly, never a finish-only float: {other:?}"),
    }
    assert_eq!(layout.tree(&src), tree_before.as_ref(), "source preserved");
    assert!(!layout.is_capacity_float_any(&WindowId("b".to_owned())));
}
