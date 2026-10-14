use super::*;

#[cfg(test)]
use super::tiling_sys_hints::test_hint_for;

/// `SendMessageTimeoutW` timeout failure code: distinguishes a hung-window
/// abort from any other setter failure.
const ERROR_TIMEOUT: u32 = 1460;
/// One system-metrics read at the window's current DPI. `GetSystemMetricsForDpi`
/// first, plain `GetSystemMetrics` fallback: a zero read fails closed to zero
/// and the seeder leaves that field zero rather than inventing a size.
fn system_metric_for_dpi(index: i32, dpi: u32) -> i32 {
    let sized = unsafe { GetSystemMetricsForDpi(index, dpi) };
    if sized > 0 {
        return sized;
    }
    unsafe { GetSystemMetrics(index) }
}

/// Correctly seeded Rust `MINMAXINFO` for one window: minimum track from the
/// window's current DPI, maximum track plus maximized size/position per the
/// normal `WM_GETMINMAXINFO` contract. Seeded directly in Rust from supported
/// system-metrics APIs, never by copying another window's struct. An app that
/// leaves a field untouched therefore reports the system default instead of a
/// fabricated zero absence.
fn seed_minmaxinfo(hwnd: HWND) -> MINMAXINFO {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let mut info: MINMAXINFO = unsafe { std::mem::zeroed() };
    let min_w = system_metric_for_dpi(SM_CXMINTRACK, dpi);
    let min_h = system_metric_for_dpi(SM_CYMINTRACK, dpi);
    if min_w > 0 && min_h > 0 {
        info.ptMinTrackSize.x = min_w;
        info.ptMinTrackSize.y = min_h;
    }
    let max_track_w = system_metric_for_dpi(SM_CXMAXTRACK, dpi);
    let max_track_h = system_metric_for_dpi(SM_CYMAXTRACK, dpi);
    if max_track_w > 0 && max_track_h > 0 {
        info.ptMaxTrackSize.x = max_track_w;
        info.ptMaxTrackSize.y = max_track_h;
    }
    let scr_w = system_metric_for_dpi(SM_CXSCREEN, dpi);
    let scr_h = system_metric_for_dpi(SM_CYSCREEN, dpi);
    if scr_w > 0 && scr_h > 0 {
        info.ptMaxSize.x = scr_w;
        info.ptMaxSize.y = scr_h;
    }
    info
}

/// Fresh outer track sizes: `(minimum, maximum, outcome)` as raw query
/// pairs. Normalization stays in one place ([`crate::tiling::hints_from_outer`]
/// and [`crate::tiling::hints_from_outer`]); the query only strips
/// seeded-untouched maximums and reports transport outcomes. The maximum is
/// `None` when the query failed, hung, overran the budget, or matched the
/// seed (app-untouched, never app evidence).
type OuterTracks = (Option<(i32, i32)>, Option<(i32, i32)>, HintOutcome);

