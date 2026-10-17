//! World planner service boundary: retained [`WorldEngine`] behind JSON.
//!
//! D-Bus surface (same service/object/interface as the legacy route,
//! [`crate::planner_service`]): `DiscoverWorld` returns the bootstrap
//! envelope, `DescribeWorld` accepts one complete JSON world report per
//! call and returns one JSON full desired snapshot or a bounded error.
//! The legacy `DescribePlan` route remains for transitional compatibility;
//! the KDE production entry uses only the world route, never a fallback.
//!
//! Rules mirrored from the legacy route: bounded 4 MiB request AND reply,
//! non-queuing single-flight (contended callers fail fast as busy, never
//! queue and never acknowledge), poisoned-lock replacement under a FRESH
//! core generation (never silently continuing on a partial engine), and
//! redacted diagnostics without payload bytes. D-Bus caller authentication
//! (same-UID plus the current KWin unique owner as the single active
//! adapter) lives in [`crate::planner_service`]; this type never touches
//! the bus and never authorizes.
//!
//! Generation lifecycle: the endpoint mints a fresh core generation at
//! construction (`w-<nanos>-<pid>-<counter>`, lowercase/digit/dash only).
//! Discovery binds the adapter incarnation to that generation and returns
//! `ready=false` with no applicable desired state. Every report binds its
//! `gen` against the serving generation; mismatches reject without state.
//! Poison recovery mints a new generation and constructs a new engine, so
//! old effects, flights, ordering, and desired caches can never leak
//! across the replacement: the adapter must re-discover and re-observe.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tiler_protocol::world_protocol::{
    WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, WorldEngine, WorldOutcome, decode_request,
    encode_error, encode_reply, is_valid_adapter_id, is_valid_request_id,
};

/// World planning route: one complete JSON world observation plus at most
/// one command in, one full desired snapshot or a bounded rejection out.
pub const WORLD_METHOD: &str = "DescribeWorld";
/// World discovery/bootstrap route: generation plus capabilities, always
/// `ready=false` with no applicable desired state.
pub const DISCOVERY_METHOD: &str = "DiscoverWorld";
/// Bounded world request cap (mirrors the protocol/core budget).
pub const WORLD_SERVICE_MAX_REQUEST: usize = WORLD_MAX_REQUEST_BYTES;
/// Bounded world reply cap (mirrors the protocol/core budget).
pub const WORLD_SERVICE_MAX_REPLY: usize = WORLD_MAX_REPLY_BYTES;

