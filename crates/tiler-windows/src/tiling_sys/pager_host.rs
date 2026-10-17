#![forbid(unsafe_code)]
use super::*;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::pager::{
    OutputGeometry, OutputPager, PagerGate, TaskbarFacts, match_bars_to_outputs, pager_suppression,
    verify_output_scoped_select,
};
use crate::pager_icons::IconImage;
use crate::workspace::WindowKey;

/// Owner-creation retry when overlay registration fails: fixed 5s, forever,
/// so a failed registration retries promptly without busy-looping pumps.
const PAGER_CREATE_RETRY_SECS: u64 = 5;
/// Icon worker spawn retry: fixed 5s, forever. A fixed delay (no backoff)
/// recovers promptly once threads spawn again while never busy-looping
/// pumps; the pager simply stays icon-less meanwhile.
const PAGER_ICON_RETRY_SECS: u64 = 5;

/// Fullscreen covering shared by the visibility sync and the click-dispatch
/// poll: the raw suspend veto plus the managed-overlay exception, so both
/// enforce exactly the same fullscreen/game hiding.
fn pager_is_covering(state: &mut TileLoop, me: &ProcessIdentity, fulls: &[Rect]) -> bool {
    let read = suspend_read(state, me, fulls);
    crate::pager::pager_covering(
        read.veto.block,
        read.veto.reason == crate::workspace_owner::ForegroundVetoReason::ManagedOverlay,
    )
}

fn drop_pager_clicks(state: &TileLoop) -> usize {
    state
        .pager_clicks
        .lock()
        .map(|mut slot| slot.take().is_some() as usize)
        .unwrap_or(0)
}

fn pager_has_pending(state: &TileLoop) -> bool {
    state
        .pager_clicks
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false)
}

