//! Admission focus, close-survivor focus, and send-stay source focus at the
//! world boundary: bootstrap seeds stay quiet, genuinely new lifetimes focus
//! at most once (reopen refocuses), overlays suppress automatic activation,
//! closes fall back to a surviving resident, view returns preserve the
//! remembered leaf, and stay sends pin exact source focus. Logical-first:
//! screens plus window facts only.

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{StaySourceFocus, WorldEngine, WorldOutcome};
use crate::world_report::{
    FrameSetter, MemberAttempt, PresentationIngressV4, PresentationRowV4, ScreenReport,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorkspaceRef, WorldCommand, WorldReportV4,
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

fn overlay(window: &str, output: &str, x: i32, fullscreen: bool, maximized: bool) -> WindowFact {
    WindowFact {
        fullscreen: Some(fullscreen),
        maximized: Some(maximized),
        ..fact(window, output, x)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    closed: Vec<&str>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReportV4 {
    // v4 carries no view-echo lane; the select-return echoes here were
    // inert (commands + member facts carry the proof), so the v4 port
    // passes an empty lane.
    assert!(
        views.is_empty(),
        "v4 has no views lane: {} echo(s) cannot convert",
        views.len()
    );
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
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

fn view_of(reply: &crate::world_report::WorldReplyV4, output: &str) -> String {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
        .expect("logical view")
}

/// Committed focus window for one logical workspace, via the retained
/// session snapshot (leaf identity to window link).
fn committed_focus(engine: &WorldEngine, output: &str, workspace: &str) -> Option<String> {
    let key = crate::session::DomainKey {
        output: OutputId(output.to_owned()),
        workspace: crate::directional::WorkspaceId(workspace.to_owned()),
    };
    let session = engine.engine().session(&key)?;
    let (domain, leaf) = session.focus();
    let (domain, leaf) = (domain?, leaf?);
    if domain != key {
        return None;
    }
    session
        .snapshot()
        .windows
        .iter()
        .find(|link| link.leaf == leaf)
        .map(|link| link.window.0.clone())
}

#[test]
fn bootstrap_seed_is_quiet_then_first_newcomer_focuses_once() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    // Fresh seeds emit nothing: the observed native focus stays authoritative.
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        first.effects.is_empty(),
        "bootstrap quiet: {:?}",
        first.effects
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(second.effects.is_empty(), "re-observation quiet");
    // A genuinely new lifetime mints exactly one admission intent.
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        third.effects.is_empty(),
        "focus never publishes as wire effects: {third:?}"
    );
    let request = third.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "win-c");
    assert_eq!(request.id, format!("focus-{}", third.state_revision));
    assert_eq!(request.issued_revision, third.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    // Never replayed.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        fourth.effects.is_empty(),
        "at-most-once: {:?}",
        fourth.effects
    );
}

#[test]
fn reopened_window_focuses_again_after_close() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        second.effects.is_empty(),
        "focus never publishes as wire effects: {second:?}"
    );
    let request = second.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(request.id, format!("focus-{}", second.state_revision));
    assert_eq!(request.issued_revision, second.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    // Explicit close spends the intent: no focus on the closing report.
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec!["win-b"],
            "",
            None,
            None,
        ),
    );
    assert!(third.effects.is_empty());
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(fourth.effects.is_empty());
    // Re-adding the same id is a fresh newcomer again.
    let fifth = accepted(
        &mut engine,
        &report(
            5,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        fifth.effects.is_empty(),
        "focus never publishes as wire effects: {fifth:?}"
    );
    let request = fifth.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(request.id, format!("focus-{}", fifth.state_revision));
    assert_eq!(request.issued_revision, fifth.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
}

#[test]
fn newcomer_behind_fullscreen_spends_without_emitting() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![
                overlay("win-full", "out-a", 0, true, false),
                fact("win-a", "out-a", 100),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    // A newcomer admitted behind a protected overlay never takes automatic
    // activation; the intent spends without emitting and never replays.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                overlay("win-full", "out-a", 0, true, false),
                fact("win-a", "out-a", 100),
                fact("win-b", "out-a", 200),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        second.effects.is_empty(),
        "suppressed: {:?}",
        second.effects
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                overlay("win-full", "out-a", 0, true, false),
                fact("win-a", "out-a", 100),
                fact("win-b", "out-a", 200),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        third.effects.is_empty(),
        "never retried: {:?}",
        third.effects
    );
}

#[test]
fn close_focused_reflows_readable_without_observed_focus() {
    use crate::world_report::ScopeAuthority;
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    let view_a = view_of(
        &accepted(
            &mut engine,
            &report(
                2,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-a", 200),
                ],
                vec![],
                vec![],
                "win-b",
                None,
                None,
            ),
        ),
        "out-a",
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    // Closing the focused window with no observed focus reflows readable:
    // survivors stay Controlled with frames, the closed row is absent, and
    // committed focus falls back to a surviving resident (MRU, never an
    // invented activation). No pending focus lane yet.
    let reflowed = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec![],
            vec!["win-b"],
            "",
            None,
            None,
        ),
    );
    assert!(reflowed.windows.iter().all(|row| row.lifetime.0 != "win-b"));
    let scope = reflowed
        .scopes
        .iter()
        .find(|status| {
            status
                .scope
                .covers_workspace(&OutputId("out-a".into()), &view_a)
        })
        .expect("scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled);
    for win in ["win-a", "win-c"] {
        let row = reflowed
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .expect("survivor row");
        assert!(row.allocation.is_some(), "{win} reflows: {row:?}");
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Controlled,
            "{win} readable: {row:?}"
        );
    }
    let fallback = committed_focus(&engine, "out-a", &view_a).expect("survivor focus");
    assert!(
        ["win-a", "win-c"].contains(&fallback.as_str()),
        "survivor focus: {fallback:?}",
    );
    // The adapter reporting the survivor stays Controlled with reflowed
    // survivors and the survivor committed.
    let recovered = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let scope = recovered
        .scopes
        .iter()
        .find(|status| {
            status
                .scope
                .covers_workspace(&OutputId("out-a".into()), &view_a)
        })
        .expect("scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled);
    for win in ["win-a", "win-c"] {
        let row = recovered
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .expect("row");
        assert!(row.allocation.is_some(), "{win} reflows: {row:?}");
    }
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-a")
    );
    // Closing with the survivor observed focuses it directly.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    let view_a = view_of(
        &accepted(
            &mut engine,
            &report(
                2,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
                vec![],
                vec![],
                "win-b",
                None,
                None,
            ),
        ),
        "out-a",
    );
    let direct = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec!["win-b"],
            "win-a",
            None,
            None,
        ),
    );
    let scope = direct
        .scopes
        .iter()
        .find(|status| {
            status
                .scope
                .covers_workspace(&OutputId("out-a".into()), &view_a)
        })
        .expect("scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled);
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-a")
    );
}

