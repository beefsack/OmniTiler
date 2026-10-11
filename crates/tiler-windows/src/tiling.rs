//! Tiling-only Windows runtime over the retained portable Engine.
//!
//! Portable policy lives here and compiles everywhere; native enumeration,
//! actuation, and the event loop live in [`crate::tiling_sys`] (`cfg(windows)`
//! only). The Engine is the layout authority: this crate builds complete
//! `Reconcile` observations, applies `CoreReply` geometry verbatim, and never
//! adopts observed rectangles into retained state.

use std::collections::HashMap;
use std::path::PathBuf;

use tiler_core::boundary::{CoreCommand, CoreEvent};
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::seed::EngineWindow;
use tiler_core::session::{DomainKey, OutputDomain};

/// Shipped KDE parity: inner gap 8, outer gap 8 (`kwin/src/domain-gap.ts`).
pub const INNER_GAP: i32 = 8;
/// Shipped KDE parity: outer gap 8.
pub const OUTER_GAP: i32 = 8;
/// Single-display tiling-only output identity.
pub const OUTPUT_ID: &str = "display-0";
/// Implicit current workspace for the tiling-only slice.
pub const WORKSPACE_ID: &str = "workspace-0";
/// Stable Engine owner token for this adapter.
pub const OWNER_ID: &str = "tiler-windows";

/// Adapter-side domain bounds: the protocol layer owns the outer inset
/// (`tiler-protocol` applies `inset_bounds(carried, outer_gap)`), and this
/// adapter constructs `CoreEvent` directly, so inset here. `None` when the
/// work area cannot carry the margin: the caller skips the tick with retained
/// Engine state rather than tiling an un-inset domain.
#[must_use]
pub fn tiling_domain_bounds(work: Rect) -> Option<Rect> {
    tiling_domain_bounds_with(work, OUTER_GAP)
}

/// Outer-gap-parameterized domain inset for live settings: the protocol
/// layer owns the outer inset and this adapter constructs `CoreEvent`
/// directly, so inset here. `None` when the work area cannot carry the
/// margin: the caller skips the tick with retained Engine state rather than
/// tiling an un-inset domain.
#[must_use]
pub fn tiling_domain_bounds_with(work: Rect, outer_gap: i32) -> Option<Rect> {
    if outer_gap < 0 {
        return None;
    }
    tiler_core::geometry::inset_bounds(work, outer_gap).ok()
}

/// Measured per-app frame: outer `GetWindowRect` minus visible
/// `DWMWA_EXTENDED_FRAME_BOUNDS`, in physical pixels. All fields non-negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameInsets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl FrameInsets {
    /// Measure from one outer/visible pair; negative deltas saturate to zero
    /// (DWM rounding) rather than failing the tick.
    #[must_use]
    pub fn measure(outer: Rect, visible: Rect) -> Self {
        Self {
            left: visible.x.saturating_sub(outer.x).max(0),
            top: visible.y.saturating_sub(outer.y).max(0),
            right: (outer.x as i64 + outer.w as i64 - visible.x as i64 - visible.w as i64)
                .clamp(0, i64::from(i32::MAX)) as i32,
            bottom: (outer.y as i64 + outer.h as i64 - visible.y as i64 - visible.h as i64)
                .clamp(0, i64::from(i32::MAX)) as i32,
        }
    }

    /// Map an Engine visible rectangle back to the outer rectangle for
    /// `SetWindowPos`. `None` on arithmetic overflow: the caller skips the
    /// write for this tick instead of clamping geometry.
    #[must_use]
    pub fn visible_to_outer(self, visible: Rect) -> Option<Rect> {
        let x = visible.x.checked_sub(self.left)?;
        let y = visible.y.checked_sub(self.top)?;
        let w = visible.w.checked_add(self.left)?.checked_add(self.right)?;
        let h = visible.h.checked_add(self.top)?.checked_add(self.bottom)?;
        if w <= 0 || h <= 0 {
            return None;
        }
        Some(Rect { x, y, w, h })
    }
}

/// Validate one raw outer bound (`ptMinTrackSize`/`ptMaxTrackSize` axis) in
/// physical pixels, independently per axis (R-SPC-04 D1, G-05):
/// - `None` when the app declares nothing usable there: negative,
///   unbounded-sentinel (`i32::MAX`), or absurd out-of-bound values all map
///   to unknown, never to a floor or cap;
/// - `Some(0)` for an explicit zero: unset per axis, transported (not
///   dropped) so the shared predicate distinguishes partial-zero vectors
///   such as min=max=(640,0) from missing bounds;
/// - otherwise the positive in-bound size.
///
/// Minimum projection behavior is unchanged: the shared
/// [`tiler_core::size_hints`] `meaningful` filter treats `Some(0)` exactly
/// like absent everywhere except the fixed-size predicate.
#[must_use]
pub fn normalize_outer_bound(value: i32) -> Option<i32> {
    match value {
        v if v < 0 => None,
        0 => Some(0),
        v if (1..=tiler_core::bounds::GEOMETRY_BOUND).contains(&v) => Some(v),
        _ => None,
    }
}

/// Validate one raw outer minimum-track extent (`ptMinTrackSize`) in physical
/// pixels, independently per axis. Each axis maps through
/// [`normalize_outer_bound`]: usable sizes and explicit zeros survive per
/// axis, unknown axes map to `None` without discarding the usable sibling.
#[must_use]
pub fn normalize_min_track(outer_w: i32, outer_h: i32) -> (Option<i32>, Option<i32>) {
    (
        normalize_outer_bound(outer_w),
        normalize_outer_bound(outer_h),
    )
}

/// Convert one normalized outer bound into visible physical pixels by
/// subtracting the frame extent on its axis. Unknown stays unknown; an
/// explicit zero stays unset (`Some(0)`, no subtraction is meaningful); a
/// positive bound whose visible remainder vanishes inside its own frame maps
/// to unknown (`None`), never to a zero floor.
fn visible_bound(outer: Option<i32>, frame: i32) -> Option<i32> {
    match outer {
        None => None,
        Some(0) => Some(0),
        Some(v) => {
            let remainder = v.checked_sub(frame)?;
            if remainder >= 1 {
                Some(remainder)
            } else {
                None
            }
        }
    }
}

/// Convert a raw outer minimum-track size into visible physical pixels by
/// subtracting the currently measured frame insets, independently per axis.
/// An axis whose remainder vanishes inside its own frame carries no usable
/// constraint on that axis; the usable sibling axis still converts.
#[must_use]
pub fn visible_min_from_outer(
    outer_w: i32,
    outer_h: i32,
    insets: FrameInsets,
) -> (Option<i32>, Option<i32>) {
    let frame_w = insets.left.checked_add(insets.right).unwrap_or(i32::MAX);
    let frame_h = insets.top.checked_add(insets.bottom).unwrap_or(i32::MAX);
    let (outer_w, outer_h) = normalize_min_track(outer_w, outer_h);
    (
        visible_bound(outer_w, frame_w),
        visible_bound(outer_h, frame_h),
    )
}

/// Portable minimum-size hint for one window from its fresh outer
/// minimum-track size and currently measured frame insets. Timeout, failure,
/// or invalid data already maps to unknown at the native query, which lands
/// here as no hint: unknown means no hint, never a learned or persistent
/// floor. Only minimum bounds are ever set; maximums stay absent. Kept for
/// the minimum-only call sites; [`hints_from_outer`] carries both sides for
/// fixed-size admission.
#[must_use]
pub fn min_hints_from_outer(
    outer_w: i32,
    outer_h: i32,
    insets: FrameInsets,
) -> tiler_core::size_hints::WindowSizeHints {
    hints_from_outer(Some((outer_w, outer_h)), None, insets)
}

/// Validate one raw outer maximum-track extent (`ptMaxTrackSize`) in physical
/// pixels. Same per-axis rule as [`normalize_min_track`]. A seeded but
/// app-untouched maximum never reaches here: the native query strips values
/// identical to the seed before conversion, so a system default is never
/// claimed as app evidence and no style/resizeable inference is involved.
#[must_use]
pub fn normalize_max_track(outer_w: i32, outer_h: i32) -> (Option<i32>, Option<i32>) {
    (
        normalize_outer_bound(outer_w),
        normalize_outer_bound(outer_h),
    )
}

/// Convert a raw outer maximum-track size into visible physical pixels by
/// subtracting the currently measured frame insets, independently per axis.
/// Same rule as [`visible_min_from_outer`].
#[must_use]
pub fn visible_max_from_outer(
    outer_w: i32,
    outer_h: i32,
    insets: FrameInsets,
) -> (Option<i32>, Option<i32>) {
    let frame_w = insets.left.checked_add(insets.right).unwrap_or(i32::MAX);
    let frame_h = insets.top.checked_add(insets.bottom).unwrap_or(i32::MAX);
    let (outer_w, outer_h) = normalize_max_track(outer_w, outer_h);
    (
        visible_bound(outer_w, frame_w),
        visible_bound(outer_h, frame_h),
    )
}

/// Portable size hints for one window from its fresh outer minimum/maximum
/// track sizes and currently measured frame insets. Each side converts
/// independently with the same fresh DPI/frame-inset conversion, and each
/// axis converts independently within its side: a usable bound sets its
/// hint, an explicit zero rides as unset (`Some(0)`), and an unknown axis
/// stays unknown without discarding its usable sibling. Either side may be
/// unknown (`None`, timeout, failure, budget skip, or invalid remainders).
/// Unknown maximums never classify: the shared
/// [`tiler_core::size_hints::is_fixed_size_with`] predicate requires both
/// sides present, usable, and equal (nonzero per axis under G-05).
#[must_use]
pub fn hints_from_outer(
    min_outer: Option<(i32, i32)>,
    max_outer: Option<(i32, i32)>,
    insets: FrameInsets,
) -> tiler_core::size_hints::WindowSizeHints {
    let (min_w, min_h) =
        min_outer.map_or((None, None), |(w, h)| visible_min_from_outer(w, h, insets));
    let (max_w, max_h) =
        max_outer.map_or((None, None), |(w, h)| visible_max_from_outer(w, h, insets));
    tiler_core::size_hints::WindowSizeHints {
        min_w,
        min_h,
        max_w,
        max_h,
    }
}

/// Effective native target for an overconstrained tile: planned origin with
/// each extent raised to the known native minimum (`WM_GETMINMAXINFO` track
/// minus frame insets; unknown minimums keep the planned rect). Explicit so
/// readback matches the OS-enforced size instead of fighting every tick.
#[must_use]
pub fn overconstrained_effective(
    planned: Rect,
    hints: tiler_core::size_hints::WindowSizeHints,
) -> Rect {
    let w = hints
        .meaningful_min_w()
        .map_or(planned.w, |min| planned.w.max(min));
    let h = hints
        .meaningful_min_h()
        .map_or(planned.h, |min| planned.h.max(min));
    Rect {
        x: planned.x,
        y: planned.y,
        w,
        h,
    }
}

/// Stable opaque per-run window tokens (`w1`, `w2`, ...). Keyed by
/// `(HWND, process creation)` so a recycled HWND never inherits its
/// predecessor's token, even within the same tick. HWND values never enter
/// logs; tokens are the only window naming in production output.
#[derive(Debug, Default)]
pub struct TokenMap {
    next: u64,
    by_window: HashMap<(u64, String), String>,
}

impl TokenMap {
    #[must_use]
    pub fn token_for(&mut self, hwnd: u64, process_creation: &str) -> String {
        let key = (hwnd, process_creation.to_owned());
        if let Some(token) = self.by_window.get(&key) {
            return token.clone();
        }
        self.next += 1;
        let token = format!("w{}", self.next);
        self.by_window.insert(key, token.clone());
        token
    }

    /// Mint a fresh token for an already-known `(HWND, creation)` pair after
    /// a same-process HWND reuse repair: the new window generation must never
    /// inherit the previous generation's Engine identity, layout slot, or
    /// focus. The caller patches the current tick's rows to the returned
    /// token; later ticks mint it back stably via [`TokenMap::token_for`].
    #[must_use]
    pub fn reissue(&mut self, hwnd: u64, process_creation: &str) -> String {
        self.next += 1;
        let token = format!("w{}", self.next);
        self.by_window
            .insert((hwnd, process_creation.to_owned()), token.clone());
        token
    }

    /// Forget entries for windows no longer enumerated so the map stays
    /// bounded. Identity is `(hwnd, creation)` pairs, never HWND alone.
    pub fn retain(&mut self, live: &[ObservedTargetRef<'_>]) {
        self.by_window.retain(|(hwnd, creation), _| {
            live.iter()
                .any(|t| t.hwnd == *hwnd && t.creation == creation)
        });
    }
}

/// Borrowed identity reference for [`TokenMap::retain`].
pub struct ObservedTargetRef<'a> {
    pub hwnd: u64,
    pub creation: &'a str,
}

/// Deterministic FNV-1a 64-bit fingerprint over sorted `(token, rect)` pairs.
#[must_use]
pub fn fingerprint(windows: &[(String, Rect)]) -> u64 {
    let mut sorted: Vec<(&String, &Rect)> = windows.iter().map(|(t, r)| (t, r)).collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut hash: u64 = 0xcbf29ce484222325;
    for (token, rect) in sorted {
        for byte in token.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for word in [rect.x, rect.y, rect.w, rect.h] {
            for byte in word.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    hash
}

/// Pre-queried native facts for one top-level window. All fields are plain
/// observed state; policy in [`classify`] decides eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowFacts {
    pub visible: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub cloaked: bool,
    pub elevated: bool,
    pub shell: bool,
    pub tool_window: bool,
    pub owned: bool,
    pub captionless_fullscreen: bool,
    pub no_activate: bool,
    /// Generic Win32 dialog class (`#32770`) without an owner window, plus
    /// the project's own Settings UI (own executable plus
    /// [`OWN_SETTINGS_WINDOW_CLASS`], set by the native gates).
    /// Owned dialogs are reported as `OwnedDialog` instead.
    pub dialog: bool,
}

/// Closed skip vocabulary for ineligible windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Hidden,
    Minimized,
    Maximized,
    Cloaked,
    Elevated,
    Shell,
    Tool,
    OwnedDialog,
    /// Unowned generic dialog (`#32770`) or the project's own Settings UI
    /// (own executable plus settings class): never a tile target.
    Dialog,
    Fullscreen,
    NoActivate,
    Unreadable,
    /// Frozen-allowlist identity stopped matching (vanished HWND, recycled
    /// HWND/PID, or foreign process): a true unknown, never a counted skip.
    /// Produced only by the stateless inspection verdict, never by `classify`.
    IdentityChanged,
}

impl SkipReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Minimized => "minimized",
            Self::Maximized => "maximized",
            Self::Cloaked => "cloaked",
            Self::Elevated => "elevated",
            Self::Shell => "shell",
            Self::Tool => "tool",
            Self::OwnedDialog => "owned-dialog",
            Self::Dialog => "dialog",
            Self::Fullscreen => "fullscreen",
            Self::NoActivate => "no-activate",
            Self::Unreadable => "unreadable",
            Self::IdentityChanged => "identity-changed",
        }
    }
}

/// Native window class of the project's own Settings UI. Must stay in sync
/// with `crate::settings_ui::SETTINGS_WINDOW_CLASS` (duplicated here so this
/// portable policy compiles everywhere).
pub const OWN_SETTINGS_WINDOW_CLASS: &str = "OmniTilerSettings";

