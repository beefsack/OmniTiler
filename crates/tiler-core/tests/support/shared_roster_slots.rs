//! Core-only slot authority and settlement, without the adapter wire.

use super::{
    NativeSelectorScope, OutputId, RawSettingValue, TestGate, WindowId, WorkspaceMode,
    WorldCommand, WorldEngine, WorldReportV4, accepted, engine_host_owned_with_selector,
    fact_native, logical_for, native_obs_occ, published_view, report, ring_ids, two_screens,
};
use tiler_core::connection::SettingsDiagnosticKind;
use tiler_core::directional::{WindowSubject, WorkspaceId};
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report::{SendSource, TransferKind, WorldReplyV4};
use tiler_core::world_roster::{
    NativeHome, RosterFeedback, RosterReadback, RosterSlot, RosterStepFeedback, RosterStepStatus,
    SharedRosterPlan,
};

const ORDER: &[&str] = &["d1", "d2", "d3"];
const DP: &str = "out-a";
const HDMI: &str = "out-b";

fn output(id: &str) -> OutputId {
    OutputId(id.to_owned())
}

fn slot_engine() -> WorldEngine {
    engine_host_owned_with_selector(
        WorkspaceMode::PerOutputLocal,
        true,
        true,
        NativeSelectorScope::PerOutput,
    )
}

fn capture(
    seq: u64,
    order: &[&str],
    currents: [&str; 2],
    homes: &[(&str, &str, &str)],
) -> WorldReportV4 {
    let counts: Vec<_> = order
        .iter()
        .map(|id| Some(homes.iter().filter(|(_, _, native)| native == id).count() as u32))
        .collect();
    let windows = homes
        .iter()
        .map(|&(id, output, native)| {
            let mut fact = fact_native(id, output, Some(native));
            fact.visibility.visible = Some(native == currents[usize::from(output == HDMI)]);
            fact
        })
        .collect();
    report(
        seq,
        seq,
        two_screens(),
        windows,
        vec![
            native_obs_occ(DP, order, Some(currents[0]), &counts),
            native_obs_occ(HDMI, order, Some(currents[1]), &counts),
        ],
        None,
        None,
        true,
    )
}

fn reference_capture(seq: u64, stayer: bool) -> WorldReportV4 {
    let mut homes = vec![("a", DP, "d1"), ("b", DP, "d2"), ("c", HDMI, "d1")];
    if stayer {
        homes.push(("stayer", DP, "d1"));
    }
    capture(seq, ORDER, ["d1", "d1"], &homes)
}

fn command(capture: &mut WorldReportV4, seq: u64, command: WorldCommand) {
    capture.command_seq = Some(seq);
    capture.command = Some(command);
}

fn send_append(capture: &mut WorldReportV4, index: u8, follow: bool) {
    command(
        capture,
        1,
        WorldCommand::SendNumbered {
            window: WindowSubject::Window(WindowId("a".to_owned())),
            source: SendSource::HomeThenAction,
            index,
            follow,
        },
    );
}

fn member_workspace<'a>(reply: &'a WorldReplyV4, window: &str) -> &'a str {
    &reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .expect("live member")
        .workspace
}

fn feedback(
    plan: &SharedRosterPlan,
    steps: impl IntoIterator<Item = (String, RosterStepFeedback)>,
) -> RosterFeedback {
    RosterFeedback {
        operation_id: plan.operation_id.clone(),
        generation: plan.generation.clone(),
        revision: plan.revision,
        steps: steps.into_iter().collect(),
    }
}

fn appended_step(before: &[&str], after: &[&str]) -> RosterStepFeedback {
    RosterStepFeedback {
        status: RosterStepStatus::Appended,
        before: Some(RosterReadback::Order(
            before.iter().map(|s| (*s).to_owned()).collect(),
        )),
        after: Some(RosterReadback::Order(
            after.iter().map(|s| (*s).to_owned()).collect(),
        )),
    }
}

fn applied_home_step(
    before_out: &str,
    before_native: &str,
    after_out: &str,
    after_native: &str,
) -> RosterStepFeedback {
    RosterStepFeedback {
        status: RosterStepStatus::Applied,
        before: Some(RosterReadback::Home(NativeHome {
            output: output(before_out),
            native: before_native.to_owned(),
        })),
        after: Some(RosterReadback::Home(NativeHome {
            output: output(after_out),
            native: after_native.to_owned(),
        })),
    }
}

fn applied_view_step(before: &str, after: &str) -> RosterStepFeedback {
    RosterStepFeedback {
        status: RosterStepStatus::Applied,
        before: Some(RosterReadback::Current(before.to_owned())),
        after: Some(RosterReadback::Current(after.to_owned())),
    }
}

fn applied_removal_step(before: &[&str], after: &[&str]) -> RosterStepFeedback {
    RosterStepFeedback {
        status: RosterStepStatus::Applied,
        before: Some(RosterReadback::Order(
            before.iter().map(|s| (*s).to_owned()).collect(),
        )),
        after: Some(RosterReadback::Order(
            after.iter().map(|s| (*s).to_owned()).collect(),
        )),
    }
}

fn refused_step(
    before: Option<RosterReadback>,
    after: Option<RosterReadback>,
) -> RosterStepFeedback {
    RosterStepFeedback {
        status: RosterStepStatus::Refused,
        before,
        after,
    }
}

fn exact_plan_feedback(plan: &SharedRosterPlan, appended_ids: &[&str]) -> RosterFeedback {
    let mut order = plan.baseline.native_order.clone();
    let mut appended = std::collections::BTreeMap::new();
    let mut steps = std::collections::BTreeMap::new();
    let mut append_iter = appended_ids.iter();
    for slot in &plan.slots {
        if let RosterSlot::Append(step) = slot {
            let native = (*append_iter.next().expect("appended id")).to_owned();
            let before = order.clone();
            order.push(native.clone());
            appended.insert(step.clone(), native);
            steps.insert(
                step.clone(),
                RosterStepFeedback {
                    status: RosterStepStatus::Appended,
                    before: Some(RosterReadback::Order(before)),
                    after: Some(RosterReadback::Order(order.clone())),
                },
            );
        }
    }
    let resolve = |slot: &RosterSlot| match slot {
        RosterSlot::Existing(native) => native.clone(),
        RosterSlot::Append(step) => appended[step].clone(),
    };
    for member in &plan.members {
        steps.insert(
            member.step_id.clone(),
            RosterStepFeedback {
                status: RosterStepStatus::Applied,
                before: Some(RosterReadback::Home(member.before.clone())),
                after: Some(RosterReadback::Home(NativeHome {
                    output: member.output.clone(),
                    native: resolve(&member.target),
                })),
            },
        );
    }
    for view in &plan.views {
        steps.insert(
            view.step_id.clone(),
            RosterStepFeedback {
                status: RosterStepStatus::Applied,
                before: Some(RosterReadback::Current(view.before.clone())),
                after: Some(RosterReadback::Current(resolve(&view.target))),
            },
        );
    }
    for removal in &plan.removals {
        let before = order.clone();
        order.retain(|id| id != &removal.native);
        steps.insert(
            removal.step_id.clone(),
            RosterStepFeedback {
                status: RosterStepStatus::Applied,
                before: Some(RosterReadback::Order(before)),
                after: Some(RosterReadback::Order(order.clone())),
            },
        );
    }
    feedback(plan, steps)
}

