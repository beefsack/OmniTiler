//! Stage-4a gesture lifecycle coverage: normalized ingress, retained core
//! gesture/target/overlay/release, and edge-share policy.
//!
//! Pure decision tests pin the moved Windows derivations
//! (`classify_gesture`, `prefer_modal_move`, `drop_point_in_domain`,
//! pointer containment, drop-destination resolution).
//! Engine tests drive the retained lifecycle through the world contract:
//! start/update/release/cancel, Esc, stale/replacement, gaps, game pause
//! (cursor-less updates), cross-output send semantics, resize share
//! policy, close clearing, and incarnation reset.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::gesture::{
    DropDestination, GestureIngress, GestureIntent, GestureKind, GesturePhase, classify_gesture,
    drop_point_in_domain, pointer_output, prefer_modal_move, resolve_drop_destination,
};
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    DragPreviewV4, PresentationIngressV4, PresentationRowV4, ScreenReport, TransferKind,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorldReportV4,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_full(output: &str, full: Rect) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(full),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

fn screens() -> Vec<ScreenReport> {
    vec![
        screen_full("out-a", rect(0, 0, 800, 600)),
        screen_full("out-b", rect(800, 0, 800, 600)),
    ]
}

fn fact(window: &str, output: &str, r: Rect) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(r),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(crate::size_hints::WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: crate::world_report::NativeWorkspaceFact::Omitted,
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
        window_type: Some(crate::world_report::WindowType::Normal),
        activation_policy: Some(crate::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: crate::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

fn ingress(
    window: &str,
    kind: GestureKind,
    cursor: Option<(i32, i32)>,
    phase: GesturePhase,
    start_frame: Rect,
) -> GestureIngress {
    ingress_id(window, kind, cursor, phase, start_frame, 1)
}

/// Same gesture with an explicit stable id. Fresh ids order above every
/// consumed terminal; reused ids replay.
fn ingress_id(
    window: &str,
    kind: GestureKind,
    cursor: Option<(i32, i32)>,
    phase: GesturePhase,
    start_frame: Rect,
    id: u64,
) -> GestureIngress {
    GestureIngress {
        id,
        window: WindowId(window.to_owned()),
        kind,
        cursor,
        start_cursor: None,
        phase,
        esc: false,
        modal_move: kind == GestureKind::Move,
        start_frame,
    }
}

#[allow(clippy::too_many_arguments)]
fn report_on(
    screens: Vec<ScreenReport>,
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    gesture: Option<GestureIngress>,
    closed: Vec<WindowId>,
    inventory_complete: bool,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        screens,
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed,
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
        inventory_complete,
        gesture_barrier: gesture.is_some(),
        gesture,
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
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> crate::world_report::WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn rebind(engine: &mut WorldEngine, adapter: &str) {
    let handshake = engine.handshake(&crate::connection::HandshakeRequest {
        adapter: adapter.to_owned(),
        request: "reconnect-1".to_owned(),
        application_identity: crate::connection::ApplicationIdentity {
            app_id: None,
            executable: None,
            process_id: None,
        },
        capabilities: crate::connection::HostCapabilities {
            // Fixtures carry native sticky and maximize facts; only native
            // identity is unavailable on this rebind lane.
            native_sticky: true,
            maximize: true,
            ..crate::connection::HostCapabilities::without_native_identity()
        },
    });
    let generation = handshake.generation.expect("rebound generation");
    let settings = engine
        .handle_settings(&crate::connection::SettingsRequest {
            adapter: adapter.to_owned(),
            generation,
            request: "settings-1".to_owned(),
            revision: 0,
            values: Vec::new(),
        })
        .expect("initial settings");
    assert_eq!(settings.status, crate::connection::SettingsStatus::Accepted);
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    gesture: Option<GestureIngress>,
    closed: Vec<WindowId>,
    inventory_complete: bool,
) -> WorldReportV4 {
    report_on(
        screens(),
        request_seq,
        observation_seq,
        windows,
        focused,
        gesture,
        closed,
        inventory_complete,
    )
}

/// Two tiled windows on out-a, focused on win-a: the stable gesture rig.
/// win-a starts at (0,0,40,90) like the fact helper; the Engine tiles the
/// pair across the 800px work area.
fn rig() -> (WorldEngine, Rect, Rect) {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let ra = rect(0, 0, 40, 90);
    let rb = rect(100, 0, 40, 90);
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "",
            None,
            vec![],
            true,
        ),
    );
    (engine, ra, rb)
}

fn row_of<'a>(
    reply: &'a crate::world_report::WorldReplyV4,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

fn preview_of(reply: &crate::world_report::WorldReplyV4) -> Rect {
    let Some(DragPreviewV4 { rect, .. }) = reply.drag_preview.clone() else {
        panic!("expected drag-preview, got {:?}", reply.drag_preview);
    };
    rect
}

// --- Pure release-intent derivation (moved Windows policy) ---

#[test]
fn classify_move_needs_cursor_and_reports_drop_point() {
    let before = rect(10, 20, 100, 100);
    let after = rect(30, 20, 100, 100);
    assert_eq!(
        classify_gesture(&before, &after, Some((40, 60))),
        Some(GestureIntent::MoveDrop { x: 40, y: 60 })
    );
    assert_eq!(classify_gesture(&before, &after, None), None);
    assert_eq!(classify_gesture(&before, &before, Some((10, 20))), None);
}

#[test]
fn classify_resize_picks_edges_per_axis_with_origin_tiebreak() {
    // Left edge moved right: origin edge wins the axis.
    assert_eq!(
        classify_gesture(
            &rect(10, 20, 100, 100),
            &rect(30, 20, 80, 100),
            Some((0, 0))
        ),
        Some(GestureIntent::PointerResize {
            direction: "left",
            boundary: 30,
            direction2: None,
            boundary2: None,
        })
    );
    // Right edge moved only: far edge wins.
    assert_eq!(
        classify_gesture(
            &rect(10, 20, 100, 100),
            &rect(10, 20, 140, 100),
            Some((0, 0))
        ),
        Some(GestureIntent::PointerResize {
            direction: "right",
            boundary: 150,
            direction2: None,
            boundary2: None,
        })
    );
    // Corner: both axes derive, horizontal first. The bottom edge moved
    // up (120 -> 80), so the vertical far edge ("down") wins at 80.
    assert_eq!(
        classify_gesture(
            &rect(10, 20, 100, 100),
            &rect(10, 20, 140, 60),
            Some((0, 0))
        ),
        Some(GestureIntent::PointerResize {
            direction: "right",
            boundary: 150,
            direction2: Some("down"),
            boundary2: Some(80),
        })
    );
}

#[test]
fn prefer_modal_move_overrides_rescale_misread_resize_only() {
    let resize = GestureIntent::PointerResize {
        direction: "right",
        boundary: 150,
        direction2: None,
        boundary2: None,
    };
    assert_eq!(
        prefer_modal_move(resize, Some((9, 9)), true),
        GestureIntent::MoveDrop { x: 9, y: 9 }
    );
    // Cursor-less resizes keep the geometry route: no point to place.
    assert_eq!(prefer_modal_move(resize, None, true), resize);
    // Move verdicts pass through even with modal truth.
    let mov = GestureIntent::MoveDrop { x: 1, y: 2 };
    assert_eq!(prefer_modal_move(mov, Some((9, 9)), true), mov);
    // Without modal truth the classified resize stands.
    assert_eq!(prefer_modal_move(resize, Some((9, 9)), false), resize);
}

#[test]
fn domain_fence_rejects_outside_and_degenerate() {
    assert!(drop_point_in_domain(0, 0, 800, 600, 799, 599));
    assert!(!drop_point_in_domain(0, 0, 800, 600, 800, 600));
    assert!(!drop_point_in_domain(0, 0, 800, 600, -1, 0));
    assert!(!drop_point_in_domain(0, 0, 0, 600, 0, 0));
}

#[test]
fn pointer_output_uses_full_containment_half_open() {
    let fulls = vec![
        (OutputId("a".to_owned()), rect(0, 0, 800, 600)),
        (OutputId("b".to_owned()), rect(800, 0, 800, 600)),
    ];
    assert_eq!(
        pointer_output(&fulls, 799, 10).map(|o| o.0),
        Some("a".to_owned())
    );
    // Half-open: the shared edge belongs to the right screen.
    assert_eq!(
        pointer_output(&fulls, 800, 10).map(|o| o.0),
        Some("b".to_owned())
    );
    assert_eq!(pointer_output(&fulls, 5000, 10), None);
    let flat = vec![(OutputId("z".to_owned()), rect(0, 0, 0, 600))];
    assert_eq!(pointer_output(&flat, 0, 10), None);
}

#[test]
fn drop_destination_names_same_cross_and_refusals() {
    let fulls = vec![
        (OutputId("a".to_owned()), rect(0, 0, 800, 600)),
        (OutputId("b".to_owned()), rect(800, 0, 800, 600)),
    ];
    let src = OutputId("a".to_owned());
    let inset_a = Some(rect(0, 0, 800, 600));
    let tiled = |_: &OutputId| Some(true);
    let inset_b = |o: &OutputId| (o.0 == "b").then_some(rect(800, 0, 800, 600));
    assert_eq!(
        resolve_drop_destination(&fulls, &src, 100, 100, inset_a, tiled, inset_b),
        DropDestination::SameOutput
    );
    // Same output but outside the inset work area refuses.
    assert_eq!(
        resolve_drop_destination(
            &fulls,
            &src,
            100,
            100,
            Some(rect(0, 0, 50, 50)),
            tiled,
            inset_b
        ),
        DropDestination::Refused("outside-work-area")
    );
    assert_eq!(
        resolve_drop_destination(&fulls, &src, 900, 100, inset_a, tiled, inset_b),
        DropDestination::Cross {
            output: OutputId("b".to_owned())
        }
    );
    assert_eq!(
        resolve_drop_destination(&fulls, &src, 5000, 100, inset_a, tiled, inset_b),
        DropDestination::Refused("outside-output")
    );
    assert_eq!(
        resolve_drop_destination(
            &fulls,
            &src,
            900,
            100,
            inset_a,
            |_: &OutputId| Some(false),
            inset_b
        ),
        DropDestination::Refused("floating-dest")
    );
    assert_eq!(
        resolve_drop_destination(
            &fulls,
            &src,
            900,
            100,
            inset_a,
            |_: &OutputId| None,
            inset_b
        ),
        DropDestination::Refused("no-view")
    );
    assert_eq!(
        resolve_drop_destination(&fulls, &src, 900, 100, inset_a, tiled, |_: &OutputId| None),
        DropDestination::Refused("unknown-output")
    );
}

#[test]
fn gesture_wire_tokens_roundtrip_and_reject_unknown() {
    assert_eq!(GestureKind::parse_wire("move"), Some(GestureKind::Move));
    assert_eq!(GestureKind::parse_wire("resize"), Some(GestureKind::Resize));
    assert_eq!(GestureKind::parse_wire("drag"), None);
    assert_eq!(
        GesturePhase::parse_wire("released"),
        Some(GesturePhase::Released)
    );
    assert_eq!(GesturePhase::parse_wire("dropped"), None);
}

// --- Retained lifecycle through the world contract ---

#[test]
fn update_without_bind_is_stale_until_first_update_binds() {
    let (mut engine, ra, rb) = rig();
    // Unknown lifetime never binds: no overlay, no session.
    let unknown = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "ghost",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(unknown.drag_preview.is_none());
    assert!(unknown.transfers.is_empty());
    // A valid update binds implicitly and previews.
    let previewed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let rect = preview_of(&previewed);
    assert!(rect.w > 0 && rect.h > 0, "bound update previews");
}

#[test]
fn move_update_previews_and_clears_on_release_commit() {
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-a").allocation;
    // Bind + preview in one update: insertion neighbor is win-b, never
    // the mover.
    let previewed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let overlay = preview_of(&previewed);
    assert!(overlay.w > 0 && overlay.h > 0);
    assert!(previewed.transfers.is_empty(), "preview never retains");
    // Release with the frame moved into the left half: commits the
    // resolver placement (win-a leaves its right slot).
    let moved_a = rect(100, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(
        released.drag_preview.is_none(),
        "release rides the snapshot"
    );
    assert_ne!(
        row_of(&released, "win-a").allocation,
        before_alloc,
        "release commits the drop"
    );
    assert!(
        released.transfers.is_empty(),
        "same-output drop retains nothing"
    );
    // Terminal: the old session is gone. A repeated release with the
    // same carried frames re-derives the same drop against the already
    // committed layout: self, unchanged, snap-back with no re-commit.
    let rerelease = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(rerelease.drag_preview.is_none());
    assert!(rerelease.transfers.is_empty(), "no double commit");
    assert_eq!(
        row_of(&rerelease, "win-a").allocation,
        row_of(&released, "win-a").allocation,
        "re-release is a no-op"
    );
}

#[test]
fn release_without_movement_snaps_back_without_commit() {
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-a").allocation;
    // Same frame plus a cursor is no change: clear, no commit.
    let released = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(row_of(&released, "win-a").allocation, before_alloc);
    assert!(released.transfers.is_empty());
}

#[test]
fn cursorless_move_release_clears_without_commit() {
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(500, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                None,
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(released.drag_preview.is_none());
    assert!(released.transfers.is_empty());
}

#[test]
fn cursorless_update_holds_with_no_overlay_work() {
    // Game pause falls out naturally: the adapter unsamples the cursor
    // under a focused fullscreen game, so updates hold with no overlay
    // and the session survives for the terminal phase.
    let (mut engine, ra, rb) = rig();
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                None,
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(held.drag_preview.is_none(), "no cursor, no overlay");
    // The session survives: a cursor-bearing update previews.
    let previewed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn cancel_and_esc_clear_without_commit() {
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-a").allocation;
    // Bind, then cancel: cleared, nothing committed.
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let cancelled = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Cancelled,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(cancelled.drag_preview.is_none());
    assert_eq!(row_of(&cancelled, "win-a").allocation, before_alloc);
    // Host Esc edge clears regardless of phase.
    accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let mut esc = ingress(
        "win-a",
        GestureKind::Move,
        Some((150, 50)),
        GesturePhase::Updated,
        ra,
    );
    esc.esc = true;
    let cleared = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(esc),
            vec![],
            true,
        ),
    );
    assert!(cleared.drag_preview.is_none());
}

#[test]
fn replacement_window_is_ignored_while_one_is_retained() {
    let (mut engine, ra, rb) = rig();
    let initial = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(initial.drag_preview.is_some());
    // A different lifetime never replaces the retained session: the old
    // one holds and the newcomer is ignored, never silently adopted.
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            Some(ingress(
                "win-b",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                rb,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(held.drag_preview, initial.drag_preview);
    // The retained session still previews.
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn invalid_starts_refuse_without_session() {
    // Minimized, unreadable, fullscreen, floating, sticky, and incomplete
    // starts all fail closed: no overlay on the next update.
    let (mut engine, ra, rb) = rig();
    let mut seq = 2u64;
    let mut minimized = fact("win-a", "out-a", ra);
    minimized.minimized = Some(true);
    minimized.rect = None;
    for (tag, facts) in [
        (
            "minimized",
            vec![minimized.clone(), fact("win-b", "out-a", rb)],
        ),
        (
            "unreadable",
            vec![
                WindowFact {
                    rect: None,
                    ..fact("win-a", "out-a", ra)
                },
                fact("win-b", "out-a", rb),
            ],
        ),
        (
            "fullscreen",
            vec![
                WindowFact {
                    fullscreen: Some(true),
                    ..fact("win-a", "out-a", ra)
                },
                fact("win-b", "out-a", rb),
            ],
        ),
        (
            "floating",
            vec![
                WindowFact {
                    transient: Some(true),
                    ..fact("win-a", "out-a", ra)
                },
                fact("win-b", "out-a", rb),
            ],
        ),
        (
            "sticky",
            vec![
                WindowFact {
                    sticky: Some(true),
                    ..fact("win-a", "out-a", ra)
                },
                fact("win-b", "out-a", rb),
            ],
        ),
    ] {
        let refused = accepted(
            &mut engine,
            &report(
                seq,
                seq,
                facts,
                "win-a",
                Some(ingress(
                    "win-a",
                    GestureKind::Move,
                    Some((150, 50)),
                    GesturePhase::Updated,
                    ra,
                )),
                vec![],
                true,
            ),
        );
        seq += 1;
        assert!(
            refused.drag_preview.is_none(),
            "{tag} start must not preview"
        );
    }
    // Incomplete inventory never binds.
    let gapped = accepted(
        &mut engine,
        &report(
            seq,
            seq,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            false,
        ),
    );
    assert!(gapped.drag_preview.is_none());
}

#[test]
fn gap_holds_and_recovers_without_replay() {
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    // An incomplete report holds the session with no overlay.
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            false,
        ),
    );
    assert!(held.drag_preview.is_none());
    // Recovery previews again with no replay of the held update.
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn close_clears_the_session() {
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    // Explicit close clears even across the gap-safe retention.
    let closed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![WindowId("win-a".to_owned())],
            true,
        ),
    );
    assert!(closed.drag_preview.is_none());
    // The next update for the closed lifetime is stale.
    let stale = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(stale.drag_preview.is_none());
}

#[test]
fn cross_output_release_retains_follow_and_settles_on_receipt() {
    let (mut engine, ra, rb) = rig();
    // Release the mover onto out-b: membership move plus follow, no
    // pointer placement (existing send semantics).
    let moved_a = rect(900, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-b", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((950, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(
        released.drag_preview.is_none(),
        "release rides snapshot+transfer"
    );
    let transfer = released
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a"))
        .expect("cross release retains a window transfer");
    assert_eq!(transfer.target_output.0, "out-b");
    assert!(transfer.follow, "cross drop follows like the native path");
    assert_eq!(row_of(&released, "win-a").output.0, "out-b");
    // Truthful arrival receipt settles the retained flight. The legacy
    // setup also sent the exact target-view echo, but echoes are inert
    // on native-free engines (and have no v4 slot), so the v4 port
    // observes the same arrival with an empty lane.
    let arrived_a = fact("win-a", "out-b", moved_a);
    let settle = report(
        3,
        3,
        vec![arrived_a, fact("win-b", "out-a", rb)],
        "win-a",
        None,
        vec![],
        true,
    );
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")),
        "arrival receipt retires the gesture transfer"
    );
}

#[test]
fn cross_release_to_unplaceable_points_refuses() {
    let (mut engine, ra, rb) = rig();
    // Outside every output refuses with no transfer and no session.
    let moved_a = rect(5000, 0, 40, 90);
    let refused = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-b", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((5050, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(refused.transfers.is_empty());
    assert!(refused.drag_preview.is_none());
}

#[test]
fn resize_release_commits_share_change() {
    // Shrink win-b (the left leaf) from its right edge: the release
    // derives a single-axis pointer resize, not a move, and commits the
    // share change through the existing resize policy.
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-b").allocation;
    // Right edge moved in (140 -> 200 in observed space): resize right/200.
    let shrunk_b = rect(100, 0, 100, 90);
    let mut start = ingress(
        "win-b",
        GestureKind::Resize,
        Some((200, 50)),
        GesturePhase::Updated,
        rb,
    );
    start.modal_move = false;
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            Some(start),
            vec![],
            true,
        ),
    );
    let mut release = ingress(
        "win-b",
        GestureKind::Resize,
        Some((200, 50)),
        GesturePhase::Released,
        rb,
    );
    release.modal_move = false;
    let released = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", shrunk_b)],
            "win-b",
            Some(release),
            vec![],
            true,
        ),
    );
    assert!(released.drag_preview.is_none(), "resize rides the snapshot");
    assert_ne!(
        row_of(&released, "win-b").allocation,
        before_alloc,
        "resize release commits the share change, got {:?}",
        row_of(&released, "win-b").allocation,
    );
    assert!(released.transfers.is_empty());
}

