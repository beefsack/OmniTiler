#![forbid(unsafe_code)]

//! World-state runtime session (Unit B adoption).
//!
//! Engine-owner half of the macOS world route: one retained
//! [`WorldEngine`](tiler_core::world_engine::WorldEngine) plus the
//! adapter-side [`WorldSequencer`](crate::world_sequencer::WorldSequencer)
//! replace the legacy per-domain `Engine.handle` / `CoreReply`
//! aggregation and cursors. Retired paths are deleted, not dual-run: every
//! observation and every command converges through one complete
//! [`WorldReportV4`](tiler_core::world_report::WorldReportV4) and actuates
//! from one complete
//! [`WorldReplyV4`](tiler_core::world_report::WorldReplyV4).
//!
//! Threading: the engine thread owns the [`WorldSession`] (never shared).
//! Observation truth still lives in
//! [`RetainedCore`](crate::runtime_core::RetainedCore) (per-app windows,
//! observed focus, window homing); this module shapes that truth into
//! reports via [`build_report`](crate::world_builder::build_report).
//! Dispatching one report per job and consuming its reply into frame
//! slots, the focus outbox, select claims, and attempt feedback lives in
//! [`crate::world_dispatch`]. No AX here, no blocking on workers.
//!
//! Boundaries preserved from the legacy route:
//! - AppKit main / engine-owner / per-app AX worker ownership; per-app
//!   generations and latest-wins frame/focus slots; observed focus is
//!   truth (planned focus never promotes).
//! - Native Space capability refusals precede actuation. Armed selects/sends
//!   use world commands; active Meta drag uses gesture ingress; other tiling
//!   commands use [`WorldCommand::Operation`](tiler_core::world_report::WorldCommand).
//! - Hidden/unreadable scopes never actuate: frames emit only for
//!   currently-actuated domains carrying `Controlled` frame authority with
//!   an effective frame. Holds preserve native state (no writes).
//! - The reply snapshot is plan truth, never a success claim — feedback
//!   rows stage only proven drain outcomes.
//! - Empty native contexts never actuate (same as the KDE null): native
//!   mode with no derived context refuses instead of reporting.
//!
//! Geometry: screen rows carry FULL display bounds plus the RAW visible work
//! area with [`PHASE1_OUTER_GAP`](crate::runtime_types::PHASE1_OUTER_GAP) separately; core applies the inset once.
//! The synthetic skeleton carries zero outer gap and raw [`PHASE1_BOUNDS`].

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::gesture::{GestureIngress, GestureSettled};
use tiler_core::session::DomainKey;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{
    FocusFeedback, FrameAttempt, FramePurpose, FrameSetter, HoldReason, IntentRecord,
    MemberAttempt, NativeCreationAttempt, OperationEvent, SwitchEvent, ViewAttempt, WorldCommand,
    WorldReplyV4, WorldReportV4,
};

use crate::focus_route::FocusRoute;
use crate::runtime::{NativeContext, ObservedWindow, parse_window_id, window_id};
use crate::runtime_core::{LifetimeMemory, MinimizedRecord, RetainedCore, clear_focus_into};
use crate::runtime_types::{
    AxCompletion, NativeDomain, PHASE1_BOUNDS, PHASE1_GENERATION, PHASE1_OUTPUT, PHASE1_WORKSPACE,
    ScopedBackendSnapshot, SelectPhysical,
};
use crate::world_builder::{ReportInput, build_report};
use crate::world_sequencer::{ObservationClock, WorldSequencer};

/// Fresh identity pair for one engine lifetime, minted from the OS CSPRNG
/// via [`crate::probe_target::mint_nonce`] (one 128-bit nonce per session
/// construction, never a PID/sequence counter: PID reuse across process
/// restarts could otherwise reproduce the pair). Stable within the session;
/// a fresh engine (restart) mints a new pair, so the core never confuses
/// reports across restarts, even in the same process under PID reuse.
/// Returns `(adapter_incarnation, core_generation_seed)`.
///
/// - incarnation `macos-<nonce>`: valid opaque id (`[A-Za-z0-9-_.]`,
///   bounded), so the genuine handshake accepts it as the adapter binding.
/// - seed `phase-1-<nonce>`: lowercase/digits/dashes within the generation
///   bound, so the genuine handshake mints exactly this seed as the bound
///   generation on first bind. The pair fences old traffic even on paths
///   that check only one half.
///
/// `None` when entropy is unavailable: the caller builds a fail-closed
/// unbound session that never binds, never reports, never actuates.
fn session_tokens() -> Option<(String, String)> {
    let nonce = crate::probe_target::mint_nonce().ok()?;
    Some((
        format!("macos-{nonce}"),
        format!("{PHASE1_GENERATION}-{nonce}"),
    ))
}

/// Sentinel tokens for an entropy-failure session: valid-shaped but never
/// bound (no handshake runs, `connection_bound` stays false, and the
/// dispatch route refuses before building any report). The engine half is a
/// pristine unbound `WorldEngine` (no connection owner), so even a directly
/// built report is rejected owner-side: nothing admits, nothing actuates.
const UNBOUND_INCARNATION: &str = "unbound";
const UNBOUND_GENERATION: &str = "unbound";

/// Bound on retained frame-write correlation entries and spent placement
/// tokens (oldest evicted past the bound): both correlate one-off explicit
/// writes, never plan truth, so a bounded window of recent history is
/// enough to settle completions and absorb reply duplicates.
pub(crate) const FRAME_CORRELATION_CAP: usize = 4096;

/// Merge the core's complete, key-ordered projection. Departed keys disappear
/// without a verdict. Inert redelivery allocates nothing; on change, move
/// unchanged rows and copy only rows whose contents changed.
fn apply_settlement_projection<T: Eq + Clone>(
    current: &mut Vec<T>,
    incoming: &[T],
    compare_key: impl Fn(&T, &T) -> std::cmp::Ordering,
    mut receipt: impl FnMut(&T),
) {
    if current.as_slice() == incoming {
        return;
    }
    let mut previous = std::mem::replace(current, Vec::with_capacity(incoming.len()))
        .into_iter()
        .peekable();
    for row in incoming {
        while previous
            .peek()
            .is_some_and(|old| compare_key(old, row).is_lt())
        {
            previous.next();
        }
        if previous
            .peek()
            .is_some_and(|old| compare_key(old, row).is_eq())
        {
            let old = previous.next().expect("matching row");
            if &old == row {
                current.push(old);
                continue;
            }
        }
        receipt(row);
        current.push(row.clone());
    }
}

/// One applied retained gesture terminal: the core-incorporated gesture
/// identity and its committed verdict, verbatim from `gesture_settled`.
/// Generation-scoped; a fresh generation never inherits the previous
/// terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GestureSettlement {
    pub id: u64,
    pub committed: bool,
}

/// One pointer sample stored on the session: the pointer point plus the
/// window attributed to it AT SAMPLE TIME.
///
/// Attribution happens once, on the engine thread, when the sample job is
/// processed — never at build time. A resting pointer therefore keeps its
/// hover memory across reports (and after the pointer later moves or the
/// window closes, the stored sample still names what was hovered). The
/// unattributed raw owner is deliberately not kept: a later homing can
/// never resurrect a foreign/chrome hit, and the builder only withholds a
/// hit whose window no longer appears in the report it is shaping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CursorSample {
    /// Observer sequence (monotonic per observer). Retries repeat the same
    /// sequence; the engine acks per sequence, never a boolean.
    pub seq: u64,
    /// Pointer point in integer CG top-left points.
    pub point: (i32, i32),
    /// Homed member window under the pointer, or `None` for chrome,
    /// foreign apps, and unreadable hits.
    pub hit: Option<WindowId>,
}

/// Adapter-owned deferred reveal for one explicit float placement that was
/// withheld while the lifetime was natively fullscreen. Shared core keeps
/// ordinary float rows `Untouched` (and the placement token unspent), so
/// the adapter must remember the durable target itself and reveal it once
/// after the overlay clears — never while fullscreen (D5: no writes to a
/// fullscreen subject). The operation token is the core one-shot placement
/// token; `target` is always the core-published durable frame, never the
/// fullscreen pose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingFullscreenFloatPlacement {
    pub(crate) operation: String,
    pub(crate) target: Rect,
    pub(crate) generation: u64,
}

