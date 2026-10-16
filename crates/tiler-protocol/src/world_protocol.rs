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
//! {v:1, owner, adapter, gen, obs:"u64", reqSeq:"u64", req, cmdSeq:"u64"|null,
//!  command:{op,...}|null, mode, topology, bindings, domains,
//!  minimized, predicate, defaultTiled:bool, focused:""|id, complete:bool,
//!  gesture:bool, feedback}
//! topology[]: {out, order:[native...], current, complete:bool,
//!              rect:[x,y,w,h]|null, primary:bool}
//! bindings[]: {logical, native, out, pin:bool, disp:bool, managed:bool}
//! domains[]: {id, ws, bounds:[x,y,w,h], gap, outer, focused, readable:bool,
//!             hold:""|token, windows, closed:[id...]}
//! windows[]: {win, out, ws, rect:[x,y,w,h], floating:bool, fitExcluded:bool,
//!             fullscreen:bool, maximized:bool, sticky:bool, fixedAuto:bool,
//!             fixedSuppress:bool, hints:{minW,maxW,minH,maxH: i32|null}}
//! minimized[]: {win, out, ws}
//! predicate: "both-axes-fixed"|"either-axis-fixed"
//! feedback[]: {op, kind:transfer|view|creation|effect|property,
//!              outcome:unattempted|possibly-executed|refused-proven|
//!                      superseded-external|applied, hold:""|token}
//! command.op: focus|send|switch-view|select-workspace|select-numbered|
//!   select-relative|select-previous|select-trailing|send-relative|
//!   send-numbered|set-workspace-tiling|
//!   migrate-workspace|operation (fields per variant, see decode)
//! ```
//!
//! Reply keys are exactly the core canonical reply encoding (`v`, `adapter`,
//! `gen`, `req`, `obs`/`rev` as decimal strings, `ready`, `consumed`,
//! `scopes`, `workspaces`, `outputs`, `windows`, `transfers`, `effects`,
//! `result`: null or the correlated per-command outcome that the snapshot
//! alone cannot express - drag-preview rect+hover, toggle-float placement,
//! active-group/no-group highlight, migrate-workspace provenance; move,
//! resize, tiled-admit, focus, and send results ride the snapshot,
//! transfers, and effects with `result: null`).
//! Bootstrap/discovery replies reuse the same shape with `ready:false` and
//! empty sections. Bounded errors reuse
//! `{v:1,outcome:"rejected",kind,message,gen}`.

use std::collections::BTreeMap;

