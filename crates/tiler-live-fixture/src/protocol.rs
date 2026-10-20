//! Versioned JSON-lines wire protocol for the live fixture.
//!
//! One JSON object per line. Every request carries the protocol version, a
//! caller-chosen request id, and the per-run connection nonce. Every reply
//! echoes the request id so clients can correlate batches and subscriptions.

use serde::{Deserialize, Serialize};

/// Protocol version. Requests with any other version are rejected.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum bytes per JSON-lines frame (including the newline).
///
/// The bound exists so a buggy or hostile local peer cannot make the fixture
/// buffer an unbounded line and exhaust dev-machine memory; oversize frames
/// are rejected and the connection is closed.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// Maximum caller window id length in bytes.
pub const MAX_ID_LEN: usize = 64;

/// Maximum request id length in bytes.
pub const MAX_REQUEST_ID_LEN: usize = 128;

/// X11 `WM_CLASS` and Wayland `app_id` shared by every fixture window.
/// Linux-only: native window identity is only set on the Linux backend.
#[cfg(target_os = "linux")]
pub const APP_ID: &str = "OmniTilerTestWindow";

/// Title prefix. The `sentinel/id/run-tag` prefix is server-controlled and
/// never client-settable; a caller `title` suffix is appended after a space.
/// The independently settable body text is `description`, rendered inside
/// the window.
pub const TITLE_SENTINEL: &str = "OmniTilerTestWindow";

/// Window presentation requested by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    Windowed,
    Maximized,
    Minimized,
    BorderlessFullscreen,
    ExclusiveFullscreen,
}

/// Creation-only X11 window type (the KDE/X11 platform example).
///
/// This is a creation-time `_NET_WM_WINDOW_TYPE` hint only. Changing it at
/// runtime is reported as unsupported; the window is never recreated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum X11WindowType {
    Normal,
    Dialog,
    Utility,
}

/// Window stacking level hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowLevel {
    AlwaysOnBottom,
    Normal,
    AlwaysOnTop,
}

/// One-shot attention request. `Clear` clears the client urgency hint only:
/// on X11/KWin the compositor `_NET_WM_STATE_DEMANDS_ATTENTION` persists on
/// an inactive window until that window is activated. Accepted never proves
/// the compositor withdrew the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attention {
    Informational,
    Critical,
    Clear,
}

/// One decorated-window button the caller wants enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowButton {
    Close,
    Minimize,
    Maximize,
}

/// Preferred theme. `System` resets to the host default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// Full settable window configuration. Every field is optional; absent means
/// "leave at the current value" for `set` and "fixture default" for `spawn`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fields {
    /// Client-area size in physical pixels.
    pub size_px: Option<[u32; 2]>,
    /// Minimum client-area size in physical pixels.
    pub min_size_px: Option<[u32; 2]>,
    /// Maximum client-area size in physical pixels.
    pub max_size_px: Option<[u32; 2]>,
    /// Whether the user may resize the window.
    pub resizable: Option<bool>,
    /// Whether host decorations are shown. Runtime-toggleable where supported.
    pub decorations: Option<bool>,
    /// Presentation mode.
    pub mode: Option<WindowMode>,
    /// Outer/global position in physical pixels where the host supports it.
    pub position_outer_px: Option<[i32; 2]>,
    /// Whether the window is visible.
    pub visible: Option<bool>,
    /// Free text rendered inside the window. Never placed in the title.
    pub description: Option<String>,
    /// Creation-only X11 window type.
    pub x11_window_type: Option<X11WindowType>,
    /// Optional suffix appended to the server-controlled title
    /// (`sentinel/id/run-tag`), separated by a space. The host title getter
    /// is unsupported on Linux, so the suffix lives in `configured` only,
    /// never in `observed`; `snapshot.title` is the constructed full string.
    pub title: Option<String>,
    /// Stacking level hint. No-op on Wayland: reported, never faked.
    pub window_level: Option<WindowLevel>,
    /// One-shot focus request: `true` raises/focuses the window now.
    /// Never stored in `requested`/`configured`; later unrelated patches
    /// never replay it.
    pub focus: Option<bool>,
    /// One-shot attention request. Never stored; never replayed.
    pub attention: Option<Attention>,
    /// Whether the cursor is visible over the window.
    pub cursor_visible: Option<bool>,
    /// Whether the window hit-tests the cursor. The host call returns a
    /// `Result`; failures are reported honestly, never swallowed.
    pub cursor_hittest: Option<bool>,
    /// Decorated-window buttons to enable. No-op on X11/Wayland: reported,
    /// never faked.
    pub enabled_buttons: Option<Vec<WindowButton>>,
    /// Resize increment hint in physical pixels. Not honored on Wayland.
    pub resize_increments_px: Option<[u32; 2]>,
    /// Preferred theme (`system` resets to the host default).
    pub theme: Option<Theme>,
    /// Transparency hint. Creation-time on X11 (32-bit visual), runtime
    /// elsewhere. The software renderer stays opaque; this only forwards
    /// the host hint, it never demonstrates alpha rendering.
    pub transparent: Option<bool>,
    /// Blur-behind hint. Only where the host supports it (Wayland via the
    /// compositor blur protocol); no-op on X11.
    pub blur: Option<bool>,
    /// Creation-only X11 override-redirect flag. Runtime changes are
    /// reported as unsupported; the window is never recreated.
    pub override_redirect: Option<bool>,
}

