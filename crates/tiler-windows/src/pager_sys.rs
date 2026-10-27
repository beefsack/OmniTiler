#![forbid(unsafe_code)]
//! Owned per-output taskbar pager overlays (`cfg(windows)` only).
//!
//! Mechanism: one process-owned external overlay window per served output
//! (class `OmniTilerTaskbarPager`), styled exactly like the validated probe:
//! `WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE` with `WS_POPUP |
//! WS_VISIBLE`, GDI-painted cells, `SetWindowPos(TOPMOST, NOACTIVATE)`.
//! The windows never activate (`WS_EX_NOACTIVATE` plus `MA_NOACTIVATE`),
//! skip Alt+Tab via `WS_EX_TOOLWINDOW`, and are positioned from physical
//! PMv2 coordinates (PMv2 is ensured before registration, like the loop).
//!
//! What this module never does: no injection or hooks into Explorer, no
//! AppBar registration, no placeholder or Start-button manipulation, no
//! taskbar resize, and no synthetic input. The single AppBar read is
//! `SHAppBarMessage(ABM_GETSTATE)`: a stateless auto-hide switch query, never
//! a registration or a state change. Clicks hit-test the stored
//! portable frame and stage the latest [`SelectScope`](crate::tray_pager)
//! for the cell; the loop thread drains the slot and dispatches through the
//! existing `workspace_do_select` resolver. WinEvent out-of-context delivery
//! is not used here at all: only app-local window messages (paint, click,
//! `TaskbarCreated`, display/DPI broadcasts).
//!
//! Native effects (owned windows, GDI paint, per-window frames, click and
//! calibration delivery, taskbar reads, calibration flags) live in
//! [`crate::ffi`]; this module holds only portable policy: the overlay
//! manager, the latest-click handoff, taskbar-fact mapping, and the
//! change-gated frame sync.
//!
//! Tray exclusion is structural: strips are refused unless they end 2px left
//! of the proven tray edge (primary `TrayNotifyWnd`, secondary an exact
//! tray/clock child edge or the bar's own right edge once its child set
//! proves no right-side shell region exists; see [`crate::pager`]).
//! Fullscreen/game safety is fail-closed: the host hides every strip on any
//! suspend veto, elevated foreground, or unreadable fact, and z-order is
//! reasserted only while the host reports safe.
//!
//! Logging is sparse lifecycle only (counts and reasons, never HWNDs,
//! paths, or titles); creation failures retry forever on a fixed delay.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ffi::{
    IconBlit, MiniSlot, PagerCalibrationSink, PagerCellFrame, PagerClick, PagerClickSink,
    PagerFrame, PagerWindow,
};
use crate::pager::{OutputPager, TaskbarFacts, TaskbarKind, pager_active_fill};
use crate::pager_icons::icon_rect_for_mini;
use crate::tray_pager::SelectScope;

type DynError = Box<dyn std::error::Error>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

/// Overlay window class (distinct from tray/border/underlay/preview so
/// residue audits count the carrier exactly). Owned by [`crate::ffi`];
/// re-exported here so audits keep one name.
pub use crate::ffi::PAGER_WINDOW_CLASS as PAGER_OVERLAY_CLASS;
/// Latest pending pager click: overlay windows overwrite this, the loop
/// thread drains it once. Only the newest click survives: a click rendered
/// under an old frame is stale and must never replay after a newer one.
pub type PendingClick = Arc<Mutex<Option<SelectScope>>>;
/// Topmost reassert cadence while safe (never over an unsafe turn): 2s keeps
/// the strip above later topmost windows without a `SetWindowPos` per pump.
/// No proven complete native event covers topmost/visibility loss.
const ZASSERT_INTERVAL: Duration = Duration::from_secs(2);
/// Creation-failure retry: fixed 5s, forever. A fixed delay (no backoff)
/// recovers promptly once Explorer returns while never busy-looping pumps.
const RETRY_SECS: u64 = 5;

/// Whether Explorer (or display/DPI calibration) asked for rediscovery.
#[must_use]
pub fn take_rediscover() -> bool {
    crate::ffi::take_pager_rediscover()
}

/// Whether a calibration broadcast asked for an immediate hide plus
/// revalidation (consumed once by the host).
#[must_use]
pub fn take_stale_hide() -> bool {
    crate::ffi::take_pager_stale_hide()
}

/// Stateless `ABS_AUTOHIDE` test for one `ABM_GETSTATE` result (native
/// query lives in [`crate::ffi`]).
pub use crate::ffi::pager_autohide_from_state as autohide_from_state;

/// Query the live auto-hide switch via `SHAppBarMessage(ABM_GETSTATE)`.
/// Read-only: `cbSize`-only `APPBARDATA`, no window handle, no callback, no
/// registration, never `ABM_SETSTATE`.
#[must_use]
pub fn taskbar_autohide_enabled() -> bool {
    autohide_from_state(crate::ffi::query_appbar_state())
}

/// Read the primary taskbar fresh: `Shell_TrayWnd` rect and visibility plus
/// the `TrayNotifyWnd` tray edge, the `ABM_GETSTATE` auto-hide switch, and
/// the live taskbar DPI. Cheap enough for every pump (five reads, no
/// enumeration); `None` when Explorer is gone or any geometry read fails,
/// which the host treats as unknown facts (hide, never lay
/// out from memory). 96 DPI is only the fallback when `GetDpiForWindow`
/// itself fails, never an assumption about the monitor. A failed
/// `ABM_GETSTATE` query reads as not-auto-hiding (fail-open on the switch
/// only): geometry/visibility refusals still fail closed, and a positive
/// auto-hide bit always refuses.
#[must_use]
pub fn read_primary_taskbar() -> Option<TaskbarFacts> {
    let probe = crate::ffi::read_primary_taskbar_probe()?;
    Some(TaskbarFacts {
        rect: probe.bar?,
        tray_left: probe.tray_left,
        kind: TaskbarKind::Primary,
        visible: probe.visible,
        autohide: autohide_from_state(probe.autohide_state),
        dpi: if probe.dpi == 0 {
            crate::pager::PAGER_BASE_DPI
        } else {
            probe.dpi
        },
    })
}

/// Owning Explorer process id behind the primary taskbar, for (re)scoping
/// the pid-filtered taskbar hooks at startup and on the rediscovery
/// broadcast only. `None` while Explorer is gone: the host installs no
/// scope then, so no queued edge can arm against a stale pid.
#[must_use]
pub fn primary_taskbar_pid() -> Option<u32> {
    crate::ffi::primary_taskbar_pid()
}

/// Enumerate secondary taskbars (`Shell_SecondaryTrayWnd`). Heavier than the
/// primary read, so the host calls this only on the stale-anchor flag
/// (pid-scoped Explorer window edge, rediscovery broadcast, or topology
/// change). Each entry carries its proven tray edge (an exact tray/clock child edge,
/// or the bar's own right edge once the child set proves no right-side shell
/// region exists); `None` refuses layout, never positions. DPI and the
/// auto-hide switch are read live like the primary path (same global switch,
/// per-bar DPI with the same 96 fallback).
#[must_use]
pub fn discover_secondaries() -> Vec<TaskbarFacts> {
    crate::ffi::discover_secondary_taskbars()
        .into_iter()
        .filter_map(|probe| {
            Some(TaskbarFacts {
                rect: probe.bar?,
                tray_left: probe.tray_left,
                kind: TaskbarKind::Secondary,
                visible: probe.visible,
                autohide: autohide_from_state(probe.autohide_state),
                dpi: if probe.dpi == 0 {
                    crate::pager::PAGER_BASE_DPI
                } else {
                    probe.dpi
                },
            })
        })
        .collect()
}

