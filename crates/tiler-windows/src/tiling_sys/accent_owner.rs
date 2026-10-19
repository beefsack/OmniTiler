#![forbid(unsafe_code)]
//! Single shared owner accent resolve: one versioned raw cache in the owner
//! state, resolved once per broadcast version on the owner thread and
//! consumed as plain data by both the border and the pager.
//!
//! The border (same-thread overlay and bridged renderer frame) and the pager
//! strips all paint from `state.accent_shared`: the first consumer to observe
//! a moved version runs the owner WinRT resolve, every later consumer in the
//! same version reads the same snapshot with no further native call. The
//! renderer never queries WinRT: it paints the snapshot the owner posted.
//! Steady-state versions perform no resolve and log nothing; every actual
//! resolve logs exactly one `shared`/`owner` diagnostic record.

use super::TileLoop;
use super::trace::log_json_at;

/// Resolve the shared accent snapshot for the current broadcast version,
/// running the owner WinRT source only when the version moved since the last
/// resolve. Returns the snapshot both surfaces paint from. The pre-query
/// version is recorded, so a broadcast landing mid-resolve re-triggers the
/// next refresh instead of being lost. Exactly one diagnostic record is
/// logged per actual resolve (surface `shared`, role `owner`); steady-state
/// cache hits log nothing. Stage markers write to the log synchronously as
/// they fire (before the native call they bracket), never buffered past the
/// resolve return.
pub(super) fn refresh_shared_accent(
    state: &mut TileLoop,
) -> Option<crate::active_border::SystemAccent> {
    let version = crate::ffi::accent_version();
    let trace = state.trace;
    let log_path = state.log_path.clone();
    let super::TileLoop {
        ref mut accent_shared,
        ref mut accent_owner,
        ..
    } = *state;
    // The trigger is fixed before the resolve: stage lines and the record
    // below name the version this resolve runs against.
    let trigger = if accent_shared.last_version().is_none() {
        crate::active_border::AccentQueryTrigger::Startup
    } else {
        crate::active_border::AccentQueryTrigger::Broadcast
    };
    refresh_shared_cache(
        accent_shared,
        version,
        |observer| accent_owner.query_raw(observer),
        |stage| {
            if trace {
                log_json_at(
                    &log_path,
                    crate::active_border::accent_query_stage_event(
                        crate::active_border::AccentQuerySurface::Shared,
                        crate::active_border::AccentQueryRole::Owner,
                        trigger,
                        version,
                        stage,
                    ),
                );
            }
        },
        |report| {
            log_json_at(&log_path, crate::active_border::accent_query_event(report));
        },
    )
}

/// Resolve the shared snapshot against an explicit version with an injected
/// resolve function. Production uses [`refresh_shared_accent`]; offline tests
/// inject a fake with no native call. `on_stage` fires synchronously inside
/// the actual resolve only (each stage is delivered before the native call
/// it brackets, so an abrupt exit still leaves its marker); steady-state
/// hits run no resolve and fire nothing. `on_report` receives the single
/// diagnostic record of an actual resolve only. No stage buffer is kept:
/// every stage is delivered to `on_stage` and forgotten. The shared record
/// uses the pager policy, so its `resolved` is the shared-source fallback,
/// not any one consumer's pixels (see `accent_query_report`).
fn refresh_shared_cache(
    shared: &mut crate::active_border::AccentCache,
    version: u64,
    query: impl FnOnce(Option<&dyn Fn(crate::ffi::AccentStage)>) -> crate::active_border::AccentRaw,
    on_stage: impl Fn(crate::ffi::AccentStage),
    on_report: impl FnOnce(&crate::active_border::AccentQueryReport),
) -> Option<crate::active_border::SystemAccent> {
    use crate::active_border::{
        AccentQueryRole, AccentQuerySurface, AccentQueryTrigger, DEFAULT_COLOR_RGB,
        accent_query_report,
    };
    let trigger = if shared.last_version().is_none() {
        AccentQueryTrigger::Startup
    } else {
        AccentQueryTrigger::Broadcast
    };
    // The pre-query version is captured here: `refresh_detailed` records it,
    // so a broadcast landing mid-resolve leaves the cache behind the live
    // version and the next refresh re-resolves instead of losing the bump.
    let raw = shared.refresh_detailed(version, || query(Some(&on_stage)));
    if let Some(raw) = raw.as_ref() {
        let report = accent_query_report(
            AccentQuerySurface::Shared,
            AccentQueryRole::Owner,
            trigger,
            version,
            raw,
            true,
            DEFAULT_COLOR_RGB,
        );
        on_report(&report);
    }
    shared.get()
}

