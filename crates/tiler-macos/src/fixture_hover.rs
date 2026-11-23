#![forbid(unsafe_code)]

//! Opt-in live hover prove-out for `fixture-leg --hover-probe`.
//!
//! The leg's normal chord phase already drives the product tap. This module
//! adds one bounded post-chord sequence on the same owned fixture windows so
//! the cursor/hover producer is proven end to end:
//!
//! 1. park the pointer off every owned window and let one observer sample
//!    land as an honest miss (departure clears the hover memory),
//! 2. read the three native window frames and require one tall anchor plus a
//!    stacked pair (a 2D subgroup); anything else refuses instead of
//!    fabricating perpendicularity,
//! 3. seed accepted focus history with explicit product chords (the anchor,
//!    then one column member, then the other),
//! 4. warp into the chosen column member while the other is focused and wait
//!    for focus-follows-cursor to move focus onto the hovered window,
//! 5. park the pointer off every owned window again and wait out the resting
//!    hit refresh so the next command cannot be attributed to a stale hover,
//! 6. natively refocus the tall anchor, then post the perpendicular chord
//!    into the stack and require the hovered member to win (recency).
//!
//! Evidence is recorded, never inferred: native AX frames and focused-window
//! ids, the two posted chords, the staged product `tiler-diag` cause tokens
//! (`focus-follows-cursor`, `explicit-command`) with counts and stamps, and
//! the verified off-owned pointer hit. The sequence is bounded throughout
//! (focus deadlines, settle windows, grid-bounded off-point search) and a
//! mismatched focus, frame, or missing cause token fails the leg with a
//! precise reason; nothing is retried against a different layout.
//!
//! Cause tokens ride the product's own opt-in stage-timing diagnostics, so
//! the leg launches the product with [`STAGE_TIMING_ENV`] only when this
//! probe is requested (adapter-only environment; no settings write). A log
//! without the expected token growth fails the leg rather than accepting an
//! ambiguous result.

use std::path::Path;

/// Minimum owned windows the probe needs: one tall anchor plus a stacked pair.
pub(crate) const HOVER_MIN_WINDOWS: usize = 3;

/// Settle after parking the pointer off every owned window (ms): one 250 ms
/// observer tick plus the 1 s resting hit refresh plus dispatch margin.
pub(crate) const HOVER_OFF_SETTLE_MS: u64 = 2_000;

/// Product launch environment that turns on the cause diagnostics the probe
/// records. Adapter-only, harness-scoped, never a settings write.
pub(crate) const STAGE_TIMING_ENV: &str = "TILER_MACOS_STAGE_TIMING=1";

/// Bounded deadline to observe one focus transition (ms).
const HOVER_FOCUS_DEADLINE_MS: u64 = 6_000;

/// Bounded deadline to observe one cause token after its effect (ms).
const HOVER_CAUSE_DEADLINE_MS: u64 = 4_000;

/// Poll interval for focus and cause-log reads (ms).
const HOVER_POLL_MS: u64 = 50;

/// Margin around blocked/owned rectangles when choosing the off point.
const OFF_MARGIN: f64 = 2.0;

/// Candidate grid resolution over the display's visible frame.
const OFF_GRID: usize = 12;

/// One native window frame in AX/CG top-left screen points.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frame {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Frame {
    fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    fn usable(self) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite())
            && self.w > 0.0
            && self.h > 0.0
    }

    fn center(self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    fn holds(self, x: f64, y: f64, margin: f64) -> bool {
        x >= self.x - margin
            && x <= self.x + self.w + margin
            && y >= self.y - margin
            && y <= self.y + self.h + margin
    }

    fn json(self) -> serde_json::Value {
        serde_json::json!({"x": self.x, "y": self.y, "w": self.w, "h": self.h})
    }
}

/// Union bounds of one non-empty frame slice.
fn union(frames: &[Frame]) -> Option<Frame> {
    let first = *frames.first()?;
    let mut frame = first;
    for next in &frames[1..] {
        let (x0, y0) = (frame.x.min(next.x), frame.y.min(next.y));
        let (x1, y1) = (
            (frame.x + frame.w).max(next.x + next.w),
            (frame.y + frame.h).max(next.y + next.h),
        );
        frame = Frame::new(x0, y0, x1 - x0, y1 - y0);
    }
    Some(frame)
}

/// The admitted 2D subgroup: one tall anchor beside a stacked pair. The anchor
/// is the perpendicular descent source; `above`/`below` are the stack members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HoverLayout {
    pub(crate) tall: usize,
    pub(crate) above: usize,
    pub(crate) below: usize,
    pub(crate) column_on_right: bool,
}

impl HoverLayout {
    /// Chord that descends from the tall anchor into the stacked pair.
    fn column_chord(self) -> &'static str {
        if self.column_on_right {
            "focus-l"
        } else {
            "focus-h"
        }
    }
}

