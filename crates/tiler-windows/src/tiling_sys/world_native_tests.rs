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
use super::super::world_diff::{
    FakeNative, FocusCallDiag, FreshHost, HostDiff, NativeOps, SelectionArrivalGrant,
    apply_world_reply,
};
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
    EffectKind, OneShotEffect, ScopeAuthority as Auth, ScopeStatus, TransferKind, TransferTarget,
    WORLD_CONTRACT_VERSION, WindowDesire, WorldCommand, WorldReply,
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
    observed_for_with_exe(key, token, vis, facts, EXE)
}

/// [`observed_for`] with an explicit executable path for the observed
/// identity (foreground shell-vs-game separation). The basename only ever
/// reaches the trace; the full path stays in the observation.
fn observed_for_with_exe(
    key: &WindowKey,
    token: &str,
    vis: Rect,
    facts: WindowFacts,
    exe: &str,
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
            exe_path: exe.to_owned(),
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
    /// Scripted live shell-window handle (`GetShellWindow`); `0` means
    /// unavailable. Reset per test.
    static S_SHELL: RefCell<u64> = const { RefCell::new(0) };
    /// Scripted shell-handle read sequence: popped per `shell_window`
    /// read while nonempty (mid-call shell drift analogue), else `S_SHELL`.
    /// Reset per test.
    static S_SHELL_SEQ: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// Scripted pid read sequence: popped per `window_pid` read whose HWND
    /// matches the front entry (mid-call recycle analogue); other HWNDs
    /// read `S_PID`. Reset per test.
    static S_PID_SEQ: RefCell<Vec<(u64, u32)>> = const { RefCell::new(Vec::new()) };
    /// Scripted identity read sequence: popped per `process_identity` read
    /// whose HWND matches the front entry (`None` = unreadable, fail
    /// closed); other HWNDs read `S_IDENT`. Reset per test.
    static S_IDENT_SEQ: RefCell<Vec<(u64, Option<ProcessIdentity>)>> =
        const { RefCell::new(Vec::new()) };
    /// Scripted system-shell-path read sequence: popped per
    /// `system_shell_path` read while nonempty, else `S_SYSTEM_SHELL`.
    /// Reset per test.
    static S_SYS_SEQ: RefCell<Vec<Option<String>>> = const { RefCell::new(Vec::new()) };
    /// HWND whose move/size probe flips to active once the prime runs
    /// (mid-call modal analogue); `None` keeps the `S_MOVE_SIZE` map
    /// exactly. Reset per test.
    static S_MODAL_AFTER_PRIME: RefCell<Option<u64>> = const { RefCell::new(None) };
    /// HWND whose observation fails once the prime runs (mid-call target
    /// departure analogue); `None` keeps `s_observe` exactly. Reset per
    /// test.
    static S_UNOBSERVE_AFTER_PRIME: RefCell<Option<u64>> = const { RefCell::new(None) };
    /// Scripted expected system shell executable path (`None` = unreadable,
    /// fail closed). Reset per test.
    static S_SYSTEM_SHELL: RefCell<Option<String>> = const { RefCell::new(None) };
    static S_VISIBLE: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static S_ICONIC: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static S_SET_OK: RefCell<HashMap<u64, bool>> = RefCell::new(HashMap::new());
    static S_SET_CALLS: RefCell<Vec<(u64, Rect)>> = const { RefCell::new(Vec::new()) };
    static S_FG: RefCell<u64> = const { RefCell::new(0) };
    /// Scripted foreground race sequence: when nonempty, each foreground
    /// read pops the front entry instead of `S_FG`, so a test can move the
    /// foreground mid-call and prove the gate-time sample differs from the
    /// later post-veto sample. Reset per test.
    static S_FG_SEQ: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// Every scripted foreground read, in order. The refusal diagnostic's
    /// single foreground sample is the only trace-gated read: probe-budget
    /// tests count it here, proving zero added probing off-trace and on
    /// success and exactly one added read on a traced refusal.
    static S_FG_CALLS: RefCell<u64> = const { RefCell::new(0) };
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
    /// Scripted post-prime foreground HWND: when set, the foreground lane
    /// reports the staged value until the prime runs, then this HWND
    /// (mid-call race analogue: the prime moved the foreground). Reset per
    /// test; `None` keeps the existing sequence behavior exactly.
    static S_FG_AFTER_PRIME: RefCell<Option<u64>> = const { RefCell::new(None) };
    static S_ADMIT: RefCell<HashMap<u64, Result<WindowIdentity, AdmitOutcome>>> =
        RefCell::new(HashMap::new());
    static S_HIDE: RefCell<HashMap<u64, Result<WindowIdentity, String>>> =
        RefCell::new(HashMap::new());
    static S_REVEAL: RefCell<HashMap<u64, Result<Revealed, String>>> =
        RefCell::new(HashMap::new());
    static S_MOVE_SIZE: RefCell<HashMap<u64, Option<bool>>> = RefCell::new(HashMap::new());
    static S_ADMIT_CALLS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// Scripted live cursor position (`None` = unreadable). Warp writes
    /// update it so re-reads observe the move, like the real position.
    static S_CURSOR: RefCell<Option<(i32, i32)>> = const { RefCell::new(None) };
    /// Every scripted warp write attempted, in order.
    static S_WARP_CALLS: RefCell<Vec<(i32, i32)>> = const { RefCell::new(Vec::new()) };
    /// Scripted warp refusals for the offline lane.
    static S_FAIL_WARP: RefCell<bool> = const { RefCell::new(false) };
    /// Accepted focus posts recorded, in order (async model: no FG flip).
    static S_FOCUS_CALLS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// Every scripted prime attempt, in VK order. The production prime
    /// lane records here so tests prove prime-before-setter ordering.
    static S_PRIME_CALLS: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    /// Whether the scripted prime was serviced since reset. The
    /// accept-iff-primed focus lane reads this, so acceptance proves the
    /// prime ran first (a setter-first order would read unprimed).
    static S_PRIMED: RefCell<bool> = const { RefCell::new(false) };
    /// Accepted-event count the scripted prime reports (clean pair: 2).
    static S_PRIME_INSERTED: RefCell<u8> = const { RefCell::new(2) };
}

fn reset_script() {
    S_OBSERVE.with(|m| m.borrow_mut().clear());
    S_PID.with(|m| m.borrow_mut().clear());
    S_SHELL.with(|m| *m.borrow_mut() = 0);
    S_SHELL_SEQ.with(|m| m.borrow_mut().clear());
    S_PID_SEQ.with(|m| m.borrow_mut().clear());
    S_IDENT_SEQ.with(|m| m.borrow_mut().clear());
    S_SYS_SEQ.with(|m| m.borrow_mut().clear());
    S_MODAL_AFTER_PRIME.with(|m| *m.borrow_mut() = None);
    S_UNOBSERVE_AFTER_PRIME.with(|m| *m.borrow_mut() = None);
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = None);
    S_VISIBLE.with(|m| m.borrow_mut().clear());
    S_ICONIC.with(|m| m.borrow_mut().clear());
    S_SET_OK.with(|m| m.borrow_mut().clear());
    S_SET_CALLS.with(|m| m.borrow_mut().clear());
    S_FG.with(|m| *m.borrow_mut() = 0);
    S_FG_SEQ.with(|m| m.borrow_mut().clear());
    S_FG_CALLS.with(|m| *m.borrow_mut() = 0);
    S_FOCUS_OK.with(|m| m.borrow_mut().clear());
    S_TAG.with(|m| m.borrow_mut().clear());
    S_HOOK.with(|m| *m.borrow_mut() = false);
    S_IDENT.with(|m| m.borrow_mut().clear());
    S_ADMIT.with(|m| m.borrow_mut().clear());
    S_HIDE.with(|m| m.borrow_mut().clear());
    S_REVEAL.with(|m| m.borrow_mut().clear());
    S_MOVE_SIZE.with(|m| m.borrow_mut().clear());
    S_ADMIT_CALLS.with(|m| m.borrow_mut().clear());
    S_DPI.with(|m| m.borrow_mut().clear());
    S_MONITORS.with(|m| m.borrow_mut().clear());
    S_GAME_FG.with(|m| *m.borrow_mut() = None);
    S_FG_AFTER_PRIME.with(|m| *m.borrow_mut() = None);
    S_CURSOR.with(|m| *m.borrow_mut() = None);
    S_WARP_CALLS.with(|m| m.borrow_mut().clear());
    S_FAIL_WARP.with(|m| *m.borrow_mut() = false);
    S_FOCUS_CALLS.with(|m| m.borrow_mut().clear());
    S_PRIME_CALLS.with(|m| m.borrow_mut().clear());
    S_PRIMED.with(|m| *m.borrow_mut() = false);
    S_PRIME_INSERTED.with(|m| *m.borrow_mut() = 2);
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
    // Mid-call target departure analogue: once the prime runs, this HWND
    // reads unreadable.
    if S_PRIMED.with(|m| *m.borrow()) && S_UNOBSERVE_AFTER_PRIME.with(|m| *m.borrow()) == Some(hwnd)
    {
        return Err(super::ObserveError::Unknown);
    }
    // Production contract: the visible reader rejects hidden windows as
    // Unknown (see `observe_window` line 950), so the scripted lane must
    // too. Prior fakes returned hidden entries here and masked the
    // self-hidden departure veto.
    if !S_VISIBLE.with(|m| m.borrow().contains(&hwnd)) {
        return Err(super::ObserveError::Unknown);
    }
    S_OBSERVE
        .with(|m| m.borrow().get(&hwnd).cloned())
        .ok_or(super::ObserveError::Unknown)
}

fn s_observe_hidden(
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
        window_pid: |hwnd| {
            if let Some(pid) = S_PID_SEQ.with(|seq| {
                let mut seq = seq.borrow_mut();
                match seq.first() {
                    Some((front_hwnd, _)) if *front_hwnd == hwnd => Some(seq.remove(0).1),
                    _ => None,
                }
            }) {
                return pid;
            }
            S_PID.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(0))
        },
        shell_window: || {
            S_SHELL_SEQ.with(|seq| {
                let mut seq = seq.borrow_mut();
                if seq.is_empty() {
                    S_SHELL.with(|m| *m.borrow())
                } else {
                    seq.remove(0)
                }
            })
        },
        system_shell_path: || {
            S_SYS_SEQ.with(|seq| {
                let mut seq = seq.borrow_mut();
                if seq.is_empty() {
                    S_SYSTEM_SHELL.with(|m| m.borrow().clone())
                } else {
                    seq.remove(0)
                }
            })
        },
        is_visible: |hwnd| S_VISIBLE.with(|m| m.borrow().contains(&hwnd)),
        is_iconic: |hwnd| S_ICONIC.with(|m| m.borrow().contains(&hwnd)),
        set_outer: |hwnd, outer| {
            S_SET_CALLS.with(|m| m.borrow_mut().push((hwnd, outer)));
            S_SET_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
        },
        foreground: || {
            S_FG_CALLS.with(|m| {
                let seen = *m.borrow();
                *m.borrow_mut() = seen.saturating_add(1);
            });
            // Post-prime race lane: once the prime ran, the foreground
            // reports the raced HWND when one is staged.
            if S_PRIMED.with(|m| *m.borrow())
                && let Some(raced) = S_FG_AFTER_PRIME.with(|m| *m.borrow())
            {
                return raced;
            }
            S_FG_SEQ.with(|seq| {
                let mut seq = seq.borrow_mut();
                if seq.is_empty() {
                    S_FG.with(|m| *m.borrow())
                } else {
                    seq.remove(0)
                }
            })
        },
        focus: |hwnd| S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true)),
        prime: |vk| {
            S_PRIME_CALLS.with(|m| m.borrow_mut().push(vk));
            S_PRIMED.with(|m| *m.borrow_mut() = true);
            S_PRIME_INSERTED.with(|m| *m.borrow())
        },
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
        process_identity: |hwnd| {
            if let Some(hit) = S_IDENT_SEQ.with(|seq| {
                let mut seq = seq.borrow_mut();
                match seq.first() {
                    Some((front_hwnd, _)) if *front_hwnd == hwnd => Some(seq.remove(0).1),
                    _ => None,
                }
            }) {
                return hit;
            }
            S_IDENT.with(|m| m.borrow().get(&hwnd).cloned())
        },
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
        hide_claim: |_store, _me, claim, _dir, _sink| {
            S_HIDE.with(|m| {
                m.borrow()
                    .get(&claim.hwnd)
                    .cloned()
                    .unwrap_or(Err("refused: scripted".to_owned()))
            })
        },
        reveal_claim: |_store, _me, claim, _min, _sink| {
            S_REVEAL.with(|m| {
                m.borrow()
                    .get(&claim.hwnd)
                    .cloned()
                    .unwrap_or(Err("refused: scripted".to_owned()))
            })
        },
        hide_claim_na: |_store, _me, claim, _dir, _sink| {
            S_HIDE.with(|m| {
                m.borrow()
                    .get(&claim.hwnd)
                    .cloned()
                    .unwrap_or(Err("refused: scripted".to_owned()))
            })
        },
        reveal_claim_na: |_store, _me, claim, _min, _sink| {
            S_REVEAL.with(|m| {
                m.borrow()
                    .get(&claim.hwnd)
                    .cloned()
                    .unwrap_or(Err("refused: scripted".to_owned()))
            })
        },
        move_size: |hwnd| {
            // Mid-call modal analogue: once the prime runs, the staged
            // HWND reports an active move/size loop.
            if S_PRIMED.with(|m| *m.borrow())
                && S_MODAL_AFTER_PRIME.with(|m| *m.borrow()) == Some(hwnd)
            {
                return Some(true);
            }
            S_MOVE_SIZE.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(Some(false)))
        },
        observe_hidden: s_observe_hidden,
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
fn switch_view_nonactivating_uses_na_claim_lanes() {
    let mut env = env_with(110_021);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let leaving = win_key(110_021, 2121, "proc-41");
    let entering = win_key(110_022, 2122, "proc-42");
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
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let mut prims = scripted();
    prims.hide_claim = |_, _, _, _, _| Err("refused: pumped".to_owned());
    prims.hide_claim_na = s_hide_conceal;
    let mut native = assemble_with_views(&mut env, prims, Some((out.clone(), ws2.clone())));
    assert_eq!(native.switch_view(&out, &ws2), Ok(false));
    assert!(!native.hidden_claims.contains_key(&leaving));
    assert_eq!(native.switch_view_nonactivating(&out, &ws2), Ok(true));
    assert!(native.hidden_claims.contains_key(&leaving));
    assert!(!native.hidden_claims.contains_key(&entering));
}

