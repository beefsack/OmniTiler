#![forbid(unsafe_code)]
//! Select-route gap-repair regression through the real `workspace_do_select`:
//! reveal adoption must keep its repair victims instead of re-tiling them
//! from stale pre-adopt rows. Scripted observation plus injected hints; the
//! apply stub records writables with no native effects, and native
//! hide/focus/marker setters fail closed on the fake HWNDs.

use super::rmax03_adapter_tests::test_state;
use super::{
    ActionCtx, LedgerStore, MonitorArea, clear_pending_seams, clear_test_hints,
    inject_pending_elevated, inject_pending_focus, inject_pending_observe, inject_pending_suspend,
    inject_pending_tag, inject_test_hints, pending_apply_writables, pending_counters,
};
use std::collections::{BTreeMap, BTreeSet};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;
use tiler_core::size_hints::WindowSizeHints;

const N: usize = 40;
const EXPECTED_VICTIMS: usize = 30;
const WORK: Rect = Rect {
    x: 0,
    y: 0,
    w: 2560,
    h: 1380,
};
const BIRTH: Rect = Rect {
    x: 200,
    y: 200,
    w: 640,
    h: 480,
};

fn hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(118),
        min_h: Some(30),
        max_w: None,
        max_h: None,
    }
}

fn member_key(base: u64, i: usize) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd: base + i as u64,
        pid: base as u32 + i as u32,
        creation: format!("c{:016x}", base + i as u64),
    }
}

fn token(i: usize) -> String {
    format!("r{i:02}")
}

fn me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn observed_window(
    key: &crate::workspace::WindowKey,
    token: &str,
    frame: Rect,
) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer: frame,
        visible: frame,
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: "test.exe".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
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

fn stage_member(
    state: &mut super::TileLoop,
    output: &str,
    workspace: &str,
    key: &crate::workspace::WindowKey,
    token: &str,
    tag: &str,
) {
    assert!(
        state
            .workspaces
            .assign(key.clone(), output, workspace, false)
    );
    state.member_tokens.insert(key.clone(), token.to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            exe_path: "test.exe".to_owned(),
        },
    );
    state.member_tags.insert(key.clone(), tag.to_owned());
    inject_pending_tag(key.hwnd, Some(tag.to_owned()));
}

struct SelectRig {
    state: super::TileLoop,
    me: crate::model::ProcessIdentity,
    store: LedgerStore,
    dir: std::path::PathBuf,
    areas: Vec<MonitorArea>,
    fulls: Vec<Rect>,
    source: String,
    target: String,
    keys: Vec<crate::workspace::WindowKey>,
}

/// Forty tiled helpers on the target workspace at gap 8/8, source empty so
/// the select hide phase is a no-op. Engine seeded serially newest-focused
/// through the production domain; hints injected per HWND (no native query,
/// no budget cost).
fn select_rig(name: &str) -> SelectRig {
    use tiler_core::boundary::CoreReply;
    clear_pending_seams();
    clear_test_hints();
    let mut state = test_state();
    let me = me();
    let tmp = std::env::temp_dir().join(format!(
        "tiler-select-repair-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = LedgerStore::open(&tmp).expect("ledger temp dir");
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    assert!(ids.len() >= 2, "source plus target");
    let (source, target) = (ids[0].clone(), ids[1].clone());
    state.workspaces.activate(&output, &source);
    let keys: Vec<crate::workspace::WindowKey> = (0..N).map(|i| member_key(0x2000, i)).collect();
    for (i, key) in keys.iter().enumerate() {
        stage_member(
            &mut state,
            &output,
            &target,
            key,
            &token(i),
            &format!("tag-r{i:02}"),
        );
        inject_test_hints(key.hwnd, hints());
    }
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: WORK,
        full: WORK,
    }];
    let (domain, key) =
        super::workspace_domain_for(&output, &target, &areas, state.inner_gap, state.outer_gap)
            .expect("target domain");
    assert_eq!(domain.gap, 8, "seed runs at gap 8");
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    for i in 0..N {
        let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = (0..=i)
            .map(|j| (WindowId(token(j)), BIRTH, hints(), false))
            .collect();
        let fp = crate::tiling::fingerprint(
            &carried
                .iter()
                .map(|(id, rect, _, _)| (id.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let revision = state
            .engine
            .session(&key)
            .map(|session| session.accepted_revision())
            .unwrap_or(0);
        let event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &CorrelationId::parse(&format!("seed-{i:02}")).expect("correlation"),
            revision,
            fp,
            &domain,
            &key,
            state.outer_gap,
            &carried,
            Some(&WindowId(token(i))),
        );
        let reply = state.engine.handle(&event);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "seed leg {i} converges, got {reply:?}"
        );
    }
    let session = state.engine.session(&key).expect("session");
    assert_eq!(session.snapshot().windows.len(), N, "roster fully tiled");
    assert_eq!(session.capacity_float_count(), 0, "no capacity floats");
    SelectRig {
        state,
        me,
        store,
        dir: tmp.clone(),
        areas,
        fulls: vec![WORK],
        source,
        target,
        keys,
    }
}

impl Drop for SelectRig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        clear_pending_seams();
        clear_test_hints();
    }
}

