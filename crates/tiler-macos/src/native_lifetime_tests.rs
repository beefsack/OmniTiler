#![forbid(unsafe_code)]

//! Per-app persistent native-lifetime tests (host-mechanical only).
//!
//! Production paths only: retained roster helpers, the world builder, and
//! the real [`WorldEngine`](tiler_core::world_engine::WorldEngine) through
//! [`WorldSession`](crate::world_runtime::WorldSession). No live AX, no
//! threads, no core changes. Unknown homes are never fabricated: grounding
//! uses fresh FULL output geometry (unique max overlap) or the fact omits
//! with a per-PID hold.

use std::collections::BTreeMap;
use std::sync::Arc;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::session::DomainKey;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report_v4_checks::validate_report_v4_reason;

use crate::runtime_core::{
    RetainedApp, RetainedCore, RosterEntry, apply_roster_covering, audit_roster_hold,
    ground_unknown_output, mark_roster_incomplete, prune_current_homed_with_oracle,
    snapshot_is_covering,
};
use crate::runtime_types::{
    NativeContext, NativeDomain, ObservedWindow, ScopedBackendSnapshot, ScopedWindowMeta,
};
use crate::world_builder::{ReportInput, build_report};
use crate::world_runtime::WorldSession;

const GENERATION: &str = "gen-1";
const INCARNATION: &str = "adapter-1";

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn two_display_ctx() -> NativeContext {
    NativeContext {
        revision: 7,
        domains: vec![
            NativeDomain {
                cg_id: 1,
                uuid: None,
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId(crate::topology::workspace_key(11)),
                space: 11,
                current: true,
                readable: true,
                bounds: rect(0, 0, 1440, 900),
                visible: rect(0, 25, 1440, 875),
            },
            NativeDomain {
                cg_id: 1,
                uuid: None,
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId(crate::topology::workspace_key(12)),
                space: 12,
                current: false,
                readable: true,
                bounds: rect(0, 0, 1440, 900),
                visible: rect(0, 25, 1440, 875),
            },
            NativeDomain {
                cg_id: 2,
                uuid: None,
                output: OutputId("macos-display-2".to_owned()),
                workspace: WorkspaceId(crate::topology::workspace_key(21)),
                space: 21,
                current: true,
                readable: true,
                bounds: rect(1440, 0, 1440, 900),
                visible: rect(1440, 25, 1440, 875),
            },
        ],
        skipped: Vec::new(),
        detached_spaces: std::collections::BTreeMap::new(),
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

fn scoped_snapshot(
    windows: Vec<ObservedWindow>,
    spaces: Vec<Vec<u64>>,
    excluded: usize,
    revision: u64,
    cg_complete: bool,
    cg_owner_wids: Vec<u32>,
) -> ScopedBackendSnapshot {
    let meta: Vec<ScopedWindowMeta> = windows
        .iter()
        .zip(spaces)
        .map(|(w, s)| ScopedWindowMeta {
            wid: w.wid,
            spaces: s,
            fullscreen: w.fullscreen,
            minimized: false,
            hidden: false,

            role: None,

            subrole: None,
            modal: None,
            resizable: None,
            layer: None,
            app_id: None,
            activation_policy: None,
        })
        .collect();
    ScopedBackendSnapshot {
        windows,
        focused_wid: None,
        focus_cg_owned: false,
        focus_evidence: None,
        sys_focus: None,
        global_focus: None,
        meta,
        excluded,
        excluded_reasons: Vec::new(),
        cg_complete,
        cg_owner_wids,
        cg_alive: Vec::new(),
        revision,
    }
}

fn retained_with_roster(pid: u32, generation: u64) -> RetainedCore {
    let mut retained = RetainedCore::default();
    retained.apps.insert(
        pid,
        RetainedApp {
            generation,
            seq_last: 1,
            cmd_counter: 0,
            windows: Vec::new(),
            focused_wid: None,
            roster: std::collections::BTreeMap::new(),
            roster_incomplete: false,
            roster_cause: String::new(),
        },
    );
    retained
}

fn session_native() -> WorldSession {
    WorldSession::new(
        true,
        crate::focus_route::FocusRoute::Slps,
        Arc::new(crate::world_sequencer::ObservationClock::new()),
        &[],
    )
}

#[test]
fn grounded_unknown_facts_carry_proven_output_ties_and_outside_omit() {
    let ctx = two_display_ctx();
    // Fully inside display 2 grounds there.
    let grounded = observed(1, 1500, 100, 200, 200);
    assert_eq!(
        ground_unknown_output(&grounded, &ctx),
        Some(OutputId("macos-display-2".to_owned())),
        "fresh FULL overlap grounds the proven output"
    );
    // Straddling the seam ties: no fact.
    let tied = observed(2, 1340, 100, 200, 200);
    assert_eq!(ground_unknown_output(&tied, &ctx), None);
    // Outside every FULL rect: no fact.
    let outside = observed(3, 5000, 5000, 10, 10);
    assert_eq!(ground_unknown_output(&outside, &ctx), None);
    // Degenerate frames never ground.
    let flat = observed(4, 1500, 100, 0, 200);
    assert_eq!(ground_unknown_output(&flat, &ctx), None);

    // Builder emits only the grounded row; ties/outside omit with a hold.
    let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    let memory = crate::runtime_core::LifetimeMemory::empty();
    let closed: Vec<WindowId> = Vec::new();
    let unreadable: BTreeMap<DomainKey, tiler_core::world_report::HoldReason> = BTreeMap::new();
    let unknown = vec![(1001u32, grounded), (1001u32, tied), (1001u32, outside)];
    let owned = "req-1".to_owned();
    let mut input = ReportInput::new(
        INCARNATION,
        GENERATION,
        &ctx,
        1,
        1,
        &owned,
        &members,
        &memory,
        &closed,
        &unreadable,
        &[],
        "",
        false,
    );
    input.unknown = &unknown;
    input.roster_incomplete = true;
    let report = build_report(&mut input);
    assert!(validate_report_v4_reason(&report).is_none());
    assert!(!report.inventory_complete, "omitted rows hold inventory");
    let row = report
        .windows
        .iter()
        .find(|w| w.window.0 == "1001_1")
        .expect("grounded unknown surfaces");
    assert_eq!(row.output.as_ref().unwrap().0, "macos-display-2");
    assert_eq!(row.native_workspace.as_deref(), None);
    assert_eq!(
        row.sticky, None,
        "unsupported sticky stays null, never false"
    );
    assert!(
        report.windows.iter().all(|w| w.window.0 != "1001_2"),
        "tied frame mints no fact on either output"
    );
    assert!(
        report.windows.iter().all(|w| w.window.0 != "1001_3"),
        "outside frame mints no fact"
    );
    assert!(
        report
            .windows
            .iter()
            .all(|w| w.output.as_ref().unwrap().0 != "macos-display-1" || w.window.0 != "1001_2"),
        "tie never falls back to the first output"
    );
}

#[test]
fn unknown_fullscreen_grounds_and_holds_without_actuation() {
    let ctx = two_display_ctx();
    let mut retained = retained_with_roster(1001, 1);
    let mut full = observed(1, 1500, 100, 200, 200);
    full.fullscreen = true;
    let snap = scoped_snapshot(vec![full], vec![vec![99]], 0, 7, true, vec![1]);
    let pruned = apply_roster_covering(&mut retained, 1001, &snap, &ctx);
    assert!(pruned.is_empty());
    // No domain homing yet: the audit grounds the frame, so the fact can
    // surface without an incomplete hold from this row alone.
    audit_roster_hold(&mut retained, 1001, &ctx, false);
    let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    let memory = crate::runtime_core::LifetimeMemory::empty();
    let closed: Vec<WindowId> = Vec::new();
    let unreadable: BTreeMap<DomainKey, tiler_core::world_report::HoldReason> = BTreeMap::new();
    let unknown = vec![(1001u32, full)];
    let owned = "req-2".to_owned();
    // Bind the report to the actual session incarnation/generation: the
    // session handshake (production macOS caps) owns the bound tokens, so
    // only they admit the report core-side.
    let mut session = session_native();
    let incarnation = session.incarnation().to_owned();
    let generation = session.sequencer().bound_generation();
    let mut input = ReportInput::new(
        &incarnation,
        &generation,
        &ctx,
        2,
        2,
        &owned,
        &members,
        &memory,
        &closed,
        &unreadable,
        &[],
        "",
        false,
    );
    input.unknown = &unknown;
    input.roster_incomplete = retained.apps[&1001].roster_incomplete;
    let report = build_report(&mut input);
    assert!(validate_report_v4_reason(&report).is_none());
    let row = report
        .windows
        .iter()
        .find(|w| w.window.0 == "1001_1")
        .expect("grounded fullscreen surfaces");
    assert_eq!(row.fullscreen, Some(true));
    assert_eq!(row.native_workspace.as_deref(), None);
    assert_eq!(row.output.as_ref().unwrap().0, "macos-display-2");
    let engine = session.engine_mut();
    match tiler_protocol::world_admission::handle_report_checked(&mut *engine, &report, &[]).outcome
    {
        WorldOutcome::Accepted(reply) => {
            assert!(
                reply.windows.iter().all(|w| w.lifetime.0 != "1001_1"
                    || w.frame_authority != tiler_core::world_report::ScopeAuthority::Controlled
                    || w.effective_frame.is_none()),
                "unknown fullscreen authorizes no controlled geometry"
            );
        }
        other => panic!("expected accepted hold, got {other:?}"),
    }
}

#[test]
fn repeated_unrepresentable_min_hidden_keep_hold_and_identity() {
    let ctx = two_display_ctx();
    let mut retained = retained_with_roster(1001, 1);
    retained.apps.insert(
        1002,
        RetainedApp {
            generation: 1,
            seq_last: 1,
            cmd_counter: 0,
            windows: Vec::new(),
            focused_wid: None,
            roster: std::collections::BTreeMap::new(),
            roster_incomplete: false,
            roster_cause: String::new(),
        },
    );
    // Covering admit of an ungroundable window, then repeated
    // unrepresentable hidden exclusions (no frame at all).
    let outside = observed(1, 5000, 5000, 10, 10);
    let cover = scoped_snapshot(vec![outside], vec![vec![99]], 0, 7, true, vec![1]);
    assert!(apply_roster_covering(&mut retained, 1001, &cover, &ctx).is_empty());
    audit_roster_hold(&mut retained, 1001, &ctx, false);
    assert!(
        retained.apps[&1001].roster_incomplete,
        "outside frame holds"
    );
    for _ in 0..3 {
        // Same-PID hidden exclusion with no frame: ID source persists.
        let mut hidden = scoped_snapshot(Vec::new(), Vec::new(), 1, 7, true, vec![1]);
        hidden
            .excluded_reasons
            .push(crate::runtime_types::ExcludedOmission {
                wid: 1,
                reason: crate::classifier::ExcludeReason::HiddenApp,
            });
        assert!(snapshot_is_covering(
            hidden.windows.is_empty(),
            hidden.excluded > 0
        ));
        assert!(apply_roster_covering(&mut retained, 1001, &hidden, &ctx).is_empty());
        audit_roster_hold(&mut retained, 1001, &ctx, false);
        // Unrelated B clean covering read clears only B.
        let b_cover = scoped_snapshot(
            vec![observed(5, 10, 10, 50, 50)],
            vec![vec![21]],
            0,
            7,
            true,
            vec![5],
        );
        assert!(apply_roster_covering(&mut retained, 1002, &b_cover, &ctx).is_empty());
        assert!(!retained.apps[&1002].roster_incomplete, "B stays clean");
        assert!(
            retained.apps[&1001].roster_incomplete,
            "A hold survives B update"
        );
        assert!(
            retained.apps[&1001].roster.contains_key(&1),
            "A identity retained"
        );
    }
    let mut session = session_native();
    session.mark_coverage_gap();
    session.clear_coverage_gap();
    assert!(
        retained.apps[&1001].roster_incomplete,
        "session gap never clears PID hold"
    );
}

#[test]
fn terminate_emits_flat_close_while_other_app_incomplete() {
    let mut retained = retained_with_roster(1001, 1);
    retained.apps.insert(
        1002,
        RetainedApp {
            generation: 1,
            seq_last: 1,
            cmd_counter: 0,
            windows: Vec::new(),
            focused_wid: None,
            roster: std::collections::BTreeMap::new(),
            roster_incomplete: true,
            roster_cause: "blank-hidden".to_owned(),
        },
    );
    let cover = scoped_snapshot(
        vec![observed(1, 1500, 10, 100, 100)],
        vec![vec![99]],
        0,
        7,
        true,
        vec![1],
    );
    assert!(apply_roster_covering(&mut retained, 1001, &cover, &two_display_ctx()).is_empty());
    let mut flat: std::collections::BTreeSet<WindowId> = std::collections::BTreeSet::new();
    for wid in retained.apps[&1001].roster.keys() {
        flat.insert(WindowId(format!("1001_{wid}")));
    }
    let flat: Vec<WindowId> = flat.into_iter().collect();
    assert_eq!(flat, vec![WindowId("1001_1".to_owned())]);
    let ctx = two_display_ctx();
    let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    let memory = crate::runtime_core::LifetimeMemory::empty();
    let unreadable: BTreeMap<DomainKey, tiler_core::world_report::HoldReason> = BTreeMap::new();
    let owned = "req-9".to_owned();
    let mut input = ReportInput::new(
        INCARNATION,
        GENERATION,
        &ctx,
        9,
        9,
        &owned,
        &members,
        &memory,
        &flat,
        &unreadable,
        &[],
        "",
        false,
    );
    input.roster_incomplete = true;
    let report = build_report(&mut input);
    assert!(validate_report_v4_reason(&report).is_none());
    assert!(!report.inventory_complete);
    assert!(report.closed.iter().any(|id| id.0 == "1001_1"));
}

#[test]
fn covering_close_prunes_and_stale_generation_never_resurrects() {
    let ctx = two_display_ctx();
    let mut retained = retained_with_roster(1001, 1);
    let cover = scoped_snapshot(
        vec![observed(1, 0, 0, 10, 10), observed(2, 0, 0, 10, 10)],
        vec![vec![11], vec![11]],
        0,
        7,
        true,
        vec![1, 2],
    );
    assert!(apply_roster_covering(&mut retained, 1001, &cover, &ctx).is_empty());
    let close_one = scoped_snapshot(
        vec![observed(2, 0, 0, 10, 10)],
        vec![vec![11]],
        0,
        7,
        true,
        vec![2],
    );
    let pruned = apply_roster_covering(&mut retained, 1001, &close_one, &ctx);
    assert_eq!(pruned, vec![WindowId("1001_1".to_owned())]);
    assert!(!retained.apps[&1001].roster.contains_key(&1));
    // Relaunch clears the roster; a stale gen-1 read for the reused WID
    // must not resurrect under the new generation (callers fence it).
    if let Some(app) = retained.apps.get_mut(&1001) {
        app.generation = 2;
        app.roster.clear();
        app.roster_incomplete = false;
        app.roster_cause.clear();
    }
    assert!(!retained.apps[&1001].roster.contains_key(&1));
    // A stale generation refusal leaves the new lifetime's gaps alone.
    mark_roster_incomplete(&mut retained, 1001, "new-gap");
    assert!(retained.apps[&1001].roster_incomplete);
}

#[test]
fn explicit_close_beats_minimized_and_live() {
    let ctx = two_display_ctx();
    let live = observed(1, 10, 10, 800, 600);
    let key = DomainKey {
        output: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId(crate::topology::workspace_key(11)),
    };
    let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> =
        BTreeMap::from([(key.clone(), vec![(1001u32, live)])]);
    let memory = crate::runtime_core::LifetimeMemory::empty();
    let closed = vec![WindowId("1001_1".to_owned())];
    let minimized = vec![crate::runtime_core::MinimizedRecord {
        window: WindowId("1001_1".to_owned()),
        output: key.output.clone(),
        workspace: key.workspace.clone(),
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
    let unreadable: BTreeMap<DomainKey, tiler_core::world_report::HoldReason> = BTreeMap::new();
    let owned = "req-3".to_owned();
    let mut input = ReportInput::new(
        INCARNATION,
        GENERATION,
        &ctx,
        3,
        3,
        &owned,
        &members,
        &memory,
        &closed,
        &unreadable,
        &minimized,
        "",
        false,
    );
    let report = build_report(&mut input);
    assert!(validate_report_v4_reason(&report).is_none());
    assert!(report.windows.iter().all(|w| w.window.0 != "1001_1"));
    assert!(report.closed.iter().any(|id| id.0 == "1001_1"));
}

#[test]
fn pending_close_survives_no_report_and_drains_only_on_incorporation() {
    let mut session = session_native();
    session.pending_closes.insert(WindowId("1001_7".to_owned()));
    let retained = RetainedCore::default();
    // Empty native context builds no report: the pending close survives.
    let empty_ctx = NativeContext {
        revision: 7,
        domains: Vec::new(),
        skipped: Vec::new(),
        detached_spaces: std::collections::BTreeMap::new(),
    };
    let dispatch = crate::world_runtime::DispatchContext::Native(Arc::new(empty_ctx));
    let report = crate::world_runtime::build_dispatch_report(
        &mut session,
        &retained,
        &dispatch,
        &[],
        1,
        1,
        None,
        None,
        None,
        "req-1",
    );
    assert!(report.is_none(), "empty context reports nothing");
    assert!(
        session
            .pending_closes
            .contains(&WindowId("1001_7".to_owned())),
        "no-report never drains pending closes"
    );
    // Next real dispatch carries the flat close; draining only the
    // incorporated ids keeps unrelated pending ids intact.
    session.pending_closes.insert(WindowId("1002_8".to_owned()));
    let ctx = two_display_ctx();
    let dispatch = crate::world_runtime::DispatchContext::Native(Arc::new(ctx));
    let report = crate::world_runtime::build_dispatch_report(
        &mut session,
        &retained,
        &dispatch,
        &[],
        2,
        2,
        None,
        None,
        None,
        "req-2",
    )
    .expect("real context reports");
    assert!(report.closed.iter().any(|id| id.0 == "1001_7"));
    assert!(report.closed.iter().any(|id| id.0 == "1002_8"));
    // Simulate incorporation of only the first id: the second survives.
    for id in &report.closed {
        if id.0 == "1001_7" {
            session.pending_closes.remove(id);
        }
    }
    assert!(
        !session
            .pending_closes
            .contains(&WindowId("1001_7".to_owned()))
    );
    assert!(
        session
            .pending_closes
            .contains(&WindowId("1002_8".to_owned()))
    );
}

#[test]
fn fullscreen_current_derives_ordered_users_with_full_and_work_screen() {
    // Real producer snapshot: fullscreen current on the SLS row. The
    // derived context keeps user order, sets no current, and keeps the
    // display with FULL plus work geometry.
    use crate::ffi::{DisplayRect, NsDisplay, SlsDisplayRow, SlsSpaceType};
    use crate::topology::build_snapshot;
    let ns = vec![NsDisplay {
        cg_id: 1,
        uuid: Some("A1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6".to_owned()),
        frame: DisplayRect {
            x: 0.0,
            y: 0.0,
            w: 1440.0,
            h: 900.0,
        },
        visible: DisplayRect {
            x: 0.0,
            y: 25.0,
            w: 1440.0,
            h: 875.0,
        },
        scale: 2.0,
    }];
    let rows = vec![SlsDisplayRow {
        identifier: Some("A1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6".to_owned()),
        spaces: vec![11, 12],
        current: Some(99),
    }];
    let snapshot = build_snapshot(&ns, &rows, Some(true));
    let space_type = |sid: u64| match sid {
        11 | 12 => Some(SlsSpaceType::User),
        _ => Some(SlsSpaceType::Fullscreen),
    };
    let (ctx, skipped) = NativeContext::derive(&snapshot, &space_type);
    assert!(
        skipped.is_empty(),
        "user Spaces retain the display: {skipped:?}"
    );
    assert_eq!(ctx.domains.len(), 2, "both user Spaces stay domains");
    assert!(ctx.domains.iter().all(|d| !d.current));
    // A current-only SID is not proof of a unique fullscreen surface.
    assert_eq!(ctx.detached_current(1), None);
    let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    let memory = crate::runtime_core::LifetimeMemory::empty();
    let closed: Vec<WindowId> = Vec::new();
    let unreadable: BTreeMap<DomainKey, tiler_core::world_report::HoldReason> = BTreeMap::new();
    let owned = "req-5".to_owned();
    let mut input = ReportInput::new(
        INCARNATION,
        GENERATION,
        &ctx,
        5,
        5,
        &owned,
        &members,
        &memory,
        &closed,
        &unreadable,
        &[],
        "",
        false,
    );
    let report = build_report(&mut input);
    assert!(validate_report_v4_reason(&report).is_none());
    assert_eq!(report.native.len(), 1);
    assert_eq!(
        report.native[0].native_order,
        vec!["space-11".to_owned(), "space-12".to_owned()],
        "native order independent of current"
    );
    assert_eq!(report.native[0].current_native, None);
    assert_eq!(report.screens.len(), 1);
    let full = report.screens[0].full_rect.expect("FULL rides");
    assert_eq!((full.x, full.y, full.w, full.h), (0, 0, 1440, 900));
    let work = report.screens[0].work_area.expect("work area rides");
    assert_eq!(
        (work.w, work.h),
        (1440, 875),
        "derived work geometry survives"
    );

    let mut rows = rows;
    rows[0].spaces.push(99);
    let (ctx, _) = NativeContext::derive(&build_snapshot(&ns, &rows, Some(true)), &space_type);
    assert_eq!(ctx.detached_current(1), Some(99));
    let output = OutputId(crate::topology::output_key(1));
    let surface = crate::runtime_types::detached_ref(output.clone(), 99);
    let mut input = ReportInput::new(
        INCARNATION, GENERATION, &ctx, 6, 6, "req-6", &members, &memory,
        &closed, &unreadable, &[], "", false,
    );
    let mut fullscreen = observed(7, -500, -500, 100, 100);
    fullscreen.fullscreen = true;
    let unknown = [(1001, fullscreen)];
    let detached = BTreeMap::from([((1001, 7), 99)]);
    input.unknown = &unknown;

    let off = build_report(&mut input);
    assert!(!off.native[0].current_known);
    assert_eq!(off.native[0].detached_current, None);
    assert!(off.windows.is_empty(), "offscreen geometry proves no output");

    input.detached_members = Some(&detached);
    let on = build_report(&mut input);
    assert!(validate_report_v4_reason(&on).is_none());
    assert!(on.native[0].current_known);
    assert_eq!(on.native[0].current_native, None);
    assert_eq!(on.native[0].detached_current.as_ref(), Some(&surface));
    assert_eq!(on.native[0].native_order, ["space-11", "space-12"]);
    assert_eq!(on.windows[0].output.as_ref(), Some(&output));
    assert_eq!(on.windows[0].detached_surface.as_ref(), Some(&surface));
    assert_eq!(
        on.windows[0].native_workspace,
        tiler_core::world_report::NativeWorkspaceFact::Null
    );

    rows[0].spaces.push(99);
    let (ambiguous, _) =
        NativeContext::derive(&build_snapshot(&ns, &rows, Some(true)), &space_type);
    assert_eq!(ambiguous.detached_owner(99), None);
}

#[test]
fn parked_detached_membership_refreshes_only_exact_same_lifetime_and_sid() {
    let mut ctx = two_display_ctx();
    ctx.detached_spaces.insert(99, (1, false));
    let mut retained = retained_with_roster(1001, 1);
    let mut window = observed(7, 20, 20, 100, 100);
    window.fullscreen = true;
    let current = scoped_snapshot(
        vec![window], vec![vec![99]], 0, ctx.revision, true, vec![7],
    );
    apply_roster_covering(&mut retained, 1001, &current, &ctx);
    assert_eq!(
        retained.apps[&1001].roster[&7].detached_membership,
        Some((ctx.revision, 99))
    );
    assert!(retained.apps[&1001].roster[&7].spaces.is_empty());

    ctx.revision += 1;
    let mut parked = scoped_snapshot(Vec::new(), Vec::new(), 0, ctx.revision, true, vec![7]);
    parked.cg_alive.push(crate::runtime_types::CgAliveDetail {
        wid: 7,
        spaces: vec![99],
        layer: 0,
        onscreen: false,
        bounds: Some((20, 20, 100, 100)),
    });
    mark_roster_incomplete(&mut retained, 1001, "blank-hidden");
    let desktop_homes =
        crate::runtime_core::rehydrate_cg_alive(&mut retained, 1001, &parked, &ctx, true);
    assert!(desktop_homes.is_empty());
    assert_eq!(
        retained.apps[&1001].roster[&7].detached_membership,
        Some((ctx.revision, 99))
    );
    assert!(!retained.apps[&1001].roster_incomplete);

    ctx.detached_spaces.insert(100, (1, false));
    parked.cg_alive[0].spaces[0] = 100;
    mark_roster_incomplete(&mut retained, 1001, "blank-hidden");
    crate::runtime_core::rehydrate_cg_alive(&mut retained, 1001, &parked, &ctx, true);
    assert_eq!(retained.apps[&1001].roster[&7].detached_membership, None);
    assert!(retained.apps[&1001].roster_incomplete);
}

#[test]
fn away_topology_publication_can_precede_the_parked_anchor_reread() {
    use crate::runtime_core::rehydrate_cg_alive;
    use crate::runtime_types::CgAliveDetail;
    use crate::world_runtime::{DispatchContext, build_dispatch_report};
    use tiler_core::world_engine::WorldEngine;
    use tiler_core::world_report::WorldCommand;

    let mut session = session_native();
    let mut cap = crate::world_connection::macos_capabilities();
    cap.detached_fullscreen_view = true;
    let engine = WorldEngine::new_test_bound_with_capabilities(GENERATION, session.incarnation(), cap)
        .expect("capability-enabled test handshake");
    session.sequencer().bind_generation(engine.core_generation());
    *session.engine_mut() = engine;
    let mut ctx = two_display_ctx();
    for (domain, sid) in ctx.domains.iter_mut().zip([317, 319, 156]) {
        domain.space = sid;
        domain.workspace = WorkspaceId(format!("space-{sid}"));
        domain.current = sid == 156;
    }
    ctx.detached_spaces.insert(320, (1, true));
    let mut retained = retained_with_roster(12_168, 1);
    let mut w0 = observed(10_747, 0, 0, 2240, 1260);
    w0.fullscreen = true;
    let current = scoped_snapshot(
        vec![w0],
        vec![vec![320]],
        0,
        ctx.revision,
        true,
        vec![w0.wid],
    );
    assert!(apply_roster_covering(&mut retained, 12_168, &current, &ctx).is_empty());
    let w0_id = crate::runtime_types::window_id(12_168, w0.wid);
    let home = ctx.domains[0].workspace.clone();
    let output = ctx.domains[0].output.clone();
    let fullscreen = crate::runtime_types::detached_ref(output.clone(), 320);
    let report = |session: &mut WorldSession, retained: &RetainedCore, ctx: &NativeContext, seq| {
        let command = (seq > 1).then(|| WorldCommand::SelectWorkspace {
            output: output.clone(),
            workspace: home.clone(),
        });
        build_dispatch_report(
            session,
            retained,
            &DispatchContext::Native(Arc::new(ctx.clone())),
            &[],
            seq,
            seq,
            command.as_ref().map(|_| seq),
            command,
            None,
            "req-native-fullscreen",
        )
        .expect("native context reports")
    };
    let initial = report(&mut session, &retained, &ctx, 1);
    assert!(initial.windows.iter().any(|w| {
        w.window.0 == w0_id && w.detached_surface.as_ref() == Some(&fullscreen)
    }));

    // The main thread publishes R+1 for the current change without waiting
    // for the app owner's topology reread. A Return command can run here.
    ctx.revision += 1;
    ctx.domains[1].current = true;
    ctx.detached_spaces.insert(320, (1, false));
    let before_reread = report(&mut session, &retained, &ctx, 2);
    assert!(before_reread.windows.iter().any(|w| w.window.0 == w0_id));
    assert!(before_reread.windows.iter().all(|w| w.detached_surface.is_none()));

    // Away exposes only w1 to AX; fresh nondegenerate offscreen CG/SLS
    // evidence carries the already witnessed fullscreen lifetime to R+1.
    let w1 = observed(10_808, 10, 25, 800, 632);
    let mut parked = scoped_snapshot(
        vec![w1],
        vec![vec![319]],
        0,
        ctx.revision,
        true,
        vec![w0.wid, w1.wid],
    );
    parked.cg_alive.push(CgAliveDetail {
        wid: w0.wid,
        spaces: vec![320],
        layer: 0,
        onscreen: false,
        bounds: Some((0, 0, 2240, 1260)),
    });
    assert!(apply_roster_covering(&mut retained, 12_168, &parked, &ctx).is_empty());
    assert!(rehydrate_cg_alive(&mut retained, 12_168, &parked, &ctx, true).is_empty());
    let after_reread = report(&mut session, &retained, &ctx, 3);
    assert!(after_reread.windows.iter().any(|w| {
        w.window.0 == w0_id && w.detached_surface.as_ref() == Some(&fullscreen)
    }));
}

/// Selected classification exclusions never hold: a new accessory /
/// unknown-role excluded sibling (never live, never reportable) stalls
/// nothing, while a previously live unknown lifetime turned HiddenApp
/// stays held with no live fact across covering hidden reads and an
/// unrelated PID read, then closes flat once a covering read proves the
/// destroy. Production roster path only: apply, homing, minimized
/// refresh, audit, and the real report builder plus engine.
#[test]
fn selected_exclusions_never_hold_hidden_unknown_holds_without_live_fact() {
    use crate::classifier::ExcludeReason;
    use crate::runtime_core::{note_scoped_members, partition_scoped_members};
    use crate::runtime_types::ExcludedOmission;
    use crate::world_runtime::{DispatchContext, build_dispatch_report, refresh_minimized};

    let ctx = two_display_ctx();
    let mut session = session_native();
    let mut retained = retained_with_roster(1001, 1);
    retained.apps.insert(
        1002,
        RetainedApp {
            generation: 1,
            seq_last: 1,
            cmd_counter: 0,
            windows: Vec::new(),
            focused_wid: None,
            roster: std::collections::BTreeMap::new(),
            roster_incomplete: false,
            roster_cause: String::new(),
        },
    );
    // Snapshot 1: regular homed w10, unhomed live w1 with a groundable
    // frame (unknown but representable), and selected-exclusion
    // siblings w11/w12 that were never live or reportable.
    let w10 = observed(10, 10, 10, 100, 100);
    let w1 = observed(1, 100, 100, 200, 200);
    let mut first = scoped_snapshot(
        vec![w10, w1],
        vec![vec![11], vec![99]],
        2,
        7,
        true,
        vec![10, 1],
    );
    first.excluded_reasons.push(ExcludedOmission {
        wid: 11,
        reason: ExcludeReason::AccessoryPolicy,
    });
    first.excluded_reasons.push(ExcludedOmission {
        wid: 12,
        reason: ExcludeReason::UnknownRole,
    });
    assert!(apply_roster_covering(&mut retained, 1001, &first, &ctx).is_empty());
    let (members, _) = partition_scoped_members(&first.windows, &first.meta, &ctx);
    refresh_minimized(&mut retained, 1001, &first);
    note_scoped_members(&mut retained, 1001, &members);
    // Production projects the homed members onto the app row; the report
    // reads membership through that projection.
    if let Some(app) = retained.apps.get_mut(&1001) {
        app.windows = members.values().flatten().copied().collect();
    }
    audit_roster_hold(&mut retained, 1001, &ctx, false);
    // The grounded unknown is representable and the selected exclusions
    // hold nothing: no hold at all after a fully representable read.
    assert!(!retained.apps[&1001].roster_incomplete);
    assert!(retained.apps[&1001].roster_cause.is_empty());
    assert!(retained.apps[&1001].roster.contains_key(&11));
    assert!(retained.apps[&1001].roster.contains_key(&12));
    // Both the regular window and the grounded unknown report; the
    // exclusions mint no facts.
    let report = build_dispatch_report(
        &mut session,
        &retained,
        &DispatchContext::Native(Arc::new(ctx.clone())),
        &[],
        1,
        1,
        None,
        None,
        None,
        "req-1",
    )
    .expect("context reports");
    assert!(validate_report_v4_reason(&report).is_none());
    let regular = report
        .windows
        .iter()
        .find(|w| w.window.0 == "1001_10")
        .expect("regular window is not stalled by excluded siblings");
    assert_eq!(regular.native_workspace.as_deref(), Some("space-11"));
    let grounded = report
        .windows
        .iter()
        .find(|w| w.window.0 == "1001_1")
        .expect("groundable unknown reports while live");
    assert_eq!(grounded.native_workspace.as_deref(), None);
    assert_eq!(grounded.output.as_ref().unwrap().0, "macos-display-1");
    assert!(
        report.inventory_complete,
        "representable read holds nothing"
    );
    assert!(
        report.windows.iter().all(|w| w.window.0 != "1001_11"),
        "accessory exclusion mints no fact"
    );
    assert!(
        report.windows.iter().all(|w| w.window.0 != "1001_12"),
        "unknown-role exclusion mints no fact"
    );
    // Three covering hidden reads for the previously live unknown: each
    // keeps the hold with no live fact, surviving a clean unrelated read.
    for seq in 2..=4 {
        let mut hidden = scoped_snapshot(vec![w10], vec![vec![11]], 3, 7, true, vec![10, 1]);
        hidden.excluded_reasons.push(ExcludedOmission {
            wid: 1,
            reason: ExcludeReason::HiddenApp,
        });
        hidden.excluded_reasons.push(ExcludedOmission {
            wid: 11,
            reason: ExcludeReason::AccessoryPolicy,
        });
        hidden.excluded_reasons.push(ExcludedOmission {
            wid: 12,
            reason: ExcludeReason::UnknownRole,
        });
        assert!(apply_roster_covering(&mut retained, 1001, &hidden, &ctx).is_empty());
        refresh_minimized(&mut retained, 1001, &hidden);
        let (members, _) = partition_scoped_members(&hidden.windows, &hidden.meta, &ctx);
        note_scoped_members(&mut retained, 1001, &members);
        if let Some(app) = retained.apps.get_mut(&1001) {
            app.windows = members.values().flatten().copied().collect();
        }
        audit_roster_hold(&mut retained, 1001, &ctx, false);
        assert!(
            retained.apps[&1001].roster_incomplete,
            "hidden hold persists on covering read {seq}"
        );
        assert_eq!(
            retained.apps[&1001].roster_cause,
            ExcludeReason::HiddenApp.log_token(),
            "the hidden cause names itself, never a selected exclusion"
        );
        // Unrelated clean read clears only its own PID.
        let b = scoped_snapshot(
            vec![observed(20, 10, 10, 50, 50)],
            vec![vec![21]],
            0,
            7,
            true,
            vec![20],
        );
        assert!(apply_roster_covering(&mut retained, 1002, &b, &ctx).is_empty());
        let (b_members, _) = partition_scoped_members(&b.windows, &b.meta, &ctx);
        note_scoped_members(&mut retained, 1002, &b_members);
        if let Some(app) = retained.apps.get_mut(&1002) {
            app.windows = b_members.values().flatten().copied().collect();
        }
        audit_roster_hold(&mut retained, 1002, &ctx, false);
        assert!(!retained.apps[&1002].roster_incomplete, "B stays clean");
        assert!(
            retained.apps[&1001].roster_incomplete,
            "A hold survives the unrelated read"
        );
        let report = build_dispatch_report(
            &mut session,
            &retained,
            &DispatchContext::Native(Arc::new(ctx.clone())),
            &[],
            seq,
            seq,
            None,
            None,
            None,
            &format!("req-{seq}"),
        )
        .expect("context reports");
        assert!(validate_report_v4_reason(&report).is_none());
        assert!(
            !report.inventory_complete,
            "hidden unrepresentable fences inventory"
        );
        assert!(
            report.windows.iter().all(|w| w.window.0 != "1001_1"),
            "stale hidden frame never reports as live on read {seq}"
        );
    }
    // Covering destroy: w1 absent from live and exclusions prunes flat
    // while the never-reportable siblings persist mechanically.
    let mut destroy = scoped_snapshot(vec![w10], vec![vec![11]], 2, 7, true, vec![10]);
    destroy.excluded_reasons.push(ExcludedOmission {
        wid: 11,
        reason: ExcludeReason::AccessoryPolicy,
    });
    destroy.excluded_reasons.push(ExcludedOmission {
        wid: 12,
        reason: ExcludeReason::UnknownRole,
    });
    let pruned = apply_roster_covering(&mut retained, 1001, &destroy, &ctx);
    assert_eq!(pruned, vec![WindowId("1001_1".to_owned())]);
    assert!(!retained.apps[&1001].roster.contains_key(&1));
    assert!(retained.apps[&1001].roster.contains_key(&11));
    assert!(retained.apps[&1001].roster.contains_key(&12));
    for id in &pruned {
        session.pending_closes.insert(id.clone());
    }
    let report = build_dispatch_report(
        &mut session,
        &retained,
        &DispatchContext::Native(Arc::new(ctx.clone())),
        &[],
        9,
        9,
        None,
        None,
        None,
        "req-9",
    )
    .expect("context reports");
    assert!(validate_report_v4_reason(&report).is_none());
    assert!(
        report.closed.iter().any(|id| id.0 == "1001_1"),
        "hidden destroy closes flat"
    );
}

/// The CG liveness oracle rides the real snapshot path: a complete read
/// carries its owner inventory into the snapshot, and an unreadable read
/// carries explicit unreadable (unknown, never absence).
#[test]
fn cg_oracle_rides_build_scoped_snapshot_from_real_read() {
    use crate::classifier::ActivationPolicy;
    use crate::ffi::AxFrame;
    use crate::native_facts::{NativeAppFacts, NativeReadOutcome, NativeWindowFacts};
    use crate::native_tile_backend::build_scoped_snapshot;

    fn facts(wid: u32) -> NativeWindowFacts {
        NativeWindowFacts {
            wid,
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            fullscreen: Some(false),
            minimized: Some(false),
            modal: Some(false),
            size_settable: Some(true),
            min_size: None,
            max_size: None,
            frame: Some(AxFrame {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            }),
            cg_layer: Some(0),
            cg_onscreen: Some(true),
        }
    }
    let app = NativeAppFacts {
        pid: 1001,
        hidden: false,
        activation_policy: ActivationPolicy::Regular,
        is_own_bundle: false,
        app_id: None,
    };
    // Complete read: the snapshot proves what it saw.
    let outcome = NativeReadOutcome {
        pid: 1001,
        requested: 1,
        observed: 1,
        failed: 0,
        first_cause: String::new(),
        windows: vec![facts(7)],
        cg_complete: true,
        cg_owner_wids: vec![7],
        cg_entries: Vec::new(),
    };
    let snap = build_scoped_snapshot(
        1001,
        7,
        &outcome,
        &app,
        &|_| Ok(vec![11u64]),
        None,
        false,
        false,
    )
    .expect("complete read maps");
    assert!(snap.cg_complete);
    assert_eq!(snap.cg_owner_wids, vec![7]);
    // Unreadable read: explicit unknown rides along (never absence).
    let outcome = NativeReadOutcome {
        pid: 1001,
        requested: 1,
        observed: 1,
        failed: 0,
        first_cause: String::new(),
        windows: vec![facts(7)],
        cg_complete: false,
        cg_owner_wids: Vec::new(),
        cg_entries: Vec::new(),
    };
    let snap = build_scoped_snapshot(
        1001,
        7,
        &outcome,
        &app,
        &|_| Ok(vec![11u64]),
        None,
        false,
        false,
    )
    .expect("complete read maps");
    assert!(!snap.cg_complete);
    assert!(snap.cg_owner_wids.is_empty());
}

fn current_key() -> DomainKey {
    DomainKey {
        output: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId(crate::topology::workspace_key(11)),
    }
}

fn homed_current_retained(pid: u32, wid: u32) -> RetainedCore {
    let mut retained = retained_with_roster(pid, 1);
    if let Some(app) = retained.apps.get_mut(&pid) {
        app.roster.insert(
            wid,
            RosterEntry {
                observed: Some(observed(wid, 10, 10, 100, 100)),
                spaces: vec![11],
                fullscreen_meta: false,
                detached_membership: None,
                minimized_meta: false,
                hidden_meta: false,
                excluded: None,

                role: None,

                subrole: None,
                modal: None,
                resizable: None,
                layer: None,
                app_id: None,
                activation_policy: None,
            },
        );
    }
    retained
        .window_domains
        .insert(format!("{pid}_{wid}"), current_key());
    retained
}

/// Blocker regression (mutation target): known A homed current, then an
/// AX-empty snapshot with CG still listing A. The old current-prune fenced
/// nothing and staged an explicit close for a live window; the oracle
/// retains A with a hold and stages nothing.
#[test]
fn empty_ax_with_cg_alive_retains_current_homed_without_close() {
    let ctx = two_display_ctx();
    let mut retained = homed_current_retained(1001, 1);
    // Same revision, AX blind (hidden-Space blindness), CG alive.
    let snap = scoped_snapshot(Vec::new(), Vec::new(), 0, 7, true, vec![1]);
    assert!(!snapshot_is_covering(
        snap.windows.is_empty(),
        snap.excluded > 0
    ));
    let pruned = prune_current_homed_with_oracle(&mut retained, 1001, &snap, &ctx);
    assert!(
        pruned.is_empty(),
        "CG-alive current id never prunes (old logic closed it): {pruned:?}"
    );
    assert!(
        retained.apps[&1001].roster.contains_key(&1),
        "live identity retained"
    );
}

/// Dual: the same empty AX read with a fresh complete CG inventory
/// proving absence closes the current-homed id (real close converges).
#[test]
fn empty_ax_with_cg_absent_closes_current_homed() {
    let ctx = two_display_ctx();
    let mut retained = homed_current_retained(1001, 1);
    let snap = scoped_snapshot(Vec::new(), Vec::new(), 0, 7, true, Vec::new());
    let pruned = prune_current_homed_with_oracle(&mut retained, 1001, &snap, &ctx);
    assert_eq!(pruned, vec![WindowId("1001_1".to_owned())]);
    assert!(!retained.apps[&1001].roster.contains_key(&1));
}

/// Unreadable CG prunes nothing: repeated unknown holds persist, and an
/// unrelated PID's clean covering read clears only its own PID.
#[test]
fn cg_unreadable_holds_across_repeats_and_unrelated_pid() {
    let ctx = two_display_ctx();
    let mut retained = homed_current_retained(1001, 1);
    retained.apps.insert(
        1002,
        RetainedApp {
            generation: 1,
            seq_last: 1,
            cmd_counter: 0,
            windows: Vec::new(),
            focused_wid: None,
            roster: std::collections::BTreeMap::new(),
            roster_incomplete: false,
            roster_cause: String::new(),
        },
    );
    for _ in 0..3 {
        let snap = scoped_snapshot(Vec::new(), Vec::new(), 0, 7, false, Vec::new());
        let pruned = prune_current_homed_with_oracle(&mut retained, 1001, &snap, &ctx);
        assert!(pruned.is_empty(), "unreadable never guesses absence");
        mark_roster_incomplete(&mut retained, 1001, "blank-hidden");
        assert!(
            retained.apps[&1001].roster.contains_key(&1),
            "identity retained while unreadable"
        );
        let b = scoped_snapshot(
            vec![observed(20, 10, 10, 50, 50)],
            vec![vec![21]],
            0,
            7,
            true,
            vec![20],
        );
        assert!(apply_roster_covering(&mut retained, 1002, &b, &ctx).is_empty());
        assert!(!retained.apps[&1002].roster_incomplete, "B stays clean");
        assert!(
            retained.apps[&1001].roster_incomplete,
            "A hold survives the unrelated read"
        );
    }
}

/// Covering partial snapshots respect the oracle too: some other window
/// live while absent A is CG-alive holds A (never a close); once the fresh
/// complete inventory drops A, the same shape closes it flat.
#[test]
fn covering_partial_with_cg_alive_retains_absent_then_absent_closes() {
    let ctx = two_display_ctx();
    let mut retained = retained_with_roster(1001, 1);
    let seed = scoped_snapshot(
        vec![observed(1, 0, 0, 10, 10), observed(2, 0, 0, 10, 10)],
        vec![vec![11], vec![11]],
        0,
        7,
        true,
        vec![1, 2],
    );
    assert!(apply_roster_covering(&mut retained, 1001, &seed, &ctx).is_empty());
    // B live, A missing from AX but CG-alive: retain with a hold.
    let partial_alive = scoped_snapshot(
        vec![observed(2, 0, 0, 10, 10)],
        vec![vec![11]],
        0,
        7,
        true,
        vec![1, 2],
    );
    assert!(apply_roster_covering(&mut retained, 1001, &partial_alive, &ctx).is_empty());
    assert!(
        retained.apps[&1001].roster.contains_key(&1),
        "CG-alive absent id retained on covering partial"
    );
    assert!(
        retained.apps[&1001].roster_incomplete,
        "oracle hold fences inventory"
    );
    assert_eq!(retained.apps[&1001].roster_cause, "cg-alive");
    // Same AX shape, fresh complete inventory without A: proven gone.
    let partial_gone = scoped_snapshot(
        vec![observed(2, 0, 0, 10, 10)],
        vec![vec![11]],
        0,
        7,
        true,
        vec![2],
    );
    let pruned = apply_roster_covering(&mut retained, 1001, &partial_gone, &ctx);
    assert_eq!(pruned, vec![WindowId("1001_1".to_owned())]);
    assert!(!retained.apps[&1001].roster.contains_key(&1));
    assert!(!retained.apps[&1001].roster_incomplete);
}

/// A fresh covering read with membership adopts after an oracle hold:
// the retained id re-homes and the hold clears.
#[test]
fn cg_alive_hold_then_fresh_membership_adopt_clears() {
    use crate::runtime_core::{note_scoped_members, partition_scoped_members};
    use crate::world_runtime::refresh_minimized;

    let ctx = two_display_ctx();
    let mut retained = retained_with_roster(1001, 1);
    let seed = scoped_snapshot(
        vec![observed(1, 0, 0, 10, 10), observed(2, 0, 0, 10, 10)],
        vec![vec![11], vec![11]],
        0,
        7,
        true,
        vec![1, 2],
    );
    assert!(apply_roster_covering(&mut retained, 1001, &seed, &ctx).is_empty());
    let partial_alive = scoped_snapshot(
        vec![observed(2, 0, 0, 10, 10)],
        vec![vec![11]],
        0,
        7,
        true,
        vec![1, 2],
    );
    assert!(apply_roster_covering(&mut retained, 1001, &partial_alive, &ctx).is_empty());
    assert!(retained.apps[&1001].roster_incomplete);
    // Return: fresh native read with membership adopts the held id.
    let back = scoped_snapshot(
        vec![observed(1, 0, 0, 10, 10), observed(2, 0, 0, 10, 10)],
        vec![vec![11], vec![11]],
        0,
        7,
        true,
        vec![1, 2],
    );
    assert!(apply_roster_covering(&mut retained, 1001, &back, &ctx).is_empty());
    let (members, _) = partition_scoped_members(&back.windows, &back.meta, &ctx);
    refresh_minimized(&mut retained, 1001, &back);
    note_scoped_members(&mut retained, 1001, &members);
    if let Some(app) = retained.apps.get_mut(&1001) {
        app.windows = members.values().flatten().copied().collect();
    }
    audit_roster_hold(&mut retained, 1001, &ctx, false);
    assert!(!retained.apps[&1001].roster_incomplete, "adopt clears hold");
    assert!(retained.apps[&1001].roster.contains_key(&1));
}

/// Terminate flat closes ride while another PID holds on its CG oracle:
// the hold fences inventory but never swallows another PID's close.
#[test]
fn terminate_flat_close_rides_while_other_app_holds_on_oracle() {
    let mut retained = retained_with_roster(1002, 1);
    if let Some(app) = retained.apps.get_mut(&1002) {
        app.roster.insert(
            1,
            RosterEntry {
                observed: Some(observed(1, 100, 100, 200, 200)),
                spaces: Vec::new(),
                fullscreen_meta: false,
                detached_membership: None,
                minimized_meta: false,
                hidden_meta: false,
                excluded: None,

                role: None,

                subrole: None,
                modal: None,
                resizable: None,
                layer: None,
                app_id: None,
                activation_policy: None,
            },
        );
        app.roster_incomplete = true;
        app.roster_cause = "cg-alive".to_owned();
    }
    let mut session = session_native();
    session.pending_closes.insert(WindowId("1001_7".to_owned()));
    let ctx = two_display_ctx();
    let dispatch = crate::world_runtime::DispatchContext::Native(Arc::new(ctx));
    let report = crate::world_runtime::build_dispatch_report(
        &mut session,
        &retained,
        &dispatch,
        &[],
        3,
        3,
        None,
        None,
        None,
        "req-3",
    )
    .expect("context reports");
    assert!(validate_report_v4_reason(&report).is_none());
    assert!(
        report.closed.iter().any(|id| id.0 == "1001_7"),
        "flat terminate close rides despite the unrelated hold"
    );
    assert!(!report.inventory_complete, "oracle hold still fences");
}

// -- Grouped multi-display indicator (offline, real builders) --------------
//
// Every scenario runs the production report/engine builders: an actual
// native snapshot, `NativeContext::derive`, `build_report`, and a bound
// world engine with macOS capabilities, then projects the accepted
// reply's own pager. No native read, no live AX, no live SLS.

const GROUP_UUID_A: &str = "A1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6";
const GROUP_UUID_B: &str = "B1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6";

fn group_display(cg_id: u32, uuid: &str, x: f64, y: f64) -> crate::ffi::NsDisplay {
    let frame = crate::ffi::DisplayRect {
        x,
        y,
        w: 1440.0,
        h: 900.0,
    };
    crate::ffi::NsDisplay {
        cg_id,
        uuid: Some(uuid.to_owned()),
        frame,
        visible: frame,
        scale: 2.0,
    }
}

fn group_row(uuid: &str, spaces: &[u64], current: u64) -> crate::ffi::SlsDisplayRow {
    crate::ffi::SlsDisplayRow {
        identifier: Some(uuid.to_owned()),
        spaces: spaces.to_vec(),
        current: Some(current),
    }
}

fn grouped_reply(
    ns: &[crate::ffi::NsDisplay],
    rows: &[crate::ffi::SlsDisplayRow],
) -> tiler_core::world_report::WorldReplyV4 {
    let snapshot = crate::topology::build_snapshot(ns, rows, Some(true));
    let space_type = |_: u64| Some(crate::ffi::SlsSpaceType::User);
    let (ctx, skipped) = NativeContext::derive(&snapshot, &space_type);
    assert!(
        skipped.is_empty(),
        "user Spaces retain displays: {skipped:?}"
    );
    let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    let memory = crate::runtime_core::LifetimeMemory::empty();
    let closed: Vec<WindowId> = Vec::new();
    let unreadable: BTreeMap<DomainKey, tiler_core::world_report::HoldReason> = BTreeMap::new();
    let owned = "req-grouped".to_owned();
    let mut input = ReportInput::new(
        INCARNATION,
        GENERATION,
        &ctx,
        1,
        1,
        &owned,
        &members,
        &memory,
        &closed,
        &unreadable,
        &[],
        "",
        false,
    );
    let report = build_report(&mut input);
    assert!(validate_report_v4_reason(&report).is_none());
    let mut engine = tiler_core::world_engine::WorldEngine::new_test_bound_with_capabilities(
        GENERATION,
        INCARNATION,
        crate::world_connection::macos_capabilities(),
    )
    .expect("bound test engine");
    match tiler_protocol::world_admission::handle_report_checked(&mut engine, &report, &[]).outcome
    {
        WorldOutcome::Accepted(reply) | WorldOutcome::Duplicate(reply) => reply,
        other => panic!("grouped report must admit, got {other:?}"),
    }
}

fn grouped_user_rows(
    rows: &[crate::ffi::SlsDisplayRow],
) -> Vec<Option<crate::workspace_select::SelectionSnapshot>> {
    rows.iter()
        .map(|row| {
            Some(crate::workspace_select::SelectionSnapshot {
                detached: None,
                display: row.identifier.clone().expect("test row identity"),
                spaces: row.spaces.clone(),
                current: row.current.expect("test row current"),
            })
        })
        .collect()
}

fn grouped_projection(
    reply: &tiler_core::world_report::WorldReplyV4,
    ns: &[crate::ffi::NsDisplay],
    rows: &[crate::ffi::SlsDisplayRow],
    user_rows: &[Option<crate::workspace_select::SelectionSnapshot>],
) -> crate::workspace_indicator::IndicatorProjection {
    crate::workspace_indicator::project_grouped_core_indicator(
        &reply.presentation.pager,
        &reply.workspaces,
        ns,
        rows,
        user_rows,
        crate::workspace_indicator::IndicatorMode::All,
        &|_| crate::workspace_indicator::SpaceOccupancy::Occupied,
        &|_| None,
    )
    .expect("coherent grouped projection")
}

fn grouped_two_output_rows() -> Vec<crate::ffi::SlsDisplayRow> {
    vec![
        group_row(GROUP_UUID_B, &[21, 22], 21),
        group_row(GROUP_UUID_A, &[11, 12], 11),
    ]
}

fn grouped_two_output_screens() -> Vec<crate::ffi::NsDisplay> {
    // Deliberately out of read order: B is read first, A sits to its left.
    vec![
        group_display(2, GROUP_UUID_B, 1440.0, 0.0),
        group_display(1, GROUP_UUID_A, 0.0, 0.0),
    ]
}

#[test]
fn grouped_render_marks_each_outputs_own_current_in_arrangement_order() {
    let ns = grouped_two_output_screens();
    let rows = grouped_two_output_rows();
    let reply = grouped_reply(&ns, &rows);
    assert_eq!(
        reply.presentation.pager.len(),
        2,
        "core pager carries both outputs"
    );
    let projection = grouped_projection(&reply, &ns, &rows, &grouped_user_rows(&rows));
    assert!(projection.visible);
    assert!(
        projection.title.is_empty(),
        "several current Spaces have no single ordinal title"
    );
    assert!(
        projection.tiling.is_none(),
        "no ambiguous multi-display toggle"
    );
    assert_eq!(
        projection
            .rows
            .iter()
            .map(|row| row.core.as_ref().expect("core row").output.as_str())
            .collect::<Vec<_>>(),
        vec![
            "macos-display-1",
            "macos-display-1",
            "macos-display-2",
            "macos-display-2"
        ],
        "groups follow the arrangement, never the read order"
    );
    assert_eq!(
        projection
            .rows
            .iter()
            .map(|row| (row.position, row.ordinal))
            .collect::<Vec<_>>(),
        vec![(1, 1), (2, 2), (3, 1), (4, 2)],
        "render slots stay unique; displayed ordinals restart per group"
    );
    assert_eq!(
        projection
            .rows
            .iter()
            .filter(|row| row.current)
            .map(|row| row.position)
            .collect::<Vec<_>>(),
        vec![1, 3],
        "both outputs' current Spaces are highlighted at once"
    );
}

#[test]
fn grouped_click_names_only_its_own_output_and_workspace() {
    use crate::runtime_types::InputSemanticAction;
    let ns = grouped_two_output_screens();
    let rows = grouped_two_output_rows();
    let reply = grouped_reply(&ns, &rows);
    let projection = grouped_projection(&reply, &ns, &rows, &grouped_user_rows(&rows));
    let row = projection
        .rows
        .iter()
        .find(|row| row.core.as_ref().expect("core row").output == "macos-display-2" && row.current)
        .expect("B's current row");
    let workspace = row.core.as_ref().expect("core row").workspace.clone();
    let click = crate::workspace_indicator::IndicatorClick {
        kind: crate::workspace_indicator::IndicatorClickKind::Numbered,
        position: row.position,
        expected_sid: row.sid,
        ui_generation: 4,
        topology_revision: 9,
    };
    let action = crate::workspace_indicator::verify_core_row_click(
        click,
        row,
        4,
        9,
        &reply.core_generation,
        &reply.presentation,
        &reply.workspaces,
        &reply.core_generation,
    )
    .expect("coherent grouped click verifies");
    assert_eq!(
        action,
        InputSemanticAction::IndicatorSelectWorkspace {
            output: "macos-display-2".to_owned(),
            workspace,
        },
        "the click carries only B's exact output/workspace"
    );
    // A stale click keeps the same shape and refuses on generation drift.
    assert!(
        crate::workspace_indicator::verify_core_row_click(
            click,
            row,
            5,
            9,
            &reply.core_generation,
            &reply.presentation,
            &reply.workspaces,
            &reply.core_generation,
        )
        .is_err(),
        "a generation move between render and click refuses"
    );
}

#[test]
fn grouped_unknown_or_unprovable_scope_safe_hides_per_group() {
    let ns = grouped_two_output_screens();
    let rows = grouped_two_output_rows();
    let reply = grouped_reply(&ns, &rows);
    let mut user_rows = grouped_user_rows(&rows);
    // B's user-Space read is unprovable: only A renders, and the item
    // still shows A's own current rather than hiding both.
    user_rows[0] = None;
    let projection = grouped_projection(&reply, &ns, &rows, &user_rows);
    assert!(projection.visible, "A's group stays provable");
    assert!(
        projection
            .rows
            .iter()
            .all(|row| row.core.as_ref().expect("core row").output == "macos-display-1"),
        "the unproven display's group is hidden, not guessed"
    );
    assert_eq!(
        projection.title, "",
        "several native displays attached: no single ordinal names the item"
    );
    assert!(
        projection.tiling.is_none(),
        "multi attached keeps the toggle disabled even when one group renders"
    );
    // Everything unprovable: the whole item hides with an explicit reason.
    let all_unknown = vec![None, None];
    let hidden = grouped_projection(&reply, &ns, &rows, &all_unknown);
    assert!(!hidden.visible);
    assert!(
        hidden
            .hidden_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("no provable core pager group")),
        "{:?}",
        hidden.hidden_reason
    );
    // Unknown occupancy is styled per row and never flips the core fact.
    let mixed = crate::workspace_indicator::project_grouped_core_indicator(
        &reply.presentation.pager,
        &reply.workspaces,
        &ns,
        &rows,
        &grouped_user_rows(&rows),
        crate::workspace_indicator::IndicatorMode::All,
        &|sid| {
            if sid == 21 {
                crate::workspace_indicator::SpaceOccupancy::Unknown
            } else {
                crate::workspace_indicator::SpaceOccupancy::Occupied
            }
        },
        &|_| None,
    )
    .expect("mixed occupancy projects");
    assert_eq!(
        mixed
            .rows
            .iter()
            .find(|row| row.sid == 21)
            .expect("B current row")
            .occupancy,
        crate::workspace_indicator::SpaceOccupancy::Unknown
    );
}

#[test]
fn grouped_hotplug_regroups_and_ambiguous_joins_safe_hide() {
    let ns = grouped_two_output_screens();
    let rows = grouped_two_output_rows();
    let reply = grouped_reply(&ns, &rows);
    // Hot-unplug B: the same display set loses B's row, so only A's group
    // remains and the render regroups without a stale B row.
    let unplugged_rows = vec![rows[1].clone()];
    let unplugged = grouped_projection(
        &reply,
        &ns,
        &unplugged_rows,
        &grouped_user_rows(&unplugged_rows),
    );
    assert!(unplugged.visible);
    assert!(
        unplugged
            .rows
            .iter()
            .all(|row| row.core.as_ref().expect("core row").output == "macos-display-1"),
        "no stale B row survives the unplug"
    );
    // A display whose UUID names no row is ambiguous scope: safe-hide it
    // while A stays provable.
    let mut unknown_identity = ns.clone();
    unknown_identity[0].uuid = Some("C1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6".to_owned());
    let partial = grouped_projection(&reply, &unknown_identity, &rows, &grouped_user_rows(&rows));
    assert!(partial.visible);
    assert!(
        partial
            .rows
            .iter()
            .all(|row| row.core.as_ref().expect("core row").output == "macos-display-1"),
        "B's unjoinable group hides alone"
    );
    // An unreadable arrangement refuses the whole grouped render (safe-hide
    // with a reason) instead of ordering by read position.
    let mut malformed = ns.clone();
    malformed[0].frame.x = f64::NAN;
    let hidden = grouped_projection(&reply, &malformed, &rows, &grouped_user_rows(&rows));
    assert!(!hidden.visible);
    assert!(
        hidden
            .hidden_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("arrangement")),
        "{:?}",
        hidden.hidden_reason
    );
    // One display keeps the legacy single group exactly: slots equal
    // ordinals, the current ordinal is the title, and no group is invented
    // for the removed display.
    let single_ns = vec![group_display(1, GROUP_UUID_A, 0.0, 0.0)];
    let single_rows = vec![group_row(GROUP_UUID_A, &[11, 12], 11)];
    let single_reply = grouped_reply(&single_ns, &single_rows);
    let single_native = crate::workspace_select::SelectionSnapshot {
        detached: None,
        display: GROUP_UUID_A.to_owned(),
        spaces: single_rows[0].spaces.clone(),
        current: single_rows[0].current.expect("test row current"),
    };
    let single = crate::workspace_indicator::project_core_indicator(
        &single_reply.presentation.pager,
        &single_reply.workspaces,
        1,
        crate::workspace_indicator::IndicatorMode::All,
        &|_| crate::workspace_indicator::SpaceOccupancy::Occupied,
        &|_| None,
        &single_native,
    )
    .expect("single display projects");
    assert!(single.visible);
    assert_eq!(single.title, "1");
    assert_eq!(
        single
            .rows
            .iter()
            .map(|row| (row.position, row.ordinal))
            .collect::<Vec<_>>(),
        vec![(1, 1), (2, 2)],
        "single display keeps slot == ordinal"
    );
}

#[test]
fn grouped_stale_current_and_rehome_disable_only_their_group_until_fresh_reply() {
    use crate::workspace_indicator::SpaceOccupancy;
    let ns = grouped_two_output_screens();
    let rows = grouped_two_output_rows();
    let reply = grouped_reply(&ns, &rows);

    // B's applied reply still names Space 21 as current, but the fresh
    // native row now reports 22. The retained B group stays visible at
    // Unknown with every action disabled; A is untouched.
    let stale_current_rows = vec![
        group_row(GROUP_UUID_B, &[21, 22], 22),
        group_row(GROUP_UUID_A, &[11, 12], 11),
    ];
    let stale_current = grouped_projection(
        &reply,
        &ns,
        &stale_current_rows,
        &grouped_user_rows(&stale_current_rows),
    );
    assert!(stale_current.visible, "the mapped stale group is retained");
    let b_rows: Vec<_> = stale_current
        .rows
        .iter()
        .filter(|row| row.core.as_ref().expect("core row").output == "macos-display-2")
        .collect();
    assert_eq!(b_rows.len(), 2, "B keeps both rows at Unknown");
    for row in &b_rows {
        assert_eq!(row.occupancy, SpaceOccupancy::Unknown);
        assert!(!row.current, "no stale B highlight");
        assert!(!row.selectable, "no stale B action");
        assert_eq!(row.sid, 0, "no stale B anchor");
    }
    let a_rows: Vec<_> = stale_current
        .rows
        .iter()
        .filter(|row| row.core.as_ref().expect("core row").output == "macos-display-1")
        .collect();
    assert_eq!(a_rows.len(), 2, "A stays truthful");
    assert_eq!(
        a_rows.iter().filter(|row| row.current).count(),
        1,
        "A keeps its own fresh current highlight"
    );
    assert!(a_rows.iter().all(|row| row.selectable), "A stays armed");
    assert!(
        stale_current.tiling.is_none(),
        "a stale current group never owns the toggle"
    );

    // A fresh reply over the changed current restores B exactly.
    let recovered_reply = grouped_reply(&ns, &stale_current_rows);
    let recovered = grouped_projection(
        &recovered_reply,
        &ns,
        &stale_current_rows,
        &grouped_user_rows(&stale_current_rows),
    );
    let recovered_b: Vec<_> = recovered
        .rows
        .iter()
        .filter(|row| row.core.as_ref().expect("core row").output == "macos-display-2")
        .collect();
    assert_eq!(
        recovered_b.iter().filter(|row| row.current).count(),
        1,
        "fresh coherent reply restores B's current"
    );
    assert!(
        recovered_b
            .iter()
            .all(|row| row.selectable && row.occupancy != SpaceOccupancy::Unknown),
        "fresh coherent reply restores B's actions"
    );

    // With B's current coherent, move its non-current bound SID 22 to A.
    // Only that stale B row disarms; B's current row and all of A remain.
    let rehome_rows = vec![
        group_row(GROUP_UUID_B, &[21], 21),
        group_row(GROUP_UUID_A, &[11, 12, 22], 11),
    ];
    let rehomed = grouped_projection(&reply, &ns, &rehome_rows, &grouped_user_rows(&rehome_rows));
    // Space 22 now appears only on A's live row. B's retained binding names
    // no native Space here, so the projector must drop it instead of
    // painting a phantom B number until the fresh reply catches up.
    assert!(
        rehomed.rows.iter().all(|row| {
            row.core
                .as_ref()
                .and_then(|core| core.native_binding.as_deref())
                != Some("space-22")
        }),
        "the positively rehomed Space never survives as a B number"
    );
    let rehomed_b: Vec<_> = rehomed
        .rows
        .iter()
        .filter(|row| row.core.as_ref().expect("core row").output == "macos-display-2")
        .collect();
    assert_eq!(
        rehomed_b.len(),
        1,
        "only B's live current row survives the rehome"
    );
    assert!(rehomed_b[0].current, "B keeps its live current row");
    assert!(rehomed_b[0].selectable, "B's live current row stays armed");
    assert_eq!(
        rehomed
            .rows
            .iter()
            .filter(|row| row.core.as_ref().expect("core row").output == "macos-display-1")
            .filter(|row| row.current)
            .count(),
        1,
        "A stays truthful after B's rehome"
    );

    // A fresh reply over the rehome attributes Space 22 to A and recovers
    // B (which no longer has that workspace).
    let rehome_reply = grouped_reply(&ns, &rehome_rows);
    let rehome_recovered = grouped_projection(
        &rehome_reply,
        &ns,
        &rehome_rows,
        &grouped_user_rows(&rehome_rows),
    );
    assert!(
        rehome_recovered.rows.iter().all(|row| {
            row.core.as_ref().expect("core row").output != "macos-display-2"
                || row
                    .core
                    .as_ref()
                    .expect("core row")
                    .native_binding
                    .as_deref()
                    != Some("space-22")
        }),
        "B no longer claims the rehomed Space after a fresh reply"
    );
    let a_rehomed = rehome_recovered
        .rows
        .iter()
        .find(|row| {
            row.core.as_ref().expect("core row").output == "macos-display-1"
                && row
                    .core
                    .as_ref()
                    .expect("core row")
                    .native_binding
                    .as_deref()
                    == Some("space-22")
        })
        .expect("A owns the fresh binding");
    assert!(
        a_rehomed.selectable,
        "the fresh owner row is armed after recovery"
    );
}

#[test]
fn reconnect_drops_phantom_rows_and_keeps_only_live_targets() {
    use crate::workspace_indicator::SpaceOccupancy;
    // A display returns with a changed native base SID while the applied
    // core reply is still the pre-unplug one. Rows the fresh native rows
    // cannot prove (vanished or rehomed bindings, orphan unbound
    // non-spares) must never paint phantom numbers; the explicit spare
    // survives as a disabled core designation, and every surviving row
    // still targets its exact core output/workspace.
    let ns = grouped_two_output_screens();
    let old_rows = grouped_two_output_rows(); // B [21, 22], A [11, 12]
    let reply = grouped_reply(&ns, &old_rows);
    let a_output = "macos-display-1";
    let b_output = "macos-display-2";
    let native_binding = |output: &str, sid: &str| {
        reply
            .workspaces
            .iter()
            .find(|row| row.output.0 == output && row.native_binding.as_deref() == Some(sid))
            .expect("fixture binding")
            .id
            .clone()
    };
    let a_workspace = native_binding(a_output, "space-11");
    let b_workspace = native_binding(b_output, "space-21");

    // Stale reply plus the leftover shapes a reconnect leaves behind:
    // A retains an unbound non-spare from a displaced logical workspace,
    // B retains bindings to a vanished SID and to a rehomed SID, and B
    // keeps its explicit trailing spare.
    let mut stale = reply.clone();
    let mut injected: Vec<tiler_core::world_report::WorkspaceDesire> = Vec::new();
    for (output, template_workspace, template_desire, id, ordinal, binding, trailing) in [
        (a_output, 1usize, 0usize, "ws-orphan", 3u32, None, false),
        (
            b_output,
            1usize,
            0usize,
            "ws-vanished",
            3u32,
            Some("space-99"),
            false,
        ),
        (b_output, 1usize, 0usize, "ws-spare", 4u32, None, true),
    ] {
        let screen = stale
            .presentation
            .pager
            .iter_mut()
            .find(|screen| screen.output == output)
            .expect("fixture screen");
        let mut workspace = screen.workspaces[template_workspace].clone();
        workspace.id = id.to_owned();
        workspace.ordinal = ordinal;
        workspace.label = tiler_core::world_presentation::pager_workspace_label(ordinal);
        workspace.current = false;
        workspace.occupied = false;
        workspace.contains_focused = false;
        workspace.windows.clear();
        workspace.trailing = trailing;
        screen.workspaces.push(workspace);
        let mut desire = stale
            .workspaces
            .iter()
            .filter(|row| row.output.0 == output)
            .nth(template_desire)
            .expect("fixture desire")
            .clone();
        desire.id = id.to_owned();
        desire.native_binding = binding.map(str::to_owned);
        desire.trailing = trailing;
        desire.order = ordinal as usize - 1;
        injected.push(desire);
    }
    stale.workspaces.extend(injected);

    // Fresh native rows after reconnect: B's base SID moved (121 returns),
    // its old Space 22 is now homed on A, and A still owns 11/12.
    let reconnect_rows = vec![
        group_row(GROUP_UUID_B, &[121, 21], 21),
        group_row(GROUP_UUID_A, &[11, 12, 22], 11),
    ];
    let projection = grouped_projection(
        &stale,
        &ns,
        &reconnect_rows,
        &grouped_user_rows(&reconnect_rows),
    );
    assert!(projection.visible, "A and B still render their live rows");
    for phantom in ["space-99", "space-22", "ws-orphan", "ws-vanished"] {
        assert!(
            projection.rows.iter().all(|row| {
                let core = row.core.as_ref().expect("core row");
                core.workspace != phantom && core.native_binding.as_deref() != Some(phantom)
            }),
            "no phantom row survives: {phantom}"
        );
    }
    assert_eq!(
        projection
            .rows
            .iter()
            .map(|row| (
                row.core.as_ref().expect("core row").output.as_str(),
                row.ordinal,
                row.selectable,
            ))
            .collect::<Vec<_>>(),
        vec![
            (a_output, 1, true),
            (a_output, 2, true),
            (b_output, 1, true),
            (b_output, 4, false),
        ],
        "only live core rows and the disabled spare render, with their core ordinals"
    );
    let spare = &projection.rows[3];
    assert!(spare.core.as_ref().expect("core row").spare);
    assert_eq!(spare.occupancy, SpaceOccupancy::Unknown);

    // Correct targets: each surviving bound row still selects its exact
    // core output/workspace, never a native slot.
    let click_for = |row: &crate::workspace_indicator::IndicatorRow| {
        crate::workspace_indicator::IndicatorClick {
            kind: crate::workspace_indicator::IndicatorClickKind::Numbered,
            position: row.position,
            expected_sid: row.sid,
            ui_generation: 4,
            topology_revision: 9,
        }
    };
    let a_row = &projection.rows[0];
    assert_eq!(a_row.core.as_ref().expect("core row").output, a_output);
    assert_eq!(
        crate::workspace_indicator::verify_core_row_click(
            click_for(a_row),
            a_row,
            4,
            9,
            &stale.core_generation,
            &stale.presentation,
            &stale.workspaces,
            &stale.core_generation,
        )
        .expect("A's live row verifies"),
        crate::runtime_types::InputSemanticAction::IndicatorSelectWorkspace {
            output: a_output.to_owned(),
            workspace: a_workspace,
        }
    );
    let b_row = &projection.rows[2];
    assert_eq!(b_row.core.as_ref().expect("core row").output, b_output);
    assert_eq!(
        crate::workspace_indicator::verify_core_row_click(
            click_for(b_row),
            b_row,
            4,
            9,
            &stale.core_generation,
            &stale.presentation,
            &stale.workspaces,
            &stale.core_generation,
        )
        .expect("B's live row verifies"),
        crate::runtime_types::InputSemanticAction::IndicatorSelectWorkspace {
            output: b_output.to_owned(),
            workspace: b_workspace,
        }
    );
}

// ---------------------------------------------------------------------------
// Product AX snapshot auxiliary exposure (capture3 entry proxy). A positively
// read non-standard auxiliary (`AXWindow`/`AXUnknown`) is never a snapshot
// member: it must not join a roster/tiled set, must not mint CG-alive
// evidence as if AX had omitted it, and must not refuse the whole snapshot
// for its own unread frame/fullscreen/minimized/CG legs. Recognized
// dialog/sheet/floating shapes and unknown reads keep their existing
// behavior (published raw facts / whole-snapshot refusal).
// ---------------------------------------------------------------------------

fn auxiliary_entry_outcome() -> crate::native_facts::NativeReadOutcome {
    use crate::classifier::{ROLE_WINDOW, SUBROLE_UNKNOWN};
    use crate::ffi::{AxFrame, CgBounds};
    use crate::native_facts::{CgWindowEntry, NativeReadOutcome, NativeWindowFacts};

    const PID: u32 = 12_168;
    const W0: u32 = 10_747;
    const AUX: u32 = 10_815;
    const W1: u32 = 10_808;

    fn frame(x: f64, y: f64, w: f64, h: f64) -> AxFrame {
        AxFrame { x, y, w, h }
    }
    fn window_facts(
        wid: u32,
        role: &str,
        subrole: &str,
        frame: AxFrame,
        fullscreen: bool,
        layer: i32,
        onscreen: bool,
    ) -> NativeWindowFacts {
        NativeWindowFacts {
            wid,
            role: Some(role.to_owned()),
            subrole: Some(subrole.to_owned()),
            fullscreen: Some(fullscreen),
            minimized: Some(false),
            modal: Some(false),
            size_settable: Some(true),
            min_size: None,
            max_size: None,
            frame: Some(frame),
            cg_layer: Some(layer),
            cg_onscreen: Some(onscreen),
        }
    }
    fn cg_entry(wid: u32, layer: i32, onscreen: bool, frame: &AxFrame) -> CgWindowEntry {
        CgWindowEntry {
            wid,
            layer,
            onscreen,
            bounds: CgBounds {
                x: frame.x,
                y: frame.y,
                w: frame.w,
                h: frame.h,
            },
        }
    }
    // Capture3 entry: fullscreen Space 320 is current; AXWindows lists only
    // the same-owner full-size AXWindow/AXUnknown proxy (fullscreen false),
    // the true w0 is omitted while live/offscreen at CG layer 0, and the
    // Away sibling w1 stays a CG row.
    let full = frame(0.0, 0.0, 2240.0, 1260.0);
    let away = frame(110.0, 110.0, 800.0, 632.0);
    let home = frame(100.0, 100.0, 800.0, 632.0);
    // Eight hidden AppKit auxiliary CG rows and the 32pt surface are
    // roster-unknown CG-owned wids: they must never fence a hold.
    let hidden = frame(0.0, 0.0, 2240.0, 0.0);
    NativeReadOutcome {
        pid: PID,
        requested: 1,
        observed: 1,
        failed: 0,
        first_cause: String::new(),
        windows: vec![window_facts(
            AUX,
            ROLE_WINDOW,
            SUBROLE_UNKNOWN,
            full,
            false,
            0,
            true,
        )],
        cg_complete: true,
        cg_owner_wids: vec![10_900, W0, AUX, W1, 10_901],
        cg_entries: vec![
            cg_entry(10_900, 0, false, &hidden),
            cg_entry(W0, 0, false, &full),
            cg_entry(AUX, 0, true, &full),
            cg_entry(W1, 0, false, &away),
            cg_entry(10_901, 0, false, &home),
        ],
    }
}

#[test]
fn auxiliary_entry_proxy_is_never_a_member_and_keeps_known_windows() {
    use crate::classifier::ActivationPolicy;
    use crate::native_facts::NativeAppFacts;
    use crate::native_tile_backend::build_scoped_snapshot;
    use crate::runtime_core::{preserve_non_covering_hidden, rehydrate_cg_alive};

    const PID: u32 = 12_168;
    const W0: u32 = 10_747;
    const AUX: u32 = 10_815;
    const W1: u32 = 10_808;

    let mut ctx = two_display_ctx();
    for (domain, sid) in ctx.domains.iter_mut().zip([317, 319, 156]) {
        domain.space = sid;
        domain.workspace = WorkspaceId(crate::topology::workspace_key(sid));
        domain.current = sid == 156;
    }
    ctx.detached_spaces.insert(320, (1, true));

    // The tiled set retained from before entry: both members windowed on
    // their own Desktops.
    let mut retained = retained_with_roster(PID, 1);
    {
        let app = retained.apps.get_mut(&PID).expect("roster owner");
        for (wid, sid, frame) in [(W0, 317, (100, 100, 800, 632)), (W1, 319, (110, 110, 800, 632))] {
            app.roster.insert(
                wid,
                RosterEntry {
                    observed: Some(observed(wid, frame.0, frame.1, frame.2, frame.3)),
                    spaces: vec![sid],
                    detached_membership: None,
                    fullscreen_meta: false,
                    minimized_meta: false,
                    hidden_meta: false,
                    excluded: None,
                    role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                    subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                    modal: Some(false),
                    resizable: Some(true),
                    layer: Some(0),
                    app_id: None,
                    activation_policy: None,
                },
            );
        }
    }
    for (wid, sid) in [(W0, 317), (W1, 319)] {
        retained.window_domains.insert(
            crate::runtime_types::window_id(PID, wid),
            DomainKey {
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId(crate::topology::workspace_key(sid)),
            },
        );
    }

    let outcome = auxiliary_entry_outcome();
    let spaces_of = |wid: u32| match wid {
        W0 | AUX => Ok(vec![320u64]),
        W1 => Ok(vec![319u64]),
        _ => Ok(Vec::new()),
    };
    let app = NativeAppFacts {
        pid: PID,
        hidden: false,
        activation_policy: ActivationPolicy::Regular,
        is_own_bundle: false,
        app_id: Some("com.example.fixture".to_owned()),
    };
    // Production snapshot from the real capture3 entry shape: the only AX
    // row is the auxiliary, so it must neither refuse nor publish.
    let snap = build_scoped_snapshot(
        PID,
        ctx.revision,
        &outcome,
        &app,
        &spaces_of,
        None,
        false,
        false,
    )
    .expect("a positively read auxiliary never refuses the whole snapshot");
    assert!(snap.windows.is_empty(), "auxiliary never a live member");
    assert!(snap.meta.is_empty(), "auxiliary never a roster meta row");
    assert!(
        snap.excluded_reasons.is_empty(),
        "auxiliary is not an exclusion row either"
    );
    assert!(
        !snap.cg_alive.iter().any(|d| d.wid == AUX),
        "an AX-enumerated auxiliary never rides AX-absent CG-alive evidence"
    );
    let w0_detail = snap
        .cg_alive
        .iter()
        .find(|d| d.wid == W0)
        .expect("the AX-omitted true member keeps fresh CG evidence");
    assert_eq!(w0_detail.spaces, vec![320]);
    assert!(!w0_detail.onscreen, "offscreen fullscreen liveness still counts");
    assert!(w0_detail.bounds.is_some_and(|(_, _, w, h)| w > 0 && h > 0));
    assert!(!snapshot_is_covering(
        snap.windows.is_empty(),
        snap.excluded > 0
    ));

    // Production non-covering roster path: no explicit close, the known
    // tiled sibling keeps its provable home, and the auxiliary never enters
    // the roster.
    let pruned = prune_current_homed_with_oracle(&mut retained, PID, &snap, &ctx);
    assert!(pruned.is_empty(), "no inferred close: {pruned:?}");
    assert!(
        preserve_non_covering_hidden(&retained, PID, &snap, &ctx).is_empty(),
        "fresh CG evidence suppresses stale-home preservation"
    );
    mark_roster_incomplete(&mut retained, PID, "blank-hidden");
    let rehomed = rehydrate_cg_alive(&mut retained, PID, &snap, &ctx, true);
    assert!(
        rehomed
            .get(&DomainKey {
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId(crate::topology::workspace_key(319)),
            })
            .is_some_and(|windows| windows.iter().any(|w| w.wid == W1)),
        "the Away sibling keeps its provable home: {rehomed:?}"
    );
    assert!(!retained.apps[&PID].roster.contains_key(&AUX));
    assert!(retained.apps[&PID].roster.contains_key(&W0));
    assert!(retained.apps[&PID].roster.contains_key(&W1));
    assert_eq!(
        retained.apps[&PID].roster[&W0].detached_membership, None,
        "a windowed anchor at entry never seeds a detached surface"
    );
    assert!(
        retained.apps[&PID].roster[&W0].spaces.is_empty(),
        "the detached SID is not fabricated into a Desktop home"
    );
    audit_roster_hold(&mut retained, PID, &ctx, true);
    assert!(
        retained.apps[&PID].roster_incomplete,
        "the unproven fullscreen state stays fail-closed until the next read"
    );
    assert_eq!(retained.apps[&PID].roster_cause, "blank-hidden");
}

/// The combined condition the entry proxy alone cannot prove: a genuine
/// prior-fullscreen lifetime (w0 already carries an exact detached fact)
/// whose next read lists ONLY the auxiliary proxy in AX while fresh CG/SLS
/// still names w0. w0 must stay retained with its detached fact published
/// under the fresh revision, and the proxy must never become a lifetime.
#[test]
fn retained_fullscreen_survives_auxiliary_only_proxy_listing() {
    use crate::classifier::ActivationPolicy;
    use crate::native_facts::NativeAppFacts;
    use crate::native_tile_backend::build_scoped_snapshot;
    use crate::runtime_core::rehydrate_cg_alive;

    const PID: u32 = 12_168;
    const W0: u32 = 10_747;
    const AUX: u32 = 10_815;

    let mut ctx = two_display_ctx();
    for (domain, sid) in ctx.domains.iter_mut().zip([317, 319, 156]) {
        domain.space = sid;
        domain.workspace = WorkspaceId(crate::topology::workspace_key(sid));
        domain.current = sid == 156;
    }
    ctx.detached_spaces.insert(320, (1, true));

    // Genuine prior fullscreen: w0 is the AX member on the detached Space
    // with fresh CG liveness, so the product roster path seeds the exact
    // detached fact (never fabricated from a windowed anchor).
    let mut retained = retained_with_roster(PID, 1);
    let mut fullscreen = observed(W0, 0, 0, 2240, 1260);
    fullscreen.fullscreen = true;
    let seeding = scoped_snapshot(
        vec![fullscreen],
        vec![vec![320]],
        0,
        ctx.revision,
        true,
        vec![W0],
    );
    assert!(
        apply_roster_covering(&mut retained, PID, &seeding, &ctx).is_empty(),
        "seeding fullscreen observation closes nothing"
    );
    assert_eq!(
        retained.apps[&PID].roster[&W0].detached_membership,
        Some((ctx.revision, 320)),
        "the prior fullscreen lifetime is a genuine retained fact"
    );
    assert!(retained.apps[&PID].roster[&W0].spaces.is_empty());

    // Next read at the fresh revision: AX lists only the same-owner
    // AXWindow/AXUnknown proxy; fresh CG/SLS still names exact w0 (layer 0,
    // offscreen, full frame) on the detached Space.
    ctx.revision += 1;
    let mut outcome = auxiliary_entry_outcome();
    outcome.cg_owner_wids = vec![W0, AUX];
    outcome
        .cg_entries
        .retain(|entry| entry.wid == W0 || entry.wid == AUX);
    let app = NativeAppFacts {
        pid: PID,
        hidden: false,
        activation_policy: ActivationPolicy::Regular,
        is_own_bundle: false,
        app_id: Some("com.example.fixture".to_owned()),
    };
    let spaces_of = |wid: u32| match wid {
        W0 => Ok(vec![320u64]),
        _ => Ok(Vec::new()),
    };
    let entry = build_scoped_snapshot(
        PID,
        ctx.revision,
        &outcome,
        &app,
        &spaces_of,
        None,
        false,
        false,
    )
    .expect("an auxiliary-only AX listing never refuses the entry snapshot");
    assert!(entry.windows.is_empty(), "auxiliary never a live member");
    assert!(entry.meta.is_empty(), "auxiliary never a roster meta row");
    assert!(entry.excluded_reasons.is_empty());
    assert!(
        !entry.cg_alive.iter().any(|d| d.wid == AUX),
        "an AX-enumerated auxiliary never rides AX-absent CG-alive evidence"
    );
    let detail = entry
        .cg_alive
        .iter()
        .find(|d| d.wid == W0)
        .expect("exact w0 keeps its fresh CG/SLS evidence");
    assert_eq!(detail.spaces, vec![320]);
    assert_eq!((detail.layer, detail.onscreen), (0, false));
    assert_eq!(detail.bounds, Some((0, 0, 2240, 1260)));

    // Retention: the AX-absent but CG-alive anchor is never closed, and the
    // parked reread publishes its detached fact under the fresh revision
    // without fabricating a Desktop home or a proxy lifetime.
    assert!(
        apply_roster_covering(&mut retained, PID, &entry, &ctx).is_empty(),
        "fresh CG liveness closes nothing"
    );
    assert!(
        retained.apps[&PID].roster.contains_key(&W0),
        "w0 stays a retained lifetime through the proxy-only listing"
    );
    assert_eq!(
        retained.apps[&PID].roster[&W0].detached_membership,
        Some((ctx.revision - 1, 320)),
        "the prior detached fact holds until the parked reread"
    );
    assert!(retained.apps[&PID].roster_incomplete);
    assert_eq!(retained.apps[&PID].roster_cause, "cg-alive");

    let extra = rehydrate_cg_alive(&mut retained, PID, &entry, &ctx, true);
    assert!(extra.is_empty(), "no Desktop home fabricated: {extra:?}");
    assert_eq!(
        retained.apps[&PID].roster[&W0].detached_membership,
        Some((ctx.revision, 320)),
        "the detached fact is republished under the valid revision"
    );
    assert!(retained.apps[&PID].roster[&W0].spaces.is_empty());
    assert!(
        retained.apps[&PID].roster[&W0]
            .observed
            .is_some_and(|w| w.fullscreen),
        "the retained anchor keeps its fullscreen observation"
    );
    assert!(
        !retained.apps[&PID].roster.contains_key(&AUX),
        "the proxy never mints a roster lifetime"
    );
    assert_eq!(retained.apps[&PID].roster.len(), 1);
    assert!(
        !retained.apps[&PID].roster_incomplete,
        "the exact parked lifetime lifts the hold"
    );
}

#[test]
fn snapshot_fence_ignores_auxiliary_legs_and_refuses_unknown_members() {
    use crate::classifier::{
        ActivationPolicy, ROLE_SHEET, ROLE_WINDOW, SUBROLE_DIALOG, SUBROLE_STANDARD, SUBROLE_UNKNOWN,
    };
    use crate::ffi::AxFrame;
    use crate::native_facts::{NativeAppFacts, NativeReadOutcome, NativeWindowFacts};
    use crate::native_tile_backend::build_scoped_snapshot;

    const PID: u32 = 12_168;
    const WID: u32 = 10_815;

    fn facts(wid: u32, role: Option<&str>, subrole: Option<&str>) -> NativeWindowFacts {
        NativeWindowFacts {
            wid,
            role: role.map(str::to_owned),
            subrole: subrole.map(str::to_owned),
            fullscreen: Some(false),
            minimized: Some(false),
            modal: Some(false),
            size_settable: Some(true),
            min_size: None,
            max_size: None,
            frame: Some(AxFrame {
                x: 0.0,
                y: 0.0,
                w: 2240.0,
                h: 1260.0,
            }),
            cg_layer: Some(0),
            cg_onscreen: Some(true),
        }
    }
    fn outcome(windows: Vec<NativeWindowFacts>) -> NativeReadOutcome {
        NativeReadOutcome {
            pid: PID,
            requested: windows.len(),
            observed: windows.len(),
            failed: 0,
            first_cause: String::new(),
            windows,
            cg_complete: true,
            cg_owner_wids: vec![WID],
            cg_entries: Vec::new(),
        }
    }
    fn build(read: &NativeReadOutcome) -> Result<ScopedBackendSnapshot, String> {
        let app = NativeAppFacts {
            pid: PID,
            hidden: false,
            activation_policy: ActivationPolicy::Regular,
            is_own_bundle: false,
            app_id: None,
        };
        build_scoped_snapshot(PID, 7, read, &app, &|_| Ok(vec![320u64]), None, false, false)
    }
    fn drop_leg(facts: &mut NativeWindowFacts, leg: &str) {
        match leg {
            "frame" => facts.frame = None,
            "fullscreen" => facts.fullscreen = None,
            "minimized" => facts.minimized = None,
            "cg" => {
                facts.cg_layer = None;
                facts.cg_onscreen = None;
            }
            other => panic!("unknown leg {other}"),
        }
    }

    // The auxiliary's own legs are irrelevant: each missing leg still
    // yields a successful, memberless snapshot.
    for leg in ["frame", "fullscreen", "minimized", "cg"] {
        let mut aux = facts(WID, Some(ROLE_WINDOW), Some(SUBROLE_UNKNOWN));
        drop_leg(&mut aux, leg);
        let snap = build(&outcome(vec![aux]))
            .unwrap_or_else(|e| panic!("auxiliary {leg} leg refused the snapshot: {e}"));
        assert!(snap.windows.is_empty() && snap.meta.is_empty());
        assert!(snap.excluded_reasons.is_empty());
    }
    // Unknown reads stay fail-closed: an ordinary shape with any missing
    // member leg still refuses the whole snapshot.
    for leg in ["frame", "fullscreen", "minimized", "cg"] {
        let mut ordinary = facts(WID, Some(ROLE_WINDOW), Some(SUBROLE_STANDARD));
        drop_leg(&mut ordinary, leg);
        assert!(build(&outcome(vec![ordinary])).is_err(), "ordinary {leg}");
    }
    // A missing kind is unknown, not an auxiliary: it must refuse too.
    let mut unknown = facts(WID, None, None);
    drop_leg(&mut unknown, "fullscreen");
    assert!(build(&outcome(vec![unknown])).is_err());
    let mut unknown_subrole = facts(WID, Some(ROLE_WINDOW), None);
    drop_leg(&mut unknown_subrole, "cg");
    assert!(build(&outcome(vec![unknown_subrole])).is_err());

    // Recognized float semantics keep publishing raw facts: dialog and
    // sheet members are never pre-omitted.
    const DIALOG_WID: u32 = 10_816;
    const SHEET_WID: u32 = 10_817;
    let dialog = facts(DIALOG_WID, Some(ROLE_WINDOW), Some(SUBROLE_DIALOG));
    let sheet = facts(SHEET_WID, Some(ROLE_SHEET), None);
    let snap = build(&outcome(vec![dialog, sheet])).expect("recognized floats publish");
    assert_eq!(snap.windows.len(), 2);
    assert_eq!(
        snap.meta
            .iter()
            .find(|m| m.wid == DIALOG_WID)
            .and_then(|m| m.subrole.as_deref()),
        Some(SUBROLE_DIALOG)
    );
    assert_eq!(
        snap.meta
            .iter()
            .find(|m| m.wid == SHEET_WID)
            .and_then(|m| m.role.as_deref()),
        Some(ROLE_SHEET)
    );
}

// ---------------------------------------------------------------------------
// Select revision fence: exact stale detached anchor.
// ---------------------------------------------------------------------------

mod stale_anchor_fence {
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::detached_membership::MembershipProbeSlot;
    use crate::runtime_shared::AppRecord;
    use crate::runtime_thread::{FocusSlot, FrameSlot, Inbox};
    use crate::runtime_types::{BracketRead, CgAliveDetail};
    use crate::world_dispatch::{DispatchVerdict, dispatch_report};
    use crate::world_runtime::{DispatchContext, StaleDetachedInjection, build_dispatch_report};
    use tiler_core::world_engine::WorldEngine;
    use tiler_core::world_report::{ActivationPolicy, TransferKind, WorldCommand, WorldReplyV4};

    const PID: u32 = 12_168;
    const OWNER_GENERATION: u64 = 1;
    const W0: u32 = 10_747;
    const W1: u32 = 10_808;
    const HOME_SID: u64 = 317;
    const AWAY_SID: u64 = 319;
    const CURRENT_SID: u64 = 156;
    const DETACHED_SID: u64 = 320;

    fn w0_id() -> String {
        crate::runtime_types::window_id(PID, W0)
    }

    fn w0_window() -> WindowId {
        WindowId(w0_id())
    }

    /// Fake per-app AX owner mailbox: serves exactly the configured frozen
    /// bracket for every request, counting attempts. This is the same
    /// single-flight slot the production owner thread serves.
    struct OwnerServer {
        stop: Arc<AtomicBool>,
        attempts: Arc<AtomicUsize>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl OwnerServer {
        fn start(
            slot: Arc<MembershipProbeSlot>,
            generation: u64,
            read: Option<BracketRead>,
        ) -> Self {
            let stop = Arc::new(AtomicBool::new(false));
            let attempts = Arc::new(AtomicUsize::new(0));
            let thread_stop = Arc::clone(&stop);
            let thread_attempts = Arc::clone(&attempts);
            let handle = std::thread::spawn(move || {
                while !thread_stop.load(Ordering::SeqCst) {
                    match slot.take() {
                        Some(request) => {
                            thread_attempts.fetch_add(1, Ordering::SeqCst);
                            slot.complete(request, generation, read.clone());
                        }
                        None => std::thread::sleep(Duration::from_millis(1)),
                    }
                }
            });
            Self {
                stop,
                attempts,
                handle: Some(handle),
            }
        }

        fn attempts(&self) -> usize {
            self.attempts.load(Ordering::SeqCst)
        }
    }

    impl Drop for OwnerServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn exact_read(spaces: Vec<u64>) -> BracketRead {
        BracketRead {
            wid: W0,
            owner_pid: Some(PID),
            before_minimized: Some(false),
            before_hidden: Some(false),
            spaces,
            frame: Some((0, 0, 2240, 1260)),
            after_minimized: Some(false),
            after_hidden: Some(false),
        }
    }

    fn dispatch_shared(
        slot: Arc<MembershipProbeSlot>,
        clock: Arc<crate::world_sequencer::ObservationClock>,
    ) -> Arc<Mutex<crate::runtime::Shared>> {
        let config = crate::runtime_types::RuntimeConfig {
            topology_mode: crate::runtime_types::TopologyMode::Native,
            ..Default::default()
        };
        let mut apps = BTreeMap::new();
        apps.insert(
            PID,
            AppRecord {
                generation: OWNER_GENERATION,
                terminated: false,
                stop: Arc::new(AtomicBool::new(false)),
                frames: Arc::new(FrameSlot::new()),
                focus: Arc::new(FocusSlot::new()),
                membership: slot,
                ready: Arc::new(Mutex::new(Some(Ok(())))),
                worker: None,
                worker_name: "stale-anchor-test".to_owned(),
            },
        );
        Arc::new(Mutex::new(crate::runtime::Shared {
            config,
            handles_live: 0,
            generation_counter: 0,
            log_counter: 0,
            start: Instant::now(),
            apps,
            clock,
            cursor_ack_seq: 0,
            midflight_hook: Mutex::new(None),
            post_accept_hook: Mutex::new(None),
            topology: None,
            display_inputs: None,
            select_wake: None,
            main_wake: None,
            topology_revision: 9,
            native_context: None,
            native_cell: crate::runtime_shared::NativeContextCell::new(),
            select_pending: std::collections::VecDeque::new(),
            select_busy: false,
            select_claim_pending: false,
            selection_focus_pending: None,
            arrival_captures: std::collections::BTreeMap::new(),
            selection_focus_floor_seq: 0,
            close_focus_current: None,
            focus_evidence: None,
            zombies: Vec::new(),
            engine_handle: None,
            input_handle: None,
            logger_handle: None,
            input_thread: None,
            terminal: None,
            admission_closed: false,
            stop_engine: Arc::new(AtomicBool::new(false)),
            stop_input: Arc::new(AtomicBool::new(false)),
            stop_logger: Arc::new(AtomicBool::new(false)),
            log_tx: None,
            diagnostics: crate::runtime_types::RuntimeDiagnostics::default(),
            log: std::collections::VecDeque::new(),
            last_desired: Vec::new(),
            test_wake: None,
            test_seq: 0,
            last_geometry_key: None,
            last_presentation: crate::runtime::empty_world_presentation_v4(),
            last_pager_bindings: Vec::new(),
            last_workspace_desires: Vec::new(),
            last_native_creations: Vec::new(),
            last_core_generation: String::new(),
            indicator_revision: 0,
            presentation_style: crate::active_border::PresentationStylePx::default(),
            gesture_settlement: None,
            gesture_settlement_seq: 0,
        }))
    }

    /// Genuine-report fixture: a live member on its canonical home at
    /// revision R with command focus memory, then the same member observed as
    /// a current detached fullscreen surface, then the topology published at
    /// R+1 for the away Display *without* the owner reread: the retained row
    /// still carries `(R, 320)`.
    struct Fixture {
        session: WorldSession,
        retained: RetainedCore,
        shared: Arc<Mutex<crate::runtime::Shared>>,
        slot: Arc<MembershipProbeSlot>,
        ctx: NativeContext,
        output: OutputId,
        /// Adapter wire workspace id for the command/context.
        home: WorkspaceId,
        /// Core logical workspace id the engine resolves that wire id to.
        home_logical: WorkspaceId,
    }

    impl Fixture {
        fn build(detached_capability: bool) -> Self {
            let clock = Arc::new(crate::world_sequencer::ObservationClock::new());
            let mut session = WorldSession::new(
                true,
                crate::focus_route::FocusRoute::Slps,
                Arc::clone(&clock),
                &[],
            );
            let mut cap = crate::world_connection::macos_capabilities();
            cap.detached_fullscreen_view = detached_capability;
            let engine = WorldEngine::new_test_bound_with_capabilities(
                GENERATION,
                session.incarnation(),
                cap,
            )
            .expect("capability-enabled test handshake");
            session.sequencer().bind_generation(engine.core_generation());
            *session.engine_mut() = engine;

            let mut ctx = two_display_ctx();
            for (domain, sid) in ctx.domains.iter_mut().zip([HOME_SID, AWAY_SID, CURRENT_SID]) {
                domain.space = sid;
                domain.workspace = WorkspaceId(format!("space-{sid}"));
                domain.current = sid == HOME_SID || sid == CURRENT_SID;
            }
            let home = ctx.domains[0].workspace.clone();
            let home_logical = WorkspaceId("ws-1".to_owned());
            let output = ctx.domains[0].output.clone();

            let mut retained = retained_with_roster(PID, OWNER_GENERATION);
            let slot = Arc::new(MembershipProbeSlot::new());
            let shared = dispatch_shared(Arc::clone(&slot), clock);

            // R: w0 is a plain live member of its canonical home.
            let mut plain = observed(W0, 0, 0, 400, 300);
            plain.fullscreen = false;
            let mut current = scoped_snapshot(
                vec![plain.clone()],
                vec![vec![HOME_SID]],
                0,
                ctx.revision,
                true,
                vec![W0],
            );
            current.meta[0].role = Some("AXWindow".to_owned());
            current.meta[0].subrole = Some("AXStandardWindow".to_owned());
            current.meta[0].modal = Some(false);
            current.meta[0].layer = Some(0);
            current.meta[0].resizable = Some(true);
            current.meta[0].activation_policy = Some(ActivationPolicy::Regular);
            assert!(
                apply_roster_covering(&mut retained, PID, &current, &ctx).is_empty(),
                "covering home read applies"
            );
            crate::runtime_core::note_scoped_members(
                &mut retained,
                PID,
                &BTreeMap::from([(
                    DomainKey {
                        output: output.clone(),
                        workspace: home.clone(),
                    },
                    vec![plain.clone()],
                )]),
            );
            retained
                .apps
                .get_mut(&PID)
                .expect("roster pid")
                .windows = vec![plain];
            let mut fixture = Self {
                session,
                retained,
                shared,
                slot,
                ctx,
                output,
                home,
                home_logical,
            };
            let first = fixture.dispatch(None, 1);
            assert!(
                matches!(first, DispatchVerdict::Applied { .. }),
                "first observation applied: {first:?}"
            );
            // Native focus arrives while the window is still on Home.
            // No typed Focus command is needed to remember that window.
            fixture.retained.observed_focus = Some(w0_window());
            fixture.retained.observed_focus_known = true;
            assert!(matches!(
                fixture.dispatch(None, 2),
                DispatchVerdict::Applied { .. }
            ));
            let home_key = fixture.home_key();
            let engine = fixture.session.engine_mut();
            assert_eq!(
                engine.native_selection_binding(&home_key).as_deref(),
                Some(fixture.home.0.as_str()),
                "logical Home is joined to the native Desktop through its binding",
            );
            assert_eq!(
                engine.remembered_detached_selection_home(&w0_window()),
                Some(home_key),
                "the observed windowed focus seeds canonical Home memory",
            );

            // R: the same lifetime is now the current detached fullscreen
            // surface, still attributed to its retained home.
            fixture.ctx.detached_spaces.insert(DETACHED_SID, (1, true));
            fixture.ctx.domains[0].current = false;
            let mut full = observed(W0, 0, 0, 2240, 1260);
            full.fullscreen = true;
            let mut detached = scoped_snapshot(
                vec![full],
                vec![vec![DETACHED_SID]],
                0,
                fixture.ctx.revision,
                true,
                vec![W0],
            );
            // The retained off-Space read keeps the same standard-window
            // metadata on the fullscreen lifetime as the windowed read.
            detached.meta[0].role = Some("AXWindow".to_owned());
            detached.meta[0].subrole = Some("AXStandardWindow".to_owned());
            detached.meta[0].modal = Some(false);
            detached.meta[0].layer = Some(0);
            detached.meta[0].resizable = Some(true);
            detached.meta[0].activation_policy = Some(ActivationPolicy::Regular);
            assert!(
                apply_roster_covering(&mut fixture.retained, PID, &detached, &fixture.ctx)
                    .is_empty(),
                "detached covering read applies"
            );
            crate::runtime_core::note_scoped_members(
                &mut fixture.retained,
                PID,
                &BTreeMap::new(),
            );
            fixture
                .retained
                .apps
                .get_mut(&PID)
                .expect("roster pid")
                .windows
                .clear();
            assert_eq!(
                fixture.retained.apps[&PID].roster[&W0].detached_membership,
                Some((fixture.ctx.revision, DETACHED_SID))
            );
            assert!(!fixture.retained.window_domains.contains_key(&w0_id()));
            assert!(matches!(
                fixture.dispatch(None, 3),
                DispatchVerdict::Applied { .. }
            ));
            let home_key = DomainKey {
                output: fixture.output.clone(),
                workspace: fixture.home_logical.clone(),
            };
            assert_eq!(
                fixture
                    .session
                    .engine_mut()
                    .remembered_detached_selection_member(&home_key),
                Some(w0_window()),
                "canonical memory names the home member before the detach"
            );

            // R+1: the away Display is current and the detached surface is no
            // longer current; the owner reread has not arrived, so the
            // retained membership revision is stale.
            fixture.ctx.revision += 1;
            fixture.ctx.domains[1].current = true;
            fixture.ctx.domains[0].current = false;
            fixture.ctx.detached_spaces.insert(DETACHED_SID, (1, false));
            fixture
                .shared
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .native_context = Some(Arc::new(fixture.ctx.clone()));
            assert_eq!(
                fixture.retained.apps[&PID].roster[&W0].detached_membership,
                Some((fixture.ctx.revision - 1, DETACHED_SID)),
                "the withheld row is stale at the published revision"
            );
            fixture
        }

        fn dispatch(&mut self, command: Option<WorldCommand>, seq: u64) -> DispatchVerdict {
            let inbox = Arc::new(Inbox::new(16));
            let focus_queue = Arc::new(Mutex::new(Vec::new()));
            let command_is_select = command.is_some();
            let native = Arc::new(self.ctx.clone());
            {
                let mut shared = self
                    .shared
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                shared.native_context = Some(Arc::clone(&native));
                shared.topology_revision = self.ctx.revision;
            }
            self.session.sequencer().invalidate();
            dispatch_report(
                &mut self.session,
                &mut self.retained,
                &self.shared,
                &inbox,
                &focus_queue,
                16,
                DispatchContext::Native(native),
                Vec::new(),
                command,
                None,
                command_is_select,
                if command_is_select { "select" } else { "reconcile" },
                0,
                0,
                seq,
            )
        }

        fn select_home(&mut self) -> DispatchVerdict {
            self.dispatch(
                Some(WorldCommand::SelectWorkspace {
                    output: self.output.clone(),
                    workspace: self.home_logical.clone(),
                }),
                4,
            )
        }

        fn home_key(&self) -> DomainKey {
            DomainKey {
                output: self.output.clone(),
                workspace: self.home_logical.clone(),
            }
        }

        fn reply_transfer_to_home(&self) -> Option<tiler_core::world_report::TransferTargetV4> {
            self.session
                .sequencer()
                .last_reply()?
                .transfers
                .into_iter()
                .find(|row| {
                    row.target_output == self.output
                        && row.target_workspace == self.home_logical.0
                        && row.kind == TransferKind::View
                })
        }
    }

    fn applied_operation(verdict: &DispatchVerdict) -> String {
        match verdict {
            DispatchVerdict::Applied {
                select_operation: Some(operation),
            } => operation.clone(),
            other => panic!("expected an applied select, got {other:?}"),
        }
    }

    #[test]
    fn stale_detached_anchor_exact_proof_dispatches_detached_view_and_pins() {
        let mut fixture = Fixture::build(true);
        let home_key = fixture.home_key();
        let before_revision = fixture.session.engine_mut().state_revision();
        let server = OwnerServer::start(
            Arc::clone(&fixture.slot),
            OWNER_GENERATION,
            Some(exact_read(vec![DETACHED_SID])),
        );

        let verdict = fixture.select_home();
        let operation = applied_operation(&verdict);
        assert_eq!(server.attempts(), 1, "exactly the provisional target probes");

        let transfer = fixture
            .reply_transfer_to_home()
            .expect("the proved target keeps its View");
        let destination = transfer
            .detached_destination
            .as_ref()
            .expect("never a Home/Desktop view");
        assert_eq!(
            crate::runtime_types::detached_sid(&destination.surface),
            Some(DETACHED_SID)
        );
        assert_eq!(destination.surface.output, fixture.output);
        assert_eq!(destination.member, w0_window());

        let command_seq: u64 = operation
            .strip_prefix("op-")
            .expect("operation identity")
            .parse()
            .expect("command sequence");
        let pinned = fixture
            .session
            .pin_detached_select_member(command_seq, &w0_window())
            .expect("the report pins the proved member for the drain");
        assert_eq!(pinned.generation, OWNER_GENERATION);

        let engine = fixture.session.engine_mut();
        assert!(engine.state_revision() > before_revision, "core adopted once");
        assert_eq!(
            engine.logical_view(&fixture.output.0),
            Some(fixture.home_logical.0.clone()),
            "the view switched, the window did not land on a Desktop home"
        );
        assert_eq!(
            engine.remembered_detached_selection_member(&home_key),
            Some(w0_window())
        );
    }

    fn assert_refused_without_adoption(
        mut fixture: Fixture,
        server: OwnerServer,
        label: &str,
    ) {
        let before_revision = fixture.session.engine_mut().state_revision();
        let before_view = fixture
            .session
            .engine_mut()
            .logical_view(&fixture.output.0);
        let refused_before = fixture
            .shared
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .diagnostics
            .input_select_refused;

        let verdict = fixture.select_home();
        assert_eq!(server.attempts(), 1, "{label}: the exact target was read once");
        match verdict {
            DispatchVerdict::Refused { token, select_operation } => {
                assert_eq!(token, "refused-select-stale-detached-anchor", "{label}");
                assert!(select_operation.is_none(), "{label}: never binds a plan");
            }
            other => panic!("{label}: expected the distinct refusal, got {other:?}"),
        }
        assert_eq!(
            fixture.session.engine_mut().state_revision(),
            before_revision,
            "{label}: no core adoption"
        );
        assert_eq!(
            fixture.session.engine_mut().logical_view(&fixture.output.0),
            before_view,
            "{label}: no Home view switch"
        );
        assert!(
            fixture.reply_transfer_to_home().is_none(),
            "{label}: no native View to the stale home"
        );
        assert_eq!(
            fixture
                .shared
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .diagnostics
                .input_select_refused,
            refused_before + 1,
            "{label}: one distinct structured refusal"
        );
        // The flight was cleared: the next ordinary observation dispatches.
        let observation = fixture.dispatch(None, 5);
        assert!(
            matches!(observation, DispatchVerdict::Applied { .. }),
            "{label}: flight cleared for the next report, got {observation:?}"
        );
    }

    #[test]
    fn stale_detached_anchor_read_failure_refuses_without_home_or_adoption() {
        let fixture = Fixture::build(true);
        let server = OwnerServer::start(Arc::clone(&fixture.slot), OWNER_GENERATION, None);
        assert_refused_without_adoption(fixture, server, "unreadable owner bracket");
    }

    #[test]
    fn stale_detached_anchor_wrong_surface_refuses_without_home_or_adoption() {
        let fixture = Fixture::build(true);
        let server = OwnerServer::start(
            Arc::clone(&fixture.slot),
            OWNER_GENERATION,
            Some(exact_read(vec![321])),
        );
        assert_refused_without_adoption(fixture, server, "readable different surface");
    }

    #[test]
    fn stale_detached_anchor_fresh_reread_needs_no_preflight_probe() {
        use crate::runtime_core::{apply_roster_covering, rehydrate_cg_alive};

        let mut fixture = Fixture::build(true);
        let w1 = observed(W1, 10, 25, 800, 632);
        let mut parked = scoped_snapshot(
            vec![w1],
            vec![vec![AWAY_SID]],
            0,
            fixture.ctx.revision,
            true,
            vec![W0, W1],
        );
        parked.cg_alive.push(CgAliveDetail {
            wid: W0,
            spaces: vec![DETACHED_SID],
            layer: 0,
            onscreen: false,
            bounds: Some((0, 0, 2240, 1260)),
        });
        assert!(
            apply_roster_covering(&mut fixture.retained, PID, &parked, &fixture.ctx)
                .is_empty()
        );
        assert!(
            rehydrate_cg_alive(&mut fixture.retained, PID, &parked, &fixture.ctx, true)
                .is_empty()
        );
        assert_eq!(
            fixture.retained.apps[&PID].roster[&W0].detached_membership,
            Some((fixture.ctx.revision, DETACHED_SID)),
            "the owner reread refreshed the membership at the current revision"
        );
        assert!(!fixture.retained.apps[&PID].roster_incomplete);
        assert!(!fixture.retained.window_domains.contains_key(&w0_id()));
        fixture
            .shared
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .native_context = Some(Arc::new(fixture.ctx.clone()));

        let server = OwnerServer::start(
            Arc::clone(&fixture.slot),
            OWNER_GENERATION,
            Some(exact_read(vec![DETACHED_SID])),
        );
        let verdict = fixture.select_home();
        applied_operation(&verdict);
        assert_eq!(server.attempts(), 0, "fresh membership never preflights");
        let transfer = fixture
            .reply_transfer_to_home()
            .expect("fresh detached member keeps its View");
        let destination = transfer
            .detached_destination
            .as_ref()
            .expect("fresh detached destination");
        assert_eq!(
            crate::runtime_types::detached_sid(&destination.surface),
            Some(DETACHED_SID)
        );
    }

    #[test]
    fn stale_detached_anchor_capability_off_takes_ordinary_path() {
        let mut fixture = Fixture::build(false);
        let server = OwnerServer::start(
            Arc::clone(&fixture.slot),
            OWNER_GENERATION,
            Some(exact_read(vec![DETACHED_SID])),
        );
        let before_revision = fixture.session.engine_mut().state_revision();
        let verdict = fixture.select_home();
        applied_operation(&verdict);
        assert_eq!(server.attempts(), 0, "capability-off never probes");
        assert!(fixture.session.engine_mut().state_revision() > before_revision);
        if let Some(transfer) = fixture.reply_transfer_to_home() {
            assert!(
                transfer.detached_destination.is_none(),
                "no detached claim without the capability"
            );
        }
    }

    /// A false preview to the stale Home (proof exists but the checked
    /// candidate publishes a plain Native View) is rejected by the same gate
    /// before adoption; this drives the genuine typed candidate, not a
    /// synthetic reply.
    #[test]
    fn stale_detached_home_view_is_rejected_before_adoption() {
        let mut fixture = Fixture::build(true);
        let command_seq = 7u64;
        let homes = BTreeSet::from([fixture.home_key()]);
        let proof = StaleDetachedInjection {
            command_seq,
            report_revision: fixture.ctx.revision,
            window: w0_window(),
            pid: PID,
            wid: W0,
            generation: OWNER_GENERATION,
            stale_revision: fixture.ctx.revision - 1,
            surface: DETACHED_SID,
            home: fixture.home_key(),
            detached: crate::runtime_types::detached_ref(fixture.output.clone(), DETACHED_SID),
        };
        let command = WorldCommand::SelectWorkspace {
            output: fixture.output.clone(),
            workspace: fixture.home_logical.clone(),
        };
        let native = Arc::new(fixture.ctx.clone());
        let report = build_dispatch_report(
            &mut fixture.session,
            &fixture.retained,
            &DispatchContext::Native(native),
            &[],
            // Beyond the accepted detach watermark, so this candidate is
            // admitted and the guard (not observation freshness) refuses it.
            4,
            7,
            Some(command_seq),
            Some(command),
            None,
            "req-stale-anchor-guard",
        )
        .expect("native context reports");

        let before_revision = fixture.session.engine_mut().state_revision();
        let before_view = fixture
            .session
            .engine_mut()
            .logical_view(&fixture.output.0);
        let before_members = fixture
            .session
            .engine_mut()
            .logical_members(&fixture.output.0, &fixture.home_logical.0);
        let gate = tiler_protocol::world_admission::ProtocolAdmissionGate::new();
        let checked = gate.dispatch_with_prepare(
            fixture.session.engine_mut(),
            &report,
            &[],
            |candidate| {
                crate::world_dispatch::stale_detached_candidate_guard(
                    candidate.reply.reply(),
                    command_seq,
                    &homes,
                    Some(&proof),
                )
            },
        );
        assert!(checked.is_err(), "a Home/Desktop View is never accepted");
        assert_eq!(
            fixture.session.engine_mut().state_revision(),
            before_revision,
            "rejected before core adoption"
        );
        assert_eq!(
            fixture.session.engine_mut().logical_view(&fixture.output.0),
            before_view,
            "the refused candidate never switched the view"
        );
        assert_eq!(
            fixture
                .session
                .engine_mut()
                .logical_members(&fixture.output.0, &fixture.home_logical.0),
            before_members,
            "no Desktop adoption of the detached member"
        );
    }

    #[test]
    fn stale_detached_guard_requires_the_exact_correlated_view() {
        use tiler_core::world_report::{DetachedDestination, TransferTargetV4};

        let home = DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("ws-a".to_owned()),
        };
        let proof = StaleDetachedInjection {
            command_seq: 1,
            report_revision: 9,
            window: WindowId("1_7".to_owned()),
            pid: 1,
            wid: 7,
            generation: 1,
            stale_revision: 8,
            surface: 320,
            home: home.clone(),
            detached: crate::runtime_types::detached_ref(home.output.clone(), 320),
        };
        let homes = BTreeSet::from([home.clone()]);
        let view = |destination: Option<DetachedDestination>| TransferTargetV4 {
            operation_id: "op-1".to_owned(),
            kind: TransferKind::View,
            target_output: home.output.clone(),
            target_workspace: home.workspace.0.clone(),
            follow: false,
            held: false,
            command_seq: Some(1),
            parent: None,
            detached_destination: destination,
        };
        let exact = DetachedDestination {
            surface: crate::runtime_types::detached_ref(home.output.clone(), 320),
            member: proof.window.clone(),
        };
        let reply = |transfers: Vec<TransferTargetV4>| {
            let reply = WorldReplyV4 {
                contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION_V4,
                adapter_incarnation: "adapter-1".to_owned(),
                core_generation: "gen-1".to_owned(),
                request_id: "req-0".to_owned(),
                based_on_observation_seq: 0,
                state_revision: 0,
                ready: false,
                command_consumed: false,
                scopes: Vec::new(),
                workspaces: Vec::new(),
                outputs: Vec::new(),
                windows: Vec::new(),
                transfers,
                focus_request: None,
                drag_preview: None,
                gesture_settled: None,
                operation_settled: Vec::new(),
                native_creations: Vec::new(),
                warp_request: None,
                presentation: crate::runtime::empty_world_presentation_v4(),
                intent_desired: Vec::new(),
                switch_settled: Vec::new(),
                diagnostics: Vec::new(),
            };
            reply
        };

        assert_eq!(
            crate::world_dispatch::stale_detached_candidate_guard(
                &reply(vec![view(Some(exact.clone()))]),
                1,
                &homes,
                Some(&proof),
            ),
            Ok(())
        );
        // Proof with no correlated View at all (e.g. every transfer is on
        // another output) is a missing claim, never acceptance.
        assert_eq!(
            crate::world_dispatch::stale_detached_candidate_guard(
                &reply(Vec::new()),
                1,
                &homes,
                Some(&proof),
            ),
            Err(())
        );
        // A correlated View missing its frozen destination is Home drift.
        assert_eq!(
            crate::world_dispatch::stale_detached_candidate_guard(
                &reply(vec![view(None)]),
                1,
                &homes,
                Some(&proof),
            ),
            Err(())
        );
        // A different member/surface never satisfies the proved target.
        let wrong = DetachedDestination {
            surface: crate::runtime_types::detached_ref(home.output.clone(), 320),
            member: WindowId("1_8".to_owned()),
        };
        assert_eq!(
            crate::world_dispatch::stale_detached_candidate_guard(
                &reply(vec![view(Some(wrong))]),
                1,
                &homes,
                Some(&proof),
            ),
            Err(())
        );
        // A still-stale sibling home newly targeted without a probe rejects.
        let sibling = DomainKey {
            output: home.output.clone(),
            workspace: WorkspaceId("ws-b".to_owned()),
        };
        let sibling_view = TransferTargetV4 {
            target_workspace: sibling.workspace.0.clone(),
            ..view(None)
        };
        assert_eq!(
            crate::world_dispatch::stale_detached_candidate_guard(
                &reply(vec![sibling_view]),
                1,
                &BTreeSet::from([sibling]),
                Some(&proof),
            ),
            Err(())
        );
        // Ordinary fast path: no proof and no stale home target accepts.
        assert_eq!(
            crate::world_dispatch::stale_detached_candidate_guard(
                &reply(Vec::new()),
                1,
                &homes,
                None,
            ),
            Ok(())
        );
        let ordinary = DomainKey {
            output: home.output.clone(),
            workspace: WorkspaceId("ws-ordinary".to_owned()),
        };
        assert_eq!(
            crate::world_dispatch::stale_detached_candidate_guard(
                &reply(vec![TransferTargetV4 {
                    target_workspace: ordinary.workspace.0.clone(),
                    ..view(None)
                }]),
                1,
                &homes,
                None,
            ),
            Ok(())
        );
    }
}
