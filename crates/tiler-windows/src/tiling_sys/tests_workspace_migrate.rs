// Item 14 whole-active-workspace migration (R-WS-12 D1-D9/G37) through a
// real `TileLoop` against the real retained Engine (no HWNDs, hooks, or
// live effects): adjacent FULL-rectangle selection, live-observation row
// carry (maximized via `IsZoomed`, fullscreen via the live inference,
// sticky/float/fixed lanes), candidate-Engine `MigrateWorkspace` planning
// with the live G-37 refill mode, commit-last project plus Engine
// publication behind topology/readback/arrival/member/view fences, and
// narrow no-replay refusal on drift. Native setters stay user-owned live
// acceptance; these pin the portable transaction shape plus the headless
// verification gates.
use super::rmax03_adapter_tests::test_state;
use super::{
    HintCx, MonitorArea, ObservedWindow, clear_pending_seams, inject_monitor_areas,
    inject_pending_arrival, inject_pending_arrival_placement, inject_pending_elevated,
    inject_pending_float_move_ok, inject_pending_focus, inject_pending_iconic,
    inject_pending_native_move_ok, inject_pending_observe, inject_pending_overlay_seq,
    inject_pending_placement_normal, inject_pending_suspend, inject_pending_tag, output_for_rect,
    pending_apply_writables, pending_counters, pending_native_transfers, revision_for,
    stamp_engine_event, workspace_domain_for,
};
use crate::snapkey::{QueuedWorkspaceMigrateIntent, SnapEdge};
use crate::workspace::{MemberLoc, WindowKey};
use std::collections::BTreeSet;
use tiler_core::directional::{Direction, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;

fn area(device: &str, work: Rect, full: Rect) -> MonitorArea {
    MonitorArea {
        device: device.to_owned(),
        work,
        full,
    }
}

fn areas() -> Vec<MonitorArea> {
    vec![
        area(
            "mon-1",
            Rect {
                x: 0,
                y: 40,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "mon-2",
            Rect {
                x: 1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
            Rect {
                x: 1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
    ]
}

fn key(hwnd: u64, pid: u32) -> WindowKey {
    WindowKey {
        hwnd,
        pid,
        creation: format!("c{hwnd:016x}"),
    }
}

fn me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 5001,
        process_creation: "creation-me".to_owned(),
        user_sid: "S-1-migrate".to_owned(),
        session_id: 7,
        exe_path: "C:\\test\\app.exe".to_owned(),
    }
}

fn observed_window(
    key: &WindowKey,
    token: &str,
    frame: Rect,
    me: &crate::model::ProcessIdentity,
) -> ObservedWindow {
    ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer: frame,
        visible: frame,
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: me.exe_path.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }
}

struct MigrateRig {
    state: super::TileLoop,
    areas: Vec<MonitorArea>,
    fulls: Vec<Rect>,
    me: crate::model::ProcessIdentity,
    keys: Vec<WindowKey>,
    tokens: Vec<String>,
    source_output: String,
    source_ws: String,
    target_output: String,
    target_ws: String,
    observed: Vec<ObservedWindow>,
    source_rows: Vec<crate::workspace_owner::OwnerRow>,
    intent: QueuedWorkspaceMigrateIntent,
}

/// Two-output rig with two tiled members on the source active workspace
/// plus a real retained Engine session: the destination holds its own
/// current workspace with no session (the Engine refuses carried target
/// residue, so the target stays adapter-owned until the move).
fn rig() -> MigrateRig {
    clear_pending_seams();
    let mut state = test_state();
    let areas = areas();
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.workspaces.ensure_output("mon-1");
    state.workspaces.ensure_output("mon-2");
    let ws_a = state.workspaces.active_id("mon-1").expect("active");
    let ws_b = state.workspaces.active_id("mon-2").expect("active");
    state.workspaces.set_tiled("mon-1", &ws_a, true);
    state.workspaces.set_tiled("mon-2", &ws_b, true);
    let src_bounds = workspace_domain_for("mon-1", &ws_a, &areas, 8, 8)
        .expect("src domain")
        .0
        .bounds;
    let me = me();
    let tokens = vec!["mig-a".to_owned(), "mig-b".to_owned()];
    let mut keys = Vec::new();
    let mut observed = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let k = key(0x6100u64 + i as u64, 9100u32 + i as u32);
        let frame = Rect {
            x: 100 + i as i32 * 500,
            y: 100,
            w: 400,
            h: 300,
        };
        state.member_tokens.insert(k.clone(), token.clone());
        state.member_identity.insert(
            k.clone(),
            crate::model::ProcessIdentity {
                pid: k.pid,
                process_creation: k.creation.clone(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                exe_path: me.exe_path.clone(),
            },
        );
        state
            .member_tags
            .insert(k.clone(), format!("tag-{:x}", k.hwnd));
        inject_pending_tag(k.hwnd, Some(format!("tag-{:x}", k.hwnd)));
        assert!(state.workspaces.assign(k.clone(), "mon-1", &ws_a, false));
        observed.push(observed_window(&k, token, frame, &me));
        keys.push(k);
    }
    // Real retained Engine session over the source rows (tiled pair,
    // first member focused).
    let domain = crate::workspace_owner::workspace_domain("mon-1", &ws_a, src_bounds, 8);
    let carried: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = tokens
        .iter()
        .map(|token| {
            (
                WindowId(token.clone()),
                src_bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            )
        })
        .collect();
    let revision = state
        .engine
        .session(&domain.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("migrate-seed").expect("correlation");
    let fp = crate::tiling::fingerprint(
        &tokens
            .iter()
            .map(|t| (t.clone(), src_bounds))
            .collect::<Vec<_>>(),
    );
    let event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(tokens[0].clone())),
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed must converge, got {reply:?}"
    );
    let source_rows: Vec<crate::workspace_owner::OwnerRow> = tokens
        .iter()
        .map(|token| crate::workspace_owner::OwnerRow {
            token: token.clone(),
            rect: Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        })
        .collect();
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    MigrateRig {
        state,
        areas,
        fulls,
        me,
        keys,
        tokens,
        source_output: "mon-1".to_owned(),
        source_ws: ws_a,
        target_output: "mon-2".to_owned(),
        target_ws: ws_b,
        observed,
        source_rows,
        intent: QueuedWorkspaceMigrateIntent {
            direction: Direction::Right,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        },
    }
}

/// Predict the migrated tiled geometry by replaying the production
/// `MigrateWorkspace` construction on a cloned Engine (no mutation):
/// success tests stage the prediction for the arrival proof.
fn predict_migrate_rects(
    rig: &MigrateRig,
    correlation: &CorrelationId,
) -> Vec<crate::workspace_owner::PlannedWrite> {
    let (source_base, source_key) =
        workspace_domain_for(&rig.source_output, &rig.source_ws, &rig.areas, 8, 8)
            .expect("source domain");
    let (target_base, target_key) =
        workspace_domain_for(&rig.target_output, &rig.source_ws, &rig.areas, 8, 8)
            .expect("target domain");
    let rows: Vec<crate::workspace_owner::MigrateRow> = rig
        .tokens
        .iter()
        .map(|token| crate::workspace_owner::MigrateRow {
            token: token.clone(),
            rect: Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            fit_excluded: false,
        })
        .collect();
    let combined_fp = {
        let mut pairs: Vec<(String, Rect)> = rig
            .source_rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        crate::tiling::fingerprint(&pairs)
    };
    let mut candidate = rig.state.engine.clone();
    let event = crate::workspace_owner::build_migrate_event(
        &rig.state.owner,
        &rig.state.generation,
        correlation,
        revision_for(&rig.state, &rig.source_output, &rig.source_ws),
        combined_fp,
        (source_base, source_key),
        (target_base, target_key),
        &rows,
        "",
        "right",
        8,
    )
    .expect("migrate event predicts");
    let mut event = event;
    stamp_engine_event(&rig.state, &mut event);
    let reply = candidate.handle(&event);
    crate::workspace_owner::planned_writes(&reply).expect("plan extracts")
}

fn run_migrate(rig: &mut MigrateRig, correlation: &CorrelationId, tick: u64) -> &'static str {
    let mut hint_cx = HintCx::new();
    let migrate_rows = super::migrate_rows_for(
        &rig.state,
        &rig.source_output,
        &rig.source_ws,
        &rig.source_rows,
        &rig.observed,
        &[],
        &rig.fulls,
    )
    .expect("rows assemble");
    let tmp = std::env::temp_dir().join(format!(
        "tiler-migrate-{}-{}",
        std::process::id(),
        correlation.as_str()
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    super::run_workspace_migrate(
        &mut rig.state,
        &rig.me,
        &store,
        &tmp,
        &rig.fulls,
        &rig.areas,
        &rig.observed,
        &[],
        &mut hint_cx,
        &rig.source_output.clone(),
        &rig.source_ws.clone(),
        &rig.source_rows.clone(),
        &migrate_rows,
        &rig.target_output.clone(),
        &rig.target_ws.clone(),
        &rig.intent,
        tick,
        correlation,
        correlation.as_str(),
    )
}

#[test]
fn migrate_rows_carry_live_overlay_sticky_and_float_flags() {
    // Touched `false` placeholders replaced with observations: the
    // maximized flag re-reads live, fullscreen re-reads live, sticky
    // rides the runtime set, floating rides the Engine row, and the
    // float exception carries fit-exclusion while plain rows do not.
    let mut rig = rig();
    let sticky_key = rig.keys[1].clone();
    rig.state.sticky.insert(sticky_key, false);
    let float_token = rig.tokens[1].clone();
    for row in rig.source_rows.iter_mut() {
        if row.token == float_token {
            row.floating = true;
        }
    }
    // Live snapshot: first member maximized only, second plain. One
    // entry per `live_overlay_flags` call, consumed in member order.
    inject_pending_overlay_seq(vec![(true, Some(false)), (false, Some(false))]);
    let rows = super::migrate_rows_for(
        &rig.state,
        &rig.source_output,
        &rig.source_ws,
        &rig.source_rows,
        &rig.observed,
        &[],
        &rig.fulls,
    )
    .expect("rows assemble");
    assert_eq!(rows.len(), 2);
    assert!(rows[0].maximized, "live maximized observation carried");
    assert!(!rows[0].fullscreen);
    assert!(!rows[0].sticky);
    assert!(!rows[0].floating);
    assert!(!rows[0].fit_excluded, "plain rows carry no fit exclusion");
    assert!(!rows[1].maximized);
    assert!(rows[1].sticky, "sticky rides the runtime set");
    assert!(rows[1].floating, "float rides the Engine row");
    assert!(
        rows[1].fit_excluded,
        "float exception carries fit exclusion for the Engine gate"
    );
    clear_pending_seams();
}

#[test]
fn migrate_verified_tail_commits_both_authorities_without_focus() {
    // Empty-focus migration (no foreground member): the Engine relocates
    // the retained domain wholesale, the project rekeys every member to
    // the target output with the moved id shown after the target current,
    // and no focus setter runs. Staged arrival frames prove the transfer.
    let mut rig = rig();
    let correlation = CorrelationId::parse("corr-migrate-commit").expect("correlation");
    let writes = predict_migrate_rects(&rig, &correlation);
    assert!(!writes.is_empty(), "relocated domain plans tiled geometry");
    for write in &writes {
        assert_eq!(
            output_for_rect(&rig.areas, &write.rect),
            "mon-2",
            "planned arrival must land on the destination monitor"
        );
        let key = rig
            .keys
            .iter()
            .find(|key| {
                rig.state
                    .member_tokens
                    .get(*key)
                    .is_some_and(|t| t == &write.window.0)
            })
            .expect("planned member");
        // Pre-move cache stays on the source frame; the fresh post-setter
        // boundary readback (distinct injection) proves arrival on target.
        let pre = rig
            .observed
            .iter()
            .find(|w| w.hwnd == key.hwnd)
            .map(|w| w.visible)
            .expect("pre-state frame");
        rig.state.stable.insert(key.hwnd, pre);
        inject_pending_arrival(key.hwnd, write.rect);
    }
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    // Both members read plain (no overlay): two live-overlay reads.
    inject_pending_overlay_seq(vec![(false, Some(false)), (false, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let moved = rig.source_ws.clone();
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_migrate(&mut rig, &correlation, 31);
    assert_eq!(outcome, "migrate-applied");
    for key in &rig.keys {
        let loc = rig.state.workspaces.member_loc(key).expect("homed");
        assert_eq!(loc.output, "mon-2", "membership rekeys wholesale");
        assert_eq!(loc.workspace, moved, "backing id preserved on target");
    }
    assert_eq!(
        rig.state.workspaces.active_id("mon-2").as_deref(),
        Some(moved.as_str()),
        "target shows the migrated workspace"
    );
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "verified transfer publishes the Engine once"
    );
    assert_eq!(
        pending_counters().focus,
        0,
        "empty-focus migration takes no focus setter"
    );
    assert!(
        rig.state.workspaces.active_id("mon-1").is_some()
            || rig.state.workspaces.scoped_ring_ids("mon-1").is_empty(),
        "source refills through the shared selector or empties for lifecycle"
    );
    clear_pending_seams();
}

#[test]
fn migrate_late_topology_drift_retains_source_and_engine() {
    // Fresh-topology drift after planning refuses with no commit and no
    // replay: both retained authorities stand and the next observation
    // converges.
    let mut rig = rig();
    let correlation = CorrelationId::parse("corr-migrate-topology").expect("correlation");
    let writes = predict_migrate_rects(&rig, &correlation);
    for write in &writes {
        let key = rig
            .keys
            .iter()
            .find(|key| {
                rig.state
                    .member_tokens
                    .get(*key)
                    .is_some_and(|t| t == &write.window.0)
            })
            .expect("planned member");
        let pre = rig
            .observed
            .iter()
            .find(|w| w.hwnd == key.hwnd)
            .map(|w| w.visible)
            .expect("pre-state frame");
        rig.state.stable.insert(key.hwnd, pre);
        inject_pending_arrival(key.hwnd, write.rect);
    }
    let mut drifted = rig.areas.clone();
    drifted[1].full.x = 2000;
    inject_monitor_areas(Some(drifted));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![(false, Some(false)), (false, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let moved = rig.source_ws.clone();
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_migrate(&mut rig, &correlation, 32);
    // The pre-commit fence sees the drift first: the pinned FULL pair no
    // longer matches the fresh topology, so the transaction refuses as
    // `topology-changed` with no commit and no replay.
    assert_eq!(outcome, "topology-changed");
    for key in &rig.keys {
        let loc = rig.state.workspaces.member_loc(key).expect("homed");
        assert_eq!(loc.output, "mon-1", "drift keeps source membership");
        assert_eq!(loc.workspace, moved);
    }
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "drift keeps the retained Engine"
    );
    assert_eq!(
        pending_counters().focus,
        0,
        "refused transfer skips follow-focus"
    );
    clear_pending_seams();
}

#[test]
fn migrate_sticky_member_stays_homed_on_source_refill() {
    // D7: a sticky all-desktops member never rides the moved entry. It
    // re-homes to the source refill while migrating members relocate,
    // and the transfer still verifies.
    let mut rig = rig();
    let sticky_key = rig.keys[1].clone();
    rig.state.sticky.insert(sticky_key.clone(), false);
    // Sticky member reads plain; the float-move tail skips sticky rows.
    inject_pending_overlay_seq(vec![(false, Some(false)), (false, Some(false))]);
    let rows = super::migrate_rows_for(
        &rig.state,
        &rig.source_output,
        &rig.source_ws,
        &rig.source_rows,
        &rig.observed,
        &[],
        &rig.fulls,
    )
    .expect("rows assemble");
    let sticky_row = rows
        .iter()
        .find(|row| row.token == rig.tokens[1])
        .expect("sticky row");
    assert!(sticky_row.sticky);
    let migrating: BTreeSet<WindowKey> = rig
        .state
        .workspaces
        .workspace_members(&rig.source_output, &rig.source_ws)
        .difference(&BTreeSet::from([sticky_key.clone()]))
        .cloned()
        .collect();
    assert_eq!(migrating.len(), 1, "only the non-sticky member migrates");
    // Project-level proof through the stable-ID primitive plus the
    // shared both-view verification (no native effects here).
    let mut candidate = rig.state.workspaces.clone();
    let outcome = candidate
        .migrate_workspace_excluding(
            &rig.source_output,
            &rig.target_output,
            rig.state
                .settings_live
                .as_ref()
                .map(|live| live.settings.core.migration_source_refill_mode())
                .unwrap_or_default(),
            &BTreeSet::from([sticky_key.clone()]),
        )
        .expect("primitive rekeys");
    let source_members = candidate.workspace_members(
        &rig.source_output,
        outcome.source_refill.as_deref().unwrap_or(""),
    );
    let target_members = candidate.workspace_members(&rig.target_output, &outcome.moved_id);
    let sticky_set = BTreeSet::from([sticky_key]);
    if outcome.source_refill.is_none() {
        assert!(
            !target_members.iter().any(|key| sticky_set.contains(key)),
            "sticky never lands on the target"
        );
    } else {
        assert!(
            crate::workspace_owner::verify_migration_transfer(
                &migrating,
                &sticky_set,
                &source_members,
                &target_members,
                &outcome.moved_id,
                outcome.source_refill.as_deref(),
                candidate.active_id(&rig.source_output).as_deref(),
                candidate.active_id(&rig.target_output).as_deref(),
            ),
            "project transfer verifies with sticky homed on the refill"
        );
    }
    let _ = MemberLoc {
        output: String::new(),
        workspace: String::new(),
        hidden: false,
    };
    clear_pending_seams();
}

struct AllKindsRig {
    state: super::TileLoop,
    areas: Vec<MonitorArea>,
    fulls: Vec<Rect>,
    me: crate::model::ProcessIdentity,
    keys: Vec<WindowKey>,
    tokens: Vec<String>,
    source_output: String,
    source_ws: String,
    target_output: String,
    target_ws: String,
    observed_visible: Vec<ObservedWindow>,
    source_rows: Vec<crate::workspace_owner::OwnerRow>,
    intent: QueuedWorkspaceMigrateIntent,
}

/// Five-member rig proving ALL-member native transfer: tiled plain,
/// maximized overlay, plain float, minimized (iconic omitted) and hidden.
/// The retained Engine seeds all five tiled (first focused); the float
/// rides `source_rows.floating`, the overlay rides live `IsZoomed`, the
/// minimized rides `minimized_omitted` plus iconic plus `member_rects`,
/// the hidden rides `assign(..., true)` plus `member_rects`. Visible
/// observation carries only the three visible members, mirroring
/// production (concealed members keep no live frame).
fn rig_all_kinds() -> AllKindsRig {
    clear_pending_seams();
    let mut state = test_state();
    let areas = areas();
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.workspaces.ensure_output("mon-1");
    state.workspaces.ensure_output("mon-2");
    let ws_a = state.workspaces.active_id("mon-1").expect("active");
    let ws_b = state.workspaces.active_id("mon-2").expect("active");
    state.workspaces.set_tiled("mon-1", &ws_a, true);
    state.workspaces.set_tiled("mon-2", &ws_b, true);
    let src_bounds = workspace_domain_for("mon-1", &ws_a, &areas, 8, 8)
        .expect("src domain")
        .0
        .bounds;
    let me = me();
    let tokens = vec![
        "mig-a".to_owned(),
        "mig-b".to_owned(),
        "mig-c".to_owned(),
        "mig-d".to_owned(),
        "mig-e".to_owned(),
    ];
    let mut keys = Vec::new();
    let mut observed_visible = Vec::new();
    let mut source_rows = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let k = key(0x6200u64 + i as u64, 9200u32 + i as u32);
        let frame = Rect {
            x: 100 + i as i32 * 200,
            y: 100,
            w: 400,
            h: 300,
        };
        state.member_tokens.insert(k.clone(), token.clone());
        state.member_identity.insert(
            k.clone(),
            crate::model::ProcessIdentity {
                pid: k.pid,
                process_creation: k.creation.clone(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                exe_path: me.exe_path.clone(),
            },
        );
        state
            .member_tags
            .insert(k.clone(), format!("tag-{:x}", k.hwnd));
        inject_pending_tag(k.hwnd, Some(format!("tag-{:x}", k.hwnd)));
        let hidden = i == 4;
        assert!(state.workspaces.assign(k.clone(), "mon-1", &ws_a, hidden));
        // Concealed normal frames live in `member_rects` (the placement
        // tail remaps them without changing showCmd).
        if i == 3 || i == 4 {
            state.member_rects.insert(token.clone(), frame);
        }
        if i < 3 {
            observed_visible.push(observed_window(&k, token, frame, &me));
        }
        source_rows.push(crate::workspace_owner::OwnerRow {
            token: token.clone(),
            rect: frame,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: i == 2,
        });
        keys.push(k);
    }
    // Minimized member: iconic plus omitted (frameless non-overlay rows
    // converge to `MinimizedSlot`, but migration still carries it).
    let min_key = keys[3].clone();
    state.minimized_omitted.insert(min_key.clone());
    inject_pending_iconic(vec![min_key.hwnd]);
    let domain = crate::workspace_owner::workspace_domain("mon-1", &ws_a, src_bounds, 8);
    let carried: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = tokens
        .iter()
        .map(|token| {
            (
                WindowId(token.clone()),
                src_bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            )
        })
        .collect();
    let revision = state
        .engine
        .session(&domain.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("migrate-seed").expect("correlation");
    let fp = crate::tiling::fingerprint(
        &tokens
            .iter()
            .map(|t| (t.clone(), src_bounds))
            .collect::<Vec<_>>(),
    );
    let event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(tokens[0].clone())),
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed must converge, got {reply:?}"
    );
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_float_move_ok(true);
    inject_pending_native_move_ok(true);
    AllKindsRig {
        state,
        areas,
        fulls,
        me,
        keys,
        tokens,
        source_output: "mon-1".to_owned(),
        source_ws: ws_a,
        target_output: "mon-2".to_owned(),
        target_ws: ws_b,
        observed_visible,
        source_rows,
        intent: QueuedWorkspaceMigrateIntent {
            direction: Direction::Right,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        },
    }
}

fn run_all_migrate(rig: &mut AllKindsRig, correlation: &CorrelationId, tick: u64) -> &'static str {
    let mut hint_cx = HintCx::new();
    let migrate_rows = super::migrate_rows_for(
        &rig.state,
        &rig.source_output,
        &rig.source_ws,
        &rig.source_rows,
        &rig.observed_visible,
        &[],
        &rig.fulls,
    )
    .expect("rows assemble");
    let tmp = std::env::temp_dir().join(format!(
        "tiler-migrate-all-{}-{}",
        std::process::id(),
        correlation.as_str()
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    super::run_workspace_migrate(
        &mut rig.state,
        &rig.me,
        &store,
        &tmp,
        &rig.fulls,
        &rig.areas,
        &rig.observed_visible,
        &[],
        &mut hint_cx,
        &rig.source_output.clone(),
        &rig.source_ws.clone(),
        &rig.source_rows.clone(),
        &migrate_rows,
        &rig.target_output.clone(),
        &rig.target_ws.clone(),
        &rig.intent,
        tick,
        correlation,
        correlation.as_str(),
    )
}

/// Manual mixed-kind rows for arrival prediction without consuming the
/// scripted overlay queue: tiled plain, maximized overlay, float,
/// minimized plain, hidden plain.
fn mixed_rows_for_predict(rig: &AllKindsRig) -> Vec<crate::workspace_owner::MigrateRow> {
    rig.tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            let rect = rig.source_rows[i].rect;
            crate::workspace_owner::MigrateRow {
                token: token.clone(),
                rect,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: i == 2,
                fullscreen: false,
                maximized: i == 1,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                fit_excluded: i == 2,
            }
        })
        .collect()
}

fn predict_mixed_writes(
    rig: &AllKindsRig,
    correlation: &CorrelationId,
) -> Vec<crate::workspace_owner::PlannedWrite> {
    let (source_base, source_key) =
        workspace_domain_for(&rig.source_output, &rig.source_ws, &rig.areas, 8, 8)
            .expect("source domain");
    let (target_base, target_key) =
        workspace_domain_for(&rig.target_output, &rig.source_ws, &rig.areas, 8, 8)
            .expect("target domain");
    let rows = mixed_rows_for_predict(rig);
    let combined_fp = {
        let mut pairs: Vec<(String, Rect)> = rig
            .source_rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        crate::tiling::fingerprint(&pairs)
    };
    let mut candidate = rig.state.engine.clone();
    let event = crate::workspace_owner::build_migrate_event(
        &rig.state.owner,
        &rig.state.generation,
        correlation,
        revision_for(&rig.state, &rig.source_output, &rig.source_ws),
        combined_fp,
        (source_base, source_key),
        (target_base, target_key),
        &rows,
        "",
        "right",
        8,
    )
    .expect("migrate event predicts");
    let mut event = event;
    stamp_engine_event(&rig.state, &mut event);
    let reply = candidate.handle(&event);
    crate::workspace_owner::planned_writes(&reply).expect("plan extracts")
}

#[test]
fn migrate_all_member_kinds_remap_output_with_per_effect_fences() {
    // D7/D8: tiled plain writes geometry; overlay, float, minimized and
    // hidden remap natively (move-only, show-state preserved) with fresh
    // per-effect fences and per-member arrival. The writable set holds
    // only the tiled plain token; the native seam records the other four
    // with their production boundary rects on the target.
    let mut rig = rig_all_kinds();
    let correlation = CorrelationId::parse("corr-migrate-all").expect("correlation");
    let writes = predict_mixed_writes(&rig, &correlation);
    let tiled_write = writes
        .iter()
        .find(|w| w.window.0 == "mig-a")
        .expect("tiled plain plans geometry");
    // Pre-move cache stays on the source frame; fresh post-setter
    // boundary readback (distinct injection) proves arrival on target.
    let pre_a = rig.source_rows[0].rect;
    rig.state.stable.insert(rig.keys[0].hwnd, pre_a);
    inject_pending_arrival(rig.keys[0].hwnd, tiled_write.rect);
    let source_full = rig.areas[0].full;
    let target_full = rig.areas[1].full;
    // Float arrival: same size on the target; overlay arrival: any frame
    // on the target (monitor/identity, never the old exact rect).
    // Concealed arrivals prove fresh placement on the target.
    for (idx, token_idx) in [(2usize, 2usize), (1usize, 1usize)] {
        let _ = idx;
        let key = &rig.keys[token_idx];
        let frame = rig.source_rows[token_idx].rect;
        let moved =
            crate::workspace_owner::translate_rect_to_output(frame, source_full, target_full);
        assert_eq!(
            output_for_rect(&rig.areas, &moved),
            "mon-2",
            "test moved math must land on target"
        );
        rig.state.stable.insert(key.hwnd, frame);
        inject_pending_arrival(key.hwnd, moved);
    }
    for idx in [3usize, 4] {
        let key = &rig.keys[idx];
        let frame = rig.source_rows[idx].rect;
        let moved =
            crate::workspace_owner::translate_rect_to_output(frame, source_full, target_full);
        rig.state.stable.insert(key.hwnd, frame);
        inject_pending_arrival_placement(key.hwnd, moved);
    }
    // Maximized overlay remap reads the live restore frame, never the
    // observed maximized frame: stage a restore rect with a distinct
    // size in workspace coordinates on the source work area (unequal
    // top inset: work y=40 vs target work y=0 exercises the WORK
    // conversion, not just the FULL delta).
    inject_pending_placement_normal(
        rig.keys[1].hwnd,
        Rect {
            x: 100,
            y: 60,
            w: 320,
            h: 240,
        },
        true,
    );
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed_visible.clone(), Vec::new(), false);
    // 3 visible rows (plain, overlay-max, float), 4 per-effect fence
    // reads in transfer order (overlay, float, minimized, hidden), plus
    // the overlay arrival flag re-read after its setter (native re-fit
    // may change the maximized frame; the flag proves the slot).
    inject_pending_overlay_seq(vec![
        (false, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (true, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (false, Some(false)),
        (false, Some(false)),
    ]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_all_migrate(&mut rig, &correlation, 41);
    assert_eq!(outcome, "migrate-applied");
    let writables = pending_apply_writables();
    assert_eq!(writables.len(), 1, "one combined tiled apply");
    assert!(
        writables[0].contains("mig-a"),
        "tiled plain takes geometry: {:?}",
        writables[0]
    );
    for excluded in ["mig-b", "mig-c", "mig-d", "mig-e"] {
        assert!(
            !writables[0].contains(excluded),
            "native member takes no tiled geometry: {excluded}"
        );
    }
    let native = pending_native_transfers();
    let kinds: std::collections::BTreeMap<String, &'static str> = native
        .iter()
        .map(|(_, _, kind)| (String::new(), *kind))
        .collect();
    let _ = kinds;
    let by_hwnd: std::collections::HashMap<u64, (Rect, &'static str)> = native
        .iter()
        .map(|(hwnd, rect, kind)| (*hwnd, (*rect, *kind)))
        .collect();
    for (idx, kind) in [
        (1usize, "overlay"),
        (2, "float"),
        (3, "minimized"),
        (4, "hidden"),
    ] {
        let key = &rig.keys[idx];
        let (moved, got) = by_hwnd
            .get(&key.hwnd)
            .unwrap_or_else(|| panic!("native transfer recorded for {} ({kind})", rig.tokens[idx]));
        assert_eq!(*got, kind, "transfer kind for {}", rig.tokens[idx]);
        assert_eq!(
            output_for_rect(&rig.areas, moved),
            "mon-2",
            "native remap lands on target for {}",
            rig.tokens[idx]
        );
        if kind == "float" {
            let frame = rig.source_rows[idx].rect;
            assert_eq!(moved.w, frame.w, "float preserves width");
            assert_eq!(moved.h, frame.h, "float preserves height");
        }
        if kind == "overlay" {
            // Maximized remap preserves the restore size (320x240
            // staged above), never the observed maximized frame size.
            assert_eq!(moved.w, 320, "maximized remap preserves restore width");
            assert_eq!(moved.h, 240, "maximized remap preserves restore height");
        }
    }
    assert!(
        !by_hwnd.contains_key(&rig.keys[0].hwnd),
        "tiled plain never takes the native move path"
    );
    for (idx, key) in rig.keys.iter().enumerate() {
        let loc = rig.state.workspaces.member_loc(key).expect("homed");
        assert_eq!(loc.output, "mon-2", "membership rekeys {}", rig.tokens[idx]);
        assert_eq!(loc.workspace, rig.source_ws);
    }
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "verified transfer publishes the Engine once"
    );
    clear_pending_seams();
}

#[test]
fn migrate_failing_overlay_arrival_refuses_without_publish() {
    // Setter succeeds but the overlay reads back on the source: arrival
    // fails, nothing publishes, reconcile converges, no replay.
    let mut rig = rig_all_kinds();
    let correlation = CorrelationId::parse("corr-migrate-arrival").expect("correlation");
    let writes = predict_mixed_writes(&rig, &correlation);
    let tiled_write = writes
        .iter()
        .find(|w| w.window.0 == "mig-a")
        .expect("tiled plain plans geometry");
    rig.state
        .stable
        .insert(rig.keys[0].hwnd, rig.source_rows[0].rect);
    inject_pending_arrival(rig.keys[0].hwnd, tiled_write.rect);
    let source_full = rig.areas[0].full;
    let target_full = rig.areas[1].full;
    let float_moved = crate::workspace_owner::translate_rect_to_output(
        rig.source_rows[2].rect,
        source_full,
        target_full,
    );
    rig.state
        .stable
        .insert(rig.keys[2].hwnd, rig.source_rows[2].rect);
    inject_pending_arrival(rig.keys[2].hwnd, float_moved);
    // Overlay stages on the source (wrong output): the move-only setter
    // succeeds but fresh monitor verification fails. Pre-state stays on
    // source; fresh arrival stays on source too.
    rig.state
        .stable
        .insert(rig.keys[1].hwnd, rig.source_rows[1].rect);
    inject_pending_arrival(rig.keys[1].hwnd, rig.source_rows[1].rect);
    inject_pending_placement_normal(
        rig.keys[1].hwnd,
        Rect {
            x: 100,
            y: 60,
            w: 320,
            h: 240,
        },
        true,
    );
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed_visible.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![
        (false, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (true, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (false, Some(false)),
        (false, Some(false)),
    ]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_all_migrate(&mut rig, &correlation, 42);
    assert_eq!(outcome, "migrate-unverified");
    for key in &rig.keys {
        let loc = rig.state.workspaces.member_loc(key).expect("homed");
        assert_eq!(loc.output, "mon-1", "failed arrival keeps source");
    }
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "failed arrival publishes nothing"
    );
    assert!(
        pending_counters().reconcile >= 1,
        "partial failure reconciles without replay"
    );
    // The overlay attempted the native move (setter succeeded) before
    // arrival refused: the seam proves the attempt happened.
    assert!(
        pending_native_transfers()
            .iter()
            .any(|(hwnd, _, kind)| *hwnd == rig.keys[1].hwnd && *kind == "overlay"),
        "overlay transfer attempted before arrival refusal"
    );
    clear_pending_seams();
}

#[test]
fn migrate_late_identity_after_first_transfer_refuses_without_publish() {
    // Late per-effect fence drift after the first native transfer
    // (overlay) succeeds: the float's live overlay no longer matches its
    // carried row, so the tail stops before its setter, reconciles,
    // publishes nothing. Proves no replay and no publication on
    // failed arrivals/late fences. (Static lifetime tags cannot drift
    // mid-run offline, so the late fence rides the queued overlay read;
    // early lifetime fences are covered by the pre-plan gates.)
    let mut rig = rig_all_kinds();
    let correlation = CorrelationId::parse("corr-migrate-identity").expect("correlation");
    let writes = predict_mixed_writes(&rig, &correlation);
    let tiled_write = writes
        .iter()
        .find(|w| w.window.0 == "mig-a")
        .expect("tiled plain plans geometry");
    rig.state
        .stable
        .insert(rig.keys[0].hwnd, rig.source_rows[0].rect);
    inject_pending_arrival(rig.keys[0].hwnd, tiled_write.rect);
    let source_full = rig.areas[0].full;
    let target_full = rig.areas[1].full;
    for idx in [1usize, 2] {
        let moved = crate::workspace_owner::translate_rect_to_output(
            rig.source_rows[idx].rect,
            source_full,
            target_full,
        );
        rig.state
            .stable
            .insert(rig.keys[idx].hwnd, rig.source_rows[idx].rect);
        inject_pending_arrival(rig.keys[idx].hwnd, moved);
    }
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed_visible.clone(), Vec::new(), false);
    // Rows (plain, overlay-max, float), overlay fence, overlay arrival
    // flag, then the float fence drifts maximized true vs carried false.
    inject_pending_placement_normal(
        rig.keys[1].hwnd,
        Rect {
            x: 100,
            y: 60,
            w: 320,
            h: 240,
        },
        true,
    );
    inject_pending_overlay_seq(vec![
        (false, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (true, Some(false)),
        (true, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (false, Some(false)),
    ]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_all_migrate(&mut rig, &correlation, 43);
    assert_eq!(outcome, "migrate-unverified");
    for key in &rig.keys {
        if *key == rig.keys[2] {
            continue;
        }
        let loc = rig.state.workspaces.member_loc(key).expect("homed");
        assert_eq!(loc.output, "mon-1", "late identity keeps source");
    }
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "late identity publishes nothing"
    );
    let native = pending_native_transfers();
    assert!(
        native
            .iter()
            .any(|(hwnd, _, kind)| *hwnd == rig.keys[1].hwnd && *kind == "overlay"),
        "first transfer ran before the late fence"
    );
    assert!(
        !native.iter().any(|(hwnd, _, _)| *hwnd == rig.keys[2].hwnd),
        "drifted member takes no setter after its fence fails"
    );
    assert!(
        pending_counters().reconcile >= 1,
        "late identity reconciles without replay"
    );
    clear_pending_seams();
}

#[test]
fn migrate_untiled_source_moves_only_without_tiled_geometry() {
    // D4 untiled source: no both-tiled gate, no Engine dispatch, no
    // tiled geometry. Both plain members remap move-only via the float
    // path with fresh size-preserved arrival; with no retained source
    // session the Engine stays untouched.
    let mut rig = rig();
    rig.state
        .workspaces
        .set_tiled(&rig.source_output, &rig.source_ws, false);
    // Release the seeded source session: untiled workspaces carry no
    // Engine domain, so the migration must see no stale retained session.
    let (_, source_key) =
        workspace_domain_for(&rig.source_output, &rig.source_ws, &rig.areas, 8, 8)
            .expect("source domain");
    rig.state.engine.remove(&source_key);
    let source_full = rig.areas[0].full;
    let target_full = rig.areas[1].full;
    for (idx, key) in rig.keys.iter().enumerate() {
        let frame = rig.source_rows[idx].rect;
        let moved =
            crate::workspace_owner::translate_rect_to_output(frame, source_full, target_full);
        rig.state.stable.insert(key.hwnd, frame);
        inject_pending_arrival(key.hwnd, moved);
    }
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![(false, Some(false)), (false, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let correlation = CorrelationId::parse("corr-migrate-untiled-src").expect("correlation");
    let outcome = run_migrate(&mut rig, &correlation, 51);
    assert_eq!(outcome, "migrate-applied");
    assert!(
        pending_apply_writables().is_empty(),
        "untiled source takes no tiled apply"
    );
    let native = pending_native_transfers();
    for (idx, key) in rig.keys.iter().enumerate() {
        assert!(
            native
                .iter()
                .any(|(hwnd, _, kind)| *hwnd == key.hwnd && *kind == "float"),
            "untiled plain member {} moves move-only",
            rig.tokens[idx]
        );
    }
    for key in &rig.keys {
        let loc = rig.state.workspaces.member_loc(key).expect("homed");
        assert_eq!(loc.output, "mon-2");
        assert_eq!(loc.workspace, rig.source_ws);
    }
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "untiled source with no session keeps the Engine untouched"
    );
    clear_pending_seams();
}

#[test]
fn migrate_untiled_source_with_stale_session_refuses_without_effects() {
    // A stale retained source session on an untiled source refuses before
    // any native effect: the rig seeds a session (release missed), so the
    // migration must report unverified with no native moves and no
    // publication, reconciling instead of straddling the retained domain.
    let mut rig = rig();
    rig.state
        .workspaces
        .set_tiled(&rig.source_output, &rig.source_ws, false);
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![(false, Some(false)), (false, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let correlation = CorrelationId::parse("corr-migrate-untiled-stale").expect("correlation");
    let outcome = run_migrate(&mut rig, &correlation, 56);
    assert_eq!(outcome, "unverified");
    assert!(
        pending_native_transfers().is_empty(),
        "stale session refuses before any native move"
    );
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "stale session refusal publishes nothing"
    );
    assert!(
        pending_counters().reconcile >= 1,
        "stale session refusal reconciles"
    );
    clear_pending_seams();
}

#[test]
fn migrate_untiled_target_prior_does_not_gate() {
    // D4 target prior untiled never gates capability: a tiled source
    // still migrates with full Engine geometry and fresh arrival.
    let mut rig = rig();
    rig.state
        .workspaces
        .set_tiled(&rig.target_output, &rig.target_ws, false);
    let correlation = CorrelationId::parse("corr-migrate-untiled-dst").expect("correlation");
    let writes = predict_migrate_rects(&rig, &correlation);
    assert!(!writes.is_empty());
    for write in &writes {
        let key = rig
            .keys
            .iter()
            .find(|key| {
                rig.state
                    .member_tokens
                    .get(*key)
                    .is_some_and(|t| t == &write.window.0)
            })
            .expect("planned member");
        let pre = rig
            .observed
            .iter()
            .find(|w| w.hwnd == key.hwnd)
            .map(|w| w.visible)
            .expect("pre-state");
        rig.state.stable.insert(key.hwnd, pre);
        inject_pending_arrival(key.hwnd, write.rect);
    }
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![(false, Some(false)), (false, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let outcome = run_migrate(&mut rig, &correlation, 52);
    assert_eq!(outcome, "migrate-applied");
    assert_eq!(
        rig.state.workspaces.active_id("mon-2").as_deref(),
        Some(rig.source_ws.as_str())
    );
    clear_pending_seams();
}

#[test]
fn migrate_occupied_prior_hides_and_refill_shows() {
    // D4/G37 views: an occupied target prior hides (ledger-authorized,
    // stays listed) while the source refill shows; the moved workspace
    // shows on target with membership rekeyed.
    let mut rig = rig();
    // Occupy the target prior with its own member (visible).
    let prior_key = key(0x6300u64, 9300u32);
    let prior_token = "prior-a".to_owned();
    rig.state
        .member_tokens
        .insert(prior_key.clone(), prior_token.clone());
    rig.state.member_identity.insert(
        prior_key.clone(),
        crate::model::ProcessIdentity {
            pid: prior_key.pid,
            process_creation: prior_key.creation.clone(),
            user_sid: rig.me.user_sid.clone(),
            session_id: rig.me.session_id,
            exe_path: rig.me.exe_path.clone(),
        },
    );
    rig.state
        .member_tags
        .insert(prior_key.clone(), "tag-prior".to_owned());
    inject_pending_tag(prior_key.hwnd, Some("tag-prior".to_owned()));
    assert!(
        rig.state
            .workspaces
            .assign(prior_key.clone(), "mon-2", &rig.target_ws, false)
    );
    // Occupy the source refill (inactive ws) and mark it hidden (inactive
    // view); migration must reveal it.
    let refill_id = rig
        .state
        .workspaces
        .scoped_ring_ids("mon-1")
        .into_iter()
        .find(|id| *id != rig.source_ws)
        .expect("refill");
    let refill_key = key(0x6301u64, 9301u32);
    let refill_token = "refill-a".to_owned();
    rig.state
        .member_tokens
        .insert(refill_key.clone(), refill_token);
    rig.state.member_identity.insert(
        refill_key.clone(),
        crate::model::ProcessIdentity {
            pid: refill_key.pid,
            process_creation: refill_key.creation.clone(),
            user_sid: rig.me.user_sid.clone(),
            session_id: rig.me.session_id,
            exe_path: rig.me.exe_path.clone(),
        },
    );
    rig.state
        .member_tags
        .insert(refill_key.clone(), "tag-refill".to_owned());
    inject_pending_tag(refill_key.hwnd, Some("tag-refill".to_owned()));
    assert!(
        rig.state
            .workspaces
            .assign(refill_key.clone(), "mon-1", &refill_id, true)
    );
    let correlation = CorrelationId::parse("corr-migrate-views").expect("correlation");
    let writes = predict_migrate_rects(&rig, &correlation);
    for write in &writes {
        let k = rig
            .keys
            .iter()
            .find(|k| {
                rig.state
                    .member_tokens
                    .get(*k)
                    .is_some_and(|t| t == &write.window.0)
            })
            .expect("planned");
        let pre = rig
            .observed
            .iter()
            .find(|w| w.hwnd == k.hwnd)
            .map(|w| w.visible)
            .expect("pre");
        rig.state.stable.insert(k.hwnd, pre);
        inject_pending_arrival(k.hwnd, write.rect);
    }
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![(false, Some(false)), (false, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let outcome = run_migrate(&mut rig, &correlation, 53);
    assert_eq!(outcome, "migrate-applied");
    // Prior stays listed but hidden; refill shows; moved shows on target.
    assert!(
        rig.state
            .workspaces
            .scoped_ring_ids("mon-2")
            .contains(&rig.target_ws)
    );
    assert!(
        rig.state.workspaces.is_hidden(&prior_key),
        "occupied prior hides"
    );
    assert!(
        !rig.state.workspaces.is_hidden(&refill_key),
        "source refill shows"
    );
    assert_eq!(
        rig.state.workspaces.active_id("mon-2").as_deref(),
        Some(rig.source_ws.as_str())
    );
    clear_pending_seams();
}

#[test]
fn migrate_concealed_placement_mismatch_refuses() {
    // Fresh concealed proof is physical: a minimized member whose
    // post-setter placement still sits on the source refuses (never
    // translation math, never the stale cache).
    let mut rig = rig_all_kinds();
    let correlation = CorrelationId::parse("corr-migrate-place-mismatch").expect("correlation");
    let writes = predict_mixed_writes(&rig, &correlation);
    let tiled_write = writes
        .iter()
        .find(|w| w.window.0 == "mig-a")
        .expect("tiled");
    rig.state
        .stable
        .insert(rig.keys[0].hwnd, rig.source_rows[0].rect);
    inject_pending_arrival(rig.keys[0].hwnd, tiled_write.rect);
    let source_full = rig.areas[0].full;
    let target_full = rig.areas[1].full;
    for idx in [1usize, 2] {
        let moved = crate::workspace_owner::translate_rect_to_output(
            rig.source_rows[idx].rect,
            source_full,
            target_full,
        );
        rig.state
            .stable
            .insert(rig.keys[idx].hwnd, rig.source_rows[idx].rect);
        inject_pending_arrival(rig.keys[idx].hwnd, moved);
    }
    // Minimized member stages on the source (wrong output): setter
    // succeeds but fresh placement verification fails.
    rig.state
        .stable
        .insert(rig.keys[3].hwnd, rig.source_rows[3].rect);
    inject_pending_arrival_placement(rig.keys[3].hwnd, rig.source_rows[3].rect);
    rig.state
        .stable
        .insert(rig.keys[4].hwnd, rig.source_rows[4].rect);
    let hidden_moved = crate::workspace_owner::translate_rect_to_output(
        rig.source_rows[4].rect,
        source_full,
        target_full,
    );
    inject_pending_arrival_placement(rig.keys[4].hwnd, hidden_moved);
    inject_pending_placement_normal(
        rig.keys[1].hwnd,
        Rect {
            x: 100,
            y: 60,
            w: 320,
            h: 240,
        },
        true,
    );
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed_visible.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![
        (false, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (true, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (false, Some(false)),
        (false, Some(false)),
    ]);
    inject_pending_focus(Some("focus-ok"));
    let outcome = run_all_migrate(&mut rig, &correlation, 54);
    assert_eq!(outcome, "migrate-unverified");
    assert!(
        pending_counters().reconcile >= 1,
        "placement mismatch reconciles without replay"
    );
    clear_pending_seams();
}

#[test]
fn migrate_float_size_mismatch_refuses() {
    // Fresh float proof preserves size: a post-setter frame with the
    // wrong size refuses even on the target output.
    let mut rig = rig_all_kinds();
    let correlation = CorrelationId::parse("corr-migrate-size-mismatch").expect("correlation");
    let writes = predict_mixed_writes(&rig, &correlation);
    let tiled_write = writes
        .iter()
        .find(|w| w.window.0 == "mig-a")
        .expect("tiled");
    rig.state
        .stable
        .insert(rig.keys[0].hwnd, rig.source_rows[0].rect);
    inject_pending_arrival(rig.keys[0].hwnd, tiled_write.rect);
    let source_full = rig.areas[0].full;
    let target_full = rig.areas[1].full;
    let overlay_moved = crate::workspace_owner::translate_rect_to_output(
        rig.source_rows[1].rect,
        source_full,
        target_full,
    );
    rig.state
        .stable
        .insert(rig.keys[1].hwnd, rig.source_rows[1].rect);
    inject_pending_arrival(rig.keys[1].hwnd, overlay_moved);
    // Float arrives on target but resized: size proof fails.
    let mut float_wrong = crate::workspace_owner::translate_rect_to_output(
        rig.source_rows[2].rect,
        source_full,
        target_full,
    );
    float_wrong.w += 50;
    rig.state
        .stable
        .insert(rig.keys[2].hwnd, rig.source_rows[2].rect);
    inject_pending_arrival(rig.keys[2].hwnd, float_wrong);
    for idx in [3usize, 4] {
        let moved = crate::workspace_owner::translate_rect_to_output(
            rig.source_rows[idx].rect,
            source_full,
            target_full,
        );
        rig.state
            .stable
            .insert(rig.keys[idx].hwnd, rig.source_rows[idx].rect);
        inject_pending_arrival_placement(rig.keys[idx].hwnd, moved);
    }
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed_visible.clone(), Vec::new(), false);
    inject_pending_placement_normal(
        rig.keys[1].hwnd,
        Rect {
            x: 100,
            y: 60,
            w: 320,
            h: 240,
        },
        true,
    );
    inject_pending_overlay_seq(vec![
        (false, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (true, Some(false)),
        (true, Some(false)),
        (false, Some(false)),
        (false, Some(false)),
        (false, Some(false)),
    ]);
    inject_pending_focus(Some("focus-ok"));
    let outcome = run_all_migrate(&mut rig, &correlation, 55);
    assert_eq!(outcome, "migrate-unverified");
    clear_pending_seams();
}

#[test]
fn migrate_placement_work_conversion_handles_unequal_insets() {
    // Pure WORK/FULL conversion (no native windows): a non-tool
    // top-level restore rect rides workspace coordinates defined as
    // screen minus the per-monitor taskbar inset delta
    // (`WORK.left - FULL.left`, `WORK.top - FULL.top`), retaining the
    // virtual-desktop monitor origin. Subtracting the entire `WORK`
    // origin would normalize a right-monitor rect to x=100 and land it
    // on the primary monitor.
    use super::{
        placement_raw_to_screen, screen_to_placement_raw, translate_placement_normal_to_target,
    };
    let source_work = Rect {
        x: 0,
        y: 40,
        w: 1920,
        h: 1040,
    };
    let target_work = Rect {
        x: 1920,
        y: 0,
        w: 1920,
        h: 1080,
    };
    let source_full = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    let target_full = Rect {
        x: 1920,
        y: 0,
        w: 1920,
        h: 1080,
    };
    let source_raw = Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    let target_raw = translate_placement_normal_to_target(
        source_raw,
        source_work,
        target_work,
        source_full,
        target_full,
        true,
    );
    assert_eq!(
        target_raw,
        Rect {
            x: 2020,
            y: 140,
            w: 400,
            h: 300,
        },
        "right-monitor raw retains the virtual-desktop origin (2020), not normalized to 100"
    );
    let target_screen = placement_raw_to_screen(target_raw, target_work, target_full, true);
    assert_eq!(
        target_screen,
        Rect {
            x: 2020,
            y: 140,
            w: 400,
            h: 300,
        },
        "zero-inset target converts raw back to the translated screen frame"
    );
    // Roundtrip on the source: raw -> screen -> raw is identity.
    let source_screen = placement_raw_to_screen(source_raw, source_work, source_full, true);
    assert_eq!(
        source_screen,
        Rect {
            x: 100,
            y: 140,
            w: 400,
            h: 300,
        }
    );
    assert_eq!(
        screen_to_placement_raw(source_screen, source_work, source_full, true),
        source_raw
    );
    let tool_raw = translate_placement_normal_to_target(
        source_raw,
        source_work,
        target_work,
        source_full,
        target_full,
        false,
    );
    assert_eq!(
        tool_raw,
        Rect {
            x: 2020,
            y: 100,
            w: 400,
            h: 300,
        },
        "tool windows ride screen coordinates untouched by WORK"
    );
    // Left taskbar on the primary: work x=60 shifts the workspace x by
    // the inset delta (60 - 0).
    let primary_full = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    let left_work = Rect {
        x: 60,
        y: 0,
        w: 1860,
        h: 1080,
    };
    let screen = Rect {
        x: 160,
        y: 200,
        w: 400,
        h: 300,
    };
    assert_eq!(
        screen_to_placement_raw(screen, left_work, primary_full, true),
        Rect {
            x: 100,
            y: 200,
            w: 400,
            h: 300,
        },
        "left inset subtracts WORK - FULL"
    );
    assert_eq!(
        screen_to_placement_raw(screen, left_work, primary_full, false),
        screen,
        "tool windows ignore the work origin"
    );
    // Right monitor with its own left taskbar (WORK.x = FULL.x + 60):
    // the raw rect must keep the 1920 origin. The old entire-WORK
    // subtraction would normalize to x=100 (primary).
    let inset_target_work = Rect {
        x: 1980,
        y: 0,
        w: 1860,
        h: 1080,
    };
    let inset_screen = Rect {
        x: 2080,
        y: 200,
        w: 400,
        h: 300,
    };
    assert_eq!(
        screen_to_placement_raw(inset_screen, inset_target_work, target_full, true),
        Rect {
            x: 2020,
            y: 200,
            w: 400,
            h: 300,
        },
        "non-primary inset retains the monitor origin"
    );
    assert_eq!(
        placement_raw_to_screen(
            Rect {
                x: 2020,
                y: 200,
                w: 400,
                h: 300,
            },
            inset_target_work,
            target_full,
            true,
        ),
        inset_screen,
    );
    // Stacked monitor below with a top taskbar (FULL.y=1080, WORK.y=1120):
    // translation must land at raw y=1180, not normalized y=100.
    let stacked_full = Rect {
        x: 0,
        y: 1080,
        w: 1920,
        h: 1080,
    };
    let stacked_work = Rect {
        x: 0,
        y: 1120,
        w: 1920,
        h: 1040,
    };
    let stacked_raw = translate_placement_normal_to_target(
        source_raw,
        source_work,
        stacked_work,
        source_full,
        stacked_full,
        true,
    );
    assert_eq!(
        stacked_raw,
        Rect {
            x: 100,
            y: 1180,
            w: 400,
            h: 300,
        },
        "stacked inset retains the vertical monitor origin"
    );
    // Negative left monitor with a top taskbar (FULL.x=-1920):
    // translation must keep x=-1820, not normalize to x=100.
    let negative_full = Rect {
        x: -1920,
        y: 0,
        w: 1920,
        h: 1080,
    };
    let negative_work = Rect {
        x: -1920,
        y: 40,
        w: 1920,
        h: 1040,
    };
    let negative_raw = translate_placement_normal_to_target(
        source_raw,
        source_work,
        negative_work,
        source_full,
        negative_full,
        true,
    );
    assert_eq!(
        negative_raw,
        Rect {
            x: -1820,
            y: 100,
            w: 400,
            h: 300,
        },
        "negative monitor origin is retained"
    );
    clear_pending_seams();
}

#[test]
fn migrate_bordered_visible_arrival_distinguishes_outer() {
    // Bordered windows verify on the visible (DWM) frame, never the
    // outer `GetWindowRect`: the observed outer frame carries a 7px
    // border each side while the visible frame rides the plan rect, and
    // the staged arrival is the visible translation. Success with
    // outer != visible locks the visible-basis contract.
    let mut rig = rig();
    let correlation = CorrelationId::parse("corr-migrate-bordered").expect("correlation");
    let writes = predict_migrate_rects(&rig, &correlation);
    assert!(!writes.is_empty());
    for (idx, window) in rig.observed.iter_mut().enumerate() {
        let visible = window.visible;
        window.outer = Rect {
            x: visible.x - 7,
            y: visible.y - 7,
            w: visible.w + 14,
            h: visible.h + 14,
        };
        assert_ne!(
            window.outer, window.visible,
            "fixture must carry a border (member {idx})"
        );
    }
    for write in &writes {
        let key = rig
            .keys
            .iter()
            .find(|key| {
                rig.state
                    .member_tokens
                    .get(*key)
                    .is_some_and(|t| t == &write.window.0)
            })
            .expect("planned member");
        let pre = rig
            .observed
            .iter()
            .find(|w| w.hwnd == key.hwnd)
            .map(|w| w.visible)
            .expect("visible pre-state");
        rig.state.stable.insert(key.hwnd, pre);
        inject_pending_arrival(key.hwnd, write.rect);
    }
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_overlay_seq(vec![(false, Some(false)), (false, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let outcome = run_migrate(&mut rig, &correlation, 57);
    assert_eq!(outcome, "migrate-applied");
    clear_pending_seams();
}

#[test]
fn migrate_follow_tail_suppresses_on_live_fullscreen() {
    // The follow tail re-reads fullscreen live immediately before focus:
    // a stale plain row (fullscreen=false) with a live fullscreen frame
    // suppresses instead of focusing.
    let mut rig = rig();
    let member = rig.keys[0].clone();
    let token = rig.tokens[0].clone();
    let rows = vec![crate::workspace_owner::MigrateRow {
        token: token.clone(),
        rect: rig.source_rows[0].rect,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        fit_excluded: false,
    }];
    let members: BTreeSet<WindowKey> = BTreeSet::from([member.clone()]);
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_overlay_seq(vec![(false, Some(true))]);
    inject_pending_focus(Some("focus-ok"));
    let log = |_outcome: &'static str, _extra: serde_json::Value| {};
    let outcome = super::migration_follow_tail(
        &mut rig.state,
        &rig.me,
        &rig.observed,
        &[],
        &members,
        &rows,
        Some(WindowId(token)),
        &rig.fulls,
        &log,
    );
    assert_eq!(outcome, "migrate-applied");
    assert_eq!(
        pending_counters().focus,
        0,
        "live fullscreen suppresses with no focus setter"
    );
    clear_pending_seams();
}

#[test]
fn migrate_follow_tail_focuses_when_live_plain() {
    // Live plain (no fullscreen, not iconic) runs one focus actuation
    // through the shared tail.
    let mut rig = rig();
    let member = rig.keys[0].clone();
    let token = rig.tokens[0].clone();
    let rows = vec![crate::workspace_owner::MigrateRow {
        token: token.clone(),
        rect: rig.source_rows[0].rect,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        fit_excluded: false,
    }];
    let members: BTreeSet<WindowKey> = BTreeSet::from([member]);
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_overlay_seq(vec![(false, Some(false))]);
    inject_pending_focus(Some("focus-ok"));
    let log = |_outcome: &'static str, _extra: serde_json::Value| {};
    let outcome = super::migration_follow_tail(
        &mut rig.state,
        &rig.me,
        &rig.observed,
        &[],
        &members,
        &rows,
        Some(WindowId(token)),
        &rig.fulls,
        &log,
    );
    assert_eq!(outcome, "migrate-applied");
    assert_eq!(
        pending_counters().focus,
        1,
        "live plain runs exactly one focus setter"
    );
    clear_pending_seams();
}
