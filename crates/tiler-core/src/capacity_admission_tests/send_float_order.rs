//! Send-float results and admission order: exhausted-float mover frame, deferred carry, and sequence stability.
//!
//! Shared helpers live in the parent (`capacity_admission_tests.rs`); this module
//! holds only `#[test]` fns.

use super::*;
use crate::world_layout::{FreshOutcomeKind, PairOperationContext, PairSendKind};

#[test]
fn send_exhausted_floats_mover_with_exact_frame() {
    // Default float result: a send to an exhausted destination succeeds
    // as a slotless capacity float instead of refusing. The mover keeps
    // its exact current frame (no native move), the source allocation is
    // released, the destination tree is untouched, the origin is
    // diagnostic capacity, and the admission order never re-sequences.
    for follow in [true, false] {
        let mut layout = WorldLayout::new();
        let d1 = domain("o", "ws-1", 120, 80, 0);
        let d2 = domain("o", "ws-2", 1, 1, 0);
        let key1 = d1.key();
        let key2 = d2.key();
        admit_into(&mut layout, &d1, "a", placement(120, 40));
        let b_geom = {
            let mut extra = tiled_observed("b", "o", "ws-1");
            extra.hints = WindowSizeHints {
                min_w: Some(200),
                min_h: None,
                max_w: None,
                max_h: None,
            };
            let obs = complete_observed_for(&layout, &key1, vec![extra]);
            layout
                .admit_window(
                    &key1,
                    &WindowId("b".to_owned()),
                    ExceptionFlags::none(),
                    None,
                    placement(120, 40),
                    false,
                    false,
                    &obs,
                )
                .expect("b tiles despite minima")
                .geometry
        };
        let frame_b = b_geom
            .iter()
            .find(|g| g.window.0 == "b")
            .expect("b projected at admission")
            .rect;
        admit_into(&mut layout, &d2, "d", placement(1, 1));
        let dest_tree_before = layout.tree(&key2).expect("destination tree").clone();
        let seq_b = layout.admission_sequence(&key1, &WindowId("b".to_owned()));
        assert!(layout.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
        let mut obs = complete_engine_obs(&layout, &[&key1, &key2]);
        for entry in &mut obs {
            if entry.window.0 == "b" {
                entry.rect = frame_b;
            }
        }
        let ctx = PairOperationContext {
            source_domain: &d1,
            source_outer_gap: 0,
            target_domain: &d2,
            target_outer_gap: Some(0),
            observed_focus: &WindowId("b".to_owned()),
            observed: &obs,
            closed: &[],
        };
        let outcome = layout
            .run_pair_send(&ctx, &WindowId("b".to_owned()), follow, false)
            .expect("exhausted send floats under default float");
        assert_eq!(outcome.kind, PairSendKind::CapacityFloat);
        assert_eq!(outcome.window.0, "b");
        assert_eq!(outcome.source_output.0, "o");
        assert_eq!(outcome.target_workspace.0, "ws-2");
        assert!(
            outcome.geometry.iter().all(|g| g.window.0 != "b"),
            "slotless mover takes no tiled geometry: no native frame move"
        );
        assert_eq!(tiled_windows(&layout), vec!["a", "d"]);
        let flags = layout
            .exception_flags(&key2, &WindowId("b".to_owned()))
            .expect("mover floated");
        assert!(flags.floating && !flags.fullscreen && !flags.maximized && !flags.sticky);
        assert!(layout.is_capacity_float(&key2, &WindowId("b".to_owned())));
        assert!(
            !layout.is_automatic_fixed_float(&key2, &WindowId("b".to_owned())),
            "capacity origin stays distinct from fixed auto"
        );
        assert_eq!(
            layout.exception_float_geometry(&key2, &WindowId("b".to_owned())),
            Some(frame_b),
            "exact current frame preserved verbatim"
        );
        assert_eq!(
            layout.retained_float_geometry(&key2, &WindowId("b".to_owned())),
            Some(frame_b)
        );
        assert_eq!(
            layout.tree(&key2),
            Some(&dest_tree_before),
            "destination tree untouched: no tile slot taken"
        );
        assert_eq!(
            layout.admission_sequence(&key2, &WindowId("b".to_owned())),
            seq_b,
            "moves never re-sequence"
        );
        if follow {
            assert_eq!(
                layout.focus_leaf(&key2),
                Some(&leaf_of(&layout, &key2, "d")),
                "follow names the target bookkeeping MRU"
            );
        } else {
            assert_eq!(
                layout.focus_leaf(&key1),
                Some(&leaf_of(&layout, &key1, "a")),
                "stay falls back through the source MRU"
            );
        }
    }
}

// Retired test: `send_float_failed_ack_preserves_source`
// Tested `Session::cancel_unacked_pending` on the deleted two-phase
// `propose`/`acknowledge` state machine; `WorldLayout` executes synchronously
// with candidate atomicity (covered by refusal preservation tests).

#[test]
fn capacity_float_send_then_deferred_carries_ordinary_status() {
    // A capacity-floated arrival sends again through the ordinary
    // deferred path (same eligibility as an intentional float): the
    // slotless carry rehomes it with the diagnostic origin riding the
    // lifetime, and tiled survivors never reflow.
    let mut layout = WorldLayout::new();
    let d1 = domain("o", "ws-1", 120, 80, 0);
    let d2 = domain("o", "ws-2", 1, 1, 0);
    let key1 = d1.key();
    let key2 = d2.key();
    admit_into(&mut layout, &d1, "a", placement(120, 40));
    admit_into(&mut layout, &d1, "b", placement(120, 40));
    admit_into(&mut layout, &d2, "d", placement(1, 1));
    assert!(layout.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
    let obs = complete_engine_obs(&layout, &[&key1, &key2]);
    let ctx1 = PairOperationContext {
        source_domain: &d1,
        source_outer_gap: 0,
        target_domain: &d2,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("b".to_owned()),
        observed: &obs,
        closed: &[],
    };
    let send_out = layout
        .run_pair_send(&ctx1, &WindowId("b".to_owned()), true, false)
        .expect("float send succeeds");
    assert_eq!(send_out.kind, PairSendKind::CapacityFloat);
    assert!(layout.is_capacity_float(&key2, &WindowId("b".to_owned())));

    // The floated arrival carries onward like any ordinary float.
    let obs = complete_engine_obs(&layout, &[&key1, &key2]);
    let ctx2 = PairOperationContext {
        source_domain: &d2,
        source_outer_gap: 0,
        target_domain: &d1,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("b".to_owned()),
        observed: &obs,
        closed: &[],
    };
    let carry = layout
        .run_pair_send(&ctx2, &WindowId("b".to_owned()), false, false)
        .expect("capacity float carries through the ordinary deferred path");
    assert_eq!(carry.kind, PairSendKind::Deferred);
    assert_eq!(carry.window.0, "b");
    assert!(
        layout.is_exception(&key1, &WindowId("b".to_owned())),
        "rehomed with the lifetime"
    );
    assert!(
        layout.is_capacity_float(&key1, &WindowId("b".to_owned())),
        "diagnostic origin rides the lifetime across moves"
    );
    assert_eq!(tiled_windows(&layout), vec!["a", "d"]);
}

#[test]
fn converge_batch_records_carried_admission_order_not_id_order() {
    // HIGH-1: a batched [z-win, a-win] observation must record z=0, a=1
    // (carried admission order), never id order (a=0, z=1). A later
    // shrink then evicts the actual newest (a-win), not the id-first
    // window.
    let mut layout = WorldLayout::new();
    let d = domain("o", "w", 120, 80, 0);
    let key = d.key();
    let counts = layout
        .converge_fresh_domain(
            &d,
            0,
            &[
                tiled_observed("z-win", "o", "w"),
                tiled_observed("a-win", "o", "w"),
            ],
            Some(&WindowId("a-win".to_owned())),
        )
        .expect("batch converges");
    assert_eq!(
        (counts.removed, counts.admitted, counts.flags_adopted),
        (0, 2, 0)
    );
    assert_eq!(
        layout.admission_sequence(&key, &WindowId("z-win".to_owned())),
        Some(0),
        "first in carried order takes the first sequence"
    );
    assert_eq!(
        layout.admission_sequence(&key, &WindowId("a-win".to_owned())),
        Some(1),
        "second in carried order takes the second sequence, never id order"
    );
    // Shrink until exactly one victim must float: the actual newest.
    let obs = complete_engine_obs(&layout, &[&key]);
    layout
        .update_domain_gaps(
            &key,
            Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 80,
            },
            0,
            0,
            &WindowId("a-win".to_owned()),
            &obs,
        )
        .expect("shrink repairs");
    assert_eq!(tiled_windows(&layout), vec!["z-win".to_string()]);
    assert!(
        layout.is_capacity_float(&key, &WindowId("a-win".to_owned())),
        "shrink evicts the actual newest first"
    );
}

#[test]
fn clean_fit_roster_shares_one_sequence_with_identity_tiebreak() {
    // A clean-fit roster shares one admission sequence number, so later
    // victim selection breaks the tie by window identity descending.
    let mut layout = WorldLayout::new();
    let d = domain("o", "w", 200, 100, 0);
    let key = d.key();
    let windows = vec![
        engine_window(
            "a",
            "o",
            "w",
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
        ),
        engine_window(
            "b",
            "o",
            "w",
            Rect {
                x: 100,
                y: 0,
                w: 100,
                h: 100,
            },
        ),
    ];
    let fresh = layout
        .reconcile_fresh(&d, 0, &WindowId("a".to_owned()), &windows)
        .expect("clean fit succeeds");
    assert_eq!(fresh.kind, FreshOutcomeKind::Fit);
    assert_eq!(
        layout.admission_sequence(&key, &WindowId("a".to_owned())),
        layout.admission_sequence(&key, &WindowId("b".to_owned())),
        "clean-fit roster shares one sequence number"
    );
    assert_eq!(
        layout.admission_sequence(&key, &WindowId("a".to_owned())),
        Some(0)
    );
    // Shrink until one victim must float: the shared sequence ties, so
    // identity descending evicts b first.
    let obs = complete_engine_obs(&layout, &[&key]);
    layout
        .update_domain_gaps(
            &key,
            Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 100,
            },
            0,
            0,
            &WindowId("a".to_owned()),
            &obs,
        )
        .expect("shrink repairs");
    assert_eq!(tiled_windows(&layout), vec!["a".to_string()]);
    assert!(layout.is_capacity_float(&key, &WindowId("b".to_owned())));
}