#[test]
fn focus_window_refuses_during_move_size() {
    let mut env = env_with(110_031);
    let ws1 = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_031, 2131, "proc-51");
    admit_member(&mut env, &key, &ws1, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out.clone(), ws1.clone());
    S_MOVE_SIZE.with(|m| m.borrow_mut().insert(key.hwnd, Some(true)));
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(false));
    S_MOVE_SIZE.with(|m| m.borrow_mut().insert(key.hwnd, None));
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(false));
    S_MOVE_SIZE.with(|m| m.borrow_mut().insert(key.hwnd, Some(false)));
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(key.hwnd, true));
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(true));
}

/// Async activation without any barrier: the accepted setter only posts
/// (no foreground flip), so the immediate proof honestly stays unproven
/// with exactly one setter call. No blocking, timeout, or retry: the
/// diff-level in-flight tracking owns any later landing, and only a fresh
/// independent foreground read proves arrival.
#[test]
fn focus_window_accepted_posts_without_landing_or_wait() {
    let mut env = env_with(110_032);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_032, 2132, "proc-52");
    let target = win_key(110_033, 2133, "proc-53");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    // Accepted setter posts only: records without flipping foreground.
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    assert_eq!(native.focus_window("w2"), Ok(true));
    assert_eq!(
        S_FOCUS_CALLS.with(|m| m.borrow().clone()),
        vec![target.hwnd]
    );
    assert!(!native.is_foreground("w2"));
    assert!(native.is_foreground("w1"));
}

/// Post-setter identity drift stays ambiguous: a same-HWND recycle across
/// the setter call reads `Err`, never accepted on the stale identity.
#[test]
fn focus_window_post_setter_recycle_is_ambiguous() {
    let mut env = env_with(110_051);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_051, 2151, "proc-71");
    let target = win_key(110_052, 2152, "proc-72");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_PID.with(|m| m.borrow_mut().insert(hwnd, 9999));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    assert_eq!(native.focus_window("w2"), Err(()));
    assert_eq!(S_FOCUS_CALLS.with(|m| m.borrow().len()), 1);
    assert!(!native.is_foreground("w2"));
}

/// Raw primitive return stays visible past post-setter fences: an accepted
/// primitive followed by same-HWND identity drift reports wrapper `Err`
/// with `native_return: Some(true)`, so the trace distinguishes native
/// acceptance from the wrapper fence. An early veto reports
/// `native_return: None`.
#[test]
fn focus_diag_native_return_survives_post_setter_fence() {
    let mut env = env_with(110_111);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_111, 2211, "proc-91");
    let target = win_key(110_112, 2212, "proc-92");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_PID.with(|m| m.borrow_mut().insert(hwnd, 9999));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    let (outcome, diag) = native.focus_window_traced("w2");
    assert_eq!(outcome, Err(()));
    assert!(diag.primitive_called);
    assert_eq!(diag.native_return, Some(true));
    assert!(
        diag.call_mono_ms
            .is_some_and(|call| diag.ret_mono_ms.is_some_and(|ret| call <= ret))
    );
    // Early veto never runs the primitive: no raw return exists.
    let mut env = env_with(110_113);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_113, 2213, "proc-93");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_MOVE_SIZE.with(|m| m.borrow_mut().insert(key.hwnd, Some(true)));
    let mut native = assemble(&mut env, scripted());
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.native_return, None);
    S_MOVE_SIZE.with(|m| m.borrow_mut().remove(&key.hwnd));
}

/// Startup-shape regression: a background owner with existing targets and
/// no prior input rights primes once (E8) immediately before the single
/// setter. The scripted setter accepts iff the prime was serviced, so
/// acceptance functionally proves prime-before-setter ordering (a
/// setter-first order would read unprimed and refuse). Exactly one prime
/// and one setter run; the trace carries the actual prime outcome with
/// call/return stamps strictly around the setter. No attach or coupling
/// exists anywhere on this path: `NativePrimitives` carries no attach
/// lane, and the input filter already drops injected E8.
#[test]
fn focus_primes_once_before_setter_and_reports_it() {
    let mut env = env_with(110_201);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_201, 2201, "proc-201");
    let target = win_key(110_202, 2202, "proc-202");
    let other = win_key(110_203, 2203, "proc-203");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    admit_member(&mut env, &other, &ws, "w3", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    script_live(&other, "w3", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        // Native stand-in for the foreground lock: the setter only
        // accepts once the prime made this owner the last input
        // provider. No prior E8/menu-mask ran in this fixture.
        S_PRIMED.with(|m| *m.borrow())
    };
    let mut native = assemble(&mut env, prims);
    let (outcome, diag) = native.focus_window_traced("w2");
    assert_eq!(outcome, Ok(true));
    assert_eq!(S_PRIME_CALLS.with(|m| m.borrow().clone()), vec![0xE8]);
    assert_eq!(
        S_FOCUS_CALLS.with(|m| m.borrow().clone()),
        vec![target.hwnd]
    );
    assert!(diag.primitive_called);
    assert_eq!(diag.prime_inserted, Some(2));
    assert_eq!(diag.native_return, Some(true));
    assert!(
        diag.call_mono_ms
            .is_some_and(|call| diag.ret_mono_ms.is_some_and(|ret| call <= ret))
    );
}

/// Safety vetos never prime: a move/size-gated target reports the
/// pre-primitive veto with no primitive detail and no prime call, while a
/// post-setter identity recycle still reports the actual prime outcome
/// alongside the honest raw return.
#[test]
fn focus_veto_runs_no_prime_and_keeps_trace_honest() {
    let mut env = env_with(110_204);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_204, 2204, "proc-204");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_MOVE_SIZE.with(|m| m.borrow_mut().insert(key.hwnd, Some(true)));
    let mut native = assemble(&mut env, scripted());
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.prime_inserted, None);
    assert_eq!(diag.native_return, None);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
    S_MOVE_SIZE.with(|m| m.borrow_mut().remove(&key.hwnd));

    let mut env = env_with(110_205);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_205, 2205, "proc-205");
    let target = win_key(110_206, 2206, "proc-206");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_PID.with(|m| m.borrow_mut().insert(hwnd, 9999));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    let (outcome, diag) = native.focus_window_traced("w2");
    assert_eq!(outcome, Err(()));
    assert!(diag.primitive_called);
    assert_eq!(diag.prime_inserted, Some(2));
    assert_eq!(diag.native_return, Some(true));
    assert_eq!(S_PRIME_CALLS.with(|m| m.borrow().clone()), vec![0xE8]);
    assert_eq!(S_FOCUS_CALLS.with(|m| m.borrow().len()), 1);
}

/// Refused setter reports an honest `Ok(false)`: nothing was posted and
/// the immediate proof stays false.
#[test]
fn focus_window_refused_setter_reports_refusal() {
    let mut env = env_with(110_034);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_034, 2134, "proc-54");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(key.hwnd, false));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    assert_eq!(native.focus_window("w1"), Ok(false));
    assert!(!native.is_foreground("w1"));
}

/// Accepted but unlanded: the async request may still land later, so the
/// accepted setter reports `Ok(true)` while the independent proof
/// honestly stays false. No convergence is claimed and nothing waits.
#[test]
fn focus_window_hung_target_keeps_accepted_without_proof() {
    let mut env = env_with(110_035);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_035, 2135, "proc-55");
    let target = win_key(110_036, 2136, "proc-56");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    assert_eq!(native.focus_window("w2"), Ok(true));
    assert!(!native.is_foreground("w2"));
    assert!(native.is_foreground("w1"));
}

/// Competitor holds foreground: the accepted setter reports `Ok(true)`
/// with an honest unproven proof. Exactly one setter ran: no refire.
#[test]
fn focus_window_competitor_holds_foreground_without_refire() {
    let mut env = env_with(110_039);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_039, 2139, "proc-59");
    let target = win_key(110_040, 2140, "proc-60");
    let foreign_hwnd = 119_999;
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    // Observable non-fullscreen competitor: the write gate reads a genuine
    // foreground holder (not an unknown veto), so the accepted setter
    // reports `Ok(true)` with an honest unproven proof.
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            foreign_hwnd,
            observed_for(
                &win_key(foreign_hwnd, 2199, "proc-99"),
                "foreign",
                rect(0, 0, 800, 600),
                plain_facts(),
            ),
        )
    });
    S_VISIBLE.with(|m| m.borrow_mut().insert(foreign_hwnd));
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = foreign_hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    assert_eq!(native.focus_window("w2"), Ok(true));
    assert_eq!(S_FOCUS_CALLS.with(|m| m.borrow().len()), 1);
    assert!(!native.is_foreground("w2"));
}

/// Late landing without any barrier: the accepted setter stays unproven,
/// the async activation lands later, and a later pass proves arrival by
/// fresh read only. Nothing refires: exactly one setter ran across both
/// passes and no causality is claimed from the first pass's setter.
#[test]
fn focus_window_late_landing_after_unproven_never_replays() {
    let mut env = env_with(110_045);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_045, 2145, "proc-65");
    let target = win_key(110_046, 2146, "proc-66");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let entry_p1 = S_FG.with(|m| *m.borrow());
    {
        let mut native = assemble(&mut env, prims);
        assert_eq!(native.focus_window("w2"), Ok(true));
        assert!(!native.is_foreground("w2"));
    }
    // The async request lands later with no further setter.
    S_FG.with(|m| *m.borrow_mut() = target.hwnd);
    {
        let mut native = assemble(&mut env, prims);
        let entry_p2 = S_FG.with(|m| *m.borrow());
        assert_ne!(entry_p1, entry_p2);
        assert_eq!(entry_p2, target.hwnd);
        assert!(native.is_foreground("w2"));
        assert!(!native.is_foreground("w1"));
    }
    assert_eq!(S_FOCUS_CALLS.with(|m| m.borrow().len()), 1);
}

/// Same-pid creation drift across the setter call: the HWND is recycled by
/// a new process before the proof read. The full post-setter identity
/// fences fail ambiguous (`Err`) instead of crediting the setter.
#[test]
fn focus_window_same_pid_creation_drift_is_ambiguous() {
    let mut env = env_with(110_047);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_047, 2147, "proc-67");
    let target = win_key(110_048, 2148, "proc-68");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_IDENT.with(|m| {
            if let Some(ident) = m.borrow_mut().get_mut(&hwnd) {
                ident.process_creation = "proc-99".to_owned();
            }
        });
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    assert_eq!(native.focus_window("w2"), Err(()));
    assert!(!native.is_foreground("w2"));
}

/// Membership-tag swap across the setter call: the HWND keeps its pid but
/// the live tag no longer matches the stored lifetime. The post-setter
/// lifetime fence fails ambiguous (`Err`) instead of crediting the
/// setter.
#[test]
fn focus_window_member_tag_drift_is_ambiguous() {
    let mut env = env_with(110_049);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_049, 2149, "proc-69");
    let target = win_key(110_050, 2150, "proc-70");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_TAG.with(|m| m.borrow_mut().insert(hwnd, "member-tag-9".to_owned()));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    assert_eq!(native.focus_window("w2"), Err(()));
    assert!(!native.is_foreground("w2"));
}

/// Drained foreground-edge resolution: a readable HWND reports its exact
/// live identity (HWND plus PID plus creation), a null HWND reports none,
/// and an unreadable HWND fails closed. Identity only, never causality.
#[test]
fn foreground_identity_of_known_unknown_and_null() {
    use super::super::world_diff::ForegroundState;
    let mut env = env_with(110_060);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_060, 2160, "proc-80");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    match native.foreground_identity_of(key.hwnd) {
        ForegroundState::Known(identity) => {
            assert_eq!(identity.hwnd, key.hwnd);
            assert_eq!(identity.pid, key.pid);
            assert_eq!(identity.creation, key.creation);
        }
        other => panic!("readable HWND must report its identity: {other:?}"),
    }
    assert_eq!(native.foreground_identity_of(0), ForegroundState::None);
    assert_eq!(
        native.foreground_identity_of(999_999),
        ForegroundState::Unknown
    );
}

/// A stale prior hidden claim never authorizes the hidden lane: without
/// a same-pass nonactivating hide of the entry foreground by this
/// actuator instance, even an exact committed claim with fresh
/// non-fullscreen hidden facts still vetoes.
#[test]
fn focus_vetoes_stale_prior_hidden_claim() {
    let mut env = env_with(110_041);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_041, 2141, "proc-61");
    let departed = win_key(110_042, 2142, "proc-62");
    admit_member(&mut env, &target, &ws, "w1", false);
    admit_member(&mut env, &departed, &ws, "w2", false);
    script_live(&target, "w1", rect(0, 0, 800, 600), plain_facts());
    // Departure reads hidden: no S_VISIBLE entry, so the visible reader
    // rejects it as Unknown while the hidden reader still sees facts.
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            departed.hwnd,
            observed_for(&departed, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(departed.hwnd, departed.pid));
    S_IDENT.with(|m| m.borrow_mut().insert(departed.hwnd, identity_of(&departed)));
    S_TAG.with(|m| {
        m.borrow_mut()
            .insert(departed.hwnd, "member-tag-1".to_owned())
    });
    env.state.hidden_claims.insert(
        departed.clone(),
        super::super::HiddenRecord {
            claim: valid_claim(departed.hwnd, &identity_of(&departed)),
            iconic: false,
        },
    );
    S_FG.with(|m| *m.borrow_mut() = departed.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(false));
}

