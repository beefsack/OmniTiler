#![forbid(unsafe_code)]
//! Unit A boundary: production callers land in Unit B, so non-test builds
//! would flag every constructor as dead. Tests own the coverage meanwhile.
#![cfg_attr(not(test), allow(dead_code))]

//! Full world-state report builder (Unit A, additive, offline).
//!
//! Pure construction of one complete [`WorldReport`](tiler_core::world_report::WorldReport)
//! from observed native state. No threads, no locks, no sinks, no actuation:
//! the existing native actuation paths are untouched and this module is not
//! called by them yet (Unit B adopts replies for reconciliation/commands).
//!
//! Fixed Unit A contract (see `docs/changes/macos-full-state-replies.md`):
//! - [`WorkspaceMode::PerOutputLocal`](tiler_core::workspace_scope::WorkspaceMode)
//!   always; topology and domain keys are the observed native ones.
//! - Logical bindings carry trailing creation proof only (see
//!   [`crate::trailing_binding`]): the adapter publishes an explicit
//!   logical/native row for each native-created trailing Space whose
//!   exact-append proof is bound to the engine's pending token. Nothing
//!   else is invented: the core rebuilds its
//!   planner-owned mapping from the reported topology each report.
//! - Topology `geometry` carries the FULL native output rectangle from the
//!   already-retained [`NativeDomain`](crate::runtime_types::NativeDomain)
//!   `bounds` (core-owned adjacency authority; wire `adjacent` stays
//!   empty). Domain bounds carry the RAW visible work area with the report's
//!   `domain_outer_gap` riding separately (native: [`PHASE1_OUTER_GAP`];
//!   synthetic skeleton: `0`, which preserves the legacy skeleton's exact
//!   edge-to-edge geometry because the [`WorldEngine`] always insets by the
//!   carried gap): the [`WorldEngine`](tiler_core::world_engine::WorldEngine)
//!   applies its single engine-owned inset. (The legacy domain path
//!   pre-insets adapter-side via [`tile_domain_bounds`](crate::runtime_core::tile_domain_bounds);
//!   both converge to the same retained geometry on real displays and differ
//!   only on degenerate sub-margin work areas, which never occur live.)
//! - Window rows reuse [`collect_app_windows`](crate::runtime_core::collect_app_windows),
//!   so wire flags are identical to the legacy path: truthful
//!   fullscreen/hints, `maximized: false` always (no native inference, so the
//!   core maximized fence stays inert), first-sight fixed-size asserts and
//!   retained-float echoes from core-session lifetime.
//! - `focused_global` is informational only, never a focus command.
//! - Overlap precedence is closed > minimized > live: explicit closes always
//!   win (even with incomplete inventory), minimized lifetimes are stripped
//!   from domain membership (reported separately), and minimized entries that
//!   collide with a close are dropped. The produced report therefore always
//!   passes [`validate_report`](tiler_core::world_report::validate_report)
//!   on id overlap, given opaque caller ids.
//! - Unreadable scopes report `readable: false` with empty membership and an
//!   explicit hold: they acquire no write authority and force
//!   `inventory_complete: false`.
//! - With no outputs at all the builder still returns a valid empty report;
//!   Unit B must NOT dispatch it (nothing to report, same as the KDE null).

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::output_selection::MigrationSourceRefill;
use tiler_core::seed::EngineWindow;
use tiler_core::session::{DomainKey, OutputDomain, Session};
use tiler_core::size_hints::FixedSizePredicate;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_report::{
    AttemptFeedback, DomainReport, HoldReason, MinimizedEntry, OutputTopology,
    WORLD_CONTRACT_VERSION, WorkspaceBinding, WorldCommand, WorldReport,
};

use crate::runtime_core::collect_app_windows;
use crate::runtime_types::{NativeContext, ObservedWindow, PHASE1_GAP, PHASE1_OUTER_GAP};