// Per-pump taskbar-pager sync for the tile loop: safety gate, fresh primary
// taskbar facts, per-output frames from live plus saved rects, and overlay
// present. Returns true when a click waits (the pump wakes for it; the tick
// dispatch drains it once). Proof owners never page: the caller skips this
// entirely on proof paths, and teardown drops the overlays.
pub(super) fn sync_taskbar_pager(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    log_path: &Path,
    now: Instant,
) -> bool {
    // A calibration broadcast hides every strip at once (inside the window
    // procedure); revalidate from fresh facts before showing again, so no
    // stale pixels survive display/DPI changes.
    if crate::pager_sys::take_stale_hide()
        && let Some(overlays) = state.pager_overlays.as_mut()
    {
        overlays.hide_all();
        state.pager_suppress_last = Some("stale-facts".to_owned());
    }
    // Fresh primary facts every pump (five reads, no enumeration): a tray
    // expansion, auto-hide slide, or Explorer disappearance is visible on
    // the very next pump, never up to 5s late. Explorer restarts and the
    // slow timer only retry this same read; layouts never use older facts.
    let primary = crate::pager_sys::read_primary_taskbar();
    if primary != state.pager_primary {
        let outcome = match (&state.pager_primary, &primary) {
            (None, Some(_)) => "appeared",
            (Some(_), None) => "gone",
            _ => "changed",
        };
        state.pager_primary = primary;
        state.pager_suppress_last = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "pager-taskbar",
                "outcome": outcome,
                "proven": state.pager_primary.as_ref().is_some_and(|bar| bar.tray_left.is_some()),
                "visible": state.pager_primary.as_ref().is_some_and(|bar| bar.visible),
                "autohide": state.pager_primary.as_ref().is_some_and(|bar| bar.autohide),
            }),
        );
    }
    // Slow secondary enumeration (refusal logging only, never positions):
    // the native rediscovery broadcast or a monitor-topology change
    // revalidates; no timer arm (the owner wait is event-driven).
    let areas_fp = areas_fingerprint(areas);
    if crate::pager_sys::take_rediscover() || state.pager_areas_fp != areas_fp {
        let secondaries = crate::pager_sys::discover_secondaries();
        if secondaries.len() != state.pager_secondaries.len() {
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "pager-taskbar",
                    "outcome": "secondaries",
                    "bars": secondaries.len(),
                }),
            );
        }
        state.pager_secondaries = secondaries;
        state.pager_areas_fp = areas_fp;
    }
    let gate = PagerGate {
        enabled: state.taskbar_pager,
        suspended: pager_is_covering(state, me, fulls),
        elevated: foreground_elevated(me),
        unknown: areas.is_empty() || state.pager_primary.is_none(),
    };
    if let Some(reason) = pager_suppression(gate) {
        // Suppression drops the staged click: a click rendered under an old
        // gate must never replay on a later safe turn.
        let dropped = drop_pager_clicks(state);
        // Suppression also clears queued icon work and stops future
        // requests: an already-started lookup may finish, but its retired
        // generation prevents accepting the result after suppression.
        if let Some(service) = state.pager_icon_service.as_ref() {
            service.clear();
        }
        if let Some(overlays) = state.pager_overlays.as_mut() {
            overlays.hide_all();
        }
        state.pager_served.clear();
        if state.pager_suppress_last.as_deref() != Some(reason) {
            state.pager_suppress_last = Some(reason.to_owned());
            let mut line = serde_json::json!({"event": "pager-suppress", "outcome": reason});
            if dropped > 0 {
                line["dropped"] = serde_json::Value::from(dropped);
            }
            log_json_at(log_path, line);
        }
        return false;
    }
    if state.pager_suppress_last.is_some() {
        state.pager_suppress_last = None;
        log_json_at(
            log_path,
            serde_json::json!({"event": "pager-suppress", "outcome": "show"}),
        );
    }
    if state.pager_overlays.is_none() {
        if state.pager_retry_at.is_some_and(|at| now < at) {
            return false;
        }
        match crate::pager_sys::PagerOverlays::new(Some(log_path.to_owned()), &state.pager_clicks) {
            Ok(overlays) => {
                state.pager_overlays = Some(overlays);
                state.pager_retry_at = None;
            }
            Err(error) => {
                state.pager_retry_at = Some(now + Duration::from_secs(PAGER_CREATE_RETRY_SECS));
                log_json_at(
                    log_path,
                    serde_json::json!({"event": "pager-window", "outcome": "owner-failed", "cause": error.to_string()}),
                );
                return false;
            }
        }
    }
    let geos: Vec<OutputGeometry> = areas
        .iter()
        .map(|area| OutputGeometry {
            device: area.device.clone(),
            full: area.full,
            work: area.work,
        })
        .collect();
    let bars: Vec<TaskbarFacts> = state
        .pager_primary
        .iter()
        .cloned()
        .chain(state.pager_secondaries.iter().cloned())
        .collect();
    let mut pagers: Vec<OutputPager> = Vec::new();
    // Artwork snapshot for this pump: prune the cache and drain finished
    // worker lookups first, then frame from cache hits. Paint never touches
    // foreign windows or waits.
    let icons = drain_icon_cache(state, now, log_path);
    for (geo_index, bar_index) in match_bars_to_outputs(&geos, &bars) {
        let area = &areas[geo_index];
        let bar: TaskbarFacts = bars[bar_index];
        match crate::pager::build_output_pager(
            &state.workspaces,
            &state.pager_live,
            &state.member_tokens,
            &state.member_rects,
            &state.float_rects,
            &icons,
            &area.device,
            area.full,
            &bar,
        ) {
            Ok(pager) => {
                state.pager_refusals.remove(&area.device);
                pagers.push(pager);
            }
            Err(refusal) => {
                let reason = refusal.as_str().to_owned();
                if state.pager_refusals.get(&area.device) != Some(&reason) {
                    state
                        .pager_refusals
                        .insert(area.device.clone(), reason.clone());
                    log_json_at(
                        log_path,
                        serde_json::json!({
                            "event": "pager-refused",
                            "output": geo_index,
                            "reason": reason,
                        }),
                    );
                }
            }
        }
    }
    // Outputs with no matched bar refuse loudly once, never guessed.
    for (geo_index, area) in areas.iter().enumerate() {
        let matched = match_bars_to_outputs(&geos, &bars)
            .iter()
            .any(|(geo, _)| *geo == geo_index);
        if !matched && state.pager_refusals.get(&area.device) != Some(&"no-taskbar".to_owned()) {
            state
                .pager_refusals
                .insert(area.device.clone(), "no-taskbar".to_owned());
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "pager-refused",
                    "output": geo_index,
                    "reason": "no-taskbar",
                }),
            );
        }
    }
    state.pager_served = pagers.iter().map(|pager| pager.output.clone()).collect();
    // Queue lookups only for served miniatures that can actually draw an
    // icon: artwork nobody can see is never loaded.
    request_drawable_icons(state, &pagers, now);
    if let Some(overlays) = state.pager_overlays.as_mut() {
        overlays.sync(&pagers, now);
        pager_has_pending(state)
    } else {
        false
    }
}