#[test]
fn modal_move_release_treats_rescale_as_move() {
    // A caption-started loop whose system rescale changed the frame size
    // releases as a move (modal truth), not a resize.
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-a").allocation;
    // Slightly rescaled frame at a new origin with a move cursor.
    let rescaled_a = rect(100, 0, 44, 94);
    let released = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fact("win-a", "out-a", rescaled_a),
                fact("win-b", "out-a", rb),
            ],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert_ne!(
        row_of(&released, "win-a").allocation,
        before_alloc,
        "modal move commits the drop despite the rescale"
    );
    assert!(
        released.transfers.is_empty(),
        "same-output move retains nothing"
    );
}

#[test]
fn consumed_command_reports_still_carry_gesture_preview() {
    // A fresh report carrying an already-consumed command reconciles the
    // observation without re-executing, and still carries the gesture
    // overlay: gesture ingress is observation, never command replay.
    use crate::world_report::WorldCommand;
    let (mut engine, ra, rb) = rig();
    let preview = GestureIngress {
        modal_move: true,
        ..ingress(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Updated,
            ra,
        )
    };
    let mut with_command = report(
        2,
        2,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(preview),
        vec![],
        true,
    );
    with_command.command_seq = Some(7);
    with_command.command = Some(WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: Some(crate::directional::Direction::Right),
    });
    let first = accepted(&mut engine, &with_command);
    assert!(first.drag_preview.is_some() || first.command_consumed);
    // Resend the consumed sequence with a fresh observation and a fresh
    // gesture sample: consumed, but the overlay still resolves.
    let preview2 = GestureIngress {
        modal_move: true,
        ..ingress(
            "win-a",
            GestureKind::Move,
            Some((160, 50)),
            GesturePhase::Updated,
            ra,
        )
    };
    let mut resend = report(
        3,
        3,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(preview2),
        vec![],
        true,
    );
    resend.command_seq = Some(7);
    resend.command = Some(WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: Some(crate::directional::Direction::Right),
    });
    match engine.handle_report(&resend, &[]).0 {
        WorldOutcome::CommandConsumed(reply) => {
            preview_of(&reply);
        }
        other => panic!("expected command-consumed with gesture overlay, got {other:?}"),
    }
}

