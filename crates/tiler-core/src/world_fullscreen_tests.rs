//! Native fullscreen production: `SetFullscreen`/`ToggleFullscreen`
//! entry/exit, exitability, retention/settlement, and coexistence with the
//! sticky row. No maximize/above/composed-maximize behavior is exercised
//! here: entry from maximized is immediate with maximize preserved, and
//! ordinary exit returns to maximize with no native maximize writes.

use crate::connection::HostCapabilities;
use crate::directional::WindowId;
use crate::world_directional::tests::{accepted, fact, report_full, screen};
use crate::world_engine::{LegacyWorldOutcome, WorldEngine};
use crate::world_report::{
    AttemptOutcome, CommandRejectKind, CommandResult, FrameSetter, HoldReason, ScopeAuthority,
    StateAttempt, StateProperty, WorldCommand, WorldReport,
};
use crate::world_state::MAX_RETAINED_STATE_BINDINGS;

fn fs_caps() -> HostCapabilities {
    HostCapabilities {
        fullscreen: true,
        native_sticky: true,
        // Fixtures carry native maximize facts; the lane under test is
        // fullscreen-only, so the orthogonal native route stays valid.
        maximize: true,
        ..Default::default()
    }
}

fn unsupported_caps() -> HostCapabilities {
    HostCapabilities {
        fullscreen: false,
        native_sticky: true,
        ..Default::default()
    }
}

fn fs_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", fs_caps())
        .expect("generation")
}

fn unsupported_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", unsupported_caps())
        .expect("generation")
}

fn bootstrap(windows: Vec<crate::world_report::WindowFact>) -> WorldReport {
    report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "",
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
        "",
        Some(command_seq),
        Some(command),
    )
}

fn observe(seq: u64, windows: Vec<crate::world_report::WindowFact>) -> WorldReport {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        "",
        None,
        None,
    )
}

fn set_fullscreen(window: &str, value: bool) -> WorldCommand {
    WorldCommand::SetFullscreen {
        window: WindowId(window.to_owned()),
        value,
    }
}

fn toggle_fullscreen(window: &str) -> WorldCommand {
    WorldCommand::ToggleFullscreen {
        window: WindowId(window.to_owned()),
    }
}

fn fs_fact(
    window: &str,
    exitable: Option<bool>,
    fullscreen: bool,
) -> crate::world_report::WindowFact {
    let mut row = fact(window, "out-a", 0);
    row.fullscreen = Some(fullscreen);
    row.fullscreen_exitable = exitable;
    row
}

fn fs_unknown_fact(window: &str, exitable: Option<bool>) -> crate::world_report::WindowFact {
    let mut row = fact(window, "out-a", 0);
    row.fullscreen = None;
    row.fullscreen_exitable = exitable;
    row
}

fn binding_value(engine: &WorldEngine, window: &str) -> Option<bool> {
    engine
        .retained_states
        .get(&(WindowId(window.to_owned()), StateProperty::Fullscreen))
        .map(|binding| binding.target.value)
}

fn target_row(
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

fn attempt(
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

#[test]
fn set_entry_mints_retained_target_with_typed_seq() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    let home_before = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();
    let geometry_before = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .geometry;
    let rev_before = engine.state_revision;
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    assert!(reply.command_result.is_none());
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding");
    assert!(binding.target.value);
    assert_eq!(binding.target.revision, rev_before + 1);
    assert_eq!(
        binding
            .fullscreen_meta
            .as_ref()
            .expect("typed carrier")
            .command_seq,
        1
    );
    assert!(binding.sticky_meta.is_none());
    assert!(!binding.spent);
    assert_eq!(binding.disposition, AttemptOutcome::Unattempted);
    // No geometry or topology writes while fullscreen: member home and
    // placement survive entry untouched.
    let after = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(after.home, home_before);
    assert_eq!(after.geometry, geometry_before);
    let (value, authority, seq) = target_row(&reply, "win-a").expect("fullscreen target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
}

#[test]
fn toggle_entry_and_exitable_exit_compose() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    let entry = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            toggle_fullscreen("win-a"),
        ),
    );
    assert!(entry.command_result.is_none());
    assert_eq!(binding_value(&engine, "win-a"), Some(true));
    let exit = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fs_fact("win-a", Some(true), true)],
            toggle_fullscreen("win-a"),
        ),
    );
    assert!(exit.command_result.is_none());
    assert_eq!(binding_value(&engine, "win-a"), Some(false));
    // The exit authorizes from the fullscreen row itself.
    let (value, authority, seq) = target_row(&exit, "win-a").expect("exit target");
    assert_eq!((value, seq), (false, Some(2)));
    assert_eq!(authority, ScopeAuthority::Controlled);
}

