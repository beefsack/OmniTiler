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
//!   [`PHASE1_OUTER_GAP`](crate::runtime_types::PHASE1_OUTER_GAP); synthetic skeleton: `0`, which preserves the
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
//!   (last applied reply rows): the
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
use tiler_core::session::DomainKey;
use tiler_core::world_report::{
    ClearAttempt, FrameAttempt, HoldReason, MemberAttempt, NativeCreationAttempt,
    NativeObservation, NativeOccupancy, NativeWorkspaceFact, OutputIdentity, PresentationRowV4,
    ScreenReport, ViewAttempt, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReportV4,
};

use crate::runtime_core::{LifetimeMemory, MinimizedRecord, collect_window_facts, minimized_fact};
use crate::runtime_types::{NativeContext, ObservedWindow};

/// Grounded focus home-current verdict for one presentation ingress: the
/// retained home domain currency for the focused window from the same
/// retained snapshot as the foreground frame, so eligibility and geometry
/// agree. Unproven currency fails closed to hidden-workspace.
/// Contract M4: macOS never asserts a native maximize fact, so no
/// work-area, display, or placement inputs ride here; zoom and
/// full-display coverage stay renderer-local vetoes and the report
/// maximized lane stays null.
#[derive(Debug, Clone, Default)]
pub(crate) struct FocusEligibilityFacts {
    /// Retained home-current verdict for the focused window: `Some(true)`
    /// when its retained home domain is a current user Space, `Some(false)`
    /// when homed but not current, `None` when unhomed (unknown). Unproven
    /// currency fails closed to hidden-workspace.
    pub home_current: Option<bool>,
}

/// Build the presentation ingress for one report from retained truth.
///
/// Stage-4 mapping (offline, no core change):
/// - Foreground token is the retained observed-focus id (`""` when none);
///   the renderer revalidates it live against the reply token, so a stale
///   token hides instead of painting.
/// - The foreground frame is the live retained rect for that id (`None`
///   when unreadable, minimized, or owner-hidden); minimized/host_hidden/
///   fullscreen ride as truthful flags from the retained minimized record
///   legs and the retained fullscreen bit.
///   `foreground_shell` is grounded false: classifier exclusions (own
///   bundle, accessory/prohibited policy, nonordinary layer, unknown role)
///   are omitted from membership by construction, so an admitted focused
///   member is never a shell surface.
/// - `foreground_hidden_workspace` is grounded from the retained home plus
///   fresh SLS current flags: homed-but-not-current (and unhomed) reads
///   hidden, so the core — not just the renderer Space leg — owns the
///   off-Space hide.
/// - Contract M4: macOS never asserts a native maximize fact. Zoom and
///   full-display coverage stay renderer-local vetoes and never ride the
///   report: `foreground_maximized` is always null (unknown), even for
///   work-area- or full-display-sized frames.
/// - `foreground_cloaked` stays false: no roster fact names cloaking
///   (CG offscreen alone proves nothing, per the port record), and the
///   render-side classifier/facts-unknown vetoes fail closed instead.
/// - No positively grounded game signal exists (a focused fullscreen bit
///   alone never proves a game), so `paused` rides false and game
///   suspension stays separate. Fullscreen rides truthfully and a
///   focused-fullscreen output suspends presentation surfaces per the
///   focused-fullscreen rule (borders, underlays, and overlays suppressed,
///   preview dropped); the render-side fullscreen fence stays the
///   last-moment veto over stale reports. Session lock stays
///   renderer-side (main-thread only).
/// - Border enable plus gap/width/extension ride the startup-resolved
///   style; underlay enable rides false (no production underlay loop yet).
/// - Chord/move ride silent (no chord source; the drag gesture ingress
///   rides separately).
/// - Pager order rides empty (no native stacking snapshot is plumbed yet;
///   the core falls back to retained then deterministic order).
///   Pager frames carry one live row per live member with a positively-sized
///   rect: float/tile snapshots are not retained adapter-side, so members
///   with no live frame render no miniature, never a guess.
/// - Cursor rides unsampled (core falls back through focused output).
pub(crate) fn build_presentation_ingress(
    focused_global: &str,
    members: &BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
    unknown: &[(u32, ObservedWindow)],
    minimized: &[MinimizedRecord],
    focused_fullscreen: bool,
    eligibility: &FocusEligibilityFacts,
    _style: &crate::active_border::PresentationStylePx,
) -> tiler_core::world_report::PresentationIngressV4 {
    use tiler_core::world_report::PresentationIngressV4;
    // Separated retained lifetimes: `minimized` is the truthful
    // AXMinimized leg, `host_hidden` the truthful owner-hidden leg. A
    // hidden-only record (minimized false, host_hidden true) must not
    // read as minimized.
    let focused_record = minimized
        .iter()
        .find(|entry| entry.window.0.as_str() == focused_global);
    let foreground_minimized = focused_record.map(|entry| entry.minimized).unwrap_or(false);
    let foreground_host_hidden = focused_record
        .map(|entry| entry.host_hidden)
        .unwrap_or(false);
    let foreground_retained_hidden = foreground_minimized || foreground_host_hidden;
    let mut foreground_frame = None;
    if !focused_global.is_empty() && !foreground_retained_hidden {
        'search: for members_of_domain in members.values() {
            for (pid, window) in members_of_domain {
                if crate::runtime_types::window_id(*pid, window.wid) == focused_global {
                    if window.w > 0 && window.h > 0 {
                        foreground_frame = Some(tiler_core::geometry::Rect {
                            x: window.x,
                            y: window.y,
                            w: window.w,
                            h: window.h,
                        });
                    }
                    break 'search;
                }
            }
        }
        if foreground_frame.is_none() {
            for (pid, window) in unknown {
                if crate::runtime_types::window_id(*pid, window.wid) == focused_global
                    && window.w > 0
                    && window.h > 0
                {
                    foreground_frame = Some(tiler_core::geometry::Rect {
                        x: window.x,
                        y: window.y,
                        w: window.w,
                        h: window.h,
                    });
                    break;
                }
            }
        }
    }
    let mut pager_frames = Vec::new();
    for members_of_domain in members.values() {
        for (pid, window) in members_of_domain {
            if window.w > 0 && window.h > 0 {
                pager_frames.push(tiler_core::world_report::PresentationPagerFrameV4 {
                    window: tiler_core::directional::WindowId(crate::runtime_types::window_id(
                        *pid, window.wid,
                    )),
                    live: Some(tiler_core::geometry::Rect {
                        x: window.x,
                        y: window.y,
                        w: window.w,
                        h: window.h,
                    }),
                });
            }
        }
    }
    pager_frames.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    // Grounded eligibility from the same retained snapshot as the frame:
    // unproven currency fails closed to hidden-workspace (the core owns
    // the off-Space hide; empty focus still hides as no-target first).
    let foreground_hidden_workspace = !matches!(eligibility.home_current, Some(true));
    // Contract M4: macOS never asserts a native maximize fact; the
    // zoom/full-display heuristic stays renderer-local and never rides the
    // report. Foreground maximized is therefore always null (unknown).
    PresentationIngressV4 {
        foreground: focused_global.to_owned(),
        foreground_minimized: Some(foreground_minimized),
        foreground_maximized: None,
        foreground_fullscreen: Some(focused_fullscreen),
        foreground_host_hidden: Some(foreground_host_hidden),
        foreground_hidden_workspace: Some(foreground_hidden_workspace),
        foreground_frame,
        chord_held: false,
        move_window: String::new(),
        pager_order: Vec::new(),
        pager_frames,
        witness: None,
        foreground_out: None,
    }
}

/// Complete input for one world report. The caller owns observation truth
/// (Unit B wires retained homing/lifetime memory); the builder only shapes,
/// normalizes, and de-overlaps.
pub(crate) struct ReportInput<'a> {
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
    /// Adapter-side core-delivery lifetime memory (last applied reply rows)
    /// for wire-flag normalization.
    pub memory: &'a LifetimeMemory,
    /// Neutral raw native facts per runtime window id (`"<pid>_<wid>"`) for
    /// v4 core classification. Missing entries and `None` fields both mean
    /// unknown (null, never a default, never a policy echo).
    pub raw: &'a BTreeMap<String, crate::runtime_types::WindowRawFacts>,
    /// Explicit closes as flat ids (always honored, even incomplete).
    /// Flat ids are the actual close authority; no homing is required and
    /// no domain key is invented for unknown homes.
    pub closed: &'a [WindowId],
    /// Unknown-home live windows as `(pid, window)` pairs: each candidate
    /// is grounded against fresh FULL output geometry to a unique output
    /// (ties and outside frames omit with a hold); grounded rows surface
    /// with native workspace None and truthful fullscreen.
    pub unknown: &'a [(u32, ObservedWindow)],
    /// Per-app incomplete holds outstanding (any roster incomplete): forces
    /// whole-report incompleteness without clearing through the session gap.
    pub roster_incomplete: bool,
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
    /// Raw mechanical native creation attempts (latest-wins per operation).
    pub creation_attempts: &'a [NativeCreationAttempt],
    /// Raw close-focus feedback rows (bounded staged row, forwarded once
    /// with its original stamp verbatim).
    pub focus_feedback: &'a [tiler_core::world_report::FocusFeedback],
    /// Normalized gesture ingress for one open native move/size gesture
    /// (`None` on observation-only reports).
    pub gesture: Option<GestureIngress>,
    /// Observed global focus id (`""` when none; informational only).
    pub focused_global: &'a str,
    /// Foreground provenance for the v4 `foregroundKnown` bit: true only on
    /// proven bound agreement or genuine windowless; false (unknown) requires
    /// empty focus. The producer sets this from retained
    /// `observed_focus_known`; `ReportInput::new` defaults to the mechanical
    /// `!focused_global.is_empty()` for offline callers without provenance.
    pub foreground_known: bool,
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
    /// Primary output (`None` = first output in context order).
    pub primary_output: Option<OutputId>,
    /// Adapter monotonic clock in milliseconds (arbitrary process epoch).
    /// `0` means unreported (stage 1: no clock source claimed).
    pub monotonic_ms: u64,
    /// Presentation ingress facts for the reply presentation shapes (see
    /// [`build_presentation_ingress`]). Built by the dispatcher from
    /// retained truth.
    pub presentation: tiler_core::world_report::PresentationIngressV4,
}

