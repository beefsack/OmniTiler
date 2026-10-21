//! Planner service boundary: retained world/intent D-Bus service.
//!
//! Contract identity: service `com.omnitiler.Planner`, object
//! `/com/omnitiler/Planner`, interface `com.omnitiler.Planner1`,
//! methods `DiscoverWorld`, `DescribeWorld`, `ReadFloatIntent`,
//! `WriteFloatIntent`. The world route carries the retained world
//! backend; the intent routes carry the session-scoped float-intent
//! membership store.
//!
//! Boundary rules: no Rust-to-KWin calls except `org.freedesktop.DBus`
//! identity/owner queries (`GetConnectionUnixUser` for same-UID caller
//! verification, `GetId` plus `GetNameOwner(org.kde.KWin)` for float-intent
//! namespace derivation and current-KWin-owner authorization), no other
//! persistence beyond the session-scoped float-intent membership store, no
//! tray coupling, no autostart, no native mutation. Bounded single-flight
//! endpoint handling via independent world and intent non-queuing async
//! locks. Name acquisition uses `DoNotQueue`; name loss is terminal. World
//! and intent calls authorize fail-closed: the caller unique name's Unix UID
//! must equal the Planner geteuid, and world/intent calls additionally
//! require the current KWin unique owner as caller.
//!
//! Rust returns domain rejections in-band; the KWin adapter journals the
//! bounded command and rejection lines after it receives each reply.

use std::io::Write;
use std::sync::Arc;

use zbus::blocking::{Connection, MessageIterator};
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::message::Type;
use zbus::{MatchRule, fdo::NameOwnerChanged};

use crate::float_intent_store::{
    FloatIntentStore, INTENT_MAX_REPLY_BYTES, INTENT_MAX_REQUEST_BYTES, IntentNamespace,
};
use crate::world_service::{
    WORLD_SERVICE_MAX_REPLY, WORLD_SERVICE_MAX_REQUEST, WorldService, parse_discovery_request,
    world_egress_summary, world_ingress_summary,
};

pub const SERVICE: &str = "com.omnitiler.Planner";
pub const OBJECT: &str = "/com/omnitiler/Planner";
pub const INTERFACE: &str = "com.omnitiler.Planner1";
/// Well-known KWin service name for intent caller authorization and
/// namespace derivation. Never accepted from the caller.
pub const KWIN_SERVICE: &str = "org.kde.KWin";

/// Pure same-UID decision. Accepts iff `caller_uid` is present and equals
/// `expected_uid`; missing or differing UIDs reject fail-closed.
#[must_use]
pub fn caller_uid_authorized(caller_uid: Option<u32>, expected_uid: u32) -> bool {
    matches!(caller_uid, Some(uid) if uid == expected_uid)
}

// No nested-KWin manifest binding, nested mode, manifest parsing,
// forensics, or ambient environment authority in this service.

#[derive(Clone, Copy)]
enum PlanEarlyExit {
    Busy,
    Closed,
    Oversize,
    Unauthorized,
}

fn plan_early_exit_summary(exit: PlanEarlyExit) -> &'static str {
    match exit {
        PlanEarlyExit::Busy => {
            "omnitiler:plan-summary direction=egress op=unknown correlation=- outcome=unavailable kind=busy base_revision=- detail=early-exit"
        }
        PlanEarlyExit::Closed => {
            "omnitiler:plan-summary direction=egress op=unknown correlation=- outcome=unavailable kind=connection-closed base_revision=- detail=early-exit"
        }
        PlanEarlyExit::Oversize => {
            "omnitiler:plan-summary direction=egress op=unknown correlation=- outcome=unavailable kind=request-oversize base_revision=- detail=early-exit"
        }
        PlanEarlyExit::Unauthorized => {
            "omnitiler:plan-summary direction=egress op=unknown correlation=- outcome=rejected kind=unauthorized base_revision=- detail=early-exit"
        }
    }
}

fn emit_plan_early_exit(exit: PlanEarlyExit) {
    eprintln!("{}", plan_early_exit_summary(exit));
}

#[derive(Debug, zbus::DBusError, PartialEq, Eq)]
#[zbus(prefix = "com.omnitiler.Planner1")]
pub enum PlannerError {
    Unauthorized,
    Unavailable(String),
}

#[derive(Clone, Debug)]
pub struct PlannerEndpoint {
    /// World boundary: retained [`WorldService`] backend (fresh core
    /// generation at construction, poison replacement under a new one).
    /// Cloned endpoints share one backend.
    /// Single-flight admission uses the service's own non-queuing lock.
    world: WorldService,
    intent_store: Option<FloatIntentStore>,
    /// Separate intent single-flight: intent I/O (including bus credential
    /// and namespace queries) never holds the world lock, so an overlapping
    /// settled-intent call cannot make `DescribeWorld` go busy. Concurrent
    /// intent calls fail fast as busy on this mutex instead of queueing.
    intent_lock: Arc<async_lock::Mutex<()>>,
}

impl PlannerEndpoint {
    #[must_use]
    pub fn new() -> Self {
        Self {
            world: WorldService::new(),
            intent_store: FloatIntentStore::from_runtime_dir(),
            intent_lock: Arc::new(async_lock::Mutex::new(())),
        }
    }

    /// Test seam: endpoint with the intent store rooted at a private
    /// directory. Production always uses [`Self::new`] (runtime dir).
    #[must_use]
    pub fn with_intent_root(root: std::path::PathBuf) -> Self {
        Self {
            world: WorldService::new(),
            intent_store: Some(FloatIntentStore::with_root(root)),
            intent_lock: Arc::new(async_lock::Mutex::new(())),
        }
    }

    /// Test seam: endpoint without an intent store (missing runtime dir).
    /// Intent calls degrade or report unavailable without touching disk.
    #[must_use]
    pub fn without_intent_store() -> Self {
        Self {
            world: WorldService::new(),
            intent_store: None,
            intent_lock: Arc::new(async_lock::Mutex::new(())),
        }
    }
}

impl Default for PlannerEndpoint {
    fn default() -> Self {
        Self::new()
    }
}

/// Fail-closed same-UID caller verification. Converts `caller` to a unique D-Bus name, queries
/// `org.freedesktop.DBus.GetConnectionUnixUser` for the caller UID through
/// the existing `DBusProxy`, and accepts iff that UID equals the Planner
/// geteuid. Any name conversion, proxy, UID lookup, or mismatch failure
/// rejects.
async fn verify_same_uid_caller(connection: &zbus::Connection, caller: &str) -> bool {
    let Ok(unique_name) = zbus::names::UniqueName::try_from(caller) else {
        return false;
    };
    let Ok(dbus) = zbus::fdo::DBusProxy::new(connection).await else {
        return false;
    };
    let Ok(caller_uid) = dbus.get_connection_unix_user(unique_name.into()).await else {
        return false;
    };
    caller_uid_authorized(Some(caller_uid), rustix::process::geteuid().as_raw())
}

