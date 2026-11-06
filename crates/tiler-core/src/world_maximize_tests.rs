//! Native maximize production through the real `ToggleMaximize`
//! producer: ordinary binding, fullscreen replacement pairing,
//! exitability/unknown/cap refusals. Production mints exact typed
//! [`NativeMaximizeMeta`](crate::world_state::NativeMaximizeMeta)
//! carriers; publication below asserts the central paired-clear decorator.

use crate::connection::HostCapabilities;
use crate::directional::WindowId;
use crate::world_directional::tests::{accepted, fact, report_full, screen};
use crate::world_engine::{LegacyWorldOutcome, WorldEngine};
use crate::world_report::{
    AttemptOutcome, CommandRejectKind, CommandResult, FrameSetter, ScopeAuthority, StateAttempt,
    StateProperty, WorldCommand, WorldReport,
};
use crate::world_state::MAX_RETAINED_STATE_BINDINGS;

fn max_caps() -> HostCapabilities {
    HostCapabilities {
        maximize: true,
        fullscreen: true,
        native_sticky: true,
        ..Default::default()
    }
}

fn max_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", max_caps())
        .expect("generation")
}

fn max_fact(
    window: &str,
    exitable: Option<bool>,
    fullscreen: bool,
    maximized: bool,
) -> crate::world_report::WindowFact {
    let mut row = fact(window, "out-a", 0);
    row.fullscreen = Some(fullscreen);
    row.fullscreen_exitable = exitable;
    row.maximized = Some(maximized);
    row
}

fn bootstrap(windows: Vec<crate::world_report::WindowFact>) -> WorldReport {
    report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "win-a",
        None,
        None,
    )
}

fn cmd(
    seq: u64,
    command_seq: u64,
    windows: Vec<crate::world_report::WindowFact>,
    command: WorldCommand,
) -> WorldReport {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "win-a",
        Some(command_seq),
        Some(command),
    )
}

fn toggle(window: &str) -> WorldCommand {
    WorldCommand::ToggleMaximize {
        window: WindowId(window.to_owned()),
    }
}

fn full_attempt(
    window: &str,
    target: bool,
    revision: u64,
    setter: FrameSetter,
    pre: Option<bool>,
    post: Option<bool>,
) -> StateAttempt {
    StateAttempt {
        window: WindowId(window.to_owned()),
        property: StateProperty::Fullscreen,
        target,
        revision,
        setter,
        pre,
        post,
        attempt_ms: 1,
    }
}

fn set_fullscreen(window: &str, value: bool) -> WorldCommand {
    WorldCommand::SetFullscreen {
        window: WindowId(window.to_owned()),
        value,
    }
}

fn observe(seq: u64, windows: Vec<crate::world_report::WindowFact>) -> WorldReport {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "win-a",
        None,
        None,
    )
}

fn max_row(
    reply: &crate::world_report::WorldReply,
    window: &str,
) -> Option<(bool, ScopeAuthority, Option<u64>)> {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime == WindowId(window.to_owned()))
        .and_then(|row| {
            row.state
                .iter()
                .find(|target| target.property == StateProperty::Maximized)
                .map(|target| (target.value, target.authority, target.command_seq))
        })
}

fn full_row(
    reply: &crate::world_report::WorldReply,
    window: &str,
) -> Option<(bool, ScopeAuthority, Option<u64>)> {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime == WindowId(window.to_owned()))
        .and_then(|row| {
            row.state
                .iter()
                .find(|target| target.property == StateProperty::Fullscreen)
                .map(|target| (target.value, target.authority, target.command_seq))
        })
}

fn max_attempt(
    window: &str,
    target: bool,
    revision: u64,
    setter: FrameSetter,
    pre: Option<bool>,
    post: Option<bool>,
) -> StateAttempt {
    StateAttempt {
        window: WindowId(window.to_owned()),
        property: StateProperty::Maximized,
        target,
        revision,
        setter,
        pre,
        post,
        attempt_ms: 1,
    }
}

