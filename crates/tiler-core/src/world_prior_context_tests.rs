//! Prior-accepted context regressions (v4 path).
//!
//! Accepted-run coverage for the shared prior-observation
//! lanes: first-observation focus recording, changed-focus progression,
//! rejected-candidate preservation, consumed-report handling, bootstrap
//! exclusion and handshake reset. These run the production
//! `handle_report` path and assert through test-visible slots and reply
//! rows, never through the typed replay cache.

use crate::connection::{ApplicationIdentity, HandshakeRequest, HostCapabilities};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::session::DomainKey;
use crate::world_engine::{KIND_REUSED, PriorAcceptedContext, WorldEngine, WorldOutcome};
use crate::world_report::{
    AuthorityScope, FocusRequestCause, HoldReason, ScopeAuthority, ScopeStatus, TilingMode,
    WindowDesire, WorkspaceDesire, WorldCommand, WorldReplyV4,
};
use crate::world_v4_support_tests::{accepted, fact, report, report_full, screen};

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn home_key(report: &WorldReplyV4) -> DomainKey {
    let row = report
        .workspaces
        .iter()
        .find(|workspace| workspace.output == OutputId("out-a".to_owned()))
        .expect("published workspace row");
    DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(row.id.clone()),
    }
}

#[test]
fn first_observation_focus_records_canonical_slot() {
    let mut engine = engine();
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    let key = home_key(&reply);
    assert_eq!(
        engine.canonical_memory(&key),
        Some(WindowId("win-a".to_owned()))
    );
}

#[test]
fn changed_focus_progresses_canonical_slot() {
    let mut engine = engine();
    let first = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        Vec::new(),
        "win-a",
        None,
        None,
    );
    let reply = accepted(&mut engine, &first);
    let key = home_key(&reply);
    assert_eq!(
        engine.canonical_memory(&key),
        Some(WindowId("win-a".to_owned()))
    );
    let second = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        Vec::new(),
        "win-b",
        None,
        None,
    );
    accepted(&mut engine, &second);
    assert_eq!(
        engine.canonical_memory(&key),
        Some(WindowId("win-b".to_owned()))
    );
}

#[test]
fn cleared_cache_preserves_focus_progression() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    // Test-only eviction of the typed replay cache: the focus lane must
    // keep progressing from retained prior context.
    engine.last_report_v4 = None;
    engine.last_reply_v4 = None;
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            Vec::new(),
            "win-b",
            None,
            None,
        ),
    );
    let key = home_key(&reply);
    assert_eq!(
        engine.canonical_memory(&key),
        Some(WindowId("win-b".to_owned()))
    );
}

#[test]
fn bootstrap_only_records_no_canonical_focus() {
    let mut engine = engine();
    let mut incomplete = report(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        None,
        None,
    );
    incomplete.inventory_complete = false;
    match engine.handle_report(&incomplete, &[]).0 {
        WorldOutcome::Accepted(reply) => assert!(!reply.ready, "bootstrap stays unready"),
        other => panic!("expected bootstrap acceptance, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 0);
    assert_eq!(
        engine.canonical_memory(&DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        }),
        None
    );
    assert_eq!(engine.prior_context, PriorAcceptedContext::default());
}

#[test]
fn rejected_report_leaves_canonical_focus_and_revision() {
    let mut engine = engine();
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    let key = home_key(&reply);
    let revision = engine.state_revision;
    let reused = report(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        None,
        None,
    );
    match engine.handle_report(&reused, &[]).0 {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REUSED),
        other => panic!("expected reused refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision);
    assert_eq!(
        engine.canonical_memory(&key),
        Some(WindowId("win-a".to_owned()))
    );
}

#[test]
fn reset_handshake_clears_canonical_focus() {
    let mut engine = engine();
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    let key = home_key(&reply);
    assert_eq!(
        engine.canonical_memory(&key),
        Some(WindowId("win-a".to_owned()))
    );
    let reset = HandshakeRequest {
        adapter: "adapter-2".to_owned(),
        request: "test-handshake-reset".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: Some("test-own-executable".to_owned()),
            process_id: Some("test-own-pid".to_owned()),
        },
        capabilities: HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..HostCapabilities::default()
        },
    };
    let handshake_reply = engine.handshake(&reset);
    assert!(handshake_reply.kind.is_none());
    assert_eq!(engine.state_revision, 0);
    assert_eq!(engine.canonical_memory(&key), None);
}

fn report_v4(
    seq: u64,
    screens: Vec<crate::world_report::ScreenReport>,
    windows: Vec<crate::world_report::WindowFact>,
    focused: &str,
) -> crate::world_report::WorldReportV4 {
    report_full(seq, seq, screens, windows, Vec::new(), focused, None, None)
}

