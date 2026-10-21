#![forbid(unsafe_code)]
use super::*;

#[cfg(test)]
use super::hints::test_hint_for;

/// `SendMessageTimeoutW` timeout failure code: distinguishes a hung-window
/// abort from any other setter failure.
const ERROR_TIMEOUT: u32 = 1460;

/// Correctly seeded Rust `MINMAXINFO` for one window: minimum track from the
/// window's current DPI, maximum track plus maximized size/position per the
/// normal `WM_GETMINMAXINFO` contract. Seeded directly in Rust from supported
/// system-metrics APIs, never by copying another window's struct. An app that
/// leaves a field untouched therefore reports the system default instead of a
/// fabricated zero absence.
fn seed_minmaxinfo(hwnd: HWND) -> MINMAXINFO {
    crate::ffi::seed_minmax_info(crate::ffi::dpi_for_window(hwnd as usize as u64))
}

/// Fresh outer track sizes plus what the native attempt cost: `(minimum,
/// maximum, outcome, cost)` as raw query pairs. Normalization stays in one
/// place ([`crate::tiling::hints_from_outer`]
/// and [`crate::tiling::hints_from_outer`]); the query only strips
/// seeded-untouched maximums and reports transport outcomes. The maximum is
/// `None` when the query failed, hung, overran the budget, or matched the
/// seed (app-untouched, never app evidence).
type OuterTracks = (
    Option<(i32, i32)>,
    Option<(i32, i32)>,
    HintOutcome,
    QueryCost,
);

/// What one native `WM_GETMINMAXINFO` attempt actually cost: the timeout
/// offered to `SendMessageTimeoutW` recomputed at call time (the remaining
/// shared budget capped at the per-query bound, zero when already over
/// budget) plus the measured native round-trip. Both zero when no query was
/// attempted. Elapsed covers the native call only, never identity or frame
/// prework; prework skew is reported nowhere, never folded in silently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct QueryCost {
    pub(super) offered_ms: u32,
    pub(super) elapsed_us: u64,
}

/// One fresh-query attempt: converted hints, outcome, and what the native
/// query actually cost (zeroed when no query was attempted: budget,
/// identity, or measurement prework refused first, or the test seam stood
/// in for the OS answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FreshHint {
    pub(super) hints: WindowSizeHints,
    pub(super) outcome: HintOutcome,
    pub(super) cost: QueryCost,
}

impl FreshHint {
    /// Unperformed query with no usable hint and no cost: budget,
    /// identity, or measurement prework refused before any native call.
    pub(super) const fn unperformed(outcome: HintOutcome) -> Self {
        Self {
            hints: WindowSizeHints::none(),
            outcome,
            cost: QueryCost {
                offered_ms: 0,
                elapsed_us: 0,
            },
        }
    }
}

