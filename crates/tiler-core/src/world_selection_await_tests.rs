//! Native-free suppression-capable explicit selection visibility/readiness
//! plus terminal switch settlement: an actual explicit
//! `SelectWorkspace`/`SwitchView` routed through `retain_view` that resolves
//! a focus target publishes same-output selected-home shows plus
//! departing-home hides under one authorization revision with an
//! `awaitShown` focus grant, then settles one terminal `SwitchEvent` on the
//! existing parent view operation once every frozen switch member has a
//! first-correlated `LastWrite` verdict and the first focus feedback
//! latches. Focusless selections (no required focus: empty-to-empty,
//! occupied-to-empty, minimized-only) freeze the same visibility scope and
//! settle one terminal `SwitchEvent` with `focus:unproven` once visibility
//! obligations are satisfied (zero members complete at issuing). Ordinary
//! focus, admission, hover, sends, and every out-of-scope route keep
//! `awaitShown=false` with no visibility targets and no switch events.

use std::collections::BTreeSet;

use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{LegacyWorldOutcome, WorldEngine};
use crate::world_report::{
    ArrivalBracket, ArrivalLocation, AuthorityScope, FocusFeedback, FrameSetter, HeldScreenRow,
    HeldWindowRow, LastWrite, LastWritePath, LastWriteProperty, MemberAttempt, OneShotEffect,
    RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane, ScopeAuthority, SwitchFocus,
    SwitchOutcome, VisibilityFact, VisibilityReason, VisibilitySample, VisibilitySetter,
    WORLD_CONTRACT_VERSION_LEGACY, WORLD_MAX_REPLY_BYTES, WindowFact, WorkspaceRef, WorldCommand,
    WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> crate::world_report::ScreenReport {
    crate::world_report::ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 600)),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

/// Suppression-capable member fact: project suppression supported, native
/// visibility readable, non-minimized, non-hidden, non-native-owned.
fn sfact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(rect(x, 0, 40, 90)),
        fullscreen: Some(false),
        maximized: Some(false),
        // Composed route: native sticky unsupported, so sticky stays null
        // (any non-null bool would narrowly hold the row).
        sticky: None,
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
        suppression_available: Some(true),
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: Some(false),
        },
        last_write: None,
    }
}

fn caps() -> HostCapabilities {
    HostCapabilities {
        window_suppression: true,
        maximize: true,
        ..Default::default()
    }
}

fn caps_no_suppression() -> HostCapabilities {
    HostCapabilities {
        window_suppression: false,
        maximize: true,
        ..Default::default()
    }
}

fn caps_host_owned() -> HostCapabilities {
    HostCapabilities {
        window_suppression: true,
        maximize: true,
        host_owned: true,
        ..Default::default()
    }
}

/// Occupied whole-workspace migration fixture: suppression plus native
/// sticky (sticky support is independent of host ownership), so a typed
/// `sticky: Some(false)` member fact is legitimate and stages instead of
/// conflicting. The plain `caps()` harness declares `native_sticky: false`,
/// where any non-null sticky rightly holds the row.
fn caps_migration() -> HostCapabilities {
    HostCapabilities {
        window_suppression: true,
        maximize: true,
        native_sticky: true,
        ..Default::default()
    }
}

fn engine_migration() -> WorldEngine {
    engine_with_caps(caps_migration())
}

fn engine() -> WorldEngine {
    engine_with_caps(caps())
}

fn engine_with_caps(caps: HostCapabilities) -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps).expect("generation")
}

fn sfact_owned(window: &str, output: &str, x: i32) -> WindowFact {
    let mut fact = sfact(window, output, x);
    fact.owned = Some(true);
    fact
}

fn sfact_fullscreen(window: &str, output: &str, x: i32) -> WindowFact {
    let mut fact = sfact(window, output, x);
    fact.fullscreen = Some(true);
    fact
}

fn sfact_maximized(window: &str, output: &str, x: i32) -> WindowFact {
    let mut fact = sfact(window, output, x);
    fact.maximized = Some(true);
    fact
}

#[allow(clippy::too_many_arguments)]
fn report(
    seq: u64,
    windows: Vec<WindowFact>,
    closed: Vec<&str>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    feedback: Vec<FocusFeedback>,
    focused: &str,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION_LEGACY,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0)],
        windows,
        views: Vec::new(),
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
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
        focus_feedback: feedback,
        presentation: crate::world_report::PresentationIngress::disabled(),
        intent_records: Vec::new(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report_legacy(report) {
        LegacyWorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn ring_ids(reply: &crate::world_report::WorldReply, output: &str) -> Vec<String> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output)
        .map(|row| row.id.clone())
        .collect()
}

fn row<'a>(
    reply: &'a crate::world_report::WorldReply,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing window row {window}"))
}

fn select_setup() -> (WorldEngine, String, String) {
    let mut engine = engine();
    let boot = accepted(
        &mut engine,
        &report(
            1,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let ring = ring_ids(&boot, "out-a");
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    let (home, away) = (ring[0].clone(), ring[1].clone());
    // Send w2 to the away workspace without following, so the next explicit
    // selection actually changes the view and resolves w2. The engine
    // transfer requires the mover to be the observed focus.
    let sent = accepted(
        &mut engine,
        &report(
            2,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w2".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            vec![],
            "w2",
        ),
    );
    let _ = sent;
    (engine, home, away)
}

fn select(engine: &mut WorldEngine, seq: u64, workspace: &str) -> crate::world_report::WorldReply {
    accepted(
        engine,
        &report(
            seq,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(seq),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: workspace.to_owned(),
            }),
            vec![],
            "",
        ),
    )
}

fn sample(
    obs: u64,
    matched: bool,
    visible: bool,
    minimized: bool,
    hidden: bool,
) -> VisibilitySample {
    VisibilitySample {
        obs,
        lifetime_matched: Some(matched),
        visibility: VisibilityFact {
            visible: Some(visible),
            suppression_owned: Some(false),
        },
        minimized: Some(minimized),
        host_hidden: Some(hidden),
    }
}

fn bracket(window: &str, out: &str, obs: u64, frame: Option<Rect>) -> ArrivalBracket {
    ArrivalBracket {
        window: WindowId(window.to_owned()),
        obs,
        before: sample(obs, true, true, false, false),
        location: ArrivalLocation::Output {
            out: Some(OutputId(out.to_owned())),
        },
        frame,
        after: sample(obs, true, true, false, false),
    }
}

fn feedback_row(
    id: &str,
    target: &str,
    revision: u64,
    proven: bool,
    post: Option<&str>,
    arrival: Vec<ArrivalBracket>,
) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId(target.to_owned()),
        revision,
        setter: VisibilitySetter::Accepted,
        reason: crate::world_report::VisibilityReason::Empty,
        pre: None,
        post: post.map(|id| WindowId(id.to_owned())),
        foreground_proven: proven,
        arrival,
        attempt_ms: 7,
    }
}

#[test]
fn selection_grants_await_shown_with_show_and_hide_targets() {
    let (mut engine, _home, away) = select_setup();
    let reply = select(&mut engine, 3, &away);
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    let effect = &reply.effects[0];
    assert_eq!(effect.target.0, "w2");
    assert!(effect.await_shown, "selection focus awaits arrival");
    assert_eq!(effect.command_seq, Some(3));
    assert_eq!(
        effect.cause,
        crate::world_report::EffectCause::ExplicitCommand
    );

    let shown = row(&reply, "w2");
    let target = shown.visibility.expect("selected home show");
    assert!(target.shown);
    assert!(!target.fallback, "no fallback on a granted show");
    assert_eq!(target.revision, reply.state_revision);
    assert_eq!(shown.visibility_authority, ScopeAuthority::Controlled);

    let departed = row(&reply, "w1");
    let hide = departed.visibility.expect("departing home hide");
    assert!(!hide.shown);
    assert!(!hide.fallback);
    assert_eq!(hide.revision, target.revision, "same authorizing revision");
    assert_eq!(departed.visibility_authority, ScopeAuthority::Controlled);

    for row in &reply.windows {
        assert!(
            row.visibility.is_none_or(|target| !target.fallback),
            "never a fallback hide on this route"
        );
    }
    // Ordinary ledger keeps no trace of the arrival-bound grant.
    for grant in &engine.retained_focus {
        assert!(
            grant.effect.id != effect.id,
            "awaitShown grant stays out of the ordinary ledger"
        );
    }
    // Frozen snapshot frames come from the final published layout, and the
    // published rows fit the standing per-window target reserve.
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("selection binding retained");
    assert_eq!(binding.effect, Some(effect.clone()));
    assert_eq!(binding.parent_op, "op-3", "parent view op retained");
    assert_eq!(
        binding.members.len(),
        1,
        "only the shown member: {binding:?}"
    );
    assert_eq!(binding.members[0].window.0, "w2");
    assert_eq!(binding.members[0].frame, shown.effective_frame);
    for row in &reply.windows {
        assert!(
            crate::world_sizing::window_target_lanes_estimate(row)
                <= crate::world_sizing::MAX_TARGET_LANES_PER_WINDOW,
            "per-window lanes fit the 626 reserve: {row:?}"
        );
    }
    let wire = crate::world_report::encode_reply_canonical(&reply);
    assert!(
        crate::world_report::reply_estimate(&reply) >= wire.len(),
        "estimate covers the selection reply"
    );
    assert!(
        wire.contains("\"awaitShown\":true"),
        "producer spells the awaitShown grant"
    );
}

#[test]
fn selection_targets_stay_stable_across_observations() {
    let (mut engine, _home, away) = select_setup();
    let reply = select(&mut engine, 3, &away);
    let rev = reply.state_revision;
    let steady = accepted(
        &mut engine,
        &report(
            4,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert_eq!(row(&steady, "w2").visibility.expect("show").revision, rev);
    assert_eq!(row(&steady, "w1").visibility.expect("hide").revision, rev);
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "observation keeps the binding"
    );
}

#[test]
fn selection_arrival_proof_needs_every_frozen_shown_member() {
    let (mut engine, _home, away) = select_setup();
    let reply = select(&mut engine, 3, &away);
    let effect = reply.effects[0].clone();
    let frame = row(&reply, "w2").effective_frame;
    assert!(frame.is_some(), "tiled member publishes a desire frame");
    // Bracket watermark 5 against report watermark 6 is legitimate: the
    // bracket obs must match its own before/after, not the later report.
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 5, frame)],
    );
    let _ = accepted(
        &mut engine,
        &report(
            6,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![row],
            "",
        ),
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert!(binding.feedback.is_some(), "first tuple latches");
    assert!(
        binding.foreground_proven,
        "ordinary equality proof recorded"
    );
    assert!(
        binding.arrival_ready,
        "full frozen arrival proves readiness"
    );
}

#[test]
fn selection_arrival_rejects_partial_or_mismatched_evidence() {
    for (name, empty, wrong_frame, wrong_out) in [
        ("empty", true, false, false),
        ("null-frame", false, false, false),
        ("wrong-frame", false, true, false),
        ("wrong-output", false, false, true),
    ] {
        let (mut engine, _home, away) = select_setup();
        let reply = select(&mut engine, 3, &away);
        let effect = reply.effects[0].clone();
        let frame = row(&reply, "w2").effective_frame;
        let arrival = if empty {
            Vec::new()
        } else if wrong_frame {
            vec![bracket("w2", "out-a", 5, Some(rect(1, 2, 3, 4)))]
        } else if wrong_out {
            vec![bracket("w2", "out-b", 5, frame)]
        } else {
            vec![bracket("w2", "out-a", 5, None)]
        };
        let row = feedback_row(
            &effect.id,
            "w2",
            effect.issued_revision,
            true,
            Some("w2"),
            arrival,
        );
        let reply = accepted(
            &mut engine,
            &report(
                6,
                vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
                vec![],
                None,
                None,
                vec![row],
                "",
            ),
        );
        assert!(
            reply.switch_settled.is_empty(),
            "{name}: wrong raw never settles"
        );
        let binding = engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("binding");
        assert!(binding.foreground_proven, "{name}: foreground still proves");
        assert!(!binding.arrival_ready, "{name}: never proves readiness");
    }
}

#[test]
fn selection_arrival_rejects_bad_lifetime_facts() {
    let (mut engine, _home, away) = select_setup();
    let reply = select(&mut engine, 3, &away);
    let effect = reply.effects[0].clone();
    let frame = row(&reply, "w2").effective_frame;
    let mut bad = bracket("w2", "out-a", 5, frame);
    bad.after = sample(5, false, true, false, false);
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bad],
    );
    let _ = accepted(
        &mut engine,
        &report(
            6,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![row],
            "",
        ),
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert!(!binding.arrival_ready, "unmatched lifetime never proves");
}

#[test]
fn ordinary_direct_focus_grants_no_arrival() {
    let mut engine = engine();
    let boot = accepted(
        &mut engine,
        &report(
            1,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let _ = boot;
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(2),
            Some(WorldCommand::Focus {
                window: WindowId("w1".to_owned()),
                direction: "left".to_owned(),
            }),
            vec![],
            "",
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert!(!reply.effects[0].await_shown, "ordinary focus never awaits");
    for row in &reply.windows {
        assert!(row.visibility.is_none(), "ordinary focus grants no targets");
    }
    assert!(engine.retained_selection.is_empty());
}

#[allow(clippy::too_many_arguments)]
fn report_multi(
    seq: u64,
    windows: Vec<WindowFact>,
    closed: Vec<&str>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    feedback: Vec<FocusFeedback>,
    focused: &str,
    screens: Vec<crate::world_report::ScreenReport>,
) -> WorldReport {
    let mut base = report(
        seq,
        windows,
        closed,
        command_seq,
        command,
        feedback,
        focused,
    );
    base.screens = screens;
    base
}

#[test]
fn selection_accepted_setter_without_foreground_proves_arrival_only() {
    let (mut engine, _home, away) = select_setup();
    let reply = select(&mut engine, 3, &away);
    let effect = reply.effects[0].clone();
    let frame = row(&reply, "w2").effective_frame;
    // Accepted setter with no foreground equality: arrival still proves on
    // its own evidence, while the foreground verdict stays unproven. A bare
    // accepted setter never proves foreground.
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        false,
        Some("w1"),
        vec![bracket("w2", "out-a", 5, frame)],
    );
    let _ = accepted(
        &mut engine,
        &report(
            6,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![row],
            "",
        ),
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert!(binding.feedback.is_some(), "first tuple latches");
    assert!(
        !binding.foreground_proven,
        "setter alone never proves foreground"
    );
    assert!(binding.arrival_ready, "full arrival proves on its own");
}

#[test]
fn selection_arrival_rejects_watermark_violations() {
    for name in ["stale-obs", "future-obs", "sample-mismatch"] {
        let (mut engine, _home, away) = select_setup();
        let reply = select(&mut engine, 3, &away);
        let effect = reply.effects[0].clone();
        let frame = row(&reply, "w2").effective_frame;
        let arrival = match name {
            // The frozen issuing observation is 3: an older bracket never
            // proves a newer grant.
            "stale-obs" => vec![bracket("w2", "out-a", 2, frame)],
            // The feedback report watermark is 6: a future bracket never
            // proves the current attempt.
            "future-obs" => vec![bracket("w2", "out-a", 7, frame)],
            _ => {
                let mut bad = bracket("w2", "out-a", 5, frame);
                bad.before = sample(4, true, true, false, false);
                vec![bad]
            }
        };
        let row = feedback_row(
            &effect.id,
            "w2",
            effect.issued_revision,
            true,
            Some("w2"),
            arrival,
        );
        let reply = accepted(
            &mut engine,
            &report(
                6,
                vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
                vec![],
                None,
                None,
                vec![row],
                "",
            ),
        );
        assert!(
            reply.switch_settled.is_empty(),
            "{name}: wrong raw never settles"
        );
        let binding = engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("binding");
        assert!(binding.foreground_proven, "{name}: foreground still proves");
        assert!(!binding.arrival_ready, "{name}: never proves readiness");
    }
}

#[test]
fn selection_native_arm_bracket_is_refused_before_adoption() {
    let (mut engine, _home, away) = select_setup();
    let reply = select(&mut engine, 3, &away);
    let effect = reply.effects[0].clone();
    let frame = row(&reply, "w2").effective_frame;
    // A native-arm bracket on a native-free host never reaches settlement:
    // the authoritative host-arm gate refuses the report, adopting nothing.
    let native = ArrivalBracket {
        window: WindowId("w2".to_owned()),
        obs: 5,
        before: sample(5, true, true, false, false),
        location: ArrivalLocation::Native { home: None },
        frame,
        after: sample(5, true, true, false, false),
    };
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![native],
    );
    match engine.handle_report_legacy(&report(
        6,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        None,
        None,
        vec![row],
        "",
    )) {
        LegacyWorldOutcome::Rejected { kind, message } => {
            assert_eq!(message, "arrival-host-mismatch");
            let _ = kind;
        }
        other => panic!("expected host-arm refusal, got {other:?}"),
    }
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert!(binding.feedback.is_none(), "refused rows latch nothing");
    assert!(!binding.arrival_ready, "refused rows prove nothing");
}

#[test]
fn selection_close_prunes_binding_and_targets_without_stale_reuse() {
    let (mut engine, _home, away) = select_setup();
    let reply = select(&mut engine, 3, &away);
    let effect = reply.effects[0].clone();
    // Explicit close of the focus target prunes the binding and its target.
    // The surviving departing member keeps its own entry: pruning drops
    // departed identities, never the live scope.
    let _ = accepted(
        &mut engine,
        &report(
            4,
            vec![sfact("w1", "out-a", 0)],
            vec!["w2"],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "closed focus drops the binding"
    );
    assert!(
        !engine
            .selection_targets
            .contains_key(&WindowId("w2".to_owned())),
        "closed lifetime drops its target"
    );
    assert!(
        engine
            .selection_targets
            .contains_key(&WindowId("w1".to_owned())),
        "surviving member keeps its entry"
    );
    // An old-tuple row after the prune latches nothing: a closed and
    // reused lifetime can never match the pruned grant.
    let stale = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![],
    );
    let _ = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0)],
            vec!["w2"],
            None,
            None,
            vec![stale],
            "",
        ),
    );
    assert!(
        engine.retained_selection.is_empty(),
        "stale tuples revive nothing"
    );
    // Reusing the lifetime inside the surviving output scope registers a
    // fresh revision for the new admission: never the stale old revision,
    // never a resurrected grant or invented focus. The survivor keeps its
    // own revision untouched.
    let old_rev = reply.state_revision;
    let readmit = accepted(
        &mut engine,
        &report(
            6,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let fresh = row(&readmit, "w2")
        .visibility
        .expect("re-admitted show")
        .revision;
    assert_eq!(
        fresh, readmit.state_revision,
        "new admission stamps its own revision"
    );
    assert!(
        fresh > old_rev,
        "reused lifetime never inherits the stale revision"
    );
    assert_eq!(
        row(&readmit, "w1").visibility.expect("survivor").revision,
        old_rev,
        "survivor keeps its revision"
    );
    assert!(engine.retained_selection.is_empty(), "no grant resurrected");
    assert!(
        readmit.effects.iter().all(|effect| !effect.await_shown),
        "no arrival grant resurrected on re-admission"
    );
}

#[test]
fn selection_refusal_consumed_and_envelope_keep_first_feedback() {
    let (mut engine, _home, away) = select_setup();
    let reply = select(&mut engine, 3, &away);
    let effect = reply.effects[0].clone();
    let frame = row(&reply, "w2").effective_frame;
    // Latch the first exact-tuple row: bracket obs 4 sits between the
    // issuing obs 3 and the report obs 4.
    let latched = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 4, frame)],
    );
    let _ = accepted(
        &mut engine,
        &report(
            4,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![latched],
            "",
        ),
    );
    let check_latched = |engine: &WorldEngine| {
        let binding = engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("binding survives");
        assert!(binding.feedback.is_some(), "first row stays latched");
        assert!(binding.foreground_proven, "verdict stays");
        assert!(binding.arrival_ready, "readiness stays");
    };
    check_latched(&engine);
    // A refused command restores the pre-command image: the latch survives.
    let refused = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(5),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-nope".to_owned(),
            }),
            vec![],
            "",
        ),
    );
    assert!(
        matches!(
            refused.command_result,
            Some(crate::world_report::CommandResult::Rejected { .. })
        ),
        "unknown workspace refuses"
    );
    check_latched(&engine);
    // A consumed (replayed) command reconciles observation only: the latch
    // survives with no duplicate effect.
    match engine.handle_report_legacy(&report(
        6,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(3),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: away.clone(),
        }),
        vec![],
        "",
    )) {
        LegacyWorldOutcome::CommandConsumed(_) => {}
        other => panic!("expected consumed, got {other:?}"),
    }
    check_latched(&engine);
    // An envelope failure mutates nothing: the latch survives.
    let mut bad = report(
        7,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        None,
        None,
        vec![],
        "",
    );
    bad.contract_version = 0;
    match engine.handle_report_legacy(&bad) {
        LegacyWorldOutcome::Rejected { .. } => {}
        other => panic!("expected envelope refusal, got {other:?}"),
    }
    check_latched(&engine);
}

fn three_facts(output: &str) -> Vec<WindowFact> {
    vec![
        sfact("w1", output, 0),
        sfact("w2", output, 100),
        sfact("w3", output, 200),
    ]
}

fn boot_three() -> (WorldEngine, String, String) {
    let mut engine = engine();
    let boot = accepted(
        &mut engine,
        &report(1, three_facts("out-a"), vec![], None, None, vec![], ""),
    );
    let ring = ring_ids(&boot, "out-a");
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    (engine, ring[0].clone(), ring[1].clone())
}

