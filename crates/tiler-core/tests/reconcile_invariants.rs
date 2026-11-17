#![forbid(unsafe_code)]
//! Contract and boundary invariants.
//!
//! Retired C1 tests: 14 of the 16 original tests in this file drove the
//! deleted `tiler_core::reconcile::Reconciler` handshake/divergence machine
//! (`propose`, `acknowledge`, `verify`, `AdapterAck`, `PostObservation`,
//! `DivergenceKind`, `StateKind`) and are retired with C1.
//!
//! Exact count rationale: 16 -> 2 retained public tests:
//! - `session_ids_are_opaque_bounded_and_redacted` (retained public typed-ID
//!   bounds/parse/redaction assertions; `Reconciler`-specific dispatch/ack
//!   redaction tail retired).
//! - `portable_modules_prohibit_platform_imports` (retained platform-import
//!   prohibition over `contract.rs` and `world_layout/mod.rs`, replacing
//!   deleted `reconcile.rs`).

use tiler_core::contract::{MAX_CORRELATION_LEN, MAX_GENERATION_LEN, MAX_OWNER_LEN};
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};

#[test]
fn session_ids_are_opaque_bounded_and_redacted() {
    // Malformed session ids cannot construct typed ids (shared bounds).
    assert!(OwnerId::parse("").is_none());
    assert!(OwnerId::parse("bad id").is_none());
    assert!(OwnerId::parse(&"x".repeat(MAX_OWNER_LEN + 1)).is_none());
    assert!(GenerationId::parse("").is_none());
    assert!(GenerationId::parse("GEN-1").is_none());
    assert!(GenerationId::parse(&"g".repeat(MAX_GENERATION_LEN + 1)).is_none());
    assert!(CorrelationId::parse("").is_none());
    assert!(CorrelationId::parse("bad corr").is_none());
    assert!(CorrelationId::parse(&"c".repeat(MAX_CORRELATION_LEN + 1)).is_none());
    // Valid ids expose bytes only via as_str, never via Debug/Display.
    let owner = OwnerId::parse("owner-1").expect("valid");
    let generation = GenerationId::parse("gen-1").expect("valid");
    let correlation = CorrelationId::parse("corr-1").expect("valid");
    assert_eq!(owner.as_str(), "owner-1");
    assert_eq!(generation.as_str(), "gen-1");
    assert_eq!(correlation.as_str(), "corr-1");
    assert!(!format!("{owner:?}").contains("owner-1"));
    assert!(!format!("{generation:?}").contains("gen-1"));
    assert!(!format!("{correlation:?}").contains("corr-1"));
}

#[test]
fn portable_modules_prohibit_platform_imports() {
    // Compile-time inclusion: inspects the production sources without
    // touching host state, processes, or workdirs at test runtime.
    const CONTRACT: &str = include_str!("../src/contract.rs");
    const WORLD_LAYOUT: &str = include_str!("../src/world_layout/mod.rs");
    // Only `use`/`extern crate`/`mod` lines are import surface; comments
    // legitimately mention platform/process/IPC/geometry, so those words
    // alone must not count. Match import lines containing platform tokens.
    const BANNED: &[&str] = &[
        "zbus",
        "rustix",
        "dbus",
        "tokio",
        "async-std",
        "smol",
        "wayland",
        "x11",
        "kwin",
        "libc",
        "nix",
        "socket",
        "ipc",
        "process",
        "platform",
        "std::process",
        "std::net",
        "std::os",
        "std::fs",
        "std::env",
        "std::thread",
        "ffi",
    ];
    for (name, source) in [
        ("contract.rs", CONTRACT),
        ("world_layout/mod.rs", WORLD_LAYOUT),
    ] {
        for raw_line in source.lines() {
            let line = raw_line.trim().to_ascii_lowercase();
            if !(line.starts_with("use ")
                || line.starts_with("extern crate")
                || line.starts_with("mod "))
            {
                continue;
            }
            for token in BANNED {
                assert!(
                    !line.contains(token),
                    "{name} import line {raw_line:?} contains banned {token:?}"
                );
            }
        }
    }
    assert!(
        CONTRACT.contains("transport-independent") || CONTRACT.contains("transport-neutral"),
        "contract.rs should document its transport-independent boundary"
    );
}

