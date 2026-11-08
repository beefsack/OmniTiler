//! World planner service boundary: retained sole [`WorldBroker`] behind JSON.
//!
//! D-Bus surface (same service/object/interface as the intent routes,
//! [`crate::planner_service`]): `DiscoverWorld` routes one v4
//! connection-family message per call (`handshake` or `settings`, dispatched
//! by `type`) and returns the connection reply, `DescribeWorld` accepts one
//! broker capture envelope per call and
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
//! Generation lifecycle: the broker mints a core generation seed at
//! construction (no broker incarnation). The adapter is the sole producer:
//! every successful handshake through the connection router clears all broker
//! ancillary state, including the first handshake whose generation equals the
//! construction seed, and binds the single active producer capture lifetime;
//! settings apply under their own revision rules without resetting. Replies
//! echo the producer lifetime with the serving generation, `ready=false`,
//! and no applicable desired state. Same-lifetime handshakes reset the same
//! way (all capture/queue/result/presentation state dropped), so no
//! old capture can ever apply again. Every capture binds its stamped `gen`
//! against the serving generation; mismatches reject without state. Poison
//! recovery mints a new generation and a new broker, so old effects,
//! flights, ordering, queued commands, settled results, and desired caches
//! can never leak across the replacement: the adapter must re-discover and
//! re-observe, and command issuers must re-establish with fresh per-client
//! command ids.

use std::sync::{Arc, Mutex};

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tiler_protocol::world_protocol::{WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES};

use crate::world_broker::{BrokerError, WorldBroker};