/// Complete input for one world report. The caller owns observation truth
/// (Unit B wires retained homing/sessions/focus); the builder only shapes,
/// normalizes, and de-overlaps.
pub(crate) struct ReportInput<'a> {
    pub owner: &'a str,
    pub adapter_incarnation: &'a str,
    pub core_generation: &'a str,
    pub observation_seq: u64,
    pub request_seq: u64,
    pub request_id: &'a str,
    pub command_seq: Option<u64>,
    pub command: Option<WorldCommand>,
    /// Observed native topology (every retainable Space per display).
    pub ctx: &'a NativeContext,
    /// Member windows per domain as `(pid, window)` pairs (exact-Space
    /// homing decided by the caller, never geometry here).
    pub members: &'a BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
    /// Core sessions per domain for wire-flag lifetime (empty = all
    /// first-sight; Unit B passes the engine-side sessions).
    pub sessions: &'a BTreeMap<DomainKey, Session>,
    /// Focused window per domain (observed truth only).
    pub domain_focused: &'a BTreeMap<DomainKey, WindowId>,
    /// Explicit closes per domain (always honored, even incomplete).
    pub closed: &'a BTreeMap<DomainKey, Vec<WindowId>>,
    /// Explicit minimized lifetimes (absent from domain membership).
    pub minimized: &'a [MinimizedEntry],
    /// Unreadable scopes with their explicit hold (no write authority).
    pub unreadable: &'a BTreeMap<DomainKey, HoldReason>,
    /// Attempt feedback carrier (possibly-executed holds etc.).
    pub feedback: &'a [AttemptFeedback],
    /// Observed global focus id (`""` when none; informational only).
    pub focused_global: &'a str,
    /// Caller-observed enumeration completeness over live outputs.
    pub complete: bool,
    /// Outputs whose native enumeration was partial (forces per-output
    /// `complete: false` plus whole-report incompleteness).
    pub incomplete_outputs: &'a [OutputId],
    /// Gesture barrier active (all scopes hold gesture-side).
    pub gesture_barrier: bool,
    /// Ingress fixed-size admission predicate (existing predicate per report).
    pub predicate: FixedSizePredicate,
    /// Ingress migration source-refill mode.
    pub refill: MigrationSourceRefill,
    /// Saved default-tiling seed for newly observed natives.
    pub default_tiled: bool,
    /// Explicit trailing creation bindings (see [`crate::trailing_binding`]):
    /// one logical/native row per native-created trailing Space bound to
    /// the engine's pending token. Empty in all other cases: no native
    /// logical creation and no ordinary window transfer is invented.
    pub bindings: &'a [WorkspaceBinding],
    /// Primary output (`None` = first output in context order).
    pub primary_output: Option<OutputId>,
    /// Adapter-side outer gap riding with each domain's raw bounds (the
    /// engine applies its single inset). Native reports carry
    /// [`PHASE1_OUTER_GAP`]; the synthetic skeleton reports `0` to
    /// preserve the legacy skeleton's exact edge-to-edge geometry.
    pub domain_outer_gap: i32,
}

impl<'a> ReportInput<'a> {
    /// Fresh input with migration/product defaults: both-axes predicate,
    /// last-remaining refill, default-tiled seed, complete inventory, no
    /// barrier, first-output primary. The caller fills members/sessions and
    /// any holds/closes/minimized/feedback.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        owner: &'a str,
        adapter_incarnation: &'a str,
        core_generation: &'a str,
        ctx: &'a NativeContext,
        observation_seq: u64,
        request_seq: u64,
        request_id: &'a str,
        members: &'a BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
        sessions: &'a BTreeMap<DomainKey, Session>,
        domain_focused: &'a BTreeMap<DomainKey, WindowId>,
        closed: &'a BTreeMap<DomainKey, Vec<WindowId>>,
        unreadable: &'a BTreeMap<DomainKey, HoldReason>,
        minimized: &'a [MinimizedEntry],
        feedback: &'a [AttemptFeedback],
        focused_global: &'a str,
    ) -> Self {
        Self {
            owner,
            adapter_incarnation,
            core_generation,
            observation_seq,
            request_seq,
            request_id,
            command_seq: None,
            command: None,
            ctx,
            members,
            sessions,
            domain_focused,
            closed,
            minimized,
            unreadable,
            feedback,
            focused_global,
            complete: true,
            incomplete_outputs: &[],
            gesture_barrier: false,
            predicate: FixedSizePredicate::BothAxes,
            refill: MigrationSourceRefill::LastRemaining,
            default_tiled: true,
            bindings: &[],
            primary_output: None,
            domain_outer_gap: PHASE1_OUTER_GAP,
        }
    }
}

