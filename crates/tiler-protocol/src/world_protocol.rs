//! JSON DTO codec for the world report/reply boundary (D1-D5).
//!
//! Structured request/reply mapping between the JSON transport and the
//! typed core contract in [`tiler_core::world_report`]. The KDE
//! adapter calls this through the planner service; no D-Bus, KWin, or
//! platform imports live here.
//!
//! Wire rules:
//! - Every `u64` counter (`obs`, `reqSeq`, `cmdSeq`, reply `rev`, effect
//!   `rev`, hover `rev`) is a bounded canonical decimal STRING, never a
//!   JSON number: JavaScript `Number` loses precision past 2^53, and JSON
//!   numbers decode as `f64`. A JSON number where a counter belongs
//!   refuses as `unknown-value`. Small `i32` geometry/gap values stay JSON
//!   numbers with exact integer/range checks.
//! - Unknown object fields refuse (`unknown-field`); unknown enum tokens
//!   refuse (`unknown-value`). `null` appears only where documented
//!   (`cmdSeq`/`command` pairing, nullable rects/hovers, empty-string
//!   optional ids in replies).
//! - Bounds mirror the core budget: 4 MiB request AND reply. The service
//!   enforces byte length before decoding and after encoding; the core
//!   enforces its structural estimate plus the exact canonical encoding
//!   before commit. [`encode_reply_v4`] is checked independently: core byte count,
//!   roundtrips, and immutable literal bytes lock the canonical v4 encoding,
//!   so the core commit gate and this transport gate cannot differ. No second writer.
//!
//! Request shape (v4): the sole public ingress is [`decode_request`] /
//! [`decode_request_with_wire`] through [`decode_report_v4`]; see that
//! function for the exact required/optional envelope keys (v4 carries
//! `rendererInput`, refuses `views`, holds identifiable bad rows narrowly).
//! Window rows carry normalized native v4 facts, never adapter verdicts:
//! retired membership verdict keys hold the identifiable row fail-closed.
//! Attempt lanes carry mechanical write/readback facts only; the core owns
//! every verdict, and a typed `Rejected` command result is an accepted
//! observation that settles the command terminally as failed for the
//! requester. No legacy shapes remain: v4 is the sole wire shape.
//!
//! Reply keys are exactly the core canonical reply encoding (`v`, `adapter`,
//! `gen`, `req`, `obs`/`rev` as decimal strings, `ready`, `consumed`,
//! `scopes`, `workspaces`, `outputs`, `windows`, `transfers`, `effects`,
//! `result`: null or the correlated per-command outcome that the snapshot
//! alone cannot express - drag-preview rect+hover, toggle-float placement,
//! migrate-workspace provenance; move, resize, tiled-admit, focus, and send
//! results ride the snapshot, transfers, and effects with `result: null` -
//! plus `gestureSettled`:
//! null or the `{"id":"N","committed":B}` terminal incorporated by this
//! report (never retained across reports), plus `operationSettled`: [] or one
//! `{"op","win","outcome","cause"}` object per retired retained operation
//! (directional maximize-clear verdict or explicit float-placement verdict:
//! original op/window/outcome; a clear continuation means the intent
//! resumed, never that the final frame arrived; only `float-frame-proven`
//! means native observation confirmed the float target), plus
//! `nativeCreations`: [] or one `{"op","ws","out"}` object per outstanding
//! core-owned native creation authorization (capability-gated, at most once
//! per op), plus
//! `warpCursor`: null or the desired `[x,y]` cursor-warp point
//! (cross-output focus only), plus `presentation`: the uniform
//! border/underlay/pager presentation (`foreground` token, required `rev`
//! issuing label, `border` shape, `underlay` found/hidden shape, `pager`
//! content), plus `diagnostics`: []
//! or one `{"scope","row","id","kind"}` object per held identifiable row
//! (a missing `id` encodes as JSON null, never the empty string).
//! Bootstrap/discovery replies reuse the same shape with `ready:false` and
//! empty sections. Bounded errors reuse
//! `{v:4,outcome:"rejected",kind,message,gen}`.

use tiler_core::active_group::ActiveGroupMember;
use tiler_core::boundary::CoreCommand;
use tiler_core::bounds::{is_opaque_id, valid_carried_rect};
use tiler_core::connection::RawSettingValue;
use tiler_core::connection::{HostCapabilities, valid_identity_text, valid_process_identity_text};
use tiler_core::contract::DragSide;
use tiler_core::directional::{Direction, NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::gesture::GestureSettled;
use tiler_core::ids::{
    MAX_CORRELATION_LEN, MAX_GENERATION_LEN, MAX_NATIVE_WORKSPACE_ID_LEN, NativeWorkspaceId,
};
use tiler_core::intent_data::MAX_INTENT_DATA_BYTES;
use tiler_core::policy::PriorGroupEdge;
use tiler_core::session::{DomainKey, DragHoverPrior};
use tiler_core::world_directional::DirectionalOutcome;
use tiler_core::world_presentation::{
    GroupHideReason, PagerScreen, PagerWindow, PagerWorkspace, PresentationUnderlay,
};
use tiler_core::world_report::{
    ActivationPolicy, ArrivalBracket, ArrivalLocation, AttemptLaneV4, AuthorityScope,
    BorderHideReasonV4, BorderPresentationV4, CloseFocusCause, CloseFocusRequest,
    CommandRejectKind, CommandResult, EffectKind, FloatOrigin, FocusFeedback, FrameWitness,
    FullscreenOwner, HeldAttemptRow, HeldNativeRow, HeldPresentationRow, HeldScreenRow,
    HeldWindowRow, HoldReason, IntentRecord, IntentWrite, LastWrite, LastWritePath,
    LastWriteProperty, MAX_INTENT_RECORDS, MAX_INTENT_WRITES, MAX_REPLY_DIAGNOSTICS,
    MAX_REPLY_EFFECTS, MAX_REPLY_NATIVE_CREATIONS, MAX_REPLY_OPERATIONS, MAX_REPLY_OUTPUTS,
    MAX_REPLY_SCOPES, MAX_REPLY_SWITCH_EVENTS, MAX_REPLY_TRANSFERS, MAX_REPLY_WINDOWS,
    MAX_REPLY_WORKSPACES, MAX_STATE_TARGETS_PER_WINDOW, NativeCreationDesire, NativeObservation,
    NativeWorkspaceFact, OneShotEffect, OperationEvent, OperationOutcome, OutputDesire,
    OutputIdentity, PresentationIngressV4, PresentationPagerFrameV4, PresentationRowV4,
    RenderTarget, RendererInput, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane,
    ScopeAuthority, ScopeStatus, ScreenReport, StateProperty, StateTarget, SwitchEvent,
    SwitchFocus, SwitchOutcome, TilingMode, TransferKind, TransferTargetV4, VisibilityFact,
    VisibilityReason, VisibilitySample, VisibilitySetter, VisibilityTarget, WindowDesire,
    WindowFact, WindowType, WorkspaceDesire, WorkspaceRef, WorldCommand, WorldPresentationV4,
    WorldReplyV4, WorldReportV4, check_screen_structure, check_window_structure,
    push_raw_setting_value,
};

/// World protocol contract version (equals the core world contract version:
/// v4).
pub const WORLD_PROTOCOL_VERSION: u32 = tiler_core::world_report::WORLD_CONTRACT_VERSION;
/// D4 transport budget mirror: 4 MiB request cap.
pub const WORLD_MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// D4 transport budget mirror: 4 MiB reply cap.
pub const WORLD_MAX_REPLY_BYTES: usize = 4 * 1024 * 1024;

/// Bounded decode/validation failure. `kind`/`message` are fixed
/// vocabularies that never echo payload bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldProtocolError {
    pub kind: &'static str,
    pub message: &'static str,
}

const KIND_OVERSIZED: &str = "oversized";
const KIND_MALFORMED: &str = "malformed";
const KIND_UNKNOWN_FIELD: &str = "unknown-field";
const KIND_UNKNOWN_VALUE: &str = "unknown-value";
const KIND_VERSION: &str = "unsupported-version";
const KIND_SNAPSHOT: &str = "snapshot-invalid";

const MSG_OVERSIZED: &str = "request exceeds size bound";
const MSG_MALFORMED: &str = "request is malformed";
const MSG_UNKNOWN_FIELD: &str = "request contains an unknown field";
const MSG_UNKNOWN_VALUE: &str = "request contains an unknown value";
const MSG_VERSION: &str = "unsupported contract version";

fn malformed() -> WorldProtocolError {
    WorldProtocolError {
        kind: KIND_MALFORMED,
        message: MSG_MALFORMED,
    }
}

fn unknown_field() -> WorldProtocolError {
    WorldProtocolError {
        kind: KIND_UNKNOWN_FIELD,
        message: MSG_UNKNOWN_FIELD,
    }
}

fn unknown_value() -> WorldProtocolError {
    WorldProtocolError {
        kind: KIND_UNKNOWN_VALUE,
        message: MSG_UNKNOWN_VALUE,
    }
}

/// Fixed bounded error body for the world route. Never echoes payload: only
/// the fixed kind/message plus the serving generation so the adapter can
/// detect rotation.
#[must_use]
pub fn encode_error(kind: &str, message: &str, core_generation: &str) -> String {
    let mut out = String::new();
    out.push_str("{\"v\":4,\"outcome\":\"rejected\",\"kind\":");
    push_json_string(&mut out, kind);
    out.push_str(",\"message\":");
    push_json_string(&mut out, message);
    out.push_str(",\"gen\":");
    push_json_string(&mut out, core_generation);
    out.push('}');
    out
}

type Result<T> = std::result::Result<T, WorldProtocolError>;

fn as_object(value: &serde_json::Value) -> Result<&serde_json::Map<String, serde_json::Value>> {
    value.as_object().ok_or_else(malformed)
}

fn check_keys(map: &serde_json::Map<String, serde_json::Value>, known: &[&str]) -> Result<()> {
    check_keys_with_optional(map, known, &[])
}

/// Exact-shape check with optional keys: every present key must be in the
/// required or optional vocabulary, and every required key must be present.
/// Unknown keys refuse as `unknown-field`; missing required keys are
/// `malformed`. Absent optional keys are fine (callers backfill defaults).
fn check_keys_with_optional(
    map: &serde_json::Map<String, serde_json::Value>,
    required: &[&str],
    optional: &[&str],
) -> Result<()> {
    if map
        .keys()
        .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err(unknown_field());
    }
    if !required.iter().all(|k| map.contains_key(*k)) {
        return Err(malformed());
    }
    Ok(())
}

fn req_str<'a>(map: &'a serde_json::Map<String, serde_json::Value>, key: &str) -> Result<&'a str> {
    map.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(malformed)
}

fn req_bool(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Result<bool> {
    map.get(key)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(malformed)
}

fn req_i32(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Result<i32> {
    let value = map.get(key).ok_or_else(malformed)?;
    value
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(malformed)
}

/// Decode a closed v4 scope union. The output arm and workspace arm have
/// intentionally disjoint key sets; no string/composite compatibility form
/// is accepted.
pub fn decode_authority_scope(value: &serde_json::Value) -> Result<AuthorityScope> {
    let map = as_object(value)?;
    let kind = req_str(map, "kind")?;
    match kind {
        "output" => {
            check_keys(map, &["kind", "out"])?;
            let output = req_str(map, "out")?;
            if !is_opaque_id(output) {
                return Err(malformed());
            }
            Ok(AuthorityScope::Output(OutputId(output.to_owned())))
        }
        "workspace" => {
            check_keys(map, &["kind", "out", "ws"])?;
            let output = req_str(map, "out")?;
            let workspace = req_str(map, "ws")?;
            if !is_opaque_id(output) || !is_opaque_id(workspace) {
                return Err(malformed());
            }
            Ok(AuthorityScope::Workspace {
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId(workspace.to_owned()),
            })
        }
        _ => Err(unknown_value()),
    }
}

/// Bounded canonical decimal `u64` string: `0` or `[1-9][0-9]{0,19}` with a
/// value at or below `u64::MAX`. JSON numbers refuse (no `f64` precision
/// loss); leading zeros, signs, whitespace, and empty strings refuse.
fn parse_u64_decimal(text: &str) -> Option<u64> {
    if text.is_empty() || text.len() > 20 {
        return None;
    }
    if text == "0" {
        return Some(0);
    }
    if !text
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_digit() && b != b'0')
    {
        return None;
    }
    if !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if text.len() == 20 && text > u64::MAX.to_string().as_str() {
        return None;
    }
    text.parse::<u64>().ok()
}

fn req_u64_str(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Result<u64> {
    let text = req_str(map, key)?;
    // A JSON number here would already have failed `req_str`: counters are
    // strings only, so no float decoding ever applies.
    parse_u64_decimal(text).ok_or_else(unknown_value)
}

fn opt_u64_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<u64>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => {
            parse_u64_decimal(text).map(Some).ok_or_else(unknown_value)
        }
        Some(_) => Err(unknown_value()),
    }
}

fn opt_rect_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<Rect>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(value) => req_rect_value(value).map(Some),
    }
}

fn req_rect_value(value: &serde_json::Value) -> Result<Rect> {
    let items = value.as_array().ok_or_else(malformed)?;
    if items.len() != 4 {
        return Err(malformed());
    }
    let mut nums = [0i32; 4];
    for (i, item) in items.iter().enumerate() {
        nums[i] = item
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(malformed)?;
    }
    if !valid_carried_rect(nums[0], nums[1], nums[2], nums[3]) {
        return Err(unknown_value());
    }
    Ok(Rect {
        x: nums[0],
        y: nums[1],
        w: nums[2],
        h: nums[3],
    })
}

fn req_id(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Result<String> {
    let text = req_str(map, key)?;
    if !is_opaque_id(text) {
        return Err(unknown_value());
    }
    Ok(text.to_owned())
}

fn req_bounded(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max: usize,
    alphabet: fn(&str) -> bool,
) -> Result<String> {
    let text = req_str(map, key)?;
    if text.len() > max || !alphabet(text) {
        return Err(unknown_value());
    }
    Ok(text.to_owned())
}

fn is_correlation_text(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_CORRELATION_LEN
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Adapter incarnation alphabet for discovery: opaque bounded token.
/// Shared with the service so both gates stay identical.
#[must_use]
pub fn is_valid_adapter_id(value: &str) -> bool {
    is_opaque_id(value)
}

/// Request id alphabet for discovery: correlation bounded token. Shared
/// with the service so both gates stay identical.
#[must_use]
pub fn is_valid_request_id(value: &str) -> bool {
    is_correlation_text(value)
}

fn is_generation_text(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_GENERATION_LEN
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn parse_frame_setter(text: &str) -> Result<tiler_core::world_report::FrameSetter> {
    match text {
        "accepted" => Ok(tiler_core::world_report::FrameSetter::Accepted),
        "refused" => Ok(tiler_core::world_report::FrameSetter::Refused),
        "threw" => Ok(tiler_core::world_report::FrameSetter::Threw),
        _ => Err(unknown_value()),
    }
}

fn parse_direction_token(text: &str) -> Result<String> {
    match text {
        "left" | "right" | "up" | "down" => Ok(text.to_owned()),
        _ => Err(unknown_value()),
    }
}

fn decode_hints(value: &serde_json::Value) -> Result<tiler_core::size_hints::WindowSizeHints> {
    let map = as_object(value)?;
    check_keys(map, &["minW", "minH", "maxW", "maxH"])?;
    let mut hint = tiler_core::size_hints::WindowSizeHints::none();
    for (key, slot) in [
        ("minW", &mut hint.min_w),
        ("minH", &mut hint.min_h),
        ("maxW", &mut hint.max_w),
        ("maxH", &mut hint.max_h),
    ] {
        *slot = match map.get(key) {
            None => return Err(malformed()),
            Some(serde_json::Value::Null) => None,
            Some(v) => Some(
                v.as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .ok_or_else(malformed)?,
            ),
        };
    }
    Ok(hint)
}

fn opt_id_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            Ok(Some(text.to_owned()))
        }
        Some(_) => Err(unknown_value()),
    }
}

/// Typed send source: `{"explicit": id}` names a genuinely semantic
/// explicit output; `{"implicit": "home-then-action"}` selects the
/// core-owned fallback chain. Any
/// other shape is malformed; non-opaque ids and unknown tokens are
/// unknown values. Legacy string/null `out` payloads refuse here (no
/// compatibility route).
fn decode_send_source(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<tiler_core::world_report::SendSource> {
    use tiler_core::world_report::SendSource;
    let inner = as_object(map.get(key).ok_or_else(malformed)?)?;
    if let Some(value) = inner.get("explicit") {
        check_keys(inner, &["explicit"])?;
        match value {
            serde_json::Value::String(text) => {
                if !is_opaque_id(text) {
                    return Err(unknown_value());
                }
                Ok(SendSource::Explicit(OutputId(text.clone())))
            }
            _ => Err(unknown_value()),
        }
    } else if let Some(value) = inner.get("implicit") {
        check_keys(inner, &["implicit"])?;
        match value {
            serde_json::Value::String(token) => match token.as_str() {
                "home-then-action" => Ok(SendSource::HomeThenAction),
                _ => Err(unknown_value()),
            },
            _ => Err(unknown_value()),
        }
    } else {
        Err(unknown_value())
    }
}

/// Nullable cursor point: `null` when the adapter could not read it, else
/// an exact `[x, y]` pair of carried-geometry integers in adapter units
/// (the same space as screen FULL rectangles and fact frames).
fn opt_point_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<(i32, i32)>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(value) => {
            let items = value.as_array().ok_or_else(malformed)?;
            if items.len() != 2 {
                return Err(malformed());
            }
            let mut nums = [0i32; 2];
            for (i, item) in items.iter().enumerate() {
                nums[i] = item
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok())
                    .ok_or_else(malformed)?;
            }
            if !(-tiler_core::bounds::GEOMETRY_BOUND..=tiler_core::bounds::GEOMETRY_BOUND)
                .contains(&nums[0])
                || !(-tiler_core::bounds::GEOMETRY_BOUND..=tiler_core::bounds::GEOMETRY_BOUND)
                    .contains(&nums[1])
            {
                return Err(unknown_value());
            }
            Ok(Some((nums[0], nums[1])))
        }
    }
}

/// Strict v4 window-row outcome: either a usable row or an identifiable
/// row held narrowly with its carried fields retained. Anything
/// unidentifiable (bad `win`) or structurally wrong (unknown non-retired
/// keys, bad `out` shape, duplicate lifetime, bad `rect`, bad `hints`)
/// refuses the whole report.
enum WindowRowOutcome {
    Valid(Box<WindowFact>),
    Held(HeldWindowRow),
}

/// Nullable bounded native text (`role`, `appId`, `executable`, `class`,
/// `title`): the key is required; `null` is unknown; strings must be
/// nonempty, NUL-free, and within the metadata bound. Overbound text is
/// reported as held by the caller (identifiable value failure), never
/// refused here: this helper refuses only empty/NUL text
/// (`unknown-value`) and wrong types (`malformed`).
fn opt_text_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => {
            if !valid_identity_text(text) {
                return Err(unknown_value());
            }
            Ok(Some(text.clone()))
        }
        Some(_) => Err(malformed()),
    }
}

/// v4-only nullable bounded text (`role`, `appId`, `class`, `title`):
/// normative bounded-text type accepts arbitrary UTF-8 including empty,
/// NUL, and C0 controls within the metadata bound (general wire rules
/// allow C0 including NUL JSON-escaped). Only wrong types refuse here
/// (`malformed`); overbound refuses (`unknown-value`, held by the caller).
/// v3 `opt_text_or_null` above stays exact (nonempty NUL-free identity
/// grammar).
fn opt_bounded_text_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => {
            if text.len() > tiler_core::world_report::MAX_METADATA_TEXT_BYTES {
                return Err(unknown_value());
            }
            Ok(Some(text.clone()))
        }
        Some(_) => Err(malformed()),
    }
}

/// Nullable native process identity: the key is required; `null` is
/// unknown; strings must satisfy the opaque process-identity grammar.
/// Wrong types refuse (`malformed`); bad grammar refuses
/// (`unknown-value`).
fn opt_process_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => {
            if !valid_process_identity_text(text) {
                return Err(unknown_value());
            }
            Ok(Some(text.clone()))
        }
        Some(_) => Err(malformed()),
    }
}

/// Nullable window/output reference (`owner`, `parent`): the key is
/// required; `null` is an unbound relationship; strings must be opaque
/// lifetime ids. Wrong types refuse (`malformed`); bad grammar refuses
/// (`unknown-value`).
fn opt_window_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<WindowId>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            Ok(Some(WindowId(text.clone())))
        }
        Some(_) => Err(malformed()),
    }
}

/// Nullable `i32` layer: the key is required; `null` is unknown; numbers
/// must fit `i32`. Wrong types refuse (`malformed`); out-of-range numbers
/// refuse (`unknown-value`).
fn opt_layer_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<i32>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| {
                if value.is_number() {
                    unknown_value()
                } else {
                    malformed()
                }
            }),
    }
}

/// Nullable closed token decoder: the key is required; `null` is unknown;
/// strings parse through `parse`, unknown tokens refuse
/// (`unknown-value`), wrong types refuse (`malformed`).
fn opt_token_or_null<T>(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    parse: fn(&str) -> Option<T>,
) -> Result<Option<T>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => parse(text).map(Some).ok_or_else(unknown_value),
        Some(_) => Err(malformed()),
    }
}

/// Strict nested `VisibilityFact` decode: both keys required and nullable;
/// wrong types refuse (`malformed`).
fn decode_visibility(value: &serde_json::Value) -> Result<VisibilityFact> {
    let map = as_object(value)?;
    check_keys(map, &["visible", "suppressionOwned"])?;
    Ok(VisibilityFact {
        visible: opt_bool_or_null(map, "visible")?,
        suppression_owned: opt_bool_or_null(map, "suppressionOwned")?,
    })
}

/// Strict nested `VisibilitySample` decode: `obs` is a required canonical
/// decimal counter string; every other key is required and nullable;
/// wrong types and bad counters refuse.
fn decode_visibility_sample(value: &serde_json::Value) -> Result<VisibilitySample> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "obs",
            "lifetimeMatched",
            "visibility",
            "minimized",
            "hostHidden",
        ],
    )?;
    Ok(VisibilitySample {
        obs: req_u64_str(map, "obs")?,
        lifetime_matched: opt_bool_or_null(map, "lifetimeMatched")?,
        visibility: decode_visibility(map.get("visibility").ok_or_else(malformed)?)?,
        minimized: opt_bool_or_null(map, "minimized")?,
        host_hidden: opt_bool_or_null(map, "hostHidden")?,
    })
}

/// Strict nested `LastWrite` decode: every key required; `pre`/`post` are
/// nullable samples; unknown property/path/setter/reason tokens refuse
/// (`unknown-value`).
fn decode_last_write(value: &serde_json::Value) -> Result<LastWrite> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "property", "target", "revision", "path", "setter", "reason", "pre", "post",
        ],
    )?;
    let property_text = req_str(map, "property")?;
    let property = LastWriteProperty::parse(property_text).ok_or_else(unknown_value)?;
    let path_text = req_str(map, "path")?;
    let path = LastWritePath::parse(path_text).ok_or_else(unknown_value)?;
    let setter_text = req_str(map, "setter")?;
    let setter = VisibilitySetter::parse(setter_text).ok_or_else(unknown_value)?;
    let reason_text = req_str(map, "reason")?;
    let reason = VisibilityReason::parse(reason_text).ok_or_else(unknown_value)?;
    let pre = match map.get("pre") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_visibility_sample(value)?),
    };
    let post = match map.get("post") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_visibility_sample(value)?),
    };
    Ok(LastWrite {
        property,
        target: req_bool(map, "target")?,
        revision: req_u64_str(map, "revision")?,
        path,
        setter,
        reason,
        pre,
        post,
    })
}

/// Retain one identifiable-but-bad window row: every carried key in
/// canonical schema order, then any other carried keys lexicographically,
/// with an `InvalidFact` diagnostic only for the row's actual reported
/// output (an output-less row carries no scope authority: `None`). The nested `hints`, `visibility`, and `lastWrite` objects are
/// retained in schema order with unknown sub-keys after in lexicographic
/// order so the canonical writer spells the exact carried row. `ordinal`
/// is the row's original index in the carried `windows` array.
fn held_window_row(
    fact: &WindowFact,
    ordinal: usize,
    map: &serde_json::Map<String, serde_json::Value>,
) -> HeldWindowRow {
    const SCHEMA_ORDER: &[&str] = &[
        "win",
        "out",
        "rect",
        "fullscreen",
        "maximized",
        "sticky",
        "minimized",
        "hints",
        "nativeWs",
        "hostHidden",
        "privileged",
        "managed",
        "acceptsInput",
        "activationInhibited",
        "toolWindow",
        "owned",
        "transient",
        "modal",
        "resizable",
        "above",
        "below",
        "owner",
        "parent",
        "role",
        "appId",
        "executable",
        "processId",
        "class",
        "title",
        "type",
        "activationPolicy",
        "layer",
        "fullscreenOwner",
        "fullscreenExitable",
        "suppressionAvailable",
        "visibility",
        "lastWrite",
    ];
    const HINTS_ORDER: &[&str] = &["minW", "minH", "maxW", "maxH"];
    const VISIBILITY_ORDER: &[&str] = &["visible", "suppressionOwned"];
    const LAST_WRITE_ORDER: &[&str] = &[
        "property", "target", "revision", "path", "setter", "reason", "pre", "post",
    ];
    fn order_object(value: &serde_json::Value, order: &[&str]) -> RawSettingValue {
        let RawSettingValue::Object(entries) = raw_setting_value(value) else {
            return raw_setting_value(value);
        };
        let mut rest: std::collections::BTreeMap<String, RawSettingValue> =
            entries.into_iter().collect();
        let mut ordered = Vec::with_capacity(rest.len());
        for key in order {
            if let Some(item) = rest.remove(*key) {
                ordered.push((key.to_string(), item));
            }
        }
        for (key, item) in rest {
            ordered.push((key, item));
        }
        RawSettingValue::Object(ordered)
    }
    let mut fields = Vec::with_capacity(map.len());
    for key in SCHEMA_ORDER {
        if let Some(value) = map.get(*key) {
            let retained = match *key {
                "hints" => order_object(value, HINTS_ORDER),
                "visibility" => order_object(value, VISIBILITY_ORDER),
                "lastWrite" => order_object(value, LAST_WRITE_ORDER),
                _ => raw_setting_value(value),
            };
            fields.push((key.to_string(), retained));
        }
    }
    let mut extra: Vec<&String> = map
        .keys()
        .filter(|key| !SCHEMA_ORDER.contains(&key.as_str()))
        .collect();
    extra.sort();
    for key in extra {
        fields.push((key.clone(), raw_setting_value(&map[key])));
    }
    // Nested visibility samples keep schema order inside the retained
    // `lastWrite` object: `pre`/`post` reorder to sample order and their
    // nested `visibility` objects to visibility order, so witnesses are
    // canonical regardless of carried order.
    for (key, value) in fields.iter_mut() {
        if key == "lastWrite" {
            reorder_last_write(value);
        }
    }
    HeldWindowRow {
        window: fact.window.clone(),
        ordinal,
        output: fact.output.clone(),
        fields,
        diagnostic: fact.output.as_ref().map(|out| RowDiagnostic {
            scope: AuthorityScope::Output(out.clone()),
            row: RowDiagnosticLane::Window,
            id: Some(fact.window.0.clone()),
            kind: RowDiagnosticKind::InvalidFact,
        }),
        kind: RowDiagnosticKind::InvalidFact,
    }
}

/// Reorder the nested objects of a retained `lastWrite` value into
/// schema order in place: the object itself is already ordered by the
/// caller; `pre`/`post` sample objects reorder to sample order and their
/// nested `visibility` objects to visibility order. Unknown sub-keys sort
/// after in lexicographic order.
fn reorder_last_write(value: &mut RawSettingValue) {
    const SAMPLE_ORDER: &[&str] = &[
        "obs",
        "lifetimeMatched",
        "visibility",
        "minimized",
        "hostHidden",
    ];
    const VISIBILITY_ORDER: &[&str] = &["visible", "suppressionOwned"];
    fn order_entries(
        entries: Vec<(String, RawSettingValue)>,
        order: &[&str],
    ) -> Vec<(String, RawSettingValue)> {
        let mut rest: std::collections::BTreeMap<String, RawSettingValue> =
            entries.into_iter().collect();
        let mut ordered = Vec::with_capacity(rest.len());
        for key in order {
            if let Some(item) = rest.remove(*key) {
                ordered.push((key.to_string(), item));
            }
        }
        for (key, item) in rest {
            ordered.push((key, item));
        }
        ordered
    }
    fn order_visibility(value: &mut RawSettingValue) {
        if let RawSettingValue::Object(entries) = value {
            let taken = std::mem::take(entries);
            *entries = order_entries(taken, VISIBILITY_ORDER);
        }
    }
    let RawSettingValue::Object(entries) = value else {
        return;
    };
    for (key, item) in entries.iter_mut() {
        if (key == "pre" || key == "post")
            && let RawSettingValue::Object(sample) = item
        {
            let taken = std::mem::take(sample);
            *sample = order_entries(taken, SAMPLE_ORDER);
            for (sub_key, sub_value) in sample.iter_mut() {
                if sub_key == "visibility" {
                    order_visibility(sub_value);
                }
            }
        }
    }
}

/// Retired window keys: adapter membership verdicts the core now derives
/// (legacy floating/fit/fixed/observed plus legacy shell/tool/dialog
/// verdicts). Any carried retired key holds the identifiable row (fail
/// closed), never a silent drop and never an unknown-field refusal.
const RETIRED_WINDOW_KEYS: &[&str] = &[
    "floating",
    "fitExcluded",
    "fixedAuto",
    "fixedSuppress",
    "observedWs",
    "shell",
    "tool",
    "ownedDialog",
    "dialog",
    "noActivate",
    "cannotControl",
];

/// Strict v4 screen-row outcome: either a usable row or an identifiable
/// row held narrowly with its carried fields retained. Anything
/// unidentifiable (bad `out`) or structurally wrong (unknown non-retired
/// keys, bad `full`/`work`/`complete`) refuses the whole report.
enum ScreenRowOutcome {
    Valid(ScreenReport),
    Held(HeldScreenRow),
}

/// Convert one carried JSON value into its JSON-agnostic retained fact.
/// JSON numbers always decode to finite `f64` (no arbitrary precision on
/// this boundary), so the conversion is total.
fn raw_setting_value(value: &serde_json::Value) -> RawSettingValue {
    match value {
        serde_json::Value::Null => RawSettingValue::Null,
        serde_json::Value::Bool(set) => RawSettingValue::Bool(*set),
        serde_json::Value::Number(number) => {
            RawSettingValue::Number(number.as_f64().unwrap_or(f64::NAN))
        }
        serde_json::Value::String(text) => RawSettingValue::String(text.clone()),
        serde_json::Value::Array(items) => {
            RawSettingValue::Array(items.iter().map(raw_setting_value).collect())
        }
        serde_json::Value::Object(entries) => RawSettingValue::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), raw_setting_value(item)))
                .collect(),
        ),
    }
}

/// Retain one identifiable-but-bad screen row: every carried key in
/// canonical schema order (`out`, `full`, `work`, `complete`, `scale`,
/// `identity`, then any other carried keys lexicographically), with an
/// `InvalidFact` diagnostic over the output scope naming the output. The
/// nested `identity` object is retained in schema order (`manufacturer`,
/// `model`, `serial`, `connector`, then unknown sub-keys lexicographically)
/// so the canonical writer spells the exact carried row. `ordinal` is the
/// row's original index in the carried `screens` array.
fn held_screen_row(
    output: &OutputId,
    ordinal: usize,
    map: &serde_json::Map<String, serde_json::Value>,
) -> HeldScreenRow {
    const SCHEMA_ORDER: &[&str] = &["out", "full", "work", "complete", "scale", "identity"];
    const IDENTITY_ORDER: &[&str] = &["manufacturer", "model", "serial", "connector"];
    fn order_identity(entries: Vec<(String, RawSettingValue)>) -> Vec<(String, RawSettingValue)> {
        let mut rest: std::collections::BTreeMap<String, RawSettingValue> =
            entries.into_iter().collect();
        let mut ordered = Vec::with_capacity(rest.len());
        for key in IDENTITY_ORDER {
            if let Some(value) = rest.remove(*key) {
                ordered.push((key.to_string(), value));
            }
        }
        for (key, value) in rest {
            ordered.push((key, value));
        }
        ordered
    }
    let mut fields = Vec::with_capacity(map.len());
    for key in SCHEMA_ORDER {
        if let Some(value) = map.get(*key) {
            let mut retained = raw_setting_value(value);
            if *key == "identity"
                && let RawSettingValue::Object(entries) = retained
            {
                retained = RawSettingValue::Object(order_identity(entries));
            }
            fields.push((key.to_string(), retained));
        }
    }
    let mut extra: Vec<&String> = map
        .keys()
        .filter(|key| !SCHEMA_ORDER.contains(&key.as_str()))
        .collect();
    extra.sort();
    for key in extra {
        fields.push((key.clone(), raw_setting_value(&map[key])));
    }
    HeldScreenRow {
        output: output.clone(),
        ordinal,
        fields,
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(output.clone()),
            row: RowDiagnosticLane::Screen,
            id: Some(output.0.clone()),
            kind: RowDiagnosticKind::InvalidFact,
        },
    }
}

/// Strict v4 screen-row decode: `{out, full, work, complete, scale,
/// identity}` with `identity = {manufacturer, model, serial, connector}`.
/// - Unknown top-level keys refuse (`unknown-field`), except the retired
///   `gap`/`outer`/`primary` keys which hold the identifiable row.
/// - A missing or non-opaque `out` refuses the whole report (missing is
///   `malformed`, a bad id is `unknown-value`).
/// - `full`/`work`/`complete` failures refuse like the other fact shapes.
///   Topology violations (half-carried rect pair, `complete` without both
///   rects) refuse before any narrow hold below, with the core reason.
/// - `scale` is required: absent refuses (`malformed`); present but not a
///   finite positive number holds the row.
/// - `identity` is a required object whose four keys are all required and
///   nullable: `null` is unknown, strings are bounded to 256 UTF-8 bytes
///   with empty text preserved as `Some("")` (never normalized to
///   unknown). A missing key, a wrong-typed value, an overbound string, a
///   non-object identity, or an unknown sub-key holds the row.
fn decode_screen(value: &serde_json::Value, ordinal: usize) -> Result<ScreenRowOutcome> {
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &["out", "full", "work", "complete", "scale", "identity"],
        &["gap", "outer", "primary"],
    )?;
    let output = match map.get("out") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => OutputId(text.clone()),
        Some(serde_json::Value::String(_)) => return Err(unknown_value()),
        _ => return Err(malformed()),
    };
    // Null rectangles stay representable for explicitly unreadable or
    // partial observations; `complete` claims require both (core gate).
    let full_rect = opt_rect_or_null(map, "full")?;
    let work_area = opt_rect_or_null(map, "work")?;
    let complete = req_bool(map, "complete")?;
    // Structural rect-pair/complete claims refuse before any narrow hold:
    // a bad scale, an overbound identity, or a retired key must not hide a
    // topology violation the core refuses directly (shared policy with the
    // core structural check; the dummy scale/identity never fire it, and
    // the fresh id set never reports a duplicate: this gate sees only the
    // rect-pair/complete shape, while id/duplicate refusal stays where it
    // is). Malformed numeric rects already refused above, unchanged.
    {
        let candidate = ScreenReport {
            output: output.clone(),
            full_rect,
            work_area,
            complete,
            scale: 1.0,
            identity: OutputIdentity::unknown(),
        };
        let mut seen = std::collections::BTreeSet::new();
        if let Some(reason) = check_screen_structure(&candidate, &mut seen) {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: reason,
            });
        }
    }
    // Retired per-output physical tweaks are never valid rows: the core
    // derives physical gaps from the logical settings times scale.
    let retired = ["gap", "outer", "primary"]
        .iter()
        .any(|key| map.contains_key(*key));
    if retired {
        return Ok(ScreenRowOutcome::Held(held_screen_row(
            &output, ordinal, map,
        )));
    }
    let scale = match map.get("scale") {
        None => return Err(malformed()),
        Some(serde_json::Value::Number(number)) => {
            let scale = number.as_f64().ok_or_else(malformed)?;
            if !scale.is_finite() || scale <= 0.0 {
                return Ok(ScreenRowOutcome::Held(held_screen_row(
                    &output, ordinal, map,
                )));
            }
            scale
        }
        Some(_) => {
            return Ok(ScreenRowOutcome::Held(held_screen_row(
                &output, ordinal, map,
            )));
        }
    };
    let identity = match map.get("identity") {
        None => return Err(malformed()),
        Some(serde_json::Value::Object(identity_map)) => {
            match decode_identity_strict(identity_map) {
                Some(identity) => identity,
                None => {
                    return Ok(ScreenRowOutcome::Held(held_screen_row(
                        &output, ordinal, map,
                    )));
                }
            }
        }
        Some(_) => {
            return Ok(ScreenRowOutcome::Held(held_screen_row(
                &output, ordinal, map,
            )));
        }
    };
    Ok(ScreenRowOutcome::Valid(ScreenReport {
        output,
        full_rect,
        work_area,
        complete,
        scale,
        identity,
    }))
}