#[test]
fn staged_v4_observation_feeds_prior_capture_without_typed_cache() {
    // Staged-only v4 evidence: no accepted run, no scenario claim. The
    // staged observation carries the raw focus for later prior-context
    // capture, with no `last_report_v4`/`last_reply_v4` seeding involved.
    let mut engine = engine();
    let staged = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        "win-a",
    );
    engine.stage_facts_v4(&staged);
    assert_eq!(engine.staged.observation.focused_global, "win-a");
    assert!(engine.last_report_v4.is_none());
    assert!(engine.last_reply_v4.is_none());
    let focus = engine.staged.observation.focused_global.clone();
    let context = PriorAcceptedContext::capture(&focus, &[], &[], &[]);
    assert_eq!(context.prev_focused_global, "win-a");
    assert!(context.empty_controlled.is_empty());
}

#[test]
fn unchanged_raw_focus_preserves_command_selected_slot_without_cache() {
    // Raw focus stays on A while an explicit command selects B: the
    // observation lane must not overwrite the command choice, and the
    // retained prior focus stays A even with the typed cache evicted.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let reply = accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            windows(),
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    let key = home_key(&reply);
    assert_eq!(
        engine.canonical_memory(&key),
        Some(WindowId("win-a".to_owned()))
    );
    let focus_b = WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: "left".to_owned(),
    };
    let reply = match engine
        .handle_report(
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                windows(),
                Vec::new(),
                "win-a",
                Some(1),
                Some(focus_b),
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted command, got {other:?}"),
    };
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target, WindowId("win-b".to_owned()));
    assert_eq!(request.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(
        engine.canonical_memory(&key),
        Some(WindowId("win-b".to_owned()))
    );
    // Test-only eviction of the typed replay cache: unchanged raw A must
    // still preserve the command-selected slot.
    engine.last_report_v4 = None;
    engine.last_reply_v4 = None;
    accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            windows(),
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    assert_eq!(
        engine.canonical_memory(&key),
        Some(WindowId("win-b".to_owned()))
    );
}

#[test]
fn reset_handshake_clears_prior_context() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    assert_eq!(engine.prior_context.prev_focused_global, "win-a");
    let reset = HandshakeRequest {
        adapter: "adapter-2".to_owned(),
        request: "test-handshake-reset".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: Some("test-own-executable".to_owned()),
            process_id: Some("test-own-pid".to_owned()),
        },
        capabilities: HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..HostCapabilities::default()
        },
    };
    assert!(engine.handshake(&reset).kind.is_none());
    assert_eq!(engine.prior_context, PriorAcceptedContext::default());
}

#[test]
fn consumed_report_refreshes_prior_focus_and_empty_set() {
    // A consumed report reconciles observation only, but the retained
    // context must refresh from its publication rather than keep the
    // previous candidate's values.
    let mut engine = engine();
    let refused = || WorldCommand::Focus {
        window: WindowId("win-zzz".to_owned()),
        direction: "left".to_owned(),
    };
    let empty_id = match engine
        .handle_report(
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                Vec::new(),
                Vec::new(),
                "win-a",
                Some(1),
                Some(refused()),
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Accepted(reply) => {
            let row = reply
                .workspaces
                .iter()
                .find(|workspace| workspace.output == OutputId("out-a".to_owned()))
                .expect("published workspace row");
            assert!(engine.prior_context.empty_controlled.contains(&DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId(row.id.clone()),
            }));
            row.id.clone()
        }
        other => panic!("expected accepted refusal, got {other:?}"),
    };
    assert_eq!(engine.prior_context.prev_focused_global, "win-a");
    // Occupy the workspace and move raw focus under a consumed sequence.
    // The previously proven key must drop from the refreshed set: a stale
    // context would keep proving the now-occupied workspace empty.
    let redelivery = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-x", "out-a", 0)],
        Vec::new(),
        "win-x",
        Some(1),
        Some(refused()),
    );
    let occupied_id = match engine.handle_report(&redelivery, &[]).0 {
        WorldOutcome::CommandConsumed(reply) => {
            let row = reply
                .workspaces
                .iter()
                .find(|workspace| {
                    workspace.output == OutputId("out-a".to_owned()) && !workspace.trailing
                })
                .expect("published occupied workspace row");
            row.id.clone()
        }
        other => panic!("expected consumed command, got {other:?}"),
    };
    assert_eq!(occupied_id, empty_id);
    assert_eq!(engine.prior_context.prev_focused_global, "win-x");
    assert!(!engine.prior_context.empty_controlled.contains(&DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(occupied_id),
    }));
}

