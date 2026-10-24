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
//!   before commit. [`encode_reply`] is byte-identical to
//!   [`tiler_core::world_report::encode_reply_canonical`] over every valid
//!   reply (ASCII-validated wire alphabet); the codec test locks the
//!   equality so the two gates cannot differ.
//!
//! Request keys (object, all required):
//! ```text
//! {v:2, owner, adapter, gen, obs:"u64", reqSeq:"u64", req, cmdSeq:"u64"|null,
//!  command:{op,...}|null, screens, windows, views, closed,
//!  hostOwned:bool, equalCount:bool, native, predicate, refill,
//!  defaultTiled:bool, focused:""|id, complete:bool, gesture:bool,
//!  switchTarget, freshPlacement, cursor:[x,y]|null,
//!  actionForeground:id|null, actionCached:id|null, actionOutputs:[id...],
//!  focusFollowsCursor:bool, cursorWarp:bool}
//! (plus optional tolerant D2/D3 settings: anchorFallback, noSpaceResult;
//!  missing or invalid values backfill the defaults)
//! screens[]: {out, full:[x,y,w,h]|null, work:[x,y,w,h]|null, gap, outer,
//!             complete:bool, primary:bool}
//! windows[] (facts): {win, out, rect:[x,y,w,h]|null, floating:bool,
//!             fitExcluded:bool, fullscreen:bool, maximized:bool, sticky:bool,
//!             fixedAuto:bool, fixedSuppress:bool, minimized:bool, hints:{...},
//!             observedWs:""|id|null, nativeWs:""|id|null (optional, tolerant:
//!             missing backfills null; capability-gated host-owned native
//!             desktop id, separate namespace from observedWs; without
//!             hostOwned it refuses)}
//! views[]: {out, view:""|id} (empty means unknown)
//! closed[]: [id...] (explicit closes)
//! native[] (host-owned observations, requires hostOwned): {out, order:[id...],
//!             current:""|id}
//! predicate: "both-axes-fixed"|"either-axis-fixed"
//! refill: "last-remaining-workspace"|"most-recently-used-workspace"
//! anchorFallback: "largest-splittable-window"|"closest-splittable-relative"|"no-search"
//!   (optional, tolerant: missing or invalid backfills the default)
//! noSpaceResult: "float"|"allow-overlap"
//!   (optional, tolerant: missing or invalid backfills the default)
//! switchTarget: "cursor-output"|"focused-output" (strict: unknown refuses)
//! freshPlacement: "cursor-output-last-focused"|"cursor-output-cursor-closest"|"focused-output-last-focused"
//!   (strict: unknown refuses)
//! cursor: [x,y] in adapter units (FULL-rect space) or null when unreadable
//! cursorHit: ""|id|null (optional, tolerant: missing backfills null) - the
//!   normalized cursor-hit managed lifetime: the exact managed window under
//!   the cursor at observation time, resolved natively. Absent, foreign, or
//!   hidden hits report null; the core never focuses without one.
//! actionForeground/actionCached: id|null (required) - the neutral
//!   migration/control action context: the adapter-grounded foreground
//!   output and the adapter-retained cache output (`None` when unknown).
//!   The core applies foreground, pointer, cache, then known-registry
//!   order; `migrate-directional` carries no source (direction only).
//! actionOutputs: [id...] (required, possibly empty) - the neutral
//!   ordered known-output registry in adapter registry order. Every
//!   action-context leg filters through it with first-entry fallback.
//! focusFollowsCursor/cursorWarp: bool (stage-4b core-actuated settings)
//! monotonicMs: "u64" adapter monotonic milliseconds, arbitrary process
//!   epoch, never wall-clock (optional, tolerant: missing backfills 0 =
//!   unreported, which never suppresses the transient lane)
//! frameAttempts[] (optional, tolerant: missing backfills []):
//!   {win, target:[x,y,w,h], setter:accepted|refused|threw,
//!    readback:[x,y,w,h]|null, dpiBefore:number|null, dpiAfter:number|null,
//!    attemptMs:"u64", op:""|id, pre:[x,y,w,h]|null} - mechanical
//!   write/readback facts only, never a policy verdict; at most one entry
//!   per window. `op` names the retained float-placement operation the
//!   write ran under ("" for ordinary writes, never inferred); `pre` is
//!   the live frame immediately before the write and requires `op`.
//! memberAttempts[] (optional, tolerant: missing backfills []):
//!   {op, win, out, ws, setter:accepted|refused|threw,
//!    pre:{out,ws}|null, post:{out,ws}|null, attemptMs:"u64"} -
//!   mechanical membership write/homing facts only; at most one entry per
//!   operation window. The core owns window/migration completion.
//! viewAttempts[] (optional, tolerant: missing backfills []):
//!   {op, out, ws, setter:accepted|refused|threw,
//!    pre:""|id|null, post:""|id|null, base:""|id|null,
//!    attemptMs:"u64"} - mechanical view write/proof facts only; at most
//!   one entry per owning view phase. The core owns every view verdict.
//! clearAttempts[] (optional, tolerant: missing backfills []):
//!   {op, win, setter:accepted|refused|threw,
//!    pre:bool|null, post:bool|null, attemptMs:"u64"} - mechanical
//!   maximize-clear write/proof facts only (immediate pre/post maximized
//!   states); at most one entry per retained clear operation. The core
//!   owns directional clear completion.
//! command.op: focus|send|switch-view|select-workspace|select-numbered|
//!   select-relative|select-previous|select-trailing|send-relative|
//!   send-numbered|set-workspace-tiling|
//!   migrate-workspace|focus-directional|move-directional|send-directional|
//!   migrate-directional|operation (fields per variant, see decode; every
//!   `ws` field names a stable core logical workspace id; the four
//!   selection commands carry `"out":null` for core-resolved output or an
//!   opaque id for a genuinely semantic explicit output)
//! ```
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
//! `warpCursor`: null or the desired `[x,y]` cursor-warp point
//! (cross-output focus only), plus `presentation`: the uniform
//! border/underlay/pager presentation (`foreground` token, `border` shape,
//! `underlay` found/hidden shape, `pager` content).
//! Bootstrap/discovery replies reuse the same shape with `ready:false` and
//! empty sections. Bounded errors reuse
//! `{v:2,outcome:"rejected",kind,message,gen}`.