/// Strict identity decode: the four keys are all required and nullable.
/// Returns `None` when the row must hold (missing key, wrong type,
/// overbound text, unknown sub-key). Empty text is preserved as
/// `Some("")`, never normalized to unknown.
fn decode_identity_strict(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<OutputIdentity> {
    use tiler_core::world_report::MAX_IDENTITY_TEXT_BYTES;
    if map.len() != 4 {
        return None;
    }
    let text = |key: &str| -> Option<Option<String>> {
        match map.get(key) {
            None => None,
            Some(serde_json::Value::Null) => Some(None),
            Some(serde_json::Value::String(text)) => {
                if text.len() > MAX_IDENTITY_TEXT_BYTES {
                    return None;
                }
                Some(Some(text.clone()))
            }
            Some(_) => None,
        }
    };
    Some(OutputIdentity {
        manufacturer: text("manufacturer")?,
        model: text("model")?,
        serial: text("serial")?,
        connector: text("connector")?,
    })
}

fn decode_closed_ids(value: &serde_json::Value) -> Result<Vec<WindowId>> {
    let items = value.as_array().ok_or_else(malformed)?;
    let mut closed = Vec::with_capacity(items.len());
    for item in items {
        let id = item.as_str().ok_or_else(malformed)?;
        if !is_opaque_id(id) {
            return Err(unknown_value());
        }
        closed.push(WindowId(id.to_owned()));
    }
    Ok(closed)
}

pub fn decode_member_attempt(
    value: &serde_json::Value,
) -> Result<tiler_core::world_report::MemberAttempt> {
    use tiler_core::world_report::MemberAttempt;
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &[
            "op",
            "win",
            "out",
            "ws",
            "setter",
            "pre",
            "post",
            "attemptMs",
        ],
        &[],
    )?;
    Ok(MemberAttempt {
        operation_id: req_id(map, "op")?,
        window: WindowId(req_id(map, "win")?),
        target_output: OutputId(req_id(map, "out")?),
        target_workspace: req_id(map, "ws")?,
        setter: parse_frame_setter(req_str(map, "setter")?)?,
        pre: opt_workspace_ref_or_null(map, "pre", false)?,
        post: opt_workspace_ref_or_null(map, "post", false)?,
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
}

pub fn decode_view_attempt(
    value: &serde_json::Value,
) -> Result<tiler_core::world_report::ViewAttempt> {
    use tiler_core::world_report::ViewAttempt;
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &[
            "op",
            "out",
            "ws",
            "setter",
            "pre",
            "post",
            "base",
            "attemptMs",
        ],
        &[],
    )?;
    Ok(ViewAttempt {
        operation_id: req_id(map, "op")?,
        target_output: OutputId(req_id(map, "out")?),
        target_workspace: req_id(map, "ws")?,
        setter: parse_frame_setter(req_str(map, "setter")?)?,
        pre: opt_workspace_ref_or_null(map, "pre", true)?,
        post: opt_workspace_ref_or_null(map, "post", true)?,
        baseline: opt_workspace_ref_or_null(map, "base", true)?,
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
}

fn opt_workspace_ref_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    native_only: bool,
) -> Result<Option<WorkspaceRef>> {
    match map.get(key).ok_or_else(malformed)? {
        serde_json::Value::Null => Ok(None),
        value => {
            let reference = as_object(value)?;
            let kind = req_str(reference, "kind")?;
            let output = req_str(reference, "out")?;
            let workspace = req_str(reference, "ws")?;
            if !is_opaque_id(output) || !is_opaque_id(workspace) {
                return Err(malformed());
            }
            match kind {
                "logical" if !native_only => {
                    check_keys(reference, &["kind", "out", "ws"])?;
                    Ok(Some(WorkspaceRef::Logical {
                        output: OutputId(output.to_owned()),
                        workspace: WorkspaceId(workspace.to_owned()),
                    }))
                }
                "native" => {
                    check_keys(reference, &["kind", "out", "ws"])?;
                    if workspace.len() > MAX_NATIVE_WORKSPACE_ID_LEN {
                        return Err(unknown_value());
                    }
                    let workspace =
                        NativeWorkspaceId::parse(workspace).ok_or_else(unknown_value)?;
                    Ok(Some(WorkspaceRef::Native {
                        output: OutputId(output.to_owned()),
                        workspace,
                    }))
                }
                "logical" => Err(unknown_value()),
                _ => Err(unknown_value()),
            }
        }
    }
}

fn opt_u32_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<u32>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .and_then(|v| u32::try_from(v).ok())
            .map(Some)
            .ok_or_else(malformed),
    }
}

fn opt_bool_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<bool>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(malformed()),
    }
}

fn decode_clear_attempt(
    value: &serde_json::Value,
) -> Result<tiler_core::world_report::ClearAttempt> {
    let map = as_object(value)?;
    check_keys(map, &["op", "win", "setter", "pre", "post", "attemptMs"])?;
    Ok(tiler_core::world_report::ClearAttempt {
        operation_id: req_id(map, "op")?,
        window: WindowId(req_id(map, "win")?),
        setter: parse_frame_setter(req_str(map, "setter")?)?,
        pre_maximized: opt_bool_or_null(map, "pre")?,
        post_maximized: opt_bool_or_null(map, "post")?,
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
}

/// Decode one raw native-state attempt row. Every key is required on the
/// wire (`win`, `property`, `target`, `rev`, `setter`, `pre`, `post`,
/// `attemptMs`); absence refuses as malformed, unknown keys refuse as
/// unknown-field, and unknown property/setter tokens or bad decimal
/// counters refuse as unknown-value. `pre`/`post` are required nullable
/// immediate proofs (`null` is unreadable, never a default).
pub fn decode_state_attempt(
    value: &serde_json::Value,
) -> Result<tiler_core::world_report::StateAttempt> {
    use tiler_core::world_report::StateProperty;
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "win",
            "property",
            "target",
            "rev",
            "setter",
            "pre",
            "post",
            "attemptMs",
        ],
    )?;
    Ok(tiler_core::world_report::StateAttempt {
        window: WindowId(req_id(map, "win")?),
        property: StateProperty::parse(req_str(map, "property")?).ok_or_else(unknown_value)?,
        target: req_bool(map, "target")?,
        revision: req_u64_str(map, "rev")?,
        setter: parse_frame_setter(req_str(map, "setter")?)?,
        pre: opt_bool_or_null(map, "pre")?,
        post: opt_bool_or_null(map, "post")?,
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
}

/// Decode one correlated focus-evidence row. Every key is required on the
/// wire (`id`, `target`, `rev`, `setter`, `reason`, `pre`, `post`,
/// `foregroundProven`, `arrival`, `attemptMs`); absence refuses as malformed,
/// unknown keys refuse as unknown-field, and unknown setter/reason tokens
/// or bad decimal counters refuse as unknown-value. `pre`/`post` are
/// required nullable foreground lifetimes (`null` is unknown, never empty).
/// Bracket arm selection is context-free here; the engine host gate owns
/// the native/output check.
fn decode_focus_feedback(value: &serde_json::Value) -> Result<FocusFeedback> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "id",
            "target",
            "rev",
            "setter",
            "reason",
            "pre",
            "post",
            "foregroundProven",
            "arrival",
            "attemptMs",
        ],
    )?;
    let setter_text = req_str(map, "setter")?;
    let setter = VisibilitySetter::parse(setter_text).ok_or_else(unknown_value)?;
    let reason_text = req_str(map, "reason")?;
    let reason = VisibilityReason::parse(reason_text).ok_or_else(unknown_value)?;
    let arrival_value = map.get("arrival").ok_or_else(malformed)?;
    let mut arrival = Vec::new();
    for item in arrival_value.as_array().ok_or_else(malformed)? {
        arrival.push(decode_arrival_bracket(item)?);
    }
    Ok(FocusFeedback {
        id: req_id(map, "id")?,
        target: WindowId(req_id(map, "target")?),
        revision: req_u64_str(map, "rev")?,
        setter,
        reason,
        pre: opt_window_or_null(map, "pre")?,
        post: opt_window_or_null(map, "post")?,
        foreground_proven: req_bool(map, "foregroundProven")?,
        arrival,
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
}

/// Decode one readiness bracket. `win`, `obs`, `before`, `kind`, `frame`,
/// and `after` are required in both arms; the exclusive location key rides
/// the selected arm only (`home` for `native`, `out` for `output`) and is
/// itself required there (nullable: `null` is unreadable evidence, never a
/// default), while the other arm's key is forbidden. A non-null native
/// `home` must use the native workspace namespace. Unknown kinds refuse as
/// unknown-value; missing keys refuse as malformed. `frame` is a required
/// nullable checked carried rect; unknown or mismatched sample facts are
/// preserved as facts, never verdicts.
fn decode_arrival_bracket(value: &serde_json::Value) -> Result<ArrivalBracket> {
    let map = as_object(value)?;
    let kind = req_str(map, "kind")?;
    let location = match kind {
        "native" => {
            check_keys(
                map,
                &["win", "obs", "before", "kind", "home", "frame", "after"],
            )?;
            ArrivalLocation::Native {
                home: opt_workspace_ref_or_null(map, "home", true)?,
            }
        }
        "output" => {
            check_keys(
                map,
                &["win", "obs", "before", "kind", "out", "frame", "after"],
            )?;
            ArrivalLocation::Output {
                out: opt_id_or_null(map, "out")?.map(OutputId),
            }
        }
        _ => return Err(unknown_value()),
    };
    Ok(ArrivalBracket {
        window: WindowId(req_id(map, "win")?),
        obs: req_u64_str(map, "obs")?,
        before: decode_visibility_sample(map.get("before").ok_or_else(malformed)?)?,
        location,
        frame: opt_rect_or_null(map, "frame")?,
        after: decode_visibility_sample(map.get("after").ok_or_else(malformed)?)?,
    })
}

fn decode_creation_attempt(
    value: &serde_json::Value,
) -> Result<tiler_core::world_report::NativeCreationAttempt> {
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &["op", "ws", "out", "setter", "before", "after", "attemptMs"],
        &["created"],
    )?;
    let order_list = |key: &str| -> Result<Vec<String>> {
        let items = map.get(key).ok_or_else(malformed)?;
        let items = items.as_array().ok_or_else(malformed)?;
        let mut order = Vec::with_capacity(items.len());
        for item in items {
            let id = item.as_str().ok_or_else(malformed)?;
            if !is_opaque_id(id) {
                return Err(unknown_value());
            }
            order.push(id.to_owned());
        }
        Ok(order)
    };
    Ok(tiler_core::world_report::NativeCreationAttempt {
        operation_id: req_id(map, "op")?,
        logical_workspace: req_id(map, "ws")?,
        target_output: OutputId(req_id(map, "out")?),
        setter: parse_frame_setter(req_str(map, "setter")?)?,
        order_before: order_list("before")?,
        order_after: order_list("after")?,
        created_native: match map.get("created") {
            // Tolerant backfill: adapters predating the correlated receipt
            // omit it (or send null/empty) and read as uncorrelated.
            None => None,
            Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(text)) if text.is_empty() => None,
            Some(serde_json::Value::String(text)) => {
                if !is_opaque_id(text) {
                    return Err(unknown_value());
                }
                Some(text.clone())
            }
            Some(_) => return Err(malformed()),
        },
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
}

fn decode_hover_prior(
    value: &serde_json::Value,
) -> Result<Option<tiler_core::session::DragHoverPrior>> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "domainOut",
            "domainWs",
            "leaf",
            "win",
            "rev",
            "prior",
            "priorOut",
            "priorWs",
        ],
    )?;
    let leaf = req_id(map, "leaf")?;
    let revision = req_u64_str(map, "rev")?;
    let edge = match map.get("prior") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(prior_value) => {
            let prior_map = as_object(prior_value)?;
            check_keys(prior_map, &["group", "edge"])?;
            let group = req_id(prior_map, "group")?;
            let edge = match req_str(prior_map, "edge")? {
                "left" => tiler_core::contract::DragSide::Left,
                "right" => tiler_core::contract::DragSide::Right,
                "top" => tiler_core::contract::DragSide::Top,
                "bottom" => tiler_core::contract::DragSide::Bottom,
                // Center never carries stickiness: advisory degrade, not a
                // refusal (mirrors the legacy hover-carry rule).
                _ => return Ok(None),
            };
            Some(tiler_core::policy::PriorGroupEdge {
                group: NodeId::from(group.as_str()),
                edge,
            })
        }
    };
    let domain_out = req_id(map, "domainOut")?;
    let domain_ws = req_id(map, "domainWs")?;
    let prior_out = req_id(map, "priorOut")?;
    let prior_ws = req_id(map, "priorWs")?;
    Ok(Some(tiler_core::session::DragHoverPrior {
        domain: tiler_core::session::DomainKey {
            output: OutputId(domain_out),
            workspace: WorkspaceId(domain_ws),
        },
        source_leaf: NodeId::from(leaf.as_str()),
        source_window: WindowId(req_id(map, "win")?),
        revision,
        prior: edge,
        prior_domain: tiler_core::session::DomainKey {
            output: OutputId(prior_out),
            workspace: WorkspaceId(prior_ws),
        },
    }))
}

fn decode_drag_source(value: &serde_json::Value) -> Result<Option<tiler_core::session::DomainKey>> {
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Object(map) => {
            check_keys(map, &["out", "ws"])?;
            Ok(Some(tiler_core::session::DomainKey {
                output: OutputId(req_id(map, "out")?),
                workspace: WorkspaceId(req_id(map, "ws")?),
            }))
        }
        _ => Err(malformed()),
    }
}

/// Normalized gesture ingress: `{"win":S,"kind":"move"|"resize",
/// "phase":"started"|"updated"|"released"|"cancelled","id":"N",
/// "cursor":[x,y]|null,"startCursor":[x,y]|null,"esc":B,"modal":B,
/// "start":[x,y,w,h]}`. `id` is the stable nonzero adapter gesture
/// sequence in decimal-u64 string form (zero refuses); `startCursor` is
/// the START down point (`null` only when unreadable). Unknown
/// kind/phase tokens refuse; cursor/rect bounds refuse like the top-level
/// cursor and fact rects.
fn decode_gesture_ingress(
    value: &serde_json::Value,
) -> Result<tiler_core::gesture::GestureIngress> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "win",
            "kind",
            "phase",
            "id",
            "cursor",
            "startCursor",
            "esc",
            "modal",
            "start",
        ],
    )?;
    let kind = tiler_core::gesture::GestureKind::parse_wire(req_str(map, "kind")?)
        .ok_or_else(unknown_value)?;
    let phase = tiler_core::gesture::GesturePhase::parse_wire(req_str(map, "phase")?)
        .ok_or_else(unknown_value)?;
    let id = req_u64_str(map, "id")?;
    if id == 0 {
        return Err(unknown_value());
    }
    let start = map.get("start").ok_or_else(malformed)?;
    Ok(tiler_core::gesture::GestureIngress {
        id,
        window: WindowId(req_id(map, "win")?),
        kind,
        cursor: opt_point_or_null(map, "cursor")?,
        start_cursor: opt_point_or_null(map, "startCursor")?,
        phase,
        esc: req_bool(map, "esc")?,
        modal_move: req_bool(map, "modal")?,
        start_frame: req_rect_value(start)?,
    })
}

/// V4 presentation row codecs (normative target in
/// `docs/porting/world-contract.md`): strict
/// shape-only decode plus checked canonical encode for the standalone
/// [`PresentationIngressV4`], [`FrameWitness`], [`RendererInput`]/
/// [`RenderTarget`], and [`PresentationPagerFrameV4`] rows used by the whole
/// v4 ingress. Whole-report pairing (renderer/command, version gate) belongs
/// to the v4 whole-report codec.
///
/// Wire rules for these standalone row codecs: required means present;
/// unknown keys refuse as `unknown-field` (whole-report identifiable invalid
/// rows instead hold narrowly); unknown enum tokens and bad id grammar refuse
/// as `unknown-value`; wrong JSON types refuse as `malformed` (nullable
/// slots accept `null` only where documented). Duplicate-key rejection
/// (including escaped duplicates) applies ONLY at the `*_str` entry points,
/// which parse bytes through `parse_strict_json`; the
/// `serde_json::Value` decoders operate on already-parsed values and give
/// no duplicate promise. Counters are canonical decimal strings (quoted,
/// `u64::MAX` accepted); rects are `[x,y,w,h]` under
/// [`valid_carried_rect`]; ids are nonempty opaque ids (ASCII
/// `[A-Za-z0-9-_.]`, 1..128 bytes) except `foreground` and `moveWindow`,
/// which additionally allow the explicit empty string for none. Witness
/// `win`/`foreground` are always nonempty: no empty convention there. Pager
/// order and frame sources each cap at `MAX_PRESENTATION_ORDER` (1024);
/// over-cap refuses as `snapshot-invalid` with the existing fixed
/// `presentation-order-over-cap` / `presentation-frames-over-cap` tokens.
/// Row string entry points gate at the 4 MiB transport bound like the
/// current route. Shape only: ids need not match one another, the ingress,
/// or live reports; well-formed but dangling/mismatched rows are accepted
/// (causal join and target-row validation belong to later units).
/// Strict shape decode of one staged v4 frame-witness object value.
pub fn decode_frame_witness(value: &serde_json::Value) -> Result<FrameWitness> {
    let map = as_object(value)?;
    check_keys(map, &["win", "obs", "frame", "foreground"])?;
    Ok(FrameWitness {
        win: WindowId(req_id(map, "win")?),
        obs: req_u64_str(map, "obs")?,
        frame: req_rect_value(map.get("frame").ok_or_else(malformed)?)?,
        foreground: WindowId(req_id(map, "foreground")?),
    })
}

/// Strict shape decode of one staged v4 frame-witness object from its exact
/// canonical bytes (4 MiB row-input gate, duplicate-key rejection).
pub fn decode_frame_witness_str(json: &str) -> Result<FrameWitness> {
    decode_row_str(json, decode_frame_witness)
}

/// Checked canonical encode of one staged v4 frame-witness object
/// (`{"win":S,"obs":"N","frame":[x,y,w,h],"foreground":S}`). The built bytes
/// re-decode through the strict row decoder, so invalid typed ids,
/// non-canonical counters, or invalid geometry never emit unsafe JSON.
pub fn encode_frame_witness(witness: &FrameWitness) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"win\":");
    push_json_string(&mut out, &witness.win.0);
    out.push_str(",\"obs\":\"");
    out.push_str(&witness.obs.to_string());
    out.push_str("\",\"frame\":");
    push_rect(&mut out, &witness.frame);
    out.push_str(",\"foreground\":");
    push_json_string(&mut out, &witness.foreground.0);
    out.push('}');
    decode_frame_witness_str(&out)?;
    Ok(out)
}

/// Strict shape decode of one staged v4 pager frame row value
/// (`{"win":S,"live":[x,y,w,h]|null}`; no float/tile slots).
pub fn decode_pager_frame_v4(value: &serde_json::Value) -> Result<PresentationPagerFrameV4> {
    let row = as_object(value)?;
    check_keys(row, &["win", "live"])?;
    Ok(PresentationPagerFrameV4 {
        window: WindowId(req_id(row, "win")?),
        live: opt_rect_or_null(row, "live")?,
    })
}

/// Strict shape decode of one staged v4 pager frame row from its exact
/// canonical bytes (4 MiB row-input gate, duplicate-key rejection).
pub fn decode_pager_frame_v4_str(json: &str) -> Result<PresentationPagerFrameV4> {
    decode_row_str(json, decode_pager_frame_v4)
}

/// Checked canonical encode of one staged v4 pager frame row. The built
/// bytes re-decode through the strict row decoder, so invalid typed ids or
/// geometry never emit unsafe JSON.
pub fn encode_pager_frame_v4(row: &PresentationPagerFrameV4) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"win\":");
    push_json_string(&mut out, &row.window.0);
    out.push_str(",\"live\":");
    push_opt_rect(&mut out, &row.live);
    out.push('}');
    decode_pager_frame_v4_str(&out)?;
    Ok(out)
}

/// Strict shape decode of one staged v4 render-target object value.
pub fn decode_render_target(value: &serde_json::Value) -> Result<RenderTarget> {
    let map = as_object(value)?;
    let kind = req_str(map, "kind")?;
    match kind {
        "workspace" => {
            check_keys(map, &["kind", "out", "ws"])?;
            Ok(RenderTarget::Workspace {
                out: OutputId(req_id(map, "out")?),
                ws: WorkspaceId(req_id(map, "ws")?),
            })
        }
        "window" => {
            check_keys(map, &["kind", "out", "ws", "win"])?;
            Ok(RenderTarget::Window {
                out: OutputId(req_id(map, "out")?),
                ws: WorkspaceId(req_id(map, "ws")?),
                win: WindowId(req_id(map, "win")?),
            })
        }
        "group" => {
            check_keys(map, &["kind", "out", "ws", "group"])?;
            Ok(RenderTarget::Group {
                out: OutputId(req_id(map, "out")?),
                ws: WorkspaceId(req_id(map, "ws")?),
                group: NodeId(req_id(map, "group")?),
            })
        }
        _ => Err(unknown_value()),
    }
}

/// Checked canonical encode of one staged v4 render-target object
/// (`{"kind":K,"out":S,"ws":S}` plus `,"win":S` on the window arm or
/// `,"group":S` on the group arm only). The built bytes re-decode through
/// the strict row decoder, so invalid typed ids never emit unsafe JSON.
pub fn encode_render_target(target: &RenderTarget) -> Result<String> {
    let mut out = String::new();
    match target {
        RenderTarget::Workspace { out: output, ws } => {
            out.push_str("{\"kind\":\"workspace\",\"out\":");
            push_json_string(&mut out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, &ws.0);
            out.push('}');
        }
        RenderTarget::Window {
            out: output,
            ws,
            win,
        } => {
            out.push_str("{\"kind\":\"window\",\"out\":");
            push_json_string(&mut out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, &ws.0);
            out.push_str(",\"win\":");
            push_json_string(&mut out, &win.0);
            out.push('}');
        }
        RenderTarget::Group {
            out: output,
            ws,
            group,
        } => {
            out.push_str("{\"kind\":\"group\",\"out\":");
            push_json_string(&mut out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, &ws.0);
            out.push_str(",\"group\":");
            push_json_string(&mut out, &group.0);
            out.push('}');
        }
    }
    decode_row_str(&out, decode_render_target)?;
    Ok(out)
}

/// Strict shape decode of one staged v4 renderer-input object value
/// (`{"rev":"N","target":{...}}`). Always an object here; top-level
/// nullability and command pairing belong to the future whole-report
/// integration, not this shape codec.
pub fn decode_renderer_input(value: &serde_json::Value) -> Result<RendererInput> {
    let map = as_object(value)?;
    check_keys(map, &["rev", "target"])?;
    Ok(RendererInput {
        rev: req_u64_str(map, "rev")?,
        target: decode_render_target(map.get("target").ok_or_else(malformed)?)?,
    })
}

/// Strict shape decode of one staged v4 renderer-input object from its exact
/// canonical bytes (4 MiB row-input gate, duplicate-key rejection).
pub fn decode_renderer_input_str(json: &str) -> Result<RendererInput> {
    decode_row_str(json, decode_renderer_input)
}

/// Checked canonical encode of one staged v4 renderer-input object. The
/// built bytes re-decode through the strict row decoder, so invalid typed
/// rows never emit unsafe JSON.
pub fn encode_renderer_input(input: &RendererInput) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"rev\":\"");
    out.push_str(&input.rev.to_string());
    out.push_str("\",\"target\":");
    out.push_str(&encode_render_target(&input.target)?);
    out.push('}');
    decode_renderer_input_str(&out)?;
    Ok(out)
}

/// Empty-or-opaque token (`foreground`/`moveWindow`): `""` means none, any
/// other value must be a nonempty opaque id.
fn req_empty_or_id(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Result<String> {
    let text = req_str(map, key)?;
    if !text.is_empty() && !is_opaque_id(text) {
        return Err(unknown_value());
    }
    Ok(text.to_owned())
}

/// Strict shape decode of one staged v4 presentation-ingress object value.
pub fn decode_presentation_ingress_v4(value: &serde_json::Value) -> Result<PresentationIngressV4> {
    use tiler_core::world_report::MAX_PRESENTATION_ORDER;
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "foreground",
            "minimized",
            "maximized",
            "fullscreen",
            "hostHidden",
            "hiddenWorkspace",
            "frame",
            "chordHeld",
            "moveWindow",
            "pagerOrder",
            "pagerFrames",
            "witness",
            "foregroundOut",
        ],
    )?;
    let order_value = map.get("pagerOrder").ok_or_else(malformed)?;
    let order_items = order_value.as_array().ok_or_else(malformed)?;
    if order_items.len() > MAX_PRESENTATION_ORDER {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "presentation-order-over-cap",
        });
    }
    let mut pager_order = Vec::with_capacity(order_items.len());
    for item in order_items {
        let id = item.as_str().ok_or_else(malformed)?;
        if !is_opaque_id(id) {
            return Err(unknown_value());
        }
        pager_order.push(WindowId(id.to_owned()));
    }
    let frames_value = map.get("pagerFrames").ok_or_else(malformed)?;
    let frame_items = frames_value.as_array().ok_or_else(malformed)?;
    if frame_items.len() > MAX_PRESENTATION_ORDER {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "presentation-frames-over-cap",
        });
    }
    let mut pager_frames = Vec::with_capacity(frame_items.len());
    for item in frame_items {
        pager_frames.push(decode_pager_frame_v4(item)?);
    }
    let witness = match map.get("witness") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_frame_witness(value)?),
    };
    let foreground_out = match map.get("foregroundOut") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            Some(OutputId(text.clone()))
        }
        Some(_) => return Err(malformed()),
    };
    Ok(PresentationIngressV4 {
        foreground: req_empty_or_id(map, "foreground")?,
        foreground_minimized: opt_bool_or_null(map, "minimized")?,
        foreground_maximized: opt_bool_or_null(map, "maximized")?,
        foreground_fullscreen: opt_bool_or_null(map, "fullscreen")?,
        foreground_host_hidden: opt_bool_or_null(map, "hostHidden")?,
        foreground_hidden_workspace: opt_bool_or_null(map, "hiddenWorkspace")?,
        foreground_frame: opt_rect_or_null(map, "frame")?,
        chord_held: req_bool(map, "chordHeld")?,
        move_window: req_empty_or_id(map, "moveWindow")?,
        pager_order,
        pager_frames,
        witness,
        foreground_out,
    })
}

/// Strict shape decode of one staged v4 presentation-ingress object from its
/// exact canonical bytes (4 MiB row-input gate, duplicate-key rejection).
pub fn decode_presentation_ingress_v4_str(json: &str) -> Result<PresentationIngressV4> {
    decode_row_str(json, decode_presentation_ingress_v4)
}

/// Checked canonical encode of one staged v4 presentation-ingress object in
/// contract table order. The built bytes re-decode through the strict row
/// decoder, so invalid typed ids, nullability, geometry, or over-cap rows
/// never emit unsafe JSON.
pub fn encode_presentation_ingress_v4(ingress: &PresentationIngressV4) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"foreground\":");
    push_json_string(&mut out, &ingress.foreground);
    out.push_str(",\"minimized\":");
    push_opt_bool(&mut out, &ingress.foreground_minimized);
    out.push_str(",\"maximized\":");
    push_opt_bool(&mut out, &ingress.foreground_maximized);
    out.push_str(",\"fullscreen\":");
    push_opt_bool(&mut out, &ingress.foreground_fullscreen);
    out.push_str(",\"hostHidden\":");
    push_opt_bool(&mut out, &ingress.foreground_host_hidden);
    out.push_str(",\"hiddenWorkspace\":");
    push_opt_bool(&mut out, &ingress.foreground_hidden_workspace);
    out.push_str(",\"frame\":");
    push_opt_rect(&mut out, &ingress.foreground_frame);
    out.push_str(",\"chordHeld\":");
    out.push_str(if ingress.chord_held { "true" } else { "false" });
    out.push_str(",\"moveWindow\":");
    push_json_string(&mut out, &ingress.move_window);
    out.push_str(",\"pagerOrder\":[");
    for (i, window) in ingress.pager_order.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, &window.0);
    }
    out.push_str("],\"pagerFrames\":[");
    for (i, row) in ingress.pager_frames.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&encode_pager_frame_v4(row)?);
    }
    out.push_str("],\"witness\":");
    match &ingress.witness {
        None => out.push_str("null"),
        Some(witness) => out.push_str(&encode_frame_witness(witness)?),
    }
    out.push_str(",\"foregroundOut\":");
    match &ingress.foreground_out {
        None => out.push_str("null"),
        Some(output) => push_json_string(&mut out, &output.0),
    }
    out.push('}');
    decode_presentation_ingress_v4_str(&out)?;
    Ok(out)
}

fn parse_border_hide_reason_v4(text: &str) -> Result<BorderHideReasonV4> {
    match text {
        "disabled" => Ok(BorderHideReasonV4::Disabled),
        "no-target" => Ok(BorderHideReasonV4::NoTarget),
        "minimized" => Ok(BorderHideReasonV4::Minimized),
        "maximized" => Ok(BorderHideReasonV4::Maximized),
        "fullscreen" => Ok(BorderHideReasonV4::Fullscreen),
        "host-hidden" => Ok(BorderHideReasonV4::HostHidden),
        "hidden-workspace" => Ok(BorderHideReasonV4::HiddenWorkspace),
        "shell" => Ok(BorderHideReasonV4::Shell),
        "suspended" => Ok(BorderHideReasonV4::Suspended),
        _ => Err(unknown_value()),
    }
}

/// Staged v4 anchored-border row codec (normative target in
/// `docs/porting/world-contract.md`): strict
/// shape-only decode plus checked canonical encode for the standalone
/// [`BorderPresentationV4`] row. Staged: no existing v3 writer, estimator,
/// reservation, producer, or whole-report entry point changes, and this
/// function is not called from any existing route. No whole-reply codec is
/// claimed here; the full reply route is untouched.
///
/// Wire rules shared with the v3 lane: every key is required on the wire
/// (absence refuses as `malformed`); unknown keys (including the retired
/// absolute `inner`/`outer`) refuse as `unknown-field`; unknown reason
/// tokens and bad id grammar refuse as `unknown-value`; wrong JSON types
/// refuse as `malformed` (nullable slots accept `null` only where
/// documented). `reason` is the closed v4 token set with `host-hidden`
/// (v3 `cloaked` refuses as `unknown-value`); `None` spells `""`.
/// Duplicate-key rejection (including escaped duplicates) applies ONLY at
/// the `*_str` entry point, which parses bytes through
/// `parse_strict_json`; the `serde_json::Value` decoder operates on
/// already-parsed values and gives no duplicate promise. Counters are
/// canonical decimal strings (quoted, `u64::MAX` accepted); witness frames
/// are `[x,y,w,h]` under [`valid_carried_rect`]; ids are nonempty opaque
/// ids (ASCII `[A-Za-z0-9-_.]`, 1..128 bytes). `outsets` is exactly four
/// bare JSON integers (`i32` range, any sign: no `Rect` bound applies to
/// thickness rows); `width`/`radius` are bare JSON integers with
/// `value >= 0` (negative refuses as `unknown-value`, non-integers and
/// `i32` overflow refuse as `malformed`). No old report style cap applies
/// to any reply outset or style number. Shape only: `visible` with a reason
/// token refuses (the contract table requires empty when visible), but a
/// hidden row with `""` stays valid (geometry-veto carry-forward); every
/// other field combination is accepted: no descriptor coupling is imposed,
/// and the row shape does not validate causal or producer consistency.
///
/// Strict shape decode of one staged v4 anchored-border row value.
pub fn decode_border_presentation_v4(value: &serde_json::Value) -> Result<BorderPresentationV4> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "visible", "reason", "subject", "outsets", "width", "radius", "witness",
        ],
    )?;
    let visible = req_bool(map, "visible")?;
    let reason_text = req_str(map, "reason")?;
    let reason = if reason_text.is_empty() {
        None
    } else {
        Some(parse_border_hide_reason_v4(reason_text)?)
    };
    if visible && reason.is_some() {
        return Err(unknown_value());
    }
    let subject = match map.get("subject") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            Some(WindowId(text.clone()))
        }
        Some(_) => return Err(malformed()),
    };
    let outsets = match map.get("outsets") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => {
            let items = value.as_array().ok_or_else(malformed)?;
            if items.len() != 4 {
                return Err(malformed());
            }
            let mut nums = [0i32; 4];
            for (i, item) in items.iter().enumerate() {
                nums[i] = item
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok())
                    .ok_or_else(malformed)?;
            }
            Some(nums)
        }
    };
    let width = match map.get("width") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => {
            let num = value
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(malformed)?;
            if num < 0 {
                return Err(unknown_value());
            }
            Some(num)
        }
    };
    let radius = match map.get("radius") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => {
            let num = value
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(malformed)?;
            if num < 0 {
                return Err(unknown_value());
            }
            Some(num)
        }
    };
    let witness = match map.get("witness") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_frame_witness(value)?),
    };
    Ok(BorderPresentationV4 {
        visible,
        reason,
        subject,
        outsets,
        width,
        radius,
        witness,
    })
}

/// Strict shape decode of one staged v4 anchored-border row from its exact
/// canonical bytes (4 MiB row-input gate, duplicate-key rejection).
pub fn decode_border_presentation_v4_str(json: &str) -> Result<BorderPresentationV4> {
    decode_row_str(json, decode_border_presentation_v4)
}

/// Checked canonical encode of one staged v4 anchored-border row in
/// contract table order
/// (`{"visible":B,"reason":S,"subject":S|null,"outsets":[l,t,r,b]|null,
/// "width":N|null,"radius":N|null,"witness":W|null}`). The built bytes
/// re-decode through the strict row decoder, so invalid typed ids, reason
/// tokens, visible/reason coupling, or style numbers never emit unsafe
/// JSON.
pub fn encode_border_presentation_v4(row: &BorderPresentationV4) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"visible\":");
    out.push_str(if row.visible { "true" } else { "false" });
    out.push_str(",\"reason\":");
    match row.reason {
        None => out.push_str("\"\""),
        Some(reason) => {
            out.push('"');
            out.push_str(reason.as_str());
            out.push('"');
        }
    }
    out.push_str(",\"subject\":");
    match &row.subject {
        None => out.push_str("null"),
        Some(subject) => push_json_string(&mut out, &subject.0),
    }
    out.push_str(",\"outsets\":");
    match &row.outsets {
        None => out.push_str("null"),
        Some(outsets) => {
            out.push('[');
            out.push_str(&outsets[0].to_string());
            out.push(',');
            out.push_str(&outsets[1].to_string());
            out.push(',');
            out.push_str(&outsets[2].to_string());
            out.push(',');
            out.push_str(&outsets[3].to_string());
            out.push(']');
        }
    }
    out.push_str(",\"width\":");
    match row.width {
        None => out.push_str("null"),
        Some(width) => out.push_str(&width.to_string()),
    }
    out.push_str(",\"radius\":");
    match row.radius {
        None => out.push_str("null"),
        Some(radius) => out.push_str(&radius.to_string()),
    }
    out.push_str(",\"witness\":");
    match &row.witness {
        None => out.push_str("null"),
        Some(witness) => out.push_str(&encode_frame_witness(witness)?),
    }
    out.push('}');
    decode_border_presentation_v4_str(&out)?;
    Ok(out)
}

/// Row string entry point shared by the staged v4 row decoders: 4 MiB input
/// gate (mirroring the current route) plus duplicate-key rejection at every
/// depth (including escaped duplicates) before the shape decode.
fn decode_row_str<T>(json: &str, decode: fn(&serde_json::Value) -> Result<T>) -> Result<T> {
    if json.len() > WORLD_MAX_REQUEST_BYTES {
        return Err(WorldProtocolError {
            kind: KIND_OVERSIZED,
            message: MSG_OVERSIZED,
        });
    }
    let value: serde_json::Value =
        crate::connection_protocol::parse_strict_json(json).map_err(|e| {
            if e.0 == "oversized" {
                WorldProtocolError {
                    kind: KIND_OVERSIZED,
                    message: MSG_OVERSIZED,
                }
            } else {
                malformed()
            }
        })?;
    decode(&value)
}

