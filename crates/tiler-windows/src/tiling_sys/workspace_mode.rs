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
    state.workspaces.is_tiled(output, workspace)
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

/// Reset fixed-size admission lanes for one released domain (R-SPC-04 D6):
/// the D4 automatic latch never survives a domain release, and last-known
/// diagnostics hints for the domain's members are dropped so retained rows
/// re-query fresh instead of riding a stale pre-release bound. The
/// Timeout-reuse last-good cache survives the release: its entries are bound
/// to the live member lifetime (not the Engine domain), so the next admission
/// still classifies every candidate fresh from current hints (changed
/// ordinary hints and formerly latched maxima included) before any
/// maximize-clear, while a timing-out member keeps its declared hint.
/// Explicit tile overrides, intentional floats, and sticky members are
/// preserved: their lanes re-assert on the next observation
/// (override set/marker, runtime intent, sticky map are untouched here).
pub(super) fn reset_fixed_admission_for_domain(
    state: &mut TileLoop,
    output: &str,
    workspace: &str,
) {
    let members = state.workspaces.workspace_members(output, workspace);
    for key in &members {
        state.fixed_auto_seen.remove(key);
        if let Some(token) = state.member_tokens.get(key).cloned() {
            state.hint_logged.remove(&token);
            state.hint_fallback_uses.remove(&token);
            drop_diag_sample(state, &token);
        }
    }
}

/// Dispatch one exact shared-Engine domain release for a workspace-mode
/// toggle: the current observed rows ride the event with `ReleaseDomain`
/// (no geometry), so nothing is written and no stale layout is retained.
/// Returns true exactly when the Engine confirms the release.
pub(super) fn dispatch_workspace_release(
    state: &mut TileLoop,
    output: &str,
    workspace: &str,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    correlation: &str,
) -> bool {
    let Some((domain, domain_key)) =
        workspace_domain_for(output, workspace, areas, state.inner_gap, state.outer_gap)
    else {
        return false;
    };
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        output,
        workspace,
        observed,
        retained,
        "release",
        correlation,
        &mut hint_cx,
    ) else {
        return false;
    };
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let Some(correlation_id) = CorrelationId::parse(correlation) else {
        return false;
    };
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation_id,
        revision_for(state, output, workspace),
        fp,
        &domain,
        &domain_key,
        state.outer_gap,
        &windows,
        None,
    );
    event.command = CoreCommand::ReleaseDomain;
    match handle_engine_event(state, &mut event) {
        CoreReply::Released => {
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-released",
                    "output": state.workspaces.output_token(output),
                    "workspace": state.workspaces.workspace_token(output, workspace),
                    "outcome": "released",
                    "members": rows.len(),
                }),
            );
            // R-SPC-04 D6: a confirmed release resets the fixed-size
            // admission lanes for this domain's candidates, so the next
            // admission classifies every candidate fresh from current
            // hints (changed ordinary hints and formerly latched maxima
            // included). Explicit tile overrides, intentional floats, and
            // sticky members are preserved: their lanes re-assert on the
            // next observation. Dropping last-known hints forces the
            // retained rows to re-query fresh instead of riding a stale
            // pre-release bound, so a fresh max candidate classifies
            // before any clear.
            reset_fixed_admission_for_domain(state, output, workspace);
            // D23: a confirmed release discards the domain's Engine slots,
            // so queued closes homed here must drop too: there is no slot
            // left to forget, and re-sending them would report ids the
            // Engine no longer knows. Other domains' queues are untouched.
            state
                .pending_closed
                .retain(|_, homing| homing.output != output || homing.workspace != workspace);
            true
        }
        reply => {
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-released",
                    "output": state.workspaces.output_token(output),
                    "workspace": state.workspaces.workspace_token(output, workspace),
                    "outcome": reply_outcome(&reply),
                    "recovery": "retry-on-event",
                }),
            );
            false
        }
    }
}

/// Cancel stale drag and queued-action effects for one workspace after a
/// mode toggle: in-flight gesture, preview, and Win+Left bindings anchored
/// to its members can never settle into geometry writes afterwards. Queued
/// keyboard intents re-resolve the live mode at dispatch and refuse there.
fn cancel_workspace_effects(state: &mut TileLoop, output: &str, workspace: &str) {
    let members = state.workspaces.workspace_members(output, workspace);
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
    let before = state.windrag_start_cursor.len();
    state
        .windrag_start_cursor
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.windrag_start_cursor.len());
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
    let before = state.windrag_bound.len();
    state.windrag_bound.retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.windrag_bound.len());
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
    let before = state.gesture_producer.len();
    state
        .gesture_producer
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_producer.len());
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
            "output": state.workspaces.output_token(output),
            "workspace": state.workspaces.workspace_token(output, workspace),
            "cancelled": cancelled,
        }),
    );
}