fn drive_select(
    rig: &mut SelectRig,
    gap: i32,
    correlation: &str,
    skip_geometry: bool,
) -> super::SelectEffect {
    rig.state.inner_gap = gap;
    rig.state.outer_gap = gap;
    let observed: Vec<super::ObservedWindow> = rig
        .keys
        .iter()
        .enumerate()
        .map(|(i, key)| observed_window(key, &token(i), BIRTH))
        .collect();
    inject_pending_observe(observed, Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("focus-ok"));
    let ctx = ActionCtx {
        correlation: correlation.to_owned(),
        tick: rig.state.tick,
        queued_at: std::time::Instant::now(),
        start: std::time::Instant::now(),
    };
    let mut pre_observed: Vec<super::ObservedWindow> = Vec::new();
    super::workspace_do_select(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut pre_observed,
        "mon-a",
        &rig.target.clone(),
        &ctx,
        None,
        false,
        skip_geometry,
    )
}

fn domain_key(rig: &SelectRig) -> tiler_core::session::DomainKey {
    let (_, key) = super::workspace_domain_for(
        "mon-a",
        &rig.target,
        &rig.areas,
        rig.state.inner_gap,
        rig.state.outer_gap,
    )
    .expect("domain");
    key
}

fn victims(rig: &SelectRig) -> BTreeSet<String> {
    let key = domain_key(rig);
    let Some(session) = rig.state.engine.session(&key) else {
        return BTreeSet::new();
    };
    (0..N)
        .map(token)
        .filter(|id| session.is_capacity_float(&WindowId(id.clone())))
        .collect()
}

fn sequences(rig: &SelectRig) -> BTreeMap<String, u64> {
    let key = domain_key(rig);
    let Some(session) = rig.state.engine.session(&key) else {
        return BTreeMap::new();
    };
    (0..N)
        .map(token)
        .map(|id| {
            let seq = session
                .admission_sequence(&WindowId(id.clone()))
                .expect("tracked order");
            (id, seq)
        })
        .collect()
}

fn newest_suffix(sequences: &BTreeMap<String, u64>, n: usize) -> BTreeSet<String> {
    let mut ranked: Vec<(&String, &u64)> = sequences.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| b.0.cmp(a.0)));
    ranked
        .into_iter()
        .take(n)
        .map(|(id, _)| (*id).clone())
        .collect()
}

fn victim_keys(
    rig: &SelectRig,
    victims: &BTreeSet<String>,
) -> BTreeSet<crate::workspace::WindowKey> {
    rig.keys
        .iter()
        .filter(|key| {
            rig.state
                .member_tokens
                .get(*key)
                .is_some_and(|token| victims.contains(token))
        })
        .cloned()
        .collect()
}

