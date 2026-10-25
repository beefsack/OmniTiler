//! KDE raw classification capture: KWin-shaped native facts in, projected
//! v2 `WindowFact`s out. No core/policy changes: this calls the existing
//! shared [`tiler_core::size_hints`] fixed-size entry point, then shapes the
//! exact projected fact JSON the strict codec decodes. The broker invokes it
//! on the sole capture lane before stamping, sequencing, or draining.
//!
//! Explicit non-goals (deferred pending neutrality/product decisions): the
//! carried native role/control/ownership facts (`normalWindow` excepted,
//! which preserves the existing candidate filter) are strictly
//! validated but never mapped to verdicts here. No shared safety flags are
//! invented and no transient/modal window is floated: ordinary normal
//! windows keep the exact pre-cutover projection (intent, sticky, fixed),
//! and minimized rows stay retained facts.

use std::collections::HashSet;

use serde_json::{Map, Value};
use tiler_core::bounds::{GEOMETRY_BOUND, is_opaque_id, valid_carried_rect};
use tiler_core::size_hints::{FixedSizePredicate, WindowSizeHints, is_fixed_size_with};

/// Exact raw window keys, sorted. KWin-shaped native facts only: identity,
/// frame/client rects, client size hints, native membership, role, control,
/// ownership, and overlay state. Role/control/ownership facts (`normalWindow`
/// excepted) are strictly validated but deliberately unmapped (deferred
/// neutrality/product decision). No projected verdicts (`floating`,
/// `fitExcluded`, `fixedAuto`, `fixedSuppress`, `hints`, `rect`,
/// `observedWs`, `nativeWs` refuse as unknown-field) and no app identity
/// (fixture scope filtering stays adapter-side).
const RAW_KEYS: [&str; 19] = [
    "clientGeometry",
    "deleted",
    "desktops",
    "dialog",
    "frameGeometry",
    "fullScreen",
    "managed",
    "maxSize",
    "maximizeMode",
    "minSize",
    "minimized",
    "modal",
    "normalWindow",
    "onAllDesktops",
    "out",
    "transient",
    "transientFor",
    "wantsInput",
    "win",
];

/// Intent lane caps (mirror the adapter intent stores).
const MAX_INTENT_IDS: usize = 1024;
const MAX_DESKTOPS_PER_WINDOW: usize = 4096;

fn malformed() -> (&'static str, &'static str) {
    ("malformed", "request is malformed")
}

fn unknown_field() -> (&'static str, &'static str) {
    ("unknown-field", "request contains an unknown field")
}

fn unknown_value() -> (&'static str, &'static str) {
    ("unknown-value", "request contains an unknown value")
}

fn snapshot_invalid(msg: &'static str) -> (&'static str, &'static str) {
    ("snapshot-invalid", msg)
}

fn req_bool(map: &Map<String, Value>, key: &str) -> Result<bool, (&'static str, &'static str)> {
    match map.get(key) {
        Some(Value::Bool(v)) => Ok(*v),
        Some(_) => Err(unknown_value()),
        None => Err(malformed()),
    }
}

fn req_id(map: &Map<String, Value>, key: &str) -> Result<String, (&'static str, &'static str)> {
    match map.get(key) {
        Some(Value::String(s)) if is_opaque_id(s) => Ok(s.clone()),
        Some(Value::String(_)) => Err(unknown_value()),
        Some(_) => Err(malformed()),
        None => Err(malformed()),
    }
}

/// Established TS `Math.round` (Stage 3 coordinate proof): halves round
/// toward positive infinity (`Math.round(-10.5) === -10`). Rust
/// `f64::round` rounds half away from zero and must not be used here.
fn math_round(v: f64) -> Option<i32> {
    if !v.is_finite() {
        return None;
    }
    let floor = v.floor();
    let r = if v - floor < 0.5 { floor } else { floor + 1.0 };
    if r < f64::from(i32::MIN) || r > f64::from(i32::MAX) {
        return None;
    }
    Some(r as i32)
}