/// True when a top-level window is the project's own Settings UI: its class
/// is the settings class and its executable matches the owner's exactly
/// (slash/case-insensitive, same [`crate::lifecycle::exe_paths_equal`]
/// identity the owner checks use). Class alone never excludes: a foreign app
/// reusing the class name stays eligible, and our own executable with any
/// other class stays eligible. Native gates classify this as a dialog so the
/// existing skip/recovery fences keep it unmanaged.
#[must_use]
pub fn is_own_settings_window(class: &str, exe_path: &str, owner_exe_path: &str) -> bool {
    class == OWN_SETTINGS_WINDOW_CLASS
        && crate::lifecycle::exe_paths_equal(exe_path, owner_exe_path)
}

/// Eligibility gate. Terminal windows are ordinary tile targets (KDE
/// parity): no ancestry Schrödinger here. Dimensions alone never classify
/// fullscreen: `captionless_fullscreen` requires a captionless style covering
/// the monitor.
pub fn classify(facts: &WindowFacts) -> Result<(), SkipReason> {
    if !facts.visible {
        return Err(SkipReason::Hidden);
    }
    if facts.minimized {
        return Err(SkipReason::Minimized);
    }
    if facts.cloaked {
        return Err(SkipReason::Cloaked);
    }
    if facts.elevated {
        return Err(SkipReason::Elevated);
    }
    if facts.shell {
        return Err(SkipReason::Shell);
    }
    if facts.tool_window {
        return Err(SkipReason::Tool);
    }
    if facts.owned {
        return Err(SkipReason::OwnedDialog);
    }
    if facts.dialog {
        return Err(SkipReason::Dialog);
    }
    // Overlay states last, fullscreen first: matches `overlay_refusal` so a
    // window holding both reports fullscreen everywhere. Safety skips above
    // keep their relative order.
    if facts.captionless_fullscreen {
        return Err(SkipReason::Fullscreen);
    }
    if facts.maximized {
        return Err(SkipReason::Maximized);
    }
    if facts.no_activate {
        return Err(SkipReason::NoActivate);
    }
    Ok(())
}

/// Maximized-overlay refusal for directional and pointer routes (KDE
/// `move-refused-maximize` / `pointer-refused-maximize` parity): a
/// maximized focused window keeps its tile slot but directional move/resize
/// on it would change its retained position/share, so it is refused
/// fail-closed before any Engine mutation. Focus carries no geometry write
/// and stays allowed. Fullscreen wins when both overlay states hold (item 4
/// owns the fullscreen toggle; this only orders the refusal vocabulary).
#[must_use]
pub const fn overlay_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("fullscreen")
    } else if maximized {
        Some("maximize")
    } else {
        None
    }
}

/// Intentional-float toggle refusal for an overlay target (KDE
/// `float-refused-fullscreen` / `float-refused-maximize` parity): a
/// fullscreen or maximized focused window never floats, and a focused float
/// that the user maximized or fullscreened natively never unfloats until it
/// reads normal again. Fullscreen wins when both hold. `None` when the route
/// may proceed.
#[must_use]
pub const fn float_toggle_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("float-refused-fullscreen")
    } else if maximized {
        Some("float-refused-maximize")
    } else {
        None
    }
}

/// Sticky-toggle refusal for an overlay target (KDE `sticky-refused-fullscreen`
/// / `sticky-refused-maximize` parity): a fullscreen or maximized focused
/// window never sticks, and a sticky window that the user maximized or
/// fullscreened natively never unsticks until it reads normal again.
/// Fullscreen wins when both hold. `None` when the route may proceed.
#[must_use]
pub const fn sticky_toggle_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("sticky-refused-fullscreen")
    } else if maximized {
        Some("sticky-refused-maximize")
    } else {
        None
    }
}

/// Orientation-toggle refusal for an overlay target (item 4, R-LAY-01): a
/// fullscreen or maximized focused window never toggles its parent axis,
/// matching the move/float/sticky overlay isolation. Fullscreen wins when
/// both hold. `None` when the route may proceed.
#[must_use]
pub const fn orientation_toggle_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("orientation-refused-fullscreen")
    } else if maximized {
        Some("orientation-refused-maximize")
    } else {
        None
    }
}

/// Directional refusal for a sticky subject window (KDE sticky isolation
/// parity): a sticky focused window never starts directional focus/move, and
/// sticky windows are never directional targets (they ride slotless floats).
/// Focus and move share the sticky subject gate; targets exclude via the
/// floating Engine observation. `None` when the route may proceed.
#[must_use]
pub const fn sticky_directional_refusal(is_sticky: bool, is_move: bool) -> Option<&'static str> {
    if !is_sticky {
        return None;
    }
    if is_move {
        Some("move-refused-sticky")
    } else {
        Some("focus-refused-sticky")
    }
}

/// Sticky marker value for one pre-sticky float state: 1 when the window was
/// tiled before sticky-on, 2 when it was already a normal float. Nonzero
/// magic only; never a pointer, never trusted across window generations (a
/// recycled HWND starts without our property).
#[must_use]
pub const fn sticky_marker_value(prior_floating: bool) -> u64 {
    if prior_floating { 2 } else { 1 }
}

/// Decode one sticky marker value to its pre-sticky float state. `None` for
/// absent, zero, or unknown values: fail closed, never guess.
#[must_use]
pub const fn parse_sticky_marker(value: u64) -> Option<bool> {
    match value {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

/// Intentional-float marker value: presence with 1 means explicit float.
/// Nonzero magic only; never a pointer, never trusted across window
/// generations (a recycled HWND starts without our property).
#[must_use]
pub const fn float_intent_marker_value() -> u64 {
    1
}

/// Decode one intentional-float marker value. `Some(())` only for 1:
/// absent, zero, or unknown values fail closed, never guess.
#[must_use]
pub const fn parse_float_intent_marker(value: u64) -> Option<()> {
    match value {
        1 => Some(()),
        _ => None,
    }
}

/// Reserved fixed-window tile-override marker value (item 13 D7): presence
/// with 1 will mean a user tile override. Codec only; no admission path
/// reads it in item 8.
#[must_use]
pub const fn tile_override_marker_value() -> u64 {
    1
}

/// Decode one reserved tile-override marker value. `Some(())` only for 1;
/// everything else fails closed. Reserved; never floats a window in item 8.
#[must_use]
pub const fn parse_tile_override_marker(value: u64) -> Option<()> {
    match value {
        1 => Some(()),
        _ => None,
    }
}

/// Project-owned topmost restore gate for graceful stop and unfloat: only a
/// band the project raised (`!prior && current`) restores. A pre-existing
/// topmost stays untouched even if the user later cleared it; crash leaves
/// every frame in place and restart resets this runtime-local preimage.
#[must_use]
pub const fn float_topmost_restore_needed(prior_topmost: bool, current_topmost: bool) -> bool {
    !prior_topmost && current_topmost
}

/// Retained canonical rectangle for an overlay member (KDE carried-snapshot
/// parity): a maximized/fullscreen member rides its last-known tile
/// rectangle instead of the compositor-owned native maximum frame, so tile
/// topology and sibling shares survive the overlay. Falls back to the fresh
/// frame only when no retained allocation exists yet (first sighting).
#[must_use]
pub const fn canonical_retained_rect(overlay: bool, fresh: Rect, retained: Option<Rect>) -> Rect {
    match (overlay, retained) {
        (true, Some(rect)) => rect,
        _ => fresh,
    }
}

/// Bound for the prompt-restore owner-loop wake after one async dispatch.
pub const RESTORE_WAKE_MS: u64 = 2000;

/// Last-known minimum hint for a retained tiled overlay member (maximized or
/// fullscreen sharing this path): the fresh `WM_GETMINMAXINFO` query is
/// unavailable while overlaid, so the last-known declared hint rides the
/// canonical slot until a normal fresh query resumes. Never derived from the
/// maximized frame, never stale: `None`/empty when the token moved on
/// (recycled HWND mints a fresh token), when the member floats or holds a
/// born-fullscreen slotless row, or when no canonical slot exists yet.
#[must_use]
pub fn retained_overlay_hint(
    logged: Option<tiler_core::size_hints::WindowSizeHints>,
    token_matches: bool,
    overlay: bool,
    is_float: bool,
    born_hold: bool,
    has_slot: bool,
) -> tiler_core::size_hints::WindowSizeHints {
    if !token_matches || !overlay || is_float || born_hold || !has_slot {
        return tiler_core::size_hints::WindowSizeHints::none();
    }
    match logged {
        Some(hints) if !hints.is_empty() => hints,
        _ => tiler_core::size_hints::WindowSizeHints::none(),
    }
}

/// Arm only on a restore request whose async setter is still pending.
#[must_use]
pub fn restore_wake_arm(outcome: &str, wanted_restored: bool) -> bool {
    wanted_restored && outcome == "dispatched"
}

/// One pump of the armed restore wake. Returns `(keep, woke)`: `woke`
/// demands a reconcile once completion is observed; `keep` survives pending
/// dispatches and gesture/suspend pauses and clears only after a gated
/// reconcile consumes it, on expiry, or on window/identity loss.
#[must_use]
pub const fn restore_wake_step(
    armed: bool,
    expired: bool,
    gone: bool,
    observed_done: bool,
    reconciled: bool,
) -> (bool, bool) {
    if !armed || expired || gone {
        return (false, false);
    }
    if observed_done {
        return (!reconciled, true);
    }
    (true, false)
}

/// One-shot maximize-clear gate for first admission (KDE
/// `maximize-admission-clear` parity): the first non-fullscreen admission of
/// a maximized window without a retained tiled slot restores the native
/// maximize exactly once with no automatic retry. Fullscreen never clears,
/// already-slotted members never re-clear, and an attempted identity never
/// retries. Floating workspaces never clear (KDE floating-gate parity): the
/// maximum is preserved until the first tiled admission.
#[must_use]
pub const fn should_clear_maximize_at_admission(
    fullscreen: bool,
    maximized: bool,
    known_slot: bool,
    attempted: bool,
    tiled: bool,
) -> bool {
    should_clear_maximize_at_admission_with_mode(
        fullscreen, maximized, known_slot, attempted, tiled, false,
    )
}

/// R-MAX-06 mode-aware form of [`should_clear_maximize_at_admission`]:
/// `keep_overlay` (latched at first-seen tiled admission) keeps
/// the native maximize over the reserved tile slot instead of clearing once,
/// so first-seen maximized tiled admission converges through the existing
/// retained-slot overlay machinery with no native clear. Every other gate is
/// unchanged: fullscreen still never clears, slotted members never re-clear,
/// attempts never retry, and floating domains still defer.
#[must_use]
pub const fn should_clear_maximize_at_admission_with_mode(
    fullscreen: bool,
    maximized: bool,
    known_slot: bool,
    attempted: bool,
    tiled: bool,
    keep_overlay: bool,
) -> bool {
    !keep_overlay && tiled && !fullscreen && maximized && !known_slot && !attempted
}

/// R-MAX-06 born-maximized mode predicate over the raw wire value: true only
/// for the exact `keep-maximized-overlay` token. Unknown hand-built values
/// tile (fail-closed to the default); only validated live settings reach the
/// production gate, where anything invalid keeps last-good instead.
#[must_use]
pub fn born_maximized_keep_overlay(value: &str) -> bool {
    value == crate::settings::BORN_MAXIMIZED_OVERLAY
}

/// R-MAX-06 latch decision at slotless admission: bind keep-overlay to the
/// exact fresh lifetime only for tiled-first admissions under the live
/// overlay value. Floating-first (R-MAX-03 lane), pre-existing, and
/// born-fullscreen lifetimes never latch, so their later tiled admission
/// keeps the existing one-shot clear regardless of the live setting. A later
/// live flip cannot move an already-bound lifetime: the stored binding, not
/// the live value, gates its clear.
#[must_use]
pub const fn born_keep_latch(tiled_first: bool, keep_setting: bool) -> bool {
    tiled_first && keep_setting
}

/// Slotless maximized admission rule (R-MAX-03): a first-seen maximized,
/// non-fullscreen retained window without workspace membership and without a
/// retained tile slot joins its workspace slotless (membership for
/// hide/reveal and focus, no tile slot, no Engine exception), so a floating
/// workspace preserves its native frame until the first tiled admission
/// clears it once.
#[must_use]
pub const fn should_admit_slotless_maximized(
    maximized: bool,
    fullscreen: bool,
    is_member: bool,
    has_slot: bool,
) -> bool {
    maximized && !fullscreen && !is_member && !has_slot
}

/// Retained-slot seed rule for row assembly: a slotless maximized row on a
/// floating domain keeps no tile slot, so the deferred admission clear still
/// sees it slotless on retile. Every other combination seeds or refreshes
/// the last-known rectangle as before.
#[must_use]
pub const fn should_seed_member_slot(domain_tiled: bool, maximized: bool, has_slot: bool) -> bool {
    domain_tiled || !maximized || has_slot
}

/// Flag-stability gate before a workspace-send membership transfer (KDE
/// `flagsStillMatch` parity): the mover's live overlay flags must still
/// equal the dispatch snapshot, or the transfer refuses with no writes.
#[must_use]
pub const fn send_flags_stable(
    snapshot_fullscreen: bool,
    snapshot_maximized: bool,
    fresh_fullscreen: bool,
    fresh_maximized: bool,
) -> bool {
    snapshot_fullscreen == fresh_fullscreen && snapshot_maximized == fresh_maximized
}

/// Focus eligibility gate (KDE `requestFocus` parity for overlays): focus
/// carries no geometry write, so a maximized or fullscreen member stays
/// focusable while every other ineligible state still refuses; geometry
/// classification is untouched. Every gate except the two overlays is
/// checked, so an overlay window that is also minimized/no-activate/etc
/// still refuses for that other reason.
pub fn classify_focus(facts: &WindowFacts) -> Result<(), SkipReason> {
    if !facts.visible {
        return Err(SkipReason::Hidden);
    }
    if facts.minimized {
        return Err(SkipReason::Minimized);
    }
    if facts.cloaked {
        return Err(SkipReason::Cloaked);
    }
    if facts.elevated {
        return Err(SkipReason::Elevated);
    }
    if facts.shell {
        return Err(SkipReason::Shell);
    }
    if facts.tool_window {
        return Err(SkipReason::Tool);
    }
    if facts.owned {
        return Err(SkipReason::OwnedDialog);
    }
    if facts.dialog {
        return Err(SkipReason::Dialog);
    }
    if facts.no_activate {
        return Err(SkipReason::NoActivate);
    }
    Ok(())
}

/// Project-owned fullscreen toggle decision for one verified managed window.
/// Our restoration metadata is authoritative: exiting needs that preimage, so
/// a press on any owned frame exits even when the frame no longer covers a
/// monitor (external move or partial restore). Entering is always
/// project-owned (the toggle stores its own restoration preimage first); an
/// app-requested fullscreen frame carries no known preimage, so a command
/// exit refuses with a bounded reason and the frame stays app-owned: never
/// synthesize input, never guess restoration. Re-entry never overwrites an
/// existing preimage: owned frames always exit first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenToggle {
    /// Window is not fullscreen: enter via the owned style/frame route.
    Enter,
    /// Window is fullscreen with our restoration metadata: restore it.
    ExitOwned,
    /// Window is fullscreen without our metadata: refuse, leave app-owned.
    RefuseAppOwned,
}

impl FullscreenToggle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::ExitOwned => "exit-owned",
            Self::RefuseAppOwned => "refuse-app-owned",
        }
    }
}

