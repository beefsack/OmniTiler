//! Strict v4 DTOs for the shared native roster route.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use tiler_core::bounds::is_opaque_id;
use tiler_core::connection::{HostCapabilities, NativeSelectorScope};
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::world_report::{MAX_REPLY_WINDOWS, MAX_WORLD_SCREENS};
use tiler_core::world_roster::{
    NativeHome, RosterFeedback, RosterMemberStep, RosterRemovalStep, RosterSlot, RosterStepProof,
    RosterViewStep, SettledRoster, SharedRosterPlan,
};

use crate::world_protocol::{
    Result, as_object, check_keys, decode_focus_request, is_correlation_text, is_generation_text,
    malformed, req_id, req_str, req_u64_str, snapshot_invalid, unknown_value,
};
use crate::world_reply_sink::{CountSink, ObjectWriter, ReplySink};
use crate::world_reply_write::{validate_focus_request_v4, write_focus_request_v4};

const MAX_SLOTS: usize = 25;
const MAX_STEPS: usize = 2 * MAX_SLOTS + MAX_WORLD_SCREENS + MAX_REPLY_WINDOWS;

type Obj = Map<String, Value>;

pub(crate) fn supports_roster(host: &HostCapabilities) -> bool {
    host.host_owned
        && host.shared_native_roster
        && host.native_selector_scope == NativeSelectorScope::PerOutput
}

fn object<'a>(value: &'a Value, keys: &[&str]) -> Result<&'a Obj> {
    let map = as_object(value)?;
    check_keys(map, keys)?;
    Ok(map)
}

fn items(value: &Value, cap: usize) -> Result<&[Value]> {
    let items = value.as_array().ok_or_else(malformed)?;
    if items.len() > cap {
        return Err(snapshot_invalid("roster-section-over-cap"));
    }
    Ok(items)
}

fn generation(map: &Obj) -> Result<String> {
    let generation = req_str(map, "gen")?;
    if !is_generation_text(generation) {
        return Err(unknown_value());
    }
    Ok(generation.to_owned())
}

fn correlation(map: &Obj, key: &str) -> Result<String> {
    let id = req_str(map, key)?;
    if !is_correlation_text(id) {
        return Err(unknown_value());
    }
    Ok(id.to_owned())
}

fn ids(value: &Value, cap: usize) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    items(value, cap)?
        .iter()
        .map(|item| {
            let id = item.as_str().ok_or_else(malformed)?;
            if !is_opaque_id(id) {
                return Err(unknown_value());
            }
            if !seen.insert(id) {
                return Err(snapshot_invalid("duplicate-roster-id"));
            }
            Ok(id.to_owned())
        })
        .collect()
}

fn insert<K: Ord, V>(map: &mut BTreeMap<K, V>, key: K, value: V) -> Result<()> {
    if map.insert(key, value).is_some() {
        return Err(snapshot_invalid("duplicate-roster-id"));
    }
    Ok(())
}

fn projections(value: &Value, count: usize) -> Result<BTreeMap<OutputId, Vec<String>>> {
    let mut projections = BTreeMap::new();
    for item in items(value, MAX_WORLD_SCREENS)? {
        let row = object(item, &["out", "rows"])?;
        insert(
            &mut projections,
            OutputId(req_id(row, "out")?),
            ids(&row["rows"], count)?,
        )?;
    }
    Ok(projections)
}

fn slot(value: &Value) -> Result<RosterSlot> {
    let row = as_object(value)?;
    match req_str(row, "kind")? {
        "existing" => {
            check_keys(row, &["kind", "native"])?;
            Ok(RosterSlot::Existing(req_id(row, "native")?))
        }
        "append" => {
            check_keys(row, &["kind", "step"])?;
            Ok(RosterSlot::Append(correlation(row, "step")?))
        }
        _ => Err(unknown_value()),
    }
}