/// Spawn (or spawn-retry) the single icon worker on the safe path. A failed
/// spawn logs once per retry and retries on a fixed cadence, never busy
/// every pump.
fn ensure_icon_service(state: &mut TileLoop, log_path: &Path, now: Instant) {
    if state.pager_icon_service.is_some() {
        return;
    }
    if state.pager_icon_retry_at.is_some_and(|at| now < at) {
        return;
    }
    match crate::pager_icons_sys::PagerIconService::try_new() {
        Some(service) => {
            state.pager_icon_service = Some(service);
            state.pager_icon_retry_at = None;
        }
        None => {
            state.pager_icon_retry_at = Some(now + Duration::from_secs(PAGER_ICON_RETRY_SECS));
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "pager-icons",
                    "outcome": "worker-spawn-failed",
                    "retry_secs": PAGER_ICON_RETRY_SECS,
                }),
            );
        }
    }
}

/// Pump the icon cache: prune to live membership, drain finished worker
/// lookups into the cache with one outcome line per active batch,
/// and return current artwork hits for the frame. Never queries or waits
/// on foreign windows.
fn drain_icon_cache(
    state: &mut TileLoop,
    now: Instant,
    log_path: &Path,
) -> HashMap<WindowKey, Arc<IconImage>> {
    let mut keys: Vec<WindowKey> = Vec::new();
    for output in state.workspaces.output_keys() {
        for id in state.workspaces.workspace_ids(&output) {
            keys.extend(state.workspaces.workspace_members(&output, &id));
        }
    }
    let live: HashSet<WindowKey> = keys.iter().cloned().collect();
    state.pager_icon_cache.prune(&live);
    ensure_icon_service(state, log_path, now);
    if let Some(service) = state.pager_icon_service.as_ref() {
        let mut hits = 0u64;
        let mut misses = 0u64;
        let mut window = 0u64;
        let mut class = 0u64;
        let mut exe = 0u64;
        let mut refused = 0u64;
        for result in service.drain() {
            if result.image.is_some() {
                hits += 1;
                match result.source {
                    crate::pager_icons::IconSource::Window => window += 1,
                    crate::pager_icons::IconSource::Class => class += 1,
                    crate::pager_icons::IconSource::Exe => exe += 1,
                    _ => {}
                }
            } else {
                misses += 1;
                if result.source == crate::pager_icons::IconSource::Refused {
                    refused += 1;
                }
            }
            // Late results for departed keys never enter the cache.
            if live.contains(&result.key) {
                state.pager_icon_cache.insert(result.key, result.image, now);
            }
        }
        if hits + misses > 0 {
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "pager-icons",
                    "outcome": "results",
                    "hits": hits,
                    "misses": misses,
                    "window": window,
                    "class": class,
                    "exe": exe,
                    "refused": refused,
                }),
            );
        }
    }
    let mut artwork = HashMap::new();
    for key in &keys {
        if let Some(image) = state.pager_icon_cache.get(key) {
            artwork.insert(key.clone(), image);
        }
    }
    artwork
}