fn read_rect_f64(value: &Value) -> Result<(i32, i32, i32, i32), (&'static str, &'static str)> {
    let map = value.as_object().ok_or_else(malformed)?;
    if map.len() != 4
        || !map.contains_key("x")
        || !map.contains_key("y")
        || !map.contains_key("width")
        || !map.contains_key("height")
    {
        if map
            .keys()
            .any(|k| k != "x" && k != "y" && k != "width" && k != "height")
        {
            return Err(unknown_field());
        }
        return Err(malformed());
    }
    let num = |k: &str| -> Result<i32, (&'static str, &'static str)> {
        match map.get(k) {
            Some(Value::Number(n)) => n.as_f64().and_then(math_round).ok_or_else(unknown_value),
            Some(_) => Err(unknown_value()),
            None => Err(malformed()),
        }
    };
    let (x, y, w, h) = (num("x")?, num("y")?, num("width")?, num("height")?);
    if !valid_carried_rect(x, y, w, h) {
        return Err(unknown_value());
    }
    Ok((x, y, w, h))
}

/// Raw client rectangle: frame-space extents plus an unknown-delta marker.
type RawClient = Option<(i32, i32, i32, i32)>;

/// Optional client-rectangle read: finite native values with positive
/// extents only (matching the producer gate). Coordinate bounds are NOT
/// enforced here: a harmless client origin outside the world bound (or an
/// unrepresentable extent that fails to round) degrades to an unknown
/// delta instead of refusing the capture - an unusable optional hint
/// source must never fail the whole report. Malformed shapes (missing
/// keys, non-numeric values) still refuse; non-finite numbers are bad
/// values.
fn read_client_rect(value: &Value) -> Result<RawClient, (&'static str, &'static str)> {
    let map = value.as_object().ok_or_else(malformed)?;
    if map.len() != 4
        || !map.contains_key("x")
        || !map.contains_key("y")
        || !map.contains_key("width")
        || !map.contains_key("height")
    {
        if map
            .keys()
            .any(|k| k != "x" && k != "y" && k != "width" && k != "height")
        {
            return Err(unknown_field());
        }
        return Err(malformed());
    }
    let num = |k: &str| -> Result<f64, (&'static str, &'static str)> {
        match map.get(k) {
            Some(Value::Number(n)) => n
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(unknown_value),
            Some(_) => Err(unknown_value()),
            None => Err(malformed()),
        }
    };
    let (x, y, w, h) = (num("x")?, num("y")?, num("width")?, num("height")?);
    if !(w > 0.0 && h > 0.0) {
        return Ok(None);
    }
    let (x, y, w, h) = match (math_round(x), math_round(y), math_round(w), math_round(h)) {
        (Some(x), Some(y), Some(w), Some(h)) => (x, y, w, h),
        // Unrepresentable extents degrade to an unknown delta.
        _ => return Ok(None),
    };
    if w <= 0 || h <= 0 {
        return Ok(None);
    }
    Ok(Some((x, y, w, h)))
}

fn read_size_f64(value: &Value) -> Result<(i32, i32), (&'static str, &'static str)> {
    let map = value.as_object().ok_or_else(malformed)?;
    if map.len() != 2 || !map.contains_key("width") || !map.contains_key("height") {
        if map.keys().any(|k| k != "width" && k != "height") {
            return Err(unknown_field());
        }
        return Err(malformed());
    }
    let num = |k: &str| -> Result<i32, (&'static str, &'static str)> {
        match map.get(k) {
            Some(Value::Number(n)) => n.as_f64().and_then(math_round).ok_or_else(unknown_value),
            Some(_) => Err(unknown_value()),
            None => Err(malformed()),
        }
    };
    Ok((num("width")?, num("height")?))
}

/// Decorated-frame hint conversion for the projected frame-space hints:
/// meaningful client bounds
/// convert via the live frame/client delta; zero/sentinel values ride
/// verbatim; negatives never reach conversion (they ride as values and the
/// shared classifier reports `not-fixed-negative`); an unknown delta nulls
/// positive in-bound client bounds instead of mixing spaces.
fn convert_hint_bound(value: Option<i32>, delta: Option<i32>) -> Option<i32> {
    let v = value?;
    if v <= 0 || v > GEOMETRY_BOUND {
        return Some(v);
    }
    let d = delta?;
    let converted = v.checked_add(d)?;
    if converted <= 0 {
        return None;
    }
    Some(converted)
}