use tiler_core::boundary::CoreCommand;
use tiler_core::bounds::{is_opaque_id, valid_carried_rect};
use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{MAX_CORRELATION_LEN, MAX_GENERATION_LEN, MAX_OWNER_LEN};
use tiler_core::world_report::{
    HoldReason, NativeObservation, PresentationIngress, ScreenReport, ScreenView, TransferKind,
    WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReply, WorldReport,
    validate_report_reason,
};

/// World protocol contract version (equals the core world contract version).
pub const WORLD_PROTOCOL_VERSION: u32 = WORLD_CONTRACT_VERSION;
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
    out.push_str("{\"v\":2,\"outcome\":\"rejected\",\"kind\":");
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

fn is_owner_text(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_OWNER_LEN
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
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
/// explicit output; `{"implicit": "home-then-action" |
/// "home-then-cached-live"}` selects the core-owned fallback chain. Any
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
                "home-then-cached-live" => Ok(SendSource::HomeThenCachedLive),
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

fn decode_fact(value: &serde_json::Value) -> Result<WindowFact> {
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &[
            "win",
            "out",
            "rect",
            "floating",
            "fitExcluded",
            "fullscreen",
            "maximized",
            "sticky",
            "fixedAuto",
            "fixedSuppress",
            "minimized",
            "hints",
            "observedWs",
        ],
        &["nativeWs"],
    )?;
    // A null rect is an explicitly unreadable frame: the staged scope
    // holds instead of authorizing geometry, never a silent absence.
    let rect = opt_rect_or_null(map, "rect")?;
    let native_workspace = match map.get("nativeWs") {
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
    };
    Ok(WindowFact {
        window: WindowId(req_id(map, "win")?),
        output: OutputId(req_id(map, "out")?),
        rect,
        floating: req_bool(map, "floating")?,
        fit_excluded: req_bool(map, "fitExcluded")?,
        fullscreen: req_bool(map, "fullscreen")?,
        maximized: req_bool(map, "maximized")?,
        sticky: req_bool(map, "sticky")?,
        fixed_auto: req_bool(map, "fixedAuto")?,
        fixed_suppress: req_bool(map, "fixedSuppress")?,
        hints: decode_hints(map.get("hints").ok_or_else(malformed)?)?,
        minimized: req_bool(map, "minimized")?,
        observed_workspace: opt_id_or_null(map, "observedWs")?,
        native_workspace,
    })
}

