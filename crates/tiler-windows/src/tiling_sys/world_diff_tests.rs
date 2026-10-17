#![forbid(unsafe_code)]
//! W2 host-diff regression fixtures (offline only).
//!
//! Source-first rigs: every baseline derives from real [`TileLoop`] owner
//! tables plus real [`ObservedWindow`]/[`RetainedRow`] per-tick products run
//! through the accepted W1 [`capture_world`](super::world_capture::capture_world)
//! boundary and converted by
//! [`FreshHost::from_sources`](super::world_diff::FreshHost::from_sources).
//! Fixtures diverge live facts per flag to prove each fence independently,
//! and no produced feedback ever uses an operation id the core cannot
//! consume. The running legacy owner is never mutated: each integration
//! fixture fingerprints its native ledgers before and after the facade ran.

use super::rmax03_adapter_tests::test_state;
use super::world_capture::capture_world;
use super::world_diff::{ApplyStats, FakeNative, FreshHost, FreshWindow, HostDiff, NativeOps};
use super::world_sequencer::{AckOutcome, WorldSequencer};
use super::{MonitorArea, ObservedWindow, RetainedRow, TileLoop};
use crate::model::{ProcessIdentity, WindowClaimKind, WindowIdentity, WindowShowState};
use crate::tiling::{FrameInsets, ObservedTarget, WindowFacts};
use crate::workspace::WindowKey;
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, EffectKind, HoldReason as CoreHold,
    OneShotEffect, ScopeAuthority as Auth, ScopeStatus, TilingMode, TransferKind, TransferTarget,
    WORLD_CONTRACT_VERSION, WindowDesire, WorkspaceBinding, WorkspaceDesire, WorldCommand,
    WorldReply,
};

const DEV1: &str = "\\\\.\\DISPLAY1";
const EXE: &str = "C:\\Apps\\app.exe";
const ADAPTER: &str = "test-windows-adapter-1";
const GENERATION: &str = "gen-1";

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

fn observed_window(key: &WindowKey, token: &str, vis: Rect, facts: WindowFacts) -> ObservedWindow {
    let outer = Rect {
        x: vis.x - 8,
        y: vis.y - 8,
        w: vis.w + 16,
        h: vis.h + 16,
    };
    ObservedWindow {
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
            tag: String::new(),
        },
        facts,
    }
}

fn retained_row(
    key: &WindowKey,
    token: &str,
    rect: Option<Rect>,
    maximized: bool,
    fullscreen: bool,
    facts: Option<WindowFacts>,
) -> RetainedRow {
    RetainedRow {
        key: key.clone(),
        token: token.to_owned(),
        rect,
        maximized,
        fullscreen,
        facts,
    }
}

fn process_identity(key: &WindowKey) -> ProcessIdentity {
    ProcessIdentity {
        pid: key.pid,
        process_creation: key.creation.clone(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: EXE.to_owned(),
    }
}

/// Admit one member through the real tables: committed membership plus the
/// stable token, stamped lifetime tag, and stored identity.
fn admit(state: &mut TileLoop, key: &WindowKey, dev: &str, ws: &str, token: &str, hidden: bool) {
    assert!(state.workspaces.assign(key.clone(), dev, ws, hidden));
    state.member_tokens.insert(key.clone(), token.to_owned());
    state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    state
        .member_identity
        .insert(key.clone(), process_identity(key));
}

fn one_area() -> Vec<MonitorArea> {
    vec![MonitorArea {
        device: DEV1.to_owned(),
        work: rect(0, 0, 1920, 1040),
        full: rect(0, 0, 1920, 1080),
    }]
}

/// Single-output two-window rig over real sources: committed membership,
/// verified frames, and the accepted capture boundary underneath.
struct Rig {
    state: TileLoop,
    observed: Vec<ObservedWindow>,
    retained: Vec<RetainedRow>,
    areas: Vec<MonitorArea>,
    ws: String,
}

fn two_window_rig() -> Rig {
    let mut state = test_state();
    state.workspaces.ensure_output(DEV1);
    let ws = state.workspaces.workspace_ids(DEV1)[0].clone();
    let k1 = win_key(1001, 2001, "proc-1");
    let k2 = win_key(1002, 2002, "proc-2");
    admit(&mut state, &k1, DEV1, &ws, "w1", false);
    admit(&mut state, &k2, DEV1, &ws, "w2", false);
    let observed = vec![
        observed_window(&k1, "w1", rect(0, 0, 800, 600), plain_facts()),
        observed_window(&k2, "w2", rect(0, 610, 800, 400), plain_facts()),
    ];
    Rig {
        state,
        observed,
        retained: Vec::new(),
        areas: one_area(),
        ws,
    }
}

fn diff_state() -> HostDiff {
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    diff
}

fn converted(rig: &Rig) -> (HostDiff, FreshHost, FakeNative) {
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(captured.inventory_complete);
    let fresh = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native.desktops.insert(rig.ws.clone());
    native.desktops.insert("ws-extra-a".to_owned());
    native.desktops.insert("ws-extra-b".to_owned());
    (diff_state(), fresh, native)
}

fn reply_shell(obs: u64, rev: u64, controlled: &[(&str, &str)]) -> WorldReply {
    WorldReply {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: ADAPTER.to_owned(),
        core_generation: GENERATION.to_owned(),
        request_id: "req-1".to_owned(),
        based_on_observation_seq: obs,
        state_revision: rev,
        ready: true,
        command_consumed: false,
        scopes: controlled
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

fn tiled_row(ws: &str, token: &str, target: Rect) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(token.to_owned()),
        output: OutputId("o1".to_owned()),
        workspace: ws.to_owned(),
        allocation: Some(target),
        effective_frame: Some(target),
        frame_authority: Auth::Controlled,
        membership_authority: Auth::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
    }
}

fn membership_row(ws: &str, token: &str) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(token.to_owned()),
        output: OutputId("o1".to_owned()),
        workspace: ws.to_owned(),
        allocation: None,
        effective_frame: None,
        frame_authority: Auth::Untouched,
        membership_authority: Auth::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
    }
}

fn window_transfer(
    op: &str,
    win: &str,
    out: &str,
    ws: &str,
    held: bool,
    follow: bool,
) -> TransferTarget {
    TransferTarget {
        operation_id: op.to_owned(),
        kind: TransferKind::Window(WindowId(win.to_owned())),
        target_output: OutputId(out.to_owned()),
        target_workspace: ws.to_owned(),
        follow,
        held,
    }
}

fn managed_binding(ws: &str) -> WorkspaceBinding {
    WorkspaceBinding {
        logical_id: ws.to_owned(),
        native_id: ws.to_owned(),
        output: OutputId("o1".to_owned()),
        pinned: false,
        displaced: false,
        managed: true,
    }
}

fn native_fingerprint(state: &TileLoop) -> (usize, usize, usize, usize) {
    (
        state.pending_closed.len(),
        state.member_tokens.len(),
        state.member_tags.len(),
        state.member_identity.len(),
    )
}

fn apply(
    diff: &mut HostDiff,
    reply: &WorldReply,
    fresh: &FreshHost,
    native: &mut FakeNative,
) -> (ApplyStats, Vec<AttemptFeedback>) {
    super::world_diff::apply_world_reply(diff, reply, fresh, native)
}

#[test]
fn conversion_covers_real_sources() {
    let rig = two_window_rig();
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured);
    let w1 = fresh.baseline.get("w1").expect("w1 baselined");
    assert_eq!(w1.key.hwnd, 1001);
    assert_eq!(w1.frame, Some(rect(0, 0, 800, 600)));
    assert_eq!(w1.facts, Some(plain_facts()));
    assert_eq!(w1.output, "o1");
    assert_eq!(w1.workspace, rig.ws);
    assert!(!w1.iconic && !w1.hidden);
    assert_eq!(w1.live_tag.as_deref(), Some("member-tag-1"));
    assert_eq!(
        fresh.stored_tags.get("w1").map(String::as_str),
        Some("member-tag-1")
    );
    // Views seed from the captured topology, not from a guessed map.
    assert_eq!(
        fresh.views.get("o1").map(String::as_str),
        Some(rig.ws.as_str())
    );
}

#[test]
fn conversion_grounds_iconic_and_hidden() {
    let mut state = test_state();
    state.workspaces.ensure_output(DEV1);
    let ws = state.workspaces.workspace_ids(DEV1)[0].clone();
    let k1 = win_key(1001, 2001, "proc-1");
    let k2 = win_key(1002, 2002, "proc-2");
    admit(&mut state, &k1, DEV1, &ws, "w1", false);
    admit(&mut state, &k2, DEV1, &ws, "w2", true);
    // Iconic omission without any fresh row: the minimized entry carries no
    // frame and no facts, grounded in the adapter omission set.
    state.minimized_omitted.insert(k1.clone());
    // Hidden plus iconic at hide time: explicit minimized entry homed on the
    // hidden member, with the hide flag from committed membership.
    state.hidden_claims.insert(
        k2.clone(),
        crate::tiling_sys::retained::HiddenRecord {
            claim: WindowIdentity {
                hwnd: 1002,
                process: process_identity(&k2),
                tag: "0123456789abcdef".to_owned(),
                kind: WindowClaimKind::Product,
                show: WindowShowState {
                    minimized: false,
                    maximized: false,
                },
            },
            iconic: true,
        },
    );
    let areas = one_area();
    let captured = capture_world(&state, Some(&[]), &[], &areas, 0);
    assert!(!captured.minimized.is_empty());
    let fresh = FreshHost::from_sources(&state, &[], &[], &captured);
    let iconic = fresh.baseline.get("w1").expect("w1 baselined");
    assert!(iconic.iconic);
    assert!(!iconic.hidden);
    assert_eq!(iconic.frame, None);
    assert_eq!(iconic.facts, None);
    let hidden = fresh.baseline.get("w2").expect("w2 baselined");
    assert!(hidden.iconic);
    assert!(hidden.hidden);
    assert_eq!(hidden.frame, None);
}