/// Engine-thread-owned world session: the retained [`WorldEngine`], the
/// adapter-side [`WorldSequencer`], pending attempt feedback, and the
/// completion-correlation maps. None of this is shared: submitters reach
/// the engine only through inbox jobs.
pub(crate) struct WorldSession {
    engine: WorldEngine,
    sequencer: WorldSequencer,
    incarnation: String,
    native_mode: bool,
    focus_route: FocusRoute,
    /// Staged raw view attempts drained into the next dispatched report,
    /// latest-wins per exact `(operation, output)` pair. Drained (cleared)
    /// on every dispatch whether or not the report is accepted — attempts
    /// describe attempted actuation, never retry themselves. A sibling
    /// output's row can never stand in for another output's flight.
    pub(crate) pending_views: BTreeMap<(String, OutputId), ViewAttempt>,
    /// Staged raw membership attempts, latest-wins per (operation, window).
    pub(crate) pending_members: BTreeMap<(String, String), MemberAttempt>,
    /// Staged raw native creation attempts, latest-wins per creation
    /// operation id. Drained (cleared) on every dispatch whether or not
    /// the report is accepted — attempts describe attempted actuation,
    /// never retry themselves.
    pub(crate) pending_creations: BTreeMap<String, NativeCreationAttempt>,
    /// Staged raw frame attempts for explicit float placements only,
    /// latest-wins per window. Ordinary layout traffic stages into the
    /// separate [`Self::pending_layouts`] map so a layout completion can
    /// never clobber a one-shot placement row for the same window before
    /// the next report drains (the placement row settles a spent core
    /// token; the layout row is re-derivable policy feedback).
    pub(crate) pending_frames: BTreeMap<String, FrameAttempt>,
    /// Staged raw layout attempts for ordinary (tokenless `Controlled`)
    /// frame writes, latest-wins per window, bounded oldest-evicted.
    /// Separate from [`Self::pending_frames`] by design (see above); the
    /// drain merges placement rows first, so one window never carries two
    /// frame rows on the wire (`duplicate-frame`).
    pending_layouts: BTreeMap<String, FrameAttempt>,
    /// Frame-write correlation for explicit float placements:
    /// `(pid, cmd_id)` to the emitted window, its attempted frame, and its
    /// placement token. Bounded (oldest evicted).
    pending_writes: BTreeMap<(u32, u64), (String, Rect, Option<String>)>,
    /// Frame-write correlation for ordinary layout writes: `(pid, cmd_id)`
    /// to the emitted window and its attempted frame, bounded (oldest
    /// evicted). Never shares space with the placement correlation.
    pub(crate) pending_layout_writes: BTreeMap<(u32, u64), (String, Rect)>,
    /// Explicit float-placement tokens already written once (exactly-once
    /// geometry under `frame_operation`). Bounded oldest-evicted like the
    /// write correlation, and generation-scoped: core command sequences
    /// restart with a fresh generation, so a dead generation's spend must
    /// never pre-refuse a new token that reuses the same string.
    spent_frame_ops: VecDeque<String>,
    /// Adapter-owned deferred reveals for explicit float placements
    /// withheld while the lifetime was natively fullscreen, keyed by
    /// runtime window id. Ordinary float rows arrive `Untouched` from
    /// shared core, so the adapter remembers the durable target and writes
    /// it exactly once after the overlay clears (never while fullscreen).
    pub(crate) pending_fullscreen_float_placements:
        BTreeMap<String, PendingFullscreenFloatPlacement>,
    /// Owner generations of exact detached facts in the current command
    /// report. Never a source of authority for a later command or rebind.
    detached_select_members: Option<(u64, BTreeMap<WindowId, u64>)>,
    /// One bounded staged close-focus feedback row: the first correlated
    /// raw row since the previous dispatch. First-wins (later rows drop
    /// until drained); drained once into the next dispatched report with
    /// its original stamp, never fabricated.
    pending_close_feedback: Option<FocusFeedback>,
    /// True once an observation was refused (stale revision, no context):
    /// retained truth missed input, so the next report publishes
    /// `inventory_complete = false` until a fresh observation lands.
    coverage_gap: bool,
    /// Pending explicit closes (flat ids) not yet incorporated by the core.
    /// Staged on covering roster prune and terminate; drained only on an
    /// applied or duplicate reply, never on NoReport or refusal.
    pub(crate) pending_closes: BTreeSet<WindowId>,
    /// Latest pointer sample (sample-time attributed hit). Persists across
    /// dispatches: a build never clears it, so a resting pointer keeps its
    /// hover memory and every report re-pairs the same sample until the
    /// observer posts a newer one.
    cursor_sample: Option<CursorSample>,
    /// Startup-resolved presentation style numbers for the report ingress.
    /// Synced from shared state on every dispatch (the engine thread owns
    /// this copy; the main thread publishes it once at startup).
    presentation_style: crate::active_border::PresentationStylePx,
    /// True once the one-time connection handshake minted a generation and
    /// bound the sequencer to it. Reports gate on this; the handshake never
    /// repeats on a bound session.
    connection_bound: bool,
    /// Accepted core reply for the initial revisioned settings exchange
    /// (status plus per-field diagnostics). `None` when the handshake
    /// refused and no settings rode. Invalid supplied values diagnose per
    /// field here with last-good retained core-side while valid siblings
    /// still apply; omitted keys diagnose `Missing` with core defaults.
    initial_settings: Option<tiler_core::connection::SettingsReply>,
    /// Adapter-owned durable intent store for this login namespace. `None`
    /// when `$HOME`/the audit-session namespace is unavailable (fail closed:
    /// a save then holds until a durable backend exists, always retried on a
    /// later accepted reply, never latched permanently).
    intent_store: Option<crate::intent_store::IntentStore>,
    /// Live-lifetime binding probe for the intent store. Production reads
    /// the OS; tests supply deterministic lifetimes without spawning apps.
    intent_identity: Option<crate::intent_store::IdentityProbe>,
    /// Process-liveness probe for the intent store. Production reads the
    /// OS; tests supply deterministic liveness without spawning apps.
    /// `None` means [`crate::intent_store::live_process`].
    intent_process: Option<crate::intent_store::ProcessProbe>,
    /// Windows whose stored row was already imported into a report, each
    /// imported at most once: a window admitted in one report never rides a
    /// later report, while a window first seen later still hydrates.
    intent_admitted: BTreeSet<String>,
    /// Windows already diagnosed as live-but-unverifiable for persistence,
    /// so the recovery path emits one bounded line per window, never one per
    /// accepted reply.
    intent_unverifiable: BTreeSet<String>,
    /// Applied retained switch terminals, latest per owning output in the
    /// projection's stable output order (see
    /// [`WorldSession::apply_switch_settlements`]). Adapter-visible
    /// completion receipts only: no claim, frame, or focus decision reads
    /// this, and an absent owner is never a verdict (its row is dropped
    /// with the owner, so retention is bounded by the projection).
    pub(crate) switch_settlements: Vec<SwitchEvent>,
    /// Applied retained operation terminals, latest per `(window, kind)` in
    /// the projection's stable key order (see
    /// [`WorldSession::apply_operation_settlements`]). Completion receipts
    /// only: no actuator reads this, and a key absent from the complete
    /// projection is dropped, never a verdict.
    operation_settlements: Vec<OperationEvent>,
    /// Latest applied retained gesture terminal for the bound generation
    /// (see [`WorldSession::apply_gesture_settlement`]). One nullable slot
    /// per generation, never a take-once queue; a consumed/stale replay
    /// cannot regress its identity or change its original verdict.
    gesture_settlement: Option<GestureSettlement>,
    /// Generation the retained settlement projections were applied under.
    /// A rebind (fresh handshake/new generation) clears every retained
    /// outcome and the retained gesture terminal; same-generation replays
    /// preserve them.
    settlements_generation: String,
}

impl WorldSession {
    pub(crate) fn new(
        native_mode: bool,
        route: FocusRoute,
        clock: Arc<ObservationClock>,
        initial_settings: &[(String, tiler_core::connection::RawSettingValue)],
    ) -> Self {
        Self::new_with_tokens(
            session_tokens(),
            native_mode,
            route,
            clock,
            initial_settings,
        )
    }

    /// Session construction over an explicit token source: `Some` pair runs
    /// the genuine one-time handshake and binds the sequencer; `None`
    /// (CSPRNG failure) builds the fail-closed unbound session (no
    /// handshake, no bind, no reports, no actuation — see
    /// [`UNBOUND_INCARNATION`]). `WorldSession::new` supplies the live
    /// [`session_tokens`]; tests supply fixed pairs (deterministic distinct
    /// sessions) or `None` (entropy-failure shape) without touching global
    /// entropy state.
    pub(crate) fn new_with_tokens(
        tokens: Option<(String, String)>,
        native_mode: bool,
        route: FocusRoute,
        clock: Arc<ObservationClock>,
        initial_settings: &[(String, tiler_core::connection::RawSettingValue)],
    ) -> Self {
        let Some((incarnation, seed)) = tokens else {
            // Entropy unavailable: pristine unbound engine (no connection
            // owner, so `handle_report` rejects everything) plus sentinel
            // sequencer binding and `connection_bound = false` (so the
            // dispatch route refuses before building any report). No
            // handshake runs, so the sentinel pair can never bind.
            let engine =
                WorldEngine::new(PHASE1_GENERATION).expect("phase-1 generation token is valid");
            let sequencer =
                WorldSequencer::with_clock(UNBOUND_INCARNATION, UNBOUND_GENERATION, clock);
            return Self {
                engine,
                sequencer,
                incarnation: UNBOUND_INCARNATION.to_owned(),
                native_mode,
                focus_route: route,
                pending_views: BTreeMap::new(),
                pending_members: BTreeMap::new(),
                pending_creations: BTreeMap::new(),
                pending_frames: BTreeMap::new(),
                pending_layouts: BTreeMap::new(),
                pending_writes: BTreeMap::new(),
                pending_layout_writes: BTreeMap::new(),
                spent_frame_ops: VecDeque::new(),
                pending_fullscreen_float_placements: BTreeMap::new(),
                detached_select_members: None,
                pending_close_feedback: None,
                coverage_gap: false,
                pending_closes: BTreeSet::new(),
                cursor_sample: None,
                presentation_style: crate::active_border::PresentationStylePx::default(),
                connection_bound: false,
                initial_settings: None,
                intent_store: None,
                intent_identity: None,
                intent_process: None,
                intent_admitted: BTreeSet::new(),
                intent_unverifiable: BTreeSet::new(),
                switch_settlements: Vec::new(),
                operation_settlements: Vec::new(),
                gesture_settlement: None,
                settlements_generation: UNBOUND_GENERATION.to_owned(),
            };
        };
        // `seed` mints the bound generation below, so the constructor always
        // holds a valid generation token; the genuine one-time handshake
        // through the protocol connection route binds the sequencer to it
        // (first connection re-mints the seed). The maximized-focus fence
        // the constructor enables stays inert because the builder reports
        // `maximized` as null unconditionally (no native inference). The
        // watermark clock is shared with every invalidating input
        // submitter (see `Shared::clock`): only submitted input moves it,
        // never a dispatch, so tickets stamp actually-submitted input and
        // mid-flight arrivals refuse as stale.
        let mut engine = WorldEngine::new(&seed).expect("session generation seed is valid");
        let (bound_generation, connection_bound, initial_settings) =
            match crate::world_connection::connect(&mut engine, &incarnation, initial_settings) {
                Some(outcome) => (outcome.generation, true, Some(outcome.settings)),
                None => (seed, false, None),
            };
        let sequencer = WorldSequencer::with_clock(&incarnation, &bound_generation, clock);
        Self {
            engine,
            sequencer,
            incarnation,
            native_mode,
            focus_route: route,
            pending_views: BTreeMap::new(),
            pending_members: BTreeMap::new(),
            pending_creations: BTreeMap::new(),
            pending_frames: BTreeMap::new(),
            pending_layouts: BTreeMap::new(),
            pending_writes: BTreeMap::new(),
            pending_layout_writes: BTreeMap::new(),
            spent_frame_ops: VecDeque::new(),
            pending_fullscreen_float_placements: BTreeMap::new(),
            detached_select_members: None,
            pending_close_feedback: None,
            coverage_gap: false,
            pending_closes: BTreeSet::new(),
            cursor_sample: None,
            presentation_style: crate::active_border::PresentationStylePx::default(),
            connection_bound,
            initial_settings,
            intent_store: None,
            intent_identity: None,
            intent_process: None,
            intent_admitted: BTreeSet::new(),
            intent_unverifiable: BTreeSet::new(),
            switch_settlements: Vec::new(),
            operation_settlements: Vec::new(),
            gesture_settlement: None,
            settlements_generation: bound_generation,
        }
    }

