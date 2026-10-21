#![forbid(unsafe_code)]
//! Production-seam regressions over [`LiveNative`](super::LiveNative).
//!
//! Every test drives the real actuator type with fully scripted
//! [`NativePrimitives`](super::NativePrimitives) (no HWNDs, hooks, or live
//! effects) through the shared [`apply_world_reply`](super::world_diff::apply_world_reply)
//! proof gates or directly against the actuator. They prove the receipts
//! are never fabricated: setter lies, partial hide/reveal, identity drift,
//! fullscreen/hidden holds, and racing invalidation all hold without an
//! `Applied` outcome, while proven batches still settle.

use super::super::rmax03_adapter_tests::test_state;
use super::super::world_capture::capture_world;
use super::super::world_diff::{FakeNative, FreshHost, HostDiff, NativeOps, apply_world_reply};
use super::super::world_owner::WorldOwner;
use super::super::world_sequencer::AckOutcome;
use super::super::world_tick::merge_applied_transfers;
use super::{AdmitOutcome, LiveNative, NativePrimitives, Revealed, observed_view_receipts};
use crate::model::{ProcessIdentity, WindowClaimKind, WindowIdentity, WindowShowState};
use crate::storage::LedgerStore;
use crate::tiling::{FrameInsets, ObservedTarget, WindowFacts};
use crate::native_guards::WindowKey;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, ScopeAuthority as Auth, ScopeStatus, TransferKind,
    TransferTarget, WORLD_CONTRACT_VERSION, WindowDesire, WorldCommand, WorldReply,
};

const ADAPTER: &str = "adapter-1";
const GENERATION: &str = "gen-1";
const DEV1: &str = "DEV1";
const EXE: &str = "C:\\Apps\\app.exe";

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win_key(hwnd: u64, pid: u32, creation: &str) -> WindowKey {
    WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
    }
}

fn plain_facts() -> WindowFacts {
    WindowFacts {
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
    }
}

fn identity_of(key: &WindowKey) -> ProcessIdentity {
    ProcessIdentity {
        pid: key.pid,
        process_creation: key.creation.clone(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: EXE.to_owned(),
    }
}

fn observed_for(
    key: &WindowKey,
    token: &str,
    vis: Rect,
    facts: WindowFacts,
) -> super::super::ObservedWindow {
    let outer = Rect {
        x: vis.x - 8,
        y: vis.y - 8,
        w: vis.w + 16,
        h: vis.h + 16,
    };
    super::super::ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer,
        visible: vis,
        insets: FrameInsets::measure(outer, vis),
        identity: ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: EXE.to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            tag: "member-tag-1".to_owned(),
        },
        facts,
    }
}

// Scripted native state: thread-local maps keyed by HWND so parallel tests
// sharing a thread never observe each other (every test uses its own HWND
// base and resets first).
thread_local! {
    static S_OBSERVE: RefCell<HashMap<u64, super::super::ObservedWindow>> =
        RefCell::new(HashMap::new());
    static S_PID: RefCell<HashMap<u64, u32>> = RefCell::new(HashMap::new());
    static S_VISIBLE: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static S_ICONIC: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static S_SET_OK: RefCell<HashMap<u64, bool>> = RefCell::new(HashMap::new());
    static S_SET_CALLS: RefCell<Vec<(u64, Rect)>> = const { RefCell::new(Vec::new()) };
    static S_FROZEN_FRAME: RefCell<HashMap<u64, Rect>> = RefCell::new(HashMap::new());
    static S_FG: RefCell<u64> = const { RefCell::new(0) };
    static S_FOCUS_OK: RefCell<HashMap<u64, bool>> = RefCell::new(HashMap::new());
    static S_TAG: RefCell<HashMap<u64, String>> = RefCell::new(HashMap::new());
    static S_HOOK: RefCell<bool> = const { RefCell::new(false) };
    static S_IDENT: RefCell<HashMap<u64, ProcessIdentity>> = RefCell::new(HashMap::new());
    static S_ADMIT: RefCell<HashMap<u64, Result<WindowIdentity, AdmitOutcome>>> =
        RefCell::new(HashMap::new());
    static S_HIDE: RefCell<HashMap<u64, Result<WindowIdentity, String>>> =
        RefCell::new(HashMap::new());
    static S_REVEAL: RefCell<HashMap<u64, Result<Revealed, String>>> =
        RefCell::new(HashMap::new());
    static S_ADMIT_CALLS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

fn reset_script() {
    S_OBSERVE.with(|m| m.borrow_mut().clear());
    S_PID.with(|m| m.borrow_mut().clear());
    S_VISIBLE.with(|m| m.borrow_mut().clear());
    S_ICONIC.with(|m| m.borrow_mut().clear());
    S_SET_OK.with(|m| m.borrow_mut().clear());
    S_SET_CALLS.with(|m| m.borrow_mut().clear());
    S_FROZEN_FRAME.with(|m| m.borrow_mut().clear());
    S_FG.with(|m| *m.borrow_mut() = 0);
    S_FOCUS_OK.with(|m| m.borrow_mut().clear());
    S_TAG.with(|m| m.borrow_mut().clear());
    S_HOOK.with(|m| *m.borrow_mut() = false);
    S_IDENT.with(|m| m.borrow_mut().clear());
    S_ADMIT.with(|m| m.borrow_mut().clear());
    S_HIDE.with(|m| m.borrow_mut().clear());
    S_REVEAL.with(|m| m.borrow_mut().clear());
    S_ADMIT_CALLS.with(|m| m.borrow_mut().clear());
}

fn script_claim(hwnd: u64, identity: &ProcessIdentity) -> WindowIdentity {
    WindowIdentity {
        hwnd,
        process: identity.clone(),
        tag: format!("nonce-{hwnd}"),
        kind: WindowClaimKind::Product,
        show: WindowShowState::default(),
    }
}

/// Hide-claim identity with a well-formed claim tag (16 lowercase hex,
/// never zero): capture's claim association proof (`valid_claim_tag`)
/// fails closed on the `nonce-*` script tags, which is correct for
/// negative tests but would mask the positive receipt path.
fn valid_claim(hwnd: u64, identity: &ProcessIdentity) -> WindowIdentity {
    WindowIdentity {
        hwnd,
        process: identity.clone(),
        tag: "abcdef1234567890".to_owned(),
        kind: WindowClaimKind::Product,
        show: WindowShowState::default(),
    }
}

fn s_observe(
    hwnd: u64,
    _me: &ProcessIdentity,
    _fulls: &[Rect],
    _tokens: &mut super::super::TokenMap,
) -> Result<super::super::ObservedWindow, super::ObserveError> {
    S_OBSERVE
        .with(|m| m.borrow().get(&hwnd).cloned())
        .ok_or(super::ObserveError::Unknown)
}

fn scripted() -> NativePrimitives {
    NativePrimitives {
        observe: s_observe,
        window_pid: |hwnd| S_PID.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(0)),
        is_visible: |hwnd| S_VISIBLE.with(|m| m.borrow().contains(&hwnd)),
        is_iconic: |hwnd| S_ICONIC.with(|m| m.borrow().contains(&hwnd)),
        is_zoomed: |_| false,
        outer_rect: |hwnd| {
            S_FROZEN_FRAME
                .with(|m| m.borrow().get(&hwnd).copied())
                .map(|vis| Rect {
                    x: vis.x - 8,
                    y: vis.y - 8,
                    w: vis.w + 16,
                    h: vis.h + 16,
                })
        },
        set_outer: |hwnd, outer| {
            S_SET_CALLS.with(|m| m.borrow_mut().push((hwnd, outer)));
            S_SET_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
        },
        foreground: || S_FG.with(|m| *m.borrow()),
        focus: |hwnd| S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true)),
        member_tag: |hwnd| S_TAG.with(|m| m.borrow().get(&hwnd).cloned()),
        hook_pending: || S_HOOK.with(|m| *m.borrow()),
        process_identity: |hwnd| S_IDENT.with(|m| m.borrow().get(&hwnd).cloned()),
        read_topmost: |_| false,
        admit: |hwnd, _me, _expected, _tag, _topmost| {
            S_ADMIT_CALLS.with(|m| m.borrow_mut().push(hwnd));
            S_ADMIT.with(|m| {
                m.borrow()
                    .get(&hwnd)
                    .cloned()
                    .unwrap_or(Err(AdmitOutcome::Refused))
            })
        },
        hide_claim: |_store, _me, claim, _dir| {
            S_HIDE.with(|m| {
                m.borrow()
                    .get(&claim.hwnd)
                    .cloned()
                    .unwrap_or(Err("refused: scripted".to_owned()))
            })
        },
        reveal_claim: |_store, _me, claim, _min| {
            S_REVEAL.with(|m| {
                m.borrow()
                    .get(&claim.hwnd)
                    .cloned()
                    .unwrap_or(Err("refused: scripted".to_owned()))
            })
        },
    }
}

struct Env {
    state: super::super::TileLoop,
    me: ProcessIdentity,
    fulls: Vec<Rect>,
    store: LedgerStore,
    dir: std::path::PathBuf,
}

