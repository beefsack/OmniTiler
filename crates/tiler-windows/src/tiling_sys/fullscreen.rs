#![forbid(unsafe_code)]
use super::world_diff::StateWrite;
use super::*;

/// Style bits the project-owned fullscreen toggle clears on entry
/// (`WS_CAPTION` covers border+frame, `WS_THICKFRAME` the sizing border).
/// Only these bits are ever stored and restored; unrelated style changes the
/// application makes meanwhile are preserved.
pub(super) const FULLSCREEN_STYLE_BITS: u32 = WS_CAPTION | WS_THICKFRAME;
/// Inert per-window ownership marker for the project-owned fullscreen toggle.
/// Nonzero magic only; never a pointer, never trusted across window lifetime
/// (the property dies with its window, so a recycled HWND never inherits it).
const FULLSCREEN_MARKER: usize = 0x4653_3131;
/// Ownership marker property: presence of the exact magic proves a
/// project-owned fullscreen frame with restoration metadata alongside.
const FULLSCREEN_PROP: &str = "OmniTilerFullscreen";
/// Stored changed style bits plus one (never zero when present, so absence
/// stays distinguishable from a no-op clear).
const FULLSCREEN_STYLE_PROP: &str = "OmniTilerFullscreenStyle";
/// Preexisting maximize state: 1 was normal, 2 was maximized. Absence means
/// corrupt metadata, never a default.
const FULLSCREEN_MAX_PROP: &str = "OmniTilerFullscreenMax";
/// Pre-fullscreen outer frame for non-maximized entries, one biased `i32`
/// per property (`stored = value + BIAS + 1`, so a missing property (NULL)
/// stays distinguishable from any real coordinate, including negative
/// multi-monitor origins). All four absent means a legacy preimage (style
/// only); any present-but-undecodable or partial set is corrupt metadata.
const FULLSCREEN_X_PROP: &str = "OmniTilerFullscreenX";
const FULLSCREEN_Y_PROP: &str = "OmniTilerFullscreenY";
const FULLSCREEN_W_PROP: &str = "OmniTilerFullscreenW";
const FULLSCREEN_H_PROP: &str = "OmniTilerFullscreenH";
/// Bias shifting the `i32` coordinate range to positive storage: stored
/// values run `1..=2^32`, so NULL (0) is never a valid encoding.
const FRAME_BIAS: i64 = 0x8000_0000;

/// Owned restoration metadata for one project-toggled fullscreen window: the
/// exact style bits cleared on entry, whether the window was maximized
/// before (fullscreen wins over maximize; exit reasserts it), and the
/// pre-fullscreen outer frame for non-maximized entries (exit moves the
/// window back to it; `None` is a legacy style-only preimage or a maximized
/// entry, which reasserts maximize instead of restoring geometry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FullscreenMeta {
    pub(super) changed: u32,
    pub(super) was_maximized: bool,
    pub(super) frame: Option<Rect>,
}

/// Pure codec for one biased frame coordinate (no HWND access): `encode`
/// never yields 0 (NULL means absent), `decode` rejects 0 and anything
/// outside the shifted `i32` range.
pub(super) fn encode_frame_value(value: i32) -> usize {
    ((i64::from(value) + FRAME_BIAS + 1) as u64) as usize
}

pub(super) fn decode_frame_value(stored: usize) -> Option<i32> {
    let value = (stored as u64 as i64).checked_sub(FRAME_BIAS + 1)?;
    i32::try_from(value).ok()
}

/// Read one frame coordinate property: `Ok(None)` when absent (legacy or
/// maximized entry), `Ok(Some(v))` when valid, `Err(())` when present but
/// undecodable (corrupt metadata, never guessed).
fn get_frame_value(hwnd: HWND, name: &str) -> std::result::Result<Option<i32>, ()> {
    let raw = crate::ffi::get_prop(hwnd as usize as u64, name);
    let Some(raw) = raw else {
        return Ok(None);
    };
    decode_frame_value(raw).map(Some).ok_or(())
}

fn set_frame_value(hwnd: HWND, name: &str, value: i32) -> bool {
    crate::ffi::set_prop(hwnd as usize as u64, name, encode_frame_value(value))
}

/// Read the owned fullscreen metadata. `None` means app-owned or absent:
/// a missing marker, a wrong magic, any missing companion property, a
/// changed-bits value outside the owned mask, or a present-but-undecodable
/// (or partial/degenerate) frame set all read as unowned, never as
/// a default preimage. Only bits in `FULLSCREEN_STYLE_BITS` are ever owned;
/// arbitrary style bits are never trusted for restoration. A non-maximized
/// entry with all four frame properties absent is a legacy style-only
/// preimage (`frame: None`); maximized entries never carry a frame.
pub(super) fn read_fullscreen_meta(hwnd_u64: u64) -> Option<FullscreenMeta> {
    let hwnd = hwnd_u64 as isize as HWND;
    if !crate::ffi::is_window(hwnd_u64) {
        return None;
    }
    let marker = crate::ffi::get_prop(hwnd_u64, FULLSCREEN_PROP);
    if marker.is_none_or(|m| m != FULLSCREEN_MARKER) {
        return None;
    }
    let bits = crate::ffi::get_prop(hwnd_u64, FULLSCREEN_STYLE_PROP);
    bits?;
    let changed = u32::try_from(bits.expect("prop present").checked_sub(1)?).ok()?;
    if changed & !FULLSCREEN_STYLE_BITS != 0 {
        return None;
    }
    let max = crate::ffi::get_prop(hwnd_u64, FULLSCREEN_MAX_PROP);
    max?;
    let was_maximized = match max.expect("prop present") {
        1 => false,
        2 => true,
        _ => return None,
    };
    if was_maximized {
        return Some(FullscreenMeta {
            changed,
            was_maximized,
            frame: None,
        });
    }
    let x = get_frame_value(hwnd, FULLSCREEN_X_PROP).ok()?;
    let y = get_frame_value(hwnd, FULLSCREEN_Y_PROP).ok()?;
    let w = get_frame_value(hwnd, FULLSCREEN_W_PROP).ok()?;
    let h = get_frame_value(hwnd, FULLSCREEN_H_PROP).ok()?;
    let frame = match (x, y, w, h) {
        (None, None, None, None) => None,
        (Some(x), Some(y), Some(w), Some(h)) if w > 0 && h > 0 => Some(Rect { x, y, w, h }),
        _ => return None,
    };
    Some(FullscreenMeta {
        changed,
        was_maximized,
        frame,
    })
}

