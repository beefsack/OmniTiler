#![forbid(unsafe_code)]
use super::*;

/// Session workspace-mode gate: tiled workspaces run every Engine/geometry
/// path; floating workspaces leave frames untouched and stop all tiled
/// geometry, group-underlay, and drop-preview operations for that domain
/// while the independent active border still marks focus. Intentional
/// per-window float/sticky and native maximize/fullscreen overlays keep
/// their own semantics on either mode.
#[must_use]
pub(super) fn workspace_mode_tiled(state: &TileLoop, output: &str, workspace: &str) -> bool {
    state.is_tiled_view(output, workspace)
}

/// Fresh `WS_EX_TOPMOST` read on one HWND (official Win32 topmost band).
pub(super) fn read_topmost_now(hwnd_u64: u64) -> bool {
    crate::ffi::window_exstyle(hwnd_u64) & WS_EX_TOPMOST != 0
}

/// Set one window's topmost band without moving, sizing, or activating it.
/// Returns whether the setter accepted; the caller re-reads for proof.
pub(super) fn set_topmost_band(hwnd_u64: u64, top: bool) -> bool {
    if !crate::ffi::is_window(hwnd_u64) {
        return false;
    }
    crate::ffi::set_topmost(hwnd_u64, top)
}

/// Cancel stale drag and queued-action effects for one workspace after a
/// mode toggle: in-flight gesture and preview bindings anchored to its
/// members can never settle into geometry writes afterwards. Queued
/// keyboard intents re-resolve the live mode at dispatch and refuse there.
fn cancel_workspace_effects(state: &mut TileLoop, output: &str, workspace: &str) {
    let members = state.workspace_members(output, workspace);
    let hwnds: HashSet<u64> = members.iter().map(|k| k.hwnd).collect();
    let mut cancelled = 0u32;
    let mut drop_entries = |before: usize, after: usize| {
        cancelled += (before - after) as u32;
    };
    let before = state.gesture_before.len();
    state.gesture_before.retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_before.len());
    let before = state.gesture_end_cursor.len();
    state
        .gesture_end_cursor
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_end_cursor.len());
    let before = state.gesture_start_key.len();
    state
        .gesture_start_key
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_start_key.len());
    let before = state.gesture_start_tag.len();
    state
        .gesture_start_tag
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_start_tag.len());
    let before = state.gesture_preview_start.len();
    state
        .gesture_preview_start
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_preview_start.len());
    let before = state.gesture_esc_seq.len();
    state
        .gesture_esc_seq
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_esc_seq.len());
    let before = state.gesture_end_seq.len();
    state
        .gesture_end_seq
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_end_seq.len());
    let before = state.move_kind.len();
    state.move_kind.retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.move_kind.len());
    let before = state.gesture_start_cursor.len();
    state
        .gesture_start_cursor
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_start_cursor.len());
    let before = state.preview_bound.len();
    state.preview_bound.retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.preview_bound.len());
    let before = state.active.len();
    state.active.retain(|hwnd| !hwnds.contains(hwnd));
    drop_entries(before, state.active.len());
    let before = state.preview_dead.len();
    state.preview_dead.retain(|hwnd| !hwnds.contains(hwnd));
    drop_entries(before, state.preview_dead.len());
    let before = state.esc_latched.len();
    state.esc_latched.retain(|hwnd| !hwnds.contains(hwnd));
    drop_entries(before, state.esc_latched.len());
    if let Some(advance) = state.snap_advance.clone()
        && hwnds.contains(&advance.hwnd)
    {
        state.snap_advance = None;
        cancelled += 1;
    }
    hide_preview(state, "workspace-mode");
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "workspace-mode",
            "op": "effects-cancelled",
            "output": state.output_token(output),
            "workspace": state.workspace_token(output, workspace),
            "cancelled": cancelled,
        }),
    );
}

/// Toggle the tiling mode of the owner's current workspace (tray edge).
/// The planner-owned mode flips through one
/// [`WorldCommand::SetWorkspaceTiling`](tiler_core::world_report::WorldCommand::SetWorkspaceTiling)
/// dispatch: disabling releases the retained session with no frame writes,
/// re-enabling re-adopts through the fresh route. Per-window float/sticky
/// and native maximize/fullscreen overlays are never touched. Stale gesture
/// effects for the workspace cancel; the toggle reports the honest world
/// outcome (never queued, never retried: a lost command resubmits on the
/// next pick).
pub(super) fn toggle_workspace_tiling(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    output: &str,
) {
    state.ensure_device(output);
    let Some(current) = state.active_view(output) else {
        return;
    };
    let tiled_now = state.is_tiled_view(output, &current);
    let Some(wire) = state.wire_of(output) else {
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "workspace-mode",
                "op": "toggle",
                "outcome": "observation-failed",
            }),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        tiler_core::world_report::WorldCommand::SetWorkspaceTiling {
            output: tiler_core::directional::OutputId(wire),
            workspace: current.clone(),
            tiled: !tiled_now,
        },
        super::observation::sample_cursor(),
    );
    let outcome = match reply {
        Some(_) => "applied",
        None => "unverified",
    };
    if reply.is_some() {
        cancel_workspace_effects(state, output, &current);
    }
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "workspace-mode",
            "op": "toggle",
            "output": state.output_token(output),
            "workspace": state.workspace_token(output, &current),
            "tiled": !tiled_now,
            "outcome": outcome,
        }),
    );
    if reply.is_some() && !tiled_now {
        // Retile converges on current observed geometry through the next
        // observation tick below.
        super::reconcile::reconcile_tick(state, me, store, dir, fulls, areas);
        refresh_active_border(state, me, fulls);
    }
    let _ = tick;
}

/// Retry unconfirmed workspace-domain releases: deleted. Mode toggles
/// dispatch synchronously through the world owner and report the honest
/// outcome inline; nothing queues, so no topology-edge retry exists.
/// unknown on the output. Stale-intent detection must distinguish unknown
/// (still releasable with empty rows) from tiled (never releasable).
pub(super) fn workspace_mode_known(
    state: &TileLoop,
    output: &str,
    workspace: &str,
) -> Option<bool> {
    state.workspace_mode_known_here(output, workspace)
}
