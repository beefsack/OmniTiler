//! Compact automatic-effect identity regressions: the real admission and
//! focus-follows-cursor producers must emit effect ids that pass the
//! opaque-id bound even for maximum-length subjects.

use crate::bounds::is_opaque_id;
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_focus::FocusVerdict;
use crate::world_report::{
    FocusFeedback, PresentationRowV4, RowDiagnosticKind, VisibilityReason, VisibilitySetter,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
};
use crate::world_v4_support_tests::empty_ingress;

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> crate::world_report::ScreenReport {
    crate::world_report::ScreenReport {
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

fn report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    cursor: Option<(i32, i32)>,
    cursor_hit: Option<&str>,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: None,
        command: None,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        windows,
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor,
        cursor_hit: cursor_hit.map(|hit| WindowId(hit.to_owned())),
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
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
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(empty_ingress()),
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

#[allow(clippy::too_many_arguments)]
fn feedback_row(
    id: &str,
    target: &str,
    revision: u64,
    setter: VisibilitySetter,
    reason: VisibilityReason,
    pre: Option<&str>,
    post: Option<&str>,
    proven: bool,
    attempt_ms: u64,
) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId(target.to_owned()),
        revision,
        setter,
        reason,
        pre: pre.map(|id| WindowId(id.to_owned())),
        post: post.map(|id| WindowId(id.to_owned())),
        foreground_proven: proven,
        arrival: Vec::new(),
        attempt_ms,
    }
}

fn with_feedback(mut base: WorldReportV4, rows: Vec<FocusFeedback>) -> WorldReportV4 {
    base.focus_feedback = rows;
    base
}

fn incomplete(mut base: WorldReportV4) -> WorldReportV4 {
    base.inventory_complete = false;
    base
}

fn max_id(prefix: char, last: char) -> String {
    let mut id = prefix.to_string().repeat(127);
    id.push(last);
    id
}

fn admission_engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn hover_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "focusFollowsCursor".to_owned(),
            crate::connection::RawSettingValue::Bool(true),
        )],
    )
    .expect("valid generation")
}

#[test]
fn admission_effect_id_stays_within_opaque_bound_at_max_subject() {
    let target = "w".repeat(128);
    let mut engine = admission_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], "", None, None),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact(&target, "out-a", 100)],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let request = reply.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, target);
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    assert!(
        is_opaque_id(&request.id),
        "admission id passes the opaque bound: {:?}",
        request.id
    );
    // Feeding the emitted tuple back as a typed row settles the retained
    // grant as proven, with no new effects and no retry.
    let emitted = engine.retained_focus[0].effect.clone();
    let live = feedback_row(
        &emitted.id,
        &target,
        emitted.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("win-a"),
        Some(&target),
        true,
        9,
    );
    let settled = accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("win-a", "out-a", 0), fact(&target, "out-a", 100)],
                &target,
                None,
                None,
            ),
            vec![live.clone()],
        ),
    );
    assert!(
        settled.effects.is_empty(),
        "feedback mints nothing: {settled:?}"
    );
    assert_eq!(engine.retained_focus.len(), 1);
    assert_eq!(engine.retained_focus[0].feedback, Some(live));
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
}

#[test]
fn hover_effect_id_stays_within_opaque_bound_at_max_subject() {
    let target = "w".repeat(128);
    let mut engine = hover_engine();
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact(&target, "out-a", 100)],
            "win-a",
            Some((10, 10)),
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact(&target, "out-a", 100)],
            "win-a",
            Some((110, 10)),
            Some(&target),
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let request = reply.focus_request.as_ref().expect("hover request");
    assert_eq!(request.target.0, target);
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::FocusFollowsCursor
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    assert!(
        is_opaque_id(&request.id),
        "hover id passes the opaque bound: {:?}",
        request.id
    );
    // Feeding the emitted tuple back as a typed row settles the retained
    // grant as proven, with no new effects and no retry. The cursor stays
    // static and the observed focus has caught up to the hover target, so
    // no re-mint occurs.
    let emitted = engine.retained_focus[0].effect.clone();
    let live = feedback_row(
        &emitted.id,
        &target,
        emitted.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("win-a"),
        Some(&target),
        true,
        9,
    );
    let settled = accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("win-a", "out-a", 0), fact(&target, "out-a", 100)],
                &target,
                Some((110, 10)),
                Some(&target),
            ),
            vec![live.clone()],
        ),
    );
    assert!(
        settled.effects.is_empty(),
        "feedback mints nothing: {settled:?}"
    );
    assert_eq!(engine.retained_focus.len(), 1);
    assert_eq!(engine.retained_focus[0].feedback, Some(live));
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
}

