#![forbid(unsafe_code)]

//! Production active-border controller (portable: no AppKit, no AX reads).
//!
//! Data flow (no new threads, no polling loops, no animation):
//! - Each per-app AX owner publishes ONLY its own PID's focused-window facts
//!   to the shared [`BorderFactsStore`] from snapshot data it ALREADY read
//!   ([`crate::native_tile_backend`], alongside the eligibility publish).
//!   Records carry a freshness timestamp on the shared eligibility clock; a
//!   hung publisher goes stale and hides instead of wedging the ring.
//! - The tile main loop ([`crate::tile`]) joins the frontmost PID's record
//!   with cheap main-thread-safe reads it already takes per cadence (session
//!   lock, workspace frontmost, current user Spaces, topology revision, live
//!   system accent) plus the product scope gate, and feeds
//!   [`BorderController::tick`]. Frames and AX/CG/app facts arrive
//!   publisher-side; the main thread performs no AX reads on this path.
//! - The controller emits at most one carrier action per tick and only on
//!   change (show/update/hide, else silent): identical ticks make no carrier
//!   call, so idle runs never spam frames or redraws. Every false/unknown
//!   leg hides; the caller destroys the carrier at run stop.
//!
//! Scoped-dev vs policy semantics: scoped runs decorate ONLY the focused
//! window of an in-scope frontmost app — the caller gates the frontmost
//! bundle through the allowlist, so an unrelated window never decorates even
//! when it somehow becomes frontmost. Unscoped runs decorate ordinary apps
//! through the same product gate (own bundles and exclusions refused).
//! The classifier's own-bundle exclusion additionally means directly
//! spawned helpers (no LaunchServices bundle entry) and the probe itself
//! never map to a target; the signed bundled helper stays decorable under
//! an explicit helper-bundle scope (fixture parity).
//!
//! macOS has no public zoom-state bit, so `maximized_h/v` for
//! NON-ENGINE-PLACED windows derives from work-area equality
//! ([`zoom_axes`]): a float whose frame matches a current visible work
//! area on an axis reads zoomed on that axis. Engine-placed windows (the
//! core's latest plan geometry names exactly that window — the tick-side
//! join in [`crate::tile`], which survives `ToggleFloat` because a floated
//! window drops out of plan truth) never read zoomed — their size is
//! tiling truth, and a green-button zoom there reflows on the next
//! observation. Classifier eligibility (`Tile` disposition) is NOT
//! placement truth and never feeds this path: every ordinary window
//! classifies `Tile`, so eligibility would disable zoom suppression for
//! all of them. A borderless full-display frame additionally reads
//! zoomed on both axes ([`covers_full_display`], safety wins over any
//! placement provenance: tiling truth only ever fills work areas).
//! Tiling gaps stay independent (the ring uses the border gap
//! only). A future native zoom read replaces the heuristic at the supplier
//! without touching the mapper.

use std::collections::BTreeMap;
use std::sync::Mutex;

use tiler_core::visual::VisualRect;

use crate::active_border::{
    ActiveBorderStyle, BorderObservation, SystemAccent, border_decision, border_outer_rect,
    effective_color, map_observation_to_target, render_fence,
};
use crate::classifier::{AppFacts, AxFacts, CgFacts};
use crate::ffi::AxFrame;
use crate::input_eligibility::{FACTS_TTL, is_fresh_at, now_ms};

/// Focused-window facts for one PID, published by its owning AX thread from
/// already-read snapshot data. The frame is AX/CG top-left points (never
/// pixels); the AppKit flip happens tick-side from the live snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderWindowFacts {
    /// Focused window ID.
    pub wid: u32,
    /// Joined AX frame in top-left points.
    pub frame: AxFrame,
    /// Typed AX facts (role/subrole/fullscreen/minimized/modal).
    pub ax: AxFacts,
    /// Typed CG facts (layer/onscreen join).
    pub cg: CgFacts,
    /// Typed owner-app facts (hidden/policy/own-bundle).
    pub app: AppFacts,
    /// Live Space membership (empty = unknown, hides tick-side).
    pub spaces: Vec<u64>,
}

/// One per-PID publisher record: the owner's bound PID plus its focused
/// window candidate. Plain data only (IDs plus status bits — never titles,
/// content, or paths). Keyed by PID in the store, so one app's publisher
/// cannot overwrite another PID's record; each owner publishes ONLY its own
/// bound PID.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderAppRecord {
    /// The publishing owner's bound PID (must equal the store key).
    pub pid: u32,
    /// Live bundle observed by the owner (`None` when unreadable).
    pub bundle: Option<String>,
    /// App-local focused window (`None` when none).
    pub focused_wid: Option<u32>,
    /// Owner-thread selected-oracle focus read alongside the snapshot.
    /// The tick requires it to name exactly `(pid, focused_wid)`.
    pub selected: Option<(u32, Option<u32>)>,
    /// Focused-window facts (`None` = no focused window published; the
    /// record still proves the owner is alive — unknown hides).
    pub window: Option<BorderWindowFacts>,
    /// Topology revision read with the snapshot.
    pub revision: u64,
    /// Milliseconds since the eligibility origin when published (freshness
    /// against [`FACTS_TTL`]).
    pub updated_ms: u64,
}

