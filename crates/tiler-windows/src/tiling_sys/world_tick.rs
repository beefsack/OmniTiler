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

// Test-only native-planning proof for latest-only scheduling (offline,
// native-free): counts `apply_accepted` passes. The command counter covers
// the full command path (`run_world_command`); the observation counter
// covers `run_observation_tick`, including the synchronous catch-up the
// command paths service on dispatch refusal/rejection. Core-only superseded
// dispatches never increment either: they plan/apply zero native writes
// themselves. Production behavior is unchanged: the counters are never read
// outside tests.
#[cfg(test)]
thread_local! {
    static COMMAND_APPLY_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static OBSERVATION_APPLY_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Reset the command-path apply counter (test-only).
#[cfg(test)]
pub(super) fn reset_command_apply_count() {
    COMMAND_APPLY_COUNT.with(|slot| slot.set(0));
}

/// Read the command-path apply counter (test-only).
#[cfg(test)]
#[must_use]
pub(super) fn command_apply_count() -> u64 {
    COMMAND_APPLY_COUNT.with(|slot| slot.get())
}

/// Reset the observation-path apply counter (test-only).
#[cfg(test)]
pub(super) fn reset_observation_apply_count() {
    OBSERVATION_APPLY_COUNT.with(|slot| slot.set(0));
}

/// Read the observation-path apply counter (test-only).
#[cfg(test)]
#[must_use]
pub(super) fn observation_apply_count() -> u64 {
    OBSERVATION_APPLY_COUNT.with(|slot| slot.get())
}

/// Reset both apply counters (test-only).
#[cfg(test)]
pub(super) fn reset_all_apply_counts() {
    reset_command_apply_count();
    reset_observation_apply_count();
}

/// Read the combined apply-pass total (test-only).
#[cfg(test)]
#[must_use]
pub(super) fn total_apply_count() -> u64 {
    command_apply_count() + observation_apply_count()
}

/// Which native-application leg a dispatched command takes. `Apply` runs
/// the full plan/apply pass; `Superseded` orders through the core exactly
/// once with no native planning or application on this leg. Owed
/// observation-only catch-up is fenced identically but serviced
/// asymmetrically: the full leg applies it natively through the observation
/// path, while the superseded leg inspects the fence before dispatching,
/// services it once through a native-free core-only observation (exact
/// receipts acked, no planning, no applied watermark, no trace proof, no
/// gesture clearing), then dispatches the command exactly once. A refused or
/// rejected superseded command stays dropped (never queued, never
/// re-dispatched): the next command's own pre-dispatch inspection services
/// any fence still owed. Intermediate states never render but every
/// dispatched command still orders.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandApplyMode {
    Apply,
    Superseded,
}

/// Acknowledge exactly the delivered close receipts the core incorporated
/// and prune exact-equality pairs from the adapter close map. Shared by
/// both command legs so the ack contract cannot drift between them.
fn ack_close_receipts(
    world: &mut super::world_owner::WorldOwner,
    pending_closed: &mut std::collections::BTreeMap<String, super::retained::PendingClosed>,
    reply: &tiler_core::world_report::WorldReply,
    applied_homing: &mut std::collections::HashMap<String, (String, String)>,
) -> super::world_sequencer::AckOutcome {
    let mut pending = std::mem::take(pending_closed);
    let (ack_outcome, pruned) = world.ack(reply, &mut pending);
    *pending_closed = pending;
    for token in pruned {
        applied_homing.remove(&token);
    }
    ack_outcome
}

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
/// only raw attempt rows, held scopes, retained transfer desires, the
/// accepted desired windows/outputs/workspaces projection, and the
/// dispatched-report screen geometry basis ride.
const MAX_APPLY_DIAG_ATTEMPTS: usize = 16;
const MAX_APPLY_DIAG_SCOPES: usize = 16;
const MAX_APPLY_DIAG_TRANSFERS: usize = 16;
const MAX_APPLY_DIAG_WINDOWS: usize = 16;
const MAX_APPLY_DIAG_OUTPUTS: usize = 16;
const MAX_APPLY_DIAG_WORKSPACES: usize = 16;
const MAX_APPLY_DIAG_SCREENS: usize = 16;
/// Trace-only focus oracle cap: minted effects plus per-effect dispatch
/// rows stay bounded with honest `omitted` counts. Effects are already
/// bounded by the reply budget; this caps only the retained trace rows.
const MAX_APPLY_DIAG_FOCUS: usize = 8;
/// Trace-only fresh receipt proof cap: the fresh full-view proof map from
/// the post-apply `receipt_updates` read (never merged applied transfers,
/// never homing). Bounded with an honest omitted count; empty means zero
/// fresh proof, never an echo of prior applied maps.
const MAX_APPLY_DIAG_RECEIPT_VIEWS: usize = 16;

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
        WorldCommand::ToggleWorkspaceTiling { .. } => "toggle-workspace-tiling",
        WorldCommand::MigrateWorkspace { .. } => "migrate-workspace",
        WorldCommand::FocusDirectional { .. } => "focus-directional",
        WorldCommand::MoveDirectional { .. } => "move-directional",
        WorldCommand::SendDirectional { .. } => "send-directional",
        WorldCommand::MigrateDirectional { .. } => "migrate-directional",
        WorldCommand::ToggleMaximize { .. } => "toggle-maximize",
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
        Some(CommandResult::Migration { .. }) => "migration",
        Some(CommandResult::Maximize { .. }) => "maximize",
    }
}

/// True when the fresh receipt proof differs from the last accepted trace
/// snapshot (bounded prefix plus honest omission, stamps excluded). The
/// full-view map and the homing map change together: either map changing
/// (or a changed omission) means the proof moved and must log. `None` or
/// a snapshot without `receipt_views` means first status and must log, so
/// a new arrival proof never stays quiet on a views-only match. Homing is
/// a bounded comparison only: the caller passes the truncated `receipt_homing`
/// prefix plus the true `receipt_homing_total`; omission derives from the
/// total, never the prefix length. A tail-only change past the cap with the
/// same prefix and same total is unavailable and stays quiet by design: this
/// is never whole-map proof, and no tail census, hash, or history is kept.
fn receipt_proof_changed(
    receipt_views: &[(String, String)],
    receipt_homing: &[(String, String, String)],
    receipt_homing_total: usize,
    prior: Option<&serde_json::Value>,
) -> bool {
    let Some(prior) = prior else {
        return true;
    };
    let Some(rows) = prior
        .get("receipt_views")
        .and_then(|value| value.as_array())
    else {
        return true;
    };
    let prior_omitted = prior
        .get("receipt_views_omitted")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let taken = receipt_views.len().min(MAX_APPLY_DIAG_RECEIPT_VIEWS);
    let current_omitted = (receipt_views.len().saturating_sub(taken)) as u64;
    if rows.len() != taken || prior_omitted != current_omitted {
        return true;
    }
    for (index, row) in receipt_views.iter().take(taken).enumerate() {
        let prior_row = &rows[index];
        if prior_row.get("output").and_then(|v| v.as_str()) != Some(row.0.as_str())
            || prior_row.get("view").and_then(|v| v.as_str()) != Some(row.1.as_str())
        {
            return true;
        }
    }
    let homing_rows = prior
        .get("receipt_homing")
        .and_then(|value| value.as_array());
    let Some(homing_rows) = homing_rows else {
        return true;
    };
    let prior_homing_omitted = prior
        .get("receipt_homing_omitted")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let homing_taken = receipt_homing
        .len()
        .min(super::world_diff::MAX_RECEIPT_HOMING_DIAG)
        .min(receipt_homing_total);
    let homing_omitted = (receipt_homing_total.saturating_sub(homing_taken)) as u64;
    if homing_rows.len() != homing_taken || prior_homing_omitted != homing_omitted {
        return true;
    }
    for (index, row) in receipt_homing.iter().take(homing_taken).enumerate() {
        let prior_row = &homing_rows[index];
        if prior_row.get("window").and_then(|v| v.as_str()) != Some(row.0.as_str())
            || prior_row.get("output").and_then(|v| v.as_str()) != Some(row.1.as_str())
            || prior_row.get("view").and_then(|v| v.as_str()) != Some(row.2.as_str())
        {
            return true;
        }
    }
    false
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
    // In-flight clearing/landing rows are first-class evidence: a pass
    // that only cleared a remembered flight (landing, supersession,
    // lifetime end) must emit its clearing row, not stay quiet.
    if !stats.inflight_events.is_empty()
        || stats.inflight_total > 0
        || stats.foreground_meta_omitted > 0
    {
        return true;
    }
    // Executed visibility posts are first-class evidence too: a pass
    // whose only trace is an executed hide/reveal post (or its omission
    // past the row cap) must emit its rows, not stay quiet.
    if !stats.visibility_posts.is_empty() || stats.visibility_total > 0 {
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
/// completion). The accepted desired projection (`desired_windows` with
/// SEPARATE `allocation` vs `effective_frame` plus per-property authority,
/// floating, lifetime/homing, and operation tokens; `desired_outputs` with
/// the logical views; `desired_workspaces`) rides from the exact accepted
/// `reply`, never synthesized from native observations or expectations.
/// The geometry basis (`desired_geometry` screen rows with wire `output`,
/// `full`/`work` rects, per-screen `gap`/`outer_gap`, `complete`, `primary`)
/// rides from the actual dispatched `report` screens, never a separate
/// native area copy: the row is published only when the report/report-reply
/// binding proves the screens are the reply's own accepted input
/// (`request_id`, `observation_seq` vs `based_on_observation_seq`,
/// `adapter_incarnation`, `core_generation`, `contract_version`). A paired
/// row carries `geometry_status: "paired"` with an honest numeric omission;
/// an unpaired report carries empty geometry, a null omission (never a valid
/// zero-completeness signal), and `geometry_status: "mismatch"` with the
/// presented report provenance, so unpaired geometry can never be mistaken
/// for the reply's accepted input. `duplicate` rows repeat the cached accepted
/// reply (identical report digest, no new revision); `command-consumed` rows
/// carry a freshly accepted observation reply whose already-consumed command
/// never re-executed. Both keep a truthful `delivery` label, never a
/// fresh-acceptance claim; `request`/`revision`/`observation` provenance joins
/// attempt rows (`window`/`op`/`attempt_ms`) and native lifetimes. Every new
/// array carries an honest `*_omitted` count and empty means zero desire,
/// never an absent field. The only refusal detail available from existing values is
/// the raw setter vocabulary: the [`NativeOps`] contract surfaces explicit
/// refusal as `Ok(false)` and torn state as `Err` without a native errno,
/// so deeper refusal detail would need a broader seam change (reported as a
/// limitation, not papered over here).
// Fifteen inputs are the disjoint trace context plus the pass products
// (reply, delivery, command kind, stats, raw attempts, fresh receipt proof,
// cursor-hit/FFC, dispatched-report geometry basis); bundling would hide
// the call-site borrow split the tick relies on.
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
    report: &tiler_core::world_report::WorldReport,
    receipt_views: &[(String, String)],
    receipt_proof_mono_ms: Option<u64>,
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
                "attempt_ms": item.attempt_ms,
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
                "attempt_ms": item.attempt_ms,
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
                "attempt_ms": item.attempt_ms,
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
    // Accepted desired projection from the exact reply: separate allocation
    // (canonical slot, never a write target) from effective_frame (the only
    // writable target), with per-property authority, floating, homing, and
    // operation tokens. Same lifetime/op vocabulary as the attempt rows so
    // a live leg can join desire to mechanical facts. Bounded with honest
    // omission; empty means zero desire, never an absent field.
    let desired_windows: Vec<serde_json::Value> = reply
        .windows
        .iter()
        .take(MAX_APPLY_DIAG_WINDOWS)
        .map(|row| {
            serde_json::json!({
                "window": row.lifetime.0.clone(),
                "out": row.output.0.clone(),
                "ws": row.workspace.clone(),
                "allocation": row.allocation.map(rect_value),
                "effective_frame": row.effective_frame.map(rect_value),
                "frame_authority": row.frame_authority.as_str(),
                "frame_hold": row.frame_authority.hold_reason().map(|reason| reason.as_str()),
                "membership_authority": row.membership_authority.as_str(),
                "membership_hold": row.membership_authority.hold_reason().map(|reason| reason.as_str()),
                "floating": row.floating,
                "float_origin": row.float_origin.map(|origin| origin.as_str()),
                "frame_op": row.frame_operation.clone(),
                "clear_op": row.clear_operation.clone(),
            })
        })
        .collect();
    // Accepted logical views: the core-owned desired output targets, labeled
    // `desired_outputs` so they never mix with raw view-attempt pre/post/base
    // proof in `views`.
    let desired_outputs: Vec<serde_json::Value> = reply
        .outputs
        .iter()
        .take(MAX_APPLY_DIAG_OUTPUTS)
        .map(|row| {
            serde_json::json!({
                "output": row.output.0.clone(),
                "logical_view": row.logical_view.clone(),
                "tiling_mode": row.tiling_mode.as_str(),
                "hold": row.hold.map(|reason| reason.as_str()),
            })
        })
        .collect();
    let desired_workspaces: Vec<serde_json::Value> = reply
        .workspaces
        .iter()
        .take(MAX_APPLY_DIAG_WORKSPACES)
        .map(|row| {
            serde_json::json!({
                "id": row.id.clone(),
                "output": row.output.0.clone(),
                "scope": row.scope.clone(),
                "order": row.order,
                "tiling_mode": row.tiling_mode.as_str(),
                "existence": row.existence,
                "hold": row.hold.map(|reason| reason.as_str()),
            })
        })
        .collect();
    // Geometry basis from the actual dispatched report: the report screens
    // that produced this accepted reply, published with wire output ids so
    // a live leg can join them to `desired_outputs` and attempt `out`
    // fields. Published only on a proven report/reply binding (request,
    // observation, incarnation, generation, contract all match); otherwise
    // empty with a null omission and an explicit mismatch status, so
    // unpaired screens can never be mistaken for the reply's accepted
    // input. Bounded with honest omission on the paired path.
    let geometry_paired = report.request_id == reply.request_id
        && report.observation_seq == reply.based_on_observation_seq
        && report.adapter_incarnation == reply.adapter_incarnation
        && report.core_generation == reply.core_generation
        && report.contract_version == reply.contract_version;
    let geometry_rows: Vec<serde_json::Value> = if geometry_paired {
        report
            .screens
            .iter()
            .take(MAX_APPLY_DIAG_SCREENS)
            .map(|screen| {
                serde_json::json!({
                    "output": screen.output.0.clone(),
                    "full": screen.full_rect.map(rect_value),
                    "work": screen.work_area.map(rect_value),
                    "gap": screen.gap,
                    "outer_gap": screen.outer_gap,
                    "complete": screen.complete,
                    "primary": screen.primary,
                })
            })
            .collect()
    } else {
        Vec::new()
    };
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
            let mut value = serde_json::json!({
                "id": row.id.clone(),
                "target": row.target.clone(),
                "kind": row.kind,
                "dispatched": row.dispatched,
                "setter": row.setter,
                "foreground": row.foreground,
                "hold": row.hold,
                "fg_baseline": row.fg_baseline.clone(),
                "fg_live": row.fg_live.clone(),
                "api": row.api,
                "call_mono_ms": row.call_mono_ms,
                "ret_mono_ms": row.ret_mono_ms,
                "fg_proof_mono_ms": row.fg_proof_mono_ms,
                "gui_dispatch": row.gui_dispatch.clone(),
                "gui_landing": row.gui_landing.clone(),
                "primitive_called": row.primitive_called,
                "native_return": row.native_return,
                "prime_inserted": row.prime_inserted,
                "hwnd": row.hwnd,
                "pid": row.pid,
                "creation": row.creation.clone(),
            });
            // Exception-only volume: the exact-desktop marker exists solely
            // on dispatches that passed the foreground-fullscreen veto
            // through the explicit-selection exception. Every other row
            // omits the key entirely, never as false.
            if row.desktop_exception {
                value["desktop_exception"] = serde_json::json!(true);
            }
            // Refusal-only volume: the refusal detail keys exist solely on
            // traced `Ok(false)` veto rows (`refusal_gate` is `Some` only
            // there). Every other row omits them entirely, never as nulls.
            // The `fg_gate_*` gate-time keys ride only the
            // `focus-write-veto` refusal rows; other gates omit them.
            if let Some(gate) = row.refusal_gate {
                value["refusal_gate"] = serde_json::json!(gate);
                value["refusal_reason"] = serde_json::json!(row.refusal_reason);
                value["target_visible"] = serde_json::json!(row.target_visible);
                value["target_cloaked"] = serde_json::json!(row.target_cloaked);
                value["target_hidden"] = serde_json::json!(row.target_hidden);
                value["fg_identity"] = serde_json::json!(row.fg_identity);
                if let Some(gate_identity) = row.fg_gate_identity {
                    value["fg_gate_identity"] = serde_json::json!(gate_identity);
                }
                if let Some(kind) = row.fg_veto_kind {
                    value["fg_veto_kind"] = serde_json::json!(kind);
                }
                if let Some(readable) = row.fg_readable {
                    value["fg_readable"] = serde_json::json!(readable);
                }
                if let Some(shell) = row.fg_shell {
                    value["fg_shell"] = serde_json::json!(shell);
                }
                if let Some(fullscreen) = row.fg_fullscreen {
                    value["fg_fullscreen"] = serde_json::json!(fullscreen);
                }
                if let Some(visible) = row.fg_visible {
                    value["fg_visible"] = serde_json::json!(visible);
                }
                if let Some(exe) = row.fg_exe.as_deref() {
                    value["fg_exe"] = serde_json::json!(exe);
                }
            }
            value
        })
        .collect();
    // Trace-only in-flight clearing/landing rows for this pass: why each
    // remembered flight ended, with the clearing edge's delivery/owner
    // stamps when an edge cleared it. Bounded with an honest omitted
    // count derived from the unbounded `inflight_total`, never the
    // truncated stored rows.
    let inflight_rows: Vec<serde_json::Value> = stats
        .inflight_events
        .iter()
        .take(MAX_APPLY_DIAG_FOCUS)
        .map(|row| {
            serde_json::json!({
                "token": row.token.clone(),
                "effect": row.effect.clone(),
                "cause": row.cause,
                "edge_event_ms": row.edge_event_ms,
                "edge_queued_mono_ms": row.edge_queued_mono_ms,
                "edge_drain_mono_ms": row.edge_drain_mono_ms,
                "resolve_mono_ms": row.resolve_mono_ms,
                "gui": row.gui.clone(),
                "hwnd": row.hwnd,
                "pid": row.pid,
                "creation": row.creation.clone(),
            })
        })
        .collect();
    // Trace-only executed visibility posts for this pass: the actual host
    // API that ran per hide/reveal post, with the activation-affecting
    // parameter (async `SetWindowPos` flag word or `ShowWindowAsync`
    // command) and the post verdict. Call-to-return alone never proves
    // arrival; the visibility proof rides the views/members rows.
    // Bounded with an honest omitted count derived from the unbounded
    // `visibility_total`, never the truncated stored rows.
    let visibility_rows: Vec<serde_json::Value> = stats
        .visibility_posts
        .iter()
        .take(MAX_APPLY_DIAG_FOCUS)
        .map(|row| {
            serde_json::json!({
                "hwnd": row.hwnd,
                "hide": row.hide,
                "api": row.api,
                "flags": row.flags,
                "cmd": row.cmd,
                "posted": row.posted,
                "call_mono_ms": row.call_mono_ms,
                "ret_mono_ms": row.ret_mono_ms,
            })
        })
        .collect();
    // Trace-only fresh receipt proof: the fresh full-view map from the
    // post-apply `receipt_updates` read, captured before merged applied
    // transfers so the source stays fresh-only (never stale/ambiguous
    // merged state). Only the `read_view` map rides, never homing fields or
    // source flags. Empty means zero fresh proof, never an echo of prior
    // applied maps. Bounded with an honest omitted count.
    let receipt_rows: Vec<serde_json::Value> = receipt_views
        .iter()
        .take(MAX_APPLY_DIAG_RECEIPT_VIEWS)
        .map(|(output, view)| {
            serde_json::json!({
                "output": output,
                "view": view,
            })
        })
        .collect();
    let mut out = serde_json::json!({
        "event": "world-apply",
        "tick": tick,
        "request": reply.request_id.clone(),
        "revision": reply.state_revision,
        "observation": reply.based_on_observation_seq,
        "delivery": delivery,
        "command": command_kind,
        "command_consumed": reply.command_consumed,
        "command_result": command_result_kind(&reply.command_result),
        "invalidation_pending": super::observe::hook_cause_summary()
            .into_iter()
            .map(|(cause, pending)| serde_json::json!({
                "cause": cause,
                "pending": pending,
            }))
            .collect::<Vec<serde_json::Value>>(),
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
            "staged_reveals": stats.staged_reveals,
            "staged_focus": stats.staged_focus,
            "staged_hides": stats.staged_hides,
            "show_posts_done_mono_ms": stats.show_posts_done_mono_ms,
            "arrivals_visible_mono_ms": stats.arrivals_visible_mono_ms,
            "departures_hidden_mono_ms": stats.departures_hidden_mono_ms,
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
        "inflight_events": inflight_rows,
        "inflight_events_omitted": stats
            .inflight_total
            .saturating_sub(stats.inflight_events.len().min(MAX_APPLY_DIAG_FOCUS)),
        "foreground_meta_omitted": stats.foreground_meta_omitted,
        "visibility_posts": visibility_rows,
        "visibility_posts_omitted": stats
            .visibility_total
            .saturating_sub(stats.visibility_posts.len().min(MAX_APPLY_DIAG_FOCUS)),
        "fresh_route": fresh_route,
    });
    out["receipt_views"] = serde_json::Value::Array(receipt_rows);
    out["receipt_views_omitted"] = serde_json::json!(
        receipt_views
            .len()
            .saturating_sub(out["receipt_views"].as_array().map_or(0, |rows| rows.len()))
    );
    out["receipt_proof_mono_ms"] = match receipt_proof_mono_ms {
        Some(stamp) => serde_json::json!(stamp),
        None => serde_json::Value::Null,
    };
    // Post-apply arrival map (fresh homing proof, distinct from the full-view
    // map above): the current-order equivalent arrival proof. Same
    // `receipt_proof_mono_ms` instant proves both maps post-apply; the staged
    // pre-focus proof (`stats.arrivals_visible_mono_ms`) is earlier and
    // separate. Empty means zero fresh arrival proof (empty arrival stays
    // N/A): extraction must require a homing row for the target view before
    // using the stamp, never treat the stamp alone as arrival success.
    // Bounded with an honest omitted count.
    let homing_rows: Vec<serde_json::Value> = stats
        .receipt_homing
        .iter()
        .take(super::world_diff::MAX_RECEIPT_HOMING_DIAG)
        .map(|(token, output, view)| {
            serde_json::json!({
                "window": token,
                "output": output,
                "view": view,
            })
        })
        .collect();
    out["receipt_homing"] = serde_json::Value::Array(homing_rows);
    out["receipt_homing_omitted"] = serde_json::json!(
        stats.receipt_homing_total.saturating_sub(
            out["receipt_homing"]
                .as_array()
                .map_or(0, |rows| rows.len())
        )
    );
    out["desired_windows"] = serde_json::Value::Array(desired_windows);
    out["desired_windows_omitted"] = serde_json::json!(
        reply
            .windows
            .len()
            .saturating_sub(reply.windows.len().min(MAX_APPLY_DIAG_WINDOWS))
    );
    out["desired_outputs"] = serde_json::Value::Array(desired_outputs);
    out["desired_outputs_omitted"] = serde_json::json!(
        reply.outputs.len().saturating_sub(
            out["desired_outputs"]
                .as_array()
                .map_or(0, |rows| rows.len())
        )
    );
    out["desired_workspaces"] = serde_json::Value::Array(desired_workspaces);
    out["desired_workspaces_omitted"] = serde_json::json!(
        reply.workspaces.len().saturating_sub(
            out["desired_workspaces"]
                .as_array()
                .map_or(0, |rows| rows.len())
        )
    );
    out["desired_geometry"] = serde_json::Value::Array(geometry_rows);
    out["desired_geometry_omitted"] = if geometry_paired {
        serde_json::json!(
            report.screens.len().saturating_sub(
                out["desired_geometry"]
                    .as_array()
                    .map_or(0, |rows| rows.len())
            )
        )
    } else {
        serde_json::Value::Null
    };
    out["geometry_status"] = serde_json::json!(if geometry_paired {
        "paired"
    } else {
        "mismatch"
    });
    out["geometry_report"] = serde_json::json!({
        "request": report.request_id.clone(),
        "observation": report.observation_seq,
        "owner": report.owner.clone(),
        "incarnation": report.adapter_incarnation.clone(),
        "generation": report.core_generation.clone(),
    });
    out
}

