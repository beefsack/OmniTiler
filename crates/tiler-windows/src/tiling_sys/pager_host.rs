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

/// Covering verdict over one native foreground capture: the bool matches the
/// previous fullscreen/game hiding exactly (raw suspend veto plus the
/// managed-overlay exception), while the returned HWND is that same capture
/// for diagnostic attribution (never a second read). `captured` is
/// false only when a test seam short-circuits the suspend read; the HWND is
/// then 0 and carries no identity.
fn pager_covering_detail(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
) -> (bool, u64, bool, super::settings::ForegroundRead) {
    let covering = |read: &super::settings::ForegroundRead| {
        crate::pager::pager_covering(
            read.veto.block,
            read.veto.reason == crate::workspace_owner::ForegroundVetoReason::ManagedOverlay,
        )
    };
    #[cfg(test)]
    if INJECTED_SUSPEND.with(|slot| slot.get()).is_some() {
        let read = suspend_read(state, me, fulls);
        return (covering(&read), 0, false, read);
    }
    let foreground = crate::ffi::foreground_window();
    let read = suspend_read_inner(state, me, fulls, foreground);
    (covering(&read), foreground, true, read)
}

/// Diagnostic-only foreground surface label. Verified desktop-handle and
/// known shell-class matches are reported separately by the caller; XAML
/// shell-island and task-view classes without exe verification are an
/// explicitly heuristic guess. Never an exemption: gates never read this.
fn pager_fg_surface(class: Option<&str>, desktop: bool) -> &'static str {
    if desktop {
        return "desktop-handle";
    }
    match class {
        None => "class-unreadable",
        Some(name) if is_shell_class(name) || crate::product_hide::shell_class_excluded(name) => {
            "known-shell-class"
        }
        Some(
            "MultitaskingViewFrame"
            | "TaskSwitcherWnd"
            | "NotifyIconOverflowWindow"
            | "Shell_Flyout"
            | "Windows.UI.Core.CoreWindow"
            | "XamlExplorerHostIslandWindow",
        ) => "possible-shell-surface-heuristic",
        Some(_) => "ordinary",
    }
}

/// Diagnostic-only foreground identity for one capture: the safe window
/// class name (or `"unknown"`) plus the bounded surface label. Uncaptured
/// reads report `skipped`; a captured null foreground reports
/// `no-foreground`. The desktop-handle verdict is measured only on the
/// covering capture by the caller; the elevation side never claims it.
/// Never titles, paths, PIDs, or error strings.
fn pager_fg_identity(captured: bool, hwnd: u64, desktop: bool) -> (String, &'static str) {
    if !captured {
        return ("unknown".to_owned(), "skipped");
    }
    if hwnd == 0 {
        return ("unknown".to_owned(), "no-foreground");
    }
    let class = live_fg_class(hwnd);
    let surface = pager_fg_surface(class.as_deref(), desktop);
    (class.unwrap_or_else(|| "unknown".to_owned()), surface)
}

/// One pump's pager-suppression facts: the gate inputs plus the honest
/// foreground sub-outcomes behind them. The gate bools preserve the existing
/// suppression order exactly; every other field is additive diagnostics.
struct PagerSuppressFacts {
    enabled: bool,
    suspended: bool,
    elevated: bool,
    unknown: bool,
    reason: Option<&'static str>,
    elev_detail: &'static str,
    elev_open: &'static str,
    elev_integrity_read: &'static str,
    elev_integrity: &'static str,
    cover: &'static str,
    cover_desktop: bool,
    cover_visible: bool,
    cover_readable: bool,
    elev_class: String,
    elev_surface: &'static str,
    cover_class: String,
    cover_surface: &'static str,
    /// Handle-value equality of the two sequential foreground captures, not
    /// an atomic same-snapshot guarantee: the covering and elevation reads
    /// run back to back, and the class reads happen later still, so `true`
    /// proves no coherency, lifetime, or security fact. Cover and elevation
    /// attribution stay separate regardless.
    handles_match: bool,
}

/// Suppression facts for one pump: the covering read and the elevation read
/// each capture the foreground once (the existing reads, no new polling).
/// `handles_match` reports only whether the two sequential captures held
/// the same handle value; differing reads never imply a coherent identity,
/// and the elevation side makes no desktop claim from its own read (the
/// measured `cover_desktop` stands alone on the covering capture).
fn pager_suppress_facts(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    enabled: bool,
    unknown: bool,
) -> PagerSuppressFacts {
    let (suspended, cover_hw, cover_captured, cover_read) = pager_covering_detail(state, me, fulls);
    let elev = foreground_pager_elevated_detailed(me);
    let reason = pager_suppression(PagerGate {
        enabled,
        suspended,
        elevated: elev.elevated,
        unknown,
    });
    let handles_match =
        cover_captured && elev.captured && cover_hw != 0 && cover_hw == elev.foreground;
    let (cover_class, cover_surface) =
        pager_fg_identity(cover_captured, cover_hw, cover_read.desktop);
    let (elev_class, elev_surface) = pager_fg_identity(elev.captured, elev.foreground, false);
    PagerSuppressFacts {
        enabled,
        suspended,
        elevated: elev.elevated,
        unknown,
        reason,
        elev_detail: elev.detail,
        elev_open: elev.open,
        elev_integrity_read: elev.integrity_read,
        elev_integrity: elev.integrity,
        cover: cover_read.veto.reason.as_str(),
        cover_desktop: cover_read.desktop,
        cover_visible: cover_read.visible,
        cover_readable: cover_read.dwm_readable,
        elev_class,
        elev_surface,
        cover_class,
        cover_surface,
        handles_match,
    }
}

/// Full change signature over every logged suppress field: identical repeats
/// emit nothing, while any gate-fact, foreground-class, or surface change
/// re-logs even under an unchanged high-level outcome.
fn pager_suppress_signature(facts: &PagerSuppressFacts) -> String {
    format!(
        "{}|en={} sus={} elev={} unk={}|{}|{}|{}|{}|{}|desk={} vis={} read={}|{}|{}|{}|{}|handles={}",
        facts.reason.unwrap_or("show"),
        facts.enabled,
        facts.suspended,
        facts.elevated,
        facts.unknown,
        facts.elev_detail,
        facts.elev_open,
        facts.elev_integrity_read,
        facts.elev_integrity,
        facts.cover,
        facts.cover_desktop,
        facts.cover_visible,
        facts.cover_readable,
        facts.elev_class,
        facts.elev_surface,
        facts.cover_class,
        facts.cover_surface,
        facts.handles_match,
    )
}

/// Compatibility suppress-state update with the exact legacy semantics:
/// a suppression stores its reason, recovery clears. Returns true when the
/// caller must log. Operates only on `pager_suppress_last`, which the
/// maintenance scheduler reads; the diagnostic signature lives separately in
/// `pager_gate_last` and is never reset here.
fn note_pager_suppress(last: &mut Option<String>, reason: Option<&str>) -> bool {
    match reason {
        Some(reason) => {
            if last.as_deref() == Some(reason) {
                return false;
            }
            *last = Some(reason.to_owned());
            true
        }
        None => {
            if last.is_none() {
                return false;
            }
            *last = None;
            true
        }
    }
}

/// Normal-level diagnostic transition over the full gate signature,
/// independent of the compatibility `pager-suppress` state: logs one
/// `pager-gate` event per signature change, including the first baseline,
/// recovery, and allowed-to-allowed fact changes. Identical repeats stay
/// silent. The outcome is the suppression reason, or `allowed` for a passing
/// gate (gate pass, not native rendering).
fn emit_pager_gate(state: &mut TileLoop, facts: &PagerSuppressFacts, log_path: &Path) {
    let signature = pager_suppress_signature(facts);
    if state.pager_gate_last.as_deref() == Some(signature.as_str()) {
        return;
    }
    state.pager_gate_last = Some(signature);
    let mut line = serde_json::json!({
        "event": "pager-gate",
        "outcome": facts.reason.unwrap_or("allowed"),
    });
    pager_suppress_diag(&mut line, facts);
    log_json_at(log_path, line);
}

