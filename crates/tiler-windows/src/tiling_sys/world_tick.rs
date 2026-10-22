//! Single active world tick (stage 2b, wired).
//!
//! The sole production owner path: mechanical admission preamble (native
//! binding/claim/identity/lifetime caches only, never policy), capture
//! source-first, sequence one typed report, deliver through the staged
//! production path, apply the accepted reply through [`HostDiff`] +
//! [`LiveNative`], acknowledge, and retain proven applied homes/views.
//!
//! Policy authority is the core reply alone. The adapter keeps only native
//! caches: stable tokens, identities, lifetime tags, rect snapshots,
//! hide-claims, and the mechanical device-to-wire-output map. No
//! ring/history/trailing/tiling mutation lives here: the core owns
//! lifecycle outright and this module only routes its commands and applies
//! its desires with fresh proof.

use super::world_capture::capture_world;
use super::world_diff::{FreshHost, NativeOps};
use super::world_native::{LiveNative, NativePrimitives};
use super::{MonitorArea, ObservedWindow, RetainedRow, TileLoop};
use std::collections::HashMap;
use tiler_core::world_report::{CommandResult, TransferKind, WorldCommand, WorldReply};

/// Merge applied transfer effects into the proven receipt maps. Each
/// staged raw attempt with post proof at the target revalidates against
/// independent fresh native proof (never the native cache alone): a window
/// transfer merges only when the live homing still names the target under
/// full identity/tag checks (plus, for tiled rows, the exact live frame
/// on the target), and a view or migration merges only when the live view
/// still names the target. Attempts whose post proof is elsewhere merge
/// nothing. Floating membership-only crossings preserve the physical frame
/// while arriving logically: an accepted attempt merges through the
/// no-effect proof below (identity plus preserved frame plus visibility),
/// never on a reinterpreted physical output. Empty views merge only on
/// explicit finished batch proof (the live view names the target with zero
/// stray visibility), never on a desired id echo when no work ran.
pub(super) fn merge_applied_transfers(
    reply: &tiler_core::world_report::WorldReply,
    member_attempts: &[tiler_core::world_report::MemberAttempt],
    view_attempts: &[tiler_core::world_report::ViewAttempt],
    fresh: &FreshHost,
    native: &mut LiveNative,
    proven_h: &mut HashMap<String, (String, String)>,
    proven_v: &mut HashMap<String, String>,
) {
    use tiler_core::world_report::{FrameSetter, TransferKind};
    for transfer in &reply.transfers {
        let proven = match &transfer.kind {
            TransferKind::Window(window) => member_attempts.iter().any(|attempt| {
                attempt.operation_id == transfer.operation_id
                    && attempt.window == *window
                    && attempt.target_output == transfer.target_output
                    && attempt.target_workspace == transfer.target_workspace
                    && (attempt.post.as_ref().is_some_and(|(out, ws)| {
                        out == &transfer.target_output && ws == &transfer.target_workspace
                    }) || (matches!(attempt.setter, FrameSetter::Accepted)
                        && reply
                            .windows
                            .iter()
                            .any(|row| row.lifetime == *window && row.floating)
                        && fresh.baseline.get(&window.0).is_some_and(|base| {
                            super::world_diff::float_no_effect_proven(
                                native,
                                base,
                                &fresh.stored_tags,
                            )
                        })))
            }),
            TransferKind::View | TransferKind::Migration => view_attempts.iter().any(|attempt| {
                attempt.operation_id == transfer.operation_id
                    && attempt.target_output == transfer.target_output
                    && attempt.target_workspace == transfer.target_workspace
                    && attempt.post.as_deref() == Some(transfer.target_workspace.as_str())
            }),
        };
        if !proven {
            continue;
        }
        // Independent fresh proof at merge time: the raw post alone
        // never merges. A recycled HWND or a raced hide between the
        // diff pass and this snapshot must not plant a receipt.
        match &transfer.kind {
            TransferKind::Window(window) => {
                if native.read_homing(&window.0).is_some_and(|(out, ws)| {
                    out == transfer.target_output.0 && ws == transfer.target_workspace
                }) && tiled_frame_matches(reply, native, &window.0)
                {
                    proven_h.insert(
                        window.0.clone(),
                        (
                            transfer.target_output.0.clone(),
                            transfer.target_workspace.clone(),
                        ),
                    );
                }
            }
            TransferKind::View | TransferKind::Migration => {
                if native.read_view(&transfer.target_output.0).as_deref()
                    == Some(transfer.target_workspace.as_str())
                {
                    proven_v.insert(
                        transfer.target_output.0.clone(),
                        transfer.target_workspace.clone(),
                    );
                }
            }
        }
    }
}

/// Tiled frame revalidation for one window transfer at merge time: rows
/// with a tiled effective frame require the exact live frame (a held app
/// size or a raced move between the diff pass and this snapshot holds
/// the merge); membership-only rows (floating, no frame) skip the check
/// and settle on the homing plus visibility proof alone.
fn tiled_frame_matches(
    reply: &tiler_core::world_report::WorldReply,
    native: &mut LiveNative,
    token: &str,
) -> bool {
    let Some(row) = reply.windows.iter().find(|row| row.lifetime.0 == token) else {
        return true;
    };
    if row.floating {
        return true;
    }
    let Some(target) = row.effective_frame else {
        return true;
    };
    native.read_frame(token).is_some_and(|live| live == target)
}

/// Trace-only apply diagnostic caps: rows stay bounded by the reply vec
/// lengths (already under `WORLD_MAX_REPLY_BYTES`) plus these fixed
/// per-event caps with honest `omitted` counts. Never a per-window dump:
/// only feedback rows, held scopes, and retained transfer desires ride.
const MAX_APPLY_DIAG_FEEDBACK: usize = 16;
const MAX_APPLY_DIAG_SCOPES: usize = 16;
const MAX_APPLY_DIAG_TRANSFERS: usize = 16;

/// Redacted fixed command vocabulary for the apply diagnostic: the kind
/// alone, never window tokens, output/workspace strings, exe paths,
/// directions, or any other raw command payload.
fn world_command_kind(command: &WorldCommand) -> &'static str {
    match command {
        WorldCommand::Focus { .. } => "focus",
        WorldCommand::Send { .. } => "send",
        WorldCommand::SwitchView { .. } => "switch-view",
        WorldCommand::SelectWorkspace { .. } => "select-workspace",
        WorldCommand::SelectNumbered { .. } => "select-numbered",
        WorldCommand::SelectRelative { .. } => "select-relative",
        WorldCommand::SelectPrevious { .. } => "select-previous",
        WorldCommand::SelectTrailing { .. } => "select-trailing",
        WorldCommand::SendRelative { .. } => "send-relative",
        WorldCommand::SendNumbered { .. } => "send-numbered",
        WorldCommand::SetWorkspaceTiling { .. } => "set-workspace-tiling",
        WorldCommand::MigrateWorkspace { .. } => "migrate-workspace",
        WorldCommand::FocusDirectional { .. } => "focus-directional",
        WorldCommand::MoveDirectional { .. } => "move-directional",
        WorldCommand::SendDirectional { .. } => "send-directional",
        WorldCommand::MigrateDirectional { .. } => "migrate-directional",
        WorldCommand::Operation(_) => "operation",
    }
}

