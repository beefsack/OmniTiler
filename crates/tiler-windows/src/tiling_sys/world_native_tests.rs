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
use super::super::world_native_receipts::observed_view_receipts;
use super::super::world_owner::WorldOwner;
use super::super::world_sequencer::AckOutcome;
use super::super::world_tick::merge_applied_transfers;
use super::{AdmitOutcome, LiveNative, NativePrimitives, Revealed};
use crate::model::{ProcessIdentity, WindowClaimKind, WindowIdentity, WindowShowState};
use crate::native_guards::WindowKey;
use crate::storage::LedgerStore;
use crate::tiling::{FrameInsets, ObservedTarget, WindowFacts};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report::{
    ScopeAuthority as Auth, ScopeStatus, TransferKind, TransferTarget, WORLD_CONTRACT_VERSION,
    WindowDesire, WorldCommand, WorldReply,
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
            // Generic fixture-style identity: the proof-only owned-helper
            // tag is empty (never carried), exactly like the live fixture.
            // Managed membership rides `member_tag` below, never this lane.
            tag: String::new(),
        },
        facts,
        // Fresh visible-membership stamp, read independently of the proof
        // tag above (mirrors `observe_window`'s separate prop read).
        member_tag: Some("member-tag-1".to_owned()),
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
    static S_FG: RefCell<u64> = const { RefCell::new(0) };
    static S_FOCUS_OK: RefCell<HashMap<u64, bool>> = RefCell::new(HashMap::new());
    static S_TAG: RefCell<HashMap<u64, String>> = RefCell::new(HashMap::new());
    static S_HOOK: RefCell<bool> = const { RefCell::new(false) };
    static S_IDENT: RefCell<HashMap<u64, ProcessIdentity>> = RefCell::new(HashMap::new());
    /// Scripted per-window DPI (`None` = unreadable). Reset per test; the
    /// cross-DPI fake flips this on a monitor-changing write.
    static S_DPI: RefCell<HashMap<u64, u32>> = RefCell::new(HashMap::new());
    /// Scripted monitor DPI map: `(full, dpi)` entries the cross-DPI fake
    /// uses to resolve a written outer rect to its target DPI.
    static S_MONITORS: RefCell<Vec<(Rect, u32)>> = const { RefCell::new(Vec::new()) };
    /// Scripted racing game HWND for the veto test: when set, the
    /// cross-DPI fake raises a fullscreen foreground mid-call.
    static S_GAME_FG: RefCell<Option<u64>> = const { RefCell::new(None) };
    static S_ADMIT: RefCell<HashMap<u64, Result<WindowIdentity, AdmitOutcome>>> =
        RefCell::new(HashMap::new());
    static S_HIDE: RefCell<HashMap<u64, Result<WindowIdentity, String>>> =
        RefCell::new(HashMap::new());
    static S_REVEAL: RefCell<HashMap<u64, Result<Revealed, String>>> =
        RefCell::new(HashMap::new());
    static S_ADMIT_CALLS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// Scripted live cursor position (`None` = unreadable). Warp writes
    /// update it so re-reads observe the move, like the real position.
    static S_CURSOR: RefCell<Option<(i32, i32)>> = const { RefCell::new(None) };
    /// Every scripted warp write attempted, in order.
    static S_WARP_CALLS: RefCell<Vec<(i32, i32)>> = const { RefCell::new(Vec::new()) };
    /// Scripted warp refusals for the offline lane.
    static S_FAIL_WARP: RefCell<bool> = const { RefCell::new(false) };
}

