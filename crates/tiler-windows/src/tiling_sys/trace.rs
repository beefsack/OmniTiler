use super::*;

use super::owner_work::OwnerWorkGuard;

/// Process-relative monotonic start for the `mono_ms` stamp below.
static LOG_START: OnceLock<Instant> = OnceLock::new();

/// Milliseconds since the first tiling log row of this process. Common
/// monotonic time across normal log records; ticks order work, this measures
/// delays. Never overwrites an existing field.
pub(super) fn log_mono_ms() -> u64 {
    LOG_START
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

pub(super) fn log_json_at(path: &Path, mut value: serde_json::Value) {
    use std::io::Write;
    if let Some(row) = value.as_object_mut()
        && row.get("mono_ms").is_none()
    {
        row.insert("mono_ms".to_owned(), serde_json::json!(log_mono_ms()));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{value}");
    }
}

/// Fresh-border refresh (foreground query plus overlay presentation together)
/// with trace-only timing into the owner-work guard. `refresh_active_border`
/// itself is unchanged: no paint-API signature churn.
pub(super) fn refresh_border_timed(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    guard: &mut OwnerWorkGuard<'_>,
) {
    if guard.timing_active() {
        let started = Instant::now();
        refresh_active_border(state, me, fulls);
        guard.add_border_refresh(started.elapsed());
    } else {
        refresh_active_border(state, me, fulls);
    }
}

/// Trace-only placement diagnostic for the just-completed Engine op:
/// bounded startup inputs (opaque window token plus rectangle) with fit
/// outcome/reason and the resulting ordered tree description, and the
/// send anchor branch plus opaque leaf with projected rectangle/axis.
/// Gated on `state.trace`; production stays quiet. Opaque tokens and
/// integer geometry only: no HWNDs, pids, paths, or titles.
pub(super) fn log_engine_placement_trace(
    state: &TileLoop,
    tick: u64,
    correlation: &str,
    op: &'static str,
) {
    if !state.trace {
        return;
    }
    let log_path = state.log_path.clone();
    if let Some(trace) = state.engine.last_startup_fit_trace() {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "placement-trace",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "kind": "startup-fit",
                "windows": trace.windows,
                "domain": [trace.domain_bounds.x, trace.domain_bounds.y, trace.domain_bounds.w, trace.domain_bounds.h],
                "inputs": trace.inputs.iter().map(|input| serde_json::json!({
                    "window": input.window.0,
                    "rect": [input.rect.x, input.rect.y, input.rect.w, input.rect.h],
                })).collect::<Vec<_>>(),
                "outcome": trace.outcome,
                "reason": trace.reason,
                "centre_splits": trace.centre_splits,
                "leaves": trace.leaves,
                "topology": trace.topology,
            }),
        );
    }
    if let Some(trace) = state.engine.last_send_placement() {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "placement-trace",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "kind": "send-placement",
                "anchor": trace.anchor_kind,
                "anchor_leaf": trace.anchor.as_ref().map(|leaf| leaf.0.as_str()),
                "axis": match trace.axis {
                    tiler_core::directional::Axis::Horizontal => "horizontal",
                    tiler_core::directional::Axis::Vertical => "vertical",
                },
                "projected": [trace.projected.x, trace.projected.y, trace.projected.w, trace.projected.h],
                "target_leaves": trace.target_leaves,
            }),
        );
    }
}
