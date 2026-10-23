//! Sessionless migration placement: workspaces toggled to floating
//! release the Engine session, so cross-output migration members carry a
//! core-owned portable placement (FULL-delta translated, constrained to
//! the destination work area) in `effective_frame`. Oversized members pin
//! to the area origin with size preserved; unreadable members, incomplete
//! screens, and protected lanes take no placement; partial arrival holds
//! the migration until every member proves, then completes to the refill.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    ScopeAuthority, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_rect(output: &str, full: Rect, work: Rect, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(work),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary,
    }
}

fn fact_at(window: &str, output: &str, rect: Rect) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(rect),
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

#[allow(clippy::too_many_arguments)]
fn report_screens(
    request_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: Vec::new(),
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        default_tiled: true,
        focused_global: String::new(),
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
        presentation: crate::world_report::PresentationIngress::disabled(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn migrate_floating_workspace(
    screens: Vec<ScreenReport>,
    members: Vec<WindowFact>,
) -> crate::world_report::WorldReply {
    migrated_session(screens, members).1
}

fn migrated_session(
    screens: Vec<ScreenReport>,
    members: Vec<WindowFact>,
) -> (WorldEngine, crate::world_report::WorldReply, String) {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report_screens(1, screens.clone(), members.clone(), None, None),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report_screens(
            2,
            screens.clone(),
            members.clone(),
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
                tiled: false,
            }),
        ),
    );
    let migrated = accepted(
        &mut engine,
        &report_screens(
            3,
            screens.clone(),
            members,
            Some(12),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
        ),
    );
    (engine, migrated, view_a)
}

fn row_of<'a>(
    reply: &'a crate::world_report::WorldReply,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

#[test]
fn sessionless_oversized_member_pins_to_destination_origin() {
    // Oversized on both axes: the FULL-delta translation cannot clamp into
    // the small destination, so the origin pins to the destination work
    // area origin with size preserved (never resized).
    let screens = vec![
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600), true),
        screen_rect(
            "out-b",
            rect(800, 0, 400, 300),
            rect(800, 0, 400, 300),
            false,
        ),
    ];
    let members = vec![fact_at("w0", "out-a", rect(100, 100, 500, 400))];
    let reply = migrate_floating_workspace(screens, members);
    let row = row_of(&reply, "w0");
    assert_eq!(row.output.0.as_str(), "out-b", "{row:?}");
    assert_eq!(
        row.effective_frame,
        Some(rect(800, 0, 500, 400)),
        "oversized pins to the area origin, size preserved: {row:?}"
    );
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched, "{row:?}");
    assert!(row.allocation.is_none(), "{row:?}");
}

#[test]
fn sessionless_unreadable_member_carries_no_placement() {
    // The migration starts readable, then the held member goes unreadable
    // at the source: the core holds the migration with no guessed
    // placement for it instead of inventing one. (An unreadable source
    // refuses the migration command itself, so the guard is reached through
    // a mid-migration readability loss.)
    let screens = vec![
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600), true),
        screen_rect(
            "out-b",
            rect(800, 0, 800, 600),
            rect(800, 0, 800, 600),
            false,
        ),
    ];
    let members = vec![
        fact_at("w0", "out-a", rect(100, 100, 40, 90)),
        fact_at("w1", "out-a", rect(200, 200, 40, 90)),
    ];
    let (mut engine, migrated, _) = migrated_session(screens.clone(), members);
    assert!(
        row_of(&migrated, "w1").effective_frame.is_some(),
        "readable member places: {:?}",
        row_of(&migrated, "w1")
    );
    let held = accepted(
        &mut engine,
        &report_screens(
            4,
            screens.clone(),
            vec![
                fact_at("w0", "out-a", rect(100, 100, 40, 90)),
                WindowFact {
                    rect: None,
                    ..fact_at("w1", "out-a", rect(0, 0, 40, 90))
                },
            ],
            None,
            None,
        ),
    );
    let row = row_of(&held, "w1");
    assert!(row.allocation.is_none(), "{row:?}");
    assert!(
        row.effective_frame.is_none(),
        "unreadable member takes no guessed placement: {row:?}"
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "migration holds for the unreadable member: {:?}",
        held.transfers
    );
}

