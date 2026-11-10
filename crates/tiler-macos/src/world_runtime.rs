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
//! - One-shot effects spend before first dispatch (sequencer-owned); the
//!   reply snapshot is plan truth, never a success claim — feedback rows
//!   stage only proven drain outcomes.
//! - Empty native contexts never actuate (same as the KDE null): native
//!   mode with no derived context refuses instead of reporting.
//!
//! Geometry: screen rows carry FULL display bounds plus the RAW visible work
//! area with [`PHASE1_OUTER_GAP`](crate::runtime_types::PHASE1_OUTER_GAP) separately; core applies the inset once.
//! The synthetic skeleton carries zero outer gap and raw [`PHASE1_BOUNDS`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::gesture::GestureIngress;
use tiler_core::session::DomainKey;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{
    FocusFeedback, FrameAttempt, FrameSetter, HoldReason, MemberAttempt, NativeCreationAttempt,
    ViewAttempt, WorldCommand, WorldReportV4,
};

use crate::focus_route::FocusRoute;
use crate::runtime::{NativeContext, ObservedWindow, parse_window_id, window_id};
use crate::runtime_core::{LifetimeMemory, MinimizedRecord, RetainedCore, clear_focus_into};
use crate::runtime_types::{
    AxCompletion, NativeDomain, PHASE1_BOUNDS, PHASE1_GENERATION, PHASE1_OUTPUT, PHASE1_WORKSPACE,
    ScopedBackendSnapshot,
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
    /// latest-wins per owning operation. Drained (cleared) on every
    /// dispatch whether or not the report is accepted — attempts describe
    /// attempted actuation, never retry themselves.
    pub(crate) pending_views: BTreeMap<String, ViewAttempt>,
    /// Staged raw membership attempts, latest-wins per (operation, window).
    pub(crate) pending_members: BTreeMap<(String, String), MemberAttempt>,
    /// Staged raw native creation attempts, latest-wins per creation
    /// operation id. Drained (cleared) on every dispatch whether or not
    /// the report is accepted — attempts describe attempted actuation,
    /// never retry themselves.
    pub(crate) pending_creations: BTreeMap<String, NativeCreationAttempt>,
    /// Staged raw frame attempts, latest-wins per window. Only explicit
    /// float-placement writes ever stage here; ordinary frame traffic
    /// rides the observation facts.
    pub(crate) pending_frames: BTreeMap<String, FrameAttempt>,
    /// Frame-write correlation: `(pid, cmd_id)` to the emitted window, its
    /// attempted frame, and its explicit placement token, if any. Bounded
    /// (oldest evicted).
    pending_writes: BTreeMap<(u32, u64), (String, Rect, Option<String>)>,
    /// Explicit float-placement tokens already written once (exactly-once
    /// geometry under `frame_operation`).
    spent_frame_ops: BTreeSet<String>,
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
    /// Session-lifetime fail-closed latch for unpersisted opaque intent
    /// saves: set when an applied reply carries nonempty `intent_writes`
    /// (no durable backend exists, so nothing is persisted). Once set it
    /// never clears — later replies without writes stay held. Ordinary
    /// admission/reflow emits no saves, so this never fires honestly.
    unpersisted_intent_writes: bool,
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
                pending_writes: BTreeMap::new(),
                spent_frame_ops: BTreeSet::new(),
                pending_close_feedback: None,
                coverage_gap: false,
                pending_closes: BTreeSet::new(),
                presentation_style: crate::active_border::PresentationStylePx::default(),
                connection_bound: false,
                initial_settings: None,
                unpersisted_intent_writes: false,
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
            pending_writes: BTreeMap::new(),
            spent_frame_ops: BTreeSet::new(),
            pending_close_feedback: None,
            coverage_gap: false,
            pending_closes: BTreeSet::new(),
            presentation_style: crate::active_border::PresentationStylePx::default(),
            connection_bound,
            initial_settings,
            unpersisted_intent_writes: false,
        }
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

    /// Whether the session already holds unpersisted intent writes
    /// (fail-closed latch, session lifetime).
    pub(crate) fn unpersisted_intent_hold(&self) -> bool {
        self.unpersisted_intent_writes
    }

    /// Latch unpersisted intent writes for the session lifetime. Never
    /// cleared: no durability backend exists, so no later reply can prove
    /// persistence.
    pub(crate) fn mark_unpersisted_intent_writes(&mut self) {
        self.unpersisted_intent_writes = true;
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
    /// Resolves the retained transfer target by operation identity (logical
    /// target correlation only, never adapter mapping). Native Space reads
    /// ride the dedicated native keys (`native_pre`/`native_post`/`native_baseline`
    /// via [`crate::topology::workspace_key`]); logical `pre`/`post`/`baseline`
    /// stay `None` because host-owned facts never translate native ids to
    /// logical receipts adapter-side. The core translates the native ids
    /// through its own bindings at settle time. `native_baseline` is the
    /// observation-time native current captured at dispatch (the plan's
    /// baseline); `native_pre`/`native_post` are the real write-time reads
    /// around the drain actuation. Unknown operations drop without evidence
    /// (never forged, never guessed). Latest-wins per owning operation.
    pub(crate) fn stage_view_attempt(
        &mut self,
        operation_id: &str,
        setter: FrameSetter,
        pre_sid: Option<u64>,
        post_sid: Option<u64>,
        baseline_sid: Option<u64>,
    ) {
        let transfer = self
            .engine
            .retained_transfers()
            .into_iter()
            .find(|target| target.operation_id == operation_id);
        let Some(transfer) = transfer else {
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
        let attempt = ViewAttempt {
            operation_id: operation_id.to_owned(),
            target_output: target_output.clone(),
            target_workspace,
            setter,
            pre: pre_sid.map(&to_native),
            post: post_sid.map(&to_native),
            baseline: baseline_sid.map(&to_native),
            attempt_ms: 0,
        };
        self.pending_views.insert(operation_id.to_owned(), attempt);
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
            setter,
            pre: pre_sid.map(&to_native),
            post: post_sid.map(&to_native),
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

    fn take_frame_attempts(&mut self) -> Vec<FrameAttempt> {
        std::mem::take(&mut self.pending_frames)
            .into_values()
            .collect()
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
    pub(crate) fn spend_frame_op(&mut self, operation: &str) -> bool {
        self.spent_frame_ops.insert(operation.to_owned())
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
        .and_then(|app| app.windows.iter().find(|w| w.wid == wid))
        .is_some_and(|w| w.fullscreen)
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
            for (pid, app) in &retained.apps {
                for (wid, entry) in &app.roster {
                    if entry.excluded.is_some() {
                        continue;
                    }
                    let Some(obs) = entry.observed else { continue };
                    let id = crate::runtime_types::window_id(*pid, *wid);
                    if !retained.window_domains.contains_key(&id) {
                        unknown.push((*pid, obs));
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
                raw: &raw,
                roster_incomplete,
                minimized: &minimized,
                unreadable: &unreadable,
                frames: &frame_rows,
                member_attempts: &member_rows,
                view_attempts: &view_rows,
                clear_attempts: &[],
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
                presentation,
            };
            let report = build_report(&mut input);
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
                raw: &*crate::runtime_types::EMPTY_RAW_FACTS,
                roster_incomplete: false,
                minimized: &minimized,
                unreadable: &empty_unreadable,
                frames: &frame_rows,
                member_attempts: &member_rows,
                view_attempts: &view_rows,
                clear_attempts: &[],
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
            let report = build_report(&mut input);
            Some(report)
        }
    }
}

/// Map one applied gesture reply to its read-only preview outcome: a
/// core-resolved insertion slot shows the filled rect above the
/// core-resolved target neighbor (never the mover, never an adapter
/// guess); every other applied reply hides transiently with the gesture
/// retained for the next sample. The carried sticky hover stays core-side
/// (the ingress carries no hover prior); the outcome still reports it for
/// evidence parity with the offline harness, but production never feeds
/// it back into the next ingress.
pub(crate) fn map_gesture_preview(
    reply: &tiler_core::world_report::WorldReplyV4,
) -> crate::drag_drop::PreviewOutcome {
    use crate::drag_drop::PreviewOutcome;
    use tiler_core::world_report::CommandResult;
    match &reply.command_result {
        Some(CommandResult::DragPreview {
            rect,
            hover,
            target,
            ..
        }) => PreviewOutcome::Show {
            rect: *rect,
            hover_prior: hover.clone().map(Box::new),
            target: target.clone(),
        },
        _ => PreviewOutcome::HideTransient { reason: "held" },
    }
}

/// Map one [`AxCompletion`] to its honest setter disposition: only a
/// reported success stages `Accepted` (arrival is never claimed here —
/// later observations prove it); partial writes and timeouts are
/// throwing/ambiguous (`Threw`, never blindly repeated); proven failures
/// are explicit refusals (`Refused`).
pub(crate) fn completion_setter(completion: &AxCompletion) -> FrameSetter {
    match completion {
        AxCompletion::Applied { .. } => FrameSetter::Accepted,
        AxCompletion::PartialApplied { .. } => FrameSetter::Threw,
        AxCompletion::Failed { .. } => FrameSetter::Refused,
        AxCompletion::Timeout { .. } => FrameSetter::Threw,
    }
}

/// Immediate post-write readback from one [`AxCompletion`], when the
/// worker captured it (`None` = unreadable, never guessed).
pub(crate) fn completion_readback(completion: &AxCompletion) -> Option<Rect> {
    match completion {
        AxCompletion::PartialApplied {
            readback_xywh: Some((x, y, w, h)),
            ..
        } => Some(Rect {
            x: *x,
            y: *y,
            w: *w,
            h: *h,
        }),
        _ => None,
    }
}

/// Track one emitted frame write for raw-attempt correlation: ONLY rows
/// carrying an explicit placement token (`frame_operation`) are recorded —
/// ordinary frame traffic stages nothing core-side (it rides the
/// completions counters), so it must never occupy correlation space nor
/// evict a pending placement entry. Bounded (oldest placement evicted).
/// The attempted target frame rides along as the core's correlation.
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
    if session.pending_writes.len() > 4096 {
        session.pending_writes.pop_first();
    }
    session.pending_writes.insert(
        (wpid, cmd_id),
        (window.to_owned(), target, Some(operation.to_owned())),
    );
}

/// Stage one raw frame attempt for an explicit placement token: only rows
/// carrying a `frame_operation` settle anything core-side, and only with
/// the recorded setter disposition (never false success). `dpi_before` /
/// `dpi_after` are always `None` (no per-window basis, never guessed);
/// `prewrite` is always `None` (no pre-write read at completion time —
/// only an explicit refusal with an unchanged readable preimage would
/// prove refusal, so a bare refusal latches ambiguity, never a verdict).
/// Ordinary frame writes stay on the completions counters and logs.
/// Consumes the correlation entry (one completion per command) and drops
/// stale generations (a relaunched PID never settles under an old
/// command). Returns whether an attempt row staged (the caller advances
/// the watermark only then: the staged row changes the next report).
pub(crate) fn stage_write_feedback(
    session: &mut WorldSession,
    retained: &RetainedCore,
    pid: u32,
    generation: u64,
    cmd_id: u64,
    completion: &AxCompletion,
) -> bool {
    if retained
        .apps
        .get(&pid)
        .is_none_or(|app| app.generation != generation)
    {
        session.pending_writes.remove(&(pid, cmd_id));
        return false;
    }
    let Some((window, target, operation)) = session.pending_writes.remove(&(pid, cmd_id)) else {
        return false;
    };
    let Some(operation) = operation else {
        return false;
    };
    session.pending_frames.insert(
        window.clone(),
        FrameAttempt {
            window: WindowId(window),
            target,
            setter: completion_setter(completion),
            readback: completion_readback(completion),
            dpi_before: None,
            dpi_after: None,
            attempt_ms: 0,
            operation_id: Some(operation),
            prewrite: None,
        },
    );
    true
}

/// Drop every retained mapping row for one terminated PID: homing,
/// minimized lifetimes, and write/effect correlation. Called before the
/// terminate dispatch so closed windows ride as closes, never members.
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
    clear_focus_into(retained, pid);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use tiler_core::directional::WindowId;
    use tiler_core::world_report::{FocusFeedback, VisibilityReason, VisibilitySetter};

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
            setter: VisibilitySetter::NotCalled,
            reason: VisibilityReason::Empty,
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
}
