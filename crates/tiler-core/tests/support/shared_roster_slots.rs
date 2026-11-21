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
use tiler_core::world_roster::{RosterFeedback, RosterSlot, RosterStepProof, SharedRosterPlan};

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
    steps: impl IntoIterator<Item = (String, RosterStepProof)>,
) -> RosterFeedback {
    RosterFeedback {
        operation_id: plan.operation_id.clone(),
        generation: plan.generation.clone(),
        revision: plan.revision,
        steps: steps.into_iter().collect(),
    }
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
    let applied = feedback(
        &plan,
        appends
            .into_iter()
            .map(|id| (id, RosterStepProof::Appended("d4".to_owned())))
            .chain(
                plan.members
                    .iter()
                    .map(|step| (step.step_id.clone(), RosterStepProof::Applied)),
            )
            .chain([(view.step_id.clone(), RosterStepProof::Applied)]),
    );
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
            (b.step_id.clone(), RosterStepProof::Applied),
            (a.step_id.clone(), RosterStepProof::Refused),
        ],
    );
    let observed = apply_feedback(&mut engine, &prefix, &partial);
    assert_no_legacy_roster_mutations(&observed);
    for id in ["a", "b", "c"] {
        assert_eq!(member_workspace(&observed, id), member_workspace(&boot, id));
    }
    assert_eq!(published_view(&observed, DP), published_view(&boot, DP));
    if let Some(focus) = observed.focus_request.as_ref() {
        assert_eq!(Some(focus), plan.focus.as_ref());
    }
    assert_eq!(engine.settled_shared_roster(), Some(&plan.baseline));

    let late = feedback(
        &plan,
        [
            (a.step_id.clone(), RosterStepProof::Applied),
            (view.step_id.clone(), RosterStepProof::Applied),
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
    assert_eq!(member_workspace(&held, "a"), member_workspace(&boot, "a"));
    assert_eq!(member_workspace(&held, "b"), member_workspace(&boot, "b"));
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
    let removed = feedback(&plan, [(remove.step_id.clone(), RosterStepProof::Applied)]);
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
    let proof = feedback(
        &plan,
        [(plan.views[0].step_id.clone(), RosterStepProof::Applied)],
    );
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
    let next_proof = feedback(
        &next_plan,
        [(next_plan.views[0].step_id.clone(), RosterStepProof::Applied)],
    );
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
    let applied = feedback(
        &plan,
        plan.views
            .iter()
            .map(|view| (view.step_id.clone(), RosterStepProof::Applied)),
    );
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
            let proof = feedback(
                &plan,
                [(append.clone(), RosterStepProof::Appended("d4".to_owned()))]
                    .into_iter()
                    .chain(
                        plan.members
                            .iter()
                            .map(|step| (step.step_id.clone(), RosterStepProof::Applied)),
                    )
                    .chain([(plan.views[0].step_id.clone(), RosterStepProof::Applied)]),
            );
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
                (append.clone(), RosterStepProof::Appended("d4".to_owned())),
                (occupant.step_id.clone(), RosterStepProof::Refused),
            ],
        );
        let held = apply_feedback(
            &mut engine,
            &capture(3, &new_order, ["d1", "d1"], homes),
            &refused,
        );
        assert_no_legacy_roster_mutations(&held);
        assert_eq!(engine.settled_shared_roster(), Some(&baseline));
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
    let applied = feedback(
        &plan,
        plan.members
            .iter()
            .map(|step| (step.step_id.clone(), RosterStepProof::Applied))
            .chain(
                plan.views
                    .iter()
                    .map(|step| (step.step_id.clone(), RosterStepProof::Applied)),
            ),
    );
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
