//! Direct tests for whole-workspace migration over [`WorldLayout`].

use std::collections::BTreeMap;

use crate::contract::MAX_REVISION;
use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::{LayoutFailure, WorkspaceMigrationFailure, WorkspaceMigrationOutcome};
use super::state::{DomainException, DomainLayout, WorldLayout};

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn domain_desc(output: &str, workspace: &str, bounds: Rect, gap: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds,
        gap,
        adjacent: BTreeMap::new(),
    }
}

fn node(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn window(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn leaf(id: &str) -> Node {
    Node::Leaf { id: node(id) }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn observed_window(id: &str, output: &str, workspace: &str) -> EngineWindow {
    EngineWindow {
        window: window(id),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: rect(0, 0, 200, 150),
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

fn two_tile_plus_float_layout() -> (WorldLayout, OutputDomain, OutputDomain) {
    let mut layout = WorldLayout::new();
    let src_bounds = rect(0, 0, 1000, 800);
    let tgt_bounds = rect(1000, 0, 1600, 900);
    let src = domain_desc("out-1", "ws-2", src_bounds, 8);
    let tgt = domain_desc("out-2", "ws-2", tgt_bounds, 12);

    let mut domain = DomainLayout::new(src_bounds, 8, 10);
    domain.tree = Some(Node::Group {
        id: node("grp-0"),
        axis: Axis::Horizontal,
        children: vec![leaf("leaf-a"), leaf("leaf-b")],
        shares: vec![3, 1],
    });
    domain.leaves.insert(window("win-a"), node("leaf-a"));
    domain.leaves.insert(window("win-b"), node("leaf-b"));
    domain.focused_leaf = Some(node("leaf-a"));
    domain.focus_stack = vec![node("leaf-b"), node("leaf-a")];
    domain.focus_history = vec![node("leaf-b"), node("leaf-a")];
    domain.last_active = Some(node("leaf-a"));
    domain.accepted_revision = 4;
    let float_rect = rect(120, 90, 400, 300);
    domain.exceptions.insert(
        window("win-f"),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                ..ExceptionFlags::none()
            },
            floating_geometry: Some(float_rect),
        },
    );
    domain
        .retained_float_geometry
        .insert(window("win-f"), float_rect);
    domain.admission_order.insert(window("win-a"), 0);
    domain.admission_order.insert(window("win-b"), 1);
    layout.admission_seq_next = 2;
    layout.domains.insert(src.key(), domain);
    (layout, src, tgt)
}

#[test]
fn topology_shares_focus_and_float_carry() {
    let (mut layout, src, tgt) = two_tile_plus_float_layout();
    let mut float_obs = observed_window("win-f", "out-1", "ws-2");
    float_obs.floating = true;
    let observed = vec![
        observed_window("win-a", "out-1", "ws-2"),
        observed_window("win-b", "out-1", "ws-2"),
        float_obs,
    ];

    let outcome = layout
        .migrate_workspace(&src, 10, &window("win-a"), &observed, &tgt)
        .expect("migration should succeed");

    assert_eq!(
        outcome,
        WorkspaceMigrationOutcome {
            members: vec![window("win-a"), window("win-b"), window("win-f")],
            floats: 1,
        }
    );
    assert!(!layout.contains_domain(&src.key()));
    let moved = layout.domain(&tgt.key()).expect("target installed");
    assert_eq!(moved.bounds, tgt.bounds);
    assert_eq!(moved.gap, tgt.gap);
    assert_eq!(moved.outer_gap, 10);
    assert_eq!(moved.accepted_revision, 4);
    assert_eq!(moved.focused_leaf, Some(node("leaf-a")));
    match moved.tree.as_ref().expect("tree preserved") {
        Node::Group { shares, .. } => assert_eq!(shares, &[3, 1]),
        other => panic!("expected group, got {other:?}"),
    }
    assert_eq!(
        moved
            .exceptions
            .get(&window("win-f"))
            .and_then(|e| e.floating_geometry),
        Some(rect(120, 90, 400, 300))
    );
}

#[test]
fn sticky_residual_and_slot_lifetimes() {
    let (mut layout, src, tgt) = two_tile_plus_float_layout();
    // Remove win-f from retained for a clean 2-tile + sticky + newcomer case.
    layout
        .domains
        .get_mut(&src.key())
        .unwrap()
        .exceptions
        .clear();

    let mut sticky_b = observed_window("win-b", "out-1", "ws-2");
    sticky_b.sticky = true;
    let observed = vec![
        observed_window("win-a", "out-1", "ws-2"),
        sticky_b,
        observed_window("win-c", "out-1", "ws-2"),
    ];

    let outcome = layout
        .migrate_workspace(&src, 10, &window("win-b"), &observed, &tgt)
        .expect("sticky split migration should succeed");

    assert_eq!(
        outcome,
        WorkspaceMigrationOutcome {
            members: vec![window("win-a"), window("win-c")],
            floats: 0,
        }
    );
    let moved = layout.domain(&tgt.key()).expect("target installed");
    let stay = layout.domain(&src.key()).expect("sticky residual stays");
    assert_eq!(moved.outer_gap, 10);
    assert_eq!(stay.outer_gap, 10);
    assert!(moved.leaves.contains_key(&window("win-a")));
    assert!(moved.leaves.contains_key(&window("win-c")));
    assert!(moved.minimized_slots.contains_key(&window("win-b")));
    assert!(stay.leaves.contains_key(&window("win-b")));
    assert!(stay.minimized_slots.contains_key(&window("win-a")));
    assert_eq!(layout.admission_seq_next, 3);

    // All-sticky float-only source retires target (no target domain inserted)
    // and keeps sticky residual on source.
    let mut float_only = WorldLayout::new();
    let mut fdom = DomainLayout::new(src.bounds, src.gap, 10);
    fdom.exceptions.insert(
        window("win-s"),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                ..ExceptionFlags::none()
            },
            floating_geometry: None,
        },
    );
    float_only.domains.insert(src.key(), fdom);
    let mut sticky_s = observed_window("win-s", "out-1", "ws-2");
    sticky_s.floating = true;
    sticky_s.sticky = true;
    let res = float_only
        .migrate_workspace(&src, 10, &window("win-s"), &[sticky_s], &tgt)
        .expect("all-sticky float-only succeeds");
    assert_eq!(
        res,
        WorkspaceMigrationOutcome {
            members: Vec::new(),
            floats: 0,
        }
    );
    assert!(!float_only.contains_domain(&tgt.key()));
    assert!(float_only.contains_domain(&src.key()));
}

#[test]
fn absent_and_retained_empty() {
    let mut layout = WorldLayout::new();
    let src = domain_desc("out-1", "ws-2", rect(0, 0, 1000, 800), 8);
    let tgt = domain_desc("out-2", "ws-2", rect(1000, 0, 1000, 800), 8);

    // Absent source + empty observation + empty focus -> Ok empty, no state inserted.
    let ok = layout
        .migrate_workspace(&src, 8, &window(""), &[], &tgt)
        .expect("empty absent succeeds");
    assert_eq!(
        ok,
        WorkspaceMigrationOutcome {
            members: Vec::new(),
            floats: 0,
        }
    );
    assert!(layout.domains.is_empty());

    // Absent source + non-empty focus -> UnknownDomain.
    let err = layout
        .migrate_workspace(&src, 8, &window("win-a"), &[], &tgt)
        .unwrap_err();
    assert_eq!(
        err,
        WorkspaceMigrationFailure::Layout(LayoutFailure::Semantics(RefusalKind::UnknownDomain))
    );

    // Retained-empty source + empty observation -> retires source.
    layout
        .domains
        .insert(src.key(), DomainLayout::new(src.bounds, src.gap, 8));
    let retired = layout
        .migrate_workspace(&src, 8, &window(""), &[], &tgt)
        .expect("retained-empty retires");
    assert_eq!(
        retired,
        WorkspaceMigrationOutcome {
            members: Vec::new(),
            floats: 0,
        }
    );
    assert!(!layout.contains_domain(&src.key()));
    assert!(!layout.contains_domain(&tgt.key()));
}

#[test]
fn refusal_priority_and_occupied_target() {
    let (mut layout, src, tgt) = two_tile_plus_float_layout();
    // Occupy target so Unchanged is armed.
    layout
        .domains
        .insert(tgt.key(), DomainLayout::new(tgt.bounds, tgt.gap, 10));

    let mut overlay = observed_window("win-a", "out-1", "ws-2");
    overlay.fit_excluded = true;

    // 1. SameOutput beats OverlayPresent and OccupiedTarget.
    assert_eq!(
        layout
            .migrate_workspace(&src, 99, &window(""), &[overlay.clone()], &src)
            .unwrap_err(),
        WorkspaceMigrationFailure::SameOutput
    );

    // 2. OverlayPresent beats OccupiedTarget and invalid outer_gap.
    assert_eq!(
        layout
            .migrate_workspace(&src, 99, &window(""), &[overlay], &tgt)
            .unwrap_err(),
        WorkspaceMigrationFailure::OverlayPresent
    );

    // 3. OccupiedTarget (Unchanged) beats invalid outer_gap.
    assert_eq!(
        layout
            .migrate_workspace(&src, 99, &window(""), &[], &tgt)
            .unwrap_err(),
        WorkspaceMigrationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );

    // Clear target for remaining priority checks.
    layout.domains.remove(&tgt.key());

    // 4. Out-of-range outer_gap (DomainMismatch) beats Absent/UnknownDomain.
    let absent_src = domain_desc("out-3", "ws-2", src.bounds, src.gap);
    assert_eq!(
        layout
            .migrate_workspace(&absent_src, 65, &window("win-a"), &[], &tgt)
            .unwrap_err(),
        WorkspaceMigrationFailure::DomainMismatch
    );

    // 5. Retained outer_gap/bounds/gap mismatch beats CrossDomainMismatch/PartialObservation.
    let off_domain = observed_window("win-a", "out-9", "ws-2");
    assert_eq!(
        layout
            .migrate_workspace(&src, 12, &window(""), &[off_domain.clone()], &tgt)
            .unwrap_err(),
        WorkspaceMigrationFailure::DomainMismatch
    );

    // 6. CrossDomainMismatch beats PartialObservation.
    assert_eq!(
        layout
            .migrate_workspace(&src, 10, &window(""), &[off_domain], &tgt)
            .unwrap_err(),
        WorkspaceMigrationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::CrossDomainMismatch
        ))
    );

    // 7. PartialObservation beats FocusMismatch.
    assert_eq!(
        layout
            .migrate_workspace(
                &src,
                10,
                &window("dangling"),
                &[observed_window("win-a", "out-1", "ws-2")],
                &tgt,
            )
            .unwrap_err(),
        WorkspaceMigrationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::PartialObservation
        ))
    );

    // 8. FocusMismatch when all retained are observed but focus is not.
    let mut float_obs = observed_window("win-f", "out-1", "ws-2");
    float_obs.floating = true;
    let full_obs = vec![
        observed_window("win-a", "out-1", "ws-2"),
        observed_window("win-b", "out-1", "ws-2"),
        float_obs,
    ];
    assert_eq!(
        layout
            .migrate_workspace(&src, 10, &window("dangling"), &full_obs, &tgt)
            .unwrap_err(),
        WorkspaceMigrationFailure::Layout(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
    );
}

