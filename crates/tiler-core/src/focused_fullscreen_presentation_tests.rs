//! Opt-in neutral focused-fullscreen presentation suspension (stage 5):
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
    match engine.handle_report(report) {
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
fn admitted(optin: bool) -> WorldEngine {
    let mut engine = if optin {
        WorldEngine::new_test_bound("gen-1", "adapter-1")
            .expect("generation")
            .with_focused_fullscreen_presentation_suspension()
    } else {
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
    };
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
    // rejects the opt-in/paused lane): the default engine suspends on the
    // same truthful-fullscreen inputs as the opted-in engine.
    let mut engine = admitted(false);
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
fn optin_suspends_border_underlay_and_output_pager_only() {
    let mut engine = admitted(true);
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
    let mut engine = admitted(true);
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
    let mut engine = admitted(true);
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
    let mut engine = admitted(true);
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
    let mut engine = admitted(true);
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
fn window_actions_and_shortcuts_match_default_during_suspension() {
    // Identical report streams through the default and opted-in engines:
    // sends, view switches, and focus effects actuate identically; only
    // the presentation verdict differs.
    let run = |optin: bool| {
        let mut engine = if optin {
            WorldEngine::new_test_bound("gen-1", "adapter-1")
                .expect("generation")
                .with_focused_fullscreen_presentation_suspension()
        } else {
            WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
        };
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
        (focused, sent, switched, target_a)
    };
    let (focus_plain, send_plain, switch_plain, target_plain) = run(false);
    let (focus_opt, send_opt, switch_opt, target_opt) = run(true);
    assert_eq!(
        target_opt, target_plain,
        "same non-current target both lanes"
    );
    assert_eq!(focus_opt.effects, focus_plain.effects);
    assert!(!focus_opt.effects.is_empty(), "focus shortcut still mints");
    assert_eq!(send_opt.transfers, send_plain.transfers);
    assert!(!send_opt.transfers.is_empty(), "send still retains");
    assert_eq!(send_opt.windows, send_plain.windows);
    // Real view transfer on the fullscreen window's output: the desired
    // view actually moves, and the member homing is unchanged.
    let view_a_opt = switch_opt
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .expect("out-a desired view publishes");
    assert_eq!(
        view_a_opt.logical_view.as_deref(),
        Some(target_opt.as_str()),
        "switch moves the desired view off the current workspace"
    );
    assert_eq!(switch_opt.outputs, switch_plain.outputs);
    assert_eq!(switch_opt.transfers, switch_plain.transfers);
    assert_eq!(switch_opt.windows, switch_plain.windows);
    let win_a_opt = switch_opt
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a row publishes");
    let win_a_plain = switch_plain
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a row publishes");
    assert_eq!(
        win_a_opt.output, win_a_plain.output,
        "member output unchanged"
    );
    assert_eq!(
        win_a_opt.workspace, win_a_plain.workspace,
        "member workspace unchanged"
    );
    // Cross-lane verdict difference has no v4 expression: suspension is
    // unconditional (contract 1225-1228), so both lanes suspend alike.
    // The identical actuation pinned above is the port.
}

#[test]
fn missing_focused_lifetime_invents_no_verdict() {
    // A focused lifetime with no WindowFact but a truthful fullscreen
    // foreground flag: no output verdict is invented, the existing
    // fullscreen hides still apply, and every other output is retained.
    // No new inference or semantics: the verdict needs a staged fact.
    for optin in [false, true] {
        let mut engine = admitted(optin);
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
            "no staged fact, no verdict (optin={optin})"
        );
        assert_eq!(
            reply.presentation.border.reason,
            Some(BorderHideReasonV4::Fullscreen),
            "existing fullscreen hide applies (optin={optin})"
        );
        assert_eq!(
            reply.presentation.underlay,
            PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Fullscreen,
            },
            "existing fullscreen hide applies (optin={optin})"
        );
        assert_eq!(
            reply.presentation.pager.len(),
            2,
            "other outputs retained (optin={optin})"
        );
        // No verdict on the typed reply (optin={optin}); wire omission
        // lives in the protocol crate (no v4 canonical encoder in core).
    }
}

#[test]
fn empty_focus_and_non_fullscreen_focus_carry_no_verdict() {
    let mut engine = admitted(true);
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
    let run = |optin| {
        let mut engine = admitted(optin);
        accepted(
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
        )
    };
    let plain = run(false);
    let suspended = run(true);
    assert_eq!(
        suspended.presentation.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
    // Witness-free visible has no v4 row (see baseline note); the shape
    // equalities below port (both lanes compute the same v4 verdicts).
    assert_eq!(suspended.presentation.border, plain.presentation.border);
    assert_eq!(suspended.presentation.underlay, plain.presentation.underlay);
    assert_eq!(
        suspended.presentation.pager,
        plain
            .presentation
            .pager
            .into_iter()
            .filter(|screen| screen.output == "out-b")
            .collect::<Vec<_>>()
    );
}

#[test]
fn default_constructor_publication_bytes_stay_stable() {
    // The same non-fullscreen stream encodes identically with and without
    // the opt-in, and omits the suspension key in both cases.
    let run = |optin: bool| {
        let mut engine = if optin {
            WorldEngine::new_test_bound("gen-1", "adapter-1")
                .expect("generation")
                .with_focused_fullscreen_presentation_suspension()
        } else {
            WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
        };
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
    let (first_plain, second_plain) = run(false);
    let (first_opt, second_opt) = run(true);
    // The same non-fullscreen stream publishes identically with and
    // without the opt-in and carries no suspension verdict in either
    // case. Legacy pinned canonical wire bytes; v4 has no full-reply
    // canonical encoder in core, so the port pins typed-reply equality
    // (the canonical wire is a deterministic function of the typed
    // reply).
    assert_eq!(first_opt, first_plain);
    assert_eq!(second_opt, second_plain);
    assert_eq!(second_plain.presentation.suspended_output, None);
    assert_eq!(second_opt.presentation.suspended_output, None);
}

#[test]
fn handshake_preserves_focused_fullscreen_optin_across_rehandshake() {
    use crate::connection::{ApplicationIdentity, HandshakeRequest, HostCapabilities};
    fn req(adapter: &str, id: &str) -> HandshakeRequest {
        HandshakeRequest {
            adapter: adapter.to_owned(),
            request: id.to_owned(),
            application_identity: ApplicationIdentity {
                app_id: None,
                executable: None,
                process_id: None,
            },
            capabilities: HostCapabilities::without_native_identity(),
        }
    }
    let mut engine = WorldEngine::new("gen-1")
        .expect("generation")
        .with_focused_fullscreen_presentation_suspension();
    assert!(engine.focused_fullscreen_presentation_suspension);
    let first = engine.handshake(&req("adapter-1", "handshake-1"));
    assert!(first.kind.is_none());
    assert!(engine.focused_fullscreen_presentation_suspension);
    // Wrong/foreign requests still refuse without mutation.
    let bad = engine.handshake(&req("adapter-1", "bad id!!"));
    assert!(bad.kind.is_some());
    assert!(engine.focused_fullscreen_presentation_suspension);
    let second = engine.handshake(&req("adapter-1", "handshake-2"));
    assert!(second.kind.is_none());
    assert!(engine.focused_fullscreen_presentation_suspension);
}