#[test]
fn explicit_exit_needs_exitable_true() {
    for (exitable, kind) in [
        (Some(false), CommandRejectKind::FullscreenNotExitable),
        (None, CommandRejectKind::StateUnknown),
    ] {
        let mut engine = fs_engine();
        accepted(
            &mut engine,
            &bootstrap(vec![fs_fact("win-a", Some(true), true)]),
        );
        let reply = accepted(
            &mut engine,
            &cmd(
                2,
                1,
                vec![fs_fact("win-a", exitable, true)],
                set_fullscreen("win-a", false),
            ),
        );
        assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected { kind }),
            "exitable {exitable:?}"
        );
        assert!(
            !engine
                .retained_states
                .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
            "no binding minted on exitability refusal"
        );
    }
}

#[test]
fn unreadable_fullscreen_consumes_state_unknown() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    let mut unknown = fs_fact("win-a", None, false);
    unknown.fullscreen = None;
    let reply = accepted(
        &mut engine,
        &cmd(2, 1, vec![unknown], set_fullscreen("win-a", true)),
    );
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::StateUnknown
        })
    );
    assert!(engine.retained_states.is_empty());
}

#[test]
fn unsupported_caps_consume_state_unsupported() {
    let mut engine = unsupported_engine();
    // All caps stay absent here: the null native claim reaches the
    // intended fullscreen-unsupported gate instead of grading.
    let mut seed = fs_fact("win-a", None, false);
    seed.maximized = None;
    accepted(&mut engine, &bootstrap(vec![seed.clone()]));
    let reply = accepted(
        &mut engine,
        &cmd(2, 1, vec![seed], set_fullscreen("win-a", true)),
    );
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::StateUnsupported
        })
    );
    assert!(engine.retained_states.is_empty());
}

#[test]
fn same_state_entry_is_accepted_noop_without_writes() {
    let mut engine = fs_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![fs_fact("win-a", Some(true), true)]),
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", Some(true), true)],
            set_fullscreen("win-a", true),
        ),
    );
    assert!(reply.command_result.is_none());
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "same-state must not mint"
    );
    assert_eq!(engine.command_highwater, Some(1));
    assert!(target_row(&reply, "win-a").is_none());
}

#[test]
fn entry_from_maximized_is_immediate_with_no_maximize_writes() {
    let mut engine = fs_engine();
    let mut maximized = fs_fact("win-a", None, false);
    maximized.maximized = Some(true);
    accepted(&mut engine, &bootstrap(vec![maximized.clone()]));
    let reply = accepted(
        &mut engine,
        &cmd(2, 1, vec![maximized], set_fullscreen("win-a", true)),
    );
    assert!(reply.command_result.is_none());
    assert_eq!(binding_value(&engine, "win-a"), Some(true));
    // No maximize writes: no Maximized binding, no maximize result, and
    // the row still publishes fullscreen Controlled under maximize.
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Maximized))
    );
    let (value, authority, _) = target_row(&reply, "win-a").expect("target");
    assert!(value);
    assert_eq!(authority, ScopeAuthority::Controlled);
}

#[test]
fn ordinary_exit_returns_to_maximize_with_no_extra_maximize_write() {
    let mut engine = fs_engine();
    let mut row = fs_fact("win-a", Some(true), true);
    row.maximized = Some(true);
    accepted(&mut engine, &bootstrap(vec![row.clone()]));
    let reply = accepted(
        &mut engine,
        &cmd(2, 1, vec![row], set_fullscreen("win-a", false)),
    );
    assert!(reply.command_result.is_none());
    assert_eq!(binding_value(&engine, "win-a"), Some(false));
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Maximized)),
        "exit mints no maximize binding"
    );
    let (value, authority, seq) = target_row(&reply, "win-a").expect("exit target");
    assert_eq!((value, seq), (false, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
}

#[test]
fn observation_only_retains_tuple_without_blind_retry() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding")
        .clone();
    let reply = accepted(&mut engine, &observe(3, vec![fs_fact("win-a", None, true)]));
    let after = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("retained");
    assert_eq!(after.target.revision, binding.target.revision);
    assert_eq!(
        after.fullscreen_meta.as_ref().expect("carrier").command_seq,
        1
    );
    assert!(!after.spent);
    assert_eq!(after.disposition, AttemptOutcome::Unattempted);
    assert!(after.pending.is_none());
    let (_, _, seq) = target_row(&reply, "win-a").expect("retained target");
    assert_eq!(seq, Some(1));
}

