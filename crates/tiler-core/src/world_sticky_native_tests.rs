//! Native sticky command production and state publication.

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId};
use crate::intent_data::{IntentData, encode_intent_data};
use crate::world_directional::tests::{accepted, fact, float_fact, report_full, screen};
use crate::world_engine::{LegacyWorldOutcome, WorldEngine};
use crate::world_report::{
    AttemptOutcome, CommandRejectKind, IntentRecord, IntentWrite, ScopeAuthority, StateProperty,
    WorldCommand, WorldReport,
};

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

fn set_sticky(window: &str, value: bool) -> WorldCommand {
    WorldCommand::SetSticky {
        window: WindowId(window.to_owned()),
        value,
    }
}

fn toggle_float(window: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect: None,
    })
}

fn native_engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn binding_value(engine: &WorldEngine, window: &str) -> Option<bool> {
    engine
        .retained_states
        .get(&(WindowId(window.to_owned()), StateProperty::Sticky))
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
                .find(|target| target.property == StateProperty::Sticky)
                .map(|target| (target.value, target.authority, target.command_seq))
        })
}

#[test]
fn native_on_from_tile_records_false_prior_and_floats() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(reply.command_result.is_none());
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    let save = reply.intent_writes.first().expect("one save");
    assert_eq!(
        crate::intent_data::decode_intent_data(save.data.as_deref().expect("bytes"))
            .expect("decodes"),
        IntentData::new(Some(true), Some(false))
    );
    let (value, authority, seq) = target_row(&reply, "win-a").expect("sticky target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert!(reply.windows.iter().all(|row| row.visibility.is_none()));
}

#[test]
fn native_on_from_float_records_true_prior() {
    let mut engine = native_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![float_fact("win-a", "out-a", 0, 0)]),
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![float_fact("win-a", "out-a", 0, 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(reply.command_result.is_none());
    let save = reply.intent_writes.first().expect("one save");
    assert_eq!(
        crate::intent_data::decode_intent_data(save.data.as_deref().expect("bytes"))
            .expect("decodes"),
        IntentData::new(Some(true), Some(true))
    );
    assert_eq!(binding_value(&engine, "win-a"), Some(true));
}

#[test]
fn native_off_holds_float_runtime_and_queues_off_record() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let before_home = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    assert!(reply.command_result.is_none());
    let after = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(after.home, before_home);
    assert!(after.floating);
    let save = reply.intent_writes.first().expect("one save");
    assert_eq!(
        crate::intent_data::decode_intent_data(save.data.as_deref().expect("bytes"))
            .expect("decodes"),
        IntentData::new(Some(false), None)
    );
    let (value, _, seq) = target_row(&reply, "win-a").expect("sticky target");
    assert_eq!((value, seq), (false, Some(2)));
}

#[test]
fn native_toggle_float_holds_runtime_with_tile_base() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let before_home = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();
    let reply = accepted(
        &mut engine,
        &cmd(3, 2, vec![fact("win-a", "out-a", 0)], toggle_float("win-a")),
    );
    assert!(reply.command_result.is_none());
    let after = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(after.home, before_home);
    assert!(after.floating);
    let save = reply.intent_writes.first().expect("one save");
    assert_eq!(
        crate::intent_data::decode_intent_data(save.data.as_deref().expect("bytes"))
            .expect("decodes"),
        IntentData::new(Some(false), None)
    );
    let (value, _, _) = target_row(&reply, "win-a").expect("sticky target");
    assert!(!value);
}

#[test]
fn native_off_prior_float_dest_floating_still_publishes_clear() {
    let mut engine = native_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![float_fact("win-a", "out-a", 0, 0)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![float_fact("win-a", "out-a", 0, 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![float_fact("win-a", "out-a", 0, 0)],
            WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
                tiled: false,
            },
        ),
    );
    assert!(!engine.floating_scopes.is_empty());
    let reply = accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![float_fact("win-a", "out-a", 0, 0)],
            set_sticky("win-a", false),
        ),
    );
    assert!(reply.command_result.is_none());
    let (value, _, _) = target_row(&reply, "win-a").expect("sticky clear publishes");
    assert!(!value);
}

#[test]
fn native_unknown_sticky_rejects_state_unknown() {
    let mut engine = native_engine();
    let mut unknown = fact("win-a", "out-a", 0);
    unknown.sticky = None;
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let reply = accepted(
        &mut engine,
        &cmd(2, 1, vec![unknown], set_sticky("win-a", true)),
    );
    assert_eq!(
        reply.command_result,
        Some(crate::world_report::CommandResult::Rejected {
            kind: CommandRejectKind::StateUnknown
        })
    );
    assert!(engine.retained_states.is_empty());
    assert!(engine.sticky_command_save.is_none());
}

#[test]
fn native_unknown_raw_null_masks_semantic_restore() {
    // Restored semantic sticky must not mask an unreadable raw null:
    // command decisions stay typed StateUnknown on the native route.
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
    let mut unknown = fact("win-a", "out-a", 0);
    unknown.sticky = None;
    let reply = accepted(
        &mut engine,
        &cmd(3, 2, vec![unknown], set_sticky("win-a", false)),
    );
    assert_eq!(
        reply.command_result,
        Some(crate::world_report::CommandResult::Rejected {
            kind: CommandRejectKind::StateUnknown
        })
    );
}

#[test]
fn native_same_state_noop_keeps_target_and_reauthorizes_after_ambiguous() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let first = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let (value, _, seq) = target_row(&first, "win-a").expect("target");
    assert_eq!((value, seq), (true, Some(1)));
    let second = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(second.command_result.is_none());
    let (_, _, second_seq) = target_row(&second, "win-a").expect("target retained");
    assert_eq!(second_seq, Some(1));
    let key = (WindowId("win-a".to_owned()), StateProperty::Sticky);
    let mut binding = engine.retained_states.get(&key).expect("binding").clone();
    binding.spent = true;
    binding.disposition = AttemptOutcome::PossiblyExecuted;
    engine.retained_states.insert(key.clone(), binding);
    let third = accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(third.command_result.is_none());
    let (_, _, third_seq) = target_row(&third, "win-a").expect("reauthorized");
    assert_eq!(third_seq, Some(3));
}

#[test]
fn native_target_cmdseq_and_revision_stable_across_observation() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let first = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let first_rev = first.state_revision;
    let (_, _, first_seq) = target_row(&first, "win-a").expect("target");
    let second = accepted(&mut engine, &observe(3, vec![fact("win-a", "out-a", 0)]));
    assert!(second.command_result.is_none());
    let (_, _, second_seq) = target_row(&second, "win-a").expect("retained target");
    assert_eq!(first_seq, second_seq);
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding retained");
    assert_eq!(binding.target.revision, first_rev);
}

#[test]
fn native_spent_binding_never_controlled() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let key = (WindowId("win-a".to_owned()), StateProperty::Sticky);
    let mut binding = engine.retained_states.get(&key).expect("binding").clone();
    binding.spent = true;
    binding.disposition = AttemptOutcome::PossiblyExecuted;
    engine.retained_states.insert(key, binding);
    let reply = accepted(&mut engine, &observe(3, vec![fact("win-a", "out-a", 0)]));
    let (_, authority, seq) = target_row(&reply, "win-a").expect("target retained");
    assert_eq!(seq, Some(1));
    assert_ne!(authority, ScopeAuthority::Controlled);
}

#[test]
fn native_spent_retry_reemits_canonical_save_preserving_prior() {
    use crate::world_report::{FrameSetter, StateAttempt};
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let first = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let first_save = first.intent_writes.first().expect("one save").data.clone();
    assert_eq!(
        crate::intent_data::decode_intent_data(first_save.as_deref().expect("bytes"))
            .expect("decodes"),
        IntentData::new(Some(true), Some(false))
    );
    // Drive the binding to spent/PossiblyExecuted through the real
    // settlement producer: a threw attempt with the exact target revision.
    let key = (WindowId("win-a".to_owned()), StateProperty::Sticky);
    let revision = engine
        .retained_states
        .get(&key)
        .expect("binding")
        .target
        .revision;
    let mut settled = observe(3, vec![fact("win-a", "out-a", 0)]);
    settled.state_attempts = vec![StateAttempt {
        window: WindowId("win-a".to_owned()),
        property: StateProperty::Sticky,
        target: true,
        revision,
        setter: FrameSetter::Threw,
        pre: None,
        post: None,
        attempt_ms: 7,
    }];
    accepted(&mut engine, &settled);
    let spent = engine.retained_states.get(&key).expect("binding");
    assert!(spent.spent, "producer marks spent");
    assert_eq!(spent.disposition, AttemptOutcome::PossiblyExecuted);
    // Fresh retry with a new sequence reauthorizes: new binding owned by
    // the new command, canonical save preserving the original prior
    // (not the now-float runtime).
    let retry = accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(retry.command_result.is_none());
    let save = retry
        .intent_writes
        .first()
        .expect("retry save")
        .data
        .clone();
    assert_eq!(save, first_save, "retry preserves original prior");
    assert_eq!(
        crate::intent_data::decode_intent_data(save.as_deref().expect("bytes")).expect("decodes"),
        IntentData::new(Some(true), Some(false))
    );
    let (_, _, seq) = target_row(&retry, "win-a").expect("reauthorized");
    assert_eq!(seq, Some(3));
    let rebound = engine.retained_states.get(&key).expect("binding");
    assert!(!rebound.spent, "replacement binding is fresh");
    assert_eq!(rebound.disposition, AttemptOutcome::Unattempted);
    let meta = rebound.sticky_meta.as_ref().expect("meta");
    assert_eq!(meta.command_seq, 3);
    assert_eq!(meta.prior_restored_sticky, Some(false));
    assert_eq!(
        meta.requested_save,
        IntentData::new(Some(true), Some(false))
    );
}

