//! Sole world broker: one [`WorldEngine`], one service-owned sequencing namespace.
//!
//! The broker is the only [`WorldEngine`] owner on this bus. It mints one
//! core generation seed at construction (no broker incarnation), stamps
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
//! Wire shapes (strict v4, no legacy compatibility route):
//!
//! ```text
//! discovery: connection-family messages dispatched by `type` through the
//!   connection router (see [`WorldBroker::discover_connection`]): `handshake`
//!   binds the single active producer capture lifetime, `settings` applies
//!   under its own revision rules without resetting. `adapter` is the sole
//!   adapter producer lifetime (the actuator's opaque incarnation), never a
//!   broker incarnation: replies echo it back with the serving generation,
//!   `ready=false`, and no applicable desired state. Every successful
//!   handshake clears all broker ancillary state (sequencing, replay, queue,
//!   result, presentation, watermark, gesture, wake), including the first
//!   handshake whose generation equals the construction seed, so no old
//!   capture can ever apply again.
//! capture:  {"v":4,"adapter":A,"gen":G,"source":S,"sourceSeq":Q,
//!            "nativeInvalid":"N","report":{...}}
//!   `report` is the full normalized v4 observation (see
//!   `tiler_protocol::world_protocol::decode_report_v4` for the exact
//!   required/optional keys, including `rendererInput`) WITHOUT the broker-owned
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
//!   {"v":4,"outcome":"rejected","kind","message","gen"} extended with
//!   "source", "nativeInvalid" and "clock". Unknown envelopes bind source
//!   "" and watermark "0", which never match a real source. Every response
//!   stays within the reply cap, including the bound envelope expansion.
//! queue:    {"v":4,"gen":G,"client":C,"clientCmd":"N","command":{...}}
//!   `gen` must equal the serving generation (stale generations refuse as
//!   generation-mismatch before any high-water or queue state is touched, so
//!   a delayed old-generation call can never file an old intent into the new
//!   world). `clientCmd` is a canonical decimal u64 advanced per client; at
//!   or below the per-sender per-client high-water it is a duplicate and
//!   never re-executes. The client namespace binds the authenticated D-Bus
//!   sender: one sender can neither take another's results nor move its
//!   high-water. Replies {"v":4,"outcome":"queued","gen","client",
//!   "clientCmd","globalCmd":"G"} (or "duplicate" with the same body).
//! take:     {"v":4,"gen":G,"client":C,"clientCmd":"N"}
//!   `gen` binds like the queue lane: stale generations refuse before any
//!   settlement or high-water access. Replies {"v":4,"outcome":"pending",
//!   "gen"} while queued or undispatched;
//!   {"v":4,"outcome":"ready","gen","globalCmd":"G","result":R|null} once the
//!   command dispatches (re-readable while remembered, never replaying
//!   effects); {"v":4,"outcome":"failed","gen","globalCmd":"G","kind",
//!   "message"} when the world rejected the dispatched command (truthful
//!   terminal settlement, never a null result masquerading as success);
//!   {"v":4,"outcome":"duplicate","gen"} when forgotten through bounded
//!   eviction (never re-executes); or an unknown-command rejection.
//! read:     {"v":4,"req":R}
//!   Replies {"v":4,"outcome":"presentation","gen","req","rev","obs",
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
//! capture snapshots the whole parked FIFO and drains it inside ONE
//! capture/run/publication: native observations incorporate once, every
//! queued command executes in order over post-previous-command state, and
//! each command settles individually (`ready` with its encoded result, or
//! `failed` with the core kind/message). An all-capture refusal adopts
//! nothing and settles nothing, so the whole batch reparks for the next
//! capture; pre-ready bootstraps and defensive duplicate/consumed verdicts
//! repark the same way. Rejected outcomes bind for their source exactly
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
//! Wait wire (strict v4, same auth as capture/discover: same-UID plus the
//! live KWin owner plus the bound producer lifetime and generation;
//! generation binds before lifetime, so a stale generation refuses first):
//! wait:     {"v":4,"gen":G,"adapter":A,"nonce":N,"cursor":"C"}
//!   `adapter` is the discovery-bound producer lifetime, `nonce` the
//!   waiter correlation fencing stale completions, `cursor` the last
//!   delivered wake cursor ("0" before the first wake), all canonical.
//!   Immediate `{"v":4,"outcome":"wake","gen":G,"cursor":"E"}` when the
//!   broker event epoch `E` advanced past `cursor` AND a command is
//!   pending; otherwise the single bounded server waiter parks (new
//!   waiters supersede parked ones, rotations drop them). Epoch `E`
//!   advances only on a newly queued command or an actual head drain
//!   (never on exact-capture replay, malformed captures, or
//!   rejected/no-drain captures), and the reply cursor only advances on a
//!   delivered wake, so a lease `{"v":4,"outcome":"lease","gen":G,
//!   "cursor":"C"}` (idle server lease, cursor unchanged) or a lost reply
//!   re-armed with the old cursor can never skip unseen work.
//!   `{"v":4,"outcome":"superseded","gen":G}` answers a replaced waiter.
//!
//! Native facts ride normalized (Stage 4): the inner `report` already carries
//! projected v4 facts through the strict v4 codec (including `rendererInput`,
//! with `views` refused); there is no runtime raw KWin window normalization
//! here. Window rows carry normalized native facts, never adapter verdicts:
//! retired membership verdict keys hold the identifiable row fail-closed.
//! The broker retains only the capture-only gesture lane (folded to the core
//! ingress lane on a staged map that installs with the accepted reply) and
//! the `nativeInvalidWitness` causal lane (validated and stripped before the
//! strict core decode); both strip before decode, and absent keys pass
//! through.
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
    DecodedReplyV4, ReplyWireOptionals, WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES,
    WorldEngine, WorldOutcome, decode_command_value_v4, decode_request, encode_error, encode_reply,
    is_valid_adapter_id, is_valid_request_id,
};

/// Broker wire contract version (equals the core/world protocol version).
pub const BROKER_VERSION: u32 = 4;
/// FIFO cap for commands awaiting the next actuator capture.
pub const MAX_PENDING_COMMANDS: usize = 128;
/// Cap for settled terminal results awaiting collection (oldest evicted).
pub const MAX_SETTLED_RESULTS: usize = 1024;
/// Cap for distinct command clients (fail-closed past it, never forget a
/// high-water and risk re-execution).
pub const MAX_COMMAND_CLIENTS: usize = 1024;