fn push_rect(out: &mut String, rect: &Rect) {
    out.push('[');
    out.push_str(&rect.x.to_string());
    out.push(',');
    out.push_str(&rect.y.to_string());
    out.push(',');
    out.push_str(&rect.w.to_string());
    out.push(',');
    out.push_str(&rect.h.to_string());
    out.push(']');
}

fn push_opt_rect(out: &mut String, rect: &Option<Rect>) {
    match rect {
        None => out.push_str("null"),
        Some(rect) => push_rect(out, rect),
    }
}

fn push_opt_bool(out: &mut String, value: &Option<bool>) {
    match value {
        None => out.push_str("null"),
        Some(true) => out.push_str("true"),
        Some(false) => out.push_str("false"),
    }
}

fn decode_core_command(map: &serde_json::Map<String, serde_json::Value>) -> Result<CoreCommand> {
    let op = req_str(map, "op")?;
    match op {
        "reconcile" => {
            check_keys(map, &["op"])?;
            Ok(CoreCommand::Reconcile)
        }
        "update-gaps" => {
            check_keys(map, &["op"])?;
            Ok(CoreCommand::UpdateGaps)
        }
        "release-domain" => {
            check_keys(map, &["op"])?;
            Ok(CoreCommand::ReleaseDomain)
        }
        "move" => {
            check_keys(map, &["op", "win", "direction", "crossOutput", "sameAxis"])?;
            let same_axis = match req_str(map, "sameAxis")? {
                "group-with-neighbor" => tiler_core::directional::SameAxisMove::GroupWithNeighbor,
                "swap-with-neighbor" => tiler_core::directional::SameAxisMove::SwapWithNeighbor,
                _ => return Err(unknown_value()),
            };
            Ok(CoreCommand::Move {
                window: req_id(map, "win")?,
                direction: parse_direction_token(req_str(map, "direction")?)?,
                cross_output_transfer: req_bool(map, "crossOutput")?,
                same_axis_move: same_axis,
            })
        }
        "focus" => {
            check_keys(
                map,
                &["op", "win", "direction", "crossOutput", "floatSubject"],
            )?;
            Ok(CoreCommand::Focus {
                window: req_id(map, "win")?,
                direction: parse_direction_token(req_str(map, "direction")?)?,
                cross_output_transfer: req_bool(map, "crossOutput")?,
                float_subject: req_bool(map, "floatSubject")?,
            })
        }
        "resize" => {
            check_keys(map, &["op", "win", "direction", "mode", "press"])?;
            let mode = match req_str(map, "mode")? {
                "inwards" | "outwards" | "horizontal" | "vertical" => {
                    req_str(map, "mode")?.to_owned()
                }
                _ => return Err(unknown_value()),
            };
            let press = map
                .get("press")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(malformed)?;
            Ok(CoreCommand::Resize {
                window: req_id(map, "win")?,
                direction: parse_direction_token(req_str(map, "direction")?)?,
                mode,
                press_index: press,
            })
        }
        "pointer-resize" => {
            check_keys(
                map,
                &[
                    "op",
                    "win",
                    "direction",
                    "boundary",
                    "direction2",
                    "boundary2",
                ],
            )?;
            let direction2 = match map.get("direction2") {
                None => return Err(malformed()),
                Some(serde_json::Value::Null) => None,
                Some(v) => Some(parse_direction_token(v.as_str().ok_or_else(malformed)?)?),
            };
            let boundary2 = match map.get("boundary2") {
                None => return Err(malformed()),
                Some(serde_json::Value::Null) => None,
                Some(v) => Some(
                    v.as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .ok_or_else(malformed)?,
                ),
            };
            if direction2.is_some() != boundary2.is_some() {
                return Err(unknown_value());
            }
            Ok(CoreCommand::PointerResize {
                window: req_id(map, "win")?,
                direction: parse_direction_token(req_str(map, "direction")?)?,
                boundary: req_i32(map, "boundary")?,
                direction2,
                boundary2,
            })
        }
        "toggle-float" => {
            check_keys(map, &["op", "win", "rect"])?;
            Ok(CoreCommand::ToggleFloat {
                window: req_id(map, "win")?,
                float_rect: opt_rect_or_null(map, "rect")?,
            })
        }
        "toggle-orientation" => {
            check_keys(map, &["op", "win"])?;
            Ok(CoreCommand::ToggleOrientation {
                window: req_id(map, "win")?,
            })
        }
        "send-to-workspace" => {
            check_keys(map, &["op", "win", "out", "ws", "follow"])?;
            Ok(CoreCommand::SendToWorkspace {
                window: req_id(map, "win")?,
                target_output: req_id(map, "out")?,
                target_workspace: req_id(map, "ws")?,
                follow: req_bool(map, "follow")?,
            })
        }
        "send-to-output" => {
            check_keys(map, &["op", "win", "out", "ws", "follow"])?;
            Ok(CoreCommand::SendToOutput {
                window: req_id(map, "win")?,
                target_output: req_id(map, "out")?,
                target_workspace: req_id(map, "ws")?,
                follow: req_bool(map, "follow")?,
            })
        }
        "migrate-workspace" => {
            check_keys(map, &["op", "direction"])?;
            Ok(CoreCommand::MigrateWorkspace {
                direction: parse_direction_token(req_str(map, "direction")?)?,
            })
        }
        "drag-drop" | "drag-preview" => {
            check_keys(map, &["op", "win", "x", "y", "hover", "source"])?;
            let hover = match map.get("hover") {
                None => return Err(malformed()),
                Some(serde_json::Value::Null) => None,
                Some(value) => decode_hover_prior(value)?,
            };
            let source = decode_drag_source(map.get("source").ok_or_else(malformed)?)?;
            let window = req_id(map, "win")?;
            let x = req_i32(map, "x")?;
            let y = req_i32(map, "y")?;
            if op == "drag-drop" {
                Ok(CoreCommand::DragDrop {
                    window,
                    x,
                    y,
                    hover_prior: hover,
                    source,
                })
            } else {
                Ok(CoreCommand::DragPreview {
                    window,
                    x,
                    y,
                    hover_prior: hover,
                    source,
                })
            }
        }
        _ => Err(unknown_value()),
    }
}

fn decode_command(value: &serde_json::Value) -> Result<WorldCommand> {
    let map = as_object(value)?;
    let op = req_str(map, "op")?;
    match op {
        "focus" => {
            check_keys(map, &["op", "win", "direction"])?;
            Ok(WorldCommand::Focus {
                window: WindowId(req_id(map, "win")?),
                direction: parse_direction_token(req_str(map, "direction")?)?,
            })
        }
        "send" => {
            check_keys(map, &["op", "win", "out", "ws", "follow"])?;
            Ok(WorldCommand::Send {
                window: WindowId(req_id(map, "win")?),
                target_output: OutputId(req_id(map, "out")?),
                target_workspace: req_id(map, "ws")?,
                follow: req_bool(map, "follow")?,
            })
        }
        "switch-view" => {
            check_keys(map, &["op", "out", "ws"])?;
            Ok(WorldCommand::SwitchView {
                output: OutputId(req_id(map, "out")?),
                workspace: req_id(map, "ws")?,
            })
        }
        "select-workspace" => {
            check_keys(map, &["op", "out", "ws"])?;
            Ok(WorldCommand::SelectWorkspace {
                output: OutputId(req_id(map, "out")?),
                workspace: req_id(map, "ws")?,
            })
        }
        "select-numbered" => {
            check_keys(map, &["op", "out", "index"])?;
            let index = map
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .ok_or_else(malformed)?;
            if !(1..=9).contains(&index) {
                return Err(unknown_value());
            }
            // `null` resolves the output in core from the cursor-first
            // switch target; an opaque id names a genuinely semantic
            // explicit output.
            let output = opt_id_or_null(map, "out")?.map(OutputId);
            Ok(WorldCommand::SelectNumbered { output, index })
        }
        "select-relative" => {
            check_keys(map, &["op", "out", "delta"])?;
            let delta = map
                .get("delta")
                .and_then(serde_json::Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(malformed)?;
            if delta != -1 && delta != 1 {
                return Err(unknown_value());
            }
            let output = opt_id_or_null(map, "out")?.map(OutputId);
            Ok(WorldCommand::SelectRelative { output, delta })
        }
        "select-previous" => {
            check_keys(map, &["op", "out"])?;
            let output = opt_id_or_null(map, "out")?.map(OutputId);
            Ok(WorldCommand::SelectPrevious { output })
        }
        "select-trailing" => {
            check_keys(map, &["op", "out"])?;
            let output = opt_id_or_null(map, "out")?.map(OutputId);
            Ok(WorldCommand::SelectTrailing { output })
        }
        "send-relative" => {
            check_keys(map, &["op", "win", "source", "delta", "follow"])?;
            let delta = map
                .get("delta")
                .and_then(serde_json::Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(malformed)?;
            if delta != -1 && delta != 1 {
                return Err(unknown_value());
            }
            // The source selects the pre-observation retained home or an
            // explicit output; only the two named implicit fallbacks exist.
            let source = decode_send_source(map, "source")?;
            Ok(WorldCommand::SendRelative {
                window: WindowId(req_id(map, "win")?),
                source,
                delta,
                follow: req_bool(map, "follow")?,
            })
        }
        "send-numbered" => {
            check_keys(map, &["op", "win", "source", "index", "follow"])?;
            let index = map
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .ok_or_else(malformed)?;
            // 0 is the trailing path; 1..=9 are ring positions.
            if index > 9 {
                return Err(unknown_value());
            }
            let source = decode_send_source(map, "source")?;
            Ok(WorldCommand::SendNumbered {
                window: WindowId(req_id(map, "win")?),
                source,
                index,
                follow: req_bool(map, "follow")?,
            })
        }
        "set-workspace-tiling" => {
            check_keys(map, &["op", "out", "ws", "tiled"])?;
            Ok(WorldCommand::SetWorkspaceTiling {
                output: OutputId(req_id(map, "out")?),
                workspace: req_id(map, "ws")?,
                tiled: req_bool(map, "tiled")?,
            })
        }
        "migrate-workspace" => {
            check_keys(map, &["op", "from", "to", "ws"])?;
            let from = req_id(map, "from")?;
            let to = req_id(map, "to")?;
            if from == to {
                return Err(unknown_value());
            }
            Ok(WorldCommand::MigrateWorkspace {
                source_output: OutputId(from),
                target_output: OutputId(to),
                workspace: req_id(map, "ws")?,
            })
        }
        "focus-directional" => {
            check_keys(map, &["op", "win", "direction"])?;
            // Explicit empty `win` is this command's no-focus marker (the
            // core starts from the current workspace/output); every other
            // form still names an opaque subject window.
            let win = req_str(map, "win")?;
            if !(win.is_empty() || is_opaque_id(win)) {
                return Err(unknown_value());
            }
            Ok(WorldCommand::FocusDirectional {
                window: WindowId(win.to_owned()),
                direction: parse_direction_token(req_str(map, "direction")?)?,
            })
        }
        "move-directional" => {
            check_keys(map, &["op", "win", "direction", "sameAxis"])?;
            let same_axis =
                tiler_core::directional::SameAxisMove::parse_wire(req_str(map, "sameAxis")?)
                    .ok_or_else(unknown_value)?;
            Ok(WorldCommand::MoveDirectional {
                window: WindowId(req_id(map, "win")?),
                direction: parse_direction_token(req_str(map, "direction")?)?,
                same_axis,
            })
        }
        "send-directional" => {
            check_keys(map, &["op", "win", "direction", "follow"])?;
            Ok(WorldCommand::SendDirectional {
                window: WindowId(req_id(map, "win")?),
                direction: parse_direction_token(req_str(map, "direction")?)?,
                follow: req_bool(map, "follow")?,
            })
        }
        "migrate-directional" => {
            // Direction only: the core resolves the action source and its
            // current view from the neutral report facts. Legacy `from`/`ws`
            // keys are deleted and refuse as unknown fields.
            check_keys(map, &["op", "direction"])?;
            Ok(WorldCommand::MigrateDirectional {
                direction: parse_direction_token(req_str(map, "direction")?)?,
            })
        }
        "toggle-workspace-tiling" => {
            check_keys(map, &["op", "out", "ws"])?;
            Ok(WorldCommand::ToggleWorkspaceTiling {
                output: OutputId(req_id(map, "out")?),
                workspace: req_id(map, "ws")?,
            })
        }
        "toggle-maximize" => {
            check_keys(map, &["op", "win"])?;
            Ok(WorldCommand::ToggleMaximize {
                window: WindowId(req_id(map, "win")?),
            })
        }
        "set-sticky" => {
            check_keys(map, &["op", "win", "value"])?;
            Ok(WorldCommand::SetSticky {
                window: WindowId(req_id(map, "win")?),
                value: req_bool(map, "value")?,
            })
        }
        "toggle-sticky" => {
            check_keys(map, &["op", "win"])?;
            Ok(WorldCommand::ToggleSticky {
                window: WindowId(req_id(map, "win")?),
            })
        }
        "set-fullscreen" => {
            check_keys(map, &["op", "win", "value"])?;
            Ok(WorldCommand::SetFullscreen {
                window: WindowId(req_id(map, "win")?),
                value: req_bool(map, "value")?,
            })
        }
        "toggle-fullscreen" => {
            check_keys(map, &["op", "win"])?;
            Ok(WorldCommand::ToggleFullscreen {
                window: WindowId(req_id(map, "win")?),
            })
        }
        "operation" => {
            check_keys(map, &["op", "command"])?;
            let inner = map.get("command").ok_or_else(malformed)?;
            let inner_map = as_object(inner)?;
            Ok(WorldCommand::Operation(decode_core_command(inner_map)?))
        }
        _ => Err(unknown_value()),
    }
}

/// Optional cursor-hit wire spelling (report-level `cursorHit`): the
/// normalized [`WorldReportV4`](tiler_core::world_report::WorldReportV4) `cursor_hit` value stays in the typed report;
/// this records which tolerant spelling carried it. `Absent` means the key
/// was omitted, `Null` an explicit `null`, `Empty` an explicit `""`, and
/// `Id` a validated opaque lifetime. `Absent`/`Null`/`Empty` normalize to
/// `None`; `Id` normalizes to `Some`. The id itself stays in the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorHitWire {
    Absent,
    Null,
    Empty,
    Id,
}

/// Optional gesture-ingress wire presence (report-level `gestureIngress`):
/// `Absent` means the key was omitted, `Null` an explicit `null` (both
/// normalize to `None`), and `Present` a validated object (normalizes to
/// `Some`). The normalized value stays in the typed report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureIngressWire {
    Absent,
    Null,
    Present,
}

/// Report-level optional wire metadata: which tolerant spellings the
/// request carried. Normalized values stay in [`WorldReportV4`](tiler_core::world_report::WorldReportV4); this only
/// records presence/spelling so callers can reconstruct the exact carried
/// optional groups. Never influences [`WorldReportV4`](tiler_core::world_report::WorldReportV4) equality, the report
/// digest, or engine input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportWireOptionals {
    pub cursor_hit: CursorHitWire,
    pub monotonic_ms_present: bool,
    pub frame_attempts_present: bool,
    pub member_attempts_present: bool,
    pub view_attempts_present: bool,
    pub clear_attempts_present: bool,
    pub creation_attempts_present: bool,
    pub gesture_ingress: GestureIngressWire,
}

/// Decode one JSON world request into the typed core v4 report. This is
/// the sole public report ingress: the genuine v4 codec
/// ([`decode_report_v4`]), which enforces the v4 envelope keys
/// (`rendererInput` required, `views` refused as `unknown-field`) and the
/// v4 contract version. No legacy dual decode and no report conversion.
pub fn decode_request(json: &str) -> std::result::Result<WorldReportV4, WorldProtocolError> {
    decode_report_v4(json).map(|decoded| decoded.report)
}

/// Decode one JSON world request into the typed core v4 report plus its
/// optional wire metadata. This is the sole public wired report ingress:
/// the genuine v4 codec ([`decode_report_v4`]). No legacy dual decode
/// and no report conversion.
pub fn decode_request_with_wire(
    json: &str,
) -> std::result::Result<DecodedReportV4, WorldProtocolError> {
    decode_report_v4(json)
}

// -- V4 whole-report decoder --
// Genuine v4 codec for the v4 contract (`rendererInput` carried, `views`
// refused, identifiable bad rows held narrowly instead of refusing the
// envelope). Snapshot refusal vocabulary is reused verbatim from the core gate:
// no new error metadata is minted here.

/// V4 world protocol version: `rendererInput` carried, `views` refused,
/// identifiable bad rows held narrowly. Equals [`WORLD_PROTOCOL_VERSION`]:
/// the public contract is v4.
pub const WORLD_PROTOCOL_VERSION_V4: u32 = tiler_core::world_report::WORLD_CONTRACT_VERSION_V4;

/// Per-valid-row creation-receipt wire spelling (lane-level
/// `creationAttempts[].created`): the normalized
/// [`tiler_core::world_report::NativeCreationAttempt::created_native`] value
/// stays in the typed row; this records which tolerant spelling carried it.
/// `Absent` means the key was omitted, `Null` an explicit `null`, `Empty`
/// an explicit `""`, and `Id` a validated opaque lifetime.
/// `Absent`/`Null`/`Empty` normalize to `None`; `Id` normalizes to `Some`.
/// The id itself stays in the row. Held rows keep their raw fields plus
/// ordinal instead, so this vector aligns to the valid creation rows only,
/// in order. Never influences report equality, the digest, or engine input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationReceiptWire {
    Absent,
    Null,
    Empty,
    Id,
}

/// Decoded v4 request plus its optional wire metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedReportV4 {
    pub report: WorldReportV4,
    pub wire: ReportWireOptionals,
    /// Creation-receipt spellings aligned to [`WorldReportV4::creation_attempts`]
    /// in order (one entry per valid row).
    pub creation_receipts: Vec<CreationReceiptWire>,
}

/// Staged v4 attempt-row outcome: either a usable row or an identifiable
/// row held narrowly with its carried fields retained.
enum AttemptRowOutcome<T> {
    Valid(T),
    Held(HeldAttemptRow),
}

/// Staged v4 native-row outcome: either a usable row or an identifiable
/// row held narrowly with its carried fields retained.
enum NativeRowOutcome {
    Valid(NativeObservation),
    Held(HeldNativeRow),
}

/// Retain one identifiable-but-bad v4 attempt row: every carried key in
/// the lane's canonical schema order, then any other carried keys
/// lexicographically, as JSON-agnostic retained facts.
fn held_attempt_row(
    lane: AttemptLaneV4,
    ordinal: usize,
    map: &serde_json::Map<String, serde_json::Value>,
    schema: &[&str],
    identity: Vec<String>,
    diagnostic: Option<RowDiagnostic>,
) -> HeldAttemptRow {
    let mut fields = Vec::with_capacity(map.len());
    for key in schema {
        if let Some(value) = map.get(*key) {
            fields.push((key.to_string(), raw_setting_value(value)));
        }
    }
    let mut extra: Vec<&String> = map
        .keys()
        .filter(|key| !schema.contains(&key.as_str()))
        .collect();
    extra.sort();
    for key in extra {
        fields.push((key.clone(), raw_setting_value(&map[key])));
    }
    HeldAttemptRow {
        lane,
        ordinal,
        identity,
        fields,
        diagnostic,
        kind: RowDiagnosticKind::InvalidFact,
    }
}

/// Attempt-hold scope: the row's actual reported target output, when the
/// carried `out` is a valid opaque id. Rows without an output (frame
/// attempts name none) carry no scope authority (`None`).
fn attempt_hold_scope(map: &serde_json::Map<String, serde_json::Value>) -> Option<RowDiagnostic> {
    match map.get("out") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => Some(RowDiagnostic {
            scope: AuthorityScope::Output(OutputId(text.clone())),
            row: RowDiagnosticLane::Attempt,
            id: Some(text.clone()),
            kind: RowDiagnosticKind::InvalidFact,
        }),
        _ => None,
    }
}

/// Reorder one retained raw object value into canonical wire order: the
/// ordered keys first, then any other carried keys lexicographically.
/// Non-object values pass through untouched.
fn order_raw_fields(
    entries: Vec<(String, RawSettingValue)>,
    order: &[&str],
) -> Vec<(String, RawSettingValue)> {
    let mut rest: std::collections::BTreeMap<String, RawSettingValue> =
        entries.into_iter().collect();
    let mut ordered = Vec::with_capacity(rest.len());
    for key in order {
        if let Some(value) = rest.remove(*key) {
            ordered.push((key.to_string(), value));
        }
    }
    for (key, value) in rest {
        ordered.push((key, value));
    }
    ordered
}

/// Reorder every object item of a retained raw array value into canonical
/// wire order (see [`order_raw_fields`]). Non-array values and non-object
/// items pass through untouched.
fn reorder_retained_items(value: &mut RawSettingValue, order: &[&str]) {
    let RawSettingValue::Array(items) = value else {
        return;
    };
    for item in items.iter_mut() {
        if let RawSettingValue::Object(entries) = item {
            let taken = std::mem::take(entries);
            *entries = order_raw_fields(taken, order);
        }
    }
}

/// Retain one identifiable-but-bad v4 native row: every carried key in
/// canonical schema key order (`out`, `order`, `current`, `occupancy`),
/// then any other carried keys lexicographically, with an `InvalidFact`
/// diagnostic over the output scope naming the output. The nested
/// `occupancy` items are retained in wire order (`native`, `count`, then
/// unknown sub-keys lexicographically) so the canonical writer spells the
/// exact carried row.
fn held_native_row(
    output: &OutputId,
    ordinal: usize,
    map: &serde_json::Map<String, serde_json::Value>,
) -> HeldNativeRow {
    const SCHEMA_ORDER: &[&str] = &["out", "order", "current", "occupancy"];
    let mut fields = Vec::with_capacity(map.len());
    for key in SCHEMA_ORDER {
        if let Some(value) = map.get(*key) {
            fields.push((key.to_string(), raw_setting_value(value)));
        }
    }
    let mut extra: Vec<&String> = map
        .keys()
        .filter(|key| !SCHEMA_ORDER.contains(&key.as_str()))
        .collect();
    extra.sort();
    for key in extra {
        fields.push((key.clone(), raw_setting_value(&map[key])));
    }
    for (key, value) in fields.iter_mut() {
        if key == "occupancy" {
            reorder_retained_items(value, &["native", "count"]);
        }
    }
    HeldNativeRow {
        output: output.clone(),
        ordinal,
        fields,
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(output.clone()),
            row: RowDiagnosticLane::Native,
            id: Some(output.0.clone()),
            kind: RowDiagnosticKind::InvalidFact,
        },
    }
}

/// Retain one held v4 presentation ingress: every carried key in canonical
/// contract table order, then any other carried keys lexicographically.
/// The nested `pagerFrames` items are retained in wire order (`win`,
/// `live`, then unknown sub-keys lexicographically) and the `witness`
/// object in wire order (`win`, `obs`, `frame`, `foreground`, then unknown
/// sub-keys lexicographically) so the canonical writer spells the exact
/// carried row.
/// The diagnostic is `Some` only when the carried `foregroundOut` names a
/// truthful output scope, otherwise `None` (no scope is fabricated).
fn held_presentation_row(map: &serde_json::Map<String, serde_json::Value>) -> HeldPresentationRow {
    const SCHEMA_ORDER: &[&str] = &[
        "foreground",
        "minimized",
        "maximized",
        "fullscreen",
        "hostHidden",
        "hiddenWorkspace",
        "frame",
        "chordHeld",
        "moveWindow",
        "pagerOrder",
        "pagerFrames",
        "witness",
        "foregroundOut",
    ];
    let mut fields = Vec::with_capacity(map.len());
    for key in SCHEMA_ORDER {
        if let Some(value) = map.get(*key) {
            fields.push((key.to_string(), raw_setting_value(value)));
        }
    }
    let mut extra: Vec<&String> = map
        .keys()
        .filter(|key| !SCHEMA_ORDER.contains(&key.as_str()))
        .collect();
    extra.sort();
    for key in extra {
        fields.push((key.clone(), raw_setting_value(&map[key])));
    }
    for (key, value) in fields.iter_mut() {
        if key == "pagerFrames" {
            reorder_retained_items(value, &["win", "live"]);
        } else if key == "witness"
            && let RawSettingValue::Object(entries) = value
        {
            let taken = std::mem::take(entries);
            *entries = order_raw_fields(taken, &["win", "obs", "frame", "foreground"]);
        }
    }
    let diagnostic = match map.get("foregroundOut") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => Some(RowDiagnostic {
            scope: AuthorityScope::Output(OutputId(text.clone())),
            row: RowDiagnosticLane::Presentation,
            id: Some(text.clone()),
            kind: RowDiagnosticKind::InvalidFact,
        }),
        _ => None,
    };
    HeldPresentationRow {
        fields,
        kind: RowDiagnosticKind::InvalidFact,
        diagnostic,
    }
}

/// Staged v4 window-row decode: same carried keys as v3, but identifiable
/// value failures hold the row narrowly instead of refusing the envelope.
/// - Missing `win` refuses (`malformed`); a bad `win` id refuses
///   (`unknown-value`); duplicates refuse via the shared seen set.
/// - Missing `executable` refuses (`malformed`): process identity stays
///   required. Every other missing required-nullable value key holds.
/// - Missing/wrong-typed `out` refuses (`malformed`); a bad `out` id and
///   bad `owner`/`parent` grammar refuse (`unknown-value`) even with a hold
///   cause (existing rule).
/// - Unknown non-retired keys refuse (`unknown-field`); retired membership
///   keys hold; every other value failure (wrong types, unknown tokens,
///   out-of-range layers, empty/NUL text, bad rects, bad nested
///   `hints`/`visibility`/`lastWrite`) holds the identifiable row.
/// - Structural id/duplicate/rect violations refuse before any narrow hold
///   (shared policy with the core structural check).
fn decode_fact_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_windows: &mut std::collections::BTreeSet<String>,
) -> Result<WindowRowOutcome> {
    const REQUIRED: &[&str] = &[
        "win",
        "out",
        "rect",
        "fullscreen",
        "maximized",
        "sticky",
        "minimized",
        "hints",
        "hostHidden",
        "privileged",
        "managed",
        "acceptsInput",
        "activationInhibited",
        "toolWindow",
        "owned",
        "transient",
        "modal",
        "resizable",
        "above",
        "below",
        "owner",
        "parent",
        "role",
        "appId",
        "executable",
        "processId",
        "class",
        "title",
        "type",
        "activationPolicy",
        "layer",
        "fullscreenOwner",
        "fullscreenExitable",
        "suppressionAvailable",
        "visibility",
        "lastWrite",
    ];
    let map = as_object(value)?;
    // v4 tolerant envelope: unknown non-retired keys refuse, but missing
    // required-nullable keys hold (except identity/process keys which stay
    // strict: missing `win`/`out`/`executable` is `malformed`).
    const OPTIONAL_V4: &[&str] = &[
        "nativeWs",
        "floating",
        "fitExcluded",
        "fixedAuto",
        "fixedSuppress",
        "observedWs",
        "shell",
        "tool",
        "ownedDialog",
        "dialog",
        "noActivate",
        "cannotControl",
    ];
    if map
        .keys()
        .any(|k| !REQUIRED.contains(&k.as_str()) && !OPTIONAL_V4.contains(&k.as_str()))
    {
        return Err(unknown_field());
    }
    for key in ["win", "out", "executable"] {
        if !map.contains_key(key) {
            return Err(malformed());
        }
    }
    let window = match map.get("win") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => WindowId(text.clone()),
        Some(serde_json::Value::String(_)) => return Err(unknown_value()),
        _ => return Err(malformed()),
    };
    // Lenient rect: a missing or bad frame holds the identifiable row
    // (never a guessed frame, never a refusal).
    let (rect, mut held) = match map.get("rect") {
        None => (None, true),
        Some(serde_json::Value::Null) => (None, false),
        Some(value) => match req_rect_value(value) {
            Ok(rect) => (Some(rect), false),
            Err(_) => (None, true),
        },
    };
    let output = match map.get("out") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            Some(OutputId(text.clone()))
        }
        Some(_) => return Err(malformed()),
    };
    // Lenient hints: a missing or bad hint set holds the identifiable row.
    let (hints, hints_held) = match map.get("hints") {
        None => (None, true),
        Some(serde_json::Value::Null) => (None, false),
        Some(value) => match decode_hints(value) {
            Ok(hints) => (Some(hints), false),
            Err(_) => (None, true),
        },
    };
    held |= hints_held;
    // Lenient native workspace: a bad shape holds the identifiable row.
    let (native_workspace, native_ws_held) = match map.get("nativeWs") {
        None => (NativeWorkspaceFact::Omitted, false),
        Some(serde_json::Value::Null) => (NativeWorkspaceFact::Null, false),
        Some(serde_json::Value::String(text)) if text.is_empty() => {
            (NativeWorkspaceFact::Value(String::new()), false)
        }
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                (NativeWorkspaceFact::Omitted, true)
            } else {
                (NativeWorkspaceFact::Value(text.clone()), false)
            }
        }
        Some(_) => (NativeWorkspaceFact::Omitted, true),
    };
    held |= native_ws_held;
    // Retired membership verdicts hold the identifiable row (fail closed).
    held |= RETIRED_WINDOW_KEYS.iter().any(|key| map.contains_key(*key));
    // Wrong-typed value fields hold the identifiable row (same pre-scan
    // vocabulary as v3).
    fn is_wrong_bool(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> bool {
        matches!(map.get(key), Some(v) if !v.is_null() && !v.is_boolean())
    }
    for key in [
        "fullscreen",
        "maximized",
        "sticky",
        "minimized",
        "hostHidden",
        "privileged",
        "managed",
        "acceptsInput",
        "activationInhibited",
        "toolWindow",
        "owned",
        "transient",
        "modal",
        "resizable",
        "above",
        "below",
        "fullscreenExitable",
        "suppressionAvailable",
    ] {
        if is_wrong_bool(map, key) {
            held = true;
        }
    }
    for key in [
        "owner",
        "parent",
        "role",
        "appId",
        "executable",
        "processId",
        "class",
        "title",
        "type",
        "activationPolicy",
        "fullscreenOwner",
    ] {
        match map.get(key) {
            None | Some(serde_json::Value::Null) => {}
            Some(serde_json::Value::String(_)) => {}
            Some(_) => {
                held = true;
            }
        }
    }
    if matches!(map.get("layer"), Some(v) if !v.is_null() && !v.is_number()) {
        held = true;
    }
    if matches!(map.get("visibility"), Some(v) if !v.is_object()) {
        held = true;
    }
    if matches!(map.get("lastWrite"), Some(v) if !v.is_null() && !v.is_object()) {
        held = true;
    }
    for key in ["role", "appId", "executable", "class", "title"] {
        if let Some(serde_json::Value::String(text)) = map.get(key)
            && text.len() > tiler_core::world_report::MAX_METADATA_TEXT_BYTES
        {
            held = true;
        }
    }
    if let Some(serde_json::Value::String(text)) = map.get("processId")
        && !tiler_core::connection::valid_process_identity_text(text)
    {
        held = true;
    }
    // v4 deltas: unknown closed tokens, out-of-range layers, and
    // empty/NUL text hold the identifiable row instead of refusing it.
    for key in ["type"] {
        if let Some(serde_json::Value::String(text)) = map.get(key)
            && WindowType::parse(text).is_none()
        {
            held = true;
        }
    }
    if let Some(serde_json::Value::String(text)) = map.get("activationPolicy")
        && ActivationPolicy::parse(text).is_none()
    {
        held = true;
    }
    if let Some(serde_json::Value::String(text)) = map.get("fullscreenOwner")
        && FullscreenOwner::parse(text).is_none()
    {
        held = true;
    }
    if let Some(value) = map.get("layer")
        && !value.is_null()
        && value.as_i64().and_then(|n| i32::try_from(n).ok()).is_none()
    {
        held = true;
    }
    // v4 normative text split: `role`/`appId`/`class`/`title` are bounded
    // text and accept arbitrary UTF-8 including empty/NUL/controls within
    // the bound (no hold here); `executable` keeps the nonempty NUL-free
    // identity grammar and `processId` keeps the opaque-Id grammar (held
    // below when bad).
    for key in ["executable"] {
        if let Some(serde_json::Value::String(text)) = map.get(key)
            && !valid_identity_text(text)
            && text.len() <= tiler_core::world_report::MAX_METADATA_TEXT_BYTES
        {
            held = true;
        }
    }
    // Bad nested `visibility`/`lastWrite` shapes hold the row.
    if let Some(value) = map.get("visibility")
        && value.is_object()
        && decode_visibility(value).is_err()
    {
        held = true;
    }
    if let Some(value) = map.get("lastWrite")
        && value.is_object()
        && decode_last_write(value).is_err()
    {
        held = true;
    }
    // v4 delta: missing required-nullable value keys hold the identifiable
    // row, except process identity (`executable`) which stays required.
    for key in REQUIRED {
        if *key == "win" || *key == "out" || *key == "executable" {
            continue;
        }
        if !map.contains_key(*key) {
            held = true;
        }
    }
    // Minimal identifiable candidate for the structural gate: win, out,
    // rect, and the duplicate check.
    let candidate = WindowFact {
        window: window.clone(),
        output: output.clone(),
        rect,
        fullscreen: None,
        maximized: None,
        sticky: None,
        minimized: None,
        hints,
        native_workspace,
        host_hidden: None,
        privileged: None,
        managed: None,
        accepts_input: None,
        activation_inhibited: None,
        tool_window: None,
        owned: None,
        transient: None,
        modal: None,
        resizable: None,
        above: None,
        below: None,
        owner: None,
        parent: None,
        role: None,
        app_id: None,
        executable: None,
        process_id: None,
        class: None,
        title: None,
        window_type: None,
        activation_policy: None,
        layer: None,
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: VisibilityFact {
            visible: None,
            suppression_owned: None,
        },
        last_write: None,
    };
    if held {
        // Bad `owner`/`parent` grammar refuses even with a hold cause
        // (existing rule, kept): a hold must not mask it.
        for key in ["owner", "parent"] {
            if let Some(serde_json::Value::String(text)) = map.get(key)
                && !is_opaque_id(text)
            {
                return Err(unknown_value());
            }
        }
        if let Some(reason) = check_window_structure(&candidate, seen_windows) {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: reason,
            });
        }
        return Ok(WindowRowOutcome::Held(held_window_row(
            &candidate, ordinal, map,
        )));
    }
    let fact = WindowFact {
        window: candidate.window.clone(),
        output: candidate.output.clone(),
        rect: candidate.rect,
        fullscreen: opt_bool_or_null(map, "fullscreen")?,
        maximized: opt_bool_or_null(map, "maximized")?,
        sticky: opt_bool_or_null(map, "sticky")?,
        minimized: opt_bool_or_null(map, "minimized")?,
        hints: candidate.hints,
        native_workspace: candidate.native_workspace.clone(),
        host_hidden: opt_bool_or_null(map, "hostHidden")?,
        privileged: opt_bool_or_null(map, "privileged")?,
        managed: opt_bool_or_null(map, "managed")?,
        accepts_input: opt_bool_or_null(map, "acceptsInput")?,
        activation_inhibited: opt_bool_or_null(map, "activationInhibited")?,
        tool_window: opt_bool_or_null(map, "toolWindow")?,
        owned: opt_bool_or_null(map, "owned")?,
        transient: opt_bool_or_null(map, "transient")?,
        modal: opt_bool_or_null(map, "modal")?,
        resizable: opt_bool_or_null(map, "resizable")?,
        above: opt_bool_or_null(map, "above")?,
        below: opt_bool_or_null(map, "below")?,
        owner: opt_window_or_null(map, "owner")?,
        parent: opt_window_or_null(map, "parent")?,
        role: opt_bounded_text_or_null(map, "role")?,
        app_id: opt_bounded_text_or_null(map, "appId")?,
        executable: opt_text_or_null(map, "executable")?,
        process_id: opt_process_or_null(map, "processId")?,
        class: opt_bounded_text_or_null(map, "class")?,
        title: opt_bounded_text_or_null(map, "title")?,
        window_type: opt_token_or_null(map, "type", WindowType::parse)?,
        activation_policy: opt_token_or_null(map, "activationPolicy", ActivationPolicy::parse)?,
        layer: opt_layer_or_null(map, "layer")?,
        fullscreen_owner: opt_token_or_null(map, "fullscreenOwner", FullscreenOwner::parse)?,
        fullscreen_exitable: opt_bool_or_null(map, "fullscreenExitable")?,
        suppression_available: opt_bool_or_null(map, "suppressionAvailable")?,
        visibility: decode_visibility(map.get("visibility").ok_or_else(malformed)?)?,
        last_write: match map.get("lastWrite") {
            None => return Err(malformed()),
            Some(serde_json::Value::Null) => None,
            Some(value) => Some(decode_last_write(value)?),
        },
    };
    if let Some(reason) = check_window_structure(&fact, seen_windows) {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: reason,
        });
    }
    Ok(WindowRowOutcome::Valid(Box::new(fact)))
}

