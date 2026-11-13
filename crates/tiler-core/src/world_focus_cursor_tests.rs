//! Execution-time logical focus cursor (retained-focus Q1): the cursor is
//! separate from native observed foreground and per-workspace history.
//! Accepted focus choices advance it immediately in FIFO order; a live
//! retained request preserves its target across unchanged-baseline
//! reports; a newer observed focus supersedes it; refusal resets it to
//! the eligible observed focus; non-focus and refused commands never
//! erase it. Implicit subjects resolve from the cursor before a newer
//! focus decision retires the older authority.

use crate::boundary::CoreCommand;
use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, SameAxisMove, WindowId};
use crate::ids::{CorrelationId, OwnerId};
use crate::world_engine::{EventIssue, KIND_GENERATION, WorldEngine};
use crate::world_report::{
    BatchSettlement, CommandRejectKind, CommandResult, FocusFeedback, FocusRequestCause,
    HeldWindowRow, PresentationIngressV4, PresentationRowV4, RowDiagnosticKind, ScopeAuthority,
    SendSource, StateProperty, StateTarget, VisibilityReason, VisibilitySetter,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
};
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

fn ingress(fg: &str) -> PresentationIngressV4 {
    PresentationIngressV4 {
        foreground: fg.to_owned(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: None,
        chord_held: false,
        move_window: String::new(),
        pager_order: Vec::new(),
        pager_frames: Vec::new(),
        witness: None,
        foreground_out: if fg.is_empty() {
            None
        } else {
            Some(crate::directional::OutputId("out-a".to_owned()))
        },
    }
}

fn report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
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
        screens: vec![screen("out-a", 0)],
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
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
        presentation: PresentationRowV4::Valid(ingress(focused)),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn observe(seq: u64, windows: Vec<WindowFact>, focused: &str) -> WorldReportV4 {
    report(seq, windows, focused, None, None)
}

fn issue_for(report: &WorldReportV4) -> EventIssue {
    EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse(&report.request_id).expect("correlation"),
    }
}

fn run_batched(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
    queued: &[(u64, WorldCommand)],
) -> (WorldReplyV4, Vec<BatchSettlement>) {
    match engine.handle_report_v4_batched(report, &issue_for(report), queued) {
        (V4RunOutcome::Accepted(reply), settlements) => (reply, settlements),
        (other, _) => panic!("expected accepted batch, got {other:?}"),
    }
}

fn focus_cmd(window: &str) -> WorldCommand {
    WorldCommand::Focus {
        window: WindowId(window.to_owned()),
        direction: String::new(),
    }
}

fn implicit(command: WorldCommand) -> WorldCommand {
    WorldCommand::Implicit(Box::new(command))
}

fn implicit_focus_dir(direction: &str) -> WorldCommand {
    implicit(WorldCommand::FocusDirectional {
        window: WindowId(String::new()),
        direction: direction.to_owned(),
    })
}

fn assert_not_rejected(result: &Option<CommandResult>, what: &str) {
    assert!(
        !matches!(result, Some(CommandResult::Rejected { .. })),
        "{what} must not reject; got {result:?}"
    );
}

/// Target proof for an implicit native maximize: the settlement carries
/// the maximized window identity, not merely a non-rejection.
fn assert_maximize_on(result: &Option<CommandResult>, expected: &str, what: &str) {
    match result {
        Some(CommandResult::Maximize { window, .. }) => {
            assert_eq!(window.0, expected, "{what} must act on {expected}",)
        }
        other => panic!("{what} must settle maximize; got {other:?}"),
    }
}

fn windows() -> Vec<WindowFact> {
    vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)]
}

/// Focus B, then a non-focus command in the same batch, then a later
/// unchanged-baseline capture: the pending request survives both, and a
/// trailing implicit command acts on the cursor (B), not stale native A.
#[test]
fn cursor_focus_then_nonfocus_survives_and_implicit_acts_on_cursor() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    // Same batch: accepted focus B followed by an accepted non-focus
    // command. The non-focus command must not cancel pending B.
    let (setup, settlements) = run_batched(
        &mut engine,
        &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[(2, WorldCommand::SelectPrevious { output: None })],
    );
    assert_eq!(settlements.len(), 2);
    let trailing = settlements
        .iter()
        .find(|settlement| settlement.seq == 2)
        .expect("trailing non-focus settles");
    assert_not_rejected(&trailing.result, "trailing non-focus");
    // Focus never publishes as one-shot effects on the v4 wire: the
    // retained request is the authority.
    assert!(setup.effects.is_empty());
    let pending = setup.focus_request.clone().expect("focus B retained");
    assert_eq!(pending.target.0, "win-b");
    assert_eq!(pending.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-b".to_owned())),
        "accepted focus advances the cursor immediately"
    );
    // Later capture on an unchanged baseline (native still A): the live
    // retained target survives with the identical id.
    let (held, _) = run_batched(&mut engine, &observe(3, windows(), "win-a"), &[]);
    let again = held.focus_request.clone().expect("pending B survives");
    assert_eq!(again.target.0, "win-b");
    assert_eq!(again.id, pending.id, "re-emission never changes the id");
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-b".to_owned())),
        "stale observed A must not overwrite pending B"
    );
    // The next implicit subject resolves from the cursor (B): moving
    // right from B reaches A, which stale-A resolution could never do.
    // The new decision supersedes pending B with a new id.
    let (moved, moved_settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-a"),
        &[(3, implicit_focus_dir("right"))],
    );
    assert_eq!(moved_settlements.len(), 1);
    assert_not_rejected(&moved_settlements[0].result, "implicit directional");
    let superseded = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(
        superseded.target.0, "win-a",
        "implicit subject must be cursor B, not stale native A"
    );
    assert_ne!(superseded.id, pending.id);
}

/// Focus B, then a refused non-focus command: the refusal settles only
/// its own sequence while pending B survives with its cursor intact.
#[test]
fn cursor_refused_nonfocus_never_cancels_pending() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let refused_send = WorldCommand::Send {
        window: WindowId("win-zzz".to_owned()),
        target_output: crate::directional::OutputId("out-a".to_owned()),
        target_workspace: "ws-1".to_owned(),
        follow: false,
    };
    let (reply, settlements) = run_batched(
        &mut engine,
        &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[(2, refused_send)],
    );
    assert_eq!(settlements.len(), 2);
    let trailing = settlements
        .iter()
        .find(|settlement| settlement.seq == 2)
        .expect("trailing send settles");
    assert!(
        matches!(trailing.result, Some(CommandResult::Rejected { .. })),
        "trailing send must refuse; got {:?}",
        trailing.result
    );
    assert!(reply.effects.is_empty());
    let pending = reply
        .focus_request
        .clone()
        .expect("pending B survives refusal");
    assert_eq!(pending.target.0, "win-b");
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-b".to_owned())),
        "refused non-focus must not erase the cursor"
    );
    // The CoreCommand operation seam stays non-focus vocabulary: an
    // operation refusal is equally powerless over pending focus.
    let refused_op = WorldCommand::Operation(CoreCommand::Move {
        window: "win-zzz".to_owned(),
        direction: "left".to_owned(),
        cross_output_transfer: false,
        same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
    });
    let (later, _) = run_batched(
        &mut engine,
        &observe(3, windows(), "win-a"),
        &[(3, refused_op)],
    );
    let still = later
        .focus_request
        .clone()
        .expect("pending B survives op refusal");
    assert_eq!(still.target.0, "win-b");
    assert_eq!(still.id, pending.id);
}

