//! Pin native-membership unknown/unreadable and minimized lifetime
//! semantics as observed: unknown `None`/unmapped membership holds its
//! home with no geometry, and minimized rows refuse ordinary send/focus
//! commands. Migration and output-loss paths can still relocate a
//! minimized lifetime; nothing here pins those.
//!
//! Vocabulary: unreadable *frame* (`rect=None`) is distinct from
//! unreadable *membership* (`native_workspace=None` with a readable
//! frame). The pins below cover membership-`None` explicitly.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    HoldReason, NativeObservation, NativeOccupancy, ScopeAuthority, WORLD_CONTRACT_VERSION,
    WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32, primary: bool) -> tiler_core::world_report::ScreenReport {
    tiler_core::world_report::ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary,
    }
}

const N1: &str = "3f9a8c1e-1111-4b2a-9c3d-0000000000d1";
const N2: &str = "3f9a8c1e-2222-4b2a-9c3d-0000000000d2";

fn fact_native(window: &str, output: &str, frame: Rect, native: Option<&str>) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(frame),
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
        native_workspace: native.map(str::to_owned),
    }
}

fn sticky_native(window: &str, output: &str, frame: Rect, native: Option<&str>) -> WindowFact {
    WindowFact {
        sticky: true,
        ..fact_native(window, output, frame, native)
    }
}

fn float_sticky_native(
    window: &str,
    output: &str,
    frame: Rect,
    native: Option<&str>,
) -> WindowFact {
    WindowFact {
        sticky: true,
        floating: true,
        ..fact_native(window, output, frame, native)
    }
}

fn fullscreen_native(window: &str, output: &str, frame: Rect, native: Option<&str>) -> WindowFact {
    WindowFact {
        fullscreen: true,
        ..fact_native(window, output, frame, native)
    }
}

fn minimized_native(window: &str, output: &str, native: Option<&str>) -> WindowFact {
    WindowFact {
        rect: None,
        minimized: true,
        ..fact_native(window, output, rect(0, 0, 40, 90), native)
    }
}

fn unreadable_native(window: &str, output: &str, native: Option<&str>) -> WindowFact {
    WindowFact {
        rect: None,
        minimized: false,
        ..fact_native(window, output, rect(0, 0, 40, 90), native)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<tiler_core::world_report::ScreenReport>,
    windows: Vec<WindowFact>,
    native: Vec<NativeObservation>,
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
        workspace_mode: WorkspaceMode::PerOutputLocal,
        screens,
        windows: windows.clone(),
        views: vec![],
        closed: vec![],
        host_owned_workspace_domains: true,
        equal_workspace_count: false,
        birth_clear: false,
        native,
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        born_maximized: tiler_core::born_maximized::BornMaximized::default(),
        default_tiled: true,
        focused_global: windows
            .first()
            .map(|f| f.window.0.clone())
            .unwrap_or_default(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: tiler_core::world_report::PresentationIngress::disabled(),
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

fn native_obs(output: &str, order: &[&str], current: Option<&str>) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|s| (*s).to_owned()).collect(),
        current_native: current.map(str::to_owned),
        occupancy: order
            .iter()
            .map(|s| NativeOccupancy {
                native: (*s).to_owned(),
                count: None,
            })
            .collect(),
    }
}

fn bound_ids(reply: &tiler_core::world_report::WorldReply, output: &str) -> Vec<(String, String)> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output && row.native_binding.is_some())
        .map(|row| (row.id.clone(), row.native_binding.clone().expect("bound")))
        .collect()
}

fn one_screen() -> Vec<tiler_core::world_report::ScreenReport> {
    vec![screen("out-a", 0, true)]
}