/// Same-pass departure passes: the nonactivating switch hides the entry
/// foreground, and the trailing focus reads its fresh hidden facts
/// (positively non-fullscreen) through the same fullscreen plus
/// focus-eligibility predicate.
#[test]
fn focus_succeeds_with_same_pass_hidden_departure() {
    let mut env = env_with(110_045);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let leaving = win_key(110_045, 2145, "proc-65");
    let entering = win_key(110_046, 2146, "proc-66");
    admit_member(&mut env, &leaving, &ws1, "w1", false);
    admit_member(&mut env, &entering, &ws2, "w2", false);
    env.state.hidden_claims.insert(
        entering.clone(),
        super::super::HiddenRecord {
            claim: script_claim(entering.hwnd, &identity_of(&entering)),
            iconic: false,
        },
    );
    script_live(&leaving, "w1", rect(0, 0, 800, 600), plain_facts());
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            entering.hwnd,
            observed_for(&entering, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(entering.hwnd, entering.pid));
    for key in [&leaving, &entering] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    S_FG.with(|m| *m.borrow_mut() = leaving.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(entering.hwnd, true));
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let mut native =
        assemble_with_views(&mut env, moving_prims(), Some((out.clone(), ws2.clone())));
    assert_eq!(native.switch_view_nonactivating(&out, &ws2), Ok(true));
    assert!(native.hidden_claims.contains_key(&leaving));
    assert!(!S_VISIBLE.with(|m| m.borrow().contains(&leaving.hwnd)));
    assert_eq!(native.focus_window("w2"), Ok(true));
}

/// Same-pass departure with fullscreen hidden facts still vetoes.
#[test]
fn focus_vetoes_same_pass_hidden_fullscreen_departure() {
    let mut env = env_with(110_047);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let leaving = win_key(110_047, 2147, "proc-67");
    let entering = win_key(110_048, 2148, "proc-68");
    admit_member(&mut env, &leaving, &ws1, "w1", false);
    admit_member(&mut env, &entering, &ws2, "w2", false);
    env.state.hidden_claims.insert(
        entering.clone(),
        super::super::HiddenRecord {
            claim: script_claim(entering.hwnd, &identity_of(&entering)),
            iconic: false,
        },
    );
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            leaving.hwnd,
            observed_for(&leaving, "w1", rect(0, 0, 1920, 1080), fg_facts),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(leaving.hwnd, leaving.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(leaving.hwnd));
    S_TAG.with(|m| {
        m.borrow_mut()
            .insert(leaving.hwnd, "member-tag-1".to_owned())
    });
    S_IDENT.with(|m| m.borrow_mut().insert(leaving.hwnd, identity_of(&leaving)));
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            entering.hwnd,
            observed_for(&entering, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(entering.hwnd, entering.pid));
    for key in [&leaving, &entering] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    S_IDENT.with(|m| m.borrow_mut().insert(entering.hwnd, identity_of(&entering)));
    S_FG.with(|m| *m.borrow_mut() = leaving.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(entering.hwnd, true));
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let mut native =
        assemble_with_views(&mut env, moving_prims(), Some((out.clone(), ws2.clone())));
    assert_eq!(native.switch_view_nonactivating(&out, &ws2), Ok(true));
    assert_eq!(native.focus_window("w2"), Ok(false));
}

/// Same-pass departure with a lifetime tag drift before the setter still
/// vetoes.
#[test]
fn focus_vetoes_same_pass_hidden_tag_drift() {
    let mut env = env_with(110_049);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let leaving = win_key(110_049, 2149, "proc-69");
    let entering = win_key(110_050, 2150, "proc-70");
    admit_member(&mut env, &leaving, &ws1, "w1", false);
    admit_member(&mut env, &entering, &ws2, "w2", false);
    env.state.hidden_claims.insert(
        entering.clone(),
        super::super::HiddenRecord {
            claim: script_claim(entering.hwnd, &identity_of(&entering)),
            iconic: false,
        },
    );
    script_live(&leaving, "w1", rect(0, 0, 800, 600), plain_facts());
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            entering.hwnd,
            observed_for(&entering, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(entering.hwnd, entering.pid));
    for key in [&leaving, &entering] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    S_FG.with(|m| *m.borrow_mut() = leaving.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(entering.hwnd, true));
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let mut native =
        assemble_with_views(&mut env, moving_prims(), Some((out.clone(), ws2.clone())));
    assert_eq!(native.switch_view_nonactivating(&out, &ws2), Ok(true));
    S_TAG.with(|m| m.borrow_mut().insert(leaving.hwnd, "stale-tag".to_owned()));
    assert_eq!(native.focus_window("w2"), Ok(false));
}

/// Same-pass departure lost to a foreign foreground reappearance before
/// the setter still vetoes.
#[test]
fn focus_vetoes_same_pass_hidden_foreground_moved() {
    let mut env = env_with(110_052);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let leaving = win_key(110_052, 2152, "proc-72");
    let entering = win_key(110_053, 2153, "proc-73");
    admit_member(&mut env, &leaving, &ws1, "w1", false);
    admit_member(&mut env, &entering, &ws2, "w2", false);
    env.state.hidden_claims.insert(
        entering.clone(),
        super::super::HiddenRecord {
            claim: script_claim(entering.hwnd, &identity_of(&entering)),
            iconic: false,
        },
    );
    script_live(&leaving, "w1", rect(0, 0, 800, 600), plain_facts());
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            entering.hwnd,
            observed_for(&entering, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(entering.hwnd, entering.pid));
    for key in [&leaving, &entering] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    S_FG.with(|m| *m.borrow_mut() = leaving.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(entering.hwnd, true));
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let mut native =
        assemble_with_views(&mut env, moving_prims(), Some((out.clone(), ws2.clone())));
    assert_eq!(native.switch_view_nonactivating(&out, &ws2), Ok(true));
    let foreign = win_key(110_054, 2154, "proc-74");
    S_VISIBLE.with(|m| m.borrow_mut().insert(foreign.hwnd));
    S_PID.with(|m| m.borrow_mut().insert(foreign.hwnd, foreign.pid));
    S_FG.with(|m| *m.borrow_mut() = foreign.hwnd);
    assert_eq!(native.focus_window("w2"), Ok(false));
}

/// Hidden fullscreen departure still vetoes: fresh hidden facts show a
/// captionless covering frame.
#[test]
fn focus_vetoes_hidden_fullscreen_departure() {
    let mut env = env_with(110_051);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_051, 2151, "proc-63");
    let departed = win_key(110_052, 2152, "proc-64");
    admit_member(&mut env, &target, &ws, "w1", false);
    admit_member(&mut env, &departed, &ws, "w2", false);
    script_live(&target, "w1", rect(0, 0, 800, 600), plain_facts());
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            departed.hwnd,
            observed_for(&departed, "w2", rect(0, 0, 1920, 1080), fg_facts),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(departed.hwnd, departed.pid));
    S_IDENT.with(|m| m.borrow_mut().insert(departed.hwnd, identity_of(&departed)));
    S_TAG.with(|m| {
        m.borrow_mut()
            .insert(departed.hwnd, "member-tag-1".to_owned())
    });
    env.state.hidden_claims.insert(
        departed.clone(),
        super::super::HiddenRecord {
            claim: valid_claim(departed.hwnd, &identity_of(&departed)),
            iconic: false,
        },
    );
    S_FG.with(|m| *m.borrow_mut() = departed.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(false));
}

/// Hidden departure with no fresh hidden facts still vetoes (unknown).
#[test]
fn focus_vetoes_hidden_unreadable_departure() {
    let mut env = env_with(110_061);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_061, 2161, "proc-65");
    let departed = win_key(110_062, 2162, "proc-66");
    admit_member(&mut env, &target, &ws, "w1", false);
    admit_member(&mut env, &departed, &ws, "w2", false);
    script_live(&target, "w1", rect(0, 0, 800, 600), plain_facts());
    // No S_OBSERVE entry for the departure: both readers stay Unknown.
    S_PID.with(|m| m.borrow_mut().insert(departed.hwnd, departed.pid));
    S_IDENT.with(|m| m.borrow_mut().insert(departed.hwnd, identity_of(&departed)));
    S_TAG.with(|m| {
        m.borrow_mut()
            .insert(departed.hwnd, "member-tag-1".to_owned())
    });
    env.state.hidden_claims.insert(
        departed.clone(),
        super::super::HiddenRecord {
            claim: valid_claim(departed.hwnd, &identity_of(&departed)),
            iconic: false,
        },
    );
    S_FG.with(|m| *m.borrow_mut() = departed.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(false));
}

/// Hidden foreground without an exact committed claim still vetoes.
#[test]
fn focus_vetoes_hidden_unowned_foreground() {
    let mut env = env_with(110_071);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_071, 2171, "proc-67");
    let departed = win_key(110_072, 2172, "proc-68");
    admit_member(&mut env, &target, &ws, "w1", false);
    admit_member(&mut env, &departed, &ws, "w2", false);
    script_live(&target, "w1", rect(0, 0, 800, 600), plain_facts());
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            departed.hwnd,
            observed_for(&departed, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(departed.hwnd, departed.pid));
    S_IDENT.with(|m| m.borrow_mut().insert(departed.hwnd, identity_of(&departed)));
    S_TAG.with(|m| {
        m.borrow_mut()
            .insert(departed.hwnd, "member-tag-1".to_owned())
    });
    // No hidden_claims entry: unowned hidden foreground.
    S_FG.with(|m| *m.borrow_mut() = departed.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(false));
}

/// Recycled hidden departure (identity drift) still vetoes.
#[test]
fn focus_vetoes_hidden_recycled_departure() {
    let mut env = env_with(110_081);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_081, 2181, "proc-69");
    let departed = win_key(110_082, 2182, "proc-70");
    admit_member(&mut env, &target, &ws, "w1", false);
    admit_member(&mut env, &departed, &ws, "w2", false);
    script_live(&target, "w1", rect(0, 0, 800, 600), plain_facts());
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            departed.hwnd,
            observed_for(&departed, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(departed.hwnd, departed.pid));
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            departed.hwnd,
            ProcessIdentity {
                process_creation: "proc-70-recycled".to_owned(),
                ..identity_of(&departed)
            },
        )
    });
    S_TAG.with(|m| {
        m.borrow_mut()
            .insert(departed.hwnd, "member-tag-1".to_owned())
    });
    env.state.hidden_claims.insert(
        departed.clone(),
        super::super::HiddenRecord {
            claim: valid_claim(departed.hwnd, &identity_of(&departed)),
            iconic: false,
        },
    );
    S_FG.with(|m| *m.borrow_mut() = departed.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(false));
}

/// Lifetime tag drift on the hidden departure still vetoes.
#[test]
fn focus_vetoes_hidden_tag_drift_departure() {
    let mut env = env_with(110_091);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_091, 2191, "proc-71");
    let departed = win_key(110_092, 2192, "proc-72");
    admit_member(&mut env, &target, &ws, "w1", false);
    admit_member(&mut env, &departed, &ws, "w2", false);
    script_live(&target, "w1", rect(0, 0, 800, 600), plain_facts());
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            departed.hwnd,
            observed_for(&departed, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(departed.hwnd, departed.pid));
    S_IDENT.with(|m| m.borrow_mut().insert(departed.hwnd, identity_of(&departed)));
    S_TAG.with(|m| m.borrow_mut().insert(departed.hwnd, "stale-tag".to_owned()));
    env.state.hidden_claims.insert(
        departed.clone(),
        super::super::HiddenRecord {
            claim: valid_claim(departed.hwnd, &identity_of(&departed)),
            iconic: false,
        },
    );
    S_FG.with(|m| *m.borrow_mut() = departed.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(false));
}

/// A newly visible foreign foreground still vetoes: it reads visible,
/// so the hidden-verified lane never engages.
#[test]
fn focus_vetoes_newly_visible_foreign() {
    let mut env = env_with(110_101);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_101, 2201, "proc-73");
    admit_member(&mut env, &target, &ws, "w1", false);
    script_live(&target, "w1", rect(0, 0, 800, 600), plain_facts());
    let foreign = win_key(110_102, 2202, "proc-74");
    S_VISIBLE.with(|m| m.borrow_mut().insert(foreign.hwnd));
    // No observe entry and no hidden claim: visible yet unreadable.
    S_PID.with(|m| m.borrow_mut().insert(foreign.hwnd, foreign.pid));
    S_FG.with(|m| *m.borrow_mut() = foreign.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window("w1"), Ok(false));
}

