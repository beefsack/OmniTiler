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
//! - Native Space capability refusals: unsupported sends/trailing/create
//!   refuse adapter-side before any world contact; exactly the armed
//!   select-only subset travels planner-owned (`Select*`), everything
//!   else rides [`WorldCommand::Operation`](tiler_core::world_report::WorldCommand).
//! - Hidden/unreadable scopes never actuate: frames emit only for
//!   currently-actuated domains carrying `Controlled` frame authority with
//!   an effective frame. Holds preserve native state (no writes).
//! - One-shot effects spend before first dispatch (sequencer-owned); the
//!   reply snapshot is plan truth, never a success claim — feedback rows
//!   stage only proven drain outcomes.
//! - Empty native contexts never actuate (same as the KDE null): native
//!   mode with no derived context refuses instead of reporting.
//!
//! Geometry (handover 1 proof): native reports carry the FULL display
//! rectangle as core-owned topology `geometry` plus the RAW visible work
//! area as domain `bounds` with [`PHASE1_OUTER_GAP`] riding separately — the world engine
//! applies its single inset, converging to exactly the legacy pre-inset
//! bounds on real displays (pinned by
//! `native_raw_plus_gap_converges_to_legacy_inset` below; only degenerate
//! sub-margin areas diverge, and those never occur live). The synthetic
//! skeleton reports `domain_outer_gap = 0` with raw [`PHASE1_BOUNDS`],
//! preserving the legacy skeleton's exact edge-to-edge geometry.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::output_selection::MigrationSourceRefill;
use tiler_core::session::{DomainKey, Session};
use tiler_core::size_hints::FixedSizePredicate;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, CommandResult, HoldReason, MinimizedEntry,
    WorldCommand, WorldReport,
};