#[test]
fn close_overlay_survivor_keeps_focus_and_truthful_rows() {
    for (name, fullscreen, maximized) in [("fullscreen", true, false), ("maximized", false, true)] {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                vec![
                    fact("win-a", "out-a", 0),
                    overlay("win-o", "out-a", 100, fullscreen, maximized),
                ],
                vec![],
                vec![],
                "win-o",
                None,
                None,
            ),
        );
        let view_a = view_of(
            &accepted(
                &mut engine,
                &report(
                    2,
                    vec![
                        fact("win-a", "out-a", 0),
                        overlay("win-o", "out-a", 100, fullscreen, maximized),
                    ],
                    vec![],
                    vec![],
                    "win-o",
                    None,
                    None,
                ),
            ),
            "out-a",
        );
        // Closing the overlay while the survivor is observed recovers
        // directly with truthful tiled rows.
        let third = accepted(
            &mut engine,
            &report(
                3,
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec!["win-o"],
                "win-a",
                None,
                None,
            ),
        );
        assert!(
            third.windows.iter().all(|row| row.lifetime.0 != "win-o"),
            "{name}"
        );
        assert_eq!(
            committed_focus(&engine, "out-a", &view_a).as_deref(),
            Some("win-a"),
            "{name}"
        );
        let scope = third
            .scopes
            .iter()
            .find(|status| {
                status
                    .scope
                    .covers_workspace(&OutputId("out-a".into()), &view_a)
            })
            .expect("scope row");
        assert_eq!(
            scope.authority,
            crate::world_report::ScopeAuthority::Controlled,
            "{name}"
        );
        let row = third
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("row");
        assert!(row.allocation.is_some(), "{name}: {row:?}");
    }
}

#[test]
fn view_switch_return_preserves_remembered_focus() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    // Remember focus on win-b through the observed global focus.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    // Switch away and back with truthful echoes: the leaf survives.
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
            }),
        ),
    );
    let calm = accepted(
        &mut engine,
        &report(
            5,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&calm, "out-a"), view_a);
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b"),
        "return preserves the leaf"
    );
}

#[test]
fn numbered_select_return_emits_remembered_focus() {
    // R-WS-09 over the numbered path (the dogfood SelectNumbered route):
    // switching away stays quiet when the target holds no remembered
    // focus, returning emits one explicit at-most-once effect to the
    // remembered leaf. A redundant select on the shown view stays quiet.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    let other_index = index_of(&other);
    let view_index = index_of(&view_a);
    // Remember focus on win-b through the observed global focus. The
    // pre-switch departing foreground stays truthful in the pager feed:
    // the shown view carries the focus mark.
    let focused = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    assert!(pager_contains_focused(&focused, "out-a", &view_a));
    // Switch away to the empty spare: no remembered focus there, so the
    // accepted reply carries admission-only effects (none here).
    let away = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: other_index,
            }),
        ),
    );
    assert!(
        away.effects.is_empty(),
        "empty target stays quiet: {:?}",
        away.effects
    );
    assert_eq!(view_of(&away, "out-a"), other);
    // Return: one explicit focus effect to the remembered leaf.
    let back = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: view_index,
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert!(
        back.effects.is_empty(),
        "focus never publishes as wire effects: {back:?}"
    );
    let request = back.focus_request.as_ref().expect("explicit focus request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(request.id, format!("focus-{}", back.state_revision));
    assert_eq!(request.issued_revision, back.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(12));
    // Post-hide desktop observation: native focus is empty while the
    // returned view is current, so the pager marks no workspace focused.
    // The reply focus mark never invents the remembered leaf.
    assert!(!pager_contains_focused_any(&back, "out-a"));
    // Redundant select on the shown view stays quiet.
    let calm = accepted(
        &mut engine,
        &report(
            5,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: view_index,
            }),
        ),
    );
    assert!(
        calm.effects.is_empty(),
        "redundant select stays quiet: {:?}",
        calm.effects
    );
}