/// Staged v4 native-row decode: same carried keys as v3, but a missing,
/// empty, or bad-grammar `current` and any carried extra key hold the
/// identifiable row instead of refusing the envelope. A missing or
/// bad-grammar `out` refuses (unidentifiable); row-local order and
/// occupancy coverage refuse before any narrow hold (structural binding
/// evidence, never masked by the hold).
fn decode_native_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_native_outputs: &mut std::collections::BTreeSet<String>,
) -> Result<NativeRowOutcome> {
    const REQUIRED: &[&str] = &["out", "order", "current", "occupancy"];
    let map = as_object(value)?;
    for key in REQUIRED {
        if !map.contains_key(*key) && *key != "current" {
            // Only `current` may be absent (held); every other missing key
            // is malformed.
            return Err(malformed());
        }
    }
    let output = match map.get("out") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => OutputId(text.clone()),
        Some(serde_json::Value::String(_)) => return Err(unknown_value()),
        _ => return Err(malformed()),
    };
    let order_value = map.get("order").ok_or_else(malformed)?;
    let order_items = order_value.as_array().ok_or_else(malformed)?;
    let mut order = Vec::with_capacity(order_items.len());
    for item in order_items {
        let id = item.as_str().ok_or_else(malformed)?;
        if !is_opaque_id(id) {
            return Err(unknown_value());
        }
        order.push(id.to_owned());
    }
    // Lenient current: missing, empty, or bad-grammar holds the row.
    let (current_native, current_held) = match map.get("current") {
        None => (None, true),
        Some(serde_json::Value::Null) => (None, false),
        Some(serde_json::Value::String(text)) => {
            if text.is_empty() || !is_opaque_id(text) {
                (None, true)
            } else {
                (Some(text.clone()), false)
            }
        }
        Some(_) => (None, true),
    };
    let extra_held = map.keys().any(|key| !REQUIRED.contains(&key.as_str()));
    let held = current_held || extra_held;
    // Strict occupancy shape (structural binding evidence): wrong shapes
    // refuse even on the hold path.
    let occupancy_value = map.get("occupancy").ok_or_else(malformed)?;
    let occupancy_items = occupancy_value.as_array().ok_or_else(malformed)?;
    let mut occupancy = Vec::with_capacity(occupancy_items.len());
    for item in occupancy_items {
        let row = as_object(item)?;
        check_keys(row, &["native", "count"])?;
        let native_id = match row.get("native") {
            Some(serde_json::Value::String(text)) => {
                if !is_opaque_id(text) {
                    return Err(unknown_value());
                }
                text.clone()
            }
            _ => return Err(malformed()),
        };
        let count = match row.get("count") {
            None => return Err(malformed()),
            Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::Number(number)) => {
                let count = number.as_u64().ok_or_else(unknown_value)?;
                let count = u32::try_from(count).map_err(|_| unknown_value())?;
                if count > tiler_core::world_report::MAX_NATIVE_OCCUPANTS_PER_DESKTOP {
                    return Err(unknown_value());
                }
                Some(count)
            }
            Some(_) => return Err(unknown_value()),
        };
        occupancy.push(tiler_core::world_report::NativeOccupancy {
            native: native_id,
            count,
        });
    }
    if !seen_native_outputs.insert(output.0.clone()) {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "duplicate-native",
        });
    }
    // Row-local structural coverage refuses before any narrow hold
    // (mirrors the core gate): per-output order bound, order uniqueness,
    // exact occupancy coverage.
    if order.len() > tiler_core::world_report::MAX_NATIVE_PER_OUTPUT {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "native-invalid",
        });
    }
    {
        let mut seen_ids = std::collections::BTreeSet::new();
        for id in &order {
            if !seen_ids.insert(id.clone()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-native",
                });
            }
        }
    }
    if occupancy.len() != order.len() {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "native-occupancy-incomplete",
        });
    }
    {
        let mut seen_occupancy = std::collections::BTreeSet::new();
        for row in &occupancy {
            if !seen_occupancy.insert(row.native.clone()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-native-occupancy",
                });
            }
        }
        for id in &order {
            if !seen_occupancy.contains(id) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "native-occupancy-incomplete",
                });
            }
        }
    }
    if held {
        return Ok(NativeRowOutcome::Held(held_native_row(
            &output, ordinal, map,
        )));
    }
    Ok(NativeRowOutcome::Valid(NativeObservation {
        output,
        native_order: order,
        current_native,
        occupancy,
    }))
}

/// Staged v4 frame-attempt decode: same carried keys as v3, but a missing
/// `op`, a missing `pre`, a `pre` without an operation, and any carried
/// extra key hold the identifiable row instead of refusing the envelope.
/// A missing or bad-grammar `win` refuses (unidentifiable); every other
/// value failure stays strict.
fn decode_frame_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_frames: &mut std::collections::BTreeSet<String>,
) -> Result<AttemptRowOutcome<tiler_core::world_report::FrameAttempt>> {
    const SCHEMA: &[&str] = &[
        "win",
        "target",
        "setter",
        "readback",
        "dpiBefore",
        "dpiAfter",
        "attemptMs",
        "op",
        "pre",
    ];
    let map = as_object(value)?;
    let window = match map.get("win") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => WindowId(text.clone()),
        Some(serde_json::Value::String(_)) => return Err(unknown_value()),
        _ => return Err(malformed()),
    };
    let extra = map.keys().any(|key| !SCHEMA.contains(&key.as_str()));
    // Lenient op/pre: missing keys and pre-without-operation hold.
    let mut held = extra;
    let operation_id = match map.get("op") {
        None => {
            held = true;
            None
        }
        Some(serde_json::Value::String(op)) if op.is_empty() => None,
        Some(serde_json::Value::String(op)) => Some(op.clone()),
        Some(_) => {
            held = true;
            None
        }
    };
    let prewrite = match map.get("pre") {
        None => {
            held = true;
            None
        }
        Some(serde_json::Value::Null) => None,
        Some(value) => match req_rect_value(value) {
            Ok(rect) => Some(rect),
            Err(_) => {
                held = true;
                None
            }
        },
    };
    if operation_id.as_deref().is_none_or(str::is_empty) && prewrite.is_some() {
        held = true;
    }
    // Remaining lane shapes stay strict; failures refuse.
    let target = req_rect_value(map.get("target").ok_or_else(malformed)?)?;
    let setter = parse_frame_setter(req_str(map, "setter")?)?;
    let readback = opt_rect_or_null(map, "readback")?;
    let dpi_before = opt_u32_or_null(map, "dpiBefore")?;
    let dpi_after = opt_u32_or_null(map, "dpiAfter")?;
    let attempt_ms = req_u64_str(map, "attemptMs")?;
    if !seen_frames.insert(window.0.clone()) {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "duplicate-frame",
        });
    }
    if held {
        let mut identity = vec![window.0.clone()];
        if let Some(op) = operation_id.as_deref()
            && !op.is_empty()
        {
            identity.push(op.to_owned());
        }
        return Ok(AttemptRowOutcome::Held(held_attempt_row(
            AttemptLaneV4::Frame,
            ordinal,
            map,
            SCHEMA,
            identity,
            None,
        )));
    }
    // Valid rows keep the core structural shape: a nonempty operation id
    // must be opaque (mirrors the core gate).
    if let Some(op) = operation_id.as_deref()
        && !op.is_empty()
        && !is_opaque_id(op)
    {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "frame-operation-invalid",
        });
    }
    Ok(AttemptRowOutcome::Valid(
        tiler_core::world_report::FrameAttempt {
            window,
            target,
            setter,
            readback,
            dpi_before,
            dpi_after,
            attempt_ms,
            operation_id,
            prewrite,
        },
    ))
}

/// Lenient attempt identity: carried `op`/`win` strings when present and
/// opaque, otherwise `None` (the strict error then refuses the envelope).
fn attempt_identity(
    map: &serde_json::Map<String, serde_json::Value>,
    want_op: bool,
    want_win: bool,
) -> (Option<String>, Option<String>) {
    let op = if want_op {
        match map.get("op") {
            Some(serde_json::Value::String(text)) if is_opaque_id(text) => Some(text.clone()),
            _ => None,
        }
    } else {
        None
    };
    let win = if want_win {
        match map.get("win") {
            Some(serde_json::Value::String(text)) if is_opaque_id(text) => Some(text.clone()),
            _ => None,
        }
    } else {
        None
    };
    (op, win)
}

/// Staged v4 member-attempt decode: strict shapes from v3, but any carried
/// extra key or other value failure holds the identifiable (`op` + `win`)
/// row instead of refusing the envelope.
fn decode_member_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_members: &mut std::collections::BTreeSet<(String, String)>,
) -> Result<AttemptRowOutcome<tiler_core::world_report::MemberAttempt>> {
    const SCHEMA: &[&str] = &[
        "op",
        "win",
        "out",
        "ws",
        "setter",
        "pre",
        "post",
        "attemptMs",
    ];
    let map = as_object(value)?;
    let extra = map.keys().any(|key| !SCHEMA.contains(&key.as_str()));
    match decode_member_attempt(value) {
        Ok(attempt) => {
            let key = (attempt.operation_id.clone(), attempt.window.0.clone());
            if !seen_members.insert(key) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-member",
                });
            }
            if extra {
                let diagnostic = attempt_hold_scope(map);
                return Ok(AttemptRowOutcome::Held(held_attempt_row(
                    AttemptLaneV4::Member,
                    ordinal,
                    map,
                    SCHEMA,
                    vec![attempt.operation_id, attempt.window.0],
                    diagnostic,
                )));
            }
            Ok(AttemptRowOutcome::Valid(attempt))
        }
        Err(reason) => {
            let (op, win) = attempt_identity(map, true, true);
            match (op, win) {
                (Some(op), Some(win)) => {
                    if !seen_members.insert((op.clone(), win.clone())) {
                        return Err(WorldProtocolError {
                            kind: KIND_SNAPSHOT,
                            message: "duplicate-member",
                        });
                    }
                    Ok(AttemptRowOutcome::Held(held_attempt_row(
                        AttemptLaneV4::Member,
                        ordinal,
                        map,
                        SCHEMA,
                        vec![op, win],
                        attempt_hold_scope(map),
                    )))
                }
                _ => Err(reason),
            }
        }
    }
}

/// Staged v4 view-attempt decode: strict shapes from v3, but a logical
/// (non-native-only) `pre`/`post`/`base`, any carried extra key, or any
/// other value failure holds the identifiable (`op`) row instead of
/// refusing the envelope.
fn decode_view_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_views: &mut std::collections::BTreeSet<String>,
) -> Result<AttemptRowOutcome<tiler_core::world_report::ViewAttempt>> {
    const SCHEMA: &[&str] = &[
        "op",
        "out",
        "ws",
        "setter",
        "pre",
        "post",
        "base",
        "attemptMs",
    ];
    let map = as_object(value)?;
    let extra = map.keys().any(|key| !SCHEMA.contains(&key.as_str()));
    match decode_view_attempt(value) {
        Ok(attempt) => {
            if !seen_views.insert(attempt.operation_id.clone()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-view-attempt",
                });
            }
            if extra {
                let diagnostic = attempt_hold_scope(map);
                return Ok(AttemptRowOutcome::Held(held_attempt_row(
                    AttemptLaneV4::View,
                    ordinal,
                    map,
                    SCHEMA,
                    vec![attempt.operation_id],
                    diagnostic,
                )));
            }
            Ok(AttemptRowOutcome::Valid(attempt))
        }
        Err(reason) => {
            let (op, _) = attempt_identity(map, true, false);
            match op {
                Some(op) => {
                    if !seen_views.insert(op.clone()) {
                        return Err(WorldProtocolError {
                            kind: KIND_SNAPSHOT,
                            message: "duplicate-view-attempt",
                        });
                    }
                    Ok(AttemptRowOutcome::Held(held_attempt_row(
                        AttemptLaneV4::View,
                        ordinal,
                        map,
                        SCHEMA,
                        vec![op],
                        attempt_hold_scope(map),
                    )))
                }
                None => Err(reason),
            }
        }
    }
}

/// Staged v4 clear-attempt decode: strict shapes from v3, but any carried
/// extra key or other value failure holds the identifiable (`op` + `win`)
/// row instead of refusing the envelope.
fn decode_clear_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_clears: &mut std::collections::BTreeSet<(String, String)>,
) -> Result<AttemptRowOutcome<tiler_core::world_report::ClearAttempt>> {
    const SCHEMA: &[&str] = &["op", "win", "setter", "pre", "post", "attemptMs"];
    let map = as_object(value)?;
    let extra = map.keys().any(|key| !SCHEMA.contains(&key.as_str()));
    match decode_clear_attempt(value) {
        Ok(attempt) => {
            let key = (attempt.operation_id.clone(), attempt.window.0.clone());
            if !seen_clears.insert(key) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-clear-attempt",
                });
            }
            if extra {
                return Ok(AttemptRowOutcome::Held(held_attempt_row(
                    AttemptLaneV4::Clear,
                    ordinal,
                    map,
                    SCHEMA,
                    vec![attempt.operation_id, attempt.window.0],
                    attempt_hold_scope(map),
                )));
            }
            Ok(AttemptRowOutcome::Valid(attempt))
        }
        Err(reason) => {
            let (op, win) = attempt_identity(map, true, true);
            match (op, win) {
                (Some(op), Some(win)) => {
                    if !seen_clears.insert((op.clone(), win.clone())) {
                        return Err(WorldProtocolError {
                            kind: KIND_SNAPSHOT,
                            message: "duplicate-clear-attempt",
                        });
                    }
                    Ok(AttemptRowOutcome::Held(held_attempt_row(
                        AttemptLaneV4::Clear,
                        ordinal,
                        map,
                        SCHEMA,
                        vec![op, win],
                        attempt_hold_scope(map),
                    )))
                }
                _ => Err(reason),
            }
        }
    }
}

/// Carried `order` length of one held native row, read off its retained
/// fields (a held row always carries `order`: a missing one refuses before
/// any hold). Used only for the global native-id budget.
fn held_native_order_len(held: &HeldNativeRow) -> usize {
    held.fields
        .iter()
        .find(|(key, _)| key == "order")
        .map(|(_, value)| match value {
            RawSettingValue::Array(items) => items.len(),
            _ => 0,
        })
        .unwrap_or(0)
}

/// Staged v4 creation-attempt decode: structural preflight over the readable
/// fields first (mirrors the core creation gate: bad ids, per-row caps and
/// dupes, op uniqueness, receipt reuse, aggregate budget), then the strict
/// v3 shape. A wrong-typed `created` receipt, any carried extra key, or any
/// other identifiable value failure holds the identifiable (`op`) row
/// instead of refusing the envelope; the preflight above still refuses
/// before such a hold can mask it.
fn decode_creation_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_creations: &mut std::collections::BTreeMap<String, Option<(String, String)>>,
    seen_receipts: &mut std::collections::BTreeSet<String>,
    total_creation_ids: &mut usize,
) -> Result<AttemptRowOutcome<tiler_core::world_report::NativeCreationAttempt>> {
    const SCHEMA: &[&str] = &[
        "op",
        "ws",
        "out",
        "setter",
        "before",
        "after",
        "created",
        "attemptMs",
    ];
    let map = as_object(value)?;
    let extra = map.keys().any(|key| !SCHEMA.contains(&key.as_str()));
    // Structural preflight over the readable fields (mirrors the core
    // creation gate): bad ids, per-row caps and dupes, op uniqueness,
    // receipt reuse, and the aggregate budget refuse before any narrow hold
    // can mask them. Unreadable shapes keep the legacy type vocabulary.
    let read_ids = |key: &str| -> Result<Vec<String>> {
        match map.get(key) {
            None => Ok(Vec::new()),
            Some(serde_json::Value::Array(items)) => {
                let mut ids = Vec::with_capacity(items.len());
                for item in items {
                    let id = item.as_str().ok_or_else(malformed)?;
                    if !is_opaque_id(id) {
                        return Err(WorldProtocolError {
                            kind: KIND_SNAPSHOT,
                            message: "creation-id-invalid",
                        });
                    }
                    ids.push(id.to_owned());
                }
                Ok(ids)
            }
            Some(_) => Err(malformed()),
        }
    };
    let before = read_ids("before")?;
    let after = read_ids("after")?;
    // Receipt spelling: absent/null/empty normalize to none; a bad-grammar
    // string refuses; any other JSON type is a hold cause (tolerated below,
    // never a fabricated receipt).
    let created: Option<String> = match map.get("created") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) if text.is_empty() => None,
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "creation-id-invalid",
                });
            }
            Some(text.clone())
        }
        Some(_) => None,
    };
    if before.len() > tiler_core::world_report::MAX_NATIVE_PER_OUTPUT
        || after.len() > tiler_core::world_report::MAX_NATIVE_PER_OUTPUT
    {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "creation-order-over-cap",
        });
    }
    {
        let mut seen_before = std::collections::BTreeSet::new();
        for id in &before {
            if !seen_before.insert(id.clone()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-creation-attempt",
                });
            }
        }
        let mut seen_after = std::collections::BTreeSet::new();
        for id in &after {
            if !seen_after.insert(id.clone()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-creation-attempt",
                });
            }
        }
    }
    let op = match map.get("op") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => Some(text.clone()),
        _ => None,
    };
    let triple: Option<(String, String)> = match (map.get("ws"), map.get("out")) {
        (Some(serde_json::Value::String(ws)), Some(serde_json::Value::String(out)))
            if is_opaque_id(ws) && is_opaque_id(out) =>
        {
            Some((ws.clone(), out.clone()))
        }
        _ => None,
    };
    // Op uniqueness joins every identifiable row, even one missing ws/out:
    // only fully-carried triples that differ keep the mismatch refusal.
    if let Some(op) = op.as_deref() {
        match seen_creations.insert(op.to_owned(), triple.clone()) {
            None => {}
            Some(prev) => match (prev, triple) {
                (Some(first), Some(second)) if first != second => {
                    return Err(WorldProtocolError {
                        kind: KIND_SNAPSHOT,
                        message: "creation-triple-mismatch",
                    });
                }
                _ => {
                    return Err(WorldProtocolError {
                        kind: KIND_SNAPSHOT,
                        message: "duplicate-creation-attempt",
                    });
                }
            },
        }
    }
    *total_creation_ids = total_creation_ids
        .saturating_add(before.len())
        .saturating_add(after.len());
    if let Some(receipt) = created.as_deref() {
        *total_creation_ids = total_creation_ids.saturating_add(1);
        if !seen_receipts.insert(receipt.to_owned()) {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "creation-receipt-reused",
            });
        }
    }
    if *total_creation_ids > tiler_core::world_report::MAX_WORLD_NATIVE.saturating_mul(2) {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "creation-over-cap",
        });
    }
    match decode_creation_attempt(value) {
        Ok(attempt) => {
            if extra {
                let diagnostic = attempt_hold_scope(map);
                return Ok(AttemptRowOutcome::Held(held_attempt_row(
                    AttemptLaneV4::Creation,
                    ordinal,
                    map,
                    SCHEMA,
                    vec![attempt.operation_id],
                    diagnostic,
                )));
            }
            Ok(AttemptRowOutcome::Valid(attempt))
        }
        Err(reason) => match op {
            Some(op) => Ok(AttemptRowOutcome::Held(held_attempt_row(
                AttemptLaneV4::Creation,
                ordinal,
                map,
                SCHEMA,
                vec![op],
                attempt_hold_scope(map),
            ))),
            None => Err(reason),
        },
    }
}

/// Staged v4 presentation-ingress decode: structural preflight over the
/// known fields first (missing keys, bad identities, bad geometry, bad
/// witness-object internals), then the narrow hold for a removed key, a
/// pager frame echo (`float`/`tile`), or a non-object `witness`. Only those
/// three hold causes still hold; every other value failure stays strict.
fn decode_presentation_v4(value: &serde_json::Value) -> Result<PresentationRowV4> {
    use tiler_core::world_report::MAX_PRESENTATION_ORDER;
    let map = as_object(value)?;
    const SCHEMA: &[&str] = &[
        "foreground",
        "minimized",
        "maximized",
        "fullscreen",
        "hostHidden",
        "hiddenWorkspace",
        "frame",
        "chordHeld",
        "moveWindow",
        "pagerOrder",
        "pagerFrames",
        "witness",
        "foregroundOut",
    ];
    // Section caps refuse before any narrow hold.
    if let Some(serde_json::Value::Array(items)) = map.get("pagerOrder")
        && items.len() > MAX_PRESENTATION_ORDER
    {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "presentation-order-over-cap",
        });
    }
    if let Some(serde_json::Value::Array(items)) = map.get("pagerFrames")
        && items.len() > MAX_PRESENTATION_ORDER
    {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "presentation-frames-over-cap",
        });
    }
    check_presentation_structure_v4(map, SCHEMA)?;
    let extra_key = map.keys().any(|key| !SCHEMA.contains(&key.as_str()));
    let pager_extra = match map.get("pagerFrames") {
        Some(serde_json::Value::Array(items)) => items.iter().any(|item| match item {
            serde_json::Value::Object(row) => row.keys().any(|key| key != "win" && key != "live"),
            _ => false,
        }),
        _ => false,
    };
    let witness_held = match map.get("witness") {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Object(row)) => {
            decode_frame_witness(&serde_json::Value::Object(row.clone())).is_err()
        }
        Some(_) => true,
    };
    if extra_key || pager_extra || witness_held {
        return Ok(PresentationRowV4::Held(held_presentation_row(map)));
    }
    decode_presentation_ingress_v4(value).map(PresentationRowV4::Valid)
}

/// Structural preflight for one staged v4 presentation-ingress object:
/// every schema key must be present, and every known field must carry its
/// strict shape (identities, key presence, geometry, witness-object
/// internals). Only the three narrow-hold causes are tolerated here: extra
/// top-level keys, extra pager-frame keys, and a non-object `witness` (a
/// constant string carries no inspectable fields). Dangling but well-formed
/// ids stay accepted (shape only; causal join belongs to a later unit).
fn check_presentation_structure_v4(
    map: &serde_json::Map<String, serde_json::Value>,
    schema: &[&str],
) -> Result<()> {
    for key in schema {
        if !map.contains_key(*key) {
            return Err(malformed());
        }
    }
    req_empty_or_id(map, "foreground")?;
    opt_bool_or_null(map, "minimized")?;
    opt_bool_or_null(map, "maximized")?;
    opt_bool_or_null(map, "fullscreen")?;
    opt_bool_or_null(map, "hostHidden")?;
    opt_bool_or_null(map, "hiddenWorkspace")?;
    opt_rect_or_null(map, "frame")?;
    req_bool(map, "chordHeld")?;
    req_empty_or_id(map, "moveWindow")?;
    let order_items = map
        .get("pagerOrder")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(malformed)?;
    for item in order_items {
        let id = item.as_str().ok_or_else(malformed)?;
        if !is_opaque_id(id) {
            return Err(unknown_value());
        }
    }
    let frame_items = map
        .get("pagerFrames")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(malformed)?;
    for item in frame_items {
        let row = as_object(item)?;
        req_id(row, "win")?;
        opt_rect_or_null(row, "live")?;
    }
    match map.get("witness") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::Object(row)) => {
            decode_frame_witness(&serde_json::Value::Object(row.clone()))?;
        }
        Some(_) => {}
    }
    match map.get("foregroundOut") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
        }
        Some(_) => return Err(malformed()),
    }
    Ok(())
}

/// Closed execution-time core-focus marker (`win:{"currentFocus":true}`):
/// exactly one key `currentFocus` with boolean `true`. Every other shape
/// (missing, null, string, empty object, extra keys, false/null/wrong-type
/// values) is not the marker and follows the normal by-id path (which
/// refuses null/non-string subjects exactly as before).
fn is_current_focus_marker(value: Option<&serde_json::Value>) -> bool {
    let Some(serde_json::Value::Object(map)) = value else {
        return false;
    };
    if map.len() != 1 {
        return false;
    }
    matches!(map.get("currentFocus"), Some(serde_json::Value::Bool(true)))
}

/// Decode one execution-time (`win:{"currentFocus":true}`) v4 command: the adapter names
/// the payload only. The wrapped explicit command carries a placeholder
/// empty subject that never executes; the core substitutes the retained
/// canonical focus at execution time.
fn decode_implicit_command_v4(
    map: &serde_json::Map<String, serde_json::Value>,
    op: &str,
) -> Result<WorldCommand> {
    use tiler_core::world_report::WorldCommand as C;
    let empty = WindowId(String::new());
    let wrapped = match op {
        "focus" => {
            check_keys(map, &["op", "win"])?;
            C::Focus {
                window: empty,
                direction: String::new(),
            }
        }
        "send" => {
            check_keys(map, &["op", "win", "out", "ws", "follow"])?;
            C::Send {
                window: empty,
                target_output: OutputId(req_id(map, "out")?),
                target_workspace: req_id(map, "ws")?,
                follow: req_bool(map, "follow")?,
            }
        }
        "send-relative" => {
            check_keys(map, &["op", "win", "source", "delta", "follow"])?;
            let delta = map
                .get("delta")
                .and_then(serde_json::Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(malformed)?;
            if delta != -1 && delta != 1 {
                return Err(unknown_value());
            }
            let source = decode_send_source(map, "source")?;
            C::SendRelative {
                window: empty,
                source,
                delta,
                follow: req_bool(map, "follow")?,
            }
        }
        "send-numbered" => {
            check_keys(map, &["op", "win", "source", "index", "follow"])?;
            let index = map
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .ok_or_else(malformed)?;
            if index > 9 {
                return Err(unknown_value());
            }
            let source = decode_send_source(map, "source")?;
            C::SendNumbered {
                window: empty,
                source,
                index,
                follow: req_bool(map, "follow")?,
            }
        }
        "focus-directional" => {
            check_keys(map, &["op", "win", "direction"])?;
            C::FocusDirectional {
                window: empty,
                direction: parse_direction_token(req_str(map, "direction")?)?,
            }
        }
        "move-directional" => {
            check_keys(map, &["op", "win", "direction", "sameAxis"])?;
            let same_axis =
                tiler_core::directional::SameAxisMove::parse_wire(req_str(map, "sameAxis")?)
                    .ok_or_else(unknown_value)?;
            C::MoveDirectional {
                window: empty,
                direction: parse_direction_token(req_str(map, "direction")?)?,
                same_axis,
            }
        }
        "send-directional" => {
            check_keys(map, &["op", "win", "direction", "follow"])?;
            C::SendDirectional {
                window: empty,
                direction: parse_direction_token(req_str(map, "direction")?)?,
                follow: req_bool(map, "follow")?,
            }
        }
        "toggle-maximize" => {
            check_keys(map, &["op", "win"])?;
            C::ToggleMaximize { window: empty }
        }
        "set-sticky" => {
            check_keys(map, &["op", "win", "value"])?;
            C::SetSticky {
                window: empty,
                value: req_bool(map, "value")?,
            }
        }
        "toggle-sticky" => {
            check_keys(map, &["op", "win"])?;
            C::ToggleSticky { window: empty }
        }
        "set-fullscreen" => {
            check_keys(map, &["op", "win", "value"])?;
            C::SetFullscreen {
                window: empty,
                value: req_bool(map, "value")?,
            }
        }
        "toggle-fullscreen" => {
            check_keys(map, &["op", "win"])?;
            C::ToggleFullscreen { window: empty }
        }
        "toggle-float" => {
            check_keys(map, &["op", "win", "rect"])?;
            C::Operation(CoreCommand::ToggleFloat {
                window: String::new(),
                float_rect: opt_rect_or_null(map, "rect")?,
            })
        }
        "toggle-orientation" => {
            check_keys(map, &["op", "win"])?;
            C::Operation(CoreCommand::ToggleOrientation {
                window: String::new(),
            })
        }
        "resize" => {
            check_keys(map, &["op", "win", "direction", "mode", "press"])?;
            let mode = match req_str(map, "mode")? {
                "inwards" | "outwards" | "horizontal" | "vertical" => {
                    req_str(map, "mode")?.to_owned()
                }
                _ => return Err(unknown_value()),
            };
            let press = map
                .get("press")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(malformed)?;
            C::Operation(CoreCommand::Resize {
                window: String::new(),
                direction: parse_direction_token(req_str(map, "direction")?)?,
                mode,
                press_index: press,
            })
        }
        _ => return Err(unknown_value()),
    };
    if !tiler_core::world_report::valid_implicit_command(&wrapped) {
        return Err(unknown_value());
    }
    Ok(WorldCommand::Implicit(Box::new(wrapped)))
}

/// Decode one bare v4 command value (queue-lane ingress): the same
/// `win:{"currentFocus":true}` execution-time spelling the report lane
/// accepts. Shape-only; execution resolves the subject from retained
/// canonical focus.
pub fn decode_command_value_v4(value: &serde_json::Value) -> Result<WorldCommand> {
    decode_command_v4(value)
}

/// Staged v4 command decode: every v3 direct world-command arm except the
/// removed `switch-view` alias (which refuses as an unknown value).
/// The removed `operation` wrapper refuses as well; its three promoted
/// payloads (`toggle-float`, `toggle-orientation`, `resize`) decode
/// directly into the equivalent [`CoreCommand`] operation instead.
/// A `win:{"currentFocus":true}` subject on any windowed op decodes to the
/// execution-time core-focus wrapper (resolved from retained canonical focus at
/// execution); explicit strings keep their by-id meaning, and the empty
/// `focus-directional` win keeps its genuine no-focus meaning. Missing,
/// null, or otherwise malformed subjects refuse exactly as before.
fn decode_command_v4(value: &serde_json::Value) -> Result<WorldCommand> {
    let map = as_object(value)?;
    if is_current_focus_marker(map.get("win")) {
        let op = req_str(map, "op")?;
        return decode_implicit_command_v4(map, op);
    }
    let op = req_str(map, "op")?;
    match op {
        "switch-view" | "operation" => Err(unknown_value()),
        // v4 by-id focus (R6): the ignored direction is removed from the
        // wire. The staged record spells the absent direction as the empty
        // string (absence, never a guessed direction); interpretation
        // belongs to the staged engine use. An extra `direction` key
        // refuses as an unknown field.
        "focus" => {
            check_keys(map, &["op", "win"])?;
            Ok(WorldCommand::Focus {
                window: WindowId(req_id(map, "win")?),
                direction: String::new(),
            })
        }
        "toggle-float" => {
            check_keys(map, &["op", "win", "rect"])?;
            Ok(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: req_id(map, "win")?,
                float_rect: opt_rect_or_null(map, "rect")?,
            }))
        }
        "toggle-orientation" => {
            check_keys(map, &["op", "win"])?;
            Ok(WorldCommand::Operation(CoreCommand::ToggleOrientation {
                window: req_id(map, "win")?,
            }))
        }
        "resize" => {
            check_keys(map, &["op", "win", "direction", "mode", "press"])?;
            let mode = match req_str(map, "mode")? {
                "inwards" | "outwards" | "horizontal" | "vertical" => {
                    req_str(map, "mode")?.to_owned()
                }
                _ => return Err(unknown_value()),
            };
            let press = map
                .get("press")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(malformed)?;
            Ok(WorldCommand::Operation(CoreCommand::Resize {
                window: req_id(map, "win")?,
                direction: parse_direction_token(req_str(map, "direction")?)?,
                mode,
                press_index: press,
            }))
        }
        _ => decode_command(value),
    }
}

