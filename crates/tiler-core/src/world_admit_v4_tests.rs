//! Focused internal v4 admission-boundary regressions (unit 6
//! sub-increment 7e): typed replay bookkeeping, bootstrap, and bounded
//! candidate adoption through [`WorldEngine::handle_report_v4`], never
//! through public ingress.

use crate::boundary::CoreCommand;
use crate::connection::{ApplicationIdentity, HandshakeRequest, HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, OwnerId};
use crate::intent_data::{IntentData, encode_intent_data};
use crate::workspace_scope::DisplacedEntry;
use crate::world_admit_v4::{V4Replay, classify_replay_v4};
use crate::world_engine::{
    EventIssue, KIND_CORRELATION, KIND_GENERATION, KIND_OBS_STALE, KIND_OWNER, KIND_REPLY_SIZE,
    KIND_REUSED, KIND_REVISION_EXHAUSTED, KIND_SNAPSHOT, KIND_SUPERSEDED, MSG_REPLY_SIZE,
    MSG_REUSED, RawRouting, WorldEngine,
};
use crate::world_report::{
    AfterShown, AfterShownMember, AfterShownScope, AttemptLaneV4, AuthorityScope, BatchSettlement,
    CommandRejectKind, CommandResult, FocusRequest, FocusRequestCause, HeldAttemptRow,
    HeldWindowRow, IntentDesire, MAX_FOCUS_DEPARTING, MAX_FOCUS_MEMBERS, MAX_FOCUS_SELECTED,
    MAX_FOCUS_VIEWS, PresentationIngressV4, PresentationRowV4, RowDiagnostic, RowDiagnosticKind,
    RowDiagnosticLane, ScreenReport, SendSource, WORLD_CONTRACT_VERSION_V4, WarpRequest,
    WindowFact, WorldCommand, WorldReportV4, validate_after_shown_reason,
    validate_focus_request_reason,
};
use crate::world_report_v4_checks::{graded_effective_v4, report_digest_v4};
use crate::world_run_v4::V4RunOutcome;
use crate::world_v4_support_tests::{fact, screen};

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

fn admit_with_settlements(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
) -> (V4RunOutcome, Vec<BatchSettlement>) {
    engine.handle_report_v4_batched(report, &issue(report), &[])
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
        foreground_known: !focused.is_empty(),
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
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        visibility_attempts: Vec::new(),
        focus_changes: None,
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
        pointer_activity: None,
        warp_spent: None,
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
        .world
        .intent
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
    assert_eq!(engine.state_revision(), 1);
    assert_eq!(engine.world.intent.ingest_calls, 1);
    let revision_before = engine.state_revision();
    let members_before = engine.world.workspace.members.clone();
    match admit(&mut engine, &observe(1, facts_pair(), "win-a")) {
        V4RunOutcome::Duplicate(cached) => assert_eq!(cached, first),
        other => panic!("expected duplicate v4 admission, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        revision_before,
        "duplicates advance no revision"
    );
    assert_eq!(
        engine.world.workspace.members, members_before,
        "duplicates mutate no state"
    );
    assert_eq!(
        engine.world.intent.ingest_calls, 1,
        "duplicates pay no ingest"
    );
    assert!(
        engine.world.command_highwater.is_none(),
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
    assert_eq!(engine.state_revision(), 1, "refusal advances no revision");
    assert_eq!(
        engine.world.intent.ingest_calls, 1,
        "refusal ingests nothing"
    );
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
    assert_eq!(engine.state_revision(), 2, "refusal advances no revision");
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
    assert_eq!(engine.state_revision(), 2, "refusal advances no revision");
    assert_eq!(
        engine.world.intent.ingest_calls, 2,
        "refusal ingests nothing"
    );
}

#[test]
fn fresh_consumed_command_reconciles_without_reexecution() {
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
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
    assert_eq!(
        executed
            .focus_request
            .as_ref()
            .map(|request| request.cmd_seq),
        Some(Some(1)),
        "fresh command executes: {:?}",
        executed.focus_request
    );
    assert_eq!(engine.world.command_highwater, Some(1));
    let rerun = report(
        3,
        facts_pair(),
        "win-b",
        PresentationRowV4::Valid(ingress("win-b")),
        Some(1),
        Some(command),
        Vec::new(),
    );
    match admit_with_settlements(&mut engine, &rerun) {
        (V4RunOutcome::CommandConsumed(reply), settlements) => {
            assert!(reply.command_consumed);
            assert_eq!(reply.based_on_observation_seq, 3);
            assert!(
                reply.focus_request.is_none() || reply.focus_request == executed.focus_request,
                "consumed sequences never re-execute: {:?}",
                reply.focus_request
            );
            assert!(settlements.is_empty());
        }
        (other, _) => panic!("expected consumed v4 admission, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), 3);
    assert_eq!(
        engine.world.command_highwater,
        Some(1),
        "consumed reruns consume no new sequence"
    );
}

#[test]
fn commandless_reports_leave_highwater_alone() {
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
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
        engine.world.command_highwater,
        Some(1),
        "commandless reports never move the watermark"
    );
}

#[test]
fn pre_ready_bootstrap_emits_no_authority() {
    let mut engine = engine();
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
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
    match admit_with_settlements(&mut engine, &incomplete) {
        (V4RunOutcome::Accepted(reply), settlements) => {
            assert!(!reply.ready);
            assert!(!reply.command_consumed);
            assert_eq!(reply.state_revision, 0);
            assert_eq!(reply.presentation.rev, 0);
            assert!(settlements.is_empty());
        }
        (other, _) => panic!("expected pre-ready bootstrap, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), 0, "bootstrap advances no revision");
    assert!(
        engine.world.command_highwater.is_none(),
        "bootstrap consumes no command"
    );
    assert!(
        engine.world.workspace.members.is_empty(),
        "bootstrap sweeps nothing"
    );
    assert!(!engine.has_last_v4(), "bootstrap populates no replay cache");
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
    assert_eq!(engine.state_revision(), 1);
    assert!(engine.world.ready_latched);
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
        engine.world.intent.pending_intent.contains_key(&win),
        "pre-ready intent retention survives bootstrap"
    );
    assert!(
        !engine
            .world
            .intent
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
        engine.world.intent.intent_admitted.contains(&win),
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
        !engine.world.intent.pending_intent.contains_key(&win)
            && !engine.world.intent.intent_admitted.contains(&win)
            && !engine.world.intent.restored_floats.contains(&win)
            && !engine.world.intent.restored_sticky.contains_key(&win),
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
    assert_eq!(engine.world.intent.ingest_calls, 1);
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
                    window: crate::directional::WindowSubject::Window(win.clone()),
                    value: true,
                }),
                Vec::new(),
            ),
        ),
    );
    assert_eq!(engine.world.intent.ingest_calls, 2);
    assert_eq!(
        engine.world.intent.restored_sticky.get(&win),
        Some(&false),
        "tiled origin restores with a false prior"
    );
    let clear_data = encode_intent_data(&IntentData::new(None, None));
    let (cleared_outcome, cleared_settlements) = admit_with_settlements(
        &mut engine,
        &report(
            3,
            vec![null_row],
            "win-a",
            PresentationRowV4::Valid(ingress("win-a")),
            Some(2),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                    "win-a".to_owned(),
                )),
                float_rect: None,
            })),
            vec![("win-a".to_owned(), clear_data)],
        ),
    );
    let _cleared = accepted_reply(3, cleared_outcome);
    assert!(
        matches!(
            cleared_settlements.first().and_then(|s| s.result.as_ref()),
            Some(CommandResult::Float { rect: None, .. })
        ),
        "toggle-float clear mints the tile result: {:?}",
        cleared_settlements
    );
    assert_eq!(
        engine.world.intent.ingest_calls, 3,
        "the late import ingested exactly once before the latch ignored it"
    );
    assert!(
        !engine.world.intent.restored_sticky.contains_key(&win),
        "clear drops sticky provenance"
    );
    assert!(
        !engine.world.intent.pending_intent.contains_key(&win),
        "the admitted lifetime leaves no pending late import"
    );
    assert_eq!(engine.world.command_highwater, Some(2));
}

#[test]
fn structural_refusals_leave_original_unchanged() {
    let mut engine = engine();
    let mut versioned = observe(1, facts_pair(), "win-a");
    // Non-v4 wire version on a v4-typed report: the v4 boundary refuses
    // before any mutation, with a raw non-v4 wire version.
    versioned.contract_version = 3;
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
    assert_eq!(engine.state_revision(), 0, "refusals advance no revision");
    assert_eq!(
        engine.world.intent.ingest_calls, 0,
        "refusals ingest nothing"
    );
    assert!(
        engine.world.workspace.members.is_empty(),
        "refusals mutate nothing"
    );
    assert!(!engine.has_last_v4(), "refusals populate no replay cache");
}

#[test]
fn prebound_reply_refusal_is_atomic_then_same_seq_fits() {
    use crate::world_admit_v4::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
        FutureReserveGateFacts, PreflightGateFacts,
    };
    use crate::world_report::WorldReplyV4;
    use std::cell::{Cell, RefCell};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ObservedPreflight {
        screen_count: usize,
        domain_count: usize,
        staged_window_count: usize,
        intent_desired_count: usize,
        retained_settlement_count: usize,
    }

    struct PreflightRefusalGate {
        refuse_preflight: Cell<bool>,
        preflight_calls: Cell<usize>,
        exact_calls: Cell<usize>,
        observed: RefCell<Option<ObservedPreflight>>,
    }

    impl PreflightRefusalGate {
        fn new(refuse_preflight: bool) -> Self {
            Self {
                refuse_preflight: Cell::new(refuse_preflight),
                preflight_calls: Cell::new(0),
                exact_calls: Cell::new(0),
                observed: RefCell::new(None),
            }
        }
    }

    impl CoreAdmissionGate for PreflightRefusalGate {
        fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            self.preflight_calls
                .set(self.preflight_calls.get().saturating_add(1));
            *self.observed.borrow_mut() = Some(ObservedPreflight {
                screen_count: facts.screen_count,
                domain_count: facts.domain_count,
                staged_window_count: facts.staged_window_count,
                intent_desired_count: facts.intent_desired_count,
                retained_settlement_count: facts.retained_settlement_count,
            });
            if self.refuse_preflight.get() {
                AdmissionGateDecision::Refuse
            } else {
                AdmissionGateDecision::Allow
            }
        }

        fn check_exact_inner(&self, _reply: &WorldReplyV4) -> ExactInnerDecision {
            self.exact_calls
                .set(self.exact_calls.get().saturating_add(1));
            ExactInnerDecision::Allow(ExactInnerCheck::new(0))
        }

        fn check_future_reserve(
            &self,
            _reply: &WorldReplyV4,
            _facts: &FutureReserveGateFacts,
            _exact: ExactInnerCheck,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }
    }

    let mut engine = engine();
    let req = observe(1, facts_pair(), "win-a");
    let gate = PreflightRefusalGate::new(true);
    let res = engine
        .handle_report_v4_batched_with_gate_and_prepare(&req, &issue(&req), &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(res.dispositions.is_empty());
    assert_eq!(
        res.outcome,
        V4RunOutcome::Rejected {
            kind: KIND_REPLY_SIZE,
            message: MSG_REPLY_SIZE,
        }
    );
    assert_eq!(gate.preflight_calls.get(), 1);
    assert_eq!(
        gate.exact_calls.get(),
        0,
        "preflight refusal stops before exact"
    );
    assert_eq!(
        *gate.observed.borrow(),
        Some(ObservedPreflight {
            screen_count: 1,
            domain_count: 0,
            staged_window_count: 0,
            intent_desired_count: 0,
            retained_settlement_count: 0,
        })
    );
    assert_eq!(engine.state_revision(), 0, "refusal advances no revision");
    assert!(
        engine.world.command_highwater.is_none(),
        "refused reports consume nothing"
    );
    assert_eq!(
        engine.world.intent.ingest_calls, 0,
        "refusal pays no ingest"
    );
    assert_eq!(trials(&engine), 0, "preflight refusal pays no trial");
    assert!(!engine.has_last_v4(), "refusals populate no replay flag");
    assert!(engine.last_reply_v4().is_none(), "refusals cache no reply");

    // The same sequence works once preflight allows: refusal cached nothing.
    gate.refuse_preflight.set(false);
    let retry_outcome = engine
        .handle_report_v4_batched_with_gate_and_prepare(&req, &issue(&req), &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {})
        .outcome;
    accepted_reply(1, retry_outcome);
    assert_eq!(engine.state_revision(), 1);
    assert_eq!(engine.world.intent.ingest_calls, 1);
    assert!(engine.has_last_v4());
}

