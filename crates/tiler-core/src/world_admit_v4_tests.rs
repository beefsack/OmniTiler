//! Focused internal v4 admission-boundary regressions (unit 6
//! sub-increment 7e): typed replay bookkeeping, bootstrap, and bounded
//! candidate adoption through [`WorldEngine::handle_report_v4`], never
//! through public ingress or the legacy runtime.

use crate::boundary::CoreCommand;
use crate::connection::{ApplicationIdentity, HandshakeRequest, HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, OwnerId};
use crate::intent_data::{IntentData, MAX_INTENT_DATA_BYTES, encode_intent_data};
use crate::workspace_scope::DisplacedEntry;
use crate::world_admit_v4::{V4Replay, classify_replay_v4};
use crate::world_directional::tests::{fact, screen};
use crate::world_engine::{
    EventIssue, KIND_CORRELATION, KIND_GENERATION, KIND_OBS_STALE, KIND_OWNER, KIND_REPLY_SIZE,
    KIND_REUSED, KIND_REVISION_EXHAUSTED, KIND_SNAPSHOT, KIND_SUPERSEDED, LegacyWorldOutcome,
    RawRouting, WorldEngine,
};
use crate::world_report::{
    AttemptLaneV4, AuthorityScope, CommandRejectKind, CommandResult, HeldAttemptRow, HeldWindowRow,
    IntentWrite, PresentationIngressV4, PresentationRowV4, RowDiagnostic, RowDiagnosticKind,
    RowDiagnosticLane, ScreenReport, SendSource, WORLD_CONTRACT_VERSION_LEGACY,
    WORLD_CONTRACT_VERSION_V4, WORLD_MAX_REPLY_BYTES, WindowFact, WorldCommand, WorldReport,
    WorldReportV4, retained_window_fields,
};
use crate::world_report_v4_checks::{graded_effective_v4, report_digest_v4};
use crate::world_run_v4::V4RunOutcome;
use crate::world_sizing::max_intent_rollback_bound;
use crate::world_sizing_v4::{reply_estimate_v4, reply_exact_bytes_v4, reply_upper_bound_v4};

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            window_suppression: true,
            ..HostCapabilities::default()
        },
        Vec::<(String, RawSettingValue)>::new(),
    )
    .expect("valid generation")
}

fn issue(report: &WorldReportV4) -> EventIssue {
    EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse(&report.request_id).expect("correlation"),
    }
}

fn admit(engine: &mut WorldEngine, report: &WorldReportV4) -> V4RunOutcome {
    engine.handle_report_v4(report, &issue(report))
}

fn ingress(fg: &str) -> PresentationIngressV4 {
    let frame = if fg.is_empty() {
        None
    } else {
        Some(Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 90,
        })
    };
    PresentationIngressV4 {
        foreground: fg.to_owned(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: frame,
        chord_held: false,
        move_window: String::new(),
        pager_order: Vec::new(),
        pager_frames: Vec::new(),
        witness: None,
        foreground_out: if fg.is_empty() {
            None
        } else {
            Some(OutputId("out-a".to_owned()))
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn report_full(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    presentation: PresentationRowV4,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    intent_data: Vec<(String, String)>,
    closed: Vec<&str>,
    inventory_complete: bool,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens,
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
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete,
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
        presentation,
        intent_records: intent_data
            .into_iter()
            .map(|(win, data)| crate::world_report::IntentRecord {
                win: WindowId(win),
                data,
            })
            .collect(),
        renderer_input: None,
    }
}

fn report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    presentation: PresentationRowV4,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    intent_data: Vec<(String, String)>,
) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows,
        focused,
        presentation,
        command_seq,
        command,
        intent_data,
        Vec::new(),
        true,
    )
}

fn observe(seq: u64, windows: Vec<WindowFact>, focused: &str) -> WorldReportV4 {
    report(
        seq,
        windows,
        focused,
        PresentationRowV4::Valid(ingress(focused)),
        None,
        None,
        Vec::new(),
    )
}

fn facts_pair() -> Vec<WindowFact> {
    vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)]
}

fn sticky_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        sticky: Some(true),
        ..fact(window, output, x)
    }
}

fn trials(engine: &WorldEngine) -> u64 {
    engine
        .ingest_trials
        .load(std::sync::atomic::Ordering::Relaxed)
}

fn accepted_reply(report_seq: u64, outcome: V4RunOutcome) -> crate::world_report::WorldReplyV4 {
    match outcome {
        V4RunOutcome::Accepted(reply) => {
            assert_eq!(reply.based_on_observation_seq, report_seq);
            assert!(!reply.command_consumed);
            reply
        }
        other => panic!("expected accepted v4 admission, got {other:?}"),
    }
}

#[test]
fn replay_classifier_splits_digest_collision_from_duplicates() {
    assert_eq!(
        classify_replay_v4(1, 7, false, 0, 0, false),
        V4Replay::Fresh
    );
    assert_eq!(classify_replay_v4(2, 7, true, 1, 9, false), V4Replay::Fresh);
    assert_eq!(classify_replay_v4(1, 7, true, 2, 7, true), V4Replay::Stale);
    assert_eq!(
        classify_replay_v4(1, 7, true, 1, 8, false),
        V4Replay::Reused
    );
    // Digest-collision counterexample: matching digests with unequal full
    // input refuse as reused, never alias the cached reply as duplicate.
    assert_eq!(
        classify_replay_v4(1, 7, true, 1, 7, false),
        V4Replay::Reused
    );
    assert_eq!(
        classify_replay_v4(1, 7, true, 1, 7, true),
        V4Replay::Duplicate
    );
}