/// Remove one owned metadata property set best-effort. Used to clean partial
/// `SetProp` writes so a failed entry never leaves a trusted-looking residue.
fn remove_fullscreen_meta_props(hwnd: HWND) {
    let hwnd_u64 = hwnd as usize as u64;
    for name in [
        FULLSCREEN_PROP,
        FULLSCREEN_STYLE_PROP,
        FULLSCREEN_MAX_PROP,
        FULLSCREEN_X_PROP,
        FULLSCREEN_Y_PROP,
        FULLSCREEN_W_PROP,
        FULLSCREEN_H_PROP,
    ] {
        crate::ffi::remove_prop(hwnd_u64, name);
    }
}

/// Store the owned metadata with readback: all properties must read
/// back exactly, or the entry fails closed with no partial residue trusted.
/// A partial `SetProp` sequence cleans up before returning false, so a later
/// press never mistakes residue for an owned preimage. Non-maximized entries
/// always carry their pre-fullscreen frame; maximized entries carry none.
pub(super) fn write_fullscreen_meta(hwnd_u64: u64, meta: &FullscreenMeta) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    if !crate::ffi::is_window(hwnd_u64) {
        return false;
    }
    if !meta.was_maximized && meta.frame.is_none_or(|frame| frame.w <= 0 || frame.h <= 0) {
        return false;
    }
    if meta.was_maximized && meta.frame.is_some() {
        return false;
    }
    let stored_bits = (meta.changed as usize).wrapping_add(1);
    let stored_max = if meta.was_maximized { 2usize } else { 1usize };
    if !crate::ffi::set_prop(hwnd_u64, FULLSCREEN_PROP, FULLSCREEN_MARKER) {
        return false;
    }
    if !crate::ffi::set_prop(hwnd_u64, FULLSCREEN_STYLE_PROP, stored_bits) {
        remove_fullscreen_meta_props(hwnd);
        return false;
    }
    if !crate::ffi::set_prop(hwnd_u64, FULLSCREEN_MAX_PROP, stored_max) {
        remove_fullscreen_meta_props(hwnd);
        return false;
    }
    if let Some(frame) = meta.frame
        && (!set_frame_value(hwnd, FULLSCREEN_X_PROP, frame.x)
            || !set_frame_value(hwnd, FULLSCREEN_Y_PROP, frame.y)
            || !set_frame_value(hwnd, FULLSCREEN_W_PROP, frame.w)
            || !set_frame_value(hwnd, FULLSCREEN_H_PROP, frame.h))
    {
        remove_fullscreen_meta_props(hwnd);
        return false;
    }
    if read_fullscreen_meta(hwnd_u64).is_some_and(|back| back == *meta) {
        true
    } else {
        remove_fullscreen_meta_props(hwnd);
        false
    }
}

/// Remove the owned metadata after a verified restore. Best-effort removal
/// with an absence readback; a failed removal keeps the marker so a later
/// exit still finds its preimage rather than stranding the frame.
pub(super) fn clear_fullscreen_meta(hwnd_u64: u64) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    if !crate::ffi::is_window(hwnd_u64) {
        return false;
    }
    remove_fullscreen_meta_props(hwnd);
    read_fullscreen_meta(hwnd_u64).is_none()
}

/// Maximum excess tolerated between a maximized visible frame and its work
/// area: a frame that still covers the full monitor (the fullscreen
/// rectangle) exceeds the work area by the taskbar strip, so a couple of
/// pixels absorb rounding while that failure can never pass.
const MAXIMIZED_WORK_AREA_EXCESS_PX: i32 = 2;
/// Maximum inset tolerated on each edge: DWM excludes the invisible resize
/// border on a manually placed maximized window (~8 logical pixels), so a
/// larger allowance still refuses a genuinely smaller frame.
const MAXIMIZED_WORK_AREA_INSET_PX: i32 = 24;

