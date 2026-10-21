//! Bootstrap restoration ingress: persisted rings/views/tiling/homing are
//! adopted on the first report only and retained (never fresh placement,
//! admission, or focus). Malformed or stale ingress refuses transactionally
//! without mutation; genuinely new lifetimes after a restoration bootstrap
//! still focus once through the ordinary admission path.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    RestoredMember, RestoredRing, RestoredTiling, RestoredWorld, ScopeAuthority, ScreenReport,
    ScreenView, TilingMode, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_full(output: &str, full: Rect, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(full),
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
    }
}

fn restored_world() -> RestoredWorld {
    RestoredWorld {
        rings: vec![
            RestoredRing {
                output: OutputId("out-a".to_owned()),
                workspaces: vec!["ws-1".to_owned(), "ws-2".to_owned()],
            },
            RestoredRing {
                output: OutputId("out-b".to_owned()),
                workspaces: vec!["ws-3".to_owned(), "ws-4".to_owned()],
            },
        ],
        views: vec![
            ScreenView {
                output: OutputId("out-a".to_owned()),
                view: "ws-1".to_owned(),
            },
            ScreenView {
                output: OutputId("out-b".to_owned()),
                view: "ws-3".to_owned(),
            },
        ],
        tiling: vec![
            RestoredTiling {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
                tiled: true,
            },
            RestoredTiling {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-2".to_owned(),
                tiled: false,
            },
            RestoredTiling {
                output: OutputId("out-b".to_owned()),
                workspace: "ws-3".to_owned(),
                tiled: true,
            },
            RestoredTiling {
                output: OutputId("out-b".to_owned()),
                workspace: "ws-4".to_owned(),
                tiled: true,
            },
        ],
        members: vec![
            RestoredMember {
                window: WindowId("win-tiled".to_owned()),
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
            },
            RestoredMember {
                window: WindowId("win-float".to_owned()),
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
            },
            RestoredMember {
                window: WindowId("win-sticky".to_owned()),
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
            },
            RestoredMember {
                window: WindowId("win-min".to_owned()),
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
            },
            RestoredMember {
                window: WindowId("win-full".to_owned()),
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
            },
            RestoredMember {
                window: WindowId("win-b".to_owned()),
                output: OutputId("out-b".to_owned()),
                workspace: "ws-3".to_owned(),
            },
        ],
    }
}

fn restored_facts() -> Vec<WindowFact> {
    // Capture-faithful flags: the adapter floats sticky windows (sticky
    // wins) and marks every non-ordinary row fit-excluded, so the core
    // converges them as retained exceptions instead of fresh tiles.
    let mut floating = fact("win-float", "out-a", 100);
    floating.floating = true;
    floating.fit_excluded = true;
    let mut sticky = fact("win-sticky", "out-a", 200);
    sticky.floating = true;
    sticky.sticky = true;
    sticky.fit_excluded = true;
    let minimized = WindowFact {
        minimized: true,
        rect: None,
        ..fact("win-min", "out-a", 300)
    };
    let mut fullscreen = fact("win-full", "out-a", 400);
    fullscreen.fullscreen = true;
    fullscreen.fit_excluded = true;
    vec![
        fact("win-tiled", "out-a", 0),
        floating,
        sticky,
        minimized,
        fullscreen,
        fact("win-b", "out-b", 500),
    ]
}

fn screens() -> Vec<ScreenReport> {
    vec![
        screen_full("out-a", rect(0, 0, 800, 600), true),
        screen_full("out-b", rect(800, 0, 800, 600), false),
    ]
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    restored: Option<RestoredWorld>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: Vec::new(),
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        // Opposite of the restored ws-2 tiling below: the floating restore
        // proves adoption instead of coinciding with the default.
        default_tiled: true,
        focused_global: focused.to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
        restored,
    }
}

fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReport,
) -> tiler_core::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn row_of<'a>(
    reply: &'a tiler_core::world_report::WorldReply,
    window: &str,
) -> &'a tiler_core::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

fn workspace_mode_of(reply: &tiler_core::world_report::WorldReply, id: &str) -> TilingMode {
    reply
        .workspaces
        .iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("missing workspace {id}"))
        .tiling_mode
}

