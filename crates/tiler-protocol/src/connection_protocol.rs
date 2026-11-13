//! Handwritten v4 connection-message JSON codecs and reusable strict JSON primitives.

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value};
use std::collections::HashSet;
use std::fmt;
use tiler_core::connection::*;

const MAX_MESSAGE: usize = 4 * 1024 * 1024;
const MAX_METADATA_TEXT: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecError(pub &'static str);

#[derive(Debug, Clone, PartialEq)]
pub struct WireError {
    pub kind: ProtocolKind,
    pub message: String,
    pub generation: String,
}

struct UniqueValue(Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(UniqueVisitor)
    }
}
struct UniqueVisitor;
impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = UniqueValue;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON without duplicate object keys")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.visit_unit()
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Bool(v)))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(v.into())))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(v.into())))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Number::from_f64(v)
            .map(|n| UniqueValue(Value::Number(n)))
            .ok_or_else(|| E::custom("nonfinite"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(v.to_owned())))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(v)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut v = Vec::new();
        while let Some(x) = a.next_element::<UniqueValue>()? {
            v.push(x.0);
        }
        Ok(UniqueValue(Value::Array(v)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut seen = HashSet::new();
        let mut m = Map::new();
        while let Some(k) = a.next_key::<String>()? {
            if !seen.insert(k.clone()) {
                return Err(de::Error::custom("duplicate key"));
            }
            let v = a.next_value::<UniqueValue>()?;
            m.insert(k, v.0);
        }
        Ok(UniqueValue(Value::Object(m)))
    }
}

/// Parse a JSON value while rejecting duplicate keys at every nesting depth.
pub fn parse_strict_json(input: &str) -> Result<Value, CodecError> {
    if input.len() > MAX_MESSAGE {
        return Err(CodecError("oversized"));
    }
    let mut d = serde_json::Deserializer::from_str(input);
    let value = UniqueValue::deserialize(&mut d)
        .map_err(|_| CodecError("malformed"))?
        .0;
    d.end().map_err(|_| CodecError("malformed"))?;
    Ok(value)
}

/// Encode a JSON string using compact UTF-8 and lowercase C0 escapes.
pub fn encode_json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Expand a finite binary64 decimal spelling into the contract's canonical form.
pub fn encode_json_number(value: f64) -> Result<String, CodecError> {
    if !value.is_finite() {
        return Err(CodecError("nonfinite"));
    }
    Ok(tiler_core::world_report::encode_finite_json_number(value))
}

fn object(value: &Value) -> Result<&Map<String, Value>, CodecError> {
    value.as_object().ok_or(CodecError("malformed"))
}
fn exact(value: &Value, required: &[&str]) -> Result<(), CodecError> {
    let m = object(value)?;
    if m.len() != required.len() || required.iter().any(|k| !m.contains_key(*k)) {
        return Err(CodecError("shape"));
    }
    Ok(())
}
fn strv(v: &Value) -> Result<&str, CodecError> {
    v.as_str().ok_or(CodecError("type"))
}
fn boolean(v: &Value) -> Result<bool, CodecError> {
    v.as_bool().ok_or(CodecError("type"))
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn valid_gen(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn counter(s: &str) -> Result<u64, CodecError> {
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) || !s.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(CodecError("counter"));
    }
    s.parse().map_err(|_| CodecError("counter"))
}
fn scalar_identity(v: &Value) -> Result<Option<String>, CodecError> {
    if v.is_null() {
        return Ok(None);
    }
    let s = strv(v)?;
    if !valid_identity_text(s) {
        return Err(CodecError("identity"));
    }
    Ok(Some(s.to_owned()))
}

/// Nullable native owning-process identity in the existing opaque Id
/// namespace (session-local exact comparison, bound to the handshake
/// lifetime): null when unavailable, otherwise a non-empty bounded opaque
/// token. Never a JSON number and never inferred from executable/appId.
fn process_identity(v: &Value) -> Result<Option<String>, CodecError> {
    if v.is_null() {
        return Ok(None);
    }
    let s = strv(v)?;
    if !valid_process_identity_text(s) {
        return Err(CodecError("identity"));
    }
    Ok(Some(s.to_owned()))
}