#[test]
fn duplicate_returns_cached_reply_without_new_revision() {
    let mut engine = engine();
    let first = accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    assert_eq!(engine.state_revision, 1);
    assert_eq!(engine.ingest_calls, 1);
    let revision_before = engine.state_revision;
    let members_before = engine.members.clone();
    match admit(&mut engine, &observe(1, facts_pair(), "win-a")) {
        V4RunOutcome::Duplicate(cached) => assert_eq!(cached, first),
        other => panic!("expected duplicate v4 admission, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision, revision_before,
        "duplicates advance no revision"
    );
    assert_eq!(engine.members, members_before, "duplicates mutate no state");
    assert_eq!(engine.ingest_calls, 1, "duplicates pay no ingest");
    assert!(
        engine.command_highwater.is_none(),
        "commandless duplicates consume no sequence"
    );
}

#[test]
fn reused_sequence_refuses_and_preserves_cache() {
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let changed = observe(1, facts_pair(), "win-b");
    match admit(&mut engine, &changed) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REUSED),
        other => panic!("expected reused refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 1, "refusal advances no revision");
    assert_eq!(engine.ingest_calls, 1, "refusal ingests nothing");
    match admit(&mut engine, &observe(1, facts_pair(), "win-a")) {
        V4RunOutcome::Duplicate(_) => {}
        other => panic!("cache survives refusal, got {other:?}"),
    }
}

#[test]
fn older_request_supersedes_before_observation_order() {
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    accepted_reply(2, admit(&mut engine, &observe(2, facts_pair(), "win-b")));
    match admit(&mut engine, &observe(1, facts_pair(), "win-a")) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_SUPERSEDED),
        other => panic!("expected superseded refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 2, "refusal advances no revision");
}

#[test]
fn stale_observation_refuses_newer_request() {
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    accepted_reply(2, admit(&mut engine, &observe(2, facts_pair(), "win-b")));
    let stale = report_full(
        3,
        1,
        vec![screen("out-a", 0)],
        facts_pair(),
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        None,
        None,
        Vec::new(),
        Vec::new(),
        true,
    );
    match admit(&mut engine, &stale) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_OBS_STALE),
        other => panic!("expected observation-stale refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 2, "refusal advances no revision");
    assert_eq!(engine.ingest_calls, 2, "refusal ingests nothing");
}

#[test]
fn fresh_consumed_command_reconciles_without_reexecution() {
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let command = WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
    };
    let executed = accepted_reply(
        2,
        admit(
            &mut engine,
            &report(
                2,
                facts_pair(),
                "win-a",
                PresentationRowV4::Valid(ingress("win-a")),
                Some(1),
                Some(command.clone()),
                Vec::new(),
            ),
        ),
    );
    assert!(
        executed
            .effects
            .iter()
            .any(|effect| effect.command_seq == Some(1)),
        "fresh command executes: {:?}",
        executed.effects
    );
    assert_eq!(engine.command_highwater, Some(1));
    let rerun = report(
        3,
        facts_pair(),
        "win-b",
        PresentationRowV4::Valid(ingress("win-b")),
        Some(1),
        Some(command),
        Vec::new(),
    );
    match admit(&mut engine, &rerun) {
        V4RunOutcome::CommandConsumed(reply) => {
            assert!(reply.command_consumed);
            assert_eq!(reply.based_on_observation_seq, 3);
            assert!(
                !reply
                    .effects
                    .iter()
                    .any(|effect| effect.command_seq == Some(1)),
                "consumed sequences never re-execute: {:?}",
                reply.effects
            );
            assert!(reply.command_result.is_none());
        }
        other => panic!("expected consumed v4 admission, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 3);
    assert_eq!(
        engine.command_highwater,
        Some(1),
        "consumed reruns consume no new sequence"
    );
}

#[test]
fn commandless_reports_leave_highwater_alone() {
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let command = WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
    };
    accepted_reply(
        2,
        admit(
            &mut engine,
            &report(
                2,
                facts_pair(),
                "win-a",
                PresentationRowV4::Valid(ingress("win-a")),
                Some(1),
                Some(command),
                Vec::new(),
            ),
        ),
    );
    accepted_reply(3, admit(&mut engine, &observe(3, facts_pair(), "win-b")));
    assert_eq!(
        engine.command_highwater,
        Some(1),
        "commandless reports never move the watermark"
    );
}

#[test]
fn pre_ready_bootstrap_emits_no_authority() {
    let mut engine = engine();
    let command = WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
    };
    let incomplete = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        facts_pair(),
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        Some(9),
        Some(command),
        Vec::new(),
        Vec::new(),
        false,
    );
    match admit(&mut engine, &incomplete) {
        V4RunOutcome::Accepted(reply) => {
            assert!(!reply.ready);
            assert!(!reply.command_consumed);
            assert_eq!(reply.state_revision, 0);
            assert_eq!(reply.presentation.rev, 0);
            assert!(reply.command_result.is_none());
            assert!(reply.effects.is_empty());
        }
        other => panic!("expected pre-ready bootstrap, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 0, "bootstrap advances no revision");
    assert!(
        engine.command_highwater.is_none(),
        "bootstrap consumes no command"
    );
    assert!(engine.members.is_empty(), "bootstrap sweeps nothing");
    assert!(!engine.has_last_v4, "bootstrap populates no replay cache");
    // A repeat bootstrap is a fresh bootstrap, never a duplicate.
    match admit(&mut engine, &incomplete) {
        V4RunOutcome::Accepted(reply) => assert!(!reply.ready),
        other => panic!("expected repeat bootstrap, got {other:?}"),
    }
}

#[test]
fn bootstrap_to_ready_progression_advances_revision() {
    let mut engine = engine();
    match admit(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            facts_pair(),
            "win-a",
            PresentationRowV4::Valid(ingress("win-a")),
            None,
            None,
            Vec::new(),
            Vec::new(),
            false,
        ),
    ) {
        V4RunOutcome::Accepted(reply) => assert!(!reply.ready),
        other => panic!("expected bootstrap, got {other:?}"),
    }
    let reply = accepted_reply(2, admit(&mut engine, &observe(2, facts_pair(), "win-a")));
    assert!(reply.ready, "ready runs publish readiness");
    assert_eq!(reply.state_revision, 1);
    assert_eq!(reply.presentation.rev, 1);
    assert_eq!(engine.state_revision, 1);
    assert!(engine.ready_latched);
}

#[test]
fn pre_ready_retains_pending_intent_then_complete_prunes() {
    let mut engine = engine();
    let data = encode_intent_data(&IntentData::new(Some(true), None));
    // Complete inventory but an incomplete screen: unusable for readiness,
    // so the pre-ready path stages intent retention and prunes the complete
    // set to live-or-held immediately.
    let mut incomplete_screen = screen("out-a", 0);
    incomplete_screen.complete = false;
    let bootstrap = report_full(
        1,
        1,
        vec![incomplete_screen],
        vec![fact("win-a", "out-a", 0)],
        "",
        PresentationRowV4::Valid(ingress("")),
        None,
        None,
        vec![
            ("win-a".to_owned(), data.clone()),
            ("win-gone".to_owned(), data.clone()),
        ],
        Vec::new(),
        true,
    );
    match admit(&mut engine, &bootstrap) {
        V4RunOutcome::Accepted(reply) => assert!(!reply.ready),
        other => panic!("expected bootstrap, got {other:?}"),
    }
    let win = WindowId("win-a".to_owned());
    assert!(
        engine.pending_intent.contains_key(&win),
        "pre-ready intent retention survives bootstrap"
    );
    assert!(
        !engine
            .pending_intent
            .contains_key(&WindowId("win-gone".to_owned())),
        "complete pre-ready inventories prune absent subjects"
    );
    accepted_reply(
        2,
        admit(
            &mut engine,
            &report(
                2,
                vec![fact("win-a", "out-a", 0)],
                "",
                PresentationRowV4::Valid(ingress("")),
                None,
                None,
                vec![("win-a".to_owned(), data)],
            ),
        ),
    );
    assert!(
        engine.intent_admitted.contains(&win),
        "first-admission intent admits on the ready run"
    );
    // Explicit close ends the lifetime: every ledger entry drops.
    accepted_reply(
        3,
        admit(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![fact("win-b", "out-a", 100)],
                "win-b",
                PresentationRowV4::Valid(ingress("win-b")),
                None,
                None,
                Vec::new(),
                vec!["win-a"],
                true,
            ),
        ),
    );
    assert!(
        !engine.pending_intent.contains_key(&win)
            && !engine.intent_admitted.contains(&win)
            && !engine.restored_floats.contains(&win)
            && !engine.restored_sticky.contains_key(&win),
        "explicit close clears departed intent ledgers"
    );
}

#[test]
fn late_import_ingests_but_admitted_latch_ignores() {
    // Closes the 7d sticky-clear ingestion-count gap with execution
    // evidence: every entry ingests exactly once, and the admitted latch
    // still ignores the late import.
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: false,
            window_suppression: true,
            maximize: true,
            fullscreen: true,
            ..HostCapabilities::default()
        },
        Vec::<(String, RawSettingValue)>::new(),
    )
    .expect("generation");
    let mut null_row = fact("win-a", "out-a", 0);
    null_row.sticky = None;
    accepted_reply(
        1,
        admit(
            &mut engine,
            &report(
                1,
                vec![null_row.clone()],
                "win-a",
                PresentationRowV4::Valid(ingress("win-a")),
                None,
                None,
                Vec::new(),
            ),
        ),
    );
    assert_eq!(engine.ingest_calls, 1);
    let win = WindowId("win-a".to_owned());
    accepted_reply(
        2,
        admit(
            &mut engine,
            &report(
                2,
                vec![null_row.clone()],
                "win-a",
                PresentationRowV4::Valid(ingress("win-a")),
                Some(1),
                Some(WorldCommand::SetSticky {
                    window: win.clone(),
                    value: true,
                }),
                Vec::new(),
            ),
        ),
    );
    assert_eq!(engine.ingest_calls, 2);
    assert_eq!(
        engine.restored_sticky.get(&win),
        Some(&false),
        "tiled origin restores with a false prior"
    );
    let clear_data = encode_intent_data(&IntentData::new(None, None));
    let cleared = accepted_reply(
        3,
        admit(
            &mut engine,
            &report(
                3,
                vec![null_row],
                "win-a",
                PresentationRowV4::Valid(ingress("win-a")),
                Some(2),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "win-a".to_owned(),
                    float_rect: None,
                })),
                vec![("win-a".to_owned(), clear_data)],
            ),
        ),
    );
    assert!(
        matches!(
            cleared.command_result,
            Some(CommandResult::Float { rect: None, .. })
        ),
        "toggle-float clear mints the tile result: {:?}",
        cleared.command_result
    );
    assert_eq!(
        engine.ingest_calls, 3,
        "the late import ingested exactly once before the latch ignored it"
    );
    assert!(
        !engine.restored_sticky.contains_key(&win),
        "clear drops sticky provenance"
    );
    assert!(
        !engine.pending_intent.contains_key(&win),
        "the admitted lifetime leaves no pending late import"
    );
    assert_eq!(engine.command_highwater, Some(2));
}