fn env_with(base_hwnd: u64) -> Env {
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let tmp = std::env::temp_dir().join(format!(
        "omnitiler-world-native-{}-{base_hwnd}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmp).expect("temp dir");
    let store = LedgerStore::open(&tmp).expect("ledger temp dir");
    let me = ProcessIdentity {
        pid: 1,
        process_creation: "owner-1".to_owned(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: "C:\\Owner\\owner.exe".to_owned(),
    };
    Env {
        state,
        me,
        fulls: vec![rect(0, 0, 1920, 1080)],
        store,
        dir: tmp,
    }
}

fn admit_member(env: &mut Env, key: &WindowKey, ws: &str, token: &str, _hidden: bool) {
    let out = env.state.output_token(DEV1);
    env.state
        .member_tokens
        .insert(key.clone(), token.to_owned());
    env.state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    env.state
        .member_identity
        .insert(key.clone(), identity_of(key));
    env.state
        .applied_homing
        .insert(token.to_owned(), (out, ws.to_owned()));
}

/// Assemble the production actuator over disjoint owner borrows: the exact
/// split the runtime tick will use (`&mut` caches beside shared tables).
/// Desired core targets derive from the test's applied proven maps plus an
/// optional view override; proven receipts come from the retained maps.
fn assemble<'e>(env: &'e mut Env, prims: NativePrimitives) -> LiveNative<'e> {
    assemble_with_views(env, prims, None)
}

fn assemble_with_views<'e>(
    env: &'e mut Env,
    prims: NativePrimitives,
    view_override: Option<(String, String)>,
) -> LiveNative<'e> {
    let mut desired_views: HashMap<String, String> = env.state.applied_views.clone();
    if let Some((out, view)) = view_override {
        desired_views.insert(out, view);
    }
    // Desired homing mirrors proven for these offline rigs (view switches
    // do not re-home); cross-output move tests override via applied maps.
    let desired_homing = env.state.applied_homing.clone();
    let mut rings: HashMap<String, Vec<String>> = HashMap::new();
    for (out, _home) in env.state.applied_homing.values() {
        rings.entry(out.clone()).or_default();
    }
    for (out, view) in desired_views.iter() {
        rings.entry(out.clone()).or_default();
        let ring = rings.get_mut(out).expect("ring");
        if !ring.contains(view) {
            ring.push(view.clone());
        }
    }
    for home in desired_homing.values() {
        let ring = rings.entry(home.0.clone()).or_default();
        if !ring.contains(&home.1) {
            ring.push(home.1.clone());
        }
    }
    let proven_homing = env.state.applied_homing.clone();
    let proven_views = env.state.applied_views.clone();
    let areas = vec![super::super::MonitorArea {
        device: DEV1.to_owned(),
        work: rect(0, 0, 1920, 1040),
        full: rect(0, 0, 1920, 1080),
    }];
    LiveNative::assemble(
        &env.me,
        &env.fulls,
        &env.state.output_tokens,
        desired_homing,
        rings,
        desired_views,
        HashMap::new(),
        areas,
        proven_homing,
        proven_views,
        &env.state.member_tokens,
        &env.state.member_tags,
        &env.state.member_identity,
        &mut env.state.hidden_claims,
        &env.state.sticky,
        &env.state.float_topmost_prev,
        &env.state.scope,
        &env.state.scope_hosts,
        None,
        false,
        &env.store,
        &env.dir,
        &env.state.active,
        prims,
    )
}

fn script_live(key: &WindowKey, token: &str, vis: Rect, facts: WindowFacts) {
    S_OBSERVE.with(|m| {
        m.borrow_mut()
            .insert(key.hwnd, observed_for(key, token, vis, facts))
    });
    S_PID.with(|m| m.borrow_mut().insert(key.hwnd, key.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(key.hwnd));
    S_FROZEN_FRAME.with(|m| m.borrow_mut().insert(key.hwnd, vis));
    S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
    S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
}

fn reply_shell(obs: u64, rev: u64, scopes: &[(&str, &str)]) -> WorldReply {
    WorldReply {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: ADAPTER.to_owned(),
        core_generation: GENERATION.to_owned(),
        request_id: "req-1".to_owned(),
        based_on_observation_seq: obs,
        state_revision: rev,
        ready: true,
        command_consumed: false,
        scopes: scopes
            .iter()
            .map(|(out, ws)| ScopeStatus {
                scope: format!("{out}/{ws}"),
                authority: Auth::Controlled,
            })
            .collect(),
        workspaces: Vec::new(),
        outputs: Vec::new(),
        windows: Vec::new(),
        transfers: Vec::new(),
        effects: Vec::new(),
        command_result: None,
        presentation_bytes: 0,
    }
}

fn float_row(out: &str, ws: &str, token: &str, target: Rect, op: &str) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(token.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: ws.to_owned(),
        allocation: None,
        effective_frame: Some(target),
        frame_authority: Auth::Untouched,
        membership_authority: Auth::Untouched,
        floating: true,
        float_origin: None,
        frame_operation: Some(op.to_owned()),
    }
}

fn convert(env: &Env, observed: &[super::super::ObservedWindow]) -> (HostDiff, FreshHost) {
    let inputs = env.state.inputs();
    let captured = capture_world(&inputs, Some(observed), &[], &one_area(), 1001);
    assert!(captured.inventory_complete);
    let fresh = FreshHost::from_sources(&inputs, observed, &[], &captured);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    (diff, fresh)
}

fn one_area() -> Vec<super::super::MonitorArea> {
    vec![super::super::MonitorArea {
        device: DEV1.to_owned(),
        work: rect(0, 0, 1920, 1040),
        full: rect(0, 0, 1920, 1080),
    }]
}

fn applied_outcomes(feedback: &[tiler_core::world_report::AttemptFeedback]) -> Vec<String> {
    feedback
        .iter()
        .filter(|f| f.outcome == AttemptOutcome::Applied)
        .map(|f| f.operation_id.clone())
        .collect()
}