#[test]
fn incarnation_reset_clears_the_session() {
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    // The same old-generation id is stale after rebind, while the same id
    // in the new generation starts fresh against reset state. The rebind
    // declares executable/processId unavailable, so the post-rebind facts
    // carry them null (a supported adapter sends no conflicting facts).
    let mut old_generation = report(
        1,
        3,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(ingress(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Updated,
            ra,
        )),
        vec![],
        true,
    );
    for fact in &mut old_generation.windows {
        fact.executable = None;
        fact.process_id = None;
    }
    rebind(&mut engine, "adapter-2");
    let mut stale_old_generation = old_generation.clone();
    stale_old_generation.adapter_incarnation = "adapter-2".to_owned();
    assert!(matches!(
        engine.handle_report(&stale_old_generation, &[]).0,
        WorldOutcome::Rejected {
            kind: "generation-mismatch",
            ..
        }
    ));
    let mut reset = old_generation;
    reset.adapter_incarnation = "adapter-2".to_owned();
    reset.core_generation = engine.core_generation().to_owned();
    let stale = accepted(&mut engine, &reset);
    assert!(
        matches!(stale.drag_preview, Some(DragPreviewV4 { .. })),
        "id 1 is fresh in the new generation, not replayed from the old one"
    );
}

#[test]
fn engine_sessionless_gesture_never_fabricates() {
    // A gesture for a lifetime the core never homed refuses even under
    // complete inventory: exact-lifetime binding only.
    let (mut engine, ra, rb) = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-c",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                rect(100, 0, 40, 90),
            )),
            vec![],
            true,
        ),
    );
    assert!(reply.drag_preview.is_none());
    assert!(reply.transfers.is_empty());
}

#[test]
fn missing_ingress_with_close_clears_the_session() {
    // A retained session with no further ingress holds, but an explicit
    // close of the retained lifetime clears exactly: the next update for
    // the closed lifetime is stale with no overlay.
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let cleared = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![WindowId("win-a".to_owned())],
            true,
        ),
    );
    assert!(cleared.drag_preview.is_none());
    let stale = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(stale.drag_preview.is_none());
}

#[test]
fn started_truth_is_immutable_and_zero_gates_preview() {
    // The START cursor is immutable: a later update carrying a different
    // cursor never rebinds it, so a sample back on the down point holds
    // with no overlay even after movement.
    let (mut engine, ra, rb) = rig();
    let mut start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((10, 10)),
        GesturePhase::Started,
        ra,
        1,
    );
    start.modal_move = true;
    start.start_cursor = Some((10, 10));
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(start),
            vec![],
            true,
        ),
    );
    // Same point as START: zero, no overlay.
    let zero = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((10, 10)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(zero.drag_preview.is_none());
    // A moved sample previews.
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
    // Back on the down point: zero again, no overlay, session survives.
    let back = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((10, 10)),
                GesturePhase::Updated,
                rect(999, 999, 10, 10),
            )),
            vec![],
            true,
        ),
    );
    assert!(back.drag_preview.is_none());
}

#[test]
fn terminal_replay_with_same_start_frame_refuses() {
    // Release commits, then an exact replay of the terminal (same stable
    // id on a new observation) refuses without a new commit or transfer:
    // at-most-once orders by id, never by window plus frame.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let alloc = row_of(&released, "win-a").allocation;
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true)),
        "move release records a committed terminal"
    );
    let replay = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(replay.drag_preview.is_none());
    assert!(replay.transfers.is_empty());
    assert_eq!(row_of(&replay, "win-a").allocation, alloc);
}

