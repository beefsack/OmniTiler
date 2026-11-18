//! Intent-lifecycle composition on the world transaction: gesture replay,
//! intent ingress/commit, exclusion capture, and command bookkeeping.

use crate::directional::WindowId;
use crate::gesture::GestureSettled;
use crate::world_engine::ExcludedIntent;
use crate::world_report::WorldCommand;
use crate::world_staged::{RawRouting, StagedObservation};
use crate::world_transaction::WorldTransaction;

impl WorldTransaction<'_> {
    pub(crate) fn consume_gesture_id(&mut self, id: u64) {
        self.world.gesture.consumed_seq = self.world.gesture.consumed_seq.max(id);
    }

    pub(crate) fn record_gesture_terminal(&mut self, id: u64, committed: bool) {
        if self.gesture_replayed(id) {
            return;
        }
        self.consume_gesture_id(id);
        if self
            .world
            .gesture
            .retained
            .as_ref()
            .is_some_and(|retained| id < retained.id)
        {
            return;
        }
        self.world.gesture.terminal = Some(GestureSettled { id, committed });
    }

    pub(crate) fn gesture_replayed(&self, id: u64) -> bool {
        id <= self.world.gesture.consumed_seq
    }

    pub(crate) fn fresh_placement_armed_for(&self, ctx: &RawRouting<'_>) -> bool {
        self.world.placement_bootstrapped && ctx.inventory_complete
    }

    pub(crate) fn intent_ledger_used(&self) -> usize {
        self.world.intent.ledger_used()
    }

    pub(crate) fn upsert_retained_intent_desire(
        &mut self,
        win: &WindowId,
        data: Option<crate::intent_data::IntentData>,
    ) {
        self.world.intent.upsert_retained_desire(win, data);
    }

    /// Ingest opaque intent rows from a staged observation capture, before
    /// classification, so the classifier sees pending first-admission wins
    /// from this same report. Reads only the capture's closed subjects,
    /// raw window facts (for own-process pruning), and intent records.
    /// Initial intakes call this exactly once per report before staging;
    /// staging and command restages never ingest.
    pub(crate) fn ingest_staged_intent(&mut self, observation: &StagedObservation) {
        self.ingest_intent_rows(
            &observation.closed,
            &observation.raw_windows,
            &observation.intent_records,
        );
    }

    pub(crate) fn ingest_intent_rows(
        &mut self,
        closed_ids: &[WindowId],
        raw_windows: &[crate::world_report::WindowFact],
        intent_records: &[crate::world_report::IntentRecord],
    ) {
        let own_pid = self
            .connection
            .application_identity
            .as_ref()
            .and_then(|identity| identity.process_id.as_deref());
        self.world
            .intent
            .ingest_rows(own_pid, closed_ids, raw_windows, intent_records);
    }

    /// Latch first successful admissions, commit durable intent, and keep
    /// the exclusion ledger truthful to proven session state. Called after
    /// each sweep (and resweep). Pending drops only on real success
    /// (tiled for `float:false`, non-automatic exception for
    /// `float:true`); minimized placeholders and holds retain pending.
    /// Sessionless floating-route admissions latch separately after
    /// membership converges (see `latch_floating_admissions`): settle
    /// runs before reconcile creates those members.
    /// The exclusion ledger syncs to current session truth for live
    /// sessions only, so a successful runtime float clears a stale tile
    /// win while exclusion gaps preserve continuity.
    pub(crate) fn settle_intent_admissions(&mut self) {
        // Own-process facts staged this report prove no import authority:
        // drop pending/conflict/restored for proven-own subjects.
        // Safety-held windows never match here and stay recoverable.
        let own_excluded: Vec<WindowId> = self
            .world
            .staged
            .derived
            .iter()
            .filter_map(|(id, classified)| {
                if classified.own_excluded {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        for id in own_excluded {
            self.world.intent.pending_intent.remove(&id);
            self.world.intent.intent_conflicts.remove(&id);
            self.world.intent.restored_floats.remove(&id);
            self.world.intent.restored_sticky.remove(&id);
            self.world.intent.intent_sticky_blocks.remove(&id);
            self.world.intent.intent_desired.remove(&id);
            self.world.intent.sticky_visible_rev.remove(&id);
        }
        // Latch first successful admissions: late rows for already
        // admitted lifetimes are ignored forever, even through
        // exclusion/holds/minimize. Session tiled links and
        // floating-base session exceptions are real success;
        // overlay-only placeholders (non-floating fullscreen/maximized
        // births) and minimized slots alone never latch. Membership
        // alone never proves admission either: reconcile homes rows
        // even when held, so members latch only on staged health.
        // Pending-only latching would miss record-free first admissions.
        let mut known: Vec<WindowId> = Vec::new();
        known.extend(self.world.layout.all_tiled_windows());
        for (window, flags) in self.world.layout.all_exception_windows_with_flags() {
            if !flags.floating && (flags.fullscreen || flags.maximized) {
                continue;
            }
            known.push(window);
        }
        let member_windows: Vec<WindowId> = self.world.workspace.members.keys().cloned().collect();
        for window in member_windows {
            if self.world.staged.excluded.contains(&window) {
                continue;
            }
            if self
                .world
                .staged
                .held_windows
                .iter()
                .any(|held| held.window == window)
            {
                continue;
            }
            if !self.world.staged.facts.contains_key(&window) {
                continue;
            }
            let derived = self.derived_for(&window);
            if derived.hold || derived.excluded || derived.own_excluded || derived.hidden {
                continue;
            }
            if !derived.floating && (derived.overlay_fullscreen || derived.overlay_maximized) {
                continue;
            }
            known.push(window);
        }
        for win in &known {
            self.world.intent.intent_admitted.insert(win.clone());
        }
        // Conflicts confer no winner: drop them once the lifetime admits
        // instead of consuming the joint bound forever.
        for win in &known {
            self.world.intent.intent_conflicts.remove(win);
        }
        let pending: Vec<(WindowId, crate::intent_data::IntentData)> = self
            .world
            .intent
            .pending_intent
            .iter()
            .map(|(id, data)| (id.clone(), *data))
            .collect();
        for (win, data) in pending {
            // Sticky-bearing restores commit only the exact prior into
            // sticky provenance, never a tile override or ordinary float:
            // sticky dominates the float lane. Real float success is a
            // healthy staged non-automatic exception (never a merely homed
            // held/hidden/minimized/overlay placeholder); pending survives
            // otherwise, exactly like the float lane below.
            if let Some(prior) = data.sticky_prior {
                let healthy = self.world.staged.facts.contains_key(&win)
                    && !self.world.staged.excluded.contains(&win)
                    && !self
                        .world
                        .staged
                        .held_windows
                        .iter()
                        .any(|held| held.window == win)
                    && {
                        let derived = self.derived_for(&win);
                        !derived.hold
                            && !derived.excluded
                            && !derived.own_excluded
                            && !derived.hidden
                            && (derived.floating
                                || !(derived.overlay_fullscreen || derived.overlay_maximized))
                    };
                if !healthy {
                    continue;
                }
                if !self.world.is_exception_window_any(&win)
                    || self.world.layout.is_automatic_fixed_float_any(&win)
                {
                    continue;
                }
                // Pending leaves first so the retained commit holds the
                // joint ledger count steady.
                self.world.intent.pending_intent.remove(&win);
                self.world.intent.restored_sticky.insert(win.clone(), prior);
                self.ensure_restored_sticky_binding(&win, prior);
                // Composed restore initializes the stable visibility
                // revision absent-only: first safe admission authorizes,
                // ordinary observations never renew.
                if self.sticky_route() == crate::world_sticky::StickyRoute::Composed
                    && !self.world.intent.sticky_visible_rev.contains_key(&win)
                {
                    self.world
                        .intent
                        .sticky_visible_rev
                        .insert(win, self.world.state_revision + 1);
                }
            } else if data.float == Some(false) {
                // Real tile success is a tiled link, not a minimized
                // placeholder or an exception: retain pending otherwise.
                if !self.world.is_tiled_window_any(&win) {
                    continue;
                }
                self.world.layout.commit_fixed_tile_override(&win);
                self.world.intent.pending_intent.remove(&win);
            } else if data.float == Some(true) {
                // Real float success is a non-automatic exception: retain
                // pending through minimized placeholders, and never
                // promote automatic floats to explicit.
                if !self.world.is_exception_window_any(&win)
                    || self.world.layout.is_automatic_fixed_float_any(&win)
                {
                    continue;
                }
                self.world.intent.restored_floats.insert(win.clone());
                self.world.intent.pending_intent.remove(&win);
            }
        }
        // Exclusion-ledger truth sync for live layout domains only: current
        // layout marks win over stale mirrors (e.g. a successful runtime
        // float clears a restored tile win), while exclusion gaps with no
        // live domain preserve continuity untouched.
        let mut live: Vec<WindowId> = Vec::new();
        live.extend(self.world.layout.all_tiled_windows());
        live.extend(self.world.layout.all_exception_windows());
        live.extend(self.world.layout.all_minimized_windows());
        live.sort();
        live.dedup();
        for win in live {
            let tile = self.world.layout.has_fixed_tile_override_any(&win);
            let automatic = self.world.layout.is_automatic_fixed_float_any(&win);
            if tile || automatic {
                self.world.intent.excluded_intent.insert(
                    win,
                    ExcludedIntent {
                        tile_override: tile,
                        automatic_fixed: automatic,
                    },
                );
            } else {
                self.world.intent.excluded_intent.remove(&win);
            }
        }
        // Runtime unfloat wins over restored float provenance: a lifetime
        // the layout now proves tiled leaves the explicit-float set.
        // Unknown/excluded lifetimes keep the mark for recovery.
        let floated_marked: Vec<WindowId> =
            self.world.intent.restored_floats.iter().cloned().collect();
        for win in floated_marked {
            if self.world.is_tiled_window_any(&win) {
                self.world.intent.restored_floats.remove(&win);
            }
        }
    }

    /// Latch healthy floating-route first admissions after membership
    /// converges. Floating workspaces release their sessions, so a
    /// floating newcomer never appears in session links at settle time
    /// and only gains membership at reconcile (after settle). Health is
    /// staged truth, never membership alone: held, hidden, excluded,
    /// or overlay-only placeholders never latch. A pending true
    /// consumes into explicit-float provenance at the observed frame
    /// (no geometry op); a pending sticky-bearing record consumes only
    /// its exact prior into sticky provenance (never ordinary float, never
    /// a tile win); a pending false stays pending as the explicit
    /// tile base for a later tiling enable (never forced tiled now).
    pub(crate) fn latch_floating_admissions(&mut self) {
        let candidates: Vec<(WindowId, bool)> = self
            .world
            .workspace
            .members
            .iter()
            .filter(|(_, state)| self.world.floating_scopes.contains(&state.home))
            .filter(|(id, _)| !self.world.intent.intent_admitted.contains(*id))
            .map(|(id, _)| {
                let healthy = self.world.staged.facts.contains_key(id)
                    && !self.world.staged.excluded.contains(id)
                    && !self
                        .world
                        .staged
                        .held_windows
                        .iter()
                        .any(|held| &held.window == id)
                    && {
                        let derived = self.derived_for(id);
                        !derived.hold
                            && !derived.excluded
                            && !derived.own_excluded
                            && !derived.hidden
                            && (derived.floating
                                || !(derived.overlay_fullscreen || derived.overlay_maximized))
                    };
                (id.clone(), healthy)
            })
            .collect();
        for (win, healthy) in candidates {
            if !healthy {
                continue;
            }
            self.world.intent.intent_admitted.insert(win.clone());
            self.world.intent.intent_conflicts.remove(&win);
            if let Some(prior) = self
                .world
                .intent
                .pending_intent
                .get(&win)
                .copied()
                .and_then(|data| data.sticky_prior)
            {
                // Pending leaves first so the retained commit holds the
                // joint ledger count steady.
                self.world.intent.pending_intent.remove(&win);
                self.world.intent.restored_sticky.insert(win.clone(), prior);
                self.ensure_restored_sticky_binding(&win, prior);
                // Composed floating restore initializes the stable
                // visibility revision absent-only, same as the session
                // route above.
                if self.sticky_route() == crate::world_sticky::StickyRoute::Composed
                    && !self.world.intent.sticky_visible_rev.contains_key(&win)
                {
                    self.world
                        .intent
                        .sticky_visible_rev
                        .insert(win, self.world.state_revision + 1);
                }
            } else if self
                .world
                .intent
                .pending_intent
                .get(&win)
                .is_some_and(|data| data.float == Some(true))
            {
                self.world.intent.restored_floats.insert(win.clone());
                self.world.intent.pending_intent.remove(&win);
            }
        }
    }

    /// Resolve one command to its post-command bookkeeping identity: an
    /// implicit (execution-time-subject) command maps to its explicit form
    /// against the resolved subject; explicit commands clone unchanged. An
    /// unresolvable implicit falls back to the original placeholder, which
    /// matches no save arm (preserving mismatch/empty semantics). Call
    /// BEFORE execution: the subject is the pre-execution slot, never
    /// focus after the action.
    pub(crate) fn bookkeeping_command(&self, command: &WorldCommand) -> WorldCommand {
        match command {
            WorldCommand::Implicit(inner) => self
                .execution_subject()
                .and_then(|window| crate::world_report::explicit_for_implicit(inner, window))
                .unwrap_or_else(|| command.clone()),
            _ => command.clone(),
        }
    }

    /// Synchronize explicit-float provenance with a successful runtime
    /// float toggle: a toggled-to-float non-automatic exception gains the
    /// durable mark, a toggled-to-tile lifetime loses it. Unknown or held
    /// subjects keep their mark. Only explicit command wins enter; generic
    /// automatic/capacity floats never do.
    pub(crate) fn sync_toggle_float_command(&mut self, command: &WorldCommand) {
        let window = match crate::world_report::unwrap_implicit(command) {
            WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
                window, ..
            }) => WindowId(window.clone()),
            _ => return,
        };
        if self.world.is_tiled_window_any(&window) {
            self.world.intent.restored_floats.remove(&window);
        } else if self.world.is_exception_window_any(&window)
            && !self.world.layout.is_automatic_fixed_float_any(&window)
        {
            self.world.intent.restored_floats.insert(window);
        }
    }

    pub(crate) fn capture_excluded_intent(&mut self) {
        // Snapshot lifetime intent before the sweep retires excluded
        // slots: an exclusion is never a native close, so the approved
        // fixed tile override and automatic fixed mark survive the
        // same-lifetime gap. Only records proven intent: never
        // overwrites a retained win with a retired-session false.
        for id in self.world.staged.excluded.clone() {
            let tile = self.world.layout.has_fixed_tile_override_any(&id);
            let automatic = self.world.layout.is_automatic_fixed_float_any(&id);
            if tile || automatic {
                self.world.intent.excluded_intent.insert(
                    id,
                    ExcludedIntent {
                        tile_override: tile,
                        automatic_fixed: automatic,
                    },
                );
            }
        }
        // Explicit native closes end the lifetime: never resurrect intent
        // for a reused id.
        for id in self.world.staged.closes.clone() {
            self.world.intent.excluded_intent.remove(&id);
        }
    }
}