#[test]
fn revision_exhaustion_refuses_before_mutation() {
    let mut engine = engine();
    engine.world.state_revision = u64::MAX;
    match admit(&mut engine, &observe(1, facts_pair(), "win-a")) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REVISION_EXHAUSTED),
        other => panic!("expected revision exhaustion, got {other:?}"),
    }
    assert!(
        engine.world.workspace.members.is_empty(),
        "exhausted runs mutate nothing"
    );
    assert_eq!(
        engine.world.intent.ingest_calls, 0,
        "exhausted runs ingest nothing"
    );
    assert!(
        !engine.has_last_v4(),
        "exhaustion populates no replay cache"
    );
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
            window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
            follow: false,
        }),
        Vec::new(),
    );
    accepted_reply(2, admit(&mut engine, &second));
    engine.world.workspace.creation_counter = u64::MAX;
    let revision_before = engine.state_revision();
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
        engine.world.command_highwater,
        Some(1),
        "envelope consumes no sequence"
    );
    assert_eq!(
        engine.state_revision(),
        revision_before,
        "envelope mutates no revision"
    );
    // The prior cached reply survives: the refused sequence cached nothing.
    match admit(&mut engine, &second) {
        V4RunOutcome::Duplicate(_) => {}
        other => panic!("cache survives exhaustion, got {other:?}"),
    }
    engine.world.workspace.creation_counter = 0;
    accepted_reply(4, admit(&mut engine, &observe(4, single(), "win-a")));
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
            .world
            .staged
            .observation
            .raw_windows
            .iter()
            .any(|row| row == &sticky_fact),
        "capture keeps the raw conflicting row verbatim: {:?}",
        engine.world.staged.observation.raw_windows
    );
    assert_eq!(
        engine.world.staged.observation.cursor_hit,
        Some(WindowId("win-a".to_owned())),
        "capture keeps the raw cursor hit verbatim"
    );
    let preheld_row = HeldWindowRow {
        window: WindowId("win-a".to_owned()),
        ordinal: 0,
        output: Some(OutputId("out-a".to_owned())),
        has_native_workspace: sticky_fact.native_workspace.as_deref().is_some(),
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
        engine.last_report_v4().as_ref(),
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
    engine.set_last_digest_v4(report_digest_v4(&other));
    match admit(&mut engine, &other) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_REUSED, "digest collision reuses: {message}");
        }
        other => panic!("expected reused digest collision, got {other:?}"),
    }
    assert_eq!(
        engine.last_report_v4().as_ref(),
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
    engine.world.staged.presentation_v4 = Some(PresentationRowV4::Valid(dirty_ingress));
    engine.world.presentation.order_v4 = vec![WindowId("win-z".to_owned())];
    engine.world.presentation.live_v4.insert(
        WindowId("win-z".to_owned()),
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 90,
        },
    );
    engine.world.presentation.focused_v4 = Some(WindowId("win-z".to_owned()));
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
    assert_eq!(engine.state_revision(), 0, "bootstrap advances no revision");
}

#[test]
fn handshake_resets_typed_admission_state() {
    // A re-handshake replaces the engine with a fresh construction, so all
    // typed admission state (replay cache, revision, watermarks, latches,
    // publication stamps) resets together.
    let mut engine = engine();
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
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
    assert!(engine.has_last_v4());
    assert_eq!(engine.world.command_highwater, Some(1));
    assert!(
        engine.world.presentation.latest_published_rev_v4 > 0,
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
    assert!(!engine.has_last_v4(), "handshake clears the replay flag");
    assert!(
        engine.last_reply_v4().is_none(),
        "handshake clears the cached reply"
    );
    assert_eq!(engine.state_revision(), 0, "handshake resets the revision");
    assert!(
        engine.world.command_highwater.is_none(),
        "handshake clears the watermark"
    );
    assert!(
        !engine.world.ready_latched,
        "handshake clears the ready latch"
    );
    assert_eq!(
        engine.world.presentation.latest_published_rev_v4, 0,
        "handshake clears the publication stamp"
    );
}

#[test]
fn forged_staged_reply_never_adopts() {
    use crate::world_admit_v4::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
        FutureReserveGateFacts, PreflightGateFacts,
    };
    use crate::world_report::WorldReplyV4;
    use std::cell::Cell;

    struct ExactCheckpointGate {
        refuse_exact: bool,
        fail_if_exact_called: bool,
        exact_calls: Cell<usize>,
        future_calls: Cell<usize>,
    }

    impl ExactCheckpointGate {
        fn refuse() -> Self {
            Self {
                refuse_exact: true,
                fail_if_exact_called: false,
                exact_calls: Cell::new(0),
                future_calls: Cell::new(0),
            }
        }

        fn fail_if_called() -> Self {
            Self {
                refuse_exact: false,
                fail_if_exact_called: true,
                exact_calls: Cell::new(0),
                future_calls: Cell::new(0),
            }
        }
    }

    impl CoreAdmissionGate for ExactCheckpointGate {
        fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            _facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_exact_inner(&self, _reply: &WorldReplyV4) -> ExactInnerDecision {
            assert!(
                !self.fail_if_exact_called,
                "structural/intent refusal must fire before check_exact_inner"
            );
            self.exact_calls
                .set(self.exact_calls.get().saturating_add(1));
            if self.refuse_exact {
                ExactInnerDecision::Refuse
            } else {
                ExactInnerDecision::Allow(ExactInnerCheck::new(0))
            }
        }

        fn check_future_reserve(
            &self,
            _reply: &WorldReplyV4,
            _facts: &FutureReserveGateFacts,
            _exact: ExactInnerCheck,
        ) -> AdmissionGateDecision {
            self.future_calls
                .set(self.future_calls.get().saturating_add(1));
            AdmissionGateDecision::Allow
        }
    }

    // The finalize gate re-checks the staged reply before adoption: an
    // exact-gate refusal or intent-invalid forgery refuses and leaves the
    // engine exactly unchanged.
    let fresh = engine();
    let first = observe(1, facts_pair(), "win-a");
    let mut staged = fresh.clone();
    let genuine = match staged.tx(|tx| tx.run_accepted_v4(&first, &issue(&first), false, 1)) {
        V4RunOutcome::Accepted(reply) | V4RunOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected staged run, got {other:?}"),
    };
    let mut engine = fresh;
    let digest = report_digest_v4(&first);
    let exact_gate = ExactCheckpointGate::refuse();
    let prepare_called = Cell::new(false);
    let outcome = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            staged.world,
            &first,
            digest,
            genuine,
            &[],
            &exact_gate,
            |_| {
                prepare_called.set(true);
                Ok::<(), std::convert::Infallible>(())
            },
        )
        .unwrap_or_else(|e| match e {});
    assert_eq!(
        outcome,
        V4RunOutcome::Rejected {
            kind: KIND_REPLY_SIZE,
            message: MSG_REPLY_SIZE,
        }
    );
    assert_eq!(exact_gate.exact_calls.get(), 1);
    assert_eq!(exact_gate.future_calls.get(), 0);
    assert!(!prepare_called.get());
    assert_eq!(engine.state_revision(), 0, "refusal advances no revision");
    assert!(
        engine.world.command_highwater.is_none(),
        "refusal consumes nothing"
    );
    assert!(!engine.has_last_v4(), "refusal populates no replay cache");
    assert!(engine.last_reply_v4().is_none(), "refusal caches no reply");

    let mut staged = engine.clone();
    let genuine = match staged.tx(|tx| tx.run_accepted_v4(&first, &issue(&first), false, 1)) {
        V4RunOutcome::Accepted(reply) | V4RunOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected staged run, got {other:?}"),
    };
    let mut forged = genuine;
    forged.intent_desired = vec![
        IntentDesire {
            win: WindowId("win-a".to_owned()),
            data: None,
        },
        IntentDesire {
            win: WindowId("win-a".to_owned()),
            data: None,
        },
    ];
    let fail_gate = ExactCheckpointGate::fail_if_called();
    let outcome = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            staged.world,
            &first,
            digest,
            forged,
            &[],
            &fail_gate,
            |_| {
                prepare_called.set(true);
                Ok::<(), std::convert::Infallible>(())
            },
        )
        .unwrap_or_else(|e| match e {});
    match outcome {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT, "intent forgery refuses: {message}");
            assert_eq!(message, "duplicate-intent-desire");
        }
        other => panic!("expected snapshot-invalid refusal, got {other:?}"),
    }
    assert_eq!(fail_gate.exact_calls.get(), 0);
    assert!(!prepare_called.get());
    assert_eq!(engine.state_revision(), 0, "refusal advances no revision");
    assert!(!engine.has_last_v4(), "refusal populates no replay cache");
    assert!(engine.last_reply_v4().is_none(), "refusal caches no reply");
}

fn after_shown_members(count: usize) -> Vec<AfterShownMember> {
    (0..count)
        .map(|i| AfterShownMember {
            win: WindowId(format!("win-{i}")),
            out: OutputId("out-a".to_owned()),
            ws: "ws-a".to_owned(),
            frame: None,
        })
        .collect()
}

fn after_shown_scopes(count: usize) -> Vec<AfterShownScope> {
    (0..count)
        .map(|i| AfterShownScope {
            out: OutputId(format!("out-{i}")),
            ws: format!("ws-{i}"),
        })
        .collect()
}

fn over_cap_shown(
    members: Vec<AfterShownMember>,
    views: Vec<AfterShownScope>,
    selected: Vec<AfterShownScope>,
    departing: Vec<AfterShownScope>,
) -> AfterShown {
    AfterShown {
        op: "op-1".to_owned(),
        obs: 1,
        members,
        views,
        selected,
        departing,
    }
}

fn over_cap_request(shown: AfterShown) -> FocusRequest {
    FocusRequest {
        id: "focus-1".to_owned(),
        target: WindowId("win-a".to_owned()),
        issued_revision: 1,
        cause: FocusRequestCause::ExplicitCommand,
        output: OutputId("out-a".to_owned()),
        workspace: "ws-a".to_owned(),
        cmd_seq: Some(7),
        after_shown: Some(shown),
    }
}

#[test]
fn after_shown_cap_helper_pins_each_lane() {
    // Absent projection admits.
    assert_eq!(validate_after_shown_reason(&None), None);
    assert_eq!(validate_focus_request_reason(&None), None);
    assert_eq!(
        validate_focus_request_reason(&Some(FocusRequest {
            id: "focus-1".to_owned(),
            target: WindowId("win-a".to_owned()),
            issued_revision: 1,
            cause: FocusRequestCause::ExplicitCommand,
            output: OutputId("out-a".to_owned()),
            workspace: "ws-a".to_owned(),
            cmd_seq: Some(7),
            after_shown: None,
        })),
        None,
        "null projection admits"
    );
    // At-cap admits on every lane without reaching the byte budget.
    let at_cap = over_cap_shown(
        after_shown_members(MAX_FOCUS_MEMBERS),
        after_shown_scopes(MAX_FOCUS_VIEWS),
        after_shown_scopes(MAX_FOCUS_SELECTED),
        after_shown_scopes(MAX_FOCUS_DEPARTING),
    );
    assert_eq!(at_cap.members.len(), 10_000);
    assert_eq!(validate_after_shown_reason(&Some(at_cap)), None);
    // Over-cap refuses per lane with the encoder's fixed token.
    let over_members = over_cap_shown(
        after_shown_members(MAX_FOCUS_MEMBERS + 1),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        validate_after_shown_reason(&Some(over_members)),
        Some("focus-members-over-cap")
    );
    for (lane, token) in [
        ("views", "focus-views-over-cap"),
        ("selected", "focus-selected-over-cap"),
        ("departing", "focus-departing-over-cap"),
    ] {
        let shown = match lane {
            "views" => {
                let views = after_shown_scopes(MAX_FOCUS_VIEWS + 1);
                over_cap_shown(Vec::new(), views.clone(), views, Vec::new())
            }
            "selected" => over_cap_shown(
                Vec::new(),
                Vec::new(),
                after_shown_scopes(MAX_FOCUS_SELECTED + 1),
                Vec::new(),
            ),
            _ => over_cap_shown(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                after_shown_scopes(MAX_FOCUS_DEPARTING + 1),
            ),
        };
        assert_eq!(
            validate_after_shown_reason(&Some(shown)),
            Some(token),
            "{lane} over cap refuses"
        );
    }
}