fn reset_script() {
    S_OBSERVE.with(|m| m.borrow_mut().clear());
    S_PID.with(|m| m.borrow_mut().clear());
    S_VISIBLE.with(|m| m.borrow_mut().clear());
    S_ICONIC.with(|m| m.borrow_mut().clear());
    S_SET_OK.with(|m| m.borrow_mut().clear());
    S_SET_CALLS.with(|m| m.borrow_mut().clear());
    S_FG.with(|m| *m.borrow_mut() = 0);
    S_FOCUS_OK.with(|m| m.borrow_mut().clear());
    S_TAG.with(|m| m.borrow_mut().clear());
    S_HOOK.with(|m| *m.borrow_mut() = false);
    S_IDENT.with(|m| m.borrow_mut().clear());
    S_ADMIT.with(|m| m.borrow_mut().clear());
    S_HIDE.with(|m| m.borrow_mut().clear());
    S_REVEAL.with(|m| m.borrow_mut().clear());
    S_ADMIT_CALLS.with(|m| m.borrow_mut().clear());
    S_DPI.with(|m| m.borrow_mut().clear());
    S_MONITORS.with(|m| m.borrow_mut().clear());
    S_GAME_FG.with(|m| *m.borrow_mut() = None);
    S_CURSOR.with(|m| *m.borrow_mut() = None);
    S_WARP_CALLS.with(|m| m.borrow_mut().clear());
    S_FAIL_WARP.with(|m| *m.borrow_mut() = false);
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

fn s_dpi(hwnd: u64) -> Option<u32> {
    S_DPI.with(|m| m.borrow().get(&hwnd).copied())
}

fn scripted() -> NativePrimitives {
    NativePrimitives {
        observe: s_observe,
        window_pid: |hwnd| S_PID.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(0)),
        is_visible: |hwnd| S_VISIBLE.with(|m| m.borrow().contains(&hwnd)),
        is_iconic: |hwnd| S_ICONIC.with(|m| m.borrow().contains(&hwnd)),
        set_outer: |hwnd, outer| {
            S_SET_CALLS.with(|m| m.borrow_mut().push((hwnd, outer)));
            S_SET_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
        },
        foreground: || S_FG.with(|m| *m.borrow()),
        focus: |hwnd| S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true)),
        member_tag: |hwnd| S_TAG.with(|m| m.borrow().get(&hwnd).cloned()),
        hook_pending: || S_HOOK.with(|m| *m.borrow()),
        read_cursor: || S_CURSOR.with(|m| *m.borrow()),
        warp_cursor: |x, y| {
            S_WARP_CALLS.with(|m| m.borrow_mut().push((x, y)));
            if S_FAIL_WARP.with(|m| *m.borrow()) {
                return false;
            }
            S_CURSOR.with(|m| *m.borrow_mut() = Some((x, y)));
            true
        },
        dpi: s_dpi,
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
        gesture_settled: None,
        operation_settled: Vec::new(),
        native_creations: Vec::new(),
        cursor_warp: None,
        presentation: tiler_core::world_presentation::WorldPresentation::default(),
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
        clear_operation: None,
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

#[test]
fn production_setter_lie_reports_raw_attempt_without_verdict() {
    let mut env = env_with(101_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(101_001, 2001, "proc-1");
    admit_member(&mut env, &key, &ws, "w1", false);
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![observed_for(&key, "w1", frozen, plain_facts())];
    script_live(&key, "w1", frozen, plain_facts());
    // Setter accepts but the frame never moves: the app held its size.
    // The seam reports the mechanical attempt with exact operation
    // correlation and no outcome verdict; the core settles it.
    S_SET_OK.with(|m| m.borrow_mut().insert(key.hwnd, true));
    let (mut diff, fresh) = convert(&env, &observed);
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    reply
        .windows
        .push(float_row(&out, &ws, "w1", rect(100, 100, 400, 300), "op-1"));
    let mut native = assemble(&mut env, scripted());
    let mut attempts = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut attempts,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    let attempt = &attempts[0];
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.target, rect(100, 100, 400, 300));
    assert_eq!(
        attempt.setter,
        tiler_core::world_report::FrameSetter::Accepted
    );
    assert_eq!(attempt.operation_id.as_deref(), Some("op-1"));
    assert_eq!(attempt.prewrite, Some(frozen));
    assert_eq!(attempt.readback, Some(frozen));
    assert_eq!(stats.geometry_writes, 1);
    assert!(stats.held > 0);
    assert_eq!(S_SET_CALLS.with(|m| m.borrow().len()), 1);
}

