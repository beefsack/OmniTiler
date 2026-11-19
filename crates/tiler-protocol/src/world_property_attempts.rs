//! V4 `propertyAttempts` wire codec: kind-partitioned decoding, per-kind caps,
//! duplicate-key checks, held-row retention, and wire-order reconstruction.

use std::collections::BTreeSet;

use tiler_core::bounds::is_opaque_id;
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::world_report::{
    AttemptLaneV4, ClearAttempt, FrameAttempt, HeldAttemptRow, LastWritePath,
    MAX_WORLD_CLEAR_ATTEMPTS, MAX_WORLD_FRAMES, MAX_WORLD_MEMBER_ATTEMPTS,
    MAX_WORLD_PROPERTY_ATTEMPTS, MAX_WORLD_STATE_ATTEMPTS, MAX_WORLD_VIEW_ATTEMPTS,
    MAX_WORLD_VISIBILITY_ATTEMPTS, MemberAttempt, MemberDisposition, StateAttempt, StateProperty,
    ViewAttempt, VisibilityAttempt, VisibilityReason, VisibilitySetter,
};

use crate::world_protocol::{
    KIND_SNAPSHOT, Result, WorldProtocolError, as_object, attempt_hold_scope, attempt_identity,
    check_keys, check_keys_with_optional, decode_visibility_sample, encode_held_fields,
    held_attempt_row, malformed, opt_bool_or_null, opt_rect_or_null, opt_u32_or_null,
    opt_workspace_ref_or_null, parse_frame_setter, push_json_string, push_opt_bool, push_opt_rect,
    push_opt_u32, push_opt_workspace_ref, push_rect, push_visibility_sample,
    reorder_visibility_sample, req_bool, req_id, req_rect_value, req_str, req_u64_str,
    snapshot_invalid, unknown_value,
};
use crate::world_report_residual::HeldFields;

/// Staged v4 attempt-row outcome: either a usable row or an identifiable
/// row held narrowly with its carried fields retained in protocol residual.
pub(crate) enum AttemptRowOutcome<T> {
    Valid(T),
    Held {
        row: HeldAttemptRow,
        fields: HeldFields,
    },
}

/// Decoded `propertyAttempts` prefix (`frame -> member -> view -> clear`)
/// plus wire-order reconstruction metadata.
pub(crate) struct DecodedPropertyPrefix {
    pub frames: Vec<FrameAttempt>,
    pub member_attempts: Vec<MemberAttempt>,
    pub view_attempts: Vec<ViewAttempt>,
    pub clear_attempts: Vec<ClearAttempt>,
    pub held_attempts: Vec<HeldAttemptRow>,
    pub held_attempt_fields: Vec<(AttemptLaneV4, usize, HeldFields)>,
    pub property_attempt_order: Vec<AttemptLaneV4>,
}