#[test]
fn started_supersedes_a_consumed_terminal() {
    // After a release consumes id 1, a fresh `Started` with id 2 binds
    // anew even with an identical START frame: only the stable id
    // orders sessions, never the frame.
    let (mut engine, ra, rb) = rig();
    let first = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        first.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, false)),
        "no-change release records uncommitted terminal"
    );
    let mut start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((150, 50)),
        GesturePhase::Started,
        ra,
        2,
    );
    start.modal_move = true;
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(start),
            vec![],
            true,
        ),
    );
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn cancelled_without_session_never_binds_or_shields() {
    // A lone cancel for an unbound lifetime binds nothing but still
    // consumes its id: a replay with the same id stays dead, while the
    // next gesture with a fresh id binds and previews.
    let (mut engine, ra, rb) = rig();
    let cancelled = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Cancelled,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(cancelled.drag_preview.is_none());
    assert_eq!(
        cancelled.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, false)),
        "lone cancel records an uncommitted terminal"
    );
    // Same id replays dead: no bind, no shield, no overlay.
    let replay = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(replay.drag_preview.is_none());
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn cursorless_resize_release_never_commits() {
    // A resize whose release frame changed but whose cursor is missing
    // fails closed: no commit, no transfer.
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-b").allocation;
    let shrunk_b = rect(100, 0, 100, 90);
    let mut release = ingress(
        "win-b",
        GestureKind::Resize,
        None,
        GesturePhase::Released,
        rb,
    );
    release.modal_move = false;
    let released = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", shrunk_b)],
            "win-b",
            Some(release),
            vec![],
            true,
        ),
    );
    assert_eq!(row_of(&released, "win-b").allocation, before_alloc);
    assert!(released.transfers.is_empty());
}

#[test]
fn focused_fullscreen_game_bars_update_and_release() {
    // A focused fullscreen game (different window, staged fullscreen
    // fact) pauses the gesture: updates hold with no overlay and the
    // release refuses with no commit.
    let (mut engine, ra, rb) = rig();
    let mut game = fact("game", "out-a", rect(0, 0, 800, 600));
    game.fullscreen = Some(true);
    game.privileged = Some(true);
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", ra),
                fact("win-b", "out-a", rb),
                game.clone(),
            ],
            "game",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fact("win-a", "out-a", ra),
                fact("win-b", "out-a", rb),
                game.clone(),
            ],
            "game",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(held.drag_preview.is_none());
    let moved_a = rect(100, 0, 40, 90);
    let refused = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", moved_a),
                fact("win-b", "out-a", rb),
                game,
            ],
            "game",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(refused.drag_preview.is_none());
    assert!(refused.transfers.is_empty());
}

#[test]
fn terminal_id_zero_never_binds() {
    // A zero id is invalid on the wire and can never bind: the report
    // refuses before any session work.
    let (mut engine, ra, rb) = rig();
    let mut bad = report(
        2,
        2,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(ingress_id(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Released,
            ra,
            0,
        )),
        vec![],
        true,
    );
    let _ = &mut bad;
    match engine.handle_report(&bad, &[]).0 {
        WorldOutcome::Rejected { .. } => {}
        other => panic!("expected rejection for id 0, got {other:?}"),
    }
    assert!(engine.world.gesture.terminal.is_none());
}

#[test]
fn terminal_consumes_unbindable_identity() {
    // A terminal for a never-homed lifetime consumes its id even though
    // nothing binds: a retry with the same id stays dead, while a fresh
    // id still binds.
    let (mut engine, ra, rb) = rig();
    let refused = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "ghost",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(refused.drag_preview.is_none());
    assert!(refused.transfers.is_empty());
    assert_eq!(
        refused.gesture_settled.map(|t| (t.id, t.committed)),
        Some((7, false)),
        "unbindable terminal still records"
    );
    let replay = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "ghost",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(replay.drag_preview.is_none());
    assert!(replay.transfers.is_empty());
    // A fresh id is unaffected by the consumed one.
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                8,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn new_id_supersedes_older_open_session() {
    // A fresh Started with a newer id replaces a dead retained session
    // without waiting for the old terminal: the old id's later terminal
    // cannot erase the new session.
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Started,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((10, 10)),
                GesturePhase::Started,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    // Stale terminal for the superseded id: no clear, no commit.
    let stale = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(stale.drag_preview.is_none());
    assert!(stale.transfers.is_empty());
    // The newer session is intact and still previews.
    let previewed = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn stale_cancel_cannot_erase_newer_session() {
    // A cancel carrying an older id than the retained session is ignored:
    // the newer gesture survives it.
    let (mut engine, ra, rb) = rig();
    let initial = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    assert!(initial.drag_preview.is_some());
    let ignored = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Cancelled,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(ignored.drag_preview, initial.drag_preview);
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn repeat_updated_and_released_same_id_has_no_replay() {
    // One lifecycle, then the exact same Updated and Released repeat:
    // neither rebinds nor recommits, and the reply carries the terminal
    // exactly once with the commit flag set.
    let (mut engine, ra, rb) = rig();
    let mut start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((10, 10)),
        GesturePhase::Started,
        ra,
        1,
    );
    start.start_cursor = Some((10, 10));
    let started = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(start),
            vec![],
            true,
        ),
    );
    assert!(started.gesture_settled.is_none());
    let moved_a = rect(100, 0, 40, 90);
    let updated = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(updated.gesture_settled.is_none());
    let released = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    let alloc = row_of(&released, "win-a").allocation;
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    assert_eq!(
        engine.world.gesture.terminal.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    // Exact Updated replay: no session, no overlay.
    let stale_update = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(stale_update.drag_preview.is_none());
    // Exact Released replay: no recommit.
    let rerelease = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(rerelease.drag_preview.is_none());
    assert!(rerelease.transfers.is_empty());
    assert_eq!(row_of(&rerelease, "win-a").allocation, alloc);
}

#[test]
fn fresh_id_same_frame_fast_end_commits() {
    // Two terminals sharing one START frame but carrying fresh ids are
    // two gestures, not a replay: a coalesced START+END on a new id
    // commits even right after an earlier terminal consumed its own id.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let first = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        first.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    // Same START frame, fresh id, fast END onto the other output: not
    // a replay, so the release re-derives and commits the cross-output
    // send with its follow transfer.
    let again = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((900, 50)),
                GesturePhase::Released,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    assert!(
        again
            .transfers
            .iter()
            .any(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")),
        "fresh id commits the cross-output send"
    );
    assert_eq!(
        again.gesture_settled.map(|t| (t.id, t.committed)),
        Some((2, true)),
        "fresh id re-derives and commits, not a replay"
    );
}

#[test]
fn missing_ingress_close_records_terminal() {
    // An explicit close with no further ingress clears the session and
    // records the terminal in the reply, so the adapter observes
    // incorporation from the fresh reply.
    let (mut engine, ra, rb) = rig();
    let updated = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(updated.gesture_settled.is_none());
    let closed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![WindowId("win-a".to_owned())],
            true,
        ),
    );
    assert_eq!(
        closed.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, false))
    );
}

#[test]
fn reply_carries_terminal() {
    // The terminal rides the reply, not a sidechannel: id plus commit
    // flag. Canonical wire encoding and byte bounds live in `tiler-protocol`.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
}

#[test]
fn terminal_retains_into_next_report() {
    // Retained latest-wins: a duplicate repeats the cached terminal,
    // and the next unrelated report retains the prior terminal unchanged.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let terminal_report = report(
        2,
        2,
        vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
        "win-a",
        Some(ingress(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Released,
            ra,
        )),
        vec![],
        true,
    );
    let released = accepted(&mut engine, &terminal_report);
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    match engine.handle_report(&terminal_report, &[]).0 {
        WorldOutcome::Duplicate(cached) => assert_eq!(
            cached.gesture_settled.map(|t| (t.id, t.committed)),
            Some((1, true)),
            "duplicate repeats the cached terminal"
        ),
        other => panic!("expected duplicate, got {other:?}"),
    }
    let next = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    assert_eq!(
        next.gesture_settled, released.gesture_settled,
        "unrelated report retains prior terminal unchanged"
    );
}

#[test]
fn terminal_replay_on_new_report_preserves_original_outcome() {
    // Same id on a new observation is a consumed replay: no rebind, no
    // recommit, and the fresh reply retains the original terminal unchanged.
    // The id/observation fences stay safe without repeating the record.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    let replay = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        replay.gesture_settled, released.gesture_settled,
        "consumed replay retains original terminal without re-settling"
    );
    assert!(replay.drag_preview.is_none());
    assert!(replay.transfers.is_empty());
}