fn send_to(
    engine: &mut WorldEngine,
    seq: u64,
    window: &str,
    workspace: &str,
) -> crate::world_report::WorldReply {
    accepted(
        engine,
        &report(
            seq,
            three_facts("out-a"),
            vec![],
            Some(seq),
            Some(WorldCommand::Send {
                window: WindowId(window.to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: workspace.to_owned(),
                follow: false,
            }),
            vec![],
            window,
        ),
    )
}

#[test]
fn selection_late_admission_registers_fresh_revision_outside_frozen_set() {
    let (mut engine, _home, away) = boot_three();
    let _ = send_to(&mut engine, 2, "w2", &away);
    let granted = accepted(
        &mut engine,
        &report(
            3,
            three_facts("out-a"),
            vec![],
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    let rev = granted.state_revision;
    assert!(row(&granted, "w2").visibility.is_some(), "grant shows w2");
    let frozen_frame = row(&granted, "w2").effective_frame;
    // A new member sent into the selected home after the grant registers
    // its own fresh revision; existing targets keep theirs and the frozen
    // set is untouched: late members never join old frozen arrivals.
    let moved = send_to(&mut engine, 4, "w3", &away);
    let fresh_rev = moved.state_revision;
    assert!(fresh_rev > rev, "fresh intent stamps its own revision");
    assert_eq!(
        row(&moved, "w3").visibility.expect("late show").revision,
        fresh_rev
    );
    assert_eq!(
        row(&moved, "w2").visibility.expect("old show").revision,
        rev,
        "existing targets keep their revision"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert_eq!(binding.members.len(), 1, "frozen set untouched");
    assert_eq!(binding.members[0].window.0, "w2");
    assert_eq!(
        binding.members[0].frame, frozen_frame,
        "frozen frame immutable"
    );
    // Arrival proves on the frozen member alone: the late member is not
    // required and cannot silently extend the old snapshot.
    let effect = granted.effects[0].clone();
    let ready = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 5, frozen_frame)],
    );
    let _ = accepted(
        &mut engine,
        &report(5, three_facts("out-a"), vec![], None, None, vec![ready], ""),
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert!(
        binding.arrival_ready,
        "frozen member alone proves readiness"
    );
}

#[test]
fn selection_multimember_snapshot_needs_every_member() {
    let (mut engine, _home, away) = boot_three();
    let _ = send_to(&mut engine, 2, "w2", &away);
    let _ = send_to(&mut engine, 3, "w3", &away);
    let granted = accepted(
        &mut engine,
        &report(
            4,
            three_facts("out-a"),
            vec![],
            Some(4),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    let effect = granted.effects[0].clone();
    assert!(
        ["w2", "w3"].contains(&effect.target.0.as_str()),
        "focus resolves a selected member: {effect:?}"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    let mut members: Vec<String> = binding
        .members
        .iter()
        .map(|member| member.window.0.clone())
        .collect();
    members.sort();
    assert_eq!(members, vec!["w2".to_owned(), "w3".to_owned()]);
    let frames: std::collections::BTreeMap<String, Option<Rect>> = granted
        .windows
        .iter()
        .map(|row| (row.lifetime.0.clone(), row.effective_frame))
        .collect();
    // One member's bracket never proves the full snapshot.
    let partial = feedback_row(
        &effect.id,
        effect.target.0.as_str(),
        effect.issued_revision,
        true,
        Some(effect.target.0.as_str()),
        vec![bracket(
            &effect.target.0,
            "out-a",
            5,
            frames[&effect.target.0],
        )],
    );
    let _ = accepted(
        &mut engine,
        &report(
            5,
            three_facts("out-a"),
            vec![],
            None,
            None,
            vec![partial],
            "",
        ),
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert!(binding.foreground_proven, "foreground still proves");
    assert!(!binding.arrival_ready, "partial arrival never proves");
    // The full snapshot proves once every frozen member brackets: fresh
    // engine, same scenario, both brackets in the first row.
    let (mut engine, _home, away) = boot_three();
    let _ = send_to(&mut engine, 2, "w2", &away);
    let _ = send_to(&mut engine, 3, "w3", &away);
    let granted = accepted(
        &mut engine,
        &report(
            4,
            three_facts("out-a"),
            vec![],
            Some(4),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    let effect = granted.effects[0].clone();
    let frames: std::collections::BTreeMap<String, Option<Rect>> = granted
        .windows
        .iter()
        .map(|row| (row.lifetime.0.clone(), row.effective_frame))
        .collect();
    let full = feedback_row(
        &effect.id,
        effect.target.0.as_str(),
        effect.issued_revision,
        true,
        Some(effect.target.0.as_str()),
        vec![
            bracket("w2", "out-a", 5, frames["w2"]),
            bracket("w3", "out-a", 5, frames["w3"]),
        ],
    );
    let _ = accepted(
        &mut engine,
        &report(5, three_facts("out-a"), vec![], None, None, vec![full], ""),
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert!(
        binding.arrival_ready,
        "every frozen member proves readiness"
    );
}

#[test]
fn selection_safety_flips_withhold_controlled_writes() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let rev = granted.state_revision;
    assert_eq!(
        row(&granted, "w2").visibility_authority,
        ScopeAuthority::Controlled
    );
    // A fullscreen flip withholds the overlay write but retains the stable
    // target; same for a capability-supported maximized flip. Facts change,
    // the authorization does not renew.
    let flipped = accepted(
        &mut engine,
        &report(
            4,
            vec![
                sfact("w1", "out-a", 0),
                sfact_fullscreen("w2", "out-a", 100),
            ],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let full_row = row(&flipped, "w2");
    assert_eq!(full_row.visibility.expect("stable target").revision, rev);
    assert_ne!(
        full_row.visibility_authority,
        ScopeAuthority::Controlled,
        "no overlay writes"
    );
    let maxed = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0), sfact_maximized("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let max_row = row(&maxed, "w2");
    assert_eq!(max_row.visibility.expect("stable target").revision, rev);
    assert_ne!(
        max_row.visibility_authority,
        ScopeAuthority::Controlled,
        "no maximized writes"
    );
    // A native-owned flip authorizes nothing: the excluded window rides
    // no row at all (never a per-window write), while the retained entry
    // and binding are preserved, not deleted.
    let owned = accepted(
        &mut engine,
        &report(
            6,
            vec![sfact("w1", "out-a", 0), sfact_owned("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        owned.windows.iter().all(|row| row.lifetime.0 != "w2"),
        "excluded windows ride no row, never a controlled write"
    );
    assert!(
        engine
            .selection_targets
            .contains_key(&WindowId("w2".to_owned())),
        "exclusion preserves the entry without granting authority"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "exclusion preserves the binding"
    );
}

#[test]
fn selection_without_suppression_caps_binds_focusless_with_direct_effect() {
    // Ineligible resolved focus (no suppression support): the legacy
    // direct focus effect stays with no await, and a focusless binding
    // freezes under the same parent with no required focus target.
    // Visibility completes through the common path with focus unproven,
    // even when the views lane contradicts the proving selection (no
    // logical echo ingress: the lane never settles, supersedes, or
    // confirms the binding).
    let mut engine = engine_with_caps(caps_no_suppression());
    let boot = accepted(
        &mut engine,
        &report(
            1,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let ring = ring_ids(&boot, "out-a");
    let (home, away) = (ring[0].clone(), ring[1].clone());
    let _ = home;
    let sent = accepted(
        &mut engine,
        &report(
            2,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w2".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            vec![],
            "w2",
        ),
    );
    assert!(sent.command_result.is_none(), "send commits: {sent:?}");
    let reply = accepted(
        &mut engine,
        &report(
            3,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    assert_eq!(reply.effects.len(), 1, "legacy direct focus stays");
    assert_eq!(reply.effects[0].target.0, "w2");
    assert!(
        !reply.effects[0].await_shown,
        "no suppression means no await"
    );
    assert!(
        reply.transfers.iter().any(|t| t.operation_id == "op-3"
            && matches!(t.kind, crate::world_report::TransferKind::View)),
        "changed select keeps its view flight: {:?}",
        reply.transfers
    );
    for row in &reply.windows {
        let target = row.visibility.as_ref().expect("targets still freeze");
        assert_ne!(
            row.visibility_authority,
            ScopeAuthority::Controlled,
            "no caps means no controlled writes"
        );
        let _ = target;
    }
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("ineligible focus freezes a focusless binding");
    assert!(binding.effect.is_none(), "no required focus target");
    assert_eq!(binding.parent_op, "op-3");
    assert_eq!(binding.home.workspace.0, away);
    assert!(
        engine
            .selection_targets
            .get(&WindowId("w2".to_owned()))
            .is_some_and(|entry| entry.shown),
        "resolved target still grants a show"
    );
    assert!(
        engine
            .selection_targets
            .get(&WindowId("w1".to_owned()))
            .is_some_and(|entry| !entry.shown),
        "departing home still grants a hide"
    );
    // First-correlated visibility verdicts complete the binding through
    // the shared terminal while the views lane names the departing home.
    let rev = reply.state_revision;
    let mut proving = report(
        4,
        vec![
            with_lw(sfact("w1", "out-a", 0), last_write_confirmed(false, rev, 4)),
            with_lw(
                sfact("w2", "out-a", 100),
                last_write_confirmed(true, rev, 4),
            ),
        ],
        vec![],
        None,
        None,
        vec![],
        "",
    );
    proving.views = vec![crate::world_report::ScreenView {
        output: OutputId("out-a".to_owned()),
        view: home.clone(),
    }];
    let done = accepted(&mut engine, &proving);
    assert_eq!(done.switch_settled.len(), 1, "one terminal: {done:?}");
    let event = &done.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(!event.fallback);
    assert_eq!(
        engine.views.get("out-a").cloned(),
        Some(away.clone()),
        "contradictory echo never adopts"
    );
    assert!(
        engine.retained_selection.is_empty(),
        "finite binding releases"
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| r.target.operation_id != "op-3"),
        "terminal retires the parent view flight"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive"
    );
    let again = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(again.switch_settled.is_empty(), "terminal never replays");
}

#[test]
fn selection_host_owned_focusless_pends_on_native_proof() {
    let mut engine = engine_with_caps(caps_host_owned());
    let boot = accepted(
        &mut engine,
        &report(
            1,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let ring = ring_ids(&boot, "out-a");
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(ring[0].clone()),
    };
    assert!(
        !engine.selection_route_eligible(&WindowId("w1".to_owned()), &home_key, &BTreeSet::new()),
        "host-owned engines never take the route"
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(2),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: ring[1].clone(),
            }),
            vec![],
            "",
        ),
    );
    assert!(reply.effects.is_empty(), "empty trailing selects nothing");
    for row in &reply.windows {
        assert!(
            row.visibility.is_none(),
            "host-owned grants no per-window targets"
        );
    }
    assert!(
        engine.selection_targets.is_empty(),
        "host-owned freezes no per-window targets"
    );
    // The focusless binding pends on the native selector readback: no
    // native observations here, so no proof either way.
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("host-owned focusless binding pends");
    assert!(binding.effect.is_none());
    assert!(
        !binding.view_targets.is_empty(),
        "native obligation freezes: {binding:?}"
    );
    assert!(
        binding.switch_members.is_empty(),
        "no per-window obligations: {binding:?}"
    );
    assert!(reply.switch_settled.is_empty(), "native proof outstanding");
}

#[test]
fn empty_selection_freezes_hide_then_supersedes_without_completing() {
    let (mut engine, home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    assert!(row(&granted, "w2").visibility.is_some());
    // Occupying the trailing spare mints a fresh trailing spare, which is
    // empty: selecting it resolves no focus and freezes the departing
    // hide under a focusless binding (latest intent supersedes the old
    // grant silently, never completing it).
    let ring = ring_ids(&granted, "out-a");
    assert!(
        ring.len() >= 3,
        "occupied spare mints a fresh spare: {ring:?}"
    );
    let empty = ring.last().expect("fresh trailing spare").clone();
    assert_ne!(empty, away, "spare is not the occupied workspace");
    let reply = select(&mut engine, 4, &empty);
    assert!(reply.effects.is_empty(), "no focus invented: {reply:?}");
    assert!(
        reply.switch_settled.is_empty(),
        "supersession completes nothing: {reply:?}"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("focusless binding retained");
    assert_eq!(binding.parent_op, "op-4", "latest intent wins: {binding:?}");
    assert!(binding.effect.is_none());
    assert!(
        binding
            .switch_members
            .iter()
            .any(|member| member.window.0 == "w2" && !member.shown),
        "departing hide freezes: {binding:?}"
    );
    // A newer selection supersedes the pending focusless binding without
    // an event: selecting home resolves w1 and drops op-4 silently.
    let back = select(&mut engine, 5, &home);
    assert_eq!(back.effects.len(), 1, "home resolves w1: {back:?}");
    assert!(
        back.switch_settled.is_empty(),
        "superseded focusless emits nothing: {back:?}"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("new binding retained");
    assert_eq!(binding.parent_op, "op-5", "latest intent wins: {binding:?}");
    assert!(
        binding.effect.is_some(),
        "focus-bearing binding replaces focusless: {binding:?}"
    );
}

#[test]
fn selection_two_outputs_retain_independent_bindings() {
    // Per-output-local mode (the default): one explicit selection per
    // output retains its own binding and revision; the second intent only
    // supersedes its own output scope.
    let mut engine = engine();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let windows = || {
        vec![
            sfact("w1", "out-a", 0),
            sfact("w2", "out-a", 100),
            sfact("w3", "out-b", 800),
            sfact("w4", "out-b", 900),
        ]
    };
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            windows(),
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let ring_a: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let ring_b: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring_a.len() >= 2, "out-a ring: {ring_a:?}");
    assert!(ring_b.len() >= 2, "out-b ring: {ring_b:?}");
    let away_a = ring_a[1].clone();
    let away_b = ring_b[1].clone();
    let _ = accepted(
        &mut engine,
        &report_multi(
            2,
            windows(),
            vec![],
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w2".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away_a.clone(),
                follow: false,
            }),
            vec![],
            "w2",
            screens.clone(),
        ),
    );
    let _ = accepted(
        &mut engine,
        &report_multi(
            3,
            windows(),
            vec![],
            Some(3),
            Some(WorldCommand::Send {
                window: WindowId("w4".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: away_b.clone(),
                follow: false,
            }),
            vec![],
            "w4",
            screens.clone(),
        ),
    );
    let first = accepted(
        &mut engine,
        &report_multi(
            4,
            windows(),
            vec![],
            Some(4),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away_a.clone(),
            }),
            vec![],
            "",
            screens.clone(),
        ),
    );
    assert_eq!(first.effects.len(), 1, "out-a selection focuses: {first:?}");
    assert_eq!(first.effects[0].target.0, "w2");
    assert!(first.effects[0].await_shown, "out-a awaits arrival");
    let rev_a = first.state_revision;
    let second = accepted(
        &mut engine,
        &report_multi(
            5,
            windows(),
            vec![],
            Some(5),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-b".to_owned()),
                workspace: away_b.clone(),
            }),
            vec![],
            "",
            screens.clone(),
        ),
    );
    assert_eq!(
        second.effects.len(),
        1,
        "out-b selection focuses: {second:?}"
    );
    assert_eq!(second.effects[0].target.0, "w4");
    assert!(second.effects[0].await_shown, "out-b awaits arrival");
    let rev_b = second.state_revision;
    assert!(rev_b > rev_a, "later intent stamps its own revision");
    // Each output keeps its own show/hide pair under its own revision.
    assert_eq!(
        row(&second, "w2").visibility.expect("out-a show").revision,
        rev_a
    );
    assert_eq!(
        row(&second, "w1").visibility.expect("out-a hide").revision,
        rev_a
    );
    assert_eq!(
        row(&second, "w4").visibility.expect("out-b show").revision,
        rev_b
    );
    assert_eq!(
        row(&second, "w3").visibility.expect("out-b hide").revision,
        rev_b
    );
    assert!(row(&second, "w2").visibility.expect("show").shown);
    assert!(!row(&second, "w1").visibility.expect("hide").shown);
    assert!(row(&second, "w4").visibility.expect("show").shown);
    assert!(!row(&second, "w3").visibility.expect("hide").shown);
    for window in ["w1", "w2", "w3", "w4"] {
        assert_eq!(
            row(&second, window).visibility_authority,
            ScopeAuthority::Controlled,
            "{window} stays controlled"
        );
        assert!(
            !row(&second, window).visibility.expect("target").fallback,
            "{window} never falls back"
        );
    }
    // Both bindings persist side by side; the ordinary ledger mints no
    // arrival-bound grants.
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "out-a binding survives the out-b intent"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-b".to_owned())),
        "out-b binding retained"
    );
    assert!(
        engine
            .retained_focus
            .iter()
            .all(|grant| !grant.effect.await_shown),
        "ordinary ledger carries no arrival grants"
    );
    // Observation-only reports keep both bindings and both revisions.
    let steady = accepted(
        &mut engine,
        &report_multi(
            6,
            windows(),
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    assert_eq!(row(&steady, "w2").visibility.expect("show").revision, rev_a);
    assert_eq!(row(&steady, "w1").visibility.expect("hide").revision, rev_a);
    assert_eq!(row(&steady, "w4").visibility.expect("show").revision, rev_b);
    assert_eq!(row(&steady, "w3").visibility.expect("hide").revision, rev_b);
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "observation keeps out-a"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-b".to_owned())),
        "observation keeps out-b"
    );
}

#[test]
fn selection_follow_send_home_invalidates_stale_hide() {
    // A follow send binds the common frozen selection binding (same as an
    // ordinary explicit selection): latest intent prunes the previously
    // granted scope, and the follow focus carries the arrival grant until
    // the destination is shown. No `{op}-follow` child exists.
    let (mut engine, home, away) = select_setup();
    // Prove the setup send's actuation inside the select report so its
    // transfer settles and the follow send below is not flight-blocked.
    let mut proving = report(
        3,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(3),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: away.clone(),
        }),
        vec![],
        "",
    );
    proving.member_attempts = vec![MemberAttempt {
        operation_id: "op-2".to_owned(),
        window: WindowId("w2".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(home.clone()),
        }),
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(away.clone()),
        }),
        attempt_ms: 7,
    }];
    let granted = accepted(&mut engine, &proving);
    assert_eq!(granted.effects.len(), 1, "one selection focus: {granted:?}");
    assert!(granted.effects[0].await_shown);
    assert_eq!(
        row(&granted, "w1").visibility_authority,
        ScopeAuthority::Controlled,
        "departing hide grants first"
    );
    let followed = accepted(
        &mut engine,
        &report(
            4,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(4),
            Some(WorldCommand::Send {
                window: WindowId("w2".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: home.clone(),
                follow: true,
            }),
            vec![],
            "w2",
        ),
    );
    assert_eq!(
        followed.effects.len(),
        1,
        "follow still focuses: {followed:?}"
    );
    assert_eq!(followed.effects[0].target.0, "w2");
    assert!(
        followed.effects[0].await_shown,
        "follow focus waits until the destination is shown"
    );
    assert!(
        engine.retained.iter().all(|r| {
            r.target.operation_id != "op-4"
                || !matches!(r.target.kind, crate::world_report::TransferKind::View)
        }),
        "no view flight for the common follow binding: {:?}",
        engine.retained
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| !r.target.operation_id.ends_with("-follow")),
        "no `{{op}}-follow` child: {:?}",
        engine.retained
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("follow binds the common frozen selection");
    assert_eq!(binding.parent_op, "op-4", "frozen post-layout binding");
    assert_eq!(
        binding.effect.as_ref().expect("follow effect").target.0,
        "w2"
    );
    assert!(
        binding
            .effect
            .as_ref()
            .is_some_and(|effect| effect.await_shown)
    );
    let rev = followed.state_revision;
    assert_eq!(binding.rev, rev, "binding stamps the issuing revision");
    for row in &followed.windows {
        if row.visibility_authority == ScopeAuthority::Controlled {
            assert_eq!(
                row.visibility.expect("bound target").revision,
                rev,
                "no stale Controlled hide survives: {row:?}"
            );
        }
    }
    assert!(
        engine
            .selection_targets
            .values()
            .all(|entry| entry.rev == rev),
        "superseded targets clear with the view move"
    );
    let wire = crate::world_report::encode_reply_canonical(&followed);
    assert!(
        crate::world_report::reply_estimate(&followed) >= wire.len(),
        "estimate covers the follow reply"
    );
    assert!(wire.contains("\"awaitShown\":true"));
}

#[test]
fn follow_send_settles_membership_before_selection_terminal() {
    // Same-output follow: the `Window` membership retires on its own proof
    // while the common frozen selection binding remains, then the binding
    // completes through the ordinary switch terminal path.
    let (mut engine, home, away) = select_setup();
    // The setup stay-send is still outstanding: its proof rides this same
    // report (settlement runs before command execution), so the follow
    // below is not flight-blocked.
    let mut follow_report = report(
        3,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(3),
        Some(WorldCommand::Send {
            window: WindowId("w2".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: home.clone(),
            follow: true,
        }),
        vec![],
        "w2",
    );
    follow_report.member_attempts = vec![MemberAttempt {
        operation_id: "op-2".to_owned(),
        window: WindowId("w2".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(home.clone()),
        }),
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(away.clone()),
        }),
        attempt_ms: 7,
    }];
    let followed = accepted(&mut engine, &follow_report);
    assert_eq!(followed.effects.len(), 1);
    assert_eq!(followed.effects.len(), 1);
    assert!(followed.effects[0].await_shown);
    let rev = followed.state_revision;
    let frame = row(&followed, "w2").effective_frame;
    assert!(
        followed.transfers.iter().any(|t| t.operation_id == "op-3"
            && matches!(t.kind, crate::world_report::TransferKind::Window(_))),
        "window flight outstanding without proof: {:?}",
        followed.transfers
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding pends without evidence"
    );
    // Membership proof alone retires the window flight; the binding holds
    // with no terminal yet.
    let mut arrival = report(
        4,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        None,
        None,
        vec![],
        "",
    );
    arrival.member_attempts = vec![MemberAttempt {
        operation_id: "op-3".to_owned(),
        window: WindowId("w2".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: home.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(away.clone()),
        }),
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(home.clone()),
        }),
        attempt_ms: 7,
    }];
    let arrived = accepted(&mut engine, &arrival);
    assert!(
        arrived.transfers.iter().all(|t| t.operation_id != "op-3"),
        "window retires independently: {:?}",
        arrived.transfers
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "selection binding remains while the window is gone"
    );
    assert!(
        arrived.switch_settled.is_empty(),
        "no terminal without visibility verdicts"
    );
    // The common terminal path completes the follow binding.
    let effect = followed.effects[0].clone();
    let w_lw = last_write_confirmed(true, rev, 5);
    let frame_w1 = row(&followed, "w1").effective_frame;
    let feedback = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![
            bracket("w1", "out-a", 5, frame_w1),
            bracket("w2", "out-a", 5, frame),
        ],
    );
    let done = accepted(
        &mut engine,
        &report(
            5,
            vec![
                with_lw(sfact("w1", "out-a", 0), w_lw),
                with_lw(sfact("w2", "out-a", 100), w_lw),
            ],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert_eq!(done.switch_settled.len(), 1, "one terminal: {done:?}");
    let event = &done.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive"
    );
    let wire = crate::world_report::encode_reply_canonical(&done);
    assert!(
        crate::world_report::reply_estimate(&done) >= wire.len(),
        "estimate covers the follow terminal reply"
    );
    assert!(wire.contains("\"outcome\":\"complete\""));
    let again = accepted(
        &mut engine,
        &report(
            6,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        again.switch_settled.is_empty(),
        "next report publishes empty"
    );
}

#[test]
fn follow_to_different_workspace_prunes_stale_select_view_flight() {
    // Producer regression for latest-follow intent: a pending ordinary
    // select flight (View to `away`) is pruned when a same-output follow
    // send moves the view to a different workspace. Host-owned so the
    // late raw can ride: a late raw for the stale view stays inert while
    // the target echo stays truthful.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let away = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    let home = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0-backed workspace")
        .id
        .clone();
    let stage = || {
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            hfact("w2", "n1", 200, false),
        ]
    };
    let mut staged = host_native_report(2, stage(), "n0", None, None);
    staged.focused_global = "w2".to_owned();
    let _ = accepted(&mut engine, &staged);
    let selected = host_select_windows(&mut engine, 3, &away, stage(), "n0", "w2");
    assert!(
        selected.transfers.iter().any(|t| t.operation_id == "op-3"
            && matches!(t.kind, crate::world_report::TransferKind::View)),
        "ordinary select retains its view flight: {:?}",
        selected.transfers
    );
    // Same-output follow of w2 back to `home`: no flight blocks it (the
    // select minted a View flight, never a Window flight).
    let mut follow_report = host_native_report(
        4,
        stage(),
        "n0",
        Some(4),
        Some(WorldCommand::Send {
            window: WindowId("w2".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: home.clone(),
            follow: true,
        }),
    );
    follow_report.focused_global = "w2".to_owned();
    let followed = accepted(&mut engine, &follow_report);
    assert_eq!(followed.effects.len(), 1, "follow grants: {followed:?}");
    assert!(followed.effects[0].await_shown);
    assert!(
        followed
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, crate::world_report::TransferKind::View))
            .all(|t| t.target_workspace != away),
        "stale select view flight pruned by the follow: {:?}",
        followed.transfers
    );
    assert!(
        followed.transfers.iter().any(|t| t.operation_id == "op-4"
            && matches!(t.kind, crate::world_report::TransferKind::Window(_))),
        "follow window flight outstanding: {:?}",
        followed.transfers
    );
    // Late raw for the stale view (its actuation readback) while the
    // target echo stays truthful: no retained flight can stage it, so
    // the new binding survives with no terminal from stale evidence.
    let mut late = host_native_report(5, stage(), "n0", None, None);
    late.focused_global = "w2".to_owned();
    late.view_attempts = vec![crate::world_report::ViewAttempt {
        operation_id: "op-3".to_owned(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(native_ref("n0")),
        post: Some(native_ref("n1")),
        baseline: Some(native_ref("n1")),
        attempt_ms: 7,
    }];
    let settled = accepted(&mut engine, &late);
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "follow binding survives the stale raw"
    );
    assert!(
        settled.switch_settled.is_empty(),
        "no terminal from stale evidence: {settled:?}"
    );
}

#[test]
fn unrelated_output_grant_cannot_shield_stale_scope() {
    // Direct control: a scope on out-a (rev 1) selects shared workspace
    // text with a stale departing echo, while an unrelated focus-bearing
    // binding on out-b grants the same text under a different rev. The
    // foreign grant must not keep the stale scope; genuine same-rev
    // shielding is covered by the follow family.
    let mut engine = engine();
    engine
        .mapping
        .insert("out-a".to_owned(), vec!["ws".to_owned(), "old".to_owned()]);
    engine
        .mapping
        .insert("out-b".to_owned(), vec!["ws".to_owned(), "old".to_owned()]);
    engine.views.insert("out-a".to_owned(), "ws".to_owned());
    engine.views.insert("out-b".to_owned(), "ws".to_owned());
    engine.staged.observation.inventory_complete = true;
    engine
        .staged
        .echoes
        .insert("out-a".to_owned(), "old".to_owned());
    engine
        .staged
        .echoes
        .insert("out-b".to_owned(), "ws".to_owned());
    engine.selection_scopes.insert(
        OutputId("out-a".to_owned()),
        crate::world_selection::SelectionScope {
            selected: vec![DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
            }],
            departing: vec![DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId("old".to_owned()),
            }],
            rev: 1,
        },
    );
    engine.retained_selection.insert(
        OutputId("out-b".to_owned()),
        crate::world_selection::RetainedSelection {
            effect: Some(OneShotEffect::explicit_focus(
                7,
                WindowId("w9".to_owned()),
                2,
            )),
            parent_op: "op-7".to_owned(),
            home: DomainKey {
                output: OutputId("out-b".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
            },
            rev: 2,
            obs: 1,
            members: Vec::new(),
            switch_members: Vec::new(),
            switch_evidence: std::collections::BTreeMap::new(),
            feedback: None,
            foreground_proven: false,
            arrival_ready: false,
            view_targets: std::collections::BTreeMap::new(),
            view_raw: std::collections::BTreeMap::new(),
            selected: vec![DomainKey {
                output: OutputId("out-b".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
            }],
            affected: vec![OutputId("out-b".to_owned())],
        },
    );
    engine.reconcile_selection_scopes(&BTreeSet::new());
    assert!(
        !engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "foreign rev cannot keep the stale scope"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-b".to_owned())),
        "unrelated binding survives"
    );
}

#[test]
fn follow_cross_output_binds_current_destination_and_settles_window_first() {
    // Cross-output follow to the neighbor's current view binds the common
    // frozen selection without minting any view flight; `Window` membership
    // settles the transfer while the binding owns visibility and focus.
    let mut engine = engine();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let windows = || {
        vec![
            sfact("w1", "out-a", 0),
            sfact("w2", "out-a", 100),
            sfact("w3", "out-b", 800),
            sfact("w4", "out-b", 900),
        ]
    };
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            windows(),
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let _target_b = boot
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-b")
        .map(|row| row.id.clone())
        .expect("out-b current view");
    // Cross-output sends travel the direction-only route: the target is
    // the neighbor's current view. (`WorldCommand::Send` is same-output
    // only at the Engine boundary.)
    let followed = accepted(
        &mut engine,
        &report_multi(
            2,
            windows(),
            vec![],
            Some(2),
            Some(WorldCommand::SendDirectional {
                window: WindowId("w1".to_owned()),
                direction: "right".to_owned(),
                follow: true,
            }),
            vec![],
            "w1",
            screens.clone(),
        ),
    );
    assert_eq!(followed.effects.len(), 1, "follow focuses: {followed:?}");
    assert_eq!(followed.effects[0].target.0, "w1");
    assert!(
        followed.effects[0].await_shown,
        "follow focus waits until the destination is shown"
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| !matches!(r.target.kind, crate::world_report::TransferKind::View)),
        "current destination mints no view flight: {:?}",
        engine.retained
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| !r.target.operation_id.ends_with("-follow")),
        "no `{{op}}-follow` child: {:?}",
        engine.retained
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-b".to_owned()))
        .expect("follow binds the common frozen selection");
    assert_eq!(binding.parent_op, "op-2");
    assert!(
        binding
            .effect
            .as_ref()
            .is_some_and(|effect| effect.await_shown)
    );
    // Observed homing at the target settles the window flight with no view
    // echo gate; the binding remains with no terminal yet.
    let arrived = accepted(
        &mut engine,
        &report_multi(
            3,
            vec![
                sfact("w2", "out-a", 100),
                sfact("w1", "out-b", 800),
                sfact("w3", "out-b", 900),
                sfact("w4", "out-b", 950),
            ],
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    assert!(
        arrived.transfers.iter().all(|t| t.operation_id != "op-2"),
        "window retires on membership: {:?}",
        arrived.transfers
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-b".to_owned())),
        "selection binding remains while the window is gone"
    );
    assert!(
        arrived.switch_settled.is_empty(),
        "no terminal without visibility verdicts"
    );
    let wire = crate::world_report::encode_reply_canonical(&arrived);
    assert!(
        crate::world_report::reply_estimate(&arrived) >= wire.len(),
        "estimate covers the cross-output follow reply"
    );
}

#[test]
fn follow_binding_superseded_then_close_drops_without_event() {
    // Latest intent wins: a second follow supersedes the first binding, and
    // closing the new focus target drops the finite grant with no terminal.
    // The first follow empties home, and trailing lifecycle reaps any
    // empty non-visible non-retained id (not just trailing spares), so the
    // superseding follow targets the surviving trailing spare with the
    // other window: no flight conflict, no reaped target.
    let (mut engine, home, away) = select_setup();
    // Settle the setup stay-send on this same report so the first follow
    // below is not flight-blocked.
    let mut first_report = report(
        3,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(3),
        Some(WorldCommand::Send {
            window: WindowId("w1".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: away.clone(),
            follow: true,
        }),
        vec![],
        "w1",
    );
    first_report.member_attempts = vec![MemberAttempt {
        operation_id: "op-2".to_owned(),
        window: WindowId("w2".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(home.clone()),
        }),
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(away.clone()),
        }),
        attempt_ms: 7,
    }];
    let first = accepted(&mut engine, &first_report);
    assert!(first.effects[0].await_shown);
    assert_eq!(
        engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("first follow binds")
            .parent_op,
        "op-3"
    );
    // The superseding follow needs no settling proof: w2 carries no
    // flight, and op-3's window stays outstanding underneath (latest
    // intent wins without completing the first grant).
    let spare = ring_ids(&first, "out-a")
        .last()
        .expect("trailing spare")
        .clone();
    assert_ne!(spare, away, "spare is a fresh id");
    let second_report = report(
        4,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(4),
        Some(WorldCommand::Send {
            window: WindowId("w2".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: spare.clone(),
            follow: true,
        }),
        vec![],
        "w2",
    );
    let second = accepted(&mut engine, &second_report);
    assert_eq!(second.effects.len(), 1, "second follow grants: {second:?}");
    assert!(second.effects[0].await_shown);
    assert_eq!(
        engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("second follow binds")
            .parent_op,
        "op-4",
        "latest intent supersedes"
    );
    let closed = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0)],
            vec!["w2"],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "close drops the finite grant"
    );
    assert!(closed.switch_settled.is_empty(), "close claims no terminal");
    assert!(
        !engine
            .selection_targets
            .contains_key(&WindowId("w2".to_owned())),
        "closed member targets drop"
    );
    assert!(
        closed.transfers.iter().any(|t| t.operation_id == "op-3"),
        "superseded window survives the close: {:?}",
        closed.transfers
    );
}

#[test]
fn native_free_follow_source_echo_survives_then_proves() {
    // Native-free follow: the actual source logical views echo before
    // member arrival must not drop the binding. Same-output follow of w1
    // home -> away; the intermediate report still echoes home (departing)
    // with no member proof, then the target echo plus the real member
    // proof retires the window, then LastWrites plus exact brackets
    // complete through the common terminal path.
    let (mut engine, home, away) = select_setup();
    let mut first_report = report(
        3,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(3),
        Some(WorldCommand::Send {
            window: WindowId("w1".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: away.clone(),
            follow: true,
        }),
        vec![],
        "w1",
    );
    first_report.member_attempts = vec![MemberAttempt {
        operation_id: "op-2".to_owned(),
        window: WindowId("w2".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(home.clone()),
        }),
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(away.clone()),
        }),
        attempt_ms: 7,
    }];
    let granted = accepted(&mut engine, &first_report);
    assert_eq!(granted.effects.len(), 1, "follow grants: {granted:?}");
    assert!(granted.effects[0].await_shown);
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("follow binds")
        .clone();
    assert_eq!(binding.parent_op, "op-3");
    assert!(
        binding.view_targets.is_empty(),
        "native-free carries no native obligation: {binding:?}"
    );
    let _spare = ring_ids(&granted, "out-a")
        .last()
        .expect("trailing spare")
        .clone();
    // Intermediate: actual source echo still names the departing home with
    // no member proof. Binding and targets survive with no event; the
    // window flight stays outstanding.
    let mut waiting_report = report(
        4,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        None,
        None,
        vec![],
        "w1",
    );
    waiting_report.views = vec![crate::world_report::ScreenView {
        output: OutputId("out-a".to_owned()),
        view: home.clone(),
    }];
    let waited = accepted(&mut engine, &waiting_report);
    assert!(
        waited.transfers.iter().any(|t| t.operation_id == "op-3"),
        "window still outstanding: {:?}",
        waited.transfers
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding survives the source echo"
    );
    assert!(
        waited.switch_settled.is_empty(),
        "no event while arrival pends: {waited:?}"
    );
    // Membership proof with the target echo: the window flight retires
    // while the binding pends with no terminal yet.
    let mut arrival_report = report(
        5,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        None,
        None,
        vec![],
        "w1",
    );
    arrival_report.views = vec![crate::world_report::ScreenView {
        output: OutputId("out-a".to_owned()),
        view: away.clone(),
    }];
    arrival_report.member_attempts = vec![MemberAttempt {
        operation_id: "op-3".to_owned(),
        window: WindowId("w1".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(home.clone()),
        }),
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(away.clone()),
        }),
        attempt_ms: 7,
    }];
    let arrived = accepted(&mut engine, &arrival_report);
    assert!(
        arrived.transfers.iter().all(|t| t.operation_id != "op-3"),
        "window retires independently: {:?}",
        arrived.transfers
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "selection binding remains while the window is gone"
    );
    assert!(
        arrived.switch_settled.is_empty(),
        "no terminal without visibility verdicts"
    );
    // Common terminal: LastWrites plus exact brackets at the destination.
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let w_lw = last_write_confirmed(true, rev, 6);
    let frames: std::collections::BTreeMap<WindowId, Option<Rect>> = granted
        .windows
        .iter()
        .map(|row| (row.lifetime.clone(), row.effective_frame))
        .collect();
    let arrival_brackets: Vec<ArrivalBracket> = binding
        .members
        .iter()
        .map(|member| {
            bracket(
                member.window.0.as_str(),
                "out-a",
                6,
                frames.get(&member.window).copied().flatten(),
            )
        })
        .collect();
    assert!(
        !arrival_brackets.is_empty(),
        "frozen members prove arrival: {binding:?}"
    );
    let feedback = feedback_row(
        &effect.id,
        "w1",
        effect.issued_revision,
        true,
        Some("w1"),
        arrival_brackets,
    );
    let mut proof_report = report(
        6,
        vec![
            with_lw(sfact("w1", "out-a", 0), w_lw),
            with_lw(sfact("w2", "out-a", 100), w_lw),
        ],
        vec![],
        None,
        None,
        vec![feedback],
        "w1",
    );
    proof_report.views = vec![crate::world_report::ScreenView {
        output: OutputId("out-a".to_owned()),
        view: away.clone(),
    }];
    let done = accepted(&mut engine, &proof_report);
    assert_eq!(done.switch_settled.len(), 1, "one terminal: {done:?}");
    let event = &done.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive"
    );
    // Controls on fresh legs: a genuine third-location move via a real
    // select command still drops, and an explicit superseding select
    // back to the departing home still supersedes (View flight to
    // departing defeats the tolerance). Bare logical echoes never move
    // the native-free view, so both legs drive through commands.
    let (mut third_engine, third_home, third_away) = select_setup();
    let mut third_first = report(
        3,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(3),
        Some(WorldCommand::Send {
            window: WindowId("w1".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: third_away.clone(),
            follow: true,
        }),
        vec![],
        "w1",
    );
    third_first.member_attempts = vec![MemberAttempt {
        operation_id: "op-2".to_owned(),
        window: WindowId("w2".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: third_away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(third_home.clone()),
        }),
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(third_away.clone()),
        }),
        attempt_ms: 7,
    }];
    let third_granted = accepted(&mut third_engine, &third_first);
    let third_spare = ring_ids(&third_granted, "out-a")
        .last()
        .expect("trailing spare")
        .clone();
    assert_ne!(third_spare, third_away);
    assert_ne!(third_spare, third_home);
    let third_moved = accepted(
        &mut third_engine,
        &report(
            4,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(4),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: third_spare,
            }),
            vec![],
            "",
        ),
    );
    assert!(
        third_engine
            .retained_selection
            .values()
            .all(|binding| binding.parent_op != "op-3"),
        "genuine third-location drops the abandoned grant"
    );
    assert!(
        third_moved
            .switch_settled
            .iter()
            .all(|event| event.operation_id != "op-3"),
        "third-location drop mints no terminal for the abandoned grant: {:?}",
        third_moved.switch_settled
    );
    let (mut sup_engine, sup_home, sup_away) = select_setup();
    let mut sup_first = report(
        3,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(3),
        Some(WorldCommand::Send {
            window: WindowId("w1".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: sup_away.clone(),
            follow: true,
        }),
        vec![],
        "w1",
    );
    sup_first.member_attempts = vec![MemberAttempt {
        operation_id: "op-2".to_owned(),
        window: WindowId("w2".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: sup_away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(sup_home.clone()),
        }),
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(sup_away.clone()),
        }),
        attempt_ms: 7,
    }];
    let sup_granted = accepted(&mut sup_engine, &sup_first);
    // Home is emptied by the follow and reaped, so supersede via the
    // surviving trailing spare (empty, focusless) like the ordinary
    // selection supersession path.
    let sup_spare = ring_ids(&sup_granted, "out-a")
        .last()
        .expect("trailing spare")
        .clone();
    let back = accepted(
        &mut sup_engine,
        &report(
            4,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            Some(4),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: sup_spare,
            }),
            vec![],
            "",
        ),
    );
    assert!(
        sup_engine
            .retained_selection
            .values()
            .all(|binding| binding.parent_op != "op-3"),
        "explicit supersession releases the abandoned grant"
    );
    assert!(
        back.switch_settled.is_empty(),
        "supersession mints no event: {back:?}"
    );
}

#[test]
fn host_owned_follow_binds_native_obligation_and_settles_window_first() {
    // Host-owned eligible follow: the binding carries the native selector
    // obligation (nonempty view_targets) with the mover frozen at the
    // retained destination; `Window` membership retires on its own proof
    // while the binding pends; native echo plus the correlated view
    // attempt and exact feedback brackets then complete through
    // SwitchOutcome::Complete.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let away = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    let mut staged = host_native_report(
        2,
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            hfact("w2", "n1", 200, false),
        ],
        "n0",
        None,
        None,
    );
    staged.focused_global = "w0".to_owned();
    let _ = accepted(&mut engine, &staged);
    let mut followed_report = host_native_report(
        3,
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            hfact("w2", "n1", 200, false),
        ],
        "n0",
        Some(3),
        Some(WorldCommand::Send {
            window: WindowId("w0".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: away.clone(),
            follow: true,
        }),
    );
    followed_report.focused_global = "w0".to_owned();
    let followed = accepted(&mut engine, &followed_report);
    assert_eq!(followed.effects.len(), 1, "follow focuses: {followed:?}");
    assert_eq!(followed.effects[0].target.0, "w0");
    assert!(
        followed.effects[0].await_shown,
        "follow focus awaits arrival"
    );
    assert!(
        followed.transfers.iter().any(|t| t.operation_id == "op-3"
            && matches!(t.kind, crate::world_report::TransferKind::Window(_))),
        "window flight outstanding: {:?}",
        followed.transfers
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("follow binds the common frozen selection")
        .clone();
    assert_eq!(binding.parent_op, "op-3");
    assert!(
        !binding.view_targets.is_empty(),
        "native selector obligation: {binding:?}"
    );
    assert_eq!(
        binding.view_targets.get(&OutputId("out-a".to_owned())),
        Some(&crate::session::DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: crate::directional::WorkspaceId(away.clone()),
        }),
        "obligation names the destination: {binding:?}"
    );
    assert!(
        binding
            .effect
            .as_ref()
            .is_some_and(|effect| effect.await_shown)
    );
    // The follow mover freezes at the retained destination with the
    // destination frame (ordinary sticky selections keep the actual
    // staged home instead).
    let mover = binding
        .members
        .iter()
        .find(|member| member.window.0 == "w0")
        .expect("mover frozen: {binding:?}");
    assert_eq!(
        mover.home.workspace.0, away,
        "follow mover frozen at destination: {binding:?}"
    );
    let destination_frame = row(&followed, "w0").effective_frame;
    assert_eq!(
        mover.frame, destination_frame,
        "frozen frame is the published destination frame"
    );
    // Intermediate report with the native echo still home and no member
    // attempt: the scope and binding survive the departing echo with no
    // event, and the window flight stays outstanding.
    let waiting = accepted(
        &mut engine,
        &host_native_report(
            4,
            vec![
                hfact("w0", "n0", 0, true),
                hfact("w1", "n1", 100, true),
                hfact("w2", "n1", 200, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    assert!(
        waiting.transfers.iter().any(|t| t.operation_id == "op-3"),
        "window still outstanding: {:?}",
        waiting.transfers
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding survives the departing echo"
    );
    assert!(
        waiting.switch_settled.is_empty(),
        "no event while arrival pends: {waiting:?}"
    );
    // Membership proof with the mover actually relocated: the window
    // flight retires while the binding pends with no terminal yet.
    let mut arrival = host_native_report(
        5,
        vec![
            hfact("w0", "n1", 0, true),
            hfact("w1", "n1", 100, true),
            hfact("w2", "n1", 200, true),
        ],
        "n1",
        None,
        None,
    );
    arrival.member_attempts = vec![MemberAttempt {
        operation_id: "op-3".to_owned(),
        window: WindowId("w0".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Native {
            output: OutputId("out-a".to_owned()),
            workspace: crate::ids::NativeWorkspaceId::parse("n0").expect("native id"),
        }),
        post: Some(WorkspaceRef::Native {
            output: OutputId("out-a".to_owned()),
            workspace: crate::ids::NativeWorkspaceId::parse("n1").expect("native id"),
        }),
        attempt_ms: 7,
    }];
    let arrived = accepted(&mut engine, &arrival);
    assert!(
        arrived.transfers.iter().all(|t| t.operation_id != "op-3"),
        "window retires independently: {:?}",
        arrived.transfers
    );
    // Post-move observed frame for the mover: the grant row is frameless
    // while the flight is outstanding (frozen frame waives equality), so
    // the proof bracket carries the first published destination frame.
    let arrived_w0_frame = row(&arrived, "w0").effective_frame;
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "selection binding remains while the window is gone"
    );
    assert!(
        arrived.switch_settled.is_empty(),
        "no terminal without visibility verdicts"
    );
    // Native echo plus the correlated view attempt and exact feedback
    // brackets (every frozen member at its destination home) complete
    // through the shared terminal path.
    let effect = followed.effects[0].clone();
    let frames = host_member_frames(&followed);
    let arrival_brackets: Vec<crate::world_report::ArrivalBracket> = binding
        .members
        .iter()
        .map(|member| {
            let frame = frames[&member.window].or(arrived_w0_frame);
            native_bracket(member.window.0.as_str(), "n1", 6, frame)
        })
        .collect();
    assert!(
        !arrival_brackets.is_empty(),
        "frozen members prove arrival: {binding:?}"
    );
    let feedback = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
        true,
        Some("w0"),
        arrival_brackets,
    );
    let mut proof = host_native_report(
        6,
        vec![
            hfact("w0", "n1", 0, true),
            hfact("w1", "n1", 100, true),
            hfact("w2", "n1", 200, true),
        ],
        "n1",
        None,
        None,
    );
    proof.focus_feedback = vec![feedback];
    proof.view_attempts = vec![crate::world_report::ViewAttempt {
        operation_id: "op-3".to_owned(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(native_ref("n0")),
        post: Some(native_ref("n1")),
        baseline: Some(native_ref("n1")),
        attempt_ms: 7,
    }];
    let done = accepted(&mut engine, &proof);
    assert_eq!(done.switch_settled.len(), 1, "one terminal: {done:?}");
    let event = &done.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert_eq!(
        row(&done, "w0").effective_frame,
        arrived_w0_frame,
        "final published destination frame matches the frame that proved arrival"
    );
}

#[test]
fn host_owned_follow_source_home_bracket_never_proves_arrival() {
    // Eligible negative: a source-home bracket against the destination
    // frozen home never proves arrival. The first wrong row latches and a
    // later corrected row never upgrades it: both reports stay
    // nonterminal with the binding pending.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let away = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    let mut staged = host_native_report(
        2,
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            hfact("w2", "n1", 200, false),
        ],
        "n0",
        None,
        None,
    );
    staged.focused_global = "w0".to_owned();
    let _ = accepted(&mut engine, &staged);
    let mut followed_report = host_native_report(
        3,
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            hfact("w2", "n1", 200, false),
        ],
        "n0",
        Some(3),
        Some(WorldCommand::Send {
            window: WindowId("w0".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: away.clone(),
            follow: true,
        }),
    );
    followed_report.focused_global = "w0".to_owned();
    let followed = accepted(&mut engine, &followed_report);
    assert_eq!(followed.effects.len(), 1, "follow focuses: {followed:?}");
    let effect = followed.effects[0].clone();
    let frames = host_member_frames(&followed);
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("follow binds")
        .clone();
    assert_eq!(
        binding
            .members
            .iter()
            .find(|member| member.window.0 == "w0")
            .expect("mover frozen")
            .home
            .workspace
            .0,
        away,
        "mover frozen at destination: {binding:?}"
    );
    // Wrong first row: the mover bracket names the source home while the
    // frozen home is the destination. Foreground still proves, arrival
    // never does, and the native obligation pends: no terminal.
    let wrong_brackets: Vec<crate::world_report::ArrivalBracket> = binding
        .members
        .iter()
        .map(|member| {
            let native = if member.window.0 == "w0" { "n0" } else { "n1" };
            native_bracket(member.window.0.as_str(), native, 4, frames[&member.window])
        })
        .collect();
    let mut wrong = host_native_report(
        4,
        vec![
            hfact("w0", "n1", 0, true),
            hfact("w1", "n1", 100, true),
            hfact("w2", "n1", 200, true),
        ],
        "n0",
        None,
        None,
    );
    wrong.focus_feedback = vec![feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
        true,
        Some("w0"),
        wrong_brackets,
    )];
    let held = accepted(&mut engine, &wrong);
    assert!(
        held.switch_settled.is_empty(),
        "source-home bracket never settles: {held:?}"
    );
    let latched = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding pends");
    assert!(latched.foreground_proven, "foreground still proves");
    assert!(!latched.arrival_ready, "arrival never proves");
    // A later corrected row never upgrades the first latch: still no
    // terminal with the binding pending.
    let right_brackets: Vec<crate::world_report::ArrivalBracket> = binding
        .members
        .iter()
        .map(|member| native_bracket(member.window.0.as_str(), "n1", 5, frames[&member.window]))
        .collect();
    let mut corrected = host_native_report(
        5,
        vec![
            hfact("w0", "n1", 0, true),
            hfact("w1", "n1", 100, true),
            hfact("w2", "n1", 200, true),
        ],
        "n0",
        None,
        None,
    );
    corrected.focus_feedback = vec![feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
        true,
        Some("w0"),
        right_brackets,
    )];
    let still_held = accepted(&mut engine, &corrected);
    assert!(
        still_held.switch_settled.is_empty(),
        "corrected row never upgrades the latch: {still_held:?}"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding still pends"
    );
}

