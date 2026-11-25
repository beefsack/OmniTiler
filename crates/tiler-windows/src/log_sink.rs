#![forbid(unsafe_code)]
//! Bounded sink for the Windows per-run owner log.
//!
//! The run log is append-only for the lifetime of one owner run: the dev
//! console follows one handle and live proof scripts address records by
//! stable line offsets. Rotation would invalidate both, so the bound is
//! enforced by stopping the log instead of rewriting it:
//!
//! - routine rows are admitted only while the file stays under
//!   [`LOG_FILE_CAP_BYTES`]; the crossing row is replaced by one
//!   `log-capped` marker and every later routine row is dropped;
//! - a single row above [`LOG_LINE_CAP_BYTES`] is replaced by a bounded
//!   `log-line-omitted` marker (JSONL stays parseable);
//! - panic records use [`append_emergency_json`], the one lock-free lane,
//!   and may add one bounded panic row per panicking thread above the cap so
//!   a crash at the limit still leaves evidence. Because that lane cannot
//!   take the sink mutex, its headroom is a per-call bound, not a global
//!   interleaving bound.
//!
//! Every run-log writer (tiling trace rows, pager/tray rows, lifecycle
//! rows) goes through this module. `trace.rs`'s source-audit test keeps
//! that property from regressing: a direct `.append(true)` elsewhere fails
//! the audit unless it is the sink itself or a test fixture.
//!
//! Diagnostic audit (F26): normal-level rows use opaque window tokens and
//! categorical vocabularies, with app identity/pid allowed by decision;
//! titles, content, full paths, SIDs/sessions and raw native handles stay
//! out of normal rows. Trace-only native correlation rows are a separate,
//! explicitly opt-in lane. The sink adds no identity fields of its own.

use std::borrow::Cow;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Hard cap for one run log. Trace runs can approach this in a long
/// session; routine rows stop here and the final on-disk row names why.
pub const LOG_FILE_CAP_BYTES: u64 = 16 * 1024 * 1024;

/// Longest single row admitted. Bounded diagnostic batches are far below
/// this; an oversized row is replaced by a marker instead of being written
/// (a torn/oversized JSONL row would break every reader).
pub const LOG_LINE_CAP_BYTES: usize = 64 * 1024;

/// Headroom above [`LOG_FILE_CAP_BYTES`] checked by the panic lane, so a
/// crash after the routine cap still records a sanitized row. The lane is
/// lock-free: concurrent panic callbacks can each pass the check, so this is
/// a per-call bound rather than a strict global one.
pub const LOG_EMERGENCY_HEADROOM_BYTES: u64 = 64 * 1024;

/// Worst-case bytes of [`capped_marker`] plus its newline. Reserved on every
/// routine check so the ordinary path never formats the marker; the marker
/// itself is built only when the cap is actually crossed.
const CAPPED_MARKER_MAX_BYTES: u64 = 64;

/// Result of one append. `Capped` is not an IO failure: it means the bound
/// did its job and the caller must not treat the run as degraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    Written,
    LineOmitted,
    Capped,
}

/// Per-process sink state. Kept small: production owns one run-log path;
/// tests may create a few temporary paths.
#[derive(Debug, Default)]
pub(crate) struct RunLogSink {
    capped: Vec<PathBuf>,
    /// One reusable line buffer so a mutex-serialized append is a single
    /// `write_all` without a per-row allocation (panic rows still build
    /// their own buffer because that lane cannot borrow this state).
    buffer: Vec<u8>,
}

impl RunLogSink {
    fn is_capped(&self, path: &Path) -> bool {
        self.capped.iter().any(|known| known.as_path() == path)
    }

    fn remember_capped(&mut self, path: &Path) {
        if !self.is_capped(path) {
            self.capped.push(path.to_path_buf());
        }
    }

    /// Append one row under `cap` bytes. Serialize no content here: callers
    /// pass an already bounded line (JSON rows are serialized by
    /// [`append_json`]).
    pub(crate) fn append(
        &mut self,
        path: &Path,
        line: &str,
        cap: u64,
    ) -> io::Result<AppendOutcome> {
        if self.is_capped(path) {
            return Ok(AppendOutcome::Capped);
        }
        let (line, omitted) = bounded_line(line);
        let len = file_len(path)?;
        let needed = len
            .saturating_add(byte_len(line.as_ref()))
            .saturating_add(1);
        // Keep room for the closure marker so the cap is never exceeded by
        // the row that announces it. The reserve is a fixed worst case, so an
        // ordinary append allocates nothing and formats no marker.
        if needed.saturating_add(CAPPED_MARKER_MAX_BYTES) > cap {
            let marker = capped_marker(cap);
            if len.saturating_add(byte_len(&marker)).saturating_add(1) <= cap {
                self.buffer.clear();
                self.buffer.extend_from_slice(marker.as_bytes());
                self.buffer.push(b'\n');
                append_buffer(path, &self.buffer)?;
            }
            self.remember_capped(path);
            return Ok(AppendOutcome::Capped);
        }
        self.buffer.clear();
        self.buffer.extend_from_slice(line.as_bytes());
        self.buffer.push(b'\n');
        append_buffer(path, &self.buffer)?;
        Ok(if omitted {
            AppendOutcome::LineOmitted
        } else {
            AppendOutcome::Written
        })
    }
}