/// Decode one JSON v4 world request into the staged core report plus its
/// optional wire metadata. Enforces byte length first (4 MiB), the v4
/// envelope keys (`rendererInput` required, `views` refused), the v4
/// contract version, per-lane row decodes with narrow holds, and the
/// structural decoder gate (section caps, id/duplicate/rect shape,
/// closed/live overlap, native binding coverage, attempt identity dupes,
/// renderer/command pairing). Never logs or echoes payload bytes. The
/// staged report is shape-only: capability-context verdicts (sticky,
/// unavailable facts, host-owned arm selection) stay valid typed rows for
/// the staged engine use.
pub fn decode_report_v4(json: &str) -> std::result::Result<DecodedReportV4, WorldProtocolError> {
    if json.len() > WORLD_MAX_REQUEST_BYTES {
        return Err(WorldProtocolError {
            kind: KIND_OVERSIZED,
            message: MSG_OVERSIZED,
        });
    }
    let value: serde_json::Value =
        crate::connection_protocol::parse_strict_json(json).map_err(|e| {
            if e.0 == "oversized" {
                WorldProtocolError {
                    kind: KIND_OVERSIZED,
                    message: MSG_OVERSIZED,
                }
            } else {
                malformed()
            }
        })?;
    let map = as_object(&value)?;
    check_keys_with_optional(
        map,
        &[
            "v",
            "adapter",
            "gen",
            "obs",
            "reqSeq",
            "req",
            "cmdSeq",
            "command",
            "screens",
            "windows",
            "closed",
            "native",
            "focused",
            "foregroundKnown",
            "complete",
            "gesture",
            "cursor",
            "actionForeground",
            "actionCached",
            "actionOutputs",
            "presentation",
            "intentRecords",
            "stateAttempts",
            "focusFeedback",
            "rendererInput",
        ],
        &[
            "cursorHit",
            "monotonicMs",
            "primary",
            "frameAttempts",
            "memberAttempts",
            "viewAttempts",
            "clearAttempts",
            "creationAttempts",
            "gestureIngress",
        ],
    )?;
    let version = map
        .get("v")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(malformed)?;
    if version != u64::from(WORLD_PROTOCOL_VERSION_V4) {
        return Err(WorldProtocolError {
            kind: KIND_VERSION,
            message: MSG_VERSION,
        });
    }
    let command_seq = opt_u64_or_null(map, "cmdSeq")?;
    let command = match map.get("command") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_command_v4(value)?),
    };
    if command_seq.is_none() != command.is_none() {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "command-pair-invalid",
        });
    }
    // No `valid_command` verdict gate here: every typed command shape is
    // enforced at decode above (unknown ops/tokens, ranges, id grammar),
    // and command verdicts belong to the staged engine use. In particular
    // the v4 direction-less focus spells absence as the empty direction,
    // which the legacy direction vocabulary would misread as invalid.
    let screens_value = map.get("screens").ok_or_else(malformed)?;
    let mut screens = Vec::new();
    let mut held_screens = Vec::new();
    for (ordinal, item) in screens_value
        .as_array()
        .ok_or_else(malformed)?
        .iter()
        .enumerate()
    {
        match decode_screen(item, ordinal)? {
            ScreenRowOutcome::Valid(screen) => screens.push(screen),
            ScreenRowOutcome::Held(held) => held_screens.push(held),
        }
    }
    if screens.len().saturating_add(held_screens.len())
        > tiler_core::world_report::MAX_WORLD_SCREENS
    {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "section-over-cap",
        });
    }
    // Shared cross-row screen gate: duplicate outputs refuse across valid
    // and held rows (rect-pair/complete shapes already gated per row).
    {
        let mut seen_outputs = std::collections::BTreeSet::new();
        for screen in &screens {
            if let Some(reason) = check_screen_structure(screen, &mut seen_outputs) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: reason,
                });
            }
        }
        for held in &held_screens {
            if !seen_outputs.insert(held.output.0.clone()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-screen",
                });
            }
        }
    }
    let windows_value = map.get("windows").ok_or_else(malformed)?;
    let mut windows = Vec::new();
    let mut held_windows = Vec::new();
    let mut seen_windows = std::collections::BTreeSet::new();
    for (ordinal, item) in windows_value
        .as_array()
        .ok_or_else(malformed)?
        .iter()
        .enumerate()
    {
        match decode_fact_v4(item, ordinal, &mut seen_windows)? {
            WindowRowOutcome::Valid(fact) => windows.push(*fact),
            WindowRowOutcome::Held(held) => held_windows.push(held),
        }
    }
    if windows.len().saturating_add(held_windows.len()) > tiler_core::world_report::MAX_WORLD_FACTS
    {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "section-over-cap",
        });
    }
    let closed_value = map.get("closed").ok_or_else(malformed)?;
    let closed = decode_closed_ids(closed_value)?;
    if closed.len() > tiler_core::world_report::MAX_WORLD_CLOSED {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "section-over-cap",
        });
    }
    {
        let mut seen_closed = std::collections::BTreeSet::new();
        for id in &closed {
            if !seen_closed.insert(id.0.clone()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-closed",
                });
            }
            // Closed ids must not also be live: one id cannot be both
            // observed (valid or held) and reported closed.
            if seen_windows.contains(&id.0) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "closed-overlaps-live",
                });
            }
        }
    }
    let native_value = map.get("native").ok_or_else(malformed)?;
    let mut native = Vec::new();
    let mut held_native = Vec::new();
    let mut seen_native_outputs = std::collections::BTreeSet::new();
    for (ordinal, item) in native_value
        .as_array()
        .ok_or_else(malformed)?
        .iter()
        .enumerate()
    {
        match decode_native_v4(item, ordinal, &mut seen_native_outputs)? {
            NativeRowOutcome::Valid(row) => native.push(row),
            NativeRowOutcome::Held(held) => held_native.push(held),
        }
    }
    if native.len().saturating_add(held_native.len()) > tiler_core::world_report::MAX_WORLD_SCREENS
    {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "section-over-cap",
        });
    }
    // Native binding gate over valid and held rows (mirrors the core
    // gate): known screen outputs (valid or held) only, exact occupancy
    // coverage, native id budget across both row kinds.
    {
        let mut known_outputs: std::collections::BTreeSet<&str> =
            screens.iter().map(|row| row.output.0.as_str()).collect();
        for held in &held_screens {
            known_outputs.insert(held.output.0.as_str());
        }
        let mut total_native = 0usize;
        for row in &native {
            if !known_outputs.contains(row.output.0.as_str()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "native-output-unknown",
                });
            }
            if row.native_order.len() != row.occupancy.len() {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "native-occupancy-incomplete",
                });
            }
            total_native += row.native_order.len();
        }
        for held in &held_native {
            if !known_outputs.contains(held.output.0.as_str()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "native-output-unknown",
                });
            }
            total_native = total_native.saturating_add(held_native_order_len(held));
        }
        if total_native > tiler_core::world_report::MAX_WORLD_NATIVE {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "native-over-cap",
            });
        }
    }
    // Tolerant monotonic clock: missing backfills 0 (unreported). A
    // present value must be a canonical decimal string; JSON numbers
    // refuse.
    let monotonic_ms = match map.get("monotonicMs") {
        None => 0,
        Some(serde_json::Value::String(text)) => {
            parse_u64_decimal(text).ok_or_else(unknown_value)?
        }
        Some(_) => return Err(unknown_value()),
    };
    let mut frames = Vec::new();
    let mut held_attempts: Vec<HeldAttemptRow> = Vec::new();
    let mut seen_frames = std::collections::BTreeSet::new();
    if let Some(frames_value) = map.get("frameAttempts") {
        let items = frames_value.as_array().ok_or_else(malformed)?;
        // Total carried rows (valid plus held) share the lane bound: the
        // gate runs before per-row decode so held rows cannot escape it.
        if items.len() > tiler_core::world_report::MAX_WORLD_FRAMES {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "section-over-cap",
            });
        }
        for (ordinal, item) in items.iter().enumerate() {
            match decode_frame_attempt_v4(item, ordinal, &mut seen_frames)? {
                AttemptRowOutcome::Valid(attempt) => frames.push(attempt),
                AttemptRowOutcome::Held(held) => held_attempts.push(held),
            }
        }
    }
    let mut member_attempts = Vec::new();
    let mut seen_members = std::collections::BTreeSet::new();
    if let Some(value) = map.get("memberAttempts") {
        let items = value.as_array().ok_or_else(malformed)?;
        if items.len() > tiler_core::world_report::MAX_WORLD_MEMBER_ATTEMPTS {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "section-over-cap",
            });
        }
        for (ordinal, item) in items.iter().enumerate() {
            match decode_member_attempt_v4(item, ordinal, &mut seen_members)? {
                AttemptRowOutcome::Valid(attempt) => member_attempts.push(attempt),
                AttemptRowOutcome::Held(held) => held_attempts.push(held),
            }
        }
    }
    let mut view_attempts = Vec::new();
    let mut seen_views = std::collections::BTreeSet::new();
    if let Some(value) = map.get("viewAttempts") {
        let items = value.as_array().ok_or_else(malformed)?;
        if items.len() > tiler_core::world_report::MAX_WORLD_VIEW_ATTEMPTS {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "section-over-cap",
            });
        }
        for (ordinal, item) in items.iter().enumerate() {
            match decode_view_attempt_v4(item, ordinal, &mut seen_views)? {
                AttemptRowOutcome::Valid(attempt) => view_attempts.push(attempt),
                AttemptRowOutcome::Held(held) => held_attempts.push(held),
            }
        }
    }
    let mut clear_attempts = Vec::new();
    let mut seen_clears = std::collections::BTreeSet::new();
    if let Some(value) = map.get("clearAttempts") {
        let items = value.as_array().ok_or_else(malformed)?;
        if items.len() > tiler_core::world_report::MAX_WORLD_CLEAR_ATTEMPTS {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "section-over-cap",
            });
        }
        for (ordinal, item) in items.iter().enumerate() {
            match decode_clear_attempt_v4(item, ordinal, &mut seen_clears)? {
                AttemptRowOutcome::Valid(attempt) => clear_attempts.push(attempt),
                AttemptRowOutcome::Held(held) => held_attempts.push(held),
            }
        }
    }
    let mut creation_attempts = Vec::new();
    let mut creation_receipts = Vec::new();
    let mut seen_creations: std::collections::BTreeMap<String, Option<(String, String)>> =
        std::collections::BTreeMap::new();
    let mut seen_receipts = std::collections::BTreeSet::new();
    let mut total_creation_ids = 0usize;
    if let Some(value) = map.get("creationAttempts") {
        let items = value.as_array().ok_or_else(malformed)?;
        if items.len() > tiler_core::world_report::MAX_WORLD_CREATION_ATTEMPTS {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "section-over-cap",
            });
        }
        for (ordinal, item) in items.iter().enumerate() {
            let spelling = match item.get("created") {
                None => CreationReceiptWire::Absent,
                Some(serde_json::Value::Null) => CreationReceiptWire::Null,
                Some(serde_json::Value::String(text)) if text.is_empty() => {
                    CreationReceiptWire::Empty
                }
                Some(_) => CreationReceiptWire::Id,
            };
            match decode_creation_attempt_v4(
                item,
                ordinal,
                &mut seen_creations,
                &mut seen_receipts,
                &mut total_creation_ids,
            )? {
                AttemptRowOutcome::Valid(attempt) => {
                    creation_attempts.push(attempt);
                    creation_receipts.push(spelling);
                }
                AttemptRowOutcome::Held(held) => held_attempts.push(held),
            }
        }
    }
    // Required native-state attempts: closed mechanical rows, no missing
    // tolerance. Unknown fields/values refuse; per-property duplicates
    // refuse below.
    let state_attempts_value = map.get("stateAttempts").ok_or_else(malformed)?;
    let mut state_attempts = Vec::new();
    let mut seen_states = std::collections::BTreeSet::new();
    for item in state_attempts_value.as_array().ok_or_else(malformed)? {
        let attempt = decode_state_attempt(item)?;
        if !seen_states.insert((attempt.window.0.clone(), attempt.property)) {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "duplicate-state-attempt",
            });
        }
        state_attempts.push(attempt);
    }
    if state_attempts.len() > tiler_core::world_report::MAX_WORLD_STATE_ATTEMPTS {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "section-over-cap",
        });
    }
    // Required correlated focus evidence: closed mechanical rows, no
    // missing tolerance. Both arms decode context-free here; the engine
    // host gate owns the native/output check.
    let focus_feedback_value = map.get("focusFeedback").ok_or_else(malformed)?;
    let mut focus_feedback = Vec::new();
    for item in focus_feedback_value.as_array().ok_or_else(malformed)? {
        focus_feedback.push(decode_focus_feedback(item)?);
    }
    if focus_feedback.len() > tiler_core::world_report::MAX_WORLD_FOCUS_FEEDBACK
        || focus_feedback
            .iter()
            .map(|row| row.arrival.len())
            .sum::<usize>()
            > tiler_core::world_report::MAX_ARRIVAL_BRACKETS_TOTAL
    {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "section-over-cap",
        });
    }
    // Tolerant gesture ingress: missing backfills observation-only
    // (`None`); a present value carries the normalized shape.
    let gesture = match map.get("gestureIngress") {
        None => None,
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_gesture_ingress(value)?),
    };
    // Required opaque host storage return: closed `{"win","data"}` rows.
    let intent_records_value = map.get("intentRecords").ok_or_else(malformed)?;
    let intent_records_items = intent_records_value.as_array().ok_or_else(malformed)?;
    if intent_records_items.len() > MAX_INTENT_RECORDS {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "section-over-cap",
        });
    }
    let mut intent_records = Vec::with_capacity(intent_records_items.len());
    for item in intent_records_items {
        let row = as_object(item)?;
        check_keys(row, &["win", "data"])?;
        let win = WindowId(req_id(row, "win")?);
        let data = match row.get("data") {
            Some(serde_json::Value::String(data)) => data.clone(),
            _ => return Err(malformed()),
        };
        if data.len() > tiler_core::intent_data::MAX_INTENT_DATA_BYTES {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "intent-data-invalid",
            });
        }
        intent_records.push(IntentRecord { win, data });
    }
    let presentation = decode_presentation_v4(map.get("presentation").ok_or_else(malformed)?)?;
    let renderer_input = match map.get("rendererInput") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_renderer_input(value)?),
    };
    // Renderer/command pairing: a carried renderer input requires a carried
    // command (one direction only; observation-only commands without a
    // renderer stay valid).
    if renderer_input.is_some() && command.is_none() {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "command-pair-invalid",
        });
    }
    let report = WorldReportV4 {
        contract_version: WORLD_PROTOCOL_VERSION_V4,
        adapter_incarnation: req_id(map, "adapter")?,
        core_generation: req_bounded(map, "gen", MAX_GENERATION_LEN, is_generation_text)?,
        observation_seq: req_u64_str(map, "obs")?,
        request_seq: req_u64_str(map, "reqSeq")?,
        request_id: req_bounded(map, "req", MAX_CORRELATION_LEN, is_correlation_text)?,
        command_seq,
        command,
        screens,
        held_screens,
        windows,
        held_windows,
        closed,
        native,
        held_native,
        cursor: opt_point_or_null(map, "cursor")?,
        cursor_hit: match map.get("cursorHit") {
            None => None,
            Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(text)) if text.is_empty() => None,
            Some(serde_json::Value::String(text)) => {
                if !is_opaque_id(text) {
                    return Err(unknown_value());
                }
                Some(WindowId(text.clone()))
            }
            Some(_) => return Err(unknown_value()),
        },
        action_foreground: match map.get("actionForeground") {
            Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(text)) => {
                if !is_opaque_id(text) {
                    return Err(unknown_value());
                }
                Some(OutputId(text.clone()))
            }
            _ => return Err(malformed()),
        },
        action_cached: match map.get("actionCached") {
            Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(text)) => {
                if !is_opaque_id(text) {
                    return Err(unknown_value());
                }
                Some(OutputId(text.clone()))
            }
            _ => return Err(malformed()),
        },
        action_outputs: match map.get("actionOutputs") {
            Some(serde_json::Value::Array(items)) => {
                let mut outputs = Vec::with_capacity(items.len());
                for item in items {
                    match item {
                        serde_json::Value::String(text) => {
                            if !is_opaque_id(text) {
                                return Err(unknown_value());
                            }
                            outputs.push(OutputId(text.clone()));
                        }
                        _ => return Err(malformed()),
                    }
                }
                outputs
            }
            _ => return Err(malformed()),
        },
        focused_global: {
            let text = req_str(map, "focused")?;
            if !text.is_empty() && !is_opaque_id(text) {
                return Err(unknown_value());
            }
            text.to_owned()
        },
        foreground_known: req_bool(map, "foregroundKnown")?,
        inventory_complete: req_bool(map, "complete")?,
        gesture_barrier: req_bool(map, "gesture")?,
        gesture,
        monotonic_ms,
        frames,
        member_attempts,
        view_attempts,
        clear_attempts,
        creation_attempts,
        held_attempts,
        presentation,
        intent_records,
        state_attempts,
        focus_feedback,
        primary_output: match map.get("primary") {
            None => None,
            Some(serde_json::Value::Null) => return Err(malformed()),
            Some(serde_json::Value::String(text)) => {
                if !is_opaque_id(text) {
                    return Err(unknown_value());
                }
                Some(OutputId(text.clone()))
            }
            Some(_) => return Err(malformed()),
        },
        renderer_input,
    };
    // Common cross-checks (mirror the core gate): the cursor hit needs the
    // cursor position and a reported valid window (held rows are not
    // reported facts), and action outputs stay unique.
    if let Some(hit) = report.cursor_hit.as_ref() {
        if report.cursor.is_none() {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "cursor-hit-without-cursor",
            });
        }
        if !report.windows.iter().any(|fact| fact.window == *hit) {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "cursor-hit-unknown",
            });
        }
    }
    {
        let mut seen_action_outputs = std::collections::BTreeSet::new();
        for output in &report.action_outputs {
            if !seen_action_outputs.insert(output.0.clone()) {
                return Err(WorldProtocolError {
                    kind: KIND_SNAPSHOT,
                    message: "duplicate-action-output",
                });
            }
        }
    }
    if !report.foreground_known && !report.focused_global.is_empty() {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "focused-unknown-mismatch",
        });
    }
    let wire = ReportWireOptionals {
        cursor_hit: match map.get("cursorHit") {
            None => CursorHitWire::Absent,
            Some(serde_json::Value::Null) => CursorHitWire::Null,
            Some(serde_json::Value::String(text)) if text.is_empty() => CursorHitWire::Empty,
            Some(serde_json::Value::String(_)) => CursorHitWire::Id,
            Some(_) => return Err(unknown_value()),
        },
        monotonic_ms_present: map.contains_key("monotonicMs"),
        frame_attempts_present: map.contains_key("frameAttempts"),
        member_attempts_present: map.contains_key("memberAttempts"),
        view_attempts_present: map.contains_key("viewAttempts"),
        clear_attempts_present: map.contains_key("clearAttempts"),
        creation_attempts_present: map.contains_key("creationAttempts"),
        gesture_ingress: match map.get("gestureIngress") {
            None => GestureIngressWire::Absent,
            Some(serde_json::Value::Null) => GestureIngressWire::Null,
            Some(_) => GestureIngressWire::Present,
        },
    };
    Ok(DecodedReportV4 {
        report,
        wire,
        creation_receipts,
    })
}

/// Staged v4 connection-context gate for decoder composition: calls the
/// context-free [`decode_report_v4`] shape decoder, then applies the same
/// arrival-arm rule the runtime owns (native arm only on host-owned hosts,
/// observed-output arm only elsewhere). The caller passes the bound
/// handshake capability; the context is never inferred from the report.
/// Both arms stay shape-valid in [`decode_report_v4`]; a wrong-host arm
/// here refuses as `snapshot-invalid`/`arrival-host-mismatch` with no new
/// error catalog or API metadata.
pub fn decode_report_v4_for_host(
    json: &str,
    host_owned: bool,
) -> std::result::Result<DecodedReportV4, WorldProtocolError> {
    let decoded = decode_report_v4(json)?;
    let mismatch = decoded.report.focus_feedback.iter().any(|row| {
        row.arrival
            .iter()
            .any(|bracket| bracket.location.is_native() != host_owned)
    });
    if mismatch {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "arrival-host-mismatch",
        });
    }
    Ok(decoded)
}

// -- Staged whole-reply v4 decoder --
// Context-free strict decode of one v4 world reply plus the declared-host
// capability gate. Reuses the existing row decoders/helpers
// (`decode_authority_scope`, `decode_frame_witness`,
// `decode_border_presentation_v4`, `decode_pager_frame_v4`,
// `decode_render_target`, `check_keys`/`check_keys_with_optional`,
// `opt_u64_or_null`, `opt_point_or_null`) where their grammar matches the
// reply contract. Replies carry no held-row concept: any invalid row
// refuses the whole message. Envelope and section key order follows the
// contract table; typed fields carry only what the wire carries.

fn reply_snapshot(message: &'static str) -> WorldProtocolError {
    WorldProtocolError {
        kind: KIND_SNAPSHOT,
        message,
    }
}

fn parse_hold_reason(text: &str) -> Option<HoldReason> {
    Some(match text {
        "unreadable" => HoldReason::Unreadable,
        "incomplete-inventory" => HoldReason::IncompleteInventory,
        "gesture-barrier" => HoldReason::GestureBarrier,
        "refused-target" => HoldReason::RefusedTarget,
        "ambiguous-creation" => HoldReason::AmbiguousCreation,
        "occupied-removal" => HoldReason::OccupiedRemoval,
        "constraint-clamp" => HoldReason::ConstraintClamp,
        "minimized" => HoldReason::Minimized,
        _ => return None,
    })
}

fn decode_authority_pair(
    map: &serde_json::Map<String, serde_json::Value>,
    auth_key: &str,
    reason_key: &str,
) -> Result<ScopeAuthority> {
    let auth = req_str(map, auth_key)?;
    let reason = req_str(map, reason_key)?;
    match auth {
        "controlled" | "untouched" => {
            if !reason.is_empty() {
                return Err(reply_snapshot("nonhold-reason"));
            }
            Ok(if auth == "controlled" {
                ScopeAuthority::Controlled
            } else {
                ScopeAuthority::Untouched
            })
        }
        "hold" => match parse_hold_reason(reason) {
            Some(hold) => Ok(ScopeAuthority::Hold(hold)),
            None => Err(unknown_value()),
        },
        _ => Err(unknown_value()),
    }
}

fn decode_hold_or_empty(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<HoldReason>> {
    let text = req_str(map, key)?;
    if text.is_empty() {
        return Ok(None);
    }
    parse_hold_reason(text).map(Some).ok_or_else(unknown_value)
}

fn decode_tiling_mode(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<TilingMode> {
    match req_str(map, key)? {
        "tiled" => Ok(TilingMode::Tiled),
        "floating" => Ok(TilingMode::Floating),
        _ => Err(unknown_value()),
    }
}

fn decode_scope_status(value: &serde_json::Value) -> Result<ScopeStatus> {
    let map = as_object(value)?;
    check_keys(map, &["scope", "auth", "reason"])?;
    Ok(ScopeStatus {
        scope: decode_authority_scope(map.get("scope").ok_or_else(malformed)?)?,
        authority: decode_authority_pair(map, "auth", "reason")?,
    })
}

fn decode_workspace_desire(value: &serde_json::Value) -> Result<WorkspaceDesire> {
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &[
            "id", "out", "bind", "scope", "order", "mode", "exist", "hold",
        ],
        &["trailing"],
    )?;
    if map
        .get("trailing")
        .is_some_and(|trailing| trailing != &serde_json::Value::Bool(true))
    {
        return Err(reply_snapshot("workspace-trailing-false"));
    }
    let order = map
        .get("order")
        .and_then(serde_json::Value::as_u64)
        .and_then(|order| usize::try_from(order).ok())
        .ok_or_else(malformed)?;
    Ok(WorkspaceDesire {
        id: req_id(map, "id")?,
        output: OutputId(req_id(map, "out")?),
        native_binding: {
            let bind = req_str(map, "bind")?;
            if bind.is_empty() {
                None
            } else if is_opaque_id(bind) {
                Some(bind.to_owned())
            } else {
                return Err(unknown_value());
            }
        },
        scope: decode_authority_scope(map.get("scope").ok_or_else(malformed)?)?,
        order,
        tiling_mode: decode_tiling_mode(map, "mode")?,
        existence: req_bool(map, "exist")?,
        hold: decode_hold_or_empty(map, "hold")?,
        trailing: map.contains_key("trailing"),
    })
}

fn decode_output_desire(value: &serde_json::Value) -> Result<OutputDesire> {
    let map = as_object(value)?;
    check_keys(map, &["out", "view", "mode", "hold"])?;
    let view = req_str(map, "view")?;
    Ok(OutputDesire {
        output: OutputId(req_id(map, "out")?),
        logical_view: if view.is_empty() {
            None
        } else if is_opaque_id(view) {
            Some(view.to_owned())
        } else {
            return Err(unknown_value());
        },
        tiling_mode: decode_tiling_mode(map, "mode")?,
        hold: decode_hold_or_empty(map, "hold")?,
    })
}

fn decode_visibility_target(value: &serde_json::Value) -> Result<VisibilityTarget> {
    let map = as_object(value)?;
    check_keys(map, &["shown", "rev", "fallback"])?;
    let shown = req_bool(map, "shown")?;
    let fallback = req_bool(map, "fallback")?;
    if shown && fallback {
        return Err(reply_snapshot("visibility-show-fallback"));
    }
    Ok(VisibilityTarget {
        shown,
        revision: req_u64_str(map, "rev")?,
        fallback,
    })
}

fn decode_state_target(value: &serde_json::Value) -> Result<StateTarget> {
    let map = as_object(value)?;
    check_keys(map, &["property", "value", "auth", "reason", "cmdSeq"])?;
    let property = match req_str(map, "property")? {
        "fullscreen" => StateProperty::Fullscreen,
        "maximized" => StateProperty::Maximized,
        "sticky" => StateProperty::Sticky,
        "above" => StateProperty::Above,
        _ => return Err(unknown_value()),
    };
    Ok(StateTarget {
        property,
        value: req_bool(map, "value")?,
        authority: decode_authority_pair(map, "auth", "reason")?,
        command_seq: opt_u64_or_null(map, "cmdSeq")?,
    })
}

fn decode_window_desire(value: &serde_json::Value) -> Result<WindowDesire> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "win",
            "out",
            "ws",
            "alloc",
            "frame",
            "frame_auth",
            "frame_hold",
            "member_auth",
            "member_hold",
            "frame_op",
            "floating",
            "float_origin",
            "clear_op",
            "cmdSeq",
            "visibility",
            "visibility_auth",
            "visibility_hold",
            "state",
        ],
    )?;
    let frame_operation = {
        let text = req_str(map, "frame_op")?;
        if text.is_empty() {
            None
        } else if is_opaque_id(text) {
            Some(text.to_owned())
        } else {
            return Err(unknown_value());
        }
    };
    let clear_operation = {
        let text = req_str(map, "clear_op")?;
        if text.is_empty() {
            None
        } else if is_opaque_id(text) {
            Some(text.to_owned())
        } else {
            return Err(unknown_value());
        }
    };
    let float_origin = match req_str(map, "float_origin")? {
        "" => None,
        "capacity" => Some(FloatOrigin::Capacity),
        _ => return Err(unknown_value()),
    };
    let visibility = match map.get("visibility") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_visibility_target(value)?),
    };
    let state_value = map.get("state").ok_or_else(malformed)?;
    let state_items = state_value.as_array().ok_or_else(malformed)?;
    if state_items.len() > MAX_STATE_TARGETS_PER_WINDOW {
        return Err(reply_snapshot("state-target-count"));
    }
    let mut state = Vec::with_capacity(state_items.len());
    let mut seen = std::collections::BTreeSet::new();
    for item in state_items {
        let target = decode_state_target(item)?;
        if !seen.insert(target.property) {
            return Err(reply_snapshot("state-target-count"));
        }
        state.push(target);
    }
    Ok(WindowDesire {
        lifetime: WindowId(req_id(map, "win")?),
        output: OutputId(req_id(map, "out")?),
        workspace: req_id(map, "ws")?,
        allocation: opt_rect_or_null(map, "alloc")?,
        effective_frame: opt_rect_or_null(map, "frame")?,
        frame_authority: decode_authority_pair(map, "frame_auth", "frame_hold")?,
        membership_authority: decode_authority_pair(map, "member_auth", "member_hold")?,
        floating: req_bool(map, "floating")?,
        float_origin,
        frame_operation,
        clear_operation,
        command_seq: opt_u64_or_null(map, "cmdSeq")?,
        visibility,
        visibility_authority: decode_authority_pair(map, "visibility_auth", "visibility_hold")?,
        state,
    })
}

fn decode_transfer_v4(value: &serde_json::Value) -> Result<TransferTargetV4> {
    let map = as_object(value)?;
    check_keys(
        map,
        &["op", "kind", "win", "out", "ws", "follow", "held", "cmdSeq"],
    )?;
    let kind = match req_str(map, "kind")? {
        "window" => {
            let win = req_str(map, "win")?;
            if !is_opaque_id(win) {
                return Err(unknown_value());
            }
            TransferKind::Window(WindowId(win.to_owned()))
        }
        "view" => {
            if !req_str(map, "win")?.is_empty() {
                return Err(unknown_value());
            }
            TransferKind::View
        }
        "migration" => {
            if !req_str(map, "win")?.is_empty() {
                return Err(unknown_value());
            }
            TransferKind::Migration
        }
        _ => return Err(unknown_value()),
    };
    Ok(TransferTargetV4 {
        operation_id: req_id(map, "op")?,
        kind,
        target_output: OutputId(req_id(map, "out")?),
        target_workspace: req_id(map, "ws")?,
        follow: req_bool(map, "follow")?,
        held: req_bool(map, "held")?,
        command_seq: opt_u64_or_null(map, "cmdSeq")?,
    })
}

fn decode_effect(value: &serde_json::Value) -> Result<OneShotEffect> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "id",
            "kind",
            "target",
            "rev",
            "cause",
            "cmdSeq",
            "awaitShown",
        ],
    )?;
    let kind = match req_str(map, "kind")? {
        "focus" => EffectKind::Focus,
        "raise" => EffectKind::Raise,
        "stack" => EffectKind::Stack,
        _ => return Err(unknown_value()),
    };
    let cause = match req_str(map, "cause")? {
        "explicit-command" => tiler_core::world_report::EffectCause::ExplicitCommand,
        "automatic-admission" => tiler_core::world_report::EffectCause::AutomaticAdmission,
        "focus-follows-cursor" => tiler_core::world_report::EffectCause::FocusFollowsCursor,
        _ => return Err(unknown_value()),
    };
    let command_seq = opt_u64_or_null(map, "cmdSeq")?;
    let await_shown = req_bool(map, "awaitShown")?;
    let explicit = cause == tiler_core::world_report::EffectCause::ExplicitCommand;
    if explicit && command_seq.is_none() {
        return Err(reply_snapshot("effect-explicit-null-seq"));
    }
    if !explicit && command_seq.is_some() {
        return Err(reply_snapshot("effect-non-explicit-seq"));
    }
    if !explicit && await_shown {
        return Err(reply_snapshot("effect-automatic-await-shown"));
    }
    Ok(OneShotEffect {
        id: req_id(map, "id")?,
        kind,
        target: WindowId(req_id(map, "target")?),
        issued_revision: req_u64_str(map, "rev")?,
        cause,
        command_seq,
        await_shown,
    })
}

fn decode_close_focus(value: &serde_json::Value) -> Result<Option<CloseFocusRequest>> {
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Object(map) => {
            check_keys(map, &["id", "target", "rev", "cause", "out", "ws"])?;
            let cause = CloseFocusCause::parse(req_str(map, "cause")?).ok_or_else(unknown_value)?;
            Ok(Some(CloseFocusRequest {
                id: req_id(map, "id")?,
                target: WindowId(req_id(map, "target")?),
                issued_revision: req_u64_str(map, "rev")?,
                cause,
                output: OutputId(req_id(map, "out")?),
                workspace: req_id(map, "ws")?,
            }))
        }
        _ => Err(malformed()),
    }
}

fn decode_domain_key(
    map: &serde_json::Map<String, serde_json::Value>,
    out_key: &str,
    ws_key: &str,
) -> Result<DomainKey> {
    Ok(DomainKey {
        output: OutputId(req_id(map, out_key)?),
        workspace: WorkspaceId(req_id(map, ws_key)?),
    })
}

fn decode_drag_prior(value: &serde_json::Value) -> Result<PriorGroupEdge> {
    let map = as_object(value)?;
    check_keys(map, &["group", "edge"])?;
    let edge = match req_str(map, "edge")? {
        "left" => DragSide::Left,
        "right" => DragSide::Right,
        "top" => DragSide::Top,
        "bottom" => DragSide::Bottom,
        "center" => DragSide::Center,
        _ => return Err(unknown_value()),
    };
    Ok(PriorGroupEdge {
        group: NodeId(req_id(map, "group")?),
        edge,
    })
}

fn decode_drag_hover(value: &serde_json::Value) -> Result<DragHoverPrior> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "domainOut",
            "domainWs",
            "leaf",
            "win",
            "rev",
            "prior",
            "priorOut",
            "priorWs",
        ],
    )?;
    Ok(DragHoverPrior {
        domain: decode_domain_key(map, "domainOut", "domainWs")?,
        source_leaf: NodeId(req_id(map, "leaf")?),
        source_window: WindowId(req_id(map, "win")?),
        revision: req_u64_str(map, "rev")?,
        prior: match map.get("prior") {
            None => return Err(malformed()),
            Some(serde_json::Value::Null) => None,
            Some(value) => Some(decode_drag_prior(value)?),
        },
        prior_domain: decode_domain_key(map, "priorOut", "priorWs")?,
    })
}

fn decode_direction_token(text: &str) -> Result<Direction> {
    match text {
        "left" => Ok(Direction::Left),
        "right" => Ok(Direction::Right),
        "up" => Ok(Direction::Up),
        "down" => Ok(Direction::Down),
        _ => Err(unknown_value()),
    }
}

fn decode_directional_outcome(text: &str) -> Result<DirectionalOutcome> {
    match text {
        "focus-refused-floating" => Ok(DirectionalOutcome::FocusRefusedFloating),
        "move-refused-floating" => Ok(DirectionalOutcome::MoveRefusedFloating),
        "focus-refused-workspace-floating" => Ok(DirectionalOutcome::FocusRefusedWorkspaceFloating),
        "move-refused-workspace-floating" => Ok(DirectionalOutcome::MoveRefusedWorkspaceFloating),
        "move-refused-fullscreen" => Ok(DirectionalOutcome::MoveRefusedFullscreen),
        "focus-refused-maximize" => Ok(DirectionalOutcome::FocusRefusedMaximize),
        "focus-float-retained" => Ok(DirectionalOutcome::FocusFloatRetained),
        "focus-unfocused-retained" => Ok(DirectionalOutcome::FocusUnfocusedRetained),
        "focus-float-refused-stale" => Ok(DirectionalOutcome::FocusFloatRefusedStale),
        "move-float-refused-stale" => Ok(DirectionalOutcome::MoveFloatRefusedStale),
        "move-float-refused-invalid-geometry" => {
            Ok(DirectionalOutcome::MoveFloatRefusedInvalidGeometry)
        }
        "move-float-refused-constraints" => Ok(DirectionalOutcome::MoveFloatRefusedConstraints),
        "move-float-unknown-output" => Ok(DirectionalOutcome::MoveFloatRefusedUnknownOutput),
        "focus-float-applied" => Ok(DirectionalOutcome::FocusFloatApplied),
        "move-float-applied" => Ok(DirectionalOutcome::MoveFloatApplied),
        "move-maximize-clear" => Ok(DirectionalOutcome::NeedClear),
        _ => Err(unknown_value()),
    }
}

fn decode_command_result(value: &serde_json::Value) -> Result<Option<CommandResult>> {
    if value.is_null() {
        return Ok(None);
    }
    let map = as_object(value)?;
    let op = req_str(map, "op")?;
    match op {
        "drag-preview" => {
            check_keys(
                map,
                &[
                    "op",
                    "rev",
                    "domainOut",
                    "domainWs",
                    "sourceOut",
                    "sourceWs",
                    "rect",
                    "hover",
                    "win",
                ],
            )?;
            Ok(Some(CommandResult::DragPreview {
                rect: req_rect_value(map.get("rect").ok_or_else(malformed)?)?,
                hover: match map.get("hover") {
                    None => return Err(malformed()),
                    Some(serde_json::Value::Null) => None,
                    Some(value) => Some(decode_drag_hover(value)?),
                },
                domain: decode_domain_key(map, "domainOut", "domainWs")?,
                source: decode_domain_key(map, "sourceOut", "sourceWs")?,
                target: WindowId(req_id(map, "win")?),
                revision: req_u64_str(map, "rev")?,
            }))
        }
        "toggle-float" => {
            check_keys_with_optional(map, &["op", "win", "rect"], &["noSpace"])?;
            let no_space = match map.get("noSpace") {
                None => false,
                Some(serde_json::Value::Bool(true)) => true,
                Some(serde_json::Value::Bool(false)) => {
                    return Err(reply_snapshot("result-float-false-marker"));
                }
                Some(_) => return Err(malformed()),
            };
            Ok(Some(CommandResult::Float {
                window: WindowId(req_id(map, "win")?),
                rect: opt_rect_or_null(map, "rect")?,
                no_space,
            }))
        }
        "migrate-workspace" => {
            check_keys(
                map,
                &[
                    "op",
                    "direction",
                    "sourceOut",
                    "sourceWs",
                    "targetOut",
                    "targetWs",
                    "members",
                    "floats",
                ],
            )?;
            let members_value = map.get("members").ok_or_else(malformed)?;
            let member_items = members_value.as_array().ok_or_else(malformed)?;
            if member_items.len() > MAX_REPLY_WINDOWS {
                return Err(reply_snapshot("section-over-cap"));
            }
            let mut members = Vec::with_capacity(member_items.len());
            for item in member_items {
                let id = item.as_str().ok_or_else(malformed)?;
                if !is_opaque_id(id) {
                    return Err(unknown_value());
                }
                members.push(WindowId(id.to_owned()));
            }
            let floats = map
                .get("floats")
                .and_then(serde_json::Value::as_u64)
                .and_then(|floats| usize::try_from(floats).ok())
                .ok_or_else(malformed)?;
            Ok(Some(CommandResult::Migration {
                direction: decode_direction_token(req_str(map, "direction")?)?,
                source: decode_domain_key(map, "sourceOut", "sourceWs")?,
                target: decode_domain_key(map, "targetOut", "targetWs")?,
                members,
                floats,
            }))
        }
        "directional" => {
            check_keys(map, &["op", "outcome", "win", "rect", "cause"])?;
            let target = match map.get("win") {
                None => return Err(malformed()),
                Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(text)) => {
                    if !is_opaque_id(text) {
                        return Err(unknown_value());
                    }
                    Some(WindowId(text.clone()))
                }
                Some(_) => return Err(malformed()),
            };
            let cause = match map.get("cause") {
                None => return Err(malformed()),
                Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(text)) => match text.as_str() {
                    "no-target" | "unknown-window" | "focus-present" | "no-source"
                    | "no-adjacent-output" => Some(text.clone()),
                    _ => return Err(unknown_value()),
                },
                Some(_) => return Err(malformed()),
            };
            Ok(Some(CommandResult::Directional {
                outcome: decode_directional_outcome(req_str(map, "outcome")?)?,
                target,
                rect: opt_rect_or_null(map, "rect")?,
                cause,
            }))
        }
        "maximize" => {
            check_keys(
                map,
                &["op", "win", "maximized", "clearFullscreen", "wasMaximized"],
            )?;
            Ok(Some(CommandResult::Maximize {
                window: WindowId(req_id(map, "win")?),
                maximized: req_bool(map, "maximized")?,
                clear_fullscreen: req_bool(map, "clearFullscreen")?,
                was_maximized: req_bool(map, "wasMaximized")?,
            }))
        }
        "rejected" => {
            check_keys(map, &["op", "kind"])?;
            match CommandRejectKind::parse(req_str(map, "kind")?) {
                Some(kind) => Ok(Some(CommandResult::Rejected { kind })),
                None => Err(unknown_value()),
            }
        }
        _ => Err(unknown_value()),
    }
}

fn decode_gesture_settled(value: &serde_json::Value) -> Result<Option<GestureSettled>> {
    if value.is_null() {
        return Ok(None);
    }
    let map = as_object(value)?;
    check_keys(map, &["id", "committed"])?;
    Ok(Some(GestureSettled {
        id: req_u64_str(map, "id")?,
        committed: req_bool(map, "committed")?,
    }))
}