pub(crate) fn decode_plan(value: &Value) -> Result<SharedRosterPlan> {
    let row = object(
        value,
        &[
            "op",
            "gen",
            "rev",
            "obs",
            "baseline",
            "currents",
            "occupants",
            "projections",
            "slots",
            "members",
            "views",
            "removals",
            "focus",
        ],
    )?;
    let baseline = object(&row["baseline"], &["epoch", "order", "projections"])?;
    let native_order = ids(&baseline["order"], MAX_SLOTS)?;
    let baseline = SettledRoster {
        epoch: req_u64_str(baseline, "epoch")?,
        projections: projections(&baseline["projections"], native_order.len())?,
        native_order,
    };
    let mut baseline_currents = BTreeMap::new();
    for item in items(&row["currents"], MAX_WORLD_SCREENS)? {
        let current = object(item, &["out", "native"])?;
        insert(
            &mut baseline_currents,
            OutputId(req_id(current, "out")?),
            req_id(current, "native")?,
        )?;
    }
    let mut baseline_occupants = BTreeMap::new();
    let mut windows_left = MAX_REPLY_WINDOWS;
    for item in items(&row["occupants"], MAX_SLOTS)? {
        let occupant = object(item, &["native", "windows"])?;
        let mut windows = BTreeMap::new();
        let carried = items(&occupant["windows"], windows_left)?;
        windows_left -= carried.len();
        for window in carried {
            let window = object(window, &["win", "out"])?;
            insert(
                &mut windows,
                WindowId(req_id(window, "win")?),
                OutputId(req_id(window, "out")?),
            )?;
        }
        insert(
            &mut baseline_occupants,
            req_id(occupant, "native")?,
            windows,
        )?;
    }
    let slots = items(&row["slots"], MAX_SLOTS)?
        .iter()
        .map(slot)
        .collect::<Result<Vec<_>>>()?;
    let members = items(&row["members"], MAX_REPLY_WINDOWS)?
        .iter()
        .map(|item| {
            let member = object(item, &["step", "win", "before", "out", "target", "deps"])?;
            let before = object(&member["before"], &["out", "native"])?;
            Ok(RosterMemberStep {
                step_id: correlation(member, "step")?,
                window: WindowId(req_id(member, "win")?),
                before: NativeHome {
                    output: OutputId(req_id(before, "out")?),
                    native: req_id(before, "native")?,
                },
                output: OutputId(req_id(member, "out")?),
                target: slot(&member["target"])?,
                dependencies: ids(&member["deps"], MAX_STEPS)?,
            })
        })
        .collect::<Result<_>>()?;
    let views = items(&row["views"], MAX_WORLD_SCREENS)?
        .iter()
        .map(|item| {
            let view = object(item, &["step", "out", "before", "target", "deps"])?;
            Ok(RosterViewStep {
                step_id: correlation(view, "step")?,
                output: OutputId(req_id(view, "out")?),
                before: req_id(view, "before")?,
                target: slot(&view["target"])?,
                dependencies: ids(&view["deps"], MAX_STEPS)?,
            })
        })
        .collect::<Result<_>>()?;
    let removals = items(&row["removals"], MAX_SLOTS)?
        .iter()
        .map(|item| {
            let removal = object(item, &["step", "native", "deps"])?;
            Ok(RosterRemovalStep {
                step_id: correlation(removal, "step")?,
                native: req_id(removal, "native")?,
                dependencies: ids(&removal["deps"], MAX_STEPS)?,
            })
        })
        .collect::<Result<_>>()?;
    let plan = SharedRosterPlan {
        operation_id: correlation(row, "op")?,
        generation: generation(row)?,
        revision: req_u64_str(row, "rev")?,
        observation_seq: req_u64_str(row, "obs")?,
        baseline,
        baseline_currents,
        baseline_occupants,
        projections: projections(&row["projections"], slots.len())?,
        slots,
        members,
        views,
        removals,
        focus: decode_focus_request(&row["focus"])?,
    };
    validate_plan(&plan)?;
    Ok(plan)
}

pub(crate) fn check_publication(
    plan: Option<&SharedRosterPlan>,
    generation: &str,
    revision: u64,
    observation: u64,
    ready: bool,
) -> Result<()> {
    if plan.is_some_and(|plan| {
        !ready
            || plan.generation != generation
            || plan.revision > revision
            || plan.observation_seq > observation
    }) {
        return Err(snapshot_invalid("roster-publication-mismatch"));
    }
    Ok(())
}

fn valid_ids(ids: &[String], cap: usize) -> bool {
    let mut seen = BTreeSet::new();
    ids.len() <= cap && ids.iter().all(|id| is_opaque_id(id) && seen.insert(id))
}

fn add_step<'a>(
    slots: &[RosterSlot],
    steps: &mut BTreeSet<&'a str>,
    step: &'a str,
    deps: &[String],
    target: Option<&RosterSlot>,
) -> bool {
    is_correlation_text(step)
        && valid_ids(deps, MAX_STEPS)
        && deps.iter().all(|id| steps.contains(id.as_str()))
        && target.is_none_or(|slot| {
            slots.contains(slot)
                && match slot {
                    RosterSlot::Append(id) => deps.contains(id),
                    RosterSlot::Existing(_) => true,
                }
        })
        && steps.insert(step)
}