#[test]
fn structural_refusals_leave_original_unchanged() {
    let mut engine = engine();
    let mut versioned = observe(1, facts_pair(), "win-a");
    versioned.contract_version = WORLD_CONTRACT_VERSION_LEGACY;
    match admit(&mut engine, &versioned) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "version-mismatch");
        }
        other => panic!("expected version refusal, got {other:?}"),
    }
    let mut duplicated = observe(1, facts_pair(), "win-a");
    duplicated.windows.push(fact("win-a", "out-a", 0));
    match admit(&mut engine, &duplicated) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "duplicate-window");
        }
        other => panic!("expected duplicate-window refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 0, "refusals advance no revision");
    assert_eq!(engine.ingest_calls, 0, "refusals ingest nothing");
    assert!(engine.members.is_empty(), "refusals mutate nothing");
    assert!(!engine.has_last_v4, "refusals populate no replay cache");
}

#[test]
fn prebound_reply_refusal_is_atomic_then_same_seq_fits() {
    let mut engine = engine();
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    let bound_of = |windows: &[WindowFact]| {
        let probe = report(
            1,
            windows.to_vec(),
            "",
            PresentationRowV4::Valid(ingress("")),
            None,
            None,
            Vec::new(),
        );
        reply_upper_bound_v4(&probe, 0, 0, 1, 0, 0).saturating_add(max_intent_rollback_bound(0))
    };
    while bound_of(&windows) <= WORLD_MAX_REPLY_BYTES && windows.len() < 10_000 {
        let id = windows.len();
        windows.push(fact(&format!("win-pad-{id:05}"), "out-a", 0));
    }
    assert!(
        bound_of(&windows) > WORLD_MAX_REPLY_BYTES,
        "padding must trip the conservative bound"
    );
    let oversized = report(
        1,
        windows,
        "",
        PresentationRowV4::Valid(ingress("")),
        None,
        None,
        Vec::new(),
    );
    match admit(&mut engine, &oversized) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REPLY_SIZE),
        other => panic!("expected reply-oversize refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 0, "refusal advances no revision");
    assert!(
        engine.command_highwater.is_none(),
        "refused reports consume nothing"
    );
    assert_eq!(engine.ingest_calls, 0, "refusal pays no clone");
    assert!(engine.last_reply_v4.is_none(), "refusals cache no reply");
    // The same sequence works once the report fits: refusal cached nothing.
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
}

#[test]
fn revision_exhaustion_refuses_before_mutation() {
    let mut engine = engine();
    engine.state_revision = u64::MAX;
    match admit(&mut engine, &observe(1, facts_pair(), "win-a")) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REVISION_EXHAUSTED),
        other => panic!("expected revision exhaustion, got {other:?}"),
    }
    assert!(engine.members.is_empty(), "exhausted runs mutate nothing");
    assert_eq!(engine.ingest_calls, 0, "exhausted runs ingest nothing");
    assert!(!engine.has_last_v4, "exhaustion populates no replay cache");
}

#[test]
fn creation_exhaustion_discards_candidate_atomically() {
    let mut engine = engine();
    let single = || vec![fact("win-a", "out-a", 0)];
    accepted_reply(1, admit(&mut engine, &observe(1, single(), "win-a")));
    let second = report(
        2,
        single(),
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        Some(1),
        Some(WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-2".to_owned(),
            follow: false,
        }),
        Vec::new(),
    );
    accepted_reply(2, admit(&mut engine, &second));
    engine.creation_counter = u64::MAX;
    let revision_before = engine.state_revision;
    match admit(
        &mut engine,
        &report(
            3,
            single(),
            "win-a",
            PresentationRowV4::Valid(ingress("win-a")),
            Some(2),
            Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }),
            Vec::new(),
        ),
    ) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, "creation-exhausted"),
        other => panic!("exhaustion stays envelope, got {other:?}"),
    }
    assert_eq!(
        engine.command_highwater,
        Some(1),
        "envelope consumes no sequence"
    );
    assert_eq!(
        engine.state_revision, revision_before,
        "envelope mutates no revision"
    );
    // The prior cached reply survives: the refused sequence cached nothing.
    match admit(&mut engine, &second) {
        V4RunOutcome::Duplicate(_) => {}
        other => panic!("cache survives exhaustion, got {other:?}"),
    }
    engine.creation_counter = 0;
    accepted_reply(4, admit(&mut engine, &observe(4, single(), "win-a")));
}

fn legacy_report(seq: u64, windows: Vec<WindowFact>, focused: &str) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION_LEGACY,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: None,
        command: None,
        screens: vec![screen("out-a", 0)],
        windows,
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
        presentation: crate::world_report::PresentationIngress::disabled(),
        intent_records: Vec::new(),
    }
}

#[test]
fn typed_v4_cache_stays_separate_from_legacy_cache() {
    let mut engine = engine();
    match engine.handle_report_legacy(&legacy_report(1, facts_pair(), "win-a")) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("expected accepted legacy run, got {other:?}"),
    }
    // The legacy run populates no typed v4 cache: the v4 boundary treats
    // its first report as fresh, never a duplicate.
    assert!(
        !engine.has_last_v4,
        "legacy runs never touch typed v4 replay"
    );
    assert!(
        engine.last_reply_v4.is_none(),
        "legacy runs cache no typed v4 reply"
    );
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    assert!(
        engine.last_reply.is_some(),
        "v4 admission never clears the legacy cache"
    );
}

#[test]
fn raw_capability_conflict_never_aliases_preheld_input() {
    // Raw-vs-graded split: admission validates the graded copy but digests,
    // equalizes, and caches the raw input, so a conflicting fact can never
    // alias an equivalent pre-held row as a duplicate.
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: false,
            window_suppression: true,
            maximize: true,
            fullscreen: true,
            ..HostCapabilities::default()
        },
        Vec::<(String, RawSettingValue)>::new(),
    )
    .expect("generation");
    let mut sticky_fact = fact("win-a", "out-a", 0);
    sticky_fact.sticky = Some(true);
    let mut raw = observe(1, vec![sticky_fact.clone()], "win-a");
    raw.cursor = Some((10, 20));
    raw.cursor_hit = Some(WindowId("win-a".to_owned()));
    accepted_reply(1, admit(&mut engine, &raw));
    assert!(
        engine
            .staged
            .observation
            .raw_windows
            .iter()
            .any(|row| row == &sticky_fact),
        "capture keeps the raw conflicting row verbatim: {:?}",
        engine.staged.observation.raw_windows
    );
    assert_eq!(
        engine.staged.observation.cursor_hit,
        Some(WindowId("win-a".to_owned())),
        "capture keeps the raw cursor hit verbatim"
    );
    let preheld_row = HeldWindowRow {
        window: WindowId("win-a".to_owned()),
        ordinal: 0,
        output: Some(OutputId("out-a".to_owned())),
        fields: retained_window_fields(&sticky_fact),
        diagnostic: Some(RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-a".to_owned())),
            row: RowDiagnosticLane::Window,
            id: Some("win-a".to_owned()),
            kind: RowDiagnosticKind::ConflictingFact,
        }),
        kind: RowDiagnosticKind::ConflictingFact,
    };
    let mut preheld = observe(1, Vec::new(), "win-a");
    preheld.held_windows = vec![preheld_row];
    preheld.cursor = Some((10, 20));
    match admit(&mut engine, &preheld) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_REUSED, "raw mismatch reuses: {message}");
        }
        other => panic!("expected reused raw conflict, got {other:?}"),
    }
    assert_eq!(
        engine.last_report_v4.as_ref(),
        Some(&raw),
        "rejections cache nothing"
    );
}