/// Keys the broker stamps itself; carrying any of them inside the inner
/// capture report refuses as `unknown-field`.
const BROKER_KEYS: [&str; 8] = [
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
/// Commands park here in FIFO order until one capture drains the whole
/// batch; pre-ready filings wait for the first usable capture the same
/// way.
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
    /// Causal coverage of the cached projection: the native-invalidation
    /// witness (source instance plus counter) the accepted capture
    /// covered, or `None` when the capture carried no proof (missing
    /// proof yields no drawable coverage, never guessed truth). Always
    /// set, cleared, and rotated in lockstep with `presentation`.
    presentation_covered: Option<CoverWitness>,
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
    /// Fresh sole broker: new core generation seed (the Linux route; no stale
    /// per-output alignment option exists in core).
    #[must_use]
    pub fn new() -> Self {
        Self::fresh()
    }

    fn fresh() -> Self {
        let generation = crate::world_service::mint_core_generation();
        let engine = WorldEngine::new(&generation).expect("minted generation is valid");
        Self {
            engine,
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
            presentation_covered: None,
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

    /// Replace backend state under a fresh generation (poison or
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

    /// Serving core generation (the handshake binding; core owns it).
    #[must_use]
    pub fn generation(&self) -> &str {
        self.engine.core_generation()
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

    /// Discovery route for v4 connection-family messages (`handshake` and
    /// `settings`, dispatched by `type` through the connection router).
    /// Only a successful handshake resets: every handshake reaches core and
    /// starts fresh, so all broker sequencing, replay, queue, result, and
    /// presentation state drops with the old lifetime and the producer
    /// lifetime rebinds. Settings keep their revision rules and never reset.
    /// The engine binds at handshake; the broker forwards the producer
    /// identity as supplied with no application-identity policy.
    pub fn discover_connection(&mut self, input: &str) -> Result<String, BrokerError> {
        if input.len() > WORLD_MAX_REQUEST_BYTES {
            return Err(BrokerError::Oversized);
        }
        let reply = match tiler_protocol::connection_protocol::route_connection_request(
            &mut self.engine,
            input,
        ) {
            Ok(reply) => reply,
            Err(e) => {
                if e.0 == "oversized" {
                    return Err(BrokerError::Oversized);
                }
                return Ok(self.error_body("malformed", "request is malformed"));
            }
        };
        if reply.len() > WORLD_MAX_REPLY_BYTES {
            return Err(BrokerError::Oversized);
        }
        if let Ok(handshake) = tiler_protocol::connection_protocol::decode_handshake_reply(&reply)
            && handshake.kind.is_none()
        {
            // Every successful handshake resets, including the first
            // one whose generation equals the construction seed.
            let displaced = self.take_parked_waiter();
            let clock_base = Instant::now();
            self.pending.clear();
            self.highwater.clear();
            self.settled.clear();
            self.settled_order.clear();
            self.retired.clear();
            self.retired_order.clear();
            self.presentation = None;
            self.presentation_covered = None;
            self.source_floor = 0;
            self.watermarks.clear();
            self.gesture_map = crate::kde_gesture::KdeGestureMap::new();
            self.responses.clear();
            self.response_order.clear();
            self.wake_epoch = 0;
            self.wait_ready = None;
            self.next_observation_seq = 1;
            self.next_request_seq = 1;
            self.next_command_seq = 1;
            self.clock_base = clock_base;
            if let Some(waiter) = displaced {
                self.complete_displaced(waiter);
            }
            self.capture_lifetime = Some(handshake.adapter);
        }
        Ok(reply)
    }

    /// Bounded in-band rejection bound to the serving generation, with the
    /// source watermark and clock binding (empty/zero when the envelope
    /// never parsed).
    #[must_use]
    pub fn error_body(&self, kind: &str, message: &str) -> String {
        bind_response(
            &encode_error(kind, message, self.generation()),
            "",
            0,
            self.monotonic_ms(),
        )
    }

    /// Bounded in-band rejection for a parsed capture: binds the source,
    /// watermark, and clock.
    fn bound_error(&self, kind: &str, message: &str, parsed: &ParsedCapture, clock: u64) -> String {
        bind_response(
            &encode_error(kind, message, self.generation()),
            &parsed.source,
            parsed.native_invalid,
            clock,
        )
    }

    /// Evaluate one capture envelope end to end: bound the bytes, parse the
    /// strict v4 envelope, replay byte-identical redeliveries of remembered
    /// sources without touching the engine (never draining from an old
    /// capture), otherwise bind generation/lifetime/watermark/source
    /// sequence, stamp the broker-owned sequencing namespace, validate
    /// commandless before draining, snapshot the whole parked FIFO batch,
    /// and dispatch. In-band rejections bind for their source and are remembered
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
                    &encode_error(failure.kind, failure.message, self.generation()),
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
        if parsed.generation != self.generation() {
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
        // v4 capture: the inner report already carries projected v4 facts
        // (the strict v4 codec owns the shape, including `rendererInput`).
        // The retired v3 raw KWin classification (`predicate`/`hostOwned`
        // adapter verdicts) has no v4 translation and is not attempted here:
        // a v4 report carries no such keys, and carrying them refuses as
        // `unknown-field` at the strict decode below. The raw gesture and
        // invalidation-witness lanes below are capture-only strips that stay
        // v4-compatible (absent keys pass through, present keys strip before
        // decode).
        let ParsedCapture {
            adapter,
            generation,
            source,
            source_seq,
            native_invalid,
            inner,
        } = parsed;
        let mut raw_inner = inner;
        // Broker-owned gesture normalization: the raw native lane folds to
        // the core ingress lane (script never mints) on a staged map clone
        // before sequencing. A malformed lane remembers an exact-replay
        // rejection and never installs map state. The staged map installs only with an accepted
        // `FreshOutcome::Reply` below; every other outcome (rejection,
        // decode failure, attempt-clock refusal, engine rejection)
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
        // Broker-owned causal-witness lane: the raw native-invalidation
        // witness rides the capture, is validated and stripped before the
        // strict core decode (same precedent as the gesture lane above), and
        // binds as this capture's covered value when the reply is accepted.
        // Missing or null proof yields no drawable coverage, never guessed
        // truth; malformed proof refuses like any other lane. This witness
        // is the effect-owned counter, never the JS transport watermark.
        let covered = match normalize_invalidation_witness(&mut raw_inner) {
            Ok(covered) => covered,
            Err((kind, message)) => {
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
        // The gate above decoded without mutation; the identical typed
        // report dispatches below through the batched core lane.
        if !attempts_clock_valid(&parsed.inner, clock) {
            return self.remember_rejection(
                &parsed,
                envelope,
                "snapshot-invalid",
                "attempt-clock-invalid",
                clock,
            );
        }
        // Parked FIFO batch: snapshot the whole queue for this fresh
        // capture. Commands arriving during this dispatch stay parked for
        // the next capture. Every entry decodes through the exact strict
        // codec (queue-time validation passed, so a decode failure settles
        // that command failed without touching the rest); the surviving
        // batch drains inside ONE capture/run/publication through the
        // batched core lane, which settles each command individually with a
        // single final world.
        let drained: VecDeque<QueuedCommand> = std::mem::take(&mut self.pending);
        let mut batch: Vec<(u64, tiler_core::world_report::WorldCommand)> = Vec::new();
        for queued in &drained {
            match decode_command_value_v4(&queued.command) {
                Ok(command) => batch.push((queued.global_seq, command)),
                Err(error) => {
                    self.settle_drained(
                        queued,
                        Settlement::Failed {
                            global_seq: queued.global_seq,
                            kind: error.kind.to_owned(),
                            message: error.message.to_owned(),
                        },
                    )?;
                }
            }
        }
        let last_drag_command = drained
            .back()
            .is_some_and(|queued| command_is_drag_preview(&queued.command));
        let (outcome, settlements) = self.engine.handle_report(&plain_report, &batch);
        // Settle FIFO from the per-command settlements: a typed `Rejected`
        // result settles that command terminally as failed with the core
        // kind, any other result readies it with the encoded outcome.
        // Drained entries WITHOUT a settlement were never executed (a
        // pre-ready bootstrap, a whole-capture refusal, or a defensive
        // duplicate/consumed verdict): they repark at the head in order,
        // ahead of arrivals filed during this dispatch, so nothing parked
        // is ever silently lost.
        let (outcome, command_produced_preview) = match outcome {
            WorldOutcome::Accepted(reply) => {
                let encoded = encode_world_reply(&reply)?;
                let produced = last_drag_command
                    && matches!(
                        reply.command_result,
                        Some(tiler_core::world_report::CommandResult::DragPreview { .. })
                    );
                self.settle_batch(&drained, &settlements)?;
                (FreshOutcome::Reply(encoded), produced)
            }
            WorldOutcome::Duplicate(reply) | WorldOutcome::CommandConsumed(reply) => {
                let encoded = encode_world_reply(&reply)?;
                self.repark_batch(drained);
                (FreshOutcome::Reply(encoded), false)
            }
            WorldOutcome::Rejected { kind, message } => {
                self.repark_batch(drained);
                (FreshOutcome::Rejected { kind, message }, false)
            }
        };
        match outcome {
            FreshOutcome::Rejected { kind, message } => {
                // The engine refusal verdict: bind it for this source and
                // remember it, but touch neither the presentation cache
                // (last accepted projection only) nor the watermark floor,
                // so a failed drain can never wedge or poison the cached
                // full reply. The batch already reparks and stays untouched.
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
                // published. The preview additionally names its producing
                // native gesture (`producerInstance`/`producerSerial`) when
                // this capture's normalized ingress proves it; a
                // command-produced or otherwise unproven preview rides with
                // null producer fields and the renderer hides it (fail
                // closed), while the take lane still settles the issuer.
                // The covered causal witness rides alongside for the read
                // lane. The expanded bound reply always fits the reply
                // cap; oversize fails closed without caching.
                // Neutral focused-fullscreen suspension consumes the core
                // verdict mechanically: a preview resolving on the
                // suspended output never rides the projection, so the
                // cache cannot retain an older preview across the
                // suspension. Cross-output previews stay intact.
                let preview_geometry = if preview_suspended_by_presentation(&encoded) {
                    None
                } else {
                    drag_preview_of(&encoded)
                };
                let preview = match (&preview_geometry, &gesture_assoc) {
                    (Some(_), Some(assoc)) if !command_produced_preview => {
                        CarriedPreview::proven(preview_geometry, assoc)
                    }
                    _ => CarriedPreview::unproven(preview_geometry),
                };
                let stripped = strip_result(&encoded);
                let with_assoc = inject_gesture_assoc(&stripped, gesture_assoc.as_ref());
                let bound =
                    bind_response(&with_assoc, &parsed.source, parsed.native_invalid, clock);
                if bound.len() > WORLD_MAX_REPLY_BYTES {
                    return Err(BrokerError::Oversized);
                }
                self.presentation = Some(project_reply(&stripped, preview.as_ref()));
                self.presentation_covered = covered.clone();
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

    /// Stamp the broker-owned namespace onto one commandless inner
    /// observation. The parked FIFO batch rides the typed core lane, never
    /// the stamped report. Pure over the map: no engine contact.
    fn stamp_inner(
        &self,
        inner: &mut serde_json::Map<String, serde_json::Value>,
        parsed: &ParsedCapture,
        observation_seq: u64,
        request_seq: u64,
        clock: u64,
    ) {
        inner.insert("adapter".to_owned(), json_string(&parsed.adapter));
        inner.insert("gen".to_owned(), json_string(self.generation()));
        inner.insert("obs".to_owned(), json_string(&observation_seq.to_string()));
        inner.insert("reqSeq".to_owned(), json_string(&request_seq.to_string()));
        inner.insert("req".to_owned(), json_string(&parsed.source));
        inner.insert("cmdSeq".to_owned(), serde_json::Value::Null);
        inner.insert("command".to_owned(), serde_json::Value::Null);
        inner.insert("monotonicMs".to_owned(), json_string(&clock.to_string()));
    }

    /// Settle one drained batch command terminally without queue motion
    /// (the batch snapshot already left `pending`): ready carries the core
    /// result, failed carries the explicit rejection kind/message. Each
    /// settlement advances the wake epoch and completes a parked waiter
    /// while work remains.
    fn settle_drained(
        &mut self,
        queued: &QueuedCommand,
        settlement: Settlement,
    ) -> Result<(), BrokerError> {
        self.insert_settlement(queued.key(), settlement);
        self.take_wake_epoch()?;
        self.complete_waiter_if_work();
        Ok(())
    }

    /// Settle one drained FIFO batch from the core per-command settlements.
    /// Every drained entry with a matching settlement settles terminally: a
    /// typed `Rejected` result fails with the core kind (never a null
    /// result masquerading as success), any other result readies with the
    /// encoded outcome. Drained entries without a settlement were never
    /// executed and repark at the head in order (see `repark_batch`).
    fn settle_batch(
        &mut self,
        drained: &VecDeque<QueuedCommand>,
        settlements: &[tiler_core::world_report::BatchSettlement],
    ) -> Result<(), BrokerError> {
        use std::collections::HashMap;
        let mut by_seq: HashMap<u64, &tiler_core::world_report::BatchSettlement> = HashMap::new();
        for settlement in settlements {
            by_seq.insert(settlement.seq, settlement);
        }
        let mut repark: Vec<QueuedCommand> = Vec::new();
        for queued in drained {
            let Some(settlement) = by_seq.get(&queued.global_seq) else {
                repark.push(queued.clone());
                continue;
            };
            match &settlement.result {
                Some(tiler_core::world_report::CommandResult::Rejected { kind }) => {
                    let kind_str = kind.as_str();
                    self.settle_drained(
                        queued,
                        Settlement::Failed {
                            global_seq: queued.global_seq,
                            kind: kind_str.to_owned(),
                            message: kind_str.to_owned(),
                        },
                    )?;
                }
                result => {
                    self.settle_drained(
                        queued,
                        Settlement::Ready {
                            global_seq: queued.global_seq,
                            result: encode_result_value(result),
                        },
                    )?;
                }
            }
        }
        self.repark_batch_vec(repark);
        Ok(())
    }

    /// Repark drained-but-unexecuted commands at the head in FIFO order,
    /// ahead of arrivals filed during this dispatch, so nothing parked is
    /// ever silently lost (pre-ready bootstrap, whole-capture refusal, or a
    /// defensive duplicate/consumed verdict).
    fn repark_batch(&mut self, drained: VecDeque<QueuedCommand>) {
        self.repark_batch_vec(drained.into_iter().collect());
    }

    /// Ordered head-repark worker for one unexecuted batch remainder.
    fn repark_batch_vec(&mut self, repark: Vec<QueuedCommand>) {
        for queued in repark.into_iter().rev() {
            self.pending.push_front(queued);
        }
    }

    /// File one external command. Duplicates at or below the per-sender
    /// per-client high-water never re-execute; new commands take the next
    /// global sequence and park for the next fresh capture, which drains
    /// the whole FIFO in one batch. The `sender` is the authenticated
    /// D-Bus unique name: the client namespace binds it, so one sender can
    /// never move another's high-water. The request's generation binds
    /// before any high-water or queue state is touched: a stale generation
    /// refuses as generation-mismatch, so a delayed call from a superseded
    /// lifetime can never file an old intent into the new world.
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
        if generation != self.generation() {
            return Ok(self.error_body("generation-mismatch", "generation mismatch"));
        }
        if !command_shape_valid(self.generation(), &command) {
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
                self.generation(),
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
        // Queue-only: the command parks here. The wire outcome stays
        // `queued`; the take lane serves the terminal result after the next
        // fresh capture drains the batch.
        // A newly queued command advances the wake epoch and completes a
        // parked waiter: the actuator's follow-up capture drains it.
        self.take_wake_epoch()?;
        self.complete_waiter_if_work();
        Ok(queue_reply(
            "queued",
            self.generation(),
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
        if generation != self.generation() {
            return Ok(self.error_body("generation-mismatch", "generation mismatch"));
        }
        let key = (sender.clone(), client.clone(), client_cmd.to_string());
        if self.pending.iter().any(|q| q.key() == key) {
            return Ok(take_pending_reply(self.generation()));
        }
        if let Some(settled) = self.settled.remove(&key) {
            self.settled_order.retain(|k| k != &key);
            let reply = take_settlement_reply(self.generation(), &settled);
            self.retire_settlement(key, settled);
            return Ok(reply);
        }
        if let Some(retired) = self.retired.get(&key) {
            return Ok(take_settlement_reply(self.generation(), retired));
        }
        let mark = self.highwater.get(&(sender, client)).copied().unwrap_or(0);
        if client_cmd <= mark {
            Ok(take_duplicate_reply(self.generation()))
        } else {
            Ok(self.error_body("unknown-command", "unknown command"))
        }
    }

    /// Read the cached accepted presentation projection (snapshot plus
    /// presentation plus the renderer-only drop preview, window write tokens
    /// stripped). Never carries transfers, effects, terminal results,
    /// settlements, or warp desires; the preview is the single carried
    /// drop-preview geometry (`rect`, mover-home `sourceOut`/`sourceWs`,
    /// insertion-neighbor `win`, producing-gesture `producerInstance`/
    /// `producerSerial` or nulls when unproven; null when the cached reply
    /// carried none). Every served projection also binds the causal
    /// coverage the cached capture covered (`coveredInstance`/
    /// `coveredInvalid`, nulls when unproven). Missing proof yields no
    /// drawable coverage, never guessed truth.
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
        Ok(project_read(
            cached,
            self.generation(),
            &req,
            self.presentation_covered.as_ref(),
        ))
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
            "{{\"v\":4,\"outcome\":\"wake\",\"gen\":\"{}\",\"cursor\":\"{}\"}}",
            self.generation(),
            self.wake_epoch
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
        if generation != self.generation() {
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
            "{{\"v\":4,\"outcome\":\"wake\",\"gen\":\"{}\",\"cursor\":\"{}\"}}",
            self.generation(),
            self.wake_epoch
        )
    }

    /// Idle lease body: the cursor is unchanged, so re-arming with it can
    /// never skip unseen work.
    #[must_use]
    pub fn wait_lease_body(&self, cursor: u64) -> String {
        format!(
            "{{\"v\":4,\"outcome\":\"lease\",\"gen\":\"{}\",\"cursor\":\"{cursor}\"}}",
            self.generation()
        )
    }

    /// Superseded body for a waiter replaced by a newer registration.
    #[must_use]
    pub fn wait_superseded_body(&self) -> String {
        format!(
            "{{\"v\":4,\"outcome\":\"superseded\",\"gen\":\"{}\"}}",
            self.generation()
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

/// Strict v4 capture envelope parse: exact keys
/// (`v,adapter,gen,source,sourceSeq,nativeInvalid,report`), v4 versions in
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

/// Strict queue-request parse: exact keys, v4, serving generation binding,
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

/// Strict take-request parse: exact keys, v4, serving generation binding,
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
/// v4, serving-generation binding, discovery-bound producer lifetime,
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

/// Strict read-request parse: exact keys, v4, opaque correlation.
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
        "{\"v\":4,\"screens\":[],\"windows\":[],\"closed\":[],\"native\":[],\
          \"focused\":\"\",\"foregroundKnown\":false,\"complete\":true,\"gesture\":false,\"cursor\":null,\
          \"actionForeground\":null,\"actionCached\":null,\"actionOutputs\":[],\
          \"presentation\":{\"foreground\":\"\",\"minimized\":null,\"maximized\":null,\
          \"fullscreen\":null,\"hostHidden\":null,\"hiddenWorkspace\":null,\"frame\":null,\
          \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[],\"pagerFrames\":[],\
          \"witness\":null,\"foregroundOut\":null},\"intentRecords\":[],\
          \"stateAttempts\":[],\"focusFeedback\":[],\"rendererInput\":null}",
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

/// Checked canonical encode of one v4 engine reply for the broker lane.
/// The wire presence flag mirrors the typed presentation so the strict
/// codec accepts its own output. Any encode failure fails closed as
/// oversize (the caller never serves a partial reply).
fn encode_world_reply(
    reply: &tiler_core::world_report::WorldReplyV4,
) -> Result<String, BrokerError> {
    let decoded = DecodedReplyV4 {
        reply: reply.clone(),
        wire: ReplyWireOptionals {
            suspended_output_present: reply.presentation.suspended_output.is_some(),
        },
    };
    match encode_reply(&decoded) {
        Ok(encoded) => {
            if encoded.len() > WORLD_MAX_REPLY_BYTES {
                return Err(BrokerError::Oversized);
            }
            Ok(encoded)
        }
        Err(_) => Err(BrokerError::Oversized),
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

/// Encode one per-command terminal result through the exact canonical lane:
/// `Rejected` never reaches here (the caller fails those with the core
/// kind); anything else (including `None`) readies with the encoded
/// outcome, null when the snapshot alone is the outcome.
fn encode_result_value(
    result: &Option<tiler_core::world_report::CommandResult>,
) -> serde_json::Value {
    let mut encoded = String::new();
    tiler_core::world_sizing::encode_command_result(&mut encoded, result);
    serde_json::from_str(&encoded).unwrap_or(serde_json::Value::Null)
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
        "{{\"v\":4,\"outcome\":\"{outcome}\",\"gen\":\"{generation}\",\
          \"client\":\"{client}\",\"clientCmd\":\"{client_cmd}\",\
          \"globalCmd\":{global_json}}}"
    )
}

fn take_pending_reply(generation: &str) -> String {
    format!("{{\"v\":4,\"outcome\":\"pending\",\"gen\":\"{generation}\"}}")
}

/// Terminal take reply for one settlement: `ready` with the core result
/// (possibly null when the snapshot alone is the outcome), or `failed`
/// with the explicit rejection kind/message. Never effects, never replay.
fn take_settlement_reply(generation: &str, settlement: &Settlement) -> String {
    match settlement {
        Settlement::Ready { global_seq, result } => format!(
            "{{\"v\":4,\"outcome\":\"ready\",\"gen\":\"{generation}\",\
              \"globalCmd\":\"{global_seq}\",\"result\":{result}}}"
        ),
        Settlement::Failed {
            global_seq,
            kind,
            message,
        } => format!(
            "{{\"v\":4,\"outcome\":\"failed\",\"gen\":\"{generation}\",\
              \"globalCmd\":\"{global_seq}\",\"kind\":\"{kind}\",\"message\":\"{message}\"}}"
        ),
    }
}

fn take_duplicate_reply(generation: &str) -> String {
    format!("{{\"v\":4,\"outcome\":\"duplicate\",\"gen\":\"{generation}\"}}")
}

/// Causal coverage witness of one accepted capture: the
/// native-invalidation source instance plus the counter value the capture
/// covered. The counter is the effect-owned causal value, never the JS
/// transport watermark: captures carry it in the stripped raw
/// `nativeInvalidWitness` lane. Missing proof is `None` (no drawable
/// coverage, never guessed truth).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverWitness {
    /// Witness source instance (opaque effect-facts instance).
    pub instance: String,
    /// Covered counter (canonical decimal u64).
    pub counter: String,
}

/// Validate and strip the raw native-invalidation witness lane of one
/// inner report before the strict core decode (same precedent as the
/// gesture lane): `nativeInvalidWitness` is null or missing (no proof,
/// `None`), or exactly `{"instance","counter"}` with an opaque instance
/// and a canonical u64 counter. Any other shape refuses as a lane error.
/// Pure over the map: never touches the engine.
pub fn normalize_invalidation_witness(
    inner: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<Option<CoverWitness>, (&'static str, &'static str)> {
    let raw = match inner.remove("nativeInvalidWitness") {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(serde_json::Value::Object(map)) => map,
        Some(_) => return Err(("malformed", "request is malformed")),
    };
    if raw.len() != 2 || !raw.contains_key("instance") || !raw.contains_key("counter") {
        if raw.keys().any(|k| k != "instance" && k != "counter") {
            return Err(("unknown-field", "request contains an unknown field"));
        }
        return Err(("malformed", "request is malformed"));
    }
    let instance = match raw.get("instance") {
        Some(serde_json::Value::String(s))
            if !s.is_empty() && s.len() <= 128 && tiler_core::bounds::is_opaque_id(s) =>
        {
            s.clone()
        }
        Some(serde_json::Value::String(_)) => {
            return Err(("unknown-value", "request contains an unknown value"));
        }
        _ => return Err(("malformed", "request is malformed")),
    };
    let counter = match raw.get("counter") {
        Some(serde_json::Value::String(s)) if is_canonical_u64(s) => s.clone(),
        Some(serde_json::Value::String(_)) => {
            return Err(("unknown-value", "request contains an unknown value"));
        }
        _ => return Err(("malformed", "request is malformed")),
    };
    Ok(Some(CoverWitness { instance, counter }))
}

/// Whether one queued command is itself a drag-preview operation: its
/// terminal result may be a command-produced `DragPreview`. Only that exact
/// op disqualifies the staged gesture association (the result bytes are the
/// command's own, not the live session's). Any other shape (including a
/// malformed command, which never drains) is not a drag-preview command.
fn command_is_drag_preview(command: &serde_json::Value) -> bool {
    let outer = match command.as_object() {
        Some(map) => map,
        None => return false,
    };
    if outer.get("op").and_then(serde_json::Value::as_str) != Some("operation") {
        return false;
    }
    match outer.get("command") {
        Some(serde_json::Value::Object(inner)) => {
            inner.get("op").and_then(serde_json::Value::as_str) == Some("drag-preview")
        }
        _ => false,
    }
}

/// Renderer-carried drop preview: the paint geometry plus subject proof,
/// with the producing native gesture when proven. Unproven previews ride
/// with null producer fields and the renderer hides them (fail closed),
/// while the take lane still settles the issuer.
#[derive(Debug, Clone)]
struct CarriedPreview {
    geometry: serde_json::Value,
    producer: Option<(String, String)>,
}

impl CarriedPreview {
    fn proven(
        geometry: Option<serde_json::Value>,
        assoc: &crate::kde_gesture::GestureAssoc,
    ) -> Option<Self> {
        geometry.map(|geometry| Self {
            geometry,
            producer: Some((assoc.instance.clone(), assoc.serial.clone())),
        })
    }

    fn unproven(geometry: Option<serde_json::Value>) -> Option<Self> {
        geometry.map(|geometry| Self {
            geometry,
            producer: None,
        })
    }
}

/// Whether one accepted reply suspends its renderer-only drop preview:
/// the core published a focused-fullscreen suspension verdict for the
/// resolution (pointer) domain output of the `DragPreview` result.
/// Mechanical consumption of the core-published verdict, no broker policy:
/// the mover-home source may differ, so a cross-output preview resolving
/// on another output stays intact.
fn preview_suspended_by_presentation(encoded: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(encoded)
        .ok()
        .and_then(|value| {
            let result = value.get("result")?;
            if result.get("op").and_then(serde_json::Value::as_str) != Some("drag-preview") {
                return None;
            }
            let domain_out = result
                .get("domainOut")
                .and_then(serde_json::Value::as_str)?;
            let suspended = value
                .get("presentation")?
                .get("suspendedOutput")
                .and_then(serde_json::Value::as_str)?;
            Some(domain_out == suspended)
        })
        .unwrap_or(false)
}

/// Renderer-only drop preview of one accepted reply: the paint geometry
/// plus the subject proof (`rect`, the mover-home `sourceOut`/`sourceWs`,
/// and the insertion-neighbor `win`) when the reply carried a `DragPreview`
/// result, else `None`. Pure over the encoded reply; never touches the
/// engine. Issuer correlation and core provenance (the `op` tag, the
/// `hover` chain, the session `rev`, the resolution `domainOut`/`domainWs`)
/// never ride: the subscriber paints from these fields plus the cached
/// snapshot/scopes/presentation, vets staleness through the projection
/// generation/revision/coverage, and applies live hide-only vetoes at
/// dispatch.
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
/// source, insertion-neighbor window, producing-gesture identity or nulls
/// when unproven; null when no drop preview was published) plus the causal
/// coverage bound at read time. The `gen`/`req` binding is added at read
/// time.
fn project_reply(encoded: &str, preview: Option<&CarriedPreview>) -> String {
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
    let preview_value = match preview {
        None => serde_json::Value::Null,
        Some(carried) => {
            let mut geometry = carried.geometry.clone();
            if let Some(map) = geometry.as_object_mut() {
                match &carried.producer {
                    Some((instance, serial)) => {
                        map.insert("producerInstance".to_owned(), json_string(instance));
                        map.insert("producerSerial".to_owned(), json_string(serial));
                    }
                    None => {
                        map.insert("producerInstance".to_owned(), serde_json::Value::Null);
                        map.insert("producerSerial".to_owned(), serde_json::Value::Null);
                    }
                }
            }
            geometry
        }
    };
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
        "preview": preview_value,
    })
    .to_string()
}

/// Serve one cached projection to a subscriber: bind the serving
/// generation, the read correlation, and the causal coverage the cached
/// capture covered (nulls when unproven: missing proof yields no drawable
/// coverage, never guessed truth).
fn project_read(
    cached: &str,
    generation: &str,
    req: &str,
    covered: Option<&CoverWitness>,
) -> String {
    let mut value: serde_json::Value = serde_json::from_str(cached).unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.insert("gen".to_owned(), json_string(generation));
        map.insert("req".to_owned(), json_string(req));
        match covered {
            Some(witness) => {
                map.insert("coveredInstance".to_owned(), json_string(&witness.instance));
                map.insert("coveredInvalid".to_owned(), json_string(&witness.counter));
            }
            None => {
                map.insert("coveredInstance".to_owned(), serde_json::Value::Null);
                map.insert("coveredInvalid".to_owned(), serde_json::Value::Null);
            }
        }
    }
    value.to_string()
}

fn json_string(value: &str) -> serde_json::Value {
    serde_json::Value::String(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handshake_message(adapter: &str, req: &str) -> String {
        format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":false,\"fullscreen\":false,\"nativeSticky\":false,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        )
    }

    fn settings_message(adapter: &str, generation: &str, req: &str) -> String {
        format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
              \"req\":\"{req}\",\"rev\":\"1\",\"values\":{{}}}}"
        )
    }

    #[test]
    fn handshake_binds_producer_lifetime_without_broker_incarnation() {
        let mut broker = WorldBroker::new();
        let reply: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&handshake_message("lifetime-a", "req-0"))
                .expect("handshake routes"),
        )
        .expect("handshake reply is JSON");
        assert_eq!(reply["adapter"], "lifetime-a");
        assert_eq!(reply["req"], "req-0");
        assert_eq!(reply["ready"], false);
        let generation = reply["gen"].as_str().expect("generation").to_owned();
        assert!(!generation.is_empty());
        assert_eq!(broker.generation(), generation.as_str());
        // No broker incarnation exists: the reply adapter is the producer
        // lifetime, and a second lifetime binds a fresh generation.
        let reply_b: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&handshake_message("lifetime-b", "req-1"))
                .expect("handshake routes"),
        )
        .expect("handshake reply is JSON");
        assert_eq!(reply_b["adapter"], "lifetime-b");
        let generation_b = reply_b["gen"].as_str().expect("generation").to_owned();
        assert_ne!(generation_b, generation);
        assert_eq!(broker.generation(), generation_b.as_str());
        // Settings dispatch through the same route without resetting.
        let settings: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&settings_message("lifetime-b", &generation_b, "req-2"))
                .expect("settings routes"),
        )
        .expect("settings reply is JSON");
        assert_eq!(settings["adapter"], "lifetime-b");
        assert_eq!(
            settings["gen"],
            serde_json::Value::String(generation_b.clone())
        );
        assert_eq!(broker.generation(), generation_b.as_str());
    }

    #[test]
    fn initial_handshake_drops_prebinding_queue_state() {
        let mut broker = WorldBroker::new();
        let generation = broker.generation().to_owned();
        let queue = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
        );
        let filed: serde_json::Value =
            serde_json::from_str(&broker.queue_command(":1.42", &queue).expect("in-band"))
                .expect("JSON");
        assert_eq!(filed["outcome"], "queued");
        broker
            .discover_connection(&handshake_message("lifetime-a", "req-0"))
            .expect("handshake routes");
        assert_eq!(broker.generation(), generation);
        assert!(broker.pending.is_empty());
        assert!(broker.highwater.is_empty());
        assert_eq!(broker.next_command_seq, 1);
        assert_eq!(broker.capture_lifetime.as_deref(), Some("lifetime-a"));
    }

    #[test]
    fn repeat_handshake_resets_broker_state() {
        let mut broker = WorldBroker::new();
        let reply: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&handshake_message("lifetime-a", "req-0"))
                .expect("handshake routes"),
        )
        .expect("handshake reply is JSON");
        let gen_a = reply["gen"].as_str().expect("gen").to_owned();
        let queue = format!(
            "{{\"v\":4,\"gen\":\"{gen_a}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
        );
        let filed: serde_json::Value =
            serde_json::from_str(&broker.queue_command(":1.42", &queue).expect("in-band"))
                .expect("JSON");
        assert_eq!(filed["outcome"], "queued");
        // Every handshake reaches core and starts fresh: repeating the same
        // lifetime resets onto a new generation and drops queued state.
        let reply2: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&handshake_message("lifetime-a", "req-1"))
                .expect("handshake routes"),
        )
        .expect("handshake reply is JSON");
        let gen_b = reply2["gen"].as_str().expect("gen").to_owned();
        assert_ne!(gen_b, gen_a);
        let take =
            format!("{{\"v\":4,\"gen\":\"{gen_b}\",\"client\":\"panel-1\",\"clientCmd\":\"1\"}}");
        let dropped: serde_json::Value =
            serde_json::from_str(&broker.take_result(":1.42", &take).expect("in-band"))
                .expect("JSON");
        assert_eq!(dropped["outcome"], "rejected");
        assert_eq!(dropped["kind"], "unknown-command");
    }

    #[test]
    fn stale_generation_wait_refuses_before_identity_after_reset() {
        let mut broker = WorldBroker::new();
        let reply: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&handshake_message("lifetime-a", "req-0"))
                .expect("handshake routes"),
        )
        .expect("handshake reply is JSON");
        let gen_a = reply["gen"].as_str().expect("gen").to_owned();
        let _ = broker
            .discover_connection(&handshake_message("lifetime-b", "req-1"))
            .expect("handshake routes");
        // Stale generation answers mismatch before producer identity: the
        // lifetime is also unknown under the fresh binding, but generation
        // binds first.
        let err = broker
            .wait_immediate("lifetime-a", &gen_a, 0)
            .expect_err("stale refuses");
        let parsed: serde_json::Value = serde_json::from_str(&err).expect("JSON");
        assert_eq!(parsed["outcome"], "rejected");
        assert_eq!(parsed["kind"], "generation-mismatch");
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
    fn capture_envelope_rejects_broker_keys_and_legacy_versions() {
        let inner = r#"{"v":4,"obs":"1"}"#;
        let with_owner = format!(
            r#"{{"v":4,"adapter":"a-1","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{inner}}}"#
        );
        // `obs` inside the report is broker-owned, but the outer envelope
        // parsed, so the rejection still binds the source watermark.
        let failure = parse_capture_envelope(&with_owner).expect_err("broker keys refuse");
        assert_eq!(failure.kind, "unknown-field");
        assert_eq!(failure.message, "request contains an unknown field");
        assert_eq!(failure.source, "src-1");
        assert_eq!(failure.native_invalid, 3);
        let v1 = r#"{"v":1,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":1}}"#;
        let failure = parse_capture_envelope(v1).expect_err("v1 refuses");
        assert_eq!(failure.kind, "unsupported-version");
        assert_eq!(failure.source, "");
        let v2 = r#"{"v":2,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":2}}"#;
        let failure = parse_capture_envelope(v2).expect_err("v2 refuses after v4 cutover");
        assert_eq!(failure.kind, "unsupported-version");
        assert_eq!(failure.source, "");
        let bad_watermark = r#"{"v":4,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"03","report":{"v":4}}"#;
        let failure = parse_capture_envelope(bad_watermark).expect_err("watermark refuses");
        assert_eq!(failure.kind, "unknown-value");
        assert_eq!(failure.source, "");
        // The per-lifetime source sequence is mandatory and canonical: a
        // missing, non-canonical, or overflowing sequence refuses as a
        // value, never as a match.
        let no_seq = r#"{"v":4,"adapter":"a","gen":"w-1","source":"src-1","nativeInvalid":"3","report":{"v":4}}"#;
        assert_eq!(
            parse_capture_envelope(no_seq)
                .expect_err("sourceSeq required")
                .kind,
            "malformed"
        );
        let bad_seq = r#"{"v":4,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"01","nativeInvalid":"3","report":{"v":4}}"#;
        assert_eq!(
            parse_capture_envelope(bad_seq)
                .expect_err("sequence canonical")
                .kind,
            "unknown-value"
        );
        // Generation and adapter are mandatory envelope bindings now.
        let no_gen = r#"{"v":4,"adapter":"a","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":4}}"#;
        assert_eq!(
            parse_capture_envelope(no_gen)
                .expect_err("gen required")
                .kind,
            "malformed"
        );
        let no_adapter = r#"{"v":4,"gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":4}}"#;
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
        // Strict v4: the generation is mandatory and token-shaped on both
        // lanes; a missing key is malformed, a bad token an unknown value.
        let no_gen_queue = "{\"v\":4,\"client\":\"panel-1\",\"clientCmd\":\"1\",\
            \"command\":{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}";
        assert_eq!(
            parse_queue_request(no_gen_queue)
                .expect_err("queue gen required")
                .0,
            "malformed"
        );
        let bad_gen_queue = "{\"v\":4,\"gen\":\"GEN STALE!\",\"client\":\"panel-1\",\
            \"clientCmd\":\"1\",\"command\":{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}";
        assert_eq!(
            parse_queue_request(bad_gen_queue)
                .expect_err("queue gen token")
                .0,
            "unknown-value"
        );
        let no_gen_take = "{\"v\":4,\"client\":\"panel-1\",\"clientCmd\":\"1\"}";
        assert_eq!(
            parse_take_request(no_gen_take)
                .expect_err("take gen required")
                .0,
            "malformed"
        );
        let bad_gen_take = "{\"v\":4,\"gen\":\"\",\"client\":\"panel-1\",\"clientCmd\":\"1\"}";
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
        let _ = broker
            .discover_connection(&handshake_message("lifetime-a", "req-0"))
            .expect("handshake routes");
        let gen_a = broker.generation().to_owned();
        let queue = |generation: &str, cmd: &str| {
            format!(
                "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"{cmd}\",\
                  \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
            )
        };
        let take = |generation: &str, cmd: &str| {
            format!(
                "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"{cmd}\"}}"
            )
        };
        let filed = broker
            .queue_command(":1.42", &queue(&gen_a, "1"))
            .expect("in-band");
        let filed: serde_json::Value = serde_json::from_str(&filed).expect("JSON");
        assert_eq!(filed["outcome"], "queued");
        // A new producer lifetime rotates onto a fresh generation.
        let _ = broker
            .discover_connection(&handshake_message("lifetime-b", "req-1"))
            .expect("handshake routes");
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
        let reply = "{\"v\":4,\"result\":{\"op\":\"toggle-float\"},\"effects\":[]}";
        let stripped: serde_json::Value = serde_json::from_str(&strip_result(reply)).expect("JSON");
        assert!(stripped["result"].is_null());
        assert!(stripped["effects"].is_array());
        // Error bodies carry no result lane and pass through untouched.
        let error = "{\"v\":4,\"outcome\":\"rejected\",\"kind\":\"k\",\"message\":\"m\"}";
        let passthrough: serde_json::Value =
            serde_json::from_str(&strip_result(error)).expect("JSON");
        assert_eq!(
            passthrough,
            serde_json::from_str::<serde_json::Value>(error).expect("JSON")
        );
    }

    #[test]
    fn projection_drops_write_tokens_and_one_shot_lanes() {
        let reply = "{\"v\":4,\"rev\":\"3\",\"obs\":\"2\",\"scopes\":[],\"workspaces\":[],\
            \"outputs\":[],\"windows\":[{\"win\":\"w\",\"frame_op\":\"op-1\",\"clear_op\":\"op-1\",\
            \"alloc\":null}],\"transfers\":[{}],\"effects\":[{}],\"result\":{},\
            \"operationSettled\":[{}],\"gestureSettled\":null,\"presentation\":{}}";
        let unproven = CarriedPreview::unproven(drag_preview_of(reply));
        let projection: serde_json::Value =
            serde_json::from_str(&project_reply(reply, unproven.as_ref())).expect("JSON");
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
            "{{\"v\":4,\"rev\":\"3\",\"obs\":\"2\",\"scopes\":[],\"workspaces\":[],\
            \"outputs\":[],\"windows\":[],\"transfers\":[],\"effects\":[],\"result\":{preview},\
            \"operationSettled\":[],\"gestureSettled\":null,\"presentation\":{{}}}}"
        );
        let geometry = drag_preview_of(&reply).expect("drop preview carries");
        // Renderer-only shape: paint geometry plus subject proof, nothing
        // issuer-correlated and no core provenance.
        assert_eq!(
            geometry,
            serde_json::json!({
                "rect": [0, 0, 396, 600],
                "sourceOut": "out-1",
                "sourceWs": "ws-1",
                "win": "win-a",
            })
        );
        for excluded in ["op", "hover", "rev", "domainOut", "domainWs"] {
            assert!(
                geometry.get(excluded).is_none(),
                "issuer/provenance field {excluded} never rides the preview"
            );
        }
        // A proven preview binds its producing native gesture; an unproven
        // one rides the same geometry with null producer fields (the
        // renderer hides it, fail closed).
        let assoc = crate::kde_gesture::GestureAssoc {
            instance: "gesture-1".to_owned(),
            serial: "1".to_owned(),
            core_id: 1,
        };
        let proven =
            CarriedPreview::proven(Some(geometry.clone()), &assoc).expect("proven carries");
        let projection: serde_json::Value =
            serde_json::from_str(&project_reply(&reply, Some(&proven))).expect("JSON");
        assert_eq!(
            projection["preview"],
            serde_json::json!({
                "rect": [0, 0, 396, 600],
                "sourceOut": "out-1",
                "sourceWs": "ws-1",
                "win": "win-a",
                "producerInstance": "gesture-1",
                "producerSerial": "1",
            })
        );
        let unproven = CarriedPreview::unproven(Some(geometry)).expect("unproven carries geometry");
        let projection: serde_json::Value =
            serde_json::from_str(&project_reply(&reply, Some(&unproven))).expect("JSON");
        assert_eq!(
            projection["preview"],
            serde_json::json!({
                "rect": [0, 0, 396, 600],
                "sourceOut": "out-1",
                "sourceWs": "ws-1",
                "win": "win-a",
                "producerInstance": null,
                "producerSerial": null,
            })
        );
        // The actuator reply lane itself still strips the terminal result:
        // only the projection carries the trimmed preview.
        let stripped: serde_json::Value =
            serde_json::from_str(&strip_result(&reply)).expect("JSON");
        assert!(stripped["result"].is_null());
        // A terminal result of any other op never becomes a preview.
        let other = reply.replace("\"drag-preview\"", "\"toggle-float\"");
        assert!(drag_preview_of(&other).is_none());
        let cleared: serde_json::Value = serde_json::from_str(&project_reply(
            &other,
            CarriedPreview::unproven(drag_preview_of(&other)).as_ref(),
        ))
        .expect("JSON");
        assert!(cleared["preview"].is_null());
        // Missing and malformed preview shapes fail closed to null, never a
        // fabricated geometry: no rect, a non-quad rect, or a missing
        // subject identity carries nothing.
        assert!(drag_preview_of("{\"v\":4}").is_none());
        assert!(drag_preview_of("not json").is_none());
        let no_rect = reply.replace("[0,0,396,600]", "null");
        assert!(drag_preview_of(&no_rect).is_none());
        let short_rect = reply.replace("[0,0,396,600]", "[0,0,396]");
        assert!(drag_preview_of(&short_rect).is_none());
        let no_win = reply.replace("\"win\":\"win-a\"", "\"win\":null");
        assert!(drag_preview_of(&no_win).is_none());
    }

    #[test]
    fn suspended_presentation_omits_only_the_matching_domain_preview() {
        // A suspended output matching the preview's resolution (pointer)
        // domain omits the renderer preview; the command result itself is
        // untouched (the take lane still settles the issuer).
        let suspended = "{\"v\":4,\"rev\":\"3\",\"obs\":\"2\",\"scopes\":[],\"workspaces\":[],\
            \"outputs\":[],\"windows\":[],\"transfers\":[],\"effects\":[],\
            \"result\":{\"op\":\"drag-preview\",\"rev\":\"7\",\"domainOut\":\"out-1\",\
            \"domainWs\":\"ws-1\",\"sourceOut\":\"out-2\",\"sourceWs\":\"ws-2\",\
            \"rect\":[0,0,396,600],\"hover\":null,\"win\":\"win-a\"},\
            \"operationSettled\":[],\"gestureSettled\":null,\
            \"presentation\":{\"foreground\":\"win-g\",\"suspendedOutput\":\"out-1\",\
            \"border\":{\"visible\":false,\"reason\":\"suspended\",\"inner\":null,\"outer\":null},\
            \"underlay\":{\"status\":\"hidden\",\"reason\":\"suspended\",\"baseRev\":null},\"pager\":[]}}";
        assert!(preview_suspended_by_presentation(suspended));
        // A cross-output preview resolving on another output stays intact:
        // only the resolution domain output gates, never the mover-home
        // source.
        let crossed = suspended.replace("\"domainOut\":\"out-1\"", "\"domainOut\":\"out-2\"");
        assert!(!preview_suspended_by_presentation(&crossed));
        // No verdict, no result, or another op never suspends.
        let plain = suspended.replace(",\"suspendedOutput\":\"out-1\"", "");
        assert!(!preview_suspended_by_presentation(&plain));
        let no_result = suspended.replace(
            "\"result\":{\"op\":\"drag-preview\",\"rev\":\"7\",\"domainOut\":\"out-1\",\
            \"domainWs\":\"ws-1\",\"sourceOut\":\"out-2\",\"sourceWs\":\"ws-2\",\
            \"rect\":[0,0,396,600],\"hover\":null,\"win\":\"win-a\"}",
            "\"result\":null",
        );
        assert!(!preview_suspended_by_presentation(&no_result));
        let other = suspended.replace("\"drag-preview\"", "\"toggle-float\"");
        assert!(!preview_suspended_by_presentation(&other));
        assert!(!preview_suspended_by_presentation("{\"v\":4}"));
        assert!(!preview_suspended_by_presentation("not json"));
        // The conversion over a suspended reply carries no preview, so the
        // projection (and its cache) cannot retain an older preview across
        // the suspension. Mirrors the production conversion sequence: the
        // verdict gates before stripping.
        let geometry = drag_preview_of(suspended);
        let gated = if preview_suspended_by_presentation(suspended) {
            None
        } else {
            geometry
        };
        assert!(gated.is_none(), "suspended resolution domain omits preview");
        let unproven = CarriedPreview::unproven(gated);
        let projection: serde_json::Value =
            serde_json::from_str(&project_reply(suspended, unproven.as_ref())).expect("JSON");
        assert!(
            projection["preview"].is_null(),
            "suspended presentation projects no preview"
        );
        // The cross-output preview survives the same conversion.
        let cross_geometry = drag_preview_of(&crossed);
        let cross_gated = if preview_suspended_by_presentation(&crossed) {
            None
        } else {
            cross_geometry
        };
        assert!(cross_gated.is_some(), "other-output preview stays intact");
    }

    #[test]
    fn invalidation_witness_strips_and_validates_strictly() {
        // Missing or null proof yields no drawable coverage, never a
        // refusal; the key strips before the strict core decode.
        let mut missing: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str::<serde_json::Value>("{}")
                .expect("object")
                .as_object()
                .expect("map")
                .clone();
        assert_eq!(
            normalize_invalidation_witness(&mut missing).expect("missing is None"),
            None
        );
        assert!(!missing.contains_key("nativeInvalidWitness"));
        let mut null: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str::<serde_json::Value>("{\"nativeInvalidWitness\":null}")
                .expect("object")
                .as_object()
                .expect("map")
                .clone();
        assert_eq!(
            normalize_invalidation_witness(&mut null).expect("null is None"),
            None
        );
        // A well-formed witness validates and strips, binding as coverage.
        let mut proven: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str::<serde_json::Value>(
                "{\"nativeInvalidWitness\":{\"instance\":\"effect-1\",\"counter\":\"7\"},\"v\":4}",
            )
            .expect("object")
            .as_object()
            .expect("map")
            .clone();
        assert_eq!(
            normalize_invalidation_witness(&mut proven).expect("witness validates"),
            Some(CoverWitness {
                instance: "effect-1".to_owned(),
                counter: "7".to_owned(),
            })
        );
        assert!(!proven.contains_key("nativeInvalidWitness"));
        assert_eq!(
            proven.get("v").expect("other keys survive").as_u64(),
            Some(4)
        );
        // Malformed proof refuses: mistyped, wrong keys, non-canonical
        // counters, and non-opaque instances.
        for bad in [
            "{\"nativeInvalidWitness\":7}",
            "{\"nativeInvalidWitness\":{\"instance\":\"effect-1\"}}",
            "{\"nativeInvalidWitness\":{\"instance\":\"effect-1\",\"counter\":\"07\"}}",
            "{\"nativeInvalidWitness\":{\"instance\":\"effect-1\",\"counter\":7}}",
            "{\"nativeInvalidWitness\":{\"instance\":\"\",\"counter\":\"7\"}}",
            "{\"nativeInvalidWitness\":{\"instance\":\"effect-1\",\"counter\":\"7\",\"extra\":1}}",
        ] {
            let mut inner: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str::<serde_json::Value>(bad)
                    .expect("object")
                    .as_object()
                    .expect("map")
                    .clone();
            assert!(
                normalize_invalidation_witness(&mut inner).is_err(),
                "malformed witness refuses: {bad}"
            );
        }
    }

    #[test]
    fn drag_preview_commands_never_prove_a_native_gesture() {
        // Only the exact drag-preview operation shape disqualifies the
        // staged association; unrelated and malformed commands do not.
        assert!(command_is_drag_preview(
            &serde_json::json!({"op":"operation","command":{"op":"drag-preview","win":"w"}})
        ));
        for other in [
            serde_json::json!({"op":"select-numbered","out":null,"index":1}),
            serde_json::json!({"op":"operation","command":{"op":"toggle-float"}}),
            serde_json::json!({"op":"operation","command":{"op":"drag-drop"}}),
            serde_json::json!({"op":"operation"}),
            serde_json::json!({"op":"operation","command":null}),
            serde_json::json!("drag-preview"),
        ] {
            assert!(
                !command_is_drag_preview(&other),
                "not a drag command: {other}"
            );
        }
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