/// Redacted fixed result vocabulary: the variant alone, never rects,
/// members, or bounds.
fn command_result_kind(result: &Option<CommandResult>) -> &'static str {
    match result {
        None => "none",
        Some(CommandResult::DragPreview { .. }) => "drag-preview",
        Some(CommandResult::Float { .. }) => "float",
        Some(CommandResult::ActiveGroup { .. }) => "active-group",
        Some(CommandResult::NoGroup { .. }) => "no-group",
        Some(CommandResult::Migration { .. }) => "migration",
    }
}

/// Event-driven throttle: commands always log (the owner trace only runs on
/// dispatch), while observation ticks log only on change: staged raw
/// attempts, landed writes, or a stopped/partial pass. Pure-held
/// observation ticks (steady-state holds with no attempt) stay quiet; a
/// command that holds with no attempt still logs on its own command row
/// with `held > 0` and empty attempt rows, which is exactly the no-attempt
/// Hold signature.
fn should_log_apply(
    command_kind: &str,
    stats: &super::world_diff::ApplyStats,
    member_attempts: &[tiler_core::world_report::MemberAttempt],
    view_attempts: &[tiler_core::world_report::ViewAttempt],
) -> bool {
    if command_kind != "observation" {
        return true;
    }
    if !member_attempts.is_empty() || !view_attempts.is_empty() {
        return true;
    }
    if stats.stopped_mid_apply || stats.partial {
        return true;
    }
    stats.geometry_writes
        + stats.membership_writes
        + stats.view_writes
        + stats.creations
        + stats.removals
        + stats.effects
        > 0
}