#[test]
fn conversion_carries_retained_overlay_facts() {
    // A retained overlay row (maximized, identity-verified facts) rides the
    // reserved lane: the conversion carries its facts and last-known slot,
    // and the tiled frame path holds instead of writing onto the overlay.
    let mut state = test_state();
    state.workspaces.ensure_output(DEV1);
    let ws = state.workspaces.workspace_ids(DEV1)[0].clone();
    let k1 = win_key(1001, 2001, "proc-1");
    admit(&mut state, &k1, DEV1, &ws, "w1", false);
    let mut overlay_facts = plain_facts();
    overlay_facts.maximized = true;
    let retained = vec![retained_row(
        &k1,
        "w1",
        Some(rect(0, 0, 800, 600)),
        true,
        false,
        Some(overlay_facts),
    )];
    let areas = one_area();
    let captured = capture_world(&state, Some(&[]), &retained, &areas, 0);
    assert!(captured.domains.iter().any(|d| !d.windows.is_empty()));
    let fresh = FreshHost::from_sources(&state, &[], &retained, &captured);
    let w1 = fresh.baseline.get("w1").expect("w1 baselined");
    assert_eq!(w1.frame, Some(rect(0, 0, 800, 600)));
    assert!(w1.facts.is_some_and(|facts| facts.maximized));
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    let mut native = FakeNative::seeded(&fresh);
    native.desktops.insert(ws.clone());
    let mut reply = reply_shell(7, 1, &[("o1", &ws)]);
    reply
        .windows
        .push(tiled_row(&ws, "w1", rect(10, 10, 400, 300)));
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(feedback.is_empty());
}

#[test]
fn converged_noop_clears_refusal() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply
        .windows
        .push(tiled_row(&rig.ws, "w1", rect(0, 0, 800, 600)));
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 0);
    assert!(feedback.is_empty());
    assert!(!stats.partial);
}

#[test]
fn allocation_without_effective_authorizes_nothing() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    // The canonical slot alone is never a write target: only effective_frame
    // authorizes a frame write.
    let mut row = tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300));
    row.effective_frame = None;
    reply.windows.push(row);
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(feedback.is_empty());
}

#[test]
fn clamp_suppresses_identical_rearm_on_change() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    // The setter accepts but the app holds its frame: proof mismatches the
    // target, so the native clamp lane records (desired, observed) with no
    // feedback invented.
    native.clamp_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply
        .windows
        .push(tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(stats.held, 1);
    assert!(feedback.is_empty());
    assert!(diff.refused.clamp_pair("w1").is_some());
    // The identical intent suppresses while the clamp pair stands; a
    // meaningful target change re-arms.
    let writes = native.frame_writes;
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(native.frame_writes, writes);
    assert_eq!(stats.geometry_writes, 0);
    let mut moved = reply_shell(7, 1, &[("o1", &rig.ws)]);
    moved
        .windows
        .push(tiled_row(&rig.ws, "w1", rect(20, 20, 400, 300)));
    let (stats, _) = apply(&mut diff, &moved, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 1);
}

#[test]
fn explicit_refusal_enters_transient_lane_without_feedback() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.fail_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply
        .windows
        .push(tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    // Ordinary geometry outcomes are proven by read-back and remembered in
    // the native refusal tracker, never by invented operation ids.
    assert!(feedback.is_empty());
    // The identical intent suppresses while the transient backoff holds;
    // a meaningful target change re-arms.
    let now = std::time::Instant::now();
    assert!(
        diff.refused
            .should_skip("w1", &rect(10, 10, 400, 300), &rect(0, 0, 800, 600), now)
    );
    assert!(!diff.refused.should_skip(
        "w1",
        &rect(20, 20, 400, 300),
        &rect(0, 0, 800, 600),
        now + std::time::Duration::from_secs(60),
    ));
}

#[test]
fn per_flag_divergence_holds_without_writes() {
    // Each live safety/identity flag diverges independently from the same
    // real-source baseline: every fence holds with no setter.
    type Diverge = (&'static str, Box<dyn Fn(&mut FreshWindow)>);
    let mutate: Vec<Diverge> = vec![
        (
            "cloaked",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().cloaked = true),
        ),
        (
            "elevated",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().elevated = true),
        ),
        (
            "shell",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().shell = true),
        ),
        (
            "tool",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().tool_window = true),
        ),
        (
            "owned-dialog",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().owned = true),
        ),
        (
            "dialog",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().dialog = true),
        ),
        (
            "no-activate",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().no_activate = true),
        ),
        (
            "invisible",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().visible = false),
        ),
        (
            "native-minimized",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().minimized = true),
        ),
        (
            "maximized",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().maximized = true),
        ),
        (
            "fullscreen",
            Box::new(|w: &mut FreshWindow| w.facts.as_mut().unwrap().captionless_fullscreen = true),
        ),
        ("iconic", Box::new(|w: &mut FreshWindow| w.iconic = true)),
        ("hidden", Box::new(|w: &mut FreshWindow| w.hidden = true)),
        ("unreadable", Box::new(|w: &mut FreshWindow| w.frame = None)),
        ("unproven", Box::new(|w: &mut FreshWindow| w.facts = None)),
        ("sticky", Box::new(|w: &mut FreshWindow| w.sticky = true)),
        (
            "floating",
            Box::new(|w: &mut FreshWindow| w.floating = true),
        ),
        (
            "fit-excluded",
            Box::new(|w: &mut FreshWindow| w.fit_excluded = true),
        ),
    ];
    for (name, diverge) in mutate {
        let rig = two_window_rig();
        let (mut diff, fresh, mut native) = converted(&rig);
        diverge(native.windows.get_mut("w1").expect("w1 live"));
        let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
        reply
            .windows
            .push(tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300)));
        let (stats, feedback) =
            super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
        assert_eq!(stats.geometry_writes, 0, "flag {name} wrote");
        assert!(
            stats.held >= 1 || stats.stopped_mid_apply,
            "flag {name} passed"
        );
        assert!(feedback.is_empty(), "flag {name} invented feedback");
    }
}

#[test]
fn lifetime_drift_stops_before_setters() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().live_tag = Some("recycled-tag".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply
        .windows
        .push(tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);
}

#[test]
fn gesture_and_stale_watermark_fence_setters() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.gesture = true;
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply
        .windows
        .push(tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);

    let (mut diff, fresh, mut native) = converted(&rig);
    diff.gate.current_obs = 8;
    let reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);
}

#[test]
fn invalidation_stops_after_first_write() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.arm_invalidation_after_writes(1);
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply
        .windows
        .push(tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300)));
    reply
        .windows
        .push(tiled_row(&rig.ws, "w2", rect(0, 610, 700, 300)));
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 1);
    assert!(stats.stopped_mid_apply);
    assert!(stats.partial);
    assert_eq!(diff.gate.current_obs, 8);
}

#[test]
fn armed_invalidation_authorizes_nothing() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.fire_pending_invalidation();
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply
        .windows
        .push(tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);
}

#[test]
fn gate_accepts_strictly_newer_revisions() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    // Bootstrap: no acceptance yet, current watermark binds.
    let reply = reply_shell(7, 0, &[("o1", &rig.ws)]);
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(!stats.stopped_mid_apply);
    diff.gate
        .note_applied(&reply_shell(7, 0, &[("o1", &rig.ws)]));
    // A duplicate revision authorizes nothing.
    let dup = reply_shell(7, 0, &[("o1", &rig.ws)]);
    let (stats, _) = apply(&mut diff, &dup, &fresh, &mut native);
    assert!(stats.stopped_mid_apply);
    // An older revision authorizes nothing.
    diff.gate
        .note_applied(&reply_shell(7, 3, &[("o1", &rig.ws)]));
    let stale = reply_shell(7, 2, &[("o1", &rig.ws)]);
    let (stats, _) = apply(&mut diff, &stale, &fresh, &mut native);
    assert!(stats.stopped_mid_apply);
    // A foreign generation or adapter authorizes nothing.
    let mut foreign = reply_shell(7, 4, &[("o1", &rig.ws)]);
    foreign.core_generation = "gen-9".to_owned();
    let (stats, _) = apply(&mut diff, &foreign, &fresh, &mut native);
    assert!(stats.stopped_mid_apply);
}

#[test]
fn generation_reset_clears_generation_scoped_state() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    diff.effects.arm("op-1", "o1/ws".to_owned());
    assert!(diff.effects.is_ambiguous("op-1"));
    diff.effects.mark_effect_spent("fx-1");
    diff.creations.start("pending-1");
    diff.gate
        .note_applied(&reply_shell(7, 5, &[("o1", &rig.ws)]));
    diff.reset_generation("gen-2");
    assert!(!diff.effects.is_ambiguous("op-1"));
    assert!(diff.effects.mark_effect_spent("fx-1"));
    assert!(!diff.creations.is_inflight("pending-1"));
    assert_eq!(diff.gate.current_obs, 0);
    assert!(!diff.gate.has_accepted);
    let mut reply = reply_shell(0, 1, &[("o1", &rig.ws)]);
    reply.core_generation.clone_from(&"gen-2".to_owned());
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(!stats.stopped_mid_apply);
    let _ = native;
}

#[test]
fn prune_bounds_tables_to_reply_and_baseline() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    diff.effects.arm("op-stale", "ctx".to_owned());
    diff.effects.arm("op-1", "ctx".to_owned());
    diff.effects.mark_effect_spent("fx-stale");
    diff.effects.mark_float_spent("op-stale-float");
    diff.effects.applied_window.insert(
        "w-gone".to_owned(),
        ("op-1".to_owned(), "o1".to_owned(), rig.ws.clone()),
    );
    diff.creations.start("pending-gone");
    // op-1 targets a live scope but throws: the operation stays ambiguous
    // while stale entries for absent operations drop.
    native.throw_membership.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply.windows.push(membership_row(&rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-1",
        "w1",
        "o1",
        "ws-target",
        false,
        false,
    ));
    let _ = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(!diff.effects.is_ambiguous("op-stale"));
    assert!(diff.effects.is_ambiguous("op-1"));
    assert!(diff.effects.mark_effect_spent("fx-stale"));
    assert!(diff.effects.mark_float_spent("op-stale-float"));
    assert!(!diff.effects.applied_window.contains_key("w-gone"));
    assert!(!diff.creations.is_inflight("pending-gone"));
}