#[test]
fn production_setter_lie_never_reports_applied() {
    let mut env = env_with(101_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(101_001, 2001, "proc-1");
    admit_member(&mut env, &key, &ws, "w1", false);
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![observed_for(&key, "w1", frozen, plain_facts())];
    script_live(&key, "w1", frozen, plain_facts());
    // Setter accepts but the frame never moves: the app held its size.
    S_SET_OK.with(|m| m.borrow_mut().insert(key.hwnd, true));
    let (mut diff, fresh) = convert(&env, &observed);
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    reply
        .windows
        .push(float_row(&out, &ws, "w1", rect(100, 100, 400, 300), "op-1"));
    let mut native = assemble(&mut env, scripted());
    let (stats, feedback) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(applied_outcomes(&feedback).is_empty(), "{feedback:?}");
    assert!(
        feedback.iter().any(|f| f.operation_id == "op-1"
            && f.kind == AttemptKind::Property
            && f.outcome == AttemptOutcome::PossiblyExecuted),
        "{feedback:?}"
    );
    assert_eq!(stats.geometry_writes, 1);
    assert!(stats.held > 0);
    assert_eq!(S_SET_CALLS.with(|m| m.borrow().len()), 1);
}

#[test]
fn production_explicit_refusal_settles_refused_proven() {
    let mut env = env_with(102_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(102_001, 2002, "proc-2");
    admit_member(&mut env, &key, &ws, "w1", false);
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![observed_for(&key, "w1", frozen, plain_facts())];
    script_live(&key, "w1", frozen, plain_facts());
    // Explicit refusal with an unchanged readable preimage.
    S_SET_OK.with(|m| m.borrow_mut().insert(key.hwnd, false));
    let (mut diff, fresh) = convert(&env, &observed);
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    reply
        .windows
        .push(float_row(&out, &ws, "w1", rect(100, 100, 400, 300), "op-1"));
    let mut native = assemble(&mut env, scripted());
    let (_stats, feedback) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(applied_outcomes(&feedback).is_empty(), "{feedback:?}");
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-1" && f.outcome == AttemptOutcome::RefusedProven),
        "{feedback:?}"
    );
}

#[test]
fn production_identity_drift_holds_without_receipt() {
    let mut env = env_with(103_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(103_001, 2003, "proc-3");
    admit_member(&mut env, &key, &ws, "w1", false);
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![observed_for(&key, "w1", frozen, plain_facts())];
    script_live(&key, "w1", frozen, plain_facts());
    // The HWND recycled under the same number: fresh reads name another pid.
    S_PID.with(|m| m.borrow_mut().insert(key.hwnd, 9999));
    let drifted = {
        let mut w = observed_for(&key, "w1", frozen, plain_facts());
        w.identity.pid = 9999;
        w
    };
    S_OBSERVE.with(|m| m.borrow_mut().insert(key.hwnd, drifted));
    let (mut diff, fresh) = convert(&env, &observed);
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    reply
        .windows
        .push(float_row(&out, &ws, "w1", rect(100, 100, 400, 300), "op-1"));
    let mut native = assemble(&mut env, scripted());
    let (stats, feedback) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(applied_outcomes(&feedback).is_empty(), "{feedback:?}");
    // The recycled HWND fails the fresh frame read first: narrow hold with
    // no writes and no receipt, never a fabricated frame proof.
    assert!(stats.held > 0, "{stats:?}");
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(S_SET_CALLS.with(|m| m.borrow().len()), 0);
}

#[test]
fn production_racing_invalidation_stops_before_any_setter() {
    let mut env = env_with(104_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(104_001, 2004, "proc-4");
    admit_member(&mut env, &key, &ws, "w1", false);
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![observed_for(&key, "w1", frozen, plain_facts())];
    script_live(&key, "w1", frozen, plain_facts());
    S_HOOK.with(|m| *m.borrow_mut() = true);
    let (mut diff, fresh) = convert(&env, &observed);
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    reply
        .windows
        .push(float_row(&out, &ws, "w1", rect(100, 100, 400, 300), "op-1"));
    let mut native = assemble(&mut env, scripted());
    let (stats, feedback) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(applied_outcomes(&feedback).is_empty(), "{feedback:?}");
    // Racing input stops the pass before the first setter: no writes and
    // no receipt.
    assert!(stats.stopped_mid_apply, "{stats:?}");
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(S_SET_CALLS.with(|m| m.borrow().len()), 0);
}

#[test]
fn production_ambiguous_view_arms_at_most_once() {
    let mut env = env_with(105_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let ws2 = "ws2".to_owned();
    let key = win_key(105_001, 2005, "proc-5");
    admit_member(&mut env, &key, &ws, "w1", false);
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![observed_for(&key, "w1", frozen, plain_facts())];
    script_live(&key, "w1", frozen, plain_facts());
    let (mut diff, fresh) = convert(&env, &observed);
    let mut reply = reply_shell(
        7,
        1,
        &[(out.as_str(), ws.as_str()), (out.as_str(), ws2.as_str())],
    );
    reply.transfers.push(TransferTarget {
        operation_id: "op-view-1".to_owned(),
        kind: TransferKind::View,
        target_output: OutputId(out.clone()),
        target_workspace: ws2.clone(),
        follow: false,
        held: false,
    });
    // Uncertain hide leg: the first attempt is ambiguous ...
    S_ADMIT.with(|m| {
        m.borrow_mut()
            .insert(key.hwnd, Err(AdmitOutcome::Uncertain))
    });
    let mut native = assemble(&mut env, scripted());
    // The engine committed ring (production `core_projections`) always
    // contains the commanded target: the synthetic reply bypasses the
    // engine, so the test stages the ring it would have projected.
    native
        .core_rings
        .entry(out.clone())
        .or_default()
        .push(ws2.clone());
    let (_stats, first) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(
        first.iter().any(|f| f.operation_id == "op-view-1"
            && f.outcome == AttemptOutcome::PossiblyExecuted),
        "{first:?}"
    );
    assert_eq!(S_ADMIT_CALLS.with(|m| m.borrow().len()), 1);
    // ... and the retry never re-dispatches: the spent phase holds
    // silently (no new admit, no resolving feedback) because no fresh
    // proof retires the ambiguous flight. The live source view cannot
    // prove itself against the desired target here: `read_view` only ever
    // proves the desired view or nothing, so the flight persists until a
    // later pass proves arrival. No Applied is fabricated either way; the
    // admit call count is frozen.
    let (_stats2, second) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(
        second.iter().all(|f| f.operation_id != "op-view-1"),
        "spent flight holds with no repeat feedback, got {second:?}"
    );
    assert_eq!(S_ADMIT_CALLS.with(|m| m.borrow().len()), 1);
    assert!(
        second.iter().all(|f| f.outcome != AttemptOutcome::Applied),
        "{second:?}"
    );
}

#[test]
fn production_memberless_active_proves_view_by_batch_completion() {
    let mut env = env_with(106_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    // Second workspace holds no members; nothing is visible anywhere.
    // Proven prior view exists so the empty batch can complete explicitly.
    let ws2 = "ws2".to_owned();
    env.state.applied_views.insert(out.clone(), ws.clone());
    env.state.applied_views.insert(out.clone(), ws2.clone());
    drop(ws);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.read_view(&out).as_deref(), Some(ws2.as_str()));
}

#[test]
fn production_drifted_visibility_withholds_homing() {
    let mut env = env_with(107_001);
    let ws = "ws1".to_owned();
    let key = win_key(107_001, 2007, "proc-7");
    admit_member(&mut env, &key, &ws, "w1", true);
    let identity = identity_of(&key);
    env.state.hidden_claims.insert(
        key.clone(),
        super::super::HiddenRecord {
            claim: script_claim(key.hwnd, &identity),
            iconic: false,
        },
    );
    let mut native = assemble(&mut env, scripted());
    // Hidden windows still carry live identity plus the lifetime tag.
    S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
    S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(&key)));
    // Cache says hidden but the window reads visible: external reveal drift
    // proves nothing, so homing stays unknown instead of echoing the cache.
    S_VISIBLE.with(|m| m.borrow_mut().insert(key.hwnd));
    assert_eq!(native.read_homing("w1"), None);
    // Genuinely invisible: the claim-backed homing proves.
    S_VISIBLE.with(|m| m.borrow_mut().remove(&key.hwnd));
    drop(native);
    let out = env.state.output_token(DEV1);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.read_homing("w1"), Some((out, ws.clone())));
}

#[test]
fn production_fullscreen_foreground_vetoes_frame_write() {
    let mut env = env_with(108_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(108_001, 2008, "proc-8");
    admit_member(&mut env, &key, &ws, "w1", false);
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![observed_for(&key, "w1", frozen, plain_facts())];
    script_live(&key, "w1", frozen, plain_facts());
    // A live fullscreen foreground arrives after the observation.
    let fg_key = win_key(108_002, 2009, "proc-9");
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            fg_key.hwnd,
            observed_for(&fg_key, "fg", rect(0, 0, 1920, 1080), fg_facts),
        )
    });
    S_FG.with(|m| *m.borrow_mut() = fg_key.hwnd);
    S_PID.with(|m| m.borrow_mut().insert(fg_key.hwnd, fg_key.pid));
    let (mut diff, fresh) = convert(&env, &observed);
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    reply
        .windows
        .push(float_row(&out, &ws, "w1", rect(100, 100, 400, 300), "op-1"));
    let mut native = assemble(&mut env, scripted());
    let (stats, feedback) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(applied_outcomes(&feedback).is_empty(), "{feedback:?}");
    assert_eq!(S_SET_CALLS.with(|m| m.borrow().len()), 0);
    assert!(stats.held > 0, "{stats:?}");
}

#[test]
fn memberless_migration_actuates_views_without_members() {
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let ws2 = "ws2".to_owned();
    let out = state.output_token(DEV1);
    // No members anywhere: the migration carries no membership phase, so the
    // target view must still actuate instead of holding forever.
    let fresh = FreshHost {
        baseline: HashMap::new(),
        stored_tags: HashMap::new(),
        views: HashMap::new(),
    };
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws2.as_str())]);
    reply.transfers.push(TransferTarget {
        operation_id: "op-m1".to_owned(),
        kind: TransferKind::Migration,
        target_output: OutputId(out.clone()),
        target_workspace: ws2.clone(),
        follow: false,
        held: false,
    });
    let (stats, feedback) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(
        feedback.iter().any(|f| f.operation_id == "op-m1"
            && f.kind == AttemptKind::Transfer
            && f.outcome == AttemptOutcome::Applied),
        "{feedback:?}"
    );
    assert_eq!(stats.view_writes, 1, "{stats:?}");
    assert!(!stats.stopped_mid_apply);
}

#[test]
fn receipts_withhold_ambiguous_views_and_drifted_homing() {
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let ws = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let k1 = win_key(109_001, 2011, "proc-11");
    let k2 = win_key(109_002, 2012, "proc-12");
    for (key, token) in [(&k1, "w1"), (&k2, "w2")] {
        state.member_tokens.insert(key.clone(), token.to_owned());
        state
            .member_tags
            .insert(key.clone(), "member-tag-1".to_owned());
        state.member_identity.insert(key.clone(), identity_of(key));
    }
    // w1 visible on ws, w2 drifted onto ws2's homing record but observed on
    // ws: two visible workspaces prove no view, and only w1 proves homing.
    let out = state.output_token(DEV1);
    state
        .applied_homing
        .insert("w1".to_owned(), (out.clone(), ws.clone()));
    state
        .applied_homing
        .insert("w2".to_owned(), (out.clone(), ws2.clone()));
    let observed = vec![
        observed_for(&k1, "w1", rect(0, 0, 800, 600), plain_facts()),
        observed_for(&k2, "w2", rect(0, 610, 800, 400), plain_facts()),
    ];
    let retained = Vec::new();
    let areas = one_area();
    let inputs = state.inputs();
    let captured = capture_world(&inputs, Some(&observed), &retained, &areas, 109_001);
    // Two non-sticky workspaces show members: no view receipt either way.
    assert!(
        captured.observed_views.is_empty(),
        "{:?}",
        captured.observed_views
    );
    let w2 = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w2")
        .expect("w2 fact");
    assert_eq!(w2.observed_workspace.as_deref(), Some(ws2.as_str()));
    let views = observed_view_receipts(&inputs, &observed, &areas);
    assert!(views.is_empty());
}

#[test]
fn switch_view_preserves_source_homing_and_activates() {
    let mut env = env_with(110_001);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let leaving = win_key(110_001, 2101, "proc-21");
    let entering = win_key(110_002, 2102, "proc-22");
    admit_member(&mut env, &leaving, &ws1, "w1", false);
    admit_member(&mut env, &entering, &ws2, "w2", true);
    let identity = identity_of(&entering);
    env.state.hidden_claims.insert(
        entering.clone(),
        super::super::HiddenRecord {
            claim: script_claim(entering.hwnd, &identity),
            iconic: false,
        },
    );
    for key in [&leaving, &entering] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    S_VISIBLE.with(|m| m.borrow_mut().insert(leaving.hwnd));
    // Proven view starts on the source; the core already desires the target.
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let mut native = assemble_with_views(
        &mut env,
        scripted(),
        Some((out.clone(), ws2.clone())),
    );
    assert!(native.switch_view(&out, &ws2).expect("switch"));
    assert!(
        native.hidden_claims.contains_key(&leaving),
        "leaving hides in place"
    );
    assert!(
        !native.hidden_claims.contains_key(&entering),
        "entering reveals"
    );
    S_VISIBLE.with(|m| {
        m.borrow_mut().remove(&leaving.hwnd);
        m.borrow_mut().insert(entering.hwnd);
    });
    assert_eq!(native.read_view(&out).as_deref(), Some(ws2.as_str()));
}

#[test]
fn partial_reveal_never_proves_entire_view() {
    let mut env = env_with(111_001);
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    env.state.applied_views.insert(out.clone(), ws2.clone());
    let shown = win_key(111_001, 2111, "proc-31");
    let held = win_key(111_002, 2112, "proc-32");
    admit_member(&mut env, &shown, &ws2, "w1", false);
    admit_member(&mut env, &held, &ws2, "w2", true);
    for key in [&shown, &held] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
    }
    S_VISIBLE.with(|m| m.borrow_mut().insert(shown.hwnd));
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.read_view(&out), None);
    S_VISIBLE.with(|m| m.borrow_mut().insert(held.hwnd));
    native.hidden_claims.remove(&held);
    assert_eq!(native.read_view(&out).as_deref(), Some(ws2.as_str()));
}

