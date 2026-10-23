#![forbid(unsafe_code)]
use super::*;

/// Test-needed exact-owner whole-workspace migration poll for the normal
/// `tile` loop only (tentative, pending user review). Consume-once,
/// exact-owner, and proof refusal ride the shared `poll_workspace_cli_request`
/// prefix shape. The built intent dispatches through the real production
/// migration route (`workspace_migrate_one`), so every production fence
/// (origin, lifetime, scope, topology, suspension) applies unchanged. The
/// keyboard takeover switch never gates this (out-of-hook dogfood/recovery,
/// like the workspace, directional, and send-output routes); the hook still
/// filters injected chords, so synthetic input can never drive a migration.
/// Logs one `migrate-workspace` line with edge `cli` for the queued request;
/// the dispatch itself logs the production `workspace-migrate` line with the
/// same direction.
pub(super) fn poll_migrate_workspace_cli_request(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    use tiler_core::directional::Direction;
    let path = dir.join(MIGRATE_WORKSPACE_REQUEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    let log_path = state.log_path.clone();
    // Best-effort label for the pre-dispatch logs below: the actual parsed
    // direction when the body parses, `unknown` otherwise. Never fake
    // precision for an unparseable payload.
    let parsed = parse_migrate_workspace_request(text.trim()).ok();
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
                "event": "migrate-workspace",
                "tick": tick,
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
                "event": "migrate-workspace",
                "tick": tick,
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
            "event": "migrate-workspace",
            "tick": tick,
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
    // (`parse_migrate_workspace_request` already bounded the direction
    // literal; this match is the exhaustive mapping, never a default).
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
    // Migration acts on the live output context's active workspace, never
    // a managed mover: no foreground origin resolves here, matching the
    // production chord route (origin rides only for queue parity and is
    // ignored at dispatch). The world route resolves the action source in
    // core from the neutral report facts (foreground, pointer, cache,
    // output order), so a stale cache never pins the source when live
    // focus moved.
    state.tick += 1;
    let tick = state.tick;
    let queued = cli_line(tick, "dispatched");
    log_json_at(&log_path, queued);
    // Takeover never gates this out-of-hook route (workspace/directional/
    // send-output parity): the intent below rides the production migration
    // dispatch directly with the same suspension/scope/topology fences.
    let intent = crate::snapkey::QueuedWorkspaceMigrateIntent {
        direction,
        edge: crate::snapkey::SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: Instant::now(),
    };
    workspace_migrate_one(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        &mut [],
        &[],
        &intent,
        super::observation::sample_cursor(),
        tick,
    );
}