#[test]
fn suppressed_same_target_automatic_candidate_keeps_token_for_later_hover() {
    // A newcomer arriving with an explicit Focus command on the same
    // window publishes only the explicit row: the automatic candidate's
    // token is allocated before the same-target dedup drops the admission
    // row in favor of the explicit winner, and the suppressed report never
    // reaches the hover gate. A later genuine hover reuses that token with
    // a fresh issuing revision, with no collision and no new token.
    let mut engine = hover_engine();
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            Some((10, 10)),
            None,
        ),
    );
    let mut arrival = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        Some((10, 10)),
        None,
    );
    arrival.command_seq = Some(2);
    arrival.command = Some(WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: "right".to_owned(),
    });
    let commanded = accepted(&mut engine, &arrival);
    assert!(
        commanded.effects.is_empty(),
        "focus never publishes as wire effects: {commanded:?}"
    );
    let request = commanded
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(request.id, format!("focus-{}", commanded.state_revision));
    assert_eq!(request.issued_revision, commanded.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(2));
    assert_eq!(request.after_shown, None);
    let minted = commanded.state_revision;
    assert_eq!(
        engine
            .automatic_focus_tokens
            .get(&WindowId("win-b".to_owned())),
        Some(&minted),
        "the dropped automatic candidate retains its R2 token"
    );
    assert_eq!(
        engine.ffc_last_hover, None,
        "the suppressed report never reaches the hover gate"
    );
    let hovered = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
        ),
    );
    assert!(
        hovered.effects.is_empty(),
        "focus never publishes as wire effects: {hovered:?}"
    );
    let request = hovered.focus_request.as_ref().expect("hover request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(request.id, format!("focus-{}", hovered.state_revision));
    assert_eq!(request.issued_revision, hovered.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::FocusFollowsCursor
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    assert!(
        is_opaque_id(&request.id),
        "hover id passes the opaque bound: {:?}",
        request.id
    );
    assert!(
        request.issued_revision > minted,
        "fresh issuing revision on the retained token"
    );
    assert_eq!(
        engine
            .automatic_focus_tokens
            .get(&WindowId("win-b".to_owned())),
        Some(&minted),
        "no new token"
    );
}

#[test]
fn admission_token_is_first_issuing_revision_and_stable_across_observations() {
    let mut engine = admission_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], "", None, None),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let request = reply.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    // The token is the first automatic-focus issuing revision for the
    // lifetime: the reply revision of the minting report.
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert!(is_opaque_id(&request.id));
    let token = *engine
        .automatic_focus_tokens
        .get(&WindowId("win-b".to_owned()))
        .expect("mapping retained");
    assert_eq!(token, reply.state_revision);
    // Observation-only echoes mint nothing and reuse the mapping.
    for seq in [3u64, 4] {
        let echo = accepted(
            &mut engine,
            &report(
                seq,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
                "win-b",
                None,
                None,
            ),
        );
        assert!(echo.effects.is_empty(), "echo mints nothing: {echo:?}");
    }
    assert_eq!(
        engine
            .automatic_focus_tokens
            .get(&WindowId("win-b".to_owned())),
        Some(&token),
        "observation advances never renew the token"
    );
}

#[test]
fn distinct_same_prefix_subjects_get_distinct_tokens() {
    // Two maximum-length subjects sharing a 127-byte prefix intern
    // different tokens: identity comes from the issuing revision, never
    // from a truncated spelling.
    let first = max_id('w', 'x');
    let second = max_id('w', 'y');
    assert_ne!(first, second);
    let mut engine = admission_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], "", None, None),
    );
    let one = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact(&first, "out-a", 100)],
            "win-a",
            None,
            None,
        ),
    );
    let two = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact(&first, "out-a", 100),
                fact(&second, "out-a", 200),
            ],
            &first,
            None,
            None,
        ),
    );
    assert!(
        one.effects.is_empty() && two.effects.is_empty(),
        "focus never publishes as wire effects: {one:?} {two:?}"
    );
    let one_request = one.focus_request.as_ref().expect("admission request");
    let two_request = two.focus_request.as_ref().expect("admission request");
    assert_eq!(one_request.id, format!("focus-{}", one.state_revision));
    assert_eq!(two_request.id, format!("focus-{}", two.state_revision));
    assert_ne!(one_request.id, two_request.id, "tokens differ");
    assert!(
        is_opaque_id(&one_request.id) && is_opaque_id(&two_request.id),
        "both compact: {:?} {:?}",
        one_request.id,
        two_request.id
    );
}