fn decode_operation_outcome(text: &str) -> Result<OperationOutcome> {
    match text {
        "continued-tiled" => Ok(OperationOutcome::ClearContinuedTiled),
        "continued-float" => Ok(OperationOutcome::ClearContinuedFloat),
        "refused-proven" => Ok(OperationOutcome::ClearRefusedProven),
        "abandoned-external-clear" => Ok(OperationOutcome::ClearAbandonedExternalClear),
        "abandoned-focus-changed" => Ok(OperationOutcome::ClearAbandonedFocusChanged),
        "abandoned-view-changed" => Ok(OperationOutcome::ClearAbandonedViewChanged),
        "abandoned-homing-changed" => Ok(OperationOutcome::ClearAbandonedHomingChanged),
        "abandoned-closed" => Ok(OperationOutcome::ClearAbandonedClosed),
        "abandoned-fullscreen" => Ok(OperationOutcome::ClearAbandonedFullscreen),
        "superseded" => Ok(OperationOutcome::ClearSuperseded),
        "continuation-refused" => Ok(OperationOutcome::ClearContinuationRefused),
        "float-frame-proven" => Ok(OperationOutcome::FloatFrameProven),
        "float-refused-proven" => Ok(OperationOutcome::FloatRefusedProven),
        "float-abandoned" => Ok(OperationOutcome::FloatAbandoned),
        _ => Err(unknown_value()),
    }
}

fn decode_operation_cause(text: &str) -> Result<Option<String>> {
    if text.is_empty() {
        return Ok(None);
    }
    if CommandRejectKind::parse(text).is_some() {
        return Ok(Some(text.to_owned()));
    }
    match text {
        "baseline-non-subject"
        | "closed"
        | "constraints"
        | "early-verdict"
        | "external"
        | "fixed"
        | "floating"
        | "homing-loss"
        | "invalid-direction"
        | "invalid-geometry"
        | "non-subject-focus"
        | "refused-with-unmaximized"
        | "sticky"
        | "tiling-changed"
        | "topology-unreadable"
        | "unknown-output"
        | "unknown-window" => Ok(Some(text.to_owned())),
        _ => Err(unknown_value()),
    }
}

fn decode_operation_event(value: &serde_json::Value) -> Result<OperationEvent> {
    let map = as_object(value)?;
    check_keys(map, &["op", "win", "outcome", "cause"])?;
    Ok(OperationEvent {
        operation_id: req_id(map, "op")?,
        window: WindowId(req_id(map, "win")?),
        outcome: decode_operation_outcome(req_str(map, "outcome")?)?,
        cause: decode_operation_cause(req_str(map, "cause")?)?,
    })
}

fn decode_native_creation(value: &serde_json::Value) -> Result<NativeCreationDesire> {
    let map = as_object(value)?;
    check_keys(map, &["op", "ws", "out"])?;
    Ok(NativeCreationDesire {
        operation_id: req_id(map, "op")?,
        logical_workspace: req_id(map, "ws")?,
        target_output: OutputId(req_id(map, "out")?),
    })
}

fn decode_intent_write(value: &serde_json::Value) -> Result<IntentWrite> {
    let map = as_object(value)?;
    check_keys(map, &["win", "data"])?;
    let data = match map.get("data") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => {
            if text.len() > MAX_INTENT_DATA_BYTES {
                return Err(reply_snapshot("intent-data-invalid"));
            }
            Some(text.clone())
        }
        Some(_) => return Err(malformed()),
    };
    Ok(IntentWrite {
        win: WindowId(req_id(map, "win")?),
        data,
    })
}

fn decode_switch_event(value: &serde_json::Value) -> Result<SwitchEvent> {
    let map = as_object(value)?;
    check_keys(map, &["op", "outcome", "failed", "focus", "fallback"])?;
    let outcome = match req_str(map, "outcome")? {
        "complete" => SwitchOutcome::Complete,
        "completed-with-failures" => SwitchOutcome::CompletedWithFailures,
        _ => return Err(unknown_value()),
    };
    let failed_value = map.get("failed").ok_or_else(malformed)?;
    let failed_items = failed_value.as_array().ok_or_else(malformed)?;
    if failed_items.len() > MAX_REPLY_WINDOWS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut failed = Vec::with_capacity(failed_items.len());
    for item in failed_items {
        let id = item.as_str().ok_or_else(malformed)?;
        if !is_opaque_id(id) {
            return Err(unknown_value());
        }
        failed.push(WindowId(id.to_owned()));
    }
    let focus = match req_str(map, "focus")? {
        "proven" => SwitchFocus::Proven,
        "refused" => SwitchFocus::Refused,
        "unproven" => SwitchFocus::Unproven,
        _ => return Err(unknown_value()),
    };
    Ok(SwitchEvent {
        operation_id: req_id(map, "op")?,
        outcome,
        failed,
        focus,
        fallback: req_bool(map, "fallback")?,
    })
}

fn decode_row_diagnostic(value: &serde_json::Value) -> Result<RowDiagnostic> {
    let map = as_object(value)?;
    check_keys(map, &["scope", "row", "id", "kind"])?;
    let row = match req_str(map, "row")? {
        "screen" => RowDiagnosticLane::Screen,
        "window" => RowDiagnosticLane::Window,
        "native" => RowDiagnosticLane::Native,
        "attempt" => RowDiagnosticLane::Attempt,
        "presentation" => RowDiagnosticLane::Presentation,
        _ => return Err(unknown_value()),
    };
    let id = match map.get("id") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            Some(text.clone())
        }
        Some(_) => return Err(malformed()),
    };
    let kind = match req_str(map, "kind")? {
        "unreadable" => RowDiagnosticKind::Unreadable,
        "invalid-fact" => RowDiagnosticKind::InvalidFact,
        "conflicting-fact" => RowDiagnosticKind::ConflictingFact,
        _ => return Err(unknown_value()),
    };
    Ok(RowDiagnostic {
        scope: decode_authority_scope(map.get("scope").ok_or_else(malformed)?)?,
        row,
        id,
        kind,
    })
}

fn decode_group_hide_reason(text: &str) -> Result<GroupHideReason> {
    match text {
        "disabled" => Ok(GroupHideReason::Disabled),
        "minimized" => Ok(GroupHideReason::Minimized),
        "maximized" => Ok(GroupHideReason::Maximized),
        "fullscreen" => Ok(GroupHideReason::Fullscreen),
        "floating" => Ok(GroupHideReason::Floating),
        "pending" => Ok(GroupHideReason::Pending),
        "diverged" => Ok(GroupHideReason::Diverged),
        "root-leaf" => Ok(GroupHideReason::RootLeaf),
        "focus-changed" => Ok(GroupHideReason::FocusChanged),
        "no-group" => Ok(GroupHideReason::NoGroup),
        "idle" => Ok(GroupHideReason::Idle),
        "suspended" => Ok(GroupHideReason::Suspended),
        _ => Err(unknown_value()),
    }
}

fn decode_underlay_member(value: &serde_json::Value) -> Result<ActiveGroupMember> {
    let map = as_object(value)?;
    check_keys(map, &["leaf", "win", "rect"])?;
    Ok(ActiveGroupMember {
        leaf: NodeId(req_id(map, "leaf")?),
        window: WindowId(req_id(map, "win")?),
        rect: req_rect_value(map.get("rect").ok_or_else(malformed)?)?,
    })
}

fn decode_presentation_underlay(value: &serde_json::Value) -> Result<PresentationUnderlay> {
    let map = as_object(value)?;
    match req_str(map, "status")? {
        "found" => {
            check_keys(
                map,
                &[
                    "status", "reason", "baseRev", "homeOut", "homeWs", "group", "leaf", "win",
                    "union", "outer", "members",
                ],
            )?;
            if !req_str(map, "reason")?.is_empty() {
                return Err(unknown_value());
            }
            let members_value = map.get("members").ok_or_else(malformed)?;
            let member_items = members_value.as_array().ok_or_else(malformed)?;
            if member_items.len() > MAX_REPLY_WINDOWS {
                return Err(reply_snapshot("section-over-cap"));
            }
            let mut members = Vec::with_capacity(member_items.len());
            for item in member_items {
                members.push(decode_underlay_member(item)?);
            }
            Ok(PresentationUnderlay::Found(
                tiler_core::world_presentation::PresentationUnderlayFound {
                    base_revision: req_u64_str(map, "baseRev")?,
                    home: decode_domain_key(map, "homeOut", "homeWs")?,
                    group: NodeId(req_id(map, "group")?),
                    focused_leaf: NodeId(req_id(map, "leaf")?),
                    focused_window: WindowId(req_id(map, "win")?),
                    members,
                    union_rect: req_rect_value(map.get("union").ok_or_else(malformed)?)?,
                    outer_rect: req_rect_value(map.get("outer").ok_or_else(malformed)?)?,
                },
            ))
        }
        "hidden" => {
            check_keys(map, &["status", "reason", "baseRev"])?;
            let base_revision = match map.get("baseRev") {
                None => return Err(malformed()),
                Some(serde_json::Value::Null) => None,
                Some(_) => Some(req_u64_str(map, "baseRev")?),
            };
            Ok(PresentationUnderlay::Hidden {
                base_revision,
                reason: decode_group_hide_reason(req_str(map, "reason")?)?,
            })
        }
        _ => Err(unknown_value()),
    }
}

fn decode_pager_window(value: &serde_json::Value) -> Result<PagerWindow> {
    let map = as_object(value)?;
    check_keys(map, &["win", "frame"])?;
    Ok(PagerWindow {
        window: WindowId(req_id(map, "win")?),
        frame: req_rect_value(map.get("frame").ok_or_else(malformed)?)?,
    })
}

fn decode_pager_workspace(value: &serde_json::Value) -> Result<PagerWorkspace> {
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &[
            "id",
            "ordinal",
            "label",
            "current",
            "occupied",
            "containsFocused",
            "windows",
        ],
        &["trailing"],
    )?;
    if map
        .get("trailing")
        .is_some_and(|trailing| trailing != &serde_json::Value::Bool(true))
    {
        return Err(reply_snapshot("pager-trailing-false"));
    }
    let ordinal = map
        .get("ordinal")
        .and_then(serde_json::Value::as_u64)
        .and_then(|ordinal| u32::try_from(ordinal).ok())
        .ok_or_else(malformed)?;
    let windows_value = map.get("windows").ok_or_else(malformed)?;
    let window_items = windows_value.as_array().ok_or_else(malformed)?;
    if window_items.len() > MAX_REPLY_WINDOWS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut windows = Vec::with_capacity(window_items.len());
    for item in window_items {
        windows.push(decode_pager_window(item)?);
    }
    Ok(PagerWorkspace {
        id: req_id(map, "id")?,
        ordinal,
        label: req_str(map, "label")?.to_owned(),
        current: req_bool(map, "current")?,
        occupied: req_bool(map, "occupied")?,
        contains_focused: req_bool(map, "containsFocused")?,
        trailing: map.contains_key("trailing"),
        windows,
    })
}

fn decode_pager_screen(value: &serde_json::Value) -> Result<PagerScreen> {
    let map = as_object(value)?;
    check_keys(map, &["out", "workspaces"])?;
    let workspaces_value = map.get("workspaces").ok_or_else(malformed)?;
    let workspace_items = workspaces_value.as_array().ok_or_else(malformed)?;
    if workspace_items.len() > MAX_REPLY_WORKSPACES {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut workspaces = Vec::with_capacity(workspace_items.len());
    for item in workspace_items {
        workspaces.push(decode_pager_workspace(item)?);
    }
    Ok(PagerScreen {
        output: req_id(map, "out")?,
        workspaces,
    })
}

fn decode_reply_presentation_v4(value: &serde_json::Value) -> Result<WorldPresentationV4> {
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &["foreground", "rev", "border", "underlay", "pager"],
        &["suspendedOutput"],
    )?;
    let foreground = req_empty_or_id(map, "foreground")?;
    let suspended_output = match map.get("suspendedOutput") {
        None => None,
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            Some(OutputId(text.clone()))
        }
        Some(_) => return Err(malformed()),
    };
    let pager_value = map.get("pager").ok_or_else(malformed)?;
    let pager_items = pager_value.as_array().ok_or_else(malformed)?;
    if pager_items.len() > MAX_REPLY_OUTPUTS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut pager = Vec::with_capacity(pager_items.len());
    for item in pager_items {
        pager.push(decode_pager_screen(item)?);
    }
    Ok(WorldPresentationV4 {
        foreground,
        rev: req_u64_str(map, "rev")?,
        suspended_output,
        border: decode_border_presentation_v4(map.get("border").ok_or_else(malformed)?)?,
        underlay: decode_presentation_underlay(map.get("underlay").ok_or_else(malformed)?)?,
        pager,
    })
}

/// Optional wire metadata for a decoded v4 reply: the typed reply carries
/// what the wire carries, and this records which omit-if-absent spellings
/// the wire used. `suspended_output_present` names the wire presence of
/// `presentation.suspendedOutput`; it never influences typed equality or
/// engine input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplyWireOptionals {
    pub suspended_output_present: bool,
}

/// Decoded v4 reply plus its optional wire metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedReplyV4 {
    pub reply: WorldReplyV4,
    pub wire: ReplyWireOptionals,
}

/// Strict shape decode of one staged v4 whole-reply object. Enforces the
/// byte budget first, then exact envelope keys, quoted counters, closed
/// enum/union vocabularies, per-section caps, identity uniqueness, and the
/// row coupling rules. Never logs or echoes payload bytes.
pub fn decode_reply_v4(json: &str) -> std::result::Result<DecodedReplyV4, WorldProtocolError> {
    if json.len() > WORLD_MAX_REPLY_BYTES {
        return Err(WorldProtocolError {
            kind: KIND_OVERSIZED,
            message: MSG_OVERSIZED,
        });
    }
    let value: serde_json::Value =
        crate::connection_protocol::parse_strict_json(json).map_err(|e| {
            if e.0 == "oversized" {
                WorldProtocolError {
                    kind: KIND_OVERSIZED,
                    message: MSG_OVERSIZED,
                }
            } else {
                malformed()
            }
        })?;
    let map = as_object(&value)?;
    check_keys(
        map,
        &[
            "v",
            "adapter",
            "gen",
            "req",
            "obs",
            "rev",
            "ready",
            "consumed",
            "scopes",
            "workspaces",
            "outputs",
            "windows",
            "transfers",
            "effects",
            "closeFocus",
            "result",
            "gestureSettled",
            "operationSettled",
            "nativeCreations",
            "warpCursor",
            "presentation",
            "intentWrites",
            "switchSettled",
            "diagnostics",
        ],
    )?;
    let version = map
        .get("v")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(malformed)?;
    if version != u64::from(WORLD_PROTOCOL_VERSION_V4) {
        return Err(WorldProtocolError {
            kind: KIND_VERSION,
            message: MSG_VERSION,
        });
    }
    let scopes_value = map.get("scopes").ok_or_else(malformed)?;
    let scope_items = scopes_value.as_array().ok_or_else(malformed)?;
    if scope_items.len() > MAX_REPLY_SCOPES {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut scopes = Vec::with_capacity(scope_items.len());
    let mut seen_scopes = std::collections::BTreeSet::new();
    for item in scope_items {
        let status = decode_scope_status(item)?;
        let key = match &status.scope {
            AuthorityScope::Output(out) => format!("out:{}", out.0),
            AuthorityScope::Workspace { output, workspace } => {
                format!("ws:{}:{}", output.0, workspace.0)
            }
        };
        if !seen_scopes.insert(key) {
            return Err(reply_snapshot("duplicate-scope"));
        }
        scopes.push(status);
    }
    let workspaces_value = map.get("workspaces").ok_or_else(malformed)?;
    let workspace_items = workspaces_value.as_array().ok_or_else(malformed)?;
    if workspace_items.len() > MAX_REPLY_WORKSPACES {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut workspaces = Vec::with_capacity(workspace_items.len());
    let mut seen_workspaces = std::collections::BTreeSet::new();
    for item in workspace_items {
        let workspace = decode_workspace_desire(item)?;
        if !seen_workspaces.insert((workspace.output.0.clone(), workspace.id.clone())) {
            return Err(reply_snapshot("duplicate-workspace"));
        }
        workspaces.push(workspace);
    }
    let outputs_value = map.get("outputs").ok_or_else(malformed)?;
    let output_items = outputs_value.as_array().ok_or_else(malformed)?;
    if output_items.len() > MAX_REPLY_OUTPUTS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut outputs = Vec::with_capacity(output_items.len());
    let mut seen_outputs = std::collections::BTreeSet::new();
    for item in output_items {
        let output = decode_output_desire(item)?;
        if !seen_outputs.insert(output.output.0.clone()) {
            return Err(reply_snapshot("duplicate-output"));
        }
        outputs.push(output);
    }
    let windows_value = map.get("windows").ok_or_else(malformed)?;
    let window_items = windows_value.as_array().ok_or_else(malformed)?;
    if window_items.len() > MAX_REPLY_WINDOWS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut windows = Vec::with_capacity(window_items.len());
    let mut seen_windows = std::collections::BTreeSet::new();
    for item in window_items {
        let window = decode_window_desire(item)?;
        if !seen_windows.insert(window.lifetime.0.clone()) {
            return Err(reply_snapshot("duplicate-window"));
        }
        windows.push(window);
    }
    let transfers_value = map.get("transfers").ok_or_else(malformed)?;
    let transfer_items = transfers_value.as_array().ok_or_else(malformed)?;
    if transfer_items.len() > MAX_REPLY_TRANSFERS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut transfers = Vec::with_capacity(transfer_items.len());
    let mut seen_transfers = std::collections::BTreeSet::new();
    for item in transfer_items {
        let transfer = decode_transfer_v4(item)?;
        if !seen_transfers.insert(transfer.operation_id.clone()) {
            return Err(reply_snapshot("duplicate-transfer"));
        }
        transfers.push(transfer);
    }
    let effects_value = map.get("effects").ok_or_else(malformed)?;
    let effect_items = effects_value.as_array().ok_or_else(malformed)?;
    if effect_items.len() > MAX_REPLY_EFFECTS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut effects = Vec::with_capacity(effect_items.len());
    let mut seen_effects = std::collections::BTreeSet::new();
    for item in effect_items {
        let effect = decode_effect(item)?;
        if !seen_effects.insert(effect.id.clone()) {
            return Err(reply_snapshot("duplicate-effect"));
        }
        effects.push(effect);
    }
    let command_result = decode_command_result(map.get("result").ok_or_else(malformed)?)?;
    let gesture_settled = decode_gesture_settled(map.get("gestureSettled").ok_or_else(malformed)?)?;
    let operations_value = map.get("operationSettled").ok_or_else(malformed)?;
    let operation_items = operations_value.as_array().ok_or_else(malformed)?;
    if operation_items.len() > MAX_REPLY_OPERATIONS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut operation_settled = Vec::with_capacity(operation_items.len());
    let mut seen_operations = std::collections::BTreeSet::new();
    for item in operation_items {
        let event = decode_operation_event(item)?;
        if !seen_operations.insert(event.operation_id.clone()) {
            return Err(reply_snapshot("duplicate-operation"));
        }
        operation_settled.push(event);
    }
    let creations_value = map.get("nativeCreations").ok_or_else(malformed)?;
    let creation_items = creations_value.as_array().ok_or_else(malformed)?;
    if creation_items.len() > MAX_REPLY_NATIVE_CREATIONS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut native_creations = Vec::with_capacity(creation_items.len());
    let mut seen_creations = std::collections::BTreeSet::new();
    for item in creation_items {
        let creation = decode_native_creation(item)?;
        if !seen_creations.insert(creation.operation_id.clone()) {
            return Err(reply_snapshot("duplicate-native-creation"));
        }
        native_creations.push(creation);
    }
    let cursor_warp = opt_point_or_null(map, "warpCursor")?;
    let presentation =
        decode_reply_presentation_v4(map.get("presentation").ok_or_else(malformed)?)?;
    let wire = ReplyWireOptionals {
        suspended_output_present: map
            .get("presentation")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|presentation| presentation.contains_key("suspendedOutput")),
    };
    let writes_value = map.get("intentWrites").ok_or_else(malformed)?;
    let write_items = writes_value.as_array().ok_or_else(malformed)?;
    if write_items.len() > MAX_INTENT_WRITES {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut intent_writes = Vec::with_capacity(write_items.len());
    let mut seen_writes = std::collections::BTreeSet::new();
    for item in write_items {
        let write = decode_intent_write(item)?;
        if !seen_writes.insert(write.win.0.clone()) {
            return Err(reply_snapshot("duplicate-intent-write"));
        }
        intent_writes.push(write);
    }
    let switches_value = map.get("switchSettled").ok_or_else(malformed)?;
    let switch_items = switches_value.as_array().ok_or_else(malformed)?;
    if switch_items.len() > MAX_REPLY_SWITCH_EVENTS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut switch_settled = Vec::with_capacity(switch_items.len());
    let mut seen_switches = std::collections::BTreeSet::new();
    for item in switch_items {
        let event = decode_switch_event(item)?;
        if !seen_switches.insert(event.operation_id.clone()) {
            return Err(reply_snapshot("duplicate-switch"));
        }
        switch_settled.push(event);
    }
    let diagnostics_value = map.get("diagnostics").ok_or_else(malformed)?;
    let diagnostic_items = diagnostics_value.as_array().ok_or_else(malformed)?;
    if diagnostic_items.len() > MAX_REPLY_DIAGNOSTICS {
        return Err(reply_snapshot("section-over-cap"));
    }
    let mut diagnostics = Vec::with_capacity(diagnostic_items.len());
    for item in diagnostic_items {
        diagnostics.push(decode_row_diagnostic(item)?);
    }
    let adapter = req_id(map, "adapter")?;
    let generation = req_bounded(map, "gen", MAX_GENERATION_LEN, is_generation_text)?;
    let request = req_bounded(map, "req", MAX_CORRELATION_LEN, is_correlation_text)?;
    Ok(DecodedReplyV4 {
        reply: WorldReplyV4 {
            contract_version: WORLD_PROTOCOL_VERSION_V4,
            adapter_incarnation: adapter,
            core_generation: generation,
            request_id: request,
            based_on_observation_seq: req_u64_str(map, "obs")?,
            state_revision: req_u64_str(map, "rev")?,
            ready: req_bool(map, "ready")?,
            command_consumed: req_bool(map, "consumed")?,
            scopes,
            workspaces,
            outputs,
            windows,
            transfers,
            effects,
            close_focus: decode_close_focus(map.get("closeFocus").ok_or_else(malformed)?)?,
            command_result,
            gesture_settled,
            operation_settled,
            native_creations,
            cursor_warp,
            presentation,
            intent_writes,
            switch_settled,
            diagnostics,
        },
        wire,
    })
}

/// Staged v4 connection-context gate for decoder composition: calls the
/// context-free [`decode_reply_v4`] shape decoder, then applies the
/// handshake-declared capability gates the runtime owns. The caller passes
/// the bound [`HostCapabilities`]; the context is never inferred from the
/// reply. All three context shapes stay shape-valid in [`decode_reply_v4`];
/// a wrong-capability row here refuses as `snapshot-invalid` with a fixed
/// rule-token message and no new error catalog or API metadata.
pub fn decode_reply_v4_for_host(
    json: &str,
    host: &HostCapabilities,
) -> std::result::Result<DecodedReplyV4, WorldProtocolError> {
    let decoded = decode_reply_v4(json)?;
    if !host.above
        && decoded
            .reply
            .windows
            .iter()
            .flat_map(|window| window.state.iter())
            .any(|target| target.property == StateProperty::Above)
    {
        return Err(reply_snapshot("above-capability-false"));
    }
    if !host.warp && decoded.reply.cursor_warp.is_some() {
        return Err(reply_snapshot("warp-capability-false"));
    }
    if !host.native_sticky
        && decoded
            .reply
            .windows
            .iter()
            .flat_map(|window| window.state.iter())
            .any(|target| target.property == StateProperty::Sticky)
    {
        return Err(reply_snapshot("sticky-capability-false"));
    }
    Ok(decoded)
}

// -- Staged canonical v4 reply writer --
// Checked canonical encode of one decoded v4 reply: compact JSON in
// contract table key order with the codec row key order, quoted counters
// via to_string, JSON integers for order/ordinal/floats, C0 escapes as
// lowercase \u00xx through the shared push_json_string, and no final LF.
// Reuses the staged border encoder, the frame witness encoder, and the
// legacy per-row lane encoders whose bytes are identical for the v4 row
// (command result, gesture settled, operation settled, native creations,
// switch settled, row diagnostics, window target lanes). Typed values
// determine every spelling except suspended-output presence, which the
// caller carries as wire metadata; an inconsistent combination refuses as
// snapshot-invalid with a fixed message, never a silent null or a payload
// echo. Unrepresentable or decoder-refused typed values refuse with the
// fixed 5a vocabulary where the rule matches. The built bytes re-decode
// through decode_reply_v4 and must equal the input, so a writer bug
// surfaces as snapshot-invalid/encode-mismatch instead of corrupt bytes.

fn check_reply_cap(len: usize, max: usize) -> Result<()> {
    if len > max {
        return Err(reply_snapshot("section-over-cap"));
    }
    Ok(())
}

fn check_reply_id(text: &str) -> Result<()> {
    if !is_opaque_id(text) {
        return Err(unknown_value());
    }
    Ok(())
}

fn push_checked_rect(out: &mut String, rect: &Rect) -> Result<()> {
    if !valid_carried_rect(rect.x, rect.y, rect.w, rect.h) {
        return Err(unknown_value());
    }
    push_rect(out, rect);
    Ok(())
}

fn push_checked_opt_rect(out: &mut String, rect: &Option<Rect>) -> Result<()> {
    match rect {
        None => out.push_str("null"),
        Some(rect) => push_checked_rect(out, rect)?,
    }
    Ok(())
}

fn push_empty_or_id(out: &mut String, text: &str) -> Result<()> {
    if !text.is_empty() {
        check_reply_id(text)?;
    }
    push_json_string(out, text);
    Ok(())
}

fn push_opt_id_or_empty(out: &mut String, value: &Option<String>) -> Result<()> {
    match value {
        None => out.push_str("\"\""),
        Some(text) => {
            check_reply_id(text)?;
            push_json_string(out, text);
        }
    }
    Ok(())
}

fn scope_status_key(scope: &AuthorityScope) -> String {
    match scope {
        AuthorityScope::Output(output) => format!("out:{}", output.0),
        AuthorityScope::Workspace { output, workspace } => {
            format!("ws:{}:{}", output.0, workspace.0)
        }
    }
}

fn encode_scope_status_v4(row: &ScopeStatus, out: &mut String) -> Result<()> {
    match &row.scope {
        AuthorityScope::Output(output) => check_reply_id(&output.0)?,
        AuthorityScope::Workspace { output, workspace } => {
            check_reply_id(&output.0)?;
            check_reply_id(&workspace.0)?;
        }
    }
    out.push_str("{\"scope\":");
    push_authority_scope(out, &row.scope);
    out.push_str(",\"auth\":");
    push_json_string(out, row.authority.as_str());
    out.push_str(",\"reason\":");
    push_json_string(
        out,
        row.authority.hold_reason().map_or("", HoldReason::as_str),
    );
    out.push('}');
    Ok(())
}

fn encode_workspace_desire_v4(row: &WorkspaceDesire, out: &mut String) -> Result<()> {
    check_reply_id(&row.id)?;
    check_reply_id(&row.output.0)?;
    if let Some(bind) = row.native_binding.as_deref() {
        check_reply_id(bind)?;
    }
    match &row.scope {
        AuthorityScope::Output(output) => check_reply_id(&output.0)?,
        AuthorityScope::Workspace { output, workspace } => {
            check_reply_id(&output.0)?;
            check_reply_id(&workspace.0)?;
        }
    }
    out.push_str("{\"id\":");
    push_json_string(out, &row.id);
    out.push_str(",\"out\":");
    push_json_string(out, &row.output.0);
    out.push_str(",\"bind\":");
    push_opt_id_or_empty(out, &row.native_binding)?;
    out.push_str(",\"scope\":");
    push_authority_scope(out, &row.scope);
    out.push_str(",\"order\":");
    out.push_str(&row.order.to_string());
    out.push_str(",\"mode\":");
    push_json_string(out, row.tiling_mode.as_str());
    out.push_str(",\"exist\":");
    out.push_str(if row.existence { "true" } else { "false" });
    out.push_str(",\"hold\":");
    push_json_string(out, row.hold.map_or("", HoldReason::as_str));
    if row.trailing {
        out.push_str(",\"trailing\":true");
    }
    out.push('}');
    Ok(())
}

fn encode_output_desire_v4(row: &OutputDesire, out: &mut String) -> Result<()> {
    check_reply_id(&row.output.0)?;
    if let Some(view) = row.logical_view.as_deref() {
        check_reply_id(view)?;
    }
    out.push_str("{\"out\":");
    push_json_string(out, &row.output.0);
    out.push_str(",\"view\":");
    push_opt_id_or_empty(out, &row.logical_view)?;
    out.push_str(",\"mode\":");
    push_json_string(out, row.tiling_mode.as_str());
    out.push_str(",\"hold\":");
    push_json_string(out, row.hold.map_or("", HoldReason::as_str));
    out.push('}');
    Ok(())
}

fn encode_window_desire_v4(row: &WindowDesire, out: &mut String) -> Result<()> {
    check_reply_id(&row.lifetime.0)?;
    check_reply_id(&row.output.0)?;
    check_reply_id(&row.workspace)?;
    if let Some(rect) = row.allocation.as_ref()
        && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
    {
        return Err(unknown_value());
    }
    if let Some(rect) = row.effective_frame.as_ref()
        && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
    {
        return Err(unknown_value());
    }
    if let Some(op) = row.frame_operation.as_deref() {
        check_reply_id(op)?;
    }
    if let Some(op) = row.clear_operation.as_deref() {
        check_reply_id(op)?;
    }
    if let Some(visibility) = row.visibility.as_ref()
        && visibility.shown
        && visibility.fallback
    {
        return Err(reply_snapshot("visibility-show-fallback"));
    }
    if row.state.len() > MAX_STATE_TARGETS_PER_WINDOW {
        return Err(reply_snapshot("state-target-count"));
    }
    let mut seen_state = std::collections::BTreeSet::new();
    for target in &row.state {
        if !seen_state.insert(target.property) {
            return Err(reply_snapshot("state-target-count"));
        }
    }
    out.push_str("{\"win\":");
    push_json_string(out, &row.lifetime.0);
    out.push_str(",\"out\":");
    push_json_string(out, &row.output.0);
    out.push_str(",\"ws\":");
    push_json_string(out, &row.workspace);
    out.push_str(",\"alloc\":");
    push_checked_opt_rect(out, &row.allocation)?;
    out.push_str(",\"frame\":");
    push_checked_opt_rect(out, &row.effective_frame)?;
    out.push_str(",\"frame_auth\":");
    push_json_string(out, row.frame_authority.as_str());
    out.push_str(",\"frame_hold\":");
    push_json_string(
        out,
        row.frame_authority
            .hold_reason()
            .map_or("", HoldReason::as_str),
    );
    out.push_str(",\"member_auth\":");
    push_json_string(out, row.membership_authority.as_str());
    out.push_str(",\"member_hold\":");
    push_json_string(
        out,
        row.membership_authority
            .hold_reason()
            .map_or("", HoldReason::as_str),
    );
    out.push_str(",\"frame_op\":");
    push_opt_id_or_empty(out, &row.frame_operation)?;
    out.push_str(",\"floating\":");
    out.push_str(if row.floating { "true" } else { "false" });
    out.push_str(",\"float_origin\":");
    push_json_string(out, row.float_origin.map_or("", FloatOrigin::as_str));
    out.push_str(",\"clear_op\":");
    push_opt_id_or_empty(out, &row.clear_operation)?;
    tiler_core::world_sizing::encode_window_target_lanes(out, row);
    out.push('}');
    Ok(())
}

fn encode_transfer_v4(row: &TransferTargetV4, out: &mut String) -> Result<()> {
    check_reply_id(&row.operation_id)?;
    check_reply_id(&row.target_output.0)?;
    check_reply_id(&row.target_workspace)?;
    match &row.kind {
        TransferKind::Window(window) => check_reply_id(&window.0)?,
        TransferKind::View | TransferKind::Migration => {}
    }
    out.push_str("{\"op\":");
    push_json_string(out, &row.operation_id);
    out.push_str(",\"kind\":");
    push_json_string(
        out,
        match &row.kind {
            TransferKind::Window(_) => "window",
            TransferKind::View => "view",
            TransferKind::Migration => "migration",
        },
    );
    out.push_str(",\"win\":");
    match &row.kind {
        TransferKind::Window(window) => push_json_string(out, &window.0),
        TransferKind::View | TransferKind::Migration => out.push_str("\"\""),
    }
    out.push_str(",\"out\":");
    push_json_string(out, &row.target_output.0);
    out.push_str(",\"ws\":");
    push_json_string(out, &row.target_workspace);
    out.push_str(",\"follow\":");
    out.push_str(if row.follow { "true" } else { "false" });
    out.push_str(",\"held\":");
    out.push_str(if row.held { "true" } else { "false" });
    out.push_str(",\"cmdSeq\":");
    match row.command_seq {
        None => out.push_str("null"),
        Some(seq) => {
            out.push('"');
            out.push_str(&seq.to_string());
            out.push('"');
        }
    }
    out.push('}');
    Ok(())
}

fn encode_effect_v4(row: &OneShotEffect, out: &mut String) -> Result<()> {
    check_reply_id(&row.id)?;
    check_reply_id(&row.target.0)?;
    let explicit = row.cause == tiler_core::world_report::EffectCause::ExplicitCommand;
    if explicit && row.command_seq.is_none() {
        return Err(reply_snapshot("effect-explicit-null-seq"));
    }
    if !explicit && row.command_seq.is_some() {
        return Err(reply_snapshot("effect-non-explicit-seq"));
    }
    if !explicit && row.await_shown {
        return Err(reply_snapshot("effect-automatic-await-shown"));
    }
    out.push_str("{\"id\":");
    push_json_string(out, &row.id);
    out.push_str(",\"kind\":");
    push_json_string(out, row.kind.as_str());
    out.push_str(",\"target\":");
    push_json_string(out, &row.target.0);
    out.push_str(",\"rev\":\"");
    out.push_str(&row.issued_revision.to_string());
    out.push_str("\",\"cause\":");
    push_json_string(out, row.cause.as_str());
    out.push_str(",\"cmdSeq\":");
    match row.command_seq {
        None => out.push_str("null"),
        Some(seq) => {
            out.push('"');
            out.push_str(&seq.to_string());
            out.push('"');
        }
    }
    out.push_str(",\"awaitShown\":");
    out.push_str(if row.await_shown { "true" } else { "false" });
    out.push('}');
    Ok(())
}

fn valid_operation_cause(text: &str) -> bool {
    if CommandRejectKind::parse(text).is_some() {
        return true;
    }
    matches!(
        text,
        "baseline-non-subject"
            | "closed"
            | "constraints"
            | "early-verdict"
            | "external"
            | "fixed"
            | "floating"
            | "homing-loss"
            | "invalid-direction"
            | "invalid-geometry"
            | "non-subject-focus"
            | "refused-with-unmaximized"
            | "sticky"
            | "tiling-changed"
            | "topology-unreadable"
            | "unknown-output"
            | "unknown-window"
    )
}

fn encode_operation_event_v4(row: &OperationEvent, out: &mut String) -> Result<()> {
    check_reply_id(&row.operation_id)?;
    check_reply_id(&row.window.0)?;
    if let Some(cause) = row.cause.as_deref()
        && !valid_operation_cause(cause)
    {
        return Err(unknown_value());
    }
    out.push_str("{\"op\":");
    push_json_string(out, &row.operation_id);
    out.push_str(",\"win\":");
    push_json_string(out, &row.window.0);
    out.push_str(",\"outcome\":");
    push_json_string(out, row.outcome.as_str());
    out.push_str(",\"cause\":");
    push_empty_or_id(out, row.cause.as_deref().unwrap_or_default())?;
    out.push('}');
    Ok(())
}

fn encode_native_creation_v4(row: &NativeCreationDesire, out: &mut String) -> Result<()> {
    check_reply_id(&row.operation_id)?;
    check_reply_id(&row.logical_workspace)?;
    check_reply_id(&row.target_output.0)?;
    out.push_str("{\"op\":");
    push_json_string(out, &row.operation_id);
    out.push_str(",\"ws\":");
    push_json_string(out, &row.logical_workspace);
    out.push_str(",\"out\":");
    push_json_string(out, &row.target_output.0);
    out.push('}');
    Ok(())
}

fn valid_directional_cause(text: &str) -> bool {
    matches!(
        text,
        "no-target" | "unknown-window" | "focus-present" | "no-source" | "no-adjacent-output"
    )
}

fn encode_command_result_v4(result: &Option<CommandResult>, out: &mut String) -> Result<()> {
    match result {
        None => out.push_str("null"),
        Some(CommandResult::DragPreview {
            rect,
            hover,
            domain,
            source,
            target,
            revision: _,
        }) => {
            if !valid_carried_rect(rect.x, rect.y, rect.w, rect.h) {
                return Err(unknown_value());
            }
            check_reply_id(&domain.output.0)?;
            check_reply_id(&domain.workspace.0)?;
            check_reply_id(&source.output.0)?;
            check_reply_id(&source.workspace.0)?;
            check_reply_id(&target.0)?;
            if let Some(hover) = hover {
                check_reply_id(&hover.domain.output.0)?;
                check_reply_id(&hover.domain.workspace.0)?;
                check_reply_id(&hover.source_leaf.0)?;
                check_reply_id(&hover.source_window.0)?;
                if let Some(prior) = hover.prior.as_ref() {
                    check_reply_id(&prior.group.0)?;
                }
                check_reply_id(&hover.prior_domain.output.0)?;
                check_reply_id(&hover.prior_domain.workspace.0)?;
            }
            tiler_core::world_sizing::encode_command_result(out, result);
        }
        Some(CommandResult::Float { window, rect, .. }) => {
            check_reply_id(&window.0)?;
            if let Some(rect) = rect
                && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
            {
                return Err(unknown_value());
            }
            tiler_core::world_sizing::encode_command_result(out, result);
        }
        Some(CommandResult::Migration {
            source,
            target,
            members,
            ..
        }) => {
            check_reply_cap(members.len(), MAX_REPLY_WINDOWS)?;
            check_reply_id(&source.output.0)?;
            check_reply_id(&source.workspace.0)?;
            check_reply_id(&target.output.0)?;
            check_reply_id(&target.workspace.0)?;
            for member in members {
                check_reply_id(&member.0)?;
            }
            tiler_core::world_sizing::encode_command_result(out, result);
        }
        Some(CommandResult::Directional {
            target,
            rect,
            cause,
            ..
        }) => {
            if let Some(target) = target {
                check_reply_id(&target.0)?;
            }
            if let Some(rect) = rect
                && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
            {
                return Err(unknown_value());
            }
            if let Some(cause) = cause
                && !valid_directional_cause(cause)
            {
                return Err(unknown_value());
            }
            tiler_core::world_sizing::encode_command_result(out, result);
        }
        Some(CommandResult::Maximize { window, .. }) => {
            check_reply_id(&window.0)?;
            tiler_core::world_sizing::encode_command_result(out, result);
        }
        Some(CommandResult::Rejected { .. }) => {
            tiler_core::world_sizing::encode_command_result(out, result);
        }
    }
    Ok(())
}

fn encode_gesture_settled_v4(settled: &Option<GestureSettled>, out: &mut String) {
    tiler_core::world_sizing::encode_gesture_settled(out, settled);
}

fn encode_intent_write_v4(row: &IntentWrite, out: &mut String) -> Result<()> {
    check_reply_id(&row.win.0)?;
    if let Some(data) = row.data.as_deref()
        && data.len() > MAX_INTENT_DATA_BYTES
    {
        return Err(reply_snapshot("intent-data-invalid"));
    }
    out.push_str("{\"win\":");
    push_json_string(out, &row.win.0);
    out.push_str(",\"data\":");
    match row.data.as_deref() {
        Some(data) => push_json_string(out, data),
        None => out.push_str("null"),
    }
    out.push('}');
    Ok(())
}

fn encode_switch_event_v4(row: &SwitchEvent, out: &mut String) -> Result<()> {
    check_reply_id(&row.operation_id)?;
    check_reply_cap(row.failed.len(), MAX_REPLY_WINDOWS)?;
    for window in &row.failed {
        check_reply_id(&window.0)?;
    }
    out.push_str("{\"op\":");
    push_json_string(out, &row.operation_id);
    out.push_str(",\"outcome\":");
    push_json_string(out, row.outcome.as_str());
    out.push_str(",\"failed\":[");
    for (i, window) in row.failed.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(out, &window.0);
    }
    out.push_str("],\"focus\":");
    push_json_string(out, row.focus.as_str());
    out.push_str(",\"fallback\":");
    out.push_str(if row.fallback { "true" } else { "false" });
    out.push('}');
    Ok(())
}

fn encode_row_diagnostic_v4(row: &RowDiagnostic, out: &mut String) -> Result<()> {
    if let Some(id) = row.id.as_deref() {
        check_reply_id(id)?;
    }
    match &row.scope {
        AuthorityScope::Output(output) => check_reply_id(&output.0)?,
        AuthorityScope::Workspace { output, workspace } => {
            check_reply_id(&output.0)?;
            check_reply_id(&workspace.0)?;
        }
    }
    tiler_core::world_sizing::push_row_diagnostic(out, row);
    Ok(())
}

fn encode_underlay_v4(row: &PresentationUnderlay, out: &mut String) -> Result<()> {
    match row {
        PresentationUnderlay::Found(found) => {
            check_reply_id(&found.home.output.0)?;
            check_reply_id(&found.home.workspace.0)?;
            check_reply_id(&found.group.0)?;
            check_reply_id(&found.focused_leaf.0)?;
            check_reply_id(&found.focused_window.0)?;
            if !valid_carried_rect(
                found.union_rect.x,
                found.union_rect.y,
                found.union_rect.w,
                found.union_rect.h,
            ) {
                return Err(unknown_value());
            }
            if !valid_carried_rect(
                found.outer_rect.x,
                found.outer_rect.y,
                found.outer_rect.w,
                found.outer_rect.h,
            ) {
                return Err(unknown_value());
            }
            check_reply_cap(found.members.len(), MAX_REPLY_WINDOWS)?;
            for member in &found.members {
                check_reply_id(&member.leaf.0)?;
                check_reply_id(&member.window.0)?;
                if !valid_carried_rect(member.rect.x, member.rect.y, member.rect.w, member.rect.h) {
                    return Err(unknown_value());
                }
            }
            out.push_str("{\"status\":\"found\",\"reason\":\"\",\"baseRev\":\"");
            out.push_str(&found.base_revision.to_string());
            out.push_str("\",\"homeOut\":");
            push_json_string(out, &found.home.output.0);
            out.push_str(",\"homeWs\":");
            push_json_string(out, &found.home.workspace.0);
            out.push_str(",\"group\":");
            push_json_string(out, &found.group.0);
            out.push_str(",\"leaf\":");
            push_json_string(out, &found.focused_leaf.0);
            out.push_str(",\"win\":");
            push_json_string(out, &found.focused_window.0);
            out.push_str(",\"union\":");
            push_checked_rect(out, &found.union_rect)?;
            out.push_str(",\"outer\":");
            push_checked_rect(out, &found.outer_rect)?;
            out.push_str(",\"members\":[");
            for (i, member) in found.members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str("{\"leaf\":");
                push_json_string(out, &member.leaf.0);
                out.push_str(",\"win\":");
                push_json_string(out, &member.window.0);
                out.push_str(",\"rect\":");
                push_checked_rect(out, &member.rect)?;
                out.push('}');
            }
            out.push_str("]}");
        }
        PresentationUnderlay::Hidden {
            base_revision,
            reason,
        } => {
            out.push_str("{\"status\":\"hidden\",\"reason\":");
            push_json_string(out, reason.as_str());
            out.push_str(",\"baseRev\":");
            match base_revision {
                Some(rev) => {
                    out.push('"');
                    out.push_str(&rev.to_string());
                    out.push('"');
                }
                None => out.push_str("null"),
            }
            out.push('}');
        }
    }
    Ok(())
}