#[test]
fn ordinary_max_mints_binding_and_revokes_stale_fullscreen_entry() {
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", None, false, false)]),
    );
    engine.retained_states.insert(
        (WindowId("win-a".to_owned()), StateProperty::Fullscreen),
        crate::world_state::RetainedStateBinding {
            target: crate::world_state::RetainedStateTarget {
                window: WindowId("win-a".to_owned()),
                property: StateProperty::Fullscreen,
                value: true,
                revision: 1,
            },
            pending: None,
            spent: false,
            disposition: AttemptOutcome::Unattempted,
            sticky_meta: None,
            fullscreen_meta: None,
            maximize_meta: None,
            above_meta: None,
        },
    );
    let rev_before = engine.state_revision;
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", None, false, false)],
            toggle("win-a"),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Maximize {
            maximized,
            clear_fullscreen,
            was_maximized,
            ..
        }) => {
            assert_eq!(
                (*maximized, *clear_fullscreen, *was_maximized),
                (true, false, false)
            );
        }
        other => panic!("expected maximize result, got {other:?}"),
    }
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "stale fullscreen entry revoked by explicit maximize"
    );
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("maximized binding");
    assert!(binding.target.value);
    assert_eq!(binding.target.revision, rev_before + 1);
    let meta = binding.maximize_meta.as_ref().expect("typed meta");
    assert_eq!(meta.command_seq, 1);
    assert!(!meta.clear_fullscreen);
}

#[test]
fn fullscreen_replacement_pairs_same_rev_seq() {
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
    );
    let rev_before = engine.state_revision;
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), true, false)],
            toggle("win-a"),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Maximize {
            maximized,
            clear_fullscreen,
            was_maximized,
            ..
        }) => {
            assert_eq!(
                (*maximized, *clear_fullscreen, *was_maximized),
                (true, true, false)
            );
        }
        other => panic!("expected maximize result, got {other:?}"),
    }
    let full = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg");
    let max = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("maximize leg");
    assert!(!full.target.value);
    assert!(max.target.value);
    assert_eq!(full.target.revision, rev_before + 1);
    assert_eq!(max.target.revision, full.target.revision, "same revision");
    assert_eq!(
        full.fullscreen_meta
            .as_ref()
            .expect("full meta")
            .command_seq,
        1,
    );
    let max_meta = max.maximize_meta.as_ref().expect("max meta");
    assert_eq!(max_meta.command_seq, 1, "same command sequence");
    assert!(max_meta.clear_fullscreen);
}

#[test]
fn both_true_mints_only_fullscreen_leg_and_cancels_obsolete_max() {
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, true)]),
    );
    engine.retained_states.insert(
        (WindowId("win-a".to_owned()), StateProperty::Maximized),
        crate::world_state::RetainedStateBinding {
            target: crate::world_state::RetainedStateTarget {
                window: WindowId("win-a".to_owned()),
                property: StateProperty::Maximized,
                value: false,
                revision: 1,
            },
            pending: None,
            spent: false,
            disposition: AttemptOutcome::Unattempted,
            sticky_meta: None,
            fullscreen_meta: None,
            maximize_meta: None,
            above_meta: None,
        },
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), true, true)],
            toggle("win-a"),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Maximize {
            maximized,
            clear_fullscreen,
            was_maximized,
            ..
        }) => {
            assert_eq!(
                (*maximized, *clear_fullscreen, *was_maximized),
                (true, true, true)
            );
        }
        other => panic!("expected maximize result, got {other:?}"),
    }
    let full = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg");
    assert!(!full.target.value);
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Maximized)),
        "obsolete opposite maximize binding cancelled, no extra maximize write"
    );
}

#[test]
fn unexitable_or_unknown_exitability_rejects_without_mutation() {
    for (exitable, kind) in [
        (Some(false), CommandRejectKind::FullscreenNotExitable),
        (None, CommandRejectKind::StateUnknown),
    ] {
        let mut engine = max_engine();
        accepted(
            &mut engine,
            &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
        );
        let reply = accepted(
            &mut engine,
            &cmd(
                2,
                1,
                vec![max_fact("win-a", exitable, true, false)],
                toggle("win-a"),
            ),
        );
        assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected { kind }),
            "exitable {exitable:?}"
        );
        assert!(
            engine.retained_states.is_empty(),
            "no binding minted on exitability refusal"
        );
    }
}

#[test]
fn unknown_maximized_home_unreadable_refuses_without_mutation() {
    // Unknown maximized holds the home scope unreadable, so the shared
    // readability gate refuses before any unknown mapping: no mutation,
    // no binding, no typed result.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", None, false, false)]),
    );
    let mut row = max_fact("win-a", None, false, false);
    row.maximized = None;
    let revision = engine.state_revision;
    match engine.handle_report_legacy(&cmd(2, 1, vec![row], toggle("win-a"))) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected readability refusal"
        ),
        other => panic!("expected readability refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "refusal issues");
    assert!(engine.retained_states.is_empty());
}

