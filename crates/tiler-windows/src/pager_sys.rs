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
//! of the proven `TrayNotifyWnd` edge (see [`crate::pager`]); secondary
//! taskbars always refuse because their clock area cannot be proven.
//! Fullscreen/game safety is fail-closed: the host hides every strip on any
//! suspend veto, elevated foreground, or unreadable fact, and z-order is
//! reasserted only while the host reports safe.
//!
//! Logging is sparse lifecycle only (counts and reasons, never HWNDs,
//! paths, or titles); creation failures retry forever on a fixed delay.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ffi::{
    IconBlit, MiniSlot, PagerCalibrationSink, PagerClick, PagerClickSink, PagerFrame, PagerWindow,
};
use crate::pager::{OutputPager, TaskbarFacts, TaskbarKind};
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

/// Enumerate secondary taskbars (`Shell_SecondaryTrayWnd`). Heavier than the
/// primary read, so the host calls this only on the slow revalidate cadence.
/// Every entry refuses layout (`secondary-tray-unproven`): the tray edge is
/// unprovable there, so this only feeds refusal logging, never positions.
#[must_use]
pub fn discover_secondaries() -> Vec<TaskbarFacts> {
    crate::ffi::discover_secondary_taskbars()
        .into_iter()
        .filter_map(|probe| {
            Some(TaskbarFacts {
                rect: probe.bar?,
                tray_left: None,
                kind: TaskbarKind::Secondary,
                visible: probe.visible,
                autohide: false,
                dpi: crate::pager::PAGER_BASE_DPI,
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
/// rectangle exactly as the painter always has.
fn pager_frame(pager: &OutputPager) -> PagerFrame {
    PagerFrame {
        ids: pager.ids.clone(),
        output: pager.output.clone(),
        cells: pager.cells.iter().map(|cell| cell.rect).collect(),
        active: pager.active_index,
        minis: pager
            .cells
            .iter()
            .map(|cell| {
                cell.minis
                    .iter()
                    .zip(cell.mini_icons.iter())
                    .map(|(mini, icon)| MiniSlot {
                        rect: *mini,
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
/// cell rects, active index, miniatures, and artwork (`Arc` equality
/// short-circuits on pointer identity, so steady-state artwork costs one
/// pointer compare per miniature, not a byte walk).
fn frames_equal(a: &OutputPager, b: &OutputPager) -> bool {
    a.ids == b.ids
        && a.output == b.output
        && a.active_index == b.active_index
        && a.cells.len() == b.cells.len()
        && a.cells
            .iter()
            .zip(b.cells.iter())
            .all(|(x, y)| x.rect == y.rect && x.minis == y.minis && x.mini_icons == y.mini_icons)
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
        })
    }

    fn log_line(&self, value: serde_json::Value) {
        use std::io::Write;
        if let Some(path) = self.log_path.as_ref()
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

    /// Present one pump's frames: create/move/show served outputs, destroy
    /// departed ones, repaint on frame change, and reassert topmost only on
    /// the safe cadence. Creation failures retry on a fixed delay and
    /// retry forever. Steady-state visibility reads stay (correctness over
    /// syscall counting); moves and repaints are change-gated.
    pub fn sync(&mut self, pagers: &[OutputPager], now: Instant) {
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
        let mut created = 0u32;
        let mut failed = 0u32;
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
                match PagerWindow::create(
                    pager.strip,
                    pager_frame(pager),
                    pager.dpi,
                    &self.sink,
                    &self.calibration,
                ) {
                    Some(window) => {
                        self.windows.insert(pager.output.clone(), window);
                        self.frames.insert(pager.output.clone(), pager.clone());
                        created += 1;
                        fresh = true;
                    }
                    None => {
                        failed += 1;
                        continue;
                    }
                }
            }
            let Some(window) = self.windows.get_mut(&pager.output) else {
                failed += 1;
                continue;
            };
            let frame_changed = match self.frames.get(&pager.output) {
                Some(last) if frames_equal(last, pager) => false,
                _ => {
                    window.set_frame(pager_frame(pager));
                    self.frames.insert(pager.output.clone(), pager.clone());
                    true
                }
            };
            if frame_changed || fresh {
                window.move_show(pager.strip);
            }
            window.ensure_shown();
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
        // elevation, or unknown fact holds this turn.
        if self
            .last_zassert
            .is_none_or(|at| now.duration_since(at) >= ZASSERT_INTERVAL)
        {
            self.last_zassert = Some(now);
            for window in self.windows.values() {
                window.reassert_topmost();
            }
        }
        self.last_hash = hash;
    }

    /// Hide every strip (suppression turns): windows persist for the next
    /// safe turn, so no destroy/create churn hides information.
    pub fn hide_all(&mut self) {
        for window in self.windows.values() {
            window.hide();
        }
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
    use crate::pager_icons::IconImage;
    use crate::workspace::WindowKey;
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
                    mini_keys: Vec::new(),
                    mini_icons: Vec::new(),
                })
                .collect(),
            active_index: active,
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
        // Artwork arrival (shared, not copied) gates a repaint; steady
        // state with the same `Arc` does not.
        let image = Arc::new(IconImage::new(32, 32, vec![4; 32 * 32 * 4]));
        let mut iconic = base.clone();
        iconic.cells[0].mini_icons.push(Some(Arc::clone(&image)));
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
    fn native_frame_shares_artwork_without_copying() {
        // `pager_frame` hands the worker `Arc` straight through: the
        // pointer the painter blits is the cache's own allocation.
        let image = Arc::new(IconImage::new(32, 32, vec![4; 32 * 32 * 4]));
        let mut pager = pager_fixture(&["ws-a"], 0);
        pager.cells[0].mini_icons.push(Some(Arc::clone(&image)));
        pager.cells[0].minis.push(Rect {
            x: 1,
            y: 1,
            w: 20,
            h: 20,
        });
        let frame = pager_frame(&pager);
        let slot = frame.minis[0][0].icon.as_ref().expect("legible mini blits");
        assert!(Arc::ptr_eq(&slot.image, &image));
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
}