async fn verify_planner_caller(connection: &zbus::Connection, caller: &str) -> bool {
    verify_same_uid_caller(connection, caller).await
}

/// Derive the float-intent namespace from the serving bus: the session bus
/// id plus the current `org.kde.KWin` unique owner. `None` when either query
/// fails or the bus values fail validation (missing KWin owner included).
/// Never logs: callers carry only fixed reasons, never the queried values.
async fn resolve_intent_namespace(connection: &zbus::Connection) -> Option<IntentNamespace> {
    let dbus = zbus::fdo::DBusProxy::new(connection).await.ok()?;
    let bus_id = dbus.get_id().await.ok()?.to_string();
    let service: zbus::names::BusName<'_> = KWIN_SERVICE.try_into().ok()?;
    let owner = dbus.get_name_owner(service).await.ok()?.to_string();
    IntentNamespace::parse(&bus_id, &owner)
}

/// Shared admission gate for the intent methods: bounded non-queuing
/// single-flight on the intent lock (never the plan lock, so a settled-intent
/// call awaiting credential/namespace queries cannot make the regular plan
/// route go busy) with same-UID verification and connection-loss checks.
/// Returns the held guard plus the verified sender on admission, an in-band
/// rejected reply for authorization failures, or a terminal D-Bus error for
/// busy/closed/oversize. Early exits reuse the fixed uncorrelated summaries.
enum IntentAdmission<'a> {
    Admitted {
        _guard: async_lock::MutexGuard<'a, ()>,
        sender: String,
    },
    Reject(String),
    Fail(PlannerError),
}

async fn admit_intent_call<'a>(
    endpoint: &'a PlannerEndpoint,
    request_len: usize,
    caller: Option<&str>,
    connection: &zbus::Connection,
) -> IntentAdmission<'a> {
    use crate::float_intent_store::{emit_intent_diag, intent_rejection};
    if request_len > INTENT_MAX_REQUEST_BYTES {
        let Some(_guard) = endpoint.intent_lock.try_lock() else {
            emit_intent_diag(plan_early_exit_summary(PlanEarlyExit::Busy));
            return IntentAdmission::Fail(PlannerError::Unavailable("planner is busy".to_owned()));
        };
        drop(_guard);
        emit_intent_diag(plan_early_exit_summary(PlanEarlyExit::Oversize));
        return IntentAdmission::Fail(PlannerError::Unavailable(
            "request exceeds size bound".to_owned(),
        ));
    }
    let Some(_guard) = endpoint.intent_lock.try_lock() else {
        emit_intent_diag(plan_early_exit_summary(PlanEarlyExit::Busy));
        return IntentAdmission::Fail(PlannerError::Unavailable("planner is busy".to_owned()));
    };
    if connection.is_closed() {
        drop(_guard);
        emit_intent_diag(plan_early_exit_summary(PlanEarlyExit::Closed));
        return IntentAdmission::Fail(PlannerError::Unavailable(
            "planner serving connection was lost".to_owned(),
        ));
    }
    let Some(caller) = caller else {
        drop(_guard);
        emit_intent_diag(plan_early_exit_summary(PlanEarlyExit::Unauthorized));
        return IntentAdmission::Reject(intent_rejection("-", "unauthorized"));
    };
    if !verify_planner_caller(connection, caller).await {
        drop(_guard);
        emit_intent_diag(plan_early_exit_summary(PlanEarlyExit::Unauthorized));
        return IntentAdmission::Reject(intent_rejection("-", "unauthorized"));
    }
    IntentAdmission::Admitted {
        _guard,
        sender: caller.to_owned(),
    }
}

/// Shared namespace-plus-sender authorization for both intent methods:
/// derives the namespace from the serving bus and checks the verified sender
/// against its live KWin owner. `Err` is the fixed in-band reason.
async fn authorize_intent_sender(
    connection: &zbus::Connection,
    sender: &str,
) -> Result<IntentNamespace, &'static str> {
    let Some(namespace) = resolve_intent_namespace(connection).await else {
        return Err("namespace-unavailable");
    };
    if !namespace.authorizes_sender(sender) {
        return Err("not-kwin-owner");
    }
    Ok(namespace)
}