#[test]
fn rejected_report_consumes_no_gesture_identity() {
    // A refused report (zero id) is transactional: the watermark never
    // advances, so the next valid gesture still binds and previews.
    let (mut engine, ra, rb) = rig();
    let bad = report(
        2,
        2,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(ingress_id(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Released,
            ra,
            0,
        )),
        vec![],
        true,
    );
    match engine.handle_report(&bad, &[]).0 {
        WorldOutcome::Rejected { .. } => {}
        other => panic!("expected rejection, got {other:?}"),
    }
    let mut start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((10, 10)),
        GesturePhase::Started,
        ra,
        1,
    );
    start.start_cursor = Some((10, 10));
    let started = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(start),
            vec![],
            true,
        ),
    );
    assert!(started.gesture_settled.is_none());
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn q1_gesture_predecessor_remains_through_successor_pending_and_replaces_at_terminal() {
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    // Complete G1 (id=1, committed=true); skip/withhold its issuing reply.
    let g1_terminal = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        g1_terminal.gesture_settled,
        Some(crate::gesture::GestureSettled {
            id: 1,
            committed: true,
        })
    );
    assert!(
        engine.world.gesture.retained.is_none(),
        "G1 is settled and cleared"
    );
    let g1_alloc_a = row_of(&g1_terminal, "win-a")
        .allocation
        .expect("win-a allocation");
    let g1_alloc_b = row_of(&g1_terminal, "win-b")
        .allocation
        .expect("win-b allocation");

    // Start successor G2 (id=2) without releasing: G2 is pending, G1 remains readable.
    let g2_start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((150, 50)),
        GesturePhase::Started,
        g1_alloc_a,
        2,
    );
    let started = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fact("win-a", "out-a", g1_alloc_a),
                fact("win-b", "out-a", g1_alloc_b),
            ],
            "win-a",
            Some(g2_start),
            vec![],
            true,
        ),
    );
    assert_eq!(
        engine.world.gesture.retained.as_ref().map(|g| g.id),
        Some(2),
        "G2 must be pending"
    );
    assert_eq!(
        started.gesture_settled,
        Some(crate::gesture::GestureSettled {
            id: 1,
            committed: true,
        }),
        "Q1: predecessor G1 terminal remains readable while G2 is pending"
    );

    let updated = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", g1_alloc_a),
                fact("win-b", "out-a", g1_alloc_b),
            ],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((650, 50)),
                GesturePhase::Updated,
                g1_alloc_a,
                2,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        engine.world.gesture.retained.as_ref().map(|g| g.id),
        Some(2)
    );
    assert_eq!(
        updated.gesture_settled,
        Some(crate::gesture::GestureSettled {
            id: 1,
            committed: true,
        }),
        "Q1: G1 terminal stays readable across G2 updates"
    );

    // Rejected report (id=0) and stale consumed replay (id=1) while G2 is pending
    // clear nothing and do not replace G1.
    let bad = report(
        5,
        5,
        vec![
            fact("win-a", "out-a", g1_alloc_a),
            fact("win-b", "out-a", g1_alloc_b),
        ],
        "win-a",
        Some(ingress_id(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Released,
            g1_alloc_a,
            0,
        )),
        vec![],
        true,
    );
    assert!(matches!(
        engine.handle_report(&bad, &[]).0,
        WorldOutcome::Rejected { .. }
    ));
    let stale_replay = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![
                fact("win-a", "out-a", g1_alloc_a),
                fact("win-b", "out-a", g1_alloc_b),
            ],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                g1_alloc_a,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        engine.world.gesture.retained.as_ref().map(|g| g.id),
        Some(2)
    );
    assert_eq!(
        stale_replay.gesture_settled,
        Some(crate::gesture::GestureSettled {
            id: 1,
            committed: true,
        })
    );

    // Complete G2 with Cancelled (committed=false): slot replaces only at G2 terminal.
    let g2_terminal = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![
                fact("win-a", "out-a", g1_alloc_a),
                fact("win-b", "out-a", g1_alloc_b),
            ],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((180, 50)),
                GesturePhase::Cancelled,
                g1_alloc_a,
                2,
            )),
            vec![],
            true,
        ),
    );
    assert!(engine.world.gesture.retained.is_none());
    assert_eq!(
        g2_terminal.gesture_settled,
        Some(crate::gesture::GestureSettled {
            id: 2,
            committed: false,
        }),
        "G2 terminal replaces G1"
    );
}

#[test]
fn q2_gesture_survives_subject_close_within_generation_and_clears_on_new_generation() {
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released.gesture_settled,
        Some(crate::gesture::GestureSettled {
            id: 1,
            committed: true,
        })
    );

    // Close win-a and follow with an unrelated observation (even reusing win-a):
    // gesture_settled is generation-scoped and survives subject close.
    let closed_reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![WindowId("win-a".to_owned())],
            true,
        ),
    );
    assert_eq!(closed_reply.gesture_settled, released.gesture_settled);

    let reused_reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![],
            true,
        ),
    );
    assert_eq!(reused_reply.gesture_settled, released.gesture_settled);

    // Fresh handshake / new generation clears all retained settlement owners.
    rebind(&mut engine, "adapter-1");
    let gen2_bootstrap = engine.handle_report(
        &WorldReportV4 {
            core_generation: engine.core_generation().to_owned(),
            ..report(
                1,
                1,
                vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
                "win-a",
                None,
                vec![],
                true,
            )
        },
        &[],
    );
    match gen2_bootstrap.0 {
        WorldOutcome::Accepted(reply) => {
            assert_eq!(reply.gesture_settled, None);
            assert!(reply.operation_settled.is_empty());
            assert!(reply.switch_settled.is_empty());
        }
        other => panic!("expected accepted in new generation, got {other:?}"),
    }
}

#[test]
fn retained_drag_preview_persists_across_omitted_ingress_and_clears_on_fences_and_neighbor_departure()
 {
    let (mut engine, ra, rb) = rig();
    let updated = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((160, 50)),
        GesturePhase::Updated,
        ra,
        7,
    );
    let previewed = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(updated.clone()),
            vec![],
            true,
        ),
    );
    let first_preview = previewed
        .drag_preview
        .clone()
        .expect("updated gesture produces drag_preview");
    assert_eq!(first_preview.id, 7);

    // Omitted ingress preserves the retained drag_preview.
    let omitted = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    assert_eq!(omitted.drag_preview, Some(first_preview.clone()));

    // Omitted ingress under incomplete inventory invalidates drag_preview.
    let gapped = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            false,
        ),
    );
    assert!(gapped.drag_preview.is_none());

    // Re-establish preview, then depart the neighbor on omitted ingress.
    let restored = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(updated),
            vec![],
            true,
        ),
    );
    assert_eq!(restored.drag_preview.as_ref().map(|p| p.id), Some(7));
    let neighbor_closed = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-a", ra)],
            "win-a",
            None,
            vec![first_preview.target.clone()],
            true,
        ),
    );
    assert!(neighbor_closed.drag_preview.is_none());
}

#[test]
fn retained_drag_preview_clears_on_genuine_held_no_preview_and_ignores_newcomer() {
    let (mut engine, ra, rb) = rig();
    let mut start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((10, 10)),
        GesturePhase::Started,
        ra,
        7,
    );
    start.start_cursor = Some((10, 10));
    let started = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(start),
            vec![],
            true,
        ),
    );
    assert!(started.drag_preview.is_none());

    let updated = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((160, 50)),
        GesturePhase::Updated,
        ra,
        7,
    );
    let previewed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(updated.clone()),
            vec![],
            true,
        ),
    );
    let active_preview = previewed
        .drag_preview
        .clone()
        .expect("updated gesture produces drag_preview");
    assert_eq!(active_preview.id, 7);

    // Genuinely ignored stale newcomer (older id <= retained.id) and unbindable
    // newcomer cannot relabel id or clear/authorize preview.
    let stale_newcomer = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-b",
                GestureKind::Move,
                Some((20, 50)),
                GesturePhase::Updated,
                rb,
                6,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(stale_newcomer.drag_preview, Some(active_preview.clone()));

    let unbindable_newcomer = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "missing-win",
                GestureKind::Move,
                Some((20, 50)),
                GesturePhase::Started,
                rb,
                8,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(unbindable_newcomer.drag_preview, Some(active_preview));

    // Genuine current-session Held (missing cursor) clears drag_preview.
    let no_cursor = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                None,
                GesturePhase::Updated,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(no_cursor.drag_preview.is_none());

    // Restore preview, then zero-movement Held clears drag_preview.
    let restored = accepted(
        &mut engine,
        &report(
            7,
            7,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(updated),
            vec![],
            true,
        ),
    );
    assert!(restored.drag_preview.is_some());
    let zero_move = accepted(
        &mut engine,
        &report(
            8,
            8,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((10, 10)),
                GesturePhase::Updated,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(zero_move.drag_preview.is_none());
}

#[test]
fn fresh_replacement_gesture_clears_predecessor_drag_preview_and_old_id_cannot_repaint() {
    let (mut engine, ra, rb) = rig();
    let updated_old = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((160, 50)),
        GesturePhase::Updated,
        ra,
        7,
    );
    let previewed = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(updated_old.clone()),
            vec![],
            true,
        ),
    );
    assert_eq!(previewed.drag_preview.as_ref().map(|p| p.id), Some(7));

    // Fresh replacement id 8 (> 7) supersedes id 7 and clears its drag_preview.
    let mut replacement = ingress_id(
        "win-b",
        GestureKind::Move,
        Some((150, 50)),
        GesturePhase::Started,
        rb,
        8,
    );
    replacement.start_cursor = Some((150, 50));
    let replaced = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            Some(replacement),
            vec![],
            true,
        ),
    );
    assert!(
        replaced.drag_preview.is_none(),
        "fresh replacement bind-only Started clears predecessor drag_preview"
    );

    // Old id 7 cannot repaint once superseded by id 8.
    let old_repaint = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            Some(updated_old),
            vec![],
            true,
        ),
    );
    assert!(old_repaint.drag_preview.is_none());
}

