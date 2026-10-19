#![forbid(unsafe_code)]
//! Portable Windows active-border policy, settings, and raster math.
//!
//! KDE parity source: `kwin/native-effect/activeborderconfig.kcfg`
//! (width 3.0, gap 0.0, `UseThemeColor` true, fallback `#2a82da`)
//! and `tiler-core::visual` (`active_border_state`, `active_border_inner_rect`,
//! `active_border_use_theme`). Outline radius defaults to 6.0 per the
//! 2026-10-11 decision. The border is independent of Engine
//! tiling/floating membership: eligibility never consults a tiled token.
//!
//! Theme colour mapping (confirmed in source): KDE selects the Plasma
//! highlight colour - `KColorScheme(QPalette::Active, KColorScheme::Selection,
//! kdeglobals).background()` gated on `isColorSetSupported(..., Selection)`
//! (`kwin/native-effect/activewindowborder.cpp:373-380`) - with the
//! configured `BorderColor` as fallback. The Windows analogue is the
//! Personalization accent from WinRT
//! `UISettings::GetColorValue(UIColorType::Accent)`: theme wins only when
//! requested (`--active-border-theme` on), available, and positively
//! alpha-quantized per the shared `tiler-core::visual` gate.

use tiler_core::geometry::Rect;
use tiler_core::visual::{VisualRect, active_border_inner_rect, active_border_state};

/// Windows border defaults.
pub const DEFAULT_WIDTH: f64 = 3.0;
pub const DEFAULT_GAP: f64 = 0.0;
pub const DEFAULT_RADIUS: f64 = 6.0;
/// Fallback border colour (`#2a82da`, KDE `BorderColor` parity), used when
/// the system accent is unavailable or theming is off.
pub const DEFAULT_COLOR_HEX: &str = "#2a82da";
pub const DEFAULT_COLOR_RGB: (u8, u8, u8) = (0x2a, 0x82, 0xda);
/// Theme follows the live system accent by default (KDE `UseThemeColor`
/// parity); the configured colour is the fallback.
pub const DEFAULT_USE_THEME: bool = true;

/// KCM spinbox maximums (`unifiedsettings.ui`): width 32, radius 64, gap 64.
/// Minimums are 0 (a zero width paints nothing but stays valid).
pub const MAX_WIDTH: f64 = 32.0;
pub const MAX_GAP: f64 = 64.0;
pub const MAX_RADIUS: f64 = 64.0;

/// Configured border style. All values are logical KDE points; the native
/// layer scales them to physical pixels at the target window's DPI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveBorderStyle {
    pub width: f64,
    pub gap: f64,
    pub radius: f64,
    pub color: (u8, u8, u8),
    pub use_theme: bool,
}

impl Default for ActiveBorderStyle {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            gap: DEFAULT_GAP,
            radius: DEFAULT_RADIUS,
            color: DEFAULT_COLOR_RGB,
            use_theme: DEFAULT_USE_THEME,
        }
    }
}

/// Full border switch: default on, explicit `--no-active-border` off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveBorderOptions {
    pub enabled: bool,
    pub style: ActiveBorderStyle,
}

impl Default for ActiveBorderOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            style: ActiveBorderStyle::default(),
        }
    }
}

/// Parse a logical width in `0..=32` (KCM maximum).
pub fn parse_width(value: &str) -> Result<f64, String> {
    let usage = "refuse: --active-border-width needs 0..=32";
    let parsed: f64 = value.parse().map_err(|_| usage.to_owned())?;
    if !parsed.is_finite() || parsed < 0.0 || parsed > MAX_WIDTH {
        return Err(usage.to_owned());
    }
    Ok(parsed)
}

/// Parse a logical gap in `0..=64` (KCM maximum).
pub fn parse_gap(value: &str) -> Result<f64, String> {
    let usage = "refuse: --active-border-gap needs 0..=64";
    let parsed: f64 = value.parse().map_err(|_| usage.to_owned())?;
    if !parsed.is_finite() || parsed < 0.0 || parsed > MAX_GAP {
        return Err(usage.to_owned());
    }
    Ok(parsed)
}

/// Parse a logical radius in `0..=64` (KCM maximum).
pub fn parse_radius(value: &str) -> Result<f64, String> {
    let usage = "refuse: --active-border-radius needs 0..=64";
    let parsed: f64 = value.parse().map_err(|_| usage.to_owned())?;
    if !parsed.is_finite() || parsed < 0.0 || parsed > MAX_RADIUS {
        return Err(usage.to_owned());
    }
    Ok(parsed)
}