    pub(crate) fn pin_detached_select_member(
        &self,
        command_seq: u64,
        window: &WindowId,
    ) -> Option<crate::runtime_types::DetachedSelectMember> {
        let (report_command, members) = self.detached_select_members.as_ref()?;
        if *report_command != command_seq {
            return None;
        }
        Some(crate::runtime_types::DetachedSelectMember {
            window: window.clone(),
            generation: *members.get(window)?,
        })
    }

    /// True once the one-time handshake minted a generation and bound the
    /// sequencer to it. Reports gate on this; the handshake never repeats
    /// on a bound session.
    pub(crate) fn connection_bound(&self) -> bool {
        self.connection_bound
    }

    /// Accepted core reply for the initial revisioned settings exchange, if
    /// the handshake bound. Per-field diagnostics live here: invalid
    /// supplied values diagnose with last-good retained while valid
    /// siblings still apply; omitted keys diagnose `Missing` on defaults.
    pub(crate) fn initial_settings_reply(&self) -> Option<&tiler_core::connection::SettingsReply> {
        self.initial_settings.as_ref()
    }

    /// Attach the adapter-owned durable intent store.
    #[cfg(test)]
    pub(crate) fn attach_intent_store(&mut self, store: crate::intent_store::IntentStore) {
        self.intent_store = Some(store);
    }

    /// Resolve and attach the configured durable intent store, returning
    /// its directory. `None` when the mode is disabled or the store cannot
    /// be opened (fail closed: no store, no persistence). The engine owner
    /// calls this once at startup; it never opens a store implicitly.
    pub(crate) fn open_intent_store(
        &mut self,
        mode: &crate::runtime_types::IntentStoreMode,
    ) -> Option<PathBuf> {
        let store = mode.open()?;
        let dir = store.dir().to_path_buf();
        self.intent_store = Some(store);
        Some(dir)
    }

    /// Override the live-lifetime binding probe (tests only; production
    /// reads the OS through [`crate::intent_store::live_binding`]).
    #[cfg(test)]
    pub(crate) fn set_intent_identity(&mut self, probe: crate::intent_store::IdentityProbe) {
        self.intent_identity = Some(probe);
    }

    /// Override the process-liveness probe (tests only; production reads
    /// the OS through [`crate::intent_store::live_process`]).
    #[cfg(test)]
    pub(crate) fn set_intent_process_liveness(&mut self, probe: crate::intent_store::ProcessProbe) {
        self.intent_process = Some(probe);
    }

    #[cfg(test)]
    pub(crate) fn intent_unverifiable_windows(&self) -> &BTreeSet<String> {
        &self.intent_unverifiable
    }

    #[cfg(test)]
    pub(crate) fn intent_store_attached(&self) -> bool {
        self.intent_store.is_some()
    }

    fn live_intent_binding(&self, pid: u32) -> Option<crate::intent_store::LiveBinding> {
        match &self.intent_identity {
            Some(probe) => probe(pid),
            None => crate::intent_store::live_binding(pid),
        }
    }

    fn process_alive(&self, pid: u32) -> bool {
        match &self.intent_process {
            Some(probe) => probe(pid),
            None => crate::intent_store::live_process(pid),
        }
    }

    /// Per-window hydration: stored rows whose lifetime binding still
    /// matches the live process AND whose window appears in this report AND
    /// that were never shipped in an earlier report. Each window's row is
    /// imported at most once (the first applied reply for that window
    /// initializes core desire); a window first observed later still
    /// hydrates, while an already-imported window never rides again and
    /// never overwrites runtime intent.
    pub(crate) fn intent_records_for_admission(&self, present: &[&str]) -> Vec<IntentRecord> {
        let Some(store) = self.intent_store.as_ref() else {
            return Vec::new();
        };
        let mut records = Vec::new();
        for entry in store.entries().values() {
            if self.intent_admitted.contains(&entry.win) {
                continue;
            }
            if !present.iter().any(|win| *win == entry.win) {
                continue;
            }
            let Some((pid, wid)) = parse_window_id(&entry.win) else {
                continue;
            };
            let live = self.live_intent_binding(pid);
            if !entry.matches_live(pid, wid, live.as_ref()) {
                continue;
            }
            records.push(IntentRecord {
                win: WindowId(entry.win.clone()),
                data: entry.data.clone(),
            });
        }
        records
    }

    /// Record the stored records actually shipped with an applied report.
    /// Only those windows are imported; a stored row that never rode (for
    /// example, its window was absent) keeps its chance to hydrate later.
    pub(crate) fn mark_intent_admitted(&mut self, records: &[IntentRecord]) {
        for record in records {
            self.intent_admitted.insert(record.win.0.clone());
        }
    }

    /// Adapter-local desired-versus-durable reconciliation.
    ///
    /// Applied to every accepted/consumed reply before any dependent native
    /// write. Core publishes one authoritative current target per window;
    /// rows absent from `intent_desired` carry no authority and never delete
    /// a persisted row. `Some(data)` persists the canonical encoding,
    /// `None` clears the entry (while remembering the clear for equality),
    /// and an equal verified binding+value costs zero writes. A failed
    /// write/verification holds the whole reply; the next accepted reply
    /// retries the difference (no timer, no permanent session latch). A
    /// target whose process is alive but whose lifetime binding is
    /// unreadable is skipped (never persisted, diagnosed once per window;
    /// other rows still reconcile and the reply is not held), while a gone
    /// process keeps the recoverable whole-reply hold.
    /// Returns `true` when the reply must be held.
    pub(crate) fn reconcile_intent(&mut self, reply: &WorldReplyV4) -> bool {
        if reply.intent_desired.is_empty() {
            return false;
        }
        let mut targets: BTreeMap<String, Option<String>> = BTreeMap::new();
        for row in &reply.intent_desired {
            let win = row.win.0.clone();
            if canonical_intent_identity(&win).is_none() {
                return true;
            }
            let data = row
                .data
                .as_ref()
                .map(tiler_core::intent_data::encode_intent_data);
            if let Some(previous) = targets.insert(win, data.clone())
                && previous != data
            {
                // Conflicting duplicate rows for one lifetime: no honest
                // single target, so never guess.
                return true;
            }
        }
        if self.intent_store.is_none() {
            // No durable backend: a save cannot be persisted. An explicit
            // clear is satisfied by the absence of any adapter-owned entry.
            return targets.values().any(Option::is_some);
        }
        let mut identities: BTreeMap<u32, (Option<crate::intent_store::LiveBinding>, bool)> =
            BTreeMap::new();
        for win in targets.keys() {
            if let Some((pid, _)) = canonical_intent_identity(win) {
                identities
                    .entry(pid)
                    .or_insert_with(|| (self.live_intent_binding(pid), self.process_alive(pid)));
            }
        }
        let mut changes: BTreeMap<String, Option<crate::intent_store::StoredEntry>> =
            BTreeMap::new();
        let mut unverifiable: Vec<String> = Vec::new();
        {
            let store = self.intent_store.as_ref().expect("checked above");
            for (win, target) in targets {
                let (pid, wid) =
                    canonical_intent_identity(&win).expect("targets validated canonical ids");
                let (live, alive) = identities
                    .get(&pid)
                    .cloned()
                    .expect("identity sampled for every target pid");
                let existing = store.entries().get(&win).cloned();
                match target {
                    Some(data) => {
                        let unchanged = existing.as_ref().is_some_and(|entry| {
                            entry.data == data && entry.matches_live(pid, wid, live.as_ref())
                        });
                        if unchanged {
                            continue;
                        }
                        let Some(live) = live else {
                            if alive {
                                // Live process without a durable lifetime
                                // identity: never persist a guessed binding.
                                // Skip this row (no persistence claim) and
                                // diagnose once per window below; other rows
                                // still reconcile and the reply is not held.
                                unverifiable.push(win);
                            } else {
                                // Gone process: unverifiable for now, but the
                                // lifetime may become readable once the app
                                // is back. Hold the whole reply and retry on a
                                // later accepted reply (no timer, no latch).
                                return true;
                            }
                            continue;
                        };
                        changes.insert(
                            win.clone(),
                            Some(crate::intent_store::StoredEntry {
                                win,
                                wid,
                                pid,
                                launch_date_ns: live.launch_date_ns,
                                bundle_id: live.bundle_id,
                                data,
                            }),
                        );
                    }
                    None => {
                        let known_absent = match store.durable().get(&win) {
                            Some(None) => true,
                            _ => !store.entries().contains_key(&win),
                        };
                        if known_absent {
                            continue;
                        }
                        changes.insert(win, None);
                    }
                }
            }
        }
        for win in unverifiable {
            if self.intent_unverifiable.insert(win.clone()) {
                // Static, bounded wording: one pid per line, never the
                // native window id, and only the first occurrence per
                // window speaks.
                eprintln!("{}", intent_unverifiable_diagnostic(&win));
            }
        }
        if changes.is_empty() {
            return false;
        }
        self.intent_store
            .as_mut()
            .expect("checked above")
            .apply_changes(changes)
            .is_err()
    }

    /// The applied projections, gesture terminal, and one-off
    /// explicit-write memory are generation-scoped. Core command sequences
    /// restart with a fresh generation, so a stale spend would pre-refuse a
    /// NEW placement token that reuses the same string (zero attempts for a
    /// published token), and a deferred fullscreen reveal still carries a
    /// dead generation's durable target. A rebind withdraws both.
    pub(crate) fn sync_settlement_generation(&mut self, bound: &str) -> bool {
        if self.settlements_generation == bound {
            return false;
        }
        self.settlements_generation = bound.to_owned();
        self.gesture_settlement = None;
        self.operation_settlements.clear();
        self.switch_settlements.clear();
        self.spent_frame_ops.clear();
        self.pending_fullscreen_float_placements.clear();
        true
    }

    /// Apply one applied reply's `gesture_settled` slot.
    ///
    /// The slot is one nullable retained value per `(adapter, gen)`, not a
    /// take-once queue: it re-publishes on later applied replies, so a
    /// terminal whose issuing reply was skipped still lands. Only a newly
    /// incorporated terminal replaces it; a same-id replay can neither
    /// regress a newer identity nor change the original verdict. Returns
    /// only a genuinely applied terminal, for the receipt/publication.
    pub(crate) fn apply_gesture_settlement(
        &mut self,
        settled: Option<&GestureSettled>,
    ) -> Option<GestureSettlement> {
        let settled = settled?;
        if self
            .gesture_settlement
            .is_some_and(|current| current.id == settled.id)
        {
            // Same identity re-published: exact replay is inert, including
            // after the core consumed the id and even if a (corrupt)
            // verdict differs.
            return None;
        }
        let settlement = GestureSettlement {
            id: settled.id,
            committed: settled.committed,
        };
        self.gesture_settlement = Some(settlement);
        Some(settlement)
    }