/// A window creation entry inside a `spawn` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnSpec {
    pub id: String,
    #[serde(flatten)]
    pub fields: Fields,
}

/// Client operation.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Spawn { windows: Vec<SpawnSpec> },
    Set { ids: Vec<String>, patch: Fields },
    Get { ids: Vec<String> },
    Close { ids: Vec<String> },
    List,
    Batch { items: Vec<BatchItem> },
    Subscribe { events: Option<Vec<String>> },
    Unsubscribe,
    Shutdown,
}

/// One ordered step inside a `batch` request. Nested batches, subscriptions
/// and shutdown are rejected per item: a batch stays a plain window-command
/// sequence, never an atomic host transaction.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchItem {
    pub op: String,
    #[serde(default)]
    pub windows: Vec<SpawnSpec>,
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub patch: Fields,
}

/// A parsed, validated request.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub request_id: String,
    pub nonce: String,
    pub op: Op,
}

/// Envelope parsed before the op body so malformed requests can still be
/// answered with their caller request id when one is recoverable.
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    v: Option<u32>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    op: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseFailure {
    pub request_id: String,
    pub message: String,
}

impl ParseFailure {
    fn named(request_id: String, message: impl Into<String>) -> Self {
        Self {
            request_id,
            message: message.into(),
        }
    }

    fn anonymous(message: impl Into<String>) -> Self {
        Self {
            request_id: String::new(),
            message: message.into(),
        }
    }
}

/// Caller window ids: short printable tokens, never paths or handles.
pub fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return Err(format!("id must be 1..={MAX_ID_LEN} bytes"));
    }
    if !id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
    {
        return Err("id must match [A-Za-z0-9_.-]+".to_string());
    }
    Ok(())
}

fn validate_size_pair(name: &str, size: [u32; 2]) -> Result<(), String> {
    if size[0] == 0 || size[1] == 0 {
        return Err(format!("{name} dimensions must be positive"));
    }
    // No upper geometry bound: the 1 MiB frame already bounds request
    // memory, and callers may target any size (huge sizes simply fail at
    // the host, which reports the failure honestly).
    Ok(())
}

fn validate_description(description: &str) -> Result<(), String> {
    if description.len() > 65536 {
        return Err("description must fit within 65536 bytes".to_string());
    }
    Ok(())
}

/// A title suffix must stay one line: the snapshot header renders it on a
/// single greppable line. Length needs no extra cap: the 1 MiB frame bound
/// already limits request text.
fn validate_title(title: &str) -> Result<(), String> {
    if title.bytes().any(|b| b == b'\n' || b == b'\r') {
        return Err("title must not contain newlines".to_string());
    }
    Ok(())
}

/// Semantic validation shared by spawn specs and set patches.
pub fn validate_fields(fields: &Fields) -> Result<(), String> {
    if let Some(size) = fields.size_px {
        validate_size_pair("size_px", size)?;
    }
    if let Some(min) = fields.min_size_px {
        validate_size_pair("min_size_px", min)?;
    }
    if let Some(max) = fields.max_size_px {
        validate_size_pair("max_size_px", max)?;
    }
    if let (Some(min), Some(max)) = (fields.min_size_px, fields.max_size_px)
        && (min[0] > max[0] || min[1] > max[1])
    {
        return Err("min_size_px must not exceed max_size_px per axis".to_string());
    }
    if let Some(description) = &fields.description {
        validate_description(description)?;
    }
    if let Some(title) = &fields.title {
        validate_title(title)?;
    }
    if let Some(increments) = fields.resize_increments_px {
        validate_size_pair("resize_increments_px", increments)?;
    }
    Ok(())
}

