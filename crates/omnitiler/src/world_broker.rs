//! Sole world broker: one [`WorldEngine`], one service-owned sequencing namespace.
//!
//! The broker is the only [`WorldEngine`] owner on this bus. It mints one
//! core generation and one broker adapter incarnation at construction, stamps
//! every capture with its own monotonic observation/request sequences and its
//! own [`std::time::Instant`] clock, and maps per-sender per-client command
//! identities onto one global command sequence. JavaScript never owns
//! counters: the capture envelope carries the generation, the
//! discovery-bound adapter id, a per-capture source correlation, and a native
//! invalidation watermark, all echoed back (bound) in every response so the
//! actuator can fence invalidations that landed during flight.
//!
//! Roles (enforced in [`crate::planner_service`], never here):
//!
//! - Capture/actuator: the single live KWin sender. `DescribeWorld` takes one
//!   broker capture envelope per call and returns one bound response. Native
//!   work rides the actuator response only; a queued external command drains
//!   into the next fresh capture, never into a stale or replayed one. The
//!   actuator reply never carries the terminal command result, even when the
//!   actuator happens to be the issuer: results are requester-only.
//! - Query/presentation/command: any same-UID client. `QueueWorldCommand`
//!   files one command, `TakeCommandResult` collects its terminal outcome
//!   (re-readable while the bounded memory holds it, so a lost reply never
//!   loses the outcome), and `ReadWorldPresentation` reads the cached
//!   accepted presentation plus the current drop preview. One-shot lanes
//!   (result, transfers, effects, settlements, warp) and native write
//!   tokens (`frame_op`, `clear_op`) never enter the presentation cache;
//!   only the renderer-only drop preview (paint `rect`, mover-home
//!   `sourceOut`/`sourceWs`, insertion-neighbor `win`; or null) rides
//!   alongside it so a renderer without take-lane authority can paint
//!   exactly what core published.
//!
//! Wire shapes (strict v2, no v1 compatibility route):
//!
//! ```text
//! discovery: {"v":2,"adapter":A,"req":R}
//!   `adapter` is the producer capture lifetime (the actuator's opaque
//!   incarnation, e.g. `kwin-world-adapter-...`), NOT the broker
//!   incarnation. Discovery binds exactly one active lifetime: rediscovery
//!   of the same lifetime rebinds without reset, while a new lifetime
//!   rotates the broker (fresh generation, fresh broker incarnation, all
//!   capture/queue/result/presentation state dropped) so no old capture can
//!   ever apply again. Replies `ready=false` with the serving generation,
//!   the broker incarnation in `adapter`, and no applicable desired state.
//! capture:  {"v":2,"adapter":A,"gen":G,"source":S,"sourceSeq":Q,
//!            "nativeInvalid":"N","report":{...}}
//!   `report` is the full projected v2 observation WITHOUT the broker-owned
//!   keys (owner, adapter, gen, obs, reqSeq, req, cmdSeq, command,
//!   monotonicMs): carrying any of them refuses as unknown-field, so there
//!   is exactly one sequencing namespace. `adapter` is the producer capture
//!   lifetime bound at discovery (undiscovered lifetimes refuse, and only
//!   one lifetime is ever bound); `gen` must equal the serving generation
//!   (stale generations refuse without applying). `source` is the
//!   per-capture correlation (JS shape `src-<counter36>-<rand36>`, opaque to
//!   the broker beyond the correlation alphabet); `sourceSeq` is the
//!   per-lifetime monotonic capture sequence (canonical decimal u64,
//!   1-based) the actuator minted alongside the source: transport only,
//!   never core sequencing. `nativeInvalid` is a canonical decimal u64 the
//!   actuator advances on any invalidating native input, including during an
//!   outstanding call; it must be monotonic per lifetime (regressions
//!   refuse). Redelivery of a remembered source replays byte-identically
//!   (never drains, never re-sequences); the same source with different
//!   bytes refuses as reused; any unremembered source at or below the
//!   per-lifetime floor refuses as stale, so evicted and older captures
//!   never re-dispatch forgotten facts into a newer queue.
//! response: the core reply object extended with "source", "nativeInvalid"
//!   and "clock" (the broker dispatch-clock reading in milliseconds, the
//!   epoch raw attempt timestamps must fit), with the terminal "result"
//!   always nulled for the actuator lane; or the bounded in-band error
//!   {"v":2,"outcome":"rejected","kind","message","gen"} extended with
//!   "source", "nativeInvalid" and "clock". Unknown envelopes bind source
//!   "" and watermark "0", which never match a real source. Every response
//!   stays within the reply cap, including the bound envelope expansion.
//! queue:    {"v":2,"gen":G,"client":C,"clientCmd":"N","command":{...}}
//!   `gen` must equal the serving generation (stale generations refuse as
//!   generation-mismatch before any high-water or queue state is touched, so
//!   a delayed old-generation call can never file an old intent into the new
//!   world). `clientCmd` is a canonical decimal u64 advanced per client; at
//!   or below the per-sender per-client high-water it is a duplicate and
//!   never re-executes. The client namespace binds the authenticated D-Bus
//!   sender: one sender can neither take another's results nor move its
//!   high-water. Replies {"v":2,"outcome":"queued","gen","client",
//!   "clientCmd","globalCmd":"G"} (or "duplicate" with the same body).
//! take:     {"v":2,"gen":G,"client":C,"clientCmd":"N"}
//!   `gen` binds like the queue lane: stale generations refuse before any
//!   settlement or high-water access. Replies {"v":2,"outcome":"pending",
//!   "gen"} while queued or undispatched;
//!   {"v":2,"outcome":"ready","gen","globalCmd":"G","result":R|null} once the
//!   command dispatches (re-readable while remembered, never replaying
//!   effects); {"v":2,"outcome":"failed","gen","globalCmd":"G","kind",
//!   "message"} when the world rejected the dispatched command (truthful
//!   terminal settlement, never a null result masquerading as success);
//!   {"v":2,"outcome":"duplicate","gen"} when forgotten through bounded
//!   eviction (never re-executes); or an unknown-command rejection.
//! read:     {"v":2,"req":R}
//!   Replies {"v":2,"outcome":"presentation","gen","req","rev","obs",
//!   "scopes","workspaces","outputs","windows","presentation","preview"} from
//!   the last accepted capture, or a not-ready rejection before the first
//!   one. Window rows carry no frame_op/clear_op tokens. `preview` is the
//!   renderer-only drop preview (paint `rect`, mover-home `sourceOut`/
//!   `sourceWs`, insertion-neighbor `win`) when the cached reply carried a
//!   drop preview, else null; every one-shot lane stays excluded.
//! ```
//!
//! Dispatch rules. A syntactically invalid capture (envelope or commandless
//! report) rejects without touching the engine or the queue, so a bad
//! capture never retires a good queued command. A syntactically valid
//! capture whose queued command the world rejects settles that command
//! terminally as `failed` with the core kind/message, then dispatches the
//! same fresh facts observation-only so the stream never wedges (the command
//! never re-executes). The fallback verdict binds for its source exactly
//! like any other outcome but never touches the presentation cache (last
//! accepted projection only), so a failed drain can never wedge or poison
//! the cached full reply. Raw attempt timestamps (`attemptMs` in
//! frame/member/view/clear attempts) ride the foreign adapter epoch: the
//! broker refuses any stamp above the dispatch clock and publishes that
//! clock in every capture response for the next attempts. Redeliveries of
//! remembered sources replay the exact bound outcome (accepted or in-band
//! rejection); forgotten sequences refuse as stale and never re-dispatch.
//!
//! Wake signal (best-effort, emitted by the service layer, never here):
//! `WorldWake(gen, rev, kind)` with kind `presentation-updated` after an
//! accepted capture or `command-queued` after a new queue entry. The KWin
//! scripting surface cannot hear signals, so the actuator additionally
//! holds one long-lived `WaitWorldWake` call per generation: the service
//! parks it while idle and completes it when queued work can drain. The
//! wait route never executes native work or engine logic; the follow-up
//! capture does that.
//!
//! Wait wire (strict v2, same auth as capture/discover: same-UID plus the
//! live KWin owner plus the bound producer lifetime and generation):
//! wait:     {"v":2,"gen":G,"adapter":A,"nonce":N,"cursor":"C"}
//!   `adapter` is the discovery-bound producer lifetime, `nonce` the
//!   waiter correlation fencing stale completions, `cursor` the last
//!   delivered wake cursor ("0" before the first wake), all canonical.
//!   Immediate `{"v":2,"outcome":"wake","gen":G,"cursor":"E"}` when the
//!   broker event epoch `E` advanced past `cursor` AND a command is
//!   pending; otherwise the single bounded server waiter parks (new
//!   waiters supersede parked ones, rotations drop them). Epoch `E`
//!   advances only on a newly queued command or an actual head drain
//!   (never on exact-capture replay, malformed captures, or
//!   rejected/no-drain captures), and the reply cursor only advances on a
//!   delivered wake, so a lease `{"v":2,"outcome":"lease","gen":G,
//!   "cursor":"C"}` (idle server lease, cursor unchanged) or a lost reply
//!   re-armed with the old cursor can never skip unseen work.
//!   `{"v":2,"outcome":"superseded","gen":G}` answers a replaced waiter.
//!
//! Raw classification capture (Stage 4): the inner `windows` lane carries
//! KWin-shaped native facts plus the tolerant intent lanes; the broker
//! normalizes them into projected v2 `WindowFact`s via
//! `crate::kde_raw` before stamping, sequencing, or draining. Carrying
//! legacy projected verdict keys inside a raw row refuses as
//! `unknown-field`: there is no projected-fact fallback lane.
//!
//! Bounds: 128 queued commands (`queue-full` past that), 1024 settled plus
//! 1024 retired terminal results with oldest-first eviction (duplicates of
//! evicted entries report `duplicate` with no result and never re-execute),
//! 1024 command clients per sender (`queue-full` past that rather than
//! forgetting a high-water and risking re-execution), 1024 remembered
//! per-source bound capture outcomes with oldest-first eviction (evicted
//! sources lose replay and refuse as stale through the sequence floor,
//! never re-dispatch).
//!
//! Reconnect limitation: client namespaces bind the authenticated D-Bus
//! unique-name sender, which changes on reconnect. A reconnected issuer must
//! re-establish (re-discover, fresh client namespace): commands queued under
//! the old sender still drain exactly once, but results settled under the old
//! sender are uncollectible from the new one. A reconnected ACTUATOR
//! re-discovers under a fresh producer lifetime, which rotates the broker
//! exactly like poison or counter exhaustion below: the actuator re-observes
//! from rev 0 and issuers re-establish after observing the new generation in
//! any reply. Poison or counter exhaustion mints a fresh generation and
//! drops queued/settled command state, the lifetime binding, the sequence
//! floor, watermarks, and remembered sources; issuers re-establish after
//! re-discovery with fresh per-client command ids.
//!
//! Stage 3 timestamp transport (specified here, implemented there): raw
//! attempt `attemptMs` values MUST be broker-epoch milliseconds learned from
//! the last capture response `clock` field (0 means unreported epoch and
//! never suppresses). The broker refuses any `attemptMs` above the dispatch
//! clock (`snapshot-invalid`/`attempt-clock-invalid`); timestamps at or below
//! the clock pass through to core unchanged. The broker never adjusts,
//! interpolates, or manufactures attempt evidence: unknown stays unknown,
//! and no synthetic stamp ever proves native completion or arrival.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use serde_json::Map;