/// Parse `#rrggbb` (case-insensitive, `#` required). Alpha forms are refused:
/// the KDE `BorderColor` default is opaque and the Windows accent arrives as
/// a separate `SystemAccent` (its alpha feeds the theme gate, never the ring).
pub fn parse_color(value: &str) -> Result<(u8, u8, u8), String> {
    let usage = "refuse: --active-border-color needs #rrggbb";
    let hex = value.strip_prefix('#').ok_or_else(|| usage.to_owned())?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(usage.to_owned());
    }
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).map_err(|_| usage.to_owned())
    };
    Ok((channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

/// Render a colour back to `#rrggbb`.
#[must_use]
pub fn render_color((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Scale one logical KDE point value to physical pixels at `dpi`.
/// Matches KWin `itemrenderer_opengl.cpp` (`round(outline.thickness *
/// scale)`): plain rounding, so a positive logical value below half a
/// physical pixel rounds to 0 (paints nothing, valid). Zero stays zero.
/// `None` on non-positive DPI or overflow.
#[must_use]
pub fn scale_to_physical(logical: f64, dpi: u32) -> Option<i32> {
    if dpi == 0 || !logical.is_finite() || logical < 0.0 {
        return None;
    }
    if logical == 0.0 {
        return Some(0);
    }
    let scaled = logical * f64::from(dpi) / 96.0;
    if !scaled.is_finite() {
        return None;
    }
    if scaled <= 0.0 {
        return None;
    }
    let rounded = scaled.round() as i64;
    if !(0..=i64::from(i32::MAX)).contains(&rounded) {
        return None;
    }
    Some(rounded as i32)
}

/// Scale the full style to physical pixels. `None` when any channel fails.
#[must_use]
pub fn scale_style(style: &ActiveBorderStyle, dpi: u32) -> Option<(i32, i32, i32)> {
    Some((
        scale_to_physical(style.width, dpi)?,
        scale_to_physical(style.gap, dpi)?,
        scale_to_physical(style.radius, dpi)?,
    ))
}

/// Live Windows system accent: the Personalization theme colour from WinRT
/// `UISettings::GetColorValue(UIColorType::Accent)`, as RGB plus its alpha
/// byte. `None` (query unavailable or failed) always falls back to the
/// configured colour. Mirrors the KDE theme source
/// (`KColorScheme` Selection background, `activewindowborder.cpp:373-380`):
/// an accent analogue, not a button-face or highlight-selection metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemAccent {
    pub color: (u8, u8, u8),
    pub alpha: u8,
}

impl SystemAccent {
    /// Split a WinRT `Color` (`A`, `R`, `G`, `B` bytes) into RGB plus the
    /// alpha byte the core theme gate (`theme_alpha > 0`) consumes.
    #[must_use]
    pub const fn from_winrt_color(alpha: u8, red: u8, green: u8, blue: u8) -> Self {
        Self {
            color: (red, green, blue),
            alpha,
        }
    }
}

/// Raw result of one owner accent resolve: the scope-lifetime WinRT init
/// result plus the activation/query HRESULTs and the returned accent, if
/// any. Produced only by an actual native resolve (startup or a broadcast
/// version move); steady-state cache hits produce nothing, so one raw
/// result always means one resolve ran. The init policy lives in
/// `crate::ffi::accent`; this struct only carries the observed values for
/// the diagnostic record below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccentRaw {
    /// Cached scope-lifetime `RoInitialize` HRESULT (`S_OK`, `S_FALSE`,
    /// `RPC_E_CHANGED_MODE`, or another error). Reported on every record,
    /// but `RoInitialize` itself ran at most once per scope: later
    /// resolves reuse the cached result without re-initializing.
    pub init_hr: i32,
    /// `UISettings` activation HRESULT (`Some(0)` on success); `None` when
    /// the apartment was unusable and activation never ran. Success is a
    /// safe projection through the `windows` `Result` mapping (ok to `0`,
    /// i.e. `S_OK`), not an exact raw code: a non-zero success (such as
    /// `S_FALSE`) would read as `0` here. Only init keeps the exact raw
    /// HRESULT; activation/query keep ok-vs-error-code.
    pub activate_hr: Option<i32>,
    /// `GetColorValue` HRESULT (`Some(0)` on success); `None` when
    /// activation failed or never ran. Same ok-to-`0` projection as
    /// `activate_hr`: success reads `0`, never an exact non-zero code.
    pub query_hr: Option<i32>,
    /// The returned accent, if the query ran to completion.
    pub accent: Option<SystemAccent>,
}

/// Closed init-result vocabulary for the diagnostic record. Mirrors the
/// `crate::ffi::accent` policy constants (`windows_sys::Win32::Foundation`
/// `S_OK`/`S_FALSE`/`RPC_E_CHANGED_MODE`, log labels only): `Ok` created a
/// new apartment, `Already` found one present, `ChangedMode` refused a
/// foreign apartment, `Failed` is any other error. Both successes own their
/// matching `RoUninitialize`; neither failure does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentInit {
    Ok,
    Already,
    ChangedMode,
    Failed,
}

impl AccentInit {
    /// Classify one raw `RoInitialize` HRESULT for the log record. The
    /// query policy itself lives in `crate::ffi::accent`; this mapping only
    /// names the observed value.
    #[must_use]
    pub const fn classify(hr: i32) -> Self {
        if hr == S_OK_HR {
            Self::Ok
        } else if hr == S_FALSE_HR {
            Self::Already
        } else if hr == RPC_E_CHANGED_MODE_HR {
            Self::ChangedMode
        } else {
            Self::Failed
        }
    }

    /// Closed log vocabulary for the init result.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Already => "already",
            Self::ChangedMode => "changed-mode",
            Self::Failed => "failed",
        }
    }
}

/// Raw WinRT HRESULT values mirrored from
/// `windows_sys::Win32::Foundation` (consumed by `crate::ffi::accent`).
/// Duplicated here so the portable diagnostic record can name the observed
/// init result without a native import; the init policy itself is not
/// duplicated.
const S_OK_HR: i32 = 0;
const S_FALSE_HR: i32 = 1;
const RPC_E_CHANGED_MODE_HR: i32 = 0x8001_0106u32 as i32;

/// Which surface the accent diagnostic names. Exactly one production
/// source exists: the owner-thread resolve (`Shared`, role `Owner`). The
/// `Border`/`Pager` variants name only historical per-surface records kept
/// by offline tests and the diagnostic FIFO; production never
/// fabricates a per-surface query record on a cache read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentQuerySurface {
    Border,
    Pager,
    Shared,
}

impl AccentQuerySurface {
    /// Closed log vocabulary for the querying surface.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Border => "border",
            Self::Pager => "pager",
            Self::Shared => "shared",
        }
    }
}

/// Which thread ran the native accent query. Production uses only `Owner`;
/// historical renderer/same-thread vocabulary remains for diagnostic tests.
/// No thread id is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentQueryRole {
    /// Historical renderer-thread query, retained for diagnostic fixtures.
    Renderer,
    /// Shared source: the owner/loop thread queries for both surfaces.
    Owner,
    /// Historical same-thread border query, retained for diagnostic fixtures.
    SameThread,
}

impl AccentQueryRole {
    /// Closed log vocabulary for the querying thread role.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Renderer => "renderer",
            Self::Owner => "owner",
            Self::SameThread => "same-thread",
        }
    }
}

/// What triggered the native accent query: the empty cache (first query of
/// the owner scope) or a broadcast version move. Steady state never queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentQueryTrigger {
    Startup,
    Broadcast,
}

impl AccentQueryTrigger {
    /// Closed log vocabulary for the query trigger.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Broadcast => "broadcast",
        }
    }
}

/// Resolved query outcome. `Accent` means the raw accent was usable and the
/// consumer paints it; `Invalid` means the query returned but the value was
/// unusable (zero alpha under the shared availability rule) so the fallback
/// paints; `Fallback` means the query never produced a value (init,
/// activation, or query failure) or the consumer is theme-off, so the
/// fallback paints. A successful black accent (`#ff000000`) is `Accent`,
/// never `Invalid`: black is legitimate, only zero alpha is unusable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentQueryOutcome {
    Accent,
    Fallback,
    Invalid,
}

impl AccentQueryOutcome {
    /// Closed log vocabulary for the resolved outcome.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::Fallback => "fallback",
            Self::Invalid => "invalid",
        }
    }
}

/// Why the fallback paints instead of the live accent. `Unavailable` covers
/// init/activation/query failures (no value returned); `ZeroAlpha` covers a
/// returned but unusable value; `ThemeOff` covers a usable value the border
/// consumer declined (`--no-active-border-theme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentFallbackReason {
    Unavailable,
    ZeroAlpha,
    ThemeOff,
}

impl AccentFallbackReason {
    /// Closed log vocabulary for the fallback reason.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::ZeroAlpha => "zero-alpha",
            Self::ThemeOff => "theme-off",
        }
    }
}

