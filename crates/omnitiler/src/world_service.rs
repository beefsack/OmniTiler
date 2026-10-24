//! World planner service boundary: retained sole [`WorldBroker`] behind JSON.
//!
//! D-Bus surface (same service/object/interface as the intent routes,
//! [`crate::planner_service`]): `DiscoverWorld` returns the bootstrap
//! envelope, `DescribeWorld` accepts one broker capture envelope per call and
//! returns one bound response, `QueueWorldCommand` files one external
//! command, `TakeCommandResult` collects its terminal result exactly once,
//! and `ReadWorldPresentation` reads the cached accepted presentation.
//! The KDE production entry uses the capture route for actuation and the
//! query/command routes for everything else.
//!
//! Rules mirrored from the legacy route: bounded 4 MiB request AND reply,
//! non-queuing single-flight (contended capture callers fail fast as busy,
//! never queue and never acknowledge), poisoned-lock replacement under a
//! FRESH core generation (never silently continuing on a partial engine),
//! and redacted diagnostics without payload bytes. D-Bus caller
//! authentication (same-UID plus the current KWin unique owner as the single
//! active adapter for the capture/discovery routes; same-UID only for the
//! query/command routes) lives in [`crate::planner_service`]; this type
//! never touches the bus and never authorizes.
//!
//! Generation lifecycle: the broker mints a core generation and its own
//! broker incarnation at construction, but the first discovery binds the
//! single active producer capture lifetime and rotates onto a fresh serving
//! generation first, so every served generation belongs to exactly one
//! lifetime. Discovery returns the serving generation plus capabilities with
//! `ready=false` and no applicable desired state. Same-lifetime rediscovery
//! rebinds without reset; a new lifetime rotates (fresh generation and
//! incarnation, all capture/queue/result/presentation state dropped), so no
//! old capture can ever apply again. Every capture binds its stamped `gen`
//! against the serving generation; mismatches reject without state. Poison
//! recovery mints a new generation and a new broker, so old effects,
//! flights, ordering, queued commands, settled results, and desired caches
//! can never leak across the replacement: the adapter must re-discover and
//! re-observe, and command issuers must re-establish with fresh per-client
//! command ids.

use std::sync::{Arc, Mutex};

use tiler_protocol::world_protocol::{
    WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, is_valid_adapter_id, is_valid_request_id,
};

use crate::world_broker::{BrokerError, WorldBroker};

/// World capture/actuator route: one broker capture envelope in, one bound
/// response out.
pub const WORLD_METHOD: &str = "DescribeWorld";
/// World discovery/bootstrap route: generation plus capabilities, always
/// `ready=false` with no applicable desired state.
pub const DISCOVERY_METHOD: &str = "DiscoverWorld";
/// External command queue route: one strict v2 command in, queued/duplicate out.
pub const QUEUE_METHOD: &str = "QueueWorldCommand";
/// Command result route: exactly-once terminal result collection.
pub const TAKE_METHOD: &str = "TakeCommandResult";
/// Presentation query route: cached accepted presentation, no one-shot lanes.
pub const READ_METHOD: &str = "ReadWorldPresentation";
/// Bounded world request cap (mirrors the protocol/core budget).
pub const WORLD_SERVICE_MAX_REQUEST: usize = WORLD_MAX_REQUEST_BYTES;
/// Bounded world reply cap (mirrors the protocol/core budget).
pub const WORLD_SERVICE_MAX_REPLY: usize = WORLD_MAX_REPLY_BYTES;

/// Mint a fresh core generation token (`w-<nanos>-<pid>-<counter>`):
/// lowercase/digit/dash only, within the generation bound. Owned here (not
/// in the broker) so poison replacement and broker construction share one
/// mint.
#[must_use]
pub fn mint_core_generation() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    static GENERATION_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let count = GENERATION_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("w-{nanos}-{pid}-{count}")
}

/// Terminal world-service failure: oversize requests and oversize replies
/// surface as D-Bus errors, never as ready replies. Application rejections
/// (including unknown generations) are bounded in-band JSON via
/// [`WorldService::error_body`]. Single-flight contention is admitted at
/// the endpoint lock before evaluation, so it never reaches this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldServiceError {
    Oversized,
    Unavailable(String),
}

