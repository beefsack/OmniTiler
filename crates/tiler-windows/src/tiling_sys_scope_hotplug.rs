//! Mode-aware monitor topology sync (item 12 stage C).
//!
//! Dispatcher plus the non-local arms for `sync_monitor_outputs`: the
//! runtime call site stays a one-line delegation. Local mode keeps the exact
//! legacy body ([`super::sync_monitor_outputs_local`]); the global-unique
//! arm reuses the same displacement mechanics but records into the scoped
//! [`crate::workspace_scope_owner`] history and converges scoped trailing
//! afterwards; the shared arm performs no project displacement or return
//! mapping (KDE parity: `reconcileOutputDisplacement` returns early for
//! shared mode).
//!
//! Tentative shared semantics (pending user review, mirroring the existing
//! KDE choice): disconnect drops the removed output's entry and re-homes
//! same-logical-id members onto the surviving destination, with no displaced
//! record and no Engine-session relocation, so sibling output-bound layout
//! domains are never clobbered. Reconnect re-converges the returning origin
//! onto the live shared order and shows the survivor's current view without
//! consulting history. Foreground-driven view switching is local/global
//! only; shared survivors always keep their view on topology signals.
//! Unsupported R-WS-17 outcomes stay TBD for physical testing.

use std::collections::BTreeSet;

use tiler_core::session::DomainKey;
use tiler_core::workspace_scope::WorkspaceMode;
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use super::{MonitorArea, OUTER_GAP, TileLoop};
use crate::workspace_scope_owner::mapping_from_workspaces;

/// Mode-aware topology sync: local keeps legacy behavior, global-unique
/// adds scoped history/trailing, shared skips displacement (KDE parity).
/// The local/global runtime paths read the live foreground window.
pub(super) fn sync_monitor_outputs(state: &mut TileLoop, areas: &[MonitorArea]) {
    match state.scope_owner.adopted() {
        WorkspaceMode::PerOutputLocal => super::sync_monitor_outputs_local(state, areas),
        WorkspaceMode::GlobalUnique => {
            let fg = unsafe { GetForegroundWindow() } as usize as u64;
            let loc = foreground_loc(state, fg);
            sync_global_with_foreground(state, areas, loc);
        }
        WorkspaceMode::Shared => sync_shared(state, areas),
    }
}

/// Foreground member location behind the visible-lifetime gate: the live
/// member tag must equal the stored admission tag, so a same-process HWND
/// reuse racing the topology signal never activates the wrong workspace.
/// The native member-tag read and lifetime gate stay live-only.
fn foreground_loc(state: &TileLoop, fg: u64) -> Option<(String, String)> {
    let fg_tag = crate::product_hide::sys::read_member_tag(fg);
    state
        .member_tokens
        .iter()
        .find(|(k, _)| {
            k.hwnd == fg
                && state.member_tags.get(*k).is_some_and(|stored| {
                    crate::workspace_owner::visible_lifetime_ok(stored, fg_tag.as_deref())
                })
        })
        .and_then(|(k, _)| state.workspaces.member_loc(k).cloned())
        .map(|loc| (loc.output, loc.workspace))
}

/// Global-unique sync with an explicit foreground location (production
/// passes the gated live read above; tests pass an identified member
/// location to drive the real foreground-on-removed branch offline).
pub(super) fn sync_global_with_foreground(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    fg_workspace: Option<(String, String)>,
) {
    sync_global(state, areas, fg_workspace);
}