#[test]
fn correlated_attempt_settles_and_fresh_command_reauthorizes() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding")
        .target
        .revision;
    // Correlated accepted proof settles spent without minting anything new.
    let mut proof = observe(3, vec![fs_fact("win-a", None, true)]);
    proof.state_attempts = vec![attempt(
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
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("retained");
    assert!(after.spent);
    assert_eq!(after.disposition, AttemptOutcome::Applied);
    assert_eq!(after.target.revision, rev);
    let (_, spent_authority, _) = target_row(&settled, "win-a").expect("target");
    assert_ne!(spent_authority, ScopeAuthority::Controlled);
    // An ambiguous throw latches PossiblyExecuted; a fresh same-state
    // command reauthorizes with a new revision and sequence.
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let first_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding")
        .target
        .revision;
    let mut ambiguous = observe(3, vec![fs_fact("win-a", None, false)]);
    ambiguous.state_attempts = vec![attempt(
        "win-a",
        true,
        first_rev,
        FrameSetter::Threw,
        Some(false),
        Some(false),
    )];
    accepted(&mut engine, &ambiguous);
    let stuck = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding");
    assert!(stuck.spent);
    assert_eq!(stuck.disposition, AttemptOutcome::PossiblyExecuted);
    let reauth = accepted(
        &mut engine,
        &cmd(
            4,
            2,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    assert!(reauth.command_result.is_none());
    let fresh = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding");
    assert!(!fresh.spent);
    assert_eq!(fresh.disposition, AttemptOutcome::Unattempted);
    assert!(fresh.target.revision > first_rev);
    assert_eq!(
        fresh.fullscreen_meta.as_ref().expect("carrier").command_seq,
        2
    );
}

#[test]
fn fullscreen_coexists_with_sticky_on_one_row() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            crate::world_report::WorldCommand::SetSticky {
                window: WindowId("win-a".to_owned()),
                value: true,
            },
        ),
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    assert!(reply.command_result.is_none());
    assert_eq!(binding_value(&engine, "win-a"), Some(true));
    let sticky = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("sticky binding");
    assert!(sticky.target.value);
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime == WindowId("win-a".to_owned()))
        .expect("row");
    assert!(
        row.state
            .iter()
            .any(|target| target.property == StateProperty::Sticky)
    );
    let (value, _, seq) = target_row(&reply, "win-a").expect("fullscreen target");
    assert_eq!((value, seq), (true, Some(2)));
}

#[test]
fn minimized_window_refuses_without_mutation() {
    let mut engine = fs_engine();
    let mut minimized = fs_fact("win-a", None, false);
    minimized.minimized = Some(true);
    accepted(&mut engine, &bootstrap(vec![minimized.clone()]));
    let rev_before = engine.state_revision;
    match engine.handle_report_legacy(&cmd(2, 1, vec![minimized], set_fullscreen("win-a", true))) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected minimized refusal"
        ),
        other => panic!("expected minimized refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, rev_before + 1, "refusal issues");
    assert!(engine.retained_states.is_empty());
}

#[test]
fn binding_cap_replacement_allowed_but_new_key_refused_atomic() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
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
        "exact bound before replacement"
    );
    // Same-window replacement at the exact cap mints through the producer:
    // raw still reads false, so the toggle resolves to true.
    let replaced = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fs_fact("win-a", None, false)],
            toggle_fullscreen("win-a"),
        ),
    );
    assert!(replaced.command_result.is_none());
    assert_eq!(binding_value(&engine, "win-a"), Some(true));
    // A distinct new window at the cap refuses before mutation.
    let mut capped = fs_engine();
    accepted(&mut capped, &bootstrap(vec![fs_fact("win-a", None, false)]));
    let need = MAX_RETAINED_STATE_BINDINGS - capped.retained_states.len();
    for i in 0..need {
        capped.retained_states.insert(
            (WindowId(format!("win-cap-{i:05}")), StateProperty::Above),
            crate::world_state::RetainedStateBinding {
                target: crate::world_state::RetainedStateTarget {
                    window: WindowId(format!("win-cap-{i:05}")),
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
    let revision = capped.state_revision;
    match capped.handle_report_legacy(&cmd(
        2,
        1,
        vec![fs_fact("win-a", None, false)],
        set_fullscreen("win-a", true),
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
    assert_eq!(capped.state_revision, revision + 1, "refusal issues");
    assert!(
        !capped
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "no binding minted past the cap"
    );
}

#[test]
fn explicit_close_prunes_fullscreen_binding_and_lifetime_row() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    assert!(
        engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
    );
    let mut closed = observe(3, vec![]);
    closed.closed = vec![WindowId("win-a".to_owned())];
    let reply = accepted(&mut engine, &closed);
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "explicit close prunes the producer target"
    );
    assert!(
        target_row(&reply, "win-a").is_none(),
        "closed window publishes no target"
    );
}

#[test]
fn unsupported_caps_with_null_fullscreen_consumes_state_unsupported() {
    for (label, command) in [
        ("set", set_fullscreen("win-a", true)),
        ("toggle", toggle_fullscreen("win-a")),
    ] {
        let mut engine = unsupported_engine();
        // All caps stay absent here: null native claims reach the
        // intended fullscreen-unsupported gate instead of grading.
        let mut seed = fs_fact("win-a", None, false);
        seed.maximized = None;
        accepted(&mut engine, &bootstrap(vec![seed]));
        let mut unknown = fs_unknown_fact("win-a", None);
        unknown.maximized = None;
        let reply = accepted(&mut engine, &cmd(2, 1, vec![unknown], command));
        assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::StateUnsupported
            }),
            "unsupported + null must be StateUnsupported ({label})"
        );
        assert!(
            engine.retained_states.is_empty(),
            "no binding minted ({label})"
        );
    }
}

