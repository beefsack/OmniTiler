//! Core-owned epochs and exact native authority for a shared pager.
//! A projection is a real-row prefix; remaining native slots are padding.

mod choice;
mod proof;

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::session::DomainKey;
use crate::workspace::{WorkspaceFacts, plan_trailing};
use crate::world_engine::WorldEngine;
use crate::world_report::{FocusRequest, TransferKind, WindowFact};
use crate::world_transaction::WorldTransaction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettledRoster {
    pub epoch: u64,
    pub native_order: Vec<String>,
    pub projections: BTreeMap<OutputId, Vec<String>>,
}

impl SettledRoster {
    fn logical(&self, output: &OutputId, native: &str) -> Option<&str> {
        let index = self.native_order.iter().position(|id| id == native)?;
        Some(self.projections.get(output)?.get(index)?.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RosterSlot {
    Existing(String),
    /// The appending step's ID; its native ID does not exist yet.
    Append(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHome {
    pub output: OutputId,
    pub native: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterMemberStep {
    pub step_id: String,
    pub window: WindowId,
    pub before: NativeHome,
    pub output: OutputId,
    pub target: RosterSlot,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterViewStep {
    pub step_id: String,
    pub output: OutputId,
    pub before: String,
    pub target: RosterSlot,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterRemovalStep {
    pub step_id: String,
    pub native: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedRosterPlan {
    pub operation_id: String,
    pub generation: String,
    pub revision: u64,
    pub observation_seq: u64,
    pub baseline: SettledRoster,
    pub baseline_currents: BTreeMap<OutputId, String>,
    /// Exact, deduplicated native occupancy for each affected existing slot.
    pub baseline_occupants: BTreeMap<String, BTreeMap<WindowId, OutputId>>,
    pub projections: BTreeMap<OutputId, Vec<String>>,
    pub slots: Vec<RosterSlot>,
    pub members: Vec<RosterMemberStep>,
    pub views: Vec<RosterViewStep>,
    pub removals: Vec<RosterRemovalStep>,
    /// Original focus authority, gated by the preceding native steps.
    pub focus: Option<FocusRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RosterReadback {
    Order(Vec<String>),
    Home(NativeHome),
    Current(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosterStepStatus {
    Applied,
    Appended,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterStepFeedback {
    pub status: RosterStepStatus,
    pub before: Option<RosterReadback>,
    pub after: Option<RosterReadback>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterFeedback {
    pub operation_id: String,
    pub generation: String,
    pub revision: u64,
    pub steps: BTreeMap<String, RosterStepFeedback>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LatchedStepProof {
    Applied,
    Appended(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RefusedSlotTarget {
    Existing(String),
    AppendIndex(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefusedRosterWork {
    baseline: SettledRoster,
    baseline_currents: BTreeMap<OutputId, String>,
    existing_slots: Vec<String>,
    append_count: usize,
    members: Vec<(WindowId, NativeHome, OutputId, RefusedSlotTarget)>,
    views: Vec<(OutputId, String, RefusedSlotTarget)>,
    removals: Vec<String>,
    transfer_ids: BTreeSet<String>,
}

struct RosterPlanCandidate<'a> {
    baseline: &'a SettledRoster,
    baseline_currents: &'a BTreeMap<OutputId, String>,
    slots: &'a [RosterSlot],
    members: &'a [RosterMemberStep],
    views: &'a [RosterViewStep],
    removals: &'a [RosterRemovalStep],
    transfer_ids: &'a BTreeSet<String>,
}

impl RefusedRosterWork {
    pub(crate) fn from_pending(pending: &PendingRoster) -> Self {
        let plan = &pending.plan;
        let slot_target = |slot: &RosterSlot| match slot {
            RosterSlot::Existing(native) => RefusedSlotTarget::Existing(native.clone()),
            RosterSlot::Append(step) => {
                let index = plan
                    .slots
                    .iter()
                    .position(|entry| matches!(entry, RosterSlot::Append(id) if id == step))
                    .unwrap_or(usize::MAX);
                RefusedSlotTarget::AppendIndex(index)
            }
        };
        let mut existing_slots = Vec::new();
        let mut append_count = 0;
        for slot in &plan.slots {
            match slot {
                RosterSlot::Existing(native) => existing_slots.push(native.clone()),
                RosterSlot::Append(_) => append_count += 1,
            }
        }
        Self {
            baseline: plan.baseline.clone(),
            baseline_currents: plan.baseline_currents.clone(),
            existing_slots,
            append_count,
            members: plan
                .members
                .iter()
                .map(|step| {
                    (
                        step.window.clone(),
                        step.before.clone(),
                        step.output.clone(),
                        slot_target(&step.target),
                    )
                })
                .collect(),
            views: plan
                .views
                .iter()
                .map(|step| {
                    (
                        step.output.clone(),
                        step.before.clone(),
                        slot_target(&step.target),
                    )
                })
                .collect(),
            removals: plan
                .removals
                .iter()
                .map(|step| step.native.clone())
                .collect(),
            transfer_ids: pending.transfer_ids.clone(),
        }
    }

    fn matches(&self, candidate: &RosterPlanCandidate<'_>) -> bool {
        if self.baseline != *candidate.baseline
            || self.baseline_currents != *candidate.baseline_currents
            || self.transfer_ids != *candidate.transfer_ids
            || self.members.len() != candidate.members.len()
            || self.views.len() != candidate.views.len()
            || self.removals.len() != candidate.removals.len()
        {
            return false;
        }
        let mut existing_idx = 0;
        let mut append_count = 0;
        for slot in candidate.slots {
            match slot {
                RosterSlot::Existing(native) => {
                    if self.existing_slots.get(existing_idx) != Some(native) {
                        return false;
                    }
                    existing_idx += 1;
                }
                RosterSlot::Append(_) => append_count += 1,
            }
        }
        if existing_idx != self.existing_slots.len() || append_count != self.append_count {
            return false;
        }
        let target_matches =
            |expected: &RefusedSlotTarget, actual: &RosterSlot| match (expected, actual) {
                (RefusedSlotTarget::Existing(a), RosterSlot::Existing(b)) => a == b,
                (RefusedSlotTarget::AppendIndex(idx), RosterSlot::Append(step)) => {
                    matches!(candidate.slots.get(*idx), Some(RosterSlot::Append(id)) if id == step)
                }
                _ => false,
            };
        self.members
            .iter()
            .zip(candidate.members)
            .all(|((win, before, out, target), step)| {
                win == &step.window
                    && before == &step.before
                    && out == &step.output
                    && target_matches(target, &step.target)
            })
            && self
                .views
                .iter()
                .zip(candidate.views)
                .all(|((out, before, target), step)| {
                    out == &step.output
                        && before == &step.before
                        && target_matches(target, &step.target)
                })
            && self
                .removals
                .iter()
                .zip(candidate.removals)
                .all(|(native, step)| native == &step.native)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingRoster {
    plan: SharedRosterPlan,
    proofs: BTreeMap<String, LatchedStepProof>,
    transfer_ids: BTreeSet<String>,
    refused: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RosterState {
    settled: Option<SettledRoster>,
    pending: Option<PendingRoster>,
    last_refused: Option<RefusedRosterWork>,
    padding_views: BTreeMap<OutputId, String>,
}

impl RosterState {
    pub(crate) fn plan(&self) -> Option<&SharedRosterPlan> {
        self.pending
            .as_ref()
            .filter(|pending| !pending.refused)
            .map(|pending| &pending.plan)
    }

    pub(crate) fn pending_plan(&self) -> Option<&SharedRosterPlan> {
        self.pending.as_ref().map(|pending| &pending.plan)
    }

    pub(crate) fn settled_ring(&self, output: &OutputId) -> Option<&Vec<String>> {
        self.settled.as_ref()?.projections.get(output)
    }

    pub(crate) fn settled_logical(&self, output: &OutputId, native: &str) -> Option<&str> {
        self.settled.as_ref()?.logical(output, native)
    }
}

impl WorldEngine {
    pub fn desired_shared_roster_plan(&self) -> Option<&SharedRosterPlan> {
        self.world.settlement.shared_roster.plan()
    }

    pub fn settled_shared_roster(&self) -> Option<&SettledRoster> {
        self.world.settlement.shared_roster.settled.as_ref()
    }
}

struct ProjectedMember<'a> {
    fact: &'a WindowFact,
    before: Option<NativeHome>,
    home: DomainKey,
    managed: bool,
    writes_allowed: bool,
}

impl WorldTransaction<'_> {
    pub(crate) fn settled_slot_logical(&self, output: &OutputId, native: &str) -> Option<String> {
        Some(
            self.world
                .settlement
                .shared_roster
                .settled
                .as_ref()?
                .logical(output, native)?
                .to_owned(),
        )
    }

    pub(crate) fn settled_slot_binding(&self, output: &str, logical: &str) -> Option<String> {
        let roster = self.world.settlement.shared_roster.settled.as_ref()?;
        let (_, ring) = roster.projections.iter().find(|(out, _)| out.0 == output)?;
        let index = ring.iter().position(|id| id == logical)?;
        roster.native_order.get(index).cloned()
    }

    pub(crate) fn is_shared_slot_bound(&self, logical: &str) -> bool {
        self.world
            .settlement
            .shared_roster
            .settled
            .as_ref()
            .is_some_and(|roster| {
                roster
                    .projections
                    .values()
                    .any(|ring| ring.iter().any(|id| id == logical))
            })
    }

    pub(crate) fn shared_slot_transition_unproved(&self) -> bool {
        self.connection.shared_roster_slots()
            && (self.world.settlement.shared_roster.pending.is_some()
                || !self.world.settlement.retained.is_empty())
    }

    pub(crate) fn shared_output_write_held(&self, output: &OutputId) -> bool {
        let state = &self.world.settlement;
        if let Some(plan) = state.shared_roster.pending_plan() {
            return plan.views.iter().any(|view| view.output == *output)
                || plan
                    .members
                    .iter()
                    .any(|member| member.before.output == *output || member.output == *output);
        }
        state.retained.iter().any(|transfer| {
            let suppressed = state
                .shared_roster
                .last_refused
                .as_ref()
                .is_some_and(|refused| {
                    refused.transfer_ids.contains(&transfer.target.operation_id)
                });
            if suppressed && !matches!(transfer.target.kind, TransferKind::Migration) {
                return false;
            }
            transfer.source_output == *output || transfer.target.target_output == *output
        })
    }

    /// Pre-plan view remains descriptive truth until the plan proves.
    pub(crate) fn unproved_shared_view(&self, output: &OutputId) -> Option<String> {
        if let Some(plan) = self.world.settlement.shared_roster.pending_plan() {
            return self.settled_slot_logical(output, plan.baseline_currents.get(output)?);
        }
        let current = self
            .world
            .staged
            .observation
            .native
            .iter()
            .find(|entry| entry.output == *output)?
            .current_native
            .as_deref()?;
        self.settled_slot_logical(output, current)
    }

    pub(crate) fn unproved_shared_home(&self, window: &WindowId) -> Option<DomainKey> {
        self.world
            .settlement
            .retained
            .iter()
            .find_map(|transfer| {
                let applies = match &transfer.target.kind {
                    TransferKind::Window(mover) => mover == window,
                    TransferKind::Migration => transfer.members.contains(window),
                    TransferKind::View => false,
                };
                let source_live = self
                    .world
                    .staged
                    .screens
                    .iter()
                    .any(|screen| screen.output == transfer.source_output);
                applies.then(|| {
                    if source_live {
                        DomainKey {
                            output: transfer.source_output.clone(),
                            workspace: WorkspaceId(transfer.source_workspace.clone()),
                        }
                    } else {
                        // Disconnect already rekeyed the logical unit;
                        // its temporary survivor membership isn't a merge.
                        DomainKey {
                            output: transfer.target.target_output.clone(),
                            workspace: WorkspaceId(transfer.target.target_workspace.clone()),
                        }
                    }
                })
            })
            .or_else(|| {
                let plan = self.world.settlement.shared_roster.pending_plan()?;
                let step = plan.members.iter().find(|step| &step.window == window)?;
                Some(DomainKey {
                    output: step.before.output.clone(),
                    workspace: WorkspaceId(
                        self.settled_slot_logical(&step.before.output, &step.before.native)?,
                    ),
                })
            })
    }

    pub(crate) fn shared_desired_binding(&self, output: &str, logical: &str) -> Option<String> {
        let Some(plan) = self.world.settlement.shared_roster.plan() else {
            return self.settled_slot_binding(output, logical);
        };
        let (_, ring) = plan.projections.iter().find(|(out, _)| out.0 == output)?;
        let index = ring.iter().position(|id| id == logical)?;
        match plan.slots.get(index)? {
            RosterSlot::Existing(native) => Some(native.clone()),
            RosterSlot::Append(_) => None,
        }
    }

    /// Intermediate writes have no logical meaning until the epoch settles.
    pub(crate) fn shared_member_intermediate(&self, fact: &WindowFact) -> bool {
        self.world
            .settlement
            .shared_roster
            .pending_plan()
            .is_some_and(|plan| {
                plan.members.iter().any(|step| {
                    step.window == fact.window
                        && (fact.output.as_ref() != Some(&step.before.output)
                            || fact.native_workspace.as_deref() != Some(&step.before.native))
                })
            })
    }

    pub(crate) fn shared_view_intermediate(&self, output: &OutputId, current: &str) -> bool {
        self.world
            .settlement
            .shared_roster
            .pending_plan()
            .is_some_and(|plan| {
                plan.baseline_currents
                    .get(output)
                    .is_none_or(|before| before != current)
            })
    }

    /// Adopt outside order by native ID. Pad only through a positively
    /// observed current/member or surviving real row, not through the full list.
    pub(crate) fn observe_shared_slots(&mut self) {
        if self.world.settlement.shared_roster.pending.is_some() {
            return;
        }
        let Some(native_order) = self.shared_identical_roster_order() else {
            return;
        };
        let previous = self.world.settlement.shared_roster.settled.clone();
        let mut projections = BTreeMap::new();
        for output in self.shared_roster_live_sorted().into_iter().map(OutputId) {
            let Some(entry) = self
                .world
                .staged
                .observation
                .native
                .iter()
                .find(|entry| entry.output == output)
            else {
                continue;
            };
            let current = entry.current_native.clone();
            let occupied = |native: &str| {
                self.world
                    .staged
                    .observation
                    .raw_windows
                    .iter()
                    .any(|fact| {
                        fact.output.as_ref() == Some(&output)
                            && fact.native_workspace.as_deref() == Some(native)
                            && fact.sticky != Some(true)
                    })
            };
            let old_slots: Vec<Option<String>> = native_order
                .iter()
                .map(|native| {
                    previous
                        .as_ref()?
                        .logical(&output, native)
                        .map(str::to_owned)
                })
                .collect();
            let is_new = previous
                .as_ref()
                .is_none_or(|old| !old.projections.contains_key(&output));
            let last = native_order
                .iter()
                .enumerate()
                .rposition(|(index, native)| {
                    old_slots[index].is_some()
                        || current.as_deref() == Some(native)
                        || occupied(native)
                })
                .unwrap_or(0);
            // On initial adoption include one backed spare when available.
            let last = if is_new && occupied(&native_order[last]) {
                (last + 1).min(native_order.len() - 1)
            } else {
                last
            };
            let padding_selection = current
                .as_ref()
                .filter(|_| !is_new)
                .and_then(|current| native_order.iter().position(|id| id == current))
                .filter(|&index| old_slots[index].is_none());
            let desired_diverged = previous
                .as_ref()
                .and_then(|prior| prior.projections.get(&output))
                .zip(self.world.workspace.mapping.get(&output.0))
                .is_some_and(|(old, current)| old != current);
            let mut ring = Vec::with_capacity(last + 1);
            let mut unbound = self
                .world
                .workspace
                .mapping
                .get(&output.0)
                .cloned()
                .unwrap_or_default();
            unbound.retain(|id| !old_slots.iter().flatten().any(|old| old == id));
            for old in old_slots.into_iter().take(last + 1) {
                let id = old.or_else(|| {
                    if is_new && !unbound.is_empty() {
                        Some(unbound.remove(0))
                    } else {
                        self.create_trailing_workspace(&output, self.connection.default_tiled())
                    }
                });
                let Some(id) = id else { return };
                ring.push(id);
            }
            if let Some(index) = padding_selection {
                self.world
                    .settlement
                    .shared_roster
                    .padding_views
                    .insert(output.clone(), ring[index].clone());
            }
            if !desired_diverged {
                self.world
                    .workspace
                    .mapping
                    .insert(output.0.clone(), ring.clone());
            }
            projections.insert(output, ring);
        }
        let changed = previous
            .as_ref()
            .is_none_or(|old| old.native_order != native_order || old.projections != projections);
        if changed {
            self.world.settlement.shared_roster.settled = Some(SettledRoster {
                epoch: previous.as_ref().map_or(1, |old| old.epoch + 1),
                projections,
                native_order,
            });
        }
    }

    /// Next, unlike Append, may enter an already-existing pager padding slot.
    pub(crate) fn materialize_next_shared_slot(
        &mut self,
        output: &OutputId,
        current: &str,
    ) -> Option<String> {
        if !self.connection.shared_roster_slots()
            || self.world.settlement.shared_roster.pending.is_some()
        {
            return None;
        }
        let roster = self.world.settlement.shared_roster.settled.as_ref()?;
        let ring = self.world.workspace.mapping.get(&output.0)?;
        if ring.last().map(String::as_str) != Some(current)
            || ring.len() >= roster.native_order.len()
        {
            return None;
        }
        let id = self.create_trailing_workspace(output, self.connection.default_tiled())?;
        self.world
            .settlement
            .shared_roster
            .padding_views
            .insert(output.clone(), id.clone());
        Some(id)
    }

    pub(crate) fn freeze_shared_roster_focus(&mut self, focus: Option<FocusRequest>) {
        if let Some(pending) = &mut self.world.settlement.shared_roster.pending
            && pending.plan.revision == self.world.staged.revision
        {
            pending.plan.focus = focus;
        }
    }

    pub(crate) fn plan_shared_roster(&mut self, held: &BTreeSet<DomainKey>) {
        if !self.connection.shared_roster_slots()
            || self.world.settlement.shared_roster.pending.is_some()
            || self.world.staged.gesture_barrier
            || self.presentation_style_hold_v4().is_some()
        {
            return;
        }
        let Some(baseline) = self.world.settlement.shared_roster.settled.clone() else {
            return;
        };
        let Some(order) = self.shared_identical_roster_order() else {
            return;
        };
        if order != baseline.native_order || order.len() > 25 {
            return;
        }
        let observations = &self.world.staged.observation;
        let Some(currents): Option<BTreeMap<_, _>> = baseline
            .projections
            .keys()
            .map(|out| {
                let entry = observations
                    .native
                    .iter()
                    .find(|entry| entry.output == *out)?;
                let current = entry
                    .current_native
                    .as_ref()
                    .filter(|current| order.contains(current))?;
                Some((out.clone(), current.clone()))
            })
            .collect()
        else {
            return;
        };

        let mut projected = BTreeMap::new();
        let mut views = BTreeMap::new();
        let mut transfer_ids = BTreeSet::new();
        for out in baseline.projections.keys() {
            let Some(logical) = baseline.logical(out, &currents[out]) else {
                return;
            };
            views.insert(out.clone(), logical.to_owned());
        }
        for transfer in &self.world.settlement.retained {
            if transfer.target.held {
                return;
            }
            let target = &transfer.target;
            let home = DomainKey {
                output: target.target_output.clone(),
                workspace: WorkspaceId(target.target_workspace.clone()),
            };
            transfer_ids.insert(target.operation_id.clone());
            match &target.kind {
                TransferKind::Window(window) => {
                    projected.insert(window.clone(), home);
                }
                TransferKind::Migration => {
                    for window in &transfer.members {
                        projected.insert(window.clone(), home.clone());
                    }
                }
                TransferKind::View => {}
            }
            if target.follow || matches!(target.kind, TransferKind::View | TransferKind::Migration)
            {
                views.insert(
                    target.target_output.clone(),
                    target.target_workspace.clone(),
                );
            }
        }
        let mut members = Vec::new();
        for fact in &observations.raw_windows {
            if self.derived_for(&fact.window).sticky || fact.sticky == Some(true) {
                continue;
            }
            let before = fact
                .output
                .as_ref()
                .zip(fact.native_workspace.as_deref())
                .map(|(output, native)| NativeHome {
                    output: output.clone(),
                    native: native.to_owned(),
                });
            let actual = before.as_ref().and_then(|raw| {
                baseline
                    .logical(&raw.output, &raw.native)
                    .map(|logical| DomainKey {
                        output: raw.output.clone(),
                        workspace: WorkspaceId(logical.to_owned()),
                    })
            });
            let home = projected
                .get(&fact.window)
                .cloned()
                .or_else(|| actual.clone())
                .or_else(|| self.core_homing(&fact.window));
            let Some(home) = home else { continue };
            let classified = self.derived_for(&fact.window);
            let managed = fact.managed != Some(false)
                && !classified.excluded
                && !classified.own_excluded
                && !classified.hold
                && self.world.staged.facts.contains_key(&fact.window);
            let writes_allowed = managed
                && fact.sticky == Some(false)
                && fact.minimized == Some(false)
                && fact.fullscreen == Some(false)
                && fact.maximized != Some(true)
                && fact.privileged != Some(true)
                && fact.modal != Some(true)
                && fact.transient != Some(true)
                && fact.owned != Some(true)
                && !held.contains(&home)
                && actual.as_ref().is_none_or(|actual| !held.contains(actual));
            members.push(ProjectedMember {
                fact,
                before,
                home,
                managed,
                writes_allowed,
            });
        }
        if projected
            .keys()
            .any(|window| !members.iter().any(|member| member.fact.window == *window))
        {
            return;
        }
        let mut rings = BTreeMap::new();
        let mut append_spares = BTreeSet::new();
        for out in baseline.projections.keys() {
            let Some(ring) = self.world.workspace.mapping.get(&out.0) else {
                return;
            };
            let holds_padding_prefix = self
                .world
                .settlement
                .shared_roster
                .padding_views
                .get(out)
                .is_some_and(|id| views.get(out) == Some(id));
            let facts: Vec<_> =
                ring.iter()
                    .map(|id| {
                        let home = DomainKey {
                            output: out.clone(),
                            workspace: WorkspaceId(id.clone()),
                        };
                        let occupied = members.iter().any(|member| member.home == home);
                        let retained =
                            holds_padding_prefix
                                || held.contains(&home)
                                || self.world.staged.held_windows.iter().any(|window| {
                                    self.core_homing(&window.window).as_ref() == Some(&home)
                                })
                                || self.world.workspace.displaced.values().any(|entry| {
                                    entry.dest == out.0 && entry.workspaces.contains(id)
                                })
                                || self.world.staged.homing.iter().any(|(window, key)| {
                                    key == &home && self.derived_for(window).sticky
                                });
                        WorkspaceFacts {
                            empty: !occupied,
                            occupied,
                            visible: views.get(out) == Some(id),
                            retained,
                        }
                    })
                    .collect();
            let trailing = plan_trailing(&facts);
            if trailing.need_append {
                append_spares.insert(out.clone());
            }
            rings.insert(
                out.clone(),
                ring.iter()
                    .enumerate()
                    .filter(|(index, _)| !trailing.remove.contains(index))
                    .map(|(_, id)| id.clone())
                    .collect::<Vec<_>>(),
            );
        }
        if !views
            .iter()
            .all(|(out, view)| rings.get(out).is_some_and(|ring| ring.contains(view)))
        {
            return;
        }
        let target = rings
            .iter()
            .map(|(out, ring)| ring.len() + usize::from(append_spares.contains(out)))
            .max()
            .unwrap_or(0)
            .max(2);
        // This shared ordered pager has a native floor and bounded count.
        if target > 25 {
            return;
        }
        let known: Vec<bool> = order
            .iter()
            .map(|native| {
                let count = observations
                    .raw_windows
                    .iter()
                    .filter(|fact| {
                        fact.sticky != Some(true)
                            && fact.native_workspace.as_deref() == Some(native)
                    })
                    .count();
                observations
                    .native
                    .iter()
                    .filter(|entry| currents.contains_key(&entry.output))
                    .all(|entry| {
                        entry
                            .occupancy
                            .iter()
                            .find(|row| row.native == *native)
                            .and_then(|row| row.count)
                            .is_some_and(|total| total as usize == count)
                    })
            })
            .collect();
        let mut protected = vec![false; order.len()];
        let mut output_unwritable: Vec<BTreeSet<OutputId>> = vec![BTreeSet::new(); order.len()];
        for (index, native) in order.iter().enumerate() {
            let mut global_fence = false;
            for fact in &observations.raw_windows {
                if fact.native_workspace.as_deref() != Some(native) || fact.sticky == Some(true) {
                    continue;
                }
                let Some(out) = fact.output.as_ref() else {
                    global_fence = true;
                    continue;
                };
                match members
                    .iter()
                    .find(|member| member.fact.window == fact.window)
                {
                    Some(member) if member.writes_allowed => {}
                    Some(member) if member.managed => {
                        output_unwritable[index].insert(out.clone());
                    }
                    _ => {
                        global_fence = true;
                    }
                }
            }
            if !global_fence {
                global_fence =
                    baseline.projections.keys().any(|out| {
                        let Some(logical) = baseline.logical(out, native) else {
                            return false;
                        };
                        let key = DomainKey {
                            output: out.clone(),
                            workspace: WorkspaceId(logical.to_owned()),
                        };
                        held.contains(&key)
                            || self.world.staged.homing.iter().any(|(window, home)| {
                                home == &key && self.derived_for(window).sticky
                            })
                    });
            }
            protected[index] = global_fence;
        }
        // Topology owns these logical units until their original output
        // returns. Their backing is not surplus, but a retained displaced
        // workspace remains selectable and can receive an explicit send.
        let displaced: Vec<bool> = order
            .iter()
            .map(|native| {
                self.world.workspace.displaced.values().any(|entry| {
                    baseline
                        .logical(&OutputId(entry.dest.clone()), native)
                        .is_some_and(|logical| entry.workspaces.iter().any(|id| id == logical))
                })
            })
            .collect();
        let mut index_by_member = Vec::with_capacity(members.len());
        for member in &members {
            let Some(index) = rings
                .get(&member.home.output)
                .and_then(|ring| ring.iter().position(|id| id == &member.home.workspace.0))
            else {
                return;
            };
            index_by_member.push(index);
        }
        let view_indexes: BTreeMap<_, _> = views
            .iter()
            .map(|(out, view)| {
                (
                    out.clone(),
                    rings[out]
                        .iter()
                        .position(|id| id == view)
                        .expect("checked projected view"),
                )
            })
            .collect();
        let index_of = |native: &str| order.iter().position(|id| id == native);
        let available_for = |out: &OutputId, index: usize| {
            known[index] && !protected[index] && !output_unwritable[index].contains(out)
        };
        let available =
            |index: usize| known[index] && !protected[index] && output_unwritable[index].is_empty();
        let cost = |slot: usize, native: Option<usize>| -> Option<(usize, usize)> {
            let mut member_writes = 0;
            let mut view_writes = 0;
            for (member, &index) in members.iter().zip(&index_by_member) {
                if slot != index {
                    continue;
                }
                let before = member.before.as_ref()?;
                let old = index_of(&before.native)?;
                if Some(old) == native && before.output == member.home.output {
                    continue;
                }
                if !member.writes_allowed
                    || !available_for(&before.output, old)
                    || native.is_some_and(|index| !available_for(&member.home.output, index))
                {
                    return None;
                }
                member_writes += 1;
            }
            for (out, &index) in &view_indexes {
                if slot != index {
                    continue;
                }
                let old = index_of(&currents[out])?;
                if Some(old) == native {
                    continue;
                }
                if !available_for(out, old)
                    || native.is_some_and(|index| !available_for(out, index))
                {
                    return None;
                }
                view_writes += 1;
            }

            // A fenced slot cannot be relabelled even if no setter is needed.
            if let Some(native) = native {
                let global_relabel_fence = !known[native] || protected[native] || displaced[native];
                for (out, ring) in &rings {
                    if !global_relabel_fence && !output_unwritable[native].contains(out) {
                        continue;
                    }
                    let desired = ring.get(slot).map(String::as_str);
                    if ring.len() == slot && append_spares.contains(out) {
                        return None;
                    }
                    if baseline.logical(out, &order[native]) != desired {
                        return None;
                    }
                }
            }
            Some((member_writes, view_writes))
        };
        let mut retained: Vec<_> = (0..order.len()).collect();
        let mut count = target.max(order.len());
        if target < order.len() {
            let removable: Vec<_> = (0..order.len())
                .map(|index| {
                    available(index)
                        && !displaced[index]
                        && members.iter().all(|member| {
                            member.before.as_ref().is_none_or(|before| {
                                before.native != order[index] || member.writes_allowed
                            })
                        })
                })
                .collect();
            let costs: Vec<Vec<_>> = (0..order.len())
                .map(|slot| {
                    (0..order.len())
                        .map(|native| cost(slot, Some(native)))
                        .collect()
                })
                .collect();
            let Some((chosen, length)) = (target..=order.len()).find_map(|length| {
                choice::choose(&costs[..length], &removable).map(|chosen| (chosen, length))
            }) else {
                return;
            };
            retained = chosen;
            count = length;
        } else if (0..count).any(|slot| cost(slot, (slot < order.len()).then_some(slot)).is_none())
        {
            return;
        }
        let revision = self.world.staged.revision;
        let operation_id = format!("op-roster-{revision}");
        let step = |kind: &str, index: usize| format!("{operation_id}-{kind}-{index}");
        let slots: Vec<_> = (0..count)
            .map(|index| {
                retained.get(index).map_or_else(
                    || RosterSlot::Append(step("append", index)),
                    |native| RosterSlot::Existing(order[*native].clone()),
                )
            })
            .collect();
        let append_ids: Vec<_> = slots
            .iter()
            .filter_map(|slot| match slot {
                RosterSlot::Append(id) => Some(id.clone()),
                RosterSlot::Existing(_) => None,
            })
            .collect();
        let mut moves: Vec<_> = members
            .iter()
            .zip(index_by_member)
            .filter_map(|(member, index)| {
                let before = member.before.as_ref()?;
                let target = &slots[index];
                if member.home.output == before.output
                    && *target == RosterSlot::Existing(before.native.clone())
                {
                    return None;
                }
                Some((
                    !projected.contains_key(&member.fact.window),
                    member.fact.window.clone(),
                    before.clone(),
                    member.home.output.clone(),
                    target.clone(),
                ))
            })
            .collect();
        // Shift existing occupants before incoming command movers where possible.
        moves.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        let members: Vec<_> = moves
            .into_iter()
            .enumerate()
            .map(
                |(index, (_, window, before, output, target))| RosterMemberStep {
                    step_id: step("member", index),
                    window,
                    before,
                    output,
                    target,
                    dependencies: append_ids.clone(),
                },
            )
            .collect();
        let mut preceding = append_ids;
        preceding.extend(members.iter().map(|step| step.step_id.clone()));
        let views: Vec<_> = view_indexes
            .into_iter()
            .filter_map(|(output, slot)| {
                let before = &currents[&output];
                let target = &slots[slot];
                (*target != RosterSlot::Existing(before.clone()))
                    .then(|| (output, before.clone(), target.clone()))
            })
            .enumerate()
            .map(|(index, (output, before, target))| RosterViewStep {
                step_id: step("view", index),
                output,
                before,
                target,
                dependencies: preceding.clone(),
            })
            .collect();
        preceding.extend(views.iter().map(|view| view.step_id.clone()));
        let removals: Vec<_> = order
            .iter()
            .enumerate()
            .filter(|(index, _)| !retained.contains(index))
            .map(|(index, native)| RosterRemovalStep {
                step_id: step("remove", index),
                native: native.clone(),
                dependencies: preceding.clone(),
            })
            .collect();

        if (!preceding.is_empty() || !removals.is_empty())
            && self
                .world
                .settlement
                .shared_roster
                .last_refused
                .as_ref()
                .is_some_and(|refused| {
                    refused.matches(&RosterPlanCandidate {
                        baseline: &baseline,
                        baseline_currents: &currents,
                        slots: &slots,
                        members: &members,
                        views: &views,
                        removals: &removals,
                        transfer_ids: &transfer_ids,
                    })
                })
        {
            return;
        }
        self.world.settlement.shared_roster.last_refused = None;

        for (out, ring) in &mut rings {
            if append_spares.contains(out) {
                let Some(id) =
                    self.reserve_trailing_workspace(out, self.connection.default_tiled())
                else {
                    return;
                };
                ring.push(id);
            }
        }
        if preceding.is_empty()
            && removals.is_empty()
            && baseline.projections == rings
            && transfer_ids.is_empty()
        {
            return;
        }
        if preceding.is_empty() && removals.is_empty() {
            self.finish_shared_roster_epoch(baseline, order, rings, &transfer_ids);
        } else {
            let affected = |native: &str| {
                let desired_index = slots
                    .iter()
                    .position(|slot| matches!(slot, RosterSlot::Existing(id) if id == native));
                desired_index.is_none()
                    || rings.iter().any(|(out, ring)| {
                        baseline.logical(out, native)
                            != desired_index
                                .and_then(|index| ring.get(index))
                                .map(String::as_str)
                    })
                    || members.iter().any(|step| {
                        step.before.native == native
                            || matches!(&step.target, RosterSlot::Existing(id) if id == native)
                    })
                    || views.iter().any(|step| {
                        step.before == native
                            || matches!(&step.target, RosterSlot::Existing(id) if id == native)
                    })
            };
            let baseline_occupants = order
                .iter()
                .filter(|native| affected(native))
                .map(|native| {
                    let occupants = self
                        .world
                        .staged
                        .observation
                        .raw_windows
                        .iter()
                        .filter_map(|fact| {
                            if fact.sticky == Some(true)
                                || fact.native_workspace.as_deref() != Some(native)
                            {
                                return None;
                            }
                            Some((fact.window.clone(), fact.output.clone()?))
                        })
                        .collect();
                    (native.clone(), occupants)
                })
                .collect();
            self.world.settlement.shared_roster.pending = Some(PendingRoster {
                plan: SharedRosterPlan {
                    operation_id,
                    generation: self.connection.core_generation_str.clone(),
                    revision,
                    observation_seq: self.world.staged.observation.observation_seq,
                    baseline,
                    baseline_currents: currents,
                    projections: rings,
                    baseline_occupants,
                    slots,
                    members,
                    views,
                    removals,
                    focus: None,
                },
                proofs: BTreeMap::new(),
                transfer_ids,
                refused: false,
            });
        }
    }

    fn finish_shared_roster_epoch(
        &mut self,
        baseline: SettledRoster,
        native_order: Vec<String>,
        projections: BTreeMap<OutputId, Vec<String>>,
        transfer_ids: &BTreeSet<String>,
    ) {
        for (out, ring) in &projections {
            if let Some(old) = self
                .world
                .workspace
                .mapping
                .insert(out.0.clone(), ring.clone())
            {
                for id in old.into_iter().filter(|id| !ring.contains(id)) {
                    let key = DomainKey {
                        output: out.clone(),
                        workspace: WorkspaceId(id),
                    };
                    self.world.layout.drop_domain(&key);
                    self.world.workspace.tiling.remove(&key);
                }
            }
        }
        for entry in &self.world.staged.observation.native {
            if let Some((ring, index)) = projections.get(&entry.output).zip(
                entry
                    .current_native
                    .as_ref()
                    .and_then(|native| native_order.iter().position(|id| id == native)),
            ) && let Some(view) = ring.get(index)
            {
                self.world
                    .workspace
                    .views
                    .insert(entry.output.0.clone(), view.clone());
                self.world
                    .workspace
                    .history
                    .observe(&entry.output.0, view, ring);
            }
        }
        let epoch = baseline.epoch
            + u64::from(
                baseline.native_order != native_order || baseline.projections != projections,
            );
        self.world.settlement.shared_roster.settled = Some(SettledRoster {
            epoch,
            native_order,
            projections,
        });
        self.world
            .settlement
            .retained
            .retain(|transfer| !transfer_ids.contains(&transfer.target.operation_id));
    }
}

pub(crate) fn changes_roster(command: &crate::world_report::WorldCommand) -> bool {
    use crate::world_report::WorldCommand::*;
    matches!(
        command,
        Send { .. }
            | SendRelative { .. }
            | SendNumbered { .. }
            | SendDirectional { .. }
            | MigrateWorkspace { .. }
            | MigrateDirectional { .. }
            | SelectWorkspace { .. }
            | SelectNumbered { .. }
            | SelectRelative { .. }
            | SelectPrevious { .. }
            | SelectTrailing { .. }
    )
}