use tiler_protocol::world_protocol::{
    WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, WorldEngine, WorldOutcome, decode_request,
    encode_error, encode_reply, is_valid_adapter_id, is_valid_request_id,
};

/// Broker-owned logical owner stamped on every core report.
pub const BROKER_OWNER: &str = "omnitiler-broker";
/// Broker wire contract version (equals the core/world protocol version).
pub const BROKER_VERSION: u32 = 2;
/// FIFO cap for commands awaiting the next actuator capture.
pub const MAX_PENDING_COMMANDS: usize = 128;
/// Cap for settled terminal results awaiting collection (oldest evicted).
pub const MAX_SETTLED_RESULTS: usize = 1024;
/// Cap for distinct command clients (fail-closed past it, never forget a
/// high-water and risk re-execution).
pub const MAX_COMMAND_CLIENTS: usize = 1024;

/// Keys the broker stamps itself; carrying any of them inside the inner
/// capture report refuses as `unknown-field`.
const BROKER_KEYS: [&str; 9] = [
    "owner",
    "adapter",
    "gen",
    "obs",
    "reqSeq",
    "req",
    "cmdSeq",
    "command",
    "monotonicMs",
];

/// Terminal dispatch failure: oversize envelopes fail closed, everything
/// else is an in-band JSON body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerError {
    Oversized,
    Rotate,
}

/// Cap for remembered per-source bound capture outcomes (accepted replies
/// and in-band rejections alike). Oldest-first eviction: evicted sources
/// lose replay and refuse as stale through the sequence floor.
pub const MAX_REMEMBERED_SOURCES: usize = 1024;

/// One queued external command awaiting the next fresh capture.
#[derive(Debug, Clone)]
struct QueuedCommand {
    sender: String,
    client: String,
    client_cmd: String,
    global_seq: u64,
    command: serde_json::Value,
}

impl QueuedCommand {
    /// Sender-bound settlement key: no cross-sender aliasing.
    fn key(&self) -> (String, String, String) {
        (
            self.sender.clone(),
            self.client.clone(),
            self.client_cmd.clone(),
        )
    }
}

/// Terminal settlement of one dispatched command: success carries the core
/// result (possibly null when the snapshot alone is the outcome); semantic
/// rejection carries the explicit core kind/message (never a null result
/// masquerading as success).
#[derive(Debug, Clone)]
enum Settlement {
    Ready {
        global_seq: u64,
        result: serde_json::Value,
    },
    Failed {
        global_seq: u64,
        kind: String,
        message: String,
    },
}

impl Settlement {
    fn global_seq(&self) -> u64 {
        match self {
            Settlement::Ready { global_seq, .. } | Settlement::Failed { global_seq, .. } => {
                *global_seq
            }
        }
    }
}

/// Outcome of one fresh capture dispatch: either an unbound core reply for
/// the caller to strip, project, bind, and remember, or an already-shaped
/// core rejection for the caller to bind and remember without touching the
/// presentation cache.
#[derive(Debug)]
enum FreshOutcome {
    Reply(String),
    Rejected {
        kind: &'static str,
        message: &'static str,
    },
}

/// Sole world broker. Not Clone: exactly one lives behind the service lock.
#[derive(Debug)]
pub struct WorldBroker {
    engine: WorldEngine,
    generation: String,
    incarnation: String,
    clock_base: Instant,
    next_observation_seq: u64,
    next_request_seq: u64,
    next_command_seq: u64,
    pending: VecDeque<QueuedCommand>,
    /// Per-sender per-client command high-water. Namespaces bind the
    /// authenticated D-Bus unique-name sender: one sender can neither move
    /// another's high-water nor take its results.
    highwater: HashMap<(String, String), u64>,
    settled: HashMap<(String, String, String), Settlement>,
    settled_order: VecDeque<(String, String, String)>,
    /// Collected terminal settlements, re-served verbatim so a lost take
    /// reply never loses the outcome. Bounded like `settled`.
    retired: HashMap<(String, String, String), Settlement>,
    retired_order: VecDeque<(String, String, String)>,
    /// Cached accepted presentation projection (snapshot plus
    /// presentation plus the renderer-only drop preview, window write tokens
    /// stripped). Never the full reply.
    presentation: Option<String>,
    /// Single active producer capture lifetime bound at discovery. Captures
    /// from any other lifetime refuse, so old actuator lifetimes can never
    /// apply after a new one binds (never an ever-growing set of valid old
    /// adapters).
    capture_lifetime: Option<String>,
    /// Highest dispatched per-capture source sequence this lifetime. Only a
    /// sequence past the floor dispatches; older or evicted (forgotten)
    /// sources refuse as stale and never re-dispatch.
    source_floor: u64,
    /// Per-sender native invalidation high-water. Regressions refuse.
    watermarks: HashMap<String, u64>,
    /// Broker-owned gesture identity map (one broker generation): raw
    /// native snapshots fold to core ingress ids here before stamping.
    /// Rotation replaces the whole broker, so the map resets with the
    /// engine.
    gesture_map: crate::kde_gesture::KdeGestureMap,
    /// Per-source bound capture outcomes keyed by exact envelope bytes
    /// (accepted bound replies and bound in-band rejections alike), with
    /// oldest-first eviction. Byte-identical redelivery of a remembered
    /// source replays exactly; the same source with different bytes refuses
    /// as reused.
    responses: HashMap<String, (Vec<u8>, String)>,
    response_order: VecDeque<String>,
    /// Broker-owned wake event epoch (the server wake cursor): advances on
    /// a newly queued command and on an actual head drain only, never on
    /// exact-capture replay or on rejected/no-drain captures. Scoped to the
    /// generation/lifetime: rotation replaces the whole broker.
    wake_epoch: u64,
    /// Single bounded parked server waiter: the outstanding `WaitWorldWake`
    /// correlation, its last delivered cursor, and its waker. New waiters
    /// supersede parked ones (the old waker fires with a superseded
    /// answer); rotations drop the slot with the broker.
    waiter: Option<BrokerWaiter>,
    /// Delivered-but-uncollected wake reply keyed by waiter nonce. The
    /// parked task collects exactly its own nonce; a lease expiry never
    /// creates one, so a lost wake reply re-uses the old cursor.
    wait_ready: Option<(String, String)>,
}

/// Single parked server waiter. `waker` wakes the parked service task when
/// queued work (or a superseding waiter) completes it; tests park without
/// one and collect through `take_wait_ready` instead. Crate-visible for the
/// poison-handoff seam in [`crate::world_service`].
#[derive(Debug)]
pub(crate) struct BrokerWaiter {
    nonce: String,
    cursor: u64,
    waker: Option<std::task::Waker>,
}

/// Mint a broker adapter incarnation (`broker-<nanos>-<pid>-<counter>`):
/// opaque alphabet, within the id bound.
#[must_use]
pub fn mint_broker_incarnation() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("broker-{nanos}-{pid}-{count}")
}

/// Authenticated sender namespace: the D-Bus unique name of the caller when
/// it looks like one (`:1.42` shape), otherwise any non-empty bounded opaque
/// token (alphanumeric plus `:`, `-`, `_`, `.`, at most 128 bytes) such as a
/// queue-client id carried over a local test transport. Empty, overlong, or
/// out-of-alphabet senders fail closed as unauthorized.
fn valid_sender(sender: &str) -> Option<String> {
    if sender.is_empty() || sender.len() > 128 {
        return None;
    }
    if !sender
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b':' || b == b'-' || b == b'_' || b == b'.')
    {
        return None;
    }
    Some(sender.to_owned())
}