#[test]
fn staged_native_last_views_follow_staged_echo_across_intakes() {
    // Host-owned last-view retention reads staged native echoes only: the
    // source report lanes never decide, and unknown proof removes.
    use crate::world_report::{NativeObservation, NativeOccupancy, ScreenView};
    let (mut engine, away) = host_boot();
    let logical_for = |engine: &WorldEngine, native: &str| -> String {
        engine
            .native_map
            .iter()
            .find(|((out, nid), _)| out.as_str() == "out-a" && nid.as_str() == native)
            .map(|(_, logical)| logical.clone())
            .expect("native binding")
    };
    let first = logical_for(&engine, "n0");
    assert_eq!(
        engine.last_views.get(&OutputId("out-a".to_owned())),
        Some(&first),
        "boot retains the staged echo"
    );
    let _ = accepted(
        &mut engine,
        &host_native_report(2, vec![], "n1", None, None),
    );
    let second = logical_for(&engine, "n1");
    assert_ne!(first, second);
    assert_eq!(
        engine.last_views.get(&OutputId("out-a".to_owned())),
        Some(&second),
        "a later source report retires through staged state"
    );
    assert_eq!(away, second, "n1-backed workspace is the away id");
    // Unknown native proof removes even when the logical views lane names a
    // workspace: logical echoes never decide under host-owned domains.
    let mut unknown = host_native_report(3, vec![], "n1", None, None);
    unknown.native = vec![NativeObservation {
        output: OutputId("out-a".to_owned()),
        native_order: vec!["n0".to_owned(), "n1".to_owned()],
        current_native: None,
        occupancy: vec![
            NativeOccupancy {
                native: "n0".to_owned(),
                count: None,
            },
            NativeOccupancy {
                native: "n1".to_owned(),
                count: None,
            },
        ],
    }];
    unknown.views = vec![ScreenView {
        output: OutputId("out-a".to_owned()),
        view: second.clone(),
    }];
    let _ = accepted(&mut engine, &unknown);
    assert!(
        !engine
            .last_views
            .contains_key(&OutputId("out-a".to_owned())),
        "unknown proof removes without logical fallback"
    );
}

fn shared_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        caps(),
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )],
    )
    .expect("generation")
}

#[test]
fn selection_shared_mode_fans_out_scopes_and_survives_member_close() {
    // Shared mode: one focus-bearing selection registers an active scope
    // on every affected output. Later members on any scoped output (even
    // the non-focus one) register their own fresh revision, and closing
    // a frozen member drops the finite grant without dropping the
    // surviving output policy.
    let mut engine = shared_engine();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let windows = || {
        vec![
            sfact("w1", "out-a", 0),
            sfact("w2", "out-a", 100),
            sfact("w3", "out-b", 800),
            sfact("w4", "out-b", 900),
        ]
    };
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            windows(),
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let ring_a: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring_a.len() >= 2, "shared ring: {ring_a:?}");
    let home = ring_a[0].clone();
    let away = ring_a[1].clone();
    let _ = accepted(
        &mut engine,
        &report_multi(
            2,
            windows(),
            vec![],
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w2".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            vec![],
            "w2",
            screens.clone(),
        ),
    );
    let granted = accepted(
        &mut engine,
        &report_multi(
            3,
            windows(),
            vec![],
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
            screens.clone(),
        ),
    );
    assert_eq!(granted.effects.len(), 1, "one selection focus: {granted:?}");
    assert!(granted.effects[0].await_shown);
    let rev = granted.state_revision;
    // Both affected outputs carry an active scope under the same revision.
    for output in ["out-a", "out-b"] {
        let scope = engine
            .selection_scopes
            .get(&OutputId(output.to_owned()))
            .unwrap_or_else(|| panic!("shared scope on {output}"));
        assert_eq!(scope.rev, rev, "{output} scope stamps the issuing rev");
    }
    assert_eq!(row(&granted, "w2").visibility.expect("show").revision, rev);
    assert_eq!(row(&granted, "w1").visibility.expect("hide").revision, rev);
    assert_eq!(row(&granted, "w3").visibility.expect("hide").revision, rev);
    assert_eq!(row(&granted, "w4").visibility.expect("hide").revision, rev);
    // A later member on the NON-focus output registers its own fresh
    // revision instead of joining the old frozen set.
    let moved = accepted(
        &mut engine,
        &report_multi(
            4,
            windows(),
            vec![],
            Some(4),
            Some(WorldCommand::Send {
                window: WindowId("w4".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            vec![],
            "w4",
            screens.clone(),
        ),
    );
    let fresh = moved.state_revision;
    assert!(fresh > rev, "new admission stamps its own revision");
    assert_eq!(
        row(&moved, "w4").visibility.expect("late show").revision,
        fresh
    );
    assert_eq!(
        row(&moved, "w3").visibility.expect("survivor").revision,
        rev,
        "survivor keeps its revision"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("finite grant");
    assert!(
        binding.members.iter().all(|member| member.window.0 != "w4"),
        "late member never joins the frozen set"
    );
    // Closing the frozen focus member drops the finite grant but keeps
    // the surviving output policy and per-window entries.
    let _ = accepted(
        &mut engine,
        &report_multi(
            5,
            vec![
                sfact("w1", "out-a", 0),
                sfact("w3", "out-b", 800),
                sfact("w4", "out-b", 900),
            ],
            vec!["w2"],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "closed member drops the finite grant"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "surviving output policy stays"
    );
    assert_eq!(
        engine
            .selection_targets
            .get(&WindowId("w1".to_owned()))
            .expect("survivor entry")
            .rev,
        rev,
        "survivor target keeps its revision"
    );
    // A new in-scope admission on the still-live output gets a fresh
    // revision while the other output's survivor stays stable, with no
    // resurrected grant.
    let _ = home;
    let readmit = accepted(
        &mut engine,
        &report_multi(
            6,
            vec![
                sfact("w1", "out-a", 0),
                sfact("w3", "out-b", 800),
                sfact("w4", "out-b", 900),
            ],
            vec![],
            Some(6),
            Some(WorldCommand::Send {
                window: WindowId("w1".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            vec![],
            "w1",
            screens.clone(),
        ),
    );
    let fresh2 = readmit.state_revision;
    assert!(fresh2 > fresh, "second admission stamps its own revision");
    assert_eq!(
        row(&readmit, "w1").visibility.expect("new show").revision,
        fresh2
    );
    assert_eq!(
        row(&readmit, "w4")
            .visibility
            .expect("other-output survivor")
            .revision,
        fresh,
        "other output survivor stays stable"
    );
    assert!(engine.retained_selection.is_empty(), "no grant resurrected");
    assert!(
        readmit.effects.iter().all(|effect| !effect.await_shown),
        "no arrival grant resurrected"
    );
}

fn held_screen_row(output: &str, ordinal: usize) -> HeldScreenRow {
    HeldScreenRow {
        output: OutputId(output.to_owned()),
        ordinal,
        fields: vec![
            ("out".to_owned(), RawSettingValue::String(output.to_owned())),
            (
                "scale".to_owned(),
                RawSettingValue::String("bad".to_owned()),
            ),
        ],
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(OutputId(output.to_owned())),
            row: RowDiagnosticLane::Screen,
            id: Some(output.to_owned()),
            kind: RowDiagnosticKind::Unreadable,
        },
    }
}

#[test]
fn selection_held_screen_preserves_grant_and_recovery_rehomes_with_fresh_target() {
    // A held (narrowly unreadable) screen is still a known output, never
    // true output loss: active scopes, per-window targets, and the finite
    // grant survive with stable revisions under held authority. Recovery
    // re-admits the departed member at the selected view through the
    // pre-existing logical-homing path (not a selection grant): `w3` keeps
    // its lifetime but changes home with fresh authorization while every
    // other target, scope, and the frozen snapshot keeps its revision.
    let mut engine = engine();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let windows = || {
        vec![
            sfact("w1", "out-a", 0),
            sfact("w2", "out-a", 100),
            sfact("w3", "out-b", 800),
            sfact("w4", "out-b", 900),
        ]
    };
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            windows(),
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let ring = |output: &str| {
        boot.workspaces
            .iter()
            .filter(|row| row.output.0 == output)
            .map(|row| row.id.clone())
            .collect::<Vec<_>>()
    };
    let (home_b, away_a, away_b) = (
        ring("out-b")[0].clone(),
        ring("out-a")[1].clone(),
        ring("out-b")[1].clone(),
    );
    for (seq, window, output, workspace) in [
        (2, "w2", "out-a", away_a.clone()),
        (3, "w4", "out-b", away_b.clone()),
    ] {
        let _ = accepted(
            &mut engine,
            &report_multi(
                seq,
                windows(),
                vec![],
                Some(seq),
                Some(WorldCommand::Send {
                    window: WindowId(window.to_owned()),
                    target_output: OutputId(output.to_owned()),
                    target_workspace: workspace,
                    follow: false,
                }),
                vec![],
                window,
                screens.clone(),
            ),
        );
    }
    let first = accepted(
        &mut engine,
        &report_multi(
            4,
            windows(),
            vec![],
            Some(4),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away_a.clone(),
            }),
            vec![],
            "",
            screens.clone(),
        ),
    );
    let second = accepted(
        &mut engine,
        &report_multi(
            5,
            windows(),
            vec![],
            Some(5),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-b".to_owned()),
                workspace: away_b.clone(),
            }),
            vec![],
            "",
            screens.clone(),
        ),
    );
    let (rev_a, rev_b) = (first.state_revision, second.state_revision);
    let frozen_w4_frame = row(&second, "w4").effective_frame;
    assert!(frozen_w4_frame.is_some(), "frozen w4 publishes a frame");
    let grant_effect = second.effects[0].clone();
    // Hold out-b's screen while its facts keep flowing.
    let mut held = report_multi(
        6,
        windows(),
        vec![],
        None,
        None,
        vec![],
        "",
        vec![screen("out-a", 0)],
    );
    held.held_screens = vec![held_screen_row("out-b", 1)];
    let narrow = accepted(&mut engine, &held);
    assert_eq!(
        engine
            .selection_scopes
            .get(&OutputId("out-b".to_owned()))
            .expect("held scope survives")
            .rev,
        rev_b,
        "held screen keeps the scope revision"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "healthy scope stays"
    );
    for (window, rev) in [("w1", rev_a), ("w2", rev_a), ("w3", rev_b), ("w4", rev_b)] {
        assert_eq!(
            engine
                .selection_targets
                .get(&WindowId(window.to_owned()))
                .expect("held target survives")
                .rev,
            rev,
            "{window} target keeps its revision under hold"
        );
    }
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-b".to_owned())),
        "held screen keeps the finite grant"
    );
    for window in ["w3", "w4"] {
        if narrow
            .windows
            .iter()
            .any(|desired| desired.lifetime.0 == window)
        {
            assert_eq!(
                row(&narrow, window).visibility.expect("target").revision,
                rev_b,
                "{window} revision stable under hold"
            );
            assert_ne!(
                row(&narrow, window).visibility_authority,
                ScopeAuthority::Controlled,
                "{window} held, never controlled"
            );
        }
    }
    for window in ["w1", "w2"] {
        assert_eq!(
            row(&narrow, window).visibility_authority,
            ScopeAuthority::Controlled,
            "{window} healthy output stays controlled"
        );
    }
    // The departed member sits on its departing home under hold.
    let held_w3 = engine
        .selection_targets
        .get(&WindowId("w3".to_owned()))
        .expect("held w3 target survives")
        .clone();
    assert_eq!(
        held_w3.home,
        DomainKey {
            output: OutputId("out-b".to_owned()),
            workspace: WorkspaceId(home_b.clone()),
        },
        "held w3 stays on its departing home"
    );
    assert!(!held_w3.shown, "held w3 stays a departing hide");
    assert_eq!(held_w3.rev, rev_b, "held w3 keeps its revision");
    let held_binding = engine
        .retained_selection
        .get(&OutputId("out-b".to_owned()))
        .expect("held grant survives")
        .clone();
    assert_eq!(
        held_binding.effect,
        Some(grant_effect.clone()),
        "held grant is the out-b award"
    );
    assert_eq!(held_binding.members.len(), 1, "frozen set is w4 alone");
    assert_eq!(held_binding.members[0].window.0, "w4");
    assert_eq!(
        held_binding.members[0].frame, frozen_w4_frame,
        "frozen frame is the published layout frame"
    );
    // Recovery re-admits `w3` at the selected view through the pre-existing
    // logical-homing path: same lifetime, genuinely changed home and polarity
    // with fresh authorization. Every other target, scope, and the frozen
    // snapshot keeps its revision; the frozen frame is never silently
    // updated to the new half-width layout.
    let recovered = accepted(
        &mut engine,
        &report_multi(
            7,
            windows(),
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let recovered_rev = recovered.state_revision;
    assert!(
        recovered_rev > rev_b,
        "re-admission stamps its own revision"
    );
    let fresh_w3 = engine
        .selection_targets
        .get(&WindowId("w3".to_owned()))
        .expect("re-admitted w3 registers")
        .clone();
    assert_eq!(
        fresh_w3.home,
        DomainKey {
            output: OutputId("out-b".to_owned()),
            workspace: WorkspaceId(away_b.clone()),
        },
        "re-admitted w3 homes to the selected workspace"
    );
    assert_ne!(
        fresh_w3.home, held_w3.home,
        "home genuinely changes ws -> selected"
    );
    assert!(
        !held_w3.shown && fresh_w3.shown,
        "polarity flips hide -> show"
    );
    assert_eq!(
        fresh_w3.rev, recovered_rev,
        "re-admitted w3 stamps the recovery revision"
    );
    let w3_row = row(&recovered, "w3");
    let w3_target = w3_row.visibility.expect("re-admitted show");
    assert!(w3_target.shown, "re-admitted w3 desires shown");
    assert!(!w3_target.fallback, "no fallback on the fresh show");
    assert_eq!(
        w3_target.revision, recovered_rev,
        "fresh show carries fresh rev"
    );
    assert_eq!(
        w3_row.visibility_authority,
        ScopeAuthority::Controlled,
        "fresh show is controlled"
    );
    for (window, rev, shown) in [
        ("w1", rev_a, false),
        ("w2", rev_a, true),
        ("w4", rev_b, true),
    ] {
        let desired = row(&recovered, window);
        let target = desired.visibility.expect("unaffected target survives");
        assert_eq!(target.revision, rev, "{window} keeps its revision");
        assert_eq!(target.shown, shown, "{window} keeps its polarity");
        assert_eq!(
            desired.visibility_authority,
            ScopeAuthority::Controlled,
            "{window} stays controlled"
        );
        assert_eq!(
            engine
                .selection_targets
                .get(&WindowId(window.to_owned()))
                .expect("unaffected entry survives")
                .rev,
            rev,
            "{window} entry keeps its revision"
        );
    }
    for (output, rev) in [("out-a", rev_a), ("out-b", rev_b)] {
        assert_eq!(
            engine
                .selection_scopes
                .get(&OutputId(output.to_owned()))
                .expect("scope survives recovery")
                .rev,
            rev,
            "{output} scope keeps its revision"
        );
    }
    let binding = engine
        .retained_selection
        .get(&OutputId("out-b".to_owned()))
        .expect("grant survives recovery");
    assert_eq!(
        binding.effect,
        Some(grant_effect.clone()),
        "same finite grant, no renewal"
    );
    assert_eq!(binding.rev, rev_b, "grant keeps its revision");
    assert_eq!(binding.members.len(), 1, "frozen set still w4 alone");
    assert_eq!(binding.members[0].window.0, "w4");
    assert_eq!(
        binding.members[0].frame, frozen_w4_frame,
        "frozen frame immutable across recovery"
    );
    assert_ne!(
        row(&recovered, "w4").effective_frame,
        frozen_w4_frame,
        "new half-width layout must not silently rewrite the frozen full-width frame"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-b".to_owned())),
        "grant survives recovery"
    );
}

#[test]
fn selection_composed_sticky_members_join_frozen_arrival_with_precedence() {
    // F5b: every Controlled desired-shown member on the focus output joins
    // the frozen arrival snapshot, including composed sticky. The sticky
    // producer keeps precedence (no selection target for the sticky
    // window); a w2-only bracket never proves the two-member snapshot while
    // the full bracket set proves it on a separate engine with the first
    // row latched.
    fn setup() -> (WorldEngine, String, crate::world_report::WorldReply) {
        let (mut engine, _home, away) = boot_three();
        let _ = send_to(&mut engine, 2, "w2", &away);
        let sticky = accepted(
            &mut engine,
            &report(
                3,
                three_facts("out-a"),
                vec![],
                Some(3),
                Some(WorldCommand::SetSticky {
                    window: WindowId("w3".to_owned()),
                    value: true,
                }),
                vec![],
                "",
            ),
        );
        assert!(
            sticky.command_result.is_none(),
            "sticky commits: {sticky:?}"
        );
        assert!(
            engine
                .restored_sticky
                .contains_key(&WindowId("w3".to_owned())),
            "w3 is composed sticky"
        );
        let granted = accepted(
            &mut engine,
            &report(
                4,
                three_facts("out-a"),
                vec![],
                Some(4),
                Some(WorldCommand::SelectWorkspace {
                    output: OutputId("out-a".to_owned()),
                    workspace: away.clone(),
                }),
                vec![],
                "",
            ),
        );
        assert_eq!(granted.effects.len(), 1, "one selection focus: {granted:?}");
        assert_eq!(granted.effects[0].target.0, "w2");
        assert!(granted.effects[0].await_shown, "selection awaits arrival");
        (engine, away, granted)
    }
    // Partial evidence on the first engine: w2 alone never proves the
    // two-member snapshot, while the foreground verdict still proves.
    let (mut engine, _away, granted) = setup();
    let effect = granted.effects[0].clone();
    let frames: std::collections::BTreeMap<String, Option<Rect>> = granted
        .windows
        .iter()
        .map(|desired| (desired.lifetime.0.clone(), desired.effective_frame))
        .collect();
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding")
        .clone();
    let mut members: Vec<String> = binding
        .members
        .iter()
        .map(|member| member.window.0.clone())
        .collect();
    members.sort();
    assert_eq!(members, vec!["w2".to_owned(), "w3".to_owned()]);
    // The floated sticky member carries no desire frame: null waives frame
    // equality only, never the need for a readable live frame in proof.
    assert_eq!(
        binding
            .members
            .iter()
            .find(|member| member.window.0 == "w3")
            .expect("w3 frozen")
            .frame,
        None
    );
    assert!(
        !engine
            .selection_targets
            .contains_key(&WindowId("w3".to_owned())),
        "sticky keeps producer precedence, no selection target"
    );
    assert_eq!(
        engine
            .selection_targets
            .get(&WindowId("w2".to_owned()))
            .expect("w2 target")
            .rev,
        granted.state_revision
    );
    let w3_row = row(&granted, "w3");
    let w3_target = w3_row.visibility.expect("sticky show");
    assert!(w3_target.shown);
    assert!(!w3_target.fallback);
    assert_eq!(w3_row.visibility_authority, ScopeAuthority::Controlled);
    assert_ne!(
        w3_target.revision, granted.state_revision,
        "sticky keeps its own revision, not the selection rev"
    );
    let partial = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 5, frames["w2"])],
    );
    let _ = accepted(
        &mut engine,
        &report(
            5,
            three_facts("out-a"),
            vec![],
            None,
            None,
            vec![partial],
            "",
        ),
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert!(binding.feedback.is_some(), "first tuple latches");
    assert!(binding.foreground_proven, "foreground still proves");
    assert!(!binding.arrival_ready, "w2 alone never proves w2+w3");
    // Full evidence on a separate engine: both brackets prove readiness
    // with the first row latched.
    let (mut engine, _away, granted) = setup();
    let effect = granted.effects[0].clone();
    let frames: std::collections::BTreeMap<String, Option<Rect>> = granted
        .windows
        .iter()
        .map(|desired| (desired.lifetime.0.clone(), desired.effective_frame))
        .collect();
    let full = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![
            bracket("w2", "out-a", 5, frames["w2"]),
            bracket("w3", "out-a", 5, Some(rect(200, 0, 40, 90))),
        ],
    );
    let _ = accepted(
        &mut engine,
        &report(5, three_facts("out-a"), vec![], None, None, vec![full], ""),
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    assert!(binding.feedback.is_some(), "first tuple latches");
    assert!(binding.foreground_proven, "foreground proves");
    assert!(
        binding.arrival_ready,
        "every frozen member proves readiness"
    );
}

#[test]
fn selection_historical_readiness_does_not_authorize_fresh_visibility() {
    // F5a: readiness is historical, authority is fresh. A latched
    // valid arrival plus foreground proof stays latched, but later
    // fullscreen/maximize flips withhold Controlled writes without
    // renewing the revision, minting a new grant, or retrying.
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let rev = granted.state_revision;
    let effect = granted.effects[0].clone();
    let frame = row(&granted, "w2").effective_frame;
    assert!(frame.is_some(), "tiled member publishes a frame");
    let latched = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 4, frame)],
    );
    let _ = accepted(
        &mut engine,
        &report(
            4,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![latched],
            "",
        ),
    );
    let check_latched = |engine: &WorldEngine| {
        let binding = engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("binding survives");
        assert!(binding.feedback.is_some(), "first row stays latched");
        assert!(binding.foreground_proven, "foreground verdict stays");
        assert!(binding.arrival_ready, "readiness stays");
    };
    check_latched(&engine);
    // Fresh fullscreen facts gate authority to Hold despite the historical
    // proof: same revision, no new grant, latch untouched.
    let flipped = accepted(
        &mut engine,
        &report(
            5,
            vec![
                sfact("w1", "out-a", 0),
                sfact_fullscreen("w2", "out-a", 100),
            ],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert_eq!(
        row(&flipped, "w2")
            .visibility
            .expect("stable target")
            .revision,
        rev,
        "fresh facts never renew the authorization"
    );
    assert_ne!(
        row(&flipped, "w2").visibility_authority,
        ScopeAuthority::Controlled,
        "fullscreen withholds fresh writes"
    );
    assert!(
        flipped.effects.iter().all(|effect| !effect.await_shown),
        "no new arrival grant follows the flip"
    );
    check_latched(&engine);
    // Capability-gated maximize flips the same way: Hold with the same
    // revision, latch stable, no new grant.
    let maxed = accepted(
        &mut engine,
        &report(
            6,
            vec![sfact("w1", "out-a", 0), sfact_maximized("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert_eq!(
        row(&maxed, "w2")
            .visibility
            .expect("stable target")
            .revision,
        rev,
        "fresh facts never renew the authorization"
    );
    assert_ne!(
        row(&maxed, "w2").visibility_authority,
        ScopeAuthority::Controlled,
        "maximize withholds fresh writes"
    );
    assert!(
        maxed.effects.iter().all(|effect| !effect.await_shown),
        "no new arrival grant follows the flip"
    );
    check_latched(&engine);
}

fn confirm_sample(obs: u64, shown: bool) -> VisibilitySample {
    VisibilitySample {
        obs,
        lifetime_matched: Some(true),
        visibility: VisibilityFact {
            visible: Some(shown),
            suppression_owned: Some(false),
        },
        minimized: Some(false),
        host_hidden: Some(false),
    }
}

fn last_write_confirmed(target: bool, rev: u64, post_obs: u64) -> LastWrite {
    LastWrite {
        property: LastWriteProperty::Visibility,
        target,
        revision: rev,
        path: LastWritePath::Staged,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(confirm_sample(post_obs, target)),
    }
}

fn last_write_with(
    target: bool,
    rev: u64,
    setter: VisibilitySetter,
    reason: VisibilityReason,
    post: Option<VisibilitySample>,
) -> LastWrite {
    LastWrite {
        property: LastWriteProperty::Visibility,
        target,
        revision: rev,
        path: LastWritePath::Staged,
        setter,
        reason,
        pre: None,
        post,
    }
}

fn with_lw(mut fact: WindowFact, last: LastWrite) -> WindowFact {
    fact.last_write = Some(last);
    fact
}

fn refused_feedback(id: &str, target: &str, revision: u64) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId(target.to_owned()),
        revision,
        setter: VisibilitySetter::Refused,
        reason: VisibilityReason::Empty,
        pre: Some(WindowId("w-other".to_owned())),
        post: Some(WindowId("w-other".to_owned())),
        foreground_proven: false,
        arrival: Vec::new(),
        attempt_ms: 7,
    }
}

fn notcalled_feedback(id: &str, target: &str, revision: u64) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId(target.to_owned()),
        revision,
        setter: VisibilitySetter::NotCalled,
        reason: VisibilityReason::ScopeHeld,
        pre: None,
        post: None,
        foreground_proven: false,
        arrival: Vec::new(),
        attempt_ms: 7,
    }
}

#[test]
fn switch_all_confirmed_completes_with_proven_focus() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    assert!(frame.is_some());
    let w2_lw = last_write_confirmed(true, rev, 4);
    let w1_lw = last_write_confirmed(false, rev, 4);
    let feedback = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 4, frame)],
    );
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_lw),
                with_lw(sfact("w2", "out-a", 100), w2_lw),
            ],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert_eq!(reply.switch_settled.len(), 1, "one terminal: {reply:?}");
    let event = &reply.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| r.target.operation_id != "op-3"),
        "actual view parent retires"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive"
    );
    assert_eq!(
        engine
            .selection_targets
            .get(&WindowId("w2".to_owned()))
            .expect("target survives")
            .rev,
        rev
    );
    let wire = crate::world_report::encode_reply_canonical(&reply);
    assert!(
        crate::world_report::reply_estimate(&reply) >= wire.len(),
        "estimate covers event reply"
    );
    assert!(wire.contains("\"switchSettled\":[{"));
    assert!(wire.contains("\"outcome\":\"complete\""));
    let again = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        again.switch_settled.is_empty(),
        "next report publishes empty"
    );
}

#[test]
fn switch_foreground_without_arrival_never_proves_focus_or_complete() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let w2_lw = last_write_confirmed(true, rev, 4);
    let w1_lw = last_write_confirmed(false, rev, 4);
    let feedback = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![],
    );
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_lw),
                with_lw(sfact("w2", "out-a", 100), w2_lw),
            ],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert_eq!(reply.switch_settled.len(), 1, "still terminal: {reply:?}");
    let event = &reply.switch_settled[0];
    assert_eq!(
        event.focus,
        SwitchFocus::Unproven,
        "no arrival means no proven focus"
    );
    assert_eq!(
        event.outcome,
        SwitchOutcome::CompletedWithFailures,
        "refused/unproven focus with confirmed targets never completes silently"
    );
    assert!(event.failed.is_empty());
}

#[test]
fn switch_refused_focus_with_confirmed_visibility_fails_outcome() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let w2_lw = last_write_confirmed(true, rev, 4);
    let w1_lw = last_write_confirmed(false, rev, 4);
    let feedback = refused_feedback(&effect.id, "w2", effect.issued_revision);
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_lw),
                with_lw(sfact("w2", "out-a", 100), w2_lw),
            ],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert_eq!(reply.switch_settled.len(), 1);
    let event = &reply.switch_settled[0];
    assert_eq!(event.focus, SwitchFocus::Refused);
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
}

#[test]
fn switch_hide_refused_terminates_with_failed_without_readiness() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let w2_lw = last_write_confirmed(true, rev, 4);
    let w1_lw = last_write_with(
        false,
        rev,
        VisibilitySetter::Refused,
        VisibilityReason::ShowBlocked,
        Some(confirm_sample(4, true)),
    );
    let feedback = notcalled_feedback(&effect.id, "w2", effect.issued_revision);
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_lw),
                with_lw(sfact("w2", "out-a", 100), w2_lw),
            ],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "blocked hide + no-call focus terminates"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.failed, vec![WindowId("w1".to_owned())]);
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive failure"
    );
}

#[test]
fn switch_blocked_show_and_nocall_focus_terminates() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let w2_lw = last_write_with(
        true,
        rev,
        VisibilitySetter::NotCalled,
        VisibilityReason::ScopeHeld,
        Some(confirm_sample(4, false)),
    );
    let w1_lw = last_write_confirmed(false, rev, 4);
    let feedback = notcalled_feedback(&effect.id, "w2", effect.issued_revision);
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_lw),
                with_lw(sfact("w2", "out-a", 100), w2_lw),
            ],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert_eq!(reply.switch_settled.len(), 1);
    let event = &reply.switch_settled[0];
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.failed, vec![WindowId("w2".to_owned())]);
}

#[test]
fn switch_missing_or_uncorrelated_evidence_stays_pending() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    let w2_lw = last_write_confirmed(true, rev, 4);
    let feedback = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 4, frame)],
    );
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                sfact("w1", "out-a", 0),
                with_lw(sfact("w2", "out-a", 100), w2_lw),
            ],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert!(
        reply.switch_settled.is_empty(),
        "missing member stays pending"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned()))
    );
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    let bad_rev = last_write_confirmed(true, rev + 1, 4);
    let w1_lw = last_write_confirmed(false, rev, 4);
    let feedback = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 4, frame)],
    );
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_lw),
                with_lw(sfact("w2", "out-a", 100), bad_rev),
            ],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert!(
        reply.switch_settled.is_empty(),
        "revision mismatch stays pending"
    );
}

#[test]
fn switch_first_evidence_immutable_and_future_stays_failed() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    let future_w2 = last_write_confirmed(true, rev, 5);
    let w1_lw = last_write_confirmed(false, rev, 4);
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_lw),
                with_lw(sfact("w2", "out-a", 100), future_w2),
            ],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(reply.switch_settled.is_empty(), "no feedback yet: pending");
    let feedback = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 5, frame)],
    );
    let reply = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "feedback completes the latch"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(
        event.failed,
        vec![WindowId("w2".to_owned())],
        "future-dated stays failed"
    );
}