/// Global-unique sync: same displacement/return mechanics as local, but
/// history records into the scoped owner (per-output entries) and scoped
/// trailing converges afterwards with displaced/in-flight protection.
fn sync_global(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    fg_workspace: Option<(String, String)>,
) {
    let current: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
    let displaced_origins: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .keys()
        .cloned()
        .collect();
    for key in &current {
        if displaced_origins.contains(key) {
            continue;
        }
        // Genuinely new outputs prime both the workspace baseline (via
        // `ensure_output`) and the scoped baseline, mirroring reconnect.
        let unprimed = state.scope_owner.history().baseline(key).is_none();
        state.workspaces.ensure_output(key);
        if unprimed {
            let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
            owner.prime_output(spaces, key);
        }
    }
    let gone: Vec<String> = state
        .known_outputs
        .iter()
        .filter(|k| !current.contains(k))
        .cloned()
        .collect();
    let mut deferred: Vec<String> = Vec::new();
    for origin in gone {
        let reference = state
            .last_areas
            .iter()
            .find(|a| a.device == origin)
            .or_else(|| state.last_areas.first())
            .map(|a| tiler_core::workspace::DisplacedRect {
                x: a.full.x,
                y: a.full.y,
                w: a.full.w,
                h: a.full.h,
            });
        let survivors: Vec<tiler_core::workspace::Survivor> = current
            .iter()
            .map(|k| {
                let rect = areas.iter().find(|a| &a.device == k).map(|a| {
                    tiler_core::workspace::DisplacedRect {
                        x: a.full.x,
                        y: a.full.y,
                        w: a.full.w,
                        h: a.full.h,
                    }
                });
                tiler_core::workspace::Survivor {
                    key: k.clone(),
                    rect,
                }
            })
            .collect();
        let active = if state.active_output.is_empty() {
            None
        } else {
            Some(state.active_output.as_str())
        };
        if let Some(dest) =
            tiler_core::workspace::choose_displaced_destination(&survivors, active, reference)
        {
            let mut relocated = 0u32;
            let mut retained_count = 0u32;
            let before: Vec<String> = state.workspaces.workspace_ids(&origin);
            for ws in &before {
                let target_key = tiler_core::session::DomainKey {
                    output: tiler_core::directional::OutputId(dest.clone()),
                    workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                };
                let bounds = areas
                    .iter()
                    .find(|a| a.device == dest)
                    .and_then(|a| super::tiling_domain_bounds_with(a.work, state.outer_gap));
                let Some(bounds) = bounds else {
                    retained_count += 1;
                    continue;
                };
                let target_domain = tiler_core::session::OutputDomain {
                    id: tiler_core::directional::OutputId(dest.clone()),
                    workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                    bounds,
                    gap: state.inner_gap,
                    adjacent: std::collections::BTreeMap::new(),
                };
                if state.engine.try_relocate_for_target(
                    &target_key,
                    &target_domain,
                    state.outer_gap,
                ) {
                    relocated += 1;
                } else {
                    retained_count += 1;
                }
            }
            state.workspaces.displace_output_to(&origin, &dest);
            // The scoped per-output entry discards with the removed origin;
            // the workspace primitive above already discarded local history.
            state.scope_owner.discard_scoped_output(&origin);
            if state.active_output == origin {
                state.active_output = dest.clone();
            }
            if let Some((fg_output, fg_workspace)) = fg_workspace.clone()
                && fg_output == origin
            {
                state.workspaces.activate(&dest, &fg_workspace);
                if let Some(fg_key) = state
                    .member_tokens
                    .keys()
                    .find(|k| {
                        state
                            .workspaces
                            .member_loc(k)
                            .is_some_and(|l| l.output == dest && l.workspace == fg_workspace)
                    })
                    .cloned()
                {
                    state.workspaces.note_foreground(&fg_key);
                }
            }
            super::log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-displace",
                    "output": state.workspaces.output_token(&dest),
                    "relocated": relocated,
                    "retained": retained_count,
                }),
            );
            // Scoped observation: the survivor's post-actuation view records
            // into the per-output scoped entry (a moved previous clears like
            // removal via the eager scope validation).
            if let Some(active) = state.workspaces.active_id(&dest) {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.record_completed(spaces, &dest, &active);
            }
        } else {
            state.workspaces.discard_output_history(&origin);
            state.scope_owner.discard_scoped_output(&origin);
            deferred.push(origin);
        }
    }
    let displaced: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .keys()
        .cloned()
        .collect();
    for origin in displaced {
        if current.contains(&origin) {
            let dest_key = state
                .workspaces
                .displaced_snapshot()
                .get(&origin)
                .map(|r| r.dest_key.clone());
            if let Some(record) = state.workspaces.displaced_snapshot().get(&origin).cloned() {
                let bounds = areas
                    .iter()
                    .find(|a| a.device == origin)
                    .and_then(|a| super::tiling_domain_bounds_with(a.work, state.outer_gap));
                if let Some(bounds) = bounds {
                    for ws in &record.workspace_ids {
                        let target_key = tiler_core::session::DomainKey {
                            output: tiler_core::directional::OutputId(origin.clone()),
                            workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                        };
                        let target_domain = tiler_core::session::OutputDomain {
                            id: tiler_core::directional::OutputId(origin.clone()),
                            workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                            bounds,
                            gap: state.inner_gap,
                            adjacent: std::collections::BTreeMap::new(),
                        };
                        state.engine.try_relocate_for_target(
                            &target_key,
                            &target_domain,
                            OUTER_GAP,
                        );
                    }
                }
            }
            state.workspaces.reconnect_output(&origin);
            // Reconnect selection never consults scoped history either: drop
            // any stale scoped entry and re-prime the baseline at the
            // returning view. The next observed change records anew.
            state.scope_owner.discard_scoped_output(&origin);
            if state.workspaces.active_id(&origin).is_some() {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.prime_output(spaces, &origin);
            }
            let dest_key = dest_key.unwrap_or_else(|| state.active_output.clone());
            if !dest_key.is_empty()
                && let Some(active) = state.workspaces.active_id(&dest_key)
            {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.record_completed(spaces, &dest_key, &active);
            }
        }
    }
    // Scoped trailing convergence on the post-topology mapping: ordinary
    // stale empty ids prune (with their idle empty Engine domains) while
    // displaced, in-flight, and unmapped-relevant layouts stay pinned, and a
    // previous naming a removed id clears eagerly.
    for output in state.workspaces.output_keys() {
        if state.workspaces.active_id(&output).is_none() {
            continue;
        }
        super::tiling_sys_scope_trailing::global_cleanup_output(state, &output);
    }
    sync_tail(state, areas, &current, &deferred);
}

