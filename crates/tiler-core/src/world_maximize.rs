//! Native maximize production: `ToggleMaximize` over caps.maximize.
//!
//! Native-only route over `caps.maximize`; unsupported keeps the existing
//! descriptive result-only path (no native targets, no fake
//! `StateUnsupported`). Readable unknown `fullscreen`/`maximized` consumes
//! a typed `StateUnknown` rejection, with the held/minimized/excluded/
//! hidden safety refusals taking precedence before any unknown mapping.
//! A needed fullscreen clear additionally requires `caps.fullscreen`
//! (`StateUnsupported` without it) and `fullscreen_exitable == Some(true)`
//! (`FullscreenNotExitable` for `Some(false)`, `StateUnknown` for unknown).
//! Ordinary toggles mint one `Maximized` retained target; fullscreen
//! replacement mints the exact `Fullscreen=false` plus `Maximized=true`
//! pair at the same revision and command sequence (the adapter clears
//! fullscreen first), or just the fullscreen leg when already maximized
//! (cancelling the obsolete opposite maximize binding). Same-value dead
//! bindings may remain. The atomic cap reserves every needed key before
//! any mutation, cancellation, result, or high-water advance. Maximize
//! does not veto unmaximize; active fullscreen permits only the paired
//! replacement. Publication correlates the command sequence, home and
//! paired-clear flag in
//! [`NativeMaximizeMeta`](crate::world_state::NativeMaximizeMeta).

use crate::directional::WindowId;
use crate::session::DomainKey;
use crate::world_commands::CommandApplied;
use crate::world_engine::{KIND_OPERATION, MSG_OPERATION, WorldEngine};
use crate::world_report::{
    AttemptOutcome, CommandRejectKind, CommandResult, HoldReason, ScopeAuthority, StateProperty,
    StateTarget, TransferKind, WindowDesire, WorldReport,
};
use crate::world_state::{MAX_RETAINED_STATE_BINDINGS, NativeFullscreenMeta, NativeMaximizeMeta};

fn refused(kind: &'static str, message: &'static str) -> CommandApplied {
    CommandApplied::Rejected { kind, message }
}