#[zbus::interface(name = "com.omnitiler.Planner1")]
impl PlannerEndpoint {
    /// World discovery/bootstrap route. Authenticated same-UID caller that
    /// must also be the current KWin unique owner (single active adapter,
    /// resolved live per call through the intent namespace seam, never from
    /// the caller); alternating same-UID non-KWin senders are rejected
    /// before touching backend state, so they can neither read generations
    /// nor reset ordering. Returns the serving generation plus capabilities
    /// with `ready=false` and no applicable desired state. Malformed
    /// discovery requests and authorization failures return bounded in-band
    /// errors bound to the serving generation; only oversize/closed surface
    /// as D-Bus errors. Lock-free: the backend mutex serializes against
    /// world flights, and poison replacement mints a fresh generation
    /// rather than serving partial state. KWin restart surfaces as a new
    /// owner that re-discovers (same generation, fresh observation);
    /// service restart surfaces as a new generation that rejects stale
    /// reports until re-discovery.
    #[zbus(name = "DiscoverWorld")]
    async fn discover_world(
        &self,
        request: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> Result<String, PlannerError> {
        if request.len() > WORLD_SERVICE_MAX_REQUEST {
            emit_plan_early_exit(PlanEarlyExit::Oversize);
            return Err(PlannerError::Unavailable(
                "request exceeds size bound".to_owned(),
            ));
        }
        if emitter.connection().is_closed() {
            emit_plan_early_exit(PlanEarlyExit::Closed);
            return Err(PlannerError::Unavailable(
                "planner serving connection was lost".to_owned(),
            ));
        }
        let caller = header.sender().map(ToString::to_string);
        let Some(caller) = caller.as_deref() else {
            emit_plan_early_exit(PlanEarlyExit::Unauthorized);
            return Ok(self.world.error_body("unauthorized", "unauthorized"));
        };
        if !verify_planner_caller(emitter.connection(), caller).await {
            emit_plan_early_exit(PlanEarlyExit::Unauthorized);
            return Ok(self.world.error_body("unauthorized", "unauthorized"));
        }
        if let Err(reason) = authorize_intent_sender(emitter.connection(), caller).await {
            emit_plan_early_exit(PlanEarlyExit::Unauthorized);
            return Ok(self.world.error_body(reason, reason));
        }
        let reply = match parse_discovery_request(&request) {
            Ok((adapter, req)) => self.world.discovery_body(&adapter, &req),
            Err((kind, message)) => self.world.error_body(kind, message),
        };
        if reply.len() > WORLD_SERVICE_MAX_REPLY {
            return Err(PlannerError::Unavailable(
                "reply exceeds size bound".to_owned(),
            ));
        }
        eprintln!("{}", world_ingress_summary(&request));
        eprintln!("{}", world_egress_summary(&request, &reply));
        Ok(reply)
    }

    /// World planning route. Bounded non-queuing single-flight with
    /// same-UID verification plus the current-KWin-owner check (single
    /// active adapter; see [`PlannerEndpoint::discover_world`]),
    /// connection-loss, and reply-size checks. A delayed reply or request
    /// from a superseded generation or a non-owner sender can never reset
    /// ordering: generation binding rejects before evaluation and the owner
    /// check rejects before any state access. Authorized requests evaluate
    /// against the retained world backend; accepted/duplicate/consumed
    /// outcomes return the ready full-state reply, core rejections return
    /// bounded in-band errors, and only oversize replies fail closed as
    /// `Unavailable`. One bounded ingress plus one bounded egress summary
    /// line each; diagnostics never echo payload bytes.
    #[zbus(name = "DescribeWorld")]
    async fn describe_world(
        &self,
        request: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> Result<String, PlannerError> {
        use crate::world_service::WorldServiceError;
        if request.len() > WORLD_SERVICE_MAX_REQUEST {
            let Some(_guard) = self.world.try_acquire_guard() else {
                emit_plan_early_exit(PlanEarlyExit::Busy);
                return Err(PlannerError::Unavailable("planner is busy".to_owned()));
            };
            drop(_guard);
            emit_plan_early_exit(PlanEarlyExit::Oversize);
            return Err(PlannerError::Unavailable(
                "request exceeds size bound".to_owned(),
            ));
        }
        let Some(_guard) = self.world.try_acquire_guard() else {
            emit_plan_early_exit(PlanEarlyExit::Busy);
            return Err(PlannerError::Unavailable("planner is busy".to_owned()));
        };
        if emitter.connection().is_closed() {
            drop(_guard);
            emit_plan_early_exit(PlanEarlyExit::Closed);
            return Err(PlannerError::Unavailable(
                "planner serving connection was lost".to_owned(),
            ));
        }
        let caller = header.sender().map(ToString::to_string);
        let Some(caller) = caller.as_deref() else {
            drop(_guard);
            emit_plan_early_exit(PlanEarlyExit::Unauthorized);
            return Ok(self.world.error_body("unauthorized", "unauthorized"));
        };
        if !verify_planner_caller(emitter.connection(), caller).await {
            drop(_guard);
            emit_plan_early_exit(PlanEarlyExit::Unauthorized);
            return Ok(self.world.error_body("unauthorized", "unauthorized"));
        }
        if let Err(reason) = authorize_intent_sender(emitter.connection(), caller).await {
            drop(_guard);
            emit_plan_early_exit(PlanEarlyExit::Unauthorized);
            return Ok(self.world.error_body(reason, reason));
        }
        let reply = match self.world.evaluate_world_request(&request) {
            Ok(reply) => reply,
            Err(WorldServiceError::Oversized) => {
                drop(_guard);
                emit_plan_early_exit(PlanEarlyExit::Oversize);
                return Err(PlannerError::Unavailable(
                    "request exceeds size bound".to_owned(),
                ));
            }
            Err(WorldServiceError::Unavailable(message)) => {
                // Evaluation produced an oversize reply: emit a bounded
                // world-vocabulary terminal line (never payload bytes), then
                // fail closed. The in-band body below is log-only and never
                // sent; only the outcome/kind shape reaches the journal.
                drop(_guard);
                eprintln!(
                    "{}",
                    world_egress_summary(
                        &request,
                        &self
                            .world
                            .error_body("reply-oversize", "reply exceeds size bound")
                    )
                );
                return Err(PlannerError::Unavailable(message));
            }
        };
        if reply.len() > WORLD_SERVICE_MAX_REPLY {
            drop(_guard);
            return Err(PlannerError::Unavailable(
                "reply exceeds size bound".to_owned(),
            ));
        }
        drop(_guard);
        eprintln!("{}", world_ingress_summary(&request));
        eprintln!("{}", world_egress_summary(&request, &reply));
        Ok(reply)
    }

    /// Float-intent read route. Authenticated same-UID caller that must also
    /// be the current KWin owner; the namespace is derived from the serving
    /// bus, never from the caller. Returns the stored membership (pruned to
    /// the caller-attested complete live inventory when `live` is present),
    /// a degraded empty reply when the content is missing or unusable, or an
    /// in-band rejection for authorization and request failures. Only an
    /// oversize body or reply fails closed as `Unavailable`.
    #[zbus(name = "ReadFloatIntent")]
    async fn read_float_intent(
        &self,
        request: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> Result<String, PlannerError> {
        use crate::float_intent_store::{
            emit_intent_diag, intent_egress_summary, intent_rejection, parse_read_request,
            read_degraded_reply, read_ok_reply,
        };
        let caller = header.sender().map(ToString::to_string);
        let (guard, sender) =
            match admit_intent_call(self, request.len(), caller.as_deref(), emitter.connection())
                .await
            {
                IntentAdmission::Admitted { _guard, sender } => (_guard, sender),
                IntentAdmission::Reject(reply) => return Ok(reply),
                IntentAdmission::Fail(error) => return Err(error),
            };
        // Emits the terminal summary after the intent lock releases, then
        // returns the in-band reply. Logging never changes the result.
        let finish = |guard: async_lock::MutexGuard<'_, ()>,
                      correlation: &str,
                      outcome: &str,
                      stored: Option<usize>,
                      returned: Option<usize>,
                      tile_stored: Option<usize>,
                      tile_returned: Option<usize>,
                      reason: Option<&str>,
                      reply: String| {
            drop(guard);
            emit_intent_diag(&intent_egress_summary(
                "read",
                correlation,
                outcome,
                stored,
                returned,
                tile_stored,
                tile_returned,
                reason,
            ));
            reply
        };
        let parsed = match parse_read_request(&request) {
            Ok(parsed) => parsed,
            Err(error) => {
                let reply = intent_rejection(&error.correlation, error.reason);
                return Ok(finish(
                    guard,
                    &error.correlation,
                    error.reason,
                    None,
                    None,
                    None,
                    None,
                    Some(error.reason),
                    reply,
                ));
            }
        };
        let namespace = match authorize_intent_sender(emitter.connection(), &sender).await {
            Ok(namespace) => namespace,
            Err("not-kwin-owner") => {
                let reply = intent_rejection(&parsed.correlation, "not-kwin-owner");
                return Ok(finish(
                    guard,
                    &parsed.correlation,
                    "rejected",
                    None,
                    None,
                    None,
                    None,
                    Some("not-kwin-owner"),
                    reply,
                ));
            }
            Err(reason) => {
                let reply = read_degraded_reply(&parsed.correlation, reason);
                return Ok(finish(
                    guard,
                    &parsed.correlation,
                    "degraded",
                    Some(0),
                    Some(0),
                    Some(0),
                    Some(0),
                    Some(reason),
                    reply,
                ));
            }
        };
        let Some(store) = &self.intent_store else {
            let reply = read_degraded_reply(&parsed.correlation, "namespace-unavailable");
            return Ok(finish(
                guard,
                &parsed.correlation,
                "degraded",
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some("namespace-unavailable"),
                reply,
            ));
        };
        let read = match &parsed.live {
            None => store.read_membership(&namespace),
            Some(live) => store.read_pruned(&namespace, live),
        };
        let (outcome, stored, returned, tile_stored, tile_returned, reason, reply) =
            match read.degraded {
                None => (
                    "ok",
                    read.stored,
                    read.members.len(),
                    read.tile_stored,
                    read.tile_members.len(),
                    None,
                    read_ok_reply(
                        &parsed.correlation,
                        &read.members,
                        read.stored,
                        &read.tile_members,
                        read.tile_stored,
                    ),
                ),
                Some(reason) => (
                    "degraded",
                    0,
                    0,
                    0,
                    0,
                    Some(reason),
                    read_degraded_reply(&parsed.correlation, reason),
                ),
            };
        if reply.len() > INTENT_MAX_REPLY_BYTES {
            drop(guard);
            return Err(PlannerError::Unavailable(
                "reply exceeds size bound".to_owned(),
            ));
        }
        Ok(finish(
            guard,
            &parsed.correlation,
            outcome,
            Some(stored),
            Some(returned),
            Some(tile_stored),
            Some(tile_returned),
            reason,
            reply,
        ))
    }

    /// Float-intent write route. Same authentication and namespace rules as
    /// the read route. Persists the full snapshot (empty clears the settled
    /// unfloat) after successful application upstream; a write failure
    /// retains the previous local intent and the next settled update
    /// rewrites. Replies mirror the read route outcomes with `stored` and
    /// `rejected`/`unavailable` failure shapes.
    #[zbus(name = "WriteFloatIntent")]
    async fn write_float_intent(
        &self,
        request: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> Result<String, PlannerError> {
        use crate::float_intent_store::{
            emit_intent_diag, intent_egress_summary, intent_rejection, parse_write_request,
            write_failed_reply, write_stored_reply,
        };
        let caller = header.sender().map(ToString::to_string);
        let (guard, sender) =
            match admit_intent_call(self, request.len(), caller.as_deref(), emitter.connection())
                .await
            {
                IntentAdmission::Admitted { _guard, sender } => (_guard, sender),
                IntentAdmission::Reject(reply) => return Ok(reply),
                IntentAdmission::Fail(error) => return Err(error),
            };
        // Emits the terminal summary after the intent lock releases, then
        // returns the in-band reply. Logging never changes the result.
        let finish = |guard: async_lock::MutexGuard<'_, ()>,
                      correlation: &str,
                      outcome: &str,
                      stored: Option<usize>,
                      tile_stored: Option<usize>,
                      reason: Option<&str>,
                      reply: String| {
            drop(guard);
            emit_intent_diag(&intent_egress_summary(
                "write",
                correlation,
                outcome,
                stored,
                None,
                tile_stored,
                None,
                reason,
            ));
            reply
        };
        let parsed = match parse_write_request(&request) {
            Ok(parsed) => parsed,
            Err(error) => {
                let reply = if error.reason == "invalid-members" {
                    write_failed_reply(&error.correlation, "rejected", error.reason)
                } else {
                    intent_rejection(&error.correlation, error.reason)
                };
                return Ok(finish(
                    guard,
                    &error.correlation,
                    "rejected",
                    None,
                    None,
                    Some(error.reason),
                    reply,
                ));
            }
        };
        let namespace = match authorize_intent_sender(emitter.connection(), &sender).await {
            Ok(namespace) => namespace,
            Err("not-kwin-owner") => {
                let reply = intent_rejection(&parsed.correlation, "not-kwin-owner");
                return Ok(finish(
                    guard,
                    &parsed.correlation,
                    "rejected",
                    None,
                    None,
                    Some("not-kwin-owner"),
                    reply,
                ));
            }
            Err(reason) => {
                let reply = write_failed_reply(&parsed.correlation, "unavailable", reason);
                return Ok(finish(
                    guard,
                    &parsed.correlation,
                    "unavailable",
                    None,
                    None,
                    Some(reason),
                    reply,
                ));
            }
        };
        let (outcome, stored, tile_stored, reason, reply) = match &self.intent_store {
            None => (
                "unavailable",
                None,
                None,
                Some("namespace-unavailable"),
                write_failed_reply(&parsed.correlation, "unavailable", "namespace-unavailable"),
            ),
            Some(store) => {
                // Absent tile means an empty full tile snapshot; present tile
                // (even empty) replaces atomically with float. Full snapshots
                // remain disjoint.
                let tile_members: Vec<String> = parsed.tile.clone().unwrap_or_default();
                match store.write_both(&namespace, &parsed.members, &tile_members) {
                    Ok((stored, stored_tile)) => (
                        "stored",
                        Some(stored),
                        Some(stored_tile),
                        None,
                        write_stored_reply(&parsed.correlation, stored, stored_tile),
                    ),
                    Err(reason) => {
                        let outcome = if reason == "invalid-members" {
                            "rejected"
                        } else {
                            "unavailable"
                        };
                        (
                            outcome,
                            None,
                            None,
                            Some(reason),
                            write_failed_reply(&parsed.correlation, outcome, reason),
                        )
                    }
                }
            }
        };
        if reply.len() > INTENT_MAX_REPLY_BYTES {
            drop(guard);
            return Err(PlannerError::Unavailable(
                "reply exceeds size bound".to_owned(),
            ));
        }
        Ok(finish(
            guard,
            &parsed.correlation,
            outcome,
            stored,
            tile_stored,
            reason,
            reply,
        ))
    }
}

fn serving_connection_lost_error() -> zbus::Error {
    zbus::Error::Failure("planner serving connection was lost".to_owned())
}

fn owner_signal_malformed_line() -> &'static str {
    "omnitiler:route-diag component=planner stage=owner event=signal outcome=malformed-signal"
}

fn owner_signal_args_invalid_line() -> &'static str {
    "omnitiler:route-diag component=planner stage=owner event=signal outcome=invalid-args"
}

#[derive(Debug, PartialEq, Eq)]
enum OwnerSignalDecision {
    SkippedMalformed,
    SkippedInvalidArgs,
    Handled,
}

fn decide_owner_signal(
    message: zbus::message::Message,
    registered_unique: &mut Option<String>,
    our_unique: Option<&str>,
) -> zbus::Result<OwnerSignalDecision> {
    let Some(signal) = NameOwnerChanged::from_message(message) else {
        return Ok(OwnerSignalDecision::SkippedMalformed);
    };
    let args = match signal.args() {
        Ok(args) => args,
        Err(_) => return Ok(OwnerSignalDecision::SkippedInvalidArgs),
    };
    let name = args.name().to_string();
    let new_owner = args.new_owner().as_ref().map(ToString::to_string);
    handle_name_owner_changed(registered_unique, &name, new_owner, our_unique)?;
    Ok(OwnerSignalDecision::Handled)
}

fn owner_monitor_ended_error() -> zbus::Error {
    zbus::Error::Failure("planner owner monitor ended unexpectedly".to_owned())
}

fn request_planner_name(connection: &Connection) -> zbus::Result<()> {
    match connection.request_name_with_flags(SERVICE, RequestNameFlags::DoNotQueue.into())? {
        RequestNameReply::PrimaryOwner => Ok(()),
        reply => Err(zbus::Error::Failure(format!(
            "planner service name was not acquired: {reply}"
        ))),
    }
}

fn owner_changes_match_rule() -> zbus::Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .build())
}