#[test]
fn numbered_select_empty_and_minimized_targets_stay_quiet() {
    // R-CLOSE-03: selecting an empty workspace emits nothing. A minimized
    // remembered member never takes focus itself: the return follows the
    // updated remembered survivor, and a sole-minimized workspace (REQ-MNZ-03)
    // stays quiet.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    let other_index = index_of(&other);
    let view_index = index_of(&view_a);
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    let away = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: other_index,
            }),
        ),
    );
    assert!(away.effects.is_empty());
    // Minimize the remembered win-b; the return must not target the
    // minimized window. The updated remembered survivor win-a is the valid
    // focus target.
    let mut minimized_b = fact("win-b", "out-a", 100);
    minimized_b.minimized = Some(true);
    minimized_b.rect = None;
    let back = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), minimized_b],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: view_index,
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert!(
        back.effects.is_empty(),
        "focus never publishes as wire effects: {back:?}"
    );
    let request = back.focus_request.as_ref().expect("explicit focus request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", back.state_revision));
    assert_eq!(request.issued_revision, back.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(12));
    // Sole-minimized workspace (REQ-MNZ-03): no tiled survivor exists, so
    // the return stays quiet.
    let mut sole = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut sole,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    accepted(
        &mut sole,
        &report(
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let mut minimized_a = fact("win-a", "out-a", 0);
    minimized_a.minimized = Some(true);
    minimized_a.rect = None;
    accepted(
        &mut sole,
        &report(
            3,
            vec![minimized_a.clone()],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    let back = accepted(
        &mut sole,
        &report(
            4,
            vec![minimized_a],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&view_a),
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert!(
        back.effects.is_empty(),
        "sole-minimized stays quiet: {:?}",
        back.effects
    );
}

fn pager_contains_focused(
    reply: &crate::world_report::WorldReplyV4,
    output: &str,
    workspace: &str,
) -> bool {
    reply
        .presentation
        .pager
        .iter()
        .find(|screen| screen.output == output)
        .is_some_and(|screen| {
            screen
                .workspaces
                .iter()
                .any(|row| row.id == workspace && row.contains_focused)
        })
}

fn pager_contains_focused_any(reply: &crate::world_report::WorldReplyV4, output: &str) -> bool {
    reply
        .presentation
        .pager
        .iter()
        .find(|screen| screen.output == output)
        .is_some_and(|screen| screen.workspaces.iter().any(|row| row.contains_focused))
}

#[test]
fn numbered_select_occupied_return_both_directions_stale_then_desktop() {
    // Incident shape (occupied WS1 <-> occupied WS2, R-WS-09): the return
    // target must already hold a remembered leaf. A focused singleton
    // newly admitted on WS2 records the leaf; a second WS2 arrival
    // updates it. Returns in both directions emit the destination's
    // remembered window. The two reports ride the realistic observation
    // orderings: a stale departing foreground, then a post-hide desktop
    // (empty) read. Pager assertions expose the existing mixed-epoch feed:
    // a reply carries pre-switch focus on the departed view while logical
    // current has already moved. This diagnoses the reported stale mark;
    // it does not establish that presentation as the desired behavior.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let view_b = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    let index_a = index_of(&view_a);
    let index_b = index_of(&view_b);
    // Remember win-b on WS1 through the observed global focus.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    // Occupy WS2: select the spare, then admit a focused singleton.
    let away = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_b,
            }),
        ),
    );
    assert!(away.effects.is_empty());
    assert_eq!(view_of(&away, "out-a"), view_b);
    let admitted = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "win-c",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&admitted, "out-a"), view_b);
    // The focused singleton newly admitted on WS2 records the leaf.
    assert_eq!(
        committed_focus(&engine, "out-a", &view_b).as_deref(),
        Some("win-c")
    );
    // A second WS2 arrival, focused, updates the leaf.
    accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![],
            vec![],
            "win-d",
            None,
            None,
        ),
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_b).as_deref(),
        Some("win-d")
    );
    // Return to WS1 under a stale departing foreground: native still
    // reports win-d focused although the view moves now.
    let back_a = accepted(
        &mut engine,
        &report(
            6,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![],
            vec![],
            "win-d",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_a,
            }),
        ),
    );
    assert_eq!(view_of(&back_a, "out-a"), view_a);
    assert!(
        back_a.effects.is_empty(),
        "focus never publishes as wire effects: {back_a:?}"
    );
    let request = back_a
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(request.id, format!("focus-{}", back_a.state_revision));
    assert_eq!(request.issued_revision, back_a.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(12));
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_b).as_deref(),
        Some("win-d")
    );
    // Stale departing foreground: native still reports win-d focused
    // although the view moves now. The departed WS2 never keeps the focus
    // mark and the not-yet-observed WS1 carries none either: no workspace
    // is marked until native observation catches up.
    assert!(!pager_contains_focused(&back_a, "out-a", &view_a));
    assert!(!pager_contains_focused(&back_a, "out-a", &view_b));
    assert!(!pager_contains_focused_any(&back_a, "out-a"));
    // Post-hide desktop observation: native focus is empty while WS1 is
    // current, so no workspace carries a focus mark.
    let calm_a = accepted(
        &mut engine,
        &report(
            7,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&calm_a, "out-a"), view_a);
    assert!(!pager_contains_focused_any(&calm_a, "out-a"));
    // Native observation catches up: the current-workspace focus win-b is
    // genuinely observed, so the returned-to WS1 carries the mark and the
    // departed WS2 carries none. This is observed focus, never the
    // remembered desire painted early.
    let caught_up = accepted(
        &mut engine,
        &report(
            8,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&caught_up, "out-a"), view_a);
    assert!(pager_contains_focused(&caught_up, "out-a", &view_a));
    assert!(!pager_contains_focused(&caught_up, "out-a", &view_b));
    // Foreign focus (unmanaged lifetime) marks nothing.
    let foreign = accepted(
        &mut engine,
        &report(
            9,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![],
            vec![],
            "ghost",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&foreign, "out-a"), view_a);
    assert!(!pager_contains_focused_any(&foreign, "out-a"));
    // Return to WS2 under the desktop observation: the remembered win-d
    // is still the emitted target.
    let back_b = accepted(
        &mut engine,
        &report(
            10,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_b,
            }),
        ),
    );
    assert_eq!(view_of(&back_b, "out-a"), view_b);
    assert!(
        back_b.effects.is_empty(),
        "focus never publishes as wire effects: {back_b:?}"
    );
    let request = back_b
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-d");
    assert_eq!(request.id, format!("focus-{}", back_b.state_revision));
    assert_eq!(request.issued_revision, back_b.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(13));
    assert!(!pager_contains_focused_any(&back_b, "out-a"));
    // Memory on both workspaces survives the roundtrip.
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_b).as_deref(),
        Some("win-d")
    );
}

