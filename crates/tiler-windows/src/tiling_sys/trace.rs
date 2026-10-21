#![forbid(unsafe_code)]
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