use tiler_core::boundary::CoreCommand;
use tiler_core::bounds::{is_opaque_id, valid_carried_rect};
use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{MAX_CORRELATION_LEN, MAX_GENERATION_LEN, MAX_OWNER_LEN};
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, DomainReport, HoldReason, MinimizedEntry,
    OutputTopology, TransferKind, WORLD_CONTRACT_VERSION, WorkspaceBinding, WorldCommand,
    WorldReply, WorldReport, validate_report,
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
const MSG_SNAPSHOT: &str = "observation does not cover the known window set";

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
    out.push_str("{\"v\":1,\"outcome\":\"rejected\",\"kind\":");
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
    if map.len() != known.len() || !known.iter().all(|k| map.contains_key(*k)) {
        // Distinguish unknown fields from missing ones: any key outside the
        // vocabulary is unknown-field, otherwise the shape is malformed.
        if map.keys().any(|k| !known.contains(&k.as_str())) {
            return Err(unknown_field());
        }
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

fn req_rect(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Result<Rect> {
    let value = map.get(key).ok_or_else(malformed)?;
    req_rect_value(value)
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

fn parse_hold_token(text: &str) -> Result<Option<HoldReason>> {
    match text {
        "" => Ok(None),
        "unreadable" => Ok(Some(HoldReason::Unreadable)),
        "incomplete-inventory" => Ok(Some(HoldReason::IncompleteInventory)),
        "gesture-barrier" => Ok(Some(HoldReason::GestureBarrier)),
        "refused-target" => Ok(Some(HoldReason::RefusedTarget)),
        "ambiguous-creation" => Ok(Some(HoldReason::AmbiguousCreation)),
        "occupied-removal" => Ok(Some(HoldReason::OccupiedRemoval)),
        "constraint-clamp" => Ok(Some(HoldReason::ConstraintClamp)),
        "minimized" => Ok(Some(HoldReason::Minimized)),
        _ => Err(unknown_value()),
    }
}

fn parse_mode(text: &str) -> Result<WorkspaceMode> {
    match text {
        "per-output-local" => Ok(WorkspaceMode::PerOutputLocal),
        "global-unique" => Ok(WorkspaceMode::GlobalUnique),
        "shared" => Ok(WorkspaceMode::Shared),
        _ => Err(unknown_value()),
    }
}

fn parse_attempt_kind(text: &str) -> Result<AttemptKind> {
    match text {
        "transfer" => Ok(AttemptKind::Transfer),
        "view" => Ok(AttemptKind::View),
        "creation" => Ok(AttemptKind::Creation),
        "effect" => Ok(AttemptKind::Effect),
        "property" => Ok(AttemptKind::Property),
        _ => Err(unknown_value()),
    }
}

fn parse_attempt_outcome(text: &str) -> Result<AttemptOutcome> {
    match text {
        "unattempted" => Ok(AttemptOutcome::Unattempted),
        "possibly-executed" => Ok(AttemptOutcome::PossiblyExecuted),
        "refused-proven" => Ok(AttemptOutcome::RefusedProven),
        "superseded-external" => Ok(AttemptOutcome::SupersededExternal),
        "applied" => Ok(AttemptOutcome::Applied),
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

fn decode_window(value: &serde_json::Value) -> Result<tiler_core::seed::EngineWindow> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "win",
            "out",
            "ws",
            "rect",
            "floating",
            "fitExcluded",
            "fullscreen",
            "maximized",
            "sticky",
            "fixedAuto",
            "fixedSuppress",
            "hints",
        ],
    )?;
    Ok(tiler_core::seed::EngineWindow {
        window: WindowId(req_id(map, "win")?),
        output: OutputId(req_id(map, "out")?),
        workspace: WorkspaceId(req_id(map, "ws")?),
        rect: req_rect(map, "rect")?,
        floating: req_bool(map, "floating")?,
        fit_excluded: req_bool(map, "fitExcluded")?,
        fullscreen: req_bool(map, "fullscreen")?,
        maximized: req_bool(map, "maximized")?,
        sticky: req_bool(map, "sticky")?,
        fixed_auto: req_bool(map, "fixedAuto")?,
        fixed_suppress: req_bool(map, "fixedSuppress")?,
        hints: decode_hints(map.get("hints").ok_or_else(malformed)?)?,
    })
}

fn decode_domain(value: &serde_json::Value) -> Result<DomainReport> {
    use tiler_core::directional::Direction;
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "id", "ws", "bounds", "gap", "outer", "focused", "readable", "hold", "adjacent",
            "windows", "closed",
        ],
    )?;
    let bounds = req_rect(map, "bounds")?;
    let mut adjacent = BTreeMap::new();
    let adjacent_value = map.get("adjacent").ok_or_else(malformed)?;
    let adjacent_map = as_object(adjacent_value)?;
    for (key, target) in adjacent_map {
        let direction = match key.as_str() {
            "left" => Direction::Left,
            "right" => Direction::Right,
            "up" => Direction::Up,
            "down" => Direction::Down,
            _ => return Err(unknown_value()),
        };
        let target = target.as_str().ok_or_else(malformed)?;
        if !is_opaque_id(target) {
            return Err(unknown_value());
        }
        if adjacent
            .insert(direction, OutputId(target.to_owned()))
            .is_some()
        {
            return Err(malformed());
        }
    }
    if adjacent.len() > 4 {
        return Err(unknown_value());
    }
    let windows_value = map.get("windows").ok_or_else(malformed)?;
    let windows_items = windows_value.as_array().ok_or_else(malformed)?;
    let mut windows = Vec::with_capacity(windows_items.len());
    for item in windows_items {
        windows.push(decode_window(item)?);
    }
    let closed_value = map.get("closed").ok_or_else(malformed)?;
    let closed_items = closed_value.as_array().ok_or_else(malformed)?;
    let mut closed = Vec::with_capacity(closed_items.len());
    for item in closed_items {
        let id = item.as_str().ok_or_else(malformed)?;
        if !is_opaque_id(id) {
            return Err(unknown_value());
        }
        closed.push(WindowId(id.to_owned()));
    }
    // Empty focused id means none; otherwise an opaque id.
    let focused_text = req_str(map, "focused")?;
    if !focused_text.is_empty() && !is_opaque_id(focused_text) {
        return Err(unknown_value());
    }
    let readable = req_bool(map, "readable")?;
    let hold = parse_hold_token(req_str(map, "hold")?)?;
    Ok(DomainReport {
        domain: tiler_core::session::OutputDomain {
            id: OutputId(req_id(map, "id")?),
            workspace: WorkspaceId(req_id(map, "ws")?),
            bounds,
            gap: req_i32(map, "gap")?,
            adjacent,
        },
        outer_gap: req_i32(map, "outer")?,
        focused: WindowId(focused_text.to_owned()),
        windows,
        closed,
        readable,
        hold,
    })
}