/// Bounded fresh-focus trace row for the `world-apply` diag (TRACE-ONLY).
/// The token/cursor/settings describe the CURRENT report; the nested
/// `last_focused_leg` is the core's cached historical leg decision (last
/// fresh admission), never the current focus fact. Never mixed: the stale
/// misread this split prevents is treating the cached leg's reported flag
/// as the current token's presence. Fixed shape, no payload bytes, no
/// input; the token is a stable lifetime id, never a title.
fn fresh_route_value(
    focused_token: &str,
    cursor: Option<(i32, i32)>,
    fresh_placement: &str,
    switch_target: &str,
    trace: (Option<String>, Option<String>, Option<String>, &'static str),
) -> serde_json::Value {
    serde_json::json!({
        "focused_token": focused_token,
        "focused_reported": !focused_token.is_empty(),
        "cursor": cursor,
        "fresh_placement": fresh_placement,
        "switch_target": switch_target,
        "last_focused_leg": {
            "current": trace.0,
            "retained": trace.1,
            "chosen": trace.2,
            "basis": trace.3,
        },
    })
}

/// Pure trace-row builder for one apply pass (TRACE-ONLY: no actuation
/// input, no retry, no interpretation). Raw attempt rows appear only when
/// the diff dispatched a setter this pass: setter-side write counts never
/// fabricate a proof row. Desired homing/views ride labeled `desired_*`,
/// never as observed proof. The only refusal detail available from existing
/// values is the raw setter vocabulary: the [`NativeOps`] contract
/// surfaces explicit refusal as `Ok(false)` and torn state as `Err`
/// without a native errno, so deeper refusal detail would need a broader
/// seam change (reported as a limitation, not papered over here).
// Nine inputs are the disjoint trace context plus the pass products;
// bundling would hide the call-site borrow split the tick relies on.
#[allow(clippy::too_many_arguments)]
fn apply_diag_value(
    tick: u64,
    reply: &WorldReply,
    delivery: &'static str,
    command_kind: &str,
    stats: &super::world_diff::ApplyStats,
    member_attempts: &[tiler_core::world_report::MemberAttempt],
    view_attempts: &[tiler_core::world_report::ViewAttempt],
    fresh_route: serde_json::Value,
) -> serde_json::Value {
    let member_rows: Vec<serde_json::Value> = member_attempts
        .iter()
        .take(MAX_APPLY_DIAG_FEEDBACK)
        .map(|item| {
            serde_json::json!({
                "op": item.operation_id.clone(),
                "window": item.window.0.clone(),
                "out": item.target_output.0.clone(),
                "ws": item.target_workspace.clone(),
                "setter": item.setter.as_str(),
                "pre": item.pre.as_ref().map(|(out, ws)| format!("{out}/{ws}", out = out.0)),
                "post": item.post.as_ref().map(|(out, ws)| format!("{out}/{ws}", out = out.0)),
            })
        })
        .collect();
    let view_rows: Vec<serde_json::Value> = view_attempts
        .iter()
        .take(MAX_APPLY_DIAG_FEEDBACK)
        .map(|item| {
            serde_json::json!({
                "op": item.operation_id.clone(),
                "out": item.target_output.0.clone(),
                "ws": item.target_workspace.clone(),
                "setter": item.setter.as_str(),
                "pre": item.pre,
                "post": item.post,
                "base": item.baseline,
            })
        })
        .collect();
    let held_scopes: Vec<serde_json::Value> = reply
        .scopes
        .iter()
        .filter(|status| status.authority != tiler_core::world_report::ScopeAuthority::Controlled)
        .take(MAX_APPLY_DIAG_SCOPES)
        .map(|status| {
            serde_json::json!({
                "scope": status.scope.clone(),
                "authority": status.authority.as_str(),
                "reason": status.authority.hold_reason().map(|reason| reason.as_str()),
            })
        })
        .collect();
    let desired_transfers: Vec<serde_json::Value> = reply
        .transfers
        .iter()
        .take(MAX_APPLY_DIAG_TRANSFERS)
        .map(|transfer| {
            let (kind, window) = match &transfer.kind {
                TransferKind::Window(id) => ("window", Some(id.0.clone())),
                TransferKind::View => ("view", None),
                TransferKind::Migration => ("migration", None),
            };
            serde_json::json!({
                "op": transfer.operation_id.clone(),
                "kind": kind,
                "window": window,
                "target_output": transfer.target_output.0.clone(),
                "target_workspace": transfer.target_workspace.clone(),
                "follow": transfer.follow,
                "held": transfer.held,
            })
        })
        .collect();
    serde_json::json!({
        "event": "world-apply",
        "tick": tick,
        "request": reply.request_id.clone(),
        "revision": reply.state_revision,
        "observation": reply.based_on_observation_seq,
        "delivery": delivery,
        "command": command_kind,
        "command_consumed": reply.command_consumed,
        "command_result": command_result_kind(&reply.command_result),
        "stats": {
            "geometry_writes": stats.geometry_writes,
            "membership_writes": stats.membership_writes,
            "view_writes": stats.view_writes,
            "creations": stats.creations,
            "removals": stats.removals,
            "effects": stats.effects,
            "held": stats.held,
            "migration_total": stats.migration_total,
            "migration_proven": stats.migration_proven,
            "stopped_mid_apply": stats.stopped_mid_apply,
            "partial": stats.partial,
        },
        "members": member_rows,
        "members_omitted": member_attempts
            .len()
            .saturating_sub(MAX_APPLY_DIAG_FEEDBACK.min(member_attempts.len())),
        "views": view_rows,
        "views_omitted": view_attempts
            .len()
            .saturating_sub(MAX_APPLY_DIAG_FEEDBACK.min(view_attempts.len())),
        "held_scopes": held_scopes,
        "held_scopes_omitted": reply
            .scopes
            .iter()
            .filter(|status| {
                status.authority != tiler_core::world_report::ScopeAuthority::Controlled
            })
            .count()
            .saturating_sub(held_scopes.len()),
        "desired_transfers": desired_transfers,
        "desired_transfers_omitted": reply
            .transfers
            .len()
            .saturating_sub(desired_transfers.len()),
        "fresh_route": fresh_route,
    })
}

/// Trace-only emission for one apply pass. Event-driven owner trace only:
/// no input-hook cost, no native re-read, no per-window dump.
// Eight inputs are the disjoint trace context plus the pass products
// (reply, delivery, command kind, stats, raw attempts); bundling would
// hide the call-site borrow split the tick relies on.
#[allow(clippy::too_many_arguments)]
fn log_apply_diag(
    log_path: &std::path::Path,
    trace: bool,
    tick: u64,
    reply: &WorldReply,
    delivery: &'static str,
    command_kind: &str,
    stats: &super::world_diff::ApplyStats,
    member_attempts: &[tiler_core::world_report::MemberAttempt],
    view_attempts: &[tiler_core::world_report::ViewAttempt],
    fresh_route: serde_json::Value,
) {
    if !trace || !should_log_apply(command_kind, stats, member_attempts, view_attempts) {
        return;
    }
    super::trace::log_json_at(
        log_path,
        apply_diag_value(
            tick,
            reply,
            delivery,
            command_kind,
            stats,
            member_attempts,
            view_attempts,
            fresh_route,
        ),
    );
}

/// Mechanical admission preamble: bind observed windows to stable tokens
/// plus stored identity and stamped lifetime tags. Admission verifies the
/// holdable key, live identity, scope fence, and hosted-child gate, then
/// stamps the visible-membership tag; it never assigns workspaces, never
/// touches rings/views, and never commits policy. Prunes only
/// verified-closed lifetimes (dropped from the member maps once the close
/// receipt pruned the native queue) and recycled-HWND stale entries.
fn admit_observed(
    state: &mut TileLoop,
    me: &crate::model::ProcessIdentity,
    observed: &[ObservedWindow],
) {
    for window in observed {
        let key = crate::native_guards::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        // Recycled-HWND stale repair (cross-process): the fresh enumeration
        // is authoritative, so any other same-HWND member is a destroyed
        // window whose number the OS recycled. Hidden claims own their HWND
        // through the ledger/audit path and never drop here. Verified
        // retirement only: the stale tag must no longer read live (or the
        // window is gone) before any table drops; a queued verified close
        // rides the next report so the core forgets the slot.
        let stale = crate::native_guards::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &key,
        );
        for dead in stale {
            let stored = state.member_tags.get(&dead).cloned().unwrap_or_default();
            if stored.is_empty() {
                continue;
            }
            let live_tag = crate::product_hide::sys::read_member_tag(dead.hwnd);
            let live_ok = crate::native_guards::visible_lifetime_ok(&stored, live_tag.as_deref())
                && crate::ffi::window_pid(dead.hwnd) == dead.pid;
            if live_ok {
                continue;
            }
            if let (Some(token), Some(loc)) = (
                state.member_tokens.get(&dead).cloned(),
                state.member_home(&dead),
            ) && !token.is_empty()
            {
                state.pending_closed.insert(
                    token,
                    super::retained::PendingClosed {
                        output: loc.output,
                        workspace: loc.workspace,
                    },
                );
            }
            super::rows::drop_member_state(state, &dead);
        }
        if state.member_tokens.contains_key(&key) {
            continue;
        }
        if state.hidden_claims.keys().any(|k| k.hwnd == key.hwnd) {
            continue;
        }
        // Holdable-key plus live identity proof (same-HWND reuse fails
        // closed): the stored tables bind full identity or nothing.
        let Some(held) = super::fullscreen::holdable_key(state, me, key.hwnd) else {
            continue;
        };
        if held != key {
            continue;
        }
        let live = match crate::native::HeldProcess::open(key.pid)
            .ok()
            .and_then(|held| held.identity().ok())
        {
            Some(live) => live,
            None => continue,
        };
        if live.pid != key.pid
            || live.process_creation != key.creation
            || live.exe_path != window.identity.exe_path
        {
            continue;
        }
        if !crate::tiling::scope_allows(&state.scope, &live.exe_path) {
            continue;
        }
        if !super::inspection::hosted_gate_allows(
            &live.exe_path,
            key.hwnd,
            key.pid,
            &state.scope_hosts,
        ) {
            continue;
        }
        if let Some(entries) = state.allowlist.as_ref() {
            let owned = entries
                .iter()
                .find(|e| e.hwnd == key.hwnd)
                .is_some_and(|entry| {
                    super::observe::verify_proof_owned(key.hwnd, entry, me).is_ok()
                });
            if !owned {
                continue;
            }
        }
        let fresh_tag = match crate::product_hide::sys::install_member_tag(key.hwnd, me.pid) {
            Ok(tag) => tag,
            Err(_) => continue,
        };
        let post_ok = crate::product_hide::sys::hold_target_verified(live.pid, &live).is_ok()
            && crate::ffi::window_pid(key.hwnd) == key.pid
            && crate::product_hide::sys::read_member_tag(key.hwnd).as_deref()
                == Some(fresh_tag.as_str());
        if !post_ok {
            continue;
        }
        state
            .member_tokens
            .insert(key.clone(), window.token.clone());
        state.member_identity.insert(key.clone(), live);
        state.member_tags.insert(key.clone(), fresh_tag);
        // Trace-only token<->HWND join for native correlation: the stable
        // token plus the full member identity (HWND/PID/creation). No
        // titles, exe paths, or raw input. Fresh admissions only.
        if state.trace {
            super::trace::log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "member-admitted",
                    "tick": state.tick,
                    "window": window.token.clone(),
                    "hwnd": key.hwnd,
                    "pid": key.pid,
                    "creation": key.creation,
                }),
            );
        }
    }
}

