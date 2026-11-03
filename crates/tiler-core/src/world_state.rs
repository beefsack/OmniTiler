//! Internal native-state attempt settlement.

use std::collections::BTreeSet;

use crate::directional::{OutputId, WindowId};
use crate::intent_data::IntentData;
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_report::{AttemptOutcome, FrameSetter, StateAttempt, StateProperty, WorldReport};

pub(crate) const MAX_RETAINED_STATE_BINDINGS: usize = crate::world_report::MAX_WORLD_FACTS * 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedStateTarget {
    pub(crate) window: WindowId,
    pub(crate) property: StateProperty,
    pub(crate) value: bool,
    pub(crate) revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeStickyMeta {
    pub(crate) command_seq: u64,
    pub(crate) prior_saved: Option<IntentData>,
    pub(crate) prior_restored_sticky: Option<bool>,
    pub(crate) prior_restored_floats: bool,
    pub(crate) prior_float: bool,
    pub(crate) home: DomainKey,
    pub(crate) dest: DomainKey,
    pub(crate) requested: bool,
    pub(crate) requested_save: IntentData,
    pub(crate) is_toggle_float_clear: bool,
    pub(crate) followup_done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedStateBinding {
    pub(crate) target: RetainedStateTarget,
    pub(crate) pending: Option<StateAttempt>,
    pub(crate) spent: bool,
    pub(crate) disposition: AttemptOutcome,
    pub(crate) sticky_meta: Option<NativeStickyMeta>,
    pub(crate) fullscreen_meta: Option<NativeFullscreenMeta>,
}

/// Typed native fullscreen command-sequence carrier. Separate from
/// [`NativeStickyMeta`]: fullscreen production mints no intent saves and
/// performs no composed landing, so only the originating typed command
/// sequence (for publication) and the issuing home scope (for lifetime
/// isolation) are retained. The authorizing revision stays internal to
/// the retained target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeFullscreenMeta {
    pub(crate) command_seq: u64,
    pub(crate) home: DomainKey,
}

#[must_use]
pub(crate) fn state_property_supported(
    capabilities: Option<&crate::connection::HostCapabilities>,
    property: StateProperty,
) -> bool {
    match capabilities {
        None => false,
        Some(capabilities) => match property {
            StateProperty::Fullscreen => capabilities.fullscreen,
            StateProperty::Maximized => capabilities.maximize,
            StateProperty::Sticky => capabilities.native_sticky,
            StateProperty::Above => capabilities.above,
        },
    }
}

#[must_use]
pub(crate) fn state_fact_value(
    fact: &crate::world_report::WindowFact,
    property: StateProperty,
) -> Option<bool> {
    match property {
        StateProperty::Fullscreen => fact.fullscreen,
        StateProperty::Maximized => fact.maximized,
        StateProperty::Sticky => fact.sticky,
        StateProperty::Above => fact.above,
    }
}

impl WorldEngine {
    pub(crate) fn settle_states(&mut self, report: &WorldReport) {
        if self.retained_states.len() > MAX_RETAINED_STATE_BINDINGS {
            return;
        }
        for attempt in &report.state_attempts {
            if !state_property_supported(self.capabilities(), attempt.property) {
                continue;
            }
            let key = (attempt.window.clone(), attempt.property);
            let Some(binding) = self.retained_states.get_mut(&key) else {
                continue;
            };
            if binding.target.window != attempt.window
                || binding.target.property != attempt.property
            {
                continue;
            }
            if binding.target.value != attempt.target || binding.target.revision != attempt.revision
            {
                continue;
            }
            if binding.spent {
                continue;
            }
            binding.pending = Some(attempt.clone());
            binding.spent = true;
        }
        let keys: Vec<(WindowId, StateProperty)> = self.retained_states.keys().cloned().collect();
        for key in keys {
            let terminal = self.retained_states.get(&key).is_some_and(|binding| {
                matches!(
                    binding.disposition,
                    AttemptOutcome::Applied | AttemptOutcome::RefusedProven
                )
            });
            if terminal {
                continue;
            }
            let (target, pending) = match self.retained_states.get(&key) {
                Some(binding) => {
                    if binding.target.window != key.0 || binding.target.property != key.1 {
                        continue;
                    }
                    match binding.pending.clone() {
                        Some(pending) => (binding.target.clone(), pending),
                        None => continue,
                    }
                }
                None => continue,
            };
            let next = self.classify_state_attempt(report, &target, &pending);
            if let Some(binding) = self.retained_states.get_mut(&key) {
                binding.disposition = next;
            }
        }
    }

    fn classify_state_attempt(
        &self,
        report: &WorldReport,
        target: &RetainedStateTarget,
        pending: &StateAttempt,
    ) -> AttemptOutcome {
        let current = self.state_readable_value(report, &target.window, target.property);
        match pending.setter {
            FrameSetter::Refused => match (pending.pre, pending.post, current) {
                (Some(pre), Some(post), Some(current))
                    if pre == post && pre != target.value && current == pre =>
                {
                    AttemptOutcome::RefusedProven
                }
                _ => AttemptOutcome::PossiblyExecuted,
            },
            FrameSetter::Accepted => match (pending.post, current) {
                (Some(post), Some(current)) if post == target.value && current == target.value => {
                    AttemptOutcome::Applied
                }
                _ => AttemptOutcome::PossiblyExecuted,
            },
            FrameSetter::Threw => AttemptOutcome::PossiblyExecuted,
        }
    }

    fn state_readable_value(
        &self,
        report: &WorldReport,
        window: &WindowId,
        property: StateProperty,
    ) -> Option<bool> {
        if !report.inventory_complete || report.gesture_barrier {
            return None;
        }
        let fact = self.staged.facts.get(window)?;
        if fact.minimized == Some(true) {
            return None;
        }
        if self.is_held_window(window) || self.is_excluded_window(window) {
            return None;
        }
        let homing = self.staged_homing(window)?;
        let readable = self
            .staged
            .domains
            .iter()
            .any(|domain| domain.key == homing && domain.readable);
        if !readable {
            return None;
        }
        state_fact_value(fact, property)
    }

    pub(crate) fn state_offroute_output(
        &self,
        report: &WorldReport,
        window: &WindowId,
    ) -> Option<OutputId> {
        if let Some(out) = report
            .windows
            .iter()
            .find(|fact| fact.window == *window)
            .and_then(|fact| fact.output.clone())
        {
            return Some(out);
        }
        if let Some(out) = report
            .held_windows
            .iter()
            .find(|held| held.window == *window)
            .and_then(|held| held.output.clone())
        {
            return Some(out);
        }
        if let Some(home) = self.core_homing(window) {
            return Some(home.output);
        }
        self.engine.retained_home_any(window)
    }

    pub(crate) fn prune_state_bindings(&mut self, report: &WorldReport) {
        if !report.closed.is_empty() {
            let closed: BTreeSet<WindowId> = report.closed.iter().cloned().collect();
            self.retained_states
                .retain(|(window, _), _| !closed.contains(window));
        }
        let own: BTreeSet<WindowId> = match (
            self.application_identity()
                .and_then(|identity| identity.process_id.clone()),
            self.capabilities(),
        ) {
            (Some(own), Some(caps))
                if !caps
                    .unavailable_facts
                    .contains(&crate::connection::FactName::ProcessId) =>
            {
                report
                    .windows
                    .iter()
                    .filter(|fact| fact.process_id.as_deref() == Some(own.as_str()))
                    .map(|fact| fact.window.clone())
                    .collect()
            }
            _ => BTreeSet::new(),
        };
        if !own.is_empty() {
            self.retained_states
                .retain(|(window, _), _| !own.contains(window));
        }
        if !report.inventory_complete {
            return;
        }
        let live: BTreeSet<&WindowId> = report.windows.iter().map(|fact| &fact.window).collect();
        let held: BTreeSet<&WindowId> = report
            .held_windows
            .iter()
            .map(|held| &held.window)
            .collect();
        let excluded = self.staged.excluded.clone();
        let known: BTreeSet<WindowId> = self
            .retained_states
            .keys()
            .map(|(window, _)| window.clone())
            .filter(|window| {
                self.engine_knows_window_any(window) || self.members.contains_key(window)
            })
            .collect();
        self.retained_states.retain(|(window, _), _| {
            live.contains(window)
                || held.contains(window)
                || excluded.contains(window)
                || known.contains(window)
        });
    }
}