#[test]
fn digest_collision_still_reuses_on_raw_mismatch() {
    // The digest is only a fast pre-check: full typed input equality
    // decides, so a digest match with unequal input reuses.
    let mut engine = engine();
    let accepted_report = observe(1, facts_pair(), "win-a");
    accepted_reply(1, admit(&mut engine, &accepted_report));
    let other = observe(1, facts_pair(), "win-b");
    engine.last_digest_v4 = report_digest_v4(&other);
    match admit(&mut engine, &other) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_REUSED, "digest collision reuses: {message}");
        }
        other => panic!("expected reused digest collision, got {other:?}"),
    }
    assert_eq!(
        engine.last_report_v4.as_ref(),
        Some(&accepted_report),
        "rejections cache nothing"
    );
}

#[test]
fn contaminated_staged_state_never_leaks_into_bootstrap() {
    // The bootstrap reply is an explicit empty literal, never the retained
    // staged/pager image, so stuffed staged state cannot leak into it.
    let mut engine = engine();
    let mut dirty_ingress = ingress("win-a");
    dirty_ingress.pager_order = vec![WindowId("win-z".to_owned())];
    engine.staged.presentation_v4 = Some(PresentationRowV4::Valid(dirty_ingress));
    engine.presentation_order_v4 = vec![WindowId("win-z".to_owned())];
    engine.presentation_live_v4.insert(
        WindowId("win-z".to_owned()),
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 90,
        },
    );
    engine.presentation_focused_v4 = Some(WindowId("win-z".to_owned()));
    let incomplete = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        facts_pair(),
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        None,
        None,
        Vec::new(),
        Vec::new(),
        false,
    );
    match admit(&mut engine, &incomplete) {
        V4RunOutcome::Accepted(reply) => {
            assert!(!reply.ready);
            assert_eq!(reply.presentation.foreground, "");
            assert!(reply.presentation.pager.is_empty());
            assert_eq!(reply.presentation.rev, 0);
        }
        other => panic!("expected pre-ready bootstrap, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 0, "bootstrap advances no revision");
}

#[test]
fn handshake_resets_typed_admission_state() {
    // A re-handshake replaces the engine with a fresh construction, so all
    // typed admission state (replay cache, revision, watermarks, latches,
    // publication stamps) resets together.
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let command = WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
    };
    accepted_reply(
        2,
        admit(
            &mut engine,
            &report(
                2,
                facts_pair(),
                "win-a",
                PresentationRowV4::Valid(ingress("win-a")),
                Some(1),
                Some(command),
                Vec::new(),
            ),
        ),
    );
    assert!(engine.has_last_v4);
    assert_eq!(engine.command_highwater, Some(1));
    assert!(
        engine.latest_published_presentation_rev_v4 > 0,
        "accepted runs stamp publication"
    );
    let reply = engine.handshake(&HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "req-hs".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: Some("test-own-executable".to_owned()),
            process_id: Some("test-own-pid".to_owned()),
        },
        capabilities: HostCapabilities {
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            window_suppression: true,
            ..HostCapabilities::default()
        },
    });
    assert!(reply.kind.is_none(), "handshake binds: {:?}", reply.kind);
    assert!(!engine.has_last_v4, "handshake clears the replay flag");
    assert!(
        engine.last_reply_v4.is_none(),
        "handshake clears the cached reply"
    );
    assert_eq!(engine.state_revision, 0, "handshake resets the revision");
    assert!(
        engine.command_highwater.is_none(),
        "handshake clears the watermark"
    );
    assert!(!engine.ready_latched, "handshake clears the ready latch");
    assert_eq!(
        engine.latest_published_presentation_rev_v4, 0,
        "handshake clears the publication stamp"
    );
}

#[test]
fn forged_staged_reply_never_adopts() {
    // The finalize gate re-checks the staged reply before adoption: an
    // oversized or intent-invalid forgery refuses and leaves the engine
    // exactly unchanged.
    let fresh = engine();
    let first = observe(1, facts_pair(), "win-a");
    let mut staged = fresh.clone();
    let genuine = match staged.run_accepted_v4(&first, &issue(&first), false, 1) {
        V4RunOutcome::Accepted(reply) | V4RunOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected staged run, got {other:?}"),
    };
    let mut forged = genuine;
    let pad = RowDiagnostic {
        scope: AuthorityScope::Output(OutputId("out-a".to_owned())),
        row: RowDiagnosticLane::Window,
        id: Some("p".repeat(4096)),
        kind: RowDiagnosticKind::InvalidFact,
    };
    let mut batch = 64;
    while reply_exact_bytes_v4(&forged) <= WORLD_MAX_REPLY_BYTES {
        for _ in 0..batch {
            forged.diagnostics.push(pad.clone());
        }
        batch *= 2;
        assert!(
            forged.diagnostics.len() < 1_000_000,
            "padding must trip the exact size gate"
        );
    }
    let estimate = reply_estimate_v4(&forged);
    let exact = reply_exact_bytes_v4(&forged);
    assert!(
        exact > WORLD_MAX_REPLY_BYTES,
        "forgery must exceed the reply cap (estimate {estimate}, exact {exact})"
    );
    let mut engine = fresh;
    let digest = report_digest_v4(&first);
    match engine.finalize_v4_candidate(staged, &first, digest, forged) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(
                kind, KIND_REPLY_SIZE,
                "oversized forgery refuses (estimate {estimate}, exact {exact}): {message}"
            );
        }
        other => panic!("expected reply-oversize refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 0, "refusal advances no revision");
    assert!(
        engine.command_highwater.is_none(),
        "refusal consumes nothing"
    );
    assert!(!engine.has_last_v4, "refusal populates no replay cache");
    assert!(engine.last_reply_v4.is_none(), "refusal caches no reply");
    let mut staged = engine.clone();
    let genuine = match staged.run_accepted_v4(&first, &issue(&first), false, 1) {
        V4RunOutcome::Accepted(reply) | V4RunOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected staged run, got {other:?}"),
    };
    let mut forged = genuine;
    forged.intent_writes = vec![IntentWrite {
        win: WindowId("win-a".to_owned()),
        data: Some("x".repeat(MAX_INTENT_DATA_BYTES + 1)),
    }];
    match engine.finalize_v4_candidate(staged, &first, digest, forged) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT, "intent forgery refuses: {message}");
            assert_eq!(message, "intent-data-invalid");
        }
        other => panic!("expected snapshot-invalid refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 0, "refusal advances no revision");
    assert!(!engine.has_last_v4, "refusal populates no replay cache");
    assert!(engine.last_reply_v4.is_none(), "refusal caches no reply");
}

#[test]
fn policy_terminal_reserve_matches_legacy_when_displaced() {
    // The v4 wrapper shares the input-only live-output core with the legacy
    // entry, so both agree on every screen lane, including nonzero lanes.
    let mut engine = engine();
    engine.displaced.insert(
        "out-gone".to_owned(),
        DisplacedEntry {
            workspaces: vec!["ws-a".to_owned()],
            owned: vec!["ws-a".to_owned()],
            dest: "out-a".to_owned(),
            origin_identity: crate::world_report::OutputIdentity::unknown(),
            origin_tuple_contested: false,
            origin_connector_contested: false,
        },
    );
    engine
        .mapping
        .insert("out-gone".to_owned(), vec!["ws-a".to_owned()]);
    let legacy = legacy_report(1, facts_pair(), "win-a");
    let typed = observe(1, facts_pair(), "win-a");
    assert_eq!(legacy.screens.len(), typed.screens.len());
    let legacy_reserve = engine.policy_terminal_reserve(&legacy);
    let typed_reserve = engine.policy_terminal_reserve_v4(&typed);
    assert_eq!(typed_reserve, legacy_reserve, "wrapper matches legacy");
    assert!(
        typed_reserve > 0,
        "populated displaced/mapping lanes must reserve"
    );
}

