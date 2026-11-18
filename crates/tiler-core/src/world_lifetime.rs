//! Lifetime cleanup: prune retained ledgers and settled outcomes against the
//! current staged observation capture.

use std::collections::BTreeSet;

use crate::directional::{OutputId, WindowId};
use crate::world_diagnostics::core_trace_enabled;
use crate::world_transaction::WorldTransaction;

impl WorldTransaction<'_> {
    /// Prune every retained-authority lifetime ledger against the current
    /// staged capture: canonical memory, exclusion/intent ledgers, the
    /// state/focus/selection lane pruners, and lifetime-local composed
    /// maximize. Lifetime presence reads the staged raw observation plus
    /// the staged held rows, never the classified facts alone: only
    /// own-process subjects are dropped from classification, while
    /// safety-excluded facts stay classified but ineligible; both stay
    /// lifetime evidence, and capability-held rows keep presence without
    /// authority. A complete report drops every dead identity; an
    /// incomplete one only drops explicit closes. Closed ids never survive
    /// even under reuse.
    pub(crate) fn prune_lifetime_state(&mut self) {
        // Canonical memory prunes to the live inventory: a complete report
        // drops every dead identity, an incomplete one only drops explicit
        // closes. Closed ids never survive even under reuse.
        // Owned narrow snapshots: the staged capture is re-read per region
        // below so no observation borrow crosses the lane-pruner calls.
        // Narrow sets stay scoped to the branch that needs them: incomplete
        // gaps never clone the raw inventory.
        let inventory_complete = self.world.staged.observation.inventory_complete;
        if inventory_complete {
            let live: BTreeSet<WindowId> = self
                .world
                .staged
                .observation
                .raw_windows
                .iter()
                .map(|fact| fact.window.clone())
                .collect();
            let held: BTreeSet<WindowId> = self
                .world
                .staged
                .held_windows
                .iter()
                .map(|held| held.window.clone())
                .collect();
            self.world
                .focus
                .last_active_window
                .retain(|_, window| live.contains(window));
            // Same lifetime bound for the exclusion ledger: a complete
            // inventory that carries neither the fact nor the exclusion
            // proves departure, so drop the retained intent. Explicit
            // closes already cleared at capture time.
            let excluded = self.world.staged.excluded.clone();
            self.world
                .intent
                .excluded_intent
                .retain(|id, _| live.contains(id) || excluded.contains(id));
            // Pending/conflict ledgers share the proven-departure
            // fence: a complete inventory carrying neither a usable
            // fact, a held row, nor an exclusion proves the subject
            // departed before first success, so drop it. A carried
            // slot row alone is not live proof: it never retains.
            // Engine-known (admitted, minimized, exception, member)
            // subjects stay: complete absence after admission is
            // handled through the admitted latch and close path, never
            // by resurrecting pending. Closed subjects were already
            // dropped on ingest.
            let known: BTreeSet<WindowId> = self
                .world
                .intent
                .pending_intent
                .keys()
                .chain(self.world.intent.intent_conflicts.iter())
                .chain(self.world.intent.intent_admitted.iter())
                .chain(self.world.intent.restored_floats.iter())
                .chain(self.world.intent.restored_sticky.keys())
                .chain(self.world.intent.intent_desired.keys())
                .filter(|id| {
                    self.world.engine_knows_window_any(id)
                        || self.world.workspace.members.contains_key(*id)
                })
                .cloned()
                .collect();
            self.world.intent.pending_intent.retain(|id, _| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.world.intent.intent_conflicts.retain(|id| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.world.intent.restored_floats.retain(|id| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.world.intent.restored_sticky.retain(|id, _| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            // Admitted lifetimes end on proven complete departure
            // (neither live, held, excluded, nor engine/member-known).
            // Temporary exclusion/holds keep the latch so late rows
            // stay ignored.
            self.world.intent.intent_admitted.retain(|id| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.world.intent.intent_sticky_blocks.retain(|id| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.world.intent.intent_desired.retain(|id, _| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.world.intent.sticky_visible_rev.retain(|id, _| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
        } else if !self.world.staged.observation.closed.is_empty() {
            let closed: BTreeSet<WindowId> = self
                .world
                .staged
                .observation
                .closed
                .iter()
                .cloned()
                .collect();
            self.world
                .focus
                .last_active_window
                .retain(|_, window| !closed.contains(window));
            self.world
                .intent
                .excluded_intent
                .retain(|id, _| !closed.contains(id));
        }
        self.prune_state_bindings();
        self.prune_selection_grants();
        self.prune_selection_scopes();
        self.prune_selection_targets();
        self.prune_automatic_focus_tokens();
        // Lifetime-local composed maximize follows the live inventory:
        // a complete report drops dead identities (engine/member-known
        // lifetimes stay like the state bindings above), explicit closes
        // always prune, and restart drops the bit with the engine.
        if !self.world.staged.observation.closed.is_empty() {
            let closed: BTreeSet<WindowId> = self
                .world
                .staged
                .observation
                .closed
                .iter()
                .cloned()
                .collect();
            self.world
                .runtime
                .composed_maximized
                .retain(|window| !closed.contains(window));
        }
        if inventory_complete {
            let live_ref: BTreeSet<&WindowId> = self
                .world
                .staged
                .observation
                .raw_windows
                .iter()
                .map(|fact| &fact.window)
                .collect();
            let held_ref: BTreeSet<&WindowId> = self
                .world
                .staged
                .held_windows
                .iter()
                .map(|held| &held.window)
                .collect();
            let excluded = self.world.staged.excluded.clone();
            let known: BTreeSet<WindowId> = self
                .world
                .runtime
                .composed_maximized
                .iter()
                .filter(|window| {
                    self.world.engine_knows_window_any(window)
                        || self.world.workspace.members.contains_key(*window)
                })
                .cloned()
                .collect();
            self.world.runtime.composed_maximized.retain(|window| {
                live_ref.contains(window)
                    || held_ref.contains(window)
                    || excluded.contains(window)
                    || known.contains(window)
            });
        }
    }

    /// Prune retained `operation_settled` and `switch_settled` outcomes when
    /// their owner departs (Q2 A): runs before `publish_v4` projects the
    /// reply so departed/closed owners' final outcomes are never published on
    /// the wire. Buffers final dropped outcomes only when `OMNITILER_TRACE=1`
    /// (`core_trace_enabled()`) so they emit solely after outbound gates
    /// succeed at actual publication/adoption. Held, minimized, and incomplete
    /// omissions retain their owners; `gesture_terminal` is generation-scoped
    /// and is never pruned here.
    pub(crate) fn prune_settled_outcomes(&mut self) {
        let trace = core_trace_enabled();
        if !self.world.staged.observation.closed.is_empty()
            && !self.world.settlement.operation_settled.is_empty()
        {
            let closed: BTreeSet<WindowId> = self
                .world
                .staged
                .observation
                .closed
                .iter()
                .cloned()
                .collect();
            let dropped_ops = &mut self.world.settlement.staged_dropped_operations;
            self.world
                .settlement
                .operation_settled
                .retain(|key, event| {
                    if closed.contains(&key.0) {
                        if trace
                            && (dropped_ops.len() < crate::world_report::MAX_REPLY_OPERATIONS
                                || dropped_ops.contains_key(key))
                        {
                            dropped_ops.insert(key.clone(), event.clone());
                        }
                        false
                    } else {
                        true
                    }
                });
        }
        if !self.world.staged.observation.inventory_complete {
            return;
        }
        if !self.world.settlement.operation_settled.is_empty() {
            let live: BTreeSet<&WindowId> = self
                .world
                .staged
                .observation
                .raw_windows
                .iter()
                .map(|fact| &fact.window)
                .collect();
            let held: BTreeSet<&WindowId> = self
                .world
                .staged
                .held_windows
                .iter()
                .map(|held| &held.window)
                .collect();
            let excluded = &self.world.staged.excluded;
            let known: BTreeSet<WindowId> = self
                .world
                .settlement
                .operation_settled
                .keys()
                .map(|(win, _)| win)
                .filter(|win| self.world.engine_knows_window_any(win))
                .cloned()
                .collect();
            let dropped_ops = &mut self.world.settlement.staged_dropped_operations;
            self.world
                .settlement
                .operation_settled
                .retain(|key, event| {
                    let win = &key.0;
                    let keep = live.contains(win)
                        || held.contains(win)
                        || excluded.contains(win)
                        || known.contains(win);
                    if !keep
                        && trace
                        && (dropped_ops.len() < crate::world_report::MAX_REPLY_OPERATIONS
                            || dropped_ops.contains_key(key))
                    {
                        dropped_ops.insert(key.clone(), event.clone());
                    }
                    keep
                });
        }
        if !self.world.settlement.switch_settled.is_empty() {
            let live_outputs: BTreeSet<OutputId> = self
                .world
                .staged
                .screens
                .iter()
                .map(|screen| screen.output.clone())
                .chain(
                    self.world
                        .staged
                        .held_screens
                        .iter()
                        .map(|held| held.output.clone()),
                )
                .collect();
            let dropped_switches = &mut self.world.settlement.staged_dropped_switches;
            self.world
                .settlement
                .switch_settled
                .retain(|output, event| {
                    let keep = live_outputs.contains(output);
                    if !keep
                        && trace
                        && (dropped_switches.len() < crate::world_report::MAX_REPLY_SWITCH_EVENTS
                            || dropped_switches.contains_key(output))
                    {
                        dropped_switches.insert(output.clone(), event.clone());
                    }
                    keep
                });
        }
    }
}