/// One per-query diagnostic record: the raw native result plus the consumer
/// resolution it produced. Emitted once per actual query (startup or
/// broadcast re-query, even when the resolved pixels match the previous
/// state); steady-state cache hits emit nothing. Raw source validity
/// (`raw_valid`) is what the API returned; `outcome`/`resolved` applies the
/// supplied policy. Shared-source records use pager policy, not border pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccentQueryReport {
    pub surface: AccentQuerySurface,
    pub role: AccentQueryRole,
    pub trigger: AccentQueryTrigger,
    /// Shared broadcast version the query ran against.
    pub version: u64,
    pub init: AccentInit,
    pub init_hr: i32,
    pub activate_hr: Option<i32>,
    pub query_hr: Option<i32>,
    /// The returned accent, if the query ran to completion (`None` when
    /// init/activation/query failed or never ran).
    pub raw: Option<SystemAccent>,
    pub outcome: AccentQueryOutcome,
    pub fallback_reason: Option<AccentFallbackReason>,
    /// RGB under the report's policy (shared-source records use pager policy).
    pub resolved: (u8, u8, u8),
}

impl AccentQueryReport {
    /// Raw source validity: the query returned a present, positively
    /// alpha-quantized accent under the shared availability rule. Black
    /// with full alpha is valid; only zero alpha (or no value) is not.
    #[must_use]
    pub fn raw_valid(self) -> bool {
        accent_available(self.raw)
    }
}

/// Build one diagnostic record from a raw native result plus the consumer
/// policy. `use_theme` is true for the pager (it always follows the accent)
/// and the border theme switch otherwise; `fallback` is the consumer
/// fallback colour. The resolution reuses the shared availability rule and
/// `effective_color` policy exactly: `outcome`/`resolved` agree with that
/// policy, independently of whether a native surface was presented.
///
/// The single shared owner resolve passes the pager policy (`use_theme`
/// true, default-blue fallback), so its `resolved` names the shared-source
/// fallback, not any one consumer's pixels: a theme-off border with a
/// custom configured colour paints its own fallback; the shared record uses
/// the accent, or default blue when unavailable. Each appearance log carries its
/// actual painted pixels and source.
#[must_use]
pub fn accent_query_report(
    surface: AccentQuerySurface,
    role: AccentQueryRole,
    trigger: AccentQueryTrigger,
    version: u64,
    raw: &AccentRaw,
    use_theme: bool,
    fallback: (u8, u8, u8),
) -> AccentQueryReport {
    let (outcome, fallback_reason, resolved) = match raw.accent {
        None => (
            AccentQueryOutcome::Fallback,
            Some(AccentFallbackReason::Unavailable),
            fallback,
        ),
        Some(accent) if !accent_available(Some(accent)) => (
            AccentQueryOutcome::Invalid,
            Some(AccentFallbackReason::ZeroAlpha),
            fallback,
        ),
        Some(_) if !use_theme => (
            AccentQueryOutcome::Fallback,
            Some(AccentFallbackReason::ThemeOff),
            fallback,
        ),
        Some(accent) => (AccentQueryOutcome::Accent, None, accent.color),
    };
    AccentQueryReport {
        surface,
        role,
        trigger,
        version,
        init: AccentInit::classify(raw.init_hr),
        init_hr: raw.init_hr,
        activate_hr: raw.activate_hr,
        query_hr: raw.query_hr,
        raw: raw.accent,
        outcome,
        fallback_reason,
        resolved,
    }
}

/// Render a returned accent as `#aarrggbb` (alpha included). Only used for
/// the diagnostic record, and only when the query actually returned a value.
#[must_use]
pub fn render_argb(accent: SystemAccent) -> String {
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        accent.alpha, accent.color.0, accent.color.1, accent.color.2
    )
}

/// Normal-level log line for one accent query. Only closed-vocabulary
/// strings, numeric HRESULTs, versions, and colours: no thread ids, window
/// handles, paths, executable names, or error payloads. Skipped
/// activation/query stages serialize as `null`, never a fabricated error;
/// `argb` is present only when the query actually returned a value.
#[must_use]
pub fn accent_query_event(report: &AccentQueryReport) -> serde_json::Value {
    serde_json::json!({
        "event": "accent-query",
        "surface": report.surface.name(),
        "role": report.role.name(),
        "trigger": report.trigger.name(),
        "version": report.version,
        "init": report.init.label(),
        "init_hr": report.init_hr,
        "activate_hr": report.activate_hr,
        "query_hr": report.query_hr,
        "argb": report.raw.map(render_argb),
        "raw_valid": report.raw_valid(),
        "outcome": report.outcome.name(),
        "fallback_reason": report.fallback_reason.map(AccentFallbackReason::name),
        "resolved": render_color(report.resolved),
    })
}

/// Trace-only log line for one stage within an actual native accent query.
/// Only closed-vocabulary strings plus the shared broadcast version: no
/// HRESULTs, no colours, no thread ids, no paths. Emitted synchronously by
/// the querying owner thread (never the renderer, which never file-logs);
/// the normal completion record ([`accent_query_event`]) is unchanged.
#[cfg(windows)]
#[must_use]
pub fn accent_query_stage_event(
    surface: AccentQuerySurface,
    role: AccentQueryRole,
    trigger: AccentQueryTrigger,
    version: u64,
    stage: crate::ffi::AccentStage,
) -> serde_json::Value {
    serde_json::json!({
        "event": "accent-query-stage",
        "surface": surface.name(),
        "role": role.name(),
        "trigger": trigger.name(),
        "version": version,
        "stage": stage.name(),
    })
}

/// One scope's shared cached accent plus the broadcast version it was taken
/// at. Exactly one instance lives in the owner state and both surfaces (the
/// border and the pager) consume the same snapshot: the owner resolves once
/// per version and every consumer paints from that single read. Refresh only
/// when the shared version moved since the last resolve attempt:
/// steady state performs no native resolve, and a failed resolve sticks to
/// its fallback until the next broadcast (no per-pump retry spam). The
/// pre-query version is recorded, so a broadcast landing mid-query
/// re-triggers the next refresh instead of being lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccentCache {
    accent: Option<SystemAccent>,
    version: Option<u64>,
}

impl AccentCache {
    /// Currently cached accent (`None` until the first successful refresh or
    /// while the query is unavailable).
    #[must_use]
    pub const fn get(&self) -> Option<SystemAccent> {
        self.accent
    }

    /// Last broadcast version a query ran against (`None` until the first
    /// query). Lets callers name the query trigger (`startup` for the first
    /// query, `broadcast` afterwards) without re-querying.
    #[must_use]
    pub const fn last_version(&self) -> Option<u64> {
        self.version
    }