fn decode_screen(value: &serde_json::Value) -> Result<ScreenReport> {
    let map = as_object(value)?;
    check_keys(
        map,
        &["out", "full", "work", "gap", "outer", "complete", "primary"],
    )?;
    // Null rectangles stay representable for explicitly unreadable or
    // partial observations; `complete` claims require both.
    let full_rect = opt_rect_or_null(map, "full")?;
    let work_area = opt_rect_or_null(map, "work")?;
    Ok(ScreenReport {
        output: OutputId(req_id(map, "out")?),
        full_rect,
        work_area,
        gap: req_i32(map, "gap")?,
        outer_gap: req_i32(map, "outer")?,
        complete: req_bool(map, "complete")?,
        primary: req_bool(map, "primary")?,
    })
}

fn decode_view(value: &serde_json::Value) -> Result<ScreenView> {
    let map = as_object(value)?;
    check_keys(map, &["out", "view"])?;
    // Empty view means unknown (no desire received yet); otherwise an
    // opaque logical id.
    let text = req_str(map, "view")?;
    if !text.is_empty() && !is_opaque_id(text) {
        return Err(unknown_value());
    }
    Ok(ScreenView {
        output: OutputId(req_id(map, "out")?),
        view: text.to_owned(),
    })
}

fn decode_native(value: &serde_json::Value) -> Result<NativeObservation> {
    let map = as_object(value)?;
    check_keys(map, &["out", "order", "current"])?;
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
    // Empty current means unknown; otherwise an opaque id.
    let current = req_str(map, "current")?;
    if !current.is_empty() && !is_opaque_id(current) {
        return Err(unknown_value());
    }
    Ok(NativeObservation {
        output: OutputId(req_id(map, "out")?),
        native_order: order,
        current_native: current.to_owned(),
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

fn opt_homing_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<(OutputId, String)>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        Some(value) => {
            let homing = as_object(value)?;
            check_keys(homing, &["out", "ws"])?;
            Ok(Some((
                OutputId(req_id(homing, "out")?),
                req_id(homing, "ws")?,
            )))
        }
    }
}

fn opt_view_or_null(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>> {
    match map.get(key) {
        None => Err(malformed()),
        Some(serde_json::Value::Null) => Ok(None),
        // Empty string normalizes to unknown (None), consistently with
        // `views[]`/`current` empty-means-unknown: validation rejects empty
        // proof rows outright, so decoding must not produce `Some("")`.
        Some(serde_json::Value::String(text)) if text.is_empty() => Ok(None),
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            Ok(Some(text.clone()))
        }
        Some(_) => Err(malformed()),
    }
}

fn decode_member_attempt(
    value: &serde_json::Value,
) -> Result<tiler_core::world_report::MemberAttempt> {
    use tiler_core::world_report::MemberAttempt;
    let map = as_object(value)?;
    check_keys(
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
    )?;
    Ok(MemberAttempt {
        operation_id: req_id(map, "op")?,
        window: WindowId(req_id(map, "win")?),
        target_output: OutputId(req_id(map, "out")?),
        target_workspace: req_id(map, "ws")?,
        setter: parse_frame_setter(req_str(map, "setter")?)?,
        pre: opt_homing_or_null(map, "pre")?,
        post: opt_homing_or_null(map, "post")?,
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
}

fn decode_view_attempt(value: &serde_json::Value) -> Result<tiler_core::world_report::ViewAttempt> {
    use tiler_core::world_report::ViewAttempt;
    let map = as_object(value)?;
    check_keys(
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
    )?;
    Ok(ViewAttempt {
        operation_id: req_id(map, "op")?,
        target_output: OutputId(req_id(map, "out")?),
        target_workspace: req_id(map, "ws")?,
        setter: parse_frame_setter(req_str(map, "setter")?)?,
        pre: opt_view_or_null(map, "pre")?,
        post: opt_view_or_null(map, "post")?,
        baseline: opt_view_or_null(map, "base")?,
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
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

fn decode_frame_attempt(
    value: &serde_json::Value,
) -> Result<tiler_core::world_report::FrameAttempt> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "win",
            "target",
            "setter",
            "readback",
            "dpiBefore",
            "dpiAfter",
            "attemptMs",
            "op",
            "pre",
        ],
    )?;
    let target_value = map.get("target").ok_or_else(malformed)?;
    let operation_id = match map.get("op") {
        None => return Err(malformed()),
        Some(serde_json::Value::String(op)) if op.is_empty() => None,
        Some(serde_json::Value::String(op)) => Some(op.clone()),
        Some(_) => return Err(unknown_value()),
    };
    let prewrite = match map.get("pre") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(req_rect_value(value)?),
    };
    Ok(tiler_core::world_report::FrameAttempt {
        window: WindowId(req_id(map, "win")?),
        target: req_rect_value(target_value)?,
        setter: parse_frame_setter(req_str(map, "setter")?)?,
        readback: opt_rect_or_null(map, "readback")?,
        dpi_before: opt_u32_or_null(map, "dpiBefore")?,
        dpi_after: opt_u32_or_null(map, "dpiAfter")?,
        attempt_ms: req_u64_str(map, "attemptMs")?,
        operation_id,
        prewrite,
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

/// Normalized stage-5 presentation ingress: `{"foreground":S,
/// "minimized":B,"maximized":B,"fullscreen":B,"cloaked":B,
/// "hiddenWorkspace":B,"shell":B,"frame":[x,y,w,h]|null,
/// "borderEnabled":B,"underlayEnabled":B,"borderGap":N,"borderWidth":N,
/// "underlayExtension":N,"chordHeld":B,"moveWindow":S,"paused":B,
/// "pagerOrder":[S...],"pagerFrames":[{"win":S,"live":[x,y,w,h]|null,
/// "float":[x,y,w,h]|null,"tile":[x,y,w,h]|null},...]}`. Empty
/// `foreground`/`moveWindow` mean none; non-empty tokens must be opaque.
/// The frame is the live foreground frame in adapter units (`null` when
/// unreadable). Style numbers are host-scaled pixels. The pager order is
/// the native top-to-bottom stacking snapshot over opaque lifetimes; frame
/// rows carry the distinct live, float, and tile sources (any slot may be
/// null; the core resolves live first, then float, then tile).
fn decode_presentation(value: &serde_json::Value) -> Result<PresentationIngress> {
    use tiler_core::world_report::{PresentationIngress, PresentationPagerFrame};
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "foreground",
            "minimized",
            "maximized",
            "fullscreen",
            "cloaked",
            "hiddenWorkspace",
            "shell",
            "frame",
            "borderEnabled",
            "underlayEnabled",
            "borderGap",
            "borderWidth",
            "underlayExtension",
            "chordHeld",
            "moveWindow",
            "paused",
            "pagerOrder",
            "pagerFrames",
        ],
    )?;
    let foreground = req_str(map, "foreground")?;
    if !foreground.is_empty() && !is_opaque_id(foreground) {
        return Err(unknown_value());
    }
    let move_window = req_str(map, "moveWindow")?;
    if !move_window.is_empty() && !is_opaque_id(move_window) {
        return Err(unknown_value());
    }
    let frame = opt_rect_or_null(map, "frame")?;
    let order_value = map.get("pagerOrder").ok_or_else(malformed)?;
    let order_items = order_value.as_array().ok_or_else(malformed)?;
    let mut pager_order = Vec::with_capacity(order_items.len());
    for item in order_items {
        let id = item.as_str().ok_or_else(malformed)?;
        if !is_opaque_id(id) {
            return Err(unknown_value());
        }
        pager_order.push(WindowId(id.to_owned()));
    }
    let snapshots_value = map.get("pagerFrames").ok_or_else(malformed)?;
    let snapshot_items = snapshots_value.as_array().ok_or_else(malformed)?;
    let mut pager_frames = Vec::with_capacity(snapshot_items.len());
    for item in snapshot_items {
        let row = as_object(item)?;
        check_keys(row, &["win", "live", "float", "tile"])?;
        let id = req_str(row, "win")?;
        if !is_opaque_id(id) {
            return Err(unknown_value());
        }
        pager_frames.push(PresentationPagerFrame {
            window: WindowId(id.to_owned()),
            live: opt_rect_or_null(row, "live")?,
            float_frame: opt_rect_or_null(row, "float")?,
            tile_frame: opt_rect_or_null(row, "tile")?,
        });
    }
    Ok(PresentationIngress {
        foreground: foreground.to_owned(),
        foreground_minimized: req_bool(map, "minimized")?,
        foreground_maximized: req_bool(map, "maximized")?,
        foreground_fullscreen: req_bool(map, "fullscreen")?,
        foreground_cloaked: req_bool(map, "cloaked")?,
        foreground_hidden_workspace: req_bool(map, "hiddenWorkspace")?,
        foreground_shell: req_bool(map, "shell")?,
        foreground_frame: frame,
        border_enabled: req_bool(map, "borderEnabled")?,
        underlay_enabled: req_bool(map, "underlayEnabled")?,
        border_gap_px: req_i32(map, "borderGap")?,
        border_width_px: req_i32(map, "borderWidth")?,
        underlay_extension_px: req_i32(map, "underlayExtension")?,
        chord_held: req_bool(map, "chordHeld")?,
        move_window: move_window.to_owned(),
        paused: req_bool(map, "paused")?,
        pager_order,
        pager_frames,
    })
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
            Ok(WorldCommand::FocusDirectional {
                window: WindowId(req_id(map, "win")?),
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
        "operation" => {
            check_keys(map, &["op", "command"])?;
            let inner = map.get("command").ok_or_else(malformed)?;
            let inner_map = as_object(inner)?;
            Ok(WorldCommand::Operation(decode_core_command(inner_map)?))
        }
        _ => Err(unknown_value()),
    }
}

/// Decode one JSON world request into the typed core report. Enforces byte
/// length first (4 MiB), then exact shape/types/vocabularies, then the core
/// [`validate_report`] gate. Never logs or echoes payload bytes.
pub fn decode_request(json: &str) -> std::result::Result<WorldReport, WorldProtocolError> {
    if json.len() > WORLD_MAX_REQUEST_BYTES {
        return Err(WorldProtocolError {
            kind: KIND_OVERSIZED,
            message: MSG_OVERSIZED,
        });
    }
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| malformed())?;
    let map = as_object(&value)?;
    check_keys_with_optional(
        map,
        &[
            "v",
            "owner",
            "adapter",
            "gen",
            "obs",
            "reqSeq",
            "req",
            "cmdSeq",
            "command",
            "mode",
            "screens",
            "windows",
            "views",
            "closed",
            "hostOwned",
            "equalCount",
            "native",
            "predicate",
            "refill",
            "defaultTiled",
            "focused",
            "complete",
            "gesture",
            "switchTarget",
            "freshPlacement",
            "cursor",
            "actionForeground",
            "actionCached",
            "actionOutputs",
            "focusFollowsCursor",
            "cursorWarp",
            "presentation",
        ],
        &[
            "anchorFallback",
            "noSpaceResult",
            "cursorHit",
            "monotonicMs",
            "frameAttempts",
            "memberAttempts",
            "viewAttempts",
            "clearAttempts",
            "gestureIngress",
        ],
    )?;
    let version = map
        .get("v")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(malformed)?;
    if version != u64::from(WORLD_PROTOCOL_VERSION) {
        return Err(WorldProtocolError {
            kind: KIND_VERSION,
            message: MSG_VERSION,
        });
    }
    let command_seq = opt_u64_or_null(map, "cmdSeq")?;
    let command = match map.get("command") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_command(value)?),
    };
    if command_seq.is_none() != command.is_none() {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "command-pair-invalid",
        });
    }
    let screens_value = map.get("screens").ok_or_else(malformed)?;
    let mut screens = Vec::new();
    for item in screens_value.as_array().ok_or_else(malformed)? {
        screens.push(decode_screen(item)?);
    }
    let windows_value = map.get("windows").ok_or_else(malformed)?;
    let mut windows = Vec::new();
    for item in windows_value.as_array().ok_or_else(malformed)? {
        windows.push(decode_fact(item)?);
    }
    let views_value = map.get("views").ok_or_else(malformed)?;
    let mut views = Vec::new();
    for item in views_value.as_array().ok_or_else(malformed)? {
        views.push(decode_view(item)?);
    }
    let closed_value = map.get("closed").ok_or_else(malformed)?;
    let closed = decode_closed_ids(closed_value)?;
    let native_value = map.get("native").ok_or_else(malformed)?;
    let mut native = Vec::new();
    for item in native_value.as_array().ok_or_else(malformed)? {
        native.push(decode_native(item)?);
    }
    // Tolerant monotonic clock: missing backfills 0 (unreported, never
    // suppresses the transient lane). A present value must be a canonical
    // decimal string like every other counter; JSON numbers refuse.
    let monotonic_ms = match map.get("monotonicMs") {
        None => 0,
        Some(serde_json::Value::String(text)) => {
            parse_u64_decimal(text).ok_or_else(unknown_value)?
        }
        Some(_) => return Err(unknown_value()),
    };
    // Tolerant frame attempts: missing backfills empty. Present entries
    // carry the exact mechanical shape; unknown fields/values refuse.
    let mut frames = Vec::new();
    if let Some(frames_value) = map.get("frameAttempts") {
        for item in frames_value.as_array().ok_or_else(malformed)? {
            frames.push(decode_frame_attempt(item)?);
        }
    }
    // Tolerant membership/view attempts: missing backfills empty.
    let mut member_attempts = Vec::new();
    if let Some(value) = map.get("memberAttempts") {
        for item in value.as_array().ok_or_else(malformed)? {
            member_attempts.push(decode_member_attempt(item)?);
        }
    }
    let mut view_attempts = Vec::new();
    if let Some(value) = map.get("viewAttempts") {
        for item in value.as_array().ok_or_else(malformed)? {
            view_attempts.push(decode_view_attempt(item)?);
        }
    }
    // Tolerant clear attempts: missing backfills empty.
    let mut clear_attempts = Vec::new();
    if let Some(value) = map.get("clearAttempts") {
        for item in value.as_array().ok_or_else(malformed)? {
            clear_attempts.push(decode_clear_attempt(item)?);
        }
    }
    // Tolerant gesture ingress: missing backfills observation-only (`None`).
    // A present value carries the exact normalized shape; unknown
    // fields/values refuse.
    let gesture = match map.get("gestureIngress") {
        None => None,
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_gesture_ingress(value)?),
    };
    let predicate =
        tiler_core::size_hints::FixedSizePredicate::parse_wire(req_str(map, "predicate")?)
            .ok_or_else(unknown_value)?;
    let refill =
        tiler_core::output_selection::MigrationSourceRefill::parse_wire(req_str(map, "refill")?)
            .ok_or_else(unknown_value)?;
    // Cursor-first targeting (strict, no tolerant backfill): unknown
    // tokens refuse like the predicate/refill above, so an adapter that
    // predates these keys fails loudly instead of silently running the
    // wrong targeting policy.
    let switch_target =
        tiler_core::cursor_targeting::SwitchTarget::parse_wire(req_str(map, "switchTarget")?)
            .ok_or_else(unknown_value)?;
    let fresh_placement =
        tiler_core::cursor_targeting::FreshPlacement::parse_wire(req_str(map, "freshPlacement")?)
            .ok_or_else(unknown_value)?;
    // Tolerant D2/D3 capacity settings: missing, non-string, or invalid
    // tokens backfill the defaults (never refuse old adapters or typos).
    // Non-string present values are not a shape error here.
    let anchor_fallback = tiler_core::admission::AnchorFallback::normalize_wire(
        map.get("anchorFallback")
            .and_then(serde_json::Value::as_str),
    );
    let no_space_result = tiler_core::admission::NoSpaceResult::normalize_wire(
        map.get("noSpaceResult").and_then(serde_json::Value::as_str),
    );
    // No restoration ingress exists: every report is a fresh-observation
    // report. A present `restored` key is an unknown key and refuses
    // above; no durable ring store ever feeds this path.
    let workspace_mode = match req_str(map, "mode")? {
        "per-output-local" => tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        "global-unique" => tiler_core::workspace_scope::WorkspaceMode::GlobalUnique,
        "shared" => tiler_core::workspace_scope::WorkspaceMode::Shared,
        _ => return Err(unknown_value()),
    };
    let report = WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: req_bounded(map, "owner", MAX_OWNER_LEN, is_owner_text)?,
        adapter_incarnation: req_id(map, "adapter")?,
        core_generation: req_bounded(map, "gen", MAX_GENERATION_LEN, is_generation_text)?,
        observation_seq: req_u64_str(map, "obs")?,
        request_seq: req_u64_str(map, "reqSeq")?,
        request_id: req_bounded(map, "req", MAX_CORRELATION_LEN, is_correlation_text)?,
        command_seq,
        command,
        workspace_mode,
        screens,
        windows,
        views,
        closed,
        host_owned_workspace_domains: req_bool(map, "hostOwned")?,
        equal_workspace_count: req_bool(map, "equalCount")?,
        native,
        fixed_size_predicate: predicate,
        migration_source_refill: refill,
        anchor_fallback,
        no_space_result,
        switch_target,
        fresh_placement,
        cursor: opt_point_or_null(map, "cursor")?,
        cursor_hit: match map.get("cursorHit") {
            // Tolerant backfill: adapters predating the hit fact report
            // nothing, which the core reads as no hover, never as policy.
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
        focus_follows_cursor: req_bool(map, "focusFollowsCursor")?,
        cursor_warp: req_bool(map, "cursorWarp")?,
        default_tiled: req_bool(map, "defaultTiled")?,
        action_foreground: match map.get("actionForeground") {
            // Required neutral fact: `null` when the foreground output is
            // unknown; a carried id must be opaque (empty refuses).
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
            // Required ordered known-output registry (possibly empty):
            // every entry must be an opaque id; duplicates refuse in the
            // core gate below.
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
        inventory_complete: req_bool(map, "complete")?,
        gesture_barrier: req_bool(map, "gesture")?,
        gesture,
        monotonic_ms,
        frames,
        member_attempts,
        view_attempts,
        clear_attempts,
        presentation: decode_presentation(map.get("presentation").ok_or_else(malformed)?)?,
    };
    if let Some(reason) = validate_report_reason(&report) {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            // Fixed check vocabulary, never payload bytes.
            message: reason,
        });
    }
    // Hard input caps are enforced by the core [`validate_report`] gate
    // above (fixed check vocabulary, never payload bytes): every carried
    // string is already length-gated by its alphabet, and section counts
    // are core-capped, so the estimator bound holds.
    Ok(report)
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