/// Decide the toggle direction from fresh fullscreen state plus the presence
/// of our restoration metadata. Owned metadata wins over geometry: an owned
/// frame exits even when not monitor-covering. Pure so the command-exit
/// refusal pins without native calls.
#[must_use]
pub const fn fullscreen_toggle_decision(fullscreen: bool, owned: bool) -> FullscreenToggle {
    if owned {
        FullscreenToggle::ExitOwned
    } else if !fullscreen {
        FullscreenToggle::Enter
    } else {
        FullscreenToggle::RefuseAppOwned
    }
}

/// Fresh per-intent gate for the native toggle arms (maximize, fullscreen,
/// float, sticky): suspended sessions and elevated foregrounds settle without
/// side effects, matching the workspace dispatcher. Suspension wins when both
/// hold. `None` when the intent may proceed to its fresh revalidation. Pure
/// so the priority pins without native calls; callers pass fresh
/// `suspend_read(...).veto.block` and `foreground_elevated(...)` reads, whose
/// owned-fullscreen exemption keeps exiting our own fullscreen working.
#[must_use]
pub const fn toggle_gate_outcome(suspended: bool, elevated: bool) -> Option<&'static str> {
    if suspended {
        Some("suspended")
    } else if elevated {
        Some("elevated-foreground")
    } else {
        None
    }
}

/// Born-fullscreen hold gate (KDE initial-fullscreen-hold parity): a
/// first-seen fullscreen window without a retained tile slot is tracked as a
/// slotless planner-only exception until its first exit, never admitted. A
/// window with a retained slot is a managed overlay transition, and a window
/// already seen non-fullscreen in this lifetime never becomes born again.
#[must_use]
pub const fn should_hold_born_fullscreen(
    fullscreen: bool,
    known_slot: bool,
    seen_nonfullscreen: bool,
) -> bool {
    fullscreen && !known_slot && !seen_nonfullscreen
}

/// Stateless pre-frame verdict for one allowlisted entry: frozen-identity
/// matching plus window state, available before (and without) any frame
/// query. This is the production decision path for the inspection report
/// when geometry is absent: legitimately hidden (passive) or minimized exact
/// helpers keep full identity match with an accurate skip, while a
/// non-matching identity is a true unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatelessVerdict {
    /// Verdict for this pass: report identity state with this skip and
    /// perform no geometry read and no actuation.
    Report {
        identity_match: bool,
        eligible: bool,
        skip: SkipReason,
    },
    /// Identity matched a visible, non-minimized window: frame queries and
    /// `classify` are still required.
    NeedsGeometry,
}

/// Map `(identity_match, visible, minimized)` to the inspection verdict.
/// Mismatch beats visibility (a foreign window at a frozen HWND is unknown
/// even when visible); hidden beats minimized; minimized needs no frame.
#[must_use]
pub const fn inspect_stateless_verdict(
    identity_match: bool,
    visible: bool,
    minimized: bool,
) -> StatelessVerdict {
    if !identity_match {
        return StatelessVerdict::Report {
            identity_match: false,
            eligible: false,
            skip: SkipReason::IdentityChanged,
        };
    }
    if !visible {
        return StatelessVerdict::Report {
            identity_match: true,
            eligible: false,
            skip: SkipReason::Hidden,
        };
    }
    if minimized {
        return StatelessVerdict::Report {
            identity_match: true,
            eligible: false,
            skip: SkipReason::Minimized,
        };
    }
    StatelessVerdict::NeedsGeometry
}

/// Portable observed target identity for frozen-allowlist matching.
/// `tag` is the owned-helper lifetime token (`OmniTilerLifetime`
/// property): nonempty for owned helpers, required in every allowlist entry
/// so a recycled HWND in the same process never matches its predecessor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedTarget {
    pub hwnd: u64,
    pub pid: u32,
    pub process_creation: String,
    pub exe_path: String,
    pub user_sid: String,
    pub session_id: u32,
    pub tag: String,
}

/// One frozen allowlist entry, captured at test start. `tag` is required:
/// empty tags are malformed and refused before any lease or write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowEntry {
    pub hwnd: u64,
    pub pid: u32,
    pub process_creation: String,
    pub exe_path: String,
    pub user_sid: String,
    pub session_id: u32,
    pub tag: String,
}

/// Parse a test-mode allowlist receipt. Malformed, empty, duplicate-HWND,
/// empty-identity, or empty-tag input is an error; the caller must never fall
/// back to normal tiling on a bad allowlist.
pub fn parse_allowlist(json: &str) -> Result<Vec<AllowEntry>, String> {
    #[derive(serde::Deserialize)]
    struct Receipt {
        windows: Vec<AllowEntryJson>,
    }
    #[derive(serde::Deserialize)]
    struct AllowEntryJson {
        hwnd: u64,
        pid: u32,
        process_creation: String,
        exe_path: String,
        user_sid: String,
        session_id: u32,
        #[serde(default)]
        tag: String,
    }
    let receipt: Receipt =
        serde_json::from_str(json).map_err(|_| "refuse: malformed allowlist".to_owned())?;
    if receipt.windows.is_empty() {
        return Err("refuse: empty allowlist".to_owned());
    }
    if receipt.windows.iter().any(|w| {
        w.hwnd == 0
            || w.pid == 0
            || w.process_creation.is_empty()
            || w.exe_path.is_empty()
            || w.user_sid.is_empty()
            || w.tag.is_empty()
    }) {
        return Err("refuse: malformed allowlist".to_owned());
    }
    let mut seen = std::collections::HashSet::new();
    for window in &receipt.windows {
        if !seen.insert(window.hwnd) {
            return Err("refuse: duplicate allowlist hwnd".to_owned());
        }
    }
    Ok(receipt
        .windows
        .into_iter()
        .map(|w| AllowEntry {
            hwnd: w.hwnd,
            pid: w.pid,
            process_creation: w.process_creation,
            exe_path: w.exe_path,
            user_sid: w.user_sid,
            session_id: w.session_id,
            tag: w.tag,
        })
        .collect())
}

/// Deterministic FNV-1a 64-bit digest over the frozen allowlist in HWND order.
/// Binds hwnd/pid/creation/exe/sid/session/tag; empty input digests as zero.
#[must_use]
pub fn allowlist_digest(entries: &[AllowEntry]) -> String {
    let mut sorted: Vec<&AllowEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| e.hwnd);
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    for e in sorted {
        mix(&e.hwnd.to_le_bytes());
        mix(&e.pid.to_le_bytes());
        mix(e.process_creation.as_bytes());
        mix(e
            .exe_path
            .to_ascii_lowercase()
            .replace('/', "\\")
            .as_bytes());
        mix(e.user_sid.as_bytes());
        mix(&e.session_id.to_le_bytes());
        mix(e.tag.as_bytes());
    }
    format!("{hash:016x}")
}

/// Exact frozen-identity match: HWND plus full process identity plus the
/// owned-helper lifetime tag. Exe paths compare slash/case-insensitively like
/// the owner checks. A recycled HWND in the same process carries a fresh tag
/// and never matches.
#[must_use]
pub fn allow_match(entry: &AllowEntry, observed: &ObservedTarget) -> bool {
    entry.hwnd == observed.hwnd
        && entry.pid == observed.pid
        && entry.process_creation == observed.process_creation
        && crate::lifecycle::exe_paths_equal(&entry.exe_path, &observed.exe_path)
        && entry.user_sid == observed.user_sid
        && entry.session_id == observed.session_id
        && entry.tag == observed.tag
        && !entry.tag.is_empty()
}

/// Per-window write memory with two narrow lanes and no learned policy:
///
/// - `clamp`: a successful write whose readback still mismatches (the app
///   holds its own size). The identical intent is skipped until the desired
///   rectangle or the native observation genuinely changes.
/// - `transient`: a failed setter API call. Retried with bounded backoff
///   (500ms doubling, capped at 5s); any intent/observation change re-arms
///   immediately, so transient failures always recover.
#[derive(Debug, Default)]
pub struct RefusedTracker {
    clamp: HashMap<String, (Rect, Rect)>,
    transient: HashMap<String, TransientEntry>,
}

/// Backoff base for transient setter failures.
pub const TRANSIENT_BASE_MS: u64 = 500;
/// Backoff cap for transient setter failures.
pub const TRANSIENT_MAX_MS: u64 = 5000;

#[derive(Debug, Clone)]
struct TransientEntry {
    desired: Rect,
    observed: Rect,
    failures: u32,
    last: std::time::Instant,
}

impl RefusedTracker {
    /// `true` when this exact `(desired, observed)` pair is currently
    /// suppressed: an identical app-clamp refusal, or a transient failure
    /// still inside its backoff window.
    #[must_use]
    pub fn should_skip(
        &self,
        token: &str,
        desired: &Rect,
        observed: &Rect,
        now: std::time::Instant,
    ) -> bool {
        if self
            .clamp
            .get(token)
            .is_some_and(|(d, o)| d == desired && o == observed)
        {
            return true;
        }
        if let Some(entry) = self.transient.get(token)
            && entry.desired == *desired
            && entry.observed == *observed
        {
            let shift = entry.failures.min(4);
            let backoff = (TRANSIENT_BASE_MS << shift).min(TRANSIENT_MAX_MS);
            if now.duration_since(entry.last).as_millis() < u128::from(backoff) {
                return true;
            }
        }
        false
    }

    /// Exact application clears both lanes.
    pub fn note_match(&mut self, token: &str) {
        self.clamp.remove(token);
        self.transient.remove(token);
    }

    /// Successful write, readback still mismatches: app-held size.
    pub fn note_clamp(&mut self, token: &str, desired: &Rect, observed: &Rect) {
        if desired == observed {
            self.note_match(token);
        } else {
            self.clamp.insert(token.to_owned(), (*desired, *observed));
        }
    }

    /// Failed setter API call: bounded-backoff lane.
    pub fn note_transient(
        &mut self,
        token: &str,
        desired: &Rect,
        observed: &Rect,
        now: std::time::Instant,
    ) {
        let failures = self
            .transient
            .get(token)
            .filter(|entry| entry.desired == *desired && entry.observed == *observed)
            .map_or(0, |entry| entry.failures.saturating_add(1));
        self.transient.insert(
            token.to_owned(),
            TransientEntry {
                desired: *desired,
                observed: *observed,
                failures,
                last: now,
            },
        );
    }

    /// Forget tokens for windows no longer managed.
    pub fn retain(&mut self, live_tokens: &[&str]) {
        self.clamp
            .retain(|token, _| live_tokens.contains(&token.as_str()));
        self.transient
            .retain(|token, _| live_tokens.contains(&token.as_str()));
    }
}

/// Post-write readback decision for one desired entry. Only a window that
/// was actually written this tick may enter the app-clamp lane; a mismatch
/// with no write behind it stays pending so transient backoff and re-reads
/// keep working instead of being pinned as a permanent clamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadbackOutcome {
    Match,
    Clamp,
    Pending,
}

/// Decide the tracker update for one `(desired, readback)` pair.
/// `wrote_this_tick` must be true only when this tick's setter succeeded
/// for the token; a failed or skipped setter is never a clamp.
#[must_use]
pub const fn readback_outcome(
    wrote_this_tick: bool,
    desired: &Rect,
    readback: &Rect,
) -> ReadbackOutcome {
    if desired.x == readback.x
        && desired.y == readback.y
        && desired.w == readback.w
        && desired.h == readback.h
    {
        ReadbackOutcome::Match
    } else if wrote_this_tick {
        ReadbackOutcome::Clamp
    } else {
        ReadbackOutcome::Pending
    }
}

/// Same-output fence for a settle-time drop point: the release cursor must
/// still sit inside the source domain bounds. Outside (including another
/// output) refuses with snap-back and no transfer; cross-output placement
/// belongs to a later item. Relocated from the discarded Win-drag producer;
/// the title-bar path uses the same fence.
#[must_use]
pub fn drop_point_in_domain(
    bounds_x: i32,
    bounds_y: i32,
    bounds_w: i32,
    bounds_h: i32,
    x: i32,
    y: i32,
) -> bool {
    if bounds_w <= 0 || bounds_h <= 0 {
        return false;
    }
    let dx = i64::from(x) - i64::from(bounds_x);
    let dy = i64::from(y) - i64::from(bounds_y);
    dx >= 0 && dy >= 0 && dx < i64::from(bounds_w) && dy < i64::from(bounds_h)
}

/// One settled manual gesture, derived from pre/post-gesture rectangles plus
/// the settle-time cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureIntent {
    /// Position changed: re-place at the cursor drop point (KDE drag-drop).
    MoveDrop { x: i32, y: i32 },
    /// Size changed: pointer-resize the grabbed edges (KDE pointer-resize).
    /// Horizontal axis first, matching the KDE dual-axis route.
    PointerResize {
        direction: &'static str,
        boundary: i32,
        direction2: Option<&'static str>,
        boundary2: Option<i32>,
    },
}
/// Derive the settle intent from the stable pre-gesture rectangle and the
/// post-gesture rectangle plus the settle-time cursor position.
///
/// Same size at a new origin is a true move (KDE drag-drop). Any size change
/// makes it a resize: a changed origin means the left/top edge moved, never
/// a move. Edges derive per axis from before/post; when both edges of one
/// axis moved, the larger-magnitude edge wins (ties go to the origin edge).
/// `None` when nothing changed, the cursor is missing for a move, or a
/// boundary overflows `i32`.
#[must_use]
pub fn classify_gesture(
    before: &Rect,
    after: &Rect,
    cursor: Option<(i32, i32)>,
) -> Option<GestureIntent> {
    let pos_changed = before.x != after.x || before.y != after.y;
    let size_changed = before.w != after.w || before.h != after.h;
    if !pos_changed && !size_changed {
        return None;
    }
    if !size_changed {
        let (x, y) = cursor?;
        return Some(GestureIntent::MoveDrop { x, y });
    }
    let before_right = i64::from(before.x) + i64::from(before.w);
    let after_right = i64::from(after.x) + i64::from(after.w);
    let before_bottom = i64::from(before.y) + i64::from(before.h);
    let after_bottom = i64::from(after.y) + i64::from(after.h);
    let horizontal = pick_edge(
        before.x,
        after.x,
        before_right,
        after_right,
        "left",
        "right",
    )?;
    let vertical = pick_edge(before.y, after.y, before_bottom, after_bottom, "up", "down")?;
    match (horizontal, vertical) {
        (None, None) => None,
        (Some((direction, boundary)), None) => Some(GestureIntent::PointerResize {
            direction,
            boundary,
            direction2: None,
            boundary2: None,
        }),
        (None, Some((direction, boundary))) => Some(GestureIntent::PointerResize {
            direction,
            boundary,
            direction2: None,
            boundary2: None,
        }),
        (Some((direction, boundary)), Some((direction2, boundary2))) => {
            Some(GestureIntent::PointerResize {
                direction,
                boundary,
                direction2: Some(direction2),
                boundary2: Some(boundary2),
            })
        }
    }
}

