//! Native float rehome across domain sweeps. When a float is positively
//! observed alone in a fresh destination, it takes its observed placement
//! and the prior home retires both records for that window, even when its
//! remaining layout is held. A same-output workspace change is a crossing too.

use crate::boundary::CoreCommand;
use crate::connection::HostCapabilities;
use crate::directional::{OutputId, WindowId, WindowSubject, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_report::{
    AuthorityScope, BatchSettlement, CommandResult, HeldScreenRow, NativeObservation,
    NativeOccupancy, NativeWorkspaceFact, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane,
    ScreenReport, WindowFact, WorldCommand, WorldReportV4,
};
use crate::world_v4_support_tests::{
    accepted, accepted_with_settlements, fact, report_full, screen,
};

/// Native drop frame used by every crossing below.
const RELEASE: Rect = Rect {
    x: 900,
    y: 50,
    w: 400,
    h: 300,
};

fn window_id(window: &str) -> WindowId {
    WindowId(window.to_owned())
}

/// Synthetic window row for the non-host-owned engine: raw `maximized` and
/// `sticky` stay null because `caps.maximize == false` refuses host facts.
fn plain(window: &str, output: &str, x: i32) -> WindowFact {
    let mut row = fact(window, output, x);
    row.maximized = None;
    row.sticky = None;
    row
}

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            maximize: false,
            fullscreen: true,
            ..HostCapabilities::default()
        },
    )
    .expect("generation")
}

fn screens_two() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn report_two(seq: u64, windows: Vec<WindowFact>, focused: &str) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        screens_two(),
        windows,
        vec![],
        focused,
        None,
        None,
    )
}

fn command_two(
    seq: u64,
    command_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    command: WorldCommand,
) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        screens_two(),
        windows,
        vec![],
        focused,
        Some(command_seq),
        Some(command),
    )
}

fn toggle_float(window: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: WindowSubject::Window(window_id(window)),
        float_rect: None,
    })
}

fn float_placement(settlements: &[BatchSettlement]) -> Rect {
    match settlements
        .first()
        .and_then(|settlement| settlement.result.as_ref())
    {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    }
}

fn with_rect(mut row: WindowFact, rect: Rect) -> WindowFact {
    row.rect = Some(rect);
    row
}

/// First logical workspace id mapped to `output`.
fn ring_id(engine: &WorldEngine, output: &str) -> String {
    engine
        .world
        .workspace
        .mapping
        .get(output)
        .and_then(|ring| ring.first())
        .cloned()
        .unwrap_or_else(|| panic!("logical ring for {output}"))
}

#[test]
fn float_rehome_held_source_crossing_retires_only_mover() {
    let mut engine = engine();
    let roster = || vec![plain("win-a", "out-a", 0), plain("win-b", "out-a", 100)];
    accepted(&mut engine, &report_two(1, roster(), "win-a"));
    let source = key("out-a", &ring_id(&engine, "out-a"));
    let destination = key("out-b", &ring_id(&engine, "out-b"));
    let window = window_id("win-a");
    let (_floated, settlements) = accepted_with_settlements(
        &mut engine,
        &command_two(2, 1, roster(), "win-a", toggle_float("win-a")),
    );
    let base = float_placement(&settlements);
    assert_eq!(
        engine
            .world
            .layout
            .retained_float_geometry(&source, &window),
        Some(base),
        "the float command records the durable placement at the source"
    );

    // The native drag lands alone on empty out-b while out-a reads held.
    // The fresh destination fact proves the mover left; out-a's other
    // windows receive no inferred departures.
    let mut crossing = report_full(
        4,
        4,
        vec![screen("out-b", 800)],
        vec![with_rect(plain("win-a", "out-b", 0), RELEASE)],
        vec![],
        "win-a",
        None,
        None,
    );
    crossing.held_screens.push(HeldScreenRow {
        output: OutputId("out-a".to_owned()),
        ordinal: 0,
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-a".to_owned())),
            row: RowDiagnosticLane::Screen,
            id: Some("out-a".to_owned()),
            kind: RowDiagnosticKind::Unreadable,
        },
    });
    accepted(&mut engine, &crossing);
    assert!(
        engine
            .world
            .layout
            .is_tiled_window(&source, &window_id("win-b")),
        "the held source still owns its unobserved sibling"
    );
    assert_eq!(
        engine
            .world
            .layout
            .exception_float_geometry(&source, &window),
        None,
        "the source retires the positively rehomed exception"
    );
    assert_eq!(
        engine
            .world
            .layout
            .retained_float_geometry(&source, &window),
        None,
        "the source retires its durable placement with the exception"
    );
    assert_eq!(
        engine
            .world
            .layout
            .retained_float_geometry(&destination, &window),
        Some(RELEASE),
        "the destination adopts the dropped frame"
    );
    assert_eq!(
        engine
            .world
            .layout
            .exception_float_geometry(&destination, &window),
        Some(RELEASE)
    );
}