#[test]
fn sessionless_incomplete_screens_carry_no_placement() {
    // Unreadable or incomplete screen descriptions hold too: a member with
    // a perfectly readable frame still takes no placement when either side
    // cannot ground the FULL-delta translation. (The migration command
    // itself refuses on incomplete screens, so the guard is reached through
    // a mid-migration screen loss.)
    let screens = || {
        vec![
            screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600), true),
            screen_rect(
                "out-b",
                rect(800, 0, 800, 600),
                rect(800, 0, 800, 600),
                false,
            ),
        ]
    };
    let members = || {
        vec![
            fact_at("w0", "out-a", rect(100, 100, 40, 90)),
            fact_at("w1", "out-a", rect(200, 200, 40, 90)),
        ]
    };
    // Sanity: the complete pair places.
    let placed = migrate_floating_workspace(screens(), members());
    assert_eq!(
        row_of(&placed, "w0").effective_frame,
        Some(rect(900, 100, 40, 90)),
        "complete pair places: {:?}",
        row_of(&placed, "w0")
    );
    // Incomplete target screen.
    let (mut engine, migrated, _) = migrated_session(screens(), members());
    assert!(
        row_of(&migrated, "w0").effective_frame.is_some(),
        "readable member places before the loss"
    );
    let mut incomplete_target = screens();
    incomplete_target[1].complete = false;
    let held = accepted(
        &mut engine,
        &report_screens(4, incomplete_target, members(), None, None),
    );
    assert!(
        row_of(&held, "w0").effective_frame.is_none(),
        "incomplete target holds: {:?}",
        row_of(&held, "w0")
    );
    // Target with no work area, and source with no full rect: report
    // validation rejects these snapshots outright (`screen-rect-partial`),
    // so the placement guard's missing-rect branches are unreachable
    // through valid reports and need no invented placement either.
    for (label, lossy) in [
        ("no-work", {
            let mut no_work = screens();
            no_work[1].work_area = None;
            no_work
        }),
        ("no-full", {
            let mut no_full = screens();
            no_full[0].full_rect = None;
            no_full
        }),
    ] {
        let (mut engine, _, _) = migrated_session(screens(), members());
        match engine.handle_report(&report_screens(4, lossy, members(), None, None)) {
            WorldOutcome::Rejected { kind, message } => {
                assert_eq!(kind, "snapshot-invalid", "{label}");
                assert!(
                    message.contains("screen-rect-partial"),
                    "{label}: {message}"
                );
            }
            other => panic!("{label}: partial screen must refuse, got {other:?}"),
        }
    }
}

#[test]
fn sessionless_protected_members_carry_no_placement() {
    // Protected lanes never take the portable placement even with a
    // readable frame on complete screens: a member that becomes maximized,
    // fullscreen, sticky, or fit-excluded mid-migration rides membership
    // only. (Floats and minimized ride the tests above; the migration
    // command refuses protected members outright, so the guard is reached
    // through a mid-migration flag flip.)
    let screens = || {
        vec![
            screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600), true),
            screen_rect(
                "out-b",
                rect(800, 0, 800, 600),
                rect(800, 0, 800, 600),
                false,
            ),
        ]
    };
    let members = || {
        vec![
            fact_at("w0", "out-a", rect(100, 100, 40, 90)),
            fact_at("w1", "out-a", rect(200, 200, 40, 90)),
        ]
    };
    for flag in ["maximized", "fullscreen", "sticky", "excluded"] {
        let (mut engine, migrated, _) = migrated_session(screens(), members());
        assert!(
            row_of(&migrated, "w0").effective_frame.is_some(),
            "{flag}: readable member places before the flip"
        );
        let mut flagged = fact_at("w0", "out-a", rect(100, 100, 40, 90));
        match flag {
            "maximized" => flagged.maximized = true,
            "fullscreen" => flagged.fullscreen = true,
            "sticky" => flagged.sticky = true,
            "excluded" => flagged.fit_excluded = true,
            _ => unreachable!("flag table"),
        }
        let held = accepted(
            &mut engine,
            &report_screens(
                4,
                screens(),
                vec![flagged, fact_at("w1", "out-a", rect(200, 200, 40, 90))],
                None,
                None,
            ),
        );
        let row = row_of(&held, "w0");
        assert!(row.allocation.is_none(), "{flag}: {row:?}");
        assert!(
            row.effective_frame.is_none(),
            "{flag}: protected lane takes no placement: {row:?}"
        );
    }
}