/// Pending B refused by correlated feedback: the cursor resets to the
/// eligible observed focus (A), never the abandoned target, while
/// per-workspace history keeps B. The next implicit subject is A.
#[test]
fn cursor_refused_pending_resets_to_observed_not_abandoned() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, _) = run_batched(
        &mut engine,
        &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[],
    );
    let pending = setup.focus_request.clone().expect("focus B retained");
    assert_eq!(pending.target.0, "win-b");
    // Correlated refusal for the exact pending tuple retires it.
    let mut refused = observe(3, windows(), "win-a");
    refused.focus_feedback = vec![FocusFeedback {
        id: pending.id.clone(),
        target: WindowId("win-b".to_owned()),
        revision: pending.issued_revision,
        setter: VisibilitySetter::Refused,
        reason: VisibilityReason::NativeRefused,
        pre: Some(WindowId("win-a".to_owned())),
        post: Some(WindowId("win-a".to_owned())),
        foreground_proven: false,
        arrival: Vec::new(),
        attempt_ms: 0,
    }];
    let (settled, _) = run_batched(&mut engine, &refused, &[]);
    assert!(
        settled.focus_request.is_none(),
        "correlated refusal must retire pending B; got {:?}",
        settled.focus_request
    );
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-a".to_owned())),
        "retired cursor resets to eligible observed A, not abandoned B"
    );
    assert!(
        engine.last_active_window.values().any(|w| w.0 == "win-b"),
        "per-workspace history stays on B while the cursor resets to A"
    );
    // The next implicit subject is observed A: moving left from A
    // reaches B, which abandoned-B resolution could never produce.
    let (moved, moved_settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-a"),
        &[(2, implicit_focus_dir("left"))],
    );
    assert_eq!(moved_settlements.len(), 1);
    assert_not_rejected(&moved_settlements[0].result, "implicit directional");
    let next = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(
        next.target.0, "win-b",
        "implicit subject after refusal must be observed A, not abandoned B"
    );
}

/// USER DECISION 2026-11-12 (empty-cursor rule), discriminator 1: a
/// restart-shaped first capture (no home for A) establishes no execution
/// subject, the commandless recovery adopts the restored sticky foreground
/// A without minting anything, and an implicit no-target sticky-off then
/// acts on A. The fallback mints no focus request and writes neither
/// canonical memory nor session state.
#[test]
fn empty_cursor_implicit_acts_on_restored_sticky_foreground() {
    let mut engine = engine();
    // No home for A yet: the host reports it focused, but nothing is
    // recordable, so no subject is established.
    engine
        .handle_report_v4_for_test(&observe(1, vec![fact("win-b", "out-a", 0)], "win-a"))
        .expect("admission observes");
    assert_eq!(
        engine.execution_subject(),
        None,
        "homeless first capture leaves no execution subject"
    );
    let windows = vec![
        WindowFact {
            sticky: Some(true),
            ..fact("win-a", "out-a", 0)
        },
        fact("win-b", "out-a", 100),
    ];
    // Commandless recovery adopts A with identity unchanged; the withheld
    // automatic sticky lane mints nothing, so the cursor stays empty.
    let (recovered, _) = run_batched(&mut engine, &observe(2, windows.clone(), "win-a"), &[]);
    assert!(
        recovered.focus_request.is_none(),
        "recovery mints no automatic request; got {:?}",
        recovered.focus_request
    );
    assert_eq!(
        engine.logical_focus_cursor, None,
        "recovery adopts A but sets no cursor slot"
    );
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-a".to_owned())),
        "empty cursor now observes live A as the implicit subject"
    );
    let before = engine.last_active_window.clone();
    let toggle = WorldCommand::Implicit(Box::new(WorldCommand::ToggleSticky {
        window: WindowId(String::new()),
    }));
    let (reply, settlements) =
        run_batched(&mut engine, &observe(3, windows, "win-a"), &[(1, toggle)]);
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "implicit sticky-off acts on live A");
    assert!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
            .is_some_and(|binding| !binding.target.value),
        "implicit sticky-off lands on win-a; got {:?}",
        engine.retained_states.keys().collect::<Vec<_>>(),
    );
    assert!(
        reply.focus_request.is_none(),
        "fallback mints no focus request; got {:?}",
        reply.focus_request
    );
    assert_eq!(
        engine.last_active_window, before,
        "fallback writes no canonical memory"
    );
}