/// Protocol-owned exact reply encoding. Byte-identical to
/// [`tiler_core::world_report::encode_reply_canonical`] over every valid
/// reply; the codec test locks the equality, so the core commit gate and
/// this transport gate cannot differ.
#[must_use]
pub fn encode_reply(reply: &WorldReply) -> String {
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
        out.push_str("{\"scope\":");
        push_json_string(&mut out, &scope.scope);
        out.push_str(",\"auth\":");
        push_json_string(&mut out, scope.authority.as_str());
        out.push_str(",\"reason\":");
        push_json_string(
            &mut out,
            scope.authority.hold_reason().map_or("", HoldReason::as_str),
        );
        out.push('}');
    }
    out.push_str("],\"workspaces\":[");
    for (i, workspace) in reply.workspaces.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        push_json_string(&mut out, &workspace.id);
        out.push_str(",\"out\":");
        push_json_string(&mut out, &workspace.output.0);
        out.push_str(",\"bind\":");
        push_json_string(
            &mut out,
            workspace.native_binding.as_deref().unwrap_or_default(),
        );
        out.push_str(",\"scope\":");
        push_json_string(&mut out, &workspace.scope);
        out.push_str(",\"order\":");
        out.push_str(&workspace.order.to_string());
        out.push_str(",\"mode\":");
        push_json_string(&mut out, workspace.tiling_mode.as_str());
        out.push_str(",\"exist\":");
        out.push_str(if workspace.existence { "true" } else { "false" });
        out.push_str(",\"hold\":");
        push_json_string(&mut out, workspace.hold.map_or("", HoldReason::as_str));
        out.push('}');
    }
    out.push_str("],\"outputs\":[");
    for (i, output) in reply.outputs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"out\":");
        push_json_string(&mut out, &output.output.0);
        out.push_str(",\"view\":");
        push_json_string(&mut out, output.logical_view.as_deref().unwrap_or_default());
        out.push_str(",\"mode\":");
        push_json_string(&mut out, output.tiling_mode.as_str());
        out.push_str(",\"hold\":");
        push_json_string(&mut out, output.hold.map_or("", HoldReason::as_str));
        out.push('}');
    }
    out.push_str("],\"windows\":[");
    for (i, window) in reply.windows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"win\":");
        push_json_string(&mut out, &window.lifetime.0);
        out.push_str(",\"out\":");
        push_json_string(&mut out, &window.output.0);
        out.push_str(",\"ws\":");
        push_json_string(&mut out, &window.workspace);
        out.push_str(",\"alloc\":");
        match window.allocation {
            Some(rect) => {
                out.push_str(&format!("[{},{},{},{}]", rect.x, rect.y, rect.w, rect.h));
            }
            None => out.push_str("null"),
        }
        out.push_str(",\"frame\":");
        match window.effective_frame {
            Some(rect) => {
                out.push_str(&format!("[{},{},{},{}]", rect.x, rect.y, rect.w, rect.h));
            }
            None => out.push_str("null"),
        }
        out.push_str(",\"frame_auth\":");
        push_json_string(&mut out, window.frame_authority.as_str());
        out.push_str(",\"frame_hold\":");
        push_json_string(
            &mut out,
            window
                .frame_authority
                .hold_reason()
                .map_or("", HoldReason::as_str),
        );
        out.push_str(",\"member_auth\":");
        push_json_string(&mut out, window.membership_authority.as_str());
        out.push_str(",\"member_hold\":");
        push_json_string(
            &mut out,
            window
                .membership_authority
                .hold_reason()
                .map_or("", HoldReason::as_str),
        );
        out.push_str(",\"frame_op\":");
        push_json_string(
            &mut out,
            window.frame_operation.as_deref().unwrap_or_default(),
        );
        out.push_str(",\"floating\":");
        out.push_str(if window.floating { "true" } else { "false" });
        out.push_str(",\"float_origin\":");
        push_json_string(
            &mut out,
            window
                .float_origin
                .map_or("", tiler_core::world_report::FloatOrigin::as_str),
        );
        out.push_str(",\"clear_op\":");
        push_json_string(
            &mut out,
            window.clear_operation.as_deref().unwrap_or_default(),
        );
        out.push('}');
    }
    out.push_str("],\"transfers\":[");
    for (i, transfer) in reply.transfers.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"op\":");
        push_json_string(&mut out, &transfer.operation_id);
        out.push_str(",\"kind\":");
        push_json_string(
            &mut out,
            match &transfer.kind {
                TransferKind::Window(_) => "window",
                TransferKind::View => "view",
                TransferKind::Migration => "migration",
            },
        );
        out.push_str(",\"win\":");
        match &transfer.kind {
            TransferKind::Window(window) => push_json_string(&mut out, &window.0),
            TransferKind::View | TransferKind::Migration => out.push_str("\"\""),
        }
        out.push_str(",\"out\":");
        push_json_string(&mut out, &transfer.target_output.0);
        out.push_str(",\"ws\":");
        push_json_string(&mut out, &transfer.target_workspace);
        out.push_str(",\"follow\":");
        out.push_str(if transfer.follow { "true" } else { "false" });
        out.push_str(",\"held\":");
        out.push_str(if transfer.held { "true" } else { "false" });
        out.push('}');
    }
    out.push_str("],\"effects\":[");
    for (i, effect) in reply.effects.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        push_json_string(&mut out, &effect.id);
        out.push_str(",\"kind\":");
        push_json_string(&mut out, effect.kind.as_str());
        out.push_str(",\"target\":");
        push_json_string(&mut out, &effect.target.0);
        out.push_str(",\"rev\":\"");
        out.push_str(&effect.issued_revision.to_string());
        out.push_str("\"}");
    }
    out.push_str("],\"result\":");
    tiler_core::world_sizing::encode_command_result(&mut out, &reply.command_result);
    out.push_str(",\"gestureSettled\":");
    tiler_core::world_sizing::encode_gesture_settled(&mut out, &reply.gesture_settled);
    out.push_str(",\"operationSettled\":");
    tiler_core::world_sizing::encode_operation_settled(&mut out, &reply.operation_settled);
    out.push_str(",\"warpCursor\":");
    match reply.cursor_warp {
        Some((x, y)) => {
            out.push_str(&format!("[{x},{y}]"));
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"presentation\":");
    tiler_core::world_sizing::encode_presentation(&mut out, &reply.presentation);
    out.push('}');
    out
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
}