#[test]
fn sticky_none_unknown_retained_as_minimized_hold_without_collapse() {
    // Sticky/all-desktops has no single native membership: `None` is
    // unknown, never a collapse into the current view. The observed lane
    // retains a minimized-hold slot at home (no rehoming, no geometry,
    // no flight).
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                sticky_native("win-s", "out-a", rect(0, 0, 200, 200), Some(N1)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                sticky_native("win-s", "out-a", rect(0, 0, 200, 200), None),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let row = held
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-s")
        .expect("sticky retained");
    assert_eq!(row.workspace, bound[N1], "unknown sticky never collapses");
    assert!(held.transfers.is_empty(), "no rehoming move");
    // Actual retained lane: minimized-hold slot at home (no geometry,
    // no op tokens, membership still controlled at the retained home).
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized),
        "unknown sticky retained without geometry"
    );
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(row.allocation, None);
    assert_eq!(row.effective_frame, None);
    assert_eq!(row.frame_operation, None);
    assert_eq!(row.clear_operation, None);
}

#[test]
fn unreadable_known_receipt_holds_scope_without_geometry() {
    // Unreadable membership (rect None, non-minimized) with known native
    // receipt still homes but authorizes no geometry.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![fact_native(
                "win-a",
                "out-a",
                rect(0, 0, 400, 560),
                Some(N1),
            )],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![unreadable_native("win-a", "out-a", Some(N1))],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    assert_eq!(
        engine
            .adapter_homing(&WindowId("win-a".to_owned()))
            .expect("homed")
            .1,
        bound[N1]
    );
    let scope = held
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{}", bound[N1]))
        .expect("scope");
    assert_eq!(
        scope.authority,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        "unreadable holds instead of authorizing geometry"
    );
    assert!(held.transfers.is_empty());
}

#[test]
fn out_of_order_unmapped_native_holds_newcomer_unhomed() {
    // A native id absent from `native_order` and unbound never adopts via
    // receipt: the newcomer publishes no row and mints no workspace.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![fact_native(
                "win-a",
                "out-a",
                rect(0, 0, 400, 560),
                Some(N1),
            )],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native(
                    "win-z",
                    "out-a",
                    rect(0, 0, 200, 200),
                    Some("ghost-outside-order"),
                ),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    assert!(
        reply.windows.iter().all(|row| row.lifetime.0 != "win-z"),
        "unlisted unmapped native never adopts"
    );
    assert_eq!(
        bound_ids(&reply, "out-a").len(),
        2,
        "no workspace minted for unlisted evidence"
    );
    assert!(reply.transfers.is_empty());
}

#[test]
fn out_of_order_retained_member_holds_fenced_home() {
    // A retained member whose evidence switches to an unlisted id keeps
    // its home and fences the scope: unknown never proves arrival.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native(
                    "win-a",
                    "out-a",
                    rect(0, 0, 400, 560),
                    Some("ghost-outside-order"),
                ),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let row = held
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("retained row");
    assert_eq!(row.workspace, bound[N1], "unlisted holds home");
    assert_eq!(
        held.scopes
            .iter()
            .find(|status| status.scope == format!("out-a/{}", bound[N1]))
            .map(|status| status.authority),
        Some(ScopeAuthority::Hold(HoldReason::Unreadable))
    );
    assert!(held.transfers.is_empty());
}

