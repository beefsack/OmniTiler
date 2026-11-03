use crate::boundary::CoreCommand;
use crate::connection::*;
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};
use crate::world_report::{
    RowDiagnosticLane, ScopeAuthority, ScreenReport, ScreenView, WindowFact, WorldCommand,
    WorldReport,
};

fn report(seq: u64, complete: bool) -> WorldReport {
    WorldReport {
        contract_version: crate::world_report::WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".into(),
        core_generation: "gen-1".into(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("report-{seq}"),
        command_seq: None,
        command: None,
        screens: vec![ScreenReport {
            output: OutputId("out-1".into()),
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
                h: 560,
            }),
            complete,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        }],
        windows: Vec::new(),
        views: vec![ScreenView {
            output: OutputId("out-1".into()),
            view: String::new(),
        }],
        closed: Vec::new(),
        native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: vec![OutputId("out-1".into())],
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete: complete,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: seq,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        state_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),

        intent_records: Vec::new(),
    }
}

fn handshake(adapter: &str) -> HandshakeRequest {
    HandshakeRequest {
        adapter: adapter.into(),
        request: "handshake-1".into(),
        application_identity: ApplicationIdentity {
            app_id: Some("example.app".into()),
            executable: Some("example".into()),
            process_id: Some("pid-own-1".into()),
        },
        capabilities: HostCapabilities {
            host_owned: true,
            equal_count: true,
            birth_clear: true,
            native_selector_scope: NativeSelectorScope::AllOutputs,
            window_suppression: true,
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            warp: false,
            tiled_float_transfer: true,
            unavailable_facts: Vec::new(),
        },
    }
}

#[test]
fn handshake_fixes_connection_facts_and_rehandshake_starts_fresh_generation() {
    let mut engine = WorldEngine::new("gen-1").unwrap();
    let request = handshake("adapter-1");
    let reply = engine.handshake(&request);
    assert!(!reply.ready);
    assert_eq!(reply.kind, None);
    assert_eq!(engine.capabilities(), Some(&request.capabilities));
    assert_eq!(
        engine.application_identity(),
        Some(&request.application_identity)
    );

    let again = engine.handshake(&handshake("adapter-2"));
    assert_ne!(reply.generation, again.generation);
    assert_eq!(engine.state_revision(), 0);
    assert_eq!(
        engine.capabilities(),
        Some(&handshake("adapter-2").capabilities)
    );
    assert_eq!(
        engine.application_identity(),
        Some(&handshake("adapter-2").application_identity)
    );
}

#[test]
fn settings_envelope_is_fenced_before_settings_state_mutation() {
    let mut engine = WorldEngine::new("gen-1").unwrap();
    let bound = handshake("adapter-1");
    let generation = engine.handshake(&bound).generation.unwrap();
    let mut request = SettingsRequest {
        adapter: bound.adapter,
        generation: generation.clone(),
        request: "settings-1".into(),
        revision: 1,
        values: Vec::new(),
    };
    request.adapter = "adapter-2".into();
    assert!(engine.handle_settings(&request).is_none());
    assert_eq!(
        engine.settings().requested[crate::settings_catalog::FIELD_INNER_GAP],
        crate::settings_state::EffectiveSetting::Int(8)
    );

    request.adapter = "adapter-1".into();
    let reply = engine.handle_settings(&request).unwrap();
    assert_eq!(reply.status, SettingsStatus::Accepted);
    assert_eq!(reply.diagnostics.len(), 20);
}