/// Pick one axis edge from origin/edge deltas. `None` when neither moved;
/// the larger-magnitude delta wins when both moved, ties to the origin edge.
fn pick_edge(
    before_origin: i32,
    after_origin: i32,
    before_far: i64,
    after_far: i64,
    origin_name: &'static str,
    far_name: &'static str,
) -> Option<Option<(&'static str, i32)>> {
    let origin_moved = before_origin != after_origin;
    let far_moved = before_far != after_far;
    if !origin_moved && !far_moved {
        return Some(None);
    }
    if origin_moved
        && (!far_moved
            || (after_origin as i64 - before_origin as i64).abs() >= (after_far - before_far).abs())
    {
        return Some(Some((origin_name, after_origin)));
    }
    if far_moved {
        return Some(Some((far_name, after_far.try_into().ok()?)));
    }
    Some(Some((origin_name, after_origin)))
}
/// Complete observation inputs for one single-domain `Reconcile` tick.
pub struct ReconcileInput<'a> {
    pub owner: &'a OwnerId,
    pub generation: &'a GenerationId,
    pub correlation: &'a CorrelationId,
    pub revision: u64,
    pub fingerprint: u64,
    pub domain_bounds: Rect,
    pub windows: &'a [(WindowId, Rect, tiler_core::size_hints::WindowSizeHints)],
    pub focused: Option<&'a WindowId>,
}

/// Build a `Reconcile` event for one explicit `(output, workspace)` domain.
/// Rectangles are the current visible observations plus hidden snapshots for
/// that domain; `revision` is the last Engine-accepted revision for it.
/// Hidden Engine membership is preserved by inclusion: a retained member that
/// is invisible, minimized, maximized, or fullscreen rides its last-known
/// rectangle instead of vanishing from the observation.
/// Hints ride per window: fresh application-declared minimums for eligible
/// visible and identity-verified hidden observations, no hint on failure.
/// Hint-only changes still reach
/// projection because the caller never skips the Engine on an unchanged
/// rectangle fingerprint.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_reconcile_event_for(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fingerprint: u64,
    domain: &OutputDomain,
    domain_key: &DomainKey,
    outer_gap: i32,
    windows: &[(WindowId, Rect, tiler_core::size_hints::WindowSizeHints)],
    focused: Option<&WindowId>,
) -> CoreEvent {
    let floating: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = windows
        .iter()
        .map(|(window, rect, hints)| (window.clone(), *rect, *hints, false))
        .collect();
    build_reconcile_event_for_floating(
        owner,
        generation,
        correlation,
        revision,
        fingerprint,
        domain,
        domain_key,
        outer_gap,
        &floating,
        focused,
    )
}

/// Floating-aware per-domain `Reconcile` event: intentional floats and
/// born-held fullscreen members ride as slotless floating Engine exceptions
/// (no tile slot, siblings keep the tile area) while every other member stays
/// tiled. Floating is an Engine observation only, never a native write.
/// Hidden floating rows stay floating until reveal; hidden born rows stay
/// floating until a verified non-fullscreen exit.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_reconcile_event_for_floating(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fingerprint: u64,
    domain: &OutputDomain,
    domain_key: &DomainKey,
    outer_gap: i32,
    windows: &[(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )],
    focused: Option<&WindowId>,
) -> CoreEvent {
    CoreEvent {
        owner: owner.clone(),
        generation: generation.clone(),
        correlation: correlation.clone(),
        revision,
        fingerprint,
        domain: domain.clone(),
        domain_key: domain_key.clone(),
        outer_gap,
        focused_window: focused.cloned().unwrap_or(WindowId(String::new())),
        windows: windows
            .iter()
            .map(|(window, rect, hints, floating)| EngineWindow {
                window: window.clone(),
                output: domain_key.output.clone(),
                workspace: domain_key.workspace.clone(),
                rect: *rect,
                floating: *floating,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: *hints,
            })
            .collect(),
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: Vec::new(),
        command: CoreCommand::Reconcile,
    }
}

/// Build the single-domain `Reconcile` event for one tick. Rectangles are the
/// current visible observations; `revision` is the last Engine-accepted
/// revision for the domain.
#[must_use]
pub fn build_reconcile_event(input: &ReconcileInput<'_>) -> CoreEvent {
    let domain = OutputDomain {
        id: OutputId(OUTPUT_ID.to_owned()),
        workspace: WorkspaceId(WORKSPACE_ID.to_owned()),
        bounds: input.domain_bounds,
        gap: INNER_GAP,
        adjacent: std::collections::BTreeMap::new(),
    };
    let domain_key = DomainKey {
        output: OutputId(OUTPUT_ID.to_owned()),
        workspace: WorkspaceId(WORKSPACE_ID.to_owned()),
    };
    CoreEvent {
        owner: input.owner.clone(),
        generation: input.generation.clone(),
        correlation: input.correlation.clone(),
        revision: input.revision,
        fingerprint: input.fingerprint,
        domain: domain.clone(),
        domain_key,
        outer_gap: OUTER_GAP,
        focused_window: input.focused.cloned().unwrap_or(WindowId(String::new())),
        windows: input
            .windows
            .iter()
            .map(|(window, rect, hints)| EngineWindow {
                window: window.clone(),
                output: OutputId(OUTPUT_ID.to_owned()),
                workspace: WorkspaceId(WORKSPACE_ID.to_owned()),
                rect: *rect,
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: *hints,
            })
            .collect(),
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: Vec::new(),
        command: CoreCommand::Reconcile,
    }
}

/// CLI options for the read-only `capture` command: an explicit target list
/// only. There is no implicit all-window capture; every receipt window must
/// be named with a repeated `--hwnd`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOptions {
    pub out: PathBuf,
    pub hwnds: Vec<u64>,
}

/// Parse `capture --out PATH --hwnd HWND [--hwnd HWND ...]`. At least one
/// `--hwnd` is required; zero targets is a refusal, never an all-window
/// capture.
pub fn parse_capture_args(args: &[String]) -> Result<CaptureOptions, String> {
    let usage = "usage: capture --out PATH --hwnd HWND [--hwnd HWND ...]";
    let mut out: Option<PathBuf> = None;
    let mut hwnds: Vec<u64> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                out = Some(PathBuf::from(value));
                i += 1;
            }
            "--hwnd" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let hwnd = crate::test_window::parse_hwnd(value).ok_or_else(|| usage.to_owned())?;
                if !hwnds.contains(&hwnd) {
                    hwnds.push(hwnd);
                }
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(out) = out else {
        return Err(usage.to_owned());
    };
    if hwnds.is_empty() {
        return Err("refuse: capture requires at least one --hwnd".to_owned());
    }
    Ok(CaptureOptions { out, hwnds })
}

/// CLI options for the normal `tile` command (user dogfood only).
/// `seconds: None` runs until an exact-owner stop request. Terminal windows
/// are ordinary tile targets and require explicit `--user-start`: agents
/// never run this path. `--allowlist` is refused here; proof uses
/// `tile-proof` so lost arguments can never fall back to normal mode.
///
/// Optional repeatable `--scope-exe NAME` restricts management to the named
/// executables (top-level exe basename, case-insensitive; `Calculator` means
/// its `ApplicationFrameHost.exe` host). Empty (default) means no filter:
/// every eligible window is managed. A scoped run fences observation,
/// geometry writes, workspace hide/reveal membership, and focus actuation to
/// the named exes; anything else is reported `scope-excluded` with no writes.
///
/// Keyboard takeover defaults ON (Win+H/J/K/L and Win+arrows focus, Shift
/// variants move, per the KDE catalog): `--no-keyboard-snap-takeover` turns it
/// visibly off. Unshifted Win+L stays gated behind `--allow-win-l` (default
/// off); Win+Shift+L is approved without it.
///
/// Session-only mouse-Snap prevention (`SPI_SETWINARRANGING FALSE` while
/// product tiling is active) defaults ON as well:
/// `--no-mouse-snap-prevention` turns it visibly off. The exact preimage is
/// persisted before the first setter with readback, and restored
/// conditionally on stop (or independent restore after a crash) only while
/// the live value is still ours.
#[derive(Debug, Clone, PartialEq)]
pub struct TileOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub user_start: bool,
    pub no_keyboard_snap_takeover: bool,
    pub allow_win_l: bool,
    pub no_mouse_snap_prevention: bool,
    pub border: crate::active_border::ActiveBorderOptions,
    pub underlay: crate::group_underlay::GroupUnderlayOptions,
    /// Live inner gap (KDE 0..64, default 8). Saved settings supply the base;
    /// `--inner-gap` overrides explicitly.
    pub inner_gap: i32,
    /// Live outer gap (KDE 0..64, default 8). Saved settings supply the base;
    /// `--outer-gap` overrides explicitly.
    pub outer_gap: i32,
    pub scope_exes: Vec<String>,
    /// Explicit host-to-child scope pairs (repeatable `--scope-host-child
    /// HOST=CHILD`). Empty (default) means no child constraint. A listed host
    /// passes observation and hide admission only while a live hosted child
    /// matches, so a newly appearing hosted app can never ride another app's
    /// host executable into management.
    pub scope_hosts: Vec<ScopeHostChild>,
}

/// Basename of one exe path for scope matching: after the last `\` or `/`,
/// lowercased. Empty input maps to empty.
#[must_use]
pub fn scope_exe_basename(exe_path: &str) -> String {
    exe_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(exe_path)
        .to_ascii_lowercase()
}

/// True when `exe_path` passes the explicit scope filter. An empty scope
/// allows everything (no default filter); a nonempty scope allows only the
/// named basenames (case-insensitive, slash-insensitive).
#[must_use]
pub fn scope_allows(scope: &[String], exe_path: &str) -> bool {
    if scope.is_empty() {
        return true;
    }
    let base = scope_exe_basename(exe_path);
    scope.iter().any(|entry| scope_exe_basename(entry) == base)
}

/// One explicit host-to-child scope pair: top-level windows of `host` pass
/// only while at least one live hosted (different-process) child matches
/// `child`. Stored raw; matching normalizes basenames. Empty pair list means
/// no constraint; a top-level executable that names no pair is unconstrained.
/// A listed host with zero matching hosted children fails closed (a bare
/// host frame shows no app and manages nothing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeHostChild {
    pub host: String,
    pub child: String,
}

/// True when a top-level window passes the explicit hosted-child fence.
/// `hosted` carries the live different-process child executable paths of the
/// top-level window (unreadable children contribute nothing, never a match).
#[must_use]
pub fn hosted_child_allows(top_exe: &str, hosted: &[String], pairs: &[ScopeHostChild]) -> bool {
    if pairs.is_empty() {
        return true;
    }
    let top = scope_exe_basename(top_exe);
    let mut constrained = false;
    for pair in pairs {
        if scope_exe_basename(&pair.host) != top {
            continue;
        }
        constrained = true;
        if hosted
            .iter()
            .any(|exe| scope_exe_basename(exe) == scope_exe_basename(&pair.child))
        {
            return true;
        }
    }
    !constrained
}

/// Parse one `--scope-host-child HOST=CHILD` value into its normalized pair.
/// Exactly one `=` with non-empty sides; anything else is a refusal.
pub fn parse_scope_host_child(value: &str) -> Result<ScopeHostChild, String> {
    let (host, child) = value
        .split_once('=')
        .ok_or_else(|| "refuse: --scope-host-child needs HOST=CHILD".to_owned())?;
    let host = host.trim().to_owned();
    let child = child.trim().to_owned();
    if host.is_empty() || child.is_empty() || child.contains('=') {
        return Err("refuse: --scope-host-child needs HOST=CHILD".to_owned());
    }
    Ok(ScopeHostChild { host, child })
}

/// Parse one live gap value in `0..=64` (KDE `domain-gap.ts` parity).
pub fn parse_gap_value(value: &str) -> Result<i32, String> {
    let usage = "refuse: gap needs 0..=64";
    let parsed: i32 = value.parse().map_err(|_| usage.to_owned())?;
    if !(0..=64).contains(&parsed) {
        return Err(usage.to_owned());
    }
    Ok(parsed)
}

/// Effective defaults for the normal `tile` command: shipped KDE parity
/// (gaps 8/8, border on with theme accent, underlay on, both takeovers on).
/// Saved settings supply an alternate base via
/// [`crate::settings::tile_options_from_settings`]; explicit CLI switches
/// always override the base they start from.
///
/// Explicit switches stay authoritative for the whole run: the owner records
/// them in [`CliOverrides`] at startup and re-applies them over every live
/// settings-file change, so an unrelated file edit never silently drops a
/// CLI lane.
#[must_use]
pub fn tile_options_defaults() -> TileOptions {
    TileOptions {
        seconds: None,
        trace: false,
        user_start: false,
        no_keyboard_snap_takeover: false,
        allow_win_l: false,
        no_mouse_snap_prevention: false,
        border: crate::active_border::ActiveBorderOptions::default(),
        underlay: crate::group_underlay::GroupUnderlayOptions::default(),
        inner_gap: INNER_GAP,
        outer_gap: OUTER_GAP,
        scope_exes: Vec::new(),
        scope_hosts: Vec::new(),
    }
}

/// Parse `tile` args with compiled-in defaults (no settings file).
pub fn parse_tile_args(args: &[String]) -> Result<TileOptions, String> {
    parse_tile_args_from(args, &tile_options_defaults())
}

/// Explicit per-lane CLI overrides captured at startup. Every `Some` value
/// came from an explicit switch and stays authoritative for the whole run:
/// live settings-file changes apply underneath these lanes (see
/// [`apply_cli_overrides`]). Scope, seconds, trace, and the user-start fence
/// are CLI/run-only and never come from the file, so they carry no override.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CliOverrides {
    pub inner_gap: Option<i32>,
    pub outer_gap: Option<i32>,
    pub no_keyboard_snap_takeover: Option<bool>,
    pub allow_win_l: Option<bool>,
    pub no_mouse_snap_prevention: Option<bool>,
    pub border_enabled: Option<bool>,
    pub border_width: Option<f64>,
    pub border_gap: Option<f64>,
    pub border_radius: Option<f64>,
    pub border_color: Option<(u8, u8, u8)>,
    pub border_use_theme: Option<bool>,
    pub underlay_enabled: Option<bool>,
    pub underlay_color: Option<((u8, u8, u8), u8)>,
    pub underlay_extension: Option<f64>,
}

/// Re-apply explicit startup CLI switches over a settings-file base. Called
/// at startup (equivalent to parsing over the base) and on every live
/// settings apply, so unrelated file edits never drop a CLI lane.
pub fn apply_cli_overrides(base: &mut TileOptions, cli: &CliOverrides) {
    if let Some(value) = cli.inner_gap {
        base.inner_gap = value;
    }
    if let Some(value) = cli.outer_gap {
        base.outer_gap = value;
    }
    if let Some(value) = cli.no_keyboard_snap_takeover {
        base.no_keyboard_snap_takeover = value;
    }
    if let Some(value) = cli.allow_win_l {
        base.allow_win_l = value;
    }
    if let Some(value) = cli.no_mouse_snap_prevention {
        base.no_mouse_snap_prevention = value;
    }
    if let Some(value) = cli.border_enabled {
        base.border.enabled = value;
    }
    if let Some(value) = cli.border_width {
        base.border.style.width = value;
    }
    if let Some(value) = cli.border_gap {
        base.border.style.gap = value;
    }
    if let Some(value) = cli.border_radius {
        base.border.style.radius = value;
    }
    if let Some(value) = cli.border_color {
        base.border.style.color = value;
    }
    if let Some(value) = cli.border_use_theme {
        base.border.style.use_theme = value;
    }
    if let Some(value) = cli.underlay_enabled {
        base.underlay.enabled = value;
    }
    if let Some((color, alpha)) = cli.underlay_color {
        base.underlay.style.color = color;
        base.underlay.style.alpha = alpha;
    }
    if let Some(value) = cli.underlay_extension {
        base.underlay.style.extension = value;
    }
}