#[test]
fn repeated_hover_retains_token_through_move_away_and_back() {
    let mut engine = hover_engine();
    accepted(
        &mut engine,
        &report(
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-a",
            Some((10, 10)),
            None,
        ),
    );
    let first = accepted(
        &mut engine,
        &report(
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
        ),
    );
    assert!(
        first.effects.is_empty(),
        "focus never publishes as wire effects: {first:?}"
    );
    let first_request = first.focus_request.as_ref().expect("hover request");
    assert_eq!(first_request.target.0, "win-b");
    assert_eq!(first_request.id, format!("focus-{}", first.state_revision));
    assert_eq!(first_request.issued_revision, first.state_revision);
    assert_eq!(
        first_request.cause,
        crate::world_report::FocusRequestCause::FocusFollowsCursor
    );
    assert_eq!(first_request.cmd_seq, None);
    assert_eq!(first_request.after_shown, None);
    assert!(is_opaque_id(&first_request.id));
    let first_id = first_request.id.clone();
    let first_token = *engine
        .automatic_focus_tokens
        .get(&WindowId("win-b".to_owned()))
        .expect("hover token retained");
    let away = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-a",
            Some((210, 10)),
            Some("win-c"),
        ),
    );
    assert!(
        away.effects.is_empty(),
        "focus never publishes as wire effects: {away:?}"
    );
    let away_request = away.focus_request.as_ref().expect("hover request");
    assert_eq!(away_request.target.0, "win-c");
    assert_eq!(away_request.id, format!("focus-{}", away.state_revision));
    assert_ne!(
        away_request.id, first_id,
        "different lifetime, different token"
    );
    assert!(is_opaque_id(&away_request.id));
    let back = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-a",
            Some((115, 10)),
            Some("win-b"),
        ),
    );
    assert!(
        back.effects.is_empty(),
        "focus never publishes as wire effects: {back:?}"
    );
    let back_request = back.focus_request.as_ref().expect("hover request");
    assert_eq!(back_request.target.0, "win-b");
    assert_eq!(back_request.id, format!("focus-{}", back.state_revision));
    assert_eq!(back_request.issued_revision, back.state_revision);
    assert!(is_opaque_id(&back_request.id));
    assert_eq!(
        engine
            .automatic_focus_tokens
            .get(&WindowId("win-b".to_owned())),
        Some(&first_token),
        "same lifetime reuses its token across the transition"
    );
}