/// Pure work-area match for one maximized visible frame: the frame must lie
/// within its work area (no taskbar overlap) and must still fill it except
/// for a small border inset. Used only for a positive post-maximize proof;
/// a full-monitor frame, a pending maximize frame, or a genuinely smaller
/// frame refuses.
pub(super) fn frame_settled_on_work_area(visible: Rect, work: Rect) -> bool {
    let visible_left = i64::from(visible.x);
    let visible_top = i64::from(visible.y);
    let visible_right = visible_left + i64::from(visible.w);
    let visible_bottom = visible_top + i64::from(visible.h);
    let work_left = i64::from(work.x);
    let work_top = i64::from(work.y);
    let work_right = work_left + i64::from(work.w);
    let work_bottom = work_top + i64::from(work.h);
    let excess = i64::from(MAXIMIZED_WORK_AREA_EXCESS_PX);
    let inset = i64::from(MAXIMIZED_WORK_AREA_INSET_PX);
    let contained = visible_left >= work_left - excess
        && visible_top >= work_top - excess
        && visible_right <= work_right + excess
        && visible_bottom <= work_bottom + excess;
    let covering = visible_left <= work_left + inset
        && visible_top <= work_top + inset
        && visible_right >= work_right - inset
        && visible_bottom >= work_bottom - inset;
    contained && covering
}

/// Pure monitor selection for one frame: the work area of the enumerated
/// display whose full bounds contain the frame's center. `None` when no
/// display qualifies: never a guessed or borrowed work area.
pub(super) fn covering_work_area(visible: Rect, monitors: &[(Rect, Rect)]) -> Option<Rect> {
    let cx = i64::from(visible.x) + i64::from(visible.w) / 2;
    let cy = i64::from(visible.y) + i64::from(visible.h) / 2;
    monitors.iter().find_map(|(full, work)| {
        let inside = cx >= i64::from(full.x)
            && cx < i64::from(full.x) + i64::from(full.w)
            && cy >= i64::from(full.y)
            && cy < i64::from(full.y) + i64::from(full.h);
        inside.then_some(*work)
    })
}

/// Fresh native display list as `(full, work)` pairs; `None` when the
/// enumeration fails or yields no readable display (never a fallback).
fn live_monitor_rects() -> Option<Vec<(Rect, Rect)>> {
    let monitors = crate::ffi::enumerate_monitors()?;
    let mut pairs = Vec::with_capacity(monitors.len());
    for info in &monitors {
        let (Some(full), Some(work)) = (rect_from_win(info.full), rect_from_win(info.work)) else {
            continue;
        };
        pairs.push((full, work));
    }
    (!pairs.is_empty()).then_some(pairs)
}

/// Fresh proof that a maximized frame has really returned to a monitor work
/// area: the window reads maximized, its DWM visible frame is readable, and
/// that frame lies within the work area of the display it covers while still
/// filling it ([`frame_settled_on_work_area`]). `ShowWindowAsync` completion
/// is asynchronous, so a pending request reads `false`; the caller keeps the
/// preimage and retries (the retirement sweep applies the same proof) instead
/// of claiming a restore it did not observe.
fn maximized_work_area_settled(hwnd_u64: u64) -> bool {
    if !crate::ffi::is_zoomed(hwnd_u64) {
        return false;
    }
    let Some(visible) = crate::ffi::extended_frame_bounds(hwnd_u64).and_then(rect_from_win) else {
        return false;
    };
    let Some(monitors) = live_monitor_rects() else {
        return false;
    };
    covering_work_area(visible, &monitors)
        .is_some_and(|work| frame_settled_on_work_area(visible, work))
}

/// One bounded corrective placement for a maximized frame that the async
/// reassert left at the fullscreen rectangle: move it to the work area of the
/// display it covers (exactly the frame a maximized entry had before the
/// project stripped the chrome). No activation or z-order change; an
/// unreadable display list or coverage refuses rather than borrowing another
/// screen's work area.
fn restore_maximized_work_area(hwnd_u64: u64) -> bool {
    let current = crate::ffi::extended_frame_bounds(hwnd_u64)
        .and_then(rect_from_win)
        .or_else(|| live_outer_rect(hwnd_u64));
    let Some(current) = current else {
        return false;
    };
    let Some(monitors) = live_monitor_rects() else {
        return false;
    };
    let Some(work) = covering_work_area(current, &monitors) else {
        return false;
    };
    crate::ffi::set_window_pos(
        hwnd_u64,
        work.x,
        work.y,
        work.w,
        work.h,
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
    )
}

/// Pure settled-outer-geometry proof for a non-maximized preimage restore:
/// the live `GetWindowRect` must equal the recorded pre-fullscreen frame
/// exactly. A refused/read-failed move, a clamped placement, an application
/// or window-manager adjustment, and a still-fullscreen rectangle all fail.
/// This is the proof the owned exit was missing before it retires its R12
/// evidence; a maximized restore proves the work-area frame separately
/// ([`frame_settled_on_work_area`]) because its outer rectangle is not a
/// direct placement target.
pub(super) fn outer_rect_settled(expected: Rect, actual: Option<Rect>) -> bool {
    actual.is_some_and(|actual| actual == expected)
}

/// Best-effort degraded-slot line after a verified restore: the native state
/// is already restored, so a failed durable removal is reported (a later
/// startup re-checks the slot) and never turned into another window write.
fn log_pending_durable_retire(dir: &Path, property: &str) {
    let Ok(session) = crate::native::current_identity() else {
        return;
    };
    super::trace::log_json_at(
        &crate::lifecycle::sys::log_path_for(dir, &session.process_creation),
        serde_json::json!({
            "event": "durable-preimage-retire",
            "property": property,
            "outcome": "pending",
        }),
    );
}