    /// Apply changed operation terminals, keyed by window lifetime and phase.
    pub(crate) fn apply_operation_settlements(
        &mut self,
        events: &[OperationEvent],
        receipt: impl FnMut(&OperationEvent),
    ) {
        apply_settlement_projection(
            &mut self.operation_settlements,
            events,
            |left, right| (&left.window, left.kind).cmp(&(&right.window, right.kind)),
            receipt,
        );
    }

    /// Apply changed switch terminals, keyed by their owning output.
    pub(crate) fn apply_switch_settlements(
        &mut self,
        events: &[SwitchEvent],
        receipt: impl FnMut(&SwitchEvent),
    ) {
        apply_settlement_projection(
            &mut self.switch_settlements,
            events,
            |left, right| left.output.cmp(&right.output),
            receipt,
        );
    }

    /// Adapter incarnation bound into every report.
    pub(crate) fn incarnation(&self) -> &str {
        &self.incarnation
    }

    /// Effective core-owned border numbers as the report presentation style.
    /// Reads the engine's effective settings (post-handshake, defaults for
    /// omitted keys); unreachable type shapes fall back to zeroed numbers
    /// with the switch on (effective state always carries validated types).
    pub(crate) fn core_border_presentation(&self) -> crate::active_border::PresentationStylePx {
        use tiler_core::settings_catalog as catalog;
        use tiler_core::settings_state::EffectiveSetting as Eff;
        let effective = &self.engine.settings().effective;
        let enabled = match effective.get(catalog::FIELD_BORDER_ENABLED) {
            Some(Eff::Bool(b)) => *b,
            _ => true,
        };
        let width = match effective.get(catalog::FIELD_BORDER_WIDTH) {
            Some(Eff::Float(v)) => *v,
            _ => 0.0,
        };
        let gap = match effective.get(catalog::FIELD_BORDER_GAP) {
            Some(Eff::Float(v)) => *v,
            _ => 0.0,
        };
        let width_px = width.round() as i32;
        crate::active_border::PresentationStylePx {
            border_enabled: enabled,
            gap_px: gap.round() as i32,
            width_px,
            extension_px: width_px,
        }
    }

    /// Sync the startup-resolved presentation style numbers from shared
    /// state. Called on every dispatch before the report builds, so reports
    /// carry the configured outset even though the engine thread never
    /// reads startup config directly.
    pub(crate) fn sync_presentation_style(
        &mut self,
        style: crate::active_border::PresentationStylePx,
    ) {
        self.presentation_style = style;
    }

    /// Stage one raw view-attempt row for the next dispatched report.
    /// Resolves the retained transfer target by the exact `(operation,
    /// output)` pair (logical target correlation only, never adapter
    /// mapping); a missing output or absent exact View transfer drops the
    /// row. Typed physical reads keep their Desktop/Detached arm and are
    /// converted through the target output only
    /// ([`SelectPhysical::to_view`]); no bare SID is ever re-spelled into a
    /// native workspace, so a detached surface cannot masquerade as a
    /// Desktop row. The baseline is the observation-time physical current
    /// captured at dispatch (the plan's baseline); pre/post are the real
    /// write-time reads around the drain actuation, and `post_member` is
    /// the adapter-bracketed membership read for an accepted exact
    /// detached post. Unknown operations drop without evidence (never
    /// forged, never guessed). Latest-wins per exact `(operation, output)`
    /// pair.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stage_view_attempt(
        &mut self,
        operation_id: &str,
        target_output: Option<&OutputId>,
        disposition: FrameSetter,
        pre: Option<SelectPhysical>,
        post: Option<SelectPhysical>,
        baseline: Option<SelectPhysical>,
        post_member: Option<WindowId>,
    ) {
        let Some(target_output) = target_output else {
            return;
        };
        let transfer = self
            .engine
            .retained_transfers()
            .into_iter()
            .find(|target| {
                target.operation_id == operation_id
                    && target.target_output == *target_output
                    && matches!(target.kind, tiler_core::world_report::TransferKind::View)
            });
        let Some(transfer) = transfer else {
            return;
        };
        // The bound capability owns whether a detached surface is a reportable
        // view at all: with `detachedFullscreenView` off, a typed detached
        // pre/post/baseline is withheld instead of being published as a
        // detached View row, and a detached post never carries a member.
        // Desktop reads keep their exact native rows either way.
        let detached_view = self.detached_fullscreen_view();
        let to_view = |physical: SelectPhysical| {
            (detached_view || !matches!(physical, SelectPhysical::Detached(_)))
                .then(|| physical.to_view(&transfer.target_output))
        };
        let attempt = ViewAttempt {
            operation_id: operation_id.to_owned(),
            target_output: transfer.target_output.clone(),
            target_workspace: transfer.target_workspace.clone(),
            disposition,
            pre: pre.and_then(to_view),
            post: post.and_then(to_view),
            baseline: baseline.and_then(to_view),
            attempt_ms: 0,
            post_member: if detached_view { post_member } else { None },
        };
        self.pending_views
            .insert((operation_id.to_owned(), transfer.target_output), attempt);
    }

    /// Stage one raw membership-attempt row for the next dispatched report.
    /// Same target-correlation/drop rules as
    /// [`WorldSession::stage_view_attempt`]; the retained transfer must be
    /// a window transfer (its subject window and target ride the staged
    /// row — the operation identity already names them core-side, so the
    /// drain never re-states them). `native_pre` is the genuine pre-write
    /// homing read around the drain actuation (refusal proof needs it
    /// unchanged and readable); `native_post` is the drain-verified
    /// arrival Space (`None` unless the drain proved arrival there).
    /// Latest-wins per (operation, window).
    pub(crate) fn stage_member_attempt(
        &mut self,
        operation_id: &str,
        setter: FrameSetter,
        pre_sid: Option<u64>,
        post_sid: Option<u64>,
    ) {
        let transfer = self
            .engine
            .retained_transfers()
            .into_iter()
            .find(|target| target.operation_id == operation_id);
        let Some(transfer) = transfer else {
            return;
        };
        let tiler_core::world_report::TransferKind::Window(window) = transfer.kind.clone() else {
            return;
        };
        let target_output = transfer.target_output.clone();
        let target_workspace = transfer.target_workspace.clone();
        let to_native = |sid: u64| tiler_core::world_report::WorkspaceRef::Native {
            output: target_output.clone(),
            workspace: tiler_core::ids::NativeWorkspaceId::parse(&crate::topology::workspace_key(
                sid,
            ))
            .expect("valid native workspace"),
        };
        let attempt = MemberAttempt {
            operation_id: operation_id.to_owned(),
            window: window.clone(),
            target_output: target_output.clone(),
            target_workspace,
            disposition: setter.into(),
            pre: pre_sid.map(to_native),
            post: post_sid.map(to_native),
            attempt_ms: 0,
        };
        self.pending_members
            .insert((operation_id.to_owned(), window.0.clone()), attempt);
    }

    /// Stage one raw native creation attempt for the next dispatched
    /// report. Exact correlation on operation id plus logical plus output
    /// (the core consumes only the first exact-correlated attempt per op;
    /// changed replays latch ambiguity). `order_before`/`order_after` are
    /// the genuine native orders around the drain's create leg (mapped to
    /// workspace keys here; logical ids are never minted adapter-side):
    /// `order_after` is a genuine full post-order, including a genuinely
    /// observed unchanged order; unreadable post-orders stay local. The
    /// `created_native` is always `None`: an observed appended order alone
    /// is not causal proof of what the create leg made (M5), so the
    /// production v4 attempt never claims a created identity even when the
    /// legacy drain exposes an appended candidate. The codec keeps
    /// `created` explicit null on this row. The drain reports
    /// exactly one attempt per creation desire. Latest-wins per op.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stage_native_creation(
        &mut self,
        operation_id: &str,
        logical_workspace: &str,
        target_output: OutputId,
        setter: FrameSetter,
        order_before: &[u64],
        order_after: &[u64],
        created_sid: Option<u64>,
        attempt_ms: u64,
    ) {
        if operation_id.is_empty() || logical_workspace.is_empty() {
            return;
        }
        // M5: no native addition-count proof is accepted as causal proof.
        // Discard any appended-SID candidate from the legacy drain; the
        // genuine orders/setter/identity below still ride verbatim.
        let _ = created_sid;
        let attempt = NativeCreationAttempt {
            operation_id: operation_id.to_owned(),
            logical_workspace: logical_workspace.to_owned(),
            target_output,
            setter,
            order_before: order_before
                .iter()
                .map(|sid| crate::topology::workspace_key(*sid))
                .collect(),
            order_after: order_after
                .iter()
                .map(|sid| crate::topology::workspace_key(*sid))
                .collect(),
            created_native: None,
            attempt_ms,
        };
        self.pending_creations
            .insert(operation_id.to_owned(), attempt);
    }

    /// Mark retained truth as having missed input (refused observation):
    /// the next report publishes incomplete inventory.
    pub(crate) fn mark_coverage_gap(&mut self) {
        self.coverage_gap = true;
    }

    /// Fresh observation landed: coverage is proven again by the caller.
    pub(crate) fn clear_coverage_gap(&mut self) {
        self.coverage_gap = false;
    }

    /// Read the current coverage gap for diagnostics (no mutation).
    pub(crate) fn coverage_gap(&self) -> bool {
        self.coverage_gap
    }

    /// Store one sample-time attributed pointer sample. Returns whether the
    /// sample was accepted: a repeated or older observer sequence is ignored
    /// (the observer retries one pending sample until the engine acks it, so
    /// a duplicate delivery carries no new truth), while the newest sample
    /// always replaces the stored one. The sample is never cleared by a
    /// build: reports re-pair the stored sample, so a resting pointer keeps
    /// its hover memory.
    pub(crate) fn note_cursor_sample(&mut self, sample: CursorSample) -> bool {
        if self
            .cursor_sample
            .as_ref()
            .is_some_and(|stored| stored.seq >= sample.seq)
        {
            return false;
        }
        self.cursor_sample = Some(sample);
        true
    }

    /// The stored pointer sample, if any (read-only; never consumed).
    pub(crate) fn cursor_sample(&self) -> Option<&CursorSample> {
        self.cursor_sample.as_ref()
    }

    fn take_view_attempts(&mut self) -> Vec<ViewAttempt> {
        std::mem::take(&mut self.pending_views)
            .into_values()
            .collect()
    }

    fn take_member_attempts(&mut self) -> Vec<MemberAttempt> {
        std::mem::take(&mut self.pending_members)
            .into_values()
            .collect()
    }

    fn take_creation_attempts(&mut self) -> Vec<NativeCreationAttempt> {
        std::mem::take(&mut self.pending_creations)
            .into_values()
            .collect()
    }

    /// Drain staged frame attempts for the next report. Placement rows
    /// (explicit one-shot float operations) win a same-window collision
    /// with a layout row and the displaced layout row is disposed of here:
    /// one window owns exactly one frame authority per report (wire
    /// `duplicate-frame` refuses two rows), a placement row settles a token
    /// the adapter already spent (it can never be re-derived), while a
    /// layout row is advisory anti-fighting feedback the core re-learns from
    /// the next observation of the same window. Both maps drain on every
    /// dispatch whether or not the report is accepted (attempts describe
    /// attempted actuation and never retry themselves).
    fn take_frame_attempts(&mut self) -> Vec<FrameAttempt> {
        let mut frames = std::mem::take(&mut self.pending_frames);
        for (window, attempt) in std::mem::take(&mut self.pending_layouts) {
            frames.entry(window).or_insert(attempt);
        }
        frames.into_values().collect()
    }

    /// Stage one raw close-focus feedback row for the next dispatched
    /// report. Bounded first-wins: only the first correlated raw row since
    /// the previous drain is kept (later rows drop), and the stored row
    /// keeps its original stamp verbatim — never re-stamped, never
    /// fabricated.
    pub(crate) fn stage_close_feedback(&mut self, feedback: FocusFeedback) {
        if self.pending_close_feedback.is_none() {
            self.pending_close_feedback = Some(feedback);
        }
    }

    /// Drain the staged close-focus feedback row, if any. Called on every
    /// dispatch alongside the attempt drains, whether or not the report is
    /// accepted — feedback rows describe attempted actuation, never retry
    /// themselves.
    fn take_close_feedback(&mut self) -> Option<FocusFeedback> {
        self.pending_close_feedback.take()
    }

    /// Test-only peek at the staged close-focus feedback row without
    /// draining (cross-module dispatch tests assert first-wins staging).
    #[cfg(test)]
    pub(crate) fn peek_close_feedback(&self) -> Option<FocusFeedback> {
        self.pending_close_feedback.clone()
    }

    /// Adapter-side core-delivery lifetime memory for wire-flag
    /// normalization: the last applied reply's rows (a live reply lookup,
    /// never a durable host table). Empty before the first applied reply
    /// (pre-adoption first-sight, matching the old empty-session read).
    fn lifetime_memory(&self) -> LifetimeMemory {
        self.sequencer
            .last_reply()
            .as_ref()
            .map(LifetimeMemory::from_reply)
            .unwrap_or_else(LifetimeMemory::empty)
    }

    /// Native mode flag (synthetic skeleton otherwise).
    pub(crate) fn is_native(&self) -> bool {
        self.native_mode
    }

    pub(crate) fn detached_fullscreen_view(&self) -> bool {
        self.native_mode
            && self.engine.capabilities().is_some_and(|cap| {
                cap.detached_fullscreen_view
                    && cap.host_owned
                    && cap.native_selector_scope == tiler_core::connection::NativeSelectorScope::PerOutput
            })
    }

    /// Core engine access for the dispatch half (see
    /// [`crate::world_dispatch`]).
    pub(crate) fn engine_mut(&mut self) -> &mut WorldEngine {
        &mut self.engine
    }

    /// Adapter-side sequencer access for the dispatch half. All sequencing
    /// operations take `&self` (interior mutability), so dispatches never
    /// hold the session mutably for ordering.
    pub(crate) fn sequencer(&self) -> &WorldSequencer {
        &self.sequencer
    }

    /// Focus route for owner-dispatched focus commands.
    pub(crate) fn focus_route(&self) -> FocusRoute {
        self.focus_route
    }

    /// Spend one explicit float-placement token; false when already spent.
    /// Bounded oldest-evicted past [`FRAME_CORRELATION_CAP`], and scoped to
    /// the bound core generation (see
    /// [`WorldSession::sync_settlement_generation`]). Called only at worker
    /// handoff (see [`crate::world_dispatch`]): a token must never burn
    /// before a live slot took the command.
    pub(crate) fn spend_frame_op(&mut self, operation: &str) -> bool {
        if self.frame_op_spent(operation) {
            return false;
        }
        if self.spent_frame_ops.len() >= FRAME_CORRELATION_CAP {
            self.spent_frame_ops.pop_front();
        }
        self.spent_frame_ops.push_back(operation.to_owned());
        true
    }

    /// Whether this explicit placement token is already spent: its command
    /// is queued in a slot, in flight, or was attempted.
    pub(crate) fn frame_op_spent(&self, operation: &str) -> bool {
        self.spent_frame_ops.iter().any(|spent| spent == operation)
    }

    /// Release one never-started explicit placement's spend and drop its
    /// write correlation: the token still owes its first attempt if a later
    /// accepted reply republishes it eligible. Never called once a native
    /// call has started (that attempt is owed to the core).
    pub(crate) fn release_frame_op(&mut self, pid: u32, cmd_id: u64, operation: &str) -> bool {
        self.pending_writes.remove(&(pid, cmd_id));
        let Some(index) = self
            .spent_frame_ops
            .iter()
            .position(|spent| spent == operation)
        else {
            return false;
        };
        self.spent_frame_ops.remove(index);
        true
    }
}