#[test]
fn switch_held_raw_evidence_correlates_without_healthy_visibility() {
    use crate::world_report::retained_window_fields;
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    let w1_lw = last_write_confirmed(false, rev, 4);
    let feedback = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 4, frame)],
    );
    let mut held_report = report(
        4,
        vec![with_lw(
            sfact("w2", "out-a", 100),
            last_write_confirmed(true, rev, 4),
        )],
        vec![],
        None,
        None,
        vec![feedback],
        "",
    );
    // w1 (departing hide) rides held with its lossless raw lastWrite; w2
    // (selected show) stays live so arrival still proves.
    let w1_fact = with_lw(sfact("w1", "out-a", 0), w1_lw);
    held_report.held_windows = vec![HeldWindowRow {
        window: WindowId("w1".to_owned()),
        ordinal: 1,
        output: Some(OutputId("out-a".to_owned())),
        fields: retained_window_fields(&w1_fact),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let reply = accepted(&mut engine, &held_report);
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "held raw completes: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert_eq!(event.focus, SwitchFocus::Proven);
}

#[test]
fn switch_close_and_supersession_emit_nothing_stale() {
    let (mut engine, _home, _away) = select_setup();
    let _granted = select(&mut engine, 3, &_away.clone());
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![sfact("w1", "out-a", 0)],
            vec!["w2"],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(reply.switch_settled.is_empty());
    assert!(engine.retained_selection.is_empty());
    let (mut engine, _home, away) = select_setup();
    let _granted = select(&mut engine, 3, &away);
    let _second = select(&mut engine, 5, &away);
    let reply = accepted(
        &mut engine,
        &report(
            6,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        reply
            .switch_settled
            .iter()
            .all(|e| e.operation_id != "op-3"),
        "no stale event for superseded op"
    );
}

#[test]
fn switch_late_admission_never_joins_frozen_set() {
    let (mut engine, _home, away) = boot_three();
    let _ = send_to(&mut engine, 2, "w2", &away);
    let granted = accepted(
        &mut engine,
        &report(
            3,
            three_facts("out-a"),
            vec![],
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    let rev = granted.state_revision;
    let frozen_before: Vec<WindowId> = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding")
        .switch_members
        .iter()
        .map(|m| m.window.clone())
        .collect();
    let moved = send_to(&mut engine, 4, "w3", &away);
    assert!(moved.state_revision > rev);
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding");
    let frozen_after: Vec<WindowId> = binding
        .switch_members
        .iter()
        .map(|m| m.window.clone())
        .collect();
    assert_eq!(
        frozen_after, frozen_before,
        "late admission never mutates the frozen set"
    );
    assert_eq!(
        engine
            .selection_targets
            .get(&WindowId("w3".to_owned()))
            .expect("late target")
            .rev,
        moved.state_revision
    );
}

#[test]
fn switch_shared_fanout_freezes_all_outputs_and_retires_flight() {
    let mut engine = shared_engine();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let windows = || {
        vec![
            sfact("w1", "out-a", 0),
            sfact("w2", "out-a", 100),
            sfact("w3", "out-b", 800),
            sfact("w4", "out-b", 900),
        ]
    };
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            windows(),
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let ring_a: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let away = ring_a[1].clone();
    let _ = accepted(
        &mut engine,
        &report_multi(
            2,
            windows(),
            vec![],
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w2".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            vec![],
            "w2",
            screens.clone(),
        ),
    );
    let granted = accepted(
        &mut engine,
        &report_multi(
            3,
            windows(),
            vec![],
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
            screens.clone(),
        ),
    );
    let rev = granted.state_revision;
    let effect = granted.effects[0].clone();
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding")
        .clone();
    assert!(
        binding.switch_members.len() >= 4,
        "fanout freezes all: {binding:?}"
    );
    assert!(binding.switch_members.iter().all(|m| m.rev == rev));
    assert!(
        engine
            .retained
            .iter()
            .any(|r| r.target.operation_id == "op-3"
                && matches!(r.target.kind, crate::world_report::TransferKind::View)),
        "real view flight retained"
    );
    let frame_a = granted
        .windows
        .iter()
        .find(|r| r.lifetime.0 == effect.target.0)
        .and_then(|r| r.effective_frame);
    let mut facts = Vec::new();
    for (window, out, x, shown) in [
        ("w1", "out-a", 0, false),
        ("w2", "out-a", 100, true),
        ("w3", "out-b", 800, false),
        ("w4", "out-b", 900, false),
    ] {
        facts.push(with_lw(
            sfact(window, out, x),
            last_write_confirmed(shown, rev, 4),
        ));
    }
    let feedback = feedback_row(
        &effect.id,
        &effect.target.0,
        effect.issued_revision,
        true,
        Some(&effect.target.0),
        vec![bracket(&effect.target.0, "out-a", 4, frame_a)],
    );
    let reply = accepted(
        &mut engine,
        &report_multi(
            4,
            facts,
            vec![],
            None,
            None,
            vec![feedback],
            "",
            screens.clone(),
        ),
    );
    assert_eq!(reply.switch_settled.len(), 1, "shared terminal: {reply:?}");
    assert!(
        engine
            .retained
            .iter()
            .all(|r| r.target.operation_id != "op-3"),
        "real parent flight retires"
    );
}

#[test]
fn switch_estimate_and_reserve_cover_maximum_ids() {
    use crate::world_report::{SwitchEvent, SwitchFocus, SwitchOutcome};
    use crate::world_sizing::{
        MAX_SWITCH_EVENT_FIXED_BYTES, MAX_SWITCH_FAILED_ROW_BYTES, max_switch_settle_reserve,
        switch_settled_estimate,
    };
    let long_window = "w".repeat(128);
    let long_op = format!("op-{}", u64::MAX);
    let event = SwitchEvent {
        operation_id: long_op,
        outcome: SwitchOutcome::CompletedWithFailures,
        failed: vec![WindowId(long_window)],
        focus: SwitchFocus::Unproven,
        fallback: true,
    };
    let estimated = switch_settled_estimate(std::slice::from_ref(&event));
    let wire = {
        let mut out = String::new();
        crate::world_sizing::encode_switch_settled(&mut out, std::slice::from_ref(&event));
        out
    };
    assert!(estimated >= wire.len(), "estimate covers maximum ids");
    let reserve = max_switch_settle_reserve(1, 1);
    assert!(reserve >= estimated, "reserve covers one maximum event");
    assert_eq!(
        reserve,
        19 + MAX_SWITCH_EVENT_FIXED_BYTES + MAX_SWITCH_FAILED_ROW_BYTES
    );
    assert_eq!(max_switch_settle_reserve(0, 0), 0);
}

#[test]
fn switch_marginal_empty_event_reserve_covers_worst_tokens() {
    // Worst-token empty event: longest op (`op-<u64::MAX>`), longest
    // outcome (`completed-with-failures`), longest focus (`unproven`),
    // longer bool (`false`). Fixed 51 + 25 + 25 + 10 + 5 + 1 comma = 117
    // per event; the pre-mutation reserve must cover the exact estimate
    // for one event and for concurrent empty events alike (the old 115
    // bound fails this: 19 + 115 = 134 < 135 estimated).
    use crate::world_report::{SwitchEvent, SwitchFocus, SwitchOutcome};
    use crate::world_sizing::{max_switch_settle_reserve, switch_settled_estimate};
    let worst = || SwitchEvent {
        operation_id: format!("op-{}", u64::MAX),
        outcome: SwitchOutcome::CompletedWithFailures,
        failed: Vec::new(),
        focus: SwitchFocus::Unproven,
        fallback: false,
    };
    for events in [vec![worst()], vec![worst(), worst()]] {
        let n = events.len();
        let estimated = switch_settled_estimate(&events);
        // Exact content: 116 bytes per worst-token empty event
        // (51 + 25 + 25 + 10 + 5), one comma between rows, plus the
        // 19-byte section key.
        assert_eq!(
            estimated,
            19 + n * 116 + (n - 1),
            "exact estimate for empty events"
        );
        let mut wire = String::new();
        crate::world_sizing::encode_switch_settled(&mut wire, &events);
        // The section writer emits only the `[...]` array; the estimate
        // additionally carries the `,"switchSettled":` key (19) while the
        // writer carries the array brackets (2).
        assert_eq!(wire.len(), estimated - 17, "section key accounts the gap");
        let reserve = max_switch_settle_reserve(n, 0);
        assert!(reserve >= estimated, "reserve covers {n} empty events");
    }
    // Failed-list maximum: one 128-char id plus the fixed row above stays
    // covered by the per-id reserve term.
    let failed_event = SwitchEvent {
        operation_id: format!("op-{}", u64::MAX),
        outcome: SwitchOutcome::CompletedWithFailures,
        failed: vec![WindowId("w".repeat(128))],
        focus: SwitchFocus::Unproven,
        fallback: false,
    };
    let estimated = switch_settled_estimate(std::slice::from_ref(&failed_event));
    let mut wire = String::new();
    crate::world_sizing::encode_switch_settled(&mut wire, std::slice::from_ref(&failed_event));
    // Exact: section key (19) + one worst-token event (116) + one
    // 128-char failed id quoted (130); writer omits the key, keeps brackets.
    assert_eq!(
        estimated,
        19 + 116 + 130,
        "exact estimate for max failed list"
    );
    assert_eq!(wire.len(), estimated - 17, "section key accounts the gap");
    assert!(
        max_switch_settle_reserve(1, 1) >= estimated,
        "reserve covers max failed list"
    );
}

#[test]
fn switch_pre_mutation_reserve_rejects_before_mutation_with_retry() {
    use crate::world_sizing::reply_upper_bound;
    let (mut engine, _home, away) = select_setup();
    let _granted = select(&mut engine, 3, &away);
    let rev = engine.state_revision;
    let mut members = Vec::new();
    for i in 0..4000 {
        members.push(crate::world_selection::SwitchMember {
            window: WindowId(format!("w-pad-{i:05}")),
            home: DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId(away.clone()),
            },
            shown: false,
            rev,
        });
    }
    if let Some(binding) = engine
        .retained_selection
        .get_mut(&OutputId("out-a".to_owned()))
    {
        binding.switch_members = members;
    }
    let reserve = engine.switch_settle_reserve();
    assert!(reserve > 4000 * 100, "reserve inflates: {reserve}");
    let mut windows = vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)];
    let logical = engine.engine.domain_keys().len();
    let staged = engine.staged.facts.len() + engine.staged.held_windows.len();
    let retained = engine.retained_publish_footprint();
    let mut big = report(
        50,
        windows.clone(),
        vec![],
        Some(50),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: away.clone(),
        }),
        vec![],
        "",
    );
    while reply_upper_bound(&big, 0, logical, 1, staged, retained).saturating_add(reserve)
        <= WORLD_MAX_REPLY_BYTES
    {
        let i = windows.len();
        windows.push(sfact(&format!("w-big-{i:05}"), "out-a", 0));
        assert!(windows.len() < crate::world_report::MAX_WORLD_FACTS);
        big = report(
            50,
            windows.clone(),
            vec![],
            Some(50),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        );
    }
    let before_rev = engine.state_revision;
    match engine.handle_report_legacy(&big) {
        LegacyWorldOutcome::Rejected { kind, message } => {
            assert_eq!(
                message, "reply exceeds size bound",
                "pre-mutation bound: {kind}"
            );
        }
        other => panic!("expected reply-size refusal, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision, before_rev,
        "rejected report mutates nothing"
    );
    let small = report(
        50,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(50),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: away.clone(),
        }),
        vec![],
        "",
    );
    match engine.handle_report_legacy(&small) {
        LegacyWorldOutcome::Accepted(_) | LegacyWorldOutcome::CommandConsumed(_) => {}
        other => panic!("same-seq smaller retry must commit, got {other:?}"),
    }
}

#[test]
fn switch_rejected_command_keeps_presettled_terminal_and_consumes_seq() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    // The switch evidence is spent before the command-only rollback
    // boundary, so a refused command restores a pre-command image that
    // already carries the terminal: the event publishes with the refusal.
    let refused_settle = report(
        4,
        vec![
            with_lw(sfact("w1", "out-a", 0), last_write_confirmed(false, rev, 4)),
            with_lw(
                sfact("w2", "out-a", 100),
                last_write_confirmed(true, rev, 4),
            ),
        ],
        vec![],
        Some(4),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-nope".to_owned(),
        }),
        vec![feedback_row(
            &effect.id,
            "w2",
            effect.issued_revision,
            true,
            Some("w2"),
            vec![bracket("w2", "out-a", 4, frame)],
        )],
        "",
    );
    let reply = accepted(&mut engine, &refused_settle);
    assert!(
        matches!(
            reply.command_result,
            Some(crate::world_report::CommandResult::Rejected { .. })
        ),
        "unknown workspace refuses"
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "terminal survives rollback: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding still releases"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive the refusal"
    );
    assert_eq!(
        engine
            .selection_targets
            .get(&WindowId("w1".to_owned()))
            .expect("hide target")
            .rev,
        rev
    );
    assert_eq!(
        engine
            .selection_targets
            .get(&WindowId("w2".to_owned()))
            .expect("show target")
            .rev,
        rev
    );
    // The refused seq is consumed: replaying it reconciles observation
    // only, with no revived terminal and no re-execution.
    match engine.handle_report_legacy(&report(
        5,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(4),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-nope".to_owned(),
        }),
        vec![],
        "",
    )) {
        LegacyWorldOutcome::CommandConsumed(consumed) => {
            assert!(consumed.switch_settled.is_empty(), "no revived terminal");
        }
        other => panic!("expected consumed replay, got {other:?}"),
    }
}

#[test]
fn switch_incomplete_inventory_settles_spent_evidence_without_renewal() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    let mut gapped = report(
        4,
        vec![
            with_lw(sfact("w1", "out-a", 0), last_write_confirmed(false, rev, 4)),
            with_lw(
                sfact("w2", "out-a", 100),
                last_write_confirmed(true, rev, 4),
            ),
        ],
        vec![],
        None,
        None,
        vec![feedback_row(
            &effect.id,
            "w2",
            effect.issued_revision,
            true,
            Some("w2"),
            vec![bracket("w2", "out-a", 4, frame)],
        )],
        "",
    );
    gapped.inventory_complete = false;
    let reply = accepted(&mut engine, &gapped);
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "gap never blocks the terminal: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases on the gap"
    );
    // Targets keep the issuing revision and authority is held, never
    // renewed: the gap settles spent evidence without granting anything.
    for window in ["w1", "w2"] {
        assert_eq!(
            engine
                .selection_targets
                .get(&WindowId(window.to_owned()))
                .expect("target")
                .rev,
            rev,
            "{window} keeps its revision"
        );
    }
    assert!(
        reply.effects.iter().all(|effect| !effect.await_shown),
        "no renewed arrival grant on the gap"
    );
    assert_eq!(
        row(&reply, "w2").visibility_authority,
        ScopeAuthority::Hold(crate::world_report::HoldReason::IncompleteInventory),
        "authority holds under the gap"
    );
}

#[test]
fn switch_two_output_bindings_settle_two_events_in_one_report() {
    // Per-output-local mode: independent selections on out-a and out-b
    // retain side-by-side bindings that settle together in one report.
    let mut engine = engine();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let windows = || {
        vec![
            sfact("w1", "out-a", 0),
            sfact("w2", "out-a", 100),
            sfact("w3", "out-b", 800),
            sfact("w4", "out-b", 900),
        ]
    };
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            windows(),
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let ring = |output: &str| {
        boot.workspaces
            .iter()
            .filter(|row| row.output.0 == output)
            .map(|row| row.id.clone())
            .collect::<Vec<_>>()
    };
    let (away_a, away_b) = (ring("out-a")[1].clone(), ring("out-b")[1].clone());
    for (seq, window, output, away) in [(2, "w2", "out-a", &away_a), (3, "w4", "out-b", &away_b)] {
        let _ = accepted(
            &mut engine,
            &report_multi(
                seq,
                windows(),
                vec![],
                Some(seq),
                Some(WorldCommand::Send {
                    window: WindowId(window.to_owned()),
                    target_output: OutputId(output.to_owned()),
                    target_workspace: away.clone(),
                    follow: false,
                }),
                vec![],
                window,
                screens.clone(),
            ),
        );
    }
    let first = accepted(
        &mut engine,
        &report_multi(
            4,
            windows(),
            vec![],
            Some(4),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away_a.clone(),
            }),
            vec![],
            "",
            screens.clone(),
        ),
    );
    assert_eq!(first.effects[0].target.0, "w2");
    let rev_a = first.state_revision;
    let second = accepted(
        &mut engine,
        &report_multi(
            5,
            windows(),
            vec![],
            Some(5),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-b".to_owned()),
                workspace: away_b.clone(),
            }),
            vec![],
            "",
            screens.clone(),
        ),
    );
    assert_eq!(second.effects[0].target.0, "w4");
    let rev_b = second.state_revision;
    let effect_a = first.effects[0].clone();
    let effect_b = second.effects[0].clone();
    let frame_a = row(&second, "w2").effective_frame;
    let frame_b = row(&second, "w4").effective_frame;
    // Pre-mutation reserve covers both concurrent terminals exactly.
    let reserve = engine.switch_settle_reserve();
    assert_eq!(
        reserve,
        crate::world_sizing::max_switch_settle_reserve(2, 4),
        "two bindings with four frozen members"
    );
    let settle = report_multi(
        6,
        vec![
            with_lw(
                sfact("w1", "out-a", 0),
                last_write_confirmed(false, rev_a, 6),
            ),
            with_lw(
                sfact("w2", "out-a", 100),
                last_write_confirmed(true, rev_a, 6),
            ),
            with_lw(
                sfact("w3", "out-b", 800),
                last_write_confirmed(false, rev_b, 6),
            ),
            with_lw(
                sfact("w4", "out-b", 900),
                last_write_confirmed(true, rev_b, 6),
            ),
        ],
        vec![],
        None,
        None,
        vec![
            feedback_row(
                &effect_a.id,
                "w2",
                effect_a.issued_revision,
                true,
                Some("w2"),
                vec![bracket("w2", "out-a", 6, frame_a)],
            ),
            feedback_row(
                &effect_b.id,
                "w4",
                effect_b.issued_revision,
                true,
                Some("w4"),
                vec![bracket("w4", "out-b", 6, frame_b)],
            ),
        ],
        "",
        screens.clone(),
    );
    let reply = accepted(&mut engine, &settle);
    assert_eq!(
        reply.switch_settled.len(),
        2,
        "one event per settled binding: {reply:?}"
    );
    let mut ops: Vec<String> = reply
        .switch_settled
        .iter()
        .map(|e| e.operation_id.clone())
        .collect();
    ops.sort();
    assert_eq!(ops, vec!["op-4".to_owned(), "op-5".to_owned()]);
    for event in &reply.switch_settled {
        assert_eq!(event.outcome, SwitchOutcome::Complete);
        assert!(event.failed.is_empty());
        assert_eq!(event.focus, SwitchFocus::Proven);
        assert!(!event.fallback);
    }
    let wire = crate::world_report::encode_reply_canonical(&reply);
    assert!(
        crate::world_report::reply_estimate(&reply) >= wire.len(),
        "estimate covers the two-event reply without truncation"
    );
    assert!(
        engine.retained_selection.is_empty(),
        "both bindings release"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned()))
            && engine
                .selection_scopes
                .contains_key(&OutputId("out-b".to_owned())),
        "both scopes survive"
    );
    let steady = accepted(
        &mut engine,
        &report_multi(
            7,
            windows(),
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    assert!(
        steady.switch_settled.is_empty(),
        "next report publishes empty"
    );
}

#[test]
fn switch_duplicate_replays_identical_event_then_next_report_empties() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    let settle = report(
        4,
        vec![
            with_lw(sfact("w1", "out-a", 0), last_write_confirmed(false, rev, 4)),
            with_lw(
                sfact("w2", "out-a", 100),
                last_write_confirmed(true, rev, 4),
            ),
        ],
        vec![],
        None,
        None,
        vec![feedback_row(
            &effect.id,
            "w2",
            effect.issued_revision,
            true,
            Some("w2"),
            vec![bracket("w2", "out-a", 4, frame)],
        )],
        "",
    );
    let reply = accepted(&mut engine, &settle);
    assert_eq!(reply.switch_settled.len(), 1);
    let wire = crate::world_report::encode_reply_canonical(&reply);
    match engine.handle_report_legacy(&settle) {
        LegacyWorldOutcome::Duplicate(cached) => {
            assert_eq!(cached.switch_settled, reply.switch_settled);
            assert_eq!(
                crate::world_report::encode_reply_canonical(&cached),
                wire,
                "duplicate returns the byte-identical cached event"
            );
        }
        other => panic!("expected cached duplicate, got {other:?}"),
    }
    let again = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        again.switch_settled.is_empty(),
        "next report publishes empty"
    );
}

#[test]
fn switch_conflicting_held_row_correlates_without_healthy_visibility() {
    use crate::world_report::retained_window_fields;
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    let w1_lw = last_write_confirmed(false, rev, 4);
    let feedback = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![bracket("w2", "out-a", 4, frame)],
    );
    let mut held_report = report(
        4,
        vec![with_lw(
            sfact("w2", "out-a", 100),
            last_write_confirmed(true, rev, 4),
        )],
        vec![],
        None,
        None,
        vec![feedback],
        "",
    );
    // Same lossless raw as the invalid-fact path, but graded conflicting:
    // the kind never gates the decoded post proof.
    let w1_fact = with_lw(sfact("w1", "out-a", 0), w1_lw);
    held_report.held_windows = vec![HeldWindowRow {
        window: WindowId("w1".to_owned()),
        ordinal: 1,
        output: Some(OutputId("out-a".to_owned())),
        fields: retained_window_fields(&w1_fact),
        diagnostic: None,
        kind: RowDiagnosticKind::ConflictingFact,
    }];
    let reply = accepted(&mut engine, &held_report);
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "conflicting held raw completes: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
}

#[test]
fn switch_nocall_hide_terminates_without_retry_and_preserves_state() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let home_w1 = engine
        .selection_targets
        .get(&WindowId("w1".to_owned()))
        .expect("hide target")
        .home
        .clone();
    let home_w2 = engine
        .selection_targets
        .get(&WindowId("w2".to_owned()))
        .expect("show target")
        .home
        .clone();
    // Explicit no-call hide: a correlated row with no post can never
    // confirm, so the departing hide fails terminally while the show
    // confirms; unproven focus (no arrival) keeps the outcome failed.
    let w1_lw = last_write_with(
        false,
        rev,
        VisibilitySetter::NotCalled,
        VisibilityReason::ScopeHeld,
        None,
    );
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_lw),
                with_lw(
                    sfact("w2", "out-a", 100),
                    last_write_confirmed(true, rev, 4),
                ),
            ],
            vec![],
            None,
            None,
            vec![notcalled_feedback(&effect.id, "w2", effect.issued_revision)],
            "",
        ),
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "no-call hide still terminates: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.failed, vec![WindowId("w1".to_owned())]);
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        reply.effects.iter().all(|effect| !effect.await_shown),
        "terminal mints no fresh grant"
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "no binding survives for a retry"
    );
    for (window, home) in [("w1", &home_w1), ("w2", &home_w2)] {
        let entry = engine
            .selection_targets
            .get(&WindowId(window.to_owned()))
            .expect("target survives");
        assert_eq!(entry.rev, rev, "{window} keeps its revision");
        assert_eq!(&entry.home, home, "{window} keeps its home");
    }
    // No retry after the terminal: a later observation publishes empty
    // with the same preserved targets and no new effects.
    let again = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(again.switch_settled.is_empty(), "no second terminal");
    assert!(
        again.effects.iter().all(|effect| !effect.await_shown),
        "no resurrected grant"
    );
    assert!(engine.retained_selection.is_empty(), "nothing to retry");
}

#[test]
fn switch_threw_hide_with_mismatched_post_terminates() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    // Throwing hide with a mismatched post (visible where hidden was
    // granted) is a correlated failure; accepted focus feedback that
    // names the wrong foreground stays unproven without arrival.
    let w1_lw = last_write_with(
        false,
        rev,
        VisibilitySetter::Threw,
        VisibilityReason::Empty,
        Some(confirm_sample(4, true)),
    );
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_lw),
                with_lw(
                    sfact("w2", "out-a", 100),
                    last_write_confirmed(true, rev, 4),
                ),
            ],
            vec![],
            None,
            None,
            vec![feedback_row(
                &effect.id,
                "w2",
                effect.issued_revision,
                false,
                Some("w1"),
                vec![],
            )],
            "",
        ),
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "threw hide still terminates: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.failed, vec![WindowId("w1".to_owned())]);
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive failure"
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "no binding survives for a retry"
    );
}

#[test]
fn switch_partial_hide_failure_with_proven_arrival_and_focus() {
    // Terminal-partial-hide discriminator (reference-outcomes.md: arrival
    // proven + focus PROVEN + one hide failed while another hide succeeds):
    // the switch settles CompletedWithFailures with exactly the stuck
    // member failed, never pending and never a staged handoff.
    let (mut engine, _home, away) = boot_three();
    let _ = send_to(&mut engine, 2, "w2", &away);
    let granted = accepted(
        &mut engine,
        &report(
            3,
            three_facts("out-a"),
            vec![],
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    let effect = granted.effects[0].clone();
    assert_eq!(effect.target.0, "w2", "focus resolves the selected member");
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    // Three frozen members: show w2 plus hides w1 and w3.
    let mut frozen: Vec<String> = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding")
        .switch_members
        .iter()
        .map(|member| member.window.0.clone())
        .collect();
    frozen.sort();
    assert_eq!(
        frozen,
        vec!["w1".to_owned(), "w2".to_owned(), "w3".to_owned()]
    );
    // w1 hide refuses with proper readable unchanged pre/post evidence
    // (visible before and after, never a ShowBlocked token); w3 hide and
    // w2 show confirm; full fresh arrival plus w2 foreground proof.
    let w1_refused = LastWrite {
        property: LastWriteProperty::Visibility,
        target: false,
        revision: rev,
        path: LastWritePath::Staged,
        setter: VisibilitySetter::Refused,
        reason: VisibilityReason::Empty,
        pre: Some(confirm_sample(3, true)),
        post: Some(confirm_sample(4, true)),
    };
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), w1_refused),
                with_lw(
                    sfact("w2", "out-a", 100),
                    last_write_confirmed(true, rev, 4),
                ),
                with_lw(
                    sfact("w3", "out-a", 200),
                    last_write_confirmed(false, rev, 4),
                ),
            ],
            vec![],
            None,
            None,
            vec![feedback_row(
                &effect.id,
                "w2",
                effect.issued_revision,
                true,
                Some("w2"),
                vec![bracket("w2", "out-a", 4, frame)],
            )],
            "",
        ),
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "partial hide still terminates: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.failed, vec![WindowId("w1".to_owned())]);
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(!event.fallback);
    // The actual parent view flight retires; homes and stable target
    // revisions survive the terminal.
    assert!(
        engine
            .retained
            .iter()
            .all(|retained| retained.target.operation_id != "op-3"),
        "actual parent flight retires"
    );
    for window in ["w1", "w2", "w3"] {
        assert_eq!(
            engine
                .selection_targets
                .get(&WindowId(window.to_owned()))
                .expect("target survives")
                .rev,
            rev,
            "{window} keeps its revision"
        );
    }
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive failure"
    );
    // No retry, no fresh grant, no admission or history disturbance on the
    // next report: the published view is unchanged and the terminal never
    // repeats.
    let grant_view = granted
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone());
    let again = accepted(
        &mut engine,
        &report(5, three_facts("out-a"), vec![], None, None, vec![], ""),
    );
    assert!(again.switch_settled.is_empty(), "no second terminal");
    assert!(
        again.effects.iter().all(|effect| !effect.await_shown),
        "no resurrected grant"
    );
    assert!(engine.retained_selection.is_empty(), "nothing to retry");
    assert_eq!(
        again
            .outputs
            .iter()
            .find(|row| row.output.0 == "out-a")
            .and_then(|row| row.logical_view.clone()),
        grant_view,
        "view history undisturbed by the terminal"
    );
}

#[test]
fn focusless_occupied_to_empty_freezes_hide_and_completes_unproven() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    assert_eq!(granted.effects.len(), 1, "setup focuses: {granted:?}");
    let ring = ring_ids(&granted, "out-a");
    let empty = ring.last().expect("fresh trailing spare").clone();
    assert_ne!(empty, away, "spare is not the occupied workspace");
    let reply = select(&mut engine, 4, &empty);
    assert!(reply.effects.is_empty(), "no focus invented: {reply:?}");
    assert!(
        reply.effects.iter().all(|effect| !effect.await_shown),
        "focusless mints no await grant"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("focusless binding retained");
    assert!(binding.effect.is_none(), "no focus effect: {binding:?}");
    assert!(
        !binding.switch_members.is_empty(),
        "departing hide freezes: {binding:?}"
    );
    assert!(
        binding
            .switch_members
            .iter()
            .any(|member| member.window.0 == "w2" && !member.shown),
        "w2 hide frozen: {binding:?}"
    );
    let rev = reply.state_revision;
    assert_eq!(
        row(&reply, "w2")
            .visibility
            .expect("departing hide")
            .revision,
        rev
    );
    assert!(!row(&reply, "w2").visibility.expect("hide").shown);
    assert_eq!(
        row(&reply, "w2").visibility_authority,
        ScopeAuthority::Controlled
    );
    assert!(reply.switch_settled.is_empty(), "hide needs proof: pending");
    // A verdict-less observation holds: no event, same finite binding.
    let missing = accepted(
        &mut engine,
        &report(
            5,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(missing.switch_settled.is_empty(), "no verdict, no event");
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding holds"
    );
    // First-correlated hide proof settles the terminal with unproven focus.
    let settled = accepted(
        &mut engine,
        &report(
            6,
            vec![
                sfact("w1", "out-a", 0),
                with_lw(
                    sfact("w2", "out-a", 100),
                    last_write_confirmed(false, rev, 6),
                ),
            ],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert_eq!(settled.switch_settled.len(), 1, "one terminal: {settled:?}");
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-4");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| r.target.operation_id != "op-4"),
        "actual view parent retires"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive the terminal"
    );
    assert_eq!(
        engine
            .selection_targets
            .get(&WindowId("w2".to_owned()))
            .expect("target survives")
            .rev,
        rev
    );
    let again = accepted(
        &mut engine,
        &report(
            7,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        again.switch_settled.is_empty(),
        "next report publishes empty"
    );
}

#[test]
fn focusless_occupied_to_empty_hide_refusal_fails() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let empty = ring_ids(&granted, "out-a").last().expect("spare").clone();
    let reply = select(&mut engine, 4, &empty);
    assert!(reply.effects.is_empty(), "no focus invented: {reply:?}");
    assert!(reply.switch_settled.is_empty(), "hide needs proof: pending");
    let rev = reply.state_revision;
    let refused = last_write_with(
        false,
        rev,
        VisibilitySetter::Refused,
        VisibilityReason::ShowBlocked,
        Some(confirm_sample(5, true)),
    );
    let settled = accepted(
        &mut engine,
        &report(
            5,
            vec![
                sfact("w1", "out-a", 0),
                with_lw(sfact("w2", "out-a", 100), refused),
            ],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "refusal terminates: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-4");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.failed, vec![WindowId("w2".to_owned())]);
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases on failure"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive failure"
    );
}

#[test]
fn focusless_empty_to_empty_zero_members_completes_at_issuing() {
    let mut engine = engine();
    let boot = accepted(
        &mut engine,
        &report(1, vec![], vec![], None, None, vec![], ""),
    );
    let ring = ring_ids(&boot, "out-a");
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    let away = ring[1].clone();
    // Windowless issuing report: no homed member can join the selected or
    // departing homes, so the frozen set is empty.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![],
            vec![],
            Some(2),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    assert!(reply.effects.is_empty(), "no focus invented: {reply:?}");
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "zero obligations complete at issuing: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases at issuing"
    );
}

#[test]
fn focusless_minimized_only_invents_no_focus() {
    let mut engine = engine();
    let mut w1 = sfact("w1", "out-a", 0);
    w1.minimized = Some(true);
    w1.rect = None;
    let mut w2 = sfact("w2", "out-a", 100);
    w2.minimized = Some(true);
    w2.rect = None;
    let boot = accepted(
        &mut engine,
        &report(
            1,
            vec![w1.clone(), w2.clone()],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let ring = ring_ids(&boot, "out-a");
    assert!(ring.len() >= 2);
    let away = ring[1].clone();
    // The issuing report keeps both survivors minimized: no live member
    // can take a per-window target, so the frozen set stays empty and the
    // terminal issues at once with unproven focus.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![w1, w2],
            vec![],
            Some(2),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "minimized-only invents no focus: {reply:?}"
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "zero members complete at issuing: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases at issuing"
    );
    assert!(
        engine.selection_targets.is_empty(),
        "minimized members take no targets"
    );
}

fn host_native_report(
    seq: u64,
    windows: Vec<WindowFact>,
    current: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    use crate::world_report::{NativeObservation, NativeOccupancy};
    let mut out = report(seq, windows, vec![], command_seq, command, vec![], "");
    out.native = vec![NativeObservation {
        output: OutputId("out-a".to_owned()),
        native_order: vec!["n0".to_owned(), "n1".to_owned()],
        current_native: Some(current.to_owned()),
        occupancy: vec![
            NativeOccupancy {
                native: "n0".to_owned(),
                count: None,
            },
            NativeOccupancy {
                native: "n1".to_owned(),
                count: None,
            },
        ],
    }];
    out
}

fn host_boot() -> (WorldEngine, String) {
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let away = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    (engine, away)
}

fn host_select(
    engine: &mut WorldEngine,
    seq: u64,
    workspace: &str,
) -> crate::world_report::WorldReply {
    accepted(
        engine,
        &host_native_report(
            seq,
            vec![],
            "n0",
            Some(seq),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: workspace.to_owned(),
            }),
        ),
    )
}

fn native_ref(workspace: &str) -> WorkspaceRef {
    WorkspaceRef::Native {
        output: OutputId("out-a".to_owned()),
        workspace: crate::ids::NativeWorkspaceId::parse(workspace).expect("native id"),
    }
}

/// Managed host-owned member fact: ordinary top-level window with a native
/// desktop receipt. Off-desktop members carry unreadable pre-arrival
/// visibility, which must never exclude them from the desired arrival set.
fn hfact(window: &str, native: &str, x: i32, visible: bool) -> WindowFact {
    let mut fact = sfact(window, "out-a", x);
    fact.sticky = Some(false);
    fact.native_workspace = crate::world_report::NativeWorkspaceFact::Value(native.to_owned());
    fact.visibility.visible = Some(visible);
    fact
}

fn native_bracket(window: &str, native: &str, obs: u64, frame: Option<Rect>) -> ArrivalBracket {
    ArrivalBracket {
        window: WindowId(window.to_owned()),
        obs,
        before: sample(obs, true, true, false, false),
        location: ArrivalLocation::Native {
            home: Some(native_ref(native)),
        },
        frame,
        after: sample(obs, true, true, false, false),
    }
}

fn host_select_windows(
    engine: &mut WorldEngine,
    seq: u64,
    workspace: &str,
    windows: Vec<WindowFact>,
    current: &str,
    focused: &str,
) -> crate::world_report::WorldReply {
    let mut staged = host_native_report(
        seq,
        windows,
        current,
        Some(seq),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: workspace.to_owned(),
        }),
    );
    staged.focused_global = focused.to_owned();
    accepted(engine, &staged)
}

#[test]
fn host_owned_focus_bearing_grants_await_shown_with_native_obligations() {
    let (mut engine, away) = host_boot();
    let mut staged = host_native_report(
        2,
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            hfact("w2", "n1", 200, false),
        ],
        "n0",
        None,
        None,
    );
    staged.focused_global = "w2".to_owned();
    let _ = accepted(&mut engine, &staged);
    let reply = host_select_windows(
        &mut engine,
        3,
        &away,
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            hfact("w2", "n1", 200, false),
        ],
        "n0",
        "w2",
    );
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    let effect = &reply.effects[0];
    assert_eq!(effect.target.0, "w2");
    assert!(effect.await_shown, "host-owned focus awaits arrival");
    assert_eq!(effect.command_seq, Some(3));
    assert_eq!(
        effect.cause,
        crate::world_report::EffectCause::ExplicitCommand
    );
    for row in &reply.windows {
        assert!(
            row.visibility.is_none(),
            "host-owned grants no per-window targets: {row:?}"
        );
    }
    assert!(
        engine.selection_targets.is_empty(),
        "no per-window targets authorized"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("selection binding retained");
    assert_eq!(binding.effect, Some(effect.clone()));
    assert_eq!(binding.parent_op, "op-3");
    assert!(
        binding.switch_members.is_empty(),
        "no LastWrite demand: {binding:?}"
    );
    assert_eq!(
        binding
            .view_targets
            .get(&OutputId("out-a".to_owned()))
            .map(|home| home.workspace.0.clone()),
        Some(away.clone()),
        "native selector obligation: {binding:?}"
    );
    let frames: Vec<Option<Rect>> = reply
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == "w1" || row.lifetime.0 == "w2")
        .map(|row| row.effective_frame)
        .collect();
    assert!(
        frames.iter().all(|frame| frame.is_some()),
        "managed members publish frames: {reply:?}"
    );
    assert_eq!(
        binding.members.len(),
        2,
        "every managed selected-home member freezes: {binding:?}"
    );
    for member in &binding.members {
        assert!(
            ["w1", "w2"].contains(&member.window.0.as_str()),
            "only selected-home members: {binding:?}"
        );
        assert_eq!(member.home.workspace.0, away);
        let published = reply
            .windows
            .iter()
            .find(|row| row.lifetime == member.window)
            .expect("published row")
            .effective_frame;
        assert_eq!(member.frame, published, "frozen final frames");
    }
    let wire = crate::world_report::encode_reply_canonical(&reply);
    assert!(
        crate::world_report::reply_estimate(&reply) >= wire.len(),
        "estimate covers the host-owned grant"
    );
    assert!(
        wire.contains("\"awaitShown\":true"),
        "producer spells the awaitShown grant"
    );
}

fn host_focus_grant() -> (WorldEngine, String, crate::world_report::WorldReply) {
    let (mut engine, away) = host_boot();
    let stage = || {
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            hfact("w2", "n1", 200, false),
        ]
    };
    let mut staged = host_native_report(2, stage(), "n0", None, None);
    staged.focused_global = "w2".to_owned();
    let _ = accepted(&mut engine, &staged);
    let reply = host_select_windows(&mut engine, 3, &away, stage(), "n0", "w2");
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    assert!(
        reply.effects[0].await_shown,
        "host-owned focus awaits arrival"
    );
    assert_eq!(reply.effects[0].target.0, "w2");
    (engine, away, reply)
}

fn host_member_frames(
    reply: &crate::world_report::WorldReply,
) -> std::collections::BTreeMap<WindowId, Option<Rect>> {
    reply
        .windows
        .iter()
        .map(|row| (row.lifetime.clone(), row.effective_frame))
        .collect()
}

fn host_proof_windows(visible: bool) -> Vec<WindowFact> {
    vec![
        hfact("w0", "n0", 0, true),
        hfact("w1", "n1", 100, visible),
        hfact("w2", "n1", 200, visible),
    ]
}

#[test]
fn host_owned_focus_bearing_native_arrival_and_selector_complete_proven() {
    let (mut engine, away, grant) = host_focus_grant();
    let effect = grant.effects[0].clone();
    let frames = host_member_frames(&grant);
    let arrival = ["w1", "w2"]
        .into_iter()
        .map(|window| native_bracket(window, "n1", 3, frames[&WindowId(window.to_owned())]))
        .collect();
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut proof = host_native_report(4, host_proof_windows(true), "n1", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    assert_eq!(settled.switch_settled.len(), 1, "proven: {settled:?}");
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| r.target.operation_id != "op-3"),
        "actual parent retires"
    );
    let _ = away;
}