#[test]
fn select_reveal_keeps_gap_repair_victims() {
    let mut rig = select_rig("keep");
    let effect = drive_select(&mut rig, 64, "sel-1", false);
    let counters = pending_counters();
    assert_eq!(effect.outcome, "ok", "select completes");
    assert!(counters.engine > 0, "route contacts the Engine");
    assert_eq!(counters.apply, 1, "one geometry pass runs");
    assert_eq!(
        counters.focus, 1,
        "one focus attempt, no per-victim setters"
    );
    assert!(effect.geometry.is_some(), "ordinary select applies");

    let order = sequences(&rig);
    assert_eq!(order.len(), N, "admission order tracked for all 40");
    let expected = newest_suffix(&order, EXPECTED_VICTIMS);
    assert_eq!(expected.len(), EXPECTED_VICTIMS);
    let got = victims(&rig);
    assert_eq!(got, expected, "reveal keeps exactly the newest 30 victims");
    let key = domain_key(&rig);
    assert_eq!(
        rig.state
            .engine
            .session(&key)
            .expect("session")
            .snapshot()
            .windows
            .len(),
        N - EXPECTED_VICTIMS,
        "10 survivors stay tiled"
    );

    // Runtime float/frame lanes register the committed victims (marker
    // install fails closed on fake HWNDs, registration still lands).
    let lanes = victim_keys(&rig, &got);
    assert_eq!(lanes.len(), EXPECTED_VICTIMS);
    assert!(
        lanes.iter().all(|key| rig.state.floated.contains(key)),
        "every victim joins the float lanes"
    );
    assert!(
        got.iter()
            .all(|token| rig.state.float_rects.contains_key(token)),
        "every victim keeps its live frame"
    );

    // Survivor-only writable geometry: no victim takes a tiled write.
    let writables = pending_apply_writables();
    assert_eq!(writables.len(), 1, "one apply scoped: {writables:?}");
    let survivors: BTreeSet<String> = (0..N).map(token).filter(|id| !got.contains(id)).collect();
    assert_eq!(survivors.len(), N - EXPECTED_VICTIMS);
    assert_eq!(writables[0], survivors, "writes cover survivors only");

    // No false explicit override on repair victims.
    assert!(
        lanes
            .iter()
            .all(|key| !rig.state.fixed_tile_overrides.contains(key)),
        "repair floats record no explicit tile win"
    );
    assert!(
        got.iter().all(|id| !rig
            .state
            .engine
            .session(&key)
            .is_some_and(|session| session.has_fixed_tile_override(&WindowId(id.clone())))),
        "repair floats carry no Engine override"
    );

    // No repair focus setter: retained focus resolves to a surviving tile,
    // never a floated victim (repair issues no native focus call).
    let (focus_domain, focus_leaf) = rig.state.engine.session(&key).expect("session").focus();
    assert_eq!(focus_domain, Some(key.clone()), "focus stays homed");
    let focus_leaf = focus_leaf.expect("surviving focus leaf");
    assert!(
        rig.state
            .engine
            .session(&key)
            .expect("session")
            .snapshot()
            .windows
            .iter()
            .any(|link| link.leaf == focus_leaf
                && link
                    .window
                    .0
                    .strip_prefix('r')
                    .is_some_and(|_| survivors.contains(&link.window.0))),
        "focus leaf names a surviving tile"
    );
}

#[test]
fn skipped_geometry_reveal_still_keeps_victims() {
    // Geometry-skipped reveal commits the same repair with no applies:
    // victims keep, lanes register, writables stay empty.
    let mut rig = select_rig("skip");
    let effect = drive_select(&mut rig, 64, "sel-skip", true);
    let counters = pending_counters();
    assert_eq!(effect.outcome, "ok", "skipped reveal completes");
    assert_eq!(counters.apply, 0, "skip applies no geometry");
    assert!(effect.geometry.is_none(), "no summary on the skip route");
    let order = sequences(&rig);
    let expected = newest_suffix(&order, EXPECTED_VICTIMS);
    assert_eq!(victims(&rig), expected, "skip keeps exactly the newest 30");
    let lanes = victim_keys(&rig, &expected);
    assert_eq!(lanes.len(), EXPECTED_VICTIMS);
    assert!(
        lanes.iter().all(|key| rig.state.floated.contains(key)),
        "lanes register without geometry"
    );
    assert!(pending_apply_writables().is_empty(), "no apply scoped");
}

struct ExitRig {
    state: super::TileLoop,
    me: crate::model::ProcessIdentity,
    store: LedgerStore,
    dir: std::path::PathBuf,
    areas: Vec<MonitorArea>,
    fulls: Vec<Rect>,
    target: String,
    akey: crate::workspace::WindowKey,
    bkey: crate::workspace::WindowKey,
}