#[test]
fn float_rehome_same_output_workspace_crossing_adopts_placement() {
    fn host_fact(window: &str, native: &str, x: i32) -> WindowFact {
        let mut row = fact(window, "out-a", x);
        row.native_workspace = NativeWorkspaceFact::Value(native.to_owned());
        row
    }

    fn native_obs(current: &str) -> NativeObservation {
        NativeObservation {
            output: OutputId("out-a".to_owned()),
            native_order: vec!["n1".to_owned(), "n2".to_owned()],
            current_native: Some(current.to_owned()),
            occupancy: vec![
                NativeOccupancy {
                    native: "n1".to_owned(),
                    count: None,
                },
                NativeOccupancy {
                    native: "n2".to_owned(),
                    count: None,
                },
            ],
        }
    }

    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let roster = |native: &str| vec![host_fact("win-a", native, 0), host_fact("win-b", "n1", 100)];
    let mut first = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        roster("n1"),
        vec![],
        "win-a",
        None,
        None,
    );
    first.native = vec![native_obs("n1")];
    accepted(&mut engine, &first);
    let ring = engine
        .world
        .workspace
        .mapping
        .get("out-a")
        .cloned()
        .expect("out-a ring");
    assert_eq!(
        ring.len(),
        2,
        "two native desktops bind two logical workspaces, got {ring:?}"
    );
    let source = key("out-a", &ring[0]);
    let destination = key("out-a", &ring[1]);
    let window = window_id("win-a");

    let mut float_command = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        roster("n1"),
        vec![],
        "win-a",
        Some(1),
        Some(toggle_float("win-a")),
    );
    float_command.native = vec![native_obs("n1")];
    let (_floated, settlements) = accepted_with_settlements(&mut engine, &float_command);
    let base = float_placement(&settlements);
    let mut settled = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![
            with_rect(host_fact("win-a", "n1", 0), base),
            host_fact("win-b", "n1", 100),
        ],
        vec![],
        "win-a",
        None,
        None,
    );
    settled.native = vec![native_obs("n1")];
    accepted(&mut engine, &settled);
    assert_eq!(
        engine
            .world
            .layout
            .retained_float_geometry(&source, &window),
        Some(base)
    );

    // Natively dragged to the otherwise-empty second desktop of the same
    // output: a same-output workspace crossing, not an output crossing.
    let mut crossing = report_full(
        4,
        4,
        vec![screen("out-a", 0)],
        vec![
            with_rect(host_fact("win-a", "n2", 0), RELEASE),
            host_fact("win-b", "n1", 100),
        ],
        vec![],
        "win-a",
        None,
        None,
    );
    crossing.native = vec![native_obs("n2")];
    accepted(&mut engine, &crossing);
    assert_eq!(
        engine
            .world
            .layout
            .retained_float_geometry(&destination, &window),
        Some(RELEASE),
        "a same-output workspace crossing adopts the dropped frame"
    );
    assert_eq!(
        engine
            .world
            .layout
            .retained_float_geometry(&source, &window),
        None,
        "the source placement retires with the adoption"
    );
    assert_eq!(
        engine
            .world
            .layout
            .exception_float_geometry(&source, &window),
        None,
        "the source exception retires with the placement"
    );
}