/// Bounded canonical decimal `u64` string (mirrors the codec gate: JSON
/// numbers, signs, leading zeros, and overflow refuse).
#[must_use]
pub fn is_canonical_u64(text: &str) -> bool {
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

impl WorldBroker {
    /// Fresh sole broker: new core generation, new broker incarnation,
    /// default engine config (the Linux route; no stale per-output alignment
    /// option exists in core).
    #[must_use]
    pub fn new() -> Self {
        Self::fresh()
    }

    fn fresh() -> Self {
        let generation = crate::world_service::mint_core_generation();
        let incarnation = mint_broker_incarnation();
        let engine = WorldEngine::new(&generation).expect("minted generation is valid");
        Self {
            engine,
            generation,
            incarnation,
            clock_base: Instant::now(),
            next_observation_seq: 1,
            next_request_seq: 1,
            next_command_seq: 1,
            pending: VecDeque::new(),
            highwater: HashMap::new(),
            settled: HashMap::new(),
            settled_order: VecDeque::new(),
            retired: HashMap::new(),
            retired_order: VecDeque::new(),
            presentation: None,
            capture_lifetime: None,
            source_floor: 0,
            watermarks: HashMap::new(),
            gesture_map: crate::kde_gesture::KdeGestureMap::new(),
            responses: HashMap::new(),
            response_order: VecDeque::new(),
            wake_epoch: 0,
            waiter: None,
            wait_ready: None,
        }
    }

    /// Replace backend state under a fresh generation/incarnation (poison or
    /// counter exhaustion). Queued and settled command state drops with the
    /// old namespace; issuers re-establish after re-discovery. A displaced
    /// parked waiter wakes immediately with generation-mismatch against the
    /// fresh generation instead of sleeping out its lease.
    pub fn rotate(&mut self) {
        let displaced = self.take_parked_waiter();
        *self = Self::fresh();
        if let Some(waiter) = displaced {
            self.complete_displaced(waiter);
        }
    }

    /// Serving core generation (the discovery/bootstrap binding).
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }

    /// Service-owned broker incarnation stamped on every core report.
    #[must_use]
    pub fn incarnation(&self) -> &str {
        &self.incarnation
    }

    /// Service-owned monotonic clock in milliseconds since broker
    /// construction (arbitrary process epoch, never wall-clock). Saturates
    /// rather than wrapping.
    #[must_use]
    pub fn monotonic_ms(&self) -> u64 {
        self.clock_base
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    /// Discovery/bootstrap body: binds the producer capture lifetime, then
    /// returns the generation plus capabilities with `ready=false` and no
    /// applicable desired state. Rediscovery of the same lifetime rebinds
    /// without reset (generation, floor, caches, and queue state survive);
    /// a new lifetime rotates the broker first (fresh generation, fresh
    /// broker incarnation, all capture/queue/result/presentation state
    /// dropped) so no capture from the old lifetime can ever apply again.
    /// The reply's adapter field carries the broker incarnation (authority
    /// stays with the broker); the caller's lifetime correlates via `req`.
    #[must_use]
    pub fn discovery_body(&mut self, adapter: &str, request_id: &str) -> String {
        if self.capture_lifetime.as_deref() != Some(adapter) {
            let displaced = self.take_parked_waiter();
            *self = Self::fresh();
            self.capture_lifetime = Some(adapter.to_owned());
            if let Some(waiter) = displaced {
                self.complete_displaced(waiter);
            }
        }
        let incarnation = self.incarnation.clone();
        encode_reply(&self.engine.bootstrap_reply(&incarnation, request_id))
    }

    /// Bounded in-band rejection bound to the serving generation, with the
    /// source watermark and clock binding (empty/zero when the envelope
    /// never parsed).
    #[must_use]
    pub fn error_body(&self, kind: &str, message: &str) -> String {
        bind_response(
            &encode_error(kind, message, &self.generation),
            "",
            0,
            self.monotonic_ms(),
        )
    }

    /// Bounded in-band rejection for a parsed capture: binds the source,
    /// watermark, and clock.
    fn bound_error(&self, kind: &str, message: &str, parsed: &ParsedCapture, clock: u64) -> String {
        bind_response(
            &encode_error(kind, message, &self.generation),
            &parsed.source,
            parsed.native_invalid,
            clock,
        )
    }

    /// Evaluate one capture envelope end to end: bound the bytes, parse the
    /// strict v2 envelope, replay byte-identical redeliveries of remembered
    /// sources without touching the engine (never draining from an old
    /// capture), otherwise bind generation/lifetime/watermark/source
    /// sequence, stamp the broker-owned sequencing namespace, validate
    /// commandless before draining, drain at most one queued command, and
    /// dispatch. In-band rejections bind for their source and are remembered
    /// exactly like accepted replies, but never touch the presentation cache
    /// (last accepted projection only) and never retire queued commands they
    /// did not dispatch.
    pub fn dispatch_capture(&mut self, envelope: &str) -> Result<String, BrokerError> {
        if envelope.len() > WORLD_MAX_REQUEST_BYTES {
            return Err(BrokerError::Oversized);
        }
        let parsed = match parse_capture_envelope(envelope) {
            Ok(parsed) => parsed,
            Err(failure) => {
                return Ok(bind_response(
                    &encode_error(failure.kind, failure.message, &self.generation),
                    &failure.source,
                    failure.native_invalid,
                    self.monotonic_ms(),
                ));
            }
        };
        // Byte-identical redelivery of a remembered source replays exactly:
        // no new sequences, no revision bump, and never a drain from an old
        // capture. A remembered source with different bytes refuses as
        // reused instead of aliasing a new observation.
        if let Some((prior, response)) = self.responses.get(&parsed.source) {
            if prior == envelope.as_bytes() {
                return Ok(response.clone());
            }
            return Ok(self.bound_error(
                "source-reused",
                "capture source reused with different body",
                &parsed,
                self.monotonic_ms(),
            ));
        }
        let clock_now = self.monotonic_ms();
        // Generation binding: delayed captures from a rotated broker refuse
        // without applying.
        if parsed.generation != self.generation {
            return Ok(self.bound_error(
                "generation-mismatch",
                "generation mismatch",
                &parsed,
                clock_now,
            ));
        }
        // Single active capture lifetime: the envelope adapter must be the
        // lifetime bound at discovery, so old lifetimes cannot apply after a
        // new one binds (and undiscovered lifetimes never apply at all).
        if self.capture_lifetime.as_deref() != Some(parsed.adapter.as_str()) {
            return Ok(self.bound_error("unknown-adapter", "unknown adapter", &parsed, clock_now));
        }
        // Watermark monotonicity per lifetime: regressions refuse.
        let floor = self.watermarks.get(&parsed.adapter).copied().unwrap_or(0);
        if parsed.native_invalid < floor {
            return Ok(self.bound_error(
                "stale-watermark",
                "native watermark is stale",
                &parsed,
                clock_now,
            ));
        }
        // Per-capture source-sequence floor: only a sequence past the floor
        // dispatches. An unremembered source at or below the floor is an
        // older capture redelivered after eviction (or a rewound producer):
        // it refuses as stale and never re-dispatches forgotten facts into
        // a newer queue.
        if parsed.source_seq <= self.source_floor {
            return Ok(self.bound_error(
                "stale-source",
                "capture source is stale",
                &parsed,
                clock_now,
            ));
        }
        // Stage 4 raw classification capture: normalize the KWin-shaped raw
        // `windows` lane into projected v2 facts before any sequencing,
        // floor, watermark, queue, or presentation change. A malformed raw
        // envelope is remembered for exact replay like any other rejection
        // but drains nothing and consumes no counters. Legacy projected
        // verdict keys inside a raw row refuse as unknown-field: there is
        // no projected-fact fallback lane.
        let ParsedCapture {
            adapter,
            generation,
            source,
            source_seq,
            native_invalid,
            inner,
        } = parsed;
        let mut raw_inner = inner;
        if let Err((kind, message)) = crate::kde_raw::normalize_raw_windows(&mut raw_inner) {
            let refused = ParsedCapture {
                adapter,
                generation,
                source,
                source_seq,
                native_invalid,
                inner: Map::new(),
            };
            return self.remember_rejection(&refused, envelope, kind, message, clock_now);
        }
        // Broker-owned gesture normalization: the raw native lane folds to
        // the core ingress lane (script never mints) on a staged map clone
        // before sequencing. Like the raw classification above, a malformed
        // lane remembers an exact-replay rejection and never installs map
        // state. The staged map installs only with an accepted
        // `FreshOutcome::Reply` below; every other outcome (rejection,
        // decode failure, attempt-clock refusal, fallback rejection)
        // discards the staged mutation so a malformed report with a valid
        // native lane can never retire sources or spend identity/revision.
        let mut staged_gesture_map = self.gesture_map.clone();
        let gesture_assoc = match staged_gesture_map.normalize_gesture_lane(&mut raw_inner) {
            Ok(assoc) => assoc,
            Err(crate::kde_gesture::GestureLaneError::Rotate) => {
                return Err(BrokerError::Rotate);
            }
            Err(crate::kde_gesture::GestureLaneError::Reject(kind, message)) => {
                let refused = ParsedCapture {
                    adapter,
                    generation,
                    source,
                    source_seq,
                    native_invalid,
                    inner: Map::new(),
                };
                return self.remember_rejection(&refused, envelope, kind, message, clock_now);
            }
        };
        let parsed = ParsedCapture {
            adapter,
            generation,
            source,
            source_seq,
            native_invalid,
            inner: raw_inner,
        };
        self.source_floor = parsed.source_seq;
        let observation_seq = self.take_observation_seq()?;
        let request_seq = self.take_request_seq()?;
        let clock = self.monotonic_ms();
        // Stamp the broker-owned namespace onto the commandless report and
        // validate it through the exact strict codec before any drain: a
        // syntactically invalid capture rejects here without touching the
        // engine or the queue, so it can never retire a good queued command.
        // No engine mutation happens in this gate.
        let mut plain_inner = parsed.inner.clone();
        self.stamp_inner(
            &mut plain_inner,
            &parsed,
            observation_seq,
            request_seq,
            None,
            clock,
        );
        let plain_stamped = serde_json::Value::Object(plain_inner).to_string();
        if plain_stamped.len() > WORLD_MAX_REQUEST_BYTES {
            return Err(BrokerError::Oversized);
        }
        let plain_report = match decode_request(&plain_stamped) {
            Ok(report) => report,
            Err(error) => {
                return self.remember_rejection(
                    &parsed,
                    envelope,
                    error.kind,
                    error.message,
                    clock,
                );
            }
        };
        // The gate above validated without mutation; execution below
        // re-decodes the identical bytes (deterministic) before dispatch.
        drop(plain_report);
        if !attempts_clock_valid(&parsed.inner, clock) {
            return self.remember_rejection(
                &parsed,
                envelope,
                "snapshot-invalid",
                "attempt-clock-invalid",
                clock,
            );
        }
        // Drain at most one queued command into this fresh capture.
        let head = self.pending.front().cloned();
        let outcome = match head {
            Some(queued) => self.dispatch_with_command(
                &parsed,
                &queued,
                &plain_stamped,
                observation_seq,
                request_seq,
                clock,
            )?,
            None => match self.run_engine(&plain_stamped)? {
                Ok(encoded) => FreshOutcome::Reply(encoded),
                Err((kind, message)) => FreshOutcome::Rejected { kind, message },
            },
        };
        match outcome {
            FreshOutcome::Rejected { kind, message } => {
                // The facts' own verdict (or the fallback's after a terminally
                // settled drain): bind it for this source and remember it, but
                // touch neither the presentation cache (last accepted
                // projection only) nor the watermark floor, so a failed drain
                // can never wedge or poison the cached full reply. The queue
                // already settled and stays untouched.
                self.remember_rejection(&parsed, envelope, kind, message, clock)
            }
            FreshOutcome::Reply(encoded) => {
                // Staged gesture map installs only on the accepted reply:
                // malformed report fields, the attempt-clock gate, the
                // engine rejection, and every other non-accepted outcome
                // above discarded their staged mutation by reaching this
                // arm selectively.
                self.gesture_map = staged_gesture_map;
                // The actuator lane never carries the terminal result
                // (requester-only even when the actuator is the issuer); the
                // projection cache keeps the snapshot plus presentation only,
                // with write tokens stripped. The renderer-only drop preview
                // is the single exception: when the accepted reply carried a
                // `DragPreview` result, its paint geometry plus subject proof
                // (`rect`, mover-home source, insertion-neighbor window; no
                // hover chain, no result tag, no session revision) rides the
                // projection as `preview` (null otherwise) so a renderer
                // without take-lane authority paints exactly what core
                // published. The expanded bound reply always fits the reply
                // cap; oversize fails closed without caching.
                let preview = drag_preview_of(&encoded);
                let stripped = strip_result(&encoded);
                let with_assoc = inject_gesture_assoc(&stripped, gesture_assoc.as_ref());
                let bound =
                    bind_response(&with_assoc, &parsed.source, parsed.native_invalid, clock);
                if bound.len() > WORLD_MAX_REPLY_BYTES {
                    return Err(BrokerError::Oversized);
                }
                self.presentation = Some(project_reply(&stripped, preview));
                self.watermarks
                    .insert(parsed.adapter.clone(), parsed.native_invalid);
                self.remember_response(&parsed.source, envelope.as_bytes(), &bound);
                Ok(bound)
            }
        }
    }

    /// Bind one in-band rejection for its source, remember it for exact
    /// redelivery replay, and return it. Never touches the presentation
    /// cache, the watermark floor, or queued command state. Oversize fails
    /// closed without remembering.
    fn remember_rejection(
        &mut self,
        parsed: &ParsedCapture,
        envelope: &str,
        kind: &str,
        message: &str,
        clock: u64,
    ) -> Result<String, BrokerError> {
        let bound = self.bound_error(kind, message, parsed, clock);
        if bound.len() > WORLD_MAX_REPLY_BYTES {
            return Err(BrokerError::Oversized);
        }
        self.remember_response(&parsed.source, envelope.as_bytes(), &bound);
        Ok(bound)
    }

    /// Stamp the broker-owned namespace onto one inner observation. The
    /// drained command rides only when `command` is `Some`; otherwise the
    /// report stays observation-only. Pure over the map: no engine contact.
    fn stamp_inner(
        &self,
        inner: &mut serde_json::Map<String, serde_json::Value>,
        parsed: &ParsedCapture,
        observation_seq: u64,
        request_seq: u64,
        command: Option<&QueuedCommand>,
        clock: u64,
    ) {
        inner.insert("owner".to_owned(), json_string(BROKER_OWNER));
        inner.insert("adapter".to_owned(), json_string(&self.incarnation));
        inner.insert("gen".to_owned(), json_string(&self.generation));
        inner.insert("obs".to_owned(), json_string(&observation_seq.to_string()));
        inner.insert("reqSeq".to_owned(), json_string(&request_seq.to_string()));
        inner.insert("req".to_owned(), json_string(&parsed.source));
        match command {
            Some(queued) => {
                inner.insert(
                    "cmdSeq".to_owned(),
                    json_string(&queued.global_seq.to_string()),
                );
                inner.insert("command".to_owned(), queued.command.clone());
            }
            None => {
                inner.insert("cmdSeq".to_owned(), serde_json::Value::Null);
                inner.insert("command".to_owned(), serde_json::Value::Null);
            }
        };
        inner.insert("monotonicMs".to_owned(), json_string(&clock.to_string()));
    }

    /// Dispatch a fresh capture carrying the drained head command. A decode
    /// failure (unreachable after queue-time validation) settles the command
    /// terminally as failed and falls back to the observation-only report. A
    /// semantic engine rejection settles the command terminally as failed
    /// with the explicit core kind/message, then dispatches the same fresh
    /// facts observation-only so the stream never wedges. The command never
    /// re-executes in any lane. The caller binds and remembers the outcome;
    /// this only settles the queue and runs the engine.
    fn dispatch_with_command(
        &mut self,
        parsed: &ParsedCapture,
        queued: &QueuedCommand,
        plain_stamped: &str,
        observation_seq: u64,
        request_seq: u64,
        clock: u64,
    ) -> Result<FreshOutcome, BrokerError> {
        let mut inner = parsed.inner.clone();
        self.stamp_inner(
            &mut inner,
            parsed,
            observation_seq,
            request_seq,
            Some(queued),
            clock,
        );
        let stamped = serde_json::Value::Object(inner).to_string();
        if stamped.len() > WORLD_MAX_REQUEST_BYTES {
            return Err(BrokerError::Oversized);
        }
        let commanded = match decode_request(&stamped) {
            Ok(report) => report,
            Err(error) => {
                self.pop_settle_failed(queued, error.kind, error.message)?;
                return self.dispatch_plain_fallback(plain_stamped);
            }
        };
        match self.engine.handle_report(&commanded) {
            WorldOutcome::Accepted(reply)
            | WorldOutcome::Duplicate(reply)
            | WorldOutcome::CommandConsumed(reply) => {
                let encoded = encode_reply(&reply);
                if encoded.len() > WORLD_MAX_REPLY_BYTES {
                    return Err(BrokerError::Oversized);
                }
                let result = result_of(&encoded);
                self.pop_settle_ready(queued, result)?;
                Ok(FreshOutcome::Reply(encoded))
            }
            WorldOutcome::Rejected { kind, message } => {
                self.pop_settle_failed(queued, kind, message)?;
                self.dispatch_plain_fallback(plain_stamped)
            }
        }
    }

    /// Observation-only follow-up after a terminally settled command.
    /// Rejections here are the facts' own verdict (the queue already settled
    /// and stays untouched); the caller binds and remembers them without
    /// touching the presentation cache.
    fn dispatch_plain_fallback(
        &mut self,
        plain_stamped: &str,
    ) -> Result<FreshOutcome, BrokerError> {
        match self.run_engine(plain_stamped)? {
            Ok(encoded) => Ok(FreshOutcome::Reply(encoded)),
            Err((kind, message)) => Ok(FreshOutcome::Rejected { kind, message }),
        }
    }

    /// Run one validated stamped report through the engine. Outer `Err` is
    /// an oversize encoded reply (fail closed); inner `Err` is a core
    /// rejection with its fixed kind/message (no state changed).
    fn run_engine(
        &mut self,
        stamped: &str,
    ) -> Result<Result<String, (&'static str, &'static str)>, BrokerError> {
        let report = match decode_request(stamped) {
            Ok(report) => report,
            Err(error) => return Ok(Err((error.kind, error.message))),
        };
        match self.engine.handle_report(&report) {
            WorldOutcome::Accepted(reply)
            | WorldOutcome::Duplicate(reply)
            | WorldOutcome::CommandConsumed(reply) => {
                let encoded = encode_reply(&reply);
                if encoded.len() > WORLD_MAX_REPLY_BYTES {
                    return Err(BrokerError::Oversized);
                }
                Ok(Ok(encoded))
            }
            WorldOutcome::Rejected { kind, message } => Ok(Err((kind, message))),
        }
    }

    /// Pop the drained head and settle it terminally as ready with the
    /// reply's terminal result (possibly null when the snapshot alone is
    /// the outcome). An actual head drain advances the wake epoch and
    /// completes a parked waiter while work remains.
    fn pop_settle_ready(
        &mut self,
        queued: &QueuedCommand,
        result: serde_json::Value,
    ) -> Result<(), BrokerError> {
        let key = queued.key();
        self.pending.pop_front();
        let global_seq = queued.global_seq;
        self.insert_settlement(key, Settlement::Ready { global_seq, result });
        self.take_wake_epoch()?;
        self.complete_waiter_if_work();
        Ok(())
    }

    /// Pop the drained head and settle it terminally as failed with the
    /// explicit rejection kind/message. Never a null result masquerading as
    /// success. Like the ready drain, this is an actual head drain: the
    /// wake epoch advances and a parked waiter completes while work
    /// remains.
    fn pop_settle_failed(
        &mut self,
        queued: &QueuedCommand,
        kind: &str,
        message: &str,
    ) -> Result<(), BrokerError> {
        let key = queued.key();
        self.pending.pop_front();
        let global_seq = queued.global_seq;
        self.insert_settlement(
            key,
            Settlement::Failed {
                global_seq,
                kind: kind.to_owned(),
                message: message.to_owned(),
            },
        );
        self.take_wake_epoch()?;
        self.complete_waiter_if_work();
        Ok(())
    }

    /// File one external command. Duplicates at or below the per-sender
    /// per-client high-water never re-execute; new commands take the next
    /// global sequence and wait for the next fresh capture. The `sender` is
    /// the authenticated D-Bus unique name: the client namespace binds it,
    /// so one sender can never move another's high-water. The request's
    /// generation binds before any high-water or queue state is touched: a
    /// stale generation refuses as generation-mismatch, so a delayed call
    /// from a superseded lifetime can never file an old intent into the new
    /// world.
    pub fn queue_command(&mut self, sender: &str, request: &str) -> Result<String, BrokerError> {
        if request.len() > WORLD_MAX_REQUEST_BYTES {
            return Err(BrokerError::Oversized);
        }
        let sender = match valid_sender(sender) {
            Some(sender) => sender,
            None => return Ok(self.error_body("unauthorized", "unauthorized")),
        };
        let (generation, client, client_cmd, command) = match parse_queue_request(request) {
            Ok(parsed) => parsed,
            Err((kind, message)) => return Ok(self.error_body(kind, message)),
        };
        if generation != self.generation {
            return Ok(self.error_body("generation-mismatch", "generation mismatch"));
        }
        if !command_shape_valid(&self.generation, &command) {
            return Ok(self.error_body("snapshot-invalid", "command-invalid"));
        }
        let namespace = (sender.clone(), client.clone());
        let mark = self.highwater.get(&namespace).copied().unwrap_or(0);
        if client_cmd <= mark {
            let key = (sender.clone(), client.clone(), client_cmd.to_string());
            let global = self
                .pending
                .iter()
                .find(|q| q.key() == key)
                .map(|q| q.global_seq)
                .or_else(|| self.settled.get(&key).map(Settlement::global_seq))
                .or_else(|| self.retired.get(&key).map(Settlement::global_seq));
            return Ok(queue_reply(
                "duplicate",
                &self.generation,
                &client,
                &client_cmd.to_string(),
                global,
            ));
        }
        if self.pending.len() >= MAX_PENDING_COMMANDS
            || (self.highwater.len() >= MAX_COMMAND_CLIENTS
                && !self.highwater.contains_key(&namespace))
        {
            return Ok(self.error_body("queue-full", "command queue is full"));
        }
        let global_seq = self.take_command_seq()?;
        let client_cmd_text = client_cmd.to_string();
        self.highwater.insert(namespace, client_cmd);
        self.pending.push_back(QueuedCommand {
            sender,
            client: client.clone(),
            client_cmd: client_cmd_text.clone(),
            global_seq,
            command,
        });
        // A newly queued command advances the wake epoch and completes a
        // parked waiter: the actuator's follow-up capture drains it.
        self.take_wake_epoch()?;
        self.complete_waiter_if_work();
        Ok(queue_reply(
            "queued",
            &self.generation,
            &client,
            &client_cmd_text,
            Some(global_seq),
        ))
    }

    /// Collect one settled terminal result. Pending commands report
    /// `pending`; settled commands report `ready` (with the core result) or
    /// `failed` (with the explicit rejection kind/message), re-served
    /// verbatim while the bounded memory holds them so a lost take reply
    /// never loses the outcome and replays never touch effects; forgotten
    /// keys report `duplicate` (never re-execute); unknown keys reject. The
    /// `sender` binds the namespace: cross-sender collection rejects as
    /// unknown. The request's generation binds before any settlement or
    /// high-water access: a stale generation refuses as
    /// generation-mismatch, so a delayed take from a superseded lifetime
    /// can never collect (or clear) another world's outcome.
    pub fn take_result(&mut self, sender: &str, request: &str) -> Result<String, BrokerError> {
        if request.len() > WORLD_MAX_REQUEST_BYTES {
            return Err(BrokerError::Oversized);
        }
        let sender = match valid_sender(sender) {
            Some(sender) => sender,
            None => return Ok(self.error_body("unauthorized", "unauthorized")),
        };
        let (generation, client, client_cmd) = match parse_take_request(request) {
            Ok(parsed) => parsed,
            Err((kind, message)) => return Ok(self.error_body(kind, message)),
        };
        if generation != self.generation {
            return Ok(self.error_body("generation-mismatch", "generation mismatch"));
        }
        let key = (sender.clone(), client.clone(), client_cmd.to_string());
        if self.pending.iter().any(|q| q.key() == key) {
            return Ok(take_pending_reply(&self.generation));
        }
        if let Some(settled) = self.settled.remove(&key) {
            self.settled_order.retain(|k| k != &key);
            let reply = take_settlement_reply(&self.generation, &settled);
            self.retire_settlement(key, settled);
            return Ok(reply);
        }
        if let Some(retired) = self.retired.get(&key) {
            return Ok(take_settlement_reply(&self.generation, retired));
        }
        let mark = self.highwater.get(&(sender, client)).copied().unwrap_or(0);
        if client_cmd <= mark {
            Ok(take_duplicate_reply(&self.generation))
        } else {
            Ok(self.error_body("unknown-command", "unknown command"))
        }
    }

    /// Read the cached accepted presentation projection (snapshot plus
    /// presentation plus the renderer-only drop preview, window write tokens
    /// stripped). Never carries transfers, effects, terminal results,
    /// settlements, or warp desires; the preview is the single carried
    /// drop-preview geometry (`rect`, mover-home `sourceOut`/`sourceWs`,
    /// insertion-neighbor `win`; null when the cached reply carried none).
    pub fn read_presentation(&self, request: &str) -> Result<String, BrokerError> {
        if request.len() > WORLD_MAX_REQUEST_BYTES {
            return Err(BrokerError::Oversized);
        }
        let req = match parse_read_request(request) {
            Ok(req) => req,
            Err((kind, message)) => return Ok(self.error_body(kind, message)),
        };
        let Some(cached) = &self.presentation else {
            return Ok(self.error_body("not-ready", "no accepted capture"));
        };
        Ok(project_read(cached, &self.generation, &req))
    }

    fn take_observation_seq(&mut self) -> Result<u64, BrokerError> {
        let seq = self.next_observation_seq;
        self.next_observation_seq = seq.checked_add(1).ok_or(BrokerError::Rotate)?;
        Ok(seq)
    }

    fn take_request_seq(&mut self) -> Result<u64, BrokerError> {
        let seq = self.next_request_seq;
        self.next_request_seq = seq.checked_add(1).ok_or(BrokerError::Rotate)?;
        Ok(seq)
    }

    fn take_command_seq(&mut self) -> Result<u64, BrokerError> {
        let seq = self.next_command_seq;
        self.next_command_seq = seq.checked_add(1).ok_or(BrokerError::Rotate)?;
        Ok(seq)
    }

    /// Advance the wake event epoch. Counter exhaustion rotates like every
    /// other broker sequence (the caller fails closed; issuers
    /// re-discover).
    fn take_wake_epoch(&mut self) -> Result<u64, BrokerError> {
        let epoch = self.wake_epoch.checked_add(1).ok_or(BrokerError::Rotate)?;
        self.wake_epoch = epoch;
        Ok(epoch)
    }

    /// Complete the parked waiter when its cursor is behind the epoch and
    /// work is still pending. The reply cursor is the epoch itself, so it
    /// only advances on a delivered wake; a lease expiry stores no reply
    /// and a lost reply re-uses the old cursor. No work pending means no
    /// wake even past the epoch (a drained queue leaves nothing to
    /// drain).
    fn complete_waiter_if_work(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let Some(waiter) = self.waiter.take() else {
            return;
        };
        if self.wake_epoch <= waiter.cursor {
            self.waiter = Some(waiter);
            return;
        }
        let reply = format!(
            "{{\"v\":2,\"outcome\":\"wake\",\"gen\":\"{}\",\"cursor\":\"{}\"}}",
            self.generation, self.wake_epoch
        );
        self.wait_ready = Some((waiter.nonce.clone(), reply));
        if let Some(waker) = waiter.waker {
            waker.wake();
        }
    }

    /// Evaluate one wait registration: bind the serving generation and the
    /// discovery-bound producer lifetime, answer immediately when the epoch
    /// advanced past the cursor with work pending, or park. `Err` is an
    /// in-band rejection body (stale generation or unknown lifetime).
    pub fn wait_immediate(
        &mut self,
        lifetime: &str,
        generation: &str,
        cursor: u64,
    ) -> Result<Option<String>, String> {
        if generation != self.generation {
            return Err(self.error_body("generation-mismatch", "generation mismatch"));
        }
        if self.capture_lifetime.as_deref() != Some(lifetime) {
            return Err(self.error_body("unknown-adapter", "unknown adapter"));
        }
        if self.wake_epoch > cursor && !self.pending.is_empty() {
            return Ok(Some(self.wait_wake_body()));
        }
        Ok(None)
    }

    /// Park one waiter, superseding any parked one: the previous waker
    /// fires so its call answers superseded instead of hanging. Call only
    /// after `wait_immediate` returned `Ok(None)` under the same lock.
    /// Tests park with `None` and collect through `take_wait_ready`. A
    /// stale uncollected ready under a different nonce is dropped here so
    /// the single bounded slot never wedges on an orphaned reply.
    pub fn wait_park(&mut self, nonce: &str, cursor: u64, waker: Option<std::task::Waker>) {
        if self
            .wait_ready
            .as_ref()
            .is_some_and(|(ready_nonce, _)| ready_nonce != nonce)
        {
            self.wait_ready = None;
        }
        if let Some(previous) = self.waiter.replace(BrokerWaiter {
            nonce: nonce.to_owned(),
            cursor,
            waker,
        }) && let Some(waker) = previous.waker
        {
            waker.wake();
        }
    }

    /// Take the parked waiter for a rotation/poison handoff. The caller
    /// replaces the broker first, then completes the waiter against the
    /// fresh generation so it wakes immediately instead of sleeping out
    /// its lease.
    pub(crate) fn take_parked_waiter(&mut self) -> Option<BrokerWaiter> {
        self.waiter.take()
    }

    /// Complete a displaced waiter against this (fresh) broker: store a
    /// generation-mismatch reply under its nonce and wake it immediately.
    /// A stale generation answers mismatch, never superseded.
    pub(crate) fn complete_displaced(&mut self, waiter: BrokerWaiter) {
        let reply = self.error_body("generation-mismatch", "generation mismatch");
        self.wait_ready = Some((waiter.nonce.clone(), reply));
        if let Some(waker) = waiter.waker {
            waker.wake();
        }
    }

    /// Collect the delivered wake reply for exactly this waiter nonce.
    pub fn take_wait_ready(&mut self, nonce: &str) -> Option<String> {
        match &self.wait_ready {
            Some((ready_nonce, _)) if ready_nonce == nonce => {
                self.wait_ready.take().map(|(_, reply)| reply)
            }
            _ => None,
        }
    }

    /// Current parked waiter nonce, if any.
    #[must_use]
    pub fn waiter_nonce(&self) -> Option<&str> {
        self.waiter.as_ref().map(|waiter| waiter.nonce.as_str())
    }

    /// Expire one parked waiter at the server lease: removes it only when
    /// the nonce still owns the slot and returns its unchanged cursor, so
    /// the lease reply can never consume unseen work.
    pub fn expire_wait(&mut self, nonce: &str) -> Option<u64> {
        if self
            .waiter
            .as_ref()
            .is_some_and(|waiter| waiter.nonce == nonce)
        {
            let waiter = self.waiter.take().expect("waiter is present");
            return Some(waiter.cursor);
        }
        None
    }

    /// Immediate wake body: the cursor is the current epoch (pending work
    /// is already established by the caller).
    fn wait_wake_body(&self) -> String {
        format!(
            "{{\"v\":2,\"outcome\":\"wake\",\"gen\":\"{}\",\"cursor\":\"{}\"}}",
            self.generation, self.wake_epoch
        )
    }

    /// Idle lease body: the cursor is unchanged, so re-arming with it can
    /// never skip unseen work.
    #[must_use]
    pub fn wait_lease_body(&self, cursor: u64) -> String {
        format!(
            "{{\"v\":2,\"outcome\":\"lease\",\"gen\":\"{}\",\"cursor\":\"{cursor}\"}}",
            self.generation
        )
    }

    /// Superseded body for a waiter replaced by a newer registration.
    #[must_use]
    pub fn wait_superseded_body(&self) -> String {
        format!(
            "{{\"v\":2,\"outcome\":\"superseded\",\"gen\":\"{}\"}}",
            self.generation
        )
    }

    /// Insert one terminal settlement under its sender-bound key. Bounded:
    /// oldest evicted entries are forgotten (duplicates report `duplicate`,
    /// never re-execute).
    fn insert_settlement(&mut self, key: (String, String, String), settlement: Settlement) {
        if !self.settled.contains_key(&key) {
            self.settled_order.push_back(key.clone());
        }
        self.settled.insert(key, settlement);
        while self.settled.len() > MAX_SETTLED_RESULTS {
            if let Some(oldest) = self.settled_order.pop_front() {
                self.settled.remove(&oldest);
            } else {
                break;
            }
        }
    }

    /// Move one collected settlement to the retired map, re-served verbatim
    /// on repeat takes so a lost take reply never loses the outcome.
    /// Bounded like the settled map.
    fn retire_settlement(&mut self, key: (String, String, String), settlement: Settlement) {
        if !self.retired.contains_key(&key) {
            self.retired_order.push_back(key.clone());
        }
        self.retired.insert(key, settlement);
        while self.retired.len() > MAX_SETTLED_RESULTS {
            if let Some(oldest) = self.retired_order.pop_front() {
                self.retired.remove(&oldest);
            } else {
                break;
            }
        }
    }

    /// Remember one per-source bound capture outcome (accepted bound reply
    /// or bound in-band rejection) keyed by exact envelope bytes. Bounded
    /// oldest-first eviction: evicted sources lose replay and refuse as
    /// stale through the sequence floor, never re-dispatching forgotten
    /// facts.
    fn remember_response(&mut self, source: &str, bytes: &[u8], bound: &str) {
        if !self.responses.contains_key(source) {
            self.response_order.push_back(source.to_owned());
        }
        self.responses
            .insert(source.to_owned(), (bytes.to_vec(), bound.to_owned()));
        while self.responses.len() > MAX_REMEMBERED_SOURCES {
            if let Some(oldest) = self.response_order.pop_front() {
                self.responses.remove(&oldest);
            } else {
                break;
            }
        }
    }
}

impl Default for WorldBroker {
    fn default() -> Self {
        Self::new()
    }
}

/// Strict-parse failure: the source watermark binding when the outer
/// envelope parsed that far (empty/zero when even the envelope is broken,
/// which never matches a real source).
#[derive(Debug)]
struct CaptureFailure {
    source: String,
    native_invalid: u64,
    kind: &'static str,
    message: &'static str,
}

impl CaptureFailure {
    fn blind(kind: &'static str, message: &'static str) -> Self {
        Self {
            source: String::new(),
            native_invalid: 0,
            kind,
            message,
        }
    }
}

/// Strict-parsed capture envelope: discovery-bound producer lifetime,
/// generation binding, per-capture source correlation plus its per-lifetime
/// monotonic sequence, native invalidation watermark, and the broker-key-
/// free inner observation object.
#[derive(Debug)]
struct ParsedCapture {
    adapter: String,
    generation: String,
    source: String,
    source_seq: u64,
    native_invalid: u64,
    inner: serde_json::Map<String, serde_json::Value>,
}

/// Generation id alphabet (lowercase/digit/dash, bounded): mirrors the core
/// gate so the broker can bind generations without touching the engine.
fn is_generation_token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Strict v2 capture envelope parse: exact keys
/// (`v,adapter,gen,source,sourceSeq,nativeInvalid,report`), v2 versions in
/// both layers, opaque lifetime, generation alphabet, opaque source
/// correlation, canonical per-lifetime source sequence, canonical watermark,
/// object report, and no broker-owned key inside the report.
/// Outer-envelope failures bind empty/zero; inner-report failures still bind
/// the parsed source watermark so the actuator can fence the rejection.
fn parse_capture_envelope(envelope: &str) -> Result<ParsedCapture, CaptureFailure> {
    let failure = |kind: &'static str, message: &'static str| CaptureFailure::blind(kind, message);
    let value: serde_json::Value =
        serde_json::from_str(envelope).map_err(|_| failure("malformed", "request is malformed"))?;
    let map = value
        .as_object()
        .ok_or_else(|| failure("malformed", "request is malformed"))?;
    if map.len() != 7
        || !map.contains_key("v")
        || !map.contains_key("adapter")
        || !map.contains_key("gen")
        || !map.contains_key("source")
        || !map.contains_key("sourceSeq")
        || !map.contains_key("nativeInvalid")
        || !map.contains_key("report")
    {
        if map.keys().any(|k| {
            k != "v"
                && k != "adapter"
                && k != "gen"
                && k != "source"
                && k != "sourceSeq"
                && k != "nativeInvalid"
                && k != "report"
        }) {
            return Err(failure(
                "unknown-field",
                "request contains an unknown field",
            ));
        }
        return Err(failure("malformed", "request is malformed"));
    }
    if map.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(BROKER_VERSION)) {
        return Err(failure(
            "unsupported-version",
            "unsupported contract version",
        ));
    }
    let adapter = map
        .get("adapter")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_valid_adapter_id(s))
        .ok_or_else(|| failure("unknown-value", "request contains an unknown value"))?;
    let generation = map
        .get("gen")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_generation_token(s))
        .ok_or_else(|| failure("unknown-value", "request contains an unknown value"))?;
    let source = map
        .get("source")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_valid_request_id(s))
        .ok_or_else(|| failure("unknown-value", "request contains an unknown value"))?;
    let source_seq = map
        .get("sourceSeq")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_canonical_u64(s))
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| failure("unknown-value", "request contains an unknown value"))?;
    let watermark = map
        .get("nativeInvalid")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_canonical_u64(s))
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| failure("unknown-value", "request contains an unknown value"))?;
    let bound = |kind: &'static str, message: &'static str| CaptureFailure {
        source: source.to_owned(),
        native_invalid: watermark,
        kind,
        message,
    };
    let inner = map
        .get("report")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| bound("malformed", "request is malformed"))?;
    if inner.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(BROKER_VERSION)) {
        return Err(bound("unsupported-version", "unsupported contract version"));
    }
    if BROKER_KEYS.iter().any(|k| inner.contains_key(*k)) {
        return Err(bound("unknown-field", "request contains an unknown field"));
    }
    Ok(ParsedCapture {
        adapter: adapter.to_owned(),
        generation: generation.to_owned(),
        source: source.to_owned(),
        source_seq,
        native_invalid: watermark,
        inner: inner.clone(),
    })
}

