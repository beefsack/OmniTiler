//! Broker-owned KDE gesture normalization (Stage 4, capture-only lane):
//! raw native `nativeGesture` snapshot plus raw script cursor/windowAt facts
//! in, core `GestureIngress` out. No core/policy changes: the broker maps
//! the effect-owned identity to the existing core wire id per broker
//! generation, then strips the raw lane before the strict codec.
//!
//! Ownership: the script never mints. A non-null script `gestureIngress`
//! refuses; the broker fills it from the raw lane. Invalid or unreadable
//! script facts read as unknown (never a refusal, never an invention):
//! terminal snapshots freeze the native finish cursor (no later script
//! pointer override), open snapshots prefer the same-capture readable
//! script cursor and fall back to native only when the script point is
//! unknown. `cursorWindow` (the live `windowAt` fact) is validated for
//! shape then stripped: advisory only, never a mover/target override and
//! never carried into core (no core hit-target field exists).
//!
//! Identity map (per broker generation; rotation replaces the whole broker
//! so the map resets with the engine): native {instance, serial} pairs
//! allocate fresh nonzero u64 core ids in first-seen order, never reused,
//! so a source restart (new instance, serials from 1) can never collide
//! with an older terminal. Serial "0" (idle) never allocates.
//!
//! Arrival-order fences (all checked before any mutation, so a rejected
//! capture never mutates the map). Frozen start facts per identity: a later
//! snapshot for a mapped identity with different window, kind, start frame,
//! start cursor, or modal truth fails the whole capture closed. Stale
//! serials: an unmapped identity whose serial is at or below the
//! per-instance floor omits truthfully (no newer core id by arrival order).
//! Retired instances: once a newer source instance is seen, late replies
//! naming an older instance omit unless already mapped (no rebind).
//! Revision staleness: a snapshot older than the last revision recorded for
//! its identity omits (never rewinds). The map stays bounded: at most
//! [`MAX_GESTURE_IDENTITIES`] mapped identities with oldest-first eviction
//! (the per-instance serial floor is preserved for evicted identities, so
//! replays stay stale and never rebind), and at most
//! [`MAX_GESTURE_INSTANCES`] tracked instances. A genuinely new source
//! past the instance cap exhausts via `Rotate` (safe fresh generation)
//! rather than evicting anti-replay evidence or admitting the old source as
//! new. Counter exhaustion is checked before any state mutation.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::{Map, Value};
use tiler_core::bounds::is_opaque_id;

/// Bounded canonical decimal `u64` string (mirrors the broker gate: JSON
/// numbers, signs, leading zeros, and overflow refuse). Local mirror of the
/// broker helper so this module path-includes into the test-only eval seam
/// without widening any public API.
fn is_canonical_u64(text: &str) -> bool {
    if text.is_empty() || text.len() > 20 {
        return false;
    }
    if text == "0" {
        return true;
    }
    let mut bytes = text.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_digit() && first != b'0' => {}
        _ => return false,
    }
    if !text.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if text.len() == 20 && text > u64::MAX.to_string().as_str() {
        return false;
    }
    true
}

/// Cap for mapped native gesture identities per broker generation.
/// Oldest-first eviction; evicted replays stay stale through the
/// per-instance serial floor, never rebound to a newer core id.
pub const MAX_GESTURE_IDENTITIES: usize = 256;
/// Cap for tracked source instances per broker generation. Oldest-first
/// eviction; effect restarts are rare, so this bound is never approached
/// in practice.
pub const MAX_GESTURE_INSTANCES: usize = 16;

/// Broker-only bound actuator association for one normalized ingress:
/// the exact native identity plus the broker core id it mapped to. Cached
/// in the exact bound response replay; absent on every omitted or
/// unsupported lane (idle/unavailable/superseded/stale/retired). The
/// script requires `GestureSettled.id == association.coreId` AND the
/// flight native pair `== association` native pair before any ack fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GestureAssoc {
    pub instance: String,
    pub serial: String,
    pub core_id: u64,
}

/// Gesture lane failure: a rejectable capture (in-band rejection, no map
/// install) or a map-source-cap exhaustion (safe fresh generation via the
/// existing `BrokerError::Rotate`, never unbounded state and never silently
/// sacrificing anti-replay).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GestureLaneError {
    Reject(&'static str, &'static str),
    Rotate,
}

fn reject(err: (&'static str, &'static str)) -> GestureLaneError {
    GestureLaneError::Reject(err.0, err.1)
}