#[test]
fn token_survives_incomplete_held_and_engine_known_absence() {
    let mut engine = admission_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], "", None, None),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    {
        let request = reply.focus_request.as_ref().expect("admission request");
        assert_eq!(request.target.0, "win-b");
        assert_eq!(request.id, format!("focus-{}", reply.state_revision));
        assert!(is_opaque_id(&request.id));
    }
    let token = reply
        .focus_request
        .as_ref()
        .expect("admission request")
        .issued_revision;
    // An incomplete gap that drops the fact keeps the mapping: no
    // positional departure is proven without a complete inventory.
    let mut gap = report(3, vec![fact("win-a", "out-a", 0)], "win-a", None, None);
    gap = incomplete(gap);
    accepted(&mut engine, &gap);
    assert_eq!(
        engine
            .automatic_focus_tokens
            .get(&WindowId("win-b".to_owned())),
        Some(&token),
        "incomplete absence retains"
    );
    // A narrowly held row keeps it too.
    let mut held = report(4, vec![fact("win-a", "out-a", 0)], "win-a", None, None);
    held.held_windows = vec![crate::world_report::HeldWindowRow {
        window: WindowId("win-b".to_owned()),
        ordinal: 1,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    accepted(&mut engine, &held);
    assert_eq!(
        engine
            .automatic_focus_tokens
            .get(&WindowId("win-b".to_owned())),
        Some(&token),
        "held presence retains"
    );
    // Complete absence of an engine-known (admitted, session-retained)
    // lifetime is not proven departure either.
    accepted(
        &mut engine,
        &report(5, vec![fact("win-a", "out-a", 0)], "win-a", None, None),
    );
    assert_eq!(
        engine
            .automatic_focus_tokens
            .get(&WindowId("win-b".to_owned())),
        Some(&token),
        "engine-known absence retains"
    );
    // A returning lifetime never re-mints: the spent fence and the
    // mapping agree.
    let echo = accepted(
        &mut engine,
        &report(
            6,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-b",
            None,
            None,
        ),
    );
    assert!(echo.effects.is_empty(), "no replay: {echo:?}");
    assert_eq!(
        engine
            .automatic_focus_tokens
            .get(&WindowId("win-b".to_owned())),
        Some(&token)
    );
}

#[test]
fn temporary_slot_exclusion_never_renews_the_mapping() {
    // Direct prune-fence check: a same-lifetime temporary slot exclusion
    // retains the mapping through a complete report that otherwise drops
    // the fact, while the same report without the exclusion prunes it.
    // This mirrors the `world_focus` departure fence (no positional
    // iterator): the spent authority must never renew for one lifetime.
    let mut engine = admission_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], "", None, None),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    {
        let request = reply.focus_request.as_ref().expect("admission request");
        assert_eq!(request.target.0, "win-b");
        assert_eq!(request.id, format!("focus-{}", reply.state_revision));
        assert!(is_opaque_id(&request.id));
    }
    let token = reply
        .focus_request
        .as_ref()
        .expect("admission request")
        .issued_revision;
    let target = WindowId("win-b".to_owned());
    let probe = report(3, vec![fact("win-a", "out-a", 0)], "win-a", None, None);
    // Explicit close always prunes: run the real close report so the
    // spent fence and the token map drop together, then re-admit.
    let mut closed = probe.clone();
    closed.closed = vec![target.clone()];
    let closed_reply = accepted(&mut engine, &closed);
    assert!(
        closed_reply.effects.is_empty(),
        "close mints nothing: {closed_reply:?}"
    );
    assert!(
        !engine.automatic_focus_tokens.contains_key(&target),
        "explicit close prunes"
    );
    // Re-mint, then prove the exclusion arm keeps the mapping.
    let readmit = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        readmit.effects.is_empty(),
        "focus never publishes as wire effects: {readmit:?}"
    );
    let readmit_request = readmit.focus_request.as_ref().expect("admission request");
    assert_eq!(readmit_request.target.0, "win-b");
    assert_eq!(
        readmit_request.id,
        format!("focus-{}", readmit.state_revision)
    );
    assert_ne!(
        readmit_request.issued_revision, token,
        "fresh lifetime, fresh token"
    );
    // Isolate the exclusion-only fence on a fresh engine seeded with the
    // actually emitted token: explicitly stage the probe (win-a only, so
    // win-b is neither live nor held) before adding the exclusion, so
    // retention proves the exclusion arm rather than staged presence. The
    // fresh engine holds no member/session knowledge of win-b, isolating
    // the engine/member-known arm as well.
    let mut fenced = admission_engine();
    fenced
        .automatic_focus_tokens
        .insert(target.clone(), readmit_request.issued_revision);
    fenced.stage_facts_v4(&probe);
    fenced.staged.excluded.insert(target.clone());
    fenced.prune_automatic_focus_tokens();
    assert_eq!(
        fenced.automatic_focus_tokens.get(&target),
        Some(&readmit_request.issued_revision),
        "temporary slot exclusion retains the mapping"
    );
    // Clearing the exclusion then pruning drops the token: with neither
    // live, held, excluded, nor engine/member-known presence, the fence
    // is exact and never renews the mapping.
    fenced.staged.excluded.clear();
    fenced.prune_automatic_focus_tokens();
    assert!(
        !fenced.automatic_focus_tokens.contains_key(&target),
        "exclusion-only fence drops once the exclusion lifts"
    );
}