#[test]
fn host_owned_focus_bearing_selector_proof_alone_holds() {
    let (mut engine, _away, _grant) = host_focus_grant();
    // Native echo proves the selector with no focus feedback at all: the
    // feedback gate still holds the terminal.
    let settled = accepted(
        &mut engine,
        &host_native_report(4, host_proof_windows(true), "n1", None, None),
    );
    assert!(
        settled.switch_settled.is_empty(),
        "selector alone never completes: {settled:?}"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding holds for the missing feedback"
    );
}

#[test]
fn host_owned_focus_bearing_arrival_without_selector_holds() {
    let (mut engine, _away, grant) = host_focus_grant();
    let effect = grant.effects[0].clone();
    let frames = host_member_frames(&grant);
    // Foreground proves and every frozen member proves its Native arm,
    // but the current stays home with no attempt rows: the native
    // obligation pends and the terminal holds.
    let arrival = ["w1", "w2"]
        .into_iter()
        .map(|window| native_bracket(window, "n1", 3, frames[&WindowId(window.to_owned())]))
        .collect();
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut proof = host_native_report(4, host_proof_windows(true), "n0", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    assert!(
        settled.switch_settled.is_empty(),
        "missing selector proof holds: {settled:?}"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding holds");
    assert!(binding.foreground_proven, "foreground still proves");
    assert!(binding.arrival_ready, "arrival still proves");
}

#[test]
fn host_owned_focus_bearing_omitted_target_arrival_fails_unproven() {
    // Artificial invariant pin, not a product path: the producer route
    // always freezes the minted focus target, so a real binding never
    // omits it and this omission is not proven reachable. Feedback that
    // proves foreground with fully valid arrival for the other frozen
    // member but no bracket for the effect target still fails the
    // terminal with focus unproven.
    let (mut engine, _away, grant) = host_focus_grant();
    let effect = grant.effects[0].clone();
    let frames = host_member_frames(&grant);
    let arrival = vec![native_bracket(
        "w1",
        "n1",
        3,
        frames[&WindowId("w1".to_owned())],
    )];
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut proof = host_native_report(4, host_proof_windows(true), "n1", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "target omission fails: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.focus, SwitchFocus::Unproven);
}

#[test]
fn host_owned_focus_bearing_accepted_without_foreground_fails() {
    let (mut engine, _away, grant) = host_focus_grant();
    let effect = grant.effects[0].clone();
    let frames = host_member_frames(&grant);
    // Setter success without foreground equality is Unproven, never a
    // completion: the shared predicate fails the terminal.
    let arrival = ["w1", "w2"]
        .into_iter()
        .map(|window| native_bracket(window, "n1", 3, frames[&WindowId(window.to_owned())]))
        .collect();
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        false,
        None,
        arrival,
    );
    let mut proof = host_native_report(4, host_proof_windows(true), "n1", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "unproven focus fails: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases on failure"
    );
}

#[test]
fn host_owned_focus_bearing_rejects_wrong_native_evidence() {
    // Each case keeps the selector pending (current stays home) so the
    // terminal holds either way; the latch must record arrival unreadiness
    // for every wrong-home/output/frame/watermark/lifetime/arm variant.
    for name in [
        "wrong-home",
        "null-home",
        "wrong-output",
        "wrong-frame",
        "null-frame",
        "watermark-splay",
        "stale-bracket",
        "lifetime-mismatch",
        "partial-members",
    ] {
        let (mut engine, _away, grant) = host_focus_grant();
        let effect = grant.effects[0].clone();
        let frames = host_member_frames(&grant);
        let frame = frames[&WindowId("w1".to_owned())];
        let good = |window: &str| native_bracket(window, "n1", 3, frame);
        let mut bad = good("w1");
        match name {
            "wrong-home" => bad = native_bracket("w1", "n0", 3, frame),
            "null-home" => bad.location = ArrivalLocation::Native { home: None },
            "wrong-output" => {
                bad.location = ArrivalLocation::Native {
                    home: Some(WorkspaceRef::Native {
                        output: OutputId("out-a".to_owned()),
                        workspace: crate::ids::NativeWorkspaceId::parse("n0").expect("native id"),
                    }),
                };
            }
            "wrong-frame" => {
                bad = native_bracket("w1", "n1", 3, Some(rect(1, 2, 3, 4)));
            }
            "null-frame" => bad = native_bracket("w1", "n1", 3, None),
            "watermark-splay" => bad.before.obs = 2,
            "stale-bracket" => bad = native_bracket("w1", "n1", 2, frame),
            "lifetime-mismatch" => bad.after.lifetime_matched = Some(false),
            _ => {}
        }
        let arrival = if name == "partial-members" {
            vec![good("w2")]
        } else if name == "wrong-frame" || name == "null-frame" {
            vec![bad.clone(), native_bracket("w2", "n1", 3, bad.frame)]
        } else {
            vec![bad, good("w2")]
        };
        let row = feedback_row(
            &effect.id,
            "w2",
            effect.issued_revision,
            true,
            Some("w2"),
            arrival,
        );
        let mut proof = host_native_report(4, host_proof_windows(true), "n0", None, None);
        proof.focus_feedback = vec![row];
        let settled = accepted(&mut engine, &proof);
        assert!(
            settled.switch_settled.is_empty(),
            "{name}: wrong evidence never completes"
        );
        let binding = engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("binding holds");
        assert!(binding.foreground_proven, "{name}: foreground still proves");
        assert!(!binding.arrival_ready, "{name}: never proves readiness");
    }
}

#[test]
fn host_owned_focus_bearing_wrong_host_arms_refuse_at_ingress() {
    // Mixed or unknown-output arms never reach the proof: typed ingress
    // refuses the wire message.
    for (name, location) in [
        (
            "logical-in-native-arm",
            ArrivalLocation::Native {
                home: Some(WorkspaceRef::Logical {
                    output: OutputId("out-a".to_owned()),
                    workspace: WorkspaceId("ws-logical".to_owned()),
                }),
            },
        ),
        (
            "output-arm-on-host-owned",
            ArrivalLocation::Output {
                out: Some(OutputId("out-a".to_owned())),
            },
        ),
        (
            "null-output-arm-on-host-owned",
            ArrivalLocation::Output { out: None },
        ),
    ] {
        let (mut engine, _away, grant) = host_focus_grant();
        let effect = grant.effects[0].clone();
        let frames = host_member_frames(&grant);
        let mut bad = native_bracket("w1", "n1", 3, frames[&WindowId("w1".to_owned())]);
        bad.location = location;
        let row = feedback_row(
            &effect.id,
            "w2",
            effect.issued_revision,
            true,
            Some("w2"),
            vec![bad],
        );
        let mut proof = host_native_report(4, host_proof_windows(true), "n0", None, None);
        proof.focus_feedback = vec![row];
        match engine.handle_report_legacy(&proof) {
            LegacyWorldOutcome::Rejected { .. } => {}
            other => panic!("{name}: wrong-host arm must refuse, got {other:?}"),
        }
        assert!(
            engine
                .retained_selection
                .contains_key(&OutputId("out-a".to_owned())),
            "{name}: refused report mutates nothing"
        );
    }
}

#[test]
fn host_owned_focus_bearing_native_refusal_fails_with_feedback() {
    let (mut engine, away, grant) = host_focus_grant();
    let effect = grant.effects[0].clone();
    // Proven selector refusal plus the latched focus feedback in the same
    // report: the shared predicate fails the switch even though focus
    // proves (settle runs before the publish reconcile).
    let frames = host_member_frames(&grant);
    let arrival = ["w1", "w2"]
        .into_iter()
        .map(|window| native_bracket(window, "n1", 3, frames[&WindowId(window.to_owned())]))
        .collect();
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut refused = host_native_report(4, host_proof_windows(true), "n0", None, None);
    refused.view_attempts = vec![view_attempt("op-3", &away, FrameSetter::Refused)];
    refused.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &refused);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "proven refusal fails: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert!(event.failed.is_empty(), "no per-window failures: {event:?}");
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases on refusal"
    );
}

#[test]
fn host_owned_focus_bearing_refusal_without_feedback_pends_until_feedback() {
    let (mut engine, away, grant) = host_focus_grant();
    let effect = grant.effects[0].clone();
    // Proven selector refusal with no focus feedback: the flight retires
    // as refused, but the refusal is the engine's own correlated attempt
    // with an unchanged preimage and the echo still home - not newer
    // external input - so the historical accounting pends with no
    // invented verdict and no actuation authority.
    let mut refused = host_native_report(4, host_proof_windows(true), "n0", None, None);
    refused.view_attempts = vec![view_attempt("op-3", &away, FrameSetter::Refused)];
    let quiet = accepted(&mut engine, &refused);
    assert!(
        quiet.switch_settled.is_empty(),
        "no invented verdict: {quiet:?}"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "own refusal pends, never supersedes"
    );
    // A further no-feedback report (the consumed raw omitted) still pends:
    // missing verdicts hold, never fail, retry, roll back, or emit.
    let held = accepted(
        &mut engine,
        &host_native_report(5, host_proof_windows(true), "n0", None, None),
    );
    assert!(held.switch_settled.is_empty(), "still pending: {held:?}");
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "accounting survives the gap"
    );
    // The first exact-tuple feedback (consumed raw omitted) fails exactly
    // once through the shared predicate: foreground proves, arrival
    // proves, the proven native refusal fails the switch.
    let frames = host_member_frames(&grant);
    let arrival = ["w1", "w2"]
        .into_iter()
        .map(|window| native_bracket(window, "n1", 3, frames[&WindowId(window.to_owned())]))
        .collect();
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut proof = host_native_report(6, host_proof_windows(true), "n0", None, None);
    proof.focus_feedback = vec![row.clone()];
    let settled = accepted(&mut engine, &proof);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "refusal fails once feedback lands: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert!(event.failed.is_empty(), "no per-window failures: {event:?}");
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases on refusal"
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| r.target.operation_id != "op-3"),
        "actual parent retires"
    );
    // Replaying the same feedback emits nothing twice.
    let replay = accepted(
        &mut engine,
        &host_native_report(7, host_proof_windows(true), "n0", None, None),
    );
    let mut replay_with_feedback =
        host_native_report(8, host_proof_windows(true), "n0", None, None);
    replay_with_feedback.focus_feedback = vec![row];
    let _ = replay;
    let again = accepted(&mut engine, &replay_with_feedback);
    assert!(
        again.switch_settled.is_empty(),
        "no second event: {again:?}"
    );
}

#[test]
fn host_owned_focus_bearing_refusal_then_reselect_supersedes_silently() {
    let (mut engine, away, _grant) = host_focus_grant();
    let mut refused = host_native_report(4, host_proof_windows(true), "n0", None, None);
    refused.view_attempts = vec![view_attempt("op-3", &away, FrameSetter::Refused)];
    let quiet = accepted(&mut engine, &refused);
    assert!(quiet.switch_settled.is_empty(), "refusal pends: {quiet:?}");
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding pends for feedback"
    );
    // A newer selection between refusal and feedback supersedes the
    // pending grant without minting an event for the abandoned parent.
    let reply = host_select_windows(&mut engine, 5, &away, host_proof_windows(true), "n0", "w1");
    assert!(
        reply.switch_settled.is_empty(),
        "supersession mints no event: {reply:?}"
    );
    assert!(
        engine
            .retained_selection
            .values()
            .all(|binding| binding.parent_op != "op-3"),
        "abandoned grant releases"
    );
}

#[test]
fn host_owned_focus_bearing_held_refusal_recovers_from_latched_raw() {
    let (mut engine, away, grant) = host_focus_grant();
    let effect = grant.effects[0].clone();
    let frames = host_member_frames(&grant);
    let arrival = ["w1", "w2"]
        .into_iter()
        .map(|window| native_bracket(window, "n1", 3, frames[&WindowId(window.to_owned())]))
        .collect();
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    // Held report: the consumed refusal latches but the incomplete screen
    // cannot decide the refusal gate, so the terminal holds.
    let mut held = host_native_report(4, host_proof_windows(true), "n0", None, None);
    held.screens = vec![{
        let mut s = screen("out-a", 0);
        s.complete = false;
        s
    }];
    held.view_attempts = vec![view_attempt("op-3", &away, FrameSetter::Refused)];
    held.focus_feedback = vec![row];
    let stalled = accepted(&mut engine, &held);
    assert!(
        stalled.switch_settled.is_empty(),
        "held refusal stays pending: {stalled:?}"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding holds across the gap"
    );
    // Recovery omits the consumed attempt: the latched raw plus the
    // readable echo naming the refused preimage still fails.
    let settled = accepted(
        &mut engine,
        &host_native_report(5, host_proof_windows(true), "n0", None, None),
    );
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "latched refusal fails: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.focus, SwitchFocus::Proven);
}

#[test]
fn host_owned_focus_bearing_shared_needs_every_output_proof() {
    use crate::connection::RawSettingValue;
    use crate::world_report::{NativeObservation, NativeOccupancy};
    let caps = HostCapabilities {
        host_owned: true,
        native_sticky: true,
        maximize: true,
        ..HostCapabilities::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        caps,
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )],
    )
    .expect("generation");
    fn hfact_on(window: &str, output: &str, native: &str, x: i32, visible: bool) -> WindowFact {
        let mut fact = sfact(window, output, x);
        fact.sticky = Some(false);
        fact.native_workspace = crate::world_report::NativeWorkspaceFact::Value(native.to_owned());
        fact.visibility.visible = Some(visible);
        fact
    }
    let mk = |seq: u64,
              cur_a: &str,
              cur_b: &str,
              windows: Vec<WindowFact>,
              feedback: Vec<FocusFeedback>,
              cmd: Option<(u64, WorldCommand)>| {
        let (s, c) = match cmd {
            Some((s, c)) => (Some(s), Some(c)),
            None => (None, None),
        };
        let mut r = report_multi(
            seq,
            windows,
            vec![],
            s,
            c,
            feedback,
            "",
            vec![screen("out-a", 0), screen("out-b", 800)],
        );
        r.native = ["out-a", "out-b"]
            .into_iter()
            .map(|o| {
                let current = if o == "out-a" { cur_a } else { cur_b };
                NativeObservation {
                    output: OutputId(o.to_owned()),
                    native_order: vec!["n0".to_owned(), "n1".to_owned()],
                    current_native: Some(current.to_owned()),
                    occupancy: ["n0", "n1"]
                        .into_iter()
                        .map(|n| NativeOccupancy {
                            native: n.to_owned(),
                            count: None,
                        })
                        .collect(),
                }
            })
            .collect();
        r
    };
    let boot = accepted(&mut engine, &mk(1, "n0", "n0", vec![], vec![], None));
    let target = boot
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    // Stage one managed member per output on the away desktop and steer
    // remembered focus to the out-a member.
    let mut staged = mk(
        2,
        "n0",
        "n0",
        vec![
            hfact_on("w0", "out-a", "n0", 0, true),
            hfact_on("wA", "out-a", "n1", 100, false),
            hfact_on("wB", "out-b", "n1", 900, false),
        ],
        vec![],
        None,
    );
    staged.focused_global = "wA".to_owned();
    let _ = accepted(&mut engine, &staged);
    let mut selecting = mk(
        3,
        "n0",
        "n0",
        vec![
            hfact_on("w0", "out-a", "n0", 0, true),
            hfact_on("wA", "out-a", "n1", 100, false),
            hfact_on("wB", "out-b", "n1", 900, false),
        ],
        vec![],
        Some((
            3,
            WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: target.clone(),
            },
        )),
    );
    selecting.focused_global = "wA".to_owned();
    let reply = accepted(&mut engine, &selecting);
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    let effect = &reply.effects[0];
    assert_eq!(effect.target.0, "wA");
    assert!(effect.await_shown, "host-owned focus awaits arrival");
    for row in &reply.windows {
        assert!(
            row.visibility.is_none(),
            "host-owned grants no per-window targets: {row:?}"
        );
    }
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("shared focus-bearing binding pends");
    assert!(binding.effect.is_some());
    assert!(
        binding.switch_members.is_empty(),
        "no LastWrite demand: {binding:?}"
    );
    assert_eq!(
        binding.view_targets.len(),
        2,
        "every output obligated: {binding:?}"
    );
    assert_eq!(
        binding
            .members
            .iter()
            .map(|member| member.window.0.clone())
            .collect::<Vec<_>>(),
        vec!["wA".to_owned()],
        "only the focus-output selected home freezes: {binding:?}"
    );
    let frame = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "wA")
        .expect("published row")
        .effective_frame;
    assert_eq!(binding.members[0].frame, frame);
    // Proving one output alone never completes: unanimity is required.
    let partial = accepted(
        &mut engine,
        &mk(
            4,
            "n1",
            "n0",
            vec![
                hfact_on("w0", "out-a", "n0", 0, true),
                hfact_on("wA", "out-a", "n1", 100, true),
                hfact_on("wB", "out-b", "n1", 900, false),
            ],
            vec![],
            None,
        ),
    );
    assert!(
        partial.switch_settled.is_empty(),
        "one proof is not unanimous"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding holds for the outstanding output"
    );
    // Both outputs on target plus proven foreground/arrival complete.
    let arrival = vec![{
        let mut bracket = native_bracket("wA", "n1", 3, frame);
        bracket.location = ArrivalLocation::Native {
            home: Some(WorkspaceRef::Native {
                output: OutputId("out-a".to_owned()),
                workspace: crate::ids::NativeWorkspaceId::parse("n1").expect("native id"),
            }),
        };
        bracket
    }];
    let row = feedback_row(
        &effect.id,
        "wA",
        effect.issued_revision,
        true,
        Some("wA"),
        arrival,
    );
    let settled = accepted(
        &mut engine,
        &mk(
            5,
            "n1",
            "n1",
            vec![
                hfact_on("w0", "out-a", "n0", 0, true),
                hfact_on("wA", "out-a", "n1", 100, true),
                hfact_on("wB", "out-b", "n1", 900, true),
            ],
            vec![row],
            None,
        ),
    );
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "unanimous proof completes: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
}

#[test]
fn host_owned_focus_bearing_shared_nonfocus_external_move_drops_binding() {
    use crate::connection::RawSettingValue;
    use crate::world_report::{NativeObservation, NativeOccupancy};
    let caps = HostCapabilities {
        host_owned: true,
        native_sticky: true,
        maximize: true,
        ..HostCapabilities::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        caps,
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )],
    )
    .expect("generation");
    fn hfact3(window: &str, output: &str, native: &str, x: i32, visible: bool) -> WindowFact {
        let mut fact = sfact(window, output, x);
        fact.sticky = Some(false);
        fact.native_workspace = crate::world_report::NativeWorkspaceFact::Value(native.to_owned());
        fact.visibility.visible = Some(visible);
        fact
    }
    let mk3 = |seq: u64,
               cur_a: &str,
               cur_b: &str,
               windows: Vec<WindowFact>,
               feedback: Vec<FocusFeedback>,
               cmd: Option<(u64, WorldCommand)>| {
        let (s, c) = match cmd {
            Some((s, c)) => (Some(s), Some(c)),
            None => (None, None),
        };
        let mut r = report_multi(
            seq,
            windows,
            vec![],
            s,
            c,
            feedback,
            "",
            vec![screen("out-a", 0), screen("out-b", 800)],
        );
        r.native = ["out-a", "out-b"]
            .into_iter()
            .map(|o| {
                let current = if o == "out-a" { cur_a } else { cur_b };
                NativeObservation {
                    output: OutputId(o.to_owned()),
                    native_order: vec!["n0".to_owned(), "n1".to_owned(), "n2".to_owned()],
                    current_native: Some(current.to_owned()),
                    occupancy: ["n0", "n1", "n2"]
                        .into_iter()
                        .map(|n| NativeOccupancy {
                            native: n.to_owned(),
                            count: None,
                        })
                        .collect(),
                }
            })
            .collect();
        r
    };
    let boot = accepted(&mut engine, &mk3(1, "n0", "n0", vec![], vec![], None));
    let target = boot
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    let stage = || {
        vec![
            hfact3("w0", "out-a", "n0", 0, true),
            hfact3("wA", "out-a", "n1", 100, false),
            hfact3("wB", "out-b", "n1", 900, false),
        ]
    };
    let mut staged = mk3(2, "n0", "n0", stage(), vec![], None);
    staged.focused_global = "wA".to_owned();
    let _ = accepted(&mut engine, &staged);
    let mut selecting = mk3(
        3,
        "n0",
        "n0",
        stage(),
        vec![],
        Some((
            3,
            WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: target.clone(),
            },
        )),
    );
    selecting.focused_global = "wA".to_owned();
    let reply = accepted(&mut engine, &selecting);
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    let effect = reply.effects[0].clone();
    assert_eq!(effect.target.0, "wA");
    assert!(effect.await_shown, "shared focus awaits arrival");
    assert_eq!(
        engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("binding pends")
            .view_targets
            .len(),
        2,
        "every output obligated"
    );
    let frame = host_member_frames(&reply)[&WindowId("wA".to_owned())];
    // out-b externally moves to the third desktop (neither preimage nor
    // target) with no new command: the whole affected-output binding must
    // drop without a stale event.
    let moved = accepted(
        &mut engine,
        &mk3(
            4,
            "n1",
            "n2",
            vec![
                hfact3("w0", "out-a", "n0", 0, true),
                hfact3("wA", "out-a", "n1", 100, true),
                hfact3("wB", "out-b", "n1", 900, true),
            ],
            vec![],
            None,
        ),
    );
    assert!(
        moved.switch_settled.is_empty(),
        "external supersession mints no event: {moved:?}"
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "stale shared binding drops"
    );
    // Late exact-tuple feedback after the adapter returns must not mint a
    // stale terminal for the dropped binding.
    let arrival = vec![native_bracket("wA", "n1", 3, frame)];
    let row = feedback_row(
        &effect.id,
        "wA",
        effect.issued_revision,
        true,
        Some("wA"),
        arrival,
    );
    let late = accepted(
        &mut engine,
        &mk3(
            5,
            "n1",
            "n1",
            vec![
                hfact3("w0", "out-a", "n0", 0, true),
                hfact3("wA", "out-a", "n1", 100, true),
                hfact3("wB", "out-b", "n1", 900, true),
            ],
            vec![row],
            None,
        ),
    );
    assert!(
        late.switch_settled.is_empty(),
        "late feedback never completes a dropped binding: {late:?}"
    );
}

#[test]
fn host_owned_focus_bearing_shared_partial_then_external_move_drops_binding() {
    use crate::connection::RawSettingValue;
    use crate::world_report::{NativeObservation, NativeOccupancy};
    let caps = HostCapabilities {
        host_owned: true,
        native_sticky: true,
        maximize: true,
        ..HostCapabilities::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        caps,
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )],
    )
    .expect("generation");
    fn hfact3(window: &str, output: &str, native: &str, x: i32, visible: bool) -> WindowFact {
        let mut fact = sfact(window, output, x);
        fact.sticky = Some(false);
        fact.native_workspace = crate::world_report::NativeWorkspaceFact::Value(native.to_owned());
        fact.visibility.visible = Some(visible);
        fact
    }
    let mk3 = |seq: u64,
               cur_a: &str,
               cur_b: &str,
               windows: Vec<WindowFact>,
               feedback: Vec<FocusFeedback>,
               cmd: Option<(u64, WorldCommand)>| {
        let (s, c) = match cmd {
            Some((s, c)) => (Some(s), Some(c)),
            None => (None, None),
        };
        let mut r = report_multi(
            seq,
            windows,
            vec![],
            s,
            c,
            feedback,
            "",
            vec![screen("out-a", 0), screen("out-b", 800)],
        );
        r.native = ["out-a", "out-b"]
            .into_iter()
            .map(|o| {
                let current = if o == "out-a" { cur_a } else { cur_b };
                NativeObservation {
                    output: OutputId(o.to_owned()),
                    native_order: vec!["n0".to_owned(), "n1".to_owned(), "n2".to_owned()],
                    current_native: Some(current.to_owned()),
                    occupancy: ["n0", "n1", "n2"]
                        .into_iter()
                        .map(|n| NativeOccupancy {
                            native: n.to_owned(),
                            count: None,
                        })
                        .collect(),
                }
            })
            .collect();
        r
    };
    let boot = accepted(&mut engine, &mk3(1, "n0", "n0", vec![], vec![], None));
    let target = boot
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    let stage = || {
        vec![
            hfact3("w0", "out-a", "n0", 0, true),
            hfact3("wA", "out-a", "n1", 100, false),
            hfact3("wB", "out-b", "n1", 900, false),
        ]
    };
    let mut staged = mk3(2, "n0", "n0", stage(), vec![], None);
    staged.focused_global = "wA".to_owned();
    let _ = accepted(&mut engine, &staged);
    let mut selecting = mk3(
        3,
        "n0",
        "n0",
        stage(),
        vec![],
        Some((
            3,
            WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: target.clone(),
            },
        )),
    );
    selecting.focused_global = "wA".to_owned();
    let reply = accepted(&mut engine, &selecting);
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    let effect = reply.effects[0].clone();
    assert_eq!(effect.target.0, "wA");
    let frame = host_member_frames(&reply)[&WindowId("wA".to_owned())];
    // Partial proof first: out-a lands on the target while out-b still
    // echoes its preimage, with no feedback yet. The covering binding
    // pends for its outstanding output.
    let partial = accepted(
        &mut engine,
        &mk3(
            4,
            "n1",
            "n0",
            vec![
                hfact3("w0", "out-a", "n0", 0, true),
                hfact3("wA", "out-a", "n1", 100, true),
                hfact3("wB", "out-b", "n1", 900, false),
            ],
            vec![],
            None,
        ),
    );
    assert!(
        partial.switch_settled.is_empty(),
        "one proof is not unanimous: {partial:?}"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding pends for the outstanding output"
    );
    // A later genuine external move of out-b to the third desktop (neither
    // preimage nor target) must drop the covering binding without an event.
    let moved = accepted(
        &mut engine,
        &mk3(
            5,
            "n1",
            "n2",
            vec![
                hfact3("w0", "out-a", "n0", 0, true),
                hfact3("wA", "out-a", "n1", 100, true),
                hfact3("wB", "out-b", "n1", 900, true),
            ],
            vec![],
            None,
        ),
    );
    assert!(
        moved.switch_settled.is_empty(),
        "external supersession mints no event: {moved:?}"
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "late external move drops the covering binding"
    );
    // Late exact-tuple feedback must not mint a stale terminal for the
    // dropped binding.
    let arrival = vec![native_bracket("wA", "n1", 3, frame)];
    let row = feedback_row(
        &effect.id,
        "wA",
        effect.issued_revision,
        true,
        Some("wA"),
        arrival,
    );
    let late = accepted(
        &mut engine,
        &mk3(
            6,
            "n1",
            "n2",
            vec![
                hfact3("w0", "out-a", "n0", 0, true),
                hfact3("wA", "out-a", "n1", 100, true),
                hfact3("wB", "out-b", "n1", 900, true),
            ],
            vec![row],
            None,
        ),
    );
    assert!(
        late.switch_settled.is_empty(),
        "late feedback never completes a dropped binding: {late:?}"
    );
}

#[test]
fn host_owned_focus_bearing_shared_nonfocus_own_refusal_pends_then_fails() {
    use crate::connection::RawSettingValue;
    use crate::world_report::{NativeObservation, NativeOccupancy};
    let caps = HostCapabilities {
        host_owned: true,
        native_sticky: true,
        maximize: true,
        ..HostCapabilities::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        caps,
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )],
    )
    .expect("generation");
    fn hfact3(window: &str, output: &str, native: &str, x: i32, visible: bool) -> WindowFact {
        let mut fact = sfact(window, output, x);
        fact.sticky = Some(false);
        fact.native_workspace = crate::world_report::NativeWorkspaceFact::Value(native.to_owned());
        fact.visibility.visible = Some(visible);
        fact
    }
    let mk3 = |seq: u64,
               cur_a: &str,
               cur_b: &str,
               windows: Vec<WindowFact>,
               feedback: Vec<FocusFeedback>,
               cmd: Option<(u64, WorldCommand)>| {
        let (s, c) = match cmd {
            Some((s, c)) => (Some(s), Some(c)),
            None => (None, None),
        };
        let mut r = report_multi(
            seq,
            windows,
            vec![],
            s,
            c,
            feedback,
            "",
            vec![screen("out-a", 0), screen("out-b", 800)],
        );
        r.native = ["out-a", "out-b"]
            .into_iter()
            .map(|o| {
                let current = if o == "out-a" { cur_a } else { cur_b };
                NativeObservation {
                    output: OutputId(o.to_owned()),
                    native_order: vec!["n0".to_owned(), "n1".to_owned(), "n2".to_owned()],
                    current_native: Some(current.to_owned()),
                    occupancy: ["n0", "n1", "n2"]
                        .into_iter()
                        .map(|n| NativeOccupancy {
                            native: n.to_owned(),
                            count: None,
                        })
                        .collect(),
                }
            })
            .collect();
        r
    };
    let boot = accepted(&mut engine, &mk3(1, "n0", "n0", vec![], vec![], None));
    let target = boot
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    let stage = || {
        vec![
            hfact3("w0", "out-a", "n0", 0, true),
            hfact3("wA", "out-a", "n1", 100, false),
            hfact3("wB", "out-b", "n1", 900, false),
        ]
    };
    let mut staged = mk3(2, "n0", "n0", stage(), vec![], None);
    staged.focused_global = "wA".to_owned();
    let _ = accepted(&mut engine, &staged);
    let mut selecting = mk3(
        3,
        "n0",
        "n0",
        stage(),
        vec![],
        Some((
            3,
            WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: target.clone(),
            },
        )),
    );
    selecting.focused_global = "wA".to_owned();
    let reply = accepted(&mut engine, &selecting);
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    let effect = reply.effects[0].clone();
    assert_eq!(effect.target.0, "wA");
    let out_b_home = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding pends")
        .view_targets
        .get(&OutputId("out-b".to_owned()))
        .expect("every output obligated")
        .clone();
    // Own refusal on the non-focus output alone: out-a lands on the target
    // while out-b still echoes its preimage, so the covering binding pends
    // for its feedback instead of dropping as external supersession.
    let native_n0 = crate::ids::NativeWorkspaceId::parse("n0").expect("native id");
    let mut refused = mk3(
        4,
        "n1",
        "n0",
        vec![
            hfact3("w0", "out-a", "n0", 0, true),
            hfact3("wA", "out-a", "n1", 100, true),
            hfact3("wB", "out-b", "n1", 900, true),
        ],
        vec![],
        None,
    );
    refused.view_attempts = vec![crate::world_report::ViewAttempt {
        operation_id: "op-3".to_owned(),
        target_output: OutputId("out-b".to_owned()),
        target_workspace: out_b_home.workspace.0.clone(),
        setter: FrameSetter::Refused,
        pre: Some(WorkspaceRef::Native {
            output: OutputId("out-b".to_owned()),
            workspace: native_n0.clone(),
        }),
        post: Some(WorkspaceRef::Native {
            output: OutputId("out-b".to_owned()),
            workspace: native_n0.clone(),
        }),
        baseline: Some(WorkspaceRef::Native {
            output: OutputId("out-b".to_owned()),
            workspace: native_n0,
        }),
        attempt_ms: 7,
    }];
    let held = accepted(&mut engine, &refused);
    assert!(
        held.switch_settled.is_empty(),
        "own refusal pends without feedback: {held:?}"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "covering binding survives its own non-focus refusal"
    );
    // Exact focus feedback with the latched refusal still failing the
    // native obligation: one terminal, focus proven, outcome failed.
    let frame = host_member_frames(&reply)[&WindowId("wA".to_owned())];
    let row = feedback_row(
        &effect.id,
        "wA",
        effect.issued_revision,
        true,
        Some("wA"),
        vec![native_bracket("wA", "n1", 3, frame)],
    );
    let mut proved = mk3(
        5,
        "n1",
        "n0",
        vec![
            hfact3("w0", "out-a", "n0", 0, true),
            hfact3("wA", "out-a", "n1", 100, true),
            hfact3("wB", "out-b", "n1", 900, true),
        ],
        vec![row],
        None,
    );
    proved.view_attempts = vec![];
    let settled = accepted(&mut engine, &proved);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "refusal plus feedback settles once: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.focus, SwitchFocus::Proven);
}

