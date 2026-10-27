#![forbid(unsafe_code)]
//! World-routed workspace digit intents.
//!
//! The core owns selection and send planning outright (rings, order,
//! history, trailing, refill): digit selects route `SelectNumbered` /
//! `SelectTrailing` and digit sends route `SendNumbered` through the single
//! world owner. No local resolvers, no preactivation, no history peek here.
//!
//! Latest-only scheduling (adapter-only, batch-scoped): pure `Select` bursts
//! order every command through the core exactly once, but only the latest
//! reply plans/applies natively. A superseded select dispatches core-only
//! (fresh observation per dispatch, exact receipts acked, no native planning
//! or application on the superseded leg) the moment the next select arrives;
//! the batch's latest select takes the full path. `Send` is a barrier: it
//! flushes any pending select through the full path first, then runs full
//! itself, so a membership move always observes the prior selection natively
//! applied. Gate failures (key-up, suspend, elevation, unknown output)
//! settle without dispatch and never disturb a pending select. A pending
//! flush rechecks the suspend, elevation, and topology gates immediately
//! before applying, so a game change between arrival and flush cannot act.
//! The pending slot lives only for one `workspace_tick` batch and never
//! crosses ticks: no timers, no retries, no saved grants, no stale effect
//! extension. A stale latest effect still drops honestly under the existing
//! apply rule.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Full native application for one gate-passing select: the adapter selects
/// no output (the core resolves numbered/trailing from the cursor-first
/// switch target) and diagnostics read the resolved target from the reply.
/// Unchanged from the pre-latest-only path: the applied reply IS the
/// outcome, and all native gates run inside it.
#[allow(clippy::too_many_arguments)]
fn apply_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    log_path: &Path,
    op: WorkspaceOp,
    index: u8,
    edge: crate::snapkey::SnapEdge,
    tick: u64,
) {
    // Flush-time gate recheck (same gates as intake): the foreground game
    // may have changed between the queued select's arrival and this flush,
    // so a newly suspended/elevated foreground or a vanished topology drops
    // the pending select instead of applying it. Never dispatches.
    if suspend_read(state, me, fulls).veto.block {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "workspace",
                "op": op.as_str(),
                "index": index,
                "edge": edge.as_str(),
                "disposition": "consumed",
                "outcome": "suspended",
            }),
        );
        return;
    }
    if foreground_elevated(me) {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "workspace",
                "op": op.as_str(),
                "index": index,
                "edge": edge.as_str(),
                "disposition": "consumed",
                "outcome": "elevated-foreground",
            }),
        );
        return;
    }
    if areas.is_empty() {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "workspace",
                "op": op.as_str(),
                "index": index,
                "edge": edge.as_str(),
                "disposition": "consumed",
                "outcome": "unknown-output",
            }),
        );
        return;
    }
    let before = state.world.core_projections().2;
    let command = if index == 0 {
        WorldCommand::SelectTrailing { output: None }
    } else {
        WorldCommand::SelectNumbered {
            output: None,
            index,
        }
    };
    let reply =
        super::world_tick::observe_and_command(state, me, store, dir, fulls, areas, command);
    let line = match reply.as_ref() {
        Some(applied) => {
            state.tick += 1;
            match super::history::resolved_select_view(state, &before, applied) {
                Some((resolved, view)) => super::history::select_resolved_log(
                    "workspace",
                    tick,
                    &resolved,
                    &view,
                    op.as_str(),
                    index,
                    edge.as_str(),
                    "ok",
                ),
                None => workspace_log(state, tick, op.as_str(), index, edge.as_str(), "ok"),
            }
        }
        None => workspace_log(state, tick, op.as_str(), index, edge.as_str(), "unverified"),
    };
    log_json_at(log_path, line);
}

/// Core-only ordering for a superseded select: fresh observation per
/// dispatch (observations never render), exact receipts acked, no native
/// planning or application on this leg (the shared admission preamble still
/// binds fail-closed like the full path). `Some` means the core ordered the
/// command (log `superseded`, never `ok`: nothing attempted is successful
/// application); `None` means it never ordered (log `unverified`, same as
/// the full path's fence outcome).
#[allow(clippy::too_many_arguments)]
fn supersede_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    log_path: &Path,
    op: WorkspaceOp,
    index: u8,
    edge: crate::snapkey::SnapEdge,
    tick: u64,
) {
    let command = if index == 0 {
        WorldCommand::SelectTrailing { output: None }
    } else {
        WorldCommand::SelectNumbered {
            output: None,
            index,
        }
    };
    let ordered = super::world_tick::observe_and_command_core_only(
        state, me, store, dir, fulls, areas, command,
    );
    if ordered.is_some() {
        state.tick += 1;
    }
    log_json_at(
        log_path,
        workspace_log(
            state,
            tick,
            op.as_str(),
            index,
            edge.as_str(),
            if ordered.is_some() {
                "superseded"
            } else {
                "unverified"
            },
        ),
    );
}

