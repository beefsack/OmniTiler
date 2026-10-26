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
use crate::visual::{BorderHideReason, BorderPresentation};
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_presentation::{GroupHideReason, PresentationUnderlay};
use crate::world_report::{
    PresentationIngress, PresentationPagerFrame, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact,
    WorldCommand, WorldReply, WorldReport, encode_reply_canonical,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary: x == 0,
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
        native_workspace: None,
    }
}

fn ingress(token: &str) -> PresentationIngress {
    PresentationIngress {
        foreground: token.to_owned(),
        foreground_frame: if token.is_empty() {
            None
        } else {
            Some(rect(0, 0, 40, 90))
        },
        border_enabled: true,
        underlay_enabled: true,
        border_gap_px: 2,
        border_width_px: 4,
        underlay_extension_px: 3,
        chord_held: true,
        ..PresentationIngress::disabled()
    }
}

/// Production-faithful ingress for a focused surface that is truthfully
/// fullscreen: the adapter-observed foreground flag rides alongside the
/// fact, exactly like the KWin capture sets it from the focused record.
fn ingress_fullscreen(token: &str) -> PresentationIngress {
    PresentationIngress {
        foreground_fullscreen: true,
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
    ingress: PresentationIngress,
) -> WorldReport {
    let frames: Vec<PresentationPagerFrame> = windows
        .iter()
        .filter_map(|fact| {
            fact.rect.map(|rect| PresentationPagerFrame {
                window: fact.window.clone(),
                live: Some(rect),
                float_frame: None,
                tile_frame: None,
            })
        })
        .collect();
    let order: Vec<WindowId> = windows.iter().map(|fact| fact.window.clone()).collect();
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
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        windows,
        views: Vec::new(),
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        born_maximized: crate::born_maximized::BornMaximized::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        default_tiled: true,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: PresentationIngress {
            pager_order: order,
            pager_frames: frames,
            ..ingress
        },
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> WorldReply {
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
                fact.fullscreen = true;
            }
            fact
        })
        .collect()
}

/// Admitted pair with managed focus latched, like the presentation suite:
/// a second identical report lets managed focus latch behind the newcomer.
fn admitted(optin: bool) -> WorldEngine {
    let mut engine = if optin {
        WorldEngine::new("gen-1")
            .expect("generation")
            .with_focused_fullscreen_presentation_suspension()
    } else {
        WorldEngine::new("gen-1").expect("generation")
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

fn border_hidden_suspended(border: &BorderPresentation) -> bool {
    !border.visible
        && border.reason == Some(BorderHideReason::Suspended)
        && border.inner.is_none()
        && border.outer.is_none()
}

#[test]
fn default_engine_never_suspends_focused_fullscreen() {
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
    assert_eq!(reply.presentation.suspended_output, None);
    // The ordinary fullscreen gates still apply without the opt-in: the
    // border hides with its accurate reason, not the suspension verdict.
    assert_eq!(
        reply.presentation.border.reason,
        Some(BorderHideReason::Fullscreen)
    );
    assert_eq!(
        reply.presentation.underlay,
        PresentationUnderlay::Hidden {
            base_revision: None,
            reason: GroupHideReason::Fullscreen,
        }
    );
    // Both outputs still publish pager content.
    assert_eq!(reply.presentation.pager.len(), 2);
    // Omitted when none: the canonical wire carries no suspension key.
    assert!(!encode_reply_canonical(&reply).contains("suspendedOutput"));
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
    assert!(baseline.presentation.border.visible);
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
    // The wire names the verdict exactly once.
    let encoded = encode_reply_canonical(&reply);
    assert_eq!(encoded.matches("suspendedOutput").count(), 1);
    assert!(encoded.contains("\"suspendedOutput\":\"out-a\""));
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
    assert!(exited.presentation.border.visible);
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
            WorldEngine::new("gen-1")
                .expect("generation")
                .with_focused_fullscreen_presentation_suspension()
        } else {
            WorldEngine::new("gen-1").expect("generation")
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
    // Only the presentation verdict differs.
    assert_eq!(
        send_opt.presentation.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
    assert_eq!(send_plain.presentation.suspended_output, None);
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
                PresentationIngress {
                    foreground: "ghost".to_owned(),
                    foreground_frame: None,
                    foreground_fullscreen: true,
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
            Some(BorderHideReason::Fullscreen),
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
        assert!(
            !encode_reply_canonical(&reply).contains("suspendedOutput"),
            "no verdict on the wire (optin={optin})"
        );
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
            PresentationIngress {
                foreground: "win-a".to_owned(),
                foreground_frame: Some(rect(0, 0, 40, 90)),
                foreground_fullscreen: true,
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
    assert!(suspended.presentation.border.visible);
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
            WorldEngine::new("gen-1")
                .expect("generation")
                .with_focused_fullscreen_presentation_suspension()
        } else {
            WorldEngine::new("gen-1").expect("generation")
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
    assert_eq!(
        encode_reply_canonical(&first_opt),
        encode_reply_canonical(&first_plain)
    );
    assert_eq!(
        encode_reply_canonical(&second_opt),
        encode_reply_canonical(&second_plain)
    );
    assert!(!encode_reply_canonical(&second_plain).contains("suspendedOutput"));
    assert!(!encode_reply_canonical(&second_opt).contains("suspendedOutput"));
}