use crate::world_report::{FrameSetter, StateAttempt};

fn sticky_attempt(
    window: &str,
    target: bool,
    revision: u64,
    setter: FrameSetter,
    pre: Option<bool>,
    post: Option<bool>,
) -> StateAttempt {
    StateAttempt {
        window: WindowId(window.to_owned()),
        property: StateProperty::Sticky,
        target,
        revision,
        setter,
        pre,
        post,
        attempt_ms: 7,
    }
}

fn sticky_fact(
    window: &str,
    output: &str,
    x: i32,
    sticky: Option<bool>,
) -> crate::world_report::WindowFact {
    let mut row = fact(window, output, x);
    row.sticky = sticky;
    row
}

fn binding_rev(engine: &WorldEngine, window: &str) -> u64 {
    engine
        .retained_states
        .get(&(WindowId(window.to_owned()), StateProperty::Sticky))
        .expect("binding")
        .target
        .revision
}

#[test]
fn native_on_applied_marks_done_no_extra_save() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let on_reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert_eq!(on_reply.intent_writes.len(), 1);
    let rev = binding_rev(&engine, "win-a");
    let mut proof = observe(3, vec![sticky_fact("win-a", "out-a", 0, Some(true))]);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Accepted,
        Some(false),
        Some(true),
    )];
    let reply = accepted(&mut engine, &proof);
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
    assert!(binding.spent);
    let meta = binding.sticky_meta.as_ref().expect("meta");
    assert!(meta.followup_done, "on applied marks done");
    assert_eq!(meta.command_seq, 1);
    assert_eq!(reply.intent_writes.len(), 0, "no extra save on applied");
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false)
    );
}

#[test]
fn native_off_unmatched_keeps_float_no_premature_tile() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    // Raw clears to false but the attempt revision mismatches: never unlocks.
    let mut unmatched = observe(4, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    unmatched.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev + 100,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &unmatched);
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_ne!(binding.disposition, AttemptOutcome::Applied);
    assert!(!binding.sticky_meta.as_ref().expect("meta").followup_done);
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(member.floating, "holds sticky-float, no premature tile");
}

#[test]
fn native_off_matched_applied_lands_dest_tile() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let off_reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    assert_eq!(off_reply.intent_writes.len(), 1);
    let rev = binding_rev(&engine, "win-a");
    let dest = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding")
        .sticky_meta
        .as_ref()
        .expect("meta")
        .dest
        .clone();
    let mut proof = observe(4, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let reply = accepted(&mut engine, &proof);
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
    assert!(binding.sticky_meta.as_ref().expect("meta").followup_done);
    assert_eq!(reply.intent_writes.len(), 0, "no extra save on followup");
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, dest);
    assert!(!member.floating, "prior false lands tile");
    assert!(
        !engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn native_off_prior_true_applied_lands_float() {
    let mut engine = native_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![float_fact("win-a", "out-a", 0, 0)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![float_fact("win-a", "out-a", 0, 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![float_fact("win-a", "out-a", 0, 0)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    let mut proof = observe(4, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    // Keep the float origin visible: transient float fact still classifies float.
    proof.windows[0].transient = Some(true);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &proof);
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(member.floating, "prior true honors float");
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn native_off_adopted_holds_then_applied_lands_float() {
    let mut engine = native_engine();
    let mut adopted = fact("win-a", "out-a", 0);
    adopted.sticky = Some(true);
    accepted(&mut engine, &bootstrap(vec![adopted.clone()]));
    let off_reply = accepted(
        &mut engine,
        &cmd(2, 1, vec![adopted.clone()], set_sticky("win-a", false)),
    );
    assert_eq!(off_reply.intent_writes.len(), 1);
    assert!(
        engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned())),
        "adopted provenance retained while awaiting proof"
    );
    let rev = binding_rev(&engine, "win-a");
    // Raw clears early with an unmatched revision: must not tile prematurely.
    let mut early = observe(3, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    early.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev + 50,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &early);
    let held = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(
        engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned())),
        "semantic hold survives early raw clear"
    );
    let _ = held;
    // Matched proof lands the requested float.
    let mut proof = observe(4, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    proof.windows[0].transient = Some(true);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &proof);
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(member.floating, "adopted off lands normal float");
}

#[test]
fn native_toggle_float_adopted_applied_tiles() {
    let mut engine = native_engine();
    let mut adopted = fact("win-a", "out-a", 0);
    adopted.sticky = Some(true);
    accepted(&mut engine, &bootstrap(vec![adopted.clone()]));
    accepted(
        &mut engine,
        &cmd(2, 1, vec![adopted.clone()], toggle_float("win-a")),
    );
    let rev = binding_rev(&engine, "win-a");
    let mut proof = observe(3, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &proof);
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(
        !member.floating,
        "toggle-float clear always lands tile base"
    );
}

#[test]
fn native_off_floating_dest_lands_floated_with_tile_base() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
                tiled: false,
            },
        ),
    );
    let off_reply = accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    assert_eq!(off_reply.intent_writes.len(), 1);
    let rev = binding_rev(&engine, "win-a");
    let dest = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding")
        .sticky_meta
        .as_ref()
        .expect("meta")
        .dest
        .clone();
    let mut proof = observe(5, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &proof);
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, dest);
    assert!(
        engine.engine.session(&dest).is_none(),
        "floating dest forces no session"
    );
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned())),
        "tile base carries no explicit-float mark"
    );
    assert_eq!(
        engine
            .sticky_saved
            .get(&WindowId("win-a".to_owned()))
            .copied(),
        Some(IntentData::new(Some(false), None)),
        "tile base saved"
    );
}

#[test]
fn native_off_cross_workspace_prior_false_lands_dest_tile() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            WorldCommand::SelectTrailing { output: None },
        ),
    );
    let dest_view = engine
        .effective_view(&OutputId("out-a".to_owned()))
        .expect("trailing view");
    let off_reply = accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    assert!(off_reply.command_result.is_none());
    let meta = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding")
        .sticky_meta
        .as_ref()
        .expect("meta")
        .clone();
    assert_ne!(meta.home, meta.dest, "cross-workspace setup");
    assert_eq!(meta.dest.workspace.0, dest_view);
    let rev = binding_rev(&engine, "win-a");
    let mut proof = observe(5, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &proof);
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, meta.dest, "lands at frozen command dest");
    assert!(!member.floating, "prior false cross lands tile");
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert!(binding.sticky_meta.as_ref().expect("meta").followup_done);
}

#[test]
fn native_off_cross_workspace_prior_true_lands_dest_float() {
    let mut engine = native_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![float_fact("win-a", "out-a", 0, 0)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![float_fact("win-a", "out-a", 0, 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![float_fact("win-a", "out-a", 0, 0)],
            WorldCommand::SelectTrailing { output: None },
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![float_fact("win-a", "out-a", 0, 0)],
            set_sticky("win-a", false),
        ),
    );
    let meta = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding")
        .sticky_meta
        .as_ref()
        .expect("meta")
        .clone();
    assert_ne!(meta.home, meta.dest, "cross-workspace setup");
    let rev = binding_rev(&engine, "win-a");
    let mut proof = observe(5, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    proof.windows[0].transient = Some(true);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &proof);
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, meta.dest);
    assert!(member.floating, "prior true cross lands float");
}