/// Parse `tile --user-start [...]` starting from a caller-supplied base
/// (normally the persisted settings, so saved values are the defaults and
/// every explicit switch overrides its corresponding value). All existing
/// switches are preserved; `--inner-gap N` and `--outer-gap N` (0..64) are
/// new. `--user-start` always starts false: saved settings can never satisfy
/// the explicit user-start fence, so agents never run this path by accident.
pub fn parse_tile_args_from(
    args: &[String],
    defaults: &TileOptions,
) -> Result<TileOptions, String> {
    parse_tile_args_from_with_overrides(args, defaults).map(|(options, _)| options)
}

/// Parse `tile` args plus the explicit per-lane [`CliOverrides`] (see
/// [`apply_cli_overrides`]): same grammar as [`parse_tile_args_from`], but
/// every explicit switch is also recorded so it stays authoritative across
/// live settings-file changes.
pub fn parse_tile_args_from_with_overrides(
    args: &[String],
    defaults: &TileOptions,
) -> Result<(TileOptions, CliOverrides), String> {
    let usage = "usage: tile --user-start [--seconds N] [--trace] [--no-keyboard-snap-takeover] [--allow-win-l] [--no-mouse-snap-prevention] [--inner-gap 0..=64] [--outer-gap 0..=64] [--scope-exe NAME ...] [--scope-host-child HOST=CHILD ...] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let mut seconds: Option<u64> = defaults.seconds;
    let mut trace = defaults.trace;
    let mut user_start = false;
    let mut no_keyboard_snap_takeover = defaults.no_keyboard_snap_takeover;
    let mut allow_win_l = defaults.allow_win_l;
    let mut no_mouse_snap_prevention = defaults.no_mouse_snap_prevention;
    let mut scope_exes: Vec<String> = defaults.scope_exes.clone();
    let mut scope_hosts: Vec<ScopeHostChild> = defaults.scope_hosts.clone();
    let mut border = defaults.border;
    let mut underlay = defaults.underlay;
    let mut inner_gap = defaults.inner_gap;
    let mut outer_gap = defaults.outer_gap;
    let mut theme_flags = 0u8;
    let mut cli = CliOverrides::default();
    let mut i = 0;
    while i < args.len() {
        if apply_underlay_arg_tracked(args, &mut i, &mut underlay, &mut cli, usage)? {
            continue;
        }
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--user-start" => {
                user_start = true;
                i += 1;
            }
            "--no-keyboard-snap-takeover" => {
                no_keyboard_snap_takeover = true;
                cli.no_keyboard_snap_takeover = Some(true);
                i += 1;
            }
            "--allow-win-l" => {
                allow_win_l = true;
                cli.allow_win_l = Some(true);
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse_snap_prevention = true;
                cli.no_mouse_snap_prevention = Some(true);
                i += 1;
            }
            "--inner-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                inner_gap = parse_gap_value(value)?;
                cli.inner_gap = Some(inner_gap);
                i += 1;
            }
            "--outer-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                outer_gap = parse_gap_value(value)?;
                cli.outer_gap = Some(outer_gap);
                i += 1;
            }
            "--no-active-border" => {
                border.enabled = false;
                cli.border_enabled = Some(false);
                i += 1;
            }
            "--active-border-width" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.width = crate::active_border::parse_width(value)?;
                cli.border_width = Some(border.style.width);
                i += 1;
            }
            "--active-border-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.gap = crate::active_border::parse_gap(value)?;
                cli.border_gap = Some(border.style.gap);
                i += 1;
            }
            "--active-border-radius" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.radius = crate::active_border::parse_radius(value)?;
                cli.border_radius = Some(border.style.radius);
                i += 1;
            }
            "--active-border-color" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.color = crate::active_border::parse_color(value)?;
                cli.border_color = Some(border.style.color);
                i += 1;
            }
            "--no-active-border-theme" => {
                if theme_flags & 0x02 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x01;
                border.style.use_theme = false;
                cli.border_use_theme = Some(false);
                i += 1;
            }
            "--active-border-theme" => {
                if theme_flags & 0x01 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x02;
                border.style.use_theme = true;
                cli.border_use_theme = Some(true);
                i += 1;
            }
            "--scope-exe" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty --scope-exe".to_owned());
                }
                let normalized = value.trim().to_owned();
                if !scope_exes
                    .iter()
                    .any(|entry| scope_exe_basename(entry) == scope_exe_basename(&normalized))
                {
                    scope_exes.push(normalized);
                }
                i += 1;
            }
            "--scope-host-child" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let pair = parse_scope_host_child(value.trim())?;
                if !scope_hosts.iter().any(|entry| {
                    scope_exe_basename(&entry.host) == scope_exe_basename(&pair.host)
                        && scope_exe_basename(&entry.child) == scope_exe_basename(&pair.child)
                }) {
                    scope_hosts.push(pair);
                }
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    if !user_start {
        return Err("refuse: tile requires explicit --user-start".to_owned());
    }
    Ok((
        TileOptions {
            seconds,
            trace,
            user_start,
            no_keyboard_snap_takeover,
            allow_win_l,
            no_mouse_snap_prevention,
            border,
            underlay,
            inner_gap,
            outer_gap,
            scope_exes,
            scope_hosts,
        },
        cli,
    ))
}

/// CLI options for the proof-only `tile-proof` command (owned helpers only).
/// `--allowlist PATH` is required and nonempty; missing/empty/malformed input
/// refuses before any lease or write and never falls back to normal mode.
#[derive(Debug, Clone, PartialEq)]
pub struct TileProofOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub allowlist: PathBuf,
    pub border: crate::active_border::ActiveBorderOptions,
    pub underlay: crate::group_underlay::GroupUnderlayOptions,
}

/// Parse `tile-proof --allowlist PATH [--seconds N] [--trace]` plus the shared
/// active-border flags (default on with `--no-active-border`) and the shared
/// group-underlay flags (default on with `--no-group-underlay`).
pub fn parse_tile_proof_args(args: &[String]) -> Result<TileProofOptions, String> {
    let usage = "usage: tile-proof --allowlist PATH [--seconds N] [--trace] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let mut seconds: Option<u64> = None;
    let mut trace = false;
    let mut allowlist: Option<PathBuf> = None;
    let mut border = crate::active_border::ActiveBorderOptions::default();
    let mut underlay = crate::group_underlay::GroupUnderlayOptions::default();
    let mut theme_flags = 0u8;
    let mut i = 0;
    while i < args.len() {
        if apply_underlay_arg(args, &mut i, &mut underlay, usage)? {
            continue;
        }
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-active-border" => {
                border.enabled = false;
                i += 1;
            }
            "--active-border-width" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.width = crate::active_border::parse_width(value)?;
                i += 1;
            }
            "--active-border-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.gap = crate::active_border::parse_gap(value)?;
                i += 1;
            }
            "--active-border-radius" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.radius = crate::active_border::parse_radius(value)?;
                i += 1;
            }
            "--active-border-color" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.color = crate::active_border::parse_color(value)?;
                i += 1;
            }
            "--no-active-border-theme" => {
                if theme_flags & 0x02 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x01;
                border.style.use_theme = false;
                i += 1;
            }
            "--active-border-theme" => {
                if theme_flags & 0x01 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x02;
                border.style.use_theme = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty allowlist".to_owned());
                }
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: tile-proof requires --allowlist".to_owned());
    };
    Ok(TileProofOptions {
        seconds,
        trace,
        allowlist,
        border,
        underlay,
    })
}

/// CLI options for the proof-only `hide-proof` command (owned helpers only,
/// product nonce mechanism). Requires a nonempty valid `--allowlist`; the
/// owner verifies every frozen entry as an owned helper BEFORE touching the
/// ordinary product nonce APIs, and never falls back to normal mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HideProofOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub allowlist: PathBuf,
}

/// Parse `hide-proof --allowlist PATH [--seconds N] [--trace]`.
pub fn parse_hide_proof_args(args: &[String]) -> Result<HideProofOptions, String> {
    let usage = "usage: hide-proof --allowlist PATH [--seconds N] [--trace]";
    let mut seconds: Option<u64> = None;
    let mut trace = false;
    let mut allowlist: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty allowlist".to_owned());
                }
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: hide-proof requires --allowlist".to_owned());
    };
    Ok(HideProofOptions {
        seconds,
        trace,
        allowlist,
    })
}

/// Verify the raw received argv against the parsed `hide-proof` options.
/// Every delivery flag must be evidenced; any inconsistency refuses before
/// any lease or write, never falls back.
pub fn verify_hide_proof_argv_consistency(
    raw: &[String],
    parsed: &HideProofOptions,
) -> Result<(), String> {
    let usage = "usage: hide-proof --allowlist PATH [--seconds N] [--trace]";
    let mut allowlist: Option<&str> = None;
    let mut seconds: Option<&str> = None;
    let mut trace = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                seconds = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                allowlist = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: hide-proof requires --allowlist".to_owned());
    };
    if parsed.allowlist.as_path() != std::path::Path::new(allowlist) {
        return Err("error: argv/parsed allowlist mismatch (impossible)".to_owned());
    }
    if parsed.trace != trace {
        return Err("error: argv/parsed trace mismatch (impossible)".to_owned());
    }
    match (parsed.seconds, seconds) {
        (None, None) => {}
        (Some(want), Some(got)) => {
            let got: u64 = got.parse().map_err(|_| usage.to_owned())?;
            if want != got {
                return Err("error: argv/parsed seconds mismatch (impossible)".to_owned());
            }
        }
        _ => return Err("error: argv/parsed seconds mismatch (impossible)".to_owned()),
    }
    Ok(())
}

/// CLI options for the proof-only `shortcut-proof` command (owned helpers
/// only, automated synthetic-input verification). Requires a nonempty valid
/// `--allowlist` at parse and at native start; missing/empty/malformed input
/// refuses before any lease or write and never falls back to normal mode.
///
/// Differences from `tile-proof`: the owner installs the same low-level hook
/// with test-only acceptance of exactly
/// [`crate::snapkey::SHORTCUT_PROOF_MARKER`] in `dwExtraInfo` (product `tile`
/// keeps filtering ALL injected), and keyboard takeover stays ON so marked
/// Win+H/J/K/L/arrows (Shift variants move) dispatch through the retained
/// Engine with fresh observation plus native readback. Unshifted Win+L is
/// NOT offered here (always gated off): live runs must never send Win+L, and
/// the gating is asserted by portable tests. Mouse-Snap prevention defaults
/// ON like product (visible `--no-mouse-snap-prevention` off switch) so the
/// same session-only preimage/restore routines run under the proof geometry
/// gate.
#[derive(Debug, Clone, PartialEq)]
pub struct ShortcutProofOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub allowlist: PathBuf,
    pub no_mouse_snap_prevention: bool,
    pub border: crate::active_border::ActiveBorderOptions,
    pub underlay: crate::group_underlay::GroupUnderlayOptions,
}

/// Parse `shortcut-proof --allowlist PATH [--seconds N] [--trace]
/// [--no-mouse-snap-prevention]` plus the shared active-border flags and the
/// shared group-underlay flags.
/// Any keyboard flag (`--allow-win-l`,
/// `--no-keyboard-snap-takeover`), `--user-start`, or unknown flag is a
/// refusal, never a silent normal run.
pub fn parse_shortcut_proof_args(args: &[String]) -> Result<ShortcutProofOptions, String> {
    let usage = "usage: shortcut-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let mut seconds: Option<u64> = None;
    let mut trace = false;
    let mut allowlist: Option<PathBuf> = None;
    let mut no_mouse_snap_prevention = false;
    let mut border = crate::active_border::ActiveBorderOptions::default();
    let mut underlay = crate::group_underlay::GroupUnderlayOptions::default();
    let mut theme_flags = 0u8;
    let mut i = 0;
    while i < args.len() {
        if apply_underlay_arg(args, &mut i, &mut underlay, usage)? {
            continue;
        }
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse_snap_prevention = true;
                i += 1;
            }
            "--no-active-border" => {
                border.enabled = false;
                i += 1;
            }
            "--active-border-width" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.width = crate::active_border::parse_width(value)?;
                i += 1;
            }
            "--active-border-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.gap = crate::active_border::parse_gap(value)?;
                i += 1;
            }
            "--active-border-radius" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.radius = crate::active_border::parse_radius(value)?;
                i += 1;
            }
            "--active-border-color" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.color = crate::active_border::parse_color(value)?;
                i += 1;
            }
            "--no-active-border-theme" => {
                if theme_flags & 0x02 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x01;
                border.style.use_theme = false;
                i += 1;
            }
            "--active-border-theme" => {
                if theme_flags & 0x01 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x02;
                border.style.use_theme = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty allowlist".to_owned());
                }
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: shortcut-proof requires --allowlist".to_owned());
    };
    Ok(ShortcutProofOptions {
        seconds,
        trace,
        allowlist,
        no_mouse_snap_prevention,
        border,
        underlay,
    })
}

/// Verify the raw received argv against the parsed `shortcut-proof` options.
/// Same shape as the `tile-proof` check plus the mouse-prevention flag: every
/// delivery flag must be evidenced with argv elements as separate strings.
pub fn verify_shortcut_proof_argv_consistency(
    raw: &[String],
    parsed: &ShortcutProofOptions,
) -> Result<(), String> {
    let usage = "usage: shortcut-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let (border_echo, rest) = scan_border_echo(raw, usage)?;
    verify_border_echo(&border_echo, &parsed.border, usage)?;
    let (underlay_echo, rest) = scan_underlay_echo(&rest, usage)?;
    verify_underlay_echo(&underlay_echo, &parsed.underlay, usage)?;
    let raw = rest;
    let mut allowlist: Option<&str> = None;
    let mut seconds: Option<&str> = None;
    let mut trace = false;
    let mut no_mouse = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                seconds = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                allowlist = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: shortcut-proof requires --allowlist".to_owned());
    };
    if parsed.allowlist.as_path() != std::path::Path::new(allowlist) {
        return Err("error: argv/parsed allowlist mismatch (impossible)".to_owned());
    }
    if parsed.trace != trace {
        return Err("error: argv/parsed trace mismatch (impossible)".to_owned());
    }
    if parsed.no_mouse_snap_prevention != no_mouse {
        return Err("error: argv/parsed mouse mismatch (impossible)".to_owned());
    }
    match (parsed.seconds, seconds) {
        (None, None) => {}
        (Some(want), Some(got)) => {
            let got: u64 = got.parse().map_err(|_| usage.to_owned())?;
            if want != got {
                return Err("error: argv/parsed seconds mismatch (impossible)".to_owned());
            }
        }
        _ => return Err("error: argv/parsed seconds mismatch (impossible)".to_owned()),
    }
    Ok(())
}
/// Verify the raw received argv against the parsed `tile-proof` options.
/// Every delivery flag must be evidenced: `--allowlist` present with the exact
/// path (argv elements stay separate strings so spaces survive without
/// quoting), `--trace` presence matching, `--seconds` value matching, and no
/// unknown flags. Any parsed/raw inconsistency is an impossible error: refuse
/// before any lease or write, never fall back.
/// Verify a raw-scanned border echo against parsed border options.
fn verify_border_echo(
    echo: &crate::active_border::BorderArgvEcho,
    border: &crate::active_border::ActiveBorderOptions,
    usage: &str,
) -> Result<(), String> {
    let impossible = |what: &str| format!("error: argv/parsed {what} mismatch (impossible)");
    if echo.no_active_border != !border.enabled {
        return Err(impossible("border-enabled"));
    }
    let check_value = |raw: &Option<String>, want: f64, what: &str| -> Result<(), String> {
        match raw {
            None => Ok(()),
            Some(text) => {
                let got: f64 = text.parse().map_err(|_| usage.to_owned())?;
                if (got - want).abs() > f64::EPSILON {
                    return Err(impossible(what));
                }
                Ok(())
            }
        }
    };
    // Presence must round-trip too: a parsed non-default with no raw flag,
    // or a raw flag collapsing to the default, is still consistent as long
    // as values match; only value mismatches refuse here.
    let defaults = crate::active_border::ActiveBorderOptions::default();
    check_value(&echo.width, border.style.width, "border-width")?;
    check_value(&echo.gap, border.style.gap, "border-gap")?;
    check_value(&echo.radius, border.style.radius, "border-radius")?;
    match (&echo.color, border.style.color) {
        (None, _) => {}
        (Some(text), want) => {
            let got = crate::active_border::parse_color(text).map_err(|_| usage.to_owned())?;
            if got != want {
                return Err(impossible("border-color"));
            }
        }
    }
    if echo.no_theme && echo.theme {
        return Err(impossible("border-theme"));
    }
    if echo.theme && !border.style.use_theme {
        return Err(impossible("border-theme"));
    }
    if echo.no_theme && border.style.use_theme {
        return Err(impossible("border-theme"));
    }
    if !echo.theme && !echo.no_theme && border.style.use_theme != defaults.style.use_theme {
        return Err(impossible("border-theme"));
    }
    // A raw flag that parses to the default is harmless; a missing flag with
    // a non-default parsed value is impossible.
    if echo.width.is_none() && border.style.width != defaults.style.width {
        return Err(impossible("border-width"));
    }
    if echo.gap.is_none() && border.style.gap != defaults.style.gap {
        return Err(impossible("border-gap"));
    }
    if echo.radius.is_none() && border.style.radius != defaults.style.radius {
        return Err(impossible("border-radius"));
    }
    if echo.color.is_none() && border.style.color != defaults.style.color {
        return Err(impossible("border-color"));
    }
    Ok(())
}

