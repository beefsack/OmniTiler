#![forbid(unsafe_code)]
//! Joint hide/reveal batch regressions (offline, native-free): the real
//! select/send/rollback phase state machine with the real recovery ledger
//! plus injected native windows (sys fake backend) and a scripted joint-wait
//! clock. No live effects, no hooks, no timers.

use super::visibility_batch::test_clock::{TestBatchClock, WaitScript, inject_script};
use super::visibility_batch::{HideAttempt, action_clock, log_visibility_timing};
use super::{
    ActionCtx, LedgerStore, MonitorArea, clear_pending_seams, hide_phase, inject_pending_elevated,
    inject_pending_focus, inject_pending_observe, inject_pending_overlay_seq,
    inject_pending_suspend, inject_pending_tag, reveal_phase,
};
use crate::model::{ProcessIdentity, RecoveryLedger, WindowClaimKind, WindowIdentity};
use crate::product_hide::sys::fake;
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;

const BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 800,
    h: 600,
};

fn me() -> ProcessIdentity {
    ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn member_process(key: &crate::workspace::WindowKey) -> ProcessIdentity {
    ProcessIdentity {
        pid: key.pid,
        process_creation: key.creation.clone(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn key(hwnd: u64, pid: u32, creation: &str) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
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

fn action_ctx(state: &super::TileLoop) -> ActionCtx {
    ActionCtx {
        correlation: "act-1".to_owned(),
        tick: state.tick,
        queued_at: std::time::Instant::now(),
        start: std::time::Instant::now(),
    }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "tiler-visbatch-{name}-{}-{:?}-{n}-{}",
        std::process::id(),
        std::thread::current().id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

fn seed_ledger(store: &LedgerStore, owner: &ProcessIdentity) {
    store
        .commit(&RecoveryLedger {
            v: crate::model::LEDGER_SCHEMA_VERSION,
            owner: owner.clone(),
            windows: Vec::new(),
            mouse_snap: None,
        })
        .expect("seed empty ledger");
}

fn ledger_notes(store: &LedgerStore) -> Vec<WindowIdentity> {
    store
        .committed()
        .expect("read ledger")
        .map(|record| record.windows)
        .unwrap_or_default()
}

/// Stage one member with matching stored identity plus lifetime tag on both
/// native seams (tiling tag read and sys fake backend).
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
    state
        .member_identity
        .insert(key.clone(), member_process(key));
    state.member_tags.insert(key.clone(), tag.to_owned());
    inject_pending_tag(key.hwnd, Some(tag.to_owned()));
}

/// Register one scripted native window: visible, unclaimed, admittable.
fn register_visible(key: &crate::workspace::WindowKey, tag: &str) {
    fake::register(
        key.hwnd,
        fake::FakeWindow {
            pid: key.pid,
            process: member_process(key),
            integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
            nonce: None,
            member_tag: Some(tag.to_owned()),
            visible: true,
            iconic: false,
            maximized: false,
            alive: true,
            post_ok: true,
            defer: false,
        },
    );
}

fn setup() -> (
    super::TileLoop,
    ProcessIdentity,
    LedgerStore,
    std::path::PathBuf,
) {
    clear_pending_seams();
    fake::clear();
    let state = super::rmax03_adapter_tests::test_state();
    let me = me();
    let dir = temp_dir("phase");
    let store = LedgerStore::open(&dir).expect("ledger temp dir");
    seed_ledger(&store, &me);
    (state, me, store, dir)
}

fn attempt(key: &crate::workspace::WindowKey, iconic: bool) -> HideAttempt {
    HideAttempt {
        key: key.clone(),
        expected: member_process(key),
        iconic,
    }
}

#[test]
fn hide_posts_all_before_first_snapshot() {
    let (mut state, me, store, dir) = setup();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    let current = ids[0].clone();
    let keys = [
        key(0xA1, 1401, "c0000000000000a1"),
        key(0xA2, 1402, "c0000000000000a2"),
        key(0xA3, 1403, "c0000000000000a3"),
    ];
    let tags = ["tag-a1", "tag-a2", "tag-a3"];
    for (i, k) in keys.iter().enumerate() {
        stage_member(&mut state, &output, &current, k, &format!("w{i}"), tags[i]);
        register_visible(k, tags[i]);
    }
    let ctx = action_ctx(&state);
    let mut clock = TestBatchClock::new(Vec::new());
    let phase = hide_phase(
        &mut state,
        &me,
        &store,
        &dir,
        &ctx,
        keys.iter().map(|k| attempt(k, false)).collect(),
        &mut clock,
    );
    assert!(!phase.failed, "all hides confirm");
    assert!(phase.results.iter().all(|r| r.outcome == "hidden"));
    assert!(clock.waits.is_empty(), "already observed: no wait needed");
    // Posts precede confirmations: every post lands before the first
    // visibility snapshot in the combined native order.
    let ops = fake::op_log();
    let last_post = ops.iter().rposition(|(k, _)| *k == 'p').expect("posts run");
    let first_snapshot = ops
        .iter()
        .position(|(k, _)| *k == 's')
        .expect("snapshots run");
    assert!(last_post < first_snapshot, "posts precede confirmations");
    assert_eq!(
        fake::post_log(),
        vec![(0xA1, true), (0xA2, true), (0xA3, true)]
    );
    assert!(!fake::snapshot_log().is_empty(), "confirmations snapshot");
    // Commit owns every claim: ledger plus owner table, hidden flags set.
    assert_eq!(ledger_notes(&store).len(), 3);
    for k in &keys {
        assert!(state.hidden_claims.contains_key(k));
        assert!(state.workspaces.is_hidden(k));
        assert_eq!(fake::with(k.hwnd, |w| w.visible), Some(false));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hide_completes_on_own_events_despite_unrelated_wakes() {
    let (mut state, me, store, dir) = setup();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    let current = ids[0].clone();
    let keys = [
        key(0xB1, 1411, "c0000000000000b1"),
        key(0xB2, 1412, "c0000000000000b2"),
    ];
    for (i, k) in keys.iter().enumerate() {
        stage_member(&mut state, &output, &current, k, &format!("w{i}"), "tag-b");
        fake::register(
            k.hwnd,
            fake::FakeWindow {
                pid: k.pid,
                process: member_process(k),
                integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
                nonce: None,
                member_tag: Some("tag-b".to_owned()),
                visible: true,
                iconic: false,
                maximized: false,
                alive: true,
                post_ok: true,
                defer: true,
            },
        );
    }
    let ctx = action_ctx(&state);
    // One unrelated wake (no snapshot, no deadline reset), then each
    // window's own visibility event completes it.
    let mut clock = TestBatchClock::new(vec![
        WaitScript::Unrelated,
        WaitScript::NotifyWith(vec![(0xB1, false)]),
        WaitScript::NotifyWith(vec![(0xB2, false)]),
    ]);
    let phase = hide_phase(
        &mut state,
        &me,
        &store,
        &dir,
        &ctx,
        keys.iter().map(|k| attempt(k, false)).collect(),
        &mut clock,
    );
    assert!(phase.results.iter().all(|r| r.outcome == "hidden"));
    assert!(!phase.failed);
    assert_eq!(clock.waits.len(), 3, "unrelated loops without confirming");
    assert_eq!(clock.now, 15, "deadline never reset by unrelated wakes");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_confirmations_share_one_deadline() {
    let (mut state, me, store, dir) = setup();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    let current = ids[0].clone();
    let keys = [
        key(0xD1, 1421, "c0000000000000d1"),
        key(0xD2, 1422, "c0000000000000d2"),
        key(0xD3, 1423, "c0000000000000d3"),
    ];
    for (i, k) in keys.iter().enumerate() {
        stage_member(&mut state, &output, &current, k, &format!("w{i}"), "tag-d");
        fake::register(
            k.hwnd,
            fake::FakeWindow {
                pid: k.pid,
                process: member_process(k),
                integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
                nonce: None,
                member_tag: Some("tag-d".to_owned()),
                visible: true,
                iconic: false,
                maximized: false,
                alive: true,
                post_ok: true,
                defer: true,
            },
        );
    }
    let ctx = action_ctx(&state);
    let mut clock = TestBatchClock::new(Vec::new());
    let phase = hide_phase(
        &mut state,
        &me,
        &store,
        &dir,
        &ctx,
        keys.iter().map(|k| attempt(k, false)).collect(),
        &mut clock,
    );
    assert!(phase.results.iter().all(|r| r.outcome == "uncertain"));
    assert!(phase.failed);
    assert_eq!(clock.waits, vec![2000], "one shared deadline, not N");
    assert_eq!(clock.now, 2000);
    // Failure keeps recoverable ownership: every committed claim stays in
    // the ledger and the owner table for narrow rollback.
    assert_eq!(ledger_notes(&store).len(), 3);
    assert_eq!(phase.newly_hidden.len(), 3);
    for k in &keys {
        assert!(state.hidden_claims.contains_key(k));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_hide_rolls_back_exactly_newly_hidden_and_keeps_ledger() {
    let (mut state, me, store, dir) = setup();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    let current = ids[0].clone();
    let h1 = key(0xE1, 1431, "c0000000000000e1");
    let h2 = key(0xE2, 1432, "c0000000000000e2");
    stage_member(&mut state, &output, &current, &h1, "w0", "tag-e");
    stage_member(&mut state, &output, &current, &h2, "w1", "tag-e");
    register_visible(&h1, "tag-e");
    fake::register(
        h2.hwnd,
        fake::FakeWindow {
            pid: h2.pid,
            process: member_process(&h2),
            integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
            nonce: None,
            member_tag: Some("tag-e".to_owned()),
            visible: true,
            iconic: false,
            maximized: false,
            alive: true,
            post_ok: false,
            defer: false,
        },
    );
    let ctx = action_ctx(&state);
    let mut clock = TestBatchClock::new(Vec::new());
    let phase = hide_phase(
        &mut state,
        &me,
        &store,
        &dir,
        &ctx,
        vec![attempt(&h1, false), attempt(&h2, false)],
        &mut clock,
    );
    let outcomes: std::collections::HashMap<_, _> = phase
        .results
        .iter()
        .map(|r| (r.key.hwnd, r.outcome))
        .collect();
    assert_eq!(outcomes[&0xE1], "hidden");
    assert_eq!(outcomes[&0xE2], "refused");
    assert!(phase.failed);
    // The refused post keeps its committed ledger note: recoverable, never
    // a silent retire and never poisoning the confirmed peer.
    assert_eq!(ledger_notes(&store).len(), 2);
    // Narrow rollback reveals exactly the newly hidden set: the confirmed
    // peer returns to visible, the refused window (never hidden) settles
    // already-visible with no blind replay, and no other window is posted.
    let mut rollback_clock = TestBatchClock::new(Vec::new());
    let reveal = reveal_phase(
        &mut state,
        &me,
        &store,
        &ctx,
        &phase.newly_hidden,
        Some("rollback"),
        true,
        &mut rollback_clock,
    );
    assert!(!reveal.failed);
    let posts = fake::post_log();
    assert!(
        posts.iter().all(|(hwnd, _)| *hwnd == 0xE1 || *hwnd == 0xE2),
        "rollback touches exactly the newly hidden set"
    );
    assert!(
        posts.contains(&(0xE1, false)),
        "confirmed peer revealed back"
    );
    assert!(state.hidden_claims.is_empty());
    assert!(ledger_notes(&store).is_empty());
    assert_eq!(fake::with(h1.hwnd, |w| w.visible), Some(true));
    assert_eq!(fake::with(h2.hwnd, |w| w.visible), Some(true));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reveal_handles_failure_recycling_and_already_visible() {
    let (mut state, me, store, dir) = setup();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    let current = ids[0].clone();
    let r1 = key(0xF1, 1441, "c0000000000000f1");
    let r2 = key(0xF2, 1442, "c0000000000000f2");
    let r3 = key(0xF3, 1443, "c0000000000000f3");
    let r4 = key(0xF4, 1444, "c0000000000000f4");
    let tags = ["tag-r1", "tag-reveal", "tag-reveal", "tag-reveal"];
    let keys = [&r1, &r2, &r3, &r4];
    for (i, k) in keys.iter().enumerate() {
        stage_member(&mut state, &output, &current, k, &format!("w{i}"), tags[i]);
    }
    // Claims as a prior hide left them: hidden, carrying exact valid tags.
    let claims: Vec<WindowIdentity> = keys
        .iter()
        .map(|k| WindowIdentity {
            hwnd: k.hwnd,
            process: member_process(k),
            tag: crate::model::generate_claim_tag(k.pid),
            kind: WindowClaimKind::Product,
            show: crate::model::WindowShowState {
                minimized: false,
                maximized: false,
            },
        })
        .collect();
    for (k, claim) in keys.iter().zip(claims.iter()) {
        state.hidden_claims.insert(
            (*k).clone(),
            super::HiddenRecord {
                claim: claim.clone(),
                iconic: false,
            },
        );
        state.workspaces.set_hidden(k, true);
    }
    let ledger = RecoveryLedger {
        v: crate::model::LEDGER_SCHEMA_VERSION,
        owner: me.clone(),
        windows: claims.clone(),
        mouse_snap: None,
    };
    store.commit(&ledger).expect("seed claims");
    // r1 reveals; r2's post refuses (ledger kept); r3 recycled under its
    // HWND (new process plus no nonce: retires with no foreign writes); r4
    // is already visible (retires its claim without a post).
    fake::register(
        r1.hwnd,
        fake::FakeWindow {
            pid: r1.pid,
            process: member_process(&r1),
            integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
            nonce: Some(claims[0].tag.clone()),
            member_tag: Some(tags[0].to_owned()),
            visible: false,
            iconic: false,
            maximized: false,
            alive: true,
            post_ok: true,
            defer: false,
        },
    );
    fake::register(
        r2.hwnd,
        fake::FakeWindow {
            pid: r2.pid,
            process: member_process(&r2),
            integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
            nonce: Some(claims[1].tag.clone()),
            member_tag: Some(tags[1].to_owned()),
            visible: false,
            iconic: false,
            maximized: false,
            alive: true,
            post_ok: false,
            defer: false,
        },
    );
    fake::register(
        r3.hwnd,
        fake::FakeWindow {
            pid: 9999,
            process: ProcessIdentity {
                pid: 9999,
                process_creation: "recycled".to_owned(),
                user_sid: "S-1-5-test".to_owned(),
                session_id: 1,
                exe_path: "other.exe".to_owned(),
            },
            integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
            nonce: None,
            member_tag: Some(tags[2].to_owned()),
            visible: true,
            iconic: false,
            maximized: false,
            alive: true,
            post_ok: true,
            defer: false,
        },
    );
    fake::register(
        r4.hwnd,
        fake::FakeWindow {
            pid: r4.pid,
            process: member_process(&r4),
            integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
            nonce: Some(claims[3].tag.clone()),
            member_tag: Some(tags[3].to_owned()),
            visible: true,
            iconic: false,
            maximized: false,
            alive: true,
            post_ok: true,
            defer: false,
        },
    );
    let ctx = action_ctx(&state);
    let mut clock = TestBatchClock::new(Vec::new());
    let reveal = reveal_phase(
        &mut state,
        &me,
        &store,
        &ctx,
        &[r1.clone(), r2.clone(), r3.clone(), r4.clone()],
        None,
        false,
        &mut clock,
    );
    assert!(reveal.failed, "the refused post fails the phase");
    assert!(!state.hidden_claims.contains_key(&r1), "revealed completes");
    assert_eq!(fake::with(r1.hwnd, |w| w.visible), Some(true));
    assert!(
        state.hidden_claims.contains_key(&r2),
        "failed reveal keeps its claim"
    );
    assert!(
        ledger_notes(&store).iter().any(|w| w.hwnd == r2.hwnd),
        "failed reveal keeps its recoverable ledger note"
    );
    assert!(
        !ledger_notes(&store).iter().any(|w| w.hwnd == r1.hwnd),
        "revealed note retires"
    );
    assert!(!state.hidden_claims.contains_key(&r3), "recycled retires");
    assert!(
        !state.member_tokens.contains_key(&r3),
        "recycled drops session state"
    );
    assert!(
        !ledger_notes(&store).iter().any(|w| w.hwnd == r3.hwnd),
        "recycled note retires with no foreign writes"
    );
    assert!(
        !fake::post_log().iter().any(|(hwnd, _)| *hwnd == r3.hwnd),
        "recycled takes no post"
    );
    assert!(
        !state.hidden_claims.contains_key(&r4),
        "already-visible completes"
    );
    assert!(
        !fake::post_log().iter().any(|(hwnd, _)| *hwnd == r4.hwnd),
        "already-visible takes no post"
    );
    assert!(
        fake::post_log().contains(&(r2.hwnd, false)),
        "failed reveal still attempted its setter without poisoning peers"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Full select through the production route: leaving hides post before any
/// entering reveal posts, and the revealed target takes focus strictly after
/// both phases confirm.
fn select_rig() -> (
    super::TileLoop,
    ProcessIdentity,
    LedgerStore,
    std::path::PathBuf,
    crate::workspace::WindowKey,
    crate::workspace::WindowKey,
    crate::workspace::WindowKey,
    String,
    String,
) {
    clear_pending_seams();
    fake::clear();
    let mut state = super::rmax03_adapter_tests::test_state();
    let me = me();
    let dir = temp_dir("select");
    let store = LedgerStore::open(&dir).expect("ledger temp dir");
    seed_ledger(&store, &me);
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    assert!(ids.len() >= 2, "current plus target");
    let (current, target) = (ids[0].clone(), ids[1].clone());
    state.workspaces.activate(&output, &current);
    let h1 = key(0x61, 1601, "c000000000000061");
    let h2 = key(0x62, 1602, "c000000000000062");
    let b1 = key(0x63, 1603, "c000000000000063");
    stage_member(&mut state, &output, &current, &h1, "h1", "tag-h1");
    stage_member(&mut state, &output, &current, &h2, "h2", "tag-h2");
    stage_member(&mut state, &output, &target, &b1, "wB", "tag-wb");
    register_visible(&h1, "tag-h1");
    register_visible(&h2, "tag-h2");
    register_visible(&b1, "tag-wb");
    // The entering member starts hidden behind an honestly committed claim:
    // hide it through the real phase first, then switch back to current.
    let pre_ctx = ActionCtx {
        correlation: "act-0".to_owned(),
        tick: state.tick,
        queued_at: std::time::Instant::now(),
        start: std::time::Instant::now(),
    };
    let mut pre_clock = TestBatchClock::new(Vec::new());
    let pre = hide_phase(
        &mut state,
        &me,
        &store,
        &dir,
        &pre_ctx,
        vec![HideAttempt {
            key: b1.clone(),
            expected: member_process(&b1),
            iconic: false,
        }],
        &mut pre_clock,
    );
    assert!(!pre.failed, "entering member pre-hide commits");
    assert!(state.workspaces.is_hidden(&b1));
    state.workspaces.activate(&output, &current);
    // Seed the target Engine session through the production domain.
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: BOUNDS,
        full: BOUNDS,
    }];
    let (domain, domain_key) =
        super::workspace_domain_for(&output, &target, &areas, state.inner_gap, state.outer_gap)
            .expect("target domain");
    let event = crate::tiling::build_reconcile_event_for(
        &state.owner.clone(),
        &state.generation.clone(),
        &CorrelationId::parse("seed").expect("correlation"),
        0,
        1,
        &domain,
        &domain_key,
        state.outer_gap,
        &[(
            WindowId("wB".to_owned()),
            BOUNDS,
            tiler_core::size_hints::WindowSizeHints::none(),
        )],
        Some(&WindowId("wB".to_owned())),
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "target seed converges"
    );
    state.overlay_now.insert("wB".to_owned(), (false, false));
    (state, me, store, dir, h1, h2, b1, current, target)
}

fn drive_select(
    skip_reveal_post: bool,
) -> (
    super::TileLoop,
    super::SelectEffect,
    Vec<(u64, bool)>,
    std::path::PathBuf,
    Vec<u64>,
) {
    let (mut state, me, store, dir, h1, h2, b1, _, target) = select_rig();
    if skip_reveal_post {
        fake::with_mut(b1.hwnd, |w| w.post_ok = false);
    }
    let areas = vec![MonitorArea {
        device: "mon-a".to_owned(),
        work: BOUNDS,
        full: BOUNDS,
    }];
    // Post-reveal fresh observation: the revealed target observes live; the
    // hidden leaving set rides no rows.
    inject_pending_observe(vec![observed_window(&b1, "wB", BOUNDS)], Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("focus-ok"));
    let ctx = action_ctx(&state);
    let mut observed = vec![
        observed_window(&h1, "h1", BOUNDS),
        observed_window(&h2, "h2", BOUNDS),
    ];
    let effect = super::workspace_do_select(
        &mut state,
        &me,
        &store,
        &dir,
        &[BOUNDS],
        &areas,
        &mut observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        true,
    );
    let posts = fake::post_log();
    let notes: Vec<u64> = ledger_notes(&store).iter().map(|w| w.hwnd).collect();
    (state, effect, posts, dir, notes)
}

#[test]
fn select_hides_before_reveals_and_focuses_after_confirms() {
    let (state, effect, posts, dir, notes) = drive_select(false);
    assert_eq!(effect.outcome, "ok");
    assert_eq!(effect.focus, "focus-ok", "focus runs after confirms");
    // All leaving hides post before any entering reveal posts.
    let hide_positions: Vec<usize> = posts
        .iter()
        .enumerate()
        .filter(|(_, (hwnd, hide))| *hide && (*hwnd == 0x61 || *hwnd == 0x62))
        .map(|(i, _)| i)
        .collect();
    let reveal_position = posts
        .iter()
        .position(|(hwnd, hide)| !hide && *hwnd == 0x63)
        .expect("entering reveal posts");
    assert_eq!(hide_positions.len(), 2);
    assert!(
        hide_positions.iter().all(|i| *i < reveal_position),
        "hides post before reveals"
    );
    assert_eq!(state.hidden_claims.len(), 2, "leaving pair stays claimed");
    assert!(
        state
            .workspaces
            .is_hidden(&key(0x61, 1601, "c000000000000061"))
    );
    assert!(
        state
            .workspaces
            .is_hidden(&key(0x62, 1602, "c000000000000062"))
    );
    assert_eq!(fake::with(0x63, |w| w.visible), Some(true));
    assert_eq!(notes, vec![0x61, 0x62], "leaving notes durably committed");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn select_reveal_failure_keeps_current_with_no_focus() {
    let (state, effect, posts, dir, notes) = drive_select(true);
    assert_eq!(effect.outcome, "partial");
    assert_eq!(effect.focus, "none", "focus never runs before confirms");
    // The leaving set rolled back to visible; the failed entering claim
    // stays recoverable in the ledger.
    assert_eq!(fake::with(0x61, |w| w.visible), Some(true));
    assert_eq!(fake::with(0x62, |w| w.visible), Some(true));
    let b1 = key(0x63, 1603, "c000000000000063");
    assert_eq!(
        state.hidden_claims.len(),
        1,
        "only the failed entering claim remains"
    );
    assert!(state.hidden_claims.contains_key(&b1));
    assert_eq!(notes, vec![0x63], "failed entering note stays recoverable");
    assert!(posts.iter().any(|(hwnd, hide)| *hwnd == 0x63 && !hide));
    assert_eq!(
        state.workspaces.active_id("mon-a").as_deref(),
        Some(state.workspaces.workspace_ids("mon-a")[0].as_str())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn send_follow_hide_failure_never_reaches_select() {
    use super::SendPreflight;
    clear_pending_seams();
    fake::clear();
    let mut state = super::rmax03_adapter_tests::test_state();
    let me = me();
    let dir = temp_dir("sendfollow");
    let store = LedgerStore::open(&dir).expect("ledger temp dir");
    seed_ledger(&store, &me);
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    let (source, target) = (ids[0].clone(), ids[1].clone());
    state.workspaces.activate(&output, &source);
    let mover = key(0x71, 1701, "c000000000000071");
    stage_member(&mut state, &output, &source, &mover, "wM", "tag-wm");
    // Membership already transferred (as `workspace_do_send` does before
    // the follow tail), so only the hide can still refuse.
    assert!(
        state
            .workspaces
            .assign(mover.clone(), &output, &target, true)
    );
    fake::register(
        mover.hwnd,
        fake::FakeWindow {
            pid: mover.pid,
            process: member_process(&mover),
            integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
            nonce: None,
            member_tag: Some("tag-wm".to_owned()),
            visible: true,
            iconic: false,
            maximized: false,
            alive: true,
            post_ok: false,
            defer: false,
        },
    );
    inject_pending_overlay_seq(vec![(false, Some(false))]);
    let preflight = SendPreflight {
        fences: crate::workspace_owner::SendFenceSnapshot {
            inner_gap: state.inner_gap,
            outer_gap: state.outer_gap,
            source_tiled: state.workspaces.is_tiled(&output, &source),
            target_tiled: state.workspaces.is_tiled(&output, &target),
            overlay_maximized: false,
            overlay_fullscreen: false,
        },
        fallback_fullscreen: false,
    };
    let ctx = action_ctx(&state);
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: BOUNDS,
        full: BOUNDS,
    }];
    let source_token = state.workspaces.workspace_token(&output, &source);
    let target_token = state.workspaces.workspace_token(&output, &target);
    let mut observed = vec![observed_window(&mover, "wM", BOUNDS)];
    let effect = super::workspace_send_follow(
        &mut state,
        &me,
        &store,
        &dir,
        &[BOUNDS],
        &areas,
        &mut observed,
        &mover,
        &member_process(&mover),
        false,
        &output,
        &target,
        &source_token,
        &target_token,
        &ctx,
        None,
        0,
        0,
        preflight,
        false,
        false,
    );
    assert_eq!(effect.outcome, "refused", "failed hide aborts the follow");
    assert_eq!(effect.focus, "none", "no focus without a reveal");
    // Only the mover hide posted: no select ran, so no other window posted
    // and no reveal ran anywhere.
    assert_eq!(fake::post_log(), vec![(0x71, true)]);
    // The refused post keeps its committed claim: recoverable, never lost.
    assert!(state.hidden_claims.contains_key(&mover));
    assert_eq!(ledger_notes(&store).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn action_clock_factory_replays_scripts_without_live_waits() {
    use crate::product_hide::batch::{BatchSignal, BatchWaitClock};
    clear_pending_seams();
    fake::clear();
    inject_script(vec![WaitScript::Notify, WaitScript::Unrelated]);
    let mut clock = action_clock();
    assert!(matches!(clock.wait(100), BatchSignal::Notified));
    assert!(matches!(clock.wait(100), BatchSignal::Unrelated));
    assert!(matches!(clock.wait(100), BatchSignal::Timeout));
}

#[test]
fn owner_clock_without_events_settles_bounded_timeout() {
    use super::visibility_batch::OwnerBatchClock;
    use crate::product_hide::batch::{BatchSignal, BatchWaitClock};
    clear_pending_seams();
    fake::clear();
    // No hooks, no messages, zero budget: the production clock maps the
    // genuine pump expiry to Timeout (bounded uncertain downstream), never
    // a poll or an extension.
    let mut clock = OwnerBatchClock::new();
    let _ = clock.now_ms();
    assert!(matches!(clock.wait(0), BatchSignal::Timeout));
}

#[test]
fn single_hide_path_still_hides_under_fake() {
    // The remaining single-window path shares admission plus
    // commit/ledger semantics with the joint phases: one fake window hides
    // through `workspace_hide_one` with its legacy per-window wait.
    let (mut state, me, store, dir) = setup();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    let current = ids[0].clone();
    let solo = key(0x91, 1901, "c000000000000091");
    stage_member(&mut state, &output, &current, &solo, "wS", "tag-solo");
    register_visible(&solo, "tag-solo");
    let (outcome, _) = super::workspace_hide_one(
        &mut state,
        &me,
        &store,
        &dir,
        &solo,
        &member_process(&solo),
        false,
    );
    assert_eq!(outcome, "hidden");
    assert!(state.hidden_claims.contains_key(&solo));
    assert_eq!(ledger_notes(&store).len(), 1);
    assert_eq!(fake::with(solo.hwnd, |w| w.visible), Some(false));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn visibility_timing_line_carries_per_phase_breakdown() {
    use super::visibility_batch::PhaseBreakdown;
    clear_pending_seams();
    fake::clear();
    let mut state = super::rmax03_adapter_tests::test_state();
    let dir = temp_dir("timing");
    std::fs::create_dir_all(&dir).expect("log dir");
    state.log_path = dir.join("t.log");
    let ctx = action_ctx(&state);
    let hide = PhaseBreakdown {
        count: 2,
        post_ms: 3,
        wait_ms: 11,
        ledger_ms: 5,
        log_ms: 7,
    };
    let reveal = PhaseBreakdown {
        count: 1,
        post_ms: 2,
        wait_ms: 9,
        ledger_ms: 4,
        log_ms: 6,
    };
    log_visibility_timing(&state, &ctx, "select", &hide, &reveal);
    let text = std::fs::read_to_string(state.log_path.clone()).expect("log written");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "one bounded line per switch");
    let line: serde_json::Value = serde_json::from_str(lines[0]).expect("valid JSON");
    assert_eq!(line["event"], "workspace-visibility-timing");
    assert_eq!(line["correlation"], "act-1");
    assert_eq!(line["side"], "select");
    assert_eq!(line["hide"]["count"], 2);
    assert_eq!(line["hide"]["post_ms"], 3);
    assert_eq!(line["hide"]["wait_ms"], 11);
    assert_eq!(line["hide"]["ledger_ms"], 5);
    assert_eq!(line["hide"]["log_ms"], 7);
    assert_eq!(line["reveal"]["count"], 1);
    assert_eq!(line["reveal"]["wait_ms"], 9);
    assert!(line["action_ms"].is_number(), "action total rides along");
    let _ = std::fs::remove_dir_all(&dir);
}