#[test]
fn stay_send_pins_exact_source_focus_or_empty_or_unknown() {
    // Survivor with the mover focused: the collapsed single-leaf tree
    // cannot name the pre-send stack, so the sole survivor is exact by
    // elimination.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let source = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(reply.transfers[0].operation_id, "op-11");
    // The stay names the engine's source-MRU survivor as an explicit focus
    // outcome; an emptied source names nothing.
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
    match engine.stay_source_focus("op-11", &source) {
        StaySourceFocus::Resident(window) => assert_eq!(window.0, "win-b"),
        other => panic!("sole survivor exact, got {other:?}"),
    }
    // Emptied source proves empty (never unknown): the session retired.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let source = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a),
    };
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(
        engine.stay_source_focus("op-11", &source),
        StaySourceFocus::Empty,
        "retired source is proven empty"
    );
    // Follow transfers never pin source focus. The mover must be the
    // observed focus to send at all; the follow still pins nothing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let source = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a),
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: true,
            }),
        ),
    );
    assert_eq!(
        engine.stay_source_focus("op-11", &source),
        StaySourceFocus::Unknown,
        "follow never pins"
    );
}

#[test]
fn stay_hidden_mover_scope_converges_without_staged_focus_or_close() {
    use crate::world_report::{ScopeAuthority, TransferKind};
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    // Stay the focused mover to a fresh trailing workspace: the target
    // session is born focusless (stay keeps the source selected).
    let stay = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            Some(11),
            Some(WorldCommand::SendNumbered {
                window: WindowId("win-b".to_owned()),
                source: crate::world_report::SendSource::Explicit(OutputId("out-a".to_owned())),
                index: 0,
                follow: false,
            }),
        ),
    );
    let target = stay
        .transfers
        .iter()
        .find_map(|t| match &t.kind {
            TransferKind::Window(w) if w.0 == "win-b" => Some(t.target_workspace.clone()),
            _ => None,
        })
        .expect("stay transfer");
    assert_ne!(target, view_a);
    // Next observation: the mover rides its snapshot rect (hidden
    // adapter-side, an ordinary readable fact here) with no staged focus
    // on the fresh domain and no close pending. The scope must converge
    // instead of wedging on focus-mismatch: no report can resolve the
    // focus of a hidden sole member, and the hold would block the reveal
    // that makes it focusable.
    let settled = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let scope = settled
        .scopes
        .iter()
        .find(|status| {
            status
                .scope
                .covers_workspace(&OutputId("out-a".into()), &target)
        })
        .expect("stay scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled);
    assert!(
        settled.windows.iter().any(|row| row.lifetime.0 == "win-b"),
        "mover row retained: {:?}",
        settled.windows
    );
}

#[test]
fn select_hidden_stay_arrival_without_history_focuses_first_leaf() {
    // Hidden arrival with no focus history: a stay-send parks win-b on
    // the hidden spare without touching its focus memory. Selecting the
    // spare must focus the first (only) layout leaf instead of staying
    // quiet; an empty select stays quiet (R-CLOSE-03).
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    // Stay-send the focused win-b to the hidden spare.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: other.clone(),
                follow: false,
            }),
        ),
    );
    // A member-attempt receipt settles the stay flight; the spare session is
    // born focusless (stay keeps the source selected).
    let arrived_b = fact("win-b", "out-a", 100);
    let mut settle = report(
        3,
        vec![fact("win-a", "out-a", 0), arrived_b],
        vec![],
        vec![],
        "win-a",
        None,
        None,
    );
    settle.member_attempts.push(MemberAttempt {
        operation_id: "op-11".to_owned(),
        window: WindowId("win-b".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: other.clone(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(other.clone()),
        }),
        attempt_ms: 11,
    });
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled.transfers.is_empty(),
        "stay flight settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &other),
        None,
        "hidden arrival records no focus memory"
    );
    // Selecting the arrival workspace focuses its first layout leaf.
    let shown = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert_eq!(view_of(&shown, "out-a"), other);
    assert!(
        shown.effects.is_empty(),
        "focus never publishes as wire effects: {shown:?}"
    );
    let request = shown
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(request.id, format!("focus-{}", shown.state_revision));
    assert_eq!(request.issued_revision, shown.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(12));
}

