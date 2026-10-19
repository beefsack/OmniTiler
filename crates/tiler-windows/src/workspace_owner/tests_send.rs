#![forbid(unsafe_code)]
use super::*;

#[test]
fn send_output_builder_is_distinct_from_workspace_send() {
    use tiler_core::boundary::CoreCommand;
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    let correlation = CorrelationId::parse("output-send").expect("correlation");
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-b", "ws-1", bounds, 8);
    let rows = |token: &str| OwnerRow {
        token: token.to_owned(),
        rect: bounds,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    };
    let source_rows = vec![rows("w1")];
    let target_rows = vec![rows("t1")];
    let mut output = build_send_output_event(
        &owner,
        &generation,
        &correlation,
        0,
        1,
        source.clone(),
        target.clone(),
        &source_rows,
        &target_rows,
        "w1",
        8,
        true,
    )
    .expect("output event");
    stamp_send_output_target(&mut output, &target.1);
    match &output.command {
        CoreCommand::SendToOutput {
            window,
            target_output,
            target_workspace,
            follow: true,
        } => {
            assert_eq!(window, "w1");
            assert_eq!(target_output, "mon-b");
            assert_eq!(target_workspace, "ws-1");
        }
        _ => panic!("expected distinct send-to-output"),
    }
    // Same inputs through the workspace route stay a different command.
    let mut same = build_send_event(
        &owner,
        &generation,
        &correlation,
        0,
        1,
        source,
        target.clone(),
        &source_rows,
        &target_rows,
        "w1",
        8,
        true,
    )
    .expect("workspace event");
    stamp_send_target(&mut same, &target.1);
    assert!(
        matches!(same.command, CoreCommand::SendToWorkspace { .. }),
        "workspace route keeps its own command"
    );
    // Stamps never cross routes: each touches only its own command.
    stamp_send_target(&mut output, &target.1);
    assert!(
        matches!(output.command, CoreCommand::SendToOutput { .. }),
        "workspace stamp leaves the output command alone"
    );
    match &output.command {
        CoreCommand::SendToOutput {
            target_output,
            target_workspace,
            ..
        } => {
            assert_eq!(target_output, "mon-b");
            assert_eq!(target_workspace, "ws-1");
        }
        _ => unreachable!(),
    }
    stamp_send_output_target(&mut same, &target.1);
    assert!(
        matches!(same.command, CoreCommand::SendToWorkspace { .. }),
        "output stamp leaves the workspace command alone"
    );
    assert!(
        build_send_output_event(
            &owner,
            &generation,
            &correlation,
            0,
            1,
            workspace_domain("mon-a", "ws-1", bounds, 8),
            target.clone(),
            &source_rows,
            &target_rows,
            "",
            8,
            true,
        )
        .is_none()
    );
}

