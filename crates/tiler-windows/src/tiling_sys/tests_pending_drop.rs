#![forbid(unsafe_code)]
use super::{
    GestureIntent, MonitorArea, ObservedWindow, RenderBridge, RenderCarrier, RetainedRow,
    clear_pending_seams, hide_render_carrier, inject_monitor_areas, inject_pending_elevated,
    inject_pending_focus, inject_pending_observe, inject_pending_stop, inject_pending_suspend,
    inject_pending_tag, live_member_tag, pending_counters, queue_pending_drop,
    service_pending_drop,
};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn pending_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 5001,
        process_creation: "creation-pending-me".to_owned(),
        user_sid: "S-1-pending".to_owned(),
        session_id: 7,
        exe_path: "C:\\test\\app.exe".to_owned(),
    }
}

fn pending_state() -> super::TileLoop {
    let mut state = super::rmax03_adapter_tests::test_state();
    let channels = Arc::new(crate::render_handoff::RenderChannels::new());
    state.render_bridge = Some(RenderBridge {
        channels,
        handle: None,
        gens: [0, 0, 0],
        hide_posted: [false, false, false],
        hide_reason: [None, None, None],
        hidden_ack: [None, None, None],
        posted: std::array::from_fn(|_| VecDeque::new()),
        applied_visible: [false, false, false],
        applied_note: [None, None, None],
        applied_ack: [None, None, None],
        applied_known: [false, false, false],
        admission_open: true,
        pending: None,
    });
    state
}

fn pending_domain() -> (
    String,
    String,
    Vec<MonitorArea>,
    Vec<tiler_core::geometry::Rect>,
) {
    let output = "mon-pending".to_owned();
    let workspace = "ws-pending".to_owned();
    let work = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let full = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    let areas = vec![MonitorArea {
        device: output.clone(),
        work,
        full,
    }];
    let fulls = vec![full];
    let _ = workspace;
    (output, workspace, areas, fulls)
}

fn member_key(hwnd: u64, pid: u32, creation: &str) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
    }
}

fn member_identity(
    pid: u32,
    creation: &str,
    me: &crate::model::ProcessIdentity,
) -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid,
        process_creation: creation.to_owned(),
        user_sid: me.user_sid.clone(),
        session_id: me.session_id,
        exe_path: me.exe_path.clone(),
    }
}

fn observed_window(
    hwnd: u64,
    token: &str,
    pid: u32,
    creation: &str,
    me: &crate::model::ProcessIdentity,
    rect: tiler_core::geometry::Rect,
) -> ObservedWindow {
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd,
            pid,
            process_creation: creation.to_owned(),
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

fn valid_fixture(
    producer: &'static str,
) -> (
    super::TileLoop,
    crate::model::ProcessIdentity,
    Vec<MonitorArea>,
    Vec<tiler_core::geometry::Rect>,
    crate::workspace::WindowKey,
    String,
    u64,
) {
    clear_pending_seams();
    let me = pending_me();
    let mut state = pending_state();
    let (output, _workspace_name, areas, fulls) = pending_domain();
    state.workspaces.ensure_output(&output);
    let active = state
        .workspaces
        .active_id(&output)
        .expect("active workspace");
    state.workspaces.set_tiled(&output, &active, true);
    let hwnd: u64 = 0x2101;
    let pid: u32 = 6001;
    let creation = "creation-2101";
    let token = "w-pending-1".to_owned();
    let key = member_key(hwnd, pid, creation);
    state.member_tokens.insert(key.clone(), token.clone());
    state
        .member_identity
        .insert(key.clone(), member_identity(pid, creation, &me));
    state.member_tags.insert(key.clone(), "tag-2101".to_owned());
    assert!(
        state
            .workspaces
            .assign(key.clone(), &output, &active, false)
    );
    state.member_rects.insert(
        token.clone(),
        tiler_core::geometry::Rect {
            x: 10,
            y: 10,
            w: 400,
            h: 300,
        },
    );
    let rect = tiler_core::geometry::Rect {
        x: 10,
        y: 10,
        w: 400,
        h: 300,
    };
    let observed = vec![observed_window(hwnd, &token, pid, creation, &me, rect)];
    let retained: Vec<RetainedRow> = Vec::new();
    inject_pending_observe(observed.clone(), retained.clone(), false);
    inject_pending_tag(hwnd, Some("tag-2101".to_owned()));
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(None);
    inject_pending_stop(false);
    // Queue via the production END path (hide + bounded slot).
    let member_key = key.clone();
    let start_key = Some(key.clone());
    let start_tag = Some(Some("tag-2101".to_owned()));
    let intent = GestureIntent::MoveDrop { x: 960, y: 500 };
    let occupied = queue_pending_drop(
        &mut state, &hwnd, producer, intent, &token, start_key, start_tag, member_key, &output,
        &active, None, None,
    );
    assert!(!occupied, "valid fixture queues into an empty slot");
    // Prove the hide: the ack pair covers the queued hide pair.
    let (hide_gen, hide_safety) = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .map(|pending| (pending.hide_gen, pending.hide_safety))
        .expect("pending queued");
    if let Some(bridge) = state.render_bridge.as_mut() {
        bridge.hidden_ack[RenderCarrier::Preview.index()] = Some((hide_gen, hide_safety));
    }
    (state, me, areas, fulls, key, token, hwnd)
}

#[test]
fn proven_hidden_executes_with_fresh_validation() {
    let (mut state, me, areas, fulls, _key, token, hwnd) = valid_fixture("native");
    let before = pending_counters();
    assert_eq!(before.engine, 0);
    assert_eq!(before.focus, 0);
    assert_eq!(before.apply, 0);
    assert_eq!(before.reconcile, 0);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    let after = pending_counters();
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "proven pending is consumed"
    );
    assert_eq!(after.engine, 1, "Engine runs only after proven hide");
    assert_eq!(after.focus, 0, "native producer skips focus");
    assert!(
        after.apply + after.reconcile >= 1,
        "drop converges via apply or reconcile, got {after:?}"
    );
    let _ = (token, hwnd);
    clear_pending_seams();
}

#[test]
fn unproven_ack_keeps_pending_with_no_actuation() {
    let (mut state, me, areas, fulls, _key, _token, hwnd) = valid_fixture("native");
    // Make the ack unproven: clear the proven pair.
    if let Some(bridge) = state.render_bridge.as_mut() {
        bridge.hidden_ack[RenderCarrier::Preview.index()] = None;
        // Keep the deadline in the future so this is a wait, not a timeout.
        if let Some(pending) = bridge.pending.as_mut() {
            pending.deadline = Instant::now() + Duration::from_secs(60);
        }
    }
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_some(),
        "unproven pending waits"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "no Engine before hidden");
    assert_eq!(counters.focus, 0, "no focus before hidden");
    assert_eq!(counters.apply, 0, "no geometry before hidden");
    assert_eq!(counters.reconcile, 0, "no converge before hidden");
    let _ = hwnd;
    clear_pending_seams();
}

#[test]
fn stale_ack_pair_keeps_pending_with_no_actuation() {
    let (mut state, me, areas, fulls, _key, _token, _hwnd) = valid_fixture("native");
    // Unknown/stale ack: a pair older than the queued hide proves nothing.
    if let Some(bridge) = state.render_bridge.as_mut() {
        let (hide_gen, hide_safety) = bridge
            .pending
            .as_ref()
            .map(|pending| (pending.hide_gen, pending.hide_safety))
            .expect("pending");
        bridge.hidden_ack[RenderCarrier::Preview.index()] =
            Some((hide_gen.saturating_sub(1), hide_safety.saturating_sub(1)));
        if let Some(pending) = bridge.pending.as_mut() {
            pending.deadline = Instant::now() + Duration::from_secs(60);
        }
    }
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_some(),
        "stale ack keeps pending"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0);
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(counters.reconcile, 0);
    clear_pending_seams();
}