#[test]
fn production_explicit_refusal_reports_raw_attempt_without_verdict() {
    let mut env = env_with(102_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(102_001, 2002, "proc-2");
    admit_member(&mut env, &key, &ws, "w1", false);
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![observed_for(&key, "w1", frozen, plain_facts())];
    script_live(&key, "w1", frozen, plain_facts());
    // Explicit refusal with an unchanged readable preimage: the seam
    // reports setter, preimage, and readback with no verdict; the core
    // derives the proven refusal from that evidence.
    S_SET_OK.with(|m| m.borrow_mut().insert(key.hwnd, false));
    let (mut diff, fresh) = convert(&env, &observed);
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    reply
        .windows
        .push(float_row(&out, &ws, "w1", rect(100, 100, 400, 300), "op-1"));
    let mut native = assemble(&mut env, scripted());
    let mut attempts = Vec::new();
    let _stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut attempts,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    let attempt = &attempts[0];
    assert_eq!(
        attempt.setter,
        tiler_core::world_report::FrameSetter::Refused
    );
    assert_eq!(attempt.operation_id.as_deref(), Some("op-1"));
    assert_eq!(attempt.prewrite, Some(frozen));
    assert_eq!(attempt.readback, Some(frozen));
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
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
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
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
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
    let mut first_members = Vec::new();
    let mut first_views = Vec::new();
    let first_stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut first_members,
        &mut first_views,
        &mut Vec::new(),
        0,
    );
    assert_eq!(first_stats.held, 1);
    // The uncertain hide leg reports one ambiguous view attempt (never a
    // verdict): the core holds from the raw setter plus unreadable proof.
    assert!(first_members.is_empty());
    assert_eq!(first_views.len(), 1);
    assert_eq!(first_views[0].operation_id, "op-view-1");
    assert_eq!(
        first_views[0].setter,
        tiler_core::world_report::FrameSetter::Threw
    );
    assert_eq!(S_ADMIT_CALLS.with(|m| m.borrow().len()), 1);
    // ... and the retry never re-dispatches: the spent phase holds
    // silently (no new admit, no new attempt) because no fresh proof
    // retires the ambiguous flight. The live source view cannot prove
    // itself against the desired target here: `read_view` only ever proves
    // the desired view or nothing, so the flight persists until a later
    // pass proves arrival. No proof row is fabricated either way; the
    // admit call count is frozen.
    let mut second_members = Vec::new();
    let mut second_views = Vec::new();
    let second_stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut second_members,
        &mut second_views,
        &mut Vec::new(),
        0,
    );
    assert!(second_stats.held > 0);
    assert!(second_members.is_empty() && second_views.is_empty());
    assert_eq!(S_ADMIT_CALLS.with(|m| m.borrow().len()), 1);
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
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(members.is_empty() && views.is_empty());
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
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    // A memberless migration and refill... (no members anywhere: the
    // migration carries no membership phase) reports one raw target-view
    // attempt and actuates it; the core settles from the attempt plus the
    // staged echo.
    assert!(members.is_empty());
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].operation_id, "op-m1");
    assert_eq!(views[0].post.as_deref(), Some(ws2.as_str()));
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

/// Membership receipts ride the fresh visible-membership stamp only: a
/// nonempty proof-only helper token proves nothing, and a missing or
/// mismatched stamp holds unknown. The generic empty-helper happy path
/// (stamped member lane, empty proof lane) still proves.
#[test]
fn receipts_require_member_stamp_never_helper_tag() {
    fn rig(
        member_tag: Option<String>,
        helper_tag: &str,
    ) -> (Option<String>, Vec<tiler_core::world_report::ScreenView>) {
        reset_script();
        let mut state = test_state();
        state.ensure_device(DEV1);
        let ws = "ws1".to_owned();
        let key = win_key(109_011, 2013, "proc-13");
        state.member_tokens.insert(key.clone(), "w1".to_owned());
        state
            .member_tags
            .insert(key.clone(), "member-tag-1".to_owned());
        state.member_identity.insert(key.clone(), identity_of(&key));
        let out = state.output_token(DEV1);
        state
            .applied_homing
            .insert("w1".to_owned(), (out.clone(), ws.clone()));
        state.applied_views.insert(out, ws.clone());
        let mut row = observed_for(&key, "w1", rect(0, 0, 800, 600), plain_facts());
        row.identity.tag = helper_tag.to_owned();
        row.member_tag = member_tag;
        let areas = one_area();
        let inputs = state.inputs();
        let captured = capture_world(
            &inputs,
            Some(std::slice::from_ref(&row)),
            &[],
            &areas,
            109_011,
        );
        let views = observed_view_receipts(&inputs, std::slice::from_ref(&row), &areas);
        let workspace = captured
            .windows
            .iter()
            .find(|f| f.window.0 == "w1")
            .expect("w1 fact")
            .observed_workspace
            .clone();
        (workspace, views)
    }
    // Generic happy path: empty helper tag, stamped member lane.
    let (workspace, views) = rig(Some("member-tag-1".to_owned()), "");
    assert_eq!(workspace.as_deref(), Some("ws1"));
    assert_eq!(views.len(), 1, "{views:?}");
    assert_eq!(views[0].view, "ws1");
    // Nonempty helper token with no member stamp proves nothing.
    let (workspace, views) = rig(None, "helper-1");
    assert_eq!(workspace, None);
    assert!(views.is_empty(), "{views:?}");
    // Mismatched member stamp holds unknown.
    let (workspace, views) = rig(Some("stale-tag".to_owned()), "");
    assert_eq!(workspace, None);
    assert!(views.is_empty(), "{views:?}");
    // Missing member stamp holds unknown even with a helper token present.
    let (workspace, views) = rig(None, "");
    assert_eq!(workspace, None);
    assert!(views.is_empty(), "{views:?}");
}