#[test]
fn select_remembered_fit_excluded_skips_to_eligible_later() {
    // Ineligible top candidate falls through: win-b is the remembered
    // leaf on WS1, but its staged row turns unexplained fit-excluded
    // while WS1 is hidden. Returning must skip win-b and focus eligible
    // win-a instead of staying quiet.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    // Remember win-b through the observed global focus.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    // Switch away to the empty spare: no remembered focus there.
    let away = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert!(away.effects.is_empty());
    // win-b turns unexplained fit-excluded natively while hidden; it
    // stays a linked tiled member, so the return must skip it rather
    // than lose focus.
    let mut excluded_b = fact("win-b", "out-a", 100);
    excluded_b.privileged = Some(true);
    let back = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), excluded_b.clone()],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&view_a),
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    // The privileged flip converges the excluded slot away exactly like a
    // close of the retained focus with no new focus proof: the close-survivor
    // hold fires, survivors publish held, and the return stages no focus
    // until the adapter proves the new focus.
    assert!(back.effects.is_empty(), "{:?}", back.effects);
    let held_scope = back
        .scopes
        .iter()
        .find(|status| {
            status
                .scope
                .covers_workspace(&OutputId("out-a".into()), &view_a)
        })
        .expect("return scope");
    assert_eq!(
        held_scope.authority,
        crate::world_report::ScopeAuthority::Hold(crate::world_report::HoldReason::Unreadable)
    );
    // Focus proof recovers: the excluded lifetime stays gone while the
    // eligible survivor takes the controlled scope with no new effect.
    let recovered = accepted(
        &mut engine,
        &report(
            5,
            vec![fact("win-a", "out-a", 0), excluded_b],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let scope = recovered
        .scopes
        .iter()
        .find(|status| {
            status
                .scope
                .covers_workspace(&OutputId("out-a".into()), &view_a)
        })
        .expect("recovered scope");
    assert_eq!(
        scope.authority,
        crate::world_report::ScopeAuthority::Controlled
    );
    assert!(recovered.effects.is_empty());
    assert!(
        !recovered
            .windows
            .iter()
            .any(|row| row.lifetime.0 == "win-b"),
        "{:?}",
        recovered.windows
    );
}

#[test]
fn select_first_layout_sticky_skips_to_later_arrival() {
    // Hidden stay-arrivals carry no focus memory; the first layout leaf
    // (win-b, admitted first) turns sticky before the reveal. Selecting
    // the spare must skip win-b and focus win-a.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    // Stay-send win-b to the hidden spare, then settle the flight.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: other.clone(),
                follow: false,
            }),
        ),
    );
    let arrived_b = fact("win-b", "out-a", 100);
    let mut settle = report(
        3,
        vec![fact("win-a", "out-a", 0), arrived_b],
        vec![],
        vec![],
        "win-a",
        None,
        None,
    );
    settle.member_attempts.push(MemberAttempt {
        operation_id: "op-11".to_owned(),
        window: WindowId("win-b".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: other.clone(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(other.clone()),
        }),
        attempt_ms: 11,
    });
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled.transfers.is_empty(),
        "stay flight settles: {:?}",
        settled.transfers
    );
    // Stay-send win-a to the same hidden spare, then settle with win-b
    // sticky. Both arrivals stay focusless (stay keeps source selected).
    accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(12),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: other.clone(),
                follow: false,
            }),
        ),
    );
    let mut sticky_b = fact("win-b", "out-a", 100);
    sticky_b.sticky = Some(true);
    let arrived_a = fact("win-a", "out-a", 0);
    let mut settle = report(5, vec![sticky_b, arrived_a], vec![], vec![], "", None, None);
    settle.member_attempts.push(MemberAttempt {
        operation_id: "op-12".to_owned(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: other.clone(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(other.clone()),
        }),
        attempt_ms: 12,
    });
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled.transfers.is_empty(),
        "second stay flight settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        first_leaf_window(&engine, "out-a", &other).as_deref(),
        Some("win-b"),
        "layout order starts at the first arrival"
    );
    // Selecting the arrival workspace skips sticky win-b for win-a.
    let mut sticky_select = fact("win-b", "out-a", 100);
    sticky_select.sticky = Some(true);
    let shown = accepted(
        &mut engine,
        &report(
            6,
            vec![sticky_select, fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert_eq!(view_of(&shown, "out-a"), other);
    assert!(
        shown.effects.is_empty(),
        "focus never publishes as wire effects: {shown:?}"
    );
    let request = shown
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", shown.state_revision));
    assert_eq!(request.issued_revision, shown.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(13));
}

#[test]
fn select_held_scope_stays_quiet_despite_eligible() {
    // Held scopes never take view-return focus: WS1 carries seeded memory
    // for win-a, but a bounds change under the gesture barrier holds the
    // scope, so returning moves the view with no focus effect.
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        // Caps match the fixture facts (sticky/maximize lanes present) to
        // retain the intended fixture admission.
        crate::connection::HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "outer".to_owned(),
            crate::connection::RawSettingValue::Number(0.0),
        )],
    )
    .expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    let away = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert!(away.effects.is_empty());
    // Same geometry with a changed gap under the barrier: the scope
    // holds, so the return moves the view with no focus effect.
    let mut held = report(
        3,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        vec![],
        vec![],
        "",
        Some(12),
        Some(WorldCommand::SelectNumbered {
            output: Some(OutputId("out-a".to_owned())),
            index: index_of(&view_a),
        }),
    );
    held.gesture_barrier = true;
    engine
        .apply_test_settings(
            1,
            vec![(
                "outer".to_owned(),
                crate::connection::RawSettingValue::Number(8.0),
            )],
        )
        .expect("settings accepted");
    let back = accepted(&mut engine, &held);
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert!(
        back.effects.is_empty(),
        "held scope stays quiet: {:?}",
        back.effects
    );
}

