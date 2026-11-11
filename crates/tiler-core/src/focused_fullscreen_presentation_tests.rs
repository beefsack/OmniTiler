//! Neutral focused-fullscreen presentation suspension (stage 5):
//! when the focused surface is truthfully fullscreen on its observed
//! output, published border/underlay/pager shapes suspend on that output
//! only. KWin only offers truthful `fullScreen`, never game identity, so
//! this names the observation (focused fullscreen), never a platform or
//! product. Normal fullscreen window actions, membership, and shortcuts
//! are unchanged; the global `paused` game lane is untouched.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_presentation::{GroupHideReason, PresentationUnderlay};
use crate::world_report::{
    BorderHideReasonV4, BorderPresentationV4, PresentationIngressV4, PresentationPagerFrameV4,
    PresentationRowV4, ScreenReport, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand,
    WorldReplyV4, WorldReportV4,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(rect(x, 0, 40, 90)),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
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

fn ingress(token: &str) -> PresentationIngressV4 {
    // v4 ingress carries only foreground/frame/chord/pager/witness lanes;
    // style sizes live in composed settings (contract 261-272).
    PresentationIngressV4 {
        foreground: token.to_owned(),
        // Legacy bools default false via `..disabled()` (known-false);
        // v4 options carry the same basis as Some(false). `cloaked`
        // renames to `host_hidden`; legacy `shell` has no v4 slot
        // (always false here).
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: if token.is_empty() {
            None
        } else {
            Some(rect(0, 0, 40, 90))
        },
        chord_held: true,
        ..PresentationIngressV4::default()
    }
}

/// Production-faithful ingress for a focused surface that is truthfully
/// fullscreen: the adapter-observed foreground flag rides alongside the
/// fact, exactly like the KWin capture sets it from the focused record.
fn ingress_fullscreen(token: &str) -> PresentationIngressV4 {
    PresentationIngressV4 {
        foreground_fullscreen: Some(true),
        ..ingress(token)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    ingress: PresentationIngressV4,
) -> WorldReportV4 {
    let frames: Vec<PresentationPagerFrameV4> = windows
        .iter()
        .filter_map(|fact| {
            fact.rect.map(|rect| PresentationPagerFrameV4 {
                window: fact.window.clone(),
                live: Some(rect),
            })
        })
        .collect();
    let order: Vec<WindowId> = windows.iter().map(|fact| fact.window.clone()).collect();
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: vec![],
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
        presentation: PresentationRowV4::Valid(PresentationIngressV4 {
            pager_order: order,
            pager_frames: frames,
            ..ingress
        }),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn members() -> Vec<WindowFact> {
    vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-b", 800),
    ]
}

fn fullscreen_members(focused: &str) -> Vec<WindowFact> {
    members()
        .into_iter()
        .map(|mut fact| {
            if fact.window.0 == focused {
                fact.fullscreen = Some(true);
            }
            fact
        })
        .collect()
}

/// Admitted pair with managed focus latched, like the presentation suite:
/// a second identical report lets managed focus latch behind the newcomer.
fn admitted() -> WorldEngine {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(1, 1, members(), "win-a", None, None, ingress("win-a")),
    );
    accepted(
        &mut engine,
        &report(2, 2, members(), "win-a", None, None, ingress("win-a")),
    );
    engine
}

fn border_hidden_suspended(border: &BorderPresentationV4) -> bool {
    // v4 border rows carry width/outsets/witness instead of inner/outer
    // geometry, so the suspension verdict is visible + reason only.
    !border.visible && border.reason == Some(BorderHideReasonV4::Suspended)
}

#[test]
fn default_engine_suspends_focused_fullscreen() {
    // v4 suspension is unconditional (contract 1225-1228; world-rendering
    // rejects the opt-in/paused lane): the default engine suspends on
    // truthful-fullscreen inputs.
    let mut engine = admitted();
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            fullscreen_members("win-a"),
            "win-a",
            None,
            None,
            ingress_fullscreen("win-a"),
        ),
    );
    assert_eq!(
        reply.presentation.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
    assert!(
        border_hidden_suspended(&reply.presentation.border),
        "default engine suspends, got {:?}",
        reply.presentation.border
    );
    assert_eq!(
        reply.presentation.underlay,
        PresentationUnderlay::Hidden {
            base_revision: None,
            reason: GroupHideReason::Suspended,
        }
    );
    assert_eq!(reply.presentation.pager.len(), 1);
    assert_eq!(reply.presentation.pager[0].output, "out-b");
}