pub fn decode_handshake_request(input: &str) -> Result<HandshakeRequest, CodecError> {
    let v = parse_strict_json(input)?;
    exact(
        &v,
        &[
            "type",
            "v",
            "adapter",
            "req",
            "applicationIdentity",
            "capabilities",
        ],
    )?;
    let m = object(&v)?;
    if strv(&m["type"])? != "handshake" || m["v"].as_u64() != Some(4) {
        return Err(CodecError("version"));
    }
    let adapter = strv(&m["adapter"])?.to_owned();
    let req = strv(&m["req"])?.to_owned();
    if !valid_id(&adapter) || !valid_id(&req) {
        return Err(CodecError("id"));
    }
    let i = &m["applicationIdentity"];
    exact(i, &["appId", "executable", "processId"])?;
    let im = object(i)?;
    let app_id = scalar_identity(&im["appId"])?;
    let executable = scalar_identity(&im["executable"])?;
    let process_id = process_identity(&im["processId"])?;
    let c = &m["capabilities"];
    let ck = [
        "hostOwned",
        "equalCount",
        "sharedNativeRoster",
        "birthClear",
        "nativeSelectorScope",
        "windowSuppression",
        "maximize",
        "fullscreen",
        "nativeSticky",
        "above",
        "warp",
        "tiledFloatTransfer",
        "unavailableFacts",
    ];
    exact(c, &ck)?;
    let cm = object(c)?;
    let scope = match strv(&cm["nativeSelectorScope"])? {
        "per-output" => NativeSelectorScope::PerOutput,
        "all-outputs" => NativeSelectorScope::AllOutputs,
        _ => return Err(CodecError("value")),
    };
    let facts = cm["unavailableFacts"]
        .as_array()
        .ok_or(CodecError("type"))?;
    // Bound mirrors the closed FactName vocabulary: a fully unavailable
    // connection lists every name exactly once.
    if facts.len() > FactName::ALL.len() {
        return Err(CodecError("facts"));
    }
    let mut unavailable = Vec::new();
    for f in facts {
        let name = FactName::parse(strv(f)?).ok_or(CodecError("fact"))?;
        unavailable.push(name);
    }
    if unavailable.windows(2).any(|w| w[0] >= w[1]) {
        return Err(CodecError("facts-order"));
    }
    if unavailable.contains(&FactName::Executable) != executable.is_none() {
        return Err(CodecError("identity-support"));
    }
    if unavailable.contains(&FactName::ProcessId) != process_id.is_none() {
        return Err(CodecError("identity-support"));
    }
    let capabilities = HostCapabilities {
        host_owned: boolean(&cm["hostOwned"])?,
        equal_count: boolean(&cm["equalCount"])?,
        shared_native_roster: boolean(&cm["sharedNativeRoster"])?,
        birth_clear: boolean(&cm["birthClear"])?,
        native_selector_scope: scope,
        window_suppression: boolean(&cm["windowSuppression"])?,
        maximize: boolean(&cm["maximize"])?,
        fullscreen: boolean(&cm["fullscreen"])?,
        native_sticky: boolean(&cm["nativeSticky"])?,
        above: boolean(&cm["above"])?,
        warp: boolean(&cm["warp"])?,
        tiled_float_transfer: boolean(&cm["tiledFloatTransfer"])?,
        unavailable_facts: unavailable,
    };
    Ok(HandshakeRequest {
        adapter,
        request: req,
        application_identity: ApplicationIdentity {
            app_id,
            executable,
            process_id,
        },
        capabilities,
    })
}