/// Validate a `set` patch against the window's current values: a `min` above
/// the effective (patched or existing) `max` is rejected before reaching the
/// host. Only size hints are merged; other fields need no cross-check.
pub fn validate_merged_hints(existing: &Fields, patch: &Fields) -> Result<(), String> {
    let min = patch.min_size_px.or(existing.min_size_px);
    let max = patch.max_size_px.or(existing.max_size_px);
    if let (Some(min), Some(max)) = (min, max)
        && (min[0] > max[0] || min[1] > max[1])
    {
        return Err("min_size_px must not exceed max_size_px per axis".to_string());
    }
    Ok(())
}

/// Validate one batch item into an executable op. Batch-local only.
pub fn parse_batch_item(item: &BatchItem) -> Result<Op, String> {
    batch_item_op(item)
}

fn batch_item_op(item: &BatchItem) -> Result<Op, String> {
    match item.op.as_str() {
        "spawn" => {
            if item.windows.is_empty() {
                return Err("batch spawn item needs a non-empty windows list".to_string());
            }
            Ok(Op::Spawn {
                windows: item.windows.clone(),
            })
        }
        "set" => {
            if item.ids.is_empty() {
                return Err("batch set item needs a non-empty ids list".to_string());
            }
            Ok(Op::Set {
                ids: item.ids.clone(),
                patch: item.patch.clone(),
            })
        }
        "get" => {
            if item.ids.is_empty() {
                return Err("batch get item needs a non-empty ids list".to_string());
            }
            Ok(Op::Get {
                ids: item.ids.clone(),
            })
        }
        "close" => {
            if item.ids.is_empty() {
                return Err("batch close item needs a non-empty ids list".to_string());
            }
            Ok(Op::Close {
                ids: item.ids.clone(),
            })
        }
        "list" => Ok(Op::List),
        other => Err(format!("op '{other}' is not permitted inside batch")),
    }
}

/// Parse one JSON-lines frame into a validated request.
pub fn parse_request(line: &str) -> Result<Request, ParseFailure> {
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|e| ParseFailure::anonymous(format!("invalid JSON: {e}")))?;
    let envelope: Envelope = serde_json::from_value(value.clone())
        .map_err(|e| ParseFailure::anonymous(format!("invalid envelope: {e}")))?;
    let request_id = envelope.request_id.unwrap_or_default();
    let fail = |message: String| ParseFailure::named(request_id.clone(), message);
    if envelope.v != Some(PROTOCOL_VERSION) {
        return Err(fail(format!(
            "unsupported version: expected v{}",
            PROTOCOL_VERSION
        )));
    }
    if request_id.is_empty() || request_id.len() > MAX_REQUEST_ID_LEN {
        return Err(ParseFailure::named(
            request_id,
            "request_id must be 1..=128 bytes",
        ));
    }
    let nonce = envelope.nonce.unwrap_or_default();
    if nonce.is_empty() {
        return Err(fail("missing nonce".to_string()));
    }
    let op_name = envelope.op.unwrap_or_default();
    let op = match op_name.as_str() {
        "spawn" => {
            let body: SpawnBody = serde_json::from_value(value)
                .map_err(|e| fail(format!("invalid spawn body: {e}")))?;
            if body.windows.is_empty() {
                return Err(fail("spawn needs a non-empty windows list".to_string()));
            }
            Op::Spawn {
                windows: body.windows,
            }
        }
        "set" => {
            let body: IdPatchBody = serde_json::from_value(value)
                .map_err(|e| fail(format!("invalid set body: {e}")))?;
            if body.ids.is_empty() {
                return Err(fail("set needs a non-empty ids list".to_string()));
            }
            Op::Set {
                ids: body.ids,
                patch: body.patch,
            }
        }
        "get" => {
            let body: IdsBody = serde_json::from_value(value)
                .map_err(|e| fail(format!("invalid get body: {e}")))?;
            if body.ids.is_empty() {
                return Err(fail("get needs a non-empty ids list".to_string()));
            }
            Op::Get { ids: body.ids }
        }
        "close" => {
            let body: IdsBody = serde_json::from_value(value)
                .map_err(|e| fail(format!("invalid close body: {e}")))?;
            if body.ids.is_empty() {
                return Err(fail("close needs a non-empty ids list".to_string()));
            }
            Op::Close { ids: body.ids }
        }
        "list" => Op::List,
        "batch" => {
            let body: BatchBody = serde_json::from_value(value)
                .map_err(|e| fail(format!("invalid batch body: {e}")))?;
            if body.items.is_empty() {
                return Err(fail("batch needs a non-empty items list".to_string()));
            }
            // No item-count cap: the 1 MiB frame already bounds batch memory.
            Op::Batch { items: body.items }
        }
        "subscribe" => {
            let body: SubscribeBody = serde_json::from_value(value)
                .map_err(|e| fail(format!("invalid subscribe body: {e}")))?;
            Op::Subscribe {
                events: body.events,
            }
        }
        "unsubscribe" => Op::Unsubscribe,
        "shutdown" => Op::Shutdown,
        "" => return Err(fail("missing op".to_string())),
        other => return Err(fail(format!("unknown op '{other}'"))),
    };
    Ok(Request {
        request_id,
        nonce,
        op,
    })
}