#[test]
fn forged_after_shown_over_cap_never_adopts() {
    use crate::world_admit_v4::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerDecision, FutureReserveGateFacts,
        PreflightGateFacts,
    };
    use crate::world_report::WorldReplyV4;

    struct FailIfExactCalledGate;

    impl CoreAdmissionGate for FailIfExactCalledGate {
        fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            _facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_exact_inner(&self, _reply: &WorldReplyV4) -> ExactInnerDecision {
            panic!("focus structural cap check must refuse before check_exact_inner")
        }

        fn check_future_reserve(
            &self,
            _reply: &WorldReplyV4,
            _facts: &FutureReserveGateFacts,
            _exact: crate::world_admit_v4::ExactInnerCheck,
        ) -> AdmissionGateDecision {
            panic!("focus structural cap check must refuse before check_future_reserve")
        }
    }

    // An over-cap projection must refuse at the finalize gate before
    // check_exact_inner and before adoption: the typed cap check owns this
    // boundary with no engine mutation, cache, or id advance.
    let fresh = engine();
    let first = observe(1, facts_pair(), "win-a");
    let mut staged = fresh.clone();
    let genuine = match staged.tx(|tx| tx.run_accepted_v4(&first, &issue(&first), false, 1)) {
        V4RunOutcome::Accepted(reply) | V4RunOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected staged run, got {other:?}"),
    };
    let digest = report_digest_v4(&first);
    let members_before = fresh.world.workspace.members.clone();
    let mut engine = fresh.clone();
    let views = after_shown_scopes(MAX_FOCUS_VIEWS + 1);
    let cases: Vec<(&str, AfterShown)> = vec![
        (
            "focus-members-over-cap",
            over_cap_shown(
                after_shown_members(MAX_FOCUS_MEMBERS + 1),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ),
        ),
        (
            "focus-views-over-cap",
            over_cap_shown(Vec::new(), views.clone(), views, Vec::new()),
        ),
        (
            "focus-selected-over-cap",
            over_cap_shown(
                Vec::new(),
                Vec::new(),
                after_shown_scopes(MAX_FOCUS_SELECTED + 1),
                Vec::new(),
            ),
        ),
        (
            "focus-departing-over-cap",
            over_cap_shown(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                after_shown_scopes(MAX_FOCUS_DEPARTING + 1),
            ),
        ),
    ];
    for (token, shown) in cases {
        let mut forged = genuine.clone();
        forged.focus_request = Some(over_cap_request(shown));
        let outcome = engine
            .finalize_v4_candidate_with_gate_and_prepare(
                staged.world.clone(),
                &first,
                digest,
                forged,
                &[],
                &FailIfExactCalledGate,
                |_| Ok::<(), std::convert::Infallible>(()),
            )
            .unwrap_or_else(|e| match e {});
        match outcome {
            V4RunOutcome::Rejected { kind, message } => {
                assert_eq!(kind, KIND_SNAPSHOT, "{token} refuses typed");
                assert_eq!(message, token);
            }
            other => panic!("expected {token} refusal, got {other:?}"),
        }
        assert_eq!(engine.state_revision(), 0, "{token} advances no revision");
        assert!(
            engine.world.command_highwater.is_none(),
            "{token} consumes nothing"
        );
        assert!(!engine.has_last_v4(), "{token} populates no replay cache");
        assert!(engine.last_reply_v4().is_none(), "{token} caches no reply");
        assert_eq!(
            engine.world.workspace.members, members_before,
            "{token} mutates no members"
        );
        assert!(
            engine.world.focus.pending_focus.is_none(),
            "{token} poisons no focus slot"
        );
        assert_eq!(
            engine.world.presentation.latest_published_rev_v4, 0,
            "{token} stamps no publication"
        );
    }
}

#[test]
fn policy_terminal_reserve_v4_counts_displaced_unlive_rings() {
    // The v4 reserve reads the input-only live-output set: with live
    // {out-a}, the displaced origin out-gone contributes nothing live
    // while its unlive mapping ring (one workspace) reserves exactly one
    // switch event of settle bound.
    let mut engine = engine();
    engine.world.workspace.displaced.insert(
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
        .world
        .workspace
        .mapping
        .insert("out-gone".to_owned(), vec!["ws-a".to_owned()]);
    let typed = observe(1, facts_pair(), "win-a");
    assert_eq!(typed.screens.len(), 1);
    let typed_count = engine
        .world
        .policy_terminal_count_v4(&typed, engine.connection.settings.workspace_mode);
    assert_eq!(
        typed_count, 1,
        "one unlive ring workspace counts one switch event"
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
    let (sent_outcome, sent_settlements) = admit_with_settlements(
        &mut engine,
        &report(
            3,
            base_windows(),
            "w0",
            PresentationRowV4::Valid(ingress("w0")),
            Some(2),
            Some(WorldCommand::Send {
                window: crate::directional::WindowSubject::Window(WindowId("w0".to_owned())),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
                follow: true,
            }),
            Vec::new(),
        ),
    );
    let _sent = accepted_reply(3, sent_outcome);
    assert_eq!(sent_settlements.len(), 1);
    assert!(
        sent_settlements[0].result.is_none(),
        "occupying send executes, got {:?}",
        sent_settlements
    );
    assert!(
        engine
            .tx_ro(|tx| tx.ring_occupied(&OutputId("out-a".to_owned())))
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
            window: crate::directional::WindowSubject::Window(WindowId("wSticky".to_owned())),
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
    let (reply_outcome, reply_settlements) = admit_with_settlements(&mut engine, &refused);
    let reply = accepted_reply(4, reply_outcome);
    assert_eq!(
        reply_settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::SendRefused
        }),
        "sticky send refuses with its catalog kind"
    );
    assert_eq!(engine.world.command_highwater, Some(3));
    assert_eq!(
        engine.world.workspace.mapping, control.world.workspace.mapping,
        "refusal preserves the commandless mapping"
    );
    assert_eq!(
        engine.world.workspace.creation_counter, control.world.workspace.creation_counter,
        "trailing creation rolls back to the commandless counter"
    );
    assert_eq!(
        engine.world.settlement.retained, control.world.settlement.retained,
        "no transfer minted for the refused mover"
    );
    assert_eq!(
        engine.world.workspace.members, control.world.workspace.members,
        "membership matches the commandless image"
    );
    assert_eq!(
        engine.world.workspace.tiling, control.world.workspace.tiling,
        "created mode rolls back"
    );
    assert_eq!(
        engine.world.staged.domains, control.world.staged.domains,
        "created domain rolls back"
    );
    assert!(
        !engine.world.settlement.retained.iter().any(|transfer| matches!(&transfer.target.kind, crate::world_report::TransferKind::Window(w) if w.0 == "wSticky")),
        "refused sticky leaves no flight: {:?}",
        engine.world.settlement.retained
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
    assert_eq!(engine.world.intent.ingest_calls, 0);
    assert_eq!(trials(&engine), 0, "fresh engine runs no trials");
    // Early refusal pays no clone and no trial.
    let mut versioned = observe(1, facts_pair(), "win-a");
    // Non-v4 wire version on a v4-typed report; see structural_refusals.
    versioned.contract_version = 3;
    match admit(&mut engine, &versioned) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "version-mismatch");
        }
        other => panic!("expected version refusal, got {other:?}"),
    }
    assert_eq!(
        engine.world.intent.ingest_calls, 0,
        "refusals ingest nothing"
    );
    assert_eq!(trials(&engine), 0, "early refusal runs no trial");
    // Successful entries pay exactly one trial each, restaging included.
    accepted_reply(1, admit(&mut engine, &observe(1, facts_pair(), "win-a")));
    assert_eq!(engine.world.intent.ingest_calls, 1);
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
                    window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
                    direction: None,
                }),
                Vec::new(),
            ),
        ),
    );
    assert_eq!(engine.world.intent.ingest_calls, 2);
    assert_eq!(
        trials(&engine),
        2,
        "command restaging ingests exactly once per entry"
    );
    let (sent_outcome, sent_settlements) = admit_with_settlements(
        &mut engine,
        &report(
            3,
            facts_pair(),
            "win-a",
            PresentationRowV4::Valid(ingress("win-a")),
            Some(2),
            Some(WorldCommand::Send {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
                follow: false,
            }),
            Vec::new(),
        ),
    );
    let _sent = accepted_reply(3, sent_outcome);
    assert_eq!(sent_settlements.len(), 1);
    assert!(
        sent_settlements[0].result.is_none(),
        "occupying send executes, got {:?}",
        sent_settlements
    );
    assert_eq!(engine.world.intent.ingest_calls, 3);
    assert_eq!(trials(&engine), 3, "third entry runs its own trial");
    // A late discarded candidate still trials once on its staged clone:
    // creation exhaustion refuses only after the trial ingested, and the
    // drop adopts nothing onto `self`. Snapshot the pre-refusal image:
    // the envelope refusal must leave state, watermarks, and replay cache
    // exactly unchanged apart from the shared trial tick.
    let saved_mapping = engine.world.workspace.mapping.clone();
    let saved_retained = engine.world.settlement.retained.clone();
    let saved_members = engine.world.workspace.members.clone();
    let saved_highwater = engine.world.command_highwater;
    let saved_revision = engine.state_revision();
    let saved_reply = engine.last_reply_v4().clone();
    let saved_request_seq = engine.last_request_seq_v4();
    engine.world.workspace.creation_counter = u64::MAX;
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
        engine.world.intent.ingest_calls, 3,
        "discarded candidate adopts no ingest onto self"
    );
    assert_eq!(
        trials(&engine),
        4,
        "discarded candidate still ran exactly one trial"
    );
    assert_eq!(
        engine.world.workspace.mapping, saved_mapping,
        "envelope refusal adopts no mapping"
    );
    assert_eq!(
        engine.world.settlement.retained, saved_retained,
        "envelope refusal adopts no transfers"
    );
    assert_eq!(
        engine.world.workspace.members, saved_members,
        "envelope refusal adopts no membership"
    );
    assert_eq!(
        engine.world.command_highwater, saved_highwater,
        "envelope refusal consumes no sequence"
    );
    assert_eq!(
        engine.state_revision(),
        saved_revision,
        "envelope refusal advances no revision"
    );
    assert_eq!(
        engine.last_reply_v4(),
        saved_reply,
        "envelope refusal caches no reply"
    );
    assert_eq!(
        engine.last_request_seq_v4(),
        saved_request_seq,
        "envelope refusal moves no replay watermark"
    );
    assert_eq!(engine.world.workspace.creation_counter, u64::MAX);
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
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
                follow: false,
            }),
            Vec::new(),
        ),
    ) {
        V4RunOutcome::Duplicate(_) => {}
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert_eq!(
        engine.world.intent.ingest_calls, 3,
        "duplicates pay no ingest"
    );
    assert_eq!(trials(&engine), 4, "duplicates run no trial");
    assert_eq!(
        engine.world.workspace.mapping, saved_mapping,
        "duplicate adopts no mapping"
    );
    assert_eq!(
        engine.world.command_highwater, saved_highwater,
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
    let revision_before = engine.state_revision();
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
    assert!(
        !engine.world.ready_latched,
        "invalid hold latches no readiness"
    );
    assert_eq!(
        engine.state_revision(),
        revision_before,
        "bootstrap advances no revision"
    );
    assert!(
        engine.world.command_highwater.is_none(),
        "bootstrap consumes no sequence"
    );
    assert!(!engine.has_last_v4(), "bootstrap caches no replay");
    assert!(
        engine.world.staged.screens.is_empty() && engine.world.staged.held_screens.is_empty(),
        "bootstrap stages no screens or holds"
    );
    // A later valid observation readies and takes authority.
    let ready = accepted_reply(2, admit(&mut engine, &observe(2, facts_pair(), "win-a")));
    assert!(ready.ready, "valid observation readies");
    assert!(engine.world.ready_latched);
    assert_eq!(
        engine.state_revision(),
        revision_before + 1,
        "valid observation advances one revision"
    );
    assert_eq!(engine.world.staged.screens.len(), 1);
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
        engine.world.staged.screens.len(),
        0,
        "bad screen stages no screen"
    );
    assert_eq!(engine.world.staged.held_screens.len(), 1, "bad screen held");
    let held = &engine.world.staged.held_screens[0];
    assert_eq!(held.output.0, "out-a");
    assert_eq!(held.ordinal, 0);
    assert_eq!(held.diagnostic.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(held.diagnostic.row, RowDiagnosticLane::Screen);
    assert!(engine.world.staged.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == RowDiagnosticKind::InvalidFact
            && diagnostic.row == RowDiagnosticLane::Screen
    }));
}