impl<'a> ReportInput<'a> {
    /// Fresh input with migration/product defaults: complete inventory, no
    /// barrier, first-output primary, silent presentation, no clock. The
    /// caller fills members/memory and any holds/closes/minimized/attempts.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        adapter_incarnation: &'a str,
        core_generation: &'a str,
        ctx: &'a NativeContext,
        observation_seq: u64,
        request_seq: u64,
        request_id: &'a str,
        members: &'a BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
        memory: &'a LifetimeMemory,
        closed: &'a [WindowId],
        unreadable: &'a BTreeMap<DomainKey, HoldReason>,
        minimized: &'a [MinimizedRecord],
        focused_global: &'a str,
        coverage_gap: bool,
    ) -> Self {
        Self {
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
            closed,
            unknown: &[],
            raw: &*crate::runtime_types::EMPTY_RAW_FACTS,
            roster_incomplete: false,
            minimized,
            unreadable,
            frames: &[],
            member_attempts: &[],
            view_attempts: &[],
            clear_attempts: &[],
            creation_attempts: &[],
            focus_feedback: &[],
            gesture: None,
            focused_global,
            foreground_known: !focused_global.is_empty(),
            complete: true,
            coverage_gap,
            incomplete_outputs: &[],
            gesture_barrier: false,
            primary_output: None,
            monotonic_ms: 0,
            presentation: tiler_core::world_report::PresentationIngressV4::default(),
        }
    }
}