/// Shared sync (tentative KDE parity): no project displacement, no return
/// mapping, no Engine relocation. Disconnect drops the removed output and
/// re-homes same-logical-id members onto the surviving destination; the one
/// shared order re-syncs stably (stale ids pruned, never reordered) and the
/// single shared history observes actual survivor views only. Reconnect
/// re-converges the returning origin onto the live shared order at the
/// survivor's current view without consulting history.
fn sync_shared(state: &mut TileLoop, areas: &[MonitorArea]) {
    let current: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
    // Cold-start bootstrap: with no output state yet, create the legacy
    // initial outputs exactly like the local path; the startup caller then
    // runs `prime_after_sync` for shared convergence (stable-key anchor and
    // cleanup). No order sync, history, or cleanup happens here.
    if state.workspaces.output_keys().is_empty() {
        for key in &current {
            state.workspaces.ensure_output(key);
        }
        sync_tail(state, areas, &current, &[]);
        return;
    }
    // Stable shared-order sync first so returning origins converge onto the
    // canonical ring (never reordered by topology churn).
    sync_shared_order_stable(state);
    // Returning or genuinely new outputs converge onto the live shared order
    // at the survivor's current view (areas order is primary-first). No
    // baseline empties are minted and history is never consulted.
    let anchor = current
        .iter()
        .filter_map(|key| state.workspaces.active_id(key))
        .next()
        .or_else(|| state.scope_owner.shared_order().first().cloned());
    let order = state.scope_owner.shared_order().clone();
    for key in &current {
        if state.workspaces.output_keys().contains(key) {
            continue;
        }
        if state.workspaces.displaced_snapshot().contains_key(key) {
            continue;
        }
        if let Some(anchor) = anchor.clone() {
            state.workspaces.insert_shared_output(key, &order, &anchor);
            super::log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-scope",
                    "adopted": "shared",
                    "scope": "reconnect",
                    "workspace": state.workspaces.workspace_token(key, &anchor),
                    "outcome": "ok",
                }),
            );
        }
    }
    let gone: Vec<String> = state
        .known_outputs
        .iter()
        .filter(|k| !current.contains(k))
        .cloned()
        .collect();
    let mut deferred: Vec<String> = Vec::new();
    for origin in gone {
        let reference = state
            .last_areas
            .iter()
            .find(|a| a.device == origin)
            .or_else(|| state.last_areas.first())
            .map(|a| tiler_core::workspace::DisplacedRect {
                x: a.full.x,
                y: a.full.y,
                w: a.full.w,
                h: a.full.h,
            });
        let survivors: Vec<tiler_core::workspace::Survivor> = current
            .iter()
            .map(|k| {
                let rect = areas.iter().find(|a| &a.device == k).map(|a| {
                    tiler_core::workspace::DisplacedRect {
                        x: a.full.x,
                        y: a.full.y,
                        w: a.full.w,
                        h: a.full.h,
                    }
                });
                tiler_core::workspace::Survivor {
                    key: k.clone(),
                    rect,
                }
            })
            .collect();
        let active = if state.active_output.is_empty() {
            None
        } else {
            Some(state.active_output.as_str())
        };
        if let Some(dest) =
            tiler_core::workspace::choose_displaced_destination(&survivors, active, reference)
        {
            // No Engine relocation: each output keeps its own layout domain,
            // so the surviving sibling layout is never clobbered by the
            // removed output's session. Same-logical-id members re-home onto
            // the survivor; the origin entry drops with no displaced record.
            let dest_token = state.workspaces.output_token(&dest);
            state.workspaces.drop_output_merge_to(&origin, &dest);
            if state.active_output == origin {
                state.active_output = dest.clone();
            }
            super::log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-scope",
                    "adopted": "shared",
                    "scope": "disconnect",
                    "output": dest_token,
                    "outcome": "ok",
                }),
            );
            // Output-scoped Engine pruning for the removed origin: ordinary
            // orphan idle-empty domains drop even when their logical id
            // stays live on the survivor; nonempty and in-flight
            // (pending/desired/drag) layouts persist for return. Survivor
            // sibling domains are never touched.
            let orphaned: Vec<DomainKey> = state
                .engine
                .domain_keys()
                .into_iter()
                .filter(|k| k.output.0 == *origin)
                .collect();
            for key in orphaned {
                let idle = state.engine.session(&key).is_some_and(|s| {
                    tiler_core::engine::committed_session_is_empty(s)
                        && !s.has_pending()
                        && !s.has_pending_desired()
                        && !s.has_drag()
                });
                if idle {
                    state.engine.remove(&key);
                }
            }
        } else {
            // No survivor: keep the entry, sessions, and members untouched;
            // only the per-output local history discards. The single shared
            // history persists (it is scope-wide, not output-bound).
            state.workspaces.discard_output_history(&origin);
            deferred.push(origin);
        }
    }
    sync_shared_order_stable(state);
    // Observe actual survivor views only; survivors keep their views (no
    // foreground-driven switching in shared mode).
    for key in &current {
        if let Some(active) = state.workspaces.active_id(key) {
            let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
            owner.record_completed(spaces, key, &active);
        }
    }
    super::tiling_sys_scope_trailing::shared_cleanup(state);
    sync_tail(state, areas, &current, &deferred);
}

