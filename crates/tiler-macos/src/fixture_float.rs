#![forbid(unsafe_code)]

//! Bounded same-window float evidence through the fixture tap.
//!
//! Two controller-owned arms share this module:
//! - `float-placement` proves a write-free first float in place and a
//!   retained-geometry re-float after retile/rotate;
//! - `float-sequences` proves the three retained float-frame recoveries on
//!   one subject lifetime: a held issuing reply converged by a later
//!   observation-only reply, composed maximize then float unmaximize, and a
//!   directional float half-snap.
//!
//! Every frame claim is a real AX plus CG readback. Setter counts come from
//! the product's own `tile frame-set` lines in its run-directory stderr log,
//! never from a controller guess. All waits are bounded.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use tiler_core::geometry::Rect;

use crate::config::ProbeModifier;
use crate::fixture_leg::{FLOAT_SEQUENCES_HOLD_OCCURRENCE, FOCUS_SETTLE_MS, settle_hold};
use crate::fixture_post::{FixtureDriver, LiveTransport};

const SETTLE_MS: u64 = 1_500;
/// Observation-only duplicate window: more than one 1 s worker backstop.
const OBSERVE_MS: u64 = 2_000;
/// Bounded wait for the product to report the one-shot issuing-reply hold.
const HOLD_TIMEOUT_MS: u64 = 3_000;
/// Bounded wait for an observation-only reply to converge a retained frame.
const CONVERGE_TIMEOUT_MS: u64 = 5_000;
/// Poll cadence for native readbacks and product log evidence.
const POLL_MS: u64 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Observed {
    ax: Rect,
    cg: Rect,
}

impl Observed {
    fn json(self) -> serde_json::Value {
        serde_json::json!({"ax": rect_json(self.ax), "cg": rect_json(self.cg)})
    }
}

fn rect_json(rect: Rect) -> serde_json::Value {
    serde_json::json!({"x": rect.x, "y": rect.y, "w": rect.w, "h": rect.h})
}

fn integral(value: f64) -> Option<i32> {
    let rounded = value.round();
    if (value - rounded).abs() > 1e-6 {
        return None;
    }
    if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return None;
    }
    Some(rounded as i32)
}

fn frame_from_parts(x: f64, y: f64, w: f64, h: f64) -> Option<Rect> {
    Some(Rect {
        x: integral(x)?,
        y: integral(y)?,
        w: integral(w)?,
        h: integral(h)?,
    })
}

fn frame(pid: u32, wid: u32) -> Result<Observed, (i32, String)> {
    let ax = crate::fixture_sample::read_joined_frame(pid, wid)
        .map_err(|e| (1, e))?
        .frame;
    let ax = frame_from_parts(ax.x, ax.y, ax.w, ax.h).ok_or_else(|| {
        (
            1,
            format!("fixture float evidence AX frame is not integral: {ax:?}"),
        )
    })?;
    let cg = crate::ffi::cg_window_list_basic()
        .map_err(|e| (1, e))?
        .into_iter()
        .find(|entry| entry.wid == wid && entry.pid == pid && entry.onscreen)
        .ok_or_else(|| (1, "fixture float evidence CG join is absent".to_owned()))?;
    let cg =
        frame_from_parts(cg.bounds.x, cg.bounds.y, cg.bounds.w, cg.bounds.h).ok_or_else(|| {
            (
                1,
                "fixture float evidence CG frame is not integral".to_owned(),
            )
        })?;
    Ok(Observed { ax, cg })
}

