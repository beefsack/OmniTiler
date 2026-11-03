//! Native-free suppression-capable explicit selection visibility/readiness
//! prerequisite: an actual explicit `SelectWorkspace`/`SwitchView` routed
//! through `retain_view` that resolves a focus target publishes same-output
//! selected-home shows plus departing-home hides under one authorization
//! revision with an `awaitShown` focus grant. Ordinary focus, admission,
//! hover, sends, and every out-of-scope route keep `awaitShown=false` with
//! no visibility targets. No `SwitchEvent` is emitted and no selector
//! completion is claimed here.

use std::collections::BTreeSet;

use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    ArrivalBracket, ArrivalLocation, AuthorityScope, FocusFeedback, FrameSetter, HeldScreenRow,
    MemberAttempt, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane, ScopeAuthority,
    VisibilityFact, VisibilitySample, VisibilitySetter, WORLD_CONTRACT_VERSION, WindowFact,
    WorkspaceRef, WorldCommand, WorldReport,
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
        contract_version: WORLD_CONTRACT_VERSION,
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
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
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
    assert_eq!(binding.effect, *effect);
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
    match engine.handle_report(&report(
        6,
        vec![sfact("w1", "out-a", 0), sfact("w2", "out-a", 100)],
        vec![],
        None,
        None,
        vec![row],
        "",
    )) {
        WorldOutcome::Rejected { kind, message } => {
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
    match engine.handle_report(&report(
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
        WorldOutcome::CommandConsumed(_) => {}
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
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { .. } => {}
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
fn selection_without_suppression_caps_grants_nothing() {
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
    assert_eq!(reply.effects.len(), 1, "ordinary focus still resolves");
    assert!(
        !reply.effects[0].await_shown,
        "no suppression means no await"
    );
    for row in &reply.windows {
        assert!(row.visibility.is_none(), "no targets without the caps");
    }
    assert!(
        engine.retained_selection.is_empty(),
        "no bindings without the caps"
    );
    assert!(
        engine.selection_targets.is_empty(),
        "no targets without the caps"
    );
}

#[test]
fn selection_host_owned_engine_grants_nothing() {
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
        assert!(row.visibility.is_none(), "host-owned grants no targets");
    }
    assert!(
        engine.retained_selection.is_empty(),
        "host-owned keeps no bindings"
    );
}

#[test]
fn empty_selection_grants_nothing_and_drops_stale_shows() {
    let (mut engine, _home, away) = select_setup();
    let granted = select(&mut engine, 3, &away);
    assert!(row(&granted, "w2").visibility.is_some());
    // Occupying the trailing spare mints a fresh trailing spare, which is
    // empty: selecting it resolves no focus, and the superseding intent
    // drops the stale controlled targets without completing the route.
    let ring = ring_ids(&granted, "out-a");
    assert!(
        ring.len() >= 3,
        "occupied spare mints a fresh spare: {ring:?}"
    );
    let empty = ring.last().expect("fresh trailing spare").clone();
    assert_ne!(empty, away, "spare is not the occupied workspace");
    let reply = select(&mut engine, 4, &empty);
    assert!(reply.effects.is_empty(), "no focus invented: {reply:?}");
    for row in &reply.windows {
        assert!(
            row.visibility.is_none() || row.visibility_authority != ScopeAuthority::Controlled,
            "stale shows drop without completing the route: {row:?}"
        );
    }
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "superseded binding drops with the new intent"
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
    // A follow send moves the core view back outside `retain_view`'s
    // scope-pruning path: the stale hide on the now-current home must not
    // stay Controlled beside the ordinary follow focus.
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
        !followed.effects[0].await_shown,
        "legacy follow behavior unchanged: no arrival grant"
    );
    for row in &followed.windows {
        assert_ne!(
            row.visibility_authority,
            ScopeAuthority::Controlled,
            "no stale Controlled hide survives the view move: {row:?}"
        );
    }
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "superseded binding clears with the view move"
    );
    assert!(
        engine
            .selection_targets
            .values()
            .all(|entry| entry.home.output.0 != "out-a"),
        "superseded targets clear with the view move"
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
        held_binding.effect, grant_effect,
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
        binding.effect, grant_effect,
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