    /// Refresh the cache against the shared broadcast version, running
    /// `query` only when it moved since the last attempt. Returns the raw
    /// native result when a resolve ran (`None` on steady-state hits, which
    /// perform no native resolve and emit no diagnostic). The cached
    /// accent/version update is identical to [`AccentCache::refresh`].
    pub fn refresh_detailed(
        &mut self,
        current_version: u64,
        query: impl FnOnce() -> AccentRaw,
    ) -> Option<AccentRaw> {
        if self.version == Some(current_version) {
            return None;
        }
        let raw = query();
        self.accent = raw.accent;
        self.version = Some(current_version);
        Some(raw)
    }

    /// Refresh the cache against the shared broadcast version. Returns true
    /// when a query ran (`current_version` moved since the last attempt).
    pub fn refresh(
        &mut self,
        current_version: u64,
        query: impl FnOnce() -> Option<SystemAccent>,
    ) -> bool {
        if self.version == Some(current_version) {
            return false;
        }
        self.accent = query();
        self.version = Some(current_version);
        true
    }
}

/// Availability of a queried accent under the shared core rule: present
/// and positively alpha-quantized (`active_border_use_theme` with the theme
/// switch forced on and the theme valid, so only the alpha gate decides).
/// The pager ignores the theme on/off switch but obeys this availability,
/// so a zero-alpha accent is unavailable everywhere - never a new
/// pager/border divergence.
#[must_use]
pub fn accent_available(accent: Option<SystemAccent>) -> bool {
    accent.is_some_and(|a| {
        tiler_core::visual::active_border_use_theme(true, true, i32::from(a.alpha))
    })
}

/// Closed log vocabulary for an accent-resolved fill source: `accent` when
/// the colour resolved from an available live accent, `fallback` otherwise.
/// Never claims the accent when the query was unavailable.
#[must_use]
pub const fn accent_source_label(sourced: bool) -> &'static str {
    if sourced { "accent" } else { "fallback" }
}

/// Whether the effective border colour resolves from the live accent.
/// Mirrors `effective_color`'s condition exactly (availability plus the
/// theme switch), so source reporting can never diverge from the painted
/// colour.
#[must_use]
pub fn effective_uses_accent(style: &ActiveBorderStyle, accent: Option<SystemAccent>) -> bool {
    accent_available(accent) && style.use_theme
}

/// Observe one accent broadcast version without querying: true once per
/// movement, for wake fanout. Steady versions stay silent, so idle
/// ack-only turns are never promoted by an unchanged version.
#[must_use]
pub fn accent_version_moved(seen: &mut u64, current: u64) -> bool {
    if *seen == current {
        return false;
    }
    *seen = current;
    true
}
/// Effective border colour with the live system accent. Mirrors KDE
/// `activeBorderColor` (`activeborderlogic.h:23-32`): the accent wins only
/// when requested (`use_theme`, i.e. `--active-border-theme`), present,
/// and positively alpha-quantized per `active_border_use_theme`; otherwise
/// the configured fallback (`--active-border-color`, default `#2a82da`) wins.
#[must_use]
pub fn effective_color(style: &ActiveBorderStyle, accent: Option<SystemAccent>) -> (u8, u8, u8) {
    if effective_uses_accent(style, accent) {
        accent.map(|theme| theme.color).unwrap_or(style.color)
    } else {
        style.color
    }
}

fn visual_of(rect: Rect) -> VisualRect {
    VisualRect {
        x: rect.x as f64,
        y: rect.y as f64,
        w: rect.w as f64,
        h: rect.h as f64,
    }
}

/// Inner rectangle: the visible frame expanded by the physical gap.
/// Governed by `tiler-core::visual::active_border_inner_rect`.
#[must_use]
pub fn border_inner_rect(visible: Rect, gap_px: i32) -> Rect {
    let inner = active_border_inner_rect(visual_of(visible), f64::from(gap_px));
    Rect {
        x: inner.x.round() as i32,
        y: inner.y.round() as i32,
        w: inner.w.round() as i32,
        h: inner.h.round() as i32,
    }
}

/// Outer rectangle: the inner rectangle expanded by the physical border
/// width on every side. The KDE `OutlinedBorderItem` paints its `BorderOutline`
/// thickness outside `innerRect` (`updateBorder` sets the inner rect and the
/// outline separately), so the ring occupies `[inner, outer)`.
#[must_use]
pub fn border_outer_rect(visible: Rect, gap_px: i32, width_px: i32) -> Option<Rect> {
    let inner = border_inner_rect(visible, gap_px);
    let w = i64::from(width_px);
    let x = i64::from(inner.x).checked_sub(w)?;
    let y = i64::from(inner.y).checked_sub(w)?;
    let w_out = i64::from(inner.w).checked_add(2 * w)?;
    let h_out = i64::from(inner.h).checked_add(2 * w)?;
    if w_out <= 0 || h_out <= 0 || w_out > i64::from(i32::MAX) || h_out > i64::from(i32::MAX) {
        return None;
    }
    if x < i64::from(i32::MIN)
        || x > i64::from(i32::MAX)
        || y < i64::from(i32::MIN)
        || y > i64::from(i32::MAX)
    {
        return None;
    }
    Some(Rect {
        x: x as i32,
        y: y as i32,
        w: w_out as i32,
        h: h_out as i32,
    })
}

/// Closed hide-reason vocabulary for the border (no tiling skip reuse).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderHideReason {
    Disabled,
    NoTarget,
    Minimized,
    Maximized,
    Fullscreen,
    Cloaked,
    HiddenWorkspace,
    Shell,
}

impl BorderHideReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoTarget => "no-target",
            Self::Minimized => "minimized",
            Self::Maximized => "maximized",
            Self::Fullscreen => "fullscreen",
            Self::Cloaked => "cloaked",
            Self::HiddenWorkspace => "hidden-workspace",
            Self::Shell => "shell",
        }
    }
}

/// Border eligibility for one foreground target, governed by
/// `tiler-core::visual::active_border_state`. Independent of Engine
/// tiling/floating membership: no tiled token participates. `maximized` is
/// the single Windows zoom axis (mapped to the horizontal maximize input).
/// `excluded` folds the Windows analogues of the KDE applet-popup exclusion:
/// cloaked surfaces, hidden-workspace members, and shell surfaces.
/// The core verdict is authoritative: reasons are derived after the verdict,
/// never pre-returned ahead of it.
#[allow(clippy::too_many_arguments)]
pub fn border_eligible(
    enabled: bool,
    has_window: bool,
    frame: Rect,
    minimized: bool,
    fullscreen: bool,
    maximized: bool,
    cloaked: bool,
    hidden_workspace: bool,
    shell: bool,
) -> Result<(), BorderHideReason> {
    if !enabled {
        return Err(BorderHideReason::Disabled);
    }
    if !has_window {
        return Err(BorderHideReason::NoTarget);
    }
    let maximized_core = tiler_core::visual::active_border_is_maximized(maximized, false);
    let state = active_border_state(
        has_window,
        visual_of(frame),
        /* deleted = */ false,
        minimized,
        fullscreen,
        maximized_core,
        cloaked || hidden_workspace || shell,
    );
    if state.visible {
        return Ok(());
    }
    if minimized {
        return Err(BorderHideReason::Minimized);
    }
    if fullscreen {
        return Err(BorderHideReason::Fullscreen);
    }
    if maximized {
        return Err(BorderHideReason::Maximized);
    }
    if cloaked {
        return Err(BorderHideReason::Cloaked);
    }
    if hidden_workspace {
        return Err(BorderHideReason::HiddenWorkspace);
    }
    if shell {
        return Err(BorderHideReason::Shell);
    }
    Err(BorderHideReason::NoTarget)
}