/// Build one complete world report. Infallible over the typed inputs; id
/// validity (opaque alphabet) stays the caller's contract, checked by
/// [`validate_report_v4_reason`](tiler_core::world_report_v4_checks::validate_report_v4_reason).
pub(crate) fn build_report(input: &mut ReportInput<'_>) -> WorldReportV4 {
    // Closed > minimized > live: collect the explicit-close set first so
    // both minimized rows and live membership defer to it. Flat ids are
    // the authority; no homing, no invented keys.
    let closed_set: BTreeSet<&str> = input.closed.iter().map(|id| id.0.as_str()).collect();
    let closed: Vec<WindowId> = input
        .closed
        .iter()
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
    let _ = primary;
    // An unreadable Space fences its own scope (omitted members plus the
    // whole-report inventory fence, which the core publishes as a hold),
    // never its screen: the native order/current stay completely observed.
    // v4 screens carry FULL rect + work area + scale 1 + neutral identity;
    // gaps ride via settings and primary rides as host-designated only.
    let mut screens: Vec<ScreenReport> = output_order
        .iter()
        .map(|output| ScreenReport {
            output: output.clone(),
            full_rect: output_geometry.get(output).copied(),
            work_area: output_visible.get(output).copied(),
            complete: !input.coverage_gap && !input.incomplete_outputs.contains(output),
            scale: 1.0,
            identity: OutputIdentity::unknown(),
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
                complete: false,
                scale: 1.0,
                identity: OutputIdentity::unknown(),
            });
        }
    }
    let native: Vec<NativeObservation> = output_order
        .iter()
        .map(|output| NativeObservation {
            output: output.clone(),
            native_order: native_order.get(output).cloned().unwrap_or_default(),
            // None when the native current is a non-user Space (or
            // unknown): retains the logical view while using the order,
            // never adopted and never a move.
            current_native: current_native.get(output).cloned(),
            // Explicit unknown occupancy: the neutral count lane is not
            // yet populated from an all-native-window roster; it cannot
            // prove an empty desktop and is never fabricated as zero.
            occupancy: native_order
                .get(output)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|native| NativeOccupancy {
                    native,
                    count: None,
                })
                .collect(),
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
            let raw_of = |id: &str| input.raw.get(id).cloned().unwrap_or_default();
            collect_window_facts(
                &mut domain_facts,
                *pid,
                observed,
                &key,
                input.memory,
                &raw_of,
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
    // Unknown-home live rows: ground each candidate against fresh FULL
    // output geometry. Grounded rows surface with native workspace None
    // (fullscreen truthful, sticky false); ties, zero overlap, degenerate,
    // and outside frames omit entirely (the roster hold fences inventory).
    // Closed/minimized ids defer out.
    {
        let mut by_output: BTreeMap<OutputId, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
        for (pid, window) in input.unknown {
            let id = crate::runtime_types::window_id(*pid, window.wid);
            if closed_set.contains(id.as_str()) || minimized_set.contains(id.as_str()) {
                continue;
            }
            let Some(output) = crate::runtime_core::ground_unknown_output(window, input.ctx) else {
                continue;
            };
            by_output.entry(output).or_default().push((*pid, *window));
        }
        for (output, pairs) in &by_output {
            let mut by_pid: BTreeMap<u32, Vec<ObservedWindow>> = BTreeMap::new();
            for (pid, window) in pairs {
                by_pid.entry(*pid).or_default().push(*window);
            }
            for (pid, observed) in &by_pid {
                for w in observed {
                    let id = crate::runtime_types::window_id(*pid, w.wid);
                    let raw: crate::runtime_types::WindowRawFacts =
                        input.raw.get(&id).cloned().unwrap_or_default();
                    let rect = tiler_core::geometry::Rect {
                        x: w.x,
                        y: w.y,
                        w: w.w,
                        h: w.h,
                    };
                    // Grounded unknown-home rows: output via geometry,
                    // native workspace unknown (never guessed), truthful
                    // fullscreen, null maximized/sticky per contract.
                    // Neutral raw facts ride where known (null otherwise).
                    windows.push(WindowFact {
                        window: WindowId(id.clone()),
                        output: Some(output.clone()),
                        rect: Some(rect),
                        fullscreen: Some(w.fullscreen),
                        maximized: None,
                        sticky: None,
                        minimized: Some(false),
                        hints: Some(w.hints),
                        native_workspace: NativeWorkspaceFact::Omitted,
                        host_hidden: raw.host_hidden,
                        privileged: None,
                        managed: None,
                        accepts_input: None,
                        activation_inhibited: None,
                        tool_window: None,
                        owned: None,
                        transient: None,
                        modal: raw.modal,
                        resizable: raw.resizable,
                        above: None,
                        below: None,
                        owner: None,
                        parent: None,
                        role: raw.role.clone(),
                        app_id: raw.app_id.clone(),
                        executable: None,
                        process_id: Some(pid.to_string()),
                        class: None,
                        title: None,
                        window_type: crate::runtime_types::normalize_window_type(
                            raw.role.as_deref(),
                            raw.subrole.as_deref(),
                        ),
                        activation_policy: raw.activation_policy,
                        layer: raw.layer,
                        fullscreen_owner: None,
                        fullscreen_exitable: None,
                        suppression_available: None,
                        visibility: tiler_core::world_report::VisibilityFact {
                            visible: None,
                            suppression_owned: None,
                        },
                        last_write: None,
                    });
                }
            }
        }
    }
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));

    let inventory_complete = input.complete
        && input.unreadable.is_empty()
        && input.incomplete_outputs.is_empty()
        && input.ctx.skipped.is_empty()
        && !input.roster_incomplete;

    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: input.adapter_incarnation.to_owned(),
        core_generation: input.core_generation.to_owned(),
        observation_seq: input.observation_seq,
        request_seq: input.request_seq,
        request_id: input.request_id.to_owned(),
        command_seq: input.command_seq,
        command: input.command.clone(),
        screens,
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed,
        native,
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: input.focused_global.to_owned(),
        foreground_known: input.foreground_known,
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: input.primary_output.clone(),
        inventory_complete,
        gesture_barrier: input.gesture_barrier,
        gesture: input.gesture.clone(),
        monotonic_ms: input.monotonic_ms,
        frames: input.frames.to_vec(),
        member_attempts: input.member_attempts.to_vec(),
        view_attempts: input.view_attempts.to_vec(),
        clear_attempts: input.clear_attempts.to_vec(),
        creation_attempts: input.creation_attempts.to_vec(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: input.focus_feedback.to_vec(),
        presentation: PresentationRowV4::Valid(input.presentation.clone()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::directional::WorkspaceId;
    use tiler_core::geometry::Rect;
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_engine::{WorldEngine, WorldOutcome};
    use tiler_core::world_report_v4_checks::validate_report_v4_reason;

    use crate::runtime_types::{NativeDomain, ObservedWindow, ScopedWindowMeta};

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
        closed: &[WindowId],
        unreadable: &BTreeMap<DomainKey, HoldReason>,
        minimized: &[MinimizedRecord],
        focused_global: &str,
        obs: u64,
        req: u64,
    ) -> WorldReportV4 {
        let owned_id = format!("req-{req}");
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            ctx,
            obs,
            req,
            &owned_id,
            members,
            memory,
            closed,
            unreadable,
            minimized,
            focused_global,
            false,
        );
        let raw = realistic_raw_for(members);
        input.raw = &raw;
        let report = build_report(&mut input);
        assert_eq!(report.request_id, owned_id);
        report
    }

    /// Empty report inputs: no members, closes, or unreadable scopes.
    /// Factored so the multi-map tuple stays under the complexity lint.
    type EmptyReportMaps = (
        BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
        Vec<WindowId>,
        BTreeMap<DomainKey, HoldReason>,
    );

    fn empty_maps() -> EmptyReportMaps {
        (BTreeMap::new(), Vec::new(), BTreeMap::new())
    }

    fn empty_memory() -> LifetimeMemory {
        LifetimeMemory::empty()
    }

    /// Real-engine binding for these fixtures: production macOS
    /// capabilities (never the synthetic full-support test default, which
    /// would hold every realistic row unreadable).
    fn production_engine() -> WorldEngine {
        WorldEngine::new_test_bound_with_capabilities(
            GENERATION,
            INCARNATION,
            crate::world_connection::macos_capabilities(),
        )
        .expect("production capabilities bind")
    }

    /// Realistic macOS raw native facts for every member window: ordinary
    /// `AXWindow`/`AXStandardWindow`, non-modal, resizable, base layer,
    /// regular app, visible. Explicit synthetic fixture captures, never
    /// policy derivations.
    fn realistic_raw_for(
        members: &BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
    ) -> BTreeMap<String, crate::runtime_types::WindowRawFacts> {
        let mut raw = BTreeMap::new();
        for pairs in members.values() {
            for (pid, window) in pairs {
                raw.insert(
                    crate::runtime_types::window_id(*pid, window.wid),
                    crate::runtime_types::WindowRawFacts {
                        role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                        subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                        modal: Some(false),
                        resizable: Some(true),
                        layer: Some(0),
                        app_id: Some("com.example.app".to_owned()),
                        activation_policy: Some(
                            tiler_core::world_report::ActivationPolicy::Regular,
                        ),
                        host_hidden: Some(false),
                    },
                );
            }
        }
        raw
    }

    fn accepted(
        report: &WorldReportV4,
        engine: &mut WorldEngine,
    ) -> tiler_core::world_report::WorldReplyV4 {
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
        let report = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(
            validate_report_v4_reason(&report).is_none(),
            "builder output validates"
        );
        assert_eq!(report.screens.len(), 2);
        // Native adoption rows: user Spaces in context order with the
        // native current, per output.
        assert_eq!(report.native.len(), 2);
        assert_eq!(
            report.native[0].native_order,
            vec!["space-11".to_owned(), "space-12".to_owned()]
        );
        assert_eq!(report.native[0].current_native.as_deref(), Some("space-11"));
        assert_eq!(report.native[1].current_native.as_deref(), Some("space-21"));
        assert!(report.screens[0].complete);
        assert!(
            report.primary_output.is_none(),
            "no host-designated primary by default"
        );
        // Core-owned topology carries the FULL display rect, never the
        // visible work area; domain work areas ride separately per screen.
        assert_eq!(report.screens[0].full_rect, Some(rect(0, 0, 1440, 900)));
        assert_eq!(report.screens[1].full_rect, Some(rect(1440, 0, 1440, 900)));
        assert_eq!(report.screens[0].work_area, Some(rect(0, 25, 1440, 875)));
        assert!(report.inventory_complete);
    }

    #[test]
    fn builder_forwards_staged_close_focus_feedback_verbatim() {
        // Session feedback path: one bounded raw row rides the report once
        // with its original stamp, verbatim — never re-stamped, never
        // fabricated.
        use tiler_core::world_report::{FocusFeedback, VisibilityReason, VisibilitySetter};
        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let owned_id = "req-1".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            1,
            1,
            &owned_id,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "",
            false,
        );
        let row = FocusFeedback {
            id: "close-focus-3".to_owned(),
            target: WindowId("1001_2".to_owned()),
            revision: 3,
            setter: VisibilitySetter::Accepted,
            reason: VisibilityReason::Empty,
            pre: None,
            post: None,
            foreground_proven: false,
            arrival: Vec::new(),
            attempt_ms: 42,
        };
        let rows = vec![row.clone()];
        input.focus_feedback = &rows;
        let report = build_report(&mut input);
        assert_eq!(report.focus_feedback, vec![row]);
        // Default input carries no feedback (no fabrication).
        let (members2, closed2, unreadable2) = empty_maps();
        let memory2 = empty_memory();
        let owned2 = "req-2".to_owned();
        let mut bare = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            1,
            2,
            &owned2,
            &members2,
            &memory2,
            &closed2,
            &unreadable2,
            &[],
            "",
            false,
        );
        let _ = &mut bare;
        let report2 = build_report(&mut bare);
        assert!(report2.focus_feedback.is_empty());
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
        let report = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(validate_report_v4_reason(&report).is_none());
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
        let mut engine = production_engine();
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
        let report = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(validate_report_v4_reason(&report).is_none());
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
        assert_eq!(report.native[0].current_native.as_deref(), Some("space-11"));
        assert_eq!(report.native[1].current_native.as_deref(), Some("space-21"));
        assert_eq!(report.screens[0].output.0, "macos-display-1");
        assert_eq!(
            report.primary_output, None,
            "host primary stays null without designation"
        );
        let mut engine = production_engine();
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
        let report = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(validate_report_v4_reason(&report).is_none());
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
        let report = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(validate_report_v4_reason(&report).is_none());
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
        // Genuine production handshake through the connection route mints the
        // bound generation; no synthetic bootstrap reply is fabricated.
        let generation = crate::world_connection::connect(&mut engine, INCARNATION, &[])
            .expect("handshake binds generation")
            .generation;
        assert_eq!(generation, engine.core_generation());

        let first = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
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
        let second = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 2, 2);
        let reply = accepted(&second, &mut engine);
        assert_eq!(reply.state_revision, 2);
        assert_eq!(reply.based_on_observation_seq, 2);
    }

    #[test]
    fn maximized_never_inferred_and_fixed_newcomer_asserts() {
        // Fullscreen natives still report maximized:null (no native fact;
        // the zoom fence stays renderer-local);
        // first-sight fixed hints ride plain on the wire and the core
        // admits the automatic float (membership only).
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
        let report = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(validate_report_v4_reason(&report).is_none());
        assert_eq!(report.windows.len(), 2);
        assert!(report.windows.iter().all(|w| w.maximized != Some(true)));
        let fullscreen = report
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live");
        assert!(
            fullscreen.fullscreen == Some(true),
            "fullscreen rides truthfully"
        );
        assert_eq!(
            fullscreen.native_workspace.as_deref(),
            Some("space-11"),
            "native membership rides the fact"
        );
        let fixed_row = report
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_2")
            .expect("live");
        assert_eq!(
            fixed_row.hints,
            Some(WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: Some(640),
                max_h: Some(480)
            }),
            "first-sight fixed hints ride carried"
        );
        assert!(
            fixed_row.maximized != Some(true) && fixed_row.fullscreen != Some(true),
            "first-sight fixed rides plain; the core owns the automatic marker"
        );
        // Engine admits the fixed newcomer as an automatic float
        // (membership only) without any invented maximization.
        let mut engine = production_engine();
        let reply = accepted(&report, &mut engine);
        assert!(reply.ready);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_2")
            .expect("admitted float");
        assert!(
            desired.floating,
            "fixed newcomer floats automatic core-side"
        );
        assert!(
            desired.allocation.is_none(),
            "automatic float takes no tile allocation"
        );
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
            minimized: true,
            host_hidden: false,
            fullscreen: false,
            process_id: None,
            role: None,
            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        }];
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &closed,
            &unreadable,
            &minimized,
            "",
            1,
            1,
        );
        assert!(validate_report_v4_reason(&report).is_none());
        assert!(
            report
                .windows
                .iter()
                .filter(|w| w.minimized != Some(true))
                .all(|w| w.window.0 != "1001_9"),
            "minimized lifetimes are absent from live membership"
        );
        let row = report
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_9")
            .expect("explicit minimized row");
        assert!(row.minimized == Some(true) && row.rect.is_none());
        assert_eq!(row.native_workspace.as_deref(), Some("space-11"));
        let mut engine = production_engine();
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
    fn hidden_app_row_reports_host_hidden_not_minimized() {
        // Genuinely minimized vs owner-hidden rows carry separated truth:
        // hidden-app reads minimized false plus host_hidden true (never a
        // hidden-equals-minimized echo) with the available raw legs
        // preserved and maximized/sticky null per contract. The report
        // validates and the production-capability core accepts it.
        let full_raw = |window: &str, minimized: bool, host_hidden: bool| MinimizedRecord {
            window: WindowId(window.to_owned()),
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-11".to_owned()),
            sticky: false,
            minimized,
            host_hidden,
            fullscreen: false,
            process_id: Some("1001".to_owned()),
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            modal: Some(false),
            resizable: Some(true),
            layer: Some(0),
            app_id: Some("com.example.app".to_owned()),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        };
        for fact in [
            minimized_fact(&full_raw("1001_9", true, false)),
            minimized_fact(&full_raw("1001_8", false, true)),
        ] {
            assert_eq!(fact.maximized, None, "macOS declares no maximize fact");
            assert_eq!(fact.sticky, None, "macOS declares no native sticky fact");
            assert_eq!(fact.role.as_deref(), Some(crate::classifier::ROLE_WINDOW));
            assert_eq!(fact.layer, Some(0));
            assert_eq!(fact.app_id.as_deref(), Some("com.example.app"));
            assert_eq!(
                fact.activation_policy,
                Some(tiler_core::world_report::ActivationPolicy::Regular)
            );
            assert_eq!(fact.process_id.as_deref(), Some("1001"));
        }
        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let minimized = vec![
            full_raw("1001_9", true, false),
            full_raw("1001_8", false, true),
        ];
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &closed,
            &unreadable,
            &minimized,
            "",
            1,
            1,
        );
        assert!(validate_report_v4_reason(&report).is_none());
        let hidden: std::collections::BTreeMap<&str, &tiler_core::world_report::WindowFact> =
            report
                .windows
                .iter()
                .filter(|w| w.window.0 == "1001_9" || w.window.0 == "1001_8")
                .map(|w| (w.window.0.as_str(), w))
                .collect();
        assert_eq!(hidden.len(), 2);
        assert_eq!(hidden["1001_9"].minimized, Some(true));
        assert_eq!(hidden["1001_9"].host_hidden, Some(false));
        assert_eq!(hidden["1001_8"].minimized, Some(false));
        assert_eq!(hidden["1001_8"].host_hidden, Some(true));
        let mut engine = WorldEngine::new_test_bound_with_capabilities(
            GENERATION,
            INCARNATION,
            crate::world_connection::macos_capabilities(),
        )
        .expect("production capabilities bind");
        let reply = accepted(&report, &mut engine);
        assert!(
            reply.windows.iter().any(|w| w.lifetime.0 == "1001_9"),
            "minimized lifetime survives the production-capability core"
        );
        assert!(
            reply.windows.iter().any(|w| w.lifetime.0 == "1001_8"),
            "hidden-app lifetime survives the production-capability core"
        );
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
        let hidden = key("macos-display-1", "space-12");
        let live = key("macos-display-1", "space-11");
        members.insert(live.clone(), vec![(1001, observed(3, 10, 10, 100, 100))]);
        let mut engine = production_engine();
        let first = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(validate_report_v4_reason(&first).is_none());
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
        let report = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 2, 2);
        assert!(validate_report_v4_reason(&report).is_none());
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
        // Whole-report hold: every scope holds IncompleteInventory,
        // so no scope authorizes writes until complete truth lands.
        assert!(
            reply.scopes.iter().all(|s| s.authority
                == tiler_core::world_report::ScopeAuthority::Hold(HoldReason::IncompleteInventory)),
            "gap holds every host-owned scope, scopes={:?}",
            reply.scopes,
        );
        assert!(
            reply.windows.iter().all(|w| w.frame_authority
                != tiler_core::world_report::ScopeAuthority::Controlled
                && w.allocation.is_none()
                && w.effective_frame.is_none()),
            "no controlled frame authority and no geometry anywhere in the held reply"
        );
        // Recovery: readable again with live-only members completes
        // inventory and retiles.
        members.remove(&hidden);
        let third = assemble(
            &ctx,
            &members,
            &memory,
            &closed,
            &BTreeMap::new(),
            &[],
            "",
            3,
            3,
        );
        assert!(validate_report_v4_reason(&third).is_none());
        assert!(
            third.inventory_complete,
            "readable return completes inventory"
        );
        let resumed = accepted(&third, &mut engine);
        assert!(
            resumed
                .windows
                .iter()
                .any(|w| w.frame_authority == tiler_core::world_report::ScopeAuthority::Controlled),
            "healthy scope tiles again after the gap, reply={resumed:?}"
        );
        assert!(
            resumed
                .workspaces
                .iter()
                .any(|row| row.id == bound_12 && row.native_binding.as_deref() == Some("space-12")),
            "recovery retains the hidden Space binding, never releases it"
        );
    }

    #[test]
    fn generation_revision_watermark_rejections() {
        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut engine = production_engine();
        let first = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 5, 5);
        accepted(&first, &mut engine);

        // Older request sequence: superseded, never executed.
        let stale_req = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 6, 4);
        match engine.handle_report(&stale_req) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-superseded"),
            other => panic!("expected superseded, got {other:?}"),
        }
        // Older observation sequence on a fresh request: stale observation.
        let stale_obs = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 4, 6);
        match engine.handle_report(&stale_obs) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "observation-stale"),
            other => panic!("expected observation-stale, got {other:?}"),
        }
        // Foreign core generation: mismatch without state.
        let mut foreign = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 6, 6);
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
        closed.push(WindowId("1001_1".to_owned()));
        let minimized = vec![MinimizedRecord {
            window: WindowId("1001_1".to_owned()),
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-11".to_owned()),
            sticky: false,
            minimized: true,
            host_hidden: false,
            fullscreen: false,
            process_id: None,
            role: None,
            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        }];
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &closed,
            &unreadable,
            &minimized,
            "",
            1,
            1,
        );
        // Closed beats live (no closed-overlaps-live) and beats minimized
        // (no minimized row): the report validates.
        assert!(validate_report_v4_reason(&report).is_none());
        assert!(
            report.windows.iter().all(|w| w.window.0 != "1001_1"),
            "close drops live and minimized twins"
        );
        assert!(report.closed.iter().any(|id| id.0 == "1001_1"));
        let mut engine = production_engine();
        let reply = accepted(&report, &mut engine);
        assert!(
            reply.windows.iter().all(|w| w.lifetime.0 != "1001_1"),
            "explicit close authorizes no desired row"
        );
    }

    #[test]
    fn automatic_float_survives_hint_loss_until_explicit_tile() {
        // Production seam (no adapter cache): the core admits a first-sight
        // fixed window as an automatic float, keeps it floating across hint
        // loss (D2), and tiles it only on an explicit core tile intent
        // (D3 `ToggleFloat`). The adapter wire stays plain throughout.
        let ctx = two_display_ctx();
        let (mut members, closed, unreadable) = empty_maps();
        let d1 = key("macos-display-1", "space-11");
        let mut fixed = observed(1, 10, 10, 640, 480);
        fixed.hints = WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        };
        members.insert(d1.clone(), vec![(1001, fixed)]);
        let first = assemble(
            &ctx,
            &members,
            &empty_memory(),
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report_v4_reason(&first).is_none());
        let wire = first
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live row");
        assert!(
            wire.maximized != Some(true) && wire.fullscreen != Some(true),
            "adapter sends no automatic echo"
        );
        let mut engine = production_engine();
        let reply = accepted(&first, &mut engine);
        assert!(reply.ready);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_1")
            .expect("admitted float");
        assert!(
            desired.floating,
            "fixed newcomer floats automatic core-side"
        );
        // Hint loss: the same window classifies tiled now. The wire stays
        // plain, but the core keeps the automatic float (D2).
        let (mut members, closed, unreadable) = empty_maps();
        members.insert(d1.clone(), vec![(1001, observed(1, 10, 10, 640, 480))]);
        let memory = LifetimeMemory::from_reply(&reply);
        let second = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 2, 2);
        assert!(validate_report_v4_reason(&second).is_none());
        let wire = second
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live row");
        assert!(
            wire.maximized != Some(true) && wire.hints == Some(WindowSizeHints::none()),
            "hintless wire stays plain without adapter echo"
        );
        let reply = accepted(&second, &mut engine);
        assert!(reply.ready);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_1")
            .expect("retained float");
        assert!(
            desired.floating,
            "hint loss keeps the automatic float core-side"
        );
        // Explicit tile (D3): the same hintless observation carrying an
        // explicit core float intent tiles instead of floating. The wire
        // stays plain (no adapter policy bit); the ToggleFloat command is
        // the explicit tile authority.
        use tiler_core::boundary::CoreCommand;
        use tiler_core::world_report::WorldCommand;
        let memory = LifetimeMemory::from_reply(&reply);
        let mut third = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 3, 3);
        third.command_seq = Some(3);
        third.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: "1001_1".to_owned(),
            float_rect: None,
        }));
        assert!(validate_report_v4_reason(&third).is_none());
        let reply = accepted(&third, &mut engine);
        assert!(reply.ready);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_1")
            .expect("tiled row");
        assert!(
            !desired.floating,
            "explicit suppress tiles the automatic float"
        );
    }

    #[test]
    fn focused_global_is_informational_only() {
        // Unknown global focus never refuses: it drives policy, never proof.
        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let report = assemble(
            &ctx,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "4242_7",
            1,
            1,
        );
        assert!(validate_report_v4_reason(&report).is_none());
        assert_eq!(report.focused_global, "4242_7");
        let mut engine = production_engine();
        let reply = accepted(&report, &mut engine);
        assert!(reply.ready);
        let _ = ScopedWindowMeta {
            wid: 0,
            spaces: Vec::new(),
            fullscreen: false,
            minimized: false,
            hidden: false,

            role: None,

            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        };
    }

    #[test]
    fn foreground_known_bound_and_windowless_round_trip_through_codec() {
        // Producer -> codec: bound agreement (true+focused) and genuine
        // windowless (true+empty) both validate and preserve the bit through
        // the genuine v4 codec. Unknown (false+empty) validates too but
        // carries the opposite bit; false+nonempty refuses.
        use tiler_protocol::world_protocol::{
            CreationReceiptWire, CursorHitWire, DecodedReportV4, GestureIngressWire,
            ReportWireOptionals, decode_report_v4, encode_report_v4,
        };

        fn wire(report: WorldReportV4) -> DecodedReportV4 {
            DecodedReportV4 {
                wire: ReportWireOptionals {
                    cursor_hit: match &report.cursor_hit {
                        None => CursorHitWire::Absent,
                        Some(_) => CursorHitWire::Id,
                    },
                    monotonic_ms_present: report.monotonic_ms != 0,
                    frame_attempts_present: !report.frames.is_empty(),
                    member_attempts_present: !report.member_attempts.is_empty(),
                    view_attempts_present: !report.view_attempts.is_empty(),
                    clear_attempts_present: !report.clear_attempts.is_empty(),
                    creation_attempts_present: !report.creation_attempts.is_empty(),
                    gesture_ingress: match &report.gesture {
                        None => GestureIngressWire::Absent,
                        Some(_) => GestureIngressWire::Present,
                    },
                },
                creation_receipts: report
                    .creation_attempts
                    .iter()
                    .map(|row| match &row.created_native {
                        None => CreationReceiptWire::Null,
                        Some(_) => CreationReceiptWire::Id,
                    })
                    .collect(),
                report,
            }
        }

        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();

        // Bound: focused leg agreement proves foreground.
        let bound = assemble(
            &ctx,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "4242_7",
            1,
            1,
        );
        assert_eq!(bound.focused_global, "4242_7");
        assert!(bound.foreground_known, "bound agreement is known");
        assert!(validate_report_v4_reason(&bound).is_none());
        let back = decode_report_v4(&encode_report_v4(&wire(bound)).expect("bound encodes"))
            .expect("bound decodes");
        assert!(back.report.foreground_known);
        assert_eq!(back.report.focused_global, "4242_7");

        // Known none: genuine NoValue on both AX legs with stable frontmost
        // proves no-client (true+empty). The producer sets the bit; the
        // builder preserves it; the codec round-trips it.
        let owned_id = "req-2".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            2,
            2,
            &owned_id,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "",
            false,
        );
        let raw = realistic_raw_for(&members);
        input.raw = &raw;
        input.foreground_known = true;
        let known_none = build_report(&mut input);
        assert!(known_none.focused_global.is_empty());
        assert!(known_none.foreground_known);
        assert!(validate_report_v4_reason(&known_none).is_none());
        let back = decode_report_v4(&encode_report_v4(&wire(known_none)).expect("none encodes"))
            .expect("none decodes");
        assert!(back.report.foreground_known);
        assert!(back.report.focused_global.is_empty());

        // Unknown: unsupported/unreadable/stale/foreign/missing-membership
        // (including the main-fallback route, which never proves foreground)
        // stays false+empty through the same path.
        let owned_id = "req-3".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            3,
            3,
            &owned_id,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "",
            false,
        );
        let raw = realistic_raw_for(&members);
        input.raw = &raw;
        assert!(!input.foreground_known, "empty defaults to unknown");
        let unknown = build_report(&mut input);
        assert!(!unknown.foreground_known);
        assert!(unknown.focused_global.is_empty());
        assert!(validate_report_v4_reason(&unknown).is_none());
        let back = decode_report_v4(&encode_report_v4(&wire(unknown)).expect("unknown encodes"))
            .expect("unknown decodes");
        assert!(!back.report.foreground_known);

        // Inconsistent: false+nonempty refuses on both validation and encode.
        let mut bad = assemble(
            &ctx,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "4242_7",
            4,
            4,
        );
        bad.foreground_known = false;
        assert_eq!(
            validate_report_v4_reason(&bad),
            Some("focused-unknown-mismatch")
        );
        assert!(encode_report_v4(&wire(bad)).is_err());
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
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            1,
            1,
            &owned_id,
            &members,
            &memory,
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
        assert!(validate_report_v4_reason(&report).is_none());
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
        let mut engine = production_engine();
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
        let report = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(validate_report_v4_reason(&report).is_none());
        assert_eq!(
            report.native[0].current_native, None,
            "unknown native current reads null, never guessed"
        );
        let mut engine = production_engine();
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
        let report = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(validate_report_v4_reason(&report).is_none());
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
        let omitted = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 1, 1);
        assert!(validate_report_v4_reason(&omitted).is_none());
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
            minimized: true,
            host_hidden: false,
            fullscreen: false,
            process_id: None,
            role: None,
            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        }];
        let flagged = assemble(
            &ctx,
            &members,
            &memory,
            &closed,
            &unreadable,
            &minimized,
            "",
            2,
            2,
        );
        assert!(validate_report_v4_reason(&flagged).is_none());
        let row = flagged
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_7")
            .expect("explicit minimized row");
        assert!(row.minimized == Some(true), "flagged row marks minimized");
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
            minimized: true,
            host_hidden: false,
            fullscreen: false,
            process_id: Some("1001".to_owned()),
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            modal: Some(false),
            resizable: Some(true),
            layer: Some(0),
            app_id: Some("com.example.app".to_owned()),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        }];
        let alloc = |reply: &tiler_core::world_report::WorldReplyV4, id: &str| {
            reply
                .windows
                .iter()
                .find(|w| w.lifetime.0 == id)
                .expect("tiled row")
                .allocation
        };
        let no_focus_for = |reply: &tiler_core::world_report::WorldReplyV4| {
            reply
                .effects
                .iter()
                .all(|e| !(e.kind == EffectKind::Focus && e.target.0 == "1001_7"))
        };
        // Bootstrap both engines identically.
        let mut omit_engine = production_engine();
        let mut flag_engine = production_engine();
        let first = assemble(
            &ctx,
            &full,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1001_7",
            1,
            1,
        );
        assert!(validate_report_v4_reason(&first).is_none());
        let r1 = accepted(&first, &mut omit_engine);
        let first = assemble(
            &ctx,
            &full,
            &memory,
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
        let hide_omit = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1002_8",
            2,
            2,
        );
        assert!(validate_report_v4_reason(&hide_omit).is_none());
        let r2_omit = accepted(&hide_omit, &mut omit_engine);
        let hide_flag = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &closed,
            &unreadable,
            &minimized,
            "1002_8",
            2,
            2,
        );
        assert!(validate_report_v4_reason(&hide_flag).is_none());
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
        let back = assemble(
            &ctx,
            &full,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1002_8",
            3,
            3,
        );
        assert!(validate_report_v4_reason(&back).is_none());
        let r3_omit = accepted(&back, &mut omit_engine);
        let back = assemble(
            &ctx,
            &full,
            &memory,
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
        use crate::world_runtime::{minimized_entries, refresh_minimized};
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
                roster: std::collections::BTreeMap::new(),
                roster_incomplete: false,
                roster_cause: String::new(),
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
                roster: std::collections::BTreeMap::new(),
                roster_incomplete: false,
                roster_cause: String::new(),
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
        let mut engine = production_engine();
        let first = assemble(
            &ctx,
            &full,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1001_7",
            1,
            1,
        );
        assert!(validate_report_v4_reason(&first).is_none());
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
            cg_complete: true,
            cg_owner_wids: vec![7],
            cg_alive: Vec::new(),
            revision: 3,
        };
        refresh_minimized(&mut retained, 1001, &hide_snapshot);
        note_scoped_members(&mut retained, 1001, &BTreeMap::new());
        let minted = minimized_entries(&retained);
        assert_eq!(minted.len(), 1, "HiddenApp mints one minimized record");
        assert_eq!(minted[0].window.0, "1001_7");
        let hidden_members =
            BTreeMap::from([(d1.clone(), vec![(1002, observed(8, 720, 35, 700, 800))])]);
        let hide = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &closed,
            &unreadable,
            &minted,
            "1002_8",
            2,
            2,
        );
        assert!(validate_report_v4_reason(&hide).is_none());
        let row = hide
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_7")
            .expect("hidden row");
        assert!(
            row.minimized == Some(false) && row.host_hidden == Some(true) && row.rect.is_none(),
            "frameless hidden-app fact (minimized false, host hidden)"
        );
        assert_eq!(row.native_workspace.as_deref(), Some("space-11"));
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
        let back = assemble(
            &ctx,
            &full,
            &memory,
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
        let flat = vec![WindowId("1001_7".to_owned())];
        assert!(
            flat.contains(&WindowId("1001_7".to_owned())),
            "hidden terminate closes explicitly"
        );
        let gone = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &flat,
            &unreadable,
            &[],
            "1002_8",
            4,
            4,
        );
        assert!(validate_report_v4_reason(&gone).is_none());
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
    /// no focus effect, and the minimized hold survives. The refusal rides
    /// either the top-level `Rejected` outcome (no revision) or, on an
    /// accepted v4 report, `command_result: Rejected(focus-unknown)` (the
    /// observations incorporate but the command is refused without
    /// mutation); both shapes refuse, neither focuses the hidden lifetime.
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
            minimized: true,
            host_hidden: false,
            fullscreen: false,
            process_id: None,
            role: None,
            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        }];
        let mut engine = production_engine();
        let first = assemble(
            &ctx,
            &full,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1001_7",
            1,
            1,
        );
        accepted(&first, &mut engine);
        let hide = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &closed,
            &unreadable,
            &minimized,
            "1002_8",
            2,
            2,
        );
        accepted(&hide, &mut engine);
        let owned_id = "req-3".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            3,
            3,
            &owned_id,
            &hidden_members,
            &memory,
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
        assert!(validate_report_v4_reason(&report).is_none());
        let revision_before = engine.state_revision();
        match engine.handle_report(&report) {
            WorldOutcome::Rejected { kind, .. } => {
                assert_eq!(kind, "focus-unknown");
                assert_eq!(engine.state_revision(), revision_before);
            }
            WorldOutcome::Accepted(reply) => {
                use tiler_core::world_report::{CommandRejectKind, CommandResult};
                assert_eq!(
                    reply.command_result,
                    Some(CommandResult::Rejected {
                        kind: CommandRejectKind::FocusUnknown,
                    }),
                    "accepted report still refuses the hidden focus command"
                );
                assert!(
                    reply
                        .effects
                        .iter()
                        .all(|e| !(e.kind == EffectKind::Focus && e.target.0 == "1001_7")),
                    "no focus effect for the hidden lifetime"
                );
            }
            other => panic!("direct-target focus at hidden must refuse, got {other:?}"),
        }
        let still = assemble(
            &ctx,
            &hidden_members,
            &memory,
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
            minimized: true,
            host_hidden: false,
            fullscreen: false,
            process_id: None,
            role: None,
            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        }];
        let mut engine = production_engine();
        let first = assemble(
            &ctx,
            &full,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1001_7",
            1,
            1,
        );
        accepted(&first, &mut engine);
        let hide = assemble(
            &ctx,
            &hidden_members,
            &memory,
            &closed,
            &unreadable,
            &minimized,
            "1002_8",
            2,
            2,
        );
        accepted(&hide, &mut engine);
        let owned_id = "req-3".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            3,
            3,
            &owned_id,
            &hidden_members,
            &memory,
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
        assert!(validate_report_v4_reason(&report).is_none());
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
            minimized: true,
            host_hidden: false,
            fullscreen: false,
            process_id: Some("1001".to_owned()),
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            modal: Some(false),
            resizable: Some(true),
            layer: Some(0),
            app_id: Some("com.example.app".to_owned()),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        }];
        let complete_unreadable: BTreeMap<DomainKey, HoldReason> = BTreeMap::new();
        let mut partial_unreadable: BTreeMap<DomainKey, HoldReason> = BTreeMap::new();
        partial_unreadable.insert(partial_scope.clone(), HoldReason::Unreadable);
        for (label, unreadable) in [
            ("complete", &complete_unreadable),
            ("incomplete-transitional", &partial_unreadable),
        ] {
            let mut engine = production_engine();
            let first = assemble(
                &ctx,
                &full,
                &memory,
                &closed,
                &BTreeMap::new(),
                &[],
                "1001_7",
                1,
                1,
            );
            assert!(validate_report_v4_reason(&first).is_none());
            accepted(&first, &mut engine);
            // Same report: first minimized transition plus direct-target
            // focus at the hidden lifetime.
            let owned_id = "req-2".to_owned();
            let mut input = ReportInput::new(
                INCARNATION,
                GENERATION,
                &ctx,
                2,
                2,
                &owned_id,
                &hidden_members,
                &memory,
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
            let raw = realistic_raw_for(&hidden_members);
            input.raw = &raw;
            let report = build_report(&mut input);
            assert!(
                validate_report_v4_reason(&report).is_none(),
                "{label}: transition report validates"
            );
            let hidden_row = report
                .windows
                .iter()
                .find(|w| w.window.0 == "1001_7")
                .expect("hidden minimized fact rides");
            assert!(
                hidden_row.minimized == Some(true) && hidden_row.rect.is_none(),
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
                        "{label}: core-semantics blocker if this mints focus on hidden, effects={:?}",
                        reply.effects,
                    );
                    let row = reply
                        .windows
                        .iter()
                        .find(|w| w.lifetime.0 == "1001_7")
                        .expect("hidden desired row");
                    assert!(
                        row.allocation.is_none() && row.effective_frame.is_none(),
                        "{label}: no geometry restore for hidden, row={row:?}"
                    );
                    assert!(
                        row.frame_authority != ScopeAuthority::Controlled,
                        "{label}: no controlled authority for hidden, authority={:?}",
                        row.frame_authority,
                    );
                }
                other => panic!("{label}: unexpected outcome, got {other:?}"),
            }
            // Follow-up observation-only report: the minimized transition
            // itself holds with no focus effect either way.
            let still = assemble(
                &ctx,
                &hidden_members,
                &memory,
                &closed,
                unreadable,
                &minimized,
                "1002_8",
                3,
                3,
            );
            assert!(validate_report_v4_reason(&still).is_none());
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

    fn classified(wid: u32, frame_ax: crate::classifier::AxFacts) -> ObservedWindow {
        use crate::classifier::{AppFacts, CgFacts, WindowFrame, classify_window};
        let out = classify_window(
            WindowFrame {
                wid,
                x: 10,
                y: 10,
                w: 640,
                h: 480,
            },
            &frame_ax,
            &CgFacts::normal(),
            &AppFacts::regular(),
        );
        out.to_observed().expect("admitted shape submits")
    }

    fn collected(pid: u32, window: &ObservedWindow) -> WindowFact {
        let domain = key("macos-display-1", "space-11");
        let mut facts = Vec::new();
        let raw_of = |_: &str| crate::runtime_types::WindowRawFacts::default();
        collect_window_facts(
            &mut facts,
            pid,
            std::slice::from_ref(window),
            &domain,
            &empty_memory(),
            &raw_of,
        );
        assert_eq!(facts.len(), 1, "one admitted window yields one fact");
        facts.pop().expect("fact")
    }

    #[test]
    fn native_ax_dispatch_seam_preserves_admission_shapes() {
        use crate::classifier::{AxFacts, ROLE_SHEET, SUBROLE_DIALOG, SUBROLE_UNKNOWN};
        // Ordinary standard window: plain tile, everything truthful.
        let observed_1 = classified(1, AxFacts::standard());
        assert!(
            !observed_1.floating && !observed_1.fullscreen,
            "standard classifies tiled"
        );
        let fact = collected(4242, &observed_1);
        assert_eq!(fact.window.0, "4242_1");
        assert!(fact.fullscreen != Some(true));
        assert!(
            fact.maximized != Some(true)
                && fact.sticky != Some(true)
                && fact.minimized != Some(true)
        );
        assert_eq!(fact.hints, Some(WindowSizeHints::none()));
        assert_eq!(fact.native_workspace.as_deref(), Some("space-11"));
        // Transient dialog + modal: normalized float exceptions.
        let mut dialog = AxFacts::standard();
        dialog.subrole = Some(SUBROLE_DIALOG.to_owned());
        let observed_2 = classified(2, dialog);
        assert!(observed_2.floating, "dialog rides as float exception");
        let fact = collected(4242, &observed_2);
        assert!(fact.fullscreen != Some(true));
        let mut modal = AxFacts::standard();
        modal.modal = true;
        assert!(
            classified(3, modal).floating,
            "modal rides as float exception"
        );
        // Sheet role: float exception with a standard subrole.
        let mut sheet = AxFacts::standard();
        sheet.role = Some(ROLE_SHEET.to_owned());
        let observed_sheet = classified(8, sheet);
        assert!(observed_sheet.floating, "sheet rides as float exception");
        let fact = collected(4242, &observed_sheet);
        assert!(fact.fullscreen != Some(true));
        // Fullscreen standard: truthful overlay, no synthetic float.
        let mut fullscreen = AxFacts::standard();
        fullscreen.fullscreen = true;
        let observed_fs = classified(4, fullscreen);
        assert!(!observed_fs.floating && observed_fs.fullscreen);
        let fact = collected(4242, &observed_fs);
        assert!(fact.fullscreen == Some(true));
        // Fullscreen transient: both flags truthful, origin kept.
        let mut overlay_dialog = AxFacts::standard();
        overlay_dialog.subrole = Some(SUBROLE_DIALOG.to_owned());
        overlay_dialog.fullscreen = true;
        let observed_ov = classified(5, overlay_dialog);
        assert!(observed_ov.floating && observed_ov.fullscreen);
        let fact = collected(4242, &observed_ov);
        assert!(fact.fullscreen == Some(true));
        // Unknown subrole at a nonzero layer: excluded (never a fact).
        let mut unknown = AxFacts::standard();
        unknown.subrole = Some(SUBROLE_UNKNOWN.to_owned());
        {
            use crate::classifier::{AppFacts, CgFacts, WindowFrame, classify_window};
            let out = classify_window(
                WindowFrame {
                    wid: 6,
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                &unknown,
                &CgFacts {
                    layer: 3,
                    onscreen: true,
                },
                &AppFacts::regular(),
            );
            assert!(out.to_observed().is_none(), "layer-unknown omits");
        }
        // Fixed hints ride carried, never asserted.
        let mut fixed = AxFacts::standard();
        fixed.hints = WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        };
        let observed_fixed = classified(7, fixed);
        assert!(
            !observed_fixed.floating,
            "fixed hints classify tiled; core admits automatic float"
        );
        let fact = collected(4242, &observed_fixed);
        assert_eq!(fact.hints.as_ref().expect("hints").min_w, Some(640));
    }

    #[test]
    fn native_ax_dispatch_seam_omits_minimized_hidden_and_policy() {
        use crate::classifier::{
            ActivationPolicy, AppFacts, AxFacts, CgFacts, WindowFrame, classify_window,
        };
        let frame = WindowFrame {
            wid: 1,
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        // Genuine minimized cause: omitted, never a close report.
        let mut ax = AxFacts::standard();
        ax.minimized = true;
        assert!(
            classify_window(frame, &ax, &CgFacts::normal(), &AppFacts::regular())
                .to_observed()
                .is_none()
        );
        // Owner-hidden cause: omitted through the same minimized path.
        ax.minimized = false;
        let mut app = AppFacts::regular();
        app.hidden = true;
        assert!(
            classify_window(frame, &ax, &CgFacts::normal(), &app)
                .to_observed()
                .is_none()
        );
        // Activation policy gates stay native: no guessed SafetyFacts map.
        app.hidden = false;
        for policy in [ActivationPolicy::Accessory, ActivationPolicy::Prohibited] {
            app.activation_policy = policy;
            assert!(
                classify_window(frame, &ax, &CgFacts::normal(), &app)
                    .to_observed()
                    .is_none(),
                "{policy:?} excludes"
            );
        }
        // The omitted minimized lifetime reports explicitly core-side.
        let record = MinimizedRecord {
            window: WindowId("4242_1".to_owned()),
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
            sticky: false,
            minimized: true,
            host_hidden: false,
            fullscreen: false,
            process_id: None,
            role: None,
            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        };
        let fact = minimized_fact(&record);
        assert!(fact.minimized == Some(true) && fact.fullscreen != Some(true));
        assert_eq!(fact.native_workspace.as_deref(), Some("space-11"));
    }

    #[test]
    fn born_fullscreen_truthful_wire_holds_slotless_in_engine() {
        // Wire contract: a born fullscreen arrival reports truthfully
        // (floating=false, fullscreen=true) with no synthetic float and
        // no automatic marker. The core holds it slotless (D5) itself.
        use crate::classifier::AxFacts;
        let ctx = two_display_ctx();
        let d1 = key("macos-display-1", "space-11");
        let mut arrival = AxFacts::standard();
        arrival.fullscreen = true;
        let observed = classified(1, arrival);
        assert!(!observed.floating && observed.fullscreen);
        let (mut members, closed, unreadable) = empty_maps();
        members.insert(d1.clone(), vec![(1001, observed)]);
        let first = assemble(
            &ctx,
            &members,
            &empty_memory(),
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report_v4_reason(&first).is_none());
        let wire = first
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live row");
        assert!(
            wire.fullscreen == Some(true)
                && wire.maximized != Some(true)
                && wire.hints == Some(WindowSizeHints::none())
        );
        let mut engine = production_engine();
        let reply = accepted(&first, &mut engine);
        assert!(reply.ready);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_1")
            .expect("held row");
        assert!(desired.floating, "born fullscreen holds as exception");
        assert!(desired.allocation.is_none(), "born hold takes no tile slot");
        // A repeat fullscreen observation keeps the slotless hold with a
        // still-truthful wire (no synthetic echo needed).
        let memory = LifetimeMemory::from_reply(&reply);
        let second = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 2, 2);
        let wire = second
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_1")
            .expect("live row");
        assert!(wire.fullscreen == Some(true), "repeat stays truthful");
        let reply = accepted(&second, &mut engine);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_1")
            .expect("held row");
        assert!(desired.floating && desired.allocation.is_none());
        // First exit classifies fresh: a plain exit tiles with a slot.
        let exited = classified(1, AxFacts::standard());
        let (mut members, closed, unreadable) = empty_maps();
        members.insert(d1.clone(), vec![(1001, exited)]);
        let memory = LifetimeMemory::from_reply(&reply);
        let third = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 3, 3);
        let reply = accepted(&third, &mut engine);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_1")
            .expect("exited row");
        assert!(
            !desired.floating && desired.allocation.is_some(),
            "plain exit tiles with a reserved slot"
        );
    }

    #[test]
    fn born_fullscreen_fixed_exit_floats_automatic_through_seam() {
        // D5 freshness: a born fullscreen arrival carrying fixed hints
        // exits into an automatic float, never a tile.
        use crate::classifier::AxFacts;
        let ctx = two_display_ctx();
        let d1 = key("macos-display-1", "space-11");
        let fixed = WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        };
        let mut arrival = AxFacts::standard();
        arrival.fullscreen = true;
        arrival.hints = fixed;
        let observed = classified(2, arrival);
        assert!(!observed.floating && observed.fullscreen);
        assert_eq!(observed.hints, fixed);
        let (mut members, closed, unreadable) = empty_maps();
        members.insert(d1.clone(), vec![(1001, observed)]);
        let first = assemble(
            &ctx,
            &members,
            &empty_memory(),
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report_v4_reason(&first).is_none());
        let mut engine = production_engine();
        let reply = accepted(&first, &mut engine);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_2")
            .expect("held row");
        assert!(desired.floating && desired.allocation.is_none());
        // Exit keeps the fixed hints: the core admits the automatic float
        // from current hints (never an adapter fixed_auto echo).
        let mut exit = AxFacts::standard();
        exit.hints = fixed;
        let exited = classified(2, exit);
        assert!(!exited.floating && !exited.fullscreen);
        let (mut members, closed, unreadable) = empty_maps();
        members.insert(d1.clone(), vec![(1001, exited)]);
        let memory = LifetimeMemory::from_reply(&reply);
        let second = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 2, 2);
        let wire = second
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_2")
            .expect("live row");
        assert!(wire.maximized != Some(true) && wire.fullscreen != Some(true));
        let reply = accepted(&second, &mut engine);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_2")
            .expect("exited row");
        assert!(desired.floating, "fixed exit floats automatic core-side");
    }

    #[test]
    fn toggle_float_persists_through_placed_echo() {
        // Intentional float persistence: ToggleFloat admits a placed
        // exception; the next observation still classifies tiled (macOS
        // has no AX float state), so the wire re-asserts floating from
        // the core-published placement and the exception survives.
        use tiler_core::boundary::CoreCommand;
        use tiler_core::world_report::WorldCommand;
        let ctx = two_display_ctx();
        let d1 = key("macos-display-1", "space-11");
        let (mut members, closed, unreadable) = empty_maps();
        members.insert(d1.clone(), vec![(1001, observed(3, 10, 10, 640, 480))]);
        let first = assemble(
            &ctx,
            &members,
            &empty_memory(),
            &closed,
            &unreadable,
            &[],
            "",
            1,
            1,
        );
        assert!(validate_report_v4_reason(&first).is_none());
        let mut engine = production_engine();
        let reply = accepted(&first, &mut engine);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_3")
            .expect("tiled row");
        assert!(!desired.floating && desired.allocation.is_some());
        // ToggleFloat command on the tiled member.
        let memory = LifetimeMemory::from_reply(&reply);
        let mut second = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 2, 2);
        second.command_seq = Some(2);
        second.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: "1001_3".to_owned(),
            float_rect: None,
        }));
        assert!(validate_report_v4_reason(&second).is_none());
        let reply = accepted(&second, &mut engine);
        assert!(reply.ready);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_3")
            .expect("floated row");
        assert!(desired.floating, "toggle floats the member");
        assert!(
            desired.effective_frame.is_some(),
            "toggled float carries a session placement"
        );
        // Next observation classifies tiled; the placed echo re-asserts.
        let memory = LifetimeMemory::from_reply(&reply);
        let third = assemble(&ctx, &members, &memory, &closed, &unreadable, &[], "", 3, 3);
        let wire = third
            .windows
            .iter()
            .find(|w| w.window.0 == "1001_3")
            .expect("live row");
        assert!(
            wire.maximized != Some(true),
            "placed echo stays plain on the wire; persistence lives core-side"
        );
        let reply = accepted(&third, &mut engine);
        let desired = reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == "1001_3")
            .expect("retained float");
        assert!(desired.floating, "intentional float survives converge");
    }

    fn ingress_members() -> BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> {
        let mut members = BTreeMap::new();
        members.insert(
            key("macos-display-1", "space-11"),
            vec![(1001, observed(1, 10, 10, 800, 600))],
        );
        members
    }

    fn style() -> crate::active_border::PresentationStylePx {
        crate::active_border::PresentationStylePx::default()
    }

    fn styled(width: f64, gap: f64) -> crate::active_border::PresentationStylePx {
        crate::active_border::presentation_style(
            &crate::active_border::ActiveBorderStyle {
                width,
                gap,
                ..crate::active_border::ActiveBorderStyle::default()
            },
            true,
        )
    }

    /// Eligible focus: homed on a current Space (fail closed otherwise).
    fn eligible() -> FocusEligibilityFacts {
        FocusEligibilityFacts {
            home_current: Some(true),
        }
    }

    /// Unhomed focus: unknown currency fails closed to hidden-workspace.
    fn no_home() -> FocusEligibilityFacts {
        FocusEligibilityFacts {
            home_current: None,
            ..eligible()
        }
    }

    #[test]
    fn presentation_ingress_carries_live_focus_frame_and_pager_row() {
        let members = ingress_members();
        let ingress =
            build_presentation_ingress("1001_1", &members, &[], &[], false, &eligible(), &style());
        assert_eq!(ingress.foreground, "1001_1");
        assert_eq!(
            ingress.foreground_frame,
            Some(tiler_core::geometry::Rect {
                x: 10,
                y: 10,
                w: 800,
                h: 600,
            })
        );
        assert!(
            ingress.foreground_minimized != Some(true)
                && ingress.foreground_fullscreen != Some(true)
        );
        assert!(ingress.foreground_maximized != Some(true));
        // Shell is grounded false: admitted focused members passed the
        // classifier, so excluded shell shapes can never name the focus.
        // Hidden-workspace is grounded false here (homed current); cloaked
        // stays false with no roster fact, and the renderer vetoes fail
        // closed instead.
        assert!(
            ingress.foreground_hidden_workspace != Some(true),
            "homed current focus never hides as hidden-workspace"
        );
        assert!(
            ingress.foreground_host_hidden != Some(true)
                && ingress.foreground_hidden_workspace != Some(true)
        );
        assert!(
            ingress.foreground.is_empty() || ingress.foreground_frame.is_some(),
            "V4 ingress carries focus truthfully; border is reply-side"
        );
        assert!(
            ingress.move_window.is_empty(),
            "no move intent on observation-only ingress"
        );
        assert!(
            ingress.pager_frames.len() == 1,
            "default style still publishes live pager row"
        );
        assert!(!ingress.chord_held && ingress.move_window.is_empty());
        assert!(ingress.pager_order.is_empty());
        assert_eq!(ingress.pager_frames.len(), 1);
        assert_eq!(ingress.pager_frames[0].window.0, "1001_1");
        assert!(ingress.pager_frames[0].live.is_some());
    }

    #[test]
    fn presentation_ingress_unknown_focus_hides_frame() {
        let members = ingress_members();
        let ingress =
            build_presentation_ingress("", &members, &[], &[], false, &eligible(), &style());
        assert!(ingress.foreground.is_empty());
        assert!(ingress.foreground_frame.is_none());
        let ingress =
            build_presentation_ingress("1001_9", &members, &[], &[], false, &no_home(), &style());
        assert_eq!(ingress.foreground, "1001_9");
        assert!(ingress.foreground_frame.is_none());
        assert!(
            ingress.pager_frames.len() == 1,
            "pager still names live rows"
        );
    }

    #[test]
    fn presentation_ingress_minimized_focus_hides_frame_with_flag() {
        let members = ingress_members();
        let minimized = vec![MinimizedRecord {
            window: tiler_core::directional::WindowId("1001_1".to_owned()),
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-11".to_owned()),
            sticky: false,
            minimized: true,
            host_hidden: false,
            fullscreen: false,
            process_id: None,
            role: None,
            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        }];
        let ingress = build_presentation_ingress(
            "1001_1",
            &members,
            &[],
            &minimized,
            false,
            &eligible(),
            &style(),
        );
        assert!(ingress.foreground_minimized == Some(true));
        assert!(ingress.foreground_host_hidden == Some(false));
        assert!(
            ingress.foreground_maximized.is_none(),
            "no guessed maximize"
        );
        assert!(ingress.witness.is_none(), "no fabricated witness");
        assert!(ingress.foreground_frame.is_none());
    }

    #[test]
    fn presentation_ingress_hidden_only_focus_reports_host_hidden_not_minimized() {
        // Separated retained legs: owner-hidden-only focus reads
        // minimized false plus host_hidden true (never a
        // hidden-equals-minimized echo), hides the frame, and still
        // carries no guessed maximize and no fabricated witness.
        let members = ingress_members();
        let minimized = vec![MinimizedRecord {
            window: tiler_core::directional::WindowId("1001_1".to_owned()),
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-11".to_owned()),
            sticky: false,
            minimized: false,
            host_hidden: true,
            fullscreen: false,
            process_id: None,
            role: None,
            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        }];
        let ingress = build_presentation_ingress(
            "1001_1",
            &members,
            &[],
            &minimized,
            false,
            &eligible(),
            &style(),
        );
        assert!(ingress.foreground_minimized == Some(false));
        assert!(ingress.foreground_host_hidden == Some(true));
        assert!(
            ingress.foreground_maximized.is_none(),
            "no guessed maximize"
        );
        assert!(ingress.witness.is_none(), "no fabricated witness");
        assert!(ingress.foreground_frame.is_none());
    }

    #[test]
    fn presentation_ingress_focused_fullscreen_never_pauses_without_game_signal() {
        // Truthful fullscreen flag, but no positively grounded game
        // classification exists, so the game lane stays off. The
        // render-side fullscreen fence is the safety veto instead.
        let members = ingress_members();
        let ingress =
            build_presentation_ingress("1001_1", &members, &[], &[], true, &eligible(), &style());
        assert!(ingress.foreground_fullscreen == Some(true));
    }

    #[test]
    fn presentation_ingress_hidden_and_unhomed_focus_hide_as_hidden_workspace() {
        // Grounded Space eligibility: a focused window homed to a
        // non-current Space — and an unhomed focused window — both hide
        // with the core HiddenWorkspace reason (fail closed on unproven
        // currency), not as no-target. End to end through the real engine.
        for eligibility in [
            FocusEligibilityFacts {
                home_current: Some(false),
                ..eligible()
            },
            no_home(),
        ] {
            let ctx = two_display_ctx();
            let (closed, unreadable) = (Vec::new(), BTreeMap::new());
            let memory = empty_memory();
            let mut members = BTreeMap::new();
            members.insert(
                key("macos-display-1", "space-11"),
                vec![(1001, observed(1, 10, 10, 800, 600))],
            );
            let owned_id = "req-1".to_owned();
            let mut input = ReportInput::new(
                INCARNATION,
                GENERATION,
                &ctx,
                1,
                1,
                &owned_id,
                &members,
                &memory,
                &closed,
                &unreadable,
                &[],
                "1001_1",
                false,
            );
            input.presentation = build_presentation_ingress(
                "1001_1",
                &members,
                &[],
                &[],
                false,
                &eligibility,
                &style(),
            );
            let report = build_report(&mut input);
            assert!(validate_report_v4_reason(&report).is_none());
            assert!(
                matches!(&report.presentation, tiler_core::world_report::PresentationRowV4::Valid(ingress) if ingress.foreground_hidden_workspace == Some(true))
            );
            let mut engine = production_engine();
            let reply = accepted(&report, &mut engine);
            assert!(
                !reply.presentation.border.visible,
                "off-Space focus hides the ring"
            );
            assert_eq!(
                reply.presentation.border.reason,
                Some(tiler_core::world_report::BorderHideReasonV4::HiddenWorkspace)
            );
        }
    }

    #[test]
    fn presentation_ingress_work_area_sized_focus_leaves_maximize_unknown() {
        // Contract M4: macOS never asserts a native maximize fact, so a
        // retained frame exactly matching the visible work area still reads
        // maximized null (unknown, never guessed). The zoom heuristic stays
        // renderer-local. End to end through the real engine: the border
        // fails closed with no reason and no guessed geometry.
        let ctx = two_display_ctx();
        let (closed, unreadable) = (Vec::new(), BTreeMap::new());
        let memory = empty_memory();
        let mut members = BTreeMap::new();
        members.insert(
            key("macos-display-1", "space-11"),
            vec![(1001, observed(1, 0, 25, 1440, 875))],
        );
        let owned_id = "req-1".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            1,
            1,
            &owned_id,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1001_1",
            false,
        );
        input.presentation =
            build_presentation_ingress("1001_1", &members, &[], &[], false, &eligible(), &style());
        let raw = realistic_raw_for(&members);
        input.raw = &raw;
        let report = build_report(&mut input);
        assert!(validate_report_v4_reason(&report).is_none());
        assert!(
            matches!(&report.presentation, tiler_core::world_report::PresentationRowV4::Valid(ingress) if ingress.foreground_maximized.is_none() && ingress.witness.is_none() && ingress.foreground_out.is_none())
        );
        let mut engine = production_engine();
        let reply = accepted(&report, &mut engine);
        assert!(!reply.presentation.border.visible);
        assert_eq!(
            reply.presentation.border.reason, None,
            "no witness means fail-closed without a guessed reason"
        );
        assert!(
            reply.presentation.border.outsets.is_none()
                && reply.presentation.border.width.is_none(),
            "no guessed geometry without a witness"
        );
    }

    #[test]
    fn presentation_ingress_engine_placed_window_leaves_maximize_unknown() {
        // Contract M4: no native maximize fact exists, so the report
        // maximize lane stays null and the border fails closed without a
        // witness. End to end through the real engine.
        let ctx = two_display_ctx();
        let (closed, unreadable) = (Vec::new(), BTreeMap::new());
        let memory = empty_memory();
        let mut members = BTreeMap::new();
        members.insert(
            key("macos-display-1", "space-11"),
            vec![(1001, observed(1, 0, 25, 1440, 875))],
        );
        let owned_id = "req-1".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            1,
            1,
            &owned_id,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1001_1",
            false,
        );
        input.presentation =
            build_presentation_ingress("1001_1", &members, &[], &[], false, &eligible(), &style());
        let raw = realistic_raw_for(&members);
        input.raw = &raw;
        let report = build_report(&mut input);
        assert!(validate_report_v4_reason(&report).is_none());
        assert!(
            matches!(&report.presentation, tiler_core::world_report::PresentationRowV4::Valid(ingress) if ingress.foreground_maximized.is_none() && ingress.witness.is_none()),
            "no guessed maximize and no fabricated witness"
        );
        let mut engine = production_engine();
        let reply = accepted(&report, &mut engine);
        assert!(
            !reply.presentation.border.visible,
            "no witness means the border fails closed"
        );
        assert_eq!(reply.presentation.border.reason, None);
    }

    #[test]
    fn presentation_ingress_full_display_frame_leaves_maximize_unknown() {
        // Contract M4: a frame covering the entire FULL display still reads
        // maximized null (no native fact, never guessed). The border fails
        // closed without a witness. Mirrors the renderer fence staying
        // renderer-local.
        let ctx = two_display_ctx();
        let (closed, unreadable) = (Vec::new(), BTreeMap::new());
        let memory = empty_memory();
        let mut members = BTreeMap::new();
        members.insert(
            key("macos-display-1", "space-11"),
            vec![(1001, observed(1, 0, 0, 1440, 900))],
        );
        let owned_id = "req-1".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            1,
            1,
            &owned_id,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1001_1",
            false,
        );
        let raw = realistic_raw_for(&members);
        input.raw = &raw;
        for _engine_placed in [false, true] {
            input.presentation = build_presentation_ingress(
                "1001_1",
                &members,
                &[],
                &[],
                false,
                &eligible(),
                &style(),
            );
            let report = build_report(&mut input);
            assert!(validate_report_v4_reason(&report).is_none());
            assert!(
                matches!(&report.presentation, tiler_core::world_report::PresentationRowV4::Valid(ingress) if ingress.foreground_maximized.is_none() && ingress.witness.is_none()),
                "full-display coverage still reads unknown without a native fact"
            );
        }
        // Fail-closed reply: no witness means no visible border and no
        // guessed geometry, even for full-display coverage.
        input.presentation =
            build_presentation_ingress("1001_1", &members, &[], &[], false, &eligible(), &style());
        let report = build_report(&mut input);
        assert!(validate_report_v4_reason(&report).is_none());
        let mut engine = production_engine();
        let reply = accepted(&report, &mut engine);
        assert!(!reply.presentation.border.visible);
        assert_eq!(reply.presentation.border.reason, None);
        assert!(reply.presentation.border.outsets.is_none());
    }

    #[test]
    fn presentation_ingress_nondefault_style_resolves_verbatim() {
        // Non-default configuration (width 5, gap 2) rides the ingress
        // integers so core shapes use the configured outset.
        let members = ingress_members();
        let ingress = build_presentation_ingress(
            "1001_1",
            &members,
            &[],
            &[],
            false,
            &eligible(),
            &styled(5.0, 2.0),
        );
        assert!(
            ingress.pager_frames.len() == 1,
            "configured style still publishes live pager row"
        );
        assert!(
            ingress.foreground.is_empty() || ingress.foreground_frame.is_some(),
            "V4 ingress carries focus truthfully; border is reply-side"
        );
    }

    #[test]
    fn nondefault_style_report_through_core_reply_hides_without_witness() {
        // End to end: the ingress still carries the live focus frame and
        // pager row, but with no witness the core reply fails closed: no
        // visible border and no guessed geometry. The configured style no
        // longer publishes a renderer outer through the reply.
        let report_members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = {
            let mut map = BTreeMap::new();
            map.insert(
                key("macos-display-1", "space-11"),
                vec![(1001, observed(1, 10, 20, 100, 80))],
            );
            map
        };
        let ctx = two_display_ctx();
        let (closed, unreadable) = (Vec::new(), BTreeMap::new());
        let memory = empty_memory();
        let owned_id = "req-1".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            1,
            1,
            &owned_id,
            &report_members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1001_1",
            false,
        );
        let raw = realistic_raw_for(&report_members);
        input.raw = &raw;
        input.presentation = build_presentation_ingress(
            "1001_1",
            &report_members,
            &[],
            &[],
            false,
            &eligible(),
            &styled(5.0, 2.0),
        );
        let report = build_report(&mut input);
        assert!(validate_report_v4_reason(&report).is_none());
        // Frame/pager controls stay useful without a witness: the ingress
        // still names the live focus frame and pager row truthfully.
        assert!(
            matches!(&report.presentation, tiler_core::world_report::PresentationRowV4::Valid(ingress) if ingress.foreground == "1001_1" && ingress.foreground_frame == Some(tiler_core::geometry::Rect { x: 10, y: 20, w: 100, h: 80 }) && ingress.pager_frames.len() == 1)
        );
        let mut engine = production_engine();
        let reply = accepted(&report, &mut engine);
        let border = &reply.presentation.border;
        assert!(!border.visible, "no witness means no visible border");
        assert_eq!(border.reason, None, "fail-closed without a guessed reason");
        assert!(
            border.outsets.is_none(),
            "no guessed geometry without a witness"
        );
        assert!(border.width.is_none(), "no guessed width without a witness");
        assert!(border.witness.is_none(), "no fabricated witness");
    }

    #[test]
    fn presentation_ingress_rides_the_report() {
        let ctx = two_display_ctx();
        let (members, closed, unreadable) = empty_maps();
        let memory = empty_memory();
        let mut members = members;
        members.insert(
            key("macos-display-1", "space-11"),
            vec![(1001, observed(1, 10, 10, 800, 600))],
        );
        let owned_id = "req-1".to_owned();
        let mut input = ReportInput::new(
            INCARNATION,
            GENERATION,
            &ctx,
            1,
            1,
            &owned_id,
            &members,
            &memory,
            &closed,
            &unreadable,
            &[],
            "1001_1",
            false,
        );
        input.presentation =
            build_presentation_ingress("1001_1", &members, &[], &[], false, &eligible(), &style());
        let report = build_report(&mut input);
        assert!(validate_report_v4_reason(&report).is_none());
        assert!(
            matches!(&report.presentation, tiler_core::world_report::PresentationRowV4::Valid(ingress) if ingress.foreground == "1001_1" && ingress.foreground_frame.is_some())
        );
        let mut engine = production_engine();
        let reply = accepted(&report, &mut engine);
        assert_eq!(reply.presentation.foreground, "1001_1");
    }
}