#[test]
fn stale_true_binding_canceled_by_same_state_set() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    assert_eq!(binding_value(&engine, "win-a"), Some(true));
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", false),
        ),
    );
    assert!(reply.command_result.is_none());
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "obsolete opposite binding must be revoked"
    );
    assert!(target_row(&reply, "win-a").is_none());
    assert_eq!(engine.command_highwater, Some(2));
}

#[test]
fn stale_false_binding_canceled_by_same_state_set() {
    let mut engine = fs_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![fs_fact("win-a", Some(true), true)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", Some(true), true)],
            set_fullscreen("win-a", false),
        ),
    );
    assert_eq!(binding_value(&engine, "win-a"), Some(false));
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fs_fact("win-a", Some(true), true)],
            set_fullscreen("win-a", true),
        ),
    );
    assert!(reply.command_result.is_none());
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "obsolete opposite binding must be revoked"
    );
    assert!(target_row(&reply, "win-a").is_none());
    assert_eq!(engine.command_highwater, Some(2));
}

#[test]
fn spent_opposite_binding_canceled_by_same_state_set() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding")
        .target
        .revision;
    let mut proof = observe(3, vec![fs_fact("win-a", None, true)]);
    proof.state_attempts = vec![attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    )];
    accepted(&mut engine, &proof);
    assert!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
            .expect("spent")
            .spent
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            4,
            2,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", false),
        ),
    );
    assert!(reply.command_result.is_none());
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "spent opposite binding must be revoked"
    );
    assert!(target_row(&reply, "win-a").is_none());
    assert_eq!(engine.command_highwater, Some(2));
}

#[test]
fn exit_exitable_fence_holds_and_resumes_with_same_tuple() {
    let mut engine = fs_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![fs_fact("win-a", Some(true), true)]),
    );
    let minted = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", Some(true), true)],
            set_fullscreen("win-a", false),
        ),
    );
    let (_, minted_authority, minted_seq) = target_row(&minted, "win-a").expect("exit target");
    assert_eq!(minted_authority, ScopeAuthority::Controlled);
    assert_eq!(minted_seq, Some(1));
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding")
        .target
        .revision;
    for (seq, exitable) in [(3, Some(false)), (4, None)] {
        let held = accepted(
            &mut engine,
            &observe(seq, vec![fs_fact("win-a", exitable, true)]),
        );
        let (value, authority, seq) = target_row(&held, "win-a").expect("exit target");
        assert!(!value);
        assert!(
            matches!(authority, ScopeAuthority::Hold(_)),
            "active exit with exitable {exitable:?} must Hold"
        );
        assert_eq!(seq, Some(1));
    }
    let resumed = accepted(
        &mut engine,
        &observe(5, vec![fs_fact("win-a", Some(true), true)]),
    );
    let (value, authority, seq) = target_row(&resumed, "win-a").expect("exit target");
    assert!(!value);
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert_eq!(seq, Some(1));
    assert_eq!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
            .expect("binding")
            .target
            .revision,
        rev
    );
}

#[test]
fn raw_null_publication_holds_entry_binding() {
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let held = accepted(
        &mut engine,
        &observe(3, vec![fs_unknown_fact("win-a", None)]),
    );
    let (value, authority, seq) = target_row(&held, "win-a").expect("entry target");
    assert!(value);
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "raw unknown must never publish Controlled"
    );
    assert_eq!(seq, Some(1));
}