#[test]
fn dual_pair_cap_refusal_mints_neither_leg() {
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
    );
    let need = MAX_RETAINED_STATE_BINDINGS - engine.retained_states.len();
    for i in 0..need {
        engine.retained_states.insert(
            (WindowId(format!("win-fill-{i:05}")), StateProperty::Above),
            crate::world_state::RetainedStateBinding {
                target: crate::world_state::RetainedStateTarget {
                    window: WindowId(format!("win-fill-{i:05}")),
                    property: StateProperty::Above,
                    value: true,
                    revision: 1,
                },
                pending: None,
                spent: false,
                disposition: AttemptOutcome::Unattempted,
                sticky_meta: None,
                fullscreen_meta: None,
                maximize_meta: None,
                above_meta: None,
            },
        );
    }
    assert_eq!(
        engine.retained_states.len(),
        MAX_RETAINED_STATE_BINDINGS,
        "exact bound before dual attempt"
    );
    let revision = engine.state_revision;
    match engine.handle_report_legacy(&cmd(
        3,
        2,
        vec![max_fact("win-a", Some(true), true, false)],
        toggle("win-a"),
    )) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected cap refusal"
        ),
        other => panic!("expected cap refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "refusal issues");
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Maximized)),
        "no partial maximize leg past the cap"
    );
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "no partial fullscreen leg past the cap"
    );
}

fn fill_above(engine: &mut WorldEngine, prefix: &str, count: usize) {
    for i in 0..count {
        engine.retained_states.insert(
            (WindowId(format!("{prefix}-{i:05}")), StateProperty::Above),
            crate::world_state::RetainedStateBinding {
                target: crate::world_state::RetainedStateTarget {
                    window: WindowId(format!("{prefix}-{i:05}")),
                    property: StateProperty::Above,
                    value: true,
                    revision: 1,
                },
                pending: None,
                spent: false,
                disposition: AttemptOutcome::Unattempted,
                sticky_meta: None,
                fullscreen_meta: None,
                maximize_meta: None,
                above_meta: None,
            },
        );
    }
}

#[test]
fn both_true_at_cap_refuses_without_mutation() {
    // Both-true still inserts the fullscreen-false leg, so a full map
    // with neither subject key present must refuse: no 40001st binding.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, true)]),
    );
    let fill = MAX_RETAINED_STATE_BINDINGS - engine.retained_states.len();
    fill_above(&mut engine, "win-fill", fill);
    assert_eq!(
        engine.retained_states.len(),
        MAX_RETAINED_STATE_BINDINGS,
        "exact bound before both-true attempt"
    );
    let revision = engine.state_revision;
    let refused_obs = cmd(
        3,
        2,
        vec![max_fact("win-a", Some(true), true, true)],
        toggle("win-a"),
    );
    // Commandless control with the same observation: the refusal must
    // converge identically apart from the consumed sequence and typed
    // result. Synthetic filler windows are not observation-live, so the
    // adoption path legitimately prunes them on either path.
    let mut control = engine.clone();
    let mut control_obs = refused_obs.clone();
    control_obs.command = None;
    control_obs.command_seq = None;
    let _ = control.handle_report_legacy(&control_obs);
    match engine.handle_report_legacy(&refused_obs) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected cap refusal"
        ),
        other => panic!("expected cap refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "refusal issues");
    assert_eq!(
        engine.retained_states, control.retained_states,
        "refusal converges with the commandless control"
    );
    assert_eq!(
        engine.retained_states.len(),
        control.retained_states.len(),
        "no insertion past the cap"
    );
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "no fullscreen leg past the cap"
    );
}

#[test]
fn dual_pair_one_vacant_slot_refuses_with_no_partial_pair() {
    // Dual replacement needs two new keys; one vacant slot still refuses
    // with neither leg minted.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
    );
    let fill = MAX_RETAINED_STATE_BINDINGS - engine.retained_states.len() - 1;
    fill_above(&mut engine, "win-fill", fill);
    assert_eq!(
        engine.retained_states.len(),
        MAX_RETAINED_STATE_BINDINGS - 1,
        "one vacant slot before dual attempt"
    );
    let revision = engine.state_revision;
    match engine.handle_report_legacy(&cmd(
        3,
        2,
        vec![max_fact("win-a", Some(true), true, false)],
        toggle("win-a"),
    )) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected cap refusal"
        ),
        other => panic!("expected cap refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision + 1, "refusal issues");
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Maximized)),
        "no partial maximize leg with one slot"
    );
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "no partial fullscreen leg with one slot"
    );
}