/// Refresh mechanical snapshots: last-known frames per token for overlay
/// slots and hidden snapshots. Tiled members refresh their tile slot;
/// floating lanes (sticky, automatic fixed, core floats, born holds) track
/// the live frame in `float_rects` and never plant a tile slot (automatic
/// floats additionally drop any first-contact seed).
pub(super) fn snapshot_frames(state: &mut TileLoop, observed: &[ObservedWindow]) {
    for window in observed {
        let key = crate::native_guards::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        let Some(bound) = state.member_tokens.get(&key).cloned() else {
            continue;
        };
        if bound != window.token {
            continue;
        }
        let float_lane = state.sticky.contains_key(&key)
            || state.fixed_auto_seen.contains(&key)
            || state.core_is_float_token(&bound)
            || state.born_fullscreen.contains(&key);
        if float_lane {
            state.float_rects.insert(bound.clone(), window.visible);
            if state.fixed_auto_seen.contains(&key) || state.core_is_automatic_token(&bound) {
                state.member_rects.remove(&bound);
            }
            continue;
        }
        state.member_rects.insert(bound, window.visible);
    }
}

/// World preamble for retained-only arrivals and quiet restores: bind
/// first-seen fullscreen/maximized members slotless, re-adopt surviving
/// float/sticky/tile-override markers, then clear first-seen maximizes
/// exactly once. Idempotent with the reconcile preamble (all three
/// early-out on membership, hydration, and attempt state).
fn admit_retained_and_marks(
    state: &mut TileLoop,
    me: &crate::model::ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    areas: &[MonitorArea],
) {
    super::restart::admit_born_fullscreen(state, me, retained, areas);
    let mut hint_cx = super::HintCx::new();
    super::restart::admit_slotless_maximized(state, me, retained, areas, &mut hint_cx);
    super::restart::adopt_restart_markers(state, me, observed, retained, areas, &mut hint_cx);
    super::admission::clear_maximize_at_admission(state, me, retained);
}

/// Run one observation tick through the single world owner. Returns the
/// applied reply when the sequencer accepted and the diff applied it.
// Nine inputs are the disjoint production tick products (state, identity,
// store, paths, observations, areas, foreground, frames); bundling would
// hide the borrow split the runtime relies on.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_observation_tick(
    state: &mut TileLoop,
    me: &crate::model::ProcessIdentity,
    store: &crate::storage::LedgerStore,
    dir: &std::path::Path,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    areas: &[MonitorArea],
    foreground_hwnd: u64,
    fulls: &[tiler_core::geometry::Rect],
    cursor: Option<(i32, i32)>,
) -> Option<tiler_core::world_report::WorldReply> {
    let trace = state.trace;
    let tick = state.tick;
    let log_path = state.log_path.clone();
    admit_observed(state, me, observed);
    admit_retained_and_marks(state, me, observed, retained, areas);
    snapshot_frames(state, observed);
    super::rows::refresh_world_hints(state, observed, retained);
    // Disjoint borrows: `world` mutably alongside every read-only input
    // field, never the whole loop (see `TileLoop::owner_inputs`).
    let TileLoop {
        ref mut world,
        ref member_tokens,
        ref member_identity,
        ref member_tags,
        ref member_rects,
        ref float_rects,
        ref hint_logged,
        ref sticky,
        ref floated,
        ref restart_reject_hold,
        ref born_fullscreen,
        ref born_first_exit,
        ref fixed_auto_seen,
        ref fixed_tile_overrides,
        ref fixed_origin_overrides,
        ref mut hidden_claims,
        ref minimized_omitted,
        ref mut pending_closed,
        ref scope,
        ref scope_hosts,
        ref settings_live,
        ref output_tokens,
        ref mut applied_homing,
        ref mut applied_views,
        inner_gap,
        outer_gap,
        ref active,
        ref allowlist,
        workspace_proof,
        ref float_topmost_prev,
        ..
    } = *state;
    // Invalidate synchronously on every tick input (dirty-until-ack).
    world.invalidate();
    // One monotonic sample for this tick (same epoch as the log stamp):
    // the report observation time and the stamp for any frame attempt the
    // apply pass below produces. Never wall-clock, never polled.
    let tick_ms = super::trace::log_mono_ms();
    let inputs = TileLoop::owner_inputs(
        member_tokens,
        member_identity,
        member_tags,
        member_rects,
        float_rects,
        hint_logged,
        sticky,
        floated,
        restart_reject_hold,
        born_fullscreen,
        born_first_exit,
        fixed_auto_seen,
        fixed_tile_overrides,
        fixed_origin_overrides,
        hidden_claims,
        minimized_omitted,
        pending_closed,
        scope,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
    );
    let report = match world.try_dispatch_observation(
        &inputs,
        Some(observed),
        retained,
        areas,
        foreground_hwnd,
        tick_ms,
    ) {
        Ok(report) => report,
        // The observation never dispatched (busy flight, catch-up owed to
        // a command, or exhausted counters): nothing ran, so no cached
        // reply may stand in as this tick's outcome.
        Err(_) => return None,
    };
    let outcome = world.deliver_production(&report);
    let (delivery, reply) = match outcome {
        tiler_core::world_engine::WorldOutcome::Accepted(reply) => ("accepted", reply),
        tiler_core::world_engine::WorldOutcome::Duplicate(reply) => ("duplicate", reply),
        tiler_core::world_engine::WorldOutcome::CommandConsumed(reply) => {
            ("command-consumed", reply)
        }
        tiler_core::world_engine::WorldOutcome::Rejected { .. } => {
            world.note_rejected_or_lost(&report.request_id);
            return None;
        }
    };
    if reply.command_consumed {
        // Observation reconciled without re-execution; still acknowledge.
    }
    let inputs = TileLoop::owner_inputs(
        member_tokens,
        member_identity,
        member_tags,
        member_rects,
        float_rects,
        hint_logged,
        sticky,
        floated,
        restart_reject_hold,
        born_fullscreen,
        born_first_exit,
        fixed_auto_seen,
        fixed_tile_overrides,
        fixed_origin_overrides,
        hidden_claims,
        minimized_omitted,
        pending_closed,
        scope,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
    );
    let captured = capture_world(&inputs, Some(observed), retained, areas, foreground_hwnd);
    let inputs = TileLoop::owner_inputs(
        member_tokens,
        member_identity,
        member_tags,
        member_rects,
        float_rects,
        hint_logged,
        sticky,
        floated,
        restart_reject_hold,
        born_fullscreen,
        born_first_exit,
        fixed_auto_seen,
        fixed_tile_overrides,
        fixed_origin_overrides,
        hidden_claims,
        minimized_omitted,
        pending_closed,
        scope,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
    );
    let fresh = FreshHost::from_sources(&inputs, observed, retained, &captured);
    let (homing, rings, views) = world.core_projections();
    let proven_homing = applied_homing.clone();
    let proven_views = applied_views.clone();
    // Tiled effective frames ride as actuation TARGETS only (never
    // proof): the production seam writes them inside `move_membership`
    // under the same gates as `write_frame`. Membership-only rows
    // (floats/sticky/overlays) carry no entry and preserve the frame.
    let mut desired_frames = HashMap::new();
    for row in &reply.windows {
        if !row.floating
            && let Some(target) = row.effective_frame
        {
            desired_frames.insert(row.lifetime.0.clone(), target);
        }
    }
    let mut native = LiveNative::assemble(
        me,
        fulls,
        output_tokens,
        homing,
        rings,
        views,
        desired_frames,
        areas.to_vec(),
        proven_homing,
        proven_views,
        member_tokens,
        member_tags,
        member_identity,
        hidden_claims,
        sticky,
        float_topmost_prev,
        scope,
        scope_hosts,
        allowlist.as_ref(),
        workspace_proof,
        store,
        dir,
        active,
        NativePrimitives::default(),
    );
    world.sync_diff_gate();
    let mut frame_attempts = Vec::new();
    let mut member_attempts = Vec::new();
    let mut view_attempts = Vec::new();
    let stats = world.apply_accepted(
        &reply,
        &fresh,
        &mut native,
        &mut frame_attempts,
        &mut member_attempts,
        &mut view_attempts,
        tick_ms,
    );
    log_apply_diag(
        &log_path,
        trace,
        tick,
        &reply,
        delivery,
        "observation",
        &stats,
        &member_attempts,
        &view_attempts,
        fresh_route_value(
            &captured.focused_global,
            captured.cursor,
            captured.fresh_placement.as_wire_str(),
            captured.switch_target.as_wire_str(),
            world.engine().fresh_focus_trace(),
        ),
    );
    // Proven receipt snapshot: desired values only where live native
    // evidence proves arrival (identity/tag/claim/visibility/scope
    // gates, exact frame on the target for tiled rows). Changed homing
    // additionally merges from staged raw attempt post proofs with
    // independent fresh revalidation. Never wholesale desire.
    let (mut proven_h, mut proven_v) = native.receipt_updates(&reply);
    merge_applied_transfers(
        &reply,
        &member_attempts,
        &view_attempts,
        &fresh,
        &mut native,
        &mut proven_h,
        &mut proven_v,
    );
    drop(native);
    for (token, home) in proven_h {
        applied_homing.insert(token, home);
    }
    for (output, view) in proven_v {
        applied_views.insert(output, view);
    }
    // Mechanical raw facts ride the next report exactly once per key
    // (latest wins); the core consumes each idempotently and reconciles
    // retained evidence across acknowledgement gaps.
    for attempt in frame_attempts {
        world.note_frame_attempt(attempt);
    }
    for attempt in member_attempts {
        world.note_member_attempt(attempt);
    }
    for attempt in view_attempts {
        world.note_view_attempt(attempt);
    }
    let mut pending = std::mem::take(pending_closed);
    let (ack_outcome, pruned) = world.ack(&reply, &mut pending);
    *pending_closed = pending;
    for token in pruned {
        applied_homing.remove(&token);
    }
    match ack_outcome {
        super::world_sequencer::AckOutcome::Applied(_) => {
            world.note_diff_applied(&reply);
            Some(reply)
        }
        super::world_sequencer::AckOutcome::Stale(_) => world.last_reply().cloned(),
        super::world_sequencer::AckOutcome::Ignored { .. } => None,
    }
}