/// Retained world backend: the sole broker plus its serving generation.
#[derive(Debug)]
struct WorldBackend {
    broker: WorldBroker,
}

impl WorldBackend {
    fn fresh() -> Self {
        Self {
            broker: WorldBroker::new(),
        }
    }

    fn generation(&self) -> &str {
        self.broker.generation()
    }
}

/// World planner endpoint: pure request evaluation plus the single-flight
/// lock. D-Bus authentication (same-UID, plus the KWin-owner check on the
/// capture/discovery routes) lives in [`crate::planner_service`]; this type
/// never touches the bus.
#[derive(Clone, Debug)]
pub struct WorldService {
    backend: Arc<Mutex<WorldBackend>>,
    world_lock: Arc<async_lock::Mutex<()>>,
}

impl WorldService {
    /// Fresh backend incarnation: new core generation, new broker
    /// incarnation, empty engine and command state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            backend: Arc::new(Mutex::new(WorldBackend::fresh())),
            world_lock: Arc::new(async_lock::Mutex::new(())),
        }
    }

    /// Serving core generation (the discovery/bootstrap binding).
    #[must_use]
    pub fn generation(&self) -> String {
        match self.backend.lock() {
            Ok(backend) => backend.generation().to_owned(),
            Err(_) => String::new(),
        }
    }

    /// Non-queuing single-flight probe for tests: `true` when the world
    /// route is idle.
    #[must_use]
    pub fn try_acquire(&self) -> bool {
        self.world_lock.try_lock().is_some()
    }

    /// Non-queuing single-flight admission for the D-Bus capture route:
    /// `Some` guard when idle, `None` when a world call is in flight (fail
    /// fast as busy, never queue). The guard must be held for the whole
    /// call, including caller verification.
    pub fn try_acquire_guard(&self) -> Option<async_lock::MutexGuard<'_, ()>> {
        self.world_lock.try_lock()
    }

    /// Discovery/bootstrap body for an adapter incarnation and request id:
    /// generation plus capabilities with `ready=false` and no applicable
    /// desired state. The adapter sends a full capture before any ready
    /// reply; empty pre-bootstrap state never authorizes removal.
    #[must_use]
    pub fn discovery_body(&self, adapter_incarnation: &str, request_id: &str) -> String {
        match self.backend.lock() {
            Ok(mut backend) => backend
                .broker
                .discovery_body(adapter_incarnation, request_id),
            Err(poison) => {
                // Poisoned backend: replace under a fresh generation, then
                // serve discovery from the replacement (never from partial
                // state). The adapter must re-observe; nothing is claimed
                // to have survived.
                let mut backend = poison.into_inner();
                *backend = WorldBackend::fresh();
                let reply = backend
                    .broker
                    .discovery_body(adapter_incarnation, request_id);
                eprintln!("omnitiler:world-fault kind=poisoned-lock detail=replaced-with-fresh");
                self.backend.clear_poison_if_needed();
                reply
            }
        }
    }

    /// Bounded in-band rejection body bound to the serving generation.
    /// Poison degrades to an empty generation rather than failing.
    #[must_use]
    pub fn error_body(&self, kind: &str, message: &str) -> String {
        match self.backend.lock() {
            Ok(backend) => backend.broker.error_body(kind, message),
            Err(_) => tiler_protocol::world_protocol::encode_error(kind, message, ""),
        }
    }

    /// Evaluate one broker capture envelope end to end: byte-bound the
    /// envelope, run the sole broker (watermark binding, redelivery replay,
    /// broker-owned sequencing, at most one drained command), and byte-bound
    /// the response. Ready outcomes return the bound reply; rejections
    /// return bounded in-band errors. Only oversize surfaces as
    /// [`WorldServiceError`].
    pub fn evaluate_capture(
        &self,
        envelope: &str,
    ) -> std::result::Result<String, WorldServiceError> {
        if envelope.len() > WORLD_SERVICE_MAX_REQUEST {
            return Err(WorldServiceError::Oversized);
        }
        self.run_broker(|broker| broker.dispatch_capture(envelope))
    }

    /// File one external command for the next fresh capture. Pure broker
    /// state: never touches the engine, never waits on the capture lock.
    /// The `sender` is the authenticated D-Bus unique name binding the
    /// client namespace.
    pub fn queue_command(
        &self,
        sender: &str,
        request: &str,
    ) -> std::result::Result<String, WorldServiceError> {
        if request.len() > WORLD_SERVICE_MAX_REQUEST {
            return Err(WorldServiceError::Oversized);
        }
        self.run_broker(|broker| broker.queue_command(sender, request))
    }

    /// Collect one settled terminal result. The `sender` binds the
    /// namespace: cross-sender collection rejects as unknown.
    pub fn take_result(
        &self,
        sender: &str,
        request: &str,
    ) -> std::result::Result<String, WorldServiceError> {
        if request.len() > WORLD_SERVICE_MAX_REQUEST {
            return Err(WorldServiceError::Oversized);
        }
        self.run_broker(|broker| broker.take_result(sender, request))
    }

    /// Read the cached accepted presentation (no one-shot lanes).
    pub fn read_presentation(
        &self,
        request: &str,
    ) -> std::result::Result<String, WorldServiceError> {
        if request.len() > WORLD_SERVICE_MAX_REQUEST {
            return Err(WorldServiceError::Oversized);
        }
        self.run_broker(|broker| broker.read_presentation(request))
    }

    /// Run one broker operation under the backend lock: poison replaces the
    /// backend under a fresh generation first, counter exhaustion rotates
    /// the backend and fails closed (the caller re-discovers; a retry
    /// under the new generation would mismatch the bound one), oversize
    /// fails closed.
    fn run_broker(
        &self,
        op: impl FnOnce(&mut WorldBroker) -> std::result::Result<String, BrokerError>,
    ) -> std::result::Result<String, WorldServiceError> {
        match self.backend.lock() {
            Ok(mut backend) => {
                let outcome = op(&mut backend.broker);
                self.backend.clear_poison_if_needed();
                Self::coerce(outcome, &mut backend)
            }
            Err(poison) => {
                let mut backend = poison.into_inner();
                *backend = WorldBackend::fresh();
                eprintln!("omnitiler:world-fault kind=poisoned-lock detail=replaced-with-fresh");
                let outcome = op(&mut backend.broker);
                self.backend.clear_poison_if_needed();
                Self::coerce(outcome, &mut backend)
            }
        }
    }

    /// Map a broker outcome onto the service surface. Counter exhaustion
    /// rotates the backend under a fresh generation and fails closed: the
    /// caller re-discovers and re-issues (retrying under the rotated
    /// namespace would mismatch the bound generation).
    fn coerce(
        outcome: std::result::Result<String, BrokerError>,
        backend: &mut WorldBackend,
    ) -> std::result::Result<String, WorldServiceError> {
        match outcome {
            Ok(reply) => {
                if reply.len() > WORLD_SERVICE_MAX_REPLY {
                    return Err(WorldServiceError::Unavailable(
                        "reply exceeds size bound".to_owned(),
                    ));
                }
                Ok(reply)
            }
            Err(BrokerError::Oversized) => Err(WorldServiceError::Unavailable(
                "reply exceeds size bound".to_owned(),
            )),
            Err(BrokerError::Rotate) => {
                backend.broker.rotate();
                eprintln!(
                    "omnitiler:world-fault kind=sequence-exhausted detail=rotated-generation"
                );
                Err(WorldServiceError::Unavailable(
                    "sequencing exhausted".to_owned(),
                ))
            }
        }
    }
}