#[test]
fn same_pid_creation_drift_withholds_frame_and_foreground() {
    let mut env = env_with(112_001);
    let ws = "ws1".to_owned();
    let key = win_key(112_001, 2121, "proc-41");
    admit_member(&mut env, &key, &ws, "w1", false);
    let mut drifted = observed_for(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    drifted.identity.pid = key.pid;
    drifted.identity.process_creation = "proc-42".to_owned();
    S_OBSERVE.with(|m| m.borrow_mut().insert(key.hwnd, drifted));
    S_PID.with(|m| m.borrow_mut().insert(key.hwnd, key.pid));
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            key.hwnd,
            ProcessIdentity {
                process_creation: "proc-42".to_owned(),
                ..identity_of(&key)
            },
        )
    });
    S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = key.hwnd);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.read_frame("w1"), None);
    assert!(!native.is_foreground("w1"));
}

#[test]
fn hidden_receipt_requires_live_lifetime_and_invisibility() {
    let mut env = env_with(113_001);
    let ws = "ws1".to_owned();
    let key = win_key(113_001, 2131, "proc-51");
    admit_member(&mut env, &key, &ws, "w1", true);
    let identity = identity_of(&key);
    env.state.hidden_claims.insert(
        key.clone(),
        super::super::HiddenRecord {
            claim: script_claim(key.hwnd, &identity),
            iconic: false,
        },
    );
    S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(&key)));
    S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "stale-tag".to_owned()));
    let mut native = assemble(&mut env, scripted());
    assert!(native.read_window("w1").is_none());
    assert_eq!(native.read_homing("w1"), None);
    S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
    S_VISIBLE.with(|m| m.borrow_mut().insert(key.hwnd));
    assert_eq!(native.read_homing("w1"), None);
    S_VISIBLE.with(|m| m.borrow_mut().remove(&key.hwnd));
    drop(native);
    let out = env.state.output_token(DEV1);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.read_homing("w1"), Some((out, ws.clone())));
}

#[test]
fn receipt_updates_never_echo_unproven_desire() {
    // Desired homing/view change with no native arrival: the window is still
    // visible on the source while the desire names an inactive workspace.
    // Receipt updates must stay empty; the caller retains prior receipts.
    let mut env = env_with(114_001);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(114_001, 2141, "proc-61");
    admit_member(&mut env, &key, &ws1, "w1", false);
    env.state.applied_views.insert(out.clone(), ws1.clone());
    S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
    S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(&key)));
    S_VISIBLE.with(|m| m.borrow_mut().insert(key.hwnd));
    let mut native =
        assemble_with_views(&mut env, scripted(), Some((out.clone(), ws2.clone())));
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws1.as_str()), (out.as_str(), ws2.as_str())]);
    reply.windows.push(WindowDesire {
        lifetime: WindowId("w1".to_owned()),
        output: OutputId(out.clone()),
        workspace: ws2.clone(),
        allocation: None,
        effective_frame: None,
        frame_authority: Auth::Untouched,
        membership_authority: Auth::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
    });
    reply.outputs.push(tiler_core::world_report::OutputDesire {
        output: OutputId(out.clone()),
        logical_view: Some(ws2.clone()),
        tiling_mode: tiler_core::world_report::TilingMode::Tiled,
        hold: None,
    });
    let (homing, views) = native.receipt_updates(&reply);
    assert!(homing.is_empty(), "unproven desire echoed: {homing:?}");
    assert!(views.is_empty(), "unproven view echoed: {views:?}");
}

#[test]
fn receipt_updates_store_only_visibility_proven_arrival() {
    // Post-switch native state: leaving member hidden, entering member
    // visible, desired view active. All three receipts store.
    let mut env = env_with(115_001);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let leaving = win_key(115_001, 2151, "proc-71");
    let entering = win_key(115_002, 2152, "proc-72");
    admit_member(&mut env, &leaving, &ws1, "w1", false);
    admit_member(&mut env, &entering, &ws2, "w2", false);
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let identity = identity_of(&entering);
    env.state.hidden_claims.insert(
        leaving.clone(),
        super::super::HiddenRecord {
            claim: script_claim(leaving.hwnd, &identity_of(&leaving)),
            iconic: false,
        },
    );
    let _ = identity;
    for key in [&leaving, &entering] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
    }
    S_VISIBLE.with(|m| m.borrow_mut().insert(entering.hwnd));
    let mut native =
        assemble_with_views(&mut env, scripted(), Some((out.clone(), ws2.clone())));
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws1.as_str()), (out.as_str(), ws2.as_str())]);
    for (token, ws) in [("w1", ws1.clone()), ("w2", ws2.clone())] {
        reply.windows.push(WindowDesire {
            lifetime: WindowId(token.to_owned()),
            output: OutputId(out.clone()),
            workspace: ws,
            allocation: None,
            effective_frame: None,
            frame_authority: Auth::Untouched,
            membership_authority: Auth::Controlled,
            floating: false,
            float_origin: None,
            frame_operation: None,
        });
    }
    reply.outputs.push(tiler_core::world_report::OutputDesire {
        output: OutputId(out.clone()),
        logical_view: Some(ws2.clone()),
        tiling_mode: tiler_core::world_report::TilingMode::Tiled,
        hold: None,
    });
    let (homing, views) = native.receipt_updates(&reply);
    assert_eq!(
        homing.get("w1"),
        Some(&(out.clone(), ws1.clone())),
        "leaving hide proves: {homing:?}"
    );
    assert_eq!(
        homing.get("w2"),
        Some(&(out.clone(), ws2.clone())),
        "entering reveal proves: {homing:?}"
    );
    assert_eq!(views.get(&out), Some(&ws2), "proven view stores: {views:?}");
}

#[test]
fn capture_withholds_cross_output_visible_echo() {
    // Retained receipt names o2/ws9 but the live frame sits on o1: plain
    // visibility never proves cross-output arrival.
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let out = state.output_token(DEV1);
    let key = win_key(116_001, 2161, "proc-81");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(key.clone(), identity_of(&key));
    state
        .applied_homing
        .insert("w1".to_owned(), ("o2".to_owned(), "ws9".to_owned()));
    state
        .applied_views
        .insert("o2".to_owned(), "ws9".to_owned());
    let observed = vec![observed_for(&key, "w1", rect(0, 0, 800, 600), plain_facts())];
    let areas = one_area();
    let inputs = state.inputs();
    let captured = capture_world(&inputs, Some(&observed), &[], &areas, 116_001);
    let w1 = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w1")
        .expect("w1 fact");
    assert_eq!(w1.observed_workspace, None, "cross-output echo: {w1:?}");
    assert!(
        captured.observed_views.is_empty(),
        "wrong-output view receipt: {:?}",
        captured.observed_views
    );
    let _ = out;
}

// ---------------------------------------------------------------------------
// Positive production proof: scripted `NativePrimitives` over the real
// `LiveNative` actuator plus real `WorldOwner`/`HostDiff` sequencing.
//
// Every rig below drives the exact production flow (capture, sequence,
// deliver, diff-apply, receipt, merge, ack) with a simulated desktop:
// `set_outer` physically moves the observed frame through the measured
// DWM insets, hide/reveal flip live visibility, and the setter may lie.
// FakeNative is never the oracle here: arrivals prove only through
// fresh frame plus visibility plus full identity reads.
// ---------------------------------------------------------------------------

/// `set_outer` that behaves like the desktop: an accepted write moves the
/// observed visible frame to the target (outer converted back through the
/// measured insets). A lying setter is simulated by using the base
/// `scripted()` override instead, which records the call but never moves.
fn s_set_outer_moving(hwnd: u64, outer: Rect) -> bool {
    S_SET_CALLS.with(|m| m.borrow_mut().push((hwnd, outer)));
    if !S_SET_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true)) {
        return false;
    }
    let vis = Rect {
        x: outer.x + 8,
        y: outer.y + 8,
        w: outer.w - 16,
        h: outer.h - 16,
    };
    S_FROZEN_FRAME.with(|m| m.borrow_mut().insert(hwnd, vis));
    S_OBSERVE.with(|m| {
        if let Some(w) = m.borrow_mut().get_mut(&hwnd) {
            w.visible = vis;
            w.outer = outer;
            w.insets = FrameInsets::measure(outer, vis);
        }
    });
    true
}

fn s_hide_conceal(
    store: &LedgerStore,
    me: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
) -> Result<WindowIdentity, String> {
    let result = S_HIDE.with(|m| {
        m.borrow()
            .get(&claim.hwnd)
            .cloned()
            .unwrap_or(Err("refused: scripted".to_owned()))
    });
    if result.is_ok() {
        S_VISIBLE.with(|m| m.borrow_mut().remove(&claim.hwnd));
    }
    let _ = (store, me, dir);
    result
}

fn s_reveal_show(
    store: &LedgerStore,
    me: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
) -> Result<Revealed, String> {
    let result = S_REVEAL.with(|m| {
        m.borrow()
            .get(&claim.hwnd)
            .cloned()
            .unwrap_or(Err("refused: scripted".to_owned()))
    });
    if result.is_ok() {
        S_VISIBLE.with(|m| m.borrow_mut().insert(claim.hwnd));
    }
    let _ = (store, me, minimized);
    result
}

/// Production seam with a live desktop behind it: writes move frames,
/// hide/reveal flip visibility. Refusals and lies still behave per flag.
fn moving_prims() -> NativePrimitives {
    let mut prims = scripted();
    prims.set_outer = s_set_outer_moving;
    prims.hide_claim = s_hide_conceal;
    prims.reveal_claim = s_reveal_show;
    prims
}

/// Currently visible live observations (hidden-claim windows read
/// invisible, so they ride no observed row, like the real enumeration).
fn live_visible_observed() -> Vec<super::super::ObservedWindow> {
    let visible: HashSet<u64> = S_VISIBLE.with(|m| m.borrow().clone());
    let mut out: Vec<super::super::ObservedWindow> = S_OBSERVE
        .with(|m| {
            m.borrow()
                .values()
                .filter(|w| visible.contains(&w.hwnd))
                .cloned()
                .collect()
        });
    out.sort_by_key(|w| w.hwnd);
    out
}