#[test]
fn transfer_throw_holds_ambiguous_without_repeat() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.throw_membership.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply.windows.push(membership_row(&rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-1",
        "w1",
        "o1",
        "ws-target",
        false,
        false,
    ));
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-1" && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
    assert!(diff.effects.is_ambiguous("op-1"));
    let before = native.membership_writes;
    let (stats2, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(native.membership_writes, before);
    assert_eq!(stats2.membership_writes, 0);
}

#[test]
fn transfer_refused_proven_and_superseded() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.fail_membership.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply.windows.push(membership_row(&rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-2",
        "w1",
        "o1",
        "ws-target",
        false,
        false,
    ));
    let (_, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(feedback.iter().any(|f| f.operation_id == "op-2"
        && f.outcome == AttemptOutcome::RefusedProven
        && f.hold == Some(CoreHold::RefusedTarget)));

    let rig3 = two_window_rig();
    let (mut diff3, fresh3, mut native3) = converted(&rig3);
    native3.windows.get_mut("w1").unwrap().output = "o9".to_owned();
    native3.windows.get_mut("w1").unwrap().workspace = "ws9".to_owned();
    let mut reply3 = reply_shell(7, 1, &[("o1", &rig3.ws), ("o1", "ws-target")]);
    reply3.windows.push(membership_row(&rig3.ws, "w1"));
    reply3.transfers.push(window_transfer(
        "op-3",
        "w1",
        "o1",
        "ws-target",
        false,
        false,
    ));
    let (stats3, feedback3) = apply(&mut diff3, &reply3, &fresh3, &mut native3);
    assert_eq!(stats3.membership_writes, 0);
    assert!(
        feedback3
            .iter()
            .any(|f| f.operation_id == "op-3" && f.outcome == AttemptOutcome::SupersededExternal)
    );
}

#[test]
fn membership_proof_and_follow_view() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    // Already homed at the target: settles applied with no new setter, and
    // the requested follow view actuates with exact-output proof.
    native.windows.get_mut("w1").unwrap().workspace = "ws-target".to_owned();
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply.windows.push(membership_row(&rig.ws, "w1"));
    reply
        .transfers
        .push(window_transfer("op-4", "w1", "o1", "ws-target", true, true));
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-4" && f.outcome == AttemptOutcome::Applied)
    );
    assert_eq!(
        native.views.get("o1").map(String::as_str),
        Some("ws-target")
    );
    assert_eq!(stats.view_writes, 1);
}

#[test]
fn follow_view_failure_holds_transfer_without_view_claim() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().workspace = "ws-target".to_owned();
    native.fail_view.insert("o1/ws-target".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply.windows.push(membership_row(&rig.ws, "w1"));
    reply
        .transfers
        .push(window_transfer("op-5", "w1", "o1", "ws-target", true, true));
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    // Membership is proven but the view is not: the transfer stays
    // ambiguous. No view outcome is ever claimed on the window transfer id.
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-5" && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
    assert!(
        feedback
            .iter()
            .all(|f| f.operation_id != "op-5" || f.kind == AttemptKind::Transfer)
    );
}

#[test]
fn effect_requires_exact_view_before_focus() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    // The target workspace is not shown on its output: focusing now would
    // steal focus from the wrong view.
    native.views.insert("o1".to_owned(), "ws-other".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply.effects.push(OneShotEffect {
        id: "fx-1".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 1,
    });
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));

    // Once the exact view proves, the same effect actuates exactly once.
    native.views.insert("o1".to_owned(), rig.ws.clone());
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 1);
    assert!(native.is_foreground("w1"));
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
}

#[test]
fn stale_effect_drops_without_stealing_focus() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    diff.gate.current_obs = 9;
    native.views.insert("o1".to_owned(), rig.ws.clone());
    let mut reply = reply_shell(7, 5, &[("o1", &rig.ws)]);
    reply.effects.push(OneShotEffect {
        id: "fx-9".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 5,
    });
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));
    assert!(stats.stopped_mid_apply);
}

#[test]
fn migration_partial_holds_views_then_completes() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.throw_membership.insert("w2".to_owned());
    native.views.insert("o1".to_owned(), rig.ws.clone());
    let mig = "op-mig";
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    for token in ["w1", "w2"] {
        reply.windows.push(WindowDesire {
            lifetime: WindowId(token.to_owned()),
            output: OutputId("o1".to_owned()),
            workspace: "ws-target".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: Auth::Untouched,
            membership_authority: Auth::Controlled,
            floating: false,
            float_origin: None,
            frame_operation: None,
        });
    }
    reply.transfers.push(TransferTarget {
        operation_id: mig.to_owned(),
        kind: TransferKind::Migration,
        target_output: OutputId("o1".to_owned()),
        target_workspace: "ws-target".to_owned(),
        follow: false,
        held: false,
    });
    reply.transfers.push(TransferTarget {
        operation_id: format!("{mig}-refill"),
        kind: TransferKind::View,
        target_output: OutputId("o1".to_owned()),
        target_workspace: rig.ws.clone(),
        follow: false,
        held: false,
    });
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 1);
    // Partial arrival: no target view, no source refill, no focus steal.
    assert_eq!(
        native.views.get("o1").map(String::as_str),
        Some(rig.ws.as_str())
    );
    assert!(stats.view_writes == 0);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == mig && f.outcome == AttemptOutcome::PossiblyExecuted)
    );

    // A stuck straggler never replays: external convergence is the only
    // way forward. The user-equivalent arrival proves every member at the
    // target, so the ambiguous parent settles without member replay, and
    // only then do the target view and paired source refill actuate.
    native.windows.get_mut("w2").unwrap().workspace = "ws-target".to_owned();
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(
        native.views.get("o1").map(String::as_str),
        Some(rig.ws.as_str())
    );
    assert!(stats.view_writes >= 2);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == mig && f.outcome == AttemptOutcome::Applied)
    );
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == format!("{mig}-refill")
                && f.outcome == AttemptOutcome::Applied)
    );
}

#[test]
fn creation_binds_direct_and_adopts_reply_rows() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply.workspaces.push(WorkspaceDesire {
        id: "pending-1".to_owned(),
        output: OutputId("o1".to_owned()),
        native_binding: None,
        logical_id: Some("pending-1".to_owned()),
        scope: "o1".to_owned(),
        order: 3,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: None,
    });
    native.create_result = Some(Some("ws-made".to_owned()));
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.creations, 1);
    // Binding recovery is report-carried, never feedback: the core resolves
    // pending logical ids through bindings, not operation ids.
    assert!(feedback.is_empty());
    assert!(diff.creations.is_bound("pending-1"));

    // An identical reply never re-creates; the later reply row carrying the
    // binding adopts without another native call.
    native.create_result = Some(None);
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.creations, 0);
    let mut bound_reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    bound_reply.workspaces.push(WorkspaceDesire {
        id: "ws-made".to_owned(),
        output: OutputId("o1".to_owned()),
        native_binding: Some("ws-made".to_owned()),
        logical_id: Some("pending-1".to_owned()),
        scope: "o1".to_owned(),
        order: 3,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: None,
    });
    let (stats, _) = apply(&mut diff, &bound_reply, &fresh, &mut native);
    assert_eq!(stats.creations, 0);
    assert!(diff.creations.is_bound("pending-1"));
}

#[test]
fn ambiguous_create_never_steals_external_workspace() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply.workspaces.push(WorkspaceDesire {
        id: "pending-1".to_owned(),
        output: OutputId("o1".to_owned()),
        native_binding: None,
        logical_id: Some("pending-1".to_owned()),
        scope: "o1".to_owned(),
        order: 3,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: None,
    });
    native.create_result = Some(None);
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.creations, 0);
    // An unrelated external workspace appearing meanwhile is never stolen:
    // the ambiguous flight holds without binding and without another call.
    native.desktops.insert("ws-external".to_owned());
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.creations, 0);
    assert!(!diff.creations.is_bound("pending-1"));
    assert!(feedback.is_empty());
}

#[test]
fn creation_skips_held_absent_and_colliding_rows() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.create_result = Some(Some("ws-made".to_owned()));
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    // Held rows never actuate.
    reply.workspaces.push(WorkspaceDesire {
        id: "pending-held".to_owned(),
        output: OutputId("o1".to_owned()),
        native_binding: None,
        logical_id: Some("pending-held".to_owned()),
        scope: "o1".to_owned(),
        order: 3,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: Some(CoreHold::IncompleteInventory),
    });
    // Desired-absent unbound rows are removal candidates, never creations.
    reply.workspaces.push(WorkspaceDesire {
        id: "pending-gone".to_owned(),
        output: OutputId("o1".to_owned()),
        native_binding: None,
        logical_id: Some("pending-gone".to_owned()),
        scope: "o1".to_owned(),
        order: 4,
        tiling_mode: TilingMode::Tiled,
        existence: false,
        hold: None,
    });
    // The logical token is never used as a native id, even on collision.
    reply.workspaces.push(WorkspaceDesire {
        id: rig.ws.clone(),
        output: OutputId("o1".to_owned()),
        native_binding: None,
        logical_id: Some(rig.ws.clone()),
        scope: "o1".to_owned(),
        order: 5,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: None,
    });
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.creations, 0);
    assert!(!diff.creations.is_inflight("pending-held"));
    assert!(!diff.creations.is_inflight("pending-gone"));
    assert!(!diff.creations.is_inflight(&rig.ws));
}

#[test]
fn dependent_targets_wait_for_logical_binding() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply.workspaces.push(WorkspaceDesire {
        id: "pending-1".to_owned(),
        output: OutputId("o1".to_owned()),
        native_binding: None,
        logical_id: Some("pending-1".to_owned()),
        scope: "o1".to_owned(),
        order: 3,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: None,
    });
    reply.windows.push(membership_row(&rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-1",
        "w1",
        "o1",
        "pending-1",
        false,
        false,
    ));
    // The logical has no binding yet, so the dependent transfer holds even
    // though the creation lands in the same pass (windows actuate before
    // creations, and the window loop sees the unbound logical).
    native.create_result = Some(Some("ws-made".to_owned()));
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(stats.creations, 1);

    // Once the reply carries the binding, the same transfer proceeds.
    let mut bound_reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-made")]);
    bound_reply.workspaces.push(WorkspaceDesire {
        id: "ws-made".to_owned(),
        output: OutputId("o1".to_owned()),
        native_binding: Some("ws-made".to_owned()),
        logical_id: Some("pending-1".to_owned()),
        scope: "o1".to_owned(),
        order: 3,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: None,
    });
    bound_reply.windows.push(membership_row(&rig.ws, "w1"));
    bound_reply
        .transfers
        .push(window_transfer("op-1", "w1", "o1", "ws-made", false, false));
    let (stats, feedback) = apply(&mut diff, &bound_reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 1);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-1" && f.outcome == AttemptOutcome::Applied)
    );
}