/// Queue lookups only for served miniatures that fit a legible icon, and
/// prune queued requests that are no longer drawable. The cache still
/// prunes against all live keys; artwork nobody can see is simply never
/// loaded.
fn request_drawable_icons(state: &mut TileLoop, pagers: &[OutputPager], now: Instant) {
    let mut drawable = HashSet::new();
    for pager in pagers {
        for cell in &pager.cells {
            for (mini, key) in cell.minis.iter().zip(cell.mini_keys.iter()) {
                if crate::pager_icons::icon_rect_for_mini(*mini, pager.dpi).is_some() {
                    drawable.insert(key.clone());
                }
            }
        }
    }
    let Some(service) = state.pager_icon_service.as_ref() else {
        return;
    };
    service.prune_pending(&drawable);
    let requests = state
        .pager_icon_cache
        .requests(drawable.iter().cloned(), now);
    service.request(requests);
}

fn areas_fingerprint(areas: &[MonitorArea]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |word: u64| {
        hash ^= word;
        hash = hash.wrapping_mul(0x100000001b3);
    };
    for area in areas {
        for byte in area.device.bytes() {
            mix(u64::from(byte));
        }
        for value in [
            area.full.x,
            area.full.y,
            area.full.w,
            area.full.h,
            area.work.x,
            area.work.y,
            area.work.w,
            area.work.h,
        ] {
            mix(value as u64);
        }
    }
    hash
}

/// Dispatch one output-scoped pager click through the existing select
/// resolver. Called beside the tray-select poll on non-gesture turns
/// (suspended and active alike); every refusal consumes once with an opaque
/// log line and no native effect. The poll re-verifies the exact visibility
/// gate (opt-out, the same fullscreen covering, elevation, unknown facts):
/// a click queued under an old gate never dispatches after suppression, and
/// clicks naming an output with no currently served strip refuse as
/// not-served. Unlike the tray menu fence this never requires the output to
/// be focused.
pub(super) fn poll_pager_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    let click = state
        .pager_clicks
        .lock()
        .map(|mut slot| slot.take())
        .unwrap_or_default();
    let Some(request) = click else {
        return;
    };
    // The 1-based ordinal is logging-only (u8 log lane): clamp the bound so
    // a corrupt index can never overflow the log field or retarget.
    let index = request.index.min(255) as u8;
    state.tick += 1;
    let tick = state.tick;
    let start = Instant::now();
    let gate = PagerGate {
        enabled: state.taskbar_pager,
        suspended: pager_is_covering(state, me, fulls),
        elevated: foreground_elevated(me),
        unknown: areas.is_empty(),
    };
    if let Some(reason) = pager_suppression(gate) {
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, "select", index, "pager", reason),
        );
        return;
    }
    if !state.pager_served.contains(&request.output) {
        let mut line = workspace_log(state, tick, "select", index, "pager", "refused-not-served");
        line["output"] = serde_json::Value::from(state.workspaces.output_token(&request.output));
        log_json_at(&state.log_path, line);
        return;
    }
    // Output-scoped fence: the rendered output must still be known and
    // the rendered workspace still live on it. Focus is irrelevant: a
    // click on an unfocused output's strip still selects there. The
    // 1-based ordinal is logging only, never a positional fallback.
    if !verify_output_scoped_select(&request, &state.workspaces) {
        let mut line = workspace_log(state, tick, "select", index, "pager", "refused-stale-scope");
        line["output"] = serde_json::Value::from(state.workspaces.output_token(&request.output));
        line["workspace"] = serde_json::Value::from(
            state
                .workspaces
                .workspace_token(&request.output, &request.workspace),
        );
        log_json_at(&state.log_path, line);
        return;
    }
    dispatch_verified_select(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        &request.output,
        &request.workspace,
        index,
        tick,
        start,
        "pager",
    );
}