#[test]
fn timeout_refuses_with_no_actuation_or_reconcile() {
    let (mut state, me, areas, fulls, _key, _token, hwnd) = valid_fixture("native");
    if let Some(bridge) = state.render_bridge.as_mut() {
        bridge.hidden_ack[RenderCarrier::Preview.index()] = None;
        if let Some(pending) = bridge.pending.as_mut() {
            pending.deadline = Instant::now() - Duration::from_secs(1);
        }
    }
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "timeout drops the slot"
    );
    assert!(
        !state.gesture_start_key.contains_key(&hwnd),
        "timeout releases START snapshots"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "timeout never runs Engine");
    assert_eq!(counters.focus, 0, "timeout never focuses");
    assert_eq!(counters.apply, 0, "timeout never writes geometry");
    assert_eq!(counters.reconcile, 0, "timeout never reconciles");
    clear_pending_seams();
}

#[test]
fn start_identity_mismatch_refuses() {
    let (mut state, me, areas, fulls, key, _token, hwnd) = valid_fixture("native");
    // Same-process HWND reuse: live tag differs from the START tag.
    inject_pending_tag(hwnd, Some("tag-reused".to_owned()));
    // Keep the START snapshot naming the old tag.
    assert_eq!(
        state.gesture_start_key.get(&hwnd),
        None,
        "service owns START snapshots via pending, not gesture maps"
    );
    let _ = key;
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "identity mismatch consumes the slot"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "identity mismatch never reaches Engine");
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(counters.reconcile, 1, "identity mismatch converges once");
    clear_pending_seams();
}

#[test]
fn member_token_remap_refuses() {
    let (mut state, me, areas, fulls, key, _token, _hwnd) = valid_fixture("native");
    // Token remap between END and the proven hide: the queued pending
    // names a stale member while the live lookup resolves the current
    // member. Fail closed instead of dropping under a recycled identity.
    if let Some(bridge) = state.render_bridge.as_mut()
        && let Some(pending) = bridge.pending.as_mut()
    {
        pending.member_key = member_key(0x9999, 9999, "creation-stale");
    }
    let _ = key;
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "remap consumes the slot"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "remap never reaches Engine");
    assert_eq!(counters.reconcile, 1, "remap converges once");
    clear_pending_seams();
}

#[test]
fn occupied_newcomer_is_refused_narrowly() {
    clear_pending_seams();
    let me = pending_me();
    let mut state = pending_state();
    let (output, _workspace_name, _areas, _fulls) = pending_domain();
    state.workspaces.ensure_output(&output);
    let active = state
        .workspaces
        .active_id(&output)
        .expect("active workspace");
    state.workspaces.set_tiled(&output, &active, true);
    let first_key = member_key(0x2201, 6101, "creation-2201");
    let second_key = member_key(0x2202, 6102, "creation-2202");
    let intent = GestureIntent::MoveDrop { x: 960, y: 500 };
    let first_occupied = queue_pending_drop(
        &mut state,
        &0x2201,
        "native",
        intent,
        "w-first",
        Some(first_key.clone()),
        Some(Some("tag-first".to_owned())),
        first_key,
        &output,
        &active,
        None,
        None,
    );
    assert!(!first_occupied, "first finish owns the slot");
    let second_occupied = queue_pending_drop(
        &mut state,
        &0x2202,
        "native",
        GestureIntent::MoveDrop { x: 960, y: 500 },
        "w-second",
        Some(second_key.clone()),
        Some(Some("tag-second".to_owned())),
        second_key,
        &output,
        &active,
        None,
        None,
    );
    assert!(second_occupied, "overlapping END is refused busy");
    let pending = state
        .render_bridge
        .as_ref()
        .and_then(|b| b.pending.as_ref())
        .expect("first pending owns the settle");
    assert_eq!(pending.hwnd, 0x2201);
    assert_eq!(pending.mover_token, "w-first");
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "queueing never runs Engine");
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(counters.reconcile, 0);
    let _ = me;
    clear_pending_seams();
}

/// Two-output cross-drop fixture: the mover plus a sibling tile the
/// primary source view, one tile lives on the negative-x destination
/// view, and both Engine sessions are seeded through real reconcile
/// events. Returns state plus addressing plus the destination drop
/// point on the destination's left edge.
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_lines)]
fn cross_pending_fixture() -> (
    super::TileLoop,
    crate::model::ProcessIdentity,
    Vec<MonitorArea>,
    Vec<tiler_core::geometry::Rect>,
    crate::workspace::WindowKey,
    String,
    u64,
    String,
    String,
    String,
    String,
    i32,
    i32,
) {
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    use tiler_core::ids::CorrelationId;
    clear_pending_seams();
    let me = pending_me();
    let mut state = pending_state();
    let areas = vec![
        MonitorArea {
            device: "mon-a".to_owned(),
            work: tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 1400,
            },
            full: tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 1440,
            },
        },
        MonitorArea {
            device: "mon-b".to_owned(),
            work: tiler_core::geometry::Rect {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1040,
            },
            full: tiler_core::geometry::Rect {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        },
    ];
    let fulls: Vec<tiler_core::geometry::Rect> = areas.iter().map(|a| a.full).collect();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    state.workspaces.set_tiled("mon-a", &ws_a, true);
    state.workspaces.set_tiled("mon-b", &ws_b, true);
    let mover = member_key(0x3101, 6101, "creation-3101");
    let sib = member_key(0x3102, 6102, "creation-3102");
    let host = member_key(0x3103, 6103, "creation-3103");
    for (key, token, pid, creation) in [
        (&mover, "w-mover", 6101, "creation-3101"),
        (&sib, "w-sib", 6102, "creation-3102"),
        (&host, "w-dest", 6103, "creation-3103"),
    ] {
        state.member_tokens.insert(key.clone(), token.to_owned());
        state
            .member_identity
            .insert(key.clone(), member_identity(pid, creation, &me));
    }
    state
        .member_tags
        .insert(mover.clone(), "tag-3101".to_owned());
    state.member_tags.insert(sib.clone(), "tag-3102".to_owned());
    state
        .member_tags
        .insert(host.clone(), "tag-3103".to_owned());
    assert!(
        state
            .workspaces
            .assign(mover.clone(), "mon-a", &ws_a, false)
    );
    assert!(state.workspaces.assign(sib.clone(), "mon-a", &ws_a, false));
    assert!(state.workspaces.assign(host.clone(), "mon-b", &ws_b, false));
    let mover_rect = tiler_core::geometry::Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    let sib_rect = tiler_core::geometry::Rect {
        x: 600,
        y: 100,
        w: 400,
        h: 300,
    };
    let dest_rect = tiler_core::geometry::Rect {
        x: -1500,
        y: 100,
        w: 400,
        h: 300,
    };
    for (token, rect) in [
        ("w-mover", mover_rect),
        ("w-sib", sib_rect),
        ("w-dest", dest_rect),
    ] {
        state.member_rects.insert(token.to_owned(), rect);
    }
    let observed = vec![
        observed_window(0x3101, "w-mover", 6101, "creation-3101", &me, mover_rect),
        observed_window(0x3102, "w-sib", 6102, "creation-3102", &me, sib_rect),
        observed_window(0x3103, "w-dest", 6103, "creation-3103", &me, dest_rect),
    ];
    // Seed both Engine sessions through real reconcile events (before
    // injection, so no counters move).
    for (output, workspace, rows, focused) in [
        (
            "mon-a",
            ws_a.clone(),
            vec![("w-mover", mover_rect), ("w-sib", sib_rect)],
            "w-mover",
        ),
        ("mon-b", ws_b.clone(), vec![("w-dest", dest_rect)], "w-dest"),
    ] {
        let (domain, domain_key) = super::workspace_domain_for(
            output,
            &workspace,
            &areas,
            state.inner_gap,
            state.outer_gap,
        )
        .expect("domain");
        let carried: Vec<(
            WindowId,
            tiler_core::geometry::Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = rows
            .iter()
            .map(|(token, rect)| {
                (
                    WindowId((*token).to_owned()),
                    *rect,
                    tiler_core::size_hints::WindowSizeHints::none(),
                    false,
                )
            })
            .collect();
        let correlation = CorrelationId::parse("cross-pending-seed").expect("correlation");
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(t, r)| ((*t).to_owned(), *r))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            0,
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &carried,
            Some(&WindowId(focused.to_owned())),
        );
        match state.engine.handle(&event) {
            CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
            other => panic!("seed must converge, got {other:?}"),
        }
    }
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_tag(0x3101, Some("tag-3101".to_owned()));
    inject_pending_tag(0x3102, Some("tag-3102".to_owned()));
    inject_pending_tag(0x3103, Some("tag-3103".to_owned()));
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("focus-ok"));
    inject_pending_stop(false);
    // Operation-tail topology reads the injected fixture list (matching
    // the tick areas here): the fresh-snapshot path is exercised without
    // native enumeration, and drift tests overwrite this injection.
    inject_monitor_areas(Some(areas.clone()));
    // Destination drop point on the destination domain's left edge.
    let (dest_domain, _) =
        super::workspace_domain_for("mon-b", &ws_b, &areas, state.inner_gap, state.outer_gap)
            .expect("dest domain");
    let x = dest_domain.bounds.x + 2;
    let y = dest_domain.bounds.y + dest_domain.bounds.h / 2;
    (
        state,
        me,
        areas,
        fulls,
        mover,
        "w-mover".to_owned(),
        0x3101,
        "mon-a".to_owned(),
        ws_a,
        "mon-b".to_owned(),
        ws_b,
        x,
        y,
    )
}