/// Process-wide generation counter so construction and poison replacement
/// never mint the same token within one process lifetime.
static GENERATION_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Mint a fresh core generation token (`w-<nanos>-<pid>-<counter>`):
/// lowercase/digit/dash only, within the generation bound.
#[must_use]
pub fn mint_core_generation() -> String {
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

/// Retained world backend: one [`WorldEngine`] plus its serving generation.
#[derive(Debug)]
struct WorldBackend {
    engine: WorldEngine,
    generation: String,
}

impl WorldBackend {
    fn fresh() -> Self {
        let generation = mint_core_generation();
        let engine = WorldEngine::new(&generation).expect("minted generation is valid");
        Self { engine, generation }
    }
}

/// World planner endpoint: pure request evaluation plus the single-flight
/// lock. D-Bus authentication (same-UID) and connection checks live in
/// [`crate::planner_service`]; this type never touches the bus.
#[derive(Clone, Debug)]
pub struct WorldService {
    backend: Arc<Mutex<WorldBackend>>,
    world_lock: Arc<async_lock::Mutex<()>>,
}

impl WorldService {
    /// Fresh backend incarnation: new core generation, empty engine.
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
            Ok(backend) => backend.generation.clone(),
            Err(_) => String::new(),
        }
    }

    /// Non-queuing single-flight probe for tests: `true` when the world
    /// route is idle.
    #[must_use]
    pub fn try_acquire(&self) -> bool {
        self.world_lock.try_lock().is_some()
    }

    /// Non-queuing single-flight admission for the D-Bus route: `Some`
    /// guard when idle, `None` when a world call is in flight (fail fast
    /// as busy, never queue). The guard must be held for the whole call,
    /// including caller verification, exactly like the legacy plan lock.
    pub fn try_acquire_guard(&self) -> Option<async_lock::MutexGuard<'_, ()>> {
        self.world_lock.try_lock()
    }

    /// Discovery/bootstrap body for an adapter incarnation and request id:
    /// generation plus capabilities with `ready=false` and no applicable
    /// desired state. The adapter sends a full report before any ready
    /// reply; empty pre-bootstrap state never authorizes removal.
    #[must_use]
    pub fn discovery_body(&self, adapter_incarnation: &str, request_id: &str) -> String {
        let (generation, reply) = match self.backend.lock() {
            Ok(backend) => {
                let reply = backend
                    .engine
                    .bootstrap_reply(adapter_incarnation, request_id);
                (backend.generation.clone(), reply)
            }
            Err(poison) => {
                // Poisoned backend: replace under a fresh generation, then
                // serve discovery from the replacement (never from partial
                // state). The adapter must re-observe; nothing is claimed
                // to have survived.
                let mut backend = poison.into_inner();
                *backend = WorldBackend::fresh();
                let reply = backend
                    .engine
                    .bootstrap_reply(adapter_incarnation, request_id);
                eprintln!("omnitiler:world-fault kind=poisoned-lock detail=replaced-with-fresh");
                (backend.generation.clone(), reply)
            }
        };
        self.backend.clear_poison_if_needed();
        let encoded = encode_reply(&reply);
        debug_assert_eq!(generation, reply.core_generation);
        encoded
    }

    /// Bounded in-band rejection body bound to the serving generation.
    /// Poison degrades to an empty generation rather than failing.
    #[must_use]
    pub fn error_body(&self, kind: &str, message: &str) -> String {
        let generation = self.generation();
        encode_error(kind, message, &generation)
    }

    /// Evaluate one JSON world request end to end: byte-bound the request,
    /// decode through the protocol DTO, run the retained engine
    /// transactionally, and byte-bound the protocol-encoded reply.Accepted
    /// (and duplicate/consumed) outcomes return the ready full-state reply;
    /// core rejections return bounded in-band errors, never ready state.
    /// Only oversize replies fail closed as [`WorldServiceError`].
    pub fn evaluate_world_request(
        &self,
        request: &str,
    ) -> std::result::Result<String, WorldServiceError> {
        if request.len() > WORLD_SERVICE_MAX_REQUEST {
            return Err(WorldServiceError::Oversized);
        }
        let report = match decode_request(request) {
            Ok(report) => report,
            Err(error) => {
                return Ok(self.error_body(error.kind, error.message));
            }
        };
        let outcome = match self.backend.lock() {
            Ok(mut backend) => backend.engine.handle_report(&report),
            Err(poison) => {
                let outcome = {
                    let mut backend = poison.into_inner();
                    *backend = WorldBackend::fresh();
                    backend.engine.handle_report(&report)
                };
                self.backend.clear_poison_if_needed();
                eprintln!("omnitiler:world-fault kind=poisoned-lock detail=replaced-with-fresh");
                outcome
            }
        };
        match outcome {
            WorldOutcome::Accepted(reply)
            | WorldOutcome::Duplicate(reply)
            | WorldOutcome::CommandConsumed(reply) => {
                let encoded = encode_reply(&reply);
                if encoded.len() > WORLD_SERVICE_MAX_REPLY {
                    return Err(WorldServiceError::Unavailable(
                        "reply exceeds size bound".to_owned(),
                    ));
                }
                Ok(encoded)
            }
            WorldOutcome::Rejected { kind, message } => Ok(self.error_body(kind, message)),
        }
    }
}