/// Sink for the run log of this process. A mutex serializes the length
/// check and append so the bound holds when owner/render/tray threads log
/// concurrently; the critical section is filesystem IO only and never waits
/// on another lock or native call.
static SINK: std::sync::Mutex<Option<RunLogSink>> = std::sync::Mutex::new(None);

fn with_sink<T>(f: impl FnOnce(&mut RunLogSink) -> T) -> T {
    let mut guard = SINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(guard.get_or_insert_with(RunLogSink::default))
}

/// Append one already-formatted row, bounded by the run-log cap. Best-effort
/// at call sites that ignore the result; `Capped` is a normal end state.
pub fn append_line(path: &Path, line: &str) -> io::Result<AppendOutcome> {
    with_sink(|sink| sink.append(path, line, LOG_FILE_CAP_BYTES))
}

/// Append one JSON row, bounded by row and file caps.
pub fn append_json(path: &Path, value: &serde_json::Value) -> io::Result<AppendOutcome> {
    let line = match serde_json::to_string(value) {
        Ok(line) => line,
        Err(_) => "{\"event\":\"log-serialize-failed\"}".to_owned(),
    };
    append_line(path, &line)
}

/// Create the run log with its first row. Refuses a preexisting path exactly
/// like the previous lifecycle writer: one owner run never overwrites
/// another run's file.
pub fn create_line(path: &Path, line: &str) -> io::Result<()> {
    let (line, _) = bounded_line(line);
    let mut buffer = Vec::with_capacity(line.len() + 1);
    buffer.extend_from_slice(line.as_bytes());
    buffer.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&buffer)?;
    file.flush()
}

/// Lock-free panic lane. Used only by the sanitized panic hook: it must not
/// take the sink mutex (the panicking thread or another thread may hold it).
/// Each call checks [`LOG_FILE_CAP_BYTES`] plus
/// [`LOG_EMERGENCY_HEADROOM_BYTES`] before appending; concurrent panic
/// callbacks can race, so the check is a per-call bound.
pub fn append_emergency_json(path: &Path, value: &serde_json::Value) -> io::Result<AppendOutcome> {
    let line = match serde_json::to_string(value) {
        Ok(line) => line,
        Err(_) => return Ok(AppendOutcome::Capped),
    };
    append_emergency_line_with_cap(
        path,
        &line,
        LOG_FILE_CAP_BYTES.saturating_add(LOG_EMERGENCY_HEADROOM_BYTES),
    )
}

pub(crate) fn append_emergency_line_with_cap(
    path: &Path,
    line: &str,
    cap: u64,
) -> io::Result<AppendOutcome> {
    let (line, omitted) = bounded_line(line);
    let len = file_len(path)?;
    if len
        .saturating_add(byte_len(line.as_ref()))
        .saturating_add(1)
        > cap
    {
        return Ok(AppendOutcome::Capped);
    }
    append_raw(path, line.as_ref())?;
    Ok(if omitted {
        AppendOutcome::LineOmitted
    } else {
        AppendOutcome::Written
    })
}

fn bounded_line(line: &str) -> (Cow<'_, str>, bool) {
    if line.len() > LOG_LINE_CAP_BYTES {
        (
            Cow::Owned(format!(
                "{{\"event\":\"log-line-omitted\",\"bytes\":{},\"cap_bytes\":{}}}",
                line.len(),
                LOG_LINE_CAP_BYTES
            )),
            true,
        )
    } else {
        (Cow::Borrowed(line), false)
    }
}

fn capped_marker(cap: u64) -> String {
    format!("{{\"event\":\"log-capped\",\"cap_bytes\":{cap}}}")
}

fn file_len(path: &Path) -> io::Result<u64> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn byte_len(text: &str) -> u64 {
    u64::try_from(text.len()).unwrap_or(u64::MAX)
}

fn append_raw(path: &Path, line: &str) -> io::Result<()> {
    let mut buffer = Vec::with_capacity(line.len() + 1);
    buffer.extend_from_slice(line.as_bytes());
    buffer.push(b'\n');
    append_buffer(path, &buffer)
}

/// One complete row in one `write_all`, so a lock-free panic row cannot
/// interleave inside a normal row (and vice versa).
fn append_buffer(path: &Path, buffer: &[u8]) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(buffer)?;
    file.flush()
}