/// Route one explicit [`WorldCommand`] through the single owner: dispatch
/// with the command, deliver, apply, and acknowledge like the observation
/// path. While observation-only catch-up is pending the caller runs the
/// catch-up first and resubmits: the command is never queued or replayed.
// Ten inputs are the disjoint production command products; bundling would
// hide the borrow split the runtime relies on.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_world_command(
    state: &mut TileLoop,
    me: &crate::model::ProcessIdentity,
    store: &crate::storage::LedgerStore,
    dir: &std::path::Path,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    areas: &[MonitorArea],
    foreground_hwnd: u64,
    fulls: &[tiler_core::geometry::Rect],
    command: WorldCommand,
    cursor: Option<(i32, i32)>,
) -> Option<tiler_core::world_report::WorldReply> {
    let trace = state.trace;
    let tick = state.tick;
    let log_path = state.log_path.clone();
    let command_kind = world_command_kind(&command);
    admit_observed(state, me, observed);
    admit_retained_and_marks(state, me, observed, retained, areas);
    snapshot_frames(state, observed);
    super::rows::refresh_world_hints(state, observed, retained);
    // Disjoint borrows: `world` mutably alongside every read-only input
    // field, never the whole loop (see `TileLoop::owner_inputs`).
    let TileLoop {
        ref mut world,
        ref member_tokens,
        ref member_identity,
        ref member_tags,
        ref member_rects,
        ref float_rects,
        ref hint_logged,
        ref sticky,
        ref floated,
        ref restart_reject_hold,
        ref born_fullscreen,
        ref born_first_exit,
        ref fixed_auto_seen,
        ref fixed_tile_overrides,
        ref fixed_origin_overrides,
        ref mut hidden_claims,
        ref minimized_omitted,
        ref mut pending_closed,
        ref scope,
        ref scope_hosts,
        ref settings_live,
        ref output_tokens,
        ref mut applied_homing,
        ref mut applied_views,
        inner_gap,
        outer_gap,
        ref active,
        ref allowlist,
        workspace_proof,
        ref float_topmost_prev,
        ..
    } = *state;
    world.invalidate();
    // One monotonic sample for this command tick (same contract as the
    // observation path above).
    let tick_ms = super::trace::log_mono_ms();
    let report = {
        let inputs = TileLoop::owner_inputs(
            member_tokens,
            member_identity,
            member_tags,
            member_rects,
            float_rects,
            hint_logged,
            sticky,
            floated,
            restart_reject_hold,
            born_fullscreen,
            born_first_exit,
            fixed_auto_seen,
            fixed_tile_overrides,
            fixed_origin_overrides,
            hidden_claims,
            minimized_omitted,
            pending_closed,
            scope,
            settings_live,
            output_tokens,
            applied_homing,
            applied_views,
            inner_gap,
            outer_gap,
            active,
            cursor,
        );
        match world.try_dispatch_command(
            &inputs,
            Some(observed),
            retained,
            areas,
            foreground_hwnd,
            tick_ms,
            command,
        ) {
            Ok(report) => report,
            // Observation-only catch-up is pending or the owner is busy:
            // the command never dispatched (never queued, never replayed),
            // so no cached reply may stand in as its outcome. The caller
            // resubmits after the next observation tick runs the catch-up.
            // Unconditional bounded refusal row (trace-independent) so a
            // live leg can tell dispatch fences from core refusals: the
            // existing `world-apply` diag only runs on accepted replies
            // under trace, so rejected commands previously left no row.
            Err(dispatch) => {
                let reason = match dispatch {
                    super::world_sequencer::DispatchError::Busy => "busy",
                    super::world_sequencer::DispatchError::CatchupRequired => "catchup-required",
                    super::world_sequencer::DispatchError::CountersExhausted => {
                        "counters-exhausted"
                    }
                };
                super::trace::log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "world-apply",
                        "tick": tick,
                        "delivery": "dispatch-refused",
                        "command": command_kind,
                        "reason": reason,
                    }),
                );
                return None;
            }
        }
    };
    let outcome = world.deliver_production(&report);
    let (delivery, reply) = match outcome {
        tiler_core::world_engine::WorldOutcome::Accepted(reply) => ("accepted", reply),
        tiler_core::world_engine::WorldOutcome::Duplicate(reply) => ("duplicate", reply),
        tiler_core::world_engine::WorldOutcome::CommandConsumed(reply) => {
            ("command-consumed", reply)
        }
        tiler_core::world_engine::WorldOutcome::Rejected { kind, message } => {
            // Unconditional bounded core-refusal row (trace-independent):
            // `kind`/`message` are the core fixed refusal vocabulary, never
            // payload bytes. Without this, a refused command (e.g. the live
            // `migrate-unverified` fence) leaves no `world-apply` row at all.
            super::trace::log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "world-apply",
                    "tick": tick,
                    "request": report.request_id.clone(),
                    "delivery": "rejected",
                    "command": command_kind,
                    "reject_kind": kind,
                    "reject_message": message,
                }),
            );
            world.note_rejected_or_lost(&report.request_id);
            return None;
        }
    };
    let inputs = TileLoop::owner_inputs(
        member_tokens,
        member_identity,
        member_tags,
        member_rects,
        float_rects,
        hint_logged,
        sticky,
        floated,
        restart_reject_hold,
        born_fullscreen,
        born_first_exit,
        fixed_auto_seen,
        fixed_tile_overrides,
        fixed_origin_overrides,
        hidden_claims,
        minimized_omitted,
        pending_closed,
        scope,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
    );
    let captured = capture_world(&inputs, Some(observed), retained, areas, foreground_hwnd);
    let inputs = TileLoop::owner_inputs(
        member_tokens,
        member_identity,
        member_tags,
        member_rects,
        float_rects,
        hint_logged,
        sticky,
        floated,
        restart_reject_hold,
        born_fullscreen,
        born_first_exit,
        fixed_auto_seen,
        fixed_tile_overrides,
        fixed_origin_overrides,
        hidden_claims,
        minimized_omitted,
        pending_closed,
        scope,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
    );
    let fresh = FreshHost::from_sources(&inputs, observed, retained, &captured);
    let (homing, rings, views) = world.core_projections();
    let proven_homing = applied_homing.clone();
    let proven_views = applied_views.clone();
    let mut desired_frames = HashMap::new();
    for row in &reply.windows {
        if !row.floating
            && let Some(target) = row.effective_frame
        {
            desired_frames.insert(row.lifetime.0.clone(), target);
        }
    }
    let mut native = LiveNative::assemble(
        me,
        fulls,
        output_tokens,
        homing,
        rings,
        views,
        desired_frames,
        areas.to_vec(),
        proven_homing,
        proven_views,
        member_tokens,
        member_tags,
        member_identity,
        hidden_claims,
        sticky,
        float_topmost_prev,
        scope,
        scope_hosts,
        allowlist.as_ref(),
        workspace_proof,
        store,
        dir,
        active,
        NativePrimitives::default(),
    );
    world.sync_diff_gate();
    let mut frame_attempts = Vec::new();
    let mut member_attempts = Vec::new();
    let mut view_attempts = Vec::new();
    let stats = world.apply_accepted(
        &reply,
        &fresh,
        &mut native,
        &mut frame_attempts,
        &mut member_attempts,
        &mut view_attempts,
        tick_ms,
    );
    log_apply_diag(
        &log_path,
        trace,
        tick,
        &reply,
        delivery,
        command_kind,
        &stats,
        &member_attempts,
        &view_attempts,
        fresh_route_value(
            &captured.focused_global,
            captured.cursor,
            captured.fresh_placement.as_wire_str(),
            captured.switch_target.as_wire_str(),
            world.engine().fresh_focus_trace(),
        ),
    );
    // Proven receipt snapshot plus raw-attempt post proofs (see the
    // observation path): re-proof of unchanged homing plus exact proven
    // arrivals with independent fresh revalidation. Never wholesale desire.
    let (mut proven_h, mut proven_v) = native.receipt_updates(&reply);
    merge_applied_transfers(
        &reply,
        &member_attempts,
        &view_attempts,
        &fresh,
        &mut native,
        &mut proven_h,
        &mut proven_v,
    );
    drop(native);
    for (token, home) in proven_h {
        applied_homing.insert(token, home);
    }
    for (output, view) in proven_v {
        applied_views.insert(output, view);
    }
    // Mechanical raw facts ride the next report exactly once per key
    // (latest wins); the core consumes each idempotently and reconciles
    // retained evidence across acknowledgement gaps.
    for attempt in frame_attempts {
        world.note_frame_attempt(attempt);
    }
    for attempt in member_attempts {
        world.note_member_attempt(attempt);
    }
    for attempt in view_attempts {
        world.note_view_attempt(attempt);
    }
    let mut pending = std::mem::take(pending_closed);
    let (ack_outcome, pruned) = world.ack(&reply, &mut pending);
    *pending_closed = pending;
    for token in pruned {
        applied_homing.remove(&token);
    }
    match ack_outcome {
        super::world_sequencer::AckOutcome::Applied(_) => {
            world.note_diff_applied(&reply);
            Some(reply)
        }
        super::world_sequencer::AckOutcome::Stale(_) => world.last_reply().cloned(),
        super::world_sequencer::AckOutcome::Ignored { .. } => None,
    }
}