fn workspace_row(id: &str, output: &str) -> WorkspaceDesire {
    WorkspaceDesire {
        id: id.to_owned(),
        output: OutputId(output.to_owned()),
        native_binding: None,
        scope: AuthorityScope::Output(OutputId(output.to_owned())),
        order: 0,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: None,
        trailing: false,
    }
}

fn scope_row(scope: AuthorityScope, authority: ScopeAuthority) -> ScopeStatus {
    ScopeStatus { scope, authority }
}

fn homed_window(
    window: &str,
    output: &str,
    workspace: &str,
    authority: ScopeAuthority,
) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: workspace.to_owned(),
        allocation: None,
        effective_frame: None,
        frame_authority: authority,
        membership_authority: authority,
        floating: false,
        float_origin: None,
        frame_operation: None,
        clear_operation: None,
        command_seq: None,
        visibility: None,
        visibility_authority: ScopeAuthority::Untouched,
        state: Vec::new(),
    }
}

fn controlled_key() -> DomainKey {
    DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    }
}

#[test]
fn capture_empty_proof_matrix() {
    // Pure helper matrix over common reply rows: no engine run, no
    // typed cache, no v4 runtime involved.
    let workspaces = vec![workspace_row("ws-1", "out-a")];
    let output_controlled = vec![scope_row(
        AuthorityScope::Output(OutputId("out-a".to_owned())),
        ScopeAuthority::Controlled,
    )];
    // Exact output scope plus workspace existence proves empty.
    let proved = PriorAcceptedContext::capture("win-a", &workspaces, &output_controlled, &[]);
    assert_eq!(proved.prev_focused_global, "win-a");
    assert!(proved.empty_controlled.contains(&controlled_key()));
    // Exact workspace scope also proves.
    let workspace_controlled = vec![scope_row(
        AuthorityScope::Workspace {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
        ScopeAuthority::Controlled,
    )];
    let proved = PriorAcceptedContext::capture("win-a", &workspaces, &workspace_controlled, &[]);
    assert!(proved.empty_controlled.contains(&controlled_key()));
    // Other output or workspace never proves, even when controlled.
    let other_output = vec![scope_row(
        AuthorityScope::Output(OutputId("out-b".to_owned())),
        ScopeAuthority::Controlled,
    )];
    let unproved = PriorAcceptedContext::capture("win-a", &workspaces, &other_output, &[]);
    assert!(unproved.empty_controlled.is_empty());
    let other_workspace = vec![scope_row(
        AuthorityScope::Workspace {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("ws-9".to_owned()),
        },
        ScopeAuthority::Controlled,
    )];
    let unproved = PriorAcceptedContext::capture("win-a", &workspaces, &other_workspace, &[]);
    assert!(unproved.empty_controlled.is_empty());
    // Untouched and held scopes never prove.
    let untouched = vec![scope_row(
        AuthorityScope::Output(OutputId("out-a".to_owned())),
        ScopeAuthority::Untouched,
    )];
    let unproved = PriorAcceptedContext::capture("win-a", &workspaces, &untouched, &[]);
    assert!(unproved.empty_controlled.is_empty());
    let held = vec![scope_row(
        AuthorityScope::Output(OutputId("out-a".to_owned())),
        ScopeAuthority::Hold(HoldReason::Unreadable),
    )];
    let unproved = PriorAcceptedContext::capture("win-a", &workspaces, &held, &[]);
    assert!(unproved.empty_controlled.is_empty());
    // Missing workspace row never proves.
    let unproved = PriorAcceptedContext::capture("win-a", &[], &output_controlled, &[]);
    assert!(unproved.empty_controlled.is_empty());
    // Any homed window row vetoes, regardless of its authorities.
    for authority in [
        ScopeAuthority::Controlled,
        ScopeAuthority::Untouched,
        ScopeAuthority::Hold(HoldReason::Minimized),
    ] {
        let windows = vec![homed_window("win-a", "out-a", "ws-1", authority)];
        let unproved =
            PriorAcceptedContext::capture("win-a", &workspaces, &output_controlled, &windows);
        assert!(
            unproved.empty_controlled.is_empty(),
            "homed row with {authority:?} must veto"
        );
    }
    // A window homed elsewhere does not veto.
    let elsewhere = vec![homed_window(
        "win-a",
        "out-a",
        "ws-2",
        ScopeAuthority::Controlled,
    )];
    let proved =
        PriorAcceptedContext::capture("win-a", &workspaces, &output_controlled, &elsewhere);
    assert!(proved.empty_controlled.contains(&controlled_key()));
}