/// Fresh outer minimum/maximum track sizes for one window in physical pixels
/// plus its outcome.
///
/// `SendMessageTimeoutW` with `WM_GETMINMAXINFO`, `SMTO_ABORTIFHUNG`, and
/// [`MIN_HINT_TIMEOUT_MS`] over the correctly seeded struct above. A zero
/// return means failure or timeout and the struct is ignored entirely (never
/// read); `ERROR_TIMEOUT` distinguishes hung aborts from other failures.
/// Success with no usable track size is `NoHint`. Requires the loop's
/// verified PMv2 awareness so the track sizes are physical pixels.
///
/// The maximum side strips seeded-untouched values: an app that leaves
/// `ptMaxTrackSize` untouched reports the system default this file seeded,
/// which is never claimed as app evidence (it arrives as unknown, and
/// unknown maximums never classify under R-SPC-04 D1). No geometry, style,
/// or resizeable inference is involved.
fn query_outer_tracks(hwnd: HWND, cx: &HintCx) -> OuterTracks {
    let info = seed_minmaxinfo(hwnd);
    let seed_max = (info.ptMaxTrackSize.x, info.ptMaxTrackSize.y);
    // Actual offered timeout recomputed at the native call, not the stale
    // pre-read the row loop took before identity/frame prework.
    let timeout_ms = cx.timeout_ms();
    if timeout_ms == 0 {
        return (None, None, HintOutcome::Budget, QueryCost::default());
    }
    let started = Instant::now();
    let query = crate::ffi::query_minmax(hwnd as usize as u64, timeout_ms);
    let info = query.info;
    // Capture the error code before any other call: even timing reads must
    // not interleave between the native call and `GetLastError`, or a
    // Timeout could misreport as Failed (or vice versa).
    let code = query.error;
    // Native round-trip only: identity and frame prework ran before this
    // call and are never folded into the elapsed label.
    let cost = QueryCost {
        offered_ms: timeout_ms,
        elapsed_us: started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
    };
    if !query.sent {
        if code == ERROR_TIMEOUT {
            return (None, None, HintOutcome::Timeout, cost);
        }
        return (None, None, HintOutcome::Failed, cost);
    }
    let min = Some((info.ptMinTrackSize.x, info.ptMinTrackSize.y));
    let live_max = (info.ptMaxTrackSize.x, info.ptMaxTrackSize.y);
    let max = if live_max == seed_max {
        None
    } else {
        Some(live_max)
    };
    // Per-axis outcomes: any usable or explicitly-unset bound on either side
    // is a hint. A completed query with no usable track size on either side
    // is a successful empty (`NoHint`, which replaces last-good state), not
    // an unperformed failure: explicit zeros ride (unset per axis, G-05)
    // rather than failing the whole query; validity itself is decided once
    // at conversion ([`crate::tiling::hints_from_outer`]).
    let norm = |(w, h)| crate::tiling::normalize_min_track(w, h);
    let min_norm = min.map_or((None, None), norm);
    let max_norm = max.map_or((None, None), norm);
    let outcome = match (min_norm, max_norm) {
        ((None, None), (None, None)) => HintOutcome::NoHint,
        _ => HintOutcome::Hint,
    };
    (min, max, outcome, cost)
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
    if !crate::ffi::is_window(hwnd_u64) {
        return false;
    }
    let Some(outer_raw) = crate::ffi::window_rect(hwnd_u64) else {
        return false;
    };
    let Some(outer) = rect_from_win(outer_raw) else {
        return false;
    };
    let Some(visible_raw) = crate::ffi::extended_frame_bounds(hwnd_u64) else {
        return false;
    };
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
    let (min_track, max_track, outcome, _) = query_outer_tracks(hwnd, cx);
    if outcome != HintOutcome::Hint {
        return false;
    }
    let hints = crate::tiling::hints_from_outer(min_track, max_track, insets);
    tiler_core::size_hints::is_fixed_size_with(hints, predicate)
}

/// Fresh size hints for one eligible observed window: a new
/// `WM_GETMINMAXINFO` query converted from outer track pixels to visible
/// physical pixels with the window's currently measured frame insets.
/// Unknown on any query failure, timeout, or budget skip; a successful query
/// with no usable remainder reports successful-empty (`NoHint`, which clears
/// last-good state): no hint, never a reused or persistent floor. Both sides
/// convert with the same fresh insets; either side may be unknown while the
/// other survives. Reports the actual native cost alongside: zeroed when the
/// test seam stands in (no query attempted).
pub(super) fn min_hint_for(window: &ObservedWindow, cx: &HintCx) -> FreshHint {
    // Test seam first: injected converted hints stand in for the OS answer
    // (unit tests only; compiled out of production builds).
    #[cfg(test)]
    if let Some((hints, outcome)) = test_hint_for(window.hwnd) {
        return FreshHint {
            hints,
            outcome,
            cost: QueryCost::default(),
        };
    }
    let hwnd = window.hwnd as isize as HWND;
    let (min_track, max_track, outcome, cost) = query_outer_tracks(hwnd, cx);
    if outcome != HintOutcome::Hint {
        return FreshHint {
            hints: WindowSizeHints::none(),
            outcome,
            cost,
        };
    }
    let hints = crate::tiling::hints_from_outer(min_track, max_track, window.insets);
    if hints.is_empty() {
        FreshHint {
            hints,
            outcome: HintOutcome::NoHint,
            cost,
        }
    } else {
        FreshHint {
            hints,
            outcome,
            cost,
        }
    }
}