/// Compact per-window geometry for a layout refusal: index (the joined order),
/// top-left AX position and size, so a mismatched native layout is reported
/// exactly instead of only by height.
fn frames_brief(frames: &[Frame]) -> String {
    frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            format!(
                "#{index} {:.1},{:.1} {:.1}x{:.1}",
                frame.x, frame.y, frame.w, frame.h
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Classify three native frames: exactly one full-height anchor beside two
/// stacked, same-column windows (the `H[A, V[B,C]]` admission shape). Pure;
/// unit-tested. A single-row (three landscape halves) or otherwise mismatched
/// layout refuses with the frames named so the leg fails instead of faking a
/// perpendicular subgroup.
fn classify_layout(frames: &[Frame]) -> Result<HoverLayout, String> {
    if frames.len() != HOVER_MIN_WINDOWS {
        return Err(format!(
            "needs exactly {HOVER_MIN_WINDOWS} owned windows, got {}",
            frames.len()
        ));
    }
    let bounds = union(frames).ok_or_else(|| "owned window frames are empty".to_owned())?;
    if !bounds.usable() {
        return Err("owned window frames are degenerate".to_owned());
    }
    let heights: Vec<f64> = frames.iter().map(|f| f.h).collect();
    // Full-height anchor: within 10% of the union height.
    let tall: Vec<usize> = (0..frames.len())
        .filter(|index| frames[*index].h >= bounds.h - bounds.h * 0.1)
        .collect();
    if tall.len() != 1 {
        return Err(format!(
            "native frames are not one tall anchor plus a stacked pair (heights {heights:?}; frames {}); refusing to fabricate perpendicularity",
            frames_brief(frames)
        ));
    }
    let anchor = tall[0];
    let others: Vec<usize> = (0..frames.len()).filter(|index| *index != anchor).collect();
    let (left, right) = (others[0], others[1]);
    let x_tolerance = bounds.w * 0.06;
    if (frames[left].x - frames[right].x).abs() > x_tolerance
        || (frames[left].w - frames[right].w).abs() > x_tolerance
    {
        return Err(format!(
            "native frames are not a stacked same-column pair (x {:.1}/{:.1} w {:.1}/{:.1}; frames {}); refusing to fabricate perpendicularity",
            frames[left].x,
            frames[right].x,
            frames[left].w,
            frames[right].w,
            frames_brief(frames)
        ));
    }
    let (above, below) = if frames[left].y + frames[left].h <= frames[right].y + OFF_MARGIN {
        (left, right)
    } else if frames[right].y + frames[right].h <= frames[left].y + OFF_MARGIN {
        (right, left)
    } else {
        return Err(format!(
            "native column members overlap vertically (y {:.1} h {:.1}, y {:.1} h {:.1}; frames {}); refusing to fabricate perpendicularity",
            frames[left].y,
            frames[left].h,
            frames[right].y,
            frames[right].h,
            frames_brief(frames)
        ));
    };
    let column_left = frames[left].x.min(frames[right].x);
    let column_right = (frames[left].x + frames[left].w).max(frames[right].x + frames[right].w);
    let column_on_right = if frames[anchor].x + frames[anchor].w <= column_left + OFF_MARGIN {
        true
    } else if frames[anchor].x >= column_right - OFF_MARGIN {
        false
    } else {
        return Err(format!(
            "native anchor is not horizontally beside the stacked pair (anchor x {:.1} w {:.1}, column {:.1}..{:.1}; frames {}); refusing to fabricate perpendicularity",
            frames[anchor].x,
            frames[anchor].w,
            column_left,
            column_right,
            frames_brief(frames)
        ));
    };
    Ok(HoverLayout {
        tall: anchor,
        above,
        below,
        column_on_right,
    })
}

/// Product cause-token counts plus the newest line stamp for each token.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CauseTokens {
    follows: u64,
    follows_stamp_us: Option<u64>,
    explicit: u64,
    explicit_stamp_us: Option<u64>,
}

/// Count the staged focus-route cause tokens in one product log snapshot.
/// Only the explicit `cause=` fields count (the ring `kind=` names are a
/// different schema); a missing stamp stays `None`, never zero. Pure;
/// unit-tested.
fn cause_tokens(raw: &str) -> CauseTokens {
    let mut out = CauseTokens::default();
    for line in raw.lines() {
        let stamp = line
            .split_whitespace()
            .find_map(|token| token.strip_prefix("stamp_us="))
            .and_then(|value| value.parse::<u64>().ok());
        if line.contains("cause=focus-follows-cursor") {
            out.follows = out.follows.saturating_add(1);
            if stamp.is_some() {
                out.follows_stamp_us = stamp;
            }
        }
        if line.contains("cause=explicit-command") {
            out.explicit = out.explicit.saturating_add(1);
            if stamp.is_some() {
                out.explicit_stamp_us = stamp;
            }
        }
    }
    out
}

fn cause_json(now: CauseTokens, base: CauseTokens) -> serde_json::Value {
    serde_json::json!({
        "focus-follows-cursor": {
            "count": now.follows,
            "delta": now.follows.saturating_sub(base.follows),
            "newest_stamp_us": now.follows_stamp_us,
        },
        "explicit-command": {
            "count": now.explicit,
            "delta": now.explicit.saturating_sub(base.explicit),
            "newest_stamp_us": now.explicit_stamp_us,
        },
    })
}

#[cfg(target_os = "macos")]
fn read_causes(path: &Path) -> Result<CauseTokens, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(cause_tokens(&String::from_utf8_lossy(&bytes))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(CauseTokens::default()),
        Err(e) => Err(format!("product log unreadable ({e})")),
    }
}

/// Running cause-token snapshot: every step records cumulative counts plus the
/// lines observed since the previous step, and required tokens are awaited
/// under a bounded deadline.
#[cfg(target_os = "macos")]
struct CauseRecorder<'a> {
    path: &'a Path,
    last: CauseTokens,
}