#[test]
fn minimized_send_focus_and_directional_refuse_without_move() {
    // Minimized lifetimes never move via commands and never take focus:
    // Send/Focus reject without mutation, directional reports stale, and
    // the row keeps minimized authority with no geometry or op tokens.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let minimized = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                minimized_native("win-b", "out-a", Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&minimized, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let row = minimized
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("minimized row");
    assert_eq!(row.workspace, bound[N2]);
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(row.allocation, None, "no managed geometry");
    assert_eq!(row.effective_frame, None, "no placement");
    assert_eq!(row.frame_operation, None, "no placement token");
    assert_eq!(row.clear_operation, None, "no clear token");
    // Explicit send of the minimized lifetime refuses without a flight.
    let mut send_report = report(
        3,
        3,
        one_screen(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
            minimized_native("win-b", "out-a", Some(N2)),
        ],
        vec![native_obs("out-a", &[N1, N2], Some(N1))],
        Some(11),
        Some(WorldCommand::Send {
            window: WindowId("win-b".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: bound[N1].clone(),
            follow: false,
        }),
    );
    send_report.focused_global = "win-a".to_owned();
    match engine.handle_report(&send_report) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-window-unknown"),
        other => panic!("minimized send must refuse, got {other:?}"),
    }
    // Explicit focus-by-id on the minimized lifetime refuses without an effect.
    let mut focus_report = report(
        3,
        3,
        one_screen(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
            minimized_native("win-b", "out-a", Some(N2)),
        ],
        vec![native_obs("out-a", &[N1, N2], Some(N1))],
        Some(12),
        Some(WorldCommand::Focus {
            window: WindowId("win-b".to_owned()),
            direction: "left".to_owned(),
        }),
    );
    focus_report.focused_global = "win-a".to_owned();
    match engine.handle_report(&focus_report) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "focus-unknown"),
        other => panic!("minimized focus must refuse, got {other:?}"),
    }
    // Directional focus naming the minimized lifetime reports stale.
    let reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                minimized_native("win-b", "out-a", Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            Some(13),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-b".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
    );
    let outcome = match &reply.command_result {
        Some(tiler_core::world_report::CommandResult::Directional { outcome, .. }) => {
            outcome.as_str()
        }
        other => panic!("directional result, got {other:?}"),
    };
    assert_eq!(outcome, "focus-float-refused-stale");
    assert!(reply.effects.is_empty(), "never focused");
    assert!(reply.transfers.is_empty(), "never moved");
}

#[test]
fn minimized_trailing_retains_workspace_and_focus_global_ignored() {
    // A sole-minimized workspace survives trailing lifecycle with no
    // geometry, and naming it focused never stages focus or mints
    // admission focus.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let mut second_input = report(
        2,
        2,
        one_screen(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
            minimized_native("win-b", "out-a", Some(N2)),
        ],
        vec![native_obs("out-a", &[N1, N2], Some(N1))],
        None,
        None,
    );
    second_input.focused_global = "win-b".to_owned();
    let second = accepted(&mut engine, &second_input);
    assert!(
        second.workspaces.iter().any(|row| row.id == bound[N2]),
        "minimized occupancy retains its workspace"
    );
    let row = second
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("row");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(row.allocation, None);
    assert_eq!(row.effective_frame, None);
    assert!(
        second.effects.is_empty(),
        "minimized focus never actuates: {:?}",
        second.effects
    );
    assert!(second.transfers.is_empty());
}

#[test]
fn readable_none_membership_retained_home_fences_without_move() {
    // Unreadable *membership*: readable frame, non-minimized, but
    // `native_workspace=None`. Retained home is kept, the scope fences
    // Unreadable, no rehoming and no flight.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), None),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let row = held
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("retained row");
    assert_eq!(row.workspace, bound[N1], "None membership never collapses");
    assert_eq!(
        row.allocation, None,
        "fenced scope authorizes no geometry while home is kept"
    );
    assert_eq!(
        held.scopes
            .iter()
            .find(|status| status.scope == format!("out-a/{}", bound[N1]))
            .map(|status| status.authority),
        Some(ScopeAuthority::Hold(HoldReason::Unreadable)),
        "unknown membership fences write authority"
    );
    assert!(held.transfers.is_empty(), "no rehoming move");
    assert!(held.effects.is_empty());
}

#[test]
fn readable_none_membership_newcomer_launch_publishes_no_row() {
    // Launch with unknown membership: the newcomer publishes no row and
    // mints no workspace, while the known member homes normally.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-x", "out-a", rect(100, 100, 200, 150), None),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    assert!(
        reply.windows.iter().all(|row| row.lifetime.0 != "win-x"),
        "unknown membership waits, never defaults"
    );
    assert_eq!(bound_ids(&reply, "out-a").len(), 2);
    assert!(reply.transfers.is_empty());
    assert!(reply.effects.is_empty());
}

