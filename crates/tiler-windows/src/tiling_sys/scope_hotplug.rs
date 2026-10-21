#![forbid(unsafe_code)]
//! Mode-aware monitor topology sync (world-owned, adapter stub).
//!
//! Hotplug displacement, return mapping, and per-mode history/trailing are
//! core policy now: screens plus window facts are the only observation
//! prerequisites and the core mints and owns its logical workspaces from
//! them. The adapter only syncs its mechanical device-to-wire output map
//! plus the known-geometry/active-output bookkeeping below. No Engine
//! relocation, no history, no trailing.

use super::{MonitorArea, TileLoop};

/// Mechanical topology sync: assign stable wire ids to live devices, retain
/// deferred no-survivor origins with their last geometry, and keep the
/// cached output context coherent. Never policy.
pub(super) fn sync_monitor_outputs(state: &mut TileLoop, areas: &[MonitorArea]) {
    let current: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
    // Stable wire-id assignment: live devices keep their ids, genuinely new
    // devices mint the next free `oN`. Ids are never reused or reordered, so
    // the core sees a stable output identity across topology churn.
    let mut next: u32 = 1;
    for id in state.output_tokens.values() {
        if let Some(n) = id.strip_prefix('o').and_then(|n| n.parse::<u32>().ok()) {
            next = next.max(n.saturating_add(1));
        }
    }
    for key in &current {
        if state.output_tokens.contains_key(key) {
            continue;
        }
        while state.output_tokens.values().any(|id| id == &format!("o{next}")) {
            next = next.saturating_add(1);
        }
        state
            .output_tokens
            .insert(key.clone(), format!("o{next}"));
        next = next.saturating_add(1);
    }
    sync_tail(state, areas, &current, &[]);
}

/// Shared tail: deferred no-survivor origins stay known with their last
/// geometry until a survivor exists.
fn sync_tail(state: &mut TileLoop, areas: &[MonitorArea], current: &[String], deferred: &[String]) {
    let mut known = current.to_vec();
    for origin in deferred {
        if !known.contains(origin) {
            known.push(origin.clone());
        }
    }
    state.known_outputs = known;
    let mut last = areas.to_vec();
    for origin in deferred {
        if !last.iter().any(|a| &a.device == origin) {
            if let Some(area) = state
                .last_areas
                .iter()
                .find(|a| &a.device == origin)
                .cloned()
            {
                last.push(area);
            }
        }
    }
    state.last_areas = last;
    if state.active_output.is_empty() {
        if let Some(first) = state.known_outputs.first().cloned() {
            state.active_output = first;
        }
    }
}