#[test]
fn select_without_native_focus_returns_recorded_leaf() {
    // Seeded memory without native focus: WS1 is bootstrapped without
    // native focus yet carries seeded memory for win-a, and the spare's
    // unfocused admissions select win-c then win-d. The canonical slot
    // records the latest emitted focus choice even if native focus never
    // arrives; topology leaf memory remains a fallback.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    // Occupy the spare; the empty select stays quiet (R-CLOSE-03).
    let away = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert!(away.effects.is_empty());
    assert_eq!(view_of(&away, "out-a"), other);
    // Two unfocused arrivals while the spare is shown; canonical memory
    // records the latest arrival, independently of retained leaf focus.
    for seq in [3u64, 4u64] {
        let mut windows = vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ];
        if seq == 4 {
            windows.push(fact("win-d", "out-a", 300));
        }
        accepted(
            &mut engine,
            &report(seq, windows, vec![], vec![], "", None, None),
        );
    }
    assert_eq!(
        committed_focus(&engine, "out-a", &other).as_deref(),
        Some("win-c"),
        "newcomer memory records the first arrival"
    );
    assert_eq!(
        first_leaf_window(&engine, "out-a", &other).as_deref(),
        Some("win-c"),
        "layout order starts at the first admission"
    );
    assert_eq!(
        engine.canonical_memory(&crate::session::DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: crate::directional::WorkspaceId(other.clone()),
        }),
        Some(crate::directional::WindowId("win-d".to_owned())),
        "latest accepted admission focus supersedes earlier leaf memory"
    );
    // Probe: what focus memory does the never-natively-focused WS1 carry?
    // (Bootstrap seeds win-a; the return below guards the recorded leaf.)
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-a")
    );
    // Return to WS1 names the seeded win-a (first layout leaf here is
    // win-b, so this guards the recorded leaf, not the layout order).
    let back = accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&view_a),
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert!(
        back.effects.is_empty(),
        "focus never publishes as wire effects: {back:?}"
    );
    let request = back.focus_request.as_ref().expect("explicit focus request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", back.state_revision));
    assert_eq!(request.issued_revision, back.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(12));
    // And back again: the spare restores the latest chosen newcomer win-d.
    let back_other = accepted(
        &mut engine,
        &report(
            6,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert_eq!(view_of(&back_other, "out-a"), other);
    assert!(
        back_other.effects.is_empty(),
        "focus never publishes as wire effects: {back_other:?}"
    );
    let request = back_other
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-d");
    assert_eq!(request.id, format!("focus-{}", back_other.state_revision));
    assert_eq!(request.issued_revision, back_other.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(13));
}

/// First layout leaf window for one workspace domain, in deterministic
/// child order (tentative no-history tie-break).
fn first_leaf_window(engine: &WorldEngine, output: &str, workspace: &str) -> Option<String> {
    let key = crate::session::DomainKey {
        output: OutputId(output.to_owned()),
        workspace: crate::directional::WorkspaceId(workspace.to_owned()),
    };
    let session = engine.engine().session(&key)?;
    let mut node = session.tree_for(&key)?;
    loop {
        match node {
            crate::directional::Node::Leaf { id } => {
                return session
                    .snapshot()
                    .windows
                    .iter()
                    .find(|link| &link.leaf == id)
                    .map(|link| link.window.0.clone());
            }
            crate::directional::Node::Group { children, .. } => {
                node = children.first()?;
            }
        }
    }
}

/// A withheld newcomer must never take canonical memory: the accepted
/// slot keeps its prior occupant instead of the forbidden birth.
fn assert_no_newcomer_memory(
    engine: &WorldEngine,
    reply: &crate::world_report::WorldReplyV4,
    output: &str,
    newcomer: &str,
    what: &str,
) {
    let view = view_of(reply, output);
    let home = crate::session::DomainKey {
        output: OutputId(output.to_owned()),
        workspace: crate::directional::WorkspaceId(view),
    };
    assert_ne!(
        engine.canonical_memory(&home),
        Some(WindowId(newcomer.to_owned())),
        "{what}"
    );
}

#[test]
fn admission_withheld_over_different_focused_floating() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    // A newcomer arriving while native focus names a different floating
    // window never takes automatic activation; the birth spends silently.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                WindowFact {
                    transient: Some(true),
                    ..fact("win-a", "out-a", 0)
                },
                fact("win-b", "out-a", 100),
            ],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        second.focus_request.is_none(),
        "automatic admission never activates over a different focused floating window: {second:?}"
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                WindowFact {
                    transient: Some(true),
                    ..fact("win-a", "out-a", 0)
                },
                fact("win-b", "out-a", 100),
            ],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        third.focus_request.is_none(),
        "withheld birth spends, never rearms: {third:?}"
    );
    // Lifting the veto restores nothing: the same lifetime reports tiled
    // foreground again, yet the spent birth never reissues.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        fourth.focus_request.is_none(),
        "spent birth never rearms on restore: {fourth:?}"
    );
}

#[test]
fn admission_withheld_over_different_focused_sticky() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                WindowFact {
                    sticky: Some(true),
                    ..fact("win-a", "out-a", 0)
                },
                fact("win-b", "out-a", 100),
            ],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        second.focus_request.is_none(),
        "automatic admission never activates over a different focused sticky window: {second:?}"
    );
    assert_no_newcomer_memory(
        &engine,
        &second,
        "out-a",
        "win-b",
        "withheld birth takes no memory",
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                WindowFact {
                    sticky: Some(true),
                    ..fact("win-a", "out-a", 0)
                },
                fact("win-b", "out-a", 100),
            ],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        third.focus_request.is_none(),
        "withheld birth spends, never rearms: {third:?}"
    );
    // Lifting the veto restores nothing: the spent birth never reissues.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        fourth.focus_request.is_none(),
        "spent birth never rearms on restore: {fourth:?}"
    );
}

#[test]
fn admission_never_targets_sticky() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    // A sticky newcomer is never an automatic target, even with a plain
    // tiled foreground that imposes no foreground guard.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    sticky: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        second.focus_request.is_none(),
        "automatic admission never targets sticky: {second:?}"
    );
    assert_no_newcomer_memory(
        &engine,
        &second,
        "out-a",
        "win-b",
        "withheld sticky birth takes no memory",
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    sticky: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        third.focus_request.is_none(),
        "withheld birth spends, never rearms: {third:?}"
    );
    // The same lifetime reporting non-sticky restores nothing: the spent
    // birth never rearms on restore.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        fourth.focus_request.is_none(),
        "spent birth never rearms on restore: {fourth:?}"
    );
}

#[test]
fn admission_float_never_targets_maximized() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    // An ordinary float carrying maximized is not an eligible ordinary
    // float: automatic admission withholds it like fullscreen.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    transient: Some(true),
                    maximized: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        second.focus_request.is_none(),
        "ordinary admission float never targets maximized: {second:?}"
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    transient: Some(true),
                    maximized: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        third.focus_request.is_none(),
        "withheld birth spends, never rearms: {third:?}"
    );
    // Dropping maximized restores an eligible ordinary float shape, yet
    // the same lifetime never reissues: the birth stays spent.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    transient: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        fourth.focus_request.is_none(),
        "spent birth never rearms on restore: {fourth:?}"
    );
}

#[test]
fn maximized_tiled_newcomer_still_takes_automatic_admission() {
    // No blanket maximized veto: a maximized tiled newcomer is an
    // ordinary automatic target (the sibling fence still owns the
    // behind-overlay case).
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    maximized: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let request = second.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
}

#[test]
fn explicit_focus_stays_exempt_from_automatic_guards() {
    // A direct focus command still actuates while a different floating
    // window holds native foreground: the automatic foreground guard
    // never leaks onto explicit decisions.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                WindowFact {
                    transient: Some(true),
                    ..fact("win-a", "out-a", 0)
                },
                fact("win-b", "out-a", 100),
            ],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
    );
    let request = second.focus_request.as_ref().expect("explicit request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
}

#[test]
fn explicit_focus_stays_exempt_for_maximized_float_target() {
    // The same maximized-float shape the automatic lane withholds still
    // actuates by direct command: the target veto never leaks onto
    // explicit decisions.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    transient: Some(true),
                    maximized: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
    );
    let request = second.focus_request.as_ref().expect("explicit request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
}

