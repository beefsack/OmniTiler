#![forbid(unsafe_code)]
use super::*;

use super::directional_resolution::output_for_rect;
use super::pager_host::dispatch_verified_select;

// Focused-output pager select (tray stopgap, decided 2026-10-15): one
// drained pager pick selects that exact workspace through the existing
// `workspace_do_select` resolver. Consume-once like the CLI request path;
// suspend/elevated fences, fresh observation, departure memory, and bounded
// opaque logging mirror that path with edge `tray`. The keyboard takeover
// switch never gates this (out-of-hook, like the CLI path). Proof owners
// never serve it: they own no tray, so there is nothing to drain.

/// Genuine focused output for the tray number and pager rows (tentative):
/// the foreground window's monitor when known, else the pointer monitor,
/// else the cached last-switch output, else output ordering. Updates for
/// output focus changes, not only workspace switches; the cached
/// `active_output` itself is untouched. `owner_hwnd` is the tray owner's
/// raw window value: when the live foreground is exactly that window, the
/// menu's own transient `SetForegroundWindow` is preserved as the last
/// genuine output instead of turning the tray click point into a focus
/// change. Retention lives in `state.last_genuine_output` and updates only
/// from a known real foreground monitor.
pub(super) fn tray_focused_output(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    owner_hwnd: u64,
) -> Option<String> {
    let keys = state.output_devices();
    let foreground_hwnd = crate::ffi::foreground_window();
    // Exact owned-UI match only (nonzero owner): a missing foreground keeps
    // the ordinary fallback, and any other real foreground recomputes, so
    // the stale fence never weakens for actual other-output app focus.
    let owned_ui = owner_hwnd != 0 && foreground_hwnd == owner_hwnd;
    let foreground = if foreground_hwnd == 0 || owned_ui {
        None
    } else {
        foreground_monitor(areas)
    };
    let pointer = cursor_pos().and_then(|(x, y)| output_for_point(areas, x, y));
    let cached = (!state.active_output.is_empty()).then_some(state.active_output.as_str());
    let (focused, retained) = crate::tray_pager::focused_output_transient(
        &keys,
        cached,
        foreground.as_deref(),
        owned_ui,
        pointer.as_deref(),
        state.last_genuine_output.as_deref(),
    );
    state.last_genuine_output = retained;
    focused
}

/// Foreground window's monitor key (`None` without a readable foreground
/// frame). Same rectangle rule as directional placement; unknown monitors
/// never invent a key (the caller filters against live output keys).
fn foreground_monitor(areas: &[MonitorArea]) -> Option<String> {
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 {
        return None;
    }
    let raw = crate::ffi::window_rect(foreground)?;
    rect_from_win(raw).map(|rect| output_for_rect(areas, &rect))
}

/// Drain one pending pager-select request and dispatch it. Called beside the
/// CLI request poll on non-gesture turns (suspended and active alike); a
/// pick waiting through a gesture stays queued in the tray until the next
/// such turn. Every refusal consumes once with an opaque log line and no
/// native effect.
pub(super) fn poll_tray_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    tray: &mut Option<crate::tray_sys::TrayOwner>,
) {
    let Some(tray) = tray.as_mut() else {
        return;
    };
    let Some(request) = tray.take_select_request() else {
        return;
    };
    let index = request.index.min(255) as u8;
    state.tick += 1;
    let tick = state.tick;
    let start = Instant::now();
    if suspend_read(state, me, fulls).veto.block {
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, "select", index, "tray", "suspended"),
        );
        return;
    }
    if foreground_elevated(me) {
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, "select", index, "tray", "elevated-foreground"),
        );
        return;
    }
    // Stale-scope fence: the rendered output must still be focused and the
    // rendered workspace still live there. Anything else refuses with opaque
    // tokens only, never raw scope ids. The focus recompute preserves the
    // last genuine output across our own transient menu foreground, but a
    // real other-output app focus moves it and refuses here.
    let owner = tray.hwnd_value();
    let focused = tray_focused_output(state, areas, owner);
    let workspace_live = state
        .workspace_ids(&request.output)
        .iter()
        .any(|id| id == &request.workspace);
    if !crate::tray_pager::verify_select_scope(&request, focused.as_deref(), workspace_live) {
        let mut line = workspace_log(state, tick, "select", index, "tray", "refused-stale-scope");
        line["output"] = serde_json::Value::from(state.output_token(&request.output));
        line["workspace"] = serde_json::Value::from(
            state.workspace_token(&request.output, &request.workspace),
        );
        log_json_at(&state.log_path, line);
        return;
    }
    let output = request.output.clone();
    let target = request.workspace.clone();
    dispatch_verified_select(
        state, me, store, dir, fulls, areas, &output, &target, index, tick, start, "tray",
    );
}
