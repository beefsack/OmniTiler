//! Commandless persistent-above owner (all floats, one owner).
//!
//! One core owner over ALL floats (automatic, explicit, sticky, capacity,
//! workspace-floating): acquisition mints a retained `(window, Above)`
//! binding at `state_revision + 1`, generic [`crate::world_state`]
//! settlement owns proof, and release/suspension mint `false` only after
//! acquire proof. Commandless throughout: every published target carries
//! `command_seq: None`. Support is capability-gated (`caps.above`); no
//! binding, write, or target exists without it.
//!
//! Union membership comes from the ASSEMBLED [`crate::world_report::WindowDesire`]
//! rows after all publish lanes complete: `floating == true` (controlled
//! exception rows) plus any row homed on a floating scope (untouched
//! workspace-floating rows). `float_origin` is never read (diagnostic
//! only). Ordinary floats and workspace-floating rows publish `Untouched`
//! membership/frame authority and stay independently eligible here.
//!
//! Safety is staged readable healthy facts only: complete inventory, no
//! gesture barrier, unheld readable scope, no transfer flight, engine- or
//! member-known (staged homing on a readable floating scope counts on the
//! first report: no session exists to know it yet), present facts with
//! `minimized != Some(true)`,
//! `host_hidden == Some(false)`, no fullscreen/maximized overlay, and
//! healthy derived state. Pre-existing `above`/`below` (either `Some(true)`
//! or unknown `None`) mint nothing and publish nothing: zero stacking
//! writes, untouched stays untouched. Durable preimages are an adapter
//! persist-before-write obligation; core gates on capability plus readable
//! facts and never reads `suppression_available` or any new ingress.
//!
//! Focused-fullscreen suspends owned `true` on the focused window's homed
//! scope with a `false` transition (other scopes unaffected; the overlay
//! subject itself holds via the healthy gate, never a replan). Proven
//! `false` (release or suspension) that floats again in a healthy
//! unsuspended scope resumes with a fresh `true` transition. External
//! `below == Some(true)` on any owned binding relinquishes into a
//! latched binding in place (no write, no row) that blocks reacquisition
//! until lifetime end or native preowned-above reset (which drops the
//! latch and stays untouched); external `above == Some(false)` latches
//! proven `true` only, never a pending `false` transition. Publication
//! is direction-aware: `true` targets need readable `above == Some(false)`
//! with `below == Some(false)` universal, so raw `true` alone never
//! authorizes and a `false` release from raw `true` stays reachable.
//! Sticky transitions retain owner/latch; sticky-off releases only via
//! the tile path.
//!
//! Runs once per [`WorldEngine::publish_v4`](crate::world_engine::WorldEngine::publish_v4), after all
//! rows assemble and before the sticky/fullscreen/maximize decorators.

use std::collections::BTreeSet;

use crate::directional::{WindowId, WorkspaceId};
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_report::{
    AttemptOutcome, HoldReason, ScopeAuthority, StateProperty, StateTarget, TransferKind,
    WindowDesire,
};
use crate::world_state::{MAX_RETAINED_STATE_BINDINGS, NativeAboveMeta};

impl WorldEngine {
    /// Focused-fullscreen homed suspension scope: the staged homing of the
    /// focused lifetime when its fact reads fullscreen. `None` otherwise.
    /// Other scopes are unaffected; the overlay subject itself holds via
    /// the healthy gate, never a replan.
    fn above_suspended_scope(&self) -> Option<DomainKey> {
        if self.staged.observation.focused_global.is_empty() {
            return None;
        }
        let focused = WindowId(self.staged.observation.focused_global.clone());
        let fact = self.staged.facts.get(&focused)?;
        if fact.fullscreen != Some(true) {
            return None;
        }
        self.staged_homing(&focused)
    }

