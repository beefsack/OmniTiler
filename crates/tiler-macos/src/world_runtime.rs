#![forbid(unsafe_code)]

//! World-state runtime session (Unit B adoption).
//!
//! Engine-owner half of the macOS world route: one retained
//! [`WorldEngine`](tiler_core::world_engine::WorldEngine) plus the
//! adapter-side [`WorldSequencer`](crate::world_sequencer::WorldSequencer)
//! replace the legacy per-domain `Engine.handle` / `CoreReply`
//! aggregation and cursors. Retired paths are deleted, not dual-run: every
//! observation and every command converges through one complete
//! [`WorldReport`](tiler_core::world_report::WorldReport) and actuates
//! from one complete
//! [`WorldReply`](tiler_core::world_report::WorldReply).
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
//! area with [`PHASE1_OUTER_GAP`] separately; core applies the inset once.
//! The synthetic skeleton carries zero outer gap and raw [`PHASE1_BOUNDS`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::gesture::GestureIngress;
use tiler_core::output_selection::MigrationSourceRefill;
use tiler_core::session::DomainKey;
use tiler_core::size_hints::FixedSizePredicate;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{
    FrameAttempt, FrameSetter, HoldReason, MemberAttempt, ViewAttempt, WorldCommand, WorldReport,
};

use crate::focus_route::FocusRoute;
use crate::runtime::{NativeContext, ObservedWindow, window_id};
use crate::runtime_core::{LifetimeMemory, MinimizedRecord, RetainedCore, clear_focus_into};
use crate::runtime_types::{
    AxCompletion, NativeDomain, PHASE1_BOUNDS, PHASE1_GAP, PHASE1_GENERATION, PHASE1_OUTER_GAP,
    PHASE1_OUTPUT, PHASE1_OWNER, PHASE1_WORKSPACE, ScopedBackendSnapshot,
};
use crate::world_builder::{ReportInput, build_report};
use crate::world_sequencer::{ObservationClock, WorldSequencer};

/// Adapter incarnation for this engine lifetime: stable within the process
/// (one engine owner per runtime), so the core never resets ordering
/// mid-lifetime. A fresh engine (restart) re-derives the same token, which
/// is safe: reports never cross processes.
fn adapter_incarnation() -> String {
    format!("macos-{}", std::process::id())
}

/// Engine-thread-owned world session: the retained [`WorldEngine`], the
/// adapter-side [`WorldSequencer`], pending attempt feedback, and the
/// completion-correlation maps. None of this is shared: submitters reach
/// the engine only through inbox jobs.
pub(crate) struct WorldSession {
    engine: WorldEngine,
    sequencer: WorldSequencer,
    incarnation: String,
    predicate: FixedSizePredicate,
    native_mode: bool,
    focus_route: FocusRoute,
    /// Staged raw view attempts drained into the next dispatched report,
    /// latest-wins per owning operation. Drained (cleared) on every
    /// dispatch whether or not the report is accepted — attempts describe
    /// attempted actuation, never retry themselves.
    pub(crate) pending_views: BTreeMap<String, ViewAttempt>,
    /// Staged raw membership attempts, latest-wins per (operation, window).
    pub(crate) pending_members: BTreeMap<(String, String), MemberAttempt>,
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
    /// Adapter-asserted automatic-float lifetimes: windows the adapter
    /// reported `fixed_auto` for (first-sight fixed asserts, re-asserted
    /// while retained). Pruned to core-floating rows on every dispatch;
    /// the builder inserts newly asserted lifetimes. Never inferred, never
    /// a durable binding — only the adapter's own assertion echo.
    pub(crate) auto_asserted: BTreeSet<String>,
    /// True once an observation was refused (stale revision, no context):
    /// retained truth missed input, so the next report publishes
    /// `inventory_complete = false` until a fresh observation lands.
    coverage_gap: bool,
}

impl WorldSession {
    pub(crate) fn new(
        predicate: FixedSizePredicate,
        native_mode: bool,
        route: FocusRoute,
        clock: Arc<ObservationClock>,
    ) -> Self {
        let incarnation = adapter_incarnation();
        // `phase-1` is a valid generation token; the maximized-focus fence
        // the constructor enables stays inert because the builder reports
        // `maximized: false` unconditionally (no native inference). The
        // watermark clock is shared with every invalidating input
        // submitter (see `Shared::clock`): only submitted input moves it,
        // never a dispatch, so tickets stamp actually-submitted input and
        // mid-flight arrivals refuse as stale.
        let engine =
            WorldEngine::new(PHASE1_GENERATION).expect("phase-1 generation token is valid");
        let sequencer = WorldSequencer::with_clock(&incarnation, PHASE1_GENERATION, clock);
        Self {
            engine,
            sequencer,
            incarnation,
            predicate,
            native_mode,
            focus_route: route,
            pending_views: BTreeMap::new(),
            pending_members: BTreeMap::new(),
            pending_frames: BTreeMap::new(),
            pending_writes: BTreeMap::new(),
            spent_frame_ops: BTreeSet::new(),
            auto_asserted: BTreeSet::new(),
            coverage_gap: false,
        }
    }