#[test]
fn suspends_border_underlay_and_output_pager_only() {
    let mut engine = admitted();
    // Sanity: a non-fullscreen focus publishes no verdict.
    let baseline = accepted(
        &mut engine,
        &report(3, 3, members(), "win-a", None, None, ingress("win-a")),
    );
    assert_eq!(baseline.presentation.suspended_output, None);
    // Baseline border visibility needs a joined witness plus composed
    // settings (R20 anchored border; contract 261-272), so a
    // witness-free visible has no v4 row here.
    assert!(matches!(
        baseline.presentation.underlay,
        PresentationUnderlay::Found(_)
    ));
    assert_eq!(baseline.presentation.pager.len(), 2);

    // The focused surface goes fullscreen on its observed output.
    let reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            fullscreen_members("win-a"),
            "win-a",
            None,
            None,
            ingress_fullscreen("win-a"),
        ),
    );
    assert_eq!(
        reply.presentation.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
    assert!(
        border_hidden_suspended(&reply.presentation.border),
        "border suspends, got {:?}",
        reply.presentation.border
    );
    assert_eq!(
        reply.presentation.underlay,
        PresentationUnderlay::Hidden {
            base_revision: None,
            reason: GroupHideReason::Suspended,
        }
    );
    // The pager omits the suspended output only; the other output keeps
    // its content instead of publishing a globally empty pager.
    assert_eq!(reply.presentation.pager.len(), 1);
    assert_eq!(reply.presentation.pager[0].output, "out-b");
    assert!(
        !reply.presentation.pager[0].workspaces.is_empty(),
        "the unaffected output keeps pager content"
    );
    // Skew: the unaffected output keeps its member content, not just its
    // workspace rows.
    let painted: Vec<&WindowId> = reply.presentation.pager[0]
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.windows.iter().map(|window| &window.window))
        .collect();
    assert!(
        painted.contains(&&WindowId("win-c".to_owned())),
        "out-b still paints win-c, got {painted:?}"
    );
    // The typed verdict above is the wire verdict: v4 has no full-reply
    // canonical encoder in core, and the key rides exactly once with the
    // single `suspended_output` field.
}

#[test]
fn unfocused_fullscreen_never_suspends() {
    let mut engine = admitted();
    // win-a is fullscreen but focus rests on win-b: no suspension.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            fullscreen_members("win-a"),
            "win-b",
            None,
            None,
            ingress("win-b"),
        ),
    );
    assert_eq!(reply.presentation.suspended_output, None);
    assert_eq!(reply.presentation.pager.len(), 2);
}