/// Trace-only emission for one apply pass. Event-driven owner trace only:
/// no input-hook cost, no native re-read, no per-window dump. Returns true
/// when a row was emitted (the caller records the hover triple then, never
/// on quiet ticks).
// Twenty inputs are the disjoint trace context plus the pass products
// (reply, delivery, command kind, stats, raw attempts, fresh receipt proof,
// cursor-hit/FFC, dispatched-report geometry basis, hover-change,
// receipt-change, command-receipt stamp); bundling would hide the
// call-site borrow split.
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
    report: &tiler_core::world_report::WorldReport,
    receipt_views: &[(String, String)],
    receipt_proof_mono_ms: Option<u64>,
    hover_changed: bool,
    receipt_changed: bool,
    recv_mono_ms: u64,
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
        ) || hover_changed
            || receipt_changed)
    {
        return false;
    }
    // Command-receipt stamp rides post-hoc so the stored proof snapshot (see
    // the call sites) and this emitted row share one value without
    // rethreading every `apply_diag_value` caller: both use the same
    // command-entry `recv_mono_ms` (sampled before the admission preamble),
    // never the post-preamble dispatch clock.
    let mut value = apply_diag_value(
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
        report,
        receipt_views,
        receipt_proof_mono_ms,
    );
    value["recv_mono_ms"] = serde_json::json!(recv_mono_ms);
    super::trace::log_json_at(log_path, value);
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
    presentation_paused: bool,
) -> Option<tiler_core::world_report::WorldReply> {
    let trace = state.trace;
    let tick = state.tick;
    let log_path = state.log_path.clone();
    admit_observed(state, me, observed);
    admit_retained_and_marks(state, me, observed, retained, areas);
    snapshot_frames(state, observed);
    super::rows::refresh_world_hints(state, observed, retained);
    // The native stacking snapshot stashed by the latest observation;
    // capture maps it to opaque lifetimes for the pager order.
    let presentation_native_order = state.last_native_order.clone();
    // Normalized presentation ingress rides this tick's capture (stage 4a):
    // computed before the disjoint split borrows the loop.
    let gesture = state.gesture_ingress(cursor);
    // Raw cursor-hit HWND for the same tick as the cursor sample: the
    // exact managed binding resolves in capture over the owner tables.
    // `None` whenever the cursor itself is unsampled (game lane,
    // unreadable), so a hit never travels without its position.
    let hit_hwnd = super::observation::cursor_hit_hwnd(cursor);
    // Normalized presentation ingress rides this tick's capture (stage 5):
    // the foreground observation, chord/move facts, and host-scaled style,
    // computed before the disjoint split borrows the loop. The game lane
    // sets `presentation_paused` so the core skips presentation work.
    let (presentation_foreground, presentation_chord, presentation_move, presentation_style) =
        state.presentation_tick_inputs(me, fulls, foreground_hwnd);
    let presentation_border_enabled = state.border.enabled;
    let presentation_underlay_enabled = state.underlay.enabled;
    // Disjoint borrows: `world` mutably alongside every read-only input
    // field, never the whole loop (see `TileLoop::owner_inputs`).
    let TileLoop {
        ref mut world,
        ref member_tokens,
        ref member_identity,
        ref member_tags,
        ref mut member_rects,
        ref mut float_rects,
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
        presentation_foreground.clone(),
        presentation_chord,
        presentation_move.clone(),
        presentation_border_enabled,
        presentation_underlay_enabled,
        presentation_style,
        presentation_paused,
        presentation_native_order.clone(),
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
        presentation_foreground.clone(),
        presentation_chord,
        presentation_move.clone(),
        presentation_border_enabled,
        presentation_underlay_enabled,
        presentation_style,
        presentation_paused,
        presentation_native_order.clone(),
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
        presentation_foreground.clone(),
        presentation_chord,
        presentation_move.clone(),
        presentation_border_enabled,
        presentation_underlay_enabled,
        presentation_style,
        presentation_paused,
        presentation_native_order.clone(),
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
    native.set_diagnostics_trace(trace);
    world.sync_diff_gate();
    let mut frame_attempts = Vec::new();
    let mut member_attempts = Vec::new();
    let mut view_attempts = Vec::new();
    let mut clear_attempts = Vec::new();
    let mut stats = world.apply_accepted(
        &reply,
        &fresh,
        &mut native,
        &mut frame_attempts,
        &mut member_attempts,
        &mut view_attempts,
        &mut clear_attempts,
        tick_ms,
    );
    #[cfg(test)]
    OBSERVATION_APPLY_COUNT.with(|slot| slot.set(slot.get() + 1));
    let hover_hit = captured.cursor_hit.as_ref().map(|hit| hit.0.clone());
    let hover_changed = world.hover_trace_changed(
        hover_hit.as_deref(),
        &captured.focused_global,
        captured.focus_follows_cursor,
    );
    let fresh_run = delivery != "duplicate";
    // Proven receipt snapshot: desired values only where live native
    // evidence proves arrival (identity/tag/claim/visibility/scope
    // gates, exact frame on the target for tiled rows). Changed homing
    // additionally merges from staged raw attempt post proofs with
    // independent fresh revalidation. Never wholesale desire.
    //
    // TRACE-ONLY measurement order: the existing `receipt_updates` reads
    // yield the fresh final full-view proven state plus the fresh homing
    // arrival proof. The existing world-apply JSON construction/log
    // emission below defers until after these existing native calls,
    // preserving exact native-probe order with no new native queries. The
    // owner-clock stamp fires immediately after the existing
    // `receipt_updates`, and both maps are captured before the merge so
    // the trace source stays fresh-only (never stale/ambiguous merged
    // state). Trace-off builds no extra payload: the snapshot, stamp, and
    // arrival map are gated on `trace && fresh_run`.
    let (mut proven_h, mut proven_v) = native.receipt_updates(&reply);
    let receipt_proof_mono_ms = if trace && fresh_run {
        Some(super::trace::log_mono_ms())
    } else {
        None
    };
    let mut receipt_views: Vec<(String, String)> = if trace && fresh_run {
        let mut rows: Vec<(String, String)> = proven_v
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        rows.sort();
        rows
    } else {
        Vec::new()
    };
    if trace && fresh_run {
        let mut homing_rows: Vec<(String, String, String)> = proven_h
            .iter()
            .map(|(token, (out, ws))| (token.clone(), out.clone(), ws.clone()))
            .collect();
        homing_rows.sort();
        stats.receipt_homing_total = homing_rows.len();
        homing_rows.truncate(super::world_diff::MAX_RECEIPT_HOMING_DIAG);
        stats.receipt_homing = homing_rows;
        // Current-order arrival fallback: the post-apply receipt instant
        // proves arrival only when the staged pre-focus proof did not fire
        // this pass. Never overwrites the staged stamp with this later one.
        if stats.arrivals_visible_mono_ms.is_none() && !stats.receipt_homing.is_empty() {
            stats.arrivals_visible_mono_ms = receipt_proof_mono_ms;
        }
    }
    merge_applied_transfers(
        &reply,
        &member_attempts,
        &view_attempts,
        &fresh,
        &mut native,
        &mut proven_h,
        &mut proven_v,
    );
    // Bounded accepted trace-proof snapshot (TRACE-ONLY): the exact
    // `world-apply` projection for this reply, captured on every fresh
    // (non-duplicate) accept even when the ordinary world log stays
    // silent, so a later `pager-cells` emission can pair it. Duplicates
    // reuse the cached snapshot and must never relabel it; stale or
    // ignored acks retain the fallback snapshot below. The paired copy
    // retains the original timestamp/row: this snapshot is built once here
    // and never rebuilt with a fresh clock on pairing.
    let proof_row = if trace && fresh_run {
        let mut row = apply_diag_value(
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
            hover_hit.clone(),
            captured.focus_follows_cursor,
            &report,
            &receipt_views,
            receipt_proof_mono_ms,
        );
        row["recv_mono_ms"] = serde_json::json!(tick_ms);
        Some(row)
    } else {
        receipt_views.clear();
        None
    };
    let receipt_changed = trace
        && fresh_run
        && receipt_proof_changed(
            &receipt_views,
            &stats.receipt_homing,
            stats.receipt_homing_total,
            world.trace_proof(),
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
        &report,
        &receipt_views,
        receipt_proof_mono_ms,
        hover_changed,
        receipt_changed,
        tick_ms,
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
    // Hide-commit snapshot seeds: exact-lifetime freshly proven frames read
    // before the hide posts. Routed into the tile/float caches under the
    // same lane row assembly uses, so the next hidden report stages the
    // proven frame instead of the pre-write slot.
    let hidden_seeds = native.take_hidden_seeds();
    drop(native);
    super::world_native::apply_hidden_seeds(
        hidden_seeds,
        member_tokens,
        member_rects,
        float_rects,
        |key| {
            let sticky = sticky.contains_key(key);
            let durable =
                fixed_tile_overrides.contains(key) || fixed_origin_overrides.contains(key);
            let automatic = fixed_auto_seen.contains(key) && !sticky && !durable;
            crate::native_guards::float_lane(
                sticky,
                automatic,
                durable,
                born_first_exit.contains(key),
                floated.contains(key),
                restart_reject_hold.contains(key),
                born_fullscreen.contains(key),
                true,
            )
        },
    );
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
    if fresh_run {
        TileLoop::clear_gesture_override_on_incorporation(&mut state.gesture_override, &reply);
    }
    match ack_outcome {
        super::world_sequencer::AckOutcome::Applied(_) => {
            world.note_diff_applied(&reply);
            // Advance the paired trace-proof with the snapshot it proves:
            // only a fresh run claims incorporation, so duplicates, stale
            // and ignored outcomes never reach here with a replacement.
            if fresh_run {
                world.note_trace_proof(proof_row);
            }
            Some(reply)
        }
        super::world_sequencer::AckOutcome::Stale(_) => None,
        super::world_sequencer::AckOutcome::Ignored { .. } => None,
    }
}

/// Core-only observation catch-up for the superseded command leg: dispatch
/// one observation, deliver through the staged production path, and
/// acknowledge the exact carried receipts with no native planning or
/// application. The diff applied watermark, trace proof, gesture stash,
/// proven receipt maps, and hidden seeds all stay untouched: those belong to
/// the latest full pass. Neither apply oracle counter moves. Returns true
/// only when the ack applied current (the fence cleared); stale/ignored
/// outcomes keep the fence and return false with no loop, no timer, no wait.
/// The admission preamble is identical to the full-leg dispatch preamble
/// (mechanical binding only, fail-closed): it may bind newly proven members
/// exactly as a full-leg tick would, so this leg is native-free only in the
/// plan/apply sense, never in the admission sense.
#[allow(clippy::too_many_arguments)]
fn run_observation_core_only(
    state: &mut TileLoop,
    me: &crate::model::ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    areas: &[MonitorArea],
    foreground_hwnd: u64,
    fulls: &[tiler_core::geometry::Rect],
    cursor: Option<(i32, i32)>,
) -> bool {
    let tick = state.tick;
    let log_path = state.log_path.clone();
    admit_observed(state, me, observed);
    admit_retained_and_marks(state, me, observed, retained, areas);
    snapshot_frames(state, observed);
    super::rows::refresh_world_hints(state, observed, retained);
    let presentation_native_order = state.last_native_order.clone();
    let gesture = state.gesture_ingress(cursor);
    let hit_hwnd = super::observation::cursor_hit_hwnd(cursor);
    // Commands never run on the game lane, so the pause flag stays clear
    // here like the command dispatch below.
    let presentation_paused = false;
    let (presentation_foreground, presentation_chord, presentation_move, presentation_style) =
        state.presentation_tick_inputs(me, fulls, foreground_hwnd);
    let presentation_border_enabled = state.border.enabled;
    let presentation_underlay_enabled = state.underlay.enabled;
    let TileLoop {
        ref mut world,
        ref member_tokens,
        ref member_identity,
        ref member_tags,
        ref mut member_rects,
        ref mut float_rects,
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
        presentation_foreground.clone(),
        presentation_chord,
        presentation_move.clone(),
        presentation_border_enabled,
        presentation_underlay_enabled,
        presentation_style,
        presentation_paused,
        presentation_native_order.clone(),
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
        Err(_) => return false,
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
            return false;
        }
    };
    // Honest row, never a native outcome: this leg planned/applied nothing.
    super::trace::log_json_at(
        &log_path,
        serde_json::json!({
            "event": "world-apply",
            "tick": tick,
            "request": report.request_id.clone(),
            "delivery": delivery,
            "command": "observation",
            "native": "catchup-core-only-skipped",
        }),
    );
    match ack_close_receipts(&mut *world, pending_closed, &reply, applied_homing) {
        // Incorporated and current: obligations acked and the fence cleared,
        // with no native planning, no applied watermark, no trace proof, and
        // no gesture clearing. The latest full pass owns all of those.
        super::world_sequencer::AckOutcome::Applied(_) => true,
        super::world_sequencer::AckOutcome::Stale(_)
        | super::world_sequencer::AckOutcome::Ignored { .. } => false,
    }
}

/// Shared command dispatch core: admission preamble, catch-up fence check
/// (superseded leg only), capture, single dispatch, deliver, then either the
/// full native plan/apply pass (`Apply`) or the superseded order-only tail
/// (`Superseded`). The superseded leg inspects the owed observation-only
/// catch-up before dispatching and services it once through a native-free
/// core-only observation on this turn's tick products, then dispatches the
/// command exactly once: no re-dispatch, no replay, no loop, no timer, no
/// wait. A refused superseded command stays dropped; the next command's own
/// pre-dispatch inspection services any fence still owed. The full leg keeps
/// its existing behavior: a refused command stays dropped while owed
/// catch-up is applied natively through the observation path on this turn's
/// tick products. Either leg services a core rejection's newly armed fence
/// synchronously on this turn's tick products (natively on the full leg,
/// core-only on the superseded leg); the rejected command stays dropped
/// (never queued, never replayed).
// Ten inputs are the disjoint production command products plus the leg
// mode; bundling would hide the borrow split the runtime relies on.
#[allow(clippy::too_many_arguments)]
fn run_world_command_impl(
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
    mode: CommandApplyMode,
) -> Option<tiler_core::world_report::WorldReply> {
    let trace = state.trace;
    let tick = state.tick;
    let log_path = state.log_path.clone();
    let command_kind = world_command_kind(&command);
    // Command-receipt clock, sampled at command entry before the admission
    // preamble below (binding/claim caches, catch-up fence service). Every
    // `recv_mono_ms` on this leg uses this stamp, never the post-preamble
    // dispatch clock, so receipt-to-phase latency starts at actual receipt.
    let recv_mono_ms = super::trace::log_mono_ms();
    admit_observed(state, me, observed);
    admit_retained_and_marks(state, me, observed, retained, areas);
    snapshot_frames(state, observed);
    super::rows::refresh_world_hints(state, observed, retained);
    // Superseded fence first: when observation-only catch-up is owed, service
    // it once through the native-free core-only observation before the single
    // command dispatch below, so the dispatch itself never meets a fence this
    // turn armed. The full leg never runs this: its post-refusal path below
    // applies owed catch-up natively instead.
    if mode == CommandApplyMode::Superseded && state.world.catchup_required() {
        run_observation_core_only(
            state,
            me,
            observed,
            retained,
            areas,
            foreground_hwnd,
            fulls,
            cursor,
        );
    }
    // The native stacking snapshot stashed by the latest observation;
    // capture maps it to opaque lifetimes for the pager order.
    let presentation_native_order = state.last_native_order.clone();
    // Normalized presentation ingress rides this tick's capture (stage 4a):
    // computed before the disjoint split borrows the loop.
    let gesture = state.gesture_ingress(cursor);
    // Raw cursor-hit HWND for the same tick as the cursor sample: the
    // exact managed binding resolves in capture over the owner tables.
    // `None` whenever the cursor itself is unsampled (game lane,
    // unreadable), so a hit never travels without its position.
    let hit_hwnd = super::observation::cursor_hit_hwnd(cursor);
    // Normalized presentation ingress rides this tick's capture (stage 5),
    // like the observation path. Commands never run on the game lane, so
    // the pause flag stays clear here.
    let (presentation_foreground, presentation_chord, presentation_move, presentation_style) =
        state.presentation_tick_inputs(me, fulls, foreground_hwnd);
    let presentation_border_enabled = state.border.enabled;
    let presentation_underlay_enabled = state.underlay.enabled;
    let presentation_paused = false;
    // Disjoint borrows: `world` mutably alongside every read-only input
    // field, never the whole loop (see `TileLoop::owner_inputs`).
    let TileLoop {
        ref mut world,
        ref member_tokens,
        ref member_identity,
        ref member_tags,
        ref mut member_rects,
        ref mut float_rects,
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
            presentation_foreground.clone(),
            presentation_chord,
            presentation_move.clone(),
            presentation_border_enabled,
            presentation_underlay_enabled,
            presentation_style,
            presentation_paused,
            presentation_native_order.clone(),
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
                        "recv_mono_ms": recv_mono_ms,
                        "delivery": "dispatch-refused",
                        "command": command_kind,
                        "reason": reason,
                    }),
                );
                let catchup_owed = matches!(
                    dispatch,
                    super::world_sequencer::DispatchError::CatchupRequired
                );
                // The superseded leg already serviced the fence before its
                // single dispatch: a refusal here (fence still owed, busy
                // flight, exhausted counters) drops the command honestly with
                // no second service and no re-dispatch. The next command's
                // own pre-dispatch inspection services any fence still owed.
                if mode == CommandApplyMode::Superseded {
                    return None;
                }
                if catchup_owed {
                    // Owed catch-up serviced synchronously on this turn's
                    // tick products: the fence stays (the refused command
                    // never queues or replays) but the next command must
                    // not strand behind an idle wait for an unrelated
                    // wake. Unserviceable fences (busy flight, exhausted
                    // counters) keep today's behavior.
                    run_observation_tick(
                        state,
                        me,
                        store,
                        dir,
                        observed,
                        retained,
                        areas,
                        foreground_hwnd,
                        fulls,
                        cursor,
                        false,
                    );
                }
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
                    "recv_mono_ms": recv_mono_ms,
                    "request": report.request_id.clone(),
                    "delivery": "rejected",
                    "command": command_kind,
                    "reject_kind": kind,
                    "reject_message": message,
                }),
            );
            world.note_rejected_or_lost(&report.request_id);
            // The rejection just armed observation-only catch-up; service
            // it synchronously on this turn's tick products instead of
            // stranding the next command behind an unrelated wake. The full
            // leg applies it natively; the superseded leg stays native-free
            // (core-only). Either way the rejected command stays dropped
            // (never queued, never replayed).
            if mode == CommandApplyMode::Superseded {
                run_observation_core_only(
                    state,
                    me,
                    observed,
                    retained,
                    areas,
                    foreground_hwnd,
                    fulls,
                    cursor,
                );
            } else {
                run_observation_tick(
                    state,
                    me,
                    store,
                    dir,
                    observed,
                    retained,
                    areas,
                    foreground_hwnd,
                    fulls,
                    cursor,
                    false,
                );
            }
            return None;
        }
    };
    if mode == CommandApplyMode::Superseded {
        // Order-only tail: the core incorporated this report under
        // `delivery`, but this leg plans/applies zero native writes itself.
        // Fence service on this leg is likewise native-free (the pre-dispatch
        // core-only observation above, or the core-only service on a core
        // rejection); only the full leg applies catch-up natively. The latest
        // reply's own `world-apply` row carries the actual native outcome;
        // this row must never read as one.
        super::trace::log_json_at(
            &log_path,
            serde_json::json!({
                "event": "world-apply",
                "tick": tick,
                "recv_mono_ms": recv_mono_ms,
                "request": report.request_id.clone(),
                "delivery": delivery,
                "command": command_kind,
                "native": "superseded-skipped",
            }),
        );
        let ack_outcome = ack_close_receipts(&mut *world, pending_closed, &reply, applied_homing);
        match ack_outcome {
            // Incorporated and current: obligations acked, desired snapshot
            // advanced core-side, but NO native planning, NO applied
            // watermark, NO trace proof, NO gesture clearing. The latest
            // full pass owns all of those.
            super::world_sequencer::AckOutcome::Applied(_) => return Some(reply),
            // Stale but correlated: incorporation acked where it happened,
            // catch-up scheduled. Return the cached reply like the full
            // path so the caller can still count the command as ordered;
            // the next dispatch services the owed catch-up through the
            // same fence as today.
            super::world_sequencer::AckOutcome::Stale(_) => {
                return world.last_reply().cloned();
            }
            super::world_sequencer::AckOutcome::Ignored { .. } => return None,
        }
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
        presentation_foreground.clone(),
        presentation_chord,
        presentation_move.clone(),
        presentation_border_enabled,
        presentation_underlay_enabled,
        presentation_style,
        presentation_paused,
        presentation_native_order.clone(),
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
        presentation_foreground.clone(),
        presentation_chord,
        presentation_move.clone(),
        presentation_border_enabled,
        presentation_underlay_enabled,
        presentation_style,
        presentation_paused,
        presentation_native_order.clone(),
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
    native.set_diagnostics_trace(trace);
    world.sync_diff_gate();
    // Fresh accepted selection arrivals only: the exact-desktop foreground
    // exception binds this report's accepted explicit arrival effect (the
    // grant carries the reply's own effect id plus target, minted from the
    // exact report command and sequence). Any other delivery (duplicate,
    // command-consumed) or command arms nothing; the observation path never
    // reaches this line.
    native.set_selection_arrival(super::world_diff::SelectionArrivalGrant::for_accepted(
        report.command.as_ref(),
        report.command_seq,
        &reply,
        delivery,
    ));
    let mut frame_attempts = Vec::new();
    let mut member_attempts = Vec::new();
    let mut view_attempts = Vec::new();
    let mut clear_attempts = Vec::new();
    let mut stats = world.apply_accepted(
        &reply,
        &fresh,
        &mut native,
        &mut frame_attempts,
        &mut member_attempts,
        &mut view_attempts,
        &mut clear_attempts,
        tick_ms,
    );
    #[cfg(test)]
    COMMAND_APPLY_COUNT.with(|slot| slot.set(slot.get() + 1));
    let hover_hit = captured.cursor_hit.as_ref().map(|hit| hit.0.clone());
    let hover_changed = world.hover_trace_changed(
        hover_hit.as_deref(),
        &captured.focused_global,
        captured.focus_follows_cursor,
    );
    // Bounded accepted trace-proof snapshot (TRACE-ONLY), same contract
    // as the observation path: command-consumed replies incorporate like
    // accepted ones, so they capture too; duplicates never relabel.
    let fresh_run = delivery != "duplicate";
    // Proven receipt snapshot plus raw-attempt post proofs (see the
    // observation path): re-proof of unchanged homing plus exact proven
    // arrivals with independent fresh revalidation. Never wholesale desire.
    //
    // Same deferred-emission order as the observation path: the existing
    // `receipt_updates` reads yield fresh final full-view state plus the
    // fresh homing arrival proof first, the world-apply JSON builds after,
    // with the owner-clock stamp immediately after `receipt_updates` and
    // both maps captured before merge. Trace-off builds no extra payload.
    let (mut proven_h, mut proven_v) = native.receipt_updates(&reply);
    let receipt_proof_mono_ms = if trace && fresh_run {
        Some(super::trace::log_mono_ms())
    } else {
        None
    };
    let mut receipt_views: Vec<(String, String)> = if trace && fresh_run {
        let mut rows: Vec<(String, String)> = proven_v
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        rows.sort();
        rows
    } else {
        Vec::new()
    };
    if trace && fresh_run {
        let mut homing_rows: Vec<(String, String, String)> = proven_h
            .iter()
            .map(|(token, (out, ws))| (token.clone(), out.clone(), ws.clone()))
            .collect();
        homing_rows.sort();
        stats.receipt_homing_total = homing_rows.len();
        homing_rows.truncate(super::world_diff::MAX_RECEIPT_HOMING_DIAG);
        stats.receipt_homing = homing_rows;
        // Current-order arrival fallback: the post-apply receipt instant
        // proves arrival only when the staged pre-focus proof did not fire
        // this pass. Never overwrites the staged stamp with this later one.
        if stats.arrivals_visible_mono_ms.is_none() && !stats.receipt_homing.is_empty() {
            stats.arrivals_visible_mono_ms = receipt_proof_mono_ms;
        }
    }
    merge_applied_transfers(
        &reply,
        &member_attempts,
        &view_attempts,
        &fresh,
        &mut native,
        &mut proven_h,
        &mut proven_v,
    );
    let proof_row = if trace && fresh_run {
        let mut row = apply_diag_value(
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
            hover_hit.clone(),
            captured.focus_follows_cursor,
            &report,
            &receipt_views,
            receipt_proof_mono_ms,
        );
        row["recv_mono_ms"] = serde_json::json!(recv_mono_ms);
        Some(row)
    } else {
        receipt_views.clear();
        None
    };
    let receipt_changed = trace
        && fresh_run
        && receipt_proof_changed(
            &receipt_views,
            &stats.receipt_homing,
            stats.receipt_homing_total,
            world.trace_proof(),
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
        &report,
        &receipt_views,
        receipt_proof_mono_ms,
        hover_changed,
        receipt_changed,
        recv_mono_ms,
    );
    if emitted {
        world.note_hover_traced(
            captured.cursor_hit.as_ref().map(|hit| hit.0.clone()),
            captured.focused_global.clone(),
            captured.focus_follows_cursor,
        );
    }
    // Hide-commit snapshot seeds, routed exactly like the observation path
    // above (see `run_observation_tick`).
    let hidden_seeds = native.take_hidden_seeds();
    drop(native);
    super::world_native::apply_hidden_seeds(
        hidden_seeds,
        member_tokens,
        member_rects,
        float_rects,
        |key| {
            let sticky = sticky.contains_key(key);
            let durable =
                fixed_tile_overrides.contains(key) || fixed_origin_overrides.contains(key);
            let automatic = fixed_auto_seen.contains(key) && !sticky && !durable;
            crate::native_guards::float_lane(
                sticky,
                automatic,
                durable,
                born_first_exit.contains(key),
                floated.contains(key),
                restart_reject_hold.contains(key),
                born_fullscreen.contains(key),
                true,
            )
        },
    );
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
    let ack_outcome = ack_close_receipts(&mut *world, pending_closed, &reply, applied_homing);
    match ack_outcome {
        super::world_sequencer::AckOutcome::Applied(_) => {
            world.note_diff_applied(&reply);
            // Incorporation proof only on a fresh run; a stale cached
            // reply never clears the stashed terminal. The paired
            // trace-proof advances here with the snapshot it proves.
            if fresh_run {
                TileLoop::clear_gesture_override_on_incorporation(
                    &mut state.gesture_override,
                    &reply,
                );
                world.note_trace_proof(proof_row);
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

/// Route one explicit [`WorldCommand`] through the single owner: dispatch
/// with the command, deliver, apply, and acknowledge like the observation
/// path. The command is never queued or replayed: while observation-only
/// catch-up is pending the dispatch refuses and the caller resubmits after,
/// and a core rejection drops the command. Either fence services the owed
/// observation-only catch-up synchronously on this turn's tick products
/// before returning, so the next command never strands behind an idle wait
/// for an unrelated wake.
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
    run_world_command_impl(
        state,
        me,
        store,
        dir,
        observed,
        retained,
        areas,
        foreground_hwnd,
        fulls,
        command,
        cursor,
        CommandApplyMode::Apply,
    )
}

/// Route one explicit [`WorldCommand`] through the single owner WITHOUT
/// native application by the dispatch itself: dispatch with the command,
/// deliver, and acknowledge the exact carried receipts, but plan/apply zero
/// native writes on this leg.
///
/// Adapter-only structural latest-only scheduling for superseded pure
/// workspace selections: every queued command still orders through the core
/// exactly once (counters, engine adoption, and flight receipts advance per
/// command), while only the batch's latest reply runs the native planning
/// pass in [`run_world_command`]. No timers, waits, or staged-arrival
/// changes: owed observation-only catch-up is inspected before the single
/// dispatch and serviced once through a native-free core-only observation
/// (never the applying observation path), then the command dispatches
/// exactly once. A refused command stays dropped (never queued, never
/// re-dispatched); a rejected command stays dropped (never queued, never
/// replayed), with its newly armed fence serviced synchronously through the
/// same native-free core-only observation.
///
/// Never sets the selection-arrival grant (only the latest applied reply may
/// arm one; grants are never copied between reports), never touches the diff
/// gate's applied watermark or trace proof, and never clears the stashed
/// gesture override: those belong to the latest applied incorporation only.
/// Close receipts still prune on exact equality even with no apply: an
/// incorporated report acks what it carried, and nothing attempted is
/// reported as successfully applied (the caller logs `superseded`, never
/// `ok`). All native hidden-write/effect/exact-desktop/flight gates stay
/// intact because they run inside the latest reply's own apply pass,
/// unchanged; a stale latest effect still drops honestly under the existing
/// rule there.
///
/// Scope is pure selection bursts (see `workspace_tick`): barrier commands
/// always take the full path and flush any pending select first.
// Ten inputs are the disjoint production command products; bundling would
// hide the borrow split the runtime relies on.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_world_command_core_only(
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
    run_world_command_impl(
        state,
        me,
        store,
        dir,
        observed,
        retained,
        areas,
        foreground_hwnd,
        fulls,
        command,
        cursor,
        CommandApplyMode::Superseded,
    )
}

/// Observe once through the production observation path and route one
/// [`WorldCommand`] core-only (no native planning or application on the
/// command leg). Enumeration failure (`None`) holds with no dispatch.
/// Returns the incorporated reply when the owner ordered it, so the caller
/// can count every queued command while no native planning or application
/// runs until the latest full pass (the shared admission preamble still
/// binds fail-closed like the full path).
pub(super) fn observe_and_command_core_only(
    state: &mut TileLoop,
    me: &crate::model::ProcessIdentity,
    store: &crate::storage::LedgerStore,
    dir: &std::path::Path,
    fulls: &[tiler_core::geometry::Rect],
    areas: &[MonitorArea],
    command: WorldCommand,
) -> Option<tiler_core::world_report::WorldReply> {
    let cursor = super::observation::sample_cursor();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let foreground = live_foreground_window();
    let observed = state.observe(me, fulls, &mut skipped, &mut retained)?;
    run_world_command_core_only(
        state, me, store, dir, &observed, &retained, areas, foreground, fulls, command, cursor,
    )
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
    // Offline seeding carries no native hit read and no presentation
    // facts: tests set `inputs.hit_hwnd` explicitly when they need a hit,
    // and drive presentation through explicit reports.
    let presentation_foreground: Option<super::world_capture::PresentationForeground> = None;
    let presentation_chord = false;
    let presentation_move = String::new();
    let presentation_border_enabled = false;
    let presentation_underlay_enabled = false;
    let presentation_style = (0, 0, 0);
    let presentation_paused = false;
    // The stashed native order from the latest `observe` (empty when the
    // test seeds without one): capture maps it to opaque lifetimes, else
    // falls back to observed-then-retained order.
    let presentation_native_order = state.last_native_order.clone();
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
        presentation_foreground.clone(),
        presentation_chord,
        presentation_move.clone(),
        presentation_border_enabled,
        presentation_underlay_enabled,
        presentation_style,
        presentation_paused,
        presentation_native_order.clone(),
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
            native_creations: Vec::new(),
            cursor_warp: None,
            presentation: tiler_core::world_presentation::WorldPresentation::default(),
        }
    }

    fn screen_shell(
        output: &str,
        full: tiler_core::geometry::Rect,
        work: tiler_core::geometry::Rect,
        gap: i32,
        outer_gap: i32,
        complete: bool,
        primary: bool,
    ) -> tiler_core::world_report::ScreenReport {
        tiler_core::world_report::ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(full),
            work_area: Some(work),
            gap,
            outer_gap,
            complete,
            primary,
        }
    }

    fn report_shell(
        screens: Vec<tiler_core::world_report::ScreenReport>,
    ) -> tiler_core::world_report::WorldReport {
        tiler_core::world_report::WorldReport {
            contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
            owner: "owner-1".to_owned(),
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq: 3,
            request_seq: 9,
            request_id: "req-9".to_owned(),
            command_seq: None,
            command: None,
            workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
            screens,
            windows: Vec::new(),
            views: Vec::new(),
            closed: Vec::new(),
            host_owned_workspace_domains: false,
            equal_workspace_count: false,
            birth_clear: false,
            native: Vec::new(),
            fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill:
                tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
            anchor_fallback: tiler_core::admission::AnchorFallback::default(),
            no_space_result: tiler_core::admission::NoSpaceResult::default(),
            born_maximized: tiler_core::born_maximized::BornMaximized::default(),
            switch_target: tiler_core::cursor_targeting::SwitchTarget::Cursor,
            fresh_placement: tiler_core::cursor_targeting::FreshPlacement::CursorOutputMru,
            focus_follows_cursor: false,
            cursor_warp: false,
            cursor: None,
            cursor_hit: None,
            focused_global: String::new(),
            action_foreground: None,
            action_cached: None,
            action_outputs: Vec::new(),
            default_tiled: true,
            inventory_complete: true,
            gesture_barrier: false,
            gesture: None,
            monotonic_ms: 0,
            frames: Vec::new(),
            member_attempts: Vec::new(),
            view_attempts: Vec::new(),
            clear_attempts: Vec::new(),
            creation_attempts: Vec::new(),
            presentation: tiler_core::world_report::PresentationIngress::disabled(),
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
            staged_reveals: 0,
            staged_focus: 0,
            staged_hides: 0,
            stopped_mid_apply: false,
            partial: false,
            focus_dispatch: Vec::new(),
            inflight_events: Vec::new(),
            inflight_total: 0,
            foreground_meta_omitted: 0,
            clear_holds: Vec::new(),
            clear_holds_total: 0,
            visibility_posts: Vec::new(),
            visibility_total: 0,
            show_posts_done_mono_ms: None,
            arrivals_visible_mono_ms: None,
            departures_hidden_mono_ms: None,
            receipt_homing: Vec::new(),
            receipt_homing_total: 0,
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
            &report_shell(vec![]),
            &[],
            None,
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
    fn diag_carries_bounded_pending_hook_causes() {
        // Trace-only pending-cause peek: counts only, no HWND/titles, at most
        // one row per cause. Offline the queue starts empty; staged edges
        // render bounded rows and clear again.
        use super::super::observe::{HookCause, HookEvent, WakeEdge, hook_queue};
        if let Ok(mut queue) = hook_queue().lock() {
            queue.clear();
        }
        let value = apply_diag_value(
            12,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
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
            &report_shell(vec![]),
            &[],
            None,
        );
        assert_eq!(value["invalidation_pending"], serde_json::json!([]));
        if let Ok(mut queue) = hook_queue().lock() {
            queue.push(HookEvent::Wake(WakeEdge::test(1, HookCause::Foreground)));
            queue.push(HookEvent::Wake(WakeEdge::test(2, HookCause::Foreground)));
            queue.push(HookEvent::Wake(WakeEdge::test(3, HookCause::Visibility)));
        }
        let value = apply_diag_value(
            13,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
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
            &report_shell(vec![]),
            &[],
            None,
        );
        assert_eq!(
            value["invalidation_pending"],
            serde_json::json!([
                {"cause": "foreground", "pending": 2},
                {"cause": "visibility", "pending": 1},
            ])
        );
        if let Ok(mut queue) = hook_queue().lock() {
            queue.clear();
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
                native_pre: None,
                native_post: None,
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
            &report_shell(vec![]),
            &[],
            None,
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
                native_pre: None,
                native_post: None,
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
                fg_baseline: None,
                fg_live: None,
                api: Some(super::super::world_diff::FOCUS_SETTER_API),
                call_mono_ms: Some(7),
                ret_mono_ms: Some(9),
                fg_proof_mono_ms: Some(10),
                gui_dispatch: Some("unknown".to_owned()),
                gui_landing: None,
                primitive_called: true,
                native_return: Some(false),
                prime_inserted: Some(2),
                hwnd: None,
                pid: None,
                creation: None,
                refusal_gate: None,
                refusal_reason: None,
                target_visible: None,
                target_cloaked: None,
                target_hidden: None,
                fg_identity: None,
                fg_gate_identity: None,
                fg_veto_kind: None,
                fg_readable: None,
                fg_shell: None,
                fg_fullscreen: None,
                fg_visible: None,
                fg_exe: None,
                desktop_exception: false,
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
                fg_baseline: None,
                fg_live: None,
                api: None,
                call_mono_ms: None,
                ret_mono_ms: None,
                fg_proof_mono_ms: None,
                gui_dispatch: None,
                gui_landing: None,
                primitive_called: false,
                native_return: None,
                prime_inserted: None,
                hwnd: None,
                pid: None,
                creation: None,
                refusal_gate: None,
                refusal_reason: None,
                target_visible: None,
                target_cloaked: None,
                target_hidden: None,
                fg_identity: None,
                fg_gate_identity: None,
                fg_veto_kind: None,
                fg_readable: None,
                fg_shell: None,
                fg_fullscreen: None,
                fg_visible: None,
                fg_exe: None,
                desktop_exception: false,
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

    fn pager_reply_with(foreground: &str, frame: tiler_core::geometry::Rect) -> WorldReply {
        use tiler_core::world_presentation::{PagerScreen, PagerWindow, PagerWorkspace};
        let mut reply = reply_shell();
        reply.presentation.foreground = foreground.to_owned();
        reply.presentation.pager = vec![PagerScreen {
            output: "out-a".to_owned(),
            workspaces: vec![PagerWorkspace {
                id: "ws-1".to_owned(),
                ordinal: 1,
                label: "Workspace 1".to_owned(),
                current: true,
                occupied: true,
                contains_focused: true,
                windows: vec![PagerWindow {
                    window: WindowId("w1".to_owned()),
                    frame,
                }],
            }],
        }];
        reply
    }

    #[test]
    fn quiet_accepted_row_stays_silent_without_presentation_forcing() {
        use tiler_core::geometry::Rect;
        use tiler_core::world_report::FrameAttempt;
        let quiet = super::super::world_diff::ApplyStats::default();
        let empty_members: Vec<tiler_core::world_report::MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        let empty_frames: Vec<FrameAttempt> = Vec::new();
        let idle = reply_shell();
        let moved = pager_reply_with(
            "w1",
            Rect {
                x: 0,
                y: 0,
                w: 961,
                h: 540,
            },
        );
        let route = fresh_route_value(
            "",
            None,
            "focused-output-last-focused",
            "cursor-output",
            (None, None, None, "retained"),
        );
        let report = report_shell(vec![]);
        static QUIET_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = QUIET_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tiler-world-quiet-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            seq
        ));
        std::fs::create_dir_all(&dir).expect("trace log dir");
        // A quiet Accepted pass stays silent even when its pager renderer
        // content changed: presentation-only forcing is retired; the pump
        // pairs the stored proof with its own payload emission instead.
        let silent_log = dir.join("silent.log");
        assert!(!log_apply_diag(
            &silent_log,
            true,
            41,
            &moved,
            "accepted",
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            route.clone(),
            None,
            false,
            &report,
            &[],
            None,
            false,
            false,
            0,
        ));
        assert!(!silent_log.exists());
        // The identical no-op stays quiet: no attempts, no hover, no
        // settlement forces nothing.
        let quiet_log = dir.join("quiet.log");
        assert!(!log_apply_diag(
            &quiet_log,
            true,
            42,
            &idle,
            "accepted",
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            route,
            None,
            false,
            &report,
            &[],
            None,
            false,
            false,
            0,
        ));
        assert!(!quiet_log.exists());
        // The trace gate still dominates: no row when trace is off, even
        // with a real presentation change.
        let dark_log = dir.join("dark.log");
        assert!(!log_apply_diag(
            &dark_log,
            false,
            43,
            &moved,
            "accepted",
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            fresh_route_value(
                "",
                None,
                "focused-output-last-focused",
                "cursor-output",
                (None, None, None, "retained"),
            ),
            None,
            false,
            &report,
            &[],
            None,
            false,
            false,
            0,
        ));
        assert!(!dark_log.exists());
    }

    #[test]
    fn paired_proof_binds_unchanged_revision40() {
        // Blocker shape (world 39 / pager 40 on one tick): two accepted
        // replies with identical content except the revision both stay
        // silent on the ordinary world log, yet each builds an exact
        // report-bound proof row the pump can pair with its payload.
        use tiler_core::geometry::Rect;
        use tiler_core::world_report::FrameAttempt;
        let quiet = super::super::world_diff::ApplyStats::default();
        let empty_members: Vec<tiler_core::world_report::MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        let empty_frames: Vec<FrameAttempt> = Vec::new();
        let frame = Rect {
            x: 0,
            y: 0,
            w: 960,
            h: 540,
        };
        let screens = vec![screen_shell(
            "o1",
            Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 560,
            },
            8,
            8,
            true,
            true,
        )];
        let report = report_shell(screens);
        let mut rev39 = pager_reply_with("w1", frame);
        rev39.state_revision = 39;
        let mut rev40 = rev39.clone();
        rev40.state_revision = 40;
        for reply in [&rev39, &rev40] {
            assert!(
                !should_log_apply(
                    "observation",
                    &quiet,
                    &empty_members,
                    &empty_views,
                    &[],
                    &empty_frames,
                    reply,
                ),
                "unchanged accept must stay silent on the ordinary log"
            );
        }
        let route = || {
            fresh_route_value(
                "",
                None,
                "focused-output-last-focused",
                "cursor-output",
                (None, None, None, "retained"),
            )
        };
        let row39 = apply_diag_value(
            41,
            &rev39,
            "accepted",
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            route(),
            None,
            false,
            &report,
            &[],
            None,
        );
        let row40 = apply_diag_value(
            42,
            &rev40,
            "accepted",
            "observation",
            &quiet,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            route(),
            None,
            false,
            &report,
            &[],
            None,
        );
        for (row, revision) in [(&row39, 39), (&row40, 40)] {
            assert_eq!(row["event"], serde_json::json!("world-apply"));
            assert_eq!(row["delivery"], serde_json::json!("accepted"));
            assert_eq!(row["revision"], serde_json::json!(revision));
            assert_eq!(row["observation"], serde_json::json!(3));
            assert_eq!(row["request"], serde_json::json!("req-9"));
            // Actual corresponding report provenance: paired geometry with
            // a numeric zero omission, never a null or missing field.
            assert_eq!(row["geometry_status"], serde_json::json!("paired"));
            assert_eq!(row["desired_geometry_omitted"], serde_json::json!(0));
            assert_eq!(
                row["geometry_report"]["request"],
                serde_json::json!("req-9")
            );
            assert_eq!(row["geometry_report"]["observation"], serde_json::json!(3));
        }
        assert_ne!(row39["revision"], row40["revision"]);
    }

    #[test]
    fn paired_proof_row_is_honest_over_caps() {
        // Over-cap geometry stays honest in stored proof rows: the capped
        // rows render with the unbounded total behind the omitted count, so
        // the runner still fails closed instead of trusting a truncated
        // proof.
        use tiler_core::geometry::Rect;
        use tiler_core::world_report::FrameAttempt;
        let quiet = super::super::world_diff::ApplyStats::default();
        let empty_frames: Vec<FrameAttempt> = Vec::new();
        let mut screens = Vec::new();
        for n in 0..(MAX_APPLY_DIAG_SCREENS + 1) {
            screens.push(screen_shell(
                &format!("o{n}"),
                Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 560,
                },
                8,
                8,
                true,
                n == 0,
            ));
        }
        let report = report_shell(screens);
        let reply = reply_shell();
        let row = apply_diag_value(
            10,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &empty_frames,
            serde_json::json!({}),
            None,
            false,
            &report,
            &[],
            None,
        );
        assert_eq!(row["geometry_status"], serde_json::json!("paired"));
        assert_eq!(row["desired_geometry_omitted"], serde_json::json!(1));
        assert_eq!(
            row["desired_geometry"]
                .as_array()
                .expect("geometry rows")
                .len(),
            MAX_APPLY_DIAG_SCREENS
        );
    }

    #[test]
    fn inflight_clearing_logs_quiet_observation() {
        // A pass that only cleared a remembered flight (landing,
        // supersession, or lifetime end) carries first-class evidence: the
        // observation tick emits its clearing row instead of staying quiet.
        use super::super::world_diff::InFlightEventDiag;
        let empty_members: Vec<tiler_core::world_report::MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        let empty_frames: Vec<tiler_core::world_report::FrameAttempt> = Vec::new();
        let idle_reply = reply_shell();
        let mut landed = super::super::world_diff::ApplyStats::default();
        landed.note_inflight(InFlightEventDiag {
            token: "w1".to_owned(),
            effect: "fx-1".to_owned(),
            cause: "landed-own",
            edge_event_ms: Some(31),
            edge_queued_mono_ms: Some(11),
            edge_drain_mono_ms: Some(13),
            resolve_mono_ms: 21,
            gui: Some("thread:7:active:1001:focus:1001".to_owned()),
            hwnd: Some(1001),
            pid: Some(2001),
            creation: Some("proc-1".to_owned()),
        });
        assert!(should_log_apply(
            "observation",
            &landed,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
        ));
        // Omitted-count-only evidence logs too: the fold itself is new.
        let folded = super::super::world_diff::ApplyStats {
            foreground_meta_omitted: 2,
            ..Default::default()
        };
        assert!(should_log_apply(
            "observation",
            &folded,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
        ));
    }

    #[test]
    fn visibility_posts_log_quiet_observation() {
        // A pass whose only evidence is executed visibility posts (or
        // their omission past the row cap) emits its rows instead of
        // staying quiet. Rows name the executed host API and the
        // activation-affecting parameter, never intended-mode labels.
        use crate::product_hide::sys::{NA_POS_HIDE_FLAGS, VIEW_POST_API_POS, VisibilityPostDiag};
        let empty_members: Vec<tiler_core::world_report::MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        let empty_frames: Vec<tiler_core::world_report::FrameAttempt> = Vec::new();
        let idle_reply = reply_shell();
        let mut posted = super::super::world_diff::ApplyStats::default();
        posted.note_visibility_post(VisibilityPostDiag {
            hwnd: 1001,
            hide: true,
            api: VIEW_POST_API_POS,
            flags: Some(NA_POS_HIDE_FLAGS),
            cmd: None,
            posted: true,
            call_mono_ms: 11,
            ret_mono_ms: 13,
        });
        assert!(should_log_apply(
            "observation",
            &posted,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
        ));
        let value = apply_diag_value(
            9,
            &idle_reply,
            "accepted",
            "observation",
            &posted,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[],
            None,
        );
        let rows = value["visibility_posts"]
            .as_array()
            .expect("visibility rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["hwnd"], serde_json::json!(1001));
        assert_eq!(rows[0]["hide"], serde_json::json!(true));
        assert_eq!(rows[0]["api"], serde_json::json!(VIEW_POST_API_POS));
        assert_eq!(rows[0]["flags"], serde_json::json!(NA_POS_HIDE_FLAGS));
        assert_eq!(rows[0]["posted"], serde_json::json!(true));
        assert_eq!(value["visibility_posts_omitted"], serde_json::json!(0));
        // Omitted-count-only evidence logs too.
        let folded = super::super::world_diff::ApplyStats {
            visibility_total: 9,
            ..Default::default()
        };
        assert!(should_log_apply(
            "observation",
            &folded,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            &idle_reply,
        ));
        // Over-cap source storage stays honest in world-apply: the capped
        // rows render with the unbounded total behind the omitted count.
        use super::super::world_diff::MAX_VISIBILITY_DIAG;
        let mut capped = super::super::world_diff::ApplyStats::default();
        for n in 0..(MAX_VISIBILITY_DIAG + 3) {
            capped.note_visibility_post(VisibilityPostDiag {
                hwnd: 2000 + n as u64,
                hide: true,
                api: VIEW_POST_API_POS,
                flags: Some(NA_POS_HIDE_FLAGS),
                cmd: None,
                posted: true,
                call_mono_ms: 11,
                ret_mono_ms: 13,
            });
        }
        let capped_value = apply_diag_value(
            10,
            &idle_reply,
            "accepted",
            "observation",
            &capped,
            &empty_members,
            &empty_views,
            &[],
            &empty_frames,
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[],
            None,
        );
        assert_eq!(
            capped_value["visibility_posts"]
                .as_array()
                .expect("capped visibility rows")
                .len(),
            MAX_VISIBILITY_DIAG
        );
        assert_eq!(
            capped_value["visibility_posts_omitted"],
            serde_json::json!(3)
        );
    }

    #[test]
    fn diag_per_phase_stamps_join_receipt_to_attempts_and_staged_counts() {
        // End-to-end measurement join on one row: per-attempt actuation
        // stamps (every show issued) plus staged phase counts plus the
        // fresh receipt proof ride with the command vocabulary, so a live
        // leg can bind command receipt to each observed required phase
        // (shows issued, arrivals proven, focus confirmed, departures
        // hidden) with no new native query. Unavailable staged phases stay
        // honest zeros, never absent keys.
        use tiler_core::world_report::{ClearAttempt, FrameSetter, MemberAttempt, ViewAttempt};
        let members = vec![MemberAttempt {
            operation_id: "op-3".to_owned(),
            window: WindowId("w3".to_owned()),
            target_output: OutputId("o1".to_owned()),
            target_workspace: "ws2".to_owned(),
            setter: FrameSetter::Threw,
            pre: None,
            post: Some((OutputId("o1".to_owned()), "ws2".to_owned())),
            native_pre: None,
            native_post: None,
            attempt_ms: 31,
        }];
        let views = vec![ViewAttempt {
            operation_id: "op-7".to_owned(),
            target_output: OutputId("o1".to_owned()),
            target_workspace: "ws2".to_owned(),
            setter: FrameSetter::Threw,
            pre: Some("ws1".to_owned()),
            post: Some("ws2".to_owned()),
            baseline: Some("ws1".to_owned()),
            native_pre: None,
            native_post: None,
            native_baseline: None,
            attempt_ms: 32,
        }];
        let clears = vec![ClearAttempt {
            operation_id: "op-9".to_owned(),
            window: WindowId("w9".to_owned()),
            setter: FrameSetter::Threw,
            pre_maximized: Some(true),
            post_maximized: Some(false),
            attempt_ms: 33,
        }];
        let mut stats = stats_shell();
        stats.staged_reveals = 1;
        stats.staged_focus = 1;
        assert_eq!(stats.staged_hides, 0);
        let value = apply_diag_value(
            9,
            &reply_shell(),
            "accepted",
            "select-workspace",
            &stats,
            &members,
            &views,
            &clears,
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[("o1".to_owned(), "ws2".to_owned())],
            Some(40),
        );
        assert_eq!(value["members"][0]["attempt_ms"], serde_json::json!(31));
        assert_eq!(value["views"][0]["attempt_ms"], serde_json::json!(32));
        assert_eq!(value["views"][0]["out"], serde_json::json!("o1"));
        assert_eq!(value["views"][0]["ws"], serde_json::json!("ws2"));
        assert_eq!(value["clears"][0]["attempt_ms"], serde_json::json!(33));
        assert_eq!(value["stats"]["staged_reveals"], serde_json::json!(1));
        assert_eq!(value["stats"]["staged_focus"], serde_json::json!(1));
        assert_eq!(value["stats"]["staged_hides"], serde_json::json!(0));
        assert_eq!(
            value["receipt_views"],
            serde_json::json!([{"output": "o1", "view": "ws2"}])
        );
        assert_eq!(value["receipt_proof_mono_ms"], serde_json::json!(40));
    }

    #[test]
    fn diag_phase_stamps_cover_show_arrival_departure_and_homing() {
        // New phase stamps ride the same row with exact joins: staged
        // arrival pre-proof plus post-apply homing rows (target-filtered)
        // plus departure proof, with empty phases staying null/empty.
        let mut stats = stats_shell();
        stats.show_posts_done_mono_ms = Some(34);
        stats.arrivals_visible_mono_ms = Some(35);
        stats.departures_hidden_mono_ms = Some(37);
        stats.receipt_homing = vec![("w3".to_owned(), "o1".to_owned(), "ws2".to_owned())];
        stats.receipt_homing_total = 1;
        let value = apply_diag_value(
            9,
            &reply_shell(),
            "accepted",
            "select-workspace",
            &stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[("o1".to_owned(), "ws2".to_owned())],
            Some(40),
        );
        assert_eq!(
            value["stats"]["show_posts_done_mono_ms"],
            serde_json::json!(34)
        );
        assert_eq!(
            value["stats"]["arrivals_visible_mono_ms"],
            serde_json::json!(35)
        );
        assert_eq!(
            value["stats"]["departures_hidden_mono_ms"],
            serde_json::json!(37)
        );
        assert_eq!(
            value["receipt_homing"],
            serde_json::json!([{"window": "w3", "output": "o1", "view": "ws2"}])
        );
        assert_eq!(value["receipt_homing_omitted"], serde_json::json!(0));
        // Empty arrival stays N/A: null stamps plus empty maps, never
        // fabricated success.
        let empty = apply_diag_value(
            10,
            &reply_shell(),
            "accepted",
            "select-workspace",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[],
            Some(41),
        );
        assert!(empty["stats"]["show_posts_done_mono_ms"].is_null());
        assert!(empty["stats"]["arrivals_visible_mono_ms"].is_null());
        assert!(empty["stats"]["departures_hidden_mono_ms"].is_null());
        assert_eq!(empty["receipt_homing"], serde_json::json!([]));
        assert_eq!(empty["receipt_homing_omitted"], serde_json::json!(0));
    }

    #[test]
    fn diag_emitted_row_carries_command_receipt_stamp() {
        // The emitted world-apply row carries the pass-entry owner-clock
        // receipt stamp, so end-to-end latency reads command receipt to
        // last observed phase with no new query and no payload bytes.
        let dir = std::env::temp_dir().join(format!(
            "tiler-recv-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let log = dir.join("recv.log");
        assert!(log_apply_diag(
            &log,
            true,
            9,
            &reply_shell(),
            "accepted",
            "select-workspace",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[],
            None,
            false,
            false,
            4242,
        ));
        let text = std::fs::read_to_string(&log).expect("row");
        let row: serde_json::Value =
            serde_json::from_str(text.trim()).expect("row is one JSON line");
        assert_eq!(row["recv_mono_ms"], serde_json::json!(4242));
        assert_eq!(row["command"], serde_json::json!("select-workspace"));
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
                fg_baseline: None,
                fg_live: None,
                api: None,
                call_mono_ms: None,
                ret_mono_ms: None,
                fg_proof_mono_ms: None,
                gui_dispatch: None,
                gui_landing: None,
                primitive_called: false,
                native_return: None,
                prime_inserted: None,
                hwnd: None,
                pid: None,
                creation: None,
                refusal_gate: None,
                refusal_reason: None,
                target_visible: None,
                target_cloaked: None,
                target_hidden: None,
                fg_identity: None,
                fg_gate_identity: None,
                fg_veto_kind: None,
                fg_readable: None,
                fg_shell: None,
                fg_fullscreen: None,
                fg_visible: None,
                fg_exe: None,
                desktop_exception: false,
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
            &report_shell(vec![]),
            &[],
            None,
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
        assert!(dispatch[0]["fg_baseline"].is_null());
        assert!(dispatch[0]["fg_live"].is_null());
        // Skipped setters carry no call window, API, or dispatch-time GUI
        // read: nulls, never fabricated zeros.
        assert!(dispatch[0]["api"].is_null());
        assert!(dispatch[0]["call_mono_ms"].is_null());
        assert!(dispatch[0]["ret_mono_ms"].is_null());
        assert!(dispatch[0]["gui_dispatch"].is_null());
        assert!(dispatch[0]["prime_inserted"].is_null());
        // No flight ended this pass: the clearing array is empty with a
        // zero total, not an absent field.
        assert_eq!(value["inflight_events"], serde_json::json!([]));
        assert_eq!(value["inflight_events_omitted"], serde_json::json!(0));
        // No foreground-edge metadata folded this pass either: the sidecar
        // omission rides the same diag, zero when nothing folded.
        assert_eq!(value["foreground_meta_omitted"], serde_json::json!(0));
    }

    #[test]
    fn diag_focus_refusal_detail_rides_only_refusal_rows() {
        // Refusal-only volume: the six refusal keys exist solely on traced
        // `Ok(false)` veto rows. A traced move/size refusal projects its
        // gate plus safe state accurately, while success and skipped rows in
        // the same pass carry no such keys at all (absent, never null).
        fn dispatch_row(
            id: &str,
            setter: &'static str,
            foreground: bool,
            hold: Option<&'static str>,
            primitive_called: bool,
            refusal_gate: Option<&'static str>,
        ) -> super::super::world_diff::FocusDispatchDiag {
            super::super::world_diff::FocusDispatchDiag {
                id: id.to_owned(),
                target: "w1".to_owned(),
                kind: "focus",
                dispatched: true,
                setter,
                foreground,
                hold,
                fg_baseline: None,
                fg_live: None,
                api: None,
                call_mono_ms: None,
                ret_mono_ms: None,
                fg_proof_mono_ms: None,
                gui_dispatch: None,
                gui_landing: None,
                primitive_called,
                native_return: None,
                prime_inserted: None,
                hwnd: Some(61001),
                pid: Some(2301),
                creation: Some("proc-308".to_owned()),
                refusal_gate,
                refusal_reason: None,
                target_visible: Some(true),
                target_cloaked: Some(false),
                target_hidden: Some(true),
                fg_identity: Some("none"),
                fg_gate_identity: None,
                fg_veto_kind: None,
                fg_readable: None,
                fg_shell: None,
                fg_fullscreen: None,
                fg_visible: None,
                fg_exe: None,
                desktop_exception: false,
            }
        }
        let reply = reply_shell();
        let stats = super::super::world_diff::ApplyStats {
            held: 2,
            focus_dispatch: vec![
                dispatch_row(
                    "ffc-refused",
                    "refused",
                    false,
                    Some("foreground-unproven"),
                    false,
                    Some("move-size-veto"),
                ),
                super::super::world_diff::FocusDispatchDiag {
                    refusal_gate: None,
                    refusal_reason: None,
                    target_visible: None,
                    target_cloaked: None,
                    target_hidden: None,
                    fg_identity: None,
                    fg_gate_identity: None,
                    fg_veto_kind: None,
                    fg_readable: None,
                    fg_shell: None,
                    fg_fullscreen: None,
                    fg_visible: None,
                    fg_exe: None,
                    ..dispatch_row("ffc-ok", "accepted", true, None, true, None)
                },
                super::super::world_diff::FocusDispatchDiag {
                    refusal_gate: None,
                    refusal_reason: None,
                    target_visible: None,
                    target_cloaked: None,
                    target_hidden: None,
                    fg_identity: None,
                    fg_gate_identity: None,
                    fg_veto_kind: None,
                    fg_readable: None,
                    fg_shell: None,
                    fg_fullscreen: None,
                    fg_visible: None,
                    fg_exe: None,
                    ..dispatch_row(
                        "ffc-skipped",
                        "skipped",
                        false,
                        Some("view-not-shown"),
                        false,
                        None,
                    )
                },
            ],
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
            &report_shell(vec![]),
            &[],
            None,
        );
        let dispatch = value["focus_dispatch"].as_array().expect("dispatch");
        assert_eq!(dispatch.len(), 3);
        // Traced refusal: gate plus safe state projected accurately.
        assert_eq!(
            dispatch[0]["refusal_gate"],
            serde_json::json!("move-size-veto")
        );
        assert!(dispatch[0]["refusal_reason"].is_null());
        assert_eq!(dispatch[0]["target_visible"], serde_json::json!(true));
        assert_eq!(dispatch[0]["target_cloaked"], serde_json::json!(false));
        assert_eq!(dispatch[0]["target_hidden"], serde_json::json!(true));
        assert_eq!(dispatch[0]["fg_identity"], serde_json::json!("none"));
        // A non-focus-write gate carries no gate-time keys either.
        for key in [
            "fg_gate_identity",
            "fg_veto_kind",
            "fg_readable",
            "fg_shell",
            "fg_fullscreen",
            "fg_visible",
            "fg_exe",
        ] {
            assert!(
                dispatch[0].get(key).is_none(),
                "non-focus-write refusal row carries {key}"
            );
        }
        // Success and skipped rows: keys absent, never null.
        for row in dispatch.iter().skip(1) {
            for key in [
                "refusal_gate",
                "refusal_reason",
                "target_visible",
                "target_cloaked",
                "target_hidden",
                "fg_identity",
                "fg_gate_identity",
                "fg_veto_kind",
                "fg_readable",
                "fg_shell",
                "fg_fullscreen",
                "fg_visible",
                "fg_exe",
            ] {
                assert!(row.get(key).is_none(), "non-refusal row carries {key}");
            }
        }
    }

    #[test]
    fn diag_focus_write_veto_gate_detail_rides_refusal_row() {
        // Gate-time volume: a traced `focus-write-veto` refusal projects
        // the veto's own attribution (relation, kind, readability, shell,
        // fullscreen, visibility, exe basename) while a same-pass success
        // row omits every refusal key.
        fn veto_row() -> super::super::world_diff::FocusDispatchDiag {
            super::super::world_diff::FocusDispatchDiag {
                id: "ffc-veto".to_owned(),
                target: "w1".to_owned(),
                kind: "focus",
                dispatched: true,
                setter: "refused",
                foreground: false,
                hold: Some("foreground-unproven"),
                fg_baseline: None,
                fg_live: None,
                api: None,
                call_mono_ms: None,
                ret_mono_ms: None,
                fg_proof_mono_ms: None,
                gui_dispatch: None,
                gui_landing: None,
                primitive_called: false,
                native_return: None,
                prime_inserted: None,
                hwnd: Some(61001),
                pid: Some(2301),
                creation: Some("proc-308".to_owned()),
                refusal_gate: Some("focus-write-veto"),
                refusal_reason: None,
                target_visible: Some(true),
                target_cloaked: Some(false),
                target_hidden: Some(false),
                fg_identity: Some("other"),
                fg_gate_identity: Some("other"),
                fg_veto_kind: Some("readable-fullscreen"),
                fg_readable: Some(true),
                fg_shell: Some(true),
                fg_fullscreen: Some(true),
                fg_visible: Some(true),
                fg_exe: Some("explorer.exe".to_owned()),
                desktop_exception: false,
            }
        }
        let reply = reply_shell();
        let mut stats = super::super::world_diff::ApplyStats {
            held: 1,
            focus_dispatch: vec![
                veto_row(),
                super::super::world_diff::FocusDispatchDiag {
                    id: "ffc-ok".to_owned(),
                    ..veto_row()
                },
            ],
            ..Default::default()
        };
        // Rewrite the second row as a proven success with no refusal
        // detail (struct-update above copies the veto base first).
        stats.focus_dispatch[1].setter = "accepted";
        stats.focus_dispatch[1].foreground = true;
        stats.focus_dispatch[1].hold = None;
        stats.focus_dispatch[1].refusal_gate = None;
        stats.focus_dispatch[1].refusal_reason = None;
        stats.focus_dispatch[1].target_visible = None;
        stats.focus_dispatch[1].target_cloaked = None;
        stats.focus_dispatch[1].target_hidden = None;
        stats.focus_dispatch[1].fg_identity = None;
        stats.focus_dispatch[1].fg_gate_identity = None;
        stats.focus_dispatch[1].fg_veto_kind = None;
        stats.focus_dispatch[1].fg_readable = None;
        stats.focus_dispatch[1].fg_shell = None;
        stats.focus_dispatch[1].fg_fullscreen = None;
        stats.focus_dispatch[1].fg_visible = None;
        stats.focus_dispatch[1].fg_exe = None;
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
            &report_shell(vec![]),
            &[],
            None,
        );
        let dispatch = value["focus_dispatch"].as_array().expect("dispatch");
        assert_eq!(dispatch.len(), 2);
        assert_eq!(
            dispatch[0]["refusal_gate"],
            serde_json::json!("focus-write-veto")
        );
        assert_eq!(dispatch[0]["fg_gate_identity"], serde_json::json!("other"));
        assert_eq!(
            dispatch[0]["fg_veto_kind"],
            serde_json::json!("readable-fullscreen")
        );
        assert_eq!(dispatch[0]["fg_readable"], serde_json::json!(true));
        assert_eq!(dispatch[0]["fg_shell"], serde_json::json!(true));
        assert_eq!(dispatch[0]["fg_fullscreen"], serde_json::json!(true));
        assert_eq!(dispatch[0]["fg_visible"], serde_json::json!(true));
        assert_eq!(dispatch[0]["fg_exe"], serde_json::json!("explorer.exe"));
        for key in [
            "refusal_gate",
            "refusal_reason",
            "target_visible",
            "target_cloaked",
            "target_hidden",
            "fg_identity",
            "fg_gate_identity",
            "fg_veto_kind",
            "fg_readable",
            "fg_shell",
            "fg_fullscreen",
            "fg_visible",
            "fg_exe",
            "desktop_exception",
        ] {
            assert!(dispatch[1].get(key).is_none(), "success row carries {key}");
        }
    }

    #[test]
    fn diag_desktop_exception_key_rides_only_exception_rows() {
        // The exact-desktop marker serializes solely on dispatches that
        // used the exception; every other row omits the key entirely
        // (never `false`), preserving the prior row shape for parsers.
        fn exception_row() -> super::super::world_diff::FocusDispatchDiag {
            super::super::world_diff::FocusDispatchDiag {
                id: "fx-7".to_owned(),
                target: "w6".to_owned(),
                kind: "focus",
                dispatched: true,
                setter: "accepted",
                foreground: false,
                hold: Some("foreground-unproven"),
                fg_baseline: None,
                fg_live: None,
                api: Some(super::super::world_diff::FOCUS_SETTER_API),
                call_mono_ms: Some(7),
                ret_mono_ms: Some(9),
                fg_proof_mono_ms: Some(10),
                gui_dispatch: None,
                gui_landing: None,
                primitive_called: true,
                native_return: Some(true),
                prime_inserted: Some(2),
                hwnd: Some(120_001),
                pid: Some(2401),
                creation: Some("proc-401".to_owned()),
                refusal_gate: None,
                refusal_reason: None,
                target_visible: None,
                target_cloaked: None,
                target_hidden: None,
                fg_identity: None,
                fg_gate_identity: None,
                fg_veto_kind: None,
                fg_readable: None,
                fg_shell: None,
                fg_fullscreen: None,
                fg_visible: None,
                fg_exe: None,
                desktop_exception: true,
            }
        }
        let reply = reply_shell();
        let stats = super::super::world_diff::ApplyStats {
            focus_dispatch: vec![exception_row()],
            ..Default::default()
        };
        let value = apply_diag_value(
            17,
            &reply,
            "accepted",
            "select-workspace",
            &stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            Some("w0".to_owned()),
            false,
            &report_shell(vec![]),
            &[],
            None,
        );
        let dispatch = value["focus_dispatch"].as_array().expect("dispatch");
        assert_eq!(dispatch.len(), 1);
        assert_eq!(dispatch[0]["desktop_exception"], serde_json::json!(true));
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
            &report_shell(vec![]),
            &[],
            None,
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
            &report_shell(vec![]),
            &[],
            None,
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
                    fg_baseline: None,
                    fg_live: None,
                    api: None,
                    call_mono_ms: None,
                    ret_mono_ms: None,
                    fg_proof_mono_ms: None,
                    gui_dispatch: None,
                    gui_landing: None,
                    primitive_called: false,
                    native_return: None,
                    prime_inserted: None,
                    hwnd: None,
                    pid: None,
                    creation: None,
                    refusal_gate: None,
                    refusal_reason: None,
                    target_visible: None,
                    target_cloaked: None,
                    target_hidden: None,
                    fg_identity: None,
                    fg_gate_identity: None,
                    fg_veto_kind: None,
                    fg_readable: None,
                    fg_shell: None,
                    fg_fullscreen: None,
                    fg_visible: None,
                    fg_exe: None,
                    desktop_exception: false,
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
            &report_shell(vec![]),
            &[],
            None,
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
            &report_shell(vec![]),
            &[],
            None,
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
            &report_shell(vec![]),
            &[],
            None,
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
            &report_shell(vec![]),
            &[],
            None,
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
            &report_shell(vec![]),
            &[],
            None,
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

    #[test]
    fn diag_desired_windows_separate_allocation_from_effective_frame_and_views() {
        // Accepted desire projection: allocation (canonical slot) stays
        // separate from effective_frame (only write target), with
        // per-property authority, floating, homing, and op tokens from the
        // exact reply. Logical views ride `desired_outputs`, never mixed
        // with raw view-attempt pre/post/base proof. Held rows appear with
        // their hold reason and no fabricated frame attempt. Geometry rides
        // from the dispatched report screens with wire output ids, never a
        // separate native copy.
        use tiler_core::world_report::{
            FrameSetter, HoldReason, MemberAttempt, OutputDesire, ScopeAuthority, TilingMode,
            ViewAttempt, WindowDesire,
        };
        let slot = tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        };
        let target = tiler_core::geometry::Rect {
            x: 8,
            y: 8,
            w: 384,
            h: 544,
        };
        let mut reply = reply_shell();
        reply.windows.push(WindowDesire {
            lifetime: WindowId("w-tiled".to_owned()),
            output: OutputId("o1".to_owned()),
            workspace: "ws-a".to_owned(),
            allocation: Some(slot),
            effective_frame: Some(target),
            frame_authority: ScopeAuthority::Controlled,
            membership_authority: ScopeAuthority::Controlled,
            floating: false,
            float_origin: None,
            frame_operation: None,
            clear_operation: None,
        });
        reply.windows.push(WindowDesire {
            lifetime: WindowId("w-held".to_owned()),
            output: OutputId("o1".to_owned()),
            workspace: "ws-a".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: ScopeAuthority::Hold(HoldReason::Unreadable),
            membership_authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            floating: false,
            float_origin: None,
            frame_operation: None,
            clear_operation: None,
        });
        reply.outputs.push(OutputDesire {
            output: OutputId("o1".to_owned()),
            logical_view: Some("ws-a".to_owned()),
            tiling_mode: TilingMode::Tiled,
            hold: None,
        });
        let report = report_shell(vec![screen_shell(
            "o1",
            tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            slot,
            8,
            12,
            true,
            true,
        )]);
        let member_attempts = vec![MemberAttempt {
            operation_id: "op-9".to_owned(),
            window: WindowId("w-tiled".to_owned()),
            target_output: OutputId("o1".to_owned()),
            target_workspace: "ws-a".to_owned(),
            setter: FrameSetter::Accepted,
            pre: Some((OutputId("o1".to_owned()), "ws-a".to_owned())),
            post: Some((OutputId("o1".to_owned()), "ws-a".to_owned())),
            native_pre: None,
            native_post: None,
            attempt_ms: 42,
        }];
        let view_attempts = vec![ViewAttempt {
            operation_id: "op-9".to_owned(),
            target_output: OutputId("o1".to_owned()),
            target_workspace: "ws-b".to_owned(),
            setter: FrameSetter::Accepted,
            pre: Some("ws-old".to_owned()),
            post: Some("ws-b".to_owned()),
            baseline: Some("ws-old".to_owned()),
            native_pre: None,
            native_post: None,
            native_baseline: None,
            attempt_ms: 42,
        }];
        let value = apply_diag_value(
            30,
            &reply,
            "accepted",
            "send",
            &stats_shell(),
            &member_attempts,
            &view_attempts,
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report,
            &[],
            None,
        );
        // Provenance joins desire to attempts: same request/revision/
        // observation on the row as the accepted reply, same window/op
        // vocabulary on both sides, same attempt_ms stamp.
        assert_eq!(value["request"], serde_json::json!("req-9"));
        assert_eq!(value["revision"], serde_json::json!(7));
        assert_eq!(value["observation"], serde_json::json!(3));
        assert_eq!(value["delivery"], serde_json::json!("accepted"));
        let desired = value["desired_windows"]
            .as_array()
            .expect("desired windows");
        assert_eq!(desired.len(), 2);
        assert_eq!(desired[0]["window"], serde_json::json!("w-tiled"));
        assert_eq!(desired[0]["allocation"]["w"], serde_json::json!(800));
        assert_eq!(desired[0]["effective_frame"]["w"], serde_json::json!(384));
        assert_ne!(desired[0]["allocation"], desired[0]["effective_frame"]);
        assert_eq!(
            desired[0]["frame_authority"],
            serde_json::json!("controlled")
        );
        assert!(desired[0]["frame_hold"].is_null());
        assert_eq!(desired[0]["floating"], serde_json::json!(false));
        assert!(desired[0]["frame_op"].is_null());
        // Held row keeps its explicit hold reasons and stays pending with
        // no fabricated frame write.
        assert_eq!(desired[1]["window"], serde_json::json!("w-held"));
        assert!(desired[1]["allocation"].is_null());
        assert!(desired[1]["effective_frame"].is_null());
        assert_eq!(desired[1]["frame_authority"], serde_json::json!("hold"));
        assert_eq!(desired[1]["frame_hold"], serde_json::json!("unreadable"));
        assert_eq!(
            desired[1]["membership_hold"],
            serde_json::json!("incomplete-inventory")
        );
        assert_eq!(value["desired_windows_omitted"], serde_json::json!(0));
        assert_eq!(value["frames"], serde_json::json!([]));
        // Logical views stay distinct from attempt views: desired carries
        // the core-owned logical target, attempts carry mechanical proof.
        let outputs = value["desired_outputs"]
            .as_array()
            .expect("desired outputs");
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0]["output"], serde_json::json!("o1"));
        assert_eq!(outputs[0]["logical_view"], serde_json::json!("ws-a"));
        assert_eq!(value["desired_outputs_omitted"], serde_json::json!(0));
        let views = value["views"].as_array().expect("view attempts");
        assert_eq!(views.len(), 1);
        assert_eq!(views[0]["pre"], serde_json::json!("ws-old"));
        assert_ne!(outputs[0]["logical_view"], views[0]["pre"]);
        // Report geometry basis rides paired alongside desire: wire output
        // ids join `desired_outputs` and attempt `out` fields, with the
        // per-screen settings gaps from the dispatched report.
        assert_eq!(value["geometry_status"], serde_json::json!("paired"));
        let geometry = value["desired_geometry"]
            .as_array()
            .expect("desired geometry");
        assert_eq!(geometry.len(), 1);
        assert_eq!(geometry[0]["output"], serde_json::json!("o1"));
        assert_eq!(geometry[0]["work"]["w"], serde_json::json!(800));
        assert_eq!(geometry[0]["full"]["h"], serde_json::json!(600));
        assert_eq!(geometry[0]["gap"], serde_json::json!(8));
        assert_eq!(geometry[0]["outer_gap"], serde_json::json!(12));
        assert_eq!(value["desired_geometry_omitted"], serde_json::json!(0));
        assert_eq!(
            value["geometry_report"]["request"],
            serde_json::json!("req-9")
        );
        assert_eq!(
            value["geometry_report"]["observation"],
            serde_json::json!(3)
        );
    }

    #[test]
    fn diag_desired_truncates_fail_closed_and_zero_completeness() {
        // Fail-closed truncation: over-cap desire and report geometry bound
        // with honest omitted counts, never implying completeness. Empty
        // desire emits empty arrays with zero omission (zero-completeness
        // signal), never absent fields.
        use tiler_core::world_report::{
            OutputDesire, ScopeAuthority, TilingMode, WindowDesire, WorkspaceDesire,
        };
        let mut reply = reply_shell();
        let mut screens = Vec::new();
        for n in 0..20 {
            reply.windows.push(WindowDesire {
                lifetime: WindowId(format!("w{n}")),
                output: OutputId("o1".to_owned()),
                workspace: "ws-a".to_owned(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Controlled,
                membership_authority: ScopeAuthority::Controlled,
                floating: false,
                float_origin: None,
                frame_operation: None,
                clear_operation: None,
            });
            reply.outputs.push(OutputDesire {
                output: OutputId(format!("o{n}")),
                logical_view: Some("ws-a".to_owned()),
                tiling_mode: TilingMode::Tiled,
                hold: None,
            });
            reply.workspaces.push(WorkspaceDesire {
                id: format!("ws-{n}"),
                output: OutputId("o1".to_owned()),
                native_binding: None,
                scope: "o1".to_owned(),
                order: n,
                tiling_mode: TilingMode::Tiled,
                existence: true,
                hold: None,
            });
            screens.push(screen_shell(
                &format!("o{n}"),
                tiler_core::geometry::Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                tiler_core::geometry::Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 560,
                },
                8,
                12,
                true,
                n == 0,
            ));
        }
        let report = report_shell(screens);
        let value = apply_diag_value(
            31,
            &reply,
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report,
            &[],
            None,
        );
        assert_eq!(
            value["desired_windows"]
                .as_array()
                .expect("desired windows")
                .len(),
            MAX_APPLY_DIAG_WINDOWS
        );
        assert_eq!(
            value["desired_windows_omitted"],
            serde_json::json!(20 - MAX_APPLY_DIAG_WINDOWS)
        );
        assert_eq!(
            value["desired_outputs"]
                .as_array()
                .expect("desired outputs")
                .len(),
            MAX_APPLY_DIAG_OUTPUTS
        );
        assert_eq!(
            value["desired_outputs_omitted"],
            serde_json::json!(20 - MAX_APPLY_DIAG_OUTPUTS)
        );
        assert_eq!(
            value["desired_workspaces"]
                .as_array()
                .expect("desired workspaces")
                .len(),
            MAX_APPLY_DIAG_WORKSPACES
        );
        assert_eq!(
            value["desired_workspaces_omitted"],
            serde_json::json!(20 - MAX_APPLY_DIAG_WORKSPACES)
        );
        assert_eq!(value["geometry_status"], serde_json::json!("paired"));
        assert_eq!(
            value["desired_geometry"]
                .as_array()
                .expect("desired geometry")
                .len(),
            MAX_APPLY_DIAG_SCREENS
        );
        assert_eq!(
            value["desired_geometry_omitted"],
            serde_json::json!(20 - MAX_APPLY_DIAG_SCREENS)
        );
        // Zero desire is explicit completeness, not an absent field.
        let empty = apply_diag_value(
            32,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[],
            None,
        );
        assert_eq!(empty["desired_windows"], serde_json::json!([]));
        assert_eq!(empty["desired_windows_omitted"], serde_json::json!(0));
        assert_eq!(empty["desired_outputs"], serde_json::json!([]));
        assert_eq!(empty["desired_outputs_omitted"], serde_json::json!(0));
        assert_eq!(empty["desired_workspaces"], serde_json::json!([]));
        assert_eq!(empty["desired_workspaces_omitted"], serde_json::json!(0));
        assert_eq!(empty["geometry_status"], serde_json::json!("paired"));
        assert_eq!(empty["desired_geometry"], serde_json::json!([]));
        assert_eq!(empty["desired_geometry_omitted"], serde_json::json!(0));
    }

    #[test]
    fn diag_duplicate_delivery_keeps_provenance_truthful() {
        // Delivery freshness gate: `duplicate` repeats the cached accepted
        // reply (identical report digest, no new revision) while
        // `command-consumed` carries a freshly accepted observation reply
        // whose already-consumed command never re-executed. Both keep a
        // truthful `delivery` label, never a fresh-acceptance claim. The
        // live leg joins on request/revision/observation plus window/op
        // vocabulary either way.
        let reply = reply_shell();
        let report = report_shell(vec![]);
        let accepted = apply_diag_value(
            33,
            &reply,
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report,
            &[],
            None,
        );
        let duplicate = apply_diag_value(
            33,
            &reply,
            "duplicate",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report,
            &[],
            None,
        );
        assert_eq!(duplicate["delivery"], serde_json::json!("duplicate"));
        assert_eq!(duplicate["request"], accepted["request"]);
        assert_eq!(duplicate["revision"], accepted["revision"]);
        assert_eq!(duplicate["observation"], accepted["observation"]);
        assert_eq!(duplicate["desired_windows"], accepted["desired_windows"]);
        assert_eq!(
            duplicate["desired_transfers"],
            accepted["desired_transfers"]
        );
        assert_eq!(duplicate["geometry_status"], serde_json::json!("paired"));
        // Command-consumed: fresh observation acceptance, no command
        // execution. The reply carries `command_consumed` and the row keeps
        // the `command-consumed` delivery label with the same paired
        // geometry binding as the accepted path.
        let mut consumed_reply = reply_shell();
        consumed_reply.command_consumed = true;
        let consumed = apply_diag_value(
            34,
            &consumed_reply,
            "command-consumed",
            "send",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report,
            &[],
            None,
        );
        assert_eq!(consumed["delivery"], serde_json::json!("command-consumed"));
        assert_eq!(consumed["command_consumed"], serde_json::json!(true));
        assert_eq!(consumed["geometry_status"], serde_json::json!("paired"));
        assert_eq!(consumed["request"], accepted["request"]);
    }

    #[test]
    fn diag_mismatched_report_geometry_never_pairs() {
        // Negative pair test: a report that is not this reply's accepted
        // input (different request id) publishes empty geometry with a null
        // omission and an explicit mismatch status, never the unpaired
        // screens and never a valid zero-completeness signal.
        let reply = reply_shell();
        let mut foreign = report_shell(vec![screen_shell(
            "o9",
            tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 560,
            },
            8,
            12,
            true,
            true,
        )]);
        foreign.request_id = "req-other".to_owned();
        let value = apply_diag_value(
            35,
            &reply,
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &foreign,
            &[],
            None,
        );
        assert_eq!(value["geometry_status"], serde_json::json!("mismatch"));
        assert_eq!(value["desired_geometry"], serde_json::json!([]));
        assert!(value["desired_geometry_omitted"].is_null());
        assert_eq!(
            value["geometry_report"]["request"],
            serde_json::json!("req-other")
        );
        // The reply desire itself is untouched: mismatch fences geometry
        // only, never the accepted desired windows/outputs projection.
        assert_eq!(value["request"], serde_json::json!("req-9"));
        assert_eq!(value["desired_windows_omitted"], serde_json::json!(0));
    }

    #[test]
    fn diag_operation_settled_emits_without_attempts_and_desire_only_stays_quiet() {
        // Throttle gate: a terminal settlement event with zero raw attempts
        // still emits (the verdict is new evidence), while a desire-only row
        // (desired windows/outputs/workspaces/geometry, no attempts, no
        // settlement, no writes, unchanged hover) stays quiet. Quiet-row
        // absence is never settlement proof.
        use tiler_core::world_report::{OperationEvent, OperationOutcome, OutputDesire};
        let quiet = super::super::world_diff::ApplyStats::default();
        let empty_vecs: Vec<tiler_core::world_report::MemberAttempt> = Vec::new();
        let empty_views: Vec<tiler_core::world_report::ViewAttempt> = Vec::new();
        let empty_frames: Vec<tiler_core::world_report::FrameAttempt> = Vec::new();
        // Desire-only (no attempts, no settlement) stays quiet even with
        // desired windows, outputs, and paired geometry present.
        let mut desire_only = reply_shell();
        desire_only
            .windows
            .push(tiler_core::world_report::WindowDesire {
                lifetime: WindowId("w1".to_owned()),
                output: OutputId("o1".to_owned()),
                workspace: "ws-a".to_owned(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Controlled,
                membership_authority: ScopeAuthority::Controlled,
                floating: false,
                float_origin: None,
                frame_operation: None,
                clear_operation: None,
            });
        desire_only.outputs.push(OutputDesire {
            output: OutputId("o1".to_owned()),
            logical_view: Some("ws-a".to_owned()),
            tiling_mode: tiler_core::world_report::TilingMode::Tiled,
            hold: None,
        });
        let paired = report_shell(vec![screen_shell(
            "o1",
            tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 560,
            },
            8,
            12,
            true,
            true,
        )]);
        assert!(!should_log_apply(
            "observation",
            &quiet,
            &empty_vecs,
            &empty_views,
            &[],
            &empty_frames,
            &desire_only,
        ));
        // Settlement with zero attempts emits through the real throttle.
        let mut settled = reply_shell();
        settled.operation_settled.push(OperationEvent {
            operation_id: "op-14".to_owned(),
            window: WindowId("w2".to_owned()),
            outcome: OperationOutcome::ClearContinuationRefused,
            cause: Some("constraints".to_owned()),
        });
        assert!(should_log_apply(
            "observation",
            &quiet,
            &empty_vecs,
            &empty_views,
            &[],
            &empty_frames,
            &settled,
        ));
        let value = apply_diag_value(
            36,
            &settled,
            "accepted",
            "observation",
            &quiet,
            &empty_vecs,
            &empty_views,
            &[],
            &empty_frames,
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[],
            None,
        );
        assert_eq!(
            value["operation_settled"]
                .as_array()
                .expect("settled")
                .len(),
            1
        );
        assert_eq!(value["members"], serde_json::json!([]));
        assert_eq!(value["geometry_status"], serde_json::json!("paired"));
        let _ = paired;
    }

    #[test]
    fn red_receipt_proof_views_present_and_bounded() {
        let mut rows: Vec<(String, String)> = (0..17)
            .map(|n| (format!("o{n:02}"), format!("ws{n:02}")))
            .collect();
        rows.sort();
        let value = apply_diag_value(
            90,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &rows,
            Some(777),
        );
        let kept = value["receipt_views"].as_array().expect("receipt rows");
        assert_eq!(kept.len(), MAX_APPLY_DIAG_RECEIPT_VIEWS);
        assert_eq!(value["receipt_views_omitted"], serde_json::json!(1));
        assert_eq!(kept[0]["output"], serde_json::json!("o00"));
        assert_eq!(value["receipt_proof_mono_ms"], serde_json::json!(777));
    }

    #[test]
    fn red_focus_proof_mono_ms_present() {
        let mut stats = stats_shell();
        stats
            .focus_dispatch
            .push(super::super::world_diff::FocusDispatchDiag {
                id: "fx-1".to_owned(),
                target: "w1".to_owned(),
                kind: "focus",
                dispatched: true,
                setter: "accepted",
                foreground: true,
                hold: None,
                fg_baseline: None,
                fg_live: None,
                api: Some(super::super::world_diff::FOCUS_SETTER_API),
                call_mono_ms: Some(100),
                ret_mono_ms: Some(101),
                fg_proof_mono_ms: Some(102),
                gui_dispatch: None,
                gui_landing: None,
                primitive_called: true,
                native_return: Some(true),
                prime_inserted: None,
                hwnd: Some(11),
                pid: Some(22),
                creation: Some("c1".to_owned()),
                refusal_gate: None,
                refusal_reason: None,
                target_visible: None,
                target_cloaked: None,
                target_hidden: None,
                fg_identity: None,
                fg_gate_identity: None,
                fg_veto_kind: None,
                fg_readable: None,
                fg_shell: None,
                fg_fullscreen: None,
                fg_visible: None,
                fg_exe: None,
                desktop_exception: false,
            });
        let value = apply_diag_value(
            91,
            &reply_shell(),
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
            &report_shell(vec![]),
            &[],
            None,
        );
        let dispatch = value["focus_dispatch"].as_array().expect("dispatch");
        assert_eq!(dispatch.len(), 1);
        assert_eq!(dispatch[0]["ret_mono_ms"], serde_json::json!(101));
        assert_eq!(dispatch[0]["fg_proof_mono_ms"], serde_json::json!(102));
        assert!(
            dispatch[0]["fg_proof_mono_ms"].as_u64().unwrap()
                >= dispatch[0]["ret_mono_ms"].as_u64().unwrap()
        );
    }

    #[test]
    fn red_empty_receipt_never_echoes_prior() {
        let value = apply_diag_value(
            92,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[],
            Some(555),
        );
        assert_eq!(value["receipt_views"], serde_json::json!([]));
        assert_eq!(value["receipt_views_omitted"], serde_json::json!(0));
        assert_eq!(value["receipt_proof_mono_ms"], serde_json::json!(555));
    }

    #[test]
    fn receipt_changed_tracks_bounded_proof_status() {
        assert!(receipt_proof_changed(&[], &[], 0, None));
        let prior_missing = serde_json::json!({"event": "world-apply"});
        assert!(receipt_proof_changed(&[], &[], 0, Some(&prior_missing)));
        let rows = vec![("o1".to_owned(), "ws1".to_owned())];
        let prior = apply_diag_value(
            1,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &rows,
            Some(100),
        );
        assert!(!receipt_proof_changed(&rows, &[], 0, Some(&prior)));
        let same_restamped = apply_diag_value(
            1,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &rows,
            Some(999),
        );
        assert!(!receipt_proof_changed(&rows, &[], 0, Some(&same_restamped)));
        assert!(receipt_proof_changed(&[], &[], 0, Some(&prior)));
        assert!(receipt_proof_changed(
            &[("o1".to_owned(), "ws2".to_owned())],
            &[],
            0,
            Some(&prior)
        ));
        let empty_prior = apply_diag_value(
            2,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &[],
            Some(101),
        );
        assert!(receipt_proof_changed(&rows, &[], 0, Some(&empty_prior)));
    }

    #[test]
    fn show_boundary_reports_latest_accepted_show() {
        // The show-phase boundary derives from the drained diagnostic
        // rows (direction plus verdict), never from drain timing: posted
        // shows stamp their latest return, while hides alone, refused
        // shows, and empty batches never do. The drain folds batches with
        // a monotonic max, so an older late batch never moves the
        // boundary backward. Clock-only trace data; no native query.
        use super::super::world_diff::show_boundary_ret_max;
        use crate::product_hide::sys::VisibilityPostDiag;
        let posted_show = |ret: u64| VisibilityPostDiag {
            hwnd: 1,
            hide: false,
            api: "ShowWindowAsync",
            flags: None,
            cmd: Some(1),
            posted: true,
            call_mono_ms: 10,
            ret_mono_ms: ret,
        };
        let posted_hide = VisibilityPostDiag {
            hide: true,
            ..posted_show(11)
        };
        let refused_show = VisibilityPostDiag {
            posted: false,
            ..posted_show(12)
        };
        assert_eq!(show_boundary_ret_max(&[]), None);
        assert_eq!(show_boundary_ret_max(&[posted_hide]), None);
        assert_eq!(show_boundary_ret_max(&[refused_show]), None);
        assert_eq!(show_boundary_ret_max(&[posted_hide, refused_show]), None);
        assert_eq!(show_boundary_ret_max(&[posted_show(11)]), Some(11));
        assert_eq!(
            show_boundary_ret_max(&[posted_hide, posted_show(9), posted_show(14)]),
            Some(14)
        );
        // Multiple drains fold monotonically: 20, then an older 14 and a
        // hides-only batch hold at 20, then 25 advances to 25.
        let first = [posted_show(20)];
        let second = [posted_show(14)];
        let third = [posted_hide];
        let fourth = [posted_show(25)];
        let mut stamp: Option<u64> = None;
        for batch in [&first[..], &second[..], &third[..], &fourth[..]] {
            if let Some(latest) = show_boundary_ret_max(batch) {
                stamp = Some(stamp.map_or(latest, |prev| prev.max(latest)));
            }
        }
        assert_eq!(stamp, Some(25));
    }

    #[test]
    fn receipt_changed_tracks_homing_only_move() {
        // A homing-only proof move (views unchanged) must still log: the
        // arrival map is first-class proof, never silently quiet on a
        // views-only match. Bounded truncation stays honest in both maps.
        let views = vec![("o1".to_owned(), "ws1".to_owned())];
        let homing = vec![("w1".to_owned(), "o1".to_owned(), "ws1".to_owned())];
        let mut stats = stats_shell();
        stats.receipt_homing = homing.clone();
        stats.receipt_homing_total = homing.len();
        let prior = apply_diag_value(
            1,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &views,
            Some(100),
        );
        assert!(receipt_proof_changed(
            &views,
            &homing,
            homing.len(),
            Some(&prior)
        ));
        let with_homing = apply_diag_value(
            1,
            &reply_shell(),
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
            &report_shell(vec![]),
            &views,
            Some(101),
        );
        assert!(!receipt_proof_changed(
            &views,
            &homing,
            homing.len(),
            Some(&with_homing)
        ));
        assert!(receipt_proof_changed(&views, &[], 0, Some(&with_homing)));
        // Over-cap homing truncates with honest omission: 17 fresh entries
        // keep 16 rows and report 1 omitted.
        let big: Vec<(String, String, String)> = (0..17)
            .map(|n| (format!("w{n:02}"), "o1".to_owned(), "ws1".to_owned()))
            .collect();
        let mut big_stats = stats_shell();
        big_stats.receipt_homing_total = big.len();
        big_stats.receipt_homing =
            big[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        let big_value = apply_diag_value(
            2,
            &reply_shell(),
            "accepted",
            "observation",
            &big_stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &views,
            Some(102),
        );
        assert_eq!(
            big_value["receipt_homing"]
                .as_array()
                .expect("homing rows")
                .len(),
            super::super::world_diff::MAX_RECEIPT_HOMING_DIAG
        );
        assert_eq!(big_value["receipt_homing_omitted"], serde_json::json!(1));
    }

    #[test]
    fn stable17_homing_stays_quiet() {
        let views = vec![("o1".to_owned(), "ws1".to_owned())];
        let big: Vec<(String, String, String)> = (0..17)
            .map(|n| (format!("w{n:02}"), "o1".to_owned(), "ws1".to_owned()))
            .collect();
        let mut big_stats = stats_shell();
        big_stats.receipt_homing_total = big.len();
        big_stats.receipt_homing =
            big[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        let prior = apply_diag_value(
            2,
            &reply_shell(),
            "accepted",
            "observation",
            &big_stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &views,
            Some(102),
        );
        let current_prefix = big[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        assert!(
            !receipt_proof_changed(&views, &current_prefix, 17, Some(&prior)),
            "stable 17-row homing must stay quiet"
        );
    }

    #[test]
    fn homing_count_change_logs() {
        let views = vec![("o1".to_owned(), "ws1".to_owned())];
        let big: Vec<(String, String, String)> = (0..17)
            .map(|n| (format!("w{n:02}"), "o1".to_owned(), "ws1".to_owned()))
            .collect();
        let mut big_stats = stats_shell();
        big_stats.receipt_homing_total = big.len();
        big_stats.receipt_homing =
            big[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        let prior = apply_diag_value(
            2,
            &reply_shell(),
            "accepted",
            "observation",
            &big_stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &views,
            Some(102),
        );
        let current_prefix = big[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        assert!(
            receipt_proof_changed(&views, &current_prefix, 18, Some(&prior)),
            "same prefix with changed total must log"
        );
    }

    #[test]
    fn homing_prefix_change_logs() {
        let views = vec![("o1".to_owned(), "ws1".to_owned())];
        let big: Vec<(String, String, String)> = (0..17)
            .map(|n| (format!("w{n:02}"), "o1".to_owned(), "ws1".to_owned()))
            .collect();
        let mut big_stats = stats_shell();
        big_stats.receipt_homing_total = big.len();
        big_stats.receipt_homing =
            big[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        let prior = apply_diag_value(
            2,
            &reply_shell(),
            "accepted",
            "observation",
            &big_stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &views,
            Some(102),
        );
        let mut changed_prefix = big[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        changed_prefix[0] = ("w99".to_owned(), "o1".to_owned(), "ws1".to_owned());
        assert!(
            receipt_proof_changed(&views, &changed_prefix, 17, Some(&prior)),
            "changed prefix with same total must log"
        );
    }

    #[test]
    fn tail_only_homing_stays_quiet() {
        // Tail-only change past the cap keeps the same prefix and total,
        // so it stays quiet: the omitted tail is unavailable, never
        // whole-map proof. No tail census, hash, or history is kept.
        let views = vec![("o1".to_owned(), "ws1".to_owned())];
        let big: Vec<(String, String, String)> = (0..17)
            .map(|n| (format!("w{n:02}"), "o1".to_owned(), "ws1".to_owned()))
            .collect();
        let mut big_stats = stats_shell();
        big_stats.receipt_homing_total = big.len();
        big_stats.receipt_homing =
            big[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        let prior = apply_diag_value(
            3,
            &reply_shell(),
            "accepted",
            "observation",
            &big_stats,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &views,
            Some(103),
        );
        let tail_changed = big[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        let mut full_tail_changed = big.clone();
        full_tail_changed[16] = ("w99".to_owned(), "o1".to_owned(), "ws1".to_owned());
        let current_prefix =
            full_tail_changed[..super::super::world_diff::MAX_RECEIPT_HOMING_DIAG].to_vec();
        assert_eq!(current_prefix, tail_changed);
        assert!(
            !receipt_proof_changed(&views, &current_prefix, 17, Some(&prior)),
            "tail-only change past the cap must stay omitted-quiet"
        );
    }

    #[test]
    fn receipt_only_row_emits_once_then_stays_quiet_with_aba() {
        let quiet = super::super::world_diff::ApplyStats::default();
        let reply = reply_shell();
        let report = report_shell(vec![]);
        let route = || {
            fresh_route_value(
                "",
                None,
                "focused-output-last-focused",
                "cursor-output",
                (None, None, None, "retained"),
            )
        };
        let dir = std::env::temp_dir().join(format!(
            "tiler-receipt-aba-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let log = dir.join("apply.log");
        let count_rows = || {
            std::fs::read_to_string(&log)
                .map(|text| text.lines().filter(|line| !line.is_empty()).count())
                .unwrap_or(0)
        };
        let empty: Vec<(String, String)> = Vec::new();
        let proven = vec![("o1".to_owned(), "ws2".to_owned())];
        let first = apply_diag_value(
            10,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &[],
            route(),
            None,
            false,
            &report,
            &empty,
            Some(10),
        );
        assert!(receipt_proof_changed(&empty, &[], 0, None));
        assert!(log_apply_diag(
            &log,
            true,
            10,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &[],
            route(),
            None,
            false,
            &report,
            &empty,
            Some(10),
            false,
            true,
            0,
        ));
        assert_eq!(count_rows(), 1);
        assert!(!receipt_proof_changed(&empty, &[], 0, Some(&first)));
        assert!(!log_apply_diag(
            &log,
            true,
            11,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &[],
            route(),
            None,
            false,
            &report,
            &empty,
            Some(11),
            false,
            false,
            0,
        ));
        assert_eq!(count_rows(), 1);
        assert!(receipt_proof_changed(&proven, &[], 0, Some(&first)));
        assert!(log_apply_diag(
            &log,
            true,
            12,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &[],
            route(),
            None,
            false,
            &report,
            &proven,
            Some(12),
            false,
            true,
            0,
        ));
        assert_eq!(count_rows(), 2);
        let second = apply_diag_value(
            12,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &[],
            route(),
            None,
            false,
            &report,
            &proven,
            Some(12),
        );
        assert!(!receipt_proof_changed(&proven, &[], 0, Some(&second)));
        assert!(!log_apply_diag(
            &log,
            true,
            13,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &[],
            route(),
            None,
            false,
            &report,
            &proven,
            Some(13),
            false,
            false,
            0,
        ));
        assert_eq!(count_rows(), 2);
        assert!(receipt_proof_changed(&empty, &[], 0, Some(&second)));
        assert!(log_apply_diag(
            &log,
            true,
            14,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &[],
            route(),
            None,
            false,
            &report,
            &empty,
            Some(14),
            false,
            true,
            0,
        ));
        assert_eq!(count_rows(), 3);
        assert!(!log.join("extra").exists());
    }

    #[test]
    fn trace_off_builds_no_receipt_payload_or_file_even_on_change() {
        let quiet = super::super::world_diff::ApplyStats::default();
        let reply = reply_shell();
        let report = report_shell(vec![]);
        let dir = std::env::temp_dir().join(format!(
            "tiler-receipt-dark-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let log = dir.join("dark.log");
        assert!(!log_apply_diag(
            &log,
            false,
            20,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &[],
            fresh_route_value(
                "",
                None,
                "focused-output-last-focused",
                "cursor-output",
                (None, None, None, "retained"),
            ),
            None,
            false,
            &report,
            &[("o1".to_owned(), "ws2".to_owned())],
            Some(20),
            false,
            true,
            0,
        ));
        assert!(!log.exists());
        let value = apply_diag_value(
            21,
            &reply,
            "accepted",
            "observation",
            &quiet,
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report,
            &[],
            None,
        );
        assert_eq!(value["receipt_views"], serde_json::json!([]));
        assert!(value["receipt_proof_mono_ms"].is_null());
    }

    #[test]
    fn paired_copy_retains_exact_proof_timestamp_and_rows() {
        let rows = vec![("o1".to_owned(), "ws2".to_owned())];
        let proof = apply_diag_value(
            30,
            &reply_shell(),
            "accepted",
            "observation",
            &stats_shell(),
            &[],
            &[],
            &[],
            &[],
            serde_json::json!({}),
            None,
            false,
            &report_shell(vec![]),
            &rows,
            Some(4242),
        );
        assert_eq!(proof["receipt_proof_mono_ms"], serde_json::json!(4242));
        let mut paired = proof.clone();
        paired["paired"] = serde_json::json!(true);
        assert_eq!(paired["receipt_proof_mono_ms"], serde_json::json!(4242));
        assert_eq!(paired["receipt_views"], proof["receipt_views"]);
    }
}
