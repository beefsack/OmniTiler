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
//!   tokens (`frame_op`) never enter the presentation cache;
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
//!   required/optional keys, including `rendererInput` and `focusChanges`)
//!   without broker-owned keys (`owner`, `adapter`, `gen`, `obs`, `reqSeq`,
//!   `req`, `cmdSeq`, `command`, `monotonicMs`): carrying any of them refuses
//!   as unknown-field. `adapter` is the producer capture
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
//!   one. Window rows carry no frame_op tokens. `preview` is the
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
//! never suppresses). After strict decode, one typed pass over
//! `&mut decoded.report` retains `attempt_ms <= clock` in place for `frame`
//! (`Some`), `member`, `view`, `state`, and `creation` (skipping null frame
//! `attempt_ms`, `attempt_id`, and unstamped `visibility`): timestamps at or
//! below the clock pass through to core unchanged, while identifiable
//! future-stamped rows are ignored as evidence (cannot settle) so valid
//! siblings still apply. Already-held codec rows and their residuals remain
//! untouched. The broker-owned decoded report is transient for local dispatch
//! only: exact replay and `source-reused` detection compare the original
//! envelope bytes before decode, and each fresh source receives a fresh
//! broker `reqSeq`, so ignoring clock-invalid rows never mutates raw capture
//! bytes or conflates distinct requests. The broker never adjusts,
//! interpolates, or manufactures attempt evidence: unknown stays unknown,
//! and no synthetic stamp ever proves native completion or arrival.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use serde_json::Map;

#[cfg(test)]
use tiler_protocol::world_protocol::decode_request;
use tiler_protocol::world_protocol::{
    WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, WorldEngine, WorldOutcome,
    decode_command_value_v4, decode_report_v4_map, encode_error, is_valid_adapter_id,
    is_valid_request_id,
};
use tiler_protocol::world_reply_write::{
    BrokerGestureAssoc, BrokerReplyBind, CachedProjectionV4, write_bound_reply_v4,
    write_bound_reply_v4_prevalidated, write_cached_projection_v4_prevalidated,
};
#[cfg(test)]
use tiler_protocol::world_reply_write::{write_cached_projection_v4, write_projection_reply_v4};

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
    InvalidReply(&'static str),
    Rotate,
}

#[cfg(test)]
std::thread_local! {
    static TEST_BROKER_TRACE_SINK: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Cached process-level `OMNITILER_TRACE=1` gate for broker trace-only
/// diagnostics (zero formatting/allocation/output when disabled).
pub(crate) fn broker_trace_enabled() -> bool {
    #[cfg(test)]
    if TEST_BROKER_TRACE_SINK.with(|sink| sink.borrow().is_some()) {
        return true;
    }
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("OMNITILER_TRACE").is_some_and(|v| v == "1"))
}

/// Best-effort trace-only broker diagnostic write (or isolated thread-local
/// capture in unit tests).
fn emit_broker_trace(line: String) {
    #[cfg(test)]
    {
        let captured = TEST_BROKER_TRACE_SINK.with(|sink| {
            let mut borrow = sink.borrow_mut();
            if let Some(lines) = borrow.as_mut() {
                lines.push(line.clone());
                true
            } else {
                false
            }
        });
        if captured {
            return;
        }
    }
    use std::io::Write as _;
    let _ = writeln!(std::io::stderr(), "{line}");
}

#[cfg(test)]
pub(crate) fn with_broker_trace_capture<F: FnOnce()>(f: F) -> Vec<String> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_BROKER_TRACE_SINK.with(|sink| {
                *sink.borrow_mut() = None;
            });
        }
    }
    TEST_BROKER_TRACE_SINK.with(|sink| {
        *sink.borrow_mut() = Some(Vec::new());
    });
    let _reset = Reset;
    f();
    TEST_BROKER_TRACE_SINK.with(|sink| sink.borrow_mut().take().unwrap_or_default())
}

struct BrokerPreviewTraceMeta {
    gen_id: String,
    req_id: String,
    rev: u64,
    obs: u64,
    preview_id: Option<u64>,
    domain_out: Option<String>,
    suspended_output: Option<String>,
}

impl BrokerPreviewTraceMeta {
    fn from_reply(reply: &tiler_core::world_report::WorldReplyV4) -> Option<Self> {
        if !broker_trace_enabled() {
            return None;
        }
        Some(Self {
            gen_id: crate::float_intent_store::sanitize_correlation(&reply.core_generation)
                .to_owned(),
            req_id: crate::float_intent_store::sanitize_correlation(&reply.request_id).to_owned(),
            rev: reply.state_revision,
            obs: reply.based_on_observation_seq,
            preview_id: reply.drag_preview.as_ref().map(|preview| preview.id),
            domain_out: reply
                .drag_preview
                .as_ref()
                .map(|preview| preview.domain.output.0.clone()),
            suspended_output: reply
                .presentation
                .suspended_output
                .as_ref()
                .map(|output| output.0.clone()),
        })
    }
}

/// Cap for remembered per-source bound capture outcomes (accepted replies
/// and in-band rejections alike). Oldest-first eviction: evicted sources
/// lose replay and refuse as stale through the sequence floor.
pub const MAX_REMEMBERED_SOURCES: usize = 1024;
/// Combined byte cap for exact retained request + bound reply payloads in
/// `responses`, enforced alongside `MAX_REMEMBERED_SOURCES`.
pub const MAX_REMEMBERED_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

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
    command: tiler_core::world_report::WorldCommand,
    suppress_orphan_result: bool,
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
struct PreparedCapture {
    bound: String,
    projection: CachedProjectionV4,
    preview_assoc_is_some: bool,
    has_preview_geometry: bool,
    suspended: bool,
    trace_meta: Option<BrokerPreviewTraceMeta>,
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
    presentation: Option<CachedProjectionV4>,
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
    #[cfg(test)]
    test_reply_fault: Option<fn(&mut tiler_core::world_report::WorldReplyV4)>,
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
    response_bytes: usize,
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
    #[cfg(test)]
    max_reply_bytes_override: Option<usize>,
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
            #[cfg(test)]
            test_reply_fault: None,
            watermarks: HashMap::new(),
            gesture_map: crate::kde_gesture::KdeGestureMap::new(),
            responses: HashMap::new(),
            response_order: VecDeque::new(),
            response_bytes: 0,
            wake_epoch: 0,
            waiter: None,
            wait_ready: None,
            #[cfg(test)]
            max_reply_bytes_override: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_reply_fault_for_test(
        &mut self,
        fault: fn(&mut tiler_core::world_report::WorldReplyV4),
    ) {
        self.test_reply_fault = Some(fault);
    }

    #[cfg(test)]
    pub(crate) fn set_max_reply_bytes_for_test(&mut self, limit: usize) {
        self.max_reply_bytes_override = Some(limit);
    }

    #[cfg(test)]
    pub(crate) fn reverse_pending_for_test(&mut self) {
        let mut reversed = std::collections::VecDeque::with_capacity(self.pending.len());
        while let Some(item) = self.pending.pop_back() {
            reversed.push_back(item);
        }
        self.pending = reversed;
    }

    #[cfg(test)]
    pub(crate) fn requeue_consumed_for_test(
        &mut self,
        sender: &str,
        client: &str,
        client_cmd: &str,
        global_seq: u64,
        command: tiler_core::world_report::WorldCommand,
    ) {
        self.pending.push_back(QueuedCommand {
            sender: sender.to_owned(),
            client: client.to_owned(),
            client_cmd: client_cmd.to_owned(),
            global_seq,
            command,
            suppress_orphan_result: false,
        });
    }

    #[cfg(test)]
    pub(crate) fn push_inadmissible_for_test(
        &mut self,
        sender: &str,
        client: &str,
        client_cmd: &str,
        command: tiler_core::world_report::WorldCommand,
    ) {
        let global_seq = self.next_command_seq;
        self.next_command_seq = self.next_command_seq.saturating_add(1);
        self.pending.push_back(QueuedCommand {
            sender: sender.to_owned(),
            client: client.to_owned(),
            client_cmd: client_cmd.to_owned(),
            global_seq,
            command,
            suppress_orphan_result: false,
        });
    }

    #[cfg(test)]
    pub(crate) fn gesture_map_debug(&self) -> String {
        format!("{:?}", self.gesture_map)
    }

