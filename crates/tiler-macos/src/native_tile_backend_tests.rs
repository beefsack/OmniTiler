#![cfg(test)]
//! Mechanical split of `native_tile_backend.rs` cfg tests (no behavior change).
use crate::native_tile_backend::*;

const INTERVAL: u64 = 1_000;

#[test]
fn backstop_covers_missed_notifications_after_one_interval() {
    // Silence is the minimize/close defect: zero notifications after
    // the last read must still reread once the interval elapses.
    assert!(!scoped_backstop_due(
        false,
        false,
        0,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(!scoped_backstop_due(
        false,
        false,
        999,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(scoped_backstop_due(
        false,
        false,
        1_000,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(scoped_backstop_due(
        false,
        false,
        60_000,
        Some(0),
        None,
        INTERVAL
    ));
    // Never read at all: due immediately (a read is always safe).
    assert!(scoped_backstop_due(false, false, 0, None, None, INTERVAL));
}

#[test]
fn backstop_notification_rereads_immediately_and_stop_withholds() {
    // A delivered notification rereads at once, even right after a read.
    assert!(scoped_backstop_due(
        false,
        true,
        10,
        Some(10),
        None,
        INTERVAL
    ));
    // Stop withholds everything, even a due backstop or notification.
    assert!(!scoped_backstop_due(
        true,
        false,
        60_000,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(!scoped_backstop_due(
        true,
        true,
        60_000,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(!scoped_backstop_due(true, false, 0, None, None, INTERVAL));
}

#[test]
fn backstop_failure_backs_off_at_the_same_cadence() {
    // A failed reread at t=1000 never retries fast: silence inside the
    // next interval stays quiet, then the backstop refires.
    assert!(!scoped_backstop_due(
        false,
        false,
        1_500,
        Some(0),
        Some(1_000),
        INTERVAL
    ));
    assert!(!scoped_backstop_due(
        false,
        false,
        1_999,
        Some(0),
        Some(1_000),
        INTERVAL
    ));
    assert!(scoped_backstop_due(
        false,
        false,
        2_000,
        Some(0),
        Some(1_000),
        INTERVAL
    ));
    // A notification still rereads immediately despite backoff (fresh
    // truth may have arrived; the failure was about the last read).
    assert!(scoped_backstop_due(
        false,
        true,
        1_500,
        Some(0),
        Some(1_000),
        INTERVAL
    ));
}

#[test]
fn window_subscription_set_carries_the_minimize_pair() {
    // The tile-path consumer of the window-level set: minimize and
    // deminimize must ride alongside move/resize/destroyed, or the
    // minimize-reflow leg can never observe. Membership is
    // order-independent; emptiness or a missing pair fails.
    for name in [
        "AXMoved",
        "AXResized",
        "AXUIElementDestroyed",
        "AXWindowMiniaturized",
        "AXWindowDeminiaturized",
    ] {
        assert!(
            crate::ffi::AX_SESSION_WINDOW_NOTIFICATIONS.contains(&name),
            "{name} must be window-observed for the tile backstop to see it"
        );
    }
    assert!(
        crate::ffi::AX_SESSION_APP_NOTIFICATIONS.contains(&"AXMainWindowChanged"),
        "AXMainWindowChanged must be app-observed"
    );
}

#[test]
fn reread_line_names_notification_vs_backstop_vs_initial() {
    // A baseline must tell a notification-driven reread from the silence
    // backstop: the trigger token is the discriminator.
    assert_eq!(
        format_scoped_reread_line(4242, "notification", 9100),
        "tiler-macos tile reread pid=4242 trigger=notification now_ms=9100",
    );
    assert_eq!(
        format_scoped_reread_line(4242, "backstop", 10100),
        "tiler-macos tile reread pid=4242 trigger=backstop now_ms=10100",
    );
    assert_eq!(
        format_scoped_reread_line(4242, "initial", 9000),
        "tiler-macos tile reread pid=4242 trigger=initial now_ms=9000",
    );
}

#[test]
fn frame_set_line_reports_actual_readback_not_the_request() {
    // Success reflects the exact AX readback: dispatch truth (`outcome`)
    // plus observed truth (`exact` plus the readback rect). An applied
    // dispatch with a mismatched readback is not arrival.
    assert_eq!(
        format_frame_set_line(
            4242,
            7,
            77,
            (0, 30, 1150, 1139),
            Some((0, 30, 1150, 1139)),
            "applied",
            true,
            12,
            9100
        ),
        "tiler-macos tile frame-set pid=4242 wid=7 revision=77 requested=(0, 30, 1150, 1139) readback=Some((0, 30, 1150, 1139)) outcome=applied exact=true elapsed_ms=12 now_ms=9100",
    );
    assert_eq!(
        format_frame_set_line(
            4242,
            7,
            77,
            (0, 30, 1150, 1139),
            Some((10, 30, 1140, 1139)),
            "applied",
            false,
            12,
            9100
        ),
        "tiler-macos tile frame-set pid=4242 wid=7 revision=77 requested=(0, 30, 1150, 1139) readback=Some((10, 30, 1140, 1139)) outcome=applied exact=false elapsed_ms=12 now_ms=9100",
    );
    // Unreadable readback renders explicit (never guessed).
    assert_eq!(
        format_frame_set_line(
            4242,
            7,
            77,
            (0, 30, 1150, 1139),
            None,
            "partial-applied",
            false,
            12,
            9100
        ),
        "tiler-macos tile frame-set pid=4242 wid=7 revision=77 requested=(0, 30, 1150, 1139) readback=None outcome=partial-applied exact=false elapsed_ms=12 now_ms=9100",
    );
}

#[test]
fn reread_line_names_topology_and_arrival_triggers() {
    // The event-driven attribution: a revision-changing publication lands
    // as `topology`, a verified arrival at an unchanged revision as
    // `arrival` - same line shape, only the token differs.
    assert_eq!(
        format_scoped_reread_line(4242, "topology", 9100),
        "tiler-macos tile reread pid=4242 trigger=topology now_ms=9100",
    );
    assert_eq!(
        format_scoped_reread_line(4242, "arrival", 9110),
        "tiler-macos tile reread pid=4242 trigger=arrival now_ms=9110",
    );
}

#[test]
fn reread_generation_consumes_once_and_coalesces() {
    // Never consumed adopts without requesting: startup already
    // published, so the first pump manufactures no reread.
    assert_eq!(consume_reread_generation(None, 5), (false, 5));
    // Steady state stays quiet.
    assert_eq!(consume_reread_generation(Some(5), 5), (false, 5));
    // One bump requests exactly one reread; adopting it keeps the next
    // pump quiet with no new bump (bursts coalesce, failures never
    // replay - the caller adopts even on failure).
    let (due, seen) = consume_reread_generation(Some(4), 7);
    assert!(due);
    assert_eq!(consume_reread_generation(Some(seen), 7), (false, 7));
}

#[test]
fn trigger_selects_notification_topology_arrival_backstop() {
    // A delivered notification rereads immediately (existing precedence:
    // the bump is still consumed by the caller alongside it).
    assert_eq!(
        select_scoped_reread_trigger(true, false, false, false, false),
        Some("notification")
    );
    assert_eq!(
        select_scoped_reread_trigger(true, true, true, true, false),
        Some("notification")
    );
    // A consumed bump rereads even when the backstop is silent: the
    // revision move attributes `topology`, an unchanged revision
    // `arrival` (verified native arrival needs no topology change).
    assert_eq!(
        select_scoped_reread_trigger(false, true, true, false, false),
        Some("topology")
    );
    assert_eq!(
        select_scoped_reread_trigger(false, true, false, false, false),
        Some("arrival")
    );
    // Silence past the interval still backstops; a never-read owner reads
    // immediately; otherwise the owner stays quiet.
    assert_eq!(
        select_scoped_reread_trigger(false, false, false, true, false),
        Some("backstop")
    );
    assert_eq!(
        select_scoped_reread_trigger(false, false, false, false, true),
        Some("initial")
    );
    assert_eq!(
        select_scoped_reread_trigger(false, false, true, false, false),
        None
    );
    assert_eq!(
        select_scoped_reread_trigger(false, false, false, false, false),
        None
    );
}

#[test]
fn owner_consumes_real_cell_topology_and_arrival_without_ax() {
    use std::sync::Arc;

    use crate::runtime::NativeContextCell;
    use crate::runtime_types::NativeContext;
    fn ctx(revision: u64) -> Arc<NativeContext> {
        Arc::new(NativeContext {
            revision,
            domains: Vec::new(),
            skipped: Vec::new(),
        })
    }
    let cell = NativeContextCell::new();
    // Startup publication (topology revision 7).
    cell.store(Some(ctx(7)));
    let mut seen: Option<u64> = None;
    let mut last_revision: Option<u64>;
    // First pump adopts without requesting (startup already published).
    let (loaded, generation) = cell.load_versioned();
    let (due, next) = consume_reread_generation(seen, generation);
    assert!(!due);
    seen = Some(next);
    last_revision = loaded.map(|c| c.revision);
    // Quiet pump: nothing due (no AX ran - pure cell plus decision).
    let (_, generation) = cell.load_versioned();
    assert!(!consume_reread_generation(seen, generation).0);
    // Verified arrival at an unchanged revision: the bump requests a
    // reread even though the revision and the backstop are both silent.
    cell.request_reread();
    let (loaded, generation) = cell.load_versioned();
    let (due, next) = consume_reread_generation(seen, generation);
    assert!(due);
    let revision_changed =
        last_revision.is_none_or(|last| Some(last) != loaded.as_ref().map(|c| c.revision));
    assert!(!revision_changed);
    assert_eq!(
        select_scoped_reread_trigger(false, due, revision_changed, false, false),
        Some("arrival")
    );
    seen = Some(next);
    last_revision = loaded.map(|c| c.revision);
    // No replay without a new bump (failed or not, the mark is spent).
    let (_, generation) = cell.load_versioned();
    assert!(!consume_reread_generation(seen, generation).0);
    // Changed topology revision: same-revision republish stays quiet,
    // the revision move requests with `topology` attribution.
    cell.store(Some(ctx(7)));
    let (_, generation) = cell.load_versioned();
    assert!(!consume_reread_generation(seen, generation).0);
    cell.store(Some(ctx(8)));
    let (loaded, generation) = cell.load_versioned();
    let (due, next) = consume_reread_generation(seen, generation);
    assert!(due);
    let revision_changed =
        last_revision.is_none_or(|last| Some(last) != loaded.as_ref().map(|c| c.revision));
    assert!(revision_changed);
    assert_eq!(
        select_scoped_reread_trigger(false, due, revision_changed, false, false),
        Some("topology")
    );
    seen = Some(next);
    // Context invalidation (clear): the loss is consumed once; downstream
    // still refuses the cleared cell (existing no-context fence).
    cell.store(None);
    let (loaded, generation) = cell.load_versioned();
    assert!(loaded.is_none());
    let (due, next) = consume_reread_generation(seen, generation);
    assert!(due);
    seen = Some(next);
    let (_, generation) = cell.load_versioned();
    assert!(!consume_reread_generation(seen, generation).0);
}

#[test]
fn runtime_producer_bumps_cell_generation_without_new_seams() {
    // Producer shape: topology publication routes through the cell store
    // (changed revisions bump), invalidation clears through it, and the
    // verified-arrival request bumps unconditionally - all under brief
    // locks, with no new bus, timer, or thread on the path.
    let runtime = include_str!("runtime.rs");
    assert!(
        runtime.contains("guard.native_cell.store(context)"),
        "topology publication must route through the invalidation cell"
    );
    assert!(
        runtime.contains("guard.native_cell.store(None)"),
        "invalidation must clear through the invalidation cell"
    );
    assert!(
        runtime.contains("pub fn request_owner_reread(&self)")
            && runtime.contains("native_cell.request_reread()"),
        "verified arrival must bump the owner generation via the handle"
    );
    // Stale/current fences are untouched: superseded revisions and absent
    // contexts still refuse instead of actuating.
    assert!(runtime.contains("stale-revision-rejected"));
    assert!(runtime.contains("no-topology-rejected"));
    let worker = include_str!("runtime_worker.rs");
    assert!(worker.contains("revision-superseded"));
    // No faster polling, no shared-core change: the owner slices keep
    // their cadence and the core stays out of the diff.
    assert!(worker.contains("OBSERVE_SLICE: Duration = Duration::from_millis(20)"));
    let backend = include_str!("native_tile_backend.rs");
    assert!(
        backend.contains(
            "NATIVE_PUMP_SLICE: std::time::Duration = std::time::Duration::from_millis(20)"
        )
    );
    assert!(
        backend.contains("forbid(unsafe_code)"),
        "no new unsafe on the tile path"
    );
}

#[test]
fn producer_path_stays_off_main_and_unsafe_free() {
    // The tile owner never reads the main-thread display snapshot and the
    // module keeps its unsafe ban: phase logging adds no new unsafe and
    // no new main-thread touch.
    let source = include_str!("native_tile_backend.rs");
    assert!(
        !source.contains("ns_display_snapshot"),
        "tile owner must not read the main-thread display snapshot"
    );
    assert!(
        source.contains("forbid(unsafe_code)"),
        "no new unsafe on the tile path"
    );
    assert!(
        source.contains("format_scoped_reread_line") && source.contains("format_frame_set_line"),
        "phase attribution stays on the metadata-only formatters"
    );
}