#[cfg(target_os = "macos")]
impl<'a> CauseRecorder<'a> {
    fn new(path: &'a Path) -> Self {
        Self {
            path,
            last: CauseTokens::default(),
        }
    }

    fn take(&mut self) -> Result<serde_json::Value, String> {
        let now = read_causes(self.path)?;
        let base = self.last;
        self.last = now;
        Ok(cause_json(now, base))
    }

    /// Wait until one token grows past the previous snapshot, then record it.
    fn await_token(&mut self, follows: bool) -> Result<serde_json::Value, String> {
        let base = self.last;
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(HOVER_CAUSE_DEADLINE_MS);
        loop {
            if crate::space_drag_diag_live::diag_interrupted() {
                return Err("interrupted (SIGINT/SIGTERM latched)".to_owned());
            }
            let now = read_causes(self.path)?;
            let grew = if follows {
                now.follows > base.follows
            } else {
                now.explicit > base.explicit
            };
            if grew {
                self.last = now;
                return Ok(cause_json(now, base));
            }
            if std::time::Instant::now() >= deadline {
                let token = if follows {
                    "focus-follows-cursor"
                } else {
                    "explicit-command"
                };
                return Err(format!(
                    "product log emitted no new {token} cause line within {HOVER_CAUSE_DEADLINE_MS}ms (stage timing off? counts {}/{} -> {}/{})",
                    base.follows, base.explicit, now.follows, now.explicit
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(HOVER_POLL_MS));
        }
    }
}

#[cfg(target_os = "macos")]
fn refuse(stage: &str, reason: String) -> (i32, String) {
    (1, format!("hover-probe: {stage}: {reason}"))
}

/// Bounded wait for two identical native frame reads before classification
/// (ms): the product may still be applying its initial tiling when the chord
/// phase ends, and a mid-flight frame read would make the layout ambiguous.
const HOVER_FRAME_SETTLE_MS: u64 = 3_000;

/// Poll interval between native frame reads while waiting for stability (ms).
const HOVER_FRAME_POLL_MS: u64 = 200;

/// Read one native AX frame for each joined `(caller, wid)` pair, requiring
/// two consecutive identical reads (bounded) so a mid-flight tiling pass is
/// never classified as the settled layout. A layout that never settles is a
/// refusal naming the last observed geometry, never an accepted guess.
#[cfg(target_os = "macos")]
fn read_settled_frames(
    fixture_pid: u32,
    windows: &[(String, u32)],
) -> Result<Vec<Frame>, (i32, String)> {
    let start = std::time::Instant::now();
    let mut last: Option<Vec<Frame>> = None;
    loop {
        if crate::space_drag_diag_live::diag_interrupted() {
            return Err(refuse(
                "frames",
                "interrupted (SIGINT/SIGTERM latched)".to_owned(),
            ));
        }
        let frames = read_frames(fixture_pid, windows)?;
        if last.as_ref().is_some_and(|previous| *previous == frames) {
            return Ok(frames);
        }
        last = Some(frames);
        if start.elapsed() >= std::time::Duration::from_millis(HOVER_FRAME_SETTLE_MS) {
            let observed = last.as_deref().map(frames_brief).unwrap_or_default();
            return Err(refuse(
                "frames",
                format!(
                    "native frames did not settle within {HOVER_FRAME_SETTLE_MS}ms (last {observed})"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(HOVER_FRAME_POLL_MS));
    }
}

/// Read one native AX frame for each joined `(caller, wid)` pair.
#[cfg(target_os = "macos")]
fn read_frames(fixture_pid: u32, windows: &[(String, u32)]) -> Result<Vec<Frame>, (i32, String)> {
    use crate::ffi::{AxElement, ax_error_name, ax_get_frame, ax_window_id};
    let app = AxElement::application(fixture_pid as i32).ok_or_else(|| {
        refuse(
            "frames",
            "no AX application element for the fixture".to_owned(),
        )
    })?;
    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    let elements = app.windows().map_err(|code| {
        refuse(
            "frames",
            format!("fixture AXWindows unreadable ({})", ax_error_name(code)),
        )
    })?;
    let mut frames = Vec::with_capacity(windows.len());
    for (caller, wid) in windows {
        let element = elements
            .iter()
            .find(|element| ax_window_id(element) == Some(*wid))
            .ok_or_else(|| {
                refuse(
                    "frames",
                    format!("caller {caller} window {wid} is not in the fixture AX window list"),
                )
            })?;
        let _ = crate::ffi::ax_set_messaging_timeout(element, 2.0);
        let frame = ax_get_frame(element).map_err(|code| {
            refuse(
                "frames",
                format!("caller {caller} frame unreadable ({})", ax_error_name(code)),
            )
        })?;
        let frame = Frame::new(frame.x, frame.y, frame.w, frame.h);
        if !frame.usable() {
            return Err(refuse(
                "frames",
                format!(
                    "caller {caller} frame is degenerate ({:.1},{:.1},{:.1},{:.1})",
                    frame.x, frame.y, frame.w, frame.h
                ),
            ));
        }
        frames.push(frame);
    }
    Ok(frames)
}

/// The index of the owned window the fixture app currently reports as focused.
/// Any other focused application is an explicit error (retried by the callers
/// that race a focus dispatch).
#[cfg(target_os = "macos")]
fn focused_caller(fixture_pid: u32, windows: &[(String, u32)]) -> Result<usize, String> {
    let (pid, wid) = crate::ffi::ax_focused_preimage_detailed()
        .map_err(|e| format!("focused preimage unreadable ({e})"))?;
    if u32::try_from(pid).ok() != Some(fixture_pid) {
        return Err("focused application is not the fixture (foreground lost)".to_owned());
    }
    let wid = wid.ok_or_else(|| "fixture reports no focused window".to_owned())?;
    windows
        .iter()
        .position(|(_, owned)| *owned == wid)
        .ok_or_else(|| format!("focused window {wid} is not an owned window"))
}

#[cfg(target_os = "macos")]
fn elapsed_ms(start: std::time::Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Poll the focused ownered window until it is one of `wanted` or the deadline
/// passes. Returns the observed index and elapsed milliseconds.
#[cfg(target_os = "macos")]
fn await_focus(
    fixture_pid: u32,
    windows: &[(String, u32)],
    wanted: &[usize],
) -> Result<(usize, u64), String> {
    let start = std::time::Instant::now();
    let deadline = start + std::time::Duration::from_millis(HOVER_FOCUS_DEADLINE_MS);
    let mut last: String;
    loop {
        if crate::space_drag_diag_live::diag_interrupted() {
            return Err("interrupted (SIGINT/SIGTERM latched)".to_owned());
        }
        match focused_caller(fixture_pid, windows) {
            Ok(index) if wanted.contains(&index) => return Ok((index, elapsed_ms(start))),
            Ok(index) => last = format!("focus is {}", windows[index].0),
            Err(error) => last = error,
        }
        if std::time::Instant::now() >= deadline {
            let names: Vec<&str> = wanted
                .iter()
                .map(|index| windows[*index].0.as_str())
                .collect();
            return Err(format!(
                "focus did not reach {} within {HOVER_FOCUS_DEADLINE_MS}ms ({last})",
                names.join("|")
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(HOVER_POLL_MS));
    }
}

/// Warp the pointer to one point and prove the OS read it back there.
#[cfg(target_os = "macos")]
fn warp_to(x: f64, y: f64) -> Result<(f64, f64), String> {
    crate::ffi::warp_cursor_position(x, y)?;
    let (read_x, read_y) = crate::synth::cursor_position()
        .ok_or_else(|| "cursor position unreadable after warp".to_owned())?;
    if (read_x - x).abs() > 2.0 || (read_y - y).abs() > 2.0 {
        return Err(format!(
            "cursor did not land at the requested point (asked {x:.0},{y:.0}, read {read_x:.0},{read_y:.0})"
        ));
    }
    Ok((read_x, read_y))
}

/// One verified off-owned pointer point plus the native hit it produced.
#[cfg(target_os = "macos")]
struct OffPoint {
    x: f64,
    y: f64,
    class: &'static str,
    hit: serde_json::Value,
}

/// AX hit classification for the off point: an owned window refuses, a
/// non-owned window is recorded by class (never by pid), and a position whose
/// element owns no window is the honest miss.
#[cfg(target_os = "macos")]
fn hit_is_off_owned(
    x: f64,
    y: f64,
    owned: &[(u32, u32)],
    fixture_pid: u32,
) -> Option<serde_json::Value> {
    match crate::ffi::ax_hit_test_owner(x, y) {
        Ok((pid, wid)) => {
            if owned
                .iter()
                .any(|(owner_pid, owner_wid)| *owner_pid == pid && *owner_wid == wid)
            {
                return None;
            }
            Some(serde_json::json!({
                "kind": "window",
                "owned": false,
                "fixture": pid == fixture_pid,
            }))
        }
        Err(_) => {
            // Only an element that exists but owns no window is an honest
            // miss; an unreadable position is not.
            crate::ffi::ax_element_at_position(x, y).ok()?;
            Some(serde_json::json!({"kind": "no-window", "owned": false, "fixture": false}))
        }
    }
}

/// Choose a bounded, verified off-owned pointer point: the first wallpaper
/// grid point outside every onscreen window, else a menu-bar point. The
/// hosting terminal is never a candidate (it is an onscreen window, excluded
/// by the same rule); no point is accepted while it hits an owned window.
#[cfg(target_os = "macos")]
fn choose_off_point(
    owned_frames: &[Frame],
    owned: &[(u32, u32)],
    fixture_pid: u32,
) -> Result<OffPoint, (i32, String)> {
    let ns = crate::ffi::ns_display_snapshot()
        .map_err(|e| refuse("off-point", format!("display snapshot unreadable ({e})")))?;
    let displays = crate::topology::join_displays(&ns);
    let display = displays
        .iter()
        .find(|display| {
            let Some(visible) = display.visible else {
                return false;
            };
            let Some(bounds) = union(owned_frames) else {
                return false;
            };
            let (cx, cy) = bounds.center();
            visible.overlap_area(&crate::topology::PointsRect {
                x: cx,
                y: cy,
                w: 0.0,
                h: 0.0,
            }) > 0.0
        })
        .or_else(|| displays.iter().find(|display| display.visible.is_some()))
        .ok_or_else(|| refuse("off-point", "no display with a visible frame".to_owned()))?;
    let visible = display.visible.ok_or_else(|| {
        refuse(
            "off-point",
            "selected display has no visible frame".to_owned(),
        )
    })?;
    let frame = display.cg_bounds.ok_or_else(|| {
        refuse(
            "off-point",
            "selected display has no converted frame".to_owned(),
        )
    })?;
    let blocked = crate::ffi::cg_window_list_basic()
        .map_err(|e| refuse("off-point", format!("CG window list unreadable ({e})")))?;
    let mut candidates: Vec<(f64, f64, &'static str, bool)> = Vec::new();
    for row in 0..=OFF_GRID {
        for column in 0..=OFF_GRID {
            let x = visible.x + 8.0 + (visible.w - 16.0) * column as f64 / OFF_GRID as f64;
            let y = visible.y + 8.0 + (visible.h - 16.0) * row as f64 / OFF_GRID as f64;
            let owned_hit = owned_frames
                .iter()
                .any(|owned_frame| owned_frame.holds(x, y, OFF_MARGIN));
            if owned_hit {
                continue;
            }
            let blocked_hit = blocked.iter().any(|window| {
                window.onscreen
                    && x >= window.bounds.x - OFF_MARGIN
                    && x <= window.bounds.x + window.bounds.w + OFF_MARGIN
                    && y >= window.bounds.y - OFF_MARGIN
                    && y <= window.bounds.y + window.bounds.h + OFF_MARGIN
            });
            if blocked_hit {
                continue;
            }
            candidates.push((x, y, "wallpaper", false));
        }
    }
    // Menu-bar fallback: only when the visible frame excludes a real menu
    // bar. Auto-hidden bars leave no gap, so the fallback would land on the
    // fullscreen window's content; require the OS-owned strip instead, and
    // accept it only as an element that owns no window (never a window, and
    // so never the protected hosting terminal).
    if visible.y - frame.y >= 8.0 {
        let y = frame.y + (visible.y - frame.y) / 2.0;
        for fraction in [0.15, 0.3, 0.5] {
            let x = frame.x + frame.w * fraction;
            candidates.push((x, y, "menu-bar", true));
        }
    }
    for (x, y, class, no_window_only) in candidates {
        if let Some(hit) = hit_is_off_owned(x, y, owned, fixture_pid) {
            if no_window_only
                && hit.get("kind").and_then(serde_json::Value::as_str) != Some("no-window")
            {
                continue;
            }
            return Ok(OffPoint { x, y, class, hit });
        }
    }
    Err(refuse(
        "off-point",
        "no bounded point outside every owned window and onscreen window was found".to_owned(),
    ))
}

/// Park the pointer at one verified off-owned point and hold the settled
/// window so the next sample is an honest miss.
#[cfg(target_os = "macos")]
fn park_off(point: &OffPoint) -> Result<serde_json::Value, String> {
    let (x, y) = warp_to(point.x, point.y)?;
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(HOVER_OFF_SETTLE_MS);
    while std::time::Instant::now() < deadline {
        if crate::space_drag_diag_live::diag_interrupted() {
            return Err("interrupted (SIGINT/SIGTERM latched)".to_owned());
        }
        std::thread::sleep(std::time::Duration::from_millis(HOVER_POLL_MS));
    }
    Ok(serde_json::json!({
        "cursor": {"x": x, "y": y},
        "class": point.class,
        "hit": point.hit,
        "settle_ms": HOVER_OFF_SETTLE_MS,
    }))
}

/// Run the bounded hover prove-out on three owned, joined fixture windows.
#[cfg(target_os = "macos")]
pub(crate) fn run(
    driver: &crate::fixture_post::FixtureDriver<'_, crate::fixture_post::LiveTransport>,
    fixture_pid: u32,
    windows: &[(String, u32)],
    modifier: crate::config::ProbeModifier,
    product_stderr: &Path,
) -> Result<serde_json::Value, (i32, String)> {
    if windows.len() < HOVER_MIN_WINDOWS {
        return Err(refuse(
            "setup",
            format!(
                "needs at least {HOVER_MIN_WINDOWS} owned windows, got {}",
                windows.len()
            ),
        ));
    }
    let windows = &windows[..HOVER_MIN_WINDOWS];
    let frames = read_settled_frames(fixture_pid, windows)?;
    let layout = classify_layout(&frames).map_err(|e| refuse("layout", e))?;
    let bounds = union(&frames).unwrap_or(frames[0]);
    let relative = |frame: Frame| {
        serde_json::json!({
            "x": (frame.x - bounds.x) / bounds.w,
            "y": (frame.y - bounds.y) / bounds.h,
            "w": frame.w / bounds.w,
            "h": frame.h / bounds.h,
        })
    };
    let region = |index: usize| {
        if index == layout.tall {
            "tall"
        } else if index == layout.above {
            "above"
        } else {
            "below"
        }
    };
    let frame_rows: Vec<serde_json::Value> = frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            serde_json::json!({
                "caller": windows[index].0,
                "wid": windows[index].1,
                "region": region(index),
                "ax": frame.json(),
                "relative": relative(*frame),
            })
        })
        .collect();
    let owned: Vec<(u32, u32)> = windows.iter().map(|(_, wid)| (fixture_pid, *wid)).collect();
    let off_point = choose_off_point(&frames, &owned, fixture_pid)?;
    let original_cursor = crate::synth::cursor_position();
    let mut recorder = CauseRecorder::new(product_stderr);
    let mut steps: Vec<serde_json::Value> = Vec::new();
    let column_chord = layout.column_chord();

    // Step 0: depart to an honest miss before any hover can mint.
    let parked = park_off(&off_point).map_err(|e| refuse("pointer-park", e))?;
    steps.push(serde_json::json!({
        "step": "pointer-park",
        "off": parked,
        "causes": recorder.take().map_err(|e| refuse("cause-log", e))?,
    }));

    // Step 1: known explicit focus on the tall anchor.
    let before = focused_caller(fixture_pid, windows).map_err(|e| refuse("focus-anchor", e))?;
    driver
        .focus(&windows[layout.tall].0, windows[layout.tall].1)
        .map_err(|(_code, error)| {
            refuse("focus-anchor", format!("fixture focus refused: {error}"))
        })?;
    let (anchor, anchor_ms) =
        await_focus(fixture_pid, windows, &[layout.tall]).map_err(|e| refuse("focus-anchor", e))?;
    steps.push(serde_json::json!({
        "step": "focus-anchor",
        "caller": windows[layout.tall].0,
        "focus_before": windows[before].0,
        "focus_after": windows[anchor].0,
        "observed_after_ms": anchor_ms,
        "causes": recorder.take().map_err(|e| refuse("cause-log", e))?,
    }));

    // Step 2: seed accepted history with the stack member the perpendicular
    // fallback would pick first.
    let guided = focus_and_post(
        driver,
        windows,
        layout.tall,
        column_chord,
        modifier,
        fixture_pid,
        &[layout.above, layout.below],
        recorder.path,
    )?;
    let first = guided.focus;
    steps.push(serde_json::json!({
        "step": "seed-column",
        "caller": windows[layout.tall].0,
        "chord": column_chord,
        "focus_before": windows[layout.tall].0,
        "focus_after": windows[guided.focus].0,
        "observed_after_ms": guided.elapsed_ms,
        "post": guided.post,
        "causes": guided.causes,
    }));

    // Step 3: seed the alternate member as the most recent accepted focus.
    let alternate = if first == layout.above {
        layout.below
    } else {
        layout.above
    };
    let vertical = if alternate == layout.above {
        "focus-k"
    } else {
        "focus-j"
    };
    let guided = focus_and_post(
        driver,
        windows,
        first,
        vertical,
        modifier,
        fixture_pid,
        &[alternate],
        recorder.path,
    )?;
    steps.push(serde_json::json!({
        "step": "seed-alternate",
        "caller": windows[first].0,
        "chord": vertical,
        "focus_before": windows[first].0,
        "focus_after": windows[guided.focus].0,
        "observed_after_ms": guided.elapsed_ms,
        "post": guided.post,
        "causes": guided.causes,
    }));

    // Step 4: warp onto the hovered member; focus-follows-cursor must move
    // focus there while the alternate is focused.
    let hover_point = frames[first].center();
    let (hover_x, hover_y) = warp_to(hover_point.0, hover_point.1)
        .map_err(|e| refuse("hover", format!("cursor warp refused ({e})")))?;
    let (hovered, hover_ms) = await_focus(fixture_pid, windows, &[first]).map_err(|e| {
        refuse(
            "hover",
            format!("focus-follows-cursor transition not observed: {e}"),
        )
    })?;
    let causes = recorder.await_token(true).map_err(|e| refuse("hover", e))?;
    steps.push(serde_json::json!({
        "step": "hover",
        "caller": windows[hovered].0,
        "cursor": {"x": hover_x, "y": hover_y},
        "focus_before": windows[alternate].0,
        "focus_after": windows[hovered].0,
        "observed_after_ms": hover_ms,
        "deadline_ms": HOVER_FOCUS_DEADLINE_MS,
        "causes": causes,
    }));

    // Step 5: depart to an honest miss and wait out the resting hit refresh.
    let parked = park_off(&off_point).map_err(|e| refuse("pointer-off", e))?;
    let focused = focused_caller(fixture_pid, windows).map_err(|e| {
        refuse(
            "pointer-off",
            format!("focus unreadable after departure ({e})"),
        )
    })?;
    if focused != hovered {
        return Err(refuse(
            "pointer-off",
            format!(
                "leaving the hover target moved focus to {} (expected {})",
                windows[focused].0, windows[hovered].0
            ),
        ));
    }
    steps.push(serde_json::json!({
        "step": "pointer-off",
        "off": parked,
        "focus_before": windows[hovered].0,
        "focus_after": windows[focused].0,
        "causes": recorder.take().map_err(|e| refuse("cause-log", e))?,
    }));

    // Step 6: explicit native refocus of the tall anchor (observed focus only;
    // accepted history keeps the hovered member newest).
    driver
        .focus(&windows[layout.tall].0, windows[layout.tall].1)
        .map_err(|(_code, error)| {
            refuse("refocus-anchor", format!("fixture focus refused: {error}"))
        })?;
    let (anchor, anchor_ms) = await_focus(fixture_pid, windows, &[layout.tall])
        .map_err(|e| refuse("refocus-anchor", e))?;
    steps.push(serde_json::json!({
        "step": "refocus-anchor",
        "caller": windows[layout.tall].0,
        "focus_before": windows[hovered].0,
        "focus_after": windows[anchor].0,
        "observed_after_ms": anchor_ms,
        "causes": recorder.take().map_err(|e| refuse("cause-log", e))?,
    }));

    // Step 7: perpendicular descent from the anchor must pick the hovered
    // member (recency), never the alternate seeded later without hovering.
    if hit_is_off_owned(off_point.x, off_point.y, &owned, fixture_pid).is_none() {
        return Err(refuse(
            "perpendicular",
            "pointer is over an owned window again; a stale hover could re-mint".to_owned(),
        ));
    }
    let guided = focus_and_post(
        driver,
        windows,
        layout.tall,
        column_chord,
        modifier,
        fixture_pid,
        &[layout.above, layout.below],
        recorder.path,
    )?;
    if guided.focus != hovered || guided.focus == alternate {
        return Err(refuse(
            "perpendicular",
            format!(
                "perpendicular chord selected {} (hovered member is {}, alternate {})",
                windows[guided.focus].0, windows[hovered].0, windows[alternate].0
            ),
        ));
    }
    let causes = recorder
        .await_token(false)
        .map_err(|e| refuse("perpendicular", e))?;
    steps.push(serde_json::json!({
        "step": "perpendicular",
        "caller": windows[layout.tall].0,
        "chord": column_chord,
        "focus_before": windows[anchor].0,
        "focus_after": windows[guided.focus].0,
        "expected": windows[hovered].0,
        "observed_after_ms": guided.elapsed_ms,
        "post": guided.post,
        "causes": causes,
    }));

    // Restoring the original pointer is post-observation cleanup: it happens
    // after the final cause snapshot so it can never alter the recorded
    // sequence evidence, and it may produce further samples before teardown.
    let restored = match original_cursor {
        Some((x, y)) => warp_to(x, y).is_ok(),
        None => false,
    };
    let final_causes = recorder.take().map_err(|e| refuse("cause-log", e))?;
    Ok(serde_json::json!({
        "layout": {
            "kind": "tall-anchor-plus-stack",
            "tall": windows[layout.tall].0,
            "above": windows[layout.above].0,
            "below": windows[layout.below].0,
            "column_side": if layout.column_on_right { "right" } else { "left" },
            "column_chord": column_chord,
        },
        "frames": frame_rows,
        "off_point": {
            "class": off_point.class,
            "hit": off_point.hit,
        },
        "steps": steps,
        "proven": {
            "focus_follows_cursor": true,
            "hovered_member_recency": true,
            "hovered_caller": windows[hovered].0,
            "alternate_caller": windows[alternate].0,
        },
        "cause_tokens": final_causes,
        "cursor_restored": restored,
    }))
}

/// One focus-plus-post step: native focus of `from`, one product chord, then a
/// bounded wait for focus to land on one of `wanted`.
#[cfg(target_os = "macos")]
struct GuidedStep {
    focus: usize,
    elapsed_ms: u64,
    post: serde_json::Value,
    causes: serde_json::Value,
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn focus_and_post(
    driver: &crate::fixture_post::FixtureDriver<'_, crate::fixture_post::LiveTransport>,
    windows: &[(String, u32)],
    from: usize,
    chord: &str,
    modifier: crate::config::ProbeModifier,
    fixture_pid: u32,
    wanted: &[usize],
    product_stderr: &Path,
) -> Result<GuidedStep, (i32, String)> {
    driver
        .focus(&windows[from].0, windows[from].1)
        .map_err(|(_code, error)| {
            refuse(
                "guided-step",
                format!("fixture focus of {} refused: {error}", windows[from].0),
            )
        })?;
    let raw = driver
        .post(&windows[from].0, windows[from].1, chord, modifier)
        .map_err(|(_code, error)| {
            refuse(
                "guided-step",
                format!("fixture post of {chord} refused: {error}"),
            )
        })?;
    let post: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| refuse("guided-step", format!("post receipt unreadable ({e})")))?;
    let (focus, elapsed_ms) = await_focus(fixture_pid, windows, wanted)
        .map_err(|e| refuse("guided-step", format!("{chord}: {e}")))?;
    let mut recorder = CauseRecorder::new(product_stderr);
    let causes = recorder.take().map_err(|e| refuse("cause-log", e))?;
    Ok(GuidedStep {
        focus,
        elapsed_ms,
        post,
        causes,
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn run(
    _driver: &crate::fixture_post::FixtureDriver<'_, crate::fixture_post::LiveTransport>,
    _fixture_pid: u32,
    _windows: &[(String, u32)],
    _modifier: crate::config::ProbeModifier,
    _product_stderr: &Path,
) -> Result<serde_json::Value, (i32, String)> {
    Err((2, "hover-probe is macOS-only".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: f64, y: f64, w: f64, h: f64) -> Frame {
        Frame::new(x, y, w, h)
    }

    #[test]
    fn layout_accepts_one_tall_anchor_beside_a_stacked_pair() {
        // H[A, V[B, C]]: left anchor, right column (above then below).
        let left = [
            frame(0.0, 0.0, 800.0, 900.0),
            frame(800.0, 0.0, 800.0, 450.0),
            frame(800.0, 450.0, 800.0, 450.0),
        ];
        let layout = classify_layout(&left).expect("left anchor layout");
        assert_eq!(layout.tall, 0);
        assert_eq!(layout.above, 1);
        assert_eq!(layout.below, 2);
        assert!(layout.column_on_right);
        assert_eq!(layout.column_chord(), "focus-l");
        // Mirrored: right anchor, left column.
        let right = [
            frame(800.0, 0.0, 800.0, 900.0),
            frame(0.0, 0.0, 800.0, 450.0),
            frame(0.0, 450.0, 800.0, 450.0),
        ];
        let layout = classify_layout(&right).expect("right anchor layout");
        assert_eq!(layout.tall, 0);
        assert!(!layout.column_on_right);
        assert_eq!(layout.column_chord(), "focus-h");
        // Admitted order is arbitrary: the tall anchor may be any member.
        let shifted = [
            frame(800.0, 0.0, 800.0, 450.0),
            frame(0.0, 0.0, 800.0, 900.0),
            frame(800.0, 450.0, 800.0, 450.0),
        ];
        let layout = classify_layout(&shifted).expect("anchor in the middle");
        assert_eq!(layout.tall, 1);
        assert_eq!((layout.above, layout.below), (0, 2));
    }

    #[test]
    fn layout_refuses_a_single_row_instead_of_faking_perpendicularity() {
        // Three landscape halves (one row): no tall anchor exists.
        let row = [
            frame(0.0, 0.0, 600.0, 900.0),
            frame(600.0, 0.0, 600.0, 900.0),
            frame(1200.0, 0.0, 600.0, 900.0),
        ];
        let err = classify_layout(&row).expect_err("single row refuses");
        assert!(err.contains("not one tall anchor"), "{err}");
        // Two tall frames refuse too.
        let two = [
            frame(0.0, 0.0, 800.0, 900.0),
            frame(800.0, 0.0, 800.0, 900.0),
            frame(800.0, 450.0, 800.0, 450.0),
        ];
        assert!(classify_layout(&two).is_err());
        // Wrong member count refuses.
        assert!(classify_layout(&row[..2]).is_err());
        // A stacked pair in a different column (not beside the anchor) refuses.
        let diagonal = [
            frame(0.0, 0.0, 800.0, 900.0),
            frame(900.0, 0.0, 400.0, 450.0),
            frame(1400.0, 450.0, 400.0, 450.0),
        ];
        assert!(classify_layout(&diagonal).is_err());
    }

    #[test]
    fn layout_refuses_overlapping_or_degenerate_column_members() {
        let overlap = [
            frame(0.0, 0.0, 800.0, 900.0),
            frame(800.0, 0.0, 800.0, 600.0),
            frame(800.0, 400.0, 800.0, 600.0),
        ];
        assert!(classify_layout(&overlap).is_err());
        let degenerate = [
            frame(0.0, 0.0, 800.0, 900.0),
            frame(800.0, 0.0, 0.0, 450.0),
            frame(800.0, 450.0, 800.0, 450.0),
        ];
        assert!(classify_layout(&degenerate).is_err());
    }

    #[test]
    fn cause_tokens_count_only_explicit_cause_fields() {
        let raw = "\
tiler-diag stage=focus stamp_us=1000 id=focus-1 cause=focus-follows-cursor target=100_7 observed=100_9 known=1 report=4 outcome=accepted provenance=retained
tiler-diag stage=focus stamp_us=1200 id=focus-2 cause=explicit-command target=100_9 observed=100_7 known=1 report=5 outcome=accepted provenance=retained
tiler-diag stage=focus stamp_us=1400 id=focus-3 cause=focus-follows-cursor target=100_9 observed=100_7 known=1 report=6 outcome=accepted provenance=retained
tiler-log seq=1 t=12ms pid=0 gen=0 ref=7 kind=diag-focus-follows outcome=accepted count=3
tiler-diag stage=frame stamp_us=1600 pid=100 wid=7 rev=3 entry=none completion=applied
";
        let tokens = cause_tokens(raw);
        assert_eq!(tokens.follows, 2);
        assert_eq!(tokens.follows_stamp_us, Some(1400));
        assert_eq!(tokens.explicit, 1);
        assert_eq!(tokens.explicit_stamp_us, Some(1200));
        // Empty and unrelated output count nothing (and stamp nothing).
        let empty = cause_tokens("");
        assert_eq!(empty, CauseTokens::default());
        assert_eq!(
            cause_tokens(
                "tiler-log seq=1 t=2ms pid=0 gen=0 ref=1 kind=diag-report outcome=fg-unknown count=0"
            ),
            CauseTokens::default()
        );
    }

    #[test]
    fn cause_json_reports_cumulative_counts_and_deltas() {
        let base = cause_tokens(
            "tiler-diag stage=focus stamp_us=10 id=a cause=focus-follows-cursor target=a observed=b known=1 report=1 outcome=accepted",
        );
        let now = cause_tokens(
            "tiler-diag stage=focus stamp_us=10 id=a cause=focus-follows-cursor target=a observed=b known=1 report=1 outcome=accepted\n\
             tiler-diag stage=focus stamp_us=20 id=b cause=explicit-command target=c observed=b known=1 report=2 outcome=accepted",
        );
        let value = cause_json(now, base);
        assert_eq!(value["focus-follows-cursor"]["count"], 1);
        assert_eq!(value["focus-follows-cursor"]["delta"], 0);
        assert_eq!(value["focus-follows-cursor"]["newest_stamp_us"], 10);
        assert_eq!(value["explicit-command"]["count"], 1);
        assert_eq!(value["explicit-command"]["delta"], 1);
        assert_eq!(value["explicit-command"]["newest_stamp_us"], 20);
    }
}