#[test]
fn slice_7_1_nonfinite_scale_grades_to_typed_hold() {
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
    assert_eq!(engine.world.staged.held_screens.len(), 1);
    let held = &engine.world.staged.held_screens[0];
    assert_eq!(held.output.0, "out-a");
    assert_eq!(held.ordinal, 0);
    assert_eq!(held.diagnostic.kind, RowDiagnosticKind::InvalidFact);
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
            .world
            .staged
            .facts
            .contains_key(&WindowId("win-bad".to_owned())),
        "bad window stages no fact"
    );
    assert!(
        engine
            .world
            .staged
            .held_windows
            .iter()
            .any(|held| held.window.0 == "win-bad"),
        "bad window held: {:?}",
        engine.world.staged.held_windows
    );
    let held = engine
        .world
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
            .world
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
    assert_eq!(
        capped.world.staged.held_screens.len(),
        1,
        "screen invalid held"
    );
    let kinds: Vec<RowDiagnosticKind> = capped
        .world
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
            .world
            .staged
            .facts
            .contains_key(&WindowId("win-bad".to_owned()))
    );
    assert!(
        !capped
            .world
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
    let revision_before = engine.state_revision();
    let calls_before = engine.world.intent.ingest_calls;
    match admit(&mut engine, &duplicated) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "duplicate-window");
        }
        other => panic!("expected duplicate refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision_before);
    assert_eq!(engine.world.intent.ingest_calls, calls_before);
    assert!(!engine.has_last_v4());

    let mut hidden = observe(1, facts_pair(), "win-a");
    hidden.windows[0].owner = Some(WindowId("bad owner".to_owned()));
    hidden.windows[0].title = Some("z".repeat(5000));
    match admit(&mut engine, &hidden) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_SNAPSHOT),
        other => panic!("expected hidden id refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision_before);
    assert!(!engine.has_last_v4());

    let mut overlap = observe(1, facts_pair(), "win-a");
    overlap.closed = vec![WindowId("win-a".to_owned())];
    match admit(&mut engine, &overlap) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "closed-overlaps-live");
        }
        other => panic!("expected closed overlap refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision_before);
    assert!(!engine.has_last_v4());
}

#[test]
fn slice_7_1_envelope_fencing_refuses_before_mutation() {
    let mut engine = engine();
    let base = observe(1, facts_pair(), "win-a");
    let revision_before = engine.state_revision();
    let calls_before = engine.world.intent.ingest_calls;

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
        engine.state_revision(),
        revision_before,
        "envelope refuses no revision"
    );
    assert_eq!(
        engine.world.intent.ingest_calls, calls_before,
        "envelope refuses no ingest"
    );
    assert!(!engine.has_last_v4(), "envelope refuses no cache");
    assert!(
        engine.world.workspace.members.is_empty(),
        "envelope refuses no mutation"
    );
}

#[test]
fn slice_7_1_held_native_capability_fence() {
    let mut engine = engine();
    assert!(!engine.connection.host_owned_workspace_domains());
    let mut held_native = observe(1, facts_pair(), "win-a");
    held_native.held_native = vec![crate::world_report::HeldNativeRow {
        output: OutputId("out-a".to_owned()),
        ordinal: 0,
        order_len: 0,
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-a".to_owned())),
            row: RowDiagnosticLane::Native,
            id: Some("out-a".to_owned()),
            kind: RowDiagnosticKind::InvalidFact,
        },
    }];
    let revision_before = engine.state_revision();
    match admit(&mut engine, &held_native) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "native-without-capability");
        }
        other => panic!("expected held native fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision_before);
    assert!(!engine.has_last_v4());
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
    preheld.held_windows = engine.world.staged.held_windows.clone();
    match admit(&mut engine, &preheld) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REUSED),
        other => panic!("expected reused raw mismatch, got {other:?}"),
    }
    assert_eq!(engine.last_report_v4().as_ref(), Some(&raw));
}

fn held_member_row(pre_native: bool, post_native: bool) -> HeldAttemptRow {
    HeldAttemptRow {
        lane: AttemptLaneV4::Member,
        ordinal: 0,
        identity: vec!["op-1".to_owned(), "win-a".to_owned()],
        has_native_ref: pre_native || post_native,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }
}