/// Partitioned `state` and `visibility` buckets deferred until after
/// `creationAttempts` to preserve legacy cross-section refusal precedence.
pub(crate) struct PropertyAttemptTail<'a> {
    state_bucket: Vec<(usize, &'a serde_json::Value)>,
    visibility_bucket: Vec<(usize, &'a serde_json::Value)>,
}

/// Decoded `state` and `visibility` suffix from [`PropertyAttemptTail`].
pub(crate) struct DecodedPropertyTail {
    pub state_attempts: Vec<StateAttempt>,
    pub visibility_attempts: Vec<VisibilityAttempt>,
}

fn section_over_cap() -> WorldProtocolError {
    WorldProtocolError {
        kind: KIND_SNAPSHOT,
        message: "section-over-cap",
    }
}

/// Partition `propertyAttempts` into the six closed kinds, enforce total cap
/// and `frame|member|view|clear` per-kind caps, and decode `frame -> member ->
/// view -> clear` (producer order within each kind; first hard error wins).
pub(crate) fn decode_property_attempts_prefix(
    items: &[serde_json::Value],
) -> Result<(DecodedPropertyPrefix, PropertyAttemptTail<'_>)> {
    if items.len() > MAX_WORLD_PROPERTY_ATTEMPTS {
        return Err(section_over_cap());
    }
    let mut frame_bucket = Vec::new();
    let mut member_bucket = Vec::new();
    let mut view_bucket = Vec::new();
    let mut clear_bucket = Vec::new();
    let mut state_bucket = Vec::new();
    let mut visibility_bucket = Vec::new();
    let mut property_attempt_order = Vec::with_capacity(items.len());

    for (ordinal, item) in items.iter().enumerate() {
        let map = as_object(item)?;
        let lane = match map.get("kind") {
            None => return Err(malformed()),
            Some(serde_json::Value::String(kind)) => match kind.as_str() {
                "frame" => {
                    frame_bucket.push((ordinal, item));
                    AttemptLaneV4::Frame
                }
                "member" => {
                    member_bucket.push((ordinal, item));
                    AttemptLaneV4::Member
                }
                "view" => {
                    view_bucket.push((ordinal, item));
                    AttemptLaneV4::View
                }
                "clear" => {
                    clear_bucket.push((ordinal, item));
                    AttemptLaneV4::Clear
                }
                "state" => {
                    state_bucket.push((ordinal, item));
                    AttemptLaneV4::State
                }
                "visibility" => {
                    visibility_bucket.push((ordinal, item));
                    AttemptLaneV4::Visibility
                }
                _ => return Err(unknown_value()),
            },
            Some(_) => return Err(malformed()),
        };
        property_attempt_order.push(lane);
    }

    let mut held_attempts = Vec::new();
    let mut held_attempt_fields = Vec::new();

    if frame_bucket.len() > MAX_WORLD_FRAMES {
        return Err(section_over_cap());
    }
    let mut frames = Vec::new();
    let mut seen_frames = BTreeSet::new();
    for (ordinal, item) in frame_bucket {
        match decode_frame_attempt_v4(item, ordinal, &mut seen_frames)? {
            AttemptRowOutcome::Valid(attempt) => frames.push(attempt),
            AttemptRowOutcome::Held { row, fields } => {
                held_attempt_fields.push((AttemptLaneV4::Frame, row.ordinal, fields));
                held_attempts.push(row);
            }
        }
    }

    if member_bucket.len() > MAX_WORLD_MEMBER_ATTEMPTS {
        return Err(section_over_cap());
    }
    let mut member_attempts = Vec::new();
    let mut seen_members = BTreeSet::new();
    for (ordinal, item) in member_bucket {
        match decode_member_attempt_v4(item, ordinal, &mut seen_members)? {
            AttemptRowOutcome::Valid(attempt) => member_attempts.push(attempt),
            AttemptRowOutcome::Held { row, fields } => {
                held_attempt_fields.push((AttemptLaneV4::Member, row.ordinal, fields));
                held_attempts.push(row);
            }
        }
    }

    if view_bucket.len() > MAX_WORLD_VIEW_ATTEMPTS {
        return Err(section_over_cap());
    }
    let mut view_attempts = Vec::new();
    let mut seen_views = BTreeSet::new();
    for (ordinal, item) in view_bucket {
        match decode_view_attempt_v4(item, ordinal, &mut seen_views)? {
            AttemptRowOutcome::Valid(attempt) => view_attempts.push(attempt),
            AttemptRowOutcome::Held { row, fields } => {
                held_attempt_fields.push((AttemptLaneV4::View, row.ordinal, fields));
                held_attempts.push(row);
            }
        }
    }

    if clear_bucket.len() > MAX_WORLD_CLEAR_ATTEMPTS {
        return Err(section_over_cap());
    }
    let mut clear_attempts = Vec::new();
    let mut seen_clears = BTreeSet::new();
    for (ordinal, item) in clear_bucket {
        match decode_clear_attempt_v4(item, ordinal, &mut seen_clears)? {
            AttemptRowOutcome::Valid(attempt) => clear_attempts.push(attempt),
            AttemptRowOutcome::Held { row, fields } => {
                held_attempt_fields.push((AttemptLaneV4::Clear, row.ordinal, fields));
                held_attempts.push(row);
            }
        }
    }

    Ok((
        DecodedPropertyPrefix {
            frames,
            member_attempts,
            view_attempts,
            clear_attempts,
            held_attempts,
            held_attempt_fields,
            property_attempt_order,
        },
        PropertyAttemptTail {
            state_bucket,
            visibility_bucket,
        },
    ))
}

/// Decode `state -> visibility` after `creationAttempts`, enforcing per-kind
/// caps and producer order within each kind.
pub(crate) fn decode_property_attempts_tail(
    tail: PropertyAttemptTail<'_>,
    held_attempts: &mut Vec<HeldAttemptRow>,
    held_attempt_fields: &mut Vec<(AttemptLaneV4, usize, HeldFields)>,
) -> Result<DecodedPropertyTail> {
    if tail.state_bucket.len() > MAX_WORLD_STATE_ATTEMPTS {
        return Err(section_over_cap());
    }
    let mut state_attempts = Vec::new();
    let mut seen_states = BTreeSet::new();
    for (_, item) in tail.state_bucket {
        let attempt = decode_state_attempt(item)?;
        if !seen_states.insert((attempt.window.0.clone(), attempt.property)) {
            return Err(WorldProtocolError {
                kind: KIND_SNAPSHOT,
                message: "duplicate-state-attempt",
            });
        }
        state_attempts.push(attempt);
    }

    if tail.visibility_bucket.len() > MAX_WORLD_VISIBILITY_ATTEMPTS {
        return Err(section_over_cap());
    }
    let mut visibility_attempts = Vec::new();
    let mut seen_visibility = BTreeSet::new();
    for (ordinal, item) in tail.visibility_bucket {
        match decode_visibility_attempt_v4(item, ordinal, &mut seen_visibility)? {
            AttemptRowOutcome::Valid(attempt) => visibility_attempts.push(attempt),
            AttemptRowOutcome::Held { row, fields } => {
                held_attempt_fields.push((AttemptLaneV4::Visibility, row.ordinal, fields));
                held_attempts.push(row);
            }
        }
    }

    Ok(DecodedPropertyTail {
        state_attempts,
        visibility_attempts,
    })
}

fn check_kind(map: &serde_json::Map<String, serde_json::Value>, expected: &str) -> Result<()> {
    if req_str(map, "kind")? != expected {
        return Err(unknown_value());
    }
    Ok(())
}

/// Staged v4 frame-attempt decode: lenient `op`/`pre` (missing or bad-typed
/// `op`/`pre`, or `pre` without `op`) and any extra key hold the identifiable
/// row instead of refusing the envelope.
fn decode_frame_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_frames: &mut BTreeSet<String>,
) -> Result<AttemptRowOutcome<FrameAttempt>> {
    const SCHEMA: &[&str] = &[
        "kind",
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
    check_kind(map, "frame")?;
    let window = match map.get("win") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => WindowId(text.clone()),
        Some(serde_json::Value::String(_)) => return Err(unknown_value()),
        _ => return Err(malformed()),
    };
    let extra = map.keys().any(|key| !SCHEMA.contains(&key.as_str()));
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
        let (row, fields) =
            held_attempt_row(AttemptLaneV4::Frame, ordinal, map, SCHEMA, identity, None);
        return Ok(AttemptRowOutcome::Held { row, fields });
    }
    if let Some(op) = operation_id.as_deref()
        && !op.is_empty()
        && !is_opaque_id(op)
    {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "frame-operation-invalid",
        });
    }
    Ok(AttemptRowOutcome::Valid(FrameAttempt {
        window,
        target,
        setter,
        readback,
        dpi_before,
        dpi_after,
        attempt_ms,
        operation_id,
        prewrite,
    }))
}