#[test]
fn pending_window_transfer_holds_fullscreen_publication() {
    // Real-route pending-transfer fence: mint a fullscreen entry, then move
    // the same window with a same-output `Send` so a window flight is
    // retained. Publication must Hold (never Controlled) until arrival,
    // keeping the exact tuple/revision/command_seq unspent; the standard
    // raw arrival evidence settles the flight and resumes the same target
    // as Controlled.
    let mut engine = fs_engine();
    let boot = accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
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
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let (_, minted_authority, _) = target_row(&minted, "win-a").expect("entry target");
    assert_eq!(minted_authority, ScopeAuthority::Controlled);
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding")
        .target
        .revision;
    let sent = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fs_fact("win-a", None, false)],
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
    assert!(
        engine.retained.iter().any(|transfer| matches!(
            &transfer.target.kind,
            crate::world_report::TransferKind::Window(w) if w.0 == "win-a"
        )),
        "send must retain the same-window flight"
    );
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding survives the send");
    assert!(!binding.spent);
    assert_eq!(binding.target.revision, rev);
    let (value, authority, seq) = target_row(&sent, "win-a").expect("entry target");
    assert!(value);
    assert!(
        !matches!(authority, ScopeAuthority::Controlled),
        "pending same-window transfer must never publish Controlled, got {authority:?}"
    );
    assert_eq!(seq, Some(1));
    // Settle the real flight with the standard raw transfer evidence: the
    // adapter echoes the destination view and carries the accepted member
    // attempt for `op-2`. The same unspent target resumes Controlled.
    let mut arrived = report_full(
        4,
        4,
        vec![screen("out-a", 0)],
        vec![fs_fact("win-a", None, false)],
        vec![("out-a", ring[1].as_str())],
        "win-a",
        None,
        None,
    );
    arrived
        .member_attempts
        .push(crate::world_report::MemberAttempt {
            operation_id: "op-2".to_owned(),
            window: WindowId("win-a".to_owned()),
            target_output: crate::directional::OutputId("out-a".to_owned()),
            target_workspace: ring[1].clone(),
            setter: FrameSetter::Accepted,
            pre: None,
            post: Some(crate::world_report::WorkspaceRef::Logical {
                output: crate::directional::OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(ring[1].clone()),
            }),
            attempt_ms: 2,
        });
    let settled = accepted(&mut engine, &arrived);
    assert!(
        !engine.retained.iter().any(|transfer| matches!(
            &transfer.target.kind,
            crate::world_report::TransferKind::Window(w) if w.0 == "win-a"
        )),
        "arrival evidence settles the same-window flight"
    );
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding survives arrival");
    assert!(!binding.spent);
    assert_eq!(binding.target.revision, rev);
    let (value, authority, seq) = target_row(&settled, "win-a").expect("entry target");
    assert!(value);
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert_eq!(seq, Some(1));
}

#[test]
fn held_scope_holds_entry_then_healthy_recovery_resumes_tuple() {
    // A held window on the shared scope fences the unspent entry to Hold;
    // the healthy report resumes the exact tuple/revision/command_seq.
    let mut engine = fs_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![
            fs_fact("win-a", None, false),
            fact("win-b", "out-a", 400),
        ]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false), fact("win-b", "out-a", 400)],
            set_fullscreen("win-a", true),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding")
        .target
        .revision;
    let mut held = observe(3, vec![fs_fact("win-a", None, false)]);
    held.held_windows = vec![crate::world_report::HeldWindowRow {
        window: WindowId("win-b".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: crate::world_report::RowDiagnosticKind::InvalidFact,
    }];
    let held_reply = accepted(&mut engine, &held);
    let (value, authority, seq) = target_row(&held_reply, "win-a").expect("entry target");
    assert!(value);
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "held scope must never publish Controlled"
    );
    assert_eq!(seq, Some(1));
    let resumed = accepted(
        &mut engine,
        &observe(
            4,
            vec![fs_fact("win-a", None, false), fact("win-b", "out-a", 400)],
        ),
    );
    let (value, authority, seq) = target_row(&resumed, "win-a").expect("entry target");
    assert!(value);
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert_eq!(seq, Some(1));
    assert_eq!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
            .expect("binding")
            .target
            .revision,
        rev
    );
}