#[test]
fn retained_drag_preview_clears_on_omitted_ingress_game_pause_style_hold_and_domain_loss() {
    let (mut engine, ra, rb) = rig();
    let updated = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((160, 50)),
        GesturePhase::Updated,
        ra,
        7,
    );
    let previewed = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(updated.clone()),
            vec![],
            true,
        ),
    );
    assert_eq!(previewed.drag_preview.as_ref().map(|p| p.id), Some(7));

    // Game pause on omitted ingress (focused other window is fullscreen) clears drag_preview.
    let mut paused_neighbor = fact("win-b", "out-a", rb);
    paused_neighbor.fullscreen = Some(true);
    let paused = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), paused_neighbor],
            "win-b",
            None,
            vec![],
            true,
        ),
    );
    assert!(paused.drag_preview.is_none());

    // Restore preview, then style/scope fence on omitted ingress (maximized subject)
    // invalidates drag_preview via gesture_continue_fences.
    let restored = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(updated.clone()),
            vec![],
            true,
        ),
    );
    assert!(restored.drag_preview.is_some());
    let mut style_held_subject = fact("win-a", "out-a", ra);
    style_held_subject.maximized = Some(true);
    let style_held = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![style_held_subject, fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    assert!(style_held.drag_preview.is_none());

    // Restore preview, then domain loss (unreadable screen) on omitted ingress clears drag_preview.
    let restored2 = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(updated),
            vec![],
            true,
        ),
    );
    assert!(restored2.drag_preview.is_some());
    let mut bad_screen_report = report(
        7,
        7,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        None,
        vec![],
        true,
    );
    bad_screen_report.screens[0].complete = false;
    bad_screen_report.screens[0].full_rect = None;
    bad_screen_report.screens[0].work_area = None;
    let domain_lost = accepted(&mut engine, &bad_screen_report);
    assert!(domain_lost.drag_preview.is_none());
}

#[test]
fn retained_drag_preview_clears_on_released_cancelled_subject_close_and_rehandshake() {
    // Released clears drag_preview.
    let (mut engine, ra, rb) = rig();
    let previewed = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(previewed.drag_preview.is_some());
    let released = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Released,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(released.drag_preview.is_none());

    // Cancelled clears drag_preview.
    let previewed2 = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                8,
            )),
            vec![],
            true,
        ),
    );
    assert!(previewed2.drag_preview.is_some());
    let cancelled = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Cancelled,
                ra,
                8,
            )),
            vec![],
            true,
        ),
    );
    assert!(cancelled.drag_preview.is_none());

    // staged.closes on subject clears drag_preview.
    let previewed3 = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                9,
            )),
            vec![],
            true,
        ),
    );
    assert!(previewed3.drag_preview.is_some());
    let subject_closed = accepted(
        &mut engine,
        &report(
            7,
            7,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![WindowId("win-a".to_owned())],
            true,
        ),
    );
    assert!(subject_closed.drag_preview.is_none());

    // Fresh handshake clears drag_preview.
    let previewed4 = accepted(
        &mut engine,
        &report(
            8,
            8,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            Some(ingress_id(
                "win-b",
                GestureKind::Move,
                Some((20, 50)),
                GesturePhase::Updated,
                rb,
                10,
            )),
            vec![],
            true,
        ),
    );
    let _ = previewed4;
    rebind(&mut engine, "adapter-1");
    let after_handshake = match engine.handle_report(
        &WorldReportV4 {
            core_generation: engine.core_generation().to_owned(),
            ..report(
                1,
                1,
                vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
                "win-a",
                None,
                vec![],
                true,
            )
        },
        &[],
    ) {
        (WorldOutcome::Accepted(reply), _) => reply,
        (other, _) => panic!("expected accepted after handshake, got {other:?}"),
    };
    assert!(after_handshake.drag_preview.is_none());
}

#[test]
fn retained_drag_preview_clears_on_active_held_resize_and_engine_refusal() {
    let (mut engine, ra, rb) = rig();
    // Engine refusal (cursor over self centre) clears retained drag_preview.
    let previewed = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(previewed.drag_preview.is_some());
    let win_a_frame = previewed
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .and_then(|w| w.effective_frame)
        .expect("win-a frame");
    let self_refused = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((
                    win_a_frame.x + win_a_frame.w / 2,
                    win_a_frame.y + win_a_frame.h / 2,
                )),
                GesturePhase::Updated,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(self_refused.drag_preview.is_none());

    // Non-Move (Resize) holds with no drag_preview.
    let resize_held = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Resize,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                8,
            )),
            vec![],
            true,
        ),
    );
    assert!(resize_held.drag_preview.is_none());
}

#[test]
fn retained_drag_preview_clears_on_omitted_ingress_local_held_style_publication_hold() {
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..crate::connection::HostCapabilities::default()
        },
        vec![
            (
                "gap".to_owned(),
                crate::connection::RawSettingValue::Number(0.0),
            ),
            (
                "outerGap".to_owned(),
                crate::connection::RawSettingValue::Number(0.0),
            ),
            (
                "borderWidth".to_owned(),
                crate::connection::RawSettingValue::Number(32.0),
            ),
            (
                "borderGap".to_owned(),
                crate::connection::RawSettingValue::Number(0.0),
            ),
            (
                "borderRadius".to_owned(),
                crate::connection::RawSettingValue::Number(0.0),
            ),
        ],
    )
    .expect("generation");
    let ra = rect(0, 0, 40, 90);
    let rb = rect(100, 0, 40, 90);
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "",
            None,
            vec![],
            true,
        ),
    );

    let previewed = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(previewed.drag_preview.as_ref().map(|p| p.id), Some(7));

    // Omitted ingress with a producer style hold on out-a (scale * borderWidth
    // exceeds the 1024px scale_float_once cap) leaves gesture_continue_fences satisfied while
    // local_held publication pruning sets out-a hold=Unreadable and clears
    // drag_preview.
    let mut style_held_report = report(
        3,
        3,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        None,
        vec![],
        true,
    );
    style_held_report.screens[0].scale = 32.015625;
    style_held_report.presentation = PresentationRowV4::Valid(PresentationIngressV4 {
        foreground: "win-a".to_owned(),
        ..PresentationIngressV4::default()
    });
    let held_reply = accepted(&mut engine, &style_held_report);
    let out_a = held_reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .expect("out-a output row");
    assert_eq!(
        out_a.hold,
        Some(crate::world_report::HoldReason::Unreadable),
        "style gate publishes Unreadable hold on out-a"
    );
    assert!(
        held_reply.drag_preview.is_none(),
        "local_held publication prune clears retained drag_preview"
    );
}

fn host_native_fact(window: &str, output: &str, r: Rect, native_ws: &str) -> WindowFact {
    let mut row = fact(window, output, r);
    row.native_workspace = crate::world_report::NativeWorkspaceFact::Value(native_ws.to_owned());
    row
}

fn host_native_obs(
    output: &str,
    order: &[&str],
    current: &str,
) -> crate::world_report::NativeObservation {
    use crate::world_report::{NativeObservation, NativeOccupancy};
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|s| (*s).to_owned()).collect(),
        current_native: Some(current.to_owned()),
        occupancy: order
            .iter()
            .map(|n| NativeOccupancy {
                native: (*n).to_owned(),
                count: None,
            })
            .collect(),
    }
}