#[test]
fn revision_limit_and_later_refusal_atomicity_with_allocator_burns() {
    let (mut layout, src, tgt) = two_tile_plus_float_layout();
    {
        let dom = layout.domains.get_mut(&src.key()).unwrap();
        dom.exceptions.clear();
        dom.accepted_revision = MAX_REVISION;
    }
    let mut sticky_b = observed_window("win-b", "out-1", "ws-2");
    sticky_b.sticky = true;
    let obs = vec![
        observed_window("win-a", "out-1", "ws-2"),
        sticky_b,
        observed_window("win-c", "out-1", "ws-2"),
    ];
    let alloc_before = layout.group_alloc.next_value();
    let seq_before = layout.admission_seq_next;

    // `moved` admits newcomer `win-c` (burning group allocator serials) and hits
    // MAX_REVISION -> no domain or admission_seq_next mutation, burns survive.
    let err = layout
        .migrate_workspace(&src, 10, &window("win-a"), &obs, &tgt)
        .unwrap_err();
    assert_eq!(
        err,
        WorkspaceMigrationFailure::Layout(LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        })
    );
    assert!(layout.group_alloc.next_value() > alloc_before);
    assert_eq!(layout.admission_seq_next, seq_before);
    assert!(layout.contains_domain(&src.key()));
    assert!(!layout.contains_domain(&tgt.key()));
    assert_eq!(layout.domain(&src.key()).unwrap().leaves.len(), 2);

    // Later target relocation refusal (invalid target gap) after `moved`
    // converged newcomer `win-c`: still atomic, allocator burns survive.
    layout
        .domains
        .get_mut(&src.key())
        .unwrap()
        .accepted_revision = 4;
    let bad_tgt = domain_desc("out-2", "ws-2", tgt.bounds, 99);
    let alloc_before_reloc = layout.group_alloc.next_value();
    let err_reloc = layout
        .migrate_workspace(&src, 10, &window("win-a"), &obs, &bad_tgt)
        .unwrap_err();
    assert_eq!(
        err_reloc,
        WorkspaceMigrationFailure::Layout(LayoutFailure::Semantics(RefusalKind::MalformedTopology))
    );
    assert!(layout.group_alloc.next_value() > alloc_before_reloc);
    assert_eq!(layout.admission_seq_next, seq_before);
    assert!(layout.contains_domain(&src.key()));
    assert!(!layout.contains_domain(&key("out-2", "ws-2")));
}