/// Frame writes keep the strict unknown veto: a hidden departure with
/// no visible facts still holds the geometry write even though focus
/// would proceed through the hidden-verified lane.
#[test]
fn frame_write_vetoes_hidden_departure_unknown() {
    let mut env = env_with(110_111);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_111, 2211, "proc-75");
    let departed = win_key(110_112, 2212, "proc-76");
    admit_member(&mut env, &target, &ws, "w1", false);
    admit_member(&mut env, &departed, &ws, "w2", false);
    script_live(&target, "w1", rect(0, 0, 800, 600), plain_facts());
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            departed.hwnd,
            observed_for(&departed, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(departed.hwnd, departed.pid));
    S_IDENT.with(|m| m.borrow_mut().insert(departed.hwnd, identity_of(&departed)));
    S_TAG.with(|m| {
        m.borrow_mut()
            .insert(departed.hwnd, "member-tag-1".to_owned())
    });
    env.state.hidden_claims.insert(
        departed.clone(),
        super::super::HiddenRecord {
            claim: valid_claim(departed.hwnd, &identity_of(&departed)),
            iconic: false,
        },
    );
    S_FG.with(|m| *m.borrow_mut() = departed.hwnd);
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.write_frame("w1", rect(10, 10, 400, 300)), Ok(false));
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
    _sink: Option<&mut crate::product_hide::sys::VisibilityPostTrace>,
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
    _sink: Option<&mut crate::product_hide::sys::VisibilityPostTrace>,
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
    prims.hide_claim_na = s_hide_conceal;
    prims.reveal_claim_na = s_reveal_show;
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
    /// Sequencer delivery of the produced reply (`accepted`, `duplicate`,
    /// or `command-consumed`): lets tests prove fresh-accepted-only
    /// behavior without re-implementing the dispatch path.
    delivery: &'static str,
    /// Command sequence of the dispatched report (`None` for
    /// observation-only ticks): binds reply effects to their command.
    command_seq: Option<u64>,
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
    let (delivery, reply) = match owner.deliver_production(&report) {
        WorldOutcome::Accepted(reply) => ("accepted", reply),
        WorldOutcome::Duplicate(reply) => ("duplicate", reply),
        WorldOutcome::CommandConsumed(reply) => ("command-consumed", reply),
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
        delivery,
        command_seq: report.command_seq,
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

/// Hide-commit snapshot seeding shared rig: one tiled member whose cache
/// holds the pre-write slot while the live window already proves the
/// post-write frame.
struct HideSeedRig {
    env: Env,
    leaving: WindowKey,
    out: String,
    ws2: String,
    old: Rect,
    proven: Rect,
}

fn hide_seed_rig(base_hwnd: u64) -> HideSeedRig {
    let mut env = env_with(base_hwnd);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let leaving = win_key(base_hwnd, 2201, "proc-71");
    admit_member(&mut env, &leaving, &ws1, "w1", false);
    let old = rect(0, 0, 800, 600);
    let proven = rect(0, 0, 1024, 640);
    // Pre-write slot, frozen by the observation tick before the write.
    env.state.member_rects.insert("w1".to_owned(), old);
    // The live window proves the post-write frame (accepted setter with an
    // exact readback on the previous pass).
    script_live(&leaving, "w1", proven, plain_facts());
    S_ADMIT.with(|m| {
        m.borrow_mut().insert(
            leaving.hwnd,
            Ok(script_claim(leaving.hwnd, &identity_of(&leaving))),
        )
    });
    // The scripted hide post returns an association-valid claim so capture
    // can ride the snapshot instead of holding on the claim.
    S_HIDE.with(|m| {
        m.borrow_mut().insert(
            leaving.hwnd,
            Ok(valid_claim(leaving.hwnd, &identity_of(&leaving))),
        )
    });
    HideSeedRig {
        env,
        leaving,
        out,
        ws2,
        old,
        proven,
    }
}

fn seed_test_areas() -> Vec<super::super::MonitorArea> {
    vec![super::super::MonitorArea {
        device: DEV1.to_owned(),
        work: rect(0, 0, 1920, 1040),
        full: rect(0, 0, 1920, 1080),
    }]
}

/// Production lane routing mirrored from the owner tick drain: one seed
/// lands in the tile/float cache under the same lane row assembly uses.
fn drain_seeds(state: &mut super::super::TileLoop, seeds: Vec<(WindowKey, Rect)>) {
    let sticky = &state.sticky;
    let fixed_auto_seen = &state.fixed_auto_seen;
    let fixed_tile_overrides = &state.fixed_tile_overrides;
    let fixed_origin_overrides = &state.fixed_origin_overrides;
    let born_first_exit = &state.born_first_exit;
    let floated = &state.floated;
    let restart_reject_hold = &state.restart_reject_hold;
    let born_fullscreen = &state.born_fullscreen;
    super::apply_hidden_seeds(
        seeds,
        &state.member_tokens,
        &mut state.member_rects,
        &mut state.float_rects,
        |key| {
            let sticky = sticky.contains_key(key);
            let durable =
                fixed_tile_overrides.contains(key) || fixed_origin_overrides.contains(key);
            let automatic = fixed_auto_seen.contains(key) && !sticky && !durable;
            crate::native_guards::float_lane(
                sticky,
                automatic,
                durable,
                born_first_exit.contains(key),
                floated.contains(key),
                restart_reject_hold.contains(key),
                born_fullscreen.contains(key),
                true,
            )
        },
    );
}

fn hidden_fact_rect(env: &Env, areas: &[super::super::MonitorArea]) -> Rect {
    let staged = capture_world(&env.state.inputs(), Some(&[]), &[], areas, 0);
    staged
        .windows
        .iter()
        .find(|f| f.window.0 == "w1")
        .expect("w1 fact")
        .rect
        .expect("hidden snapshot rides a rect")
}

#[test]
fn hide_seeds_hidden_snapshot_with_fresh_proven_frame() {
    // Stress-home-away regression: a tiled write lands accepted with an
    // exact readback, then the member hides in the same pass. The staged
    // hidden snapshot must carry the freshly proven frame, not the pre-write
    // slot: the core prefers the staged fact over the readback
    // (`world_frames::attempt_observed`), so a stale snapshot pins the
    // permanent clamp lane while the fresh one clears it
    // (observed == target).
    let mut rig = hide_seed_rig(120_101);
    let mut native = assemble_with_views(
        &mut rig.env,
        scripted(),
        Some((rig.out.clone(), rig.ws2.clone())),
    );
    assert!(native.switch_view(&rig.out, &rig.ws2).expect("switch"));
    assert!(
        native.hidden_claims.contains_key(&rig.leaving),
        "leaving member hides in place"
    );
    let seeds = native.take_hidden_seeds();
    assert_eq!(seeds, vec![(rig.leaving.clone(), rig.proven)]);
    drop(native);
    // Post-hide the window reads invisible: no live row rides.
    S_VISIBLE.with(|m| m.borrow_mut().remove(&rig.leaving.hwnd));
    let areas = seed_test_areas();
    // Before the drain the report still stages the pre-write slot: the exact
    // stale-fact shape that pins the core clamp lane against the proven
    // exact readback.
    assert_eq!(hidden_fact_rect(&rig.env, &areas), rig.old);
    // Production drain, then the report stages the proven frame, which
    // clears the attempt instead of clamping it.
    drain_seeds(&mut rig.env.state, seeds);
    assert_eq!(rig.env.state.member_rects.get("w1"), Some(&rig.proven));
    assert_eq!(hidden_fact_rect(&rig.env, &areas), rig.proven);
}

#[test]
fn hide_seeds_float_snapshot_for_floated_member() {
    // Float lane routes to `float_rects` and leaves the tile slot alone.
    let mut rig = hide_seed_rig(120_102);
    rig.env.state.floated.insert(rig.leaving.clone());
    let old_float = rect(10, 10, 200, 200);
    rig.env.state.float_rects.insert("w1".to_owned(), old_float);
    let mut native = assemble_with_views(
        &mut rig.env,
        scripted(),
        Some((rig.out.clone(), rig.ws2.clone())),
    );
    assert!(native.switch_view(&rig.out, &rig.ws2).expect("switch"));
    let seeds = native.take_hidden_seeds();
    assert_eq!(seeds, vec![(rig.leaving.clone(), rig.proven)]);
    drop(native);
    S_VISIBLE.with(|m| m.borrow_mut().remove(&rig.leaving.hwnd));
    let areas = seed_test_areas();
    assert_eq!(hidden_fact_rect(&rig.env, &areas), old_float);
    drain_seeds(&mut rig.env.state, seeds);
    assert_eq!(rig.env.state.float_rects.get("w1"), Some(&rig.proven));
    assert_eq!(
        rig.env.state.member_rects.get("w1"),
        Some(&rig.old),
        "tile slot untouched for the float lane"
    );
    assert_eq!(hidden_fact_rect(&rig.env, &areas), rig.proven);
}

#[test]
fn hide_without_live_frame_keeps_stale_fallback_without_seed() {
    // No live proof at hide time (identity and lifetime still verify, but
    // the frame no longer reads): the hide commits, no seed is recorded, and
    // the report keeps the existing fallback instead of fabricating geometry
    // from the desired target.
    let mut rig = hide_seed_rig(120_103);
    S_VISIBLE.with(|m| m.borrow_mut().remove(&rig.leaving.hwnd));
    S_OBSERVE.with(|m| m.borrow_mut().remove(&rig.leaving.hwnd));
    let mut native = assemble_with_views(
        &mut rig.env,
        scripted(),
        Some((rig.out.clone(), rig.ws2.clone())),
    );
    assert!(native.switch_view(&rig.out, &rig.ws2).expect("switch"));
    assert!(
        native.hidden_claims.contains_key(&rig.leaving),
        "hide still commits without a live frame"
    );
    assert!(
        native.take_hidden_seeds().is_empty(),
        "no live proof, no seed"
    );
    drop(native);
    let areas = seed_test_areas();
    assert_eq!(
        hidden_fact_rect(&rig.env, &areas),
        rig.old,
        "existing fallback retained"
    );
}

#[test]
fn hide_identity_drift_refuses_without_seed() {
    // Recycled HWND: the live process no longer names the member lifetime.
    // The hide refuses and records nothing.
    let mut rig = hide_seed_rig(120_104);
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            rig.leaving.hwnd,
            ProcessIdentity {
                pid: 9999,
                ..identity_of(&rig.leaving)
            },
        )
    });
    let mut native = assemble_with_views(
        &mut rig.env,
        scripted(),
        Some((rig.out.clone(), rig.ws2.clone())),
    );
    assert!(
        native.switch_view(&rig.out, &rig.ws2).is_err(),
        "identity drift holds ambiguously"
    );
    assert!(!native.hidden_claims.contains_key(&rig.leaving));
    assert!(native.take_hidden_seeds().is_empty());
}

/// Trace-only refusal diagnostics for the four `Ok(false)` pre-primitive
/// focus branches: scope, `classify_focus`, focus-write veto, move/size veto.
/// Each names its gate plus non-sensitive safe state (target
/// visibility/cloak/ledger-hidden at dispatch, foreground identity class);
/// classify additionally carries its reason. Refusal-only, trace-armed only:
/// off-trace and success paths report no detail and probe nothing extra.
#[test]
fn focus_refusal_diag_reports_scope_gate() {
    let mut env = env_with(110_301);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_301, 2301, "proc-301");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    // Nonempty scope naming another basename refuses this member.
    env.state.scope = vec!["other.exe".to_owned()];
    // Foreground already names the refused target.
    S_FG.with(|m| *m.borrow_mut() = key.hwnd);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.native_return, None);
    assert_eq!(diag.prime_inserted, None);
    assert_eq!(diag.refusal_gate, Some("scope"));
    assert_eq!(diag.refusal_reason, None);
    assert_eq!(diag.target_visible, Some(true));
    assert_eq!(diag.target_cloaked, Some(false));
    assert_eq!(diag.target_hidden, Some(false));
    assert_eq!(diag.fg_identity, Some("target"));
    assert_eq!(diag.gui_before, None);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
}

#[test]
fn focus_refusal_diag_reports_classify_reason() {
    let mut env = env_with(110_302);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_302, 2302, "proc-302");
    admit_member(&mut env, &key, &ws, "w1", false);
    let mut facts = plain_facts();
    facts.tool_window = true;
    script_live(&key, "w1", rect(0, 0, 800, 600), facts);
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("classify-focus"));
    assert_eq!(diag.refusal_reason, Some("tool"));
    assert_eq!(diag.target_visible, Some(true));
    assert_eq!(diag.target_cloaked, Some(false));
    assert_eq!(diag.target_hidden, Some(false));
    // No foreground anywhere: the null class, never a guess.
    assert_eq!(diag.fg_identity, Some("none"));
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
}