#[test]
fn exact_cap_existing_both_keys_replacement_valid() {
    // Both subject keys already present at the exact cap: the dual
    // replacement overwrites in place instead of refusing.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), true, false)],
            toggle("win-a"),
        ),
    );
    let fill = MAX_RETAINED_STATE_BINDINGS - engine.retained_states.len();
    fill_above(&mut engine, "win-fill", fill);
    assert_eq!(
        engine.retained_states.len(),
        MAX_RETAINED_STATE_BINDINGS,
        "exact bound with both subject keys present"
    );
    let rev_before = engine.state_revision;
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![max_fact("win-a", Some(true), true, false)],
            toggle("win-a"),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Maximize {
            maximized,
            clear_fullscreen,
            ..
        }) => {
            assert_eq!((*maximized, *clear_fullscreen), (true, true));
        }
        other => panic!("expected maximize result, got {other:?}"),
    }
    let full = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg");
    let max = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("maximize leg");
    assert!(max.target.value);
    assert_eq!(full.target.revision, rev_before + 1);
    assert_eq!(max.target.revision, full.target.revision, "same revision");
}

#[test]
fn ordinary_max_and_unmax_publish_controlled() {
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", None, false, false)]),
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", None, false, false)],
            toggle("win-a"),
        ),
    );
    let (value, authority, seq) = max_row(&reply, "win-a").expect("maximize target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert!(full_row(&reply, "win-a").is_none());
    let unmax = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![max_fact("win-a", None, false, true)],
            toggle("win-a"),
        ),
    );
    let (value, authority, seq) = max_row(&unmax, "win-a").expect("unmaximize target");
    assert_eq!((value, seq), (false, Some(2)));
    assert_eq!(authority, ScopeAuthority::Controlled);
}

#[test]
fn dual_pair_both_controlled_same_rev_seq() {
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), true, false)],
            toggle("win-a"),
        ),
    );
    let (full_value, full_authority, full_seq) = full_row(&reply, "win-a").expect("clear target");
    assert_eq!((full_value, full_seq), (false, Some(1)));
    assert_eq!(full_authority, ScopeAuthority::Controlled);
    let (max_value, max_authority, max_seq) = max_row(&reply, "win-a").expect("max target");
    assert_eq!((max_value, max_seq), (true, Some(1)));
    assert_eq!(max_authority, ScopeAuthority::Controlled);
    let full = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("clear leg");
    let max = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg");
    assert_eq!(max.target.revision, full.target.revision);
    assert_eq!(
        max.maximize_meta.as_ref().expect("meta").command_seq,
        full.fullscreen_meta.as_ref().expect("meta").command_seq
    );
}

#[test]
fn raw_both_true_with_only_clear_binding_publishes_no_max_row() {
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, true)]),
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), true, true)],
            toggle("win-a"),
        ),
    );
    let (value, authority, seq) = full_row(&reply, "win-a").expect("clear target");
    assert_eq!((value, seq), (false, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert!(
        max_row(&reply, "win-a").is_none(),
        "both-true mints only the fullscreen leg, no extra MAX row"
    );
}

#[test]
fn exitability_loss_holds_max_then_recovery_resumes_tuple() {
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), true, false)],
            toggle("win-a"),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    for (seq, exitable) in [(3, Some(false)), (4, None)] {
        let held = accepted(
            &mut engine,
            &observe(seq, vec![max_fact("win-a", exitable, true, false)]),
        );
        let (_, full_authority, _) = full_row(&held, "win-a").expect("clear target");
        assert!(
            !matches!(full_authority, ScopeAuthority::Controlled),
            "exitable {exitable:?} must hold the clear leg"
        );
        let (value, authority, seq_out) = max_row(&held, "win-a").expect("max target");
        assert!(value);
        assert!(
            !matches!(authority, ScopeAuthority::Controlled),
            "lost clear authority must hold max, got {authority:?}"
        );
        assert_eq!(seq_out, Some(1));
    }
    let resumed = accepted(
        &mut engine,
        &observe(5, vec![max_fact("win-a", Some(true), true, false)]),
    );
    let (_, full_authority, _) = full_row(&resumed, "win-a").expect("clear");
    assert_eq!(full_authority, ScopeAuthority::Controlled);
    let (value, authority, seq) = max_row(&resumed, "win-a").expect("max");
    assert!(value);
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert_eq!(seq, Some(1));
    assert_eq!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
            .expect("max leg")
            .target
            .revision,
        rev
    );
}