/// Canonical runtime window identity (`"<pid>_<wid>"`) for an intent
/// subject. Non-canonical text (leading zeros, extra separators) and the
/// reserved low PID range are refused rather than guessed.
fn canonical_intent_identity(win: &str) -> Option<(u32, u32)> {
    let (pid, wid) = parse_window_id(win)?;
    (pid > 1 && wid > 0 && window_id(pid, wid) == win).then_some((pid, wid))
}

/// Normal-level diagnostic for one unverifiable intent binding. R15 permits
/// the process identity but not the native window handle, so the canonical
/// `<pid>_<wid>` id is reduced to its pid; malformed input names no subject.
fn intent_unverifiable_diagnostic(win: &str) -> String {
    match canonical_intent_identity(win) {
        Some((pid, _)) => {
            format!("intent-store: intent binding unverifiable for pid {pid}; row not persisted")
        }
        None => "intent-store: intent binding unverifiable; row not persisted".to_owned(),
    }
}

/// Minimized lifetimes for one report, in stable order.
pub(crate) fn minimized_entries(retained: &RetainedCore) -> Vec<MinimizedRecord> {
    let mut entries: Vec<MinimizedRecord> = retained.minimized.values().cloned().collect();
    entries.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    entries
}

/// Grounded focus home-current verdict for one presentation ingress: the
/// retained home-current verdict (homed-but-not-current and unhomed both
/// fail closed to hidden-workspace). Pure over retained truth plus the
/// dispatch context; both the native and synthetic branches share it.
pub(crate) fn focus_eligibility_facts(
    retained: &RetainedCore,
    ctx: &NativeContext,
    _memory: &LifetimeMemory,
    focused_global: &str,
) -> crate::world_builder::FocusEligibilityFacts {
    let home_current = retained
        .window_domains
        .get(focused_global)
        .and_then(|home| {
            ctx.domains
                .iter()
                .find(|domain| domain.output == home.output && domain.workspace == home.workspace)
        })
        .map(|domain| domain.current);
    crate::world_builder::FocusEligibilityFacts { home_current }
}

/// Whether the retained observed-focus window currently reports native
/// fullscreen (the focused-fullscreen-game lane for the presentation
/// `paused` flag). Unknown or minimized focus reads false.
fn is_focused_fullscreen(retained: &RetainedCore, focused_global: &str) -> bool {
    if focused_global.is_empty() {
        return false;
    }
    let Some((pid, wid)) = parse_window_id(focused_global) else {
        return false;
    };
    retained
        .apps
        .get(&pid)
        .and_then(|app| app.roster.get(&wid))
        .is_some_and(|entry| {
            entry.excluded.is_none() && !entry.minimized_meta && entry.fullscreen_meta
        })
}

/// Whether a classifier exclusion mints a retained hidden-lane lifetime
/// (option B): genuine `Minimized` plus owner-hidden `HiddenApp` (Cmd+H).
/// Both ride the retained-lifetime path (never silent absence, never
/// inferred close); the emitted fact keeps the truthful `minimized` /
/// `host_hidden` legs apart (a hidden-app row reads `minimized: false`,
/// `host_hidden: true`), and the true native cause stays in diagnostics
/// via `ExcludeReason::log_token` (`classifier-omitted-minimized` vs
/// `classifier-omitted-hidden-app`), never in the fact itself.
fn is_minimized_source(reason: crate::classifier::ExcludeReason) -> bool {
    use crate::classifier::ExcludeReason;
    matches!(reason, ExcludeReason::Minimized | ExcludeReason::HiddenApp)
}

