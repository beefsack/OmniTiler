#![forbid(unsafe_code)]
//! Unit A boundary: production callers land in Unit B, so non-test builds
//! would flag every constructor as dead. Tests own the coverage meanwhile.
#![cfg_attr(not(test), allow(dead_code))]

//! Full world-state report builder (world contract v2, additive, offline).
//!
//! Pure construction of one complete [`WorldReport`](tiler_core::world_report::WorldReport)
//! from observed native facts. No threads, no locks, no sinks, no actuation:
//! Unit B adopts replies for reconciliation/commands.
//!
//! Fixed stage-1 contract:
//! - [`WorkspaceMode::PerOutputLocal`](tiler_core::workspace_scope::WorkspaceMode)
//!   always ("Displays have separate workspaces"); separate/spanning mode
//!   detection is later-stage work, never guessed here.
//! - Screens carry the FULL native output rectangle plus the RAW visible
//!   work area with the adapter-side outer gap riding separately (native:
//!   [`PHASE1_OUTER_GAP`]; synthetic skeleton: `0`, which preserves the
//!   legacy skeleton's exact edge-to-edge geometry because the
//!   [`WorldEngine`](tiler_core::world_engine::WorldEngine) always insets by
//!   the carried gap).
//! - The capability `host_owned_workspace_domains` is always set: the core
//!   owns adoption, existence/order/current/membership, and native-to-logical
//!   bindings. The adapter publishes native order/current (user Spaces
//!   only — fullscreen/system Spaces never become domains upstream, so the
//!   native rows omit them), per-window native membership facts, and no
//!   logical receipts (`observed_workspace: None`, views empty): the core
//!   maps `(output, native_workspace)` to stable logical ids and derives
//!   the logical receipts. Logical echo translation via live reply lookups
//!   is stage-3 work, never a durable host table.
//! - Window rows reuse [`collect_window_facts`](crate::runtime_core::collect_window_facts)
//!   over adapter-side [`LifetimeMemory`](crate::runtime_core::LifetimeMemory)
//!   (last applied reply rows plus the native-to-logical home map): the
//!   same wire-flag normalization as the retired session-read path, with no
//!   private engine-boundary access.
//! - Minimized lifetimes are explicit [`WindowFact`](tiler_core::world_report::WindowFact)
//!   rows (`minimized: true`, no frame) homed through their native Space,
//!   never silent absence or inferred close. App-hidden (Cmd+H) windows
//!   report through the same retained minimized path (option B, reversible
//!   adapter mapping): the fact carries no cause, the true HiddenApp vs
//!   Minimized cause stays in diagnostics via `ExcludeReason::log_token`.
//!   Transition-partial inventory never reads
//!   as closes: unreadable domains omit their members and fence their
//!   screen plus the whole report incomplete.
//! - Overlap precedence is closed > minimized > live: explicit closes always
//!   win (even with incomplete inventory). The produced report always
//!   passes [`validate_report`](tiler_core::world_report::validate_report)
//!   on id overlap, given opaque caller ids.
//! - Raw mechanical attempts ride verbatim (latest-wins per key is the
//!   caller's contract); the core derives every verdict. `dpi_before` /
//!   `dpi_after` are always `None`: the logical-point adapter space has no
//!   per-window basis, never a guessed DPI.
//! - With no outputs at all the builder still returns a valid empty report;
//!   Unit B must NOT dispatch it (nothing to report, same as the KDE null).

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::gesture::GestureIngress;
use tiler_core::output_selection::MigrationSourceRefill;
use tiler_core::session::DomainKey;
use tiler_core::size_hints::FixedSizePredicate;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_report::{
    ClearAttempt, FrameAttempt, HoldReason, MemberAttempt, NativeObservation, ScreenReport,
    ViewAttempt, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

use crate::runtime_core::{LifetimeMemory, MinimizedRecord, collect_window_facts, minimized_fact};
use crate::runtime_types::{NativeContext, ObservedWindow, PHASE1_GAP, PHASE1_OUTER_GAP};

/// Complete input for one world report. The caller owns observation truth
/// (Unit B wires retained homing/lifetime memory); the builder only shapes,
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
    /// Observed native topology (every retainable user Space per display).
    pub ctx: &'a NativeContext,
    /// Member windows per native domain as `(pid, window)` pairs (exact-Space
    /// homing decided by the caller, never geometry here).
    pub members: &'a BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
    /// Adapter-side core-delivery lifetime memory (last applied reply rows
    /// plus the native-to-logical home map) for wire-flag normalization.
    pub memory: &'a LifetimeMemory,
    /// Adapter-side automatic-float assertions (see
    /// [`LifetimeMemory`]): pruned to core-floating rows by the caller,
    /// extended with newly asserted lifetimes during collection.
    pub auto_asserted: &'a mut BTreeSet<String>,
    /// Explicit closes per native domain (always honored, even incomplete).
    pub closed: &'a BTreeMap<DomainKey, Vec<WindowId>>,
    /// Explicit minimized lifetimes (absent from domain membership).
    pub minimized: &'a [MinimizedRecord],
    /// Unreadable scopes with their explicit hold (no write authority, no
    /// guessed membership: members omit and the scope fences incomplete).
    pub unreadable: &'a BTreeMap<DomainKey, HoldReason>,
    /// Raw mechanical frame attempts since the previous report (latest-wins
    /// per window is the caller's contract).
    pub frames: &'a [FrameAttempt],
    /// Raw mechanical membership attempts (latest-wins per operation window).
    pub member_attempts: &'a [MemberAttempt],
    /// Raw mechanical view attempts (latest-wins per owning view phase).
    pub view_attempts: &'a [ViewAttempt],
    /// Raw mechanical maximize-clear attempts (latest-wins per operation).
    pub clear_attempts: &'a [ClearAttempt],
    /// Normalized gesture ingress for one open native move/size gesture
    /// (`None` on observation-only reports).
    pub gesture: Option<GestureIngress>,
    /// Observed global focus id (`""` when none; informational only).
    pub focused_global: &'a str,
    /// Caller-observed enumeration completeness over live outputs.
    /// False when retained truth missed input (coverage gap) or a display
    /// went unenumerated (skipped); the whole-report inventory fence.
    pub complete: bool,
    /// Retained truth missed input (refused observation): fences every
    /// screen, since the gap cannot be localized. Skipped displays fence
    /// only their own screen row, never live screens.
    pub coverage_gap: bool,
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
    /// Primary output (`None` = first output in context order).
    pub primary_output: Option<OutputId>,
    /// Adapter-side inner gap riding with each screen (the engine applies
    /// its single inset).
    pub gap: i32,
    /// Adapter-side outer gap riding with each screen (the engine applies
    /// its single inset). Native reports carry [`PHASE1_OUTER_GAP`]; the
    /// synthetic skeleton reports `0` to preserve the legacy skeleton's
    /// exact edge-to-edge geometry.
    pub outer_gap: i32,
    /// Adapter monotonic clock in milliseconds (arbitrary process epoch).
    /// `0` means unreported (stage 1: no clock source claimed).
    pub monotonic_ms: u64,
}