#[derive(Debug, Deserialize)]
struct SpawnBody {
    #[serde(default)]
    windows: Vec<SpawnSpec>,
}

#[derive(Debug, Deserialize)]
struct IdPatchBody {
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    patch: Fields,
}

#[derive(Debug, Deserialize)]
struct IdsBody {
    #[serde(default)]
    ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BatchBody {
    #[serde(default)]
    items: Vec<BatchItem>,
}

#[derive(Debug, Deserialize)]
struct SubscribeBody {
    #[serde(default)]
    events: Option<Vec<String>>,
}

/// Per-field application outcome: an accepted setter is only proof the
/// request reached the host, never proof the host settled on the value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldStatus {
    Accepted,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldOutcome {
    pub field: String,
    pub status: FieldStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl FieldOutcome {
    pub fn accepted(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            status: FieldStatus::Accepted,
            detail: None,
        }
    }

    pub fn unsupported(field: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            status: FieldStatus::Unsupported,
            detail: Some(detail.into()),
        }
    }
}

/// Host-observed window state. `None` serializes to null and means unknown:
/// the host offers no trustworthy reading for that field.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Observed {
    pub size_px: Option<[u32; 2]>,
    pub position_outer_px: Option<[i32; 2]>,
    pub mode: Option<WindowMode>,
    pub visible: Option<bool>,
    pub scale_factor: Option<f64>,
    pub logical_size_px: Option<[u32; 2]>,
    /// Real focus readback (`has_focus`). `None` means unknown.
    pub focused: Option<bool>,
}

/// Full per-window snapshot: requested values beside currently observed ones.
/// `configured` holds the last values handed to the host where the host
/// offers no trustworthy readback; it is explicitly a cache, not an
/// observation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub id: String,
    pub pid: u32,
    pub backend: String,
    pub native_handle: String,
    pub title: String,
    pub scale_factor: Option<f64>,
    pub requested: Fields,
    pub configured: Fields,
    pub observed: Observed,
    pub unsupported: Vec<String>,
    /// False after an accepted set until host feedback confirms the values.
    pub settled: bool,
}

/// One entry of an ID-list reply.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WindowResult {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Snapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldOutcome>,
}

/// Machine-readable server event for subscribers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Created { id: String, snapshot: Snapshot },
    State { id: String, snapshot: Snapshot },
    Closed { id: String },
    Error { id: Option<String>, message: String },
}

impl Event {
    /// Subscription filter names: "created", "state", "closed", "error", "all".
    pub fn name(&self) -> &'static str {
        match self {
            Event::Created { .. } => "created",
            Event::State { .. } => "state",
            Event::Closed { .. } => "closed",
            Event::Error { .. } => "error",
        }
    }
}

/// Reply envelope. `result` carries the op payload; `error` carries a
/// request-level failure (bad framing, auth, version, unknown op).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reply {
    pub v: u32,
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Reply {
    pub fn ok(request_id: &str, result: serde_json::Value) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            request_id: request_id.to_string(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn fail(request_id: &str, message: impl Into<String>) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            request_id: request_id.to_string(),
            ok: false,
            result: None,
            error: Some(message.into()),
        }
    }
}