#[test]
fn host_owned_focus_bearing_late_admission_never_joins_frozen() {
    let (mut engine, _away, grant) = host_focus_grant();
    let frozen: Vec<WindowId> = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding pends")
        .members
        .iter()
        .map(|member| member.window.clone())
        .collect();
    assert_eq!(frozen.len(), 2, "two members freeze: {frozen:?}");
    // A newcomer homed to the selected home after the grant registers no
    // authority and never joins the frozen arrival set.
    let mut late = host_proof_windows(true);
    late.push(hfact("w3", "n1", 300, false));
    let quiet = accepted(&mut engine, &host_native_report(4, late, "n0", None, None));
    assert!(
        quiet.switch_settled.is_empty(),
        "no verdict without proof: {quiet:?}"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding holds");
    assert_eq!(
        binding
            .members
            .iter()
            .map(|member| member.window.clone())
            .collect::<Vec<_>>(),
        frozen,
        "frozen set untouched by late admission"
    );
    assert!(
        engine.selection_targets.is_empty(),
        "host-owned admits no per-window targets"
    );
    let _ = grant;
}

#[test]
fn host_owned_focus_bearing_supersession_drops_without_event() {
    let (mut engine, _away, grant) = host_focus_grant();
    let home = grant
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0-backed workspace")
        .id
        .clone();
    // A superseding selection back to the home workspace drops the pending
    // grant without minting an event for the abandoned parent.
    let reply = host_select_windows(&mut engine, 4, &home, host_proof_windows(true), "n0", "w0");
    assert!(
        reply.switch_settled.is_empty(),
        "supersession mints no event: {reply:?}"
    );
    assert!(
        engine
            .retained_selection
            .values()
            .all(|binding| binding.parent_op != "op-3"),
        "abandoned grant releases"
    );
}

#[test]
fn host_owned_focus_bearing_open_binding_never_wedges_repeated_populated_switches() {
    // KDE v4 wedge guard on the real v4 admission path: the lane always sends
    // `focusFeedback: []`, and each report carries the previous selection's
    // fresh native echo plus its native-tagged attempt.
    let host_v4 = |seq: u64,
                   windows: Vec<WindowFact>,
                   current: &str,
                   command_seq: Option<u64>,
                   command: Option<WorldCommand>,
                   focused: &str,
                   view_attempts: Vec<crate::world_report::ViewAttempt>|
     -> crate::world_report::WorldReportV4 {
        crate::world_report::WorldReportV4 {
            contract_version: crate::world_report::WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq: seq,
            request_seq: seq,
            request_id: format!("req-{seq}"),
            command_seq,
            command,
            screens: vec![screen("out-a", 0)],
            held_screens: Vec::new(),
            windows,
            held_windows: Vec::new(),
            closed: Vec::new(),
            native: vec![crate::world_report::NativeObservation {
                output: OutputId("out-a".to_owned()),
                native_order: vec!["n0".to_owned(), "n1".to_owned()],
                current_native: Some(current.to_owned()),
                occupancy: vec![
                    crate::world_report::NativeOccupancy {
                        native: "n0".to_owned(),
                        count: None,
                    },
                    crate::world_report::NativeOccupancy {
                        native: "n1".to_owned(),
                        count: None,
                    },
                ],
            }],
            held_native: Vec::new(),
            cursor: None,
            cursor_hit: None,
            focused_global: focused.to_owned(),
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
            view_attempts,
            clear_attempts: Vec::new(),
            creation_attempts: Vec::new(),
            held_attempts: Vec::new(),
            state_attempts: Vec::new(),
            focus_feedback: Vec::new(),
            presentation: crate::world_report::PresentationRowV4::Valid(
                crate::world_report::PresentationIngressV4::default(),
            ),
            intent_records: Vec::new(),
            renderer_input: None,
        }
    };
    let accepted_v4 = |engine: &mut WorldEngine,
                       report: &crate::world_report::WorldReportV4|
     -> crate::world_report::WorldReplyV4 {
        match engine.handle_report_v4_for_test(report) {
            Ok(reply) => reply,
            Err((kind, message)) => panic!("expected accepted v4, got {kind}: {message}"),
        }
    };
    // Native-tagged attempt: logical target with native pre/post/baseline.
    let attempt_for =
        |op: &str, target: &str, pre: &str, post: &str| crate::world_report::ViewAttempt {
            operation_id: op.to_owned(),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: target.to_owned(),
            setter: FrameSetter::Accepted,
            pre: Some(native_ref(pre)),
            post: Some(native_ref(post)),
            baseline: Some(native_ref(pre)),
            attempt_ms: 7,
        };
    let staged = || host_proof_windows(true);
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted_v4(
        &mut engine,
        &host_v4(1, vec![], "n0", None, None, "", vec![]),
    );
    assert_eq!(
        boot.contract_version,
        crate::world_report::WORLD_CONTRACT_VERSION_V4
    );
    let away = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    let home = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0-backed workspace")
        .id
        .clone();
    let _ = accepted_v4(
        &mut engine,
        &host_v4(2, staged(), "n0", None, None, "w2", vec![]),
    );
    let grant = accepted_v4(
        &mut engine,
        &host_v4(
            3,
            staged(),
            "n0",
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            "w2",
            vec![],
        ),
    );
    assert_eq!(grant.effects.len(), 1);
    assert!(grant.effects[0].await_shown);
    assert_eq!(grant.effects[0].command_seq, Some(3));
    assert!(grant.switch_settled.is_empty());
    assert!(!grant.command_consumed);
    assert!(grant.command_result.is_none());
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("selection binding retained");
    assert!(binding.effect.is_some());
    assert!(!binding.members.is_empty());
    assert!(binding.switch_members.is_empty());
    assert_eq!(binding.view_targets.len(), 1);
    // Four populated switches: each carries the previous selection's echo
    // plus its native-tagged attempt, then supersedes it silently.
    let mut prev_native = "n0".to_owned();
    let mut prev_target_native = "n1".to_owned();
    let mut prev_op = "op-3".to_owned();
    let mut prev_logical = away.clone();
    for i in 0..4_u64 {
        let seq = 4 + i;
        let (target, focus, target_native) = if i % 2 == 0 {
            (home.clone(), "w0", "n0".to_owned())
        } else {
            (away.clone(), "w2", "n1".to_owned())
        };
        let reply = accepted_v4(
            &mut engine,
            &host_v4(
                seq,
                staged(),
                &prev_target_native,
                Some(seq),
                Some(WorldCommand::SelectWorkspace {
                    output: OutputId("out-a".to_owned()),
                    workspace: target.clone(),
                }),
                focus,
                vec![attempt_for(
                    &prev_op,
                    &prev_logical,
                    &prev_native,
                    &prev_target_native,
                )],
            ),
        );
        assert_eq!(
            reply.contract_version,
            crate::world_report::WORLD_CONTRACT_VERSION_V4
        );
        assert!(reply.switch_settled.is_empty());
        assert_eq!(reply.effects.len(), 1);
        assert!(reply.effects[0].await_shown);
        assert_eq!(reply.effects[0].command_seq, Some(seq));
        assert!(!reply.command_consumed);
        assert!(reply.command_result.is_none());
        assert_eq!(engine.retained_selection.len(), 1);
        let binding = engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("open binding retained");
        assert_eq!(binding.parent_op, format!("op-{seq}"));
        assert!(binding.effect.is_some());
        assert!(!binding.members.is_empty());
        assert!(binding.switch_members.is_empty());
        assert!(binding.switch_evidence.is_empty());
        assert_eq!(binding.view_targets.len(), 1);
        assert!(binding.view_raw.is_empty());
        assert!(binding.feedback.is_none());
        assert_eq!(engine.selection_scopes.len(), 1);
        assert!(engine.selection_targets.is_empty());
        let views = engine
            .retained
            .iter()
            .filter(|r| {
                matches!(r.target.kind, crate::world_report::TransferKind::View)
                    && r.target.target_output == OutputId("out-a".to_owned())
            })
            .count();
        assert_eq!(views, 1);
        assert!(
            reply
                .transfers
                .iter()
                .any(|t| t.operation_id == format!("op-{seq}"))
        );
        assert!(!reply.windows.is_empty());
        for window in ["w0", "w1", "w2"] {
            let row = reply
                .windows
                .iter()
                .find(|row| row.lifetime.0 == window)
                .unwrap_or_else(|| panic!("missing managed row {window} (iter {i})"));
            assert!(row.effective_frame.is_some());
        }
        prev_native = prev_target_native;
        prev_target_native = target_native;
        prev_op = format!("op-{seq}");
        prev_logical = target;
    }
    // Fresh observation holds the last open binding; a real orientation
    // mutation behind it must replan frames without touching selection.
    let frame_in = |reply: &crate::world_report::WorldReplyV4, window: &str| {
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .unwrap_or_else(|| panic!("missing managed row {window}"))
            .effective_frame
    };
    let quiet = accepted_v4(
        &mut engine,
        &host_v4(100, staged(), "n1", None, None, "w2", vec![]),
    );
    assert!(quiet.switch_settled.is_empty());
    assert!(
        engine
            .retained_selection
            .values()
            .any(|binding| binding.parent_op == "op-7")
    );
    let (before1, before2) = (frame_in(&quiet, "w1"), frame_in(&quiet, "w2"));
    let other = accepted_v4(
        &mut engine,
        &host_v4(
            101,
            staged(),
            "n1",
            Some(101),
            Some(WorldCommand::Operation(
                crate::boundary::CoreCommand::ToggleOrientation {
                    window: "w2".to_owned(),
                },
            )),
            "w2",
            vec![],
        ),
    );
    assert!(other.switch_settled.is_empty());
    assert!(!other.command_consumed);
    assert!(other.command_result.is_none());
    assert!(other.effects.is_empty());
    assert!(
        engine
            .retained_selection
            .values()
            .any(|binding| binding.parent_op == "op-7")
    );
    assert_eq!(engine.retained_selection.len(), 1);
    assert_eq!(engine.selection_scopes.len(), 1);
    let (after1, after2) = (frame_in(&other, "w1"), frame_in(&other, "w2"));
    assert!((before1, before2) != (after1, after2));
    for (window, frame) in [
        ("w0", frame_in(&other, "w0")),
        ("w1", after1),
        ("w2", after2),
    ] {
        assert!(frame.is_some(), "frame planned for {window}");
    }
}

#[test]
fn host_owned_focus_bearing_close_drops_without_event() {
    let (mut engine, _away, _grant) = host_focus_grant();
    // Explicit close of the focus target drops the finite grant without an
    // event; surviving scopes persist independently.
    let mut closed = host_native_report(
        4,
        vec![hfact("w0", "n0", 0, true), hfact("w1", "n1", 100, true)],
        "n0",
        None,
        None,
    );
    closed.closed = vec![WindowId("w2".to_owned())];
    let quiet = accepted(&mut engine, &closed);
    assert!(
        quiet.switch_settled.is_empty(),
        "close drops without event: {quiet:?}"
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite grant drops"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "surviving scope persists"
    );
}

fn view_attempt(op: &str, target: &str, setter: FrameSetter) -> crate::world_report::ViewAttempt {
    crate::world_report::ViewAttempt {
        operation_id: op.to_owned(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: target.to_owned(),
        setter,
        pre: Some(native_ref("n0")),
        post: Some(native_ref("n0")),
        baseline: Some(native_ref("n0")),
        attempt_ms: 7,
    }
}

#[test]
fn host_owned_focusless_native_echo_completes_unproven() {
    let (mut engine, away) = host_boot();
    let reply = host_select(&mut engine, 2, &away);
    assert!(reply.effects.is_empty(), "no focus invented: {reply:?}");
    assert!(reply.switch_settled.is_empty(), "native proof outstanding");
    assert!(
        engine
            .retained
            .iter()
            .any(|r| r.target.operation_id == "op-2"),
        "actual view parent flight retained"
    );
    let settled = accepted(
        &mut engine,
        &host_native_report(3, vec![], "n1", None, None),
    );
    assert_eq!(settled.switch_settled.len(), 1, "echo proves: {settled:?}");
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| r.target.operation_id != "op-2"),
        "actual view parent retires"
    );
}

#[test]
fn host_owned_focusless_minimized_native_member_takes_no_window_target() {
    let (mut engine, away) = host_boot();
    // An actual native-owned minimized survivor on the target: owned plus
    // minimized plus a native readback naming the target desktop.
    let mut survivor = sfact("wmin", "out-a", 0);
    survivor.owned = Some(true);
    survivor.minimized = Some(true);
    survivor.rect = None;
    survivor.native_workspace = crate::world_report::NativeWorkspaceFact::Value("n1".to_owned());
    let reply = accepted(
        &mut engine,
        &host_native_report(
            2,
            vec![survivor],
            "n0",
            Some(2),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "minimized-only invents no focus: {reply:?}"
    );
    assert!(reply.switch_settled.is_empty(), "native proof outstanding");
    for row in &reply.windows {
        assert!(
            row.visibility.is_none(),
            "host-owned grants no per-window targets: {row:?}"
        );
    }
    assert!(engine.selection_targets.is_empty(), "no per-window targets");
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("host-owned focusless binding pends");
    assert!(binding.effect.is_none());
    assert!(
        binding.switch_members.is_empty(),
        "native-owned member takes no LastWrite demand: {binding:?}"
    );
    assert_eq!(
        binding
            .view_targets
            .get(&OutputId("out-a".to_owned()))
            .map(|home| home.workspace.0.clone()),
        Some(away.clone()),
        "native obligation freezes the selected home: {binding:?}"
    );
    // The native echo still proves the selection once the selector lands.
    let mut survivor = sfact("wmin", "out-a", 0);
    survivor.owned = Some(true);
    survivor.minimized = Some(true);
    survivor.rect = None;
    survivor.native_workspace = crate::world_report::NativeWorkspaceFact::Value("n1".to_owned());
    let settled = accepted(
        &mut engine,
        &host_native_report(3, vec![survivor], "n1", None, None),
    );
    assert_eq!(settled.switch_settled.len(), 1, "echo proves: {settled:?}");
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
}

#[test]
fn host_owned_focusless_refused_attempt_fails_unproven() {
    let (mut engine, away) = host_boot();
    let reply = host_select(&mut engine, 2, &away);
    assert!(reply.switch_settled.is_empty(), "native proof outstanding");
    let mut refused = host_native_report(3, vec![], "n0", None, None);
    refused.view_attempts = vec![view_attempt("op-2", &away, FrameSetter::Refused)];
    let settled = accepted(&mut engine, &refused);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "proven refusal fails: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert!(event.failed.is_empty(), "no per-window failures: {event:?}");
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases on refusal"
    );
}

#[test]
fn host_owned_focusless_held_refusal_recovers_from_latched_raw() {
    let (mut engine, away) = host_boot();
    let reply = host_select(&mut engine, 2, &away);
    assert!(reply.switch_settled.is_empty(), "native proof outstanding");
    // Held report: the adapter consumed the refusal on an unreadable
    // screen, so the gate cannot decide yet, but the exact raw latches.
    let mut held = host_native_report(3, vec![], "n0", None, None);
    held.screens = vec![{
        let mut s = screen("out-a", 0);
        s.complete = false;
        s
    }];
    held.view_attempts = vec![view_attempt("op-2", &away, FrameSetter::Refused)];
    let stalled = accepted(&mut engine, &held);
    assert!(
        stalled.switch_settled.is_empty(),
        "held refusal stays pending"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding holds across the gap"
    );
    // Recovery omits the consumed attempt: the latched raw plus the
    // readable echo naming the refused preimage still fails the terminal.
    let settled = accepted(
        &mut engine,
        &host_native_report(4, vec![], "n0", None, None),
    );
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "latched refusal fails: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
}

#[test]
fn host_owned_focusless_wrong_tuples_and_logical_echo_hold() {
    use crate::world_report::ScreenView;
    let (mut engine, away) = host_boot();
    let reply = host_select(&mut engine, 2, &away);
    assert!(reply.switch_settled.is_empty(), "native proof outstanding");
    // Wrong operation id, wrong target workspace, and a logical echo
    // naming the target while the native current stays home: none of
    // these prove anything for a host-owned selector.
    let mut noisy = host_native_report(3, vec![], "n0", None, None);
    noisy.views = vec![ScreenView {
        output: OutputId("out-a".to_owned()),
        view: away.clone(),
    }];
    noisy.view_attempts = vec![
        view_attempt("op-9", &away, FrameSetter::Refused),
        crate::world_report::ViewAttempt {
            target_workspace: "ws-1".to_owned(),
            ..view_attempt("op-2", &away, FrameSetter::Refused)
        },
    ];
    let stalled = accepted(&mut engine, &noisy);
    assert!(
        stalled.switch_settled.is_empty(),
        "forged rows and logical echoes hold"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding holds");
    assert!(
        binding.view_raw.is_empty(),
        "wrong tuples never latch: {binding:?}"
    );
}

#[test]
fn host_owned_focusless_shared_needs_every_output_proof() {
    use crate::connection::RawSettingValue;
    use crate::world_report::{NativeObservation, NativeOccupancy};
    let caps = HostCapabilities {
        host_owned: true,
        native_sticky: true,
        maximize: true,
        ..HostCapabilities::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        caps,
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )],
    )
    .expect("generation");
    let mk = |seq: u64, cur_a: &str, cur_b: &str, cmd: Option<(u64, WorldCommand)>| {
        let (s, c) = match cmd {
            Some((s, c)) => (Some(s), Some(c)),
            None => (None, None),
        };
        let mut r = report_multi(
            seq,
            vec![],
            vec![],
            s,
            c,
            vec![],
            "",
            vec![screen("out-a", 0), screen("out-b", 800)],
        );
        r.native = ["out-a", "out-b"]
            .into_iter()
            .map(|o| {
                let (n0, n1) = (format!("{o}-n0"), format!("{o}-n1"));
                NativeObservation {
                    output: OutputId(o.to_owned()),
                    native_order: vec![n0.clone(), n1.clone()],
                    current_native: Some(if o == "out-a" {
                        cur_a.to_owned()
                    } else {
                        cur_b.to_owned()
                    }),
                    occupancy: vec![n0, n1]
                        .into_iter()
                        .map(|n| NativeOccupancy {
                            native: n,
                            count: None,
                        })
                        .collect(),
                }
            })
            .collect();
        r
    };
    let boot = accepted(&mut engine, &mk(1, "out-a-n0", "out-b-n0", None));
    let target = boot
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some("out-a-n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    // Shared fanout obligates every live output; the reply carries the
    // pending binding with per-output native obligations.
    let reply = accepted(
        &mut engine,
        &mk(
            2,
            "out-a-n0",
            "out-b-n0",
            Some((
                2,
                WorldCommand::SelectWorkspace {
                    output: OutputId("out-a".to_owned()),
                    workspace: target.clone(),
                },
            )),
        ),
    );
    assert!(reply.effects.is_empty(), "no focus invented: {reply:?}");
    assert!(reply.switch_settled.is_empty(), "proofs outstanding");
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("shared focusless binding pends");
    assert!(binding.effect.is_none());
    assert_eq!(
        binding.view_targets.len(),
        2,
        "every output obligated: {binding:?}"
    );
    // Proving one output alone never completes: unanimity is required.
    let partial = accepted(&mut engine, &mk(3, "out-a-n1", "out-b-n0", None));
    assert!(
        partial.switch_settled.is_empty(),
        "one proof is not unanimous"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding holds for the outstanding output"
    );
}

#[test]
fn host_owned_focusless_first_latched_raw_wins_over_later_row() {
    let (mut engine, away) = host_boot();
    let reply = host_select(&mut engine, 2, &away);
    assert!(reply.switch_settled.is_empty(), "native proof outstanding");
    // First exact-tuple row latches: an accepted attempt whose post still
    // names the preimage, so the write has not landed and the gate holds.
    let mut unlanded = host_native_report(3, vec![], "n0", None, None);
    unlanded.view_attempts = vec![crate::world_report::ViewAttempt {
        operation_id: "op-2".to_owned(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(native_ref("n0")),
        post: Some(native_ref("n0")),
        baseline: Some(native_ref("n0")),
        attempt_ms: 7,
    }];
    let stalled = accepted(&mut engine, &unlanded);
    assert!(
        stalled.switch_settled.is_empty(),
        "unlanded write holds: {stalled:?}"
    );
    // A later row claiming the target landed must not upgrade the first
    // latched receipt; only a genuine current-target echo proves. The
    // current is unknown here so no independent current-target fact
    // exists: the fresh row is the only new signal, and first-evidence
    // discipline keeps the original latch.
    let mut upgraded = host_native_report(4, vec![], "n0", None, None);
    upgraded.native[0].current_native = None;
    upgraded.view_attempts = vec![crate::world_report::ViewAttempt {
        operation_id: "op-2".to_owned(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: away.clone(),
        setter: FrameSetter::Accepted,
        pre: Some(native_ref("n0")),
        post: Some(native_ref("n1")),
        baseline: Some(native_ref("n0")),
        attempt_ms: 9,
    }];
    let still = accepted(&mut engine, &upgraded);
    assert!(
        still.switch_settled.is_empty(),
        "later row never upgrades the first latch: {still:?}"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding holds for genuine proof"
    );
    // The independent current-target echo still proves on its own facts.
    let settled = accepted(
        &mut engine,
        &host_native_report(5, vec![], "n1", None, None),
    );
    assert_eq!(settled.switch_settled.len(), 1, "echo proves: {settled:?}");
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
}

#[test]
fn host_owned_focusless_shared_same_ids_complete_unproven() {
    use crate::connection::RawSettingValue;
    use crate::world_report::{NativeObservation, NativeOccupancy};
    let caps = HostCapabilities {
        host_owned: true,
        native_sticky: true,
        maximize: true,
        ..HostCapabilities::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        caps,
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )],
    )
    .expect("generation");
    // Same native ids on both outputs bind to the same logical backing:
    // one unified ring, not per-output fakes.
    let mk = |seq: u64, cur_a: &str, cur_b: &str, cmd: Option<(u64, WorldCommand)>| {
        let (s, c) = match cmd {
            Some((s, c)) => (Some(s), Some(c)),
            None => (None, None),
        };
        let mut r = report_multi(
            seq,
            vec![],
            vec![],
            s,
            c,
            vec![],
            "",
            vec![screen("out-a", 0), screen("out-b", 800)],
        );
        r.native = ["out-a", "out-b"]
            .into_iter()
            .map(|o| {
                let current = if o == "out-a" { cur_a } else { cur_b };
                NativeObservation {
                    output: OutputId(o.to_owned()),
                    native_order: vec!["n0".to_owned(), "n1".to_owned()],
                    current_native: Some(current.to_owned()),
                    occupancy: ["n0", "n1"]
                        .into_iter()
                        .map(|n| NativeOccupancy {
                            native: n.to_owned(),
                            count: None,
                        })
                        .collect(),
                }
            })
            .collect();
        r
    };
    let boot = accepted(&mut engine, &mk(1, "n0", "n0", None));
    let target = boot
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    assert!(
        boot.workspaces.iter().any(|row| row.output.0 == "out-b"
            && row.id == target
            && row.native_binding.as_deref() == Some("n1")),
        "same ids share one logical backing: {boot:?}"
    );
    // Shared fanout obligates every live output; the reply carries the
    // pending binding with per-output native obligations.
    let reply = accepted(
        &mut engine,
        &mk(
            2,
            "n0",
            "n0",
            Some((
                2,
                WorldCommand::SelectWorkspace {
                    output: OutputId("out-a".to_owned()),
                    workspace: target.clone(),
                },
            )),
        ),
    );
    assert!(reply.effects.is_empty(), "no focus invented: {reply:?}");
    assert!(reply.switch_settled.is_empty(), "proofs outstanding");
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("shared focusless binding pends");
    assert!(binding.effect.is_none());
    assert_eq!(
        binding.view_targets.len(),
        2,
        "every output obligated: {binding:?}"
    );
    // Proving one output alone never completes: unanimity is required.
    let partial = accepted(&mut engine, &mk(3, "n1", "n0", None));
    assert!(
        partial.switch_settled.is_empty(),
        "one proof is not unanimous"
    );
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding holds for the outstanding output"
    );
    // Both outputs on the same target prove together: one Complete
    // Unproven terminal on the existing parent, and the parent retires.
    let settled = accepted(&mut engine, &mk(4, "n1", "n1", None));
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "unanimous proof completes: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(!event.fallback);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| r.target.operation_id != "op-2"),
        "actual parent retires"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive"
    );
}

#[test]
fn host_owned_focusless_external_move_supersedes_without_event() {
    use crate::world_report::{NativeObservation, NativeOccupancy};
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let mk = |seq: u64, current: &str, cmd: Option<(u64, WorldCommand)>| {
        let (s, c) = match cmd {
            Some((s, c)) => (Some(s), Some(c)),
            None => (None, None),
        };
        let mut r = report(seq, vec![], vec![], s, c, vec![], "");
        r.native = vec![NativeObservation {
            output: OutputId("out-a".to_owned()),
            native_order: vec!["n0".to_owned(), "n1".to_owned(), "n2".to_owned()],
            current_native: Some(current.to_owned()),
            occupancy: ["n0", "n1", "n2"]
                .into_iter()
                .map(|n| NativeOccupancy {
                    native: n.to_owned(),
                    count: None,
                })
                .collect(),
        }];
        r
    };
    let boot = accepted(&mut engine, &mk(1, "n0", None));
    let target = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n1"))
        .expect("n1-backed workspace")
        .id
        .clone();
    let other = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n2"))
        .expect("n2-backed workspace")
        .id
        .clone();
    // Attempt a selection of the n1-backed workspace: the binding pends.
    let reply = accepted(
        &mut engine,
        &mk(
            2,
            "n0",
            Some((
                2,
                WorldCommand::SelectWorkspace {
                    output: OutputId("out-a".to_owned()),
                    workspace: target.clone(),
                },
            )),
        ),
    );
    assert!(reply.effects.is_empty(), "no focus invented: {reply:?}");
    assert!(reply.switch_settled.is_empty(), "native proof outstanding");
    assert!(
        engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding pends"
    );
    // Genuine external move to n2 with no command: the attempted flight
    // settles to observed, the reconcile supersedes the binding, and no
    // event mints.
    let moved = accepted(&mut engine, &mk(3, "n2", None));
    assert!(moved.switch_settled.is_empty(), "no stale event: {moved:?}");
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding superseded"
    );
    assert!(
        !engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "no stale scope"
    );
    assert!(engine.selection_targets.is_empty(), "no stale authority");
    assert_eq!(
        engine
            .effective_view(&OutputId("out-a".to_owned()))
            .as_deref(),
        Some(other.as_str()),
        "views follow adapter truth"
    );
    // No hang: the next observation stays quiet and a fresh selection to
    // the externally selected workspace still settles.
    let quiet = accepted(&mut engine, &mk(4, "n2", None));
    assert!(quiet.switch_settled.is_empty(), "no hang event: {quiet:?}");
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "no hang binding"
    );
    let retry = accepted(
        &mut engine,
        &mk(
            5,
            "n2",
            Some((
                5,
                WorldCommand::SelectWorkspace {
                    output: OutputId("out-a".to_owned()),
                    workspace: target.clone(),
                },
            )),
        ),
    );
    assert!(retry.effects.is_empty(), "no focus invented: {retry:?}");
    assert!(
        retry.switch_settled.is_empty(),
        "new binding pends proof: {retry:?}"
    );
    let done = accepted(&mut engine, &mk(6, "n1", None));
    assert_eq!(done.switch_settled.len(), 1, "fresh lane settles: {done:?}");
    let event = &done.switch_settled[0];
    assert_eq!(event.operation_id, "op-5");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert_eq!(event.focus, SwitchFocus::Unproven);
}

#[test]
fn host_owned_focusless_refused_command_keeps_staged_terminal() {
    use crate::world_report::CommandResult;
    let (mut engine, away) = host_boot();
    let reply = host_select(&mut engine, 2, &away);
    assert!(reply.switch_settled.is_empty(), "native proof outstanding");
    // A refused selection command on the proof report: the pre-command
    // image keeps the staged terminal for the pending binding, while the
    // refused command mints nothing itself.
    let settled = accepted(
        &mut engine,
        &host_native_report(
            3,
            vec![],
            "n1",
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-nope".to_owned(),
            }),
        ),
    );
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "staged terminal survives refusal: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(!event.fallback);
    assert!(
        matches!(settled.command_result, Some(CommandResult::Rejected { .. })),
        "refusal reported: {settled:?}"
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "binding releases"
    );
    assert!(
        engine
            .retained_selection
            .values()
            .all(|binding| binding.parent_op != "op-3"),
        "refused command retains nothing"
    );
}

#[test]
fn focusless_issuing_terminal_duplicate_replays_cached_then_empties() {
    let mut engine = engine();
    let mut w1 = sfact("w1", "out-a", 0);
    w1.minimized = Some(true);
    w1.rect = None;
    let mut w2 = sfact("w2", "out-a", 100);
    w2.minimized = Some(true);
    w2.rect = None;
    let boot = accepted(
        &mut engine,
        &report(
            1,
            vec![w1.clone(), w2.clone()],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let ring = ring_ids(&boot, "out-a");
    assert!(ring.len() >= 2);
    let away = ring[1].clone();
    let issuing = report(
        2,
        vec![w1, w2],
        vec![],
        Some(2),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: away.clone(),
        }),
        vec![],
        "",
    );
    let reply = accepted(&mut engine, &issuing);
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "zero members complete at issuing: {reply:?}"
    );
    let wire = crate::world_report::encode_reply_canonical(&reply);
    match engine.handle_report_legacy(&issuing) {
        LegacyWorldOutcome::Duplicate(cached) => {
            assert_eq!(cached.switch_settled, reply.switch_settled);
            assert_eq!(
                crate::world_report::encode_reply_canonical(&cached),
                wire,
                "duplicate returns the byte-identical cached event"
            );
        }
        other => panic!("expected cached duplicate, got {other:?}"),
    }
    let again = accepted(
        &mut engine,
        &report(
            3,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        again.switch_settled.is_empty(),
        "next report publishes empty"
    );
}

#[test]
fn focusless_issuing_terminal_consumed_replay_reconciles_empty() {
    let mut engine = engine();
    let mut w1 = sfact("w1", "out-a", 0);
    w1.minimized = Some(true);
    w1.rect = None;
    let mut w2 = sfact("w2", "out-a", 100);
    w2.minimized = Some(true);
    w2.rect = None;
    let boot = accepted(
        &mut engine,
        &report(
            1,
            vec![w1.clone(), w2.clone()],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    let ring = ring_ids(&boot, "out-a");
    assert!(ring.len() >= 2);
    let away = ring[1].clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![w1.clone(), w2.clone()],
            vec![],
            Some(2),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "zero members complete at issuing: {reply:?}"
    );
    // The consumed seq reconciles only: no revived terminal, no re-execution.
    match engine.handle_report_legacy(&report(
        3,
        vec![w1, w2],
        vec![],
        Some(2),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: away.clone(),
        }),
        vec![],
        "",
    )) {
        LegacyWorldOutcome::CommandConsumed(consumed) => {
            assert!(
                consumed.switch_settled.is_empty(),
                "no revived terminal: {consumed:?}"
            );
            assert!(
                consumed.command_consumed,
                "replay marks consumed: {consumed:?}"
            );
        }
        other => panic!("expected consumed replay, got {other:?}"),
    }
}

#[test]
fn switch_issuing_reserve_covers_zero_event_beside_settling_binding() {
    use crate::world_report::{SwitchEvent, SwitchFocus, SwitchOutcome};
    use crate::world_sizing::{MAX_SWITCH_EVENT_FIXED_BYTES, MAX_SWITCH_FAILED_ROW_BYTES};
    use crate::world_sizing::{max_switch_settle_reserve, switch_settled_estimate};
    let (mut engine, _home, away) = select_setup();
    let _granted = select(&mut engine, 3, &away);
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("pending binding");
    assert!(!binding.switch_members.is_empty(), "members to settle");
    let member_count = binding.switch_members.len();
    // A select-family command on the same report can additionally mint the
    // zero-obligation issuing event while the old binding settles.
    let issuing = report(
        4,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(4),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: away.clone(),
        }),
        vec![],
        "",
    );
    assert_eq!(
        engine.switch_issuing_reserve(issuing.command.as_ref()),
        max_switch_settle_reserve(1, 0),
        "at most one issuing event"
    );
    let switching = report(
        4,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(4),
        Some(WorldCommand::SwitchView {
            output: OutputId("out-a".to_owned()),
            workspace: away.clone(),
        }),
        vec![],
        "",
    );
    assert_eq!(
        engine.switch_issuing_reserve(switching.command.as_ref()),
        max_switch_settle_reserve(1, 0),
        "switch view is select-family"
    );
    let numbered = report(
        4,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(4),
        Some(WorldCommand::SelectNumbered {
            output: Some(OutputId("out-a".to_owned())),
            index: 1,
        }),
        vec![],
        "",
    );
    assert_eq!(
        engine.switch_issuing_reserve(numbered.command.as_ref()),
        max_switch_settle_reserve(1, 0),
        "numbered select is select-family"
    );
    let observation = report(
        4,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        None,
        None,
        vec![],
        "",
    );
    assert_eq!(
        engine.switch_issuing_reserve(observation.command.as_ref()),
        0,
        "observation-only reserves nothing"
    );
    let focus = report(
        4,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        Some(4),
        Some(WorldCommand::Focus {
            window: WindowId("w1".to_owned()),
            direction: "left".to_owned(),
        }),
        vec![],
        "",
    );
    assert_eq!(
        engine.switch_issuing_reserve(focus.command.as_ref()),
        0,
        "non-select commands reserve nothing"
    );
    // Worst case on one report: the retained binding's full-failure
    // terminal plus one worst-token empty issuing event.
    let old = SwitchEvent {
        operation_id: format!("op-{}", u64::MAX),
        outcome: SwitchOutcome::CompletedWithFailures,
        failed: (0..member_count)
            .map(|_| WindowId("w".repeat(128)))
            .collect(),
        focus: SwitchFocus::Refused,
        fallback: true,
    };
    let new = SwitchEvent {
        operation_id: format!("op-{}", u64::MAX),
        outcome: SwitchOutcome::CompletedWithFailures,
        failed: Vec::new(),
        focus: SwitchFocus::Unproven,
        fallback: false,
    };
    let estimated = switch_settled_estimate(&[old, new]);
    let reserve =
        engine.switch_settle_reserve() + engine.switch_issuing_reserve(issuing.command.as_ref());
    assert!(
        reserve >= estimated,
        "no undercount: reserve {reserve} covers estimate {estimated}"
    );
    assert_eq!(MAX_SWITCH_EVENT_FIXED_BYTES, 117, "fixed event bound stays");
    assert_eq!(MAX_SWITCH_FAILED_ROW_BYTES, 131, "failed id bound stays");
}

#[test]
fn host_owned_focus_bearing_fullscreen_focus_freezes_frameless_and_proves() {
    // Intentional fullscreen focus: the native overlay owns its pixels,
    // so the published row is frameless, and the off-desktop
    // `visible=false` pre-arrival fact never excludes it.
    let (mut engine, away) = host_boot();
    let mut w2 = hfact("w2", "n1", 200, false);
    w2.fullscreen = Some(true);
    let stage = || {
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            w2.clone(),
        ]
    };
    let mut staged = host_native_report(2, stage(), "n0", None, None);
    staged.focused_global = "w2".to_owned();
    let _ = accepted(&mut engine, &staged);
    let reply = host_select_windows(&mut engine, 3, &away, stage(), "n0", "w2");
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    assert_eq!(reply.effects[0].target.0, "w2");
    assert!(
        reply.effects[0].await_shown,
        "fullscreen focus awaits arrival"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding pends");
    let frozen = binding
        .members
        .iter()
        .find(|member| member.window.0 == "w2")
        .expect("frameless fullscreen focus freezes: {binding:?}");
    assert_eq!(frozen.frame, None);
    assert_eq!(frozen.home.workspace.0, away);
    // Any readable live frame proves: null desired waives equality only.
    let effect = reply.effects[0].clone();
    let frames = host_member_frames(&reply);
    let arrival = vec![
        native_bracket("w1", "n1", 3, frames[&WindowId("w1".to_owned())]),
        native_bracket("w2", "n1", 3, Some(rect(200, 0, 40, 90))),
    ];
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut wins = host_proof_windows(true);
    wins[2].fullscreen = Some(true);
    let mut proof = host_native_report(4, wins, "n1", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "fullscreen arrival proves: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert_eq!(event.focus, SwitchFocus::Proven);
}

#[test]
fn host_owned_focus_bearing_float_focus_freezes_frameless_and_proves() {
    // Intentional float focus: a fresh float carries no retained frame,
    // so its published row is frameless, and the off-desktop
    // `visible=false` pre-arrival fact never excludes it.
    let (mut engine, away) = host_boot();
    let mut w2 = hfact("w2", "n1", 200, false);
    w2.transient = Some(true);
    let stage = || {
        vec![
            hfact("w0", "n0", 0, true),
            hfact("w1", "n1", 100, false),
            w2.clone(),
        ]
    };
    let mut staged = host_native_report(2, stage(), "n0", None, None);
    staged.focused_global = "w2".to_owned();
    let _ = accepted(&mut engine, &staged);
    let reply = host_select_windows(&mut engine, 3, &away, stage(), "n0", "w2");
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    assert_eq!(reply.effects[0].target.0, "w2");
    assert!(reply.effects[0].await_shown, "float focus awaits arrival");
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding pends");
    let frozen = binding
        .members
        .iter()
        .find(|member| member.window.0 == "w2")
        .expect("frameless float focus freezes: {binding:?}");
    assert_eq!(frozen.frame, None);
    assert_eq!(frozen.home.workspace.0, away);
    let effect = reply.effects[0].clone();
    let frames = host_member_frames(&reply);
    let arrival = vec![
        native_bracket("w1", "n1", 3, frames[&WindowId("w1".to_owned())]),
        native_bracket("w2", "n1", 3, Some(rect(200, 0, 40, 90))),
    ];
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut proof = host_native_report(4, host_proof_windows(true), "n1", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "float arrival proves: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert_eq!(event.focus, SwitchFocus::Proven);
}

#[test]
fn host_owned_focus_bearing_freezes_sticky_focus_at_actual_home() {
    let (mut engine, away) = host_boot();
    let mut ws = hfact("ws", "n0", 50, false);
    ws.sticky = Some(true);
    let stage = || {
        vec![
            hfact("w0", "n0", 0, true),
            ws.clone(),
            hfact("w1", "n1", 100, false),
            hfact("w2", "n1", 200, false),
        ]
    };
    // Newcomers first (admission settles), so the later sticky
    // observation is the only canonical write.
    let mut introduced = host_native_report(2, stage(), "n0", None, None);
    introduced.focused_global = "w2".to_owned();
    let _ = accepted(&mut engine, &introduced);
    // Record sticky memory at the away domain: the adapter shows away
    // while the sticky window stays homed to home. No newcomers, so no
    // admission focus overwrites the slot.
    let mut observed = host_native_report(3, stage(), "n1", None, None);
    observed.focused_global = "ws".to_owned();
    let _ = accepted(&mut engine, &observed);
    // Select away from home: the remembered sticky focus must resolve.
    let mut selecting = host_native_report(
        4,
        stage(),
        "n0",
        Some(4),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: away.clone(),
        }),
    );
    selecting.focused_global = "ws".to_owned();
    let reply = accepted(&mut engine, &selecting);
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    assert_eq!(
        reply.effects[0].target.0, "ws",
        "remembered sticky focus resolves: {reply:?}"
    );
    assert!(reply.effects[0].await_shown, "sticky focus awaits arrival");
    let home = reply
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0-backed workspace")
        .id
        .clone();
    assert_ne!(home, away);
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding pends");
    for row in &reply.windows {
        assert!(
            row.visibility.is_none(),
            "host-owned grants no per-window targets: {row:?}"
        );
    }
    // Every minted effect target freezes with its ACTUAL home: the sticky
    // focus stays homed to home, never fabricated onto the target.
    let frozen = binding
        .members
        .iter()
        .find(|member| member.window.0 == "ws")
        .expect("sticky effect target freezes: {binding:?}");
    assert_eq!(
        frozen.home.workspace.0, home,
        "actual core home: {binding:?}"
    );
    assert_eq!(
        binding.members.len(),
        3,
        "only required members freeze: {binding:?}"
    );
    assert!(
        binding
            .members
            .iter()
            .filter(|member| member.window.0 == "w1" || member.window.0 == "w2")
            .all(|member| member.home.workspace.0 == away),
        "selected-home members keep the target home: {binding:?}"
    );
    // Native proof translates each frozen per-window home through this
    // engine's own bindings: the sticky bracket names n0, the rest n1.
    let effect = reply.effects[0].clone();
    let frames = host_member_frames(&reply);
    let mut arrival = vec![
        native_bracket("w1", "n1", 4, frames[&WindowId("w1".to_owned())]),
        native_bracket("w2", "n1", 4, frames[&WindowId("w2".to_owned())]),
    ];
    let ws_frame = frames.get(&WindowId("ws".to_owned())).copied().flatten();
    arrival.push(native_bracket(
        "ws",
        "n0",
        4,
        ws_frame.or(Some(rect(50, 0, 40, 90))),
    ));
    let row = feedback_row(
        &effect.id,
        "ws",
        effect.issued_revision,
        true,
        Some("ws"),
        arrival,
    );
    let mut proof = host_native_report(5, stage(), "n1", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "sticky arrival proves: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-4");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert_eq!(event.focus, SwitchFocus::Proven);
}