fn cross_pending_fence(
    state: &super::TileLoop,
    areas: &[MonitorArea],
    ws_a: &str,
    ws_b: &str,
) -> crate::workspace_owner::DragDropFenceSnapshot {
    let (source_domain, _) =
        super::workspace_domain_for("mon-a", ws_a, areas, state.inner_gap, state.outer_gap)
            .expect("source domain");
    let (target_domain, _) =
        super::workspace_domain_for("mon-b", ws_b, areas, state.inner_gap, state.outer_gap)
            .expect("dest domain");
    let source_full = areas
        .iter()
        .find(|a| a.device == "mon-a")
        .expect("source area")
        .full;
    let target_full = areas
        .iter()
        .find(|a| a.device == "mon-b")
        .expect("dest area")
        .full;
    crate::workspace_owner::DragDropFenceSnapshot {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_output: "mon-a".to_owned(),
        source_workspace: ws_a.to_owned(),
        target_output: "mon-b".to_owned(),
        target_workspace: ws_b.to_owned(),
        source_tiled: true,
        target_tiled: true,
        overlay_maximized: false,
        overlay_fullscreen: false,
        source_full,
        target_full,
        source_bounds: source_domain.bounds,
        target_bounds: target_domain.bounds,
        owner: state.owner.as_str().to_owned(),
        generation: state.generation.as_str().to_owned(),
    }
}

#[allow(clippy::too_many_arguments)]
fn queue_cross_pending(
    state: &mut super::TileLoop,
    mover: &crate::workspace::WindowKey,
    token: &str,
    hwnd: u64,
    output: &str,
    workspace: &str,
    dest_output: &str,
    dest_workspace: &str,
    fence: crate::workspace_owner::DragDropFenceSnapshot,
    x: i32,
    y: i32,
    source_rect: Option<tiler_core::geometry::Rect>,
) {
    let dest = super::PendingCrossDest {
        output: dest_output.to_owned(),
        workspace: dest_workspace.to_owned(),
        fence,
    };
    // START snapshot tag travels with the gesture exactly like the
    // production queue site (read from the member table, never invented).
    let start_tag = state.member_tags.get(mover).cloned();
    let occupied = queue_pending_drop(
        state,
        &hwnd,
        "native",
        GestureIntent::MoveDrop { x, y },
        token,
        Some(mover.clone()),
        Some(start_tag),
        mover.clone(),
        output,
        workspace,
        Some(dest),
        source_rect,
    );
    assert!(!occupied, "cross fixture queues into an empty slot");
    let (hide_gen, hide_safety) = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .map(|pending| (pending.hide_gen, pending.hide_safety))
        .expect("pending queued");
    if let Some(bridge) = state.render_bridge.as_mut() {
        bridge.hidden_ack[RenderCarrier::Preview.index()] = Some((hide_gen, hide_safety));
    }
}