/// Strict queue-request parse: exact keys, v2, serving generation binding,
/// opaque client, canonical per-client command id, object command.
fn parse_queue_request(
    request: &str,
) -> Result<(String, String, u64, serde_json::Value), (&'static str, &'static str)> {
    let value: serde_json::Value =
        serde_json::from_str(request).map_err(|_| ("malformed", "request is malformed"))?;
    let map = value
        .as_object()
        .ok_or(("malformed", "request is malformed"))?;
    if map.len() != 5
        || !map.contains_key("v")
        || !map.contains_key("gen")
        || !map.contains_key("client")
        || !map.contains_key("clientCmd")
        || !map.contains_key("command")
    {
        if map
            .keys()
            .any(|k| k != "v" && k != "gen" && k != "client" && k != "clientCmd" && k != "command")
        {
            return Err(("unknown-field", "request contains an unknown field"));
        }
        return Err(("malformed", "request is malformed"));
    }
    if map.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(BROKER_VERSION)) {
        return Err(("unsupported-version", "unsupported contract version"));
    }
    let generation = map
        .get("gen")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_generation_token(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    let client = map
        .get("client")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_valid_adapter_id(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    let client_cmd = map
        .get("clientCmd")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_canonical_u64(s))
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    let command = map
        .get("command")
        .ok_or(("malformed", "request is malformed"))?;
    if !command.is_object() {
        return Err(("unknown-value", "request contains an unknown value"));
    }
    Ok((
        generation.to_owned(),
        client.to_owned(),
        client_cmd,
        command.clone(),
    ))
}