fn held_view_row() -> HeldAttemptRow {
    HeldAttemptRow {
        lane: AttemptLaneV4::View,
        ordinal: 0,
        identity: vec!["op-v".to_owned()],
        has_native_ref: false,
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
        engine.world.staged.held_attempts.len(),
        1,
        "logical held member stages"
    );
    assert_eq!(engine.state_revision(), 2);
    // A held member row retaining a native `pre` reference is still
    // native evidence and refuses without capability.
    let mut native = observe(3, facts_pair(), "win-a");
    native.held_attempts = vec![held_member_row(true, false)];
    let revision_before = engine.state_revision();
    let calls_before = engine.world.intent.ingest_calls;
    match admit(&mut engine, &native) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "native-without-capability");
        }
        other => panic!("expected held native member fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision_before);
    assert_eq!(engine.world.intent.ingest_calls, calls_before);
    assert_eq!(
        engine.last_request_seq_v4(),
        2,
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
    assert_eq!(engine.state_revision(), revision_before);
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
        window: crate::directional::WindowSubject::Window(WindowId("win-new".to_owned())),
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
        engine.tx_ro(|tx| tx.freeze_command_sources(Some(&command), &raw_routing)),
        engine.tx_ro(|tx| tx.freeze_command_sources(Some(&command), &effective_routing)),
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
    let (reply_outcome, reply_settlements) = admit_with_settlements(&mut engine, &invalid);
    let _reply = accepted_reply(2, reply_outcome);
    let (control_outcome, control_settlements) =
        control_engine.handle_report_v4_batched(&control, &issue(&control), &[]);
    let _control_reply = accepted_reply(2, control_outcome);
    assert_eq!(
        reply_settlements, control_settlements,
        "invalid screen freezes as its held equivalent"
    );
    assert_eq!(
        engine.world.command_highwater, control_engine.world.command_highwater,
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
    let calls_after_ready = engine.world.intent.ingest_calls;
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
    assert_eq!(engine.state_revision(), 1);
    assert_eq!(engine.world.intent.ingest_calls, calls_after_ready);
    assert_eq!(
        engine.last_request_seq_v4(),
        1,
        "mixed refusal caches nothing"
    );
    assert!(
        engine.world.staged.held_screens.is_empty() && engine.world.staged.held_windows.is_empty(),
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
    assert_eq!(engine.state_revision(), 1);
    assert_eq!(engine.world.intent.ingest_calls, calls_after_ready);
    assert_eq!(engine.last_request_seq_v4(), 1);
    // Out-of-range held ordinals refuse without adoption.
    let mut ordinal = observe(2, facts_pair(), "win-a");
    ordinal.held_windows = vec![HeldWindowRow {
        window: WindowId("win-held".to_owned()),
        ordinal: 99,
        output: Some(OutputId("out-a".to_owned())),
        has_native_workspace: false,
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
    assert_eq!(engine.state_revision(), 1);
    assert_eq!(engine.last_request_seq_v4(), 1);
}

#[test]
fn slice_7_1_invalid_version_and_unbound_owner_refused() {
    let mut engine = engine();
    let revision_before = engine.state_revision();
    let mut version = observe(1, facts_pair(), "win-a");
    // Non-v4 wire version on a v4-typed report; see structural_refusals.
    version.contract_version = 3;
    match admit(&mut engine, &version) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "version-mismatch");
        }
        other => panic!("expected version refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision_before);
    assert!(!engine.has_last_v4());
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
    let revision_before = engine.state_revision();
    let calls_before = engine.world.intent.ingest_calls;
    match admit(&mut engine, &invalid) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "cursor-hit-unknown");
        }
        other => panic!("expected cursor-hit refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision_before);
    assert_eq!(engine.world.intent.ingest_calls, calls_before);
    assert_eq!(engine.last_request_seq_v4(), 1, "refusal caches nothing");
    assert!(
        engine.world.staged.held_windows.is_empty(),
        "refusal stages no holds"
    );
}

#[test]
fn prepare_callback_refusal_discards_ready_and_bootstrap_candidates() {
    use crate::world_report::IntentRecord;

    // 1. Bootstrap path: prepare refusal leaves pending intent and replay untouched.
    let mut boot = engine();
    let mut pre_ready = observe(1, Vec::new(), "");
    pre_ready.screens.clear();
    pre_ready.inventory_complete = false;
    pre_ready.intent_records.push(IntentRecord {
        win: WindowId("win-boot".to_owned()),
        data: encode_intent_data(&IntentData::new(Some(true), None)),
    });
    let mut boot_prepare_calls = 0usize;
    let boot_err = boot.handle_report_with_prepare(&pre_ready, &[], |candidate| {
        boot_prepare_calls += 1;
        assert!(!candidate.reply.reply().ready);
        assert!(candidate.dispositions.is_empty());
        Err("late-boot-refusal")
    });
    assert!(matches!(boot_err, Err("late-boot-refusal")));
    assert_eq!(boot_prepare_calls, 1);
    assert!(
        boot.world.intent.intent_desired.is_empty(),
        "bootstrap pending intent must not install when prepare fails"
    );
    assert!(!boot.has_last_v4(), "bootstrap must not advance replay");

    // Next bootstrap report succeeds and installs pending intent without advancing replay.
    let boot_ok = boot
        .handle_report_with_prepare(&pre_ready, &[], |_| Ok::<(), &str>(()))
        .expect("bootstrap prepare ok");
    assert!(matches!(boot_ok.0, V4RunOutcome::Accepted(ref r) if !r.ready));
    assert!(
        boot.world
            .intent
            .intent_desired
            .contains_key(&WindowId("win-boot".to_owned()))
    );
    assert!(
        !boot.has_last_v4(),
        "bootstrap still does not advance replay"
    );

    // 2. Ready path: prepare refusal discards staged world and replay.
    let mut ready = engine();
    let r1 = observe(1, facts_pair(), "win-a");
    accepted_reply(1, admit(&mut ready, &r1));
    assert_eq!(ready.state_revision(), 1);
    assert_eq!(ready.last_request_seq_v4(), 1);

    let r2 = observe(2, facts_pair(), "win-b");
    let mut ready_prepare_calls = 0usize;
    let ready_err = ready.handle_report_with_prepare(&r2, &[], |candidate| {
        ready_prepare_calls += 1;
        assert!(candidate.reply.reply().ready);
        assert_eq!(candidate.reply.reply().state_revision, 2);
        Err("late-ready-refusal")
    });
    assert!(matches!(ready_err, Err("late-ready-refusal")));
    assert_eq!(ready_prepare_calls, 1);
    assert_eq!(ready.state_revision(), 1);
    assert_eq!(ready.last_request_seq_v4(), 1);

    // 3. Duplicate path: prepare is never called, returns cached reply.
    let mut dup_prepare_calls = 0usize;
    let dup = ready
        .handle_report_with_prepare(&r1, &[], |_| {
            dup_prepare_calls += 1;
            Ok::<(), &str>(())
        })
        .expect("duplicate");
    assert_eq!(dup_prepare_calls, 0, "duplicate must not invoke prepare");
    assert!(matches!(dup.0, V4RunOutcome::Duplicate(_)));
    assert_eq!(ready.state_revision(), 1);
}

#[test]
fn admission_gate_checkpoint_order_and_refusal_mapping() {
    use crate::world_admit_v4::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
        FutureReserveGateFacts, PreflightGateFacts,
    };
    use crate::world_engine::{KIND_OVERSIZED, KIND_REPLY_SIZE, MSG_OVERSIZED, MSG_REPLY_SIZE};
    use crate::world_report::WorldReplyV4;
    use std::cell::RefCell;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum RefuseAt {
        None,
        RequestSize,
        Preflight,
        ExactInner,
        FutureReserve,
    }

    struct RecordingGate {
        refuse_at: RefuseAt,
        log: RefCell<Vec<&'static str>>,
        preflight_screens: RefCell<Option<usize>>,
    }

    impl RecordingGate {
        fn new(refuse_at: RefuseAt) -> Self {
            Self {
                refuse_at,
                log: RefCell::new(Vec::new()),
                preflight_screens: RefCell::new(None),
            }
        }
    }

    impl CoreAdmissionGate for RecordingGate {
        fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
            self.log.borrow_mut().push("request_size");
            if self.refuse_at == RefuseAt::RequestSize {
                AdmissionGateDecision::Refuse
            } else {
                AdmissionGateDecision::Allow
            }
        }

        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            self.log.borrow_mut().push("preflight");
            *self.preflight_screens.borrow_mut() = Some(facts.screen_count);
            if self.refuse_at == RefuseAt::Preflight {
                AdmissionGateDecision::Refuse
            } else {
                AdmissionGateDecision::Allow
            }
        }

        fn check_exact_inner(&self, _reply: &WorldReplyV4) -> ExactInnerDecision {
            self.log.borrow_mut().push("exact_inner");
            if self.refuse_at == RefuseAt::ExactInner {
                ExactInnerDecision::Refuse
            } else {
                ExactInnerDecision::Allow(ExactInnerCheck::new(0))
            }
        }

        fn check_future_reserve(
            &self,
            _reply: &WorldReplyV4,
            _facts: &FutureReserveGateFacts,
            _exact: ExactInnerCheck,
        ) -> AdmissionGateDecision {
            self.log.borrow_mut().push("future_reserve");
            if self.refuse_at == RefuseAt::FutureReserve {
                AdmissionGateDecision::Refuse
            } else {
                AdmissionGateDecision::Allow
            }
        }
    }

    // 1. Bootstrap path invokes request_size -> preflight -> exact_inner -> prepare.
    let mut boot = engine();
    let mut pre_ready = observe(1, Vec::new(), "");
    pre_ready.screens.clear();
    pre_ready.inventory_complete = false;
    let boot_gate = RecordingGate::new(RefuseAt::None);
    let boot_outcome = boot
        .handle_report_with_gate_and_prepare(&pre_ready, &[], &boot_gate, |_| {
            boot_gate.log.borrow_mut().push("prepare");
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {})
        .outcome;
    assert!(matches!(boot_outcome, V4RunOutcome::Accepted(ref r) if !r.ready));
    assert_eq!(
        *boot_gate.log.borrow(),
        vec!["request_size", "preflight", "exact_inner", "prepare"]
    );
    assert_eq!(*boot_gate.preflight_screens.borrow(), Some(0));

    // 2. Ready path invokes request_size -> preflight -> exact_inner -> future_reserve -> prepare.
    let mut ready = engine();
    let r1 = observe(1, facts_pair(), "win-a");
    let ready_gate = RecordingGate::new(RefuseAt::None);
    let ready_outcome = ready
        .handle_report_with_gate_and_prepare(&r1, &[], &ready_gate, |_| {
            ready_gate.log.borrow_mut().push("prepare");
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {})
        .outcome;
    assert!(matches!(ready_outcome, V4RunOutcome::Accepted(ref r) if r.ready));
    assert_eq!(
        *ready_gate.log.borrow(),
        vec![
            "request_size",
            "preflight",
            "exact_inner",
            "future_reserve",
            "prepare",
        ]
    );
    assert_eq!(*ready_gate.preflight_screens.borrow(), Some(1));
    assert_eq!(ready.state_revision(), 1);
    assert_eq!(ready.last_request_seq_v4(), 1);

    // 3. Refusal at each hook stops subsequent hooks and maps to the static core refusal.
    let r2 = observe(2, facts_pair(), "win-b");
    let cases = [
        (
            RefuseAt::RequestSize,
            KIND_OVERSIZED,
            MSG_OVERSIZED,
            vec!["request_size"],
        ),
        (
            RefuseAt::Preflight,
            KIND_REPLY_SIZE,
            MSG_REPLY_SIZE,
            vec!["request_size", "preflight"],
        ),
        (
            RefuseAt::ExactInner,
            KIND_REPLY_SIZE,
            MSG_REPLY_SIZE,
            vec!["request_size", "preflight", "exact_inner"],
        ),
        (
            RefuseAt::FutureReserve,
            KIND_REPLY_SIZE,
            MSG_REPLY_SIZE,
            vec!["request_size", "preflight", "exact_inner", "future_reserve"],
        ),
    ];
    for (refuse_at, expected_kind, expected_msg, expected_log) in cases {
        let gate = RecordingGate::new(refuse_at);
        let res = ready
            .handle_report_with_gate_and_prepare(&r2, &[], &gate, |_| {
                gate.log.borrow_mut().push("prepare");
                Ok::<(), std::convert::Infallible>(())
            })
            .unwrap_or_else(|e| match e {});
        assert!(res.dispositions.is_empty());
        assert_eq!(
            res.outcome,
            V4RunOutcome::Rejected {
                kind: expected_kind,
                message: expected_msg,
            }
        );
        assert_eq!(*gate.log.borrow(), expected_log);
        assert_eq!(ready.state_revision(), 1);
        assert_eq!(ready.last_request_seq_v4(), 1);
    }
}

#[test]
fn slice_7_1_held_window_typed_native_workspace_refuses_without_capability() {
    let mut engine = engine();
    assert!(!engine.connection.host_owned_workspace_domains());
    let mut report = observe(1, facts_pair(), "win-a");
    report.held_windows = vec![HeldWindowRow {
        window: WindowId("win-held".to_owned()),
        ordinal: 0,
        output: Some(OutputId("out-a".to_owned())),
        has_native_workspace: true,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let revision_before = engine.state_revision();
    match admit(&mut engine, &report) {
        V4RunOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "native-without-capability");
        }
        other => panic!("expected held window native fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision_before);
    assert!(!engine.has_last_v4());
}

#[test]
fn residual_gate_discriminates_duplicate_reused_and_prepare_refusal() {
    use crate::world_admit_v4::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
        FutureReserveGateFacts, PreflightGateFacts, ReplayResidualToken, RetainedResidual,
    };
    use std::sync::Arc;

    #[derive(Debug, PartialEq, Eq)]
    struct TagResidual(&'static str);

    impl ReplayResidualToken for TagResidual {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn residual_eq(&self, other: &dyn ReplayResidualToken) -> bool {
            other
                .as_any()
                .downcast_ref::<TagResidual>()
                .is_some_and(|rhs| self == rhs)
        }
    }

    struct TagGate(&'static str);

    impl CoreAdmissionGate for TagGate {
        fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }
        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            _facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }
        fn check_exact_inner(
            &self,
            _reply: &crate::world_report::WorldReplyV4,
        ) -> ExactInnerDecision {
            ExactInnerDecision::Allow(ExactInnerCheck::new(0))
        }
        fn check_future_reserve(
            &self,
            _reply: &crate::world_report::WorldReplyV4,
            _facts: &FutureReserveGateFacts,
            _exact: ExactInnerCheck,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }
        fn residual_matches(&self, retained: Option<&RetainedResidual>) -> bool {
            retained.is_some_and(|r| {
                r.0.as_any()
                    .downcast_ref::<TagResidual>()
                    .is_some_and(|tag| tag.0 == self.0)
            })
        }
        fn capture_residual(&self) -> Option<RetainedResidual> {
            Some(RetainedResidual(Arc::new(TagResidual(self.0))))
        }
    }

    let mut engine = engine();
    let r1 = observe(1, facts_pair(), "win-a");

    // Prepare refusal must not retain residual or populate replay.
    let err = engine
        .handle_report_with_gate_and_prepare(&r1, &[], &TagGate("alpha"), |_| Err("prepare-fail"))
        .expect_err("prepare refusal propagates");
    assert_eq!(err, "prepare-fail");
    assert!(!engine.has_last_v4());

    // Successful install retains residual "alpha".
    let accepted = engine
        .handle_report_with_gate_and_prepare(&r1, &[], &TagGate("alpha"), |_| Ok::<(), ()>(()))
        .expect("ok")
        .outcome;
    assert!(matches!(accepted, V4RunOutcome::Accepted(_)));

    // Matching residual -> Duplicate.
    let dup = engine
        .handle_report_with_gate_and_prepare(&r1, &[], &TagGate("alpha"), |_| Ok::<(), ()>(()))
        .expect("ok")
        .outcome;
    assert!(matches!(dup, V4RunOutcome::Duplicate(_)));

    // Changed residual with identical typed report -> RequestReused.
    let reused = engine
        .handle_report_with_gate_and_prepare(&r1, &[], &TagGate("beta"), |_| Ok::<(), ()>(()))
        .expect("ok")
        .outcome;
    assert_eq!(
        reused,
        V4RunOutcome::Rejected {
            kind: KIND_REUSED,
            message: MSG_REUSED,
        }
    );
}

#[test]
fn unit_p_intrinsic_reply_validation_enforces_semantics_and_host_capabilities() {
    use crate::connection::HostCapabilities;
    use crate::geometry::Rect;
    use crate::world_admit_v4::StructuralCoreAdmissionGate;
    use crate::world_reply_validation::validate_reply_v4;
    use crate::world_report::{StateProperty, StateTarget, VisibilityTarget};

    let mut template_engine = self::engine();
    let r1 = observe(1, facts_pair(), "win-a");
    let valid_reply = accepted_reply(1, admit(&mut template_engine, &r1));
    assert!(!valid_reply.windows.is_empty());

    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            above: false,
            warp: false,
            native_sticky: false,
            ..HostCapabilities::default()
        },
        Vec::new(),
    )
    .expect("bound engine");
    accepted_reply(1, admit(&mut engine, &r1));
    assert!(validate_reply_v4(&valid_reply, engine.capabilities()).is_ok());

    // 1. Small invalid rect (w=0) and fixed semantic token (visibility-show-fallback)
    //    must fail before prepare and leave state/replay unchanged, even with a no-op prepare hook.
    let rev_before = engine.state_revision();
    let mut bad_rect_reply = valid_reply.clone();
    bad_rect_reply.windows[0].allocation = Some(Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 10,
    });
    let mut prepare_called = false;
    let r2 = observe(2, facts_pair(), "win-a");
    let outcome = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            engine.world.clone(),
            &r2,
            report_digest_v4(&r2),
            bad_rect_reply,
            &[],
            &StructuralCoreAdmissionGate,
            |_| {
                prepare_called = true;
                Ok::<(), ()>(())
            },
        )
        .expect("ok");
    assert!(!prepare_called, "invalid rect must refuse before prepare");
    assert_eq!(
        outcome,
        V4RunOutcome::Rejected {
            kind: "unknown-value",
            message: "request contains an unknown value",
        }
    );
    assert_eq!(engine.state_revision(), rev_before);
    assert!(matches!(
        admit(&mut engine, &r1),
        V4RunOutcome::Duplicate(_)
    ));

    let mut show_fallback_reply = valid_reply.clone();
    show_fallback_reply.windows[0].visibility = Some(VisibilityTarget {
        shown: true,
        revision: 1,
        fallback: true,
    });
    let outcome = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            engine.world.clone(),
            &r2,
            report_digest_v4(&r2),
            show_fallback_reply,
            &[],
            &StructuralCoreAdmissionGate,
            |_| Ok::<(), ()>(()),
        )
        .expect("ok");
    assert_eq!(
        outcome,
        V4RunOutcome::Rejected {
            kind: KIND_SNAPSHOT,
            message: "visibility-show-fallback",
        }
    );

    // 2. Host capability gates: above, warp, sticky.
    let mut above_reply = valid_reply.clone();
    above_reply.windows[0].state.push(StateTarget {
        property: StateProperty::Above,
        value: true,
        authority: crate::world_report::ScopeAuthority::Controlled,
        command_seq: None,
        automatic_clear: false,
    });
    assert_eq!(
        validate_reply_v4(&above_reply, engine.capabilities())
            .expect_err("above capability false")
            .message,
        "above-capability-false"
    );
    let outcome = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            engine.world.clone(),
            &r2,
            report_digest_v4(&r2),
            above_reply,
            &[],
            &StructuralCoreAdmissionGate,
            |_| Ok::<(), ()>(()),
        )
        .expect("ok");
    assert_eq!(
        outcome,
        V4RunOutcome::Rejected {
            kind: KIND_SNAPSHOT,
            message: "above-capability-false",
        }
    );

    let mut warp_reply = valid_reply.clone();
    warp_reply.warp_request = Some(WarpRequest {
        id: "warp-0".to_owned(),
        target: WindowId("win-b".to_owned()),
        revision: 1,
        output: OutputId("out-b".to_owned()),
        workspace: WorkspaceId("ws-2".to_owned()),
        point: (10, 20),
        pointer_baseline: Some(0),
    });
    assert_eq!(
        validate_reply_v4(&warp_reply, engine.capabilities())
            .expect_err("warp capability false")
            .message,
        "warp-capability-false"
    );

    let mut sticky_reply = valid_reply.clone();
    sticky_reply.windows[0].state.push(StateTarget {
        property: StateProperty::Sticky,
        value: true,
        authority: crate::world_report::ScopeAuthority::Controlled,
        command_seq: None,
        automatic_clear: false,
    });
    assert_eq!(
        validate_reply_v4(&sticky_reply, engine.capabilities())
            .expect_err("sticky capability false")
            .message,
        "sticky-capability-false"
    );

    // 3. Bootstrap reply validates cleanly.
    let mut pre_ready = self::engine();
    let mut bootstrap = observe(1, Vec::new(), "");
    bootstrap.screens.clear();
    let boot_reply = accepted_reply(1, admit(&mut pre_ready, &bootstrap));
    assert!(!boot_reply.ready);
    assert!(validate_reply_v4(&boot_reply, pre_ready.capabilities()).is_ok());
}