#[test]
fn cross_pending_proven_hide_settles_transfer_with_arrival() {
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state,
        &mover,
        &token,
        hwnd,
        &src_out,
        &ws_a,
        &dst_out,
        &ws_b,
        fence,
        x,
        y,
        // PRE-gesture allocation: the source capture row stays bounded
        // here even though the observed frame below travels.
        Some(tiler_core::geometry::Rect {
            x: 100,
            y: 100,
            w: 400,
            h: 300,
        }),
    );
    // Learn the planned arrival through the production pair preview
    // (read-only), then rig the observed mover frame to it so the
    // arrival proof exercises the real comparison instead of failing on
    // the frozen pre-drop frame.
    let observed: Vec<ObservedWindow> = vec![
        observed_window(
            0x3101,
            "w-mover",
            6101,
            "creation-3101",
            &me,
            tiler_core::geometry::Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3102,
            "w-sib",
            6102,
            "creation-3102",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3103,
            "w-dest",
            6103,
            "creation-3103",
            &me,
            tiler_core::geometry::Rect {
                x: -1500,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    let settle = super::CrossDragSettle {
        mover_token: token.clone(),
        member_key: mover.clone(),
        source_output: src_out.clone(),
        source_workspace: ws_a.clone(),
        dest_output: dst_out.clone(),
        dest_workspace: ws_b.clone(),
        x,
        y,
        drop_prior: None,
        producer: "native".to_owned(),
        start_rect: Some(tiler_core::geometry::Rect {
            x: 100,
            y: 100,
            w: 400,
            h: 300,
        }),
    };
    let correlation = state.correlation();
    let preview = super::prepare_cross_drag_preview(
        &mut state,
        &areas,
        &observed,
        &[],
        &settle,
        &correlation,
    )
    .expect("preview prepares");
    let mut preview = preview;
    let proposed = match super::handle_engine_event(&mut state, &mut preview) {
        tiler_core::boundary::CoreReply::DragPreview(plan) => plan.preview.proposed_rect,
        other => panic!("cross preview must project, got {other:?}"),
    };
    // The arrival lands inside the destination work area, not the panel.
    let dest_work = areas
        .iter()
        .find(|a| a.device == dst_out)
        .expect("dest area")
        .work;
    assert!(proposed.x >= dest_work.x);
    assert!(proposed.y >= dest_work.y);
    assert!(proposed.x + proposed.w <= dest_work.x + dest_work.w);
    assert!(proposed.y + proposed.h <= dest_work.y + dest_work.h);
    let rigged = vec![
        observed_window(0x3101, "w-mover", 6101, "creation-3101", &me, proposed),
        observed_window(
            0x3102,
            "w-sib",
            6102,
            "creation-3102",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3103,
            "w-dest",
            6103,
            "creation-3103",
            &me,
            tiler_core::geometry::Rect {
                x: -1500,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    inject_pending_observe(rigged, Vec::new(), false);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "proven cross pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 2, "preview plus drop run Engine");
    assert_eq!(counters.focus, 1, "arrival gates the single follow focus");
    assert_eq!(counters.apply, 1, "one combined union apply");
    assert_eq!(
        counters.reconcile, 0,
        "success converges through apply, not reconcile"
    );
    // Membership transferred Engine-side and project-side.
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some(dst_out.as_str())
    );
    let dest_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(dst_out.clone()),
        workspace: tiler_core::directional::WorkspaceId(ws_b.clone()),
    };
    assert!(
        state
            .engine
            .session(&dest_key)
            .expect("dest kept")
            .snapshot()
            .windows
            .iter()
            .any(|l| l.window.0 == "w-mover")
    );
    let src_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(src_out.clone()),
        workspace: tiler_core::directional::WorkspaceId(ws_a.clone()),
    };
    assert!(
        state
            .engine
            .session(&src_key)
            .expect("source kept")
            .snapshot()
            .windows
            .iter()
            .all(|l| l.window.0 != "w-mover")
    );
    // No view switch and no hiding: both outputs keep their current
    // workspaces and the destination tile stays visible.
    assert_eq!(
        state.workspaces.active_id(&src_out).as_deref(),
        Some(ws_a.as_str())
    );
    assert_eq!(
        state.workspaces.active_id(&dst_out).as_deref(),
        Some(ws_b.as_str())
    );
    let host = member_key(0x3103, 6103, "creation-3103");
    assert!(!state.workspaces.is_hidden(&host));
    clear_pending_seams();
}

#[test]
fn cross_pending_native_moved_frame_commits_with_bounded_source_row() {
    // Native title-bar drags move the frame inside the OS modal loop:
    // at END the mover's live frame sits on the destination output
    // (same size, no DPI rescale here) while project membership still
    // homes it on the source. The production prepare route must carry
    // the retained START allocation on the source capture row (bounded
    // in the source) while destination rows stay current. Commit-last:
    // the pair plans on a candidate, and an unproven arrival keeps
    // source membership and the retained Engine with no follow-focus
    // (homing binds, not rects, but arrival still gates the commit).
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let start_rect = tiler_core::geometry::Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    let moved_rect = tiler_core::geometry::Rect {
        x: -1500,
        y: 100,
        w: 400,
        h: 300,
    };
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state,
        &mover,
        &token,
        hwnd,
        &src_out,
        &ws_a,
        &dst_out,
        &ws_b,
        fence,
        x,
        y,
        Some(start_rect),
    );
    // Re-observe with the traveled native frame (same size: a move,
    // not a DPI rescale).
    let moved = vec![
        observed_window(0x3101, "w-mover", 6101, "creation-3101", &me, moved_rect),
        observed_window(
            0x3102,
            "w-sib",
            6102,
            "creation-3102",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3103,
            "w-dest",
            6103,
            "creation-3103",
            &me,
            tiler_core::geometry::Rect {
                x: -1500,
                y: 500,
                w: 400,
                h: 300,
            },
        ),
    ];
    inject_pending_observe(moved.clone(), Vec::new(), false);
    // Production prepare: the source capture row carries the START
    // allocation (bounded in the source domain bounds) while the
    // destination current row keeps its live frame.
    let settle = super::CrossDragSettle {
        mover_token: token.clone(),
        member_key: mover.clone(),
        source_output: src_out.clone(),
        source_workspace: ws_a.clone(),
        dest_output: dst_out.clone(),
        dest_workspace: ws_b.clone(),
        x,
        y,
        drop_prior: None,
        producer: "native".to_owned(),
        start_rect: Some(start_rect),
    };
    let correlation = state.correlation();
    let event = super::prepare_cross_drag_event(
        &mut state,
        &areas,
        &moved,
        &[],
        &settle,
        &correlation,
        correlation.as_str(),
        "drag-drop",
    )
    .expect("cross event prepares");
    let source_row = event
        .windows
        .iter()
        .find(|w| w.window.0 == "w-mover" && w.output.0 == src_out && w.workspace.0 == ws_a)
        .expect("source-homed mover row");
    assert_eq!(source_row.rect, start_rect);
    let (source_domain, _) =
        super::workspace_domain_for(&src_out, &ws_a, &areas, state.inner_gap, state.outer_gap)
            .expect("source domain");
    assert!(
        source_row.rect.x >= source_domain.bounds.x
            && source_row.rect.y >= source_domain.bounds.y
            && source_row.rect.x + source_row.rect.w
                <= source_domain.bounds.x + source_domain.bounds.w
            && source_row.rect.y + source_row.rect.h
                <= source_domain.bounds.y + source_domain.bounds.h,
        "source capture row stays bounded"
    );
    let dest_row = event
        .windows
        .iter()
        .find(|w| w.window.0 == "w-dest")
        .expect("destination current row");
    assert_eq!(
        dest_row.rect,
        tiler_core::geometry::Rect {
            x: -1500,
            y: 500,
            w: 400,
            h: 300,
        }
    );
    // Full pending route with the traveled frame: the pair plans on a
    // candidate (no convergence refusal) but the unproven arrival
    // withholds the commit, so the tail reports unverified with no
    // focus actuation, source membership retained, and the retained
    // Engine published nowhere — exactly like a real missed readback.
    let engine_before = format!("{:?}", state.engine);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "proven cross pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 1, "drop plans Engine once on a candidate");
    assert_eq!(counters.focus, 0, "unproven arrival never focuses");
    assert_eq!(counters.apply, 1, "combined apply still runs");
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some(src_out.as_str()),
        "unproven arrival keeps the mover home"
    );
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "unproven arrival keeps the retained Engine"
    );
    let dest_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(dst_out.clone()),
        workspace: tiler_core::directional::WorkspaceId(ws_b.clone()),
    };
    assert!(
        state.engine.session(&dest_key).is_none_or(|session| session
            .snapshot()
            .windows
            .iter()
            .all(|l| l.window.0 != "w-mover")),
        "unproven arrival never publishes the mover remotely"
    );
    let src_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(src_out.clone()),
        workspace: tiler_core::directional::WorkspaceId(ws_a.clone()),
    };
    assert!(
        state
            .engine
            .session(&src_key)
            .expect("source kept")
            .snapshot()
            .windows
            .iter()
            .any(|l| l.window.0 == "w-mover"),
        "unproven arrival keeps the mover in the retained source session"
    );
    clear_pending_seams();
}