#[test]
fn cross_native_workspace_migration_and_same_output_refusal() {
    let (mut layout, src, _) = two_tile_plus_float_layout();
    let cross_ws_tgt = domain_desc("out-2", "ws-9", rect(1000, 0, 1600, 900), 12);
    let mut float_obs = observed_window("win-f", "out-1", "ws-2");
    float_obs.floating = true;
    let observed = vec![
        observed_window("win-a", "out-1", "ws-2"),
        observed_window("win-b", "out-1", "ws-2"),
        float_obs,
    ];

    // Same-output cross-workspace target refuses.
    let same_out = domain_desc("out-1", "ws-9", src.bounds, src.gap);
    assert_eq!(
        layout
            .migrate_workspace(&src, 10, &window("win-a"), &observed, &same_out)
            .unwrap_err(),
        WorkspaceMigrationFailure::SameOutput
    );

    // Cross-output cross-workspace target succeeds and rehomes exceptions/focus.
    let outcome = layout
        .migrate_workspace(&src, 10, &window("win-a"), &observed, &cross_ws_tgt)
        .expect("cross-native migration succeeds");
    assert_eq!(outcome.members.len(), 3);
    assert!(!layout.contains_domain(&src.key()));
    let moved = layout.domain(&cross_ws_tgt.key()).expect("target present");
    assert_eq!(moved.focused_leaf, Some(node("leaf-a")));
    assert_eq!(
        layout
            .exception_record(&cross_ws_tgt.key(), &window("win-f"))
            .expect("exception homed on target")
            .workspace,
        WorkspaceId("ws-9".to_owned())
    );
}