/// Strict take-request parse: exact keys, v2, serving generation binding,
/// opaque client, canonical id.
fn parse_take_request(
    request: &str,
) -> Result<(String, String, u64), (&'static str, &'static str)> {
    let value: serde_json::Value =
        serde_json::from_str(request).map_err(|_| ("malformed", "request is malformed"))?;
    let map = value
        .as_object()
        .ok_or(("malformed", "request is malformed"))?;
    if map.len() != 4
        || !map.contains_key("v")
        || !map.contains_key("gen")
        || !map.contains_key("client")
        || !map.contains_key("clientCmd")
    {
        if map
            .keys()
            .any(|k| k != "v" && k != "gen" && k != "client" && k != "clientCmd")
        {
            return Err(("unknown-field", "request contains an unknown field"));
        }
        return Err(("malformed", "request is malformed"));
    }
    if map.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(BROKER_VERSION)) {
        return Err(("unsupported-version", "unsupported contract version"));
    }
    let generation = map
        .get("gen")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_generation_token(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    let client = map
        .get("client")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_valid_adapter_id(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    let client_cmd = map
        .get("clientCmd")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_canonical_u64(s))
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    Ok((generation.to_owned(), client.to_owned(), client_cmd))
}

/// Strict wait-request parse: exact keys (`v,gen,adapter,nonce,cursor`),
/// v2, serving-generation binding, discovery-bound producer lifetime,
/// opaque waiter correlation, canonical cursor.
pub fn parse_wait_request(
    request: &str,
) -> Result<(String, String, String, u64), (&'static str, &'static str)> {
    let value: serde_json::Value =
        serde_json::from_str(request).map_err(|_| ("malformed", "request is malformed"))?;
    let map = value
        .as_object()
        .ok_or(("malformed", "request is malformed"))?;
    if map.len() != 5
        || !map.contains_key("v")
        || !map.contains_key("gen")
        || !map.contains_key("adapter")
        || !map.contains_key("nonce")
        || !map.contains_key("cursor")
    {
        if map
            .keys()
            .any(|k| k != "v" && k != "gen" && k != "adapter" && k != "nonce" && k != "cursor")
        {
            return Err(("unknown-field", "request contains an unknown field"));
        }
        return Err(("malformed", "request is malformed"));
    }
    if map.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(BROKER_VERSION)) {
        return Err(("unsupported-version", "unsupported contract version"));
    }
    let generation = map
        .get("gen")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_generation_token(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    let adapter = map
        .get("adapter")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_valid_adapter_id(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    let nonce = map
        .get("nonce")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_valid_request_id(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    let cursor = map
        .get("cursor")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_canonical_u64(s))
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    Ok((
        generation.to_owned(),
        adapter.to_owned(),
        nonce.to_owned(),
        cursor,
    ))
}

/// Strict read-request parse: exact keys, v2, opaque correlation.
fn parse_read_request(request: &str) -> Result<String, (&'static str, &'static str)> {
    let value: serde_json::Value =
        serde_json::from_str(request).map_err(|_| ("malformed", "request is malformed"))?;
    let map = value
        .as_object()
        .ok_or(("malformed", "request is malformed"))?;
    if map.len() != 2 || !map.contains_key("v") || !map.contains_key("req") {
        if map.keys().any(|k| k != "v" && k != "req") {
            return Err(("unknown-field", "request contains an unknown field"));
        }
        return Err(("malformed", "request is malformed"));
    }
    if map.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(BROKER_VERSION)) {
        return Err(("unsupported-version", "unsupported contract version"));
    }
    let req = map
        .get("req")
        .and_then(serde_json::Value::as_str)
        .filter(|s| is_valid_request_id(s))
        .ok_or(("unknown-value", "request contains an unknown value"))?;
    Ok(req.to_owned())
}