pub(crate) fn validate_plan(plan: &SharedRosterPlan) -> Result<()> {
    let order = &plan.baseline.native_order;
    let slot_cap = plan.slots.len();
    let valid_projections = |projections: &BTreeMap<OutputId, Vec<String>>, cap| {
        projections.len() <= MAX_WORLD_SCREENS
            && projections
                .iter()
                .all(|(out, rows)| is_opaque_id(&out.0) && valid_ids(rows, cap))
    };
    let invalid = || snapshot_invalid("roster-plan-invalid");
    if !is_correlation_text(&plan.operation_id)
        || !is_generation_text(&plan.generation)
        || !valid_ids(order, MAX_SLOTS)
        || slot_cap > MAX_SLOTS
        || !valid_projections(&plan.baseline.projections, order.len())
        || !valid_projections(&plan.projections, slot_cap)
        || plan.baseline_currents.len() > MAX_WORLD_SCREENS
        || !plan
            .baseline_currents
            .iter()
            .all(|(out, native)| is_opaque_id(&out.0) && order.contains(native))
        || plan.baseline_occupants.len() > MAX_SLOTS
        || plan.members.len() > MAX_REPLY_WINDOWS
        || plan.views.len() > MAX_WORLD_SCREENS
        || plan.removals.len() > MAX_SLOTS
    {
        return Err(invalid());
    }
    let mut occupants = BTreeSet::new();
    for (native, windows) in &plan.baseline_occupants {
        if !order.contains(native)
            || !windows.iter().all(|(win, out)| {
                is_opaque_id(&win.0) && is_opaque_id(&out.0) && occupants.insert(win)
            })
            || occupants.len() > MAX_REPLY_WINDOWS
        {
            return Err(invalid());
        }
    }
    let mut steps = BTreeSet::new();
    let mut kept = BTreeSet::new();
    let mut previous = None;
    for slot in &plan.slots {
        match slot {
            RosterSlot::Existing(native) => {
                let Some(index) = order.iter().position(|id| id == native) else {
                    return Err(invalid());
                };
                if !steps.is_empty()
                    || previous.is_some_and(|prev| prev >= index)
                    || !kept.insert(native)
                {
                    return Err(invalid());
                }
                previous = Some(index);
            }
            RosterSlot::Append(step)
                if is_correlation_text(step) && steps.insert(step.as_str()) => {}
            RosterSlot::Append(_) => return Err(invalid()),
        }
    }
    let mut moved = BTreeSet::new();
    for step in &plan.members {
        if !is_opaque_id(&step.window.0)
            || !moved.insert(&step.window)
            || !is_opaque_id(&step.before.output.0)
            || !order.contains(&step.before.native)
            || !is_opaque_id(&step.output.0)
            || (step.before.output == step.output
                && matches!(&step.target, RosterSlot::Existing(id) if *id == step.before.native))
            || !add_step(
                &plan.slots,
                &mut steps,
                &step.step_id,
                &step.dependencies,
                Some(&step.target),
            )
        {
            return Err(invalid());
        }
    }
    let mut selectors = BTreeSet::new();
    for step in &plan.views {
        if !selectors.insert(&step.output)
            || plan.baseline_currents.get(&step.output) != Some(&step.before)
            || matches!(&step.target, RosterSlot::Existing(id) if *id == step.before)
            || !add_step(
                &plan.slots,
                &mut steps,
                &step.step_id,
                &step.dependencies,
                Some(&step.target),
            )
        {
            return Err(invalid());
        }
    }
    let mut removed = BTreeSet::new();
    for step in &plan.removals {
        if !order.contains(&step.native)
            || kept.contains(&step.native)
            || !removed.insert(&step.native)
            || !add_step(
                &plan.slots,
                &mut steps,
                &step.step_id,
                &step.dependencies,
                None,
            )
        {
            return Err(invalid());
        }
    }
    if kept.len() + removed.len() != order.len() {
        return Err(invalid());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Readback {
    Order(Vec<String>),
    Home(NativeHome),
    Current(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StepReadback {
    step: String,
    before: Option<Readback>,
    after: Option<Readback>,
}

/// Checked wire evidence. Replay additionally compares the actual pre/post
/// snapshots and carried step order, not just the reduced core proofs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterFeedbackWire {
    proof: RosterFeedback,
    readbacks: Vec<StepReadback>,
}

impl RosterFeedbackWire {
    pub fn proof(&self) -> &RosterFeedback {
        &self.proof
    }

    pub(crate) fn byte_len(&self) -> usize {
        let mut sink = CountSink(0);
        self.write(&mut sink)
            .expect("writing checked roster evidence is infallible");
        sink.0 + ",\"rosterFeedback\":".len()
    }

    pub(crate) fn write<S: ReplySink>(&self, sink: &mut S) -> Result<()> {
        sink.write_object(|obj| {
            obj.field_str("op", &self.proof.operation_id)?;
            obj.field_str("gen", &self.proof.generation)?;
            obj.field_u64_str("rev", self.proof.revision)?;
            obj.field_array("steps", &self.readbacks, |row, s| {
                s.write_object(|o| {
                    o.field_str("step", &row.step)?;
                    o.field_str(
                        "status",
                        match self.proof.steps[&row.step] {
                            RosterStepProof::Applied => "applied",
                            RosterStepProof::Appended(_) => "appended",
                            RosterStepProof::Refused => "refused",
                        },
                    )?;
                    o.field("before", |s| write_readback(s, row.before.as_ref()))?;
                    o.field("after", |s| write_readback(s, row.after.as_ref()))
                })
            })
        })
    }
}

fn readback(value: &Value) -> Result<Option<Readback>> {
    if value.is_null() {
        return Ok(None);
    }
    let row = as_object(value)?;
    let result = match req_str(row, "kind")? {
        "order" => {
            check_keys(row, &["kind", "order"])?;
            Readback::Order(ids(&row["order"], MAX_SLOTS)?)
        }
        "home" => {
            check_keys(row, &["kind", "out", "native"])?;
            Readback::Home(NativeHome {
                output: OutputId(req_id(row, "out")?),
                native: req_id(row, "native")?,
            })
        }
        "current" => {
            check_keys(row, &["kind", "native"])?;
            Readback::Current(req_id(row, "native")?)
        }
        _ => return Err(unknown_value()),
    };
    Ok(Some(result))
}

pub(crate) fn decode_feedback(value: &Value) -> Result<RosterFeedbackWire> {
    let row = object(value, &["op", "gen", "rev", "steps"])?;
    let mut proof = RosterFeedback {
        operation_id: correlation(row, "op")?,
        generation: generation(row)?,
        revision: req_u64_str(row, "rev")?,
        steps: BTreeMap::new(),
    };
    let steps = items(&row["steps"], MAX_STEPS)?;
    let mut readbacks = Vec::with_capacity(steps.len());
    for item in steps {
        let item = object(item, &["step", "status", "before", "after"])?;
        let step = correlation(item, "step")?;
        let before = readback(&item["before"])?;
        let after = readback(&item["after"])?;
        let same_kind = match (&before, &after) {
            (Some(before), Some(after)) => {
                std::mem::discriminant(before) == std::mem::discriminant(after)
            }
            _ => true,
        };
        let invalid = || snapshot_invalid("roster-readback-invalid");
        let status = match req_str(item, "status")? {
            "appended" => match (&before, &after) {
                (Some(Readback::Order(before)), Some(Readback::Order(after)))
                    if after.len() == before.len() + 1 && after.starts_with(before) =>
                {
                    RosterStepProof::Appended(after[before.len()].clone())
                }
                _ => return Err(invalid()),
            },
            "applied" => match (&before, &after) {
                (Some(Readback::Order(before)), Some(Readback::Order(after)))
                    if !after.is_empty() && before.len() == after.len() + 1 && {
                        let mut rest = after.iter().peekable();
                        for id in before {
                            if rest.peek() == Some(&id) {
                                rest.next();
                            }
                        }
                        rest.next().is_none()
                    } =>
                {
                    RosterStepProof::Applied
                }
                (Some(Readback::Home(before)), Some(Readback::Home(after))) if before != after => {
                    RosterStepProof::Applied
                }
                (Some(Readback::Current(before)), Some(Readback::Current(after)))
                    if before != after =>
                {
                    RosterStepProof::Applied
                }
                _ => return Err(invalid()),
            },
            "refused" if same_kind => RosterStepProof::Refused,
            "refused" => return Err(invalid()),
            _ => return Err(unknown_value()),
        };
        insert(&mut proof.steps, step.clone(), status)?;
        readbacks.push(StepReadback {
            step,
            before,
            after,
        });
    }
    Ok(RosterFeedbackWire { proof, readbacks })
}

fn write_ids<'a, S: ReplySink>(
    sink: &mut S,
    items: impl IntoIterator<Item = &'a String>,
) -> Result<()> {
    write_array(sink, items, |id, s| {
        s.push_json_str(id);
        Ok(())
    })
}

fn write_array<S: ReplySink, T>(
    sink: &mut S,
    items: impl IntoIterator<Item = T>,
    mut write: impl FnMut(T, &mut S) -> Result<()>,
) -> Result<()> {
    sink.push_char('[');
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            sink.push_char(',');
        }
        write(item, sink)?;
    }
    sink.push_char(']');
    Ok(())
}