/// Enter project-owned fullscreen: persist the durable R12 preimage first,
/// store the window-prop restoration preimage, then clear the frame bits and
/// cover the monitor without activating. No blind input synthesis; the
/// official style/frame APIs only. The preimage keeps the pre-fullscreen
/// outer frame for non-maximized windows (floats and tiled alike) so exit can
/// move the window back; an unreadable frame fails closed before any write.
/// Re-entry never overwrites an existing preimage: an owned frame always
/// exits first, so an entry that finds metadata returns without a write.
///
/// Typed outcome: [`StateWrite::NotAttempted`] for every pre-actuator refusal
/// (dead window, existing preimage, unreadable live frame, unreadable durable
/// identity/slot/session, failed durable publish, or a metadata write that
/// read back unverifiable: the target property never changed and the fresh
/// preimages are cleared), [`StateWrite::Called(false)`] when the style setter
/// explicitly refused, [`StateWrite::Threw`] when the chrome changed but the
/// covering move or the readback did not settle (ambiguous; the preimage is
/// preserved for a later exit), and [`StateWrite::Called(true)`] only when the
/// owned bits read back cleared. No automatic retries on any path.
pub(super) fn enter_fullscreen(hwnd_u64: u64, full: Rect, dir: &Path) -> StateWrite {
    if !crate::ffi::is_window(hwnd_u64) {
        return StateWrite::NotAttempted;
    }
    if read_fullscreen_meta(hwnd_u64).is_some() {
        return StateWrite::NotAttempted;
    }
    let style = crate::ffi::window_style(hwnd_u64);
    let was_maximized = is_zoomed_now(hwnd_u64);
    let frame = if was_maximized {
        None
    } else {
        match live_outer_rect(hwnd_u64) {
            Some(frame) => Some(frame),
            None => return StateWrite::NotAttempted,
        }
    };
    let meta = FullscreenMeta {
        changed: style & FULLSCREEN_STYLE_BITS,
        was_maximized,
        frame,
    };
    // R12 durable preimage: the crash/restart authority is written before any
    // chrome write, and an unreadable identity/tag/session or a failed atomic
    // publish fails closed with no window props written.
    let Ok(session) = crate::native::current_identity() else {
        return StateWrite::NotAttempted;
    };
    let Some((pid, creation, tag)) = durable_identity(hwnd_u64) else {
        return StateWrite::NotAttempted;
    };
    let Some(row) = super::fullscreen_preimage::FullscreenPreimage::new(
        hwnd_u64,
        pid,
        &creation,
        &tag,
        &session.process_creation,
        meta.changed,
        meta.was_maximized,
        meta.frame,
    ) else {
        return StateWrite::NotAttempted;
    };
    if super::fullscreen_preimage::persist(dir, &row).is_err() {
        return StateWrite::NotAttempted;
    }
    if !write_fullscreen_meta(hwnd_u64, &meta) {
        // No chrome changed: the durable row never gained restoration
        // authority, so drop it rather than let a later startup touch
        // untouched state.
        let _ = super::fullscreen_preimage::clear(dir, hwnd_u64, pid, &creation, &tag);
        return StateWrite::NotAttempted;
    }
    let next = (style & !FULLSCREEN_STYLE_BITS) as i32;
    let (prev, style_err) = crate::ffi::set_window_style(hwnd_u64, next);
    if prev == 0 && style_err != 0 {
        clear_fullscreen_meta(hwnd_u64);
        let _ = super::fullscreen_preimage::clear(dir, hwnd_u64, pid, &creation, &tag);
        return StateWrite::Called(false);
    }
    let placed = crate::ffi::set_window_pos(
        hwnd_u64,
        full.x,
        full.y,
        full.w,
        full.h,
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_FRAMECHANGED,
    );
    if !placed {
        return StateWrite::Threw;
    }
    if crate::ffi::window_style(hwnd_u64) & FULLSCREEN_STYLE_BITS == 0 {
        StateWrite::Called(true)
    } else {
        StateWrite::Threw
    }
}

/// Live exact lifetime of one window for the durable preimage slot: pid,
/// held-process creation, and the installed membership tag. Any unreadable
/// leg is `None` (fail closed, never guessed).
fn durable_identity(hwnd_u64: u64) -> Option<(u32, String, String)> {
    let pid = crate::ffi::window_pid(hwnd_u64);
    if pid == 0 {
        return None;
    }
    let held = crate::native::HeldProcess::open(pid).ok()?;
    let live = held.identity().ok()?;
    if live.pid != pid || live.process_creation.is_empty() {
        return None;
    }
    let tag = crate::product_hide::sys::read_member_tag(hwnd_u64)?;
    if tag.is_empty() {
        return None;
    }
    Some((pid, live.process_creation, tag))
}

/// Typed result of the owned-exit ordering, projected onto [`StateWrite`] by
/// [`exit_fullscreen_owned`]. `Refused` is an explicit native refusal after a
/// call; `Unproven` is an acted-on but unobserved shape (pending asynchronous
/// maximize, disagreeing readback, or retained metadata); `NotAttempted` means
/// no actuator ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnedExitOutcome {
    NotAttempted,
    Refused,
    Unproven,
    Restored,
}