/// Mechanical admission only: stable token, identity, and lifetime tag.
/// Plants no applied receipts (the first tick must prove them).
fn admit_bare(env: &mut Env, key: &WindowKey, token: &str) {
    env.state
        .member_tokens
        .insert(key.clone(), token.to_owned());
    env.state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    env.state
        .member_identity
        .insert(key.clone(), identity_of(key));
}

struct LiveTick {
    reply: WorldReply,
    feedback: Vec<AttemptFeedback>,
    stats: super::super::world_diff::ApplyStats,
}

/// One production tick through the real owner with scripted natives:
/// dispatch, deliver, diff-apply, receipt, merge with independent fresh
/// revalidation, feedback staging, and acknowledgement. Mirrors
/// `run_observation_tick`/`run_world_command` with injectable prims.
#[allow(clippy::too_many_arguments)]
fn live_tick(
    env: &mut Env,
    owner: &mut WorldOwner,
    observed: &[super::super::ObservedWindow],
    retained: &[super::super::RetainedRow],
    areas: &[super::super::MonitorArea],
    fulls: &[Rect],
    prims: NativePrimitives,
    foreground: u64,
    command: Option<WorldCommand>,
) -> LiveTick {
    owner.invalidate();
    let inputs = env.state.inputs();
    let report = match command {
        None => owner
            .try_dispatch_observation(&inputs, Some(observed), retained, areas, foreground)
            .expect("dispatch observation"),
        Some(cmd) => owner
            .try_dispatch_command(&inputs, Some(observed), retained, areas, foreground, cmd)
            .expect("dispatch command"),
    };
    let reply = match owner.deliver_production(&report) {
        WorldOutcome::Accepted(reply)
        | WorldOutcome::Duplicate(reply)
        | WorldOutcome::CommandConsumed(reply) => reply,
        WorldOutcome::Rejected { kind, message } => panic!("production rejected {kind}: {message}"),
    };
    let inputs = env.state.inputs();
    let captured = capture_world(&inputs, Some(observed), retained, areas, foreground);
    if std::env::var("STAYDBG").is_ok() {
        eprintln!(
            "STAYDBG reply_windows={:?}",
            reply
                .windows
                .iter()
                .map(|r| (
                    r.lifetime.0.clone(),
                    r.output.0.clone(),
                    r.workspace.clone(),
                    r.effective_frame,
                    r.frame_authority,
                    r.membership_authority,
                    r.floating
                ))
                .collect::<Vec<_>>()
        );
    }
    let fresh = FreshHost::from_sources(&inputs, observed, retained, &captured);
    let (homing, rings, views) = owner.core_projections();
    let mut desired_frames: HashMap<String, Rect> = HashMap::new();
    for row in &reply.windows {
        if !row.floating
            && let Some(target) = row.effective_frame
        {
            desired_frames.insert(row.lifetime.0.clone(), target);
        }
    }
    let mut native = LiveNative::assemble(
        &env.me,
        fulls,
        &env.state.output_tokens,
        homing,
        rings,
        views,
        desired_frames,
        areas.to_vec(),
        env.state.applied_homing.clone(),
        env.state.applied_views.clone(),
        &env.state.member_tokens,
        &env.state.member_tags,
        &env.state.member_identity,
        &mut env.state.hidden_claims,
        &env.state.sticky,
        &env.state.float_topmost_prev,
        &env.state.scope,
        &env.state.scope_hosts,
        None,
        false,
        &env.store,
        &env.dir,
        &env.state.active,
        prims,
    );
    owner.sync_diff_gate();
    let (stats, feedback) = owner.apply_accepted(&reply, &fresh, &mut native);
    let (mut proven_h, mut proven_v) = native.receipt_updates(&reply);
    merge_applied_transfers(&reply, &feedback, &mut native, &mut proven_h, &mut proven_v);
    drop(native);
    for (token, home) in proven_h {
        env.state.applied_homing.insert(token, home);
    }
    for (output, view) in proven_v {
        env.state.applied_views.insert(output, view);
    }
    for item in &feedback {
        owner.note_feedback(item.clone());
    }
    let mut pending = std::mem::take(&mut env.state.pending_closed);
    let (ack, pruned) = owner.ack(&reply, &mut pending);
    env.state.pending_closed = pending;
    for token in pruned {
        env.state.applied_homing.remove(&token);
    }
    match ack {
        AckOutcome::Applied(_) => owner.note_diff_applied(&reply),
        AckOutcome::Stale(_) => {}
        AckOutcome::Ignored { reason } => panic!("ack ignored: {reason}"),
    }
    LiveTick {
        reply,
        feedback,
        stats,
    }
}

fn applied_of(feedback: &[AttemptFeedback], op: &str) -> bool {
    feedback.iter().any(|f| {
        f.operation_id == op
            && f.kind == AttemptKind::Transfer
            && f.outcome == AttemptOutcome::Applied
    })
}

fn window_op(reply: &WorldReply, window: &str) -> String {
    reply
        .transfers
        .iter()
        .find(|t| {
            matches!(&t.kind, TransferKind::Window(w) if w.0 == window)
        })
        .expect("window transfer retained")
        .operation_id
        .clone()
}

fn second_workspace(owner: &WorldOwner, output: &str, current: &str) -> String {
    owner
        .engine()
        .logical_ring(output)
        .into_iter()
        .find(|id| id != current)
        .expect("second workspace")
}

#[test]
fn bootstrap_first_binding_proves_homing_and_views_then_settles() {
    let mut env = env_with(201_001);
    let out = env.state.output_token(DEV1);
    let key = win_key(201_001, 2201, "proc-101");
    admit_bare(&mut env, &key, "w1");
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    // Tick 1: bootstrap. Geometry places the core slot; the exact
    // post-write frame plus visibility proves the first homing and view
    // with no prior receipt to re-prove.
    let observed = live_visible_observed();
    let tick1 = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(tick1.reply.ready);
    let home = env
        .state
        .applied_homing
        .get("w1")
        .expect("first homing proves")
        .clone();
    assert_eq!(home.0, out, "homed on the live output: {home:?}");
    let view = env
        .state
        .applied_views
        .get(&out)
        .expect("first view proves")
        .clone();
    assert_eq!(home.1, view, "homing names the proven view");
    // The placed frame is the reply slot, not the pre-move rectangle.
    let slot = tick1
        .reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .and_then(|row| row.effective_frame)
        .expect("tiled slot");
    let live = live_visible_observed();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].visible, slot, "slot physically placed");
    // Tick 2: the next report carries the receipts and settles with no
    // retained flights.
    let observed = live_visible_observed();
    let tick2 = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(tick2.reply.ready);
    assert!(
        tick2.reply.transfers.is_empty(),
        "no flights survive proof: {:?}",
        tick2.reply.transfers
    );
    assert_eq!(env.state.applied_homing.get("w1"), Some(&home));
    assert_eq!(env.state.applied_views.get(&out), Some(&view));
}

#[test]
fn same_output_numbered_send_follow_settles() {
    let mut env = env_with(202_001);
    let out = env.state.output_token(DEV1);
    let key = win_key(202_001, 2202, "proc-102");
    admit_bare(&mut env, &key, "w1");
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    S_ADMIT.with(|m| {
        m.borrow_mut()
            .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(&key))))
    });
    S_HIDE.with(|m| {
        m.borrow_mut()
            .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(&key))))
    });
    S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let current = owner
        .engine()
        .logical_view(&out)
        .expect("current view");
    let target = second_workspace(&owner, &out, &current);
    let ring = owner.engine().logical_ring(&out);
    let index = ring
        .iter()
        .position(|id| id == &target)
        .expect("target in ring") as u8
        + 1;
    // Numbered send with follow: the mover hides onto the target
    // workspace; the follow view batch cannot prove while the core view
    // stays behind, so the transfer holds as possibly-executed with the
    // homing unproven. The explicit select then proves both.
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        key.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index,
            follow: true,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    assert!(
        tick.feedback.iter().any(|f| f.operation_id == op
            && f.kind == AttemptKind::Transfer
            && f.outcome == AttemptOutcome::PossiblyExecuted),
        "follow holds until the view arrives: {:?}",
        tick.feedback
    );
    assert!(
        !applied_of(&tick.feedback, &op),
        "no fabricated arrival: {:?}",
        tick.feedback
    );
    // The follow batch ran the hide leg (identity plus claim proof) but
    // the view stays unproven: the mover is revealed natively while the
    // core view never moved.
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&key.hwnd)),
        "follow batch reveals the mover"
    );
    assert!(env.state.hidden_claims.get(&key).is_none());
    // Explicit select proves the view; the held transfer settles on the
    // proven homing without re-actuation.
    let observed = live_visible_observed();
    let selected = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId(out.clone()),
            workspace: target.clone(),
        }),
    );
    let view_op = selected
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("view flight")
        .operation_id
        .clone();
    assert!(
        selected.feedback.iter().any(|f| f.operation_id == view_op
            && f.kind == AttemptKind::View
            && f.outcome == AttemptOutcome::Applied),
        "select proves: {:?}",
        selected.feedback
    );
    assert!(applied_of(&selected.feedback, &op), "{:?}", selected.feedback);
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone())),
        "send-follow homing proves"
    );
    assert_eq!(env.state.applied_views.get(&out), Some(&target));
    // Next report settles: the flight is gone, receipts retained.
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "send-follow settles: {:?}",
        settled.reply.transfers
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone()))
    );
}