#[test]
fn refused_numbered_send_rolls_back_trailing_create_through_boundary() {
    // Incomplete inventory skips lifecycle's spare mint, forcing the numbered
    // trailing send to create before the sticky mover refuses. Complete
    // inventory would instead supply a spare for mutation-free reuse, making
    // that setup unable to discriminate the pre-command restore.
    let mut engine = engine();
    let base_windows = || {
        vec![
            fact("w0", "out-a", 0),
            sticky_fact("wSticky", "out-a", 100),
            fact("wClose", "out-a", 200),
        ]
    };
    accepted_reply(
        1,
        admit(
            &mut engine,
            &report(
                1,
                base_windows(),
                "w0",
                PresentationRowV4::Valid(ingress("w0")),
                None,
                None,
                Vec::new(),
            ),
        ),
    );
    accepted_reply(
        2,
        admit(
            &mut engine,
            &report(
                2,
                base_windows(),
                "w0",
                PresentationRowV4::Valid(ingress("w0")),
                Some(1),
                Some(WorldCommand::SelectTrailing { output: None }),
                Vec::new(),
            ),
        ),
    );
    let sent = accepted_reply(
        3,
        admit(
            &mut engine,
            &report(
                3,
                base_windows(),
                "w0",
                PresentationRowV4::Valid(ingress("w0")),
                Some(2),
                Some(WorldCommand::Send {
                    window: WindowId("w0".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-2".to_owned(),
                    follow: true,
                }),
                Vec::new(),
            ),
        ),
    );
    assert!(
        sent.command_result.is_none(),
        "occupying send executes, got {:?}",
        sent.command_result
    );
    assert!(
        engine
            .ring_occupied(&OutputId("out-a".to_owned()))
            .contains("ws-2"),
        "trailing occupied so the numbered send must take the Create path"
    );
    // Fresh observation for the refused report: a new window and the real
    // close of a bootstrapped lifetime must incorporate despite the refusal.
    // Incomplete inventory holds lifecycle so the occupied trailing forces
    // the Create path (see above); explicit closes still drop.
    let refused_windows = vec![
        fact("w0", "out-a", 0),
        sticky_fact("wSticky", "out-a", 100),
        fact("w2", "out-a", 300),
    ];
    let refused = report_full(
        4,
        4,
        vec![screen("out-a", 0)],
        refused_windows,
        "w0",
        PresentationRowV4::Valid(ingress("w0")),
        Some(3),
        Some(WorldCommand::SendNumbered {
            window: WindowId("wSticky".to_owned()),
            source: SendSource::Explicit(OutputId("out-a".to_owned())),
            index: 0,
            follow: false,
        }),
        Vec::new(),
        vec!["wClose"],
        false,
    );
    // Commandless control over the same observation: the refused boundary
    // run must converge identically apart from the consumed sequence and
    // typed result. The control never sees the refused command.
    let mut control = engine.clone();
    let mut control_obs = refused.clone();
    control_obs.command_seq = None;
    control_obs.command = None;
    accepted_reply(4, admit(&mut control, &control_obs));
    let reply = accepted_reply(4, admit(&mut engine, &refused));
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::SendRefused
        }),
        "sticky send refuses with its catalog kind"
    );
    assert_eq!(engine.command_highwater, Some(3));
    assert_eq!(
        engine.mapping, control.mapping,
        "refusal preserves the commandless mapping"
    );
    assert_eq!(
        engine.creation_counter, control.creation_counter,
        "trailing creation rolls back to the commandless counter"
    );
    assert_eq!(
        engine.retained, control.retained,
        "no transfer minted for the refused mover"
    );
    assert_eq!(
        engine.members, control.members,
        "membership matches the commandless image"
    );
    assert_eq!(engine.tiling, control.tiling, "created mode rolls back");
    assert_eq!(
        engine.staged.domains, control.staged.domains,
        "created domain rolls back"
    );
    assert!(
        !engine.retained.iter().any(|transfer| matches!(&transfer.target.kind, crate::world_report::TransferKind::Window(w) if w.0 == "wSticky")),
        "refused sticky leaves no flight: {:?}",
        engine.retained
    );
    let ids: Vec<String> = reply
        .windows
        .iter()
        .map(|window| window.lifetime.0.clone())
        .collect();
    assert!(
        ids.contains(&"w2".to_owned()),
        "new window publishes: {ids:?}"
    );
    assert!(
        !ids.contains(&"wClose".to_owned()),
        "closed lifetime drops from published rows: {ids:?}"
    );
}

#[test]
fn ingest_sensor_counts_adopted_and_discarded_trials() {
    // Exact ingestion accounting through the genuine `handle_report_v4`
    // boundary: `ingest_calls` counts adoptions on `self`, while the
    // clone-shared `ingest_trials` sensor counts every genuine run trial,
    // including staged candidates that are later discarded. Command
    // restaging stays classification-only, so each accepted entry pays
    // exactly one trial.
    let mut engine = engine();
    assert_eq!(engine.ingest_calls, 0);
    assert_eq!(trials(&engine), 0, "fresh engine runs no trials");
    // Early refusal pays no clone and no trial.
    let mut versioned = observe(1, facts_pair(), "win-a");
    versioned.contract_version = WORLD_CONTRACT_VERSION_LEGACY;
    match admit(&mut engine, &versioned) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "version-mismatch");
        }
        other => panic!("expected version refusal, got {other:?}"),
    }
    assert_eq!(engine.ingest_calls, 0, "refusals ingest nothing");
    assert_eq!(trials(&engine), 0, "early refusal runs no trial");
    // Successful entries pay exactly one trial each, restaging included.
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    assert_eq!(engine.ingest_calls, 1);
    assert_eq!(trials(&engine), 1, "one adopted entry runs one trial");
    accepted_reply(
        2,
        admit(
            &mut engine,
            &report(
                2,
                facts_pair(),
                "win-a",
                PresentationRowV4::Valid(ingress("win-a")),
                Some(1),
                Some(WorldCommand::Focus {
                    window: WindowId("win-b".to_owned()),
                    direction: String::new(),
                }),
                Vec::new(),
            ),
        ),
    );
    assert_eq!(engine.ingest_calls, 2);
    assert_eq!(
        trials(&engine),
        2,
        "command restaging ingests exactly once per entry"
    );
    let sent = accepted_reply(
        3,
        admit(
            &mut engine,
            &report(
                3,
                facts_pair(),
                "win-a",
                PresentationRowV4::Valid(ingress("win-a")),
                Some(2),
                Some(WorldCommand::Send {
                    window: WindowId("win-a".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-2".to_owned(),
                    follow: false,
                }),
                Vec::new(),
            ),
        ),
    );
    assert!(
        sent.command_result.is_none(),
        "occupying send executes, got {:?}",
        sent.command_result
    );
    assert_eq!(engine.ingest_calls, 3);
    assert_eq!(trials(&engine), 3, "third entry runs its own trial");
    // A late discarded candidate still trials once on its staged clone:
    // creation exhaustion refuses only after the trial ingested, and the
    // drop adopts nothing onto `self`. Snapshot the pre-refusal image:
    // the envelope refusal must leave state, watermarks, and replay cache
    // exactly unchanged apart from the shared trial tick.
    let saved_mapping = engine.mapping.clone();
    let saved_retained = engine.retained.clone();
    let saved_members = engine.members.clone();
    let saved_highwater = engine.command_highwater;
    let saved_revision = engine.state_revision;
    let saved_reply = engine.last_reply_v4.clone();
    let saved_request_seq = engine.last_request_seq_v4;
    engine.creation_counter = u64::MAX;
    match admit(
        &mut engine,
        &report(
            4,
            facts_pair(),
            "win-a",
            PresentationRowV4::Valid(ingress("win-a")),
            Some(3),
            Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }),
            Vec::new(),
        ),
    ) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, "creation-exhausted"),
        other => panic!("exhaustion stays envelope, got {other:?}"),
    }
    assert_eq!(
        engine.ingest_calls, 3,
        "discarded candidate adopts no ingest onto self"
    );
    assert_eq!(
        trials(&engine),
        4,
        "discarded candidate still ran exactly one trial"
    );
    assert_eq!(
        engine.mapping, saved_mapping,
        "envelope refusal adopts no mapping"
    );
    assert_eq!(
        engine.retained, saved_retained,
        "envelope refusal adopts no transfers"
    );
    assert_eq!(
        engine.members, saved_members,
        "envelope refusal adopts no membership"
    );
    assert_eq!(
        engine.command_highwater, saved_highwater,
        "envelope refusal consumes no sequence"
    );
    assert_eq!(
        engine.state_revision, saved_revision,
        "envelope refusal advances no revision"
    );
    assert_eq!(
        engine.last_reply_v4, saved_reply,
        "envelope refusal caches no reply"
    );
    assert_eq!(
        engine.last_request_seq_v4, saved_request_seq,
        "envelope refusal moves no replay watermark"
    );
    assert_eq!(engine.creation_counter, u64::MAX);
    // Replays run no trial at all. The duplicate hits the replay cache,
    // so no counter fixup is needed: the cached reply returns before any
    // creation gate reads the saturated counter.
    match admit(
        &mut engine,
        &report(
            3,
            facts_pair(),
            "win-a",
            PresentationRowV4::Valid(ingress("win-a")),
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: "ws-2".to_owned(),
                follow: false,
            }),
            Vec::new(),
        ),
    ) {
        V4RunOutcome::Duplicate(_) => {}
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert_eq!(engine.ingest_calls, 3, "duplicates pay no ingest");
    assert_eq!(trials(&engine), 4, "duplicates run no trial");
    assert_eq!(engine.mapping, saved_mapping, "duplicate adopts no mapping");
    assert_eq!(
        engine.command_highwater, saved_highwater,
        "duplicate consumes no sequence"
    );
}