/// Exit project-owned fullscreen: restore exactly the owned mask bits
/// (unrelated application style changes meanwhile are preserved), reapply
/// the frame without moving (so externally moved frames still exit), move a
/// non-maximized window back to its pre-fullscreen frame, verify the live
/// style *and the settled actual outer rectangle*, reassert a preexisting
/// maximize, then retire the exact durable R12 row *before* dropping the
/// window metadata. A failed restore, an unsettled outer rectangle, or a
/// durable row that cannot clear keeps the metadata so a later press or the
/// retirement sweep can retry; nothing is guessed and nothing retries
/// automatically. `meta.changed` is already validated to the owned mask on
/// read, so restoration never touches arbitrary bits. A legacy preimage
/// without a frame restores style only (old behavior, no strand).
///
/// A maximized exit must also prove the async maximize really returned the
/// frame to the covered display's work area (a window already maximized at
/// entry keeps the fullscreen rectangle otherwise): one bounded corrective
/// move runs when the first fresh proof fails, and an unsettled result keeps
/// the metadata for the next press or the retirement sweep. Only a verified
/// `Restored` outcome clears the durable row, and only when the live exact
/// lifetime is still readable; keeping the marker on a degraded durable slot
/// means a stale row can never replay an old frame on the next startup.
pub(super) fn exit_fullscreen_owned(
    hwnd_u64: u64,
    meta: &FullscreenMeta,
    dir: &Path,
) -> StateWrite {
    let retire_durable = || -> bool {
        let Some((pid, creation, tag)) = durable_identity(hwnd_u64) else {
            log_pending_durable_retire(dir, "fullscreen");
            return false;
        };
        if super::fullscreen_preimage::clear(dir, hwnd_u64, pid, &creation, &tag).is_err() {
            log_pending_durable_retire(dir, "fullscreen");
            return false;
        }
        true
    };
    match exit_fullscreen_owned_with(&mut NativeOwnedExit, hwnd_u64, meta, retire_durable) {
        OwnedExitOutcome::NotAttempted => StateWrite::NotAttempted,
        OwnedExitOutcome::Refused => StateWrite::Called(false),
        OwnedExitOutcome::Unproven => StateWrite::Threw,
        OwnedExitOutcome::Restored => StateWrite::Called(true),
    }
}

/// Restore one durable fullscreen-chrome leftover through the exact
/// production owned-exit sequence (no ledger identity needed here: the
/// durable row already carries the verified lifetime). Used only by
/// [`super::fullscreen_preimage`] startup recovery, whose contract is the
/// `"restored"` verdict and which retires its own durable rows after that
/// verdict re-verifies; the exit's durable-clear phase is therefore a no-op
/// here. Every other outcome (`"dispatched"` for an unproven shape,
/// `"threw"` for not attempted/refused) leaves the row for a later attempt.
pub(super) fn restore_owned_chrome(hwnd_u64: u64, meta: &FullscreenMeta) -> &'static str {
    // Startup and graceful restore are already PMv2; the independent
    // restore command and crash watcher are not. R12 stores physical
    // pixels, so both the write and its independent readback need PMv2.
    match crate::ffi::with_pm_v2(|| {
        exit_fullscreen_owned_with(&mut NativeOwnedExit, hwnd_u64, meta, || true)
    }) {
        Some(OwnedExitOutcome::Restored) => "restored",
        Some(OwnedExitOutcome::Unproven) => "dispatched",
        None | Some(OwnedExitOutcome::NotAttempted | OwnedExitOutcome::Refused) => "threw",
    }
}

/// Fresh native proof behind one metadata-retirement decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RetireProof {
    /// Live style read: every originally cleared owned bit reads restored.
    pub(super) style_restored: bool,
    /// Live fullscreen probe (`None` = unreadable, never restored).
    pub(super) fullscreen: Option<bool>,
    /// Live maximize read (`false` = positively not maximized).
    pub(super) maximized: bool,
    /// Fresh work-area proof of a maximized frame (ignored otherwise).
    pub(super) maximize_settled: bool,
}

/// Pure retirement decision over one fresh native proof: the original
/// nonzero chrome mask must read restored and fullscreen must read positively
/// false; a maximized preimage that still reads maximized must also prove the
/// work-area frame (a restored caption can make a full-monitor frame read
/// non-fullscreen while the maximize placement is still the fullscreen
/// rectangle). A window the application has since restored to normal is
/// app-owned and needs no work-area proof. Any unreadable or transitional
/// shape refuses.
pub(super) fn retire_eligible(meta: &FullscreenMeta, proof: RetireProof) -> bool {
    crate::tiling::stale_fullscreen_retire_eligible(
        meta.changed,
        proof.style_restored,
        proof.fullscreen,
    ) && (!meta.was_maximized || !proof.maximized || proof.maximize_settled)
}

/// Retirement ordering: the durable row leaves before the window properties,
/// and a failed durable removal keeps the marker (no success is claimed).
/// Clearing the properties first could strand a stale durable row that a
/// later startup would replay over already-normal chrome.
pub(super) fn retire_metadata(
    clear_durable: impl FnOnce() -> bool,
    clear_properties: impl FnOnce() -> bool,
) -> bool {
    clear_durable() && clear_properties()
}