impl WorldEngine {
    /// Native maximize toggle after the shared focused/subject/homing/
    /// readability/held-scope/in-flight gates. `home` is the staged homing
    /// scope; `fullscreen`/`maximized` are the proven raw values and
    /// `exitable` the staged exitability fact.
    pub(crate) fn execute_native_maximize(
        &mut self,
        command_seq: u64,
        window: &WindowId,
        home: DomainKey,
        fullscreen: bool,
        maximized: bool,
        exitable: Option<bool>,
    ) -> CommandApplied {
        if self.is_held_window(window) {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        if self
            .staged
            .facts
            .get(window)
            .is_some_and(|fact| fact.minimized == Some(true))
        {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        if self.is_excluded_window(window) {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let derived = self.derived_for(window);
        if derived.own_excluded || derived.excluded || derived.hidden {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let decision = crate::session::Session::decide_maximize_toggle(fullscreen, maximized);
        if decision.clear_fullscreen {
            if !self.capabilities().is_some_and(|caps| caps.fullscreen) {
                self.command_result = Some(CommandResult::Rejected {
                    kind: CommandRejectKind::StateUnsupported,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
            match exitable {
                Some(true) => {}
                Some(false) => {
                    self.command_result = Some(CommandResult::Rejected {
                        kind: CommandRejectKind::FullscreenNotExitable,
                    });
                    self.note_command(command_seq);
                    return CommandApplied::NoEffect;
                }
                None => {
                    self.command_result = Some(CommandResult::Rejected {
                        kind: CommandRejectKind::StateUnknown,
                    });
                    self.note_command(command_seq);
                    return CommandApplied::NoEffect;
                }
            }
        }
        let max_key = (window.clone(), StateProperty::Maximized);
        let full_key = (window.clone(), StateProperty::Fullscreen);
        let need_full = decision.clear_fullscreen;
        let need_max = !decision.clear_fullscreen || !maximized;
        let mut missing = 0;
        if need_max && !self.retained_states.contains_key(&max_key) {
            missing += 1;
        }
        if need_full && !self.retained_states.contains_key(&full_key) {
            missing += 1;
        }
        if self.retained_states.len() + missing > MAX_RETAINED_STATE_BINDINGS {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let revision = self.state_revision + 1;
        if decision.clear_fullscreen {
            // One actual fullscreen-false leg for both the dual pair and
            // the already-maximized single: the adapter clears fullscreen
            // first, ordering max after it with no intermediate wait.
            self.retained_states.insert(
                full_key.clone(),
                crate::world_state::RetainedStateBinding {
                    target: crate::world_state::RetainedStateTarget {
                        window: window.clone(),
                        property: StateProperty::Fullscreen,
                        value: false,
                        revision,
                    },
                    pending: None,
                    spent: false,
                    disposition: AttemptOutcome::Unattempted,
                    sticky_meta: None,
                    fullscreen_meta: Some(NativeFullscreenMeta {
                        command_seq,
                        home: home.clone(),
                    }),
                    maximize_meta: None,
                    above_meta: None,
                },
            );
        }
        if need_max {
            self.retained_states.insert(
                max_key,
                crate::world_state::RetainedStateBinding {
                    target: crate::world_state::RetainedStateTarget {
                        window: window.clone(),
                        property: StateProperty::Maximized,
                        value: decision.maximized,
                        revision,
                    },
                    pending: None,
                    spent: false,
                    disposition: AttemptOutcome::Unattempted,
                    sticky_meta: None,
                    fullscreen_meta: None,
                    maximize_meta: Some(NativeMaximizeMeta {
                        command_seq,
                        home,
                        clear_fullscreen: decision.clear_fullscreen,
                    }),
                    above_meta: None,
                },
            );
        } else if self
            .retained_states
            .get(&max_key)
            .is_some_and(|binding| binding.target.value != maximized)
        {
            // Already maximized: cancel the obsolete opposite maximize
            // binding; no extra maximize write.
            self.retained_states.remove(&max_key);
        }
        if !decision.clear_fullscreen
            && self
                .retained_states
                .get(&full_key)
                .is_some_and(|binding| binding.target.value)
        {
            // Ordinary maximize: revoke the stale opposite fullscreen
            // entry so an old pending fullscreen cannot undo this command.
            self.retained_states.remove(&full_key);
        }
        self.command_result = Some(CommandResult::Maximize {
            window: window.clone(),
            maximized: decision.maximized,
            clear_fullscreen: decision.clear_fullscreen,
            was_maximized: maximized,
        });
        self.note_command(command_seq);
        CommandApplied::NoEffect
    }

    /// Central native maximize state publication over the assembled rows.
    /// Only producer-correlated maximize bindings (exact typed
    /// [`NativeMaximizeMeta`](crate::world_state::NativeMaximizeMeta))
    /// publish a `StateTarget { property: Maximized, value, authority,
    /// command_seq }`. The command sequence is the original typed command;
    /// the authorizing revision stays internal and never mints from raw
    /// facts or revision changes. Targets retain across observations;
    /// observation-only replies never renew. `Controlled` only for unspent
    /// readable no-held/no-excluded/no-min no-pending-transfer healthy
    /// windows. Fullscreen takes precedence while raw fullscreen reads
    /// true: only the exact paired RMAX11 replacement (maximize `true`
    /// with the matching fullscreen-false leg from the same command)
    /// publishes `Controlled`, and no other combination does. A newer
    /// fullscreen-true target with no observed attempt yet
    /// additionally holds maximize before raw facts flip, so a newer
    /// fullscreen-on never shares a reply with Maximized `Controlled`.
    /// A maximized overlay never vetoes (unmaximize stays reachable under a maximize
    /// overlay); an active fullscreen overlay always holds. Property
    /// authority is independent of the row frame; the row `command_seq`
    /// lane is untouched. Runs after the fullscreen decorator so the
    /// paired-clear gate can inspect the exact clear row authority.
    /// While raw fullscreen stays true, only a maximizing `true` target
    /// from a paired replacement (`meta.clear_fullscreen` with the exact
    /// paired fullscreen-false leg at the same authorizing revision and
    /// typed command sequence) whose current published clear row is
    /// `Controlled` publishes `Controlled`; every other maximize target
    /// (unpaired, ordinary, or unmaximize) holds. The exitable check lives
    /// in that fullscreen row and maximize obeys its authority, never
    /// stale metadata. While raw fullscreen is false an unspent tuple
    /// resumes without a new command. A missing, superseded, mismatched, or spent-failed clear leg holds maximize
    /// while raw fullscreen stays true, as does a later fullscreen entry
    /// that supersedes the paired clear. Spent bindings never reauthorize.
    pub(crate) fn decorate_native_maximize_state(
        &self,
        report: &WorldReport,
        held_scopes: &std::collections::BTreeSet<DomainKey>,
        windows: &mut [WindowDesire],
    ) {
        if !self.capabilities().is_some_and(|caps| caps.maximize) {
            return;
        }
        for desire in windows.iter_mut() {
            let window = desire.lifetime.clone();
            let Some(binding) = self
                .retained_states
                .get(&(window.clone(), StateProperty::Maximized))
            else {
                continue;
            };
            let Some(meta) = binding.maximize_meta.as_ref() else {
                continue;
            };
            if desire
                .state
                .iter()
                .any(|row| row.property == StateProperty::Maximized)
            {
                continue;
            }
            let home = self.staged_homing(&window).unwrap_or(meta.home.clone());
            let derived = self.derived_for(&window);
            let excluded_lane =
                self.is_excluded_window(&window) || derived.excluded || derived.own_excluded;
            let domain_readable = self
                .staged
                .domains
                .iter()
                .any(|domain| domain.key == home && domain.readable);
            let mut controlled = !binding.spent
                && report.inventory_complete
                && !report.gesture_barrier
                && !held_scopes.contains(&home)
                && !self.is_held_window(&window)
                && !excluded_lane
                && domain_readable
                && !self.retained.iter().any(
                    |transfer| matches!(&transfer.target.kind, TransferKind::Window(w) if *w == window),
                )
                && (self.engine_knows_window_any(&window) || self.members.contains_key(&window));
            match self.staged.facts.get(&window) {
                Some(fact) => {
                    if fact.minimized == Some(true) || fact.host_hidden != Some(false) {
                        controlled = false;
                    }
                }
                None => controlled = false,
            }
            if derived.hold || derived.hidden {
                controlled = false;
            }
            if binding.spent {
                controlled = false;
            }
            let raw_full = self
                .staged
                .facts
                .get(&window)
                .and_then(|fact| fact.fullscreen);
            let raw_max = self
                .staged
                .facts
                .get(&window)
                .and_then(|fact| fact.maximized);
            if raw_max.is_none() || raw_full.is_none() {
                controlled = false;
            }
            if controlled
                && binding.pending.is_none()
                && self
                    .retained_states
                    .get(&(window.clone(), StateProperty::Fullscreen))
                    .is_some_and(|full| {
                        full.target.value
                            && full.pending.is_none()
                            && full.fullscreen_meta.is_some()
                            && full.target.revision > binding.target.revision
                    })
            {
                // Before an attempt is observed, newer fullscreen-on holds
                // maximize even with unchanged raw facts. Afterward, the
                // raw gate holds active fullscreen; readable false releases
                // the original maximize tuple without retrying fullscreen.
                controlled = false;
            }
            if controlled && raw_full == Some(true) {
                let paired =
                    self.retained_states
                        .get(&(window.clone(), StateProperty::Fullscreen))
                        .filter(|full| {
                            !full.target.value
                                && full.target.revision == binding.target.revision
                                && full.fullscreen_meta.as_ref().is_some_and(|full_meta| {
                                    full_meta.command_seq == meta.command_seq
                                })
                        });
                let clear_controlled = desire.state.iter().any(|row| {
                    row.property == StateProperty::Fullscreen
                        && !row.value
                        && row.authority == ScopeAuthority::Controlled
                        && row.command_seq == Some(meta.command_seq)
                });
                if !binding.target.value
                    || !meta.clear_fullscreen
                    || paired.is_none()
                    || !clear_controlled
                {
                    controlled = false;
                }
            }
            let authority = if controlled {
                ScopeAuthority::Controlled
            } else if excluded_lane {
                ScopeAuthority::Untouched
            } else if report.gesture_barrier {
                ScopeAuthority::Hold(HoldReason::GestureBarrier)
            } else if !report.inventory_complete {
                ScopeAuthority::Hold(HoldReason::IncompleteInventory)
            } else if self
                .staged
                .facts
                .get(&window)
                .is_some_and(|fact| fact.minimized == Some(true))
            {
                ScopeAuthority::Hold(HoldReason::Minimized)
            } else if let Some(reason) = self.hold_reasons.get(&home).copied() {
                ScopeAuthority::Hold(reason)
            } else {
                ScopeAuthority::Hold(HoldReason::Unreadable)
            };
            desire.state.push(StateTarget {
                property: StateProperty::Maximized,
                value: binding.target.value,
                authority,
                command_seq: Some(meta.command_seq),
            });
        }
    }
}