#[test]
fn lost_issued_reply_republish_accepts_republished_stamp() {
    // Lost R then first acted-on republish at R+1 through the real
    // producer: the observation-only reply republishes the same binding
    // revision as Controlled, and the R+1-stamped attempt latches as the
    // original raw evidence.
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    let issued = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let (value, authority, seq) = target_row(&issued, "win-a").expect("issued target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let rev = issued.state_revision;
    let binding_rev = |engine: &WorldEngine| {
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
            .expect("binding")
            .target
            .revision
    };
    assert_eq!(binding_rev(&engine), rev);
    // The issued reply is lost: native stays false, so the next
    // observation-only reply republishes Controlled at R+1 with the same
    // binding revision.
    let republished = accepted(
        &mut engine,
        &observe(3, vec![fs_fact("win-a", None, false)]),
    );
    assert_eq!(republished.state_revision, rev + 1);
    let (value, authority, seq) = target_row(&republished, "win-a").expect("republished target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert_eq!(binding_rev(&engine), rev);
    // First acted-on attempt stamps the republish revision R+1.
    let row = attempt(
        "win-a",
        true,
        rev + 1,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    );
    let mut proof = observe(4, vec![fs_fact("win-a", None, true)]);
    proof.state_attempts = vec![row.clone()];
    accepted(&mut engine, &proof);
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding retained");
    assert!(binding.spent);
    assert_eq!(binding.pending.as_ref(), Some(&row));
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
}

#[test]
fn initial_hold_publication_attempt_records_evidence() {
    // The initial publication is itself Hold: the command report arrives
    // with incomplete inventory (the fullscreen pregates do not gate
    // inventory; publication does), so the fresh binding publishes
    // Hold(IncompleteInventory) at its own revision. The issued-revision
    // stamp still correlates as raw evidence with no Controlled row and
    // no new binding, and a later healthy report proves from it with no
    // new attempt.
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    let mut commanded = cmd(
        2,
        1,
        vec![fs_fact("win-a", None, false)],
        set_fullscreen("win-a", true),
    );
    commanded.inventory_complete = false;
    let issued = accepted(&mut engine, &commanded);
    let rev = issued.state_revision;
    let (value, authority, seq) = target_row(&issued, "win-a").expect("issued target");
    assert!(value);
    assert_eq!(
        authority,
        ScopeAuthority::Hold(HoldReason::IncompleteInventory)
    );
    assert_eq!(seq, Some(1));
    assert_eq!(
        engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
            .expect("binding")
            .target
            .revision,
        rev,
    );
    // Attempt stamped with the issued Hold revision: raw evidence records
    // first and marks spent; still no Controlled row, no new binding.
    let row = attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    );
    let mut incomplete = observe(3, vec![fs_fact("win-a", None, false)]);
    incomplete.inventory_complete = false;
    incomplete.state_attempts = vec![row.clone()];
    let before = engine.retained_states.len();
    let reply = accepted(&mut engine, &incomplete);
    let (value, authority, _) = target_row(&reply, "win-a").expect("held target");
    assert!(value);
    assert!(matches!(authority, ScopeAuthority::Hold(_)));
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding retained");
    assert_eq!(binding.target.revision, rev);
    assert!(binding.spent);
    assert_eq!(binding.pending.as_ref(), Some(&row));
    assert_eq!(binding.disposition, AttemptOutcome::PossiblyExecuted);
    assert_eq!(engine.retained_states.len(), before);
    // Later healthy report proves from the evidence with no new attempt.
    accepted(&mut engine, &observe(4, vec![fs_fact("win-a", None, true)]));
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding retained");
    assert_eq!(binding.pending.as_ref(), Some(&row));
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
}

#[test]
fn earlier_controlled_evidence_under_held_receipt() {
    // Healthy Controlled issued at R, then a held (minimized) receipt at
    // R+1 carries the earlier rev-R evidence: settle records it first and
    // marks spent; the reply keeps the Hold row with the same binding
    // revision and mints no new binding. A healthy recovery then proves
    // from the held evidence, and a held redelivery of the same earlier
    // evidence keeps the positive proof.
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    let issued = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let (value, authority, seq) = target_row(&issued, "win-a").expect("issued target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let rev = issued.state_revision;
    let mut minimized = fs_fact("win-a", None, false);
    minimized.minimized = Some(true);
    let row = attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    );
    let mut held = observe(3, vec![minimized]);
    held.state_attempts = vec![row.clone()];
    let before = engine.retained_states.len();
    let reply = accepted(&mut engine, &held);
    assert_eq!(reply.state_revision, rev + 1);
    let (value, authority, seq) = target_row(&reply, "win-a").expect("held target");
    assert!(value);
    assert!(
        matches!(authority, ScopeAuthority::Hold(_)),
        "minimized publication never authorizes Controlled"
    );
    assert_eq!(seq, Some(1));
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding retained");
    assert_eq!(binding.target.revision, rev);
    assert!(binding.spent);
    assert_eq!(binding.pending.as_ref(), Some(&row));
    assert_eq!(binding.disposition, AttemptOutcome::PossiblyExecuted);
    assert_eq!(engine.retained_states.len(), before);
    // Healthy recovery proves from the held evidence with no new attempt.
    accepted(&mut engine, &observe(4, vec![fs_fact("win-a", None, true)]));
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding retained");
    assert_eq!(binding.pending.as_ref(), Some(&row));
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
    // A held redelivery of the same earlier evidence keeps the proof.
    let mut redelivered = fs_fact("win-a", None, true);
    redelivered.minimized = Some(true);
    let mut redelivery = observe(5, vec![redelivered]);
    redelivery.state_attempts = vec![row.clone()];
    let reply = accepted(&mut engine, &redelivery);
    let (value, authority, _) = target_row(&reply, "win-a").expect("held target");
    assert!(value);
    assert!(matches!(authority, ScopeAuthority::Hold(_)));
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding retained");
    assert_eq!(binding.pending.as_ref(), Some(&row));
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
    assert_eq!(engine.retained_states.len(), before);
}

#[test]
fn held_republish_accepts_republished_stamp_without_authority() {
    // Robustness against a misbehaving adapter writing a Hold row: the
    // adapter must only write Controlled rows, but if it acts on a Hold
    // publication anyway, settle still correlates the evidence without
    // granting authority. Hold published across the republish: the first
    // arrival under hold stamps R+1 and latches as raw evidence.
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    let issued = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let rev = issued.state_revision;
    // First held receipt carries no attempt: reply R+1 publishes Hold and
    // the binding stays unspent.
    let mut minimized = fs_fact("win-a", None, false);
    minimized.minimized = Some(true);
    let reply = accepted(&mut engine, &observe(3, vec![minimized]));
    assert_eq!(reply.state_revision, rev + 1);
    let (_, authority, _) = target_row(&reply, "win-a").expect("held target");
    assert!(matches!(authority, ScopeAuthority::Hold(_)));
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding retained");
    assert_eq!(binding.target.revision, rev);
    assert!(!binding.spent);
    // Second held receipt carries the R+1 stamp: in-interval evidence
    // latches spent with no new binding and no authority.
    let row = attempt(
        "win-a",
        true,
        rev + 1,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    );
    let mut minimized = fs_fact("win-a", None, false);
    minimized.minimized = Some(true);
    let mut held = observe(4, vec![minimized]);
    held.state_attempts = vec![row.clone()];
    let before = engine.retained_states.len();
    let reply = accepted(&mut engine, &held);
    let (_, authority, _) = target_row(&reply, "win-a").expect("held target");
    assert!(matches!(authority, ScopeAuthority::Hold(_)));
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding retained");
    assert_eq!(binding.target.revision, rev);
    assert!(binding.spent);
    assert_eq!(binding.pending.as_ref(), Some(&row));
    assert_eq!(binding.disposition, AttemptOutcome::PossiblyExecuted);
    assert_eq!(engine.retained_states.len(), before);
}

#[test]
fn replaced_true_false_true_obsoletes_original_stamp() {
    // Real producer replacements: entry, same-state exit (consumed with
    // no write, obsolete entry binding removed), fresh entry. The
    // original R stamp reads absent against the R+2 binding even though
    // its value matches.
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    let first = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let rev = first.state_revision;
    let second = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", false),
        ),
    );
    assert!(second.command_result.is_none());
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "same-state exit removes the obsolete entry binding"
    );
    let third = accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let (value, authority, seq) = target_row(&third, "win-a").expect("fresh entry target");
    assert_eq!((value, seq), (true, Some(3)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    let latest = third.state_revision;
    assert_eq!(latest, rev + 2);
    let mut proof = observe(5, vec![fs_fact("win-a", None, true)]);
    proof.state_attempts = vec![attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    )];
    let reply = accepted(&mut engine, &proof);
    assert!(reply.diagnostics.is_empty());
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding retained");
    assert_eq!(binding.target.revision, latest);
    assert!(!binding.spent);
    assert_eq!(binding.disposition, AttemptOutcome::Unattempted);
    assert!(binding.pending.is_none());
}