#[test]
fn native_off_held_and_minimized_retain_proof_then_recover_once() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    // Incomplete inventory attempt: spent latches, disposition ambiguous.
    let mut incomplete = observe(4, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    incomplete.inventory_complete = false;
    incomplete.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &incomplete);
    let mid = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_ne!(mid.disposition, AttemptOutcome::Applied);
    assert!(!mid.sticky_meta.as_ref().expect("meta").followup_done);
    // Minimized attempt on the same spent binding never unlocks the tile.
    let mut minimized_fact = sticky_fact("win-a", "out-a", 0, Some(false));
    minimized_fact.minimized = Some(true);
    let mut minimized = observe(5, vec![minimized_fact]);
    minimized.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Threw,
        None,
        None,
    )];
    accepted(&mut engine, &minimized);
    let still = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(still.floating, "no premature tile while unreadable");
    // Healthy record-free recovery settles exactly once.
    let recovered = accepted(
        &mut engine,
        &observe(6, vec![sticky_fact("win-a", "out-a", 0, Some(false))]),
    );
    assert_eq!(recovered.intent_writes.len(), 0);
    let done = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(done.disposition, AttemptOutcome::Applied);
    assert!(done.sticky_meta.as_ref().expect("meta").followup_done);
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(!member.floating, "healthy recovery lands tile once");
}

#[test]
fn native_off_duplicate_attempt_no_repeat() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    let mut proof = observe(4, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &proof);
    let first_home = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();
    let mut duplicate = observe(5, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
    duplicate.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let reply = accepted(&mut engine, &duplicate);
    assert_eq!(reply.intent_writes.len(), 0);
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert!(binding.sticky_meta.as_ref().expect("meta").followup_done);
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, first_home);
    assert!(!member.floating);
}

fn refused_proof(
    seq: u64,
    windows: Vec<crate::world_report::WindowFact>,
    window: &str,
    target: bool,
    rev: u64,
    pre_post: bool,
) -> WorldReport {
    let mut proof = observe(seq, windows);
    proof.state_attempts = vec![sticky_attempt(
        window,
        target,
        rev,
        FrameSetter::Refused,
        Some(pre_post),
        Some(pre_post),
    )];
    proof
}

fn rollback_binding(
    engine: &WorldEngine,
    window: &str,
) -> crate::world_state::RetainedStateBinding {
    engine
        .retained_states
        .get(&(WindowId(window.to_owned()), StateProperty::Sticky))
        .expect("binding")
        .clone()
}

#[test]
fn native_on_refused_proven_null_clears_and_returns_tile() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let home_before = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();
    let on_reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert_eq!(on_reply.intent_writes.len(), 1);
    assert!(
        engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned())),
        "producer holds sticky-float runtime"
    );
    let rev = binding_rev(&engine, "win-a");
    let reply = accepted(
        &mut engine,
        &refused_proof(
            3,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            true,
            rev,
            false,
        ),
    );
    assert_eq!(
        reply.intent_writes,
        vec![IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: None,
        }],
        "no prior means null-clear rollback"
    );
    assert!(
        !engine
            .sticky_saved
            .contains_key(&WindowId("win-a".to_owned())),
        "prior absence restores absence"
    );
    assert!(
        !engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned())),
        "sticky provenance rolled back"
    );
    assert!(
        !engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, home_before);
    assert!(!member.floating, "on refusal returns original tile");
    assert_eq!(
        engine
            .staged
            .facts
            .get(&WindowId("win-a".to_owned()))
            .and_then(|fact| fact.sticky),
        Some(false),
        "raw facts are never rewritten"
    );
    let binding = rollback_binding(&engine, "win-a");
    assert!(binding.spent);
    assert_eq!(binding.disposition, AttemptOutcome::RefusedProven);
    assert!(binding.sticky_meta.as_ref().expect("meta").followup_done);
    let (_, authority, seq) = target_row(&reply, "win-a").expect("target retained");
    assert_eq!(seq, Some(1));
    assert_ne!(authority, ScopeAuthority::Controlled);
    // A duplicate of the original proof emits nothing further.
    let again = accepted(
        &mut engine,
        &refused_proof(
            4,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            true,
            rev,
            false,
        ),
    );
    assert!(again.intent_writes.is_empty(), "one proof, one write");
}

#[test]
fn native_off_refused_proven_restores_prior_and_keeps_sticky() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let prior = IntentData::new(Some(true), Some(false));
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&prior)
    );
    let off_reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    assert_eq!(off_reply.intent_writes.len(), 1);
    let rev = binding_rev(&engine, "win-a");
    let home = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();
    let reply = accepted(
        &mut engine,
        &refused_proof(
            4,
            vec![sticky_fact("win-a", "out-a", 0, Some(true))],
            "win-a",
            false,
            rev,
            true,
        ),
    );
    assert_eq!(
        reply.intent_writes,
        vec![IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some(encode_intent_data(&prior)),
        }],
        "rollback restores the exact prior record"
    );
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&prior)
    );
    assert_eq!(
        engine.restored_sticky.get(&WindowId("win-a".to_owned())),
        Some(&false),
        "off refusal keeps prior sticky"
    );
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, home);
    assert!(member.floating, "sticky runtime preserved");
    let binding = rollback_binding(&engine, "win-a");
    assert_eq!(binding.disposition, AttemptOutcome::RefusedProven);
    assert!(binding.sticky_meta.as_ref().expect("meta").followup_done);
    let (_, authority, _) = target_row(&reply, "win-a").expect("target retained");
    assert_ne!(authority, ScopeAuthority::Controlled);
}

#[test]
fn native_refused_restores_all_nine_prior_shapes() {
    let priors = [
        IntentData::new(None, None),
        IntentData::new(None, Some(false)),
        IntentData::new(None, Some(true)),
        IntentData::new(Some(false), None),
        IntentData::new(Some(false), Some(false)),
        IntentData::new(Some(false), Some(true)),
        IntentData::new(Some(true), None),
        IntentData::new(Some(true), Some(false)),
        IntentData::new(Some(true), Some(true)),
    ];
    for prior in priors {
        let mut engine = native_engine();
        // Establish the prior through the real import producer: a single
        // sticky-bearing, float-only, or null/null row on a healthy fact.
        // Sticky-bearing and float-true priors need a healthy float
        // exception for admission; tile and null priors admit on tile.
        let float_healthy = prior.sticky_prior.is_some() || prior.float == Some(true);
        let mut base = if float_healthy {
            bootstrap(vec![float_fact("win-a", "out-a", 0, 0)])
        } else {
            bootstrap(vec![fact("win-a", "out-a", 0)])
        };
        base.intent_records = vec![IntentRecord {
            win: WindowId("win-a".to_owned()),
            data: encode_intent_data(&prior),
        }];
        accepted(&mut engine, &base);
        assert_eq!(
            engine.sticky_saved.get(&WindowId("win-a".to_owned())),
            Some(&prior),
            "import establishes exact prior {prior:?}"
        );
        let sticky_now = prior.sticky_prior.is_some();
        if let Some(expected) = prior.sticky_prior {
            assert_eq!(
                engine.restored_sticky.get(&WindowId("win-a".to_owned())),
                Some(&expected),
                "sticky-bearing import admits exact prior {prior:?}"
            );
        }
        // Sticky subjects refuse OFF (proof keeps raw true); the rest
        // refuse ON (proof keeps raw false).
        let command = set_sticky("win-a", !sticky_now);
        let fact_row = if sticky_now {
            sticky_fact("win-a", "out-a", 0, Some(true))
        } else if float_healthy {
            let mut row = float_fact("win-a", "out-a", 0, 0);
            row.sticky = Some(false);
            row
        } else {
            fact("win-a", "out-a", 0)
        };
        accepted(&mut engine, &cmd(2, 1, vec![fact_row.clone()], command));
        let home = rollback_binding(&engine, "win-a")
            .sticky_meta
            .expect("meta")
            .home
            .clone();
        let rev = binding_rev(&engine, "win-a");
        let raw = if sticky_now { Some(true) } else { Some(false) };
        let mut row = fact_row.clone();
        row.sticky = raw;
        let reply = accepted(
            &mut engine,
            &refused_proof(3, vec![row], "win-a", !sticky_now, rev, sticky_now),
        );
        assert_eq!(
            reply.intent_writes,
            vec![IntentWrite {
                win: WindowId("win-a".to_owned()),
                data: Some(encode_intent_data(&prior)),
            }],
            "rollback restores exact prior {prior:?}"
        );
        assert_eq!(
            engine.sticky_saved.get(&WindowId("win-a".to_owned())),
            Some(&prior),
            "saved knowledge restored on commit {prior:?}"
        );
        let member = engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("member");
        assert_eq!(member.home, home, "home restored {prior:?}");
        if sticky_now {
            assert!(
                engine
                    .restored_sticky
                    .contains_key(&WindowId("win-a".to_owned())),
                "off refusal keeps prior sticky {prior:?}"
            );
            assert!(member.floating, "sticky runtime {prior:?}");
        } else if prior.float == Some(true) {
            assert!(member.floating, "original float returns {prior:?}");
            assert!(
                engine
                    .restored_floats
                    .contains(&WindowId("win-a".to_owned())),
                "float provenance restored {prior:?}"
            );
        } else {
            assert!(!member.floating, "original tile returns {prior:?}");
        }
        let binding = rollback_binding(&engine, "win-a");
        assert_eq!(
            binding.disposition,
            AttemptOutcome::RefusedProven,
            "{prior:?}"
        );
        assert!(binding.sticky_meta.as_ref().expect("meta").followup_done);
    }
}