#[test]
fn float_focus_float_only_and_absent_active_migration() {
    // 1. Active float focus preserves stored tiled focus without fabricating tile focus.
    {
        let (mut layout, src, tgt) = two_tile_plus_float_layout();
        let mut float_obs = observed_window("win-f", "out-1", "ws-2");
        float_obs.floating = true;
        let observed = vec![
            observed_window("win-a", "out-1", "ws-2"),
            observed_window("win-b", "out-1", "ws-2"),
            float_obs,
        ];
        let outcome = layout
            .migrate_workspace(&src, 10, &window("win-f"), &observed, &tgt)
            .expect("active-float migration succeeds");
        assert_eq!(outcome.floats, 1);
        let moved = layout.domain(&tgt.key()).expect("target installed");
        assert_eq!(moved.focused_leaf, Some(node("leaf-a")));
        assert!(moved.exceptions.contains_key(&window("win-f")));
    }

    // 2. Non-sticky float-only source carries exceptions to target and retires source.
    {
        let mut layout = WorldLayout::new();
        let src = domain_desc("out-1", "ws-2", rect(0, 0, 1000, 800), 8);
        let tgt = domain_desc("out-2", "ws-2", rect(1000, 0, 1600, 900), 12);
        let mut fdom = DomainLayout::new(src.bounds, src.gap, 10);
        for name in ["win-a", "win-b"] {
            fdom.exceptions.insert(
                window(name),
                DomainException {
                    flags: ExceptionFlags {
                        floating: true,
                        ..ExceptionFlags::none()
                    },
                    floating_geometry: None,
                },
            );
        }
        layout.domains.insert(src.key(), fdom);
        let mut fa = observed_window("win-a", "out-1", "ws-2");
        fa.floating = true;
        let mut fb = observed_window("win-b", "out-1", "ws-2");
        fb.floating = true;
        let outcome = layout
            .migrate_workspace(&src, 10, &window("win-a"), &[fa, fb], &tgt)
            .expect("float-only migration succeeds");
        assert_eq!(outcome.members, vec![window("win-a"), window("win-b")]);
        assert_eq!(outcome.floats, 2);
        assert!(!layout.contains_domain(&src.key()));
        let moved = layout.domain(&tgt.key()).expect("target installed");
        assert!(moved.exceptions.contains_key(&window("win-a")));
        assert!(moved.exceptions.contains_key(&window("win-b")));
    }

    // 3. Absent active client (empty focus with members) still migrates all members.
    {
        let (mut layout, src, tgt) = two_tile_plus_float_layout();
        layout
            .domains
            .get_mut(&src.key())
            .unwrap()
            .exceptions
            .clear();
        let observed = vec![
            observed_window("win-a", "out-1", "ws-2"),
            observed_window("win-b", "out-1", "ws-2"),
        ];
        let outcome = layout
            .migrate_workspace(&src, 10, &window(""), &observed, &tgt)
            .expect("absent active client migration succeeds");
        assert_eq!(outcome.members, vec![window("win-a"), window("win-b")]);
        assert!(!layout.contains_domain(&src.key()));
        assert!(layout.contains_domain(&tgt.key()));
    }
}