#[test]
fn workspace_send_preserves_global_admission_order() {
    // Transfer keeps the mover's first sequence globally: a later fresh
    // admission in either domain takes the next counter, never a
    // collision with the moved lifetime.
    let mut layout = WorldLayout::new();
    let d1 = domain("o", "ws-1", 120, 80, 0);
    let d2 = domain("o", "ws-2", 120, 80, 0);
    let key1 = d1.key();
    let key2 = d2.key();
    admit_into(&mut layout, &d1, "a", placement(120, 40));
    admit_into(&mut layout, &d1, "b", placement(120, 40));
    admit_into(&mut layout, &d2, "d", placement(120, 40));
    assert_eq!(
        layout.admission_sequence(&key1, &WindowId("b".to_owned())),
        Some(1)
    );
    assert!(layout.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
    let obs = complete_engine_obs(&layout, &[&key1, &key2]);
    let ctx = PairOperationContext {
        source_domain: &d1,
        source_outer_gap: 0,
        target_domain: &d2,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("b".to_owned()),
        observed: &obs,
        closed: &[],
    };
    layout
        .run_pair_send(&ctx, &WindowId("b".to_owned()), true, false)
        .expect("send succeeds");
    assert_eq!(
        layout.admission_sequence(&key2, &WindowId("b".to_owned())),
        Some(1),
        "transfer keeps the first sequence"
    );
    admit_into(&mut layout, &d1, "c", placement(120, 40));
    assert_eq!(
        layout.admission_sequence(&key1, &WindowId("c".to_owned())),
        Some(3),
        "fresh admission advances the global counter past the moved lifetime"
    );
    assert_eq!(
        layout.admission_sequence(&key2, &WindowId("d".to_owned())),
        Some(2)
    );
}

#[test]
fn send_to_float_same_output_keeps_native_live_frame() {
    // MED-2: the carried native live frame is the exact current
    // native frame. It wins over the projected allocation (which can
    // differ under B6/clamped hints) instead of assuming equality.
    let mut layout = WorldLayout::new();
    let d1 = domain("o", "ws-1", 120, 80, 0);
    let d2 = domain("o", "ws-2", 1, 1, 0);
    let key1 = d1.key();
    let key2 = d2.key();
    admit_into(&mut layout, &d1, "a", placement(120, 40));
    let b_geom = admit_into(&mut layout, &d1, "b", placement(120, 40));
    let allocation = b_geom
        .iter()
        .find(|g| g.window.0 == "b")
        .expect("b projected at admission")
        .rect;
    admit_into(&mut layout, &d2, "d", placement(1, 1));
    let live = Rect {
        x: 1,
        y: 2,
        w: 30,
        h: 30,
    };
    assert_ne!(live, allocation, "live frame differs from the allocation");
    assert!(layout.sync_focus_from_window(&key1, &WindowId("b".to_owned())));
    let mut obs = complete_engine_obs(&layout, &[&key1, &key2]);
    for entry in &mut obs {
        if entry.window.0 == "b" {
            entry.rect = live;
        }
    }
    let ctx = PairOperationContext {
        source_domain: &d1,
        source_outer_gap: 0,
        target_domain: &d2,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("b".to_owned()),
        observed: &obs,
        closed: &[],
    };
    let out = layout
        .run_pair_send(&ctx, &WindowId("b".to_owned()), true, false)
        .expect("exhausted send floats");
    assert_eq!(out.kind, PairSendKind::CapacityFloat);
    assert!(
        out.geometry.iter().all(|g| g.window.0 != "b"),
        "slotless mover takes no tiled geometry"
    );
    assert!(layout.is_capacity_float(&key2, &WindowId("b".to_owned())));
    assert_eq!(
        layout.exception_float_geometry(&key2, &WindowId("b".to_owned())),
        Some(live),
        "same-output float keeps the exact native live frame"
    );
    assert_eq!(
        layout.retained_float_geometry(&key2, &WindowId("b".to_owned())),
        Some(live)
    );
}

#[test]
fn send_to_float_cross_output_stages_no_source_coordinate_claim() {
    // MED-2: side-by-side outputs with nonzero origins. A cross-output
    // capacity float never translates, centers, or otherwise guesses the
    // source coordinates into a target claim: the frame stays untouched
    // (None) for the established native remap plus its arrival proof,
    // even when a live frame was carried.
    let out1 = OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    let out2 = OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        bounds: Rect {
            x: 1000,
            y: 0,
            w: 1,
            h: 1,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    let mut layout = WorldLayout::new();
    admit_into(&mut layout, &out1, "m", placement(120, 40));
    admit_into(&mut layout, &out2, "t", placement(1, 1));
    let src = out1.key();
    let target_key = out2.key();
    let dest_tree_before = layout.tree(&target_key).expect("target tree").clone();
    assert!(layout.sync_focus_from_window(&src, &WindowId("m".to_owned())));
    let mut obs = complete_engine_obs(&layout, &[&src, &target_key]);
    for entry in &mut obs {
        if entry.window.0 == "m" {
            entry.rect = Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            };
        }
    }
    let ctx = PairOperationContext {
        source_domain: &out1,
        source_outer_gap: 0,
        target_domain: &out2,
        target_outer_gap: Some(0),
        observed_focus: &WindowId("m".to_owned()),
        observed: &obs,
        closed: &[],
    };
    let out = layout
        .run_pair_send(&ctx, &WindowId("m".to_owned()), true, true)
        .expect("exhausted cross-output send floats");
    assert_eq!(out.kind, PairSendKind::CapacityFloat);
    assert_eq!(tiled_windows(&layout), vec!["t".to_string()]);
    assert!(layout.is_capacity_float(&target_key, &WindowId("m".to_owned())));
    assert_eq!(
        layout.exception_float_geometry(&target_key, &WindowId("m".to_owned())),
        None,
        "no source-coordinate claim at the target"
    );
    assert_eq!(
        layout.retained_float_geometry(&target_key, &WindowId("m".to_owned())),
        None
    );
    assert!(layout.is_exception(&target_key, &WindowId("m".to_owned())));
    assert_eq!(
        layout.tree(&target_key),
        Some(&dest_tree_before),
        "destination tree untouched"
    );
}

#[test]
fn send_to_output_refused_when_target_unknown_source_preserved() {
    use crate::boundary::CoreCommand;
    use crate::size_hints::WindowSizeHints;
    use crate::world_engine::{WorldEngine, WorldOutcome};
    use crate::world_report::{
        ActivationPolicy, CommandRejectKind, CommandResult, NativeWorkspaceFact, OutputIdentity,
        PresentationIngressV4, PresentationRowV4, ScreenReport, VisibilityFact,
        WORLD_CONTRACT_VERSION_V4, WindowFact, WindowType, WorldCommand, WorldReportV4,
    };

    let make_screen = |output: &str, x: i32| ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(Rect {
            x,
            y: 0,
            w: 120,
            h: 80,
        }),
        work_area: Some(Rect {
            x,
            y: 0,
            w: 120,
            h: 80,
        }),
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
        last_write: None,
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
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    };

    // Unavailable remap target: the output send refuses SendTargetUnknown
    // with the source exactly preserved and no re-sequencing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let screens = vec![make_screen("o1", 0), make_screen("o2", 120)];
    let windows = vec![
        make_fact(
            "a",
            "o1",
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 40,
            },
        ),
        make_fact(
            "b",
            "o1",
            Rect {
                x: 0,
                y: 40,
                w: 120,
                h: 40,
            },
        ),
        make_fact(
            "c",
            "o2",
            Rect {
                x: 120,
                y: 0,
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
        .find(|k| k.output.0 == "o1")
        .expect("o1 domain");
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
            Some(WorldCommand::Operation(CoreCommand::SendToOutput {
                window: "b".to_owned(),
                target_output: "o9".to_owned(),
                target_workspace: "ws-1".to_owned(),
                follow: true,
            })),
        ),
        &[],
    );
    assert!(matches!(outcome, WorldOutcome::Accepted(_)));
    assert_eq!(
        settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::SendTargetUnknown
        })
    );
    assert_eq!(
        engine.world.layout.tree(&key1),
        tree_before.as_ref(),
        "source preserved"
    );
    assert_eq!(engine.world.layout.focus_leaf(&key1), focus_before.as_ref());
    assert_eq!(engine.world.layout.accepted_revision(&key1), rev_before);
    assert_eq!(
        engine
            .world
            .layout
            .admission_sequence(&key1, &WindowId("b".to_owned())),
        seq_b,
        "refusal never re-sequences"
    );
    assert!(
        engine.world.layout.all_exception_windows().next().is_none(),
        "no fake float staged"
    );
}