/// Refresh retained minimized lifetimes from one scoped snapshot's
/// classifier exclusions: minimized and hidden-app wids gain entries under
/// last-known homing; windows live again (or closed, or homing-less) drop
/// out, and entries whose home domain retired drop too.
///
/// Ordering: call BEFORE [`note_scoped_members`](crate::runtime_core::note_scoped_members)
/// for the same snapshot — the mapping update prunes this PID first, so
/// only the pre-update homing still names the minimized window's home.
pub(crate) fn refresh_minimized(
    retained: &mut RetainedCore,
    pid: u32,
    snapshot: &ScopedBackendSnapshot,
) {
    use std::collections::BTreeSet;
    let live: BTreeSet<String> = snapshot
        .windows
        .iter()
        .map(|w| window_id(pid, w.wid))
        .collect();
    let excluded_minimized: BTreeSet<String> = snapshot
        .excluded_reasons
        .iter()
        .filter(|o| is_minimized_source(o.reason))
        .map(|o| window_id(pid, o.wid))
        .collect();
    // Live windows are never minimized rows; windows absent from members
    // AND still-minimized exclusions closed (or reclassified) in-app: an
    // accepted snapshot lists every excluded window with its reason, and
    // whole-snapshot gaps refuse before this runs — so absence here is a
    // close, never a transient. Other PIDs' entries never match the
    // PID-scoped sets and survive.
    let prefix = format!("{pid}_");
    retained.minimized.retain(|id, _| {
        if !id.starts_with(&prefix) {
            return true;
        }
        !live.contains(id) && excluded_minimized.contains(id)
    });
    for omission in &snapshot.excluded_reasons {
        if !is_minimized_source(omission.reason) {
            continue;
        }
        let id = window_id(pid, omission.wid);
        if live.contains(&id) {
            continue;
        }
        // Truthful hidden-lane legs from the just-applied roster (covering
        // snapshots refresh it above; otherwise the retained row stands):
        // show-state plus actually read raw legs, null when unknown. A
        // hand-built snapshot without roster detail falls back to the
        // reason-derived show-state with null raw legs.
        let roster = retained
            .apps
            .get(&pid)
            .and_then(|app| app.roster.get(&omission.wid));
        let minimized = roster
            .map(|e| e.minimized_meta)
            .unwrap_or(omission.reason == crate::classifier::ExcludeReason::Minimized);
        let host_hidden = roster
            .map(|e| e.hidden_meta)
            .unwrap_or(omission.reason == crate::classifier::ExcludeReason::HiddenApp);
        match retained.window_domains.get(&id).cloned() {
            Some(key) => {
                retained.minimized.insert(
                    id.clone(),
                    MinimizedRecord {
                        window: WindowId(id),
                        output: key.output,
                        workspace: key.workspace,
                        sticky: false,
                        minimized,
                        host_hidden,
                        fullscreen: roster.map(|e| e.fullscreen_meta).unwrap_or(false),
                        process_id: Some(pid.to_string()),
                        role: roster.and_then(|e| e.role.clone()),
                        subrole: roster.and_then(|e| e.subrole.clone()),
                        modal: roster.and_then(|e| e.modal),
                        resizable: roster.and_then(|e| e.resizable),
                        layer: roster.and_then(|e| e.layer),
                        app_id: roster.and_then(|e| e.app_id.clone()),
                        activation_policy: roster.and_then(|e| e.activation_policy),
                    },
                );
            }
            None => {
                // Repeated hidden/minimized snapshot after the mapping prune:
                // `note_scoped_members` prunes this PID first, so the second
                // same-lifetime snapshot has no live homing to mint from.
                // Preserve the previous proven home instead of dropping the
                // just-retained fact. Unknown never-homed windows still
                // cannot fabricate a home (no entry exists to preserve).
                // Actual close (absent from exclusions) already dropped
                // above via the retain gate; newer homing overwrites via
                // the `Some` arm on the next observed homing; retired
                // domains drop in the known-domain fence below.
            }
        }
    }
    // Entries whose home vanished (display/Space retired) drop: a hold
    // without a live domain authorizes nothing and never resurrects.
    let known: BTreeSet<DomainKey> = retained.known_domains.keys().cloned().collect();
    retained.minimized.retain(|_, entry| {
        known.contains(&DomainKey {
            output: entry.output.clone(),
            workspace: entry.workspace.clone(),
        })
    });
}

/// Member windows per domain as `(pid, window)` pairs for
/// [`ReportInput`]: every retained window currently homed to the key.
/// Observations arrive per app, but each report converges complete domain
/// membership — a partial observation must never read as mass removal.
fn report_members(
    retained: &RetainedCore,
    keys: &[DomainKey],
) -> BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> {
    use crate::runtime_core::domain_member_apps;
    let mut out: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    for key in keys {
        let mut members: Vec<(u32, ObservedWindow)> = Vec::new();
        for (pid, windows) in domain_member_apps(retained, key) {
            for w in windows {
                members.push((pid, w));
            }
        }
        members.sort_by_key(|a| (a.0, a.1.wid));
        if !members.is_empty() {
            out.insert(key.clone(), members);
        }
    }
    out
}

/// Attribute one AX hit owner `(pid, wid)` to a live retained member window.
///
/// Both legs must be current: the window must be exactly homed
/// ([`RetainedCore::window_domains`]) and its app must still be a live
/// retained generation ([`RetainedCore::apps`]). A chained app relaunch
/// clears its retained rows, so a stale `(pid, wid)` never resurrects
/// through a new generation. Foreign apps, chrome/desk surfaces (the hit
/// test returns no owning window), and unreadable hits (`owner` None)
/// attribute to nothing.
pub(crate) fn attribute_cursor_hit(
    retained: &RetainedCore,
    owner: Option<(u32, u32)>,
) -> Option<WindowId> {
    let (pid, wid) = owner?;
    let id = window_id(pid, wid);
    if !retained.window_domains.contains_key(&id) || !retained.apps.contains_key(&pid) {
        return None;
    }
    Some(WindowId(id))
}

/// Dispatch context for one report: native topology plus the skeleton
/// fallback. Empty native contexts never report (KDE null parity).
pub(crate) enum DispatchContext {
    Native(Arc<NativeContext>),
    Synthetic,
}

/// Build one complete world report from retained truth. Returns `None`
/// when there is nothing to report (native mode without a derived
/// context, or a context with no domains): the caller refuses instead of
/// actuating on emptiness.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_dispatch_report(
    session: &mut WorldSession,
    retained: &RetainedCore,
    dispatch: &DispatchContext,
    closed: &[WindowId],
    ticket_observation: u64,
    ticket_request: u64,
    ticket_command_seq: Option<u64>,
    ticket_command: Option<WorldCommand>,
    gesture: Option<GestureIngress>,
    request_id: &str,
) -> Option<WorldReportV4> {
    session.detached_select_members = None;
    // No-context guard BEFORE any drain: an empty native context reports
    // nothing (KDE null parity), and the staged rows must survive for the
    // next real dispatch — attempts describe attempted actuation and must
    // not be dropped by a report that never runs.
    if let DispatchContext::Native(ctx) = dispatch
        && ctx.domains.is_empty()
    {
        return None;
    }
    let view_rows = session.take_view_attempts();
    let member_rows = session.take_member_attempts();
    let frame_rows = session.take_frame_attempts();
    let creation_rows = session.take_creation_attempts();
    let focus_rows: Vec<FocusFeedback> = session.take_close_feedback().into_iter().collect();
    let memory = session.lifetime_memory();
    let focused_global = retained
        .observed_focus
        .as_ref()
        .map(|w| w.0.clone())
        .unwrap_or_default();
    // Truthful foreground provenance: non-empty focus is always proven bound
    // (the codec refuses false+nonempty); empty focus is known only on
    // genuine windowless proof, otherwise unknown. Never invent known from
    // bare absence.
    let foreground_known = !focused_global.is_empty() || retained.observed_focus_known;
    // Stored pointer sample (sample-time attribution): never consumed here,
    // so a resting pointer keeps its hover memory across reports. The
    // builder withholds the pair unless the hit still names a window row
    // emitted below, and a missing sample omits the pair entirely.
    let (cursor, cursor_hit) = match session.cursor_sample() {
        Some(sample) => (Some(sample.point), sample.hit.clone()),
        None => (None, None),
    };
    match dispatch {
        DispatchContext::Native(ctx) => {
            let keys: Vec<DomainKey> = ctx
                .domains
                .iter()
                .map(|d| DomainKey {
                    output: d.output.clone(),
                    workspace: d.workspace.clone(),
                })
                .collect();
            let members = report_members(retained, &keys);
            // Explicit closes are flat ids (the actual authority, no
            // homing, no invented keys): the caller flat list plus the
            // pending outbox merge. Unknown-home ids ride explicitly even
            // with incomplete inventory.
            let mut closed_flat: Vec<WindowId> = session
                .pending_closes
                .iter()
                .cloned()
                .chain(closed.iter().cloned())
                .collect::<BTreeSet<WindowId>>()
                .into_iter()
                .collect();
            closed_flat.sort_by(|a, b| a.0.cmp(&b.0));
            // Unknown-home roster rows surface explicitly (native None):
            // live-only rows with no domain homing (empty, multi, tied,
            // or unhomed SID). Classifier-excluded rows never surface as
            // live facts, even when a stale prior frame lingers: a hold
            // (not a false live fact) covers the unrepresentable, and
            // genuine minimized lifetimes ride the MinimizedRecord path.
            // Homing is the authority, not the normalized spaces alone,
            // so shared-Space ties also hold here.
            let mut unknown: Vec<(u32, ObservedWindow)> = Vec::new();
            let mut detached_members = session.detached_fullscreen_view().then(BTreeMap::new);
            for (pid, app) in &retained.apps {
                for (wid, entry) in &app.roster {
                    if entry.excluded.is_some() {
                        continue;
                    }
                    let Some(obs) = entry.observed else { continue };
                    let id = crate::runtime_types::window_id(*pid, *wid);
                    if !retained.window_domains.contains_key(&id) {
                        unknown.push((*pid, obs));
                        if let Some(members) = &mut detached_members
                            && !session.coverage_gap
                            && !app.roster_incomplete
                            && let Some((revision, sid)) = entry.detached_membership
                            && revision == ctx.revision
                        {
                            members.insert((*pid, *wid), sid);
                        }
                    }
                }
            }
            unknown.sort_by_key(|(pid, w)| (*pid, w.wid));
            let roster_incomplete = retained.apps.values().any(|a| a.roster_incomplete);
            let unreadable: BTreeMap<DomainKey, HoldReason> = ctx
                .domains
                .iter()
                .filter(|d| !d.readable)
                .map(|d| {
                    (
                        DomainKey {
                            output: d.output.clone(),
                            workspace: d.workspace.clone(),
                        },
                        HoldReason::Unreadable,
                    )
                })
                .collect();
            // Coverage gap (a refused observation missed input) or skipped
            // displays (unprovable homing on live outputs) both publish
            // incomplete inventory; the gap cannot be localized per output,
            // so `complete = false` marks every row stale until fresh
            // truth lands.
            let incomplete = session.coverage_gap || !ctx.skipped.is_empty();
            let owned_command = ticket_command.clone();
            let owned_request = request_id.to_owned();
            let minimized = minimized_entries(retained);
            let focused_fullscreen = is_focused_fullscreen(retained, &focused_global);
            let eligibility = focus_eligibility_facts(retained, ctx, &memory, &focused_global);
            let style = session.presentation_style;
            let presentation = crate::world_builder::build_presentation_ingress(
                &focused_global,
                &members,
                &unknown,
                &minimized,
                focused_fullscreen,
                &eligibility,
                &style,
            );
            // Neutral raw facts per runtime window id for v4 core
            // classification (admitted rows only; excluded rows carry
            // default/null via `raw_facts`). Unknown-home grounded rows
            // resolve through the same map.
            let raw: BTreeMap<String, crate::runtime_types::WindowRawFacts> = retained
                .apps
                .iter()
                .flat_map(|(pid, app)| {
                    app.roster.iter().map(|(wid, entry)| {
                        (
                            crate::runtime_types::window_id(*pid, *wid),
                            entry.raw_facts(),
                        )
                    })
                })
                .collect();
            let bound_generation = session.sequencer().bound_generation();
            let mut input = ReportInput {
                adapter_incarnation: session.incarnation(),
                core_generation: &bound_generation,
                observation_seq: ticket_observation,
                request_seq: ticket_request,
                request_id: &owned_request,
                command_seq: ticket_command_seq,
                command: owned_command,
                ctx,
                members: &members,
                memory: &memory,
                closed: &closed_flat,
                unknown: &unknown,
                detached_members: detached_members.as_ref(),
                raw: &raw,
                roster_incomplete,
                minimized: &minimized,
                unreadable: &unreadable,
                frames: &frame_rows,
                member_attempts: &member_rows,
                view_attempts: &view_rows,
                creation_attempts: &creation_rows,
                focus_feedback: &focus_rows,
                gesture,
                focused_global: &focused_global,
                foreground_known,
                complete: !incomplete,
                coverage_gap: session.coverage_gap,
                incomplete_outputs: &[],
                gesture_barrier: false,
                primary_output: None,
                monotonic_ms: crate::input_eligibility::now_ms(),
                cursor,
                cursor_hit,
                presentation,
            };
            let mut report = build_report(&mut input);
            if detached_members.is_some()
                && let Some(command_seq) = ticket_command_seq
            {
                let pins = report
                    .windows
                    .iter()
                    .filter(|fact| fact.detached_surface.is_some())
                    .filter_map(|fact| {
                        let (pid, _) = crate::runtime_types::parse_window_id(&fact.window.0)?;
                        let generation = retained.apps.get(&pid)?.generation;
                        Some((fact.window.clone(), generation))
                    })
                    .collect();
                session.detached_select_members = Some((command_seq, pins));
            }
            let records = {
                let present: Vec<&str> = report
                    .windows
                    .iter()
                    .map(|window| window.window.0.as_str())
                    .collect();
                session.intent_records_for_admission(&present)
            };
            report.intent_records = records;
            Some(report)
        }
        DispatchContext::Synthetic => {
            let key = DomainKey {
                output: OutputId(PHASE1_OUTPUT.to_owned()),
                workspace: WorkspaceId(PHASE1_WORKSPACE.to_owned()),
            };
            // Synthetic legacy observations home every retained window to
            // the skeleton domain at ingest (see `note_synthetic_homing`),
            // so membership here is exactly the homed set.
            let members = report_members(retained, std::slice::from_ref(&key));
            let ctx = NativeContext {
                revision: 0,
                domains: vec![NativeDomain {
                    cg_id: 0,
                    uuid: None,
                    output: OutputId(PHASE1_OUTPUT.to_owned()),
                    workspace: WorkspaceId(PHASE1_WORKSPACE.to_owned()),
                    space: 0,
                    current: true,
                    readable: true,
                    bounds: PHASE1_BOUNDS,
                    visible: PHASE1_BOUNDS,
                }],
                skipped: Vec::new(),
                detached_spaces: BTreeMap::new(),
            };
            let owned_command = ticket_command.clone();
            let owned_request = request_id.to_owned();
            let empty_unreadable: BTreeMap<DomainKey, HoldReason> = BTreeMap::new();
            // Synthetic lifecycle also carries the pending flat outbox so
            // enqueued closes are never lost on that route.
            let mut closed_flat: Vec<WindowId> = session
                .pending_closes
                .iter()
                .cloned()
                .chain(closed.iter().cloned())
                .collect::<BTreeSet<WindowId>>()
                .into_iter()
                .collect();
            closed_flat.sort_by(|a, b| a.0.cmp(&b.0));
            // Same retained facts as the native route: explicit minimized
            // rows and the retained fullscreen bit ride truthfully here
            // too, never empty minimized/false fullscreen.
            let minimized = minimized_entries(retained);
            let focused_fullscreen = is_focused_fullscreen(retained, &focused_global);
            let eligibility = focus_eligibility_facts(retained, &ctx, &memory, &focused_global);
            let style = session.presentation_style;
            let bound_generation = session.sequencer().bound_generation();
            let mut input = ReportInput {
                adapter_incarnation: session.incarnation(),
                core_generation: &bound_generation,
                observation_seq: ticket_observation,
                request_seq: ticket_request,
                request_id: &owned_request,
                command_seq: ticket_command_seq,
                command: owned_command,
                ctx: &ctx,
                members: &members,
                memory: &memory,
                closed: &closed_flat,
                unknown: &[],
                detached_members: None,
                raw: &crate::runtime_types::EMPTY_RAW_FACTS,
                roster_incomplete: false,
                minimized: &minimized,
                unreadable: &empty_unreadable,
                frames: &frame_rows,
                member_attempts: &member_rows,
                view_attempts: &view_rows,
                creation_attempts: &creation_rows,
                focus_feedback: &focus_rows,
                gesture,
                focused_global: &focused_global,
                foreground_known,
                complete: true,
                coverage_gap: session.coverage_gap,
                incomplete_outputs: &[],
                gesture_barrier: false,
                primary_output: None,
                monotonic_ms: crate::input_eligibility::now_ms(),
                cursor,
                cursor_hit,
                presentation: crate::world_builder::build_presentation_ingress(
                    &focused_global,
                    &members,
                    &[],
                    &minimized,
                    focused_fullscreen,
                    &eligibility,
                    &style,
                ),
            };
            let mut report = build_report(&mut input);
            let records = {
                let present: Vec<&str> = report
                    .windows
                    .iter()
                    .map(|window| window.window.0.as_str())
                    .collect();
                session.intent_records_for_admission(&present)
            };
            report.intent_records = records;
            Some(report)
        }
    }
}