#[test]
fn reports_wait_for_initial_settings_and_ready_latches_across_gaps() {
    let mut engine = WorldEngine::new("gen-1").unwrap();
    let first_handshake = handshake("adapter-1");
    let generation = engine.handshake(&first_handshake).generation.unwrap();
    let initial = report(1, true);
    let WorldOutcome::Accepted(bootstrap) = engine.handle_report(&initial) else {
        panic!("valid pre-settings report should receive a bootstrap reply");
    };
    assert_eq!(bootstrap.request_id, initial.request_id);
    assert_eq!(bootstrap.based_on_observation_seq, initial.observation_seq);
    assert!(!bootstrap.ready);
    assert!(bootstrap.scopes.is_empty());
    assert!(bootstrap.workspaces.is_empty());
    assert!(bootstrap.effects.is_empty());
    assert!(!bootstrap.command_consumed);
    assert_eq!(engine.state_revision(), 0);
    assert!(!engine.is_ready());

    let settings = engine
        .handle_settings(&SettingsRequest {
            adapter: first_handshake.adapter,
            generation,
            request: "settings-1".into(),
            revision: 1,
            values: Vec::new(),
        })
        .unwrap();
    assert_eq!(settings.status, SettingsStatus::Accepted);
    assert!(!settings.diagnostics.is_empty());

    let WorldOutcome::Accepted(ready) = engine.handle_report(&initial) else {
        panic!("same pre-settings report should process normally after settings");
    };
    assert!(ready.ready);
    assert!(engine.is_ready());

    let WorldOutcome::Accepted(incomplete) = engine.handle_report(&report(2, false)) else {
        panic!("incomplete first report should receive a bootstrap reply");
    };
    assert!(incomplete.ready);
    assert!(engine.is_ready());

    let WorldOutcome::Accepted(gapped) = engine.handle_report(&report(3, false)) else {
        panic!("later incomplete report should be accepted after readiness latches");
    };
    assert!(gapped.ready);
    assert!(engine.is_ready());

    let next = engine.handshake(&handshake("adapter-2"));
    assert!(!next.ready);
    assert!(!engine.is_ready());
}

#[test]
fn incomplete_first_report_after_settings_does_not_authorize_effects_or_readiness() {
    let mut engine = WorldEngine::new("gen-1").unwrap();
    let bound = handshake("adapter-1");
    let generation = engine.handshake(&bound).generation.unwrap();
    engine
        .handle_settings(&SettingsRequest {
            adapter: bound.adapter,
            generation,
            request: "settings-1".into(),
            revision: 1,
            values: Vec::new(),
        })
        .unwrap();

    let incomplete_report = report(1, false);
    let WorldOutcome::Accepted(bootstrap) = engine.handle_report(&incomplete_report) else {
        panic!("incomplete first report should receive a bootstrap reply");
    };
    assert_eq!(bootstrap.request_id, incomplete_report.request_id);
    assert_eq!(bootstrap.based_on_observation_seq, 1);
    assert!(!bootstrap.ready);
    assert!(bootstrap.scopes.is_empty());
    assert!(bootstrap.workspaces.is_empty());
    assert!(bootstrap.effects.is_empty());
    assert_eq!(engine.state_revision(), 0);
    assert!(!engine.is_ready());

    let WorldOutcome::Accepted(ready) = engine.handle_report(&report(2, true)) else {
        panic!("first usable complete report should process normally");
    };
    assert!(ready.ready);
    assert!(engine.is_ready());
}

#[test]
fn foreign_and_malformed_reports_reject_before_not_ready_bootstrap() {
    let mut engine = WorldEngine::new("gen-1").unwrap();
    let bound = handshake("adapter-1");
    engine.handshake(&bound);
    // Foreign adapter incarnation rejects before any bootstrap reply.
    let mut foreign_adapter = report(1, true);
    foreign_adapter.adapter_incarnation = "adapter-2".into();
    assert!(
        matches!(
            engine.handle_report(&foreign_adapter),
            WorldOutcome::Rejected { .. }
        ),
        "foreign adapter must reject, never bootstrap"
    );
    // Foreign generation rejects before any bootstrap reply.
    let mut foreign_generation = report(1, true);
    foreign_generation.core_generation = "gen-2".into();
    assert!(
        matches!(
            engine.handle_report(&foreign_generation),
            WorldOutcome::Rejected { .. }
        ),
        "foreign generation must reject, never bootstrap"
    );
    // Malformed command pair (sequence without a command) rejects before
    // any bootstrap reply.
    let mut unpaired = report(1, true);
    unpaired.command_seq = Some(7);
    assert!(
        matches!(
            engine.handle_report(&unpaired),
            WorldOutcome::Rejected { .. }
        ),
        "unpaired command sequence must reject, never bootstrap"
    );
    assert!(!engine.is_ready());
    assert_eq!(engine.state_revision(), 0);
}