#[test]
fn retained_admission_retires_when_foreground_turns_floating() {
    // Retained revalidation: an eligible admission publishes while the
    // foreground is tiled; when the same foreground identity turns
    // floating with no baseline change, the retained row retires at
    // publication instead of activating behind it. Restoring the tile
    // reissues nothing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let issued = second.focus_request.as_ref().expect("issued request");
    assert_eq!(issued.target.0, "win-b");
    assert_eq!(
        issued.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                WindowFact {
                    transient: Some(true),
                    ..fact("win-a", "out-a", 0)
                },
                fact("win-b", "out-a", 100),
            ],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        third.focus_request.is_none(),
        "retained admission retires when its foreground turns floating: {third:?}"
    );
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        fourth.focus_request.is_none(),
        "retired row never reissues on restore: {fourth:?}"
    );
}

#[test]
fn retained_admission_retires_when_target_turns_sticky() {
    // Retained revalidation: the target turning sticky retires the live
    // admission row; restoring the tile reissues nothing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    second.focus_request.as_ref().expect("issued request");
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    sticky: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        third.focus_request.is_none(),
        "retained admission retires when its target turns sticky: {third:?}"
    );
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        fourth.focus_request.is_none(),
        "retired row never reissues on restore: {fourth:?}"
    );
}

#[test]
fn retained_admission_retires_when_float_target_turns_maximized() {
    // Retained revalidation: the ordinary float target gaining maximized
    // retires the live admission row; shedding maximized reissues nothing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    transient: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let issued = second.focus_request.as_ref().expect("issued request");
    assert_eq!(issued.target.0, "win-b");
    assert_eq!(
        issued.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    transient: Some(true),
                    maximized: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        third.focus_request.is_none(),
        "retained admission retires when its float target turns maximized: {third:?}"
    );
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    transient: Some(true),
                    ..fact("win-b", "out-a", 100)
                },
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        fourth.focus_request.is_none(),
        "retired row never reissues on restore: {fourth:?}"
    );
}

#[test]
fn inactive_workspace_birth_anchors_remembered_without_focus_or_switch_and_never_replays() {
    use crate::world_report::{NativeObservation, NativeOccupancy, NativeWorkspaceFact};

    fn host_fact(window: &str, output: &str, native: &str, x: i32) -> WindowFact {
        let mut f = fact(window, output, x);
        f.sticky = Some(false);
        f.native_workspace = NativeWorkspaceFact::Value(native.to_owned());
        f
    }

    fn native_obs(output: &str, order: &[&str], current: &str) -> NativeObservation {
        NativeObservation {
            output: OutputId(output.to_owned()),
            native_order: order.iter().map(|id| (*id).to_owned()).collect(),
            current_native: Some(current.to_owned()),
            occupancy: order
                .iter()
                .map(|id| NativeOccupancy {
                    native: (*id).to_owned(),
                    count: None,
                })
                .collect(),
        }
    }

    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");

    // 1. Bootstrap on n2 (W2) with H[win-b, win-c] and win-c focused so W2
    //    remembers win-c, while win-a sits on n1 (W1).
    let mut boot = report(
        1,
        vec![
            host_fact("win-a", "out-a", "n1", 0),
            host_fact("win-b", "out-a", "n2", 0),
            host_fact("win-c", "out-a", "n2", 200),
        ],
        vec![],
        vec![],
        "win-c",
        None,
        None,
    );
    boot.screens = vec![screen("out-a", 0)];
    boot.native = vec![native_obs("out-a", &["n1", "n2"], "n2")];
    let boot_reply = accepted(&mut engine, &boot);
    let w1_id = boot_reply
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n1"))
        .expect("W1 bound to n1")
        .id
        .clone();
    let w2_id = boot_reply
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n2"))
        .expect("W2 bound to n2")
        .id
        .clone();
    let w2_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(w2_id.clone()),
    };

    // 2. Switch to W1 (n1) with win-a focused; W2 becomes inactive with
    //    remembered leaf win-c.
    let mut on_w1 = report(
        2,
        vec![
            host_fact("win-a", "out-a", "n1", 0),
            host_fact("win-b", "out-a", "n2", 0),
            host_fact("win-c", "out-a", "n2", 200),
        ],
        vec![],
        vec![],
        "win-a",
        None,
        None,
    );
    on_w1.screens = vec![screen("out-a", 0)];
    on_w1.native = vec![native_obs("out-a", &["n1", "n2"], "n1")];
    let on_w1_reply = accepted(&mut engine, &on_w1);
    assert_eq!(view_of(&on_w1_reply, "out-a"), w1_id);
    assert_eq!(
        committed_focus(&engine, "out-a", &w1_id).as_deref(),
        Some("win-a")
    );
    {
        let session = engine.engine().session(&w2_key).expect("W2 session");
        let remembered = session
            .remembered_leaf(&w2_key)
            .expect("W2 remembered leaf");
        let snapshot = session.snapshot();
        let remembered_win = snapshot
            .windows
            .iter()
            .find(|link| link.leaf == remembered)
            .map(|link| link.window.0.as_str());
        assert_eq!(remembered_win, Some("win-c"), "W2 remembers win-c");
    }

    // 3. Newcomer win-d observed on inactive W2 (n2) while W1 (n1) stays
    //    displayed with win-a focused.
    let mut admit_d = report(
        3,
        vec![
            host_fact("win-a", "out-a", "n1", 0),
            host_fact("win-b", "out-a", "n2", 0),
            host_fact("win-c", "out-a", "n2", 200),
            host_fact("win-d", "out-a", "n2", 300),
        ],
        vec![],
        vec![],
        "win-a",
        None,
        None,
    );
    admit_d.screens = vec![screen("out-a", 0)];
    admit_d.native = vec![native_obs("out-a", &["n1", "n2"], "n1")];
    let admit_reply = accepted(&mut engine, &admit_d);

    // No automatic focus or view switch.
    assert!(
        admit_reply.focus_request.is_none(),
        "inactive birth must not request focus: {admit_reply:?}"
    );
    assert!(
        admit_reply.effects.is_empty(),
        "inactive birth must not emit focus effects: {:?}",
        admit_reply.effects
    );
    assert!(
        admit_reply.transfers.is_empty(),
        "inactive birth must not trigger transfers: {:?}",
        admit_reply.transfers
    );
    assert_eq!(view_of(&admit_reply, "out-a"), w1_id);
    assert!(
        engine.admission_focus_spent.contains("win-d"),
        "inactive birth spends admission focus"
    );

    // Allocations prove win-d anchored at remembered win-c (right half of H[B,C]),
    // leaving win-b in the untouched left half.
    let row_b = admit_reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("win-b row");
    let row_c = admit_reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-c")
        .expect("win-c row");
    let row_d = admit_reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-d")
        .expect("win-d row");
    assert_eq!(row_b.workspace, w2_id);
    assert_eq!(row_c.workspace, w2_id);
    assert_eq!(row_d.workspace, w2_id);
    let alloc_b = row_b.allocation.expect("win-b allocation");
    let alloc_c = row_c.allocation.expect("win-c allocation");
    let alloc_d = row_d.allocation.expect("win-d allocation");
    assert!(
        alloc_b.x < alloc_c.x && alloc_c.x == alloc_d.x && alloc_c.w == alloc_d.w,
        "win-d splits remembered win-c rather than win-b: b={alloc_b:?} c={alloc_c:?} d={alloc_d:?}"
    );

    // 4. Returning to W2 (n2) must never replay admission focus for win-d.
    let mut return_w2 = report(
        4,
        vec![
            host_fact("win-a", "out-a", "n1", 0),
            host_fact("win-b", "out-a", "n2", 0),
            host_fact("win-c", "out-a", "n2", 200),
            host_fact("win-d", "out-a", "n2", 300),
        ],
        vec![],
        vec![],
        "win-c",
        None,
        None,
    );
    return_w2.screens = vec![screen("out-a", 0)];
    return_w2.native = vec![native_obs("out-a", &["n1", "n2"], "n2")];
    let return_reply = accepted(&mut engine, &return_w2);
    assert!(
        return_reply.focus_request.is_none(),
        "returning to W2 must not replay admission focus: {return_reply:?}"
    );
    assert!(return_reply.effects.is_empty());
}