/// Map one applied gesture reply to its read-only preview outcome: a
/// core-resolved insertion slot shows the filled rect above the
/// core-resolved target neighbor (never the mover, never an adapter
/// guess) only when the retained preview names the active gesture. The
/// core preserves an older gesture's preview across an ignored mismatched
/// newcomer, so an unbound id never repaints a replacement gesture;
/// every other applied reply hides transiently with the gesture retained
/// for the next sample. The carried sticky hover stays core-side (the
/// ingress carries no hover prior); the outcome still reports it for
/// evidence parity with the offline harness, but production never feeds
/// it back into the next ingress.
pub(crate) fn map_gesture_preview(
    reply: &tiler_core::world_report::WorldReplyV4,
    gesture_id: u64,
) -> crate::drag_drop::PreviewOutcome {
    use crate::drag_drop::PreviewOutcome;
    match &reply.drag_preview {
        Some(preview) if preview.id == gesture_id => PreviewOutcome::Show {
            rect: preview.rect,
            hover_prior: preview.hover.clone().map(Box::new),
            target: preview.target.clone(),
        },
        Some(_) => PreviewOutcome::HideTransient {
            reason: "stale-preview",
        },
        None => PreviewOutcome::HideTransient { reason: "held" },
    }
}

/// Map one [`AxCompletion`] to its honest setter disposition: only a
/// reported success stages `Accepted` (arrival is never claimed here —
/// later observations prove it); partial writes and timeouts are
/// throwing/ambiguous (`Threw`, never blindly repeated); proven failures
/// are explicit refusals (`Refused`). `FencedNoCall` is a pre-call refusal
/// (no AX setter ran) and is short-circuited by the staging path before
/// this mapping: it must never stage a row at all.
pub(crate) fn completion_setter(completion: &AxCompletion) -> FrameSetter {
    match completion {
        AxCompletion::Applied { .. } => FrameSetter::Accepted,
        AxCompletion::PartialApplied { .. } => FrameSetter::Threw,
        AxCompletion::Failed { .. } | AxCompletion::FencedNoCall { .. } => FrameSetter::Refused,
        AxCompletion::Timeout { .. } => FrameSetter::Threw,
    }
}

/// Immediate post-write readback from one [`AxCompletion`], when the
/// worker captured a carriable one (`None` = unreadable, not carriable on
/// the wire, or no readback lane, never guessed). The wire bound is
/// enforced here as the single staging choke point: AppKit can report
/// float geometry outside the carried rectangle, and a non-carriable
/// readback must ride as unreadable rather than invalidate the whole
/// report (`frame-readback-invalid`).
pub(crate) fn completion_readback(completion: &AxCompletion) -> Option<Rect> {
    let (AxCompletion::Applied {
        readback_xywh: Some((x, y, w, h)),
        ..
    }
    | AxCompletion::PartialApplied {
        readback_xywh: Some((x, y, w, h)),
        ..
    }) = completion
    else {
        return None;
    };
    tiler_core::bounds::valid_carried_rect(*x, *y, *w, *h).then_some(Rect {
        x: *x,
        y: *y,
        w: *w,
        h: *h,
    })
}

/// Track one emitted frame write carrying an explicit placement token
/// (`frame_operation`) for raw-attempt correlation. Placement writes are
/// one-shot imperative operations, so they keep their own correlation space
/// (see [`track_layout_write`] for the ordinary lane): neither may evict
/// the other. Bounded to [`FRAME_CORRELATION_CAP`] entries (oldest
/// placement evicted). The attempted target frame rides along as the core's
/// correlation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn track_write(
    session: &mut WorldSession,
    wpid: u32,
    cmd_id: u64,
    window: &str,
    target: Rect,
    operation: Option<&str>,
) {
    let Some(operation) = operation else {
        return;
    };
    if session.pending_writes.len() >= FRAME_CORRELATION_CAP {
        session.pending_writes.pop_first();
    }
    session.pending_writes.insert(
        (wpid, cmd_id),
        (window.to_owned(), target, Some(operation.to_owned())),
    );
}

/// Track one emitted ordinary layout write (a `Controlled` row, which
/// carries no placement token) for raw-attempt correlation. The core's
/// anti-fighting policy (clamp memory, transient backoff, burst budget)
/// only consumes reported attempts, so a macOS Controlled write must stage
/// one; keeping it in its own bounded map means layout traffic can never
/// evict a one-shot placement entry nor be mistaken for one. Bounded to
/// [`FRAME_CORRELATION_CAP`] entries (oldest evicted).
pub(crate) fn track_layout_write(
    session: &mut WorldSession,
    wpid: u32,
    cmd_id: u64,
    window: &str,
    target: Rect,
) {
    if session.pending_layout_writes.len() >= FRAME_CORRELATION_CAP {
        session.pending_layout_writes.pop_first();
    }
    session
        .pending_layout_writes
        .insert((wpid, cmd_id), (window.to_owned(), target));
}