#[test]
fn pre_settings_command_sequence_is_not_consumed_or_retained() {
    let mut engine = WorldEngine::new("gen-1").unwrap();
    let bound = handshake("adapter-1");
    let generation = engine.handshake(&bound).generation.unwrap();
    let mut commanded = report(1, true);
    commanded.command_seq = Some(7);
    commanded.command = Some(WorldCommand::Operation(CoreCommand::Reconcile));
    let WorldOutcome::Accepted(bootstrap) = engine.handle_report(&commanded) else {
        panic!("valid pre-settings commanded report should receive a bootstrap reply");
    };
    assert!(!bootstrap.ready);
    assert!(!bootstrap.command_consumed);
    assert!(!engine.is_ready());

    engine
        .handle_settings(&SettingsRequest {
            adapter: bound.adapter,
            generation,
            request: "settings-1".into(),
            revision: 1,
            values: Vec::new(),
        })
        .unwrap();

    // The same sequence after accepted settings executes instead of
    // replaying a consumed command.
    let WorldOutcome::Accepted(executed) = engine.handle_report(&commanded) else {
        panic!("same command sequence after settings must execute");
    };
    assert!(executed.ready);
    assert!(!executed.command_consumed);

    // A fresh report reusing the now-consumed sequence reconciles only.
    let mut replay = report(2, true);
    replay.command_seq = Some(7);
    replay.command = Some(WorldCommand::Operation(CoreCommand::Reconcile));
    let WorldOutcome::CommandConsumed(consumed) = engine.handle_report(&replay) else {
        panic!("reused command sequence must report consumed");
    };
    assert!(consumed.command_consumed);
    assert!(consumed.ready);
}

fn typed_handshake(identity: ApplicationIdentity, caps: HostCapabilities) -> HandshakeRequest {
    HandshakeRequest {
        adapter: "adapter-1".into(),
        request: "handshake-1".into(),
        application_identity: identity,
        capabilities: caps,
    }
}

#[test]
fn handshake_refuses_ill_formed_identity_text_without_binding() {
    // Grammar gate mirrors the strict wire codec: empty, NUL-bearing, or
    // overbound appId/executable text and non-opaque processId spellings
    // refuse before any generation is minted or state mutates.
    let overlong_text = "x".repeat(4097);
    let overlong_pid = "x".repeat(129);
    let bad_identities = [
        ApplicationIdentity {
            app_id: Some(String::new()),
            executable: Some("example".into()),
            process_id: Some("pid-own-1".into()),
        },
        ApplicationIdentity {
            app_id: Some("has\0nul".into()),
            executable: Some("example".into()),
            process_id: Some("pid-own-1".into()),
        },
        ApplicationIdentity {
            app_id: Some(overlong_text.clone()),
            executable: Some("example".into()),
            process_id: Some("pid-own-1".into()),
        },
        ApplicationIdentity {
            app_id: None,
            executable: Some(String::new()),
            process_id: Some("pid-own-1".into()),
        },
        ApplicationIdentity {
            app_id: None,
            executable: Some("has\0nul".into()),
            process_id: Some("pid-own-1".into()),
        },
        ApplicationIdentity {
            app_id: None,
            executable: Some(overlong_text.clone()),
            process_id: Some("pid-own-1".into()),
        },
        ApplicationIdentity {
            app_id: None,
            executable: Some("example".into()),
            process_id: Some(String::new()),
        },
        ApplicationIdentity {
            app_id: None,
            executable: Some("example".into()),
            process_id: Some("has space".into()),
        },
        ApplicationIdentity {
            app_id: None,
            executable: Some("example".into()),
            process_id: Some(overlong_pid),
        },
    ];
    for identity in bad_identities {
        let mut engine = WorldEngine::new("gen-1").unwrap();
        let reply = engine.handshake(&typed_handshake(identity, HostCapabilities::default()));
        assert_eq!(reply.kind, Some(ProtocolKind::Malformed));
        assert!(
            reply.generation.is_none(),
            "no generation on grammar refusal"
        );
        assert_eq!(engine.capabilities(), None, "no state on grammar refusal");
        assert_eq!(engine.application_identity(), None);
        assert_eq!(engine.state_revision(), 0);
    }
}

#[test]
fn handshake_binds_split_identity_availability_consistent_with_caps() {
    // Supported executable with unavailable process binds when the process
    // value is null and listed, and vice versa.
    let mut engine = WorldEngine::new("gen-1").unwrap();
    let exe_only = engine.handshake(&typed_handshake(
        ApplicationIdentity {
            app_id: None,
            executable: Some("example".into()),
            process_id: None,
        },
        HostCapabilities {
            unavailable_facts: vec![FactName::ProcessId],
            ..Default::default()
        },
    ));
    assert_eq!(exe_only.kind, None);
    assert!(exe_only.generation.is_some());
    let mut engine = WorldEngine::new("gen-1").unwrap();
    let pid_only = engine.handshake(&typed_handshake(
        ApplicationIdentity {
            app_id: None,
            executable: None,
            process_id: Some("pid-own-1".into()),
        },
        HostCapabilities {
            unavailable_facts: vec![FactName::Executable],
            ..Default::default()
        },
    ));
    assert_eq!(pid_only.kind, None);
    assert!(pid_only.generation.is_some());
}