/// Discriminator 2: windows the automatic lane withholds still serve as
/// the implicit subject once the host focuses them: a newcomer sticky, a
/// float owning the foreground over a withheld tiled newcomer, and a
/// maximized ordinary float.
#[test]
fn empty_cursor_implicit_acts_despite_withheld_automatic_guards() {
    // Newcomer sticky: the automatic target restriction withholds it.
    let mut sticky_engine = engine();
    sticky_engine
        .handle_report_v4_for_test(&observe(1, vec![fact("win-b", "out-a", 0)], "win-s"))
        .expect("admission observes");
    assert_eq!(sticky_engine.execution_subject(), None);
    let sticky_windows = vec![
        WindowFact {
            sticky: Some(true),
            ..fact("win-s", "out-a", 0)
        },
        fact("win-b", "out-a", 100),
    ];
    let toggle = WorldCommand::Implicit(Box::new(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    }));
    let (reply, settlements) = run_batched(
        &mut sticky_engine,
        &observe(2, sticky_windows, "win-s"),
        &[(1, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "newcomer sticky");
    assert_maximize_on(&settlements[0].result, "win-s", "newcomer sticky maximize");
    assert!(
        reply.focus_request.is_none(),
        "newcomer sticky: no focus request minted; got {:?}",
        reply.focus_request
    );
    // Floating over the foreground: the tiled newcomer's activation is
    // withheld under the focused float, so no subject is established, and
    // the implicit command then acts on the float itself. This Act 1 is
    // the distinct empty-cursor-F case: the withheld newcomer T is present
    // but unfocused, so the fallback can only see F.
    let mut float_engine = engine();
    float_engine
        .handle_report_v4_for_test(&observe(1, vec![fact("win-a", "out-a", 0)], "win-f"))
        .expect("admission observes");
    assert_eq!(float_engine.execution_subject(), None);
    let float_windows = vec![
        fact("win-a", "out-a", 0),
        WindowFact {
            transient: Some(true),
            ..fact("win-f", "out-a", 100)
        },
        fact("win-t", "out-a", 200),
    ];
    let toggle = WorldCommand::Implicit(Box::new(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    }));
    let (reply, settlements) = run_batched(
        &mut float_engine,
        &observe(2, float_windows.clone(), "win-f"),
        &[(1, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "floating over foreground");
    assert_maximize_on(&settlements[0].result, "win-f", "empty-cursor fallback");
    assert!(
        reply.focus_request.is_none(),
        "floating over foreground: no focus request minted; got {:?}",
        reply.focus_request
    );
    // Act 2 (withheld-newcomer-later-focused): the host now focuses the
    // previously withheld tiled newcomer T. The implicit command acts on
    // T, with no rearmed automatic focus. No canonical-memory assertion
    // here: the genuine native focus change F -> T records through the
    // pre-existing native path.
    let toggle = WorldCommand::Implicit(Box::new(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    }));
    let (reply, settlements) = run_batched(
        &mut float_engine,
        &observe(3, float_windows, "win-t"),
        &[(2, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "later-focused withheld newcomer");
    assert_maximize_on(
        &settlements[0].result,
        "win-t",
        "later-focused withheld newcomer",
    );
    assert!(
        reply.focus_request.is_none(),
        "withheld newcomer focus mints no automatic request; got {:?}",
        reply.focus_request
    );
    // Maximized ordinary float: the automatic float-overlay veto withholds
    // it, but the implicit command still acts on it once host-focused.
    let mut max_engine = engine();
    max_engine
        .handle_report_v4_for_test(&observe(1, vec![fact("win-a", "out-a", 0)], "win-m"))
        .expect("admission observes");
    assert_eq!(max_engine.execution_subject(), None);
    let max_windows = vec![
        fact("win-a", "out-a", 0),
        WindowFact {
            transient: Some(true),
            maximized: Some(true),
            ..fact("win-m", "out-a", 100)
        },
    ];
    let toggle = WorldCommand::Implicit(Box::new(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    }));
    let (reply, settlements) = run_batched(
        &mut max_engine,
        &observe(2, max_windows, "win-m"),
        &[(1, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "maximized ordinary float");
    assert_maximize_on(&settlements[0].result, "win-m", "maximized ordinary float");
    assert!(
        reply.focus_request.is_none(),
        "maximized ordinary float: no focus request minted; got {:?}",
        reply.focus_request
    );
}

/// Discriminator 3: with an empty cursor, an unknown, unbound, or stale
/// (closed) foreground is still refused as focus-unknown.
#[test]
fn empty_cursor_unknown_unbound_stale_still_refused() {
    let toggle = || {
        WorldCommand::Implicit(Box::new(WorldCommand::ToggleMaximize {
            window: WindowId(String::new()),
        }))
    };
    // Unknown: names no live fact.
    let mut unknown_engine = engine();
    unknown_engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    assert_eq!(unknown_engine.execution_subject(), None);
    let (_, settlements) = run_batched(
        &mut unknown_engine,
        &observe(2, windows(), "win-ghost"),
        &[(1, toggle())],
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unknown foreground must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
    // Unbound: a fact with no logical homing.
    let mut unbound_engine = engine();
    unbound_engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    let mut unbound = observe(
        2,
        vec![
            fact("win-a", "out-a", 0),
            WindowFact {
                output: None,
                ..fact("win-u", "out-a", 200)
            },
        ],
        "win-u",
    );
    unbound.foreground_known = true;
    let (_, settlements) = run_batched(&mut unbound_engine, &unbound, &[(1, toggle())]);
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unbound foreground must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
    // Stale: explicitly closed this report.
    let mut stale_engine = engine();
    stale_engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    let mut stale = observe(2, vec![fact("win-a", "out-a", 0)], "win-b");
    stale.closed = vec![WindowId("win-b".to_owned())];
    let (_, settlements) = run_batched(&mut stale_engine, &stale, &[(1, toggle())]);
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "stale foreground must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
}

/// Discriminator 3b (USER DECISION 2026-11-12, stale-generation
/// ingress): a wrong-generation report with a queued implicit is refused
/// at the generation gate before any run or mutation -- empty
/// settlements, no subject adopted, no state effects -- and a following
/// current-bound unknown-foreground report then refuses focus-unknown at
/// command scope. The gate refusal must not be conflated with the
/// command FocusUnknown that follows it.
#[test]
fn empty_cursor_stale_generation_ingress_refuses_before_run() {
    let toggle = || {
        WorldCommand::Implicit(Box::new(WorldCommand::ToggleMaximize {
            window: WindowId(String::new()),
        }))
    };
    let mut gen_engine = engine();
    gen_engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    assert_eq!(gen_engine.execution_subject(), None);
    let revision = gen_engine.state_revision;
    let mut bad = observe(2, windows(), "win-a");
    bad.core_generation = "gen-999".to_owned();
    let (outcome, settlements) =
        gen_engine.handle_report_v4_batched(&bad, &issue_for(&bad), &[(1, toggle())]);
    match outcome {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(
            kind, KIND_GENERATION,
            "wrong generation must refuse at the generation gate",
        ),
        other => panic!("wrong generation must gate-reject; got {other:?}"),
    }
    assert!(
        settlements.is_empty(),
        "gate refusal settles nothing; got {settlements:?}",
    );
    assert_eq!(
        gen_engine.execution_subject(),
        None,
        "gate refusal adopts no subject",
    );
    assert_eq!(
        gen_engine.logical_focus_cursor, None,
        "gate refusal sets no cursor slot",
    );
    assert!(
        gen_engine.pending_focus_request().is_none(),
        "gate refusal mints no request; got {:?}",
        gen_engine.pending_focus_request(),
    );
    assert_eq!(
        gen_engine.state_revision, revision,
        "gate refusal mutates no state",
    );
    // The next current-bound report is unaffected by the gate refusal:
    // its unknown foreground refuses focus-unknown at command scope.
    let (_, settlements) = run_batched(
        &mut gen_engine,
        &observe(3, windows(), "win-ghost"),
        &[(2, toggle())],
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unknown foreground after gate refusal must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
}

/// Pending-live guard: an explicit focus on a newcomer mints a retained
/// request through the production run, while its narrowly held home
/// scope fail-closes the canonical record so the cursor slot stays
/// empty. The live request then owns the subject -- the empty cursor
/// never falls back to observed foreground -- so a queued implicit
/// refuses focus-unknown while the request stays live.
#[test]
fn pending_live_empty_cursor_guards_implicit_subject() {
    let mut pending_engine = engine();
    pending_engine
        .handle_report_v4_for_test(&observe(
            1,
            vec![fact("win-b", "out-a", 0), fact("win-h", "out-a", 100)],
            "win-a",
        ))
        .expect("admission observes");
    assert_eq!(pending_engine.execution_subject(), None);
    // win-h rides a narrowly held row (live rows stay unheld) so its
    // retained scope -- also win-n's home -- fences the canonical
    // record without refusing the explicit effect.
    let mut held_focus = report(
        2,
        vec![fact("win-b", "out-a", 0), fact("win-n", "out-a", 100)],
        "win-a",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    held_focus.held_windows = vec![HeldWindowRow {
        window: WindowId("win-h".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let (reply, _) = run_batched(&mut pending_engine, &held_focus, &[]);
    let pending = reply
        .focus_request
        .clone()
        .expect("held-scope explicit focus is retained");
    assert_eq!(pending.target.0, "win-n");
    assert_eq!(
        pending_engine.logical_focus_cursor, None,
        "fail-closed canonical record leaves the cursor slot empty",
    );
    assert_eq!(
        pending_engine.execution_subject(),
        None,
        "a live retained request owns the subject; empty cursor never falls back",
    );
    // The next report drops win-n from the staged set on the unchanged
    // baseline: the preserve-slot logic cannot rehome it, so the cursor
    // stays empty while the retained request stays live, and the queued
    // implicit refuses focus-unknown under the guard.
    let toggle = WorldCommand::Implicit(Box::new(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    }));
    let (_, settlements) = run_batched(
        &mut pending_engine,
        &observe(3, vec![fact("win-b", "out-a", 0)], "win-a"),
        &[(2, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "implicit under live pending with empty cursor must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
    let still = pending_engine
        .pending_focus_request()
        .expect("retained request stays live across the refusal");
    assert_eq!(still.target.0, "win-n");
    assert_eq!(still.id, pending.id);
}

/// Discriminator 4: a live logical cursor still wins over a divergent
/// observed foreground; the empty-cursor fallback never overrides it.
#[test]
fn logical_cursor_wins_over_divergent_observed() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, _) = run_batched(
        &mut engine,
        &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[],
    );
    let pending = setup.focus_request.clone().expect("focus B retained");
    assert_eq!(pending.target.0, "win-b");
    let (held, _) = run_batched(&mut engine, &observe(3, windows(), "win-a"), &[]);
    assert!(held.focus_request.is_some(), "pending B survives");
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-b".to_owned())),
        "divergent observed A must not override cursor B"
    );
    // The next implicit subject resolves from the cursor (B): the new
    // rightward decision supersedes pending B with a fresh id.
    let (moved, moved_settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-a"),
        &[(3, implicit_focus_dir("right"))],
    );
    assert_eq!(moved_settlements.len(), 1);
    assert_not_rejected(&moved_settlements[0].result, "implicit directional");
    let next = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(next.target.0, "win-a");
    assert_ne!(next.id, pending.id, "cursor path supersedes with a new id");
}

fn gap_windows() -> Vec<WindowFact> {
    vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ]
}

fn implicit_move_dir(direction: &str) -> WorldCommand {
    implicit(WorldCommand::MoveDirectional {
        window: WindowId(String::new()),
        direction: direction.to_owned(),
        same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
    })
}

fn scope_hold_any(reply: &WorldReplyV4) -> bool {
    reply
        .scopes
        .iter()
        .any(|status| matches!(status.authority, ScopeAuthority::Hold(_)))
}

fn gap_prefix(engine: &mut WorldEngine) -> WorldReplyV4 {
    engine
        .handle_report_v4_for_test(&observe(1, gap_windows(), "win-a"))
        .expect("admission observes");
    let (_, _) = run_batched(engine, &observe(2, gap_windows(), "win-a"), &[]);
    let mut gapped = observe(
        3,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
        "win-b",
    );
    gapped.screens[0].complete = false;
    gapped.inventory_complete = false;
    gapped.held_windows = vec![HeldWindowRow {
        window: WindowId("win-a".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let (reply, _) = run_batched(engine, &gapped, &[]);
    assert!(
        scope_hold_any(&reply),
        "gap report holds the source domain; got {:?}",
        reply.scopes
    );
    assert!(
        reply.focus_request.is_none(),
        "gap mints no request; got {:?}",
        reply.focus_request
    );
    let (recovered, _) = run_batched(engine, &observe(4, gap_windows(), "win-b"), &[]);
    assert!(
        !scope_hold_any(&recovered),
        "recovery clears the hold; got {:?}",
        recovered.scopes
    );
    assert!(
        recovered.focus_request.is_none(),
        "recovery mints no request; got {:?}",
        recovered.focus_request
    );
    assert_eq!(
        engine.logical_focus_cursor, None,
        "no focus decision ever ran, so the cursor slot stays empty"
    );
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-b".to_owned())),
        "recovery subject is observed B"
    );
    recovered
}

/// Gap then focus: incomplete inventory plus a held source domain on a
/// genuine A -> B native change, then complete unchanged B. The observed
/// rightward probe from B reaches C.
#[test]
fn gap_held_incomplete_then_focus_acts_on_recovered_foreground() {
    let mut engine = engine();
    gap_prefix(&mut engine);
    let (moved, settlements) = run_batched(
        &mut engine,
        &observe(5, gap_windows(), "win-b"),
        &[(7, implicit_focus_dir("right"))],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "gap focus probe");
    let next = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(next.target.0, "win-c", "observed rightward probe from B");
    assert_eq!(next.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(next.cmd_seq, Some(7));
    assert_eq!(next.id, "focus-5");
}

/// Gap then move: same prefix on a fresh engine. The observed rightward
/// windowed move from B changes the tiling and keeps focus on mover B.
#[test]
fn gap_held_incomplete_then_move_acts_on_recovered_foreground() {
    let mut engine = engine();
    let recovered = gap_prefix(&mut engine);
    let before: Vec<_> = recovered
        .windows
        .iter()
        .map(|row| (row.lifetime.0.clone(), row.allocation))
        .collect();
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(5, gap_windows(), "win-b"),
        &[(7, implicit_move_dir("right"))],
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        settlements[0].result.is_none(),
        "tiled move success carries no result; got {:?}",
        settlements[0].result
    );
    assert!(
        reply.focus_request.as_ref().is_some_and(|row| {
            row.target.0 == "win-b"
                && row.cause == FocusRequestCause::ExplicitCommand
                && row.cmd_seq == Some(7)
                && row.id == "focus-5"
        }),
        "move keeps focus on mover B; got {:?}",
        reply.focus_request
    );
    let after: Vec<_> = reply
        .windows
        .iter()
        .map(|row| (row.lifetime.0.clone(), row.allocation))
        .collect();
    assert_ne!(before, after, "move must change the tiling allocation");
    let moved_b = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("win-b row");
    assert!(
        before
            .iter()
            .any(|(win, alloc)| win == "win-b" && *alloc != moved_b.allocation),
        "B allocation must change; before {before:?} after {after:?}"
    );
}

/// Send-with-follow settles by target equality on the next unchanged
/// observation (zero-report retention): the follow row lives for the
/// issuing publication only. A later genuine user focus change then runs
/// on the new baseline with no row involved.
#[test]
fn send_follow_satisfies_at_next_observation_and_changed_focus_runs_new_baseline() {
    let mut engine = engine();
    let facts = vec![
        fact("win-a", "out-a", 0),
        fact("win-s", "out-a", 100),
        fact("win-b", "out-a", 200),
    ];
    let first = engine
        .handle_report_v4_for_test(&observe(1, facts.clone(), "win-a"))
        .expect("admission observes");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring.len() >= 2, "same-output send needs a ring: {ring:?}");
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("logical view");
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("second workspace");
    let send = WorldCommand::Send {
        window: WindowId("win-a".to_owned()),
        target_output: crate::directional::OutputId("out-a".to_owned()),
        target_workspace: other.clone(),
        follow: true,
    };
    let (sent, _) = run_batched(
        &mut engine,
        &report(2, facts.clone(), "win-a", Some(11), Some(send)),
        &[],
    );
    let pending = sent.focus_request.clone().expect("send-follow retains");
    assert_eq!(pending.target.0, "win-a");
    assert_eq!(pending.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(pending.cmd_seq, Some(11));
    assert_eq!(pending.id, "focus-2");
    assert_eq!(pending.issued_revision, 2);
    assert_eq!(pending.after_shown, None);
    let satisfied = run_batched(&mut engine, &observe(3, facts.clone(), "win-a"), &[]).0;
    assert_eq!(
        satisfied.focus_request, None,
        "follow satisfies on next unchanged observation; got {:?}",
        satisfied.focus_request
    );
    // Genuine user focus change to win-s, then the observed implicit
    // right from S reaches B.
    let (moved, settlements) = run_batched(
        &mut engine,
        &observe(4, facts.clone(), "win-s"),
        &[(12, implicit_focus_dir("right"))],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "implicit after change");
    let next = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(next.target.0, "win-b", "observed rightward probe from S");
}

/// Held domain with a live bound baseline: B/C live, native moves B -> C
/// while the source screen is held, then an explicit focus on newcomer N
/// in the held domain mints a live row. Repeated held reports keep N
/// with a restored cursor; a held move refuses with the row retained;
/// clearing the hold lets a focus probe supersede to C and a windowed
/// probe act on C.
#[test]
fn held_live_baseline_newcomer_restores_cursor_and_probes_act_from_it() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(
            1,
            vec![fact("win-b", "out-a", 0), fact("win-c", "out-a", 100)],
            "win-b",
        ))
        .expect("admission observes");
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-b".to_owned()))
    );
    let live = vec![
        fact("win-n", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    // Genuine native change B -> C with the source screen held.
    let mut held_change = observe(
        2,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
        "win-c",
    );
    held_change.screens[0].complete = false;
    let (changed, _) = run_batched(&mut engine, &held_change, &[]);
    assert!(
        scope_hold_any(&changed),
        "native change under incomplete screen holds; got {:?}",
        changed.scopes
    );
    assert_eq!(
        engine.logical_focus_cursor, None,
        "no focus decision ran, so the cursor slot stays empty"
    );
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-c".to_owned())),
        "empty-cursor fallback observes live C under held change"
    );
    // Explicit focus on the live held-domain newcomer N mints a row.
    let mut held_mint = report(3, live.clone(), "win-c", Some(1), Some(focus_cmd("win-n")));
    held_mint.screens[0].complete = false;
    let (minted, _) = run_batched(&mut engine, &held_mint, &[]);
    let pending = minted.focus_request.clone().expect("held explicit retains");
    assert_eq!(pending.target.0, "win-n");
    assert_eq!(pending.cause, FocusRequestCause::ExplicitCommand);
    // The same held baseline with no command keeps N and restores the
    // cursor to N even while held: the preserve slot has no hold gate.
    let mut held_same = observe(4, live.clone(), "win-c");
    held_same.screens[0].complete = false;
    let (kept, _) = run_batched(&mut engine, &held_same, &[]);
    assert_eq!(
        kept.focus_request.clone().map(|row| (row.target, row.id)),
        Some((WindowId("win-n".to_owned()), pending.id.clone())),
        "held repeat keeps the identical row; got {:?}",
        kept.focus_request
    );
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-n".to_owned())),
        "live N restores the cursor even while held"
    );
    // Held move from N refuses (held source domain fences mutation) and
    // the live row survives the refusal unchanged.
    let mut held_move = observe(5, live.clone(), "win-c");
    held_move.screens[0].complete = false;
    let (after_move, move_settlements) =
        run_batched(&mut engine, &held_move, &[(2, implicit_move_dir("right"))]);
    assert_eq!(move_settlements.len(), 1);
    assert!(
        matches!(
            move_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused,
            })
        ),
        "held move must refuse operation-refused; got {:?}",
        move_settlements[0].result
    );
    assert_eq!(
        after_move
            .focus_request
            .clone()
            .map(|row| (row.target, row.id)),
        Some((WindowId("win-n".to_owned()), pending.id.clone())),
        "held refusal retains the identical row; got {:?}",
        after_move.focus_request
    );
    // Repeated Focus(N) on the still-held domain when the cursor is
    // already Some(N) must preserve the matching cursor so a same-batch
    // implicit probe resolves subject N (refusing OperationRefused on the
    // held scope, never FocusUnknown). The new explicit decision supersedes
    // the earlier request even though its target is unchanged.
    let mut held_repeat_focus = report(6, live.clone(), "win-c", Some(3), Some(focus_cmd("win-n")));
    held_repeat_focus.screens[0].complete = false;
    let (after_repeat, repeat_settlements) = run_batched(
        &mut engine,
        &held_repeat_focus,
        &[(4, implicit_move_dir("right"))],
    );
    assert_eq!(repeat_settlements.len(), 2);
    assert!(
        matches!(
            repeat_settlements[1].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused,
            })
        ),
        "repeated same-target held focus must preserve cursor N so trailing probe refuses operation-refused (not focus-unknown); got {:?}",
        repeat_settlements[1].result
    );
    assert_eq!(
        engine.logical_focus_cursor,
        Some(WindowId("win-n".to_owned())),
        "repeated same-target held focus must keep matching cursor N"
    );
    let repeated = after_repeat.focus_request.expect("new explicit decision");
    assert_eq!(repeated.target.0, "win-n");
    assert_eq!(repeated.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(repeated.cmd_seq, Some(3));
    assert_eq!(repeated.issued_revision, 6);
    assert_ne!(repeated.id, pending.id);
    // Clear the hold: the observed leftward focus from N reaches C and
    // supersedes with a fresh id, then a windowed probe acts on C.
    let (after_focus, focus_settlements) = run_batched(
        &mut engine,
        &observe(7, live.clone(), "win-c"),
        &[(5, implicit_focus_dir("left"))],
    );
    assert_eq!(focus_settlements.len(), 1);
    assert_not_rejected(&focus_settlements[0].result, "cleared focus probe");
    let next = after_focus.focus_request.clone().expect("new decision");
    assert_eq!(next.target.0, "win-c", "observed leftward probe from N");
    assert_eq!(next.cause, FocusRequestCause::ExplicitCommand);
    assert_ne!(next.id, pending.id, "focus probe supersedes with a new id");
    let toggle = WorldCommand::Implicit(Box::new(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    }));
    let (_, toggle_settlements) = run_batched(
        &mut engine,
        &observe(8, live.clone(), "win-c"),
        &[(6, toggle)],
    );
    assert_eq!(toggle_settlements.len(), 1);
    assert_maximize_on(
        &toggle_settlements[0].result,
        "win-c",
        "cleared windowed probe",
    );
}

/// Held target absent from live windows: explicit N mints while live,
/// then N survives only as a held row while B/C stay live under a known
/// C baseline. Unlike the live-N case above (where the preserve slot
/// restores the cursor because staged homing holds N), here staged
/// homing lacks N, so repeated held reports keep the identical row with
/// an empty cursor and no subject; recovery with N live restores N.
#[test]
fn held_absent_target_retains_row_with_empty_cursor_until_live_recovery() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(
            1,
            vec![fact("win-b", "out-a", 0), fact("win-c", "out-a", 100)],
            "win-b",
        ))
        .expect("admission observes");
    let live = vec![
        fact("win-n", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    let mut held_change = observe(
        2,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
        "win-c",
    );
    held_change.screens[0].complete = false;
    let (changed, _) = run_batched(&mut engine, &held_change, &[]);
    assert!(
        scope_hold_any(&changed),
        "native change under incomplete screen holds; got {:?}",
        changed.scopes
    );
    // Explicit focus on newcomer N mints while N is still live.
    let mut held_mint = report(3, live.clone(), "win-c", Some(1), Some(focus_cmd("win-n")));
    held_mint.screens[0].complete = false;
    let (minted, _) = run_batched(&mut engine, &held_mint, &[]);
    let pending = minted.focus_request.clone().expect("held explicit retains");
    assert_eq!(pending.target.0, "win-n");
    let held_n = || HeldWindowRow {
        window: WindowId("win-n".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let absent = |seq: u64| {
        let mut report = observe(
            4 + seq,
            vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
            "win-c",
        );
        report.screens[0].complete = false;
        report.held_windows = vec![held_n()];
        report
    };
    // Same held baseline with no command: the identical row survives,
    // and with staged homing lacking N the preserve slot cannot restore,
    // so the cursor stays empty while this newer report observes live
    // bound C as the execution subject (option 2).
    let (kept, _) = run_batched(&mut engine, &absent(0), &[]);
    assert_eq!(
        kept.focus_request,
        Some(pending.clone()),
        "held repeat keeps the identical row; got {:?}",
        kept.focus_request
    );
    assert_eq!(
        engine.logical_focus_cursor, None,
        "absent target cannot restore the cursor slot"
    );
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-c".to_owned())),
        "newer report with unavailable pending target acts on live bound observed C"
    );
    // Queued implicit focus under the same held baseline resolves subject C
    // and hits the held scope gate (refusing operation-refused, not
    // focus-unknown) while retaining the identical pending row.
    let (still, focus_settlements) =
        run_batched(&mut engine, &absent(1), &[(2, implicit_focus_dir("left"))]);
    assert_eq!(focus_settlements.len(), 1);
    assert!(
        matches!(
            focus_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::ScopeHeld,
            })
        ),
        "held absent-target focus acts on observed C and hits scope hold; got {:?}",
        focus_settlements[0].result
    );
    assert_eq!(
        still.focus_request,
        Some(pending.clone()),
        "focus refusal retains the identical row; got {:?}",
        still.focus_request
    );
    assert_eq!(engine.logical_focus_cursor, None);
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-c".to_owned()))
    );
    // Queued implicit move likewise resolves subject C and hits the held
    // scope gate with the row retained.
    let (again, move_settlements) =
        run_batched(&mut engine, &absent(2), &[(3, implicit_move_dir("left"))]);
    assert_eq!(move_settlements.len(), 1);
    assert!(
        matches!(
            move_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::ScopeHeld,
            })
        ),
        "held absent-target move acts on observed C and hits scope hold; got {:?}",
        move_settlements[0].result
    );
    assert_eq!(
        again.focus_request,
        Some(pending.clone()),
        "move refusal retains the identical row; got {:?}",
        again.focus_request
    );
    assert_eq!(engine.logical_focus_cursor, None);
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-c".to_owned()))
    );
    // Recovery with N live on the unchanged C baseline: the returning
    // newcomer re-admits, superseding the explicit row with a fresh
    // admission row for the same target, and the subject restores to N.
    let (recovered, _) = run_batched(&mut engine, &observe(7, live.clone(), "win-c"), &[]);
    let readmitted = recovered.focus_request.clone().expect("recovery retains N");
    assert_eq!(readmitted.target.0, "win-n");
    assert_eq!(readmitted.cause, FocusRequestCause::AutomaticAdmission);
    assert_eq!(readmitted.id, "focus-7");
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-n".to_owned())),
        "live N restores the subject on recovery"
    );
    // The observed leftward probe from N reaches C and supersedes.
    let (acted, act_settlements) = run_batched(
        &mut engine,
        &observe(8, live.clone(), "win-c"),
        &[(4, implicit_focus_dir("left"))],
    );
    assert_eq!(act_settlements.len(), 1);
    assert_not_rejected(&act_settlements[0].result, "recovery probe");
    let next = acted.focus_request.clone().expect("new decision");
    assert_eq!(next.target.0, "win-c", "observed leftward probe from N");
    // Target equality retires the superseding row.
    let retired = run_batched(&mut engine, &observe(9, live.clone(), "win-c"), &[]).0;
    assert_eq!(
        retired.focus_request, None,
        "target equality retires; got {:?}",
        retired.focus_request
    );
    // Re-mint N, then closing N retires via target loss.
    let (reminted, _) = run_batched(
        &mut engine,
        &report(10, live.clone(), "win-c", Some(5), Some(focus_cmd("win-n"))),
        &[],
    );
    let pending2 = reminted.focus_request.clone().expect("re-mint retains N");
    assert_eq!(pending2.target.0, "win-n");
    let mut close_n = report(
        11,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
        "win-c",
        None,
        None,
    );
    close_n.closed = vec![WindowId("win-n".to_owned())];
    let closed = run_batched(&mut engine, &close_n, &[]).0;
    assert_eq!(
        closed.focus_request, None,
        "closing the pending target retires; got {:?}",
        closed.focus_request
    );
    // Re-mint N once more, then a genuine user focus change to live B
    // retires via baseline change and B owns the subject.
    let (reminted2, _) = run_batched(
        &mut engine,
        &report(12, live.clone(), "win-c", Some(6), Some(focus_cmd("win-n"))),
        &[],
    );
    assert_eq!(
        reminted2.focus_request.clone().expect("re-mint").target.0,
        "win-n"
    );
    let changed_user = run_batched(&mut engine, &observe(13, live.clone(), "win-b"), &[]).0;
    assert_eq!(
        changed_user.focus_request, None,
        "changed user focus retires; got {:?}",
        changed_user.focus_request
    );
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-b".to_owned())),
        "observed B owns the subject after retirement"
    );
}