/// Two-member first-exit rig: A tiled, B floated through the real Engine
/// with a still-pending non-sticky first-exit mark. B rides the deliberate
/// tiled lane (`floating = false`) over its existing exception.
fn first_exit_rig(name: &str) -> ExitRig {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    clear_pending_seams();
    clear_test_hints();
    let mut state = test_state();
    let me = me();
    let tmp = std::env::temp_dir().join(format!(
        "tiler-select-firstexit-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = LedgerStore::open(&tmp).expect("ledger temp dir");
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    assert!(ids.len() >= 2, "source plus target");
    let (source, target) = (ids[0].clone(), ids[1].clone());
    state.workspaces.activate(&output, &source);
    let akey = member_key(0x4000, 0);
    let bkey = member_key(0x4000, 1);
    stage_member(&mut state, &output, &target, &akey, "fa", "tag-fa");
    stage_member(&mut state, &output, &target, &bkey, "fb", "tag-fb");
    inject_test_hints(akey.hwnd, hints());
    inject_test_hints(bkey.hwnd, hints());
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: WORK,
        full: WORK,
    }];
    let (domain, key) =
        super::workspace_domain_for(&output, &target, &areas, state.inner_gap, state.outer_gap)
            .expect("target domain");
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    let carried = |floating_b: bool| {
        vec![
            (WindowId("fa".to_owned()), BIRTH, hints(), false),
            (WindowId("fb".to_owned()), BIRTH, hints(), floating_b),
        ]
    };
    let revision = state
        .engine
        .session(&key)
        .map(|session| session.accepted_revision())
        .unwrap_or(0);
    let fp = crate::tiling::fingerprint(&[("fa".to_owned(), BIRTH), ("fb".to_owned(), BIRTH)]);
    let seed = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("exit-seed").expect("correlation"),
        revision,
        fp,
        &domain,
        &key,
        state.outer_gap,
        &carried(false),
        Some(&WindowId("fa".to_owned())),
    );
    let reply = state.engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed converges, got {reply:?}"
    );
    // Floatify B through the real Engine, then mark its first exit while
    // the exception stands (neither automatic nor intentional).
    let revision = state
        .engine
        .session(&key)
        .map(|session| session.accepted_revision())
        .unwrap_or(0);
    let mut float = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("exit-float").expect("correlation"),
        revision,
        fp,
        &domain,
        &key,
        state.outer_gap,
        &carried(false),
        Some(&WindowId("fa".to_owned())),
    );
    float.command = CoreCommand::ToggleFloat {
        window: "fb".to_owned(),
        float_rect: Some(BIRTH),
    };
    let reply = state.engine.handle(&float);
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "floatify commits, got {reply:?}"
    );
    assert!(
        state
            .engine
            .session(&key)
            .is_some_and(|session| session.is_exception(&WindowId("fb".to_owned()))),
        "B floats slotless"
    );
    state.born_first_exit.insert(bkey.clone());
    ExitRig {
        state,
        me,
        store,
        dir: tmp.clone(),
        areas,
        fulls: vec![WORK],
        target,
        akey,
        bkey,
    }
}

impl Drop for ExitRig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        clear_pending_seams();
        clear_test_hints();
    }
}

fn exit_domain_key(rig: &ExitRig) -> tiler_core::session::DomainKey {
    let (_, key) = super::workspace_domain_for(
        "mon-a",
        &rig.target,
        &rig.areas,
        rig.state.inner_gap,
        rig.state.outer_gap,
    )
    .expect("domain");
    key
}

fn drive_exit(rig: &mut ExitRig, gap: i32, correlation: &str) -> super::SelectEffect {
    rig.state.inner_gap = gap;
    rig.state.outer_gap = gap;
    let observed = vec![
        observed_window(&rig.akey.clone(), "fa", BIRTH),
        observed_window(&rig.bkey.clone(), "fb", BIRTH),
    ];
    inject_pending_observe(observed, Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("focus-ok"));
    let ctx = ActionCtx {
        correlation: correlation.to_owned(),
        tick: rig.state.tick,
        queued_at: std::time::Instant::now(),
        start: std::time::Instant::now(),
    };
    let mut pre_observed: Vec<super::ObservedWindow> = Vec::new();
    super::workspace_do_select(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut pre_observed,
        "mon-a",
        &rig.target.clone(),
        &ctx,
        None,
        false,
        false,
    )
}

