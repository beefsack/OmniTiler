use super::{PendingRoster, RosterFeedback, RosterSlot, RosterStepProof};
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
        if let Some(feedback) = feedback.filter(|feedback| {
            feedback.operation_id == pending.plan.operation_id
                && feedback.generation == pending.plan.generation
                && feedback.revision == pending.plan.revision
        }) {
            merge_feedback(pending, feedback);
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

fn merge_feedback(pending: &mut PendingRoster, feedback: &RosterFeedback) {
    let plan = &pending.plan;
    let proofs = &mut pending.proofs;
    // Appends have an implicit dependency on the preceding append: its
    // fresh ID is the preimage of the next ordered slot.
    for slot in &plan.slots {
        let RosterSlot::Append(step) = slot else {
            continue;
        };
        match proofs.get(step) {
            Some(RosterStepProof::Appended(_)) => continue,
            Some(_) => break,
            None => {}
        }
        match feedback.steps.get(step) {
            Some(RosterStepProof::Appended(native))
                if !native.is_empty()
                    && !plan.baseline.native_order.contains(native)
                    && !proofs.values().any(|proof| {
                        matches!(proof, RosterStepProof::Appended(other) if other == native)
                    }) =>
            {
                proofs.insert(step.clone(), RosterStepProof::Appended(native.clone()));
            }
            Some(RosterStepProof::Refused) => {
                proofs.insert(step.clone(), RosterStepProof::Refused);
                break;
            }
            _ => break,
        }
    }

    // Use the execution order, not the feedback map's key order. A missing
    // or refused prerequisite cannot authorize the dependent proof.
    for (step, dependencies) in plan
        .members
        .iter()
        .map(|step| (&step.step_id, &step.dependencies))
        .chain(
            plan.views
                .iter()
                .map(|step| (&step.step_id, &step.dependencies)),
        )
        .chain(
            plan.removals
                .iter()
                .map(|step| (&step.step_id, &step.dependencies)),
        )
    {
        if proofs.contains_key(step)
            || !dependencies.iter().all(|dep| {
                matches!(
                    proofs.get(dep),
                    Some(RosterStepProof::Applied | RosterStepProof::Appended(_))
                )
            })
        {
            continue;
        }
        match feedback.steps.get(step) {
            Some(RosterStepProof::Applied) => {
                proofs.insert(step.clone(), RosterStepProof::Applied);
            }
            Some(RosterStepProof::Refused) => {
                proofs.insert(step.clone(), RosterStepProof::Refused);
            }
            _ => {}
        }
    }
}

fn resolve_slot<'a>(pending: &'a PendingRoster, slot: &'a RosterSlot) -> Option<&'a str> {
    match slot {
        RosterSlot::Existing(native) => Some(native),
        RosterSlot::Append(step) => match pending.proofs.get(step) {
            Some(RosterStepProof::Appended(native)) => Some(native),
            _ => None,
        },
    }
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
    let applied = |step| matches!(pending.proofs.get(step), Some(RosterStepProof::Applied));
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