/// Shared per-PID border facts: plain records plus invalidation. `Mutex`
/// only — the tile main loop locks it briefly per cadence; publishers lock
/// it briefly per snapshot. No callbacks, no AX, no AppKit touch here.
/// The `dirty` flag is the publisher→main wakeup: set on every publish or
/// invalidation, cleared by the main loop's border-only fast path (which
/// throttles re-joins itself, so AX-frequency publishes never trigger
/// AX-frequency joins, topology, or tiling).
#[derive(Debug, Default)]
pub struct BorderFactsStore {
    inner: Mutex<BTreeMap<u32, BorderAppRecord>>,
    dirty: std::sync::atomic::AtomicBool,
}

impl BorderFactsStore {
    /// Empty store (initially hides: no records).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current monotonic milliseconds on the shared eligibility clock.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        now_ms()
    }

    /// Publish one PID's candidate record. Each per-app AX owner passes ONLY
    /// its own bound PID; the write touches exactly that PID's key, so a
    /// background publisher can never overwrite the foreground PID's
    /// record. Records with `pid` 0/1 are refused (never stored). Every
    /// accepted publish marks the store dirty for the main-loop border-only
    /// fast path.
    pub fn publish_app(&self, record: BorderAppRecord) {
        if record.pid <= 1 {
            return;
        }
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.insert(record.pid, record);
        self.dirty.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Drop one PID's record (worker terminate/vanish/lock-stop path):
    /// a terminated PID never decorates. Marks the store dirty so the
    /// fast path re-hides without waiting for the cadence.
    pub fn invalidate_pid(&self, pid: u32) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.remove(&pid);
        self.dirty.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Take the publisher wakeup flag (main-loop border fast path only):
    /// true when facts changed since the last take. Clearing is the
    /// fast path's own throttle business — publishers never block on it.
    #[must_use]
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, std::sync::atomic::Ordering::AcqRel)
    }

    /// Peek the publisher wakeup flag without clearing it (fast-path gate
    /// only; the take happens on a joining tick so throttled bursts keep
    /// their wakeup).
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Cloned plain record for one PID (`None` when never published or
    /// invalidated). Tile-main consult only.
    #[must_use]
    pub fn snapshot_for(&self, pid: u32) -> Option<BorderAppRecord> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&pid)
            .cloned()
    }
}

/// Primary display height in points from converted snapshot geometry: the
/// display whose CG top-left bounds sit at the origin. `None` when missing
/// or degenerate (frame conversion refuses instead of guessing).
#[must_use]
pub fn snapshot_primary_height(displays: &[crate::topology::DisplayGeometry]) -> Option<f64> {
    displays.iter().find_map(|d| {
        let b = d.cg_bounds?;
        (b.x == 0.0 && b.y == 0.0 && b.h.is_finite() && b.h > 0.0).then_some(b.h)
    })
}

/// Zoom axes for one frame in CG top-left points against the current visible
/// work areas. Engine-placed windows never read zoomed: their size is
/// tiling truth (see the [`BorderTickInput::engine_placed`] join). Floats
/// match per axis within 1pt (independent axes, any display: a window
/// zoomed on one display still reads zoomed there). The 1pt tolerance
/// covers AX/CG point rounding across APIs — never a claim about window
/// state. No public zoom bit exists on macOS, so this stays an explicit,
/// documented heuristic — never a claim about window state.
#[must_use]
pub fn zoom_axes(frame: VisualRect, visibles: &[VisualRect], engine_placed: bool) -> (bool, bool) {
    if engine_placed {
        return (false, false);
    }
    let close = |a: f64, b: f64| (a - b).abs() <= 1.0;
    let h = visibles
        .iter()
        .any(|v| close(frame.x, v.x) && close(frame.w, v.w));
    let v = visibles
        .iter()
        .any(|v| close(frame.y, v.y) && close(frame.h, v.h));
    (h, v)
}

/// Borderless full-display fence: true when the frame covers an entire
/// display's full bounds (not just its work area) on both axes within 1pt
/// (same AX/CG rounding justification as [`zoom_axes`]). Both axes must
/// match on the SAME display; a work-area-sized frame on a menu-bar/Dock
/// display never matches (its origin/extent differ from the full bounds).
/// The caller ORs this into both zoom axes regardless of engine placement
/// (safety wins: tiling truth only ever fills work areas, so a
/// full-display frame is never legitimate tiling — and a borderless game
/// must never gain a ring from tiled provenance).
#[must_use]
pub fn covers_full_display(frame: VisualRect, displays: &[VisualRect]) -> bool {
    let close = |a: f64, b: f64| (a - b).abs() <= 1.0;
    displays.iter().any(|d| {
        close(frame.x, d.x) && close(frame.y, d.y) && close(frame.w, d.w) && close(frame.h, d.h)
    })
}

/// Plain tick input: the tile main loop fills the OS legs (no AX reads),
/// the focused record arrives publisher-side. All unknowns hide.
#[derive(Debug, Clone)]
pub struct BorderTickInput {
    /// Master switch (startup config).
    pub enabled: bool,
    /// Configured ring style.
    pub style: ActiveBorderStyle,
    /// Live system accent (`None` = configured fallback wins).
    pub accent: Option<SystemAccent>,
    /// Session unlocked verdict (false hides).
    pub session_unlocked: bool,
    /// Workspace frontmost PID (`None` = unreadable, hides).
    pub frontmost_pid: Option<u32>,
    /// Workspace frontmost bundle (product scope input).
    pub frontmost_bundle: Option<String>,
    /// Product scope gate on the frontmost bundle (scoped: allowlist;
    /// unscoped: ordinary-app rule). False never decorates.
    pub scope_allowed: bool,
    /// Publisher record for the frontmost PID (`None` = unknown, hides).
    pub record: Option<BorderAppRecord>,
    /// Current user Space IDs across displays (empty denies).
    pub context_currents: Vec<u64>,
    /// Live topology revision (record drift hides).
    pub context_revision: u64,
    /// Current visible work areas in CG top-left points (zoom heuristic).
    pub visibles: Vec<VisualRect>,
    /// Current full display bounds in CG top-left points (borderless
    /// full-display game fence — never work areas).
    pub displays: Vec<VisualRect>,
    /// Engine-placement truth for the focused window: true only when the
    /// core's latest plan geometry names exactly this window (tick-side
    /// join over [`crate::runtime::RuntimeHandle::last_desired_geometry`]).
    /// Classifier eligibility is NOT consulted here: `ToggleFloat`-floated
    /// windows keep their `Tile` disposition but drop out of plan truth,
    /// so a floated-then-maximized window reads zoomed and hides. Unknown
    /// (no plan yet) reads false — the safe direction (a ring may hide,
    /// never wrongly show on a maximized float).
    pub engine_placed: bool,
    /// AppKit flip height in points (`None` = conversion refuses, hides).
    pub primary_h: Option<f64>,
    /// Millisecond clock for record freshness.
    pub now_ms: u64,
}

