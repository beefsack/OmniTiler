//! Owned intent import, provenance, fence, and save/clear target substate
//! for [`crate::world_engine::WorldState`].

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::WindowId;
use crate::intent_data::{IntentData, decode_intent_data};
use crate::world_engine::ExcludedIntent;
use crate::world_report::{IntentRecord, MAX_INTENT_DESIRED, MAX_INTENT_RECORDS, WindowFact};

#[derive(Debug, Clone)]
pub(crate) struct IntentState {
    pub(crate) excluded_intent: BTreeMap<WindowId, ExcludedIntent>,
    pub(crate) pending_intent: BTreeMap<WindowId, IntentData>,
    pub(crate) intent_conflicts: BTreeSet<WindowId>,
    pub(crate) intent_admitted: BTreeSet<WindowId>,
    pub(crate) restored_floats: BTreeSet<WindowId>,
    pub(crate) restored_sticky: BTreeMap<WindowId, bool>,
    pub(crate) intent_desired: BTreeMap<WindowId, Option<IntentData>>,
    pub(crate) sticky_saved_incomplete: bool,
    pub(crate) sticky_visible_rev: BTreeMap<WindowId, u64>,
    pub(crate) intent_sticky_blocks: BTreeSet<WindowId>,
    pub(crate) intent_sticky_saturated: bool,
    #[cfg(test)]
    pub(crate) ingest_calls: u64,
    #[cfg(test)]
    pub(crate) ingest_trials: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Default for IntentState {
    fn default() -> Self {
        Self::new()
    }
}

impl IntentState {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            excluded_intent: BTreeMap::new(),
            pending_intent: BTreeMap::new(),
            intent_conflicts: BTreeSet::new(),
            intent_admitted: BTreeSet::new(),
            restored_floats: BTreeSet::new(),
            restored_sticky: BTreeMap::new(),
            intent_desired: BTreeMap::new(),
            sticky_saved_incomplete: false,
            sticky_visible_rev: BTreeMap::new(),
            intent_sticky_blocks: BTreeSet::new(),
            intent_sticky_saturated: false,
            #[cfg(test)]
            ingest_calls: 0,
            #[cfg(test)]
            ingest_trials: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Joint intent ledger occupancy: pending restores plus conflict marks
    /// plus sticky save blocks plus retained sticky-prior provenance share
    /// the single [`MAX_INTENT_RECORDS`] cap, so every insert branch (and the
    /// sticky-block insert) must gate on this, never on a partial sum. A
    /// pending-to-retained sticky commit removes the pending entry first, so
    /// it holds the joint count steady.
    #[must_use]
    pub(crate) fn ledger_used(&self) -> usize {
        self.pending_intent.len()
            + self.intent_conflicts.len()
            + self.intent_sticky_blocks.len()
            + self.restored_sticky.len()
    }

    /// Record one lifetime-scoped save block for deferred sticky intent.
    /// Jointly bounded with pending/conflicts by [`MAX_INTENT_RECORDS`]: when
    /// the bound leaves no room, the generation-scoped saturation latch trips
    /// instead and every save is suppressed until handshake reset.
    pub(crate) fn insert_sticky_block(&mut self, win: &WindowId) {
        if self.intent_sticky_blocks.contains(win) {
            return;
        }
        if self.ledger_used() < MAX_INTENT_RECORDS {
            self.intent_sticky_blocks.insert(win.clone());
        } else {
            self.intent_sticky_saturated = true;
        }
    }

    /// First-accepted retained desire initializer: inserts only when no
    /// retained target exists (no late clobber of runtime intent). A cap
    /// drop latches `sticky_saved_incomplete` fail-closed instead of silently
    /// losing the exact prior.
    pub(crate) fn remember_first_sticky_saved(&mut self, win: &WindowId, data: IntentData) {
        if self.intent_desired.contains_key(win) {
            return;
        }
        if self.intent_desired.len() < MAX_INTENT_DESIRED {
            self.intent_desired.insert(win.clone(), Some(data));
        } else {
            self.sticky_saved_incomplete = true;
        }
    }

    /// Retained desire upsert for accepted runtime mutations, in execution
    /// order (latest-wins per window).
    pub(crate) fn upsert_retained_desire(&mut self, win: &WindowId, data: Option<IntentData>) {
        if self.intent_desired.contains_key(win) {
            self.intent_desired.insert(win.clone(), data);
            return;
        }
        if self.intent_desired.len() < MAX_INTENT_DESIRED {
            self.intent_desired.insert(win.clone(), data);
        } else {
            self.sticky_saved_incomplete = true;
        }
    }

    /// Conflicting imports invalidate any first-cached exact prior: drop
    /// the bogus retained target and latch incomplete fail-closed.
    pub(crate) fn drop_saved_as_conflicting(&mut self, win: &WindowId) {
        self.intent_desired.remove(win);
        self.sticky_saved_incomplete = true;
    }