    /// Adapter incarnation bound into every report.
    pub(crate) fn incarnation(&self) -> &str {
        &self.incarnation
    }

    /// Stage one raw view-attempt row for the next dispatched report.
    /// Resolves the retained transfer target by operation identity (logical
    /// target correlation only, never adapter mapping). Native Space reads
    /// ride as absence (`pre`/`post` are `None`): host-owned facts never
    /// translate native ids to logical receipts. A native pre/post schema
    /// is not represented by the current logical attempt fields; the adapter
    /// reports no observed pre/post rather than forging logical values.
    /// Unknown operations drop without evidence (never forged, never
    /// guessed). Latest-wins per owning operation.
    pub(crate) fn stage_view_attempt(
        &mut self,
        operation_id: &str,
        setter: FrameSetter,
        _pre_sid: Option<u64>,
        _post_sid: Option<u64>,
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
        let attempt = ViewAttempt {
            operation_id: operation_id.to_owned(),
            target_output,
            target_workspace,
            setter,
            pre: None,
            post: None,
            baseline: None,
            attempt_ms: 0,
        };
        self.pending_views.insert(operation_id.to_owned(), attempt);
    }

    /// Stage one raw membership-attempt row for the next dispatched report.
    /// Same target-correlation/drop rules as
    /// [`WorldSession::stage_view_attempt`]; the retained transfer must be
    /// a window transfer (its subject window and target ride the staged
    /// row — the operation identity already names them core-side, so the
    /// drain never re-states them). Native Space reads ride as absence
    /// (`post` is `None`, same logical-field limitation as above).
    /// Latest-wins per (operation, window).
    pub(crate) fn stage_member_attempt(
        &mut self,
        operation_id: &str,
        setter: FrameSetter,
        _post_sid: Option<u64>,
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
        let attempt = MemberAttempt {
            operation_id: operation_id.to_owned(),
            window: window.clone(),
            target_output,
            target_workspace,
            setter,
            pre: None,
            post: None,
            attempt_ms: 0,
        };
        self.pending_members
            .insert((operation_id.to_owned(), window.0.clone()), attempt);
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

    fn take_frame_attempts(&mut self) -> Vec<FrameAttempt> {
        std::mem::take(&mut self.pending_frames)
            .into_values()
            .collect()
    }

    /// Adapter-side core-delivery lifetime memory for wire-flag
    /// normalization: the last applied reply's rows plus the
    /// native-to-logical home map (a live reply lookup, never a durable
    /// host table). Empty before the first applied reply (pre-adoption
    /// first-sight, matching the old empty-session read).
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

/// Whether a classifier exclusion mints a retained minimized lifetime
/// (option B): genuine `Minimized` plus owner-hidden `HiddenApp` (Cmd+H).
/// Both report as explicit `minimized: true` facts through the existing
/// retained-lifetime path; the true native cause stays in diagnostics via
/// `ExcludeReason::log_token` (`classifier-omitted-minimized` vs
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
        match retained.window_domains.get(&id).cloned() {
            Some(key) => {
                retained.minimized.insert(
                    id.clone(),
                    MinimizedRecord {
                        window: WindowId(id),
                        output: key.output,
                        workspace: key.workspace,
                        sticky: false,
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

/// Per-domain explicit closes partitioned from window ids through current
/// homing (unknown ids omit: an unhomed close proves nothing).
pub(crate) fn partition_closes(
    retained: &RetainedCore,
    closed: &[WindowId],
) -> BTreeMap<DomainKey, Vec<WindowId>> {
    let mut out: BTreeMap<DomainKey, Vec<WindowId>> = BTreeMap::new();
    for id in closed {
        if let Some(key) = retained.window_domains.get(&id.0) {
            out.entry(key.clone()).or_default().push(id.clone());
        }
    }
    out
}

/// Explicit closes for one terminating PID: member closes partitioned
/// through live homing plus minimized closes through their entry homing.
/// Minimized windows ride no membership, so without this their slots
/// would leak core-side. Call BEFORE forgetting homing/minimized state.
pub(crate) fn terminate_close_map(
    retained: &RetainedCore,
    pid: u32,
    closed: &[WindowId],
) -> BTreeMap<DomainKey, Vec<WindowId>> {
    let mut map = partition_closes(retained, closed);
    let prefix = format!("{pid}_");
    for entry in retained
        .minimized
        .iter()
        .filter(|(id, _)| id.starts_with(&prefix))
        .map(|(_, entry)| entry.clone())
    {
        map.entry(DomainKey {
            output: entry.output.clone(),
            workspace: entry.workspace.clone(),
        })
        .or_default()
        .push(entry.window.clone());
    }
    map
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
    closed: BTreeMap<DomainKey, Vec<WindowId>>,
    ticket_observation: u64,
    ticket_request: u64,
    ticket_command_seq: Option<u64>,
    ticket_command: Option<WorldCommand>,
    gesture: Option<GestureIngress>,
    request_id: &str,
) -> Option<WorldReport> {
    // No-context guard BEFORE any drain: an empty native context reports
    // nothing (KDE null parity), and the staged rows must survive for the
    // next real dispatch — attempts describe attempted actuation and the
    // assertion set names asserted lifetimes, and neither may be dropped
    // by a report that never runs.
    if let DispatchContext::Native(ctx) = dispatch
        && ctx.domains.is_empty()
    {
        return None;
    }
    let view_rows = session.take_view_attempts();
    let member_rows = session.take_member_attempts();
    let frame_rows = session.take_frame_attempts();
    let memory = session.lifetime_memory();
    // Prune automatic-float assertions to lifetimes the core still floats
    // (un-floated lifetimes drop the assertion with the exception); the
    // builder inserts newly asserted lifetimes during collection.
    session
        .auto_asserted
        .retain(|id| memory.rows.get(id).is_some_and(|row| row.floating));
    let mut auto_asserted = std::mem::take(&mut session.auto_asserted);
    let focused_global = retained
        .observed_focus
        .as_ref()
        .map(|w| w.0.clone())
        .unwrap_or_default();
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
            // Explicit closes arrive pre-partitioned by the caller
            // (terminate homes member closes through live homing plus
            // minimized closes through their entry homing, before
            // forgetting either — never members, never dropped).
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
            let mut input = ReportInput {
                owner: PHASE1_OWNER,
                adapter_incarnation: session.incarnation(),
                core_generation: PHASE1_GENERATION,
                observation_seq: ticket_observation,
                request_seq: ticket_request,
                request_id: &owned_request,
                command_seq: ticket_command_seq,
                command: owned_command,
                ctx,
                members: &members,
                memory: &memory,
                auto_asserted: &mut auto_asserted,
                closed: &closed,
                minimized: &minimized_entries(retained),
                unreadable: &unreadable,
                frames: &frame_rows,
                member_attempts: &member_rows,
                view_attempts: &view_rows,
                clear_attempts: &[],
                creation_attempts: &[],
                gesture,
                focused_global: &focused_global,
                complete: !incomplete,
                coverage_gap: session.coverage_gap,
                incomplete_outputs: &[],
                gesture_barrier: false,
                predicate: session.predicate,
                refill: MigrationSourceRefill::LastRemaining,
                default_tiled: true,
                primary_output: None,
                gap: PHASE1_GAP,
                outer_gap: PHASE1_OUTER_GAP,
                monotonic_ms: 0,
            };
            let report = build_report(&mut input);
            session.auto_asserted = std::mem::take(&mut auto_asserted);
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
            let mut input = ReportInput {
                owner: PHASE1_OWNER,
                adapter_incarnation: session.incarnation(),
                core_generation: PHASE1_GENERATION,
                observation_seq: ticket_observation,
                request_seq: ticket_request,
                request_id: &owned_request,
                command_seq: ticket_command_seq,
                command: owned_command,
                ctx: &ctx,
                members: &members,
                memory: &memory,
                auto_asserted: &mut auto_asserted,
                closed: &closed,
                minimized: &[],
                unreadable: &empty_unreadable,
                frames: &frame_rows,
                member_attempts: &member_rows,
                view_attempts: &view_rows,
                clear_attempts: &[],
                creation_attempts: &[],
                gesture,
                focused_global: &focused_global,
                complete: true,
                coverage_gap: session.coverage_gap,
                incomplete_outputs: &[],
                gesture_barrier: false,
                predicate: session.predicate,
                refill: MigrationSourceRefill::LastRemaining,
                default_tiled: true,
                primary_output: None,
                // Edge-to-edge legacy skeleton parity (the world engine
                // insets by the carried gap; 0 preserves exact geometry).
                gap: PHASE1_GAP,
                outer_gap: 0,
                monotonic_ms: 0,
            };
            let report = build_report(&mut input);
            session.auto_asserted = std::mem::take(&mut auto_asserted);
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
    reply: &tiler_core::world_report::WorldReply,
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