    fn final_reply_max_bytes(&self) -> usize {
        #[cfg(test)]
        if let Some(limit) = self.max_reply_bytes_override {
            return limit;
        }
        WORLD_MAX_REPLY_BYTES
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
        let max_reply_bytes = self.final_reply_max_bytes();
        let reply = match tiler_protocol::connection_protocol::route_connection_request_bounded(
            &mut self.engine,
            input,
            max_reply_bytes,
        ) {
            Ok(reply) => reply,
            Err(e) => {
                if e.0 == "oversized" {
                    return Err(BrokerError::Oversized);
                }
                return Ok(self.setup_error_body("malformed", "request is malformed"));
            }
        };
        if reply.len() > max_reply_bytes {
            return Err(BrokerError::Oversized);
        }
        if let Ok(handshake) = tiler_protocol::connection_protocol::decode_handshake_reply(&reply) {
            if let Some(kind) = handshake.kind {
                let k = kind.as_str();
                return Ok(self.setup_error_body(k, k));
            }
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
            self.response_bytes = 0;
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

    /// Unbound in-band rejection for connection setup (`DiscoverWorld`)
    /// carrying `{v,outcome,kind,message,gen}` without capture watermark,
    /// clock, or gesture binding.
    #[must_use]
    pub fn setup_error_body(&self, kind: &str, message: &str) -> String {
        encode_error(kind, message, self.generation())
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
        // `focusChanges` is a core report lane, not a capture-only strip.
        // Its `after` names an incorporated core revision, never the
        // broker-stamped observation sequence or transport watermark.
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
        // decode failure, engine rejection)
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
        let mut parsed = ParsedCapture {
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
        self.stamp_inner(
            &mut parsed.inner,
            &parsed.adapter,
            &parsed.source,
            observation_seq,
            request_seq,
            clock,
        );
        if compact_json_exceeds_limit(&parsed.inner, WORLD_MAX_REQUEST_BYTES) {
            return Err(BrokerError::Oversized);
        }
        let mut plain_decoded = match decode_report_v4_map(&parsed.inner) {
            Ok(decoded) => decoded,
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
        // The gate above decoded without mutation; ignore any identifiable
        // future-stamped attempt rows in place on the transient decoded report
        // before dispatching through the batched core lane. Replay and
        // source-reuse remain anchored to the original envelope bytes above,
        // and each fresh source carries a fresh broker reqSeq.
        gate_decoded_attempts_by_clock(&mut plain_decoded.report, clock);
        // Parked FIFO batch: snapshot the whole queue for this fresh
        // capture. Commands arriving during this dispatch stay parked for
        // the next capture. Every entry is already a typed WorldCommand
        // decoded once at queue ingress; the batch drains inside ONE
        // capture/run/publication through the batched core lane, which
        // settles each command individually with a single final world.
        let drained: VecDeque<QueuedCommand> = std::mem::take(&mut self.pending);
        let batch: Vec<(u64, tiler_core::world_report::WorldCommand)> = drained
            .iter()
            .map(|queued| (queued.global_seq, queued.command.clone()))
            .collect();
        let wake_epoch = self.wake_epoch;
        let max_reply_bytes = self.final_reply_max_bytes();
        let mut prepared: Option<PreparedCapture> = None;
        #[cfg(test)]
        let test_reply_fault = self.test_reply_fault.take();
        #[cfg(test)]
        let engine_caps = self.engine.capabilities().cloned();
        let wake_precheck =
            |candidate: &tiler_core::world_engine::WakePrecheckCandidate<'_>| -> bool {
                let issued_count: u64 = if candidate.reply.ready {
                    drained
                        .iter()
                        .filter(|queued| {
                            candidate.dispositions.iter().any(|disp| {
                                disp.seq == queued.global_seq
                                    && matches!(
                                        disp.disposition,
                                        tiler_core::world_report::CommandDisposition::Executed(_)
                                            | tiler_core::world_report::CommandDisposition::Refused(
                                                _
                                            )
                                            | tiler_core::world_report::CommandDisposition::AlreadyConsumed
                                    )
                            })
                        })
                        .count() as u64
                } else {
                    0
                };
                wake_epoch.checked_add(issued_count).is_some()
            };
        let gate = tiler_protocol::world_admission::ProtocolAdmissionGate::with_residual(
            plain_decoded.residual.as_ref(),
        )
        .with_wake_precheck(&wake_precheck);
        let settled = &self.settled;
        let retired = &self.retired;
        let transition = match gate.dispatch_with_prepare(
            &mut self.engine,
            &plain_decoded.report,
            &batch,
            |candidate| {
                for disp in candidate.dispositions {
                    if matches!(
                        disp.disposition,
                        tiler_core::world_report::CommandDisposition::AlreadyConsumed
                    ) && let Some(queued) = drained.iter().find(|q| q.global_seq == disp.seq)
                    {
                        let key = queued.key();
                        if !queued.suppress_orphan_result
                            && !settled.contains_key(&key)
                            && !retired.contains_key(&key)
                        {
                            eprintln!(
                                "omnitiler:world-fault missing-prior-result seq={} client={}",
                                disp.seq, queued.client
                            );
                            return Err(BrokerError::Rotate);
                        }
                    }
                }
                #[cfg(test)]
                if let Some(fault) = test_reply_fault {
                    let mut mutated = candidate.reply.reply().clone();
                    fault(&mut mutated);
                    if let Err(err) = tiler_core::world_reply_validation::validate_reply_v4(
                        &mutated,
                        engine_caps.as_ref(),
                    ) {
                        let rule = if err.message == "request contains an unknown value" {
                            err.kind
                        } else {
                            err.message
                        };
                        return Err(BrokerError::InvalidReply(rule));
                    }
                }
                prepared = Some(prepare_capture_reply(
                    candidate,
                    true,
                    max_reply_bytes,
                    &parsed,
                    clock,
                    &mut staged_gesture_map,
                    gesture_assoc.as_ref(),
                )?);
                Ok(())
            },
        ) {
            Ok(result) => {
                if gate.take_wake_exhausted() {
                    self.repark_batch(drained);
                    return Err(BrokerError::Rotate);
                }
                if let Some(rule) = gate.take_invalid_reply() {
                    self.repark_batch(drained);
                    return Err(BrokerError::InvalidReply(rule));
                }
                result
            }
            Err(err) => {
                self.repark_batch(drained);
                return Err(err);
            }
        };
        self.finish_capture_outcome_prepared(
            &parsed,
            envelope,
            clock,
            covered,
            staged_gesture_map,
            drained,
            transition.incorporation,
            transition.outcome,
            transition.dispositions,
            prepared,
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn finish_capture_outcome(
        &mut self,
        parsed: &ParsedCapture,
        envelope: &str,
        clock: u64,
        covered: Option<CoverWitness>,
        mut staged_gesture_map: crate::kde_gesture::KdeGestureMap,
        gesture_assoc: Option<crate::kde_gesture::GestureAssoc>,
        drained: VecDeque<QueuedCommand>,
        outcome: WorldOutcome,
        dispositions: Vec<tiler_core::world_report::BatchDisposition>,
    ) -> Result<String, BrokerError> {
        use tiler_core::world_report::ReportIncorporation;
        let incorporation = match &outcome {
            WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
                if reply.ready {
                    ReportIncorporation::Incorporated
                } else {
                    ReportIncorporation::BootstrapIntentOnly
                }
            }
            WorldOutcome::Duplicate(_) => ReportIncorporation::Replay,
            WorldOutcome::Rejected { .. } => ReportIncorporation::Rejected,
        };
        let prepared = match &outcome {
            WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
                Some(prepare_capture_reply_checked(
                    reply,
                    self.engine.active_gesture_id(),
                    true,
                    self.final_reply_max_bytes(),
                    parsed,
                    clock,
                    &mut staged_gesture_map,
                    gesture_assoc.as_ref(),
                )?)
            }
            WorldOutcome::Duplicate(_) | WorldOutcome::Rejected { .. } => None,
        };
        self.finish_capture_outcome_prepared(
            parsed,
            envelope,
            clock,
            covered,
            staged_gesture_map,
            drained,
            incorporation,
            outcome,
            dispositions,
            prepared,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_capture_outcome_prepared(
        &mut self,
        parsed: &ParsedCapture,
        envelope: &str,
        clock: u64,
        covered: Option<CoverWitness>,
        staged_gesture_map: crate::kde_gesture::KdeGestureMap,
        drained: VecDeque<QueuedCommand>,
        incorporation: tiler_core::world_report::ReportIncorporation,
        outcome: WorldOutcome,
        dispositions: Vec<tiler_core::world_report::BatchDisposition>,
        prepared: Option<PreparedCapture>,
    ) -> Result<String, BrokerError> {
        let prepared = match outcome {
            WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
                if reply.ready {
                    self.settle_batch_infallible(&drained, &dispositions);
                } else {
                    // Pre-ready bootstrap can install pending intent without
                    // advancing replay, and never executes the drained batch.
                    self.repark_batch(drained);
                }
                prepared.expect("prepare callback runs before Accepted/CommandConsumed adoption")
            }
            WorldOutcome::Duplicate(reply) => {
                // Defensive core duplicate returns cached publication with no
                // transition: repark drained batch and return without
                // installing gesture_map, projection, coverage, watermark,
                // or source replay.
                self.repark_batch(drained);
                let bind = BrokerReplyBind {
                    source: &parsed.source,
                    native_invalid: parsed.native_invalid,
                    clock,
                    gesture_assoc: None,
                    gesture_settled_assoc: None,
                };
                // Defensive core replay is for the latest adopted candidate;
                // its retained roster plan is still the one published with it.
                let bound =
                    write_bound_reply_v4(&reply, self.engine.desired_shared_roster_plan(), &bind)
                        .map_err(map_protocol_reply_error)?;
                if bound.len() > self.final_reply_max_bytes() {
                    return Err(BrokerError::Oversized);
                }
                return Ok(bound);
            }
            WorldOutcome::Rejected { kind, message } => {
                if incorporation != tiler_core::world_report::ReportIncorporation::NotAdmitted
                    && !drained.is_empty()
                    && kind == "snapshot-invalid"
                    && (message == "command-invalid" || message == "command-order-invalid")
                {
                    eprintln!("omnitiler:world-fault internal-drain kind={kind} message={message}");
                    return Err(BrokerError::Rotate);
                }
                self.repark_batch(drained);
                return self.remember_rejection(parsed, envelope, kind, message, clock);
            }
        };
        // Staged gesture map, projection, coverage, watermark, and source
        // replay install only after final preparation and settlement succeed.
        self.gesture_map = staged_gesture_map;
        if let Some(meta) = prepared.trace_meta {
            let decision = if prepared.suspended {
                "suspended"
            } else if meta.preview_id.is_none() {
                "absent"
            } else if !prepared.has_preview_geometry {
                "malformed"
            } else if prepared.preview_assoc_is_some {
                "projected-proven"
            } else {
                "projected-unproven"
            };
            let core_preview = if meta.preview_id.is_some() {
                "present"
            } else {
                "none"
            };
            let id_str = meta
                .preview_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_owned());
            let domain_out = meta
                .domain_out
                .as_deref()
                .map(crate::float_intent_store::sanitize_correlation)
                .unwrap_or("-");
            let suspended_out = meta
                .suspended_output
                .as_deref()
                .map(crate::float_intent_store::sanitize_correlation)
                .unwrap_or("-");
            let assoc_state = if !prepared.has_preview_geometry {
                "-"
            } else if prepared.preview_assoc_is_some {
                "proven"
            } else {
                "none"
            };
            emit_broker_trace(format!(
                "omnitiler:broker:trace ev=drag-preview-projection gen={} req={} rev={} obs={} corePreview={} id={} domainOut={} suspendedOutput={} assoc={} decision={}",
                meta.gen_id,
                meta.req_id,
                meta.rev,
                meta.obs,
                core_preview,
                id_str,
                domain_out,
                suspended_out,
                assoc_state,
                decision,
            ));
        }
        self.presentation = Some(prepared.projection);
        self.presentation_covered = covered;
        self.watermarks
            .insert(parsed.adapter.clone(), parsed.native_invalid);
        self.remember_response(&parsed.source, envelope.as_bytes(), &prepared.bound);
        Ok(prepared.bound)
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
        adapter: &str,
        source: &str,
        observation_seq: u64,
        request_seq: u64,
        clock: u64,
    ) {
        inner.insert("adapter".to_owned(), json_string(adapter));
        inner.insert("gen".to_owned(), json_string(self.generation()));
        inner.insert("obs".to_owned(), json_string(&observation_seq.to_string()));
        inner.insert("reqSeq".to_owned(), json_string(&request_seq.to_string()));
        inner.insert("req".to_owned(), json_string(source));
        inner.insert("cmdSeq".to_owned(), serde_json::Value::Null);
        inner.insert("command".to_owned(), serde_json::Value::Null);
        inner.insert("monotonicMs".to_owned(), json_string(&clock.to_string()));
    }

    /// Settle one drained batch command terminally without queue motion
    /// (the batch snapshot already left `pending`): ready carries the core
    /// result, failed carries the explicit rejection kind/message. Each
    /// settlement advances the wake epoch and completes a parked waiter
    /// while work remains.
    fn advance_wake_after_drain(&mut self) {
        self.wake_epoch = self
            .wake_epoch
            .checked_add(1)
            .expect("wake_epoch prechecked before adoption");
        self.complete_waiter_if_work();
    }

    fn settle_drained_infallible(&mut self, queued: &QueuedCommand, settlement: Settlement) {
        if !queued.suppress_orphan_result {
            self.insert_settlement(queued.key(), settlement);
        }
        self.advance_wake_after_drain();
    }

    /// Settle one drained FIFO batch from the core per-command settlements.
    /// Every drained entry with a matching settlement settles terminally: a
    /// typed `Rejected` result fails with the core kind (never a null
    /// result masquerading as success), any other result readies with the
    /// encoded outcome. Drained entries without a settlement were never
    /// executed and repark at the head in order (see `repark_batch`).
    #[cfg(test)]
    fn settle_batch(
        &mut self,
        drained: &VecDeque<QueuedCommand>,
        dispositions: &[tiler_core::world_report::BatchDisposition],
    ) -> Result<(), BrokerError> {
        let issued_count = drained
            .iter()
            .filter(|q| {
                dispositions.iter().any(|d| {
                    d.seq == q.global_seq
                        && matches!(
                            d.disposition,
                            tiler_core::world_report::CommandDisposition::Executed(_)
                                | tiler_core::world_report::CommandDisposition::Refused(_)
                                | tiler_core::world_report::CommandDisposition::AlreadyConsumed
                        )
                })
            })
            .count() as u64;
        if self.wake_epoch.checked_add(issued_count).is_none() {
            return Err(BrokerError::Rotate);
        }
        self.settle_batch_infallible(drained, dispositions);
        Ok(())
    }

    fn settle_batch_infallible(
        &mut self,
        drained: &VecDeque<QueuedCommand>,
        dispositions: &[tiler_core::world_report::BatchDisposition],
    ) {
        let mut by_seq: HashMap<u64, &tiler_core::world_report::CommandDisposition> =
            HashMap::new();
        for item in dispositions {
            by_seq.insert(item.seq, &item.disposition);
        }
        let mut repark: Vec<QueuedCommand> = Vec::new();
        for queued in drained {
            let Some(disposition) = by_seq.get(&queued.global_seq) else {
                repark.push(queued.clone());
                continue;
            };
            match disposition {
                tiler_core::world_report::CommandDisposition::NotRun => {
                    repark.push(queued.clone());
                }
                _ if queued.suppress_orphan_result => {
                    self.advance_wake_after_drain();
                }
                tiler_core::world_report::CommandDisposition::Executed(result) => match result {
                    Some(tiler_core::world_report::CommandResult::Rejected { kind }) => {
                        let kind_str = kind.as_str();
                        self.settle_drained_infallible(
                            queued,
                            Settlement::Failed {
                                global_seq: queued.global_seq,
                                kind: kind_str.to_owned(),
                                message: kind_str.to_owned(),
                            },
                        );
                    }
                    other => {
                        self.settle_drained_infallible(
                            queued,
                            Settlement::Ready {
                                global_seq: queued.global_seq,
                                result: encode_result_value(other),
                            },
                        );
                    }
                },
                tiler_core::world_report::CommandDisposition::Refused(kind) => {
                    let kind_str = kind.as_str();
                    self.settle_drained_infallible(
                        queued,
                        Settlement::Failed {
                            global_seq: queued.global_seq,
                            kind: kind_str.to_owned(),
                            message: kind_str.to_owned(),
                        },
                    );
                }
                tiler_core::world_report::CommandDisposition::AlreadyConsumed => {
                    let key = queued.key();
                    let prior = self
                        .settled
                        .get(&key)
                        .cloned()
                        .or_else(|| self.retired.get(&key).cloned())
                        .expect("AlreadyConsumed prior result prechecked before adoption");
                    self.settle_drained_infallible(queued, prior);
                }
            }
        }
        self.repark_batch_vec(repark);
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
        let (generation, client, client_cmd, command_value) = match parse_queue_request(request) {
            Ok(parsed) => parsed,
            Err((kind, message)) => return Ok(self.error_body(kind, message)),
        };
        if generation != self.generation() {
            return Ok(self.error_body("generation-mismatch", "generation mismatch"));
        }
        let Ok(command) = decode_command_value_v4(&command_value) else {
            return Ok(self.error_body("snapshot-invalid", "command-invalid"));
        };
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
            suppress_orphan_result: false,
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

    /// Retire all per-client high-water marks and settled/retired results
    /// belonging to a proved-lost D-Bus unique sender. Already-filed pending
    /// commands remain in FIFO order so committed work still executes, while
    /// post-loss settlements are suppressed rather than published as orphans.
    pub fn retire_sender(&mut self, sender: &str) {
        let before = self.highwater.len();
        self.highwater.retain(|(s, _), _| s != sender);
        if self.highwater.len() == before {
            return;
        }
        self.settled.retain(|(s, _, _), _| s != sender);
        self.settled_order.retain(|(s, _, _)| s != sender);
        self.retired.retain(|(s, _, _), _| s != sender);
        self.retired_order.retain(|(s, _, _)| s != sender);
        for queued in &mut self.pending {
            if queued.sender == sender {
                queued.suppress_orphan_result = true;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn settled_and_retired_counts_for_sender(&self, sender: &str) -> (usize, usize) {
        let settled = self.settled.keys().filter(|(s, _, _)| s == sender).count();
        let retired = self.retired.keys().filter(|(s, _, _)| s == sender).count();
        (settled, retired)
    }

    #[cfg(test)]
    pub(crate) fn has_highwater_for_sender(&self, sender: &str) -> bool {
        self.highwater.keys().any(|(s, _)| s == sender)
    }

    /// Remember one per-source bound capture outcome (accepted bound reply
    /// or bound in-band rejection) keyed by exact envelope bytes. Bounded
    /// oldest-first eviction by both `MAX_REMEMBERED_SOURCES` and
    /// `MAX_REMEMBERED_RESPONSE_BYTES`: evicted sources lose replay and refuse
    /// as stale through the sequence floor, never re-dispatching forgotten
    /// facts. A pair exceeding `MAX_REMEMBERED_RESPONSE_BYTES` is never
    /// copied or retained.
    fn remember_response(&mut self, source: &str, bytes: &[u8], bound: &str) {
        let Some(pair_bytes) = bytes.len().checked_add(bound.len()) else {
            return;
        };
        if pair_bytes > MAX_REMEMBERED_RESPONSE_BYTES {
            return;
        }
        if let Some((prior_req, prior_rep)) = self.responses.remove(source) {
            let prior_bytes = prior_req
                .len()
                .checked_add(prior_rep.len())
                .expect("retained pair_bytes fits usize");
            self.response_bytes = self
                .response_bytes
                .checked_sub(prior_bytes)
                .expect("response_bytes covers retained entry");
            self.response_order.retain(|item| item != source);
        }
        while self.responses.len() >= MAX_REMEMBERED_SOURCES
            || self
                .response_bytes
                .checked_add(pair_bytes)
                .is_none_or(|total| total > MAX_REMEMBERED_RESPONSE_BYTES)
        {
            let oldest = self
                .response_order
                .pop_front()
                .expect("response_order non-empty while responses exceeds cap");
            let (evicted_req, evicted_rep) = self
                .responses
                .remove(&oldest)
                .expect("response_order key present in responses");
            let evicted_bytes = evicted_req
                .len()
                .checked_add(evicted_rep.len())
                .expect("retained pair_bytes fits usize");
            self.response_bytes = self
                .response_bytes
                .checked_sub(evicted_bytes)
                .expect("response_bytes covers retained entry");
        }
        self.response_order.push_back(source.to_owned());
        self.responses
            .insert(source.to_owned(), (bytes.to_vec(), bound.to_owned()));
        self.response_bytes = self
            .response_bytes
            .checked_add(pair_bytes)
            .expect("response_bytes prechecked against MAX_REMEMBERED_RESPONSE_BYTES");
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

/// Count the same compact serde JSON encoding used for a stamped wire report.
fn compact_json_exceeds_limit(map: &Map<String, serde_json::Value>, limit: usize) -> bool {
    struct CountingSink {
        count: usize,
        limit: usize,
    }

    impl std::io::Write for CountingSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.count = self.count.saturating_add(buf.len());
            if self.count > self.limit {
                return Err(std::io::ErrorKind::WriteZero.into());
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut sink = CountingSink { count: 0, limit };
    serde_json::to_writer(&mut sink, map).is_err()
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
    let value = tiler_protocol::connection_protocol::parse_strict_json(envelope)
        .map_err(|_| failure("malformed", "request is malformed"))?;
    let serde_json::Value::Object(mut map) = value else {
        return Err(failure("malformed", "request is malformed"));
    };
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
    let mut take_string = |key, valid: fn(&str) -> bool| match map.remove(key) {
        Some(serde_json::Value::String(text)) if valid(&text) => Ok(text),
        _ => Err(failure(
            "unknown-value",
            "request contains an unknown value",
        )),
    };
    let adapter = take_string("adapter", is_valid_adapter_id)?;
    let generation = take_string("gen", is_generation_token)?;
    let source = take_string("source", is_valid_request_id)?;
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
    let bound = |source, kind, message| CaptureFailure {
        source,
        native_invalid: watermark,
        kind,
        message,
    };
    let inner = match map.remove("report") {
        Some(serde_json::Value::Object(inner)) => inner,
        _ => return Err(bound(source, "malformed", "request is malformed")),
    };
    if inner.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(BROKER_VERSION)) {
        return Err(bound(
            source,
            "unsupported-version",
            "unsupported contract version",
        ));
    }
    if BROKER_KEYS.iter().any(|k| inner.contains_key(*k)) {
        return Err(bound(
            source,
            "unknown-field",
            "request contains an unknown field",
        ));
    }
    Ok(ParsedCapture {
        adapter,
        generation,
        source,
        source_seq,
        native_invalid: watermark,
        inner,
    })
}

/// Strict queue-request parse: exact keys, v4, serving generation binding,
/// opaque client, canonical per-client command id, object command.
fn parse_queue_request(
    request: &str,
) -> Result<(String, String, u64, serde_json::Value), (&'static str, &'static str)> {
    let value = tiler_protocol::connection_protocol::parse_strict_json(request)
        .map_err(|_| ("malformed", "request is malformed"))?;
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
    let value = tiler_protocol::connection_protocol::parse_strict_json(request)
        .map_err(|_| ("malformed", "request is malformed"))?;
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
    let value = tiler_protocol::connection_protocol::parse_strict_json(request)
        .map_err(|_| ("malformed", "request is malformed"))?;
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
    let value = tiler_protocol::connection_protocol::parse_strict_json(request)
        .map_err(|_| ("malformed", "request is malformed"))?;
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

fn to_broker_assoc(assoc: &crate::kde_gesture::GestureAssoc) -> BrokerGestureAssoc<'_> {
    BrokerGestureAssoc {
        instance: &assoc.instance,
        serial: &assoc.serial,
        core_id: assoc.core_id,
    }
}

/// Checked canonical encode of one v4 engine reply for the broker lane.
/// Never serves a partial reply if encoding or the byte gate fails.
#[allow(clippy::too_many_arguments)]
fn prepare_capture_reply(
    candidate: tiler_core::world_engine::PreparedCandidate<'_>,
    reconcile_pins: bool,
    max_reply_bytes: usize,
    parsed: &ParsedCapture,
    clock: u64,
    staged_gesture_map: &mut crate::kde_gesture::KdeGestureMap,
    gesture_assoc: Option<&crate::kde_gesture::GestureAssoc>,
) -> Result<PreparedCapture, BrokerError> {
    let validated = candidate.reply;
    let reply = validated.reply();
    let trace_meta = BrokerPreviewTraceMeta::from_reply(reply);
    let preview_id = reply.drag_preview.as_ref().map(|preview| preview.id);
    if reconcile_pins {
        staged_gesture_map.reconcile_pins(
            candidate.active_gesture_id,
            reply.gesture_settled.map(|settled| settled.id),
            gesture_assoc,
        );
    }
    let settled_assoc = staged_gesture_map
        .terminal_assoc_for(reply.gesture_settled.map(|settled| settled.id))
        .cloned();
    let bind = BrokerReplyBind {
        source: &parsed.source,
        native_invalid: parsed.native_invalid,
        clock,
        gesture_assoc: gesture_assoc.map(to_broker_assoc),
        gesture_settled_assoc: settled_assoc.as_ref().map(to_broker_assoc),
    };
    let bound = write_bound_reply_v4_prevalidated(validated, candidate.shared_roster_plan, &bind)
        .map_err(map_protocol_reply_error)?;
    if bound.len() > max_reply_bytes {
        return Err(BrokerError::Oversized);
    }
    let suspended = reply.drag_preview.as_ref().is_some_and(|preview| {
        reply
            .presentation
            .suspended_output
            .as_ref()
            .is_some_and(|suspended| preview.domain.output == *suspended)
    });
    let has_preview_geometry = !suspended && reply.drag_preview.is_some();
    let preview_assoc = staged_gesture_map.active_assoc_for(preview_id);
    let producer = if has_preview_geometry {
        preview_assoc.map(|assoc| (assoc.instance.as_str(), assoc.serial.as_str()))
    } else {
        None
    };
    let projection = write_cached_projection_v4_prevalidated(validated, producer)
        .map_err(map_protocol_reply_error)?;
    Ok(PreparedCapture {
        bound,
        projection,
        preview_assoc_is_some: preview_assoc.is_some(),
        has_preview_geometry,
        suspended,
        trace_meta,
    })
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn prepare_capture_reply_checked(
    reply: &tiler_core::world_report::WorldReplyV4,
    active_gesture_id: Option<u64>,
    reconcile_pins: bool,
    max_reply_bytes: usize,
    parsed: &ParsedCapture,
    clock: u64,
    staged_gesture_map: &mut crate::kde_gesture::KdeGestureMap,
    gesture_assoc: Option<&crate::kde_gesture::GestureAssoc>,
) -> Result<PreparedCapture, BrokerError> {
    let trace_meta = BrokerPreviewTraceMeta::from_reply(reply);
    let preview_id = reply.drag_preview.as_ref().map(|preview| preview.id);
    if reconcile_pins {
        staged_gesture_map.reconcile_pins(
            active_gesture_id,
            reply.gesture_settled.map(|settled| settled.id),
            gesture_assoc,
        );
    }
    let settled_assoc = staged_gesture_map
        .terminal_assoc_for(reply.gesture_settled.map(|settled| settled.id))
        .cloned();
    let bind = BrokerReplyBind {
        source: &parsed.source,
        native_invalid: parsed.native_invalid,
        clock,
        gesture_assoc: gesture_assoc.map(to_broker_assoc),
        gesture_settled_assoc: settled_assoc.as_ref().map(to_broker_assoc),
    };
    let bound = write_bound_reply_v4(reply, None, &bind).map_err(map_protocol_reply_error)?;
    if bound.len() > max_reply_bytes {
        return Err(BrokerError::Oversized);
    }
    let suspended = reply.drag_preview.as_ref().is_some_and(|preview| {
        reply
            .presentation
            .suspended_output
            .as_ref()
            .is_some_and(|suspended| preview.domain.output == *suspended)
    });
    let has_preview_geometry = !suspended && reply.drag_preview.is_some();
    let preview_assoc = staged_gesture_map.active_assoc_for(preview_id);
    let producer = if has_preview_geometry {
        preview_assoc.map(|assoc| (assoc.instance.as_str(), assoc.serial.as_str()))
    } else {
        None
    };
    let projection =
        write_cached_projection_v4(reply, producer).map_err(map_protocol_reply_error)?;
    Ok(PreparedCapture {
        bound,
        projection,
        preview_assoc_is_some: preview_assoc.is_some(),
        has_preview_geometry,
        suspended,
        trace_meta,
    })
}

fn map_protocol_reply_error(
    err: tiler_protocol::world_protocol::WorldProtocolError,
) -> BrokerError {
    let rule = if err.message == "request contains an unknown value" {
        err.kind
    } else {
        err.message
    };
    BrokerError::InvalidReply(rule)
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
        // Broker-owned bound actuator associations default to null here;
        // the accepted-reply path overwrites them via `inject_gesture_assocs`
        // before binding. Rejections and errors always carry null (never a
        // fabricated binding), so the script can require the association
        // without a separate absence proof.
        if !map.contains_key("gestureAssoc") {
            map.insert("gestureAssoc".to_owned(), serde_json::Value::Null);
        }
        if !map.contains_key("gestureSettledAssoc") {
            map.insert("gestureSettledAssoc".to_owned(), serde_json::Value::Null);
        }
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
    tiler_protocol::world_command_result::encode_command_result(&mut encoded, result);
    serde_json::from_str(&encoded).unwrap_or(serde_json::Value::Null)
}

/// One typed stamp comparison pass over decoded attempt evidence against the
/// authoritative bound broker dispatch `clock`.
///
/// Retains `attempt_ms <= clock` in place for `frame` (`Some`), `member`,
/// `view`, `state`, and `creation`. Null frame `attempt_ms` is skipped,
/// `attempt_id` is identity only, and `visibility` is unstamped. Identifiable
/// rows with `attempt_ms > clock` are ignored as evidence (cannot settle)
/// while valid siblings remain; already-held codec rows and protocol
/// residuals stay untouched.
///
/// The decoded report is transient for local core dispatch only: the broker
/// enforces byte-exact replay / `source-reused` against the original envelope
/// before decode and assigns a fresh `reqSeq` to each fresh source, so
/// ignoring clock-invalid rows never mutates raw capture bytes or conflates
/// distinct requests.
fn gate_decoded_attempts_by_clock(
    report: &mut tiler_core::world_report::WorldReportV4,
    clock: u64,
) {
    report
        .frames
        .retain(|attempt| attempt.attempt_ms.is_none_or(|ms| ms <= clock));
    report
        .member_attempts
        .retain(|attempt| attempt.attempt_ms <= clock);
    report
        .view_attempts
        .retain(|attempt| attempt.attempt_ms <= clock);
    report
        .state_attempts
        .retain(|attempt| attempt.attempt_ms <= clock);
    report
        .creation_attempts
        .retain(|attempt| attempt.attempt_ms <= clock);
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

/// Renderer-carried drop preview: the paint geometry plus subject proof,
/// with the producing native gesture when proven. Unproven previews ride
/// with null producer fields and the renderer hides them (fail closed).
#[cfg(test)]
#[derive(Debug, Clone)]
struct CarriedPreview {
    geometry: serde_json::Value,
    producer: Option<(String, String)>,
}

#[cfg(test)]
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
/// resolution (pointer) domain output of the retained `dragPreview`.
/// Mechanical consumption of the core-published verdict, no broker policy:
/// the mover-home source may differ, so a cross-output preview resolving
/// on another output stays intact.
#[cfg(test)]
fn preview_suspended_by_presentation(encoded: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(encoded)
        .ok()
        .and_then(|value| {
            let preview = value.get("dragPreview")?.as_object()?;
            let domain_out = preview
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
/// and the insertion-neighbor `win`) when the reply carried a retained
/// `dragPreview`, else `None`. Pure over the encoded reply; never touches
/// the engine. Core provenance (the `id`, the `hover` chain, the session
/// `rev`, the resolution `domainOut`/`domainWs`) never rides: the
/// subscriber paints from these fields plus the cached
/// snapshot/scopes/presentation, vets staleness through the projection
/// generation/revision/coverage, and applies live hide-only vetoes at
/// dispatch.
/// Malformed shapes fail closed to `None`, never a fabricated preview.
#[cfg(test)]
fn drag_preview_of(encoded: &str) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(encoded).ok()?;
    let preview = value.get("dragPreview")?.as_object()?;
    let rect = preview.get("rect")?;
    if !rect
        .as_array()
        .is_some_and(|items| items.len() == 4 && items.iter().all(|item| item.as_i64().is_some()))
    {
        return None;
    }
    let source_out = preview
        .get("sourceOut")
        .and_then(serde_json::Value::as_str)?;
    let source_ws = preview
        .get("sourceWs")
        .and_then(serde_json::Value::as_str)?;
    let win = preview.get("win").and_then(serde_json::Value::as_str)?;
    Some(serde_json::json!({
        "rect": rect,
        "sourceOut": source_out,
        "sourceWs": source_ws,
        "win": win,
    }))
}

/// Project one accepted reply for the presentation cache: snapshot plus
/// presentation plus the renderer-only drop preview, with the native write
/// tokens (`frame_op`) stripped from every window row.
/// One-shot lanes (transfers, effects, settlements, warp) never enter, and
/// `dragPreview` enters only through the carried `preview` (paint `rect`,
/// mover-home source, insertion-neighbor window, producing-gesture identity
/// or nulls when unproven; null when no drop preview was published) plus
/// the causal coverage bound at read time. The `gen`/`req` binding is added
/// at read time.
#[cfg(test)]
fn project_reply(encoded: &str, preview: Option<&CarriedPreview>) -> String {
    let value: serde_json::Value = serde_json::from_str(encoded).unwrap_or_default();
    let get = |key: &str| value.get(key).cloned().unwrap_or(serde_json::Value::Null);
    let mut windows = get("windows");
    if let Some(rows) = windows.as_array_mut() {
        for row in rows {
            if let Some(map) = row.as_object_mut() {
                map.remove("frame_op");
                if let Some(states) = map
                    .get_mut("state")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    states.retain(|item| {
                        item.get("automaticClear") != Some(&serde_json::json!(true))
                    });
                }
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
    cached: &CachedProjectionV4,
    generation: &str,
    req: &str,
    covered: Option<&CoverWitness>,
) -> String {
    cached.bind_read(
        generation,
        req,
        covered.map(|witness| (witness.instance.as_str(), witness.counter.as_str())),
    )
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
    fn capture_envelope_rejects_duplicate_outer_and_nested_keys() {
        let dup_outer = r#"{"v":1,"v":4,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":4}}"#;
        let failure = parse_capture_envelope(dup_outer).expect_err("duplicate outer key refuses");
        assert_eq!(failure.kind, "malformed");
        assert_eq!(failure.source, "");
        assert_eq!(failure.native_invalid, 0);

        let dup_report = r#"{"v":4,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":1,"v":4}}"#;
        let failure = parse_capture_envelope(dup_report).expect_err("duplicate report key refuses");
        assert_eq!(failure.kind, "malformed");
        assert_eq!(failure.source, "");
        assert_eq!(failure.native_invalid, 0);

        let dup_nested = r#"{"v":4,"adapter":"a","gen":"w-1","source":"src-1","sourceSeq":"1","nativeInvalid":"3","report":{"v":4,"nested":{"k":1,"k":2}}}"#;
        let failure = parse_capture_envelope(dup_nested).expect_err("duplicate nested key refuses");
        assert_eq!(failure.kind, "malformed");
        assert_eq!(failure.source, "");
        assert_eq!(failure.native_invalid, 0);
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
    fn projection_drops_write_tokens_and_one_shot_lanes() {
        let reply = "{\"v\":4,\"rev\":\"3\",\"obs\":\"2\",\"scopes\":[],\"workspaces\":[],\
            \"outputs\":[],\"windows\":[{\"win\":\"w\",\"frame_op\":\"op-1\",\
            \"alloc\":null}],\"transfers\":[{}],\"effects\":[{}],\"dragPreview\":null,\
            \"operationSettled\":[{}],\"gestureSettled\":null,\"presentation\":{}}";
        let unproven = CarriedPreview::unproven(drag_preview_of(reply));
        let projection: serde_json::Value =
            serde_json::from_str(&project_reply(reply, unproven.as_ref())).expect("JSON");
        let window = &projection["windows"][0];
        assert!(window.get("frame_op").is_none());
        assert_eq!(window["win"], "w");
        for key in [
            "dragPreview",
            "transfers",
            "effects",
            "operationSettled",
            "gestureSettled",
        ] {
            assert!(projection.get(key).is_none(), "{key}");
        }
        assert!(projection["preview"].is_null());
        assert_eq!(projection["rev"], "3");
    }

    #[test]
    fn projection_carries_only_the_trimmed_drop_preview_geometry() {
        let preview = "{\"id\":\"1\",\"rev\":\"7\",\"domainOut\":\"out-1\",\
            \"domainWs\":\"ws-1\",\"sourceOut\":\"out-1\",\"sourceWs\":\"ws-1\",\
            \"rect\":[0,0,396,600],\"hover\":null,\"win\":\"win-a\"}";
        let reply = format!(
            "{{\"v\":4,\"rev\":\"3\",\"obs\":\"2\",\"scopes\":[],\"workspaces\":[],\
            \"outputs\":[],\"windows\":[],\"transfers\":[],\"effects\":[],\"dragPreview\":{preview},\
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
        for excluded in ["id", "hover", "rev", "domainOut", "domainWs"] {
            assert!(
                geometry.get(excluded).is_none(),
                "provenance field {excluded} never rides the preview"
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
        // Null dragPreview carries no preview.
        let cleared_reply = reply.replace(preview, "null");
        assert!(drag_preview_of(&cleared_reply).is_none());
        let cleared: serde_json::Value = serde_json::from_str(&project_reply(
            &cleared_reply,
            CarriedPreview::unproven(drag_preview_of(&cleared_reply)).as_ref(),
        ))
        .expect("JSON");
        assert!(cleared["preview"].is_null());
        // Missing and malformed preview shapes fail closed to null, never a
        // fabricated geometry: non-object, no rect, a non-quad rect, or a
        // missing subject identity carries nothing.
        assert!(drag_preview_of("{\"v\":4}").is_none());
        assert!(drag_preview_of("not json").is_none());
        let non_object = reply.replace(preview, "\"bad\"");
        assert!(drag_preview_of(&non_object).is_none());
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
        // domain omits the renderer preview.
        let suspended = "{\"v\":4,\"rev\":\"3\",\"obs\":\"2\",\"scopes\":[],\"workspaces\":[],\
            \"outputs\":[],\"windows\":[],\"transfers\":[],\"effects\":[],\
            \"dragPreview\":{\"id\":\"1\",\"rev\":\"7\",\"domainOut\":\"out-1\",\
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
        // No verdict or no dragPreview never suspends.
        let plain = suspended.replace(",\"suspendedOutput\":\"out-1\"", "");
        assert!(!preview_suspended_by_presentation(&plain));
        let no_preview = suspended.replace(
            "\"dragPreview\":{\"id\":\"1\",\"rev\":\"7\",\"domainOut\":\"out-1\",\
            \"domainWs\":\"ws-1\",\"sourceOut\":\"out-2\",\"sourceWs\":\"ws-2\",\
            \"rect\":[0,0,396,600],\"hover\":null,\"win\":\"win-a\"}",
            "\"dragPreview\":null",
        );
        assert!(!preview_suspended_by_presentation(&no_preview));
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
    fn requester_serializer_preserves_all_non_preview_result_vocabulary() {
        use tiler_core::directional::{Direction, OutputId, WindowId, WorkspaceId};
        use tiler_core::geometry::Rect;
        use tiler_core::session::DomainKey;
        use tiler_core::world_directional::DirectionalOutcome;
        use tiler_core::world_report::{
            BatchDisposition, CommandDisposition, CommandRejectKind, CommandResult,
        };

        let mut broker = WorldBroker::new();
        let generation = broker.generation().to_owned();
        let sender = ":1.99";
        let mut next_client_cmd = 1u64;

        let mut settle_and_take = |result: Option<CommandResult>| -> serde_json::Value {
            let client_cmd = next_client_cmd.to_string();
            next_client_cmd += 1;
            let queue_req = format!(
                "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"vocab\",\"clientCmd\":\"{client_cmd}\",\
                  \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
            );
            let queued: serde_json::Value =
                serde_json::from_str(&broker.queue_command(sender, &queue_req).expect("queued"))
                    .expect("JSON");
            assert_eq!(queued["outcome"], "queued");
            let drained = std::mem::take(&mut broker.pending);
            assert_eq!(drained.len(), 1);
            let seq = drained[0].global_seq;
            let disposition = match result {
                Some(CommandResult::Rejected { kind }) => CommandDisposition::Refused(kind),
                other => CommandDisposition::Executed(other),
            };
            broker
                .settle_batch(&drained, &[BatchDisposition { seq, disposition }])
                .expect("settle");
            let take_req = format!(
                "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"vocab\",\"clientCmd\":\"{client_cmd}\"}}"
            );
            serde_json::from_str(&broker.take_result(sender, &take_req).expect("take"))
                .expect("JSON")
        };

        // None -> ready with null result.
        let none_take = settle_and_take(None);
        assert_eq!(none_take["outcome"], "ready");
        assert!(none_take["result"].is_null());

        // Float placement, no-space, and unfloat.
        let placed = settle_and_take(Some(CommandResult::Float {
            window: WindowId("win-a".to_owned()),
            rect: Some(Rect {
                x: 10,
                y: 20,
                w: 640,
                h: 480,
            }),
            no_space: false,
        }));
        assert_eq!(placed["outcome"], "ready");
        assert_eq!(placed["result"]["op"], "toggle-float");
        assert_eq!(placed["result"]["win"], "win-a");
        assert_eq!(
            placed["result"]["rect"],
            serde_json::json!([10, 20, 640, 480])
        );
        assert!(placed["result"].get("noSpace").is_none());

        let no_space = settle_and_take(Some(CommandResult::Float {
            window: WindowId("win-a".to_owned()),
            rect: None,
            no_space: true,
        }));
        assert_eq!(no_space["outcome"], "ready");
        assert_eq!(no_space["result"]["op"], "toggle-float");
        assert_eq!(no_space["result"]["win"], "win-a");
        assert!(no_space["result"]["rect"].is_null());
        assert_eq!(no_space["result"]["noSpace"], true);

        let unfloat = settle_and_take(Some(CommandResult::Float {
            window: WindowId("win-a".to_owned()),
            rect: None,
            no_space: false,
        }));
        assert_eq!(unfloat["outcome"], "ready");
        assert_eq!(unfloat["result"]["win"], "win-a");
        assert!(unfloat["result"]["rect"].is_null());
        assert!(unfloat["result"].get("noSpace").is_none());

        // Migration.
        let migration = settle_and_take(Some(CommandResult::Migration {
            direction: Direction::Right,
            source: DomainKey {
                output: OutputId("out-b".to_owned()),
                workspace: WorkspaceId("ws-a".to_owned()),
            },
            target: DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId("ws-a".to_owned()),
            },
            members: vec![WindowId("win-a".to_owned())],
            floats: 0,
        }));
        assert_eq!(migration["outcome"], "ready");
        assert_eq!(migration["result"]["op"], "migrate-workspace");
        assert_eq!(migration["result"]["direction"], "right");
        assert_eq!(migration["result"]["sourceOut"], "out-b");
        assert_eq!(migration["result"]["targetOut"], "out-a");

        // Maximize.
        let maximize = settle_and_take(Some(CommandResult::Maximize {
            window: WindowId("win-a".to_owned()),
            maximized: true,
            clear_fullscreen: true,
            was_maximized: false,
        }));
        assert_eq!(maximize["outcome"], "ready");
        assert_eq!(maximize["result"]["op"], "maximize");
        assert_eq!(maximize["result"]["win"], "win-a");
        assert_eq!(maximize["result"]["maximized"], true);
        assert_eq!(maximize["result"]["clearFullscreen"], true);
        assert_eq!(maximize["result"]["wasMaximized"], false);

        // All 16 DirectionalOutcome tokens.
        let directional_cases = [
            (
                DirectionalOutcome::FocusRefusedFloating,
                Some("win-a"),
                None,
            ),
            (DirectionalOutcome::MoveRefusedFloating, Some("win-a"), None),
            (
                DirectionalOutcome::FocusRefusedWorkspaceFloating,
                Some("win-a"),
                None,
            ),
            (
                DirectionalOutcome::MoveRefusedWorkspaceFloating,
                Some("win-a"),
                None,
            ),
            (
                DirectionalOutcome::MoveRefusedFullscreen,
                Some("win-a"),
                None,
            ),
            (
                DirectionalOutcome::FocusRefusedMaximize,
                Some("win-a"),
                None,
            ),
            (DirectionalOutcome::FocusFloatRetained, Some("win-a"), None),
            (
                DirectionalOutcome::FocusFloatRefusedStale,
                Some("win-a"),
                None,
            ),
            (DirectionalOutcome::FocusUnfocusedRetained, None, None),
            (
                DirectionalOutcome::MoveFloatRefusedStale,
                Some("win-a"),
                None,
            ),
            (
                DirectionalOutcome::MoveFloatRefusedInvalidGeometry,
                Some("win-a"),
                None,
            ),
            (
                DirectionalOutcome::MoveFloatRefusedConstraints,
                Some("win-a"),
                None,
            ),
            (
                DirectionalOutcome::MoveFloatRefusedUnknownOutput,
                Some("win-a"),
                None,
            ),
            (
                DirectionalOutcome::FocusFloatApplied,
                Some("win-a"),
                Some(Rect {
                    x: 8,
                    y: 8,
                    w: 800,
                    h: 600,
                }),
            ),
            (
                DirectionalOutcome::MoveFloatApplied,
                Some("win-a"),
                Some(Rect {
                    x: 8,
                    y: 8,
                    w: 800,
                    h: 600,
                }),
            ),
            (DirectionalOutcome::NeedClear, Some("win-a"), None),
        ];
        for (outcome, target, rect) in directional_cases {
            let taken = settle_and_take(Some(CommandResult::Directional {
                outcome,
                target: target.map(|w| WindowId(w.to_owned())),
                rect,
                cause: None,
            }));
            assert_eq!(taken["outcome"], "ready");
            assert_eq!(taken["result"]["op"], "directional");
            assert_eq!(taken["result"]["outcome"], outcome.as_str());
            assert!(taken["result"]["cause"].is_null());
        }

        // All 5 directional cause tokens.
        for cause in [
            "no-target",
            "unknown-window",
            "focus-present",
            "no-source",
            "no-adjacent-output",
        ] {
            let taken = settle_and_take(Some(CommandResult::Directional {
                outcome: DirectionalOutcome::FocusUnfocusedRetained,
                target: None,
                rect: None,
                cause: Some(cause.to_owned()),
            }));
            assert_eq!(taken["outcome"], "ready");
            assert_eq!(taken["result"]["op"], "directional");
            assert_eq!(taken["result"]["cause"], cause);
        }

        // All 17 CommandRejectKind tokens via failed Take lane.
        for kind in [
            CommandRejectKind::FocusUnknown,
            CommandRejectKind::SendTargetUnknown,
            CommandRejectKind::SendWindowUnknown,
            CommandRejectKind::SendRefused,
            CommandRejectKind::ViewOutputUnknown,
            CommandRejectKind::ViewTargetUnknown,
            CommandRejectKind::TransferInFlight,
            CommandRejectKind::DirectionInvalid,
            CommandRejectKind::ScopeHeld,
            CommandRejectKind::SelectionUnknown,
            CommandRejectKind::MigrationRefused,
            CommandRejectKind::OperationRefused,
            CommandRejectKind::TilingTargetUnknown,
            CommandRejectKind::StateUnsupported,
            CommandRejectKind::StateUnknown,
            CommandRejectKind::FullscreenNotExitable,
            CommandRejectKind::StaleRenderTarget,
        ] {
            let taken = settle_and_take(Some(CommandResult::Rejected { kind }));
            assert_eq!(taken["outcome"], "failed");
            assert_eq!(taken["kind"], kind.as_str());
            assert!(taken.get("result").is_none());
        }
    }

    #[test]
    fn typed_clock_gate_checks_all_stamped_lanes_skips_null_and_visibility_and_preserves_identity()
    {
        let decode_with_attempts = |property_attempts: &str, creation_attempts: &str| {
            let win_a = test_window_json("win-a");
            let json = format!(
                "{{\"v\":4,\"adapter\":\"ad-1\",\"gen\":\"g-1\",\"obs\":\"1\",\"reqSeq\":\"1\",\
                \"req\":\"r-1\",\"cmdSeq\":null,\"command\":null,\"monotonicMs\":\"9\",\
                \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
                \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
                \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
                \"windows\":[{win_a}],\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
                \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
                \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
                \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
                \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
                \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[\"win-a\"],\
                \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,200,200]}}],\
                \"witness\":null,\"foregroundOut\":null}},\
                \"intentRecords\":[],\"propertyAttempts\":{property_attempts},\
                \"creationAttempts\":{creation_attempts},\"focusFeedback\":[],\
                \"focusChanges\":null,\"rendererInput\":null,\"gestureIngress\":null}}"
            );
            let map = serde_json::from_str::<serde_json::Value>(&json)
                .expect("json")
                .as_object()
                .expect("obj")
                .clone();
            decode_report_v4_map(&map).expect("decoded report")
        };

        // 1. Boundary control: attemptMs == clock passes on all 5 stamped lanes
        // (frame Some, member, view, state, creation), plus frame None + huge
        // attemptId and unstamped visibility.
        let prop_ok = "[{\"kind\":\"frame\",\"win\":\"win-a\",\"target\":[0,0,200,200],\
            \"disposition\":{\"kind\":\"called\",\"setter\":\"accepted\"},\"readback\":null,\
            \"dpiBefore\":null,\"dpiAfter\":null,\"attemptId\":\"999\",\"attemptMs\":\"9\",\"purpose\":{\"kind\":\"layout\"}},\
            {\"kind\":\"frame\",\"win\":\"win-b\",\"target\":[0,0,200,200],\
            \"disposition\":{\"kind\":\"called\",\"setter\":\"accepted\"},\"readback\":null,\
            \"dpiBefore\":null,\"dpiAfter\":null,\"attemptId\":\"999\",\"attemptMs\":null,\"purpose\":{\"kind\":\"layout\"}},\
            {\"kind\":\"member\",\"op\":\"op-m\",\"win\":\"win-a\",\"out\":\"DP-1\",\"ws\":\"ws-1\",\
            \"disposition\":{\"kind\":\"called\",\"setter\":\"accepted\"},\"pre\":null,\"post\":null,\"attemptMs\":\"9\"},\
            {\"kind\":\"view\",\"op\":\"op-v\",\"out\":\"DP-1\",\"ws\":\"ws-1\",\
            \"disposition\":{\"kind\":\"called\",\"setter\":\"accepted\"},\"pre\":null,\"post\":null,\"base\":null,\"attemptMs\":\"9\"},\
            {\"kind\":\"state\",\"win\":\"win-a\",\"property\":\"fullscreen\",\"target\":true,\"rev\":\"1\",\
            \"disposition\":{\"kind\":\"called\",\"setter\":\"accepted\"},\"pre\":false,\"post\":true,\"attemptMs\":\"9\"},\
            {\"kind\":\"visibility\",\"win\":\"win-a\",\"target\":true,\"revision\":\"1\",\"path\":\"staged\",\
            \"disposition\":{\"kind\":\"called\",\"setter\":\"accepted\"},\"reason\":\"\",\"pre\":null,\"post\":null}]";
        let create_ok = "[{\"op\":\"op-c\",\"ws\":\"ws-2\",\"out\":\"DP-1\",\
            \"setter\":\"accepted\",\"before\":[],\"after\":[],\
            \"created\":null,\"attemptMs\":\"9\"}]";
        let mut at_clock = decode_with_attempts(prop_ok, create_ok);
        gate_decoded_attempts_by_clock(&mut at_clock.report, 9);
        assert_eq!(at_clock.report.frames.len(), 2);
        assert_eq!(at_clock.report.frames[0].attempt_id, 999);
        assert_eq!(at_clock.report.frames[0].attempt_ms, Some(9));
        assert_eq!(at_clock.report.frames[1].attempt_id, 999);
        assert_eq!(at_clock.report.frames[1].attempt_ms, None);
        assert_eq!(at_clock.report.member_attempts.len(), 1);
        assert_eq!(at_clock.report.view_attempts.len(), 1);
        assert_eq!(at_clock.report.state_attempts.len(), 1);
        assert_eq!(at_clock.report.creation_attempts.len(), 1);
        assert_eq!(at_clock.report.visibility_attempts.len(), 1);
        assert!(at_clock.report.held_attempts.is_empty());

        // 2. Clock == 8: all 5 stamped rows (attemptMs == 9) are ignored in
        // place without synthesizing held_attempts or mutating residuals;
        // null frame attemptMs (win-b, with attemptId == 999) and unstamped
        // visibility survive.
        let mut above_clock = decode_with_attempts(prop_ok, create_ok);
        let residual_before = above_clock.residual.clone();
        gate_decoded_attempts_by_clock(&mut above_clock.report, 8);
        assert_eq!(above_clock.report.frames.len(), 1);
        assert_eq!(above_clock.report.frames[0].window.0, "win-b");
        assert_eq!(above_clock.report.frames[0].attempt_ms, None);
        assert_eq!(above_clock.report.frames[0].attempt_id, 999);
        assert!(above_clock.report.member_attempts.is_empty());
        assert!(above_clock.report.view_attempts.is_empty());
        assert!(above_clock.report.state_attempts.is_empty());
        assert!(above_clock.report.creation_attempts.is_empty());
        assert_eq!(above_clock.report.visibility_attempts.len(), 1);
        assert!(above_clock.report.held_attempts.is_empty());
        assert_eq!(above_clock.residual, residual_before);

        // 3. Already codec-held rows and their original residual stay
        // untouched while valid siblings survive and future-stamped rows are
        // ignored.
        let prop_with_held = "[{\"kind\":\"frame\",\"win\":\"win-a\",\"target\":[0,0,200,200],\
            \"disposition\":{\"kind\":\"called\",\"setter\":\"accepted\"},\"readback\":null,\
            \"dpiBefore\":null,\"dpiAfter\":null,\"attemptId\":\"999\",\"attemptMs\":\"9\",\"purpose\":{\"kind\":\"layout\"}},\
            {\"kind\":\"frame\",\"win\":\"win-c\",\"target\":[0,0,200,200],\
            \"disposition\":{\"kind\":\"called\",\"setter\":\"accepted\"},\"readback\":null,\
            \"dpiBefore\":null,\"dpiAfter\":null,\"attemptId\":\"2\",\"attemptMs\":\"9\",\"purpose\":{\"kind\":\"layout\"},\"extra\":1},\
            {\"kind\":\"frame\",\"win\":\"win-b\",\"target\":[0,0,200,200],\
            \"disposition\":{\"kind\":\"called\",\"setter\":\"accepted\"},\"readback\":null,\
            \"dpiBefore\":null,\"dpiAfter\":null,\"attemptId\":\"999\",\"attemptMs\":\"8\",\"purpose\":{\"kind\":\"layout\"}}]";
        let mut with_held = decode_with_attempts(prop_with_held, "[]");
        let held_before = with_held.report.held_attempts.clone();
        let held_residual_before = with_held.residual.clone();
        assert_eq!(held_before.len(), 1);
        gate_decoded_attempts_by_clock(&mut with_held.report, 8);
        assert_eq!(with_held.report.frames.len(), 1);
        assert_eq!(with_held.report.frames[0].window.0, "win-b");
        assert_eq!(with_held.report.frames[0].attempt_id, 999);
        assert_eq!(with_held.report.frames[0].attempt_ms, Some(8));
        assert_eq!(with_held.report.held_attempts, held_before);
        assert_eq!(with_held.residual, held_residual_before);
    }

    fn bound_test_broker() -> (WorldBroker, &'static str, String) {
        let mut broker = WorldBroker::new();
        let adapter = "kwin-stamp-gate-1";
        let hs = format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"hs-1\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        );
        let _ = broker.discover_connection(&hs).expect("handshake");
        let generation = broker.generation().to_owned();
        let _ = broker
            .discover_connection(&settings_message(adapter, &generation, "set-1"))
            .expect("settings");
        (broker, adapter, generation)
    }

    fn test_window_json(win: &str) -> String {
        format!(
            "{{\"win\":\"{win}\",\"out\":\"DP-1\",\"rect\":[0,0,200,200],\
            \"fullscreen\":false,\"maximized\":false,\"sticky\":false,\"minimized\":false,\
            \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}},\
            \"hostHidden\":false,\"privileged\":false,\"managed\":true,\"acceptsInput\":true,\
            \"activationInhibited\":false,\"toolWindow\":false,\"owned\":false,\"transient\":false,\
            \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\"owner\":null,\
            \"parent\":null,\"role\":null,\"appId\":null,\"executable\":null,\
            \"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\"title\":null,\
            \"type\":\"normal\",\"activationPolicy\":\"regular\",\"layer\":0,\
            \"fullscreenOwner\":null,\"fullscreenExitable\":null,\"suppressionAvailable\":null,\
            \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}}}}"
        )
    }

    fn bound_capture_env(
        adapter: &str,
        generation: &str,
        source_seq: u64,
        property_attempts_json: &str,
    ) -> String {
        let win_a = test_window_json("win-a");
        let win_b = test_window_json("win-b");
        let inner = format!(
            "{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":[{win_a},{win_b}],\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[\"win-a\",\"win-b\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,200,200]}},{{\"win\":\"win-b\",\"live\":[0,0,200,200]}}],\
            \"witness\":null,\"foregroundOut\":null}},\
            \"intentRecords\":[],\"propertyAttempts\":{property_attempts_json},\"focusFeedback\":[],\
            \"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\"nativeGesture\":null,\"cursorWindow\":null,\"nativeInvalidWitness\":null}}"
        );
        format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-{source_seq}-t\",\"sourceSeq\":\"{source_seq}\",\"nativeInvalid\":\"{source_seq}\",\
            \"report\":{inner}}}"
        )
    }

    #[test]
    fn bound_capture_future_state_stamp_cannot_settle_while_valid_sibling_applies() {
        let (mut broker, adapter, generation) = bound_test_broker();
        // Seed win-a and win-b.
        let seed_env = bound_capture_env(adapter, &generation, 1, "[]");
        let _ = broker.dispatch_capture(&seed_env).expect("seed");
        // Queue toggle-fullscreen for win-a so core publishes a state desire.
        let q = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"cli-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"toggle-fullscreen\",\"win\":\"win-a\"}}}}"
        );
        let q_reply = broker
            .queue_command("cli-1", &q)
            .expect("queue toggle-fullscreen");
        assert!(
            q_reply.contains("\"queued\""),
            "expected queued reply, got {q_reply}"
        );
        let issue_env = bound_capture_env(adapter, &generation, 2, "[]");
        let issued: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&issue_env).expect("issue"))
                .expect("JSON");
        let rev = issued["rev"].as_str().expect("rev string").to_owned();
        let has_win_a_fullscreen = |reply: &serde_json::Value| -> bool {
            reply["windows"].as_array().is_some_and(|wins| {
                wins.iter().any(|w| {
                    w["win"] == "win-a"
                        && w["state"].as_array().is_some_and(|states| {
                            states
                                .iter()
                                .any(|s| s["property"] == "fullscreen" && s["value"] == true)
                        })
                })
            })
        };
        assert!(
            has_win_a_fullscreen(&issued),
            "expected pending fullscreen state desire for win-a, got {issued}"
        );

        // Send a future-stamped refused state attempt for win-a (which would
        // retire the desire if accepted) alongside a valid sibling frame
        // attempt for win-b.
        let future_ms = "9999999999999";
        let attempts = format!(
            "[{{\"kind\":\"state\",\"win\":\"win-a\",\"property\":\"fullscreen\",\"target\":true,\
              \"rev\":\"{rev}\",\"disposition\":{{\"kind\":\"called\",\"setter\":\"refused\"}},\"pre\":false,\"post\":false,\"attemptMs\":\"{future_ms}\"}},\
              {{\"kind\":\"frame\",\"win\":\"win-b\",\"target\":[0,0,200,200],\
              \"disposition\":{{\"kind\":\"called\",\"setter\":\"accepted\"}},\"readback\":[0,0,200,200],\"dpiBefore\":null,\"dpiAfter\":null,\
              \"attemptId\":\"1\",\"attemptMs\":\"0\",\"purpose\":{{\"kind\":\"layout\"}}}}]"
        );
        let env = bound_capture_env(adapter, &generation, 3, &attempts);
        let reply: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&env).expect("dispatch")).expect("JSON");
        assert!(
            reply.get("outcome").is_none() || reply["outcome"] != "rejected",
            "expected accepted capture, got {reply}"
        );
        // Because the future state attempt cannot settle, the fullscreen desire
        // for win-a must remain controlled (not held/refused).
        assert!(
            reply["windows"].as_array().is_some_and(|wins| {
                wins.iter().any(|w| {
                    w["win"] == "win-a"
                        && w["state"].as_array().is_some_and(|states| {
                            states.iter().any(|s| {
                                s["property"] == "fullscreen"
                                    && s["value"] == true
                                    && s["auth"] == "controlled"
                            })
                        })
                })
            }),
            "future state stamp must not settle/refuse win-a fullscreen desire, got {reply}"
        );
    }

    #[test]
    fn bound_capture_identifiable_future_frame_and_held_row_do_not_sink_valid_sibling() {
        let (mut broker, adapter, generation) = bound_test_broker();
        let seed_env = bound_capture_env(adapter, &generation, 1, "[]");
        let _ = broker.dispatch_capture(&seed_env).expect("seed");
        let q = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"cli-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"toggle-fullscreen\",\"win\":\"win-a\"}}}}"
        );
        let q_reply = broker
            .queue_command("cli-1", &q)
            .expect("queue toggle-fullscreen");
        assert!(
            q_reply.contains("\"queued\""),
            "expected queued reply, got {q_reply}"
        );
        let issue_env = bound_capture_env(adapter, &generation, 2, "[]");
        let issued: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&issue_env).expect("issue"))
                .expect("JSON");
        let rev = issued["rev"].as_str().expect("rev string").to_owned();
        assert!(
            issued["windows"].as_array().is_some_and(|wins| {
                wins.iter().any(|w| {
                    w["win"] == "win-a"
                        && w["state"].as_array().is_some_and(|states| {
                            states.iter().any(|s| {
                                s["property"] == "fullscreen"
                                    && s["value"] == true
                                    && s["auth"] == "controlled"
                            })
                        })
                })
            }),
            "expected pending controlled fullscreen desire for win-a, got {issued}"
        );

        // Combine an identifiable future-stamped frame row (win-a), an
        // identifiable codec-held frame row (win-c), a valid null-stamped
        // frame row (win-b), and a valid in-epoch refused state sibling
        // (win-a). The valid state sibling must settle and move win-a's
        // fullscreen desire to auth:hold.
        let future_ms = "9999999999999";
        let attempts = format!(
            "[{{\"kind\":\"frame\",\"win\":\"win-a\",\"target\":[0,0,200,200],\
              \"disposition\":{{\"kind\":\"called\",\"setter\":\"accepted\"}},\"readback\":[0,0,200,200],\"dpiBefore\":null,\"dpiAfter\":null,\
              \"attemptId\":\"1\",\"attemptMs\":\"{future_ms}\",\"purpose\":{{\"kind\":\"layout\"}}}},\
              {{\"kind\":\"frame\",\"win\":\"win-c\",\"target\":[0,0,200,200],\
              \"disposition\":{{\"kind\":\"called\",\"setter\":\"accepted\"}},\"readback\":null,\"dpiBefore\":null,\"dpiAfter\":null,\
              \"attemptId\":\"2\",\"attemptMs\":\"{future_ms}\",\"purpose\":{{\"kind\":\"layout\"}},\"extra\":1}},\
              {{\"kind\":\"frame\",\"win\":\"win-b\",\"target\":[0,0,200,200],\
              \"disposition\":{{\"kind\":\"called\",\"setter\":\"accepted\"}},\"readback\":[0,0,200,200],\"dpiBefore\":null,\"dpiAfter\":null,\
              \"attemptId\":\"{future_ms}\",\"attemptMs\":null,\"purpose\":{{\"kind\":\"layout\"}}}},\
              {{\"kind\":\"state\",\"win\":\"win-a\",\"property\":\"fullscreen\",\"target\":true,\
              \"rev\":\"{rev}\",\"disposition\":{{\"kind\":\"called\",\"setter\":\"refused\"}},\"pre\":false,\"post\":false,\"attemptMs\":\"0\"}}]"
        );
        let env = bound_capture_env(adapter, &generation, 3, &attempts);
        let reply: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&env).expect("dispatch")).expect("JSON");
        assert!(
            reply.get("outcome").is_none() || reply["outcome"] != "rejected",
            "expected accepted capture (not whole-capture refusal), got {reply}"
        );
        assert!(
            reply.get("rev").and_then(|v| v.as_str()).is_some(),
            "expected normal bound reply with rev, got {reply}"
        );
        assert!(
            reply["windows"].as_array().is_some_and(|wins| {
                wins.iter().any(|w| {
                    w["win"] == "win-a"
                        && w["state"].as_array().is_some_and(|states| {
                            states.iter().any(|s| {
                                s["property"] == "fullscreen"
                                    && s["value"] == true
                                    && s["auth"] == "hold"
                            })
                        })
                })
            }),
            "valid in-epoch state sibling must settle win-a fullscreen desire to auth:hold, got {reply}"
        );
    }

    #[test]
    fn bound_capture_unidentified_hard_invalid_row_keeps_whole_capture_refusal_precedence() {
        let (mut broker, adapter, generation) = bound_test_broker();
        let future_ms = "9999999999999";
        let attempts = format!(
            "[{{\"kind\":\"frame\",\"win\":\"win-a\",\"target\":[0,0,200,200],\
              \"disposition\":{{\"kind\":\"called\",\"setter\":\"accepted\"}},\"readback\":[0,0,200,200],\"dpiBefore\":null,\"dpiAfter\":null,\
              \"attemptId\":\"1\",\"attemptMs\":\"{future_ms}\",\"purpose\":{{\"kind\":\"layout\"}}}},\
              {{\"kind\":\"frame\",\"target\":[0,0,200,200],\
              \"disposition\":{{\"kind\":\"called\",\"setter\":\"accepted\"}},\"readback\":null,\"dpiBefore\":null,\"dpiAfter\":null,\
              \"attemptId\":\"2\",\"attemptMs\":\"0\",\"purpose\":{{\"kind\":\"layout\"}}}}]"
        );
        let env = bound_capture_env(adapter, &generation, 1, &attempts);
        let reply: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&env).expect("dispatch")).expect("JSON");
        assert_eq!(reply["outcome"], "rejected");
        assert_ne!(
            reply["message"], "attempt-clock-invalid",
            "codec shape/identity refusal must take precedence over clock gate"
        );
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
        let mut broker = WorldBroker::new();
        let generation = broker.generation().to_owned();
        let select = serde_json::json!({"op":"select-numbered","out":null,"index":3});
        assert!(decode_command_value_v4(&select).is_ok());
        let bogus = serde_json::json!({"op":"no-such-op"});
        assert!(decode_command_value_v4(&bogus).is_err());
        // Explicit string outputs stay valid (genuinely semantic outputs);
        // only unknown ops and shapes refuse.
        let explicit = serde_json::json!({"op":"select-numbered","out":"DP-1","index":3});
        assert!(decode_command_value_v4(&explicit).is_ok());
        let bad_index = serde_json::json!({"op":"select-numbered","out":null,"index":10});
        assert!(decode_command_value_v4(&bad_index).is_err());
        // Direction-only commands carry no float key on this wire (the
        // stale TS vocabulary did): exact shape or refusal.
        let focus = serde_json::json!({"op":"focus-directional","win":"win-001",
            "direction":"left"});
        assert!(decode_command_value_v4(&focus).is_ok());
        let stale_float = serde_json::json!({"op":"focus-directional","win":"win-001",
            "direction":"left","floatSubject":false});
        assert!(decode_command_value_v4(&stale_float).is_err());

        // Queue ingress preserves the exact command-invalid mapping.
        let bad_req = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"1\",\
              \"command\":{stale_float}}}"
        );
        let bad_reply: serde_json::Value =
            serde_json::from_str(&broker.queue_command(":1.42", &bad_req).expect("in-band"))
                .expect("JSON");
        assert_eq!(bad_reply["outcome"], "rejected");
        assert_eq!(bad_reply["kind"], "snapshot-invalid");
        assert_eq!(bad_reply["message"], "command-invalid");
        assert!(broker.pending.is_empty());

        let good_req = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"1\",\
              \"command\":{focus}}}"
        );
        let good_reply: serde_json::Value =
            serde_json::from_str(&broker.queue_command(":1.42", &good_req).expect("queued"))
                .expect("JSON");
        assert_eq!(good_reply["outcome"], "queued");
        assert_eq!(broker.pending.len(), 1);
    }

    #[test]
    fn monotonic_clock_never_goes_backwards() {
        let broker = WorldBroker::new();
        let first = broker.monotonic_ms();
        assert!(broker.monotonic_ms() >= first);
    }

    #[test]
    fn command_consumed_close_reconciles_pinned_settled_assoc_and_reparks_batch() {
        let mut broker = WorldBroker::new();
        let adapter = "kwin-actuator-1";
        let hs = format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"hs-1\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        );
        let _ = broker.discover_connection(&hs).expect("handshake");
        let generation = broker.generation().to_owned();
        let _ = broker
            .discover_connection(&settings_message(adapter, &generation, "set-1"))
            .expect("settings");

        let win_a = "{\"win\":\"win-a\",\"out\":\"DP-1\",\"rect\":[0,0,200,200],\
            \"fullscreen\":false,\"maximized\":false,\"sticky\":false,\"minimized\":false,\
            \"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null},\
            \"hostHidden\":false,\"privileged\":false,\"managed\":true,\"acceptsInput\":true,\
            \"activationInhibited\":false,\"toolWindow\":false,\"owned\":false,\"transient\":false,\
            \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\"owner\":null,\
            \"parent\":null,\"role\":null,\"appId\":null,\"executable\":null,\
            \"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\"title\":null,\
            \"type\":\"normal\",\"activationPolicy\":\"regular\",\"layer\":0,\
            \"fullscreenOwner\":null,\"fullscreenExitable\":null,\"suppressionAvailable\":null,\
            \"visibility\":{\"visible\":true,\"suppressionOwned\":null}}";
        // Seed one observation-only capture so win-a is admitted and ready
        // before opening the gesture.
        let seed_inner = format!(
            "{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":[{win_a}],\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[\"win-a\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,200,200]}}],\
            \"witness\":null,\"foregroundOut\":null}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\"nativeGesture\":null,\"cursorWindow\":null,\"nativeInvalidWitness\":null}}"
        );
        let seed_env = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-0-t\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\
            \"report\":{seed_inner}}}"
        );
        let _ = broker.dispatch_capture(&seed_env).expect("seed");

        let open_inner = format!(
            "{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":[{win_a}],\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[\"win-a\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,200,200]}}],\
            \"witness\":null,\"foregroundOut\":null}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\
            \"nativeGesture\":{{\"v\":1,\"instance\":\"gesture-1\",\"serial\":\"1\",\"revision\":\"1\",\
            \"phase\":\"started\",\"win\":\"win-a\",\"kind\":\"move\",\"startRect\":[0,0,200,200],\
            \"startCursor\":[10,10],\"cursor\":[150,50],\"modalMove\":1,\"changed\":1,\
            \"superseded\":0,\"available\":1}},\"cursorWindow\":null,\"nativeInvalidWitness\":null}}"
        );
        let open_env = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-1-t\",\"sourceSeq\":\"2\",\"nativeInvalid\":\"2\",\
            \"report\":{open_inner}}}"
        );
        let opened: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&open_env).expect("open")).expect("JSON");
        let assoc1 = serde_json::json!({"instance": "gesture-1", "serial": "1", "coreId": "1"});
        assert_eq!(opened["gestureAssoc"], assoc1);
        assert_eq!(broker.engine.active_gesture_id(), Some(1));

        // Advance engine command_highwater to 1 via a normal command report.
        let mut cmd_inner: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str::<serde_json::Value>(&open_inner)
                .expect("obj")
                .as_object()
                .expect("map")
                .clone();
        cmd_inner.remove("nativeGesture");
        cmd_inner.remove("cursorWindow");
        cmd_inner.remove("nativeInvalidWitness");
        let cmd_obs = broker.take_observation_seq().expect("obs").to_string();
        let cmd_req = broker.take_request_seq().expect("req").to_string();
        cmd_inner.insert("adapter".to_owned(), json_string(adapter));
        cmd_inner.insert("gen".to_owned(), json_string(&generation));
        cmd_inner.insert("obs".to_owned(), json_string(&cmd_obs));
        cmd_inner.insert("reqSeq".to_owned(), json_string(&cmd_req));
        cmd_inner.insert("req".to_owned(), json_string("src-2-t"));
        cmd_inner.insert("cmdSeq".to_owned(), json_string("1"));
        cmd_inner.insert(
            "command".to_owned(),
            serde_json::json!({"op":"select-numbered","out":null,"index":1}),
        );
        cmd_inner.insert("monotonicMs".to_owned(), json_string("3"));
        let cmd_report = decode_request(&serde_json::Value::Object(cmd_inner.clone()).to_string())
            .expect("valid command report");
        let cmd_outcome = broker
            .engine
            .handle_report_with_gate_and_prepare(
                &cmd_report,
                &[],
                &tiler_protocol::world_admission::ProtocolAdmissionGate::default(),
                |_| Ok::<(), BrokerError>(()),
            )
            .expect("cmd report")
            .outcome;
        assert!(matches!(cmd_outcome, WorldOutcome::Accepted(_)));

        // Queue a pending command that must be reparked on CommandConsumed.
        let parked = QueuedCommand {
            global_seq: 99,
            sender: ":1.1".to_owned(),
            client: "client-1".to_owned(),
            client_cmd: "cmd-parked".to_owned(),
            command: decode_command_value_v4(
                &serde_json::json!({"op":"select-numbered","out":null,"index":1}),
            )
            .expect("valid command"),
            suppress_orphan_result: false,
        };
        let mut drained = VecDeque::new();
        drained.push_back(parked);

        // Fresh report with already-consumed cmdSeq=1 closes win-a with omitted
        // gesture ingress -> WorldOutcome::CommandConsumed settles gesture 1.
        let consumed_obs = broker.take_observation_seq().expect("obs").to_string();
        let consumed_req = broker.take_request_seq().expect("req").to_string();
        cmd_inner.insert("obs".to_owned(), json_string(&consumed_obs));
        cmd_inner.insert("reqSeq".to_owned(), json_string(&consumed_req));
        cmd_inner.insert("req".to_owned(), json_string("src-3-t"));
        cmd_inner.insert("windows".to_owned(), serde_json::json!([]));
        cmd_inner.insert("closed".to_owned(), serde_json::json!(["win-a"]));
        let consumed_report = decode_request(&serde_json::Value::Object(cmd_inner).to_string())
            .expect("valid consumed close report");
        let transition = broker
            .engine
            .handle_report_with_gate_and_prepare(
                &consumed_report,
                &[],
                &tiler_protocol::world_admission::ProtocolAdmissionGate::default(),
                |_| Ok::<(), BrokerError>(()),
            )
            .expect("consumed report");
        let consumed_outcome = transition.outcome;
        let consumed_dispositions = transition.dispositions;
        assert!(matches!(consumed_outcome, WorldOutcome::CommandConsumed(_)));

        let parsed = ParsedCapture {
            adapter: adapter.to_owned(),
            generation: generation.clone(),
            source: "src-3-t".to_owned(),
            source_seq: 4,
            native_invalid: 4,
            inner: serde_json::Map::new(),
        };
        let staged_map = broker.gesture_map.clone();
        let reply_str = broker
            .finish_capture_outcome(
                &parsed,
                "{\"synthetic\":true}",
                4,
                None,
                staged_map,
                None,
                drained,
                consumed_outcome,
                consumed_dispositions,
            )
            .expect("consumed outcome finishes");
        let reply: serde_json::Value = serde_json::from_str(&reply_str).expect("JSON");
        assert!(reply["gestureAssoc"].is_null());
        assert_eq!(reply["gestureSettled"]["id"], serde_json::json!("1"));
        assert_eq!(
            reply["gestureSettled"]["committed"],
            serde_json::json!(false)
        );
        assert_eq!(reply["gestureSettledAssoc"], assoc1);
        assert_eq!(
            broker.pending.len(),
            1,
            "drained batch reparks on CommandConsumed"
        );
        assert_eq!(broker.pending[0].global_seq, 99);

        // Subsequent idle capture retains the pinned gestureSettledAssoc.
        broker.pending.clear();
        let idle_env = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-4-t\",\"sourceSeq\":\"5\",\"nativeInvalid\":\"5\",\
            \"report\":{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":[],\"closed\":[],\"native\":[],\"focused\":\"\",\"foregroundKnown\":false,\
            \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[],\"pagerFrames\":[],\
            \"witness\":null,\"foregroundOut\":null}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\"nativeGesture\":null,\"cursorWindow\":null,\
            \"nativeInvalidWitness\":null}}}}"
        );
        let idle_reply: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&idle_env).expect("idle")).expect("JSON");
        assert!(idle_reply["gestureAssoc"].is_null());
        assert_eq!(idle_reply["gestureSettled"]["id"], serde_json::json!("1"));
        assert_eq!(idle_reply["gestureSettledAssoc"], assoc1);
    }

    #[test]
    fn drag_preview_projection_trace_emits_correlated_decisions() {
        let mut broker = WorldBroker::new();
        let adapter = "adapter-trace";
        let hs = format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"hs-trace\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        );
        let _ = broker.discover_connection(&hs).expect("handshake");
        let generation = broker.generation().to_owned();
        let _ = broker
            .discover_connection(&settings_message(adapter, &generation, "set-trace"))
            .expect("settings");

        let mk_win = |id: &str, x: i32| {
            format!(
                "{{\"win\":\"{id}\",\"out\":\"DP-1\",\"rect\":[{x},0,400,600],\
                \"fullscreen\":false,\"maximized\":false,\"sticky\":false,\"minimized\":false,\
                \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}},\
                \"hostHidden\":false,\"privileged\":false,\"managed\":true,\"acceptsInput\":true,\
                \"activationInhibited\":false,\"toolWindow\":false,\"owned\":false,\"transient\":false,\
                \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\"owner\":null,\
                \"parent\":null,\"role\":null,\"appId\":null,\"executable\":null,\
                \"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\"title\":null,\
                \"type\":\"normal\",\"activationPolicy\":\"regular\",\"layer\":0,\
                \"fullscreenOwner\":null,\"fullscreenExitable\":null,\"suppressionAvailable\":null,\
                \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}}}}"
            )
        };
        let wins = format!("[{},{}]", mk_win("win-a", 0), mk_win("win-b", 400));

        // 1. Seed capture with no dragPreview -> decision=absent.
        let seed_env = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-1\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\
            \"report\":{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":{wins},\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"win-a\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[\"win-a\",\"win-b\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,400,600]}},{{\"win\":\"win-b\",\"live\":[400,0,400,600]}}],\
            \"witness\":null,\"foregroundOut\":\"DP-1\"}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\"nativeGesture\":null,\"cursorWindow\":null,\"nativeInvalidWitness\":null}}}}"
        );
        let absent = with_broker_trace_capture(|| {
            let _ = broker.dispatch_capture(&seed_env).expect("seed");
        });
        assert_eq!(
            absent,
            vec![format!(
                "omnitiler:broker:trace ev=drag-preview-projection gen={generation} req=src-1 rev=1 obs=1 corePreview=none id=- domainOut=- suspendedOutput=- assoc=- decision=absent"
            )]
        );

        // 2. Open + update gesture over win-b with nativeGesture -> decision=projected-proven.
        let start_env = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-2\",\"sourceSeq\":\"2\",\"nativeInvalid\":\"2\",\
            \"report\":{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":{wins},\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":true,\"cursor\":[10,10],\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"win-a\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"win-a\",\"pagerOrder\":[\"win-a\",\"win-b\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,400,600]}},{{\"win\":\"win-b\",\"live\":[400,0,400,600]}}],\
            \"witness\":null,\"foregroundOut\":\"DP-1\"}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\
            \"nativeGesture\":{{\"v\":1,\"instance\":\"g-1\",\"serial\":\"1\",\"revision\":\"1\",\
            \"phase\":\"started\",\"win\":\"win-a\",\"kind\":\"move\",\"startRect\":[0,0,400,600],\
            \"startCursor\":[10,10],\"cursor\":[10,10],\"modalMove\":1,\"changed\":1,\
            \"superseded\":0,\"available\":1}},\"cursorWindow\":null,\"nativeInvalidWitness\":null}}}}"
        );
        let _ = broker.dispatch_capture(&start_env).expect("start");