#[test]
fn removal_requires_ownership_and_fresh_proof() {
    let rig = two_window_rig();
    let (mut diff, mut fresh, mut native) = converted(&rig);
    native.occupants.insert(rig.ws.clone(), (0, true));
    native.visible.insert(rig.ws.clone(), Some(false));
    let removal = |ws: &str| WorkspaceDesire {
        id: ws.to_owned(),
        output: OutputId("o1".to_owned()),
        native_binding: Some(ws.to_owned()),
        logical_id: None,
        scope: "o1".to_owned(),
        order: 1,
        tiling_mode: TilingMode::Tiled,
        existence: false,
        hold: None,
    };
    // No known mapping: unknown ownership keeps. (The conversion carries
    // the real capture bindings; clear them to prove the unknown case.)
    fresh.bindings.clear();
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply.workspaces.push(removal(&rig.ws));
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.removals, 0);

    // Unmanaged, displaced, and pinned bindings all keep. The mapping is
    // adapter-observed (report-carried); the proof below it stays live.
    for binding in [
        WorkspaceBinding {
            managed: false,
            ..managed_binding(&rig.ws)
        },
        WorkspaceBinding {
            displaced: true,
            ..managed_binding(&rig.ws)
        },
        WorkspaceBinding {
            pinned: true,
            ..managed_binding(&rig.ws)
        },
    ] {
        fresh.bindings = vec![binding];
        let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
        reply.workspaces.push(removal(&rig.ws));
        let (stats, _) =
            super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
        assert_eq!(stats.removals, 0);
    }

    // Managed, clean, invisible, and above the minimum: removes.
    fresh.bindings = vec![managed_binding(&rig.ws)];
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    reply.workspaces.push(removal(&rig.ws));
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.removals, 1);
    assert!(!native.desktops.contains(&rig.ws));
}

#[test]
fn removal_honors_fresh_occupants_visibility_and_scope() {
    let rig = two_window_rig();
    let (mut diff, mut fresh, mut native) = converted(&rig);
    for extra in ["ws-a", "ws-b", "ws-c"] {
        native.desktops.insert(extra.to_owned());
        native.occupants.insert(extra.to_owned(), (0, true));
        native.visible.insert(extra.to_owned(), Some(false));
    }
    native.occupants.insert("ws-a".to_owned(), (2, true));
    let removal = |ws: &str| WorkspaceDesire {
        id: ws.to_owned(),
        output: OutputId("o1".to_owned()),
        native_binding: Some(ws.to_owned()),
        logical_id: None,
        scope: "o1".to_owned(),
        order: 1,
        tiling_mode: TilingMode::Tiled,
        existence: false,
        hold: None,
    };
    let attempt = |diff: &mut HostDiff,
                   fresh: &mut FreshHost,
                   native: &mut FakeNative,
                   ws: &str,
                   scopes: &[(&str, &str)]|
     -> (ApplyStats, Vec<AttemptFeedback>) {
        let mut reply = reply_shell(7, 1, scopes);
        reply.workspaces.push(removal(ws));
        fresh.bindings = vec![WorkspaceBinding {
            logical_id: ws.to_owned(),
            native_id: ws.to_owned(),
            output: OutputId("o1".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        }];
        apply(diff, &reply, fresh, native)
    };
    // Occupied (including non-tiled occupants) keeps.
    let (stats, _) = attempt(
        &mut diff,
        &mut fresh,
        &mut native,
        "ws-a",
        &[("o1", "ws-a")],
    );
    assert_eq!(stats.removals, 0);
    // Fresh proof of emptiness on the next pass removes.
    native.occupants.insert("ws-a".to_owned(), (0, true));
    let (stats, _) = attempt(
        &mut diff,
        &mut fresh,
        &mut native,
        "ws-a",
        &[("o1", "ws-a")],
    );
    assert_eq!(stats.removals, 1);
    // Visible keeps.
    native.visible.insert("ws-b".to_owned(), Some(true));
    let (stats, _) = attempt(
        &mut diff,
        &mut fresh,
        &mut native,
        "ws-b",
        &[("o1", "ws-b")],
    );
    assert_eq!(stats.removals, 0);
    // Unknown occupancy keeps.
    native.occupants.insert("ws-c".to_owned(), (0, false));
    let (stats, _) = attempt(
        &mut diff,
        &mut fresh,
        &mut native,
        "ws-c",
        &[("o1", "ws-c")],
    );
    assert_eq!(stats.removals, 0);
    // A held scope keeps even when every native proof allows it.
    native.occupants.insert("ws-c".to_owned(), (0, true));
    let mut reply = reply_shell(7, 1, &[]);
    reply.scopes.push(ScopeStatus {
        scope: "o1/ws-c".to_owned(),
        authority: Auth::Hold(CoreHold::IncompleteInventory),
    });
    reply.workspaces.push(removal("ws-c"));
    fresh.bindings = vec![WorkspaceBinding {
        logical_id: "ws-c".to_owned(),
        native_id: "ws-c".to_owned(),
        output: OutputId("o1".to_owned()),
        pinned: false,
        displaced: false,
        managed: true,
    }];
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.removals, 0);
}

#[test]
fn held_scope_blocks_window_actuation() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[]);
    reply.scopes.push(ScopeStatus {
        scope: format!("o1/{}", rig.ws),
        authority: Auth::Hold(CoreHold::GestureBarrier),
    });
    reply
        .windows
        .push(tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
}

#[test]
fn float_token_places_once_with_tracker_memory() {
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().floating = true;
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    let mut row = tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Untouched;
    row.frame_operation = Some("op-9".to_owned());
    reply.windows.push(row);
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 1);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-9" && f.outcome == AttemptOutcome::Applied)
    );
    // Duplicate reply never double-places the same operation.
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
}

/// Same-output send rig over real sources: two workspaces on one output
/// (the trailing spare mints the second), one member, and the accepted
/// capture boundary underneath.
struct SendRig {
    state: TileLoop,
    observed: Vec<ObservedWindow>,
    retained: Vec<RetainedRow>,
    areas: Vec<MonitorArea>,
    ws1: String,
    ws2: String,
}

fn send_rig() -> SendRig {
    let mut state = test_state();
    state.workspaces.ensure_output(DEV1);
    // ensure_output seeds the minimum pair; the second workspace is the
    // same-output send target, mirroring the W1 sequencing rigs.
    let ws1 = state.workspaces.workspace_ids(DEV1)[0].clone();
    let ws2 = state.workspaces.workspace_ids(DEV1)[1].clone();
    let k1 = win_key(1001, 2001, "proc-1");
    admit(&mut state, &k1, DEV1, &ws1, "w1", false);
    let observed = vec![observed_window(
        &k1,
        "w1",
        rect(0, 0, 800, 600),
        plain_facts(),
    )];
    SendRig {
        state,
        observed,
        retained: Vec::new(),
        areas: one_area(),
        ws1,
        ws2,
    }
}

/// Second physical output device for two-output fixtures.
const DEV2: &str = "\\\\.\\DISPLAY2";

/// Two side-by-side monitor areas with real native geometry.
fn two_areas() -> Vec<MonitorArea> {
    vec![
        MonitorArea {
            device: DEV1.to_owned(),
            work: rect(0, 0, 1920, 1040),
            full: rect(0, 0, 1920, 1080),
        },
        MonitorArea {
            device: DEV2.to_owned(),
            work: rect(1920, 0, 1920, 1040),
            full: rect(1920, 0, 1920, 1080),
        },
    ]
}

/// Real two-output fixture over real sources: both outputs synced, one
/// member homed on the first workspace of DEV1, and the accepted capture
/// boundary underneath. Output tokens are o1 (DEV1) then o2 (DEV2).
struct TwoOutputRig {
    state: TileLoop,
    observed: Vec<ObservedWindow>,
    retained: Vec<RetainedRow>,
    areas: Vec<MonitorArea>,
    ws_a: String,
    ws_b: String,
}

fn two_output_rig() -> TwoOutputRig {
    let mut state = test_state();
    state.workspaces.ensure_output(DEV1);
    state.workspaces.ensure_output(DEV2);
    let ws_a = state.workspaces.workspace_ids(DEV1)[0].clone();
    let ws_b = state.workspaces.workspace_ids(DEV2)[0].clone();
    let k1 = win_key(1001, 2001, "proc-1");
    admit(&mut state, &k1, DEV1, &ws_a, "w1", false);
    let observed = vec![observed_window(
        &k1,
        "w1",
        rect(100, 100, 800, 600),
        plain_facts(),
    )];
    TwoOutputRig {
        state,
        observed,
        retained: Vec::new(),
        areas: two_areas(),
        ws_a,
        ws_b,
    }
}