fn host_focus_grant_fullscreen_nonfocus() -> (WorldEngine, String, crate::world_report::WorldReply)
{
    let (mut engine, away) = host_boot();
    let mut w1 = hfact("w1", "n1", 100, false);
    w1.fullscreen = Some(true);
    let stage = || {
        vec![
            hfact("w0", "n0", 0, true),
            w1.clone(),
            hfact("w2", "n1", 200, false),
        ]
    };
    let mut staged = host_native_report(2, stage(), "n0", None, None);
    staged.focused_global = "w2".to_owned();
    let _ = accepted(&mut engine, &staged);
    let reply = host_select_windows(&mut engine, 3, &away, stage(), "n0", "w2");
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    assert_eq!(reply.effects[0].target.0, "w2");
    assert!(
        reply.effects[0].await_shown,
        "host-owned focus awaits arrival"
    );
    (engine, away, reply)
}

fn host_fullscreen_proof_windows() -> Vec<WindowFact> {
    let mut wins = host_proof_windows(true);
    wins[1].fullscreen = Some(true);
    wins
}

#[test]
fn host_owned_focus_bearing_freezes_frameless_fullscreen_nonfocus_member() {
    let (engine, away, grant) = host_focus_grant_fullscreen_nonfocus();
    let published = grant
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("fullscreen member publishes a row: {grant:?}");
    assert_eq!(
        published.effective_frame, None,
        "native overlay owns fullscreen pixels: {grant:?}"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding pends");
    let frozen = binding
        .members
        .iter()
        .find(|member| member.window.0 == "w1")
        .expect("frameless required member freezes: {binding:?}");
    assert_eq!(frozen.frame, None, "frame equality waived, never dropped");
    assert_eq!(frozen.home.workspace.0, away);
    assert!(
        binding.members.iter().any(|member| member.window.0 == "w2"),
        "focus member still freezes: {binding:?}"
    );
}

#[test]
fn host_owned_focus_bearing_missing_frameless_bracket_holds_not_ready() {
    let (mut engine, _away, grant) = host_focus_grant_fullscreen_nonfocus();
    let effect = grant.effects[0].clone();
    let frames = host_member_frames(&grant);
    // Arrival covers every frozen member except the frameless one: the
    // switch holds with foreground proven but arrival unreadiness.
    let arrival = vec![native_bracket(
        "w2",
        "n1",
        3,
        frames[&WindowId("w2".to_owned())],
    )];
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut proof = host_native_report(4, host_fullscreen_proof_windows(), "n1", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    // The latched feedback row decides arrival once and for all: the
    // missing member bracket fails the terminal (never a hold for more
    // evidence), with foreground proven but arrival unreadiness.
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "missing member bracket fails: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert!(event.failed.is_empty(), "no per-window failures: {event:?}");
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases on failure"
    );
}

#[test]
fn host_owned_focus_bearing_null_live_frame_never_proves() {
    let (mut engine, _away, grant) = host_focus_grant_fullscreen_nonfocus();
    let effect = grant.effects[0].clone();
    let frames = host_member_frames(&grant);
    // A null live frame for the frameless member never proves: waived
    // equality still needs a readable live frame.
    let arrival = vec![
        native_bracket("w1", "n1", 3, None),
        native_bracket("w2", "n1", 3, frames[&WindowId("w2".to_owned())]),
    ];
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut proof = host_native_report(4, host_fullscreen_proof_windows(), "n1", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    // Waived equality still needs a readable live frame: the null live
    // frame fails the terminal, never proves.
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "null live frame fails: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert!(event.failed.is_empty(), "no per-window failures: {event:?}");
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases on failure"
    );
}

#[test]
fn host_owned_focus_bearing_frameless_member_proves_on_readable_frame() {
    let (mut engine, away, grant) = host_focus_grant_fullscreen_nonfocus();
    let effect = grant.effects[0].clone();
    let frames = host_member_frames(&grant);
    // Any readable live frame proves the frameless member: null desired
    // waives equality only.
    let arrival = vec![
        native_bracket("w1", "n1", 3, Some(rect(100, 0, 40, 90))),
        native_bracket("w2", "n1", 3, frames[&WindowId("w2".to_owned())]),
    ];
    let row = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        arrival,
    );
    let mut proof = host_native_report(4, host_fullscreen_proof_windows(), "n1", None, None);
    proof.focus_feedback = vec![row];
    let settled = accepted(&mut engine, &proof);
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "readable frame proves: {settled:?}"
    );
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .retained
            .iter()
            .all(|r| r.target.operation_id != "op-3"),
        "actual parent retires"
    );
    let _ = away;
}

// ---- Native-free whole-workspace migration producer family ----
//
// Every test here runs the suppression-capable route (no host-owned
// domains): whole-workspace migration binds ONE frozen selection covering
// BOTH outputs with distinct selected workspaces, effect ONLY for the
// moved active client (D6), member arrival retires Migration while Switch
// pends on visibility/focus proofs.

fn mfact(window: &str, output: &str, x: i32) -> WindowFact {
    let mut fact = sfact(window, output, x);
    fact.sticky = Some(false);
    fact
}

fn mreport(
    seq: u64,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    feedback: Vec<FocusFeedback>,
    focused: &str,
) -> WorldReport {
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let mut base = report_multi(
        seq,
        windows,
        vec![],
        command_seq,
        command,
        feedback,
        focused,
        screens,
    );
    base.views = views
        .into_iter()
        .map(|(output, view)| crate::world_report::ScreenView {
            output: OutputId(output.to_owned()),
            view: view.to_owned(),
        })
        .collect();
    base
}

/// Boot out-a with w2 on the home workspace and w0 parked on the away
/// workspace, then pin the MRU source-refill setting with an observed
/// away -> home history so the occupied away (not the fresh trailing
/// spare) refills the source when home migrates. History builds through
/// observation-only ScreenViews (no commands), preserving op numbering:
/// boot seq 1, send op-2, history obs 3/4, migrate op-5.
fn migration_boot() -> (WorldEngine, String, String) {
    let mut engine = engine_migration();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    let (home, away) = (ring[0].clone(), ring[1].clone());
    let _ = accepted(
        &mut engine,
        &report_multi(
            2,
            vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w0".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            vec![],
            "w0",
            screens,
        ),
    );
    engine
        .apply_test_settings(
            1,
            vec![(
                "refill".to_owned(),
                crate::connection::RawSettingValue::String(
                    "most-recently-used-workspace".to_owned(),
                ),
            )],
        )
        .expect("settings accepted");
    // View history away -> home through real select commands: the
    // remembered previous is the occupied away, which the MRU refill
    // then selects over the fresh trailing spare. Bare ScreenViews never
    // move the native-free view, so observation-only reports cannot prime
    // history here. Op numbering: boot seq 1, send op-2, selects op-3 and
    // op-4, migrate op-5.
    let _ = accepted(
        &mut engine,
        &mreport(
            3,
            vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            Some(3),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: away.clone(),
            }),
            vec![],
            "",
        ),
    );
    let _ = accepted(
        &mut engine,
        &mreport(
            4,
            vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            Some(4),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: home.clone(),
            }),
            vec![],
            "",
        ),
    );
    (engine, home, away)
}

fn migrate(
    engine: &mut WorldEngine,
    seq: u64,
    source: &str,
    workspace: &str,
    focused: &str,
) -> crate::world_report::WorldReply {
    accepted(
        engine,
        &mreport(
            seq,
            vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            Some(seq),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId(source.to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: workspace.to_owned(),
            }),
            vec![],
            focused,
        ),
    )
}

#[test]
fn migration_focused_producer_binds_both_outputs_and_settles_terminal() {
    let (mut engine, home, away) = migration_boot();
    let granted = migrate(&mut engine, 5, "out-a", &home, "w2");
    // Commit: exactly one Migration flight, no refill child.
    assert!(
        granted.transfers.iter().any(|t| t.operation_id == "op-5"
            && matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "migration flight retained: {:?}",
        granted.transfers
    );
    assert!(
        granted
            .transfers
            .iter()
            .all(|t| t.operation_id != "op-5-refill"),
        "no refill child on the native-free route: {:?}",
        granted.transfers
    );
    // One frozen binding covers both outputs with distinct selected homes.
    let binding = engine
        .retained_selection
        .values()
        .find(|b| b.parent_op == "op-5")
        .expect("migration binding")
        .clone();
    {
        let mut affected = binding.affected.clone();
        affected.sort();
        assert_eq!(
            affected,
            vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())]
        );
    }
    assert!(
        binding
            .selected
            .iter()
            .any(|h| h.output.0 == "out-b" && h.workspace.0 == home),
        "target selected: {:?}",
        binding.selected
    );
    let refill = binding
        .selected
        .iter()
        .find(|h| h.output.0 == "out-a")
        .expect("source refill selected")
        .workspace
        .0
        .clone();
    assert_ne!(refill, home, "refill is the actual source view");
    assert_eq!(
        refill, away,
        "MRU refill selects the occupied away: {binding:?}"
    );
    // D6: only the moved active client refocuses, awaiting arrival.
    assert_eq!(granted.effects.len(), 1, "one focus effect: {granted:?}");
    assert_eq!(granted.effects[0].target.0, "w2");
    assert!(granted.effects[0].await_shown);
    // B1 pin: the mover freezes at the TARGET with its final published
    // frame even though the pre-arrival target domain decorates Hold.
    let w2_row = row(&granted, "w2");
    let w2_target = w2_row.visibility.expect("mover granted show");
    assert!(w2_target.shown, "mover shows at the target");
    assert_eq!(
        w2_row.visibility_authority,
        ScopeAuthority::Controlled,
        "fully readable facts grant Controlled: {w2_row:?}"
    );
    assert_eq!(
        binding.members.len(),
        1,
        "arrival set is the mover: {binding:?}"
    );
    assert_eq!(binding.members[0].window.0, "w2");
    assert_eq!(binding.members[0].home.workspace.0, home);
    assert_eq!(binding.members[0].frame, w2_row.effective_frame);
    assert!(w2_row.effective_frame.is_some(), "mover publishes a frame");
    // Both-output obligations, membership separation: the arrival set is
    // the focus-output mover alone; the switch set also obligates the
    // refill stayer on the other output.
    let mut switch_windows: Vec<String> = binding
        .switch_members
        .iter()
        .map(|m| m.window.0.clone())
        .collect();
    switch_windows.sort();
    assert_eq!(switch_windows, vec!["w0".to_owned(), "w2".to_owned()]);
    assert!(
        binding
            .switch_members
            .iter()
            .any(|m| m.window.0 == "w0" && m.home.workspace.0 == refill && m.shown),
        "refill stayer obligated: {:?}",
        binding.switch_members
    );
    // Source-homing report holds both flights with no terminal.
    let rev = granted.state_revision;
    let effect = granted.effects[0].clone();
    let frame = w2_row.effective_frame;
    let held = accepted(
        &mut engine,
        &mreport(
            6,
            vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "w2",
        ),
    );
    assert!(
        held.transfers.iter().any(|t| t.operation_id == "op-5"),
        "source homing holds the parent: {:?}",
        held.transfers
    );
    assert!(
        engine
            .retained_selection
            .values()
            .any(|b| b.parent_op == "op-5"),
        "binding holds under source homing"
    );
    assert!(held.switch_settled.is_empty(), "no terminal while held");
    // Arrival WITHOUT ScreenViews retires the Migration parent while the
    // Switch pends: no echo tail on this route.
    let arrived = accepted(
        &mut engine,
        &mreport(
            7,
            vec![mfact("w2", "out-b", 800), mfact("w0", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "w2",
        ),
    );
    assert!(
        arrived.transfers.iter().all(|t| t.operation_id != "op-5"),
        "member arrival retires the parent echoless: {:?}",
        arrived.transfers
    );
    assert!(
        engine
            .retained_selection
            .values()
            .any(|b| b.parent_op == "op-5"),
        "Switch pends without visibility verdicts"
    );
    assert!(
        arrived.switch_settled.is_empty(),
        "no terminal without verdicts"
    );
    // Real producer arrival proof: the frozen frame with an Output arm.
    let ready = |obs| {
        feedback_row(
            &effect.id,
            "w2",
            effect.issued_revision,
            true,
            Some("w2"),
            vec![bracket("w2", "out-b", obs, frame)],
        )
    };
    // Target-only verdicts plus the first feedback still pend: the refill
    // obligation is missing.
    let w2_lw = last_write_confirmed(true, rev, 8);
    let partial = accepted(
        &mut engine,
        &mreport(
            8,
            vec![
                with_lw(mfact("w2", "out-b", 800), w2_lw),
                mfact("w0", "out-a", 0),
            ],
            vec![],
            None,
            None,
            vec![ready(8)],
            "w2",
        ),
    );
    assert!(
        partial.switch_settled.is_empty(),
        "refill verdict missing: {partial:?}"
    );
    let latched = engine
        .retained_selection
        .values()
        .find(|b| b.parent_op == "op-5")
        .expect("binding pends");
    assert!(latched.feedback.is_some(), "first feedback latches");
    assert!(latched.foreground_proven, "foreground proves");
    assert!(latched.arrival_ready, "real frame proves arrival");
    // Full verdicts complete with ONE terminal event; the repeated
    // feedback never upgrades the first latch.
    let w0_lw = last_write_confirmed(true, rev, 9);
    let w2_lw2 = last_write_confirmed(true, rev, 9);
    let done = accepted(
        &mut engine,
        &mreport(
            9,
            vec![
                with_lw(mfact("w2", "out-b", 800), w2_lw2),
                with_lw(mfact("w0", "out-a", 0), w0_lw),
            ],
            vec![],
            None,
            None,
            vec![ready(8)],
            "w2",
        ),
    );
    assert_eq!(done.switch_settled.len(), 1, "one terminal: {done:?}");
    let event = &done.switch_settled[0];
    assert_eq!(event.operation_id, "op-5");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(!event.fallback);
    assert!(
        engine
            .retained_selection
            .values()
            .all(|b| b.parent_op != "op-5"),
        "finite binding releases"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned()))
            || engine
                .selection_scopes
                .contains_key(&OutputId("out-b".to_owned())),
        "scopes survive"
    );
    // A repeat report emits nothing: first-latched evidence stays settled.
    let again = accepted(
        &mut engine,
        &mreport(
            10,
            vec![mfact("w2", "out-b", 800), mfact("w0", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "w2",
        ),
    );
    assert!(
        again.switch_settled.is_empty(),
        "next report publishes empty"
    );
    let _ = away;
}

#[test]
fn migration_zero_member_issues_unproven_and_retires_parent_echoless() {
    // Genuinely zero visibility members: the trailing spare is empty, so
    // the issuing reply itself carries the terminal and retains no parent:
    // a zero-member native-free migration has no membership to await and
    // never mints a Migration flight, even when the next report arrives.
    let mut engine = engine_migration();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let boot = accepted(
        &mut engine,
        &report_multi(1, vec![], vec![], None, None, vec![], "", screens.clone()),
    );
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|r| r.output.0 == "out-a")
        .map(|r| r.id.clone())
        .collect();
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    let empty = ring.last().expect("trailing spare").clone();
    let reply = accepted(
        &mut engine,
        &mreport(
            2,
            vec![],
            vec![],
            Some(2),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: empty.clone(),
            }),
            vec![],
            "",
        ),
    );
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "zero-member native-free migration retains no parent: {:?}",
        reply.transfers
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "zero-obligation issues: {reply:?}"
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.operation_id, "op-2");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    // No parent was ever retained: the next report and even a gapped
    // report publish no Migration flight.
    let next = accepted(
        &mut engine,
        &mreport(3, vec![], vec![], None, None, vec![], ""),
    );
    assert!(
        next.transfers.iter().all(|t| t.operation_id != "op-2"),
        "no Migration flight was ever retained: {:?}",
        next.transfers
    );
    assert!(next.switch_settled.is_empty());
    let mut gapped = mreport(4, vec![], vec![], None, None, vec![], "");
    gapped.inventory_complete = false;
    let gap_held = accepted(&mut engine, &gapped);
    assert!(
        gap_held.transfers.iter().all(|t| t.operation_id != "op-2"),
        "gap never mints a Migration flight: {:?}",
        gap_held.transfers
    );
}

#[test]
fn migration_empty_with_refill_stayer_pends_switch_but_retires_parent() {
    // Empty move whose refill still obligates a stayer: the Switch pends
    // on the stayer verdict while no zero-member parent is ever retained.
    let mut engine = engine_migration();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            vec![mfact("w1", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "",
            screens,
        ),
    );
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|r| r.output.0 == "out-a")
        .map(|r| r.id.clone())
        .collect();
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    let empty = ring.last().expect("trailing spare").clone();
    let reply = accepted(
        &mut engine,
        &mreport(
            2,
            vec![mfact("w1", "out-a", 0)],
            vec![],
            Some(2),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: empty.clone(),
            }),
            vec![],
            "",
        ),
    );
    let binding = engine
        .retained_selection
        .values()
        .find(|b| b.parent_op == "op-2")
        .expect("stayer-obligated binding");
    assert!(
        !binding.switch_members.is_empty(),
        "refill stayer obligated: {binding:?}"
    );
    assert!(
        reply.switch_settled.is_empty(),
        "Switch pends on the stayer"
    );
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "zero-member parent never retained even while Switch pends: {:?}",
        reply.transfers
    );
    let next = accepted(
        &mut engine,
        &mreport(
            3,
            vec![mfact("w1", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        next.transfers.iter().all(|t| t.operation_id != "op-2"),
        "zero-member parent never retained while Switch pends: {:?}",
        next.transfers
    );
    assert!(
        engine
            .retained_selection
            .values()
            .any(|b| b.parent_op == "op-2"),
        "visibility obligations stay held in the binding"
    );
    assert!(next.switch_settled.is_empty());
}

#[test]
fn migration_issuing_reserve_covers_both_migration_commands() {
    let engine = engine();
    let ws = WorldCommand::MigrateWorkspace {
        source_output: OutputId("out-a".to_owned()),
        target_output: OutputId("out-b".to_owned()),
        workspace: "ws-1".to_owned(),
    };
    let dir = WorldCommand::MigrateDirectional {
        direction: "right".to_owned(),
    };
    assert!(
        engine.switch_issuing_reserve(Some(&ws)) > 0,
        "MigrateWorkspace reserves the zero-obligation issuing terminal"
    );
    assert!(
        engine.switch_issuing_reserve(Some(&dir)) > 0,
        "MigrateDirectional reserves the zero-obligation issuing terminal"
    );
}

#[test]
fn migration_focusless_variants_bind_without_effect() {
    // Focusless occupied (empty foreground): same frozen scope, no effect.
    let (mut engine, home, _) = migration_boot();
    let granted = migrate(&mut engine, 5, "out-a", &home, "");
    assert!(granted.effects.is_empty(), "no focus invented: {granted:?}");
    let binding = engine
        .retained_selection
        .values()
        .find(|b| b.parent_op == "op-5")
        .expect("focusless binding")
        .clone();
    assert!(binding.effect.is_none());
    assert!(binding.members.is_empty(), "no arrival set without focus");
    assert!(
        !binding.switch_members.is_empty(),
        "visibility still obligated"
    );
    // Arrival retires the parent while the focusless Switch pends.
    let arrived = accepted(
        &mut engine,
        &mreport(
            6,
            vec![mfact("w2", "out-b", 800), mfact("w0", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "",
        ),
    );
    assert!(
        arrived.transfers.iter().all(|t| t.operation_id != "op-5"),
        "parent retires: {:?}",
        arrived.transfers
    );
    // Full verdicts complete focusless with Unproven and no feedback gate.
    let rev = granted.state_revision;
    let lws: Vec<WindowFact> = binding
        .switch_members
        .iter()
        .map(|m| {
            with_lw(
                mfact(&m.window.0, &m.home.output.0, 0),
                last_write_confirmed(m.shown, rev, 7),
            )
        })
        .collect();
    let done = accepted(
        &mut engine,
        &mreport(7, lws, vec![], None, None, vec![], ""),
    );
    assert_eq!(done.switch_settled.len(), 1, "one terminal: {done:?}");
    assert_eq!(done.switch_settled[0].focus, SwitchFocus::Unproven);
    assert_eq!(done.switch_settled[0].outcome, SwitchOutcome::Complete);
}

#[test]
fn migration_newer_selection_supersedes_both_outputs_silently() {
    // A newer explicit selection supersedes the pending both-output
    // migration binding without completing it: latest intent wins, no
    // terminal for the abandoned parent.
    let (mut engine, home, away) = migration_boot();
    let granted = migrate(&mut engine, 5, "out-a", &home, "w2");
    assert!(
        engine
            .retained_selection
            .values()
            .any(|b| b.parent_op == "op-5"),
        "migration binding pends: {:?}",
        granted.transfers
    );
    let ring = ring_ids(&granted, "out-a");
    let spare = ring
        .iter()
        .find(|id| *id != &home && *id != &away)
        .expect("fresh trailing spare")
        .clone();
    let back = accepted(
        &mut engine,
        &mreport(
            6,
            vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            Some(6),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: spare.clone(),
            }),
            vec![],
            "",
        ),
    );
    assert!(
        back.switch_settled.is_empty(),
        "supersession completes nothing: {back:?}"
    );
    // Per-output-local supersession: the newer intent wins its own scope.
    // The both-output migration grant releases out-a; out-b may persist.
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("new binding retained");
    assert_eq!(binding.parent_op, "op-6", "latest intent wins: {binding:?}");
}

#[test]
fn migration_close_mover_drops_binding_without_terminal() {
    // Closing the moved focus target drops the finite both-output grant
    // with no terminal: the closed lifetime can never prove arrival.
    let (mut engine, home, _) = migration_boot();
    let granted = migrate(&mut engine, 5, "out-a", &home, "w2");
    assert_eq!(granted.effects.len(), 1, "one focus effect: {granted:?}");
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let closed = accepted(
        &mut engine,
        &report_multi(
            6,
            vec![mfact("w0", "out-a", 0)],
            vec!["w2"],
            None,
            None,
            vec![],
            "",
            screens,
        ),
    );
    assert!(
        engine
            .retained_selection
            .values()
            .all(|b| b.parent_op != "op-5"),
        "closed focus drops the binding"
    );
    assert!(closed.switch_settled.is_empty(), "close claims no terminal");
    assert!(
        !engine
            .selection_targets
            .contains_key(&WindowId("w2".to_owned())),
        "closed member targets drop"
    );
}

#[test]
fn migration_rollback_revokes_exact_rev_and_keeps_newer_selection() {
    // B2: terminal migration rollback revokes ONLY the exact parent-issued
    // selection binding/scopes/targets by the stored issuing revision, so a
    // newer selection on the same output survives a late parent refusal.
    // Silent drop: no new event or token.
    let (mut engine, home, away) = migration_boot();
    let granted = migrate(&mut engine, 5, "out-a", &home, "w2");
    let sel_rev = engine
        .retained_selection
        .values()
        .find(|b| b.parent_op == "op-5")
        .expect("migration binding")
        .rev;
    // Newer selection on out-a to the fresh trailing spare (op-6).
    let ring = ring_ids(&granted, "out-a");
    let spare = ring
        .iter()
        .find(|id| *id != &home && *id != &away)
        .expect("fresh trailing spare")
        .clone();
    let _ = accepted(
        &mut engine,
        &mreport(
            6,
            vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            Some(6),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: spare,
            }),
            vec![],
            "",
        ),
    );
    assert_eq!(
        engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("newer binding")
            .parent_op,
        "op-6",
        "newer selection binds"
    );
    // Late wholly-unarrived refusal for op-5 with complete inventory:
    // the mover never left the source.
    let mut refusal = mreport(
        7,
        vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
        vec![],
        None,
        None,
        vec![],
        "",
    );
    refusal.member_attempts = vec![MemberAttempt {
        operation_id: "op-5".to_owned(),
        window: WindowId("w2".to_owned()),
        target_output: OutputId("out-b".to_owned()),
        target_workspace: home.clone(),
        setter: FrameSetter::Refused,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(home.clone()),
        }),
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(home.clone()),
        }),
        attempt_ms: 7,
    }];
    let settled = accepted(&mut engine, &refusal);
    // Mapping rolls back: home is back on the source output.
    let row = settled
        .workspaces
        .iter()
        .find(|row| row.id == home)
        .expect("mapping restored");
    assert_eq!(
        row.output.0, "out-a",
        "rollback restores the source: {row:?}"
    );
    // Exact-rev revoke: no op-5 binding survives anywhere ...
    assert!(
        engine
            .retained_selection
            .values()
            .all(|b| !(b.parent_op == "op-5" && b.rev == sel_rev)),
        "exact parent-issued grant revoked"
    );
    // ... while the newer op-6 selection on the same output survives.
    assert_eq!(
        engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("newer survives rollback")
            .parent_op,
        "op-6",
        "newer selection survives"
    );
    assert!(
        settled.switch_settled.is_empty(),
        "rollback drops silently: {settled:?}"
    );
}

#[test]
fn migration_source_output_bracket_never_proves_arrival() {
    // A source-output arrival bracket against the target-frozen home never
    // proves arrival. With every visibility verdict latched, the shared
    // terminal still issues: wrong arrival degrades focus to Unproven, so
    // the outcome is CompletedWithFailures, never Complete/Proven.
    // failed[] carries per-window visibility failures only (all verdicts
    // here confirm), so it stays empty. A later corrected bracket cannot
    // upgrade or replay the terminal.
    let (mut engine, home, _) = migration_boot();
    let granted = migrate(&mut engine, 5, "out-a", &home, "w2");
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let frame = row(&granted, "w2").effective_frame;
    let w0_frame = row(&granted, "w0").effective_frame;
    // Source-homing hold keeps the focused terminal shape, then member
    // arrival retires the parent while the Switch pends.
    let _ = accepted(
        &mut engine,
        &mreport(
            6,
            vec![mfact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "w2",
        ),
    );
    let _ = accepted(
        &mut engine,
        &mreport(
            7,
            vec![mfact("w2", "out-b", 800), mfact("w0", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "w2",
        ),
    );
    // Full visibility verdicts but the mover bracket names the source
    // output: the frozen home is the migrated target id on out-b.
    let w2_lw = last_write_confirmed(true, rev, 8);
    let w0_lw = last_write_confirmed(true, rev, 8);
    let wrong = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![
            bracket("w2", "out-a", 8, frame),
            bracket("w0", "out-a", 8, w0_frame),
        ],
    );
    let held = accepted(
        &mut engine,
        &mreport(
            8,
            vec![
                with_lw(mfact("w2", "out-b", 800), w2_lw),
                with_lw(mfact("w0", "out-a", 0), w0_lw),
            ],
            vec![],
            None,
            None,
            vec![wrong],
            "w2",
        ),
    );
    assert_eq!(
        held.switch_settled.len(),
        1,
        "wrong arrival still terminates: {held:?}"
    );
    let event = &held.switch_settled[0];
    assert_eq!(event.operation_id, "op-5");
    assert_eq!(event.outcome, SwitchOutcome::CompletedWithFailures);
    assert_ne!(
        event.outcome,
        SwitchOutcome::Complete,
        "never Complete on wrong arrival"
    );
    assert!(
        event.failed.is_empty(),
        "visibility verdicts all confirm: {event:?}"
    );
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert_ne!(
        event.focus,
        SwitchFocus::Proven,
        "never Proven on wrong arrival"
    );
    assert!(!event.fallback);
    assert!(
        engine
            .retained_selection
            .values()
            .all(|b| b.parent_op != "op-5"),
        "finite binding releases on the failure terminal"
    );
    // A corrected late bracket cannot upgrade or replay the terminal.
    let correct = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        vec![
            bracket("w2", "out-b", 9, frame),
            bracket("w0", "out-a", 9, w0_frame),
        ],
    );
    let again = accepted(
        &mut engine,
        &mreport(
            9,
            vec![mfact("w2", "out-b", 800), mfact("w0", "out-a", 0)],
            vec![],
            None,
            None,
            vec![correct],
            "w2",
        ),
    );
    assert!(
        again.switch_settled.is_empty(),
        "corrected replay emits nothing: {again:?}"
    );
}

#[test]
fn migration_minimized_only_binds_focusless() {
    // Minimized-only mover: ineligible focus, same frozen scope, no effect.
    let mut engine = engine_migration();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            vec![
                {
                    let mut f = mfact("w2", "out-a", 100);
                    f.minimized = Some(true);
                    f.rect = None;
                    f
                },
                mfact("w0", "out-a", 0),
            ],
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|r| r.output.0 == "out-a")
        .map(|r| r.id.clone())
        .collect();
    let (home, away) = (ring[0].clone(), ring[1].clone());
    let _ = accepted(
        &mut engine,
        &report_multi(
            2,
            vec![
                {
                    let mut f = mfact("w2", "out-a", 100);
                    f.minimized = Some(true);
                    f.rect = None;
                    f
                },
                mfact("w0", "out-a", 0),
            ],
            vec![],
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w0".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            vec![],
            "w0",
            screens,
        ),
    );
    let granted = accepted(
        &mut engine,
        &mreport(
            3,
            vec![
                {
                    let mut f = mfact("w2", "out-a", 100);
                    f.minimized = Some(true);
                    f.rect = None;
                    f
                },
                mfact("w0", "out-a", 0),
            ],
            vec![],
            Some(3),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: home.clone(),
            }),
            vec![],
            "w2",
        ),
    );
    assert!(
        granted.effects.is_empty(),
        "minimized mover takes no focus: {granted:?}"
    );
    // The minimized mover is blocked from per-window targets and the
    // refill resolves to the trailing spare rather than the stayer's
    // home, so no switch member obligates: the focusless binding
    // completes at issuing through the existing zero-obligation rule
    // while the non-empty Migration parent (members=[w2]) stays
    // retained for membership proof.
    assert!(
        granted.transfers.iter().any(|t| t.operation_id == "op-3"
            && matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "non-empty parent retained: {:?}",
        granted.transfers
    );
    assert_eq!(
        granted.switch_settled.len(),
        1,
        "zero-obligation focusless issues: {granted:?}"
    );
    assert_eq!(granted.switch_settled[0].operation_id, "op-3");
    assert!(
        engine
            .retained_selection
            .values()
            .all(|b| b.parent_op != "op-3"),
        "zero-obligation binding releases at issuing"
    );
}

#[test]
fn migration_sticky_foreground_takes_no_focus() {
    // Sticky-active member with the foreground naming it: the sticky
    // lifetime is not a migration member, so no focus resolves.
    let mut engine = engine_migration();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let sticky_fact = |window: &str, output: &str, x: i32| {
        let mut f = mfact(window, output, x);
        f.sticky = Some(true);
        f
    };
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            vec![sticky_fact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|r| r.output.0 == "out-a")
        .map(|r| r.id.clone())
        .collect();
    let (home, away) = (ring[0].clone(), ring[1].clone());
    let _ = accepted(
        &mut engine,
        &report_multi(
            2,
            vec![sticky_fact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("w0".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: away.clone(),
                follow: false,
            }),
            vec![],
            "w0",
            screens,
        ),
    );
    let granted = accepted(
        &mut engine,
        &mreport(
            3,
            vec![sticky_fact("w2", "out-a", 100), mfact("w0", "out-a", 0)],
            vec![],
            Some(3),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: home.clone(),
            }),
            vec![],
            "w2",
        ),
    );
    assert!(
        granted.effects.iter().all(|e| e.target.0 != "w2"),
        "sticky foreground never focuses: {granted:?}"
    );
}

#[test]
fn migration_floating_source_binds_covering_selection() {
    // Floating-scope source migrates sessionless on the same covering route.
    let mut engine = engine_migration();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            vec![mfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
            screens.clone(),
        ),
    );
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|r| r.output.0 == "out-a")
        .map(|r| r.id.clone())
        .collect();
    let home = ring[0].clone();
    let _ = accepted(
        &mut engine,
        &report_multi(
            2,
            vec![mfact("w2", "out-a", 100)],
            vec![],
            Some(2),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: home.clone(),
                tiled: false,
            }),
            vec![],
            "",
            screens,
        ),
    );
    let granted = migrate(&mut engine, 3, "out-a", &home, "w2");
    assert!(
        granted.transfers.iter().any(|t| t.operation_id == "op-3"
            && matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "floating parent retained: {:?}",
        granted.transfers
    );
    assert!(
        granted
            .transfers
            .iter()
            .all(|t| t.operation_id != "op-3-refill"),
        "no refill child on the floating route: {:?}",
        granted.transfers
    );
    let binding = engine
        .retained_selection
        .values()
        .find(|b| b.parent_op == "op-3")
        .expect("floating binding");
    {
        let mut affected = binding.affected.clone();
        affected.sort();
        assert_eq!(
            affected,
            vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())]
        );
    }
}