/// Toggle the tiling mode of the owner's current workspace (tray edge).
/// Floating applies immediately with frames untouched, then the exact shared
/// Engine domain releases with no writes. Retile releases first and stays
/// floating until the release confirms, then marks tiled with a fresh
/// complete reconcile on current observed geometry. Release failures queue
/// for retry on later topology edges; per-window float/sticky and native
/// maximize/fullscreen exceptions are never touched.
pub(super) fn toggle_workspace_tiling(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    output: &str,
) {
    state.workspaces.ensure_output(output);
    let Some(current) = state.workspaces.active_id(output) else {
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let correlation = format!("ws-mode-{tick}");
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
    let current_clone = current.clone();
    if state.workspaces.is_tiled(output, &current_clone) {
        state.workspaces.set_tiled(output, &current_clone, false);
        cancel_workspace_effects(state, output, &current_clone);
        let confirmed = dispatch_workspace_release(
            state,
            output,
            &current_clone,
            areas,
            &observed,
            &retained,
            &correlation,
        );
        if !confirmed {
            state.pending_releases.queue(output, &current_clone, false);
        } else {
            // A confirming toggle supersedes any earlier intent for the
            // domain (e.g. a failed float queued before a successful retile):
            // without this the stale entry would later fire at live layout.
            state.pending_releases.confirm(output, &current_clone);
        }
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "workspace-mode",
                "op": "toggle",
                "output": state.workspaces.output_token(output),
                "workspace": state.workspaces.workspace_token(output, &current),
                "tiled": false,
                "outcome": "applied",
            }),
        );
    } else {
        let confirmed = dispatch_workspace_release(
            state,
            output,
            &current_clone,
            areas,
            &observed,
            &retained,
            &correlation,
        );
        if confirmed {
            // The successful retile supersedes every earlier intent for the
            // domain: drop the entry so no stale float can fire later.
            state.pending_releases.confirm(output, &current_clone);
            state.workspaces.set_tiled(output, &current_clone, true);
            cancel_workspace_effects(state, output, &current_clone);
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-mode",
                    "op": "toggle",
                    "output": state.workspaces.output_token(output),
                    "workspace": state.workspaces.workspace_token(output, &current),
                    "tiled": true,
                    "outcome": "applied",
                }),
            );
            reconcile_tick(state, me, fulls, areas);
            refresh_active_border(state, me, fulls);
        } else {
            state.pending_releases.queue(output, &current_clone, true);
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-mode",
                    "op": "toggle",
                    "output": state.workspaces.output_token(output),
                    "workspace": state.workspaces.workspace_token(output, &current),
                    "tiled": false,
                    "outcome": "pending-retile",
                }),
            );
        }
    }
}

/// Retry unconfirmed workspace-domain releases on a topology edge with the
/// maintenance observation (no second enumeration). Confirmed retiles mark
/// tiled with a fresh complete reconcile; confirmed float releases stay
/// floating. Entries whose workspace reads tiled again drop stale without
/// dispatch: releasing would destroy the live tiled layout. Failures stay
/// queued and the fingerprint gate retries them on the next topology edge
/// only, so quiet ticks never poll or log.
pub(super) fn retry_pending_releases(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) {
    if state.pending_releases.is_empty() {
        return;
    }
    let fingerprint_now = pending_topology_fingerprint(state, areas);
    if fingerprint_now == state.pending_retry_fp {
        return;
    }
    state.pending_retry_fp = fingerprint_now;
    let mut retiled = false;
    for (output, workspace, retile) in state.pending_releases.snapshot() {
        let current = workspace_mode_known(state, &output, &workspace);
        if crate::workspace::PendingReleases::is_stale(current) {
            state.pending_releases.confirm(&output, &workspace);
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-mode",
                    "op": "retry",
                    "output": state.workspaces.output_token(&output),
                    "workspace": state.workspaces.workspace_token(&output, &workspace),
                    "outcome": "stale-dropped",
                }),
            );
            continue;
        }
        state.tick += 1;
        let tick = state.tick;
        let correlation = format!("ws-retry-{tick}");
        if dispatch_workspace_release(
            state,
            &output,
            &workspace,
            areas,
            observed,
            retained,
            &correlation,
        ) {
            state.pending_releases.confirm(&output, &workspace);
            if retile {
                state.workspaces.set_tiled(&output, &workspace, true);
                cancel_workspace_effects(state, &output, &workspace);
                retiled = true;
            }
        }
    }
    if retiled {
        reconcile_tick(state, me, fulls, areas);
        refresh_active_border(state, me, fulls);
    }
}

/// Session tiled state for one workspace, or `None` when the workspace is
/// unknown on the output. Stale-intent detection must distinguish unknown
/// (still releasable with empty rows) from tiled (never releasable).
pub(super) fn workspace_mode_known(
    state: &TileLoop,
    output: &str,
    workspace: &str,
) -> Option<bool> {
    state
        .workspaces
        .workspace_ids(output)
        .iter()
        .any(|id| id == workspace)
        .then(|| state.workspaces.is_tiled(output, workspace))
}

/// Topology fingerprint behind the pending-release retry gate: monitor
/// devices plus work/full rects, workspace order per output, member
/// identities, and the raw HWND inventory size.
fn pending_topology_fingerprint(state: &TileLoop, areas: &[MonitorArea]) -> u64 {
    fn pack(rect: &Rect) -> (i32, i32, i32, i32) {
        (rect.x, rect.y, rect.w, rect.h)
    }
    let area_tuples: Vec<crate::workspace::TopologyArea> = areas
        .iter()
        .map(|area| (area.device.clone(), pack(&area.work), pack(&area.full)))
        .collect();
    let workspace_tuples: Vec<(String, Vec<String>)> = state
        .workspaces
        .output_keys()
        .into_iter()
        .map(|output| {
            let ids = state.workspaces.workspace_ids(&output);
            (output, ids)
        })
        .collect();
    let members: Vec<crate::workspace::WindowKey> = state.member_tokens.keys().cloned().collect();
    crate::workspace::topology_fingerprint(
        &area_tuples,
        &workspace_tuples,
        &members,
        state.last_hwnds.len(),
    )
}