/// Observe once through the production observation path and route one
/// [`WorldCommand`]. The caller supplies the current tick products;
/// enumeration failure (`None`) holds with no dispatch. Returns the
/// applied reply when the owner accepted it, so outcomes reflect the
/// actual command result instead of a cached reply.
pub(super) fn observe_and_command(
    state: &mut TileLoop,
    me: &crate::model::ProcessIdentity,
    store: &crate::storage::LedgerStore,
    dir: &std::path::Path,
    fulls: &[tiler_core::geometry::Rect],
    areas: &[MonitorArea],
    command: WorldCommand,
) -> Option<tiler_core::world_report::WorldReply> {
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let foreground = crate::ffi::foreground_window();
    // One normalized sample for this command snapshot (existing capture
    // sampling only; the focused-fullscreen-game lane never reaches here,
    // so no cursor work runs under it).
    let cursor = super::observation::sample_cursor();
    let observed = state.observe(me, fulls, &mut skipped, &mut retained)?;
    run_world_command(
        state, me, store, dir, &observed, &retained, areas, foreground, fulls, command, cursor,
    )
}

/// Test-only world seeding: dispatch one observation through the loop's
/// own [`WorldOwner`](super::world_owner::WorldOwner), deliver through the
/// single production path, and acknowledge. Plants genuine core homing,
/// rings, and views for member tables the test seeded (never fabricated
/// adapter state, never native effects): the offline stand-in for the
/// ticks that prove membership in production. Returns the accepted reply
/// when the sequencer applied it.
#[cfg(test)]
pub(super) fn dispatch_test_observation(
    state: &mut TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    areas: &[MonitorArea],
    foreground_hwnd: u64,
    cursor: Option<(i32, i32)>,
    monotonic_ms: u64,
) -> Option<tiler_core::world_report::WorldReply> {
    super::rows::refresh_world_hints(state, observed, retained);
    let TileLoop {
        ref mut world,
        ref member_tokens,
        ref member_identity,
        ref member_tags,
        ref member_rects,
        ref float_rects,
        ref hint_logged,
        ref sticky,
        ref floated,
        ref restart_reject_hold,
        ref born_fullscreen,
        ref born_first_exit,
        ref fixed_auto_seen,
        ref fixed_tile_overrides,
        ref fixed_origin_overrides,
        ref mut hidden_claims,
        ref minimized_omitted,
        ref mut pending_closed,
        ref scope,
        ref settings_live,
        ref output_tokens,
        ref mut applied_homing,
        ref mut applied_views,
        inner_gap,
        outer_gap,
        ref active,
        ..
    } = *state;
    world.invalidate();
    let inputs = TileLoop::owner_inputs(
        member_tokens,
        member_identity,
        member_tags,
        member_rects,
        float_rects,
        hint_logged,
        sticky,
        floated,
        restart_reject_hold,
        born_fullscreen,
        born_first_exit,
        fixed_auto_seen,
        fixed_tile_overrides,
        fixed_origin_overrides,
        hidden_claims,
        minimized_omitted,
        pending_closed,
        scope,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
    );
    let report = match world.try_dispatch_observation(
        &inputs,
        Some(observed),
        retained,
        areas,
        foreground_hwnd,
        monotonic_ms,
    ) {
        Ok(report) => report,
        Err(_) => return world.last_reply().cloned(),
    };
    let reply = match world.deliver_production(&report) {
        tiler_core::world_engine::WorldOutcome::Accepted(reply)
        | tiler_core::world_engine::WorldOutcome::Duplicate(reply)
        | tiler_core::world_engine::WorldOutcome::CommandConsumed(reply) => reply,
        tiler_core::world_engine::WorldOutcome::Rejected { .. } => {
            world.note_rejected_or_lost(&report.request_id);
            return None;
        }
    };
    let (ack_outcome, pruned) = world.ack(&reply, pending_closed);
    for token in pruned {
        applied_homing.remove(&token);
    }
    match ack_outcome {
        super::world_sequencer::AckOutcome::Applied(_) => {
            world.note_diff_applied(&reply);
            Some(reply)
        }
        super::world_sequencer::AckOutcome::Stale(_) => world.last_reply().cloned(),
        super::world_sequencer::AckOutcome::Ignored { .. } => None,
    }
}