    /// Single mutating above reconcile plus central decoration. Mints on
    /// actual policy transitions only (acquire, release, suspension,
    /// resume); observation-only replies never renew. Then publishes one
    /// `StateTarget` per producer-owned binding, skipping latched tombstones.
    pub(crate) fn reconcile_and_decorate_native_above_state(
        &mut self,
        held_scopes: &BTreeSet<DomainKey>,
        windows: &mut [WindowDesire],
    ) {
        if !self.capabilities().is_some_and(|caps| caps.above) {
            return;
        }
        let mut seen: BTreeSet<WindowId> = BTreeSet::new();
        let mut float_now: BTreeSet<WindowId> = BTreeSet::new();
        let mut homes: std::collections::BTreeMap<WindowId, DomainKey> =
            std::collections::BTreeMap::new();
        for desire in windows.iter() {
            if !seen.insert(desire.lifetime.clone()) {
                continue;
            }
            let home = self.staged_homing(&desire.lifetime).unwrap_or(DomainKey {
                output: desire.output.clone(),
                workspace: WorkspaceId(desire.workspace.clone()),
            });
            if desire.floating || self.floating_scopes.contains(&home) {
                float_now.insert(desire.lifetime.clone());
            }
            homes.insert(desire.lifetime.clone(), home);
        }
        let suspended = self.above_suspended_scope();
        // Latch reset: native preowned-above drops the latch untouched.
        let mut unlatch: Vec<(WindowId, StateProperty)> = Vec::new();
        for ((window, property), binding) in self.retained_states.iter() {
            if *property != StateProperty::Above {
                continue;
            }
            let Some(meta) = binding.above_meta.as_ref() else {
                continue;
            };
            if !meta.latched {
                continue;
            }
            if self
                .staged
                .facts
                .get(window)
                .is_some_and(|fact| fact.above == Some(true))
            {
                unlatch.push((window.clone(), *property));
            }
        }
        for key in unlatch {
            self.retained_states.remove(&key);
        }
        // External below on any core-owned producer binding relinquishes
        // into a latched binding in place: bookkeeping only, preserving the
        // exact tuple/pending/spent/disposition, regardless of target
        // polarity or attempt disposition. No write, no row, no reacquire.
        // External above-clear only applies to proven true: a pending false
        // transition legitimately reads false, so it must never be
        // misread as an external clear.
        for ((window, property), binding) in self.retained_states.iter_mut() {
            if *property != StateProperty::Above {
                continue;
            }
            let Some(meta) = binding.above_meta.as_mut() else {
                continue;
            };
            if meta.latched {
                continue;
            }
            let Some(fact) = self.staged.facts.get(window) else {
                continue;
            };
            if fact.below == Some(true)
                || (binding.target.value
                    && binding.disposition == AttemptOutcome::Applied
                    && fact.above == Some(false))
            {
                meta.latched = true;
            }
        }
        let revision = self.state_revision + 1;
        // Acquire: newly eligible safe floats with explicit false stacking
        // facts mint exactly one unspent true target. Suspended scopes,
        // latched tombstones, and a full binding table mint nothing.
        // Running count keeps the bound exact; replacements below reuse keys.
        let mut acquire: Vec<(WindowId, DomainKey)> = Vec::new();
        let mut used = self.retained_states.len();
        for window in &float_now {
            let key = (window.clone(), StateProperty::Above);
            if self.retained_states.contains_key(&key) {
                continue;
            }
            let home = homes.get(window).cloned().unwrap_or_else(|| {
                self.staged_homing(window).unwrap_or(DomainKey {
                    output: crate::directional::OutputId(String::new()),
                    workspace: WorkspaceId(String::new()),
                })
            });
            if suspended.as_ref().is_some_and(|scope| scope == &home) {
                continue;
            }
            if !self.above_publish_healthy(held_scopes, window, &home) {
                continue;
            }
            let Some(fact) = self.staged.facts.get(window) else {
                continue;
            };
            if fact.above != Some(false) || fact.below != Some(false) {
                continue;
            }
            if used >= MAX_RETAINED_STATE_BINDINGS {
                continue;
            }
            used += 1;
            acquire.push((window.clone(), home));
        }
        // Release/suspend/resume over producer-owned bindings only (manual
        // settlement-era inserts carry no above_meta and stay untouched).
        let mut release: Vec<(WindowId, DomainKey)> = Vec::new();
        let mut reacquire: Vec<(WindowId, DomainKey)> = Vec::new();
        let mut drop_unowned: Vec<(WindowId, StateProperty)> = Vec::new();
        for ((window, property), binding) in self.retained_states.iter() {
            if *property != StateProperty::Above {
                continue;
            }
            let Some(meta) = binding.above_meta.as_ref() else {
                continue;
            };
            if meta.latched {
                continue;
            }
            let floating = float_now.contains(window);
            let home = self
                .staged_homing(window)
                .unwrap_or_else(|| meta.home.clone());
            if binding.target.value {
                if binding.disposition != AttemptOutcome::Applied {
                    if !floating {
                        drop_unowned.push((window.clone(), StateProperty::Above));
                    }
                    continue;
                }
                if floating {
                    // Suspension: proven true on the suspended scope mints
                    // false. The overlay subject holds via health, never here.
                    if suspended.as_ref().is_some_and(|scope| scope == &home)
                        && self.above_publish_healthy(held_scopes, window, &home)
                    {
                        release.push((window.clone(), home));
                    }
                    continue;
                }
                // Float->tile: release false only after acquire proof.
                // Unproven bindings drop with no owned change and no write.
                if !self.above_publish_healthy(held_scopes, window, &home) {
                    continue;
                }
                release.push((window.clone(), home));
            } else if floating && binding.disposition == AttemptOutcome::Applied {
                // Proven false that floats again in a healthy unsuspended
                // scope is a fresh transition (release- or suspension-false
                // resume), never a spent retry: opposite value, new revision.
                if suspended.as_ref().is_some_and(|scope| scope == &home) {
                    continue;
                }
                if !self.above_publish_healthy(held_scopes, window, &home) {
                    continue;
                }
                let Some(fact) = self.staged.facts.get(window) else {
                    continue;
                };
                if fact.above != Some(false) || fact.below != Some(false) {
                    continue;
                }
                reacquire.push((window.clone(), home));
            }
        }
        for (window, home) in acquire.into_iter().chain(reacquire) {
            self.retained_states.insert(
                (window.clone(), StateProperty::Above),
                crate::world_state::RetainedStateBinding {
                    target: crate::world_state::RetainedStateTarget {
                        window: window.clone(),
                        property: StateProperty::Above,
                        value: true,
                        revision,
                    },
                    pending: None,
                    spent: false,
                    disposition: AttemptOutcome::Unattempted,
                    sticky_meta: None,
                    fullscreen_meta: None,
                    maximize_meta: None,
                    above_meta: Some(NativeAboveMeta {
                        home,
                        latched: false,
                    }),
                },
            );
        }
        for (window, home) in release {
            self.retained_states.insert(
                (window.clone(), StateProperty::Above),
                crate::world_state::RetainedStateBinding {
                    target: crate::world_state::RetainedStateTarget {
                        window: window.clone(),
                        property: StateProperty::Above,
                        value: false,
                        revision,
                    },
                    pending: None,
                    spent: false,
                    disposition: AttemptOutcome::Unattempted,
                    sticky_meta: None,
                    fullscreen_meta: None,
                    maximize_meta: None,
                    above_meta: Some(NativeAboveMeta {
                        home,
                        latched: false,
                    }),
                },
            );
        }
        for key in drop_unowned {
            self.retained_states.remove(&key);
        }
        // Central decoration: producer-owned bindings publish exactly one
        // Above row except latched tombstones (no row). Commandless always:
        // command_seq None. Spent bindings never reauthorize;
        // observation-only replies renew nothing. True targets hold (never
        // Controlled) on the suspended scope; false transitions stay
        // Controlled there.
        for desire in windows.iter_mut() {
            let window = desire.lifetime.clone();
            let Some(binding) = self
                .retained_states
                .get(&(window.clone(), StateProperty::Above))
            else {
                continue;
            };
            let Some(meta) = binding.above_meta.as_ref() else {
                continue;
            };
            if meta.latched {
                continue;
            }
            if desire
                .state
                .iter()
                .any(|row| row.property == StateProperty::Above)
            {
                continue;
            }
            let home = self.staged_homing(&window).unwrap_or(meta.home.clone());
            let derived = self.derived_for(&window);
            let excluded_lane =
                self.is_excluded_window(&window) || derived.excluded || derived.own_excluded;
            let suspended_true =
                binding.target.value && suspended.as_ref().is_some_and(|scope| scope == &home);
            // Direction-aware stacking veto: true targets need readable
            // above false, below false is universal. Never a global
            // above-false gate: an owned false release from raw above true
            // stays reachable.
            let stack_ok = self.staged.facts.get(&window).is_some_and(|fact| {
                fact.below == Some(false) && (!binding.target.value || fact.above == Some(false))
            });
            let controlled = !binding.spent
                && !suspended_true
                && stack_ok
                && self.above_publish_healthy(held_scopes, &window, &home);
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
                property: StateProperty::Above,
                value: binding.target.value,
                authority,
                command_seq: None,
            });
        }
    }

    /// Healthy-publish gate: staged readable facts with no veto. Unknown
    /// stacking facts (`above`/`below` None) fail here only for the
    /// mint path via explicit `Some(false)` checks; decoration reuses
    /// the same gate so unhealthy bindings publish Hold, never Controlled.
    fn above_publish_healthy(
        &self,
        held_scopes: &BTreeSet<DomainKey>,
        window: &WindowId,
        home: &DomainKey,
    ) -> bool {
        if !self.staged.observation.inventory_complete || self.staged.gesture_barrier {
            return false;
        }
        if held_scopes.contains(home) {
            return false;
        }
        if self.is_held_window(window) || self.is_excluded_window(window) {
            return false;
        }
        if !self
            .staged
            .domains
            .iter()
            .any(|domain| domain.key == *home && domain.readable)
        {
            return false;
        }
        if self
            .retained
            .iter()
            .any(|transfer| matches!(&transfer.target.kind, TransferKind::Window(w) if w == window))
        {
            return false;
        }
        if !(self.engine_knows_window_any(window)
            || self.members.contains_key(window)
            // Floating scopes host no session, so retained knowledge only
            // arrives one report later via post-publish membership
            // convergence: staged homing on the readable floating scope is
            // the first-report evidence.
            || (self.floating_scopes.contains(home)
                && self.staged_homing(window).as_ref() == Some(home)))
        {
            return false;
        }
        match self.staged.facts.get(window) {
            Some(fact) => {
                if fact.minimized == Some(true)
                    || fact.host_hidden != Some(false)
                    || fact.fullscreen == Some(true)
                    || fact.maximized == Some(true)
                {
                    return false;
                }
                if fact.above.is_none() || fact.below.is_none() {
                    return false;
                }
            }
            None => return false,
        }
        let derived = self.derived_for(window);
        if derived.hold
            || derived.hidden
            || derived.overlay_fullscreen
            || derived.overlay_maximized
            || derived.excluded
            || derived.own_excluded
        {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directional::WindowId;
    use crate::world_engine::WorldEngine;
    use crate::world_report::{FrameSetter, StateAttempt, StateProperty};
    use crate::world_v4_support_tests::{accepted, fact, float_fact, report_full, screen};

    fn caps_above() -> crate::connection::HostCapabilities {
        crate::connection::HostCapabilities {
            fullscreen: true,
            maximize: true,
            native_sticky: true,
            above: true,
            ..Default::default()
        }
    }

    fn engine_above() -> WorldEngine {
        WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps_above())
            .expect("generation")
    }

    fn engine_no_above() -> WorldEngine {
        WorldEngine::new_test_bound_with_capabilities(
            "gen-1",
            "adapter-1",
            crate::connection::HostCapabilities::default(),
        )
        .expect("generation")
    }

    fn above_row(reply: &crate::world_report::WorldReplyV4, window: &str) -> Option<StateTarget> {
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)?
            .state
            .iter()
            .find(|row| row.property == StateProperty::Above)
            .copied()
    }

    #[test]
    fn commandless_acquire_on_safe_float() {
        let mut engine = engine_above();
        let reply = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let row = above_row(&reply, "win-a").expect("above target minted");
        assert!(row.value);
        assert_eq!(row.authority, ScopeAuthority::Controlled);
        assert_eq!(row.command_seq, None);
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("binding retained");
        assert!(binding.target.value);
        assert_eq!(binding.target.revision, reply.state_revision);
        assert!(binding.above_meta.is_some());
        assert!(!binding.spent);
    }

    #[test]
    fn exact_proof_owns_and_holds_without_renewal() {
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let rev = first.state_revision;
        let mut shown = float_fact("win-a", "out-a", 100, 100);
        shown.above = Some(true);
        let mut proven = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![shown],
            vec![],
            "win-a",
            None,
            None,
        );
        proven.state_attempts = vec![StateAttempt {
            window: WindowId("win-a".to_owned()),
            property: StateProperty::Above,
            target: true,
            revision: rev,
            setter: FrameSetter::Accepted,
            pre: Some(false),
            post: Some(true),
            attempt_ms: 7,
        }];
        let reply = accepted(&mut engine, &proven);
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("binding retained");
        assert!(binding.spent);
        assert_eq!(binding.disposition, AttemptOutcome::Applied);
        // Spent proof never reauthorizes: no Controlled renewal.
        assert!(
            above_row(&reply, "win-a")
                .is_none_or(|row| row.authority != ScopeAuthority::Controlled)
        );
        // Observation-only followup renews nothing.
        let quiet = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![{
                    let mut row = float_fact("win-a", "out-a", 100, 100);
                    row.above = Some(true);
                    row
                }],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("binding retained");
        assert_eq!(binding.target.revision, rev);
        assert!(
            above_row(&quiet, "win-a")
                .is_none_or(|row| row.authority != ScopeAuthority::Controlled)
        );
    }

    #[test]
    fn lost_acquire_republish_accepts_republished_stamp() {
        // Lost commandless acquire at R: the observation-only republish at
        // R+1 carries the Controlled row (`command_seq: None`) with the
        // same binding revision, and the first acted-on attempt stamps R+1
        // and latches as the original raw evidence.
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let rev = first.state_revision;
        let row = above_row(&first, "win-a").expect("acquire target");
        assert!(row.value);
        assert_eq!(row.authority, ScopeAuthority::Controlled);
        assert_eq!(row.command_seq, None);
        // The acquire reply is lost: native stays below, so the next
        // observation-only reply republishes Controlled with the same
        // binding revision.
        let second = accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert_eq!(second.state_revision, rev + 1);
        let row = above_row(&second, "win-a").expect("republished target");
        assert!(row.value);
        assert_eq!(row.authority, ScopeAuthority::Controlled);
        assert_eq!(row.command_seq, None);
        assert_eq!(
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
                .expect("binding retained")
                .target
                .revision,
            rev,
        );
        // First acted-on attempt stamps the republish revision R+1.
        let mut shown = float_fact("win-a", "out-a", 100, 100);
        shown.above = Some(true);
        let mut proven = report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![shown],
            vec![],
            "win-a",
            None,
            None,
        );
        let attempt_row = StateAttempt {
            window: WindowId("win-a".to_owned()),
            property: StateProperty::Above,
            target: true,
            revision: rev + 1,
            setter: FrameSetter::Accepted,
            pre: Some(false),
            post: Some(true),
            attempt_ms: 7,
        };
        proven.state_attempts = vec![attempt_row.clone()];
        accepted(&mut engine, &proven);
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("binding retained");
        assert!(binding.spent);
        assert_eq!(binding.pending.as_ref(), Some(&attempt_row));
        assert_eq!(binding.disposition, AttemptOutcome::Applied);
    }

    #[test]
    fn float_to_tile_releases_false_only_after_proof() {
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let rev = first.state_revision;
        let mut shown = float_fact("win-a", "out-a", 100, 100);
        shown.above = Some(true);
        let mut proven = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![shown],
            vec![],
            "win-a",
            None,
            None,
        );
        proven.state_attempts = vec![StateAttempt {
            window: WindowId("win-a".to_owned()),
            property: StateProperty::Above,
            target: true,
            revision: rev,
            setter: FrameSetter::Accepted,
            pre: Some(false),
            post: Some(true),
            attempt_ms: 7,
        }];
        accepted(&mut engine, &proven);
        // Tile the window: ordinary fact drops float membership while native
        // still holds the proven true (core must clear it).
        let mut tiled = fact("win-a", "out-a", 0);
        tiled.above = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![tiled],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let row = above_row(&reply, "win-a").expect("release minted");
        assert!(!row.value);
        assert_eq!(row.authority, ScopeAuthority::Controlled);
        assert_eq!(row.command_seq, None);
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("binding retained");
        assert!(!binding.target.value);
        assert!(binding.target.revision > rev);
        assert!(!binding.spent);
    }

    #[test]
    fn unproven_float_to_tile_releases_nothing() {
        let mut engine = engine_above();
        accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(
            engine
                .retained_states
                .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Above))
        );
        // Tile before any proof: no owned preimage, so no false write and
        // no stale latch.
        let reply = accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        assert!(
            !engine
                .retained_states
                .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Above))
        );
    }

    #[test]
    fn preexisting_stacking_facts_mint_nothing() {
        for (above, below) in [
            (Some(true), Some(false)),
            (Some(false), Some(true)),
            (Some(true), Some(true)),
            (None, Some(false)),
            (Some(false), None),
        ] {
            let mut engine = engine_above();
            let mut row = float_fact("win-a", "out-a", 100, 100);
            row.above = above;
            row.below = below;
            let reply = accepted(
                &mut engine,
                &report_full(
                    1,
                    1,
                    vec![screen("out-a", 0)],
                    vec![row],
                    vec![],
                    "win-a",
                    None,
                    None,
                ),
            );
            assert!(
                above_row(&reply, "win-a").is_none(),
                "above={above:?} below={below:?}"
            );
            assert!(
                !engine
                    .retained_states
                    .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Above)),
                "above={above:?} below={below:?}"
            );
        }
    }

    #[test]
    fn no_capability_means_no_binding_or_target() {
        let mut engine = engine_no_above();
        let reply = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        assert!(engine.retained_states.is_empty());
    }

    #[test]
    fn failed_acquire_never_retries_or_invents_ownership() {
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let rev = first.state_revision;
        let mut refused = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![float_fact("win-a", "out-a", 100, 100)],
            vec![],
            "win-a",
            None,
            None,
        );
        refused.state_attempts = vec![StateAttempt {
            window: WindowId("win-a".to_owned()),
            property: StateProperty::Above,
            target: true,
            revision: rev,
            setter: FrameSetter::Refused,
            pre: Some(false),
            post: Some(false),
            attempt_ms: 7,
        }];
        accepted(&mut engine, &refused);
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("binding retained");
        assert!(binding.spent);
        assert_eq!(binding.disposition, AttemptOutcome::RefusedProven);
        // Still floating, still false: no blind retry, same revision.
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("binding retained");
        assert_eq!(binding.target.revision, rev);
        assert!(
            above_row(&reply, "win-a")
                .is_none_or(|row| row.authority != ScopeAuthority::Controlled)
        );
    }

    #[test]
    fn raw_true_alone_never_invents_ownership() {
        // No binding exists: raw true with no exact proof mints nothing
        // and records nothing, even though the fact reads applied.
        let mut engine = engine_above();
        let mut row = float_fact("win-a", "out-a", 100, 100);
        row.above = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![row],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        assert!(
            !engine
                .retained_states
                .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Above))
        );
    }

    fn prove_above(engine: &mut WorldEngine, seq: u64, window: &str, rev: u64, pre: bool) {
        let mut shown = float_fact(window, "out-a", 100, 100);
        shown.above = Some(true);
        let mut report = report_full(
            seq,
            seq,
            vec![screen("out-a", 0)],
            vec![shown],
            vec![],
            window,
            None,
            None,
        );
        report.state_attempts = vec![StateAttempt {
            window: WindowId(window.to_owned()),
            property: StateProperty::Above,
            target: true,
            revision: rev,
            setter: FrameSetter::Accepted,
            pre: Some(pre),
            post: Some(true),
            attempt_ms: 7,
        }];
        accepted(engine, &report);
    }

    #[test]
    fn focused_fullscreen_suspends_homed_scope_then_resumes() {
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![
                    float_fact("win-float", "out-a", 100, 100),
                    float_fact("win-other", "out-b", 900, 100),
                    fact("win-full", "out-a", 300),
                ],
                vec![],
                "win-float",
                None,
                None,
            ),
        );
        let rev = first.state_revision;
        // Prove both acquisitions sequentially (same authorizing revision).
        for (seq, window) in [(2u64, "win-float"), (3u64, "win-other")] {
            let mut a = float_fact("win-float", "out-a", 100, 100);
            a.above = Some(true);
            let mut b = float_fact("win-other", "out-b", 900, 100);
            b.above = Some(true);
            let mut report = report_full(
                seq,
                seq,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![a, b, fact("win-full", "out-a", 300)],
                vec![],
                window,
                None,
                None,
            );
            report.state_attempts = vec![StateAttempt {
                window: WindowId(window.to_owned()),
                property: StateProperty::Above,
                target: true,
                revision: rev,
                setter: FrameSetter::Accepted,
                pre: Some(false),
                post: Some(true),
                attempt_ms: 7,
            }];
            accepted(&mut engine, &report);
        }
        for window in ["win-float", "win-other"] {
            let binding = engine
                .retained_states
                .get(&(WindowId(window.to_owned()), StateProperty::Above))
                .expect("proven binding");
            assert_eq!(binding.disposition, AttemptOutcome::Applied);
            assert!(binding.spent);
        }
        // Focused fullscreen on out-a suspends the homed float only.
        let mut full = fact("win-full", "out-a", 300);
        full.fullscreen = Some(true);
        let mut a = float_fact("win-float", "out-a", 100, 100);
        a.above = Some(true);
        let mut b = float_fact("win-other", "out-b", 900, 100);
        b.above = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                4,
                4,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![a, b, full],
                vec![],
                "win-full",
                None,
                None,
            ),
        );
        let suspended = above_row(&reply, "win-float").expect("suspension false");
        assert!(!suspended.value);
        assert_eq!(suspended.authority, ScopeAuthority::Controlled);
        assert_eq!(suspended.command_seq, None);
        // Other output unaffected: no fresh false, revision stable.
        let other_rev = engine
            .retained_states
            .get(&(WindowId("win-other".to_owned()), StateProperty::Above))
            .expect("other retained")
            .target
            .revision;
        assert!(
            above_row(&reply, "win-other")
                .is_none_or(|row| row.value || row.authority != ScopeAuthority::Controlled)
        );
        let suspension_rev = engine
            .retained_states
            .get(&(WindowId("win-float".to_owned()), StateProperty::Above))
            .expect("suspension binding")
            .target
            .revision;
        assert!(
            suspension_rev > rev,
            "suspension is a fresh transition, not a retry"
        );
        assert_eq!(
            engine
                .retained_states
                .get(&(WindowId("win-other".to_owned()), StateProperty::Above))
                .expect("other retained")
                .target
                .revision,
            other_rev
        );
        // Prove the suspension false, then exit fullscreen and resume true.
        let mut a = float_fact("win-float", "out-a", 100, 100);
        a.above = Some(false);
        let mut b = float_fact("win-other", "out-b", 900, 100);
        b.above = Some(true);
        let mut full = fact("win-full", "out-a", 300);
        full.fullscreen = Some(true);
        let mut proven = report_full(
            5,
            5,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![a, b, full],
            vec![],
            "win-full",
            None,
            None,
        );
        proven.state_attempts = vec![StateAttempt {
            window: WindowId("win-float".to_owned()),
            property: StateProperty::Above,
            target: false,
            revision: suspension_rev,
            setter: FrameSetter::Accepted,
            pre: Some(true),
            post: Some(false),
            attempt_ms: 7,
        }];
        accepted(&mut engine, &proven);
        let binding = engine
            .retained_states
            .get(&(WindowId("win-float".to_owned()), StateProperty::Above))
            .expect("proven suspension");
        assert_eq!(binding.disposition, AttemptOutcome::Applied);
        assert!(!binding.target.value);
        let mut a = float_fact("win-float", "out-a", 100, 100);
        a.above = Some(false);
        let mut b = float_fact("win-other", "out-b", 900, 100);
        b.above = Some(true);
        let resumed = accepted(
            &mut engine,
            &report_full(
                6,
                6,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![a, b, fact("win-full", "out-a", 300)],
                vec![],
                "win-float",
                None,
                None,
            ),
        );
        let row = above_row(&resumed, "win-float").expect("resume true");
        assert!(row.value);
        assert_eq!(row.authority, ScopeAuthority::Controlled);
        assert_eq!(row.command_seq, None);
    }

    #[test]
    fn external_clear_and_below_latch_until_preowned_reset() {
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        prove_above(&mut engine, 2, "win-a", first.state_revision, false);
        let owned = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("proven owned")
            .clone();
        assert_eq!(owned.disposition, AttemptOutcome::Applied);
        // External clear while still floating: no write, no row, latched.
        let mut cleared = float_fact("win-a", "out-a", 100, 100);
        cleared.above = Some(false);
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![cleared],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("latch tombstone");
        assert!(binding.above_meta.as_ref().is_some_and(|meta| meta.latched));
        // Latch preserves the exact tuple/proof: no new rev, no fabricated
        // settlement evidence.
        assert_eq!(binding.target, owned.target);
        assert_eq!(binding.pending, owned.pending);
        assert_eq!(binding.spent, owned.spent);
        assert_eq!(binding.disposition, AttemptOutcome::Applied);
        // Still floating with false facts: no reacquire while latched.
        let mut again = float_fact("win-a", "out-a", 100, 100);
        again.above = Some(false);
        let reply = accepted(
            &mut engine,
            &report_full(
                4,
                4,
                vec![screen("out-a", 0)],
                vec![again],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        assert!(
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
                .is_some_and(|binding| binding
                    .above_meta
                    .as_ref()
                    .is_some_and(|meta| meta.latched))
        );
        // Native preowned-above resets the latch and stays untouched.
        let mut preowned = float_fact("win-a", "out-a", 100, 100);
        preowned.above = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                5,
                5,
                vec![screen("out-a", 0)],
                vec![preowned],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        assert!(
            !engine
                .retained_states
                .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Above))
        );
        // External below latches the same way.
        let first = accepted(
            &mut engine,
            &report_full(
                6,
                6,
                vec![screen("out-a", 0)],
                vec![float_fact("win-b", "out-a", 200, 100)],
                vec![],
                "win-b",
                None,
                None,
            ),
        );
        let mut shown = float_fact("win-b", "out-a", 200, 100);
        shown.above = Some(true);
        let mut report = report_full(
            7,
            7,
            vec![screen("out-a", 0)],
            vec![shown],
            vec![],
            "win-b",
            None,
            None,
        );
        report.state_attempts = vec![StateAttempt {
            window: WindowId("win-b".to_owned()),
            property: StateProperty::Above,
            target: true,
            revision: first.state_revision,
            setter: FrameSetter::Accepted,
            pre: Some(false),
            post: Some(true),
            attempt_ms: 7,
        }];
        accepted(&mut engine, &report);
        let mut belowed = float_fact("win-b", "out-a", 200, 100);
        belowed.above = Some(true);
        belowed.below = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                8,
                8,
                vec![screen("out-a", 0)],
                vec![belowed],
                vec![],
                "win-b",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-b").is_none());
        assert!(
            engine
                .retained_states
                .get(&(WindowId("win-b".to_owned()), StateProperty::Above))
                .is_some_and(|binding| binding
                    .above_meta
                    .as_ref()
                    .is_some_and(|meta| meta.latched))
        );
    }

    #[test]
    fn external_below_latches_pending_true_without_controlled_write() {
        // B1: pending acquire then external below with no attempt: no
        // stacking write, no row, latch in place preserving the tuple.
        let mut engine = engine_above();
        accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let owned = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("pending acquire")
            .clone();
        assert!(!owned.spent);
        assert_eq!(owned.disposition, AttemptOutcome::Unattempted);
        let mut belowed = float_fact("win-a", "out-a", 100, 100);
        belowed.above = Some(false);
        belowed.below = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![belowed],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("latched")
            .clone();
        assert!(binding.above_meta.as_ref().is_some_and(|meta| meta.latched));
        assert_eq!(binding.target, owned.target);
        assert_eq!(binding.pending, owned.pending);
        assert_eq!(binding.spent, owned.spent);
        assert_eq!(binding.disposition, owned.disposition);
        // Below clears with no preowned above: latch persists, still no row
        // and no retry.
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        assert!(
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
                .is_some_and(|binding| binding
                    .above_meta
                    .as_ref()
                    .is_some_and(|meta| meta.latched))
        );
    }

    #[test]
    fn latched_unsettled_acquire_stays_silent_after_late_attempt() {
        // Adapter-invoked but unreported acquire meets external below: the
        // binding latches while unsettled (unspent, no pending). The late
        // attempt still correlates and spends, yet no publish reauthorizes
        // the identity: no row, never Controlled again. A latch never
        // unlatches in place; only native preowned-above deletes it.
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let rev = first.state_revision;
        let mut belowed = float_fact("win-a", "out-a", 100, 100);
        belowed.above = Some(false);
        belowed.below = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![belowed],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        let mut shown = float_fact("win-a", "out-a", 100, 100);
        shown.above = Some(false);
        shown.below = Some(true);
        let mut report = report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![shown],
            vec![],
            "win-a",
            None,
            None,
        );
        report.state_attempts = vec![StateAttempt {
            window: WindowId("win-a".to_owned()),
            property: StateProperty::Above,
            target: true,
            revision: rev,
            setter: FrameSetter::Accepted,
            pre: Some(false),
            post: Some(true),
            attempt_ms: 7,
        }];
        accepted(&mut engine, &report);
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("latched binding retained")
            .clone();
        assert!(binding.spent);
        assert!(binding.above_meta.as_ref().is_some_and(|meta| meta.latched));
        let reply = accepted(
            &mut engine,
            &report_full(
                4,
                4,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        assert!(
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
                .is_some_and(|binding| binding
                    .above_meta
                    .as_ref()
                    .is_some_and(|meta| meta.latched))
        );
    }

    #[test]
    fn external_below_latches_pending_release_without_controlled_write() {
        // B2: proven acquire, tile mints release false, then external below
        // with no attempt: no second write, no row, latch preserves tuple;
        // below clearing alone never retries.
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        prove_above(&mut engine, 2, "win-a", first.state_revision, false);
        let mut tiled = fact("win-a", "out-a", 0);
        tiled.above = Some(true);
        tiled.below = Some(false);
        accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![tiled],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let owned = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("pending release")
            .clone();
        assert!(!owned.target.value);
        assert!(!owned.spent);
        let mut belowed = fact("win-a", "out-a", 0);
        belowed.above = Some(true);
        belowed.below = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                4,
                4,
                vec![screen("out-a", 0)],
                vec![belowed],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("latched")
            .clone();
        assert!(binding.above_meta.as_ref().is_some_and(|meta| meta.latched));
        assert_eq!(binding.target, owned.target);
        assert_eq!(binding.pending, owned.pending);
        assert_eq!(binding.spent, owned.spent);
        assert_eq!(binding.disposition, owned.disposition);
        // Below clears with still no preowned above: latch persists, no
        // row, no retry.
        let mut cleared = fact("win-a", "out-a", 0);
        cleared.above = Some(false);
        cleared.below = Some(false);
        let reply = accepted(
            &mut engine,
            &report_full(
                5,
                5,
                vec![screen("out-a", 0)],
                vec![cleared],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        assert!(
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
                .is_some_and(|binding| binding
                    .above_meta
                    .as_ref()
                    .is_some_and(|meta| meta.latched))
        );
    }

    #[test]
    fn pending_true_with_raw_above_true_publishes_no_controlled_write() {
        // Pending acquire then native above reads true before any
        // correlated attempt: preexisting above stays untouched, raw true
        // alone never authorizes a Controlled true.
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let rev = first.state_revision;
        let mut preowned = float_fact("win-a", "out-a", 100, 100);
        preowned.above = Some(true);
        preowned.below = Some(false);
        let reply = accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![preowned],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(
            above_row(&reply, "win-a")
                .is_none_or(|row| row.authority != ScopeAuthority::Controlled)
        );
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("pending binding retained");
        assert_eq!(binding.target.revision, rev);
        assert!(!binding.spent);
        assert!(!binding.above_meta.as_ref().is_some_and(|meta| meta.latched));
    }

    #[test]
    fn external_clear_with_simultaneous_tile_latches_without_false_write() {
        // Owned float takes external clear in the same report it returns
        // to tile: relinquish with no restoration write, same latch.
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        prove_above(&mut engine, 2, "win-a", first.state_revision, false);
        let owned_rev = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("proven owned")
            .target
            .revision;
        let mut tiled_cleared = fact("win-a", "out-a", 0);
        tiled_cleared.above = Some(false);
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![tiled_cleared],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(above_row(&reply, "win-a").is_none());
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("latched");
        assert!(binding.above_meta.as_ref().is_some_and(|meta| meta.latched));
        assert_eq!(binding.target.revision, owned_rev);
        assert_eq!(binding.disposition, AttemptOutcome::Applied);
    }

    #[test]
    fn sticky_transitions_retain_owner_and_tile_releases() {
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        prove_above(&mut engine, 2, "win-a", first.state_revision, false);
        let owned_rev = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
            .expect("owned")
            .target
            .revision;
        // Sticky-on while floating retains the same ownership.
        let mut sticky = float_fact("win-a", "out-a", 100, 100);
        sticky.sticky = Some(true);
        sticky.above = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![sticky],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(
            above_row(&reply, "win-a")
                .is_none_or(|row| row.authority != ScopeAuthority::Controlled)
        );
        assert_eq!(
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
                .expect("retained across sticky")
                .target
                .revision,
            owned_rev
        );
        // Sticky-off back to tile releases false (native still holds true).
        let mut tiled = fact("win-a", "out-a", 0);
        tiled.above = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                4,
                4,
                vec![screen("out-a", 0)],
                vec![tiled],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let row = above_row(&reply, "win-a").expect("tile release");
        assert!(!row.value);
        assert_eq!(row.authority, ScopeAuthority::Controlled);
    }

    #[test]
    fn capacity_floats_acquire_above_with_diagnostic_origin_ignored() {
        use crate::directional::OutputId;
        use crate::geometry::Rect;
        use crate::world_report::{FloatOrigin, ScreenReport};
        // Production capacity route: tiny 10x10 with zero outer gap
        // exhausts plain tile facts into capacity floats (no transient
        // mark, no command). `float_origin` rides diagnostic-only and
        // grants nothing; union eligibility keys off the assembled
        // floating row.
        let mut engine = WorldEngine::new_test_bound_with_initial_settings(
            "gen-1",
            "adapter-1",
            caps_above(),
            vec![(
                "outer".to_owned(),
                crate::connection::RawSettingValue::Number(0.0),
            )],
        )
        .expect("generation");
        let tiny = ScreenReport {
            output: OutputId("out-a".to_owned()),
            full_rect: Some(Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            }),
            work_area: Some(Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            }),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        };
        let windows: Vec<_> = (0..6)
            .map(|i| fact(&format!("win-{i}"), "out-a", 0))
            .collect();
        let reply = accepted(
            &mut engine,
            &report_full(1, 1, vec![tiny], windows, vec![], "", None, None),
        );
        let floats: Vec<_> = reply.windows.iter().filter(|row| row.floating).collect();
        assert!(
            !floats.is_empty(),
            "tiny 10x10 exhausts into capacity floats: {:?}",
            reply.windows
        );
        for row in &floats {
            assert_eq!(
                row.float_origin,
                Some(FloatOrigin::Capacity),
                "{} carries the diagnostic origin",
                row.lifetime.0
            );
            let above = above_row(&reply, &row.lifetime.0)
                .expect("capacity float acquires above like any float");
            assert!(above.value, "{}", row.lifetime.0);
            assert_eq!(above.authority, ScopeAuthority::Controlled);
            assert_eq!(above.command_seq, None);
        }
        // Tiled survivors are not floats: no acquisition.
        for row in reply.windows.iter().filter(|row| !row.floating) {
            assert!(
                above_row(&reply, &row.lifetime.0).is_none(),
                "tile {} mints nothing",
                row.lifetime.0
            );
            assert_eq!(row.float_origin, None);
        }
    }

    #[test]
    fn workspace_floating_scope_rows_acquire_above() {
        use crate::directional::OutputId;
        use crate::world_report::WorldCommand;
        // Workspace-floating route: toggling the workspace to floating
        // releases the session, so rows publish `floating: true` with
        // empty `float_origin` and untouched authorities. Union
        // eligibility keys off the assembled floating row (or the
        // floating scope home), never `float_origin`.
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let ws = first
            .workspaces
            .iter()
            .find(|row| row.output.0 == "out-a")
            .map(|row| row.id.clone())
            .expect("workspace");
        let reply = accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "win-a",
                Some(11),
                Some(WorldCommand::SetWorkspaceTiling {
                    output: OutputId("out-a".to_owned()),
                    workspace: ws.clone(),
                    tiled: false,
                }),
            ),
        );
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("floating-scope row");
        assert!(
            row.floating,
            "floating scopes publish float-marked rows with no write authority"
        );
        assert_eq!(row.float_origin, None, "empty diagnostic origin");
        assert!(
            engine
                .floating_scopes
                .iter()
                .any(|key| key.workspace.0 == ws)
        );
        let above = above_row(&reply, "win-a").expect("workspace-floating acquires above");
        assert!(above.value);
        assert_eq!(above.authority, ScopeAuthority::Controlled);
        assert_eq!(above.command_seq, None);
        // A newcomer observed only on the floating scope acquires on its
        // first report: staged homing on a readable floating scope is the
        // evidence (no session exists to know it first).
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-new", "out-a", 0)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let newcomer = above_row(&reply, "win-new").expect("floating-scope newcomer acquires");
        assert!(newcomer.value);
        assert_eq!(newcomer.authority, ScopeAuthority::Controlled);
        assert_eq!(newcomer.command_seq, None);
        let newcomer_row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-new")
            .expect("floating-scope newcomer row");
        assert!(
            newcomer_row.floating,
            "floating-scope newcomer publishes float-marked"
        );
        assert_eq!(newcomer_row.float_origin, None, "empty diagnostic origin");
        assert_eq!(
            newcomer_row.frame_authority,
            ScopeAuthority::Untouched,
            "newcomer frame untouched"
        );
        assert_eq!(
            newcomer_row.membership_authority,
            ScopeAuthority::Untouched,
            "newcomer membership untouched"
        );
    }

    #[test]
    fn workspace_floating_minimized_row_stays_hold_without_above() {
        use crate::directional::OutputId;
        use crate::world_report::{HoldReason, WorldCommand};
        // Second floating-scope arm: minimized occupants publish
        // `floating: true` with empty `float_origin`, minimized frame
        // hold, untouched membership, and no allocation or above target.
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let ws = first
            .workspaces
            .iter()
            .find(|row| row.output.0 == "out-a")
            .map(|row| row.id.clone())
            .expect("workspace");
        accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "win-a",
                Some(11),
                Some(WorldCommand::SetWorkspaceTiling {
                    output: OutputId("out-a".to_owned()),
                    workspace: ws.clone(),
                    tiled: false,
                }),
            ),
        );
        let mut minimized = fact("win-min", "out-a", 0);
        minimized.minimized = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), minimized],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-min")
            .expect("floating-scope minimized row");
        assert!(row.floating, "minimized occupant still float-marked");
        assert_eq!(row.float_origin, None, "empty diagnostic origin");
        assert_eq!(row.allocation, None, "no allocation on floats");
        assert_eq!(row.effective_frame, None, "no frame on floats");
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Hold(HoldReason::Minimized),
            "minimized hold unchanged"
        );
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Untouched,
            "membership stays untouched"
        );
        assert!(
            above_row(&reply, "win-min")
                .is_none_or(|row| row.authority != ScopeAuthority::Controlled),
            "minimized never acquires controlled above"
        );
    }

    #[test]
    fn near_bound_single_vacancy_mints_at_most_one_acquire() {
        use crate::world_report::{AttemptOutcome, StateProperty};
        use crate::world_state::MAX_RETAINED_STATE_BINDINGS;
        // One vacancy plus two newly eligible floats: the used-counter
        // gate mints at most one acquire and never exceeds the bound.
        // Dummy fillers carry no above_meta (manual settlement-era shape),
        // so they count toward the bound while staying inert; finish
        // prunes them, so the published mint count is the assertion.
        let mut engine = engine_above();
        accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(
            engine
                .retained_states
                .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Above))
        );
        let need = MAX_RETAINED_STATE_BINDINGS - engine.retained_states.len() - 1;
        for i in 0..need {
            engine.retained_states.insert(
                (WindowId(format!("win-fill-{i:05}")), StateProperty::Above),
                crate::world_state::RetainedStateBinding {
                    target: crate::world_state::RetainedStateTarget {
                        window: WindowId(format!("win-fill-{i:05}")),
                        property: StateProperty::Above,
                        value: true,
                        revision: 1,
                    },
                    pending: None,
                    spent: false,
                    disposition: AttemptOutcome::Unattempted,
                    sticky_meta: None,
                    fullscreen_meta: None,
                    maximize_meta: None,
                    above_meta: None,
                },
            );
        }
        assert_eq!(
            engine.retained_states.len(),
            MAX_RETAINED_STATE_BINDINGS - 1,
            "exactly one vacancy"
        );
        let reply = accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0)],
                vec![
                    float_fact("win-a", "out-a", 100, 100),
                    float_fact("win-b", "out-a", 200, 100),
                    float_fact("win-c", "out-a", 300, 100),
                ],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        assert!(
            engine.retained_states.len() <= MAX_RETAINED_STATE_BINDINGS,
            "never exceeds the bound"
        );
        let minted = ["win-b", "win-c"]
            .iter()
            .filter(|window| {
                above_row(&reply, window)
                    .is_some_and(|row| row.value && row.authority == ScopeAuthority::Controlled)
            })
            .count();
        assert!(
            minted <= 1,
            "one vacancy mints at most one acquire, minted {minted}"
        );
        let bound_new = ["win-b", "win-c"]
            .iter()
            .filter(|window| {
                engine
                    .retained_states
                    .contains_key(&(WindowId(window.to_string()), StateProperty::Above))
            })
            .count();
        assert!(
            bound_new <= 1,
            "at most one new binding past the vacancy, found {bound_new}"
        );
    }

    #[test]
    fn exact_cap_owned_release_replaces_without_growth() {
        use crate::world_report::{AttemptOutcome, StateProperty};
        use crate::world_state::MAX_RETAINED_STATE_BINDINGS;
        // Exact-cap owned release: a proven true that returns to tile
        // replaces its own key with false (no growth), so the full table
        // never blocks the owned transition.
        let mut engine = engine_above();
        let first = accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![float_fact("win-a", "out-a", 100, 100)],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        prove_above(&mut engine, 2, "win-a", first.state_revision, false);
        let fill = MAX_RETAINED_STATE_BINDINGS - engine.retained_states.len();
        for i in 0..fill {
            engine.retained_states.insert(
                (WindowId(format!("win-fill-{i:05}")), StateProperty::Above),
                crate::world_state::RetainedStateBinding {
                    target: crate::world_state::RetainedStateTarget {
                        window: WindowId(format!("win-fill-{i:05}")),
                        property: StateProperty::Above,
                        value: true,
                        revision: 1,
                    },
                    pending: None,
                    spent: false,
                    disposition: AttemptOutcome::Unattempted,
                    sticky_meta: None,
                    fullscreen_meta: None,
                    maximize_meta: None,
                    above_meta: None,
                },
            );
        }
        assert_eq!(
            engine.retained_states.len(),
            MAX_RETAINED_STATE_BINDINGS,
            "exact bound with the owned key present"
        );
        // Tile the window while native still holds the proven true.
        let mut tiled = fact("win-a", "out-a", 0);
        tiled.above = Some(true);
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0)],
                vec![tiled],
                vec![],
                "win-a",
                None,
                None,
            ),
        );
        let row = above_row(&reply, "win-a").expect("owned release replaces at cap");
        assert!(!row.value);
        assert_eq!(row.authority, ScopeAuthority::Controlled);
        assert_eq!(row.command_seq, None);
        assert!(
            engine.retained_states.len() <= MAX_RETAINED_STATE_BINDINGS,
            "replacement never grows past the bound"
        );
        assert!(
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Above))
                .is_some_and(|binding| !binding.target.value),
            "owned key now carries false"
        );
    }
}