/// Shared select body for the tray menu ("tray") and the taskbar pager
/// ("pager"): fresh observation, departure memory, and the existing
/// `workspace_do_select` resolver with bounded opaque logging. The caller
/// owns the scope fence; this never guesses a target.
#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_verified_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    output: &str,
    target: &str,
    index: u8,
    tick: u64,
    start: Instant,
    edge: &'static str,
) {
    state.workspaces.ensure_output(output);
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, "select", index, edge, "observation-failed"),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    let ctx = ActionCtx {
        correlation: format!("act-{tick}"),
        tick,
        queued_at: start,
        start,
    };
    // REQ-WS-09 departure memory, same as the tray path: the live
    // foreground resolves through the shared `snap_origins` map (never a
    // carried HWND).
    let foreground_hwnd = crate::ffi::foreground_window();
    if let Some(source_id) = state.workspaces.active_id(output)
        && let Some(origin) = state.snap_origins.get(&foreground_hwnd).cloned()
        && !origin.token.is_empty()
        && let Some(token) = remember_select_departure(
            state,
            &origin,
            output,
            &source_id,
            foreground_hwnd,
            &observed,
            &retained,
        )
    {
        log_json_at(
            &state.log_path.clone(),
            serde_json::json!({"event":"select-departure","correlation":ctx.correlation,"window":token}),
        );
    }
    let effect = workspace_do_select(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        &mut observed,
        output,
        target,
        &ctx,
        None,
        false,
        false,
    );
    log_json_at(
        &state.log_path,
        workspace_log(state, tick, "select", index, edge, effect.outcome),
    );
    log_workspace_action(
        state,
        &state.log_path.clone(),
        &ActionLine {
            ctx: &ctx,
            op: "select",
            index,
            edge,
            outcome: effect.outcome,
            focus: effect.focus,
            timings: &ActionTimings {
                transition_ms: effect.transition_ms,
                observation_ms: effect.observation_ms,
                source_plan_ms: 0,
                target_plan_ms: effect.plan_ms,
                source_geometry_ms: 0,
                target_geometry_ms: effect.geometry_ms,
                hide_ms: effect.hide_ms,
                reveal_ms: effect.reveal_ms,
                focus_ms: effect.focus_ms,
            },
            source_workspace: &effect.source_workspace,
            target_workspace: &effect.target_workspace,
            source: None,
            target: effect.geometry.as_ref(),
        },
    );
}

#[cfg(test)]
mod pager_host_tests {
    use super::rmax03_adapter_tests::test_state;
    use super::{
        MonitorArea, ObservedWindow, RetainedRow, clear_pending_seams, inject_pending_elevated,
        inject_pending_observe, inject_pending_suspend, poll_pager_select,
    };
    use crate::tray_pager::SelectScope;

    fn pager_me() -> crate::model::ProcessIdentity {
        crate::model::ProcessIdentity {
            pid: 5009,
            process_creation: "creation-pager-me".to_owned(),
            user_sid: "S-1-pager".to_owned(),
            session_id: 7,
            exe_path: "C:\\test\\app.exe".to_owned(),
        }
    }

    fn pager_areas(output: &str) -> (Vec<MonitorArea>, Vec<tiler_core::geometry::Rect>) {
        let full = tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        (
            vec![MonitorArea {
                device: output.to_owned(),
                work: full,
                full,
            }],
            vec![full],
        )
    }