/// True when the point lies inside the rounded rectangle `(x, y, w, h)` with
/// corner radius `radius` (clamped to half the smallest side).
#[must_use]
pub fn point_in_rounded_rect(
    px: i32,
    py: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    if px < x || py < y || px >= x + w || py >= y + h {
        return false;
    }
    let r = radius.max(0).min(w / 2).min(h / 2);
    if r == 0 {
        return true;
    }
    // Edge bands (pixel centres): a pixel column/row inside the straight
    // section is covered regardless of the other axis.
    if px >= x + r && px < x + w - r {
        return true;
    }
    if py >= y + r && py < y + h - r {
        return true;
    }
    // Corner discs in doubled coordinates (pixel centres at odd coordinates,
    // corner centres at even ones) so the boundary is exact without floats.
    let cx2 = if px < x + r {
        2 * i64::from(x) + 2 * i64::from(r)
    } else {
        2 * i64::from(x) + 2 * i64::from(w) - 2 * i64::from(r)
    };
    let cy2 = if py < y + r {
        2 * i64::from(y) + 2 * i64::from(r)
    } else {
        2 * i64::from(y) + 2 * i64::from(h) - 2 * i64::from(r)
    };
    let dx = 2 * i64::from(px) + 1 - cx2;
    let dy = 2 * i64::from(py) + 1 - cy2;
    let rr = 2 * i64::from(r);
    dx * dx + dy * dy <= rr * rr
}

/// True when the overlay-local pixel `(px, py)` paints: inside the outer
/// rounded rect but outside the inner rounded rect. The overlay origin is the
/// outer rect origin. Matches KWin `outlinedborderitem.cpp` +
/// `itemrenderer_opengl.cpp`: `radius` is the INNER radius (the `Border`
/// shader `box` is the inner rect), the outer corner extent is
/// `radius + width`, and the inner hole at `(width, width)` keeps `radius`.
#[must_use]
pub fn ring_covers(
    px: i32,
    py: i32,
    outer_w: i32,
    outer_h: i32,
    width_px: i32,
    radius_px: i32,
) -> bool {
    if width_px <= 0 {
        return false;
    }
    // Square inner corners stay square (KWin emits no corner quads when
    // radius is 0); rounded inner corners extend the outer corner by the
    // ring width (KWin corner extent thickness + radius).
    let outer_r = if radius_px > 0 {
        radius_px.saturating_add(width_px)
    } else {
        0
    };
    if !point_in_rounded_rect(px, py, 0, 0, outer_w, outer_h, outer_r) {
        return false;
    }
    !point_in_rounded_rect(
        px,
        py,
        width_px,
        width_px,
        outer_w - 2 * width_px,
        outer_h - 2 * width_px,
        radius_px.max(0),
    )
}