#[test]
fn cross_pending_empty_native_flow_admits_then_reflows() {
    // Empty destination with a natively moved frame: commit-last plans
    // the singleton admission on a candidate, applies the destination,
    // reflows the source without the mover on that same candidate, and
    // publishes both authorities once after the arrival proof verifies,
    // before follow-focus.
    let state = {
        clear_pending_seams();
        let me = pending_me();
        let mut state = pending_state();
        let areas = vec![
            MonitorArea {
                device: "mon-a".to_owned(),
                work: tiler_core::geometry::Rect {
                    x: 0,
                    y: 0,
                    w: 2560,
                    h: 1400,
                },
                full: tiler_core::geometry::Rect {
                    x: 0,
                    y: 0,
                    w: 2560,
                    h: 1440,
                },
            },
            MonitorArea {
                device: "mon-b".to_owned(),
                work: tiler_core::geometry::Rect {
                    x: -1920,
                    y: 0,
                    w: 1920,
                    h: 1040,
                },
                full: tiler_core::geometry::Rect {
                    x: -1920,
                    y: 0,
                    w: 1920,
                    h: 1080,
                },
            },
        ];
        let fulls: Vec<tiler_core::geometry::Rect> = areas.iter().map(|a| a.full).collect();
        state.workspaces.ensure_output("mon-a");
        state.workspaces.ensure_output("mon-b");
        let ws_a = state.workspaces.active_id("mon-a").expect("active");
        let ws_b = state.workspaces.active_id("mon-b").expect("active");
        state.workspaces.set_tiled("mon-a", &ws_a, true);
        state.workspaces.set_tiled("mon-b", &ws_b, true);
        (state, me, areas, fulls, ws_a, ws_b)
    };
    let (mut state, me, areas, fulls, ws_a, ws_b) = state;
    let mover = member_key(0x3201, 6201, "creation-3201");
    let sib = member_key(0x3202, 6202, "creation-3202");
    for (key, token, pid, creation) in [
        (&mover, "w-mover", 6201, "creation-3201"),
        (&sib, "w-sib", 6202, "creation-3202"),
    ] {
        state.member_tokens.insert(key.clone(), token.to_owned());
        state
            .member_identity
            .insert(key.clone(), member_identity(pid, creation, &me));
    }
    state
        .member_tags
        .insert(mover.clone(), "tag-3201".to_owned());
    state.member_tags.insert(sib.clone(), "tag-3202".to_owned());
    assert!(
        state
            .workspaces
            .assign(mover.clone(), "mon-a", &ws_a, false)
    );
    assert!(state.workspaces.assign(sib.clone(), "mon-a", &ws_a, false));
    let start_rect = tiler_core::geometry::Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    let moved_rect = tiler_core::geometry::Rect {
        x: -1500,
        y: 100,
        w: 400,
        h: 300,
    };
    // Seed the source Engine session only; the destination stays empty.
    {
        use tiler_core::boundary::CoreReply;
        use tiler_core::directional::WindowId;
        use tiler_core::ids::CorrelationId;
        let (domain, domain_key) =
            super::workspace_domain_for("mon-a", &ws_a, &areas, state.inner_gap, state.outer_gap)
                .expect("domain");
        let carried = vec![
            (
                WindowId("w-mover".to_owned()),
                start_rect,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("w-sib".to_owned()),
                tiler_core::geometry::Rect {
                    x: 600,
                    y: 100,
                    w: 400,
                    h: 300,
                },
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
        ];
        let correlation = CorrelationId::parse("cross-empty-seed").expect("correlation");
        let fp = crate::tiling::fingerprint(
            &carried
                .iter()
                .map(|(w, r, _, _)| (w.0.clone(), *r))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            0,
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &carried,
            Some(&WindowId("w-mover".to_owned())),
        );
        match state.engine.handle(&event) {
            CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
            other => panic!("seed must converge, got {other:?}"),
        }
    }
    let observed = vec![
        observed_window(0x3201, "w-mover", 6201, "creation-3201", &me, moved_rect),
        observed_window(
            0x3202,
            "w-sib",
            6202,
            "creation-3202",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_tag(0x3201, Some("tag-3201".to_owned()));
    inject_pending_tag(0x3202, Some("tag-3202".to_owned()));
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("focus-ok"));
    inject_pending_stop(false);
    inject_monitor_areas(Some(areas.clone()));
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    // Destination drop point on the empty destination's left edge.
    let (dest_domain, _) =
        super::workspace_domain_for("mon-b", &ws_b, &areas, state.inner_gap, state.outer_gap)
            .expect("dest domain");
    let (x, y) = (
        dest_domain.bounds.x + 2,
        dest_domain.bounds.y + dest_domain.bounds.h / 2,
    );
    queue_cross_pending(
        &mut state,
        &mover,
        "w-mover",
        0x3201,
        "mon-a",
        &ws_a,
        "mon-b",
        &ws_b,
        fence,
        x,
        y,
        Some(start_rect),
    );
    // Learn the planned arrival through the production singleton preview
    // (read-only on a clone), then rig the observed mover frame to it so
    // the arrival proof exercises the real comparison.
    let settle = super::CrossDragSettle {
        mover_token: "w-mover".to_owned(),
        member_key: mover.clone(),
        source_output: "mon-a".to_owned(),
        source_workspace: ws_a.clone(),
        dest_output: "mon-b".to_owned(),
        dest_workspace: ws_b.clone(),
        x,
        y,
        drop_prior: None,
        producer: "native".to_owned(),
        start_rect: Some(start_rect),
    };
    let correlation = state.correlation();
    let mut hint_cx = super::HintCx::new();
    let mut preview = super::prepare_cross_singleton_event(
        &mut state,
        &areas,
        &observed,
        &settle,
        &correlation,
        &mut hint_cx,
        true,
    )
    .expect("singleton preview prepares");
    let proposed = {
        let mut candidate = state.engine.clone();
        super::stamp_engine_event(&state, &mut preview);
        match candidate.handle(&preview) {
            tiler_core::boundary::CoreReply::DragPreview(plan) => plan.preview.proposed_rect,
            other => panic!("singleton preview must project, got {other:?}"),
        }
    };
    let rigged = vec![
        observed_window(0x3201, "w-mover", 6201, "creation-3201", &me, proposed),
        observed_window(
            0x3202,
            "w-sib",
            6202,
            "creation-3202",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    inject_pending_observe(rigged, Vec::new(), false);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "proven empty pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(
        counters.engine, 2,
        "admission plus source reflow run Engine"
    );
    assert_eq!(
        counters.apply, 2,
        "destination apply plus source reflow apply"
    );
    assert_eq!(counters.focus, 1, "verified arrival follows focus once");
    assert_eq!(counters.reconcile, 0, "success converges through apply");
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some("mon-b")
    );
    let dest_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId("mon-b".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(ws_b.clone()),
    };
    let dest_windows: Vec<String> = state
        .engine
        .session(&dest_key)
        .expect("dest kept")
        .snapshot()
        .windows
        .iter()
        .map(|l| l.window.0.clone())
        .collect();
    assert_eq!(dest_windows, vec!["w-mover".to_owned()]);
    let src_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId("mon-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(ws_a.clone()),
    };
    let src_windows: Vec<String> = state
        .engine
        .session(&src_key)
        .expect("source kept")
        .snapshot()
        .windows
        .iter()
        .map(|l| l.window.0.clone())
        .collect();
    assert_eq!(src_windows, vec!["w-sib".to_owned()]);
    assert_eq!(
        state.workspaces.active_id("mon-b").as_deref(),
        Some(ws_b.as_str()),
        "no destination view switch"
    );
    clear_pending_seams();
}

/// Commit-last occupied refusal: stage the verified arrival, then drift
/// the fresh topology under the tail. The candidate plan runs but the
/// commit withholds every write, keeping source membership and the
/// retained Engine with no follow-focus.
#[test]
fn cross_pending_occupied_late_topology_retains_source_and_engine() {
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    let start_rect = tiler_core::geometry::Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    queue_cross_pending(
        &mut state,
        &mover,
        &token,
        hwnd,
        &src_out,
        &ws_a,
        &dst_out,
        &ws_b,
        fence,
        x,
        y,
        Some(start_rect),
    );
    // Stage the verified arrival exactly like the success path so only
    // the late topology refuses.
    let settle = super::CrossDragSettle {
        mover_token: token.clone(),
        member_key: mover.clone(),
        source_output: src_out.clone(),
        source_workspace: ws_a.clone(),
        dest_output: dst_out.clone(),
        dest_workspace: ws_b.clone(),
        x,
        y,
        drop_prior: None,
        producer: "native".to_owned(),
        start_rect: Some(start_rect),
    };
    let correlation = state.correlation();
    let observed_probe: Vec<ObservedWindow> = vec![
        observed_window(0x3101, "w-mover", 6101, "creation-3101", &me, start_rect),
        observed_window(
            0x3102,
            "w-sib",
            6102,
            "creation-3102",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3103,
            "w-dest",
            6103,
            "creation-3103",
            &me,
            tiler_core::geometry::Rect {
                x: -1500,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    let preview = super::prepare_cross_drag_preview(
        &mut state,
        &areas,
        &observed_probe,
        &[],
        &settle,
        &correlation,
    )
    .expect("preview prepares");
    let mut preview = preview;
    let proposed = match super::handle_engine_event(&mut state, &mut preview) {
        tiler_core::boundary::CoreReply::DragPreview(plan) => plan.preview.proposed_rect,
        other => panic!("cross preview must project, got {other:?}"),
    };
    let rigged = vec![
        observed_window(0x3101, "w-mover", 6101, "creation-3101", &me, proposed),
        observed_window(
            0x3102,
            "w-sib",
            6102,
            "creation-3102",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3103,
            "w-dest",
            6103,
            "creation-3103",
            &me,
            tiler_core::geometry::Rect {
                x: -1500,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    inject_pending_observe(rigged, Vec::new(), false);
    // Fresh topology drifted after pair-build: the destination FULL
    // moved, so the tail withholds every write. The tick areas stay
    // original so the pre-settle pointer check still passes.
    let mut drifted = areas.clone();
    if let Some(area) = drifted.iter_mut().find(|a| a.device == dst_out) {
        area.full.x += 100;
    }
    inject_monitor_areas(Some(drifted));
    let engine_before = format!("{:?}", state.engine);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "late-drift pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.focus, 0, "refused transfer skips follow-focus");
    assert_eq!(counters.apply, 0, "late drift withholds every write");
    assert!(
        counters.reconcile >= 1,
        "late drift reconciles with the fresh topology"
    );
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some(src_out.as_str()),
        "mover stays home"
    );
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "late drift keeps the retained Engine"
    );
    clear_pending_seams();
}

/// Commit-last occupied refusal on a failed readback: the scripted apply
/// seam fails the single combined write, so the candidate discards with
/// source membership and the retained Engine and no follow-focus.
#[test]
fn cross_pending_occupied_failed_readback_retains_source_and_engine() {
    use super::{inject_pending_apply, pending_counters};
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    let start_rect = tiler_core::geometry::Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    queue_cross_pending(
        &mut state,
        &mover,
        &token,
        hwnd,
        &src_out,
        &ws_a,
        &dst_out,
        &ws_b,
        fence,
        x,
        y,
        Some(start_rect),
    );
    let settle = super::CrossDragSettle {
        mover_token: token.clone(),
        member_key: mover.clone(),
        source_output: src_out.clone(),
        source_workspace: ws_a.clone(),
        dest_output: dst_out.clone(),
        dest_workspace: ws_b.clone(),
        x,
        y,
        drop_prior: None,
        producer: "native".to_owned(),
        start_rect: Some(start_rect),
    };
    let correlation = state.correlation();
    let observed_probe: Vec<ObservedWindow> = vec![
        observed_window(0x3101, "w-mover", 6101, "creation-3101", &me, start_rect),
        observed_window(
            0x3102,
            "w-sib",
            6102,
            "creation-3102",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3103,
            "w-dest",
            6103,
            "creation-3103",
            &me,
            tiler_core::geometry::Rect {
                x: -1500,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    let preview = super::prepare_cross_drag_preview(
        &mut state,
        &areas,
        &observed_probe,
        &[],
        &settle,
        &correlation,
    )
    .expect("preview prepares");
    let mut preview = preview;
    let proposed = match super::handle_engine_event(&mut state, &mut preview) {
        tiler_core::boundary::CoreReply::DragPreview(plan) => plan.preview.proposed_rect,
        other => panic!("cross preview must project, got {other:?}"),
    };
    let rigged = vec![
        observed_window(0x3101, "w-mover", 6101, "creation-3101", &me, proposed),
        observed_window(
            0x3102,
            "w-sib",
            6102,
            "creation-3102",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3103,
            "w-dest",
            6103,
            "creation-3103",
            &me,
            tiler_core::geometry::Rect {
                x: -1500,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    inject_pending_observe(rigged, Vec::new(), false);
    inject_monitor_areas(Some(areas.clone()));
    inject_pending_apply(Some((false, 0)));
    let engine_before = format!("{:?}", state.engine);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "refused pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.apply, 1, "the combined apply ran once");
    assert_eq!(counters.focus, 0, "refused transfer skips follow-focus");
    assert_eq!(counters.reconcile, 0, "no replay on a refused write");
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some(src_out.as_str()),
        "mover stays home"
    );
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "failed readback keeps the retained Engine"
    );
    clear_pending_seams();
}

/// Commit-last occupied refusal on a write mismatch: same retention as a
/// failed readback, reported as a mismatch with no follow-focus.
#[test]
fn cross_pending_occupied_mismatch_retains_source_and_engine() {
    use super::{inject_pending_apply, pending_counters};
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    let start_rect = tiler_core::geometry::Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    queue_cross_pending(
        &mut state,
        &mover,
        &token,
        hwnd,
        &src_out,
        &ws_a,
        &dst_out,
        &ws_b,
        fence,
        x,
        y,
        Some(start_rect),
    );
    let settle = super::CrossDragSettle {
        mover_token: token.clone(),
        member_key: mover.clone(),
        source_output: src_out.clone(),
        source_workspace: ws_a.clone(),
        dest_output: dst_out.clone(),
        dest_workspace: ws_b.clone(),
        x,
        y,
        drop_prior: None,
        producer: "native".to_owned(),
        start_rect: Some(start_rect),
    };
    let correlation = state.correlation();
    let observed_probe: Vec<ObservedWindow> = vec![
        observed_window(0x3101, "w-mover", 6101, "creation-3101", &me, start_rect),
        observed_window(
            0x3102,
            "w-sib",
            6102,
            "creation-3102",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3103,
            "w-dest",
            6103,
            "creation-3103",
            &me,
            tiler_core::geometry::Rect {
                x: -1500,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    let preview = super::prepare_cross_drag_preview(
        &mut state,
        &areas,
        &observed_probe,
        &[],
        &settle,
        &correlation,
    )
    .expect("preview prepares");
    let mut preview = preview;
    let proposed = match super::handle_engine_event(&mut state, &mut preview) {
        tiler_core::boundary::CoreReply::DragPreview(plan) => plan.preview.proposed_rect,
        other => panic!("cross preview must project, got {other:?}"),
    };
    let rigged = vec![
        observed_window(0x3101, "w-mover", 6101, "creation-3101", &me, proposed),
        observed_window(
            0x3102,
            "w-sib",
            6102,
            "creation-3102",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
        observed_window(
            0x3103,
            "w-dest",
            6103,
            "creation-3103",
            &me,
            tiler_core::geometry::Rect {
                x: -1500,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    inject_pending_observe(rigged, Vec::new(), false);
    inject_monitor_areas(Some(areas.clone()));
    inject_pending_apply(Some((true, 1)));
    let engine_before = format!("{:?}", state.engine);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "mismatched pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.apply, 1, "the combined apply ran once");
    assert_eq!(counters.focus, 0, "mismatch skips follow-focus");
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some(src_out.as_str()),
        "mover stays home"
    );
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "mismatch keeps the retained Engine"
    );
    clear_pending_seams();
}

/// Empty-destination commit-last fixture: mover plus sibling tile the
/// `mon-a` source view, `mon-b` stays empty project-side and Engine-side
/// with only the source session seeded, and the tail seams script the
/// fixture topology. Returns state plus addressing, the destination drop
/// point, and the START capture rect.
#[allow(clippy::type_complexity)]
fn empty_cross_fixture() -> (
    super::TileLoop,
    crate::model::ProcessIdentity,
    Vec<MonitorArea>,
    Vec<tiler_core::geometry::Rect>,
    crate::workspace::WindowKey,
    crate::workspace::WindowKey,
    String,
    String,
    i32,
    i32,
    tiler_core::geometry::Rect,
) {
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    use tiler_core::ids::CorrelationId;
    clear_pending_seams();
    let me = pending_me();
    let mut state = pending_state();
    let areas = vec![
        MonitorArea {
            device: "mon-a".to_owned(),
            work: tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 1400,
            },
            full: tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 1440,
            },
        },
        MonitorArea {
            device: "mon-b".to_owned(),
            work: tiler_core::geometry::Rect {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1040,
            },
            full: tiler_core::geometry::Rect {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        },
    ];
    let fulls: Vec<tiler_core::geometry::Rect> = areas.iter().map(|a| a.full).collect();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    state.workspaces.set_tiled("mon-a", &ws_a, true);
    state.workspaces.set_tiled("mon-b", &ws_b, true);
    let mover = member_key(0x3301, 6301, "creation-3301");
    let sib = member_key(0x3302, 6302, "creation-3302");
    for (key, token, pid, creation) in [
        (&mover, "w-mover", 6301, "creation-3301"),
        (&sib, "w-sib", 6302, "creation-3302"),
    ] {
        state.member_tokens.insert(key.clone(), token.to_owned());
        state
            .member_identity
            .insert(key.clone(), member_identity(pid, creation, &me));
    }
    state
        .member_tags
        .insert(mover.clone(), "tag-3301".to_owned());
    state.member_tags.insert(sib.clone(), "tag-3302".to_owned());
    assert!(
        state
            .workspaces
            .assign(mover.clone(), "mon-a", &ws_a, false)
    );
    assert!(state.workspaces.assign(sib.clone(), "mon-a", &ws_a, false));
    let start_rect = tiler_core::geometry::Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    let sib_rect = tiler_core::geometry::Rect {
        x: 600,
        y: 100,
        w: 400,
        h: 300,
    };
    {
        let (domain, domain_key) =
            super::workspace_domain_for("mon-a", &ws_a, &areas, state.inner_gap, state.outer_gap)
                .expect("domain");
        let carried = vec![
            (
                WindowId("w-mover".to_owned()),
                start_rect,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("w-sib".to_owned()),
                sib_rect,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
        ];
        let correlation = CorrelationId::parse("empty-commitlast-seed").expect("correlation");
        let fp = crate::tiling::fingerprint(
            &carried
                .iter()
                .map(|(w, r, _, _)| (w.0.clone(), *r))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            0,
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &carried,
            Some(&WindowId("w-mover".to_owned())),
        );
        match state.engine.handle(&event) {
            CoreReply::Projection(_) | CoreReply::Tiled(_) => {}
            other => panic!("seed must converge, got {other:?}"),
        }
    }
    inject_pending_tag(0x3301, Some("tag-3301".to_owned()));
    inject_pending_tag(0x3302, Some("tag-3302".to_owned()));
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("focus-ok"));
    inject_pending_stop(false);
    inject_monitor_areas(Some(areas.clone()));
    let (dest_domain, _) =
        super::workspace_domain_for("mon-b", &ws_b, &areas, state.inner_gap, state.outer_gap)
            .expect("dest domain");
    let x = dest_domain.bounds.x + 2;
    let y = dest_domain.bounds.y + dest_domain.bounds.h / 2;
    (
        state, me, areas, fulls, mover, sib, ws_a, ws_b, x, y, start_rect,
    )
}

/// Predict the singleton arrival on a clone (no counters, no mutation)
/// and rig the observed mover frame to it so the proof exercises the
/// real comparison.
#[allow(clippy::too_many_arguments)]
fn rig_empty_arrival(
    state: &mut super::TileLoop,
    me: &crate::model::ProcessIdentity,
    areas: &[MonitorArea],
    mover: &crate::workspace::WindowKey,
    ws_a: &str,
    ws_b: &str,
    x: i32,
    y: i32,
    start_rect: tiler_core::geometry::Rect,
) {
    let moved_probe = vec![
        observed_window(0x3301, "w-mover", 6301, "creation-3301", me, start_rect),
        observed_window(
            0x3302,
            "w-sib",
            6302,
            "creation-3302",
            me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    let settle = super::CrossDragSettle {
        mover_token: "w-mover".to_owned(),
        member_key: mover.clone(),
        source_output: "mon-a".to_owned(),
        source_workspace: ws_a.to_owned(),
        dest_output: "mon-b".to_owned(),
        dest_workspace: ws_b.to_owned(),
        x,
        y,
        drop_prior: None,
        producer: "native".to_owned(),
        start_rect: Some(start_rect),
    };
    let correlation = state.correlation();
    let mut hint_cx = super::HintCx::new();
    let mut preview = super::prepare_cross_singleton_event(
        state,
        areas,
        &moved_probe,
        &settle,
        &correlation,
        &mut hint_cx,
        true,
    )
    .expect("singleton preview prepares");
    let proposed = {
        let mut candidate = state.engine.clone();
        super::stamp_engine_event(state, &mut preview);
        match candidate.handle(&preview) {
            tiler_core::boundary::CoreReply::DragPreview(plan) => plan.preview.proposed_rect,
            other => panic!("singleton preview must project, got {other:?}"),
        }
    };
    let rigged = vec![
        observed_window(0x3301, "w-mover", 6301, "creation-3301", me, proposed),
        observed_window(
            0x3302,
            "w-sib",
            6302,
            "creation-3302",
            me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    inject_pending_observe(rigged, Vec::new(), false);
}

/// Commit-last empty refusal on a failed destination write: the first
/// scripted verdict fails the destination apply, so the source reflow
/// never runs and the candidate discards with source membership and the
/// retained Engine and no follow-focus.
#[test]
fn cross_pending_empty_dest_readback_retains_source_and_engine() {
    use super::{inject_pending_apply_seq, pending_counters};
    let (mut state, me, areas, fulls, mover, _sib, ws_a, ws_b, x, y, start_rect) =
        empty_cross_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state,
        &mover,
        "w-mover",
        0x3301,
        "mon-a",
        &ws_a,
        "mon-b",
        &ws_b,
        fence,
        x,
        y,
        Some(start_rect),
    );
    rig_empty_arrival(
        &mut state, &me, &areas, &mover, &ws_a, &ws_b, x, y, start_rect,
    );
    inject_pending_apply_seq(vec![(false, 0)]);
    let engine_before = format!("{:?}", state.engine);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "refused pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 1, "only the admission plans");
    assert_eq!(counters.apply, 1, "only the destination apply runs");
    assert_eq!(counters.focus, 0, "refused transfer skips follow-focus");
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some("mon-a"),
        "mover stays home"
    );
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "failed destination write keeps the retained Engine"
    );
    clear_pending_seams();
}

/// Commit-last empty refusal on a failed source reflow: the destination
/// write succeeds but the second scripted verdict fails the reflow, so
/// both authorities stay home with no follow-focus (no fake rollback of
/// the observed destination write; the next observation converges).
#[test]
fn cross_pending_empty_source_reflow_fail_retains_source_and_engine() {
    use super::{inject_pending_apply_seq, pending_counters};
    let (mut state, me, areas, fulls, mover, _sib, ws_a, ws_b, x, y, start_rect) =
        empty_cross_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state,
        &mover,
        "w-mover",
        0x3301,
        "mon-a",
        &ws_a,
        "mon-b",
        &ws_b,
        fence,
        x,
        y,
        Some(start_rect),
    );
    rig_empty_arrival(
        &mut state, &me, &areas, &mover, &ws_a, &ws_b, x, y, start_rect,
    );
    inject_pending_apply_seq(vec![(true, 0), (false, 0)]);
    let engine_before = format!("{:?}", state.engine);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "refused pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 2, "admission plus reflow plan");
    assert_eq!(counters.apply, 2, "destination plus reflow applies run");
    assert_eq!(counters.focus, 0, "refused transfer skips follow-focus");
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some("mon-a"),
        "mover stays home"
    );
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "failed source reflow keeps the retained Engine"
    );
    clear_pending_seams();
}

/// Commit-last empty refusal on an unproven arrival: both applies succeed
/// but the mover never read back on the destination, so both authorities
/// stay home with no follow-focus.
#[test]
fn cross_pending_empty_arrival_fail_retains_source_and_engine() {
    use super::pending_counters;
    let (mut state, me, areas, fulls, mover, _sib, ws_a, ws_b, x, y, start_rect) =
        empty_cross_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state,
        &mover,
        "w-mover",
        0x3301,
        "mon-a",
        &ws_a,
        "mon-b",
        &ws_b,
        fence,
        x,
        y,
        Some(start_rect),
    );
    // No arrival staging: the observed mover stays at the START shape on
    // the source output, so the proof cannot pass.
    let observed = vec![
        observed_window(0x3301, "w-mover", 6301, "creation-3301", &me, start_rect),
        observed_window(
            0x3302,
            "w-sib",
            6302,
            "creation-3302",
            &me,
            tiler_core::geometry::Rect {
                x: 600,
                y: 100,
                w: 400,
                h: 300,
            },
        ),
    ];
    inject_pending_observe(observed, Vec::new(), false);
    let engine_before = format!("{:?}", state.engine);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "unproven pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 2, "admission plus reflow plan");
    assert_eq!(counters.apply, 2, "destination plus reflow applies run");
    assert_eq!(counters.focus, 0, "unproven arrival never focuses");
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some("mon-a"),
        "mover stays home"
    );
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "unproven arrival keeps the retained Engine"
    );
    clear_pending_seams();
}

/// Commit-last empty refusal on late topology drift: the fresh
/// enumeration moved under the tail, so every write withholds with
/// source membership and the retained Engine kept and no follow-focus.
#[test]
fn cross_pending_empty_late_topology_retains_source_and_engine() {
    use super::pending_counters;
    let (mut state, me, areas, fulls, mover, _sib, ws_a, ws_b, x, y, start_rect) =
        empty_cross_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state,
        &mover,
        "w-mover",
        0x3301,
        "mon-a",
        &ws_a,
        "mon-b",
        &ws_b,
        fence,
        x,
        y,
        Some(start_rect),
    );
    rig_empty_arrival(
        &mut state, &me, &areas, &mover, &ws_a, &ws_b, x, y, start_rect,
    );
    let mut drifted = areas.clone();
    if let Some(area) = drifted.iter_mut().find(|a| a.device == "mon-b") {
        area.full.x += 100;
    }
    inject_monitor_areas(Some(drifted));
    let engine_before = format!("{:?}", state.engine);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "late-drift pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.apply, 0, "late drift withholds every write");
    assert_eq!(counters.focus, 0, "refused transfer skips follow-focus");
    assert!(
        counters.reconcile >= 1,
        "late drift reconciles with the fresh topology"
    );
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some("mon-a"),
        "mover stays home"
    );
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "late drift keeps the retained Engine"
    );
    clear_pending_seams();
}

#[test]
fn cross_pending_stale_topology_refuses_without_actuation() {
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state, &mover, &token, hwnd, &src_out, &ws_a, &dst_out, &ws_b, fence, x, y, None,
    );
    // Topology drifted under the pending finish: the destination FULL
    // rectangle moved, so the pointer output no longer resolves there.
    let mut drifted = areas.clone();
    if let Some(area) = drifted.iter_mut().find(|a| a.device == dst_out) {
        area.full.x += 100;
    }
    service_pending_drop(&mut state, &me, &fulls, &drifted);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "stale pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "stale topology never reaches Engine");
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(counters.reconcile, 1, "stale topology converges once");
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| loc.output.clone())
            .as_deref(),
        Some(src_out.as_str()),
        "mover stays home"
    );
    clear_pending_seams();
}

#[test]
fn cross_pending_stale_view_refuses_without_actuation() {
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state, &mover, &token, hwnd, &src_out, &ws_a, &dst_out, &ws_b, fence, x, y, None,
    );
    // The destination switched its current workspace after END: the
    // queued view is stale, so the drop refuses instead of landing in
    // the wrong layout.
    let (trailing, _) = state
        .workspaces
        .select_trailing(&dst_out)
        .expect("trailing");
    assert_ne!(trailing, ws_b);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "stale pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "stale view never reaches Engine");
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(counters.reconcile, 1, "stale view converges once");
    clear_pending_seams();
}

#[test]
fn cross_pending_stale_lifetime_refuses_without_actuation() {
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state, &mover, &token, hwnd, &src_out, &ws_a, &dst_out, &ws_b, fence, x, y, None,
    );
    // Same-process HWND reuse between END and the proven hide.
    inject_pending_tag(hwnd, Some("tag-reused".to_owned()));
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "stale pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "stale lifetime never reaches Engine");
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(counters.reconcile, 1, "stale lifetime converges once");
    clear_pending_seams();
}

#[test]
fn cross_pending_suspend_refuses_without_actuation() {
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state, &mover, &token, hwnd, &src_out, &ws_a, &dst_out, &ws_b, fence, x, y, None,
    );
    inject_pending_suspend(Some(true));
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "suspended pending is consumed"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "suspend never reaches Engine");
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    // The ack-wait invalidation owns suspend and drops the slot without
    // converging; the settle-time fence stays the second guard for
    // arrivals racing the ack.
    assert_eq!(counters.reconcile, 0, "suspend drops without converging");
    clear_pending_seams();
}