#[test]
fn focus_refusal_diag_reports_focus_write_veto() {
    let mut env = env_with(110_303);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_303, 2303, "proc-303");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    // Live fullscreen competitor holds the foreground.
    let fg_key = win_key(110_304, 2304, "proc-304");
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            fg_key.hwnd,
            observed_for(&fg_key, "fg", rect(0, 0, 1920, 1080), fg_facts),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(fg_key.hwnd, fg_key.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(fg_key.hwnd));
    S_FG.with(|m| *m.borrow_mut() = fg_key.hwnd);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.refusal_reason, None);
    assert_eq!(diag.target_visible, Some(true));
    assert_eq!(diag.target_cloaked, Some(false));
    assert_eq!(diag.target_hidden, Some(false));
    assert_eq!(diag.fg_identity, Some("other"));
    // Gate-time attribution replays the veto's own sample: readable
    // fullscreen nonshell fixture app, relation `other` like the
    // post-veto sample here (they diverge only on a mid-call race).
    assert_eq!(diag.fg_gate_identity, Some("other"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert_eq!(diag.fg_readable, Some(true));
    assert_eq!(diag.fg_shell, Some(false));
    assert_eq!(diag.fg_fullscreen, Some(true));
    assert_eq!(diag.fg_visible, Some(true));
    assert_eq!(diag.fg_exe.as_deref(), Some("app.exe"));
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
}

/// Gate-time veto attribution: a readable captionless-fullscreen shell
/// foreground (Explorer desktop) vetoes with its gate-time relation plus
/// shell/fullscreen/visibility facts and the exe basename (never a path).
#[test]
fn focus_write_veto_diag_readable_fullscreen_shell() {
    let mut env = env_with(110_320);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_320, 2320, "proc-320");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    let fg_key = win_key(110_321, 2321, "proc-321");
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    fg_facts.shell = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            fg_key.hwnd,
            observed_for_with_exe(
                &fg_key,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Windows\\explorer.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(fg_key.hwnd, fg_key.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(fg_key.hwnd));
    S_FG.with(|m| *m.borrow_mut() = fg_key.hwnd);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_identity, Some("other"));
    assert_eq!(diag.fg_gate_identity, Some("other"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert_eq!(diag.fg_readable, Some(true));
    assert_eq!(diag.fg_shell, Some(true));
    assert_eq!(diag.fg_fullscreen, Some(true));
    assert_eq!(diag.fg_visible, Some(true));
    assert_eq!(diag.fg_exe.as_deref(), Some("explorer.exe"));
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
}

/// Gate-time veto attribution: a readable captionless-fullscreen nonshell
/// foreground (game) vetoes identically except shell/exe, so the next live
/// leg separates shell from game without a class probe.
#[test]
fn focus_write_veto_diag_readable_fullscreen_nonshell() {
    let mut env = env_with(110_322);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_322, 2322, "proc-322");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    let fg_key = win_key(110_323, 2323, "proc-323");
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            fg_key.hwnd,
            observed_for_with_exe(
                &fg_key,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Games\\game.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(fg_key.hwnd, fg_key.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(fg_key.hwnd));
    S_FG.with(|m| *m.borrow_mut() = fg_key.hwnd);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert_eq!(diag.fg_readable, Some(true));
    assert_eq!(diag.fg_shell, Some(false));
    assert_eq!(diag.fg_fullscreen, Some(true));
    assert_eq!(diag.fg_exe.as_deref(), Some("game.exe"));
    assert_eq!(diag.fg_gate_identity, Some("other"));
}

/// Gate-time veto attribution: an invisible unreadable foreign foreground
/// (no observation, no departure) vetoes as `unreadable` with no fg facts.
#[test]
fn focus_write_veto_diag_unreadable_foreign() {
    let mut env = env_with(110_324);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_324, 2324, "proc-324");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    // Foreign foreground: no observation entry and not visible, so the
    // visible read fails Unknown and the hidden lane has no departure.
    S_FG.with(|m| *m.borrow_mut() = 990_324);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_gate_identity, Some("other"));
    assert_eq!(diag.fg_veto_kind, Some("unreadable"));
    assert_eq!(diag.fg_readable, Some(false));
    assert_eq!(diag.fg_shell, None);
    assert_eq!(diag.fg_fullscreen, None);
    assert_eq!(diag.fg_visible, Some(false));
    assert_eq!(diag.fg_exe, None);
}

/// Gate-time veto attribution: a visible-but-unobservable foreign
/// foreground (elevated/foreign-session analogue: visible yet the read
/// fails) vetoes as `unreadable`, never as a departure.
#[test]
fn focus_write_veto_diag_unreadable_visible_foreign() {
    let mut env = env_with(110_325);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_325, 2325, "proc-325");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    // Visible HWND with no observation entry: the visible read fails
    // while `is_visible` still holds, so the veto records `unreadable`
    // before any departure check.
    S_VISIBLE.with(|m| m.borrow_mut().insert(990_325));
    S_FG.with(|m| *m.borrow_mut() = 990_325);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("unreadable"));
    assert_eq!(diag.fg_readable, Some(false));
    assert_eq!(diag.fg_shell, None);
    // The gate's own visibility read rides the detail: visible here.
    assert_eq!(diag.fg_visible, Some(true));
}

/// Gate-time veto attribution: a stale prior hidden claim without a
/// same-pass departure record is `unreadable`, never `hidden-departure`,
/// even with an exact committed claim and fresh hidden facts.
#[test]
fn focus_write_veto_diag_stale_claim_is_unreadable() {
    let mut env = env_with(110_326);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_326, 2326, "proc-326");
    let departed = win_key(110_327, 2327, "proc-327");
    admit_member(&mut env, &target, &ws, "w1", false);
    admit_member(&mut env, &departed, &ws, "w2", false);
    script_live(&target, "w1", rect(0, 0, 800, 600), plain_facts());
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            departed.hwnd,
            observed_for(&departed, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(departed.hwnd, departed.pid));
    S_IDENT.with(|m| m.borrow_mut().insert(departed.hwnd, identity_of(&departed)));
    S_TAG.with(|m| {
        m.borrow_mut()
            .insert(departed.hwnd, "member-tag-1".to_owned())
    });
    env.state.hidden_claims.insert(
        departed.clone(),
        super::super::HiddenRecord {
            claim: valid_claim(departed.hwnd, &identity_of(&departed)),
            iconic: false,
        },
    );
    S_FG.with(|m| *m.borrow_mut() = departed.hwnd);
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_gate_identity, Some("other"));
    assert_eq!(diag.fg_veto_kind, Some("unreadable"));
    assert_eq!(diag.fg_readable, Some(false));
    assert_eq!(diag.fg_visible, Some(false));
}

/// Gate-time veto attribution: the same-pass hidden departure with
/// fullscreen hidden facts vetoes as `hidden-departure` carrying the
/// hidden read's facts, proving the departure lane was validated.
#[test]
fn focus_write_veto_diag_hidden_departure_fullscreen() {
    let mut env = env_with(110_328);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let out = env.state.output_token(DEV1);
    let leaving = win_key(110_328, 2328, "proc-328");
    let entering = win_key(110_329, 2329, "proc-329");
    admit_member(&mut env, &leaving, &ws1, "w1", false);
    admit_member(&mut env, &entering, &ws2, "w2", false);
    env.state.hidden_claims.insert(
        entering.clone(),
        super::super::HiddenRecord {
            claim: script_claim(entering.hwnd, &identity_of(&entering)),
            iconic: false,
        },
    );
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            leaving.hwnd,
            observed_for_with_exe(
                &leaving,
                "w1",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Games\\game.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(leaving.hwnd, leaving.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(leaving.hwnd));
    S_TAG.with(|m| {
        m.borrow_mut()
            .insert(leaving.hwnd, "member-tag-1".to_owned())
    });
    S_IDENT.with(|m| m.borrow_mut().insert(leaving.hwnd, identity_of(&leaving)));
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            entering.hwnd,
            observed_for(&entering, "w2", rect(0, 0, 800, 600), plain_facts()),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(entering.hwnd, entering.pid));
    for key in [&leaving, &entering] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    S_IDENT.with(|m| m.borrow_mut().insert(entering.hwnd, identity_of(&entering)));
    S_FG.with(|m| *m.borrow_mut() = leaving.hwnd);
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let mut native =
        assemble_with_views(&mut env, moving_prims(), Some((out.clone(), ws2.clone())));
    assert_eq!(native.switch_view_nonactivating(&out, &ws2), Ok(true));
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w2");
    assert_eq!(outcome, Ok(false));
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_gate_identity, Some("other"));
    assert_eq!(diag.fg_veto_kind, Some("hidden-departure"));
    assert_eq!(diag.fg_readable, Some(false));
    assert_eq!(diag.fg_fullscreen, Some(true));
    assert_eq!(diag.fg_visible, Some(false));
    assert_eq!(diag.fg_exe.as_deref(), Some("game.exe"));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
}

/// Gate-time vs post-veto race: the foreground moves between the veto's
/// own sample and the diagnostic sample, so the gate-time relation keeps
/// the veto's `other` while the post-veto sample reads `target`.
#[test]
fn focus_write_veto_diag_gate_time_beats_later_sample() {
    let mut env = env_with(110_330);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_330, 2330, "proc-330");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    let fg_key = win_key(110_331, 2331, "proc-331");
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            fg_key.hwnd,
            observed_for(&fg_key, "fg", rect(0, 0, 1920, 1080), fg_facts),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(fg_key.hwnd, fg_key.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(fg_key.hwnd));
    // The veto's own sample sees the fullscreen competitor; the later
    // post-veto helper sample sees the target (foreground raced home).
    // The readable-fullscreen path takes exactly one gate read, then the
    // single diagnostic sample.
    S_FG_SEQ.with(|m| *m.borrow_mut() = vec![fg_key.hwnd, key.hwnd]);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert_eq!(diag.fg_gate_identity, Some("other"));
    assert_eq!(diag.fg_identity, Some("target"));
}

/// Trace gating for the gate-time detail: off-trace vetoes and traced
/// successes carry no gate-time fields and probe nothing extra.
#[test]
fn focus_write_veto_diag_gate_detail_trace_gated() {
    // Off-trace veto: identical refusal outcome with no gate-time detail.
    let mut env = env_with(110_332);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_332, 2332, "proc-332");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    let fg_key = win_key(110_333, 2333, "proc-333");
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            fg_key.hwnd,
            observed_for(&fg_key, "fg", rect(0, 0, 1920, 1080), fg_facts),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(fg_key.hwnd, fg_key.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(fg_key.hwnd));
    S_FG.with(|m| *m.borrow_mut() = fg_key.hwnd);
    let mut native = assemble(&mut env, scripted());
    let before = S_FG_CALLS.with(|m| *m.borrow());
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert_eq!(diag.refusal_gate, None);
    assert_eq!(diag.fg_gate_identity, None);
    assert_eq!(diag.fg_veto_kind, None);
    assert_eq!(diag.fg_readable, None);
    assert_eq!(diag.fg_shell, None);
    assert_eq!(diag.fg_fullscreen, None);
    assert_eq!(diag.fg_visible, None);
    assert_eq!(diag.fg_exe, None);
    let off_trace_reads = S_FG_CALLS.with(|m| *m.borrow()) - before;
    // Traced veto: the same gate reads plus exactly one diagnostic
    // sample; the gate-time detail rides without any further probe.
    let mut env = env_with(110_334);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_334, 2334, "proc-334");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    let fg_key = win_key(110_335, 2335, "proc-335");
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            fg_key.hwnd,
            observed_for(&fg_key, "fg", rect(0, 0, 1920, 1080), fg_facts),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(fg_key.hwnd, fg_key.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(fg_key.hwnd));
    S_FG.with(|m| *m.borrow_mut() = fg_key.hwnd);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let before = S_FG_CALLS.with(|m| *m.borrow());
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert_eq!(diag.fg_gate_identity, Some("other"));
    assert_eq!(
        S_FG_CALLS.with(|m| *m.borrow()) - before,
        off_trace_reads + 1,
        "traced focus-write veto adds only the one diagnostic sample"
    );
    // Traced success: no refusal detail at all.
    let mut env = env_with(110_336);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_337, 2337, "proc-337");
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w2");
    assert_eq!(outcome, Ok(true));
    assert_eq!(diag.refusal_gate, None);
    assert_eq!(diag.fg_gate_identity, None);
    assert_eq!(diag.fg_veto_kind, None);
    assert_eq!(diag.fg_exe, None);
}

/// Storage minimality: the gate-time stash stays `None` on a traced
/// success (readable non-fullscreen pass) and on any off-trace veto.
/// Diagnostics-only means no exe allocation and no stored detail outside
/// a traced refusal.
#[test]
fn focus_veto_detail_none_on_success_and_off_trace() {
    // Traced success: readable non-fullscreen foreground passes, and no
    // gate-time detail is stored (this is the former exe-allocation site).
    let mut env = env_with(110_340);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let other = win_key(110_339, 2339, "proc-339");
    let target = win_key(110_340, 2340, "proc-340");
    admit_member(&mut env, &other, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&other, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = other.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w2");
    assert_eq!(outcome, Ok(true));
    assert_eq!(diag.fg_gate_identity, None);
    assert_eq!(diag.fg_veto_kind, None);
    assert_eq!(diag.fg_exe, None);
    assert!(
        native.focus_veto_detail.is_none(),
        "traced success must store no gate-time detail"
    );
    // Off-trace veto: readable fullscreen refuses, and no gate-time
    // detail is stored either.
    let mut env = env_with(110_341);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_341, 2341, "proc-341");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    let fg_key = win_key(110_342, 2342, "proc-342");
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            fg_key.hwnd,
            observed_for(&fg_key, "fg", rect(0, 0, 1920, 1080), fg_facts),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(fg_key.hwnd, fg_key.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(fg_key.hwnd));
    S_FG.with(|m| *m.borrow_mut() = fg_key.hwnd);
    let mut native = assemble(&mut env, scripted());
    let (outcome, _) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(
        native.focus_veto_detail.is_none(),
        "off-trace veto must store no gate-time detail"
    );
}

/// Builder laziness: `note_focus_veto` runs its payload builder only
/// when trace is armed. This observes invocation itself, proving
/// off-trace never builds the payload (no exe allocation) rather than
/// merely discarding a prebuilt value.
#[test]
fn note_focus_veto_builds_only_when_traced() {
    use super::FocusWriteVetoDetail;
    let mut env = env_with(110_343);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_343, 2343, "proc-343");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    let built = std::cell::RefCell::new(false);
    let builder = || {
        *built.borrow_mut() = true;
        FocusWriteVetoDetail {
            gate_identity: "other",
            kind: "unreadable",
            readable: false,
            ..FocusWriteVetoDetail::default()
        }
    };
    // Off-trace: the builder never runs and nothing is stored.
    native.note_focus_veto(builder);
    assert!(
        !*built.borrow(),
        "off-trace must not invoke the payload builder"
    );
    assert!(native.focus_veto_detail.is_none());
    // Traced: the builder runs exactly once and the detail is stored.
    // (`builder` moved above, so rebuild an identical one here.)
    native.set_diagnostics_trace(true);
    native.note_focus_veto(|| {
        *built.borrow_mut() = true;
        FocusWriteVetoDetail {
            gate_identity: "other",
            kind: "unreadable",
            readable: false,
            ..FocusWriteVetoDetail::default()
        }
    });
    assert!(*built.borrow());
    assert!(native.focus_veto_detail.is_some());
}