#[test]
fn send_roundtrip_through_sequencer_without_stranding() {
    use tiler_core::world_engine::WorldOutcome;
    let rig = send_rig();
    let before = native_fingerprint(&rig.state);
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    // Explicit same-output send under a fresh watermark: the core retains
    // a window transfer with a stable operation id.
    seq.invalidate();
    let send = WorldCommand::Send {
        window: WindowId("w1".to_owned()),
        target_output: OutputId("o1".to_owned()),
        target_workspace: rig.ws2.clone(),
        follow: false,
    };
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            Some(send),
        )
        .expect("send dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted send, got {other:?}"),
    };
    assert_eq!(reply.transfers.len(), 1);
    let op = reply.transfers[0].operation_id.clone();
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }

    // The facade actuates the retained transfer against the offline native
    // and proves it by read-back under the real operation id. The gate is
    // fresh (nothing applied yet), so the current revision proceeds.
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 1);
    assert!(
        produced
            .iter()
            .any(|f| f.operation_id == op && f.outcome == AttemptOutcome::Applied)
    );
    // Consuming the revision arms the gate: the identical reply authorizes
    // nothing more, so desired acceptance never double-actuates.
    diff.gate.note_applied(&reply);
    let writes = native.membership_writes;
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(stats.stopped_mid_apply);
    assert_eq!(native.membership_writes, writes);
    for item in &produced {
        seq.note_feedback(item.clone());
    }
    assert!(seq.has_pending_feedback());
    seq.invalidate();
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(seq.is_dirty(&captured));
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("feedback dispatch");
    assert!(report.feedback.iter().any(|f| f.operation_id == op));
    let reply2 = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply2) {
        AckOutcome::Applied(receipt) => {
            assert!(receipt.feedback_acked.contains(&op));
        }
        other => panic!("expected applied, got {other:?}"),
    }
    // The core holds the target scope until arrival proves (the native
    // observation still shows source homing): the reply carries the
    // outstanding transfer with no rows, so the facade invents no
    // actuation targets and produces no feedback.
    diff.gate.current_obs = seq.current_obs();
    assert!(
        reply2
            .scopes
            .iter()
            .any(|s| s.scope == format!("o1/{}", rig.ws2) && s.authority != Auth::Controlled)
    );
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply2, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(stats.view_writes, 0);
    assert_eq!(stats.creations, 0);
    assert_eq!(stats.removals, 0);
    assert_eq!(stats.effects, 0);
    assert!(!stats.stopped_mid_apply);
    assert!(produced.is_empty());
    // The running legacy owner never moved.
    assert_eq!(native_fingerprint(&rig.state), before);
    let _ = rig.ws1;
}

#[test]
fn ambiguous_transfer_holds_without_repeat_or_strand() {
    use tiler_core::world_engine::WorldOutcome;
    let rig = send_rig();
    let before = native_fingerprint(&rig.state);
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let send = WorldCommand::Send {
        window: WindowId("w1".to_owned()),
        target_output: OutputId("o1".to_owned()),
        target_workspace: rig.ws2.clone(),
        follow: false,
    };
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            Some(send),
        )
        .expect("send dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted send, got {other:?}"),
    };
    let op = reply.transfers[0].operation_id.clone();
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    // The native setter throws: possibly-executed holds the transfer, and
    // the identical reply never repeats the setter. The gate stays fresh
    // (nothing consumed), so the hold below is the ambiguity fence itself.
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native.throw_membership.insert("w1".to_owned());
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    let (stats, first) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(
        first
            .iter()
            .any(|f| f.operation_id == op && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
    let writes = native.membership_writes;
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(native.membership_writes, writes);
    assert_eq!(stats.membership_writes, 0);
    assert!(produced.is_empty());
    // The first flight's feedback rides the next report; the core holds the
    // transfer (never repeats, never drops), and the held reply authorizes
    // no setter on the facade either.
    for item in &first {
        seq.note_feedback(item.clone());
    }
    assert!(seq.has_pending_feedback());
    seq.invalidate();
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(seq.is_dirty(&captured));
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("feedback dispatch");
    assert!(report.feedback.iter().any(|f| f.operation_id == op));
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| t.operation_id == op && t.held)
    );
    match seq.ack(&reply) {
        AckOutcome::Applied(receipt) => {
            assert!(receipt.feedback_acked.contains(&op));
        }
        other => panic!("expected applied, got {other:?}"),
    }
    diff.gate.current_obs = seq.current_obs();
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(produced.is_empty());
    assert!(!stats.stopped_mid_apply);
    assert_eq!(native_fingerprint(&rig.state), before);
    let _ = rig.ws1;
}

#[test]
fn float_live_sticky_holds_without_write() {
    // B4: explicit float placement onto a live-sticky window must hold.
    // Baseline is ordinary (non-sticky); only the live re-read is sticky.
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().sticky = true;
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    let mut row = tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Untouched;
    row.frame_operation = Some("op-float-sticky".to_owned());
    reply.windows.push(row);
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0, "live sticky took a float write");
    assert!(stats.held >= 1);
}

#[test]
fn float_live_fit_excluded_holds_without_write() {
    // B4: explicit float placement onto a live fit-excluded window holds.
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().fit_excluded = true;
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    let mut row = tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Untouched;
    row.frame_operation = Some("op-float-excluded".to_owned());
    reply.windows.push(row);
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(
        stats.geometry_writes, 0,
        "live fit-excluded took a float write"
    );
    assert!(stats.held >= 1);
}

/// Test-only NativeOps wrapper: delegates to an inner FakeNative except for
/// an overridden view proof path, so fixtures can simulate a successful
/// setter whose read-back is missing (async proof) or names a third
/// location (user moved elsewhere). Counts switch_view attempts.
struct ProbeNative {
    inner: FakeNative,
    switches: usize,
    switch_result: Option<Result<bool, ()>>,
    read_override: Option<Option<String>>,
}

impl ProbeNative {
    fn wrapping(inner: FakeNative) -> Self {
        Self {
            inner,
            switches: 0,
            switch_result: None,
            read_override: None,
        }
    }
}

impl NativeOps for ProbeNative {
    fn read_window(&self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&self, output: &str) -> Option<String> {
        if let Some(over) = self.read_override.clone() {
            return over;
        }
        self.inner.read_view(output)
    }
    fn is_foreground(&self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&self) -> bool {
        self.inner.gesture_barrier()
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        self.inner.write_frame(token, rect)
    }
    fn move_membership(&mut self, token: &str, out: &str, ws: &str) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws)
    }
    fn create_workspace(&mut self, logical: &str) -> Result<Option<String>, ()> {
        self.inner.create_workspace(logical)
    }
    fn remove_workspace(&mut self, native: &str) -> Result<bool, ()> {
        self.inner.remove_workspace(native)
    }
    fn switch_view(&mut self, output: &str, native: &str) -> Result<bool, ()> {
        self.switches += 1;
        if let Some(result) = self.switch_result {
            if result.is_ok() {
                let _ = self.inner.switch_view(output, native);
            }
            return result;
        }
        self.inner.switch_view(output, native)
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.inner.focus_window(token)
    }
    fn occupants(&self, native: &str) -> (usize, bool) {
        self.inner.occupants(native)
    }
    fn is_visible(&self, native: &str) -> Option<bool> {
        self.inner.is_visible(native)
    }
    fn native_ids(&self) -> std::collections::BTreeSet<String> {
        self.inner.native_ids()
    }
    fn workspace_count(&self) -> usize {
        self.inner.workspace_count()
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
    }
}

fn view_transfer(op: &str, out: &str, ws: &str) -> TransferTarget {
    TransferTarget {
        operation_id: op.to_owned(),
        kind: TransferKind::View,
        target_output: OutputId(out.to_owned()),
        target_workspace: ws.to_owned(),
        follow: false,
        held: false,
    }
}

#[test]
fn view_ok_true_with_unreadable_proof_is_not_refused_proven() {
    // View truthfulness: Ok(true) with a missing read-back cannot prove an
    // explicit refusal; it is ambiguous (possibly executed).
    let rig = two_window_rig();
    let (mut diff, fresh, native) = converted(&rig);
    let mut probe = ProbeNative::wrapping(native);
    probe.inner.views.insert("o1".to_owned(), rig.ws.clone());
    probe.switch_result = Some(Ok(true));
    probe.read_override = Some(None);
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply
        .transfers
        .push(view_transfer("op-view-async", "o1", "ws-target"));
    let (_, feedback) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    let got = feedback.iter().find(|f| f.operation_id == "op-view-async");
    assert!(got.is_some(), "expected feedback for the view attempt");
    assert_eq!(
        got.unwrap().outcome,
        AttemptOutcome::PossiblyExecuted,
        "Ok(true) with unreadable proof claimed {:?}",
        got.unwrap().outcome
    );
}

#[test]
fn view_ok_true_with_third_location_is_not_refused_proven() {
    // View truthfulness: Ok(true) proving a third location (neither the
    // baseline view nor the target) is newer external input, never a proven
    // refusal of this target.
    let rig = two_window_rig();
    let (mut diff, fresh, native) = converted(&rig);
    let mut probe = ProbeNative::wrapping(native);
    probe.inner.views.insert("o1".to_owned(), rig.ws.clone());
    probe.switch_result = Some(Ok(true));
    probe.read_override = Some(Some("ws-other".to_owned()));
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply
        .transfers
        .push(view_transfer("op-view-third", "o1", "ws-target"));
    let (_, feedback) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    let got = feedback.iter().find(|f| f.operation_id == "op-view-third");
    assert!(got.is_some(), "expected feedback for the view attempt");
    assert_ne!(
        got.unwrap().outcome,
        AttemptOutcome::RefusedProven,
        "third-location proof claimed refused-proven"
    );
}

