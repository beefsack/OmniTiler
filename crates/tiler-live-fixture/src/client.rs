//! Thin `send` client: forwarding, compact printing, event-driven wait.
//!
//! Wire behavior is unchanged: replies are matched by `request_id`, events
//! interleave freely, and snapshots on the wire keep every optional field
//! (unknown is null). `--compact` only strips null-valued optional fields
//! from locally printed snapshots; the in-window rendering and the protocol
//! keep the full form.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::protocol::MAX_LINE_BYTES;
use crate::transport::{FrameRead, read_bounded_line};

/// Observed fields a `--expect` predicate may mention. These are exactly the
/// real `Observed` fields; anything else (e.g. `decorations`, `description`,
/// `settled`) is rejected as an unsupported field. `focused` reads the real
/// `observed.focused` focus readback; focus requests themselves stay one-shot
/// and are never replayed.
pub const EXPECT_ALLOWED_FIELDS: &[&str] = &[
    "size_px",
    "position_outer_px",
    "mode",
    "visible",
    "focused",
    "scale_factor",
    "logical_size_px",
];

/// Caller-id to nonempty observed-field subset.
pub type ExpectMap = BTreeMap<String, BTreeMap<String, serde_json::Value>>;

/// Parse and validate `--expect` JSON.
///
/// Shape: `{"id":{"size_px":[W,H],"focused":true,...}}`. Every id must pass
/// id validation, map to a nonempty object of allowed observed fields with
/// correctly shaped non-null values. Unknown fields, null values, empty maps,
/// and bad shapes are rejected. Unknown (null) observed never matches, so a
/// null expectation could never succeed and is rejected here.
pub fn parse_expect(raw: &str) -> Result<ExpectMap, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid --expect JSON: {e}"))?;
    let obj = value.as_object().ok_or_else(|| {
        "--expect must be a JSON object mapping ids to field predicates".to_string()
    })?;
    if obj.is_empty() {
        return Err("--expect must name at least one window id".to_string());
    }
    let mut out = ExpectMap::new();
    for (id, pred) in obj {
        crate::protocol::validate_id(id).map_err(|e| format!("invalid --expect id '{id}': {e}"))?;
        let fields = pred
            .as_object()
            .ok_or_else(|| format!("--expect['{id}'] must be an object of observed fields"))?;
        if fields.is_empty() {
            return Err(format!(
                "--expect['{id}'] must name at least one observed field"
            ));
        }
        let mut checked = BTreeMap::new();
        for (field, want) in fields {
            if !EXPECT_ALLOWED_FIELDS.contains(&field.as_str()) {
                return Err(format!(
                    "unsupported field '{field}' in --expect['{id}']: only {} are allowed",
                    EXPECT_ALLOWED_FIELDS.join(",")
                ));
            }
            if want.is_null() {
                return Err(format!(
                    "null never matches: --expect['{id}']['{field}'] must be a concrete value"
                ));
            }
            validate_expect_value(field, want)
                .map_err(|e| format!("--expect['{id}']['{field}']: {e}"))?;
            checked.insert(field.clone(), want.clone());
        }
        out.insert(id.clone(), checked);
    }
    Ok(out)
}

fn validate_expect_value(field: &str, want: &serde_json::Value) -> Result<(), String> {
    match field {
        "size_px" | "logical_size_px" => {
            let pair = want.as_array().ok_or_else(|| "must be [W,H]".to_string())?;
            if pair.len() != 2 {
                return Err("must be [W,H]".to_string());
            }
            for n in pair {
                let v = n
                    .as_u64()
                    .ok_or_else(|| "dimensions must be positive integers".to_string())?;
                if v == 0 || v > u32::MAX as u64 {
                    return Err("dimensions must be positive integers".to_string());
                }
            }
            Ok(())
        }
        "position_outer_px" => {
            let pair = want.as_array().ok_or_else(|| "must be [X,Y]".to_string())?;
            if pair.len() != 2 {
                return Err("must be [X,Y]".to_string());
            }
            for n in pair {
                if n.as_i64().is_none() {
                    return Err("coordinates must be integers".to_string());
                }
            }
            Ok(())
        }
        "mode" => {
            let s = want
                .as_str()
                .ok_or_else(|| "mode must be a string".to_string())?;
            match s {
                "windowed"
                | "maximized"
                | "minimized"
                | "borderless_fullscreen"
                | "exclusive_fullscreen" => Ok(()),
                _ => Err(format!("unknown mode '{s}'")),
            }
        }
        "visible" | "focused" => {
            if want.as_bool().is_none() {
                return Err("must be a boolean".to_string());
            }
            Ok(())
        }
        "scale_factor" => {
            let n = want
                .as_f64()
                .ok_or_else(|| "scale_factor must be a number".to_string())?;
            if !n.is_finite() || n <= 0.0 {
                return Err("scale_factor must be a positive finite number".to_string());
            }
            Ok(())
        }
        _ => Err(format!("unsupported field '{field}'")),
    }
}

/// Strip null-valued optional fields from printed snapshots only.
///
/// Keeps the envelope (`v`, `request_id`, `ok`, `result`/`error`), per-window
/// `ok`/`error`, per-field `fields` outcomes, and the
/// requested/configured/observed distinction plus `unsupported`/`settled`.
/// Only null entries inside snapshot objects (top level like `scale_factor`,
/// and inside `requested`/`configured`/`observed`) are removed, so a compacted
/// snapshot that reads `{}` for `observed` still shows the key. The wire and
/// the in-window full snapshot are unchanged.
pub fn compact_line(line: &str) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return line.to_string(),
    };
    compact_value(&mut value);
    serde_json::to_string(&value).unwrap_or_else(|_| line.to_string())
}

fn is_snapshot_object(map: &serde_json::Map<String, serde_json::Value>) -> bool {
    map.contains_key("observed") && (map.contains_key("settled") || map.contains_key("requested"))
}

fn compact_snapshot_object(map: &mut serde_json::Map<String, serde_json::Value>) {
    map.retain(|_, v| !v.is_null());
    for key in ["requested", "configured", "observed"] {
        if let Some(inner) = map.get_mut(key).and_then(|v| v.as_object_mut()) {
            inner.retain(|_, v| !v.is_null());
        }
    }
}

fn compact_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if is_snapshot_object(map) {
                compact_snapshot_object(map);
            }
            for child in map.values_mut() {
                compact_value(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                compact_value(item);
            }
        }
        _ => {}
    }
}