#[test]
fn same_output_send_stay_hides_and_settles() {
    let mut env = env_with(203_001);
    let out = env.state.output_token(DEV1);
    let key = win_key(203_001, 2203, "proc-103");
    admit_bare(&mut env, &key, "w1");
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    S_ADMIT.with(|m| {
        m.borrow_mut()
            .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(&key))))
    });
    S_HIDE.with(|m| {
        m.borrow_mut()
            .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(&key))))
    });
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let current = owner
        .engine()
        .logical_view(&out)
        .expect("current view");
    let target = second_workspace(&owner, &out, &current);
    // Stay send: the mover leaves the visible set; the view never follows.
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        key.hwnd,
        Some(WorldCommand::Send {
            window: WindowId("w1".to_owned()),
            target_output: OutputId(out.clone()),
            target_workspace: target.clone(),
            follow: false,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    assert!(applied_of(&tick.feedback, &op), "{:?}", tick.feedback);
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone())),
        "stay homing proves through the hide"
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&key.hwnd)),
        "stayed window reads invisible"
    );
    assert_eq!(
        env.state.applied_views.get(&out),
        Some(&current),
        "stay never moves the view"
    );
    // Next report settles on the hidden receipt.
    let observed = live_visible_observed();
    assert!(observed.is_empty(), "nothing visible while stayed away");
    eprintln!(
        "STAYDBG applied_homing={:?} applied_views={:?} hidden={:?}",
        env.state.applied_homing,
        env.state.applied_views,
        env.state.hidden_claims.keys().collect::<Vec<_>>()
    );
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "stay settles: {:?}",
        settled.reply.transfers
    );
}

fn two_output_areas() -> (Vec<super::super::MonitorArea>, Vec<Rect>) {
    let areas = vec![
        super::super::MonitorArea {
            device: DEV1.to_owned(),
            work: rect(0, 0, 1920, 1040),
            full: rect(0, 0, 1920, 1080),
        },
        super::super::MonitorArea {
            device: DEV2.to_owned(),
            work: rect(1920, 0, 1920, 1040),
            full: rect(1920, 0, 1920, 1080),
        },
    ];
    let fulls = vec![rect(0, 0, 1920, 1080), rect(1920, 0, 1920, 1080)];
    (areas, fulls)
}

const DEV2: &str = "DEV2";

fn cross_output_target(owner: &WorldOwner, output: &str) -> String {
    // The output's own active view: arrival needs no view switch, so the
    // frame plus visibility proof alone settles the crossing.
    owner
        .engine()
        .logical_view(output)
        .expect("target output view")
}

#[test]
fn tiled_cross_output_send_writes_target_frame_both_ways() {
    let mut env = env_with(204_001);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = two_output_areas();
    let key = win_key(204_001, 2204, "proc-104");
    // Residents keep both outputs' engine domains live: the mover never
    // targets an empty domain the session cannot plan.
    let stay1 = win_key(204_011, 2214, "proc-114");
    let stay2 = win_key(204_012, 2215, "proc-115");
    admit_bare(&mut env, &key, "w1");
    admit_bare(&mut env, &stay1, "w0");
    admit_bare(&mut env, &stay2, "w2");
    script_live(&key, "w1", rect(100, 100, 400, 300), plain_facts());
    script_live(&stay1, "w0", rect(200, 200, 300, 200), plain_facts());
    script_live(&stay2, "w2", rect(2000, 100, 400, 300), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    // Forward: o1 -> o2 active view with follow. The directional send
    // resolves the strict neighbor plus its current view; the redirect
    // lane publishes no frame, so the seam translates the live frame
    // mechanically (FULL-delta) and proves arrival by affinity plus
    // the view batch; the sweep re-tiles to the exact slot after.
    let target_b = cross_output_target(&owner, &o2);
    let observed = live_visible_observed();
    let pre = observed
        .iter()
        .find(|w| w.token == "w1")
        .expect("mover observed")
        .visible;
    let remapped = Rect {
        x: pre.x + 1920,
        y: pre.y,
        w: pre.w,
        h: pre.h,
    };
    let before_writes = S_SET_CALLS.with(|m| m.borrow().len());
    let forward = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        key.hwnd,
        Some(WorldCommand::SendDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            follow: true,
        }),
    );
    let op = window_op(&forward.reply, "w1");
    assert!(applied_of(&forward.feedback, &op), "{:?}", forward.feedback);
    let expected_outer = Rect {
        x: remapped.x - 8,
        y: remapped.y - 8,
        w: remapped.w + 16,
        h: remapped.h + 16,
    };
    let writes: Vec<(u64, Rect)> = S_SET_CALLS.with(|m| m.borrow().clone());
    assert!(
        writes.len() > before_writes,
        "cross-output physically writes: {writes:?}"
    );
    assert!(
        writes
            .iter()
            .any(|(hwnd, outer)| *hwnd == key.hwnd && *outer == expected_outer),
        "DWM-inset outer write for the translated target: {writes:?} vs {expected_outer:?}"
    );
    let live = live_visible_observed();
    let w1live = live
        .iter()
        .find(|w| w.token == "w1")
        .expect("mover stays visible");
    assert_eq!(w1live.visible, remapped, "frame lands exactly on target");
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(o2.clone(), target_b.clone()))
    );
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "forward crossing settles: {:?}",
        settled.reply.transfers
    );
    // Back: o2 -> o1 active view with follow (same mechanical proof).
    let target_a = cross_output_target(&owner, &o1);
    let observed = live_visible_observed();
    let pre_back = observed
        .iter()
        .find(|w| w.token == "w1")
        .expect("mover observed")
        .visible;
    let remapped_back = Rect {
        x: pre_back.x - 1920,
        y: pre_back.y,
        w: pre_back.w,
        h: pre_back.h,
    };
    let back = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        key.hwnd,
        Some(WorldCommand::SendDirectional {
            window: WindowId("w1".to_owned()),
            direction: "left".to_owned(),
            follow: true,
        }),
    );
    let back_op = window_op(&back.reply, "w1");
    assert!(applied_of(&back.feedback, &back_op), "{:?}", back.feedback);
    let live = live_visible_observed();
    let w1live = live
        .iter()
        .find(|w| w.token == "w1")
        .expect("mover stays visible on return");
    assert_eq!(
        w1live.visible, remapped_back,
        "return frame lands exactly"
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(o1.clone(), target_a.clone()))
    );
    let observed = live_visible_observed();
    let resettled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        resettled.reply.transfers.is_empty(),
        "return crossing settles: {:?}",
        resettled.reply.transfers
    );
}

#[test]
fn lying_setter_keeps_no_receipt_and_flight_pending() {
    let mut env = env_with(205_001);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = two_output_areas();
    let key = win_key(205_001, 2205, "proc-105");
    let stay2 = win_key(205_012, 2215, "proc-115");
    admit_bare(&mut env, &key, "w1");
    admit_bare(&mut env, &stay2, "w2");
    script_live(&key, "w1", rect(100, 100, 400, 300), plain_facts());
    script_live(&stay2, "w2", rect(2000, 100, 400, 300), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let home_before = env.state.applied_homing.get("w1").cloned().unwrap();
    let target_b = cross_output_target(&owner, &o2);
    // Lying desktop: the setter reports success but the frame never moves
    // (base scripted seam records the call without moving). Directional
    // send owns the crossing; explicit Send stays same-output only.
    S_SET_OK.with(|m| m.borrow_mut().insert(key.hwnd, true));
    let observed = live_visible_observed();
    let before_mover = S_SET_CALLS.with(|m| m.borrow().iter().filter(|(h, _)| *h == key.hwnd).count());
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        scripted(),
        key.hwnd,
        Some(WorldCommand::SendDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            follow: true,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    assert!(
        tick.feedback.iter().any(|f| f.operation_id == op
            && f.kind == AttemptKind::Transfer
            && f.outcome == AttemptOutcome::PossiblyExecuted),
        "lie holds ambiguous: {:?}",
        tick.feedback
    );
    assert!(
        !applied_of(&tick.feedback, &op),
        "no fabricated arrival: {:?}",
        tick.feedback
    );
    assert_eq!(
        S_SET_CALLS.with(|m| m.borrow().iter().filter(|(h, _)| *h == key.hwnd).count()),
        before_mover + 1,
        "one attempted mover write, never repeated: {:?}",
        S_SET_CALLS.with(|m| m.borrow().clone())
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&home_before),
        "no receipt merges on a lie"
    );
    // The flight stays pending: the next report retains the transfer.
    let observed = live_visible_observed();
    let pending = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        pending
            .reply
            .transfers
            .iter()
            .any(|t| t.operation_id == op),
        "flight pending after lie: {:?}",
        pending.reply.transfers
    );
}

#[test]
fn floating_cross_output_send_preserves_rectangle_and_settles() {
    let mut env = env_with(206_001);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = two_output_areas();
    let key = win_key(206_001, 2206, "proc-106");
    // Tiled resident keeps the target output's engine domain plannable.
    let stay2 = win_key(206_012, 2216, "proc-116");
    admit_bare(&mut env, &key, "w1");
    admit_bare(&mut env, &stay2, "w2");
    env.state.floated.insert(key.clone());
    let float_rect = rect(120, 120, 400, 300);
    script_live(&key, "w1", float_rect, plain_facts());
    script_live(&stay2, "w2", rect(2000, 100, 400, 300), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        env.state.applied_homing.get("w1").is_some(),
        "float bootstraps a receipt"
    );
    let target_b = cross_output_target(&owner, &o2);
    let writes_before = S_SET_CALLS.with(|m| m.borrow().len());
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        key.hwnd,
        Some(WorldCommand::SendDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            follow: true,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    assert!(applied_of(&tick.feedback, &op), "{:?}", tick.feedback);
    assert_eq!(
        S_SET_CALLS.with(|m| m.borrow().len()),
        writes_before,
        "membership-only crossing writes no frame"
    );
    let live = live_visible_observed();
    let w1live = live
        .iter()
        .find(|w| w.token == "w1")
        .expect("floater stays visible");
    assert_eq!(
        w1live.visible, float_rect,
        "physical rectangle preserved across the logical crossing"
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(o2.clone(), target_b.clone())),
        "logical home still arrives"
    );
    // Next report settles on the logical receipt even though the frame
    // physically stays on the source output.
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "float crossing settles: {:?}",
        settled.reply.transfers
    );
}