/// Retire one owned fullscreen marker whose chrome has provably been
/// restored by other means (the application or an external caller put every
/// owned style bit back and the live window is positively non-fullscreen).
///
/// Metadata removal only: this function performs no style, frame, activation
/// or z-order write, so retirement can never undo the restored chrome or move
/// the window. It re-proves the exact native lifetime (pid, held-process
/// creation, live membership tag), the live style restoration, and that the
/// marker still equals the caller's probed preimage immediately before
/// clearing. The fullscreen verdict is a fresh native read (live monitor
/// rects plus the live caption/frame probe), never the caller's observation
/// and never a hardcoded false; a maximized preimage that still reads
/// maximized must additionally prove the work-area frame.
///
/// Clearing drops the matching durable R12 row *before* the window props,
/// otherwise a later startup could restore chrome and a pre-fullscreen frame
/// over a window that is already normal; a failed durable removal keeps the
/// marker so a later sweep retries and no success is claimed. Any unreadable
/// or drifted leg fails closed.
pub(super) fn retire_restored_fullscreen(
    dir: &std::path::Path,
    key: &crate::native_guards::WindowKey,
    tag: &str,
    meta: &FullscreenMeta,
) -> bool {
    if meta.changed == 0 || tag.is_empty() || !crate::ffi::is_window(key.hwnd) {
        return false;
    }
    if crate::ffi::window_pid(key.hwnd) != key.pid {
        return false;
    }
    let Some(held) = HeldProcess::open(key.pid).ok() else {
        return false;
    };
    let Ok(live) = held.identity() else {
        return false;
    };
    if live.pid != key.pid || live.process_creation != key.creation {
        return false;
    }
    if crate::product_hide::sys::read_member_tag(key.hwnd).as_deref() != Some(tag) {
        return false;
    }
    let Some(style) = crate::ffi::window_style_checked(key.hwnd) else {
        return false;
    };
    if read_fullscreen_meta(key.hwnd).as_ref() != Some(meta) {
        return false;
    }
    // Second, independent native proof immediately before removal: fresh
    // style, monitor rects, caption/frame fullscreen read and maximize read.
    let Some(monitors) = live_monitor_rects() else {
        return false;
    };
    let fulls: Vec<Rect> = monitors.iter().map(|(full, _)| *full).collect();
    let (maximized, fullscreen) = super::probes::live_overlay_flags(key.hwnd, &fulls);
    let proof = RetireProof {
        style_restored: style & meta.changed == meta.changed,
        fullscreen,
        maximized,
        maximize_settled: !maximized || maximized_work_area_settled(key.hwnd),
    };
    if !retire_eligible(meta, proof) {
        return false;
    }
    retire_metadata(
        || super::fullscreen_preimage::clear(dir, key.hwnd, key.pid, &key.creation, tag).is_ok(),
        || clear_fullscreen_meta(key.hwnd),
    )
}

/// Native effects behind the project-owned fullscreen exit, split from the
/// effect ordering so hermetic tests can substitute scripted answers while
/// the production sequence (style restore, frame reapply, pre-fullscreen
/// frame move, readback verify, maximize reassert, work-area settle proof,
/// metadata removal) runs unchanged. [`NativeOwnedExit`]
/// calls exactly the functions the inline code called; the added settle
/// reads are bounded observation, never another writable state change.
/// Message-only owned windows cannot carry this path (never enumerated with
/// live frames), so scripted stubs are the hermetic route.
pub(super) trait OwnedExitEffects {
    fn is_window(&mut self, hwnd: u64) -> bool;
    fn current_style(&mut self, hwnd: u64) -> u32;
    /// Exactly like `SetWindowLongW` plus `GetLastError`: `(prev, err)`.
    fn restore_style(&mut self, hwnd: u64, next: i32) -> (i32, u32);
    fn reapply_frame(&mut self, hwnd: u64) -> bool;
    /// Move to the pre-fullscreen frame (non-maximized exits only).
    fn restore_rect(&mut self, hwnd: u64, rect: Rect) -> bool;
    /// Fresh actual outer rectangle (`GetWindowRect`) after the move. `None`
    /// is unreadable and never a settled frame.
    fn outer_rect(&mut self, hwnd: u64) -> Option<Rect>;
    fn style_readback(&mut self, hwnd: u64) -> u32;
    fn reassert_maximize(&mut self, hwnd: u64) -> bool;
    /// Fresh readback of the maximized frame: reads maximized and covers
    /// exactly its display's work area. Asynchronous completion may read
    /// false; this is a proof, never a guess.
    fn maximize_settled(&mut self, hwnd: u64) -> bool;
    /// One bounded corrective move back to the covered display's work area
    /// when the async reassert left the fullscreen rectangle in place.
    fn restore_maximized_frame(&mut self, hwnd: u64) -> bool;
    fn clear_meta(&mut self, hwnd: u64) -> bool;
}

/// Production native effects for the owned exit: exact pass-throughs to the
/// Win32 reads/writes the route requires.
struct NativeOwnedExit;

impl OwnedExitEffects for NativeOwnedExit {
    fn is_window(&mut self, hwnd: u64) -> bool {
        crate::ffi::is_window(hwnd)
    }

    fn current_style(&mut self, hwnd: u64) -> u32 {
        crate::ffi::window_style(hwnd)
    }

    fn restore_style(&mut self, hwnd: u64, next: i32) -> (i32, u32) {
        crate::ffi::set_window_style(hwnd, next)
    }

    fn reapply_frame(&mut self, hwnd: u64) -> bool {
        crate::ffi::reapply_frame(hwnd)
    }