/// World capture/actuator route: one broker capture envelope in, one bound
/// response out.
pub const WORLD_METHOD: &str = "DescribeWorld";
/// World discovery/bootstrap route: generation plus capabilities, always
/// `ready=false` with no applicable desired state.
pub const DISCOVERY_METHOD: &str = "DiscoverWorld";
/// External command queue route: one strict v4 command in, queued/duplicate out.
pub const QUEUE_METHOD: &str = "QueueWorldCommand";
/// Command result route: exactly-once terminal result collection.
pub const TAKE_METHOD: &str = "TakeCommandResult";
/// Presentation query route: cached accepted presentation, no one-shot lanes.
pub const READ_METHOD: &str = "ReadWorldPresentation";
/// External wait route: one long-held `WaitWorldWake` call per generation.
/// The parked call completes on queued work, supersession, or the idle
/// lease below. The wait never executes native work or engine logic and
/// never holds the capture single-flight lock; a still-parked command
/// drains on the follow-up capture.
pub const WAIT_METHOD: &str = "WaitWorldWake";
/// Idle server lease in milliseconds: the parked wait answers `lease`
/// (cursor unchanged) at the latest after this long. A heuristic under the
/// 25 s host call deadline, NOT a proven scheduling margin: the client
/// watchdog covers late or missing replies.
pub const WAIT_LEASE_MS: u64 = 20_000;
/// Verified host call deadline in milliseconds: KWin `callDBus` issues a
/// nonblocking `sessionBus().asyncCall` with no timeout argument (default
/// -1), Qt passes that through to libdbus, and libdbus substitutes
/// `_DBUS_DEFAULT_TIMEOUT_VALUE` (25 s); errors and timeouts only log and
/// never invoke the JS callback. Every long-held `callDBus` therefore
/// carries a watchdog at this deadline that rediscovers and re-arms even
/// with zero native events.
pub const HOST_CALL_DEADLINE_MS: u64 = 25_000;
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
    /// Fresh backend incarnation: new core generation seed, empty engine and command state.
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

    /// Discovery route for v4 connection-family messages (`handshake` and
    /// `settings`, dispatched by `type` through the connection router).
    /// Only a successful handshake resets; settings keep their revision
    /// rules. The adapter sends a full capture before any ready reply.
    pub fn discover(&self, request: &str) -> std::result::Result<String, WorldServiceError> {
        if request.len() > WORLD_SERVICE_MAX_REQUEST {
            return Err(WorldServiceError::Oversized);
        }
        self.run_broker(|broker| broker.discover_connection(request))
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
    /// broker-owned sequencing, joint-head plus synchronous drain of any
    /// parked commands), and byte-bound
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

    /// File one external command: it parks for the next fresh capture,
    /// which drains the whole FIFO batch in order. Pure broker state:
    /// never waits on the capture lock.
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

    /// Park one `WaitWorldWake` call: immediate wake when the broker epoch
    /// advanced past the cursor with work pending, in-band rejection for a
    /// stale generation or unknown lifetime, otherwise a parked waiter that
    /// completes on queued work, supersession, or the idle lease. The
    /// future never holds the backend lock across an await and never takes
    /// the capture single-flight lock. Poison replaces the backend under a
    /// fresh generation first, so the stranded call answers
    /// generation-mismatch and the waiter re-establishes after
    /// re-discovery.
    pub fn wait_for_wake(
        &self,
        lifetime: String,
        generation: String,
        nonce: String,
        cursor: u64,
    ) -> WorldWait {
        self.wait_with_lease(lifetime, generation, nonce, cursor, WAIT_LEASE_MS)
    }

    /// Lease-parameterized wait constructor. The production lease is
    /// [`WAIT_LEASE_MS`]; tests drive shorter leases through this seam
    /// without a public option.
    fn wait_with_lease(
        &self,
        lifetime: String,
        generation: String,
        nonce: String,
        cursor: u64,
        lease_ms: u64,
    ) -> WorldWait {
        WorldWait {
            service: self.clone(),
            lifetime,
            generation,
            nonce,
            cursor,
            lease_ms,
            timer: None,
        }
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
                let displaced = backend.broker.take_parked_waiter();
                *backend = WorldBackend::fresh();
                if let Some(waiter) = displaced {
                    backend.broker.complete_displaced(waiter);
                }
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

/// One parked `WaitWorldWake` call. Registration (immediate wake, park, or
/// rejection) happens under a single short backend critical section on the
/// first poll; completion, supersession, and lease expiry each re-lock
/// briefly. The backend lock is never held across an await.
pub struct WorldWait {
    service: WorldService,
    lifetime: String,
    generation: String,
    nonce: String,
    cursor: u64,
    lease_ms: u64,
    timer: Option<Pin<Box<async_io::Timer>>>,
}

impl Drop for WorldWait {
    fn drop(&mut self) {
        // Best-effort owned-nonce cleanup: try-lock only, never block in
        // Drop. Only this nonce's waiter slot and its uncollected ready are
        // touched, so a dropped waiter can never displace a newer one;
        // supersession and lease expiry self-heal anything left behind.
        if let Ok(mut backend) = self.service.backend.try_lock() {
            backend.broker.expire_wait(&self.nonce);
            backend.broker.take_wait_ready(&self.nonce);
        }
    }
}

impl Future for WorldWait {
    type Output = String;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<String> {
        let this = &mut *self;
        if this.timer.is_none() {
            // Single registration critical section: immediate wake, park
            // with this waker, or in-band rejection.
            match this.service.backend.lock() {
                Ok(mut backend) => {
                    match backend.broker.wait_immediate(
                        &this.lifetime,
                        &this.generation,
                        this.cursor,
                    ) {
                        Ok(Some(wake)) => {
                            this.service.backend.clear_poison_if_needed();
                            return Poll::Ready(wake);
                        }
                        Ok(None) => {
                            backend.broker.wait_park(
                                &this.nonce,
                                this.cursor,
                                Some(cx.waker().clone()),
                            );
                            this.service.backend.clear_poison_if_needed();
                            this.timer = Some(Box::pin(async_io::Timer::after(
                                std::time::Duration::from_millis(this.lease_ms),
                            )));
                        }
                        Err(rejection) => {
                            this.service.backend.clear_poison_if_needed();
                            return Poll::Ready(rejection);
                        }
                    }
                }
                Err(poison) => {
                    let mut backend = poison.into_inner();
                    *backend = WorldBackend::fresh();
                    let reply = backend
                        .broker
                        .error_body("generation-mismatch", "generation mismatch");
                    eprintln!(
                        "omnitiler:world-fault kind=poisoned-lock detail=replaced-with-fresh"
                    );
                    this.service.backend.clear_poison_if_needed();
                    return Poll::Ready(reply);
                }
            }
        }
        // Parked: a delivered wake wins over expiry, and expiry wins over
        // further parking. A stale bound generation answers mismatch, never
        // superseded. All clocks here are relative lease durations; the
        // broker clock stays the authority for attempt stamps.
        match this.service.backend.lock() {
            Ok(mut backend) => {
                if let Some(wake) = backend.broker.take_wait_ready(&this.nonce) {
                    this.service.backend.clear_poison_if_needed();
                    return Poll::Ready(wake);
                }
                if this.generation != backend.broker.generation() {
                    let reply = backend
                        .broker
                        .error_body("generation-mismatch", "generation mismatch");
                    this.service.backend.clear_poison_if_needed();
                    return Poll::Ready(reply);
                }
                if backend.broker.waiter_nonce() != Some(this.nonce.as_str()) {
                    let reply = backend.broker.wait_superseded_body();
                    this.service.backend.clear_poison_if_needed();
                    return Poll::Ready(reply);
                }
                let timer = this.timer.as_mut().expect("wait timer is armed");
                match timer.as_mut().poll(cx) {
                    Poll::Ready(_) => {
                        let cursor = backend
                            .broker
                            .expire_wait(&this.nonce)
                            .unwrap_or(this.cursor);
                        let reply = backend.broker.wait_lease_body(cursor);
                        this.service.backend.clear_poison_if_needed();
                        Poll::Ready(reply)
                    }
                    Poll::Pending => {
                        this.service.backend.clear_poison_if_needed();
                        Poll::Pending
                    }
                }
            }
            Err(poison) => {
                let mut backend = poison.into_inner();
                *backend = WorldBackend::fresh();
                let reply = backend
                    .broker
                    .error_body("generation-mismatch", "generation mismatch");
                eprintln!("omnitiler:world-fault kind=poisoned-lock detail=replaced-with-fresh");
                this.service.backend.clear_poison_if_needed();
                Poll::Ready(reply)
            }
        }
    }
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
    if object.and_then(|o| o.get("nonce")).is_some() {
        let correlation = world_id_token(
            object
                .and_then(|o| o.get("nonce"))
                .and_then(serde_json::Value::as_str),
            "-",
        );
        return (correlation, "wait".to_owned(), "-".to_owned());
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
        "wake" | "lease" => world_counter_token(
            object
                .and_then(|o| o.get("cursor"))
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

    fn handshake_request(adapter: &str, req: &str) -> String {
        format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":false,\"fullscreen\":false,\"nativeSticky\":false,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        )
    }

    #[test]
    fn discovery_is_not_ready_without_state() {
        let service = WorldService::new();
        let body = service
            .discover(&handshake_request("adapter-1", "req-1"))
            .expect("handshake routes");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(parsed["ready"], false);
        assert_eq!(
            parsed["gen"],
            serde_json::Value::String(service.generation())
        );
        // The reply echoes the producer lifetime, never a broker incarnation.
        assert_eq!(parsed["adapter"], "adapter-1");
        assert_eq!(parsed["req"], "req-1");
    }

    #[test]
    fn discovery_dispatches_handshake_and_settings_by_type() {
        let service = WorldService::new();
        // Malformed connection messages refuse in-band, bound to the serving
        // generation.
        let bad = service
            .discover("{\"type\":\"nope\",\"v\":4}")
            .expect("in-band rejection");
        let parsed: serde_json::Value = serde_json::from_str(&bad).expect("valid JSON");
        assert_eq!(parsed["outcome"], "rejected");
        // Oversize fails closed before routing.
        let big = "x".repeat(WORLD_SERVICE_MAX_REQUEST + 1);
        match service.discover(&big) {
            Err(WorldServiceError::Oversized) => {}
            other => panic!("expected oversized, got {other:?}"),
        }
    }

    #[test]
    fn broker_keys_inside_report_refuse_with_source_binding() {
        let service = WorldService::new();
        // The serving generation comes from the handshake: the first
        // handshake binds the lifetime and mints its generation.
        let discovery: serde_json::Value = serde_json::from_str(
            &service
                .discover(&handshake_request("adapter-1", "req-0"))
                .expect("handshake routes"),
        )
        .expect("valid JSON");
        let generation = discovery["gen"].as_str().expect("gen").to_owned();
        let envelope = format!(
            "{{\"v\":4,\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
            \"source\":\"src-1\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\
            \"report\":{{\"v\":4,\"obs\":\"1\"}}}}"
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
        let body = service
            .discover(&handshake_request("adapter-1", "req-1"))
            .expect("handshake routes");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let fresh = parsed["gen"].as_str().expect("gen").to_owned();
        assert_ne!(fresh.as_str(), stale.as_str());
        assert_eq!(parsed["ready"], false);
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
        let queued_request = "{\"v\":4,\"gen\":\"gen-1\",\"client\":\"panel-1\",\"clientCmd\":\"4\",\
             \"command\":{\"op\":\"focus\"}}";
        let queued = "{\"v\":4,\"outcome\":\"queued\",\"gen\":\"gen-1\",\"client\":\"panel-1\",\
            \"clientCmd\":\"4\",\"globalCmd\":\"9\"}";
        let line = world_egress_summary(queued_request, queued);
        assert_eq!(
            line,
            "omnitiler:world-summary direction=egress req=panel-1-4 \
             outcome=queued kind=- rev=9 msg=-"
        );
        // Terminal command failures carry their kind and global sequence.
        let failed = "{\"v\":4,\"outcome\":\"failed\",\"gen\":\"gen-1\",\"globalCmd\":\"9\",\
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
        let capture = "{\"v\":4,\"source\":\"src-9\",\"nativeInvalid\":\"12\",\"report\":{}}";
        assert_eq!(
            world_ingress_summary(capture),
            "omnitiler:world-summary direction=ingress req=src-9 cmd=capture nwinv=12"
        );
        let read = "{\"v\":4,\"req\":\"read-3\"}";
        assert_eq!(
            world_ingress_summary(read),
            "omnitiler:world-summary direction=ingress req=read-3 cmd=read nwinv=-"
        );
        let wait =
            "{\"v\":4,\"gen\":\"w-1\",\"adapter\":\"a-1\",\"nonce\":\"wait-7\",\"cursor\":\"3\"}";
        assert_eq!(
            world_ingress_summary(wait),
            "omnitiler:world-summary direction=ingress req=wait-7 cmd=wait nwinv=-"
        );
    }

    fn discovered_generation(service: &WorldService) -> (String, String) {
        let discovery: serde_json::Value = serde_json::from_str(
            &service
                .discover(&handshake_request("adapter-1", "req-0"))
                .expect("handshake routes"),
        )
        .expect("valid JSON");
        (
            discovery["gen"].as_str().expect("gen").to_owned(),
            discovery["adapter"]
                .as_str()
                .expect("producer lifetime")
                .to_owned(),
        )
    }

    fn queue_select_numbered(service: &WorldService, generation: &str, client_cmd: &str) {
        let request = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\
              \"clientCmd\":\"{client_cmd}\",\
              \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
        );
        let reply: serde_json::Value =
            serde_json::from_str(&service.queue_command(":1.42", &request).expect("in-band"))
                .expect("valid JSON");
        assert_eq!(reply["outcome"], "queued");
    }

    #[test]
    fn wait_future_answers_immediate_wake_and_rejections() {
        let service = WorldService::new();
        let (generation, _lifetime) = discovered_generation(&service);
        queue_select_numbered(&service, &generation, "1");
        let wake: serde_json::Value =
            serde_json::from_str(&async_io::block_on(service.wait_for_wake(
                "adapter-1".to_owned(),
                generation.clone(),
                "wait-1".to_owned(),
                0,
            )))
            .expect("wake JSON");
        assert_eq!(wake["outcome"], "wake");
        assert_eq!(wake["gen"], serde_json::Value::String(generation.clone()));
        assert_eq!(wake["cursor"], "1");
        // Stale generations and unknown lifetimes reject in-band.
        let stale = async_io::block_on(service.wait_for_wake(
            "adapter-1".to_owned(),
            "w-stale".to_owned(),
            "wait-2".to_owned(),
            0,
        ));
        let stale_value: serde_json::Value = serde_json::from_str(&stale).expect("valid JSON");
        assert_eq!(stale_value["outcome"], "rejected");
        assert_eq!(stale_value["kind"], "generation-mismatch");
        let unknown = async_io::block_on(service.wait_for_wake(
            "other".to_owned(),
            generation.clone(),
            "wait-3".to_owned(),
            0,
        ));
        let unknown_value: serde_json::Value = serde_json::from_str(&unknown).expect("valid JSON");
        assert_eq!(unknown_value["outcome"], "rejected");
        assert_eq!(unknown_value["kind"], "unknown-adapter");
    }

    #[test]
    fn wait_future_delivers_queued_work_to_a_parked_call() {
        let service = WorldService::new();
        let (generation, _lifetime) = discovered_generation(&service);
        let parked = service.clone();
        let waiter = std::thread::spawn(move || {
            async_io::block_on(parked.wait_for_wake(
                "adapter-1".to_owned(),
                generation.clone(),
                "wait-parked".to_owned(),
                0,
            ))
        });
        // Let the waiter park; the queue then completes it without any
        // capture or native event.
        std::thread::sleep(std::time::Duration::from_millis(50));
        queue_select_numbered(&service, &service.generation(), "1");
        let wake: serde_json::Value =
            serde_json::from_str(&waiter.join().expect("waiter resolves")).expect("wake JSON");
        assert_eq!(wake["outcome"], "wake");
        assert_eq!(wake["cursor"], "1");
    }

    #[test]
    fn wait_future_lease_keeps_the_cursor() {
        let service = WorldService::new();
        let (generation, _lifetime) = discovered_generation(&service);
        let lease: serde_json::Value =
            serde_json::from_str(&async_io::block_on(service.wait_with_lease(
                "adapter-1".to_owned(),
                generation.clone(),
                "wait-idle".to_owned(),
                0,
                50,
            )))
            .expect("lease JSON");
        assert_eq!(lease["outcome"], "lease");
        assert_eq!(lease["gen"], serde_json::Value::String(generation));
        assert_eq!(lease["cursor"], "0");
    }

    #[test]
    fn wait_future_answers_superseded_to_the_replaced_call() {
        let service = WorldService::new();
        let (generation, _lifetime) = discovered_generation(&service);
        let first = service.clone();
        let first_generation = generation.clone();
        let replaced = std::thread::spawn(move || {
            async_io::block_on(first.wait_with_lease(
                "adapter-1".to_owned(),
                first_generation,
                "wait-old".to_owned(),
                0,
                5_000,
            ))
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        // A newer registration supersedes the parked call; the lease on the
        // replacement is short so this test never hangs.
        let superseding = async_io::block_on(service.wait_with_lease(
            "adapter-1".to_owned(),
            generation.clone(),
            "wait-new".to_owned(),
            0,
            50,
        ));
        let superseding_value: serde_json::Value =
            serde_json::from_str(&superseding).expect("valid JSON");
        assert_eq!(superseding_value["outcome"], "lease");
        let replaced_value: serde_json::Value =
            serde_json::from_str(&replaced.join().expect("replaced resolves")).expect("valid JSON");
        assert_eq!(replaced_value["outcome"], "superseded");
        assert_eq!(replaced_value["gen"], serde_json::Value::String(generation));
    }

    #[test]
    fn wait_summaries_name_wake_and_lease_without_content() {
        let request =
            "{\"v\":4,\"gen\":\"w-1\",\"adapter\":\"a-1\",\"nonce\":\"wait-7\",\"cursor\":\"3\"}";
        let wake = "{\"v\":4,\"outcome\":\"wake\",\"gen\":\"w-1\",\"cursor\":\"4\"}";
        assert_eq!(
            world_egress_summary(request, wake),
            "omnitiler:world-summary direction=egress req=wait-7 \
             outcome=wake kind=- rev=4 msg=-"
        );
        let lease = "{\"v\":4,\"outcome\":\"lease\",\"gen\":\"w-1\",\"cursor\":\"3\"}";
        assert_eq!(
            world_egress_summary(request, lease),
            "omnitiler:world-summary direction=egress req=wait-7 \
             outcome=lease kind=- rev=3 msg=-"
        );
        let superseded = "{\"v\":4,\"outcome\":\"superseded\",\"gen\":\"w-1\"}";
        assert_eq!(
            world_egress_summary(request, superseded),
            "omnitiler:world-summary direction=egress req=wait-7 \
             outcome=superseded kind=- rev=- msg=-"
        );
    }

    fn park_wait(
        service: &WorldService,
        lifetime: &str,
        generation: &str,
        nonce: &str,
    ) -> std::sync::mpsc::Receiver<String> {
        let parked = service.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let (lifetime, generation, nonce) =
            (lifetime.to_owned(), generation.to_owned(), nonce.to_owned());
        std::thread::spawn(move || {
            let reply = async_io::block_on(parked.wait_for_wake(lifetime, generation, nonce, 0));
            let _ = tx.send(reply);
        });
        // Let the waiter park; every assertion below resolves through the
        // waker or not at all (never through a lease sleep).
        std::thread::sleep(std::time::Duration::from_millis(50));
        rx
    }

    fn recv_immediate(rx: &std::sync::mpsc::Receiver<String>) -> serde_json::Value {
        let reply = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("displacement wakes immediately, never sleeps the lease");
        serde_json::from_str(&reply).expect("valid JSON")
    }

    #[test]
    fn rotation_wakes_the_parked_waiter_with_mismatch_immediately() {
        let service = WorldService::new();
        let (generation, _) = discovered_generation(&service);
        let rx = park_wait(&service, "adapter-1", &generation, "wait-parked");
        let rotated: serde_json::Value = serde_json::from_str(
            &service
                .discover(&handshake_request("adapter-2", "req-1"))
                .expect("handshake routes"),
        )
        .expect("valid JSON");
        let fresh = rotated["gen"].as_str().expect("gen").to_owned();
        assert_ne!(fresh, generation);
        let value = recv_immediate(&rx);
        assert_eq!(value["outcome"], "rejected");
        assert_eq!(value["kind"], "generation-mismatch");
        assert_eq!(value["gen"], serde_json::Value::String(fresh));
    }

    #[test]
    fn poison_wakes_the_parked_waiter_with_mismatch_immediately() {
        let service = WorldService::new();
        let (generation, _) = discovered_generation(&service);
        let rx = park_wait(&service, "adapter-1", &generation, "wait-parked");
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = service.backend.lock().unwrap();
            panic!("intentional poison for waiter displacement coverage");
        }));
        assert!(poisoned.is_err());
        // The next backend op replaces the poisoned backend and displaces
        // the waiter against the fresh generation first.
        let replaced: serde_json::Value = serde_json::from_str(
            &service
                .discover(&handshake_request("adapter-1", "req-9"))
                .expect("handshake routes"),
        )
        .expect("valid JSON");
        let fresh = replaced["gen"].as_str().expect("gen").to_owned();
        assert_ne!(fresh, generation);
        let value = recv_immediate(&rx);
        assert_eq!(value["outcome"], "rejected");
        assert_eq!(value["kind"], "generation-mismatch");
        assert_eq!(value["gen"], serde_json::Value::String(fresh));
        assert!(!service.backend.is_poisoned());
    }

    #[test]
    fn concurrent_wait_clobber_settles_without_lost_work() {
        let service = WorldService::new();
        let (generation, _) = discovered_generation(&service);
        let rx_old = park_wait(&service, "adapter-1", &generation, "wait-old");
        // A newer registration supersedes the parked call: the old waiter
        // answers superseded while the new one parks.
        let fresh = service.clone();
        let fresh_generation = generation.clone();
        let newer = std::thread::spawn(move || {
            async_io::block_on(fresh.wait_for_wake(
                "adapter-1".to_owned(),
                fresh_generation,
                "wait-new".to_owned(),
                0,
            ))
        });
        let old: serde_json::Value = serde_json::from_str(
            &rx_old
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("superseded promptly"),
        )
        .expect("valid JSON");
        assert_eq!(old["outcome"], "superseded");
        // Queued work still wakes the live waiter: no lost work.
        queue_select_numbered(&service, &generation, "1");
        let new: serde_json::Value =
            serde_json::from_str(&newer.join().expect("live waiter resolves")).expect("valid JSON");
        assert_eq!(new["outcome"], "wake");
        assert_eq!(new["cursor"], "1");
    }

    #[test]
    fn dropped_waiter_clears_only_its_own_slot() {
        use std::task::{Context, Waker};
        let service = WorldService::new();
        let (generation, _) = discovered_generation(&service);
        // A live waiter owns the slot first.
        let rx = park_wait(&service, "adapter-1", &generation, "wait-live");
        // A second future parks and is dropped mid-park: it supersedes the
        // live waiter (which answers superseded), and its Drop clears only
        // its own nonce, so the slot stays reusable and bounded.
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut ghost = service.wait_with_lease(
            "adapter-1".to_owned(),
            generation.clone(),
            "wait-ghost".to_owned(),
            0,
            60_000,
        );
        assert!(std::pin::Pin::new(&mut ghost).poll(&mut cx).is_pending());
        drop(ghost);
        queue_select_numbered(&service, &generation, "1");
        // The live waiter answers superseded (displaced by the ghost), and
        // the ghost's Drop left no wedge behind: a fresh waiter wakes.
        let old: serde_json::Value = serde_json::from_str(
            &rx.recv_timeout(std::time::Duration::from_secs(5))
                .expect("superseded promptly"),
        )
        .expect("valid JSON");
        assert_eq!(old["outcome"], "superseded");
        // The ghost's Drop left no wedge behind: a fresh waiter wakes
        // immediately on the still-pending work (no lost work).
        let rx_new = park_wait(&service, "adapter-1", &generation, "wait-new");
        let new: serde_json::Value = serde_json::from_str(
            &rx_new
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("fresh waiter wakes"),
        )
        .expect("valid JSON");
        assert_eq!(new["outcome"], "wake");
        assert_eq!(new["cursor"], "1");
    }

    // Fullscreen suspension through the actual capture route: genuine v4
    // facts (fullscreen truthfully carried) into the core-published
    // verdict and the preview-cache replacement, plus queued
    // command-produced take outcomes. No hand-built core replies: every
    // verdict and preview below comes from WorldService capture dispatch,
    // command drain, take settlement, and presentation reads.
    const WIN_A: &str = "0aa8f1c2-3b4d-4e5f-8a9b-0c1d2e3f4a5b";
    const WIN_B: &str = "1bb8f1c2-3b4d-4e5f-8a9b-0c1d2e3f4a5c";
    const WIN_C: &str = "2cc8f1c2-3b4d-4e5f-8a9b-0c1d2e3f4a5d";
    const WIN_D: &str = "3dd8f1c2-3b4d-4e5f-8a9b-0c1d2e3f4a5e";

    // Genuine v4 window facts: every required key present with hold-healthy
    // values (known class/type/policy/layer so the core admits the row).
    // The retired v3 raw KWin classification has no v4 translation: rows
    // arrive already projected, never normalized here.
    fn v4_win(win: &str, out: &str, x: i32, fullscreen: bool) -> serde_json::Value {
        serde_json::json!({
            "win": win, "out": out, "rect": [x, 0, 40, 90],
            "fullscreen": fullscreen, "maximized": false, "sticky": false,
            "minimized": false,
            "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null},
            "hostHidden": false, "privileged": false, "managed": true,
            "acceptsInput": true, "activationInhibited": false,
            "toolWindow": false, "owned": false, "transient": false,
            "modal": false, "resizable": true, "above": false, "below": false,
            "owner": null, "parent": null, "role": null, "appId": null,
            "executable": null, "processId": "pid-foreign-1", "class": "Synthetic",
            "title": null, "type": "normal", "activationPolicy": "regular",
            "layer": 0, "fullscreenOwner": null, "fullscreenExitable": null,
            "suppressionAvailable": null,
            "visibility": {"visible": true, "suppressionOwned": null},
            "lastWrite": null,
        })
    }

    fn suspension_inner(
        fullscreen_a: bool,
        focused: &str,
        gesture: Option<(&str, &str, &str)>,
    ) -> String {
        let fullscreen = if fullscreen_a { "true" } else { "false" };
        let frame = if focused.is_empty() {
            "null".to_owned()
        } else {
            "[0,0,40,90]".to_owned()
        };
        let foreground_out = if focused.is_empty() {
            "null".to_owned()
        } else {
            "\"out-a\"".to_owned()
        };
        let (gesture_lane, native_gesture, cursor_window, cursor) = match gesture {
            Some((serial, rev, phase)) => (
                "true",
                format!(
                    "{{\"v\":1,\"instance\":\"effect-1\",\"serial\":\"{serial}\",\
                      \"revision\":\"{rev}\",\"phase\":\"{phase}\",\"win\":\"{WIN_A}\",\"kind\":\"move\",\
                      \"startRect\":[0,0,40,90],\"startCursor\":null,\"cursor\":[150,50],\
                      \"modalMove\":1,\"changed\":1,\"superseded\":0,\"available\":1}}"
                ),
                format!("\"{WIN_A}\""),
                "[150,50]".to_owned(),
            ),
            None => (
                "false",
                "null".to_owned(),
                "null".to_owned(),
                "null".to_owned(),
            ),
        };
        format!(
            "{{\"v\":4,\
              \"screens\":[{{\"out\":\"out-a\",\"full\":[0,0,800,600],\"work\":[0,0,800,560],\
              \"complete\":true,\"scale\":1,\
              \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}},\
              {{\"out\":\"out-b\",\"full\":[800,0,800,600],\"work\":[800,0,800,560],\
              \"complete\":true,\"scale\":1,\
              \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
              \"windows\":[{win_a},{win_b},{win_c},{win_d}],\
              \"closed\":[],\"native\":[],\
              \"focused\":\"{focused}\",\"complete\":true,\"gesture\":{gesture_lane},\
              \"cursor\":{cursor},\"actionForeground\":null,\"actionCached\":null,\
              \"actionOutputs\":[],\
              \"presentation\":{{\"foreground\":\"{focused}\",\"minimized\":false,\"maximized\":false,\
              \"fullscreen\":{fullscreen},\"hostHidden\":false,\"hiddenWorkspace\":false,\
              \"frame\":{frame},\"chordHeld\":true,\"moveWindow\":\"\",\
              \"pagerOrder\":[\"{WIN_A}\",\"{WIN_B}\",\"{WIN_C}\",\"{WIN_D}\"],\
              \"pagerFrames\":[{{\"win\":\"{WIN_A}\",\"live\":[0,0,40,90]}},\
              {{\"win\":\"{WIN_B}\",\"live\":[100,0,40,90]}},\
              {{\"win\":\"{WIN_C}\",\"live\":[800,0,40,90]}},\
              {{\"win\":\"{WIN_D}\",\"live\":[900,0,40,90]}}],\
              \"witness\":null,\"foregroundOut\":{foreground_out}}},\
              \"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[],\
              \"rendererInput\":null,\"gestureIngress\":null,\
              \"nativeGesture\":{native_gesture},\
              \"cursorWindow\":{cursor_window},\"nativeInvalidWitness\":null}}",
            win_a = v4_win(WIN_A, "out-a", 0, fullscreen_a),
            win_b = v4_win(WIN_B, "out-a", 100, false),
            win_c = v4_win(WIN_C, "out-b", 800, false),
            win_d = v4_win(WIN_D, "out-b", 900, false),
        )
    }

    fn suspension_capture(
        service: &WorldService,
        generation: &str,
        source: &str,
        seq: u64,
        fullscreen_a: bool,
        focused: &str,
        gesture: Option<(&str, &str, &str)>,
    ) -> serde_json::Value {
        let envelope = format!(
            "{{\"v\":4,\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
              \"source\":\"{source}\",\"sourceSeq\":\"{seq}\",\"nativeInvalid\":\"{seq}\",\
              \"report\":{}}}",
            suspension_inner(fullscreen_a, focused, gesture),
        );
        let reply = service
            .evaluate_capture(&envelope)
            .expect("in-band capture");
        serde_json::from_str(&reply).expect("valid JSON")
    }

    fn suspension_read(service: &WorldService, req: &str) -> serde_json::Value {
        let body = service
            .read_presentation(&format!("{{\"v\":4,\"req\":\"{req}\"}}"))
            .expect("read");
        serde_json::from_str(&body).expect("valid JSON")
    }

    // Reports wait for initial settings: the handshake binds the lifetime,
    // and settings (sent through discover like production) unlock report
    // readiness. Settings never reset the lifetime.
    fn suspension_settings(service: &WorldService, generation: &str) -> serde_json::Value {
        let body = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
              \"req\":\"settings-1\",\"rev\":\"1\",\"values\":{{}}}}"
        );
        let reply = service.discover(&body).expect("settings routes");
        serde_json::from_str(&reply).expect("valid JSON")
    }

    #[test]
    fn focused_fullscreen_suspends_cached_preview_through_capture_route() {
        // End-to-end through the real capture route (genuine v4 facts,
        // gesture lane, presentation reads): a proven live preview caches,
        // suspends to null on focused fullscreen, and restores on exit.
        // Cross-output resolution gating rides the broker unit
        // (`suspended_presentation_omits_only_the_matching_domain_preview`):
        // the engine focus-gates drags to the focused tiled window, so no
        // live gesture can produce an out-of-suspension preview while the
        // verdict holds (pre-existing behavior, independent of suspension).
        // The v4 queue holds no drag-preview vocabulary (the `operation`
        // wrapper refuses as an unknown value), so live gestures are the
        // only preview source here; queue/take settlement rides the
        // dedicated queue oracle tests.
        let service = WorldService::new();
        let mut handshake: serde_json::Value =
            serde_json::from_str(&handshake_request("adapter-1", "req-0")).expect("handshake JSON");
        for capability in ["maximize", "fullscreen", "nativeSticky"] {
            handshake["capabilities"][capability] = serde_json::Value::Bool(true);
        }
        let bound: serde_json::Value = serde_json::from_str(
            &service
                .discover(&handshake.to_string())
                .expect("handshake routes"),
        )
        .expect("handshake reply");
        let generation = bound["gen"].as_str().expect("generation").to_owned();
        let settings = suspension_settings(&service, &generation);
        assert_eq!(settings["status"], "accepted");
        assert_eq!(settings["adapter"], "adapter-1");
        // Admission baseline first (mirrors the core rig): the open
        // gesture binds implicitly only against retained homing, and the
        // managed focus latches behind the focused newcomer.
        let admitted = suspension_capture(&service, &generation, "cap-0", 1, false, "", None);
        assert_eq!(
            admitted["ready"], true,
            "admission capture accepted: {admitted}"
        );
        // Baseline: an open live move gesture publishes a proven preview
        // through the real v4 capture route (genuine fullscreen facts,
        // real gesture lane, real command inputs throughout). The gesture
        // opens with started, then updates: the update publishes the
        // proven preview.
        let opened = suspension_capture(
            &service,
            &generation,
            "cap-1",
            2,
            false,
            WIN_A,
            Some(("1", "1", "started")),
        );
        assert_eq!(opened["ready"], true, "gesture open accepted: {opened}");
        let first = suspension_capture(
            &service,
            &generation,
            "cap-2",
            3,
            false,
            WIN_A,
            Some(("1", "2", "updated")),
        );
        assert_eq!(first["ready"], true, "baseline capture accepted: {first}");
        let proj = suspension_read(&service, "read-1");
        assert_eq!(proj["outcome"], "presentation");
        assert!(
            proj["preview"].is_object(),
            "proven preview cached, got {}",
            proj["preview"]
        );
        assert_eq!(proj["preview"]["producerInstance"], "effect-1");
        assert_eq!(proj["preview"]["producerSerial"], "1");
        assert!(
            proj["presentation"]["suspendedOutput"].is_null(),
            "no verdict without fullscreen"
        );
        // Focused fullscreen on the same output: the verdict publishes and
        // the cached preview is replaced with null (no stale retention).
        let suspended = suspension_capture(&service, &generation, "cap-3", 4, true, WIN_A, None);
        assert_eq!(suspended["ready"], true, "fullscreen capture accepted");
        let proj = suspension_read(&service, "read-3");
        assert!(proj["preview"].is_null(), "suspended output omits preview");
        assert_eq!(proj["presentation"]["suspendedOutput"], "out-a");
        // The suspension holds across captures: a further suspended
        // capture keeps the verdict and the null preview.
        let still = suspension_capture(&service, &generation, "cap-4", 5, true, WIN_A, None);
        assert_eq!(still["ready"], true, "suspended capture accepted");
        let proj = suspension_read(&service, "read-4");
        assert!(
            proj["preview"].is_null(),
            "suspension holds across captures"
        );
        assert_eq!(proj["presentation"]["suspendedOutput"], "out-a");
        // Fullscreen exit: the verdict clears and the live gesture preview
        // restores, proven again.
        let exited = suspension_capture(
            &service,
            &generation,
            "cap-5",
            6,
            false,
            WIN_A,
            Some(("2", "1", "updated")),
        );
        assert_eq!(exited["ready"], true, "exit capture accepted");
        let proj = suspension_read(&service, "read-5");
        assert!(
            proj["preview"].is_object(),
            "exit restores the live preview, got {}",
            proj["preview"]
        );
        assert_eq!(proj["preview"]["producerInstance"], "effect-1");
        assert_eq!(
            proj["preview"]["producerSerial"], "2",
            "fresh gesture serial proves the new binding"
        );
        assert!(
            proj["presentation"]["suspendedOutput"].is_null(),
            "verdict clears on exit"
        );
    }

    #[test]
    fn maximum_v4_settings_snapshot_fits_discovery_request_bound() {
        // The v4 settings decoder and the DiscoverWorld request bound share
        // the 4 MiB budget: the largest decoder-accepted settings message
        // must still pass the route length gate, and the live discover
        // route must accept exactly that snapshot while the next byte
        // fails closed (no widening).
        let prefix = "{\"type\":\"settings\",\"v\":4,\"adapter\":\"a-1\",\"gen\":\"g-1\",\
            \"req\":\"r-1\",\"rev\":\"0\",\"values\":{\"pad\":\"";
        let suffix = "\"}}";
        let pad_len = WORLD_SERVICE_MAX_REQUEST - prefix.len() - suffix.len();
        let wire = format!("{prefix}{}{suffix}", "x".repeat(pad_len));
        assert_eq!(wire.len(), WORLD_SERVICE_MAX_REQUEST);
        assert!(
            tiler_protocol::connection_protocol::decode_settings(&wire).is_ok(),
            "max-size settings must decode"
        );
        assert!(
            !(wire.len() > WORLD_SERVICE_MAX_REQUEST),
            "max-size settings must pass the request length gate"
        );
        let oversized = format!("{wire}x");
        assert_eq!(oversized.len(), WORLD_SERVICE_MAX_REQUEST + 1);
        assert_eq!(
            tiler_protocol::connection_protocol::decode_settings(&oversized),
            Err(tiler_protocol::connection_protocol::CodecError("oversized"))
        );
        assert!(
            oversized.len() > WORLD_SERVICE_MAX_REQUEST,
            "bound+1 must trip the request length gate"
        );
        // The same bound holds on the live discover route: handshake first
        // (settings bind the serving generation and producer lifetime), then
        // the largest fitting snapshot routes to an accepted settings reply.
        let service = WorldService::new();
        let discovery: serde_json::Value = serde_json::from_str(
            &service
                .discover(&handshake_request("a-1", "req-0"))
                .expect("handshake routes"),
        )
        .expect("valid JSON");
        let generation = discovery["gen"].as_str().expect("gen").to_owned();
        let route_prefix = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"a-1\",\"gen\":\"{generation}\",\
              \"req\":\"r-1\",\"rev\":\"1\",\"values\":{{\"pad\":\""
        );
        let route_pad = WORLD_SERVICE_MAX_REQUEST - route_prefix.len() - suffix.len();
        let route_wire = format!("{route_prefix}{}{suffix}", "x".repeat(route_pad));
        assert_eq!(route_wire.len(), WORLD_SERVICE_MAX_REQUEST);
        let accepted = service
            .discover(&route_wire)
            .expect("max-size settings route");
        let accepted_value: serde_json::Value =
            serde_json::from_str(&accepted).expect("valid JSON");
        assert_eq!(accepted_value["adapter"], "a-1");
        assert_eq!(accepted_value["req"], "r-1");
        assert_eq!(accepted_value["status"], "accepted");
        // Bound+1 fails closed before routing: no reply, no state touched.
        let over = format!("{route_wire}x");
        assert_eq!(over.len(), WORLD_SERVICE_MAX_REQUEST + 1);
        match service.discover(&over) {
            Err(WorldServiceError::Oversized) => {}
            other => panic!("expected oversized, got {other:?}"),
        }
    }
}