impl Default for WorldService {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed discovery/bootstrap request: adapter incarnation plus request id.
/// Strict v2 shape (`{v:2,adapter,req}`); failures carry fixed kind/message
/// pairs for the in-band error body.
pub fn parse_discovery_request(
    request: &str,
) -> std::result::Result<(String, String), (&'static str, &'static str)> {
    let value: serde_json::Value =
        serde_json::from_str(request).map_err(|_| ("malformed", "request is malformed"))?;
    let map = value
        .as_object()
        .ok_or(("malformed", "request is malformed"))?;
    if map.len() != 3
        || !map.contains_key("v")
        || !map.contains_key("adapter")
        || !map.contains_key("req")
    {
        if map.keys().any(|k| k != "v" && k != "adapter" && k != "req") {
            return Err(("unknown-field", "request contains an unknown field"));
        }
        return Err(("malformed", "request is malformed"));
    }
    if map.get("v").and_then(serde_json::Value::as_u64) != Some(2) {
        return Err(("unsupported-version", "unsupported contract version"));
    }
    let adapter = map
        .get("adapter")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_valid_adapter_id(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    let req = map
        .get("req")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_valid_request_id(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    Ok((adapter.to_owned(), req.to_owned()))
}

fn world_token(raw: Option<&str>, missing: &str) -> String {
    match raw {
        Some(text)
            if !text.is_empty()
                && text.len() <= 64
                && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') =>
        {
            text.to_owned()
        }
        Some(_) => "unknown".to_owned(),
        None => missing.to_owned(),
    }
}

/// Sanitize one correlation/client token for world summaries: alphanumeric
/// plus dash/underscore/dot, capped at 64 chars. Compound `client:cmd`
/// correlations sanitize the same way (colon maps to unknown, so callers
/// pass the two halves joined by `-`).
fn world_id_token(raw: Option<&str>, missing: &str) -> String {
    match raw {
        Some(text)
            if !text.is_empty()
                && text.len() <= 64
                && text
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.') =>
        {
            text.to_owned()
        }
        Some(_) => "unknown".to_owned(),
        None => missing.to_owned(),
    }
}

/// Sanitize one decimal counter for world summaries: digits only, capped
/// at 20 chars.
fn world_counter_token(raw: Option<&str>) -> String {
    match raw {
        Some(text)
            if !text.is_empty() && text.len() <= 20 && text.bytes().all(|b| b.is_ascii_digit()) =>
        {
            text.to_owned()
        }
        _ => "-".to_owned(),
    }
}

/// Request correlation plus route for one authorized world call: capture
/// envelopes contribute the source, queue/take the client and per-client
/// command id, read/discovery the correlation. Pure over the request
/// string; never echoes payload bytes.
fn ingress_identity(request: &str) -> (String, String, String) {
    let parsed: serde_json::Value = serde_json::from_str(request).unwrap_or_default();
    let object = parsed.as_object();
    if object.and_then(|o| o.get("report")).is_some() {
        let source = world_id_token(
            object
                .and_then(|o| o.get("source"))
                .and_then(serde_json::Value::as_str),
            "-",
        );
        let watermark = world_counter_token(
            object
                .and_then(|o| o.get("nativeInvalid"))
                .and_then(serde_json::Value::as_str),
        );
        return (source, "capture".to_owned(), watermark);
    }
    if object.and_then(|o| o.get("command")).is_some() {
        let client = object
            .and_then(|o| o.get("client"))
            .and_then(serde_json::Value::as_str);
        let cmd = object
            .and_then(|o| o.get("clientCmd"))
            .and_then(serde_json::Value::as_str);
        let correlation = match (client, cmd) {
            (Some(client), Some(cmd)) => format!("{client}-{cmd}"),
            _ => "-".to_owned(),
        };
        return (
            world_id_token(Some(&correlation), "-"),
            "queue".to_owned(),
            "-".to_owned(),
        );
    }
    if object.and_then(|o| o.get("clientCmd")).is_some() {
        let client = object
            .and_then(|o| o.get("client"))
            .and_then(serde_json::Value::as_str);
        let cmd = object
            .and_then(|o| o.get("clientCmd"))
            .and_then(serde_json::Value::as_str);
        let correlation = match (client, cmd) {
            (Some(client), Some(cmd)) => format!("{client}-{cmd}"),
            _ => "-".to_owned(),
        };
        return (
            world_id_token(Some(&correlation), "-"),
            "take".to_owned(),
            "-".to_owned(),
        );
    }
    let correlation = world_id_token(
        object
            .and_then(|o| o.get("req"))
            .and_then(serde_json::Value::as_str),
        "-",
    );
    let op = if object.and_then(|o| o.get("adapter")).is_some() {
        "discover"
    } else if object.is_some() {
        "read"
    } else {
        "unknown"
    };
    (correlation, op.to_owned(), "-".to_owned())
}

/// Bounded ingress summary for one authorized world call. Pure over the
/// request string; never echoes payload beyond the correlation, route, and
/// source watermark.
#[must_use]
pub fn world_ingress_summary(request: &str) -> String {
    let (correlation, op, watermark) = ingress_identity(request);
    format!(
        "omnitiler:world-summary direction=ingress req={correlation} cmd={op} nwinv={watermark}"
    )
}

/// Bounded egress summary for one authorized world outcome. Pure over the
/// request/reply strings; never echoes payload beyond correlation, outcome,
/// kind, the fixed check-vocabulary message, and revision.
#[must_use]
pub fn world_egress_summary(request: &str, reply: &str) -> String {
    let (correlation, _, _) = ingress_identity(request);
    let reply_value: serde_json::Value = serde_json::from_str(reply).unwrap_or_default();
    let object = reply_value.as_object();
    let outcome = match object
        .and_then(|o| o.get("outcome"))
        .and_then(serde_json::Value::as_str)
    {
        Some("rejected") => "rejected".to_owned(),
        Some(outcome) => world_token(Some(outcome), "unknown"),
        None => {
            let consumed = object
                .and_then(|o| o.get("consumed"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if consumed {
                "consumed".to_owned()
            } else {
                "accepted".to_owned()
            }
        }
    };
    let kind = match outcome.as_str() {
        "rejected" | "failed" => world_token(
            object
                .and_then(|o| o.get("kind"))
                .and_then(serde_json::Value::as_str),
            "-",
        ),
        _ => "-".to_owned(),
    };
    let revision = match outcome.as_str() {
        "queued" | "ready" | "failed" | "duplicate" => world_counter_token(
            object
                .and_then(|o| o.get("globalCmd"))
                .and_then(serde_json::Value::as_str),
        ),
        "pending" => "-".to_owned(),
        _ => world_counter_token(
            object
                .and_then(|o| o.get("rev"))
                .and_then(serde_json::Value::as_str),
        ),
    };
    // Fixed check-vocabulary message on rejections and terminal command
    // failures (dash tokens pass through; sentence messages and absent
    // fields sanitize to unknown/-). Non-rejections carry no message.
    let msg = match outcome.as_str() {
        "rejected" | "failed" => world_token(
            object
                .and_then(|o| o.get("message"))
                .and_then(serde_json::Value::as_str),
            "-",
        ),
        _ => "-".to_owned(),
    };
    format!(
        "omnitiler:world-summary direction=egress req={correlation} outcome={outcome} kind={kind} rev={revision} msg={msg}"
    )
}

/// Helper for poison-flag hygiene on the std mutex: no-op when healthy,
/// clears the flag after the caller replaced the backend through
/// `PoisonError::into_inner`.
trait ClearPoison {
    fn clear_poison_if_needed(&self);
}

impl ClearPoison for Mutex<WorldBackend> {
    fn clear_poison_if_needed(&self) {
        // `clear_poison` is stable on `Mutex::clear_poison`; call it only
        // when poisoned to avoid touching healthy state.
        if self.is_poisoned() {
            self.clear_poison();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minted_generation_is_valid_and_unique() {
        let first = mint_core_generation();
        let second = mint_core_generation();
        assert_ne!(first, second);
        // Validity is the engine parse gate itself: both tokens construct.
        assert!(tiler_protocol::world_protocol::WorldEngine::new(&first).is_some());
        assert!(tiler_protocol::world_protocol::WorldEngine::new(&second).is_some());
    }

    #[test]
    fn discovery_is_not_ready_without_state() {
        let service = WorldService::new();
        let body = service.discovery_body("adapter-1", "req-1");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(parsed["ready"], false);
        assert_eq!(parsed["rev"], "0");
        assert_eq!(parsed["windows"].as_array().map(Vec::len), Some(0));
        assert_eq!(
            parsed["gen"],
            serde_json::Value::String(service.generation())
        );
        // The reply binds the broker incarnation, never the caller's id.
        let incarnation = parsed["adapter"].as_str().expect("broker incarnation");
        assert!(!incarnation.is_empty());
        assert_ne!(incarnation, "adapter-1");
    }

    #[test]
    fn discovery_parse_is_strict_v2() {
        assert!(parse_discovery_request("{\"v\":2,\"adapter\":\"a-1\",\"req\":\"r-1\"}").is_ok());
        // v1 refuses: no compatibility route.
        assert_eq!(
            parse_discovery_request("{\"v\":1,\"adapter\":\"a-1\",\"req\":\"r-1\"}"),
            Err(("unsupported-version", "unsupported contract version"))
        );
        assert_eq!(
            parse_discovery_request("{\"v\":2,\"adapter\":\"a-1\",\"req\":\"r-1\",\"x\":1}"),
            Err(("unknown-field", "request contains an unknown field"))
        );
    }

    #[test]
    fn broker_keys_inside_report_refuse_with_source_binding() {
        let service = WorldService::new();
        // The serving generation comes from the bootstrap: the first
        // discovery binds the lifetime and mints its generation.
        let discovery: serde_json::Value =
            serde_json::from_str(&service.discovery_body("adapter-1", "req-0"))
                .expect("valid JSON");
        let generation = discovery["gen"].as_str().expect("gen");
        let envelope = format!(
            "{{\"v\":2,\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
            \"source\":\"src-1\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\
            \"report\":{{\"v\":2,\"mode\":\"per-output-local\",\"screens\":[],\"windows\":[],\
            \"views\":[],\"closed\":[],\"hostOwned\":false,\"equalCount\":false,\"native\":[],\
            \"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\
            \"defaultTiled\":true,\"focused\":\"\",\"complete\":false,\"gesture\":false,\
            \"switchTarget\":\"cursor-output\",\"freshPlacement\":\"cursor-output-last-focused\",\
            \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\"actionOutputs\":[],\
            \"focusFollowsCursor\":false,\"cursorWarp\":false,\
            \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"cloaked\":false,\"hiddenWorkspace\":false,\"shell\":false,\
            \"frame\":null,\"borderEnabled\":false,\"underlayEnabled\":false,\"borderGap\":0,\
            \"borderWidth\":0,\"underlayExtension\":0,\"chordHeld\":false,\"moveWindow\":\"\",\
            \"paused\":false,\"pagerOrder\":[],\"pagerFrames\":[]}},\
            \"owner\":\"x\",\"adapter\":\"y\",\"gen\":\"gen-stale\",\"obs\":\"1\",\"reqSeq\":\"1\"}}}}"
        );
        // Broker-owned keys inside the report refuse before dispatch:
        // exactly one sequencing namespace exists.
        let reply = service.evaluate_capture(&envelope).expect("in-band");
        let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
        assert_eq!(parsed["outcome"], "rejected");
        assert_eq!(parsed["kind"], "unknown-field");
        assert_eq!(parsed["source"], "src-1");
        assert_eq!(parsed["nativeInvalid"], "1");
    }

    #[test]
    fn oversize_request_fails_closed() {
        let service = WorldService::new();
        let big = "x".repeat(WORLD_SERVICE_MAX_REQUEST + 1);
        match service.evaluate_capture(&big) {
            Err(WorldServiceError::Oversized) => {}
            other => panic!("expected oversized, got {other:?}"),
        }
    }

    #[test]
    fn world_lock_is_non_queuing() {
        let service = WorldService::new();
        let guard = service.world_lock.try_lock();
        assert!(guard.is_some());
        assert!(!service.try_acquire());
        drop(guard);
        assert!(service.try_acquire());
    }

    #[test]
    fn poisoned_backend_replaces_under_a_fresh_generation() {
        let service = WorldService::new();
        let stale = service.generation();
        // Poison the backend mutex by panicking while holding the guard.
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = service.backend.lock().unwrap();
            panic!("intentional poison for replacement coverage");
        }));
        assert!(poisoned.is_err());
        assert!(service.backend.is_poisoned());
        // Discovery serves from the replacement: a fresh generation with
        // ready=false and no surviving state.
        let body = service.discovery_body("adapter-1", "req-1");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let fresh = parsed["gen"].as_str().expect("gen").to_owned();
        assert_ne!(fresh.as_str(), stale.as_str());
        assert_eq!(parsed["ready"], false);
        assert_eq!(parsed["rev"], "0");
        assert_eq!(service.generation(), fresh);
        assert!(!service.backend.is_poisoned());
    }

    #[test]
    fn egress_summary_names_rejection_reason_without_content() {
        // Fixed check-vocabulary messages surface as the msg token so one
        // daemon line diagnoses the rejection; anything else sanitizes and
        // no request/reply content ever echoes.
        let request = "{\"source\":\"src-7\",\"nativeInvalid\":\"2\",\"report\":{}}";
        let fixed = "{\"outcome\":\"rejected\",\"kind\":\"snapshot-invalid\",\
            \"message\":\"duplicate-window\",\"gen\":\"gen-1\"}";
        let line = world_egress_summary(request, fixed);
        assert_eq!(
            line,
            "omnitiler:world-summary direction=egress req=src-7 \
             outcome=rejected kind=snapshot-invalid rev=- msg=duplicate-window"
        );
        // Sentence messages and hostile correlation sanitize.
        let hostile_request = "{\"source\":\"bad src!\",\"nativeInvalid\":\"2\",\"report\":{}}";
        let hostile_reply = "{\"outcome\":\"rejected\",\"kind\":\"snapshot-invalid\",\
            \"message\":\"observation of win-secret-1 failed\",\"gen\":\"gen-1\"}";
        let line = world_egress_summary(hostile_request, hostile_reply);
        assert_eq!(
            line,
            "omnitiler:world-summary direction=egress req=unknown \
             outcome=rejected kind=snapshot-invalid rev=- msg=unknown"
        );
        assert!(
            !line.contains("secret"),
            "no content echoes into the summary"
        );
        // Accepted captures and queued commands carry their sequences.
        let accepted = "{\"consumed\":false,\"rev\":\"3\"}";
        let line = world_egress_summary(request, accepted);
        assert_eq!(
            line,
            "omnitiler:world-summary direction=egress req=src-7 \
             outcome=accepted kind=- rev=3 msg=-"
        );
        let queued_request = "{\"v\":2,\"gen\":\"gen-1\",\"client\":\"panel-1\",\"clientCmd\":\"4\",\
             \"command\":{\"op\":\"focus\"}}";
        let queued = "{\"v\":2,\"outcome\":\"queued\",\"gen\":\"gen-1\",\"client\":\"panel-1\",\
            \"clientCmd\":\"4\",\"globalCmd\":\"9\"}";
        let line = world_egress_summary(queued_request, queued);
        assert_eq!(
            line,
            "omnitiler:world-summary direction=egress req=panel-1-4 \
             outcome=queued kind=- rev=9 msg=-"
        );
        // Terminal command failures carry their kind and global sequence.
        let failed = "{\"v\":2,\"outcome\":\"failed\",\"gen\":\"gen-1\",\"globalCmd\":\"9\",\
            \"kind\":\"selection-unknown\",\"message\":\"selection target is not a known workspace\"}";
        let line = world_egress_summary(queued_request, failed);
        assert_eq!(
            line,
            "omnitiler:world-summary direction=egress req=panel-1-4 \
             outcome=failed kind=selection-unknown rev=9 msg=unknown"
        );
    }

    #[test]
    fn ingress_summary_names_route_and_watermark() {
        let capture = "{\"v\":2,\"source\":\"src-9\",\"nativeInvalid\":\"12\",\"report\":{}}";
        assert_eq!(
            world_ingress_summary(capture),
            "omnitiler:world-summary direction=ingress req=src-9 cmd=capture nwinv=12"
        );
        let read = "{\"v\":2,\"req\":\"read-3\"}";
        assert_eq!(
            world_ingress_summary(read),
            "omnitiler:world-summary direction=ingress req=read-3 cmd=read nwinv=-"
        );
    }
}