pub fn reply_to_string(reply: &Reply) -> String {
    serde_json::to_string(reply).unwrap_or_else(|_| {
        r#"{"v":1,"request_id":"","ok":false,"error":"reply encode failed"}"#.to_string()
    })
}

pub fn event_to_string(event: &Event) -> String {
    serde_json::to_string(event).unwrap_or_else(|_| {
        r#"{"event":"error","id":null,"message":"event encode failed"}"#.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_mismatch_keeps_request_id() {
        let err =
            parse_request(r#"{"v":2,"request_id":"r1","nonce":"n","op":"list"}"#).unwrap_err();
        assert_eq!(err.request_id, "r1");
        assert!(err.message.contains("unsupported version"));
    }

    #[test]
    fn unknown_op_is_rejected() {
        let err =
            parse_request(r#"{"v":1,"request_id":"r1","nonce":"n","op":"warp"}"#).unwrap_err();
        assert!(err.message.contains("unknown op"));
    }

    #[test]
    fn missing_nonce_is_rejected() {
        let err = parse_request(r#"{"v":1,"request_id":"r1","op":"list"}"#).unwrap_err();
        assert!(err.message.contains("nonce"));
    }

    #[test]
    fn invalid_json_has_empty_request_id() {
        let err = parse_request("{oops").unwrap_err();
        assert_eq!(err.request_id, "");
    }

    #[test]
    fn batch_rejects_nested_batch() {
        let req = parse_request(
            r#"{"v":1,"request_id":"r","nonce":"n","op":"batch","items":[{"op":"batch","ids":[]}]}"#,
        )
        .unwrap();
        let Op::Batch { items } = req.op else {
            panic!("expected batch")
        };
        assert!(
            batch_item_op(&items[0])
                .unwrap_err()
                .contains("not permitted")
        );
    }

    #[test]
    fn batch_rejects_shutdown_and_subscribe() {
        for op in ["shutdown", "subscribe", "unsubscribe", "bogus"] {
            let item = BatchItem {
                op: op.to_string(),
                windows: vec![],
                ids: vec![],
                patch: Fields::default(),
            };
            assert!(
                batch_item_op(&item).is_err(),
                "op {op} must be rejected in batch"
            );
        }
    }

    #[test]
    fn id_charset_is_restricted() {
        assert!(validate_id("win-1.a_b").is_ok());
        assert!(validate_id("").is_err());
        assert!(validate_id("../escape").is_err());
        assert!(validate_id("with space").is_err());
        assert!(validate_id(&"x".repeat(65)).is_err());
    }

    #[test]
    fn conflicting_min_max_is_rejected() {
        let fields = Fields {
            min_size_px: Some([800, 600]),
            max_size_px: Some([640, 480]),
            ..Fields::default()
        };
        assert!(validate_fields(&fields).is_err());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let err = parse_request(
            r#"{"v":1,"request_id":"r","nonce":"n","op":"set","ids":["a"],"patch":{"bogus":1}}"#,
        )
        .unwrap_err();
        assert!(err.message.contains("invalid set body"));
    }

    #[test]
    fn new_enums_parse_from_snake_case() {
        let req = parse_request(
            r#"{"v":1,"request_id":"r","nonce":"n","op":"set","ids":["a"],"patch":{"window_level":"always_on_top","attention":"critical","theme":"dark","enabled_buttons":["close","minimize"]}}"#,
        )
        .unwrap();
        let Op::Set { patch, .. } = req.op else {
            panic!("expected set");
        };
        assert_eq!(patch.window_level, Some(WindowLevel::AlwaysOnTop));
        assert_eq!(patch.attention, Some(Attention::Critical));
        assert_eq!(patch.theme, Some(Theme::Dark));
        assert_eq!(
            patch.enabled_buttons,
            Some(vec![WindowButton::Close, WindowButton::Minimize])
        );
    }

    #[test]
    fn new_field_validation() {
        assert!(
            validate_fields(&Fields {
                title: Some("has\nnewline".to_string()),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_fields(&Fields {
                title: Some("x".repeat(1024)),
                ..Default::default()
            })
            .is_ok()
        );
        assert!(
            validate_fields(&Fields {
                resize_increments_px: Some([0, 8]),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_fields(&Fields {
                title: Some("lab-1".to_string()),
                resize_increments_px: Some([8, 8]),
                ..Default::default()
            })
            .is_ok()
        );
    }
}