/// Validate one queued command through the exact strict codec by embedding
/// it in a fixed minimal observation (or its commandless twin as the
/// harness). No core changes: the codec owns the whole command vocabulary,
/// so queue-time validation can never drift from dispatch-time decoding.
/// Decoding is inventory-independent, so inventory races (a window the
/// world outgrew between queue and dispatch) resolve at dispatch with
/// bounded retries, never here. `false` fails the command closed at queue
/// time instead of poisoning a later actuator capture.
fn command_shape_valid(generation: &str, command: &serde_json::Value) -> bool {
    // The harness (commandless twin) must decode: otherwise the probe
    // itself is broken, and failing closed is the only safe answer.
    let mut harness_inner = minimal_skeleton();
    stamp_probe(&mut harness_inner, generation, None);
    if decode_request(&serde_json::Value::Object(harness_inner).to_string()).is_err() {
        return false;
    }
    let mut probe_inner = minimal_skeleton();
    stamp_probe(&mut probe_inner, generation, Some(command));
    decode_request(&serde_json::Value::Object(probe_inner).to_string()).is_ok()
}

/// Minimal observation skeleton for pre-first-capture validation: no
/// inventory, so only inventory-free commands queue.
fn minimal_skeleton() -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(
        "{\"v\":2,\"mode\":\"per-output-local\",\
          \"screens\":[],\"windows\":[],\"views\":[],\"closed\":[],\
          \"hostOwned\":false,\"equalCount\":false,\"native\":[],\
          \"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\
          \"defaultTiled\":true,\"focused\":\"\",\
          \"complete\":false,\"gesture\":false,\"switchTarget\":\"cursor-output\",\
          \"freshPlacement\":\"cursor-output-last-focused\",\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[],\"focusFollowsCursor\":false,\"cursorWarp\":false,\
          \"presentation\":{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
          \"fullscreen\":false,\"cloaked\":false,\"hiddenWorkspace\":false,\
          \"shell\":false,\"frame\":null,\"borderEnabled\":false,\
          \"underlayEnabled\":false,\"borderGap\":0,\"borderWidth\":0,\
          \"underlayExtension\":0,\"chordHeld\":false,\"moveWindow\":\"\",\
          \"paused\":false,\"pagerOrder\":[],\"pagerFrames\":[]}}",
    )
    .expect("skeleton is a JSON object")
    .as_object()
    .expect("skeleton is an object")
    .clone()
}

/// Stamp one probe report with the broker-owned namespace plus the optional
/// candidate command.
fn stamp_probe(
    inner: &mut serde_json::Map<String, serde_json::Value>,
    generation: &str,
    command: Option<&serde_json::Value>,
) {
    inner.insert("owner".to_owned(), json_string(BROKER_OWNER));
    inner.insert("adapter".to_owned(), json_string("broker-probe"));
    inner.insert("gen".to_owned(), json_string(generation));
    inner.insert("obs".to_owned(), json_string("1"));
    inner.insert("reqSeq".to_owned(), json_string("1"));
    inner.insert("req".to_owned(), json_string("probe-1"));
    inner.insert("monotonicMs".to_owned(), json_string("1"));
    match command {
        Some(command) => {
            inner.insert("cmdSeq".to_owned(), json_string("1"));
            inner.insert("command".to_owned(), command.clone());
        }
        None => {
            inner.insert("cmdSeq".to_owned(), serde_json::Value::Null);
            inner.insert("command".to_owned(), serde_json::Value::Null);
        }
    }
}

/// Extend a core reply or in-band error object with the source watermark
/// and dispatch-clock binding. Pure over the strings; never echoes payload
/// beyond the correlation, watermark, and clock.
fn bind_response(reply: &str, source: &str, watermark: u64, clock: u64) -> String {
    let mut value: serde_json::Value = serde_json::from_str(reply).unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.insert("source".to_owned(), json_string(source));
        map.insert(
            "nativeInvalid".to_owned(),
            json_string(&watermark.to_string()),
        );
        map.insert("clock".to_owned(), json_string(&clock.to_string()));
        // Broker-owned bound actuator association defaults to null here;
        // the accepted-reply path overwrites it via `inject_gesture_assoc`
        // before binding. Rejections and errors always carry null (never a
        // fabricated binding), so the script can require the association
        // without a separate absence proof.
        if !map.contains_key("gestureAssoc") {
            map.insert("gestureAssoc".to_owned(), serde_json::Value::Null);
        }
    }
    value.to_string()
}

/// Attach the broker-only bound actuator association for the actual
/// normalized ingress (`{instance, serial, coreId}`) to one stripped
/// accepted reply, or an explicit null when the lane omitted. The key is
/// broker-owned (never core/protocol): the exact response replay caches
/// it verbatim, so an unrelated settled id can never alias the current
/// ingress. Pure over the strings.
fn inject_gesture_assoc(reply: &str, assoc: Option<&crate::kde_gesture::GestureAssoc>) -> String {
    let mut value: serde_json::Value = serde_json::from_str(reply).unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        match assoc {
            Some(a) => {
                map.insert(
                    "gestureAssoc".to_owned(),
                    serde_json::json!({
                        "instance": a.instance,
                        "serial": a.serial,
                        "coreId": a.core_id.to_string(),
                    }),
                );
            }
            None => {
                map.insert("gestureAssoc".to_owned(), serde_json::Value::Null);
            }
        }
    }
    value.to_string()
}

/// Null the terminal result lane of one accepted reply for the actuator
/// response. The result is requester-only even when the actuator is the
/// issuer; the issuer collects it via take.
fn strip_result(encoded: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(encoded).unwrap_or_default();
    if let Some(map) = value.as_object_mut()
        && map.contains_key("result")
    {
        map.insert("result".to_owned(), serde_json::Value::Null);
    }
    value.to_string()
}