pub fn decode_member_attempt(value: &serde_json::Value) -> Result<MemberAttempt> {
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &[
            "kind",
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
    check_kind(map, "member")?;
    Ok(MemberAttempt {
        operation_id: req_id(map, "op")?,
        window: WindowId(req_id(map, "win")?),
        target_output: OutputId(req_id(map, "out")?),
        target_workspace: req_id(map, "ws")?,
        setter: match req_str(map, "setter")? {
            "accepted" => MemberDisposition::Accepted,
            "refused" => MemberDisposition::Refused,
            "threw" => MemberDisposition::Threw,
            "already-at-target" => MemberDisposition::AlreadyAtTarget,
            _ => return Err(unknown_value()),
        },
        pre: opt_workspace_ref_or_null(map, "pre", false)?,
        post: opt_workspace_ref_or_null(map, "post", false)?,
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
}

fn decode_member_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_members: &mut BTreeSet<(String, String)>,
) -> Result<AttemptRowOutcome<MemberAttempt>> {
    const SCHEMA: &[&str] = &[
        "kind",
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
                let (row, fields) = held_attempt_row(
                    AttemptLaneV4::Member,
                    ordinal,
                    map,
                    SCHEMA,
                    vec![attempt.operation_id, attempt.window.0],
                    diagnostic,
                );
                return Ok(AttemptRowOutcome::Held { row, fields });
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
                    let (row, fields) = held_attempt_row(
                        AttemptLaneV4::Member,
                        ordinal,
                        map,
                        SCHEMA,
                        vec![op, win],
                        attempt_hold_scope(map),
                    );
                    Ok(AttemptRowOutcome::Held { row, fields })
                }
                _ => Err(reason),
            }
        }
    }
}

