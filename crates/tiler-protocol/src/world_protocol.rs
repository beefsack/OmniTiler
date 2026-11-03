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
//! {v:3, owner, adapter, gen, obs:"u64", reqSeq:"u64", req, cmdSeq:"u64"|null,
//!  command:{op,...}|null, screens, windows, views, closed,
//!  hostOwned:bool, equalCount:bool, native, predicate, refill,
//!  defaultTiled:bool, focused:""|id, complete:bool, gesture:bool,
//!  switchTarget, freshPlacement, cursor:[x,y]|null,
//!  actionForeground:id|null, actionCached:id|null, actionOutputs:[id...],
//!  focusFollowsCursor:bool, cursorWarp:bool}
//! (plus optional tolerant D2/D3 settings: anchorFallback, noSpaceResult,
//!  and bornMaximized; missing or invalid values backfill the defaults;
//!  plus the optional tolerant birthClear capability: missing backfills
//!  false = no automatic birth clear, a present bool decodes, any other
//!  shape refuses)
//! screens[]: {out, full:[x,y,w,h]|null, work:[x,y,w,h]|null,
//!             complete:bool, scale:finite-positive-number,
//!             identity:{manufacturer,model,serial,connector}}
//!             (all six keys required; retired gap/outer/primary keys hold
//!             the identifiable row, other unknown keys refuse; scale is a
//!             finite positive JSON number; identity keys are all required
//!             and nullable with empty text preserved; bad identifiable rows
//!             hold narrowly with their carried fields retained, bad `out`
//!             refuses the report)
//! report-level primary: "primary":id|null (optional: absent means
//!             unknown, null refuses, an opaque id designates)
//! windows[] (facts): {win, out:null|id, rect:[x,y,w,h]|null,
//!             fullscreen:bool|null, maximized:bool|null, sticky:bool|null,
//!             minimized:bool|null, hints:{...}, nativeWs:""|id|null (optional,
//!             spelling-retained: missing stays omitted, null stays null,
//!             empty stays "", capability-gated host-owned
//!             native desktop id, separate namespace from logical ids;
//!             without hostOwned it refuses), hostHidden:bool|null,
//!             privileged:bool|null, managed:bool|null,
//!             acceptsInput:bool|null, activationInhibited:bool|null,
//!             toolWindow:bool|null, owned:bool|null, transient:bool|null,
//!             modal:bool|null, resizable:bool|null, above:bool|null,
//!             below:bool|null, owner:id|null, parent:id|null, role:text|null,
//!             appId:text|null, executable:text|null, processId:id|null,
//!             class:text|null, title:text|null, type:token|null,
//!             activationPolicy:token|null, layer:i32|null,
//!             fullscreenOwner:token|null, fullscreenExitable:bool|null,
//!             suppressionAvailable:bool|null,
//!             visibility:{visible:bool|null, suppressionOwned:bool|null},
//!             lastWrite:{...}|null} (retired floating/fitExcluded/fixedAuto/
//!             fixedSuppress/observedWs/shell/tool/ownedDialog/dialog/noActivate/
//!             cannotControl keys hold the identifiable row;
//!             unknown keys refuse; bad win/out-shape/rect/hints refuse;
//!             overbound text and wrong-typed values hold; unknown enum
//!             tokens refuse)
//! views[]: {out, view:""|id} (empty means unknown)
//! closed[]: [id...] (explicit closes)
//! native[] (host-owned observations, requires hostOwned): {out, order:[id...],
//!             current:id|null, occupancy:[{native,count:int|null}...]} (strict:
//!             null means the current surface is not an ordered desktop and
//!             retains the logical view; occupancy covers exactly each order
//!             id: count is the neutral all-window occupant total for that
//!             desktop, null is explicit unknown and never an inferred zero;
//!             empty strings and missing keys refuse)
//! predicate: "both-axes-fixed"|"either-axis-fixed"
//! refill: "last-remaining-workspace"|"most-recently-used-workspace"
//! anchorFallback: "largest-splittable-window"|"closest-splittable-relative"|"no-search"
//!   (optional, tolerant: missing or invalid backfills the default)
//! noSpaceResult: "float"|"allow-overlap"
//!   (optional, tolerant: missing or invalid backfills the default)
//! bornMaximized: "tile-on-launch"|"keep-maximized-overlay"
//!   (optional, tolerant: missing or invalid backfills tile-on-launch)
//! birthClear: bool (optional, tolerant: missing backfills false = no
//!   automatic birth clear; a present bool decodes, any other shape
//!   refuses like the strict capability flags)
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
//!    pre:{out,ws}|null, post:{out,ws}|null,
//!    nativePre:id|null (optional, tolerant: missing/null/empty backfills
//!    null for tiler-owned adapters), nativePost:id|null (same tolerance),
//!    attemptMs:"u64"} -
//!   mechanical membership write/homing facts only; at most one entry per
//!   operation window. The core owns window/migration completion. The
//!   host-owned native pre/post ids name the native desktops read
//!   immediately before/after the write (translated core-side through its
//!   own bindings; never an adapter table); without hostOwned any carried
//!   native id refuses.
//! viewAttempts[] (optional, tolerant: missing backfills []):
//!   {op, out, ws, setter:accepted|refused|threw,
//!    pre:""|id|null, post:""|id|null, base:""|id|null,
//!    nativePre:id|null, nativePost:id|null, nativeBase:id|null
//!    (each optional, tolerant: missing/null/empty backfills null),
//!    attemptMs:"u64"} - mechanical view write/proof facts only; at most
//!   one entry per owning view phase. The core owns every view verdict.
//!   Host-owned native current ids ride the native keys (translated
//!   core-side); logical keys stay null there. Without hostOwned any
//!   carried native id refuses.
//! clearAttempts[] (optional, tolerant: missing backfills []):
//!   {op, win, setter:accepted|refused|threw,
//!    pre:bool|null, post:bool|null, attemptMs:"u64"} - mechanical
//!   maximize-clear write/proof facts only (immediate pre/post maximized
//!   states); at most one entry per retained clear operation. The core
//!   owns directional clear completion.
//! stateAttempts[] (required, no tolerant backfill: missing refuses):
//!   {win, property:fullscreen|maximized|sticky|above, target:bool,
//!    rev:"u64", setter:accepted|refused|threw,
//!    pre:bool|null, post:bool|null, attemptMs:"u64"} - mechanical
//!   native-state write/proof facts only (immediate pre/post property
//!   states); at most one entry per window property across every carried
//!   revision. The core owns every state verdict.
//! creationAttempts[] (optional, tolerant: missing backfills []):
//!   {op, ws, out, setter:accepted|refused|threw,
//!    before:[id...], after:[id...],
//!    created:id|null (optional, tolerant: missing/null/empty backfills
//!    null for adapters predating the correlated receipt),
//!    attemptMs:"u64"} - mechanical native
//!   creation write/readback facts only (live native order immediately
//!   before and first successfully observed full order after the create call,
//!   plus the host-correlated created native
//!   id when the host can prove which fresh desktop its create call
//!   produced); at most one entry per outstanding
//!   creation operation (requires hostOwned). The core owns binding and
//!   settlement over the fresh native[] readback.
//! command.op: focus|send|switch-view|select-workspace|select-numbered|
//!   select-relative|select-previous|select-trailing|send-relative|
//!   send-numbered|set-workspace-tiling|toggle-workspace-tiling|
//!   migrate-workspace|focus-directional|move-directional|send-directional|
//!   migrate-directional|set-sticky|toggle-sticky|set-fullscreen|
//!   toggle-fullscreen|operation
//!   (fields per variant, see decode; every
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
//! `nativeCreations`: [] or one `{"op","ws","out"}` object per outstanding
//! core-owned native creation authorization (capability-gated, at most once
//! per op), plus
//! `warpCursor`: null or the desired `[x,y]` cursor-warp point
//! (cross-output focus only), plus `presentation`: the uniform
//! border/underlay/pager presentation (`foreground` token, `border` shape,
//! `underlay` found/hidden shape, `pager` content), plus `diagnostics`: []
//! or one `{"scope","row","id","kind"}` object per held identifiable row
//! (a missing `id` encodes as JSON null, never the empty string).
//! Bootstrap/discovery replies reuse the same shape with `ready:false` and
//! empty sections. Bounded errors reuse
//! `{v:3,outcome:"rejected",kind,message,gen}`.