/// Extract the terminal result value of one encoded reply for
/// requester-only settlement.
fn result_of(encoded: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(encoded)
        .ok()
        .and_then(|v| v.get("result").cloned())
        .unwrap_or(serde_json::Value::Null)
}

/// Foreign-epoch guard over the mechanical attempt lanes: every present
/// canonical `attemptMs` must fit the dispatch clock. Malformed stamps are
/// the codec's business; only comparable stamps gate here. Never adjusts or
/// manufactures evidence.
fn attempts_clock_valid(inner: &serde_json::Map<String, serde_json::Value>, clock: u64) -> bool {
    for lane in [
        "frameAttempts",
        "memberAttempts",
        "viewAttempts",
        "clearAttempts",
        "creationAttempts",
    ] {
        let Some(items) = inner.get(lane).and_then(|v| v.as_array()) else {
            continue;
        };
        for item in items {
            let Some(stamp) = item.get("attemptMs").and_then(|v| v.as_str()) else {
                continue;
            };
            if !is_canonical_u64(stamp) {
                continue;
            }
            let Ok(value) = stamp.parse::<u64>() else {
                continue;
            };
            if value > clock {
                return false;
            }
        }
    }
    true
}

fn queue_reply(
    outcome: &str,
    generation: &str,
    client: &str,
    client_cmd: &str,
    global: Option<u64>,
) -> String {
    let global_json = match global {
        Some(seq) => format!("\"{seq}\""),
        None => "null".to_owned(),
    };
    format!(
        "{{\"v\":2,\"outcome\":\"{outcome}\",\"gen\":\"{generation}\",\
          \"client\":\"{client}\",\"clientCmd\":\"{client_cmd}\",\
          \"globalCmd\":{global_json}}}"
    )
}

fn take_pending_reply(generation: &str) -> String {
    format!("{{\"v\":2,\"outcome\":\"pending\",\"gen\":\"{generation}\"}}")
}

/// Terminal take reply for one settlement: `ready` with the core result
/// (possibly null when the snapshot alone is the outcome), or `failed`
/// with the explicit rejection kind/message. Never effects, never replay.
fn take_settlement_reply(generation: &str, settlement: &Settlement) -> String {
    match settlement {
        Settlement::Ready { global_seq, result } => format!(
            "{{\"v\":2,\"outcome\":\"ready\",\"gen\":\"{generation}\",\
              \"globalCmd\":\"{global_seq}\",\"result\":{result}}}"
        ),
        Settlement::Failed {
            global_seq,
            kind,
            message,
        } => format!(
            "{{\"v\":2,\"outcome\":\"failed\",\"gen\":\"{generation}\",\
              \"globalCmd\":\"{global_seq}\",\"kind\":\"{kind}\",\"message\":\"{message}\"}}"
        ),
    }
}

fn take_duplicate_reply(generation: &str) -> String {
    format!("{{\"v\":2,\"outcome\":\"duplicate\",\"gen\":\"{generation}\"}}")
}

/// Renderer-only drop preview of one accepted reply: the paint geometry
/// plus the subject proof (`rect`, the mover-home `sourceOut`/`sourceWs`,
/// and the insertion-neighbor `win`) when the reply carried a `DragPreview`
/// result, else `None`. Pure over the encoded reply; never touches the
/// engine. Issuer correlation and core provenance (the `op` tag, the
/// `hover` chain, the session `rev`, the resolution `domainOut`/`domainWs`)
/// never ride: the subscriber paints from these fields plus the cached
/// snapshot/scopes/presentation, vets staleness through the projection
/// generation/revision, and applies live hide-only vetoes at dispatch.
/// Malformed shapes fail closed to `None`, never a fabricated preview.
fn drag_preview_of(encoded: &str) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(encoded).ok()?;
    let result = value.get("result")?;
    if result.get("op").and_then(serde_json::Value::as_str) != Some("drag-preview") {
        return None;
    }
    let rect = result.get("rect")?;
    if !rect
        .as_array()
        .is_some_and(|items| items.len() == 4 && items.iter().all(|item| item.as_i64().is_some()))
    {
        return None;
    }
    let source_out = result
        .get("sourceOut")
        .and_then(serde_json::Value::as_str)?;
    let source_ws = result.get("sourceWs").and_then(serde_json::Value::as_str)?;
    let win = result.get("win").and_then(serde_json::Value::as_str)?;
    Some(serde_json::json!({
        "rect": rect,
        "sourceOut": source_out,
        "sourceWs": source_ws,
        "win": win,
    }))
}

/// Project one accepted reply for the presentation cache: snapshot plus
/// presentation plus the renderer-only drop preview, with the native write
/// tokens (`frame_op`, `clear_op`) stripped from every window row.
/// One-shot lanes (result, transfers, effects, settlements, warp) never
/// enter except through the carried `preview` (paint `rect`, mover-home
/// source, insertion-neighbor window; null when no drop preview was
/// published). The `gen`/`req` binding is added at read time.
fn project_reply(encoded: &str, preview: Option<serde_json::Value>) -> String {
    let value: serde_json::Value = serde_json::from_str(encoded).unwrap_or_default();
    let get = |key: &str| value.get(key).cloned().unwrap_or(serde_json::Value::Null);
    let mut windows = get("windows");
    if let Some(rows) = windows.as_array_mut() {
        for row in rows {
            if let Some(map) = row.as_object_mut() {
                map.remove("frame_op");
                map.remove("clear_op");
            }
        }
    }
    serde_json::json!({
        "v": BROKER_VERSION,
        "outcome": "presentation",
        "rev": get("rev"),
        "obs": get("obs"),
        "scopes": get("scopes"),
        "workspaces": get("workspaces"),
        "outputs": get("outputs"),
        "windows": windows,
        "presentation": get("presentation"),
        "preview": preview.unwrap_or(serde_json::Value::Null),
    })
    .to_string()
}

/// Serve one cached projection to a subscriber: bind the serving
/// generation and the read correlation.
fn project_read(cached: &str, generation: &str, req: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(cached).unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.insert("gen".to_owned(), json_string(generation));
        map.insert("req".to_owned(), json_string(req));
    }
    value.to_string()
}

