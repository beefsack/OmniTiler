#![forbid(unsafe_code)]
use super::*;

/// Test-needed exact-owner keyboard-resize poll for the normal `tile` loop
/// only (tentative, pending user review). Consume-once, exact-owner, and
/// proof refusal ride the shared `poll_workspace_cli_request` prefix shape.
/// The live foreground resolves through the same `snap_origins` map the hook
/// binds chords against (never a carried HWND, never a retarget), and the
/// built intent dispatches through the real `keyboard_tick` resize arm, so
/// every production fence (takeover, suspend/elevated, origin, lifetime,
/// proof, scope, float/sticky, workspace, overlay, busy) and the exact
/// repeat tracker apply unchanged. Logs one `resize` line with edge `cli`
/// for the queued request; the dispatch itself logs the production `resize`
/// line with the scheduled `press_index`.
pub(super) fn poll_resize_cli_request(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    use tiler_core::directional::Direction;
    let path = dir.join(RESIZE_REQUEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    let log_path = state.log_path.clone();
    // Best-effort labels for the pre-dispatch logs below: the actual parsed
    // direction/mode when the body parses, `unknown` otherwise. Never fake
    // precision for an unparseable payload.
    let parsed = parse_resize_request(text.trim()).ok();
    let direction_hint = parsed
        .as_ref()
        .map(|request| request.direction.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    let mode_hint = parsed
        .as_ref()
        .map(|request| request.mode.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    // Proof owners never serve the normal control: consume once with an
    // honest refusal and no native effect so the queue cannot wedge.
    if state.allowlist.is_some() {
        let _ = std::fs::remove_file(&path);
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "resize",
                "tick": tick,
                "mode": mode_hint,
                "direction": direction_hint,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "refused-proof",
            }),
        );
        return;
    }
    // Consume-before-dispatch: exactly once, never replayed, never timed out
    // and retried. A bad body is still consumed with a refused outcome.
    let _ = std::fs::remove_file(&path);
    let Some(request) = parsed else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "resize",
                "tick": tick,
                "mode": mode_hint,
                "direction": direction_hint,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "refused",
            }),
        );
        return;
    };
    let cli_line = |tick: u64, outcome: &'static str| {
        serde_json::json!({
            "event": "resize",
            "tick": tick,
            "mode": request.mode,
            "direction": request.direction,
            "edge": "cli",
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    if request.creation != me.process_creation
        || request.pid != me.pid
        || !exe_paths_equal(&request.exe_path, &me.exe_path)
        || request.user_sid != me.user_sid
        || request.session_id != me.session_id
    {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "refused"));
        return;
    }
    // Production per-intent fence first, before any observation: a suspended
    // session or an elevated foreground settles here with no side effects.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, outcome));
        return;
    }
    // Direction/mode typed decode onto the classifier vocabulary
    // (`parse_resize_request` already bounded the literals; this match is
    // the exhaustive mapping, never a default).
    let direction = match request.direction.as_str() {
        "left" => Direction::Left,
        "right" => Direction::Right,
        "up" => Direction::Up,
        "down" => Direction::Down,
        _ => {
            state.tick += 1;
            let tick = state.tick;
            log_json_at(&log_path, cli_line(tick, "refused"));
            return;
        }
    };
    let mode = match request.mode.as_str() {
        "outwards" => crate::snapkey::ResizeMode::Outwards,
        "inwards" => crate::snapkey::ResizeMode::Inwards,
        _ => {
            state.tick += 1;
            let tick = state.tick;
            log_json_at(&log_path, cli_line(tick, "refused"));
            return;
        }
    };
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget. The dispatch
    // re-resolves it against its own fresh observation, so a foreground
    // change between here and there refuses instead of misacting.
    let foreground_hwnd = crate::ffi::foreground_window();
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "unmanaged"));
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut queued = cli_line(tick, "dispatched");
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&log_path, queued);
    keyboard_tick(
        state,
        me,
        fulls,
        areas,
        vec![QueuedSnapEvent::Resize(
            crate::snapkey::QueuedResizeIntent {
                mode,
                direction,
                edge: crate::snapkey::SnapEdge::Down,
                origin: Some(origin),
                consumed: true,
                announce: true,
                tick: Instant::now(),
            },
        )],
        None,
    );
}