#[test]
fn synchronous_atomic_revision_and_public_reply_contract() {
    use tiler_core::boundary::CoreCommand;
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::geometry::Rect;
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_engine::{WorldEngine, WorldOutcome};
    use tiler_core::world_report::{
        BatchSettlement, CommandRejectKind, CommandResult, NativeWorkspaceFact, OutputIdentity,
        PresentationIngressV4, PresentationRowV4, ScreenReport, VisibilityFact,
        WORLD_CONTRACT_VERSION_V4, WindowFact, WindowType, WorldCommand, WorldReportV4,
    };

    fn make_fact(window: &str, x: i32) -> WindowFact {
        WindowFact {
            window: WindowId(window.to_owned()),
            output: Some(OutputId("out-a".to_owned())),
            rect: Some(Rect {
                x,
                y: 0,
                w: 400,
                h: 600,
            }),
            fullscreen: Some(false),
            maximized: Some(false),
            sticky: Some(false),
            hints: Some(WindowSizeHints::none()),
            minimized: Some(false),
            native_workspace: NativeWorkspaceFact::Omitted,
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
            window_type: Some(WindowType::Normal),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
            layer: Some(0),
            fullscreen_owner: None,
            fullscreen_exitable: None,
            suppression_available: None,
            visibility: VisibilityFact {
                visible: Some(true),
                suppression_owned: None,
            },
            last_write: None,
        }
    }

    fn make_report(
        seq: u64,
        command_seq: Option<u64>,
        command: Option<WorldCommand>,
    ) -> WorldReportV4 {
        WorldReportV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq: seq,
            request_seq: seq,
            request_id: format!("req-{seq}"),
            command_seq,
            command,
            screens: vec![ScreenReport {
                output: OutputId("out-a".to_owned()),
                full_rect: Some(Rect {
                    x: 0,
                    y: 0,
                    w: 1600,
                    h: 900,
                }),
                work_area: Some(Rect {
                    x: 0,
                    y: 0,
                    w: 1600,
                    h: 900,
                }),
                complete: true,
                scale: 1.0,
                identity: OutputIdentity::unknown(),
            }],
            held_screens: Vec::new(),
            windows: vec![make_fact("win-a", 0), make_fact("win-b", 800)],
            held_windows: Vec::new(),
            closed: Vec::new(),
            native: Vec::new(),
            held_native: Vec::new(),
            cursor: None,
            cursor_hit: None,
            focused_global: "win-b".to_owned(),
            foreground_known: true,
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

    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("engine");

    // 1. Initial observation commits synchronously at revision 1.
    let first = match engine.handle_report(&make_report(1, None, None), &[]) {
        (WorldOutcome::Accepted(reply), settlements) => {
            assert!(settlements.is_empty());
            reply
        }
        (other, _) => panic!("expected accepted initial reply, got {other:?}"),
    };
    assert_eq!(first.contract_version, WORLD_CONTRACT_VERSION_V4);
    assert_eq!(first.core_generation, "gen-1");
    assert_eq!(first.adapter_incarnation, "adapter-1");
    assert_eq!(first.state_revision, 1);
    assert_eq!(first.windows.len(), 2);

    // 2. Synchronous command advances revision by exactly 1 and mutates public rects.
    let second = match engine.handle_report(
        &make_report(
            2,
            Some(11),
            Some(WorldCommand::Operation(CoreCommand::Resize {
                window: "win-b".to_owned(),
                direction: "left".to_owned(),
                mode: "outwards".to_owned(),
                press_index: 0,
            })),
        ),
        &[],
    ) {
        (WorldOutcome::Accepted(reply), settlements) => {
            assert_eq!(
                settlements,
                vec![BatchSettlement {
                    seq: 11,
                    result: None
                }]
            );
            reply
        }
        (other, _) => panic!("expected accepted resize reply, got {other:?}"),
    };
    assert_eq!(second.state_revision, 2);
    let win_b_after = second
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-b")
        .expect("win-b desire")
        .allocation;
    let win_b_before = first
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-b")
        .expect("win-b initial")
        .allocation;
    assert_ne!(win_b_after, win_b_before);

    // 3. Rejected command with secret token leaves layout rects unchanged and redacts secret.
    let (outcome, settlements) = engine.handle_report(
        &make_report(
            3,
            Some(12),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: "ws-SECRET".to_owned(),
                follow: false,
            }),
        ),
        &[],
    );
    let third = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted reply with rejected settlement, got {other:?}"),
    };
    assert_eq!(third.state_revision, 3);
    assert_eq!(
        settlements,
        vec![BatchSettlement {
            seq: 12,
            result: Some(CommandResult::Rejected {
                kind: CommandRejectKind::SendTargetUnknown,
            }),
        }]
    );
    assert!(!format!("{settlements:?}").contains("SECRET"));
    let win_b_rejected = third
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-b")
        .expect("win-b after rejected")
        .allocation;
    assert_eq!(win_b_rejected, win_b_after);
}
