#![forbid(unsafe_code)]
use super::*;

/// Test-needed exact-owner explicit output-send poll for the normal `tile`
/// loop only (tentative, pending user review). Consume-once, exact-owner, and
/// proof refusal ride the shared `poll_workspace_cli_request` prefix shape.
/// The live foreground resolves through the same `snap_origins` map the hook
/// binds chords against (never a carried HWND, never a retarget), and the
/// built intent dispatches through the real explicit `SendToOutput`
/// production route (`workspace_output_send_one` / `run_output_send`), so
/// every production fence (origin, lifetime, scope, topology, arrival,
/// follow/stay, suspension) applies unchanged. The keyboard takeover switch
/// never gates this (out-of-hook dogfood/recovery, like the workspace and
/// directional routes); the hook still filters injected chords, so synthetic
/// input can never drive a send. Logs one `send-output` line with edge `cli`
/// for the queued request; the dispatch itself logs the production
/// `workspace-output` line with the same direction/follow.
pub(super) fn poll_send_output_cli_request(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    use tiler_core::directional::Direction;
    let path = dir.join(SEND_OUTPUT_REQUEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    let log_path = state.log_path.clone();
    // Best-effort labels for the pre-dispatch logs below: the actual parsed
    // follow/direction when the body parses, `follow`/`unknown` otherwise.
    // Never fake precision for an unparseable payload.
    let parsed = parse_send_output_request(text.trim()).ok();
    let follow_hint = parsed
        .as_ref()
        .map(|request| request.follow)
        .unwrap_or(true);
    let direction_hint = parsed
        .as_ref()
        .map(|request| request.direction.clone())
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
                "event": "send-output",
                "tick": tick,
                "follow": follow_hint,
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
                "event": "send-output",
                "tick": tick,
                "follow": follow_hint,
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
            "event": "send-output",
            "tick": tick,
            "follow": request.follow,
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
    // Direction typed decode onto the classifier vocabulary
    // (`parse_send_output_request` already bounded the direction literal;
    // this match is the exhaustive mapping, never a default).
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
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget. The dispatch
    // validates this captured origin (token/HWND/PID/creation/lifetime),
    // never a fresh foreground read.
    let foreground_hwnd = crate::ffi::foreground_window();
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "unmanaged"));
        return;
    };
    if origin.hwnd != foreground_hwnd {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "unmanaged"));
        return;
    }
    let Some(output) = chord_output(state, areas) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "unknown-output"));
        return;
    };
    state.ensure_device(&output);
    state.tick += 1;
    let tick = state.tick;
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(&log_path, cli_line(tick, "observation-failed"));
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    let mut queued = cli_line(tick, "dispatched");
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&log_path, queued);
    // Takeover never gates this out-of-hook route (workspace/directional
    // parity): the intent below rides the production output-send dispatch
    // directly with the same origin/lifetime/scope/arrival fences.
    let intent = crate::snapkey::QueuedWorkspaceOutputIntent {
        direction,
        follow: request.follow,
        edge: crate::snapkey::SnapEdge::Down,
        origin: Some(origin.clone()),
        consumed: true,
        announce: true,
        tick: Instant::now(),
    };
    workspace_output_send_one(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        &mut observed,
        &retained,
        &output,
        &intent,
        &origin,
        tick,
    );
}