#[test]
fn follow_view_failure_does_not_repeat_switch() {
    // Follow no-replay: membership proven but follow view refused/errored
    // holds the transfer ambiguous; the next identical reply must not call
    // switch_view again.
    let rig = two_window_rig();
    let (mut diff, fresh, native) = converted(&rig);
    let mut probe = ProbeNative::wrapping(native);
    probe.inner.windows.get_mut("w1").unwrap().workspace = "ws-target".to_owned();
    probe.inner.views.insert("o1".to_owned(), rig.ws.clone());
    probe.inner.fail_view.insert("o1/ws-target".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply.windows.push(membership_row(&rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-follow-repeat",
        "w1",
        "o1",
        "ws-target",
        true,
        true,
    ));
    let (_, first) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert!(
        first.iter().any(|f| f.operation_id == "op-follow-repeat"
            && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
    assert_eq!(probe.switches, 1);
    let (_, second) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert!(second.is_empty() || second.iter().all(|f| f.operation_id != "op-follow-repeat"));
    assert_eq!(
        probe.switches, 1,
        "ambiguous follow view re-attempted the switch"
    );
}

#[test]
fn migration_refill_held_does_not_actuate() {
    // Refill authority: a held refill transfer must not actuate even after
    // member arrivals and the target view prove.
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    for token in ["w1", "w2"] {
        native.windows.get_mut(token).unwrap().workspace = "ws-target".to_owned();
    }
    native
        .views
        .insert("o1".to_owned(), "ws-elsewhere".to_owned());
    let mig = "op-mig-held-refill";
    let mut reply = reply_shell(
        7,
        1,
        &[("o1", &rig.ws), ("o1", "ws-target"), ("o1", "ws-elsewhere")],
    );
    for token in ["w1", "w2"] {
        reply.windows.push(WindowDesire {
            lifetime: WindowId(token.to_owned()),
            output: OutputId("o1".to_owned()),
            workspace: "ws-target".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: Auth::Untouched,
            membership_authority: Auth::Controlled,
            floating: false,
            float_origin: None,
            frame_operation: None,
        });
    }
    reply.transfers.push(TransferTarget {
        operation_id: mig.to_owned(),
        kind: TransferKind::Migration,
        target_output: OutputId("o1".to_owned()),
        target_workspace: "ws-target".to_owned(),
        follow: false,
        held: false,
    });
    reply.transfers.push(TransferTarget {
        operation_id: format!("{mig}-refill"),
        kind: TransferKind::View,
        target_output: OutputId("o1".to_owned()),
        target_workspace: "ws-elsewhere".to_owned(),
        follow: false,
        held: true,
    });
    let (_, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(
        native.views.get("o1").map(String::as_str),
        Some("ws-target"),
        "target view should still actuate after arrivals"
    );
}

#[test]
fn migration_refill_scope_held_does_not_actuate() {
    // Refill scope authority: a refill naming a held scope must not actuate.
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    for token in ["w1", "w2"] {
        native.windows.get_mut(token).unwrap().workspace = "ws-target".to_owned();
    }
    native.views.insert("o1".to_owned(), "ws-fourth".to_owned());
    let mig = "op-mig-scope-refill";
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply.scopes.push(ScopeStatus {
        scope: "o1/ws-elsewhere".to_owned(),
        authority: Auth::Hold(CoreHold::IncompleteInventory),
    });
    for token in ["w1", "w2"] {
        reply.windows.push(WindowDesire {
            lifetime: WindowId(token.to_owned()),
            output: OutputId("o1".to_owned()),
            workspace: "ws-target".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: Auth::Untouched,
            membership_authority: Auth::Controlled,
            floating: false,
            float_origin: None,
            frame_operation: None,
        });
    }
    reply.transfers.push(TransferTarget {
        operation_id: mig.to_owned(),
        kind: TransferKind::Migration,
        target_output: OutputId("o1".to_owned()),
        target_workspace: "ws-target".to_owned(),
        follow: false,
        held: false,
    });
    reply.transfers.push(TransferTarget {
        operation_id: format!("{mig}-refill"),
        kind: TransferKind::View,
        target_output: OutputId("o1".to_owned()),
        target_workspace: "ws-elsewhere".to_owned(),
        follow: false,
        held: false,
    });
    let before = native.views.get("o1").cloned();
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(
        native.views.get("o1").map(String::as_str),
        Some("ws-target"),
        "scope-held refill actuated (target view overwritten)"
    );
    assert!(stats.view_writes >= 1);
    let _ = before;
}

#[test]
fn effect_focus_uses_live_homing_not_base() {
    // Switch-before-focus liveness/safety: the focus gate must read the live
    // homing, not the observation preimage. After a send arrival the window
    // lives in ws-target while the baseline still names the source; with
    // only the source view shown, focusing would steal from the wrong view.
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().workspace = "ws-target".to_owned();
    native.views.insert("o1".to_owned(), rig.ws.clone());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    reply.effects.push(OneShotEffect {
        id: "fx-live-homing".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 1,
    });
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(
        stats.effects, 0,
        "focused a moved window while its live view is hidden"
    );
    assert!(!native.is_foreground("w1"));
}

#[test]
fn float_clamp_settles_possibly_executed_without_repeat() {
    // Float clamp: the setter accepts but the app holds its frame. Proof
    // mismatches the target, so the placement settles possibly-executed
    // (never a fabricated refusal) and never repeats the setter.
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().floating = true;
    native.clamp_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    let mut row = tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Untouched;
    row.frame_operation = Some("op-float-clamp".to_owned());
    reply.windows.push(row);
    let (stats, first) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 1);
    assert!(first.iter().any(
        |f| f.operation_id == "op-float-clamp" && f.outcome == AttemptOutcome::PossiblyExecuted
    ));
    let writes = native.frame_writes;
    let (stats, second) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(
        native.frame_writes, writes,
        "clamped float repeated the setter"
    );
    assert_eq!(stats.geometry_writes, 0);
    assert!(second.is_empty());
    // The synthetic placement still settles its feedback obligation
    // through the W1 boundary: the PossiblyExecuted report rides the next
    // report and the ack consumes it, even though no core operation
    // exists for this id.
    use tiler_core::world_engine::WorldOutcome;
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("dispatch");
    let ack_reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&ack_reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    for item in &first {
        seq.note_feedback(item.clone());
    }
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("feedback dispatch");
    assert!(
        report
            .feedback
            .iter()
            .any(|f| f.operation_id == "op-float-clamp"),
        "clamp feedback did not ride the next report"
    );
    let ack_reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&ack_reply) {
        AckOutcome::Applied(receipt) => {
            assert!(
                receipt
                    .feedback_acked
                    .contains(&"op-float-clamp".to_owned())
            );
        }
        other => panic!("expected applied, got {other:?}"),
    }
}

#[test]
fn float_throw_settles_possibly_executed_without_repeat() {
    // Float throw: an ambiguous call cannot prove non-execution even though
    // the frame still names the baseline. Possibly-executed holds the phase;
    // the identical reply never repeats the setter.
    let rig = two_window_rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().floating = true;
    native.throw_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws)]);
    let mut row = tiled_row(&rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Untouched;
    row.frame_operation = Some("op-float-throw".to_owned());
    reply.windows.push(row);
    let (stats, first) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(first.iter().any(
        |f| f.operation_id == "op-float-throw" && f.outcome == AttemptOutcome::PossiblyExecuted
    ));
    let writes = native.frame_writes;
    let (_, second) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(
        native.frame_writes, writes,
        "thrown float repeated the setter"
    );
    assert!(second.is_empty());
    // The synthetic placement still settles its feedback obligation
    // through the W1 boundary: the PossiblyExecuted report rides the next
    // report and the ack consumes it, even though no core operation
    // exists for this id.
    use tiler_core::world_engine::WorldOutcome;
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("dispatch");
    let ack_reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&ack_reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    for item in &first {
        seq.note_feedback(item.clone());
    }
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("feedback dispatch");
    assert!(
        report
            .feedback
            .iter()
            .any(|f| f.operation_id == "op-float-throw"),
        "throw feedback did not ride the next report"
    );
    let ack_reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&ack_reply) {
        AckOutcome::Applied(receipt) => {
            assert!(
                receipt
                    .feedback_acked
                    .contains(&"op-float-throw".to_owned())
            );
        }
        other => panic!("expected applied, got {other:?}"),
    }
}