#[test]
fn focus_refusal_diag_reports_move_size_veto() {
    let mut env = env_with(110_305);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_305, 2305, "proc-305");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    // A ledger-hidden target still dispatches no setter while its thread
    // runs a move/size loop: the hidden-at-dispatch flag reads true.
    env.state.hidden_claims.insert(
        key.clone(),
        super::super::HiddenRecord {
            claim: valid_claim(key.hwnd, &identity_of(&key)),
            iconic: false,
        },
    );
    S_MOVE_SIZE.with(|m| m.borrow_mut().insert(key.hwnd, Some(true)));
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("move-size-veto"));
    assert_eq!(diag.refusal_reason, None);
    assert_eq!(diag.target_visible, Some(true));
    assert_eq!(diag.target_cloaked, Some(false));
    assert_eq!(diag.target_hidden, Some(true));
    assert_eq!(diag.fg_identity, Some("none"));
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
    S_MOVE_SIZE.with(|m| m.borrow_mut().remove(&key.hwnd));
}

#[test]
fn focus_refusal_diag_off_trace_reports_no_detail() {
    let mut env = env_with(110_306);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_306, 2306, "proc-306");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_MOVE_SIZE.with(|m| m.borrow_mut().insert(key.hwnd, Some(true)));
    // Trace off: identical refusal outcome with no diagnostic detail.
    let mut native = assemble(&mut env, scripted());
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, None);
    assert_eq!(diag.refusal_reason, None);
    assert_eq!(diag.target_visible, None);
    assert_eq!(diag.target_cloaked, None);
    assert_eq!(diag.target_hidden, None);
    assert_eq!(diag.fg_identity, None);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
    S_MOVE_SIZE.with(|m| m.borrow_mut().remove(&key.hwnd));
}

#[test]
fn focus_refusal_diag_success_reports_no_detail() {
    let mut env = env_with(110_307);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let old = win_key(110_307, 2307, "proc-307");
    let target = win_key(110_308, 2308, "proc-308");
    admit_member(&mut env, &old, &ws, "w1", false);
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&old, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FG.with(|m| *m.borrow_mut() = old.hwnd);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut prims = scripted();
    prims.focus = |hwnd| {
        S_FOCUS_CALLS.with(|m| m.borrow_mut().push(hwnd));
        S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
    };
    let mut native = assemble(&mut env, prims);
    // Trace armed: the accepted setter still carries no refusal detail,
    // with exactly one prime and one setter as before.
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w2");
    assert_eq!(outcome, Ok(true));
    assert!(diag.primitive_called);
    assert_eq!(diag.refusal_gate, None);
    assert_eq!(diag.refusal_reason, None);
    assert_eq!(diag.target_visible, None);
    assert_eq!(diag.target_cloaked, None);
    assert_eq!(diag.target_hidden, None);
    assert_eq!(diag.fg_identity, None);
    assert_eq!(S_PRIME_CALLS.with(|m| m.borrow().clone()), vec![0xE8]);
    assert_eq!(
        S_FOCUS_CALLS.with(|m| m.borrow().clone()),
        vec![target.hwnd]
    );
}

#[test]
fn focus_refusal_diag_reports_cloaked_target() {
    // Cloak engages after admission: the target still observes (visible
    // lane) but classifies `cloaked`, so the refusal carries the reason
    // with the cloaked target bit set.
    let mut env = env_with(110_309);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_309, 2309, "proc-309");
    admit_member(&mut env, &key, &ws, "w1", false);
    let mut facts = plain_facts();
    facts.cloaked = true;
    script_live(&key, "w1", rect(0, 0, 800, 600), facts);
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("classify-focus"));
    assert_eq!(diag.refusal_reason, Some("cloaked"));
    assert_eq!(diag.target_visible, Some(true));
    assert_eq!(diag.target_cloaked, Some(true));
    assert_eq!(diag.target_hidden, Some(false));
    assert_eq!(diag.fg_identity, Some("none"));
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
}

#[test]
fn focus_refusal_diag_reports_raced_invisible_target() {
    // Production hide race (`observe.rs`: the visible gate passes, then the
    // facts re-read observes the window hidden): the observation succeeds
    // with `visible: false` facts, so `classify-focus` refuses with reason
    // `hidden` and the diag reports the raced target bit. A fully hidden
    // window that fails the observation gate instead takes the `Err`
    // branch, never `Ok(false)`: no test fabricates that path.
    let mut env = env_with(110_310);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_310, 2310, "proc-310");
    admit_member(&mut env, &key, &ws, "w1", false);
    let mut facts = plain_facts();
    facts.visible = false;
    script_live(&key, "w1", rect(0, 0, 800, 600), facts);
    env.state.applied_views.insert(out, ws);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("classify-focus"));
    assert_eq!(diag.refusal_reason, Some("hidden"));
    assert_eq!(diag.target_visible, Some(false));
    assert_eq!(diag.target_cloaked, Some(false));
    assert_eq!(diag.target_hidden, Some(false));
    assert_eq!(diag.fg_identity, Some("none"));
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
}

/// Foreground probe budget: the diagnostic sample is the only trace-gated
/// read. Off-trace refusals and success (traced or not) add zero reads;
/// a traced refusal adds exactly one on top of the vetoes' own reads.
#[test]
fn focus_refusal_diag_foreground_probe_budget() {
    fn fg_calls() -> u64 {
        S_FG_CALLS.with(|m| *m.borrow())
    }
    // Scope branch: no veto reads at all.
    let mut env = env_with(110_311);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_311, 2311, "proc-311");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    env.state.scope = vec!["other.exe".to_owned()];
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window_traced("w1").0, Ok(false));
    assert_eq!(fg_calls(), 0, "off-trace scope refusal probes nothing");
    let mut env = env_with(110_312);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_312, 2312, "proc-312");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    env.state.scope = vec!["other.exe".to_owned()];
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w1");
    assert_eq!(outcome, Ok(false));
    assert_eq!(diag.refusal_gate, Some("scope"));
    assert_eq!(fg_calls(), 1, "traced scope refusal adds one sample");
    // Move/size branch: the traced refusal adds exactly one read over the
    // veto's own reads, whatever those are.
    let mut env = env_with(110_313);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_313, 2313, "proc-313");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_MOVE_SIZE.with(|m| m.borrow_mut().insert(key.hwnd, Some(true)));
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window_traced("w1").0, Ok(false));
    let veto_reads = fg_calls();
    let mut env = env_with(110_314);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(110_314, 2314, "proc-314");
    admit_member(&mut env, &key, &ws, "w1", false);
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_MOVE_SIZE.with(|m| m.borrow_mut().insert(key.hwnd, Some(true)));
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    assert_eq!(native.focus_window_traced("w1").0, Ok(false));
    assert_eq!(
        fg_calls(),
        veto_reads + 1,
        "traced veto refusal adds one sample"
    );
    S_MOVE_SIZE.with(|m| {
        m.borrow_mut().remove(&110_313);
        m.borrow_mut().remove(&key.hwnd);
    });
    // Success: trace arming adds no reads (vetoes read identically either
    // way, and the accepted path takes no sample).
    let mut env = env_with(110_315);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_316, 2316, "proc-316");
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut native = assemble(&mut env, scripted());
    assert_eq!(native.focus_window_traced("w2").0, Ok(true));
    let untraced_reads = fg_calls();
    let mut env = env_with(110_317);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(110_318, 2318, "proc-318");
    admit_member(&mut env, &target, &ws, "w2", false);
    script_live(&target, "w2", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out, ws);
    S_FOCUS_OK.with(|m| m.borrow_mut().insert(target.hwnd, true));
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    assert_eq!(native.focus_window_traced("w2").0, Ok(true));
    assert_eq!(fg_calls(), untraced_reads, "trace adds no reads on success");
}

/// Workspace-return regression, occupied -> empty -> occupied: the
/// empty-workspace visit leaves the exact desktop holding the foreground,
/// and the return leg's remembered-target arrival focus effect must reach
/// the native setter through the explicit-selection desktop exception.
/// Failed before the fix: the readable-fullscreen desktop vetoed before the
/// primitive ran (user run-01dd65cae8797264 L97/103).
#[test]
fn selection_return_past_desktop_foreground_dispatches_setter() {
    use super::super::world_diff::SelectionArrivalGrant;
    use tiler_core::directional::OutputId as CoreOutputId;

    let mut env = env_with(120_001);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let key = win_key(120_001, 2401, "proc-401");
    admit_member(&mut env, &key, &ws, "w6", false);
    script_live(&key, "w6", rect(0, 0, 800, 600), plain_facts());
    env.state.applied_views.insert(out.clone(), ws.clone());
    // Exact desktop holds the foreground after the empty-workspace visit:
    // readable captionless fullscreen shell owned by Explorer.
    let desktop = win_key(120_002, 2402, "proc-402");
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    fg_facts.shell = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            desktop.hwnd,
            observed_for_with_exe(
                &desktop,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Windows\\explorer.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(desktop.hwnd, desktop.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(desktop.hwnd));
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            desktop.hwnd,
            ProcessIdentity {
                pid: desktop.pid,
                process_creation: desktop.creation.clone(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Windows\\explorer.exe".to_owned(),
            },
        )
    });
    S_SHELL.with(|m| *m.borrow_mut() = desktop.hwnd);
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = Some("C:\\Windows\\explorer.exe".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = desktop.hwnd);
    // Return-leg reply: remembered-target arrival focus effect on the
    // occupied workspace, exactly as the core mints it (`fx-<seq>`).
    let observed = vec![observed_for(
        &key,
        "w6",
        rect(0, 0, 800, 600),
        plain_facts(),
    )];
    let (mut diff, fresh) = convert(&env, &observed);
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    reply.effects.push(OneShotEffect {
        id: "fx-7".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w6".to_owned()),
        issued_revision: 1,
    });
    // Authority binding through the production mint: the exact accepted
    // select command, its sequence, and this reply's own explicit effect.
    let command = WorldCommand::SelectWorkspace {
        output: CoreOutputId(out.clone()),
        workspace: ws.clone(),
    };
    let grant = SelectionArrivalGrant::for_accepted(Some(&command), Some(7), &reply, "accepted");
    assert!(grant.is_some(), "select must mint the arrival grant");
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    native.set_selection_arrival(grant);
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
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert_eq!(row.id, "fx-7");
    assert_eq!(row.target, "w6");
    assert!(row.dispatched);
    assert!(
        row.primitive_called,
        "selection-return arrival must reach the setter past the desktop foreground"
    );
    assert!(
        row.desktop_exception,
        "the dispatch row must mark the desktop exception"
    );
}

/// Shared rig for the selection-arrival exception tests: a healthy arrival
/// target plus a healthy exact-desktop foreground (readable fullscreen
/// shell, full identity agreement, live shell handle, system Explorer
/// path). Each negative mutates exactly one lane.
fn script_arrival_target(env: &mut Env, key: &WindowKey, token: &str, ws: &str) {
    admit_member(env, key, ws, token, false);
    script_live(key, token, rect(0, 0, 800, 600), plain_facts());
    let out = env.state.output_token(DEV1);
    env.state.applied_views.insert(out, ws.to_owned());
}

fn script_healthy_desktop(desktop: &WindowKey) {
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    fg_facts.shell = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            desktop.hwnd,
            observed_for_with_exe(
                desktop,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Windows\\explorer.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(desktop.hwnd, desktop.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(desktop.hwnd));
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            desktop.hwnd,
            ProcessIdentity {
                pid: desktop.pid,
                process_creation: desktop.creation.clone(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Windows\\explorer.exe".to_owned(),
            },
        )
    });
    S_SHELL.with(|m| *m.borrow_mut() = desktop.hwnd);
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = Some("C:\\Windows\\explorer.exe".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = desktop.hwnd);
}

/// Arrival reply carrying one core-shaped explicit focus effect.
fn fx_reply(out: &str, ws: &str, id: &str, target: &str) -> WorldReply {
    let mut reply = reply_shell(7, 1, &[(out, ws)]);
    reply.effects.push(OneShotEffect {
        id: id.to_owned(),
        kind: EffectKind::Focus,
        target: WindowId(target.to_owned()),
        issued_revision: 1,
    });
    reply
}

/// Production grant mint for one accepted select reply.
fn select_grant(out: &str, ws: &str, reply: &WorldReply) -> Option<SelectionArrivalGrant> {
    let command = WorldCommand::SelectWorkspace {
        output: OutputId(out.to_owned()),
        workspace: ws.to_owned(),
    };
    // Handcrafted replies in this file mint `fx-7`: the sequence must name
    // it exactly, or no explicit effect matches and nothing mints.
    SelectionArrivalGrant::for_accepted(Some(&command), Some(7), reply, "accepted")
}

/// One effect-bound arrival dispatch through the production seam.
fn arrival_call(
    env: &mut Env,
    grant: Option<SelectionArrivalGrant>,
    token: &str,
    effect: &str,
) -> (Result<bool, ()>, FocusCallDiag) {
    let mut native = assemble(env, scripted());
    native.set_diagnostics_trace(true);
    native.set_selection_arrival(grant);
    native.focus_window_traced_selected(token, effect)
}

/// The exception passes a fully proven desktop: setter runs once after one
/// prime, and the record marks the exception.
#[test]
fn selection_arrival_passes_proven_desktop_direct() {
    let mut env = env_with(120_010);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_010, 2410, "proc-410");
    let desktop = win_key(120_011, 2411, "proc-411");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    assert!(grant.is_some());
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(true));
    assert!(diag.primitive_called);
    assert_eq!(diag.native_return, Some(true));
    assert!(diag.desktop_exception);
    assert_eq!(S_PRIME_CALLS.with(|m| m.borrow().len()), 1);
}