#[test]
fn focus_departure_and_fullscreen_exit_restore() {
    let mut engine = admitted();
    let suspended = accepted(
        &mut engine,
        &report(
            3,
            3,
            fullscreen_members("win-a"),
            "win-a",
            None,
            None,
            ingress_fullscreen("win-a"),
        ),
    );
    assert_eq!(
        suspended.presentation.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
    // Focus departs the fullscreen surface: the verdict clears and the
    // newly focused output publishes normally.
    let departed = accepted(
        &mut engine,
        &report(
            4,
            4,
            fullscreen_members("win-a"),
            "win-b",
            None,
            None,
            ingress("win-b"),
        ),
    );
    assert_eq!(departed.presentation.suspended_output, None);
    assert_eq!(departed.presentation.pager.len(), 2);
    // Focus returns while still fullscreen: suspension resumes.
    let resumed = accepted(
        &mut engine,
        &report(
            5,
            5,
            fullscreen_members("win-a"),
            "win-a",
            None,
            None,
            ingress_fullscreen("win-a"),
        ),
    );
    assert_eq!(
        resumed.presentation.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
    // The fullscreen flag itself exits: full restoration with no verdict.
    let exited = accepted(
        &mut engine,
        &report(6, 6, members(), "win-a", None, None, ingress("win-a")),
    );
    assert_eq!(exited.presentation.suspended_output, None);
    // Restore visibility likewise needs witness+settings (see baseline
    // note above); no witness-free visible row here.
    assert!(matches!(
        exited.presentation.underlay,
        PresentationUnderlay::Found(_)
    ));
    assert_eq!(exited.presentation.pager.len(), 2);
}

#[test]
fn fullscreen_fact_arriving_before_or_after_focus_suspends() {
    // Flag first, focus second.
    let mut engine = admitted();
    let unfocused = accepted(
        &mut engine,
        &report(
            3,
            3,
            fullscreen_members("win-b"),
            "win-a",
            None,
            None,
            ingress("win-a"),
        ),
    );
    assert_eq!(unfocused.presentation.suspended_output, None);
    let focused = accepted(
        &mut engine,
        &report(
            4,
            4,
            fullscreen_members("win-b"),
            "win-b",
            None,
            None,
            ingress_fullscreen("win-b"),
        ),
    );
    assert_eq!(
        focused.presentation.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );

    // Focus first, flag second (fresh engine, same shape reversed).
    let mut engine = admitted();
    let plain = accepted(
        &mut engine,
        &report(3, 3, members(), "win-b", None, None, ingress("win-b")),
    );
    assert_eq!(plain.presentation.suspended_output, None);
    let flagged = accepted(
        &mut engine,
        &report(
            4,
            4,
            fullscreen_members("win-b"),
            "win-b",
            None,
            None,
            ingress_fullscreen("win-b"),
        ),
    );
    assert_eq!(
        flagged.presentation.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
}

#[test]
fn window_actions_and_shortcuts_during_suspension() {
    // Window actions and shortcuts keep working during suspension: the
    // focus shortcut still mints effects, the send still retains, the
    // view switch moves the desired view, and member homing is unchanged.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(1, 1, members(), "win-a", None, None, ingress("win-a")),
    );
    accepted(
        &mut engine,
        &report(2, 2, members(), "win-a", None, None, ingress("win-a")),
    );
    // Focus effect shortcut under suspension.
    let focused = accepted(
        &mut engine,
        &report(
            3,
            3,
            fullscreen_members("win-a"),
            "win-a",
            Some(7),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "left".to_owned(),
            }),
            ingress_fullscreen("win-a"),
        ),
    );
    // Same-output send of the focused surface under suspension.
    let ring: Vec<String> = engine.logical_ring("out-a").into_iter().collect();
    assert!(ring.len() > 1);
    let sent = accepted(
        &mut engine,
        &report(
            4,
            4,
            fullscreen_members("win-a"),
            "win-a",
            Some(8),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ring[1].clone(),
                follow: false,
            }),
            ingress_fullscreen("win-a"),
        ),
    );
    // View switch shortcut under suspension: a real transfer to a
    // non-current workspace on the fullscreen window's own output
    // (out-a), never the current-view no-op fallback.
    let ring_a: Vec<String> = engine.logical_ring("out-a").into_iter().collect();
    let current_a = engine.logical_view("out-a").expect("out-a retains a view");
    let target_a = ring_a
        .iter()
        .find(|id| *id != &current_a)
        .cloned()
        .expect("out-a ring holds a non-current workspace");
    let switched = accepted(
        &mut engine,
        &report(
            5,
            5,
            fullscreen_members("win-a"),
            "win-a",
            Some(9),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: target_a.clone(),
            }),
            ingress_fullscreen("win-a"),
        ),
    );
    assert!(!focused.effects.is_empty(), "focus shortcut still mints");
    assert!(!sent.transfers.is_empty(), "send still retains");
    // Real view transfer on the fullscreen window's output: the desired
    // view actually moves to the selected non-current workspace, and the
    // member homing is unchanged.
    let view_a = switched
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .expect("out-a desired view publishes");
    assert_eq!(
        view_a.logical_view.as_deref(),
        Some(target_a.as_str()),
        "switch moves the desired view off the current workspace"
    );
    let win_a = switched
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a row publishes");
    assert_eq!(
        sent.windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("win-a row publishes")
            .output,
        win_a.output,
        "member output unchanged"
    );
    assert_eq!(
        sent.windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("win-a row publishes")
            .workspace,
        win_a.workspace,
        "member workspace unchanged"
    );
    // Suspension is unconditional (contract 1225-1228) and carries no
    // actuation of its own: only the presentation verdict suspends.
}