    pub(crate) fn ingest_rows(
        &mut self,
        own_pid: Option<&str>,
        closed_ids: &[WindowId],
        raw_windows: &[WindowFact],
        intent_records: &[IntentRecord],
    ) {
        #[cfg(test)]
        {
            self.ingest_calls = self.ingest_calls.saturating_add(1);
            self.ingest_trials
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let closed: BTreeSet<&WindowId> = closed_ids.iter().collect();
        for id in closed_ids {
            self.pending_intent.remove(id);
            self.intent_conflicts.remove(id);
            self.intent_admitted.remove(id);
            self.restored_floats.remove(id);
            self.restored_sticky.remove(id);
            self.intent_sticky_blocks.remove(id);
            self.intent_desired.remove(id);
            self.sticky_visible_rev.remove(id);
        }
        let mut own_proven: BTreeSet<WindowId> = BTreeSet::new();
        if let Some(own) = own_pid {
            for fact in raw_windows {
                if fact.process_id.as_deref() == Some(own) {
                    own_proven.insert(fact.window.clone());
                }
            }
        }
        for id in &own_proven {
            self.pending_intent.remove(id);
            self.intent_conflicts.remove(id);
            self.restored_floats.remove(id);
            self.restored_sticky.remove(id);
            self.intent_sticky_blocks.remove(id);
            self.intent_desired.remove(id);
            self.sticky_visible_rev.remove(id);
        }
        let mut grouped: BTreeMap<&WindowId, BTreeSet<&str>> = BTreeMap::new();
        for record in intent_records {
            if closed.contains(&record.win) || own_proven.contains(&record.win) {
                continue;
            }
            grouped
                .entry(&record.win)
                .or_default()
                .insert(record.data.as_str());
        }
        for (win, raws) in grouped {
            if self.intent_admitted.contains(win) {
                continue;
            }
            if raws.iter().any(|raw| {
                matches!(
                    decode_intent_data(raw),
                    Ok(data) if data.sticky_prior.is_some()
                )
            }) {
                self.insert_sticky_block(win);
            }
            if self.intent_conflicts.contains(win) {
                continue;
            }
            if raws.len() > 1 {
                self.pending_intent.remove(win);
                self.drop_saved_as_conflicting(win);
                if self.ledger_used() < MAX_INTENT_RECORDS {
                    self.intent_conflicts.insert(win.clone());
                }
                continue;
            }
            let raw = raws.iter().next().expect("grouped raw");
            let decoded = match decode_intent_data(raw) {
                Ok(data) => data,
                Err(_) => continue,
            };
            let decoded_null = decoded.float.is_none() && decoded.sticky_prior.is_none();
            let saved_boundary_conflict = self
                .intent_desired
                .get(win)
                .and_then(|cached| *cached)
                .is_some_and(|cached| {
                    cached != decoded
                        && (decoded_null
                            || (cached.float.is_none() && cached.sticky_prior.is_none()))
                });
            if saved_boundary_conflict {
                self.drop_saved_as_conflicting(win);
            }
            if decoded_null {
                if !saved_boundary_conflict {
                    self.remember_first_sticky_saved(win, decoded);
                }
                continue;
            }
            match self.pending_intent.get(win).copied() {
                Some(existing) if existing == decoded => {
                    if !saved_boundary_conflict {
                        self.remember_first_sticky_saved(win, decoded);
                    }
                }
                Some(_) => {
                    self.pending_intent.remove(win);
                    self.drop_saved_as_conflicting(win);
                    if self.ledger_used() < MAX_INTENT_RECORDS {
                        self.intent_conflicts.insert(win.clone());
                    }
                }
                None => {
                    if self.ledger_used() < MAX_INTENT_RECORDS {
                        self.pending_intent.insert(win.clone(), decoded);
                        if !saved_boundary_conflict {
                            self.remember_first_sticky_saved(win, decoded);
                        }
                    } else {
                        self.sticky_saved_incomplete = true;
                        self.remember_first_sticky_saved(win, decoded);
                    }
                }
            }
        }
    }

    pub(crate) fn retain_pre_ready_complete(
        &mut self,
        live: &BTreeSet<WindowId>,
        held: &BTreeSet<WindowId>,
    ) {
        self.pending_intent
            .retain(|id, _| live.contains(id) || held.contains(id));
        self.intent_conflicts
            .retain(|id| live.contains(id) || held.contains(id));
        self.restored_floats
            .retain(|id| live.contains(id) || held.contains(id));
        self.restored_sticky
            .retain(|id, _| live.contains(id) || held.contains(id));
        self.intent_admitted
            .retain(|id| live.contains(id) || held.contains(id));
        self.intent_sticky_blocks
            .retain(|id| live.contains(id) || held.contains(id));
        self.intent_desired
            .retain(|id, _| live.contains(id) || held.contains(id));
    }
}