fn json_string(value: &str) -> serde_json::Value {
    serde_json::Value::String(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incarnation_is_opaque_and_unique() {
        let first = mint_broker_incarnation();
        let second = mint_broker_incarnation();
        assert_ne!(first, second);
        assert!(is_valid_adapter_id(&first));
    }

    #[test]
    fn canonical_u64_mirrors_the_codec_gate() {
        assert!(is_canonical_u64("0"));
        assert!(is_canonical_u64("7"));
        assert!(is_canonical_u64("18446744073709551615"));
        for bad in [
            "",
            "00",
            "01",
            "-1",
            "1.0",
            "18446744073709551616",
            " 1",
            "1 ",
        ] {
            assert!(!is_canonical_u64(bad), "{bad}");
        }
        assert!(!is_canonical_u64("1".repeat(21).as_str()));
    }

    #[test]
    fn capture_envelope_rejects_broker_keys_and_v1() {
        let inner = r#"{"v":2,"obs":"1"}"#;
        let with_owner = format!(
            r#"{{"v":2,"adapter":"a-1","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{inner}}}"#
        );
        // `obs` inside the report is broker-owned, but the outer envelope
        // parsed, so the rejection still binds the source watermark.
        let failure = parse_capture_envelope(&with_owner).expect_err("broker keys refuse");
        assert_eq!(failure.kind, "unknown-field");
        assert_eq!(failure.message, "request contains an unknown field");
        assert_eq!(failure.source, "src-1");
        assert_eq!(failure.native_invalid, 3);
        let v1 = r#"{"v":1,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":2}}"#;
        let failure = parse_capture_envelope(v1).expect_err("v1 refuses");
        assert_eq!(failure.kind, "unsupported-version");
        assert_eq!(failure.source, "");
        let bad_watermark = r#"{"v":2,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"03","report":{"v":2}}"#;
        let failure = parse_capture_envelope(bad_watermark).expect_err("watermark refuses");
        assert_eq!(failure.kind, "unknown-value");
        assert_eq!(failure.source, "");
        // The per-lifetime source sequence is mandatory and canonical: a
        // missing, non-canonical, or overflowing sequence refuses as a
        // value, never as a match.
        let no_seq = r#"{"v":2,"adapter":"a","gen":"w-1","source":"src-1","nativeInvalid":"3","report":{"v":2}}"#;
        assert_eq!(
            parse_capture_envelope(no_seq)
                .expect_err("sourceSeq required")
                .kind,
            "malformed"
        );
        let bad_seq = r#"{"v":2,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"01","nativeInvalid":"3","report":{"v":2}}"#;
        assert_eq!(
            parse_capture_envelope(bad_seq)
                .expect_err("sequence canonical")
                .kind,
            "unknown-value"
        );
        // Generation and adapter are mandatory envelope bindings now.
        let no_gen = r#"{"v":2,"adapter":"a","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":2}}"#;
        assert_eq!(
            parse_capture_envelope(no_gen)
                .expect_err("gen required")
                .kind,
            "malformed"
        );
        let no_adapter = r#"{"v":2,"gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":2}}"#;
        assert_eq!(
            parse_capture_envelope(no_adapter)
                .expect_err("adapter required")
                .kind,
            "malformed"
        );
    }

    #[test]
    fn sender_namespace_rejects_non_unique_names() {
        assert!(valid_sender(":1.42").is_some());
        assert!(valid_sender("").is_none());
        assert!(valid_sender("panel-1").is_some());
        assert!(valid_sender("has space").is_none());
        assert!(valid_sender(&"x".repeat(129)).is_none());
    }

    #[test]
    fn queue_and_take_bind_the_serving_generation() {
        // Strict v2: the generation is mandatory and token-shaped on both
        // lanes; a missing key is malformed, a bad token an unknown value.
        let no_gen_queue = "{\"v\":2,\"client\":\"panel-1\",\"clientCmd\":\"1\",\
            \"command\":{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}";
        assert_eq!(
            parse_queue_request(no_gen_queue)
                .expect_err("queue gen required")
                .0,
            "malformed"
        );
        let bad_gen_queue = "{\"v\":2,\"gen\":\"GEN STALE!\",\"client\":\"panel-1\",\
            \"clientCmd\":\"1\",\"command\":{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}";
        assert_eq!(
            parse_queue_request(bad_gen_queue)
                .expect_err("queue gen token")
                .0,
            "unknown-value"
        );
        let no_gen_take = "{\"v\":2,\"client\":\"panel-1\",\"clientCmd\":\"1\"}";
        assert_eq!(
            parse_take_request(no_gen_take)
                .expect_err("take gen required")
                .0,
            "malformed"
        );
        let bad_gen_take = "{\"v\":2,\"gen\":\"\",\"client\":\"panel-1\",\"clientCmd\":\"1\"}";
        assert_eq!(
            parse_take_request(bad_gen_take)
                .expect_err("take gen token")
                .0,
            "unknown-value"
        );
    }

    #[test]
    fn stale_queue_and_take_refuse_before_touching_state() {
        // A delayed old-generation queue/take after a lifetime rotation
        // refuses as generation-mismatch before any high-water or queue
        // access: no old intent files, and the new namespace starts clean.
        let mut broker = WorldBroker::new();
        let _ = broker.discovery_body("lifetime-a", "req-0");
        let gen_a = broker.generation().to_owned();
        let queue = |generation: &str, cmd: &str| {
            format!(
                "{{\"v\":2,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"{cmd}\",\
                  \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
            )
        };
        let take = |generation: &str, cmd: &str| {
            format!(
                "{{\"v\":2,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"{cmd}\"}}"
            )
        };
        let filed = broker
            .queue_command(":1.42", &queue(&gen_a, "1"))
            .expect("in-band");
        let filed: serde_json::Value = serde_json::from_str(&filed).expect("JSON");
        assert_eq!(filed["outcome"], "queued");
        // A new producer lifetime rotates onto a fresh generation.
        let _ = broker.discovery_body("lifetime-b", "req-1");
        let gen_b = broker.generation().to_owned();
        assert_ne!(gen_b, gen_a);
        // Stale queue refuses bound to the serving generation, moving
        // nothing: the same per-client id files fresh under the new
        // generation (never a duplicate of the dead namespace, never a
        // remint of the old intent).
        let stale = broker
            .queue_command(":1.42", &queue(&gen_a, "2"))
            .expect("in-band");
        let stale: serde_json::Value = serde_json::from_str(&stale).expect("JSON");
        assert_eq!(stale["outcome"], "rejected");
        assert_eq!(stale["kind"], "generation-mismatch");
        assert_eq!(stale["gen"], gen_b.as_str());
        let fresh = broker
            .queue_command(":1.42", &queue(&gen_b, "2"))
            .expect("in-band");
        let fresh: serde_json::Value = serde_json::from_str(&fresh).expect("JSON");
        assert_eq!(fresh["outcome"], "queued");
        assert_eq!(fresh["globalCmd"], "1");
        // Ambiguous redelivery keeps exactly the same id and bytes: the
        // identical request replays as a duplicate under the same global
        // sequence, never a second execution.
        let redelivered = broker
            .queue_command(":1.42", &queue(&gen_b, "2"))
            .expect("in-band");
        let redelivered: serde_json::Value = serde_json::from_str(&redelivered).expect("JSON");
        assert_eq!(redelivered["outcome"], "duplicate");
        assert_eq!(redelivered["globalCmd"], "1");
        // Stale takes refuse alike and collect nothing: the new-namespace
        // command is still pending, never settled by the old call.
        let stale_take = broker
            .take_result(":1.42", &take(&gen_a, "2"))
            .expect("in-band");
        let stale_take: serde_json::Value = serde_json::from_str(&stale_take).expect("JSON");
        assert_eq!(stale_take["outcome"], "rejected");
        assert_eq!(stale_take["kind"], "generation-mismatch");
        assert_eq!(stale_take["gen"], gen_b.as_str());
        let pending = broker
            .take_result(":1.42", &take(&gen_b, "2"))
            .expect("in-band");
        let pending: serde_json::Value = serde_json::from_str(&pending).expect("JSON");
        assert_eq!(pending["outcome"], "pending");
    }

    #[test]
    fn strip_result_nulls_only_the_terminal_lane() {
        let reply = "{\"v\":2,\"result\":{\"op\":\"toggle-float\"},\"effects\":[]}";
        let stripped: serde_json::Value = serde_json::from_str(&strip_result(reply)).expect("JSON");
        assert!(stripped["result"].is_null());
        assert!(stripped["effects"].is_array());
        // Error bodies carry no result lane and pass through untouched.
        let error = "{\"v\":2,\"outcome\":\"rejected\",\"kind\":\"k\",\"message\":\"m\"}";
        let passthrough: serde_json::Value =
            serde_json::from_str(&strip_result(error)).expect("JSON");
        assert_eq!(
            passthrough,
            serde_json::from_str::<serde_json::Value>(error).expect("JSON")
        );
    }

    #[test]
    fn projection_drops_write_tokens_and_one_shot_lanes() {
        let reply = "{\"v\":2,\"rev\":\"3\",\"obs\":\"2\",\"scopes\":[],\"workspaces\":[],\
            \"outputs\":[],\"windows\":[{\"win\":\"w\",\"frame_op\":\"op-1\",\"clear_op\":\"op-1\",\
            \"alloc\":null}],\"transfers\":[{}],\"effects\":[{}],\"result\":{},\
            \"operationSettled\":[{}],\"gestureSettled\":null,\"presentation\":{}}";
        let projection: serde_json::Value =
            serde_json::from_str(&project_reply(reply, drag_preview_of(reply))).expect("JSON");
        let window = &projection["windows"][0];
        assert!(window.get("frame_op").is_none());
        assert!(window.get("clear_op").is_none());
        assert_eq!(window["win"], "w");
        for key in [
            "result",
            "transfers",
            "effects",
            "operationSettled",
            "gestureSettled",
        ] {
            assert!(projection.get(key).is_none(), "{key}");
        }
        // A non-preview result never rides the projection: the preview lane
        // stays an explicit null until core publishes a drop preview.
        assert!(projection["preview"].is_null());
        assert_eq!(projection["rev"], "3");
    }

    #[test]
    fn projection_carries_only_the_trimmed_drop_preview_geometry() {
        let preview = "{\"op\":\"drag-preview\",\"rev\":\"7\",\"domainOut\":\"out-1\",\
            \"domainWs\":\"ws-1\",\"sourceOut\":\"out-1\",\"sourceWs\":\"ws-1\",\
            \"rect\":[0,0,396,600],\"hover\":null,\"win\":\"win-a\"}";
        let reply = format!(
            "{{\"v\":2,\"rev\":\"3\",\"obs\":\"2\",\"scopes\":[],\"workspaces\":[],\
            \"outputs\":[],\"windows\":[],\"transfers\":[],\"effects\":[],\"result\":{preview},\
            \"operationSettled\":[],\"gestureSettled\":null,\"presentation\":{{}}}}"
        );
        let carried = drag_preview_of(&reply).expect("drop preview carries");
        // Renderer-only shape: paint geometry plus subject proof, nothing
        // issuer-correlated and no core provenance.
        assert_eq!(
            carried,
            serde_json::json!({
                "rect": [0, 0, 396, 600],
                "sourceOut": "out-1",
                "sourceWs": "ws-1",
                "win": "win-a",
            })
        );
        for excluded in ["op", "hover", "rev", "domainOut", "domainWs"] {
            assert!(
                carried.get(excluded).is_none(),
                "issuer/provenance field {excluded} never rides the preview"
            );
        }
        let projection: serde_json::Value =
            serde_json::from_str(&project_reply(&reply, drag_preview_of(&reply))).expect("JSON");
        assert_eq!(projection["preview"], carried);
        // The actuator reply lane itself still strips the terminal result:
        // only the projection carries the trimmed preview.
        let stripped: serde_json::Value =
            serde_json::from_str(&strip_result(&reply)).expect("JSON");
        assert!(stripped["result"].is_null());
        // A terminal result of any other op never becomes a preview.
        let other = reply.replace("\"drag-preview\"", "\"toggle-float\"");
        assert!(drag_preview_of(&other).is_none());
        let cleared: serde_json::Value =
            serde_json::from_str(&project_reply(&other, drag_preview_of(&other))).expect("JSON");
        assert!(cleared["preview"].is_null());
        // Missing and malformed preview shapes fail closed to null, never a
        // fabricated geometry: no rect, a non-quad rect, or a missing
        // subject identity carries nothing.
        assert!(drag_preview_of("{\"v\":2}").is_none());
        assert!(drag_preview_of("not json").is_none());
        let no_rect = reply.replace("[0,0,396,600]", "null");
        assert!(drag_preview_of(&no_rect).is_none());
        let short_rect = reply.replace("[0,0,396,600]", "[0,0,396]");
        assert!(drag_preview_of(&short_rect).is_none());
        let no_win = reply.replace("\"win\":\"win-a\"", "\"win\":null");
        assert!(drag_preview_of(&no_win).is_none());
    }

    #[test]
    fn foreign_epoch_attempts_refuse_above_the_clock() {
        let inner: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str::<serde_json::Value>(
                "{\"frameAttempts\":[{\"attemptMs\":\"9\"}]}",
            )
            .expect("object")
            .as_object()
            .expect("map")
            .clone();
        assert!(attempts_clock_valid(&inner, 9));
        assert!(!attempts_clock_valid(&inner, 8));
        // The creation lane gates exactly like the other attempt lanes.
        let created: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str::<serde_json::Value>(
                "{\"creationAttempts\":[{\"attemptMs\":\"9\"}]}",
            )
            .expect("object")
            .as_object()
            .expect("map")
            .clone();
        assert!(attempts_clock_valid(&created, 9));
        assert!(!attempts_clock_valid(&created, 8));
        // Missing lanes and unparseable stamps are the codec's business.
        let empty: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str::<serde_json::Value>("{}")
                .expect("object")
                .as_object()
                .expect("map")
                .clone();
        assert!(attempts_clock_valid(&empty, 0));
        let junk: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str::<serde_json::Value>(
                "{\"frameAttempts\":[{\"attemptMs\":\"xx\"}]}",
            )
            .expect("object")
            .as_object()
            .expect("map")
            .clone();
        assert!(attempts_clock_valid(&junk, 0));
    }

    #[test]
    fn take_settlement_replies_shape() {
        let ready = Settlement::Ready {
            global_seq: 7,
            result: serde_json::Value::Null,
        };
        let reply: serde_json::Value =
            serde_json::from_str(&take_settlement_reply("w-1", &ready)).expect("JSON");
        assert_eq!(reply["outcome"], "ready");
        assert_eq!(reply["globalCmd"], "7");
        assert!(reply["result"].is_null());
        assert!(reply.get("effects").is_none());
        let failed = Settlement::Failed {
            global_seq: 7,
            kind: "selection-unknown".to_owned(),
            message: "selection target is not a known workspace".to_owned(),
        };
        let reply: serde_json::Value =
            serde_json::from_str(&take_settlement_reply("w-1", &failed)).expect("JSON");
        assert_eq!(reply["outcome"], "failed");
        assert_eq!(reply["kind"], "selection-unknown");
        assert!(reply.get("result").is_none());
    }

    #[test]
    fn queued_command_validates_through_the_strict_codec() {
        let broker = WorldBroker::new();
        let select = serde_json::json!({"op":"select-numbered","out":null,"index":3});
        // Before the first accepted capture only inventory-free commands
        // queue; validation still runs through the strict codec.
        assert!(command_shape_valid(broker.generation(), &select));
        let bogus = serde_json::json!({"op":"no-such-op"});
        assert!(!command_shape_valid(broker.generation(), &bogus));
        // Explicit string outputs stay valid (genuinely semantic outputs);
        // only unknown ops and shapes refuse.
        let explicit = serde_json::json!({"op":"select-numbered","out":"DP-1","index":3});
        assert!(command_shape_valid(broker.generation(), &explicit));
        let bad_index = serde_json::json!({"op":"select-numbered","out":null,"index":10});
        assert!(!command_shape_valid(broker.generation(), &bad_index));
        // Direction-only commands carry no float key on this wire (the
        // stale TS vocabulary did): exact shape or refusal.
        let focus = serde_json::json!({"op":"focus-directional","win":"win-001",
            "direction":"left"});
        assert!(command_shape_valid(broker.generation(), &focus));
        let stale_float = serde_json::json!({"op":"focus-directional","win":"win-001",
            "direction":"left","floatSubject":false});
        assert!(!command_shape_valid(broker.generation(), &stale_float));
    }

    #[test]
    fn monotonic_clock_never_goes_backwards() {
        let broker = WorldBroker::new();
        let first = broker.monotonic_ms();
        assert!(broker.monotonic_ms() >= first);
    }
}