fn decode_topology(value: &serde_json::Value) -> Result<OutputTopology> {
    let map = as_object(value)?;
    check_keys(
        map,
        &["out", "order", "current", "complete", "rect", "primary"],
    )?;
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
    // Live native output geometry when observed (null when unreadable);
    // a present rect must carry valid carried bounds, never a window
    // frame proxy (placement only reads disconnect-time output facts).
    let geometry = match map.get("rect") {
        None => return Err(malformed()),
        Some(serde_json::Value::Null) => None,
        Some(rect) => Some(req_rect_value(rect)?),
    };
    Ok(OutputTopology {
        output: OutputId(req_id(map, "out")?),
        native_order: order,
        current_native: req_id(map, "current").map_err(|_| {
            // An empty current view refuses: topology currents are always
            // observed natives. Kept as unknown-value (vocabulary), not
            // malformed (shape).
            if map.get("current").and_then(serde_json::Value::as_str) == Some("") {
                unknown_value()
            } else {
                malformed()
            }
        })?,
        complete: req_bool(map, "complete")?,
        geometry,
        primary: req_bool(map, "primary")?,
    })
}

fn decode_binding(value: &serde_json::Value) -> Result<WorkspaceBinding> {
    let map = as_object(value)?;
    check_keys(map, &["logical", "native", "out", "pin", "disp", "managed"])?;
    Ok(WorkspaceBinding {
        logical_id: req_id(map, "logical")?,
        native_id: req_id(map, "native")?,
        output: OutputId(req_id(map, "out")?),
        pinned: req_bool(map, "pin")?,
        displaced: req_bool(map, "disp")?,
        managed: req_bool(map, "managed")?,
    })
}

fn decode_minimized(value: &serde_json::Value) -> Result<MinimizedEntry> {
    let map = as_object(value)?;
    check_keys(map, &["win", "out", "ws"])?;
    Ok(MinimizedEntry {
        window: WindowId(req_id(map, "win")?),
        output: OutputId(req_id(map, "out")?),
        workspace: WorkspaceId(req_id(map, "ws")?),
    })
}