#[test]
fn migration_target_view_no_repeat_after_late_member_proof() {
    // Late member proof never rearms an attempted migration target view.
    // First pass holds partial arrival with no view attempt; external
    // convergence then proves every member; the target view attempts exactly
    // once (throwing here) and never repeats.
    let rig = two_window_rig();
    let (mut diff, fresh, native) = converted(&rig);
    let mut probe = ProbeNative::wrapping(native);
    probe.inner.windows.get_mut("w1").unwrap().workspace = "ws-target".to_owned();
    probe.inner.views.insert("o1".to_owned(), rig.ws.clone());
    probe.inner.throw_view.insert("o1/ws-target".to_owned());
    let mig = "op-mig-late-view";
    let mut reply = reply_shell(7, 1, &[("o1", &rig.ws), ("o1", "ws-target")]);
    // The straggler move itself throws, so the first pass holds partial
    // arrival with no target view attempt yet.
    probe.inner.throw_membership.insert("w2".to_owned());
    for token in ["w1", "w2"] {
        reply.windows.push(WindowDesire {
            lifetime: WindowId(token.to_owned()),
            output: OutputId("o1".to_owned()),
            workspace: "ws-target".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: Auth::Untouched,
            membership_authority: Auth::Controlled,
            floating: false,
            float_origin: None,
            frame_operation: None,
        });
    }
    reply.transfers.push(TransferTarget {
        operation_id: mig.to_owned(),
        kind: TransferKind::Migration,
        target_output: OutputId("o1".to_owned()),
        target_workspace: "ws-target".to_owned(),
        follow: false,
        held: false,
    });
    // Partial arrival: w2 still sources, so no target view attempt yet.
    let (stats, first) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert_eq!(probe.switches, 0, "target view ran before full arrival");
    assert!(
        first
            .iter()
            .any(|f| f.operation_id == mig && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
    assert!(stats.view_writes == 0);
    // External convergence proves the straggler; the target view attempts
    // exactly once and fails truthfully.
    probe.inner.throw_membership.clear();
    probe.inner.windows.get_mut("w2").unwrap().workspace = "ws-target".to_owned();
    let (_, second) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert_eq!(
        probe.switches, 1,
        "late arrival did not attempt the target view once"
    );
    assert!(
        second
            .iter()
            .any(|f| f.operation_id == mig && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
    // The identical reply never repeats the spent target view.
    let (_, third) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert_eq!(
        probe.switches, 1,
        "spent migration target view re-attempted"
    );
    assert!(third.is_empty());
}

#[test]
fn ambiguous_refill_no_repeat_and_third_location() {
    // Refill owns its own view phase under the actual refill id. A throwing
    // refill holds ambiguously without repeating; a later third-location
    // proof supersedes instead of fighting the user.
    let rig = two_window_rig();
    let (mut diff, fresh, native) = converted(&rig);
    let mut probe = ProbeNative::wrapping(native);
    for token in ["w1", "w2"] {
        probe.inner.windows.get_mut(token).unwrap().workspace = "ws-target".to_owned();
    }
    probe
        .inner
        .views
        .insert("o1".to_owned(), "ws-target".to_owned());
    // The refill targets a second output so its third-location proof stays
    // independent of the migration target view on o1.
    probe
        .inner
        .views
        .insert("o2".to_owned(), "ws-o2-base".to_owned());
    probe.inner.throw_view.insert("o2/ws-elsewhere".to_owned());
    let mig = "op-mig-refill-amb";
    let mut reply = reply_shell(
        7,
        1,
        &[
            ("o1", &rig.ws),
            ("o1", "ws-target"),
            ("o2", "ws-o2-base"),
            ("o2", "ws-elsewhere"),
        ],
    );
    for token in ["w1", "w2"] {
        reply.windows.push(WindowDesire {
            lifetime: WindowId(token.to_owned()),
            output: OutputId("o1".to_owned()),
            workspace: "ws-target".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: Auth::Untouched,
            membership_authority: Auth::Controlled,
            floating: false,
            float_origin: None,
            frame_operation: None,
        });
    }
    reply.transfers.push(TransferTarget {
        operation_id: mig.to_owned(),
        kind: TransferKind::Migration,
        target_output: OutputId("o1".to_owned()),
        target_workspace: "ws-target".to_owned(),
        follow: false,
        held: false,
    });
    reply.transfers.push(TransferTarget {
        operation_id: format!("{mig}-refill"),
        kind: TransferKind::View,
        target_output: OutputId("o2".to_owned()),
        target_workspace: "ws-elsewhere".to_owned(),
        follow: false,
        held: false,
    });
    let refill_op = format!("{mig}-refill");
    // Members and target view already prove; the refill throws once.
    let (_, first) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert_eq!(probe.switches, 1, "refill did not attempt exactly once");
    assert!(
        first
            .iter()
            .any(|f| f.operation_id == refill_op && f.outcome == AttemptOutcome::PossiblyExecuted),
        "throwing refill did not settle possibly-executed"
    );
    // The identical reply never repeats the ambiguous refill.
    let (_, _second) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert_eq!(
        probe.switches, 1,
        "ambiguous refill re-attempted the switch"
    );
    // A fresh third-location proof on the refill output supersedes instead
    // of refusing. The migration target on o1 still shows, so only the
    // refill phase settles.
    probe.inner.throw_view.clear();
    probe
        .inner
        .views
        .insert("o2".to_owned(), "ws-third".to_owned());
    let (_, third) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert!(
        third.iter().any(|f| f.operation_id == refill_op
            && f.outcome == AttemptOutcome::SupersededExternal),
        "third-location refill proof did not supersede"
    );
}

#[test]
fn core_float_roundtrip_applied_with_feedback() {
    // Production float placement: the core retains an explicit placement
    // under a stable operation id; the facade places it exactly once and
    // reports Applied under the core-consumed id. Settlement is by
    // observation, not by ack alone: only after the adapter re-observes
    // the placed frame does a later reply drop the retained operation,
    // and the facade then converges with nothing to actuate.
    use tiler_core::world_engine::WorldOutcome;
    let mut rig = two_window_rig();
    let before = native_fingerprint(&rig.state);
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    seq.invalidate();
    let float = WorldCommand::Operation(tiler_core::boundary::CoreCommand::ToggleFloat {
        window: "w1".to_owned(),
        float_rect: None,
    });
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            Some(float),
        )
        .expect("float dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted float, got {other:?}"),
    };
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "w1")
        .expect("float row present");
    let op = row
        .frame_operation
        .clone()
        .expect("float operation retained");
    let target = row.effective_frame.expect("float target carried");
    assert_eq!(row.frame_authority, Auth::Untouched);
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    // The facade places the core-produced target exactly once under the
    // retained id, with legitimate floating state allowed.
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native.windows.get_mut("w1").unwrap().floating = true;
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(stats.geometry_writes >= 1);
    assert!(
        produced
            .iter()
            .any(|f| f.operation_id == op && f.outcome == AttemptOutcome::Applied)
    );
    assert_eq!(native.read_frame("w1"), Some(target));
    diff.gate.note_applied(&reply);
    let writes = native.frame_writes;
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(stats.stopped_mid_apply);
    assert_eq!(native.frame_writes, writes);
    for item in &produced {
        seq.note_feedback(item.clone());
    }
    assert!(seq.has_pending_feedback());
    // Native re-observation: the adapter's next enumeration sees the
    // placed frame on a floating member, so the report carries rect ==
    // target with the floating lane set (owner float intent, the same
    // table production capture reads). The ack alone never settles a
    // float; only this observation proof clears the retained placement
    // core-side.
    if let Some(entry) = rig.observed.iter_mut().find(|w| w.token == "w1") {
        *entry = observed_window(&win_key(1001, 2001, "proc-1"), "w1", target, plain_facts());
    } else {
        panic!("w1 observed row missing");
    }
    rig.state.floated.insert(win_key(1001, 2001, "proc-1"));
    seq.invalidate();
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(seq.is_dirty(&captured));
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("feedback dispatch");
    assert!(report.feedback.iter().any(|f| f.operation_id == op));
    let reply2 = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply2) {
        AckOutcome::Applied(receipt) => {
            assert!(receipt.feedback_acked.contains(&op));
        }
        other => panic!("expected applied, got {other:?}"),
    }
    // The core settled the retained placement by observation: the later
    // reply carries no frame operation for w1.
    let row2 = reply2
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "w1")
        .expect("float row present");
    assert!(
        row2.frame_operation.is_none(),
        "retained float op not cleared by observation"
    );
    // The facade converges on the settled reply against the converged
    // native: the placed float frame is undisturbed, nothing re-actuates,
    // nothing is reported.
    diff.gate.current_obs = seq.current_obs();
    let captured2 = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh2 = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured2);
    let (stats2, produced2) =
        super::world_diff::apply_world_reply(&mut diff, &reply2, &fresh2, &mut native);
    assert_eq!(stats2.geometry_writes, 0, "settled float re-actuated");
    assert!(produced2.is_empty());
    assert_eq!(native.read_frame("w1"), Some(target));
    assert_eq!(native_fingerprint(&rig.state), before);
}

#[test]
fn core_view_roundtrip_failure_no_repeat() {
    // Production view switch: the core retains a View transfer; the facade
    // attempts it once and settles RefusedProven on explicit refusal with
    // unchanged preimage (never a fabricated refusal: Ok(true) with old or
    // unreadable proof stays PossiblyExecuted). The spent phase never
    // repeats the switch, and the feedback ack converges next-report.
    use tiler_core::world_engine::WorldOutcome;
    let rig = send_rig();
    let before = native_fingerprint(&rig.state);
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    seq.invalidate();
    let switch = WorldCommand::SwitchView {
        output: OutputId("o1".to_owned()),
        workspace: rig.ws2.clone(),
    };
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            Some(switch),
        )
        .expect("view dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted view, got {other:?}"),
    };
    assert!(!reply.transfers.is_empty());
    let op = reply.transfers[0].operation_id.clone();
    assert!(matches!(reply.transfers[0].kind, TransferKind::View));
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured);
    let native = FakeNative::seeded(&fresh);
    let mut probe = ProbeNative::wrapping(native);
    probe.inner.fail_view.insert(format!("o1/{}", rig.ws2));
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    let (stats, first) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert_eq!(stats.view_writes, 0);
    // An explicit refusal with unchanged preimage proves non-execution:
    // RefusedProven settles the spent phase and never repeats the switch.
    assert!(
        first
            .iter()
            .any(|f| f.operation_id == op && f.outcome == AttemptOutcome::RefusedProven)
    );
    assert_eq!(probe.switches, 1);
    let (stats, second) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert_eq!(probe.switches, 1, "ambiguous view repeated the switch");
    assert_eq!(stats.view_writes, 0);
    assert!(second.is_empty());
    for item in &first {
        seq.note_feedback(item.clone());
    }
    seq.invalidate();
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(seq.is_dirty(&captured));
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("feedback dispatch");
    assert!(report.feedback.iter().any(|f| f.operation_id == op));
    let reply2 = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply2) {
        AckOutcome::Applied(receipt) => {
            assert!(receipt.feedback_acked.contains(&op));
        }
        other => panic!("expected applied, got {other:?}"),
    }
    diff.gate.current_obs = seq.current_obs();
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply2, &fresh, &mut probe);
    assert_eq!(stats.view_writes, 0);
    assert!(produced.is_empty());
    assert!(!stats.stopped_mid_apply);
    assert_eq!(native_fingerprint(&rig.state), before);
    let _ = rig.ws1;
}