#[test]
fn native_on_refused_restores_prior_ordinary_toggle_float_save() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    // An ordinary ToggleFloat stages the tile-base... float save first.
    let floated = accepted(
        &mut engine,
        &cmd(2, 1, vec![fact("win-a", "out-a", 0)], toggle_float("win-a")),
    );
    assert_eq!(floated.intent_writes.len(), 1);
    let prior = IntentData::new(Some(true), None);
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&prior)
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![float_fact("win-a", "out-a", 0, 0)],
            set_sticky("win-a", true),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    let mut raw = float_fact("win-a", "out-a", 0, 0);
    raw.sticky = Some(false);
    let reply = accepted(
        &mut engine,
        &refused_proof(4, vec![raw], "win-a", true, rev, false),
    );
    assert_eq!(
        reply.intent_writes,
        vec![IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some(encode_intent_data(&prior)),
        }]
    );
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&prior)
    );
    assert!(
        engine
            .restored_floats
            .contains(&WindowId("win-a".to_owned()))
    );
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(member.floating, "original float returns");
}

#[test]
fn native_ambiguous_keeps_requested_without_retry() {
    // Threw proof stays ambiguous: requested record kept, no save, no
    // followup, runtime still holds the sticky float.
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let requested = IntentData::new(Some(false), None);
    let rev = binding_rev(&engine, "win-a");
    let mut threw = observe(4, vec![sticky_fact("win-a", "out-a", 0, Some(true))]);
    threw.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Threw,
        Some(true),
        Some(true),
    )];
    let reply = accepted(&mut engine, &threw);
    assert!(reply.intent_writes.is_empty(), "ambiguous mints no save");
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&requested),
        "requested record retained"
    );
    let binding = rollback_binding(&engine, "win-a");
    assert_eq!(binding.disposition, AttemptOutcome::PossiblyExecuted);
    assert!(!binding.sticky_meta.as_ref().expect("meta").followup_done);
    assert!(
        engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("member")
            .floating,
        "no premature tile on ambiguity"
    );
    let (_, authority, _) = target_row(&reply, "win-a").expect("target retained");
    assert_ne!(authority, ScopeAuthority::Controlled);
    // A mismatched revision never unlocks the binding either.
    let mut mismatch = observe(5, vec![sticky_fact("win-a", "out-a", 0, Some(true))]);
    mismatch.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev + 100,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let reply = accepted(&mut engine, &mismatch);
    assert!(reply.intent_writes.is_empty());
    let binding = rollback_binding(&engine, "win-a");
    assert!(binding.spent, "earlier capture latches spent");
    assert_eq!(
        binding.disposition,
        AttemptOutcome::PossiblyExecuted,
        "wrong revision never resolves"
    );
    assert_eq!(
        binding.pending.as_ref().map(|pending| pending.setter),
        Some(FrameSetter::Threw),
        "wrong revision never replaces retained proof"
    );
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&requested)
    );
    // An unreadable (incomplete-inventory) exact refusal also stays
    // ambiguous with the requested record intact.
    let mut gapped = observe(6, vec![sticky_fact("win-a", "out-a", 0, Some(true))]);
    gapped.inventory_complete = false;
    gapped.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Refused,
        Some(true),
        Some(true),
    )];
    let reply = accepted(&mut engine, &gapped);
    assert!(reply.intent_writes.is_empty());
    assert_eq!(
        rollback_binding(&engine, "win-a").disposition,
        AttemptOutcome::PossiblyExecuted
    );
    assert_eq!(
        engine.sticky_saved.get(&WindowId("win-a".to_owned())),
        Some(&requested)
    );
}

#[test]
fn native_refused_overlay_held_then_healthy_recovery_emits_once() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    // A fullscreen overlay still proves refusal (exact tuple against the
    // readable sticky fact) but the followup holds: no write, proof kept.
    let mut overlay = sticky_fact("win-a", "out-a", 0, Some(true));
    overlay.fullscreen = Some(true);
    let held = accepted(
        &mut engine,
        &refused_proof(4, vec![overlay], "win-a", false, rev, true),
    );
    assert_eq!(
        rollback_binding(&engine, "win-a").disposition,
        AttemptOutcome::RefusedProven
    );
    assert!(held.intent_writes.is_empty(), "held proof emits nothing");
    assert!(
        !rollback_binding(&engine, "win-a")
            .sticky_meta
            .expect("meta")
            .followup_done
    );
    // Healthy record-free recovery rolls back exactly once.
    let recovered = accepted(
        &mut engine,
        &observe(5, vec![sticky_fact("win-a", "out-a", 0, Some(true))]),
    );
    let prior = IntentData::new(Some(true), Some(false));
    assert_eq!(
        recovered.intent_writes,
        vec![IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some(encode_intent_data(&prior)),
        }]
    );
    assert!(
        rollback_binding(&engine, "win-a")
            .sticky_meta
            .expect("meta")
            .followup_done
    );
    let again = accepted(
        &mut engine,
        &observe(6, vec![sticky_fact("win-a", "out-a", 0, Some(true))]),
    );
    assert!(again.intent_writes.is_empty(), "recovery emits once");
}

#[test]
fn native_rollback_same_window_replacement_owns_new_intent() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let first_rev = binding_rev(&engine, "win-a");
    // A newer same-window command replaces the binding (new meta, new
    // revision); the stale proof for the old revision never acts.
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let second_rev = binding_rev(&engine, "win-a");
    assert_ne!(first_rev, second_rev);
    let stale = accepted(
        &mut engine,
        &refused_proof(
            4,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            true,
            first_rev,
            false,
        ),
    );
    assert!(
        stale.intent_writes.is_empty(),
        "old proof cannot act on new intent"
    );
    assert_eq!(binding_rev(&engine, "win-a"), second_rev);
    assert!(
        !rollback_binding(&engine, "win-a")
            .sticky_meta
            .expect("meta")
            .followup_done
    );
    // The current proof rolls back the current intent only.
    let reply = accepted(
        &mut engine,
        &refused_proof(
            5,
            vec![sticky_fact("win-a", "out-a", 0, Some(true))],
            "win-a",
            false,
            second_rev,
            true,
        ),
    );
    let prior = IntentData::new(Some(true), Some(false));
    assert_eq!(
        reply.intent_writes,
        vec![IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some(encode_intent_data(&prior)),
        }]
    );
}

#[test]
fn native_rollback_unrelated_newer_command_preserved() {
    let mut engine = native_engine();
    accepted(
        &mut engine,
        &bootstrap(vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)]),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    // A newer command on an unrelated window shares the proof report; the
    // rollback still drains alongside the fresh save.
    let mut proof = refused_proof(
        4,
        vec![
            sticky_fact("win-a", "out-a", 0, Some(true)),
            fact("win-b", "out-a", 400),
        ],
        "win-a",
        false,
        rev,
        true,
    );
    proof.command_seq = Some(3);
    proof.command = Some(set_sticky("win-b", true));
    let reply = accepted(&mut engine, &proof);
    let prior = IntentData::new(Some(true), Some(false));
    let rollback = IntentWrite {
        win: WindowId("win-a".to_owned()),
        data: Some(encode_intent_data(&prior)),
    };
    let fresh = reply
        .intent_writes
        .iter()
        .find(|write| write.win == WindowId("win-b".to_owned()))
        .expect("fresh unrelated save emits");
    assert_eq!(
        fresh
            .data
            .as_deref()
            .map(|raw| { crate::intent_data::decode_intent_data(raw).expect("decodes") }),
        Some(IntentData::new(Some(true), Some(false)))
    );
    assert!(
        reply.intent_writes.contains(&rollback),
        "unrelated command never erases rollback: {:?}",
        reply.intent_writes
    );
}

#[test]
fn native_rollback_consumed_report_still_emits() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    // A consumed (redelivered) command envelope carrying fresh refusal
    // proof still rolls back: consumption suppresses command saves only.
    let mut proof = refused_proof(
        4,
        vec![sticky_fact("win-a", "out-a", 0, Some(true))],
        "win-a",
        false,
        rev,
        true,
    );
    proof.command_seq = Some(2);
    proof.command = Some(set_sticky("win-a", false));
    match engine.handle_report_legacy(&proof) {
        LegacyWorldOutcome::CommandConsumed(reply) => {
            let prior = IntentData::new(Some(true), Some(false));
            assert_eq!(
                reply.intent_writes,
                vec![IntentWrite {
                    win: WindowId("win-a".to_owned()),
                    data: Some(encode_intent_data(&prior)),
                }],
                "consumed suppresses command saves, not rollback"
            );
        }
        other => panic!("expected consumed, got {other:?}"),
    }
    assert!(
        rollback_binding(&engine, "win-a")
            .sticky_meta
            .expect("meta")
            .followup_done
    );
}