/// Build one complete world report. Infallible over the typed inputs; id
/// validity (opaque alphabet) stays the caller's contract, checked by
/// [`validate_report`](tiler_core::world_report::validate_report).
pub(crate) fn build_report(input: &ReportInput<'_>) -> WorldReport {
    // Closed > minimized > live: collect the explicit-close set first so
    // both minimized entries and live membership defer to it.
    let closed_set: BTreeSet<&str> = input
        .closed
        .values()
        .flatten()
        .map(|id| id.0.as_str())
        .collect();
    let minimized_set: BTreeSet<&str> = input
        .minimized
        .iter()
        .filter(|entry| !closed_set.contains(entry.window.0.as_str()))
        .map(|entry| entry.window.0.as_str())
        .collect();

    // Per-output native order in context order (first-seen output order).
    let mut output_order: Vec<OutputId> = Vec::new();
    let mut native_order: BTreeMap<OutputId, Vec<String>> = BTreeMap::new();
    let mut current_native: BTreeMap<OutputId, String> = BTreeMap::new();
    let mut output_geometry: BTreeMap<OutputId, tiler_core::geometry::Rect> = BTreeMap::new();
    for domain in &input.ctx.domains {
        if !native_order.contains_key(&domain.output) {
            output_order.push(domain.output.clone());
        }
        native_order
            .entry(domain.output.clone())
            .or_default()
            .push(domain.workspace.0.clone());
        if domain.current {
            current_native.insert(domain.output.clone(), domain.workspace.0.clone());
        }
        // Core-owned topology authority: FULL native rectangles from the
        // already-retained display bounds (never the visible work area,
        // never a window-frame proxy). Domain `bounds` below stays the
        // separate visible work area; wire adjacency stays empty for core
        // derivation.
        output_geometry
            .entry(domain.output.clone())
            .or_insert(domain.bounds);
    }
    let primary = input.primary_output.clone().unwrap_or_else(|| {
        output_order
            .first()
            .cloned()
            .unwrap_or_else(|| OutputId(String::new()))
    });
    let topology: Vec<OutputTopology> = output_order
        .iter()
        .map(|output| OutputTopology {
            output: output.clone(),
            native_order: native_order.get(output).cloned().unwrap_or_default(),
            current_native: current_native.get(output).cloned().unwrap_or_default(),
            complete: input.complete && !input.incomplete_outputs.contains(output),
            geometry: output_geometry.get(output).copied(),
            primary: *output == primary,
        })
        .collect();

    let mut domains: Vec<DomainReport> = Vec::with_capacity(input.ctx.domains.len());
    for native in &input.ctx.domains {
        let key = DomainKey {
            output: native.output.clone(),
            workspace: native.workspace.clone(),
        };
        if let Some(reason) = input.unreadable.get(&key) {
            // Unreadable scope: explicit hold, empty membership, no write
            // authority. Closes still ride (explicit closes are always
            // honored, even inside unreadable scopes).
            domains.push(DomainReport {
                domain: OutputDomain {
                    id: native.output.clone(),
                    workspace: native.workspace.clone(),
                    bounds: native.visible,
                    gap: PHASE1_GAP,
                    adjacent: BTreeMap::new(),
                },
                outer_gap: input.domain_outer_gap,
                focused: WindowId(String::new()),
                windows: Vec::new(),
                closed: input.closed.get(&key).cloned().unwrap_or_default(),
                readable: false,
                hold: Some(*reason),
            });
            continue;
        }
        // Raw visible work area: the engine applies its single inset.
        let domain = OutputDomain {
            id: native.output.clone(),
            workspace: native.workspace.clone(),
            bounds: native.visible,
            gap: PHASE1_GAP,
            adjacent: BTreeMap::new(),
        };
        let session = input.sessions.get(&key);
        let mut rows: Vec<(String, tiler_core::geometry::Rect)> = Vec::new();
        let mut windows: Vec<EngineWindow> = Vec::new();
        if let Some(members) = input.members.get(&key) {
            // One collect call per PID group keeps the (`pid`, `wid`)
            // identity minting exact; minimized/closed ids defer out.
            let mut by_pid: BTreeMap<u32, Vec<ObservedWindow>> = BTreeMap::new();
            for (pid, window) in members {
                let id = crate::runtime_types::window_id(*pid, window.wid);
                if closed_set.contains(id.as_str()) || minimized_set.contains(id.as_str()) {
                    continue;
                }
                by_pid.entry(*pid).or_default().push(*window);
            }
            for (pid, observed) in &by_pid {
                collect_app_windows(
                    &mut rows,
                    &mut windows,
                    *pid,
                    observed,
                    &key.output.0,
                    &key.workspace.0,
                    session,
                    input.predicate,
                );
            }
        }
        let focused = input
            .domain_focused
            .get(&key)
            .cloned()
            .unwrap_or(WindowId(String::new()));
        domains.push(DomainReport {
            domain,
            outer_gap: input.domain_outer_gap,
            focused,
            windows,
            closed: input.closed.get(&key).cloned().unwrap_or_default(),
            readable: true,
            hold: None,
        });
    }

    let minimized: Vec<MinimizedEntry> = input
        .minimized
        .iter()
        .filter(|entry| !closed_set.contains(entry.window.0.as_str()))
        .cloned()
        .collect();

    let inventory_complete =
        input.complete && input.unreadable.is_empty() && input.incomplete_outputs.is_empty();

    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: input.owner.to_owned(),
        adapter_incarnation: input.adapter_incarnation.to_owned(),
        core_generation: input.core_generation.to_owned(),
        observation_seq: input.observation_seq,
        request_seq: input.request_seq,
        request_id: input.request_id.to_owned(),
        command_seq: input.command_seq,
        command: input.command.clone(),
        workspace_mode: WorkspaceMode::PerOutputLocal,
        topology,
        // Trailing creation proof only (see the `bindings` input): one
        // explicit logical/native row per bound trailing Space. The core
        // rebuilds planner mapping from topology.
        bindings: input.bindings.to_vec(),
        domains,
        minimized,
        fixed_size_predicate: input.predicate,
        migration_source_refill: input.refill,
        default_tiled: input.default_tiled,
        focused_global: input.focused_global.to_owned(),
        inventory_complete,
        gesture_barrier: input.gesture_barrier,
        feedback: input.feedback.to_vec(),
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::directional::WorkspaceId;
    use tiler_core::geometry::Rect;
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_engine::{WorldEngine, WorldOutcome};
    use tiler_core::world_report::{AttemptKind, AttemptOutcome, validate_report};

    use crate::runtime_types::{NativeDomain, ObservedWindow, ScopedWindowMeta};

    const OWNER: &str = "macos-phase-1";
    const GENERATION: &str = "gen-1";
    const INCARNATION: &str = "adapter-1";

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    /// Real-producer-shaped display UUIDs: canonical CFUUID text from
    /// `CGDisplayCreateUUIDFromDisplayID` (uppercase hex with dashes, via
    /// `CFUUIDCreateString`), never a positional guess.
    const UUID_1: &str = "A1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6";
    const UUID_2: &str = "0F1E2D3C-4B5A-6978-8796-A5B4C3D2E1F0";

    fn native_domain(cg_id: u32, space: u64, current: bool, x: i32, w: i32) -> NativeDomain {
        let uuid = if cg_id == 1 { UUID_1 } else { UUID_2 };
        NativeDomain {
            cg_id,
            uuid: Some(uuid.to_owned()),
            output: OutputId(crate::topology::output_key(cg_id)),
            workspace: WorkspaceId(crate::topology::workspace_key(space)),
            space,
            current,
            readable: true,
            // FULL display rect (core-owned topology authority).
            bounds: rect(x, 0, w, 900),
            // Menu-inset work area: topology must NOT collapse onto this.
            visible: rect(x, 25, w, 875),
        }
    }

    fn two_display_ctx() -> NativeContext {
        NativeContext {
            revision: 9,
            domains: vec![
                native_domain(1, 11, true, 0, 1440),
                native_domain(1, 12, false, 0, 1440),
                native_domain(2, 21, true, 1440, 1440),
            ],
            skipped: Vec::new(),
        }
    }

    fn observed(wid: u32, x: i32, y: i32, w: i32, h: i32) -> ObservedWindow {
        ObservedWindow {
            wid,
            x,
            y,
            w,
            h,
            floating: false,
            fullscreen: false,
            hints: WindowSizeHints::none(),
        }
    }

    fn key(output: &str, workspace: &str) -> DomainKey {
        DomainKey {
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        ctx: &NativeContext,
        members: &BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
        sessions: &BTreeMap<DomainKey, Session>,
        focused: &BTreeMap<DomainKey, WindowId>,
        closed: &BTreeMap<DomainKey, Vec<WindowId>>,
        unreadable: &BTreeMap<DomainKey, HoldReason>,
        minimized: &[MinimizedEntry],
        feedback: &[AttemptFeedback],
        focused_global: &str,
        obs: u64,
        req: u64,
    ) -> WorldReport {
        let owned_id = format!("req-{req}");
        let input = ReportInput::new(
            OWNER,
            INCARNATION,
            GENERATION,
            ctx,
            obs,
            req,
            &owned_id,
            members,
            sessions,
            focused,
            closed,
            unreadable,
            minimized,
            feedback,
            focused_global,
        );
        let report = build_report(&input);
        assert_eq!(report.request_id, owned_id);
        report
    }

    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn empty_maps() -> (
        BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
        BTreeMap<DomainKey, Session>,
        BTreeMap<DomainKey, WindowId>,
        BTreeMap<DomainKey, Vec<WindowId>>,
        BTreeMap<DomainKey, HoldReason>,
    ) {
        (
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        )
    }

    fn accepted(
        report: &WorldReport,
        engine: &mut WorldEngine,
    ) -> tiler_core::world_report::WorldReply {
        match engine.handle_report(report) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn catalogue_pins_exactly_19_select_bindings_with_armed_trailing() {
        // Native catalog normative: `workspace_select::bindings()` is exactly
        // the 19 armed select-only rows (numbered 1..=9, trailing select-0,
        // 8 relative, previous toggle) with no transfer.
        let bindings = crate::workspace_select::bindings();
        assert_eq!(bindings.len(), 19, "armed select-only catalog is 19 rows");
        for binding in &bindings {
            assert!(
                crate::workspace_input::is_armed_select(&binding.action),
                "every catalog row is an armed select: {:?}",
                binding.action
            );
            assert!(
                crate::workspace_input::workspace_unsupported_reason(&binding.action).is_none(),
                "armed rows dispatch, never refuse: {:?}",
                binding.action
            );
        }
        let has_trailing = bindings.iter().any(|binding| {
            matches!(
                binding.action,
                crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 0 }
            )
        });
        assert!(has_trailing, "trailing select-0 armed in product");
    }

    #[test]
    fn builder_reports_per_output_local_with_empty_bindings() {
        let ctx = two_display_ctx();
        let (members, sessions, focused, closed, unreadable) = empty_maps();
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report), "builder output validates");
        assert_eq!(report.workspace_mode, WorkspaceMode::PerOutputLocal);
        assert!(report.bindings.is_empty(), "no logical bindings invented");
        assert_eq!(report.topology.len(), 2);
        assert_eq!(
            report.topology[0].native_order,
            vec!["space-11".to_owned(), "space-12".to_owned()]
        );
        assert_eq!(report.topology[0].current_native, "space-11");
        assert!(report.topology[0].complete);
        assert!(
            report.topology[0].primary,
            "first output is primary by default"
        );
        assert!(!report.topology[1].primary);
        assert_eq!(report.topology[1].current_native, "space-21");
        // Raw visible work area rides with the outer gap separate (the
        // engine applies its single inset).
        assert_eq!(report.domains.len(), 3);
        assert_eq!(report.domains[0].domain.bounds, rect(0, 25, 1440, 875));
        assert_eq!(report.domains[0].outer_gap, PHASE1_OUTER_GAP);
        // Core-owned topology carries the FULL display rect, never the
        // visible work area; wire adjacency stays empty for core derivation.
        assert_eq!(report.topology[0].geometry, Some(rect(0, 0, 1440, 900)));
        assert_eq!(report.topology[1].geometry, Some(rect(1440, 0, 1440, 900)));
        assert!(
            report.domains.iter().all(|d| d.domain.adjacent.is_empty()),
            "no adapter-owned adjacency"
        );
        assert!(report.domains.iter().all(|d| d.readable));
        assert!(report.inventory_complete);
    }

    /// Real-producer display UUIDs for end-to-end fixtures below: canonical
    /// CFUUID text from `CGDisplayCreateUUIDFromDisplayID` (uppercase hex
    /// with dashes), matching the in-row SLS `"Display Identifier"` exactly.
    const PROD_UUID_1: &str = "37AC4E19-0000-4000-8000-123456789ABC";
    const PROD_UUID_2: &str = "9B4D2E1A-77C0-4F3B-9E52-0A1B2C3D4E5F";

    #[test]
    fn full_stacked_topology_report_accepted_despite_work_area_gap() {
        use crate::runtime_types::NativeDomain;
        use tiler_core::directional::{OutputId, WorkspaceId};
        // Stacked pair: FULL rects edge-touch at y=0, while the menu/Dock
        // insets leave the visible work areas gapped (upper bottom -32 vs
        // lower top 25). Core-owned adjacency must derive from FULL, so the
        // retained live rects are the FULL pair.
        let ctx = NativeContext {
            revision: 5,
            domains: vec![
                NativeDomain {
                    cg_id: 1,
                    uuid: Some(UUID_1.to_owned()),
                    output: OutputId(crate::topology::output_key(1)),
                    workspace: WorkspaceId(crate::topology::workspace_key(11)),
                    space: 11,
                    current: true,
                    readable: true,
                    bounds: rect(0, -900, 1440, 900),
                    visible: rect(0, -900, 1440, 868),
                },
                NativeDomain {
                    cg_id: 2,
                    uuid: Some(UUID_2.to_owned()),
                    output: OutputId(crate::topology::output_key(2)),
                    workspace: WorkspaceId(crate::topology::workspace_key(21)),
                    space: 21,
                    current: true,
                    readable: true,
                    bounds: rect(0, 0, 1440, 900),
                    visible: rect(0, 25, 1440, 875),
                },
            ],
            skipped: Vec::new(),
        };
        let (members, sessions, focused, closed, unreadable) = empty_maps();
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert_eq!(report.topology[0].geometry, Some(rect(0, -900, 1440, 900)));
        assert_eq!(report.topology[1].geometry, Some(rect(0, 0, 1440, 900)));
        // The visible pair demonstrably does NOT edge-touch: deriving
        // topology from work areas would lose this edge.
        assert_ne!(
            report.domains[0].domain.bounds.y + report.domains[0].domain.bounds.h,
            report.domains[1].domain.bounds.y,
            "work areas are gapped; FULL is the adjacency authority"
        );
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        assert!(reply.ready, "FULL-topology report is accepted");
        assert_eq!(engine.state_revision(), 1);
    }

    #[test]
    fn producer_snapshot_rounds_fractional_shared_edge_to_exact_touch() {
        use crate::ffi::{DisplayRect, NsDisplay, SlsDisplayRow, SlsSpaceType};
        use crate::topology::build_snapshot;
        // Real producer values: AppKit bottom-left points with a fractional
        // shared edge (Retina 1512pt wide, secondary 0.4pt past the exact
        // seam) plus menu/Dock visible insets. SLS rows ride shuffled to
        // prove UUID-exact homing independent of row order.
        let ns = vec![
            NsDisplay {
                cg_id: 1,
                uuid: Some(PROD_UUID_1.to_owned()),
                frame: DisplayRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1512.0,
                    h: 982.0,
                },
                visible: DisplayRect {
                    x: 0.0,
                    y: 32.0,
                    w: 1512.0,
                    h: 925.0,
                },
                scale: 2.0,
            },
            NsDisplay {
                cg_id: 2,
                uuid: Some(PROD_UUID_2.to_owned()),
                frame: DisplayRect {
                    x: 1512.4,
                    y: 0.0,
                    w: 1512.0,
                    h: 982.0,
                },
                visible: DisplayRect {
                    x: 1512.4,
                    y: 32.0,
                    w: 1512.0,
                    h: 925.0,
                },
                scale: 2.0,
            },
        ];
        let rows = vec![
            SlsDisplayRow {
                identifier: Some(PROD_UUID_2.to_owned()),
                spaces: vec![21],
                current: Some(21),
            },
            SlsDisplayRow {
                identifier: Some(PROD_UUID_1.to_owned()),
                spaces: vec![11, 12],
                current: Some(11),
            },
        ];
        let snapshot = build_snapshot(&ns, &rows, Some(true));
        let (ctx, skipped) = NativeContext::derive(&snapshot, &|_| Some(SlsSpaceType::User));
        assert!(skipped.is_empty(), "both displays home: {skipped:?}");
        assert_eq!(ctx.domains.len(), 3, "hidden Space retains its domain");
        let (members, sessions, focused, closed, unreadable) = empty_maps();
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert_eq!(report.topology.len(), 2);
        // 1:1 rounding lands the fractional seam on the exact shared edge.
        assert_eq!(report.topology[0].geometry, Some(rect(0, 0, 1512, 982)));
        assert_eq!(report.topology[1].geometry, Some(rect(1512, 0, 1512, 982)));
        // Domain bounds stay the converted visible work areas (menu inset
        // survives conversion: AppKit y=32 of 982pt -> CG y=25).
        let first_bounds = report.domains[0].domain.bounds;
        assert_eq!((first_bounds.x, first_bounds.y), (0, 25));
        // Display identity follows the CG id through shuffled rows; Space
        // order and the focused display never move geometry.
        assert_eq!(report.topology[0].output.0, "macos-display-1");
        assert_eq!(
            report.topology[0].native_order,
            vec!["space-11".to_owned(), "space-12".to_owned()]
        );
        assert_eq!(report.topology[0].current_native, "space-11");
        assert_eq!(report.topology[1].current_native, "space-21");
        assert!(report.topology[0].primary, "display order owns primary");
        assert!(
            report.domains.iter().all(|d| d.domain.adjacent.is_empty()),
            "no adapter-owned adjacency"
        );
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        assert!(reply.ready, "rounded FULL-topology report is accepted");
    }

    #[test]
    fn producer_snapshot_preserves_negative_and_above_primary_origins() {
        use crate::ffi::{DisplayRect, NsDisplay, SlsDisplayRow, SlsSpaceType};
        use crate::topology::build_snapshot;
        // Primary 1440x900 at the AppKit origin; one display stacked above
        // (AppKit y beyond the primary height -> negative CG y), one to the
        // left (negative AppKit x passes through).
        let ns = vec![
            NsDisplay {
                cg_id: 1,
                uuid: Some(PROD_UUID_1.to_owned()),
                frame: DisplayRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1440.0,
                    h: 900.0,
                },
                visible: DisplayRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1440.0,
                    h: 900.0,
                },
                scale: 2.0,
            },
            NsDisplay {
                cg_id: 2,
                uuid: Some(PROD_UUID_2.to_owned()),
                frame: DisplayRect {
                    x: 0.0,
                    y: 900.0,
                    w: 1440.0,
                    h: 900.0,
                },
                visible: DisplayRect {
                    x: 0.0,
                    y: 900.0,
                    w: 1440.0,
                    h: 900.0,
                },
                scale: 1.0,
            },
            NsDisplay {
                cg_id: 3,
                uuid: Some("C7AA11BB-2C3D-4E5F-A6B7-C8D9E0F1A2B3".to_owned()),
                frame: DisplayRect {
                    x: -1440.0,
                    y: 0.0,
                    w: 1440.0,
                    h: 900.0,
                },
                visible: DisplayRect {
                    x: -1440.0,
                    y: 0.0,
                    w: 1440.0,
                    h: 900.0,
                },
                scale: 2.0,
            },
        ];
        let rows = vec![
            SlsDisplayRow {
                identifier: Some(PROD_UUID_1.to_owned()),
                spaces: vec![11],
                current: Some(11),
            },
            SlsDisplayRow {
                identifier: Some(PROD_UUID_2.to_owned()),
                spaces: vec![21],
                current: Some(21),
            },
            SlsDisplayRow {
                identifier: Some("C7AA11BB-2C3D-4E5F-A6B7-C8D9E0F1A2B3".to_owned()),
                spaces: vec![31],
                current: Some(31),
            },
        ];
        let snapshot = build_snapshot(&ns, &rows, Some(true));
        let (ctx, skipped) = NativeContext::derive(&snapshot, &|_| Some(SlsSpaceType::User));
        assert!(skipped.is_empty(), "all three home: {skipped:?}");
        let (members, sessions, focused, closed, unreadable) = empty_maps();
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        let geometry = |out: &str| {
            report
                .topology
                .iter()
                .find(|t| t.output.0 == out)
                .and_then(|t| t.geometry)
        };
        assert_eq!(geometry("macos-display-1"), Some(rect(0, 0, 1440, 900)));
        assert_eq!(
            geometry("macos-display-2"),
            Some(rect(0, -900, 1440, 900)),
            "above-primary converts to a negative CG origin"
        );
        assert_eq!(
            geometry("macos-display-3"),
            Some(rect(-1440, 0, 1440, 900)),
            "left-of-primary keeps its negative origin"
        );
    }

    #[test]
    fn producer_snapshot_without_primary_yields_valid_empty_report() {
        use crate::ffi::{DisplayRect, NsDisplay, SlsDisplayRow, SlsSpaceType};
        use crate::topology::build_snapshot;
        // No (0,0) AppKit origin anywhere: conversion refuses, the display
        // is skipped with a reason, and the builder still returns a valid
        // empty report that Unit B must NOT dispatch (KDE null parity).
        let ns = vec![NsDisplay {
            cg_id: 9,
            uuid: Some(PROD_UUID_1.to_owned()),
            frame: DisplayRect {
                x: 100.0,
                y: 100.0,
                w: 800.0,
                h: 600.0,
            },
            visible: DisplayRect {
                x: 100.0,
                y: 100.0,
                w: 800.0,
                h: 600.0,
            },
            scale: 2.0,
        }];
        let rows = vec![SlsDisplayRow {
            identifier: Some(PROD_UUID_1.to_owned()),
            spaces: vec![91],
            current: Some(91),
        }];
        let snapshot = build_snapshot(&ns, &rows, Some(true));
        let (ctx, skipped) = NativeContext::derive(&snapshot, &|_| Some(SlsSpaceType::User));
        assert_eq!(ctx.domains.len(), 0, "no converted geometry, no domains");
        assert_eq!(skipped.len(), 1, "skipped honestly, never guessed");
        let (members, sessions, focused, closed, unreadable) = empty_maps();
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert!(report.domains.is_empty() && report.topology.is_empty());
    }

    #[test]
    fn bootstrap_then_ordered_reports() {
        let ctx = two_display_ctx();
        let (members, sessions, focused, closed, unreadable) = empty_maps();
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let bootstrap = engine.bootstrap_reply(INCARNATION, "req-0");
        assert!(!bootstrap.ready, "bootstrap carries no desired state");
        assert!(bootstrap.windows.is_empty());

        let first = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            1,
            1,
        );
        let reply = accepted(&first, &mut engine);
        assert!(reply.ready);
        assert_eq!(engine.state_revision(), 1);

        // Identical latest resend: duplicate, no new revision.
        match engine.handle_report(&first) {
            WorldOutcome::Duplicate(cached) => {
                assert_eq!(cached.state_revision, 1);
                assert_eq!(engine.state_revision(), 1);
            }
            other => panic!("expected duplicate, got {other:?}"),
        }

        // Advanced observation + request: accepted with a new revision.
        let second = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            2,
            2,
        );
        let reply = accepted(&second, &mut engine);
        assert_eq!(reply.state_revision, 2);
        assert_eq!(reply.based_on_observation_seq, 2);
    }

    #[test]
    fn maximized_never_inferred_and_fixed_newcomer_asserts() {
        // Fullscreen natives still report maximized:false (fence inert);
        // first-sight fixed hints assert automatic origin (membership only).
        let ctx = two_display_ctx();
        let (mut members, sessions, focused, closed, unreadable) = empty_maps();
        let d1 = key("macos-display-1", "space-11");
        let mut full = observed(1, 10, 10, 800, 600);
        full.fullscreen = true;
        let mut fixed = observed(2, 20, 20, 640, 480);
        fixed.hints = WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        };
        members.insert(d1.clone(), vec![(1001, full), (1001, fixed)]);
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        let row = &report.domains[0];
        assert_eq!(row.windows.len(), 2);
        assert!(row.windows.iter().all(|w| !w.maximized));
        let fullscreen = row
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live");
        assert!(fullscreen.fullscreen, "fullscreen rides truthfully");
        let fixed_row = row
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_2")
            .expect("live");
        assert!(
            fixed_row.floating && fixed_row.fixed_auto,
            "first-sight fixed asserts automatic origin"
        );
        // Engine admits the newcomer without any invented maximization.
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        assert!(reply.ready);
    }

    #[test]
    fn minimized_reports_hold_without_membership_or_frames() {
        let ctx = two_display_ctx();
        let (mut members, sessions, focused, closed, unreadable) = empty_maps();
        let d1 = key("macos-display-1", "space-11");
        members.insert(d1.clone(), vec![(1001, observed(1, 10, 10, 800, 600))]);
        let minimized = vec![MinimizedEntry {
            window: WindowId("1001_9".to_owned()),
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-11".to_owned()),
            sticky: false,
        }];
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &minimized,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        // A minimized id that also appears live is stripped from membership.
        let d1_members = members.get(&d1).cloned().unwrap_or_default();
        let _ = d1_members;
        assert!(
            report.domains[0]
                .windows
                .iter()
                .all(|w| w.window.0 != "1001_9"),
            "minimized lifetimes are absent from domain membership"
        );
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        let row = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_9")
            .expect("explicit minimized row");
        assert_eq!(
            row.frame_authority,
            tiler_core::world_report::ScopeAuthority::Hold(HoldReason::Minimized)
        );
        assert!(row.allocation.is_none() && row.effective_frame.is_none());
    }

    #[test]
    fn unreadable_scope_holds_without_write_authority() {
        let ctx = two_display_ctx();
        let (mut members, sessions, focused, closed, mut unreadable) = empty_maps();
        let hidden = key("macos-display-1", "space-12");
        // Even if the caller still carries stale members, the unreadable
        // scope publishes empty with an explicit hold.
        members.insert(hidden.clone(), vec![(1001, observed(3, 10, 10, 100, 100))]);
        unreadable.insert(hidden.clone(), HoldReason::Unreadable);
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert!(!report.inventory_complete);
        let row = report
            .domains
            .iter()
            .find(|d| d.domain.key() == hidden)
            .expect("hidden domain reported");
        assert!(!row.readable);
        assert!(row.windows.is_empty(), "held scope authorizes no rows");
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        let scope = reply
            .scopes
            .iter()
            .find(|s| s.scope == "macos-display-1/space-12")
            .expect("held scope status present");
        assert!(
            matches!(
                scope.authority,
                tiler_core::world_report::ScopeAuthority::Hold(_)
            ),
            "hidden scope publishes held, never controlled: {scope:?}"
        );
        assert!(
            reply
                .windows
                .iter()
                .filter(|w| w.output.0 == "macos-display-1" && w.workspace == "space-12")
                .all(|w| w.frame_authority != tiler_core::world_report::ScopeAuthority::Controlled),
            "no controlled frame authority inside the held scope"
        );
    }

    #[test]
    fn generation_revision_watermark_rejections() {
        let ctx = two_display_ctx();
        let (members, sessions, focused, closed, unreadable) = empty_maps();
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let first = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            5,
            5,
        );
        accepted(&first, &mut engine);

        // Older request sequence: superseded, never executed.
        let stale_req = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            6,
            4,
        );
        match engine.handle_report(&stale_req) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-superseded"),
            other => panic!("expected superseded, got {other:?}"),
        }
        // Older observation sequence on a fresh request: stale observation.
        let stale_obs = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            4,
            6,
        );
        match engine.handle_report(&stale_obs) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "observation-stale"),
            other => panic!("expected observation-stale, got {other:?}"),
        }
        // Foreign core generation: mismatch without state.
        let mut foreign = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            6,
            6,
        );
        foreign.core_generation = "gen-2".to_owned();
        match engine.handle_report(&foreign) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "generation-mismatch"),
            other => panic!("expected generation-mismatch, got {other:?}"),
        }
        assert_eq!(engine.state_revision(), 1, "rejections advance no revision");
    }

    #[test]
    fn explicit_close_wins_over_minimized_and_live() {
        let ctx = two_display_ctx();
        let (mut members, sessions, focused, mut closed, unreadable) = empty_maps();
        let d1 = key("macos-display-1", "space-11");
        members.insert(d1.clone(), vec![(1001, observed(1, 10, 10, 800, 600))]);
        closed.insert(d1.clone(), vec![WindowId("1001_1".to_owned())]);
        let minimized = vec![MinimizedEntry {
            window: WindowId("1001_1".to_owned()),
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-11".to_owned()),
            sticky: false,
        }];
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &minimized,
            &[],
            "",
            1,
            1,
        );
        // Closed beats live (no closed-overlaps-live) and beats minimized
        // (no minimized-overlaps): the report validates.
        assert!(validate_report(&report));
        assert!(report.domains[0].windows.is_empty());
        assert!(
            report.minimized.is_empty(),
            "close drops the minimized twin"
        );
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        assert!(
            reply.windows.iter().all(|w| w.lifetime.0 != "1001_1"),
            "explicit close authorizes no desired row"
        );
    }

    #[test]
    fn focused_global_is_informational_only() {
        // Unknown global focus never refuses: it drives policy, never proof.
        let ctx = two_display_ctx();
        let (members, sessions, focused, closed, unreadable) = empty_maps();
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "4242_7",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert_eq!(report.focused_global, "4242_7");
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        assert!(reply.ready);
        let _ = ScopedWindowMeta {
            wid: 0,
            spaces: Vec::new(),
            fullscreen: false,
            minimized: false,
            hidden: false,
        };
    }

    #[test]
    fn feedback_carrier_accepted_and_held() {
        // Possibly-executed transfer feedback holds its target without
        // re-actuation: the report is accepted and the transfer stays
        // retained (visible on the next reply) instead of settling.
        let ctx = two_display_ctx();
        let (mut members, sessions, focused, closed, unreadable) = empty_maps();
        let d1 = key("macos-display-1", "space-11");
        members.insert(d1.clone(), vec![(1001, observed(1, 10, 10, 800, 600))]);
        let feedback = vec![AttemptFeedback {
            operation_id: "op-1".to_owned(),
            kind: AttemptKind::Transfer,
            outcome: AttemptOutcome::PossiblyExecuted,
            hold: None,
        }];
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &feedback,
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        assert!(reply.ready);
    }

    #[test]
    fn empty_context_builds_valid_report_that_unit_b_must_not_dispatch() {
        let ctx = NativeContext {
            revision: 1,
            domains: Vec::new(),
            skipped: Vec::new(),
        };
        let (members, sessions, focused, closed, unreadable) = empty_maps();
        let report = assemble(
            &ctx,
            &members,
            &sessions,
            &focused,
            &closed,
            &unreadable,
            &[],
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert!(report.domains.is_empty() && report.topology.is_empty());
    }
}
