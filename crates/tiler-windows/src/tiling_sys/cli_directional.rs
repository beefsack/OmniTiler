#![forbid(unsafe_code)]
use super::*;

/// Test-needed exact-owner directional move/focus poll for the normal `tile`
/// loop only (tentative, pending user review). Consume-once, exact-owner, and
/// proof refusal ride the shared `poll_workspace_cli_request` prefix shape.
/// The live foreground resolves through the same `snap_origins` map the hook
/// binds chords against (never a carried HWND, never a retarget), and the
/// built intent dispatches through the real `keyboard_tick` directional arm,
/// so every production fence (origin, lifetime, scope, topology, arrival,
/// follow-focus) and the exact directional semantics apply unchanged. The
/// keyboard takeover switch never gates this (out-of-hook dogfood/recovery,
/// like the workspace route); the hook still filters injected chords, so
/// synthetic input can never drive a move or focus. Logs one `directional`
/// line with edge `cli` for the queued request; the dispatch itself logs the
/// production `snap` line with the same op/direction.
pub(super) fn poll_directional_cli_request(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    use tiler_core::directional::Direction;
    let path = dir.join(DIRECTIONAL_REQUEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    let log_path = state.log_path.clone();
    // Best-effort labels for the pre-dispatch logs below: the actual parsed
    // op/direction when the body parses, `move`/`unknown` otherwise. Never
    // fake precision for an unparseable payload.
    let parsed = parse_directional_request(text.trim()).ok();
    let op_hint = parsed
        .as_ref()
        .map(|request| request.op.as_str())
        .unwrap_or("move");
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
                "event": "directional",
                "tick": tick,
                "op": op_hint,
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
                "event": "directional",
                "tick": tick,
                "op": op_hint,
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
            "event": "directional",
            "tick": tick,
            "op": request.op.as_str(),
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
    // Op/direction typed decode onto the classifier vocabulary
    // (`parse_directional_request` already bounded the direction literal and
    // the serde op; this match is the exhaustive mapping, never a default).
    let op = match request.op {
        crate::tiling::DirectionalOp::Move => crate::snapkey::SnapOp::Move,
        crate::tiling::DirectionalOp::Focus => crate::snapkey::SnapOp::Focus,
    };
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
    // against; never a carried HWND, never a retarget. The cached map still
    // names the pre-reveal set right after a foreground return (the reveal
    // proves focus before the next observation publishes), so rebuild it
    // from a fresh observation first: `observe` plus `publish_managed`
    // only. Admission (`ensure_workspace_assignments`,
    // `clear_maximize_at_admission`) stays with the dispatch below, which
    // runs the full sequence itself; rebuilding the map performs no native
    // writes and no membership or tag mutation; a refused poll may refresh
    // observation caches only. The dispatch still re-resolves
    // against its own fresh observation, so a foreground change between
    // here and there refuses instead of misacting. No invented origins:
    // the map rebuilds only from eligible observed plus verified
    // overlay-retained rows. The foreground read is the seam-aware one
    // (production-identical; scripted in unit tests).
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "unmanaged"));
        return;
    };
    publish_managed(state, me, &observed, &retained);
    let foreground_hwnd = live_foreground_window();
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
        store,
        dir,
        fulls,
        areas,
        vec![QueuedSnapEvent::Intent(crate::snapkey::QueuedIntent {
            op,
            direction,
            edge: crate::snapkey::SnapEdge::Down,
            origin: Some(origin),
            consumed: true,
            announce: true,
            tick: Instant::now(),
        })],
        None,
    );
}