fn home_fields<S: ReplySink>(obj: &mut ObjectWriter<'_, S>, home: &NativeHome) -> Result<()> {
    obj.field_str("out", &home.output.0)?;
    obj.field_str("native", &home.native)
}

fn write_readback<S: ReplySink>(sink: &mut S, readback: Option<&Readback>) -> Result<()> {
    let Some(readback) = readback else {
        sink.push_str("null");
        return Ok(());
    };
    sink.write_object(|obj| match readback {
        Readback::Order(order) => {
            obj.field_str("kind", "order")?;
            obj.field("order", |s| write_ids(s, order))
        }
        Readback::Home(home) => {
            obj.field_str("kind", "home")?;
            home_fields(obj, home)
        }
        Readback::Current(native) => {
            obj.field_str("kind", "current")?;
            obj.field_str("native", native)
        }
    })
}

fn write_slot<S: ReplySink>(slot: &RosterSlot, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| match slot {
        RosterSlot::Existing(id) => {
            obj.field_str("kind", "existing")?;
            obj.field_str("native", id)
        }
        RosterSlot::Append(step) => {
            obj.field_str("kind", "append")?;
            obj.field_str("step", step)
        }
    })
}

fn write_projections<S: ReplySink>(
    sink: &mut S,
    projections: &BTreeMap<OutputId, Vec<String>>,
) -> Result<()> {
    write_array(sink, projections, |(out, rows), s| {
        s.write_object(|obj| {
            obj.field_str("out", &out.0)?;
            obj.field("rows", |s| write_ids(s, rows))
        })
    })
}