/// A competing fullscreen game still vetoes with the grant armed: the
/// handle is not the live shell window.
#[test]
fn selection_arrival_still_vetoes_fullscreen_game() {
    let mut env = env_with(120_020);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_020, 2420, "proc-420");
    let game = win_key(120_021, 2421, "proc-421");
    script_arrival_target(&mut env, &target, "w6", &ws);
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            game.hwnd,
            observed_for_with_exe(
                &game,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Games\\game.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(game.hwnd, game.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(game.hwnd));
    S_FG.with(|m| *m.borrow_mut() = game.hwnd);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// An unknown fullscreen app with the shell handle equal to it still
/// vetoes: the executable is not the system shell.
#[test]
fn selection_arrival_still_vetoes_unknown_fullscreen_app() {
    let mut env = env_with(120_030);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_030, 2430, "proc-430");
    let foreign = win_key(120_031, 2431, "proc-431");
    script_arrival_target(&mut env, &target, "w6", &ws);
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            foreign.hwnd,
            observed_for_with_exe(
                &foreign,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Apps\\unknown.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(foreign.hwnd, foreign.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(foreign.hwnd));
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            foreign.hwnd,
            ProcessIdentity {
                pid: foreign.pid,
                process_creation: foreign.creation.clone(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Apps\\unknown.exe".to_owned(),
            },
        )
    });
    // Even with the shell handle naming this window, the executable proof
    // fails closed.
    S_SHELL.with(|m| *m.borrow_mut() = foreign.hwnd);
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = Some("C:\\Windows\\explorer.exe".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = foreign.hwnd);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// An unreadable shell-handle foreground vetoes: nothing proves the
/// desktop without a readable observation.
#[test]
fn selection_arrival_unreadable_shell_vetoes() {
    let mut env = env_with(120_040);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_040, 2440, "proc-440");
    script_arrival_target(&mut env, &target, "w6", &ws);
    // Shell handle names an unreadable window: no observation entry and
    // not visible, so the gate fails closed before any desktop proof.
    S_SHELL.with(|m| *m.borrow_mut() = 990_040);
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = Some("C:\\Windows\\explorer.exe".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = 990_040);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("unreadable"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// A spoofed Progman-class window that is not the live shell handle
/// vetoes: class and basename alone are never authority.
#[test]
fn selection_arrival_spoof_progman_vetoes() {
    let mut env = env_with(120_050);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_050, 2450, "proc-450");
    let spoof = win_key(120_051, 2451, "proc-451");
    script_arrival_target(&mut env, &target, "w6", &ws);
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    fg_facts.shell = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            spoof.hwnd,
            observed_for_with_exe(
                &spoof,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Windows\\explorer.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(spoof.hwnd, spoof.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(spoof.hwnd));
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            spoof.hwnd,
            ProcessIdentity {
                pid: spoof.pid,
                process_creation: spoof.creation.clone(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Windows\\explorer.exe".to_owned(),
            },
        )
    });
    // Live shell handle is a different window: the spoof's class, facts,
    // and basename prove nothing.
    S_SHELL.with(|m| *m.borrow_mut() = 991_051);
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = Some("C:\\Windows\\explorer.exe".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = spoof.hwnd);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// A fullscreen WorkerW surface (same Explorer process, desktop geometry)
/// that is not the exact shell handle vetoes.
#[test]
fn selection_arrival_workerw_surface_vetoes() {
    let mut env = env_with(120_060);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_060, 2460, "proc-460");
    let worker = win_key(120_061, 2461, "proc-461");
    script_arrival_target(&mut env, &target, "w6", &ws);
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    fg_facts.shell = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            worker.hwnd,
            observed_for_with_exe(
                &worker,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Windows\\explorer.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(worker.hwnd, worker.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(worker.hwnd));
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            worker.hwnd,
            ProcessIdentity {
                pid: worker.pid,
                process_creation: worker.creation.clone(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Windows\\explorer.exe".to_owned(),
            },
        )
    });
    // Exact desktop is a different handle in the same process.
    S_SHELL.with(|m| *m.borrow_mut() = 991_061);
    S_PID.with(|m| m.borrow_mut().insert(991_061, worker.pid));
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = Some("C:\\Windows\\explorer.exe".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = worker.hwnd);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// A recycled shell handle (same HWND number, new process creation)
/// vetoes: HWND/PID/creation must agree across the gate's own reads.
#[test]
fn selection_arrival_recycled_shell_identity_vetoes() {
    let mut env = env_with(120_070);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_070, 2470, "proc-470");
    let desktop = win_key(120_071, 2471, "proc-471");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    // Same-process HWND reuse: the live process creation no longer matches
    // the observation read.
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            desktop.hwnd,
            ProcessIdentity {
                pid: desktop.pid,
                process_creation: "proc-471-recycled".to_owned(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Windows\\explorer.exe".to_owned(),
            },
        )
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// A non-system Explorer executable behind the shell handle vetoes: the
/// path must equal the trusted system directory derivation.
#[test]
fn selection_arrival_wrong_shell_executable_vetoes() {
    let mut env = env_with(120_080);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_080, 2480, "proc-480");
    let desktop = win_key(120_081, 2481, "proc-481");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    // Consistent reads, but the executable is not the system shell.
    S_OBSERVE.with(|m| {
        if let Some(entry) = m.borrow_mut().get_mut(&desktop.hwnd) {
            entry.identity.exe_path = "C:\\Evil\\explorer.exe".to_owned();
        }
    });
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            desktop.hwnd,
            ProcessIdentity {
                pid: desktop.pid,
                process_creation: desktop.creation.clone(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Evil\\explorer.exe".to_owned(),
            },
        )
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// A shell process under a different user/session vetoes.
#[test]
fn selection_arrival_shell_user_session_mismatch_vetoes() {
    let mut env = env_with(120_090);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_090, 2490, "proc-490");
    let desktop = win_key(120_091, 2491, "proc-491");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    // Consistent reads under a foreign session.
    S_OBSERVE.with(|m| {
        if let Some(entry) = m.borrow_mut().get_mut(&desktop.hwnd) {
            entry.identity.user_sid = "S-1-5-21-9999".to_owned();
            entry.identity.session_id = 9;
        }
    });
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            desktop.hwnd,
            ProcessIdentity {
                pid: desktop.pid,
                process_creation: desktop.creation.clone(),
                user_sid: "S-1-5-21-9999".to_owned(),
                session_id: 9,
                exe_path: "C:\\Windows\\explorer.exe".to_owned(),
            },
        )
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// An unavailable shell handle (`GetShellWindow` zero) vetoes: a null
/// foreground proves nothing.
#[test]
fn selection_arrival_missing_shell_handle_vetoes() {
    let mut env = env_with(120_100);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_100, 2500, "proc-500");
    let desktop = win_key(120_101, 2501, "proc-501");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    S_SHELL.with(|m| *m.borrow_mut() = 0);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// A foreground race to a game between the gate proof and the setter
/// refuses: the post-prime recheck requires a fresh equal proof. The prime
/// already ran (ordering preserved), but the setter never does.
#[test]
fn selection_arrival_foreground_race_to_game_refuses_after_prime() {
    let mut env = env_with(120_110);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_110, 2510, "proc-510");
    let desktop = win_key(120_111, 2511, "proc-511");
    let game = win_key(120_112, 2512, "proc-512");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    let mut game_facts = plain_facts();
    game_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            game.hwnd,
            observed_for_with_exe(
                &game,
                "fg",
                rect(0, 0, 1920, 1080),
                game_facts,
                "C:\\Games\\game.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(game.hwnd, game.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(game.hwnd));
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    // Race driven by prime state, not read counts: the foreground names
    // the desktop until the prime runs, then the game.
    S_FG_AFTER_PRIME.with(|m| *m.borrow_mut() = Some(game.hwnd));
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    // The recheck refusal names its reason distinctly from a gate-time
    // veto, and still records the prime that ran before the race.
    assert_eq!(diag.refusal_reason, Some("arrival-recheck"));
    assert_eq!(diag.prime_inserted, Some(2));
    // The gate did pass through the exception before the race, so the row
    // marks it even though the setter never ran.
    assert!(diag.desktop_exception);
    assert_eq!(S_PRIME_CALLS.with(|m| m.borrow().len()), 1);
}

/// Shared assertions for an exception-path post-prime desktop/modal
/// refusal: no setter, named gate plus reason, the actual prime outcome,
/// and the gate-pass marker (never an actuation claim).
fn assert_recheck_refusal(diag: &FocusCallDiag, gate: &'static str, reason: Option<&'static str>) {
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some(gate));
    assert_eq!(diag.refusal_reason, reason);
    assert_eq!(diag.prime_inserted, Some(2));
    assert!(diag.desktop_exception);
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
    assert_eq!(S_PRIME_CALLS.with(|m| m.borrow().len()), 1);
}

/// Shared assertions for an exception-path post-prime ambiguous hold
/// (hook or target drift): no setter, no refusal gate (at-most-once, like
/// entry), but the prime outcome and gate-pass marker still ride.
fn assert_recheck_ambiguous(outcome: &Result<bool, ()>, diag: &FocusCallDiag) {
    assert_eq!(outcome, &Err(()));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, None);
    assert_eq!(diag.prime_inserted, Some(2));
    assert!(diag.desktop_exception);
    assert!(S_FOCUS_CALLS.with(|m| m.borrow().is_empty()));
    assert_eq!(S_PRIME_CALLS.with(|m| m.borrow().len()), 1);
}