fn monitor_owner_changes(monitor: &Connection) -> zbus::Result<MessageIterator> {
    MessageIterator::for_match_rule(owner_changes_match_rule()?, monitor, Some(16))
}

/// Pure name-loss decision: terminal when our planner name loses its owner.
fn planner_name_lost(name: &str, new_owner: Option<&str>, our_unique: Option<&str>) -> bool {
    if name != SERVICE {
        return false;
    }
    match (new_owner, our_unique) {
        (Some(next), Some(ours)) => next != ours,
        (None, _) => true,
        (_, None) => new_owner.is_none(),
    }
}

fn handle_name_owner_changed(
    registered_unique: &mut Option<String>,
    name: &str,
    new_owner: Option<String>,
    our_unique: Option<&str>,
) -> zbus::Result<()> {
    if name != SERVICE {
        return Ok(());
    }
    if planner_name_lost(name, new_owner.as_deref(), our_unique) {
        *registered_unique = None;
        return Err(zbus::Error::Failure(
            "planner service name ownership was lost".to_owned(),
        ));
    }
    *registered_unique = new_owner;
    Ok(())
}

/// Retained manually invoked planner service. Acquires the planner name
/// without queueing and serves the world plus the narrow
/// session-scoped float-intent membership routes until the serving
/// connection or the planner name is lost. No tray coupling; the float-intent
/// store is the sole persistence exception.
pub fn run() -> zbus::Result<()> {
    serve(PlannerEndpoint::new())
}