#[test]
fn warp_request_validation_bounds_and_capability() {
    use crate::world_reply_validation::validate_reply_v4;

    let mut template = self::engine();
    let r1 = observe(1, facts_pair(), "win-a");
    let valid_reply = accepted_reply(1, admit(&mut template, &r1));

    let request = || WarpRequest {
        id: "warp-0".to_owned(),
        target: WindowId("win-b".to_owned()),
        revision: 1,
        output: OutputId("out-b".to_owned()),
        workspace: WorkspaceId("ws-2".to_owned()),
        point: (10, 20),
        pointer_baseline: Some(0),
    };
    let with_request = |warp: WarpRequest| {
        let mut reply = valid_reply.clone();
        reply.warp_request = Some(warp);
        reply
    };
    assert!(validate_reply_v4(&with_request(request()), None).is_ok());

    let overlong_id = "w".repeat(crate::bounds::MAX_OPAQUE_ID_LEN + 1);
    let mut cases: Vec<(&str, WarpRequest)> = vec![
        (
            "point",
            WarpRequest {
                point: (32769, 0),
                ..request()
            },
        ),
        (
            "empty-id",
            WarpRequest {
                id: String::new(),
                ..request()
            },
        ),
        (
            "overlong-id",
            WarpRequest {
                id: overlong_id,
                ..request()
            },
        ),
        (
            "target",
            WarpRequest {
                target: WindowId(String::new()),
                ..request()
            },
        ),
        (
            "output",
            WarpRequest {
                output: OutputId("bad output".to_owned()),
                ..request()
            },
        ),
        (
            "workspace",
            WarpRequest {
                workspace: WorkspaceId(String::new()),
                ..request()
            },
        ),
    ];
    for (label, warp) in cases.drain(..) {
        let error = validate_reply_v4(&with_request(warp), None)
            .expect_err("malformed warp request must refuse");
        assert_eq!(error.kind, "unknown-value", "case {label}");
    }

    // `warp:false` rejects even a well-formed request; `warp:true` accepts.
    let host_off = HostCapabilities {
        warp: false,
        ..HostCapabilities::default()
    };
    let error = validate_reply_v4(&with_request(request()), Some(&host_off))
        .expect_err("capability false refuses");
    assert_eq!(error.kind, KIND_SNAPSHOT);
    assert_eq!(error.message, "warp-capability-false");
    let host_on = HostCapabilities {
        warp: true,
        ..HostCapabilities::default()
    };
    assert!(validate_reply_v4(&with_request(request()), Some(&host_on)).is_ok());
}

#[test]
fn unit_p_c5_size_and_wake_fences_beat_invalid_candidate() {
    use crate::world_admit_v4::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
        FutureReserveGateFacts, PreflightGateFacts,
    };
    use crate::world_report::WorldReplyV4;

    struct SizeAndWakeGate {
        refuse_exact: bool,
        refuse_reserve: bool,
        refuse_wake: bool,
    }

    impl CoreAdmissionGate for SizeAndWakeGate {
        fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }
        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            _facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }
        fn check_exact_inner(&self, _reply: &WorldReplyV4) -> ExactInnerDecision {
            if self.refuse_exact {
                ExactInnerDecision::Refuse
            } else {
                ExactInnerDecision::Allow(ExactInnerCheck::new(0))
            }
        }
        fn check_future_reserve(
            &self,
            _reply: &WorldReplyV4,
            _facts: &FutureReserveGateFacts,
            _exact: ExactInnerCheck,
        ) -> AdmissionGateDecision {
            if self.refuse_reserve {
                AdmissionGateDecision::Refuse
            } else {
                AdmissionGateDecision::Allow
            }
        }
        fn check_wake_precheck(
            &self,
            _candidate: &crate::world_engine::WakePrecheckCandidate<'_>,
        ) -> AdmissionGateDecision {
            if self.refuse_wake {
                AdmissionGateDecision::Reject {
                    kind: "sequence-exhausted",
                    message: "wake epoch exhausted",
                }
            } else {
                AdmissionGateDecision::Allow
            }
        }
    }

    let mut engine = self::engine();
    let r1 = observe(1, facts_pair(), "win-a");
    let valid_reply = accepted_reply(1, admit(&mut engine, &r1));
    let mut invalid_reply = valid_reply;
    invalid_reply.warp_request = Some(WarpRequest {
        id: "warp-0".to_owned(),
        target: WindowId("win-b".to_owned()),
        revision: 1,
        output: OutputId("out-b".to_owned()),
        workspace: WorkspaceId("ws-2".to_owned()),
        point: (32769, 0),
        pointer_baseline: Some(0),
    });

    let r2 = observe(2, facts_pair(), "win-a");
    let digest = report_digest_v4(&r2);

    // 1. Exact inner oversize beats invalid candidate.
    let outcome = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            engine.world.clone(),
            &r2,
            digest,
            invalid_reply.clone(),
            &[],
            &SizeAndWakeGate {
                refuse_exact: true,
                refuse_reserve: true,
                refuse_wake: true,
            },
            |_| Ok::<(), ()>(()),
        )
        .expect("ok");
    assert_eq!(
        outcome,
        V4RunOutcome::Rejected {
            kind: KIND_REPLY_SIZE,
            message: MSG_REPLY_SIZE,
        }
    );

    // 2. Future reserve beats wake precheck and invalid candidate.
    let outcome = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            engine.world.clone(),
            &r2,
            digest,
            invalid_reply.clone(),
            &[],
            &SizeAndWakeGate {
                refuse_exact: false,
                refuse_reserve: true,
                refuse_wake: true,
            },
            |_| Ok::<(), ()>(()),
        )
        .expect("ok");
    assert_eq!(
        outcome,
        V4RunOutcome::Rejected {
            kind: KIND_REPLY_SIZE,
            message: MSG_REPLY_SIZE,
        }
    );

    // 3. Wake precheck beats invalid candidate.
    let outcome = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            engine.world.clone(),
            &r2,
            digest,
            invalid_reply,
            &[],
            &SizeAndWakeGate {
                refuse_exact: false,
                refuse_reserve: false,
                refuse_wake: true,
            },
            |_| Ok::<(), ()>(()),
        )
        .expect("ok");
    assert_eq!(
        outcome,
        V4RunOutcome::Rejected {
            kind: "sequence-exhausted",
            message: "wake epoch exhausted",
        }
    );
}

#[test]
fn unit_p_trailing_workspace_minting_skips_global_collisions_and_rolls_back() {
    use crate::directional::OutputId;
    use crate::world_admit_v4::StructuralCoreAdmissionGate;
    use crate::world_transaction::WorldTransaction;

    let single = || vec![fact("win-a", "out-a", 0)];
    let mut engine = self::engine();
    let r1 = observe(1, single(), "win-a");
    let reply1 = accepted_reply(1, admit(&mut engine, &r1));
    assert_eq!(
        reply1
            .workspaces
            .iter()
            .map(|w| (w.output.0.as_str(), w.id.as_str(), w.trailing))
            .collect::<Vec<_>>(),
        vec![("out-a", "ws-1", false), ("out-a", "ws-2", true)]
    );

    // Explicit Send to ws-2 (which already exists as trailing) leaves ws-1 empty
    // and mints ws-3 cleanly without duplicating ws-2.
    let r2 = report(
        2,
        single(),
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        Some(1),
        Some(WorldCommand::Send {
            window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
            follow: false,
        }),
        Vec::new(),
    );
    let reply2 = accepted_reply(2, admit(&mut engine, &r2));
    assert_eq!(
        reply2
            .workspaces
            .iter()
            .map(|w| (w.output.0.as_str(), w.id.as_str(), w.trailing))
            .collect::<Vec<_>>(),
        vec![("out-a", "ws-1", false), ("out-a", "ws-2", false)]
    );

    let r3_select = report(
        3,
        single(),
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        Some(2),
        Some(WorldCommand::SelectTrailing {
            output: Some(OutputId("out-a".to_owned())),
        }),
        Vec::new(),
    );
    let reply3 = accepted_reply(3, admit(&mut engine, &r3_select));
    assert_eq!(
        reply3
            .workspaces
            .iter()
            .map(|w| (w.output.0.as_str(), w.id.as_str(), w.trailing))
            .collect::<Vec<_>>(),
        vec![
            ("out-a", "ws-1", false),
            ("out-a", "ws-2", false),
            ("out-a", "ws-3", true),
        ]
    );

    // Cross-output collision: seed ws-4 on out-b, then mint on out-a -> must skip ws-4 and mint ws-5.
    engine
        .world
        .workspace
        .mapping
        .entry("out-b".to_owned())
        .or_default()
        .push("ws-4".to_owned());
    let counter_before = engine.world.workspace.creation_counter;
    let ring_before = engine.world.workspace.mapping.clone();

    // Refused candidate rollback: a failed prepare must leave creation_counter and mapping unchanged.
    let mut staged = engine.world.clone();
    let minted = WorldTransaction::new(&engine.connection, &[], &mut staged)
        .create_trailing_workspace(&OutputId("out-a".to_owned()), true)
        .expect("mint");
    assert_eq!(minted, "ws-5");
    let r3 = observe(4, single(), "win-a");
    let err = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            staged,
            &r3,
            report_digest_v4(&r3),
            reply2,
            &[],
            &StructuralCoreAdmissionGate,
            |_| Err("prepare-fail"),
        )
        .expect_err("prepare refusal propagates");
    assert_eq!(err, "prepare-fail");
    assert_eq!(engine.world.workspace.creation_counter, counter_before);
    assert_eq!(engine.world.workspace.mapping, ring_before);

    // Adopted mint advances creation_counter past ws-4 to 5 and appends ws-5.
    let mut staged = engine.world.clone();
    let minted = WorldTransaction::new(&engine.connection, &[], &mut staged)
        .create_trailing_workspace(&OutputId("out-a".to_owned()), true)
        .expect("mint");
    assert_eq!(minted, "ws-5");
    assert_eq!(staged.workspace.creation_counter, 5);

    // In-use-only retained switch witness: ws-6 is absent from all rings but
    // retained in selection_targets, so minting must skip ws-6 and mint ws-7.
    staged.settlement.selection_targets.insert(
        WindowId("win-switch".to_owned()),
        crate::world_selection::SelectionTarget {
            home: crate::session::DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId("ws-6".to_owned()),
            },
            shown: true,
            rev: 1,
        },
    );
    let minted_after_switch = WorldTransaction::new(&engine.connection, &[], &mut staged)
        .create_trailing_workspace(&OutputId("out-a".to_owned()), true)
        .expect("mint after switch witness");
    assert_eq!(minted_after_switch, "ws-7");
    assert_eq!(staged.workspace.creation_counter, 7);
}