fn encode_pager_v4(pager: &[PagerScreen], out: &mut String) -> Result<()> {
    check_reply_cap(pager.len(), MAX_REPLY_OUTPUTS)?;
    out.push('[');
    for (i, screen) in pager.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        check_reply_id(&screen.output)?;
        check_reply_cap(screen.workspaces.len(), MAX_REPLY_WORKSPACES)?;
        out.push_str("{\"out\":");
        push_json_string(out, &screen.output);
        out.push_str(",\"workspaces\":[");
        for (j, workspace) in screen.workspaces.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            check_reply_id(&workspace.id)?;
            check_reply_cap(workspace.windows.len(), MAX_REPLY_WINDOWS)?;
            for window in &workspace.windows {
                check_reply_id(&window.window.0)?;
                if !valid_carried_rect(
                    window.frame.x,
                    window.frame.y,
                    window.frame.w,
                    window.frame.h,
                ) {
                    return Err(unknown_value());
                }
            }
            out.push_str("{\"id\":");
            push_json_string(out, &workspace.id);
            out.push_str(",\"ordinal\":");
            out.push_str(&workspace.ordinal.to_string());
            out.push_str(",\"label\":");
            push_json_string(out, &workspace.label);
            out.push_str(",\"current\":");
            out.push_str(if workspace.current { "true" } else { "false" });
            out.push_str(",\"occupied\":");
            out.push_str(if workspace.occupied { "true" } else { "false" });
            out.push_str(",\"containsFocused\":");
            out.push_str(if workspace.contains_focused {
                "true"
            } else {
                "false"
            });
            if workspace.trailing {
                out.push_str(",\"trailing\":true");
            }
            out.push_str(",\"windows\":[");
            for (k, window) in workspace.windows.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                out.push_str("{\"win\":");
                push_json_string(out, &window.window.0);
                out.push_str(",\"frame\":");
                push_checked_rect(out, &window.frame)?;
                out.push('}');
            }
            out.push_str("]}");
        }
        out.push_str("]}");
    }
    out.push(']');
    Ok(())
}

fn encode_reply_presentation_v4(
    presentation: &WorldPresentationV4,
    suspended_present: bool,
    out: &mut String,
) -> Result<()> {
    if !presentation.foreground.is_empty() && !is_opaque_id(&presentation.foreground) {
        return Err(unknown_value());
    }
    match (&presentation.suspended_output, suspended_present) {
        (None, false) => {}
        (Some(_), true) => {}
        _ => return Err(reply_snapshot("suspended-output-inconsistent")),
    }
    if let Some(output) = presentation.suspended_output.as_ref() {
        check_reply_id(&output.0)?;
    }
    out.push_str("{\"foreground\":");
    push_json_string(out, &presentation.foreground);
    out.push_str(",\"rev\":\"");
    out.push_str(&presentation.rev.to_string());
    out.push('"');
    if let Some(output) = presentation.suspended_output.as_ref() {
        out.push_str(",\"suspendedOutput\":");
        push_json_string(out, &output.0);
    }
    out.push_str(",\"border\":");
    out.push_str(&encode_border_presentation_v4(&presentation.border)?);
    out.push_str(",\"underlay\":");
    encode_underlay_v4(&presentation.underlay, out)?;
    out.push_str(",\"pager\":");
    encode_pager_v4(&presentation.pager, out)?;
    out.push('}');
    Ok(())
}

/// Checked canonical encode of one decoded v4 reply. Refuses inconsistent
/// wire metadata and unspellable typed values instead of emitting
/// non-wire bytes.
pub fn encode_reply_v4(decoded: &DecodedReplyV4) -> Result<String> {
    let reply = &decoded.reply;
    if reply.contract_version != WORLD_PROTOCOL_VERSION_V4 {
        return Err(WorldProtocolError {
            kind: KIND_VERSION,
            message: MSG_VERSION,
        });
    }
    if !is_opaque_id(&reply.adapter_incarnation) {
        return Err(unknown_value());
    }
    if reply.core_generation.len() > MAX_GENERATION_LEN
        || !is_generation_text(&reply.core_generation)
    {
        return Err(unknown_value());
    }
    if reply.request_id.len() > MAX_CORRELATION_LEN || !is_correlation_text(&reply.request_id) {
        return Err(unknown_value());
    }
    check_reply_cap(reply.scopes.len(), MAX_REPLY_SCOPES)?;
    check_reply_cap(reply.workspaces.len(), MAX_REPLY_WORKSPACES)?;
    check_reply_cap(reply.outputs.len(), MAX_REPLY_OUTPUTS)?;
    check_reply_cap(reply.windows.len(), MAX_REPLY_WINDOWS)?;
    check_reply_cap(reply.transfers.len(), MAX_REPLY_TRANSFERS)?;
    check_reply_cap(reply.effects.len(), MAX_REPLY_EFFECTS)?;
    check_reply_cap(reply.operation_settled.len(), MAX_REPLY_OPERATIONS)?;
    check_reply_cap(reply.native_creations.len(), MAX_REPLY_NATIVE_CREATIONS)?;
    check_reply_cap(reply.intent_writes.len(), MAX_INTENT_WRITES)?;
    check_reply_cap(reply.switch_settled.len(), MAX_REPLY_SWITCH_EVENTS)?;
    check_reply_cap(reply.diagnostics.len(), MAX_REPLY_DIAGNOSTICS)?;
    let mut seen_scopes = std::collections::BTreeSet::new();
    for scope in &reply.scopes {
        let key = scope_status_key(&scope.scope);
        if !seen_scopes.insert(key) {
            return Err(reply_snapshot("duplicate-scope"));
        }
    }
    let mut seen_workspaces = std::collections::BTreeSet::new();
    for workspace in &reply.workspaces {
        if !seen_workspaces.insert((workspace.output.0.clone(), workspace.id.clone())) {
            return Err(reply_snapshot("duplicate-workspace"));
        }
    }
    let mut seen_outputs = std::collections::BTreeSet::new();
    for output in &reply.outputs {
        if !seen_outputs.insert(output.output.0.clone()) {
            return Err(reply_snapshot("duplicate-output"));
        }
    }
    let mut seen_windows = std::collections::BTreeSet::new();
    for window in &reply.windows {
        if !seen_windows.insert(window.lifetime.0.clone()) {
            return Err(reply_snapshot("duplicate-window"));
        }
    }
    let mut seen_transfers = std::collections::BTreeSet::new();
    for transfer in &reply.transfers {
        if !seen_transfers.insert(transfer.operation_id.clone()) {
            return Err(reply_snapshot("duplicate-transfer"));
        }
    }
    let mut seen_effects = std::collections::BTreeSet::new();
    for effect in &reply.effects {
        if !seen_effects.insert(effect.id.clone()) {
            return Err(reply_snapshot("duplicate-effect"));
        }
    }
    let mut seen_operations = std::collections::BTreeSet::new();
    for event in &reply.operation_settled {
        if !seen_operations.insert(event.operation_id.clone()) {
            return Err(reply_snapshot("duplicate-operation"));
        }
    }
    let mut seen_creations = std::collections::BTreeSet::new();
    for creation in &reply.native_creations {
        if !seen_creations.insert(creation.operation_id.clone()) {
            return Err(reply_snapshot("duplicate-native-creation"));
        }
    }
    let mut seen_writes = std::collections::BTreeSet::new();
    for write in &reply.intent_writes {
        if !seen_writes.insert(write.win.0.clone()) {
            return Err(reply_snapshot("duplicate-intent-write"));
        }
    }
    let mut seen_switches = std::collections::BTreeSet::new();
    for event in &reply.switch_settled {
        if !seen_switches.insert(event.operation_id.clone()) {
            return Err(reply_snapshot("duplicate-switch"));
        }
    }
    if let Some((x, y)) = reply.cursor_warp
        && (!(-tiler_core::bounds::GEOMETRY_BOUND..=tiler_core::bounds::GEOMETRY_BOUND)
            .contains(&x)
            || !(-tiler_core::bounds::GEOMETRY_BOUND..=tiler_core::bounds::GEOMETRY_BOUND)
                .contains(&y))
    {
        return Err(unknown_value());
    }
    let mut out = String::new();
    out.push_str("{\"v\":");
    out.push_str(&reply.contract_version.to_string());
    out.push_str(",\"adapter\":");
    push_json_string(&mut out, &reply.adapter_incarnation);
    out.push_str(",\"gen\":");
    push_json_string(&mut out, &reply.core_generation);
    out.push_str(",\"req\":");
    push_json_string(&mut out, &reply.request_id);
    out.push_str(",\"obs\":\"");
    out.push_str(&reply.based_on_observation_seq.to_string());
    out.push_str("\",\"rev\":\"");
    out.push_str(&reply.state_revision.to_string());
    out.push_str("\",\"ready\":");
    out.push_str(if reply.ready { "true" } else { "false" });
    out.push_str(",\"consumed\":");
    out.push_str(if reply.command_consumed {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"scopes\":[");
    for (i, scope) in reply.scopes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_scope_status_v4(scope, &mut out)?;
    }
    out.push_str("],\"workspaces\":[");
    for (i, workspace) in reply.workspaces.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_workspace_desire_v4(workspace, &mut out)?;
    }
    out.push_str("],\"outputs\":[");
    for (i, output) in reply.outputs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_output_desire_v4(output, &mut out)?;
    }
    out.push_str("],\"windows\":[");
    for (i, window) in reply.windows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_window_desire_v4(window, &mut out)?;
    }
    out.push_str("],\"transfers\":[");
    for (i, transfer) in reply.transfers.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_transfer_v4(transfer, &mut out)?;
    }
    out.push_str("],\"effects\":[");
    for (i, effect) in reply.effects.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_effect_v4(effect, &mut out)?;
    }
    out.push_str("],\"closeFocus\":");
    match &reply.close_focus {
        None => out.push_str("null"),
        Some(request) => {
            out.push_str("{\"id\":");
            push_json_string(&mut out, &request.id);
            out.push_str(",\"target\":");
            push_json_string(&mut out, &request.target.0);
            out.push_str(",\"rev\":\"");
            out.push_str(&request.issued_revision.to_string());
            out.push_str("\",\"cause\":\"");
            out.push_str(request.cause.as_str());
            out.push_str("\",\"out\":");
            push_json_string(&mut out, &request.output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, &request.workspace);
            out.push('}');
        }
    }
    out.push_str(",\"result\":");
    encode_command_result_v4(&reply.command_result, &mut out)?;
    out.push_str(",\"gestureSettled\":");
    encode_gesture_settled_v4(&reply.gesture_settled, &mut out);
    out.push_str(",\"operationSettled\":[");
    for (i, event) in reply.operation_settled.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_operation_event_v4(event, &mut out)?;
    }
    out.push_str("],\"nativeCreations\":[");
    for (i, creation) in reply.native_creations.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_native_creation_v4(creation, &mut out)?;
    }
    out.push_str("],\"warpCursor\":");
    match reply.cursor_warp {
        Some((x, y)) => {
            out.push_str(&format!("[{x},{y}]"));
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"presentation\":");
    encode_reply_presentation_v4(
        &reply.presentation,
        decoded.wire.suspended_output_present,
        &mut out,
    )?;
    out.push_str(",\"intentWrites\":[");
    for (i, write) in reply.intent_writes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_intent_write_v4(write, &mut out)?;
    }
    out.push_str("],\"switchSettled\":[");
    for (i, event) in reply.switch_settled.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_switch_event_v4(event, &mut out)?;
    }
    out.push_str("],\"diagnostics\":[");
    for (i, diagnostic) in reply.diagnostics.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_row_diagnostic_v4(diagnostic, &mut out)?;
    }
    out.push_str("]}");
    let back = match decode_reply_v4(&out) {
        Ok(back) => back,
        Err(_) => return Err(reply_snapshot("encode-mismatch")),
    };
    if back != *decoded {
        return Err(reply_snapshot("encode-mismatch"));
    }
    Ok(out)
}

// -- Staged canonical v4 report writer --
// Checked canonical encode of one decoded v4 report: compact JSON in
// contract table key order with the codec/table key order for rows,
// shortest binary64 spelling, C0 escapes as lowercase `\u00xx`, and no
// final LF. Reuses the row encoders (`encode_screen_canonical`,
// `encode_window_canonical`, `encode_held_screen_canonical`,
// `encode_held_window_canonical`, `encode_presentation_ingress_v4`,
// `encode_pager_frame_v4`, `encode_frame_witness`, `encode_render_target`,
// `encode_renderer_input`) and the shared canonical scalar writers, so no
// number/string spelling is duplicated here. Checked: inconsistent wire
// metadata or unrepresentable values refuse as `snapshot-invalid` with a
// fixed message (never a silent `null`, never a payload echo) instead of
// emitting non-wire bytes. The built bytes re-decode through
// [`decode_report_v4`] and must equal the input, so a writer bug surfaces
// as `snapshot-invalid`/`encode-mismatch` instead of corrupt bytes.

/// Fixed-message refusal for the staged v4 report writer: every writer
/// refusal is `snapshot-invalid` with a fixed message, never a payload
/// echo.
fn snapshot_invalid(message: &'static str) -> WorldProtocolError {
    WorldProtocolError {
        kind: KIND_SNAPSHOT,
        message,
    }
}

/// Merge one report lane from its valid rows (kept in relative order) and
/// its held rows (pinned to their carried ordinals) into wire order.
/// Held ordinals must be unique and inside the merged lane.
fn merge_lane(valid: Vec<String>, held: Vec<(usize, String)>) -> Result<Vec<String>> {
    let total = valid.len() + held.len();
    let mut seen = std::collections::BTreeSet::new();
    for (ordinal, _) in &held {
        if *ordinal >= total || !seen.insert(*ordinal) {
            return Err(snapshot_invalid("held-ordinal-invalid"));
        }
    }
    let mut slots: Vec<Option<String>> = (0..total).map(|_| None).collect();
    for (ordinal, bytes) in held {
        slots[ordinal] = Some(bytes);
    }
    let mut valid = valid.into_iter();
    for slot in slots.iter_mut() {
        if slot.is_none() {
            slot.replace(valid.next().expect("slots match valid count"));
        }
    }
    Ok(slots
        .into_iter()
        .map(|slot| slot.expect("slot filled"))
        .collect())
}

/// Encode one retained held row field map in stored order (canonical schema
/// order at decode, then extras lexicographically). A non-finite retained
/// number anywhere inside refuses instead of normalizing to `null`.
fn encode_held_fields(fields: &[(String, RawSettingValue)]) -> Result<String> {
    let mut out = String::new();
    out.push('{');
    for (i, (key, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, key);
        out.push(':');
        push_raw_setting_value(&mut out, value).map_err(|_| snapshot_invalid("nonfinite"))?;
    }
    out.push('}');
    Ok(out)
}

/// Checked canonical encode of one valid v4 native row
/// (`{"out":S,"order":[S...],"current":S|null,"occupancy":[{native,count}]}`).
fn encode_native_observation_v4(row: &NativeObservation) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"out\":");
    push_json_string(&mut out, &row.output.0);
    out.push_str(",\"order\":[");
    for (i, id) in row.native_order.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, id);
    }
    out.push_str("],\"current\":");
    match &row.current_native {
        None => out.push_str("null"),
        Some(id) => push_json_string(&mut out, id),
    }
    out.push_str(",\"occupancy\":[");
    for (i, item) in row.occupancy.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"native\":");
        push_json_string(&mut out, &item.native);
        out.push_str(",\"count\":");
        match item.count {
            None => out.push_str("null"),
            Some(count) => out.push_str(&count.to_string()),
        }
        out.push('}');
    }
    out.push_str("]}");
    Ok(out)
}

/// Checked canonical encode of one valid v4 frame row.
fn encode_frame_attempt_v4(row: &tiler_core::world_report::FrameAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"win\":");
    push_json_string(&mut out, &row.window.0);
    out.push_str(",\"target\":");
    push_rect(&mut out, &row.target);
    out.push_str(",\"setter\":");
    push_json_string(&mut out, row.setter.as_str());
    out.push_str(",\"readback\":");
    push_opt_rect(&mut out, &row.readback);
    out.push_str(",\"dpiBefore\":");
    push_opt_u32(&mut out, &row.dpi_before);
    out.push_str(",\"dpiAfter\":");
    push_opt_u32(&mut out, &row.dpi_after);
    out.push_str(",\"attemptMs\":\"");
    out.push_str(&row.attempt_ms.to_string());
    out.push_str("\",\"op\":");
    push_json_string(&mut out, row.operation_id.as_deref().unwrap_or(""));
    out.push_str(",\"pre\":");
    push_opt_rect(&mut out, &row.prewrite);
    out.push('}');
    Ok(out)
}

/// Canonical encode of one nullable bare `u32` lane scalar.
fn push_opt_u32(out: &mut String, value: &Option<u32>) {
    match value {
        None => out.push_str("null"),
        Some(value) => out.push_str(&value.to_string()),
    }
}

/// Canonical encode of one workspace reference (`{"kind","out","ws"}` on
/// the logical arm, `{"kind","out","ws"}` with the native id on the
/// native arm).
fn push_workspace_ref(out: &mut String, reference: &WorkspaceRef) {
    match reference {
        WorkspaceRef::Logical { output, workspace } => {
            out.push_str("{\"kind\":\"logical\",\"out\":");
            push_json_string(out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(out, &workspace.0);
        }
        WorkspaceRef::Native { output, workspace } => {
            out.push_str("{\"kind\":\"native\",\"out\":");
            push_json_string(out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(out, workspace.as_str());
        }
    }
    out.push('}');
}

/// Canonical encode of one nullable workspace reference.
fn push_opt_workspace_ref(out: &mut String, value: &Option<WorkspaceRef>) {
    match value {
        None => out.push_str("null"),
        Some(reference) => push_workspace_ref(out, reference),
    }
}

/// Checked canonical encode of one valid v4 member row.
fn encode_member_attempt_v4(row: &tiler_core::world_report::MemberAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"op\":");
    push_json_string(&mut out, &row.operation_id);
    out.push_str(",\"win\":");
    push_json_string(&mut out, &row.window.0);
    out.push_str(",\"out\":");
    push_json_string(&mut out, &row.target_output.0);
    out.push_str(",\"ws\":");
    push_json_string(&mut out, &row.target_workspace);
    out.push_str(",\"setter\":");
    push_json_string(&mut out, row.setter.as_str());
    out.push_str(",\"pre\":");
    push_opt_workspace_ref(&mut out, &row.pre);
    out.push_str(",\"post\":");
    push_opt_workspace_ref(&mut out, &row.post);
    out.push_str(",\"attemptMs\":\"");
    out.push_str(&row.attempt_ms.to_string());
    out.push_str("\"}");
    Ok(out)
}

/// Checked canonical encode of one valid v4 view row.
fn encode_view_attempt_v4(row: &tiler_core::world_report::ViewAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"op\":");
    push_json_string(&mut out, &row.operation_id);
    out.push_str(",\"out\":");
    push_json_string(&mut out, &row.target_output.0);
    out.push_str(",\"ws\":");
    push_json_string(&mut out, &row.target_workspace);
    out.push_str(",\"setter\":");
    push_json_string(&mut out, row.setter.as_str());
    out.push_str(",\"pre\":");
    push_opt_workspace_ref(&mut out, &row.pre);
    out.push_str(",\"post\":");
    push_opt_workspace_ref(&mut out, &row.post);
    out.push_str(",\"base\":");
    push_opt_workspace_ref(&mut out, &row.baseline);
    out.push_str(",\"attemptMs\":\"");
    out.push_str(&row.attempt_ms.to_string());
    out.push_str("\"}");
    Ok(out)
}

/// Checked canonical encode of one valid v4 clear row.
fn encode_clear_attempt_v4(row: &tiler_core::world_report::ClearAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"op\":");
    push_json_string(&mut out, &row.operation_id);
    out.push_str(",\"win\":");
    push_json_string(&mut out, &row.window.0);
    out.push_str(",\"setter\":");
    push_json_string(&mut out, row.setter.as_str());
    out.push_str(",\"pre\":");
    push_opt_bool(&mut out, &row.pre_maximized);
    out.push_str(",\"post\":");
    push_opt_bool(&mut out, &row.post_maximized);
    out.push_str(",\"attemptMs\":\"");
    out.push_str(&row.attempt_ms.to_string());
    out.push_str("\"}");
    Ok(out)
}

/// Checked canonical encode of one valid v4 creation row. The receipt key
/// spelling rides `receipt`: `Absent` omits the key, every other spelling
/// carries it, and the carried id always comes from the row. A receipt
/// spelling inconsistent with the carried value refuses.
fn encode_creation_attempt_v4(
    row: &tiler_core::world_report::NativeCreationAttempt,
    receipt: CreationReceiptWire,
) -> Result<String> {
    match (receipt, &row.created_native) {
        (
            CreationReceiptWire::Absent | CreationReceiptWire::Null | CreationReceiptWire::Empty,
            None,
        ) => {}
        (CreationReceiptWire::Id, Some(_)) => {}
        _ => return Err(snapshot_invalid("creation-receipt-inconsistent")),
    }
    let mut out = String::new();
    out.push_str("{\"op\":");
    push_json_string(&mut out, &row.operation_id);
    out.push_str(",\"ws\":");
    push_json_string(&mut out, &row.logical_workspace);
    out.push_str(",\"out\":");
    push_json_string(&mut out, &row.target_output.0);
    out.push_str(",\"setter\":");
    push_json_string(&mut out, row.setter.as_str());
    out.push_str(",\"before\":[");
    for (i, id) in row.order_before.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, id);
    }
    out.push_str("],\"after\":[");
    for (i, id) in row.order_after.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, id);
    }
    out.push(']');
    match receipt {
        CreationReceiptWire::Absent => {}
        CreationReceiptWire::Null => out.push_str(",\"created\":null"),
        CreationReceiptWire::Empty => out.push_str(",\"created\":\"\""),
        CreationReceiptWire::Id => {
            out.push_str(",\"created\":");
            push_json_string(
                &mut out,
                row.created_native.as_deref().expect("id receipt carries"),
            );
        }
    }
    out.push_str(",\"attemptMs\":\"");
    out.push_str(&row.attempt_ms.to_string());
    out.push_str("\"}");
    Ok(out)
}

/// Canonical encode of one visibility sample.
fn push_visibility_sample(out: &mut String, sample: &VisibilitySample) {
    out.push_str("{\"obs\":\"");
    out.push_str(&sample.obs.to_string());
    out.push_str("\",\"lifetimeMatched\":");
    push_opt_bool(out, &sample.lifetime_matched);
    out.push_str(",\"visibility\":{\"visible\":");
    push_opt_bool(out, &sample.visibility.visible);
    out.push_str(",\"suppressionOwned\":");
    push_opt_bool(out, &sample.visibility.suppression_owned);
    out.push_str("},\"minimized\":");
    push_opt_bool(out, &sample.minimized);
    out.push_str(",\"hostHidden\":");
    push_opt_bool(out, &sample.host_hidden);
    out.push('}');
}

/// Canonical encode of one arrival bracket: the location key rides the
/// carried arm only (`home` for native, `out` for output).
fn push_arrival_bracket(out: &mut String, bracket: &ArrivalBracket) {
    out.push_str("{\"win\":");
    push_json_string(out, &bracket.window.0);
    out.push_str(",\"obs\":\"");
    out.push_str(&bracket.obs.to_string());
    out.push_str("\",\"before\":");
    push_visibility_sample(out, &bracket.before);
    out.push_str(",\"kind\":\"");
    out.push_str(bracket.location.kind_str());
    out.push('"');
    match &bracket.location {
        ArrivalLocation::Native { home } => {
            out.push_str(",\"home\":");
            push_opt_workspace_ref(out, home);
        }
        ArrivalLocation::Output { out: output } => {
            out.push_str(",\"out\":");
            match output {
                None => out.push_str("null"),
                Some(output) => push_json_string(out, &output.0),
            }
        }
    }
    out.push_str(",\"frame\":");
    push_opt_rect(out, &bracket.frame);
    out.push_str(",\"after\":");
    push_visibility_sample(out, &bracket.after);
    out.push('}');
}

/// Checked canonical encode of one state row.
fn encode_state_attempt(row: &tiler_core::world_report::StateAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"win\":");
    push_json_string(&mut out, &row.window.0);
    out.push_str(",\"property\":");
    push_json_string(&mut out, row.property.as_str());
    out.push_str(",\"target\":");
    out.push_str(if row.target { "true" } else { "false" });
    out.push_str(",\"rev\":\"");
    out.push_str(&row.revision.to_string());
    out.push_str("\",\"setter\":");
    push_json_string(&mut out, row.setter.as_str());
    out.push_str(",\"pre\":");
    push_opt_bool(&mut out, &row.pre);
    out.push_str(",\"post\":");
    push_opt_bool(&mut out, &row.post);
    out.push_str(",\"attemptMs\":\"");
    out.push_str(&row.attempt_ms.to_string());
    out.push_str("\"}");
    Ok(out)
}

/// Checked canonical encode of one focus-feedback row.
fn encode_focus_feedback(row: &FocusFeedback) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"id\":");
    push_json_string(&mut out, &row.id);
    out.push_str(",\"target\":");
    push_json_string(&mut out, &row.target.0);
    out.push_str(",\"rev\":\"");
    out.push_str(&row.revision.to_string());
    out.push_str("\",\"setter\":");
    push_json_string(&mut out, row.setter.as_str());
    out.push_str(",\"reason\":");
    push_json_string(&mut out, row.reason.as_str());
    out.push_str(",\"pre\":");
    push_opt_window(&mut out, &row.pre);
    out.push_str(",\"post\":");
    push_opt_window(&mut out, &row.post);
    out.push_str(",\"foregroundProven\":");
    out.push_str(if row.foreground_proven {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"arrival\":[");
    for (i, bracket) in row.arrival.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_arrival_bracket(&mut out, bracket);
    }
    out.push_str("],\"attemptMs\":\"");
    out.push_str(&row.attempt_ms.to_string());
    out.push_str("\"}");
    Ok(out)
}

/// Canonical encode of one nullable window lifetime.
fn push_opt_window(out: &mut String, value: &Option<WindowId>) {
    match value {
        None => out.push_str("null"),
        Some(window) => push_json_string(out, &window.0),
    }
}

/// Canonical encode of one nullable output id.
fn push_opt_output(out: &mut String, value: &Option<OutputId>) {
    match value {
        None => out.push_str("null"),
        Some(output) => push_json_string(out, &output.0),
    }
}

/// Checked canonical encode of one gesture-ingress object.
fn encode_gesture_ingress_v4(row: &tiler_core::gesture::GestureIngress) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"win\":");
    push_json_string(&mut out, &row.window.0);
    out.push_str(",\"kind\":");
    push_json_string(&mut out, row.kind.as_str());
    out.push_str(",\"phase\":");
    push_json_string(&mut out, row.phase.as_str());
    out.push_str(",\"id\":\"");
    out.push_str(&row.id.to_string());
    out.push_str("\",\"cursor\":");
    push_opt_point(&mut out, &row.cursor);
    out.push_str(",\"startCursor\":");
    push_opt_point(&mut out, &row.start_cursor);
    out.push_str(",\"esc\":");
    out.push_str(if row.esc { "true" } else { "false" });
    out.push_str(",\"modal\":");
    out.push_str(if row.modal_move { "true" } else { "false" });
    out.push_str(",\"start\":");
    push_rect(&mut out, &row.start_frame);
    out.push('}');
    Ok(out)
}

/// Canonical encode of one nullable cursor point.
fn push_opt_point(out: &mut String, value: &Option<(i32, i32)>) {
    match value {
        None => out.push_str("null"),
        Some((x, y)) => {
            out.push('[');
            out.push_str(&x.to_string());
            out.push(',');
            out.push_str(&y.to_string());
            out.push(']');
        }
    }
}

/// Canonical encode of one send source (`{"explicit":S}` or
/// `{"implicit":"home-then-action"}`).
fn push_send_source(out: &mut String, source: &tiler_core::world_report::SendSource) {
    match source {
        tiler_core::world_report::SendSource::Explicit(output) => {
            out.push_str("{\"explicit\":");
            push_json_string(out, &output.0);
            out.push('}');
        }
        tiler_core::world_report::SendSource::HomeThenAction => {
            out.push_str("{\"implicit\":\"home-then-action\"}");
        }
    }
}