fn ring_of(reply: &tiler_core::world_report::WorldReply, output: &str) -> Vec<String> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output)
        .map(|row| row.id.clone())
        .collect()
}

fn output_view(reply: &tiler_core::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

#[test]
fn restoration_bootstrap_retains_rings_views_tiling_and_membership() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    // The focused tiled member lets the fresh floating-aware build project
    // the retained exceptions, exactly like a live bootstrap report.
    let bootstrap = || {
        report(
            1,
            1,
            screens(),
            restored_facts(),
            "win-tiled",
            Some(restored_world()),
            None,
            None,
        )
    };
    let reply = accepted(&mut engine, &bootstrap());
    assert!(reply.ready);
    // Rings, views, and per-workspace tiling match the persisted restore.
    assert_eq!(ring_of(&reply, "out-a"), vec!["ws-1", "ws-2"]);
    assert_eq!(ring_of(&reply, "out-b"), vec!["ws-3", "ws-4"]);
    assert_eq!(output_view(&reply, "out-a").as_deref(), Some("ws-1"));
    assert_eq!(output_view(&reply, "out-b").as_deref(), Some("ws-3"));
    assert_eq!(workspace_mode_of(&reply, "ws-1"), TilingMode::Tiled);
    assert_eq!(workspace_mode_of(&reply, "ws-2"), TilingMode::Floating);
    assert_eq!(workspace_mode_of(&reply, "ws-3"), TilingMode::Tiled);
    // Membership homing matches the persisted restore, with live flags from
    // the facts (floating, sticky, minimized, fullscreen truth rides facts).
    assert_eq!(row_of(&reply, "win-tiled").workspace, "ws-1");
    assert_eq!(row_of(&reply, "win-tiled").output.0, "out-a");
    assert_eq!(
        row_of(&reply, "win-tiled").frame_authority,
        ScopeAuthority::Controlled
    );
    assert!(row_of(&reply, "win-tiled").allocation.is_some());
    let float_row = row_of(&reply, "win-float");
    assert_eq!(float_row.workspace, "ws-1");
    assert!(float_row.floating);
    assert_eq!(float_row.membership_authority, ScopeAuthority::Untouched);
    assert_eq!(float_row.frame_authority, ScopeAuthority::Untouched);
    assert!(float_row.allocation.is_none());
    let sticky_row = row_of(&reply, "win-sticky");
    assert_eq!(sticky_row.workspace, "ws-1");
    assert!(sticky_row.floating);
    let min_row = row_of(&reply, "win-min");
    assert_eq!(min_row.workspace, "ws-1");
    assert_eq!(
        min_row.frame_authority,
        ScopeAuthority::Hold(tiler_core::world_report::HoldReason::Minimized)
    );
    let full_row = row_of(&reply, "win-full");
    assert_eq!(full_row.workspace, "ws-1");
    assert!(full_row.effective_frame.is_none());
    assert_eq!(full_row.frame_authority, ScopeAuthority::Untouched);
    assert_eq!(row_of(&reply, "win-b").workspace, "ws-3");
    assert_eq!(row_of(&reply, "win-b").output.0, "out-b");
    // Restored members are retained, never fresh admission: no focus effects.
    assert!(reply.effects.is_empty(), "effects: {:?}", reply.effects);
    // An identical resend is a duplicate with no new revision, never a
    // second restoration.
    let revision = reply.state_revision;
    match engine.handle_report(&bootstrap()) {
        WorldOutcome::Duplicate(cached) => assert_eq!(cached.state_revision, revision),
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision);
    // The next observation-only report retains everything with no effects.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens(),
            restored_facts(),
            "win-tiled",
            None,
            None,
            None,
        ),
    );
    assert_eq!(ring_of(&second, "out-a"), vec!["ws-1", "ws-2"]);
    assert_eq!(output_view(&second, "out-a").as_deref(), Some("ws-1"));
    assert_eq!(row_of(&second, "win-tiled").workspace, "ws-1");
    assert_eq!(row_of(&second, "win-float").workspace, "ws-1");
    assert!(row_of(&second, "win-float").floating);
    assert_eq!(row_of(&second, "win-b").workspace, "ws-3");
    assert!(second.effects.is_empty());
    // No phantom history: previous-toggle right after a restore is a quiet
    // no-op, never a recreated selection.
    let previous = accepted(
        &mut engine,
        &report(
            3,
            3,
            screens(),
            restored_facts(),
            "win-tiled",
            None,
            Some(7),
            Some(WorldCommand::SelectPrevious {
                output: OutputId("out-a".to_owned()),
            }),
        ),
    );
    assert!(previous.transfers.is_empty());
    assert!(previous.effects.is_empty());
}