/// Stage one raw frame attempt from one correlation: an explicit placement
/// token settles with the recorded disposition (never false success), and
/// an ordinary layout write stages `FramePurpose::Layout`. Both lanes carry
/// the immediate post-write readback so the core can distinguish a
/// successful-but-clamped write from a transient failure. `dpi_before` /
/// `dpi_after` are always `None` (no per-window basis, never guessed);
/// `prewrite` is always `None` (no pre-write read at completion time —
/// only an explicit refusal with an unchanged readable preimage would
/// prove refusal, so a bare refusal latches ambiguity, never a verdict).
/// `attempt_ms` is the process-monotonic stamp of the native call on the
/// same epoch as the report `monotonic_ms` (`None` while clockless), never
/// a per-call duration: the core compares it against observation stamps for
/// burst quiet re-arm and transient backoff. Each fresh attempt takes one
/// producer-minted identity; the same staged row keeps it across report
/// resends.
/// Consumes the correlation entry (one completion per command) and drops
/// stale generations (a relaunched PID never settles under an old
/// command). A typed pre-call fence ([`AxCompletion::FencedNoCall`])
/// stages nothing on either lane: no AX setter ran, so it must not spend
/// core burst or transient budget. Returns whether an attempt row staged
/// (the caller advances the watermark only then: the staged row changes
/// the next report).
#[allow(clippy::too_many_arguments)]
pub(crate) fn stage_write_feedback(
    session: &mut WorldSession,
    retained: &RetainedCore,
    pid: u32,
    generation: u64,
    cmd_id: u64,
    completion: &AxCompletion,
    attempt_ms: Option<u64>,
) -> bool {
    if retained
        .apps
        .get(&pid)
        .is_none_or(|app| app.generation != generation)
    {
        session.pending_writes.remove(&(pid, cmd_id));
        session.pending_layout_writes.remove(&(pid, cmd_id));
        return false;
    }
    let placement = session.pending_writes.remove(&(pid, cmd_id));
    let layout = session.pending_layout_writes.remove(&(pid, cmd_id));
    if matches!(completion, AxCompletion::FencedNoCall { .. }) {
        return false;
    }
    if let Some((window, target, operation)) = placement {
        let Some(operation) = operation else {
            return false;
        };
        let attempt_id = session.sequencer().next_frame_attempt_id();
        session.pending_frames.insert(
            window.clone(),
            FrameAttempt {
                window: WindowId(window),
                target,
                disposition: completion_setter(completion),
                readback: completion_readback(completion),
                dpi_before: None,
                dpi_after: None,
                attempt_id,
                attempt_ms,
                purpose: FramePurpose::FloatPlacement {
                    operation_id: operation,
                    prewrite: None,
                },
            },
        );
        return true;
    }
    let Some((window, target)) = layout else {
        return false;
    };
    let attempt_id = session.sequencer().next_frame_attempt_id();
    if session.pending_layouts.len() >= FRAME_CORRELATION_CAP {
        session.pending_layouts.pop_first();
    }
    session.pending_layouts.insert(
        window.clone(),
        FrameAttempt {
            window: WindowId(window),
            target,
            disposition: completion_setter(completion),
            readback: completion_readback(completion),
            dpi_before: None,
            dpi_after: None,
            attempt_id,
            attempt_ms,
            purpose: FramePurpose::Layout,
        },
    );
    true
}

/// Drop every retained mapping row for one terminated PID: homing,
/// minimized lifetimes, and both write-correlation lanes plus any staged
/// layout attempt carried by its windows. Called before the terminate
/// dispatch so closed windows ride as closes, never members; a terminated
/// lifetime's layout feedback is dead evidence (core prunes it anyway) and
/// must never cost a frame row in the terminate report.
pub(crate) fn forget_world_members(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    pid: u32,
) {
    crate::runtime_core::forget_app_members(retained, pid);
    retained
        .minimized
        .retain(|id, _| !id.starts_with(&format!("{pid}_")));
    session.pending_writes.retain(|(wpid, _), _| *wpid != pid);
    session
        .pending_layout_writes
        .retain(|(wpid, _), _| *wpid != pid);
    let prefix = format!("{pid}_");
    session
        .pending_layouts
        .retain(|window, _| !window.starts_with(&prefix));
    clear_focus_into(retained, pid);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use tiler_core::directional::WindowId;
    use tiler_core::world_report::{FocusDisposition, FocusFeedback, FocusNoCallReason};

    use crate::focus_route::FocusRoute;

    fn fixed_session(nonce: &str) -> WorldSession {
        WorldSession::new_with_tokens(
            Some((format!("macos-{nonce}"), format!("phase-1-{nonce}"))),
            false,
            FocusRoute::Slps,
            Arc::new(ObservationClock::new()),
            &[],
        )
    }

    fn feedback_row(id: &str, revision: u64) -> FocusFeedback {
        FocusFeedback {
            id: id.to_owned(),
            target: WindowId("1001_2".to_owned()),
            revision,
            disposition: FocusDisposition::NotCalled {
                reason: FocusNoCallReason::NoReason,
            },
            pre: Some(WindowId("1001_2".to_owned())),
            post: Some(WindowId("1001_2".to_owned())),
            foreground_proven: true,
            arrival: Vec::new(),
            attempt_ms: 7,
        }
    }

    #[test]
    fn close_feedback_first_wins_and_drains_once_with_stamp() {
        let mut session = fixed_session("cccccccccccccccccccccccccccccccc");
        let first = feedback_row("close-focus-3", 3);
        let second = feedback_row("close-focus-4", 4);
        session.stage_close_feedback(first.clone());
        session.stage_close_feedback(second);
        // First-wins: the second staged row drops.
        let drained = session.take_close_feedback().expect("staged row");
        assert_eq!(drained, first, "original stamp forwarded verbatim");
        assert!(
            session.take_close_feedback().is_none(),
            "drained once, never retried"
        );
        // After a drain a new row stages again.
        let third = feedback_row("close-focus-5", 5);
        session.stage_close_feedback(third.clone());
        assert_eq!(session.take_close_feedback(), Some(third));
    }

    #[test]
    fn close_feedback_rides_next_dispatch_report_once() {
        let mut session = fixed_session("dddddddddddddddddddddddddddddddd");
        session.stage_close_feedback(feedback_row("close-focus-3", 3));
        let retained = RetainedCore::default();
        let first = build_dispatch_report(
            &mut session,
            &retained,
            &DispatchContext::Synthetic,
            &[],
            1,
            1,
            None,
            None,
            None,
            "req-1",
        )
        .expect("synthetic report builds");
        assert_eq!(first.focus_feedback.len(), 1);
        assert_eq!(first.focus_feedback[0].id, "close-focus-3");
        assert_eq!(first.focus_feedback[0].attempt_ms, 7);
        // Second dispatch carries nothing: forwarded once.
        let second = build_dispatch_report(
            &mut session,
            &retained,
            &DispatchContext::Synthetic,
            &[],
            2,
            2,
            None,
            None,
            None,
            "req-2",
        )
        .expect("synthetic report builds");
        assert!(second.focus_feedback.is_empty());
    }

    #[test]
    fn spent_frame_ops_are_bounded_like_the_placement_correlation() {
        // Explicit placement tokens are per-command and never recycled
        // within one core generation, so the spent set must stay bounded
        // instead of growing for the life of the session (the write
        // correlation map shares the same bound).
        let mut session = fixed_session("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
        for id in 0..=FRAME_CORRELATION_CAP {
            assert!(
                session.spend_frame_op(&format!("op-{id}")),
                "a fresh token spends once: op-{id}"
            );
        }
        assert!(
            session.spend_frame_op("op-0"),
            "the oldest spend is evicted past the bound"
        );
        assert!(
            !session.spend_frame_op(&format!("op-{FRAME_CORRELATION_CAP}")),
            "the newest spend stays remembered"
        );
    }

    #[test]
    fn generation_rebind_clears_placement_spend_and_deferrals() {
        // Both are scoped to the bound core generation: core command
        // sequences restart with a fresh generation, so a stale spend would
        // pre-refuse a NEW token that reuses the same string, and a deferred
        // reveal would write a dead generation's durable target.
        let mut session = fixed_session("ffffffffffffffffffffffffffffffff");
        assert!(session.spend_frame_op("op-2"));
        session.pending_fullscreen_float_placements.insert(
            "1001_2".to_owned(),
            PendingFullscreenFloatPlacement {
                operation: "op-2".to_owned(),
                target: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                generation: 1,
            },
        );
        assert!(session.sync_settlement_generation("gen-2"), "rebind");
        assert!(
            session.spend_frame_op("op-2"),
            "a fresh generation's token is never pre-refused by a dead spend"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "a dead generation's deferred reveal is withdrawn"
        );
    }

    #[test]
    fn released_placement_spend_allows_the_first_attempt_again() {
        // A token cancelled before any native call hands its spend back, so
        // the same token republished eligible still gets its first attempt.
        // Release also drops the write correlation (nothing ran, so no
        // completion can ever consume it) and a second release changes
        // nothing.
        let mut session = fixed_session("dddddddddddddddddddddddddddddddd");
        let target = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        track_write(
            &mut session,
            1001,
            7,
            "1001_2",
            target,
            Some("op-2"),
        );
        assert!(session.spend_frame_op("op-2"));
        assert!(session.frame_op_spent("op-2"));
        assert_eq!(session.pending_writes.len(), 1);
        assert!(session.release_frame_op(1001, 7, "op-2"));
        assert!(!session.frame_op_spent("op-2"));
        assert!(
            session.pending_writes.is_empty(),
            "a never-started placement's correlation is dropped with its spend"
        );
        assert!(
            !session.release_frame_op(1001, 7, "op-2"),
            "a second release finds nothing and changes nothing"
        );
        assert!(
            session.spend_frame_op("op-2"),
            "the republished token spends again: its first attempt is still owed"
        );
    }

    #[test]
    fn unverifiable_intent_diagnostic_redacts_native_window_id() {
        // R15: normal-level diagnostics may name the pid, never the native
        // window handle. A malformed id names no subject at all.
        let line = intent_unverifiable_diagnostic("4242_100021");
        assert!(line.contains("pid 4242"), "pid rides: {line}");
        assert!(!line.contains("100021"), "raw wid must not leak: {line}");
        assert!(
            !line.contains("4242_100021"),
            "canonical id must not leak: {line}"
        );
        let malformed = intent_unverifiable_diagnostic("not-a-window");
        assert!(!malformed.contains("not-a-window"), "{malformed}");
        assert_eq!(
            malformed,
            "intent-store: intent binding unverifiable; row not persisted"
        );
    }
}