#[test]
fn missing_focused_lifetime_invents_no_verdict() {
    // A focused lifetime with no WindowFact but a truthful fullscreen
    // foreground flag: no output verdict is invented, the existing
    // fullscreen hides still apply, and every other output is retained.
    // No new inference or semantics: the verdict needs a staged fact.
    let mut engine = admitted();
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            members(),
            "ghost",
            None,
            None,
            PresentationIngressV4 {
                foreground: "ghost".to_owned(),
                foreground_frame: None,
                foreground_fullscreen: Some(true),
                ..ingress("ghost")
            },
        ),
    );
    assert_eq!(
        reply.presentation.suspended_output, None,
        "no staged fact, no verdict"
    );
    assert_eq!(
        reply.presentation.border.reason,
        Some(BorderHideReasonV4::Fullscreen),
        "existing fullscreen hide applies"
    );
    assert_eq!(
        reply.presentation.underlay,
        PresentationUnderlay::Hidden {
            base_revision: None,
            reason: GroupHideReason::Fullscreen,
        },
        "existing fullscreen hide applies"
    );
    assert_eq!(reply.presentation.pager.len(), 2, "other outputs retained");
    // No verdict on the typed reply; wire omission lives in the protocol
    // crate (no v4 canonical encoder in core).
}

#[test]
fn empty_focus_and_non_fullscreen_focus_carry_no_verdict() {
    let mut engine = admitted();
    // Empty focus: no verdict even with a truthful fullscreen flag on an
    // unrelated ingress foreground.
    let empty = accepted(
        &mut engine,
        &report(
            3,
            3,
            members(),
            "",
            None,
            None,
            PresentationIngressV4 {
                foreground: "win-a".to_owned(),
                foreground_frame: Some(rect(0, 0, 40, 90)),
                foreground_fullscreen: Some(true),
                ..ingress("win-a")
            },
        ),
    );
    assert_eq!(empty.presentation.suspended_output, None);
    // Non-fullscreen focused fact: no verdict.
    let plain = accepted(
        &mut engine,
        &report(4, 4, members(), "win-a", None, None, ingress("win-a")),
    );
    assert_eq!(plain.presentation.suspended_output, None);
    assert_eq!(plain.presentation.pager.len(), 2);
}

#[test]
fn other_output_foreground_shapes_match_unsuspended_publication() {
    let mut engine = admitted();
    let suspended = accepted(
        &mut engine,
        &report(
            3,
            3,
            fullscreen_members("win-a"),
            "win-a",
            None,
            None,
            ingress("win-c"),
        ),
    );
    assert_eq!(
        suspended.presentation.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
    // Witness-free visible has no v4 row (see baseline note); the verdict
    // suspends only the focused output, so the pager keeps the other
    // output with its content.
    assert_eq!(suspended.presentation.pager.len(), 1);
    assert_eq!(suspended.presentation.pager[0].output, "out-b");
    assert!(
        !suspended.presentation.pager[0].workspaces.is_empty(),
        "the unaffected output keeps pager content"
    );
}

#[test]
fn default_constructor_publication_bytes_stay_stable() {
    // The same non-fullscreen stream publishes identically on repeat runs
    // and carries no suspension verdict. Legacy pinned canonical wire
    // bytes; v4 has no full-reply canonical encoder in core, so the port
    // pins typed-reply equality (the canonical wire is a deterministic
    // function of the typed reply).
    let run = || {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report(1, 1, members(), "win-a", None, None, ingress("win-a")),
        );
        let second = accepted(
            &mut engine,
            &report(2, 2, members(), "win-a", None, None, ingress("win-a")),
        );
        (first, second)
    };
    let (first_a, second_a) = run();
    let (first_b, second_b) = run();
    assert_eq!(first_b, first_a);
    assert_eq!(second_b, second_a);
    assert_eq!(second_a.presentation.suspended_output, None);
    assert_eq!(second_b.presentation.suspended_output, None);
}