#[test]
fn two_output_binding_isolation_keeps_other_output_healthy() {
    // The entry binding targets only win-a on out-a; win-b on out-b leaks
    // no StateTarget, mints no binding, and keeps its healthy homing.
    let mut engine = fs_engine();
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let windows = vec![fs_fact("win-a", None, false), fact("win-b", "out-b", 800)];
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            screens.clone(),
            windows.clone(),
            vec![],
            "",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            screens,
            windows,
            vec![],
            "",
            Some(1),
            Some(set_fullscreen("win-a", true)),
        ),
    );
    let (value, authority, seq) = target_row(&reply, "win-a").expect("entry target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert!(
        target_row(&reply, "win-b").is_none(),
        "untargeted output window leaks no StateTarget"
    );
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-b".to_owned()), StateProperty::Fullscreen))
    );
    let other = engine
        .members
        .get(&WindowId("win-b".to_owned()))
        .expect("member");
    assert_eq!(
        other.home.output,
        crate::directional::OutputId("out-b".to_owned())
    );
}

#[test]
fn rehandshake_resets_binding_and_old_proof_cannot_actuate() {
    // Generation reset clears the producer binding; a redelivered old
    // attempt (exact pre-reset tuple/revision) settles nothing and mints
    // nothing.
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            set_fullscreen("win-a", true),
        ),
    );
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding")
        .target
        .revision;
    let reply = engine.handshake(&crate::connection::HandshakeRequest {
        adapter: "adapter-2".to_owned(),
        request: "handshake-2".to_owned(),
        application_identity: crate::connection::ApplicationIdentity {
            app_id: Some("example.app".to_owned()),
            executable: Some("example".to_owned()),
            process_id: Some("pid-own-1".to_owned()),
        },
        capabilities: crate::connection::HostCapabilities {
            fullscreen: true,
            native_sticky: true,
            ..Default::default()
        },
    });
    assert!(reply.kind.is_none());
    let generation = reply.generation.expect("generation");
    let settings = engine.handle_settings(&crate::connection::SettingsRequest {
        adapter: "adapter-2".to_owned(),
        generation,
        request: "post-handshake-settings".to_owned(),
        revision: 0,
        values: vec![(
            "mode".to_owned(),
            crate::connection::RawSettingValue::String(
                crate::workspace_scope::WORKSPACE_MODE_TOKENS[0].to_owned(),
            ),
        )],
    });
    assert!(settings.is_some());
    assert!(
        engine.retained_states.is_empty(),
        "handshake clears producer targets"
    );
    let mut stale = observe(3, vec![fs_fact("win-a", None, true)]);
    stale.adapter_incarnation = engine
        .connection_owner
        .as_ref()
        .map(|owner| owner.as_str().to_owned())
        .expect("owner");
    stale.core_generation = engine.core_generation_str.clone();
    stale.state_attempts = vec![attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    )];
    let reply = accepted(&mut engine, &stale);
    assert!(
        engine.retained_states.is_empty(),
        "old proof after reset mints nothing"
    );
    assert!(
        target_row(&reply, "win-a").is_none(),
        "old proof actuates no target"
    );
}