#[cfg(test)]
mod apply_diag_tests {
    use super::*;
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::world_report::{HoldReason, ScopeAuthority, ScopeStatus, TransferTarget};

    fn reply_shell() -> WorldReply {
        WorldReply {
            contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            request_id: "req-9".to_owned(),
            based_on_observation_seq: 3,
            state_revision: 7,
            ready: true,
            command_consumed: false,
            scopes: vec![
                ScopeStatus {
                    scope: "o1/w1".to_owned(),
                    authority: ScopeAuthority::Controlled,
                },
                ScopeStatus {
                    scope: "o1/w2".to_owned(),
                    authority: ScopeAuthority::Hold(HoldReason::GestureBarrier),
                },
            ],
            workspaces: Vec::new(),
            outputs: Vec::new(),
            windows: Vec::new(),
            transfers: vec![TransferTarget {
                operation_id: "op-3".to_owned(),
                kind: TransferKind::Window(WindowId("w1".to_owned())),
                target_output: OutputId("o1".to_owned()),
                target_workspace: "w2".to_owned(),
                follow: false,
                held: false,
            }],
            effects: Vec::new(),
            command_result: None,
            presentation_bytes: 0,
        }
    }

    fn stats_shell() -> super::super::world_diff::ApplyStats {
        super::super::world_diff::ApplyStats {
            geometry_writes: 0,
            membership_writes: 1,
            view_writes: 0,
            creations: 0,
            removals: 0,
            effects: 0,
            held: 2,
            migration_total: 0,
            migration_proven: 0,
            stopped_mid_apply: false,
            partial: false,
        }
    }