#[test]
fn fullscreen_none_absent_order_current_none_launch_adopts_nothing() {
    // Recommended encoding for a fullscreen Space: the window carries
    // `native_workspace=None`, its id is absent from `native_order`, and
    // the current surface is not an ordered desktop (`current=None`).
    // Launch adopts nothing for it: no row, no workspace, no move, while
    // the order still binds and the retained view stays.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fullscreen_native("win-g", "out-a", rect(0, 0, 800, 560), None),
            ],
            vec![native_obs("out-a", &[N1, N2], None)],
            None,
            None,
        ),
    );
    assert_eq!(bound_ids(&reply, "out-a").len(), 2, "order still binds");
    assert!(
        reply.windows.iter().all(|row| row.lifetime.0 != "win-g"),
        "fullscreen Space adopts nothing, never a guess"
    );
    assert!(reply.transfers.is_empty());
    assert!(reply.effects.is_empty());
}

#[test]
fn fullscreen_none_retained_holds_fenced_home() {
    // A retained fullscreen member whose evidence becomes `None` keeps
    // its home and fences the scope instead of collapsing.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fullscreen_native("win-g", "out-a", rect(0, 0, 800, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fullscreen_native("win-g", "out-a", rect(0, 0, 800, 560), None),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let row = held
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-g")
        .expect("retained fullscreen row");
    assert_eq!(row.workspace, bound[N2], "unknown holds home");
    assert!(held.transfers.is_empty());
}

#[test]
fn sticky_tiled_launch_none_publishes_no_row() {
    // Positive all-desktop case at launch: tiled sticky with `None` has
    // no single native membership, so the newcomer publishes no row and
    // mints nothing (there is no initial home to retain).
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                sticky_native("win-s", "out-a", rect(0, 0, 200, 200), None),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    assert!(
        reply.windows.iter().all(|row| row.lifetime.0 != "win-s"),
        "launch sticky-None publishes no row: no home to retain"
    );
    assert_eq!(bound_ids(&reply, "out-a").len(), 2);
    assert!(reply.transfers.is_empty());
    assert!(reply.effects.is_empty());
}

#[test]
fn sticky_float_newcomer_none_publishes_no_row() {
    // Float-only sticky (`floating=true`, `sticky=true`) with `None` at
    // launch likewise publishes no row: a floating unknown newcomer
    // cannot retain an initial home it never had.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                float_sticky_native("win-s", "out-a", rect(0, 0, 200, 200), None),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    assert!(
        reply.windows.iter().all(|row| row.lifetime.0 != "win-s"),
        "float-sticky-None newcomer publishes no row"
    );
    assert_eq!(bound_ids(&reply, "out-a").len(), 2);
    assert!(reply.transfers.is_empty());
    assert!(reply.effects.is_empty());
}

#[test]
fn sticky_float_retained_none_loses_row_without_move() {
    // Retained float-sticky transitioning known->`None`: the exception
    // row is dropped (exceptions are removed when unobserved, never
    // minimized), with no collapse into the current view and no flight.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                float_sticky_native("win-s", "out-a", rect(0, 0, 200, 200), Some(N1)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    assert!(
        first.windows.iter().any(|row| row.lifetime.0 == "win-s"),
        "float-sticky homes while evidence is known"
    );
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                float_sticky_native("win-s", "out-a", rect(0, 0, 200, 200), None),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    assert!(
        held.windows.iter().all(|row| row.lifetime.0 != "win-s"),
        "float-sticky-None drops its row instead of collapsing"
    );
    assert!(held.transfers.is_empty(), "no rehoming move");
    assert!(held.effects.is_empty());
}