/// Apply one shared group-underlay flag (`--no-group-underlay`,
/// `--group-underlay-color #aarrggbb`, `--group-underlay-extension -1..=32`).
/// Returns true when `args[*i]` was an underlay flag (index advanced past its
/// value); false leaves every other flag for the caller. Called at the top of
/// each loop parser so all four commands share one flag shape.
fn apply_underlay_arg(
    args: &[String],
    i: &mut usize,
    underlay: &mut crate::group_underlay::GroupUnderlayOptions,
    usage: &str,
) -> Result<bool, String> {
    match args[*i].as_str() {
        "--no-group-underlay" => {
            underlay.enabled = false;
            *i += 1;
            Ok(true)
        }
        "--group-underlay-color" => {
            *i += 1;
            let value = args.get(*i).ok_or_else(|| usage.to_owned())?;
            let (color, alpha) = crate::group_underlay::parse_color_argb(value)?;
            underlay.style.color = color;
            underlay.style.alpha = alpha;
            *i += 1;
            Ok(true)
        }
        "--group-underlay-extension" => {
            *i += 1;
            let value = args.get(*i).ok_or_else(|| usage.to_owned())?;
            underlay.style.extension = crate::group_underlay::parse_extension(value)?;
            *i += 1;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Tracked underlay-arg scan for the normal `tile` parser: same grammar as
/// [`apply_underlay_arg`] (used by the proof parsers), but every explicit
/// switch is also recorded in [`CliOverrides`] so it stays authoritative
/// across live settings-file changes.
fn apply_underlay_arg_tracked(
    args: &[String],
    i: &mut usize,
    underlay: &mut crate::group_underlay::GroupUnderlayOptions,
    cli: &mut CliOverrides,
    usage: &str,
) -> Result<bool, String> {
    match args[*i].as_str() {
        "--no-group-underlay" => {
            underlay.enabled = false;
            cli.underlay_enabled = Some(false);
            *i += 1;
            Ok(true)
        }
        "--group-underlay-color" => {
            *i += 1;
            let value = args.get(*i).ok_or_else(|| usage.to_owned())?;
            let (color, alpha) = crate::group_underlay::parse_color_argb(value)?;
            underlay.style.color = color;
            underlay.style.alpha = alpha;
            cli.underlay_color = Some((color, alpha));
            *i += 1;
            Ok(true)
        }
        "--group-underlay-extension" => {
            *i += 1;
            let value = args.get(*i).ok_or_else(|| usage.to_owned())?;
            underlay.style.extension = crate::group_underlay::parse_extension(value)?;
            cli.underlay_extension = Some(underlay.style.extension);
            *i += 1;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Scan raw argv border flags into an echo. Unknown non-border flags are left
/// for the caller: this returns the echo plus the non-border remainder.
fn scan_border_echo(
    raw: &[String],
    usage: &str,
) -> Result<(crate::active_border::BorderArgvEcho, Vec<String>), String> {
    let mut echo = crate::active_border::BorderArgvEcho::default();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--no-active-border" => {
                echo.no_active_border = true;
                i += 1;
            }
            "--active-border-width"
            | "--active-border-gap"
            | "--active-border-radius"
            | "--active-border-color" => {
                let flag = raw[i].clone();
                i += 1;
                let value = raw.get(i).ok_or_else(|| usage.to_owned())?.clone();
                match flag.as_str() {
                    "--active-border-width" => echo.width = Some(value),
                    "--active-border-gap" => echo.gap = Some(value),
                    "--active-border-radius" => echo.radius = Some(value),
                    _ => echo.color = Some(value),
                }
                i += 1;
            }
            "--no-active-border-theme" => {
                echo.no_theme = true;
                i += 1;
            }
            "--active-border-theme" => {
                echo.theme = true;
                i += 1;
            }
            _ => {
                rest.push(raw[i].clone());
                i += 1;
            }
        }
    }
    Ok((echo, rest))
}

/// Scan raw argv underlay flags into an echo, mirroring `scan_border_echo`.
fn scan_underlay_echo(
    raw: &[String],
    usage: &str,
) -> Result<(crate::group_underlay::UnderlayArgvEcho, Vec<String>), String> {
    let mut echo = crate::group_underlay::UnderlayArgvEcho::default();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--no-group-underlay" => {
                echo.no_group_underlay = true;
                i += 1;
            }
            "--group-underlay-color" | "--group-underlay-extension" => {
                let flag = raw[i].clone();
                i += 1;
                let value = raw.get(i).ok_or_else(|| usage.to_owned())?.clone();
                if flag == "--group-underlay-color" {
                    echo.color = Some(value);
                } else {
                    echo.extension = Some(value);
                }
                i += 1;
            }
            _ => {
                rest.push(raw[i].clone());
                i += 1;
            }
        }
    }
    Ok((echo, rest))
}

/// Verify a raw-scanned underlay echo against parsed underlay options.
fn verify_underlay_echo(
    echo: &crate::group_underlay::UnderlayArgvEcho,
    underlay: &crate::group_underlay::GroupUnderlayOptions,
    usage: &str,
) -> Result<(), String> {
    let impossible = |what: &str| format!("error: argv/parsed {what} mismatch (impossible)");
    if echo.no_group_underlay != !underlay.enabled {
        return Err(impossible("underlay-enabled"));
    }
    match (&echo.color, underlay.style.color, underlay.style.alpha) {
        (None, _, _) => {}
        (Some(text), want_rgb, want_alpha) => {
            let (got_rgb, got_alpha) =
                crate::group_underlay::parse_color_argb(text).map_err(|_| usage.to_owned())?;
            if got_rgb != want_rgb || got_alpha != want_alpha {
                return Err(impossible("underlay-color"));
            }
        }
    }
    match &echo.extension {
        None => {}
        Some(text) => {
            let got: f64 = text.parse().map_err(|_| usage.to_owned())?;
            if (got - underlay.style.extension).abs() > f64::EPSILON {
                return Err(impossible("underlay-extension"));
            }
        }
    }
    let defaults = crate::group_underlay::GroupUnderlayOptions::default();
    if echo.color.is_none()
        && (underlay.style.color != defaults.style.color
            || underlay.style.alpha != defaults.style.alpha)
    {
        return Err(impossible("underlay-color"));
    }
    if echo.extension.is_none() && underlay.style.extension != defaults.style.extension {
        return Err(impossible("underlay-extension"));
    }
    Ok(())
}

pub fn verify_proof_argv_consistency(
    raw: &[String],
    parsed: &TileProofOptions,
) -> Result<(), String> {
    let usage = "usage: tile-proof --allowlist PATH [--seconds N] [--trace] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let (border_echo, rest) = scan_border_echo(raw, usage)?;
    verify_border_echo(&border_echo, &parsed.border, usage)?;
    let (underlay_echo, rest) = scan_underlay_echo(&rest, usage)?;
    verify_underlay_echo(&underlay_echo, &parsed.underlay, usage)?;
    let raw = rest;
    let mut allowlist: Option<&str> = None;
    let mut seconds: Option<&str> = None;
    let mut trace = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                seconds = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                allowlist = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: tile-proof requires --allowlist".to_owned());
    };
    if parsed.allowlist.as_path() != std::path::Path::new(allowlist) {
        return Err("error: argv/parsed allowlist mismatch (impossible)".to_owned());
    }
    if parsed.trace != trace {
        return Err("error: argv/parsed trace mismatch (impossible)".to_owned());
    }
    match (parsed.seconds, seconds) {
        (None, None) => {}
        (Some(want), Some(got)) => {
            let got: u64 = got.parse().map_err(|_| usage.to_owned())?;
            if want != got {
                return Err("error: argv/parsed seconds mismatch (impossible)".to_owned());
            }
        }
        _ => return Err("error: argv/parsed seconds mismatch (impossible)".to_owned()),
    }
    Ok(())
}

/// CLI options for the proof-only `workspace-proof` command (owned helpers
/// only, automated synthetic-input verification for workspace digits).
/// Same frozen-allowlist geometry gate as `shortcut-proof` (never falls back
/// to normal), but the owner additionally drives the workspace dispatcher
/// (select/send/follow with hide/reveal) for exactly the allowlisted helpers:
/// every hide verifies `verify_proof_owned` fresh before the write, and the
/// hook accepts exactly [`crate::snapkey::SHORTCUT_PROOF_MARKER`].
/// `shortcut-proof` never hides for workspace intents; `tile-proof` installs
/// no hook at all.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceProofOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub allowlist: PathBuf,
    pub no_mouse_snap_prevention: bool,
    pub border: crate::active_border::ActiveBorderOptions,
    pub underlay: crate::group_underlay::GroupUnderlayOptions,
}

/// Parse `workspace-proof --allowlist PATH [--seconds N] [--trace]
/// [--no-mouse-snap-prevention]` plus the shared active-border flags and the
/// shared group-underlay flags.
/// Any keyboard flag, `--user-start`, or
/// unknown flag is a refusal, never a silent normal run.
pub fn parse_workspace_proof_args(args: &[String]) -> Result<WorkspaceProofOptions, String> {
    let usage = "usage: workspace-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let mut seconds: Option<u64> = None;
    let mut trace = false;
    let mut allowlist: Option<PathBuf> = None;
    let mut no_mouse_snap_prevention = false;
    let mut border = crate::active_border::ActiveBorderOptions::default();
    let mut underlay = crate::group_underlay::GroupUnderlayOptions::default();
    let mut theme_flags = 0u8;
    let mut i = 0;
    while i < args.len() {
        if apply_underlay_arg(args, &mut i, &mut underlay, usage)? {
            continue;
        }
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse_snap_prevention = true;
                i += 1;
            }
            "--no-active-border" => {
                border.enabled = false;
                i += 1;
            }
            "--active-border-width" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.width = crate::active_border::parse_width(value)?;
                i += 1;
            }
            "--active-border-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.gap = crate::active_border::parse_gap(value)?;
                i += 1;
            }
            "--active-border-radius" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.radius = crate::active_border::parse_radius(value)?;
                i += 1;
            }
            "--active-border-color" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.color = crate::active_border::parse_color(value)?;
                i += 1;
            }
            "--no-active-border-theme" => {
                if theme_flags & 0x02 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x01;
                border.style.use_theme = false;
                i += 1;
            }
            "--active-border-theme" => {
                if theme_flags & 0x01 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x02;
                border.style.use_theme = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty allowlist".to_owned());
                }
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: workspace-proof requires --allowlist".to_owned());
    };
    Ok(WorkspaceProofOptions {
        seconds,
        trace,
        allowlist,
        no_mouse_snap_prevention,
        border,
        underlay,
    })
}

/// Verify the raw received argv against the parsed `workspace-proof` options.
pub fn verify_workspace_proof_argv_consistency(
    raw: &[String],
    parsed: &WorkspaceProofOptions,
) -> Result<(), String> {
    let usage = "usage: workspace-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let (border_echo, rest) = scan_border_echo(raw, usage)?;
    verify_border_echo(&border_echo, &parsed.border, usage)?;
    let (underlay_echo, rest) = scan_underlay_echo(&rest, usage)?;
    verify_underlay_echo(&underlay_echo, &parsed.underlay, usage)?;
    let raw = rest;
    let mut allowlist: Option<&str> = None;
    let mut seconds: Option<&str> = None;
    let mut trace = false;
    let mut no_mouse = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                seconds = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                allowlist = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: workspace-proof requires --allowlist".to_owned());
    };
    if parsed.allowlist.as_path() != std::path::Path::new(allowlist) {
        return Err("error: argv/parsed allowlist mismatch (impossible)".to_owned());
    }
    if parsed.trace != trace {
        return Err("error: argv/parsed trace mismatch (impossible)".to_owned());
    }
    if parsed.no_mouse_snap_prevention != no_mouse {
        return Err("error: argv/parsed mouse mismatch (impossible)".to_owned());
    }
    match (parsed.seconds, seconds) {
        (None, None) => {}
        (Some(want), Some(got)) => {
            let got: u64 = got.parse().map_err(|_| usage.to_owned())?;
            if want != got {
                return Err("error: argv/parsed seconds mismatch (impossible)".to_owned());
            }
        }
        _ => return Err("error: argv/parsed seconds mismatch (impossible)".to_owned()),
    }
    Ok(())
}

/// CLI options for the read-only `children` command: explicit top-level
/// targets whose child windows are reported with verified process identity.
/// Never reads titles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildrenOptions {
    pub hwnds: Vec<u64>,
}

/// Parse `children --hwnd HWND [--hwnd HWND ...]`. At least one `--hwnd`
/// is required; zero targets is a refusal, never an all-window enumeration.
pub fn parse_children_args(args: &[String]) -> Result<ChildrenOptions, String> {
    let usage = "usage: children --hwnd HWND [--hwnd HWND ...]";
    let mut hwnds: Vec<u64> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--hwnd" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let hwnd = crate::test_window::parse_hwnd(value).ok_or_else(|| usage.to_owned())?;
                if !hwnds.contains(&hwnd) {
                    hwnds.push(hwnd);
                }
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    if hwnds.is_empty() {
        return Err("refuse: children requires at least one --hwnd".to_owned());
    }
    Ok(ChildrenOptions { hwnds })
}