#[test]
fn sessionless_partial_arrival_holds_then_recovers() {
    use crate::world_report::TransferKind;
    // Two sessionless members migrate; only one arrives. The migration
    // stays retained while the other member is still observed at the
    // source (then unreadable there), and settles once both prove at the
    // target: partial arrival is progress, never settlement.
    let screens = vec![
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600), true),
        screen_rect(
            "out-b",
            rect(800, 0, 800, 600),
            rect(800, 0, 800, 600),
            false,
        ),
    ];
    let members = vec![
        fact_at("w0", "out-a", rect(100, 100, 40, 90)),
        fact_at("w1", "out-a", rect(200, 200, 40, 90)),
    ];
    let (mut engine, migrated, _) = migrated_session(screens.clone(), members);
    let op = migrated
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration retained")
        .operation_id
        .clone();
    let slot_w0 = row_of(&migrated, "w0")
        .effective_frame
        .expect("placed member");
    let slot_w1 = row_of(&migrated, "w1")
        .effective_frame
        .expect("placed member");
    // Partial arrival: w0 proves at its published slot, w1 still reads at
    // the source. The migration holds for the missing member.
    let partial = accepted(
        &mut engine,
        &report_screens(
            4,
            screens.clone(),
            vec![
                fact_at("w0", "out-b", slot_w0),
                fact_at("w1", "out-a", rect(200, 200, 40, 90)),
            ],
            None,
            None,
        ),
    );
    assert!(
        partial.transfers.iter().any(|t| t.operation_id == op),
        "partial arrival holds the migration: {:?}",
        partial.transfers
    );
    assert_eq!(
        row_of(&partial, "w0").output.0.as_str(),
        "out-b",
        "arrived member stays homed at the target: {:?}",
        row_of(&partial, "w0")
    );
    // The held member goes unreadable at the source: still no settlement
    // and still no guessed placement for it.
    let unreadable = accepted(
        &mut engine,
        &report_screens(
            5,
            screens.clone(),
            vec![
                fact_at("w0", "out-b", slot_w0),
                WindowFact {
                    rect: None,
                    ..fact_at("w1", "out-a", rect(0, 0, 40, 90))
                },
            ],
            None,
            None,
        ),
    );
    assert!(
        unreadable.transfers.iter().any(|t| t.operation_id == op),
        "unreadable held member still holds: {:?}",
        unreadable.transfers
    );
    // Full arrival: both members prove at their published slots. The
    // held member recovers to the target homing, and the migration holds
    // only for the deferred source refill (never for arrival anymore).
    let settled = accepted(
        &mut engine,
        &report_screens(
            6,
            screens.clone(),
            vec![
                fact_at("w0", "out-b", slot_w0),
                fact_at("w1", "out-b", slot_w1),
            ],
            None,
            None,
        ),
    );
    assert_eq!(
        row_of(&settled, "w0").output.0.as_str(),
        "out-b",
        "arrived member stays homed at the target: {:?}",
        row_of(&settled, "w0")
    );
    assert_eq!(
        row_of(&settled, "w1").output.0.as_str(),
        "out-b",
        "held member recovers to the target homing: {:?}",
        row_of(&settled, "w1")
    );
    assert!(
        settled.transfers.iter().any(|t| t.operation_id == op
            && matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "full arrival keeps the migration only for the refill: {:?}",
        settled.transfers
    );
    assert!(
        settled
            .transfers
            .iter()
            .any(|t| t.operation_id == format!("{op}-refill")),
        "the remaining work is the source refill: {:?}",
        settled.transfers
    );
}