#[test]
fn fullscreen_and_maximized_d8_carry() {
    // Fullscreen tiled member carries with its tiled slot preserved.
    {
        let (mut layout, src, tgt) = two_tile_plus_float_layout();
        layout
            .domains
            .get_mut(&src.key())
            .unwrap()
            .exceptions
            .clear();
        let mut fs = observed_window("win-a", "out-1", "ws-2");
        fs.fullscreen = true;
        fs.fit_excluded = true;
        let outcome = layout
            .migrate_workspace(
                &src,
                10,
                &window("win-a"),
                &[fs, observed_window("win-b", "out-1", "ws-2")],
                &tgt,
            )
            .expect("fullscreen carry succeeds");
        assert_eq!(outcome.members, vec![window("win-a"), window("win-b")]);
        let moved = layout.domain(&tgt.key()).expect("target installed");
        assert!(moved.leaves.contains_key(&window("win-a")));
    }

    // Maximized float carries with its exception slot preserved.
    {
        let (mut layout, src, tgt) = two_tile_plus_float_layout();
        layout
            .domains
            .get_mut(&src.key())
            .unwrap()
            .exceptions
            .clear();
        let mut max_float = observed_window("win-a", "out-1", "ws-2");
        max_float.floating = true;
        max_float.maximized = true;
        let outcome = layout
            .migrate_workspace(
                &src,
                10,
                &window("win-a"),
                &[max_float, observed_window("win-b", "out-1", "ws-2")],
                &tgt,
            )
            .expect("maximized float carry succeeds");
        assert_eq!(outcome.floats, 1);
        let moved = layout.domain(&tgt.key()).expect("target installed");
        assert!(moved.exceptions.contains_key(&window("win-a")));
    }

    // Maximized tiled carries with its reserved tile slot preserved.
    {
        let (mut layout, src, tgt) = two_tile_plus_float_layout();
        layout
            .domains
            .get_mut(&src.key())
            .unwrap()
            .exceptions
            .clear();
        let mut max_tiled = observed_window("win-b", "out-1", "ws-2");
        max_tiled.maximized = true;
        max_tiled.fit_excluded = true;
        let outcome = layout
            .migrate_workspace(
                &src,
                10,
                &window("win-a"),
                &[observed_window("win-a", "out-1", "ws-2"), max_tiled],
                &tgt,
            )
            .expect("maximized tiled carry succeeds");
        assert_eq!(outcome.floats, 0);
        let moved = layout.domain(&tgt.key()).expect("target installed");
        assert_eq!(moved.leaves.len(), 2);
        assert!(!layout.contains_domain(&src.key()));
    }
}

#[test]
fn retained_empty_adopts_carried_newcomers() {
    let mut layout = WorldLayout::new();
    let src = domain_desc("out-1", "ws-2", rect(0, 0, 1000, 800), 8);
    let tgt = domain_desc("out-2", "ws-2", rect(1000, 0, 1600, 900), 12);
    layout
        .domains
        .insert(src.key(), DomainLayout::new(src.bounds, src.gap, 10));

    let observed = vec![
        observed_window("win-1", "out-1", "ws-2"),
        observed_window("win-2", "out-1", "ws-2"),
    ];
    let outcome = layout
        .migrate_workspace(&src, 10, &window("win-1"), &observed, &tgt)
        .expect("retained-empty newcomers adopt and migrate");
    assert_eq!(outcome.members, vec![window("win-1"), window("win-2")]);
    assert!(!layout.contains_domain(&src.key()));
    let moved = layout.domain(&tgt.key()).expect("target installed");
    assert_eq!(moved.leaves.len(), 2);
}
