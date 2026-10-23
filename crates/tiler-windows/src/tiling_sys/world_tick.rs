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
use super::{MonitorArea, ObservedWindow, RetainedRow, TileLoop, live_foreground_window};
use std::collections::HashMap;
use tiler_core::world_report::{CommandResult, TransferKind, WorldCommand, WorldReply};

impl TileLoop {
    /// Clear the stashed terminal override on genuine incorporation proof:
    /// a freshly accepted reply carrying a gesture terminal whose id
    /// matches the stashed override proves that terminal ran in core, so
    /// the override clears and no later tick replays it. Any other reply
    /// (none, no terminal, mismatched id) leaves the stash for the next
    /// eligible observation. Call only with a freshly incorporated reply,
    /// never a stale/duplicate cache. Takes the override slot directly so
    /// the tick's disjoint borrow of the world owner stays intact.
    pub(super) fn clear_gesture_override_on_incorporation(
        override_slot: &mut Option<tiler_core::gesture::GestureIngress>,
        reply: &WorldReply,
    ) {
        if let Some(terminal) = reply.gesture_settled
            && override_slot
                .as_ref()
                .is_some_and(|stashed| stashed.id == terminal.id)
        {
            *override_slot = None;
        }
    }
}

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
/// only raw attempt rows, held scopes, and retained transfer desires ride.
const MAX_APPLY_DIAG_ATTEMPTS: usize = 16;
const MAX_APPLY_DIAG_SCOPES: usize = 16;
const MAX_APPLY_DIAG_TRANSFERS: usize = 16;
/// Trace-only focus oracle cap: minted effects plus per-effect dispatch
/// rows stay bounded with honest `omitted` counts. Effects are already
/// bounded by the reply budget; this caps only the retained trace rows.
const MAX_APPLY_DIAG_FOCUS: usize = 8;

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
        Some(CommandResult::Directional { .. }) => "directional",
        Some(CommandResult::ActiveGroup { .. }) => "active-group",
        Some(CommandResult::NoGroup { .. }) => "no-group",
        Some(CommandResult::Migration { .. }) => "migration",
    }
}