/// Checked canonical encode of one v4 command. Only the v4-decodable
/// shapes encode: the removed `switch-view`, a nonempty v4 focus
/// direction, and unpromoted core operations refuse as `command-invalid`
/// instead of emitting unknown wire shapes.
fn encode_command_v4(command: &WorldCommand) -> Result<String> {
    let mut out = String::new();
    // Execution-time subjects encode the identical op shape with the
    // closed `{"currentFocus":true}` subject; every other field rides verbatim.
    if let WorldCommand::Implicit(inner) = command {
        return encode_implicit_command_v4(inner);
    }
    match command {
        WorldCommand::Focus { window, direction } => {
            if !direction.is_empty() {
                return Err(snapshot_invalid("command-invalid"));
            }
            out.push_str("{\"op\":\"focus\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push('}');
        }
        WorldCommand::Send {
            window,
            target_output,
            target_workspace,
            follow,
        } => {
            out.push_str("{\"op\":\"send\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push_str(",\"out\":");
            push_json_string(&mut out, &target_output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, target_workspace);
            out.push_str(",\"follow\":");
            out.push_str(if *follow { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::SwitchView { .. } => return Err(snapshot_invalid("command-invalid")),
        WorldCommand::SelectWorkspace { output, workspace } => {
            out.push_str("{\"op\":\"select-workspace\",\"out\":");
            push_json_string(&mut out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, workspace);
            out.push('}');
        }
        WorldCommand::SelectNumbered { output, index } => {
            out.push_str("{\"op\":\"select-numbered\",\"out\":");
            push_opt_output(&mut out, output);
            out.push_str(",\"index\":");
            out.push_str(&index.to_string());
            out.push('}');
        }
        WorldCommand::SelectRelative { output, delta } => {
            out.push_str("{\"op\":\"select-relative\",\"out\":");
            push_opt_output(&mut out, output);
            out.push_str(",\"delta\":");
            out.push_str(&delta.to_string());
            out.push('}');
        }
        WorldCommand::SelectPrevious { output } => {
            out.push_str("{\"op\":\"select-previous\",\"out\":");
            push_opt_output(&mut out, output);
            out.push('}');
        }
        WorldCommand::SelectTrailing { output } => {
            out.push_str("{\"op\":\"select-trailing\",\"out\":");
            push_opt_output(&mut out, output);
            out.push('}');
        }
        WorldCommand::SendRelative {
            window,
            source,
            delta,
            follow,
        } => {
            out.push_str("{\"op\":\"send-relative\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push_str(",\"source\":");
            push_send_source(&mut out, source);
            out.push_str(",\"delta\":");
            out.push_str(&delta.to_string());
            out.push_str(",\"follow\":");
            out.push_str(if *follow { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::SendNumbered {
            window,
            source,
            index,
            follow,
        } => {
            out.push_str("{\"op\":\"send-numbered\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push_str(",\"source\":");
            push_send_source(&mut out, source);
            out.push_str(",\"index\":");
            out.push_str(&index.to_string());
            out.push_str(",\"follow\":");
            out.push_str(if *follow { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::SetWorkspaceTiling {
            output,
            workspace,
            tiled,
        } => {
            out.push_str("{\"op\":\"set-workspace-tiling\",\"out\":");
            push_json_string(&mut out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, workspace);
            out.push_str(",\"tiled\":");
            out.push_str(if *tiled { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::ToggleWorkspaceTiling { output, workspace } => {
            out.push_str("{\"op\":\"toggle-workspace-tiling\",\"out\":");
            push_json_string(&mut out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, workspace);
            out.push('}');
        }
        WorldCommand::MigrateWorkspace {
            source_output,
            target_output,
            workspace,
        } => {
            out.push_str("{\"op\":\"migrate-workspace\",\"from\":");
            push_json_string(&mut out, &source_output.0);
            out.push_str(",\"to\":");
            push_json_string(&mut out, &target_output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, workspace);
            out.push('}');
        }
        WorldCommand::FocusDirectional { window, direction } => {
            out.push_str("{\"op\":\"focus-directional\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push_str(",\"direction\":");
            push_json_string(&mut out, direction);
            out.push('}');
        }
        WorldCommand::MoveDirectional {
            window,
            direction,
            same_axis,
        } => {
            out.push_str("{\"op\":\"move-directional\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push_str(",\"direction\":");
            push_json_string(&mut out, direction);
            out.push_str(",\"sameAxis\":");
            push_json_string(&mut out, same_axis.as_wire_str());
            out.push('}');
        }
        WorldCommand::SendDirectional {
            window,
            direction,
            follow,
        } => {
            out.push_str("{\"op\":\"send-directional\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push_str(",\"direction\":");
            push_json_string(&mut out, direction);
            out.push_str(",\"follow\":");
            out.push_str(if *follow { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::MigrateDirectional { direction } => {
            out.push_str("{\"op\":\"migrate-directional\",\"direction\":");
            push_json_string(&mut out, direction);
            out.push('}');
        }
        WorldCommand::ToggleMaximize { window } => {
            out.push_str("{\"op\":\"toggle-maximize\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push('}');
        }
        WorldCommand::SetSticky { window, value } => {
            out.push_str("{\"op\":\"set-sticky\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push_str(",\"value\":");
            out.push_str(if *value { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::ToggleSticky { window } => {
            out.push_str("{\"op\":\"toggle-sticky\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push('}');
        }
        WorldCommand::SetFullscreen { window, value } => {
            out.push_str("{\"op\":\"set-fullscreen\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push_str(",\"value\":");
            out.push_str(if *value { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::ToggleFullscreen { window } => {
            out.push_str("{\"op\":\"toggle-fullscreen\",\"win\":");
            push_json_string(&mut out, &window.0);
            out.push('}');
        }
        WorldCommand::Operation(core) => match core {
            CoreCommand::ToggleFloat { window, float_rect } => {
                out.push_str("{\"op\":\"toggle-float\",\"win\":");
                push_json_string(&mut out, window);
                out.push_str(",\"rect\":");
                push_opt_rect(&mut out, float_rect);
                out.push('}');
            }
            CoreCommand::ToggleOrientation { window } => {
                out.push_str("{\"op\":\"toggle-orientation\",\"win\":");
                push_json_string(&mut out, window);
                out.push('}');
            }
            CoreCommand::Resize {
                window,
                direction,
                mode,
                press_index,
            } => {
                out.push_str("{\"op\":\"resize\",\"win\":");
                push_json_string(&mut out, window);
                out.push_str(",\"direction\":");
                push_json_string(&mut out, direction);
                out.push_str(",\"mode\":");
                push_json_string(&mut out, mode);
                out.push_str(",\"press\":");
                out.push_str(&press_index.to_string());
                out.push('}');
            }
            _ => return Err(snapshot_invalid("command-invalid")),
        },
        WorldCommand::Implicit(_) => {
            unreachable!("implicit subjects encode through the current-focus path")
        }
    }
    Ok(out)
}

/// Canonical encode of one execution-time (`win:{"currentFocus":true}`)
/// v4 command: the identical op shape with the closed current-focus
/// subject. Windowless payloads (never a valid implicit spelling) refuse instead of emitting wrong-subject
/// bytes.
fn encode_implicit_command_v4(command: &WorldCommand) -> Result<String> {
    let mut out = String::new();
    match command {
        WorldCommand::Focus { direction, .. } => {
            if !direction.is_empty() {
                return Err(snapshot_invalid("command-invalid"));
            }
            out.push_str("{\"op\":\"focus\",\"win\":{\"currentFocus\":true}}");
        }
        WorldCommand::Send {
            target_output,
            target_workspace,
            follow,
            ..
        } => {
            out.push_str("{\"op\":\"send\",\"win\":{\"currentFocus\":true}");
            out.push_str(",\"out\":");
            push_json_string(&mut out, &target_output.0);
            out.push_str(",\"ws\":");
            push_json_string(&mut out, target_workspace);
            out.push_str(",\"follow\":");
            out.push_str(if *follow { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::SendRelative {
            source,
            delta,
            follow,
            ..
        } => {
            out.push_str("{\"op\":\"send-relative\",\"win\":{\"currentFocus\":true}");
            out.push_str(",\"source\":");
            push_send_source(&mut out, source);
            out.push_str(",\"delta\":");
            out.push_str(&delta.to_string());
            out.push_str(",\"follow\":");
            out.push_str(if *follow { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::SendNumbered {
            source,
            index,
            follow,
            ..
        } => {
            out.push_str("{\"op\":\"send-numbered\",\"win\":{\"currentFocus\":true}");
            out.push_str(",\"source\":");
            push_send_source(&mut out, source);
            out.push_str(",\"index\":");
            out.push_str(&index.to_string());
            out.push_str(",\"follow\":");
            out.push_str(if *follow { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::FocusDirectional { direction, .. } => {
            out.push_str("{\"op\":\"focus-directional\",\"win\":{\"currentFocus\":true}");
            out.push_str(",\"direction\":");
            push_json_string(&mut out, direction);
            out.push('}');
        }
        WorldCommand::MoveDirectional {
            direction,
            same_axis,
            ..
        } => {
            out.push_str("{\"op\":\"move-directional\",\"win\":{\"currentFocus\":true}");
            out.push_str(",\"direction\":");
            push_json_string(&mut out, direction);
            out.push_str(",\"sameAxis\":");
            push_json_string(&mut out, same_axis.as_wire_str());
            out.push('}');
        }
        WorldCommand::SendDirectional {
            direction, follow, ..
        } => {
            out.push_str("{\"op\":\"send-directional\",\"win\":{\"currentFocus\":true}");
            out.push_str(",\"direction\":");
            push_json_string(&mut out, direction);
            out.push_str(",\"follow\":");
            out.push_str(if *follow { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::ToggleMaximize { .. } => {
            out.push_str("{\"op\":\"toggle-maximize\",\"win\":{\"currentFocus\":true}}");
        }
        WorldCommand::SetSticky { value, .. } => {
            out.push_str("{\"op\":\"set-sticky\",\"win\":{\"currentFocus\":true}");
            out.push_str(",\"value\":");
            out.push_str(if *value { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::ToggleSticky { .. } => {
            out.push_str("{\"op\":\"toggle-sticky\",\"win\":{\"currentFocus\":true}}");
        }
        WorldCommand::SetFullscreen { value, .. } => {
            out.push_str("{\"op\":\"set-fullscreen\",\"win\":{\"currentFocus\":true}");
            out.push_str(",\"value\":");
            out.push_str(if *value { "true" } else { "false" });
            out.push('}');
        }
        WorldCommand::ToggleFullscreen { .. } => {
            out.push_str("{\"op\":\"toggle-fullscreen\",\"win\":{\"currentFocus\":true}}");
        }
        WorldCommand::Operation(core) => match core {
            CoreCommand::ToggleFloat { float_rect, .. } => {
                out.push_str("{\"op\":\"toggle-float\",\"win\":{\"currentFocus\":true}");
                out.push_str(",\"rect\":");
                push_opt_rect(&mut out, float_rect);
                out.push('}');
            }
            CoreCommand::ToggleOrientation { .. } => {
                out.push_str("{\"op\":\"toggle-orientation\",\"win\":{\"currentFocus\":true}}");
            }
            CoreCommand::Resize {
                direction,
                mode,
                press_index,
                ..
            } => {
                out.push_str("{\"op\":\"resize\",\"win\":{\"currentFocus\":true}");
                out.push_str(",\"direction\":");
                push_json_string(&mut out, direction);
                out.push_str(",\"mode\":");
                push_json_string(&mut out, mode);
                out.push_str(",\"press\":");
                out.push_str(&press_index.to_string());
                out.push('}');
            }
            _ => return Err(snapshot_invalid("command-invalid")),
        },
        _ => return Err(snapshot_invalid("command-invalid")),
    }
    Ok(out)
}

/// Checked canonical encode of one decoded v4 report: compact JSON in
/// contract table key order that re-decodes to the input. Optional groups
/// ride their carried spellings from the wire metadata; carried attempt
/// rows without their lane, a nonzero clock without its key, a cursor hit
/// or gesture ingress inconsistent with its spelling, mismatched creation
/// receipts, unmergeable held ordinals, and unencodable commands refuse
/// instead of emitting non-wire bytes.
pub fn encode_report_v4(decoded: &DecodedReportV4) -> Result<String> {
    let report = &decoded.report;
    let wire = &decoded.wire;
    if report.command_seq.is_none() != report.command.is_none() {
        return Err(snapshot_invalid("command-pair-invalid"));
    }
    if report.renderer_input.is_some() && report.command.is_none() {
        return Err(snapshot_invalid("command-pair-invalid"));
    }
    match (&report.cursor_hit, wire.cursor_hit) {
        (None, CursorHitWire::Absent | CursorHitWire::Null | CursorHitWire::Empty) => {}
        (Some(_), CursorHitWire::Id) => {}
        _ => return Err(snapshot_invalid("cursor-hit-inconsistent")),
    }
    if report.monotonic_ms != 0 && !wire.monotonic_ms_present {
        return Err(snapshot_invalid("monotonic-inconsistent"));
    }
    match (&report.gesture, wire.gesture_ingress) {
        (None, GestureIngressWire::Absent | GestureIngressWire::Null) => {}
        (Some(_), GestureIngressWire::Present) => {}
        _ => return Err(snapshot_invalid("gesture-ingress-inconsistent")),
    }
    if !report.foreground_known && !report.focused_global.is_empty() {
        return Err(snapshot_invalid("focused-unknown-mismatch"));
    }
    let held_in_lane = |lane: AttemptLaneV4| {
        decoded
            .report
            .held_attempts
            .iter()
            .filter(move |held| held.lane == lane)
            .count()
    };
    for (valid, lane, present) in [
        (
            report.frames.len(),
            AttemptLaneV4::Frame,
            wire.frame_attempts_present,
        ),
        (
            report.member_attempts.len(),
            AttemptLaneV4::Member,
            wire.member_attempts_present,
        ),
        (
            report.view_attempts.len(),
            AttemptLaneV4::View,
            wire.view_attempts_present,
        ),
        (
            report.clear_attempts.len(),
            AttemptLaneV4::Clear,
            wire.clear_attempts_present,
        ),
        (
            report.creation_attempts.len(),
            AttemptLaneV4::Creation,
            wire.creation_attempts_present,
        ),
    ] {
        if (valid > 0 || held_in_lane(lane) > 0) && !present {
            return Err(snapshot_invalid("attempt-lane-inconsistent"));
        }
    }
    if decoded.creation_receipts.len() != report.creation_attempts.len() {
        return Err(snapshot_invalid("creation-receipt-mismatch"));
    }
    let mut out = String::new();
    out.push_str("{\"v\":");
    out.push_str(&report.contract_version.to_string());
    out.push_str(",\"adapter\":");
    push_json_string(&mut out, &report.adapter_incarnation);
    out.push_str(",\"gen\":");
    push_json_string(&mut out, &report.core_generation);
    out.push_str(",\"obs\":\"");
    out.push_str(&report.observation_seq.to_string());
    out.push_str("\",\"reqSeq\":\"");
    out.push_str(&report.request_seq.to_string());
    out.push_str("\",\"req\":");
    push_json_string(&mut out, &report.request_id);
    out.push_str(",\"cmdSeq\":");
    match report.command_seq {
        None => out.push_str("null"),
        Some(seq) => {
            out.push('"');
            out.push_str(&seq.to_string());
            out.push('"');
        }
    }
    out.push_str(",\"command\":");
    match &report.command {
        None => out.push_str("null"),
        Some(command) => out.push_str(&encode_command_v4(command)?),
    }
    out.push_str(",\"screens\":[");
    let mut screens = Vec::with_capacity(report.screens.len());
    for screen in &report.screens {
        screens.push(
            tiler_core::world_report::encode_screen_canonical(screen)
                .map_err(|_| snapshot_invalid("nonfinite"))?,
        );
    }
    let mut held_screens = Vec::with_capacity(report.held_screens.len());
    for held in &report.held_screens {
        held_screens.push((
            held.ordinal,
            tiler_core::world_report::encode_held_screen_canonical(held)
                .map_err(|_| snapshot_invalid("nonfinite"))?,
        ));
    }
    for (i, row) in merge_lane(screens, held_screens)?.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(row);
    }
    out.push_str("],\"windows\":[");
    let mut windows = Vec::with_capacity(report.windows.len());
    for fact in &report.windows {
        windows.push(
            tiler_core::world_report::encode_window_canonical(fact)
                .map_err(|_| snapshot_invalid("nonfinite"))?,
        );
    }
    let mut held_windows = Vec::with_capacity(report.held_windows.len());
    for held in &report.held_windows {
        held_windows.push((
            held.ordinal,
            tiler_core::world_report::encode_held_window_canonical(held)
                .map_err(|_| snapshot_invalid("nonfinite"))?,
        ));
    }
    for (i, row) in merge_lane(windows, held_windows)?.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(row);
    }
    out.push_str("],\"closed\":[");
    for (i, id) in report.closed.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, &id.0);
    }
    out.push_str("],\"native\":[");
    let mut native = Vec::with_capacity(report.native.len());
    for row in &report.native {
        native.push(encode_native_observation_v4(row)?);
    }
    let mut held_native = Vec::with_capacity(report.held_native.len());
    for held in &report.held_native {
        held_native.push((held.ordinal, encode_held_fields(&held.fields)?));
    }
    for (i, row) in merge_lane(native, held_native)?.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(row);
    }
    out.push_str("],\"focused\":");
    push_json_string(&mut out, &report.focused_global);
    out.push_str(",\"foregroundKnown\":");
    out.push_str(if report.foreground_known {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"complete\":");
    out.push_str(if report.inventory_complete {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"gesture\":");
    out.push_str(if report.gesture_barrier {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"cursor\":");
    push_opt_point(&mut out, &report.cursor);
    out.push_str(",\"actionForeground\":");
    push_opt_output(&mut out, &report.action_foreground);
    out.push_str(",\"actionCached\":");
    push_opt_output(&mut out, &report.action_cached);
    out.push_str(",\"actionOutputs\":[");
    for (i, output) in report.action_outputs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, &output.0);
    }
    out.push_str("],\"presentation\":");
    match &report.presentation {
        PresentationRowV4::Valid(ingress) => {
            out.push_str(&encode_presentation_ingress_v4(ingress)?);
        }
        PresentationRowV4::Held(held) => out.push_str(&encode_held_fields(&held.fields)?),
    }
    match (&report.cursor_hit, wire.cursor_hit) {
        (None, CursorHitWire::Absent) => {}
        (None, CursorHitWire::Null) => out.push_str(",\"cursorHit\":null"),
        (None, CursorHitWire::Empty) => out.push_str(",\"cursorHit\":\"\""),
        (Some(hit), CursorHitWire::Id) => {
            out.push_str(",\"cursorHit\":");
            push_json_string(&mut out, &hit.0);
        }
        _ => return Err(snapshot_invalid("cursor-hit-inconsistent")),
    }
    if wire.monotonic_ms_present {
        out.push_str(",\"monotonicMs\":\"");
        out.push_str(&report.monotonic_ms.to_string());
        out.push('"');
    }
    emit_attempt_lane(
        &mut out,
        "frameAttempts",
        wire.frame_attempts_present,
        merge_lane(
            report
                .frames
                .iter()
                .map(encode_frame_attempt_v4)
                .collect::<Result<Vec<_>>>()?,
            report
                .held_attempts
                .iter()
                .filter(|held| held.lane == AttemptLaneV4::Frame)
                .map(|held| encode_held_fields(&held.fields).map(|bytes| (held.ordinal, bytes)))
                .collect::<Result<Vec<_>>>()?,
        )?,
    );
    emit_attempt_lane(
        &mut out,
        "memberAttempts",
        wire.member_attempts_present,
        merge_lane(
            report
                .member_attempts
                .iter()
                .map(encode_member_attempt_v4)
                .collect::<Result<Vec<_>>>()?,
            report
                .held_attempts
                .iter()
                .filter(|held| held.lane == AttemptLaneV4::Member)
                .map(|held| encode_held_fields(&held.fields).map(|bytes| (held.ordinal, bytes)))
                .collect::<Result<Vec<_>>>()?,
        )?,
    );
    emit_attempt_lane(
        &mut out,
        "viewAttempts",
        wire.view_attempts_present,
        merge_lane(
            report
                .view_attempts
                .iter()
                .map(encode_view_attempt_v4)
                .collect::<Result<Vec<_>>>()?,
            report
                .held_attempts
                .iter()
                .filter(|held| held.lane == AttemptLaneV4::View)
                .map(|held| encode_held_fields(&held.fields).map(|bytes| (held.ordinal, bytes)))
                .collect::<Result<Vec<_>>>()?,
        )?,
    );
    emit_attempt_lane(
        &mut out,
        "clearAttempts",
        wire.clear_attempts_present,
        merge_lane(
            report
                .clear_attempts
                .iter()
                .map(encode_clear_attempt_v4)
                .collect::<Result<Vec<_>>>()?,
            report
                .held_attempts
                .iter()
                .filter(|held| held.lane == AttemptLaneV4::Clear)
                .map(|held| encode_held_fields(&held.fields).map(|bytes| (held.ordinal, bytes)))
                .collect::<Result<Vec<_>>>()?,
        )?,
    );
    emit_attempt_lane(
        &mut out,
        "creationAttempts",
        wire.creation_attempts_present,
        merge_lane(
            report
                .creation_attempts
                .iter()
                .zip(decoded.creation_receipts.iter())
                .map(|(row, receipt)| encode_creation_attempt_v4(row, *receipt))
                .collect::<Result<Vec<_>>>()?,
            report
                .held_attempts
                .iter()
                .filter(|held| held.lane == AttemptLaneV4::Creation)
                .map(|held| encode_held_fields(&held.fields).map(|bytes| (held.ordinal, bytes)))
                .collect::<Result<Vec<_>>>()?,
        )?,
    );
    match (&report.gesture, wire.gesture_ingress) {
        (None, GestureIngressWire::Absent) => {}
        (None, GestureIngressWire::Null) => out.push_str(",\"gestureIngress\":null"),
        (Some(ingress), GestureIngressWire::Present) => {
            out.push_str(",\"gestureIngress\":");
            out.push_str(&encode_gesture_ingress_v4(ingress)?);
        }
        _ => return Err(snapshot_invalid("gesture-ingress-inconsistent")),
    }
    if let Some(primary) = &report.primary_output {
        out.push_str(",\"primary\":");
        push_json_string(&mut out, &primary.0);
    }
    out.push_str(",\"intentRecords\":[");
    for (i, record) in report.intent_records.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"win\":");
        push_json_string(&mut out, &record.win.0);
        out.push_str(",\"data\":");
        push_json_string(&mut out, &record.data);
        out.push('}');
    }
    out.push_str("],\"stateAttempts\":[");
    for (i, row) in report.state_attempts.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&encode_state_attempt(row)?);
    }
    out.push_str("],\"focusFeedback\":[");
    for (i, row) in report.focus_feedback.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&encode_focus_feedback(row)?);
    }
    out.push_str("],\"rendererInput\":");
    match &report.renderer_input {
        None => out.push_str("null"),
        Some(input) => out.push_str(&encode_renderer_input(input)?),
    }
    out.push('}');
    let back = match decode_report_v4(&out) {
        Ok(back) => back,
        Err(_) => return Err(snapshot_invalid("encode-mismatch")),
    };
    if back != *decoded {
        return Err(snapshot_invalid("encode-mismatch"));
    }
    Ok(out)
}

/// Emit one optional attempt lane (`,"lane":[...]`) when the lane was
/// carried, including the empty-array spelling.
fn emit_attempt_lane(out: &mut String, lane: &str, present: bool, rows: Vec<String>) {
    if !present {
        return;
    }
    out.push_str(",\"");
    out.push_str(lane);
    out.push_str("\":[");
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(row);
    }
    out.push(']');
}

/// Canonical JSON string writer for replies and errors: `"` and `\` escape,
/// C0 controls become `lcase \u00XX`, every other char passes through.
/// Byte-identical to the core canonical writer over the ASCII-validated
/// wire alphabet.
fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\0'..='\u{1f}' => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let byte = c as u8;
                out.push_str("\\u00");
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 0x0F)]));
            }
            _ => out.push(c),
        }
    }
    out.push('"');
}

/// Protocol-owned exact reply encoding, checked independently of the core
/// (core byte count, roundtrips, and immutable literal bytes); the core commit
/// gate and this transport gate cannot differ.
fn push_authority_scope(out: &mut String, scope: &AuthorityScope) {
    match scope {
        AuthorityScope::Output(output) => {
            out.push_str("{\"kind\":\"output\",\"out\":");
            push_json_string(out, &output.0);
            out.push('}');
        }
        AuthorityScope::Workspace { output, workspace } => {
            out.push_str("{\"kind\":\"workspace\",\"out\":");
            push_json_string(out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(out, &workspace.0);
            out.push('}');
        }
    }
}

/// Checked canonical encode of one decoded v4 reply. This is the sole
/// public reply encode: the genuine v4 codec ([`encode_reply_v4`]) with
/// strict v4 shape enforcement. No legacy dual encode.
pub fn encode_reply(decoded: &DecodedReplyV4) -> Result<String> {
    encode_reply_v4(decoded)
}

/// Retained engine surface for the service boundary: the planner service
/// drives [`WorldEngine`] through this module so only `tiler-protocol` sits
/// between the service and the core.
pub use tiler_core::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u64_decimal_rejects_floats_and_formats() {
        assert_eq!(parse_u64_decimal("0"), Some(0));
        assert_eq!(parse_u64_decimal("7"), Some(7));
        assert_eq!(parse_u64_decimal(&u64::MAX.to_string()), Some(u64::MAX));
        assert_eq!(parse_u64_decimal(""), None);
        assert_eq!(parse_u64_decimal("007"), None);
        assert_eq!(parse_u64_decimal("-1"), None);
        assert_eq!(parse_u64_decimal("+1"), None);
        assert_eq!(parse_u64_decimal(" 1"), None);
        assert_eq!(parse_u64_decimal("18446744073709551616"), None);
        assert_eq!(parse_u64_decimal("99999999999999999999"), None);
    }

    #[test]
    fn error_body_is_bounded_without_payload() {
        let body = encode_error("generation-mismatch", "message", "gen-1");
        assert!(body.len() < 512);
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(parsed["outcome"], "rejected");
        assert_eq!(parsed["gen"], "gen-1");
    }

    #[test]
    fn switch_settled_v4_writer_emits_exact_bytes() {
        use tiler_core::directional::WindowId;
        use tiler_core::world_report::{
            BorderPresentationV4, SwitchEvent, SwitchFocus, SwitchOutcome,
        };
        let settled = vec![
            SwitchEvent {
                operation_id: "op-3".to_owned(),
                outcome: SwitchOutcome::Complete,
                failed: Vec::new(),
                focus: SwitchFocus::Proven,
                fallback: false,
            },
            SwitchEvent {
                operation_id: format!("op-{}", u64::MAX),
                outcome: SwitchOutcome::CompletedWithFailures,
                failed: vec![WindowId("w".repeat(128))],
                focus: SwitchFocus::Unproven,
                fallback: true,
            },
        ];
        let mut core_section = String::new();
        tiler_core::world_sizing::encode_switch_settled(&mut core_section, &settled);
        let reply = tiler_core::world_report::WorldReplyV4 {
            contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            request_id: "req-1".to_owned(),
            based_on_observation_seq: 4,
            state_revision: 9,
            ready: true,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            close_focus: None,
            command_result: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            presentation: tiler_core::world_report::WorldPresentationV4 {
                foreground: String::new(),
                rev: 0,
                suspended_output: None,
                border: BorderPresentationV4 {
                    visible: false,
                    reason: None,
                    subject: None,
                    outsets: None,
                    width: None,
                    radius: None,
                    witness: None,
                },
                underlay: tiler_core::world_presentation::PresentationUnderlay::default(),
                pager: Vec::new(),
            },
            intent_writes: Vec::new(),
            switch_settled: settled.clone(),
            diagnostics: Vec::new(),
        };
        let decoded = DecodedReplyV4 {
            reply,
            wire: ReplyWireOptionals {
                suspended_output_present: false,
            },
        };
        let wire = encode_reply_v4(&decoded).expect("reply encodes");
        assert_eq!(
            wire.len(),
            tiler_core::world_sizing_v4::reply_exact_bytes_v4(&decoded.reply),
            "core byte count is exact over the event reply"
        );
        assert_eq!(
            decode_reply_v4(&wire).expect("reply round-trips"),
            decoded,
            "wire round-trips to the identical typed reply and metadata"
        );
        assert!(
            tiler_core::world_sizing_v4::reply_estimate_v4(&decoded.reply) >= wire.len(),
            "estimate covers event reply"
        );
        assert!(wire.contains("\"switchSettled\":[{"));
        assert!(wire.contains("\"outcome\":\"complete\""));
        assert!(wire.contains("\"outcome\":\"completed-with-failures\""));
    }

    /// Minimal v4 report envelope carrying one candidate command, mirroring
    /// the broker probe skeleton. Decode is shape-only: inventory-free, so
    /// any well-formed command spelling decodes here.
    fn rapid_report_with_command(command_json: &str) -> String {
        format!(
            "{{\"v\":4,\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\"obs\":\"1\",\
              \"reqSeq\":\"1\",\"req\":\"req-1\",\"monotonicMs\":\"1\",\"cmdSeq\":\"1\",\
              \"command\":{command_json},\"screens\":[],\"windows\":[],\"closed\":[],\
              \"native\":[],\"focused\":\"\",\"foregroundKnown\":false,\"complete\":true,\"gesture\":false,\
              \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
              \"actionOutputs\":[],\
              \"presentation\":{{\"foreground\":\"\",\"minimized\":null,\
              \"maximized\":null,\"fullscreen\":null,\"hostHidden\":null,\
              \"hiddenWorkspace\":null,\"frame\":null,\"chordHeld\":false,\
              \"moveWindow\":\"\",\"pagerOrder\":[],\"pagerFrames\":[],\
              \"witness\":null,\"foregroundOut\":null}},\
              \"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[],\
              \"rendererInput\":null}}"
        )
    }

    fn assert_current_focus_subject_accepted(op: &str, command_json: &str) {
        let decoded = decode_report_v4(&rapid_report_with_command(command_json));
        let decoded = decoded.unwrap_or_else(|error| {
            panic!(
                "op {op} must accept win:{{\"currentFocus\":true}} as the execution-time core-focus subject; got {error:?}"
            )
        });
        assert!(
            matches!(
                decoded.report.command,
                Some(tiler_core::world_report::WorldCommand::Implicit(_))
            ),
            "op {op} current-focus subject must decode to the implicit wrapper"
        );
        // The implicit spelling round-trips through the canonical encode
        // with the closed marker and decodes back to the wrapper.
        let reencoded = encode_report_v4(&decoded).expect("implicit command encodes");
        assert!(
            reencoded.contains("\"win\":{\"currentFocus\":true}"),
            "op {op} implicit encode must carry the current-focus subject"
        );
        let reparsed: serde_json::Value =
            serde_json::from_str(&reencoded).expect("canonical encode is JSON");
        let command_value = reparsed
            .get("command")
            .expect("encoded report carries the command");
        let round_tripped =
            decode_command_value_v4(command_value).expect("current-focus subject re-decodes");
        assert!(
            matches!(
                round_tripped,
                tiler_core::world_report::WorldCommand::Implicit(_)
            ),
            "op {op} round trip must preserve the implicit wrapper"
        );
    }

    #[test]
    fn rapid_input_current_focus_subject_focus_and_directional_send() {
        assert_current_focus_subject_accepted(
            "focus",
            r#"{"op":"focus","win":{"currentFocus":true}}"#,
        );
        assert_current_focus_subject_accepted(
            "send",
            r#"{"op":"send","win":{"currentFocus":true},"out":"out-a","ws":"ws-1","follow":false}"#,
        );
        assert_current_focus_subject_accepted(
            "send-relative",
            r#"{"op":"send-relative","win":{"currentFocus":true},"source":{"implicit":"home-then-action"},"delta":1,"follow":false}"#,
        );
        assert_current_focus_subject_accepted(
            "send-numbered",
            r#"{"op":"send-numbered","win":{"currentFocus":true},"source":{"explicit":"out-a"},"index":2,"follow":true}"#,
        );
        assert_current_focus_subject_accepted(
            "focus-directional",
            r#"{"op":"focus-directional","win":{"currentFocus":true},"direction":"right"}"#,
        );
        assert_current_focus_subject_accepted(
            "move-directional",
            r#"{"op":"move-directional","win":{"currentFocus":true},"direction":"left","sameAxis":"group-with-neighbor"}"#,
        );
        assert_current_focus_subject_accepted(
            "send-directional",
            r#"{"op":"send-directional","win":{"currentFocus":true},"direction":"left","follow":false}"#,
        );
    }

    #[test]
    fn rapid_input_current_focus_subject_resize_and_orientation() {
        assert_current_focus_subject_accepted(
            "resize",
            r#"{"op":"resize","win":{"currentFocus":true},"direction":"left","mode":"horizontal","press":1}"#,
        );
        assert_current_focus_subject_accepted(
            "toggle-orientation",
            r#"{"op":"toggle-orientation","win":{"currentFocus":true}}"#,
        );
        assert_current_focus_subject_accepted(
            "toggle-float",
            r#"{"op":"toggle-float","win":{"currentFocus":true},"rect":null}"#,
        );
    }

    #[test]
    fn rapid_input_current_focus_subject_state_toggles() {
        assert_current_focus_subject_accepted(
            "toggle-maximize",
            r#"{"op":"toggle-maximize","win":{"currentFocus":true}}"#,
        );
        assert_current_focus_subject_accepted(
            "set-fullscreen",
            r#"{"op":"set-fullscreen","win":{"currentFocus":true},"value":true}"#,
        );
        assert_current_focus_subject_accepted(
            "toggle-fullscreen",
            r#"{"op":"toggle-fullscreen","win":{"currentFocus":true}}"#,
        );
        assert_current_focus_subject_accepted(
            "set-sticky",
            r#"{"op":"set-sticky","win":{"currentFocus":true},"value":true}"#,
        );
        assert_current_focus_subject_accepted(
            "toggle-sticky",
            r#"{"op":"toggle-sticky","win":{"currentFocus":true}}"#,
        );
    }

    #[test]
    fn rapid_input_current_focus_marker_malformed_refuses() {
        // The closed marker is exactly one key `currentFocus` with boolean
        // true: extra keys, false/null/wrong-type values, an empty object,
        // or a wrong-case key all refuse; null and missing subjects refuse
        // exactly as before (the historical null reject is preserved).
        for (label, command_json) in [
            (
                "false value",
                r#"{"op":"toggle-maximize","win":{"currentFocus":false}}"#,
            ),
            (
                "null value",
                r#"{"op":"toggle-maximize","win":{"currentFocus":null}}"#,
            ),
            ("missing key", r#"{"op":"toggle-maximize","win":{}}"#),
            (
                "extra key",
                r#"{"op":"toggle-maximize","win":{"currentFocus":true,"extra":1}}"#,
            ),
            (
                "string value",
                r#"{"op":"toggle-maximize","win":{"currentFocus":"true"}}"#,
            ),
            (
                "number value",
                r#"{"op":"toggle-maximize","win":{"currentFocus":1}}"#,
            ),
            (
                "wrong-case key",
                r#"{"op":"toggle-maximize","win":{"currentfocus":true}}"#,
            ),
            ("null subject", r#"{"op":"toggle-maximize","win":null}"#),
            (
                "null focus-directional subject",
                r#"{"op":"focus-directional","win":null,"direction":"right"}"#,
            ),
        ] {
            assert!(
                decode_report_v4(&rapid_report_with_command(command_json)).is_err(),
                "{label} marker shape stays refused"
            );
        }
        // Windowless payloads never ride the marker: a windowless op with a
        // marker-shaped win refuses.
        assert!(
            decode_report_v4(&rapid_report_with_command(
                r#"{"op":"migrate-directional","win":{"currentFocus":true},"direction":"left"}"#,
            ))
            .is_err(),
            "marker on a windowless command stays refused"
        );
    }

    #[test]
    fn rapid_input_explicit_subjects_keep_meaning() {
        // Explicit strings still decode to by-id subjects; the empty
        // focus-directional win keeps its genuine no-focus meaning; missing,
        // null, or empty subjects on other commands stay refused.
        let explicit = decode_request(&rapid_report_with_command(
            r#"{"op":"toggle-maximize","win":"win-a"}"#,
        ))
        .expect("explicit win still decodes");
        assert!(matches!(
            explicit.command,
            Some(tiler_core::world_report::WorldCommand::ToggleMaximize { .. })
        ));
        let no_focus = decode_request(&rapid_report_with_command(
            r#"{"op":"focus-directional","win":"","direction":"right"}"#,
        ))
        .expect("empty focus-directional win still decodes");
        assert!(matches!(
            no_focus.command,
            Some(tiler_core::world_report::WorldCommand::FocusDirectional { .. })
        ));
        assert!(
            decode_request(&rapid_report_with_command(
                r#"{"op":"focus-directional","direction":"right"}"#,
            ))
            .is_err(),
            "missing win stays refused"
        );
        assert!(
            decode_request(&rapid_report_with_command(
                r#"{"op":"focus-directional","win":null,"direction":"right"}"#,
            ))
            .is_err(),
            "null focus-directional win stays refused"
        );
        assert!(
            decode_request(&rapid_report_with_command(
                r#"{"op":"toggle-maximize","win":null}"#,
            ))
            .is_err(),
            "null win outside focus-directional stays refused"
        );
        assert!(
            decode_request(&rapid_report_with_command(
                r#"{"op":"toggle-maximize","win":""}"#,
            ))
            .is_err(),
            "empty win outside focus-directional stays refused"
        );
    }
}