#[test]
fn core_creation_roundtrip_binds_without_steal() {
    // Production creation: SelectTrailing mints a pending logical; the
    // facade creates at most once, binds strictly from the direct result,
    // never steals an unrelated external workspace and never uses the
    // logical token as a native id.
    use tiler_core::world_engine::WorldOutcome;
    // Occupy every live workspace so trailing selection must mint a pending
    // logical instead of reusing a live empty.
    let mut rig = two_window_rig();
    {
        let second = rig.state.workspaces.workspace_ids(DEV1)[1].clone();
        let k3 = win_key(1003, 2003, "proc-3");
        admit(&mut rig.state, &k3, DEV1, &second, "w3", false);
        rig.observed.push(observed_window(
            &k3,
            "w3",
            rect(0, 0, 400, 300),
            plain_facts(),
        ));
    }
    let before = native_fingerprint(&rig.state);
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    seq.invalidate();
    let trailing = WorldCommand::SelectTrailing {
        output: OutputId("o1".to_owned()),
    };
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            Some(trailing),
        )
        .expect("creation dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted creation, got {other:?}"),
    };
    let pending = reply
        .workspaces
        .iter()
        .find(|w| w.native_binding.is_none() && w.existence)
        .expect("pending creation row present");
    let logical = pending
        .logical_id
        .clone()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| pending.id.clone());
    assert!(!logical.is_empty());
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native.desktops.insert(rig.ws.clone());
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    native.create_result = Some(Some("ws-made".to_owned()));
    let (stats, feedback) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.creations, 1);
    assert!(feedback.is_empty(), "creation emits no core feedback");
    assert!(diff.creations.is_bound(&logical));
    assert!(native.desktops.contains("ws-made"));
    // An unrelated external workspace appearing meanwhile is never stolen,
    // and the logical token is never used as a native id.
    native.desktops.insert("ws-external".to_owned());
    assert!(!diff.creations.is_bound("ws-external"));
    assert!(!native.desktops.contains(&logical) || logical == "ws-made");
    // Inventory reported back: the adapter observes the new native
    // workspace, so the next capture carries its binding and topology.
    // The core resolves the pending token against the live inventory:
    // no second pending row, and a bound row for the made workspace.
    assert!(rig.state.workspaces.ensure_workspace_id(DEV1, "ws-made"));
    seq.invalidate();
    diff.gate.current_obs = seq.current_obs();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("inventory dispatch");
    let reply2 = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted inventory, got {other:?}"),
    };
    assert!(
        reply2
            .workspaces
            .iter()
            .any(|w| w.native_binding.as_deref() == Some("ws-made") && w.existence),
        "core reply does not reflect the reported inventory"
    );
    assert!(
        !reply2.workspaces.iter().any(
            |w| w.native_binding.is_none() && w.logical_id.as_deref() == Some(logical.as_str())
        ),
        "pending token not resolved against live inventory"
    );
    match seq.ack(&reply2) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    // The facade converges on the resolved inventory: nothing to create,
    // nothing to remove, no theft, no logical-as-native actuation.
    let captured2 = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh2 = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured2);
    let (stats2, produced2) =
        super::world_diff::apply_world_reply(&mut diff, &reply2, &fresh2, &mut native);
    assert_eq!(stats2.creations, 0);
    assert_eq!(stats2.removals, 0);
    assert!(produced2.is_empty());
    assert!(!native.desktops.contains(&logical) || logical == "ws-made");
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.creations, 0, "creation repeated the setter");
    assert_eq!(native_fingerprint(&rig.state), before);
}

#[test]
fn core_follow_success_roundtrip_and_convergence() {
    // Production follow send: the core retains a follow window transfer;
    // the facade moves membership, switches the exact target view, and
    // reports Applied once under the core-consumed id. After the adapter
    // re-observes arrival, the core settles the transfer by homing alone
    // (known contract limitation: a failed follow tail is best-effort and
    // leaves no core view id), so the next reply carries no transfer and
    // the facade converges with nothing left to actuate.
    use tiler_core::world_engine::WorldOutcome;
    let mut rig = send_rig();
    let before = native_fingerprint(&rig.state);
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    seq.invalidate();
    let send = WorldCommand::Send {
        window: WindowId("w1".to_owned()),
        target_output: OutputId("o1".to_owned()),
        target_workspace: rig.ws2.clone(),
        follow: true,
    };
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            Some(send),
        )
        .expect("follow dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted follow, got {other:?}"),
    };
    assert_eq!(reply.transfers.len(), 1);
    assert!(
        reply.transfers[0].follow,
        "core did not retain a follow transfer"
    );
    let op = reply.transfers[0].operation_id.clone();
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    // The facade actuates the retained follow transfer against the offline
    // native: membership proves by read-back and the exact target view
    // switches, all under the real operation id.
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 1);
    assert_eq!(stats.view_writes, 1);
    assert!(
        produced
            .iter()
            .any(|f| f.operation_id == op && f.outcome == AttemptOutcome::Applied)
    );
    assert_eq!(
        native.read_homing("w1"),
        Some(("o1".to_owned(), rig.ws2.clone()))
    );
    assert_eq!(native.read_view("o1").as_deref(), Some(rig.ws2.as_str()));
    // Consuming the revision arms the gate: the identical reply authorizes
    // nothing more, so the follow view never re-switches.
    diff.gate.note_applied(&reply);
    let (stats, _) = super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut native);
    assert!(stats.stopped_mid_apply);
    // Native re-observation: the adapter observes the arrival, so the next
    // report homes w1 at the target. The core settles the transfer by
    // homing alone and the feedback obligation acks through.
    let ws2 = rig.ws2.clone();
    assert!(
        rig.state
            .workspaces
            .assign(win_key(1001, 2001, "proc-1"), DEV1, &ws2, false)
    );
    for item in &produced {
        seq.note_feedback(item.clone());
    }
    seq.invalidate();
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(seq.is_dirty(&captured));
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("feedback dispatch");
    assert!(report.feedback.iter().any(|f| f.operation_id == op));
    let reply2 = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        reply2.transfers.iter().all(|t| t.operation_id != op),
        "transfer not settled by homing observation"
    );
    match seq.ack(&reply2) {
        AckOutcome::Applied(receipt) => {
            assert!(receipt.feedback_acked.contains(&op));
        }
        other => panic!("expected applied, got {other:?}"),
    }
    // The facade converges on the settled reply: no membership left to
    // move and no feedback left to emit.
    diff.gate.current_obs = seq.current_obs();
    let captured2 = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh2 = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured2);
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply2, &fresh2, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(produced.is_empty());
    assert!(!stats.stopped_mid_apply);
    assert_eq!(native_fingerprint(&rig.state), before);
    let _ = rig.ws1;
}

#[test]
fn core_follow_failure_no_repeat_and_catchup() {
    // Production follow failure: membership lands but the follow view
    // misses, so the transfer stays PossiblyExecuted without ever claiming
    // a view outcome on the window id. The identical reply never repeats
    // the switch; after arrival is re-observed the core settles the
    // transfer by homing alone, and an advanced watermark authorizes
    // nothing further (catch-up fencing).
    use tiler_core::world_engine::WorldOutcome;
    let mut rig = send_rig();
    let before = native_fingerprint(&rig.state);
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    seq.invalidate();
    let send = WorldCommand::Send {
        window: WindowId("w1".to_owned()),
        target_output: OutputId("o1".to_owned()),
        target_workspace: rig.ws2.clone(),
        follow: true,
    };
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            Some(send),
        )
        .expect("follow dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted follow, got {other:?}"),
    };
    assert!(
        reply.transfers[0].follow,
        "core did not retain a follow transfer"
    );
    let op = reply.transfers[0].operation_id.clone();
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured);
    let native = FakeNative::seeded(&fresh);
    let mut probe = ProbeNative::wrapping(native);
    probe.inner.fail_view.insert(format!("o1/{}", rig.ws2));
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    let (stats, first) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert_eq!(stats.membership_writes, 1);
    assert_eq!(stats.view_writes, 0);
    assert!(
        first
            .iter()
            .any(|f| f.operation_id == op && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
    assert!(
        first
            .iter()
            .all(|f| f.operation_id != op || f.kind == AttemptKind::Transfer),
        "follow claimed a view outcome on the window id"
    );
    assert_eq!(probe.switches, 1);
    // The identical reply never repeats the spent follow view: silent
    // hold with no new switch and no repeat feedback.
    let (stats, second) =
        super::world_diff::apply_world_reply(&mut diff, &reply, &fresh, &mut probe);
    assert_eq!(
        probe.switches, 1,
        "ambiguous follow view re-attempted the switch"
    );
    assert_eq!(stats.membership_writes, 0);
    assert!(second.is_empty());
    // Arrival re-observed: the next report homes w1 at the target, the
    // core settles the transfer by homing alone, and the feedback acks.
    let ws2 = rig.ws2.clone();
    assert!(
        rig.state
            .workspaces
            .assign(win_key(1001, 2001, "proc-1"), DEV1, &ws2, false)
    );
    for item in &first {
        seq.note_feedback(item.clone());
    }
    seq.invalidate();
    let captured = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(seq.is_dirty(&captured));
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("feedback dispatch");
    assert!(report.feedback.iter().any(|f| f.operation_id == op));
    let reply2 = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        reply2.transfers.iter().all(|t| t.operation_id != op),
        "transfer not settled by homing observation"
    );
    match seq.ack(&reply2) {
        AckOutcome::Applied(receipt) => {
            assert!(receipt.feedback_acked.contains(&op));
        }
        other => panic!("expected applied, got {other:?}"),
    }
    diff.gate.current_obs = seq.current_obs();
    let captured2 = capture_world(
        &rig.state,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh2 = FreshHost::from_sources(&rig.state, &rig.observed, &rig.retained, &captured2);
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply2, &fresh2, &mut probe);
    assert_eq!(stats.membership_writes, 0);
    assert!(produced.is_empty());
    assert!(!stats.stopped_mid_apply);
    // Catch-up fencing: a watermark advanced past the reply (the same
    // condition racing native input produces) authorizes no setter.
    diff.gate.current_obs = seq.current_obs() + 1;
    let (stats, produced) =
        super::world_diff::apply_world_reply(&mut diff, &reply2, &fresh2, &mut probe);
    assert!(stats.stopped_mid_apply);
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(stats.view_writes, 0);
    assert_eq!(stats.geometry_writes, 0);
    assert!(produced.is_empty());
    assert_eq!(native_fingerprint(&rig.state), before);
    let _ = rig.ws1;
}

#[test]
fn core_migration_refused_without_adjacency() {
    // Empirical product boundary, not coverage of a migration roundtrip:
    // Windows capture deliberately emits no domain adjacency (the
    // accepted W1 capture withholds the KDE-style computation for lack
    // of a native evidence route), while the core resolves every
    // migration direction strictly through carried adjacency. A real
    // two-output MigrateWorkspace command over real inventory, bindings
    // and monitor areas therefore refuses, so no core-produced Migration
    // (and no paired refill) can arise from this path without a product
    // adjacency source. This test locks the refusal as the exact stop
    // evidence for the missing roundtrip.
    use tiler_core::world_engine::WorldOutcome;
    let rig = two_output_rig();
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            None,
        )
        .expect("dispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    seq.invalidate();
    let migrate = WorldCommand::MigrateWorkspace {
        source_output: OutputId("o1".to_owned()),
        target_output: OutputId("o2".to_owned()),
        workspace: rig.ws_a.clone(),
    };
    let report = seq
        .try_dispatch(
            &rig.state,
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            Some(migrate),
        )
        .expect("migration dispatch");
    match seq.deliver(&report) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "migration-refused"),
        other => panic!("expected migration refusal, got {other:?}"),
    }
    let _ = rig.ws_b;
}