fn parse_requested(line: &str) -> Option<Rect> {
    let raw = line.split_once("requested=(")?.1.split_once(')')?.0;
    let mut parts = raw.split(',').map(|part| part.trim().parse::<i32>().ok());
    Some(Rect {
        x: parts.next()??,
        y: parts.next()??,
        w: parts.next()??,
        h: parts.next()??,
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct Attempts {
    subject: usize,
    sibling: usize,
    subject_requested: Option<Rect>,
}

/// Count target and sibling frame-set lines after `start`, retaining the
/// first subject-requested rectangle from those fresh lines.
fn frame_attempts(
    log: &mut File,
    start: u64,
    pid: u32,
    wid: u32,
    sibling: u32,
) -> std::io::Result<Attempts> {
    let end = log.metadata()?.len();
    let Some(len) = end.checked_sub(start) else {
        return Err(std::io::Error::other("fixture product log was truncated"));
    };
    log.seek(SeekFrom::Start(start))?;
    let subject_prefix = format!("tiler-macos tile frame-set pid={pid} wid={wid} ");
    let sibling_prefix = format!("tiler-macos tile frame-set pid={pid} wid={sibling} ");
    let mut reader = BufReader::new(log.take(len));
    let mut line = String::new();
    let mut counted = Attempts::default();
    while reader.read_line(&mut line)? != 0 {
        if line.starts_with(&subject_prefix) {
            counted.subject += 1;
            if counted.subject_requested.is_none() {
                counted.subject_requested = parse_requested(&line);
            }
        }
        counted.sibling += usize::from(line.starts_with(&sibling_prefix));
        line.clear();
    }
    Ok(counted)
}

fn log_error(error: std::io::Error) -> (i32, String) {
    (1, format!("fixture float evidence log: {error}"))
}

fn pause(ms: u64) -> Result<(), (i32, String)> {
    if crate::space_drag_diag_live::diag_interrupted() {
        return Err((
            1,
            "fixture-leg: interrupted (SIGINT/SIGTERM latched); restoring".to_owned(),
        ));
    }
    std::thread::sleep(std::time::Duration::from_millis(ms));
    Ok(())
}

/// Wait (bounded) for both native readbacks to equal `want`.
fn await_frame(pid: u32, wid: u32, want: Rect) -> Result<(bool, Observed), (i32, String)> {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(CONVERGE_TIMEOUT_MS);
    loop {
        let observed = frame(pid, wid)?;
        if observed.ax == want && observed.cg == want {
            return Ok((true, observed));
        }
        if std::time::Instant::now() >= deadline {
            return Ok((false, observed));
        }
        pause(POLL_MS)?;
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct HoldProof {
    count: usize,
    occurrence: Option<u32>,
    command_seq: Option<u64>,
}

fn u64_field(line: &str, key: &str) -> Option<u64> {
    line.split_once(key)?
        .1
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Scan the product's own hold marker after `start`. The marker is written
/// once, when the ordinary float reply's emission is genuinely held.
fn hold_marker(log: &mut File, start: u64) -> std::io::Result<HoldProof> {
    let end = log.metadata()?.len();
    let Some(len) = end.checked_sub(start) else {
        return Err(std::io::Error::other("fixture product log was truncated"));
    };
    log.seek(SeekFrom::Start(start))?;
    let mut reader = BufReader::new(log.take(len));
    let mut line = String::new();
    let mut proof = HoldProof::default();
    while reader.read_line(&mut line)? != 0 {
        if line.starts_with("tiler-macos fixture float reply held ") {
            proof.count += 1;
            if proof.occurrence.is_none() {
                proof.occurrence =
                    u64_field(&line, "occurrence=").and_then(|value| u32::try_from(value).ok());
                proof.command_seq = u64_field(&line, "command_seq=");
            }
        }
        line.clear();
    }
    Ok(proof)
}

fn await_hold_marker(log: &mut File, start: u64) -> Result<HoldProof, (i32, String)> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(HOLD_TIMEOUT_MS);
    loop {
        let proof = hold_marker(log, start).map_err(log_error)?;
        if proof.count != 0 || std::time::Instant::now() >= deadline {
            return Ok(proof);
        }
        pause(POLL_MS)?;
    }
}

/// Observe the first float in place, then rotate the tiled scene before
/// re-floating the same lifetime so its remembered and tiled frames differ.
pub(crate) fn run(
    driver: &FixtureDriver<'_, LiveTransport>,
    pid: u32,
    (caller, wid): (&str, u32),
    sibling: u32,
    modifier: ProbeModifier,
    product_log: &Path,
) -> Result<serde_json::Value, (i32, String)> {
    let mut log = File::open(product_log).map_err(log_error)?;
    let post = |chord| -> Result<serde_json::Value, (i32, String)> {
        let raw = driver.post(caller, wid, chord, modifier)?;
        settle_hold(SETTLE_MS)?;
        serde_json::from_str(&raw).map_err(|e| (1, e.to_string()))
    };
    settle_hold(SETTLE_MS)?;
    let base = frame(pid, wid)?;
    let sibling_before = frame(pid, sibling)?;
    let first_start = log.metadata().map_err(log_error)?.len();
    let first = post("float-g")?;
    let floated = frame(pid, wid)?;
    let sibling_after = frame(pid, sibling)?;
    let first_attempts =
        frame_attempts(&mut log, first_start, pid, wid, sibling).map_err(log_error)?;
    let untile_proved = sibling_before != sibling_after && first_attempts.sibling > 0;
    if !untile_proved || floated != base || first_attempts.subject != 0 {
        return Ok(serde_json::json!({
            "outcome": "float-placement-unproved", "first_press": first,
            "base": base.json(), "floated": floated.json(),
            "sibling_before": sibling_before.json(), "sibling_after": sibling_after.json(),
            "subject_frame_attempts": first_attempts.subject,
            "sibling_frame_attempts": first_attempts.sibling,
        }));
    }
    let retile = post("float-g")?;
    let rotate = post("orientation-o")?;
    let rotated = frame(pid, wid)?;
    let rotated_sibling = frame(pid, sibling)?;
    let refloat_start = log.metadata().map_err(log_error)?.len();
    let refloat = post("float-g")?;
    let remembered = frame(pid, wid)?;
    let refloat_sibling = frame(pid, sibling)?;
    let refloat_attempts =
        frame_attempts(&mut log, refloat_start, pid, wid, sibling).map_err(log_error)?;
    let restored = rotated != base && remembered == base && refloat_attempts.subject == 1;
    let sibling_reflowed = refloat_sibling != rotated_sibling && refloat_attempts.sibling > 0;
    Ok(serde_json::json!({
        "outcome": if restored && sibling_reflowed { "float-placement" } else { "float-placement-unproved" },
        "caller": caller, "wid": wid,
        "first_press": first, "retile_press": retile, "rotate_press": rotate,
        "refloat_press": refloat, "base": base.json(), "floated": floated.json(),
        "sibling_before": sibling_before.json(), "sibling_after": sibling_after.json(),
        "subject_frame_attempts": first_attempts.subject,
        "sibling_frame_attempts": first_attempts.sibling,
        "rotated_tile": rotated.json(),
        "refloat": remembered.json(), "refloat_sibling_reflowed": sibling_reflowed,
        "refloat_subject_frame_attempts": refloat_attempts.subject,
        "refloat_sibling_frame_attempts": refloat_attempts.sibling,
        "remembered_frame_restored": restored,
    }))
}

/// Run the three retained float-frame recovery sequences on one subject
/// lifetime. The subject starts tiled; the sibling supplies the reflow proof.
/// A failed sequence records its blocker and dependent sequences that cannot
/// start from a proven float base are skipped, never repaired by repeating
/// command input.
pub(crate) fn run_sequences(
    driver: &FixtureDriver<'_, LiveTransport>,
    pid: u32,
    (caller, wid): (&str, u32),
    (sibling_caller, sibling): (&str, u32),
    modifier: ProbeModifier,
    product_dir: &Path,
) -> Result<serde_json::Value, (i32, String)> {
    let mut log = File::open(product_dir.join("stderr.log")).map_err(log_error)?;
    let post_now = |chord| -> Result<serde_json::Value, (i32, String)> {
        let raw = driver.post(caller, wid, chord, modifier)?;
        serde_json::from_str(&raw).map_err(|e| (1, e.to_string()))
    };
    let post = |chord| -> Result<serde_json::Value, (i32, String)> {
        let report = post_now(chord)?;
        settle_hold(SETTLE_MS)?;
        Ok(report)
    };
    let mark = |log: &File| -> Result<u64, (i32, String)> {
        log.metadata().map(|meta| meta.len()).map_err(log_error)
    };

    driver.focus(caller, wid)?;
    settle_hold(FOCUS_SETTLE_MS)?;
    settle_hold(SETTLE_MS)?;
    let base = frame(pid, wid)?;
    let sibling_before = frame(pid, sibling)?;

    // Sequence 1: first float in place, retile/rotate, then re-float with the
    // issuing reply held. A real focus change to the sibling produces the
    // first observation-only reply; the retained placement must land exactly
    // once. Re-focusing the subject produces a later observation-only reply
    // that must add no duplicate setter.
    let first_start = mark(&log)?;
    let first = post("float-g")?;
    let floated = frame(pid, wid)?;
    let sibling_after_first = frame(pid, sibling)?;
    let first_attempts =
        frame_attempts(&mut log, first_start, pid, wid, sibling).map_err(log_error)?;
    let retile = post("float-g")?;
    let rotate = post("orientation-o")?;
    let rotated = frame(pid, wid)?;
    let refloat_start = mark(&log)?;
    let refloat = post_now("float-g")?;
    let hold_proof = await_hold_marker(&mut log, refloat_start)?;
    let hold_consumed = hold_proof.count == 1
        && hold_proof.occurrence == Some(FLOAT_SEQUENCES_HOLD_OCCURRENCE)
        && hold_proof.command_seq.is_some();
    let frame_after_hold = frame(pid, wid)?;
    driver.focus(sibling_caller, sibling)?;
    settle_hold(FOCUS_SETTLE_MS)?;
    let (converged, converged_frame) = await_frame(pid, wid, base.ax)?;
    settle_hold(SETTLE_MS)?;
    let refloat_attempts =
        frame_attempts(&mut log, refloat_start, pid, wid, sibling).map_err(log_error)?;
    let refloat_frame = frame(pid, wid)?;
    let duplicate_start = mark(&log)?;
    driver.focus(caller, wid)?;
    settle_hold(FOCUS_SETTLE_MS)?;
    settle_hold(OBSERVE_MS)?;
    let duplicate_attempts =
        frame_attempts(&mut log, duplicate_start, pid, wid, sibling).map_err(log_error)?;
    let duplicate_frame = frame(pid, wid)?;
    let sequence1_proved = hold_consumed
        && converged
        && refloat_frame == base
        && refloat_attempts.subject == 1
        && duplicate_attempts.subject == 0
        && duplicate_frame == base;
    let sequence1 = serde_json::json!({
        "outcome": if sequence1_proved { "held-reply-recovery" } else { "held-reply-recovery-unproved" },
        "base": base.json(), "first_press": first, "first_float": floated.json(),
        "sibling_before": sibling_before.json(), "sibling_after_first": sibling_after_first.json(),
        "first_float_subject_frame_attempts": first_attempts.subject,
        "first_float_sibling_frame_attempts": first_attempts.sibling,
        "retile_press": retile, "rotate_press": rotate, "rotated_tile": rotated.json(),
        "hold_consumed": hold_consumed,
        "hold_marker_count": hold_proof.count,
        "hold_occurrence": hold_proof.occurrence,
        "hold_command_seq": hold_proof.command_seq,
        "hold_frame": frame_after_hold.json(),
        "refloat_press": refloat,
        "converged": converged, "converged_frame": converged_frame.json(),
        "refloat_subject_frame_attempts": refloat_attempts.subject,
        "refloat_sibling_frame_attempts": refloat_attempts.sibling,
        "refloat_requested": refloat_attempts.subject_requested.map(rect_json),
        "duplicate_subject_frame_attempts": duplicate_attempts.subject,
        "duplicate_sibling_frame_attempts": duplicate_attempts.sibling,
        "duplicate_frame": duplicate_frame.json(),
    });

    // Sequence 2: composed maximize on the floated subject, then float
    // unmaximize back to the exact retained base.
    let mut sequence2 = serde_json::json!({
        "outcome": "composed-max-unmaximize-skipped",
        "blocker": "sequence 1 did not converge the subject to its float base",
    });
    let mut work_area = None;
    if sequence1_proved {
        let max_on_start = mark(&log)?;
        let max_on_press = post("maximize-m")?;
        let maximized = frame(pid, wid)?;
        let max_on_attempts =
            frame_attempts(&mut log, max_on_start, pid, wid, sibling).map_err(log_error)?;
        let max_on_ok = maximized.ax == maximized.cg
            && maximized.ax != base.ax
            && max_on_attempts.subject == 1
            && max_on_attempts.subject_requested == Some(maximized.ax);
        let max_off_start = mark(&log)?;
        let max_off_press = post("maximize-m")?;
        let unmaximized = frame(pid, wid)?;
        let max_off_attempts =
            frame_attempts(&mut log, max_off_start, pid, wid, sibling).map_err(log_error)?;
        let max_off_ok = unmaximized == base
            && max_off_attempts.subject == 1
            && max_off_attempts.subject_requested == Some(base.ax);
        let observe_start = mark(&log)?;
        settle_hold(OBSERVE_MS)?;
        let observe_attempts =
            frame_attempts(&mut log, observe_start, pid, wid, sibling).map_err(log_error)?;
        let observe_frame = frame(pid, wid)?;
        let idle_ok = observe_attempts.subject == 0 && observe_frame == base;
        let proved = max_on_ok && max_off_ok && idle_ok;
        if max_on_ok {
            work_area = Some(maximized.ax);
        }
        sequence2 = serde_json::json!({
            "outcome": if proved { "composed-max-unmaximize" } else { "composed-max-unmaximize-unproved" },
            "base": base.json(),
            "maximize_press": max_on_press, "maximized": maximized.json(),
            "maximize_subject_frame_attempts": max_on_attempts.subject,
            "maximize_sibling_frame_attempts": max_on_attempts.sibling,
            "maximize_requested": max_on_attempts.subject_requested.map(rect_json),
            "unmaximize_press": max_off_press, "unmaximized": unmaximized.json(),
            "unmaximize_subject_frame_attempts": max_off_attempts.subject,
            "unmaximize_sibling_frame_attempts": max_off_attempts.sibling,
            "unmaximize_requested": max_off_attempts.subject_requested.map(rect_json),
            "observe_subject_frame_attempts": observe_attempts.subject,
            "observe_frame": observe_frame.json(),
            "base_restored": max_off_ok,
        });
        if !proved {
            // The subject is still at the maximized frame when the off leg
            // failed; sequence 3 cannot start from a proven float base.
            work_area = None;
        }
    }

    // Sequence 3: directional float half-snap on the floated subject.
    let mut sequence3 = serde_json::json!({
        "outcome": "directional-half-snap-skipped",
        "blocker": "sequence 2 did not restore the subject to its float base",
    });
    if let Some(work) = work_area {
        let half_start = mark(&log)?;
        let move_press = post("move-right")?;
        let half = frame(pid, wid)?;
        let half_attempts =
            frame_attempts(&mut log, half_start, pid, wid, sibling).map_err(log_error)?;
        let gap = half.ax.y.saturating_sub(work.y);
        let expected_half = if (0..=64).contains(&gap) {
            tiler_core::world_directional::float_half_snap_rect(work, gap, "right")
        } else {
            None
        };
        let half_ok = half.ax == half.cg
            && half.ax != base.ax
            && half_attempts.subject == 1
            && half_attempts.subject_requested == Some(half.ax)
            && expected_half == Some(half.ax);
        let observe_start = mark(&log)?;
        settle_hold(OBSERVE_MS)?;
        let observe_attempts =
            frame_attempts(&mut log, observe_start, pid, wid, sibling).map_err(log_error)?;
        let observe_frame = frame(pid, wid)?;
        let idle_ok = observe_attempts.subject == 0 && observe_frame == half;
        sequence3 = serde_json::json!({
            "outcome": if half_ok && idle_ok { "directional-half-snap" } else { "directional-half-snap-unproved" },
            "base": base.json(), "work_area": rect_json(work),
            "move_press": move_press, "half": half.json(),
            "half_subject_frame_attempts": half_attempts.subject,
            "half_sibling_frame_attempts": half_attempts.sibling,
            "half_requested": half_attempts.subject_requested.map(rect_json),
            "expected_half": expected_half.map(rect_json),
            "observe_subject_frame_attempts": observe_attempts.subject,
            "observe_frame": observe_frame.json(),
        });
    }

    Ok(serde_json::json!({
        "outcome": "float-sequences",
        "caller": caller, "wid": wid,
        "sibling_caller": sibling_caller, "sibling_wid": sibling,
        "subject_lifetime": format!("{pid}_{wid}"),
        "base": base.json(), "sibling_before": sibling_before.json(),
        "sequence1": sequence1, "sequence2": sequence2, "sequence3": sequence3,
    }))
}