/// Empty-refill view receipts with hidden homing: a memberless applied
/// view still carrying hidden homing proves only through the same fresh
/// claim association the membership receipts require, with zero managed
/// presence on the output. Rig: one member homed on the output's other
/// workspace, applied view naming the fresh refill, plus caller-supplied
/// observed rows (the hidden member itself never enumerates).
fn empty_refill_views(
    claim: Option<WindowIdentity>,
    member_tag: Option<&str>,
    minimized_omitted: bool,
    observed_extra: Vec<super::super::ObservedWindow>,
) -> Vec<tiler_core::world_report::ScreenView> {
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let refill = "ws-9".to_owned();
    let other = "ws-3".to_owned();
    let key = win_key(109_021, 2021, "proc-14");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    if let Some(tag) = member_tag {
        state.member_tags.insert(key.clone(), tag.to_owned());
    }
    state.member_identity.insert(key.clone(), identity_of(&key));
    if minimized_omitted {
        state.minimized_omitted.insert(key.clone());
    }
    let out = state.output_token(DEV1);
    state
        .applied_homing
        .insert("w1".to_owned(), (out.clone(), other.clone()));
    state.applied_views.insert(out, refill.clone());
    if let Some(claim) = claim {
        state.hidden_claims.insert(
            key.clone(),
            super::super::HiddenRecord {
                claim,
                iconic: false,
            },
        );
    }
    let areas = one_area();
    let inputs = state.inputs();
    observed_view_receipts(&inputs, &observed_extra, &areas)
}

#[test]
fn empty_refill_receipt_proves_with_associated_hidden_homing() {
    let key = win_key(109_021, 2021, "proc-14");
    let views = empty_refill_views(
        Some(valid_claim(key.hwnd, &identity_of(&key))),
        Some("member-tag-1"),
        false,
        Vec::new(),
    );
    assert_eq!(views.len(), 1, "{views:?}");
    assert_eq!(views[0].view, "ws-9");
}

#[test]
fn empty_refill_receipt_withholds_on_malformed_claim_tag() {
    // `script_claim` tags ("nonce-<hwnd>") fail `valid_claim_tag`: a bare
    // hide-ledger entry never proves deliberate hiding.
    let key = win_key(109_021, 2021, "proc-14");
    let views = empty_refill_views(
        Some(script_claim(key.hwnd, &identity_of(&key))),
        Some("member-tag-1"),
        false,
        Vec::new(),
    );
    assert!(views.is_empty(), "{views:?}");
}

#[test]
fn empty_refill_receipt_withholds_on_claim_identity_mismatch() {
    // Claim bound to another lifetime (wrong HWND/PID/creation): stale
    // association proves nothing about this member.
    let other = win_key(109_099, 2099, "proc-99");
    let views = empty_refill_views(
        Some(valid_claim(other.hwnd, &identity_of(&other))),
        Some("member-tag-1"),
        false,
        Vec::new(),
    );
    assert!(views.is_empty(), "{views:?}");
}

#[test]
fn empty_refill_receipt_withholds_without_member_tag() {
    let key = win_key(109_021, 2021, "proc-14");
    let views = empty_refill_views(
        Some(valid_claim(key.hwnd, &identity_of(&key))),
        None,
        false,
        Vec::new(),
    );
    assert!(views.is_empty(), "{views:?}");
}