/// Option 2 across genuine reports and commands:
/// (a) same-batch implicit command after a held-scope focus mint refuses
///     focus-unknown (no newer report yet);
/// (b) newer report with unavailable N and live bound C acts on C with
///     an actual maximize effect while retaining the exact original N
///     focus request tuple;
/// (c) when N later returns live, the subject follows N and an implicit
///     command acts on N;
/// (e) when native focus reaches N, target equality satisfies and retires
///     the pending request and N owns the subject.
#[test]
fn unavailable_pending_same_batch_refuses_then_newer_report_acts_on_observed_and_recovers() {
    let mut engine = engine();
    let two_screens = || vec![screen("out-a", 0), screen("out-b", 800)];
    let live = vec![
        fact("win-b", "out-a", 0),
        fact("win-c", "out-a", 100),
        fact("win-n", "out-b", 800),
        fact("win-h", "out-b", 900),
    ];
    let mut r1 = observe(1, live.clone(), "win-c");
    r1.screens = two_screens();
    engine
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");

    // (a) Report 2: out-b is held via win-h; explicit Focus(win-n) mints
    // pending_focus for win-n with cursor staying empty (held scope).
    // A same-batch implicit command has no newer report yet, so it refuses
    // focus-unknown.
    let held_h = HeldWindowRow {
        window: WindowId("win-h".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let mut r2 = report(
        2,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-n", "out-b", 800),
        ],
        "win-c",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    r2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r2.held_windows = vec![held_h.clone()];
    let toggle = || {
        implicit(WorldCommand::ToggleMaximize {
            window: WindowId(String::new()),
        })
    };
    let (minted, same_batch_settlements) = run_batched(&mut engine, &r2, &[(2, toggle())]);
    assert_eq!(same_batch_settlements.len(), 2);
    let trailing = same_batch_settlements
        .iter()
        .find(|settlement| settlement.seq == 2)
        .expect("trailing implicit settles");
    assert!(
        matches!(
            trailing.result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "same-batch implicit command after held focus mint must refuse focus-unknown; got {:?}",
        trailing.result
    );
    let pending = minted
        .focus_request
        .clone()
        .expect("held explicit focus retains");
    assert_eq!(pending.target.0, "win-n");
    assert_eq!(pending.issued_revision, 2);
    assert_eq!(pending.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(pending.cmd_seq, Some(1));
    assert_eq!(engine.logical_focus_cursor, None);
    assert_eq!(
        engine.execution_subject(),
        None,
        "immediately after minting report with no newer report yet, subject remains None"
    );

    // (b) Report 3 (newer than request revision 2): win-n is now held-only
    // and unavailable on out-b, while live bound win-c is focused on
    // complete unheld out-a. Implicit ToggleMaximize acts on win-c AND
    // retains the exact original pending tuple for win-n.
    let held_n = HeldWindowRow {
        window: WindowId("win-n".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let held_h2 = HeldWindowRow {
        ordinal: 1,
        ..held_h.clone()
    };
    let mut r3 = observe(
        3,
        vec![fact("win-b", "out-a", 0), fact("win-c", "out-a", 100)],
        "win-c",
    );
    r3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r3.held_windows = vec![held_n, held_h2];
    let expected_c_maximized = StateTarget {
        property: StateProperty::Maximized,
        value: true,
        authority: ScopeAuthority::Controlled,
        command_seq: Some(3),
    };
    let (acted_c, c_settlements) = run_batched(&mut engine, &r3, &[(3, toggle())]);
    assert_eq!(c_settlements.len(), 1);
    assert_maximize_on(
        &c_settlements[0].result,
        "win-c",
        "newer report with unavailable pending N must maximize observed live bound C",
    );
    assert_eq!(
        acted_c.focus_request,
        Some(pending.clone()),
        "non-focus command on observed C must keep exact original pending tuple; got {:?}",
        acted_c.focus_request
    );
    assert!(
        acted_c
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .is_some_and(|row| row.state.contains(&expected_c_maximized)),
        "acted_c must publish win-c Maximized controlled state with command_seq Some(3); got {:?}",
        acted_c.windows
    );
    assert_eq!(engine.logical_focus_cursor, None);
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-c".to_owned()))
    );

    // (c) Report 4: N recovers live on out-b (still with native focus on
    // C). The subject follows N and an implicit maximize acts on N.
    let mut r4 = observe(4, live.clone(), "win-c");
    r4.screens = two_screens();
    let (recovered_n, n_settlements) = run_batched(&mut engine, &r4, &[(4, toggle())]);
    assert_eq!(n_settlements.len(), 1);
    assert_maximize_on(
        &n_settlements[0].result,
        "win-n",
        "when pending N returns live, implicit command must act on N",
    );
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-n".to_owned()))
    );
    assert_eq!(
        recovered_n.focus_request,
        Some(pending.clone()),
        "pending N remains active until focus arrives; got {:?}",
        recovered_n.focus_request
    );
    assert!(
        recovered_n
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .is_some_and(|row| row.state.contains(&expected_c_maximized)),
        "recovered_n must preserve win-c Maximized controlled state with command_seq Some(3); got {:?}",
        recovered_n.windows
    );

    // (e) Report 5: native focus arrives on N -> target equality
    // satisfies and retires pending_focus, and N owns the subject.
    let mut r5 = observe(5, live, "win-n");
    r5.screens = two_screens();
    r5.presentation = PresentationRowV4::Valid(PresentationIngressV4 {
        foreground_out: Some(crate::directional::OutputId("out-b".to_owned())),
        ..ingress("win-n")
    });
    let (completed, final_settlements) = run_batched(&mut engine, &r5, &[(5, toggle())]);
    assert_eq!(completed.focus_request, None, "target equality retires");
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-n".to_owned()))
    );
    assert_eq!(final_settlements.len(), 1);
    assert_maximize_on(
        &final_settlements[0].result,
        "win-n",
        "after normal completion on N, commands continue acting on N",
    );
    assert!(
        completed
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .is_some_and(|row| row.state.contains(&expected_c_maximized)),
        "completed must preserve win-c Maximized controlled state with command_seq Some(3); got {:?}",
        completed.windows
    );
}

/// Newer accepted report with equal `observation_seq` and strictly higher
/// `request_seq` still counts as strictly newer than the minting report
/// (freshness follows accepted report/state revision order, not
/// `observation_seq` alone).
#[test]
fn unavailable_pending_newer_accepted_report_with_equal_observation_seq_acts_on_observed() {
    let mut engine = engine();
    let two_screens = || vec![screen("out-a", 0), screen("out-b", 800)];
    let live = vec![
        fact("win-b", "out-a", 0),
        fact("win-c", "out-a", 100),
        fact("win-n", "out-b", 800),
        fact("win-h", "out-b", 900),
    ];
    let mut r1 = observe(1, live, "win-c");
    r1.screens = two_screens();
    engine
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");

    let held_row = |id: &str, ordinal: usize| HeldWindowRow {
        window: WindowId(id.to_owned()),
        ordinal,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let mut r2 = report(
        2,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-n", "out-b", 800),
        ],
        "win-c",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    r2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r2.held_windows = vec![held_row("win-h", 0)];
    let (minted, _) = run_batched(&mut engine, &r2, &[]);
    let pending = minted.focus_request.clone().expect("pending N retained");
    assert_eq!(pending.issued_revision, 2);

    // Report 3 keeps observation_seq == 2 (same as r2) while advancing
    // request_seq to 3 -> accepted at revision 3 > pending.issued_revision (2).
    let mut r3 = observe(
        3,
        vec![fact("win-b", "out-a", 0), fact("win-c", "out-a", 100)],
        "win-c",
    );
    r3.observation_seq = 2;
    r3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r3.held_windows = vec![held_row("win-n", 0), held_row("win-h", 1)];
    let toggle = implicit(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    });
    let (acted_c, c_settlements) = run_batched(&mut engine, &r3, &[(2, toggle)]);
    assert_eq!(c_settlements.len(), 1);
    assert_maximize_on(
        &c_settlements[0].result,
        "win-c",
        "equal observation_seq with higher request_seq is newer report and must act on C",
    );
    assert_eq!(acted_c.focus_request, Some(pending));
    assert!(
        acted_c
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .is_some_and(|row| row.state.contains(&StateTarget {
                property: StateProperty::Maximized,
                value: true,
                authority: ScopeAuthority::Controlled,
                command_seq: Some(2),
            })),
        "acted_c must publish win-c Maximized controlled state with command_seq Some(2); got {:?}",
        acted_c.windows
    );
}

/// (a) Same-batch refusal also holds when an older unavailable pending
/// request exists from a prior report and a fresh Focus command in the
/// current batch leaves the cursor empty: the same-batch implicit command
/// must refuse focus-unknown rather than falling back via the older row.
#[test]
fn same_batch_focus_decision_blocks_fallback_even_with_older_unavailable_pending() {
    let mut engine = engine();
    let two_screens = || vec![screen("out-a", 0), screen("out-b", 800)];
    let mut r1 = observe(
        1,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-x", "out-b", 800),
            fact("win-n", "out-b", 900),
            fact("win-h", "out-b", 950),
        ],
        "win-c",
    );
    r1.screens = two_screens();
    engine
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");

    let held_row = |id: &str, ordinal: usize| HeldWindowRow {
        window: WindowId(id.to_owned()),
        ordinal,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    // Report 2: mint pending_focus on win-x while out-b is held.
    let mut r2 = report(
        2,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-x", "out-b", 800),
            fact("win-n", "out-b", 900),
        ],
        "win-c",
        Some(1),
        Some(focus_cmd("win-x")),
    );
    r2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r2.held_windows = vec![held_row("win-h", 0)];
    let (minted_x, _) = run_batched(&mut engine, &r2, &[]);
    assert_eq!(
        minted_x.focus_request.as_ref().map(|r| r.target.0.as_str()),
        Some("win-x")
    );

    // Report 3 (newer than win-x): win-x is unavailable, out-b is still
    // held, and Command 1 explicitly focuses live win-n (leaving cursor
    // empty because out-b is held). Command 2 (same batch) is Implicit:
    // must refuse FocusUnknown, never fall back to observed win-c.
    let mut r3 = report(
        3,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-n", "out-b", 900),
        ],
        "win-c",
        Some(2),
        Some(focus_cmd("win-n")),
    );
    r3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r3.held_windows = vec![held_row("win-x", 0), held_row("win-h", 1)];
    let toggle = implicit(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    });
    let (minted_n, settlements) = run_batched(&mut engine, &r3, &[(3, toggle)]);
    assert_eq!(settlements.len(), 2);
    let trailing = settlements
        .iter()
        .find(|settlement| settlement.seq == 3)
        .expect("trailing implicit settles");
    assert!(
        matches!(
            trailing.result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "same-batch implicit after Focus(win-n) must refuse focus-unknown; got {:?}",
        trailing.result
    );
    assert_eq!(
        minted_n.focus_request.as_ref().map(|r| r.target.0.as_str()),
        Some("win-n")
    );
    assert_eq!(engine.logical_focus_cursor, None);
    assert_eq!(engine.execution_subject(), None);
}

/// (d) Newer report with unavailable pending target and unknown or
/// unbound/foreign foreground must refuse focus-unknown while keeping
/// the pending request retained.
#[test]
fn unavailable_pending_newer_unknown_or_unbound_foreground_refuses() {
    let held_row = |id: &str, ordinal: usize| HeldWindowRow {
        window: WindowId(id.to_owned()),
        ordinal,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let toggle = || {
        implicit(WorldCommand::ToggleMaximize {
            window: WindowId(String::new()),
        })
    };

    // Case 1: unknown empty foreground (foreground_known = false).
    let mut engine_unknown = engine();
    let mut u1 = observe(
        1,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-n", "out-b", 800),
            fact("win-h", "out-b", 900),
        ],
        "win-b",
    );
    u1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_unknown
        .handle_report_v4_for_test(&u1)
        .expect("admission observes");
    let mut u2 = report(
        2,
        vec![fact("win-b", "out-a", 0), fact("win-n", "out-b", 800)],
        "",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    u2.foreground_known = false;
    u2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    u2.held_windows = vec![held_row("win-h", 0)];
    let (u_minted, _) = run_batched(&mut engine_unknown, &u2, &[]);
    let u_pending = u_minted.focus_request.clone().expect("pending N retained");
    let mut u3 = observe(3, vec![fact("win-b", "out-a", 0)], "");
    u3.foreground_known = false;
    u3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    u3.held_windows = vec![held_row("win-n", 0), held_row("win-h", 1)];
    let (u_after, u_settlements) = run_batched(&mut engine_unknown, &u3, &[(2, toggle())]);
    assert_eq!(u_settlements.len(), 1);
    assert!(
        matches!(
            u_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unknown foreground must refuse focus-unknown; got {:?}",
        u_settlements[0].result
    );
    assert_eq!(u_after.focus_request, Some(u_pending));
    assert_eq!(engine_unknown.execution_subject(), None);

    // Case 2: unbound/foreign foreground (not present in windows).
    let mut engine_foreign = engine();
    let mut f1 = observe(
        1,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-n", "out-b", 800),
            fact("win-h", "out-b", 900),
        ],
        "win-foreign",
    );
    f1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_foreign
        .handle_report_v4_for_test(&f1)
        .expect("admission observes");
    let mut f2 = report(
        2,
        vec![fact("win-b", "out-a", 0), fact("win-n", "out-b", 800)],
        "win-foreign",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    f2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    f2.held_windows = vec![held_row("win-h", 0)];
    let (f_minted, _) = run_batched(&mut engine_foreign, &f2, &[]);
    let f_pending = f_minted.focus_request.clone().expect("pending N retained");
    let mut f3 = observe(3, vec![fact("win-b", "out-a", 0)], "win-foreign");
    f3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    f3.held_windows = vec![held_row("win-n", 0), held_row("win-h", 1)];
    let (f_after, f_settlements) = run_batched(&mut engine_foreign, &f3, &[(2, toggle())]);
    assert_eq!(f_settlements.len(), 1);
    assert!(
        matches!(
            f_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unbound/foreign foreground must refuse focus-unknown; got {:?}",
        f_settlements[0].result
    );
    assert_eq!(f_after.focus_request, Some(f_pending));
    assert_eq!(engine_foreign.execution_subject(), None);
}

/// Non-`record_command_focus` focus-effect carriers (`settle_send` follow/stay
/// and `move_directional`) must not wipe a valid prior `logical_focus_cursor`
/// when returning an `Effect(Focus)` with a different target.
#[test]
fn send_follow_effect_preserves_command_focus_cursor() {
    let toggle = || {
        implicit(WorldCommand::ToggleMaximize {
            window: WindowId(String::new()),
        })
    };
    let mut engine_follow = engine();
    let mut r1 = observe(
        1,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
    );
    r1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_follow
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");
    let mut r2 = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
        Some(1),
        Some(focus_cmd("win-b")),
    );
    r2.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let send_follow = WorldCommand::SendRelative {
        window: WindowId("win-a".to_owned()),
        source: SendSource::Explicit(OutputId("out-a".to_owned())),
        delta: 1,
        follow: true,
    };
    let (reply_follow, follow_settlements) =
        run_batched(&mut engine_follow, &r2, &[(2, send_follow), (3, toggle())]);
    assert_eq!(follow_settlements.len(), 3);
    assert_not_rejected(&follow_settlements[1].result, "SendRelative(follow=true)");
    assert_maximize_on(
        &follow_settlements[2].result,
        "win-b",
        "SendRelative(follow=true) effect must not clear prior Focus(win-b) cursor",
    );
    assert!(
        reply_follow
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-b")
            .is_some_and(|row| row.state.contains(&StateTarget {
                property: StateProperty::Maximized,
                value: true,
                authority: ScopeAuthority::Controlled,
                command_seq: Some(3),
            })),
        "trailing implicit maximize must publish win-b Maximized; got {:?}",
        reply_follow.windows
    );
    assert_eq!(
        reply_follow
            .focus_request
            .as_ref()
            .map(|req| req.target.0.as_str()),
        Some("win-a"),
        "post-batch drain_focus_candidates retains SendRelative's explicit focus winner"
    );
    assert_eq!(
        engine_follow.logical_focus_cursor,
        Some(WindowId("win-a".to_owned()))
    );
}

#[test]
fn send_stay_effect_preserves_command_focus_cursor() {
    let toggle = || {
        implicit(WorldCommand::ToggleMaximize {
            window: WindowId(String::new()),
        })
    };
    let mut engine_stay = engine();
    let mut r1_stay = observe(
        1,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-b", "out-b", 800),
        ],
        "win-a",
    );
    r1_stay.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_stay
        .handle_report_v4_for_test(&r1_stay)
        .expect("admission observes");
    let mut r2_stay = report(
        2,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-b", "out-b", 800),
        ],
        "win-a",
        Some(1),
        Some(focus_cmd("win-b")),
    );
    r2_stay.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let send_stay = WorldCommand::SendRelative {
        window: WindowId("win-a".to_owned()),
        source: SendSource::Explicit(OutputId("out-a".to_owned())),
        delta: 1,
        follow: false,
    };
    let (reply_stay, stay_settlements) =
        run_batched(&mut engine_stay, &r2_stay, &[(2, send_stay), (3, toggle())]);
    assert_eq!(stay_settlements.len(), 3);
    assert_not_rejected(&stay_settlements[1].result, "SendRelative(follow=false)");
    assert_maximize_on(
        &stay_settlements[2].result,
        "win-b",
        "SendRelative(follow=false) effect must not clear prior Focus(win-b) cursor",
    );
    assert!(
        reply_stay
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-b")
            .is_some_and(|row| row.state.contains(&StateTarget {
                property: StateProperty::Maximized,
                value: true,
                authority: ScopeAuthority::Controlled,
                command_seq: Some(3),
            })),
        "trailing implicit maximize must publish win-b Maximized; got {:?}",
        reply_stay.windows
    );
    assert_eq!(
        reply_stay
            .focus_request
            .as_ref()
            .map(|req| req.target.0.as_str()),
        Some("win-c"),
        "post-batch drain_focus_candidates retains SendRelative's source-survivor winner"
    );
    assert_eq!(
        engine_stay.logical_focus_cursor,
        Some(WindowId("win-c".to_owned()))
    );
}

#[test]
fn move_directional_effect_preserves_command_focus_cursor() {
    let toggle = || {
        implicit(WorldCommand::ToggleMaximize {
            window: WindowId(String::new()),
        })
    };
    let mut engine_move = engine();
    let mut m1 = observe(
        1,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-b", "out-b", 800),
        ],
        "win-a",
    );
    m1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_move
        .handle_report_v4_for_test(&m1)
        .expect("admission observes");
    let mut m2 = report(
        2,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-b", "out-b", 800),
        ],
        "win-a",
        Some(1),
        Some(focus_cmd("win-b")),
    );
    m2.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let move_cmd = WorldCommand::MoveDirectional {
        window: WindowId("win-a".to_owned()),
        direction: "right".to_owned(),
        same_axis: SameAxisMove::SwapWithNeighbor,
    };
    let (reply_move, move_settlements) =
        run_batched(&mut engine_move, &m2, &[(2, move_cmd), (3, toggle())]);
    assert_eq!(move_settlements.len(), 3);
    assert_not_rejected(&move_settlements[1].result, "MoveDirectional");
    assert_maximize_on(
        &move_settlements[2].result,
        "win-b",
        "MoveDirectional effect must not clear prior Focus(win-b) cursor",
    );
    assert!(
        reply_move
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-b")
            .is_some_and(|row| row.state.contains(&StateTarget {
                property: StateProperty::Maximized,
                value: true,
                authority: ScopeAuthority::Controlled,
                command_seq: Some(3),
            })),
        "trailing implicit maximize must publish win-b Maximized; got {:?}",
        reply_move.windows
    );
    assert_eq!(
        reply_move
            .focus_request
            .as_ref()
            .map(|req| req.target.0.as_str()),
        Some("win-a"),
        "post-batch drain_focus_candidates retains MoveDirectional's mover focus winner"
    );
    assert_eq!(
        engine_move.logical_focus_cursor,
        Some(WindowId("win-a".to_owned()))
    );
}