    fn restore_rect(&mut self, hwnd: u64, rect: Rect) -> bool {
        crate::ffi::set_window_pos(
            hwnd,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
    }

    fn outer_rect(&mut self, hwnd: u64) -> Option<Rect> {
        crate::ffi::window_rect(hwnd).and_then(rect_from_win)
    }

    fn style_readback(&mut self, hwnd: u64) -> u32 {
        crate::ffi::window_style(hwnd)
    }

    fn reassert_maximize(&mut self, hwnd: u64) -> bool {
        crate::ffi::show_window_async(hwnd, SW_MAXIMIZE)
    }

    fn maximize_settled(&mut self, hwnd: u64) -> bool {
        maximized_work_area_settled(hwnd)
    }

    fn restore_maximized_frame(&mut self, hwnd: u64) -> bool {
        restore_maximized_work_area(hwnd)
    }

    fn clear_meta(&mut self, hwnd: u64) -> bool {
        clear_fullscreen_meta(hwnd)
    }
}

/// Owned-exit effect ordering shared by the native wrapper and hermetic
/// tests: the exact durable row is retired only after the style restore,
/// frame reapply, pre-fullscreen frame move, style readback, settled actual
/// outer geometry (non-maximized frame preimages only), and maximize
/// reassert all verify, and the native marker is dropped only after that
/// durable retirement succeeded. A maximized entry additionally proves the
/// async maximize really returned the frame to the covered display's work
/// area, with one bounded corrective move when the first proof fails; an
/// unsettled frame keeps the metadata for a later explicit press or the
/// retirement sweep. Any explicit refusal is [`OwnedExitOutcome::Refused`];
/// an acted-on but unobserved shape (including a durable row that cannot
/// clear) is [`OwnedExitOutcome::Unproven`], so a stale durable row never
/// outlives the marker it could replay over. Legacy preimages without a
/// frame keep the style-only path. Recovery passes a no-op durable retirement
/// because its durable store owner retires rows itself.
pub(super) fn exit_fullscreen_owned_with(
    effects: &mut dyn OwnedExitEffects,
    hwnd_u64: u64,
    meta: &FullscreenMeta,
    retire_durable: impl FnOnce() -> bool,
) -> OwnedExitOutcome {
    if !effects.is_window(hwnd_u64) {
        return OwnedExitOutcome::NotAttempted;
    }
    let current = effects.current_style(hwnd_u64);
    let next = (current | meta.changed) as i32;
    let (prev, style_err) = effects.restore_style(hwnd_u64, next);
    if prev == 0 && style_err != 0 {
        return OwnedExitOutcome::Refused;
    }
    if !effects.reapply_frame(hwnd_u64) {
        return OwnedExitOutcome::Refused;
    }
    if !meta.was_maximized
        && let Some(frame) = meta.frame
        && !effects.restore_rect(hwnd_u64, frame)
    {
        return OwnedExitOutcome::Refused;
    }
    if effects.style_readback(hwnd_u64) & meta.changed != meta.changed {
        return OwnedExitOutcome::Unproven;
    }
    // Settled actual outer geometry: the move must have produced exactly the
    // recorded pre-fullscreen frame before any restore evidence is retired.
    // A read failure or a drifted/clamped frame keeps the metadata for a
    // later press or the retirement sweep; nothing is guessed.
    if !meta.was_maximized
        && let Some(frame) = meta.frame
        && !outer_rect_settled(frame, effects.outer_rect(hwnd_u64))
    {
        return OwnedExitOutcome::Unproven;
    }
    if meta.was_maximized {
        // The pre-maximized frame was the covered display's work area. The
        // async reassert alone is not proof: a window already maximized on
        // entry keeps the fullscreen rectangle, so verify the settled frame
        // and make one bounded corrective move before dropping the preimage.
        if !effects.reassert_maximize(hwnd_u64) {
            return OwnedExitOutcome::Unproven;
        }
        if !effects.maximize_settled(hwnd_u64)
            && (!effects.restore_maximized_frame(hwnd_u64) || !effects.maximize_settled(hwnd_u64))
        {
            return OwnedExitOutcome::Unproven;
        }
    }
    if !retire_durable() {
        // The exact row could not be retired: keep the native marker so a
        // later sweep retries and no success is claimed over a stale row.
        return OwnedExitOutcome::Unproven;
    }
    if effects.clear_meta(hwnd_u64) {
        OwnedExitOutcome::Restored
    } else {
        OwnedExitOutcome::Unproven
    }
}

/// Fresh holdability check for one fullscreen HWND: same-session,
/// medium-integrity identity plus every safety gate observation admission
/// requires (never elevated, shell, tool, owned, dialog including the own
/// Settings UI, cloaked, or no-activate; in scope with a live hosted child where listed; frozen
/// allowlist plus owned-helper verification in proof modes). Returns the
/// member key on pass; the caller decides hold vs exemption. Read-only.
pub(super) fn holdable_key(
    state: &TileLoop,
    me: &ProcessIdentity,
    hwnd_u64: u64,
) -> Option<crate::native_guards::WindowKey> {
    if hwnd_u64 == 0 {
        return None;
    }
    if !crate::ffi::is_window(hwnd_u64) {
        return None;
    }
    let pid = crate::ffi::window_pid(hwnd_u64);
    if pid == 0 || pid == me.pid {
        return None;
    }
    let held = HeldProcess::open(pid).ok()?;
    let live = held.identity().ok()?;
    if live.pid != pid {
        return None;
    }
    if live.user_sid != me.user_sid || live.session_id != me.session_id {
        return None;
    }
    if held.integrity().ok().is_none_or(|rid| !is_medium_rid(rid)) {
        return None;
    }
    if crate::ffi::is_iconic(hwnd_u64) {
        return None;
    }
    let class = class_of(hwnd_u64 as isize as HWND);
    // Own Settings is an ordinary slotless member (never a dialog);
    // generic unowned `#32770` dialogs still refuse here while owned
    // windows (including owned dialogs) refuse at the owned gate below.
    let owned_live = crate::ffi::owner_of(hwnd_u64) != 0;
    if is_shell_class(&class)
        || crate::tiling::workspace_dialog_refuses(&class, owned_live, &live.exe_path, &me.exe_path)
    {
        return None;
    }
    if crate::ffi::owner_of(hwnd_u64) != 0 {
        return None;
    }
    let exstyle = crate::ffi::window_exstyle_checked(hwnd_u64)?;
    if exstyle & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) != 0 {
        return None;
    }
    let cloaked = crate::ffi::cloaked(hwnd_u64)?;
    if cloaked {
        return None;
    }
    if !scope_allows(&state.scope, &live.exe_path) {
        return None;
    }
    if !hosted_gate_allows(&live.exe_path, hwnd_u64, pid, &state.scope_hosts) {
        return None;
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == hwnd_u64)?;
        if verify_proof_owned(hwnd_u64, entry, me).is_err() {
            return None;
        }
    }
    Some(crate::native_guards::WindowKey {
        hwnd: hwnd_u64,
        pid,
        creation: live.process_creation,
    })
}