        let update_env = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-3\",\"sourceSeq\":\"3\",\"nativeInvalid\":\"3\",\
            \"report\":{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":{wins},\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":true,\"cursor\":[600,300],\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"win-a\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"win-a\",\"pagerOrder\":[\"win-a\",\"win-b\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,400,600]}},{{\"win\":\"win-b\",\"live\":[400,0,400,600]}}],\
            \"witness\":null,\"foregroundOut\":\"DP-1\"}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\
            \"nativeGesture\":{{\"v\":1,\"instance\":\"g-1\",\"serial\":\"2\",\"revision\":\"2\",\
            \"phase\":\"updated\",\"win\":\"win-a\",\"kind\":\"move\",\"startRect\":[0,0,400,600],\
            \"startCursor\":[10,10],\"cursor\":[600,300],\"modalMove\":1,\"changed\":1,\
            \"superseded\":0,\"available\":1}},\"cursorWindow\":null,\"nativeInvalidWitness\":null}}}}"
        );
        let proven = with_broker_trace_capture(|| {
            let _ = broker.dispatch_capture(&update_env).expect("update");
        });
        assert_eq!(
            proven,
            vec![format!(
                "omnitiler:broker:trace ev=drag-preview-projection gen={generation} req=src-3 rev=3 obs=3 corePreview=present id=2 domainOut=DP-1 suspendedOutput=- assoc=proven decision=projected-proven"
            )]
        );

        // 3. Present dragPreview without active_assoc -> decision=projected-unproven.
        let mut unproven_inner: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str::<serde_json::Value>(&update_env)
                .expect("obj")
                .as_object()
                .expect("map")
                .get("report")
                .expect("report")
                .as_object()
                .expect("report map")
                .clone();
        unproven_inner.remove("nativeGesture");
        unproven_inner.remove("cursorWindow");
        unproven_inner.remove("nativeInvalidWitness");
        let obs4 = broker.take_observation_seq().expect("obs4").to_string();
        let req4 = broker.take_request_seq().expect("req4").to_string();
        unproven_inner.insert("adapter".to_owned(), json_string(adapter));
        unproven_inner.insert("gen".to_owned(), json_string(&generation));
        unproven_inner.insert("obs".to_owned(), json_string(&obs4));
        unproven_inner.insert("reqSeq".to_owned(), json_string(&req4));
        unproven_inner.insert("req".to_owned(), json_string("req-unproven"));
        unproven_inner.insert("cmdSeq".to_owned(), serde_json::Value::Null);
        unproven_inner.insert("command".to_owned(), serde_json::Value::Null);
        unproven_inner.insert("monotonicMs".to_owned(), json_string("4"));
        unproven_inner.insert("cursor".to_owned(), serde_json::json!([620, 300]));
        unproven_inner.insert("gestureIngress".to_owned(), serde_json::Value::Null);
        let report4 =
            decode_request(&serde_json::Value::Object(unproven_inner).to_string()).expect("r4");
        let outcome4 = broker
            .engine
            .handle_report_with_gate_and_prepare(
                &report4,
                &[],
                &tiler_protocol::world_admission::ProtocolAdmissionGate::default(),
                |_| Ok::<(), BrokerError>(()),
            )
            .expect("report4")
            .outcome;
        let WorldOutcome::Accepted(base_reply) = outcome4 else {
            panic!("expected accepted reply");
        };
        let parsed_unproven = ParsedCapture {
            adapter: adapter.to_owned(),
            generation: generation.clone(),
            source: "src-4".to_owned(),
            source_seq: 4,
            native_invalid: 4,
            inner: serde_json::Map::new(),
        };
        let empty_map = crate::kde_gesture::KdeGestureMap::default();
        let unproven = with_broker_trace_capture(|| {
            let _ = broker
                .finish_capture_outcome(
                    &parsed_unproven,
                    "{\"synthetic\":true}",
                    4,
                    None,
                    empty_map,
                    None,
                    VecDeque::new(),
                    WorldOutcome::Accepted(base_reply.clone()),
                    Vec::new(),
                )
                .expect("unproven");
        });
        assert_eq!(
            unproven,
            vec![format!(
                "omnitiler:broker:trace ev=drag-preview-projection gen={generation} req=req-unproven rev=4 obs=4 corePreview=present id=2 domainOut=DP-1 suspendedOutput=- assoc=none decision=projected-unproven"
            )]
        );

        // 4. Present dragPreview with matching suspendedOutput -> decision=suspended.
        let mut suspended_reply = base_reply;
        suspended_reply.request_id = "req-suspended".to_owned();
        suspended_reply.state_revision = 5;
        suspended_reply.based_on_observation_seq = 5;
        suspended_reply.presentation.suspended_output = suspended_reply
            .drag_preview
            .as_ref()
            .map(|p| p.domain.output.clone());
        let parsed_suspended = ParsedCapture {
            adapter: adapter.to_owned(),
            generation: generation.clone(),
            source: "src-5".to_owned(),
            source_seq: 5,
            native_invalid: 5,
            inner: serde_json::Map::new(),
        };
        let empty_map_2 = crate::kde_gesture::KdeGestureMap::default();
        let suspended = with_broker_trace_capture(|| {
            let _ = broker
                .finish_capture_outcome(
                    &parsed_suspended,
                    "{\"synthetic\":true}",
                    5,
                    None,
                    empty_map_2,
                    None,
                    VecDeque::new(),
                    WorldOutcome::Accepted(suspended_reply),
                    Vec::new(),
                )
                .expect("suspended");
        });
        assert_eq!(
            suspended,
            vec![format!(
                "omnitiler:broker:trace ev=drag-preview-projection gen={generation} req=req-suspended rev=5 obs=5 corePreview=present id=2 domainOut=DP-1 suspendedOutput=DP-1 assoc=- decision=suspended"
            )]
        );
    }

    #[test]
    fn wake_epoch_exhaustion_rotates_before_partial_publish_or_settle() {
        let mut broker = WorldBroker::new();
        let adapter = "kwin-actuator-1";
        let hs = format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"hs-1\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        );
        broker.discover_connection(&hs).expect("handshake");
        let generation = broker.generation().to_owned();
        let settings = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
              \"req\":\"set-1\",\"rev\":\"1\",\
              \"values\":{{\"mode\":\"per-output-local\",\"gap\":8,\"outer\":8,\"bornMaximized\":\"tile-on-launch\"}}}}"
        );
        broker.discover_connection(&settings).expect("settings");

        let win_a = "{\"win\":\"win-a\",\"out\":\"DP-1\",\"rect\":[0,0,200,200],\
            \"minimized\":false,\"maximized\":false,\"fullscreen\":false,\"sticky\":false,\
            \"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null},\
            \"hostHidden\":false,\"privileged\":false,\"managed\":true,\"acceptsInput\":true,\
            \"activationInhibited\":false,\"toolWindow\":false,\"owned\":false,\"transient\":false,\
            \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\"owner\":null,\
            \"parent\":null,\"role\":null,\"appId\":null,\"executable\":null,\
            \"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\"title\":null,\
            \"type\":\"normal\",\"activationPolicy\":\"regular\",\"layer\":0,\
            \"fullscreenOwner\":null,\"fullscreenExitable\":null,\"suppressionAvailable\":null,\
            \"visibility\":{\"visible\":true,\"suppressionOwned\":null}}";
        let inner = format!(
            "{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":[{win_a}],\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[\"win-a\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,200,200]}}],\
            \"witness\":null,\"foregroundOut\":null}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\"nativeGesture\":null,\"cursorWindow\":null,\"nativeInvalidWitness\":null}}"
        );
        let cap1 = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-1\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\"report\":{inner}}}"
        );
        let cap1_reply = broker.dispatch_capture(&cap1).expect("initial capture");
        assert!(
            cap1_reply.contains("\"rev\":\"1\""),
            "cap1 accepted: {cap1_reply}"
        );
        let rev_before = broker.engine.state_revision();

        for idx in 0..2 {
            let cmd = format!(
                "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"{}\",\
                \"command\":{{\"op\":\"focus\",\"win\":\"win-a\"}}}}",
                idx + 1
            );
            broker.queue_command(":1.42", &cmd).expect("queued");
        }
        broker.settled.clear();
        broker.settled_order.clear();
        // Leave room for 1 settlement, while the drained batch will issue 2.
        broker.wake_epoch = u64::MAX - 1;

        let cap2 = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-2\",\"sourceSeq\":\"2\",\"nativeInvalid\":\"2\",\"report\":{inner}}}"
        );
        let err = broker.dispatch_capture(&cap2).expect_err("must rotate");
        assert!(matches!(err, BrokerError::Rotate));
        assert!(
            broker.settled.is_empty(),
            "no partial settlement allowed on wake epoch exhaustion"
        );
        assert_eq!(broker.pending.len(), 2, "drained batch must repark intact");
        assert_eq!(
            broker.engine.state_revision(),
            rev_before,
            "core world must not adopt on wake epoch exhaustion"
        );
    }

    #[test]
    fn late_oversize_rolls_back_and_reparks_batch_in_order() {
        let mut broker = WorldBroker::new();
        let adapter = "kwin-actuator-1";
        let hs = format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"hs-1\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        );
        broker.discover_connection(&hs).expect("handshake");
        let generation = broker.generation().to_owned();
        let settings = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
              \"req\":\"set-1\",\"rev\":\"1\",\
              \"values\":{{\"mode\":\"per-output-local\",\"gap\":8,\"outer\":8,\"bornMaximized\":\"tile-on-launch\"}}}}"
        );
        broker.discover_connection(&settings).expect("settings");

        let win_a = "{\"win\":\"win-a\",\"out\":\"DP-1\",\"rect\":[0,0,200,200],\
            \"minimized\":false,\"maximized\":false,\"fullscreen\":false,\"sticky\":false,\
            \"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null},\
            \"hostHidden\":false,\"privileged\":false,\"managed\":true,\"acceptsInput\":true,\
            \"activationInhibited\":false,\"toolWindow\":false,\"owned\":false,\"transient\":false,\
            \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\"owner\":null,\
            \"parent\":null,\"role\":null,\"appId\":null,\"executable\":null,\
            \"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\"title\":null,\
            \"type\":\"normal\",\"activationPolicy\":\"regular\",\"layer\":0,\
            \"fullscreenOwner\":null,\"fullscreenExitable\":null,\"suppressionAvailable\":null,\
            \"visibility\":{\"visible\":true,\"suppressionOwned\":null}}";
        let inner = format!(
            "{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":[{win_a}],\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[\"win-a\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,200,200]}}],\
            \"witness\":null,\"foregroundOut\":null}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\"nativeGesture\":null,\"cursorWindow\":null,\"nativeInvalidWitness\":null}}"
        );
        let cap1 = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-1\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\"report\":{inner}}}"
        );
        let cap1_reply = broker.dispatch_capture(&cap1).expect("initial capture");
        assert!(
            cap1_reply.contains("\"rev\":\"1\""),
            "cap1 accepted: {cap1_reply}"
        );

        let cmd1 = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"1\",\
            \"command\":{{\"op\":\"focus\",\"win\":\"win-a\"}}}}"
        );
        let cmd2 = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"2\",\
            \"command\":{{\"op\":\"focus\",\"win\":\"win-a\"}}}}"
        );
        broker.queue_command(":1.42", &cmd1).expect("queued 1");
        broker.queue_command(":1.42", &cmd2).expect("queued 2");
        broker.set_max_reply_bytes_for_test(64);

        let cap2 = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-2\",\"sourceSeq\":\"2\",\"nativeInvalid\":\"2\",\"report\":{inner}}}"
        );
        let err = broker.dispatch_capture(&cap2).expect_err("oversized");
        assert!(matches!(err, BrokerError::Oversized));
        assert!(
            broker.settled.is_empty(),
            "late oversize must not settle before final prepare passes"
        );
        assert_eq!(broker.pending.len(), 2);
        assert_eq!(broker.pending[0].client_cmd, "1");
        assert_eq!(broker.pending[1].client_cmd, "2");

        // Retry with normal reply budget and a fresh capture source drains the
        // reparked batch in FIFO order.
        broker.set_max_reply_bytes_for_test(WORLD_MAX_REPLY_BYTES);
        let cap3 = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-3\",\"sourceSeq\":\"3\",\"nativeInvalid\":\"3\",\"report\":{inner}}}"
        );
        let cap3_retry = broker.dispatch_capture(&cap3).expect("retry succeeds");
        assert!(
            cap3_retry.contains("\"rev\":\"2\""),
            "cap3 retry: {cap3_retry}"
        );
        assert!(broker.pending.is_empty());
        let take1: serde_json::Value = serde_json::from_str(
            &broker
                .take_result(
                    ":1.42",
                    &format!(
                        "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"1\"}}"
                    ),
                )
                .expect("take 1"),
        )
        .expect("JSON");
        let take2: serde_json::Value = serde_json::from_str(
            &broker
                .take_result(
                    ":1.42",
                    &format!(
                        "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"2\"}}"
                    ),
                )
                .expect("take 2"),
        )
        .expect("JSON");
        assert_eq!(take1["globalCmd"], "1");
        assert_eq!(take2["globalCmd"], "2");
    }

    #[test]
    fn defensive_duplicate_reparks_without_installing_broker_state() {
        let mut broker = WorldBroker::new();
        let adapter = "kwin-actuator-1";
        let hs = format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"hs-1\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        );
        broker.discover_connection(&hs).expect("handshake");
        let generation = broker.generation().to_owned();
        let settings = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
              \"req\":\"set-1\",\"rev\":\"1\",\
              \"values\":{{\"mode\":\"per-output-local\",\"gap\":8,\"outer\":8,\"bornMaximized\":\"tile-on-launch\"}}}}"
        );
        broker.discover_connection(&settings).expect("settings");

        let win_a = "{\"win\":\"win-a\",\"out\":\"DP-1\",\"rect\":[0,0,200,200],\
            \"minimized\":false,\"maximized\":false,\"fullscreen\":false,\"sticky\":false,\
            \"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null},\
            \"hostHidden\":false,\"privileged\":false,\"managed\":true,\"acceptsInput\":true,\
            \"activationInhibited\":false,\"toolWindow\":false,\"owned\":false,\"transient\":false,\
            \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\"owner\":null,\
            \"parent\":null,\"role\":null,\"appId\":null,\"executable\":null,\
            \"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\"title\":null,\
            \"type\":\"normal\",\"activationPolicy\":\"regular\",\"layer\":0,\
            \"fullscreenOwner\":null,\"fullscreenExitable\":null,\"suppressionAvailable\":null,\
            \"visibility\":{\"visible\":true,\"suppressionOwned\":null}}";
        let inner = format!(
            "{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":[{win_a}],\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[\"win-a\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,200,200]}}],\
            \"witness\":null,\"foregroundOut\":null}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\"nativeGesture\":null,\"cursorWindow\":null,\
            \"nativeInvalidWitness\":{{\"instance\":\"inst-1\",\"counter\":\"5\"}}}}"
        );
        let cap1 = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-1\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\"report\":{inner}}}"
        );
        broker.dispatch_capture(&cap1).expect("initial capture");

        let presentation_before = broker.presentation.clone();
        let covered_before = broker.presentation_covered.clone();
        let watermarks_before = broker.watermarks.clone();
        let responses_before = broker.responses.clone();
        let gesture_map_before = broker.gesture_map.clone();

        let cmd = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"1\",\
            \"command\":{{\"op\":\"focus\",\"win\":\"win-a\"}}}}"
        );
        broker.queue_command(":1.42", &cmd).expect("queued");
        let drained = std::mem::take(&mut broker.pending);

        let mut staged_map = broker.gesture_map.clone();
        let mut fake_raw = serde_json::Map::new();
        fake_raw.insert(
            "nativeGesture".to_owned(),
            serde_json::json!({
                "v": 1, "instance": "g-dup", "serial": "9", "revision": "1",
                "phase": "started", "win": "win-a", "kind": "move",
                "startRect": [0, 0, 200, 200], "startCursor": [10, 10],
                "cursor": [150, 50], "modalMove": 1, "changed": 1,
                "superseded": 0, "available": 1
            }),
        );
        let assoc = staged_map
            .normalize_gesture_lane(&mut fake_raw)
            .expect("staged gesture");
        assert!(assoc.is_some());
        assert_ne!(format!("{staged_map:?}"), format!("{gesture_map_before:?}"));

        let mut cached_value: serde_json::Value =
            serde_json::from_str(&broker.responses.get("src-1").expect("cached").1)
                .expect("cached reply json");
        if let Some(obj) = cached_value.as_object_mut() {
            for key in [
                "source",
                "nativeInvalid",
                "clock",
                "gestureAssoc",
                "gestureSettledAssoc",
            ] {
                obj.remove(key);
            }
        }
        let cached_unbound = serde_json::to_string(&cached_value).expect("serialize unbound");
        let mut cached_reply = tiler_protocol::world_protocol::decode_reply_v4(&cached_unbound)
            .expect("decode cached")
            .reply;
        cached_reply.state_revision = 999;

        let parsed = ParsedCapture {
            adapter: adapter.to_owned(),
            generation: generation.clone(),
            source: "src-dup".to_owned(),
            source_seq: 2,
            native_invalid: 99,
            inner: serde_json::Map::new(),
        };
        let reply = broker
            .finish_capture_outcome(
                &parsed,
                "{\"synthetic\":true}",
                77,
                Some(CoverWitness {
                    instance: "inst-new".to_owned(),
                    counter: "99".to_owned(),
                }),
                staged_map,
                assoc,
                drained,
                WorldOutcome::Duplicate(cached_reply),
                Vec::new(),
            )
            .expect("duplicate finishes");
        assert!(reply.contains("\"rev\":\"999\""));
        let reply_json: serde_json::Value = serde_json::from_str(&reply).expect("reply json");
        assert!(
            reply_json["gestureAssoc"].is_null(),
            "defensive duplicate must not attach unaccepted ingress gestureAssoc"
        );
        assert_eq!(broker.pending.len(), 1, "drained batch reparked");
        assert_eq!(
            format!("{:?}", broker.gesture_map),
            format!("{gesture_map_before:?}")
        );
        assert_eq!(broker.presentation, presentation_before);
        assert_eq!(broker.presentation_covered, covered_before);
        assert_eq!(broker.watermarks, watermarks_before);
        assert_eq!(broker.responses, responses_before);
    }

    #[test]
    fn unit_p_bound_and_projection_error_classification_vs_oversize() {
        let mut broker = WorldBroker::new();
        let adapter = "kwin-actuator-1";
        let hs = format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"hs-1\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        );
        broker.discover_connection(&hs).expect("handshake");
        let generation = broker.generation().to_owned();
        let settings = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
              \"req\":\"set-1\",\"rev\":\"1\",\
              \"values\":{{\"mode\":\"per-output-local\",\"gap\":8,\"outer\":8,\"bornMaximized\":\"tile-on-launch\"}}}}"
        );
        broker.discover_connection(&settings).expect("settings");

        let win_a = "{\"win\":\"win-a\",\"out\":\"DP-1\",\"rect\":[0,0,200,200],\
            \"minimized\":false,\"maximized\":false,\"fullscreen\":false,\"sticky\":false,\
            \"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null},\
            \"hostHidden\":false,\"privileged\":false,\"managed\":true,\"acceptsInput\":true,\
            \"activationInhibited\":false,\"toolWindow\":false,\"owned\":false,\"transient\":false,\
            \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\"owner\":null,\
            \"parent\":null,\"role\":null,\"appId\":null,\"executable\":null,\
            \"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\"title\":null,\
            \"type\":\"normal\",\"activationPolicy\":\"regular\",\"layer\":0,\
            \"fullscreenOwner\":null,\"fullscreenExitable\":null,\"suppressionAvailable\":null,\
            \"visibility\":{\"visible\":true,\"suppressionOwned\":null}}";
        let inner = format!(
            "{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
            \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
            \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
            \"windows\":[{win_a}],\"closed\":[],\"native\":[],\"focused\":\"win-a\",\"foregroundKnown\":true,\
            \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
            \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
            \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
            \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
            \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[\"win-a\"],\
            \"pagerFrames\":[{{\"win\":\"win-a\",\"live\":[0,0,200,200]}}],\
            \"witness\":null,\"foregroundOut\":null}},\
            \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\
            \"gestureIngress\":null,\"nativeGesture\":null,\"cursorWindow\":null,\"nativeInvalidWitness\":null}}"
        );
        let cap1 = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-1\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\"report\":{inner}}}"
        );
        let reply_json = broker.dispatch_capture(&cap1).expect("initial capture");
        let mut reply_val: serde_json::Value =
            serde_json::from_str(&reply_json).expect("reply json");
        assert_eq!(reply_val["ready"], true, "cap1 must be ready: {reply_json}");
        if let Some(obj) = reply_val.as_object_mut() {
            for key in [
                "source",
                "nativeInvalid",
                "clock",
                "gestureAssoc",
                "gestureSettledAssoc",
            ] {
                obj.remove(key);
            }
        }
        let valid_reply = tiler_protocol::world_protocol::decode_reply_v4(
            &serde_json::to_string(&reply_val).expect("serialize unbound"),
        )
        .expect("decode reply")
        .reply;
        let parsed = ParsedCapture {
            adapter: adapter.to_owned(),
            generation: generation.clone(),
            source: "src-2".to_owned(),
            source_seq: 2,
            native_invalid: 2,
            inner: serde_json::Map::new(),
        };

        // 1. Standalone bound writer: small invalid warp -> InvalidReply("unknown-value"), never Oversized.
        let mut bad_rect = valid_reply.clone();
        bad_rect.cursor_warp = Some((32769, 0));
        let bind = BrokerReplyBind {
            source: &parsed.source,
            native_invalid: parsed.native_invalid,
            clock: 2,
            gesture_assoc: None,
            gesture_settled_assoc: None,
        };
        let err = map_protocol_reply_error(
            tiler_protocol::world_reply_write::write_bound_reply_v4(&bad_rect, None, &bind)
                .expect_err("bad warp must fail"),
        );
        assert_eq!(err, BrokerError::InvalidReply("unknown-value"));

        // 2. Standalone bound writer: fixed semantic structural token -> InvalidReply("duplicate-scope").
        let mut bad_semantic = valid_reply.clone();
        if let Some(first_scope) = bad_semantic.scopes.first().cloned() {
            bad_semantic.scopes.push(first_scope);
        }
        let err = map_protocol_reply_error(
            tiler_protocol::world_reply_write::write_bound_reply_v4(&bad_semantic, None, &bind)
                .expect_err("duplicate-scope must fail"),
        );
        assert_eq!(err, BrokerError::InvalidReply("duplicate-scope"));

        // 3. Projection writer error translation -> InvalidReply, never Oversized.
        let mut bad_proj = valid_reply.clone();
        bad_proj.presentation.foreground = "bad id with spaces".to_owned();
        let proj_err =
            tiler_protocol::world_reply_write::write_projection_reply_v4(&bad_proj, None)
                .expect_err("projection must fail");
        assert_eq!(
            map_protocol_reply_error(proj_err),
            BrokerError::InvalidReply("unknown-value")
        );

        // 4. Broker prevalidated prepare: envelope-only bind.source check -> InvalidReply, bound overrun -> Oversized.
        let bad_bind_parsed = ParsedCapture {
            adapter: parsed.adapter.clone(),
            generation: parsed.generation.clone(),
            source: "bad source".to_owned(),
            source_seq: parsed.source_seq,
            native_invalid: parsed.native_invalid,
            inner: serde_json::Map::new(),
        };
        let mut map = broker.gesture_map.clone();
        let err = prepare_capture_reply_checked(
            &valid_reply,
            None,
            true,
            WORLD_MAX_REPLY_BYTES,
            &bad_bind_parsed,
            2,
            &mut map,
            None,
        )
        .err()
        .expect("bad bind source must fail as InvalidReply");
        assert_eq!(err, BrokerError::InvalidReply("unknown-value"));

        let mut map = broker.gesture_map.clone();
        let err =
            prepare_capture_reply_checked(&valid_reply, None, true, 64, &parsed, 2, &mut map, None)
                .err()
                .expect("bound overrun must fail as Oversized");
        assert_eq!(err, BrokerError::Oversized);

        // 5. Explicit Send to ws-2 then select-trailing over real broker produces distinct ws-3 without duplicate-workspace.
        let send_ws2_cmd = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"1\",\
            \"command\":{{\"op\":\"send\",\"win\":\"win-a\",\"out\":\"DP-1\",\"ws\":\"ws-2\",\"follow\":false}}}}"
        );
        broker
            .queue_command(":1.55", &send_ws2_cmd)
            .expect("queue send ws-2");
        let cap2 = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-2\",\"sourceSeq\":\"2\",\"nativeInvalid\":\"2\",\"report\":{inner}}}"
        );
        let cap2_reply = broker.dispatch_capture(&cap2).expect("cap2 succeeds");
        let cap2_json: serde_json::Value = serde_json::from_str(&cap2_reply).expect("json");
        let ws_ids: Vec<&str> = cap2_json["workspaces"]
            .as_array()
            .expect("workspaces")
            .iter()
            .map(|w| w["id"].as_str().expect("id"))
            .collect();
        assert_eq!(ws_ids, vec!["ws-1", "ws-2"]);

        let select_cmd = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"2\",\
            \"command\":{{\"op\":\"select-trailing\",\"out\":\"DP-1\"}}}}"
        );
        broker
            .queue_command(":1.55", &select_cmd)
            .expect("queue select");
        let cap3 = format!(
            "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
            \"source\":\"src-3\",\"sourceSeq\":\"3\",\"nativeInvalid\":\"3\",\"report\":{inner}}}"
        );
        let cap3_reply = broker.dispatch_capture(&cap3).expect("cap3 succeeds");
        let cap3_json: serde_json::Value = serde_json::from_str(&cap3_reply).expect("json");
        let ws_ids3: Vec<&str> = cap3_json["workspaces"]
            .as_array()
            .expect("workspaces")
            .iter()
            .map(|w| w["id"].as_str().expect("id"))
            .collect();
        assert_eq!(ws_ids3, vec!["ws-1", "ws-2", "ws-3"]);
    }

    fn build_overflow_settings_request(
        adapter: &str,
        generation: &str,
        req: &str,
        rev: &str,
        unknown_key_count: usize,
        extra_pairs: &[(&str, &str)],
    ) -> String {
        let mut out = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\"req\":\"{req}\",\"rev\":\"{rev}\",\"values\":{{"
        );
        let mut first = true;
        for &(k, v) in extra_pairs {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!("\"{k}\":{v}"));
        }
        for i in 0..unknown_key_count {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!("\"u{i:05}\":0"));
        }
        out.push_str("}}");
        out
    }

    #[test]
    fn settings_reply_overflow_within_request_bound_refuses_without_mutating_readiness_or_layout() {
        let mut broker = WorldBroker::new();
        let adapter = "a";
        let hs = format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"h\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\"maximize\":true,\"fullscreen\":true,\
              \"nativeSticky\":true,\"above\":false,\"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        );
        let hs_reply: serde_json::Value =
            serde_json::from_str(&broker.discover_connection(&hs).expect("handshake succeeds"))
                .expect("handshake json");
        let generation = hs_reply["gen"].as_str().expect("gen").to_owned();

        // 105,000 6-char unknown keys (~1.16 MiB request, < WORLD_MAX_REQUEST_BYTES) expand to ~4.20 MiB (> WORLD_MAX_REPLY_BYTES).
        let overflow_keys = 105_000;

        // 1. Before readiness: overflowing rev 0 refuses as Oversized without mutating readiness or layout.
        let overflow_rev0 = build_overflow_settings_request(
            adapter,
            &generation,
            "s0",
            "0",
            overflow_keys,
            &[
                ("gap", "17"),
                ("predicate", "\"either-axis-fixed\""),
                ("freshPlacement", "\"focused-output-last-focused\""),
            ],
        );
        assert!(overflow_rev0.len() <= WORLD_MAX_REQUEST_BYTES);
        assert_eq!(
            broker
                .discover_connection(&overflow_rev0)
                .expect_err("overflowing rev 0 must fail as Oversized"),
            BrokerError::Oversized
        );
        assert_eq!(
            broker.engine.settings().fixed_size_predicate,
            tiler_core::size_hints::FixedSizePredicate::BothAxes
        );
        assert_eq!(
            broker.engine.settings().fresh_placement,
            tiler_core::cursor_targeting::FreshPlacement::CursorOutputMru
        );

        let fixed_width_report = |win: &str| {
            format!(
                "{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
                \"identity\":{{\"manufacturer\":\"Synthetic\",\"model\":\"Panel\",\"serial\":\"s-1\",\"connector\":\"DP-1\"}}}}],\
                \"windows\":[{{\"win\":\"{win}\",\"out\":\"DP-1\",\"rect\":[0,0,400,400],\"minimized\":false,\"maximized\":false,\"fullscreen\":false,\"sticky\":false,\
                \"hints\":{{\"minW\":400,\"minH\":100,\"maxW\":400,\"maxH\":900}},\"hostHidden\":false,\"privileged\":false,\"managed\":true,\"acceptsInput\":true,\
                \"activationInhibited\":false,\"toolWindow\":false,\"owned\":false,\"transient\":false,\"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\
                \"owner\":null,\"parent\":null,\"role\":\"normal\",\"appId\":\"org.example.fw\",\"executable\":\"/usr/bin/fw\",\"processId\":\"pid-foreign-1\",\
                \"class\":\"Synthetic\",\"title\":\"FW\",\"type\":\"normal\",\"activationPolicy\":\"regular\",\"layer\":0,\"fullscreenOwner\":null,\
                \"fullscreenExitable\":null,\"suppressionAvailable\":false,\"visibility\":{{\"visible\":true,\"suppressionOwned\":false}}}}],\
                \"closed\":[],\"native\":[],\"focused\":\"{win}\",\"foregroundKnown\":true,\"complete\":true,\"gesture\":false,\"cursor\":[100,100],\
                \"actionForeground\":\"DP-1\",\"actionCached\":\"DP-1\",\"actionOutputs\":[\"DP-1\"],\"presentation\":{{\"foreground\":\"{win}\",\"minimized\":false,\
                \"maximized\":false,\"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":[0,0,400,400],\"chordHeld\":false,\"moveWindow\":\"\",\
                \"pagerOrder\":[\"{win}\"],\"pagerFrames\":[{{\"win\":\"{win}\",\"live\":[0,0,400,400]}}],\"witness\":null,\"foregroundOut\":\"DP-1\"}},\
                \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\"gestureIngress\":null,\"nativeGesture\":null,\
                \"cursorWindow\":null,\"nativeInvalidWitness\":null}}"
            )
        };
        let cap_pre: serde_json::Value = serde_json::from_str(
            &broker
                .dispatch_capture(&format!(
                    "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\"source\":\"src-1\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\"report\":{}}}",
                    fixed_width_report("win-fw")
                ))
                .expect("capture routes"),
        )
        .expect("json");
        assert!(!broker.engine.is_ready());
        assert_eq!(cap_pre["ready"], false);

        // 2. Fitting rev 0 succeeds and activates readiness + BothAxes predicate.
        let fit_values = |gap: i32, mode: &str, pred: &str, fresh: &str| {
            format!(
                "{{\"gap\":{gap},\"outer\":8,\"defaultTiled\":true,\"mode\":\"{mode}\",\"sameAxis\":\"group-with-neighbor\",\
                \"bornMaximized\":\"tile-on-launch\",\"predicate\":\"{pred}\",\"refill\":\"last-remaining-workspace\",\
                \"switchTarget\":\"cursor-output\",\"freshPlacement\":\"{fresh}\",\"focusFollowsCursor\":false,\"cursorWarp\":false,\
                \"borderEnabled\":false,\"underlayEnabled\":false,\"borderWidth\":2,\"borderGap\":0,\"borderRadius\":0,\"underlayExtension\":0}}"
            )
        };
        let fit0_req = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\"req\":\"s0-fit\",\"rev\":\"0\",\"values\":{}}}",
            fit_values(8, "shared", "both-axes-fixed", "cursor-output-last-focused")
        );
        let fit0_reply: serde_json::Value =
            serde_json::from_str(&broker.discover_connection(&fit0_req).expect("fit rev 0"))
                .expect("json");
        assert_eq!(fit0_reply["status"], "accepted");
        let cap_ready: serde_json::Value = serde_json::from_str(
            &broker
                .dispatch_capture(&format!(
                    "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\"source\":\"src-2\",\"sourceSeq\":\"2\",\"nativeInvalid\":\"2\",\"report\":{}}}",
                    fixed_width_report("win-fw")
                ))
                .expect("capture ready"),
        )
        .expect("json");
        assert!(broker.engine.is_ready());
        assert!(cap_ready["windows"][0]["alloc"].is_array());

        // 3. After readiness: overflowing rev 1 refuses without mutating values, revision, or layout;
        //    duplicate/reused rev 0 remain valid, then fitting rev 1 succeeds with restart-required.
        let overflow_rev1 = build_overflow_settings_request(
            adapter,
            &generation,
            "s1",
            "1",
            overflow_keys,
            &[
                ("gap", "24"),
                ("mode", "\"global-unique\""),
                ("predicate", "\"either-axis-fixed\""),
                ("freshPlacement", "\"focused-output-last-focused\""),
            ],
        );
        assert_eq!(
            broker
                .discover_connection(&overflow_rev1)
                .expect_err("overflowing rev 1 must fail as Oversized"),
            BrokerError::Oversized
        );
        assert_eq!(
            broker.engine.settings().fixed_size_predicate,
            tiler_core::size_hints::FixedSizePredicate::BothAxes
        );

        let dup0: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&fit0_req.replace("\"s0-fit\"", "\"s0-dup\""))
                .expect("dup rev 0"),
        )
        .expect("json");
        assert_eq!(dup0["status"], "accepted");
        let reused0: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(
                    &fit0_req
                        .replace("\"gap\":8", "\"gap\":12")
                        .replace("\"s0-fit\"", "\"s0-reuse\""),
                )
                .expect("reused rev 0"),
        )
        .expect("json");
        assert_eq!(reused0["status"], "revision-reused");

        let fit1_req = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\"req\":\"s1-fit\",\"rev\":\"1\",\"values\":{}}}",
            fit_values(
                24,
                "global-unique",
                "either-axis-fixed",
                "focused-output-last-focused"
            )
        );
        let fit1_reply: serde_json::Value =
            serde_json::from_str(&broker.discover_connection(&fit1_req).expect("fit rev 1"))
                .expect("json");
        assert_eq!(fit1_reply["status"], "accepted");
        assert_eq!(
            fit1_reply["diagnostics"],
            serde_json::json!([{"key": "mode", "kind": "restart-required"}])
        );

        let cap_after: serde_json::Value = serde_json::from_str(
            &broker
                .dispatch_capture(&format!(
                    "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\"source\":\"src-3\",\"sourceSeq\":\"3\",\"nativeInvalid\":\"3\",\"report\":{}}}",
                    fixed_width_report("win-fw-2")
                ))
                .expect("capture after rev 1"),
        )
        .expect("json");
        assert_eq!(cap_after["windows"][0]["floating"], true);

        let stale0: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&fit0_req.replace("\"s0-fit\"", "\"s0-stale\""))
                .expect("stale rev 0"),
        )
        .expect("json");
        assert_eq!(stale0["status"], "stale");
    }

    #[test]
    fn remember_response_byte_cap_evicts_oldest_preserves_replay_and_resets_with_generation() {
        let mut broker = WorldBroker::new();
        let adapter = "byte-cap-adapter";
        let hs: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&handshake_message(adapter, "hs-1"))
                .expect("handshake"),
        )
        .expect("json");
        let generation = hs["gen"].as_str().expect("gen").to_owned();
        let settings: serde_json::Value = serde_json::from_str(
            &broker
                .discover_connection(&settings_message(adapter, &generation, "set-1"))
                .expect("settings"),
        )
        .expect("json");
        assert_eq!(settings["status"], "accepted");

        // Near-request-limit valid capture pairs so combined request + bound
        // reply payloads trigger MAX_REMEMBERED_RESPONSE_BYTES (32 MiB) well
        // below MAX_REMEMBERED_SOURCES (1024).
        let title_pad = "p".repeat(3500);
        let build_large_envelope = |source: &str, seq: u64, title_tag: &str| -> String {
            let mut windows = String::with_capacity(2_600_000);
            for i in 0..600 {
                if i > 0 {
                    windows.push(',');
                }
                let win = format!("w-{seq}-{i}");
                let x = (i % 10) * 100;
                let y = (i / 10) * 10;
                windows.push_str(&format!(
                    "{{\"win\":\"{win}\",\"out\":\"DP-1\",\"rect\":[{x},{y},100,100],\"minimized\":false,\"maximized\":null,\"fullscreen\":null,\"sticky\":null,\
                    \"hints\":{{\"minW\":10,\"minH\":10,\"maxW\":1920,\"maxH\":1080}},\"hostHidden\":false,\"privileged\":false,\"managed\":true,\"acceptsInput\":true,\
                    \"activationInhibited\":false,\"toolWindow\":false,\"owned\":false,\"transient\":false,\"modal\":false,\"resizable\":true,\"above\":null,\"below\":null,\
                    \"owner\":null,\"parent\":null,\"role\":\"normal\",\"appId\":\"org.example.cap\",\"executable\":\"/usr/bin/cap\",\"processId\":\"pid-cap-1\",\
                    \"class\":\"Synthetic\",\"title\":\"{title_tag}-{i:04}-{title_pad}\",\"type\":\"normal\",\"activationPolicy\":\"regular\",\"layer\":0,\"fullscreenOwner\":null,\
                    \"fullscreenExitable\":null,\"suppressionAvailable\":false,\"visibility\":{{\"visible\":true,\"suppressionOwned\":false}}}}"
                ));
            }
            format!(
                "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\"source\":\"{source}\",\"sourceSeq\":\"{seq}\",\"nativeInvalid\":\"1\",\
                \"report\":{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
                \"identity\":{{\"manufacturer\":\"Synthetic\",\"model\":\"Panel\",\"serial\":\"s-1\",\"connector\":\"DP-1\"}}}}],\
                \"windows\":[{windows}],\"closed\":[],\"native\":[],\"focused\":\"w-{seq}-0\",\"foregroundKnown\":true,\"complete\":true,\"gesture\":false,\"cursor\":[100,100],\
                \"actionForeground\":\"DP-1\",\"actionCached\":\"DP-1\",\"actionOutputs\":[\"DP-1\"],\"presentation\":{{\"foreground\":\"w-{seq}-0\",\"minimized\":false,\
                \"maximized\":false,\"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":[0,0,100,100],\"chordHeld\":false,\"moveWindow\":\"\",\
                \"pagerOrder\":[],\"pagerFrames\":[],\"witness\":null,\"foregroundOut\":\"DP-1\"}},\
                \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null,\"gestureIngress\":null,\"nativeGesture\":null,\
                \"cursorWindow\":null,\"nativeInvalidWitness\":null}}}}"
            )
        };

        let env_1 = build_large_envelope("src-1", 1, "t1");
        assert!(
            env_1.len() > WORLD_MAX_REQUEST_BYTES / 2 && env_1.len() <= WORLD_MAX_REQUEST_BYTES,
            "expected near-limit request, got {} bytes",
            env_1.len()
        );
        let rep_1 = broker.dispatch_capture(&env_1).expect("dispatch src-1");
        assert!(rep_1.len() <= WORLD_MAX_REPLY_BYTES);
        let rep_1_json: serde_json::Value = serde_json::from_str(&rep_1).expect("json");
        assert_eq!(rep_1_json["ready"], true, "{rep_1_json}");
        assert_eq!(
            rep_1_json["diagnostics"].as_array().map(Vec::len),
            Some(0),
            "expected non-held admitted windows: {rep_1_json}"
        );
        assert_eq!(
            rep_1_json["scopes"][0]["auth"], "controlled",
            "expected controlled scope: {rep_1_json}"
        );
        assert_eq!(
            rep_1_json["outputs"][0]["hold"], "",
            "expected non-held output: {rep_1_json}"
        );
        assert_eq!(
            rep_1_json["presentation"]["pager"][0]["workspaces"][0]["occupied"], true,
            "expected admitted windows occupying workspace: {rep_1_json}"
        );

        // Exact replay returns identical bytes.
        assert_eq!(
            broker.dispatch_capture(&env_1).expect("replay src-1"),
            rep_1
        );
        // Changed bytes for remembered source refuse as source-reused.
        let env_1_changed = env_1.replacen("\"t1-0000-", "\"t1-diff-", 1);
        let reused_json: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&env_1_changed).expect("reused"))
                .expect("json");
        assert_eq!(reused_json["kind"], "source-reused");

        // Pre-sequencing malformed gesture rejection with queued work:
        // remembers bound rejection without advancing source_floor or draining.
        let q_pre = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c-pre\",\"clientCmd\":\"1\",\
            \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
        );
        let q_pre_rep: serde_json::Value =
            serde_json::from_str(&broker.queue_command(":1.41", &q_pre).expect("queue pre"))
                .expect("json");
        assert_eq!(q_pre_rep["outcome"], "queued");
        assert_eq!(broker.pending.len(), 1);

        let mut bad_gesture_env = build_large_envelope("src-bad", 2, "tbad");
        bad_gesture_env =
            bad_gesture_env.replacen("\"nativeGesture\":null", "\"nativeGesture\":{\"bad\":1}", 1);
        let bad_rep = broker
            .dispatch_capture(&bad_gesture_env)
            .expect("bad gesture rejection");
        let bad_json: serde_json::Value = serde_json::from_str(&bad_rep).expect("json");
        assert_eq!(bad_json["outcome"], "rejected");
        assert_eq!(broker.pending.len(), 1);
        assert_eq!(
            broker
                .dispatch_capture(&bad_gesture_env)
                .expect("bad gesture replay"),
            bad_rep
        );
        assert_eq!(broker.pending.len(), 1);
        assert_eq!(broker.source_floor, 1);

        // Fill past 32 MiB using actual accepted bound captures (count << 1024).
        let mut last_seq = 2;
        let mut last_env = String::new();
        let mut last_rep = String::new();
        while broker.responses.contains_key("src-1") {
            last_seq += 1;
            assert!(
                last_seq <= 40,
                "expected byte-induced eviction within 40 captures, len={}, bytes={}",
                broker.responses.len(),
                broker.response_bytes
            );
            let src = format!("src-{last_seq}");
            let env = build_large_envelope(&src, last_seq, "fill");
            let rep = broker.dispatch_capture(&env).expect("fill dispatch");
            let rep_json: serde_json::Value = serde_json::from_str(&rep).expect("fill json");
            assert_eq!(rep_json["ready"], true, "fill must be accepted: {rep_json}");
            last_env = env;
            last_rep = rep;
        }
        assert!(broker.pending.is_empty());
        assert!(broker.responses.len() < MAX_REMEMBERED_SOURCES);
        assert!(broker.response_bytes <= MAX_REMEMBERED_RESPONSE_BYTES);
        let exact_sum: usize = broker
            .responses
            .values()
            .map(|(req, rep)| req.len() + rep.len())
            .sum();
        assert_eq!(broker.response_bytes, exact_sum);

        // Queue one command: evicted src-1 refuses as stale-source and does not
        // drain; retained last source replays identically and does not drain;
        // fresh source drains the command once only.
        let q_req = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"c1\",\"clientCmd\":\"1\",\
            \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
        );
        let q_rep: serde_json::Value =
            serde_json::from_str(&broker.queue_command(":1.42", &q_req).expect("queue"))
                .expect("json");
        assert_eq!(q_rep["outcome"], "queued");
        assert_eq!(broker.pending.len(), 1);

        let stale_evicted: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&env_1).expect("evicted stale"))
                .expect("json");
        assert_eq!(stale_evicted["kind"], "stale-source");
        assert_eq!(broker.pending.len(), 1);

        assert_eq!(
            broker.dispatch_capture(&last_env).expect("retained replay"),
            last_rep
        );
        assert_eq!(broker.pending.len(), 1);

        let fresh_seq = last_seq + 1;
        let fresh_env = build_large_envelope(&format!("src-{fresh_seq}"), fresh_seq, "fresh");
        let fresh_rep: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&fresh_env).expect("fresh drain"))
                .expect("json");
        assert_eq!(fresh_rep["ready"], true, "{fresh_rep}");
        assert!(broker.pending.is_empty());

        // Non-retainable pair (> MAX_REMEMBERED_RESPONSE_BYTES) is never copied
        // or retained and does not evict existing entries.
        let count_before = broker.responses.len();
        let bytes_before = broker.response_bytes;
        let oversized_bound = "x".repeat(MAX_REMEMBERED_RESPONSE_BYTES + 1);
        broker.remember_response("non-retainable", b"req", &oversized_bound);
        assert!(!broker.responses.contains_key("non-retainable"));
        assert_eq!(broker.responses.len(), count_before);
        assert_eq!(broker.response_bytes, bytes_before);

        // Reset with broker generation clears count, order, bytes, and floor.
        let _hs2 = broker
            .discover_connection(&handshake_message("byte-cap-adapter-2", "hs-2"))
            .expect("handshake reset");
        assert!(broker.responses.is_empty());
        assert!(broker.response_order.is_empty());
        assert_eq!(broker.response_bytes, 0);
        assert_eq!(broker.source_floor, 0);
    }

    #[test]
    fn b6_strict_ingress_rejects_duplicates_and_preserves_precedence() {
        let mut broker = WorldBroker::new();
        let _ = broker
            .discover_connection(&handshake_message("b6-adapter", "hs-1"))
            .expect("handshake");
        let _ = broker
            .discover_connection(&settings_message(
                "b6-adapter",
                broker.generation(),
                "set-1",
            ))
            .expect("settings");
        let generation = broker.generation().to_owned();
        let stale_gen = "stale-gen-0";

        let rejection_kind = |res: Result<String, BrokerError>| -> String {
            let body = res.expect("in-band reply");
            let v: serde_json::Value = serde_json::from_str(&body).expect("json");
            assert_eq!(v["outcome"], "rejected", "{v}");
            v["kind"].as_str().expect("kind").to_owned()
        };

        // Top-level duplicates on all 4 endpoints (duplicate beats version/gen/fields).
        let dup_queue = format!(
            "{{\"v\":3,\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\
              \"clientCmd\":\"1\",\"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &dup_queue)),
            "malformed"
        );
        let dup_take = format!(
            "{{\"v\":3,\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\"}}"
        );
        assert_eq!(
            rejection_kind(broker.take_result(":1.42", &dup_take)),
            "malformed"
        );
        let dup_wait = format!(
            "{{\"v\":3,\"v\":4,\"gen\":\"{generation}\",\"adapter\":\"b6-adapter\",\
              \"nonce\":\"w-1\",\"cursor\":\"0\"}}"
        );
        assert_eq!(
            parse_wait_request(&dup_wait).expect_err("dup wait").0,
            "malformed"
        );
        let dup_read = r#"{"v":3,"v":4,"req":"r-1"}"#;
        assert_eq!(
            rejection_kind(broker.read_presentation(dup_read)),
            "malformed"
        );

        // Nested and escaped-equivalent keys on Queue.
        let nested_dup = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"select-numbered\",\"op\":\"focus\",\"out\":null,\"index\":1}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &nested_dup)),
            "malformed"
        );
        let deep_dup = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1,\
              \"extra\":[{{\"k\":1,\"k\":2}}]}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &deep_dup)),
            "malformed"
        );
        let escaped_top = format!(
            "{{\"v\":4,\"\\u0076\":3,\"gen\":\"{generation}\",\"client\":\"panel-1\",\
              \"clientCmd\":\"1\",\"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &escaped_top)),
            "malformed"
        );
        let escaped_nested = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"select-numbered\",\"\\u006fp\":\"focus\",\"out\":null,\"index\":1}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &escaped_nested)),
            "malformed"
        );

        // Mixed precedence: oversize > auth > malformed(dup) > keys > version > field-values > gen > command-semantics.
        let oversize = format!(
            "{{\"v\":4,\"v\":3,\"pad\":\"{}\"}}",
            "x".repeat(WORLD_MAX_REQUEST_BYTES)
        );
        assert!(matches!(
            broker.queue_command("", &oversize),
            Err(BrokerError::Oversized)
        ));
        assert!(matches!(
            broker.take_result("", &oversize),
            Err(BrokerError::Oversized)
        ));
        assert!(matches!(
            broker.read_presentation(&oversize),
            Err(BrokerError::Oversized)
        ));

        assert_eq!(
            rejection_kind(broker.queue_command("", &dup_queue)),
            "unauthorized"
        );
        assert_eq!(
            rejection_kind(broker.take_result("", &dup_take)),
            "unauthorized"
        );

        // Duplicate beats unknown-field, unsupported-version, bad values, stale gen, bad command.
        let dup_mixed = format!(
            "{{\"v\":3,\"v\":9,\"extra\":1,\"gen\":\"{stale_gen}\",\"client\":\"\",\
              \"clientCmd\":\"01\",\"command\":{{\"op\":\"bogus\"}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &dup_mixed)),
            "malformed"
        );
        // Unknown-field beats unsupported-version.
        let unknown_vs_ver = format!(
            "{{\"v\":3,\"extra\":1,\"gen\":\"{stale_gen}\",\"client\":\"\",\"clientCmd\":\"01\"}}"
        );
        assert_eq!(
            rejection_kind(broker.take_result(":1.42", &unknown_vs_ver)),
            "unknown-field"
        );
        assert_eq!(
            rejection_kind(broker.read_presentation(r#"{"v":3,"extra":1,"req":""}"#)),
            "unknown-field"
        );
        // Unsupported-version beats bad field-values and stale gen.
        let ver_vs_val = format!(
            "{{\"v\":3,\"gen\":\"{stale_gen}\",\"client\":\"\",\"clientCmd\":\"01\",\
              \"command\":{{\"op\":\"bogus\"}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &ver_vs_val)),
            "unsupported-version"
        );
        assert_eq!(
            parse_wait_request(&format!(
                "{{\"v\":3,\"gen\":\"{stale_gen}\",\"adapter\":\"\",\"nonce\":\"\",\"cursor\":\"01\"}}"
            ))
            .expect_err("ver beats val")
            .0,
            "unsupported-version"
        );
        // Bad field-value beats stale gen.
        let val_vs_gen = format!(
            "{{\"v\":4,\"gen\":\"{stale_gen}\",\"client\":\"panel-1\",\"clientCmd\":\"01\",\
              \"command\":{{\"op\":\"bogus\"}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &val_vs_gen)),
            "unknown-value"
        );
        assert_eq!(
            parse_wait_request(&format!(
                "{{\"v\":4,\"gen\":\"{stale_gen}\",\"adapter\":\"b6-adapter\",\"nonce\":\"w-1\",\"cursor\":\"01\"}}"
            ))
            .expect_err("val beats gen")
            .0,
            "unknown-value"
        );
        assert_eq!(
            rejection_kind(broker.read_presentation(r#"{"v":4,"req":""}"#)),
            "unknown-value"
        );
        // Serving generation beats command semantics.
        let gen_vs_sem = format!(
            "{{\"v\":4,\"gen\":\"{stale_gen}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"bogus\"}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &gen_vs_sem)),
            "generation-mismatch"
        );
        // Valid gen + bad command semantics -> snapshot-invalid.
        let bad_sem = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"bogus\"}}}}"
        );
        assert_eq!(
            rejection_kind(broker.queue_command(":1.42", &bad_sem)),
            "snapshot-invalid"
        );

        // No queue/highwater/presentation progress from all those refusals.
        assert!(broker.pending.is_empty());
        assert!(broker.highwater.is_empty());
        assert_eq!(
            rejection_kind(broker.read_presentation(r#"{"v":4,"req":"r-pre"}"#)),
            "not-ready"
        );

        // Clean valid controls: queue, capture drain, take refusal does not consume, then valid take & read.
        let valid_queue = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
              \"command\":{{\"op\":\"select-numbered\",\"out\":null,\"index\":1}}}}"
        );
        let q_rep: serde_json::Value = serde_json::from_str(
            &broker
                .queue_command(":1.42", &valid_queue)
                .expect("queue ok"),
        )
        .expect("json");
        assert_eq!(q_rep["outcome"], "queued");
        assert_eq!(q_rep["globalCmd"], "1");

        let valid_wait = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"adapter\":\"b6-adapter\",\
              \"nonce\":\"w-ok\",\"cursor\":\"0\"}}"
        );
        assert!(parse_wait_request(&valid_wait).is_ok());

        let cap_env = format!(
            "{{\"v\":4,\"adapter\":\"b6-adapter\",\"gen\":\"{generation}\",\
              \"source\":\"src-1\",\"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\
              \"report\":{{\"v\":4,\"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
              \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
              \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
              \"windows\":[],\"closed\":[],\"native\":[],\"focused\":\"\",\"foregroundKnown\":false,\
              \"complete\":true,\"gesture\":false,\"cursor\":null,\"actionForeground\":null,\
              \"actionCached\":null,\"actionOutputs\":[\"DP-1\"],\
              \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
              \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\
              \"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[],\"pagerFrames\":[],\
              \"witness\":null,\"foregroundOut\":null}},\
              \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\"rendererInput\":null}}}}"
        );
        let cap_rep: serde_json::Value =
            serde_json::from_str(&broker.dispatch_capture(&cap_env).expect("capture"))
                .expect("json");
        assert_eq!(cap_rep["ready"], true, "{cap_rep}");

        // Refused Takes must not consume the settled result.
        assert_eq!(
            rejection_kind(broker.take_result(":1.42", &dup_take)),
            "malformed"
        );
        let valid_take = format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\"}}"
        );
        let take_rep: serde_json::Value =
            serde_json::from_str(&broker.take_result(":1.42", &valid_take).expect("take ok"))
                .expect("json");
        assert_eq!(take_rep["outcome"], "ready");

        // Refused Read must not disturb presentation.
        assert_eq!(
            rejection_kind(broker.read_presentation(dup_read)),
            "malformed"
        );
        let read_rep: serde_json::Value = serde_json::from_str(
            &broker
                .read_presentation(r#"{"v":4,"req":"r-ok"}"#)
                .expect("read ok"),
        )
        .expect("json");
        assert_eq!(read_rep["outcome"], "presentation");
        assert_eq!(read_rep["rev"], "1");
    }

    #[test]
    fn compact_json_limit_and_map_decode_match_serialized_roundtrip() {
        let raw = concat!(
            r#"{"negZero":-0.0,"float":1.2345678901234567,"sci":1e20,"#,
            r#""ctrl":"\u0000\n\t\"\\","utf8":"cafe\u00e9","nested":{"a":[0.1,-0.0]}}"#
        );
        let map = tiler_protocol::connection_protocol::parse_strict_json(raw)
            .expect("strict parse")
            .as_object()
            .cloned()
            .expect("object");
        let serialized_len = serde_json::Value::Object(map.clone()).to_string().len();
        assert!(!compact_json_exceeds_limit(&map, serialized_len));
        assert!(compact_json_exceeds_limit(&map, serialized_len - 1));

        let mut stamped = tiler_protocol::connection_protocol::parse_strict_json(include_str!(
            "../../../test-fixtures/world-wire/v4-active/messages/report-foreground-known-bound.json"
        ))
        .expect("strict parse")
        .as_object()
        .cloned()
        .expect("object");
        let broker = WorldBroker::new();
        broker.stamp_inner(&mut stamped, "kde", "r-1", 1, 1, 100);
        let stamped_json = serde_json::Value::Object(stamped.clone()).to_string();
        assert!(!compact_json_exceeds_limit(&stamped, stamped_json.len()));
        assert!(compact_json_exceeds_limit(&stamped, stamped_json.len() - 1));
        assert_eq!(
            decode_report_v4_map(&stamped).expect("parsed report decodes"),
            tiler_protocol::world_protocol::decode_request_with_wire(&stamped_json)
                .expect("serialized report decodes"),
        );
    }

    #[test]
    fn cached_projection_bind_read_matches_legacy_value_oracle() {
        fn legacy_project_read(
            cached_json: &str,
            generation: &str,
            req: &str,
            covered: Option<&CoverWitness>,
        ) -> String {
            let mut value: serde_json::Value =
                serde_json::from_str(cached_json).expect("valid cached projection json");
            let map = value.as_object_mut().expect("projection object");
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
            value.to_string()
        }

        let reply = tiler_protocol::world_protocol::decode_reply_v4(include_str!(
            "../../../test-fixtures/world-wire/v4-active/messages/reply-presentation-hidden-suspended.json"
        ))
        .expect("decode reply")
        .reply;
        let generation = "gen-1";

        let cached = write_cached_projection_v4(&reply, None).expect("cached projection");
        let legacy_json = write_projection_reply_v4(&reply, None).expect("legacy projection");
        let witness = CoverWitness {
            instance: "inst-\"quoted\"\\1".to_owned(),
            counter: "42".to_owned(),
        };

        assert_eq!(
            project_read(&cached, generation, "req-1", Some(&witness)),
            legacy_project_read(&legacy_json, generation, "req-1", Some(&witness)),
        );
        assert_eq!(
            project_read(&cached, generation, "req-2", None),
            legacy_project_read(&legacy_json, generation, "req-2", None),
        );
    }
}