/// Resolved show parameters: everything one carrier call needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderShowParams {
    /// Target window ID for relative ordering.
    pub target_wid: u32,
    /// Carrier frame in AppKit points (global coordinates).
    pub outer: VisualRect,
    /// Ring style at show time.
    pub style: ActiveBorderStyle,
    /// Resolved ring colour (theme or configured fallback).
    pub color: (u8, u8, u8),
}

/// Pure per-tick decision: either show parameters or a static hide token.
/// Tokens reuse the shared hide-reason strings where the mapper decided,
/// with `locked`, `no-frontmost`, `out-of-scope`, `facts-unknown`,
/// `facts-stale`, `selected-disagreement`, `revision-stale`,
/// `not-on-current-space`, and `bad-geometry` for the join legs before it.
pub fn decide_border(input: &BorderTickInput) -> Result<BorderShowParams, &'static str> {
    if !input.enabled {
        return Err("disabled");
    }
    if !input.session_unlocked {
        return Err("locked");
    }
    let frontmost = input
        .frontmost_pid
        .filter(|pid| *pid > 1)
        .ok_or("no-frontmost")?;
    if !input.scope_allowed {
        return Err("out-of-scope");
    }
    let record = input.record.as_ref().ok_or("facts-unknown")?;
    if record.pid != frontmost {
        return Err("facts-stale");
    }
    if record.bundle.is_none() || record.bundle != input.frontmost_bundle {
        return Err("facts-stale");
    }
    if record.revision != input.context_revision {
        return Err("revision-stale");
    }
    if !is_fresh_at(
        record.updated_ms,
        input.now_ms,
        FACTS_TTL.as_millis() as u64,
    ) {
        return Err("facts-stale");
    }
    let wid = record.focused_wid.filter(|w| *w != 0).ok_or("no-focus")?;
    if record.selected != Some((frontmost, Some(wid))) {
        return Err("selected-disagreement");
    }
    let window = record.window.as_ref().ok_or("facts-unknown")?;
    if window.wid != wid {
        return Err("facts-stale");
    }
    // Current-Space leg of the render fence (fresh membership, exact
    // single-home on a live current user Space). Kept as a bool: the
    // final `render_fence` gate below joins it with target visibility
    // (shared core verdict) and the unlocked session as the single show
    // authority, so no leg can pass alone.
    let on_active_space =
        window.spaces.len() == 1 && input.context_currents.contains(&window.spaces[0]);
    let primary_h = input
        .primary_h
        .filter(|h| h.is_finite() && *h > 0.0)
        .ok_or("bad-geometry")?;
    for v in [
        window.frame.x,
        window.frame.y,
        window.frame.w,
        window.frame.h,
    ] {
        if !v.is_finite() {
            return Err("bad-geometry");
        }
    }
    if window.frame.w <= 0.0 || window.frame.h <= 0.0 {
        return Err("bad-geometry");
    }
    // CG top-left points to AppKit bottom-left points (global flip around
    // the primary height; x/width pass through untouched).
    let frame = VisualRect {
        x: window.frame.x,
        y: primary_h - (window.frame.y + window.frame.h),
        w: window.frame.w,
        h: window.frame.h,
    };
    let (zoom_h, zoom_v) = zoom_axes(
        VisualRect {
            x: window.frame.x,
            y: window.frame.y,
            w: window.frame.w,
            h: window.frame.h,
        },
        &input.visibles,
        input.engine_placed,
    );
    // Borderless full-display fence: a frame covering a whole display reads
    // zoomed on both axes even when engine-placed (safety wins — tiling
    // truth only ever fills work areas; tiled provenance never excuses a
    // full-display frame, and borderless games stay ringless). The closed
    // hide vocabulary keeps reporting this as `maximized`.
    let full_display = covers_full_display(
        VisualRect {
            x: window.frame.x,
            y: window.frame.y,
            w: window.frame.w,
            h: window.frame.h,
        },
        &input.displays,
    );
    let (maximized_h, maximized_v) = (zoom_h || full_display, zoom_v || full_display);
    let observation = BorderObservation {
        frame: Some(frame),
        ax: window.ax.clone(),
        cg: window.cg,
        app: window.app,
        maximized_h,
        maximized_v,
        focused: true,
        deleted: false,
    };
    let target = map_observation_to_target(&observation).ok_or("no-target")?;
    let inner =
        border_decision(true, &target, input.style.gap).map_err(|reason| reason.as_str())?;
    // Single show authority: the shared core verdict (target visibility),
    // current-Space membership, and the unlocked session must ALL hold.
    // The session leg repeats the early `locked` refusal (constant-true
    // here) so the fence stays complete if the fast path ever moves; only
    // the Space leg can fail at this point.
    if !render_fence(true, on_active_space, input.session_unlocked) {
        return Err("not-on-current-space");
    }
    let outer = border_outer_rect(inner, input.style.width).ok_or("bad-geometry")?;
    Ok(BorderShowParams {
        target_wid: wid,
        outer,
        style: input.style,
        color: effective_color(&input.style, input.accent),
    })
}