/// Echo of parsed border flags for argv-consistency verification.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BorderArgvEcho {
    pub no_active_border: bool,
    pub width: Option<String>,
    pub gap: Option<String>,
    pub radius: Option<String>,
    pub color: Option<String>,
    pub no_theme: bool,
    pub theme: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn defaults_match_kde_parity() {
        let options = ActiveBorderOptions::default();
        assert!(options.enabled);
        assert_eq!(options.style.width, 3.0);
        assert_eq!(options.style.gap, 0.0);
        assert_eq!(options.style.radius, 6.0);
        assert_eq!(options.style.color, (0x2a, 0x82, 0xda));
        assert!(options.style.use_theme);
        assert_eq!(render_color(options.style.color), "#2a82da");
    }

    #[test]
    fn setting_ranges_follow_kcm_maximums() {
        assert!(parse_width("0").is_ok());
        assert!(parse_width("3").is_ok());
        assert!(parse_width("32").is_ok());
        assert!(parse_width("32.1").is_err());
        assert!(parse_width("-1").is_err());
        assert!(parse_width("nan").is_err());
        assert!(parse_gap("64").is_ok());
        assert!(parse_gap("64.1").is_err());
        assert!(parse_radius("64").is_ok());
        assert!(parse_radius("65").is_err());
    }

    #[test]
    fn color_requires_rrggbb() {
        assert_eq!(parse_color("#2a82da"), Ok((0x2a, 0x82, 0xda)));
        assert_eq!(parse_color("#2A82DA"), Ok((0x2a, 0x82, 0xda)));
        assert!(parse_color("2a82da").is_err());
        assert!(parse_color("#fff").is_err());
        assert!(parse_color("#402a82da").is_err());
        assert!(parse_color("#zzzzzz").is_err());
    }

    #[test]
    fn dpi_scaling_rounds_physical_pixels() {
        // 125% (120 DPI): 3.0 logical -> 4 physical (3.75 rounds to 4).
        assert_eq!(scale_to_physical(3.0, 120), Some(4));
        assert_eq!(scale_to_physical(3.0, 96), Some(3));
        assert_eq!(scale_to_physical(0.0, 120), Some(0));
        assert_eq!(scale_to_physical(3.0, 0), None);
        assert_eq!(scale_to_physical(-1.0, 96), None);
    }

    #[test]
    fn fractional_width_below_half_pixel_rounds_to_zero_kwin_parity() {
        // KWin `round(thickness * scale)`: sub-half-pixel widths round to 0
        // (valid, paints nothing) rather than clamping to 1.
        assert_eq!(scale_to_physical(0.4, 96), Some(0));
        assert_eq!(scale_to_physical(0.1, 96), Some(0));
        assert_eq!(scale_to_physical(0.4, 120), Some(1));
        assert_eq!(scale_to_physical(0.49, 96), Some(0));
        assert_eq!(scale_to_physical(0.5, 96), Some(1));
        assert_eq!(
            scale_style(
                &ActiveBorderStyle {
                    width: 0.4,
                    ..ActiveBorderStyle::default()
                },
                96
            )
            .map(|s| s.0),
            Some(0)
        );
    }

    #[test]
    fn eligibility_agrees_with_core_verdict() {
        // Core `active_border_state` is authoritative: for every flag
        // combination the portable verdict matches the core visibility, and
        // hidden reasons derive after the verdict.
        let frame = rect(10, 20, 320, 200);
        for bits in 0..64u32 {
            let minimized = bits & 1 != 0;
            let fullscreen = bits & 2 != 0;
            let maximized = bits & 4 != 0;
            let cloaked = bits & 8 != 0;
            let hidden = bits & 16 != 0;
            let shell = bits & 32 != 0;
            let maximized_core = tiler_core::visual::active_border_is_maximized(maximized, false);
            let core = tiler_core::visual::active_border_state(
                true,
                tiler_core::visual::VisualRect {
                    x: 10.0,
                    y: 20.0,
                    w: 320.0,
                    h: 200.0,
                },
                false,
                minimized,
                fullscreen,
                maximized_core,
                cloaked || hidden || shell,
            );
            let verdict = border_eligible(
                true, true, frame, minimized, fullscreen, maximized, cloaked, hidden, shell,
            );
            assert_eq!(verdict.is_ok(), core.visible, "bits={bits:06b}");
        }
    }

    #[test]
    fn ring_expands_gap_then_width() {
        // Visible 100x80 at (10,20), gap 2, width 4:
        // inner (8,18,104,84), outer (4,14,112,92).
        let visible = rect(10, 20, 100, 80);
        assert_eq!(border_inner_rect(visible, 2), rect(8, 18, 104, 84));
        assert_eq!(border_outer_rect(visible, 2, 4), Some(rect(4, 14, 112, 92)));
        // Zero gap/width is the identity.
        assert_eq!(border_outer_rect(visible, 0, 3), Some(rect(7, 17, 106, 86)));
    }

    #[test]
    fn eligibility_suppression_matrix() {
        let frame = rect(10, 20, 320, 200);
        assert!(
            border_eligible(true, true, frame, false, false, false, false, false, false).is_ok()
        );
        assert_eq!(
            border_eligible(false, true, frame, false, false, false, false, false, false),
            Err(BorderHideReason::Disabled)
        );
        assert_eq!(
            border_eligible(true, false, frame, false, false, false, false, false, false),
            Err(BorderHideReason::NoTarget)
        );
        for (min, fs, max, cloak, hidden, shell, want) in [
            (
                true,
                false,
                false,
                false,
                false,
                false,
                BorderHideReason::Minimized,
            ),
            (
                false,
                true,
                false,
                false,
                false,
                false,
                BorderHideReason::Fullscreen,
            ),
            (
                false,
                false,
                true,
                false,
                false,
                false,
                BorderHideReason::Maximized,
            ),
            (
                false,
                false,
                false,
                true,
                false,
                false,
                BorderHideReason::Cloaked,
            ),
            (
                false,
                false,
                false,
                false,
                true,
                false,
                BorderHideReason::HiddenWorkspace,
            ),
            (
                false,
                false,
                false,
                false,
                false,
                true,
                BorderHideReason::Shell,
            ),
        ] {
            assert_eq!(
                border_eligible(true, true, frame, min, fs, max, cloak, hidden, shell),
                Err(want)
            );
        }
    }

    #[test]
    fn accent_winrt_color_splits_argb_channels() {
        // WinRT `Color` arrives as separate `A`/`R`/`G`/`B` bytes.
        let accent = SystemAccent::from_winrt_color(0xC4, 0x2A, 0x82, 0xDA);
        assert_eq!(accent.color, (0x2a, 0x82, 0xda));
        assert_eq!(accent.alpha, 0xC4);
        assert_eq!(render_color(accent.color), "#2a82da");
        let opaque_red = SystemAccent::from_winrt_color(0xFF, 0xFF, 0x00, 0x00);
        assert_eq!(opaque_red.color, (0xff, 0x00, 0x00));
        assert_eq!(opaque_red.alpha, 0xff);
        let transparent = SystemAccent::from_winrt_color(0x00, 0x11, 0x22, 0x33);
        assert_eq!(transparent.color, (0x11, 0x22, 0x33));
        assert_eq!(transparent.alpha, 0);
    }

    #[test]
    fn theme_gate_selects_accent_or_configured_fallback() {
        let themed = ActiveBorderStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: true,
            ..ActiveBorderStyle::default()
        };
        let explicit = ActiveBorderStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: false,
            ..ActiveBorderStyle::default()
        };
        let accent = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        // Requested, present, positive alpha: accent wins (KDE highlight parity).
        assert_eq!(effective_color(&themed, Some(accent)), (0xaa, 0xbb, 0xcc));
        // Zero alpha fails the core gate: fallback wins.
        assert_eq!(
            effective_color(
                &themed,
                Some(SystemAccent {
                    color: (0xaa, 0xbb, 0xcc),
                    alpha: 0
                })
            ),
            (0x11, 0x22, 0x33)
        );
        // Unavailable query: fallback wins.
        assert_eq!(effective_color(&themed, None), (0x11, 0x22, 0x33));
        // `--no-active-border-theme` keeps the explicit configured colour even
        // when a valid accent is present.
        assert_eq!(effective_color(&explicit, Some(accent)), (0x11, 0x22, 0x33));
        assert_eq!(effective_color(&explicit, None), (0x11, 0x22, 0x33));
    }

    #[test]
    fn accent_cache_queries_once_until_version_moves() {
        let live = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        let mut cache = AccentCache::default();
        let mut calls = 0;
        // Empty cache queries even at the initial version.
        assert!(cache.refresh(7, || {
            calls += 1;
            Some(live)
        }));
        assert_eq!(calls, 1);
        assert_eq!(cache.get(), Some(live));
        // Steady state performs no native query.
        assert!(!cache.refresh(7, || {
            calls += 1;
            Some(live)
        }));
        assert_eq!(calls, 1);
        // One broadcast needs exactly one re-query, even across consumers.
        let mut sibling = AccentCache::default();
        assert!(cache.refresh(8, || {
            calls += 1;
            Some(live)
        }));
        assert!(sibling.refresh(8, || {
            calls += 1;
            Some(live)
        }));
        assert_eq!(calls, 3, "each consumer re-queries, none is starved");
        assert!(!cache.refresh(8, || {
            calls += 1;
            None
        }));
        assert_eq!(calls, 3);
    }

    #[test]
    fn accent_cache_failed_query_falls_back_without_sticking() {
        // An unavailable query caches `None` (callers fall back) but keeps
        // the taken version, so the next broadcast retries instead of
        // pinning the fallback forever.
        let mut cache = AccentCache::default();
        assert!(cache.refresh(1, || None));
        assert_eq!(cache.get(), None);
        assert!(
            !cache.refresh(1, || None),
            "no retry without a new broadcast"
        );
        let live = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        assert!(cache.refresh(2, || Some(live)));
        assert_eq!(cache.get(), Some(live));
    }

    #[test]
    fn accent_availability_is_presence_plus_positive_alpha() {
        // Unavailable means no query result or a zero-alpha result: the
        // pager and the border theme gate share this exact rule.
        assert!(!accent_available(None));
        assert!(!accent_available(Some(SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0,
        })));
        assert!(accent_available(Some(SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 1,
        })));
        assert!(accent_available(Some(SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        })));
    }

    #[test]
    fn effective_source_agrees_with_effective_color() {
        // Source reporting mirrors the painted colour exactly: the source
        // claims the accent iff the effective colour is the accent colour
        // (and the accent is available), for every flag combination. A
        // configured colour coincidentally equal to the accent still
        // reports the accent source - the pixels match either way.
        let styles = [
            ActiveBorderStyle {
                color: (0x11, 0x22, 0x33),
                use_theme: true,
                ..ActiveBorderStyle::default()
            },
            ActiveBorderStyle {
                color: (0x11, 0x22, 0x33),
                use_theme: false,
                ..ActiveBorderStyle::default()
            },
            ActiveBorderStyle {
                color: (0xaa, 0xbb, 0xcc),
                use_theme: true,
                ..ActiveBorderStyle::default()
            },
        ];
        let accents = [
            None,
            Some(SystemAccent {
                color: (0xaa, 0xbb, 0xcc),
                alpha: 0xC4,
            }),
            Some(SystemAccent {
                color: (0xaa, 0xbb, 0xcc),
                alpha: 0,
            }),
        ];
        for style in styles {
            for accent in accents {
                let uses = effective_uses_accent(&style, accent);
                let painted = effective_color(&style, accent);
                assert_eq!(uses, accent_available(accent) && style.use_theme);
                if uses {
                    assert_eq!(painted, accent.map(|a| a.color).expect("used accent"));
                } else {
                    assert_eq!(painted, style.color);
                }
            }
        }
    }

    #[test]
    fn accent_version_move_promotes_once_then_silent() {
        // Wake fanout observation: one promotion per movement, steady
        // versions never promote (idle ack-only turns stay untouched).
        let mut seen = 7;
        assert!(!accent_version_moved(&mut seen, 7));
        assert_eq!(seen, 7);
        assert!(accent_version_moved(&mut seen, 8));
        assert_eq!(seen, 8);
        assert!(!accent_version_moved(&mut seen, 8));
        assert!(accent_version_moved(&mut seen, 9));
        assert_eq!(seen, 9);
    }

    fn fake_raw(init_hr: i32, accent: Option<SystemAccent>) -> AccentRaw {
        let ran = accent.is_some();
        AccentRaw {
            init_hr,
            activate_hr: ran.then_some(0),
            query_hr: ran.then_some(0),
            accent,
        }
    }

    #[test]
    fn accent_init_labels_name_the_observed_result() {
        assert_eq!(AccentInit::classify(0), AccentInit::Ok);
        assert_eq!(AccentInit::classify(1), AccentInit::Already);
        assert_eq!(
            AccentInit::classify(0x8001_0106u32 as i32),
            AccentInit::ChangedMode
        );
        assert_eq!(AccentInit::classify(-1), AccentInit::Failed);
        assert_eq!(AccentInit::Ok.label(), "ok");
        assert_eq!(AccentInit::Already.label(), "already");
        assert_eq!(AccentInit::ChangedMode.label(), "changed-mode");
        assert_eq!(AccentInit::Failed.label(), "failed");
    }

    /// Portable HRESULT mirrors must equal the native constants: the labels
    /// above are only names for observed values, so a wrong mirror would
    /// mislabel a real result. The init policy itself stays solely in
    /// `crate::ffi::accent`; this test only pins the vocabulary.
    #[cfg(windows)]
    #[test]
    fn accent_init_constants_match_native() {
        assert_eq!(S_OK_HR, windows_sys::Win32::Foundation::S_OK);
        assert_eq!(S_FALSE_HR, windows_sys::Win32::Foundation::S_FALSE);
        assert_eq!(
            RPC_E_CHANGED_MODE_HR,
            windows_sys::Win32::Foundation::RPC_E_CHANGED_MODE
        );
    }

    #[test]
    fn accent_report_resolves_raw_vs_consumer_outcome() {
        let live = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        let raw = fake_raw(0, Some(live));
        // Usable accent with the theme on: accent paints.
        let themed = accent_query_report(
            AccentQuerySurface::Border,
            AccentQueryRole::Renderer,
            AccentQueryTrigger::Startup,
            7,
            &raw,
            true,
            (0x11, 0x22, 0x33),
        );
        assert_eq!(themed.outcome, AccentQueryOutcome::Accent);
        assert_eq!(themed.fallback_reason, None);
        assert_eq!(themed.resolved, (0xaa, 0xbb, 0xcc));
        assert!(themed.raw_valid());
        // Same raw value with the theme off: fallback paints, honestly named.
        let explicit = accent_query_report(
            AccentQuerySurface::Border,
            AccentQueryRole::SameThread,
            AccentQueryTrigger::Broadcast,
            8,
            &raw,
            false,
            (0x11, 0x22, 0x33),
        );
        assert_eq!(explicit.outcome, AccentQueryOutcome::Fallback);
        assert_eq!(
            explicit.fallback_reason,
            Some(AccentFallbackReason::ThemeOff)
        );
        assert_eq!(explicit.resolved, (0x11, 0x22, 0x33));
        assert!(explicit.raw_valid(), "raw stays valid under theme-off");
        // Zero alpha is the only invalid source: fallback paints.
        let ghost = fake_raw(
            0,
            Some(SystemAccent {
                color: (0xaa, 0xbb, 0xcc),
                alpha: 0,
            }),
        );
        let invalid = accent_query_report(
            AccentQuerySurface::Pager,
            AccentQueryRole::Owner,
            AccentQueryTrigger::Broadcast,
            9,
            &ghost,
            true,
            (0x2a, 0x82, 0xda),
        );
        assert_eq!(invalid.outcome, AccentQueryOutcome::Invalid);
        assert_eq!(
            invalid.fallback_reason,
            Some(AccentFallbackReason::ZeroAlpha)
        );
        assert_eq!(invalid.resolved, (0x2a, 0x82, 0xda));
        assert!(!invalid.raw_valid());
        // Failed init: nothing ran, fallback paints, stages read null.
        let failed = accent_query_report(
            AccentQuerySurface::Border,
            AccentQueryRole::Renderer,
            AccentQueryTrigger::Broadcast,
            10,
            &AccentRaw {
                init_hr: 0x8001_0106u32 as i32,
                activate_hr: None,
                query_hr: None,
                accent: None,
            },
            true,
            (0x11, 0x22, 0x33),
        );
        assert_eq!(failed.init, AccentInit::ChangedMode);
        assert_eq!(failed.outcome, AccentQueryOutcome::Fallback);
        assert_eq!(
            failed.fallback_reason,
            Some(AccentFallbackReason::Unavailable)
        );
        assert_eq!(failed.resolved, (0x11, 0x22, 0x33));
        assert!(!failed.raw_valid());
    }

    #[test]
    fn accent_report_treats_black_as_legitimate() {
        // Opaque black is a successful accent, never invalid: only zero
        // alpha is diagnostically unusable.
        let black = fake_raw(
            1,
            Some(SystemAccent {
                color: (0, 0, 0),
                alpha: 0xFF,
            }),
        );
        let report = accent_query_report(
            AccentQuerySurface::Pager,
            AccentQueryRole::Owner,
            AccentQueryTrigger::Startup,
            1,
            &black,
            true,
            (0x2a, 0x82, 0xda),
        );
        assert_eq!(report.init, AccentInit::Already);
        assert!(report.raw_valid());
        assert_eq!(report.outcome, AccentQueryOutcome::Accent);
        assert_eq!(report.resolved, (0, 0, 0));
    }

    #[test]
    fn accent_query_event_shape_carries_no_identity_or_paths() {
        let report = accent_query_report(
            AccentQuerySurface::Border,
            AccentQueryRole::Renderer,
            AccentQueryTrigger::Broadcast,
            7,
            &fake_raw(
                0,
                Some(SystemAccent {
                    color: (0x2a, 0x82, 0xda),
                    alpha: 0xFF,
                }),
            ),
            true,
            (0x11, 0x22, 0x33),
        );
        let event = accent_query_event(&report);
        assert_eq!(
            event,
            serde_json::json!({
                "event": "accent-query",
                "surface": "border",
                "role": "renderer",
                "trigger": "broadcast",
                "version": 7,
                "init": "ok",
                "init_hr": 0,
                "activate_hr": 0,
                "query_hr": 0,
                "argb": "#ff2a82da",
                "raw_valid": true,
                "outcome": "accent",
                "fallback_reason": null,
                "resolved": "#2a82da",
            })
        );
        // Skipped stages stay null (never a fabricated error), and no value
        // means no argb: failed attempts still record the init HRESULT.
        let failed = accent_query_report(
            AccentQuerySurface::Pager,
            AccentQueryRole::Owner,
            AccentQueryTrigger::Startup,
            0,
            &AccentRaw {
                init_hr: -1,
                activate_hr: None,
                query_hr: None,
                accent: None,
            },
            true,
            (0x2a, 0x82, 0xda),
        );
        let event = accent_query_event(&failed);
        assert_eq!(event["activate_hr"], serde_json::Value::Null);
        assert_eq!(event["query_hr"], serde_json::Value::Null);
        assert_eq!(event["argb"], serde_json::Value::Null);
        assert_eq!(event["init_hr"], serde_json::json!(-1));
        assert_eq!(event["outcome"], serde_json::json!("fallback"));
        assert_eq!(event["fallback_reason"], serde_json::json!("unavailable"));
        // Privacy fence: no thread ids, handles, paths, or messages.
        for line in [
            serde_json::to_string(&event).expect("event serializes"),
            serde_json::to_string(&accent_query_event(&report)).expect("event serializes"),
        ] {
            for forbidden in [
                "thread", "handle", "hwnd", "path", "exe", "pid", "error", "message",
            ] {
                assert!(
                    !line.to_ascii_lowercase().contains(forbidden),
                    "event leaks {forbidden}: {line}"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn accent_query_stage_event_shape_carries_no_identity_or_paths() {
        let event = accent_query_stage_event(
            AccentQuerySurface::Pager,
            AccentQueryRole::Owner,
            AccentQueryTrigger::Startup,
            7,
            crate::ffi::AccentStage::Query,
        );
        assert_eq!(event["event"], "accent-query-stage");
        assert_eq!(event["surface"], "pager");
        assert_eq!(event["role"], "owner");
        assert_eq!(event["trigger"], "startup");
        assert_eq!(event["version"], 7);
        assert_eq!(event["stage"], "query");
        let line = serde_json::to_string(&event).expect("event serializes");
        for forbidden in [
            "thread", "handle", "hwnd", "path", "exe", "pid", "error", "message", "hr",
        ] {
            assert!(
                !line.to_ascii_lowercase().contains(forbidden),
                "stage event leaks {forbidden}: {line}"
            );
        }
    }

    #[test]
    fn accent_cache_detailed_queries_once_per_version_move() {
        let live = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        let mut cache = AccentCache::default();
        assert_eq!(cache.last_version(), None);
        let mut calls = 0;
        // First query runs (startup trigger) and records its version.
        let raw = cache
            .refresh_detailed(7, || {
                calls += 1;
                fake_raw(0, Some(live))
            })
            .expect("startup queries");
        assert_eq!(calls, 1);
        assert_eq!(raw.accent, Some(live));
        assert_eq!(cache.last_version(), Some(7));
        // Steady state performs no native query and yields no diagnostic.
        assert_eq!(
            cache.refresh_detailed(7, || {
                calls += 1;
                fake_raw(0, Some(live))
            }),
            None
        );
        assert_eq!(calls, 1, "repeat silence performs no query");
        // A version move re-queries even when the colour is identical: the
        // caller still emits one diagnostic for the actual query.
        let again = cache
            .refresh_detailed(8, || {
                calls += 1;
                fake_raw(1, Some(live))
            })
            .expect("broadcast re-queries");
        assert_eq!(calls, 2);
        assert_eq!(again.init_hr, 1, "S_FALSE observed, not collapsed");
        assert_eq!(cache.get(), Some(live));
        assert_eq!(cache.last_version(), Some(8));
    }

    #[test]
    fn ring_raster_is_hollow() {
        // 12x12 outer, 3px ring, square corners (inner radius 0).
        assert!(ring_covers(0, 0, 12, 12, 3, 0));
        assert!(ring_covers(11, 11, 12, 12, 3, 0));
        assert!(!ring_covers(6, 6, 12, 12, 3, 0));
        assert!(!ring_covers(12, 6, 12, 12, 3, 0));
        // KWin inner-radius parity: radius param is the INNER radius.
        // Outer corner extent is radius + width, so with inner radius 3 and
        // width 3 the outer corner (extent 6) rounds the extreme corner
        // away while the top-edge centre still paints.
        assert!(!ring_covers(0, 0, 12, 12, 3, 3));
        assert!(ring_covers(6, 0, 12, 12, 3, 3));
        // Symmetric case from the old outer-radius reading would have
        // painted (0,0) here; inner-radius must not.
        assert!(!ring_covers(0, 0, 14, 14, 2, 5));
        assert!(ring_covers(7, 0, 14, 14, 2, 5));
        // Zero width never paints (width 0 hides the border).
        assert!(!ring_covers(0, 0, 12, 12, 0, 0));
    }
}