use crate::drag_drop::map_preview_rejection;
use crate::focus_route::FocusRoute;
use crate::runtime::{NativeContext, ObservedWindow, Shared, lock_shared, window_id};
use crate::runtime_core::{RetainedCore, clear_focus_into};
use crate::runtime_types::{
    AxCompletion, NativeDomain, PHASE1_BOUNDS, PHASE1_GENERATION, PHASE1_OUTER_GAP, PHASE1_OUTPUT,
    PHASE1_OWNER, PHASE1_WORKSPACE, ScopedBackendSnapshot,
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
    /// Staged attempt feedback drained into the next dispatched report
    /// (select-drain outcomes). Drained (cleared) on every dispatch
    /// whether or not the report is accepted — feedback describes
    /// attempted actuation, never retries itself.
    pending_feedback: Vec<AttemptFeedback>,
    /// Frame-write correlation: `(pid, cmd_id)` to the emitted window plus
    /// its explicit placement token, if any. Bounded (oldest evicted).
    pending_writes: BTreeMap<(u32, u64), (String, Option<String>)>,
    /// Explicit float-placement tokens already written once (exactly-once
    /// geometry under `frame_operation`).
    spent_frame_ops: BTreeSet<String>,
    /// True once an observation was refused (stale revision, no context):
    /// retained truth missed input, so the next report publishes
    /// `inventory_complete = false` until a fresh observation lands.
    coverage_gap: bool,
    /// Outstanding trailing creation bindings (see
    /// [`crate::trailing_binding`]): published on every native report
    /// until the core acknowledges the bound row or the native vanishes.
    /// Empty until the trailing drain records an exact-append proof, so
    /// all current reports are unchanged.
    trailing: crate::trailing_binding::TrailingBindingStore,
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
            pending_feedback: Vec::new(),
            pending_writes: BTreeMap::new(),
            spent_frame_ops: BTreeSet::new(),
            coverage_gap: false,
            trailing: crate::trailing_binding::TrailingBindingStore::default(),
        }
    }

    /// Adapter incarnation bound into every report.
    pub(crate) fn incarnation(&self) -> &str {
        &self.incarnation
    }

    /// Stage one attempt-feedback row for the next dispatched report.
    /// Bounded: oldest rows evict past a small cap (well under the core's
    /// feedback cap).
    pub(crate) fn stage_feedback(&mut self, feedback: AttemptFeedback) {
        self.pending_feedback.push(feedback);
        while self.pending_feedback.len() > 64 {
            self.pending_feedback.remove(0);
        }
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

    fn take_feedback(&mut self) -> Vec<AttemptFeedback> {
        std::mem::take(&mut self.pending_feedback)
    }

    /// Per-domain core sessions cloned for one report (handover 2): the
    /// engine-side sessions carry fixed-size admission and retained float
    /// lifetime, so [`collect_app_windows`](crate::runtime_core::collect_app_windows)
    /// normalizes wire flags exactly as the legacy path did.
    fn domain_sessions(&self, keys: &[DomainKey]) -> BTreeMap<DomainKey, Session> {
        let inner = self.engine.engine();
        keys.iter()
            .filter_map(|key| {
                inner
                    .session(key)
                    .map(|session| (key.clone(), session.clone()))
            })
            .collect()
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

    /// Outstanding trailing bindings for the next report. Prunes natives
    /// that left this dispatch's live topology first, so a vanished Space
    /// never reports a stale binding.
    pub(crate) fn trailing_report_bindings(
        &mut self,
        ctx: &NativeContext,
    ) -> Vec<tiler_core::world_report::WorkspaceBinding> {
        use std::collections::BTreeMap;
        let mut live: BTreeMap<OutputId, Vec<String>> = BTreeMap::new();
        for domain in &ctx.domains {
            live.entry(domain.output.clone())
                .or_default()
                .push(domain.workspace.0.clone());
        }
        self.trailing.prune_vanished(&live);
        self.trailing.report_bindings()
    }

    /// Release trailing bindings the core acknowledged (the applied reply
    /// publishes their bound rows). Called once per applied reply; returns
    /// the acknowledged logical ids.
    pub(crate) fn note_trailing_ack(
        &mut self,
        reply: &tiler_core::world_report::WorldReply,
    ) -> Vec<String> {
        self.trailing.note_ack(reply)
    }

    /// Record one exact-append trailing proof against the engine's pending
    /// token (see
    /// [`TrailingBindingStore::bind_exact_append`](crate::trailing_binding::TrailingBindingStore::bind_exact_append)).
    /// Production drains call this through the [`EngineJob::TrailingProof`](crate::runtime_thread::EngineJob::TrailingProof)
    /// lane (capability-disabled production never reaches it: admission
    /// refuses trailing first).
    pub(crate) fn adopt_trailing_binding(
        &mut self,
        logical_id: &str,
        output: &OutputId,
        created_sid: u64,
        before: &[u64],
        after: &[u64],
    ) -> Result<(), String> {
        self.trailing
            .bind_exact_append(logical_id, output, created_sid, before, after)
    }
}

/// Minimized lifetimes for one report, in stable order.
pub(crate) fn minimized_entries(retained: &RetainedCore) -> Vec<MinimizedEntry> {
    let mut entries: Vec<MinimizedEntry> = retained.minimized.values().cloned().collect();
    entries.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    entries
}

/// Refresh retained minimized lifetimes from one scoped snapshot's
/// classifier exclusions: minimized wids gain entries under last-known
/// homing; windows live again (or closed, or homing-less) drop out, and
/// entries whose home domain retired drop too.
///
/// Ordering: call BEFORE [`note_scoped_members`](crate::runtime_core::note_scoped_members)
/// for the same snapshot — the mapping update prunes this PID first, so
/// only the pre-update homing still names the minimized window's home.
pub(crate) fn refresh_minimized(
    retained: &mut RetainedCore,
    pid: u32,
    snapshot: &ScopedBackendSnapshot,
) {
    use crate::classifier::ExcludeReason;
    use std::collections::BTreeSet;
    let live: BTreeSet<String> = snapshot
        .windows
        .iter()
        .map(|w| window_id(pid, w.wid))
        .collect();
    let excluded_minimized: BTreeSet<String> = snapshot
        .excluded_reasons
        .iter()
        .filter(|o| o.reason == ExcludeReason::Minimized)
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
        if omission.reason != ExcludeReason::Minimized {
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
                    MinimizedEntry {
                        window: WindowId(id),
                        output: key.output,
                        workspace: key.workspace,
                        sticky: false,
                    },
                );
            }
            None => {
                // Never homed: unprovable home, omit (never guessed into a
                // domain).
                retained.minimized.remove(&id);
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

/// Per-domain observed focus: the global truth restricted to its homed
/// domain (empty id elsewhere — focus never leaks across domains).
fn domain_focused(retained: &RetainedCore, keys: &[DomainKey]) -> BTreeMap<DomainKey, WindowId> {
    let mut out = BTreeMap::new();
    if let Some(focus) = retained.observed_focus.as_ref()
        && let Some(key) = retained.window_domains.get(&focus.0)
    {
        out.insert(key.clone(), focus.clone());
    }
    for key in keys {
        out.entry(key.clone()).or_insert(WindowId(String::new()));
    }
    out
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
    request_id: &str,
) -> Option<WorldReport> {
    let feedback = session.take_feedback();
    let focused_global = retained
        .observed_focus
        .as_ref()
        .map(|w| w.0.clone())
        .unwrap_or_default();
    match dispatch {
        DispatchContext::Native(ctx) => {
            if ctx.domains.is_empty() {
                return None;
            }
            let keys: Vec<DomainKey> = ctx
                .domains
                .iter()
                .map(|d| DomainKey {
                    output: d.output.clone(),
                    workspace: d.workspace.clone(),
                })
                .collect();
            let sessions = session.domain_sessions(&keys);
            let members = report_members(retained, &keys);
            let focused = domain_focused(retained, &keys);
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
            let staged_bindings = session.trailing_report_bindings(ctx);
            let input = ReportInput {
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
                sessions: &sessions,
                domain_focused: &focused,
                closed: &closed,
                minimized: &minimized_entries(retained),
                unreadable: &unreadable,
                feedback: &feedback,
                focused_global: &focused_global,
                complete: !incomplete,
                incomplete_outputs: &[],
                gesture_barrier: false,
                predicate: session.predicate,
                refill: MigrationSourceRefill::LastRemaining,
                default_tiled: true,
                bindings: &staged_bindings,
                primary_output: None,
                domain_outer_gap: PHASE1_OUTER_GAP,
            };
            Some(build_report(&input))
        }
        DispatchContext::Synthetic => {
            let key = DomainKey {
                output: OutputId(PHASE1_OUTPUT.to_owned()),
                workspace: WorkspaceId(PHASE1_WORKSPACE.to_owned()),
            };
            let sessions = session.domain_sessions(std::slice::from_ref(&key));
            // Synthetic legacy observations home every retained window to
            // the skeleton domain at ingest (see `note_synthetic_homing`),
            // so membership here is exactly the homed set.
            let members = report_members(retained, std::slice::from_ref(&key));
            let focused = domain_focused(retained, std::slice::from_ref(&key));
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
            let input = ReportInput {
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
                sessions: &sessions,
                domain_focused: &focused,
                closed: &closed,
                minimized: &[],
                unreadable: &empty_unreadable,
                feedback: &feedback,
                focused_global: &focused_global,
                complete: true,
                incomplete_outputs: &[],
                gesture_barrier: false,
                predicate: session.predicate,
                refill: MigrationSourceRefill::LastRemaining,
                default_tiled: true,
                // The synthetic skeleton never trails: no bindings invented.
                bindings: &[],
                primary_output: None,
                // Edge-to-edge legacy skeleton parity (the world engine
                // insets by the carried gap; 0 preserves exact geometry).
                domain_outer_gap: 0,
            };
            Some(build_report(&input))
        }
    }
}

/// Read-only drag preview through the world route: one
/// `Operation(DragPreview)` report (never committed, never advancing the
/// revision), mapped to a pure outcome. Consumes request/command sequences
/// (at-most-once); the flight clears without sequencer acceptance so the
/// unchanged revision never self-refuses as stale.
pub(crate) fn preview_world(
    session: &mut WorldSession,
    retained: &RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    window: &str,
    x: i32,
    y: i32,
    hover_prior: Option<tiler_core::session::DragHoverPrior>,
) -> crate::drag_drop::PreviewOutcome {
    use crate::drag_drop::PreviewOutcome;
    use tiler_core::boundary::CoreCommand;
    let native_ctx: Option<Arc<NativeContext>> = lock_shared(shared).native_context.clone();
    let dispatch = match native_ctx {
        Some(ctx) => {
            let Some(key) = retained.window_domains.get(window).cloned() else {
                return PreviewOutcome::CancelDead {
                    reason: "unknown-window",
                };
            };
            if !ctx.is_current(&key) {
                return PreviewOutcome::CancelDead {
                    reason: "hidden-domain",
                };
            }
            DispatchContext::Native(ctx)
        }
        None => DispatchContext::Synthetic,
    };
    let command = CoreCommand::DragPreview {
        window: window.to_owned(),
        x,
        y,
        hover_prior,
        source: None,
    };
    let Some(ticket) = session
        .sequencer
        .try_begin_request(Some(WorldCommand::Operation(command)))
    else {
        session.sequencer().clear_flight();
        return PreviewOutcome::CancelDead {
            reason: "single-flight",
        };
    };
    let report = build_dispatch_report(
        session,
        retained,
        &dispatch,
        BTreeMap::new(),
        ticket.observation_seq,
        ticket.request_seq,
        ticket.command_seq,
        ticket.command.clone(),
        &ticket.request_id,
    );
    // The flight always clears without sequencer acceptance (read-only:
    // the unchanged revision must never self-refuse as stale); the dirty
    // mark drives a fresh observation dispatch on the next job.
    session.sequencer().clear_flight();
    let Some(report) = report else {
        return PreviewOutcome::CancelDead {
            reason: "no-context",
        };
    };
    match session.engine_mut().handle_report(&report) {
        WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
            match reply.command_result {
                Some(CommandResult::DragPreview {
                    rect,
                    hover,
                    target,
                    ..
                }) => PreviewOutcome::Show {
                    rect,
                    hover_prior: hover.map(Box::new),
                    // Core-resolved insertion neighbor (never the mover):
                    // the carrier orders above this window.
                    target,
                },
                _ => PreviewOutcome::CancelDead {
                    reason: "no-preview-result",
                },
            }
        }
        WorldOutcome::Duplicate(cached) => match cached.command_result {
            Some(CommandResult::DragPreview {
                rect,
                hover,
                target,
                ..
            }) => PreviewOutcome::Show {
                rect,
                hover_prior: hover.map(Box::new),
                target,
            },
            _ => PreviewOutcome::CancelDead {
                reason: "no-preview-result",
            },
        },
        WorldOutcome::Rejected { kind, .. } => map_preview_rejection(kind),
    }
}

/// Map one [`AxCompletion`] to its honest attempt outcome: only proven
/// dispatch stages `Applied`; partial writes and timeouts hold as
/// possibly-executed (never blindly repeated); proven failures refuse.
pub(crate) fn completion_outcome(completion: &AxCompletion) -> AttemptOutcome {
    match completion {
        AxCompletion::Applied { .. } => AttemptOutcome::Applied,
        AxCompletion::PartialApplied { .. } => AttemptOutcome::PossiblyExecuted,
        AxCompletion::Failed { .. } => AttemptOutcome::RefusedProven,
        AxCompletion::Timeout { .. } => AttemptOutcome::PossiblyExecuted,
    }
}

/// Track one emitted frame write for proven-outcome feedback: ONLY rows
/// carrying an explicit placement token (`frame_operation`) are recorded —
/// ordinary frame traffic stages nothing core-side (it rides the
/// completions counters), so it must never occupy correlation space nor
/// evict a pending placement entry. Bounded (oldest placement evicted).
pub(crate) fn track_write(
    session: &mut WorldSession,
    wpid: u32,
    cmd_id: u64,
    window: &str,
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
        (window.to_owned(), Some(operation.to_owned())),
    );
}

/// Stage proven frame-write feedback for explicit placement tokens:
/// only rows carrying a `frame_operation` settle anything core-side, and
/// only with the recorded outcome (never false success). Ordinary frame
/// writes stay on the completions counters and logs. Consumes the
/// correlation entry (one completion per command). Returns whether a
/// feedback row staged (the caller advances the watermark only then: the
/// staged row changes the next report).
pub(crate) fn stage_write_feedback(
    session: &mut WorldSession,
    pid: u32,
    cmd_id: u64,
    completion: &AxCompletion,
) -> bool {
    let Some((_, operation)) = session.pending_writes.remove(&(pid, cmd_id)) else {
        return false;
    };
    let Some(operation) = operation else {
        return false;
    };
    session.stage_feedback(AttemptFeedback {
        operation_id: operation,
        kind: AttemptKind::Property,
        outcome: completion_outcome(completion),
        hold: None,
    });
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