use tiler_core::boundary::CoreCommand;
use tiler_core::bounds::{is_opaque_id, valid_carried_rect};
use tiler_core::connection::RawSettingValue;
use tiler_core::connection::{valid_identity_text, valid_process_identity_text};
use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{
    MAX_CORRELATION_LEN, MAX_GENERATION_LEN, MAX_NATIVE_WORKSPACE_ID_LEN, NativeWorkspaceId,
};
use tiler_core::world_report::{
    ActivationPolicy, AuthorityScope, FullscreenOwner, HeldScreenRow, HeldWindowRow, HoldReason,
    IntentRecord, LastWrite, LastWritePath, LastWriteProperty, MAX_INTENT_RECORDS,
    NativeObservation, NativeWorkspaceFact, OutputIdentity, PresentationIngress, RowDiagnostic,
    RowDiagnosticKind, RowDiagnosticLane, ScreenReport, ScreenView, TransferKind, VisibilityFact,
    VisibilityReason, VisibilitySample, VisibilitySetter, WORLD_CONTRACT_VERSION, WindowFact,
    WindowType, WorkspaceRef, WorldCommand, WorldReply, WorldReport, check_screen_structure,
    check_window_structure, validate_report_reason,
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
    out.push_str("{\"v\":3,\"outcome\":\"rejected\",\"kind\":");
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

/// Strict v4 window-row decode: `{win, out, rect, fullscreen, maximized,
/// sticky, minimized, hints, nativeWs?, hostHidden, privileged, managed,
/// acceptsInput, activationInhibited, toolWindow, owned, transient, modal,
/// resizable, above, below, owner, parent, role, appId, executable,
/// processId, class, title, type, activationPolicy, layer, fullscreenOwner,
/// fullscreenExitable, suppressionAvailable, visibility, lastWrite}` with
/// nested `hints`/`visibility`/`lastWrite` objects.
/// - Unknown top-level keys refuse (`unknown-field`), except the retired
///   membership keys above which hold the identifiable row.
/// - A missing or non-opaque `win` refuses the whole report (missing is
///   `malformed`, a bad id or a duplicate lifetime is `unknown-value`
///   grammar / `snapshot-invalid` duplicate refusal).
/// - `out` is required but nullable: `null` is unknown affinity (never a
///   current-output guess, never structurally invalid); a bad id shape
///   refuses. Unknown-but-valid outputs stay valid: the core retains the
///   prior home and holds the scope.
/// - `rect` failures refuse like the other structural shapes; `hints`
///   is required but nullable (`null` is unknown, never a default empty
///   hint set), and a wrong-typed `hints` refuses.
/// - Overbound metadata text and wrong-typed value fields hold the
///   identifiable row; unknown enum tokens refuse (`unknown-value`).
/// - Structural id/duplicate/rect violations refuse before any narrow hold
///   below (shared policy with the core structural check).
fn decode_fact(
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
    check_keys_with_optional(
        map,
        REQUIRED,
        &[
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
        ],
    )?;
    let window = match map.get("win") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => WindowId(text.clone()),
        Some(serde_json::Value::String(_)) => return Err(unknown_value()),
        _ => return Err(malformed()),
    };
    // A null rect is an explicitly unreadable frame: the homed scope
    // holds instead of authorizing geometry, never a silent absence.
    let rect = opt_rect_or_null(map, "rect")?;
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
    let hints = match map.get("hints") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_hints(value)?),
    };
    let native_workspace = match map.get("nativeWs") {
        None => NativeWorkspaceFact::Omitted,
        Some(serde_json::Value::Null) => NativeWorkspaceFact::Null,
        Some(serde_json::Value::String(text)) if text.is_empty() => {
            NativeWorkspaceFact::Value(String::new())
        }
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            NativeWorkspaceFact::Value(text.clone())
        }
        Some(_) => return Err(malformed()),
    };
    // Retired membership verdicts hold the identifiable row (fail closed).
    let retired = RETIRED_WINDOW_KEYS.iter().any(|key| map.contains_key(*key));
    // Wrong-typed value fields and overbound metadata hold the
    // identifiable row instead of refusing it. The strict decoders below
    // re-check every shape; this pre-scan only decides Valid vs Held so
    // the structural gate below fires before any hold.
    let mut held = retired;
    let mut overbound = false;
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
            overbound = true;
            held = true;
        }
    }
    // Bad process grammar holds like overbound text (typed parity): the
    // strict decoder below would otherwise refuse what the typed grader
    // holds. Bad owner/parent grammar is never held: it refuses below even
    // when this row also carries a hold cause.
    if let Some(serde_json::Value::String(text)) = map.get("processId")
        && !tiler_core::connection::valid_process_identity_text(text)
    {
        held = true;
    }
    // Minimal identifiable candidate for the structural gate: win, out,
    // rect, and the duplicate check. Value shapes decode strictly below.
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
    if held || overbound {
        // Structural id/duplicate/rect violations refuse before any
        // narrow hold (shared policy with the core structural check).
        // Bad owner/parent grammar refuses here too: a hold cause
        // (retired/overbound/wrong-type/bad-process) must not mask it.
        // Native grammar already refused above; bad process holds.
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
        role: opt_text_or_null(map, "role")?,
        app_id: opt_text_or_null(map, "appId")?,
        executable: opt_text_or_null(map, "executable")?,
        process_id: opt_process_or_null(map, "processId")?,
        class: opt_text_or_null(map, "class")?,
        title: opt_text_or_null(map, "title")?,
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
    check_keys(map, &["out", "order", "current", "occupancy"])?;
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
    // Strict codec: `current` is a string opaque id or `null`. `null`
    // means the current surface is not an ordered desktop (retains the
    // logical view while still using the order). Empty strings and missing
    // keys refuse: no compatibility tolerance is invented.
    let current_native = match map.get("current") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => {
            if !is_opaque_id(text) {
                return Err(unknown_value());
            }
            Some(text.clone())
        }
        Some(_) => return Err(unknown_value()),
    };
    // v3 required neutral occupancy: `[{native, count:int|null}...]`.
    // Shape-checked here (opaque id, u32 JSON number or null, no
    // tolerance); exact coverage of `order` is enforced by core
    // validation (`native-occupancy-incomplete`, `duplicate-native-occupancy`).
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
    Ok(NativeObservation {
        output: OutputId(req_id(map, "out")?),
        native_order: order,
        current_native,
        occupancy,
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
            "views",
            "closed",
            "native",
            "focused",
            "complete",
            "gesture",
            "cursor",
            "actionForeground",
            "actionCached",
            "actionOutputs",
            "presentation",
            "intentRecords",
            "stateAttempts",
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
        match decode_fact(item, ordinal, &mut seen_windows)? {
            WindowRowOutcome::Valid(fact) => windows.push(*fact),
            WindowRowOutcome::Held(held) => held_windows.push(held),
        }
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
    // Tolerant creation attempts: missing backfills empty. Present entries
    // carry the exact mechanical shape; unknown fields/values refuse. The
    // core gate below refuses them without the host-owned capability.
    let mut creation_attempts = Vec::new();
    if let Some(value) = map.get("creationAttempts") {
        for item in value.as_array().ok_or_else(malformed)? {
            creation_attempts.push(decode_creation_attempt(item)?);
        }
    }
    // Required native-state attempts: closed mechanical rows, no missing
    // tolerance (adapters always carry the lane, possibly empty). Unknown
    // fields/values refuse; per-property duplicates and bad window ids
    // refuse in the core gate below.
    let state_attempts_value = map.get("stateAttempts").ok_or_else(malformed)?;
    let mut state_attempts = Vec::new();
    for item in state_attempts_value.as_array().ok_or_else(malformed)? {
        state_attempts.push(decode_state_attempt(item)?);
    }
    // Tolerant gesture ingress: missing backfills observation-only (`None`).
    // A present value carries the exact normalized shape; unknown
    // fields/values refuse.
    let gesture = match map.get("gestureIngress") {
        None => None,
        Some(serde_json::Value::Null) => None,
        Some(value) => Some(decode_gesture_ingress(value)?),
    };
    // Required opaque host storage return: closed `{"win","data"}` rows.
    // The `data` string rides verbatim (never decoded here): unsupported
    // or malformed inner JSON stays an accepted envelope, while a
    // wrong-typed `data` (object, null) refuses as malformed and an
    // overbound string or an over-cap section refuses the envelope.
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
    let report = WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: req_id(map, "adapter")?,
        core_generation: req_bounded(map, "gen", MAX_GENERATION_LEN, is_generation_text)?,
        observation_seq: req_u64_str(map, "obs")?,
        request_seq: req_u64_str(map, "reqSeq")?,
        request_id: req_bounded(map, "req", MAX_CORRELATION_LEN, is_correlation_text)?,
        command_seq,
        command,
        screens,
        windows,
        views,
        closed,
        native,
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
        creation_attempts,
        presentation: decode_presentation(map.get("presentation").ok_or_else(malformed)?)?,
        intent_records,
        state_attempts,
        primary_output: match map.get("primary") {
            // Host-designated primary: absent means unknown (no fallback
            // to any per-screen flag: `primary` is not a screen key and
            // any screen row carrying it is held). Explicit null refuses;
            // unknown is absence, never an explicit null.
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
        held_screens,
        held_windows,
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

/// Transport mirror of the core canonical reply writer: byte-identical by
/// construction. Same unchecked-`String` caller precondition as
/// [`tiler_core::world_sizing::encode_reply_canonical`]: the reply's
/// `intent_writes` must satisfy
/// [`tiler_core::world_report::validate_intent_writes_reason`]; the engine
/// publication gate enforces it before any commit or encode.
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
        push_authority_scope(&mut out, &scope.scope);
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
        push_authority_scope(&mut out, &workspace.scope);
        out.push_str(",\"order\":");
        out.push_str(&workspace.order.to_string());
        out.push_str(",\"mode\":");
        push_json_string(&mut out, workspace.tiling_mode.as_str());
        out.push_str(",\"exist\":");
        out.push_str(if workspace.existence { "true" } else { "false" });
        out.push_str(",\"hold\":");
        push_json_string(&mut out, workspace.hold.map_or("", HoldReason::as_str));
        if workspace.trailing {
            out.push_str(",\"trailing\":true");
        }
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
        tiler_core::world_sizing::encode_window_target_lanes(&mut out, window);
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
    out.push_str(",\"nativeCreations\":");
    tiler_core::world_sizing::encode_native_creations(&mut out, &reply.native_creations);
    out.push_str(",\"warpCursor\":");
    match reply.cursor_warp {
        Some((x, y)) => {
            out.push_str(&format!("[{x},{y}]"));
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"presentation\":");
    tiler_core::world_sizing::encode_presentation(&mut out, &reply.presentation);
    out.push_str(",\"intentWrites\":[");
    for (i, write) in reply.intent_writes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"win\":");
        push_json_string(&mut out, &write.win.0);
        out.push_str(",\"data\":");
        match write.data.as_deref() {
            Some(data) => push_json_string(&mut out, data),
            None => out.push_str("null"),
        }
        out.push('}');
    }
    out.push_str("],\"diagnostics\":[");
    for (i, diagnostic) in reply.diagnostics.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        tiler_core::world_sizing::push_row_diagnostic(&mut out, diagnostic);
    }
    out.push(']');
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