#[test]
fn f4_pre_inspection_not_admitted_distinguishes_preflight_invalid_batch_and_replay() {
    use crate::directional::WindowSubject;
    use crate::world_admit_v4::StructuralCoreAdmissionGate;
    use crate::world_engine::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
        FutureReserveGateFacts, MSG_CORRELATION, MSG_SUPERSEDED, PreflightGateFacts,
    };
    use crate::world_report::{
        BatchDisposition, CommandDisposition, NativeWorkspaceFact, ReportIncorporation,
        StateProperty, WorldReplyV4,
    };

    struct RefusePreflightGate;
    impl CoreAdmissionGate for RefusePreflightGate {
        fn check_request_size(&self, report: &WorldReportV4) -> AdmissionGateDecision {
            StructuralCoreAdmissionGate.check_request_size(report)
        }

        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            _facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Refuse
        }

        fn check_exact_inner(&self, reply: &WorldReplyV4) -> ExactInnerDecision {
            StructuralCoreAdmissionGate.check_exact_inner(reply)
        }

        fn check_future_reserve(
            &self,
            reply: &WorldReplyV4,
            facts: &FutureReserveGateFacts,
            exact: ExactInnerCheck,
        ) -> AdmissionGateDecision {
            StructuralCoreAdmissionGate.check_future_reserve(reply, facts, exact)
        }
    }

    let mut engine = engine();
    let r1 = report_full(
        2,
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
    let seed = engine
        .handle_report_with_gate_and_prepare(&r1, &[], &StructuralCoreAdmissionGate, |_| {
            Ok::<(), ()>(())
        })
        .expect("seed");
    assert_eq!(seed.incorporation, ReportIncorporation::Incorporated);
    let rev_after_seed = engine.state_revision();
    assert_eq!(engine.world.command_highwater, None);

    let queued = [
        (
            1,
            WorldCommand::Focus {
                window: WindowSubject::Window(WindowId("win-b".to_owned())),
                direction: None,
            },
        ),
        (
            2,
            WorldCommand::ToggleMaximize {
                window: WindowSubject::CurrentFocus,
            },
        ),
    ];
    let disordered = [queued[1].clone(), queued[0].clone()];

    let mut prepare_calls = 0usize;
    let assert_not_admitted = |engine: &mut WorldEngine,
                               report: &WorldReportV4,
                               batch: &[(u64, WorldCommand)],
                               kind: &'static str,
                               message: &'static str,
                               prepare_calls: &mut usize| {
        let before_prepare = *prepare_calls;
        let res = engine
            .handle_report_with_gate_and_prepare(
                report,
                batch,
                &StructuralCoreAdmissionGate,
                |_| {
                    *prepare_calls += 1;
                    Ok::<(), ()>(())
                },
            )
            .expect("not-admitted refusal");
        assert_eq!(res.incorporation, ReportIncorporation::NotAdmitted);
        assert_eq!(res.outcome, V4RunOutcome::Rejected { kind, message });
        assert!(res.dispositions.is_empty());
        assert_eq!(*prepare_calls, before_prepare);
        assert_eq!(engine.state_revision(), rev_after_seed);
        assert_eq!(engine.world.command_highwater, None);
    };

    // 1. Public wrapper correlation fence refuses before queue inspection.
    let mut bad_corr = observe(2, facts_pair(), "win-a");
    bad_corr.request_seq = 3;
    bad_corr.request_id.clear();
    assert_not_admitted(
        &mut engine,
        &bad_corr,
        &queued,
        KIND_CORRELATION,
        MSG_CORRELATION,
        &mut prepare_calls,
    );

    // 2. Actual stale request (request_seq 1 < 2) refuses before queue inspection.
    let mut stale_req = observe(2, facts_pair(), "win-a");
    stale_req.request_seq = 1;
    assert_not_admitted(
        &mut engine,
        &stale_req,
        &queued,
        KIND_SUPERSEDED,
        MSG_SUPERSEDED,
        &mut prepare_calls,
    );

    // 3. Reused request (request_seq 2 with changed report) refuses before queue inspection.
    let reused_req = observe(2, facts_pair(), "win-a");
    assert_not_admitted(
        &mut engine,
        &reused_req,
        &queued,
        KIND_REUSED,
        MSG_REUSED,
        &mut prepare_calls,
    );

    // 4. Capability fence refuses before queue inspection even with a disordered queue.
    let mut native_fact = observe(2, facts_pair(), "win-a");
    native_fact.request_seq = 3;
    native_fact.windows[0].native_workspace = NativeWorkspaceFact::Value("ws-1".to_owned());
    assert_not_admitted(
        &mut engine,
        &native_fact,
        &disordered,
        KIND_SNAPSHOT,
        "native-without-capability",
        &mut prepare_calls,
    );

    // 5. Next good capture executes the same queue once in FIFO order.
    let mut r2 = observe(2, facts_pair(), "win-a");
    r2.request_seq = 3;
    let res_good = engine
        .handle_report_with_gate_and_prepare(&r2, &queued, &StructuralCoreAdmissionGate, |_| {
            prepare_calls += 1;
            Ok::<(), ()>(())
        })
        .expect("good capture");
    assert_eq!(res_good.incorporation, ReportIncorporation::Incorporated);
    assert_eq!(prepare_calls, 1);
    assert_eq!(engine.world.command_highwater, Some(2));
    assert_eq!(res_good.dispositions.len(), 2);
    assert!(matches!(
        res_good.dispositions[0],
        BatchDisposition {
            seq: 1,
            disposition: CommandDisposition::Executed(_),
        }
    ));
    assert!(matches!(
        res_good.dispositions[1],
        BatchDisposition {
            seq: 2,
            disposition: CommandDisposition::Executed(_),
        }
    ));
    let reply = accepted_reply(2, res_good.outcome);
    assert_eq!(
        reply.focus_request.as_ref().map(|f| &f.target),
        Some(&WindowId("win-b".to_owned()))
    );
    let win_b = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-b")
        .expect("win-b");
    let win_a = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("win-a");
    assert!(
        win_b
            .state
            .iter()
            .any(|s| s.property == StateProperty::Maximized && s.value)
    );
    assert!(
        !win_a
            .state
            .iter()
            .any(|s| s.property == StateProperty::Maximized && s.value)
    );

    // 6. Exact replay continues the cached-publication contract with NotRun.
    let res_replay = engine
        .handle_report_with_gate_and_prepare(&r2, &queued, &StructuralCoreAdmissionGate, |_| {
            prepare_calls += 1;
            Ok::<(), ()>(())
        })
        .expect("exact replay");
    assert_eq!(res_replay.incorporation, ReportIncorporation::Replay);
    assert!(matches!(res_replay.outcome, V4RunOutcome::Duplicate(_)));
    assert_eq!(prepare_calls, 1);
    assert_eq!(
        res_replay.dispositions,
        vec![
            BatchDisposition {
                seq: 1,
                disposition: CommandDisposition::NotRun,
            },
            BatchDisposition {
                seq: 2,
                disposition: CommandDisposition::NotRun,
            },
        ]
    );

    // 7. Valid inspected preflight refusal returns Rejected with NotRun.
    let mut r3 = observe(3, facts_pair(), "win-a");
    r3.request_seq = 4;
    let next_queued = [
        (
            3,
            WorldCommand::Focus {
                window: WindowSubject::Window(WindowId("win-a".to_owned())),
                direction: None,
            },
        ),
        (
            4,
            WorldCommand::ToggleMaximize {
                window: WindowSubject::CurrentFocus,
            },
        ),
    ];
    let res_preflight = engine
        .handle_report_with_gate_and_prepare(&r3, &next_queued, &RefusePreflightGate, |_| {
            prepare_calls += 1;
            Ok::<(), ()>(())
        })
        .expect("preflight refusal");
    assert_eq!(res_preflight.incorporation, ReportIncorporation::Rejected);
    assert_eq!(
        res_preflight.outcome,
        V4RunOutcome::Rejected {
            kind: KIND_REPLY_SIZE,
            message: MSG_REPLY_SIZE,
        }
    );
    assert_eq!(
        res_preflight.dispositions,
        vec![
            BatchDisposition {
                seq: 3,
                disposition: CommandDisposition::NotRun,
            },
            BatchDisposition {
                seq: 4,
                disposition: CommandDisposition::NotRun,
            },
        ]
    );
    assert_eq!(prepare_calls, 1);

    // 8. Invalid inspected batch returns Rejected with no per-command identities.
    let bad_order = [next_queued[1].clone(), next_queued[0].clone()];
    let res_invalid = engine
        .handle_report_with_gate_and_prepare(&r3, &bad_order, &StructuralCoreAdmissionGate, |_| {
            prepare_calls += 1;
            Ok::<(), ()>(())
        })
        .expect("invalid batch");
    assert_eq!(res_invalid.incorporation, ReportIncorporation::Rejected);
    assert_eq!(
        res_invalid.outcome,
        V4RunOutcome::Rejected {
            kind: KIND_SNAPSHOT,
            message: "command-order-invalid",
        }
    );
    assert!(res_invalid.dispositions.is_empty());
    assert_eq!(prepare_calls, 1);
}

fn cap_report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    cmd: Option<(u64, WorldCommand)>,
    closed: Vec<&str>,
    inventory_complete: bool,
) -> WorldReportV4 {
    let (command_seq, command) = match cmd {
        Some((cseq, c)) => (Some(cseq), Some(c)),
        None => (None, None),
    };
    report_full(
        seq,
        seq,
        vec![screen("out-a", 1920)],
        windows,
        focused,
        PresentationRowV4::Valid(ingress(focused)),
        command_seq,
        command,
        Vec::new(),
        closed,
        inventory_complete,
    )
}

fn seed_cap_members(
    eng: &mut WorldEngine,
    prefix: &str,
    count: usize,
    home: &crate::session::DomainKey,
    floating: bool,
) {
    use crate::world_members::MemberState;

    for i in 0..count {
        eng.world.workspace.members.insert(
            WindowId(format!("{prefix}-{i}")),
            MemberState {
                home: home.clone(),
                floating,
                minimized: false,
                geometry: None,
            },
        );
    }
}

fn run_cap_report<G: crate::world_admit_v4::CoreAdmissionGate + ?Sized>(
    eng: &mut WorldEngine,
    rep: &WorldReportV4,
    gate: &G,
) -> crate::world_engine::TransitionResult {
    eng.handle_report_v4_batched_with_gate_and_prepare(rep, &issue(rep), &[], gate, |_| {
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap_or_else(|e| match e {})
}

#[test]
fn retained_membership_cap_cumulative_churn_refusal_before_outbound_and_close_recovery() {
    use crate::session::DomainKey;
    use crate::world_admit_v4::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
        FutureReserveGateFacts, PreflightGateFacts, StructuralCoreAdmissionGate,
    };
    use crate::world_report::{MAX_RETAINED_MEMBERSHIPS, ReportIncorporation, WorldReplyV4};

    struct FailIfExactCalledGate;

    impl CoreAdmissionGate for FailIfExactCalledGate {
        fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            _facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_exact_inner(&self, _reply: &WorldReplyV4) -> ExactInnerDecision {
            panic!("membership cap must refuse before check_exact_inner")
        }

        fn check_future_reserve(
            &self,
            _reply: &WorldReplyV4,
            _facts: &FutureReserveGateFacts,
            _exact: ExactInnerCheck,
        ) -> AdmissionGateDecision {
            panic!("membership cap must refuse before check_future_reserve")
        }
    }

    let mut eng = engine();
    let _ = accepted_reply(1, admit(&mut eng, &observe(1, facts_pair(), "win-a")));
    assert_eq!(eng.world.workspace.members.len(), 2);

    // Seed 9,990 retained lifetimes on ("out-a", "ws-1") so 8 more across
    // cumulative incomplete reports reach the exact 10,000 cap.
    let home = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
    };
    seed_cap_members(&mut eng, "win-seed", 9_990, &home, false);
    assert_eq!(eng.world.workspace.members.len(), 9_992);

    let mut seq = 2u64;
    for batch in 0..2 {
        let mut wins = facts_pair();
        for j in 0..4 {
            let idx = batch * 4 + j;
            wins.push(fact(&format!("win-churn-{idx}"), "out-a", 200 + idx));
        }
        let rep = cap_report(seq, wins, "win-a", None, Vec::new(), false);
        let _ = accepted_reply(seq, admit(&mut eng, &rep));
        seq += 1;
    }
    assert_eq!(eng.world.workspace.members.len(), MAX_RETAINED_MEMBERSHIPS);

    // Transitory pre-retirement count > 10,000 succeeds when exact closes
    // bring the final staged count back to 10,000.
    let mut fit_wins = facts_pair();
    fit_wins.push(fact("win-fit-1", "out-a", 300));
    fit_wins.push(fact("win-fit-2", "out-a", 301));
    let fit_rep = cap_report(
        seq,
        fit_wins,
        "win-a",
        None,
        vec!["win-seed-0", "win-seed-1"],
        false,
    );
    let _ = accepted_reply(seq, admit(&mut eng, &fit_rep));
    seq += 1;
    assert_eq!(eng.world.workspace.members.len(), MAX_RETAINED_MEMBERSHIPS);
    for closed_id in ["win-seed-0", "win-seed-1"] {
        assert!(
            !eng.world
                .workspace
                .members
                .contains_key(&WindowId(closed_id.to_owned()))
        );
    }

    // Over-cap refuses before outbound/reply-size work and preserves state/replay.
    let rev_before = eng.state_revision();
    let members_before = eng.world.workspace.members.clone();
    let replay_before = eng.last_reply_v4();
    let mut over_wins = facts_pair();
    over_wins.push(fact("win-over-10001", "out-a", 302));
    let over_rep = cap_report(seq, over_wins, "win-a", None, Vec::new(), false);
    let over_res = run_cap_report(&mut eng, &over_rep, &FailIfExactCalledGate);
    assert_eq!(over_res.incorporation, ReportIncorporation::Rejected);
    assert_eq!(
        over_res.outcome,
        V4RunOutcome::Rejected {
            kind: KIND_SNAPSHOT,
            message: "membership-over-cap",
        }
    );
    assert!(over_res.dispositions.is_empty());
    assert_eq!(eng.state_revision(), rev_before);
    assert_eq!(eng.world.workspace.members, members_before);
    assert_eq!(eng.last_reply_v4(), replay_before);

    // Exact authoritative close frees space and a fitting report succeeds in the
    // same generation.
    let mut recover_wins = facts_pair();
    recover_wins.push(fact("win-recovered", "out-a", 303));
    let recover_rep = cap_report(seq, recover_wins, "win-a", None, vec!["win-seed-2"], false);
    let rec_res = run_cap_report(&mut eng, &recover_rep, &StructuralCoreAdmissionGate);
    assert_eq!(rec_res.incorporation, ReportIncorporation::Incorporated);
    match rec_res.outcome {
        V4RunOutcome::Accepted(reply) => assert_eq!(reply.state_revision, rev_before + 1),
        other => panic!("expected recovery acceptance, got {other:?}"),
    }
    assert_eq!(eng.world.workspace.members.len(), MAX_RETAINED_MEMBERSHIPS);
    assert!(
        !eng.world
            .workspace
            .members
            .contains_key(&WindowId("win-seed-2".to_owned()))
    );
    assert!(
        eng.world
            .workspace
            .members
            .contains_key(&WindowId("win-recovered".to_owned()))
    );
}