pub fn decode_handshake_reply(input: &str) -> Result<HandshakeReply, CodecError> {
    let v = parse_strict_json(input)?;
    exact(&v, &["type", "v", "adapter", "req", "gen", "ready", "kind"])?;
    let m = object(&v)?;
    if strv(&m["type"])? != "handshake-reply" || m["v"].as_u64() != Some(4) {
        return Err(CodecError("version"));
    }
    let adapter = strv(&m["adapter"])?.to_owned();
    let request = strv(&m["req"])?.to_owned();
    if !valid_id(&adapter) || !valid_id(&request) {
        return Err(CodecError("id"));
    }
    let generation = if m["gen"].is_null() {
        None
    } else {
        let s = strv(&m["gen"])?;
        if !valid_gen(s) {
            return Err(CodecError("generation"));
        }
        Some(s.to_owned())
    };
    let ready = boolean(&m["ready"])?;
    let k = strv(&m["kind"])?;
    let kind = if k.is_empty() {
        None
    } else {
        Some(parse_kind(k).ok_or(CodecError("kind"))?)
    };
    if ready && (generation.is_none() || kind.is_some())
        || !ready && kind.is_some() && generation.is_some()
        || !ready && kind.is_none() && generation.is_none()
    {
        return Err(CodecError("reply-shape"));
    }
    Ok(HandshakeReply {
        adapter,
        request,
        generation,
        ready,
        kind,
    })
}

fn parse_kind(s: &str) -> Option<ProtocolKind> {
    Some(match s {
        "malformed" => ProtocolKind::Malformed,
        "oversized" => ProtocolKind::Oversized,
        "snapshot-invalid" => ProtocolKind::SnapshotInvalid,
        "unknown-field" => ProtocolKind::UnknownField,
        "unknown-value" => ProtocolKind::UnknownValue,
        "unsupported-version" => ProtocolKind::UnsupportedVersion,
        "owner-invalid" => ProtocolKind::OwnerInvalid,
        "correlation-invalid" => ProtocolKind::CorrelationInvalid,
        "generation-mismatch" => ProtocolKind::GenerationMismatch,
        "observation-stale" => ProtocolKind::ObservationStale,
        "request-superseded" => ProtocolKind::RequestSuperseded,
        "request-reused" => ProtocolKind::RequestReused,
        "reply-oversize" => ProtocolKind::ReplyOversize,
        "revision-exhausted" => ProtocolKind::RevisionExhausted,
        "creation-exhausted" => ProtocolKind::CreationExhausted,
        _ => return None,
    })
}

fn raw(v: &Value) -> Result<RawSettingValue, CodecError> {
    Ok(match v {
        Value::Null => RawSettingValue::Null,
        Value::Bool(b) => RawSettingValue::Bool(*b),
        Value::Number(n) => RawSettingValue::Number(n.as_f64().ok_or(CodecError("number"))?),
        Value::String(s) => RawSettingValue::String(s.clone()),
        Value::Array(a) => RawSettingValue::Array(a.iter().map(raw).collect::<Result<_, _>>()?),
        Value::Object(m) => RawSettingValue::Object(
            m.iter()
                .map(|(k, v)| Ok((k.clone(), raw(v)?)))
                .collect::<Result<_, CodecError>>()?,
        ),
    })
}

pub fn decode_settings(input: &str) -> Result<SettingsRequest, CodecError> {
    let v = parse_strict_json(input)?;
    exact(&v, &["type", "v", "adapter", "gen", "req", "rev", "values"])?;
    let m = object(&v)?;
    if strv(&m["type"])? != "settings" || m["v"].as_u64() != Some(4) {
        return Err(CodecError("version"));
    }
    let adapter = strv(&m["adapter"])?.to_owned();
    let generation = strv(&m["gen"])?.to_owned();
    let request = strv(&m["req"])?.to_owned();
    if !valid_id(&adapter) || !valid_id(&request) || !valid_gen(&generation) {
        return Err(CodecError("identity"));
    }
    let revision = counter(strv(&m["rev"])?)?;
    let vals = object(&m["values"])?;
    let values = vals
        .iter()
        .map(|(k, v)| {
            if k.len() > MAX_METADATA_TEXT {
                return Err(CodecError("setting-key"));
            }
            Ok((k.clone(), raw(v)?))
        })
        .collect::<Result<_, CodecError>>()?;
    Ok(SettingsRequest {
        adapter,
        generation,
        request,
        revision,
        values,
    })
}