#[test]
fn malformed_restoration_refuses_transactionally() {
    // Each case runs on a fresh engine and must leave zero retained state:
    // a follow-up fresh bootstrap still mints the ordinary rings.
    let one_ring = || RestoredRing {
        output: OutputId("out-a".to_owned()),
        workspaces: vec!["ws-1".to_owned(), "ws-2".to_owned()],
    };
    let one_member = || RestoredMember {
        window: WindowId("win-tiled".to_owned()),
        output: OutputId("out-a".to_owned()),
        workspace: "ws-1".to_owned(),
    };
    let empty_world = || RestoredWorld {
        rings: Vec::new(),
        views: Vec::new(),
        tiling: Vec::new(),
        members: Vec::new(),
    };
    let cases: Vec<(&str, RestoredWorld, &str)> = vec![
        ("empty rings", empty_world(), "restored-ring-invalid"),
        (
            "duplicate ring outputs",
            RestoredWorld {
                rings: vec![
                    RestoredRing {
                        output: OutputId("out-a".to_owned()),
                        workspaces: vec!["ws-1".to_owned()],
                    },
                    RestoredRing {
                        output: OutputId("out-a".to_owned()),
                        workspaces: vec!["ws-2".to_owned()],
                    },
                ],
                ..empty_world()
            },
            "restored-output-invalid",
        ),
        (
            "view names unknown workspace",
            RestoredWorld {
                rings: vec![one_ring()],
                views: vec![ScreenView {
                    output: OutputId("out-a".to_owned()),
                    view: "ws-9".to_owned(),
                }],
                ..empty_world()
            },
            "restored-view-invalid",
        ),
        (
            "member homed on unknown workspace",
            RestoredWorld {
                rings: vec![one_ring()],
                members: vec![RestoredMember {
                    window: WindowId("win-tiled".to_owned()),
                    output: OutputId("out-a".to_owned()),
                    workspace: "ws-9".to_owned(),
                }],
                ..empty_world()
            },
            "restored-member-unknown",
        ),
        (
            "member unknown to the facts",
            RestoredWorld {
                rings: vec![one_ring()],
                members: vec![RestoredMember {
                    window: WindowId("win-gone".to_owned()),
                    output: OutputId("out-a".to_owned()),
                    workspace: "ws-1".to_owned(),
                }],
                ..empty_world()
            },
            "restored-member-unknown",
        ),
        (
            "tiling names unknown workspace",
            RestoredWorld {
                rings: vec![one_ring()],
                tiling: vec![RestoredTiling {
                    output: OutputId("out-a".to_owned()),
                    workspace: "ws-9".to_owned(),
                    tiled: true,
                }],
                ..empty_world()
            },
            "restored-tiling-unknown",
        ),
        (
            "duplicate member windows",
            RestoredWorld {
                rings: vec![one_ring()],
                members: vec![
                    one_member(),
                    RestoredMember {
                        workspace: "ws-2".to_owned(),
                        ..one_member()
                    },
                ],
                ..empty_world()
            },
            "restored-member-duplicate",
        ),
    ];
    for (name, restored, message) in cases {
        let mut engine = WorldEngine::new("gen-1").expect("valid generation");
        match engine.handle_report(&report(
            1,
            1,
            screens(),
            restored_facts(),
            "win-tiled",
            Some(restored),
            None,
            None,
        )) {
            WorldOutcome::Rejected { kind, message: got } => {
                assert_eq!(kind, "snapshot-invalid", "{name}");
                assert_eq!(got, message, "{name}");
            }
            other => panic!("{name}: expected rejection, got {other:?}"),
        }
        assert_eq!(engine.state_revision(), 0, "{name}: no mutation");
        // Transactional: a fresh bootstrap right after still works.
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                screens(),
                restored_facts(),
                "win-tiled",
                None,
                None,
                None,
            ),
        );
        assert_eq!(reply.state_revision, 1, "{name}");
        assert!(!reply.workspaces.is_empty(), "{name}");
    }
    // A restored member that is also explicitly closed refuses too: the
    // window races closure while the owner is down, so the restore is
    // stale. (Not in the facts, so the generic closed-overlap gate stays
    // silent and the restoration gate reports it.)
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let mut gone = restored_world();
    gone.members.push(RestoredMember {
        window: WindowId("win-gone".to_owned()),
        output: OutputId("out-a".to_owned()),
        workspace: "ws-1".to_owned(),
    });
    let mut closed_report = report(
        1,
        1,
        screens(),
        restored_facts(),
        "win-tiled",
        Some(gone),
        None,
        None,
    );
    closed_report.closed = vec![WindowId("win-gone".to_owned())];
    match engine.handle_report(&closed_report) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "snapshot-invalid");
            assert_eq!(message, "restored-member-closed");
        }
        other => panic!("expected closed-member rejection, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), 0);
}