/// One carrier effect per tick: at most one panel call, only on change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderEffect {
    /// Steady state (identical target/geometry/style, or staying hidden):
    /// no carrier call.
    None,
    /// Ensure a live carrier showing these parameters: create when absent,
    /// else move/restyle. `reorder_above` is true only when the target wid
    /// is new (first show or focus switch): the caller orders above it
    /// after create/update; same-target moves keep their relative order.
    Show {
        target_wid: u32,
        reorder_above: bool,
        outer: VisualRect,
        style: ActiveBorderStyle,
        color: (u8, u8, u8),
    },
    /// Hide the live carrier (emitted once; later ticks stay silent until
    /// the next show).
    Hide,
}

/// Change-only border controller: owns the last pushed carrier state plus
/// outcome counters. The carrier handle itself stays caller-side (main
/// thread); this object only decides which single call the tick needs.
#[derive(Debug, Default)]
pub struct BorderController {
    shown: Option<BorderShowParams>,
    shows: u64,
    updates: u64,
    hides: u64,
    carrier_errors: u64,
    /// Bounded observed-frame→panel-application timing: maximum
    /// `now_ms - record.updated_ms` (shared monotonic clock) sampled on
    /// pushed shows/updates only — never per-frame, never logged per tick.
    /// The live move/resize leg reads this as the production tracking lag
    /// (publisher AX settle included; display/pixel latency excluded).
    apply_lag_max_ms: u64,
    /// Pushed show/update count backing the maximum above.
    apply_lag_samples: u64,
    /// Most recent panel-application lag (this show/update); the carrier
    /// driver logs it on the transition line (transitions only, no spam).
    last_apply_lag_ms: u64,
    /// Most recent hide-decision token (`None` after a show decision):
    /// the carrier driver logs it on the hide transition so live
    /// suppression stays diagnosable.
    last_hide_reason: Option<&'static str>,
}

impl BorderController {
    /// Empty controller (carrier hidden).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One tick: resolve the decision, then diff against the last pushed
    /// state. Shows/updates/hides count pushed effects only (silent ticks
    /// change nothing). Pushed shows/updates sample the
    /// observed→application lag; denials record their token for the hide
    /// transition log.
    pub fn tick(&mut self, input: &BorderTickInput) -> BorderEffect {
        let params = match decide_border(input) {
            Ok(params) => params,
            Err(reason) => {
                self.last_hide_reason = Some(reason);
                if self.shown.is_some() {
                    self.shown = None;
                    self.hides += 1;
                    return BorderEffect::Hide;
                }
                return BorderEffect::None;
            }
        };
        self.last_hide_reason = None;
        // Observed-frame→tick lag on the shared clock (publisher stamped
        // `updated_ms` when the frame was read; this tick applies it).
        // Saturates at 0 when clocks skew backwards; staleness itself is a
        // separate denial leg above. Sampled on pushed shows/updates only
        // (silent steady-state ticks never resample).
        let lag_ms = input
            .record
            .as_ref()
            .map(|record| input.now_ms.saturating_sub(record.updated_ms))
            .unwrap_or(0);
        let sample = |controller: &mut Self| {
            controller.last_apply_lag_ms = lag_ms;
            controller.apply_lag_max_ms = controller.apply_lag_max_ms.max(lag_ms);
            controller.apply_lag_samples += 1;
        };
        match self.shown {
            Some(shown) if shown == params => BorderEffect::None,
            Some(shown) => {
                let reorder_above = shown.target_wid != params.target_wid;
                self.shown = Some(params);
                self.updates += 1;
                sample(self);
                BorderEffect::Show {
                    target_wid: params.target_wid,
                    reorder_above,
                    outer: params.outer,
                    style: params.style,
                    color: params.color,
                }
            }
            None => {
                self.shown = Some(params);
                self.shows += 1;
                sample(self);
                BorderEffect::Show {
                    target_wid: params.target_wid,
                    reorder_above: true,
                    outer: params.outer,
                    style: params.style,
                    color: params.color,
                }
            }
        }
    }

    /// The carrier call failed (create/update refused): forget the pushed
    /// state so the next tick retries, and count the error. Never hides
    /// twice for one failure.
    pub fn note_carrier_error(&mut self) {
        self.shown = None;
        self.carrier_errors += 1;
    }

    /// Cleanup path: hide once when a carrier is logically shown (the caller
    /// still destroys the handle), else silent.
    pub fn reset(&mut self) -> BorderEffect {
        if self.shown.is_some() {
            self.shown = None;
            self.hides += 1;
            BorderEffect::Hide
        } else {
            BorderEffect::None
        }
    }

    /// Pushed-show count (creates).
    #[must_use]
    pub fn shows(&self) -> u64 {
        self.shows
    }

    /// Pushed move/restyle count.
    #[must_use]
    pub fn updates(&self) -> u64 {
        self.updates
    }

    /// Pushed-hide count.
    #[must_use]
    pub fn hides(&self) -> u64 {
        self.hides
    }