pub(crate) fn write_plan<S: ReplySink>(plan: &SharedRosterPlan, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("op", &plan.operation_id)?;
        obj.field_str("gen", &plan.generation)?;
        obj.field_u64_str("rev", plan.revision)?;
        obj.field_u64_str("obs", plan.observation_seq)?;
        obj.field("baseline", |s| {
            s.write_object(|obj| {
                obj.field_u64_str("epoch", plan.baseline.epoch)?;
                obj.field("order", |s| write_ids(s, &plan.baseline.native_order))?;
                obj.field("projections", |s| {
                    write_projections(s, &plan.baseline.projections)
                })
            })
        })?;
        obj.field("currents", |s| {
            write_array(s, &plan.baseline_currents, |(out, native), s| {
                s.write_object(|o| {
                    o.field_str("out", &out.0)?;
                    o.field_str("native", native)
                })
            })
        })?;
        obj.field("occupants", |s| {
            write_array(s, &plan.baseline_occupants, |(native, windows), s| {
                s.write_object(|o| {
                    o.field_str("native", native)?;
                    o.field("windows", |s| {
                        write_array(s, windows, |(win, out), s| {
                            s.write_object(|o| {
                                o.field_str("win", &win.0)?;
                                o.field_str("out", &out.0)
                            })
                        })
                    })
                })
            })
        })?;
        obj.field("projections", |s| write_projections(s, &plan.projections))?;
        obj.field_array("slots", &plan.slots, write_slot)?;
        obj.field_array("members", &plan.members, |step, s| {
            s.write_object(|o| {
                o.field_str("step", &step.step_id)?;
                o.field_str("win", &step.window.0)?;
                o.field("before", |s| {
                    s.write_object(|o| home_fields(o, &step.before))
                })?;
                o.field_str("out", &step.output.0)?;
                o.field("target", |s| write_slot(&step.target, s))?;
                o.field("deps", |s| write_ids(s, &step.dependencies))
            })
        })?;
        obj.field_array("views", &plan.views, |step, s| {
            s.write_object(|o| {
                o.field_str("step", &step.step_id)?;
                o.field_str("out", &step.output.0)?;
                o.field_str("before", &step.before)?;
                o.field("target", |s| write_slot(&step.target, s))?;
                o.field("deps", |s| write_ids(s, &step.dependencies))
            })
        })?;
        obj.field_array("removals", &plan.removals, |step, s| {
            s.write_object(|o| {
                o.field_str("step", &step.step_id)?;
                o.field_str("native", &step.native)?;
                o.field("deps", |s| write_ids(s, &step.dependencies))
            })
        })?;
        obj.field("focus", |s| match &plan.focus {
            Some(focus) => {
                validate_focus_request_v4(focus)?;
                write_focus_request_v4(focus, s)
            }
            None => {
                s.push_str("null");
                Ok(())
            }
        })
    })
}