/// Fresh size hints for one verified managed hidden member without
/// showing or moving it: fresh identity fenced by the existing
/// `WindowKey` plus owned lifetime-property checks, then fresh `GetWindowRect`
/// plus `DWMWA_EXTENDED_FRAME_BOUNDS` for current insets, then a fresh
/// `WM_GETMINMAXINFO` query converted with those insets. Both minimum and
/// maximum sides convert with the same fresh insets; either side may be
/// unknown. Hidden DWM frames
/// may legitimately be absent; any measurement or identity failure is an
/// unperformed query (`Invalid`/`Failed`, never clearing last-good state),
/// never a stale cached inset. A completed query with no usable remainder
/// reports successful-empty (`NoHint`, which does clear it). The Engine row
/// rectangle stays the stored snapshot: fresh frames feed insets only, never
/// adopted geometry. Reports the actual native cost alongside: zeroed when
/// identity, measurement, or budget prework refuses before any native query
/// (or the test seam stands in).
pub(super) fn hidden_hint_for(
    state: &TileLoop,
    key: &crate::native_guards::WindowKey,
    cx: &HintCx,
) -> FreshHint {
    if cx.over_budget() {
        return FreshHint::unperformed(HintOutcome::Budget);
    }
    let Some(stored) = state.member_identity.get(key).cloned() else {
        return FreshHint::unperformed(HintOutcome::Invalid);
    };
    let Some(stored_tag) = state.member_tags.get(key).cloned() else {
        return FreshHint::unperformed(HintOutcome::Invalid);
    };
    if key.hwnd == 0 {
        return FreshHint::unperformed(HintOutcome::Invalid);
    }
    // Test seam after the member-state fences (which tests populate) but
    // before any OS handle use: injected converted hints stand in for the
    // live measurement plus query (unit tests only; compiled out of
    // production builds).
    #[cfg(test)]
    if let Some((hints, outcome)) = test_hint_for(key.hwnd) {
        return FreshHint {
            hints,
            outcome,
            cost: QueryCost::default(),
        };
    }
    let hwnd = key.hwnd as isize as HWND;
    if !crate::ffi::is_window(key.hwnd) {
        return FreshHint::unperformed(HintOutcome::Invalid);
    }
    let live_pid = crate::ffi::window_pid(key.hwnd);
    if live_pid == 0 || live_pid != key.pid {
        return FreshHint::unperformed(HintOutcome::Invalid);
    }
    let held = match HeldProcess::open(live_pid) {
        Ok(held) => held,
        Err(_) => return FreshHint::unperformed(HintOutcome::Invalid),
    };
    let live = match held.identity() {
        Ok(live) => live,
        Err(_) => return FreshHint::unperformed(HintOutcome::Invalid),
    };
    if !crate::native_guards::member_matches(key, key.hwnd, live.pid, &live.process_creation)
        || live != stored
    {
        return FreshHint::unperformed(HintOutcome::Invalid);
    }
    let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
    if !crate::native_guards::visible_lifetime_ok(&stored_tag, live_tag.as_deref()) {
        return FreshHint::unperformed(HintOutcome::Invalid);
    }
    let Some(outer_raw) = crate::ffi::window_rect(key.hwnd) else {
        return FreshHint::unperformed(HintOutcome::Failed);
    };
    let Some(outer) = rect_from_win(outer_raw) else {
        return FreshHint::unperformed(HintOutcome::Invalid);
    };
    let Some(visible_raw) = crate::ffi::extended_frame_bounds(key.hwnd) else {
        return FreshHint::unperformed(HintOutcome::Failed);
    };
    let Some(visible) = rect_from_win(visible_raw) else {
        return FreshHint::unperformed(HintOutcome::Invalid);
    };
    if !tiler_core::bounds::valid_carried_rect(visible.x, visible.y, visible.w, visible.h) {
        return FreshHint::unperformed(HintOutcome::Invalid);
    }
    let insets = FrameInsets::measure(outer, visible);
    if cx.over_budget() {
        return FreshHint::unperformed(HintOutcome::Budget);
    }
    let (min_track, max_track, outcome, cost) = query_outer_tracks(hwnd, cx);
    if outcome != HintOutcome::Hint {
        return FreshHint {
            hints: WindowSizeHints::none(),
            outcome,
            cost,
        };
    }
    let hints = crate::tiling::hints_from_outer(min_track, max_track, insets);
    let outcome = if hints.is_empty() {
        HintOutcome::NoHint
    } else {
        HintOutcome::Hint
    };
    let out = if outcome == HintOutcome::Hint {
        hints
    } else {
        WindowSizeHints::none()
    };
    FreshHint {
        hints: out,
        outcome,
        cost,
    }
}