#[test]
fn test_bound_helper_passes_caller_capabilities_through_unmodified() {
    // The helper must never rewrite the caller's unavailableFacts: the
    // synthetic test identity follows the exact given caps instead.
    let cases = [
        HostCapabilities::default(),
        HostCapabilities::without_native_identity(),
        HostCapabilities {
            unavailable_facts: vec![FactName::Executable],
            ..Default::default()
        },
        HostCapabilities {
            unavailable_facts: vec![FactName::ProcessId],
            ..Default::default()
        },
    ];
    for caps in cases {
        let engine = WorldEngine::new_test_bound_with_config_and_initial_settings(
            "gen-1",
            "adapter-1",
            caps.clone(),
            WorldEngineConfig::default(),
            Vec::new(),
        )
        .expect("helper binds with exact caller caps");
        assert_eq!(
            engine.capabilities(),
            Some(&caps),
            "helper must not extend caller unavailableFacts"
        );
    }
}

fn accepted_reply(
    engine: &mut WorldEngine,
    report: &WorldReport,
) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn identity_report(seq: u64, windows: Vec<WindowFact>) -> WorldReport {
    let mut base = report(seq, true);
    base.windows = windows;
    base
}

fn identity_fact(
    window: &str,
    app_id: Option<&str>,
    executable: Option<&str>,
    process_id: Option<&str>,
    class: &str,
) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId("out-1".into())),
        rect: Some(Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 90,
        }),
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
        app_id: app_id.map(|value| value.to_owned()),
        executable: executable.map(|value| value.to_owned()),
        process_id: process_id.map(|value| value.to_owned()),
        class: Some(class.to_owned()),
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

fn controlled_lifetimes(reply: &crate::world_report::WorldReply) -> Vec<String> {
    reply
        .windows
        .iter()
        .filter(|row| {
            row.membership_authority == ScopeAuthority::Controlled
                && row.frame_authority == ScopeAuthority::Controlled
        })
        .map(|row| row.lifetime.0.clone())
        .collect()
}

fn effect_targets(reply: &crate::world_report::WorldReply) -> Vec<String> {
    reply
        .effects
        .iter()
        .map(|effect| effect.target.0.clone())
        .collect()
}