#[test]
fn consecutive_toggles_resolve_from_raw_facts_not_queued_intent() {
    // Two fresh toggles over an unchanged raw false both resolve to true
    // from the raw fact (never by flipping the unconfirmed queued true);
    // once true is observed, the next toggle resolves false.
    let mut engine = fs_engine();
    accepted(&mut engine, &bootstrap(vec![fs_fact("win-a", None, false)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fs_fact("win-a", None, false)],
            toggle_fullscreen("win-a"),
        ),
    );
    assert_eq!(binding_value(&engine, "win-a"), Some(true));
    let first_rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding")
        .target
        .revision;
    let second = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fs_fact("win-a", None, false)],
            toggle_fullscreen("win-a"),
        ),
    );
    assert!(second.command_result.is_none());
    let fresh = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
        .expect("binding");
    assert!(fresh.target.value, "stale raw false still resolves true");
    assert_eq!(
        fresh.fullscreen_meta.as_ref().expect("carrier").command_seq,
        2
    );
    assert!(fresh.target.revision > first_rev);
    accepted(
        &mut engine,
        &observe(4, vec![fs_fact("win-a", Some(true), true)]),
    );
    let exit = accepted(
        &mut engine,
        &cmd(
            5,
            3,
            vec![fs_fact("win-a", Some(true), true)],
            toggle_fullscreen("win-a"),
        ),
    );
    assert!(exit.command_result.is_none());
    assert_eq!(binding_value(&engine, "win-a"), Some(false));
}

#[test]
fn spent_same_value_binding_consumes_same_state_without_renewal() {
    // Spent failed binding whose target has since become raw reality: a
    // fresh SetFullscreen to that same known value consumes as a no-op:
    // no rejection, no new Controlled setter, no new binding rev/cmdseq.
    // The false leg carries exitable None: raw already false needs no exit.
    for (desired, boot_exitable, boot_value, arrived_exitable, arrived_value, pre) in [
        (true, None, false, None, true, Some(false)),
        (false, Some(true), true, None, false, Some(true)),
    ] {
        let mut engine = fs_engine();
        accepted(
            &mut engine,
            &bootstrap(vec![fs_fact("win-a", boot_exitable, boot_value)]),
        );
        accepted(
            &mut engine,
            &cmd(
                2,
                1,
                vec![fs_fact("win-a", boot_exitable, boot_value)],
                set_fullscreen("win-a", desired),
            ),
        );
        let rev = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
            .expect("binding")
            .target
            .revision;
        // Correlated throw latches PossiblyExecuted, still spent.
        let mut threw = observe(3, vec![fs_fact("win-a", boot_exitable, boot_value)]);
        threw.state_attempts = vec![attempt("win-a", desired, rev, FrameSetter::Threw, pre, pre)];
        accepted(&mut engine, &threw);
        // Raw fact later reaches the requested value with no new command.
        accepted(
            &mut engine,
            &observe(4, vec![fs_fact("win-a", arrived_exitable, arrived_value)]),
        );
        // Fresh SetFullscreen to that same known value: pure no-op.
        let reply = accepted(
            &mut engine,
            &cmd(
                5,
                2,
                vec![fs_fact("win-a", arrived_exitable, arrived_value)],
                set_fullscreen("win-a", desired),
            ),
        );
        assert!(
            reply.command_result.is_none(),
            "desired {desired}: same-state must not reject"
        );
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen))
            .expect("spent tuple preserved");
        assert_eq!(binding.target.revision, rev, "desired {desired}");
        assert_eq!(
            binding
                .fullscreen_meta
                .as_ref()
                .expect("carrier")
                .command_seq,
            1,
            "desired {desired}"
        );
        assert!(binding.spent, "desired {desired}");
        assert_eq!(binding.disposition, AttemptOutcome::PossiblyExecuted);
        let (value, authority, seq) = target_row(&reply, "win-a").expect("target");
        assert_eq!(value, desired);
        assert!(
            !matches!(authority, ScopeAuthority::Controlled),
            "desired {desired}: spent target must not renew Controlled"
        );
        assert_eq!(seq, Some(1));
        assert_eq!(engine.command_highwater, Some(2));
    }
}
