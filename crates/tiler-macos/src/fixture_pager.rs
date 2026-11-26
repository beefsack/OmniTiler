#![forbid(unsafe_code)]

//! Observe the product indicator rebuilding across two controller-owned Spaces.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::fixture_leg::settle_hold;
use crate::live_leg::{LiveLeg, LiveLegTransport};
use crate::space_probe::NativeSpaces;

#[derive(Serialize)]
struct Rebuild {
    current: usize,
    rows: usize,
    occupied: usize,
    unknown: usize,
    generation: u64,
}

fn rebuild(line: &str) -> Option<Rebuild> {
    let fields = line.strip_prefix("tiler-macos dev indicator ")?;
    let value = |key: &str| {
        fields.split_ascii_whitespace().find_map(|field| {
            let (name, value) = field.split_once('=')?;
            (name == key).then_some(value)
        })
    };
    (value("complete")? == "true").then_some(())?;
    Some(Rebuild {
        current: value("current")?.parse().ok()?,
        rows: value("rows")?.parse().ok()?,
        occupied: value("occupied")?.parse().ok()?,
        unknown: value("unknown")?.parse().ok()?,
        generation: value("generation")?.parse().ok()?,
    })
}

fn wait_for_rebuild(
    log: &mut File,
    offset: u64,
    position: usize,
    newer_than: u64,
) -> Result<Rebuild, (i32, String)> {
    log.seek(SeekFrom::Start(offset)).map_err(log_error)?;
    let mut reader = BufReader::new(log);
    let mut line = String::new();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if Instant::now() >= deadline {
            return Err((
                1,
                format!("fixture pager: no newer rebuild for position {position}"),
            ));
        }
        if reader.read_line(&mut line).map_err(log_error)? == 0 {
            settle_hold(100)?;
        } else if line.ends_with('\n') {
            if let Some(rebuild) = rebuild(&line)
                && rebuild.current == position
                && rebuild.generation > newer_than
            {
                return Ok(rebuild);
            }
            line.clear();
        }
    }
}

fn log_error(error: std::io::Error) -> (i32, String) {
    (1, format!("fixture pager product log: {error}"))
}

/// Verify each switch both through the native current SID and through a
/// newer real indicator rebuild. This proves the AppKit input model, not pixels.
pub(crate) fn run(
    leg: &mut LiveLeg<'_, LiveLegTransport>,
    source: u64,
    destination: u64,
    product_log: &Path,
) -> Result<serde_json::Value, (i32, String)> {
    let mut log = File::open(product_log).map_err(log_error)?;
    let before = leg.transport_mut().space_snapshot().map_err(|e| (1, e))?;
    let [row] = before.displays.as_slice() else {
        return Err((1, "fixture pager requires one readable display".to_owned()));
    };
    if row.current != source || source == destination {
        return Err((1, "fixture pager source is not current".to_owned()));
    }
    let position = |sid| {
        row.spaces
            .iter()
            .position(|id| *id == sid)
            .map(|index| index + 1)
    };
    let source_position =
        position(source).ok_or_else(|| (1, "fixture pager source is absent".to_owned()))?;
    let destination_position = position(destination)
        .ok_or_else(|| (1, "fixture pager destination is absent".to_owned()))?;
    let first = wait_for_rebuild(&mut log, 0, source_position, 0)?;
    let mut switch = |target, expected, position, generation| {
        let fresh = leg.transport_mut().space_snapshot().map_err(|e| (1, e))?;
        if !matches!(fresh.displays.as_slice(), [current] if current.spaces == row.spaces && current.current == expected)
        {
            return Err((1, "fixture pager native row drifted".to_owned()));
        }
        let offset = log.metadata().map_err(log_error)?.len();
        let report = leg
            .transport_mut()
            .space_goto(0, target)
            .map_err(|e| (1, e))?;
        if let Some(shortfall) = report.shortfall_text("fixture pager switch") {
            return Err((1, shortfall));
        }
        let after = leg.transport_mut().space_snapshot().map_err(|e| (1, e))?;
        if !matches!(after.displays.as_slice(), [current] if current.spaces == row.spaces && current.current == target)
        {
            return Err((1, "fixture pager switch did not arrive".to_owned()));
        }
        wait_for_rebuild(&mut log, offset, position, generation)
    };
    let away = switch(destination, source, destination_position, first.generation)?;
    let returned = switch(source, destination, source_position, away.generation)?;
    Ok(
        serde_json::json!({"outcome":"pager-switched", "starting":first, "away":away, "returned":returned}),
    )
}