fn serve(endpoint: PlannerEndpoint) -> zbus::Result<()> {
    let connection = Connection::session()?;
    connection.object_server().at(OBJECT, endpoint)?;
    request_planner_name(&connection)?;
    let our_unique = connection.unique_name().map(|name| name.to_string());

    let monitor = Connection::session()?;
    let owner_changes = monitor_owner_changes(&monitor)?;
    let mut registered_unique = our_unique.clone();
    for message in owner_changes {
        if connection.is_closed() {
            return Err(serving_connection_lost_error());
        }
        if monitor.is_closed() {
            return Err(owner_monitor_ended_error());
        }
        let message = message?;
        match decide_owner_signal(message, &mut registered_unique, our_unique.as_deref())? {
            OwnerSignalDecision::SkippedMalformed => {
                let _ = writeln!(std::io::stderr(), "{}", owner_signal_malformed_line());
                continue;
            }
            OwnerSignalDecision::SkippedInvalidArgs => {
                let _ = writeln!(std::io::stderr(), "{}", owner_signal_args_invalid_line());
                continue;
            }
            OwnerSignalDecision::Handled => {}
        };
    }
    Err(owner_monitor_ended_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_identity_is_exact() {
        assert_eq!(SERVICE, "com.omnitiler.Planner");
        assert_eq!(OBJECT, "/com/omnitiler/Planner");
        assert_eq!(INTERFACE, "com.omnitiler.Planner1");
    }

    #[test]
    fn planner_name_request_disallows_queueing() {
        let flags = zbus::fdo::RequestNameFlags::DoNotQueue as u32;
        assert_eq!(flags, 0x04);
        assert_eq!(flags & (0x01 | 0x02), 0);
    }

    #[test]
    fn owner_monitor_uses_fixed_dbus_contract() {
        let rule = owner_changes_match_rule().unwrap().to_string();
        assert!(rule.contains("type='signal'"), "{rule}");
        assert!(rule.contains("sender='org.freedesktop.DBus'"), "{rule}");
        assert!(rule.contains("interface='org.freedesktop.DBus'"), "{rule}");
        assert!(rule.contains("member='NameOwnerChanged'"), "{rule}");
    }

    #[test]
    fn planner_name_loss_is_terminal() {
        assert!(planner_name_lost(SERVICE, None, Some(":1.7")));
        assert!(planner_name_lost(SERVICE, Some(":1.9"), Some(":1.7")));
        assert!(!planner_name_lost(SERVICE, Some(":1.7"), Some(":1.7")));
        assert!(!planner_name_lost(KWIN_SERVICE, None, Some(":1.7")));
    }

    #[test]
    fn planner_name_change_handler_exits_on_loss() {
        let mut registered = Some(":1.7".to_owned());
        let result = handle_name_owner_changed(&mut registered, SERVICE, None, Some(":1.7"));
        assert!(result.is_err());
        assert_eq!(registered, None);

        let mut registered = Some(":1.7".to_owned());
        let result = handle_name_owner_changed(&mut registered, KWIN_SERVICE, None, Some(":1.7"));
        assert!(result.is_ok());
        assert_eq!(registered, Some(":1.7".to_owned()));
    }

    #[test]
    fn same_uid_accepts() {
        let expected = rustix::process::geteuid().as_raw();
        assert!(caller_uid_authorized(Some(expected), expected));
    }

    #[test]
    fn differing_uid_rejects() {
        let expected = rustix::process::geteuid().as_raw();
        let differing = expected.wrapping_add(1);
        assert_ne!(differing, expected);
        assert!(!caller_uid_authorized(Some(differing), expected));
        assert!(!caller_uid_authorized(
            Some(expected.wrapping_add(1000)),
            expected
        ));
    }

    #[test]
    fn unavailable_uid_rejects() {
        let expected = rustix::process::geteuid().as_raw();
        assert!(!caller_uid_authorized(None, expected));
    }

    #[test]
    fn endpoint_is_single_flight_shared() {
        let endpoint = PlannerEndpoint::new();
        let cloned = endpoint.clone();
        assert!(Arc::ptr_eq(&endpoint.intent_lock, &cloned.intent_lock));
        assert_eq!(
            endpoint.world.generation(),
            cloned.world.generation(),
            "cloned endpoints share one world backend"
        );
    }

    #[test]
    fn intent_lock_is_independent_of_world_lock() {
        // Holding the world single-flight guard must not contend the intent
        // lock: a settled-intent call overlapping a world flight is admitted
        // on its own mutex instead of surfacing busy.
        let endpoint = PlannerEndpoint::without_intent_store();
        let held = endpoint.clone();
        let _world_guard = held
            .world
            .try_acquire_guard()
            .expect("world lock idle in test");
        assert!(
            endpoint.intent_lock.try_lock().is_some(),
            "intent admission must not wait on the world lock"
        );
    }

    #[test]
    fn single_flight_is_non_queuing_bounded() {
        let endpoint = PlannerEndpoint::new();
        let guard = endpoint.intent_lock.try_lock();
        assert!(guard.is_some(), "idle endpoint must acquire");
        assert!(
            endpoint.intent_lock.try_lock().is_none(),
            "contended endpoint must fail fast instead of queueing verify work"
        );
        drop(guard);
        assert!(endpoint.intent_lock.try_lock().is_some());
    }

    #[test]
    fn connection_loss_and_monitor_end_are_errors() {
        let serving = serving_connection_lost_error();
        assert!(serving.to_string().contains("serving connection was lost"));
        let ended = owner_monitor_ended_error();
        assert!(ended.to_string().contains("monitor ended unexpectedly"));
    }

    #[test]
    fn world_method_identity_is_exact_and_distinct() {
        use crate::world_service::{DISCOVERY_METHOD, WORLD_METHOD};
        assert_eq!(WORLD_METHOD, "DescribeWorld");
        assert_eq!(DISCOVERY_METHOD, "DiscoverWorld");
        assert_ne!(WORLD_METHOD, DISCOVERY_METHOD);
        assert_eq!(SERVICE, "com.omnitiler.Planner");
        assert_eq!(OBJECT, "/com/omnitiler/Planner");
        assert_eq!(INTERFACE, "com.omnitiler.Planner1");
    }

    #[test]
    fn world_method_signature_is_json_string_to_json_string() {
        use crate::world_service::WORLD_METHOD;
        let message = zbus::message::Message::method_call(OBJECT, WORLD_METHOD)
            .unwrap()
            .destination(SERVICE)
            .unwrap()
            .interface(INTERFACE)
            .unwrap()
            .build(&("{\"v\":1}".to_owned(),))
            .unwrap();
        assert_eq!(message.body().signature().to_string(), "s");
        let body: (String,) = message.body().deserialize().unwrap();
        assert_eq!(body.0, "{\"v\":1}");
    }

    #[test]
    fn intent_method_identity_is_exact_and_json_string_shaped() {
        use crate::float_intent_store::{
            INTENT_MAX_REPLY_BYTES, INTENT_MAX_REQUEST_BYTES, MAX_FLOAT_MEMBERS, READ_METHOD,
            WRITE_METHOD,
        };
        use crate::world_service::{DISCOVERY_METHOD, WORLD_METHOD};
        assert_eq!(READ_METHOD, "ReadFloatIntent");
        assert_eq!(WRITE_METHOD, "WriteFloatIntent");
        assert_ne!(READ_METHOD, WRITE_METHOD);
        assert_ne!(READ_METHOD, WORLD_METHOD);
        assert_ne!(WRITE_METHOD, WORLD_METHOD);
        assert_ne!(READ_METHOD, DISCOVERY_METHOD);
        assert_ne!(WRITE_METHOD, DISCOVERY_METHOD);
        for method in [READ_METHOD, WRITE_METHOD] {
            let message = zbus::message::Message::method_call(OBJECT, method)
                .unwrap()
                .destination(SERVICE)
                .unwrap()
                .interface(INTERFACE)
                .unwrap()
                .build(&("{\"v\":1}".to_owned(),))
                .unwrap();
            assert_eq!(message.body().signature().to_string(), "s");
        }
        // Bounds fit a full membership snapshot with margin.
        assert_eq!(INTENT_MAX_REQUEST_BYTES, 256 * 1024);
        assert_eq!(INTENT_MAX_REPLY_BYTES, 256 * 1024);
        assert_eq!(MAX_FLOAT_MEMBERS, 1024);
    }

    #[test]
    fn intent_constructors_scope_the_store() {
        // Construction never touches disk: the hermetic seam roots the store
        // at a private directory, and the absent seam disables persistence.
        assert!(
            PlannerEndpoint::without_intent_store()
                .intent_store
                .is_none()
        );
        let root = std::path::PathBuf::from("/nonexistent-intent-root");
        assert!(
            PlannerEndpoint::with_intent_root(root.clone())
                .intent_store
                .is_some()
        );
    }

    /// Throwaway private bus for hermetic intent tests. Private unix socket,
    /// no host session bus contact, no KWin, no mutation outside the temp
    /// dirs. Skips (returns `None`) when `dbus-daemon` cannot run here.
    struct PrivateBus {
        child: std::process::Child,
        socket_dir: std::path::PathBuf,
        address: String,
    }

    impl PrivateBus {
        fn start() -> Option<Self> {
            let socket_dir = std::env::temp_dir().join(format!(
                "omnitiler-test-intent-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or_default()
            ));
            std::fs::create_dir_all(&socket_dir).ok()?;
            let address = format!("unix:path={}", socket_dir.join("bus.sock").display());
            let child = std::process::Command::new("dbus-daemon")
                .arg("--session")
                .arg(format!("--address={address}"))
                .arg("--nofork")
                .arg("--nopidfile")
                .arg("--nosyslog")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()?;
            Some(Self {
                child,
                socket_dir,
                address,
            })
        }

        /// Connects to the private bus, waiting briefly for daemon startup.
        /// Returns `None` (skip) when the daemon never listens.
        fn session(&self) -> Option<Connection> {
            let start = std::time::Instant::now();
            loop {
                match zbus::blocking::connection::Builder::address(self.address.as_str())
                    .and_then(|builder| builder.build())
                {
                    Ok(connection) => return Some(connection),
                    Err(_) if start.elapsed() < std::time::Duration::from_secs(3) => {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    Err(_) => return None,
                }
            }
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.socket_dir);
        }
    }

    fn fresh_intent_root() -> Option<std::path::PathBuf> {
        let root = std::env::temp_dir().join(format!(
            "omnitiler-test-intent-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ));
        let mut builder = std::fs::DirBuilder::new();
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&root).ok()?;
        Some(root)
    }

    fn serve_intent_endpoint(server: &Connection, endpoint: PlannerEndpoint) {
        server
            .object_server()
            .at(OBJECT, endpoint)
            .expect("serve intent endpoint");
        server
            .request_name(SERVICE)
            .expect("request planner name on private bus");
    }

    fn intent_proxy(client: &Connection) -> zbus::blocking::Proxy<'_> {
        zbus::blocking::Proxy::new(client, SERVICE, OBJECT, INTERFACE).expect("intent proxy")
    }

    fn call_intent(proxy: &zbus::blocking::Proxy<'_>, method: &str, request: &str) -> String {
        let reply: (String,) = proxy
            .call(method, &(request.to_owned(),))
            .expect("intent call answers");
        reply.0
    }

    #[test]
    fn private_bus_intent_without_kwin_owner_is_unavailable() {
        use crate::float_intent_store::READ_METHOD as READ;
        use crate::float_intent_store::WRITE_METHOD as WRITE;
        let Some(bus) = PrivateBus::start() else {
            eprintln!("SKIP: dbus-daemon unavailable for hermetic intent test");
            return;
        };
        let Some(server) = bus.session() else {
            eprintln!("SKIP: private bus daemon never listened");
            return;
        };
        let Some(root) = fresh_intent_root() else {
            eprintln!("SKIP: temp dir unavailable for hermetic intent test");
            return;
        };
        serve_intent_endpoint(&server, PlannerEndpoint::with_intent_root(root.clone()));
        let Some(client) = bus.session() else {
            eprintln!("SKIP: private bus daemon dropped the second connection");
            return;
        };
        // Nobody owns org.kde.KWin here, so the bus-derived namespace is
        // unresolvable: reads degrade empty, writes stay unavailable, and
        // nothing is persisted.
        let proxy = intent_proxy(&client);
        let read: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            READ,
            r#"{"v":1,"correlation_id":"intent-neg-1"}"#,
        ))
        .expect("read reply is JSON");
        assert_eq!(read["outcome"], "degraded");
        assert_eq!(read["correlation_id"], "intent-neg-1");
        assert_eq!(read["reason"], "namespace-unavailable");
        assert_eq!(read["members"].as_array().expect("array").len(), 0);
        let write: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            WRITE,
            r#"{"v":1,"correlation_id":"intent-neg-1","members":["win-a"]}"#,
        ))
        .expect("write reply is JSON");
        assert_eq!(write["outcome"], "unavailable");
        assert_eq!(write["reason"], "namespace-unavailable");
        assert!(
            !root.join("omnitiler").exists(),
            "refused writes persist nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn private_bus_kwin_owner_roundtrip_persists_membership() {
        use crate::float_intent_store::READ_METHOD as READ;
        use crate::float_intent_store::WRITE_METHOD as WRITE;
        let Some(bus) = PrivateBus::start() else {
            eprintln!("SKIP: dbus-daemon unavailable for hermetic intent test");
            return;
        };
        let Some(server) = bus.session() else {
            eprintln!("SKIP: private bus daemon never listened");
            return;
        };
        let Some(root) = fresh_intent_root() else {
            eprintln!("SKIP: temp dir unavailable for hermetic intent test");
            return;
        };
        serve_intent_endpoint(&server, PlannerEndpoint::with_intent_root(root.clone()));
        let Some(client) = bus.session() else {
            eprintln!("SKIP: private bus daemon dropped the second connection");
            return;
        };
        // The caller becomes the current KWin owner on the private bus, so
        // its sender unique name authorizes the intent calls.
        client
            .request_name(KWIN_SERVICE)
            .expect("claim KWin name on private bus");
        let proxy = intent_proxy(&client);
        // A stale peer that is not the KWin owner is rejected in-band.
        let Some(outsider) = bus.session() else {
            eprintln!("SKIP: private bus daemon dropped the third connection");
            return;
        };
        let outsider_proxy = intent_proxy(&outsider);
        let rejected: serde_json::Value = serde_json::from_str(&call_intent(
            &outsider_proxy,
            WRITE,
            r#"{"v":1,"correlation_id":"intent-out-1","members":["win-a"]}"#,
        ))
        .expect("rejection is JSON");
        assert_eq!(rejected["outcome"], "rejected");
        assert_eq!(rejected["reason"], "not-kwin-owner");
        // Settled float persists the full snapshot through the wire.
        let stored: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            WRITE,
            r#"{"v":1,"correlation_id":"intent-rt-1","members":["win-a","win-b"]}"#,
        ))
        .expect("write reply is JSON");
        assert_eq!(stored["outcome"], "stored");
        assert_eq!(stored["stored"], 2);
        // Reads return the membership; a complete live inventory prunes the
        // closed window from the reply without mutating the file.
        let read: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            READ,
            r#"{"v":1,"correlation_id":"intent-rt-2"}"#,
        ))
        .expect("read reply is JSON");
        assert_eq!(read["outcome"], "ok");
        assert_eq!(read["stored"], 2);
        assert_eq!(read["returned"], 2);
        let pruned: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            READ,
            r#"{"v":1,"correlation_id":"intent-rt-3","live":["win-a"]}"#,
        ))
        .expect("pruned read is JSON");
        assert_eq!(pruned["outcome"], "ok");
        assert_eq!(pruned["stored"], 2);
        assert_eq!(pruned["returned"], 1);
        // Settled unfloat clears with an empty snapshot.
        let cleared: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            WRITE,
            r#"{"v":1,"correlation_id":"intent-rt-4","members":[]}"#,
        ))
        .expect("clear reply is JSON");
        assert_eq!(cleared["outcome"], "stored");
        assert_eq!(cleared["stored"], 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn private_bus_tile_override_roundtrip_preserves_disjointness() {
        use crate::float_intent_store::READ_METHOD as READ;
        use crate::float_intent_store::WRITE_METHOD as WRITE;
        let Some(bus) = PrivateBus::start() else {
            eprintln!("SKIP: dbus-daemon unavailable for hermetic intent test");
            return;
        };
        let Some(server) = bus.session() else {
            eprintln!("SKIP: private bus daemon never listened");
            return;
        };
        let Some(root) = fresh_intent_root() else {
            eprintln!("SKIP: temp dir unavailable for hermetic intent test");
            return;
        };
        serve_intent_endpoint(&server, PlannerEndpoint::with_intent_root(root.clone()));
        let Some(client) = bus.session() else {
            eprintln!("SKIP: private bus daemon dropped the second connection");
            return;
        };
        client
            .request_name(KWIN_SERVICE)
            .expect("claim KWin name on private bus");
        let proxy = intent_proxy(&client);
        // Float plus tile persist atomically with disjoint counts.
        let stored: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            WRITE,
            r#"{"v":1,"correlation_id":"intent-tile-1","members":["win-a"],"tile":["win-t"]}"#,
        ))
        .expect("write reply is JSON");
        assert_eq!(stored["outcome"], "stored");
        assert_eq!(stored["stored"], 1);
        assert_eq!(stored["tile_stored"], 1);
        // Overlapping snapshots reject without touching disk.
        let rejected: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            WRITE,
            r#"{"v":1,"correlation_id":"intent-tile-2","members":["win-a"],"tile":["win-a"]}"#,
        ))
        .expect("rejection is JSON");
        assert_eq!(rejected["outcome"], "rejected");
        assert_eq!(rejected["reason"], "invalid-members");
        // Reads return both sides; live prunes both without mutating the file.
        let read: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            READ,
            r#"{"v":1,"correlation_id":"intent-tile-3"}"#,
        ))
        .expect("read reply is JSON");
        assert_eq!(read["outcome"], "ok");
        assert_eq!(read["stored"], 1);
        assert_eq!(read["tile_stored"], 1);
        assert_eq!(read["tile_returned"], 1);
        let pruned: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            READ,
            r#"{"v":1,"correlation_id":"intent-tile-4","live":["win-a"]}"#,
        ))
        .expect("pruned read is JSON");
        assert_eq!(pruned["outcome"], "ok");
        assert_eq!(pruned["stored"], 1);
        assert_eq!(pruned["returned"], 1);
        assert_eq!(pruned["tile_stored"], 1);
        assert_eq!(pruned["tile_returned"], 0);
        // Float-only write carries an empty tile snapshot and clears it.
        let preserved: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            WRITE,
            r#"{"v":1,"correlation_id":"intent-tile-5","members":["win-b"]}"#,
        ))
        .expect("write reply is JSON");
        assert_eq!(preserved["outcome"], "stored");
        assert_eq!(preserved["stored"], 1);
        assert_eq!(preserved["tile_stored"], 0);
        let cleared: serde_json::Value = serde_json::from_str(&call_intent(
            &proxy,
            WRITE,
            r#"{"v":1,"correlation_id":"intent-tile-6","members":["win-b"],"tile":[]}"#,
        ))
        .expect("clear reply is JSON");
        assert_eq!(cleared["outcome"], "stored");
        assert_eq!(cleared["tile_stored"], 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn private_bus_intent_proceeds_while_world_flight_held() {
        use crate::float_intent_store::WRITE_METHOD as WRITE;
        let Some(bus) = PrivateBus::start() else {
            eprintln!("SKIP: dbus-daemon unavailable for hermetic intent test");
            return;
        };
        let Some(server) = bus.session() else {
            eprintln!("SKIP: private bus daemon never listened");
            return;
        };
        let Some(root) = fresh_intent_root() else {
            eprintln!("SKIP: temp dir unavailable for hermetic intent test");
            return;
        };
        let endpoint = PlannerEndpoint::with_intent_root(root.clone());
        // Simulate an in-flight world call holding the world single-flight
        // guard: the intent route must still be admitted on its own lock,
        // never surfacing the world's busy error. The guard is held through
        // a cloned endpoint so the serving endpoint can move into the
        // object server.
        let held = endpoint.clone();
        let _world_guard = held
            .world
            .try_acquire_guard()
            .expect("world lock idle in test");
        serve_intent_endpoint(&server, endpoint);
        let Some(client) = bus.session() else {
            eprintln!("SKIP: private bus daemon dropped the second connection");
            return;
        };
        client
            .request_name(KWIN_SERVICE)
            .expect("claim KWin name on private bus");
        let stored: serde_json::Value = serde_json::from_str(&call_intent(
            &intent_proxy(&client),
            WRITE,
            r#"{"v":1,"correlation_id":"intent-lock-1","members":["win-a"]}"#,
        ))
        .expect("write reply is JSON");
        assert_eq!(stored["outcome"], "stored");
        assert_eq!(stored["stored"], 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_early_exit_summaries_are_fixed_and_uncorrelated() {
        let lines = [
            super::plan_early_exit_summary(super::PlanEarlyExit::Busy),
            super::plan_early_exit_summary(super::PlanEarlyExit::Closed),
            super::plan_early_exit_summary(super::PlanEarlyExit::Oversize),
            super::plan_early_exit_summary(super::PlanEarlyExit::Unauthorized),
        ];
        assert_eq!(
            lines,
            [
                "omnitiler:plan-summary direction=egress op=unknown correlation=- outcome=unavailable kind=busy base_revision=- detail=early-exit",
                "omnitiler:plan-summary direction=egress op=unknown correlation=- outcome=unavailable kind=connection-closed base_revision=- detail=early-exit",
                "omnitiler:plan-summary direction=egress op=unknown correlation=- outcome=unavailable kind=request-oversize base_revision=- detail=early-exit",
                "omnitiler:plan-summary direction=egress op=unknown correlation=- outcome=rejected kind=unauthorized base_revision=- detail=early-exit",
            ]
        );
        for line in lines {
            assert!(line.contains("correlation=-"), "{line}");
            assert!(!line.contains("owner"), "{line}");
            assert!(!line.contains("payload"), "{line}");
        }
    }

    fn malformed_owner_message() -> zbus::message::Message {
        use crate::world_service::WORLD_METHOD;
        zbus::message::Message::method_call(OBJECT, WORLD_METHOD)
            .unwrap()
            .destination(SERVICE)
            .unwrap()
            .interface(INTERFACE)
            .unwrap()
            .build(&("arg",))
            .unwrap()
    }

    fn owner_signal_message(
        name: &str,
        old_owner: &str,
        new_owner: &str,
    ) -> zbus::message::Message {
        zbus::message::Message::signal(
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameOwnerChanged",
        )
        .unwrap()
        .sender("org.freedesktop.DBus")
        .unwrap()
        .build(&(name, old_owner, new_owner))
        .unwrap()
    }

    fn invalid_owner_args_message() -> zbus::message::Message {
        zbus::message::Message::signal(
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameOwnerChanged",
        )
        .unwrap()
        .sender("org.freedesktop.DBus")
        .unwrap()
        .build(&("only-one-field",))
        .unwrap()
    }

    #[test]
    fn malformed_owner_signal_skips_and_later_valid_signal_applies() {
        let our = ":1.7";
        let mut registered = Some(our.to_owned());
        let decision =
            super::decide_owner_signal(malformed_owner_message(), &mut registered, Some(our))
                .expect("malformed signal never fails");
        assert_eq!(decision, super::OwnerSignalDecision::SkippedMalformed);
        assert_eq!(registered, Some(our.to_owned()), "skip preserves owner pin");
        let line = super::owner_signal_malformed_line();
        assert!(line.contains("outcome=malformed-signal"), "{line}");
        assert!(line.contains("component=planner"), "{line}");
        assert!(!line.contains(":1."), "{line}");
        assert!(!line.contains('\n'), "{line}");

        let decision = super::decide_owner_signal(
            owner_signal_message(KWIN_SERVICE, "", ":1.9"),
            &mut registered,
            Some(our),
        )
        .expect("unrelated valid signal never fails");
        assert_eq!(decision, super::OwnerSignalDecision::Handled);
        assert_eq!(
            registered,
            Some(our.to_owned()),
            "unrelated signal keeps pin"
        );
    }

    #[test]
    fn invalid_owner_args_skip_and_later_valid_signal_applies() {
        let our = ":1.7";
        let mut registered = Some(our.to_owned());
        let decision =
            super::decide_owner_signal(invalid_owner_args_message(), &mut registered, Some(our))
                .expect("invalid args never fail");
        assert_eq!(decision, super::OwnerSignalDecision::SkippedInvalidArgs);
        assert_eq!(registered, Some(our.to_owned()), "skip preserves owner pin");
        let line = super::owner_signal_args_invalid_line();
        assert!(line.contains("outcome=invalid-args"), "{line}");
        assert!(line.contains("component=planner"), "{line}");
        assert!(!line.contains(":1."), "{line}");
        assert!(!line.contains('\n'), "{line}");

        let decision = super::decide_owner_signal(
            owner_signal_message(SERVICE, our, our),
            &mut registered,
            Some(our),
        )
        .expect("later valid signal never fails");
        assert_eq!(decision, super::OwnerSignalDecision::Handled);
        assert_eq!(registered, Some(our.to_owned()));

        let loss = super::decide_owner_signal(
            owner_signal_message(SERVICE, our, ""),
            &mut registered,
            Some(our),
        );
        assert!(loss.is_err(), "real name loss stays terminal");
        assert_eq!(registered, None);
    }
}