/// Fresh outer minimum/maximum track sizes for one window in physical pixels
/// plus its outcome.
///
/// `SendMessageTimeoutW` with `WM_GETMINMAXINFO`, `SMTO_ABORTIFHUNG`, and
/// [`MIN_HINT_TIMEOUT_MS`] over the correctly seeded struct above. A zero
/// return means failure or timeout and the struct is ignored entirely (never
/// read); `ERROR_TIMEOUT` distinguishes hung aborts from other failures.
/// Success with no usable track size is `Invalid`. Requires the loop's
/// verified PMv2 awareness so the track sizes are physical pixels.
///
/// The maximum side strips seeded-untouched values: an app that leaves
/// `ptMaxTrackSize` untouched reports the system default this file seeded,
/// which is never claimed as app evidence (it arrives as unknown, and
/// unknown maximums never classify under R-SPC-04 D1). No geometry, style,
/// or resizeable inference is involved.
fn query_outer_tracks(hwnd: HWND, cx: &HintCx) -> OuterTracks {
    let mut info = seed_minmaxinfo(hwnd);
    let seed_max = (info.ptMaxTrackSize.x, info.ptMaxTrackSize.y);
    let mut result: usize = 0;
    let timeout_ms = cx.timeout_ms();
    if timeout_ms == 0 {
        return (None, None, HintOutcome::Budget);
    }
    let sent = unsafe {
        SetLastError(0);
        SendMessageTimeoutW(
            hwnd,
            WM_GETMINMAXINFO,
            0,
            &mut info as *mut MINMAXINFO as LPARAM,
            SMTO_ABORTIFHUNG,
            timeout_ms,
            &mut result,
        )
    };
    if sent == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_TIMEOUT {
            return (None, None, HintOutcome::Timeout);
        }
        return (None, None, HintOutcome::Failed);
    }
    let min = Some((info.ptMinTrackSize.x, info.ptMinTrackSize.y));
    let live_max = (info.ptMaxTrackSize.x, info.ptMaxTrackSize.y);
    let max = if live_max == seed_max {
        None
    } else {
        Some(live_max)
    };
    // Per-axis outcomes: any usable or explicitly-unset bound on either side
    // is a hint; only a fully unknown query is invalid. Explicit zeros ride
    // (unset per axis, G-05) rather than failing the whole query; validity
    // itself is decided once at conversion
    // ([`crate::tiling::hints_from_outer`]).
    let norm = |(w, h)| crate::tiling::normalize_min_track(w, h);
    let min_norm = min.map_or((None, None), norm);
    let max_norm = max.map_or((None, None), norm);
    let outcome = match (min_norm, max_norm) {
        ((None, None), (None, None)) => HintOutcome::Invalid,
        _ => HintOutcome::Hint,
    };
    (min, max, outcome)
}

/// Fresh fixed-size verdict for one first-seen maximized window (R-SPC-04
/// D4): currently measured frame insets plus a fresh bounded
/// `WM_GETMINMAXINFO` min/max query under the same aggregate budget,
/// classified with the live predicate through the shared policy (never a
/// copy, never style/resizeable inference). Seeded-untouched maximums stay
/// unknown and never classify. `false` on any measurement, query, or budget
/// failure: fail-closed to the existing clear/overlay lane, so an unknown
/// window never claims fixed and takes no float lane.
pub(super) fn fixed_birth_for_slotless(
    hwnd_u64: u64,
    predicate: tiler_core::size_hints::FixedSizePredicate,
    cx: &mut HintCx,
) -> bool {
    if hwnd_u64 == 0 || cx.over_budget() {
        return false;
    }
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    let mut outer_raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut outer_raw) } == 0 {
        return false;
    }
    let Some(outer) = rect_from_win(outer_raw) else {
        return false;
    };
    let mut visible_raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut visible_raw as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } != 0
    {
        return false;
    }
    let Some(visible) = rect_from_win(visible_raw) else {
        return false;
    };
    if !tiler_core::bounds::valid_carried_rect(visible.x, visible.y, visible.w, visible.h) {
        return false;
    }
    let insets = FrameInsets::measure(outer, visible);
    if cx.over_budget() {
        return false;
    }
    let (min_track, max_track, outcome) = query_outer_tracks(hwnd, cx);
    if outcome != HintOutcome::Hint {
        return false;
    }
    let hints = crate::tiling::hints_from_outer(min_track, max_track, insets);
    tiler_core::size_hints::is_fixed_size_with(hints, predicate)
}

/// Fresh size hints for one eligible observed window: a new
/// `WM_GETMINMAXINFO` query converted from outer track pixels to visible
/// physical pixels with the window's currently measured frame insets.
/// Unknown on any query failure, timeout, budget skip, or invalid remainder:
/// no hint, never a reused or persistent floor. Both sides convert with the
/// same fresh insets; either side may be unknown while the other survives.
pub(super) fn min_hint_for(window: &ObservedWindow, cx: &HintCx) -> (WindowSizeHints, HintOutcome) {
    // Test seam first: injected converted hints stand in for the OS answer
    // (unit tests only; compiled out of production builds).
    #[cfg(test)]
    if let Some(injected) = test_hint_for(window.hwnd) {
        return injected;
    }
    let hwnd = window.hwnd as isize as HWND;
    let (min_track, max_track, outcome) = query_outer_tracks(hwnd, cx);
    if outcome != HintOutcome::Hint {
        return (WindowSizeHints::none(), outcome);
    }
    let hints = crate::tiling::hints_from_outer(min_track, max_track, window.insets);
    if hints.is_empty() {
        (hints, HintOutcome::Invalid)
    } else {
        (hints, HintOutcome::Hint)
    }
}