#[test]
fn cross_pending_timeout_refuses_without_actuation() {
    let (mut state, me, areas, fulls, mover, token, hwnd, src_out, ws_a, dst_out, ws_b, x, y) =
        cross_pending_fixture();
    let fence = cross_pending_fence(&state, &areas, &ws_a, &ws_b);
    queue_cross_pending(
        &mut state, &mover, &token, hwnd, &src_out, &ws_a, &dst_out, &ws_b, fence, x, y, None,
    );
    // Withhold the renderer's verified hidden acknowledgement past the
    // bound: the finish refuses with no Engine drop and no writes.
    if let Some(bridge) = state.render_bridge.as_mut() {
        bridge.hidden_ack[RenderCarrier::Preview.index()] = None;
        if let Some(pending) = bridge.pending.as_mut() {
            pending.deadline = Instant::now() - Duration::from_secs(1);
        }
    }
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "timeout drops the slot"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "timeout never runs Engine");
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(counters.reconcile, 0, "timeout never reconciles");
    clear_pending_seams();
}

#[test]
fn esc_invalidates_pending_with_converge() {
    let (mut state, me, areas, fulls, _key, _token, hwnd) = valid_fixture("native");
    state.esc_latched.insert(hwnd);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "Esc drops the slot"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "Esc never reaches Engine");
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(
        counters.reconcile, 1,
        "Esc converges once like the inline path"
    );
    clear_pending_seams();
}