fn assert_first_exit_tiled(rig: &ExitRig, what: &str) {
    let key = exit_domain_key(rig);
    let session = rig.state.engine.session(&key).expect("session");
    assert!(
        !session.is_exception(&WindowId("fb".to_owned())),
        "{what}: first exit reclassifies to tiled"
    );
    assert!(
        session
            .snapshot()
            .windows
            .iter()
            .any(|link| link.window.0 == "fb"),
        "{what}: B holds a tile slot"
    );
    assert_eq!(
        session.snapshot().windows.len(),
        2,
        "{what}: both members tiled"
    );
    assert_eq!(
        session.capacity_float_count(),
        0,
        "{what}: no repair floats"
    );
}

#[test]
fn first_exit_tiles_without_gap_drift() {
    // Gaps already match: adoption early-returns with no commit, so the
    // deliberate first-exit tiled row must survive untouched.
    let mut rig = first_exit_rig("nodrift");
    let effect = drive_exit(&mut rig, 8, "exit-1");
    assert_eq!(effect.outcome, "ok", "select completes");
    assert_first_exit_tiled(&rig, "no-drift");
}

#[test]
fn first_exit_tiles_after_successful_gap_adoption() {
    // Gaps drift 8 -> 64 and the adoption commits cleanly (two members
    // fit, no repair): the still-pending first-exit mark must survive the
    // post-commit refresh exactly as row assembly emits it.
    let mut rig = first_exit_rig("drift");
    let effect = drive_exit(&mut rig, 64, "exit-2");
    assert_eq!(effect.outcome, "ok", "select completes");
    let key = exit_domain_key(&rig);
    assert!(
        crate::settings::retained_gaps_match(&rig.state.engine, &key, 64, 64),
        "drifted gaps adopted"
    );
    assert_first_exit_tiled(&rig, "drift");
}

#[test]
fn repeated_select_and_smaller_gaps_do_not_retile() {
    let mut rig = select_rig("repeat");
    let first = drive_select(&mut rig, 64, "sel-1", false);
    assert_eq!(first.outcome, "ok", "first reveal completes");
    let pinned = victims(&rig);
    assert_eq!(pinned.len(), EXPECTED_VICTIMS, "first reveal repairs 30");

    // Back to source without native effects, then reveal again at smaller
    // gaps: survivors fit a fortiori, victims ride floating rows and stay
    // floated with no automatic retiling.
    rig.state.workspaces.activate("mon-a", &rig.source.clone());
    let second = drive_select(&mut rig, 32, "sel-2", false);
    assert_eq!(second.outcome, "ok", "second reveal completes");
    assert_eq!(
        victims(&rig),
        pinned,
        "repeated select at smaller gaps retiles nothing"
    );
    let key = domain_key(&rig);
    assert_eq!(
        rig.state
            .engine
            .session(&key)
            .expect("session")
            .snapshot()
            .windows
            .len(),
        N - EXPECTED_VICTIMS,
        "still 10 tiled after the second reveal"
    );
}