#[test]
fn native_toggle_float_refused_keeps_sticky_tile_base() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(3, 2, vec![fact("win-a", "out-a", 0)], toggle_float("win-a")),
    );
    let rev = binding_rev(&engine, "win-a");
    let reply = accepted(
        &mut engine,
        &refused_proof(
            4,
            vec![sticky_fact("win-a", "out-a", 0, Some(true))],
            "win-a",
            false,
            rev,
            true,
        ),
    );
    // A toggle-float clear refuses like OFF: prior sticky kept, and the
    // rollback restores the on-command record (tile base requested).
    let prior = IntentData::new(Some(true), Some(false));
    assert_eq!(
        reply.intent_writes,
        vec![IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some(encode_intent_data(&prior)),
        }]
    );
    assert!(
        engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("member")
            .floating
    );
}

#[test]
fn native_off_cross_workspace_refused_rolls_back_home() {
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            WorldCommand::SelectTrailing { output: None },
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let meta = rollback_binding(&engine, "win-a")
        .sticky_meta
        .expect("meta");
    assert_ne!(meta.home, meta.dest, "cross-workspace setup");
    let rev = binding_rev(&engine, "win-a");
    let reply = accepted(
        &mut engine,
        &refused_proof(
            5,
            vec![sticky_fact("win-a", "out-a", 0, Some(true))],
            "win-a",
            false,
            rev,
            true,
        ),
    );
    let prior = IntentData::new(Some(true), Some(false));
    assert_eq!(
        reply.intent_writes,
        vec![IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some(encode_intent_data(&prior)),
        }]
    );
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, meta.home, "lands at frozen home on refusal");
    assert!(member.floating, "prior sticky keeps float");
    assert!(
        rollback_binding(&engine, "win-a")
            .sticky_meta
            .expect("meta")
            .followup_done
    );
}

#[test]
fn native_rollback_reserve_rejects_before_mutation_and_small_retry_rolls_back() {
    use crate::world_engine::KIND_REPLY_SIZE;
    use crate::world_report::{WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES};
    use crate::world_sizing::{max_intent_rollback_bound, reply_upper_bound, report_estimate};
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let on_reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert_eq!(on_reply.intent_writes.len(), 1);
    let rev = binding_rev(&engine, "win-a");
    let (value, authority, seq) = target_row(&on_reply, "win-a").expect("pending target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert_eq!(engine.rollback_reserve_count(), 1);
    let empty = observe(3, vec![fact("win-a", "out-a", 0)]);
    assert!(empty.state_attempts.is_empty());
    assert_eq!(engine.rollback_reserve_count(), 1);
    assert_eq!(max_intent_rollback_bound(1), 196);
    let logical = engine.engine.domain_keys().len();
    let staged = engine.staged.facts.len() + engine.staged.held_windows.len();
    let retained = engine.retained_publish_footprint();
    let proof_of = |windows: Vec<crate::world_report::WindowFact>| {
        let mut report = report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            windows,
            vec![],
            "",
            None,
            None,
        );
        report.state_attempts = vec![sticky_attempt(
            "win-a",
            true,
            rev,
            FrameSetter::Accepted,
            Some(false),
            Some(false),
        )];
        report
    };
    let bound_of = |windows: &[crate::world_report::WindowFact]| {
        let report = proof_of(windows.to_vec());
        reply_upper_bound(&report, 0, logical, 1, staged, retained)
    };
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    for i in 0..2200 {
        windows.push(fact(&format!("win-pad-{i:05}"), "out-a", 0));
    }
    while bound_of(&windows) > WORLD_MAX_REPLY_BYTES {
        windows.pop();
    }
    let deficit = WORLD_MAX_REPLY_BYTES
        .saturating_sub(195)
        .saturating_sub(bound_of(&windows))
        + 2;
    windows[2].title = Some("t".repeat(deficit));
    let report = proof_of(windows);
    let without = reply_upper_bound(&report, 0, logical, 1, staged, retained);
    assert!(
        report_estimate(&report) <= WORLD_MAX_REQUEST_BYTES,
        "request itself fits; only the reply bound trips"
    );
    assert!(without <= WORLD_MAX_REPLY_BYTES, "reserveless bound fits");
    assert!(
        without.saturating_add(max_intent_rollback_bound(1)) > WORLD_MAX_REPLY_BYTES,
        "rollback reserve is the discriminator"
    );
    let revision = engine.state_revision;
    let binding_before = rollback_binding(&engine, "win-a");
    assert!(!binding_before.spent);
    assert!(
        !binding_before
            .sticky_meta
            .as_ref()
            .expect("meta")
            .followup_done
    );
    let restored = engine.restored_sticky.clone();
    let saved = engine.sticky_saved.clone();
    let floats = engine.restored_floats.clone();
    let highwater = engine.command_highwater;
    let queued = engine.sticky_rollback_saves.clone();
    match engine.handle_report_legacy(&report) {
        LegacyWorldOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REPLY_SIZE),
        other => panic!("expected reply-oversize, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision, "refusal mutates nothing");
    assert_eq!(rollback_binding(&engine, "win-a"), binding_before);
    assert_eq!(engine.restored_sticky, restored);
    assert_eq!(engine.sticky_saved, saved);
    assert_eq!(engine.restored_floats, floats);
    assert_eq!(engine.command_highwater, highwater);
    assert_eq!(engine.sticky_rollback_saves, queued);
    let retry = refused_proof(
        3,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        true,
        rev,
        false,
    );
    let pre = reply_upper_bound(&retry, 0, logical, 1, staged, retained)
        .saturating_add(max_intent_rollback_bound(1));
    assert!(pre <= WORLD_MAX_REPLY_BYTES, "retry fits the reserve");
    let reply = accepted(&mut engine, &retry);
    assert_eq!(
        reply.intent_writes,
        vec![IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: None,
        }]
    );
    let done = rollback_binding(&engine, "win-a");
    assert_eq!(done.disposition, AttemptOutcome::RefusedProven);
    assert!(done.sticky_meta.as_ref().expect("meta").followup_done);
    let actual = crate::world_sizing::encode_reply_canonical(&reply).len();
    assert!(actual <= pre, "retry reply {actual} fits pre-bound {pre}");
}

#[test]
fn native_rollback_queue_cap_holds_proof_and_replace_still_wins() {
    use crate::world_report::MAX_INTENT_WRITES;
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    for i in 0..MAX_INTENT_WRITES {
        engine.sticky_rollback_saves.insert(
            WindowId(format!("win-q-{i:05}")),
            IntentWrite {
                win: WindowId(format!("win-q-{i:05}")),
                data: None,
            },
        );
    }
    assert_eq!(engine.sticky_rollback_saves.len(), MAX_INTENT_WRITES);
    accepted(
        &mut engine,
        &refused_proof(
            3,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            true,
            rev,
            false,
        ),
    );
    assert!(
        !rollback_binding(&engine, "win-a")
            .sticky_meta
            .as_ref()
            .expect("meta")
            .followup_done,
        "full queue never silently drops proof"
    );
    assert!(
        !engine
            .sticky_rollback_saves
            .contains_key(&WindowId("win-a".to_owned())),
        "no queue growth past the cap"
    );
    // Same-key replacement stays allowed at the cap: refill with a stale
    // win-a entry plus unrelated filler, then the next proof replaces it.
    engine.sticky_rollback_saves.clear();
    engine.sticky_rollback_saves.insert(
        WindowId("win-a".to_owned()),
        IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some("stale".to_owned()),
        },
    );
    for i in 0..MAX_INTENT_WRITES - 1 {
        engine.sticky_rollback_saves.insert(
            WindowId(format!("win-q-{i:05}")),
            IntentWrite {
                win: WindowId(format!("win-q-{i:05}")),
                data: None,
            },
        );
    }
    assert_eq!(engine.sticky_rollback_saves.len(), MAX_INTENT_WRITES);
    let reply = accepted(
        &mut engine,
        &refused_proof(
            4,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            true,
            rev,
            false,
        ),
    );
    assert!(
        reply
            .intent_writes
            .iter()
            .any(|write| write.win.0 == "win-a")
    );
    assert!(
        rollback_binding(&engine, "win-a")
            .sticky_meta
            .as_ref()
            .expect("meta")
            .followup_done
    );
}