#[test]
fn empty_refill_receipt_withholds_on_drifted_hidden_window() {
    // The hidden member enumerates live (external reveal or HWND drift):
    // continued invisibility fails, so the empty screen proves nothing.
    // The drift row carries a stale stamp so it cannot enter the visible
    // set; only the drift check withholds.
    let key = win_key(109_021, 2021, "proc-14");
    let mut drifted = observed_for(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    drifted.member_tag = None;
    let views = empty_refill_views(
        Some(valid_claim(key.hwnd, &identity_of(&key))),
        Some("member-tag-1"),
        false,
        vec![drifted],
    );
    assert!(views.is_empty(), "{views:?}");
}

#[test]
fn empty_refill_receipt_withholds_on_visible_capable_homing() {
    // Homing with no hidden claim at all: the member could be visibly
    // showing its workspace, so the refill view stays unknown.
    let views = empty_refill_views(None, Some("member-tag-1"), false, Vec::new());
    assert!(views.is_empty(), "{views:?}");
}

#[test]
fn empty_refill_receipt_withholds_on_minimized_homing() {
    // Minimized homing is not hidden-claim hiding: without the fresh
    // claim association the refill view stays unknown, preserving the
    // existing minimized semantics.
    let views = empty_refill_views(None, Some("member-tag-1"), true, Vec::new());
    assert!(views.is_empty(), "{views:?}");
}

#[test]
fn empty_refill_receipt_ignores_minimized_foreign_presence() {
    // Minimized rows never count as on-output presence (matching the
    // visible set): a minimized foreign window does not fence the proof.
    let fkey = win_key(109_031, 2031, "proc-15");
    let mut row = observed_for(&fkey, "fx", rect(0, 0, 800, 600), plain_facts());
    row.facts.minimized = true;
    let key = win_key(109_021, 2021, "proc-14");
    let views = empty_refill_views(
        Some(valid_claim(key.hwnd, &identity_of(&key))),
        Some("member-tag-1"),
        false,
        vec![row],
    );
    assert_eq!(views.len(), 1, "{views:?}");
    assert_eq!(views[0].view, "ws-9");
}

#[test]
fn empty_refill_receipt_ignores_foreign_visible_presence() {
    // Scoped inventory ignores foreign never-member windows (capture
    // never admits them): a raw foreign window visibly on the output
    // never fences the managed empty-screen proof, so a legitimately
    // scoped-empty output cannot pin flights on foreign absence.
    let fkey = win_key(109_031, 2031, "proc-15");
    let row = observed_for(&fkey, "fx", rect(0, 0, 800, 600), plain_facts());
    let key = win_key(109_021, 2021, "proc-14");
    let views = empty_refill_views(
        Some(valid_claim(key.hwnd, &identity_of(&key))),
        Some("member-tag-1"),
        false,
        vec![row],
    );
    assert_eq!(views.len(), 1, "{views:?}");
    assert_eq!(views[0].view, "ws-9");
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
    let mut native = assemble_with_views(&mut env, scripted(), Some((out.clone(), ws2.clone())));
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
    let mut native = assemble_with_views(&mut env, scripted(), Some((out.clone(), ws2.clone())));
    let mut reply = reply_shell(
        7,
        1,
        &[(out.as_str(), ws1.as_str()), (out.as_str(), ws2.as_str())],
    );
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
        clear_operation: None,
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
    let mut native = assemble_with_views(&mut env, scripted(), Some((out.clone(), ws2.clone())));
    let mut reply = reply_shell(
        7,
        1,
        &[(out.as_str(), ws1.as_str()), (out.as_str(), ws2.as_str())],
    );
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
            clear_operation: None,
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
    let observed = vec![observed_for(
        &key,
        "w1",
        rect(0, 0, 800, 600),
        plain_facts(),
    )];
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
    let mut out: Vec<super::super::ObservedWindow> = S_OBSERVE.with(|m| {
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
    members: Vec<tiler_core::world_report::MemberAttempt>,
    views: Vec<tiler_core::world_report::ViewAttempt>,
}

/// One production tick through the real owner with scripted natives:
/// dispatch, deliver, diff-apply, receipt, merge with independent fresh
/// revalidation, raw attempt staging, and acknowledgement. Mirrors
/// `run_observation_tick`/`run_world_command` with injectable prims.
/// Cursor-absent: every pre-existing caller keeps this shape.
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
    live_tick_cursor(
        env, owner, observed, retained, areas, fulls, prims, foreground, command, None,
    )
}

// Deterministic harness monotonic clock: one 1000ms sample per production
// tick, thread-local so parallel tests never share a stamp. Mirrors the
// production one-sample-per-tick contract without wall-clock flakiness;
// transient backoff (500ms base) always expires between ticks here.
thread_local! {
    static LIVE_TICK_MS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn live_tick_ms() -> u64 {
    LIVE_TICK_MS.with(|slot| {
        let next = slot.get().saturating_add(1000);
        slot.set(next);
        next
    })
}

/// Cursor-carrying sibling of [`live_tick`]: the sampled cursor rides the
/// owner inputs so core cursor-first policy (fresh placement, switch
/// target) resolves exactly like production. Old callers stay on
/// [`live_tick`]; cursor cases use this entry point.
#[allow(clippy::too_many_arguments)]
fn live_tick_cursor(
    env: &mut Env,
    owner: &mut WorldOwner,
    observed: &[super::super::ObservedWindow],
    retained: &[super::super::RetainedRow],
    areas: &[super::super::MonitorArea],
    fulls: &[Rect],
    prims: NativePrimitives,
    foreground: u64,
    command: Option<WorldCommand>,
    cursor: Option<(i32, i32)>,
) -> LiveTick {
    // Production fidelity: snapshot last-known frames before dispatch so
    // hidden members keep riding snapshot rectangles (readable domains),
    // exactly like `run_observation_tick`/`run_world_command`.
    super::super::world_tick::snapshot_frames(&mut env.state, observed);
    owner.invalidate();
    // One deterministic monotonic sample for this tick: the report
    // observation time and the stamp for any frame attempt below.
    let tick_ms = live_tick_ms();
    let mut inputs = env.state.inputs();
    inputs.cursor = cursor;
    let report = match command {
        None => owner
            .try_dispatch_observation(
                &inputs,
                Some(observed),
                retained,
                areas,
                foreground,
                tick_ms,
            )
            .expect("dispatch observation"),
        Some(cmd) => owner
            .try_dispatch_command(
                &inputs,
                Some(observed),
                retained,
                areas,
                foreground,
                tick_ms,
                cmd,
            )
            .expect("dispatch command"),
    };
    let reply = match owner.deliver_production(&report) {
        WorldOutcome::Accepted(reply)
        | WorldOutcome::Duplicate(reply)
        | WorldOutcome::CommandConsumed(reply) => reply,
        WorldOutcome::Rejected { kind, message } => panic!("production rejected {kind}: {message}"),
    };
    let mut inputs = env.state.inputs();
    inputs.cursor = cursor;
    let captured = capture_world(&inputs, Some(observed), retained, areas, foreground);
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
    let mut frame_attempts = Vec::new();
    let mut member_attempts = Vec::new();
    let mut view_attempts = Vec::new();
    let mut clear_attempts = Vec::new();
    let _stats = owner.apply_accepted(
        &reply,
        &fresh,
        &mut native,
        &mut frame_attempts,
        &mut member_attempts,
        &mut view_attempts,
        &mut clear_attempts,
        tick_ms,
    );
    let (mut proven_h, mut proven_v) = native.receipt_updates(&reply);
    merge_applied_transfers(
        &reply,
        &member_attempts,
        &view_attempts,
        &fresh,
        &mut native,
        &mut proven_h,
        &mut proven_v,
    );
    drop(native);
    for (token, home) in proven_h {
        env.state.applied_homing.insert(token, home);
    }
    for (output, view) in proven_v {
        env.state.applied_views.insert(output, view);
    }
    for attempt in &member_attempts {
        owner.note_member_attempt(attempt.clone());
    }
    for attempt in &view_attempts {
        owner.note_view_attempt(attempt.clone());
    }
    for attempt in &frame_attempts {
        owner.note_frame_attempt(attempt.clone());
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
        members: member_attempts,
        views: view_attempts,
    }
}

/// True when the raw attempts carry post proof at the target for one
/// operation: the native proof the core settles from.
fn applied_of_members(
    members: &[tiler_core::world_report::MemberAttempt],
    views: &[tiler_core::world_report::ViewAttempt],
    op: &str,
) -> bool {
    members.iter().any(|a| {
        a.operation_id == op
            && a.post
                .as_ref()
                .is_some_and(|(out, ws)| out == &a.target_output && ws == &a.target_workspace)
    }) || views
        .iter()
        .any(|a| a.operation_id == op && a.post.as_deref() == Some(a.target_workspace.as_str()))
}

fn window_op(reply: &WorldReply, window: &str) -> String {
    reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == window))
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

const DEV2: &str = "DEV2";

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

fn set_calls() -> usize {
    S_SET_CALLS.with(|m| m.borrow().len())
}

#[path = "world_native_composition_tests.rs"]
mod composition;
#[path = "world_native_dpi_tests.rs"]
mod dpi_cases;

#[path = "world_native_cursor_tests.rs"]
mod cursor_cases;

#[path = "world_native_roundtrip_tests.rs"]
mod roundtrip;