#[test]
fn superseded_mismatched_and_spent_clear_hold_max_while_raw_full() {
    // Superseded: a later fullscreen exit re-mints the clear leg with a new
    // revision and sequence; the stale max tuple no longer matches.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), true, false)],
            toggle("win-a"),
        ),
    );
    let stale_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    let reauth = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![max_fact("win-a", Some(true), true, false)],
            WorldCommand::SetFullscreen {
                window: WindowId("win-a".to_owned()),
                value: false,
            },
        ),
    );
    let (_, full_authority, full_seq) = full_row(&reauth, "win-a").expect("clear");
    assert_eq!(full_authority, ScopeAuthority::Controlled);
    assert_eq!(full_seq, Some(2));
    let (value, authority, seq) = max_row(&reauth, "win-a").expect("max");
    assert!(value);
    assert!(
        !matches!(authority, ScopeAuthority::Controlled),
        "superseded clear must hold stale max"
    );
    assert_eq!(seq, Some(1));
    assert_eq!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
            .expect("max leg")
            .target
            .revision,
        stale_rev
    );
    // Mismatched: same revision shape but a foreign command sequence on the
    // clear leg never authorizes the max leg.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), true, false)],
            toggle("win-a"),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    engine
        .retained_states
        .get_mut(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("clear leg")
        .fullscreen_meta
        .as_mut()
        .expect("meta")
        .command_seq = 99;
    let held = accepted(
        &mut engine,
        &observe(3, vec![max_fact("win-a", Some(true), true, false)]),
    );
    let (value, authority, _) = max_row(&held, "win-a").expect("max");
    assert!(value);
    assert!(
        !matches!(authority, ScopeAuthority::Controlled),
        "sequence-mismatched clear must hold max"
    );
    assert_eq!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
            .expect("max leg")
            .target
            .revision,
        rev
    );
    // Spent-failed clear: an ambiguous throw on the clear leg latches it
    // spent, its row leaves Controlled, and the paired max holds too.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), true, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), true, false)],
            toggle("win-a"),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("clear leg")
        .target
        .revision;
    let mut threw = observe(3, vec![max_fact("win-a", Some(true), true, false)]);
    threw.state_attempts = vec![StateAttempt {
        window: WindowId("win-a".to_owned()),
        property: StateProperty::Fullscreen,
        target: false,
        revision: rev,
        setter: FrameSetter::Threw,
        pre: Some(true),
        post: Some(true),
        attempt_ms: 1,
    }];
    let settled = accepted(&mut engine, &threw);
    let (_, full_authority, _) = full_row(&settled, "win-a").expect("clear");
    assert!(!matches!(full_authority, ScopeAuthority::Controlled));
    let (value, authority, seq) = max_row(&settled, "win-a").expect("max");
    assert!(value);
    assert!(
        !matches!(authority, ScopeAuthority::Controlled),
        "spent-failed clear must hold max while raw fullscreen stays true"
    );
    assert_eq!(seq, Some(1));
}

#[test]
fn matched_max_attempt_settles_without_renewal() {
    // Applied: exact accepted proof with the live true fact settles spent.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", None, false, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", None, false, false)],
            toggle("win-a"),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    let mut proof = observe(3, vec![max_fact("win-a", None, false, true)]);
    proof.state_attempts = vec![max_attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    )];
    let settled = accepted(&mut engine, &proof);
    let after = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("retained");
    assert!(after.spent);
    assert_eq!(after.disposition, AttemptOutcome::Applied);
    let (_, authority, seq) = max_row(&settled, "win-a").expect("max");
    assert!(!matches!(authority, ScopeAuthority::Controlled));
    assert_eq!(seq, Some(1));
    // RefusedProven: explicit refusal with unchanged readable preimage.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", None, false, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", None, false, false)],
            toggle("win-a"),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    let mut refused = observe(3, vec![max_fact("win-a", None, false, false)]);
    refused.state_attempts = vec![max_attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Refused,
        Some(false),
        Some(false),
    )];
    accepted(&mut engine, &refused);
    let stuck = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("binding");
    assert!(stuck.spent);
    assert_eq!(stuck.disposition, AttemptOutcome::RefusedProven);
    // Ambiguous: a throw latches PossiblyExecuted, still spent, no retry.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", None, false, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", None, false, false)],
            toggle("win-a"),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    let mut ambiguous = observe(3, vec![max_fact("win-a", None, false, false)]);
    ambiguous.state_attempts = vec![max_attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Threw,
        Some(false),
        Some(false),
    )];
    let reply = accepted(&mut engine, &ambiguous);
    let stuck = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("binding");
    assert!(stuck.spent);
    assert_eq!(stuck.disposition, AttemptOutcome::PossiblyExecuted);
    let (value, authority, seq) = max_row(&reply, "win-a").expect("max");
    assert!(value);
    assert!(!matches!(authority, ScopeAuthority::Controlled));
    assert_eq!(seq, Some(1));
}

