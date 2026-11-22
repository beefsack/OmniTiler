use std::collections::BTreeMap;

use super::{
    LatchedStepProof, PendingRoster, RefusedRosterWork, RosterFeedback, RosterReadback, RosterSlot,
    RosterStepFeedback, RosterStepStatus,
};
use crate::world_staged::StagedReport;
use crate::world_transaction::WorldTransaction;

impl WorldTransaction<'_> {
    pub(crate) fn settle_shared_roster(&mut self, feedback: Option<&RosterFeedback>) {
        let roster = &mut self.world.settlement.shared_roster;
        let Some(pending) = roster.pending.as_mut() else {
            return;
        };
        if roster.settled.as_ref() != Some(&pending.plan.baseline) {
            return;
        }
        let was_refused = pending.refused;
        if let Some(feedback) = feedback.filter(|feedback| {
            feedback.operation_id == pending.plan.operation_id
                && feedback.generation == pending.plan.generation
                && feedback.revision == pending.plan.revision
        }) {
            merge_feedback(pending, feedback);
        }
        if pending.refused {
            if can_recover_refused(pending, &self.world.staged) {
                if !was_refused {
                    roster.last_refused = Some(RefusedRosterWork::from_pending(pending));
                }
                roster.pending = None;
            }
            return;
        }
        // A complete prefix can land before the full readback. Keep it for
        // the next accepted observation without requiring feedback redelivery.
        let Some(order) = proved_order(pending, &self.world.staged) else {
            return;
        };
        if pending.plan.baseline.epoch == u64::MAX {
            return;
        }
        let native_order = order.to_vec();
        let pending = roster
            .pending
            .take()
            .expect("roster proof requires a pending plan");
        self.finish_shared_roster_epoch(
            pending.plan.baseline,
            native_order,
            pending.plan.projections,
            &pending.transfer_ids,
        );
    }
}

fn dependencies_proved(proofs: &BTreeMap<String, LatchedStepProof>, deps: &[String]) -> bool {
    deps.iter().all(|dep| {
        matches!(
            proofs.get(dep),
            Some(LatchedStepProof::Applied | LatchedStepProof::Appended(_))
        )
    })
}

fn resolve_slot_from_proofs<'a>(
    proofs: &'a BTreeMap<String, LatchedStepProof>,
    slot: &'a RosterSlot,
) -> Option<&'a str> {
    match slot {
        RosterSlot::Existing(native) => Some(native),
        RosterSlot::Append(step) => match proofs.get(step) {
            Some(LatchedStepProof::Appended(native)) => Some(native),
            _ => None,
        },
    }
}