fn transfer_targets(reply: &crate::world_report::WorldReply) -> Vec<String> {
    reply
        .transfers
        .iter()
        .filter_map(|target| match &target.kind {
            crate::world_report::TransferKind::Window(window) => Some(window.0.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn own_process_identity_excludes_while_same_output_sibling_stays_controlled() {
    // Real-engine own-process equality (`test-own-pid` from the test-bound
    // handshake): the own row authorizes no membership, frame, transfer,
    // or effect path, while the healthy same-output sibling stays fully
    // controlled.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let reply = accepted_reply(
        &mut engine,
        &identity_report(
            1,
            vec![
                identity_fact(
                    "win-own",
                    None,
                    Some("test-own-executable"),
                    Some("test-own-pid"),
                    "Synthetic",
                ),
                identity_fact(
                    "win-healthy",
                    None,
                    Some("test-own-executable"),
                    Some("foreign-pid-9"),
                    "Synthetic",
                ),
            ],
        ),
    );
    assert!(reply.ready);
    let controlled = controlled_lifetimes(&reply);
    assert!(
        controlled.contains(&"win-healthy".to_owned()),
        "same-output sibling stays controlled: {controlled:?}"
    );
    assert!(
        !reply.windows.iter().any(|row| row.lifetime.0 == "win-own"),
        "own row authorizes no window desire: {:?}",
        reply.windows
    );
    assert!(
        !effect_targets(&reply).contains(&"win-own".to_owned()),
        "own row authorizes no effect: {:?}",
        reply.effects
    );
    assert!(
        !transfer_targets(&reply).contains(&"win-own".to_owned()),
        "own row authorizes no transfer: {:?}",
        reply.transfers
    );
    let members = engine.logical_members_all();
    assert!(
        members.iter().any(|member| member.0 == "win-healthy"),
        "sibling homed: {members:?}"
    );
    assert!(
        !members.iter().any(|member| member.0 == "win-own"),
        "own row never homed: {members:?}"
    );
}

#[test]
fn foreign_same_executable_and_settings_surface_tile_ordinarily() {
    // Process identity is never inferred from executable/appId/class:
    // foreign processes sharing the own executable (or carrying a
    // settings-bundle app id) follow ordinary tile membership.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let reply = accepted_reply(
        &mut engine,
        &identity_report(
            1,
            vec![
                identity_fact(
                    "win-same-exe",
                    None,
                    Some("test-own-executable"),
                    Some("foreign-pid-1"),
                    "Synthetic",
                ),
                identity_fact(
                    "win-settings",
                    Some("settings-app"),
                    Some("test-own-executable"),
                    Some("foreign-pid-7"),
                    "SettingsWindow",
                ),
            ],
        ),
    );
    let controlled = controlled_lifetimes(&reply);
    assert!(
        controlled.contains(&"win-same-exe".to_owned()),
        "same-exe foreign process tiles ordinarily: {controlled:?}"
    );
    assert!(
        controlled.contains(&"win-settings".to_owned()),
        "settings surface tiles ordinarily: {controlled:?}"
    );
    let members = engine.logical_members_all();
    assert!(members.iter().any(|member| member.0 == "win-same-exe"));
    assert!(members.iter().any(|member| member.0 == "win-settings"));
}

#[test]
fn supported_null_process_holds_only_dependent_row_while_unavailable_admits() {
    // Supported-but-null processId holds only the dependent row (own
    // exclusion undecidable); the same-output healthy sibling stays
    // controlled. Declaring processId unavailable admits the same row.
    let null_row = || {
        identity_fact(
            "win-null",
            None,
            Some("test-own-executable"),
            None,
            "Synthetic",
        )
    };
    let healthy_row = || {
        identity_fact(
            "win-healthy",
            None,
            Some("test-own-executable"),
            Some("foreign-pid-9"),
            "Synthetic",
        )
    };

    let mut supported = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let held = accepted_reply(
        &mut supported,
        &identity_report(1, vec![null_row(), healthy_row()]),
    );
    let controlled = controlled_lifetimes(&held);
    assert!(
        controlled.contains(&"win-healthy".to_owned()),
        "healthy sibling stays controlled past a held row: {controlled:?}"
    );
    assert!(
        !held.windows.iter().any(|row| row.lifetime.0 == "win-null"),
        "supported-null process holds the dependent row: {:?}",
        held.windows
    );
    assert!(
        held.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.id.as_deref() != Some("win-null")),
        "held row mints no spurious diagnostic: {:?}",
        held.diagnostics
    );
    let members = supported.logical_members_all();
    assert!(members.iter().any(|member| member.0 == "win-healthy"));
    assert!(
        members.iter().any(|member| member.0 == "win-null"),
        "held row keeps its retained home (never an inferred close): {members:?}"
    );

    let mut unavailable = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            unavailable_facts: vec![FactName::ProcessId],
            // Fixtures carry native sticky facts; only processId is
            // unavailable in this lane.
            native_sticky: true,
            ..Default::default()
        },
    )
    .expect("generation");
    // An unavailable fact must be carried as null: repeating the foreign
    // pid here would be a conflicting-fact hold, not an admission.
    let unavailable_healthy = || {
        identity_fact(
            "win-healthy",
            None,
            Some("test-own-executable"),
            None,
            "Synthetic",
        )
    };
    let admitted = accepted_reply(
        &mut unavailable,
        &identity_report(1, vec![null_row(), unavailable_healthy()]),
    );
    let controlled = controlled_lifetimes(&admitted);
    assert!(
        controlled.contains(&"win-null".to_owned()),
        "unavailable processId admits the null row: {controlled:?}"
    );
    assert!(
        controlled.contains(&"win-healthy".to_owned()),
        "sibling stays controlled: {controlled:?}"
    );
    assert!(
        !admitted
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.row == RowDiagnosticLane::Window
                && diagnostic.id.as_deref() == Some("win-null")),
        "no window hold once unavailable: {:?}",
        admitted.diagnostics
    );
}