/// Shared-scope twin reproduction through the real shared `workspace_do_select`
/// tail: a 40-roster on (mon-b, target) with drifted gaps must keep its
/// repair victims like the local tail. Both outputs start on empty sources
/// so hide/reveal are no-ops; only the memberful side reconciles.
#[test]
fn shared_select_tail_keeps_gap_repair_victims() {
    use crate::workspace_scope_owner::WorkspaceScopeOwner;
    use tiler_core::boundary::CoreReply;
    use tiler_core::workspace_scope::WorkspaceMode::Shared;
    clear_pending_seams();
    clear_test_hints();
    let mut state = test_state();
    let me = me();
    let tmp = std::env::temp_dir().join(format!(
        "tiler-select-repair-shared-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = LedgerStore::open(&tmp).expect("ledger temp dir");
    state.scope_owner = WorkspaceScopeOwner::adopt_startup(Shared);
    for output in ["mon-a", "mon-b"] {
        state.workspaces.ensure_output(output);
    }
    let aids = state.workspaces.workspace_ids("mon-a");
    let bids = state.workspaces.workspace_ids("mon-b");
    assert!(!aids.is_empty() && bids.len() >= 2, "ids on both outputs");
    let (src_a, src_b, target) = (aids[0].clone(), bids[0].clone(), bids[1].clone());
    state.workspaces.activate("mon-a", &src_a);
    state.workspaces.activate("mon-b", &src_b);
    state.scope_owner.sync_shared_order(&state.workspaces);
    state
        .scope_owner
        .prime_shared_view(&state.workspaces, "mon-a");
    let keys: Vec<crate::workspace::WindowKey> = (0..N).map(|i| member_key(0x3000, i)).collect();
    for (i, key) in keys.iter().enumerate() {
        stage_member(
            &mut state,
            "mon-b",
            &target,
            key,
            &token(i),
            &format!("tag-s{i:02}"),
        );
        inject_test_hints(key.hwnd, hints());
    }
    let areas = vec![
        MonitorArea {
            device: "mon-a".to_owned(),
            work: WORK,
            full: WORK,
        },
        MonitorArea {
            device: "mon-b".to_owned(),
            work: WORK,
            full: WORK,
        },
    ];
    let fulls = vec![WORK, WORK];
    let (domain, key) =
        super::workspace_domain_for("mon-b", &target, &areas, state.inner_gap, state.outer_gap)
            .expect("target domain");
    assert_eq!(domain.gap, 8, "seed runs at gap 8");
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    for i in 0..N {
        let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = (0..=i)
            .map(|j| (WindowId(token(j)), BIRTH, hints(), false))
            .collect();
        let fp = crate::tiling::fingerprint(
            &carried
                .iter()
                .map(|(id, rect, _, _)| (id.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let revision = state
            .engine
            .session(&key)
            .map(|session| session.accepted_revision())
            .unwrap_or(0);
        let event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &CorrelationId::parse(&format!("shared-seed-{i:02}")).expect("correlation"),
            revision,
            fp,
            &domain,
            &key,
            state.outer_gap,
            &carried,
            Some(&WindowId(token(i))),
        );
        let reply = state.engine.handle(&event);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "seed leg {i} converges, got {reply:?}"
        );
    }
    assert!(
        state
            .scope_owner
            .plan_shared_select(&state.workspaces, &target)
            .is_some(),
        "target plans across live outputs"
    );
    state.inner_gap = 64;
    state.outer_gap = 64;
    let observed: Vec<super::ObservedWindow> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| observed_window(key, &token(i), BIRTH))
        .collect();
    inject_pending_observe(observed, Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("focus-ok"));
    let ctx = ActionCtx {
        correlation: "sel-shared-1".to_owned(),
        tick: state.tick,
        queued_at: std::time::Instant::now(),
        start: std::time::Instant::now(),
    };
    let mut pre_observed: Vec<super::ObservedWindow> = Vec::new();
    let effect = super::workspace_do_select(
        &mut state,
        &me,
        &store,
        &tmp,
        &fulls,
        &areas,
        &mut pre_observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        false,
    );
    let counters = pending_counters();
    assert_eq!(effect.outcome, "ok", "shared select converges");
    assert!(counters.engine > 0, "route contacts the Engine");
    let session = state.engine.session(&key).expect("session");
    let order: BTreeMap<String, u64> = (0..N)
        .map(token)
        .map(|id| {
            let seq = session
                .admission_sequence(&WindowId(id.clone()))
                .expect("tracked order");
            (id, seq)
        })
        .collect();
    let expected = newest_suffix(&order, EXPECTED_VICTIMS);
    let got: BTreeSet<String> = (0..N)
        .map(token)
        .filter(|id| session.is_capacity_float(&WindowId(id.clone())))
        .collect();
    assert_eq!(got, expected, "shared tail keeps exactly the newest 30");
    assert_eq!(
        session.snapshot().windows.len(),
        N - EXPECTED_VICTIMS,
        "10 survivors stay tiled"
    );
    let writables = pending_apply_writables();
    let survivors: BTreeSet<String> = (0..N).map(token).filter(|id| !got.contains(id)).collect();
    assert!(
        writables.contains(&survivors),
        "one side writes survivors only: {writables:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
    clear_pending_seams();
    clear_test_hints();
}
