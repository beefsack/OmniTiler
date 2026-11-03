//! Native fullscreen production: `SetFullscreen`/`ToggleFullscreen`.
//!
//! Native-only route over `caps.fullscreen`; unsupported consumes a typed
//! `StateUnsupported` rejection and unreadable `fullscreen` consumes a
//! typed `StateUnknown` rejection. Explicit exit from active fullscreen
//! requires `fullscreen_exitable == Some(true)` (`Some(false)` consumes a
//! typed `FullscreenNotExitable` rejection, unknown consumes `StateUnknown`).
//! Entry from maximized enters immediately with maximize preserved
//! underneath: no native unmaximize on entry, no extra maximize on exit,
//! and no maximize/fullscreen overlay vetoes on either leg, so exit stays
//! reachable. Same-state consumes with no writes and no new binding.
//! Production mints one retained `Fullscreen` state target with an exact
//! typed [`NativeFullscreenMeta`](crate::world_state::NativeFullscreenMeta)
//! command-sequence carrier; the existing generic `settle_states`
//! settlement owns correlation, spent-before-holds, terminal dispositions,
//! and no-automatic-retry. A fresh command may reauthorize a spent failed
//! binding; observation alone never mints. No geometry, intent, topology,
//! or maximize writes. Publication rides `decorate_native_fullscreen_state`
//! below, which coexists with the sticky row on one window.

use std::collections::BTreeSet;

use crate::directional::WindowId;
use crate::session::DomainKey;
use crate::world_commands::CommandApplied;
use crate::world_engine::{
    KIND_IN_FLIGHT, KIND_OPERATION, KIND_SCOPE_HELD, MSG_IN_FLIGHT, MSG_OPERATION, MSG_SCOPE_HELD,
    WorldEngine,
};
use crate::world_report::{
    AttemptOutcome, CommandRejectKind, CommandResult, HoldReason, ScopeAuthority, StateProperty,
    StateTarget, TransferKind, WindowDesire, WorldReport,
};
use crate::world_state::{MAX_RETAINED_STATE_BINDINGS, NativeFullscreenMeta};

fn refused(kind: &'static str, message: &'static str) -> CommandApplied {
    CommandApplied::Rejected { kind, message }
}

impl WorldEngine {
    fn fullscreen_current(&self, window: &WindowId) -> Option<bool> {
        self.staged
            .facts
            .get(window)
            .and_then(|fact| fact.fullscreen)
    }

    fn fullscreen_unknown_safety_refusal(&self, window: &WindowId) -> Option<CommandApplied> {
        if self
            .retained
            .iter()
            .any(|transfer| matches!(&transfer.target.kind, crate::world_report::TransferKind::Window(w) if w == window))
        {
            return Some(refused(KIND_IN_FLIGHT, MSG_IN_FLIGHT));
        }
        if self.is_held_window(window) || self.is_excluded_window(window) {
            return Some(refused(KIND_OPERATION, MSG_OPERATION));
        }
        if self
            .staged
            .facts
            .get(window)
            .is_some_and(|fact| fact.minimized == Some(true))
        {
            return Some(refused(KIND_OPERATION, MSG_OPERATION));
        }
        let derived = self.derived_for(window);
        if derived.own_excluded || derived.excluded || derived.hidden {
            return Some(refused(KIND_OPERATION, MSG_OPERATION));
        }
        None
    }