/// Shared known-geometry/active tail: deferred no-survivor origins stay
/// known with their last geometry until a survivor exists.
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
        if !last.iter().any(|a| &a.device == origin)
            && let Some(area) = state
                .last_areas
                .iter()
                .find(|a| &a.device == origin)
                .cloned()
        {
            last.push(area);
        }
    }
    state.last_areas = last;
    if state.active_output.is_empty()
        && let Some(first) = state.known_outputs.first().cloned()
    {
        state.active_output = first;
    }
}

/// Stable shared-order sync: prune ids that left the live union, then adopt
/// any live ids missing from the order. Unlike a fresh live-union rebuild
/// this never reorders surviving ids when the first-sorted output drops.
fn sync_shared_order_stable(state: &mut TileLoop) {
    let mapping = mapping_from_workspaces(&state.workspaces);
    let live: BTreeSet<String> = mapping
        .values()
        .flat_map(|ids| ids.iter().cloned())
        .collect();
    let mut kept: Vec<String> = state
        .scope_owner
        .shared_order()
        .iter()
        .filter(|id| live.contains(*id))
        .cloned()
        .collect();
    for ids in mapping.values() {
        for id in ids {
            if !kept.iter().any(|existing| existing == id) {
                kept.push(id.clone());
            }
        }
    }
    state.scope_owner.set_shared_order(kept);
}