#[test]
fn close_reuse_mints_fresh_token_and_stale_feedback_cannot_settle() {
    let mut engine = admission_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], "", None, None),
    );
    let first = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        first.effects.is_empty(),
        "focus never publishes as wire effects: {first:?}"
    );
    let first_request = first.focus_request.as_ref().expect("admission request");
    assert_eq!(first_request.target.0, "win-b");
    assert_eq!(first_request.id, format!("focus-{}", first.state_revision));
    assert!(is_opaque_id(&first_request.id));
    let stale_id = engine.retained_focus[0].effect.id.clone();
    let stale_rev = engine.retained_focus[0].effect.issued_revision;
    let mut closed = report(3, vec![fact("win-a", "out-a", 0)], "win-a", None, None);
    closed.closed = vec![WindowId("win-b".to_owned())];
    let closed_reply = accepted(&mut engine, &closed);
    assert!(
        closed_reply.effects.is_empty(),
        "close mints nothing: {closed_reply:?}"
    );
    assert!(
        !engine
            .automatic_focus_tokens
            .contains_key(&WindowId("win-b".to_owned())),
        "close drops the mapping"
    );
    let readmit = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        readmit.effects.is_empty(),
        "focus never publishes as wire effects: {readmit:?}"
    );
    let readmit_request = readmit.focus_request.as_ref().expect("admission request");
    assert_eq!(readmit_request.target.0, "win-b");
    assert_eq!(
        readmit_request.id,
        format!("focus-{}", readmit.state_revision)
    );
    assert_ne!(readmit_request.id, stale_id, "fresh token on reuse");
    assert!(is_opaque_id(&readmit_request.id));
    assert_eq!(engine.retained_focus.len(), 1);
    // Stale feedback for the previous lifetime cannot settle the new
    // grant: full opaque tuple mismatch.
    let stale = feedback_row(
        &stale_id,
        "win-b",
        stale_rev,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("win-a"),
        Some("win-b"),
        true,
        9,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                5,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
                "win-b",
                None,
                None,
            ),
            vec![stale],
        ),
    );
    assert_eq!(
        engine.retained_focus[0].feedback, None,
        "stale tuple settles nothing"
    );
    // Fresh feedback with the new tuple settles and correlates.
    let fresh = engine.retained_focus[0].effect.clone();
    let live = feedback_row(
        &fresh.id,
        "win-b",
        fresh.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("win-a"),
        Some("win-b"),
        true,
        10,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                6,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
                "win-b",
                None,
                None,
            ),
            vec![live],
        ),
    );
    assert!(engine.retained_focus[0].feedback.is_some());
    assert_eq!(
        engine.retained_focus[0]
            .feedback
            .as_ref()
            .expect("feedback")
            .id,
        fresh.id
    );
}

#[test]
fn duplicate_cache_and_consumed_command_mint_no_new_identities() {
    let mut engine = admission_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], "", None, None),
    );
    let arrival = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        None,
        None,
    );
    let reply = accepted(&mut engine, &arrival);
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    {
        let request = reply.focus_request.as_ref().expect("admission request");
        assert_eq!(request.target.0, "win-b");
        assert_eq!(request.id, format!("focus-{}", reply.state_revision));
        assert!(is_opaque_id(&request.id));
    }
    let revision = reply.state_revision;
    let tokens = engine.automatic_focus_tokens.clone();
    // An exact duplicate returns the cached reply without running the
    // producers again.
    match engine.handle_report(&arrival, &[]).0 {
        WorldOutcome::Duplicate(cached) => {
            assert_eq!(cached.state_revision, revision);
        }
        other => panic!("expected cached duplicate, got {other:?}"),
    }
    assert_eq!(
        engine.automatic_focus_tokens, tokens,
        "duplicate mints nothing"
    );
    assert_eq!(engine.state_revision, revision, "duplicate mutates nothing");
    // A consumed command reconciles without re-executing or minting.
    let mut commanded = report(
        3,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-b",
        None,
        None,
    );
    commanded.command_seq = Some(7);
    commanded.command = Some(WorldCommand::Focus {
        window: WindowId("win-a".to_owned()),
        direction: "left".to_owned(),
    });
    let focus_reply = accepted(&mut engine, &commanded);
    assert!(
        focus_reply.effects.is_empty(),
        "focus never publishes as wire effects: {focus_reply:?}"
    );
    {
        let request = focus_reply
            .focus_request
            .as_ref()
            .expect("explicit focus request");
        assert_eq!(request.target.0, "win-a");
        assert_eq!(request.id, format!("focus-{}", focus_reply.state_revision));
        assert_eq!(
            request.cause,
            crate::world_report::FocusRequestCause::ExplicitCommand
        );
        assert_eq!(request.cmd_seq, Some(7));
        assert_eq!(request.after_shown, None);
    }
    let after_focus = engine.automatic_focus_tokens.clone();
    let mut replay = commanded.clone();
    replay.request_seq = 4;
    replay.observation_seq = 4;
    replay.request_id = "req-4".to_owned();
    let consumed = match engine.handle_report(&replay, &[]).0 {
        WorldOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected consumed command, got {other:?}"),
    };
    assert!(
        consumed.effects.is_empty(),
        "consumed command mints nothing: {consumed:?}"
    );
    assert_eq!(
        engine.automatic_focus_tokens, after_focus,
        "consumed path mints no identities"
    );
}

