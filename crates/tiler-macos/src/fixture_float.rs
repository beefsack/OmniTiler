#![forbid(unsafe_code)]

//! Bounded same-window float placement evidence through the fixture tap.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::config::ProbeModifier;
use crate::fixture_leg::settle_hold;
use crate::fixture_post::{FixtureDriver, LiveTransport};

const SETTLE_MS: u64 = 1_500;

fn frame(pid: u32, wid: u32) -> Result<serde_json::Value, (i32, String)> {
    let ax = crate::fixture_sample::read_joined_frame(pid, wid)
        .map_err(|e| (1, e))?
        .frame;
    let cg = crate::ffi::cg_window_list_basic()
        .map_err(|e| (1, e))?
        .into_iter()
        .find(|entry| entry.wid == wid && entry.pid == pid && entry.onscreen)
        .ok_or_else(|| (1, "fixture float evidence CG join is absent".to_owned()))?;
    Ok(serde_json::json!({"ax": ax, "cg": cg.bounds}))
}

fn frame_attempts(
    log: &mut File,
    start: u64,
    pid: u32,
    wid: u32,
    sibling: u32,
) -> std::io::Result<(usize, usize)> {
    let end = log.metadata()?.len();
    let Some(len) = end.checked_sub(start) else {
        return Err(std::io::Error::other("fixture product log was truncated"));
    };
    log.seek(SeekFrom::Start(start))?;
    let subject_prefix = format!("tiler-macos tile frame-set pid={pid} wid={wid} ");
    let sibling_prefix = format!("tiler-macos tile frame-set pid={pid} wid={sibling} ");
    let mut reader = BufReader::new(log.take(len));
    let mut line = String::new();
    let (mut subject, mut other) = (0, 0);
    while reader.read_line(&mut line)? != 0 {
        subject += usize::from(line.starts_with(&subject_prefix));
        other += usize::from(line.starts_with(&sibling_prefix));
        line.clear();
    }
    Ok((subject, other))
}

fn log_error(error: std::io::Error) -> (i32, String) {
    (1, format!("fixture float evidence log: {error}"))
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
    let (subject_frame_attempts, sibling_frame_attempts) =
        frame_attempts(&mut log, first_start, pid, wid, sibling).map_err(log_error)?;
    let untile_proved = sibling_before != sibling_after && sibling_frame_attempts > 0;
    if !untile_proved || floated != base || subject_frame_attempts != 0 {
        return Ok(serde_json::json!({
            "outcome": "float-placement-unproved", "first_press": first,
            "base": base, "floated": floated, "sibling_before": sibling_before,
            "sibling_after": sibling_after, "subject_frame_attempts": subject_frame_attempts,
            "sibling_frame_attempts": sibling_frame_attempts,
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
    let (refloat_subject_attempts, refloat_sibling_attempts) =
        frame_attempts(&mut log, refloat_start, pid, wid, sibling).map_err(log_error)?;
    let restored = rotated != base && remembered == base && refloat_subject_attempts == 1;
    let sibling_reflowed = refloat_sibling != rotated_sibling && refloat_sibling_attempts > 0;
    Ok(serde_json::json!({
        "outcome": if restored && sibling_reflowed { "float-placement" } else { "float-placement-unproved" },
        "caller": caller, "wid": wid,
        "first_press": first, "retile_press": retile, "rotate_press": rotate,
        "refloat_press": refloat, "base": base, "floated": floated,
        "sibling_before": sibling_before, "sibling_after": sibling_after,
        "subject_frame_attempts": subject_frame_attempts,
        "sibling_frame_attempts": sibling_frame_attempts, "rotated_tile": rotated,
        "refloat": remembered, "refloat_sibling_reflowed": sibling_reflowed,
        "refloat_subject_frame_attempts": refloat_subject_attempts,
        "refloat_sibling_frame_attempts": refloat_sibling_attempts,
        "remembered_frame_restored": restored,
    }))
}