#[test]
fn native_rollback_ledger_cap_holds_proof_for_retry() {
    use crate::world_report::MAX_INTENT_RECORDS;
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    // OFF snapshots the ON sticky provenance, then the runtime entry is
    // pruned (departure fence) so the refusal restore would regrow the
    // joint ledger past the cap.
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    engine.restored_sticky.remove(&WindowId("win-a".to_owned()));
    let need = MAX_INTENT_RECORDS.saturating_sub(engine.intent_ledger_used());
    for i in 0..need {
        engine
            .restored_sticky
            .insert(WindowId(format!("win-l-{i:05}")), false);
    }
    assert_eq!(engine.intent_ledger_used(), MAX_INTENT_RECORDS);
    accepted(
        &mut engine,
        &refused_proof(
            4,
            vec![sticky_fact("win-a", "out-a", 0, Some(true))],
            "win-a",
            false,
            rev,
            true,
        ),
    );
    assert!(
        !rollback_binding(&engine, "win-a")
            .sticky_meta
            .as_ref()
            .expect("meta")
            .followup_done,
        "ledger breach holds proof for retry"
    );
    assert!(
        !engine
            .sticky_rollback_saves
            .contains_key(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn native_off_overlay_deferred_restick_holds_until_fresh_false() {
    // Overlay-deferred Applied proof must not unlock the dependent tile
    // while the current raw still reads sticky: the generic settlement
    // ignores the fullscreen overlay (readability only gates
    // min/held/domain), so the binding turns Applied while the followup
    // waits. An external restick once the overlay lifts retains that
    // stale proof with no landing, no save, and no completion; only a
    // record-free fresh `Some(false)` recovers.
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    let dest = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding")
        .sticky_meta
        .as_ref()
        .expect("meta")
        .dest
        .clone();
    // Overlay proof: readable sticky false classifies Applied, but the
    // fullscreen overlay defers the dependent followup.
    let mut overlay = sticky_fact("win-a", "out-a", 0, Some(false));
    overlay.fullscreen = Some(true);
    let mut proof = observe(4, vec![overlay]);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let proof_reply = accepted(&mut engine, &proof);
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
    assert!(binding.spent);
    assert!(
        !binding.sticky_meta.as_ref().expect("meta").followup_done,
        "overlay defers the dependent landing"
    );
    assert!(proof_reply.intent_writes.is_empty());
    let home_before = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();
    // External restick once the overlay lifts: the stale Applied proof
    // must not clear semantic sticky or tile.
    let restick = accepted(
        &mut engine,
        &observe(5, vec![sticky_fact("win-a", "out-a", 0, Some(true))]),
    );
    let held = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(held.disposition, AttemptOutcome::Applied);
    assert!(
        held.spent,
        "original attempt stays spent with no new target"
    );
    assert_eq!(held.target.revision, rev, "no new target minted");
    assert_eq!(held.sticky_meta.as_ref().expect("meta").command_seq, 2);
    assert!(
        !held.sticky_meta.as_ref().expect("meta").followup_done,
        "restick retains proof with no completion"
    );
    assert!(
        engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned())),
        "semantic sticky survives the restick"
    );
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, home_before, "no home change on restick");
    assert!(member.floating, "no tile landing on restick");
    assert!(restick.intent_writes.is_empty(), "no save on restick");
    let (_, authority, seq) = target_row(&restick, "win-a").expect("target retained");
    assert_eq!(seq, Some(2), "spent target keeps the original command");
    assert_ne!(
        authority,
        ScopeAuthority::Controlled,
        "spent target never Controlled"
    );
    // Record-free fresh false recovers exactly once with no extra save.
    let recovered = accepted(
        &mut engine,
        &observe(6, vec![sticky_fact("win-a", "out-a", 0, Some(false))]),
    );
    let done = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(done.disposition, AttemptOutcome::Applied);
    assert!(done.sticky_meta.as_ref().expect("meta").followup_done);
    assert_eq!(
        recovered.intent_writes.len(),
        0,
        "no extra save on followup"
    );
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert_eq!(member.home, dest);
    assert!(!member.floating, "fresh false lands the requested tile");
    assert!(
        !engine
            .restored_sticky
            .contains_key(&WindowId("win-a".to_owned()))
    );
    let (_, authority, seq) = target_row(&recovered, "win-a").expect("target retained");
    assert_eq!(seq, Some(2));
    assert_ne!(
        authority,
        ScopeAuthority::Controlled,
        "spent target never Controlled"
    );
}

#[test]
fn native_toggle_float_overlay_deferred_null_holds_until_false() {
    // ToggleFloat clear defers the same way under an overlay; a current
    // unknown (`sticky: null`) after the overlay lifts must hold like a
    // restick, and only a fresh `Some(false)` lands the tile base.
    let mut engine = native_engine();
    let mut adopted = fact("win-a", "out-a", 0);
    adopted.sticky = Some(true);
    accepted(&mut engine, &bootstrap(vec![adopted.clone()]));
    accepted(
        &mut engine,
        &cmd(2, 1, vec![adopted.clone()], toggle_float("win-a")),
    );
    let rev = binding_rev(&engine, "win-a");
    let mut overlay = sticky_fact("win-a", "out-a", 0, Some(false));
    overlay.fullscreen = Some(true);
    let mut proof = observe(3, vec![overlay]);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &proof);
    let deferred = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(deferred.disposition, AttemptOutcome::Applied);
    assert!(
        !deferred.sticky_meta.as_ref().expect("meta").followup_done,
        "overlay defers the toggle-float landing"
    );
    // Unknown current: no landing, no completion.
    let win = WindowId("win-a".to_owned());
    let home_before = engine.members.get(&win).expect("member").home.clone();
    let nulled = accepted(
        &mut engine,
        &observe(4, vec![sticky_fact("win-a", "out-a", 0, None)]),
    );
    let held = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert!(
        !held.sticky_meta.as_ref().expect("meta").followup_done,
        "null current retains proof"
    );
    assert!(nulled.intent_writes.is_empty());
    assert!(
        engine.restored_sticky.contains_key(&win),
        "semantic sticky survives the unknown observation"
    );
    assert_eq!(
        engine.sticky_saved.get(&win).copied(),
        Some(IntentData::new(Some(false), None)),
        "requested tile base saved, never rewritten"
    );
    let member = engine.members.get(&win).expect("member");
    assert_eq!(member.home, home_before, "no home change on null");
    assert!(
        !engine.is_tiled_window_any(&win),
        "no tile landing while proof is retained"
    );
    let (_, authority, seq) = target_row(&nulled, "win-a").expect("target retained");
    assert_eq!(seq, Some(1));
    assert_ne!(
        authority,
        ScopeAuthority::Controlled,
        "spent target never Controlled"
    );
    // Fresh false lands the tile base exactly once.
    let recovered = accepted(
        &mut engine,
        &observe(5, vec![sticky_fact("win-a", "out-a", 0, Some(false))]),
    );
    let done = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert!(done.sticky_meta.as_ref().expect("meta").followup_done);
    assert_eq!(
        recovered.intent_writes.len(),
        0,
        "no extra save on followup"
    );
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(!member.floating, "toggle-float clear lands the tile base");
}