/// Drain one bounded batch of workspace digit intents through the world
/// owner. Select works on empty workspaces and unmanaged foreground; send
/// refuses unmanaged focus. The takeover-off switch disables all
/// interception. Fullscreen and elevated foreground gate both ops.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<crate::snapkey::QueuedWorkspaceIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    // Batch-scoped pending latest select: superseded entries dispatch
    // core-only on arrival of the next select; the surviving entry applies
    // fully at the next barrier or at batch end. Never crosses ticks.
    let mut pending_select: Option<crate::snapkey::QueuedWorkspaceIntent> = None;
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace",
                        "tick": state.tick,
                        "op": intent.op.as_str(),
                        "index": intent.index,
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        // Mechanical topology-existence fence (no policy): with no live
        // area the core has no registry to resolve from. Every command
        // below omits the output; the core resolves select targets from
        // the cursor-first switch target and keyboard send sources from
        // the mover's retained home else the cached-live fallback.
        if areas.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        let op = intent.op;
        let index = intent.index;
        let edge = intent.edge;
        let tick = state.tick;
        match op {
            WorkspaceOp::Select => {
                // Order the previously pending select core-only now: it is
                // superseded by this arrival, so its reply must never plan
                // or apply natively. Each dispatch observes fresh; only the
                // surviving select applies below.
                if let Some(prev) = pending_select.take() {
                    let prev_tick = tick;
                    supersede_select(
                        state, me, store, dir, fulls, areas, &log_path, prev.op, prev.index,
                        prev.edge, prev_tick,
                    );
                }
                pending_select = Some(intent);
            }
            WorkspaceOp::Send => {
                // Barrier: the pending select (if any) applies fully first
                // so the membership move observes it natively, exactly as
                // the pre-latest-only sequential order did.
                if let Some(pending) = pending_select.take() {
                    let pending_tick = state.tick;
                    apply_select(
                        state,
                        me,
                        store,
                        dir,
                        fulls,
                        areas,
                        &log_path,
                        pending.op,
                        pending.index,
                        pending.edge,
                        pending_tick,
                    );
                }
                // Seam-aware foreground read (production-identical; scripted
                // in unit tests): the chord origin binds against this HWND,
                // so tests drive the barrier deterministically.
                let foreground_hwnd = live_foreground_window();
                let origin = intent.origin.clone();
                match origin.filter(|o| o.hwnd == foreground_hwnd) {
                    Some(o) if !o.token.is_empty() => {
                        // The command carries a home-then-cached-live
                        // source: the core resolves the mover's retained
                        // home, else the cached output when live, else the
                        // first live screen. The cursor never steers a
                        // homed mover; only a homeless mover can fall
                        // through to a pointer-independent cached-live
                        // fallback, never an adapter decision.
                        let command = WorldCommand::SendNumbered {
                            window: tiler_core::directional::WindowId(o.token.clone()),
                            source: tiler_core::world_report::SendSource::HomeThenCachedLive,
                            index,
                            follow: intent.follow,
                        };
                        super::world_tick::observe_and_command(
                            state, me, store, dir, fulls, areas, command,
                        );
                        let outcome: &'static str = if state.world.last_reply().is_some() {
                            state.tick += 1;
                            "ok"
                        } else {
                            "unverified"
                        };
                        log_json_at(
                            &log_path,
                            workspace_log(state, tick, op.as_str(), index, edge.as_str(), outcome),
                        );
                    }
                    _ => {
                        log_json_at(
                            &log_path,
                            workspace_log(
                                state,
                                tick,
                                op.as_str(),
                                index,
                                edge.as_str(),
                                "unmanaged",
                            ),
                        );
                    }
                }
            }
        }
    }
    // Flush the surviving select (if any) through the full native path.
    if let Some(pending) = pending_select.take() {
        let pending_tick = state.tick;
        apply_select(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            &log_path,
            pending.op,
            pending.index,
            pending.edge,
            pending_tick,
        );
    }
}