/// Fresh size hints for one verified managed hidden member without
/// showing or moving it: fresh identity fenced by the existing
/// `WindowKey` plus owned lifetime-property checks, then fresh `GetWindowRect`
/// plus `DWMWA_EXTENDED_FRAME_BOUNDS` for current insets, then a fresh
/// `WM_GETMINMAXINFO` query converted with those insets. Both minimum and
/// maximum sides convert with the same fresh insets; either side may be
/// unknown. Hidden DWM frames
/// may legitimately be absent; any measurement failure is unknown (no hint),
/// never a stale cached inset. The Engine row rectangle stays the stored
/// snapshot: fresh frames feed insets only, never adopted geometry.
pub(super) fn hidden_hint_for(
    state: &TileLoop,
    key: &crate::workspace::WindowKey,
    cx: &HintCx,
) -> (WindowSizeHints, HintOutcome) {
    if cx.over_budget() {
        return (WindowSizeHints::none(), HintOutcome::Budget);
    }
    let Some(stored) = state.member_identity.get(key).cloned() else {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    };
    let Some(stored_tag) = state.member_tags.get(key).cloned() else {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    };
    if key.hwnd == 0 {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    // Test seam after the member-state fences (which tests populate) but
    // before any OS handle use: injected converted hints stand in for the
    // live measurement plus query (unit tests only; compiled out of
    // production builds).
    #[cfg(test)]
    if let Some(injected) = test_hint_for(key.hwnd) {
        return injected;
    }
    let hwnd = key.hwnd as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let mut live_pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut live_pid);
    }
    if live_pid == 0 || live_pid != key.pid {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let held = match HeldProcess::open(live_pid) {
        Ok(held) => held,
        Err(_) => return (WindowSizeHints::none(), HintOutcome::Invalid),
    };
    let live = match held.identity() {
        Ok(live) => live,
        Err(_) => return (WindowSizeHints::none(), HintOutcome::Invalid),
    };
    if !crate::workspace_owner::member_matches(key, key.hwnd, live.pid, &live.process_creation)
        || live != stored
    {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
    if !crate::workspace_owner::visible_lifetime_ok(&stored_tag, live_tag.as_deref()) {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let mut outer_raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut outer_raw) } == 0 {
        return (WindowSizeHints::none(), HintOutcome::Failed);
    }
    let Some(outer) = rect_from_win(outer_raw) else {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    };
    let mut visible_raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut visible_raw as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } != 0
    {
        return (WindowSizeHints::none(), HintOutcome::Failed);
    }
    let Some(visible) = rect_from_win(visible_raw) else {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    };
    if !tiler_core::bounds::valid_carried_rect(visible.x, visible.y, visible.w, visible.h) {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let insets = FrameInsets::measure(outer, visible);
    if cx.over_budget() {
        return (WindowSizeHints::none(), HintOutcome::Budget);
    }
    let (min_track, max_track, outcome) = query_outer_tracks(hwnd, cx);
    if outcome != HintOutcome::Hint {
        return (WindowSizeHints::none(), outcome);
    }
    let hints = crate::tiling::hints_from_outer(min_track, max_track, insets);
    let outcome = if hints.is_empty() {
        HintOutcome::Invalid
    } else {
        HintOutcome::Hint
    };
    let out = if outcome == HintOutcome::Hint {
        hints
    } else {
        WindowSizeHints::none()
    };
    (out, outcome)
}