#[test]
fn suspend_invalidates_pending_without_actuation() {
    let (mut state, me, areas, fulls, _key, _token, _hwnd) = valid_fixture("native");
    inject_pending_suspend(Some(true));
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "suspend drops the slot"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "suspend never reaches Engine");
    assert_eq!(counters.focus, 0, "suspend never focuses");
    assert_eq!(counters.apply, 0, "suspend never writes geometry");
    assert_eq!(counters.reconcile, 0, "suspend drops without converging");
    clear_pending_seams();
}

#[test]
fn new_gesture_supersedes_pending() {
    let (mut state, me, areas, fulls, _key, _token, hwnd) = valid_fixture("native");
    state.active.insert(hwnd);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "new gesture supersedes the stale slot"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0);
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(
        counters.reconcile, 0,
        "supersede leaves settle to the new gesture"
    );
    clear_pending_seams();
}

#[test]
fn stop_while_pending_drops_without_actuation() {
    let (mut state, me, areas, fulls, _key, _token, _hwnd) = valid_fixture("native");
    inject_pending_stop(true);
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "stop drops the slot"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "stop never runs Engine");
    assert_eq!(counters.focus, 0);
    assert_eq!(counters.apply, 0);
    assert_eq!(counters.reconcile, 0);
    clear_pending_seams();
}