pub fn decode_settings_reply(input: &str) -> Result<SettingsReply, CodecError> {
    let v = parse_strict_json(input)?;
    exact(
        &v,
        &[
            "type",
            "v",
            "adapter",
            "gen",
            "req",
            "rev",
            "status",
            "diagnostics",
        ],
    )?;
    let m = object(&v)?;
    if strv(&m["type"])? != "settings-reply" || m["v"].as_u64() != Some(4) {
        return Err(CodecError("version"));
    }
    let adapter = strv(&m["adapter"])?.to_owned();
    let generation = strv(&m["gen"])?.to_owned();
    let request = strv(&m["req"])?.to_owned();
    if !valid_id(&adapter) || !valid_id(&request) || !valid_gen(&generation) {
        return Err(CodecError("identity"));
    }
    let revision = counter(strv(&m["rev"])?)?;
    let status = match strv(&m["status"])? {
        "accepted" => SettingsStatus::Accepted,
        "stale" => SettingsStatus::Stale,
        "revision-reused" => SettingsStatus::RevisionReused,
        _ => return Err(CodecError("status")),
    };
    let a = m["diagnostics"].as_array().ok_or(CodecError("type"))?;
    let mut seen = HashSet::new();
    let mut diagnostics = Vec::new();
    for d in a {
        exact(d, &["key", "kind"])?;
        let dm = object(d)?;
        let key = strv(&dm["key"])?.to_owned();
        if key.len() > MAX_METADATA_TEXT || !seen.insert(key.clone()) {
            return Err(CodecError("diagnostic-key"));
        }
        let kind = match strv(&dm["kind"])? {
            "missing" => SettingsDiagnosticKind::Missing,
            "unknown-field" => SettingsDiagnosticKind::UnknownField,
            "wrong-type" => SettingsDiagnosticKind::WrongType,
            "out-of-range" => SettingsDiagnosticKind::OutOfRange,
            "unknown-value" => SettingsDiagnosticKind::UnknownValue,
            "restart-required" => SettingsDiagnosticKind::RestartRequired,
            "unsupported-mode" => SettingsDiagnosticKind::UnsupportedMode,
            "unsupported-capability" => SettingsDiagnosticKind::UnsupportedCapability,
            _ => return Err(CodecError("diagnostic-kind")),
        };
        diagnostics.push(SettingsDiagnostic { key, kind });
    }
    Ok(SettingsReply {
        adapter,
        generation,
        request,
        revision,
        status,
        diagnostics,
    })
}

pub fn decode_error(input: &str) -> Result<WireError, CodecError> {
    let v = parse_strict_json(input)?;
    exact(&v, &["v", "outcome", "kind", "message", "gen"])?;
    let m = object(&v)?;
    if m["v"].as_u64() != Some(4) || strv(&m["outcome"])? != "rejected" {
        return Err(CodecError("version/outcome"));
    }
    let kind = parse_kind(strv(&m["kind"])?).ok_or(CodecError("kind"))?;
    let message = strv(&m["message"])?;
    let generation = strv(&m["gen"])?;
    if message.len() > MAX_METADATA_TEXT
        || generation.len() > 64
        || (!generation.is_empty() && !valid_gen(generation))
    {
        return Err(CodecError("error-bounds"));
    }
    Ok(WireError {
        kind,
        message: message.to_owned(),
        generation: generation.to_owned(),
    })
}

fn push_value(out: &mut String, v: &RawSettingValue) -> Result<(), CodecError> {
    match v {
        RawSettingValue::Null => out.push_str("null"),
        RawSettingValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        RawSettingValue::Number(n) => out.push_str(&encode_json_number(*n)?),
        RawSettingValue::String(s) => out.push_str(&encode_json_string(s)),
        RawSettingValue::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',')
                }
                push_value(out, x)?;
            }
            out.push(']');
        }
        RawSettingValue::Object(m) => {
            out.push('{');
            let mut keys = m.iter().collect::<Vec<_>>();
            keys.sort_by(|a, b| a.0.cmp(&b.0));
            for (i, (k, x)) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',')
                }
                out.push_str(&encode_json_string(k));
                out.push(':');
                push_value(out, x)?;
            }
            out.push('}');
        }
    }
    Ok(())
}
const SETTINGS_ORDER: &[&str] = &[
    "gap",
    "outer",
    "defaultTiled",
    "mode",
    "sameAxis",
    "bornMaximized",
    "predicate",
    "refill",
    "switchTarget",
    "freshPlacement",
    "focusFollowsCursor",
    "cursorWarp",
    "borderEnabled",
    "underlayEnabled",
    "borderWidth",
    "borderGap",
    "borderRadius",
    "underlayExtension",
];