#[test]
fn fences_hold_max_without_renewal() {
    // Pending same-window transfer fences the unspent max tuple to Hold.
    let mut engine = max_engine();
    let boot = accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", None, false, false)]),
    );
    let mut ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    ring.sort();
    assert!(ring.len() >= 2, "single screen still rings: {ring:?}");
    let minted = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", None, false, false)],
            toggle("win-a"),
        ),
    );
    let (_, authority, _) = max_row(&minted, "win-a").expect("max");
    assert_eq!(authority, ScopeAuthority::Controlled);
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    let sent = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![max_fact("win-a", None, false, false)],
            vec![],
            "win-a",
            Some(2),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: crate::directional::OutputId("out-a".to_owned()),
                target_workspace: ring[1].clone(),
                follow: false,
            }),
        ),
    );
    let (value, authority, seq) = max_row(&sent, "win-a").expect("max");
    assert!(value);
    assert!(
        !matches!(authority, ScopeAuthority::Controlled),
        "pending transfer must hold max"
    );
    assert_eq!(seq, Some(1));
    assert_eq!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
            .expect("max leg")
            .target
            .revision,
        rev
    );
    // Held scope fences a healthy unspent tuple to Hold.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![
            max_fact("win-a", None, false, false),
            fact("win-b", "out-a", 400),
        ]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![
                max_fact("win-a", None, false, false),
                fact("win-b", "out-a", 400),
            ],
            toggle("win-a"),
        ),
    );
    let mut held = observe(3, vec![max_fact("win-a", None, false, false)]);
    held.held_windows = vec![crate::world_report::HeldWindowRow {
        window: WindowId("win-b".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: crate::world_report::RowDiagnosticKind::InvalidFact,
    }];
    let held_reply = accepted(&mut engine, &held);
    let (_, authority, seq) = max_row(&held_reply, "win-a").expect("max");
    assert!(!matches!(authority, ScopeAuthority::Controlled));
    assert_eq!(seq, Some(1));
    // Minimized fact fences the same tuple to a minimized Hold.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", None, false, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", None, false, false)],
            toggle("win-a"),
        ),
    );
    let mut minimized = max_fact("win-a", None, false, false);
    minimized.minimized = Some(true);
    let reply = accepted(&mut engine, &observe(3, vec![minimized]));
    let (_, authority, seq) = max_row(&reply, "win-a").expect("max");
    assert!(matches!(authority, ScopeAuthority::Hold(_)));
    assert_eq!(seq, Some(1));
}