impl<'a> ReportInput<'a> {
    /// Fresh input with migration/product defaults: both-axes predicate,
    /// last-remaining refill, default-tiled seed, complete inventory, no
    /// barrier, first-output primary, silent presentation, no clock. The
    /// caller fills members/memory and any holds/closes/minimized/attempts.
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
        memory: &'a LifetimeMemory,
        auto_asserted: &'a mut BTreeSet<String>,
        closed: &'a BTreeMap<DomainKey, Vec<WindowId>>,
        unreadable: &'a BTreeMap<DomainKey, HoldReason>,
        minimized: &'a [MinimizedRecord],
        focused_global: &'a str,
        coverage_gap: bool,
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
            memory,
            auto_asserted,
            closed,
            minimized,
            unreadable,
            frames: &[],
            member_attempts: &[],
            view_attempts: &[],
            clear_attempts: &[],
            gesture: None,
            focused_global,
            complete: true,
            coverage_gap,
            incomplete_outputs: &[],
            gesture_barrier: false,
            predicate: FixedSizePredicate::BothAxes,
            refill: MigrationSourceRefill::LastRemaining,
            default_tiled: true,
            primary_output: None,
            gap: PHASE1_GAP,
            outer_gap: PHASE1_OUTER_GAP,
            monotonic_ms: 0,
        }
    }
}