/// Generate an internal request id not present in `used`.
pub fn unused_request_id(base: &str, used: &HashSet<String>) -> String {
    if !used.contains(base) {
        return base.to_string();
    }
    let mut n = 1u32;
    loop {
        let candidate = format!("{base}-{n}");
        if !used.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Numeric-tolerant equality for predicates: integers and floats compare by
/// value (`1` equals `1.0`), arrays compare element-wise, everything else by
/// JSON equality.
fn values_equal(want: &serde_json::Value, got: &serde_json::Value) -> bool {
    match (want, got) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            match (a.as_f64(), b.as_f64()) {
                (Some(x), Some(y)) => (x - y).abs() < 1e-9,
                _ => a == b,
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
        }
        _ => want == got,
    }
}

/// Pure wait bookkeeping: pending replies, latest full snapshots per expected
/// id, and a fail-fast reason. No I/O, so it unit-tests without sockets.
///
/// Matching uses only the latest snapshot per id, never a latched historical
/// match: a newer state event that stops matching un-matches the id. `settled`
/// is ignored (geometry-only); only `observed` fields are compared, and
/// unknown (null/missing) observed never matches.
#[derive(Debug)]
pub struct WaitState {
    expect: ExpectMap,
    pending: HashSet<String>,
    latest: HashMap<String, serde_json::Value>,
    failed: Option<String>,
}

impl WaitState {
    pub fn new(expect: ExpectMap, pending_ids: Vec<String>) -> Self {
        Self {
            expect,
            pending: pending_ids.into_iter().collect(),
            latest: HashMap::new(),
            failed: None,
        }
    }

    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    pub fn latest(&self) -> &HashMap<String, serde_json::Value> {
        &self.latest
    }

    fn fail(&mut self, reason: String) {
        if self.failed.is_none() {
            self.failed = Some(reason);
        }
    }

    /// Note one incoming line (already parsed). Returns true when the wait is
    /// terminally failed and the caller should stop promptly.
    pub fn note_value(&mut self, value: &serde_json::Value) -> bool {
        if self.failed.is_some() {
            return true;
        }
        if let Some(request_id) = value.get("request_id").and_then(|r| r.as_str()) {
            self.note_reply(value, request_id);
        } else {
            self.note_event(value);
        }
        self.failed.is_some()
    }

    fn note_reply(&mut self, value: &serde_json::Value, request_id: &str) {
        self.pending.remove(request_id);
        let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok {
            let detail = value
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("reply failed");
            self.fail(format!("request '{request_id}' failed: {detail}"));
            return;
        }
        // Extract per-window outcomes; any expected id that errors, goes
        // missing, or succeeds without a snapshot (a close) fails promptly
        // instead of silently succeeding later.
        if let Some(result) = value.get("result") {
            self.note_window_list(result.get("windows"));
            if let Some(items) = result.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    if item.get("ok").and_then(|v| v.as_bool()) == Some(false)
                        && item.get("error").is_some()
                    {
                        // Batch item error without windows (e.g. bad shape).
                        // Only fail when it concerns an expected window or is
                        // a top-level payload failure; conservative: fail.
                        // Batch item errors for unrelated ids still mean a
                        // payload failed, so fail promptly.
                        let op = item.get("op").and_then(|v| v.as_str()).unwrap_or("?");
                        let detail = item
                            .get("error")
                            .and_then(|e| e.as_str())
                            .unwrap_or("batch item failed");
                        self.fail(format!("batch item '{op}' failed: {detail}"));
                        return;
                    }
                    self.note_window_list(item.get("windows"));
                    if self.failed.is_some() {
                        return;
                    }
                }
            }
        }
    }

    fn note_window_list(&mut self, windows: Option<&serde_json::Value>) {
        let Some(list) = windows.and_then(|v| v.as_array()) else {
            return;
        };
        for w in list {
            let Some(id) = w.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            if !self.expect.contains_key(id) {
                continue;
            }
            let ok = w.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            if !ok {
                let detail = w
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("window failed");
                self.fail(format!("window '{id}' failed: {detail}"));
                return;
            }
            match w.get("snapshot") {
                Some(snap) => {
                    self.latest.insert(id.to_string(), snap.clone());
                }
                None => {
                    // Successful close (or unexpected shape) for an expected
                    // window: it is gone, so the wait must fail, not succeed.
                    self.fail(format!("window '{id}' closed before matching"));
                    return;
                }
            }
        }
    }

    fn note_event(&mut self, value: &serde_json::Value) {
        let Some(event) = value.get("event").and_then(|e| e.as_str()) else {
            return;
        };
        match event {
            "subscribed" => {}
            "created" | "state" => {
                let Some(id) = value.get("id").and_then(|v| v.as_str()) else {
                    return;
                };
                if !self.expect.contains_key(id) {
                    return;
                }
                if let Some(snap) = value.get("snapshot") {
                    self.latest.insert(id.to_string(), snap.clone());
                }
            }
            "closed" => {
                let Some(id) = value.get("id").and_then(|v| v.as_str()) else {
                    return;
                };
                if self.expect.contains_key(id) {
                    self.fail(format!("window '{id}' closed before matching"));
                }
            }
            "error" => {
                let detail = value
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("server error");
                let id = value.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if id.is_empty() {
                    self.fail(format!("server error: {detail}"));
                } else if self.expect.contains_key(id) {
                    self.fail(format!("window '{id}' error: {detail}"));
                } else {
                    self.fail(format!("server error: {detail}"));
                }
            }
            _ => {}
        }
    }

    /// True only when every payload/baseline reply arrived and every expected
    /// id's latest observed subset equals the predicate. Unknown (null or
    /// missing) observed never matches.
    pub fn is_matched(&self) -> bool {
        if self.failed.is_some() || !self.pending.is_empty() {
            return false;
        }
        for (id, fields) in &self.expect {
            let Some(snap) = self.latest.get(id) else {
                return false;
            };
            let Some(observed) = snap.get("observed").and_then(|v| v.as_object()) else {
                return false;
            };
            for (field, want) in fields {
                match observed.get(field) {
                    Some(got) if !got.is_null() && values_equal(want, got) => {}
                    _ => return false,
                }
            }
        }
        true
    }
}

/// Parse the payload's top-level `op` without full request validation.
fn payload_op(payload: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v.get("op").and_then(|op| op.as_str()).map(str::to_string))
}

