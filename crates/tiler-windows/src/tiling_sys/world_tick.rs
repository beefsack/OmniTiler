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
use super::world_sequencer::DispatchError;
use super::{MonitorArea, ObservedWindow, RetainedRow, TileLoop};
use std::collections::HashMap;
use tiler_core::directional::WindowId;
use tiler_core::world_report::{AttemptOutcome, WorldCommand};

/// Merge applied transfer effects into the proven receipt maps. Each
/// `Applied` feedback revalidates against independent fresh native proof
/// (never the native cache alone): a window transfer merges only when
/// the live homing still names the target under full identity/tag
/// checks (plus, for tiled rows, the exact live frame on the target),
/// and a view or migration merges only when the live view still names
/// the target. Possibly-executed, refused, and superseded outcomes never
/// merge. Empty views merge only on explicit finished batch proof
/// (the live view names the target with zero stray visibility), never
/// on a desired id echo when no work ran.
pub(super) fn merge_applied_transfers(
    reply: &tiler_core::world_report::WorldReply,
    feedback: &[tiler_core::world_report::AttemptFeedback],
    native: &mut LiveNative,
    proven_h: &mut HashMap<String, (String, String)>,
    proven_v: &mut HashMap<String, String>,
) {
    use tiler_core::world_report::{AttemptKind, TransferKind};
    for transfer in &reply.transfers {
        let proven = feedback.iter().any(|item| {
            item.operation_id == transfer.operation_id
                && matches!(item.kind, AttemptKind::Transfer)
                && item.outcome == AttemptOutcome::Applied
        });
        if !proven {
            continue;
        }
        // Independent fresh proof at merge time: the `Applied` outcome
        // alone never merges. A recycled HWND or a raced hide between
        // the diff pass and this snapshot must not plant a receipt.
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
                if native
                    .read_view(&transfer.target_output.0)
                    .as_deref()
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
    let Some(row) = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == token)
    else {
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

/// Mechanical admission preamble: bind observed windows to stable tokens
/// plus stored identity and stamped lifetime tags. Admission verifies the
/// holdable key, live identity, scope fence, and hosted-child gate, then
/// stamps the visible-membership tag; it never assigns workspaces, never
/// touches rings/views, and never commits policy. Prunes only
/// verified-closed lifetimes (dropped from the member maps once the close
/// receipt pruned the native queue) and recycled-HWND stale entries.
fn admit_observed(state: &mut TileLoop, me: &crate::model::ProcessIdentity, observed: &[ObservedWindow]) {
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
                .is_some_and(|entry| super::observe::verify_proof_owned(key.hwnd, entry, me).is_ok());
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
            && crate::product_hide::sys::read_member_tag(key.hwnd).as_deref() == Some(fresh_tag.as_str());
        if !post_ok {
            continue;
        }
        state
            .member_tokens
            .insert(key.clone(), window.token.clone());
        state.member_identity.insert(key.clone(), live);
        state.member_tags.insert(key, fresh_tag);
    }
}

/// Refresh mechanical snapshots: last-known frames per token for overlay
/// slots and hidden snapshots. Tiled members refresh their tile slot;
/// floating lanes (sticky, automatic fixed, core floats, born holds) track
/// the live frame in `float_rects` and never plant a tile slot (automatic
/// floats additionally drop any first-contact seed).
fn snapshot_frames(state: &mut TileLoop, observed: &[ObservedWindow]) {
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
) -> Option<tiler_core::world_report::WorldReply> {
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
        scope_hosts,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
    );
    let report = match world.try_dispatch_observation(
        &inputs,
        Some(observed),
        retained,
        areas,
        foreground_hwnd,
    ) {
        Ok(report) => report,
        Err(DispatchError::Busy) => return world.last_reply().cloned(),
        Err(DispatchError::CatchupRequired) => return world.last_reply().cloned(),
        Err(DispatchError::CountersExhausted) => return None,
    };
    let outcome = world.deliver_production(&report);
    let reply = match outcome {
        tiler_core::world_engine::WorldOutcome::Accepted(reply)
        | tiler_core::world_engine::WorldOutcome::Duplicate(reply)
        | tiler_core::world_engine::WorldOutcome::CommandConsumed(reply) => reply,
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
        scope_hosts,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
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
        scope_hosts,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
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
    let (_stats, feedback) = world.apply_accepted(&reply, &fresh, &mut native);
    // Proven receipt snapshot: desired values only where live native
    // evidence proves arrival (identity/tag/claim/visibility/scope
    // gates, exact frame on the target for tiled rows). Changed homing
    // additionally merges from applied transfer feedback with
    // independent fresh revalidation. Never wholesale desire.
    let (mut proven_h, mut proven_v) = native.receipt_updates(&reply);
    merge_applied_transfers(&reply, &feedback, &mut native, &mut proven_h, &mut proven_v);
    drop(native);
    for (token, home) in proven_h {
        applied_homing.insert(token, home);
    }
    for (output, view) in proven_v {
        applied_views.insert(output, view);
    }
    for item in &feedback {
        // Only core-consumed operation feedback rides the next report;
        // creation/removal/effect outcomes prove by read-back.
        if matches!(
            item.outcome,
            AttemptOutcome::PossiblyExecuted
                | AttemptOutcome::RefusedProven
                | AttemptOutcome::SupersededExternal
                | AttemptOutcome::Applied
        ) {
            world.note_feedback(item.clone());
        }
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
) -> Option<tiler_core::world_report::WorldReply> {
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
            scope_hosts,
            settings_live,
            output_tokens,
            applied_homing,
            applied_views,
            inner_gap,
            outer_gap,
            active,
        );
        match world.try_dispatch_command(
            &inputs,
            Some(observed),
            retained,
            areas,
            foreground_hwnd,
            command,
        ) {
            Ok(report) => report,
            Err(DispatchError::CatchupRequired) => {
                // Observation-only catch-up is pending: drop the command
                // (never queued, never replayed) and let the next
                // observation tick dispatch the catch-up first; the caller
                // resubmits after.
                return world.last_reply().cloned();
            }
            Err(_) => return world.last_reply().cloned(),
        }
    };
    let outcome = world.deliver_production(&report);
    let reply = match outcome {
        tiler_core::world_engine::WorldOutcome::Accepted(reply)
        | tiler_core::world_engine::WorldOutcome::Duplicate(reply)
        | tiler_core::world_engine::WorldOutcome::CommandConsumed(reply) => reply,
        tiler_core::world_engine::WorldOutcome::Rejected { .. } => {
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
        scope_hosts,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
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
        scope_hosts,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
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
    let (_stats, feedback) = world.apply_accepted(&reply, &fresh, &mut native);
    // Proven receipt snapshot plus applied transfer effects (see the
    // observation path): re-proof of unchanged homing plus exact proven
    // arrivals with independent fresh revalidation. Never wholesale desire.
    let (mut proven_h, mut proven_v) = native.receipt_updates(&reply);
    merge_applied_transfers(&reply, &feedback, &mut native, &mut proven_h, &mut proven_v);
    drop(native);
    for (token, home) in proven_h {
        applied_homing.insert(token, home);
    }
    for (output, view) in proven_v {
        applied_views.insert(output, view);
    }
    for item in &feedback {
        if matches!(
            item.outcome,
            AttemptOutcome::PossiblyExecuted
                | AttemptOutcome::RefusedProven
                | AttemptOutcome::SupersededExternal
                | AttemptOutcome::Applied
        ) {
            world.note_feedback(item.clone());
        }
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
    let observed = state.observe(me, fulls, &mut skipped, &mut retained)?;
    run_world_command(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        command,
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
        ref scope_hosts,
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
        scope_hosts,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
    );
    let report = match world.try_dispatch_observation(
        &inputs,
        Some(observed),
        retained,
        areas,
        foreground_hwnd,
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