pub fn decode_view_attempt(value: &serde_json::Value) -> Result<ViewAttempt> {
    let map = as_object(value)?;
    check_keys_with_optional(
        map,
        &[
            "kind",
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
    check_kind(map, "view")?;
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

fn decode_view_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_views: &mut BTreeSet<String>,
) -> Result<AttemptRowOutcome<ViewAttempt>> {
    const SCHEMA: &[&str] = &[
        "kind",
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
                let (row, fields) = held_attempt_row(
                    AttemptLaneV4::View,
                    ordinal,
                    map,
                    SCHEMA,
                    vec![attempt.operation_id],
                    diagnostic,
                );
                return Ok(AttemptRowOutcome::Held { row, fields });
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
                    let (row, fields) = held_attempt_row(
                        AttemptLaneV4::View,
                        ordinal,
                        map,
                        SCHEMA,
                        vec![op],
                        attempt_hold_scope(map),
                    );
                    Ok(AttemptRowOutcome::Held { row, fields })
                }
                None => Err(reason),
            }
        }
    }
}

fn decode_clear_attempt(value: &serde_json::Value) -> Result<ClearAttempt> {
    let map = as_object(value)?;
    check_keys(
        map,
        &["kind", "op", "win", "setter", "pre", "post", "attemptMs"],
    )?;
    check_kind(map, "clear")?;
    Ok(ClearAttempt {
        operation_id: req_id(map, "op")?,
        window: WindowId(req_id(map, "win")?),
        setter: parse_frame_setter(req_str(map, "setter")?)?,
        pre_maximized: opt_bool_or_null(map, "pre")?,
        post_maximized: opt_bool_or_null(map, "post")?,
        attempt_ms: req_u64_str(map, "attemptMs")?,
    })
}

fn decode_clear_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_clears: &mut BTreeSet<(String, String)>,
) -> Result<AttemptRowOutcome<ClearAttempt>> {
    const SCHEMA: &[&str] = &["kind", "op", "win", "setter", "pre", "post", "attemptMs"];
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
                let (row, fields) = held_attempt_row(
                    AttemptLaneV4::Clear,
                    ordinal,
                    map,
                    SCHEMA,
                    vec![attempt.operation_id, attempt.window.0],
                    attempt_hold_scope(map),
                );
                return Ok(AttemptRowOutcome::Held { row, fields });
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
                    let (row, fields) = held_attempt_row(
                        AttemptLaneV4::Clear,
                        ordinal,
                        map,
                        SCHEMA,
                        vec![op, win],
                        attempt_hold_scope(map),
                    );
                    Ok(AttemptRowOutcome::Held { row, fields })
                }
                _ => Err(reason),
            }
        }
    }
}