/// Batch item ops inside a payload (empty when not a batch or unparsable).
fn batch_item_ops(payload: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
        return vec![];
    };
    if v.get("op").and_then(|o| o.as_str()) != Some("batch") {
        return vec![];
    }
    v.get("items")
        .and_then(|i| i.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|it| it.get("op").and_then(|o| o.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Reject explicit subscription lifecycle ops when waiting: the waiter owns
/// the connection's subscription (`state`/`created`/`closed`/`error`) and a
/// caller subscribe/unsubscribe/shutdown would steal or end it.
fn reject_wait_lifecycle(payloads: &[String]) -> Result<(), String> {
    for payload in payloads {
        if let Some(op) = payload_op(payload) {
            if matches!(op.as_str(), "subscribe" | "unsubscribe" | "shutdown") {
                return Err(format!(
                    "op '{op}' is not permitted with --wait-ms: the waiter owns the subscription"
                ));
            }
            if op == "batch" {
                for item_op in batch_item_ops(payload) {
                    if matches!(item_op.as_str(), "subscribe" | "unsubscribe" | "shutdown") {
                        return Err(format!(
                            "op '{item_op}' is not permitted inside batch with --wait-ms"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Client-side wait state for one `send` connection: which replies are
/// outstanding and how many subscription events have been seen. Pure (no
/// I/O), so it unit-tests without windows.
#[derive(Debug)]
pub struct ReplyTracker {
    pending: HashMap<String, usize>,
    want_events: Option<u64>,
    seen_events: u64,
    follow: bool,
    subscribed: bool,
}

impl ReplyTracker {
    pub fn new(
        request_ids: Vec<String>,
        events: Option<u64>,
        follow: bool,
        subscribed: bool,
    ) -> Self {
        let mut pending = HashMap::new();
        for id in request_ids {
            *pending.entry(id).or_default() += 1;
        }
        Self {
            pending,
            want_events: events,
            seen_events: 0,
            follow,
            subscribed,
        }
    }

    /// Note one incoming line. Replies carry their `request_id`; anything
    /// else is an event or the `subscribed` handshake marker. Events count
    /// even while replies are still outstanding (same-connection
    /// subscribe+mutate interleaves broadcasts with replies; clients tell
    /// them apart by shape, never by line position). The handshake marker
    /// is never counted. Returns true when the client is done.
    pub fn note_line(&mut self, request_id: Option<&str>, is_marker: bool) -> bool {
        match request_id {
            Some(id) => {
                if let Some(count) = self.pending.get_mut(id) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.pending.remove(id);
                    }
                }
            }
            None if !is_marker => {
                self.seen_events += 1;
            }
            None => {}
        }
        if !self.pending.is_empty() {
            return false;
        }
        if self.follow {
            return false;
        }
        match self.want_events {
            // `--events 0` is done as soon as the replies are in: no hang.
            Some(want) => self.seen_events >= want,
            None => !self.subscribed,
        }
    }
}

#[cfg(any(unix, windows))]
pub fn send_lines(
    socket: &Path,
    nonce: &str,
    payloads: &[String],
    events: Option<u64>,
    follow: bool,
    compact: bool,
) -> Result<(), String> {
    use std::io::BufReader;

    let mut stream =
        crate::transport::connect(socket).map_err(|e| format!("connect failed: {e}"))?;
    // Inject the nonce into every request that lacks one.
    let mut framed = Vec::with_capacity(payloads.len());
    for payload in payloads {
        framed.push(inject_nonce(payload, nonce)?);
    }
    let subscribed = framed.iter().any(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|v| {
                v.get("op")
                    .and_then(|op| op.as_str())
                    .map(|op| op == "subscribe")
            })
            .unwrap_or(false)
    });
    for line in &framed {
        crate::transport::write_line(&mut stream, line)
            .map_err(|e| format!("write failed: {e}"))?;
    }
    if follow {
        // A deliberate indefinite subscription: no read deadline. The user
        // ends it with Ctrl-C; EOF ends it from the server side.
        crate::transport::set_read_timeout(&stream, None)
            .map_err(|e| format!("timeout setup failed: {e}"))?;
    } else {
        // Request/response deadline: a wedged server must not hang scripts
        // forever. Use --follow for deliberate indefinite idling.
        crate::transport::set_read_timeout(&stream, Some(std::time::Duration::from_secs(30)))
            .map_err(|e| format!("timeout setup failed: {e}"))?;
    }
    let mut reader = BufReader::new(
        crate::transport::try_clone_stream(&stream).map_err(|e| format!("clone failed: {e}"))?,
    );
    let mut scratch = Vec::new();
    // Replies carry request_id; events and markers do not. Replies and events
    // interleave on the wire, so completion is tracked by request id, never by
    // line position.
    let request_ids: Vec<String> = framed
        .iter()
        .filter_map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| {
                    v.get("request_id")
                        .and_then(|r| r.as_str())
                        .map(str::to_string)
                })
        })
        .collect();
    let mut tracker = ReplyTracker::new(request_ids, events, follow, subscribed);
    loop {
        let line = match read_bounded_line(&mut reader, &mut scratch) {
            Ok(FrameRead::Line(line)) => line,
            Ok(FrameRead::Eof) => break,
            Err(e) => return Err(format!("read failed: {e:?}")),
        };
        if compact {
            println!("{}", compact_line(&line));
        } else {
            println!("{line}");
        }
        let value: serde_json::Value =
            serde_json::from_str(&line).unwrap_or(serde_json::Value::Null);
        let reply_id = value
            .get("request_id")
            .and_then(|r| r.as_str())
            .map(str::to_string);
        let is_marker = value.get("event").and_then(|e| e.as_str()) == Some("subscribed");
        if tracker.note_line(reply_id.as_deref(), is_marker) {
            break;
        }
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub fn send_lines(
    _: &Path,
    _: &str,
    _: &[String],
    _: Option<u64>,
    _: bool,
    _: bool,
) -> Result<(), String> {
    Err("local transport is not supported on this OS".to_string())
}

/// Outcome of one deadline-aware frame read. `Timeout` means the absolute
/// wait deadline passed with no complete frame; it is distinct from I/O
/// errors so the caller reports a timeout rather than a read failure.
#[cfg(any(unix, windows))]
enum WaitRead {
    Line(String),
    Eof,
    Timeout,
    Err(crate::transport::FrameError),
}

/// Line reader that enforces one absolute monotonic deadline across every
/// blocking read.
///
/// `transport::read_bounded_line` loops on `fill_buf` with whatever timeout
/// the socket currently carries, so a peer trickling one byte at a time
/// without a newline resets the per-syscall timeout forever and defeats an
/// outer deadline. This reader instead recomputes the remaining duration from
/// the absolute deadline before each blocking `read` and applies it as the
/// socket read timeout, then blocks. No sleep/poll loop: each iteration
/// either returns a complete frame or blocks in exactly one `read` with the
/// remaining duration. Buffered partial bytes and the 1 MiB frame cap are
/// preserved with the same semantics as the transport reader (the cap covers
/// the frame including its newline; oversize callers must close).
#[cfg(unix)]
struct DeadlineReader {
    stream: std::os::unix::net::UnixStream,
    /// Buffered bytes not yet returned. May hold the tail of a coalesced
    /// chunk after the leading frame is extracted, so a buffered newline is
    /// possible and every extracted frame is length-checked, not just the
    /// first newline of each read.
    buf: Vec<u8>,
}

#[cfg(unix)]
impl DeadlineReader {
    fn new(stream: std::os::unix::net::UnixStream) -> Self {
        Self {
            stream,
            buf: Vec::new(),
        }
    }

    fn read_frame(&mut self, deadline: std::time::Instant) -> WaitRead {
        use std::io::Read;
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                // Cap covers the frame including its newline. Residual tail
                // bytes of an earlier coalesced chunk can assemble an
                // oversize later frame, so validate on extract as well as on
                // append.
                if pos + 1 > MAX_LINE_BYTES {
                    return WaitRead::Err(crate::transport::FrameError::TooLong);
                }
                let mut line: Vec<u8> = self.buf.drain(..pos + 1).collect();
                line.pop();
                if line.ends_with(b"\r") {
                    line.pop();
                }
                match String::from_utf8(line) {
                    Ok(s) => return WaitRead::Line(s),
                    Err(_) => return WaitRead::Err(crate::transport::FrameError::InvalidUtf8),
                }
            }
            if self.buf.len() > MAX_LINE_BYTES {
                return WaitRead::Err(crate::transport::FrameError::TooLong);
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return WaitRead::Timeout;
            }
            if let Err(e) = self.stream.set_read_timeout(Some(remaining)) {
                return WaitRead::Err(crate::transport::FrameError::Io(e.to_string()));
            }
            let mut tmp = [0u8; 4096];
            match self.stream.read(&mut tmp) {
                Ok(0) => {
                    if self.buf.is_empty() {
                        return WaitRead::Eof;
                    }
                    return WaitRead::Err(crate::transport::FrameError::Io(
                        "connection closed mid-line".to_string(),
                    ));
                }
                Ok(n) => {
                    let chunk = &tmp[..n];
                    match chunk.iter().position(|&b| b == b'\n') {
                        Some(j) => {
                            // Cap covers the frame including its newline,
                            // matching the transport reader.
                            if self.buf.len().saturating_add(j + 1) > MAX_LINE_BYTES {
                                return WaitRead::Err(crate::transport::FrameError::TooLong);
                            }
                            self.buf.extend_from_slice(chunk);
                        }
                        None => {
                            if self.buf.len().saturating_add(n) > MAX_LINE_BYTES {
                                return WaitRead::Err(crate::transport::FrameError::TooLong);
                            }
                            self.buf.extend_from_slice(chunk);
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    // The per-read timeout fired: report the absolute-deadline
                    // timeout only when the deadline truly passed; otherwise
                    // re-block with the fresh remaining duration.
                    if std::time::Instant::now() >= deadline {
                        return WaitRead::Timeout;
                    }
                }
                Err(e) => {
                    return WaitRead::Err(crate::transport::FrameError::Io(e.to_string()));
                }
            }
        }
    }
}

/// Outcome of one deadline-aware frame write.
#[cfg(any(unix, windows))]
#[derive(Debug)]
enum WriteError {
    Timeout,
    Io(String),
}

/// Write one JSON-lines frame (`line` plus its `\n`) before the absolute
/// deadline, recomputing the remaining duration before each blocking `write`.
///
/// `write_all` loops internally on the once-set socket timeout, so a
/// slow-draining peer resets the per-syscall timeout on every partial write
/// and defeats an outer deadline the same way trickled reads did. This loop
/// instead applies the fresh remaining duration before each single `write`
/// and blocks at most that long. No sleep/poll loop. Normal (non-wait)
/// transport is untouched.
#[cfg(unix)]
fn write_line_deadline(
    stream: &mut std::os::unix::net::UnixStream,
    line: &str,
    deadline: std::time::Instant,
) -> Result<(), WriteError> {
    use std::io::Write;
    let mut buf = line.as_bytes().to_vec();
    buf.push(b'\n');
    let mut pos = 0;
    while pos < buf.len() {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(WriteError::Timeout);
        }
        stream
            .set_write_timeout(Some(remaining))
            .map_err(|e| WriteError::Io(e.to_string()))?;
        match stream.write(&buf[pos..]) {
            Ok(0) => return Err(WriteError::Io("write returned zero".to_string())),
            Ok(n) => pos += n,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if std::time::Instant::now() >= deadline {
                    return Err(WriteError::Timeout);
                }
            }
            Err(e) => return Err(WriteError::Io(e.to_string())),
        }
    }
    Ok(())
}

/// Event-driven bounded wait: subscribe, run payloads, baseline-get the
/// expected ids, then consume replies/events until the latest observations
/// match or one absolute deadline expires.
///
/// No polling: the deadline is a single monotonic `Instant` for the whole
/// wait. Reads go through a deadline-aware reader that recomputes the
/// remaining duration before each blocking `read`, so a chatty stream of
/// partial frames, coalesced lines, or unrelated events cannot reset the
/// deadline; each iteration either returns a complete frame or blocks once
/// with the remaining duration. Writes go through a deadline-aware inner
/// write loop that re-applies the remaining duration before each partial
/// `write`, so a slow-draining peer cannot reset the deadline either. The
/// deadline exists because hosts may clamp/ignore requests or withhold
/// state, so scripts must not hang.
#[cfg(any(unix, windows))]
#[allow(clippy::too_many_arguments)]
pub fn send_wait(
    socket: &Path,
    nonce: &str,
    payloads: &[String],
    wait_ms: u64,
    expect: &ExpectMap,
    compact: bool,
) -> Result<(), String> {
    reject_wait_lifecycle(payloads)?;
    if expect.is_empty() {
        return Err("--expect must name at least one window id".to_string());
    }
    // Payloads need explicit request ids when waiting: completion and the
    // internal subscribe/baseline ids are tracked by id, and a missing id
    // could silently collide with or hide a pending reply.
    let mut used: HashSet<String> = HashSet::new();
    for payload in payloads {
        let v: serde_json::Value =
            serde_json::from_str(payload).map_err(|e| format!("invalid JSON payload: {e}"))?;
        let id = v
            .get("request_id")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            return Err(
                "waiting needs an explicit non-empty request_id in every payload".to_string(),
            );
        }
        if !used.insert(id.clone()) {
            return Err(format!("duplicate request_id '{id}' in wait payloads"));
        }
    }
    let subscribe_id = unused_request_id("__wait_subscribe", &used);
    used.insert(subscribe_id.clone());
    let baseline_id = unused_request_id("__wait_baseline", &used);
    used.insert(baseline_id.clone());

    let mut framed = Vec::with_capacity(payloads.len() + 2);
    let subscribe_line = serde_json::json!({
        "v": 1,
        "request_id": subscribe_id,
        "nonce": nonce,
        "op": "subscribe",
        "events": ["state", "created", "closed", "error"],
    });
    framed.push(serde_json::to_string(&subscribe_line).map_err(|e| format!("encode failed: {e}"))?);
    for payload in payloads {
        framed.push(inject_nonce(payload, nonce)?);
    }
    let expected_ids: Vec<String> = expect.keys().cloned().collect();
    let baseline_line = serde_json::json!({
        "v": 1,
        "request_id": baseline_id,
        "nonce": nonce,
        "op": "get",
        "ids": expected_ids,
    });
    framed.push(serde_json::to_string(&baseline_line).map_err(|e| format!("encode failed: {e}"))?);

    let pending_ids: Vec<String> = framed
        .iter()
        .filter_map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| {
                    v.get("request_id")
                        .and_then(|r| r.as_str())
                        .map(str::to_string)
                })
        })
        .collect();
    let mut state = WaitState::new(expect.clone(), pending_ids);

    let mut stream =
        crate::transport::connect(socket).map_err(|e| format!("connect failed: {e}"))?;
    // One absolute deadline for the whole wait, not a per-message timeout:
    // every blocking read and write below uses the remaining duration.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
    let start = std::time::Instant::now();
    let report_timeout = |state: &WaitState| {
        let snapshots = render_snapshots(state, compact);
        println!(
            "{}",
            serde_json::json!({"wait": "timeout", "reason": "hosts can clamp/ignore requests or withhold state, so scripts must not hang; deadline exceeded", "elapsed_ms": start.elapsed().as_millis(), "expect": expect, "snapshots": snapshots})
        );
    };
    for line in &framed {
        match write_line_deadline(&mut stream, line, deadline) {
            Ok(()) => {}
            Err(WriteError::Timeout) => {
                report_timeout(&state);
                return Err(format!(
                    "wait timed out after {wait_ms}ms: latest observed did not match"
                ));
            }
            Err(WriteError::Io(e)) => {
                if std::time::Instant::now() >= deadline {
                    report_timeout(&state);
                    return Err(format!(
                        "wait timed out after {wait_ms}ms: latest observed did not match"
                    ));
                }
                return Err(format!("write failed: {e}"));
            }
        }
    }
    drop(framed);
    let mut reader = DeadlineReader::new(stream);
    let emit = |line: &str| {
        if compact {
            println!("{}", compact_line(line));
        } else {
            println!("{line}");
        }
    };
    loop {
        if state.is_matched() {
            let snapshots = render_snapshots(&state, compact);
            println!(
                "{}",
                serde_json::json!({"wait": "matched", "elapsed_ms": start.elapsed().as_millis(), "snapshots": snapshots})
            );
            return Ok(());
        }
        if let Some(reason) = state.failed().map(str::to_string) {
            let snapshots = render_snapshots(&state, compact);
            println!(
                "{}",
                serde_json::json!({"wait": "failed", "reason": reason, "elapsed_ms": start.elapsed().as_millis(), "snapshots": snapshots})
            );
            return Err(format!("wait failed: {reason}"));
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            report_timeout(&state);
            return Err(format!(
                "wait timed out after {wait_ms}ms: latest observed did not match"
            ));
        }
        let line = match reader.read_frame(deadline) {
            WaitRead::Line(line) => line,
            WaitRead::Eof => {
                let snapshots = render_snapshots(&state, compact);
                println!(
                    "{}",
                    serde_json::json!({"wait": "failed", "reason": "EOF before match", "elapsed_ms": start.elapsed().as_millis(), "snapshots": snapshots})
                );
                return Err("wait failed: EOF before match".to_string());
            }
            WaitRead::Timeout => {
                report_timeout(&state);
                return Err(format!(
                    "wait timed out after {wait_ms}ms: latest observed did not match"
                ));
            }
            WaitRead::Err(e) => {
                return Err(format!("read failed: {e:?}"));
            }
        };
        let value: serde_json::Value =
            serde_json::from_str(&line).unwrap_or(serde_json::Value::Null);
        let is_internal = value
            .get("request_id")
            .and_then(|r| r.as_str())
            .is_some_and(|id| id == subscribe_id || id == baseline_id)
            || value.get("event").and_then(|e| e.as_str()) == Some("subscribed");
        if !is_internal {
            emit(&line);
        }
        if state.note_value(&value) {
            let reason = state.failed().unwrap_or("wait failed").to_string();
            let snapshots = render_snapshots(&state, compact);
            println!(
                "{}",
                serde_json::json!({"wait": "failed", "reason": reason, "elapsed_ms": start.elapsed().as_millis(), "snapshots": snapshots})
            );
            return Err(format!("wait failed: {reason}"));
        }
    }
}