/// CLI options for the read-only `inspect` command: one frozen allowlist.
/// Reports fresh identity, rectangles, and eligibility per entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectOptions {
    pub allowlist: PathBuf,
}

/// Parse `inspect --allowlist PATH`.
pub fn parse_inspect_args(args: &[String]) -> Result<InspectOptions, String> {
    let usage = "usage: inspect --allowlist PATH";
    let mut allowlist: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err(usage.to_owned());
    };
    Ok(InspectOptions { allowlist })
}

/// Exact-owner `workspace` control action (normal `tile` only): `select`
/// focuses an existing/trailing same-output workspace, `send` moves the
/// focused managed window there and follows, `stay` moves it without
/// following, `previous` toggles the previous view, `fullscreen` toggles
/// fullscreen on the focused managed window with no workspace move, `float`
/// toggles float and `sticky` toggles sticky on the focused managed window
/// (both test-needed routes, tentative pending user review), and the three
/// `relative` forms step the item 1 scoped ring (`relative` selects,
/// `send-relative` follows, `stay-relative` stays). No synthetic input, no
/// keyboard acceptance, never a proof path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WorkspaceAction {
    #[serde(rename = "select")]
    Select,
    #[serde(rename = "send")]
    Send,
    #[serde(rename = "stay")]
    Stay,
    #[serde(rename = "previous")]
    Previous,
    #[serde(rename = "fullscreen")]
    Fullscreen,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "sticky")]
    Sticky,
    #[serde(rename = "relative")]
    RelativeHistory,
    #[serde(rename = "send-relative")]
    RelativeSend,
    #[serde(rename = "stay-relative")]
    RelativeStay,
}

impl WorkspaceAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Send => "send",
            Self::Stay => "stay",
            Self::Previous => "previous",
            Self::Fullscreen => "fullscreen",
            Self::Float => "float",
            Self::Sticky => "sticky",
            Self::RelativeHistory => "relative",
            Self::RelativeSend => "send-relative",
            Self::RelativeStay => "stay-relative",
        }
    }

    /// True for the three send-family actions (numbered/relative follow/stay):
    /// they move the focused managed window and require a managed origin.
    /// Select/history/fullscreen/float/sticky actions never move workspaces;
    /// `fullscreen`/`float`/`sticky` still require a managed origin (they
    /// toggle the focused managed window), while select/history work on empty
    /// workspaces and unmanaged foreground.
    #[must_use]
    pub const fn is_send(self) -> bool {
        match self {
            Self::Send | Self::RelativeSend | Self::RelativeStay | Self::Stay => true,
            Self::Select
            | Self::Previous
            | Self::Fullscreen
            | Self::Float
            | Self::Sticky
            | Self::RelativeHistory => false,
        }
    }

    /// Explicit follow intent for the send family (`send`/`send-relative`
    /// follow; `stay`/`stay-relative` stay). `None` for select/history/
    /// fullscreen/float/sticky.
    #[must_use]
    pub const fn follow(self) -> Option<bool> {
        match self {
            Self::Send | Self::RelativeSend => Some(true),
            Self::Stay | Self::RelativeStay => Some(false),
            Self::Select
            | Self::Previous
            | Self::Fullscreen
            | Self::Float
            | Self::Sticky
            | Self::RelativeHistory => None,
        }
    }
}

/// Relative direction for the `workspace --relative` / `--send-relative` /
/// `--stay-relative` forms: an ordinal step in the item 1 scoped ring, never
/// MRU, resolved once before transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WorkspaceDirection {
    #[serde(rename = "previous")]
    Previous,
    #[serde(rename = "next")]
    Next,
}

impl WorkspaceDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Previous => "previous",
            Self::Next => "next",
        }
    }

    /// Ring step: -1 previous, +1 next.
    #[must_use]
    pub const fn delta(self) -> i32 {
        match self {
            Self::Previous => -1,
            Self::Next => 1,
        }
    }
}

/// CLI options for the exact-owner `workspace` control (normal `tile` only):
/// `--select`/`--send`/`--stay` take a digit index 0..=9; `--previous`,
/// `--fullscreen`, `--float`, and `--sticky` take no value;
/// `--relative`/`--send-relative`/`--stay-relative` take `previous`|`next`.
/// The CLI only queues a bounded single-pending request file; the owner loop
/// validates the full owner binding and dispatches through the existing
/// `workspace_do_select` / `workspace_do_send` resolvers (plus the pure
/// history resolvers) or, for `--fullscreen`, through the existing
/// project-owned fullscreen toggle (`fullscreen_toggle_decision` +
/// `enter_fullscreen`/`exit_fullscreen_owned` with the `RefuseAppOwned`
/// R-MAX-05 refusal), or, for `--float`/`--sticky`, through the existing
/// hook float/sticky dispatch arms (`dispatch_float_intent` /
/// `dispatch_sticky_intent` with every production fence). No synthetic
/// input, no keyboard acceptance, never a proof path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOptions {
    pub action: WorkspaceAction,
    pub index: u8,
    pub direction: Option<WorkspaceDirection>,
}

/// Parse `workspace` CLI forms. Exactly one action flag; indexed forms take a
/// digit 0..=9, relative forms take `previous`|`next`, `--previous`,
/// `--fullscreen`, `--float`, and `--sticky` take no value. Anything else
/// (including a missing value or both flags) refuses.
pub fn parse_workspace_args(args: &[String]) -> Result<WorkspaceOptions, String> {
    let usage = "usage: workspace (--select|--send|--stay) INDEX | --previous | --fullscreen | --float | --sticky | (--relative|--send-relative|--stay-relative) (previous|next)";
    let mut action: Option<WorkspaceAction> = None;
    let mut index: u8 = 0;
    let mut direction: Option<WorkspaceDirection> = None;
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let next = match flag {
            "--select" => WorkspaceAction::Select,
            "--send" => WorkspaceAction::Send,
            "--stay" => WorkspaceAction::Stay,
            "--previous" => WorkspaceAction::Previous,
            "--fullscreen" => WorkspaceAction::Fullscreen,
            "--float" => WorkspaceAction::Float,
            "--sticky" => WorkspaceAction::Sticky,
            "--relative" => WorkspaceAction::RelativeHistory,
            "--send-relative" => WorkspaceAction::RelativeSend,
            "--stay-relative" => WorkspaceAction::RelativeStay,
            _ => return Err(usage.to_owned()),
        };
        if action.is_some() {
            return Err(usage.to_owned());
        }
        i += 1;
        match next {
            WorkspaceAction::Select | WorkspaceAction::Send | WorkspaceAction::Stay => {
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u8 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed > 9 {
                    return Err("refuse: workspace index must be 0..=9".to_owned());
                }
                index = parsed;
                i += 1;
            }
            WorkspaceAction::Previous
            | WorkspaceAction::Fullscreen
            | WorkspaceAction::Float
            | WorkspaceAction::Sticky => {}
            WorkspaceAction::RelativeHistory
            | WorkspaceAction::RelativeSend
            | WorkspaceAction::RelativeStay => {
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                direction = Some(match value.as_str() {
                    "previous" => WorkspaceDirection::Previous,
                    "next" => WorkspaceDirection::Next,
                    _ => return Err(usage.to_owned()),
                });
                i += 1;
            }
        }
        action = Some(next);
    }
    match action {
        Some(action) => Ok(WorkspaceOptions {
            action,
            index,
            direction,
        }),
        None => Err(usage.to_owned()),
    }
}

/// Verify the raw received argv against the parsed `workspace` options:
/// exactly one action flag plus its matching value (none for `--previous`,
/// `--fullscreen`, `--float`, and `--sticky`), no unknown flags.
pub fn verify_workspace_argv_consistency(
    raw: &[String],
    parsed: &WorkspaceOptions,
) -> Result<(), String> {
    let usage = "usage: workspace (--select|--send|--stay) INDEX | --previous | --fullscreen | --float | --sticky | (--relative|--send-relative|--stay-relative) (previous|next)";
    let want = match parsed.action {
        WorkspaceAction::Select => "--select",
        WorkspaceAction::Send => "--send",
        WorkspaceAction::Stay => "--stay",
        WorkspaceAction::Previous => "--previous",
        WorkspaceAction::Fullscreen => "--fullscreen",
        WorkspaceAction::Float => "--float",
        WorkspaceAction::Sticky => "--sticky",
        WorkspaceAction::RelativeHistory => "--relative",
        WorkspaceAction::RelativeSend => "--send-relative",
        WorkspaceAction::RelativeStay => "--stay-relative",
    };
    match parsed.action {
        WorkspaceAction::Previous
        | WorkspaceAction::Fullscreen
        | WorkspaceAction::Float
        | WorkspaceAction::Sticky => {
            if raw.len() != 1 {
                return Err(usage.to_owned());
            }
        }
        WorkspaceAction::Select | WorkspaceAction::Send | WorkspaceAction::Stay => {
            if raw.len() != 2 {
                return Err(usage.to_owned());
            }
            let got: u8 = raw[1].parse().map_err(|_| usage.to_owned())?;
            if got != parsed.index {
                return Err("error: argv/parsed workspace mismatch (impossible)".to_owned());
            }
        }
        WorkspaceAction::RelativeHistory
        | WorkspaceAction::RelativeSend
        | WorkspaceAction::RelativeStay => {
            if raw.len() != 2 {
                return Err(usage.to_owned());
            }
            let want_direction = parsed.direction.ok_or_else(|| usage.to_owned())?;
            if raw[1] != want_direction.as_str() {
                return Err("error: argv/parsed workspace mismatch (impossible)".to_owned());
            }
        }
    }
    if raw[0] != want {
        return Err(usage.to_owned());
    }
    Ok(())
}

/// Versioned exact-owner workspace request body. `creation`/`pid`/`exe_path`/
/// `user_sid`/`session_id` bind the exact ledger owner; `action` is the
/// transport op; `index` is the digit for indexed actions (ignored
/// otherwise, still bounded); `direction` rides the relative step for the
/// three relative actions (`None` elsewhere); `correlation` is a
/// client-generated opaque token the owner echoes in its dispatch log. No
/// titles, no geometry, no secrets.
pub const WORKSPACE_REQUEST_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceRequest {
    pub v: u32,
    pub creation: String,
    pub pid: u32,
    pub exe_path: String,
    pub user_sid: String,
    pub session_id: u32,
    pub action: WorkspaceAction,
    pub index: u8,
    #[serde(default)]
    pub direction: Option<WorkspaceDirection>,
    pub correlation: String,
}

/// Render one workspace request body. Bounds only; full owner equality stays
/// with the owner-side check.
pub fn render_workspace_request(request: &WorkspaceRequest) -> String {
    serde_json::to_string(request).unwrap_or_default()
}

/// Parse and bound one workspace request body. Refuses malformed JSON,
/// version drift, empty identity, out-of-range index, missing/misplaced
/// direction (required exactly for the three relative actions, refused
/// elsewhere), unknown actions, and invalid correlation tokens. Owner
/// equality (exact creation/pid/exe/sid/session) stays with the caller, which
/// holds the live owner identity.
pub fn parse_workspace_request(json: &str) -> Result<WorkspaceRequest, String> {
    let request: WorkspaceRequest =
        serde_json::from_str(json).map_err(|_| "refuse: malformed workspace request".to_owned())?;
    if request.v != WORKSPACE_REQUEST_VERSION {
        return Err("refuse: workspace request version".to_owned());
    }
    if request.creation.is_empty()
        || request.pid == 0
        || request.exe_path.is_empty()
        || request.user_sid.is_empty()
        || request.correlation.is_empty()
    {
        return Err("refuse: malformed workspace request".to_owned());
    }
    if request.index > 9 {
        return Err("refuse: workspace index must be 0..=9".to_owned());
    }
    match request.action {
        WorkspaceAction::RelativeHistory
        | WorkspaceAction::RelativeSend
        | WorkspaceAction::RelativeStay => {
            if request.direction.is_none() {
                return Err("refuse: malformed workspace request".to_owned());
            }
        }
        WorkspaceAction::Select
        | WorkspaceAction::Send
        | WorkspaceAction::Stay
        | WorkspaceAction::Previous
        | WorkspaceAction::Fullscreen
        | WorkspaceAction::Float
        | WorkspaceAction::Sticky => {
            if request.direction.is_some() {
                return Err("refuse: malformed workspace request".to_owned());
            }
        }
    }
    if tiler_core::ids::CorrelationId::parse(&request.correlation).is_none() {
        return Err("refuse: malformed workspace request".to_owned());
    }
    Ok(request)
}

/// Exact KDE `requestResize` repeat tracker (portable pure step): the press
/// index for one keyboard-resize dispatch plus the stored repeat state for
/// the next press. Continues only when the focused window, direction, and
/// mode all match the stored press; any focus/direction/mode change restarts
/// at 0. Key-up never resets: like KDE, only a focus/direction/mode mismatch
/// restarts the schedule. The pixel step itself comes from
/// `tiler_core::cosmic_v1::keyboard_step_px` (12/14/16/18/20px cap), so an
/// unbounded repeat run stays capped downstream. Wire tokens throughout
/// (`left`/`right`/`up`/`down`, `outwards`/`inwards`) so the native
/// dispatcher and offline tests share this exact rule with no duplication.
/// No fingerprint input: the Windows rows cannot replicate KDE's carried
/// snapshot stability across own verified writes, and a fingerprint reset on
/// every own write would restart the mandated 12/14/16/18/20 schedule after
/// the first press. External drift still fails closed at the Engine
/// (`partial-observation`) and the next press then restarts the schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeRepeat {
    pub focused: String,
    pub direction: String,
    pub mode: String,
    pub next: u32,
}

/// Next keyboard-resize press index plus the stored state for the following
/// press. See [`ResizeRepeat`] for the exact continuation rule.
#[must_use]
pub fn resize_repeat_next(
    prev: Option<ResizeRepeat>,
    focused: &str,
    direction: &str,
    mode: &str,
) -> (u32, ResizeRepeat) {
    if let Some(state) = prev
        && state.focused == focused
        && state.direction == direction
        && state.mode == mode
    {
        let index = state.next;
        return (
            index,
            ResizeRepeat {
                focused: state.focused,
                direction: state.direction,
                mode: state.mode,
                next: index.saturating_add(1),
            },
        );
    }
    (
        0,
        ResizeRepeat {
            focused: focused.to_owned(),
            direction: direction.to_owned(),
            mode: mode.to_owned(),
            next: 1,
        },
    )
}

/// Exact-owner test-needed keyboard-resize control (tentative, pending user
/// review): `resize --direction DIR --mode MODE` queues one bounded request
/// for the normal `tile` loop only. The owner consumes it once, resolves the
/// live foreground as the resize subject through the production origin/
/// member/lifetime/scope gates, and dispatches through the real
/// `keyboard_tick` resize arm with the same fences and repeat tracker as a
/// physical chord. No synthetic input, no classifier bypass, never a proof
/// path. The production origin is the real captured foreground; the CLI
/// carries no HWND. proof owners refuse; suspended/elevated sessions refuse.
pub const RESIZE_REQUEST_FILE: &str = "resize.request";
pub const RESIZE_REQUEST_VERSION: u32 = 1;

/// CLI options for the exact-owner test-needed `resize` control (normal
/// `tile` only): `--direction` takes `left`|`right`|`up`|`down`,
/// `--mode` takes `outwards`|`inwards`, in exactly this order. The CLI only
/// queues a bounded single-pending request file; the owner loop validates
/// the full owner binding and dispatches through the production resize arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeOptions {
    pub direction: String,
    pub mode: String,
}