#[allow(clippy::too_many_arguments)]
fn seed_output_pair(
    engine: &mut tiler_core::engine::Engine,
    owner: &OwnerId,
    generation: &GenerationId,
    source: &(OutputDomain, DomainKey),
    target: &(OutputDomain, DomainKey),
    source_rows: &[(&str, Rect)],
    target_rows: &[(&str, Rect)],
    source_focus: &str,
    target_focus: Option<&str>,
) {
    for (domain, rows, focused) in [
        (source, source_rows, Some(source_focus)),
        (target, target_rows, target_focus),
    ] {
        let Some(focused) = focused else { continue };
        let correlation = CorrelationId::parse("seed-output").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            owner,
            generation,
            &correlation,
            0,
            rows.len() as u64,
            &domain.0,
            &domain.1,
            8,
            &rows
                .iter()
                .map(|(t, r)| {
                    (
                        tiler_core::directional::WindowId((*t).to_owned()),
                        *r,
                        tiler_core::size_hints::WindowSizeHints::none(),
                    )
                })
                .collect::<Vec<_>>(),
            Some(&tiler_core::directional::WindowId(focused.to_owned())),
        );
        let reply = engine.handle(&event);
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed must converge, got {reply:?}"
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn output_send_reply(
    engine: &mut tiler_core::engine::Engine,
    owner: &OwnerId,
    generation: &GenerationId,
    source: &(OutputDomain, DomainKey),
    target: &(OutputDomain, DomainKey),
    source_rows: &[OwnerRow],
    target_rows: &[OwnerRow],
    mover: &str,
    follow: bool,
) -> tiler_core::boundary::CoreReply {
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("output-send").expect("correlation");
    let mut pairs: Vec<(String, Rect)> = source_rows
        .iter()
        .chain(target_rows.iter())
        .map(|r| (r.token.clone(), r.rect))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let fingerprint = crate::tiling::fingerprint(
        &pairs
            .iter()
            .map(|(t, r)| (t.clone(), *r))
            .collect::<Vec<_>>(),
    );
    let mut event = build_send_output_event(
        owner,
        generation,
        &correlation,
        revision,
        fingerprint,
        source.clone(),
        target.clone(),
        source_rows,
        target_rows,
        mover,
        8,
        follow,
    )
    .expect("output event");
    stamp_send_output_target(&mut event, &target.1);
    engine.handle(&event)
}

#[test]
fn send_output_commits_across_outputs_with_follow_and_stay() {
    use tiler_core::boundary::CoreReply;
    // Same workspace id on different outputs is a valid explicit-send
    // target; the empty destination admits through the root fallback.
    for follow in [true, false] {
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-b", "ws-1", bounds, 8);
        seed_output_pair(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &[("w1", bounds), ("w2", bounds)],
            &[],
            "w1",
            None,
        );
        let row = |token: &str| OwnerRow {
            token: token.to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        };
        let reply = output_send_reply(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &[row("w1"), row("w2")],
            &[],
            "w1",
            follow,
        );
        let CoreReply::SendOutput(plan) = &reply else {
            panic!("explicit send commits SendOutput, got {reply:?}");
        };
        assert_eq!(plan.follow, follow, "command follow/stay echoes");
        assert!(
            matches!(
                plan.operation,
                tiler_core::contract::LifecycleOperation::MoveTiled { .. }
            ),
            "ordinary transfer shape, got {:?}",
            plan.operation
        );
        let writes = super::super::planned_writes(&reply).expect("SendOutput extracts");
        assert_eq!(writes.len(), plan.geometry.len(), "nothing dropped");
        assert!(writes.iter().any(|w| w.window.0 == "w1"), "mover placed");
        assert!(
            writes.iter().any(|w| w.window.0 == "w2"),
            "source survivor reflowed"
        );
        let mover = plan
            .geometry
            .iter()
            .find(|g| g.window.0 == "w1")
            .expect("mover geometry");
        assert_eq!(mover.output.0, "mon-b", "mover lands cross-output");
        assert_eq!(mover.workspace.0, "ws-1", "same workspace id valid");
    }
}

#[test]
fn send_output_admits_remembered_mru_and_root_instead_of_r4() {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::directional::{Direction, WindowId};
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-b", "ws-2", bounds, 8);
    let row = |token: &str| OwnerRow {
        token: token.to_owned(),
        rect: bounds,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    };
    // Occupied destination with focus history on t1: the ordinary
    // destination-history admission places the mover (no R4 edge rule).
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    seed_output_pair(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &[("w1", bounds)],
        &[("t1", bounds)],
        "w1",
        Some("t1"),
    );
    // Second target converge keeps focus history on t1 (MRU admission
    // source); a remembered leaf pinned on t2 still wins over the MRU,
    // landing the mover in t2's right half - while R4 answers a
    // full-height edge strip, so the two geometries differ, not just the
    // reply variants.
    let correlation = CorrelationId::parse("seed-mru").expect("correlation");
    let event = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        engine
            .session(&target.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        2,
        &target.0,
        &target.1,
        8,
        &[
            (
                WindowId("t1".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
            (
                WindowId("t2".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
        ],
        Some(&WindowId("t1".to_owned())),
    );
    let _ = engine.handle(&event);
    assert!(engine.remember_domain_focus(&target.1, &WindowId("t2".to_owned())));
    let reply = output_send_reply(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &[row("w1")],
        &[row("t1"), row("t2")],
        "w1",
        true,
    );
    let CoreReply::SendOutput(plan) = &reply else {
        panic!("remembered-leaf admission commits, got {reply:?}");
    };
    let send_mover = plan
        .geometry
        .iter()
        .find(|g| g.window.0 == "w1")
        .expect("mover admitted into the occupied target");
    assert!(
        matches!(
            plan.operation,
            tiler_core::contract::LifecycleOperation::MoveTiled { .. }
        ),
        "ordinary admission, never R4 edge insertion"
    );
    // Same topology through the directional R4 route answers
    // MoveDirectional with a cross view instead of SendOutput. A fresh
    // Engine replays the pre-send topology: the send above already
    // committed its transfer on the first Engine.
    let mut engine = tiler_core::engine::Engine::new();
    engine.sync_binding(&owner, &generation);
    seed_output_pair(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &[("w1", bounds)],
        &[("t1", bounds), ("t2", bounds)],
        "w1",
        Some("t2"),
    );
    let ((source_domain, source_key), (target_domain, target_key)) =
        super::super::directional_pair_with_adjacency(
            (source.0.clone(), source.1.clone()),
            (target.0.clone(), target.1.clone()),
            Direction::Right,
        );
    let correlation = CorrelationId::parse("r4-contrast").expect("correlation");
    let mut windows = Vec::new();
    for (key, token) in [
        (&source_key, "w1"),
        (&target_key, "t1"),
        (&target_key, "t2"),
    ] {
        windows.push(tiler_core::seed::EngineWindow {
            window: WindowId(token.to_owned()),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            rect: bounds,
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        });
    }
    let cross_event = tiler_core::boundary::CoreEvent {
        owner: owner.clone(),
        generation: generation.clone(),
        correlation,
        revision: 0,
        fingerprint: 7,
        domain: source_domain.clone(),
        domain_key: source_key.clone(),
        outer_gap: 8,
        focused_window: WindowId("w1".to_owned()),
        windows,
        directional: Some(vec![
            (source_domain, source_key),
            (target_domain, target_key),
        ]),
        directional_target_outer_gap: Some(8),
        target_domain: None,
        target_windows: Vec::new(),
        closed_windows: Vec::new(),
        command: CoreCommand::Move {
            window: "w1".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    };
    match engine.handle(&cross_event) {
        CoreReply::MoveDirectional(plan) => {
            assert!(plan.cross.is_some(), "R4 answers a cross view");
            let r4_mover = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w1")
                .expect("R4 places the crossing mover");
            for (label, rect) in [("send", send_mover.rect), ("r4", r4_mover.rect)] {
                assert!(
                    rect.x >= 0 && rect.y >= 0 && rect.x + rect.w <= 800 && rect.y + rect.h <= 600,
                    "{label} mover lands inside the target bounds, got {rect:?}"
                );
            }
            assert!(
                send_mover.rect != r4_mover.rect,
                "ordinary remembered-leaf landing differs from R4 edge insertion: \
                 send {send:?} vs r4 {r4:?}",
                send = send_mover.rect,
                r4 = r4_mover.rect,
            );
            // Observed layout (800x600 target, t1 left half, t2 right
            // half): the remembered-leaf split lands the mover in the
            // bottom of t2's right half, while R4 answers a full-height
            // edge strip. Both land inside the target; only the admission
            // rule selects which.
            assert!(
                send_mover.rect.x >= 400 && send_mover.rect.y >= 300,
                "remembered right-leaf split lands bottom-right, got {send:?}",
                send = send_mover.rect,
            );
            assert!(
                r4_mover.rect.y == 0 && r4_mover.rect.h == 600,
                "R4 edge insertion spans the full target height, got {r4:?}",
                r4 = r4_mover.rect,
            );
        }
        reply => panic!("directional route stays MoveDirectional, got {reply:?}"),
    }
}

#[test]
fn output_context_prefers_foreground_then_pointer_then_cache() {
    let mut spaces = ManagedWorkspaces::new();
    spaces.ensure_output("mon-a");
    spaces.ensure_output("mon-b");
    // Stale cache (mon-b) loses to genuine foreground (mon-a): defect (3).
    assert_eq!(
        output_context(&spaces, Some("mon-b"), Some("mon-a"), Some("mon-b")).as_deref(),
        Some("mon-a")
    );
    assert_eq!(
        output_context(&spaces, Some("mon-a"), Some("mon-b"), Some("mon-a")).as_deref(),
        Some("mon-b")
    );
    // No foreground: pointer beats a stale cache.
    assert_eq!(
        output_context(&spaces, Some("mon-a"), None, Some("mon-b")).as_deref(),
        Some("mon-b")
    );
    // Neither focus proxy known: fall back to the cache, then ordering.
    assert_eq!(
        output_context(&spaces, Some("mon-b"), Some("gone"), Some("gone")).as_deref(),
        Some("mon-b")
    );
    assert_eq!(
        output_context(&spaces, Some("gone"), Some("gone"), Some("gone")).as_deref(),
        Some("mon-a")
    );
    assert_eq!(
        output_context(&spaces, None, None, None).as_deref(),
        Some("mon-a")
    );
    let empty = ManagedWorkspaces::new();
    assert_eq!(output_context(&empty, None, None, None), None);
}

#[test]
fn send_event_binds_target_and_focused_mover() {
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    let source_rows = vec![OwnerRow {
        token: "w1".to_owned(),
        rect: bounds,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    let target_rows = vec![OwnerRow {
        token: "w2".to_owned(),
        rect: bounds,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        0,
        1,
        source,
        target.clone(),
        &source_rows,
        &target_rows,
        "w1",
        8,
        true,
    )
    .expect("event");
    stamp_send_target(&mut event, &target.1);
    match &event.command {
        CoreCommand::SendToWorkspace {
            window,
            target_output,
            target_workspace,
            follow: true,
        } => {
            assert_eq!(window, "w1");
            assert_eq!(target_output, "mon-a");
            assert_eq!(target_workspace, "ws-2");
        }
        _ => panic!("expected send-to-workspace"),
    }
    assert_eq!(event.focused_window.0, "w1");
    assert_eq!(event.windows.len(), 1);
    assert_eq!(event.target_windows.len(), 1);
    assert!(event.target_domain.is_some());
}

#[test]
fn membership_transfer_rejects_noop_and_foreign() {
    let mover = key(7);
    let other = key(8);
    let source: BTreeSet<WindowKey> = [other.clone()].into_iter().collect();
    let target: BTreeSet<WindowKey> = [mover.clone(), other.clone()].into_iter().collect();
    assert!(verify_membership_transfer(&mover, &source, &target));
    let source: BTreeSet<WindowKey> = [mover.clone()].into_iter().collect();
    assert!(!verify_membership_transfer(&mover, &source, &target));
    assert!(!verify_membership_transfer(
        &mover,
        &BTreeSet::new(),
        &BTreeSet::new()
    ));
}

#[test]
fn send_route_splits_engine_from_native_boundary() {
    use super::super::{SendRoute, send_route};
    // Tiled-to-tiled keeps the shared two-domain Engine plan.
    assert_eq!(send_route(true, true), SendRoute::Engine);
    // Any floating boundary transfers native membership instead: no
    // two-domain plan, no floating-side geometry, tiled side reflows.
    assert_eq!(send_route(false, true), SendRoute::Native);
    assert_eq!(send_route(true, false), SendRoute::Native);
    assert_eq!(send_route(false, false), SendRoute::Native);
}

#[test]
fn directional_refuses_both_focus_and_move_on_floating() {
    // KDE plan-adapter parity: a floating workspace runs no tile layout,
    // so directional focus refuses exactly like directional move, with
    // the matching workspace-floating vocabulary. Tiled workspaces pass.
    use super::super::directional_workspace_refusal;
    use crate::snapkey::SnapOp;
    assert_eq!(
        directional_workspace_refusal(SnapOp::Focus, false),
        Some("focus-refused-workspace-floating")
    );
    assert_eq!(
        directional_workspace_refusal(SnapOp::Move, false),
        Some("move-refused-workspace-floating")
    );
    assert_eq!(directional_workspace_refusal(SnapOp::Focus, true), None);
    assert_eq!(directional_workspace_refusal(SnapOp::Move, true), None);
}

#[test]
fn output_at_point_resolves_pointer_output_without_adjacency() {
    use super::super::{DirectionalOutput, output_at_point};
    // Sideways pair with a negative-x secondary: arrangement-independent
    // containment, no adjacency derivation involved.
    let outputs = vec![
        DirectionalOutput {
            id: "primary".to_owned(),
            full: Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 1440,
            },
        },
        DirectionalOutput {
            id: "left".to_owned(),
            full: Rect {
                x: -1920,
                y: -200,
                w: 1920,
                h: 1080,
            },
        },
    ];
    assert_eq!(
        output_at_point(&outputs, 100, 100).as_deref(),
        Some("primary")
    );
    assert_eq!(output_at_point(&outputs, -100, 0).as_deref(), Some("left"));
    assert_eq!(
        output_at_point(&outputs, -1920, -200).as_deref(),
        Some("left")
    );
    // Exclusive far edges belong to no output.
    assert_eq!(output_at_point(&outputs, 2560, 100), None);
    assert_eq!(output_at_point(&outputs, 0, 1440), None);
    // Between-monitor gap and far outside resolve nowhere.
    assert_eq!(output_at_point(&outputs, -10, 2000), None);
    assert_eq!(output_at_point(&outputs, 9999, 9999), None);
    // Degenerate rectangles never claim a point.
    let degenerate = vec![DirectionalOutput {
        id: "flat".to_owned(),
        full: Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 600,
        },
    }];
    assert_eq!(output_at_point(&degenerate, 0, 100), None);
    assert!(output_at_point(&[], 0, 0).is_none());
}

#[test]
fn drag_drop_fences_mirror_directional_tokens_without_direction() {
    use super::super::{DirectionalFenceLive, DragDropFenceSnapshot, drag_drop_fences_hold};
    fn snapshot() -> DragDropFenceSnapshot {
        DragDropFenceSnapshot {
            inner_gap: 8,
            outer_gap: 8,
            source_output: "mon-a".to_owned(),
            source_workspace: "ws-1".to_owned(),
            target_output: "mon-b".to_owned(),
            target_workspace: "ws-1".to_owned(),
            source_tiled: true,
            target_tiled: true,
            overlay_maximized: false,
            overlay_fullscreen: false,
            source_full: rect(0, 0),
            target_full: rect(-1920, 0),
            source_bounds: rect(0, 0),
            target_bounds: rect(-1920, 0),
            owner: "owner".to_owned(),
            generation: "gen".to_owned(),
        }
    }
    fn live() -> DirectionalFenceLive {
        DirectionalFenceLive {
            inner_gap: 8,
            outer_gap: 8,
            source_tiled: true,
            target_tiled: true,
            source_is_active: true,
            target_is_active: true,
            topology_ok: true,
            scope_ok: true,
            lifetime_ok: true,
            membership_ok: true,
            overlay_maximized: false,
            overlay_fullscreen: false,
            owner_ok: true,
        }
    }
    let snap = snapshot();
    assert_eq!(drag_drop_fences_hold(&snap, &live()), None);
    // Each fence maps to its directional token; view drift (the
    // destination current workspace changed under the pointer) refuses
    // instead of dropping into a stale layout.
    let mut view = live();
    view.target_is_active = false;
    assert_eq!(drag_drop_fences_hold(&snap, &view), Some("view-changed"));
    let mut topology = live();
    topology.topology_ok = false;
    assert_eq!(
        drag_drop_fences_hold(&snap, &topology),
        Some("topology-changed")
    );
    let mut lifetime = live();
    lifetime.lifetime_ok = false;
    assert_eq!(
        drag_drop_fences_hold(&snap, &lifetime),
        Some("identity-changed")
    );
    let mut membership = live();
    membership.membership_ok = false;
    assert_eq!(
        drag_drop_fences_hold(&snap, &membership),
        Some("origin-vanished")
    );
    let mut mode = live();
    mode.target_tiled = false;
    assert_eq!(
        drag_drop_fences_hold(&snap, &mode),
        Some("workspace-floating")
    );
    let mut overlay = live();
    overlay.overlay_fullscreen = true;
    assert_eq!(
        drag_drop_fences_hold(&snap, &overlay),
        Some("overlay-changed")
    );
    // A fullscreen that was already present at snapshot time vetoes.
    let mut snap_full = snapshot();
    snap_full.overlay_fullscreen = true;
    assert_eq!(
        drag_drop_fences_hold(&snap_full, &overlay),
        Some("move-refused-fullscreen")
    );
    let mut owner = live();
    owner.owner_ok = false;
    assert_eq!(drag_drop_fences_hold(&snap, &owner), Some("owner-changed"));
}

#[test]
fn float_carry_survives_a_fresh_engine_session() {
    // After a workspace-mode release the Engine session carries no
    // exception, and a boundary-send target session has never seen the
    // mover: the lifetime-keyed intent still rides the token floating,
    // while tiled members and recycled lifetimes stay out.
    use super::super::float_carry_tokens;
    use std::collections::{BTreeMap, BTreeSet};
    let floated = key(11);
    let tiled = key(12);
    let members: BTreeSet<WindowKey> = [floated.clone(), tiled.clone()].into_iter().collect();
    let mut token_of: BTreeMap<WindowKey, String> = BTreeMap::new();
    token_of.insert(floated.clone(), "w-float".to_owned());
    token_of.insert(tiled.clone(), "w-tiled".to_owned());
    let intent: BTreeSet<WindowKey> = [floated.clone()].into_iter().collect();
    // Empty Engine set (fresh after release): intent still carries.
    let carried = float_carry_tokens(&members, &token_of, &intent);
    assert_eq!(carried, ["w-float".to_owned()].into_iter().collect());
    // A recycled HWND (same number, fresh lifetime) never inherits.
    let recycled = WindowKey {
        hwnd: 11,
        pid: 9000,
        creation: "c000000000009000".to_owned(),
    };
    let members: BTreeSet<WindowKey> = [recycled.clone(), tiled.clone()].into_iter().collect();
    let mut token_of: BTreeMap<WindowKey, String> = BTreeMap::new();
    token_of.insert(recycled, "w-new".to_owned());
    token_of.insert(tiled, "w-tiled".to_owned());
    assert!(float_carry_tokens(&members, &token_of, &intent).is_empty());
    // Actual Engine sequence: per-window float, release the exact domain,
    // then a fresh reconcile carrying the intent. The floated window stays
    // a slotless exception with no geometry target; the sibling reflows.
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::directional::WindowId;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_two_tiled(&mut engine, &owner, &generation, &domain, bounds);
    let float = engine.handle(&float_event(
        &owner,
        &generation,
        "float-carry-1",
        revision_of(&engine, &domain),
        &domain,
        &[
            (WindowId("w1".to_owned()), bounds, false),
            (WindowId("w2".to_owned()), bounds, false),
        ],
        "w1",
        "w1",
        None,
    ));
    let CoreReply::Tiled(float_plan) = float else {
        panic!("float commits, got {float:?}");
    };
    assert!(
        !float_plan.geometry.iter().any(|g| g.window.0 == "w1"),
        "floated window leaves the tree"
    );
    assert!(
        engine
            .session(&domain.1)
            .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
        "floated window rides as an exception"
    );
    let mut release = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &CorrelationId::parse("float-carry-release").expect("correlation"),
        revision_of(&engine, &domain),
        2,
        &domain.0,
        &domain.1,
        8,
        &[
            (
                WindowId("w1".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
            (
                WindowId("w2".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
        ],
        Some(&WindowId("w2".to_owned())),
    );
    release.command = CoreCommand::ReleaseDomain;
    match engine.handle(&release) {
        CoreReply::Released => {}
        other => panic!("release drops the exact domain, got {other:?}"),
    }
    assert!(!engine.contains(&domain.1));
    // The intent outlives the released session: the fresh observation
    // still carries the floated token while the tiled sibling stays out.
    let carry_keys: BTreeSet<WindowKey> = [key(11), key(12)].into_iter().collect();
    let carry_tokens: BTreeMap<WindowKey, String> =
        [(key(11), "w1".to_owned()), (key(12), "w2".to_owned())]
            .into_iter()
            .collect();
    let carry_intent: BTreeSet<WindowKey> = [key(11)].into_iter().collect();
    let carried = float_carry_tokens(&carry_keys, &carry_tokens, &carry_intent);
    assert_eq!(carried, ["w1".to_owned()].into_iter().collect());
    let carried_rows: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = ["w1", "w2"]
        .iter()
        .map(|token| {
            (
                WindowId((*token).to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                carried.contains(*token),
            )
        })
        .collect();
    let fp = crate::tiling::fingerprint(
        &carried_rows
            .iter()
            .map(|(token, rect, _, _)| (token.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let fresh = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("float-carry-fresh").expect("correlation"),
        0,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried_rows,
        Some(&WindowId("w2".to_owned())),
    );
    match engine.handle(&fresh) {
        CoreReply::Tiled(plan) => {
            assert!(
                !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                "carried float takes no geometry target"
            );
            assert!(
                plan.geometry.iter().any(|g| g.window.0 == "w2"),
                "sibling reflows around the exception"
            );
        }
        CoreReply::Projection(plan) => {
            assert!(
                !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                "carried float takes no geometry target"
            );
            assert!(
                plan.geometry.iter().any(|g| g.window.0 == "w2"),
                "sibling reflows around the exception"
            );
        }
        CoreReply::SendWorkspace(plan) => {
            assert!(
                !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                "carried float takes no geometry target"
            );
            assert!(
                plan.geometry.iter().any(|g| g.window.0 == "w2"),
                "sibling reflows around the exception"
            );
        }
        other => panic!("fresh reconcile converges, got {other:?}"),
    }
    assert!(
        engine
            .session(&domain.1)
            .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
        "exception survives the release through the carried intent"
    );
}

#[test]
fn native_boundary_source_reconcile_drops_mover_and_reflows_survivor() {
    // Native-boundary sends transfer membership with no Engine plan and no
    // floating-side geometry: the runtime source phase is a complete
    // reconcile carrying only the survivors. The shared Engine must drop
    // the mover and reflow the survivor, never retain the stale member.
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_two_tiled(&mut engine, &owner, &generation, &source, bounds);
    // Native transfer of w1 out: no send plan, source membership is now
    // exactly the survivor.
    let correlation = CorrelationId::parse("native-source-1").expect("correlation");
    let event = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        revision_of(&engine, &source),
        1,
        &source.0,
        &source.1,
        8,
        &[(
            WindowId("w2".to_owned()),
            bounds,
            tiler_core::size_hints::WindowSizeHints::none(),
        )],
        Some(&WindowId("w2".to_owned())),
    );
    match engine.handle(&event) {
        CoreReply::Tiled(plan) => {
            assert!(
                !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                "dropped mover takes no source geometry"
            );
            let survivor = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w2")
                .expect("survivor reflows");
            assert_eq!(survivor.workspace.0, "ws-1");
            assert!(
                survivor.rect.w > 0 && survivor.rect.h > 0,
                "survivor reflows, got {:?}",
                survivor.rect
            );
        }
        CoreReply::Projection(plan) => {
            assert!(
                !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                "dropped mover takes no source geometry"
            );
            let survivor = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w2")
                .expect("survivor reflows");
            assert_eq!(survivor.workspace.0, "ws-1");
            assert!(
                survivor.rect.w > 0 && survivor.rect.h > 0,
                "survivor reflows, got {:?}",
                survivor.rect
            );
        }
        CoreReply::SendWorkspace(plan) => {
            assert!(
                !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                "dropped mover takes no source geometry"
            );
            assert!(
                plan.geometry.iter().any(|g| g.window.0 == "w2"),
                "survivor reflows"
            );
        }
        other => panic!("source survivor converges, got {other:?}"),
    }
}

#[test]
fn dispatch_gate_covers_off_suspended_unmanaged() {
    use super::super::dispatch_decision;
    assert_eq!(
        dispatch_decision(false, false, false, false),
        Some("takeover-off")
    );
    assert_eq!(
        dispatch_decision(true, true, false, false),
        Some("suspended")
    );
    assert_eq!(
        dispatch_decision(true, false, true, false),
        Some("unmanaged")
    );
    // Select serves empty workspaces and unmanaged foreground.
    assert_eq!(dispatch_decision(true, false, false, false), None);
    // Send with a managed focus dispatches.
    assert_eq!(dispatch_decision(true, false, true, true), None);
}

#[test]
fn domain_rows_union_visible_retained_hidden() {
    use super::super::{MemberView, domain_rows, member_matches};
    let a = key(1);
    let b = key(2);
    let c = key(3);
    let members: BTreeSet<WindowKey> = [a.clone(), b.clone(), c.clone()].into_iter().collect();
    // Visible, retained-minimized (last-known frame), hidden snapshot.
    let views = vec![
        MemberView {
            key: a.clone(),
            token: "w1".to_owned(),
            rect: rect(0, 0),
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        },
        MemberView {
            key: b.clone(),
            token: "w2".to_owned(),
            rect: rect(100, 100),
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        },
        MemberView {
            key: c.clone(),
            token: "w3".to_owned(),
            rect: rect(200, 200),
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        },
    ];
    let rows = domain_rows(&members, &views).expect("complete rows");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].token, "w1");
    // A member without any view defers instead of a falsely complete
    // observation: the caller retains Engine state with no layout loss.
    assert_eq!(domain_rows(&members, &views[..2]), None);
    // Recycled HWND (same hwnd, fresh creation) never matches.
    assert!(member_matches(&a, 1, 1001, "c0000000000000001"));
    assert!(!member_matches(&a, 1, 1001, "c0000000000000002"));
    assert!(!member_matches(&a, 1, 9999, "c0000000000000001"));
    assert!(!member_matches(&a, 9, 1001, "c0000000000000001"));
}

#[test]
fn domain_rows_and_send_carry_born_floating_only() {
    // Slotless born holds ride the Engine as floating exceptions while
    // the tiled member stays tiled; the flag is an Engine observation,
    // never a native write, and send carries it into both domains.
    use super::super::{MemberView, build_send_event, domain_rows};
    let born = key(11);
    let tiled = key(12);
    let members: BTreeSet<WindowKey> = [born.clone(), tiled.clone()].into_iter().collect();
    let views = vec![
        MemberView {
            key: born.clone(),
            token: "w-born".to_owned(),
            rect: rect(0, 0),
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: true,
        },
        MemberView {
            key: tiled.clone(),
            token: "w-tiled".to_owned(),
            rect: rect(100, 100),
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        },
    ];
    let rows = domain_rows(&members, &views).expect("complete rows");
    assert!(
        rows.iter()
            .find(|r| r.token == "w-born")
            .expect("born")
            .floating
    );
    assert!(
        !rows
            .iter()
            .find(|r| r.token == "w-tiled")
            .expect("tiled")
            .floating
    );
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    let event = build_send_event(
        &owner,
        &generation,
        &correlation,
        0,
        3,
        source,
        target,
        &rows,
        &[],
        "w-tiled",
        8,
        true,
    )
    .expect("event");
    assert!(
        event
            .windows
            .iter()
            .find(|w| w.window.0 == "w-born")
            .expect("born")
            .floating
    );
    assert!(
        !event
            .windows
            .iter()
            .find(|w| w.window.0 == "w-tiled")
            .expect("tiled")
            .floating
    );
}

#[test]
fn reused_hwnd_repair_drops_only_unclaimed_stale() {
    use super::super::reused_hwnd_stale;
    use std::collections::BTreeSet;
    let stale = key(1);
    // Same HWND, fresh process: new key plus an unrelated member.
    let fresh = WindowKey {
        hwnd: 1,
        pid: 9000,
        creation: "c000000000009000".to_owned(),
    };
    let other = key(2);
    let known = vec![stale.clone(), fresh.clone(), other.clone()];
    // No claims: exactly the stale key drops.
    assert_eq!(
        reused_hwnd_stale(&known, &BTreeSet::new(), &fresh),
        vec![stale.clone()]
    );
    // A live hidden claim for the stale key owns its HWND through the
    // ledger/audit path: never dropped here.
    let claimed: BTreeSet<WindowKey> = [stale.clone()].into_iter().collect();
    assert!(reused_hwnd_stale(&known, &claimed, &fresh).is_empty());
    // An unknown HWND repairs nothing.
    let unknown = WindowKey {
        hwnd: 99,
        pid: 9000,
        creation: "c000000000009000".to_owned(),
    };
    assert!(reused_hwnd_stale(&known, &BTreeSet::new(), &unknown).is_empty());
}

#[test]
fn visible_lifetime_needs_exact_stored_tag() {
    use super::super::visible_lifetime_ok;
    // Exact match is the only pass: same-process reuse starts without the
    // property (absent) or with the previous generation's tag.
    assert!(visible_lifetime_ok("abc123", Some("abc123")));
    assert!(!visible_lifetime_ok("abc123", Some("abc124")));
    assert!(!visible_lifetime_ok("abc123", None));
    // An untracked window (no stored tag) is never trusted, only stamped.
    assert!(!visible_lifetime_ok("", Some("abc123")));
    assert!(!visible_lifetime_ok("", None));
}

#[test]
fn same_process_reuse_invisible_to_key_guards_but_not_to_tag() {
    use super::super::{member_matches, reused_hwnd_stale, visible_lifetime_ok};
    use std::collections::BTreeSet;
    // Destroy plus same-process recreate: HWND, PID, and creation all
    // still agree, so every (hwnd, pid, creation) guard passes and the
    // cross-process repair sees no stale key. Only the visible lifetime
    // tag distinguishes the new generation.
    let previous = key(1);
    assert!(member_matches(&previous, 1, 1001, "c0000000000000001"));
    let known = vec![previous.clone()];
    assert!(reused_hwnd_stale(&known, &BTreeSet::new(), &previous).is_empty());
    // Previous generation's tag stored; the fresh window carries none, so
    // no write, hide, focus, or Engine row is authorized for it.
    assert!(!visible_lifetime_ok("9f2c41aa07bd33e0", None));
    assert!(!visible_lifetime_ok(
        "9f2c41aa07bd33e0",
        Some("9f2c41aa07bd33e1")
    ));
    assert!(visible_lifetime_ok(
        "9f2c41aa07bd33e0",
        Some("9f2c41aa07bd33e0")
    ));
}

#[test]
fn unified_domain_reuses_session_across_ticks() {
    // Same member, same domain, two ticks: one retained session, no
    // reseed, revision advances monotonically.
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let (domain, key) = workspace_domain("mon-a", "ws-1", bounds, 8);
    for tick in 1..=2u64 {
        let correlation = CorrelationId::parse(&format!("tick-{tick}")).expect("correlation");
        let revision = engine
            .session(&key)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            revision,
            tick,
            &domain,
            &key,
            8,
            &[(
                tiler_core::directional::WindowId("w1".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            )],
            None,
        );
        let _ = engine.handle(&event);
    }
    assert_eq!(engine.len(), 1);
}

#[test]
fn engine_topology_reuses_both_domains() {
    // Both source and target rows (including hidden snapshots) reach the
    // Engine; the planned mutation is topology reuse, never remove/reseed.
    use tiler_core::boundary::CoreReply;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    // Real converge first: source holds two windows, target holds none.
    for (key, rows) in [
        (&source.1, vec![("w1", bounds), ("w2", bounds)]),
        (&target.1, vec![]),
    ] {
        let domain = if key.workspace.0 == "ws-1" {
            &source.0
        } else {
            &target.0
        };
        let correlation = CorrelationId::parse("seed").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            0,
            rows.len() as u64,
            domain,
            key,
            8,
            &rows
                .iter()
                .map(|(t, r)| {
                    (
                        tiler_core::directional::WindowId((*t).to_owned()),
                        *r,
                        tiler_core::size_hints::WindowSizeHints::none(),
                    )
                })
                .collect::<Vec<_>>(),
            None,
        );
        let _ = engine.handle(&event);
    }
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        revision,
        42,
        source.clone(),
        target.clone(),
        &[
            OwnerRow {
                token: "w1".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
            OwnerRow {
                token: "w2".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
        ],
        &[],
        "w1",
        8,
        true,
    )
    .expect("event");
    stamp_send_target(&mut event, &target.1);
    // Typed success: the Engine moved the tile with desired geometry,
    // and both domain sessions exist after the send.
    let reply = engine.handle(&event);
    let CoreReply::SendWorkspace(plan) = reply else {
        panic!("expected SendWorkspace, got {reply:?}");
    };
    assert!(!plan.geometry.is_empty(), "send carries geometry");
    assert!(
        matches!(
            plan.operation,
            tiler_core::contract::LifecycleOperation::MoveTiled { .. }
        ),
        "send is a tiled move, got {:?}",
        plan.operation
    );
    assert!(engine.session(&source.1).is_some(), "source retained");
    assert!(engine.session(&target.1).is_some(), "target retained");
    // Retained-tree roundtrip with a window left behind: the source still
    // converges its remaining window and the target converges the mover.
    for (key, domain, rows) in [
        (&source.1, &event.domain, vec![("w2", bounds)]),
        (
            &target.1,
            event
                .target_domain
                .as_ref()
                .map(|(d, _)| d)
                .expect("target"),
            vec![("w1", bounds)],
        ),
    ] {
        let correlation = CorrelationId::parse("roundtrip").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            engine
                .session(key)
                .map(|s| s.accepted_revision())
                .unwrap_or(0),
            rows.len() as u64,
            domain,
            key,
            engine.outer_gap(key).unwrap_or(8),
            &rows
                .iter()
                .map(|(t, r)| {
                    (
                        tiler_core::directional::WindowId((*t).to_owned()),
                        *r,
                        tiler_core::size_hints::WindowSizeHints::none(),
                    )
                })
                .collect::<Vec<_>>(),
            None,
        );
        let reply = engine.handle(&event);
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "roundtrip converges, got {reply:?}"
        );
    }
}

#[test]
fn send_to_empty_then_product_reconcile_with_fixed_gap() {
    // Send to an empty workspace, then reconcile both sides with the
    // same carried outer gap: the destination must place the mover and
    // the source must reflow its survivor, never `domain-mismatch`.
    use tiler_core::boundary::CoreReply;
    use tiler_core::geometry::Rect;
    fn contains(bounds: Rect, r: Rect) -> bool {
        r.w > 0
            && r.h > 0
            && r.x >= bounds.x
            && r.y >= bounds.y
            && r.x + r.w <= bounds.x + bounds.w
            && r.y + r.h <= bounds.y + bounds.h
    }
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    for (key, rows) in [
        (&source.1, vec![("w1", bounds), ("w2", bounds)]),
        (&target.1, vec![]),
    ] {
        let domain = if key.workspace.0 == "ws-1" {
            &source.0
        } else {
            &target.0
        };
        let correlation = CorrelationId::parse("seed").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            0,
            rows.len() as u64,
            domain,
            key,
            8,
            &rows
                .iter()
                .map(|(t, r)| {
                    (
                        tiler_core::directional::WindowId((*t).to_owned()),
                        *r,
                        tiler_core::size_hints::WindowSizeHints::none(),
                    )
                })
                .collect::<Vec<_>>(),
            None,
        );
        let _ = engine.handle(&event);
    }
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        revision,
        42,
        source.clone(),
        target.clone(),
        &[
            OwnerRow {
                token: "w1".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
            OwnerRow {
                token: "w2".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
        ],
        &[],
        "w1",
        8,
        true,
    )
    .expect("event");
    stamp_send_target(&mut event, &target.1);
    let reply = engine.handle(&event);
    let CoreReply::SendWorkspace(plan) = reply else {
        panic!("send commits, got {reply:?}");
    };
    assert!(
        matches!(
            plan.operation,
            tiler_core::contract::LifecycleOperation::MoveTiled { .. }
        ),
        "send is a tiled move, got {:?}",
        plan.operation
    );
    let mover = plan
        .geometry
        .iter()
        .find(|g| g.window.0 == "w1")
        .expect("send places mover");
    assert_eq!(mover.workspace.0, "ws-2");
    assert!(contains(bounds, mover.rect), "mover inside target");
    assert_eq!(
        engine.outer_gap(&target.1),
        Some(8),
        "new destination inherits carried gap"
    );
    // Destination follow-up with the same carried gap places the mover.
    let correlation = CorrelationId::parse("tick-2").expect("correlation");
    let (target_domain, target_key) = target.clone();
    let event = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        engine
            .session(&target_key)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        1,
        &target_domain,
        &target_key,
        8,
        &[(
            tiler_core::directional::WindowId("w1".to_owned()),
            bounds,
            tiler_core::size_hints::WindowSizeHints::none(),
        )],
        None,
    );
    match engine.handle(&event) {
        CoreReply::Tiled(plan) => {
            let placed = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w1")
                .expect("destination places mover");
            assert_eq!(placed.workspace.0, "ws-2");
            assert!(contains(bounds, placed.rect), "mover inside target");
        }
        CoreReply::Projection(plan) => {
            let placed = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w1")
                .expect("destination places mover");
            assert_eq!(placed.workspace.0, "ws-2");
            assert!(contains(bounds, placed.rect), "mover inside target");
        }
        CoreReply::SendWorkspace(plan) => {
            let placed = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w1")
                .expect("destination places mover");
            assert_eq!(placed.workspace.0, "ws-2");
            assert!(contains(bounds, placed.rect), "mover inside target");
        }
        reply => panic!("destination converges, got {reply:?}"),
    }
    // Source survivor reflows with the same carried gap.
    let correlation = CorrelationId::parse("tick-3").expect("correlation");
    let (source_domain, source_key) = source.clone();
    let event = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        engine
            .session(&source_key)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        1,
        &source_domain,
        &source_key,
        8,
        &[(
            tiler_core::directional::WindowId("w2".to_owned()),
            bounds,
            tiler_core::size_hints::WindowSizeHints::none(),
        )],
        None,
    );
    match engine.handle(&event) {
        CoreReply::Tiled(plan) => {
            let survivor = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w2")
                .expect("source reflows survivor");
            assert_eq!(survivor.workspace.0, "ws-1");
            assert!(contains(bounds, survivor.rect), "survivor inside source");
        }
        CoreReply::Projection(plan) => {
            let survivor = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w2")
                .expect("source reflows survivor");
            assert_eq!(survivor.workspace.0, "ws-1");
            assert!(contains(bounds, survivor.rect), "survivor inside source");
        }
        CoreReply::SendWorkspace(plan) => {
            let survivor = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w2")
                .expect("source reflows survivor");
            assert_eq!(survivor.workspace.0, "ws-1");
            assert!(contains(bounds, survivor.rect), "survivor inside source");
        }
        reply => panic!("source converges, got {reply:?}"),
    }
}

#[test]
fn send_plan_scopes_source_reflow_through_writable_subset() {
    // Both-domain send regression through the actual native seams: one
    // Engine `SendWorkspace` plan carries source survivor reflow plus
    // target mover placement together (`planned_writes`, the extraction
    // the native write path consumes), and the domain writable set
    // (`writable_subset`, which the native `writable_tokens` delegates to)
    // scopes the pre-hide source pass to the survivor only.
    use tiler_core::boundary::CoreReply;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    for (key, rows) in [
        (&source.1, vec![("w1", bounds), ("w2", bounds)]),
        (&target.1, vec![]),
    ] {
        let domain = if key.workspace.0 == "ws-1" {
            &source.0
        } else {
            &target.0
        };
        let correlation = CorrelationId::parse("seed").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            0,
            rows.len() as u64,
            domain,
            key,
            8,
            &rows
                .iter()
                .map(|(t, r)| {
                    (
                        tiler_core::directional::WindowId((*t).to_owned()),
                        *r,
                        tiler_core::size_hints::WindowSizeHints::none(),
                    )
                })
                .collect::<Vec<_>>(),
            None,
        );
        let _ = engine.handle(&event);
    }
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        revision,
        42,
        source.clone(),
        target.clone(),
        &[
            OwnerRow {
                token: "w1".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
            OwnerRow {
                token: "w2".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
        ],
        &[],
        "w1",
        8,
        true,
    )
    .expect("event");
    stamp_send_target(&mut event, &target.1);
    let reply = engine.handle(&event);
    let CoreReply::SendWorkspace(plan) = &reply else {
        panic!("send commits");
    };
    // Native extraction seam: the single plan covers both domains.
    let writes = super::super::planned_writes(&reply).expect("plan extracts");
    assert_eq!(writes.len(), plan.geometry.len(), "nothing dropped");
    assert!(writes.iter().any(|w| w.window.0 == "w1"), "mover placed");
    assert!(
        writes.iter().any(|w| w.window.0 == "w2"),
        "survivor reflowed"
    );
    // Domain eligibility seam: after the membership transfer the source
    // domain holds only the survivor, so the pre-hide source pass writes
    // exactly the survivor while the moved token is no longer writable
    // there. Hidden members never take writes.
    let mover = key(1);
    let survivor = key(2);
    let hidden = key(3);
    let members: BTreeSet<WindowKey> = [mover.clone(), survivor.clone(), hidden.clone()]
        .into_iter()
        .collect();
    let token_of: std::collections::BTreeMap<WindowKey, String> = [
        (mover.clone(), "w1".to_owned()),
        (survivor.clone(), "w2".to_owned()),
        (hidden.clone(), "w3".to_owned()),
    ]
    .into_iter()
    .collect();
    let hidden_set: BTreeSet<WindowKey> = [hidden.clone()].into_iter().collect();
    // Fresh observation holds survivor plus mover (still visible pre-hide
    // in this portable model); the source pass scopes by membership, so
    // model the post-transfer source membership explicitly.
    let source_members: BTreeSet<WindowKey> =
        [survivor.clone(), hidden.clone()].into_iter().collect();
    let fresh: std::collections::HashSet<String> =
        ["w1".to_owned(), "w2".to_owned()].into_iter().collect();
    let writable = super::super::writable_subset(
        &source_members,
        |k| hidden_set.contains(k),
        &token_of,
        &fresh,
    );
    assert!(writable.contains("w2"), "survivor stays writable");
    assert!(
        !writable.contains("w1"),
        "moved token not writable in source"
    );
    assert!(!writable.contains("w3"), "hidden rows never writable");
    let _ = members;
    // Non-plan replies extract to `None` so callers serialize null with
    // reason instead of zeros.
    let released = tiler_core::boundary::CoreReply::Released;
    assert!(super::super::planned_writes(&released).is_none());
}

#[test]
fn non_plan_replies_extract_to_none() {
    use tiler_core::boundary::{CoreReply, NoGroupReason};
    use tiler_core::contract::DivergenceKind;
    for reply in [
        CoreReply::Released,
        CoreReply::Rejected {
            kind: "empty",
            message: "empty domain",
        },
        CoreReply::Diverged(DivergenceKind::StaleRevision),
        CoreReply::SnapshotInvalid {
            message: "stale",
            detail: "stale snapshot",
        },
        CoreReply::NoGroup {
            base_revision: None,
            reason: NoGroupReason::NoSession,
        },
    ] {
        assert!(super::super::planned_writes(&reply).is_none(), "{reply:?}");
    }
}

#[test]
fn focus_before_geometry_gate_blocks_real_fullscreen_and_elevated() {
    use super::super::focus_before_geometry;
    // Verified transition with a clean foreground focuses before geometry.
    assert!(focus_before_geometry(true, false, false));
    // A real fullscreen or elevated arrival must never be stolen from.
    assert!(!focus_before_geometry(true, true, false));
    assert!(!focus_before_geometry(true, false, true));
    assert!(!focus_before_geometry(true, true, true));
    // An unverified transition never focuses first, even when clean.
    assert!(!focus_before_geometry(false, false, false));
}

#[test]
fn writable_subset_never_includes_hidden_or_stale() {
    use super::super::writable_subset;
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;
    let a = key(1);
    let b = key(2);
    let c = key(3);
    let members: BTreeSet<WindowKey> = [a.clone(), b.clone(), c.clone()].into_iter().collect();
    let token_of: BTreeMap<WindowKey, String> = [
        (a.clone(), "w1".to_owned()),
        (b.clone(), "w2".to_owned()),
        (c.clone(), "w3".to_owned()),
    ]
    .into_iter()
    .collect();
    // b is hidden; c has no fresh frame (retained without observation).
    let hidden: BTreeSet<WindowKey> = [b.clone()].into_iter().collect();
    let fresh: std::collections::HashSet<String> =
        ["w1".to_owned(), "w2".to_owned()].into_iter().collect();
    let writable = writable_subset(&members, |k| hidden.contains(k), &token_of, &fresh);
    assert!(
        writable.contains("w1"),
        "visible fresh member stays writable"
    );
    assert!(!writable.contains("w2"), "hidden rows never writable");
    assert!(!writable.contains("w3"), "retained rows never writable");
}

#[test]
fn foreground_classifier_keeps_gates_but_fixes_desktop_and_hidden() {
    use super::super::{ForegroundFacts, ForegroundVetoReason, classify_foreground};
    let facts = |valid: bool,
                 is_desktop: bool,
                 visible: bool,
                 captioned: bool,
                 dwm_readable: bool,
                 cloak_readable: bool,
                 cloaked: bool,
                 covers_monitor: bool| {
        classify_foreground(ForegroundFacts {
            valid,
            is_desktop,
            visible,
            captioned,
            dwm_readable,
            cloak_readable,
            cloaked,
            covers_monitor,
        })
    };
    // Visible real fullscreen still vetoes.
    let v = facts(true, false, true, false, true, true, false, true);
    assert!(v.block && v.reason == ForegroundVetoReason::Fullscreen);
    // Visible unreadable foreground stays blocked (fail closed).
    let v = facts(true, false, true, false, false, true, false, false);
    assert!(v.block && v.reason == ForegroundVetoReason::Unreadable);
    // Unreadable cloak state stays blocked (fail closed): an unknown
    // cloak never clears a covering frame.
    let v = facts(true, false, true, false, true, false, false, true);
    assert!(v.block && v.reason == ForegroundVetoReason::Unreadable);
    // Cloaked captionless monitor cover never vetoes: invisible to the
    // compositor, so it cannot be a covering fullscreen.
    let v = facts(true, false, true, false, true, true, true, true);
    assert!(!v.block && v.reason == ForegroundVetoReason::Cloaked);
    // Cloaked small frame never vetoes either.
    let v = facts(true, false, true, false, true, true, true, false);
    assert!(!v.block && v.reason == ForegroundVetoReason::Cloaked);
    // Invalid handle fails closed.
    let v = facts(false, false, false, false, false, false, false, false);
    assert!(v.block && v.reason == ForegroundVetoReason::Invalid);
    // Exact desktop shell handle never vetoes, even when covering.
    let v = facts(true, true, true, false, true, true, false, true);
    assert!(!v.block && v.reason == ForegroundVetoReason::Desktop);
    // Valid non-visible foreground never vetoes: invisible cannot cover.
    let v = facts(true, false, false, false, true, true, false, true);
    assert!(!v.block && v.reason == ForegroundVetoReason::Nonvisible);
    // Captioned foreground never vetoes.
    let v = facts(true, false, true, true, true, true, false, false);
    assert!(!v.block && v.reason == ForegroundVetoReason::Captioned);
    // Small visible borderless window never vetoes.
    let v = facts(true, false, true, false, true, true, false, false);
    assert!(!v.block && v.reason == ForegroundVetoReason::None);
}

#[test]
fn managed_overlay_reason_is_opaque_and_distinct() {
    // The managed-overlay bypass (verified member or born-held track)
    // carries its own bounded token, never the raw fullscreen one, so
    // lifecycle logs stay distinguishable. The classifier itself never
    // produces it: only the native managed check may.
    use super::super::ForegroundVetoReason;
    assert_eq!(
        ForegroundVetoReason::ManagedOverlay.as_str(),
        "managed-overlay"
    );
    assert_ne!(
        ForegroundVetoReason::ManagedOverlay,
        ForegroundVetoReason::Fullscreen
    );
}