    fn pager_rig() -> (
        super::TileLoop,
        crate::model::ProcessIdentity,
        crate::storage::LedgerStore,
        std::path::PathBuf,
        String,
        String,
        String,
    ) {
        clear_pending_seams();
        let mut state = test_state();
        let me = pager_me();
        let tmp = std::env::temp_dir().join(format!(
            "tiler-pager-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
        let output = "mon-pager".to_owned();
        state.workspaces.ensure_output(&output);
        let ids = state.workspaces.workspace_ids(&output);
        assert!(ids.len() >= 2, "active plus a distinct target");
        // Hermetic observation insurance: any accidental dispatch below
        // must still touch no native window. Refused clicks never observe.
        inject_pending_observe(Vec::new(), Vec::new(), false);
        (
            state,
            me,
            store,
            tmp,
            output,
            ids[0].clone(),
            ids[1].clone(),
        )
    }

    fn push_click(state: &mut super::TileLoop, output: &str, workspace: &str) {
        *state
            .pager_clicks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) =
            Some(SelectScope::new(output, workspace, 1));
    }

    fn pending_clicks(state: &super::TileLoop) -> usize {
        state
            .pager_clicks
            .lock()
            .map(|slot| usize::from(slot.is_some()))
            .unwrap_or(usize::MAX)
    }

    fn open_gate() {
        inject_pending_suspend(Some(false));
        inject_pending_elevated(Some(false));
    }

    #[test]
    fn opt_out_drops_queued_clicks_without_dispatch() {
        let (mut state, me, store, dir, output, active, target) = pager_rig();
        let (areas, fulls) = pager_areas(&output);
        state.taskbar_pager = false;
        // The click names a distinct workspace: any dispatch would move the
        // active view, so an unchanged view proves no dispatch.
        push_click(&mut state, &output, &target);
        open_gate();
        poll_pager_select(&mut state, &me, &store, &dir, &fulls, &areas);
        // Consumed once, never dispatched: no replay later.
        assert_eq!(pending_clicks(&state), 0);
        assert_eq!(
            state.workspaces.active_id(&output).as_deref(),
            Some(active.as_str())
        );
        clear_pending_seams();
    }

    #[test]
    fn suspended_and_elevated_gates_drop_like_visibility() {
        for (suspended, elevated) in [(Some(true), None), (None, Some(true))] {
            let (mut state, me, store, dir, output, active, target) = pager_rig();
            let (areas, fulls) = pager_areas(&output);
            push_click(&mut state, &output, &target);
            inject_pending_suspend(suspended);
            inject_pending_elevated(elevated);
            poll_pager_select(&mut state, &me, &store, &dir, &fulls, &areas);
            assert_eq!(pending_clicks(&state), 0);
            assert_eq!(
                state.workspaces.active_id(&output).as_deref(),
                Some(active.as_str())
            );
            clear_pending_seams();
        }
    }

    #[test]
    fn unknown_areas_drop_without_dispatch() {
        let (mut state, me, store, dir, output, active, target) = pager_rig();
        let (_, fulls) = pager_areas(&output);
        push_click(&mut state, &output, &target);
        open_gate();
        poll_pager_select(&mut state, &me, &store, &dir, &fulls, &[]);
        assert_eq!(pending_clicks(&state), 0);
        assert_eq!(
            state.workspaces.active_id(&output).as_deref(),
            Some(active.as_str())
        );
        clear_pending_seams();
    }

    #[test]
    fn unserved_output_refuses_without_dispatch() {
        let (mut state, me, store, dir, output, active, target) = pager_rig();
        let (areas, fulls) = pager_areas(&output);
        // Gate open and scope live, but no strip is currently served for the
        // output (removed/hidden since the click rendered).
        assert!(state.pager_served.is_empty());
        push_click(&mut state, &output, &target);
        open_gate();
        poll_pager_select(&mut state, &me, &store, &dir, &fulls, &areas);
        assert_eq!(pending_clicks(&state), 0);
        assert_eq!(
            state.workspaces.active_id(&output).as_deref(),
            Some(active.as_str())
        );
        clear_pending_seams();
    }

    #[test]
    fn latest_click_overwrites_stale_click() {
        let (mut state, _, _, _, output, _, target) = pager_rig();
        // Stage two clicks without draining: only the newest survives, so a
        // stale cell never replays after a newer one.
        push_click(&mut state, &output, "ws-stale");
        push_click(&mut state, &output, &target);
        assert_eq!(pending_clicks(&state), 1);
        let staged = state
            .pager_clicks
            .lock()
            .map(|slot| slot.clone())
            .unwrap_or_default();
        assert_eq!(
            staged,
            Some(SelectScope::new(&output, &target, 1)),
            "only the latest click survives"
        );
        clear_pending_seams();
    }

    #[test]
    fn icon_spawn_retry_holds_fixed_delay() {
        clear_pending_seams();
        let mut state = test_state();
        let log = std::path::PathBuf::from("NUL");
        let now = std::time::Instant::now();
        // Future gate: no spawn attempt, service stays absent.
        state.pager_icon_retry_at = Some(now + std::time::Duration::from_secs(3600));
        super::ensure_icon_service(&mut state, &log, now);
        assert!(state.pager_icon_service.is_none());
        // Due gate: spawns the worker thread only (no window operations);
        // the gate clears on success.
        state.pager_icon_retry_at = Some(now - std::time::Duration::from_secs(1));
        super::ensure_icon_service(&mut state, &log, now);
        assert!(state.pager_icon_service.is_some());
        assert!(state.pager_icon_retry_at.is_none());
        clear_pending_seams();
    }

    #[test]
    fn observe_records_live_frames_for_managed_members_only() {
        use tiler_core::geometry::Rect;
        clear_pending_seams();
        let mut state = test_state();
        let output = "mon-live".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        let member = crate::workspace::WindowKey {
            hwnd: 701,
            pid: 7,
            creation: "creation-live".to_owned(),
        };
        assert!(
            state
                .workspaces
                .assign(member.clone(), &output, &active, false)
        );
        state
            .member_tokens
            .insert(member.clone(), "tok-live".to_owned());
        let live_frame = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        let overlay_frame = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let overlay_key = crate::workspace::WindowKey {
            hwnd: 702,
            pid: 8,
            creation: "creation-overlay".to_owned(),
        };
        assert!(
            state
                .workspaces
                .assign(overlay_key.clone(), &output, &active, false)
        );
        state
            .member_tokens
            .insert(overlay_key.clone(), "tok-overlay".to_owned());
        let mk_observed = |hwnd: u64, token: &str, pid: u32, creation: &str| ObservedWindow {
            hwnd,
            token: token.to_owned(),
            outer: live_frame,
            visible: live_frame,
            insets: crate::tiling::FrameInsets {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            identity: crate::tiling::ObservedTarget {
                hwnd,
                pid,
                process_creation: creation.to_owned(),
                exe_path: "C:\\test\\app.exe".to_owned(),
                user_sid: "S-1-5-test".to_owned(),
                session_id: 1,
                tag: String::new(),
            },
            facts: crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: false,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            },
        };
        // A foreign window observes fine but is never a member: excluded.
        let foreign = mk_observed(703, "tok-foreign", 9, "creation-foreign");
        inject_pending_observe(
            vec![mk_observed(701, "tok-live", 7, "creation-live"), foreign],
            vec![RetainedRow {
                key: overlay_key.clone(),
                token: "tok-overlay".to_owned(),
                rect: Some(overlay_frame),
                maximized: false,
                fullscreen: true,
                facts: None,
            }],
            false,
        );
        let me = pager_me();
        let fulls = vec![live_frame];
        let mut skipped = Vec::new();
        let mut retained = Vec::new();
        let observed = state
            .observe(&me, &fulls, &mut skipped, &mut retained)
            .expect("injected observation");
        assert_eq!(observed.len(), 2);
        // Visible member and retained overlay frames record; the foreign
        // window never enters the pager snapshot.
        assert_eq!(state.pager_live.get(&member), Some(&live_frame));
        assert_eq!(state.pager_live.get(&overlay_key), Some(&overlay_frame));
        assert_eq!(state.pager_live.len(), 2);
        clear_pending_seams();
    }
}