    #[test]
    fn diag_shape_uses_fixed_vocab_and_bounds_rows() {
        use tiler_core::world_report::{FrameSetter, MemberAttempt, ViewAttempt};
        let reply = reply_shell();
        let stats = stats_shell();
        let setters = [
            FrameSetter::Accepted,
            FrameSetter::Refused,
            FrameSetter::Threw,
            FrameSetter::Accepted,
        ];
        let members: Vec<MemberAttempt> = (0..20)
            .map(|n| MemberAttempt {
                operation_id: format!("op-{n}"),
                window: WindowId(format!("w{n}")),
                target_output: OutputId("o1".to_owned()),
                target_workspace: "w2".to_owned(),
                setter: setters[n % setters.len()],
                pre: (n % 2 == 0).then(|| (OutputId("o1".to_owned()), format!("w{}", n % 3))),
                post: (n % 3 == 0).then(|| (OutputId("o1".to_owned()), "w2".to_owned())),
                attempt_ms: n as u64,
            })
            .collect();
        let views: Vec<ViewAttempt> = Vec::new();
        let value = apply_diag_value(
            11,
            &reply,
            "accepted",
            "send",
            &stats,
            &members,
            &views,
            fresh_route_value(
                "w1",
                Some((10, 20)),
                "focused-output-last-focused",
                "cursor-output",
                (
                    Some("o1".to_owned()),
                    Some("o2".to_owned()),
                    Some("o2".to_owned()),
                    "retained",
                ),
            ),
        );
        assert_eq!(value["event"], serde_json::json!("world-apply"));
        assert_eq!(value["tick"], serde_json::json!(11));
        assert_eq!(value["request"], serde_json::json!("req-9"));
        assert_eq!(value["revision"], serde_json::json!(7));
        assert_eq!(value["observation"], serde_json::json!(3));
        assert_eq!(value["delivery"], serde_json::json!("accepted"));
        assert_eq!(value["command"], serde_json::json!("send"));
        assert_eq!(value["command_result"], serde_json::json!("none"));
        assert_eq!(value["stats"]["membership_writes"], serde_json::json!(1));
        assert_eq!(value["stats"]["held"], serde_json::json!(2));
        assert_eq!(value["stats"]["migration_total"], serde_json::json!(0));
        assert_eq!(value["stats"]["migration_proven"], serde_json::json!(0));
        let rows = value["members"].as_array().expect("member rows");
        assert_eq!(rows.len(), MAX_APPLY_DIAG_FEEDBACK);
        assert_eq!(
            value["members_omitted"],
            serde_json::json!(20 - MAX_APPLY_DIAG_FEEDBACK)
        );
        assert_eq!(rows[0]["setter"], serde_json::json!("accepted"));
        assert_eq!(rows[1]["setter"], serde_json::json!("refused"));
        assert_eq!(rows[2]["setter"], serde_json::json!("threw"));
        assert_eq!(rows[0]["ws"], serde_json::json!("w2"));
        assert_eq!(rows[0]["pre"], serde_json::json!("o1/w0"));
        assert!(rows[1]["pre"].is_null());
        let view_rows = value["views"].as_array().expect("view rows");
        assert!(view_rows.is_empty());
        assert_eq!(value["views_omitted"], serde_json::json!(0));
        let scopes = value["held_scopes"].as_array().expect("held scopes");
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0]["scope"], serde_json::json!("o1/w2"));
        assert_eq!(scopes[0]["authority"], serde_json::json!("hold"));
        assert_eq!(scopes[0]["reason"], serde_json::json!("gesture-barrier"));
        let transfers = value["desired_transfers"]
            .as_array()
            .expect("desired transfers");
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0]["kind"], serde_json::json!("window"));
        assert_eq!(transfers[0]["target_workspace"], serde_json::json!("w2"));
        assert_eq!(value["desired_transfers_omitted"], serde_json::json!(0));
        assert_eq!(
            value["fresh_route"]["focused_token"],
            serde_json::json!("w1")
        );
        // `focused_reported` derives from the CURRENT token, never from
        // the cached leg: a nonempty current token reports true even when
        // the cached leg is historical (or default).
        assert_eq!(
            value["fresh_route"]["focused_reported"],
            serde_json::json!(true)
        );
        assert_eq!(value["fresh_route"]["cursor"], serde_json::json!([10, 20]));
        assert_eq!(
            value["fresh_route"]["fresh_placement"],
            serde_json::json!("focused-output-last-focused")
        );
        let leg = &value["fresh_route"]["last_focused_leg"];
        assert_eq!(leg["basis"], serde_json::json!("retained"));
        assert_eq!(leg["chosen"], serde_json::json!("o2"));
        assert_eq!(leg["current"], serde_json::json!("o1"));
        assert_eq!(leg["retained"], serde_json::json!("o2"));
        // Empty current token reports false while the cached leg stands
        // apart under its explicit label.
        let stale = fresh_route_value(
            "",
            None,
            "focused-output-last-focused",
            "cursor-output",
            (
                Some("o1".to_owned()),
                Some("o2".to_owned()),
                Some("o2".to_owned()),
                "retained",
            ),
        );
        assert_eq!(stale["focused_reported"], serde_json::json!(false));
        assert_eq!(
            stale["last_focused_leg"]["basis"],
            serde_json::json!("retained")
        );
        assert!(stale["last_focused_leg"].is_object());
    }

    #[test]
    fn diag_throttles_quiet_observation_ticks_only() {
        use tiler_core::world_report::{FrameSetter, MemberAttempt};
        let quiet = super::super::world_diff::ApplyStats::default();
        let empty_members: Vec<MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        assert!(!should_log_apply(
            "observation",
            &quiet,
            &empty_members,
            &empty_views
        ));
        // Pure-held observation ticks stay quiet: steady-state holds with no
        // attempt carry no new evidence.
        let held_only = super::super::world_diff::ApplyStats {
            held: 5,
            ..Default::default()
        };
        assert!(!should_log_apply(
            "observation",
            &held_only,
            &empty_members,
            &empty_views
        ));
        // Commands always log, so a no-attempt Hold on a command still shows
        // as `held > 0` with empty attempt rows.
        assert!(should_log_apply(
            "send",
            &held_only,
            &empty_members,
            &empty_views
        ));
        assert!(should_log_apply(
            "observation",
            &quiet,
            &[MemberAttempt {
                operation_id: "op-3".to_owned(),
                window: WindowId("w3".to_owned()),
                target_output: OutputId("o1".to_owned()),
                target_workspace: "w2".to_owned(),
                setter: FrameSetter::Threw,
                pre: None,
                post: None,
                attempt_ms: 3,
            }],
            &empty_views,
        ));
        let writes = super::super::world_diff::ApplyStats {
            geometry_writes: 1,
            ..Default::default()
        };
        assert!(should_log_apply(
            "observation",
            &writes,
            &empty_members,
            &empty_views
        ));
        let stopped = super::super::world_diff::ApplyStats {
            stopped_mid_apply: true,
            held: 1,
            ..Default::default()
        };
        assert!(should_log_apply(
            "observation",
            &stopped,
            &empty_members,
            &empty_views
        ));
    }

    #[test]
    fn diag_never_reports_applied_from_setter_counts() {
        // Setter-side write counts with no raw attempts must not fabricate
        // a proof row: only the diff's own attempt rows speak.
        let reply = reply_shell();
        let stats = super::super::world_diff::ApplyStats {
            geometry_writes: 3,
            held: 1,
            ..Default::default()
        };
        let empty_members: Vec<tiler_core::world_report::MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        let value = apply_diag_value(
            12,
            &reply,
            "accepted",
            "send",
            &stats,
            &empty_members,
            &empty_views,
            serde_json::json!({}),
        );
        let rows = value["members"].as_array().expect("member rows");
        assert!(rows.is_empty());
        assert_eq!(value["members_omitted"], serde_json::json!(0));
        assert_eq!(value["stats"]["geometry_writes"], serde_json::json!(3));
    }

    #[test]
    fn diag_command_kind_is_redacted_vocab() {
        let send = WorldCommand::Send {
            window: WindowId("w1".to_owned()),
            target_output: OutputId("o1".to_owned()),
            target_workspace: "w2".to_owned(),
            follow: true,
        };
        assert_eq!(world_command_kind(&send), "send");
        let reply = reply_shell();
        let stats = super::super::world_diff::ApplyStats::default();
        let value = apply_diag_value(
            13,
            &reply,
            "accepted",
            world_command_kind(&send),
            &stats,
            &[],
            &[],
            serde_json::json!({}),
        );
        assert_eq!(value["command"], serde_json::json!("send"));
        let rendered = value.to_string();
        assert!(!rendered.contains(".exe"));
    }
}