    /// Read-only pre-gates: staged homing, held scopes, readability,
    /// in-flight transfers, and the held/minimized/excluded/hidden safety
    /// gates. Overlay state is never a veto: entry from maximized is
    /// allowed, exit stays reachable under either overlay, and unknown
    /// overlays are never guessed false.
    fn fullscreen_pregates(
        &self,
        window: &WindowId,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Result<DomainKey, CommandApplied> {
        let Some(home) = self.staged_homing(window) else {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        };
        if held_scopes.contains(&home) {
            return Err(refused(KIND_SCOPE_HELD, MSG_SCOPE_HELD));
        }
        if !self
            .staged
            .domains
            .iter()
            .any(|domain| domain.readable && domain.key == home)
        {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        }
        let Some(fact) = self.staged.facts.get(window) else {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        };
        if self
            .retained
            .iter()
            .any(|transfer| matches!(&transfer.target.kind, crate::world_report::TransferKind::Window(w) if w == window))
        {
            return Err(refused(KIND_IN_FLIGHT, MSG_IN_FLIGHT));
        }
        if self.is_held_window(window) {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        }
        if fact.minimized == Some(true) {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        }
        if self.is_excluded_window(window) {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        }
        let derived = self.derived_for(window);
        if derived.own_excluded || derived.excluded || derived.hidden {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        }
        Ok(home)
    }

    /// `SetFullscreen`/`ToggleFullscreen`: `Some(value)` sets, `None`
    /// toggles at execution time.
    pub(crate) fn execute_fullscreen_command(
        &mut self,
        command_seq: u64,
        window: &WindowId,
        desired_some: Option<bool>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        if self.fullscreen_current(window).is_none() {
            if self.fullscreen_unknown_safety_refusal(window).is_some()
                && let Err(rejected) = self.fullscreen_pregates(window, held_scopes)
            {
                return rejected;
            }
            if !self.capabilities().is_some_and(|caps| caps.fullscreen) {
                self.command_result = Some(CommandResult::Rejected {
                    kind: CommandRejectKind::StateUnsupported,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
            self.command_result = Some(CommandResult::Rejected {
                kind: CommandRejectKind::StateUnknown,
            });
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
        }
        let home = match self.fullscreen_pregates(window, held_scopes) {
            Ok(home) => home,
            Err(rejected) => return rejected,
        };
        if !self.capabilities().is_some_and(|caps| caps.fullscreen) {
            self.command_result = Some(CommandResult::Rejected {
                kind: CommandRejectKind::StateUnsupported,
            });
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
        }
        let desired = match desired_some {
            Some(value) => value,
            None => match self.fullscreen_current(window) {
                Some(current) => !current,
                None => {
                    self.command_result = Some(CommandResult::Rejected {
                        kind: CommandRejectKind::StateUnknown,
                    });
                    self.note_command(command_seq);
                    return CommandApplied::NoEffect;
                }
            },
        };
        if self.fullscreen_current(window) == Some(desired) {
            let obsolete_opposite = self
                .retained_states
                .get(&(window.clone(), StateProperty::Fullscreen))
                .is_some_and(|binding| binding.target.value != desired);
            if obsolete_opposite {
                self.retained_states
                    .remove(&(window.clone(), StateProperty::Fullscreen));
            }
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
        }
        if !desired {
            match self
                .staged
                .facts
                .get(window)
                .and_then(|fact| fact.fullscreen_exitable)
            {
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
        if !self
            .retained_states
            .contains_key(&(window.clone(), StateProperty::Fullscreen))
            && self.retained_states.len() >= MAX_RETAINED_STATE_BINDINGS
        {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let revision = self.state_revision + 1;
        self.retained_states.insert(
            (window.clone(), StateProperty::Fullscreen),
            crate::world_state::RetainedStateBinding {
                target: crate::world_state::RetainedStateTarget {
                    window: window.clone(),
                    property: StateProperty::Fullscreen,
                    value: desired,
                    revision,
                },
                pending: None,
                spent: false,
                disposition: AttemptOutcome::Unattempted,
                sticky_meta: None,
                fullscreen_meta: Some(NativeFullscreenMeta { command_seq, home }),
                maximize_meta: None,
            },
        );
        self.note_command(command_seq);
        CommandApplied::NoEffect
    }

    /// Central native fullscreen state publication over the assembled rows.
    /// Only producer-correlated fullscreen bindings (exact typed
    /// [`NativeFullscreenMeta`](crate::world_state::NativeFullscreenMeta))
    /// publish a `StateTarget { property: Fullscreen, value, authority,
    /// command_seq }`. The command sequence is the original typed command;
    /// the retained authorizing revision (`target.revision`) stays internal
    /// and never mints from raw facts or revision changes. Targets retain
    /// across observations; observation-only replies never renew.
    /// `Controlled` only for unspent readable no-held/no-excluded/no-min
    /// no-pending-transfer healthy windows; the subject's own fullscreen/maximized overlays
    /// never veto (exit publishes on fullscreen rows, entry coexists with
    /// maximize). Spent bindings never reauthorize automatically and stay
    /// `Untouched`/`Hold`. Property authority is independent of the row
    /// frame; the row `command_seq` lane is untouched. Coexists with the
    /// sticky row on one window.
    pub(crate) fn decorate_native_fullscreen_state(
        &self,
        report: &WorldReport,
        held_scopes: &BTreeSet<DomainKey>,
        windows: &mut [WindowDesire],
    ) {
        if !self.capabilities().is_some_and(|caps| caps.fullscreen) {
            return;
        }
        for desire in windows.iter_mut() {
            let window = desire.lifetime.clone();
            let Some(binding) = self
                .retained_states
                .get(&(window.clone(), StateProperty::Fullscreen))
            else {
                continue;
            };
            let Some(meta) = binding.fullscreen_meta.as_ref() else {
                continue;
            };
            if desire
                .state
                .iter()
                .any(|row| row.property == StateProperty::Fullscreen)
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
            if let Some(fact) = self.staged.facts.get(&window) {
                if fact.fullscreen.is_none() {
                    controlled = false;
                }
                if !binding.target.value
                    && fact.fullscreen == Some(true)
                    && fact.fullscreen_exitable != Some(true)
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
                property: StateProperty::Fullscreen,
                value: binding.target.value,
                authority,
                command_seq: Some(meta.command_seq),
            });
        }
    }
}