#[test]
fn output_none_known_retains_home_without_new_authority() {
    // Known lifetime observed with `out: null` keeps its retained home;
    // the row carries no output authority and mints no new homing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted_reply(
        &mut engine,
        &identity_report(
            1,
            vec![
                identity_fact(
                    "win-known",
                    None,
                    Some("test-own-executable"),
                    Some("foreign-pid-9"),
                    "Synthetic",
                ),
                identity_fact(
                    "win-sibling",
                    None,
                    Some("test-own-executable"),
                    Some("foreign-pid-10"),
                    "Synthetic",
                ),
            ],
        ),
    );
    assert!(first.ready);
    let mut second = identity_report(2, Vec::new());
    let mut known_none = identity_fact(
        "win-known",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-9"),
        "Synthetic",
    );
    known_none.output = None;
    let sibling = identity_fact(
        "win-sibling",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-10"),
        "Synthetic",
    );
    second.windows = vec![known_none, sibling];
    let reply = accepted_reply(&mut engine, &second);
    assert!(reply.ready);
    let members = engine.logical_members_all();
    assert!(
        members.iter().any(|member| member.0 == "win-known"),
        "output-less known keeps retained home: {members:?}"
    );
    assert!(
        members.iter().any(|member| member.0 == "win-sibling"),
        "sibling stays homed: {members:?}"
    );
    let controlled = controlled_lifetimes(&reply);
    assert!(
        controlled.contains(&"win-sibling".to_owned()),
        "sibling stays controlled: {controlled:?}"
    );
}

#[test]
fn output_none_new_unhomed_authorizes_nothing() {
    // A brand-new lifetime with `out: null` has no retained home: it homes
    // nowhere, authorizes no desire, and retains raw evidence only.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut unhomed = identity_fact(
        "win-new",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-9"),
        "Synthetic",
    );
    unhomed.output = None;
    let sibling = identity_fact(
        "win-healthy",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-10"),
        "Synthetic",
    );
    let reply = accepted_reply(&mut engine, &identity_report(1, vec![unhomed, sibling]));
    assert!(reply.ready);
    assert!(
        !reply.windows.iter().any(|row| row.lifetime.0 == "win-new"),
        "unhomed output-less authorizes no desire: {:?}",
        reply.windows
    );
    assert!(
        controlled_lifetimes(&reply).contains(&"win-healthy".to_owned()),
        "sibling stays controlled"
    );
    let members = engine.logical_members_all();
    assert!(
        !members.iter().any(|member| member.0 == "win-new"),
        "unhomed output-less never guesses a home: {members:?}"
    );
    assert!(members.iter().any(|member| member.0 == "win-healthy"));
}