/// Build one complete world report. Infallible over the typed inputs; id
/// validity (opaque alphabet) stays the caller's contract, checked by
/// [`validate_report`](tiler_core::world_report::validate_report).
pub(crate) fn build_report(input: &mut ReportInput<'_>) -> WorldReport {
    // Closed > minimized > live: collect the explicit-close set first so
    // both minimized rows and live membership defer to it.
    let closed_set: BTreeSet<&str> = input
        .closed
        .values()
        .flatten()
        .map(|id| id.0.as_str())
        .collect();
    let closed: Vec<WindowId> = input
        .closed
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<WindowId>>()
        .into_iter()
        .collect();
    let minimized_set: BTreeSet<&str> = input
        .minimized
        .iter()
        .filter(|entry| !closed_set.contains(entry.window.0.as_str()))
        .map(|entry| entry.window.0.as_str())
        .collect();

    // Per-output native order in context order (first-seen output order).
    // Non-user Spaces never become domains upstream, so the native rows
    // omit fullscreen/system by construction.
    let mut output_order: Vec<OutputId> = Vec::new();
    let mut native_order: BTreeMap<OutputId, Vec<String>> = BTreeMap::new();
    let mut current_native: BTreeMap<OutputId, String> = BTreeMap::new();
    let mut output_geometry: BTreeMap<OutputId, tiler_core::geometry::Rect> = BTreeMap::new();
    let mut output_visible: BTreeMap<OutputId, tiler_core::geometry::Rect> = BTreeMap::new();
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
        // never a window-frame proxy). The visible work area rides
        // separately below.
        output_geometry
            .entry(domain.output.clone())
            .or_insert(domain.bounds);
        output_visible
            .entry(domain.output.clone())
            .or_insert(domain.visible);
    }
    let primary = input.primary_output.clone().unwrap_or_else(|| {
        output_order
            .first()
            .cloned()
            .unwrap_or_else(|| OutputId(String::new()))
    });
    // An unreadable Space fences its own scope (omitted members plus the
    // whole-report inventory fence, which the core publishes as a hold),
    // never its screen: the native order/current stay completely observed.
    let mut screens: Vec<ScreenReport> = output_order
        .iter()
        .map(|output| ScreenReport {
            output: output.clone(),
            full_rect: output_geometry.get(output).copied(),
            work_area: output_visible.get(output).copied(),
            gap: input.gap,
            outer_gap: input.outer_gap,
            complete: !input.coverage_gap && !input.incomplete_outputs.contains(output),
            primary: *output == primary,
        })
        .collect();
    // Skipped displays stay reported (incomplete, never actuated) rather
    // than deleted: their geometry is last-observed fact, only the Space
    // homing failed. Deleting them would read as a hotplug removal and
    // displace their parked members; an incomplete screen parks them
    // instead (holds, no writes) until the homing proof returns.
    {
        let mut seen: BTreeSet<String> = output_order.iter().map(|o| o.0.clone()).collect();
        for skipped in &input.ctx.skipped {
            let output = crate::topology::output_key(skipped.cg_id);
            if !seen.insert(output.clone()) {
                continue;
            }
            screens.push(ScreenReport {
                output: OutputId(output),
                full_rect: skipped.full_rect,
                work_area: skipped.work_area,
                gap: input.gap,
                outer_gap: input.outer_gap,
                complete: false,
                primary: false,
            });
        }
    }
    let native: Vec<NativeObservation> = output_order
        .iter()
        .map(|output| NativeObservation {
            output: output.clone(),
            native_order: native_order.get(output).cloned().unwrap_or_default(),
            // Empty when the native current is a non-user Space (or
            // unknown): unknown view holds, never proof of absence.
            current_native: current_native.get(output).cloned().unwrap_or_default(),
        })
        .collect();

    // Live window facts: member apps per domain (unreadable domains omit
    // their members — never guessed), minimized/closed ids stripped.
    let mut windows: Vec<WindowFact> = Vec::new();
    for domain in &input.ctx.domains {
        let key = DomainKey {
            output: domain.output.clone(),
            workspace: domain.workspace.clone(),
        };
        if input.unreadable.contains_key(&key) {
            // Unreadable scope: no membership guessed, no write authority.
            // Closes still ride (explicit closes are always honored, even
            // inside unreadable scopes).
            continue;
        }
        let Some(members) = input.members.get(&key) else {
            continue;
        };
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
        let mut domain_facts: Vec<WindowFact> = Vec::new();
        for (pid, observed) in &by_pid {
            collect_window_facts(
                &mut domain_facts,
                *pid,
                observed,
                &key,
                input.memory,
                input.auto_asserted,
                input.predicate,
            );
        }
        domain_facts.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        windows.extend(domain_facts);
    }
    // Explicit minimized rows (absent from domain membership).
    let mut minimized_facts: Vec<WindowFact> = input
        .minimized
        .iter()
        .filter(|entry| !closed_set.contains(entry.window.0.as_str()))
        .map(minimized_fact)
        .collect();
    minimized_facts.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.extend(minimized_facts);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));

    let inventory_complete = input.complete
        && input.unreadable.is_empty()
        && input.incomplete_outputs.is_empty()
        && input.ctx.skipped.is_empty();

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
        screens,
        windows,
        // Stage 1 carries no independently observed logical views: empty
        // means unknown and the core keeps its retained views. Native
        // evidence (native[] plus native_workspace facts) proves arrival.
        views: Vec::new(),
        closed,
        // The macOS adapter always opts into host-owned adoption: core
        // owns the mapping, the adapter only observes.
        host_owned_workspace_domains: true,
        equal_workspace_count: false,
        native,
        fixed_size_predicate: input.predicate,
        migration_source_refill: input.refill,
        default_tiled: input.default_tiled,
        focused_global: input.focused_global.to_owned(),
        inventory_complete,
        gesture_barrier: input.gesture_barrier,
        gesture: input.gesture.clone(),
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        monotonic_ms: input.monotonic_ms,
        frames: input.frames.to_vec(),
        member_attempts: input.member_attempts.to_vec(),
        view_attempts: input.view_attempts.to_vec(),
        clear_attempts: input.clear_attempts.to_vec(),
        presentation: tiler_core::world_report::PresentationIngress::disabled(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::directional::WorkspaceId;
    use tiler_core::geometry::Rect;
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_engine::{WorldEngine, WorldOutcome};
    use tiler_core::world_report::validate_report;

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
        memory: &LifetimeMemory,
        auto_asserted: &mut BTreeSet<String>,
        closed: &BTreeMap<DomainKey, Vec<WindowId>>,
        unreadable: &BTreeMap<DomainKey, HoldReason>,
        minimized: &[MinimizedRecord],
        focused_global: &str,
        obs: u64,
        req: u64,
    ) -> WorldReport {
        let owned_id = format!("req-{req}");
        let mut input = ReportInput::new(
            OWNER,
            INCARNATION,
            GENERATION,
            ctx,
            obs,
            req,
            &owned_id,
            members,
            memory,
            auto_asserted,
            closed,
            unreadable,
            minimized,
            focused_global,
            false,
        );
        let report = build_report(&mut input);
        assert_eq!(report.request_id, owned_id);
        report
    }

    /// Empty report inputs: no members, closes, or unreadable scopes.
    /// Factored so the multi-map tuple stays under the complexity lint.
    type EmptyReportMaps = (
        BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
        BTreeMap<DomainKey, Vec<WindowId>>,
        BTreeMap<DomainKey, HoldReason>,
    );

    fn empty_maps() -> EmptyReportMaps {
        (BTreeMap::new(), BTreeMap::new(), BTreeMap::new())
    }

    fn empty_memory() -> LifetimeMemory {
        LifetimeMemory::empty()
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
    fn builder_reports_per_output_local_with_native_adoption_rows() {
        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report), "builder output validates");
        assert_eq!(report.workspace_mode, WorkspaceMode::PerOutputLocal);
        assert!(report.host_owned_workspace_domains);
        assert_eq!(report.screens.len(), 2);
        assert_eq!(report.views, Vec::new(), "no logical views invented");
        // Native adoption rows: user Spaces in context order with the
        // native current, per output.
        assert_eq!(report.native.len(), 2);
        assert_eq!(
            report.native[0].native_order,
            vec!["space-11".to_owned(), "space-12".to_owned()]
        );
        assert_eq!(report.native[0].current_native, "space-11");
        assert_eq!(report.native[1].current_native, "space-21");
        assert!(report.screens[0].complete);
        assert!(
            report.screens[0].primary,
            "first output is primary by default"
        );
        assert!(!report.screens[1].primary);
        // Core-owned topology carries the FULL display rect, never the
        // visible work area; domain work areas ride separately per screen.
        assert_eq!(report.screens[0].full_rect, Some(rect(0, 0, 1440, 900)));
        assert_eq!(report.screens[1].full_rect, Some(rect(1440, 0, 1440, 900)));
        assert_eq!(report.screens[0].work_area, Some(rect(0, 25, 1440, 875)));
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
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert_eq!(report.screens[0].full_rect, Some(rect(0, -900, 1440, 900)));
        assert_eq!(report.screens[1].full_rect, Some(rect(0, 0, 1440, 900)));
        // The visible pair demonstrably does NOT edge-touch: deriving
        // topology from work areas would lose this edge.
        let first_work = report.screens[0].work_area.expect("work area");
        let second_work = report.screens[1].work_area.expect("work area");
        assert_ne!(
            first_work.y + first_work.h,
            second_work.y,
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
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert_eq!(report.screens.len(), 2);
        // 1:1 rounding lands the fractional seam on the exact shared edge.
        assert_eq!(report.screens[0].full_rect, Some(rect(0, 0, 1512, 982)));
        assert_eq!(report.screens[1].full_rect, Some(rect(1512, 0, 1512, 982)));
        // Domain work areas stay the converted visible work areas (menu inset
        // survives conversion: AppKit y=32 of 982pt -> CG y=25).
        let first_work = report.screens[0].work_area.expect("work area");
        assert_eq!((first_work.x, first_work.y), (0, 25));
        // Display identity follows the CG id through shuffled rows; Space
        // order and the focused display never move geometry.
        assert_eq!(report.screens[0].output.0, "macos-display-1");
        assert_eq!(
            report.native[0].native_order,
            vec!["space-11".to_owned(), "space-12".to_owned()]
        );
        assert_eq!(report.native[0].current_native, "space-11");
        assert_eq!(report.native[1].current_native, "space-21");
        assert!(report.screens[0].primary, "display order owns primary");
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
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        let geometry = |out: &str| {
            report
                .screens
                .iter()
                .find(|t| t.output.0 == out)
                .and_then(|t| t.full_rect)
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
    fn producer_snapshot_without_primary_yields_valid_skipped_screen_report() {
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
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        // The skipped display stays reported as one incomplete screen
        // (last-observed geometry, never guessed homing) with no windows
        // and no native rows.
        assert!(report.windows.is_empty());
        assert!(report.native.is_empty());
        assert_eq!(report.screens.len(), 1);
        assert!(!report.screens[0].complete);
        assert!(!report.inventory_complete);
    }

    #[test]
    fn bootstrap_then_ordered_reports() {
        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let bootstrap = engine.bootstrap_reply(INCARNATION, "req-0");
        assert!(!bootstrap.ready, "bootstrap carries no desired state");
        assert!(bootstrap.windows.is_empty());

        let mut auto_asserted = BTreeSet::new();
        let first = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
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
        let mut auto_asserted = BTreeSet::new();
        let second = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
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
        let (mut members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
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
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert_eq!(report.windows.len(), 2);
        assert!(report.windows.iter().all(|w| !w.maximized));
        let fullscreen = report
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live");
        assert!(fullscreen.fullscreen, "fullscreen rides truthfully");
        assert_eq!(
            fullscreen.native_workspace.as_deref(),
            Some("space-11"),
            "native membership rides the fact"
        );
        assert!(fullscreen.observed_workspace.is_none());
        let fixed_row = report
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
    fn minimized_reports_hold_without_membership_frames() {
        let ctx = two_display_ctx();
        let (mut members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let d1 = key("macos-display-1", "space-11");
        members.insert(d1.clone(), vec![(1001, observed(1, 10, 10, 800, 600))]);
        let minimized = vec![MinimizedRecord {
            window: WindowId("1001_9".to_owned()),
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-11".to_owned()),
            sticky: false,
        }];
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minimized,
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert!(
            report
                .windows
                .iter()
                .filter(|w| !w.minimized)
                .all(|w| w.window.0 != "1001_9"),
            "minimized lifetimes are absent from live membership"
        );
        let row = report
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_9")
            .expect("explicit minimized row");
        assert!(row.minimized && row.rect.is_none());
        assert_eq!(row.native_workspace.as_deref(), Some("space-11"));
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_9")
            .expect("explicit minimized row");
        assert_eq!(
            desired.frame_authority,
            tiler_core::world_report::ScopeAuthority::Hold(HoldReason::Minimized)
        );
        assert!(desired.allocation.is_none() && desired.effective_frame.is_none());
    }

    #[test]
    fn unreadable_scope_holds_without_write_authority() {
        // Adopt first (production order: complete observations precede
        // the gap), then lose Space 12 to an unreadable type. The
        // unreadable scope authorizes no writes — its members omit and no
        // desired row homes to its bound logical — while the healthy
        // current scope keeps tiling. Inventory reads incomplete throughout
        // the gap (never a silent close, never a guessed membership).
        let ctx = two_display_ctx();
        let (mut members, closed, mut unreadable) = empty_maps();
        let memory = empty_memory();
        let mut auto_asserted = BTreeSet::new();
        let hidden = key("macos-display-1", "space-12");
        let live = key("macos-display-1", "space-11");
        members.insert(live.clone(), vec![(1001, observed(3, 10, 10, 100, 100))]);
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let first = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&first));
        let reply = accepted(&first, &mut engine);
        assert!(reply.ready);
        let bound_12 = reply
            .workspaces
            .iter()
            .find(|row| row.native_binding.as_deref() == Some("space-12"))
            .map(|row| row.id.clone())
            .expect("adoption binds the hidden Space too");
        // Even if the caller still carries stale members, the unreadable
        // scope publishes nothing for that Space: no guessed membership.
        members.insert(hidden.clone(), vec![(1001, observed(3, 10, 10, 100, 100))]);
        unreadable.insert(hidden.clone(), HoldReason::Unreadable);
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            2,
            2,
        );
        assert!(validate_report(&report));
        assert!(!report.inventory_complete);
        assert!(
            report
                .windows
                .iter()
                .all(|w| w.native_workspace.as_deref() != Some("space-12")),
            "held scope authorizes no rows"
        );
        assert!(
            report
                .screens
                .iter()
                .find(|s| s.output.0 == "macos-display-1")
                .is_some_and(|s| s.complete),
            "unreadable Space fences its scope and the inventory, never screen geometry"
        );
        let reply = accepted(&report, &mut engine);
        assert!(
            reply
                .windows
                .iter()
                .filter(|w| w.output.0 == "macos-display-1" && w.workspace == bound_12)
                .all(
                    |w| w.frame_authority != tiler_core::world_report::ScopeAuthority::Controlled
                        && w.allocation.is_none()
                        && w.effective_frame.is_none()
                ),
            "no controlled frame authority and no geometry inside the held scope"
        );
        assert!(
            reply
                .windows
                .iter()
                .any(|w| w.frame_authority == tiler_core::world_report::ScopeAuthority::Controlled),
            "healthy scopes keep tiling through the gap"
        );
    }

    #[test]
    fn generation_revision_watermark_rejections() {
        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let mut auto_asserted = BTreeSet::new();
        let first = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            5,
            5,
        );
        accepted(&first, &mut engine);

        // Older request sequence: superseded, never executed.
        let mut auto_asserted = BTreeSet::new();
        let stale_req = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
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
        let mut auto_asserted = BTreeSet::new();
        let stale_obs = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
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
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
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
        let (mut members, mut closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let d1 = key("macos-display-1", "space-11");
        members.insert(d1.clone(), vec![(1001, observed(1, 10, 10, 800, 600))]);
        closed.insert(d1.clone(), vec![WindowId("1001_1".to_owned())]);
        let minimized = vec![MinimizedRecord {
            window: WindowId("1001_1".to_owned()),
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-11".to_owned()),
            sticky: false,
        }];
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minimized,
            "",
            1,
            1,
        );
        // Closed beats live (no closed-overlaps-live) and beats minimized
        // (no minimized row): the report validates.
        assert!(validate_report(&report));
        assert!(
            report.windows.iter().all(|w| w.window.0 != "1001_1"),
            "close drops live and minimized twins"
        );
        assert!(report.closed.iter().any(|id| id.0 == "1001_1"));
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        assert!(
            reply.windows.iter().all(|w| w.lifetime.0 != "1001_1"),
            "explicit close authorizes no desired row"
        );
    }

    #[test]
    fn retained_automatic_float_reasserts_across_hint_loss() {
        // D2: a retained automatic float (adapter-asserted, core still
        // floating it) stays floating with its origin across hint loss —
        // never reclassified to tile. An intentional float without
        // placement and no automatic assertion reports tiled instead, so
        // hold exits tile.
        use tiler_core::world_report::{ScopeAuthority, WindowDesire};
        let ctx = two_display_ctx();
        let (mut members, closed, unreadable) = empty_maps();
        let d1 = key("macos-display-1", "space-11");
        // Hintless observation: the classifier calls it tiled.
        members.insert(d1.clone(), vec![(1001, observed(1, 10, 10, 800, 600))]);
        let mut rows = BTreeMap::new();
        rows.insert(
            "1001_1".to_owned(),
            WindowDesire {
                lifetime: WindowId("1001_1".to_owned()),
                output: OutputId("macos-display-1".to_owned()),
                workspace: "ws-1".to_owned(),
                allocation: None,
                effective_frame: Some(rect(10, 10, 800, 600)),
                frame_authority: ScopeAuthority::Untouched,
                membership_authority: ScopeAuthority::Untouched,
                floating: true,
                float_origin: None,
                frame_operation: None,
                clear_operation: None,
            },
        );
        let mut native_home = BTreeMap::new();
        native_home.insert(
            ("macos-display-1".to_owned(), "space-11".to_owned()),
            ("macos-display-1".to_owned(), "ws-1".to_owned()),
        );
        let memory = LifetimeMemory { rows, native_home };
        // Adapter-asserted: re-asserts floating with automatic origin.
        let mut auto_asserted = BTreeSet::from(["1001_1".to_owned()]);
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        let row = report
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live row");
        assert!(
            row.floating && row.fixed_auto,
            "retained automatic re-asserts across hint loss, got floating={} fixed_auto={}",
            row.floating,
            row.fixed_auto,
        );
        // Not asserted but placed (effective frame): the intentional
        // placement re-asserts floating without automatic origin.
        let mut auto_empty = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_empty,
            &closed,
            &unreadable,
            &[],
            "",
            2,
            2,
        );
        assert!(validate_report(&report));
        let row = report
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live row");
        assert!(
            row.floating && !row.fixed_auto,
            "placed intentional float re-asserts without origin"
        );
        // Not asserted and placement-less: reports tiled (hold exit),
        // never a guessed float.
        let mut rows_bare = BTreeMap::new();
        rows_bare.insert(
            "1001_1".to_owned(),
            WindowDesire {
                lifetime: WindowId("1001_1".to_owned()),
                output: OutputId("macos-display-1".to_owned()),
                workspace: "ws-1".to_owned(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Untouched,
                membership_authority: ScopeAuthority::Untouched,
                floating: true,
                float_origin: None,
                frame_operation: None,
                clear_operation: None,
            },
        );
        let memory_bare = LifetimeMemory {
            rows: rows_bare,
            native_home: memory.native_home.clone(),
        };
        let mut auto_empty = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory_bare,
            &mut auto_empty,
            &closed,
            &unreadable,
            &[],
            "",
            3,
            3,
        );
        assert!(validate_report(&report));
        let row = report
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live row");
        assert!(
            !row.floating && !row.fixed_auto,
            "unasserted placement-less float reports tiled"
        );
    }

    #[test]
    fn focused_global_is_informational_only() {
        // Unknown global focus never refuses: it drives policy, never proof.
        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
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
    fn raw_attempts_ride_without_verdicts() {
        // Raw mechanical attempts ride the report; the core derives every
        // verdict. An unmatched operation id is forged evidence: staged
        // but never settling anything.
        use tiler_core::world_report::{FrameAttempt, FrameSetter, MemberAttempt, ViewAttempt};
        let ctx = two_display_ctx();
        let (mut members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let d1 = key("macos-display-1", "space-11");
        members.insert(d1.clone(), vec![(1001, observed(1, 10, 10, 800, 600))]);
        let owned_id = "req-1".to_owned();
        let mut auto_asserted = BTreeSet::new();
        let mut input = ReportInput::new(
            OWNER,
            INCARNATION,
            GENERATION,
            &ctx,
            1,
            1,
            &owned_id,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            false,
        );
        let frames = [FrameAttempt {
            window: WindowId("1001_1".to_owned()),
            target: rect(0, 25, 1440, 875),
            setter: FrameSetter::Accepted,
            readback: Some(rect(0, 25, 1440, 875)),
            dpi_before: None,
            dpi_after: None,
            attempt_ms: 0,
            operation_id: None,
            prewrite: None,
        }];
        let member_attempts = [MemberAttempt {
            operation_id: "op-9".to_owned(),
            window: WindowId("1001_1".to_owned()),
            target_output: OutputId("macos-display-1".to_owned()),
            target_workspace: "ws-1".to_owned(),
            setter: FrameSetter::Accepted,
            pre: None,
            post: None,
            attempt_ms: 0,
        }];
        let view_attempts = [ViewAttempt {
            operation_id: "op-9".to_owned(),
            target_output: OutputId("macos-display-1".to_owned()),
            target_workspace: "ws-1".to_owned(),
            setter: FrameSetter::Accepted,
            pre: None,
            post: None,
            baseline: None,
            attempt_ms: 0,
        }];
        input.frames = &frames;
        input.member_attempts = &member_attempts;
        input.view_attempts = &view_attempts;
        let report = build_report(&mut input);
        assert!(validate_report(&report));
        assert_eq!(report.frames.len(), 1);
        assert_eq!(report.member_attempts.len(), 1);
        assert_eq!(report.view_attempts.len(), 1);
        assert!(
            report
                .frames
                .iter()
                .all(|a| a.dpi_before.is_none() && a.dpi_after.is_none()),
            "no per-window DPI basis is ever claimed"
        );
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let reply = accepted(&report, &mut engine);
        assert!(reply.ready);
    }

    #[test]
    fn fullscreen_current_native_holds_view_with_empty_logical() {
        // Fullscreen/system Spaces never become domains upstream: an
        // output whose native current is unknown carries an empty
        // current_native, and the core holds the view instead of
        // inventing one.
        let mut ctx = two_display_ctx();
        for domain in &mut ctx.domains {
            if domain.output.0 == "macos-display-1" {
                domain.current = false;
            }
        }
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert_eq!(
            report.native[0].current_native, "",
            "unknown native current reads empty, never guessed"
        );
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
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut auto_asserted = BTreeSet::new();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&report));
        assert!(report.windows.is_empty() && report.screens.is_empty());
    }

    /// Hide-producer shapes with realistic separate apps (Cmd+H hides the
    /// whole app, so the survivor rides a different PID): omission
    /// (interim A) carries no row for the hidden window; an explicit
    /// minimized record (B) carries a frameless `minimized: true` row homed
    /// through its native Space. Both validate. Which shape the adapter
    /// uses is undecided; this pins the wire difference only.
    #[test]
    fn app_hide_omission_vs_explicit_minimized_report_shapes() {
        let ctx = two_display_ctx();
        let d1 = key("macos-display-1", "space-11");
        let (_, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let members = BTreeMap::from([(d1.clone(), vec![(1002, observed(8, 10, 10, 800, 600))])]);
        // A: omission under complete inventory.
        let mut auto_asserted = BTreeSet::new();
        let omitted = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report(&omitted));
        assert!(
            omitted.windows.iter().all(|w| w.window.0 != "1001_7"),
            "omission carries no row for the hidden window"
        );
        // B: explicit minimized record for the same absent window.
        let minimized = vec![MinimizedRecord {
            window: WindowId("1001_7".to_owned()),
            output: d1.output.clone(),
            workspace: d1.workspace.clone(),
            sticky: false,
        }];
        let mut auto_asserted = BTreeSet::new();
        let flagged = assemble(
            &ctx,
            &members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minimized,
            "",
            2,
            2,
        );
        assert!(validate_report(&flagged));
        let row = flagged
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_7")
            .expect("explicit minimized row");
        assert!(row.minimized, "flagged row marks minimized");
        assert!(row.rect.is_none(), "flagged row carries no frame");
    }

    /// A-vs-B hide geometry in this sequence: omission and flagged-minimized
    /// converge to the same survivor allocation, publish matching minimized
    /// hold rows for the hidden window, and restore the same pre-hide
    /// allocation on return, with no focus effect on the hidden/returning
    /// window. Only these compared properties are established; other reply
    /// semantics are untested.
    #[test]
    fn hidden_omission_vs_flagged_minimized_geometry_parity() {
        use tiler_core::world_report::EffectKind;
        let ctx = two_display_ctx();
        let d1 = key("macos-display-1", "space-11");
        let memory = empty_memory();
        let (_, closed, unreadable) = empty_maps();
        // Separate apps: Cmd+H hides pid 1001, pid 1002 survives.
        let full = BTreeMap::from([(
            d1.clone(),
            vec![
                (1001, observed(7, 10, 35, 700, 800)),
                (1002, observed(8, 720, 35, 700, 800)),
            ],
        )]);
        let hidden_members =
            BTreeMap::from([(d1.clone(), vec![(1002, observed(8, 720, 35, 700, 800))])]);
        let minimized = vec![MinimizedRecord {
            window: WindowId("1001_7".to_owned()),
            output: d1.output.clone(),
            workspace: d1.workspace.clone(),
            sticky: false,
        }];
        let alloc = |reply: &tiler_core::world_report::WorldReply, id: &str| {
            reply
                .windows
                .iter()
                .find(|w| w.lifetime.0 == id)
                .expect("tiled row")
                .allocation
        };
        let no_focus_for = |reply: &tiler_core::world_report::WorldReply| {
            reply
                .effects
                .iter()
                .all(|e| !(e.kind == EffectKind::Focus && e.target.0 == "1001_7"))
        };
        // Bootstrap both engines identically.
        let mut omit_engine = WorldEngine::new(GENERATION).expect("valid generation");
        let mut flag_engine = WorldEngine::new(GENERATION).expect("valid generation");
        let mut auto_asserted = BTreeSet::new();
        let first = assemble(
            &ctx,
            &full,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "1001_7",
            1,
            1,
        );
        assert!(validate_report(&first));
        let r1 = accepted(&first, &mut omit_engine);
        let mut auto_asserted = BTreeSet::new();
        let first = assemble(
            &ctx,
            &full,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "1001_7",
            1,
            1,
        );
        accepted(&first, &mut flag_engine);
        let a1 = alloc(&r1, "1001_7");
        // Hide: omission on one engine, flagged row on the other.
        let mut auto_asserted = BTreeSet::new();
        let hide_omit = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "1002_8",
            2,
            2,
        );
        assert!(validate_report(&hide_omit));
        let r2_omit = accepted(&hide_omit, &mut omit_engine);
        let mut auto_asserted = BTreeSet::new();
        let hide_flag = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minimized,
            "1002_8",
            2,
            2,
        );
        assert!(validate_report(&hide_flag));
        let r2_flag = accepted(&hide_flag, &mut flag_engine);
        // The compared rows match in this sequence: omission converges into
        // the D23 slot and the sweep publishes it as a minimized hold, like
        // the flagged fact. Other reply semantics are untested.
        let row_omit = r2_omit
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_7")
            .expect("omission path publishes a minimized hold row");
        let row_flag = r2_flag
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_7")
            .expect("flagged path publishes an explicit minimized row");
        assert!(
            row_omit.allocation.is_none() && row_flag.allocation.is_none(),
            "hidden rows carry no allocation on either path"
        );
        assert_eq!(
            row_omit, row_flag,
            "compared hidden desired rows match across A/B hide paths"
        );
        assert_eq!(
            alloc(&r2_omit, "1002_8"),
            alloc(&r2_flag, "1002_8"),
            "survivor allocation matches across A/B hide paths"
        );
        assert!(
            no_focus_for(&r2_omit),
            "no focus effect on the omitted window"
        );
        assert!(
            no_focus_for(&r2_flag),
            "no focus effect on the flagged window"
        );
        // Return on both paths: same restored allocation, still no focus.
        let mut auto_asserted = BTreeSet::new();
        let back = assemble(
            &ctx,
            &full,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "1002_8",
            3,
            3,
        );
        assert!(validate_report(&back));
        let r3_omit = accepted(&back, &mut omit_engine);
        let mut auto_asserted = BTreeSet::new();
        let back = assemble(
            &ctx,
            &full,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "1002_8",
            3,
            3,
        );
        let r3_flag = accepted(&back, &mut flag_engine);
        assert_eq!(
            alloc(&r3_omit, "1001_7"),
            a1,
            "omission path restores the exact pre-hide allocation"
        );
        assert_eq!(
            alloc(&r3_flag, "1001_7"),
            a1,
            "flagged path restores the same pre-hide allocation"
        );
        assert!(
            no_focus_for(&r3_omit),
            "restore mints no focus (host owns it)"
        );
        assert!(
            no_focus_for(&r3_flag),
            "restore mints no focus (host owns it)"
        );
    }

    /// Option B production path (fake-native, real builder plus real
    /// engine): hide window `1001_7` in appA while `1002_8` survives in
    /// appB. The minimized record is minted through the real retained
    /// path (`refresh_minimized` over a `HiddenApp` exclusion), then the
    /// builder reports it. The hidden row is frameless, homed through its
    /// native Space, and holds `Minimized` core-side with no focus effect;
    /// the survivor keeps tiling and no logical receipts leak.
    #[test]
    fn hidden_app_production_path_reports_retained_minimized_fact() {
        use crate::runtime_core::{RetainedApp, RetainedCore, note_scoped_members};
        use crate::runtime_types::{ExcludedOmission, ScopedBackendSnapshot};
        use crate::world_runtime::{minimized_entries, refresh_minimized, terminate_close_map};
        use tiler_core::world_report::{EffectKind, HoldReason, ScopeAuthority};
        let ctx = two_display_ctx();
        let d1 = key("macos-display-1", "space-11");
        let memory = empty_memory();
        let (_, closed, unreadable) = empty_maps();
        let mut retained = RetainedCore::default();
        retained.apps.insert(
            1001,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![observed(7, 10, 35, 700, 800)],
                focused_wid: None,
            },
        );
        retained.apps.insert(
            1002,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![observed(8, 720, 35, 700, 800)],
                focused_wid: None,
            },
        );
        retained
            .window_domains
            .insert("1001_7".to_owned(), d1.clone());
        retained
            .window_domains
            .insert("1002_8".to_owned(), d1.clone());
        for domain in &ctx.domains {
            retained.known_domains.insert(
                tiler_core::session::DomainKey {
                    output: domain.output.clone(),
                    workspace: domain.workspace.clone(),
                },
                tiler_core::session::OutputDomain {
                    id: domain.output.clone(),
                    workspace: domain.workspace.clone(),
                    bounds: domain.bounds,
                    gap: 0,
                    adjacent: std::collections::BTreeMap::new(),
                },
            );
        }
        let full = BTreeMap::from([(
            d1.clone(),
            vec![
                (1001, observed(7, 10, 35, 700, 800)),
                (1002, observed(8, 720, 35, 700, 800)),
            ],
        )]);
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let mut auto_asserted = BTreeSet::new();
        let first = assemble(
            &ctx,
            &full,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "1001_7",
            1,
            1,
        );
        assert!(validate_report(&first));
        accepted(&first, &mut engine);
        let hide_snapshot = ScopedBackendSnapshot {
            windows: Vec::new(),
            focused_wid: None,
            focus_cg_owned: false,
            focus_evidence: None,
            sys_focus: None,
            global_focus: None,
            meta: Vec::new(),
            excluded: 1,
            excluded_reasons: vec![ExcludedOmission {
                wid: 7,
                reason: crate::classifier::ExcludeReason::HiddenApp,
            }],
            revision: 3,
        };
        refresh_minimized(&mut retained, 1001, &hide_snapshot);
        note_scoped_members(&mut retained, 1001, &BTreeMap::new());
        let minted = minimized_entries(&retained);
        assert_eq!(minted.len(), 1, "HiddenApp mints one minimized record");
        assert_eq!(minted[0].window.0, "1001_7");
        let hidden_members =
            BTreeMap::from([(d1.clone(), vec![(1002, observed(8, 720, 35, 700, 800))])]);
        let mut auto_asserted = BTreeSet::new();
        let hide = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minted,
            "1002_8",
            2,
            2,
        );
        assert!(validate_report(&hide));
        let row = hide
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_7")
            .expect("hidden row");
        assert!(
            row.minimized && row.rect.is_none(),
            "frameless minimized fact"
        );
        assert_eq!(row.native_workspace.as_deref(), Some("space-11"));
        assert!(row.observed_workspace.is_none(), "no logical receipt");
        assert!(hide.views.is_empty(), "no logical view receipts");
        assert!(!hide.native.is_empty(), "native order/current rides");
        let reply = accepted(&hide, &mut engine);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_7")
            .expect("held row");
        assert_eq!(
            desired.frame_authority,
            ScopeAuthority::Hold(HoldReason::Minimized)
        );
        assert!(desired.allocation.is_none() && desired.effective_frame.is_none());
        assert!(
            reply
                .effects
                .iter()
                .all(|e| !(e.kind == EffectKind::Focus && e.target.0 == "1001_7")),
            "hide mints no focus for the hidden lifetime"
        );
        assert!(
            reply.windows.iter().any(|w| w.lifetime.0 == "1002_8"),
            "survivor keeps tiling"
        );
        let mut auto_asserted = BTreeSet::new();
        let back = assemble(
            &ctx,
            &full,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "1002_8",
            3,
            3,
        );
        let back_reply = accepted(&back, &mut engine);
        assert!(
            back_reply
                .effects
                .iter()
                .all(|e| !(e.kind == EffectKind::Focus && e.target.0 == "1001_7")),
            "restore mints no focus (host owns it)"
        );
        let map = terminate_close_map(&retained, 1001, &[]);
        assert_eq!(
            map.get(&d1),
            Some(&vec![WindowId("1001_7".to_owned())]),
            "hidden terminate closes explicitly"
        );
        let mut closed_map: BTreeMap<tiler_core::session::DomainKey, Vec<WindowId>> =
            BTreeMap::new();
        closed_map.insert(d1.clone(), vec![WindowId("1001_7".to_owned())]);
        let mut auto_asserted = BTreeSet::new();
        let gone = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &mut auto_asserted,
            &closed_map,
            &unreadable,
            &[],
            "1002_8",
            4,
            4,
        );
        assert!(validate_report(&gone));
        assert!(
            gone.windows.iter().all(|w| w.window.0 != "1001_7"),
            "closed rides no fact"
        );
        let gone_reply = accepted(&gone, &mut engine);
        assert!(
            gone_reply.windows.iter().all(|w| w.lifetime.0 != "1001_7"),
            "explicit close authorizes no desired row, no resurrection"
        );
    }

    /// Direct-target focus seam proof (option B): the public direct-target
    /// command is `WorldCommand::Focus { window, direction }`
    /// (`crates/tiler-core/src/world_commands.rs:69-99` checks the window
    /// is known then mints `explicit_focus` targeting that same window;
    /// `WorldCommand::Operation(CoreCommand::Focus { .. })` with a local
    /// route is the sibling ticket accepted by `dispatch_focus_effects` in
    /// `crates/tiler-macos/src/world_dispatch.rs:1377-1380`). Targeting the
    /// minimized-reported hidden lifetime must refuse `focus-unknown` with
    /// no revision and no effect, and the minimized hold survives.
    #[test]
    fn direct_target_focus_at_hidden_minimized_refuses_without_effect() {
        use tiler_core::world_report::{EffectKind, HoldReason, ScopeAuthority};
        let ctx = two_display_ctx();
        let d1 = key("macos-display-1", "space-11");
        let memory = empty_memory();
        let (_, closed, unreadable) = empty_maps();
        let full = BTreeMap::from([(
            d1.clone(),
            vec![
                (1001, observed(7, 10, 35, 700, 800)),
                (1002, observed(8, 720, 35, 700, 800)),
            ],
        )]);
        let hidden_members =
            BTreeMap::from([(d1.clone(), vec![(1002, observed(8, 720, 35, 700, 800))])]);
        let minimized = vec![MinimizedRecord {
            window: WindowId("1001_7".to_owned()),
            output: d1.output.clone(),
            workspace: d1.workspace.clone(),
            sticky: false,
        }];
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let mut auto_asserted = BTreeSet::new();
        let first = assemble(
            &ctx,
            &full,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "1001_7",
            1,
            1,
        );
        accepted(&first, &mut engine);
        let mut auto_asserted = BTreeSet::new();
        let hide = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minimized,
            "1002_8",
            2,
            2,
        );
        accepted(&hide, &mut engine);
        let mut auto_asserted = BTreeSet::new();
        let owned_id = "req-3".to_owned();
        let mut input = ReportInput::new(
            OWNER,
            INCARNATION,
            GENERATION,
            &ctx,
            3,
            3,
            &owned_id,
            &hidden_members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minimized,
            "1002_8",
            false,
        );
        input.command_seq = Some(3);
        input.command = Some(WorldCommand::Focus {
            window: WindowId("1001_7".to_owned()),
            direction: "left".to_owned(),
        });
        let report = build_report(&mut input);
        assert!(validate_report(&report));
        let revision_before = engine.state_revision();
        match engine.handle_report(&report) {
            WorldOutcome::Rejected { kind, .. } => {
                assert_eq!(kind, "focus-unknown");
                assert_eq!(engine.state_revision(), revision_before);
            }
            other => panic!("direct-target focus at hidden must refuse, got {other:?}"),
        }
        let mut auto_asserted = BTreeSet::new();
        let still = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minimized,
            "1002_8",
            4,
            4,
        );
        let reply = accepted(&still, &mut engine);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_7")
            .expect("hold survives");
        assert_eq!(
            desired.frame_authority,
            ScopeAuthority::Hold(HoldReason::Minimized)
        );
        assert!(
            reply
                .effects
                .iter()
                .all(|e| !(e.kind == EffectKind::Focus && e.target.0 == "1001_7")),
            "no focus effect for the hidden lifetime"
        );
    }

    /// Directional-source seam proof (option B): `WorldCommand::FocusDirectional
    /// { window, direction }` names the focus SOURCE
    /// (`crates/tiler-core/src/world_operations.rs:359-422` resolves the
    /// neighbor from the source row; unknown sources yield `NoEffect`,
    /// never an explicit focus). Sourced at the minimized-reported hidden
    /// lifetime it must mint no focus effect and never restore geometry.
    #[test]
    fn directional_source_focus_from_hidden_mints_no_effect() {
        use tiler_core::world_report::EffectKind;
        let ctx = two_display_ctx();
        let d1 = key("macos-display-1", "space-11");
        let memory = empty_memory();
        let (_, closed, unreadable) = empty_maps();
        let full = BTreeMap::from([(
            d1.clone(),
            vec![
                (1001, observed(7, 10, 35, 700, 800)),
                (1002, observed(8, 720, 35, 700, 800)),
            ],
        )]);
        let hidden_members =
            BTreeMap::from([(d1.clone(), vec![(1002, observed(8, 720, 35, 700, 800))])]);
        let minimized = vec![MinimizedRecord {
            window: WindowId("1001_7".to_owned()),
            output: d1.output.clone(),
            workspace: d1.workspace.clone(),
            sticky: false,
        }];
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let mut auto_asserted = BTreeSet::new();
        let first = assemble(
            &ctx,
            &full,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &[],
            "1001_7",
            1,
            1,
        );
        accepted(&first, &mut engine);
        let mut auto_asserted = BTreeSet::new();
        let hide = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minimized,
            "1002_8",
            2,
            2,
        );
        accepted(&hide, &mut engine);
        let mut auto_asserted = BTreeSet::new();
        let owned_id = "req-3".to_owned();
        let mut input = ReportInput::new(
            OWNER,
            INCARNATION,
            GENERATION,
            &ctx,
            3,
            3,
            &owned_id,
            &hidden_members,
            &memory,
            &mut auto_asserted,
            &closed,
            &unreadable,
            &minimized,
            "1002_8",
            false,
        );
        input.command_seq = Some(3);
        input.command = Some(WorldCommand::FocusDirectional {
            window: WindowId("1001_7".to_owned()),
            direction: "right".to_owned(),
        });
        let report = build_report(&mut input);
        assert!(validate_report(&report));
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(reply) => {
                assert!(
                    reply
                        .effects
                        .iter()
                        .all(|e| !(e.kind == EffectKind::Focus && e.target.0 == "1001_7")),
                    "directional source at hidden mints no focus for it"
                );
                assert!(
                    reply
                        .windows
                        .iter()
                        .find(|w| w.lifetime.0 == "1001_7")
                        .is_none_or(|w| w.allocation.is_none()),
                    "hidden lifetime never restores geometry via focus"
                );
            }
            WorldOutcome::Rejected { kind, .. } => {
                assert!(
                    kind == "focus-unknown" || kind == "scope-held",
                    "got {kind}"
                );
            }
            other => panic!("directional source focus must not focus hidden, got {other:?}"),
        }
    }

    /// Same-report FIRST live-to-hidden transition plus direct-target
    /// focus (option B): report 1 bootstraps both windows live, then a
    /// single report carries the first minimized hidden fact for `1001_7`
    /// (survivor `1002_8` only) together with `WorldCommand::Focus`
    /// targeting `1001_7`. Covers the complete report and the incomplete
    /// transitional-inventory report (unreadable hidden Space fences
    /// inventory while the minimized fact rides). Either outcome is
    /// accepted without blessing focus: rejection must be `focus-unknown`
    /// with no revision, acceptance must mint no `Focus` effect on the
    /// hidden lifetime and restore no geometry/allocation. A minted focus
    /// on the hidden minimized lifetime is a core-semantics blocker, not
    /// an adapter fix.
    #[test]
    fn first_transition_same_report_focus_never_restores_hidden() {
        use tiler_core::world_report::{EffectKind, HoldReason, ScopeAuthority};
        let ctx = two_display_ctx();
        let d1 = key("macos-display-1", "space-11");
        let partial_scope = key("macos-display-1", "space-12");
        let memory = empty_memory();
        let (_, closed, _) = empty_maps();
        let full = BTreeMap::from([(
            d1.clone(),
            vec![
                (1001, observed(7, 10, 35, 700, 800)),
                (1002, observed(8, 720, 35, 700, 800)),
            ],
        )]);
        let hidden_members =
            BTreeMap::from([(d1.clone(), vec![(1002, observed(8, 720, 35, 700, 800))])]);
        let minimized = vec![MinimizedRecord {
            window: WindowId("1001_7".to_owned()),
            output: d1.output.clone(),
            workspace: d1.workspace.clone(),
            sticky: false,
        }];
        let complete_unreadable: BTreeMap<DomainKey, HoldReason> = BTreeMap::new();
        let mut partial_unreadable: BTreeMap<DomainKey, HoldReason> = BTreeMap::new();
        partial_unreadable.insert(partial_scope.clone(), HoldReason::Unreadable);
        for (label, unreadable) in [
            ("complete", &complete_unreadable),
            ("incomplete-transitional", &partial_unreadable),
        ] {
            let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
            let mut auto_asserted = BTreeSet::new();
            let first = assemble(
                &ctx,
                &full,
                &memory,
                &mut auto_asserted,
                &closed,
                &BTreeMap::new(),
                &[],
                "1001_7",
                1,
                1,
            );
            assert!(validate_report(&first));
            accepted(&first, &mut engine);
            // Same report: first minimized transition plus direct-target
            // focus at the hidden lifetime.
            let mut auto_asserted = BTreeSet::new();
            let owned_id = "req-2".to_owned();
            let mut input = ReportInput::new(
                OWNER,
                INCARNATION,
                GENERATION,
                &ctx,
                2,
                2,
                &owned_id,
                &hidden_members,
                &memory,
                &mut auto_asserted,
                &closed,
                unreadable,
                &minimized,
                "1002_8",
                false,
            );
            input.command_seq = Some(2);
            input.command = Some(WorldCommand::Focus {
                window: WindowId("1001_7".to_owned()),
                direction: "left".to_owned(),
            });
            let report = build_report(&mut input);
            assert!(
                validate_report(&report),
                "{label}: transition report validates"
            );
            let hidden_row = report
                .windows
                .iter()
                .find(|w| w.window.0 == "1001_7")
                .expect("hidden minimized fact rides");
            assert!(
                hidden_row.minimized && hidden_row.rect.is_none(),
                "{label}: frameless fact"
            );
            assert_eq!(hidden_row.native_workspace.as_deref(), Some("space-11"));
            if label == "incomplete-transitional" {
                assert!(!report.inventory_complete, "partial AX holds inventory");
            }
            let revision_before = engine.state_revision();
            match engine.handle_report(&report) {
                WorldOutcome::Rejected { kind, .. } => {
                    assert_eq!(kind, "focus-unknown", "{label}: rejected as focus-unknown");
                    assert_eq!(
                        engine.state_revision(),
                        revision_before,
                        "{label}: no convergence"
                    );
                }
                WorldOutcome::Accepted(reply) => {
                    assert!(
                        reply
                            .effects
                            .iter()
                            .all(|e| !(e.kind == EffectKind::Focus && e.target.0 == "1001_7")),
                        "{label}: core-semantics blocker if this mints focus on hidden",
                    );
                    let row = reply
                        .windows
                        .iter()
                        .find(|w| w.lifetime.0 == "1001_7")
                        .expect("hidden desired row");
                    assert!(
                        row.allocation.is_none() && row.effective_frame.is_none(),
                        "{label}: no geometry restore for hidden"
                    );
                    assert!(
                        row.frame_authority != ScopeAuthority::Controlled,
                        "{label}: no controlled authority for hidden"
                    );
                }
                other => panic!("{label}: unexpected outcome, got {other:?}"),
            }
            // Follow-up observation-only report: the minimized transition
            // itself holds with no focus effect either way.
            let mut auto_asserted = BTreeSet::new();
            let still = assemble(
                &ctx,
                &hidden_members,
                &memory,
                &mut auto_asserted,
                &closed,
                unreadable,
                &minimized,
                "1002_8",
                3,
                3,
            );
            assert!(validate_report(&still));
            let reply = accepted(&still, &mut engine);
            let desired = reply
                .windows
                .iter()
                .find(|w| w.lifetime.0 == "1001_7")
                .expect("minimized hold survives");
            assert!(
                desired.allocation.is_none() && desired.effective_frame.is_none(),
                "{label}: follow-up restores no geometry"
            );
            assert!(
                reply
                    .effects
                    .iter()
                    .all(|e| !(e.kind == EffectKind::Focus && e.target.0 == "1001_7")),
                "{label}: follow-up mints no focus for hidden"
            );
        }
    }
}