#[test]
fn slice_7_1_invalid_screen_scale_held_no_authority() {
    // An invalid-only first observation never readies: readiness keys on
    // the effective graded report, so the invalid-held screen alone keeps
    // the engine on the bootstrap path with no authority, no revision, and
    // no command consumption.
    let mut engine = engine();
    let mut bad = screen("out-a", 0);
    bad.scale = 0.0;
    let mut invalid = observe(1, facts_pair(), "win-a");
    invalid.screens = vec![bad];
    let revision_before = engine.state_revision;
    match admit(&mut engine, &invalid) {
        V4RunOutcome::Accepted(reply) => {
            assert!(
                !reply.ready,
                "invalid-only first observation stays bootstrap"
            );
            assert!(
                reply.scopes.is_empty()
                    && reply.workspaces.is_empty()
                    && reply.outputs.is_empty()
                    && reply.windows.is_empty(),
                "bootstrap carries no authority"
            );
            assert!(!reply.command_consumed, "bootstrap consumes no command");
        }
        other => panic!("expected accepted invalid screen, got {other:?}"),
    }
    assert!(!engine.ready_latched, "invalid hold latches no readiness");
    assert_eq!(
        engine.state_revision, revision_before,
        "bootstrap advances no revision"
    );
    assert!(
        engine.command_highwater.is_none(),
        "bootstrap consumes no sequence"
    );
    assert!(!engine.has_last_v4, "bootstrap caches no replay");
    assert!(
        engine.staged.screens.is_empty() && engine.staged.held_screens.is_empty(),
        "bootstrap stages no screens or holds"
    );
    // A later valid observation readies and takes authority.
    let ready = accepted_reply(2, admit(&mut engine, &observe(2, facts_pair(), "win-a")));
    assert!(ready.ready, "valid observation readies");
    assert!(engine.ready_latched);
    assert_eq!(
        engine.state_revision,
        revision_before + 1,
        "valid observation advances one revision"
    );
    assert_eq!(engine.staged.screens.len(), 1);
    // The same invalid screen on the ready path holds narrowly with no
    // staged screen authority.
    let mut bad_again = screen("out-a", 0);
    bad_again.scale = 0.0;
    let mut third = observe(3, facts_pair(), "win-a");
    third.screens = vec![bad_again];
    match admit(&mut engine, &third) {
        V4RunOutcome::Accepted(reply) => {
            assert!(reply.ready, "ready path stays ready");
        }
        other => panic!("expected accepted invalid screen, got {other:?}"),
    }
    assert_eq!(
        engine.staged.screens.len(),
        0,
        "bad screen stages no screen"
    );
    assert_eq!(engine.staged.held_screens.len(), 1, "bad screen held");
    let held = &engine.staged.held_screens[0];
    assert_eq!(held.output.0, "out-a");
    assert_eq!(held.ordinal, 0);
    assert_eq!(held.diagnostic.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(held.diagnostic.row, RowDiagnosticLane::Screen);
    assert!(engine.staged.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == RowDiagnosticKind::InvalidFact
            && diagnostic.row == RowDiagnosticLane::Screen
    }));
}

#[test]
fn slice_7_1_nonfinite_scale_retained_faithfully() {
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let mut bad = screen("out-a", 0);
    bad.scale = f64::NAN;
    let mut invalid = observe(2, facts_pair(), "win-a");
    invalid.screens = vec![bad];
    match admit(&mut engine, &invalid) {
        V4RunOutcome::Accepted(_) => {}
        other => panic!("expected accepted nonfinite screen, got {other:?}"),
    }
    let held = &engine.staged.held_screens[0];
    let scale = held
        .fields
        .iter()
        .find(|(key, _)| key == "scale")
        .map(|(_, value)| value);
    match scale {
        Some(RawSettingValue::Number(value)) => {
            assert!(!value.is_finite(), "nonfinite stays a held number");
        }
        other => panic!("scale must stay a held number, got {other:?}"),
    }
}

#[test]
fn slice_7_1_invalid_window_metadata_held_no_authority() {
    let mut engine = engine();
    let mut bad = fact("win-bad", "out-a", 0);
    bad.title = Some("x".repeat(5000));
    let invalid = observe(1, vec![fact("win-a", "out-a", 0), bad], "win-a");
    match admit(&mut engine, &invalid) {
        V4RunOutcome::Accepted(_) => {}
        other => panic!("expected accepted invalid window, got {other:?}"),
    }
    assert!(
        !engine
            .staged
            .facts
            .contains_key(&WindowId("win-bad".to_owned())),
        "bad window stages no fact"
    );
    assert!(
        engine
            .staged
            .held_windows
            .iter()
            .any(|held| held.window.0 == "win-bad"),
        "bad window held: {:?}",
        engine.staged.held_windows
    );
    let held = engine
        .staged
        .held_windows
        .iter()
        .find(|held| held.window.0 == "win-bad")
        .expect("held row");
    assert_eq!(held.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(
        held.diagnostic.as_ref().expect("diagnostic").kind,
        RowDiagnosticKind::InvalidFact
    );
    assert!(
        engine
            .staged
            .observation
            .raw_windows
            .iter()
            .any(|row| row.window.0 == "win-bad"),
        "raw capture keeps the bad row verbatim"
    );
}

#[test]
fn slice_7_1_mixed_invalid_compose_with_conflict() {
    let mut capped = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            maximize: true,
            fullscreen: true,
            native_sticky: false,
            above: true,
            window_suppression: true,
            ..HostCapabilities::default()
        },
        Vec::<(String, RawSettingValue)>::new(),
    )
    .expect("generation");
    let mut bad_screen = screen("out-a", 0);
    bad_screen.scale = 0.0;
    let mut bad_window = fact("win-bad", "out-a", 0);
    bad_window.title = Some("y".repeat(5000));
    let mut conflict = fact("win-c", "out-a", 100);
    conflict.sticky = Some(true);
    // Readiness first: the invalid-only screen lane alone bootstraps, so
    // the composed holds below run on the ready path.
    let baseline = observe(1, facts_pair(), "win-a");
    match capped.handle_report_v4(&baseline, &issue(&baseline)) {
        V4RunOutcome::Accepted(_) => {}
        other => panic!("expected valid baseline, got {other:?}"),
    }
    let mut mixed = observe(
        2,
        vec![fact("win-a", "out-a", 0), bad_window, conflict],
        "win-a",
    );
    mixed.screens = vec![bad_screen];
    match capped.handle_report_v4(&mixed, &issue(&mixed)) {
        V4RunOutcome::Accepted(_) => {}
        other => panic!("expected composed invalid holds, got {other:?}"),
    }
    assert_eq!(capped.staged.held_screens.len(), 1, "screen invalid held");
    let kinds: Vec<RowDiagnosticKind> = capped
        .staged
        .held_windows
        .iter()
        .map(|held| held.kind)
        .collect();
    assert!(
        kinds.contains(&RowDiagnosticKind::InvalidFact),
        "window invalid held: {kinds:?}"
    );
    assert!(
        kinds.contains(&RowDiagnosticKind::ConflictingFact),
        "conflict held alongside invalid: {kinds:?}"
    );
    assert!(
        !capped
            .staged
            .facts
            .contains_key(&WindowId("win-bad".to_owned()))
    );
    assert!(
        !capped
            .staged
            .facts
            .contains_key(&WindowId("win-c".to_owned()))
    );
}