/// Wire-valid resize direction token (Engine parser vocabulary).
#[must_use]
pub fn resize_direction_valid(direction: &str) -> bool {
    matches!(direction, "left" | "right" | "up" | "down")
}

/// Wire-valid resize mode token (Engine parser vocabulary).
#[must_use]
pub fn resize_mode_valid(mode: &str) -> bool {
    matches!(mode, "outwards" | "inwards")
}

/// Parse the test-needed `resize` CLI form. Exactly `--direction DIR`
/// `--mode MODE` in this order; anything else (including swapped order, a
/// missing value, or repeated flags) refuses.
pub fn parse_resize_args(args: &[String]) -> Result<ResizeOptions, String> {
    let usage = "usage: resize --direction (left|right|up|down) --mode (outwards|inwards)";
    if args.len() != 4 {
        return Err(usage.to_owned());
    }
    if args[0] != "--direction" || args[2] != "--mode" {
        return Err(usage.to_owned());
    }
    if !resize_direction_valid(&args[1]) {
        return Err("refuse: resize direction must be left|right|up|down".to_owned());
    }
    if !resize_mode_valid(&args[3]) {
        return Err("refuse: resize mode must be outwards|inwards".to_owned());
    }
    Ok(ResizeOptions {
        direction: args[1].clone(),
        mode: args[3].clone(),
    })
}

/// Verify the raw received argv against the parsed `resize` options:
/// exactly `--direction DIR --mode MODE` with matching values, no unknown
/// flags.
pub fn verify_resize_argv_consistency(
    raw: &[String],
    parsed: &ResizeOptions,
) -> Result<(), String> {
    let usage = "usage: resize --direction (left|right|up|down) --mode (outwards|inwards)";
    if raw.len() != 4 || raw[0] != "--direction" || raw[2] != "--mode" {
        return Err(usage.to_owned());
    }
    if raw[1] != parsed.direction || raw[3] != parsed.mode {
        return Err("error: argv/parsed resize mismatch (impossible)".to_owned());
    }
    Ok(())
}

/// Versioned exact-owner resize request body. `creation`/`pid`/`exe_path`/
/// `user_sid`/`session_id` bind the exact ledger owner; `direction`/`mode`
/// are the validated wire tokens; `correlation` is a client-generated opaque
/// token the owner echoes in its dispatch log. No titles, no geometry, no
/// HWNDs, no secrets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResizeRequest {
    pub v: u32,
    pub creation: String,
    pub pid: u32,
    pub exe_path: String,
    pub user_sid: String,
    pub session_id: u32,
    pub direction: String,
    pub mode: String,
    pub correlation: String,
}

/// Render one resize request body. Bounds only; full owner equality stays
/// with the owner-side check.
pub fn render_resize_request(request: &ResizeRequest) -> String {
    serde_json::to_string(request).unwrap_or_default()
}

/// Parse and bound one resize request body. Refuses malformed JSON, version
/// drift, empty identity, unknown direction/mode tokens, and invalid
/// correlation tokens. Owner equality (exact creation/pid/exe/sid/session)
/// stays with the caller, which holds the live owner identity.
pub fn parse_resize_request(json: &str) -> Result<ResizeRequest, String> {
    let request: ResizeRequest =
        serde_json::from_str(json).map_err(|_| "refuse: malformed resize request".to_owned())?;
    if request.v != RESIZE_REQUEST_VERSION {
        return Err("refuse: resize request version".to_owned());
    }
    if request.creation.is_empty()
        || request.pid == 0
        || request.exe_path.is_empty()
        || request.user_sid.is_empty()
        || request.correlation.is_empty()
    {
        return Err("refuse: malformed resize request".to_owned());
    }
    if !resize_direction_valid(&request.direction) {
        return Err("refuse: malformed resize request".to_owned());
    }
    if !resize_mode_valid(&request.mode) {
        return Err("refuse: malformed resize request".to_owned());
    }
    if tiler_core::ids::CorrelationId::parse(&request.correlation).is_none() {
        return Err("refuse: malformed resize request".to_owned());
    }
    Ok(request)
}

/// Central portable borderless-fullscreen predicate: a captionless window
/// whose visible frame completely contains any monitor's full bounds
/// (`rcMonitor`, not `rcWork`). Dimensions alone never classify: captioned
/// windows are never fullscreen, and small borderless windows stay eligible.
/// Negative coordinates are handled by exact containment, not size checks.
#[must_use]
pub fn is_borderless_fullscreen(captionless: bool, visible: Rect, monitor_fulls: &[Rect]) -> bool {
    if !captionless {
        return false;
    }
    monitor_fulls
        .iter()
        .any(|full| tiler_core::bounds::rect_contained(*full, visible))
}

/// Stable tick-summary signature for log dedupe. Carries no tick: an
/// identical observation (window count, applied writes, sorted skip reasons,
/// mismatches, op, Engine revision) stays quiet no matter how many ticks pass.
/// Bounded: the caller passes already-sorted skip pairs; no arbitrary caps.
#[must_use]
pub fn tick_summary_signature(
    windows: usize,
    applied: usize,
    sorted_skipped: &[(String, String)],
    mismatched: usize,
    op: &str,
    revision: u64,
) -> String {
    let skips = sorted_skipped
        .iter()
        .map(|(t, r)| format!("{t}={r}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{windows}|{applied}|{skips}|{revision}|{mismatched}|{op}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(min_w: Option<i32>, min_h: Option<i32>) -> tiler_core::size_hints::WindowSizeHints {
        tiler_core::size_hints::WindowSizeHints {
            min_w,
            min_h,
            max_w: None,
            max_h: None,
        }
    }

    fn no_insets() -> FrameInsets {
        FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }
    }

    #[test]
    fn hints_from_outer_carries_both_sides_with_shared_inset_conversion() {
        // Fresh outer tracks convert with the same frame-inset subtraction:
        // a 640x480 client with an 8px frame on every side carries visible
        // 624x464 on both sides.
        let insets = FrameInsets {
            left: 8,
            top: 8,
            right: 8,
            bottom: 8,
        };
        let got = hints_from_outer(Some((640, 480)), Some((640, 480)), insets);
        assert_eq!(
            got,
            tiler_core::size_hints::WindowSizeHints {
                min_w: Some(624),
                min_h: Some(464),
                max_w: Some(624),
                max_h: Some(464),
            }
        );
        // Minimum-only queries keep the historical shape (maximums absent).
        assert_eq!(
            hints_from_outer(Some((640, 480)), None, no_insets()),
            tiler_core::size_hints::WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: None,
                max_h: None,
            }
        );
        assert_eq!(
            min_hints_from_outer(640, 480, no_insets()),
            hints_from_outer(Some((640, 480)), None, no_insets())
        );
        // Maximum-only queries carry no minimum.
        assert_eq!(
            hints_from_outer(None, Some((640, 480)), no_insets()),
            tiler_core::size_hints::WindowSizeHints {
                min_w: None,
                min_h: None,
                max_w: Some(640),
                max_h: Some(480),
            }
        );
        // Each axis converts independently: an invalid minimum axis keeps a
        // valid maximum axis and vice versa. Explicit zeros ride as unset
        // (G-05); negatives, sentinels, and absurd values map to unknown.
        let partial = hints_from_outer(Some((640, 0)), Some((640, 480)), no_insets());
        assert_eq!(
            (partial.min_w, partial.min_h, partial.max_w, partial.max_h),
            (Some(640), Some(0), Some(640), Some(480))
        );
        let negative = hints_from_outer(Some((-1, 480)), Some((640, 480)), no_insets());
        assert_eq!(
            (
                negative.min_w,
                negative.min_h,
                negative.max_w,
                negative.max_h
            ),
            (None, Some(480), Some(640), Some(480))
        );
        assert!(hints_from_outer(Some((-1, -2)), Some((-3, -4)), no_insets()).is_empty());
        // A bound that vanishes inside its own frame maps to unknown on that
        // axis only; the usable sibling axis still converts.
        let deep = FrameInsets {
            left: 400,
            top: 0,
            right: 400,
            bottom: 0,
        };
        let got = hints_from_outer(Some((640, 480)), Some((640, 480)), deep);
        assert_eq!(
            (got.min_w, got.min_h, got.max_w, got.max_h),
            (None, Some(480), None, Some(480))
        );
        // Minimum projection behavior is unchanged: explicit zeros behave
        // exactly like absent bounds outside the fixed-size predicate.
        let zero_min = hints_from_outer(Some((0, 480)), None, no_insets());
        assert_eq!(
            overconstrained_effective(
                Rect {
                    x: 8,
                    y: 8,
                    w: 400,
                    h: 300,
                },
                zero_min
            ),
            Rect {
                x: 8,
                y: 8,
                w: 400,
                h: 480
            }
        );
    }

    #[test]
    fn normalize_outer_bound_keeps_zero_unset_and_rejects_unusable() {
        assert_eq!(normalize_outer_bound(0), Some(0));
        assert_eq!(normalize_outer_bound(1), Some(1));
        assert_eq!(
            normalize_outer_bound(tiler_core::bounds::GEOMETRY_BOUND),
            Some(tiler_core::bounds::GEOMETRY_BOUND)
        );
        // Negative, sentinel, and absurd values are unknown, never a floor.
        assert_eq!(normalize_outer_bound(-1), None);
        assert_eq!(normalize_outer_bound(i32::MIN), None);
        assert_eq!(normalize_outer_bound(i32::MAX), None);
        assert_eq!(
            normalize_outer_bound(tiler_core::bounds::GEOMETRY_BOUND + 1),
            None
        );
        // Pairs validate per axis: one bad axis never discards its sibling.
        assert_eq!(normalize_min_track(640, 0), (Some(640), Some(0)));
        assert_eq!(normalize_min_track(-5, 480), (None, Some(480)));
        assert_eq!(normalize_min_track(0, 0), (Some(0), Some(0)));
        assert_eq!(normalize_max_track(i32::MAX, 480), (None, Some(480)));
    }

    #[test]
    fn converted_hints_classify_like_outer_tracks_under_both_predicates() {
        // Production-connected parity (no speculative helper): converted
        // visible hints classify exactly as the shared predicate decides on
        // the same vectors, because frame insets shift both sides of an axis
        // equally and preserve equality. Covers the G-05 partial-zero table,
        // guards, and one-axis cases through the real conversion path.
        use tiler_core::size_hints::FixedSizePredicate::{BothAxes, EitherAxis};
        use tiler_core::size_hints::is_fixed_size_with;
        let cases = [
            (Some((640, 480)), Some((640, 480)), true, true),
            (Some((640, 480)), None, false, false),
            (Some((640, 0)), Some((640, 0)), false, true),
            (Some((0, 480)), Some((0, 480)), false, true),
            (Some((0, 0)), Some((0, 0)), false, false),
            (None, None, false, false),
            (Some((640, 480)), Some((i32::MAX, i32::MAX)), false, false),
            (Some((640, 480)), Some((640, 960)), false, true),
            (Some((-1, 480)), Some((-1, 480)), false, false),
        ];
        for (min_outer, max_outer, both, either) in cases {
            for insets in [
                FrameInsets {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                FrameInsets {
                    left: 8,
                    top: 8,
                    right: 8,
                    bottom: 8,
                },
            ] {
                let hints = hints_from_outer(min_outer, max_outer, insets);
                assert_eq!(
                    is_fixed_size_with(hints, BothAxes),
                    both,
                    "both-axes {min_outer:?}/{max_outer:?} insets {insets:?}"
                );
                assert_eq!(
                    is_fixed_size_with(hints, EitherAxis),
                    either,
                    "either-axis {min_outer:?}/{max_outer:?} insets {insets:?}"
                );
            }
        }
    }

    #[test]
    fn overconstrained_effective_keeps_origin_and_raises_to_minimum() {
        // Planned tile origin is kept; each extent rises to the known native
        // minimum. Real5 shape: a small proportional tile for Paint clamped
        // to its 864x617-class minimum stays at the tile origin.
        let planned = Rect {
            x: 8,
            y: 8,
            w: 400,
            h: 300,
        };
        let got = overconstrained_effective(planned, hints(Some(864), Some(617)));
        assert_eq!(
            got,
            Rect {
                x: 8,
                y: 8,
                w: 864,
                h: 617
            }
        );
        // Satisfiable extents pass through byte-identical.
        assert_eq!(
            overconstrained_effective(planned, hints(Some(100), Some(100))),
            planned
        );
        // Unknown minimums (no hint) keep the planned rect.
        assert_eq!(
            overconstrained_effective(planned, tiler_core::size_hints::WindowSizeHints::none()),
            planned
        );
        // Non-meaningful hints (zero/absurd) behave as absent.
        assert_eq!(
            overconstrained_effective(planned, hints(Some(0), Some(-5))),
            planned
        );
        // Only the violating axis grows.
        let wide = overconstrained_effective(planned, hints(Some(500), None));
        assert_eq!((wide.x, wide.y, wide.w, wide.h), (8, 8, 500, 300));
    }

    #[test]
    fn overconstrained_target_is_refused_stable() {
        // Minimum clamping must not fight every tick: the identical
        // (effective, observed) clamp pair suppresses, while any genuine
        // change re-arms immediately.
        let planned = Rect {
            x: 8,
            y: 8,
            w: 400,
            h: 300,
        };
        let effective = overconstrained_effective(planned, hints(Some(864), Some(617)));
        let observed = effective;
        let mut tracker = RefusedTracker::default();
        let now = std::time::Instant::now();
        // Exact match clears: no skip, no clamp lane.
        tracker.note_match("w1");
        assert!(!tracker.should_skip("w1", &effective, &observed, now));
        // Successful write that reads back at the effective target is a
        // match, not a clamp.
        assert_eq!(
            readback_outcome(true, &effective, &observed),
            ReadbackOutcome::Match
        );
        // App-held larger size pins the identical pair; the same pair then
        // suppresses instead of retrying every tick.
        let held = Rect {
            x: 8,
            y: 8,
            w: 900,
            h: 700,
        };
        assert_eq!(
            readback_outcome(true, &effective, &held),
            ReadbackOutcome::Clamp
        );
        tracker.note_clamp("w1", &effective, &held);
        assert!(tracker.should_skip("w1", &effective, &held, now));
        // A new planned tile or a new observation re-arms.
        let moved = Rect {
            x: 100,
            ..effective
        };
        assert!(!tracker.should_skip("w1", &moved, &held, now));
        assert!(!tracker.should_skip("w1", &effective, &planned, now));
    }

    #[test]
    fn drop_fence_is_source_bounds_only() {
        // Inside (including origin edge, exclusive far edge).
        assert!(drop_point_in_domain(0, 0, 100, 100, 0, 0));
        assert!(drop_point_in_domain(0, 0, 100, 100, 99, 99));
        // Outside: cross-output and off-work-area refuse alike.
        assert!(!drop_point_in_domain(0, 0, 100, 100, 100, 50));
        assert!(!drop_point_in_domain(0, 0, 100, 100, -1, 50));
        assert!(!drop_point_in_domain(0, 0, 100, 100, 50, 100));
        assert!(!drop_point_in_domain(2560, 0, 100, 100, 50, 50));
        // Degenerate bounds never admit.
        assert!(!drop_point_in_domain(0, 0, 0, 100, 0, 50));
        assert!(!drop_point_in_domain(0, 0, 100, 0, 50, 0));
    }
}
