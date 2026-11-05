//! Composed sticky execution: `SetSticky`/`ToggleSticky`, plus `ToggleFloat`
//! clearing sticky on the composed route. Native sticky commands produce
//! retained `Sticky` state targets with exact prior-save metadata.
//!
//! Both-host-absent consumes with a typed `StateUnsupported` rejection;
//! native unknown sticky consumes with typed `StateUnknown`. Transitions
//! never rewrite raw facts: composed mutates provenance maps plus core
//! membership on a clone, then restages (`stage_facts`/
//! `build_staged_domains`) and sweeps the affected domains there. Native
//! off/`ToggleFloat` clears hold runtime (no premature tile/home
//! transition) until correlated clear proof settles through
//! `run_native_sticky_followups`.
//! The live engine adopts the clone only after every affected sweep and
//! homing check succeeds, so envelope rejections leak no map, session,
//! or record mutation. The staged save (`sticky_command_save`) is drained
//! at publish by `drain_intent_saves`; visibility decoration lives in
//! `decorate_sticky_visibility` and native state publication in
//! `decorate_native_sticky_state` below.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreCommand, CoreEvent};
use crate::directional::{WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::intent_data::{IntentData, encode_intent_data};
use crate::session::{DomainKey, OutputDomain};
use crate::world_commands::CommandApplied;
use crate::world_engine::{
    KIND_IN_FLIGHT, KIND_OPERATION, KIND_SCOPE_HELD, MSG_IN_FLIGHT, MSG_OPERATION, MSG_SCOPE_HELD,
    WorldEngine,
};
use crate::world_members::MemberState;
use crate::world_report::{
    AttemptOutcome, CommandRejectKind, CommandResult, HoldReason, IntentWrite, ScopeAuthority,
    StateProperty, StateTarget, TransferKind, VisibilityTarget, WindowDesire,
};
use crate::world_state::{MAX_RETAINED_STATE_BINDINGS, NativeStickyMeta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StickyRoute {
    Native,
    Composed,
    Unsupported,
}

fn refused(kind: &'static str, message: &'static str) -> CommandApplied {
    CommandApplied::Rejected { kind, message }
}

impl WorldEngine {
    pub(crate) fn sticky_route(&self) -> StickyRoute {
        let caps = self.capabilities();
        if caps.is_some_and(|c| c.native_sticky) {
            return StickyRoute::Native;
        }
        if caps.is_some_and(|c| c.window_suppression) {
            return StickyRoute::Composed;
        }
        StickyRoute::Unsupported
    }

    fn usable_pending_sticky(&self, window: &WindowId) -> Option<IntentData> {
        self.pending_intent
            .get(window)
            .copied()
            .filter(|_| !self.intent_conflicts.contains(window))
            .filter(|data| data.sticky_prior.is_some())
    }

    /// Current semantic sticky state: `Some(true)` means sticky (restored
    /// provenance or a usable pending whole record), `Some(false)` means
    /// proven non-sticky, `None` means unknown (native route with an
    /// unreadable `sticky:null`, never inferred).
    pub(crate) fn sticky_current(&self, window: &WindowId) -> Option<bool> {
        // Native observations are authoritative for command decisions: an
        // unreadable `sticky:null` is unknown even when semantic maps
        // (restored provenance, usable pending) claim a value. Composed
        // keeps the semantic fallback.
        if self.sticky_route() == StickyRoute::Native
            && self
                .staged
                .facts
                .get(window)
                .and_then(|fact| fact.sticky)
                .is_none()
        {
            return None;
        }
        if self.restored_sticky.contains_key(window) {
            return Some(true);
        }
        if self.usable_pending_sticky(window).is_some() {
            return Some(true);
        }
        match self.sticky_route() {
            StickyRoute::Composed | StickyRoute::Unsupported => Some(false),
            StickyRoute::Native => self.staged.facts.get(window).and_then(|fact| fact.sticky),
        }
    }

    fn sticky_prior_for_off(&self, window: &WindowId) -> Option<bool> {
        if let Some(prior) = self.restored_sticky.get(window).copied() {
            return Some(prior);
        }
        if let Some(data) = self.usable_pending_sticky(window) {
            return data.sticky_prior;
        }
        // Adopted native sticky with no same-runtime origin: prior-float.
        if self.staged.facts.get(window).and_then(|fact| fact.sticky) == Some(true) {
            return Some(true);
        }
        None
    }

    /// Current-workspace destination on the fact's output: the adapter echo
    /// wins through `effective_view`, else the retained view, else the ring
    /// head. `None` when the output names no known workspace.
    fn sticky_dest(&self, src: &DomainKey, window: &WindowId) -> Option<DomainKey> {
        let output = self
            .staged
            .facts
            .get(window)?
            .output
            .clone()
            .unwrap_or_else(|| src.output.clone());
        let view = self.effective_view(&output)?;
        if !self.known_workspace(&output, &view) {
            return None;
        }
        Some(DomainKey {
            output,
            workspace: WorkspaceId(view),
        })
    }

    fn native_unknown_safety_refusal(&self, window: &WindowId) -> Option<CommandApplied> {
        if self
            .retained
            .iter()
            .any(|transfer| matches!(&transfer.target.kind, TransferKind::Window(w) if w == window))
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
        if derived.own_excluded
            || derived.excluded
            || derived.hidden
            || derived.overlay_fullscreen
            || derived.overlay_maximized
        {
            return Some(refused(KIND_OPERATION, MSG_OPERATION));
        }
        None
    }

    /// Read-only pre-gates: staged homing, held scopes, readability,
    /// in-flight transfers, and the preexisting held/minimized/excluded/
    /// overlay safety gates. Returns `(source, current-workspace dest)`.
    fn sticky_pregates(
        &self,
        window: &WindowId,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Result<(DomainKey, DomainKey), CommandApplied> {
        let Some(src) = self.staged_homing(window) else {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        };
        if held_scopes.contains(&src) {
            return Err(refused(KIND_SCOPE_HELD, MSG_SCOPE_HELD));
        }
        if !self
            .staged
            .domains
            .iter()
            .any(|domain| domain.readable && domain.key == src)
        {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        }
        let Some(fact) = self.staged.facts.get(window) else {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        };
        if self
            .retained
            .iter()
            .any(|transfer| matches!(&transfer.target.kind, TransferKind::Window(w) if w == window))
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
        if derived.own_excluded
            || derived.excluded
            || derived.hidden
            || derived.overlay_fullscreen
            || derived.overlay_maximized
        {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        }
        let Some(dest) = self.sticky_dest(&src, window) else {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        };
        if held_scopes.contains(&dest) {
            return Err(refused(KIND_SCOPE_HELD, MSG_SCOPE_HELD));
        }
        if !self
            .staged
            .domains
            .iter()
            .any(|domain| domain.readable && domain.key == dest)
        {
            return Err(refused(KIND_OPERATION, MSG_OPERATION));
        }
        Ok((src, dest))
    }

    /// `SetSticky`/`ToggleSticky`: `Some(value)` sets, `None` toggles.
    pub(crate) fn execute_sticky_command(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        command_seq: u64,
        window: &WindowId,
        desired_some: Option<bool>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        if self.sticky_route() == StickyRoute::Native && self.sticky_current(window).is_none() {
            if self.native_unknown_safety_refusal(window).is_some() {
                let (src, dest) = match self.sticky_pregates(window, held_scopes) {
                    Ok(found) => found,
                    Err(rejected) => return rejected,
                };
                let _ = (src, dest);
            } else {
                self.command_result = Some(CommandResult::Rejected {
                    kind: CommandRejectKind::StateUnknown,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
        }
        let (src, dest) = match self.sticky_pregates(window, held_scopes) {
            Ok(found) => found,
            Err(rejected) => return rejected,
        };
        match self.sticky_route() {
            StickyRoute::Native => {}
            StickyRoute::Unsupported => {
                self.command_result = Some(CommandResult::Rejected {
                    kind: CommandRejectKind::StateUnsupported,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
            StickyRoute::Composed => {}
        }
        if self.sticky_route() == StickyRoute::Native {
            return self.execute_native_sticky_command(
                issue,
                command_seq,
                window,
                desired_some,
                &src,
                &dest,
                held_scopes,
            );
        }
        let desired = match desired_some {
            Some(value) => value,
            None => !self.sticky_current(window).unwrap_or(false),
        };
        self.run_sticky_transition(
            issue,
            command_seq,
            window,
            &src,
            &dest,
            desired,
            held_scopes,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_native_sticky_command(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        command_seq: u64,
        window: &WindowId,
        desired_some: Option<bool>,
        src: &DomainKey,
        dest: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let desired = match desired_some {
            Some(value) => value,
            None => match self.sticky_current(window) {
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
        match self.sticky_current(window) {
            None => {
                self.command_result = Some(CommandResult::Rejected {
                    kind: CommandRejectKind::StateUnknown,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
            Some(current)
                if current == desired && !self.native_sticky_needs_reauthorize(window) =>
            {
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
            _ => {}
        }
        if desired {
            self.run_native_sticky_on(issue, command_seq, window, src, dest, held_scopes)
        } else {
            self.run_native_sticky_off(command_seq, window, src, dest, false)
        }
    }

    fn native_sticky_needs_reauthorize(&self, window: &WindowId) -> bool {
        match self
            .retained_states
            .get(&(window.clone(), StateProperty::Sticky))
        {
            Some(binding) => {
                binding.spent
                    && !matches!(
                        binding.disposition,
                        AttemptOutcome::Applied | AttemptOutcome::Unattempted
                    )
            }
            None => false,
        }
    }

    fn native_prior_exact(&self, window: &WindowId) -> (Option<IntentData>, bool) {
        let prior_saved = self.sticky_saved.get(window).copied();
        let mut unknown = false;
        if self.intent_conflicts.contains(window) {
            unknown = true;
        }
        if self.intent_sticky_saturated {
            unknown = true;
        }
        if self.sticky_saved_incomplete {
            unknown = true;
        }
        if prior_saved.is_none()
            && self.sticky_saved.len() >= crate::world_report::MAX_INTENT_RECORDS
        {
            unknown = true;
        }
        (prior_saved, unknown)
    }

    fn native_check_binding_cap(&self, window: &WindowId) -> bool {
        self.retained_states
            .contains_key(&(window.clone(), StateProperty::Sticky))
            || self.retained_states.len() < MAX_RETAINED_STATE_BINDINGS
    }

    #[allow(clippy::too_many_arguments)]
    fn run_native_sticky_on(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        command_seq: u64,
        window: &WindowId,
        src: &DomainKey,
        dest: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        if !self.native_check_binding_cap(window) {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let (prior_saved, prior_unknown) = self.native_prior_exact(window);
        if prior_unknown {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let home = src.clone();
        let dest_key = dest.clone();
        // Pre-mutation snapshot for settlement/rollback: capture before
        // any transition or write-cache mutation.
        let snap_restored = self.restored_sticky.get(window).copied();
        let snap_floats = self.restored_floats.contains(window);
        let snap_float = self.is_exception_window_any(window);
        // Spent-ambiguous retry on the same semantic: the composed
        // transition would no-op with no queued save, yet a new target is
        // minted. Re-emit the canonical save preserving the original
        // sticky prior instead of recomputing the now-float runtime.
        if self.sticky_current(window) == Some(true) && self.native_sticky_needs_reauthorize(window)
        {
            let prior = match snap_restored {
                Some(prior) => prior,
                None => return refused(KIND_OPERATION, MSG_OPERATION),
            };
            let requested_save = IntentData::new(Some(true), Some(prior));
            if !self.sticky_saved.contains_key(window)
                && (self.sticky_saved.len() >= crate::world_report::MAX_INTENT_RECORDS
                    || (self.intent_ledger_used() >= crate::world_report::MAX_INTENT_RECORDS
                        && !self.restored_sticky.contains_key(window)))
            {
                return refused(KIND_OPERATION, MSG_OPERATION);
            }
            self.sticky_saved.insert(window.clone(), requested_save);
            self.sticky_command_save = Some(IntentWrite {
                win: window.clone(),
                data: Some(encode_intent_data(&requested_save)),
            });
            let revision = self.state_revision + 1;
            let meta = NativeStickyMeta {
                command_seq,
                prior_saved,
                prior_restored_sticky: snap_restored,
                prior_restored_floats: snap_floats,
                prior_float: snap_float,
                home,
                dest: dest_key.clone(),
                requested: true,
                requested_save,
                is_toggle_float_clear: false,
                followup_done: false,
            };
            self.retained_states.insert(
                (window.clone(), StateProperty::Sticky),
                crate::world_state::RetainedStateBinding {
                    target: crate::world_state::RetainedStateTarget {
                        window: window.clone(),
                        property: StateProperty::Sticky,
                        value: true,
                        revision,
                    },
                    pending: None,
                    spent: false,
                    disposition: AttemptOutcome::Unattempted,
                    sticky_meta: Some(meta),
                    fullscreen_meta: None,
                    maximize_meta: None,
                    above_meta: None,
                },
            );
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
        }
        let applied =
            self.run_sticky_transition(issue, command_seq, window, src, dest, true, held_scopes);
        match (&applied, &self.command_result) {
            (CommandApplied::NoEffect, None) => {}
            _ => return applied,
        }
        let revision = self.state_revision + 1;
        let key = (window.clone(), StateProperty::Sticky);
        let requested_save = self
            .sticky_saved
            .get(window)
            .copied()
            .unwrap_or(IntentData::new(Some(true), Some(snap_float)));
        let meta = NativeStickyMeta {
            command_seq,
            prior_saved,
            prior_restored_sticky: snap_restored,
            prior_restored_floats: snap_floats,
            prior_float: snap_float,
            home,
            dest: dest_key.clone(),
            requested: true,
            requested_save,
            is_toggle_float_clear: false,
            followup_done: false,
        };
        self.retained_states.insert(
            key.clone(),
            crate::world_state::RetainedStateBinding {
                target: crate::world_state::RetainedStateTarget {
                    window: window.clone(),
                    property: StateProperty::Sticky,
                    value: true,
                    revision,
                },
                pending: None,
                spent: false,
                disposition: AttemptOutcome::Unattempted,
                sticky_meta: Some(meta),
                fullscreen_meta: None,
                maximize_meta: None,
                above_meta: None,
            },
        );
        CommandApplied::NoEffect
    }

    fn run_native_sticky_off(
        &mut self,
        command_seq: u64,
        window: &WindowId,
        src: &DomainKey,
        dest: &DomainKey,
        is_toggle_float_clear: bool,
    ) -> CommandApplied {
        if !self.native_check_binding_cap(window) {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let (prior_saved, prior_unknown) = self.native_prior_exact(window);
        if prior_unknown {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        // Pre-mutation snapshot for settlement/rollback.
        let snap_restored = self.restored_sticky.get(window).copied();
        let snap_floats = self.restored_floats.contains(window);
        let snap_float = self.is_exception_window_any(window);
        let requested_save = if is_toggle_float_clear {
            IntentData::new(Some(false), None)
        } else {
            let prior_bool = match self.sticky_prior_for_off(window) {
                Some(prior) => prior,
                None => return refused(KIND_OPERATION, MSG_OPERATION),
            };
            IntentData::new(Some(prior_bool), None)
        };
        if !self.sticky_saved.contains_key(window)
            && (self.sticky_saved.len() >= crate::world_report::MAX_INTENT_RECORDS
                || (self.intent_ledger_used() >= crate::world_report::MAX_INTENT_RECORDS
                    && !self.restored_sticky.contains_key(window)))
        {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let home = src.clone();
        let dest_key = dest.clone();
        let revision = self.state_revision + 1;
        // Adopted native sticky (raw true with no restored provenance) would
        // otherwise lose its sticky-float hold if raw flips false before the
        // correlated clear proof arrives: the ordinary sweep would tile it
        // prematurely. Retain semantic provenance (adopted prior true) while
        // the binding awaits proof. Joint-ledger bound plus runtime preimage
        // (captured above) gate the insert; raw facts are never rewritten.
        if snap_restored.is_none()
            && self.staged.facts.get(window).and_then(|fact| fact.sticky) == Some(true)
        {
            if self.intent_ledger_used() >= crate::world_report::MAX_INTENT_RECORDS {
                return refused(KIND_OPERATION, MSG_OPERATION);
            }
            self.restored_sticky.insert(window.clone(), true);
        }
        self.sticky_saved.insert(window.clone(), requested_save);
        self.sticky_command_save = Some(IntentWrite {
            win: window.clone(),
            data: Some(encode_intent_data(&requested_save)),
        });
        let meta = NativeStickyMeta {
            command_seq,
            prior_saved,
            prior_restored_sticky: snap_restored,
            prior_restored_floats: snap_floats,
            prior_float: snap_float,
            home,
            dest: dest_key.clone(),
            requested: false,
            requested_save,
            is_toggle_float_clear,
            followup_done: false,
        };
        self.retained_states.insert(
            (window.clone(), StateProperty::Sticky),
            crate::world_state::RetainedStateBinding {
                target: crate::world_state::RetainedStateTarget {
                    window: window.clone(),
                    property: StateProperty::Sticky,
                    value: false,
                    revision,
                },
                pending: None,
                spent: false,
                disposition: AttemptOutcome::Unattempted,
                sticky_meta: Some(meta),
                fullscreen_meta: None,
                maximize_meta: None,
                above_meta: None,
            },
        );
        self.note_command(command_seq);
        CommandApplied::NoEffect
    }

    /// Native `ToggleFloat` against a sticky subject: clears sticky but holds
    /// sticky-float runtime (no premature tile/home transition) until
    /// correlated clear proof. Queues the tile-base requested record before
    /// the actual native write. Unsupported route keeps StateUnsupported.
    pub(crate) fn execute_sticky_toggle_float(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        command_seq: u64,
        window: &WindowId,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        if self.sticky_route() == StickyRoute::Unsupported {
            self.command_result = Some(CommandResult::Rejected {
                kind: CommandRejectKind::StateUnsupported,
            });
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
        }
        if self.sticky_route() == StickyRoute::Native {
            if self.sticky_current(window) != Some(true) {
                match self.sticky_current(window) {
                    None => {
                        self.command_result = Some(CommandResult::Rejected {
                            kind: CommandRejectKind::StateUnknown,
                        });
                        self.note_command(command_seq);
                        return CommandApplied::NoEffect;
                    }
                    _ => return refused(KIND_OPERATION, MSG_OPERATION),
                }
            }
            let (src, dest) = match self.sticky_pregates(window, held_scopes) {
                Ok(found) => found,
                Err(rejected) => return rejected,
            };
            return self.run_native_sticky_off(command_seq, window, &src, &dest, true);
        }
        if !self.restored_sticky.contains_key(window)
            && self.usable_pending_sticky(window).is_none()
        {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let (src, dest) = match self.sticky_pregates(window, held_scopes) {
            Ok(found) => found,
            Err(rejected) => return rejected,
        };
        self.run_sticky_toggle_float(issue, command_seq, window, &src, &dest, held_scopes)
    }

    /// Composed sticky clear-and-tile on the current workspace. Classification
    /// convergence cannot tile a window whose facts still prove a float
    /// origin (transient and friends keep classifying float after sticky
    /// clears), so this runs the established explicit `ToggleFloat` event
    /// on a clone instead of inventing a new tile path: same capacity
    /// policy, same refusal vocabulary. A genuine no-space re-float refuses
    /// with no false tile save. The tile lands at the current-workspace
    /// destination on the owning output, never in place at the source; the
    /// source scope still sweeps for survivor reflow and its minimized slot
    /// is forgotten on a cross-workspace move. A floating destination
    /// cannot host a tile: the clear lands floated there with its base tile
    /// intent saved and no session forced. Adopting the clone commits
    /// provenance, membership, and the staged save together.
    #[allow(clippy::too_many_arguments)]
    fn run_sticky_toggle_float(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        command_seq: u64,
        window: &WindowId,
        src: &DomainKey,
        dest: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let mut staged = self.clone();
        let dest_floating = staged.floating_scopes.contains(dest);
        staged.restored_sticky.remove(window);
        staged.pending_intent.remove(window);
        staged.intent_conflicts.remove(window);
        staged.intent_sticky_blocks.remove(window);
        staged.sticky_visible_rev.remove(window);
        // The clear intends a tile: a floating destination lands floated
        // for workspace reasons only, with no explicit-float mark, so a
        // later retiling restores the saved tile base.
        staged.restored_floats.remove(window);
        if staged.intent_ledger_used() > crate::world_report::MAX_INTENT_RECORDS {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let save = IntentData::new(Some(false), None);
        if staged.sticky_saved.contains_key(window)
            || staged.sticky_saved.len() < crate::world_report::MAX_INTENT_RECORDS
        {
            staged.sticky_saved.insert(window.clone(), save);
        } else {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        staged.sticky_command_save = Some(IntentWrite {
            win: window.clone(),
            data: Some(encode_intent_data(&save)),
        });
        let rect = staged.staged.facts.get(window).and_then(|fact| fact.rect);
        match staged.members.get_mut(window) {
            Some(state) => {
                state.home = dest.clone();
                state.floating = dest_floating;
                state.minimized = false;
                state.geometry = rect;
            }
            None => {
                staged.members.insert(
                    window.clone(),
                    MemberState {
                        home: dest.clone(),
                        floating: dest_floating,
                        minimized: false,
                        geometry: rect,
                    },
                );
            }
        }
        staged.restage_classification();
        staged.build_staged_domains();
        if staged.staged_homing(window).as_ref() != Some(dest) {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        if dest_floating {
            let owner = issue.owner.clone();
            let correlation = issue.correlation.clone();
            let mut held = held_scopes.clone();
            let mut affected = vec![src.clone()];
            if dest != src {
                affected.push(dest.clone());
            }
            let domains = staged.staged.domains.clone();
            for key in &affected {
                if staged.floating_scopes.contains(key) {
                    continue;
                }
                let Some(domain) = domains.iter().find(|domain| domain.key == *key) else {
                    return refused(KIND_OPERATION, MSG_OPERATION);
                };
                staged.sweep_domain(domain, &owner, &correlation, &mut held);
            }
            if affected.iter().any(|key| held.contains(key)) {
                return refused(KIND_OPERATION, MSG_OPERATION);
            }
            if dest != src && !staged.floating_scopes.contains(src) {
                staged.engine.forget_minimized_slot(src, window);
            }
            staged.command_result = Some(CommandResult::Float {
                window: window.clone(),
                rect: None,
                no_space: false,
            });
            staged.note_command(command_seq);
            *self = staged;
            return CommandApplied::NoEffect;
        }
        // The explicit unfloat binds observed flags to the retained
        // exception (both floating), while convergence tiles a carrier
        // that already reads tiled: pick the established path the
        // restaged carrier supports. A transient-style origin still
        // classifies float after the clear, so it rides the explicit
        // event; an ordinary carrier converges through the sweep.
        let floated_carrier = staged
            .staged
            .domains
            .iter()
            .find(|domain| domain.key == *dest)
            .map(|domain| {
                domain
                    .windows
                    .iter()
                    .any(|entry| entry.window == *window && entry.floating)
            })
            .unwrap_or(false);
        if !floated_carrier {
            // Ordinary carrier: converge the current-workspace re-tile
            // through sweeps over both affected scopes.
            let owner = issue.owner.clone();
            let correlation = issue.correlation.clone();
            let domains = staged.staged.domains.clone();
            let mut affected = vec![src.clone()];
            if dest != src {
                affected.push(dest.clone());
            }
            let mut held = held_scopes.clone();
            for key in &affected {
                if staged.floating_scopes.contains(key) {
                    continue;
                }
                let Some(domain) = domains.iter().find(|domain| domain.key == *key) else {
                    return refused(KIND_OPERATION, MSG_OPERATION);
                };
                staged.sweep_domain(domain, &owner, &correlation, &mut held);
            }
            if affected.iter().any(|key| held.contains(key)) {
                return refused(KIND_OPERATION, MSG_OPERATION);
            }
            if dest != src && !staged.floating_scopes.contains(src) {
                staged.engine.forget_minimized_slot(src, window);
            }
            let tiled = staged.engine.session(dest).is_some_and(|session| {
                session
                    .snapshot()
                    .windows
                    .iter()
                    .any(|link| link.window == *window)
            });
            if !tiled {
                return refused(KIND_OPERATION, MSG_OPERATION);
            }
            staged.engine.commit_fixed_tile_override(window);
            staged.command_result = Some(CommandResult::Float {
                window: window.clone(),
                rect: None,
                no_space: false,
            });
            staged.note_command(command_seq);
            *self = staged;
            return CommandApplied::NoEffect;
        }
        let command = CoreCommand::ToggleFloat {
            window: window.0.clone(),
            float_rect: None,
        };
        let domain_report = match staged
            .staged
            .domains
            .iter()
            .find(|domain| domain.readable && domain.key == *dest)
        {
            Some(found) => found.clone(),
            None => return refused(KIND_OPERATION, MSG_OPERATION),
        };
        if held_scopes.contains(dest) {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        // Sweep the tiled source for survivor reflow before the explicit
        // tile lands at the destination; a held source refuses atomically.
        let owner = issue.owner.clone();
        let correlation = issue.correlation.clone();
        if dest != src && !staged.floating_scopes.contains(src) {
            let domains = staged.staged.domains.clone();
            if let Some(source_domain) = domains.iter().find(|domain| domain.key == *src) {
                let mut held = held_scopes.clone();
                staged.sweep_domain(source_domain, &owner, &correlation, &mut held);
                if held.contains(src) || held.contains(dest) {
                    return refused(KIND_OPERATION, MSG_OPERATION);
                }
            }
        }
        let accepted = staged
            .engine
            .session(dest)
            .map(|session| (session.accepted_revision(), session.accepted_fingerprint()));
        let (revision, fingerprint) = accepted.unwrap_or((0, 0));
        let staged_output = OutputDomain {
            id: dest.output.clone(),
            workspace: dest.workspace.clone(),
            bounds: domain_report.bounds,
            gap: domain_report.gap,
            adjacent: BTreeMap::new(),
        };
        let Some(inset) =
            crate::world_engine::inset_world_domain(&staged_output, domain_report.outer_gap)
        else {
            return refused(KIND_SCOPE_HELD, MSG_SCOPE_HELD);
        };
        let event = CoreEvent {
            owner,
            generation: staged.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset),
            domain_key: dest.clone(),
            outer_gap: domain_report.outer_gap,
            focused_window: domain_report.focused.clone(),
            windows: staged.shielded_windows(dest, &domain_report.windows),
            closed_windows: staged.closed_for_domain(&domain_report),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command: command.clone(),
        };
        let applied = staged.finish_operation_event(command_seq, &event);
        match (&applied, &staged.command_result) {
            (
                CommandApplied::NoEffect,
                Some(CommandResult::Float {
                    rect: None,
                    no_space: false,
                    ..
                }),
            ) => {
                staged.engine.commit_fixed_tile_override(window);
                if dest != src && !staged.floating_scopes.contains(src) {
                    staged.engine.forget_minimized_slot(src, window);
                }
                *self = staged;
                CommandApplied::NoEffect
            }
            (CommandApplied::Rejected { .. }, _) => applied,
            _ => refused(KIND_OPERATION, MSG_OPERATION),
        }
    }

    /// Shared composed transition. Same-state consumes without staging a
    /// save. Anything else runs on a clone and adopts only after restage,
    /// affected sweeps, and convergence checks all succeed; dropping the
    /// clone on refusal leaks no mutation.
    #[allow(clippy::too_many_arguments)]
    fn run_sticky_transition(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        command_seq: u64,
        window: &WindowId,
        src: &DomainKey,
        dest: &DomainKey,
        desired: bool,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        if self.sticky_current(window) == Some(desired) {
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
        }
        // Off needs the exact prior; on records whether the source was
        // already floated (tiled prior `false`, any float prior `true`).
        let off_prior = if desired {
            None
        } else {
            match self.sticky_prior_for_off(window) {
                Some(prior) => Some(prior),
                None => return refused(KIND_OPERATION, MSG_OPERATION),
            }
        };
        let was_float = self.is_exception_window_any(window);
        let mut staged = self.clone();
        // A floating destination cannot host a tile: an off-to-tile
        // intent lands floated there with its base tile intent saved, and
        // no session is forced until a retiling. Every other path lands
        // exactly as desired.
        let dest_floating = staged.floating_scopes.contains(dest);
        let landing_float =
            desired || off_prior == Some(true) || (off_prior.is_some() && dest_floating);
        let save = if desired {
            staged.restored_sticky.insert(window.clone(), was_float);
            staged.pending_intent.remove(window);
            staged.intent_conflicts.remove(window);
            staged.intent_sticky_blocks.remove(window);
            staged.restored_floats.remove(window);
            staged
                .sticky_visible_rev
                .insert(window.clone(), staged.state_revision + 1);
            IntentData::new(Some(true), Some(was_float))
        } else {
            let prior = off_prior.expect("off always resolves a prior");
            staged.restored_sticky.remove(window);
            staged.pending_intent.remove(window);
            staged.intent_conflicts.remove(window);
            staged.intent_sticky_blocks.remove(window);
            staged.sticky_visible_rev.remove(window);
            // Genuine float intent keeps explicit provenance; a
            // workspace-forced float over a tile base carries no
            // explicit-float mark, so a later retiling restores the
            // saved tile instead of pinning the float.
            if prior {
                staged.restored_floats.insert(window.clone());
            } else {
                staged.restored_floats.remove(window);
            }
            IntentData::new(Some(prior), None)
        };
        if staged.intent_ledger_used() > crate::world_report::MAX_INTENT_RECORDS {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        if staged.sticky_saved.contains_key(window)
            || staged.sticky_saved.len() < crate::world_report::MAX_INTENT_RECORDS
        {
            staged.sticky_saved.insert(window.clone(), save);
        } else {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        staged.sticky_command_save = Some(IntentWrite {
            win: window.clone(),
            data: Some(encode_intent_data(&save)),
        });
        let rect = staged.staged.facts.get(window).and_then(|fact| fact.rect);
        match staged.members.get_mut(window) {
            Some(state) => {
                state.home = dest.clone();
                state.floating = landing_float;
                state.minimized = false;
                state.geometry = rect;
            }
            None => {
                staged.members.insert(
                    window.clone(),
                    MemberState {
                        home: dest.clone(),
                        floating: landing_float,
                        minimized: false,
                        geometry: rect,
                    },
                );
            }
        }
        staged.restage_classification();
        staged.build_staged_domains();
        // Exact single homing: the window must stage at the destination,
        // never under two homings in one sweep.
        if staged.staged_homing(window).as_ref() != Some(dest) {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        let owner = issue.owner.clone();
        let correlation = issue.correlation.clone();
        let mut held = held_scopes.clone();
        let mut affected = vec![src.clone()];
        if dest != src {
            affected.push(dest.clone());
        }
        let domains = staged.staged.domains.clone();
        for key in &affected {
            if staged.floating_scopes.contains(key) {
                continue;
            }
            let Some(domain) = domains.iter().find(|domain| domain.key == *key) else {
                return refused(KIND_OPERATION, MSG_OPERATION);
            };
            staged.sweep_domain(domain, &owner, &correlation, &mut held);
        }
        if affected.iter().any(|key| held.contains(key)) {
            return refused(KIND_OPERATION, MSG_OPERATION);
        }
        if dest != src && !staged.floating_scopes.contains(src) {
            // A moved tiled leaf minimizes at the source; forget the slot
            // so no stale minimized lifetime survives the move. A moved
            // exception drops cleanly and the forget is a no-op.
            staged.engine.forget_minimized_slot(src, window);
        }
        // Genuine capacity fallback never commits a false tile save: a
        // tile outcome that did not land tiled refuses with no mutation
        // instead of recording tile intent over a still-float window. A
        // surviving float-classifying raw origin (transient and friends)
        // still reads floated after the clear, so it rides the established
        // explicit `ToggleFloat` event at the destination where an
        // ordinary carrier converges through the sweep above.
        let converged = if landing_float {
            staged.floating_scopes.contains(dest)
                || staged
                    .engine
                    .session(dest)
                    .is_some_and(|session| session.is_exception(window))
        } else {
            staged.engine.session(dest).is_some_and(|session| {
                session
                    .snapshot()
                    .windows
                    .iter()
                    .any(|link| link.window == *window)
            })
        };
        if !converged {
            if landing_float {
                return refused(KIND_OPERATION, MSG_OPERATION);
            }
            let floated_carrier = staged
                .staged
                .domains
                .iter()
                .find(|domain| domain.key == *dest)
                .map(|domain| {
                    domain
                        .windows
                        .iter()
                        .any(|entry| entry.window == *window && entry.floating)
                })
                .unwrap_or(false);
            if !floated_carrier {
                return refused(KIND_OPERATION, MSG_OPERATION);
            }
            let domain_report = match staged
                .staged
                .domains
                .iter()
                .find(|domain| domain.readable && domain.key == *dest)
            {
                Some(found) => found.clone(),
                None => return refused(KIND_OPERATION, MSG_OPERATION),
            };
            if held_scopes.contains(dest) {
                return refused(KIND_OPERATION, MSG_OPERATION);
            }
            let accepted = staged
                .engine
                .session(dest)
                .map(|session| (session.accepted_revision(), session.accepted_fingerprint()));
            let (revision, fingerprint) = accepted.unwrap_or((0, 0));
            let staged_output = OutputDomain {
                id: dest.output.clone(),
                workspace: dest.workspace.clone(),
                bounds: domain_report.bounds,
                gap: domain_report.gap,
                adjacent: BTreeMap::new(),
            };
            let Some(inset) =
                crate::world_engine::inset_world_domain(&staged_output, domain_report.outer_gap)
            else {
                return refused(KIND_SCOPE_HELD, MSG_SCOPE_HELD);
            };
            let command = CoreCommand::ToggleFloat {
                window: window.0.clone(),
                float_rect: None,
            };
            let event = CoreEvent {
                owner,
                generation: staged.core_generation.clone(),
                correlation,
                revision,
                fingerprint,
                domain: Engine::canonical_component_domain(&inset),
                domain_key: dest.clone(),
                outer_gap: domain_report.outer_gap,
                focused_window: domain_report.focused.clone(),
                windows: staged.shielded_windows(dest, &domain_report.windows),
                closed_windows: staged.closed_for_domain(&domain_report),
                directional: None,
                directional_target_outer_gap: None,
                target_domain: None,
                target_windows: Vec::new(),
                command: command.clone(),
            };
            let applied = staged.finish_operation_event(command_seq, &event);
            match (&applied, &staged.command_result) {
                (
                    CommandApplied::NoEffect,
                    Some(CommandResult::Float {
                        rect: None,
                        no_space: false,
                        ..
                    }),
                ) => {
                    staged.engine.commit_fixed_tile_override(window);
                    staged.command_result = None;
                    staged.note_command(command_seq);
                    *self = staged;
                    return CommandApplied::NoEffect;
                }
                (CommandApplied::Rejected { .. }, _) => return applied,
                _ => return refused(KIND_OPERATION, MSG_OPERATION),
            }
        }
        if !landing_float {
            staged.engine.commit_fixed_tile_override(window);
        }
        staged.note_command(command_seq);
        *self = staged;
        CommandApplied::NoEffect
    }

    /// Proven-refusal rollback for one exact-tuple `RefusedProven` native
    /// sticky binding. Restores the exact pre-command runtime from the
    /// minted metadata on a clone: sticky provenance, restored-float
    /// membership, home, and the prior float base (ON refusal returns the
    /// original tile/float, OFF/`ToggleFloat` refusal keeps prior sticky),
    /// then converges through fresh restage plus source/dest sweep, never
    /// a snapshot tree or raw-fact rewrite. Restores the exact prior saved
    /// record (cap-safe: update in place, insert with room, else latch
    /// incomplete instead of a silent drop) and queues exactly one
    /// latest-wins rollback write (prior canonical bytes, or null-clear
    /// when there was none). Knowledge changes only on commit: any
    /// unhealthy gate (unreadable home/dest, held scopes, minimized,
    /// excluded, in-flight, overlay, unknown window, restage mismatch,
    /// held sweep, missing owner/correlation) retains proof untouched for
    /// a healthy record-free retry. Never stages a command save and never
    /// touches the current command result or focus.
    fn run_native_sticky_rollback_for(
        &mut self,
        key: &(WindowId, crate::world_report::StateProperty),
        meta: &NativeStickyMeta,
        issue: &crate::world_engine::EventIssue,
        held_scopes: &BTreeSet<DomainKey>,
    ) {
        if !self.staged.observation.inventory_complete || self.staged.gesture_barrier {
            return;
        }
        let window = key.0.clone();
        let Some(fact) = self.staged.facts.get(&window) else {
            return;
        };
        if fact.minimized == Some(true) {
            return;
        }
        if self.is_held_window(&window) || self.is_excluded_window(&window) {
            return;
        }
        if self.retained.iter().any(
            |transfer| matches!(&transfer.target.kind, TransferKind::Window(w) if *w == window),
        ) {
            return;
        }
        let derived = self.derived_for(&window);
        if derived.own_excluded
            || derived.excluded
            || derived.hidden
            || derived.overlay_fullscreen
            || derived.overlay_maximized
        {
            return;
        }
        let home_readable = self
            .staged
            .domains
            .iter()
            .any(|domain| domain.key == meta.home && domain.readable);
        let dest_readable = self
            .staged
            .domains
            .iter()
            .any(|domain| domain.key == meta.dest && domain.readable);
        if !home_readable || !dest_readable {
            return;
        }
        if held_scopes.contains(&meta.home) || held_scopes.contains(&meta.dest) {
            return;
        }
        if !(self.engine_knows_window_any(&window) || self.members.contains_key(&window)) {
            return;
        }
        let mut staged = self.clone();
        match meta.prior_restored_sticky {
            Some(prior) => {
                staged.restored_sticky.insert(window.clone(), prior);
            }
            None => {
                staged.restored_sticky.remove(&window);
            }
        }
        if meta.prior_restored_floats {
            staged.restored_floats.insert(window.clone());
        } else {
            staged.restored_floats.remove(&window);
        }
        // A fresh ON inserted the stable visibility revision; a reauth ON
        // (already sticky) and every OFF/clear never touched it.
        if meta.requested && meta.prior_restored_sticky.is_none() {
            staged.sticky_visible_rev.remove(&window);
        }
        let home = meta.home.clone();
        let floating = if meta.requested {
            meta.prior_float
        } else {
            true
        };
        let rect = staged.staged.facts.get(&window).and_then(|fact| fact.rect);
        match staged.members.get_mut(&window) {
            Some(state) => {
                state.home = home.clone();
                state.floating = floating;
                state.minimized = false;
                state.geometry = rect;
            }
            None => {
                staged.members.insert(
                    window.clone(),
                    MemberState {
                        home: home.clone(),
                        floating,
                        minimized: false,
                        geometry: rect,
                    },
                );
            }
        }
        let rollback = match meta.prior_saved {
            Some(prior) => {
                if !staged.sticky_saved.contains_key(&window)
                    && staged.sticky_saved.len() >= crate::world_report::MAX_INTENT_RECORDS
                {
                    // Cap-safe: latch incomplete instead of silently
                    // dropping the exact prior (mirrors first-admission).
                    staged.sticky_saved_incomplete = true;
                } else {
                    staged.sticky_saved.insert(window.clone(), prior);
                }
                IntentWrite {
                    win: window.clone(),
                    data: Some(encode_intent_data(&prior)),
                }
            }
            None => {
                staged.sticky_saved.remove(&window);
                IntentWrite {
                    win: window.clone(),
                    data: None,
                }
            }
        };
        staged.restage_classification();
        staged.build_staged_domains();
        if staged.staged_homing(&window).as_ref() != Some(&home) {
            return;
        }
        let owner = issue.owner.clone();
        let correlation = issue.correlation.clone();
        let mut held = held_scopes.clone();
        let mut affected = vec![home.clone()];
        if meta.dest != home {
            affected.push(meta.dest.clone());
        }
        let domains = staged.staged.domains.clone();
        for key in &affected {
            if staged.floating_scopes.contains(key) {
                continue;
            }
            let Some(domain) = domains.iter().find(|domain| domain.key == *key) else {
                return;
            };
            staged.sweep_domain(domain, &owner, &correlation, &mut held);
        }
        if affected.iter().any(|key| held.contains(key)) {
            return;
        }
        // An ON moved home-to-dest; the rollback moves back, so the stale
        // minimized slot at the dest is forgotten exactly like the
        // producer forgot the source slot on the way out.
        if meta.requested && meta.dest != home {
            staged.engine.forget_minimized_slot(&meta.dest, &window);
        }
        // Joint-ledger cap after restoring the exact prior runtime: the
        // restored sticky provenance shares the single MAX_INTENT_RECORDS
        // bound, so an over-cap restore holds proof for a healthy retry
        // instead of adopting a ledger breach or silently dropping it.
        if staged.intent_ledger_used() > crate::world_report::MAX_INTENT_RECORDS {
            return;
        }
        // Latest-wins per window: one proof, one write. The queue is
        // explicitly bounded by MAX_INTENT_WRITES; a full queue holds proof
        // for retry (same-key replacement stays allowed) instead of
        // silently dropping the write while marking the followup done.
        if !staged.sticky_rollback_saves.contains_key(&window)
            && staged.sticky_rollback_saves.len() >= crate::world_report::MAX_INTENT_WRITES
        {
            return;
        }
        staged
            .sticky_rollback_saves
            .insert(window.clone(), rollback);
        if let Some(binding) = staged.retained_states.get_mut(key)
            && let Some(meta_mut) = binding.sticky_meta.as_mut()
        {
            meta_mut.followup_done = true;
        }
        *self = staged;
    }

    /// One-time native dependent followup after generic `settle_states`.
    /// Runs before the ordinary sweep with fresh `floating_scopes` and the
    /// staged held scopes. Only exact-tuple `Applied`/`RefusedProven`
    /// bindings with retained proof run; ambiguous
    /// `PossiblyExecuted`/`Unattempted` never unlock dependent membership,
    /// keep the requested record, and never retry: later native
    /// observation reconciles per existing fact-authority rules.
    /// `Applied` ON marks done with no extra save and no new target,
    /// preserving prior. `Applied` OFF/`ToggleFloat` performs the safe
    /// readable clear on a clone via the composed helpers (OFF honors the
    /// requested prior float/tile, `ToggleFloat` always lands the tile
    /// base), then suppresses the helper's staged save/result so the
    /// producer's prewrite stays the single save. `RefusedProven` rolls
    /// back the exact pre-command runtime (sticky/restored-float/home and
    /// the prior float base) through fresh restage plus source/dest sweep
    /// convergence, never a snapshot tree or raw-fact rewrite, restores
    /// the exact prior saved record (or null-clears when there was none),
    /// and queues exactly one rollback write; native-ON refusal returns
    /// the original tile/float, native-OFF refusal keeps prior sticky.
    /// Unreadable home/dest, minimized, excluded, incomplete inventory, or
    /// gesture hold retains proof with no native write for a healthy
    /// record-free retry. Original typed `command_seq`/revision stay for
    /// the publisher; spent bindings never reauthorize as `Controlled`.
    /// Newer same-window replacements own the new intent; no global
    /// highwater gate blocks unrelated newer commands. Non-state ordinary
    /// same-window overrides supersede a pending rollback only where the
    /// fresh save actually wins at publisher merge.
    pub(crate) fn run_native_sticky_followups(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        held_scopes: &BTreeSet<DomainKey>,
    ) {
        if self.sticky_route() != StickyRoute::Native {
            return;
        }
        let keys: Vec<(WindowId, crate::world_report::StateProperty)> = self
            .retained_states
            .keys()
            .filter(|(_, property)| *property == crate::world_report::StateProperty::Sticky)
            .cloned()
            .collect();
        for key in keys {
            let (meta, disposition, spent) = match self.retained_states.get(&key) {
                Some(binding) => match binding.sticky_meta.as_ref() {
                    Some(meta) => (meta.clone(), binding.disposition, binding.spent),
                    None => continue,
                },
                None => continue,
            };
            if meta.followup_done {
                continue;
            }
            if !spent {
                continue;
            }
            if disposition == AttemptOutcome::RefusedProven {
                self.run_native_sticky_rollback_for(&key, &meta, issue, held_scopes);
                continue;
            }
            if disposition != AttemptOutcome::Applied {
                // Ambiguous/unproven: keep the requested record and let
                // later native observation reconcile per existing
                // fact-authority rules; never retry here.
                continue;
            }
            if meta.requested {
                if let Some(binding) = self.retained_states.get_mut(&key)
                    && let Some(meta_mut) = binding.sticky_meta.as_mut()
                {
                    meta_mut.followup_done = true;
                }
                continue;
            }
            if !self.staged.observation.inventory_complete || self.staged.gesture_barrier {
                continue;
            }
            let window = key.0.clone();
            let Some(fact) = self.staged.facts.get(&window) else {
                continue;
            };
            if fact.minimized == Some(true) {
                continue;
            }
            if self.is_held_window(&window) || self.is_excluded_window(&window) {
                continue;
            }
            if self.retained.iter().any(
                |transfer| matches!(&transfer.target.kind, TransferKind::Window(w) if *w == window),
            ) {
                continue;
            }
            let derived = self.derived_for(&window);
            if derived.own_excluded
                || derived.excluded
                || derived.hidden
                || derived.overlay_fullscreen
                || derived.overlay_maximized
            {
                continue;
            }
            let home_readable = self
                .staged
                .domains
                .iter()
                .any(|domain| domain.key == meta.home && domain.readable);
            let dest_readable = self
                .staged
                .domains
                .iter()
                .any(|domain| domain.key == meta.dest && domain.readable);
            if !home_readable || !dest_readable {
                continue;
            }
            if held_scopes.contains(&meta.home) || held_scopes.contains(&meta.dest) {
                continue;
            }
            if !(self.engine_knows_window_any(&window) || self.members.contains_key(&window)) {
                continue;
            }
            // Stale-proof guard: an Applied off/clear proof only unlocks
            // the dependent transition while the current raw fact still
            // reads non-sticky. The generic settlement ignores overlays
            // (readability only gates minimized/held/domain), so a proof
            // taken under a fullscreen/maximized overlay turns Applied
            // while this followup waits; a later external restick
            // (`Some(true)`) or unknown (`None`) raw must retain proof
            // with no landing, no save, and no completion. A later
            // record-free fresh `Some(false)` reuses the retained proof
            // and recovers. The original attempt stays spent with
            // no new target; only ON marks done without this recheck.
            if fact.sticky != Some(false) {
                continue;
            }
            let mut staged = self.clone();
            let pre_cmd_save = staged.sticky_command_save.clone();
            let pre_result = staged.command_result.clone();
            let applied = if meta.is_toggle_float_clear {
                staged.run_sticky_toggle_float(
                    issue,
                    meta.command_seq,
                    &window,
                    &meta.home,
                    &meta.dest,
                    held_scopes,
                )
            } else {
                staged.run_sticky_transition(
                    issue,
                    meta.command_seq,
                    &window,
                    &meta.home,
                    &meta.dest,
                    false,
                    held_scopes,
                )
            };
            match applied {
                CommandApplied::NoEffect => {}
                CommandApplied::Rejected { .. } | CommandApplied::Effect(_) => continue,
            }
            staged.sticky_command_save = pre_cmd_save;
            staged.command_result = pre_result;
            if let Some(binding) = staged.retained_states.get_mut(&key)
                && let Some(meta_mut) = binding.sticky_meta.as_mut()
            {
                meta_mut.followup_done = true;
            }
            *self = staged;
        }
    }

    /// Central composed sticky visibility decoration over the assembled
    /// publish rows. Only composed semantic sticky (committed
    /// `restored_sticky` provenance with a retained `sticky_visible_rev`)
    /// is touched, with `VisibilityTarget { shown: true, revision:
    /// retained, fallback: false }`; every other row keeps its assembled
    /// visibility lane untouched. The decorated row is the owning-output
    /// row only, across any managed workspace on that output; window home
    /// never moves here. Visibility authority is independent of the
    /// frame/member lanes: `Controlled` only for a fresh known safe
    /// window, otherwise the stable target is retained with an existing
    /// `HoldReason` (or `Untouched` when excluded). No new holds, no
    /// frame writes, no state writes.
    pub(crate) fn decorate_sticky_visibility(
        &self,
        held_scopes: &BTreeSet<DomainKey>,
        windows: &mut [WindowDesire],
    ) {
        if self.sticky_route() != StickyRoute::Composed {
            return;
        }
        let caps_ok = self
            .capabilities()
            .is_some_and(|caps| caps.window_suppression && !caps.native_sticky);
        for desire in windows.iter_mut() {
            let window = desire.lifetime.clone();
            if !self.restored_sticky.contains_key(&window) {
                continue;
            }
            let Some(rev) = self.sticky_visible_rev.get(&window).copied() else {
                continue;
            };
            // Native sticky facts never take a composed target: raw truth
            // stays authoritative and no composed desire is fabricated.
            if self.staged.facts.get(&window).and_then(|fact| fact.sticky) == Some(true) {
                continue;
            }
            let Some(home) = self.staged_homing(&window) else {
                continue;
            };
            if desire.output != home.output {
                continue;
            }
            let target = VisibilityTarget {
                shown: true,
                revision: rev,
                fallback: false,
            };
            let derived = self.derived_for(&window);
            let excluded_lane =
                self.is_excluded_window(&window) || derived.excluded || derived.own_excluded;
            let domain_readable = self
                .staged
                .domains
                .iter()
                .any(|domain| domain.key == home && domain.readable);
            let mut controlled = caps_ok
                && self.staged.observation.inventory_complete
                && !self.staged.gesture_barrier
                && !held_scopes.contains(&home)
                && !self.is_held_window(&window)
                && !excluded_lane
                && domain_readable
                && (self.engine_knows_window_any(&window) || self.members.contains_key(&window));
            match self.staged.facts.get(&window) {
                Some(fact) => {
                    if fact.suppression_available != Some(true) {
                        controlled = false;
                    }
                    if fact.visibility.visible.is_none()
                        || fact.visibility.suppression_owned.is_none()
                    {
                        controlled = false;
                    }
                    let owned_supported = self.capabilities().is_none_or(|caps| {
                        !caps
                            .unavailable_facts
                            .contains(&crate::connection::FactName::Owned)
                    });
                    if owned_supported && fact.owned.is_none() {
                        controlled = false;
                    }
                    // Unknown never defaults false: each nullable safety
                    // input must read an explicit known-safe value.
                    // `host_hidden`/`minimized`/`fullscreen` carry no
                    // unavailable-fact name, so their gates stay
                    // unconditional. `maximized` is capability-gated:
                    // unsupported `null` is the contract-valid absent
                    // value and must not hold the composed show;
                    // `Some(true)` still holds via the overlay gate above.
                    if fact.minimized != Some(false)
                        || fact.host_hidden != Some(false)
                        || fact.fullscreen != Some(false)
                    {
                        controlled = false;
                    }
                    let maximize_supported = self.capabilities().is_some_and(|caps| caps.maximize);
                    if maximize_supported && fact.maximized != Some(false) {
                        controlled = false;
                    }
                }
                None => controlled = false,
            }
            if derived.hold
                || derived.hidden
                || derived.overlay_fullscreen
                || derived.overlay_maximized
            {
                controlled = false;
            }
            if controlled {
                desire.visibility = Some(target);
                desire.visibility_authority = ScopeAuthority::Controlled;
                continue;
            }
            desire.visibility = Some(target);
            if excluded_lane {
                desire.visibility_authority = ScopeAuthority::Untouched;
            } else if self.staged.gesture_barrier {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::GestureBarrier);
            } else if !self.staged.observation.inventory_complete {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::IncompleteInventory);
            } else if self
                .staged
                .facts
                .get(&window)
                .is_some_and(|fact| fact.minimized == Some(true))
            {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::Minimized);
            } else if let Some(reason) = self.hold_reasons.get(&home).copied() {
                desire.visibility_authority = ScopeAuthority::Hold(reason);
            } else {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::Unreadable);
            }
        }
    }

    /// Central native sticky state publication over the assembled rows.
    /// Only native-capable sticky bindings with exact metadata publish a
    /// `StateTarget { property: Sticky, value, authority, command_seq }`.
    /// The command sequence is the original typed command; the retained
    /// authorizing revision (`target.revision`, `state_revision + 1` at
    /// issue) stays internal and never mints from raw facts or revision
    /// changes. Targets retain across observations; observation-only
    /// replies never renew. `Controlled` only for unspent readable
    /// no-held/no-excluded/no-min/no-overlay/no-safety-barrier complete
    /// windows with no pending same-window transfer; spent bindings never
    /// reauthorize automatically and stay `Untouched`/`Hold`. Property
    /// authority is independent of the row frame; the row `command_seq`
    /// lane is untouched.
    pub(crate) fn decorate_native_sticky_state(
        &self,
        held_scopes: &BTreeSet<DomainKey>,
        windows: &mut [WindowDesire],
    ) {
        if self.sticky_route() != StickyRoute::Native {
            return;
        }
        if !self.capabilities().is_some_and(|caps| caps.native_sticky) {
            return;
        }
        for desire in windows.iter_mut() {
            let window = desire.lifetime.clone();
            let Some(binding) = self
                .retained_states
                .get(&(window.clone(), StateProperty::Sticky))
            else {
                continue;
            };
            let Some(meta) = binding.sticky_meta.as_ref() else {
                continue;
            };
            if desire
                .state
                .iter()
                .any(|row| row.property == StateProperty::Sticky)
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
                && self.staged.observation.inventory_complete
                && !self.staged.gesture_barrier
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
                    if fact.minimized == Some(true)
                        || fact.host_hidden != Some(false)
                        || fact.fullscreen == Some(true)
                    {
                        controlled = false;
                    }
                    if fact.maximized == Some(true) {
                        controlled = false;
                    }
                }
                None => controlled = false,
            }
            if derived.hold
                || derived.hidden
                || derived.overlay_fullscreen
                || derived.overlay_maximized
            {
                controlled = false;
            }
            if binding.spent {
                controlled = false;
            }
            let authority = if controlled {
                ScopeAuthority::Controlled
            } else if excluded_lane {
                ScopeAuthority::Untouched
            } else if self.staged.gesture_barrier {
                ScopeAuthority::Hold(HoldReason::GestureBarrier)
            } else if !self.staged.observation.inventory_complete {
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
                property: StateProperty::Sticky,
                value: binding.target.value,
                authority,
                command_seq: Some(meta.command_seq),
            });
        }
    }
}