pub fn encode_settings(v: &SettingsRequest) -> Result<String, CodecError> {
    if !valid_id(&v.adapter) || !valid_id(&v.request) || !valid_gen(&v.generation) {
        return Err(CodecError("identity"));
    }
    let mut out = format!(
        "{{\"type\":\"settings\",\"v\":4,\"adapter\":{},\"gen\":{},\"req\":{},\"rev\":{},\"values\":{{",
        encode_json_string(&v.adapter),
        encode_json_string(&v.generation),
        encode_json_string(&v.request),
        encode_json_string(&v.revision.to_string())
    );
    let mut unique = HashSet::new();
    for (key, _) in &v.values {
        if key.len() > MAX_METADATA_TEXT {
            return Err(CodecError("setting-key"));
        }
        if !unique.insert(key.as_str()) {
            return Err(CodecError("duplicate-setting"));
        }
    }
    let map = v
        .values
        .iter()
        .map(|(k, x)| (k.as_str(), x))
        .collect::<std::collections::HashMap<_, _>>();
    let mut keys = SETTINGS_ORDER
        .iter()
        .filter(|k| map.contains_key(**k))
        .copied()
        .collect::<Vec<_>>();
    let mut extra = v
        .values
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| !SETTINGS_ORDER.contains(k))
        .collect::<Vec<_>>();
    extra.sort_unstable();
    keys.extend(extra);
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            out.push(',')
        }
        out.push_str(&encode_json_string(k));
        out.push(':');
        if let Some(x) = map.get(k) {
            push_value(&mut out, x)?;
        }
    }
    out.push_str("}}");
    if out.len() > MAX_MESSAGE {
        return Err(CodecError("oversized"));
    }
    decode_settings(&out)?;
    Ok(out)
}