/// Event-driven throttle: commands always log (the owner trace only runs on
/// dispatch), while observation ticks log only on change: staged raw
/// attempts (membership/view/clear/frame), any minted focus effect
/// (dispatched or held), a terminal retained-operation settlement event
/// (completion/refusal/abandonment/continuation, observation-only or not),
/// a changed hover triple (see `hover_changed`), or a stopped/partial pass.
/// Pure-idle observation ticks (no attempt, no minted effect, no settlement
/// event, unchanged hover) stay quiet; minting is transition/motion driven
/// so idle rows never re-log. A command that holds with no attempt still logs
/// on its own command row with `held > 0` and empty attempt rows, which is
/// exactly the no-attempt Hold signature. A held minted effect logs with its
/// individual hold reason, so dispatched-but-unproven and held-without-setter
/// are both distinguishable from idle quiet.
fn should_log_apply(
    command_kind: &str,
    stats: &super::world_diff::ApplyStats,
    member_attempts: &[tiler_core::world_report::MemberAttempt],
    view_attempts: &[tiler_core::world_report::ViewAttempt],
    clear_attempts: &[tiler_core::world_report::ClearAttempt],
    frame_attempts: &[tiler_core::world_report::FrameAttempt],
    reply: &WorldReply,
) -> bool {
    if command_kind != "observation" {
        return true;
    }
    if !member_attempts.is_empty()
        || !view_attempts.is_empty()
        || !clear_attempts.is_empty()
        || !frame_attempts.is_empty()
        || !stats.clear_holds.is_empty()
    {
        return true;
    }
    if !reply.operation_settled.is_empty() {
        return true;
    }
    if !stats.focus_dispatch.is_empty() {
        return true;
    }
    if stats.stopped_mid_apply || stats.partial {
        return true;
    }
    stats.geometry_writes
        + stats.membership_writes
        + stats.view_writes
        + stats.clear_writes
        + stats.creations
        + stats.removals
        + stats.effects
        + stats.cursor_warps
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

/// Trace-only rect rendering for the `world-apply` diag: `Rect` carries no
/// `Serialize` impl, so rows render the fixed `{x,y,w,h}` shape by hand.
fn rect_value(rect: tiler_core::geometry::Rect) -> serde_json::Value {
    serde_json::json!({
        "x": rect.x,
        "y": rect.y,
        "w": rect.w,
        "h": rect.h,
    })
}

/// Pure trace-row builder for one apply pass (TRACE-ONLY: no actuation
/// input, no retry, no interpretation). Raw attempt rows appear only when
/// the diff dispatched a setter this pass: setter-side write counts never
/// fabricate a proof row. Desired homing/views ride labeled `desired_*`,
/// never as observed proof. Pending window-operation desire (published
/// `frame_operation`/`clear_operation` tokens) rides labeled
/// `desired_operations`, one row per actual token; actual raw frame writes
/// ride `frames`; terminal core verdicts ride `operation_settled`. The three
/// stay distinct: desire is pending intent, attempts are mechanical facts,
/// events are terminal verdicts. An empty `desired_operations` on a fresh
/// accepted reply is truthfully no PUBLISHED operation desire; it never
/// claims an ambiguous internally-retained binding retired. Only terminal
/// events prove a retained clear/float binding retired (refusal,
/// abandonment, supersession, continuation, or frame-proven float
/// completion). The only refusal detail available from existing values is
/// the raw setter vocabulary: the [`NativeOps`] contract surfaces explicit
/// refusal as `Ok(false)` and torn state as `Err` without a native errno,
/// so deeper refusal detail would need a broader seam change (reported as a
/// limitation, not papered over here).
// Thirteen inputs are the disjoint trace context plus the pass products;
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
    clear_attempts: &[tiler_core::world_report::ClearAttempt],
    frame_attempts: &[tiler_core::world_report::FrameAttempt],
    fresh_route: serde_json::Value,
    cursor_hit: Option<String>,
    ffc_enabled: bool,
) -> serde_json::Value {
    let member_rows: Vec<serde_json::Value> = member_attempts
        .iter()
        .take(MAX_APPLY_DIAG_ATTEMPTS)
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
        .take(MAX_APPLY_DIAG_ATTEMPTS)
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
    let clear_rows: Vec<serde_json::Value> = clear_attempts
        .iter()
        .take(MAX_APPLY_DIAG_ATTEMPTS)
        .map(|item| {
            serde_json::json!({
                "op": item.operation_id.clone(),
                "window": item.window.0.clone(),
                "setter": item.setter.as_str(),
                "pre": item.pre_maximized,
                "post": item.post_maximized,
            })
        })
        .collect();
    // Trace-only per-clear hold attribution for this pass: one row per
    // published clear desire that dispatched no setter, naming the exact
    // pre-dispatch gate. Bounded with an honest omitted count derived
    // from the unbounded `clear_holds_total`, never the truncated stored
    // rows. Veto holds ride alongside their correlated refused attempt in
    // `clears` above; silent holds ride with no attempt and no spent mark.
    let clear_hold_rows: Vec<serde_json::Value> = stats
        .clear_holds
        .iter()
        .take(MAX_APPLY_DIAG_ATTEMPTS)
        .map(|item| {
            serde_json::json!({
                "op": item.op.clone(),
                "window": item.window.clone(),
                "hold": item.hold,
            })
        })
        .collect();
    // Terminal retained-operation lifecycle events for this reply (one per
    // retired binding, with original op/window/outcome): pending desire
    // rides `desired_operations` below, actual raw attempts ride `frames`
    // (float placements) and `clears` (maximize clears) above, holds emit
    // nothing, and frame-proven float completion rides the float settlement
    // outcome. Covers both clear and float bindings (continuation,
    // refusal, abandonment, supersession, float completion). Never a
    // success claim: `continued-tiled`/`continued-float` mean the intent
    // resumed, never that the final frame arrived.
    let operation_settled_rows: Vec<serde_json::Value> = reply
        .operation_settled
        .iter()
        .take(MAX_APPLY_DIAG_ATTEMPTS)
        .map(|event| {
            serde_json::json!({
                "op": event.operation_id.clone(),
                "window": event.window.0.clone(),
                "outcome": event.outcome.as_str(),
                "cause": event.cause,
            })
        })
        .collect();
    // Actual raw frame writes this pass produced, bounded with an honest
    // omitted count: operation id (null for ordinary writes with no
    // placement authority) plus window/target/setter/prewrite/readback/DPI
    // pair so a live leg can join the attempt to its published desire.
    let frame_rows: Vec<serde_json::Value> = frame_attempts
        .iter()
        .take(MAX_APPLY_DIAG_ATTEMPTS)
        .map(|item| {
            serde_json::json!({
                "op": item.operation_id.clone(),
                "window": item.window.0.clone(),
                "target": rect_value(item.target),
                "setter": item.setter.as_str(),
                "prewrite": item.prewrite.map(rect_value),
                "readback": item.readback.map(rect_value),
                "dpi_before": item.dpi_before,
                "dpi_after": item.dpi_after,
                "attempt_ms": item.attempt_ms,
            })
        })
        .collect();
    // Published pending window-operation desire: one row per actual
    // `frame_operation`/`clear_operation` token in window order (frame then
    // clear per window), with the frame target when already available.
    // Bounded with an honest omitted count. Empty on a fresh accepted reply
    // is truthfully no published operation; it never claims an ambiguous
    // internally-retained binding retired (only terminal events prove that).
    let desired_operation_total: usize = reply
        .windows
        .iter()
        .map(|row| {
            usize::from(
                row.frame_operation
                    .as_deref()
                    .is_some_and(|op| !op.is_empty()),
            ) + usize::from(
                row.clear_operation
                    .as_deref()
                    .is_some_and(|op| !op.is_empty()),
            )
        })
        .sum();
    let desired_operation_rows: Vec<serde_json::Value> = reply
        .windows
        .iter()
        .flat_map(|row| {
            let mut rows = Vec::with_capacity(2);
            if let Some(op) = row.frame_operation.as_deref()
                && !op.is_empty()
            {
                rows.push(serde_json::json!({
                    "op": op,
                    "window": row.lifetime.0.clone(),
                    "kind": "frame",
                    "target": row.effective_frame.map(rect_value),
                }));
            }
            if let Some(op) = row.clear_operation.as_deref()
                && !op.is_empty()
            {
                rows.push(serde_json::json!({
                    "op": op,
                    "window": row.lifetime.0.clone(),
                    "kind": "clear",
                }));
            }
            rows
        })
        .take(MAX_APPLY_DIAG_ATTEMPTS)
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
    let focus_effects: Vec<serde_json::Value> = reply
        .effects
        .iter()
        .take(MAX_APPLY_DIAG_FOCUS)
        .map(|effect| {
            serde_json::json!({
                "id": effect.id.clone(),
                "kind": effect.kind.as_str(),
                "target": effect.target.0.clone(),
            })
        })
        .collect();
    let focus_dispatch: Vec<serde_json::Value> = stats
        .focus_dispatch
        .iter()
        .take(MAX_APPLY_DIAG_FOCUS)
        .map(|row| {
            serde_json::json!({
                "id": row.id.clone(),
                "target": row.target.clone(),
                "kind": row.kind,
                "dispatched": row.dispatched,
                "setter": row.setter,
                "foreground": row.foreground,
                "hold": row.hold,
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
            "clear_writes": stats.clear_writes,
            "creations": stats.creations,
            "removals": stats.removals,
            "effects": stats.effects,
            "cursor_warps": stats.cursor_warps,
            "held": stats.held,
            "migration_total": stats.migration_total,
            "migration_proven": stats.migration_proven,
            "stopped_mid_apply": stats.stopped_mid_apply,
            "partial": stats.partial,
        },
        "cursor_hit": cursor_hit,
        "ffc_enabled": ffc_enabled,
        "members": member_rows,
        "members_omitted": member_attempts
            .len()
            .saturating_sub(MAX_APPLY_DIAG_ATTEMPTS.min(member_attempts.len())),
        "views": view_rows,
        "views_omitted": view_attempts
            .len()
            .saturating_sub(MAX_APPLY_DIAG_ATTEMPTS.min(view_attempts.len())),
        "clears": clear_rows,
        "clears_omitted": clear_attempts
            .len()
            .saturating_sub(MAX_APPLY_DIAG_ATTEMPTS.min(clear_attempts.len())),
        "clear_holds": clear_hold_rows,
        "clear_holds_omitted": stats
            .clear_holds_total
            .saturating_sub(stats.clear_holds.len().min(MAX_APPLY_DIAG_ATTEMPTS)),
        "frames": frame_rows,
        "frames_omitted": frame_attempts
            .len()
            .saturating_sub(frame_rows.len()),
        "desired_operations": desired_operation_rows,
        "desired_operations_omitted": desired_operation_total
            .saturating_sub(desired_operation_rows.len()),
        "operation_settled": operation_settled_rows,
        "operation_settled_omitted": reply
            .operation_settled
            .len()
            .saturating_sub(operation_settled_rows.len()),
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
        "focus_effects": focus_effects,
        "focus_effects_omitted": reply
            .effects
            .len()
            .saturating_sub(focus_effects.len()),
        "focus_dispatch": focus_dispatch,
        "focus_dispatch_omitted": reply
            .effects
            .len()
            .saturating_sub(focus_dispatch.len()),
        "fresh_route": fresh_route,
    })
}

/// Trace-only emission for one apply pass. Event-driven owner trace only:
/// no input-hook cost, no native re-read, no per-window dump. Returns true
/// when a row was emitted (the caller records the hover triple then, never
/// on quiet ticks).
// Thirteen inputs are the disjoint trace context plus the pass products
// (reply, delivery, command kind, stats, raw attempts, cursor-hit/FFC,
// hover-change); bundling would hide the call-site borrow split the tick
// relies on.
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
    clear_attempts: &[tiler_core::world_report::ClearAttempt],
    frame_attempts: &[tiler_core::world_report::FrameAttempt],
    fresh_route: serde_json::Value,
    cursor_hit: Option<String>,
    ffc_enabled: bool,
    hover_changed: bool,
) -> bool {
    if !trace
        || !(should_log_apply(
            command_kind,
            stats,
            member_attempts,
            view_attempts,
            clear_attempts,
            frame_attempts,
            reply,
        ) || hover_changed)
    {
        return false;
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
            clear_attempts,
            frame_attempts,
            fresh_route,
            cursor_hit,
            ffc_enabled,
        ),
    );
    true
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
    // Normalized gesture ingress rides this tick's capture (stage 4a):
    // computed before the disjoint split borrows the loop.
    let gesture = state.gesture_ingress(cursor);
    // Raw cursor-hit HWND for the same tick as the cursor sample: the
    // exact managed binding resolves in capture over the owner tables.
    // `None` whenever the cursor itself is unsampled (game lane,
    // unreadable), so a hit never travels without its position.
    let hit_hwnd = super::observation::cursor_hit_hwnd(cursor);
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
        ref active_output,
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
        active_output.as_str(),
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
        hit_hwnd,
        gesture.clone(),
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
        active_output.as_str(),
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
        hit_hwnd,
        gesture.clone(),
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
        active_output.as_str(),
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
        hit_hwnd,
        gesture.clone(),
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
    let mut clear_attempts = Vec::new();
    let stats = world.apply_accepted(
        &reply,
        &fresh,
        &mut native,
        &mut frame_attempts,
        &mut member_attempts,
        &mut view_attempts,
        &mut clear_attempts,
        tick_ms,
    );
    let hover_hit = captured.cursor_hit.as_ref().map(|hit| hit.0.clone());
    let hover_changed = world.hover_trace_changed(
        hover_hit.as_deref(),
        &captured.focused_global,
        captured.focus_follows_cursor,
    );
    let emitted = log_apply_diag(
        &log_path,
        trace,
        tick,
        &reply,
        delivery,
        "observation",
        &stats,
        &member_attempts,
        &view_attempts,
        &clear_attempts,
        &frame_attempts,
        fresh_route_value(
            &captured.focused_global,
            captured.cursor,
            captured.fresh_placement.as_wire_str(),
            captured.switch_target.as_wire_str(),
            world.engine().fresh_focus_trace(),
        ),
        hover_hit,
        captured.focus_follows_cursor,
        hover_changed,
    );
    // Record the hover triple only for an emitted row, never on quiet
    // ticks, so a changed hover keeps triggering until it is on the trace.
    if emitted {
        world.note_hover_traced(
            captured.cursor_hit.as_ref().map(|hit| hit.0.clone()),
            captured.focused_global.clone(),
            captured.focus_follows_cursor,
        );
    }
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
    for attempt in clear_attempts {
        world.note_clear_attempt(attempt);
    }
    let mut pending = std::mem::take(pending_closed);
    let (ack_outcome, pruned) = world.ack(&reply, &mut pending);
    *pending_closed = pending;
    for token in pruned {
        applied_homing.remove(&token);
    }
    // Only a freshly accepted reply claims incorporation: stale and
    // duplicate deliveries never stand in. A stashed terminal survives
    // transient busy/gap ticks until an eligible observation runs it. A
    // fresh run clears a matching stash even when the ack races stale:
    // incorporation already happened, only the claim is withheld.
    let fresh_run = delivery != "duplicate";
    if fresh_run {
        TileLoop::clear_gesture_override_on_incorporation(&mut state.gesture_override, &reply);
    }
    match ack_outcome {
        super::world_sequencer::AckOutcome::Applied(_) => {
            world.note_diff_applied(&reply);
            Some(reply)
        }
        super::world_sequencer::AckOutcome::Stale(_) => None,
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
    // Normalized gesture ingress rides this tick's capture (stage 4a):
    // computed before the disjoint split borrows the loop.
    let gesture = state.gesture_ingress(cursor);
    // Raw cursor-hit HWND for the same tick as the cursor sample: the
    // exact managed binding resolves in capture over the owner tables.
    // `None` whenever the cursor itself is unsampled (game lane,
    // unreadable), so a hit never travels without its position.
    let hit_hwnd = super::observation::cursor_hit_hwnd(cursor);
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
        ref active_output,
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
            active_output.as_str(),
            applied_homing,
            applied_views,
            inner_gap,
            outer_gap,
            active,
            cursor,
            hit_hwnd,
            gesture.clone(),
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
        active_output.as_str(),
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
        hit_hwnd,
        gesture.clone(),
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
        active_output.as_str(),
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
        hit_hwnd,
        gesture.clone(),
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
    let mut clear_attempts = Vec::new();
    let stats = world.apply_accepted(
        &reply,
        &fresh,
        &mut native,
        &mut frame_attempts,
        &mut member_attempts,
        &mut view_attempts,
        &mut clear_attempts,
        tick_ms,
    );
    let hover_hit = captured.cursor_hit.as_ref().map(|hit| hit.0.clone());
    let hover_changed = world.hover_trace_changed(
        hover_hit.as_deref(),
        &captured.focused_global,
        captured.focus_follows_cursor,
    );
    let emitted = log_apply_diag(
        &log_path,
        trace,
        tick,
        &reply,
        delivery,
        command_kind,
        &stats,
        &member_attempts,
        &view_attempts,
        &clear_attempts,
        &frame_attempts,
        fresh_route_value(
            &captured.focused_global,
            captured.cursor,
            captured.fresh_placement.as_wire_str(),
            captured.switch_target.as_wire_str(),
            world.engine().fresh_focus_trace(),
        ),
        hover_hit,
        captured.focus_follows_cursor,
        hover_changed,
    );
    if emitted {
        world.note_hover_traced(
            captured.cursor_hit.as_ref().map(|hit| hit.0.clone()),
            captured.focused_global.clone(),
            captured.focus_follows_cursor,
        );
    }
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
    for attempt in clear_attempts {
        world.note_clear_attempt(attempt);
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
            // Incorporation proof only on a fresh run; a stale cached
            // reply never clears the stashed terminal.
            if delivery != "duplicate" {
                TileLoop::clear_gesture_override_on_incorporation(
                    &mut state.gesture_override,
                    &reply,
                );
            }
            Some(reply)
        }
        // Stale command deliveries keep the existing cached-reply contract
        // (callers resubmit after the next observation tick runs the
        // owed catch-up); only the observation tick refuses to equate a
        // cached reply with a fresh run. A fresh run still clears a
        // matching stash even when the ack races stale.
        super::world_sequencer::AckOutcome::Stale(_) => {
            if delivery != "duplicate" {
                TileLoop::clear_gesture_override_on_incorporation(
                    &mut state.gesture_override,
                    &reply,
                );
            }
            world.last_reply().cloned()
        }
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
    // One normalized sample for this command snapshot (existing capture
    // sampling only; the focused-fullscreen-game lane never reaches here,
    // so no cursor work runs under it).
    let cursor = super::observation::sample_cursor();
    observe_and_command_at(state, me, store, dir, fulls, areas, command, cursor)
}