#[test]
fn domain_drift_after_ack_refuses() {
    let (mut state, me, areas, fulls, key, _token, _hwnd) = valid_fixture("native");
    // The member moved workspaces between END and the proven hide.
    let other = "ws-other".to_owned();
    state.workspaces.ensure_output("mon-pending");
    // Create a second workspace by toggling? Use a direct assign to a
    // different workspace id via a new output to force domain mismatch.
    let _ = other;
    if let Some(bridge) = state.render_bridge.as_mut()
        && let Some(pending) = bridge.pending.as_mut()
    {
        pending.workspace = "ws-stale".to_owned();
    }
    let _ = key;
    service_pending_drop(&mut state, &me, &fulls, &areas);
    assert!(
        state
            .render_bridge
            .as_ref()
            .and_then(|b| b.pending.as_ref())
            .is_none(),
        "domain drift consumes the slot"
    );
    let counters = pending_counters();
    assert_eq!(counters.engine, 0, "domain drift never reaches Engine");
    assert_eq!(counters.reconcile, 1, "domain drift converges once");
    clear_pending_seams();
}

#[test]
fn live_member_tag_prefers_injected_without_native() {
    clear_pending_seams();
    inject_pending_tag(0x9999, Some("tag-injected".to_owned()));
    assert_eq!(live_member_tag(0x9999), Some("tag-injected".to_owned()));
    assert_eq!(
        live_member_tag(0x8888),
        None,
        "unscripted stays None native-free"
    );
    clear_pending_seams();
}

#[test]
fn hide_reason_transition_posts_once_without_forging_visibility() {
    // Consecutive hidden reason changes were silently suppressed by the
    // early return; the ack then logged the stale reason. A changed
    // reason posts one new hide generation/epoch, a repeated reason
    // stays idempotent, visibility is never forged, and the queued
    // finish keeps its original pair (no unrelated clear).
    let (mut state, _me, _areas, _fulls, _key, _token, _hwnd) = valid_fixture("native");
    let bridge = state.render_bridge.as_ref().expect("bridge");
    let preview = RenderCarrier::Preview.index();
    let safety_before = bridge.channels.safety();
    let gen_before = bridge.gens[preview];
    let (pending_gen, pending_safety) = bridge
        .pending
        .as_ref()
        .map(|pending| (pending.hide_gen, pending.hide_safety))
        .expect("pending queued");
    // Same reason as the queued finish stays idempotent: no new epoch.
    hide_render_carrier(&mut state, RenderCarrier::Preview, "finish");
    let bridge = state.render_bridge.as_ref().expect("bridge");
    assert_eq!(bridge.channels.safety(), safety_before);
    assert_eq!(bridge.gens[preview], gen_before);
    // Changed reason posts exactly one new hide generation/epoch.
    hide_render_carrier(&mut state, RenderCarrier::Preview, "cancel");
    let bridge = state.render_bridge.as_ref().expect("bridge");
    assert_eq!(bridge.channels.safety(), safety_before + 1);
    assert_eq!(bridge.gens[preview], gen_before + 1);
    assert_eq!(
        bridge.hide_reason[preview].as_deref(),
        Some("cancel"),
        "ack logs the true transition reason"
    );
    // Repeating the new reason stays idempotent: no per-tick feedback.
    hide_render_carrier(&mut state, RenderCarrier::Preview, "cancel");
    let bridge = state.render_bridge.as_ref().expect("bridge");
    assert_eq!(bridge.channels.safety(), safety_before + 1);
    assert_eq!(bridge.gens[preview], gen_before + 1);
    // No visibility is forged and the unrelated pending finish keeps its
    // original pair: only the renderer ack proves hidden.
    assert!(!bridge.applied_visible[preview]);
    assert!(!bridge.applied_known[preview]);
    let pending = bridge.pending.as_ref().expect("pending retained");
    assert_eq!(pending.hide_gen, pending_gen);
    assert_eq!(pending.hide_safety, pending_safety);
    clear_pending_seams();
}