/// Windows named-pipe deadline reader: same absolute-deadline framing as
/// the Unix reader. Each blocking wait is one Tokio IO-readiness wait
/// bounded by one Tokio timer for the remaining duration: a chatty peer
/// cannot reset the deadline, and no sleep/poll loop is involved.
#[cfg(windows)]
struct DeadlineReader {
    stream: crate::transport::Stream,
    buf: Vec<u8>,
}

#[cfg(windows)]
impl DeadlineReader {
    fn new(stream: crate::transport::Stream) -> Self {
        Self {
            stream,
            buf: Vec::new(),
        }
    }

    fn read_frame(&mut self, deadline: std::time::Instant) -> WaitRead {
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                if pos + 1 > MAX_LINE_BYTES {
                    return WaitRead::Err(crate::transport::FrameError::TooLong);
                }
                let mut line: Vec<u8> = self.buf.drain(..pos + 1).collect();
                line.pop();
                if line.ends_with(b"\r") {
                    line.pop();
                }
                match String::from_utf8(line) {
                    Ok(s) => return WaitRead::Line(s),
                    Err(_) => return WaitRead::Err(crate::transport::FrameError::InvalidUtf8),
                }
            }
            if self.buf.len() > MAX_LINE_BYTES {
                return WaitRead::Err(crate::transport::FrameError::TooLong);
            }
            if deadline
                .saturating_duration_since(std::time::Instant::now())
                .is_zero()
            {
                return WaitRead::Timeout;
            }
            let mut tmp = [0u8; 4096];
            match crate::transport::read_chunk(&self.stream, &mut tmp, deadline) {
                Ok(Some(n)) => {
                    let chunk = &tmp[..n];
                    match chunk.iter().position(|&b| b == b'\n') {
                        Some(j) => {
                            if self.buf.len().saturating_add(j + 1) > MAX_LINE_BYTES {
                                return WaitRead::Err(crate::transport::FrameError::TooLong);
                            }
                            self.buf.extend_from_slice(chunk);
                        }
                        None => {
                            if self.buf.len().saturating_add(n) > MAX_LINE_BYTES {
                                return WaitRead::Err(crate::transport::FrameError::TooLong);
                            }
                            self.buf.extend_from_slice(chunk);
                        }
                    }
                }
                Ok(None) => {
                    if self.buf.is_empty() {
                        return WaitRead::Eof;
                    }
                    return WaitRead::Err(crate::transport::FrameError::Io(
                        "connection closed mid-line".to_string(),
                    ));
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    return WaitRead::Timeout;
                }
                Err(e) => {
                    return WaitRead::Err(crate::transport::FrameError::Io(e.to_string()));
                }
            }
        }
    }
}