#[test]
fn same_output_drag_preview_on_non_first_active_workspace_succeeds() {
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let ra = rect(0, 0, 400, 600);
    let rb = rect(400, 0, 400, 600);

    let mut seed = report(
        1,
        1,
        vec![
            host_native_fact("win-a", "out-a", ra, "n1"),
            host_native_fact("win-b", "out-a", rb, "n1"),
        ],
        "",
        None,
        vec![],
        true,
    );
    seed.native = vec![
        host_native_obs("out-a", &["n0", "n1"], "n1"),
        host_native_obs("out-b", &["b0"], "b0"),
    ];
    let seeded = accepted(&mut engine, &seed);
    let alloc_a = row_of(&seeded, "win-a").allocation.expect("win-a slot");
    let alloc_b = row_of(&seeded, "win-b").allocation.expect("win-b slot");

    let mut settle_report = report(
        2,
        2,
        vec![
            host_native_fact("win-a", "out-a", alloc_a, "n1"),
            host_native_fact("win-b", "out-a", alloc_b, "n1"),
        ],
        "win-a",
        None,
        vec![],
        true,
    );
    settle_report.native = vec![
        host_native_obs("out-a", &["n0", "n1"], "n1"),
        host_native_obs("out-b", &["b0"], "b0"),
    ];
    accepted(&mut engine, &settle_report);

    let mut move_report = report(
        3,
        3,
        vec![
            host_native_fact("win-a", "out-a", alloc_a, "n1"),
            host_native_fact("win-b", "out-a", alloc_b, "n1"),
        ],
        "win-a",
        Some(ingress(
            "win-a",
            GestureKind::Move,
            Some((alloc_b.x + alloc_b.w - 20, alloc_b.y + alloc_b.h / 2)),
            GesturePhase::Updated,
            alloc_a,
        )),
        vec![],
        true,
    );
    move_report.native = vec![
        host_native_obs("out-a", &["n0", "n1"], "n1"),
        host_native_obs("out-b", &["b0"], "b0"),
    ];
    let mut previewed = None;
    let traces = crate::world_engine::with_drag_preview_trace_capture(|| {
        previewed = Some(accepted(&mut engine, &move_report));
    });
    let previewed = previewed.expect("reply");

    let preview = previewed.drag_preview.as_ref().unwrap_or_else(|| {
        panic!(
            "same-output move on non-first active workspace must produce drag_preview; traces={traces:?}, view={:?}",
            engine.tx_ro(|tx| tx.effective_view(&OutputId("out-a".to_owned())))
        )
    });
    assert_eq!(preview.domain.output.0, "out-a");
    assert_eq!(
        preview.domain.workspace.0,
        engine
            .tx_ro(|tx| tx.effective_view(&OutputId("out-a".to_owned())))
            .expect("active view")
    );
    assert!(preview.rect.w > 0 && preview.rect.h > 0);
}

#[test]
fn cross_output_drag_preview_selects_non_first_active_workspace_on_destination() {
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let ra = rect(0, 0, 800, 600);
    let rb = rect(800, 0, 800, 600);

    let mut seed = report(
        1,
        1,
        vec![
            host_native_fact("win-a", "out-a", ra, "a0"),
            host_native_fact("win-b", "out-b", rb, "b1"),
        ],
        "",
        None,
        vec![],
        true,
    );
    seed.native = vec![
        host_native_obs("out-a", &["a0"], "a0"),
        host_native_obs("out-b", &["b0", "b1"], "b1"),
    ];
    accepted(&mut engine, &seed);

    let mut move_report = report(
        2,
        2,
        vec![
            host_native_fact("win-a", "out-a", ra, "a0"),
            host_native_fact("win-b", "out-b", rb, "b1"),
        ],
        "win-a",
        Some(ingress(
            "win-a",
            GestureKind::Move,
            Some((1500, 300)),
            GesturePhase::Updated,
            ra,
        )),
        vec![],
        true,
    );
    move_report.native = vec![
        host_native_obs("out-a", &["a0"], "a0"),
        host_native_obs("out-b", &["b0", "b1"], "b1"),
    ];
    let previewed = accepted(&mut engine, &move_report);

    let preview = previewed
        .drag_preview
        .as_ref()
        .expect("cross-output move to non-first active workspace must produce drag_preview");
    assert_eq!(preview.domain.output.0, "out-b");
    assert_eq!(
        preview.domain.workspace.0,
        engine
            .tx_ro(|tx| tx.effective_view(&OutputId("out-b".to_owned())))
            .expect("active view on out-b")
    );
    assert!(preview.rect.w > 0 && preview.rect.h > 0);
}

#[test]
fn drag_preview_refuses_when_active_workspace_is_unreadable_without_falling_back_to_inactive() {
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let ra = rect(0, 0, 800, 600);
    let rb = rect(800, 0, 800, 600);

    let mut bad_fact = host_native_fact("win-bad", "out-b", rb, "b1");
    bad_fact.rect = None;

    let mut seed = report(
        1,
        1,
        vec![
            host_native_fact("win-a", "out-a", ra, "a0"),
            host_native_fact("win-b0", "out-b", rb, "b0"),
            host_native_fact("win-b", "out-b", rb, "b1"),
            bad_fact.clone(),
        ],
        "",
        None,
        vec![],
        true,
    );
    seed.native = vec![
        host_native_obs("out-a", &["a0"], "a0"),
        host_native_obs("out-b", &["b0", "b1"], "b1"),
    ];
    accepted(&mut engine, &seed);

    let mut move_report = report(
        2,
        2,
        vec![
            host_native_fact("win-a", "out-a", ra, "a0"),
            host_native_fact("win-b0", "out-b", rb, "b0"),
            host_native_fact("win-b", "out-b", rb, "b1"),
            bad_fact,
        ],
        "win-a",
        Some(ingress(
            "win-a",
            GestureKind::Move,
            Some((1500, 300)),
            GesturePhase::Updated,
            ra,
        )),
        vec![],
        true,
    );
    move_report.native = vec![
        host_native_obs("out-a", &["a0"], "a0"),
        host_native_obs("out-b", &["b0", "b1"], "b1"),
    ];
    let previewed = accepted(&mut engine, &move_report);

    assert!(
        previewed.drag_preview.is_none(),
        "unreadable active workspace on out-b must refuse without falling back to inactive b0"
    );
}

#[test]
fn drag_pointer_domain_staged_refuses_when_active_workspace_is_held_without_falling_back_to_inactive()
 {
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let ra = rect(0, 0, 800, 600);
    let rb = rect(800, 0, 800, 600);

    let mut seed = report(
        1,
        1,
        vec![
            host_native_fact("win-a", "out-a", ra, "a0"),
            host_native_fact("win-b0", "out-b", rb, "b0"),
            host_native_fact("win-b", "out-b", rb, "b1"),
        ],
        "",
        None,
        vec![],
        true,
    );
    seed.native = vec![
        host_native_obs("out-a", &["a0"], "a0"),
        host_native_obs("out-b", &["b0", "b1"], "b1"),
    ];
    accepted(&mut engine, &seed);

    let active_ws = engine
        .tx_ro(|tx| tx.effective_view(&OutputId("out-b".to_owned())))
        .expect("active view on out-b");
    let active_key = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(active_ws),
    };

    let empty_held = std::collections::BTreeSet::new();
    let (selected, _) = engine
        .tx(|tx| tx.drag_pointer_domain_staged(1500, 300, &empty_held))
        .expect("unheld active workspace on out-b must be selected");
    assert_eq!(selected, active_key);

    let mut held_scopes = std::collections::BTreeSet::new();
    held_scopes.insert(active_key);
    assert!(
        engine
            .tx(|tx| tx.drag_pointer_domain_staged(1500, 300, &held_scopes))
            .is_none(),
        "held active workspace on out-b must refuse without falling back to inactive b0"
    );
}

#[test]
fn omitted_ingress_clears_drag_preview_when_destination_effective_view_changes_while_retaining_same_view()
 {
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let ra = rect(0, 0, 800, 600);
    let rb = rect(800, 0, 800, 600);

    let mut seed = report(
        1,
        1,
        vec![
            host_native_fact("win-a", "out-a", ra, "a0"),
            host_native_fact("win-b0", "out-b", rb, "b0"),
            host_native_fact("win-b", "out-b", rb, "b1"),
        ],
        "",
        None,
        vec![],
        true,
    );
    seed.native = vec![
        host_native_obs("out-a", &["a0"], "a0"),
        host_native_obs("out-b", &["b0", "b1"], "b1"),
    ];
    accepted(&mut engine, &seed);

    let mut move_report = report(
        2,
        2,
        vec![
            host_native_fact("win-a", "out-a", ra, "a0"),
            host_native_fact("win-b0", "out-b", rb, "b0"),
            host_native_fact("win-b", "out-b", rb, "b1"),
        ],
        "win-a",
        Some(ingress(
            "win-a",
            GestureKind::Move,
            Some((1500, 300)),
            GesturePhase::Updated,
            ra,
        )),
        vec![],
        true,
    );
    move_report.native = vec![
        host_native_obs("out-a", &["a0"], "a0"),
        host_native_obs("out-b", &["b0", "b1"], "b1"),
    ];
    let previewed = accepted(&mut engine, &move_report);
    let preview = previewed
        .drag_preview
        .clone()
        .expect("cross-output move to active b1 must produce drag_preview");
    assert_eq!(preview.domain.output.0, "out-b");
    assert_eq!(
        preview.domain.workspace.0,
        engine
            .tx_ro(|tx| tx.effective_view(&OutputId("out-b".to_owned())))
            .expect("active view on out-b")
    );

    // Omitted ingress on the same active destination view retains the preview.
    let mut same_view_omitted = report(
        3,
        3,
        vec![
            host_native_fact("win-a", "out-a", ra, "a0"),
            host_native_fact("win-b0", "out-b", rb, "b0"),
            host_native_fact("win-b", "out-b", rb, "b1"),
        ],
        "win-a",
        None,
        vec![],
        true,
    );
    same_view_omitted.native = vec![
        host_native_obs("out-a", &["a0"], "a0"),
        host_native_obs("out-b", &["b0", "b1"], "b1"),
    ];
    let same_view_reply = accepted(&mut engine, &same_view_omitted);
    assert_eq!(
        same_view_reply.drag_preview.as_ref(),
        Some(&preview),
        "omitted ingress with unchanged destination effective_view must retain drag_preview"
    );

    // Omitted ingress after destination effective_view switches to b0 must clear
    // the preview even though old workspace b1 remains readable and unheld.
    let mut switched_view_omitted = report(
        4,
        4,
        vec![
            host_native_fact("win-a", "out-a", ra, "a0"),
            host_native_fact("win-b0", "out-b", rb, "b0"),
            host_native_fact("win-b", "out-b", rb, "b1"),
        ],
        "win-a",
        None,
        vec![],
        true,
    );
    switched_view_omitted.native = vec![
        host_native_obs("out-a", &["a0"], "a0"),
        host_native_obs("out-b", &["b0", "b1"], "b0"),
    ];
    let mut switched_reply = None;
    let traces = crate::world_engine::with_drag_preview_trace_capture(|| {
        switched_reply = Some(accepted(&mut engine, &switched_view_omitted));
    });
    let switched_reply = switched_reply.expect("reply");
    assert!(
        switched_reply.drag_preview.is_none(),
        "omitted ingress after destination effective_view switch must clear drag_preview; view={:?}, traces={traces:?}",
        engine.tx_ro(|tx| tx.effective_view(&OutputId("out-b".to_owned())))
    );
}