pub fn encode_handshake_request(v: &HandshakeRequest) -> Result<String, CodecError> {
    if !valid_id(&v.adapter) || !valid_id(&v.request) {
        return Err(CodecError("identity"));
    }
    let id = |x: &Option<String>| {
        x.as_deref()
            .map_or_else(|| "null".into(), encode_json_string)
    };
    let c = &v.capabilities;
    let scope = match c.native_selector_scope {
        NativeSelectorScope::PerOutput => "per-output",
        NativeSelectorScope::AllOutputs => "all-outputs",
    };
    let facts = c
        .unavailable_facts
        .iter()
        .map(|f| encode_json_string(f.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    let out = format!(
        "{{\"type\":\"handshake\",\"v\":4,\"adapter\":{},\"req\":{},\"applicationIdentity\":{{\"appId\":{},\"executable\":{},\"processId\":{}}},\"capabilities\":{{\"hostOwned\":{},\"equalCount\":{},\"sharedNativeRoster\":{},\"birthClear\":{},\"nativeSelectorScope\":{},\"windowSuppression\":{},\"maximize\":{},\"fullscreen\":{},\"nativeSticky\":{},\"above\":{},\"warp\":{},\"tiledFloatTransfer\":{},\"unavailableFacts\":[{}]}}}}",
        encode_json_string(&v.adapter),
        encode_json_string(&v.request),
        id(&v.application_identity.app_id),
        id(&v.application_identity.executable),
        id(&v.application_identity.process_id),
        c.host_owned,
        c.equal_count,
        c.shared_native_roster,
        c.birth_clear,
        encode_json_string(scope),
        c.window_suppression,
        c.maximize,
        c.fullscreen,
        c.native_sticky,
        c.above,
        c.warp,
        c.tiled_float_transfer,
        facts
    );
    if out.len() > MAX_MESSAGE {
        return Err(CodecError("oversized"));
    }
    decode_handshake_request(&out)?;
    Ok(out)
}

pub fn encode_handshake_reply(v: &HandshakeReply) -> Result<String, CodecError> {
    let generation_json = v
        .generation
        .as_deref()
        .map_or_else(|| "null".into(), encode_json_string);
    let kind = v.kind.map_or("".to_owned(), |k| k.as_str().to_owned());
    let out = format!(
        "{{\"type\":\"handshake-reply\",\"v\":4,\"adapter\":{},\"req\":{},\"gen\":{},\"ready\":{},\"kind\":{}}}",
        encode_json_string(&v.adapter),
        encode_json_string(&v.request),
        generation_json,
        v.ready,
        encode_json_string(&kind)
    );
    decode_handshake_reply(&out)?;
    Ok(out)
}

pub fn encode_settings_reply(v: &SettingsReply) -> Result<String, CodecError> {
    if v.diagnostics
        .iter()
        .any(|diagnostic| diagnostic.key.len() > MAX_METADATA_TEXT)
    {
        return Err(CodecError("diagnostic-key"));
    }
    let status = match v.status {
        SettingsStatus::Accepted => "accepted",
        SettingsStatus::Stale => "stale",
        SettingsStatus::RevisionReused => "revision-reused",
    };
    let mut out = format!(
        "{{\"type\":\"settings-reply\",\"v\":4,\"adapter\":{},\"gen\":{},\"req\":{},\"rev\":{},\"status\":{},\"diagnostics\":[",
        encode_json_string(&v.adapter),
        encode_json_string(&v.generation),
        encode_json_string(&v.request),
        encode_json_string(&v.revision.to_string()),
        encode_json_string(status)
    );
    for (i, d) in v.diagnostics.iter().enumerate() {
        if i > 0 {
            out.push(',')
        }
        out.push_str(&format!(
            "{{\"key\":{},\"kind\":{}}}",
            encode_json_string(&d.key),
            encode_json_string(d.kind.as_str())
        ));
    }
    out.push_str("]}");
    if out.len() > MAX_MESSAGE {
        return Err(CodecError("oversized"));
    }
    decode_settings_reply(&out)?;
    Ok(out)
}
fn fixed_error_message(kind: ProtocolKind) -> &'static str {
    kind.as_str()
}

pub fn encode_error(kind: ProtocolKind, generation: &str) -> Result<String, CodecError> {
    if generation.len() > 64 {
        return Err(CodecError("error-bounds"));
    }
    let out = format!(
        "{{\"v\":4,\"outcome\":\"rejected\",\"kind\":{},\"message\":{},\"gen\":{}}}",
        encode_json_string(kind.as_str()),
        encode_json_string(fixed_error_message(kind)),
        encode_json_string(generation)
    );
    if out.len() > MAX_MESSAGE {
        return Err(CodecError("oversized"));
    }
    decode_error(&out)?;
    Ok(out)
}

/// Route a connection-family message to the supplied engine instance. World
/// reports continue through the staged world decoder until their v4-only
/// envelope is ready; this entry point owns handshake and settings routing.
pub fn route_connection_request(
    engine: &mut tiler_core::world_engine::WorldEngine,
    input: &str,
) -> Result<String, CodecError> {
    let value = parse_strict_json(input)?;
    let message_type = value
        .as_object()
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        .ok_or(CodecError("malformed"))?;
    match message_type {
        "handshake" => {
            let request = decode_handshake_request(input)?;
            encode_handshake_reply(&engine.handshake(&request))
        }
        "settings" => {
            let request = decode_settings(input)?;
            match engine.handle_settings(&request) {
                Some(reply) => encode_settings_reply(&reply),
                None => encode_error(ProtocolKind::GenerationMismatch, engine.core_generation()),
            }
        }
        _ => Err(CodecError("message-type")),
    }
}

#[cfg(test)]
mod tests;