#[test]
fn native_off_overlay_deferred_null_then_false_recovers() {
    // OFF variant of the delayed-proof hold: overlay-deferred Applied,
    // then a null (supported unknown) observation holds, then a fresh
    // `Some(false)` recovers with no extra save.
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    let rev = binding_rev(&engine, "win-a");
    let mut overlay = sticky_fact("win-a", "out-a", 0, Some(false));
    overlay.maximized = Some(true);
    let mut proof = observe(4, vec![overlay]);
    proof.state_attempts = vec![sticky_attempt(
        "win-a",
        false,
        rev,
        FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    accepted(&mut engine, &proof);
    let deferred = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(deferred.disposition, AttemptOutcome::Applied);
    assert!(
        !deferred.sticky_meta.as_ref().expect("meta").followup_done,
        "maximized overlay defers the dependent landing"
    );
    let win = WindowId("win-a".to_owned());
    let home_before = engine.members.get(&win).expect("member").home.clone();
    let nulled = accepted(
        &mut engine,
        &observe(5, vec![sticky_fact("win-a", "out-a", 0, None)]),
    );
    let held = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert!(
        !held.sticky_meta.as_ref().expect("meta").followup_done,
        "null current retains proof"
    );
    assert!(nulled.intent_writes.is_empty());
    assert!(
        engine.restored_sticky.contains_key(&win),
        "semantic sticky survives the unknown observation"
    );
    let member = engine.members.get(&win).expect("member");
    assert_eq!(member.home, home_before, "no home change on null");
    assert!(
        !engine.is_tiled_window_any(&win),
        "no tile landing while proof is retained"
    );
    let (_, authority, seq) = target_row(&nulled, "win-a").expect("target retained");
    assert_eq!(seq, Some(2));
    assert_ne!(
        authority,
        ScopeAuthority::Controlled,
        "spent target never Controlled"
    );
    let recovered = accepted(
        &mut engine,
        &observe(6, vec![sticky_fact("win-a", "out-a", 0, Some(false))]),
    );
    let done = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(done.disposition, AttemptOutcome::Applied);
    assert!(done.sticky_meta.as_ref().expect("meta").followup_done);
    assert_eq!(
        recovered.intent_writes.len(),
        0,
        "no extra save on followup"
    );
    let member = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member");
    assert!(!member.floating, "fresh false lands the requested tile");
}

fn insert_generic_binding(
    engine: &mut WorldEngine,
    window: &str,
    property: StateProperty,
    value: bool,
    revision: u64,
) {
    engine.retained_states.insert(
        (WindowId(window.to_owned()), property),
        crate::world_state::RetainedStateBinding {
            target: crate::world_state::RetainedStateTarget {
                window: WindowId(window.to_owned()),
                property,
                value,
                revision,
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

fn rehandshake_local(engine: &mut WorldEngine) {
    let reply = engine.handshake(&crate::connection::HandshakeRequest {
        adapter: "adapter-2".to_owned(),
        request: "handshake-2".to_owned(),
        application_identity: crate::connection::ApplicationIdentity {
            app_id: Some("example.app".to_owned()),
            executable: Some("example".to_owned()),
            process_id: Some("pid-own-1".to_owned()),
        },
        capabilities: crate::connection::HostCapabilities {
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
}

#[test]
fn native_binding_cap_replacement_allowed_but_distinct_refused_atomic() {
    use crate::world_state::MAX_RETAINED_STATE_BINDINGS;
    // Same-window replacement at the exact bound mints through the real
    // native producer: 39,999 other-property fillers plus the live sticky
    // binding fill the map, and a fresh SetSticky on that window replaces.
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let need = MAX_RETAINED_STATE_BINDINGS - engine.retained_states.len();
    assert_eq!(need, MAX_RETAINED_STATE_BINDINGS - 1);
    for i in 0..need {
        insert_generic_binding(
            &mut engine,
            &format!("win-fill-{i:05}"),
            StateProperty::Fullscreen,
            false,
            1,
        );
    }
    assert_eq!(
        engine.retained_states.len(),
        MAX_RETAINED_STATE_BINDINGS,
        "exact bound before replacement"
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", false),
        ),
    );
    assert!(reply.command_result.is_none());
    let binding = rollback_binding(&engine, "win-a");
    assert!(!binding.spent);
    assert!(!binding.target.value);
    assert_eq!(binding.sticky_meta.as_ref().expect("meta").command_seq, 2);
    assert_eq!(reply.intent_writes.len(), 1);

    // Distinct new binding at the cap refuses with a typed result: the
    // observation still issues (revision +1, sequence consumed), no save
    // staged, no binding minted for the refused subject.
    let mut capped = native_engine();
    accepted(
        &mut capped,
        &bootstrap(vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)]),
    );
    accepted(
        &mut capped,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            set_sticky("win-a", true),
        ),
    );
    let need = MAX_RETAINED_STATE_BINDINGS - capped.retained_states.len();
    for i in 0..need {
        insert_generic_binding(
            &mut capped,
            &format!("win-cap-{i:05}"),
            StateProperty::Fullscreen,
            false,
            1,
        );
    }
    assert_eq!(capped.retained_states.len(), MAX_RETAINED_STATE_BINDINGS);
    let revision = capped.state_revision;
    let saved = capped.sticky_saved.clone();
    let restored = capped.restored_sticky.clone();
    let refused_obs = cmd(
        3,
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        set_sticky("win-b", true),
    );
    // Commandless control with the same observation: the refusal must
    // converge identically apart from the consumed sequence and typed
    // result. Synthetic filler bindings are not observation-live, so the
    // adoption path legitimately prunes them on either path.
    let mut control = capped.clone();
    let mut control_obs = refused_obs.clone();
    control_obs.command = None;
    control_obs.command_seq = None;
    let _ = control.handle_report_legacy(&control_obs);
    match capped.handle_report_legacy(&refused_obs) {
        LegacyWorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(crate::world_report::CommandResult::Rejected {
                kind: crate::world_report::CommandRejectKind::OperationRefused
            }),
            "expected cap refusal"
        ),
        other => panic!("expected cap refusal, got {other:?}"),
    }
    assert_eq!(capped.state_revision, revision + 1, "refusal issues");
    assert_eq!(capped.command_highwater, Some(2), "command consumed");
    assert_eq!(capped.sticky_saved, saved);
    assert_eq!(capped.restored_sticky, restored);
    // Synthetic filler bindings are not observation-live, so the adoption
    // path legitimately prunes them; the refusal must converge with the
    // commandless control, minting no binding past the cap for the
    // refused subject.
    assert_eq!(
        capped.retained_states, control.retained_states,
        "refusal converges with the commandless control"
    );
    assert!(
        capped.retained_states.len() <= MAX_RETAINED_STATE_BINDINGS,
        "no insertion past the cap"
    );
    assert!(
        !capped
            .retained_states
            .contains_key(&(WindowId("win-b".to_owned()), StateProperty::Sticky)),
        "no binding minted past the cap"
    );
    assert!(
        capped.sticky_command_save.is_none(),
        "no save staged on refusal"
    );
}

#[test]
fn native_producer_lifecycle_close_own_handshake_and_incomplete_retains() {
    // Explicit close prunes the native sticky binding.
    let mut engine = native_engine();
    accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    assert!(
        engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
    );
    let mut closed = observe(3, vec![]);
    closed.closed = vec![WindowId("win-a".to_owned())];
    let reply = accepted(&mut engine, &closed);
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Sticky)),
        "explicit close prunes the producer target"
    );
    assert!(
        target_row(&reply, "win-a").is_none(),
        "closed window publishes no target"
    );

    // Known own-process fact prunes the binding.
    let mut owned = native_engine();
    accepted(&mut owned, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut owned,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let mut own_fact = fact("win-a", "out-a", 0);
    own_fact.process_id = Some("test-own-pid".to_owned());
    accepted(&mut owned, &observe(3, vec![own_fact]));
    assert!(
        !owned
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Sticky)),
        "own-process fact prunes the producer target"
    );

    // Handshake resets to a fresh engine with no retained targets.
    let mut re = native_engine();
    accepted(&mut re, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut re,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    rehandshake_local(&mut re);
    assert!(
        re.retained_states.is_empty(),
        "handshake clears producer targets"
    );

    // Incomplete inventory retains a spent proof with metadata, record-free.
    let mut gapped = native_engine();
    accepted(&mut gapped, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    accepted(
        &mut gapped,
        &cmd(
            2,
            1,
            vec![fact("win-a", "out-a", 0)],
            set_sticky("win-a", true),
        ),
    );
    let rev = binding_rev(&gapped, "win-a");
    let mut threw = observe(3, vec![fact("win-a", "out-a", 0)]);
    threw.state_attempts = vec![sticky_attempt(
        "win-a",
        true,
        rev,
        FrameSetter::Threw,
        Some(false),
        Some(false),
    )];
    accepted(&mut gapped, &threw);
    let spent = rollback_binding(&gapped, "win-a");
    assert!(spent.spent);
    assert_eq!(spent.disposition, AttemptOutcome::PossiblyExecuted);
    let saved = gapped.sticky_saved.clone();
    let mut incomplete = observe(4, vec![fact("win-a", "out-a", 0)]);
    incomplete.inventory_complete = false;
    let reply = accepted(&mut gapped, &incomplete);
    assert!(reply.intent_writes.is_empty(), "incomplete emits nothing");
    let held = rollback_binding(&gapped, "win-a");
    assert!(held.spent, "incomplete retains the spent proof");
    assert_eq!(held.disposition, AttemptOutcome::PossiblyExecuted);
    assert_eq!(
        held.pending.as_ref().map(|pending| pending.setter),
        Some(FrameSetter::Threw),
        "incomplete retains the exact proof"
    );
    assert!(held.sticky_meta.is_some(), "metadata retained");
    assert!(!held.sticky_meta.as_ref().expect("meta").followup_done);
    assert_eq!(gapped.sticky_saved, saved, "records untouched");
    let (_, authority, seq) = target_row(&reply, "win-a").expect("target retained");
    assert_eq!(seq, Some(1));
    assert_ne!(authority, ScopeAuthority::Controlled);
}

#[test]
fn native_two_output_isolation_and_generic_binding_emits_no_target() {
    // Two outputs: a native command targets only the selected window; the
    // ordinary window on the other output stays untouched with no leak.
    let mut engine = native_engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
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
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            vec![],
            "",
            Some(1),
            Some(set_sticky("win-a", true)),
        ),
    );
    assert!(reply.command_result.is_none());
    let (value, authority, seq) = target_row(&reply, "win-a").expect("target");
    assert_eq!((value, seq), (true, Some(1)));
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert!(
        target_row(&reply, "win-b").is_none(),
        "untargeted output window leaks no StateTarget"
    );
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-b".to_owned()), StateProperty::Sticky))
    );
    let other = engine
        .members
        .get(&WindowId("win-b".to_owned()))
        .expect("member");
    assert_eq!(other.home.output, OutputId("out-b".to_owned()));
    assert!(!other.floating, "ordinary window untouched");

    // Generic producer binding without sticky metadata never publishes.
    let mut generic = native_engine();
    accepted(&mut generic, &bootstrap(vec![fact("win-a", "out-a", 0)]));
    let rev = generic.state_revision + 1;
    insert_generic_binding(&mut generic, "win-a", StateProperty::Sticky, true, rev);
    let reply = accepted(&mut generic, &observe(2, vec![fact("win-a", "out-a", 0)]));
    assert!(
        target_row(&reply, "win-a").is_none(),
        "meta-less binding publishes no StateTarget"
    );
}