#[test]
fn switch_view_two_entering_two_leaving_proves_only_when_complete() {
    let mut env = env_with(207_001);
    let out = env.state.output_token(DEV1);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let leaving1 = win_key(207_001, 2211, "proc-111");
    let leaving2 = win_key(207_002, 2212, "proc-112");
    let entering1 = win_key(207_003, 2213, "proc-113");
    let entering2 = win_key(207_004, 2214, "proc-114");
    admit_bare(&mut env, &leaving1, "a1");
    admit_bare(&mut env, &leaving2, "a2");
    admit_bare(&mut env, &entering1, "b1");
    admit_bare(&mut env, &entering2, "b2");
    env.state
        .applied_homing
        .insert("a1".to_owned(), (out.clone(), ws1.clone()));
    env.state
        .applied_homing
        .insert("a2".to_owned(), (out.clone(), ws1.clone()));
    env.state
        .applied_homing
        .insert("b1".to_owned(), (out.clone(), ws2.clone()));
    env.state
        .applied_homing
        .insert("b2".to_owned(), (out.clone(), ws2.clone()));
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![
        observed_for(&leaving1, "a1", frozen, plain_facts()),
        observed_for(&leaving2, "a2", rect(0, 610, 800, 400), plain_facts()),
    ];
    for key in [&leaving1, &leaving2, &entering1, &entering2] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(&key)));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(&key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(&key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    script_live(&leaving1, "a1", frozen, plain_facts());
    script_live(&leaving2, "a2", rect(0, 610, 800, 400), plain_facts());
    script_live(&entering1, "b1", frozen, plain_facts());
    script_live(&entering2, "b2", rect(0, 610, 800, 400), plain_facts());
    S_VISIBLE.with(|m| {
        m.borrow_mut().remove(&entering1.hwnd);
        m.borrow_mut().remove(&entering2.hwnd);
    });
    for key in [&entering1, &entering2] {
        env.state.hidden_claims.insert(
            key.clone(),
            super::super::HiddenRecord {
                claim: script_claim(key.hwnd, &identity_of(&key)),
                iconic: false,
            },
        );
    }
    let inputs = env.state.inputs();
    let captured = capture_world(&inputs, Some(&observed), &[], &one_area(), 207_001);
    let fresh = FreshHost::from_sources(&inputs, &observed, &[], &captured);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws1.as_str()), (out.as_str(), ws2.as_str())]);
    reply.outputs.push(tiler_core::world_report::OutputDesire {
        output: OutputId(out.clone()),
        logical_view: Some(ws2.clone()),
        tiling_mode: tiler_core::world_report::TilingMode::Tiled,
        hold: None,
    });
    reply.transfers.push(TransferTarget {
        operation_id: "op-view-2x2".to_owned(),
        kind: TransferKind::View,
        target_output: OutputId(out.clone()),
        target_workspace: ws2.clone(),
        follow: false,
        held: false,
    });
    // Full completion: both hide legs and both reveal legs land.
    let mut native = assemble_with_views(&mut env, moving_prims(), Some((out.clone(), ws2.clone())));
    native
        .core_rings
        .entry(out.clone())
        .or_default()
        .extend([ws1.clone(), ws2.clone()]);
    let (stats, feedback) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(
        feedback.iter().any(|f| f.operation_id == "op-view-2x2"
            && f.kind == AttemptKind::View
            && f.outcome == AttemptOutcome::Applied),
        "2+2 batch proves: {feedback:?}"
    );
    assert_eq!(stats.view_writes, 1, "{stats:?}");
    drop(native);
    // Partial completion: the second reveal refuses, so nothing proves.
    reset_script();
    for key in [&leaving1, &leaving2, &entering1, &entering2] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(&key)));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(&key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(&key))))
        });
    }
    S_REVEAL.with(|m| {
        m.borrow_mut()
            .insert(entering1.hwnd, Ok(Revealed::Shown))
    });
    S_REVEAL.with(|m| {
        m.borrow_mut()
            .insert(entering2.hwnd, Err("refused: scripted".to_owned()))
    });
    let mut env2 = env_with(207_011);
    let out2 = env2.state.output_token(DEV1);
    for (key, token, ws) in [
        (&leaving1, "a1", ws1.clone()),
        (&leaving2, "a2", ws1.clone()),
        (&entering1, "b1", ws2.clone()),
        (&entering2, "b2", ws2.clone()),
    ] {
        admit_bare(&mut env2, key, token);
        env2
            .state
            .applied_homing
            .insert(token.to_owned(), (out2.clone(), ws));
    }
    env2.state.applied_views.insert(out2.clone(), ws1.clone());
    script_live(&leaving1, "a1", frozen, plain_facts());
    script_live(&leaving2, "a2", rect(0, 610, 800, 400), plain_facts());
    script_live(&entering1, "b1", frozen, plain_facts());
    script_live(&entering2, "b2", rect(0, 610, 800, 400), plain_facts());
    S_VISIBLE.with(|m| {
        m.borrow_mut().remove(&entering1.hwnd);
        m.borrow_mut().remove(&entering2.hwnd);
    });
    for key in [&entering1, &entering2] {
        env2.state.hidden_claims.insert(
            key.clone(),
            super::super::HiddenRecord {
                claim: script_claim(key.hwnd, &identity_of(&key)),
                iconic: false,
            },
        );
    }
    let inputs = env2.state.inputs();
    let captured = capture_world(&inputs, Some(&observed), &[], &one_area(), 207_001);
    let fresh = FreshHost::from_sources(&inputs, &observed, &[], &captured);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    let mut reply = reply_shell(7, 1, &[(out2.as_str(), ws1.as_str()), (out2.as_str(), ws2.as_str())]);
    reply.outputs.push(tiler_core::world_report::OutputDesire {
        output: OutputId(out2.clone()),
        logical_view: Some(ws2.clone()),
        tiling_mode: tiler_core::world_report::TilingMode::Tiled,
        hold: None,
    });
    reply.transfers.push(TransferTarget {
        operation_id: "op-view-2x2-partial".to_owned(),
        kind: TransferKind::View,
        target_output: OutputId(out2.clone()),
        target_workspace: ws2.clone(),
        follow: false,
        held: false,
    });
    let mut native = assemble_with_views(
        &mut env2,
        moving_prims(),
        Some((out2.clone(), ws2.clone())),
    );
    native
        .core_rings
        .entry(out2.clone())
        .or_default()
        .extend([ws1.clone(), ws2.clone()]);
    let (_stats, feedback) = apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(
        feedback.iter().all(|f| f.outcome != AttemptOutcome::Applied),
        "partial batch proves nothing: {feedback:?}"
    );
    assert!(
        feedback.iter().any(|f| f.operation_id == "op-view-2x2-partial"
            && f.outcome == AttemptOutcome::PossiblyExecuted),
        "partial batch holds truthfully: {feedback:?}"
    );
}

#[test]
fn source_refill_migration_arrival_and_views_settle() {
    let mut env = env_with(208_001);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = two_output_areas();
    let a1 = win_key(208_001, 2211, "proc-121");
    let a2 = win_key(208_002, 2212, "proc-122");
    admit_bare(&mut env, &a1, "w1");
    admit_bare(&mut env, &a2, "w2");
    script_live(&a1, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&a2, "w2", rect(0, 610, 800, 400), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let ws_m = owner
        .engine()
        .logical_view(&o1)
        .expect("migrated workspace");
    // Migrate the whole workspace o1 -> o2 through the real command.
    let observed = live_visible_observed();
    let moved = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId(o1.clone()),
            target_output: OutputId(o2.clone()),
            workspace: ws_m.clone(),
        }),
    );
    let migration_op = moved
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration retained")
        .operation_id
        .clone();
    let refill_op = moved
        .reply
        .transfers
        .iter()
        .find(|t| t.operation_id == format!("{migration_op}-refill"))
        .expect("source refill retained")
        .operation_id
        .clone();
    // Tick 1 actuates member arrival; the source refill view cannot prove
    // until every member arrival is proven, so the migration holds.
    assert!(
        moved.feedback.iter().any(|f| f.operation_id == migration_op
            && f.outcome == AttemptOutcome::PossiblyExecuted),
        "migration holds until views prove: {:?}",
        moved.feedback
    );
    for token in ["w1", "w2"] {
        assert_eq!(
            env.state.applied_homing.get(token),
            Some(&(o2.clone(), ws_m.clone())),
            "{token} arrival proves"
        );
    }
    // Tick 2 proves the refill view behind the arrived members.
    let observed = live_visible_observed();
    let proved = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        proved
            .feedback
            .iter()
            .any(|f| f.operation_id == migration_op && f.outcome == AttemptOutcome::Applied),
        "migration applies: {:?}",
        proved.feedback
    );
    assert!(
        proved
            .feedback
            .iter()
            .any(|f| f.operation_id == refill_op && f.outcome == AttemptOutcome::Applied),
        "refill applies: {:?}",
        proved.feedback
    );
    // Tick 3 settles: no migration flight survives proven arrival.
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled
            .reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "migration settles: {:?}",
        settled.reply.transfers
    );
}

// ---------------------------------------------------------------------------
// Stage 2b admission/startup/restart truth hardening: named regressions for
// the deleted Windows accepted fixed/admission/hint/first-exit/restart
// coverage (producer-shaped hints, recycled-HWND retirement, born capture,
// minimized/hidden truth, marker restore, first exit, token-scoped forget).
// ---------------------------------------------------------------------------