/// Observe once and route one [`WorldCommand`] with a caller-supplied
/// cursor sample (one sample per dispatch, matching the retired
/// per-chord sampling). Used by migration dispatch so tests script the
/// pointer leg deterministically; all other routes sample internally.
#[allow(clippy::too_many_arguments)]
pub(super) fn observe_and_command_at(
    state: &mut TileLoop,
    me: &crate::model::ProcessIdentity,
    store: &crate::storage::LedgerStore,
    dir: &std::path::Path,
    fulls: &[tiler_core::geometry::Rect],
    areas: &[MonitorArea],
    command: WorldCommand,
    cursor: Option<(i32, i32)>,
) -> Option<tiler_core::world_report::WorldReply> {
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    // Seam-aware foreground read (production-identical; scripted in unit
    // tests): the neutral action-foreground fact grounds from this HWND.
    let foreground = live_foreground_window();
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
    // Test seeding carries the loop's gesture tables as ingress (same as
    // production): tests needing terminal phases set the override slot
    // before dispatching.
    let gesture = state.gesture_ingress(cursor);
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
        ref active_output,
        ref mut applied_homing,
        ref mut applied_views,
        inner_gap,
        outer_gap,
        ref active,
        ..
    } = *state;
    world.invalidate();
    // Offline seeding carries no native hit read: tests set
    // `inputs.hit_hwnd` explicitly when they need a hit.
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
        active_output.as_str(),
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        cursor,
        None,
        gesture.clone(),
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
    let (ack_outcome, pruned) = world.ack(&reply, pending_closed);
    for token in pruned {
        applied_homing.remove(&token);
    }
    // Only a freshly accepted reply claims incorporation: stale and
    // duplicate deliveries never stand in. A stashed terminal survives
    // transient busy/gap ticks until an eligible observation runs it. A
    // fresh run clears a matching stash even when the ack races stale:
    // incorporation already happened, only the claim is withheld.
    let fresh_run = delivery != "duplicate";
    if fresh_run {
        TileLoop::clear_gesture_override_on_incorporation(&mut state.gesture_override, &reply);
    }
    match ack_outcome {
        super::world_sequencer::AckOutcome::Applied(_) => {
            world.note_diff_applied(&reply);
            Some(reply)
        }
        super::world_sequencer::AckOutcome::Stale(_) => None,
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
            gesture_settled: None,
            operation_settled: Vec::new(),
            cursor_warp: None,
        }
    }

    fn stats_shell() -> super::super::world_diff::ApplyStats {
        super::super::world_diff::ApplyStats {
            geometry_writes: 0,
            membership_writes: 1,
            view_writes: 0,
            clear_writes: 0,
            creations: 0,
            removals: 0,
            effects: 0,
            cursor_warps: 0,
            held: 2,
            migration_total: 0,
            migration_proven: 0,
            stopped_mid_apply: false,
            partial: false,
            focus_dispatch: Vec::new(),
            clear_holds: Vec::new(),
            clear_holds_total: 0,
        }
    }

    #[test]
    fn diag_operation_settled_logs_original_op_window_outcome_cause() {
        // Terminal operation verdicts ride the trace with their original
        // op/window/outcome/cause: clear continuations (intent resumed,
        // never frame proof), clear refusals, and float frame-proven
        // completions share the one carrier. Bounds stay honest.
        use tiler_core::world_report::{
            ClearAttempt, FrameSetter, OperationEvent, OperationOutcome,
        };
        let mut reply = reply_shell();
        reply.operation_settled = vec![
            OperationEvent {
                operation_id: "op-13".to_owned(),
                window: WindowId("w1".to_owned()),
                outcome: OperationOutcome::ClearContinuedTiled,
                cause: None,
            },
            OperationEvent {
                operation_id: "op-14".to_owned(),
                window: WindowId("w2".to_owned()),
                outcome: OperationOutcome::ClearContinuationRefused,
                cause: Some("constraints".to_owned()),
            },
            OperationEvent {
                operation_id: "op-15".to_owned(),
                window: WindowId("w3".to_owned()),
                outcome: OperationOutcome::FloatFrameProven,
                cause: None,
            },
        ];
        let clears = vec![ClearAttempt {
            operation_id: "op-13".to_owned(),
            window: WindowId("w1".to_owned()),
            setter: FrameSetter::Accepted,
            pre_maximized: Some(true),
            post_maximized: Some(false),
            attempt_ms: 9,
        }];
        let value = apply_diag_value(
            12,
            &reply,
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &clears,
            &[],
            fresh_route_value(
                "w1",
                None,
                "focused-output-last-focused",
                "cursor-output",
                (None, None, None, "retained"),
            ),
            None,
            false,
        );
        // Raw attempts and terminal verdicts stay distinct rows: the
        // attempt is mechanical proof, the event is the core verdict.
        let attempt_rows = value["clears"].as_array().expect("clear rows");
        assert_eq!(attempt_rows.len(), 1);
        assert_eq!(attempt_rows[0]["op"], serde_json::json!("op-13"));
        assert_eq!(attempt_rows[0]["setter"], serde_json::json!("accepted"));
        assert_eq!(value["clears_omitted"], serde_json::json!(0));
        let settled = value["operation_settled"].as_array().expect("settled rows");
        assert_eq!(settled.len(), 3);
        assert_eq!(settled[0]["op"], serde_json::json!("op-13"));
        assert_eq!(settled[0]["window"], serde_json::json!("w1"));
        assert_eq!(settled[0]["outcome"], serde_json::json!("continued-tiled"));
        assert!(settled[0]["cause"].is_null());
        assert_eq!(
            settled[1]["outcome"],
            serde_json::json!("continuation-refused")
        );
        assert_eq!(settled[1]["cause"], serde_json::json!("constraints"));
        assert_eq!(
            settled[2]["outcome"],
            serde_json::json!("float-frame-proven")
        );
        assert_eq!(value["operation_settled_omitted"], serde_json::json!(0));
        // No frame writes and no published window-operation desire on this
        // reply: empty arrays with zero omission, never absent fields.
        assert_eq!(value["frames"], serde_json::json!([]));
        assert_eq!(value["frames_omitted"], serde_json::json!(0));
        assert_eq!(value["desired_operations"], serde_json::json!([]));
        assert_eq!(value["desired_operations_omitted"], serde_json::json!(0));
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
            &[],
            &[],
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
            Some("w2".to_owned()),
            true,
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
        assert_eq!(rows.len(), MAX_APPLY_DIAG_ATTEMPTS);
        assert_eq!(
            value["members_omitted"],
            serde_json::json!(20 - MAX_APPLY_DIAG_ATTEMPTS)
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
        // Stage 4b FFC oracle: the exact reported hit plus the FFC switch flag
        // ride trace-only, never as policy input.
        assert_eq!(value["cursor_hit"], serde_json::json!("w2"));
        assert_eq!(value["ffc_enabled"], serde_json::json!(true));
        // No minted effects or dispatch rows on this transfer-only reply:
        // empty arrays with zero omission, never absent fields.
        assert_eq!(value["focus_effects"], serde_json::json!([]));
        assert_eq!(value["focus_effects_omitted"], serde_json::json!(0));
        assert_eq!(value["focus_dispatch"], serde_json::json!([]));
        assert_eq!(value["focus_dispatch_omitted"], serde_json::json!(0));
        // No frame writes and no published window-operation desire on this
        // reply (no window rows carry tokens): empty with zero omission.
        assert_eq!(value["frames"], serde_json::json!([]));
        assert_eq!(value["frames_omitted"], serde_json::json!(0));
        assert_eq!(value["desired_operations"], serde_json::json!([]));
        assert_eq!(value["desired_operations_omitted"], serde_json::json!(0));
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
        use tiler_core::world_report::{FrameAttempt, FrameSetter, MemberAttempt};
        let quiet = super::super::world_diff::ApplyStats::default();
        let empty_members: Vec<MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        let empty_frames: Vec<FrameAttempt> = Vec::new();
        let idle_reply = reply_shell();
        assert!(!should_log_apply(
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
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
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
        ));
        // Commands always log, so a no-attempt Hold on a command still shows
        // as `held > 0` with empty attempt rows.
        assert!(should_log_apply(
            "send",
            &held_only,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
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
            &[],
            &empty_frames,
            &idle_reply,
        ));
        let writes = super::super::world_diff::ApplyStats {
            geometry_writes: 1,
            ..Default::default()
        };
        assert!(should_log_apply(
            "observation",
            &writes,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
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
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
        ));
        // Any minted focus effect logs on observation ticks, dispatched or
        // held: the individual hold reason must reach the trace in the
        // exact FFC observation-only scenario. Idle ticks with no minted
        // effect stay quiet below.
        let dispatched_unproven = super::super::world_diff::ApplyStats {
            held: 1,
            focus_dispatch: vec![super::super::world_diff::FocusDispatchDiag {
                id: "ffc-w1".to_owned(),
                target: "w1".to_owned(),
                kind: "focus",
                dispatched: true,
                setter: "refused",
                foreground: false,
                hold: Some("foreground-unproven"),
            }],
            ..Default::default()
        };
        assert!(should_log_apply(
            "observation",
            &dispatched_unproven,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
        ));
        let held_no_setter = super::super::world_diff::ApplyStats {
            held: 1,
            focus_dispatch: vec![super::super::world_diff::FocusDispatchDiag {
                id: "ffc-w1".to_owned(),
                target: "w1".to_owned(),
                kind: "focus",
                dispatched: false,
                setter: "skipped",
                foreground: false,
                hold: Some("view-not-shown"),
            }],
            ..Default::default()
        };
        // Regression: a held minted effect is emitted, not lost to quiet.
        assert!(should_log_apply(
            "observation",
            &held_no_setter,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
        ));
        // Settlement-only observation ticks log: an observation-only
        // completion/refusal/abandonment carries no attempt and no write,
        // but the terminal verdict is new evidence and must reach the trace.
        let mut settled_reply = reply_shell();
        settled_reply
            .operation_settled
            .push(tiler_core::world_report::OperationEvent {
                operation_id: "op-14".to_owned(),
                window: WindowId("w2".to_owned()),
                outcome: tiler_core::world_report::OperationOutcome::ClearContinuationRefused,
                cause: Some("constraints".to_owned()),
            });
        assert!(should_log_apply(
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &settled_reply,
        ));
        // Frame-only observation ticks log: a fresh raw frame write with no
        // other evidence (e.g. the float half-snap native arrival) must
        // emit, so the attempt joins its desire on the trace.
        let frame_only = vec![FrameAttempt {
            window: WindowId("w9".to_owned()),
            target: tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 960,
                h: 1040,
            },
            setter: FrameSetter::Accepted,
            readback: None,
            dpi_before: None,
            dpi_after: None,
            attempt_ms: 5,
            operation_id: Some("op-20".to_owned()),
            prewrite: None,
        }];
        assert!(should_log_apply(
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &frame_only,
            &idle_reply,
        ));
    }

    #[test]
    fn diag_held_focus_dispatch_carries_hold_reason() {
        // The held observation oracle: a minted-but-held FFC effect rides
        // with its individual hold reason, so the live leg can attribute a
        // quiet native focus to a held dispatch rather than an absent hit.
        let mut reply = reply_shell();
        reply.effects.push(tiler_core::world_report::OneShotEffect {
            id: "ffc-w1".to_owned(),
            kind: tiler_core::world_report::EffectKind::Focus,
            target: WindowId("w1".to_owned()),
            issued_revision: 1,
        });
        let stats = super::super::world_diff::ApplyStats {
            held: 1,
            focus_dispatch: vec![super::super::world_diff::FocusDispatchDiag {
                id: "ffc-w1".to_owned(),
                target: "w1".to_owned(),
                kind: "focus",
                dispatched: false,
                setter: "skipped",
                foreground: false,
                hold: Some("view-not-shown"),
            }],
            ..Default::default()
        };
        let value = apply_diag_value(
            15,
            &reply,
            "accepted",
            "observation",
            &stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            Some("w1".to_owned()),
            true,
        );
        assert_eq!(value["cursor_hit"], serde_json::json!("w1"));
        let minted = value["focus_effects"].as_array().expect("effects");
        assert_eq!(minted.len(), 1);
        assert_eq!(minted[0]["id"], serde_json::json!("ffc-w1"));
        let dispatch = value["focus_dispatch"].as_array().expect("dispatch");
        assert_eq!(dispatch.len(), 1);
        assert_eq!(dispatch[0]["dispatched"], serde_json::json!(false));
        assert_eq!(dispatch[0]["setter"], serde_json::json!("skipped"));
        assert_eq!(dispatch[0]["foreground"], serde_json::json!(false));
        assert_eq!(dispatch[0]["hold"], serde_json::json!("view-not-shown"));
    }

    #[test]
    fn diag_clear_hold_oracle_logs_gate_on_quiet_observation() {
        // The clear-lane attribution oracle: a published clear desire held
        // with no setter rides the trace with its exact gate, and the
        // observation tick carrying it logs instead of staying quiet.
        let reply = reply_shell();
        let empty_members: Vec<tiler_core::world_report::MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        let empty_frames: Vec<tiler_core::world_report::FrameAttempt> = Vec::new();
        let idle_reply = reply_shell();
        let held_clear = super::super::world_diff::ApplyStats {
            held: 1,
            clear_holds_total: 1,
            clear_holds: vec![super::super::world_diff::ClearHoldDiag {
                op: "op-15".to_owned(),
                window: "w3".to_owned(),
                hold: "frame-race",
            }],
            ..Default::default()
        };
        assert!(should_log_apply(
            "observation",
            &held_clear,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
        ));
        let value = apply_diag_value(
            16,
            &reply,
            "accepted",
            "observation",
            &held_clear,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            serde_json::json!({}),
            None,
            false,
        );
        let holds = value["clear_holds"].as_array().expect("clear holds");
        assert_eq!(holds.len(), 1);
        assert_eq!(holds[0]["op"], serde_json::json!("op-15"));
        assert_eq!(holds[0]["window"], serde_json::json!("w3"));
        assert_eq!(holds[0]["hold"], serde_json::json!("frame-race"));
        assert_eq!(value["clear_holds_omitted"], serde_json::json!(0));
        assert_eq!(value["clears"], serde_json::json!([]));
    }

    #[test]
    fn diag_clear_holds_bound_rows_and_omit_honestly_past_cap() {
        // Past-cap regression for the reviewed defect: the omitted count
        // derives from the unbounded total, never the truncated storage,
        // so a pass with more held desires than stored rows still reports
        // the excess instead of a false zero.
        let reply = reply_shell();
        let mut stats = super::super::world_diff::ApplyStats {
            held: 20,
            ..Default::default()
        };
        for n in 0..20 {
            stats.note_clear_hold(&format!("op-{n}"), "w3", "frame-race");
        }
        assert_eq!(
            stats.clear_holds.len(),
            super::super::world_diff::MAX_CLEAR_HOLD_DIAG
        );
        assert_eq!(stats.clear_holds_total, 20);
        let value = apply_diag_value(
            17,
            &reply,
            "accepted",
            "observation",
            &stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
        );
        let holds = value["clear_holds"].as_array().expect("clear holds");
        assert_eq!(holds.len(), super::super::world_diff::MAX_CLEAR_HOLD_DIAG);
        assert_eq!(holds[0]["op"], serde_json::json!("op-0"));
        assert_eq!(
            value["clear_holds_omitted"],
            serde_json::json!(20 - super::super::world_diff::MAX_CLEAR_HOLD_DIAG)
        );
    }

    #[test]
    fn diag_focus_oracle_bounds_rows_and_omits_honestly() {
        // Minted FFC effects plus per-effect dispatch rows ride bounded
        // with honest omission: the oracle distinguishes dispatched from
        // held without a per-window dump.
        use tiler_core::world_report::OneShotEffect;
        let mut reply = reply_shell();
        reply.effects = (0..10)
            .map(|n| OneShotEffect {
                id: format!("ffc-w{n}"),
                kind: tiler_core::world_report::EffectKind::Focus,
                target: WindowId(format!("w{n}")),
                issued_revision: 1,
            })
            .collect();
        let stats = super::super::world_diff::ApplyStats {
            effects: 1,
            held: 9,
            focus_dispatch: (0..10)
                .map(|n| super::super::world_diff::FocusDispatchDiag {
                    id: format!("ffc-w{n}"),
                    target: format!("w{n}"),
                    kind: "focus",
                    dispatched: n == 0,
                    setter: if n == 0 { "accepted" } else { "skipped" },
                    foreground: n == 0,
                    hold: if n == 0 { None } else { Some("view-not-shown") },
                })
                .collect(),
            ..Default::default()
        };
        let value = apply_diag_value(
            14,
            &reply,
            "accepted",
            "observation",
            &stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            Some("w0".to_owned()),
            true,
        );
        assert_eq!(value["cursor_hit"], serde_json::json!("w0"));
        assert_eq!(value["ffc_enabled"], serde_json::json!(true));
        let minted = value["focus_effects"].as_array().expect("effects");
        assert_eq!(minted.len(), MAX_APPLY_DIAG_FOCUS);
        assert_eq!(minted[0]["id"], serde_json::json!("ffc-w0"));
        assert_eq!(minted[0]["target"], serde_json::json!("w0"));
        assert_eq!(minted[0]["kind"], serde_json::json!("focus"));
        assert_eq!(value["focus_effects_omitted"], serde_json::json!(2));
        let dispatch = value["focus_dispatch"].as_array().expect("dispatch");
        assert_eq!(dispatch.len(), MAX_APPLY_DIAG_FOCUS);
        assert_eq!(dispatch[0]["dispatched"], serde_json::json!(true));
        assert_eq!(dispatch[0]["setter"], serde_json::json!("accepted"));
        // Immediate foreground proof rides even when false: the held row
        // below carries explicit false, never a missing field.
        assert_eq!(dispatch[0]["foreground"], serde_json::json!(true));
        assert!(dispatch[0]["hold"].is_null());
        assert_eq!(dispatch[1]["dispatched"], serde_json::json!(false));
        assert_eq!(dispatch[1]["foreground"], serde_json::json!(false));
        assert_eq!(dispatch[1]["hold"], serde_json::json!("view-not-shown"));
        assert_eq!(value["focus_dispatch_omitted"], serde_json::json!(2));
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
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
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
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
        );
        assert_eq!(value["command"], serde_json::json!("send"));
        let rendered = value.to_string();
        assert!(!rendered.contains(".exe"));
    }

    #[test]
    fn diag_frames_and_desired_operations_stay_separate_and_bounded() {
        // Truthful pending desire vs actual attempts vs terminal verdict:
        // published `frame_operation`/`clear_operation` tokens ride
        // `desired_operations`, actual apply-produced writes ride `frames`,
        // and the core verdict rides `operation_settled`. Desire never
        // claims arrival; only the event proves the retained binding
        // retired. Rows stay bounded with honest omitted counts.
        use tiler_core::world_report::{
            FrameAttempt, FrameSetter, OperationEvent, OperationOutcome, WindowDesire,
        };
        let target = tiler_core::geometry::Rect {
            x: 10,
            y: 20,
            w: 960,
            h: 1040,
        };
        let mut reply = reply_shell();
        reply.windows.push(WindowDesire {
            lifetime: WindowId("w-float".to_owned()),
            output: OutputId("o1".to_owned()),
            workspace: "w2".to_owned(),
            allocation: None,
            effective_frame: Some(target),
            frame_authority: tiler_core::world_report::ScopeAuthority::Untouched,
            membership_authority: tiler_core::world_report::ScopeAuthority::Untouched,
            floating: true,
            float_origin: None,
            frame_operation: Some("op-20".to_owned()),
            clear_operation: None,
        });
        reply.windows.push(WindowDesire {
            lifetime: WindowId("w-max".to_owned()),
            output: OutputId("o1".to_owned()),
            workspace: "w2".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: tiler_core::world_report::ScopeAuthority::Untouched,
            membership_authority: tiler_core::world_report::ScopeAuthority::Untouched,
            floating: true,
            float_origin: None,
            frame_operation: None,
            clear_operation: Some("op-21".to_owned()),
        });
        reply.operation_settled.push(OperationEvent {
            operation_id: "op-21".to_owned(),
            window: WindowId("w-max".to_owned()),
            outcome: OperationOutcome::ClearRefusedProven,
            cause: Some("settler".to_owned()),
        });
        let frames = vec![FrameAttempt {
            window: WindowId("w-float".to_owned()),
            target,
            setter: FrameSetter::Accepted,
            readback: Some(target),
            dpi_before: Some(96),
            dpi_after: Some(96),
            attempt_ms: 7,
            operation_id: Some("op-20".to_owned()),
            prewrite: Some(tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            }),
        }];
        let quiet = super::super::world_diff::ApplyStats::default();
        let empty_members: Vec<tiler_core::world_report::MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        // Both triggers fire through the REAL throttle: frame-only and
        // settled-only observation ticks each log.
        assert!(should_log_apply(
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &frames,
            &reply_shell(),
        ));
        assert!(should_log_apply(
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &[],
            &reply,
        ));
        let value = apply_diag_value(
            21,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &frames,
            serde_json::json!({}),
            None,
            false,
        );
        let desired = value["desired_operations"]
            .as_array()
            .expect("desired operations");
        assert_eq!(desired.len(), 2);
        assert_eq!(desired[0]["op"], serde_json::json!("op-20"));
        assert_eq!(desired[0]["window"], serde_json::json!("w-float"));
        assert_eq!(desired[0]["kind"], serde_json::json!("frame"));
        assert_eq!(desired[0]["target"]["w"], serde_json::json!(960));
        assert_eq!(desired[1]["op"], serde_json::json!("op-21"));
        assert_eq!(desired[1]["kind"], serde_json::json!("clear"));
        assert_eq!(value["desired_operations_omitted"], serde_json::json!(0));
        let frame_rows = value["frames"].as_array().expect("frame rows");
        assert_eq!(frame_rows.len(), 1);
        assert_eq!(frame_rows[0]["op"], serde_json::json!("op-20"));
        assert_eq!(frame_rows[0]["window"], serde_json::json!("w-float"));
        assert_eq!(frame_rows[0]["setter"], serde_json::json!("accepted"));
        assert_eq!(frame_rows[0]["target"]["x"], serde_json::json!(10));
        assert_eq!(frame_rows[0]["prewrite"]["w"], serde_json::json!(800));
        assert_eq!(frame_rows[0]["readback"]["w"], serde_json::json!(960));
        assert_eq!(frame_rows[0]["dpi_before"], serde_json::json!(96));
        assert_eq!(frame_rows[0]["attempt_ms"], serde_json::json!(7));
        assert_eq!(value["frames_omitted"], serde_json::json!(0));
        let settled = value["operation_settled"].as_array().expect("settled");
        assert_eq!(settled.len(), 1);
        assert_eq!(settled[0]["op"], serde_json::json!("op-21"));
        assert_eq!(settled[0]["outcome"], serde_json::json!("refused-proven"));
        // Desire and verdict share the op but stay separate rows: the
        // desired row is pending intent, the event is the terminal verdict.
        assert_ne!(desired[1]["kind"], settled[0]["outcome"]);
    }

    #[test]
    fn diag_frames_and_desired_operations_omit_honestly() {
        // Over-cap attempts and desires bound at the shared attempt cap
        // with honest omitted counts, and a quiet idle tick stays silent
        // through the REAL throttle (no extra rows while idle).
        use tiler_core::world_report::{FrameAttempt, FrameSetter, WindowDesire};
        let quiet = super::super::world_diff::ApplyStats::default();
        let empty_members: Vec<tiler_core::world_report::MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        let idle_reply = reply_shell();
        assert!(!should_log_apply(
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &[],
            &idle_reply,
        ));
        let mut reply = reply_shell();
        for n in 0..20 {
            reply.windows.push(WindowDesire {
                lifetime: WindowId(format!("w{n}")),
                output: OutputId("o1".to_owned()),
                workspace: "w2".to_owned(),
                allocation: None,
                effective_frame: Some(tiler_core::geometry::Rect {
                    x: n,
                    y: 0,
                    w: 100,
                    h: 100,
                }),
                frame_authority: tiler_core::world_report::ScopeAuthority::Untouched,
                membership_authority: tiler_core::world_report::ScopeAuthority::Untouched,
                floating: true,
                float_origin: None,
                frame_operation: Some(format!("op-{n}")),
                clear_operation: None,
            });
        }
        let frames: Vec<FrameAttempt> = (0..20)
            .map(|n| FrameAttempt {
                window: WindowId(format!("w{n}")),
                target: tiler_core::geometry::Rect {
                    x: n,
                    y: 0,
                    w: 100,
                    h: 100,
                },
                setter: FrameSetter::Accepted,
                readback: None,
                dpi_before: None,
                dpi_after: None,
                attempt_ms: n as u64,
                operation_id: Some(format!("op-{n}")),
                prewrite: None,
            })
            .collect();
        let value = apply_diag_value(
            22,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &frames,
            serde_json::json!({}),
            None,
            false,
        );
        assert_eq!(
            value["frames"].as_array().expect("frames").len(),
            MAX_APPLY_DIAG_ATTEMPTS
        );
        assert_eq!(
            value["frames_omitted"],
            serde_json::json!(20 - MAX_APPLY_DIAG_ATTEMPTS)
        );
        assert_eq!(
            value["desired_operations"]
                .as_array()
                .expect("desired")
                .len(),
            MAX_APPLY_DIAG_ATTEMPTS
        );
        assert_eq!(
            value["desired_operations_omitted"],
            serde_json::json!(20 - MAX_APPLY_DIAG_ATTEMPTS)
        );
    }
}