/// Verify Per-Monitor-V2 awareness before any overlay geometry (same
/// fail-closed contract as the loop's own check; duplicated here because the
/// loop's monitor module is not visible from the crate root).
fn ensure_pm_v2() -> Result<(), DynError> {
    if crate::ffi::thread_pm_v2_active() {
        return Ok(());
    }
    if !crate::ffi::set_process_pm_v2() {
        return Err(err("refuse: pager SetProcessDpiAwarenessContext failed"));
    }
    if !crate::ffi::thread_pm_v2_active() {
        return Err(err("refuse: pager PMv2 awareness not active"));
    }
    Ok(())
}

/// Convert one portable frame to the owned native snapshot. Artwork stays
/// shared (`Arc` clone, never re-buffered); icon destination slots use the
/// canonical miniature sizing, so illegible miniatures keep the plain
/// rectangle exactly as the painter always has. `active_fill` is the resolved
/// accent-or-fallback the painter carries for the active cell. Cell ordinals
/// ride the core values verbatim into paint and click attribution.
fn pager_frame(pager: &OutputPager, active_fill: (u8, u8, u8)) -> PagerFrame {
    PagerFrame {
        ids: pager.ids.clone(),
        output: pager.output.clone(),
        cells: pager
            .cells
            .iter()
            .map(|cell| PagerCellFrame {
                rect: cell.rect,
                ordinal: cell.ordinal,
            })
            .collect(),
        active: pager.active_index,
        active_fill,
        focused_highlight: pager.focused_highlight,
        minis: pager
            .cells
            .iter()
            .map(|cell| {
                cell.minis
                    .iter()
                    .zip(cell.mini_icons.iter())
                    .zip(cell.mini_focused.iter())
                    .map(|((mini, icon), focused)| MiniSlot {
                        rect: *mini,
                        focused: *focused,
                        icon: icon.as_ref().and_then(|image| {
                            icon_rect_for_mini(*mini, pager.dpi).map(|dest| IconBlit {
                                image: Arc::clone(image),
                                dest,
                            })
                        }),
                    })
                    .collect()
            })
            .collect(),
    }
}

/// Same field set the native store compared before: workspace ids, output,
/// cell rects and core ordinals, active index, miniatures, per-miniature
/// focus, the independent highlight colour, and artwork
/// (`Arc` equality short-circuits on pointer identity, so steady-state
/// artwork costs one pointer compare per miniature, not a byte walk).
/// Ordinal changes repaint because the painter formats the stored ordinal;
/// focus changes repaint because the painter styles the focused miniature;
/// highlight-colour changes repaint because the miniature border repaints.
fn frames_equal(a: &OutputPager, b: &OutputPager) -> bool {
    a.ids == b.ids
        && a.output == b.output
        && a.active_index == b.active_index
        && a.focused_highlight == b.focused_highlight
        && a.cells.len() == b.cells.len()
        && a.cells.iter().zip(b.cells.iter()).all(|(x, y)| {
            x.rect == y.rect
                && x.ordinal == y.ordinal
                && x.minis == y.minis
                && x.mini_focused == y.mini_focused
                && x.mini_icons == y.mini_icons
        })
}

/// Pure maintenance-deadline combination (windows are opaque values, so
/// tests drive this directly instead of fabricating ids).
fn maintenance_due(
    has_windows: bool,
    last_zassert: Option<Instant>,
    retry_at: Option<Instant>,
) -> Option<Instant> {
    let zassert = if has_windows {
        Some(
            last_zassert
                .map(|at| at + ZASSERT_INTERVAL)
                .unwrap_or_else(Instant::now),
        )
    } else {
        None
    };
    match (zassert, retry_at) {
        (None, None) => None,
        (Some(at), None) | (None, Some(at)) => Some(at),
        (Some(first), Some(second)) => Some(first.max(second)),
    }
}
/// Pure repaint decision: a geometry change or an accent-fill change
/// repaints (the frame carries the fill, so an accent change repaints even
/// when every cell is identical).
fn frame_needs_repaint(geometry_changed: bool, fill_changed: bool) -> bool {
    geometry_changed || fill_changed
}

/// Presented accent source: whether the fill resolved from an available
/// live accent, plus the RGB it produced. Both matter: a fallback
/// `#2a82da` and a live `#2a82da` accent paint identical pixels but are
/// different states (one must retry on broadcast, the other must not claim
/// success).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AccentSource {
    available: bool,
    fill: (u8, u8, u8),
}

/// Pending-vs-presented accent fill: `resolve_from` paints the already
/// observed shared owner snapshot and reports whether the current fill
/// still needs a present; the pending fill is only committed by
/// `note_presented` after every served strip actually received it, so a
/// creation-backoff early return can never swallow a repaint. Source rides
/// the state, so an equal-RGB source flip still presents (and logs) instead
/// of comparing equal. The gate never queries: the single shared owner
/// resolve owns the native call and its one diagnostic record.
#[derive(Debug, Default)]
struct AccentGate {
    presented: Option<AccentSource>,
}

impl AccentGate {
    /// Resolve the current source fill from the shared owner snapshot.
    /// Returns the source plus whether it still needs a present. Steady
    /// snapshots repeat silently through the presented comparison; a changed
    /// fill or source flip needs a present.
    fn resolve_from(
        &mut self,
        accent: Option<crate::active_border::SystemAccent>,
    ) -> (AccentSource, bool) {
        let current = AccentSource {
            available: crate::active_border::accent_available(accent),
            fill: pager_active_fill(accent),
        };
        let needs_present = self.presented != Some(current);
        (current, needs_present)
    }

    /// Commit after every served strip received `source`. Returns true on a
    /// genuine transition (for the one bounded log line), including the
    /// initial present (which documents the starting source).
    fn note_presented(&mut self, source: AccentSource) -> bool {
        let changed = self.presented != Some(source);
        self.presented = Some(source);
        changed
    }
}
/// Native pager visibility readback for transition-only observability:
/// actual visibility, the checked topmost-band bit, and the bounded
/// read-only z-order encounter against a fresh `Shell_TrayWnd`, counted
/// without HWNDs, PIDs, paths, or content. `taskbar_visible` is the fresh
/// primary probe verdict (`None` when unknown); it rides the signature so
/// a taskbar show/hide transition re-logs even when the pager counts
/// match. A same-band case (pager and taskbar both topmost) is resolved
/// only by the `above`/`below` encounter, never inferred from the band
/// bits; no encounter (or an absent taskbar) counts as `order_unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativePagerFacts {
    pub total: usize,
    pub visible: usize,
    pub topmost_true: usize,
    pub topmost_unknown: usize,
    pub taskbar_visible: Option<bool>,
    pub taskbar_topmost: Option<bool>,
    pub above: usize,
    pub below: usize,
    pub order_unknown: usize,
}