#[test]
fn selection_eligible_ignores_logical_echo_then_completes_on_verdicts() {
    // Eligible explicit select on native-free: the logical echo lane is
    // never ingress (views rows are inert whether absent or
    // contradictory), so the binding and its view flight survive until
    // real producer verdicts plus first feedback complete the terminal.
    for (what, contradict) in [("absent", false), ("contradictory", true)] {
        let (mut engine, home, away) = select_setup();
        let granted = select(&mut engine, 3, &away);
        assert_eq!(
            granted.effects.len(),
            1,
            "eligible select grants ({what}): {granted:?}"
        );
        assert!(granted.effects[0].await_shown);
        let binding = engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .expect("binding")
            .clone();
        assert_eq!(binding.parent_op, "op-3");
        let mut waiting = report(
            4,
            vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
            vec![],
            None,
            None,
            vec![],
            "",
        );
        if contradict {
            waiting.views = vec![crate::world_report::ScreenView {
                output: OutputId("out-a".to_owned()),
                view: home.clone(),
            }];
        }
        let waited = accepted(&mut engine, &waiting);
        assert!(
            waited.transfers.iter().any(|t| t.operation_id == "op-3"),
            "flight survives the {what} echo lane: {:?}",
            waited.transfers
        );
        assert!(
            engine
                .retained_selection
                .contains_key(&OutputId("out-a".to_owned())),
            "binding survives the {what} echo lane"
        );
        assert!(
            waited.switch_settled.is_empty(),
            "no terminal without verdicts ({what}): {waited:?}"
        );
    }
    // Producer completion: hide/show verdicts plus the first exact
    // feedback brackets retire the flight through one terminal.
    let (mut engine, home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    let _ = home;
    let effect = granted.effects[0].clone();
    let rev = granted.state_revision;
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding")
        .clone();
    let frames: std::collections::BTreeMap<WindowId, Option<Rect>> = granted
        .windows
        .iter()
        .map(|row| (row.lifetime.clone(), row.effective_frame))
        .collect();
    let brackets: Vec<ArrivalBracket> = binding
        .members
        .iter()
        .map(|member| crate::world_report::ArrivalBracket {
            window: member.window.clone(),
            obs: 4,
            before: sample(4, true, true, false, false),
            location: ArrivalLocation::Output {
                out: Some(OutputId("out-a".to_owned())),
            },
            frame: frames.get(&member.window).copied().flatten(),
            after: sample(4, true, true, false, false),
        })
        .collect();
    assert!(
        !brackets.is_empty(),
        "frozen members prove arrival: {binding:?}"
    );
    let feedback = feedback_row(
        &effect.id,
        "w2",
        effect.issued_revision,
        true,
        Some("w2"),
        brackets,
    );
    let done = accepted(
        &mut engine,
        &report(
            4,
            vec![
                with_lw(sfact("w1", "out-a", 0), last_write_confirmed(false, rev, 4)),
                with_lw(
                    sfact("w2", "out-a", 100),
                    last_write_confirmed(true, rev, 4),
                ),
            ],
            vec![],
            None,
            None,
            vec![feedback],
            "",
        ),
    );
    assert_eq!(done.switch_settled.len(), 1, "one terminal: {done:?}");
    let event = &done.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Proven);
    assert!(!event.fallback);
    assert!(
        done.transfers.iter().all(|t| t.operation_id != "op-3"),
        "verdicts retire the flight: {:?}",
        done.transfers
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive the terminal"
    );
    let _ = away;
}

#[test]
fn policy_displaced_view_binds_focusless_under_own_parent_and_completes() {
    // Hotplug displacement with the focused window homed in the moving
    // workspace: the policy retains the survivor view under its own
    // per-cycle op-dpv-{epoch}-{index} parent and binds one focusless
    // frozen selection under the same parent (no command sequence, no
    // focus effect, native focus untouched). Real hide/show verdicts
    // complete the common terminal and retire the flight; scopes survive.
    let mut engine = engine();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            vec![sfact("w-a", "out-a", 0), sfact("w-b", "out-b", 800)],
            vec![],
            None,
            None,
            vec![],
            "",
            screens,
        ),
    );
    let home_a = ring_ids(&boot, "out-a")[0].clone();
    let home_b = ring_ids(&boot, "out-b")[0].clone();
    // out-b disconnects while w-b is focused and homed in the moving
    // workspace: the survivor retains its view under a policy parent.
    let displaced = accepted(
        &mut engine,
        &report_multi(
            2,
            vec![sfact("w-a", "out-a", 0), sfact("w-b", "out-a", 800)],
            vec![],
            None,
            None,
            vec![],
            "w-b",
            vec![screen("out-a", 0)],
        ),
    );
    let parent = displaced
        .transfers
        .iter()
        .find_map(|t| {
            t.operation_id
                .starts_with("op-dpv-")
                .then(|| t.operation_id.clone())
        })
        .expect("policy view flight");
    assert!(
        parent.ends_with("-0"),
        "per-run index pins the parent identity: {parent}"
    );
    assert_eq!(
        engine.effective_view(&OutputId("out-a".to_owned())),
        Some(home_b.clone()),
        "policy view retained on the survivor"
    );
    assert!(
        displaced.effects.is_empty(),
        "policy mints no focus effect: {displaced:?}"
    );
    assert!(
        engine.pending_selection.is_none(),
        "policy never touches the command slot"
    );
    assert!(
        engine.pending_policy_selections.is_empty(),
        "policy carrier drains at publish"
    );
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("policy binding")
        .clone();
    assert_eq!(binding.parent_op, parent);
    assert!(binding.effect.is_none(), "focusless: {binding:?}");
    assert!(
        binding.view_targets.is_empty(),
        "native-free carries no native obligation"
    );
    let mut members: Vec<(String, bool)> = binding
        .switch_members
        .iter()
        .map(|m| (m.window.0.clone(), m.shown))
        .collect();
    members.sort();
    assert_eq!(
        members,
        vec![("w-a".to_owned(), false), ("w-b".to_owned(), true)],
        "departing hides and selected shows freeze: {binding:?}"
    );
    // Producer verdicts complete the common terminal and retire the
    // flight; scopes survive the terminal.
    let rev = binding.rev;
    let done = accepted(
        &mut engine,
        &report_multi(
            3,
            vec![
                with_lw(
                    sfact("w-a", "out-a", 0),
                    last_write_confirmed(false, rev, 3),
                ),
                with_lw(
                    sfact("w-b", "out-a", 800),
                    last_write_confirmed(true, rev, 3),
                ),
            ],
            vec![],
            None,
            None,
            vec![],
            "",
            vec![screen("out-a", 0)],
        ),
    );
    assert_eq!(done.switch_settled.len(), 1, "one terminal: {done:?}");
    let event = &done.switch_settled[0];
    assert_eq!(event.operation_id, parent);
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(!event.fallback);
    assert!(
        done.transfers.iter().all(|t| t.operation_id != parent),
        "terminal retires the policy flight: {:?}",
        done.transfers
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "finite binding releases"
    );
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "scopes survive the terminal"
    );
    let _ = home_a;
}

#[test]
fn policy_pending_loses_to_overlapping_command_in_the_same_cycle() {
    // Same-cycle overlapping command supersedes the policy carrier:
    // command dispatch owns supersession, so the policy pending drops
    // and only the command binding freezes (its view flight replaces the
    // policy flight latest-wins per output).
    let mut engine = engine();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let boot = accepted(
        &mut engine,
        &report_multi(
            1,
            vec![sfact("w-a", "out-a", 0), sfact("w-b", "out-b", 800)],
            vec![],
            None,
            None,
            vec![],
            "",
            screens,
        ),
    );
    let home_a = ring_ids(&boot, "out-a")[0].clone();
    let raced = accepted(
        &mut engine,
        &report_multi(
            2,
            vec![sfact("w-a", "out-a", 0), sfact("w-b", "out-a", 800)],
            vec![],
            Some(2),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: home_a.clone(),
            }),
            vec![],
            "w-b",
            vec![screen("out-a", 0)],
        ),
    );
    assert!(
        raced
            .transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-dpv-")),
        "no policy flight survives the race: {:?}",
        raced.transfers
    );
    assert_eq!(raced.effects.len(), 1, "command effect only: {raced:?}");
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("command binding")
        .clone();
    assert_eq!(binding.parent_op, "op-2");
    assert!(
        engine.pending_policy_selections.is_empty(),
        "overlapping policy carrier dropped"
    );
}

#[test]
fn selection_redundant_unchanged_preserves_binding_history_and_mints_nothing() {
    // Redundant native-free select of the shown view: exact NoEffect
    // reply semantics (no focus effect, no new view flight, no terminal)
    // with the outstanding previous selection fully preserved
    // (supersession end-state, history, and per-window targets untouched).
    let (mut engine, home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    assert_eq!(granted.effects.len(), 1, "prior select grants");
    // First redundant: converge catches history up to the op-3 move (not
    // the command: the command itself mints nothing).
    let first = select(&mut engine, 4, &away);
    assert!(
        first.effects.is_empty(),
        "redundant select mints no focus: {first:?}"
    );
    assert!(
        first.transfers.iter().all(|t| t.operation_id != "op-4"),
        "redundant select mints no view flight: {:?}",
        first.transfers
    );
    assert!(
        first.switch_settled.is_empty(),
        "redundant select mints no terminal: {first:?}"
    );
    assert_eq!(
        engine.history.previous("out-a"),
        Some(home.clone()),
        "converge catches up to the op-3 move"
    );
    let before_binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("outstanding binding")
        .clone();
    let before_scope = engine
        .selection_scopes
        .get(&OutputId("out-a".to_owned()))
        .expect("outstanding scope")
        .clone();
    let before_targets = engine.selection_targets.clone();
    let before_view = engine.effective_view(&OutputId("out-a".to_owned()));
    // Second redundant: everything preserved exactly.
    let again = select(&mut engine, 5, &away);
    assert!(
        again.effects.is_empty(),
        "redundant select mints no focus: {again:?}"
    );
    assert!(
        again.transfers.iter().all(|t| t.operation_id != "op-5"),
        "redundant select mints no view flight: {:?}",
        again.transfers
    );
    assert!(
        again.transfers.iter().any(|t| t.operation_id == "op-3"),
        "prior flight supersession end-state preserved: {:?}",
        again.transfers
    );
    assert!(
        again.switch_settled.is_empty(),
        "redundant select mints no terminal: {again:?}"
    );
    assert_eq!(
        engine.retained_selection.get(&OutputId("out-a".to_owned())),
        Some(&before_binding),
        "outstanding binding preserved"
    );
    assert_eq!(
        engine.selection_scopes.get(&OutputId("out-a".to_owned())),
        Some(&before_scope),
        "outstanding scope preserved"
    );
    assert_eq!(
        engine.selection_targets, before_targets,
        "per-window targets preserved"
    );
    assert_eq!(
        engine.history.previous("out-a"),
        Some(home),
        "history preserved"
    );
    assert_eq!(
        engine.effective_view(&OutputId("out-a".to_owned())),
        before_view,
        "view preserved"
    );
}

fn d23_subtree_leaves(node: &Node) -> Vec<String> {
    match node {
        Node::Leaf { id } => vec![id.0.clone()],
        Node::Group { children, .. } => children.iter().flat_map(d23_subtree_leaves).collect(),
    }
}

fn d23_group_leaves(tree: &Node, leaf: &NodeId) -> Option<Vec<String>> {
    let Node::Group { children, .. } = tree else {
        return None;
    };
    for child in children {
        if d23_subtree_leaves(child).contains(&leaf.0) {
            return match child {
                Node::Leaf { .. } => Some(d23_subtree_leaves(tree)),
                Node::Group { .. } => d23_group_leaves(child, leaf),
            };
        }
    }
    None
}

fn d23_home_tree(engine: &WorldEngine, key: &DomainKey) -> Node {
    engine
        .engine
        .session(key)
        .expect("home retained")
        .snapshot()
        .domains
        .iter()
        .find(|domain| domain.output == key.output && domain.workspace == key.workspace)
        .expect("home domain")
        .tree
        .clone()
        .expect("home tree")
}

fn d23_link_leaf(engine: &WorldEngine, key: &DomainKey, window: &WindowId) -> NodeId {
    engine
        .engine
        .session(key)
        .expect("home retained")
        .snapshot()
        .windows
        .iter()
        .find(|link| link.window == *window)
        .expect("window linked")
        .leaf
        .clone()
}

fn d23_focused_window(engine: &WorldEngine, key: &DomainKey) -> Option<String> {
    let session = engine.engine.session(key).expect("home retained");
    let (domain, leaf) = session.focus();
    let (domain, leaf) = (domain?, leaf?);
    session
        .snapshot()
        .windows
        .iter()
        .find(|link| {
            link.leaf == leaf && link.output == domain.output && link.workspace == domain.workspace
        })
        .map(|link| link.window.0.clone())
}

#[test]
fn d23_minimized_native_rehome_forgets_source_slot() {
    // D23 option B: the first confirmed per-window minimized rehome (same
    // lifetime, complete inventory) forgets the source restore slot, so a
    // later restore fresh-admits instead of rejoining the old group.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let away_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n1"))
        .expect("n1 workspace")
        .id
        .clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws.clone()),
    };
    let away_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(away_ws),
    };
    let win_b = WindowId("win-b".to_owned());
    // Admit A, then B at focused B, then C at focused B through real
    // reports: fresh admission splits the focused leaf, yielding nested
    // A / (B, C) grouping on the n0 workspace.
    let mut staged = host_native_report(2, vec![hfact("win-a", "n0", 0, true)], "n0", None, None);
    staged.focused_global = "win-a".to_owned();
    let _ = accepted(&mut engine, &staged);
    let mut staged = host_native_report(
        3,
        vec![
            hfact("win-a", "n0", 0, true),
            hfact("win-b", "n0", 100, true),
        ],
        "n0",
        None,
        None,
    );
    staged.focused_global = "win-b".to_owned();
    let _ = accepted(&mut engine, &staged);
    let mut staged = host_native_report(
        4,
        vec![
            hfact("win-a", "n0", 0, true),
            hfact("win-b", "n0", 100, true),
            hfact("win-c", "n0", 200, true),
        ],
        "n0",
        None,
        None,
    );
    staged.focused_global = "win-b".to_owned();
    let _ = accepted(&mut engine, &staged);
    // Confirm the starting tree nests B with C, apart from A.
    let pre_tree = d23_home_tree(&engine, &home_key);
    let b_leaf = d23_link_leaf(&engine, &home_key, &win_b);
    let c_leaf = d23_link_leaf(&engine, &home_key, &WindowId("win-c".to_owned()));
    let mut bc_group = d23_group_leaves(&pre_tree, &b_leaf).expect("B grouped");
    bc_group.sort();
    let mut bc_expected = vec![b_leaf.0.clone(), c_leaf.0.clone()];
    bc_expected.sort();
    assert_eq!(
        bc_group, bc_expected,
        "starting tree nests B with C: {pre_tree:?}"
    );
    // Minimize B in place; the hidden fact still reads, so the session
    // converges the omission into the WS1 restore slot.
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    let mut staged = host_native_report(
        5,
        vec![
            hfact("win-a", "n0", 0, true),
            b_min.clone(),
            hfact("win-c", "n0", 200, true),
        ],
        "n0",
        None,
        None,
    );
    staged.focused_global = "win-c".to_owned();
    let _ = accepted(&mut engine, &staged);
    let old_slot = engine
        .engine
        .session(&home_key)
        .expect("home retained")
        .minimized_slot(&win_b)
        .expect("minimize stores the source restore slot")
        .clone();
    assert_eq!(
        old_slot.tree, pre_tree,
        "slot preserves the pre-minimize C-group tree"
    );
    assert_eq!(old_slot.leaf, b_leaf, "slot preserves the old B leaf");
    // Confirmed per-window departure: B still minimized on the same output
    // but natively rehomed to n1 under a complete inventory.
    let mut b_away = b_min.clone();
    b_away.native_workspace = crate::world_report::NativeWorkspaceFact::Value("n1".to_owned());
    let reply6 = accepted(
        &mut engine,
        &host_native_report(
            6,
            vec![
                hfact("win-a", "n0", 0, true),
                b_away,
                hfact("win-c", "n0", 200, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let row_ws = reply6
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .map(|row| row.workspace.clone());
    let homing = engine.adapter_homing(&win_b);
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_none(),
        "first confirmed minimized rehome forgets the source slot; row_ws={row_ws:?} homing={homing:?}"
    );
    // Return leg: B back to n0 while still minimized. No slot exists to
    // resurrect, so none may appear at either home.
    let _ = accepted(
        &mut engine,
        &host_native_report(
            7,
            vec![
                hfact("win-a", "n0", 0, true),
                b_min.clone(),
                hfact("win-c", "n0", 200, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_none(),
        "return leg resurrects no slot"
    );
    assert!(
        engine
            .engine
            .session(&away_key)
            .is_none_or(|session| session.minimized_slot(&win_b).is_none()),
        "away home holds no slot either"
    );
    // Focus-settle leg: B still minimized while observed focus moves to
    // A on its own accepted report, so the restore below cannot inherit
    // a stale C focus. The settled session focus must read A.
    let mut focus_a = host_native_report(
        8,
        vec![
            hfact("win-a", "n0", 0, true),
            b_min.clone(),
            hfact("win-c", "n0", 200, true),
        ],
        "n0",
        None,
        None,
    );
    focus_a.focused_global = "win-a".to_owned();
    let _ = accepted(&mut engine, &focus_a);
    assert_eq!(
        d23_focused_window(&engine, &home_key),
        Some("win-a".to_owned()),
        "settled session focus is A before the restore"
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_none(),
        "focus move resurrects no slot"
    );
    // Restore leg: B unminimizes on n0 with settled focus on A. No slot
    // survives, so the restore must fresh-admit at A instead of
    // rejoining the old (B, C) group.
    let mut staged = host_native_report(
        9,
        vec![
            hfact("win-a", "n0", 0, true),
            hfact("win-b", "n0", 100, true),
            hfact("win-c", "n0", 200, true),
        ],
        "n0",
        None,
        None,
    );
    staged.focused_global = "win-a".to_owned();
    let reply9 = accepted(&mut engine, &staged);
    assert!(
        reply9.transfers.is_empty(),
        "restore performs no rehoming move: {reply9:?}"
    );
    let home_session = engine.engine.session(&home_key).expect("home retained");
    let new_b_leaf = home_session
        .snapshot()
        .windows
        .iter()
        .find(|link| link.window == win_b)
        .expect("restored B linked")
        .leaf
        .clone();
    let new_tree = d23_home_tree(&engine, &home_key);
    assert_ne!(
        new_tree, pre_tree,
        "fresh admission rewrites the topology, never restores the slot tree"
    );
    let mut new_group = d23_group_leaves(&new_tree, &new_b_leaf).expect("B grouped");
    new_group.sort();
    let a_leaf = d23_link_leaf(&engine, &home_key, &WindowId("win-a".to_owned()));
    let mut ab_expected = vec![a_leaf.0.clone(), new_b_leaf.0.clone()];
    ab_expected.sort();
    assert_eq!(
        new_group, ab_expected,
        "fresh admit groups B with settled-focus A, never the old C group: {new_tree:?}"
    );
    assert!(
        home_session.minimized_slot(&win_b).is_none(),
        "no slot survives the restore"
    );
    assert_eq!(
        engine.adapter_homing(&win_b),
        Some(("out-a".to_owned(), home_ws.clone())),
        "restored B homes at the source workspace"
    );
}

#[test]
fn d23_minimized_cross_output_rehome_forgets_first_slot() {
    // Cross-output minimized rehome: B minimized on out-a/n0, then the
    // same minimized lifetime reads on out-b/m0. The first source slot is
    // forgotten; the later restore tiles fresh at the destination.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &d23_two_screen_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let ob_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("m0"))
        .expect("m0 workspace")
        .id
        .clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws),
    };
    let ob_key = DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: WorkspaceId(ob_ws.clone()),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &d23_two_screen_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    let _ = accepted(
        &mut engine,
        &d23_two_screen_report(
            3,
            vec![hfact("win-a", "n0", 0, true), b_min.clone()],
            "n0",
            None,
            None,
        ),
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_some(),
        "minimize stores the source slot"
    );
    let mut b_ob = b_min.clone();
    b_ob.output = Some(OutputId("out-b".to_owned()));
    b_ob.native_workspace = crate::world_report::NativeWorkspaceFact::Value("m0".to_owned());
    let _ = accepted(
        &mut engine,
        &d23_two_screen_report(
            4,
            vec![hfact("win-a", "n0", 0, true), b_ob],
            "n0",
            None,
            None,
        ),
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_none(),
        "cross-output minimized rehome forgets the first source slot"
    );
    assert_eq!(
        engine.adapter_homing(&win_b),
        Some(("out-b".to_owned(), ob_ws.clone())),
        "departure homes at the destination"
    );
    // Restore at the destination tiles fresh there with no slot anywhere.
    let mut b_live = hfact("win-b", "m0", 100, true);
    b_live.output = Some(OutputId("out-b".to_owned()));
    let _ = accepted(
        &mut engine,
        &d23_two_screen_report(
            5,
            vec![hfact("win-a", "n0", 0, true), b_live],
            "n0",
            None,
            None,
        ),
    );
    let _ = d23_link_leaf(&engine, &ob_key, &win_b);
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_none(),
        "no slot resurrects at the source"
    );
    assert!(
        engine
            .engine
            .session(&ob_key)
            .is_none_or(|session| session.minimized_slot(&win_b).is_none()),
        "no slot lingers at the destination"
    );
    assert_eq!(
        engine.adapter_homing(&win_b),
        Some(("out-b".to_owned(), ob_ws)),
        "restored B homes at the destination"
    );
}

fn d23_two_screen_report(
    seq: u64,
    windows: Vec<WindowFact>,
    current_a: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    let mut out = report(seq, windows, vec![], command_seq, command, vec![], "");
    out.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    out.native = vec![
        crate::world_report::NativeObservation {
            output: OutputId("out-a".to_owned()),
            native_order: vec!["n0".to_owned(), "n1".to_owned()],
            current_native: Some(current_a.to_owned()),
            occupancy: vec![
                crate::world_report::NativeOccupancy {
                    native: "n0".to_owned(),
                    count: None,
                },
                crate::world_report::NativeOccupancy {
                    native: "n1".to_owned(),
                    count: None,
                },
            ],
        },
        crate::world_report::NativeObservation {
            output: OutputId("out-b".to_owned()),
            native_order: vec!["m0".to_owned()],
            current_native: Some("m0".to_owned()),
            occupancy: vec![crate::world_report::NativeOccupancy {
                native: "m0".to_owned(),
                count: None,
            }],
        },
    ];
    out
}

#[test]
fn d23_minimized_stable_native_retains_slot_and_restores() {
    // Ordinary minimize with a stable receipt retains the slot, and the
    // restore consumes it: the rehome forget above only fires on a
    // confirmed departure, never on stability.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &host_native_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
                hfact("win-c", "n0", 200, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    for seq in [3, 4] {
        let _ = accepted(
            &mut engine,
            &host_native_report(
                seq,
                vec![
                    hfact("win-a", "n0", 0, true),
                    b_min.clone(),
                    hfact("win-c", "n0", 200, true),
                ],
                "n0",
                None,
                None,
            ),
        );
        assert!(
            engine
                .engine
                .session(&home_key)
                .expect("home retained")
                .minimized_slot(&win_b)
                .is_some(),
            "stable minimized receipt retains the slot (seq {seq})"
        );
    }
    let _ = accepted(
        &mut engine,
        &host_native_report(
            5,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
                hfact("win-c", "n0", 200, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let home_session = engine.engine.session(&home_key).expect("home retained");
    assert!(
        home_session
            .snapshot()
            .windows
            .iter()
            .any(|link| link.window == win_b),
        "restored B tiles again"
    );
    assert!(
        home_session.minimized_slot(&win_b).is_none(),
        "restore consumes the slot"
    );
}

#[test]
fn d23_minimized_unknown_native_never_confirms_departure() {
    // Omitted/null/empty/unmapped native reads keep retained homing: none
    // of them may confirm a departure or disturb the slot.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &host_native_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
                hfact("win-c", "n0", 200, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    for (seq, native) in (3..).zip([
        crate::world_report::NativeWorkspaceFact::Omitted,
        crate::world_report::NativeWorkspaceFact::Null,
        crate::world_report::NativeWorkspaceFact::Value(String::new()),
        crate::world_report::NativeWorkspaceFact::Value("n9-unmapped".to_owned()),
    ]) {
        let mut b_unknown = b_min.clone();
        b_unknown.native_workspace = native;
        let _ = accepted(
            &mut engine,
            &host_native_report(
                seq,
                vec![
                    hfact("win-a", "n0", 0, true),
                    b_unknown,
                    hfact("win-c", "n0", 200, true),
                ],
                "n0",
                None,
                None,
            ),
        );
        assert!(
            engine
                .engine
                .session(&home_key)
                .expect("home retained")
                .minimized_slot(&win_b)
                .is_some(),
            "unknown native read retains the slot (seq {seq})"
        );
    }
}

fn d23_three_native_report(
    seq: u64,
    windows: Vec<WindowFact>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    let mut out = report(seq, windows, vec![], command_seq, command, vec![], "");
    out.native = vec![crate::world_report::NativeObservation {
        output: OutputId("out-a".to_owned()),
        native_order: vec!["n0".to_owned(), "n1".to_owned(), "n2".to_owned()],
        current_native: Some("n0".to_owned()),
        occupancy: vec![
            crate::world_report::NativeOccupancy {
                native: "n0".to_owned(),
                count: None,
            },
            crate::world_report::NativeOccupancy {
                native: "n1".to_owned(),
                count: None,
            },
            crate::world_report::NativeOccupancy {
                native: "n2".to_owned(),
                count: None,
            },
        ],
    }];
    out
}

#[test]
fn d23_minimized_incomplete_inventory_retains_slot() {
    // Gapped evidence never confirms a departure: the same minimized
    // rehome that forgets under a complete inventory retains the slot
    // when the inventory reads incomplete.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &host_native_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    let _ = accepted(
        &mut engine,
        &host_native_report(
            3,
            vec![hfact("win-a", "n0", 0, true), b_min.clone()],
            "n0",
            None,
            None,
        ),
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_some(),
        "minimize stores the source slot"
    );
    let mut b_away = b_min.clone();
    b_away.native_workspace = crate::world_report::NativeWorkspaceFact::Value("n1".to_owned());
    let mut staged = host_native_report(
        4,
        vec![hfact("win-a", "n0", 0, true), b_away],
        "n0",
        None,
        None,
    );
    staged.inventory_complete = false;
    let _ = accepted(&mut engine, &staged);
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_some(),
        "gapped inventory never confirms the departure"
    );
}

#[test]
fn d23_minimized_unsettled_transfer_retains_slot() {
    // A requested but unsettled per-window transfer blocks confirmation:
    // B is sent toward the n1 workspace, then minimized evidence names
    // the contradictory n2 workspace while the flight is outstanding.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(&mut engine, &d23_three_native_report(1, vec![], None, None));
    let by_native: std::collections::BTreeMap<String, String> = boot
        .workspaces
        .iter()
        .filter_map(|row| {
            row.native_binding
                .clone()
                .map(|native| (native, row.id.clone()))
        })
        .collect();
    let home_ws = by_native["n0"].clone();
    let away_ws = by_native["n1"].clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &d23_three_native_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
            ],
            None,
            None,
        ),
    );
    let mut send = d23_three_native_report(
        3,
        vec![
            hfact("win-a", "n0", 0, true),
            hfact("win-b", "n0", 100, true),
        ],
        Some(3),
        Some(WorldCommand::Send {
            window: win_b.clone(),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: away_ws.clone(),
            follow: false,
        }),
    );
    send.focused_global = "win-b".to_owned();
    let sent = accepted(&mut engine, &send);
    assert!(
        sent.transfers.iter().any(|t| matches!(
            t.kind,
            crate::world_report::TransferKind::Window(ref w) if *w == win_b
        )),
        "send flight retained: {:?}",
        sent.transfers
    );
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    let minimizing = accepted(
        &mut engine,
        &d23_three_native_report(
            4,
            vec![hfact("win-a", "n0", 0, true), b_min.clone()],
            None,
            None,
        ),
    );
    assert!(
        minimizing.transfers.iter().any(|t| matches!(
            t.kind,
            crate::world_report::TransferKind::Window(ref w) if *w == win_b
        )),
        "flight stays outstanding across the minimize: {:?}",
        minimizing.transfers
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_none(),
        "outstanding flight holds the sweep: no slot is stored mid-flight"
    );
    let mut b_third = b_min.clone();
    b_third.native_workspace = crate::world_report::NativeWorkspaceFact::Value("n2".to_owned());
    let held = accepted(
        &mut engine,
        &d23_three_native_report(5, vec![hfact("win-a", "n0", 0, true), b_third], None, None),
    );
    assert!(
        held.transfers.iter().any(|t| matches!(
            t.kind,
            crate::world_report::TransferKind::Window(ref w) if *w == win_b
        )),
        "contradicted flight stays outstanding: {:?}",
        held.transfers
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_none(),
        "unsettled transfer never confirms a departure"
    );
    // Arrival settles the flight normally: B tiles at the target.
    let mut arrival = d23_three_native_report(
        6,
        vec![
            hfact("win-a", "n0", 0, true),
            hfact("win-b", "n1", 100, true),
        ],
        None,
        None,
    );
    arrival.focused_global = "win-b".to_owned();
    let settled = accepted(&mut engine, &arrival);
    assert!(
        settled.transfers.is_empty(),
        "arrival settles the flight: {:?}",
        settled.transfers
    );
    assert_eq!(
        engine.adapter_homing(&win_b),
        Some(("out-a".to_owned(), away_ws)),
        "settled B homes at the target"
    );
}

#[test]
fn d23_minimized_refused_command_still_forgets() {
    // A refused command does not fence genuine observation: the same
    // report carries a refused selection plus a confirmed minimized
    // rehome, and the slot still forgets.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &host_native_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    let _ = accepted(
        &mut engine,
        &host_native_report(
            3,
            vec![hfact("win-a", "n0", 0, true), b_min.clone()],
            "n0",
            None,
            None,
        ),
    );
    let mut b_away = b_min.clone();
    b_away.native_workspace = crate::world_report::NativeWorkspaceFact::Value("n1".to_owned());
    let refused = accepted(
        &mut engine,
        &host_native_report(
            4,
            vec![hfact("win-a", "n0", 0, true), b_away],
            "n0",
            Some(4),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-nope".to_owned(),
            }),
        ),
    );
    assert!(
        matches!(
            refused.command_result,
            Some(crate::world_report::CommandResult::Rejected { .. })
        ),
        "unknown workspace refuses: {refused:?}"
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_none(),
        "refused command never fences a confirmed departure"
    );
}

#[test]
fn d23_minimized_unreadable_receipt_retains_slot() {
    // Unreadable frames are not minimized evidence: B present but
    // frame-unreadable on the n1 workspace keeps the source slot even
    // though the receipt moved.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &host_native_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    let _ = accepted(
        &mut engine,
        &host_native_report(
            3,
            vec![hfact("win-a", "n0", 0, true), b_min],
            "n0",
            None,
            None,
        ),
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_some(),
        "minimize stores the source slot"
    );
    let mut b_unreadable = hfact("win-b", "n1", 100, false);
    b_unreadable.minimized = Some(false);
    b_unreadable.rect = None;
    let _ = accepted(
        &mut engine,
        &host_native_report(
            4,
            vec![hfact("win-a", "n0", 0, true), b_unreadable],
            "n0",
            None,
            None,
        ),
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_some(),
        "unreadable frames never confirm a departure"
    );
}

#[test]
fn d23_minimized_workspace_migration_retains_relocated_slot() {
    // Whole-workspace migration is not a per-window rehome: with B
    // minimized and slotted at home, migrating the home workspace to
    // out-b relocates the slot with the workspace instead of forgetting
    // it, and B keeps a restore path at the new home.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &d23_two_screen_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let new_home_key = DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: WorkspaceId(home_ws.clone()),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &d23_two_screen_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    let _ = accepted(
        &mut engine,
        &d23_two_screen_report(
            3,
            vec![hfact("win-a", "n0", 0, true), b_min.clone()],
            "n0",
            None,
            None,
        ),
    );
    let mut migrate = d23_two_screen_report(
        4,
        vec![hfact("win-a", "n0", 0, true), b_min.clone()],
        "n0",
        Some(4),
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: home_ws,
        }),
    );
    migrate.focused_global = "win-a".to_owned();
    let migrated = accepted(&mut engine, &migrate);
    assert!(
        migrated
            .transfers
            .iter()
            .any(|t| matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "migration flight retained: {:?}",
        migrated.transfers
    );
    assert!(
        engine
            .engine
            .session(&new_home_key)
            .is_some_and(|session| session.minimized_slot(&win_b).is_some()),
        "migration relocates the slot with the workspace, never forgets it"
    );
}

#[test]
fn d23_minimized_close_wins_over_rehome() {
    // A native close arriving with a minimized rehome wins: the closed
    // lifetime leaves no slot behind at either home.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &host_native_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let mut b_min = hfact("win-b", "n0", 100, false);
    b_min.minimized = Some(true);
    b_min.rect = None;
    let _ = accepted(
        &mut engine,
        &host_native_report(
            3,
            vec![hfact("win-a", "n0", 0, true), b_min.clone()],
            "n0",
            None,
            None,
        ),
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_some(),
        "minimize stores the source slot"
    );
    // A native close of the minimized lifetime arrives as omission plus
    // a close entry (never a live fact plus a close): the close path owns
    // the slot, so no stale slot survives at the source home.
    let mut staged = host_native_report(4, vec![hfact("win-a", "n0", 0, true)], "n0", None, None);
    staged.closed = vec![win_b.clone()];
    let _ = accepted(&mut engine, &staged);
    assert!(
        engine.adapter_homing(&win_b).is_none(),
        "closed lifetime drops membership"
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .is_none_or(|session| session.minimized_slot(&win_b).is_none()),
        "close leaves no slot behind"
    );
}

#[test]
fn d23_minimized_same_turn_rehome_forgets_slot() {
    // Minimize and native rehome in a single report: the sweep stores the
    // source slot while converging, and the post-sweep forget still clears
    // it, so no stale slot survives the turn.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let boot = accepted(
        &mut engine,
        &host_native_report(1, vec![], "n0", None, None),
    );
    let home_ws = boot
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n0"))
        .expect("n0 workspace")
        .id
        .clone();
    let home_key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(home_ws),
    };
    let win_b = WindowId("win-b".to_owned());
    let _ = accepted(
        &mut engine,
        &host_native_report(
            2,
            vec![
                hfact("win-a", "n0", 0, true),
                hfact("win-b", "n0", 100, true),
                hfact("win-c", "n0", 200, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    let mut b_moved = hfact("win-b", "n1", 100, false);
    b_moved.minimized = Some(true);
    b_moved.rect = None;
    let _ = accepted(
        &mut engine,
        &host_native_report(
            3,
            vec![
                hfact("win-a", "n0", 0, true),
                b_moved,
                hfact("win-c", "n0", 200, true),
            ],
            "n0",
            None,
            None,
        ),
    );
    assert!(
        engine
            .engine
            .session(&home_key)
            .expect("home retained")
            .minimized_slot(&win_b)
            .is_none(),
        "same-turn minimize+rehome leaves no source slot"
    );
}