#[cfg(test)]
mod tests {
    use super::super::rmax03_adapter_tests::test_state;
    use crate::active_border::{
        AccentQueryRole, AccentQuerySurface, AccentQueryTrigger, SystemAccent,
    };

    fn live_accent() -> SystemAccent {
        SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        }
    }

    fn live_raw() -> crate::active_border::AccentRaw {
        crate::active_border::AccentRaw {
            init_hr: 0,
            activate_hr: Some(0),
            query_hr: Some(0),
            accent: Some(live_accent()),
        }
    }

    fn resolve_with(
        shared: &mut crate::active_border::AccentCache,
        version: u64,
        calls: &mut u32,
        raw: crate::active_border::AccentRaw,
    ) -> (
        Option<SystemAccent>,
        Vec<crate::active_border::AccentQueryReport>,
        Vec<crate::ffi::AccentStage>,
    ) {
        let mut reports = Vec::new();
        let stages = std::cell::RefCell::new(Vec::new());
        let snapshot = super::refresh_shared_cache(
            shared,
            version,
            |_| {
                *calls += 1;
                raw
            },
            |stage| stages.borrow_mut().push(stage),
            |report| reports.push(*report),
        );
        (snapshot, reports, stages.into_inner())
    }

    #[test]
    fn one_shared_read_serves_border_and_pager_per_version() {
        // The owner resolves once per version: the border and the pager
        // consume the same snapshot with no second native call, in either
        // consumption order.
        let mut state = test_state();
        let mut calls = 0u32;
        let (border, border_reports, _) =
            resolve_with(&mut state.accent_shared, 7, &mut calls, live_raw());
        assert_eq!(calls, 1);
        assert_eq!(border, Some(live_accent()));
        assert_eq!(border_reports.len(), 1);
        let report = &border_reports[0];
        assert_eq!(report.surface, AccentQuerySurface::Shared);
        assert_eq!(report.role, AccentQueryRole::Owner);
        assert_eq!(report.trigger, AccentQueryTrigger::Startup);
        assert_eq!(report.version, 7);
        // The pager consumes the same version: a cache hit, no new resolve,
        // the identical snapshot, no second record.
        let (pager, pager_reports, pager_stages) =
            resolve_with(&mut state.accent_shared, 7, &mut calls, live_raw());
        assert_eq!(calls, 1, "same version must not re-resolve");
        assert_eq!(pager, border);
        assert!(pager_reports.is_empty());
        assert!(pager_stages.is_empty(), "hits fire no stages");
        // Both surfaces paint the same colour from that one snapshot.
        let themed = crate::active_border::ActiveBorderStyle {
            use_theme: true,
            ..crate::active_border::ActiveBorderStyle::default()
        };
        assert_eq!(
            crate::active_border::effective_color(&themed, border),
            crate::active_border::effective_color(&themed, pager)
        );
        assert_eq!(
            crate::pager::pager_active_fill(pager),
            crate::active_border::effective_color(&themed, border)
        );
    }

    #[test]
    fn shared_snapshot_is_order_independent() {
        // Pager-first consumption resolves once and the border reuses it:
        // the snapshot never depends on which surface observed the version
        // first.
        let mut state = test_state();
        let mut calls = 0u32;
        let (pager, _, _) = resolve_with(&mut state.accent_shared, 3, &mut calls, live_raw());
        let (border, reports, hit_stages) =
            resolve_with(&mut state.accent_shared, 3, &mut calls, live_raw());
        assert_eq!(calls, 1);
        assert_eq!(border, pager);
        assert!(reports.is_empty());
        assert!(hit_stages.is_empty(), "hits fire no stages");
    }

    #[test]
    fn mid_query_bump_records_pre_query_version() {
        // Actual mutation inside the query closure (not storytelling): a
        // broadcast bumps the live version while the resolve runs. The
        // helper captures the pre-query version the same way production
        // captures `accent_version()` before resolving, so the cache
        // records 7 even though the world moved to 8 mid-resolve - and the
        // next refresh at the live version re-resolves instead of losing it.
        use std::cell::{Cell, RefCell};
        let mut state = test_state();
        let mut calls = 0u32;
        let live = Cell::new(7u64);
        let stages: RefCell<Vec<crate::ffi::AccentStage>> = RefCell::new(Vec::new());
        let mut reports = Vec::new();
        let first = super::refresh_shared_cache(
            &mut state.accent_shared,
            live.get(),
            |_| {
                live.set(8);
                calls += 1;
                live_raw()
            },
            |stage| stages.borrow_mut().push(stage),
            |report| reports.push(*report),
        );
        assert_eq!(first, Some(live_accent()));
        assert_eq!(calls, 1);
        assert_eq!(reports.len(), 1);
        assert_eq!(
            state.accent_shared.last_version(),
            Some(7),
            "cache records the pre-query version despite the mid-query bump"
        );
        // The world is now at 8 while the cache holds 7: refreshing at the
        // live version must re-resolve.
        let (second, second_reports, _) =
            resolve_with(&mut state.accent_shared, live.get(), &mut calls, live_raw());
        assert_eq!(live.get(), 8);
        assert_eq!(calls, 2, "mid-query broadcast must re-resolve");
        assert_eq!(second, Some(live_accent()));
        assert_eq!(second_reports.len(), 1);
        assert_eq!(state.accent_shared.last_version(), Some(8));
    }

    #[test]
    fn version_move_mid_resolve_stays_pending() {
        // The pre-query version is recorded: a broadcast landing after the
        // resolve started (i.e. the cache holds the older version) forces
        // the next refresh to re-resolve instead of treating the new
        // version as seen.
        let mut state = test_state();
        let mut calls = 0u32;
        let (first, _, _) = resolve_with(&mut state.accent_shared, 7, &mut calls, live_raw());
        assert_eq!(first, Some(live_accent()));
        assert_eq!(calls, 1);
        // A broadcast bumped the world to 8 while the resolve above was in
        // flight: refreshing at 8 must re-resolve (the recorded 7 is stale).
        let (second, reports, _) =
            resolve_with(&mut state.accent_shared, 8, &mut calls, live_raw());
        assert_eq!(calls, 2, "mid-resolve broadcast must re-resolve");
        assert_eq!(second, Some(live_accent()));
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].trigger, AccentQueryTrigger::Broadcast);
        assert_eq!(reports[0].version, 8);
        // Steady state after the catch-up stays silent.
        let (_, steady, steady_stages) =
            resolve_with(&mut state.accent_shared, 8, &mut calls, live_raw());
        assert_eq!(calls, 2);
        assert!(steady.is_empty());
        assert!(steady_stages.is_empty(), "hits fire no stages");
    }

    #[test]
    fn shared_fallback_paths_stay_honest() {
        // Init failure, zero alpha, and opaque black resolve exactly as the
        // portable policy names them; the shared record never fabricates a
        // value for stages that never ran.
        let mut state = test_state();
        let mut calls = 0u32;
        let failed = crate::active_border::AccentRaw {
            init_hr: 0x8001_0106u32 as i32,
            activate_hr: None,
            query_hr: None,
            accent: None,
        };
        let (snapshot, reports, _stages) =
            resolve_with(&mut state.accent_shared, 1, &mut calls, failed);
        assert_eq!(snapshot, None);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].activate_hr, None);
        assert_eq!(reports[0].query_hr, None);
        assert_eq!(
            reports[0].fallback_reason,
            Some(crate::active_border::AccentFallbackReason::Unavailable)
        );
        // Zero alpha is invalid (not an accent) but still a completed
        // resolve at its version.
        let ghost = crate::active_border::AccentRaw {
            init_hr: 0,
            activate_hr: Some(0),
            query_hr: Some(0),
            accent: Some(SystemAccent {
                color: (0xaa, 0xbb, 0xcc),
                alpha: 0,
            }),
        };
        let (ghost_snapshot, ghost_reports, _) =
            resolve_with(&mut state.accent_shared, 2, &mut calls, ghost);
        assert_eq!(ghost_snapshot.map(|a| a.alpha), Some(0));
        assert_eq!(
            ghost_reports[0].outcome,
            crate::active_border::AccentQueryOutcome::Invalid
        );
        // Opaque black is a legitimate accent, never invalid.
        let black = crate::active_border::AccentRaw {
            init_hr: 0,
            activate_hr: Some(0),
            query_hr: Some(0),
            accent: Some(SystemAccent {
                color: (0, 0, 0),
                alpha: 0xFF,
            }),
        };
        let (black_snapshot, black_reports, _) =
            resolve_with(&mut state.accent_shared, 3, &mut calls, black);
        assert_eq!(
            black_snapshot.map(|a| a.color),
            Some((0, 0, 0)),
            "valid black survives the shared cache"
        );
        assert_eq!(
            black_reports[0].outcome,
            crate::active_border::AccentQueryOutcome::Accent
        );
        assert_eq!(black_reports[0].resolved, (0, 0, 0));
    }

    #[test]
    fn theme_off_border_falls_back_while_pager_follows_accent() {
        // One shared snapshot feeds both consumers: the pager always
        // follows an available accent while a theme-off border keeps its
        // configured colour. No second resolve, no divergence.
        let mut state = test_state();
        let mut calls = 0u32;
        let (snapshot, _, _) = resolve_with(&mut state.accent_shared, 5, &mut calls, live_raw());
        let explicit = crate::active_border::ActiveBorderStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: false,
            ..crate::active_border::ActiveBorderStyle::default()
        };
        assert_eq!(
            crate::active_border::effective_color(&explicit, snapshot),
            (0x11, 0x22, 0x33),
            "theme-off border keeps its configured colour"
        );
        assert!(
            !crate::active_border::effective_uses_accent(&explicit, snapshot),
            "theme-off border never claims the accent source"
        );
        assert_eq!(
            crate::pager::pager_active_fill(snapshot),
            (0xaa, 0xbb, 0xcc),
            "pager follows the same available accent"
        );
        assert_eq!(calls, 1);
    }

    #[test]
    fn stages_deliver_synchronously_inside_the_resolve() {
        // Crash-diagnostic discipline: each stage is delivered to the
        // observer before the native call it brackets, so a marker is
        // already logged when an abrupt exit lands mid-resolve. The
        // injected query asserts the `start` marker arrived before the
        // query itself completes; steady-state hits run no resolve and
        // deliver nothing.
        use crate::ffi::AccentStage;
        use std::cell::{Cell, RefCell};
        let mut state = test_state();
        let seen: RefCell<Vec<AccentStage>> = RefCell::new(Vec::new());
        let delivered_before_completion = Cell::new(false);
        let mut reports = Vec::new();
        let snapshot = super::refresh_shared_cache(
            &mut state.accent_shared,
            7,
            |trace| {
                let trace = trace.expect("actual resolve observes stages");
                trace(AccentStage::Start);
                // The observer already ran: the marker is delivered before
                // this query completes.
                delivered_before_completion.set(!seen.borrow().is_empty());
                for step in [AccentStage::Activate, AccentStage::Query, AccentStage::Done] {
                    trace(step);
                }
                live_raw()
            },
            |stage| seen.borrow_mut().push(stage),
            |report| reports.push(*report),
        );
        assert_eq!(snapshot, Some(live_accent()));
        assert!(
            delivered_before_completion.get(),
            "start marker must land before the resolve returns"
        );
        assert_eq!(
            seen.borrow().as_slice(),
            [
                AccentStage::Start,
                AccentStage::Activate,
                AccentStage::Query,
                AccentStage::Done,
            ]
        );
        assert_eq!(reports.len(), 1);
        // Steady hit: no resolve, no stages, no record.
        let steady_seen: RefCell<Vec<AccentStage>> = RefCell::new(Vec::new());
        let mut steady_reports = Vec::new();
        let steady = super::refresh_shared_cache(
            &mut state.accent_shared,
            7,
            |_| panic!("steady version must not resolve"),
            |stage| steady_seen.borrow_mut().push(stage),
            |report| steady_reports.push(*report),
        );
        assert_eq!(steady, Some(live_accent()));
        assert!(steady_seen.borrow().is_empty());
        assert!(steady_reports.is_empty());
    }

    #[test]
    fn lazy_source_never_touches_winrt_without_resolve() {
        // Offline lifetime ordering: constructing (and shutting down) the
        // fixture state performs no WinRT call; only an explicit injected
        // resolve populates the snapshot.
        let mut state = test_state();
        assert!(!state.accent_owner.is_initialized());
        assert_eq!(state.accent_shared.get(), None);
        assert_eq!(state.accent_shared.last_version(), None);
        state.accent_owner.shutdown();
        assert!(!state.accent_owner.is_initialized());
        let mut calls = 0u32;
        let (snapshot, _, _) = resolve_with(&mut state.accent_shared, 9, &mut calls, live_raw());
        assert_eq!(calls, 1);
        assert_eq!(snapshot, Some(live_accent()));
        assert!(
            !state.accent_owner.is_initialized(),
            "injected resolve touches no apartment"
        );
    }
}