#[test]
fn slice_7_1_structural_still_refuses_before_mutation() {
    let mut engine = engine();
    let mut duplicated = observe(1, facts_pair(), "win-a");
    duplicated.windows.push(fact("win-a", "out-a", 0));
    let revision_before = engine.state_revision;
    let calls_before = engine.ingest_calls;
    match admit(&mut engine, &duplicated) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "duplicate-window");
        }
        other => panic!("expected duplicate refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision_before);
    assert_eq!(engine.ingest_calls, calls_before);
    assert!(!engine.has_last_v4);

    let mut hidden = observe(1, facts_pair(), "win-a");
    hidden.windows[0].owner = Some(WindowId("bad owner".to_owned()));
    hidden.windows[0].title = Some("z".repeat(5000));
    match admit(&mut engine, &hidden) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_SNAPSHOT),
        other => panic!("expected hidden id refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision_before);
    assert!(!engine.has_last_v4);

    let mut overlap = observe(1, facts_pair(), "win-a");
    overlap.closed = vec![WindowId("win-a".to_owned())];
    match admit(&mut engine, &overlap) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "closed-overlaps-live");
        }
        other => panic!("expected closed overlap refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision_before);
    assert!(!engine.has_last_v4);
}

#[test]
fn slice_7_1_envelope_fencing_refuses_before_mutation() {
    let mut engine = engine();
    let base = observe(1, facts_pair(), "win-a");
    let revision_before = engine.state_revision;
    let calls_before = engine.ingest_calls;

    let mut bad_adapter = base.clone();
    bad_adapter.adapter_incarnation = "adapter-other".to_owned();
    match engine.handle_report_v4(&bad_adapter, &issue(&bad_adapter)) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_OWNER),
        other => panic!("expected adapter refusal, got {other:?}"),
    }

    let mut bad_gen = base.clone();
    bad_gen.core_generation = "gen-999".to_owned();
    match engine.handle_report_v4(&bad_gen, &issue(&bad_gen)) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_GENERATION),
        other => panic!("expected generation refusal, got {other:?}"),
    }

    let wrong_owner = EventIssue {
        owner: OwnerId::parse("adapter-other").expect("owner"),
        correlation: CorrelationId::parse(&base.request_id).expect("correlation"),
    };
    match engine.handle_report_v4(&base, &wrong_owner) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_OWNER),
        other => panic!("expected owner refusal, got {other:?}"),
    }

    let wrong_correlation = EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse("req-999").expect("correlation"),
    };
    match engine.handle_report_v4(&base, &wrong_correlation) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_CORRELATION),
        other => panic!("expected correlation refusal, got {other:?}"),
    }

    assert_eq!(
        engine.state_revision, revision_before,
        "envelope refuses no revision"
    );
    assert_eq!(
        engine.ingest_calls, calls_before,
        "envelope refuses no ingest"
    );
    assert!(!engine.has_last_v4, "envelope refuses no cache");
    assert!(engine.members.is_empty(), "envelope refuses no mutation");
}

#[test]
fn slice_7_1_held_native_capability_fence() {
    let mut engine = engine();
    assert!(!engine.host_owned_workspace_domains());
    let mut held_native = observe(1, facts_pair(), "win-a");
    held_native.held_native = vec![crate::world_report::HeldNativeRow {
        output: OutputId("out-a".to_owned()),
        ordinal: 0,
        fields: vec![(
            "out".to_owned(),
            RawSettingValue::String("out-a".to_owned()),
        )],
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-a".to_owned())),
            row: RowDiagnosticLane::Native,
            id: Some("out-a".to_owned()),
            kind: RowDiagnosticKind::InvalidFact,
        },
    }];
    let revision_before = engine.state_revision;
    match admit(&mut engine, &held_native) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "native-without-capability");
        }
        other => panic!("expected held native fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision_before);
    assert!(!engine.has_last_v4);
}

#[test]
fn slice_7_1_raw_invalid_never_aliases_preheld() {
    let mut engine = engine();
    let mut bad = fact("win-bad", "out-a", 0);
    bad.title = Some("q".repeat(5000));
    let raw = observe(1, vec![fact("win-a", "out-a", 0), bad.clone()], "win-a");
    match admit(&mut engine, &raw) {
        V4RunOutcome::Accepted(_) => {}
        other => panic!("expected accepted raw invalid, got {other:?}"),
    }
    let mut preheld = observe(1, vec![fact("win-a", "out-a", 0)], "win-a");
    preheld.held_windows = engine.staged.held_windows.clone();
    match admit(&mut engine, &preheld) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REUSED),
        other => panic!("expected reused raw mismatch, got {other:?}"),
    }
    assert_eq!(engine.last_report_v4.as_ref(), Some(&raw));
}

fn workspace_ref_value(native: bool) -> RawSettingValue {
    RawSettingValue::Object(vec![
        (
            "kind".to_owned(),
            RawSettingValue::String(if native {
                "native".to_owned()
            } else {
                "logical".to_owned()
            }),
        ),
        (
            "out".to_owned(),
            RawSettingValue::String("out-a".to_owned()),
        ),
        ("ws".to_owned(), RawSettingValue::String("ws-1".to_owned())),
    ])
}

fn held_member_row(pre_native: bool, post_native: bool) -> HeldAttemptRow {
    HeldAttemptRow {
        lane: AttemptLaneV4::Member,
        ordinal: 0,
        identity: vec!["op-1".to_owned(), "win-a".to_owned()],
        fields: vec![
            ("op".to_owned(), RawSettingValue::String("op-1".to_owned())),
            (
                "win".to_owned(),
                RawSettingValue::String("win-a".to_owned()),
            ),
            (
                "out".to_owned(),
                RawSettingValue::String("out-a".to_owned()),
            ),
            ("ws".to_owned(), RawSettingValue::String("ws-1".to_owned())),
            (
                "setter".to_owned(),
                RawSettingValue::String("accepted".to_owned()),
            ),
            ("pre".to_owned(), workspace_ref_value(pre_native)),
            ("post".to_owned(), workspace_ref_value(post_native)),
            (
                "attemptMs".to_owned(),
                RawSettingValue::String("1".to_owned()),
            ),
        ],
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }
}

fn held_view_row() -> HeldAttemptRow {
    HeldAttemptRow {
        lane: AttemptLaneV4::View,
        ordinal: 0,
        identity: vec!["op-v".to_owned()],
        fields: vec![("op".to_owned(), RawSettingValue::String("op-v".to_owned()))],
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }
}