#[test]
fn rejected_command_keeps_admission_mapping_envelope_rejection_adopts_nothing() {
    let mut engine = admission_engine();
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    // A refused command on an accepted report still publishes the
    // pre-command admission effect and keeps its mapping.
    let mut refused = report(
        2,
        vec![
            fact("w0", "out-a", 0),
            fact("w1", "out-a", 100),
            fact("w2", "out-a", 200),
        ],
        "w1",
        None,
        None,
    );
    refused.command_seq = Some(2);
    refused.command = Some(WorldCommand::Focus {
        window: WindowId("w-unknown".to_owned()),
        direction: "left".to_owned(),
    });
    let reply = accepted(&mut engine, &refused);
    assert_eq!(
        reply.command_result,
        Some(crate::world_report::CommandResult::Rejected {
            kind: crate::world_report::CommandRejectKind::FocusUnknown
        })
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let admission = reply
        .focus_request
        .as_ref()
        .expect("admission survives refusal");
    assert_eq!(admission.target.0, "w2");
    assert_eq!(admission.id, format!("focus-{}", reply.state_revision));
    assert_eq!(
        admission.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(admission.cmd_seq, None);
    assert_eq!(admission.after_shown, None);
    assert!(is_opaque_id(&admission.id));
    assert_eq!(
        engine
            .automatic_focus_tokens
            .get(&WindowId("w2".to_owned())),
        Some(&admission.issued_revision),
        "accepted observation plus rejected command retains the mapping"
    );
    let revision = reply.state_revision;
    let tokens = engine.automatic_focus_tokens.clone();
    // An envelope refusal (reused sequence, changed body) adopts
    // nothing: no mapping, no revision motion.
    let mut tampered = refused.clone();
    tampered.focused_global = "w2".to_owned();
    tampered.foreground_known = true;
    match engine.handle_report(&tampered, &[]).0 {
        WorldOutcome::Rejected { .. } => {}
        other => panic!("expected envelope rejection, got {other:?}"),
    }
    assert_eq!(
        engine.automatic_focus_tokens, tokens,
        "envelope refusal adopts nothing"
    );
    assert_eq!(
        engine.state_revision, revision,
        "envelope refusal mutates nothing"
    );
}

#[test]
fn max_revision_boundary_mints_no_unissued_identity() {
    let mut engine = admission_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], "", None, None),
    );
    engine.state_revision = u64::MAX - 1;
    // One slot remains: the token is exactly u64::MAX, still a compact
    // valid identity and the last admissible issuing revision.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    {
        let request = reply.focus_request.as_ref().expect("admission request");
        assert_eq!(request.target.0, "win-b");
        assert_eq!(request.issued_revision, u64::MAX);
        assert_eq!(request.id, format!("focus-{}", u64::MAX));
        assert!(is_opaque_id(&request.id));
    }
    assert_eq!(reply.state_revision, u64::MAX);
    let tokens = engine.automatic_focus_tokens.clone();
    // The next report refuses before staging: no new mapping, no
    // unissued identity, no mutation.
    let probe = report(
        3,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ],
        "win-b",
        None,
        None,
    );
    match engine.handle_report(&probe, &[]).0 {
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, crate::world_engine::KIND_REVISION_EXHAUSTED);
        }
        other => panic!("expected revision exhaustion, got {other:?}"),
    }
    assert_eq!(engine.automatic_focus_tokens, tokens);
    assert_eq!(engine.state_revision, u64::MAX);
}