/// A mismatched observed HWND never proves: the gate observation must name
/// the gate foreground itself.
#[test]
fn selection_arrival_mismatched_observed_hwnd_vetoes() {
    let mut env = env_with(120_160);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_160, 2560, "proc-560");
    let desktop = win_key(120_161, 2561, "proc-561");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    // Identity reads agree, but the observation names another window.
    S_OBSERVE.with(|m| {
        if let Some(entry) = m.borrow_mut().get_mut(&desktop.hwnd) {
            entry.hwnd = desktop.hwnd + 50;
        }
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// A shell handle that changes during the recheck refuses: the gate proof
/// passed, the final bracket does not.
#[test]
fn selection_arrival_shell_drift_during_recheck_refuses() {
    let mut env = env_with(120_161);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_162, 2562, "proc-562");
    let desktop = win_key(120_163, 2563, "proc-563");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    // Gate fast-path plus bracket agree; the recheck reads unavailable.
    // (The gate proof itself samples the shell handle twice.)
    S_SHELL_SEQ.with(|m| m.borrow_mut().extend([desktop.hwnd, desktop.hwnd, 0]));
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert_recheck_refusal(&diag, "focus-write-veto", Some("arrival-recheck"));
}

/// A shell PID that changes during the recheck proof refuses.
#[test]
fn selection_arrival_pid_drift_during_recheck_refuses() {
    let mut env = env_with(120_164);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_165, 2565, "proc-565");
    let desktop = win_key(120_166, 2566, "proc-566");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    // Two gate reads agree, then the recheck proof reads a recycled PID.
    S_PID_SEQ.with(|m| {
        m.borrow_mut().extend([
            (desktop.hwnd, desktop.pid),
            (desktop.hwnd, desktop.pid),
            (desktop.hwnd, 9999),
        ])
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert_recheck_refusal(&diag, "focus-write-veto", Some("arrival-recheck"));
}

/// A shell PID that changes inside the recheck's final bracket refuses:
/// the recheck proof passed, the closing HWND-to-PID read does not.
#[test]
fn selection_arrival_pid_drift_in_final_bracket_refuses() {
    let mut env = env_with(120_167);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_168, 2568, "proc-568");
    let desktop = win_key(120_169, 2569, "proc-569");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    // Gate pair plus recheck proof agree; only the bracket read drifts.
    S_PID_SEQ.with(|m| {
        m.borrow_mut().extend([
            (desktop.hwnd, desktop.pid),
            (desktop.hwnd, desktop.pid),
            (desktop.hwnd, desktop.pid),
            (desktop.hwnd, 9999),
        ])
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert_recheck_refusal(&diag, "focus-write-veto", Some("arrival-recheck"));
}

/// A shell process creation that changes during the recheck refuses.
#[test]
fn selection_arrival_creation_drift_during_recheck_refuses() {
    let mut env = env_with(120_170);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_171, 2571, "proc-571");
    let desktop = win_key(120_172, 2572, "proc-572");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    let desktop_ident = || ProcessIdentity {
        pid: desktop.pid,
        process_creation: desktop.creation.clone(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: "C:\\Windows\\explorer.exe".to_owned(),
    };
    // Gate identity pops healthy; only the recheck pops the recycled
    // creation.
    S_IDENT_SEQ.with(|m| {
        m.borrow_mut().extend([
            (desktop.hwnd, Some(desktop_ident())),
            (
                desktop.hwnd,
                Some(ProcessIdentity {
                    process_creation: "proc-572-recycled".to_owned(),
                    ..desktop_ident()
                }),
            ),
        ])
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert_recheck_refusal(&diag, "focus-write-veto", Some("arrival-recheck"));
}

/// A shell executable that changes during the recheck refuses.
#[test]
fn selection_arrival_exe_drift_during_recheck_refuses() {
    let mut env = env_with(120_173);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_174, 2574, "proc-574");
    let desktop = win_key(120_175, 2575, "proc-575");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    let desktop_ident = || ProcessIdentity {
        pid: desktop.pid,
        process_creation: desktop.creation.clone(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: "C:\\Windows\\explorer.exe".to_owned(),
    };
    S_IDENT_SEQ.with(|m| {
        m.borrow_mut().extend([
            (desktop.hwnd, Some(desktop_ident())),
            (
                desktop.hwnd,
                Some(ProcessIdentity {
                    exe_path: "C:\\Evil\\explorer.exe".to_owned(),
                    ..desktop_ident()
                }),
            ),
        ])
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert_recheck_refusal(&diag, "focus-write-veto", Some("arrival-recheck"));
}

/// A shell user/session that changes during the recheck refuses.
#[test]
fn selection_arrival_session_drift_during_recheck_refuses() {
    let mut env = env_with(120_176);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_177, 2577, "proc-577");
    let desktop = win_key(120_178, 2578, "proc-578");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    let desktop_ident = || ProcessIdentity {
        pid: desktop.pid,
        process_creation: desktop.creation.clone(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: "C:\\Windows\\explorer.exe".to_owned(),
    };
    S_IDENT_SEQ.with(|m| {
        m.borrow_mut().extend([
            (desktop.hwnd, Some(desktop_ident())),
            (
                desktop.hwnd,
                Some(ProcessIdentity {
                    user_sid: "S-1-5-21-9999".to_owned(),
                    session_id: 9,
                    ..desktop_ident()
                }),
            ),
        ])
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert_recheck_refusal(&diag, "focus-write-veto", Some("arrival-recheck"));
}

/// An unreadable shell identity during the recheck refuses: the gate
/// proof passed, the fresh proof cannot read.
#[test]
fn selection_arrival_unreadable_recheck_refuses() {
    let mut env = env_with(120_179);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_180, 2580, "proc-580");
    let desktop = win_key(120_181, 2581, "proc-581");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    let desktop_ident = || ProcessIdentity {
        pid: desktop.pid,
        process_creation: desktop.creation.clone(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: "C:\\Windows\\explorer.exe".to_owned(),
    };
    S_IDENT_SEQ.with(|m| {
        m.borrow_mut()
            .extend([(desktop.hwnd, Some(desktop_ident())), (desktop.hwnd, None)])
    });
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert_recheck_refusal(&diag, "focus-write-veto", Some("arrival-recheck"));
}

/// A hook arriving after the prime holds ambiguous: at-most-once, like
/// entry, with the prime outcome recorded.
#[test]
fn selection_arrival_hook_after_prime_holds_ambiguous() {
    let mut env = env_with(120_182);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_183, 2583, "proc-583");
    let desktop = win_key(120_184, 2584, "proc-584");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let mut prims = scripted();
    prims.hook_pending = || S_PRIMED.with(|m| *m.borrow());
    let mut native = assemble(&mut env, prims);
    native.set_diagnostics_trace(true);
    native.set_selection_arrival(grant);
    let (outcome, diag) = native.focus_window_traced_selected("w6", "fx-7");
    assert_recheck_ambiguous(&outcome, &diag);
}

/// A modal loop starting after the prime refuses under the existing
/// move-size vocabulary, with the prime outcome recorded.
#[test]
fn selection_arrival_modal_after_prime_refuses() {
    let mut env = env_with(120_185);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_186, 2586, "proc-586");
    let desktop = win_key(120_187, 2587, "proc-587");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    S_MODAL_AFTER_PRIME.with(|m| *m.borrow_mut() = Some(target.hwnd));
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert_recheck_refusal(&diag, "move-size-veto", None);
}

/// A target that departs after the prime holds ambiguous: at-most-once,
/// like entry, with the prime outcome recorded.
#[test]
fn selection_arrival_target_departure_after_prime_holds_ambiguous() {
    let mut env = env_with(120_188);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_189, 2589, "proc-589");
    let desktop = win_key(120_190, 2590, "proc-590");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    S_UNOBSERVE_AFTER_PRIME.with(|m| *m.borrow_mut() = Some(target.hwnd));
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_recheck_ambiguous(&outcome, &diag);
}

/// A fullscreen file-Explorer window that is not the exact shell handle
/// vetoes: same executable, wrong window.
#[test]
fn selection_arrival_fullscreen_explorer_window_vetoes() {
    let mut env = env_with(120_191);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_192, 2592, "proc-592");
    let browser = win_key(120_193, 2593, "proc-593");
    script_arrival_target(&mut env, &target, "w6", &ws);
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            browser.hwnd,
            observed_for_with_exe(
                &browser,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Windows\\explorer.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(browser.hwnd, browser.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(browser.hwnd));
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            browser.hwnd,
            ProcessIdentity {
                pid: browser.pid,
                process_creation: browser.creation.clone(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Windows\\explorer.exe".to_owned(),
            },
        )
    });
    // Exact desktop is a different handle: no exemption for fellow
    // Explorer windows.
    S_SHELL.with(|m| *m.borrow_mut() = 991_193);
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = Some("C:\\Windows\\explorer.exe".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = browser.hwnd);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// A fullscreen shell popup (Start surface) that is not the exact shell
/// handle vetoes: shell process family, wrong window and executable.
#[test]
fn selection_arrival_fullscreen_shell_popup_vetoes() {
    let mut env = env_with(120_194);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_195, 2595, "proc-595");
    let popup = win_key(120_196, 2596, "proc-596");
    script_arrival_target(&mut env, &target, "w6", &ws);
    let mut fg_facts = plain_facts();
    fg_facts.captionless_fullscreen = true;
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            popup.hwnd,
            observed_for_with_exe(
                &popup,
                "fg",
                rect(0, 0, 1920, 1080),
                fg_facts,
                "C:\\Windows\\SystemApps\\StartMenuExperienceHost.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(popup.hwnd, popup.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(popup.hwnd));
    S_IDENT.with(|m| {
        m.borrow_mut().insert(
            popup.hwnd,
            ProcessIdentity {
                pid: popup.pid,
                process_creation: popup.creation.clone(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Windows\\SystemApps\\StartMenuExperienceHost.exe".to_owned(),
            },
        )
    });
    S_SHELL.with(|m| *m.borrow_mut() = 991_196);
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = Some("C:\\Windows\\explorer.exe".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = popup.hwnd);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// Exact effect binding: a grant for (`fx-7`, `w6`) authorizes neither a
/// tampered id nor a tampered target.
#[test]
fn selection_arrival_rejects_ungranted_effect_id() {
    let mut env = env_with(120_120);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_120, 2520, "proc-520");
    let other = win_key(120_122, 2522, "proc-522");
    let desktop = win_key(120_121, 2521, "proc-521");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_arrival_target(&mut env, &other, "w9", &ws);
    script_healthy_desktop(&desktop);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    for (token, effect) in [("w6", "fx-8"), ("w9", "fx-7")] {
        let (outcome, diag) = arrival_call(&mut env, grant.clone(), token, effect);
        assert_eq!(outcome, Ok(false), "unbound pair must veto");
        assert!(!diag.primitive_called);
        assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
        assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
        assert!(!diag.desktop_exception);
        assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
    }
}

/// Without a grant the same desktop foreground vetoes: directional focus,
/// send/migrate follow, and observation effects never ride the exception.
#[test]
fn ungranted_arrival_still_vetoes_desktop() {
    let mut env = env_with(120_130);
    let ws = "ws1".to_owned();
    let target = win_key(120_130, 2530, "proc-530");
    let desktop = win_key(120_131, 2531, "proc-531");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    // Unbound lane (no effect id, no grant): existing behavior exactly.
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
    let (outcome, diag) = native.focus_window_traced("w6");
    assert_eq!(outcome, Ok(false));
    assert!(!diag.primitive_called);
    assert_eq!(diag.refusal_gate, Some("focus-write-veto"));
    assert_eq!(diag.fg_veto_kind, Some("readable-fullscreen"));
    assert!(!diag.desktop_exception);
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// A non-fullscreen taskbar foreground needs no exception: the ordinary
/// gate passes without marking it.
#[test]
fn selection_arrival_taskbar_foreground_needs_no_exception() {
    let mut env = env_with(120_140);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_140, 2540, "proc-540");
    let taskbar = win_key(120_141, 2541, "proc-541");
    script_arrival_target(&mut env, &target, "w6", &ws);
    let fg_facts = plain_facts();
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            taskbar.hwnd,
            observed_for_with_exe(
                &taskbar,
                "fg",
                rect(0, 1040, 1920, 40),
                fg_facts,
                "C:\\Windows\\explorer.exe",
            ),
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(taskbar.hwnd, taskbar.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(taskbar.hwnd));
    S_SHELL.with(|m| *m.borrow_mut() = 991_141);
    S_SYSTEM_SHELL.with(|m| *m.borrow_mut() = Some("C:\\Windows\\explorer.exe".to_owned()));
    S_FG.with(|m| *m.borrow_mut() = taskbar.hwnd);
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let grant = select_grant(&out, &ws, &reply);
    let (outcome, diag) = arrival_call(&mut env, grant, "w6", "fx-7");
    assert_eq!(outcome, Ok(true));
    assert!(diag.primitive_called);
    assert!(!diag.desktop_exception);
}

/// Observation-only replies dispatch no focus: with the desktop holding
/// the foreground and no effects, the pass emits no dispatch row.
#[test]
fn observation_only_desktop_focus_loss_dispatches_nothing() {
    let mut env = env_with(120_150);
    let ws = "ws1".to_owned();
    let out = env.state.output_token(DEV1);
    let target = win_key(120_150, 2550, "proc-550");
    let desktop = win_key(120_151, 2551, "proc-551");
    script_arrival_target(&mut env, &target, "w6", &ws);
    script_healthy_desktop(&desktop);
    let observed = vec![observed_for(
        &target,
        "w6",
        rect(0, 0, 800, 600),
        plain_facts(),
    )];
    let (mut diff, fresh) = convert(&env, &observed);
    // Observation reply: no effects, no grant (observation ticks never
    // arm one).
    let reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    let mut native = assemble(&mut env, scripted());
    native.set_diagnostics_trace(true);
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
    assert!(stats.focus_dispatch.is_empty());
    assert!(S_PRIME_CALLS.with(|m| m.borrow().is_empty()));
}

/// Grant authority binding: only the five explicit-selection commands
/// qualify, only on a fresh `accepted` delivery, and only for the reply's
/// own focus effects. Every other command, delivery, or effect shape mints
/// nothing, so directional focus, send/migrate follow, view switches, and
/// replays can never arm the desktop exception.
#[test]
fn selection_grant_binds_exact_accepted_select_focus_effects() {
    use tiler_core::directional::WindowId as CoreWindowId;

    let out = "out-1".to_owned();
    let ws = "ws-1".to_owned();
    let reply = fx_reply(&out, &ws, "fx-7", "w6");
    let selects = [
        WorldCommand::SelectWorkspace {
            output: OutputId(out.clone()),
            workspace: ws.clone(),
        },
        WorldCommand::SelectNumbered {
            output: Some(OutputId(out.clone())),
            index: 2,
        },
        WorldCommand::SelectRelative {
            output: None,
            delta: 1,
        },
        WorldCommand::SelectPrevious { output: None },
        WorldCommand::SelectTrailing { output: None },
    ];
    for command in &selects {
        assert!(
            SelectionArrivalGrant::selects(command),
            "selection command must qualify: {command:?}"
        );
    }
    let non_selects = [
        WorldCommand::Focus {
            window: CoreWindowId("w6".to_owned()),
            direction: "left".to_owned(),
        },
        WorldCommand::FocusDirectional {
            window: CoreWindowId("w6".to_owned()),
            direction: "left".to_owned(),
        },
        WorldCommand::Send {
            window: CoreWindowId("w6".to_owned()),
            target_output: OutputId(out.clone()),
            target_workspace: ws.clone(),
            follow: true,
        },
        WorldCommand::SwitchView {
            output: OutputId(out.clone()),
            workspace: ws.clone(),
        },
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-2".to_owned()),
            target_output: OutputId(out.clone()),
            workspace: ws.clone(),
        },
    ];
    for command in &non_selects {
        assert!(
            !SelectionArrivalGrant::selects(command),
            "non-selection command must not qualify: {command:?}"
        );
    }
    // Exact accepted select reply: the grant carries the reply's own
    // explicit arrival effect (id plus target), and nothing else.
    let select = &selects[0];
    let grant = SelectionArrivalGrant::for_accepted(Some(select), Some(7), &reply, "accepted")
        .expect("accepted select must mint");
    assert!(grant.allows("fx-7", "w6"));
    assert!(!grant.allows("fx-7", "w9"), "tampered target never binds");
    assert!(!grant.allows("fx-8", "w6"), "other ids never bind");
    // A wrong sequence mints nothing even for a selection command: the
    // effect is not this command's explicit outcome.
    assert!(
        SelectionArrivalGrant::for_accepted(Some(select), Some(8), &reply, "accepted").is_none()
    );
    assert!(SelectionArrivalGrant::for_accepted(Some(select), None, &reply, "accepted").is_none());
    // Replays never re-arm.
    assert!(
        SelectionArrivalGrant::for_accepted(Some(select), Some(7), &reply, "duplicate").is_none()
    );
    assert!(
        SelectionArrivalGrant::for_accepted(Some(select), Some(7), &reply, "command-consumed")
            .is_none()
    );
    // Non-selection commands never mint, even accepted with a sequence.
    for command in &non_selects {
        assert!(
            SelectionArrivalGrant::for_accepted(Some(command), Some(7), &reply, "accepted")
                .is_none(),
            "non-selection command must not mint: {command:?}"
        );
    }
    // An accepted select with no focus effects (empty-workspace arrival)
    // mints nothing: there is no arrival to except.
    let bare = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    assert!(
        SelectionArrivalGrant::for_accepted(Some(select), Some(7), &bare, "accepted").is_none()
    );
    // Automatic admission and cursor-follow effects sharing a select reply
    // never mint: only the command's explicit outcome binds.
    let mut mixed = reply.clone();
    for (id, target) in [("af-w6", "w6"), ("ffc-w6", "w6")] {
        mixed.effects.push(OneShotEffect {
            id: id.to_owned(),
            kind: EffectKind::Focus,
            target: WindowId(target.to_owned()),
            issued_revision: 1,
        });
    }
    let grant = SelectionArrivalGrant::for_accepted(Some(select), Some(7), &mixed, "accepted")
        .expect("explicit effect still mints beside automatic ones");
    assert!(grant.allows("fx-7", "w6"));
    assert!(!grant.allows("af-w6", "w6"), "admission intent never binds");
    assert!(!grant.allows("ffc-w6", "w6"), "cursor-follow never binds");
}

#[path = "world_native_composition_tests.rs"]
mod composition;
#[path = "world_native_dpi_tests.rs"]
mod dpi_cases;

#[path = "world_native_cursor_tests.rs"]
mod cursor_cases;

#[path = "world_native_roundtrip_tests.rs"]
mod roundtrip;