#[test]
fn held_invalid_and_conflicting_keep_kind_parity() {
    use crate::world_report::{
        RowDiagnosticKind, grade_conflicting_windows, grade_invalid_windows,
    };
    // Overbound text grades to an InvalidFact hold with no fabricated scope.
    let mut overbound = identity_fact(
        "win-over",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-9"),
        "Synthetic",
    );
    overbound.title = Some("x".repeat(crate::world_report::MAX_METADATA_TEXT_BYTES + 1));
    let mut base = report(1, true);
    base.windows = vec![overbound];
    let graded = grade_invalid_windows(&base).expect("overbound grades");
    assert_eq!(graded.windows.len(), 0);
    assert_eq!(graded.held_windows.len(), 1);
    let held = &graded.held_windows[0];
    assert_eq!(held.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(held.output, Some(OutputId("out-1".into())));
    let diagnostic = held.diagnostic.as_ref().expect("scoped diagnostic");
    assert_eq!(diagnostic.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(diagnostic.row, RowDiagnosticLane::Window);
    // A listed-unavailable fact grades to a ConflictingFact hold, same
    // shape but distinct kind.
    let mut conflict = identity_fact(
        "win-conflict",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-9"),
        "Synthetic",
    );
    conflict.privileged = Some(true);
    let mut base = report(2, true);
    base.windows = vec![conflict];
    let graded =
        grade_conflicting_windows(&base, &[FactName::Privileged], true).expect("conflict grades");
    assert_eq!(graded.windows.len(), 0);
    assert_eq!(graded.held_windows.len(), 1);
    let held = &graded.held_windows[0];
    assert_eq!(held.kind, RowDiagnosticKind::ConflictingFact);
    let diagnostic = held.diagnostic.as_ref().expect("scoped diagnostic");
    assert_eq!(diagnostic.kind, RowDiagnosticKind::ConflictingFact);
    assert_eq!(diagnostic.row, RowDiagnosticLane::Window);
}

#[test]
fn masked_owner_with_overbound_refuses_typed() {
    use crate::world_report::{grade_invalid_windows, validate_report_reason};
    // Overbound text must not mask a bad owner reference in the same row.
    let mut masked = identity_fact(
        "win-masked",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-9"),
        "Synthetic",
    );
    masked.title = Some("x".repeat(crate::world_report::MAX_METADATA_TEXT_BYTES + 1));
    masked.owner = Some(WindowId("has space".into()));
    let mut base = report(1, true);
    base.windows = vec![masked];
    assert!(
        matches!(
            validate_report_reason(&base),
            Some("window-id-invalid" | "window-metadata-invalid")
        ),
        "masked row reports: {:?}",
        validate_report_reason(&base)
    );
    assert!(
        grade_invalid_windows(&base).is_none(),
        "masked owner refuses, never holds"
    );
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    assert!(
        matches!(engine.handle_report(&base), WorldOutcome::Rejected { .. }),
        "engine refuses masked owner"
    );
}

#[test]
fn ordinary_maximized_null_without_maximize_holds_frame_only() {
    use crate::world_report::HoldReason;
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut unknown = identity_fact(
        "win-unknown",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-9"),
        "Synthetic",
    );
    unknown.maximized = None;
    let healthy = identity_fact(
        "win-healthy",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-10"),
        "Synthetic",
    );
    let reply = accepted_reply(&mut engine, &identity_report(1, vec![unknown, healthy]));
    assert!(reply.ready);
    let row_unknown = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-unknown")
        .expect("unknown row present");
    assert_eq!(
        row_unknown.membership_authority,
        ScopeAuthority::Controlled,
        "unsupported null keeps membership: {row_unknown:?}"
    );
    assert_eq!(
        row_unknown.frame_authority,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        "unknown overlay holds frame narrowly: {row_unknown:?}"
    );
    assert!(
        row_unknown.allocation.is_some(),
        "membership keeps allocation: {row_unknown:?}"
    );
    assert_eq!(
        row_unknown.clear_operation, None,
        "no clear on unknown: {row_unknown:?}"
    );
    let row_healthy = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-healthy")
        .expect("sibling row present");
    assert_eq!(row_healthy.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(row_healthy.frame_authority, ScopeAuthority::Controlled);
    assert_eq!(row_healthy.clear_operation, None);
    assert!(
        reply
            .scopes
            .iter()
            .all(|scope| scope.authority == ScopeAuthority::Controlled),
        "no broad revocation: {:?}",
        reply.scopes
    );
    assert!(engine.retained_clears.is_empty(), "no retained clear");

    // Supported capability: same null holds the dependent domain, still no
    // unsafe geometry and no clear; the other output stays controlled.
    let mut supported = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            maximize: true,
            // Fixtures carry native sticky facts; the maximize lane under
            // test is the native route.
            native_sticky: true,
            ..Default::default()
        },
    )
    .expect("generation");
    let mut unknown2 = identity_fact(
        "win-unknown",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-9"),
        "Synthetic",
    );
    unknown2.maximized = None;
    unknown2.output = Some(OutputId("out-1".into()));
    let mut healthy2 = identity_fact(
        "win-healthy",
        None,
        Some("test-own-executable"),
        Some("foreign-pid-10"),
        "Synthetic",
    );
    healthy2.output = Some(OutputId("out-2".into()));
    let mut base = report(1, true);
    base.screens.push(crate::world_report::ScreenReport {
        output: OutputId("out-2".into()),
        full_rect: Some(Rect {
            x: 800,
            y: 0,
            w: 800,
            h: 600,
        }),
        work_area: Some(Rect {
            x: 800,
            y: 0,
            w: 800,
            h: 560,
        }),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    });
    base.action_outputs = vec![OutputId("out-1".into()), OutputId("out-2".into())];
    base.windows = vec![unknown2, healthy2];
    let held = accepted_reply(&mut supported, &base);
    assert!(
        !held
            .windows
            .iter()
            .any(|row| row.lifetime.0 == "win-unknown"
                && row.membership_authority == ScopeAuthority::Controlled
                && row.frame_authority == ScopeAuthority::Controlled),
        "supported null authorizes no controlled geometry: {:?}",
        held.windows
    );
    assert!(
        held.windows.iter().all(|row| row.clear_operation.is_none()),
        "supported null authorizes no clear: {:?}",
        held.windows
    );
}