#[test]
fn minimized_occupant_native_desktop_removal_retains_workspace_unbound() {
    // Native desktop removal with a minimized occupant: the vanished
    // binding prunes (removal, never rename) while the minimized
    // occupancy retains the logical workspace unbound with minimized
    // authority and no move.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                minimized_native("win-b", "out-a", Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let home = bound[N2].clone();
    // Complete snapshot: N2 vanished from the order. The stale `Some(N2)`
    // receipt no longer maps, so homing falls back to the retained home.
    let after = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                minimized_native("win-b", "out-a", Some(N2)),
            ],
            vec![native_obs("out-a", &[N1], Some(N1))],
            None,
            None,
        ),
    );
    assert!(
        after.workspaces.iter().any(|row| row.id == home),
        "minimized occupancy retains its workspace"
    );
    assert!(
        after
            .workspaces
            .iter()
            .filter(|row| row.output.0 == "out-a" && row.native_binding.is_some())
            .all(|row| row.native_binding.as_deref() != Some(N2)),
        "vanished binding prunes, never renames"
    );
    let row = after
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("minimized row retained");
    assert_eq!(row.workspace, home, "minimized keeps its home");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(row.allocation, None);
    assert_eq!(row.effective_frame, None);
    assert!(after.transfers.is_empty(), "removal moves nothing");
}

#[test]
fn close_focused_tiled_reflows_without_minimized_focus() {
    // Remove-one-foreground: closing the focused tiled window reflows
    // the survivor with no focus effect, and the minimized lifetime
    // keeps minimized authority with no geometry.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N1)),
                minimized_native("win-c", "out-a", Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let mut closing = report(
        2,
        2,
        one_screen(),
        vec![
            fact_native("win-b", "out-a", rect(0, 0, 400, 560), Some(N1)),
            minimized_native("win-c", "out-a", Some(N2)),
        ],
        vec![native_obs("out-a", &[N1, N2], Some(N1))],
        None,
        None,
    );
    closing.closed = vec![WindowId("win-a".to_owned())];
    closing.focused_global = "win-a".to_owned();
    let reply = accepted(&mut engine, &closing);
    let minimized = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-c")
        .expect("minimized retained");
    assert_eq!(
        minimized.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(minimized.allocation, None);
    assert_eq!(minimized.effective_frame, None);
    assert!(
        reply
            .effects
            .iter()
            .all(|effect| effect.target.0 != "win-c"),
        "close-reflow never focuses the minimized lifetime: {:?}",
        reply.effects
    );
    assert!(reply.transfers.is_empty());
}

#[test]
fn pager_click_minimized_refuses_while_tiled_focus_succeeds() {
    // Explicit pager-click path is `WorldCommand::Focus`: a click on a
    // minimized lifetime refuses `focus-unknown` with no effect, while a
    // click on a tiled lifetime mints the explicit `fx-<seq>` effect.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                minimized_native("win-b", "out-a", Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], Some(N1))],
            None,
            None,
        ),
    );
    let mut click_minimized = report(
        2,
        2,
        one_screen(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
            minimized_native("win-b", "out-a", Some(N2)),
        ],
        vec![native_obs("out-a", &[N1, N2], Some(N1))],
        Some(11),
        Some(WorldCommand::Focus {
            window: WindowId("win-b".to_owned()),
            direction: "left".to_owned(),
        }),
    );
    click_minimized.focused_global = "win-a".to_owned();
    match engine.handle_report(&click_minimized) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "focus-unknown"),
        other => panic!("pager click on minimized must refuse, got {other:?}"),
    }
    let mut click_tiled = report(
        2,
        2,
        one_screen(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
            minimized_native("win-b", "out-a", Some(N2)),
        ],
        vec![native_obs("out-a", &[N1, N2], Some(N1))],
        Some(11),
        Some(WorldCommand::Focus {
            window: WindowId("win-a".to_owned()),
            direction: "left".to_owned(),
        }),
    );
    click_tiled.focused_global = "win-a".to_owned();
    let reply = accepted(&mut engine, &click_tiled);
    assert!(
        reply
            .effects
            .iter()
            .any(|effect| effect.target.0 == "win-a" && effect.is_explicit_focus_for(11)),
        "pager click on tiled mints fx-11: {:?}",
        reply.effects
    );
}