fn opt_word(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

/// Full change signature over the native pager readback: identical repeats
/// stay silent; any count, taskbar-visibility, taskbar-topmost, or
/// relative-encounter change re-logs.
#[must_use]
pub fn native_pager_signature(facts: NativePagerFacts) -> String {
    format!(
        "total={} visible={} topmost={} topmost_unknown={} taskbar={} taskbar_topmost={} above={} below={} order_unknown={}",
        facts.total,
        facts.visible,
        facts.topmost_true,
        facts.topmost_unknown,
        match facts.taskbar_visible {
            Some(true) => "visible",
            Some(false) => "hidden",
            None => "unknown",
        },
        opt_word(facts.taskbar_topmost),
        facts.above,
        facts.below,
        facts.order_unknown,
    )
}

/// Bounded trace-only `pager-wnd` line over the drained ordered native
/// batch. `events` is the exact drain order (sequence tells observed
/// order); `omitted` is the diagnostic-only overflow count. The tick plus
/// the production triple (request/observation/revision) and the
/// drain-context label ride the line for correlation only: queued OS
/// messages carry no causal request attribution, and an unchanged
/// presentation with live native traffic still emits. Fixed scalar shape
/// plus one bounded event array; no HWNDs, PIDs, paths, titles, content,
/// or error strings.
#[must_use]
pub fn pager_trace_value(
    events: &[crate::ffi::pager_trace::PagerTraceEvent],
    omitted: u64,
    tick: u64,
    request: &str,
    observation: u64,
    revision: u64,
    context: &str,
) -> serde_json::Value {
    serde_json::json!({
        "event": "pager-wnd",
        "tick": tick,
        "request": request,
        "observation": observation,
        "revision": revision,
        "context": context,
        "omitted": omitted,
        "events": events.iter().map(|event| event.to_json()).collect::<Vec<_>>(),
    })
}

/// Drain the thread-local ordered batch and append one bounded `pager-wnd`
/// line when the batch (or its overflow count) is non-empty. Returns true
/// when a line was emitted. Trace-gated: disabled tracing captured
/// nothing, so the drain is a no-op with no allocation and no native
/// query. Callers drain on every pager exit (suppressed, retry-wait,
/// owner-failed, safe) so OS-observed traffic never waits for the next
/// safe turn.
pub fn emit_pager_trace(
    log_path: &Path,
    trace: bool,
    tick: u64,
    request: &str,
    observation: u64,
    revision: u64,
    context: &'static str,
) -> bool {
    if !trace {
        return false;
    }
    let (events, omitted) = crate::ffi::pager_trace::drain();
    if events.is_empty() && omitted == 0 {
        return false;
    }
    PagerOverlays::write_line(
        Some(log_path.to_owned()).as_ref(),
        pager_trace_value(
            &events,
            omitted,
            tick,
            request,
            observation,
            revision,
            context,
        ),
    );
    true
}

/// Live owner pager: one overlay window per served output plus the shared
/// latest-click slot the loop thread drains. The slot is owned by the loop
/// (so suppression can drop it with no windows present); each window carries
/// a sink clone for its click path.
pub struct PagerOverlays {
    windows: HashMap<String, PagerWindow>,
    frames: HashMap<String, OutputPager>,
    sink: PagerClickSink,
    calibration: PagerCalibrationSink,
    log_path: Option<PathBuf>,
    retry_at: Option<Instant>,
    last_zassert: Option<Instant>,
    last_hash: u64,
    /// Pending-vs-presented accent fill (cached query plus committed
    /// source): refreshed only when the shared version moved, committed
    /// only after every served strip received it, so steady-state pumps
    /// perform no native query and a creation backoff never swallows a
    /// repaint. The pager always follows the accent (no theme setting).
    gate: AccentGate,
    /// Last emitted native visibility signature for transition-only
    /// `pager-native` logging (see `emit_native_state`). `None` until the
    /// first served show; cleared when no overlays remain so the next show
    /// re-baselines.
    last_native: Option<String>,
}

impl PagerOverlays {
    /// Register the overlay class and the `TaskbarCreated` broadcast.
    /// PMv2 must precede overlay creation; this ensures it first (the loop
    /// already did, so this is normally a no-op verification). `queue` is
    /// the loop-owned latest-click slot every window overwrites.
    pub fn new(log_path: Option<PathBuf>, queue: &PendingClick) -> Result<Self, DynError> {
        ensure_pm_v2()?;
        if !crate::ffi::ensure_pager_class() {
            return Err(err("error: pager RegisterClass failed"));
        }
        if !crate::ffi::register_taskbar_message(crate::tray::TASKBAR_CREATED_MESSAGE) {
            return Err(err("error: pager TaskbarCreated register failed"));
        }
        // Keep-latest only: overwrite any undrained click so a stale
        // cell never replays after a newer one.
        let clicks = Arc::clone(queue);
        let sink: PagerClickSink = Arc::new(move |click: PagerClick| {
            if let Ok(mut slot) = clicks.lock() {
                *slot = Some(SelectScope::new(
                    &click.output,
                    &click.workspace,
                    click.ordinal,
                ));
            }
        });
        // Calibration hook at broadcast time (the exact monitor-dirty
        // timing the owner has always relied on).
        let calibration: PagerCalibrationSink = Arc::new(crate::tiling_sys::mark_monitor_dirty);
        Ok(Self {
            windows: HashMap::new(),
            frames: HashMap::new(),
            sink,
            calibration,
            log_path,
            retry_at: None,
            last_zassert: None,
            last_hash: 0,
            gate: AccentGate::default(),
            last_native: None,
        })
    }

    fn log_line(&self, value: serde_json::Value) {
        Self::write_line(self.log_path.as_ref(), value);
    }

    /// Lock-free best-effort JSON line append (create/append, single row).
    /// Ignores all errors; never touches Win32 or channel locks.
    fn write_line(path: Option<&PathBuf>, value: serde_json::Value) {
        use std::io::Write;
        if let Some(path) = path
            && let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
        {
            let _ = writeln!(file, "{value}");
        }
    }

    fn retry_due(&self, now: Instant) -> bool {
        self.retry_at.is_none_or(|at| now >= at)
    }

    fn note_failure(&mut self, now: Instant) {
        self.retry_at = Some(now + Duration::from_secs(RETRY_SECS));
    }

    /// Next pager maintenance due time for the owner's event-driven wait,
    /// or `None` when nothing is outstanding. Covers both the
    /// creation-retry backoff (`retry_at`, including partial failures with
    /// existing windows and total failures with none) and the topmost
    /// reassert (`last_zassert` plus the interval, only with overlay
    /// windows to reassert). Taking the maximum prevents a past-due
    /// zassert from spinning zero-timeout turns while a creation backoff
    /// holds `sync` in early-return: after the backoff the combined
    /// deadline wakes the retry, and the reassert follows on that same
    /// turn, preserving the 2s z-order contract after backoff.
    pub fn maintenance_due_at(&self) -> Option<Instant> {
        maintenance_due(!self.windows.is_empty(), self.last_zassert, self.retry_at)
    }

    /// Transition-only native readback log over already-collected facts:
    /// logs one `pager-native` line per signature change (baseline,
    /// change, and recovery included; identical repeats stay silent) and
    /// returns true when a line was emitted. `stage` names the call site
    /// (`shown` after real show effects, `reasserted` after the existing
    /// topmost recovery) without entering the signature, so equal native
    /// state across stages stays silent. Counts and bounded words only;
    /// never HWNDs, PIDs, paths, titles, or content. This is the offline
    /// test seam: unit tests script `facts` directly and assert on the
    /// real log file, proving baseline/change/repeat silence/recovery and
    /// the honest unknown legs without native windows.
    fn emit_facts(&mut self, stage: &'static str, facts: NativePagerFacts) -> bool {
        let signature = native_pager_signature(facts);
        if self.last_native.as_deref() == Some(signature.as_str()) {
            return false;
        }
        self.last_native = Some(signature);
        let mut line = serde_json::json!({
            "event": "pager-native",
            "stage": stage,
            "outputs": facts.total,
            "visible": facts.visible,
            "topmost": facts.topmost_true,
            "topmost_unknown": facts.topmost_unknown,
            "taskbar_topmost": opt_word(facts.taskbar_topmost),
            "above": facts.above,
            "below": facts.below,
            "order_unknown": facts.order_unknown,
        });
        match facts.taskbar_visible {
            Some(taskbar) => line["taskbar_visible"] = serde_json::Value::from(taskbar),
            None => line["taskbar_visible"] = serde_json::Value::from("unknown"),
        }
        self.log_line(line);
        true
    }

    /// Native probe plus transition-only log: collects actual visibility,
    /// the checked topmost bit, and the fresh taskbar-relative encounter
    /// for every overlay, then logs via [`Self::emit_facts`]. Callers
    /// invoke this only after real show effects and on the existing
    /// topmost-recovery path, never on unchanged syncs, so no new polling
    /// is added. Empty windows reset the baseline silently (no log) so the
    /// next show re-baselines.
    fn emit_native_state(&mut self, stage: &'static str, taskbar_visible: Option<bool>) {
        if self.windows.is_empty() {
            self.last_native = None;
            return;
        }
        let mut visible = 0usize;
        let mut topmost_true = 0usize;
        let mut topmost_unknown = 0usize;
        let mut above = 0usize;
        let mut below = 0usize;
        let mut order_unknown = 0usize;
        let mut taskbar_topmost: Option<bool> = None;
        let mut taskbar_seen = false;
        for window in self.windows.values() {
            let (is_visible, topmost) = window.probe_native();
            visible += usize::from(is_visible);
            match topmost {
                Some(true) => topmost_true += 1,
                Some(false) => {}
                None => topmost_unknown += 1,
            }
            let order = window.taskbar_order();
            if !taskbar_seen {
                taskbar_topmost = order.taskbar_topmost;
                taskbar_seen = true;
            } else if taskbar_topmost != order.taskbar_topmost {
                // Inconsistent taskbar reads across overlays in one pass:
                // stay honest and report unknown rather than the first.
                taskbar_topmost = None;
            }
            match order.pager_above_taskbar {
                Some(true) => above += 1,
                Some(false) => below += 1,
                None => order_unknown += 1,
            }
        }
        self.emit_facts(
            stage,
            NativePagerFacts {
                total: self.windows.len(),
                visible,
                topmost_true,
                topmost_unknown,
                taskbar_visible,
                taskbar_topmost,
                above,
                below,
                order_unknown,
            },
        );
    }

    /// Present one pump's frames: create/move/show served outputs, destroy
    /// departed ones, repaint on frame change, and reassert topmost only on
    /// the safe cadence. Creation failures retry on a fixed delay and
    /// retry forever. Steady-state visibility reads stay (correctness over
    /// syscall counting); moves and repaints are change-gated. `accent` is
    /// the already-observed shared owner snapshot for the current broadcast
    /// version (plain data, never a query): the single shared owner resolve
    /// owns the native call and its one diagnostic record. `trace`
    /// enables one trace-only bracket per actual `PagerWindow::create`
    /// (including DPI recreates and retries, which run with no accent
    /// resolve); non-trace runs stay silent outside the change-gated
    /// vocabulary. `taskbar_visible` is the fresh primary probe verdict
    /// (`None` when unknown) for the native readback line only.
    pub fn sync(
        &mut self,
        pagers: &[OutputPager],
        now: Instant,
        trace: bool,
        accent: Option<crate::active_border::SystemAccent>,
        taskbar_visible: Option<bool>,
    ) {
        let hash = crate::pager::snapshot_hash(pagers);
        // Drop overlays for outputs no longer served.
        let live: Vec<String> = pagers.iter().map(|pager| pager.output.clone()).collect();
        let gone: Vec<String> = self
            .windows
            .keys()
            .filter(|output| !live.contains(output))
            .cloned()
            .collect();
        for output in gone {
            self.windows.remove(&output);
            self.frames.remove(&output);
        }
        if !self.retry_due(now) {
            return;
        }
        // Paint the shared owner snapshot: no native resolve here. The
        // pending fill commits only after every served strip received it
        // below, so the creation-backoff early return above can never
        // swallow a repaint; hidden strips keep their windows and cache,
        // so a change during suppression still repaints on reshow.
        let (source, needs_present) = self.gate.resolve_from(accent);
        let mut created = 0u32;
        let mut failed = 0u32;
        // Actual show effects this pump: creation, a change-gated
        // move/show, or a hidden-to-shown transition. The native readback
        // below probes only when at least one effect ran, so steady-state
        // syncs add no native queries and no polling.
        let mut showed = false;
        for pager in pagers {
            // A DPI change recreates the overlay so the font and geometry
            // match the new facts; anything else reuses the window.
            if self
                .windows
                .get(&pager.output)
                .is_some_and(|window| window.dpi() != pager.dpi)
            {
                self.windows.remove(&pager.output);
                self.frames.remove(&pager.output);
            }
            let mut fresh = false;
            if !self.windows.contains_key(&pager.output) {
                // Trace-only per-create bracket: marks the actual native
                // create attempt and its outcome, so a death between them
                // reads as `start` without `done`. Fires for every real
                // create (first show, DPI recreate, retry after failure),
                // including turns with no accent query. No new native calls.
                if trace {
                    self.log_line(serde_json::json!({
                        "event": "pager-create",
                        "stage": "start",
                    }));
                }
                match PagerWindow::create(
                    pager.strip,
                    pager_frame(pager, source.fill),
                    pager.dpi,
                    &self.sink,
                    &self.calibration,
                ) {
                    Some(window) => {
                        self.windows.insert(pager.output.clone(), window);
                        self.frames.insert(pager.output.clone(), pager.clone());
                        created += 1;
                        fresh = true;
                        if trace {
                            self.log_line(serde_json::json!({
                                "event": "pager-create",
                                "stage": "done",
                                "outcome": "created",
                            }));
                        }
                    }
                    None => {
                        failed += 1;
                        if trace {
                            self.log_line(serde_json::json!({
                                "event": "pager-create",
                                "stage": "done",
                                "outcome": "create-failed",
                            }));
                        }
                        continue;
                    }
                }
            }
            let Some(window) = self.windows.get_mut(&pager.output) else {
                failed += 1;
                continue;
            };
            let geometry_changed = !self
                .frames
                .get(&pager.output)
                .is_some_and(|last| frames_equal(last, pager));
            // An accent-source change repaints even when every cell is
            // identical: the frame carries the fill the painter reads.
            let frame_changed = frame_needs_repaint(geometry_changed, needs_present);
            if frame_changed {
                window.set_frame(pager_frame(pager, source.fill));
                self.frames.insert(pager.output.clone(), pager.clone());
            }
            if frame_changed || fresh {
                window.move_show(pager.strip);
                showed = true;
            }
            if window.ensure_shown() {
                showed = true;
            }
        }
        if failed > 0 {
            self.note_failure(now);
            self.log_line(serde_json::json!({
                "event": "pager-window",
                "outcome": "create-failed",
                "failed": failed,
                "retry_secs": self.retry_at.map(|at| at.saturating_duration_since(now).as_secs()),
            }));
        } else {
            self.retry_at = None;
            // Commit the pending fill only after every served strip
            // received it: a partial failure keeps the demand for the retry
            // turn instead of swallowing the repaint. One bounded line per
            // genuine transition (including the initial source); a fallback
            // names its cause, never app content.
            if self.gate.note_presented(source) {
                let mut event = serde_json::json!({
                    "event": "pager-accent",
                    "outcome": if source.available { "accent" } else { "fallback" },
                    "color": crate::active_border::render_color(source.fill),
                });
                if !source.available {
                    event["reason"] = serde_json::Value::from("accent-unavailable");
                }
                self.log_line(event);
            }
            if created > 0 {
                self.log_line(serde_json::json!({
                    "event": "pager-window",
                    "outcome": "created",
                    "outputs": created,
                }));
            }
        }
        // Periodic topmost reassert, safe-cadence only: the host calls sync
        // only while the gate is open, so reaching here means no fullscreen,
        // elevation, or unknown fact holds this turn. Native readback logs
        // after real show effects above and again after the existing
        // recovery below, transition-only in both places with no new timer
        // or poll.
        if showed {
            self.emit_native_state("shown", taskbar_visible);
        }
        if self
            .last_zassert
            .is_none_or(|at| now.duration_since(at) >= ZASSERT_INTERVAL)
        {
            self.last_zassert = Some(now);
            for window in self.windows.values() {
                window.reassert_topmost();
            }
            self.emit_native_state("reasserted", taskbar_visible);
        }
        self.last_hash = hash;
    }

    /// Hide every strip (suppression turns): windows persist for the next
    /// safe turn, so no destroy/create churn hides information. Resets the
    /// native-readback baseline silently (no log) so the next show after
    /// suppression re-baselines with fresh observability instead of
    /// comparing against pre-hide counts.
    pub fn hide_all(&mut self) {
        for window in self.windows.values() {
            window.hide();
        }
        self.last_native = None;
    }
}

impl Drop for PagerOverlays {
    /// Plain RAII teardown: every overlay destroyed exactly once by its
    /// window value. Process exit destroys them implicitly (no residue).
    fn drop(&mut self) {
        self.windows.clear();
        self.frames.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_guards::WindowKey;
    use crate::pager_icons::IconImage;
    use tiler_core::geometry::Rect;

    fn empty_overlays() -> PagerOverlays {
        PagerOverlays {
            windows: HashMap::new(),
            frames: HashMap::new(),
            sink: Arc::new(|_click: PagerClick| {}),
            calibration: Arc::new(|| {}),
            log_path: None,
            retry_at: None,
            last_zassert: None,
            last_hash: 0,
            gate: AccentGate::default(),
            last_native: None,
        }
    }

    fn pager_fixture(ids: &[&str], active: usize) -> OutputPager {
        use crate::pager::PagerCell;
        OutputPager {
            output: "mon-a".to_owned(),
            strip: Rect {
                x: 0,
                y: 0,
                w: 144,
                h: 40,
            },
            ids: ids.iter().map(|id| id.to_string()).collect(),
            cells: ids
                .iter()
                .enumerate()
                .map(|(index, _)| PagerCell {
                    rect: Rect {
                        x: index as i32 * 72,
                        y: 0,
                        w: 72,
                        h: 40,
                    },
                    active: index == active,
                    ordinal: index as u32 + 1,
                    minis: Vec::new(),
                    mini_focused: Vec::new(),
                    mini_keys: Vec::new(),
                    mini_sources: Vec::new(),
                    mini_icons: Vec::new(),
                })
                .collect(),
            active_index: active,
            focused_highlight: (0xbd, 0x93, 0xf9),
            dpi: 96,
        }
    }

    #[test]
    fn autohide_bit_is_the_only_hide_signal() {
        use windows_sys::Win32::UI::Shell::ABS_AUTOHIDE;
        assert!(!autohide_from_state(0), "failed query never hides");
        assert!(autohide_from_state(ABS_AUTOHIDE as usize));
        assert!(autohide_from_state(u32::MAX as usize));
        assert!(
            !autohide_from_state(0x02),
            "unrelated state bits never hide"
        );
    }

    #[test]
    fn frame_gate_compares_ids_cells_and_artwork() {
        let base = pager_fixture(&["ws-a", "ws-b"], 0);
        assert!(frames_equal(&base, &base.clone()));
        // Active move, id change, and geometry change all gate a repaint.
        let mut moved = base.clone();
        moved.active_index = 1;
        assert!(!frames_equal(&base, &moved));
        let mut renamed = base.clone();
        renamed.ids[0] = "ws-x".to_owned();
        assert!(!frames_equal(&base, &renamed));
        let mut resized = base.clone();
        resized.cells[0].rect.w = 73;
        assert!(!frames_equal(&base, &resized));
        // Ordinal change alone gates a repaint: the painter formats the
        // stored ordinal, so a renumber is visible pixels.
        let mut renumbered = base.clone();
        renumbered.cells[1].ordinal = 7;
        assert!(!frames_equal(&base, &renumbered));
        assert!(frames_equal(&renumbered, &renumbered.clone()));
        // Artwork arrival (shared, not copied) gates a repaint; steady
        // state with the same `Arc` does not.
        let image = Arc::new(IconImage::new(32, 32, vec![4; 32 * 32 * 4]));
        let mut iconic = base.clone();
        iconic.cells[0].mini_icons.push(Some(Arc::clone(&image)));
        iconic.cells[0].mini_focused.push(false);
        iconic.cells[0].minis.push(Rect {
            x: 1,
            y: 1,
            w: 10,
            h: 10,
        });
        iconic.cells[0].mini_keys.push(WindowKey {
            hwnd: 9,
            pid: 7,
            creation: "c1".to_owned(),
        });
        assert!(!frames_equal(&base, &iconic));
        assert!(frames_equal(&iconic, &iconic.clone()));
    }

    #[test]
    fn frame_gate_compares_miniature_focus() {
        use crate::pager::PagerContentMiniature;
        let miniature = PagerContentMiniature {
            key: WindowKey {
                hwnd: 9,
                pid: 7,
                creation: "c1".to_owned(),
            },
            rect: Rect {
                x: 1,
                y: 1,
                w: 10,
                h: 10,
            },
            focused: false,
        };
        let frame_of = |focused: bool| {
            let mut pager = pager_fixture(&["ws-a"], 0);
            pager.cells[0].minis.push(miniature.rect);
            pager.cells[0].mini_focused.push(focused);
            pager.cells[0].mini_keys.push(miniature.key.clone());
            pager.cells[0].mini_icons.push(None);
            pager
        };
        let plain = frame_of(false);
        assert!(frames_equal(&plain, &frame_of(false)));
        // Identical geometry with a moved focus gates a repaint, and the
        // native frame carries the flag plus the independent highlight
        // colour the painter styles.
        let focused = frame_of(true);
        assert!(!frames_equal(&plain, &focused));
        let native = pager_frame(&focused, (0xaa, 0xbb, 0xcc));
        assert!(native.minis[0][0].focused);
        assert_eq!(native.focused_highlight, (0xbd, 0x93, 0xf9));
        assert_eq!(
            crate::pager::pager_dim_accent(native.focused_highlight),
            (76, 59, 100)
        );
        assert!(!pager_frame(&plain, (0xaa, 0xbb, 0xcc)).minis[0][0].focused);
        // A highlight-colour change with identical geometry also gates: the
        // miniature border repaints even when the cell accent is steady.
        let mut rehighlighted = focused.clone();
        rehighlighted.focused_highlight = (0x4c, 0x4a, 0x48);
        assert!(!frames_equal(&focused, &rehighlighted));
    }

    #[test]
    fn native_frame_shares_artwork_without_copying() {
        // `pager_frame` hands the worker `Arc` straight through: the
        // pointer the painter blits is the cache's own allocation.
        let image = Arc::new(IconImage::new(32, 32, vec![4; 32 * 32 * 4]));
        let mut pager = pager_fixture(&["ws-a"], 0);
        pager.cells[0].mini_icons.push(Some(Arc::clone(&image)));
        pager.cells[0].mini_focused.push(false);
        pager.cells[0].minis.push(Rect {
            x: 1,
            y: 1,
            w: 20,
            h: 20,
        });
        let frame = pager_frame(&pager, (0xaa, 0xbb, 0xcc));
        assert_eq!(frame.active_fill, (0xaa, 0xbb, 0xcc));
        let slot = frame.minis[0][0].icon.as_ref().expect("legible mini blits");
        assert!(Arc::ptr_eq(&slot.image, &image));
    }

    #[test]
    fn native_frame_carries_core_ordinals_verbatim() {
        // Non-positional core ordinals reach the owned native frame
        // verbatim: the painter and click attribution read the stored
        // ordinal, never the cell position.
        let mut pager = pager_fixture(&["ws-a", "ws-b"], 1);
        pager.cells[0].ordinal = 7;
        pager.cells[1].ordinal = 3;
        let frame = pager_frame(&pager, (0x2a, 0x82, 0xda));
        assert_eq!(frame.cells.len(), 2);
        assert_eq!(frame.cells[0].ordinal, 7);
        assert_eq!(frame.cells[1].ordinal, 3);
        assert_eq!(frame.active, 1);
    }

    #[test]
    fn accent_fill_change_repaints_without_geometry_change() {
        // The change-gate must not swallow an accent change: identical
        // geometry with a new fill still repaints, while steady state stays
        // quiet and a geometry change repaints regardless of the fill.
        assert!(frame_needs_repaint(false, true));
        assert!(frame_needs_repaint(true, false));
        assert!(frame_needs_repaint(true, true));
        assert!(!frame_needs_repaint(false, false));
        // The fill rides the native frame, so the painter sees the accent
        // (or the fallback) without any geometry delta.
        let pager = pager_fixture(&["ws-a"], 0);
        assert_eq!(
            pager_frame(&pager, (0x2a, 0x82, 0xda)).active_fill,
            (0x2a, 0x82, 0xda)
        );
    }

    #[test]
    fn accent_gate_retains_pending_fill_across_backoff() {
        use crate::active_border::SystemAccent;
        let live_accent = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        let mut gate = AccentGate::default();
        // Nothing presented yet: the first snapshot needs a present.
        let (first, needs) = gate.resolve_from(Some(live_accent));
        assert!(needs);
        assert!(first.available);
        assert_eq!(first.fill, (0xaa, 0xbb, 0xcc));
        // Simulated creation-backoff early return: no commit. Re-resolving
        // the same snapshot still needs the present - the backoff cannot
        // swallow the repaint.
        let (again, still_needs) = gate.resolve_from(Some(live_accent));
        assert!(still_needs, "backoff must not swallow the pending repaint");
        assert_eq!(again, first);
        // Commit (every served strip received it) clears the demand...
        assert!(gate.note_presented(first));
        let (_, clean) = gate.resolve_from(Some(live_accent));
        assert!(!clean);
        // ...until the snapshot actually moves (new colour).
        let other = SystemAccent {
            color: (0x11, 0x22, 0x33),
            alpha: 0xC4,
        };
        let (second, moved) = gate.resolve_from(Some(other));
        assert!(moved);
        assert_eq!(second.fill, (0x11, 0x22, 0x33));
        assert!(gate.note_presented(second));
        let (_, clean_again) = gate.resolve_from(Some(other));
        assert!(!clean_again);
    }

    #[test]
    fn shared_snapshot_resolves_without_stages_or_queries() {
        use crate::active_border::SystemAccent;
        // The gate consumes an already-observed snapshot: no version, no
        // query, no stage observer, no diagnostic - just the fill and its
        // source. The single shared owner resolve owns all of that.
        let live_accent = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        let mut gate = AccentGate::default();
        let (source, needs) = gate.resolve_from(Some(live_accent));
        assert!(needs);
        assert!(source.available);
        assert_eq!(source.fill, (0xaa, 0xbb, 0xcc));
        assert!(gate.note_presented(source));
        let (_, clean) = gate.resolve_from(Some(live_accent));
        assert!(!clean);
        // An unavailable snapshot falls back without claiming the accent.
        let (fallback, needs_fallback) = gate.resolve_from(None);
        assert!(needs_fallback);
        assert!(!fallback.available);
        assert_eq!(fallback.fill, (0x2a, 0x82, 0xda));
    }

    #[test]
    fn accent_gate_tracks_source_not_just_rgb() {
        use crate::active_border::SystemAccent;
        // A fallback `#2a82da` and a live `#2a82da` accent paint identical
        // pixels but are different states: the flip must present (and log)
        // rather than compare equal.
        let mut gate = AccentGate::default();
        let (fallback, needs) = gate.resolve_from(None);
        assert!(needs);
        assert!(!fallback.available);
        assert_eq!(fallback.fill, (0x2a, 0x82, 0xda));
        assert!(gate.note_presented(fallback));
        let live_blue = SystemAccent {
            color: (0x2a, 0x82, 0xda),
            alpha: 0xC4,
        };
        let (live, needs_live) = gate.resolve_from(Some(live_blue));
        assert!(needs_live, "equal RGB with a new source must still present");
        assert!(live.available);
        assert_eq!(live.fill, (0x2a, 0x82, 0xda));
        assert!(gate.note_presented(live));
        // Zero-alpha is unavailable everywhere: losing the live accent is a
        // new state even though the fallback pixels match the old fill.
        let ghost = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0,
        };
        let (gone, needs_gone) = gate.resolve_from(Some(ghost));
        assert!(needs_gone);
        assert!(!gone.available);
        assert_eq!(gone.fill, (0x2a, 0x82, 0xda));
        assert!(gate.note_presented(gone));
        let (_, clean) = gate.resolve_from(Some(ghost));
        assert!(!clean);
    }

    #[test]
    fn creation_retry_stays_fixed_without_backoff() {
        let mut overlays = empty_overlays();
        assert!(overlays.retry_due(Instant::now()));
        // Repeated failures re-arm exactly 5s out each time: no growth.
        for _ in 0..6 {
            let now = Instant::now();
            overlays.note_failure(now);
            assert_eq!(
                overlays.retry_at,
                Some(now + Duration::from_secs(5)),
                "retry must stay fixed at 5s"
            );
            assert!(!overlays.retry_due(now));
            assert!(overlays.retry_due(now + Duration::from_secs(5)));
        }
    }

    #[test]
    fn maintenance_due_combines_retry_and_zassert_without_spin() {
        let now = Instant::now();
        // No overlay windows: no deadline (hidden/suppressed pager never
        // polls for z-order recovery).
        assert_eq!(maintenance_due(false, None, None), None);
        // Served windows with no reassert yet: due immediately so the
        // first sync establishes the cadence.
        assert!(
            maintenance_due(true, None, None).is_some_and(|due| due <= Instant::now()),
            "first reassert is due at once"
        );
        // After a reassert the next due time is one interval out.
        assert_eq!(
            maintenance_due(true, Some(now), None),
            Some(now + Duration::from_secs(2))
        );
        // Failed creation arms the backoff even with no zassert arm: the
        // retry still wakes the owner (no strand while windows are absent).
        assert_eq!(
            maintenance_due(false, None, Some(now + Duration::from_secs(5))),
            Some(now + Duration::from_secs(5))
        );
        // Past-due zassert under an active backoff: the backoff wins (no
        // zero-timeout spin while `sync` early-returns), and the reassert
        // follows on the retry turn.
        assert_eq!(
            maintenance_due(
                true,
                Some(now - Duration::from_secs(60)),
                Some(now + Duration::from_secs(5)),
            ),
            Some(now + Duration::from_secs(5)),
            "backoff suppresses the past-due zassert"
        );
    }

    #[test]
    fn native_signature_baseline_change_and_repeat_silence() {
        // Baseline facts produce a stable signature (first show logs once).
        let baseline = NativePagerFacts {
            total: 1,
            visible: 1,
            topmost_true: 1,
            topmost_unknown: 0,
            taskbar_visible: Some(true),
            taskbar_topmost: Some(true),
            above: 1,
            below: 0,
            order_unknown: 0,
        };
        let sig = native_pager_signature(baseline);
        assert_eq!(sig, native_pager_signature(baseline));
        // Any count change re-logs (visibility loss, topmost loss or
        // unknown read, added output), while identical repeats stay silent
        // via equality.
        let hidden = NativePagerFacts {
            visible: 0,
            ..baseline
        };
        assert_ne!(native_pager_signature(hidden), sig);
        let untopmost = NativePagerFacts {
            topmost_true: 0,
            ..baseline
        };
        assert_ne!(native_pager_signature(untopmost), sig);
        let unreadable = NativePagerFacts {
            topmost_true: 0,
            topmost_unknown: 1,
            ..baseline
        };
        assert_ne!(native_pager_signature(unreadable), sig);
        // Taskbar-relative legs: the same pager counts with a hidden or
        // unknown taskbar, a changed taskbar-topmost bit, or a lost
        // above/below encounter are different states (no invented order).
        let bar_hidden = NativePagerFacts {
            taskbar_visible: Some(false),
            ..baseline
        };
        assert_ne!(native_pager_signature(bar_hidden), sig);
        let bar_unknown = NativePagerFacts {
            taskbar_visible: None,
            ..baseline
        };
        assert_ne!(native_pager_signature(bar_unknown), sig);
        assert_ne!(
            native_pager_signature(bar_hidden),
            native_pager_signature(bar_unknown)
        );
        let bar_band = NativePagerFacts {
            taskbar_topmost: Some(false),
            ..baseline
        };
        assert_ne!(native_pager_signature(bar_band), sig);
        let fell_below = NativePagerFacts {
            above: 0,
            below: 1,
            ..baseline
        };
        assert_ne!(native_pager_signature(fell_below), sig);
        let lost_order = NativePagerFacts {
            above: 0,
            below: 0,
            order_unknown: 1,
            taskbar_topmost: None,
            ..baseline
        };
        assert_ne!(native_pager_signature(lost_order), sig);
        // Privacy: counts and bounded words only, never handles or paths.
        assert!(!sig.contains("0x"));
        assert!(!sig.contains('\\'));
    }

    fn native_log_overlays(log: &std::path::Path) -> PagerOverlays {
        PagerOverlays {
            windows: HashMap::new(),
            frames: HashMap::new(),
            sink: Arc::new(|_click: PagerClick| {}),
            calibration: Arc::new(|| {}),
            log_path: Some(log.to_owned()),
            retry_at: None,
            last_zassert: None,
            last_hash: 0,
            gate: AccentGate::default(),
            last_native: None,
        }
    }

    fn native_log_rig() -> (PagerOverlays, std::path::PathBuf, std::path::PathBuf) {
        static NATIVE_LOG_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = NATIVE_LOG_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tiler-pager-native-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            seq
        ));
        std::fs::create_dir_all(&dir).expect("native log dir");
        let log = dir.join("run.log");
        let overlays = native_log_overlays(&log);
        (overlays, dir, log)
    }

    fn native_log_lines(log: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn shown_facts() -> NativePagerFacts {
        NativePagerFacts {
            total: 1,
            visible: 1,
            topmost_true: 1,
            topmost_unknown: 0,
            taskbar_visible: Some(true),
            taskbar_topmost: Some(true),
            above: 1,
            below: 0,
            order_unknown: 0,
        }
    }

    #[test]
    fn native_emit_baseline_change_repeat_recovery() {
        // The production `emit_facts` helper writes real `pager-native`
        // lines: baseline logs once, identical repeats stay silent, a
        // changed readback re-logs, and returning to the baseline recovers
        // with a third line. Fully offline via scripted facts.
        let (mut overlays, _dir, log) = native_log_rig();
        let baseline = shown_facts();
        assert!(overlays.emit_facts("shown", baseline));
        let lines = native_log_lines(&log);
        assert_eq!(lines.len(), 1, "baseline logs: {lines:?}");
        assert!(
            lines[0].contains("\"event\":\"pager-native\""),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("\"stage\":\"shown\""), "{}", lines[0]);
        assert!(lines[0].contains("\"outputs\":1"), "{}", lines[0]);
        assert!(lines[0].contains("\"visible\":1"), "{}", lines[0]);
        assert!(lines[0].contains("\"above\":1"), "{}", lines[0]);
        assert!(lines[0].contains("\"below\":0"), "{}", lines[0]);
        assert!(
            lines[0].contains("\"taskbar_topmost\":\"true\""),
            "{}",
            lines[0]
        );
        // Identical repeat emits nothing.
        assert!(!overlays.emit_facts("shown", baseline));
        assert!(!overlays.emit_facts("reasserted", baseline));
        assert_eq!(native_log_lines(&log).len(), 1);
        // Changed readback (visibility lost, fell below the taskbar)
        // re-logs under the recovery stage.
        let changed = NativePagerFacts {
            visible: 0,
            above: 0,
            below: 1,
            ..baseline
        };
        assert!(overlays.emit_facts("reasserted", changed));
        let lines = native_log_lines(&log);
        assert_eq!(lines.len(), 2, "change re-logs: {lines:?}");
        assert!(
            lines[1].contains("\"stage\":\"reasserted\""),
            "{}",
            lines[1]
        );
        assert!(lines[1].contains("\"visible\":0"), "{}", lines[1]);
        assert!(lines[1].contains("\"below\":1"), "{}", lines[1]);
        // Recovery to the baseline logs again.
        assert!(overlays.emit_facts("shown", baseline));
        assert_eq!(native_log_lines(&log).len(), 3);
        // Privacy on every line: no handles, paths, titles, or content.
        for line in native_log_lines(&log) {
            assert!(!line.contains("0x"), "{line}");
            assert!(!line.contains('\\'), "{line}");
            assert!(!line.contains("HWND"), "{line}");
        }
    }

    #[test]
    fn native_emit_unknown_relative_order() {
        // Absent taskbar plus failed topmost reads: every unknown leg
        // reports "unknown" with zero invented above/below counts.
        let (mut overlays, _dir, log) = native_log_rig();
        let unknown = NativePagerFacts {
            total: 2,
            visible: 1,
            topmost_true: 0,
            topmost_unknown: 2,
            taskbar_visible: None,
            taskbar_topmost: None,
            above: 0,
            below: 0,
            order_unknown: 2,
        };
        assert!(overlays.emit_facts("shown", unknown));
        let lines = native_log_lines(&log);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("\"taskbar_visible\":\"unknown\""),
            "{}",
            lines[0]
        );
        assert!(
            lines[0].contains("\"taskbar_topmost\":\"unknown\""),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("\"order_unknown\":2"), "{}", lines[0]);
        assert!(lines[0].contains("\"above\":0"), "{}", lines[0]);
        assert!(lines[0].contains("\"below\":0"), "{}", lines[0]);
        assert!(!lines[0].contains("0x"), "{}", lines[0]);
        // Identical unknown repeats stay silent.
        assert!(!overlays.emit_facts("shown", unknown));
        assert_eq!(native_log_lines(&log).len(), 1);
    }

    #[test]
    fn native_emit_clears_when_no_overlays_remain() {
        // No native windows: the transition cache resets silently so the
        // next show re-baselines instead of replaying stale counts. Fully
        // offline (no window reads): the empty-windows arm never probes.
        let mut overlays = empty_overlays();
        overlays.last_native = Some(native_pager_signature(shown_facts()));
        overlays.emit_native_state("shown", Some(true));
        assert_eq!(
            overlays.last_native, None,
            "empty windows reset the baseline silently"
        );
    }

    #[test]
    fn native_hide_resets_baseline_for_next_show() {
        // Suppression hide clears the readback baseline silently, so the
        // next show after suppression re-baselines with fresh
        // observability instead of staying silent on equal counts.
        let (mut overlays, _dir, _log) = native_log_rig();
        assert!(overlays.emit_facts("shown", shown_facts()));
        assert!(overlays.last_native.is_some());
        overlays.hide_all();
        assert_eq!(overlays.last_native, None);
        // The next show re-emits the same counts as a fresh baseline.
        assert!(overlays.emit_facts("shown", shown_facts()));
    }

    fn trace_rig() -> (std::path::PathBuf, std::path::PathBuf) {
        static TRACE_LOG_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = TRACE_LOG_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tiler-pager-trace-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            seq
        ));
        std::fs::create_dir_all(&dir).expect("trace log dir");
        let log = dir.join("run.log");
        (dir, log)
    }

    #[test]
    fn trace_value_joins_drain_context_with_bounded_words() {
        // Real serialization seam: an ordered hide->show batch plus the
        // drain context and the production triple ride one `pager-wnd`
        // line. Sequence tells observed order; the context is correlation
        // for queued OS messages, never causal attribution.
        use crate::ffi::pager_trace::PagerTraceEvent;
        let events = vec![
            PagerTraceEvent {
                seq: 1,
                kind: "tiler-hide",
                win: Some(1),
                flags: None,
                after: None,
                show: None,
                show_status: None,
                x: None,
                y: None,
                w: None,
                h: None,
                size_type: None,
                dpi: None,
            },
            PagerTraceEvent {
                seq: 2,
                kind: "show",
                win: Some(1),
                flags: None,
                after: None,
                show: Some(true),
                show_status: Some(0),
                x: None,
                y: None,
                w: None,
                h: None,
                size_type: None,
                dpi: None,
            },
            PagerTraceEvent {
                seq: 3,
                kind: "pos-changed",
                win: Some(2),
                flags: Some(0x0040),
                after: Some("topmost"),
                show: None,
                show_status: None,
                x: Some(0),
                y: Some(40),
                w: Some(144),
                h: Some(40),
                size_type: None,
                dpi: None,
            },
            PagerTraceEvent {
                seq: 4,
                kind: "os-create",
                win: None,
                flags: None,
                after: None,
                show: None,
                show_status: None,
                x: None,
                y: None,
                w: None,
                h: None,
                size_type: None,
                dpi: None,
            },
        ];
        let value = pager_trace_value(&events, 0, 41, "req-7", 12, 34, "safe");
        assert_eq!(value["event"], serde_json::json!("pager-wnd"));
        assert_eq!(value["tick"], serde_json::json!(41));
        assert_eq!(value["request"], serde_json::json!("req-7"));
        assert_eq!(value["observation"], serde_json::json!(12));
        assert_eq!(value["revision"], serde_json::json!(34));
        assert_eq!(value["context"], serde_json::json!("safe"));
        assert_eq!(value["omitted"], serde_json::json!(0));
        let rendered = value["events"].as_array().expect("events array");
        assert_eq!(rendered.len(), 4);
        assert_eq!(rendered[0]["kind"], serde_json::json!("tiler-hide"));
        assert_eq!(rendered[0]["win"], serde_json::json!("pw1"));
        assert_eq!(rendered[1]["kind"], serde_json::json!("show"));
        assert_eq!(rendered[1]["show"], serde_json::json!(true));
        assert_eq!(rendered[2]["after"], serde_json::json!("topmost"));
        assert_eq!(rendered[3]["win"], serde_json::json!("unknown"));
        // Bounded vocabulary: opaque tokens plus words, never raw handles.
        let text = value.to_string();
        assert!(!text.contains("0x"), "{text}");
        assert!(!text.contains('\\'), "{text}");
        assert!(!text.contains("HWND"), "{text}");
        // Hide and show both survive: no coalescing up to the cap.
        assert!(
            text.contains("tiler-hide") && text.contains("\"show\""),
            "{text}"
        );
    }

    #[test]
    fn trace_emit_writes_batch_and_quiet_stays_silent() {
        // Real drain seam against the thread-local batch: the first drain
        // writes the batch with its drain-context label, the second drain
        // stays silent. Unchanged presentation with live traffic still
        // reports via the batch itself.
        use crate::ffi::pager_trace::{
            assign_window, drain, push_pos_changed, push_tiler_hide, set_enabled,
        };
        use windows_sys::Win32::Foundation::HWND;
        set_enabled(true);
        let _ = drain();
        let hwnd = 0x7100_isize as HWND;
        let token = assign_window(hwnd).expect("token");
        push_tiler_hide(hwnd);
        push_pos_changed(hwnd, 0x0040, 0_isize as HWND, 0, 40, 144, 40);
        let (_dir, log) = trace_rig();
        assert!(emit_pager_trace(&log, true, 9, "req-9", 3, 5, "safe"));
        let lines = native_log_lines(&log);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("\"event\":\"pager-wnd\""), "{}", lines[0]);
        assert!(lines[0].contains("\"context\":\"safe\""), "{}", lines[0]);
        assert!(lines[0].contains("\"request\":\"req-9\""), "{}", lines[0]);
        assert!(lines[0].contains("tiler-hide"), "{}", lines[0]);
        assert!(lines[0].contains(&format!("pw{token}")), "{}", lines[0]);
        // Drained: the next pump is quiet even under trace.
        assert!(!emit_pager_trace(&log, true, 10, "req-10", 3, 5, "safe"));
        assert_eq!(native_log_lines(&log).len(), 1);
        // Disabled tracing never writes: disable first so the pushes below
        // capture nothing, then prove no row and no pending batch.
        set_enabled(false);
        push_tiler_hide(hwnd);
        push_pos_changed(hwnd, 0x0040, 0_isize as HWND, 0, 40, 144, 40);
        assert!(!emit_pager_trace(&log, false, 11, "req-11", 3, 5, "safe"));
        assert_eq!(native_log_lines(&log).len(), 1);
        set_enabled(true);
        let (events, omitted) = drain();
        assert!(
            events.is_empty() && omitted == 0,
            "disabled capture leaves nothing"
        );
    }

    #[test]
    fn trace_emit_reports_overflow_and_suppressed_context() {
        // Overflow folds into an explicit omitted count; the suppressed
        // drain context labels correlation for queued OS traffic.
        use crate::ffi::pager_trace::{TRACE_CAP, assign_window, drain, push_show, set_enabled};
        use windows_sys::Win32::Foundation::HWND;
        set_enabled(true);
        let _ = drain();
        let hwnd = 0x7200_isize as HWND;
        assign_window(hwnd);
        for i in 0..(TRACE_CAP + 3) {
            push_show(hwnd, i % 2 == 0, 0);
        }
        let (_dir, log) = trace_rig();
        assert!(emit_pager_trace(&log, true, 7, "req-s", 1, 2, "suppressed"));
        let lines = native_log_lines(&log);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("\"context\":\"suppressed\""),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("\"omitted\":3"), "{}", lines[0]);
        let value: serde_json::Value = serde_json::from_str(&lines[0]).expect("valid json line");
        assert_eq!(value["events"].as_array().expect("array").len(), TRACE_CAP);
    }
}