#[test]
fn center_drop_refuses_without_commit_and_retains_session() {
    let (mut engine, ra, _) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", rect(0, 0, 400, 600)),
                fact("win-b", "out-a", rect(400, 0, 400, 600)),
            ],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    let before_a = row_of(&before, "win-a").allocation.expect("win-a alloc");
    let before_b = row_of(&before, "win-b").allocation.expect("win-b alloc");

    // Preview over center of win-b (600, 300) refuses without overlay and retains session.
    let center_preview = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fact("win-a", "out-a", before_a),
                fact("win-b", "out-a", before_b),
            ],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((600, 300)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(center_preview.drag_preview.is_none());

    // Session is still active: edge update over win-b previews.
    let edge_preview = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", before_a),
                fact("win-b", "out-a", before_b),
            ],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((before_b.x + before_b.w - 5, before_b.y + before_b.h / 2)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(edge_preview.drag_preview.is_some());

    // Release over center of win-b refuses without commit and clears the session.
    let center_release = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![
                fact("win-a", "out-a", before_a),
                fact("win-b", "out-a", before_b),
            ],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((before_b.x + before_b.w / 2, before_b.y + before_b.h / 2)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(center_release.drag_preview.is_none());
    assert_eq!(
        center_release.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, false))
    );
    assert_eq!(row_of(&center_release, "win-a").allocation, Some(before_a));
    assert_eq!(row_of(&center_release, "win-b").allocation, Some(before_b));
}

#[test]
fn deterministic_move_replay_produces_identical_preview_and_commit() {
    let run_once = || {
        let (mut engine, ra, _) = rig();
        let seeded = accepted(
            &mut engine,
            &report(
                2,
                2,
                vec![
                    fact("win-a", "out-a", rect(0, 0, 400, 600)),
                    fact("win-b", "out-a", rect(400, 0, 400, 600)),
                ],
                "win-a",
                None,
                vec![],
                true,
            ),
        );
        let a0 = row_of(&seeded, "win-a").allocation.expect("win-a alloc");
        let b0 = row_of(&seeded, "win-b").allocation.expect("win-b alloc");
        let previewed = accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![fact("win-a", "out-a", a0), fact("win-b", "out-a", b0)],
                "win-a",
                Some(ingress(
                    "win-a",
                    GestureKind::Move,
                    Some((790, 300)),
                    GesturePhase::Updated,
                    ra,
                )),
                vec![],
                true,
            ),
        );
        let released = accepted(
            &mut engine,
            &report(
                4,
                4,
                vec![fact("win-a", "out-a", a0), fact("win-b", "out-a", b0)],
                "win-a",
                Some(ingress(
                    "win-a",
                    GestureKind::Move,
                    Some((790, 300)),
                    GesturePhase::Released,
                    ra,
                )),
                vec![],
                true,
            ),
        );
        (
            previewed.drag_preview,
            released.gesture_settled,
            row_of(&released, "win-a").allocation,
            row_of(&released, "win-b").allocation,
        )
    };
    assert_eq!(run_once(), run_once());
}

#[test]
fn failed_center_preview_retains_group_edge_stickiness() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let seed = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fact("win-a", "out-a", rect(0, 0, 200, 600)),
                fact("win-b", "out-a", rect(200, 0, 200, 600)),
                fact("win-c", "out-a", rect(400, 0, 200, 600)),
            ],
            "win-c",
            None,
            vec![],
            true,
        ),
    );
    let a0 = row_of(&seed, "win-a").allocation.expect("win-a alloc");
    let b0 = row_of(&seed, "win-b").allocation.expect("win-b alloc");
    let c0 = row_of(&seed, "win-c").allocation.expect("win-c alloc");
    let nested = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", a0),
                fact("win-b", "out-a", b0),
                fact("win-c", "out-a", c0),
            ],
            "win-c",
            Some(ingress_id(
                "win-c",
                GestureKind::Move,
                Some((b0.x + b0.w / 2, b0.y + b0.h - 2)),
                GesturePhase::Released,
                c0,
                1,
            )),
            vec![],
            true,
        ),
    );
    let a1 = row_of(&nested, "win-a").allocation.expect("win-a nested");
    let b1 = row_of(&nested, "win-b").allocation.expect("win-b nested");
    let c1 = row_of(&nested, "win-c").allocation.expect("win-c nested");
    assert!(b1.y + b1.h < c1.y, "vertical group must have an inner gap");
    let inner_x = b1.x.min(c1.x);
    let gx = inner_x + 5;
    let gy = (b1.y + b1.h + c1.y) / 2;
    let sticky_x = inner_x + 50;
    let center_x = b1.x + b1.w / 2;
    let center_y = b1.y + b1.h / 2;

    let edge_reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fact("win-a", "out-a", a1),
                fact("win-b", "out-a", b1),
                fact("win-c", "out-a", c1),
            ],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((gx, gy)),
                GesturePhase::Updated,
                a1,
                2,
            )),
            vec![],
            true,
        ),
    );
    let edge = edge_reply
        .drag_preview
        .as_ref()
        .expect("group edge preview");
    assert_eq!(
        edge.hover
            .as_ref()
            .and_then(|h| h.prior.as_ref())
            .map(|p| p.edge),
        Some(crate::contract::DragSide::Left)
    );

    let center_reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", a1),
                fact("win-b", "out-a", b1),
                fact("win-c", "out-a", c1),
            ],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((center_x, center_y)),
                GesturePhase::Updated,
                a1,
                2,
            )),
            vec![],
            true,
        ),
    );
    assert!(center_reply.drag_preview.is_none());
    assert!(center_reply.transfers.is_empty());
    assert_eq!(row_of(&center_reply, "win-a").allocation, Some(a1));
    assert_eq!(row_of(&center_reply, "win-b").allocation, Some(b1));
    assert_eq!(row_of(&center_reply, "win-c").allocation, Some(c1));
    assert_eq!(
        engine
            .world
            .gesture
            .retained
            .as_ref()
            .and_then(|g| g.hover.as_ref())
            .and_then(|h| h.prior.as_ref())
            .map(|p| p.edge),
        Some(crate::contract::DragSide::Left)
    );

    let sticky_reply = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![
                fact("win-a", "out-a", a1),
                fact("win-b", "out-a", b1),
                fact("win-c", "out-a", c1),
            ],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((sticky_x, gy)),
                GesturePhase::Updated,
                a1,
                2,
            )),
            vec![],
            true,
        ),
    );
    let sticky = sticky_reply
        .drag_preview
        .as_ref()
        .expect("sticky edge preview");
    assert_eq!(sticky.rect, edge.rect);
    assert_eq!(sticky.hover, edge.hover);

    let cancelled = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![
                fact("win-a", "out-a", a1),
                fact("win-b", "out-a", b1),
                fact("win-c", "out-a", c1),
            ],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((sticky_x, gy)),
                GesturePhase::Cancelled,
                a1,
                2,
            )),
            vec![],
            true,
        ),
    );
    assert!(cancelled.drag_preview.is_none());
    assert!(cancelled.transfers.is_empty());
    assert_eq!(row_of(&cancelled, "win-a").allocation, Some(a1));
    assert_eq!(row_of(&cancelled, "win-b").allocation, Some(b1));
    assert_eq!(row_of(&cancelled, "win-c").allocation, Some(c1));
}

// Cross-output gesture release suite: pair-targeted slot agreement,
// retained transfer fences, and gap bookkeeping live in the child;
// shared rig helpers above are used via `super::*`.
#[path = "world_gesture_tests/cross_output.rs"]
mod cross_output;
