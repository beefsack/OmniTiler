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
    audit_roster_hold(&mut retained, 1001, &ctx);
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
    audit_roster_hold(&mut retained, 1001, &ctx);
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
        audit_roster_hold(&mut retained, 1001, &ctx);
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
    audit_roster_hold(&mut retained, 1001, &ctx);
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
        audit_roster_hold(&mut retained, 1001, &ctx);
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
        audit_roster_hold(&mut retained, 1002, &ctx);
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
    audit_roster_hold(&mut retained, 1001, &ctx);
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