impl Default for WorldService {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed discovery/bootstrap request: adapter incarnation plus request
/// id. Strict shape (`{v:1,adapter,req}`); failures carry fixed
/// kind/message pairs for the in-band error body.
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
    if map.get("v").and_then(serde_json::Value::as_u64) != Some(1) {
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

/// Sanitize one correlation/generation token for world summaries:
/// alphanumeric plus dash/underscore/dot, capped at 64 chars.
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

/// Bounded ingress summary for one authorized world call. Pure over the
/// request string; never echoes payload beyond the correlation and the
/// command op token (`observe` for observation-only reports).
#[must_use]
pub fn world_ingress_summary(request: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(request).unwrap_or_default();
    let object = parsed.as_object();
    let req = world_id_token(
        object
            .and_then(|o| o.get("req"))
            .and_then(serde_json::Value::as_str),
        "-",
    );
    let cmd = match object.and_then(|o| o.get("command")) {
        None => "unknown".to_owned(),
        Some(serde_json::Value::Null) => "observe".to_owned(),
        Some(value) => world_token(
            value
                .as_object()
                .and_then(|o| o.get("op"))
                .and_then(serde_json::Value::as_str),
            "unknown",
        ),
    };
    format!("omnitiler:world-summary direction=ingress req={req} cmd={cmd}")
}

/// Bounded egress summary for one authorized world outcome. Pure over the
/// request/reply strings; never echoes payload beyond correlation, outcome,
/// kind, the fixed check-vocabulary message, and revision.
#[must_use]
pub fn world_egress_summary(request: &str, reply: &str) -> String {
    let request_value: serde_json::Value = serde_json::from_str(request).unwrap_or_default();
    let req = world_id_token(
        request_value
            .as_object()
            .and_then(|o| o.get("req"))
            .and_then(serde_json::Value::as_str),
        "-",
    );
    let reply_value: serde_json::Value = serde_json::from_str(reply).unwrap_or_default();
    let object = reply_value.as_object();
    let (outcome, kind, rev) = match object
        .and_then(|o| o.get("outcome"))
        .and_then(serde_json::Value::as_str)
    {
        Some("rejected") => (
            "rejected".to_owned(),
            world_token(
                object
                    .and_then(|o| o.get("kind"))
                    .and_then(serde_json::Value::as_str),
                "-",
            ),
            "-".to_owned(),
        ),
        _ => {
            let consumed = object
                .and_then(|o| o.get("consumed"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            (
                if consumed {
                    "consumed".to_owned()
                } else {
                    "accepted".to_owned()
                },
                "-".to_owned(),
                world_counter_token(
                    object
                        .and_then(|o| o.get("rev"))
                        .and_then(serde_json::Value::as_str),
                ),
            )
        }
    };
    // Fixed check-vocabulary message on rejections (dash tokens pass
    // through; sentence messages and absent fields sanitize to unknown/-).
    // Accepted replies carry no message.
    let msg = match object
        .and_then(|o| o.get("outcome"))
        .and_then(serde_json::Value::as_str)
    {
        Some("rejected") => world_token(
            object
                .and_then(|o| o.get("message"))
                .and_then(serde_json::Value::as_str),
            "-",
        ),
        _ => "-".to_owned(),
    };
    format!(
        "omnitiler:world-summary direction=egress req={req} outcome={outcome} kind={kind} rev={rev} msg={msg}"
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
        assert!(WorldEngine::new(&first).is_some());
        assert!(WorldEngine::new(&second).is_some());
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
    }

    #[test]
    fn unknown_generation_rejects_inband() {
        let service = WorldService::new();
        let request = "{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\
             \"gen\":\"gen-stale\",\"obs\":\"1\",\"reqSeq\":\"1\",\"req\":\"req-1\",\
             \"cmdSeq\":null,\"command\":null,\"mode\":\"per-output-local\",\
             \"topology\":[],\"bindings\":[],\"domains\":[],\"minimized\":[],\
             \"predicate\":\"both-axes-fixed\",\"defaultTiled\":true,\"focused\":\"\",\
             \"complete\":true,\
             \"gesture\":false,\"feedback\":[]}"
            .to_owned();
        let reply = service.evaluate_world_request(&request).expect("in-band");
        let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
        assert_eq!(parsed["outcome"], "rejected");
        assert_eq!(parsed["kind"], "generation-mismatch");
        assert_eq!(
            parsed["gen"],
            serde_json::Value::String(service.generation())
        );
    }

    #[test]
    fn oversize_request_fails_closed() {
        let service = WorldService::new();
        let big = "x".repeat(WORLD_SERVICE_MAX_REQUEST + 1);
        match service.evaluate_world_request(&big) {
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
        assert_ne!(fresh, stale.as_str());
        assert_eq!(parsed["ready"], false);
        assert_eq!(parsed["rev"], "0");
        assert_eq!(service.generation(), fresh);
        assert!(!service.backend.is_poisoned());
        // The old generation stays rejected, now bound to the replacement.
        let request = format!(
            "{{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\"gen\":\"{stale}\",\
             \"obs\":\"1\",\"reqSeq\":\"1\",\"req\":\"req-1\",\"cmdSeq\":null,\"command\":null,\
             \"mode\":\"per-output-local\",\"topology\":[],\"bindings\":[],\"domains\":[],\
             \"minimized\":[],\"predicate\":\"both-axes-fixed\",\"defaultTiled\":true,\
             \"focused\":\"\",\
             \"complete\":true,\"gesture\":false,\"feedback\":[]}}"
        );
        let reply = service.evaluate_world_request(&request).expect("in-band");
        let parsed: serde_json::Value = serde_json::from_str(&reply).expect("valid JSON");
        assert_eq!(parsed["kind"], "generation-mismatch");
        assert_eq!(parsed["gen"].as_str(), Some(fresh.as_str()));
    }

    #[test]
    fn egress_summary_names_rejection_reason_without_content() {
        // Fixed check-vocabulary messages surface as the msg token so one
        // daemon line diagnoses the rejection; anything else sanitizes and
        // no request/reply content ever echoes.
        let request = "{\"req\":\"req-7\"}";
        let fixed = "{\"outcome\":\"rejected\",\"kind\":\"snapshot-invalid\",\
            \"message\":\"duplicate-window\",\"gen\":\"gen-1\"}";
        let line = world_egress_summary(request, fixed);
        assert_eq!(
            line,
            "omnitiler:world-summary direction=egress req=req-7 \
             outcome=rejected kind=snapshot-invalid rev=- msg=duplicate-window"
        );
        // Sentence messages and hostile correlation/content sanitize.
        let hostile_request = "{\"req\":\"bad req!\",\"command\":{\"op\":\"send secret\"}}";
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
        // Accepted replies carry no message.
        let accepted = "{\"consumed\":false,\"rev\":\"3\"}";
        let line = world_egress_summary(request, accepted);
        assert_eq!(
            line,
            "omnitiler:world-summary direction=egress req=req-7 \
             outcome=accepted kind=- rev=3 msg=-"
        );
    }
}