#[test]
fn slice_7_1_held_member_logical_accepted_native_refused() {
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    // Logical-only held member rows stay admissible on logical hosts,
    // matching the valid member-attempt gate.
    let mut logical = observe(2, facts_pair(), "win-a");
    logical.held_attempts = vec![held_member_row(false, false)];
    match admit(&mut engine, &logical) {
        V4RunOutcome::Accepted(_) => {}
        other => panic!("expected accepted logical held member, got {other:?}"),
    }
    assert_eq!(
        engine.staged.held_attempts.len(),
        1,
        "logical held member stages"
    );
    assert_eq!(engine.state_revision, 2);
    // A held member row retaining a native `pre` reference is still
    // native evidence and refuses without capability.
    let mut native = observe(3, facts_pair(), "win-a");
    native.held_attempts = vec![held_member_row(true, false)];
    let revision_before = engine.state_revision;
    let calls_before = engine.ingest_calls;
    match admit(&mut engine, &native) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "native-without-capability");
        }
        other => panic!("expected held native member fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision_before);
    assert_eq!(engine.ingest_calls, calls_before);
    assert_eq!(
        engine.last_request_seq_v4, 2,
        "refusal adopts no replay cache"
    );
    // Held view rows stay native-only: any held view attempt refuses.
    let mut view = observe(3, facts_pair(), "win-a");
    view.held_attempts = vec![held_view_row()];
    match admit(&mut engine, &view) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "native-without-capability");
        }
        other => panic!("expected held view fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision_before);
}

#[test]
fn slice_7_1_invalid_screen_not_routing_authority() {
    // Codec-equivalent invalid-held screens must not become command
    // routing authority merely because the raw input carried them: the
    // pre-observation freeze reads the effective graded rows.
    let mut control_engine = engine();
    let mut engine = engine();
    let mut first = report_full(
        1,
        1,
        vec![screen("out-a", 0), screen("out-b", 800)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        None,
        None,
        Vec::new(),
        Vec::new(),
        true,
    );
    first.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    accepted_reply(1, admit(&mut engine, &first));
    // A newcomer with no retained home resolves its implicit send
    // source from the action context: the cursor leg must not see the
    // invalid-held screen.
    let command = WorldCommand::SendRelative {
        window: WindowId("win-new".to_owned()),
        source: SendSource::HomeThenAction,
        delta: 1,
        follow: false,
    };
    let mut bad = screen("out-b", 800);
    bad.scale = 0.0;
    let mut invalid = report_full(
        2,
        2,
        vec![screen("out-a", 0), bad],
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-b", 800),
            fact("win-new", "out-a", 40),
        ],
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        Some(1),
        Some(command.clone()),
        Vec::new(),
        Vec::new(),
        true,
    );
    invalid.cursor = Some((900, 10));
    invalid.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    // Mechanism check: raw routing maps the cursor onto the invalid
    // screen while the effective routing does not, so the frozen send
    // sources differ.
    let raw_routing = RawRouting::from_v4(&invalid);
    let graded = graded_effective_v4(&invalid, &[], true, true).expect("grades invalid screen");
    let effective_routing = RawRouting::from_v4(&graded);
    assert_ne!(
        engine.freeze_command_sources(Some(&command), &raw_routing),
        engine.freeze_command_sources(Some(&command), &effective_routing),
        "raw invalid screen perturbs frozen sources"
    );
    // Wiring check: the admitted run matches the effective-equivalent
    // control (valid screen plus the same pre-held invalid row), never
    // the raw routing.
    let mut control = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-b", 800),
            fact("win-new", "out-a", 40),
        ],
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        Some(1),
        Some(command),
        Vec::new(),
        Vec::new(),
        true,
    );
    control.cursor = Some((900, 10));
    control.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    control.held_screens = graded.held_screens.clone();
    accepted_reply(1, control_engine.handle_report_v4(&first, &issue(&first)));
    let reply = accepted_reply(2, admit(&mut engine, &invalid));
    let control_reply = accepted_reply(
        2,
        control_engine.handle_report_v4(&control, &issue(&control)),
    );
    assert_eq!(
        reply.command_result, control_reply.command_result,
        "invalid screen freezes as its held equivalent"
    );
    assert_eq!(
        engine.command_highwater, control_engine.command_highwater,
        "invalid screen consumes as its held equivalent"
    );
}

#[test]
fn slice_7_1_mixed_invalid_structural_refused_atomically() {
    // Mixed invalid+structural reports refuse transactionally on the
    // ready path: rejection with no revision, ingest, replay-cache, or
    // staged-hold adoption. Readiness comes from a valid first
    // observation; the invalid lane alone would bootstrap instead.
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let calls_after_ready = engine.ingest_calls;
    // Bad scale plus a duplicate window: grading bails on the
    // structural defect, so the invalid lane names the refusal.
    let mut bad = screen("out-a", 0);
    bad.scale = 0.0;
    let mut mixed = observe(2, facts_pair(), "win-a");
    mixed.screens = vec![bad];
    mixed.windows.push(fact("win-a", "out-a", 0));
    match admit(&mut engine, &mixed) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "screen-scale-invalid");
        }
        other => panic!("expected mixed refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 1);
    assert_eq!(engine.ingest_calls, calls_after_ready);
    assert_eq!(
        engine.last_request_seq_v4, 1,
        "mixed refusal caches nothing"
    );
    assert!(
        engine.staged.held_screens.is_empty() && engine.staged.held_windows.is_empty(),
        "mixed refusal stages no holds"
    );
    // Bad window metadata plus duplicate screens: structural refusal wins.
    let mut bad_window = fact("win-bad", "out-a", 0);
    bad_window.title = Some("y".repeat(5000));
    let mut mixed_screens = observe(2, vec![fact("win-a", "out-a", 0), bad_window], "win-a");
    mixed_screens.screens = vec![screen("out-a", 0), screen("out-a", 0)];
    match admit(&mut engine, &mixed_screens) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "duplicate-screen");
        }
        other => panic!("expected duplicate-screen refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 1);
    assert_eq!(engine.ingest_calls, calls_after_ready);
    assert_eq!(engine.last_request_seq_v4, 1);
    // Out-of-range held ordinals refuse without adoption.
    let mut ordinal = observe(2, facts_pair(), "win-a");
    ordinal.held_windows = vec![HeldWindowRow {
        window: WindowId("win-held".to_owned()),
        ordinal: 99,
        output: Some(OutputId("out-a".to_owned())),
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    match admit(&mut engine, &ordinal) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "held-ordinal-invalid");
        }
        other => panic!("expected held ordinal refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, 1);
    assert_eq!(engine.last_request_seq_v4, 1);
}

#[test]
fn slice_7_1_invalid_version_and_unbound_owner_refused() {
    let mut engine = engine();
    let revision_before = engine.state_revision;
    let mut version = observe(1, facts_pair(), "win-a");
    version.contract_version = WORLD_CONTRACT_VERSION_LEGACY;
    match admit(&mut engine, &version) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "version-mismatch");
        }
        other => panic!("expected version refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision_before);
    assert!(!engine.has_last_v4);
    let mut unbound = WorldEngine::new("gen-1").expect("generation");
    let base = observe(1, facts_pair(), "win-a");
    match unbound.handle_report_v4(
        &base,
        &EventIssue {
            owner: OwnerId::parse("adapter-1").expect("owner"),
            correlation: CorrelationId::parse(&base.request_id).expect("correlation"),
        },
    ) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_OWNER),
        other => panic!("expected unbound owner refusal, got {other:?}"),
    }
}

#[test]
fn slice_7_1_invalid_window_hit_refused_atomically() {
    // A cursor hit naming an identifiable invalid (gradable metadata)
    // window fails closed with decoder-equivalent structural refusal:
    // the gradable row is not a readable reported window for hit
    // membership, so the report refuses before any grading, staging, or
    // mutation. Ordinary invalid metadata without a hit still holds.
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let mut bad = fact("win-bad", "out-a", 0);
    bad.title = Some("x".repeat(5000));
    let mut invalid = observe(2, vec![fact("win-a", "out-a", 0), bad], "win-a");
    invalid.cursor = Some((10, 20));
    invalid.cursor_hit = Some(WindowId("win-bad".to_owned()));
    let revision_before = engine.state_revision;
    let calls_before = engine.ingest_calls;
    match admit(&mut engine, &invalid) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "cursor-hit-unknown");
        }
        other => panic!("expected cursor-hit refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision_before);
    assert_eq!(engine.ingest_calls, calls_before);
    assert_eq!(engine.last_request_seq_v4, 1, "refusal caches nothing");
    assert!(
        engine.staged.held_windows.is_empty(),
        "refusal stages no holds"
    );
}