#[test]
fn unmax_target_holds_while_raw_fullscreen_true_then_resumes() {
    // Fullscreen takes precedence: an outstanding unmaximize (target
    // false) must Hold while raw fullscreen reads true, and resume the
    // same unspent tuple once raw fullscreen clears.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", None, false, true)]),
    );
    let minted = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", None, false, true)],
            toggle("win-a"),
        ),
    );
    let (value, authority, seq) = max_row(&minted, "win-a").expect("unmax target");
    assert_eq!((value, seq), (false, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    let held = accepted(
        &mut engine,
        &observe(3, vec![max_fact("win-a", Some(true), true, true)]),
    );
    let (value, authority, seq) = max_row(&held, "win-a").expect("unmax target");
    assert_eq!((value, seq), (false, Some(1)));
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "raw fullscreen true must hold even an unmaximize target, got {authority:?}"
    );
    let resumed = accepted(
        &mut engine,
        &observe(4, vec![max_fact("win-a", None, false, true)]),
    );
    let (value, authority, seq) = max_row(&resumed, "win-a").expect("unmax target");
    assert_eq!((value, seq), (false, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg");
    assert_eq!(binding.target.revision, rev);
    assert!(!binding.spent);
}

#[test]
fn newer_fullscreen_on_holds_pending_max_true_before_raw_flips() {
    // Pending maximize-true (unattempted) held while a newer fullscreen-on
    // is pending even though raw facts are unchanged, then through active,
    // then resumes the exact tuple after the fullscreen exit settles.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), false, false)]),
    );
    let minted = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), false, false)],
            toggle("win-a"),
        ),
    );
    let (value, authority, seq) = max_row(&minted, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let max_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    // Newer fullscreen-on with raw facts unchanged: no maximize attempt,
    // no changed flags.
    let pending = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![max_fact("win-a", Some(true), false, false)],
            set_fullscreen("win-a", true),
        ),
    );
    assert!(pending.command_result.is_none());
    let full_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg")
        .target
        .revision;
    assert!(
        full_rev > max_rev,
        "fullscreen-on must be newer than the pending maximize"
    );
    let (full_value, full_authority, full_seq) =
        full_row(&pending, "win-a").expect("fullscreen target");
    assert_eq!((full_value, full_seq), (true, Some(2)));
    assert_eq!(full_authority, ScopeAuthority::Controlled);
    let (value, authority, seq) = max_row(&pending, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "newer pending fullscreen-on must hold maximize-true before raw flips, got {authority:?}"
    );
    assert!(
        !(full_authority == ScopeAuthority::Controlled && authority == ScopeAuthority::Controlled),
        "no same-reply Fullscreen=true + Maximized write authority"
    );
    let max_binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg");
    assert_eq!(max_binding.target.revision, max_rev);
    assert!(!max_binding.spent);
    assert_eq!(max_binding.disposition, AttemptOutcome::Unattempted);
    // Fullscreen entry proves active: correlated accepted attempt, raw flips.
    let mut entry = observe(4, vec![max_fact("win-a", Some(true), true, false)]);
    entry.state_attempts = vec![full_attempt(
        "win-a",
        true,
        full_rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    )];
    let active = accepted(&mut engine, &entry);
    let (value, authority, seq) = max_row(&active, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "active fullscreen must keep holding maximize-true, got {authority:?}"
    );
    assert_eq!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
            .expect("max leg")
            .target
            .revision,
        max_rev
    );
    // Fullscreen exit mints a fresh false leg; maximize stays held on raw true.
    let exiting = accepted(
        &mut engine,
        &cmd(
            5,
            3,
            vec![max_fact("win-a", Some(true), true, false)],
            set_fullscreen("win-a", false),
        ),
    );
    let exit_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("exit leg")
        .target
        .revision;
    let (value, authority, seq) = max_row(&exiting, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "maximize-true must stay held until the exit proves, got {authority:?}"
    );
    // Exit proof with raw cleared: original maximize tuple resumes Controlled.
    let mut cleared = observe(6, vec![max_fact("win-a", Some(true), false, false)]);
    cleared.state_attempts = vec![full_attempt(
        "win-a",
        false,
        exit_rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let resumed = accepted(&mut engine, &cleared);
    let (value, authority, seq) = max_row(&resumed, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let max_binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg");
    assert_eq!(max_binding.target.revision, max_rev);
    assert!(!max_binding.spent);
    assert_eq!(max_binding.disposition, AttemptOutcome::Unattempted);
}

#[test]
fn newer_fullscreen_on_holds_pending_max_false_before_raw_flips() {
    // Pending unmaximize (target false) held while a newer fullscreen-on is
    // pending with raw facts unchanged, then resumes the exact tuple after
    // the fullscreen exit settles.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), false, true)]),
    );
    let minted = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), false, true)],
            toggle("win-a"),
        ),
    );
    let (value, authority, seq) = max_row(&minted, "win-a").expect("unmax target");
    assert_eq!((value, seq), (false, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let max_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    let pending = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![max_fact("win-a", Some(true), false, true)],
            set_fullscreen("win-a", true),
        ),
    );
    assert!(pending.command_result.is_none());
    let full_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg")
        .target
        .revision;
    assert!(
        full_rev > max_rev,
        "fullscreen-on must be newer than the pending unmaximize"
    );
    let (full_value, full_authority, full_seq) =
        full_row(&pending, "win-a").expect("fullscreen target");
    assert_eq!((full_value, full_seq), (true, Some(2)));
    assert_eq!(full_authority, ScopeAuthority::Controlled);
    let (value, authority, seq) = max_row(&pending, "win-a").expect("unmax target");
    assert_eq!((value, seq), (false, Some(1)));
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "newer pending fullscreen-on must hold unmaximize before raw flips, got {authority:?}"
    );
    let mut entry = observe(4, vec![max_fact("win-a", Some(true), true, true)]);
    entry.state_attempts = vec![full_attempt(
        "win-a",
        true,
        full_rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    )];
    let active = accepted(&mut engine, &entry);
    let (value, authority, _) = max_row(&active, "win-a").expect("unmax target");
    assert!(!value);
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "active fullscreen must keep holding unmaximize, got {authority:?}"
    );
    let exiting = accepted(
        &mut engine,
        &cmd(
            5,
            3,
            vec![max_fact("win-a", Some(true), true, true)],
            set_fullscreen("win-a", false),
        ),
    );
    let exit_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("exit leg")
        .target
        .revision;
    let (value, authority, seq) = max_row(&exiting, "win-a").expect("unmax target");
    assert_eq!((value, seq), (false, Some(1)));
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "unmaximize must stay held until the exit proves, got {authority:?}"
    );
    let mut cleared = observe(6, vec![max_fact("win-a", Some(true), false, true)]);
    cleared.state_attempts = vec![full_attempt(
        "win-a",
        false,
        exit_rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let resumed = accepted(&mut engine, &cleared);
    let (value, authority, seq) = max_row(&resumed, "win-a").expect("unmax target");
    assert_eq!((value, seq), (false, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let max_binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg");
    assert_eq!(max_binding.target.revision, max_rev);
    assert!(!max_binding.spent);
    assert_eq!(max_binding.disposition, AttemptOutcome::Unattempted);
}

#[test]
fn ambiguous_fullscreen_on_releases_max_on_first_readable_false() {
    // User 2026-11-03: an ambiguous (PossiblyExecuted) fullscreen-on attempt
    // releases the held maximize tuple at the first readable fact report
    // that still shows fullscreen=false -- including the attempt-carrying
    // report itself, whose observations do not predate the immediate
    // readback. No fullscreen retry, never reauthorize the spent target.
    // Phase 1 pins the same-report release; phase 2 pins that unreadable
    // attempt facts hold (no readable post-attempt observation yet, no
    // bypass) and the first later readable-false report recovers.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), false, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), false, false)],
            toggle("win-a"),
        ),
    );
    let max_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    let pending = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![max_fact("win-a", Some(true), false, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let full_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg")
        .target
        .revision;
    assert!(full_rev > max_rev);
    let (full_value, full_authority, _) = full_row(&pending, "win-a").expect("fullscreen target");
    assert_eq!(
        (full_value, full_authority),
        (true, ScopeAuthority::Controlled)
    );
    let (value, authority, seq) = max_row(&pending, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "newer pending fullscreen-on must hold maximize-true, got {authority:?}"
    );
    // Same-report readable still-false reconciles: the attempt-carrying
    // report is already the first post-attempt fact report.
    let mut threw = observe(4, vec![max_fact("win-a", Some(true), false, false)]);
    threw.state_attempts = vec![full_attempt(
        "win-a",
        true,
        full_rev,
        FrameSetter::Threw,
        Some(false),
        Some(false),
    )];
    let settled = accepted(&mut engine, &threw);
    let full_binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg");
    assert!(full_binding.spent);
    assert_eq!(full_binding.disposition, AttemptOutcome::PossiblyExecuted);
    assert!(full_binding.pending.is_some());
    let (full_value, full_authority, full_seq) = full_row(&settled, "win-a").expect("clear target");
    assert_eq!((full_value, full_seq), (true, Some(2)));
    assert!(
        !matches!(full_authority, ScopeAuthority::Controlled),
        "spent fullscreen never reauthorizes"
    );
    let (value, authority, seq) = max_row(&settled, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let max_binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg");
    assert_eq!(max_binding.target.revision, max_rev);
    assert!(!max_binding.spent);
    assert_eq!(max_binding.disposition, AttemptOutcome::Unattempted);
    // Follow-up observation: still released, still no retry or reauth.
    let steady = accepted(
        &mut engine,
        &observe(5, vec![max_fact("win-a", Some(true), false, false)]),
    );
    let (value, authority, seq) = max_row(&steady, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let full_binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg");
    assert_eq!(full_binding.target.revision, full_rev);
    assert!(full_binding.spent);
    assert_eq!(full_binding.disposition, AttemptOutcome::PossiblyExecuted);

    // Phase 2: unreadable attempt facts hold; first readable false recovers.
    let mut engine = max_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![max_fact("win-a", Some(true), false, false)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![max_fact("win-a", Some(true), false, false)],
            toggle("win-a"),
        ),
    );
    let max_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg")
        .target
        .revision;
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![max_fact("win-a", Some(true), false, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let full_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg")
        .target
        .revision;
    let mut unknown_row = max_fact("win-a", Some(true), false, false);
    unknown_row.fullscreen = None;
    let mut threw = observe(4, vec![unknown_row]);
    threw.state_attempts = vec![full_attempt(
        "win-a",
        true,
        full_rev,
        FrameSetter::Threw,
        Some(false),
        None,
    )];
    let held = accepted(&mut engine, &threw);
    let full_binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg");
    assert!(full_binding.spent);
    assert_eq!(full_binding.disposition, AttemptOutcome::PossiblyExecuted);
    let (value, authority, seq) = max_row(&held, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "no readable post-attempt observation yet: maximize must stay held, got {authority:?}"
    );
    let recovered = accepted(
        &mut engine,
        &observe(5, vec![max_fact("win-a", Some(true), false, false)]),
    );
    let (value, authority, seq) = max_row(&recovered, "win-a").expect("max target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let max_binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
        .expect("max leg");
    assert_eq!(max_binding.target.revision, max_rev);
    assert!(!max_binding.spent);
    assert_eq!(max_binding.disposition, AttemptOutcome::Unattempted);
    let full_binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("fullscreen leg");
    assert_eq!(full_binding.target.revision, full_rev);
    assert!(full_binding.spent);
    assert_eq!(full_binding.disposition, AttemptOutcome::PossiblyExecuted);
}