/// Additive diagnostic fields for a suppress log line. Bounded vocabulary
/// only: gate bools, honest elevation/cover sub-outcomes, the safe window
/// class names, and bounded surface labels. Never HWNDs, PIDs, paths,
/// titles, or error strings.
fn pager_suppress_diag(line: &mut serde_json::Value, facts: &PagerSuppressFacts) {
    line["enabled"] = serde_json::Value::from(facts.enabled);
    line["suspended"] = serde_json::Value::from(facts.suspended);
    line["elevated"] = serde_json::Value::from(facts.elevated);
    line["unknown"] = serde_json::Value::from(facts.unknown);
    line["elev_detail"] = serde_json::Value::from(facts.elev_detail);
    line["elev_open"] = serde_json::Value::from(facts.elev_open);
    line["elev_integrity_read"] = serde_json::Value::from(facts.elev_integrity_read);
    line["elev_integrity"] = serde_json::Value::from(facts.elev_integrity);
    line["cover"] = serde_json::Value::from(facts.cover);
    line["cover_desktop"] = serde_json::Value::from(facts.cover_desktop);
    line["cover_visible"] = serde_json::Value::from(facts.cover_visible);
    line["cover_readable"] = serde_json::Value::from(facts.cover_readable);
    line["fg_class"] = serde_json::Value::from(facts.elev_class.as_str());
    line["fg_surface"] = serde_json::Value::from(facts.elev_surface);
    line["cover_class"] = serde_json::Value::from(facts.cover_class.as_str());
    line["cover_surface"] = serde_json::Value::from(facts.cover_surface);
    line["fg_handles_match"] = serde_json::Value::from(facts.handles_match);
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

/// Pure secondary-enumeration gate: a stale-anchor flag (validated
/// taskbar-subtree edge, rediscovery broadcast, or monitor-topology change
/// folded into `dirty`) or a changed topology fingerprint revalidates, but
/// never while suspended: enumeration and its filter work defer with the
/// flag retained for resume, so gaming owns no taskbar work. The gate never
/// inspects the cached census, so empty, unproven, and proven-but-churned
/// anchors all converge on genuine Explorer change; healthy idle with no
/// Explorer change never enumerates. No timer: rebuild traffic itself
/// (child create/show/hide/location edges) drives convergence.
fn secondary_revalidate_due(dirty: bool, areas_fp: u64, last_fp: u64, suspended: bool) -> bool {
    (dirty || areas_fp != last_fp) && !suspended
}

/// Taskbar bar classes terminating the subtree walk: a queued Explorer
/// window belongs to taskbar UI only when itself or an ancestor carries one
/// of these. Ordinary File Explorer windows (`CabinetWClass` and kin) never
/// match, however hot their traffic gets.
const TASKBAR_BAR_CLASSES: [&str; 2] = ["Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

/// Max ancestor hops validated per taskbar event: taskbar descendants nest
/// shallowly (bar, child, sub-child); anything deeper is not taskbar UI.
const TASKBAR_SUBTREE_WALK_CAP: usize = 8;

/// Pure taskbar-subtree check over live reads: true when `hwnd` itself or a
/// bounded ancestor chain carries a taskbar bar class. Fail conservatively
/// to stale (true) only when an unreadable class or destroyed handle
/// prevents proving the window unrelated; a fully readable non-matching
/// chain is unrelated (false). Reads run on the owner drain, never in the
/// hook callback.
pub(super) fn taskbar_subtree_hit(hwnd: u64) -> bool {
    let mut cursor = hwnd;
    for _ in 0..TASKBAR_SUBTREE_WALK_CAP {
        if cursor == 0 {
            return false;
        }
        match live_fg_class(cursor) {
            Some(class) if TASKBAR_BAR_CLASSES.contains(&class.as_str()) => return true,
            Some(_) => {}
            None => return true,
        }
        cursor = live_parent_of(cursor);
    }
    false
}

/// Taskbar-scope hook ranges: the same narrow object bands as the global
/// hooks (create/destroy/show/hide plus explicit location-change), installed
/// pid-scoped to the live Explorer identity. The OS delivers only that
/// process here, so game and app windows never reach the taskbar queue.
pub(super) fn taskbar_hook_ranges() -> [(u32, u32); 2] {
    [
        (
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_SHOW
                .max(EVENT_OBJECT_DESTROY)
                .max(EVENT_OBJECT_HIDE),
        ),
        (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
    ]
}

/// Install the pid-scoped taskbar hooks for one Explorer identity; empty
/// when there is no identity (Explorer gone) or a band refuses. A partial
/// set never stands: the guards drop it on return (unhook on drop) so the
/// next broadcast retries a coherent full scope instead of silently keeping
/// half the bands.
pub(super) fn install_taskbar_hooks(pid: Option<u32>) -> Vec<crate::ffi::WinEventHook> {
    let Some(pid) = pid else {
        return Vec::new();
    };
    let hooks: Vec<crate::ffi::WinEventHook> = taskbar_hook_ranges()
        .into_iter()
        .filter_map(|(min, max)| crate::ffi::install_taskbar_hook(min, max, pid))
        .collect();
    if hooks.len() < taskbar_hook_ranges().len() {
        Vec::new()
    } else {
        hooks
    }
}

/// Pure taskbar-scope reinstall decision: reinstall when the live Explorer
/// pid differs from the cached scope (startup, restart with a new pid, or
/// Explorer gone) or when the installed set is incomplete (failed or partial
/// install must retry on the next broadcast, never strand half-scoped).
/// Identity and band count only, never geometry.
pub(super) fn taskbar_reinstall_due(
    cached: Option<u32>,
    current: Option<u32>,
    installed: usize,
) -> bool {
    cached != current || installed < taskbar_hook_ranges().len()
}

// Per-pump taskbar-pager sync for the tile loop: safety gate, fresh primary
// taskbar facts, per-output frames from live plus saved rects, and overlay
// present. `rediscovered` is the once-per-turn native rediscovery broadcast
// consumed by the caller (single consumption point). Returns true when a
// click waits (the pump wakes for it; the tick dispatch drains it once).
// Proof owners never page: the caller skips this entirely on proof paths,
// and teardown drops the overlays.
pub(super) fn sync_taskbar_pager(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    log_path: &Path,
    now: Instant,
    rediscovered: bool,
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
    // Fresh primary facts every pump (bar, tray edge, visibility, auto-hide
    // switch, DPI: five reads, no enumeration): a tray expansion, auto-hide
    // slide, or Explorer disappearance is visible on the very next pump.
    // Child create/show/location traffic arrives through the existing global
    // hooks and wakes that pump (see `window_edge_wakes`), so healthy
    // primary churn re-reads on the changing pump itself; layouts never use
    // older facts.
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
    // a stale-anchor flag (validated taskbar-subtree edge from the drain,
    // the rediscovery broadcast passed in by the caller, or a
    // monitor-topology change) revalidates; the owner wait stays fully
    // event-driven with no timer, and nothing enumerates while suspended
    // (the flag is retained for resume). The drain sets the flag without
    // inspecting the cached census, so empty, unproven, and proven-but-
    // churned anchors all revalidate on genuine taskbar change; the flag
    // clears only on revalidation, so a restart observed on an idle pump
    // is never lost.
    if rediscovered {
        state.pager_taskbar_dirty = true;
    }
    let areas_fp = areas_fingerprint(areas);
    if secondary_revalidate_due(
        state.pager_taskbar_dirty,
        areas_fp,
        state.pager_areas_fp,
        state.suspended,
    ) {
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
        state.pager_taskbar_dirty = false;
    }
    let gate_unknown = areas.is_empty() || state.pager_primary.is_none();
    let facts = pager_suppress_facts(state, me, fulls, state.taskbar_pager, gate_unknown);
    if let Some(reason) = facts.reason {
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
        if note_pager_suppress(&mut state.pager_suppress_last, Some(reason)) {
            let mut line = serde_json::json!({"event": "pager-suppress", "outcome": reason});
            if dropped > 0 {
                line["dropped"] = serde_json::Value::from(dropped);
            }
            pager_suppress_diag(&mut line, &facts);
            log_json_at(log_path, line);
        }
        emit_pager_gate(state, &facts, log_path);
        return false;
    }
    if note_pager_suppress(&mut state.pager_suppress_last, None) {
        let mut line = serde_json::json!({"event": "pager-suppress", "outcome": "show"});
        pager_suppress_diag(&mut line, &facts);
        log_json_at(log_path, line);
    }
    emit_pager_gate(state, &facts, log_path);
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
            &state.pager_z,
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
    // Shared owner resolve (first consumer to observe a moved version runs
    // the single WinRT resolve; the border already resolved above on turns
    // it painted, so this is usually a cache hit): the strips paint the
    // same snapshot the border used, with no second native call.
    let accent = super::accent_owner::refresh_shared_accent(state);
    if let Some(overlays) = state.pager_overlays.as_mut() {
        let taskbar_visible = state.pager_primary.as_ref().map(|bar| bar.visible);
        overlays.sync(&pagers, now, state.trace, accent, taskbar_visible);
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
    let facts = pager_suppress_facts(state, me, fulls, state.taskbar_pager, areas.is_empty());
    if let Some(reason) = facts.reason {
        let mut line = workspace_log(state, tick, "select", index, "pager", reason);
        pager_suppress_diag(&mut line, &facts);
        log_json_at(&state.log_path, line);
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
    // REQ-WS-09 departure memory, same as the tray path: the
    // dispatch-time foreground resolves directly against verified source
    // membership.
    let foreground_hwnd = crate::ffi::foreground_window();
    if let Some(source_id) = state.workspaces.active_id(output)
        && let Some(token) = remember_select_departure(
            state,
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
        MonitorArea, ObservedWindow, RetainedRow, clear_pending_seams, emit_pager_gate,
        foreground_elevated, foreground_elevated_detailed, foreground_pager_elevated,
        foreground_pager_elevated_detailed, inject_pending_elev_probe, inject_pending_elevated,
        inject_pending_fg_class, inject_pending_observe, inject_pending_suspend,
        inject_pending_zorder, note_pager_suppress, pager_fg_surface, pager_suppress_diag,
        pager_suppress_facts, pager_suppress_signature, poll_pager_select,
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
    fn window_edge_wakes_accepts_window_shapes_only() {
        use super::observe::window_edge_wakes;
        use super::{
            EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE,
            EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART,
        };
        // Window-object shapes wake, whether top-level or child: the rule
        // reads the event plus the object id only, never the HWND, so a
        // taskbar descendant (own HWND, OBJID_WINDOW) wakes exactly like
        // its bar. Producer-shaped child traffic is therefore a wake.
        for event in [
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_HIDE,
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_SYSTEM_FOREGROUND,
        ] {
            assert!(
                window_edge_wakes(event, 0),
                "window shape wakes: {event:#X}"
            );
        }
        // Sub-object noise never wakes: client area, title bar, menu and
        // system menu ids carry no window-identity change. Foreground with
        // a non-window id is noise too.
        for id in [-4, -3, -2, -1] {
            for event in [
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_SYSTEM_FOREGROUND,
            ] {
                assert!(
                    !window_edge_wakes(event, id),
                    "sub-object never wakes: {event:#X}/{id}"
                );
            }
        }
        // Move/size transitions and unknown events route elsewhere, never
        // through the wake rule.
        assert!(!window_edge_wakes(EVENT_SYSTEM_MOVESIZESTART, 0));
        assert!(!window_edge_wakes(EVENT_SYSTEM_MOVESIZEEND, 0));
        assert!(!window_edge_wakes(0x800A, 0));
        clear_pending_seams();
    }

    #[test]
    fn secondary_revalidate_due_covers_dirty_topology_and_suspend() {
        // The gate never inspects the cached census: a stale-anchor flag
        // (validated taskbar-subtree edge, broadcast, or topology mark) or a
        // changed fingerprint revalidates, so empty, unproven, and
        // proven-but-churned anchors all converge. Clean plus stable never
        // enumerates. While suspended nothing enumerates: the flag is
        // retained for resume, so gaming owns no taskbar work.
        assert!(super::secondary_revalidate_due(true, 7, 7, false));
        assert!(super::secondary_revalidate_due(false, 7, 8, false));
        assert!(!super::secondary_revalidate_due(false, 7, 7, false));
        assert!(!super::secondary_revalidate_due(true, 7, 7, true));
        assert!(!super::secondary_revalidate_due(false, 7, 8, true));
        assert!(!super::secondary_revalidate_due(false, 7, 7, true));
        clear_pending_seams();
    }

    #[test]
    fn taskbar_reinstall_due_tracks_identity_and_completeness() {
        // Startup or a vanished Explorer with no installed bands re-runs the
        // (native-free, for `None`) install path on the next trigger rather
        // than stranding half-scoped; a stable pid with both bands never
        // reinstalls. Identity and band count only, never geometry: the live
        // unit proves delivery, this locks the decision.
        assert!(super::taskbar_reinstall_due(None, Some(4242), 0));
        assert!(super::taskbar_reinstall_due(None, None, 0));
        assert!(!super::taskbar_reinstall_due(
            Some(4242),
            Some(4242),
            super::taskbar_hook_ranges().len()
        ));
        assert!(super::taskbar_reinstall_due(Some(4242), Some(7777), 2));
        assert!(super::taskbar_reinstall_due(Some(4242), None, 0));
        assert!(super::taskbar_reinstall_due(Some(4242), Some(4242), 0));
        assert!(super::taskbar_reinstall_due(Some(4242), Some(4242), 1));
        clear_pending_seams();
    }

    #[test]
    fn durable_tray_broadcast_refreshes_scope_without_pager_windows() {
        // Restart with zero pager carriers: no overlay exists to receive
        // the broadcast, so the durable tray owner forwards it through the
        // shared flag. The host consumes it exactly once and the reinstall
        // predicate retries a coherent full scope on that turn.
        let _ = crate::ffi::take_pager_rediscover();
        crate::ffi::note_pager_taskbar_rediscover();
        assert!(
            crate::pager_sys::take_rediscover(),
            "forwarded broadcast must flag once"
        );
        assert!(
            !crate::pager_sys::take_rediscover(),
            "flag never replays a second pump"
        );
        assert!(super::taskbar_reinstall_due(None, Some(4242), 0));
        assert!(!super::taskbar_reinstall_due(
            Some(4242),
            Some(4242),
            super::taskbar_hook_ranges().len()
        ));
        clear_pending_seams();
    }

    #[test]
    fn install_taskbar_hooks_needs_identity() {
        // No Explorer identity installs no hooks without touching native
        // scope state; ranges mirror the narrow global object bands.
        // (Refused identities cannot be simulated portably offline; the
        // all-or-nothing shape is covered by `taskbar_reinstall_due` and the
        // unavailable log path in the loop.)
        assert!(super::install_taskbar_hooks(None).is_empty());
        assert_eq!(super::taskbar_hook_ranges().len(), 2);
        clear_pending_seams();
    }

    #[test]
    fn drain_taskbar_edge_marks_dirty_regardless_of_cache() {
        use super::drain_hook_edges;
        use super::observe::hook_queue;
        use super::{EVENT_OBJECT_CREATE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW};
        // Empty census (pre-rebuild): producer-shaped Explorer child edges
        // still mark anchors stale. Child HWNDs are fabricated ids, never
        // dereferenced; the object id carries the window shape.
        let (mut state, me, _, dir, _, _, _) = pager_rig();
        assert!(state.pager_secondaries.is_empty());
        crate::ffi::hooks::drive_taskbar_event(EVENT_OBJECT_CREATE, 901, 0, 11, 1);
        crate::ffi::hooks::drive_taskbar_event(EVENT_OBJECT_SHOW, 902, 0, 11, 2);
        crate::ffi::hooks::drive_taskbar_event(EVENT_OBJECT_LOCATIONCHANGE, 903, 0, 11, 3);
        let drain = drain_hook_edges(&mut state, &me, &dir);
        assert!(drain.woke);
        assert!(state.pager_taskbar_dirty);
        // Unrelated main-queue traffic never marks anchors stale: game and
        // app wakes flow through the global queue only, so they can never
        // arm secondary enumeration however hot they get.
        let (mut quiet, me, _, dir, _, _, _) = pager_rig();
        hook_queue()
            .lock()
            .map(|mut queue| {
                queue.push(super::observe::HookEvent::Wake(701));
                queue.push(super::observe::HookEvent::Wake(702));
            })
            .expect("hook queue");
        drain_hook_edges(&mut quiet, &me, &dir);
        assert!(!quiet.pager_taskbar_dirty);
        clear_pending_seams();
    }

    #[test]
    fn drain_taskbar_sub_object_noise_never_marks_dirty() {
        use super::drain_hook_edges;
        use super::{EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW};
        // Sub-object Explorer traffic (client/menu ids on real shapes)
        // reaches the scoped queue but the shared window-edge rule drops
        // it: no stale mark, no wake, no enumeration.
        let (mut state, me, _, dir, _, _, _) = pager_rig();
        crate::ffi::hooks::drive_taskbar_event(EVENT_OBJECT_SHOW, 911, -4, 11, 1);
        crate::ffi::hooks::drive_taskbar_event(EVENT_OBJECT_LOCATIONCHANGE, 912, -3, 11, 2);
        let drain = drain_hook_edges(&mut state, &me, &dir);
        assert!(!drain.woke);
        assert!(!state.pager_taskbar_dirty);
        clear_pending_seams();
    }

    #[test]
    fn taskbar_subtree_hit_accepts_bars_and_children() {
        use super::{inject_pending_fg_class, inject_pending_parents, taskbar_subtree_hit};
        // Bar itself matches with no walk; a tray child matches through its
        // bar parent; an unreadable mid-chain link fails conservatively to
        // stale (a destroyed handle cannot prove the window unrelated).
        inject_pending_fg_class(900, Some("Shell_SecondaryTrayWnd".to_owned()));
        inject_pending_fg_class(901, Some("TrayNotifyWnd".to_owned()));
        inject_pending_fg_class(903, Some("TrayNotifyWnd".to_owned()));
        inject_pending_parents(vec![(900, 0), (901, 900), (903, 904), (904, 0)]);
        assert!(taskbar_subtree_hit(900));
        assert!(taskbar_subtree_hit(901));
        assert!(taskbar_subtree_hit(903));
        clear_pending_seams();
    }

    #[test]
    fn taskbar_subtree_hit_rejects_ordinary_explorer_windows() {
        use super::{inject_pending_fg_class, inject_pending_parents, taskbar_subtree_hit};
        // Ordinary File Explorer traffic (top-level or nested, fully
        // readable, no bar class anywhere) never invalidates anchors,
        // however hot it gets. Null handles never match either.
        inject_pending_fg_class(910, Some("CabinetWClass".to_owned()));
        inject_pending_fg_class(911, Some("DirectUIHWND".to_owned()));
        inject_pending_parents(vec![(910, 0), (911, 910)]);
        assert!(!taskbar_subtree_hit(910));
        assert!(!taskbar_subtree_hit(911));
        assert!(!taskbar_subtree_hit(0));
        clear_pending_seams();
    }

    #[test]
    fn drain_taskbar_unrelated_explorer_event_ignored() {
        use super::drain_hook_edges;
        use super::{EVENT_OBJECT_CREATE, inject_pending_fg_class, inject_pending_parents};
        // A fully readable ordinary File Explorer create reaches the scoped
        // queue (same Explorer pid) but validates unrelated: no stale mark,
        // no wake, no enumeration.
        let (mut state, me, _, dir, _, _, _) = pager_rig();
        inject_pending_fg_class(920, Some("CabinetWClass".to_owned()));
        inject_pending_parents(vec![(920, 0)]);
        crate::ffi::hooks::drive_taskbar_event(EVENT_OBJECT_CREATE, 920, 0, 11, 1);
        let drain = drain_hook_edges(&mut state, &me, &dir);
        assert!(!drain.woke);
        assert!(!state.pager_taskbar_dirty);
        clear_pending_seams();
    }

    #[test]
    fn drain_taskbar_child_accepted_and_game_suspend_deferred() {
        use super::drain_hook_edges;
        use super::{EVENT_OBJECT_SHOW, inject_pending_fg_class, inject_pending_parents};
        // A validated taskbar child marks stale and wakes the turn.
        let (mut state, me, _, dir, _, _, _) = pager_rig();
        inject_pending_fg_class(930, Some("TrayNotifyWnd".to_owned()));
        inject_pending_fg_class(931, Some("Shell_TrayWnd".to_owned()));
        inject_pending_parents(vec![(930, 931), (931, 0)]);
        crate::ffi::hooks::drive_taskbar_event(EVENT_OBJECT_SHOW, 930, 0, 11, 1);
        let drain = drain_hook_edges(&mut state, &me, &dir);
        assert!(drain.woke);
        assert!(state.pager_taskbar_dirty);
        // While suspended no filter work runs: the batch only retains the
        // flag for resume (no validation, no wake from the scoped edge).
        let (mut held, me, _, dir, _, _, _) = pager_rig();
        held.suspended = true;
        inject_pending_fg_class(940, Some("CabinetWClass".to_owned()));
        inject_pending_parents(vec![(940, 0)]);
        crate::ffi::hooks::drive_taskbar_event(EVENT_OBJECT_SHOW, 940, 0, 11, 2);
        let drain = drain_hook_edges(&mut held, &me, &dir);
        assert!(!drain.woke);
        assert!(held.pager_taskbar_dirty);
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

    fn live_observed_hwnd(
        hwnd: u64,
        pid: u32,
        creation: &str,
        frame: tiler_core::geometry::Rect,
    ) -> ObservedWindow {
        ObservedWindow {
            hwnd,
            token: format!("tok-{creation}"),
            outer: frame,
            visible: frame,
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
        }
    }

    fn pager_bar() -> crate::pager::TaskbarFacts {
        crate::pager::TaskbarFacts {
            rect: tiler_core::geometry::Rect {
                x: 0,
                y: 1040,
                w: 1920,
                h: 40,
            },
            tray_left: Some(1700),
            kind: crate::pager::TaskbarKind::Primary,
            visible: true,
            autohide: false,
            dpi: crate::pager::PAGER_BASE_DPI,
        }
    }

    fn pager_full() -> tiler_core::geometry::Rect {
        tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        }
    }

    fn pager_frame_for(state: &super::TileLoop, output: &str) -> crate::pager::OutputPager {
        crate::pager::build_output_pager(
            &state.workspaces,
            &state.pager_live,
            &state.member_tokens,
            &state.member_rects,
            &state.float_rects,
            &std::collections::HashMap::new(),
            &state.pager_z,
            output,
            pager_full(),
            &pager_bar(),
        )
        .expect("pager anchors on the bottom-docked bar")
    }

    #[test]
    fn pager_retains_native_order_with_overlay_topmost() {
        use tiler_core::geometry::Rect;
        clear_pending_seams();
        let mut state = test_state();
        let output = "mon-z".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        // HWND order is the inverse of the native stacking: the topmost
        // (kept fullscreen overlay) carries the smallest HWND.
        let mk = |hwnd: u64, creation: &str| crate::workspace::WindowKey {
            hwnd,
            pid: 7,
            creation: creation.to_owned(),
        };
        let plain = mk(801, "creation-plain");
        let tiled = mk(802, "creation-tiled");
        let overlay = mk(800, "creation-overlay");
        for key in [&plain, &tiled, &overlay] {
            assert!(
                state
                    .workspaces
                    .assign(key.clone(), &output, &active, false)
            );
        }
        let plain_frame = Rect {
            x: 0,
            y: 0,
            w: 960,
            h: 540,
        };
        let tiled_frame = Rect {
            x: 960,
            y: 0,
            w: 960,
            h: 540,
        };
        let overlay_frame = pager_full();
        inject_pending_observe(
            vec![
                live_observed_hwnd(801, 7, "creation-plain", plain_frame),
                live_observed_hwnd(802, 7, "creation-tiled", tiled_frame),
            ],
            vec![RetainedRow {
                key: overlay.clone(),
                token: "tok-creation-overlay".to_owned(),
                rect: Some(overlay_frame),
                maximized: false,
                fullscreen: true,
                facts: None,
            }],
            false,
        );
        // Scripted native order, top-to-bottom: the retained overlay covers
        // both ordinary windows despite its smaller HWND.
        inject_pending_zorder(vec![overlay.clone(), tiled.clone(), plain.clone()]);
        let me = pager_me();
        let fulls = vec![pager_full()];
        let mut skipped = Vec::new();
        let mut retained = Vec::new();
        let observed = state
            .observe(&me, &fulls, &mut skipped, &mut retained)
            .expect("injected observation");
        assert_eq!(observed.len(), 2);
        assert_eq!(
            state.pager_z,
            vec![overlay.clone(), tiled.clone(), plain.clone()]
        );
        assert_eq!(state.pager_live.get(&overlay), Some(&overlay_frame));
        let pager = pager_frame_for(&state, &output);
        let cell = pager
            .cells
            .iter()
            .find(|cell| !cell.mini_keys.is_empty())
            .expect("active cell carries the members");
        // Paint order is bottom first, so the topmost overlay paints last.
        assert_eq!(cell.mini_keys, vec![plain, tiled, overlay]);
        clear_pending_seams();
    }

    #[test]
    fn pager_retains_hidden_order_prunes_and_reuses() {
        use tiler_core::geometry::Rect;
        clear_pending_seams();
        let mut state = test_state();
        let output = "mon-zhide".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        let mk = |hwnd: u64, creation: &str| crate::workspace::WindowKey {
            hwnd,
            pid: 7,
            creation: creation.to_owned(),
        };
        let x = mk(901, "creation-x");
        let y = mk(902, "creation-y");
        for key in [&x, &y] {
            assert!(
                state
                    .workspaces
                    .assign(key.clone(), &output, &active, false)
            );
        }
        let me = pager_me();
        let fulls = vec![pager_full()];
        let frame_x = Rect {
            x: 0,
            y: 0,
            w: 960,
            h: 540,
        };
        let frame_y = Rect {
            x: 960,
            y: 0,
            w: 960,
            h: 540,
        };
        // Saved tile slots so hidden members still resolve miniatures
        // through the portable fallback, as staged by assignment on live
        // ticks (injected observation alone stages no slots).
        state
            .member_tokens
            .insert(x.clone(), "tok-creation-x".to_owned());
        state
            .member_tokens
            .insert(y.clone(), "tok-creation-y".to_owned());
        state
            .member_rects
            .insert("tok-creation-x".to_owned(), frame_x);
        state
            .member_rects
            .insert("tok-creation-y".to_owned(), frame_y);
        // Tick 1: both visible, native top-to-bottom [x, y].
        inject_pending_observe(
            vec![
                live_observed_hwnd(901, 7, "creation-x", frame_x),
                live_observed_hwnd(902, 7, "creation-y", frame_y),
            ],
            Vec::new(),
            false,
        );
        let mut skipped = Vec::new();
        let mut retained = Vec::new();
        state
            .observe(&me, &fulls, &mut skipped, &mut retained)
            .expect("tick 1");
        assert_eq!(state.pager_z, vec![x.clone(), y.clone()]);
        // Tick 2: x hidden (still assigned, absent from the enumeration);
        // the unset zorder falls back to observed order and x keeps its
        // retained slot after the visible member.
        inject_pending_observe(
            vec![live_observed_hwnd(902, 7, "creation-y", frame_y)],
            Vec::new(),
            false,
        );
        state
            .observe(&me, &fulls, &mut skipped, &mut retained)
            .expect("tick 2");
        assert_eq!(state.pager_z, vec![y.clone(), x.clone()]);
        assert_eq!(state.pager_live.get(&y), Some(&frame_y));
        assert!(!state.pager_live.contains_key(&x));
        let pager = pager_frame_for(&state, &output);
        let cell = pager
            .cells
            .iter()
            .find(|cell| !cell.mini_keys.is_empty())
            .expect("hidden workspace still paints its members");
        assert_eq!(cell.mini_keys, vec![x.clone(), y.clone()]);
        // Tick 3: x closed (membership gone) prunes the retained slot.
        assert!(state.workspaces.remove_window(&x));
        inject_pending_observe(
            vec![live_observed_hwnd(902, 7, "creation-y", frame_y)],
            Vec::new(),
            false,
        );
        state
            .observe(&me, &fulls, &mut skipped, &mut retained)
            .expect("tick 3");
        assert_eq!(state.pager_z, vec![y.clone()]);
        // Tick 4: a recycled HWND with a new creation is a new identity in
        // the deterministic fallback, never the stale slot.
        let x2 = mk(901, "creation-x2");
        assert!(state.workspaces.assign(x2.clone(), &output, &active, false));
        inject_pending_observe(
            vec![live_observed_hwnd(902, 7, "creation-y", frame_y)],
            Vec::new(),
            false,
        );
        state
            .observe(&me, &fulls, &mut skipped, &mut retained)
            .expect("tick 4");
        assert_eq!(state.pager_z, vec![y.clone(), x2.clone()]);
        assert!(!state.pager_z.contains(&x));
        clear_pending_seams();
    }

    #[test]
    fn pager_orders_first_paint_without_second_observe() {
        use tiler_core::geometry::Rect;
        clear_pending_seams();
        let mut state = test_state();
        let output = "mon-zfirst".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        let mk = |hwnd: u64, creation: &str| crate::workspace::WindowKey {
            hwnd,
            pid: 7,
            creation: creation.to_owned(),
        };
        // Inverted HWND order: the topmost window carries the smallest HWND,
        // so the deterministic key fallback would paint the wrong order.
        let top = mk(810, "creation-first-top");
        let bottom = mk(820, "creation-first-bottom");
        let alien = mk(830, "creation-first-alien");
        let top_frame = Rect {
            x: 0,
            y: 0,
            w: 960,
            h: 540,
        };
        let bottom_frame = Rect {
            x: 960,
            y: 0,
            w: 960,
            h: 540,
        };
        let alien_frame = Rect {
            x: 0,
            y: 540,
            w: 960,
            h: 540,
        };
        // Tick 1: neither member assigned yet; the alien is never admitted.
        inject_pending_observe(
            vec![
                live_observed_hwnd(810, 7, "creation-first-top", top_frame),
                live_observed_hwnd(820, 7, "creation-first-bottom", bottom_frame),
                live_observed_hwnd(830, 7, "creation-first-alien", alien_frame),
            ],
            Vec::new(),
            false,
        );
        inject_pending_zorder(vec![top.clone(), bottom.clone(), alien.clone()]);
        let me = pager_me();
        let fulls = vec![pager_full()];
        let mut skipped = Vec::new();
        let mut retained = Vec::new();
        state
            .observe(&me, &fulls, &mut skipped, &mut retained)
            .expect("tick 1");
        // Fresh verified candidates persist until the next observation even
        // though admission has not assigned them yet.
        assert_eq!(
            state.pager_z,
            vec![top.clone(), bottom.clone(), alien.clone()]
        );
        // Admission assigns the members and stages their slots (the live
        // frame map itself still lags until the next observation: preexisting
        // first-admission behavior, unchanged here). No second observation
        // runs before the first paint.
        for key in [&top, &bottom] {
            assert!(
                state
                    .workspaces
                    .assign(key.clone(), &output, &active, false)
            );
        }
        state
            .member_tokens
            .insert(top.clone(), "tok-first-top".to_owned());
        state
            .member_tokens
            .insert(bottom.clone(), "tok-first-bottom".to_owned());
        state
            .member_rects
            .insert("tok-first-top".to_owned(), top_frame);
        state
            .member_rects
            .insert("tok-first-bottom".to_owned(), bottom_frame);
        let pager = pager_frame_for(&state, &output);
        let cell = pager
            .cells
            .iter()
            .find(|cell| !cell.mini_keys.is_empty())
            .expect("admitted members paint on the first frame");
        // Native paint order despite the inverted HWNDs; the never-admitted
        // alien renders nothing.
        assert_eq!(cell.mini_keys, vec![bottom.clone(), top.clone()]);
        let cell_rect = cell.rect;
        assert_eq!(
            cell.minis,
            vec![
                crate::pager::miniature(
                    cell_rect,
                    pager_full(),
                    bottom_frame,
                    crate::pager::PAGER_BASE_DPI
                ),
                crate::pager::miniature(
                    cell_rect,
                    pager_full(),
                    top_frame,
                    crate::pager::PAGER_BASE_DPI
                ),
            ]
        );
        // Tick 2: the never-admitted alien is absent and pruned; fresh
        // candidates persist only until the next observation. The scripted
        // order is cleared so this tick falls back to observed order, like
        // the production enumeration path.
        clear_pending_seams();
        inject_pending_observe(
            vec![
                live_observed_hwnd(810, 7, "creation-first-top", top_frame),
                live_observed_hwnd(820, 7, "creation-first-bottom", bottom_frame),
            ],
            Vec::new(),
            false,
        );
        state
            .observe(&me, &fulls, &mut skipped, &mut retained)
            .expect("tick 2");
        assert_eq!(state.pager_z, vec![top.clone(), bottom.clone()]);
        assert!(!state.pager_z.contains(&alien));
        clear_pending_seams();
    }

    /// Legacy elevation bool table through the production probe path: every
    /// skip case and every fail-closed leg keeps its exact bool while the
    /// detail names the firing leg and the category names the OS integrity
    /// level independently (medium-plus stays non-elevated, gaps report
    /// unknown). Fully offline via scripted native inputs.
    #[allow(clippy::type_complexity)]
    #[test]
    fn elevated_probe_preserves_legacy_bool_table() {
        // (foreground, pid, open_ok, integrity, expected bool, detail, open,
        //  integrity-read, integrity category).
        let table: Vec<(
            u64,
            u32,
            bool,
            Option<std::result::Result<u32, ()>>,
            bool,
            &str,
            &str,
            &str,
            &str,
        )> = vec![
            (
                0,
                0,
                false,
                None,
                false,
                "no-foreground",
                "skipped",
                "skipped",
                "unknown",
            ),
            (
                6101, 0, false, None, false, "no-pid", "skipped", "skipped", "unknown",
            ),
            (
                6101,
                5009,
                false,
                None,
                false,
                "own-process",
                "skipped",
                "skipped",
                "unknown",
            ),
            (
                6101,
                42,
                false,
                None,
                true,
                "process-open-failed",
                "open-failed",
                "skipped",
                "unknown",
            ),
            (
                6101,
                42,
                true,
                None,
                true,
                "integrity-read-failed",
                "opened",
                "read-failed",
                "unknown",
            ),
            (
                6101,
                42,
                true,
                Some(Err(())),
                true,
                "integrity-read-failed",
                "opened",
                "read-failed",
                "unknown",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(8192)),
                false,
                "medium",
                "opened",
                "ok",
                "medium",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(10000)),
                false,
                "medium",
                "opened",
                "ok",
                "unknown",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(12287)),
                false,
                "medium",
                "opened",
                "ok",
                "unknown",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(8448)),
                false,
                "medium",
                "opened",
                "ok",
                "medium-plus",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(9000)),
                false,
                "medium",
                "opened",
                "ok",
                "unknown",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(0)),
                true,
                "non-medium",
                "opened",
                "ok",
                "untrusted",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(500)),
                true,
                "non-medium",
                "opened",
                "ok",
                "unknown",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(4096)),
                true,
                "non-medium",
                "opened",
                "ok",
                "low",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(5000)),
                true,
                "non-medium",
                "opened",
                "ok",
                "low",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(12288)),
                true,
                "non-medium",
                "opened",
                "ok",
                "high",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(16384)),
                true,
                "non-medium",
                "opened",
                "ok",
                "system",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(20000)),
                true,
                "non-medium",
                "opened",
                "ok",
                "system",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(20480)),
                true,
                "non-medium",
                "opened",
                "ok",
                "protected",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(25000)),
                true,
                "non-medium",
                "opened",
                "ok",
                "unknown",
            ),
        ];
        let me = pager_me();
        for (foreground, pid, open_ok, integrity, elevated, detail, open, read, category) in table {
            clear_pending_seams();
            inject_pending_elev_probe(foreground, pid, open_ok, integrity);
            let probe = foreground_elevated_detailed(&me);
            assert_eq!(probe.elevated, elevated, "bool for {detail}");
            assert_eq!(
                foreground_elevated(&me),
                elevated,
                "wrapper bool for {detail}"
            );
            assert_eq!(probe.detail, detail);
            assert_eq!(probe.open, open);
            assert_eq!(probe.integrity_read, read);
            assert_eq!(probe.integrity, category);
            assert_eq!(probe.foreground, foreground);
            assert!(probe.captured);
        }
        clear_pending_seams();
    }

    #[test]
    fn elevated_probe_coarse_seam_reports_honest_skip() {
        clear_pending_seams();
        let me = pager_me();
        // Coarse seam wins over the scripted probe and says so: no implicit
        // elevation claim from a skipped read.
        inject_pending_elev_probe(6101, 42, true, Some(Ok(5000)));
        inject_pending_elevated(Some(true));
        let probe = foreground_elevated_detailed(&me);
        assert!(probe.elevated);
        assert_eq!(probe.detail, "skipped-injected");
        assert_eq!(probe.open, "skipped");
        assert_eq!(probe.integrity, "unknown");
        assert!(!probe.captured);
        inject_pending_elevated(Some(false));
        assert!(!foreground_elevated_detailed(&me).elevated);
        clear_pending_seams();
    }

    /// Pager-only probe table through the production pager classifier: skip
    /// and fail-closed legs match the shared table exactly; only the
    /// read-RID leg differs (above-medium suppresses, medium-or-below
    /// stays visible). Tiling keeps the medium-only table deliberately.
    #[allow(clippy::type_complexity)]
    #[test]
    fn pager_probe_suppresses_only_above_medium() {
        let table: Vec<(
            u64,
            u32,
            bool,
            Option<std::result::Result<u32, ()>>,
            bool,
            &str,
            &str,
            &str,
            &str,
        )> = vec![
            (
                0,
                0,
                false,
                None,
                false,
                "no-foreground",
                "skipped",
                "skipped",
                "unknown",
            ),
            (
                6101, 0, false, None, false, "no-pid", "skipped", "skipped", "unknown",
            ),
            (
                6101,
                5009,
                false,
                None,
                false,
                "own-process",
                "skipped",
                "skipped",
                "unknown",
            ),
            (
                6101,
                42,
                false,
                None,
                true,
                "process-open-failed",
                "open-failed",
                "skipped",
                "unknown",
            ),
            (
                6101,
                42,
                true,
                None,
                true,
                "integrity-read-failed",
                "opened",
                "read-failed",
                "unknown",
            ),
            (
                6101,
                42,
                true,
                Some(Err(())),
                true,
                "integrity-read-failed",
                "opened",
                "read-failed",
                "unknown",
            ),
            // Medium-or-below stays visible: untrusted, low/AppContainer low,
            // gaps below high, medium, medium-plus, and the top of medium.
            (
                6101,
                42,
                true,
                Some(Ok(0)),
                false,
                "medium-or-below",
                "opened",
                "ok",
                "untrusted",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(4096)),
                false,
                "medium-or-below",
                "opened",
                "ok",
                "low",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(5000)),
                false,
                "medium-or-below",
                "opened",
                "ok",
                "low",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(8192)),
                false,
                "medium-or-below",
                "opened",
                "ok",
                "medium",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(8448)),
                false,
                "medium-or-below",
                "opened",
                "ok",
                "medium-plus",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(12287)),
                false,
                "medium-or-below",
                "opened",
                "ok",
                "unknown",
            ),
            // At-or-above HIGH suppresses: high, system, protected, and an
            // above-protected gap (still >= high, honestly unknown level).
            (
                6101,
                42,
                true,
                Some(Ok(12288)),
                true,
                "above-medium",
                "opened",
                "ok",
                "high",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(16384)),
                true,
                "above-medium",
                "opened",
                "ok",
                "system",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(20480)),
                true,
                "above-medium",
                "opened",
                "ok",
                "protected",
            ),
            (
                6101,
                42,
                true,
                Some(Ok(25000)),
                true,
                "above-medium",
                "opened",
                "ok",
                "unknown",
            ),
        ];
        let me = pager_me();
        for (foreground, pid, open_ok, integrity, elevated, detail, open, read, category) in table {
            clear_pending_seams();
            inject_pending_elev_probe(foreground, pid, open_ok, integrity);
            let probe = foreground_pager_elevated_detailed(&me);
            assert_eq!(
                probe.elevated, elevated,
                "pager bool for {detail} {category}"
            );
            assert_eq!(
                foreground_pager_elevated(&me),
                elevated,
                "pager wrapper bool for {detail}"
            );
            assert_eq!(probe.detail, detail);
            assert_eq!(probe.open, open);
            assert_eq!(probe.integrity_read, read);
            assert_eq!(probe.integrity, category);
            assert_eq!(probe.foreground, foreground);
            assert!(probe.captured);
        }
        clear_pending_seams();
        // Shared tiling gate deliberately stays medium-only: low still
        // elevates there while the pager stays visible.
        clear_pending_seams();
        inject_pending_elev_probe(6101, 42, true, Some(Ok(5000)));
        assert!(foreground_elevated_detailed(&pager_me()).elevated);
        assert!(!foreground_pager_elevated_detailed(&pager_me()).elevated);
        clear_pending_seams();
    }

    #[test]
    fn fg_surface_labels_are_bounded_and_honest() {
        assert_eq!(pager_fg_surface(None, false), "class-unreadable");
        assert_eq!(
            pager_fg_surface(Some("Progman"), false),
            "known-shell-class"
        );
        assert_eq!(
            pager_fg_surface(Some("WorkerW"), false),
            "known-shell-class"
        );
        assert_eq!(
            pager_fg_surface(Some("Shell_TrayWnd"), false),
            "known-shell-class"
        );
        assert_eq!(
            pager_fg_surface(Some("Shell_SecondaryTrayWnd"), false),
            "known-shell-class"
        );
        assert_eq!(
            pager_fg_surface(Some("DV2ControlHost"), false),
            "known-shell-class"
        );
        assert_eq!(
            pager_fg_surface(Some("MSCTFIME UI"), false),
            "known-shell-class"
        );
        assert_eq!(
            pager_fg_surface(Some("Windows.UI.Core.CoreWindow"), false),
            "possible-shell-surface-heuristic"
        );
        assert_eq!(
            pager_fg_surface(Some("XamlExplorerHostIslandWindow"), false),
            "possible-shell-surface-heuristic"
        );
        assert_eq!(
            pager_fg_surface(Some("MultitaskingViewFrame"), false),
            "possible-shell-surface-heuristic"
        );
        assert_eq!(pager_fg_surface(Some("Notepad"), false), "ordinary");
        assert_eq!(pager_fg_surface(Some("Notepad"), true), "desktop-handle");
        assert_eq!(pager_fg_surface(None, true), "desktop-handle");
    }

    fn suppress_diag_rig() -> (
        super::TileLoop,
        crate::model::ProcessIdentity,
        Vec<tiler_core::geometry::Rect>,
    ) {
        clear_pending_seams();
        let state = test_state();
        let me = pager_me();
        (state, me, vec![pager_full()])
    }

    #[test]
    fn suppress_facts_shape_and_privacy() {
        let (mut state, me, fulls) = suppress_diag_rig();
        // Open cover gate, scripted above-medium ordinary foreground.
        inject_pending_suspend(Some(false));
        inject_pending_elev_probe(6101, 42, true, Some(Ok(12288)));
        inject_pending_fg_class(6101, Some("Notepad".to_owned()));
        let facts = pager_suppress_facts(&mut state, &me, &fulls, true, false);
        assert_eq!(facts.reason, Some("elevated-foreground"));
        assert!(facts.elevated);
        assert!(!facts.suspended);
        assert_eq!(facts.elev_detail, "above-medium");
        assert_eq!(facts.elev_open, "opened");
        assert_eq!(facts.elev_integrity_read, "ok");
        assert_eq!(facts.elev_integrity, "high");
        assert_eq!(facts.cover, "none");
        assert_eq!(facts.elev_class, "Notepad");
        assert_eq!(facts.elev_surface, "ordinary");
        // Cover side was seam-short-circuited: honest skip, no invented class.
        assert_eq!(facts.cover_class, "unknown");
        assert_eq!(facts.cover_surface, "skipped");
        // Sequential captures, never an atomic snapshot: no coherency claim.
        assert!(!facts.handles_match);
        let mut line =
            serde_json::json!({"event": "pager-suppress", "outcome": "elevated-foreground"});
        pager_suppress_diag(&mut line, &facts);
        assert_eq!(line["outcome"], "elevated-foreground");
        assert_eq!(line["fg_class"], "Notepad");
        assert_eq!(line["fg_surface"], "ordinary");
        assert_eq!(line["elev_integrity"], "high");
        assert_eq!(line["cover"], "none");
        // Privacy: no paths, handles, PIDs, titles, or error strings.
        let rendered = serde_json::to_string(&line).expect("render");
        assert!(!rendered.contains('\\'), "no paths: {rendered}");
        assert!(!rendered.contains("6101"), "no HWNDs: {rendered}");
        assert!(!rendered.contains("5009"), "no PIDs: {rendered}");
        assert!(!rendered.contains("error"), "no error strings: {rendered}");
        clear_pending_seams();
    }

    #[test]
    fn suppress_facts_low_and_medium_stay_visible() {
        // Low (including AppContainer low), untrusted, medium, and
        // medium-plus foregrounds must not suppress the pager: only
        // at-or-above HIGH suppresses, with fail-closed open/read legs
        // unchanged.
        for rid in [0u32, 4096, 5000, 8192, 8448, 12287] {
            let (mut state, me, fulls) = suppress_diag_rig();
            inject_pending_suspend(Some(false));
            inject_pending_elev_probe(6101, 42, true, Some(Ok(rid)));
            inject_pending_fg_class(6101, Some("Notepad".to_owned()));
            let facts = pager_suppress_facts(&mut state, &me, &fulls, true, false);
            assert_eq!(facts.reason, None, "RID {rid} must stay visible");
            assert!(!facts.elevated, "RID {rid} must not elevate the pager");
            assert_eq!(facts.elev_detail, "medium-or-below");
            clear_pending_seams();
        }
        // At-or-above HIGH suppresses, with fail-closed legs unchanged.
        for rid in [12288u32, 16384, 20480] {
            let (mut state, me, fulls) = suppress_diag_rig();
            inject_pending_suspend(Some(false));
            inject_pending_elev_probe(6101, 42, true, Some(Ok(rid)));
            inject_pending_fg_class(6101, Some("Notepad".to_owned()));
            let facts = pager_suppress_facts(&mut state, &me, &fulls, true, false);
            assert_eq!(
                facts.reason,
                Some("elevated-foreground"),
                "RID {rid} must suppress"
            );
            assert!(facts.elevated);
            assert_eq!(facts.elev_detail, "above-medium");
            clear_pending_seams();
        }
        // Open/read failures still fail closed.
        for (open_ok, integrity) in [(false, None), (true, None), (true, Some(Err(())))] {
            let (mut state, me, fulls) = suppress_diag_rig();
            inject_pending_suspend(Some(false));
            inject_pending_elev_probe(6101, 42, open_ok, integrity);
            inject_pending_fg_class(6101, Some("Notepad".to_owned()));
            let facts = pager_suppress_facts(&mut state, &me, &fulls, true, false);
            assert_eq!(
                facts.reason,
                Some("elevated-foreground"),
                "fail-closed leg must suppress"
            );
            assert!(facts.elevated);
            clear_pending_seams();
        }
    }

    #[test]
    fn suppress_facts_report_honest_skips() {
        let (mut state, me, fulls) = suppress_diag_rig();
        inject_pending_suspend(Some(true));
        inject_pending_elevated(Some(true));
        let facts = pager_suppress_facts(&mut state, &me, &fulls, true, false);
        // Fullscreen suspend outranks elevation in the preserved order.
        assert_eq!(facts.reason, Some("suspended-fullscreen"));
        assert!(facts.suspended);
        assert_eq!(facts.elev_detail, "skipped-injected");
        assert_eq!(facts.elev_class, "unknown");
        assert_eq!(facts.elev_surface, "skipped");
        assert_eq!(facts.cover_surface, "skipped");
        assert!(!facts.handles_match);
        clear_pending_seams();
    }

    /// Log-file rig for the `pager-gate` transition path: the production
    /// `pager_suppress_facts` + `emit_pager_gate` pair writes real lines, so
    /// these tests prove what the operator actually sees. Fully offline via
    /// scripted native inputs; no desktop reads. The per-process atomic
    /// counter keeps parallel tests on coarse Windows clocks from sharing
    /// one `run.log`.
    fn gate_log_rig() -> (
        super::TileLoop,
        crate::model::ProcessIdentity,
        Vec<tiler_core::geometry::Rect>,
        std::path::PathBuf,
    ) {
        static GATE_LOG_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        clear_pending_seams();
        let mut state = test_state();
        let seq = GATE_LOG_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tiler-gate-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            seq
        ));
        std::fs::create_dir_all(&dir).expect("gate log dir");
        let log = dir.join("run.log");
        state.log_path = log.clone();
        (state, pager_me(), vec![pager_full()], log)
    }

    fn gate_lines(log: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn emit_gate(
        state: &mut super::TileLoop,
        me: &crate::model::ProcessIdentity,
        fulls: &[tiler_core::geometry::Rect],
        enabled: bool,
        unknown: bool,
    ) {
        let facts = pager_suppress_facts(state, me, fulls, enabled, unknown);
        let log_path = state.log_path.clone();
        emit_pager_gate(state, &facts, &log_path);
    }

    #[test]
    fn gate_logs_initial_allowed_baseline_and_stays_silent_on_repeat() {
        let (mut state, me, fulls, log) = gate_log_rig();
        assert_eq!(state.pager_gate_last, None);
        inject_pending_suspend(Some(false));
        inject_pending_elev_probe(6101, 42, true, Some(Ok(8192)));
        inject_pending_fg_class(6101, Some("Notepad".to_owned()));
        emit_gate(&mut state, &me, &fulls, true, false);
        let lines = gate_lines(&log);
        assert_eq!(lines.len(), 1, "first baseline logs: {lines:?}");
        assert!(
            lines[0].contains("\"event\":\"pager-gate\""),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("\"outcome\":\"allowed\""), "{}", lines[0]);
        assert!(
            lines[0].contains("\"fg_surface\":\"ordinary\""),
            "{}",
            lines[0]
        );
        assert!(
            lines[0].contains("\"elev_integrity\":\"medium\""),
            "{}",
            lines[0]
        );
        // Identical repeat emits nothing.
        emit_gate(&mut state, &me, &fulls, true, false);
        assert_eq!(gate_lines(&log).len(), 1);
        clear_pending_seams();
    }

    #[test]
    fn gate_logs_allowed_to_allowed_surface_change() {
        let (mut state, me, fulls, log) = gate_log_rig();
        inject_pending_suspend(Some(false));
        inject_pending_elev_probe(6101, 42, true, Some(Ok(8192)));
        inject_pending_fg_class(6101, Some("Notepad".to_owned()));
        emit_gate(&mut state, &me, &fulls, true, false);
        assert_eq!(gate_lines(&log).len(), 1);
        // Same passing gate, new foreground surface: still `allowed`, but a
        // second line so the desktop-vs-shell transition is observable.
        inject_pending_fg_class(6101, Some("Progman".to_owned()));
        emit_gate(&mut state, &me, &fulls, true, false);
        let lines = gate_lines(&log);
        assert_eq!(lines.len(), 2, "surface change re-logs: {lines:?}");
        assert!(lines[1].contains("\"outcome\":\"allowed\""), "{}", lines[1]);
        assert!(
            lines[1].contains("\"fg_surface\":\"known-shell-class\""),
            "{}",
            lines[1]
        );
        clear_pending_seams();
    }

    #[test]
    fn gate_logs_failed_open_to_failed_integrity_under_same_suppression() {
        let (mut state, me, fulls, log) = gate_log_rig();
        inject_pending_suspend(Some(false));
        inject_pending_fg_class(6101, Some("Notepad".to_owned()));
        inject_pending_elev_probe(6101, 42, false, None);
        emit_gate(&mut state, &me, &fulls, true, false);
        inject_pending_elev_probe(6101, 42, true, None);
        emit_gate(&mut state, &me, &fulls, true, false);
        let lines = gate_lines(&log);
        assert_eq!(lines.len(), 2, "sub-outcome change re-logs: {lines:?}");
        for line in &lines {
            assert!(
                line.contains("\"outcome\":\"elevated-foreground\""),
                "{line}"
            );
        }
        assert!(
            lines[0].contains("\"elev_detail\":\"process-open-failed\""),
            "{}",
            lines[0]
        );
        assert!(
            lines[1].contains("\"elev_detail\":\"integrity-read-failed\""),
            "{}",
            lines[1]
        );
        // An identical repeat emits nothing.
        emit_gate(&mut state, &me, &fulls, true, false);
        assert_eq!(gate_lines(&log).len(), 2);
        clear_pending_seams();
    }

    #[test]
    fn gate_logs_recovery() {
        let (mut state, me, fulls, log) = gate_log_rig();
        inject_pending_suspend(Some(false));
        inject_pending_fg_class(6101, Some("Notepad".to_owned()));
        inject_pending_elev_probe(6101, 42, true, Some(Ok(12288)));
        emit_gate(&mut state, &me, &fulls, true, false);
        let lines = gate_lines(&log);
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].contains("\"outcome\":\"elevated-foreground\""),
            "{}",
            lines[0]
        );
        assert!(
            lines[0].contains("\"elev_integrity\":\"high\""),
            "{}",
            lines[0]
        );
        // Foreground back to medium: recovery logs `allowed`.
        inject_pending_elev_probe(6101, 42, true, Some(Ok(8192)));
        emit_gate(&mut state, &me, &fulls, true, false);
        let lines = gate_lines(&log);
        assert_eq!(lines.len(), 2, "recovery logs: {lines:?}");
        assert!(lines[1].contains("\"outcome\":\"allowed\""), "{}", lines[1]);
        clear_pending_seams();
    }

    #[test]
    fn suppress_state_keeps_exact_prior_semantics() {
        let (mut state, _, _, _) = gate_log_rig();
        // Suppression stores the reason string only.
        assert!(note_pager_suppress(
            &mut state.pager_suppress_last,
            Some("elevated-foreground")
        ));
        assert_eq!(
            state.pager_suppress_last.as_deref(),
            Some("elevated-foreground")
        );
        assert!(!state.pager_suppress_last.is_none());
        // Identical repeat stays silent.
        assert!(!note_pager_suppress(
            &mut state.pager_suppress_last,
            Some("elevated-foreground")
        ));
        // Changed reason re-logs.
        assert!(note_pager_suppress(
            &mut state.pager_suppress_last,
            Some("suspended-fullscreen")
        ));
        // The legacy stale-facts marker never equals a reason.
        state.pager_suppress_last = Some("stale-facts".to_owned());
        assert!(note_pager_suppress(
            &mut state.pager_suppress_last,
            Some("elevated-foreground")
        ));
        assert_eq!(
            state.pager_suppress_last.as_deref(),
            Some("elevated-foreground")
        );
        // Recovery clears, and the scheduler predicate sees showing.
        assert!(note_pager_suppress(&mut state.pager_suppress_last, None));
        assert_eq!(state.pager_suppress_last, None);
        assert!(state.pager_suppress_last.is_none());
        assert!(!note_pager_suppress(&mut state.pager_suppress_last, None));
        // The compatibility state never carries a diagnostic signature.
        assert_eq!(state.pager_gate_last, None);
        clear_pending_seams();
    }

    #[test]
    fn changed_elev_suboutcome_relogs_under_same_suppression() {
        let (mut state, me, fulls) = suppress_diag_rig();
        inject_pending_suspend(Some(false));
        inject_pending_fg_class(6101, Some("Notepad".to_owned()));
        inject_pending_elev_probe(6101, 42, false, None);
        let first = pager_suppress_facts(&mut state, &me, &fulls, true, false);
        assert_eq!(first.reason, Some("elevated-foreground"));
        assert_eq!(first.elev_detail, "process-open-failed");
        let first_sig = pager_suppress_signature(&first);
        // The open now succeeds but the integrity read fails: identical
        // high-level suppression, changed sub-outcomes.
        inject_pending_elev_probe(6101, 42, true, None);
        let second = pager_suppress_facts(&mut state, &me, &fulls, true, false);
        assert_eq!(second.reason, Some("elevated-foreground"));
        assert_eq!(second.elev_detail, "integrity-read-failed");
        assert_ne!(pager_suppress_signature(&second), first_sig);
        // An identical repeat emits nothing.
        let repeat = pager_suppress_facts(&mut state, &me, &fulls, true, false);
        assert_eq!(
            pager_suppress_signature(&repeat),
            pager_suppress_signature(&second)
        );
        clear_pending_seams();
    }
}