fn apply_feedback(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
    feedback: &RosterFeedback,
) -> WorldReplyV4 {
    let run = engine
        .handle_report_with_roster_feedback_and_prepare(report, &[], &TestGate, feedback, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    match run.outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn assert_no_legacy_roster_mutations(reply: &WorldReplyV4) {
    assert!(reply.native_creations.is_empty(), "{reply:?}");
    assert!(
        reply.workspaces.iter().all(|row| row.existence),
        "{reply:?}"
    );
    assert!(reply.transfers.iter().all(|row| row.held), "{reply:?}");
}

fn assert_sibling_unchanged(boot: &WorldReplyV4, reply: &WorldReplyV4) {
    assert_eq!(published_view(reply, HDMI), published_view(boot, HDMI));
    assert_eq!(member_workspace(reply, "c"), member_workspace(boot, "c"));
    assert_eq!(ring_ids(reply, HDMI), ring_ids(boot, HDMI));
}

fn reference_send(stayer: bool) {
    let mut engine = slot_engine();
    let boot = accepted(&mut engine, &reference_capture(1, stayer));
    assert_no_legacy_roster_mutations(&boot);
    assert!(engine.desired_shared_roster_plan().is_none());
    let dp = output(DP);
    let hdmi = output(HDMI);
    let a_source = member_workspace(&boot, "a");
    let b_logical = member_workspace(&boot, "b");
    let destination = logical_for(&boot, DP, "d3").expect("real spare");
    assert_eq!(ring_ids(&boot, HDMI).len(), 2, "no logical padding");

    let mut send = reference_capture(2, stayer);
    send_append(&mut send, 0, true);
    let issued = accepted(&mut engine, &send);
    assert_no_legacy_roster_mutations(&issued);
    assert_eq!(issued.transfers.len(), 1, "only A's logical command");
    let transfer = &issued.transfers[0];
    assert_eq!(transfer.command_seq, Some(1));
    assert!(matches!(&transfer.kind, TransferKind::Window(window) if window.0 == "a"));
    let plan = engine
        .desired_shared_roster_plan()
        .expect("first publication has a plan")
        .clone();
    assert_eq!(plan.revision, issued.state_revision);
    assert_eq!(plan.observation_seq, issued.based_on_observation_seq);
    assert_sibling_unchanged(&boot, &issued);
    assert_eq!(plan.baseline.native_order, ORDER);
    assert_eq!(plan.baseline_currents[&dp], "d1");
    assert_eq!(plan.baseline_currents[&hdmi], "d1");
    assert_eq!(plan.projections[&hdmi], plan.baseline.projections[&hdmi]);
    assert!(plan.removals.is_empty());
    for (slot, native) in plan.slots.iter().zip(ORDER) {
        assert_eq!(slot, &RosterSlot::Existing((*native).to_owned()));
    }
    let appends: Vec<_> = plan
        .slots
        .iter()
        .filter_map(|slot| match slot {
            RosterSlot::Append(step_id) => Some(step_id.clone()),
            RosterSlot::Existing(_) => None,
        })
        .collect();
    assert_eq!(appends.len(), usize::from(stayer));
    assert_eq!(plan.slots.len(), ORDER.len() + appends.len());
    assert_eq!(plan.members.len(), if stayer { 1 } else { 2 });
    let a = plan
        .members
        .iter()
        .find(|step| step.window.0 == "a")
        .expect("move a");
    let a_target = if stayer { "d3" } else { "d2" };
    assert_eq!(a.before.output, dp);
    assert_eq!(a.before.native, "d1");
    assert_eq!(a.output, dp);
    assert_eq!(a.target, RosterSlot::Existing(a_target.to_owned()));
    if stayer {
        assert_eq!(
            &plan.projections[&dp][..3],
            &[a_source, b_logical, &destination]
        );
    } else {
        let b = plan
            .members
            .iter()
            .find(|step| step.window.0 == "b")
            .expect("relocate b");
        assert_eq!(b.before.output, dp);
        assert_eq!(b.before.native, "d2");
        assert_eq!(b.output, dp);
        assert_eq!(b.target, RosterSlot::Existing("d1".to_owned()));
        assert_eq!(&plan.projections[&dp][..2], &[b_logical, &destination]);
        assert!(
            !plan.projections[&dp]
                .iter()
                .any(|logical| logical == a_source)
        );
    }
    let [view] = plan.views.as_slice() else {
        panic!("expected one selector: {plan:?}")
    };
    assert_eq!(view.output, dp);
    assert_eq!(view.before, "d1");
    assert_eq!(view.target, a.target);
    let focus = plan
        .focus
        .as_ref()
        .expect("send-follow carries focus authority");
    assert_eq!(focus.target.0, "a");
    assert_eq!(focus.output, dp);
    assert_eq!(focus.workspace, destination);
    assert_eq!(issued.focus_request.as_ref(), Some(focus));
    assert!(
        view.dependencies.contains(&a.step_id),
        "arrival before follow"
    );
    assert_eq!(
        engine
            .settled_shared_roster()
            .expect("settled baseline")
            .epoch,
        plan.baseline.epoch
    );

    let mut homes = vec![
        ("a", DP, a_target),
        ("b", DP, if stayer { "d2" } else { "d1" }),
        ("c", HDMI, "d1"),
    ];
    let mut native_order = ORDER.to_vec();
    if stayer {
        native_order.push("d4");
        homes.push(("stayer", DP, "d1"));
    }
    let applied = exact_plan_feedback(&plan, if stayer { &["d4"] } else { &[] });
    let arrival = capture(3, &native_order, [a_target, "d1"], &homes);
    let settled = apply_feedback(&mut engine, &arrival, &applied);
    assert_no_legacy_roster_mutations(&settled);
    assert!(engine.desired_shared_roster_plan().is_none());
    assert_eq!(
        engine
            .settled_shared_roster()
            .expect("proved order")
            .native_order,
        native_order
    );
    assert_eq!(member_workspace(&settled, "a"), destination);
    assert_eq!(member_workspace(&settled, "b"), b_logical);
    let b_native = if stayer { "d2" } else { "d1" };
    assert_eq!(
        logical_for(&settled, DP, b_native).as_deref(),
        Some(b_logical)
    );
    assert_eq!(
        logical_for(&settled, DP, a_target).as_deref(),
        Some(destination.as_str())
    );
    let spare = settled
        .workspaces
        .iter()
        .find(|row| row.output == dp && row.trailing)
        .expect("assigned real spare");
    assert_eq!(
        spare.native_binding.as_deref(),
        native_order.last().copied()
    );
    assert_eq!(
        published_view(&settled, DP).as_deref(),
        Some(destination.as_str())
    );
    if stayer {
        assert_eq!(member_workspace(&settled, "stayer"), a_source);
    } else {
        assert!(
            !ring_ids(&settled, DP)
                .iter()
                .any(|logical| logical == a_source)
        );
    }
    assert_sibling_unchanged(&boot, &settled);

    let replay = engine
        .handle_report_with_roster_feedback_and_prepare(&arrival, &[], &TestGate, &applied, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    let WorldOutcome::Duplicate(replayed) = replay.outcome else {
        panic!("same request must replay, got {:?}", replay.outcome)
    };
    assert_eq!(replayed, settled);
    assert!(engine.desired_shared_roster_plan().is_none());
    let mut previous = capture(4, &native_order, [a_target, "d1"], &homes);
    command(
        &mut previous,
        2,
        WorldCommand::SelectPrevious { output: Some(hdmi) },
    );
    let unchanged = accepted(&mut engine, &previous);
    assert_sibling_unchanged(&boot, &unchanged);
    assert_no_legacy_roster_mutations(&unchanged);
    assert!(
        engine.desired_shared_roster_plan().is_none(),
        "sibling history stays empty"
    );
}

#[test]
fn send_follow_compacts_at_the_same_native_count() {
    reference_send(false);
}

#[test]
fn send_follow_with_a_stayer_appends_exactly_one_slot() {
    reference_send(true);
}

#[test]
fn partial_apply_and_late_contradictory_feedback_do_not_relabel_members() {
    let mut engine = slot_engine();
    let boot = accepted(&mut engine, &reference_capture(1, false));
    let mut send = reference_capture(2, false);
    send_append(&mut send, 0, true);
    accepted(&mut engine, &send);
    let plan = engine
        .desired_shared_roster_plan()
        .expect("send plan")
        .clone();
    let [b, a] = plan.members.as_slice() else {
        panic!("expected two moves: {plan:?}")
    };
    assert_eq!((b.window.0.as_str(), a.window.0.as_str()), ("b", "a"));
    let view = &plan.views[0];
    let prefix = capture(
        3,
        ORDER,
        ["d1", "d1"],
        &[("a", DP, "d1"), ("b", DP, "d1"), ("c", HDMI, "d1")],
    );
    let partial = feedback(
        &plan,
        [
            (b.step_id.clone(), applied_home_step(DP, "d2", DP, "d1")),
            (a.step_id.clone(), refused_step(None, None)),
        ],
    );
    let observed = apply_feedback(&mut engine, &prefix, &partial);
    assert_no_legacy_roster_mutations(&observed);
    assert_eq!(
        member_workspace(&observed, "a"),
        member_workspace(&boot, "a")
    );
    assert_eq!(
        member_workspace(&observed, "c"),
        member_workspace(&boot, "c")
    );
    assert_eq!(published_view(&observed, DP), published_view(&boot, DP));
    // Refusal terminated the original plan epoch; a fresh plan is replanned from the actual state.
    let replanned = engine
        .desired_shared_roster_plan()
        .expect("replan from partial state")
        .clone();
    assert_ne!(replanned.operation_id, plan.operation_id);

    // Late contradictory feedback for the terminated plan cannot advance it.
    let late = feedback(
        &plan,
        [
            (a.step_id.clone(), applied_home_step(DP, "d1", DP, "d2")),
            (view.step_id.clone(), applied_view_step("d1", "d2")),
        ],
    );
    let purported_final = capture(
        4,
        ORDER,
        ["d2", "d1"],
        &[("a", DP, "d2"), ("b", DP, "d1"), ("c", HDMI, "d1")],
    );
    let held = apply_feedback(&mut engine, &purported_final, &late);
    assert_no_legacy_roster_mutations(&held);
    assert_eq!(engine.settled_shared_roster(), Some(&plan.baseline));
    assert_eq!(engine.desired_shared_roster_plan(), Some(&replanned));
    assert_eq!(member_workspace(&held, "a"), member_workspace(&boot, "a"));
    assert_eq!(published_view(&held, DP), published_view(&boot, DP));
}

#[test]
fn incomplete_or_uncontrollable_affected_slots_fence_compaction() {
    for fault in ["unknown", "residual", "unmanaged", "divergent"] {
        let mut engine = slot_engine();
        let boot = accepted(&mut engine, &reference_capture(1, false));
        let epoch = engine.settled_shared_roster().expect("baseline").clone();
        let mut send = reference_capture(2, false);
        send_append(&mut send, 0, true);
        match fault {
            "unknown" | "residual" => {
                for native in &mut send.native {
                    native.occupancy[1].count = (fault == "residual").then_some(2);
                }
            }
            "unmanaged" => send.windows[1].managed = Some(false),
            "divergent" => send.native[1].native_order.swap(0, 1),
            _ => unreachable!(),
        }
        let reply = accepted(&mut engine, &send);
        assert_no_legacy_roster_mutations(&reply);
        assert!(
            engine.desired_shared_roster_plan().is_none(),
            "{fault} must not authorize relocation"
        );
        assert_eq!(engine.settled_shared_roster(), Some(&epoch), "{fault}");
        assert_eq!(
            published_view(&reply, DP),
            published_view(&boot, DP),
            "{fault}"
        );
    }
}

#[test]
fn adjacent_follow_from_sole_local_member_does_not_spend_the_spare() {
    for index in [0, 2] {
        let mut engine = slot_engine();
        let homes = &[("a", DP, "d1"), ("c", HDMI, "d1")];
        let boot = accepted(&mut engine, &capture(1, &ORDER[..2], ["d1", "d1"], homes));
        let mut send = capture(2, &ORDER[..2], ["d1", "d1"], homes);
        send_append(&mut send, index, true);
        let reply = accepted(&mut engine, &send);
        assert_no_legacy_roster_mutations(&reply);
        assert!(engine.desired_shared_roster_plan().is_none());
        assert_eq!(published_view(&reply, DP), published_view(&boot, DP));
        assert_eq!(member_workspace(&reply, "a"), member_workspace(&boot, "a"));
        assert_sibling_unchanged(&boot, &reply);
    }
}

#[test]
fn send_stay_keeps_the_shown_source_and_grows_without_a_view_step() {
    let mut engine = slot_engine();
    let boot = accepted(&mut engine, &reference_capture(1, false));
    let mut send = reference_capture(2, false);
    send_append(&mut send, 0, false);
    let reply = accepted(&mut engine, &send);
    assert_no_legacy_roster_mutations(&reply);
    let plan = engine.desired_shared_roster_plan().expect("send-stay plan");
    assert_eq!(plan.members.len(), 1);
    assert_eq!(plan.members[0].window.0, "a");
    assert_eq!(
        plan.members[0].target,
        RosterSlot::Existing("d3".to_owned())
    );
    assert!(matches!(
        plan.slots.as_slice(),
        [_, _, _, RosterSlot::Append(_)]
    ));
    assert!(plan.removals.is_empty());
    assert!(plan.views.is_empty());
    assert!(plan.focus.is_none());
    assert_eq!(published_view(&reply, DP), published_view(&boot, DP));
    assert_sibling_unchanged(&boot, &reply);
}

#[test]
fn shrink_removes_only_proved_empty_surplus_and_settles_on_absence() {
    let mut engine = slot_engine();
    let homes = &[("a", DP, "d1"), ("b", DP, "d2"), ("c", HDMI, "d1")];
    let oversized = ["d1", "d2", "d3", "unviewed-empty"];
    let reply = accepted(&mut engine, &capture(1, &oversized, ["d1", "d1"], homes));
    assert_no_legacy_roster_mutations(&reply);
    assert!(reply.transfers.is_empty(), "slot shrink is commandless");
    let plan = engine
        .desired_shared_roster_plan()
        .unwrap_or_else(|| panic!("expected shrink plan after {reply:?}"))
        .clone();
    assert!(plan.members.is_empty());
    assert!(plan.views.is_empty());
    assert_eq!(
        plan.slots,
        ORDER
            .iter()
            .map(|id| RosterSlot::Existing((*id).to_owned()))
            .collect::<Vec<_>>()
    );
    let [remove] = plan.removals.as_slice() else {
        panic!("one removal: {plan:?}")
    };
    assert_eq!(remove.native, "unviewed-empty");
    let removed = exact_plan_feedback(&plan, &[]);
    let still_present = capture(2, &oversized, ["d1", "d1"], homes);
    apply_feedback(&mut engine, &still_present, &removed);
    assert_eq!(engine.settled_shared_roster(), Some(&plan.baseline));
    let settled = accepted(&mut engine, &capture(3, ORDER, ["d1", "d1"], homes));
    assert_no_legacy_roster_mutations(&settled);
    assert!(engine.desired_shared_roster_plan().is_none());
    assert_eq!(
        engine
            .settled_shared_roster()
            .expect("new epoch")
            .native_order,
        ORDER
    );
}

#[test]
fn pager_reorder_adopts_existing_logicals_by_native_identity() {
    let mut engine = slot_engine();
    let homes = &[
        ("a", DP, "d1"),
        ("b", DP, "d2"),
        ("c", HDMI, "d1"),
        ("d", HDMI, "d2"),
    ];
    let boot = accepted(&mut engine, &capture(1, ORDER, ["d1", "d1"], homes));
    let external_order = ["d2", "d1", "d3"];
    let reply = accepted(
        &mut engine,
        &capture(2, &external_order, ["d2", "d1"], homes),
    );
    assert_no_legacy_roster_mutations(&reply);
    assert!(engine.desired_shared_roster_plan().is_none());
    assert_eq!(
        engine
            .settled_shared_roster()
            .expect("adopted order")
            .native_order,
        external_order
    );
    for id in ["a", "b", "c", "d"] {
        assert_eq!(member_workspace(&reply, id), member_workspace(&boot, id));
    }
    assert_eq!(
        published_view(&reply, DP).as_deref(),
        Some(member_workspace(&boot, "b"))
    );
    assert_eq!(published_view(&reply, HDMI), published_view(&boot, HDMI));
}

#[test]
fn append_selects_the_real_spare_but_next_can_materialize_existing_padding() {
    let mut engine = slot_engine();
    let boot = accepted(&mut engine, &reference_capture(1, false));
    let spare = logical_for(&boot, HDMI, "d2").expect("real spare");
    assert!(logical_for(&boot, HDMI, "d3").is_none());
    let mut trailing = reference_capture(2, false);
    command(
        &mut trailing,
        1,
        WorldCommand::SelectTrailing {
            output: Some(output(HDMI)),
        },
    );
    let issued = accepted(&mut engine, &trailing);
    assert_no_legacy_roster_mutations(&issued);
    let plan = engine
        .desired_shared_roster_plan()
        .expect("select real spare")
        .clone();
    assert_eq!(plan.views.len(), 1);
    assert_eq!(plan.views[0].target, RosterSlot::Existing("d2".to_owned()));
    assert_eq!(plan.projections[&output(HDMI)].len(), 2);

    let homes = &[("a", DP, "d1"), ("b", DP, "d2"), ("c", HDMI, "d1")];
    let at_spare = capture(3, ORDER, ["d1", "d2"], homes);
    let proof = exact_plan_feedback(&plan, &[]);
    let on_spare = apply_feedback(&mut engine, &at_spare, &proof);
    assert_eq!(published_view(&on_spare, HDMI), Some(spare));
    let mut next = capture(4, ORDER, ["d1", "d2"], homes);
    command(
        &mut next,
        2,
        WorldCommand::SelectRelative {
            output: Some(output(HDMI)),
            delta: 1,
        },
    );
    let reply = accepted(&mut engine, &next);
    assert_no_legacy_roster_mutations(&reply);
    let next_plan = engine
        .desired_shared_roster_plan()
        .expect("Next reaches pager padding")
        .clone();
    assert_eq!(next_plan.views.len(), 1);
    assert_eq!(next_plan.views[0].output, output(HDMI));
    assert_eq!(
        next_plan.views[0].target,
        RosterSlot::Existing("d3".to_owned())
    );
    assert_eq!(next_plan.slots, plan.slots);
    assert!(next_plan.members.is_empty());
    let next_proof = exact_plan_feedback(&next_plan, &[]);
    let landed = apply_feedback(
        &mut engine,
        &capture(5, ORDER, ["d1", "d3"], homes),
        &next_proof,
    );
    assert_no_legacy_roster_mutations(&landed);
    let materialized = logical_for(&landed, HDMI, "d3").expect("proved padding materialization");
    assert_eq!(published_view(&landed, HDMI), Some(materialized));
    assert!(engine.desired_shared_roster_plan().is_none());
}

#[test]
fn external_selection_materializes_padding_without_a_native_write() {
    let mut engine = slot_engine();
    let boot = accepted(&mut engine, &reference_capture(1, false));
    assert!(logical_for(&boot, HDMI, "d3").is_none());
    let homes = &[("a", DP, "d1"), ("b", DP, "d2"), ("c", HDMI, "d1")];
    let reply = accepted(&mut engine, &capture(2, ORDER, ["d1", "d3"], homes));
    assert_no_legacy_roster_mutations(&reply);
    let shown = logical_for(&reply, HDMI, "d3").expect("selected padding has a real logical");
    assert_eq!(published_view(&reply, HDMI), Some(shown));
    assert_eq!(published_view(&reply, DP), published_view(&boot, DP));
    assert!(engine.desired_shared_roster_plan().is_none());
}

#[test]
fn same_send_keeps_independent_roster_and_setting_off_routes_legacy() {
    let mut independent = engine_host_owned_with_selector(
        WorkspaceMode::PerOutputLocal,
        false,
        false,
        NativeSelectorScope::PerOutput,
    );
    let independent_capture = |seq| {
        let mut capture = report(
            seq,
            seq,
            two_screens(),
            vec![
                fact_native("a", DP, Some("d1")),
                fact_native("b", DP, Some("d2")),
                fact_native("c", HDMI, Some("h1")),
            ],
            vec![
                native_obs_occ(DP, ORDER, Some("d1"), &[Some(1), Some(1), Some(0)]),
                native_obs_occ(HDMI, &["h1", "h2"], Some("h1"), &[Some(1), Some(0)]),
            ],
            None,
            None,
            true,
        );
        capture.windows[1].visibility.visible = Some(false);
        capture
    };
    let boot = accepted(&mut independent, &independent_capture(1));
    let mut send = independent_capture(2);
    send_append(&mut send, 0, true);
    let issued = accepted(&mut independent, &send);
    assert_sibling_unchanged(&boot, &issued);
    assert_eq!(logical_for(&issued, DP, "d2"), logical_for(&boot, DP, "d2"));
    assert_eq!(
        logical_for(&issued, HDMI, "h2"),
        logical_for(&boot, HDMI, "h2")
    );
    assert!(issued.transfers.iter().any(|transfer| {
        matches!(&transfer.kind, tiler_core::world_report::TransferKind::Window(id) if id.0 == "a")
            && transfer.target_output.0 == DP
            && Some(&transfer.target_workspace) == logical_for(&boot, DP, "d3").as_ref()
    }));
    assert!(independent.settled_shared_roster().is_none());
    let mut previous = independent_capture(3);
    command(
        &mut previous,
        2,
        WorldCommand::SelectPrevious {
            output: Some(output(HDMI)),
        },
    );
    assert_sibling_unchanged(&boot, &accepted(&mut independent, &previous));

    let mut linked = engine_host_owned_with_selector(
        WorkspaceMode::Shared,
        true,
        true,
        NativeSelectorScope::AllOutputs,
    );
    let mut setting_off = engine_host_owned_with_selector(
        WorkspaceMode::PerOutputLocal,
        true,
        true,
        NativeSelectorScope::AllOutputs,
    );
    let status = setting_off
        .apply_test_settings(
            2,
            vec![(
                "mode".to_owned(),
                RawSettingValue::String("per-output-local".to_owned()),
            )],
        )
        .expect("settings response");
    assert!(status.diagnostics.iter().any(|diag| {
        diag.key == "mode" && diag.kind == SettingsDiagnosticKind::UnsupportedMode
    }));
    assert_eq!(setting_off.settings().workspace_mode, WorkspaceMode::Shared);
    assert_eq!(
        setting_off.settings().requested["workspace.mode"],
        tiler_core::settings_state::EffectiveSetting::Enum("per-output-local"),
    );
    let first = reference_capture(1, false);
    let explicit_boot = accepted(&mut linked, &first);
    let implicit_boot = accepted(&mut setting_off, &first);
    assert_eq!(implicit_boot.outputs, explicit_boot.outputs);
    let mut send = reference_capture(2, false);
    send_append(&mut send, 0, true);
    let explicit = accepted(&mut linked, &send);
    let implicit = accepted(&mut setting_off, &send);
    assert_eq!(implicit.outputs, explicit.outputs);
    assert_eq!(implicit.workspaces, explicit.workspaces);
    assert_eq!(implicit.transfers, explicit.transfers);
    assert_eq!(
        published_view(&implicit, DP),
        published_view(&implicit, HDMI)
    );
    assert!(linked.settled_shared_roster().is_none());
    assert!(setting_off.settled_shared_roster().is_none());
}

#[test]
fn separate_global_unique_routes_but_linked_per_output_does_not() {
    for mode in [WorkspaceMode::GlobalUnique, WorkspaceMode::Shared] {
        let mut engine =
            engine_host_owned_with_selector(mode, true, true, NativeSelectorScope::PerOutput);
        let boot = accepted(&mut engine, &reference_capture(1, false));
        if mode == WorkspaceMode::GlobalUnique {
            assert!(engine.settled_shared_roster().is_some());
            assert_eq!(ring_ids(&boot, HDMI).len(), 2);
        } else {
            assert!(engine.settled_shared_roster().is_none());
            assert_eq!(published_view(&boot, DP), published_view(&boot, HDMI));
        }
    }
}

#[test]
fn two_output_selectors_share_one_plan_without_moving_members() {
    let mut engine = slot_engine();
    let boot = accepted(&mut engine, &reference_capture(1, false));
    let select = |out| WorldCommand::SelectNumbered {
        output: Some(output(out)),
        index: 2,
    };
    let result = engine
        .handle_report_with_gate_and_prepare(
            &reference_capture(2, false),
            &[(1, select(DP)), (2, select(HDMI))],
            &TestGate,
            |_| Ok::<(), std::convert::Infallible>(()),
        )
        .unwrap_or_else(|e| match e {});
    let WorldOutcome::Accepted(reply) = result.outcome else {
        panic!("selectors must be admitted")
    };
    assert_no_legacy_roster_mutations(&reply);
    let plan = engine
        .desired_shared_roster_plan()
        .expect("one shared plan")
        .clone();
    assert_eq!(plan.views.len(), 2);
    assert!(plan.members.is_empty() && plan.removals.is_empty());
    for out in [DP, HDMI] {
        let view = plan
            .views
            .iter()
            .find(|view| view.output.0 == out)
            .expect("local selector");
        assert_eq!(view.before, "d1");
        assert_eq!(view.target, RosterSlot::Existing("d2".to_owned()));
    }
    let applied = exact_plan_feedback(&plan, &[]);
    let homes = &[("a", DP, "d1"), ("b", DP, "d2"), ("c", HDMI, "d1")];
    let settled = apply_feedback(
        &mut engine,
        &capture(3, ORDER, ["d2", "d2"], homes),
        &applied,
    );
    assert_no_legacy_roster_mutations(&settled);
    for out in [DP, HDMI] {
        assert_eq!(published_view(&settled, out), logical_for(&boot, out, "d2"));
    }
    assert!(engine.desired_shared_roster_plan().is_none());
}

#[test]
fn migration_reuses_assigned_backing_and_waits_for_proof_before_source_release() {
    for outcome in ["applied", "refused", "unreadable"] {
        let mut engine = slot_engine();
        let homes = &[
            ("a", DP, "d1"),
            ("b", DP, "d2"),
            ("c", HDMI, "d1"),
            ("d", HDMI, "d2"),
        ];
        let boot = accepted(&mut engine, &capture(1, ORDER, ["d1", "d1"], homes));
        let source = member_workspace(&boot, "b");
        let dest_current = member_workspace(&boot, "c");
        let dest_resident = member_workspace(&boot, "d");
        let baseline = engine.settled_shared_roster().expect("baseline").clone();
        let mut migrate = capture(2, ORDER, ["d1", "d1"], homes);
        command(
            &mut migrate,
            1,
            WorldCommand::MigrateWorkspace {
                source_output: output(DP),
                target_output: output(HDMI),
                workspace: WorkspaceId(source.to_owned()),
            },
        );
        if outcome == "unreadable" {
            for out in &mut migrate.native {
                out.occupancy[1].count = None;
            }
        }
        let issued = accepted(&mut engine, &migrate);
        assert_no_legacy_roster_mutations(&issued);
        assert_eq!(engine.settled_shared_roster(), Some(&baseline));
        if outcome == "unreadable" {
            assert!(engine.desired_shared_roster_plan().is_none());
            assert_eq!(logical_for(&issued, DP, "d2").as_deref(), Some(source));
            continue;
        }
        let plan = engine
            .desired_shared_roster_plan()
            .expect("migration plan")
            .clone();
        assert_eq!(
            &plan.projections[&output(HDMI)][..3],
            &[dest_current, source, dest_resident]
        );
        let moved = plan
            .members
            .iter()
            .find(|step| step.window.0 == "b")
            .expect("move migrated member");
        assert_eq!(moved.before.output, output(DP));
        assert_eq!(moved.before.native, "d2");
        assert_eq!(moved.output, output(HDMI));
        assert_eq!(moved.target, RosterSlot::Existing("d2".to_owned()));
        let occupant = plan
            .members
            .iter()
            .find(|step| step.window.0 == "d")
            .expect("move destination occupant");
        assert_eq!(occupant.before.output, output(HDMI));
        assert_eq!(occupant.before.native, "d2");
        assert_eq!(occupant.target, RosterSlot::Existing("d3".to_owned()));
        let [_, _, _, RosterSlot::Append(append)] = plan.slots.as_slice() else {
            panic!("the only append backs the new spare: {plan:?}");
        };
        let new_order = ["d1", "d2", "d3", "d4"];
        if outcome == "applied" {
            assert_eq!(plan.views.len(), 1);
            assert_eq!(plan.views[0].output, output(HDMI));
            assert_eq!(plan.views[0].target, moved.target);
            assert!(plan.views[0].dependencies.contains(&moved.step_id));
            assert!(plan.views[0].dependencies.contains(&occupant.step_id));
            let proof = exact_plan_feedback(&plan, &["d4"]);
            let arrived = &[
                ("a", DP, "d1"),
                ("b", HDMI, "d2"),
                ("c", HDMI, "d1"),
                ("d", HDMI, "d3"),
            ];
            let settled = apply_feedback(
                &mut engine,
                &capture(3, &new_order, ["d1", "d2"], arrived),
                &proof,
            );
            assert_no_legacy_roster_mutations(&settled);
            assert!(engine.desired_shared_roster_plan().is_none());
            assert_eq!(logical_for(&settled, HDMI, "d2").as_deref(), Some(source));
            assert_eq!(
                logical_for(&settled, HDMI, "d3").as_deref(),
                Some(dest_resident)
            );
            assert!(!ring_ids(&settled, DP).iter().any(|row| row == source));
            assert_eq!(published_view(&settled, HDMI).as_deref(), Some(source));
            continue;
        }
        let refused = feedback(
            &plan,
            [
                (append.clone(), appended_step(ORDER, &new_order)),
                (occupant.step_id.clone(), refused_step(None, None)),
            ],
        );
        let held = apply_feedback(
            &mut engine,
            &capture(3, &new_order, ["d1", "d1"], homes),
            &refused,
        );
        assert_no_legacy_roster_mutations(&held);
        let recovered = engine
            .settled_shared_roster()
            .expect("fresh observation recovers partial order");
        assert_eq!(recovered.native_order, new_order);
        assert_ne!(recovered.epoch, baseline.epoch);
        let replanned = engine
            .desired_shared_roster_plan()
            .expect("replan remaining migration from partial state");
        assert_ne!(replanned.operation_id, plan.operation_id);
        assert!(
            replanned
                .slots
                .iter()
                .all(|slot| matches!(slot, RosterSlot::Existing(_)))
        );
        assert_eq!(member_workspace(&held, "b"), source);
        assert_eq!(member_workspace(&held, "d"), dest_resident);
        assert_eq!(published_view(&held, HDMI), published_view(&boot, HDMI));
    }
}

#[test]
fn disconnect_keeps_the_displaced_member_in_its_own_logical_workspace() {
    let mut engine = slot_engine();
    let boot = accepted(&mut engine, &reference_capture(1, false));
    let displaced = member_workspace(&boot, "c");
    let mut disconnected = capture(
        2,
        ORDER,
        ["d1", "d1"],
        &[("a", DP, "d1"), ("b", DP, "d2"), ("c", DP, "d1")],
    );
    disconnected.screens.pop();
    disconnected.native.pop();
    let reply = accepted(&mut engine, &disconnected);
    assert_no_legacy_roster_mutations(&reply);
    assert_eq!(member_workspace(&reply, "c"), displaced);
    assert!(ring_ids(&reply, DP).iter().any(|row| row == displaced));
    assert!(ring_ids(&reply, HDMI).is_empty());
    assert_eq!(published_view(&reply, DP), published_view(&boot, DP));
}

#[test]
fn stale_request_and_mismatched_feedback_cannot_advance_a_pending_epoch() {
    let mut engine = slot_engine();
    let baseline_capture = reference_capture(1, false);
    accepted(&mut engine, &baseline_capture);
    let mut send = reference_capture(2, false);
    send_append(&mut send, 0, true);
    accepted(&mut engine, &send);
    let plan = engine
        .desired_shared_roster_plan()
        .expect("send plan")
        .clone();
    let applied = exact_plan_feedback(&plan, &[]);
    let stale = engine
        .handle_report_with_roster_feedback_and_prepare(
            &baseline_capture,
            &[],
            &TestGate,
            &applied,
            |_| Ok::<(), std::convert::Infallible>(()),
        )
        .unwrap_or_else(|e| match e {});
    assert!(matches!(stale.outcome, WorldOutcome::Rejected { .. }));
    assert_eq!(engine.desired_shared_roster_plan(), Some(&plan));
    let homes = &[("a", DP, "d2"), ("b", DP, "d1"), ("c", HDMI, "d1")];
    let mut wrong_revision = applied.clone();
    wrong_revision.revision -= 1;
    let reply = apply_feedback(
        &mut engine,
        &capture(3, ORDER, ["d2", "d1"], homes),
        &wrong_revision,
    );
    assert_no_legacy_roster_mutations(&reply);
    assert_eq!(engine.settled_shared_roster(), Some(&plan.baseline));
    assert_eq!(engine.desired_shared_roster_plan(), Some(&plan));
    let mut ambiguous_arrival = capture(4, ORDER, ["d2", "d1"], homes);
    for native in &mut ambiguous_arrival.native {
        native.occupancy[1].count = Some(2);
    }
    let reply = apply_feedback(&mut engine, &ambiguous_arrival, &applied);
    assert_no_legacy_roster_mutations(&reply);
    assert_eq!(engine.settled_shared_roster(), Some(&plan.baseline));
    let reply = accepted(&mut engine, &capture(5, ORDER, ["d2", "d1"], homes));
    assert_no_legacy_roster_mutations(&reply);
    assert!(engine.desired_shared_roster_plan().is_none());
}

#[test]
fn managed_unchanged_maximized_sibling_does_not_fence_stayer_append() {
    let mut engine = slot_engine();
    let make_capture = |seq: u64| {
        let mut cap = capture(
            seq,
            ORDER,
            ["d1", "d1"],
            &[
                ("a", DP, "d1"),
                ("stayer", DP, "d1"),
                ("b", DP, "d2"),
                ("c", HDMI, "d1"),
                ("sib-max", HDMI, "d1"),
            ],
        );
        cap.windows
            .iter_mut()
            .find(|fact| fact.window.0 == "sib-max")
            .expect("sib-max")
            .maximized = Some(true);
        cap
    };
    let boot = accepted(&mut engine, &make_capture(1));
    assert_no_legacy_roster_mutations(&boot);
    assert!(engine.desired_shared_roster_plan().is_none());

    let mut send = make_capture(2);
    send_append(&mut send, 0, true);
    let issued = accepted(&mut engine, &send);
    assert_no_legacy_roster_mutations(&issued);
    assert_sibling_unchanged(&boot, &issued);
    assert_eq!(
        member_workspace(&issued, "sib-max"),
        member_workspace(&boot, "sib-max")
    );
    let plan = engine
        .desired_shared_roster_plan()
        .expect("unchanged maximized sibling must not fence stayer append")
        .clone();
    assert_eq!(plan.slots.len(), 4);
    assert_eq!(plan.slots[0], RosterSlot::Existing("d1".to_owned()));
    assert_eq!(plan.slots[1], RosterSlot::Existing("d2".to_owned()));
    assert_eq!(plan.slots[2], RosterSlot::Existing("d3".to_owned()));
    assert!(matches!(plan.slots[3], RosterSlot::Append(_)));
    assert!(plan.removals.is_empty());
    assert_eq!(plan.members.len(), 1);
    assert_eq!(plan.members[0].window.0, "a");
    assert_eq!(
        plan.members[0].target,
        RosterSlot::Existing("d3".to_owned())
    );
    assert_eq!(plan.views.len(), 1);
    assert_eq!(plan.views[0].output, output(DP));
    assert_eq!(plan.views[0].target, RosterSlot::Existing("d3".to_owned()));
    assert_eq!(
        plan.projections[&output(HDMI)],
        plan.baseline.projections[&output(HDMI)]
    );
}

#[test]
fn managed_unchanged_maximized_sibling_does_not_fence_sole_mover_compaction() {
    let mut engine = slot_engine();
    let make_capture = |seq: u64| {
        let mut cap = capture(
            seq,
            ORDER,
            ["d1", "d1"],
            &[
                ("a", DP, "d1"),
                ("b", DP, "d2"),
                ("c", HDMI, "d1"),
                ("sib-max", HDMI, "d1"),
            ],
        );
        cap.windows
            .iter_mut()
            .find(|fact| fact.window.0 == "sib-max")
            .expect("sib-max")
            .maximized = Some(true);
        cap
    };
    let boot = accepted(&mut engine, &make_capture(1));
    assert_no_legacy_roster_mutations(&boot);

    let mut send = make_capture(2);
    send_append(&mut send, 0, true);
    let issued = accepted(&mut engine, &send);
    assert_no_legacy_roster_mutations(&issued);
    assert_sibling_unchanged(&boot, &issued);
    assert_eq!(
        member_workspace(&issued, "sib-max"),
        member_workspace(&boot, "sib-max")
    );
    let plan = engine
        .desired_shared_roster_plan()
        .expect("unchanged maximized sibling must not fence sole-mover compaction")
        .clone();
    assert_eq!(plan.slots.len(), 3);
    assert!(plan.removals.is_empty());
    assert_eq!(plan.members.len(), 2);
    assert_eq!(plan.views.len(), 1);
    assert_eq!(plan.views[0].output, output(DP));
    assert_eq!(plan.views[0].target, RosterSlot::Existing("d2".to_owned()));
    assert_eq!(
        plan.projections[&output(HDMI)],
        plan.baseline.projections[&output(HDMI)]
    );
}

#[test]
fn affected_output_maximized_occupant_still_fences_stayer_append() {
    let mut engine = slot_engine();
    let make_capture = |seq: u64| {
        let mut cap = reference_capture(seq, true);
        cap.windows
            .iter_mut()
            .find(|fact| fact.window.0 == "stayer")
            .expect("stayer")
            .maximized = Some(true);
        cap
    };
    accepted(&mut engine, &make_capture(1));
    let mut send = make_capture(2);
    send_append(&mut send, 0, true);
    let issued = accepted(&mut engine, &send);
    assert_no_legacy_roster_mutations(&issued);
    assert!(
        engine.desired_shared_roster_plan().is_none(),
        "maximized occupant on affected source output must remain fenced"
    );
}

#[test]
fn unmanaged_sibling_occupant_still_fences_stayer_append() {
    let mut engine = slot_engine();
    let make_capture = |seq: u64| {
        let mut cap = capture(
            seq,
            ORDER,
            ["d1", "d1"],
            &[
                ("a", DP, "d1"),
                ("stayer", DP, "d1"),
                ("b", DP, "d2"),
                ("c", HDMI, "d1"),
                ("sib-unmanaged", HDMI, "d1"),
            ],
        );
        cap.windows
            .iter_mut()
            .find(|fact| fact.window.0 == "sib-unmanaged")
            .expect("sib-unmanaged")
            .managed = Some(false);
        cap
    };
    accepted(&mut engine, &make_capture(1));
    let mut send = make_capture(2);
    send_append(&mut send, 0, true);
    let issued = accepted(&mut engine, &send);
    assert_no_legacy_roster_mutations(&issued);
    assert!(
        engine.desired_shared_roster_plan().is_none(),
        "unmanaged sibling occupant must remain fenced"
    );
}

#[test]
fn wrong_kind_wrong_target_and_ordered_identity_cannot_settle_epoch() {
    // 1) Wrong kind and wrong target on member + view steps, plus wrong preimage order on append.
    for case in ["wrong-kind", "wrong-target", "wrong-append-order"] {
        let mut engine = slot_engine();
        accepted(&mut engine, &reference_capture(1, true));
        let mut send = reference_capture(2, true);
        send_append(&mut send, 0, true);
        accepted(&mut engine, &send);
        let plan = engine
            .desired_shared_roster_plan()
            .expect("stayer append plan")
            .clone();
        let [_, _, _, RosterSlot::Append(append_step)] = plan.slots.as_slice() else {
            panic!("expected append step: {plan:?}");
        };
        let member = &plan.members[0];
        let view = &plan.views[0];
        let new_order = ["d1", "d2", "d3", "d4"];
        let arrived = capture(
            3,
            &new_order,
            ["d3", "d1"],
            &[
                ("a", DP, "d3"),
                ("stayer", DP, "d1"),
                ("b", DP, "d2"),
                ("c", HDMI, "d1"),
            ],
        );
        let bad = match case {
            "wrong-kind" => feedback(
                &plan,
                [
                    (append_step.clone(), appended_step(ORDER, &new_order)),
                    (member.step_id.clone(), applied_view_step("d1", "d3")),
                    (view.step_id.clone(), applied_home_step(DP, "d1", DP, "d3")),
                ],
            ),
            "wrong-target" => feedback(
                &plan,
                [
                    (append_step.clone(), appended_step(ORDER, &new_order)),
                    (
                        member.step_id.clone(),
                        applied_home_step(DP, "d1", DP, "d4"),
                    ),
                    (view.step_id.clone(), applied_view_step("d1", "d4")),
                ],
            ),
            _ => feedback(
                &plan,
                [
                    (
                        append_step.clone(),
                        appended_step(&["d2", "d1", "d3"], &["d2", "d1", "d3", "d4"]),
                    ),
                    (
                        member.step_id.clone(),
                        applied_home_step(DP, "d1", DP, "d3"),
                    ),
                    (view.step_id.clone(), applied_view_step("d1", "d3")),
                ],
            ),
        };
        apply_feedback(&mut engine, &arrived, &bad);
        assert_eq!(
            engine.desired_shared_roster_plan(),
            Some(&plan),
            "{case} must not settle epoch"
        );
        assert_eq!(engine.settled_shared_roster(), Some(&plan.baseline));
    }

    // 2) Wrong removal identity/order cannot settle a shrink plan.
    {
        let mut engine = slot_engine();
        let oversized = ["d1", "d2", "d3", "unviewed-empty"];
        let homes = &[("a", DP, "d1"), ("b", DP, "d2"), ("c", HDMI, "d1")];
        accepted(&mut engine, &capture(1, &oversized, ["d1", "d1"], homes));
        let plan = engine
            .desired_shared_roster_plan()
            .expect("shrink plan")
            .clone();
        let remove = &plan.removals[0];
        let wrong_removal = feedback(
            &plan,
            [(
                remove.step_id.clone(),
                applied_removal_step(&oversized, &["d1", "d2", "unviewed-empty"]),
            )],
        );
        apply_feedback(
            &mut engine,
            &capture(2, ORDER, ["d1", "d1"], homes),
            &wrong_removal,
        );
        assert_eq!(
            engine.desired_shared_roster_plan(),
            Some(&plan),
            "wrong removal identity must not settle shrink"
        );
        assert_eq!(engine.settled_shared_roster(), Some(&plan.baseline));
    }
}

#[test]
fn correct_proof_with_delayed_observation_settles_without_redelivery() {
    let mut engine = slot_engine();
    accepted(&mut engine, &reference_capture(1, true));
    let mut send = reference_capture(2, true);
    send_append(&mut send, 0, true);
    accepted(&mut engine, &send);
    let plan = engine
        .desired_shared_roster_plan()
        .expect("stayer append plan")
        .clone();
    let proof = exact_plan_feedback(&plan, &["d4"]);

    // Exact proof arrives before the native observation reflects the change.
    apply_feedback(&mut engine, &reference_capture(3, true), &proof);
    assert_eq!(engine.desired_shared_roster_plan(), Some(&plan));

    // Later fresh observation without redelivered feedback settles the epoch.
    let new_order = ["d1", "d2", "d3", "d4"];
    let arrived = capture(
        4,
        &new_order,
        ["d3", "d1"],
        &[
            ("a", DP, "d3"),
            ("stayer", DP, "d1"),
            ("b", DP, "d2"),
            ("c", HDMI, "d1"),
        ],
    );
    let settled = accepted(&mut engine, &arrived);
    assert_no_legacy_roster_mutations(&settled);
    assert!(engine.desired_shared_roster_plan().is_none());
    assert_eq!(
        engine
            .settled_shared_roster()
            .expect("settled")
            .native_order,
        new_order
    );
}

#[test]
fn refusal_is_terminal_and_recovers_only_from_fresh_complete_observation() {
    let mut engine = slot_engine();
    let oversized = ["d1", "d2", "d3", "unviewed-empty"];
    let homes = &[("a", DP, "d1"), ("b", DP, "d2"), ("c", HDMI, "d1")];
    accepted(&mut engine, &capture(1, &oversized, ["d1", "d1"], homes));
    let plan = engine
        .desired_shared_roster_plan()
        .expect("shrink plan")
        .clone();
    let remove = &plan.removals[0];

    // Refusal on an incomplete observation terminates the plan (never re-emitted)
    // but cannot authorize recovery yet.
    let mut incomplete = capture(2, &oversized, ["d1", "d1"], homes);
    incomplete.inventory_complete = false;
    let refused = feedback(&plan, [(remove.step_id.clone(), refused_step(None, None))]);
    apply_feedback(&mut engine, &incomplete, &refused);
    assert!(
        engine.desired_shared_roster_plan().is_none(),
        "refused plan must not be re-emitted"
    );
    assert_eq!(engine.settled_shared_roster(), Some(&plan.baseline));

    // Fresh complete observation showing the surplus already gone recovers cleanly
    // with no held writes and no new plan.
    let recovered = accepted(&mut engine, &capture(3, ORDER, ["d1", "d1"], homes));
    assert_no_legacy_roster_mutations(&recovered);
    assert!(engine.desired_shared_roster_plan().is_none());
    assert_eq!(
        engine
            .settled_shared_roster()
            .expect("recovered")
            .native_order,
        ORDER
    );
}

#[test]
fn partial_application_then_refusal_replans_from_actual_partial_state() {
    let mut engine = slot_engine();
    accepted(&mut engine, &reference_capture(1, true));
    let mut send = reference_capture(2, true);
    send_append(&mut send, 0, true);
    accepted(&mut engine, &send);
    let plan = engine
        .desired_shared_roster_plan()
        .expect("stayer append plan")
        .clone();
    let [_, _, _, RosterSlot::Append(append_step)] = plan.slots.as_slice() else {
        panic!("expected append step: {plan:?}");
    };
    let member = &plan.members[0];
    let view = &plan.views[0];
    let new_order = ["d1", "d2", "d3", "d4"];

    // Append and member succeeded; view was refused. Fresh observation shows the
    // actual partial state (d4 appended, window a on d3, DP still viewing d1).
    let partial_feedback = feedback(
        &plan,
        [
            (append_step.clone(), appended_step(ORDER, &new_order)),
            (
                member.step_id.clone(),
                applied_home_step(DP, "d1", DP, "d3"),
            ),
            (
                view.step_id.clone(),
                refused_step(
                    Some(RosterReadback::Current("d1".to_owned())),
                    Some(RosterReadback::Current("d1".to_owned())),
                ),
            ),
        ],
    );
    let partial_obs = capture(
        3,
        &new_order,
        ["d1", "d1"],
        &[
            ("a", DP, "d3"),
            ("stayer", DP, "d1"),
            ("b", DP, "d2"),
            ("c", HDMI, "d1"),
        ],
    );
    let reply = apply_feedback(&mut engine, &partial_obs, &partial_feedback);
    assert_no_legacy_roster_mutations(&reply);
    assert_eq!(
        engine
            .settled_shared_roster()
            .expect("settled partial order")
            .native_order,
        new_order
    );
    if let Some(next_plan) = engine.desired_shared_roster_plan() {
        assert_ne!(next_plan.operation_id, plan.operation_id);
        assert_eq!(next_plan.baseline.native_order, new_order);
        assert!(
            next_plan
                .slots
                .iter()
                .all(|slot| matches!(slot, RosterSlot::Existing(_)))
        );
    }
}

#[test]
fn refusal_with_missing_or_mismatched_prefix_is_still_terminal_and_unknown_step_ignored() {
    let mut engine = slot_engine();
    accepted(&mut engine, &reference_capture(1, true));
    let mut send = reference_capture(2, true);
    send_append(&mut send, 0, true);
    accepted(&mut engine, &send);
    let plan = engine
        .desired_shared_roster_plan()
        .expect("stayer append plan")
        .clone();
    let view = &plan.views[0];

    // An unknown step ID with Refused must not terminate the pending plan.
    let unknown_refusal = feedback(
        &plan,
        [("unknown-step-id".to_owned(), refused_step(None, None))],
    );
    apply_feedback(&mut engine, &reference_capture(3, true), &unknown_refusal);
    assert_eq!(
        engine.desired_shared_roster_plan(),
        Some(&plan),
        "unknown step refusal must be ignored"
    );

    // A correlated Refused on a dependent step (view) must terminate the epoch
    // even when prerequisite append/member steps are missing from feedback.
    let mut incomplete = reference_capture(4, true);
    incomplete.inventory_complete = false;
    let missing_prefix_refusal =
        feedback(&plan, [(view.step_id.clone(), refused_step(None, None))]);
    apply_feedback(&mut engine, &incomplete, &missing_prefix_refusal);
    assert!(
        engine.desired_shared_roster_plan().is_none(),
        "refused plan must not be re-emitted before fresh complete observation"
    );

    // Later fresh complete observation recovers and replans a new op.
    accepted(&mut engine, &reference_capture(5, true));
    let replanned = engine
        .desired_shared_roster_plan()
        .expect("fresh complete observation should replan");
    assert_ne!(replanned.operation_id, plan.operation_id);
}

#[test]
fn refusal_recovery_rejects_unknown_window_home_and_mismatched_occupancy() {
    let mut engine = slot_engine();
    let oversized = ["d1", "d2", "d3", "unviewed-empty"];
    let homes = &[("a", DP, "d1"), ("b", DP, "d2"), ("c", HDMI, "d1")];
    accepted(&mut engine, &capture(1, &oversized, ["d1", "d1"], homes));
    let plan = engine
        .desired_shared_roster_plan()
        .expect("shrink plan")
        .clone();
    let remove = &plan.removals[0];

    // Trigger refusal on an observation where a live window has unknown native home.
    let mut unknown_home = capture(2, ORDER, ["d1", "d1"], homes);
    unknown_home
        .windows
        .iter_mut()
        .find(|w| w.window.0 == "a")
        .expect("window a")
        .native_workspace = tiler_core::world_report::NativeWorkspaceFact::Null;
    let refused = feedback(&plan, [(remove.step_id.clone(), refused_step(None, None))]);
    apply_feedback(&mut engine, &unknown_home, &refused);
    assert!(engine.desired_shared_roster_plan().is_none());
    assert_eq!(
        engine.settled_shared_roster(),
        Some(&plan.baseline),
        "unknown per-window home must not authorize refusal recovery"
    );

    // Mismatched occupancy total also cannot authorize recovery.
    let mut bad_occ = capture(3, ORDER, ["d1", "d1"], homes);
    for native in &mut bad_occ.native {
        native.occupancy[0].count = Some(99);
    }
    accepted(&mut engine, &bad_occ);
    assert!(engine.desired_shared_roster_plan().is_none());
    assert_eq!(
        engine.settled_shared_roster(),
        Some(&plan.baseline),
        "mismatched occupancy must not authorize refusal recovery"
    );

    // Fresh complete observation with reconciled homes and occupancy recovers.
    accepted(&mut engine, &capture(4, ORDER, ["d1", "d1"], homes));
    assert!(engine.desired_shared_roster_plan().is_none());
    assert_eq!(
        engine
            .settled_shared_roster()
            .expect("recovered")
            .native_order,
        ORDER
    );
}

#[test]
fn unrecovered_refusal_suppresses_plan_while_preserving_scoped_guards() {
    let mut engine = slot_engine();
    let boot = accepted(&mut engine, &reference_capture(1, true));
    let mut send = reference_capture(2, true);
    send_append(&mut send, 0, true);
    accepted(&mut engine, &send);
    let plan = engine
        .desired_shared_roster_plan()
        .expect("stayer append plan")
        .clone();
    let view = &plan.views[0];

    // Refusal on an incomplete observation that already shows intermediate
    // member/view movement on DP: publication must suppress the old plan while
    // keeping DP's view/home on the settled baseline and holding DP writes.
    let mut incomplete_intermediate = capture(
        3,
        ORDER,
        ["d3", "d1"],
        &[
            ("a", DP, "d3"),
            ("stayer", DP, "d1"),
            ("b", DP, "d2"),
            ("c", HDMI, "d1"),
        ],
    );
    incomplete_intermediate.inventory_complete = false;
    let refused = feedback(&plan, [(view.step_id.clone(), refused_step(None, None))]);
    let reply = apply_feedback(&mut engine, &incomplete_intermediate, &refused);
    assert!(
        engine.desired_shared_roster_plan().is_none(),
        "refused plan must be suppressed before recovery"
    );
    let dp_boot_view = &boot
        .outputs
        .iter()
        .find(|row| row.output.0 == DP)
        .expect("DP boot output")
        .logical_view;
    let dp_reply = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == DP)
        .expect("DP reply output");
    assert_eq!(
        &dp_reply.logical_view, dp_boot_view,
        "unrecovered refusal must keep baseline view instead of intermediate d3"
    );
    assert_eq!(
        member_workspace(&reply, "a"),
        member_workspace(&boot, "a"),
        "unrecovered refusal must keep baseline home for moved member"
    );
    assert!(
        reply
            .windows
            .iter()
            .filter(|row| row.output.0 == DP)
            .all(|row| row.frame_authority != tiler_core::world_report::ScopeAuthority::Controlled),
        "unrecovered refusal must keep DP frame writes held"
    );
}

#[test]
fn recovered_suppressed_transfer_does_not_hold_output_writes_indefinitely() {
    let mut engine = slot_engine();
    accepted(&mut engine, &reference_capture(1, true));
    let mut send = reference_capture(2, true);
    send_append(&mut send, 0, true);
    accepted(&mut engine, &send);
    let plan = engine
        .desired_shared_roster_plan()
        .expect("stayer append plan")
        .clone();
    let view = &plan.views[0];

    // Refusal on a fresh complete observation with unchanged native state
    // recovers and suppresses duplicate replanning of the same refused work.
    // Unrelated frame writes on DP must not stay held indefinitely.
    let refused = feedback(&plan, [(view.step_id.clone(), refused_step(None, None))]);
    let reply = apply_feedback(&mut engine, &reference_capture(3, true), &refused);
    assert!(
        engine.desired_shared_roster_plan().is_none(),
        "identical refused work must not immediately replan"
    );
    assert!(
        reply
            .windows
            .iter()
            .filter(|row| row.output.0 == DP)
            .any(|row| row.frame_authority == tiler_core::world_report::ScopeAuthority::Controlled),
        "recovered suppressed transfer must not hold DP frame writes indefinitely"
    );
}