    /// Carrier-call failure count.
    #[must_use]
    pub fn carrier_errors(&self) -> u64 {
        self.carrier_errors
    }

    /// Maximum observed-frame→panel-application lag in milliseconds,
    /// sampled on pushed shows/updates only.
    #[must_use]
    pub fn apply_lag_max_ms(&self) -> u64 {
        self.apply_lag_max_ms
    }

    /// Pushed show/update count backing [`Self::apply_lag_max_ms`].
    #[must_use]
    pub fn apply_lag_samples(&self) -> u64 {
        self.apply_lag_samples
    }

    /// Most recent panel-application lag in milliseconds.
    #[must_use]
    pub fn last_apply_lag_ms(&self) -> u64 {
        self.last_apply_lag_ms
    }

    /// Most recent hide-decision token (`None` after a show decision).
    #[must_use]
    pub fn hide_reason(&self) -> Option<&'static str> {
        self.last_hide_reason
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classifier::{AppFacts, AxFacts, CgFacts};

    fn ax_frame(x: f64, y: f64, w: f64, h: f64) -> AxFrame {
        AxFrame { x, y, w, h }
    }

    fn standard_facts() -> (AxFacts, CgFacts, AppFacts) {
        (AxFacts::standard(), CgFacts::normal(), AppFacts::regular())
    }

    fn window(wid: u32, frame: AxFrame) -> BorderWindowFacts {
        let (ax, cg, app) = standard_facts();
        BorderWindowFacts {
            wid,
            frame,
            ax,
            cg,
            app,
            spaces: vec![11],
        }
    }

    fn record(pid: u32, wid: u32, now_ms: u64) -> BorderAppRecord {
        BorderAppRecord {
            pid,
            bundle: Some("com.apple.TextEdit".to_owned()),
            focused_wid: Some(wid),
            selected: Some((pid, Some(wid))),
            window: Some(window(wid, ax_frame(100.0, 100.0, 400.0, 300.0))),
            revision: 9,
            updated_ms: now_ms,
        }
    }

    fn input(now_ms: u64) -> BorderTickInput {
        BorderTickInput {
            enabled: true,
            style: ActiveBorderStyle::default(),
            accent: None,
            session_unlocked: true,
            frontmost_pid: Some(4242),
            frontmost_bundle: Some("com.apple.TextEdit".to_owned()),
            scope_allowed: true,
            record: Some(record(4242, 7, now_ms)),
            context_currents: vec![11],
            context_revision: 9,
            visibles: vec![VisualRect {
                x: 0.0,
                y: 25.0,
                w: 1440.0,
                h: 875.0,
            }],
            displays: vec![VisualRect {
                x: 0.0,
                y: 0.0,
                w: 1440.0,
                h: 900.0,
            }],
            engine_placed: true,
            primary_h: Some(900.0),
            now_ms,
        }
    }

    #[test]
    fn store_keeps_per_pid_records_independent() {
        let store = BorderFactsStore::new();
        assert!(store.snapshot_for(4242).is_none());
        store.publish_app(record(4242, 7, 1_000));
        store.publish_app(record(9999, 9, 1_000));
        let before = store.snapshot_for(4242).expect("record");
        // A background publisher cannot overwrite the foreground record.
        let mut other = record(9999, 9, 2_000);
        other.window = None;
        store.publish_app(other);
        assert_eq!(store.snapshot_for(4242).expect("record"), before);
        assert!(store.snapshot_for(9999).expect("record").window.is_none());
        // Reserved PIDs are never stored.
        let mut reserved = record(4242, 7, 2_000);
        reserved.pid = 0;
        store.publish_app(reserved);
        assert!(store.snapshot_for(0).is_none());
        // Termination invalidates exactly that PID.
        store.invalidate_pid(4242);
        assert!(store.snapshot_for(4242).is_none());
        assert!(store.snapshot_for(9999).is_some());
    }

    #[test]
    fn store_dirty_flag_wakes_only_on_change() {
        let store = BorderFactsStore::new();
        // Fresh store: no wakeup (idle fast path costs one load).
        assert!(!store.is_dirty());
        assert!(!store.take_dirty());
        // Accepted publish marks dirty; peek keeps it, take clears it.
        store.publish_app(record(4242, 7, 1_000));
        assert!(store.is_dirty());
        assert!(store.is_dirty());
        assert!(store.take_dirty());
        assert!(!store.is_dirty());
        assert!(!store.take_dirty());
        // Refused records (reserved PIDs) never wake.
        let mut reserved = record(4242, 7, 2_000);
        reserved.pid = 1;
        store.publish_app(reserved);
        assert!(!store.is_dirty());
        // Invalidation wakes (fast re-hide without the cadence).
        store.invalidate_pid(4242);
        assert!(store.take_dirty());
    }

    #[test]
    fn decide_shows_live_target_with_flipped_frame() {
        // AX top-left (100,100,400x300) on a 900pt primary flips to
        // AppKit (100,500,400x300); gap 0 keeps the inner rect, width 3
        // expands the outer rect on every side.
        let outcome = decide_border(&input(10_000)).expect("shows");
        assert_eq!(outcome.target_wid, 7);
        assert_eq!(
            outcome.outer,
            VisualRect {
                x: 97.0,
                y: 497.0,
                w: 406.0,
                h: 306.0,
            }
        );
        assert_eq!(outcome.color, crate::active_border::DEFAULT_COLOR_RGB);
    }