#[test]
fn same_report_inactive_and_active_new_births_keep_active_focus_eligible() {
    use crate::world_report::{
        FocusRequestCause, NativeObservation, NativeOccupancy, NativeWorkspaceFact,
    };

    fn host_fact(window: &str, output: &str, native: &str, x: i32) -> WindowFact {
        let mut f = fact(window, output, x);
        f.sticky = Some(false);
        f.native_workspace = NativeWorkspaceFact::Value(native.to_owned());
        f
    }

    fn native_obs(output: &str, order: &[&str], current: &str) -> NativeObservation {
        NativeObservation {
            output: OutputId(output.to_owned()),
            native_order: order.iter().map(|id| (*id).to_owned()).collect(),
            current_native: Some(current.to_owned()),
            occupancy: order
                .iter()
                .map(|id| NativeOccupancy {
                    native: (*id).to_owned(),
                    count: None,
                })
                .collect(),
        }
    }

    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");

    // 1. Bootstrap on n1 (W1) with win-a focused and win-b on n2 (W2).
    let mut boot = report(
        1,
        vec![
            host_fact("win-a", "out-a", "n1", 0),
            host_fact("win-b", "out-a", "n2", 0),
        ],
        vec![],
        vec![],
        "win-a",
        None,
        None,
    );
    boot.screens = vec![screen("out-a", 0)];
    boot.native = vec![native_obs("out-a", &["n1", "n2"], "n1")];
    let boot_reply = accepted(&mut engine, &boot);
    let w1_id = boot_reply
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("n1"))
        .expect("W1 bound to n1")
        .id
        .clone();

    // 2. Same report births: win-d on inactive W2 (n2) and win-e on active W1 (n1).
    let mut both_born = report(
        2,
        vec![
            host_fact("win-a", "out-a", "n1", 0),
            host_fact("win-b", "out-a", "n2", 0),
            host_fact("win-e", "out-a", "n1", 100),
            host_fact("win-d", "out-a", "n2", 100),
        ],
        vec![],
        vec![],
        "win-a",
        None,
        None,
    );
    both_born.screens = vec![screen("out-a", 0)];
    both_born.native = vec![native_obs("out-a", &["n1", "n2"], "n1")];
    let reply = accepted(&mut engine, &both_born);

    assert_eq!(view_of(&reply, "out-a"), w1_id);
    let issued = reply
        .focus_request
        .as_ref()
        .expect("active newcomer win-e remains eligible for admission focus");
    assert_eq!(issued.target.0, "win-e");
    assert_eq!(issued.cause, FocusRequestCause::AutomaticAdmission);
    assert!(engine.admission_focus_spent.contains("win-d"));
    assert!(engine.admission_focus_spent.contains("win-e"));

    // 3. Switching to W2 (n2) must not replay admission focus for win-d.
    let mut switch_w2 = report(
        3,
        vec![
            host_fact("win-a", "out-a", "n1", 0),
            host_fact("win-b", "out-a", "n2", 0),
            host_fact("win-e", "out-a", "n1", 100),
            host_fact("win-d", "out-a", "n2", 100),
        ],
        vec![],
        vec![],
        "win-b",
        None,
        None,
    );
    switch_w2.screens = vec![screen("out-a", 0)];
    switch_w2.native = vec![native_obs("out-a", &["n1", "n2"], "n2")];
    let switch_reply = accepted(&mut engine, &switch_w2);
    assert!(
        switch_reply.focus_request.is_none(),
        "spent inactive birth win-d never replays on switch: {switch_reply:?}"
    );
    assert!(switch_reply.effects.is_empty());
}