#[test]
fn world_fixed_hint_producer_shapes_fixed_admission() {
    // Producer-shaped hints: one bounded `WM_GETMINMAXINFO` query per bound
    // visible member stores into `hint_logged` and rides the captured fact,
    // so fixed-size admission (min=max pinned) classifies without invention.
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let key = win_key(301_001, 3201, "proc-301");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(key.clone(), identity_of(&key));
    let pinned = tiler_core::size_hints::WindowSizeHints {
        min_w: Some(400),
        min_h: Some(300),
        max_w: Some(400),
        max_h: Some(300),
    };
    super::super::inject_test_hints(301_001, pinned);
    let observed = vec![observed_for(&key, "w1", rect(0, 0, 800, 600), plain_facts())];
    let areas = one_area();
    let reply = super::super::world_tick::dispatch_test_observation(&mut state, &observed, &[], &areas, 0)
        .expect("dispatch converges");
    super::super::clear_test_hints();
    assert_eq!(
        state.hint_logged.get("w1"),
        Some(&pinned),
        "producer syncs hint_logged, got {:?}",
        state.hint_logged
    );
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 0);
    let fact = captured.windows.iter().find(|f| f.window.0 == "w1").expect("w1 fact");
    assert_eq!(fact.hints, pinned, "captured fact carries producer hints");
    assert!(reply.ready, "fixed admission still converges");
    assert!(
        state.hint_fallback_uses.get("w1").is_some_and(|use_| !use_.reused),
        "fresh query reports fresh, got {:?}",
        state.hint_fallback_uses.get("w1")
    );
}

#[test]
fn world_recycled_hwnd_stale_holds_without_dropping_live() {
    // Recycled HWND: the live lifetime rides its row while the stale
    // same-HWND lifetime holds as unreadable (never a fabricated row, never
    // a close, never a dropped live claim).
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let live = win_key(302_001, 401, "proc-live");
    let stale = win_key(302_001, 402, "proc-stale");
    for (key, token) in [(&live, "w-live"), (&stale, "w-stale")] {
        state.member_tokens.insert(key.clone(), token.to_owned());
        state.member_tags.insert(key.clone(), "member-tag-1".to_owned());
        state.member_identity.insert(key.clone(), identity_of(key));
    }
    let observed = vec![observed_for(&live, "w-live", rect(0, 0, 800, 600), plain_facts())];
    let areas = one_area();
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 0);
    let live_fact = captured.windows.iter().find(|f| f.window.0 == "w-live").expect("live fact");
    assert!(live_fact.rect.is_some(), "live lifetime rides its row");
    let stale_fact = captured.windows.iter().find(|f| f.window.0 == "w-stale").expect("stale fact");
    assert!(stale_fact.rect.is_none() && !stale_fact.minimized, "stale lifetime holds unreadable");
    assert!(captured.closed.is_empty(), "omission is never a close: {:?}", captured.closed);
}

#[test]
fn world_forget_member_clears_token_scoped_caches() {
    // Causal stale cleanup: forgetting a member drops every token-scoped
    // cache (tile slot, float frame, hints, fallback, overlay) with the
    // token, not a retain-true no-op. Unknown keys report false.
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let key = win_key(303_001, 501, "proc-303");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(key.clone(), identity_of(&key));
    state.member_rects.insert("w1".to_owned(), rect(0, 0, 100, 100));
    state.float_rects.insert("w1".to_owned(), rect(0, 0, 100, 100));
    state.hint_logged.insert("w1".to_owned(), tiler_core::size_hints::WindowSizeHints::none());
    state.overlay_now.insert("w1".to_owned(), (false, false));
    assert!(state.forget_member(&key), "known member reports true");
    assert!(!state.member_tokens.contains_key(&key));
    assert!(!state.member_rects.contains_key("w1"), "tile slot clears");
    assert!(!state.float_rects.contains_key("w1"), "float frame clears");
    assert!(!state.hint_logged.contains_key("w1"), "hints clear");
    assert!(!state.overlay_now.contains_key("w1"), "overlay clears");
    let unknown = win_key(303_009, 599, "proc-unknown");
    assert!(!state.forget_member(&unknown), "unknown reports false");
}

#[test]
fn world_born_fullscreen_captures_quiet_with_slot_truth() {
    // Newly observed born fullscreen: floating fullscreen fact on the live
    // frame with no invented tile slot, quiet first admission (no close, no
    // restoration ingress).
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let key = win_key(304_001, 601, "proc-304");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(key.clone(), identity_of(&key));
    state.born_fullscreen.insert(key.clone());
    let mut facts = plain_facts();
    facts.captionless_fullscreen = true;
    let observed = vec![observed_for(&key, "w1", rect(0, 0, 800, 600), facts)];
    let areas = one_area();
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 304_001);
    let fact = captured.windows.iter().find(|f| f.window.0 == "w1").expect("w1 fact");
    assert!(fact.floating && fact.fullscreen, "born hold floats fullscreen: {fact:?}");
    assert_eq!(fact.rect, Some(rect(0, 0, 800, 600)), "live frame rides");
    assert!(captured.closed.is_empty(), "quiet admission: {:?}", captured.closed);
    assert!(captured.restored.is_none(), "no restoration ingress");
}

#[test]
fn world_minimized_omission_never_closes_hidden_keeps_fullscreen() {
    // Minimized omission rides a minimized fact (never a close); a hidden
    // ordinary fullscreen keeps its fullscreen truth through the retained
    // overlay flags (durable preimage carries maximized only).
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let min_key = win_key(305_001, 701, "proc-min");
    state.member_tokens.insert(min_key.clone(), "w-min".to_owned());
    state.member_tags.insert(min_key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(min_key.clone(), identity_of(&min_key));
    state.member_rects.insert("w-min".to_owned(), rect(0, 0, 400, 300));
    state.minimized_omitted.insert(min_key.clone());
    let hid_key = win_key(305_002, 702, "proc-hid");
    state.member_tokens.insert(hid_key.clone(), "w-hid".to_owned());
    state.member_tags.insert(hid_key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(hid_key.clone(), identity_of(&hid_key));
    state.member_rects.insert("w-hid".to_owned(), rect(0, 0, 800, 600));
    let mut claim = valid_claim(305_002, &identity_of(&hid_key));
    claim.show = WindowShowState { minimized: false, maximized: false };
    state.hidden_claims.insert(
        hid_key.clone(),
        super::super::HiddenRecord { claim, iconic: false },
    );
    let retained = vec![super::super::RetainedRow {
        key: hid_key.clone(),
        token: "w-hid".to_owned(),
        rect: Some(rect(0, 0, 800, 600)),
        maximized: false,
        fullscreen: true,
        facts: Some(plain_facts()),
    }];
    let areas = one_area();
    let captured = capture_world(&state.inputs(), Some(&[]), &retained, &areas, 0);
    let min_fact = captured.windows.iter().find(|f| f.window.0 == "w-min").expect("min fact");
    assert!(min_fact.minimized && min_fact.rect.is_none(), "minimized rides: {min_fact:?}");
    let hid_fact = captured.windows.iter().find(|f| f.window.0 == "w-hid").expect("hidden fact");
    assert!(hid_fact.fullscreen, "hidden keeps fullscreen truth: {hid_fact:?}");
    assert!(!hid_fact.minimized, "hidden snapshot is not minimized");
    assert!(captured.closed.is_empty(), "omission is never a close: {:?}", captured.closed);
}

#[test]
fn world_marker_intent_restores_quiet_without_receipts() {
    // Fresh-owner marker restore: staged float/sticky lanes ride floating
    // facts on the first observation with no fabricated restoration and no
    // proven receipts (quiet bootstrap).
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let float_key = win_key(306_001, 801, "proc-float");
    let sticky_key = win_key(306_002, 802, "proc-sticky");
    for (key, token) in [(&float_key, "w-float"), (&sticky_key, "w-sticky")] {
        state.member_tokens.insert(key.clone(), token.to_owned());
        state.member_tags.insert(key.clone(), "member-tag-1".to_owned());
        state.member_identity.insert(key.clone(), identity_of(key));
        state.float_rects.insert(token.to_owned(), rect(10, 10, 400, 300));
    }
    state.floated.insert(float_key.clone());
    state.sticky.insert(sticky_key.clone(), false);
    let observed = vec![
        observed_for(&float_key, "w-float", rect(10, 10, 400, 300), plain_facts()),
        observed_for(&sticky_key, "w-sticky", rect(10, 10, 400, 300), plain_facts()),
    ];
    let areas = one_area();
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 0);
    for token in ["w-float", "w-sticky"] {
        let fact = captured.windows.iter().find(|f| f.window.0 == token).expect("fact");
        assert!(fact.floating, "{token} restores floating: {fact:?}");
    }
    assert!(captured.restored.is_none(), "no restoration ingress");
    assert!(captured.closed.is_empty(), "quiet: {:?}", captured.closed);
    assert!(
        captured.observed_views.is_empty(),
        "marker-only restore proves no view: {:?}",
        captured.observed_views
    );
}

#[test]
fn world_first_exit_tiles_quietly() {
    // First born-fullscreen exit with non-fixed hints tiles on the fresh
    // query output (floating false), carrying exactly the fresh hints.
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let key = win_key(307_001, 901, "proc-exit");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(key.clone(), identity_of(&key));
    state.born_fullscreen.insert(key.clone());
    state.born_first_exit.insert(key.clone());
    super::super::inject_test_hints(307_001, tiler_core::size_hints::WindowSizeHints::none());
    let observed = vec![observed_for(&key, "w1", rect(0, 0, 800, 600), plain_facts())];
    let areas = one_area();
    let reply = super::super::world_tick::dispatch_test_observation(&mut state, &observed, &[], &areas, 0)
        .expect("dispatch converges");
    super::super::clear_test_hints();
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 0);
    let fact = captured.windows.iter().find(|f| f.window.0 == "w1").expect("w1 fact");
    assert!(!fact.floating, "first exit tiles: {fact:?}");
    assert!(fact.hints.is_empty(), "fresh empty hints ride: {:?}", fact.hints);
    assert!(reply.ready);
}