/// Decode one raw native-state attempt row. Strict mechanical row: missing
/// keys, unknown keys, unknown property/setter tokens, or bad decimal
/// counters refuse.
pub fn decode_state_attempt(value: &serde_json::Value) -> Result<StateAttempt> {
    let map = as_object(value)?;
    check_keys(
        map,
        &[
            "kind",
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
    check_kind(map, "state")?;
    Ok(StateAttempt {
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

fn parse_visibility_fields(
    map: &serde_json::Map<String, serde_json::Value>,
    window: WindowId,
) -> Result<VisibilityAttempt> {
    check_kind(map, "visibility")?;
    let path = LastWritePath::parse(req_str(map, "path")?).ok_or_else(unknown_value)?;
    let setter = VisibilitySetter::parse(req_str(map, "setter")?).ok_or_else(unknown_value)?;
    let reason = VisibilityReason::parse(req_str(map, "reason")?).ok_or_else(unknown_value)?;
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
    Ok(VisibilityAttempt {
        window,
        target: req_bool(map, "target")?,
        revision: req_u64_str(map, "revision")?,
        path,
        setter,
        reason,
        pre,
        post,
    })
}

fn decode_visibility_attempt_v4(
    value: &serde_json::Value,
    ordinal: usize,
    seen_visibility: &mut BTreeSet<String>,
) -> Result<AttemptRowOutcome<VisibilityAttempt>> {
    const SCHEMA: &[&str] = &[
        "kind", "win", "target", "revision", "path", "setter", "reason", "pre", "post",
    ];
    let map = as_object(value)?;
    let window = match map.get("win") {
        Some(serde_json::Value::String(text)) if is_opaque_id(text) => WindowId(text.clone()),
        Some(serde_json::Value::String(_)) => return Err(unknown_value()),
        _ => return Err(malformed()),
    };
    if !seen_visibility.insert(window.0.clone()) {
        return Err(WorldProtocolError {
            kind: KIND_SNAPSHOT,
            message: "duplicate-visibility-attempt",
        });
    }
    let extra = map.keys().any(|key| !SCHEMA.contains(&key.as_str()));
    let parsed = if !extra {
        parse_visibility_fields(map, window.clone())
    } else {
        Err(malformed())
    };
    match parsed {
        Ok(attempt) => Ok(AttemptRowOutcome::Valid(attempt)),
        Err(_) => {
            let (row, mut fields) = held_attempt_row(
                AttemptLaneV4::Visibility,
                ordinal,
                map,
                SCHEMA,
                vec![window.0],
                None,
            );
            for (key, val) in &mut fields {
                if key == "pre" || key == "post" {
                    reorder_visibility_sample(val);
                }
            }
            Ok(AttemptRowOutcome::Held { row, fields })
        }
    }
}

fn encode_frame_attempt_v4(row: &FrameAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"kind\":\"frame\",\"win\":");
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

fn encode_member_attempt_v4(row: &MemberAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"kind\":\"member\",\"op\":");
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

fn encode_view_attempt_v4(row: &ViewAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"kind\":\"view\",\"op\":");
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

fn encode_clear_attempt_v4(row: &ClearAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"kind\":\"clear\",\"op\":");
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

fn encode_state_attempt(row: &StateAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"kind\":\"state\",\"win\":");
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

fn encode_visibility_attempt_v4(row: &VisibilityAttempt) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"kind\":\"visibility\",\"win\":");
    push_json_string(&mut out, &row.window.0);
    out.push_str(",\"target\":");
    out.push_str(if row.target { "true" } else { "false" });
    out.push_str(",\"revision\":\"");
    out.push_str(&row.revision.to_string());
    out.push_str("\",\"path\":");
    push_json_string(&mut out, row.path.as_str());
    out.push_str(",\"setter\":");
    push_json_string(&mut out, row.setter.as_str());
    out.push_str(",\"reason\":");
    push_json_string(&mut out, row.reason.as_str());
    out.push_str(",\"pre\":");
    match &row.pre {
        None => out.push_str("null"),
        Some(sample) => push_visibility_sample(&mut out, sample),
    }
    out.push_str(",\"post\":");
    match &row.post {
        None => out.push_str("null"),
        Some(sample) => push_visibility_sample(&mut out, sample),
    }
    out.push('}');
    Ok(out)
}

/// Reconstruct the required `propertyAttempts` array in exact decoded wire order.
pub(crate) fn encode_property_attempts_v4(
    out: &mut String,
    report: &tiler_core::world_report::WorldReportV4,
    order: &[AttemptLaneV4],
    residual_attempts: &[(AttemptLaneV4, usize, HeldFields)],
) -> Result<()> {
    out.push_str(",\"propertyAttempts\":[");
    let held_property: std::collections::BTreeMap<(AttemptLaneV4, usize), &HeldAttemptRow> = report
        .held_attempts
        .iter()
        .filter(|h| h.lane != AttemptLaneV4::Creation)
        .map(|h| ((h.lane, h.ordinal), h))
        .collect();
    if held_property.len()
        != report
            .held_attempts
            .iter()
            .filter(|h| h.lane != AttemptLaneV4::Creation)
            .count()
    {
        return Err(snapshot_invalid("held-ordinal-invalid"));
    }
    for &(_, ord) in held_property.keys() {
        if ord >= order.len() {
            return Err(snapshot_invalid("held-ordinal-invalid"));
        }
    }

    let mut frames = report.frames.iter();
    let mut members = report.member_attempts.iter();
    let mut views = report.view_attempts.iter();
    let mut clears = report.clear_attempts.iter();
    let mut states = report.state_attempts.iter();
    let mut visibilities = report.visibility_attempts.iter();

    let mut matched_held = 0usize;
    for (ordinal, &lane) in order.iter().enumerate() {
        if lane == AttemptLaneV4::Creation {
            return Err(snapshot_invalid("property-attempt-order-invalid"));
        }
        if ordinal > 0 {
            out.push(',');
        }
        if held_property.contains_key(&(lane, ordinal)) {
            let idx = residual_attempts
                .binary_search_by_key(&(lane, ordinal), |(l, o, _)| (*l, *o))
                .map_err(|_| snapshot_invalid("held-attempt-residual-missing"))?;
            matched_held += 1;
            out.push_str(&encode_held_fields(&residual_attempts[idx].2)?);
        } else {
            let encoded = match lane {
                AttemptLaneV4::Frame => encode_frame_attempt_v4(
                    frames
                        .next()
                        .ok_or_else(|| snapshot_invalid("property-attempt-order-invalid"))?,
                )?,
                AttemptLaneV4::Member => encode_member_attempt_v4(
                    members
                        .next()
                        .ok_or_else(|| snapshot_invalid("property-attempt-order-invalid"))?,
                )?,
                AttemptLaneV4::View => encode_view_attempt_v4(
                    views
                        .next()
                        .ok_or_else(|| snapshot_invalid("property-attempt-order-invalid"))?,
                )?,
                AttemptLaneV4::Clear => encode_clear_attempt_v4(
                    clears
                        .next()
                        .ok_or_else(|| snapshot_invalid("property-attempt-order-invalid"))?,
                )?,
                AttemptLaneV4::State => encode_state_attempt(
                    states
                        .next()
                        .ok_or_else(|| snapshot_invalid("property-attempt-order-invalid"))?,
                )?,
                AttemptLaneV4::Visibility => encode_visibility_attempt_v4(
                    visibilities
                        .next()
                        .ok_or_else(|| snapshot_invalid("property-attempt-order-invalid"))?,
                )?,
                AttemptLaneV4::Creation => unreachable!(),
            };
            out.push_str(&encoded);
        }
    }
    if matched_held != held_property.len()
        || frames.next().is_some()
        || members.next().is_some()
        || views.next().is_some()
        || clears.next().is_some()
        || states.next().is_some()
        || visibilities.next().is_some()
    {
        return Err(snapshot_invalid("property-attempt-order-invalid"));
    }
    out.push(']');
    Ok(())
}