fn parse_intent_ids(
    value: Option<&Value>,
) -> Result<HashSet<String>, (&'static str, &'static str)> {
    let Some(value) = value else {
        return Ok(HashSet::new());
    };
    let items = value.as_array().ok_or_else(malformed)?;
    if items.len() > MAX_INTENT_IDS {
        return Err(unknown_value());
    }
    let mut set = HashSet::new();
    for item in items {
        match item {
            Value::String(s) if is_opaque_id(s) => {
                set.insert(s.clone());
            }
            Value::String(_) => return Err(unknown_value()),
            _ => return Err(malformed()),
        }
    }
    Ok(set)
}

/// Normalize one inner report in place: validate the strict raw `windows`
/// lane plus the tolerant intent lanes, then replace `windows` with the
/// projected v2 facts and strip the intent lanes before the strict codec.
/// Rejects before any sequencing, drain, watermark, or presentation change
/// (the caller remembers the rejection for exact replay).
pub fn normalize_raw_windows(
    inner: &mut Map<String, Value>,
) -> Result<(), (&'static str, &'static str)> {
    let predicate_wire = match inner.get("predicate") {
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err(unknown_value()),
        None => return Err(malformed()),
    };
    let predicate = FixedSizePredicate::parse_wire(&predicate_wire).ok_or_else(unknown_value)?;
    let host_owned = match inner.get("hostOwned") {
        Some(Value::Bool(v)) => *v,
        Some(_) => return Err(unknown_value()),
        None => return Err(malformed()),
    };
    let float_intent = parse_intent_ids(inner.get("floatIntent"))?;
    let tile_intent = parse_intent_ids(inner.get("tileIntent"))?;
    // No fixed-origins lane exists: the `fixedOriginOf` seam has no
    // producer, and tile suppression rides `tileIntent`. A carried
    // `fixedOrigins` key refuses as an unknown field (no silent second
    // verdict route).
    if inner.contains_key("fixedOrigins") {
        return Err(unknown_field());
    }

    let windows_value = inner.get("windows").ok_or_else(malformed)?;
    let rows = windows_value.as_array().ok_or_else(malformed)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut projected: Vec<Value> = Vec::with_capacity(rows.len());

    for row in rows {
        let map = row.as_object().ok_or_else(malformed)?;
        if map.len() != RAW_KEYS.len() || RAW_KEYS.iter().any(|k| !map.contains_key(*k)) {
            if map.keys().any(|k| !RAW_KEYS.contains(&k.as_str())) {
                return Err(unknown_field());
            }
            return Err(malformed());
        }
        let win = req_id(map, "win")?;
        let out = req_id(map, "out")?;
        if !seen.insert(win.clone()) {
            return Err(snapshot_invalid("duplicate-window"));
        }
        let normal_window = req_bool(map, "normalWindow")?;
        // Strictly validated but deliberately unmapped (deferred
        // role/control/ownership decision): bindings prefixed unused.
        let _managed = req_bool(map, "managed")?;
        let _deleted = req_bool(map, "deleted")?;
        let _transient = req_bool(map, "transient")?;
        let _dialog = req_bool(map, "dialog")?;
        let _modal = req_bool(map, "modal")?;
        let _wants_input = req_bool(map, "wantsInput")?;
        let full_screen = req_bool(map, "fullScreen")?;
        let on_all_desktops = req_bool(map, "onAllDesktops")?;
        let minimized = req_bool(map, "minimized")?;
        // Native maximize-mode enum (KWin `MaximizeMode`: 0 restore,
        // 1 vertical, 2 horizontal, 3 full). The capture producer holds
        // anything else client-side; a malformed raw numeric refuses here
        // as malformed (never a guessed overlay verdict).
        let maximize_mode = match map.get("maximizeMode") {
            Some(Value::Number(n)) => match n.as_i64() {
                Some(mode) if (0..=3).contains(&mode) => mode as i32,
                _ => return Err(malformed()),
            },
            Some(_) => return Err(unknown_value()),
            None => return Err(malformed()),
        };
        let _transient_for = match map.get("transientFor") {
            Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) => {
                if !is_opaque_id(s) {
                    return Err(unknown_value());
                }
                Some(s.clone())
            }
            Some(_) => return Err(malformed()),
            None => return Err(malformed()),
        };
        let (fx, fy, fw, fh) = match map.get("frameGeometry") {
            Some(v) => read_rect_f64(v)?,
            None => return Err(malformed()),
        };
        let client = match map.get("clientGeometry") {
            Some(Value::Null) => None,
            Some(v) => read_client_rect(v)?,
            None => return Err(malformed()),
        };
        let (raw_min_w, raw_min_h) = match map.get("minSize") {
            Some(Value::Null) => (None, None),
            Some(v) => {
                let (w, h) = read_size_f64(v)?;
                (Some(w), Some(h))
            }
            None => return Err(malformed()),
        };
        let (raw_max_w, raw_max_h) = match map.get("maxSize") {
            Some(Value::Null) => (None, None),
            Some(v) => {
                let (w, h) = read_size_f64(v)?;
                (Some(w), Some(h))
            }
            None => return Err(malformed()),
        };
        let desktops = match map.get("desktops") {
            Some(Value::Array(items)) => {
                if items.len() > MAX_DESKTOPS_PER_WINDOW {
                    return Err(unknown_value());
                }
                let mut ids = Vec::with_capacity(items.len());
                let mut seen = HashSet::new();
                for item in items {
                    match item {
                        Value::String(s) if is_opaque_id(s) => {
                            // Duplicate native ids must never collapse into
                            // a single home: refuse instead of guessing.
                            if !seen.insert(s.clone()) {
                                return Err(unknown_value());
                            }
                            ids.push(s.clone());
                        }
                        Value::String(_) => return Err(unknown_value()),
                        _ => return Err(malformed()),
                    }
                }
                ids
            }
            Some(_) => return Err(malformed()),
            None => return Err(malformed()),
        };

        // Preserve the existing normal-window candidate boundary. The
        // producer also retains that filter until role normalization is
        // resolved; minimized rows stay retained below. Native
        // role/control/ownership facts above are strictly validated but
        // deliberately not mapped to verdicts here (deferred decision).
        if !normal_window {
            continue;
        }

        let delta_w = client.and_then(|(_, _, cw, _)| fw.checked_sub(cw));
        let delta_h = client.and_then(|(_, _, _, ch)| fh.checked_sub(ch));
        let hints = WindowSizeHints {
            min_w: convert_hint_bound(raw_min_w, delta_w),
            min_h: convert_hint_bound(raw_min_h, delta_h),
            max_w: convert_hint_bound(raw_max_w, delta_w),
            max_h: convert_hint_bound(raw_max_h, delta_h),
        };
        // Fixed classification runs on client-size semantics (mirroring the
        // retired adapter gate): the raw client pair, absent when either
        // bound is negative or missing. Decoration preserves min==max
        // equality, but an unknown delta nulls the decorated pair, so
        // classifying the decorated pair would lose genuine fixed pins
        // (e.g. an undecorated 100x100 client). The decorated pair keeps
        // riding for core clamp assessment.
        let client_pair = |pair: Option<(i32, i32)>| -> (Option<i32>, Option<i32>) {
            match pair {
                Some((w, h)) if w >= 0 && h >= 0 => (Some(w), Some(h)),
                _ => (None, None),
            }
        };
        let (client_min_w, client_min_h) = client_pair(raw_min_w.zip(raw_min_h));
        let (client_max_w, client_max_h) = client_pair(raw_max_w.zip(raw_max_h));
        let fresh_fixed = is_fixed_size_with(
            WindowSizeHints {
                min_w: client_min_w,
                min_h: client_min_h,
                max_w: client_max_w,
                max_h: client_max_h,
            },
            predicate,
        );
        // No recorded fixed origins exist (the seam has no producer):
        // automatic status is the fresh classification, suppression is the
        // tile-override intent alone.
        let tile_override = tile_intent.contains(&win);
        let fixed_auto = fresh_fixed;
        let fixed_suppress = tile_override;

        let adapter_float = float_intent.contains(&win);
        let sticky = on_all_desktops;
        let fullscreen = full_screen;
        let maximized = maximize_mode != 0;
        let floating = adapter_float || sticky || (fixed_auto && !tile_override);
        let fit_excluded = floating || sticky || fullscreen || maximized;
        let native_ws = if host_owned && desktops.len() == 1 {
            Some(desktops[0].clone())
        } else {
            None
        };

        let hints_json = serde_json::json!({
            "minW": hints.min_w,
            "minH": hints.min_h,
            "maxW": hints.max_w,
            "maxH": hints.max_h,
        });
        projected.push(serde_json::json!({
            "win": win,
            "out": out,
            "rect": [fx, fy, fw, fh],
            "floating": floating,
            "fitExcluded": fit_excluded,
            "fullscreen": fullscreen,
            "maximized": maximized,
            "sticky": sticky,
            "fixedAuto": fixed_auto,
            "fixedSuppress": fixed_suppress,
            "minimized": minimized,
            "hints": hints_json,
            "observedWs": null,
            "nativeWs": native_ws,
        }));
    }

    inner.insert("windows".to_owned(), Value::Array(projected));
    inner.remove("floatIntent");
    inner.remove("tileIntent");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_inner(windows: Value) -> Map<String, Value> {
        let mut inner = Map::new();
        inner.insert("v".to_owned(), json!(2));
        inner.insert("windows".to_owned(), windows);
        inner.insert("predicate".to_owned(), json!("both-axes-fixed"));
        inner.insert("hostOwned".to_owned(), json!(true));
        inner
    }

    fn raw_win(win: &str) -> Value {
        json!({
            "win": win,
            "out": "out-1",
            "frameGeometry": {"x": 0, "y": 0, "width": 200, "height": 200},
            "clientGeometry": {"x": 0, "y": 0, "width": 200, "height": 200},
            "minSize": null,
            "maxSize": null,
            "desktops": ["ws-1"],
            "normalWindow": true,
            "managed": true,
            "deleted": false,
            "transient": false,
            "transientFor": null,
            "dialog": false,
            "modal": false,
            "wantsInput": true,
            "fullScreen": false,
            "maximizeMode": 0,
            "onAllDesktops": false,
            "minimized": false,
        })
    }

    #[test]
    fn projects_plain_tiled_row() {
        let mut inner = base_inner(json!([raw_win("win-a")]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let windows = inner["windows"].as_array().expect("array");
        assert_eq!(windows.len(), 1);
        let row = &windows[0];
        assert_eq!(row["win"], json!("win-a"));
        assert_eq!(row["rect"], json!([0, 0, 200, 200]));
        assert_eq!(row["floating"], json!(false));
        assert_eq!(row["nativeWs"], json!("ws-1"));
        assert_eq!(row["observedWs"], Value::Null);
        assert!(!inner.contains_key("floatIntent"));
    }

    #[test]
    fn drops_non_normal_windows() {
        let mut bad = raw_win("win-a");
        bad["normalWindow"] = json!(false);
        let mut inner = base_inner(json!([bad]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        assert_eq!(inner["windows"].as_array().expect("array").len(), 0);
    }

    #[test]
    fn retains_minimized_facts() {
        let mut w = raw_win("win-min");
        w["minimized"] = json!(true);
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let windows = inner["windows"].as_array().expect("array");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0]["minimized"], json!(true));
    }

    #[test]
    fn classifies_equal_nonzero_pin_as_fixed() {
        let mut w = raw_win("win-fixed");
        w["minSize"] = json!({"width": 400, "height": 300});
        w["maxSize"] = json!({"width": 400, "height": 300});
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(row["fixedAuto"], json!(true));
        assert_eq!(row["floating"], json!(true));
        assert_eq!(
            row["hints"],
            json!({"minW": 400, "minH": 300, "maxW": 400, "maxH": 300})
        );
    }

    #[test]
    fn converts_client_hints_with_frame_delta() {
        let mut w = raw_win("win-hints");
        w["frameGeometry"] = json!({"x": 10.6, "y": 20.4, "width": 204.4, "height": 104.5});
        w["clientGeometry"] = json!({"x": 12.6, "y": 48.4, "width": 200.4, "height": 70.5});
        w["minSize"] = json!({"width": 176, "height": 150});
        w["maxSize"] = json!({"width": 800, "height": 600});
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(row["rect"], json!([11, 20, 204, 105]));
        assert_eq!(
            row["hints"],
            json!({"minW": 180, "minH": 184, "maxW": 804, "maxH": 634})
        );
    }

    #[test]
    fn rides_sentinels_verbatim_for_core_sanitize() {
        let mut w = raw_win("win-s");
        w["minSize"] = json!({"width": 0, "height": 0});
        w["maxSize"] = json!({"width": 16777215, "height": 16777215});
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(row["fixedAuto"], json!(false));
        assert_eq!(
            row["hints"],
            json!({"minW": 0, "minH": 0, "maxW": 16777215, "maxH": 16777215})
        );
    }

    #[test]
    fn nulls_converted_bounds_without_client_rect() {
        let mut w = raw_win("win-n");
        w["clientGeometry"] = Value::Null;
        w["minSize"] = json!({"width": 176, "height": 150});
        w["maxSize"] = json!({"width": 800, "height": 600});
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(
            row["hints"],
            json!({"minW": null, "minH": null, "maxW": null, "maxH": null})
        );
    }

    #[test]
    fn rejects_legacy_verdict_keys_extra_and_duplicates() {
        let mut w = raw_win("win-a");
        w.as_object_mut()
            .expect("obj")
            .insert("floating".to_owned(), json!(false));
        let mut inner = base_inner(json!([w]));
        assert_eq!(normalize_raw_windows(&mut inner), Err(unknown_field()));
        let mut inner = base_inner(json!([raw_win("win-a"), raw_win("win-a")]));
        assert_eq!(
            normalize_raw_windows(&mut inner),
            Err(snapshot_invalid("duplicate-window"))
        );
        let mut bad = raw_win("win-a");
        bad.as_object_mut().expect("obj").remove("out");
        let mut inner = base_inner(json!([bad]));
        assert_eq!(normalize_raw_windows(&mut inner), Err(malformed()));
        let mut bad = raw_win("win-a");
        bad["win"] = json!("has space");
        let mut inner = base_inner(json!([bad]));
        assert_eq!(normalize_raw_windows(&mut inner), Err(unknown_value()));
    }

    #[test]
    fn fixed_pin_without_client_delta_still_classifies_on_client_sizes() {
        // Retired adapter semantics: the fixed predicate runs on the raw
        // client extents, so an undecorated equal pin classifies even
        // though the decorated pair nulls without a delta.
        let mut w = raw_win("win-c");
        w["minSize"] = json!({"width": 100, "height": 100});
        w["maxSize"] = json!({"width": 100, "height": 100});
        w["clientGeometry"] = Value::Null;
        w["frameGeometry"] = json!({"x": 1050, "y": 150, "width": 100, "height": 100});
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(row["fixedAuto"], json!(true));
        assert_eq!(row["floating"], json!(true));
        assert_eq!(
            row["hints"],
            json!({"minW": null, "minH": null, "maxW": null, "maxH": null})
        );
    }

    #[test]
    fn kde_raw_corpus_matches_real_broker() {
        // Exact production raw shapes with authoritative expectations,
        // run through the real `normalize_raw_windows` (not the test-only
        // TS transport glue, which is never compared against this corpus).
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../test-fixtures/kde-raw-corpus.json"))
                .expect("kde raw corpus parses");
        let entries = fixture.as_array().expect("corpus is an array");
        assert!(!entries.is_empty(), "corpus is non-empty");
        for entry in entries {
            let name = entry
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<unnamed>");
            let mut inner = Map::new();
            inner.insert("v".to_owned(), serde_json::json!(2));
            inner.insert(
                "predicate".to_owned(),
                entry
                    .get("predicate")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            );
            inner.insert(
                "hostOwned".to_owned(),
                entry
                    .get("hostOwned")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            );
            inner.insert(
                "floatIntent".to_owned(),
                entry
                    .get("floatIntent")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            );
            inner.insert(
                "tileIntent".to_owned(),
                entry
                    .get("tileIntent")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            );
            for (key, value) in entry.as_object().expect("entry is an object").iter() {
                if ![
                    "name",
                    "predicate",
                    "hostOwned",
                    "floatIntent",
                    "tileIntent",
                    "raw",
                    "projected",
                    "error",
                ]
                .contains(&key.as_str())
                {
                    // Extra top-level lanes (e.g. fixedOrigins) ride into
                    // the inner report to prove they refuse.
                    inner.insert(key.clone(), value.clone());
                }
            }
            inner.insert(
                "windows".to_owned(),
                serde_json::Value::Array(vec![
                    entry.get("raw").cloned().expect("entry carries raw"),
                ]),
            );
            let outcome = normalize_raw_windows(&mut inner);
            match entry.get("error") {
                Some(serde_json::Value::Null) | None => {
                    outcome.unwrap_or_else(|e| panic!("{name} normalizes: {e:?}"));
                    let windows = inner["windows"].as_array().expect("array");
                    match entry.get("projected") {
                        Some(serde_json::Value::Null) | None => {
                            assert!(windows.is_empty(), "{name} drops");
                        }
                        Some(expected) => {
                            assert_eq!(windows.len(), 1, "{name}");
                            assert_eq!(&windows[0], expected, "{name}");
                        }
                    }
                    assert!(
                        !inner.contains_key("floatIntent") && !inner.contains_key("tileIntent"),
                        "{name} strips intent lanes"
                    );
                }
                Some(expected) => {
                    let (kind, message) = outcome.expect_err(&format!("{name} refuses"));
                    assert_eq!(kind, expected["kind"].as_str().expect("kind"), "{name}");
                    assert_eq!(
                        message,
                        expected["message"].as_str().expect("message"),
                        "{name}"
                    );
                }
            }
        }
    }

    #[test]
    fn math_round_matches_ts_at_negative_half() {
        // Established TS `Math.round`: halves round toward +inf.
        assert_eq!(math_round(-10.5), Some(-10));
        assert_eq!(math_round(-0.5), Some(0));
        assert_eq!(math_round(2.5), Some(3));
        assert_eq!(math_round(10.6), Some(11));
        assert_eq!(math_round(0.49999999999999994), Some(0));
        assert_eq!(math_round(f64::NAN), None);
        assert_eq!(math_round(f64::INFINITY), None);
        // A negative-half frame origin rounds up, never away from zero.
        let mut w = raw_win("win-neg");
        w["frameGeometry"] = json!({"x": -10.5, "y": 0.0, "width": 200.0, "height": 200.0});
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(row["rect"], json!([-10, 0, 200, 200]));
    }

    #[test]
    fn maximize_mode_is_exact_enum_zero_to_three() {
        for mode in [0, 1, 2, 3] {
            let mut w = raw_win("win-a");
            w["maximizeMode"] = json!(mode);
            let mut inner = base_inner(json!([w]));
            normalize_raw_windows(&mut inner).expect("normalizes");
            let row = &inner["windows"].as_array().expect("array")[0];
            assert_eq!(row["maximized"], json!(mode != 0));
        }
        // Out-of-enum numerics refuse as malformed raw, never a guess.
        for bad in [json!(4), json!(-1), json!(1.5)] {
            let mut w = raw_win("win-a");
            w["maximizeMode"] = bad;
            let mut inner = base_inner(json!([w]));
            assert_eq!(normalize_raw_windows(&mut inner), Err(malformed()));
        }
    }

    #[test]
    fn duplicate_and_invalid_desktops_never_become_single_home() {
        // Duplicate native ids refuse instead of collapsing to one home.
        let mut w = raw_win("win-a");
        w["desktops"] = json!(["ws-1", "ws-1"]);
        let mut inner = base_inner(json!([w]));
        assert_eq!(normalize_raw_windows(&mut inner), Err(unknown_value()));
        // Invalid desktop entries refuse (never silently dropped).
        let mut w = raw_win("win-a");
        w["desktops"] = json!(["ws-1", "has space"]);
        let mut inner = base_inner(json!([w]));
        assert_eq!(normalize_raw_windows(&mut inner), Err(unknown_value()));
        let mut w = raw_win("win-a");
        w["desktops"] = json!(["ws-1", 7]);
        let mut inner = base_inner(json!([w]));
        assert_eq!(normalize_raw_windows(&mut inner), Err(malformed()));
        // Multi-homed rows stay unknown, never a fabricated single home.
        let mut w = raw_win("win-a");
        w["desktops"] = json!(["ws-1", "ws-2"]);
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(row["nativeWs"], Value::Null);
    }

    #[test]
    fn client_rect_ignores_harmless_origin_and_degrades_unrepresentable() {
        // A far-away client origin leaves the width delta unaffected.
        let mut w = raw_win("win-a");
        w["clientGeometry"] = json!({"x": 1e9, "y": 0.0, "width": 200.0, "height": 200.0});
        w["minSize"] = json!({"width": 100, "height": 100});
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(
            row["hints"],
            json!({"minW": 100, "minH": 100, "maxW": null, "maxH": null})
        );
        // Unrepresentable extents degrade to an unknown delta: the capture
        // is accepted with nulled converted bounds, never refused.
        let mut w = raw_win("win-b");
        w["clientGeometry"] = json!({"x": 0.0, "y": 0.0, "width": 1e30, "height": 1e30});
        w["minSize"] = json!({"width": 100, "height": 100});
        let mut inner = base_inner(json!([w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(
            row["hints"],
            json!({"minW": null, "minH": null, "maxW": null, "maxH": null})
        );
    }

    #[test]
    fn roles_carry_without_verdicts_normal_only_preserved() {
        // Native transient/modal facts ride the lane but float nothing:
        // ordinary normal windows keep the exact pre-cutover projection.
        for (key, value) in [
            ("transient", json!(true)),
            ("modal", json!(true)),
            ("dialog", json!(true)),
        ] {
            let mut w = raw_win("win-a");
            w[key] = value;
            let mut inner = base_inner(json!([w]));
            normalize_raw_windows(&mut inner).expect("normalizes");
            let row = &inner["windows"].as_array().expect("array")[0];
            assert_eq!(row["floating"], json!(false), "{key}");
            assert_eq!(row["fixedAuto"], json!(false), "{key}");
        }
        // Even a transient window with a known owner stays unmanaged by
        // verdict: carried facts, no invented float.
        let mut w = raw_win("win-t");
        w["transient"] = json!(true);
        w["transientFor"] = json!("win-a");
        let mut inner = base_inner(json!([raw_win("win-a"), w]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let windows = inner["windows"].as_array().expect("array");
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[1]["floating"], json!(false));
    }

    #[test]
    fn honors_intent_lanes_and_rejects_fixed_origins() {
        let mut inner = base_inner(json!([raw_win("win-a")]));
        inner.insert("floatIntent".to_owned(), json!(["win-a"]));
        inner.insert("tileIntent".to_owned(), json!([]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(row["floating"], json!(true));
        assert_eq!(row["fixedAuto"], json!(false));
        // Tile suppression rides the tile intent lane alone.
        let mut inner = base_inner(json!([raw_win("win-a")]));
        inner.insert("tileIntent".to_owned(), json!(["win-a"]));
        normalize_raw_windows(&mut inner).expect("normalizes");
        let row = &inner["windows"].as_array().expect("array")[0];
        assert_eq!(row["fixedSuppress"], json!(true));
        // The origins lane has no producer and refuses when carried.
        let mut inner = base_inner(json!([raw_win("win-a")]));
        inner.insert(
            "fixedOrigins".to_owned(),
            json!([{"win": "win-a", "auto": true, "suppress": false}]),
        );
        assert_eq!(normalize_raw_windows(&mut inner), Err(unknown_field()));
    }
}