fn malformed() -> (&'static str, &'static str) {
    ("malformed", "request is malformed")
}

fn unknown_value() -> (&'static str, &'static str) {
    ("unknown-value", "request contains an unknown value")
}

fn snapshot_invalid(msg: &'static str) -> (&'static str, &'static str) {
    ("snapshot-invalid", msg)
}

fn is_int_coord(n: &Value) -> Option<i32> {
    match n {
        Value::Number(v) => v.as_i64().and_then(|x| {
            if (-16384..=16384).contains(&x) {
                Some(x as i32)
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn read_point(value: &Value) -> Result<Option<[i32; 2]>, (&'static str, &'static str)> {
    match value {
        Value::Null => Ok(None),
        Value::Array(items) if items.len() == 2 => {
            let x = is_int_coord(&items[0]).ok_or_else(unknown_value)?;
            let y = is_int_coord(&items[1]).ok_or_else(unknown_value)?;
            Ok(Some([x, y]))
        }
        Value::Array(_) => Err(unknown_value()),
        _ => Err(malformed()),
    }
}

fn read_rect(value: &Value) -> Result<Option<[i32; 4]>, (&'static str, &'static str)> {
    match value {
        Value::Null => Ok(None),
        Value::Array(items) if items.len() == 4 => {
            let x = is_int_coord(&items[0]).ok_or_else(unknown_value)?;
            let y = is_int_coord(&items[1]).ok_or_else(unknown_value)?;
            let w = match &items[2] {
                Value::Number(v) => v
                    .as_i64()
                    .filter(|w| (1..=16384).contains(w))
                    .map(|w| w as i32),
                _ => None,
            }
            .ok_or_else(unknown_value)?;
            let h = match &items[3] {
                Value::Number(v) => v
                    .as_i64()
                    .filter(|h| (1..=16384).contains(h))
                    .map(|h| h as i32),
                _ => None,
            }
            .ok_or_else(unknown_value)?;
            Ok(Some([x, y, w, h]))
        }
        Value::Array(_) => Err(unknown_value()),
        _ => Err(malformed()),
    }
}

fn read_flag(map: &Map<String, Value>, key: &str) -> Result<bool, (&'static str, &'static str)> {
    match map.get(key) {
        Some(Value::Number(n)) => match n.as_u64() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            _ => Err(unknown_value()),
        },
        Some(_) => Err(malformed()),
        None => Err(malformed()),
    }
}

fn req_counter(
    map: &Map<String, Value>,
    key: &str,
) -> Result<(String, u64), (&'static str, &'static str)> {
    match map.get(key) {
        Some(Value::String(s)) if is_canonical_u64(s) => {
            let num = s.parse::<u64>().map_err(|_| unknown_value())?;
            Ok((s.clone(), num))
        }
        Some(Value::String(_)) => Err(unknown_value()),
        Some(_) => Err(malformed()),
        None => Err(malformed()),
    }
}

/// Frozen native start facts for one mapped identity. Compared exactly on
/// every later snapshot for the same identity; any drift fails closed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FrozenStart {
    win: String,
    kind: String,
    start: [i32; 4],
    start_cursor: Option<[i32; 2]>,
    modal: bool,
}

#[derive(Debug, Clone)]
struct IdentityRecord {
    core_id: u64,
    frozen: FrozenStart,
    last_revision: u64,
}

/// Broker-owned gesture identity map for one broker generation.
#[derive(Debug, Clone)]
pub struct KdeGestureMap {
    next_core_id: u64,
    by_key: HashMap<(String, String), IdentityRecord>,
    order: VecDeque<(String, String)>,
    instance_floor: HashMap<String, u64>,
    instance_order: VecDeque<String>,
    latest_instance: Option<String>,
    retired: HashSet<String>,
}

impl KdeGestureMap {
    /// Fresh map: core ids start at 1 (0 never mints).
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_core_id: 1,
            by_key: HashMap::new(),
            order: VecDeque::new(),
            instance_floor: HashMap::new(),
            instance_order: VecDeque::new(),
            latest_instance: None,
            retired: HashSet::new(),
        }
    }

    /// Normalize one inner report in place: validate the capture-only raw
    /// `nativeGesture`/`cursorWindow` lane, map it to the core
    /// `gestureIngress` lane, and strip the raw keys before the strict
    /// codec. A non-null script `gestureIngress` refuses (the script never
    /// mints). Omissions (idle/unavailable/superseded/stale/retired) set a
    /// null ingress truthfully and return no association. Rejects before
    /// any map mutation; exhaustion (`Rotate`) also precedes mutation.
    pub fn normalize_gesture_lane(
        &mut self,
        inner: &mut Map<String, Value>,
    ) -> Result<Option<GestureAssoc>, GestureLaneError> {
        // Script ownership fence: only null or missing rides here. A
        // present value of any other shape refuses (an object would be a
        // minted ingress; anything else is malformed).
        match inner.get("gestureIngress") {
            None | Some(Value::Null) => {}
            Some(Value::Object(_)) => {
                return Err(reject(unknown_value()));
            }
            Some(_) => return Err(reject(malformed())),
        }
        let raw = match inner.get("nativeGesture") {
            None | Some(Value::Null) => None,
            Some(Value::Object(map)) => Some(map.clone()),
            Some(_) => return Err(reject(malformed())),
        };
        // Raw script windowAt fact (advisory only, validated then stripped):
        // an unreadable or invalid value reads as unknown, never a refusal
        // and never a mover/target override. There is no core hit-target
        // field; the broker never carries this into `gestureIngress`.
        // A mistyped value is a producer bug.
        let _cursor_window: Option<String> = match inner.get("cursorWindow") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) if is_opaque_id(s) => Some(s.clone()),
            Some(Value::String(_)) => None,
            Some(_) => return Err(reject(malformed())),
        };
        // Same-capture script cursor (tolerant read; the strict codec owns
        // the top-level refusal): only a well-formed point folds.
        let script_cursor: Option<[i32; 2]> = match inner.get("cursor") {
            None | Some(Value::Null) => None,
            Some(Value::Array(items)) if items.len() == 2 => {
                match (is_int_coord(&items[0]), is_int_coord(&items[1])) {
                    (Some(x), Some(y)) => Some([x, y]),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(raw) = raw else {
            inner.remove("nativeGesture");
            inner.remove("cursorWindow");
            return Ok(None);
        };
        // Strict versioned parse of one gesture-facts snapshot (v:1).
        if raw.get("v").and_then(serde_json::Value::as_u64) != Some(1) {
            return Err(reject(unknown_value()));
        }
        let instance = match raw.get("instance") {
            Some(Value::String(s)) if !s.is_empty() && s.len() <= 128 && is_opaque_id(s) => {
                s.clone()
            }
            Some(Value::String(_)) => return Err(reject(unknown_value())),
            Some(_) => return Err(reject(malformed())),
            None => return Err(reject(malformed())),
        };
        let (serial, serial_num) = req_counter(&raw, "serial").map_err(reject)?;
        let (_revision, revision_num) = req_counter(&raw, "revision").map_err(reject)?;
        let phase = match raw.get("phase").and_then(Value::as_str) {
            Some("started") | Some("updated") | Some("released") | Some("cancelled")
            | Some("idle") => raw
                .get("phase")
                .and_then(Value::as_str)
                .expect("phase is str")
                .to_owned(),
            Some(_) => return Err(reject(unknown_value())),
            None => return Err(reject(malformed())),
        };
        let win = match raw.get("win") {
            Some(Value::String(s)) if s.is_empty() => String::new(),
            Some(Value::String(s)) if is_opaque_id(s) => s.clone(),
            Some(Value::String(_)) => return Err(reject(unknown_value())),
            Some(_) => return Err(reject(malformed())),
            None => return Err(reject(malformed())),
        };
        let kind = match raw.get("kind").and_then(Value::as_str) {
            Some("") | Some("move") | Some("resize") => raw
                .get("kind")
                .and_then(Value::as_str)
                .expect("kind is str")
                .to_owned(),
            Some(_) => return Err(reject(unknown_value())),
            None => return Err(reject(malformed())),
        };
        let start_rect = read_rect(raw.get("startRect").unwrap_or(&Value::Null)).map_err(reject)?;
        let start_cursor =
            read_point(raw.get("startCursor").unwrap_or(&Value::Null)).map_err(reject)?;
        let cursor = read_point(raw.get("cursor").unwrap_or(&Value::Null)).map_err(reject)?;
        let modal = read_flag(&raw, "modalMove").map_err(reject)?;
        let changed_wire = read_flag(&raw, "changed").map_err(reject)?;
        let _changed: bool = changed_wire;
        let superseded = read_flag(&raw, "superseded").map_err(reject)?;
        let available = read_flag(&raw, "available").map_err(reject)?;
        let idle = phase == "idle";
        if idle {
            if serial != "0" || !win.is_empty() || !kind.is_empty() {
                return Err(reject(snapshot_invalid("idle gesture carries facts")));
            }
            if start_rect.is_some() || start_cursor.is_some() || cursor.is_some() {
                return Err(reject(snapshot_invalid("idle gesture carries facts")));
            }
        } else {
            if serial == "0" || win.is_empty() || kind.is_empty() {
                return Err(reject(snapshot_invalid("open gesture lacks identity")));
            }
            if start_rect.is_none() {
                return Err(reject(snapshot_invalid("open gesture lacks start facts")));
            }
        }
        // Omission lanes (no map mutation, mapping persists): idle,
        // unavailable, and superseded snapshots never mint or resolve.
        if idle || !available || superseded {
            inner.insert("gestureIngress".to_owned(), Value::Null);
            inner.remove("nativeGesture");
            inner.remove("cursorWindow");
            return Ok(None);
        }
        let frozen = FrozenStart {
            win,
            kind,
            start: start_rect.expect("open snapshots carry startRect"),
            start_cursor,
            modal,
        };
        let terminal = phase == "released" || phase == "cancelled";
        // Resolve the broker core id without mutating on any stale path.
        let key = (instance.clone(), serial.clone());
        let core_id = if let Some(record) = self.by_key.get_mut(&key) {
            if record.frozen != frozen {
                return Err(reject(snapshot_invalid(
                    "gesture start facts changed for identity",
                )));
            }
            if revision_num < record.last_revision {
                inner.insert("gestureIngress".to_owned(), Value::Null);
                inner.remove("nativeGesture");
                inner.remove("cursorWindow");
                return Ok(None);
            }
            record.last_revision = revision_num;
            record.core_id
        } else {
            if self.retired.contains(&instance) {
                inner.insert("gestureIngress".to_owned(), Value::Null);
                inner.remove("nativeGesture");
                inner.remove("cursorWindow");
                return Ok(None);
            }
            let floor = self.instance_floor.get(&instance).copied().unwrap_or(0);
            if serial_num <= floor {
                inner.insert("gestureIngress".to_owned(), Value::Null);
                inner.remove("nativeGesture");
                inner.remove("cursorWindow");
                return Ok(None);
            }
            // Checked exhaustion before ANY state mutation: a fresh
            // identity past the core-id space, or a genuinely new source
            // past the tracked-instance cap, fails closed before retiring
            // sources, evicting identities, or minting. Instance-cap
            // exhaustion rotates (safe fresh generation); the retired and
            // floor evidence is never evicted to admit an old source as
            // new.
            if self.next_core_id == u64::MAX {
                return Err(reject(snapshot_invalid("gesture identity exhausted")));
            }
            let fresh_instance = !self.instance_floor.contains_key(&instance);
            if fresh_instance && self.instance_floor.len() >= MAX_GESTURE_INSTANCES {
                return Err(GestureLaneError::Rotate);
            }
            // A newer source instance retires every older tracked one:
            // late replies from retired instances never rebind. Retired
            // evidence is never removed: the set stays bounded by the
            // instance cap.
            if self.latest_instance.as_deref() != Some(instance.as_str()) {
                if self.latest_instance.is_some() {
                    let known: Vec<String> = self.instance_floor.keys().cloned().collect();
                    for old in known {
                        if old != instance {
                            self.retired.insert(old);
                        }
                    }
                }
                self.latest_instance = Some(instance.clone());
            }
            // Bounded identity map: oldest-first eviction. The
            // per-instance serial floor is preserved for evicted
            // identities, so replays stay stale and never rebind.
            if self.by_key.len() >= MAX_GESTURE_IDENTITIES
                && let Some(oldest) = self.order.pop_front()
            {
                self.by_key.remove(&oldest);
            }
            if fresh_instance {
                self.instance_order.push_back(instance.clone());
            }
            let core_id = self.next_core_id;
            self.next_core_id += 1;
            self.by_key.insert(
                key.clone(),
                IdentityRecord {
                    core_id,
                    frozen: frozen.clone(),
                    last_revision: revision_num,
                },
            );
            self.order.push_back(key);
            self.instance_floor
                .insert(instance.clone(), floor.max(serial_num));
            core_id
        };
        // Cursor fold: terminal snapshots freeze the native finish cursor
        // (a later script pointer never overrides); open snapshots prefer
        // the same-capture readable script cursor and fall back to native
        // only when the script point is unknown.
        let fold_cursor = if terminal {
            cursor
        } else {
            script_cursor.or(cursor)
        };
        let point = |p: Option<[i32; 2]>| match p {
            None => Value::Null,
            Some([x, y]) => Value::Array(vec![Value::from(x), Value::from(y)]),
        };
        let ingress = serde_json::json!({
            "win": frozen.win,
            "kind": frozen.kind,
            "phase": phase,
            "id": core_id.to_string(),
            "cursor": point(fold_cursor),
            "startCursor": point(frozen.start_cursor),
            "esc": false,
            "modal": frozen.modal,
            "start": [frozen.start[0], frozen.start[1], frozen.start[2], frozen.start[3]],
        });
        inner.insert("gestureIngress".to_owned(), ingress);
        inner.remove("nativeGesture");
        inner.remove("cursorWindow");
        Ok(Some(GestureAssoc {
            instance,
            serial,
            core_id,
        }))
    }
}

impl Default for KdeGestureMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inner_with(
        native: Value,
        cursor: Value,
        cursor_window: Value,
        ingress: Value,
    ) -> Map<String, Value> {
        let mut inner = Map::new();
        inner.insert("gestureIngress".to_owned(), ingress);
        inner.insert("nativeGesture".to_owned(), native);
        inner.insert("cursor".to_owned(), cursor);
        inner.insert("cursorWindow".to_owned(), cursor_window);
        inner
    }

    fn native_open(instance: &str, serial: &str, revision: &str, cursor: Value) -> Value {
        serde_json::json!({
            "v": 1, "instance": instance, "serial": serial, "revision": revision,
            "phase": "updated", "win": "win-a", "kind": "move",
            "startRect": [0, 0, 200, 200], "startCursor": [10, 10], "cursor": cursor,
            "modalMove": 1, "changed": 1, "superseded": 0, "available": 1,
        })
    }

    #[test]
    fn rejects_script_minted_ingress() {
        let mut map = KdeGestureMap::new();
        let mut inner = inner_with(
            native_open("gesture-1", "1", "1", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            serde_json::json!({"win": "win-a"}),
        );
        assert_eq!(
            map.normalize_gesture_lane(&mut inner),
            Err(GestureLaneError::Reject(
                "unknown-value",
                "request contains an unknown value"
            ))
        );
        assert!(map.by_key.is_empty(), "rejection mutates nothing");
    }

    #[test]
    fn stale_revision_omits_without_mutation() {
        let mut map = KdeGestureMap::new();
        let mut first = inner_with(
            native_open("gesture-1", "1", "5", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        let assoc = map.normalize_gesture_lane(&mut first).expect("maps");
        assert_eq!(first["gestureIngress"]["id"], serde_json::json!("1"));
        assert_eq!(
            assoc,
            Some(GestureAssoc {
                instance: "gesture-1".to_owned(),
                serial: "1".to_owned(),
                core_id: 1
            })
        );
        // Older revision for the same mapped identity omits.
        let mut stale = inner_with(
            native_open("gesture-1", "1", "4", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        let stale_assoc = map.normalize_gesture_lane(&mut stale).expect("omits");
        assert_eq!(stale_assoc, None);
        assert_eq!(stale["gestureIngress"], Value::Null);
        assert!(!stale.contains_key("nativeGesture"));
        assert!(!stale.contains_key("cursorWindow"));
    }

    #[test]
    fn malformed_snapshot_leaves_map_untouched() {
        let mut map = KdeGestureMap::new();
        let mut good = inner_with(
            native_open("gesture-1", "1", "1", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        map.normalize_gesture_lane(&mut good).expect("maps");
        let mut bad = inner_with(
            serde_json::json!({"v": 1, "instance": "gesture-1"}),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        assert!(map.normalize_gesture_lane(&mut bad).is_err());
        // The good identity still resolves to its original core id.
        let mut replay = inner_with(
            native_open("gesture-1", "1", "1", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        map.normalize_gesture_lane(&mut replay).expect("maps");
        assert_eq!(replay["gestureIngress"]["id"], serde_json::json!("1"));
    }

    #[test]
    fn open_prefers_script_cursor_over_native() {
        let mut map = KdeGestureMap::new();
        let mut inner = inner_with(
            native_open("gesture-1", "1", "1", serde_json::json!([150, 50])),
            serde_json::json!([11, 12]),
            Value::Null,
            Value::Null,
        );
        map.normalize_gesture_lane(&mut inner).expect("maps");
        assert_eq!(
            inner["gestureIngress"]["cursor"],
            serde_json::json!([11, 12])
        );
        // Unknown script point falls back to native.
        let mut fallback = inner_with(
            native_open("gesture-2", "1", "1", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        map.normalize_gesture_lane(&mut fallback).expect("maps");
        assert_eq!(
            fallback["gestureIngress"]["cursor"],
            serde_json::json!([150, 50])
        );
    }

    #[test]
    fn terminal_freezes_native_cursor() {
        let mut map = KdeGestureMap::new();
        let mut inner = inner_with(
            serde_json::json!({
                "v": 1, "instance": "gesture-1", "serial": "1", "revision": "1",
                "phase": "released", "win": "win-a", "kind": "move",
                "startRect": [0, 0, 200, 200], "startCursor": [10, 10], "cursor": [150, 50],
                "modalMove": 1, "changed": 1, "superseded": 0, "available": 1,
            }),
            serde_json::json!([11, 12]),
            serde_json::json!("win-b"),
            Value::Null,
        );
        let assoc = map.normalize_gesture_lane(&mut inner).expect("maps");
        assert_eq!(
            inner["gestureIngress"]["cursor"],
            serde_json::json!([150, 50])
        );
        assert!(!inner.contains_key("cursorWindow"));
        assert!(assoc.is_some());
    }

    #[test]
    fn seventeenth_source_rotates_without_sacrificing_antireplay() {
        let mut map = KdeGestureMap::new();
        for i in 1..=MAX_GESTURE_INSTANCES {
            let instance = format!("gesture-{i}");
            let mut inner = inner_with(
                native_open(&instance, "1", "1", serde_json::json!([150, 50])),
                Value::Null,
                Value::Null,
                Value::Null,
            );
            assert!(
                map.normalize_gesture_lane(&mut inner)
                    .expect("maps")
                    .is_some()
            );
        }
        assert_eq!(map.instance_floor.len(), MAX_GESTURE_INSTANCES);
        // One more genuinely new source exhausts via Rotate, not eviction.
        let mut extra = inner_with(
            native_open("gesture-99", "1", "1", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        assert_eq!(
            map.normalize_gesture_lane(&mut extra),
            Err(GestureLaneError::Rotate)
        );
        // Anti-replay evidence preserved: old serials still omit, and the
        // retired discrimination holds (no new identity admitted).
        assert_eq!(map.instance_floor.len(), MAX_GESTURE_INSTANCES);
        assert!(map.instance_floor.contains_key("gesture-1"));
        assert!(map.retired.contains("gesture-1"));
        let mut replay_old = inner_with(
            native_open("gesture-1", "1", "2", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        let assoc = map
            .normalize_gesture_lane(&mut replay_old)
            .expect("mapped replay");
        // Already-mapped old identity still resolves to its original core id.
        assert_eq!(replay_old["gestureIngress"]["id"], serde_json::json!("1"));
        assert!(assoc.is_some());
        // Identity eviction preserves floors: fill identities then replay an
        // evicted identity and require stale omission, never a rebind.
        let mut map2 = KdeGestureMap::new();
        let mut first = inner_with(
            native_open("gesture-a", "5", "1", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        map2.normalize_gesture_lane(&mut first).expect("maps");
        assert_eq!(first["gestureIngress"]["id"], serde_json::json!("1"));
        for i in 0..MAX_GESTURE_IDENTITIES {
            let serial = format!("{}", 10 + i);
            let mut inner = inner_with(
                native_open("gesture-a", &serial, "1", serde_json::json!([150, 50])),
                Value::Null,
                Value::Null,
                Value::Null,
            );
            let _ = map2.normalize_gesture_lane(&mut inner);
        }
        assert!(
            !map2
                .by_key
                .contains_key(&("gesture-a".to_owned(), "5".to_owned()))
        );
        assert_eq!(map2.instance_floor.get("gesture-a"), Some(&265));
        let mut evicted_replay = inner_with(
            native_open("gesture-a", "5", "2", serde_json::json!([150, 50])),
            Value::Null,
            Value::Null,
            Value::Null,
        );
        let assoc = map2
            .normalize_gesture_lane(&mut evicted_replay)
            .expect("stale");
        assert_eq!(assoc, None);
        assert_eq!(evicted_replay["gestureIngress"], Value::Null);
    }
}