#[test]
fn native_unmatched_proof_never_lands_or_rewrites() {
    // Revision, property, and value mismatches never settle the OFF hold:
    // the binding stays unspent, the tile never lands, raw facts untouched.
    // Case 0: wrong revision. Case 1: wrong property. Case 2: wrong value.
    for case in 0..3 {
        let mut engine = native_engine();
        accepted(&mut engine, &bootstrap(vec![fact("win-a", "out-a", 0)]));
        accepted(
            &mut engine,
            &cmd(
                2,
                1,
                vec![fact("win-a", "out-a", 0)],
                set_sticky("win-a", true),
            ),
        );
        accepted(
            &mut engine,
            &cmd(
                3,
                2,
                vec![fact("win-a", "out-a", 0)],
                set_sticky("win-a", false),
            ),
        );
        let rev = binding_rev(&engine, "win-a");
        let attempt = match case {
            0 => sticky_attempt(
                "win-a",
                false,
                rev + 100,
                FrameSetter::Accepted,
                Some(true),
                Some(false),
            ),
            1 => crate::world_report::StateAttempt {
                window: WindowId("win-a".to_owned()),
                property: StateProperty::Maximized,
                target: false,
                revision: rev,
                setter: FrameSetter::Accepted,
                pre: Some(false),
                post: Some(false),
                attempt_ms: 7,
            },
            _ => sticky_attempt(
                "win-a",
                true,
                rev,
                FrameSetter::Accepted,
                Some(false),
                Some(true),
            ),
        };
        let mut proof = observe(4, vec![sticky_fact("win-a", "out-a", 0, Some(false))]);
        proof.state_attempts = vec![attempt];
        let reply = accepted(&mut engine, &proof);
        assert!(
            reply.intent_writes.is_empty(),
            "mismatch mints nothing {case}"
        );
        let binding = rollback_binding(&engine, "win-a");
        assert!(!binding.spent, "binding remains unspent {case}");
        assert_eq!(
            binding.disposition,
            AttemptOutcome::Unattempted,
            "no settlement {case}"
        );
        assert!(
            !binding.sticky_meta.as_ref().expect("meta").followup_done,
            "no landing {case}"
        );
        let member = engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("member");
        assert!(member.floating, "off hold keeps float {case}");
        assert_eq!(
            engine
                .staged
                .facts
                .get(&WindowId("win-a".to_owned()))
                .and_then(|row| row.sticky),
            Some(false),
            "raw facts never rewritten {case}"
        );
        let (value, authority, seq) = target_row(&reply, "win-a").expect("target");
        assert_eq!(
            (value, seq),
            (false, Some(2)),
            "original target kept {case}"
        );
        assert_eq!(authority, ScopeAuthority::Controlled);
    }
}

#[test]
fn pending_window_transfer_holds_sticky_publication() {
    // Mint through SetSticky, then stage a same-window pending flight:
    // the real Send route refuses this sticky window. Hold and recovery
    // must retain the unspent tuple; another window's flight must not gate it.
    let mut engine = native_engine();
    let boot = accepted(
        &mut engine,
        &bootstrap(vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)]),
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
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            set_sticky("win-a", true),
        ),
    );
    let (_, minted_authority, _) = target_row(&minted, "win-a").expect("entry target");
    assert_eq!(minted_authority, ScopeAuthority::Controlled);
    let rev = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding")
        .target
        .revision;
    engine.retained.push(crate::world_engine::RetainedTransfer {
        target: crate::world_report::TransferTarget {
            operation_id: "op-2".to_owned(),
            kind: crate::world_report::TransferKind::Window(WindowId("win-a".to_owned())),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: ring[1].clone(),
            follow: false,
            held: false,
        },
        policy: false,
        command_seq: None,
        source_output: OutputId("out-a".to_owned()),
        source_workspace: ring[0].clone(),
        members: Vec::new(),
        migration_refill: None,
        migration_initial_empty: false,
        migration_source_index: None,
        migration_source_view: None,
        migration_target_view: None,
        migration_terminal: None,
        migration_selection_rev: None,
        pending_member: None,
        pending_members: std::collections::BTreeMap::new(),
        pending_view: None,
    });
    let sent = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![("out-a", ring[1].as_str())],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        engine.retained.iter().any(|transfer| matches!(
            &transfer.target.kind,
            crate::world_report::TransferKind::Window(w) if w.0 == "win-a"
        )),
        "harness must retain the same-window flight"
    );
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
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
    // Settle the staged flight with the standard raw transfer evidence: the
    // adapter echoes the destination view and carries the accepted member
    // attempt for `op-2`. The same unspent target resumes Controlled.
    let mut arrived = report_full(
        4,
        4,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
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
            target_output: OutputId("out-a".to_owned()),
            target_workspace: ring[1].clone(),
            setter: crate::world_report::FrameSetter::Accepted,
            pre: None,
            post: Some(crate::world_report::WorkspaceRef::Logical {
                output: OutputId("out-a".to_owned()),
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
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding survives arrival");
    assert!(!binding.spent);
    assert_eq!(binding.target.revision, rev);
    let (value, authority, seq) = target_row(&settled, "win-a").expect("entry target");
    assert!(value);
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert_eq!(seq, Some(1));
    // An other-window flight never fences this target.
    let other_sent = accepted(
        &mut engine,
        &report_full(
            5,
            5,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![("out-a", ring[1].as_str())],
            "win-b",
            Some(3),
            Some(WorldCommand::Send {
                window: WindowId("win-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ring[1].clone(),
                follow: false,
            }),
        ),
    );
    assert!(
        engine.retained.iter().any(|transfer| matches!(
            &transfer.target.kind,
            crate::world_report::TransferKind::Window(w) if w.0 == "win-b"
        )),
        "send must retain the other-window flight"
    );
    let (value, authority, seq) = target_row(&other_sent, "win-a").expect("entry target");
    assert!(value);
    assert_eq!(authority, ScopeAuthority::Controlled);
    assert_eq!(seq, Some(1));
    let mut other_arrived = report_full(
        6,
        6,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        vec![("out-a", ring[1].as_str())],
        "win-b",
        None,
        None,
    );
    other_arrived
        .member_attempts
        .push(crate::world_report::MemberAttempt {
            operation_id: "op-3".to_owned(),
            window: WindowId("win-b".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: ring[1].clone(),
            setter: crate::world_report::FrameSetter::Accepted,
            pre: None,
            post: Some(crate::world_report::WorkspaceRef::Logical {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(ring[1].clone()),
            }),
            attempt_ms: 2,
        });
    accepted(&mut engine, &other_arrived);
    assert!(
        !engine.retained.iter().any(|transfer| matches!(
            &transfer.target.kind,
            crate::world_report::TransferKind::Window(w) if w.0 == "win-b"
        )),
        "arrival evidence settles the other-window flight"
    );
    // A spent target never reauthorizes: drive the binding spent through
    // the real settlement producer (a threw attempt with the exact target
    // revision), then observation keeps the exact sequence un-Controlled.
    let key = (WindowId("win-a".to_owned()), StateProperty::Sticky);
    let mut failed = observe(
        7,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    failed.state_attempts = vec![crate::world_report::StateAttempt {
        window: WindowId("win-a".to_owned()),
        property: StateProperty::Sticky,
        target: true,
        revision: rev,
        setter: crate::world_report::FrameSetter::Threw,
        pre: None,
        post: None,
        attempt_ms: 7,
    }];
    accepted(&mut engine, &failed);
    let spent = engine.retained_states.get(&key).expect("binding");
    assert!(spent.spent, "producer marks spent");
    let reply = accepted(
        &mut engine,
        &observe(
            8,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    let (_, authority, seq) = target_row(&reply, "win-a").expect("target retained");
    assert_eq!(seq, Some(1));
    assert_ne!(authority, ScopeAuthority::Controlled);
}