/// Failed canonical command focus (`record_command_focus` returning false
/// because the target scope is held) still clears a valid prior
/// `logical_focus_cursor`, so a trailing same-batch implicit command refuses
/// `FocusUnknown` instead of acting on the stale cursor.
#[test]
fn failed_record_command_focus_clears_prior_cursor() {
    let mut engine = engine();
    let mut r1 = observe(
        1,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-n", "out-b", 800),
            fact("win-h", "out-b", 900),
        ],
        "win-b",
    );
    r1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");

    let mut r2 = report(
        2,
        vec![fact("win-b", "out-a", 0), fact("win-n", "out-b", 800)],
        "win-b",
        Some(1),
        Some(focus_cmd("win-b")),
    );
    r2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r2.held_windows = vec![HeldWindowRow {
        window: WindowId("win-h".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let toggle = implicit(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    });
    let (reply, settlements) =
        run_batched(&mut engine, &r2, &[(2, focus_cmd("win-n")), (3, toggle)]);
    assert_eq!(settlements.len(), 3);
    assert!(
        matches!(
            settlements[2].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "failed record_command_focus must clear prior cursor so trailing implicit refuses; got {:?}",
        settlements[2].result
    );
    assert_eq!(
        reply
            .focus_request
            .as_ref()
            .map(|req| req.target.0.as_str()),
        Some("win-n")
    );
    assert_eq!(engine.logical_focus_cursor, None);
    assert_eq!(engine.execution_subject(), None);
}