#[test]
fn stale_restoration_on_a_later_report_refuses() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens(),
            restored_facts(),
            "win-tiled",
            None,
            None,
            None,
        ),
    );
    assert_eq!(first.state_revision, 1);
    // Restoration past the first report is stale: refused without mutation.
    match engine.handle_report(&report(
        2,
        2,
        screens(),
        restored_facts(),
        "win-tiled",
        Some(restored_world()),
        None,
        None,
    )) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "snapshot-invalid");
            assert_eq!(
                message,
                "restored ingress is only valid on the first report"
            );
        }
        other => panic!("expected stale rejection, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), 1);
    // Ordinary observation-only reports still proceed.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens(),
            restored_facts(),
            "win-tiled",
            None,
            None,
            None,
        ),
    );
    assert_eq!(second.state_revision, 2);
}

#[test]
fn fresh_newcomer_after_restore_focuses_once() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            screens(),
            restored_facts(),
            "win-tiled",
            Some(restored_world()),
            None,
            None,
        ),
    );
    // A genuinely new lifetime after the restoration bootstrap focuses
    // once through the ordinary admission path. out-b carries no
    // fullscreen sibling, so no overlay suppresses the intent.
    let mut arrived = restored_facts();
    arrived.push(fact("win-new", "out-b", 600));
    let reply = accepted(
        &mut engine,
        &report(2, 2, screens(), arrived, "win-new", None, None, None),
    );
    assert_eq!(reply.effects.len(), 1, "effects: {:?}", reply.effects);
    assert_eq!(reply.effects[0].id, "af-win-new");
    assert_eq!(reply.effects[0].target.0, "win-new");
    assert_eq!(row_of(&reply, "win-new").workspace, "ws-3");
    // Re-observation spends nothing more.
    let mut again = restored_facts();
    again.push(fact("win-new", "out-b", 600));
    let third = accepted(
        &mut engine,
        &report(3, 3, screens(), again, "win-new", None, None, None),
    );
    assert!(third.effects.is_empty());
}

#[test]
fn restoration_advances_trailing_creation_past_restored_ids() {
    // A restored ring whose only workspace is occupied grows its trailing
    // spare from the restored counter, never by re-minting `ws-1`.
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let restored = RestoredWorld {
        rings: vec![RestoredRing {
            output: OutputId("out-a".to_owned()),
            workspaces: vec!["ws-5".to_owned()],
        }],
        views: vec![ScreenView {
            output: OutputId("out-a".to_owned()),
            view: "ws-5".to_owned(),
        }],
        tiling: vec![RestoredTiling {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-5".to_owned(),
            tiled: true,
        }],
        members: vec![RestoredMember {
            window: WindowId("win-tiled".to_owned()),
            output: OutputId("out-a".to_owned()),
            workspace: "ws-5".to_owned(),
        }],
    };
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen_full("out-a", rect(0, 0, 800, 600), true)],
            vec![fact("win-tiled", "out-a", 0)],
            "win-tiled",
            Some(restored),
            None,
            None,
        ),
    );
    let ring: Vec<String> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(ring, vec!["ws-5", "ws-6"]);
    assert_eq!(output_view(&reply, "out-a").as_deref(), Some("ws-5"));
    assert_eq!(row_of(&reply, "win-tiled").workspace, "ws-5");
}