    #[test]
    fn decide_denial_matrix() {
        // Every false/unknown leg hides with a stable token (never a ring).
        type Denial = (&'static str, Box<dyn Fn(&mut BorderTickInput)>);
        let good = input(10_000);
        assert!(decide_border(&good).is_ok());
        let cases: Vec<Denial> = vec![
            ("disabled", Box::new(|i| i.enabled = false)),
            ("locked", Box::new(|i| i.session_unlocked = false)),
            ("no-frontmost", Box::new(|i| i.frontmost_pid = None)),
            ("no-frontmost", Box::new(|i| i.frontmost_pid = Some(1))),
            ("out-of-scope", Box::new(|i| i.scope_allowed = false)),
            ("facts-unknown", Box::new(|i| i.record = None)),
            (
                "facts-stale",
                Box::new(|i| i.record.as_mut().unwrap().pid = 9999),
            ),
            (
                "facts-stale",
                Box::new(|i| i.record.as_mut().unwrap().bundle = None),
            ),
            (
                "facts-stale",
                Box::new(|i| {
                    i.record.as_mut().unwrap().bundle = Some("com.other.app".to_owned());
                }),
            ),
            (
                "revision-stale",
                Box::new(|i| i.record.as_mut().unwrap().revision = 10),
            ),
            (
                "facts-stale",
                Box::new(|i| i.record.as_mut().unwrap().updated_ms = 0),
            ),
            (
                "no-focus",
                Box::new(|i| i.record.as_mut().unwrap().focused_wid = None),
            ),
            (
                "selected-disagreement",
                Box::new(|i| i.record.as_mut().unwrap().selected = Some((4242, Some(9)))),
            ),
            (
                "facts-unknown",
                Box::new(|i| i.record.as_mut().unwrap().window = None),
            ),
            (
                "not-on-current-space",
                Box::new(|i| i.record.as_mut().unwrap().window.as_mut().unwrap().spaces = vec![12]),
            ),
            (
                "not-on-current-space",
                Box::new(|i| i.context_currents = vec![]),
            ),
            ("bad-geometry", Box::new(|i| i.primary_h = None)),
            (
                "excluded",
                Box::new(|i| {
                    i.record
                        .as_mut()
                        .unwrap()
                        .window
                        .as_mut()
                        .unwrap()
                        .app
                        .is_own_bundle = true;
                }),
            ),
            (
                "minimized",
                Box::new(|i| {
                    i.record
                        .as_mut()
                        .unwrap()
                        .window
                        .as_mut()
                        .unwrap()
                        .ax
                        .minimized = true;
                }),
            ),
            (
                "fullscreen",
                Box::new(|i| {
                    i.record
                        .as_mut()
                        .unwrap()
                        .window
                        .as_mut()
                        .unwrap()
                        .ax
                        .fullscreen = true;
                }),
            ),
        ];
        for (want, mutate) in cases {
            let mut probe = good.clone();
            mutate(&mut probe);
            assert_eq!(decide_border(&probe).unwrap_err(), want, "token");
        }
    }

    #[test]
    fn decide_suppresses_zoomed_floats_but_not_placed_full_area() {
        // A ToggleFloat-floated window (engine truth: not placed) exactly
        // filling the work area reads zoomed and hides — classifier
        // eligibility never excuses it (every ordinary window classifies
        // `Tile`, so eligibility would disable this leg for all of them).
        let mut probe = input(10_000);
        {
            probe.engine_placed = false;
            probe
                .record
                .as_mut()
                .unwrap()
                .window
                .as_mut()
                .unwrap()
                .frame = ax_frame(0.0, 25.0, 1440.0, 875.0);
        }
        assert_eq!(decide_border(&probe).unwrap_err(), "maximized");
        // The same frame on an engine-placed tile never reads zoomed: its
        // size is tiling truth (zero-gap layouts keep their ring).
        let mut probe = input(10_000);
        {
            probe.engine_placed = true;
            probe
                .record
                .as_mut()
                .unwrap()
                .window
                .as_mut()
                .unwrap()
                .frame = ax_frame(0.0, 25.0, 1440.0, 875.0);
        }
        assert!(decide_border(&probe).is_ok());
        // Unknown placement (no plan yet) takes the safe direction: the
        // heuristic applies and a maximized float hides.
        let mut probe = input(10_000);
        {
            probe.engine_placed = false;
            probe
                .record
                .as_mut()
                .unwrap()
                .window
                .as_mut()
                .unwrap()
                .frame = ax_frame(0.0, 25.0, 1440.0, 875.0);
        }
        assert_eq!(decide_border(&probe).unwrap_err(), "maximized");
        // Partial match on one axis only: zoomed on exactly that axis.
        assert_eq!(
            zoom_axes(rect(0.0, 100.0, 1440.0, 300.0), &visibles(), false),
            (true, false)
        );
        assert_eq!(
            zoom_axes(rect(100.0, 25.0, 400.0, 875.0), &visibles(), false),
            (false, true)
        );
        assert_eq!(
            zoom_axes(rect(0.0, 25.0, 1440.0, 875.0), &visibles(), false),
            (true, true)
        );
        assert_eq!(
            zoom_axes(rect(0.0, 25.0, 1440.0, 875.0), &visibles(), true),
            (false, false)
        );
    }

    #[test]
    fn full_display_fence_suppresses_borderless_coverage() {
        // Full bounds (not work area) on one display.
        let full = rect(0.0, 0.0, 1440.0, 900.0);
        assert!(covers_full_display(full, &full_displays()));
        // Work-area-sized frames never match (origin/extent differ).
        assert!(!covers_full_display(
            rect(0.0, 25.0, 1440.0, 875.0),
            &full_displays()
        ));
        // One axis only never matches.
        assert!(!covers_full_display(
            rect(0.0, 0.0, 1440.0, 875.0),
            &full_displays()
        ));
        assert!(!covers_full_display(
            rect(0.0, 0.0, 800.0, 900.0),
            &full_displays()
        ));
        // 1pt AX/CG rounding tolerated; beyond it refuses.
        assert!(covers_full_display(
            rect(0.5, -0.5, 1439.5, 900.5),
            &full_displays()
        ));
        assert!(!covers_full_display(
            rect(0.0, 0.0, 1440.0, 897.9),
            &full_displays()
        ));
        // Second display homes its own match.
        assert!(covers_full_display(
            rect(1440.0, 100.0, 1920.0, 1080.0),
            &full_displays()
        ));
        assert!(!covers_full_display(
            rect(0.0, 0.0, 1920.0, 1080.0),
            &full_displays()
        ));
    }

    #[test]
    fn decide_suppresses_full_display_regardless_of_placement() {
        // A borderless float covering the whole display hides (game
        // isolation) with the closed `maximized` token.
        let mut probe = input(10_000);
        {
            probe.engine_placed = false;
            probe
                .record
                .as_mut()
                .unwrap()
                .window
                .as_mut()
                .unwrap()
                .frame = ax_frame(0.0, 0.0, 1440.0, 900.0);
        }
        assert_eq!(decide_border(&probe).unwrap_err(), "maximized");
        // Safety wins: even engine-placed provenance never excuses a
        // full-display frame (tiling truth only ever fills work areas).
        let mut probe = input(10_000);
        {
            probe.engine_placed = true;
            probe
                .record
                .as_mut()
                .unwrap()
                .window
                .as_mut()
                .unwrap()
                .frame = ax_frame(0.0, 0.0, 1440.0, 900.0);
        }
        assert_eq!(decide_border(&probe).unwrap_err(), "maximized");
        // A legitimate engine-placed size on the same display still shows.
        let probe = input(10_000);
        assert!(decide_border(&probe).is_ok());
    }

    #[test]
    fn controller_records_hide_reason_and_apply_lag() {
        let mut controller = BorderController::new();
        assert_eq!(controller.hide_reason(), None);
        // Show samples the observed→application lag on the shared clock:
        // `updated_ms` 10_000 applied at 10_150 reads 150 ms.
        let mut shown = input(10_150);
        shown.record.as_mut().unwrap().updated_ms = 10_000;
        assert!(matches!(controller.tick(&shown), BorderEffect::Show { .. }));
        assert_eq!(controller.hide_reason(), None);
        assert_eq!(
            (
                controller.apply_lag_max_ms(),
                controller.apply_lag_samples()
            ),
            (150, 1)
        );
        assert_eq!(controller.last_apply_lag_ms(), 150);
        // Steady-state tick stays silent without resampling.
        assert_eq!(controller.tick(&shown), BorderEffect::None);
        assert_eq!(
            (
                controller.apply_lag_max_ms(),
                controller.apply_lag_samples()
            ),
            (150, 1)
        );
        // A smaller lag keeps the maximum; the last-sample tracks latest.
        let mut moved = shown.clone();
        moved.now_ms = 10_160;
        moved.record.as_mut().unwrap().updated_ms = 10_150;
        moved
            .record
            .as_mut()
            .unwrap()
            .window
            .as_mut()
            .unwrap()
            .frame = ax_frame(110.0, 100.0, 400.0, 300.0);
        assert!(matches!(controller.tick(&moved), BorderEffect::Show { .. }));
        assert_eq!(
            (
                controller.apply_lag_max_ms(),
                controller.apply_lag_samples()
            ),
            (150, 2)
        );
        assert_eq!(controller.last_apply_lag_ms(), 10);
        // Denial records its token for the hide transition log; a later
        // show clears it.
        let mut denied = moved.clone();
        denied.now_ms = 10_170;
        denied.record.as_mut().unwrap().updated_ms = 10_170;
        denied.scope_allowed = false;
        assert_eq!(controller.tick(&denied), BorderEffect::Hide);
        assert_eq!(controller.hide_reason(), Some("out-of-scope"));
        // Silent-while-hidden ticks keep reporting the last token.
        assert_eq!(controller.tick(&denied), BorderEffect::None);
        assert_eq!(controller.hide_reason(), Some("out-of-scope"));
        let mut reshown = denied.clone();
        reshown.now_ms = 10_180;
        reshown.record.as_mut().unwrap().updated_ms = 10_180;
        reshown.scope_allowed = true;
        assert!(matches!(
            controller.tick(&reshown),
            BorderEffect::Show { .. }
        ));
        assert_eq!(controller.hide_reason(), None);
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> VisualRect {
        VisualRect { x, y, w, h }
    }

    fn visibles() -> Vec<VisualRect> {
        vec![rect(0.0, 25.0, 1440.0, 875.0)]
    }

    fn full_displays() -> Vec<VisualRect> {
        vec![
            rect(0.0, 0.0, 1440.0, 900.0),
            rect(1440.0, 100.0, 1920.0, 1080.0),
        ]
    }

    #[test]
    fn controller_shows_updates_and_hides_once() {
        let mut controller = BorderController::new();
        // First show creates with a reorder.
        let first = controller.tick(&input(10_000));
        assert!(matches!(
            first,
            BorderEffect::Show {
                target_wid: 7,
                reorder_above: true,
                ..
            }
        ));
        assert_eq!(
            (controller.shows(), controller.updates(), controller.hides()),
            (1, 0, 0)
        );
        // Identical tick stays silent (no frame spam while idle).
        assert_eq!(controller.tick(&input(10_001)), BorderEffect::None);
        assert_eq!(
            (controller.shows(), controller.updates(), controller.hides()),
            (1, 0, 0)
        );
        // Same-app focus switch (wid 7 -> 9): update with a reorder above
        // the new target.
        let mut switched = input(10_002);
        {
            let record = switched.record.as_mut().unwrap();
            record.focused_wid = Some(9);
            record.selected = Some((4242, Some(9)));
            let window = record.window.as_mut().unwrap();
            window.wid = 9;
            window.frame = ax_frame(520.0, 100.0, 400.0, 300.0);
        }
        let effect = controller.tick(&switched);
        assert!(matches!(
            effect,
            BorderEffect::Show {
                target_wid: 9,
                reorder_above: true,
                ..
            }
        ));
        assert_eq!(
            (controller.shows(), controller.updates(), controller.hides()),
            (1, 1, 0)
        );
        // Same-target move: update without a reorder (relative order kept).
        let mut moved = switched.clone();
        moved.now_ms = 10_003;
        moved
            .record
            .as_mut()
            .unwrap()
            .window
            .as_mut()
            .unwrap()
            .frame = ax_frame(530.0, 100.0, 400.0, 300.0);
        moved.record.as_mut().unwrap().updated_ms = 10_003;
        let effect = controller.tick(&moved);
        assert!(matches!(
            effect,
            BorderEffect::Show {
                target_wid: 9,
                reorder_above: false,
                ..
            }
        ));
        assert_eq!(
            (controller.shows(), controller.updates(), controller.hides()),
            (1, 2, 0)
        );
        // Focus loss hides once, then stays silent.
        let mut lost = moved.clone();
        lost.now_ms = 10_004;
        lost.record.as_mut().unwrap().focused_wid = None;
        lost.record.as_mut().unwrap().selected = None;
        lost.record.as_mut().unwrap().updated_ms = 10_004;
        assert_eq!(controller.tick(&lost), BorderEffect::Hide);
        assert_eq!(controller.tick(&lost), BorderEffect::None);
        assert_eq!(
            (controller.shows(), controller.updates(), controller.hides()),
            (1, 2, 1)
        );
        // Cleanup reset on an already-hidden controller stays silent.
        assert_eq!(controller.reset(), BorderEffect::None);
    }

    #[test]
    fn controller_config_change_restyles_without_refocus() {
        // A style change (width/color/theme) re-shows the same target
        // without a reorder: configuration applies consistently.
        let mut controller = BorderController::new();
        let _ = controller.tick(&input(10_000));
        let mut restyled = input(10_001);
        restyled.style.width = 5.0;
        restyled.style.color = (0x11, 0x22, 0x33);
        restyled.style.use_theme = false;
        let effect = controller.tick(&restyled);
        match effect {
            BorderEffect::Show {
                target_wid,
                reorder_above,
                outer,
                color,
                ..
            } => {
                assert_eq!(target_wid, 7);
                assert!(!reorder_above);
                assert_eq!(color, (0x11, 0x22, 0x33));
                // Width 5 around the (100,500,400x300) AppKit frame.
                assert_eq!(outer, rect(95.0, 495.0, 410.0, 310.0));
            }
            other => panic!("want restyle show, got {other:?}"),
        }
        // Carrier failure forgets the pushed state: the next identical tick
        // retries the show instead of wedging silent.
        controller.note_carrier_error();
        assert_eq!(controller.carrier_errors(), 1);
        assert!(matches!(
            controller.tick(&restyled),
            BorderEffect::Show {
                reorder_above: true,
                ..
            }
        ));
    }

    #[test]
    fn controller_off_disables_and_reset_cleans_up() {
        let mut controller = BorderController::new();
        let _ = controller.tick(&input(10_000));
        // Switch off: one hide, then silence.
        let mut off = input(10_001);
        off.enabled = false;
        assert_eq!(controller.tick(&off), BorderEffect::Hide);
        assert_eq!(controller.tick(&off), BorderEffect::None);
        assert_eq!(controller.hides(), 1);
        // Cleanup reset on a shown controller hides (the caller still
        // destroys the handle); a hidden one stays silent.
        let mut controller = BorderController::new();
        let _ = controller.tick(&input(10_000));
        assert_eq!(controller.reset(), BorderEffect::Hide);
        assert_eq!(controller.reset(), BorderEffect::None);
        assert_eq!(controller.hides(), 1);
    }

    #[test]
    fn snapshot_primary_height_reads_origin_display() {
        use crate::topology::{DisplayGeometry, PointsRect};
        let displays = vec![
            DisplayGeometry {
                cg_id: 2,
                uuid: None,
                cg_bounds: Some(PointsRect {
                    x: 1440.0,
                    y: 0.0,
                    w: 1440.0,
                    h: 900.0,
                }),
                visible: None,
                scale: 2.0,
            },
            DisplayGeometry {
                cg_id: 1,
                uuid: None,
                cg_bounds: Some(PointsRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1440.0,
                    h: 900.0,
                }),
                visible: None,
                scale: 2.0,
            },
        ];
        assert_eq!(snapshot_primary_height(&displays), Some(900.0));
        assert_eq!(snapshot_primary_height(&[]), None);
        // Degenerate origins refuse instead of guessing.
        let flat = vec![DisplayGeometry {
            cg_id: 1,
            uuid: None,
            cg_bounds: Some(PointsRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            }),
            visible: None,
            scale: 1.0,
        }];
        assert_eq!(snapshot_primary_height(&flat), None);
    }
}