/// Track the born-fullscreen hold after one observation (KDE
/// initial-fullscreen-hold parity): lifetime-known non-fullscreen identities
/// accumulate here so a later fullscreen transition is a managed overlay,
/// never a born hold again; held keys observed non-fullscreen release for
/// normal fresh admission; hold and seen sets prune with the enumeration.
/// New holds are admitted in `ensure_workspace_assignments` (which owns
/// workspace membership, lifetime tags, and the held lifecycle log), never
/// here, so tracking never creates a non-member hold. Every observation path
/// funnels through `observe`, so one call here covers reconcile,
/// directional, workspace, gesture, and select ticks.
pub(super) fn track_fullscreen_holds(
    state: &mut TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) {
    let mut live_nonfullscreen: BTreeSet<crate::native_guards::WindowKey> = BTreeSet::new();
    for window in observed {
        live_nonfullscreen.insert(crate::native_guards::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        });
    }
    for row in retained {
        if !row.fullscreen {
            live_nonfullscreen.insert(row.key.clone());
        }
    }
    // Lifetime-known non-fullscreen (KDE `seenNonFullscreen` parity): any
    // non-fullscreen observation, including minimized members, means a later
    // fullscreen transition is a managed overlay, never a born hold again.
    // Windows that could never verify holdable (scope, proof, safety gates)
    // suspend either way, so this never weakens a fence.
    state
        .seen_nonfullscreen
        .extend(live_nonfullscreen.iter().cloned());
    let log_path = state.log_path.clone();
    let released: Vec<crate::native_guards::WindowKey> = state
        .born_fullscreen
        .iter()
        .filter(|key| live_nonfullscreen.contains(key) || !state.last_hwnds.contains(&key.hwnd))
        .cloned()
        .collect();
    for key in released {
        state.born_fullscreen.remove(&key);
        // R-SPC-04 D5: the first exit classifies with current hints and
        // predicate before maximize-clear/restore/slot side effects. The
        // entry persists until committed classification (see
        // `consume_classified_first_exits`): the next maximize-clear pass
        // only peeks at it, and assembly keeps classifying while it stands.
        state.born_first_exit.insert(key.clone());
        // Drop the hidden-snapshot seed so the first exit converges through
        // normal fresh admission (and the existing maximize clear when the
        // exit lands maximized) with no faked slot. The log below is the
        // once-per-lifecycle release line; held was logged at admission.
        if let Some(token) = state.member_tokens.get(&key).cloned() {
            state.member_rects.remove(&token);
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "initial-fullscreen-released",
                    "window": token,
                }),
            );
        } else if let Some(token) = retained
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.token.clone())
        {
            state.member_rects.remove(&token);
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "initial-fullscreen-released",
                    "window": token,
                }),
            );
        }
    }
    state
        .seen_nonfullscreen
        .retain(|key| state.last_hwnds.contains(&key.hwnd));
    state
        .born_first_exit
        .retain(|key| state.last_hwnds.contains(&key.hwnd));
    consume_classified_first_exits(state);
}

/// Consume first-exit marks whose classification committed (R-SPC-04 D5).
/// The retained Engine proves the outcome, so a classified mark must not
/// linger into later ticks: a lingering mark would force the tiled lane
/// again and could undo a later intentional float. Consumed when the live
/// client is Engine-automatic, a tiled member, or intentionally
/// floating/sticky. Anything else (no retained session yet, a refused
/// commit, or an uncommitted hold) keeps deferring: the clear keeps peeking
/// and assembly keeps classifying with no state reset. Runs at the end of
/// tracking, after releases insert fresh marks (a just-inserted mark still
/// shows its pre-commit Engine state and is never consumed here).
fn consume_classified_first_exits(state: &mut TileLoop) {
    let mut done = Vec::new();
    for key in state.born_first_exit.iter() {
        let Some(token) = state.member_tokens.get(key).cloned() else {
            continue;
        };
        // Intentional lanes own the window now (settled explicit float or
        // sticky, including restart-adopted intent): the exit already
        // resolved through them.
        if state.floating_of(key) || state.sticky_of(key) {
            done.push(key.clone());
            continue;
        }
        let Some(loc) = state.member_home(key) else {
            continue;
        };
        // World-owned classification: the last applied reply row decides.
        // An automatic fixed mark consumes the deferral (fixed exit floats);
        // a homed tiled row consumes it too (non-fixed exit tiles). No row
        // yet keeps deferring (uncommitted or foreign hold).
        let done_here = state.world.last_reply().is_some_and(|reply| {
            reply.windows.iter().any(|row| {
                row.lifetime.0 == token
                    && state.device_of(&row.output.0).as_deref() == Some(loc.output.as_str())
                    && row.workspace == loc.workspace
            })
        });
        if done_here {
            done.push(key.clone());
            continue;
        }
    }
    for key in done {
        state.born_first_exit.remove(&key);
    }
}