fn decode_feedback(value: &serde_json::Value) -> Result<AttemptFeedback> {
    let map = as_object(value)?;
    check_keys(map, &["op", "kind", "outcome", "hold"])?;
    Ok(AttemptFeedback {
        operation_id: req_id(map, "op")?,
        kind: parse_attempt_kind(req_str(map, "kind")?)?,
        outcome: parse_attempt_outcome(req_str(map, "outcome")?)?,
        hold: parse_hold_token(req_str(map, "hold")?)?,
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
        "active-group" => {
            check_keys(map, &["op"])?;
            Ok(CoreCommand::ActiveGroup)
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
            Ok(WorldCommand::SelectNumbered {
                output: OutputId(req_id(map, "out")?),
                index,
            })
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
            Ok(WorldCommand::SelectRelative {
                output: OutputId(req_id(map, "out")?),
                delta,
            })
        }
        "select-previous" => {
            check_keys(map, &["op", "out"])?;
            Ok(WorldCommand::SelectPrevious {
                output: OutputId(req_id(map, "out")?),
            })
        }
        "select-trailing" => {
            check_keys(map, &["op", "out"])?;
            Ok(WorldCommand::SelectTrailing {
                output: OutputId(req_id(map, "out")?),
            })
        }
        "send-relative" => {
            check_keys(map, &["op", "win", "out", "delta", "follow"])?;
            let delta = map
                .get("delta")
                .and_then(serde_json::Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(malformed)?;
            if delta != -1 && delta != 1 {
                return Err(unknown_value());
            }
            Ok(WorldCommand::SendRelative {
                window: WindowId(req_id(map, "win")?),
                output: OutputId(req_id(map, "out")?),
                delta,
                follow: req_bool(map, "follow")?,
            })
        }
        "send-numbered" => {
            check_keys(map, &["op", "win", "out", "index", "follow"])?;
            let index = map
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .ok_or_else(malformed)?;
            // 0 is the trailing path; 1..=9 are ring positions.
            if index > 9 {
                return Err(unknown_value());
            }
            Ok(WorldCommand::SendNumbered {
                window: WindowId(req_id(map, "win")?),
                output: OutputId(req_id(map, "out")?),
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
    check_keys(
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
            "topology",
            "bindings",
            "domains",
            "minimized",
            "predicate",
            "defaultTiled",
            "focused",
            "complete",
            "gesture",
            "feedback",
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
            message: MSG_SNAPSHOT,
        });
    }
    let topology_value = map.get("topology").ok_or_else(malformed)?;
    let mut topology = Vec::new();
    for item in topology_value.as_array().ok_or_else(malformed)? {
        topology.push(decode_topology(item)?);
    }
    let bindings_value = map.get("bindings").ok_or_else(malformed)?;
    let mut bindings = Vec::new();
    for item in bindings_value.as_array().ok_or_else(malformed)? {
        bindings.push(decode_binding(item)?);
    }
    let domains_value = map.get("domains").ok_or_else(malformed)?;
    let mut domains = Vec::new();
    for item in domains_value.as_array().ok_or_else(malformed)? {
        domains.push(decode_domain(item)?);
    }
    let feedback_value = map.get("feedback").ok_or_else(malformed)?;
    let mut feedback = Vec::new();
    for item in feedback_value.as_array().ok_or_else(malformed)? {
        feedback.push(decode_feedback(item)?);
    }
    let minimized_value = map.get("minimized").ok_or_else(malformed)?;
    let mut minimized = Vec::new();
    for item in minimized_value.as_array().ok_or_else(malformed)? {
        minimized.push(decode_minimized(item)?);
    }
    let predicate =
        tiler_core::size_hints::FixedSizePredicate::parse_wire(req_str(map, "predicate")?)
            .ok_or_else(unknown_value)?;
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
        workspace_mode: parse_mode(req_str(map, "mode")?)?,
        topology,
        bindings,
        domains,
        minimized,
        fixed_size_predicate: predicate,
        default_tiled: req_bool(map, "defaultTiled")?,
        focused_global: {
            let text = req_str(map, "focused")?;
            if !text.is_empty() && !is_opaque_id(text) {
                return Err(unknown_value());
            }
            text.to_owned()
        },
        inventory_complete: req_bool(map, "complete")?,
        gesture_barrier: req_bool(map, "gesture")?,
        feedback,
    };
    if !validate_report(&report) {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: MSG_SNAPSHOT,
        });
    }
    // Hard input caps that `validate_report` does not bound alone: every
    // carried string is already length-gated by its alphabet above, and
    // section counts are core-capped, so the estimator bound holds.
    if report.domains.len() > tiler_core::world_report::MAX_WORLD_DOMAINS {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: MSG_SNAPSHOT,
        });
    }
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
        out.push_str(",\"logical\":");
        push_json_string(
            &mut out,
            workspace.logical_id.as_deref().unwrap_or_default(),
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
    out.push('}');
    out
}

/// Retained engine surface for the service boundary: the planner service
/// drives [`WorldEngine`] through this module so only `tiler-protocol` sits
/// between the service and the core.
pub use tiler_core::world_engine::{WorldEngine, WorldOutcome};

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
