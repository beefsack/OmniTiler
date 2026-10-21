#![forbid(unsafe_code)]
use super::keyboard_directional::dispatch_directional_intent;
use super::keyboard_overlays::{dispatch_fullscreen_intent, dispatch_maximize_intent};
use super::*;

pub(super) fn keyboard_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedSnapEvent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    let dropped = owner_snap_dropped(
        state
            .input_bridge
            .as_ref()
            .map(|bridge| bridge.channels.diag.snapshot().snap_dropped),
    );
    if dropped > state.snap_dropped {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"snap-drop","lost": dropped - state.snap_dropped}),
        );
        state.snap_dropped = dropped;
    }
    if let Some(cause) = blocked {
        // Suspension and gestures invalidate any own-focus chain: the next
        // batch starts without continuation.
        state.snap_advance = None;
        let mut stale = 0u32;
        for ev in events {
            match ev {
                QueuedSnapEvent::Mask(mask) => {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap-mask",
                            "mask": crate::snapkey::sys::mask_evidence(&mask),
                        }),
                    );
                }
                QueuedSnapEvent::Intent(_) => stale += 1,
                QueuedSnapEvent::Resize(_) => stale += 1,
                QueuedSnapEvent::Workspace(_) => stale += 1,
                QueuedSnapEvent::WorkspaceSend(_) => stale += 1,
                QueuedSnapEvent::WorkspaceOutput(_) => stale += 1,
                QueuedSnapEvent::WorkspaceMigrate(_) => stale += 1,
                QueuedSnapEvent::WorkspaceHistory(_) => stale += 1,
                QueuedSnapEvent::Maximize(_) => stale += 1,
                QueuedSnapEvent::Orientation(_) => stale += 1,
                QueuedSnapEvent::WorkspaceTiling(_) => stale += 1,
                QueuedSnapEvent::Fullscreen(_) => stale += 1,
                QueuedSnapEvent::Float(_) => stale += 1,
                QueuedSnapEvent::Sticky(_) => stale += 1,
                QueuedSnapEvent::Launch(_) => stale += 1,
            }
        }
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"snap-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    // Owner-caused focus advances verified across batches live on the loop:
    // the only retarget a chord origin may take, so rapid repeated
    // navigation survives bounded drains while external focus moves reject.
    // Cleared on external focus, lifetime mismatch, suspension, or gesture.
    for ev in events {
        match ev {
            QueuedSnapEvent::Mask(mask) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "snap-mask",
                        "mask": crate::snapkey::sys::mask_evidence(&mask),
                    }),
                );
            }
            QueuedSnapEvent::Intent(intent) => {
                dispatch_directional_intent(state, me, store, dir, fulls, areas, intent, &log_path);
            }
            QueuedSnapEvent::Maximize(intent) => {
                dispatch_maximize_intent(state, me, store, dir, fulls, areas, intent, &log_path);
            }
            QueuedSnapEvent::Fullscreen(intent) => {
                dispatch_fullscreen_intent(state, me, store, dir, fulls, areas, intent, &log_path);
            }
            QueuedSnapEvent::Workspace(intent) => {
                // Routed to workspace_tick by the caller; defensive drop here
                // stays trace-only so held-key traffic never pollutes logs.
                if state.trace {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "workspace",
                            "tick": state.tick,
                            "op": intent.op.as_str(),
                            "index": intent.index,
                            "edge": intent.edge.as_str(),
                            "disposition": "rerouted",
                            "outcome": "workspace-tick",
                        }),
                    );
                }
            }
            QueuedSnapEvent::WorkspaceHistory(intent) => {
                // Routed to workspace_history_tick by the caller; defensive
                // drop here stays trace-only like digits.
                if state.trace {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "workspace-history",
                            "tick": state.tick,
                            "op": intent.op.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "rerouted",
                            "outcome": "workspace-history-tick",
                        }),
                    );
                }
            }
            QueuedSnapEvent::WorkspaceSend(intent) => {
                // Routed to workspace_relative_send_tick by the caller;
                // defensive drop here stays trace-only like digits.
                if state.trace {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "workspace-send",
                            "tick": state.tick,
                            "delta": intent.delta,
                            "follow": intent.follow,
                            "edge": intent.edge.as_str(),
                            "disposition": "rerouted",
                            "outcome": "workspace-send-tick",
                        }),
                    );
                }
            }
            QueuedSnapEvent::WorkspaceOutput(intent) => {
                // Routed to workspace_output_send_tick by the caller;
                // defensive drop here stays trace-only like digits.
                if state.trace {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "workspace-output",
                            "tick": state.tick,
                            "direction": direction_name(intent.direction),
                            "follow": intent.follow,
                            "edge": intent.edge.as_str(),
                            "disposition": "rerouted",
                            "outcome": "workspace-output-tick",
                        }),
                    );
                }
            }
            QueuedSnapEvent::WorkspaceMigrate(intent) => {
                // Routed to workspace_migrate_tick by the caller;
                // defensive drop here stays trace-only like digits.
                if state.trace {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "workspace-migrate",
                            "tick": state.tick,
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "rerouted",
                            "outcome": "workspace-migrate-tick",
                        }),
                    );
                }
            }
            QueuedSnapEvent::Float(intent) => {
                dispatch_float_intent(state, me, store, dir, fulls, areas, intent);
            }
            QueuedSnapEvent::Resize(intent) => {
                dispatch_resize_intent(state, me, store, dir, fulls, areas, intent);
            }
            QueuedSnapEvent::Orientation(intent) => {
                dispatch_orientation_intent(state, me, store, dir, fulls, areas, intent);
            }
            QueuedSnapEvent::WorkspaceTiling(intent) => {
                // The suspend/elevated fence reads live foreground state, so
                // it is computed here at the production call site and injected
                // below: `dispatch_workspace_tiling_intent` stays a pure
                // function of its gate argument, which keeps owner-dispatch
                // tests deterministic on headless runners.
                let gate = crate::tiling::toggle_gate_outcome(
                    suspend_read(state, me, fulls).veto.block,
                    foreground_elevated(me),
                );
                dispatch_workspace_tiling_intent(state, me, store, dir, fulls, areas, intent, gate);
            }
            QueuedSnapEvent::Sticky(intent) => {
                dispatch_sticky_intent(state, me, store, dir, fulls, areas, intent);
            }
            QueuedSnapEvent::Launch(intent) => {
                // The suspend/elevated fence reads live foreground state, so
                // it is computed here at the production call site and injected
                // below: `dispatch_launch_intent` stays a pure function of its
                // gate argument, which keeps owner-dispatch tests deterministic
                // on headless runners.
                let gate = crate::tiling::toggle_gate_outcome(
                    suspend_read(state, me, fulls).veto.block,
                    foreground_elevated(me),
                );
                dispatch_launch_intent(state, intent, &SystemLaunchBackend, gate);
            }
        }
    }
}
