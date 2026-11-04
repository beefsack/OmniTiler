//! Selection reply dual-writer parity over an actual producer reply.
//!
//! Drives the minimal native-free suppression-capable explicit selection
//! route through the real engine (boot, send-aside, select), then asserts
//! the transport writer spells the producer reply byte-identically to the
//! core canonical writer with exact size accounting, and that the wire
//! carries the `awaitShown` grant plus the same-revision show/hide pair.
//!
//! Offline only: no D-Bus, no KWin.

use tiler_core::connection::HostCapabilities;
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ArrivalBracket, ArrivalLocation, FocusFeedback, LastWrite, LastWritePath, LastWriteProperty,
    ScopeAuthority, ScreenReport, VisibilityFact, VisibilityReason, VisibilitySample,
    VisibilitySetter, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};
use tiler_protocol::world_protocol::encode_reply;

fn rect(x: i32) -> Rect {
    Rect {
        x,
        y: 0,
        w: 40,
        h: 90,
    }
}

fn screen() -> ScreenReport {
    ScreenReport {
        output: OutputId("out-a".to_owned()),
        full_rect: Some(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        }),
        work_area: Some(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        }),
        complete: true,
        scale: 1.0,
        identity: tiler_core::world_report::OutputIdentity::unknown(),
    }
}

/// Suppression-capable member fact mirroring the core await-route lane:
/// project suppression supported, native visibility readable, non-minimized,
/// non-hidden, non-native-owned.
fn sfact(window: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId("out-a".to_owned())),
        rect: Some(rect(x)),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: None,
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: tiler_core::world_report::NativeWorkspaceFact::Omitted,
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
        window_type: Some(tiler_core::world_report::WindowType::Normal),
        activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: Some(true),
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: Some(false),
        },
        last_write: None,
    }
}

fn report(
    seq: u64,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    focused: &str,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        screens: vec![screen()],
        windows: vec![sfact("w1", 0), sfact("w2", 100)],
        views: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: tiler_core::world_report::PresentationIngress::disabled(),
        intent_records: Vec::new(),
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

fn confirmed_write(target: bool, rev: u64, post_obs: u64) -> LastWrite {
    LastWrite {
        property: LastWriteProperty::Visibility,
        target,
        revision: rev,
        path: LastWritePath::Staged,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(VisibilitySample {
            obs: post_obs,
            lifetime_matched: Some(true),
            visibility: VisibilityFact {
                visible: Some(target),
                suppression_owned: Some(false),
            },
            minimized: Some(false),
            host_hidden: Some(false),
        }),
    }
}

fn settle_report(
    seq: u64,
    w1_last: LastWrite,
    w2_last: LastWrite,
    feedback: FocusFeedback,
) -> WorldReport {
    let mut base = report(seq, None, None, "");
    base.windows = vec![
        {
            let mut fact = sfact("w1", 0);
            fact.last_write = Some(w1_last);
            fact
        },
        {
            let mut fact = sfact("w2", 100);
            fact.last_write = Some(w2_last);
            fact
        },
    ];
    base.focus_feedback = vec![feedback];
    base
}

#[test]
fn selection_reply_spells_through_both_writers() {
    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            window_suppression: true,
            maximize: true,
            ..Default::default()
        },
    )
    .expect("generation");
    let boot = accepted(&mut engine, &report(1, None, None, ""));
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    let away = ring[1].clone();
    // Stage w2 on the away workspace without following, so the explicit
    // selection below actually changes the view and resolves w2. The
    // transfer requires the mover to be the observed focus.
    let _ = accepted(
        &mut engine,
        &report(
            2,
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w2".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            "w2",
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            "",
        ),
    );
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    assert_eq!(reply.effects[0].target.0, "w2");
    assert!(
        reply.effects[0].await_shown,
        "selection focus awaits arrival"
    );

    // Both writers spell the actual producer reply byte-identically, and
    // the structural estimate covers it.
    let wire = encode_reply(&reply);
    assert_eq!(
        wire,
        tiler_core::world_report::encode_reply_canonical(&reply),
        "transport mirrors the core writer over a selection reply"
    );
    assert!(
        tiler_core::world_report::reply_estimate(&reply) >= wire.len(),
        "estimate covers the selection reply"
    );
    assert!(
        wire.contains("\"awaitShown\":true"),
        "wire carries the grant"
    );

    // The wire pair rides one authorizing revision with no fallback.
    let show = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w2")
        .and_then(|row| row.visibility)
        .expect("selected home show");
    let hide = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .and_then(|row| row.visibility)
        .expect("departing home hide");
    assert!(show.shown && !hide.shown);
    assert!(!show.fallback && !hide.fallback);
    assert_eq!(show.revision, reply.state_revision);
    assert_eq!(hide.revision, show.revision);
    assert!(wire.contains(&format!("\"rev\":\"{}\"", show.revision)));
    for row in &reply.windows {
        assert_eq!(
            row.visibility_authority,
            ScopeAuthority::Controlled,
            "selection rows stay controlled"
        );
    }
}