fn merge_feedback(pending: &mut PendingRoster, feedback: &RosterFeedback) {
    if pending.refused {
        return;
    }
    let plan = &pending.plan;
    let proofs = &mut pending.proofs;

    // Refused readbacks may be null or show partial state and are never
    // success receipts. Any correlated refusal on a step belonging to this
    // plan is terminal for the epoch even if earlier steps are missing or
    // mismatched; unknown step IDs are ignored.
    let has_refused = plan
        .slots
        .iter()
        .filter_map(|slot| match slot {
            RosterSlot::Append(step) => Some(step.as_str()),
            RosterSlot::Existing(_) => None,
        })
        .chain(plan.members.iter().map(|step| step.step_id.as_str()))
        .chain(plan.views.iter().map(|step| step.step_id.as_str()))
        .chain(plan.removals.iter().map(|step| step.step_id.as_str()))
        .any(|step_id| {
            feedback
                .steps
                .get(step_id)
                .is_some_and(|row| row.status == RosterStepStatus::Refused)
        });
    if has_refused {
        pending.refused = true;
        return;
    }

    // Appends have an implicit dependency on the preceding append: its
    // fresh ID is the preimage of the next ordered slot.
    let mut order = plan.baseline.native_order.clone();
    for slot in &plan.slots {
        let RosterSlot::Append(step) = slot else {
            continue;
        };
        match proofs.get(step) {
            Some(LatchedStepProof::Appended(native)) => {
                order.push(native.clone());
                continue;
            }
            Some(_) => break,
            None => {}
        }
        match feedback.steps.get(step) {
            Some(RosterStepFeedback {
                status: RosterStepStatus::Appended,
                before: Some(RosterReadback::Order(before)),
                after: Some(RosterReadback::Order(after)),
            }) if before == &order
                && after.len() == order.len() + 1
                && after.starts_with(&order) =>
            {
                let native = &after[order.len()];
                if !native.is_empty() && !order.contains(native) {
                    proofs.insert(step.clone(), LatchedStepProof::Appended(native.clone()));
                    order.push(native.clone());
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    // Use the execution order, not the feedback map's key order. A missing
    // prerequisite cannot authorize the dependent proof.
    for step in &plan.members {
        if proofs.contains_key(&step.step_id) || !dependencies_proved(proofs, &step.dependencies) {
            continue;
        }
        let Some(target) = resolve_slot_from_proofs(proofs, &step.target) else {
            continue;
        };
        if let Some(RosterStepFeedback {
            status: RosterStepStatus::Applied,
            before: Some(RosterReadback::Home(before)),
            after: Some(RosterReadback::Home(after)),
        }) = feedback.steps.get(&step.step_id)
            && *before == step.before
            && after.output == step.output
            && after.native == target
            && before != after
        {
            proofs.insert(step.step_id.clone(), LatchedStepProof::Applied);
        }
    }

    for step in &plan.views {
        if proofs.contains_key(&step.step_id) || !dependencies_proved(proofs, &step.dependencies) {
            continue;
        }
        let Some(target) = resolve_slot_from_proofs(proofs, &step.target) else {
            continue;
        };
        if let Some(RosterStepFeedback {
            status: RosterStepStatus::Applied,
            before: Some(RosterReadback::Current(before)),
            after: Some(RosterReadback::Current(after)),
        }) = feedback.steps.get(&step.step_id)
            && before == &step.before
            && after == target
            && before != after
        {
            proofs.insert(step.step_id.clone(), LatchedStepProof::Applied);
        }
    }

    let mut removal_order = order;
    for step in &plan.removals {
        match proofs.get(&step.step_id) {
            Some(LatchedStepProof::Applied) => {
                removal_order.retain(|id| id != &step.native);
                continue;
            }
            Some(_) => break,
            None => {}
        }
        if !dependencies_proved(proofs, &step.dependencies) || !removal_order.contains(&step.native)
        {
            break;
        }
        let mut expected_after = removal_order.clone();
        expected_after.retain(|id| id != &step.native);
        if expected_after.is_empty() {
            break;
        }
        match feedback.steps.get(&step.step_id) {
            Some(RosterStepFeedback {
                status: RosterStepStatus::Applied,
                before: Some(RosterReadback::Order(before)),
                after: Some(RosterReadback::Order(after)),
            }) if before == &removal_order && after == &expected_after => {
                proofs.insert(step.step_id.clone(), LatchedStepProof::Applied);
                removal_order = expected_after;
            }
            _ => break,
        }
    }
}

fn can_recover_refused(pending: &PendingRoster, staged: &StagedReport) -> bool {
    let observation = &staged.observation;
    if !observation.inventory_complete
        || observation.observation_seq <= pending.plan.observation_seq
        || !staged.held_screens.is_empty()
        || !staged.held_native.is_empty()
        || !staged.held_windows.is_empty()
        || staged.screens.is_empty()
        || !staged.screens.iter().all(|screen| screen.complete)
    {
        return false;
    }
    let Some(first) = observation.native.first() else {
        return false;
    };
    if first.native_order.is_empty() {
        return false;
    }
    let order = first.native_order.as_slice();
    // Every live non-closed window must have a known home/sticky status so
    // partial membership is fully established before replanning.
    let mut counts: BTreeMap<&str, u32> = order.iter().map(|id| (id.as_str(), 0)).collect();
    for fact in &observation.raw_windows {
        if staged.closes.contains(&fact.window) {
            continue;
        }
        if fact.output.is_none() {
            return false;
        }
        match fact.sticky {
            Some(true) => {}
            Some(false) => {
                let Some(native) = fact.native_workspace.as_deref() else {
                    return false;
                };
                let Some(count) = counts.get_mut(native) else {
                    return false;
                };
                *count = count.saturating_add(1);
            }
            None => return false,
        }
    }
    staged.screens.iter().all(|screen| {
        observation.native.iter().any(|entry| {
            entry.output == screen.output
                && entry.native_order.as_slice() == order
                && entry
                    .current_native
                    .as_ref()
                    .is_some_and(|current| order.contains(current))
                && order.iter().all(|native| {
                    entry.occupancy.iter().any(|occ| {
                        occ.native == *native && occ.count == counts.get(native.as_str()).copied()
                    })
                })
        })
    })
}

fn resolve_slot<'a>(pending: &'a PendingRoster, slot: &'a RosterSlot) -> Option<&'a str> {
    resolve_slot_from_proofs(&pending.proofs, slot)
}

fn proved_order<'a>(pending: &PendingRoster, staged: &'a StagedReport) -> Option<&'a [String]> {
    let plan = &pending.plan;
    let observation = &staged.observation;
    if !observation.inventory_complete
        || observation.observation_seq <= plan.observation_seq
        || !staged.held_screens.is_empty()
        || !staged.held_native.is_empty()
        || staged.screens.len() != plan.baseline_currents.len()
        || observation.native.len() != plan.baseline_currents.len()
    {
        return None;
    }
    let applied = |step| matches!(pending.proofs.get(step), Some(LatchedStepProof::Applied));
    if !plan.removals.iter().all(|step| applied(&step.step_id))
        || !plan.views.iter().all(|step| applied(&step.step_id))
        || !plan.members.iter().all(|step| {
            applied(&step.step_id)
                && !staged.closes.contains(&step.window)
                && !staged
                    .held_windows
                    .iter()
                    .any(|held| held.window == step.window)
                && resolve_slot(pending, &step.target).is_some_and(|target| {
                    staged.facts.get(&step.window).is_some_and(|fact| {
                        fact.output.as_ref() == Some(&step.output)
                            && fact.sticky != Some(true)
                            && fact.native_workspace.as_deref() == Some(target)
                    })
                })
        })
    {
        return None;
    }
    let order = observation.native.first()?.native_order.as_slice();
    if order.len() != plan.slots.len()
        || !order
            .iter()
            .zip(&plan.slots)
            .all(|(native, slot)| resolve_slot(pending, slot) == Some(native.as_str()))
    {
        return None;
    }
    for (output, before) in &plan.baseline_currents {
        if !staged
            .screens
            .iter()
            .any(|screen| &screen.output == output && screen.complete)
        {
            return None;
        }
        let row = observation
            .native
            .iter()
            .find(|row| &row.output == output)?;
        let expected = match plan.views.iter().find(|step| &step.output == output) {
            Some(step) => resolve_slot(pending, &step.target)?,
            None => before.as_str(),
        };
        if row.native_order.as_slice() != order || row.current_native.as_deref() != Some(expected) {
            return None;
        }
    }
    proves_occupancy(pending, staged, order).then_some(order)
}

fn proves_occupancy(pending: &PendingRoster, staged: &StagedReport, order: &[String]) -> bool {
    use std::collections::BTreeMap;

    let plan = &pending.plan;
    let mut expected = plan.baseline_occupants.clone();
    for slot in &plan.slots {
        if matches!(slot, RosterSlot::Append(_)) {
            let Some(native) = resolve_slot(pending, slot) else {
                return false;
            };
            expected.insert(native.to_owned(), BTreeMap::new());
        }
    }
    for member in &plan.members {
        let Some(source) = expected.get_mut(&member.before.native) else {
            return false;
        };
        if source.remove(&member.window).as_ref() != Some(&member.before.output) {
            return false;
        }
        let Some(target) =
            resolve_slot(pending, &member.target).and_then(|native| expected.get_mut(native))
        else {
            return false;
        };
        if target
            .insert(member.window.clone(), member.output.clone())
            .is_some()
        {
            return false;
        }
    }
    for removal in &plan.removals {
        if !expected
            .remove(&removal.native)
            .is_some_and(|occupants| occupants.is_empty())
            || staged.observation.raw_windows.iter().any(|fact| {
                fact.sticky != Some(true)
                    && fact.native_workspace.as_deref() == Some(&removal.native)
            })
        {
            return false;
        }
    }
    for (native, members) in expected {
        if !order.contains(&native) {
            return false;
        }
        let mut actual = BTreeMap::new();
        for fact in &staged.observation.raw_windows {
            if fact.sticky == Some(true) || fact.native_workspace.as_deref() != Some(&native) {
                continue;
            }
            let Some(output) = fact.output.as_ref() else {
                return false;
            };
            if fact.sticky != Some(false)
                || staged.closes.contains(&fact.window)
                || staged
                    .held_windows
                    .iter()
                    .any(|held| held.window == fact.window)
                || actual.insert(&fact.window, output).is_some()
            {
                return false;
            }
        }
        if actual.len() != members.len()
            || !members
                .iter()
                .all(|(window, out)| actual.get(window) == Some(&out))
        {
            return false;
        }
        if !staged.observation.native.iter().all(|row| {
            row.occupancy.iter().any(|occupant| {
                occupant.native == native
                    && occupant
                        .count
                        .is_some_and(|count| count as usize == actual.len())
            })
        }) {
            return false;
        }
    }
    true
}