/// Windows deadline-aware frame write: same absolute-deadline contract as
/// Unix. Each partial write is one Tokio writability wait bounded by one
/// Tokio timer for the remaining duration; no sleep/poll loop.
#[cfg(windows)]
fn write_line_deadline(
    stream: &mut crate::transport::Stream,
    line: &str,
    deadline: std::time::Instant,
) -> Result<(), WriteError> {
    let mut buf = line.as_bytes().to_vec();
    buf.push(b'\n');
    let mut pos = 0;
    while pos < buf.len() {
        if std::time::Instant::now() >= deadline {
            return Err(WriteError::Timeout);
        }
        match crate::transport::write_chunk(stream, &buf[pos..], deadline) {
            Ok(0) => return Err(WriteError::Io("write returned zero".to_string())),
            Ok(n) => pos += n,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                return Err(WriteError::Timeout);
            }
            Err(e) => return Err(WriteError::Io(e.to_string())),
        }
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub fn send_wait(
    _: &Path,
    _: &str,
    _: &[String],
    _: u64,
    _: &ExpectMap,
    _: bool,
) -> Result<(), String> {
    Err("local transport is not supported on this OS".to_string())
}

/// Latest full (or compacted) snapshots for the final wait summary.
fn render_snapshots(state: &WaitState, compact: bool) -> BTreeMap<String, serde_json::Value> {
    let mut out = BTreeMap::new();
    for (id, snap) in state.latest() {
        if compact {
            let line = serde_json::to_string(snap).unwrap_or_default();
            out.insert(
                id.clone(),
                serde_json::from_str(&compact_line(&line)).unwrap_or_else(|_| snap.clone()),
            );
        } else {
            out.insert(id.clone(), snap.clone());
        }
    }
    out
}

/// Parse the payload and set its `nonce` field. Payloads must be JSON objects.
pub fn inject_nonce(payload: &str, nonce: &str) -> Result<String, String> {
    let mut value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| format!("invalid JSON payload: {e}"))?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "payload must be a JSON object".to_string())?;
    obj.entry("nonce")
        .or_insert_with(|| serde_json::Value::String(nonce.to_string()));
    serde_json::to_string(&value).map_err(|e| format!("payload encode failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_nonce_keeps_existing_nonce() {
        let out = inject_nonce(r#"{"v":1,"nonce":"keep"}"#, "secret").unwrap();
        assert!(out.contains("\"nonce\":\"keep\""));
        let out = inject_nonce(r#"{"v":1}"#, "secret").unwrap();
        assert!(out.contains("\"nonce\":\"secret\""));
        assert!(inject_nonce("[1,2]", "secret").is_err());
    }

    #[test]
    fn tracker_counts_interleaved_events_and_ignores_the_marker() {
        // Same-connection subscribe+mutate: the marker never counts, real
        // events count even while replies are outstanding, and completion
        // needs both all replies and enough events.
        let mut tracker = ReplyTracker::new(
            vec!["sub".to_string(), "spawn".to_string()],
            Some(2),
            false,
            true,
        );
        assert!(!tracker.note_line(None, true)); // handshake marker
        assert!(!tracker.note_line(Some("sub"), false));
        assert!(!tracker.note_line(None, false)); // created (pending left)
        assert!(!tracker.note_line(Some("spawn"), false));
        assert!(tracker.note_line(None, false)); // closed: 2 events, done
    }

    #[test]
    fn tracker_events_zero_returns_with_replies() {
        let mut tracker = ReplyTracker::new(vec!["r".to_string()], Some(0), false, false);
        assert!(tracker.note_line(Some("r"), false));
    }

    #[test]
    fn tracker_follow_never_finishes_on_its_own() {
        let mut tracker = ReplyTracker::new(vec!["r".to_string()], None, true, true);
        assert!(!tracker.note_line(Some("r"), false));
        assert!(!tracker.note_line(None, false));
        assert!(!tracker.note_line(None, true));
    }

    #[test]
    fn tracker_plain_requests_finish_without_events() {
        let mut tracker =
            ReplyTracker::new(vec!["a".to_string(), "b".to_string()], None, false, false);
        assert!(!tracker.note_line(Some("a"), false));
        assert!(tracker.note_line(Some("b"), false));
    }

    /// `send_lines` against a scripted socket server (no windows): one
    /// connection, replies plus one event, `--events 1` completes.
    #[cfg(any(unix, windows))]
    #[test]
    fn send_lines_counts_same_connection_events() {
        let (dir, socket_path, listener) = test_bind("send-test");
        let server = std::thread::spawn(move || {
            {
                let stream = test_accept(&listener);
                let mut reader = std::io::BufReader::new(test_clone(&stream));
                let mut writer = stream;
                let mut scratch = Vec::new();
                // Two requests: subscribe then list.
                for _ in 0..2 {
                    match crate::transport::read_bounded_line(&mut reader, &mut scratch) {
                        Ok(FrameRead::Line(_)) => {}
                        other => panic!("expected request line, got {other:?}"),
                    }
                }
                // Marker, reply, interleaved event, reply: the exact
                // same-connection shape the real server produces.
                for line in [
                    r#"{"event":"subscribed","subscription":1,"filter":["all"]}"#,
                    r#"{"v":1,"request_id":"s1","ok":true,"result":{"subscribed":["all"],"subscription":1}}"#,
                    r#"{"event":"closed","id":"c"}"#,
                    r#"{"v":1,"request_id":"l1","ok":true,"result":{"ids":[]}}"#,
                ] {
                    crate::transport::write_line(&mut writer, line).expect("write");
                }
            }
        });
        let result = send_lines(
            &socket_path,
            "secret",
            &[
                r#"{"v":1,"request_id":"s1","op":"subscribe"}"#.to_string(),
                r#"{"v":1,"request_id":"l1","op":"list"}"#.to_string(),
            ],
            Some(1),
            false,
            false,
        );
        assert!(result.is_ok(), "send_lines must complete: {result:?}");
        server.join().expect("server thread");
        test_cleanup(&dir, &socket_path);
    }

    /// `--events 0` returns as soon as the replies are in: no hang.
    #[cfg(any(unix, windows))]
    #[test]
    fn send_lines_events_zero_does_not_hang() {
        let (dir, socket_path, listener) = test_bind("send0-test");
        let server = std::thread::spawn(move || {
            let stream = test_accept(&listener);
            let mut reader = std::io::BufReader::new(test_clone(&stream));
            let mut writer = stream;
            let mut scratch = Vec::new();
            match crate::transport::read_bounded_line(&mut reader, &mut scratch) {
                Ok(FrameRead::Line(_)) => {}
                other => panic!("expected request line, got {other:?}"),
            }
            crate::transport::write_line(
                &mut writer,
                r#"{"v":1,"request_id":"l1","ok":true,"result":{"ids":[]}}"#,
            )
            .expect("write");
        });
        let result = send_lines(
            &socket_path,
            "secret",
            &[r#"{"v":1,"request_id":"l1","op":"list"}"#.to_string()],
            Some(0),
            false,
            false,
        );
        assert!(result.is_ok(), "events 0 must return: {result:?}");
        server.join().expect("server thread");
        test_cleanup(&dir, &socket_path);
    }

    #[test]
    fn expect_rejects_unknown_and_null_and_empty() {
        assert!(parse_expect(r#"{"a":{"size_px":[640,480]}}"#).is_ok());
        assert!(parse_expect(r#"{"a":{"decorations":false}}"#).is_err());
        assert!(parse_expect(r#"{"a":{"size_px":null}}"#).is_err());
        assert!(parse_expect(r#"{"a":{}}"#).is_err());
        assert!(parse_expect(r#"{}"#).is_err());
        assert!(parse_expect(r#"{"a":{"size_px":[0,10]}}"#).is_err());
        assert!(parse_expect(r#"{"a":{"mode":"bogus"}}"#).is_err());
        assert!(parse_expect(r#"{"a":{"visible":"yes"}}"#).is_err());
        assert!(parse_expect(r#"{"../x":{"size_px":[1,1]}}"#).is_err());
        assert!(parse_expect(r#"{"a":{"scale_factor":"big"}}"#).is_err());
        assert!(parse_expect(r#"{"a":{"logical_size_px":[640,480]}}"#).is_ok());
    }

    #[test]
    fn compact_strips_snapshot_nulls_but_keeps_errors_and_distinction() {
        let line = r#"{"v":1,"request_id":"g","ok":false,"error":"bad","result":{"windows":[{"id":"a","ok":false,"error":"unknown window id","fields":[{"field":"size_px","status":"unsupported","detail":"nope"}]}]}}"#;
        let compacted = compact_line(line);
        let v: serde_json::Value = serde_json::from_str(&compacted).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"], "bad");
        assert_eq!(v["result"]["windows"][0]["error"], "unknown window id");
        assert_eq!(
            v["result"]["windows"][0]["fields"][0]["status"],
            "unsupported"
        );

        let snap_line = r#"{"v":1,"request_id":"g","ok":true,"result":{"windows":[{"id":"a","ok":true,"snapshot":{"id":"a","pid":1,"backend":"x11","native_handle":"h","title":"t","scale_factor":null,"requested":{"size_px":[640,480],"visible":null},"configured":{"size_px":[640,480]},"observed":{"size_px":[640,480],"focused":null},"unsupported":[],"settled":false},"fields":[]}]}}"#;
        let compacted = compact_line(snap_line);
        let v: serde_json::Value = serde_json::from_str(&compacted).unwrap();
        let snap = &v["result"]["windows"][0]["snapshot"];
        assert_eq!(snap["requested"]["size_px"], serde_json::json!([640, 480]));
        assert!(snap["requested"].get("visible").is_none());
        assert!(snap["observed"].get("focused").is_none());
        assert!(snap.get("scale_factor").is_none());
        // Distinction and settlement signals survive compaction.
        assert!(snap.get("requested").is_some());
        assert!(snap.get("configured").is_some());
        assert!(snap.get("observed").is_some());
        assert!(snap.get("unsupported").is_some());
        assert!(snap.get("settled").is_some());
        assert!(compacted.len() < snap_line.len());
    }

    #[test]
    fn wait_state_matches_latest_not_latched() {
        let expect = parse_expect(r#"{"a":{"size_px":[640,480],"focused":true}}"#).unwrap();
        let mut st = WaitState::new(expect, vec!["p1".to_string(), "base".to_string()]);
        // Event arrives before any reply: records latest but stays pending.
        st.note_value(&serde_json::json!({"event":"state","id":"a","snapshot":{"observed":{"size_px":[640,480],"focused":true}}}));
        assert!(!st.is_matched());
        // Payload + baseline replies carry the same snapshot: now matched.
        st.note_value(&serde_json::json!({"v":1,"request_id":"p1","ok":true,"result":{"windows":[{"id":"a","ok":true,"snapshot":{"observed":{"size_px":[640,480],"focused":true}}}]}}));
        assert!(!st.is_matched());
        st.note_value(&serde_json::json!({"v":1,"request_id":"base","ok":true,"result":{"windows":[{"id":"a","ok":true,"snapshot":{"observed":{"size_px":[640,480],"focused":true}}}]}}));
        assert!(st.is_matched());
        // A newer mismatching state un-matches (no stale latch).
        st.note_value(&serde_json::json!({"event":"state","id":"a","snapshot":{"observed":{"size_px":[800,600],"focused":true}}}));
        assert!(!st.is_matched());
    }

    #[test]
    fn wait_state_unknown_never_matches_and_closed_fails() {
        let expect = parse_expect(r#"{"a":{"size_px":[640,480]}}"#).unwrap();
        let mut st = WaitState::new(expect, vec![]);
        st.note_value(
            &serde_json::json!({"event":"state","id":"a","snapshot":{"observed":{"size_px":null}}}),
        );
        assert!(!st.is_matched());
        assert!(st.failed().is_none());
        st.note_value(&serde_json::json!({"event":"closed","id":"a"}));
        assert!(st.failed().is_some());
    }

    #[test]
    fn wait_state_failed_reply_and_error_event_fail_promptly() {
        let expect = parse_expect(r#"{"a":{"visible":true}}"#).unwrap();
        let mut st = WaitState::new(expect, vec!["p1".to_string()]);
        st.note_value(&serde_json::json!({"v":1,"request_id":"p1","ok":false,"error":"boom"}));
        assert!(st.failed().is_some());

        let expect = parse_expect(r#"{"a":{"visible":true}}"#).unwrap();
        let mut st = WaitState::new(expect, vec![]);
        st.note_value(&serde_json::json!({"event":"error","id":"a","message":"host down"}));
        assert!(st.failed().is_some());
    }

    #[test]
    fn wait_state_missing_window_and_close_reply_fail() {
        let expect = parse_expect(r#"{"a":{"visible":true}}"#).unwrap();
        let mut st = WaitState::new(expect, vec!["base".to_string()]);
        st.note_value(&serde_json::json!({"v":1,"request_id":"base","ok":true,"result":{"windows":[{"id":"a","ok":false,"error":"unknown window id"}]}}));
        assert!(st.failed().is_some());

        let expect = parse_expect(r#"{"a":{"visible":true}}"#).unwrap();
        let mut st = WaitState::new(expect, vec!["c1".to_string()]);
        st.note_value(&serde_json::json!({"v":1,"request_id":"c1","ok":true,"result":{"windows":[{"id":"a","ok":true}]}}));
        assert!(st.failed().is_some());
    }

    #[test]
    fn unused_ids_avoid_collision() {
        let used: HashSet<String> = ["__wait_subscribe".to_string()].into_iter().collect();
        assert_eq!(
            unused_request_id("__wait_subscribe", &used),
            "__wait_subscribe-1"
        );
        assert_eq!(
            unused_request_id("__wait_baseline", &used),
            "__wait_baseline"
        );
    }

    #[test]
    fn wait_rejects_lifecycle_ops() {
        assert!(reject_wait_lifecycle(&[r#"{"op":"subscribe"}"#.to_string()]).is_err());
        assert!(reject_wait_lifecycle(&[r#"{"op":"unsubscribe"}"#.to_string()]).is_err());
        assert!(reject_wait_lifecycle(&[r#"{"op":"shutdown"}"#.to_string()]).is_err());
        assert!(
            reject_wait_lifecycle(&[r#"{"op":"batch","items":[{"op":"shutdown"}]}"#.to_string()])
                .is_err()
        );
        assert!(reject_wait_lifecycle(&[r#"{"op":"set"}"#.to_string()]).is_ok());
        assert!(
            reject_wait_lifecycle(&[r#"{"op":"batch","items":[{"op":"set"}]}"#.to_string()])
                .is_ok()
        );
    }

    /// Portable bound endpoint for transport-backed client tests: a real
    /// listener on both platforms (Unix socket file vs. owner-restricted
    /// named pipe), so the shared `send_lines`/`send_wait`/`DeadlineReader`
    /// paths run headless on Windows too. No logic is reimplemented here:
    /// every helper delegates to `transport`.
    #[cfg(any(unix, windows))]
    fn test_bind(
        tag: &str,
    ) -> (
        std::path::PathBuf,
        std::path::PathBuf,
        crate::transport::Listener,
    ) {
        let dir = std::env::temp_dir().join(format!(
            "tiler-live-fixture-{tag}-{}-{}",
            std::process::id(),
            crate::transport::generate_run_tag()
        ));
        crate::transport::prepare_socket_dir(&dir).unwrap();
        let (listener, socket) =
            crate::transport::bind_private_socket(&dir, "fixture.sock").unwrap();
        (dir, socket, listener)
    }

    #[cfg(any(unix, windows))]
    fn test_accept(listener: &crate::transport::Listener) -> crate::transport::Stream {
        #[cfg(unix)]
        {
            listener.accept().map(|(s, _)| s).expect("accept")
        }
        #[cfg(windows)]
        {
            listener.accept().expect("accept")
        }
    }

    #[cfg(any(unix, windows))]
    fn test_clone(s: &crate::transport::Stream) -> crate::transport::Stream {
        crate::transport::try_clone_stream(s).expect("clone")
    }

    #[cfg(any(unix, windows))]
    fn test_cleanup(dir: &std::path::Path, socket: &std::path::Path) {
        // `remove_file` deletes the Unix socket file; on Windows the
        // endpoint is a kernel pipe name, so it fails silently and only
        // the run directory lifecycle applies.
        let _ = std::fs::remove_file(socket);
        std::fs::remove_dir(dir).unwrap();
    }

    /// Connected loopback pair on both transports (replaces
    /// `UnixStream::pair`): the server side is accepted in a helper thread
    /// while the test thread connects.
    #[cfg(any(unix, windows))]
    fn test_pair(
        tag: &str,
    ) -> (
        crate::transport::Stream,
        crate::transport::Stream,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let (dir, socket, listener) = test_bind(tag);
        let server = std::thread::spawn(move || test_accept(&listener));
        let client = crate::transport::connect(&socket).expect("connect");
        let accepted = server.join().expect("accept thread");
        (client, accepted, dir, socket)
    }

    /// Event-before-reply over a real socket: the mock server emits the
    /// matching state event before the payload reply, then the baseline. The
    /// waiter must match without polling.
    #[cfg(any(unix, windows))]
    #[test]
    fn wait_matches_event_before_reply() {
        let (dir, socket_path, listener) = test_bind("wait-test");
        let server = std::thread::spawn(move || {
            let stream = test_accept(&listener);
            let mut reader = std::io::BufReader::new(test_clone(&stream));
            let mut writer = stream;
            let mut scratch = Vec::new();
            let mut ids = Vec::new();
            for _ in 0..3 {
                match crate::transport::read_bounded_line(&mut reader, &mut scratch) {
                    Ok(FrameRead::Line(line)) => {
                        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                        ids.push(v["request_id"].as_str().unwrap_or("").to_string());
                    }
                    other => panic!("expected request, got {other:?}"),
                }
            }
            assert_eq!(ids.len(), 3);
            let sub_id = ids[0].clone();
            let payload_id = ids[1].clone();
            let base_id = ids[2].clone();
            let snap =
                serde_json::json!({"id":"a","observed":{"size_px":[640,480],"focused":true}});
            // Marker, matching event BEFORE payload reply, then replies.
            for line in [
                r#"{"event":"subscribed","subscription":1,"filter":["all"]}"#.to_string(),
                serde_json::to_string(&serde_json::json!({"event":"state","id":"a","snapshot":snap})).unwrap(),
                serde_json::to_string(&serde_json::json!({"v":1,"request_id":sub_id,"ok":true,"result":{"subscribed":["all"]}})).unwrap(),
                serde_json::to_string(&serde_json::json!({"v":1,"request_id":payload_id,"ok":true,"result":{"windows":[{"id":"a","ok":true,"snapshot":snap}]}})).unwrap(),
                serde_json::to_string(&serde_json::json!({"v":1,"request_id":base_id,"ok":true,"result":{"windows":[{"id":"a","ok":true,"snapshot":snap}]}})).unwrap(),
            ] {
                crate::transport::write_line(&mut writer, &line).expect("write");
            }
        });
        let expect = parse_expect(r#"{"a":{"size_px":[640,480],"focused":true}}"#).unwrap();
        let result = send_wait(
            &socket_path,
            "secret",
            &[
                r#"{"v":1,"request_id":"p1","op":"set","ids":["a"],"patch":{"size_px":[640,480]}}"#
                    .to_string(),
            ],
            5000,
            &expect,
            false,
        );
        assert!(result.is_ok(), "event-before-reply must match: {result:?}");
        server.join().expect("server");
        test_cleanup(&dir, &socket_path);
    }

    /// Mismatch times out at the absolute deadline even with chatty events.
    #[cfg(any(unix, windows))]
    #[test]
    fn wait_mismatch_times_out_on_absolute_deadline() {
        let (dir, socket_path, listener) = test_bind("wait-timeout");
        let server = std::thread::spawn(move || {
            let stream = test_accept(&listener);
            let mut reader = std::io::BufReader::new(test_clone(&stream));
            let mut writer = stream;
            let mut scratch = Vec::new();
            let mut ids = Vec::new();
            for _ in 0..3 {
                match crate::transport::read_bounded_line(&mut reader, &mut scratch) {
                    Ok(FrameRead::Line(line)) => {
                        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                        ids.push(v["request_id"].as_str().unwrap_or("").to_string());
                    }
                    other => panic!("expected request, got {other:?}"),
                }
            }
            let snap = serde_json::json!({"id":"a","observed":{"size_px":[800,600]}});
            crate::transport::write_line(&mut writer, r#"{"event":"subscribed"}"#).unwrap();
            for id in &ids {
                let line = serde_json::to_string(&serde_json::json!({"v":1,"request_id":id,"ok":true,"result":{"windows":[{"id":"a","ok":true,"snapshot":snap}]}})).unwrap();
                crate::transport::write_line(&mut writer, &line).unwrap();
            }
            // Chatty non-matching states: they must not extend the deadline.
            // Keep the connection open past the deadline so the client times
            // out instead of seeing an early EOF.
            for _ in 0..20 {
                let line = serde_json::to_string(
                    &serde_json::json!({"event":"state","id":"a","snapshot":snap}),
                )
                .unwrap();
                let _ = crate::transport::write_line(&mut writer, &line);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            std::thread::sleep(std::time::Duration::from_millis(2000));
        });
        let expect = parse_expect(r#"{"a":{"size_px":[640,480]}}"#).unwrap();
        let start = std::time::Instant::now();
        let result = send_wait(
            &socket_path,
            "secret",
            &[
                r#"{"v":1,"request_id":"p1","op":"set","ids":["a"],"patch":{"size_px":[640,480]}}"#
                    .to_string(),
            ],
            400,
            &expect,
            false,
        );
        let elapsed = start.elapsed();
        assert!(result.is_err(), "mismatch must time out");
        // Absolute deadline: chatty events must not push completion far past
        // wait_ms (generous upper bound for loaded CI).
        assert!(
            elapsed.as_millis() < 3000,
            "deadline not absolute: {elapsed:?}"
        );
        assert!(
            elapsed.as_millis() >= 350,
            "returned before deadline: {elapsed:?}"
        );
        server.join().expect("server");
        test_cleanup(&dir, &socket_path);
    }

    /// EOF before the match is an error, not a silent success.
    #[cfg(any(unix, windows))]
    #[test]
    fn wait_eof_before_match_is_error() {
        let (dir, socket_path, listener) = test_bind("wait-eof");
        let server = std::thread::spawn(move || {
            let stream = test_accept(&listener);
            let mut reader = std::io::BufReader::new(test_clone(&stream));
            let mut scratch = Vec::new();
            for _ in 0..3 {
                let _ = crate::transport::read_bounded_line(&mut reader, &mut scratch);
            }
            // Drop without replying: EOF.
        });
        let expect = parse_expect(r#"{"a":{"size_px":[640,480]}}"#).unwrap();
        let result = send_wait(
            &socket_path,
            "secret",
            &[r#"{"v":1,"request_id":"p1","op":"list"}"#.to_string()],
            2000,
            &expect,
            false,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("EOF"));
        server.join().expect("server");
        test_cleanup(&dir, &socket_path);
    }

    /// Closed expected windows and error events fail instead of matching.
    #[cfg(any(unix, windows))]
    #[test]
    fn wait_closed_and_error_fail_promptly() {
        for (name, extra) in [
            ("closed", serde_json::json!({"event":"closed","id":"a"})),
            (
                "error",
                serde_json::json!({"event":"error","id":"a","message":"host down"}),
            ),
        ] {
            let (dir, socket_path, listener) = test_bind(&format!("wait-{name}"));
            let server = std::thread::spawn(move || {
                let stream = test_accept(&listener);
                let mut reader = std::io::BufReader::new(test_clone(&stream));
                let mut writer = stream;
                let mut scratch = Vec::new();
                let mut ids = Vec::new();
                for _ in 0..3 {
                    match crate::transport::read_bounded_line(&mut reader, &mut scratch) {
                        Ok(FrameRead::Line(line)) => {
                            let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                            ids.push(v["request_id"].as_str().unwrap_or("").to_string());
                        }
                        other => panic!("expected request, got {other:?}"),
                    }
                }
                crate::transport::write_line(&mut writer, r#"{"event":"subscribed"}"#).unwrap();
                // Terminal close/error arrives before the final baseline
                // reply: the waiter must fail even though the earlier
                // snapshots match, instead of returning a stale success.
                let terminal = serde_json::to_string(&extra).unwrap();
                let _ = crate::transport::write_line(&mut writer, &terminal);
                for id in &ids {
                    let line = serde_json::to_string(&serde_json::json!({"v":1,"request_id":id,"ok":true,"result":{"windows":[{"id":"a","ok":true,"snapshot":{"id":"a","observed":{"size_px":[640,480]}}}]}})).unwrap();
                    let _ = crate::transport::write_line(&mut writer, &line);
                }
            });
            let expect = parse_expect(r#"{"a":{"size_px":[640,480]}}"#).unwrap();
            let result = send_wait(
                &socket_path,
                "secret",
                &[r#"{"v":1,"request_id":"p1","op":"list"}"#.to_string()],
                2000,
                &expect,
                false,
            );
            assert!(result.is_err(), "{name} must fail");
            server.join().expect("server");
            test_cleanup(&dir, &socket_path);
        }
    }

    /// Unknown expected windows (failed baseline entry) fail promptly.
    #[cfg(any(unix, windows))]
    #[test]
    fn wait_unknown_window_fails_promptly() {
        let (dir, socket_path, listener) = test_bind("wait-unknown");
        let server = std::thread::spawn(move || {
            let stream = test_accept(&listener);
            let mut reader = std::io::BufReader::new(test_clone(&stream));
            let mut writer = stream;
            let mut scratch = Vec::new();
            let mut ids = Vec::new();
            for _ in 0..3 {
                match crate::transport::read_bounded_line(&mut reader, &mut scratch) {
                    Ok(FrameRead::Line(line)) => {
                        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                        ids.push(v["request_id"].as_str().unwrap_or("").to_string());
                    }
                    other => panic!("expected request, got {other:?}"),
                }
            }
            crate::transport::write_line(&mut writer, r#"{"event":"subscribed"}"#).unwrap();
            for id in &ids {
                let line = if id == &ids[2] {
                    serde_json::to_string(&serde_json::json!({"v":1,"request_id":id,"ok":true,"result":{"windows":[{"id":"ghost","ok":false,"error":"unknown window id"}]}})).unwrap()
                } else {
                    serde_json::to_string(&serde_json::json!({"v":1,"request_id":id,"ok":true,"result":{"windows":[]}})).unwrap()
                };
                crate::transport::write_line(&mut writer, &line).unwrap();
            }
        });
        let expect = parse_expect(r#"{"ghost":{"size_px":[640,480]}}"#).unwrap();
        let result = send_wait(
            &socket_path,
            "secret",
            &[r#"{"v":1,"request_id":"p1","op":"list"}"#.to_string()],
            2000,
            &expect,
            false,
        );
        assert!(result.is_err());
        server.join().expect("server");
        test_cleanup(&dir, &socket_path);
    }

    /// Partial-frame trickle without a newline must not defeat the absolute
    /// deadline. The old per-read timeout reset on every trickle byte, so the
    /// wait hung past the deadline; the deadline-aware reader recomputes the
    /// remaining duration before each blocking read instead. Bounded cleanup:
    /// the server trickles ~1s total and tolerates the client's disconnect.
    #[cfg(any(unix, windows))]
    #[test]
    fn wait_partial_trickle_times_out_on_absolute_deadline() {
        use std::io::Write as _;
        let (dir, socket_path, listener) = test_bind("wait-trickle");
        let server = std::thread::spawn(move || {
            let mut stream = test_accept(&listener);
            let mut reader = std::io::BufReader::new(test_clone(&stream));
            let mut scratch = Vec::new();
            let mut ids = Vec::new();
            for _ in 0..3 {
                match crate::transport::read_bounded_line(&mut reader, &mut scratch) {
                    Ok(FrameRead::Line(line)) => {
                        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                        ids.push(v["request_id"].as_str().unwrap_or("").to_string());
                    }
                    other => panic!("expected request, got {other:?}"),
                }
            }
            let snap = serde_json::json!({"id":"a","observed":{"size_px":[800,600]}});
            let _ = crate::transport::write_line(&mut stream, r#"{"event":"subscribed"}"#);
            for id in &ids {
                let line = serde_json::to_string(&serde_json::json!({"v":1,"request_id":id,"ok":true,"result":{"windows":[{"id":"a","ok":true,"snapshot":snap}]}})).unwrap();
                let _ = crate::transport::write_line(&mut stream, &line);
            }
            // Partial frame, never newline-terminated: one byte at a time,
            // well past the client's deadline.
            let _ = stream.write_all(b"{\"event\":\"state\",\"id\":\"a\",\"snap");
            let _ = stream.flush();
            for _ in 0..20 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                let _ = stream.write_all(b"x");
                let _ = stream.flush();
            }
        });
        let expect = parse_expect(r#"{"a":{"size_px":[640,480]}}"#).unwrap();
        let start = std::time::Instant::now();
        let result = send_wait(
            &socket_path,
            "secret",
            &[
                r#"{"v":1,"request_id":"p1","op":"set","ids":["a"],"patch":{"size_px":[640,480]}}"#
                    .to_string(),
            ],
            300,
            &expect,
            false,
        );
        let elapsed = start.elapsed();
        let msg = result.unwrap_err();
        assert!(
            msg.contains("timed out"),
            "trickle must time out, got: {msg}"
        );
        assert!(
            elapsed.as_millis() >= 250,
            "returned before deadline: {elapsed:?}"
        );
        assert!(
            elapsed.as_millis() < 2000,
            "deadline not absolute: {elapsed:?}"
        );
        server.join().expect("server");
        test_cleanup(&dir, &socket_path);
    }

    /// Loopback-pair unit tests for the deadline reader: coalesced full
    /// frames split exactly like the transport reader, the 1 MiB cap is
    /// preserved, and a byte trickle without a newline hits the absolute
    /// deadline.
    #[cfg(any(unix, windows))]
    #[test]
    fn deadline_reader_splits_coalesced_and_enforces_cap() {
        use std::io::Write as _;
        // Coalesced writes split into separate frames.
        let (reader_stream, mut writer, pair_dir, pair_socket) = test_pair("pair");
        let mut reader = DeadlineReader::new(reader_stream);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        writer.write_all(b"{\"a\":1}\n{\"b\":2}\n").expect("write");
        assert!(matches!(
            reader.read_frame(deadline),
            WaitRead::Line(line) if line == r#"{"a":1}"#
        ));
        assert!(matches!(
            reader.read_frame(deadline),
            WaitRead::Line(line) if line == r#"{"b":2}"#
        ));
        test_cleanup(&pair_dir, &pair_socket);

        // Oversize frame without a newline is rejected, not buffered forever.
        // The writer runs on its own thread: 1 MiB exceeds the pipe/socket
        // buffer, so a single-threaded write would block forever waiting for
        // the reader to drain.
        let (reader_stream, mut writer, pair_dir, pair_socket) = test_pair("pair");
        let big_writer = std::thread::spawn(move || {
            let big = "x".repeat(MAX_LINE_BYTES + 16);
            let _ = writer.write_all(big.as_bytes());
        });
        let mut reader = DeadlineReader::new(reader_stream);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        assert!(matches!(
            reader.read_frame(deadline),
            WaitRead::Err(crate::transport::FrameError::TooLong)
        ));
        big_writer.join().expect("writer");
        test_cleanup(&pair_dir, &pair_socket);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn deadline_reader_trickle_times_out() {
        use std::io::Write as _;
        let (reader_stream, mut writer, pair_dir, pair_socket) = test_pair("pair");
        let trickler = std::thread::spawn(move || {
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(20));
                if writer.write_all(b"x").is_err() {
                    break;
                }
                if writer.flush().is_err() {
                    break;
                }
            }
        });
        let mut reader = DeadlineReader::new(reader_stream);
        let start = std::time::Instant::now();
        let deadline = start + std::time::Duration::from_millis(200);
        let outcome = reader.read_frame(deadline);
        let elapsed = start.elapsed();
        assert!(
            matches!(outcome, WaitRead::Timeout),
            "trickle must hit the deadline"
        );
        assert!(
            elapsed.as_millis() >= 150,
            "returned before deadline: {elapsed:?}"
        );
        assert!(
            elapsed.as_millis() < 2000,
            "deadline not absolute: {elapsed:?}"
        );
        trickler.join().expect("trickler");
        test_cleanup(&pair_dir, &pair_socket);
    }

    /// A slow-draining peer must not defeat the write deadline either:
    /// each partial write honors the remaining absolute duration, so a
    /// peer that never reads cannot reset it. Here the peer is held and
    /// never read while a 2 MiB payload far exceeds the pipe/socket
    /// buffer, forcing the deadline path.
    #[cfg(any(unix, windows))]
    #[test]
    fn wait_write_hits_absolute_deadline_when_peer_never_reads() {
        let (mut writer, _peer, pair_dir, pair_socket) = test_pair("pair-write");
        // `_peer` is held, never read: the kernel buffer fills and further
        // writes block instead of failing, exercising the deadline.
        let big = "y".repeat(2 * 1024 * 1024);
        let start = std::time::Instant::now();
        let deadline = start + std::time::Duration::from_millis(300);
        let outcome = write_line_deadline(&mut writer, &big, deadline);
        let elapsed = start.elapsed();
        assert!(
            matches!(outcome, Err(WriteError::Timeout)),
            "blocked write must hit the deadline, got {outcome:?}"
        );
        assert!(
            elapsed.as_millis() >= 250,
            "returned before deadline: {elapsed:?}"
        );
        assert!(
            elapsed.as_millis() < 5000,
            "deadline not absolute: {elapsed:?}"
        );
        test_cleanup(&pair_dir, &pair_socket);
    }

    /// The deadline-aware write sends the full line including its newline.
    #[cfg(any(unix, windows))]
    #[test]
    fn wait_write_sends_full_line_with_newline() {
        let (mut writer, reader, pair_dir, pair_socket) = test_pair("pair");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        write_line_deadline(&mut writer, r#"{"a":1}"#, deadline).expect("write");
        drop(writer);
        let mut reader = std::io::BufReader::new(reader);
        let mut scratch = Vec::new();
        assert_eq!(
            crate::transport::read_bounded_line(&mut reader, &mut scratch),
            Ok(FrameRead::Line(r#"{"a":1}"#.to_string()))
        );
        test_cleanup(&pair_dir, &pair_socket);
    }

    /// Coalesced short first line then an oversize second line: the second
    /// frame is rejected even though the per-read append only validates the
    /// first newline of each chunk. The writer runs on its own thread: the
    /// 1 MiB payload exceeds the loopback buffer.
    #[cfg(any(unix, windows))]
    #[test]
    fn deadline_reader_rejects_oversize_coalesced_second_line() {
        use std::io::Write as _;
        let (reader_stream, mut writer, pair_dir, pair_socket) = test_pair("pair");
        let sender = std::thread::spawn(move || {
            let mut payload = b"{\"a\":1}\n".to_vec();
            payload.extend(std::iter::repeat_n(b'x', MAX_LINE_BYTES + 1));
            payload.push(b'\n');
            let _ = writer.write_all(&payload);
        });
        let mut reader = DeadlineReader::new(reader_stream);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        assert!(matches!(
            reader.read_frame(deadline),
            WaitRead::Line(line) if line == r#"{"a":1}"#
        ));
        assert!(matches!(
            reader.read_frame(deadline),
            WaitRead::Err(crate::transport::FrameError::TooLong)
        ));
        sender.join().expect("sender");
        test_cleanup(&pair_dir, &pair_socket);
    }
}