#[test]
fn retained_membership_cap_uncertainty_controls_and_command_created_entry() {
    use crate::directional::WindowSubject;
    use crate::session::DomainKey;
    use crate::world_admit_v4::StructuralCoreAdmissionGate;
    use crate::world_report::{
        FrameSetter, MAX_RETAINED_MEMBERSHIPS, ReportIncorporation, StateAttempt, StateProperty,
        TilingMode,
    };

    // 1. Uncertainty protection: held, floating-scope, displaced, and transfer
    //    members are never evicted at the 10,000 cap.
    let mut eng = engine();
    let mut initial_wins = facts_pair();
    initial_wins.push(fact("win-flight", "out-a", 150));
    let _ = accepted_reply(
        1,
        admit(&mut eng, &observe(1, initial_wins.clone(), "win-a")),
    );

    let send_rep = cap_report(
        2,
        initial_wins,
        "win-flight",
        Some((
            1,
            WorldCommand::Send {
                window: WindowSubject::Window(WindowId("win-flight".to_owned())),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
                follow: false,
            },
        )),
        Vec::new(),
        true,
    );
    let send_reply = accepted_reply(2, admit(&mut eng, &send_rep));
    assert_eq!(send_reply.transfers.len(), 1);
    assert_eq!(eng.world.workspace.members.len(), 3);

    let ws1 = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
    };
    let float_scope = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
    };
    let disp_home = DomainKey {
        output: OutputId("out-disp".to_owned()),
        workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
    };
    eng.world.floating_scopes.insert(float_scope.clone());
    eng.world
        .workspace
        .tiling
        .insert(float_scope.clone(), TilingMode::Floating);
    eng.world.workspace.displaced.insert(
        "out-disp".to_owned(),
        DisplacedEntry {
            workspaces: vec!["ws-1".to_owned()],
            owned: vec!["ws-1".to_owned()],
            dest: "out-a".to_owned(),
            origin_identity: crate::world_report::OutputIdentity::unknown(),
            origin_tuple_contested: false,
            origin_connector_contested: false,
        },
    );

    seed_cap_members(&mut eng, "win-held", 1, &ws1, false);
    seed_cap_members(&mut eng, "win-float", 1, &float_scope, true);
    seed_cap_members(&mut eng, "win-disp", 1, &disp_home, false);
    seed_cap_members(&mut eng, "win-u", 9_994, &float_scope, true);
    assert_eq!(eng.world.workspace.members.len(), MAX_RETAINED_MEMBERSHIPS);

    let mut complete_over = observe(
        3,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-uncertain-over", "out-a", 200),
        ],
        "win-a",
    );
    complete_over.held_windows.push(HeldWindowRow {
        window: WindowId("win-held-0".to_owned()),
        ordinal: 0,
        output: Some(OutputId("out-a".to_owned())),
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::ConflictingFact,
    });
    let rev_before = eng.state_revision();
    let over_res = run_cap_report(&mut eng, &complete_over, &StructuralCoreAdmissionGate);
    assert_eq!(over_res.incorporation, ReportIncorporation::Rejected);
    assert_eq!(
        over_res.outcome,
        V4RunOutcome::Rejected {
            kind: KIND_SNAPSHOT,
            message: "membership-over-cap",
        }
    );
    assert!(over_res.dispositions.is_empty());
    assert_eq!(eng.state_revision(), rev_before);
    for id in ["win-held-0", "win-float-0", "win-disp-0", "win-flight"] {
        assert!(
            eng.world
                .workspace
                .members
                .contains_key(&WindowId(id.to_owned())),
            "{id} must not be evicted"
        );
    }

    // 2. Causal command-created membership entry at the 10,000 cap:
    //    A sticky window (sticky: Some(true)) is excluded from members by
    //    report-only ingestion. Issuing ToggleSticky mints a pending exact
    //    native Sticky=false target; when the host refuses it (StateAttempt
    //    RefusedProven), native sticky rollback inserts win-sticky into members,
    //    overflowing 10,000 -> 10,001 until a positive close frees 1 slot.
    let mut cmd_eng = engine();
    let _ = accepted_reply(
        1,
        admit(
            &mut cmd_eng,
            &cap_report(
                1,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    sticky_fact("win-sticky", "out-a", 200),
                ],
                "win-a",
                None,
                Vec::new(),
                true,
            ),
        ),
    );
    assert_eq!(cmd_eng.world.workspace.members.len(), 2);
    assert!(
        !cmd_eng
            .world
            .workspace
            .members
            .contains_key(&WindowId("win-sticky".to_owned()))
    );

    cmd_eng.world.floating_scopes.insert(float_scope.clone());
    cmd_eng
        .world
        .workspace
        .tiling
        .insert(float_scope.clone(), TilingMode::Floating);
    seed_cap_members(&mut cmd_eng, "win-seed", 9_998, &float_scope, true);
    assert_eq!(
        cmd_eng.world.workspace.members.len(),
        MAX_RETAINED_MEMBERSHIPS
    );

    let cmd_rep = cap_report(
        2,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            sticky_fact("win-sticky", "out-a", 200),
        ],
        "win-a",
        Some((
            1,
            WorldCommand::ToggleSticky {
                window: WindowSubject::Window(WindowId("win-sticky".to_owned())),
            },
        )),
        Vec::new(),
        true,
    );
    let cmd_reply = accepted_reply(2, admit(&mut cmd_eng, &cmd_rep));
    assert!(cmd_eng.world.command_result.is_none());
    let sticky_row = cmd_reply
        .windows
        .iter()
        .find(|w| w.lifetime == WindowId("win-sticky".to_owned()))
        .expect("win-sticky reply row");
    assert_eq!(
        sticky_row
            .state
            .iter()
            .find_map(|t| (t.property == StateProperty::Sticky).then_some(t.value)),
        Some(false),
        "ToggleSticky must issue pending exact native Sticky=false target"
    );
    assert_eq!(
        cmd_eng.world.workspace.members.len(),
        MAX_RETAINED_MEMBERSHIPS
    );

    // Control at the exact same cap and identical sticky=true window fact
    // (absent StateAttempt): accepted without inserting win-sticky.
    let ack_base = cap_report(
        3,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            sticky_fact("win-sticky", "out-a", 200),
        ],
        "win-a",
        None,
        Vec::new(),
        true,
    );

    let mut ctrl_eng = cmd_eng.clone();
    let ctrl_res = run_cap_report(&mut ctrl_eng, &ack_base, &StructuralCoreAdmissionGate);
    assert_eq!(ctrl_res.incorporation, ReportIncorporation::Incorporated);
    assert_eq!(
        ctrl_eng.world.workspace.members.len(),
        MAX_RETAINED_MEMBERSHIPS
    );
    assert!(
        !ctrl_eng
            .world
            .workspace
            .members
            .contains_key(&WindowId("win-sticky".to_owned()))
    );

    // Causal refusal: identical report + Refused StateAttempt proves
    // RefusedProven and triggers native sticky rollback to insert win-sticky
    // into members -> 10,001 -> refused.
    let mut ack_with_attempt = ack_base;
    ack_with_attempt.state_attempts.push(StateAttempt {
        window: WindowId("win-sticky".to_owned()),
        property: StateProperty::Sticky,
        target: false,
        revision: cmd_reply.state_revision,
        disposition: FrameSetter::Refused,
        pre: Some(true),
        post: Some(true),
        attempt_ms: 1,
    });
    let rev_before_ack = cmd_eng.state_revision();
    let ack_res = run_cap_report(
        &mut cmd_eng,
        &ack_with_attempt,
        &StructuralCoreAdmissionGate,
    );
    assert_eq!(ack_res.incorporation, ReportIncorporation::Rejected);
    assert_eq!(
        ack_res.outcome,
        V4RunOutcome::Rejected {
            kind: KIND_SNAPSHOT,
            message: "membership-over-cap",
        }
    );
    assert!(ack_res.dispositions.is_empty());
    assert_eq!(cmd_eng.state_revision(), rev_before_ack);
    assert_eq!(
        cmd_eng.world.workspace.members.len(),
        MAX_RETAINED_MEMBERSHIPS
    );
    assert!(
        !cmd_eng
            .world
            .workspace
            .members
            .contains_key(&WindowId("win-sticky".to_owned()))
    );

    // Positive close freeing 1 slot lets the command-created member fit.
    let mut ack_recover = ack_with_attempt;
    ack_recover.closed.push(WindowId("win-seed-0".to_owned()));
    let fit_res = run_cap_report(&mut cmd_eng, &ack_recover, &StructuralCoreAdmissionGate);
    assert_eq!(fit_res.incorporation, ReportIncorporation::Incorporated);
    match fit_res.outcome {
        V4RunOutcome::Accepted(reply) => {
            assert_eq!(reply.state_revision, rev_before_ack + 1);
            assert!(
                reply
                    .windows
                    .iter()
                    .any(|w| w.lifetime == WindowId("win-sticky".to_owned())),
                "recovered reply must publish command-created member win-sticky"
            );
        }
        other => panic!("expected command-created member to fit after close, got {other:?}"),
    }
    assert_eq!(
        cmd_eng.world.workspace.members.len(),
        MAX_RETAINED_MEMBERSHIPS
    );
    assert!(
        !cmd_eng
            .world
            .workspace
            .members
            .contains_key(&WindowId("win-seed-0".to_owned()))
    );
    assert!(
        cmd_eng
            .world
            .workspace
            .members
            .contains_key(&WindowId("win-sticky".to_owned()))
    );
}

#[test]
fn numbered_send_moves_minimized_member_without_engine_carrier() {
    // R12 live regression: an iconified window keeps its membership home but
    // loses its Engine carrier (it lives only in the domain minimized slot),
    // so the focused-window send must settle as a membership-only transfer
    // instead of refusing with `send-window-unknown`.
    let mut engine = engine();
    let tiled_windows = || vec![fact("w0", "out-a", 0), fact("wKeep", "out-a", 100)];
    accepted_reply(1, admit(&mut engine, &observe(1, tiled_windows(), "w0")));
    let source_home = engine
        .world
        .workspace
        .members
        .get(&WindowId("w0".to_owned()))
        .expect("tiled membership")
        .home
        .clone();
    let iconic = || {
        let mut fact = fact("w0", "out-a", 0);
        fact.minimized = Some(true);
        fact.rect = None;
        fact
    };
    let minimized_windows = || vec![iconic(), fact("wKeep", "out-a", 100)];
    let mut minimized_ingress = ingress("w0");
    minimized_ingress.foreground_minimized = Some(true);
    minimized_ingress.foreground_frame = None;
    accepted_reply(
        2,
        admit(
            &mut engine,
            &report(
                2,
                minimized_windows(),
                "w0",
                PresentationRowV4::Valid(minimized_ingress.clone()),
                None,
                None,
                Vec::new(),
            ),
        ),
    );
    assert!(
        engine
            .world
            .workspace
            .members
            .get(&WindowId("w0".to_owned()))
            .expect("iconic membership")
            .minimized,
        "minimization keeps the member but clears the Engine carrier"
    );
    assert!(
        engine
            .world
            .staged
            .domains
            .iter()
            .all(|domain| domain.windows.iter().all(|w| w.window.0 != "w0")),
        "no Engine carrier exists for the iconified member"
    );
    let (outcome, settlements) = admit_with_settlements(
        &mut engine,
        &report(
            3,
            minimized_windows(),
            "w0",
            PresentationRowV4::Valid(minimized_ingress),
            Some(2),
            Some(WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("w0".to_owned())),
                source: SendSource::Explicit(OutputId("out-a".to_owned())),
                index: 2,
                follow: true,
            }),
            Vec::new(),
        ),
    );
    let _reply = accepted_reply(3, outcome);
    assert_eq!(settlements.len(), 1);
    assert!(
        settlements[0].result.is_none(),
        "membership-only send executes without a typed rejection, got {:?}",
        settlements
    );
    let target_home = engine
        .world
        .workspace
        .members
        .get(&WindowId("w0".to_owned()))
        .expect("iconic membership survives the send")
        .home
        .clone();
    assert_ne!(target_home.workspace, source_home.workspace);
    assert_eq!(target_home.output, source_home.output);
    let transfer = engine
        .world
        .settlement
        .retained
        .iter()
        .find(|r| matches!(&r.target.kind, crate::world_report::TransferKind::Window(w) if w.0 == "w0"))
        .expect("membership-only transfer retained");
    assert_eq!(transfer.target.target_workspace, target_home.workspace.0);
    assert!(
        engine.world.staged.domains.iter().any(|domain| {
            domain.key == target_home
                && domain.minimized.iter().any(|w| w.0 == "w0")
        }),
        "the iconified mover stages minimized in its new domain"
    );
    // Unknown lifetimes still refuse with the cataloged kind: the new branch
    // widens exactly to known iconic members.
    let mut minimized_b = fact("wKeep", "out-a", 100);
    minimized_b.minimized = Some(true);
    minimized_b.rect = None;
    let (unknown_outcome, unknown_settlements) = admit_with_settlements(
        &mut engine,
        &report(
            4,
            vec![iconic(), minimized_b],
            "w0",
            PresentationRowV4::Valid(ingress("w0")),
            Some(3),
            Some(WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId(
                    "w-ghost".to_owned(),
                )),
                source: SendSource::Explicit(OutputId("out-a".to_owned())),
                index: 1,
                follow: false,
            }),
            Vec::new(),
        ),
    );
    let _unknown_reply = accepted_reply(4, unknown_outcome);
    assert_eq!(
        unknown_settlements
            .first()
            .and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::SendWindowUnknown
        }),
        "unknown windows keep the send-window-unknown refusal"
    );
}