#[test]
fn selection_reply_carries_switch_terminal_through_both_writers() {
    // Real-producer parity through the actual terminal: the same
    // boot/send-aside/select route, then raw LastWrite plus arrival plus
    // focus feedback settle the parent op on the real engine.
    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            window_suppression: true,
            maximize: true,
            ..Default::default()
        },
    )
    .expect("generation");
    let boot = accepted(&mut engine, &report(1, None, None, ""));
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    let away = ring[1].clone();
    let _ = accepted(
        &mut engine,
        &report(
            2,
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w2".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            "w2",
        ),
    );
    let granted = accepted(
        &mut engine,
        &report(
            3,
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            "",
        ),
    );
    let effect = granted.effects[0].clone();
    assert!(effect.await_shown, "selection focus awaits arrival");
    let rev = granted.state_revision;
    let frame = granted
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w2")
        .and_then(|row| row.effective_frame);
    let sample = |obs: u64| VisibilitySample {
        obs,
        lifetime_matched: Some(true),
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: Some(false),
        },
        minimized: Some(false),
        host_hidden: Some(false),
    };
    let feedback = FocusFeedback {
        id: effect.id.clone(),
        target: effect.target.clone(),
        revision: effect.issued_revision,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(effect.target.clone()),
        foreground_proven: true,
        arrival: vec![ArrivalBracket {
            window: effect.target.clone(),
            obs: 4,
            before: sample(4),
            location: ArrivalLocation::Output {
                out: Some(OutputId("out-a".to_owned())),
            },
            frame,
            after: sample(4),
        }],
        attempt_ms: 7,
    };
    let settled = accepted(
        &mut engine,
        &settle_report(
            4,
            confirmed_write(false, rev, 4),
            confirmed_write(true, rev, 4),
            feedback,
        ),
    );
    assert_eq!(settled.switch_settled.len(), 1, "one terminal: {settled:?}");
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(
        event.outcome,
        tiler_core::world_report::SwitchOutcome::Complete
    );
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, tiler_core::world_report::SwitchFocus::Proven);
    assert!(!event.fallback);
    // Both writers spell the terminal reply byte-identically with the
    // exact canonical event row in frozen key order.
    let wire = encode_reply(&settled);
    assert_eq!(
        wire,
        tiler_core::world_report::encode_reply_canonical(&settled),
        "transport mirrors the core writer over a switch terminal"
    );
    assert!(
        wire.contains(
            "\"switchSettled\":[{\"op\":\"op-3\",\"outcome\":\"complete\",\
             \"failed\":[],\"focus\":\"proven\",\"fallback\":false}]"
        ),
        "exact canonical event row in frozen key order"
    );
    // Marginal switch lane is exact against its wire bytes (the estimate
    // carries the 19-byte section key while the section writer keeps the
    // 2 array brackets), and the whole-reply estimate bounds the wire.
    let mut section = String::new();
    tiler_core::world_sizing::encode_switch_settled(&mut section, &settled.switch_settled);
    let estimated = tiler_core::world_sizing::switch_settled_estimate(&settled.switch_settled);
    assert_eq!(
        estimated,
        section.len() + 17,
        "section key accounts the gap"
    );
    assert!(
        tiler_core::world_report::reply_estimate(&settled) >= wire.len(),
        "estimate covers the terminal reply"
    );
    // The terminal never repeats: the next report publishes empty.
    let again = accepted(&mut engine, &report(5, None, None, ""));
    assert!(
        again.switch_settled.is_empty(),
        "next report publishes empty"
    );
}
