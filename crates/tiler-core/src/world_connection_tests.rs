use crate::boundary::CoreCommand;
use crate::connection::*;
use crate::directional::OutputId;
use crate::geometry::Rect;
use crate::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};
use crate::world_report::{ScreenReport, ScreenView, WorldCommand, WorldReport};

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
        inventory_complete: complete,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: seq,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
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
