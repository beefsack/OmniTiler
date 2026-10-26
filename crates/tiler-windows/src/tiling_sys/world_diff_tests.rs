#![forbid(unsafe_code)]
//! Offline capture/sequencer/diff safety regressions (source-first).
//!
//! Every rig builds the real owner tables plus real per-tick products and
//! runs them through [`capture_world`](super::world_capture::capture_world)
//! and [`FreshHost::from_sources`](super::world_diff::FreshHost::from_sources).
//! Fixtures diverge only the live [`FakeNative`] facts per fence. Native
//! workspace creation/removal/binding shapes are intentionally absent: the logical
//! core owns workspaces outright, so there is nothing to actuate there.

use super::rmax03_adapter_tests::test_state;
use super::world_capture::{capture_world, resolve_cursor_hit};
use super::world_diff::{
    ApplyStats, FakeNative, ForegroundState, FreshHost, FreshWindow, GuiFocusState, HostDiff,
    MAX_CLEAR_HOLD_DIAG, NativeOps, apply_world_reply,
};
use super::world_sequencer::{AckOutcome, DispatchError, WorldSequencer};
use super::{HiddenRecord, MonitorArea, ObservedWindow, PendingClosed, RetainedRow, TileLoop};
use crate::model::{ProcessIdentity, WindowClaimKind, WindowIdentity, WindowShowState};
use crate::native_guards::WindowKey;
use crate::tiling::{FrameInsets, ObservedTarget, WindowFacts};
use std::collections::BTreeMap;
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report::{
    EffectKind, FrameSetter, HoldReason as CoreHold, OneShotEffect, ScopeAuthority as Auth,
    ScopeStatus, TransferKind, TransferTarget, WORLD_CONTRACT_VERSION, WindowDesire, WorldCommand,
    WorldReply, WorldReport,
};

pub(super) const ADAPTER: &str = "adapter-1";
pub(super) const GENERATION: &str = "gen-1";
pub(super) const DEV1: &str = "DEV1";
const EXE: &str = "C:\\Apps\\app.exe";

pub(super) fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

pub(super) fn win_key(hwnd: u64, pid: u32, creation: &str) -> WindowKey {
    WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
    }
}

pub(super) fn plain_facts() -> WindowFacts {
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

pub(super) fn observed_window(
    key: &WindowKey,
    token: &str,
    vis: Rect,
    facts: WindowFacts,
) -> ObservedWindow {
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
            // Generic fixture-style identity: the proof-only helper tag is
            // empty; managed membership rides `member_tag` below.
            tag: String::new(),
        },
        facts,
        member_tag: Some("member-tag-1".to_owned()),
    }
}

pub(super) fn process_identity(key: &WindowKey) -> ProcessIdentity {
    ProcessIdentity {
        pid: key.pid,
        process_creation: key.creation.clone(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: EXE.to_owned(),
    }
}

pub(super) fn admit(
    state: &mut TileLoop,
    key: &WindowKey,
    _dev: &str,
    _ws: &str,
    token: &str,
    _hidden: bool,
) {
    state.member_tokens.insert(key.clone(), token.to_owned());
    state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    state
        .member_identity
        .insert(key.clone(), process_identity(key));
}

fn one_area(dev: &str) -> Vec<MonitorArea> {
    vec![MonitorArea {
        device: dev.to_owned(),
        work: rect(0, 0, 1920, 1040),
        full: rect(0, 0, 1920, 1080),
    }]
}

pub(super) struct Rig {
    pub(super) state: TileLoop,
    pub(super) observed: Vec<ObservedWindow>,
    pub(super) retained: Vec<RetainedRow>,
    pub(super) areas: Vec<MonitorArea>,
    pub(super) out: String,
    pub(super) ws: String,
}

pub(super) fn rig() -> Rig {
    let mut state = test_state();
    state.ensure_device(DEV1);
    let ws = "ws1".to_owned();
    let out = state.output_token(DEV1);
    let k1 = win_key(1001, 2001, "proc-1");
    let k2 = win_key(1002, 2002, "proc-2");
    admit(&mut state, &k1, DEV1, &ws, "w1", false);
    admit(&mut state, &k2, DEV1, &ws, "w2", false);
    // Proven receipts: both members home on the source view, so capture
    // proves observed workspaces and baselines are known (never ""). Tests
    // needing an unknown preimage construct their own FreshHost.
    for token in ["w1", "w2"] {
        state
            .applied_homing
            .insert(token.to_owned(), (out.clone(), ws.clone()));
    }
    let observed = vec![
        observed_window(&k1, "w1", rect(0, 0, 800, 600), plain_facts()),
        observed_window(&k2, "w2", rect(0, 610, 800, 400), plain_facts()),
    ];
    Rig {
        state,
        observed,
        retained: Vec::new(),
        areas: one_area(DEV1),
        out,
        ws,
    }
}

fn diff_at(obs: u64) -> HostDiff {
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = obs;
    diff
}

fn converted(rig: &Rig) -> (HostDiff, FreshHost, FakeNative) {
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(captured.inventory_complete);
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    let native = FakeNative::seeded(&fresh);
    (diff_at(7), fresh, native)
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

fn tiled_row(out: &str, ws: &str, token: &str, target: Rect) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(token.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: ws.to_owned(),
        allocation: Some(target),
        effective_frame: Some(target),
        frame_authority: Auth::Controlled,
        membership_authority: Auth::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
        clear_operation: None,
    }
}

fn member_row(out: &str, ws: &str, token: &str) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(token.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: ws.to_owned(),
        allocation: None,
        effective_frame: None,
        frame_authority: Auth::Untouched,
        membership_authority: Auth::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
        clear_operation: None,
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

fn apply(
    diff: &mut HostDiff,
    reply: &WorldReply,
    fresh: &FreshHost,
    native: &mut FakeNative,
) -> (
    ApplyStats,
    Vec<tiler_core::world_report::MemberAttempt>,
    Vec<tiler_core::world_report::ViewAttempt>,
) {
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        diff,
        reply,
        fresh,
        native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    (stats, members, views)
}

/// Diff helper that keeps the mechanical raw attempts: the seam-level
/// proof that writes report raw facts (never verdicts) for the core.
fn apply_frames(
    diff: &mut HostDiff,
    reply: &WorldReply,
    fresh: &FreshHost,
    native: &mut FakeNative,
    attempt_ms: u64,
) -> (
    ApplyStats,
    Vec<tiler_core::world_report::FrameAttempt>,
    Vec<tiler_core::world_report::MemberAttempt>,
    Vec<tiler_core::world_report::ViewAttempt>,
) {
    let mut frames = Vec::new();
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        diff,
        reply,
        fresh,
        native,
        &mut frames,
        &mut members,
        &mut views,
        &mut Vec::new(),
        attempt_ms,
    );
    (stats, frames, members, views)
}

fn fingerprint(state: &TileLoop) -> (usize, usize, usize, usize) {
    (
        state.pending_closed.len(),
        state.member_tokens.len(),
        state.member_tags.len(),
        state.member_identity.len(),
    )
}

// --- capture truthfulness ---

#[test]
fn failed_enumeration_holds_every_scope() {
    let rig = rig();
    let captured = capture_world(&rig.state.inputs(), None, &rig.retained, &rig.areas, 1001);
    assert!(!captured.inventory_complete);
    assert_eq!(captured.windows.len(), 2);
    for fact in &captured.windows {
        assert!(fact.rect.is_none() && !fact.minimized);
    }
    assert!(captured.closed.is_empty());
    assert_eq!(captured.held_members.len(), 2);
}

#[test]
fn token_mismatch_holds_without_inheritance() {
    let mut rig = rig();
    rig.observed[0].token = "w1-recycled".to_owned();
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let w1 = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w1")
        .expect("w1 fact");
    assert!(w1.rect.is_none() && !w1.minimized);
    assert!(!captured.inventory_complete);
}

#[test]
fn hidden_without_claim_holds() {
    let mut state = test_state();
    state.ensure_device(DEV1);
    let ws = "ws1".to_owned();
    admit(
        &mut state,
        &win_key(1001, 2001, "proc-1"),
        DEV1,
        &ws,
        "w1",
        true,
    );
    let captured = capture_world(&state.inputs(), Some(&[]), &[], &one_area(DEV1), 0);
    let fact = &captured.windows[0];
    assert!(fact.rect.is_none() && !fact.minimized);
    assert!(!captured.inventory_complete);
}

#[test]
fn hidden_iconic_minimizes() {
    let mut state = test_state();
    state.ensure_device(DEV1);
    let ws = "ws1".to_owned();
    let key = win_key(1001, 2001, "proc-1");
    admit(&mut state, &key, DEV1, &ws, "w1", true);
    state.hidden_claims.insert(
        key.clone(),
        HiddenRecord {
            claim: WindowIdentity {
                hwnd: 1001,
                process: process_identity(&key),
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
    let captured = capture_world(&state.inputs(), Some(&[]), &[], &one_area(DEV1), 0);
    let fact = &captured.windows[0];
    assert!(fact.minimized && fact.rect.is_none());
}

#[test]
fn hidden_snapshot_rides_slot_with_verified_fullscreen() {
    let mut state = test_state();
    state.ensure_device(DEV1);
    let ws = "ws1".to_owned();
    let key = win_key(1001, 2001, "proc-1");
    admit(&mut state, &key, DEV1, &ws, "w1", true);
    state.hidden_claims.insert(
        key.clone(),
        HiddenRecord {
            claim: WindowIdentity {
                hwnd: 1001,
                process: process_identity(&key),
                tag: "0123456789abcdef".to_owned(),
                kind: WindowClaimKind::Product,
                show: WindowShowState {
                    minimized: false,
                    maximized: false,
                },
            },
            iconic: false,
        },
    );
    let slot = rect(5, 5, 400, 300);
    state.member_rects.insert("w1".to_owned(), slot);
    state.born_fullscreen.insert(key.clone());
    let captured = capture_world(&state.inputs(), Some(&[]), &[], &one_area(DEV1), 0);
    let fact = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w1")
        .expect("w1 fact");
    assert_eq!(fact.rect, Some(slot));
    assert!(fact.fullscreen && !fact.minimized);
}

#[test]
fn minimized_omission_minimizes_frameless_holds() {
    let mut state = test_state();
    state.ensure_device(DEV1);
    let ws = "ws1".to_owned();
    let k1 = win_key(1001, 2001, "proc-1");
    let k2 = win_key(1002, 2002, "proc-2");
    admit(&mut state, &k1, DEV1, &ws, "w1", false);
    admit(&mut state, &k2, DEV1, &ws, "w2", false);
    state.minimized_omitted.insert(k1.clone());
    let retained = vec![RetainedRow {
        key: k2.clone(),
        token: "w2".to_owned(),
        rect: None,
        maximized: false,
        fullscreen: false,
        facts: None,
    }];
    let captured = capture_world(&state.inputs(), Some(&[]), &retained, &one_area(DEV1), 0);
    let w1 = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w1")
        .expect("w1");
    let w2 = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w2")
        .expect("w2");
    assert!(w1.minimized && w1.rect.is_none());
    assert!(!w2.minimized && w2.rect.is_none());
    assert!(!captured.inventory_complete);
}

#[test]
fn scope_excluded_holds_without_geometry() {
    let mut rig = rig();
    rig.state.scope = vec!["other.exe".to_owned()];
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    for fact in &captured.windows {
        assert!(fact.rect.is_none() && !fact.minimized);
    }
    assert!(!captured.inventory_complete);
}

#[test]
fn foreign_never_member_observed_does_not_fence_scoped_inventory() {
    // Scoped fixture-only inventory is complete over the scoped managed
    // set: a foreign (out-of-scope, never-member) observed window is
    // ignored, never ridden as unreadable and never clearing the flag.
    // A bound member that drifts out of scope still holds (covered by
    // `scope_excluded_holds_without_geometry` above).
    let mut rig = rig();
    rig.state.scope = vec!["app.exe".to_owned()];
    let foreign_key = win_key(9999, 2999, "proc-9");
    let foreign = ObservedWindow {
        hwnd: foreign_key.hwnd,
        token: "w9".to_owned(),
        outer: rect(0, 0, 800, 600),
        visible: rect(0, 0, 800, 600),
        insets: crate::tiling::FrameInsets::measure(rect(0, 0, 800, 600), rect(0, 0, 800, 600)),
        identity: ObservedTarget {
            hwnd: foreign_key.hwnd,
            pid: foreign_key.pid,
            process_creation: foreign_key.creation.clone(),
            exe_path: "C:\\Other\\foreign.exe".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: plain_facts(),
        member_tag: None,
    };
    let mut observed = rig.observed.clone();
    observed.push(foreign);
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(
        captured.inventory_complete,
        "foreign window must not fence scoped inventory: {:?}",
        captured.windows
    );
    assert_eq!(captured.windows.len(), 2);
    assert!(captured.windows.iter().all(|f| f.rect.is_some()));
    assert!(captured.held_members.is_empty());
}

#[test]
fn nonmember_retained_does_not_fence_scoped_inventory() {
    // Retained occupancy for a window that never became a member is not
    // managed inventory: it rides nothing and clears nothing. Member
    // frameless/unrideable rows keep every genuine hold.
    let mut rig = rig();
    rig.state.scope = vec!["app.exe".to_owned()];
    let foreign_key = win_key(9998, 2998, "proc-8");
    let retained = vec![RetainedRow {
        key: foreign_key,
        token: "w8".to_owned(),
        rect: Some(rect(0, 0, 800, 600)),
        maximized: false,
        fullscreen: false,
        facts: Some(plain_facts()),
    }];
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &retained,
        &rig.areas,
        1001,
    );
    assert!(
        captured.inventory_complete,
        "non-member retained must not fence scoped inventory"
    );
    assert_eq!(captured.windows.len(), 2);
}

#[test]
fn retained_overlay_rides_slot_and_holds_tiled_write() {
    let mut state = test_state();
    state.ensure_device(DEV1);
    let ws = "ws1".to_owned();
    let out = state.output_token(DEV1);
    let key = win_key(1001, 2001, "proc-1");
    admit(&mut state, &key, DEV1, &ws, "w1", false);
    let slot = rect(0, 0, 800, 600);
    state.member_rects.insert("w1".to_owned(), slot);
    let mut overlay = plain_facts();
    overlay.maximized = true;
    let retained = vec![RetainedRow {
        key: key.clone(),
        token: "w1".to_owned(),
        rect: Some(rect(0, 0, 1920, 1040)),
        maximized: true,
        fullscreen: false,
        facts: Some(overlay),
    }];
    let captured = capture_world(&state.inputs(), Some(&[]), &retained, &one_area(DEV1), 0);
    let fact = &captured.windows[0];
    assert_eq!(fact.rect, Some(slot));
    assert!(fact.maximized);
    // The reserved lane never takes an automatic tiled write.
    let fresh = FreshHost::from_sources(&state.inputs(), &[], &retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = diff_at(7);
    let mut reply = reply_shell(7, 1, &[(out.as_str(), ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&out, &ws, "w1", rect(10, 10, 400, 300)));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(members.is_empty() && views.is_empty());
}

#[test]
fn cloaked_observation_holds_truthfully() {
    let mut rig = rig();
    rig.observed[0].facts.cloaked = true;
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let w1 = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w1")
        .expect("w1");
    assert!(w1.rect.is_none() && !w1.minimized);
    assert!(!captured.inventory_complete);
}

// --- diff geometry authority ---

#[test]
fn effective_frame_writes_allocation_never_does() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(stats.held, 0);
    assert!(members.is_empty() && views.is_empty());
    assert!(!stats.partial);

    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
    row.effective_frame = None;
    reply.windows.push(row);
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(members.is_empty() && views.is_empty());
}

#[test]
fn converged_noop_reports_no_attempt() {
    // An exact desired/observed match needs no write and reports no
    // mechanical facts: the core clears retained frame memory on the
    // observed match itself. The rig observes w1 at (0,0,800,600).
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(0, 0, 800, 600)));
    let (stats, frames, members, views) =
        apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 0);
    assert!(members.is_empty() && views.is_empty());
    assert!(frames.is_empty());
    assert!(!stats.partial);
}

#[test]
fn clamped_write_reports_raw_attempt_and_obeys_core_hold() {
    // A clamping setter (accepted write, mismatched readback) reports raw
    // mechanical facts only: no clamp verdict rides the attempt. The core's
    // next desire holds the pair, and the seam obeys the hold without
    // another setter (never desired-as-proof); a changed desire re-arms.
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.clamp_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, frames, members, views) =
        apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(stats.held, 1);
    assert!(members.is_empty() && views.is_empty());
    assert_eq!(frames.len(), 1);
    let attempt = &frames[0];
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.target, rect(10, 10, 400, 300));
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert!(
        attempt
            .readback
            .is_some_and(|readback| readback != attempt.target)
    );
    let writes = native.frame_writes;
    let mut held = reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]);
    let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Hold(CoreHold::ConstraintClamp);
    held.windows.push(row);
    let (stats, _, held_frames, _) = apply_frames(&mut diff, &held, &fresh, &mut native, 2000);
    assert_eq!(native.frame_writes, writes);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(held_frames.is_empty());
    let mut moved = reply_shell(7, 3, &[(rig.out.as_str(), rig.ws.as_str())]);
    moved
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(20, 20, 400, 300)));
    let (stats, _, _, _) = apply_frames(&mut diff, &moved, &fresh, &mut native, 3000);
    assert_eq!(stats.geometry_writes, 1);
}

#[test]
fn explicit_refusal_reports_transient_facts_without_verdict() {
    // A refused setter reports transient facts (never a verdict): ordinary
    // geometry proves by read-back. The core times the retry; the seam
    // retries on the next controlled desire.
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.fail_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, frames, members, views) =
        apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(members.is_empty() && views.is_empty());
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].setter, FrameSetter::Refused);
    assert_eq!(frames[0].target, rect(10, 10, 400, 300));
    // A controlled re-desire retries the setter (no local backoff memory).
    let writes = native.frame_writes;
    native.fail_frame.remove("w1");
    let (stats, _, _, _) = apply_frames(&mut diff, &reply, &fresh, &mut native, 2000);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(native.frame_writes, writes + 1);
}

#[test]
fn per_flag_live_divergence_holds_without_writes() {
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
        let rig = rig();
        let (mut diff, fresh, mut native) = converted(&rig);
        diverge(native.windows.get_mut("w1").expect("w1 live"));
        let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
        reply
            .windows
            .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
        let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
        assert_eq!(stats.geometry_writes, 0, "flag {name} wrote");
        assert!(
            stats.held >= 1 || stats.stopped_mid_apply,
            "flag {name} passed"
        );
        assert!(
            members.is_empty() && views.is_empty(),
            "flag {name} invented raw attempts"
        );
    }
}

#[test]
fn lifetime_tag_drift_stops_before_setters() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().live_tag = Some("recycled-tag".to_owned());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);
}

#[test]
fn gesture_watermark_generation_revision_fences() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.gesture = true;
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);

    let (mut diff, fresh, mut native) = converted(&rig);
    diff.gate.current_obs = 8;
    let reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);

    let (mut diff, fresh, mut native) = converted(&rig);
    diff.gate
        .note_applied(&reply_shell(7, 3, &[(rig.out.as_str(), rig.ws.as_str())]));
    for rev in [2, 3] {
        let stale = reply_shell(7, rev, &[(rig.out.as_str(), rig.ws.as_str())]);
        let (stats, _, _) = apply(&mut diff, &stale, &fresh, &mut native);
        assert!(stats.stopped_mid_apply, "rev {rev} authorized");
    }

    let (mut diff, fresh, mut native) = converted(&rig);
    let mut foreign = reply_shell(7, 4, &[(rig.out.as_str(), rig.ws.as_str())]);
    foreign.core_generation = "gen-9".to_owned();
    let (stats, _, _) = apply(&mut diff, &foreign, &fresh, &mut native);
    assert!(stats.stopped_mid_apply);
}

#[test]
fn racing_invalidation_stops_before_any_write() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.pending_invalidation = true;
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);
}

/// Racing native input that lands mid-apply: the first setter succeeds,
/// the invalidation fires, and the second unsafe setter never runs.
struct InvalidateAfterWrites {
    inner: FakeNative,
    remaining: usize,
}

impl NativeOps for InvalidateAfterWrites {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.inner.read_view(output)
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.inner.gesture_barrier()
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.inner.read_dpi(token)
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        let result = self.inner.write_frame(token, rect);
        if result == Ok(true) && self.remaining > 0 {
            self.remaining -= 1;
            if self.remaining == 0 {
                self.inner.pending_invalidation = true;
            }
        }
        result
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.inner.switch_view(output, logical)
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.inner.focus_window(token)
    }
    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.inner.read_cursor()
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        self.inner.move_cursor(x, y)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
    }
    fn foreground_state(&mut self) -> ForegroundState {
        self.inner.foreground_state()
    }
}

#[test]
fn invalidation_stops_after_first_write() {
    let rig = rig();
    let (mut diff, fresh, native) = converted(&rig);
    let mut probe = InvalidateAfterWrites {
        inner: native,
        remaining: 1,
    };
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w2", rect(0, 610, 700, 300)));
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.geometry_writes, 1);
    assert!(stats.stopped_mid_apply);
    assert!(stats.partial);
    assert_eq!(diff.gate.current_obs, 8);
}

/// Same-pass view switch arming a pending external invalidation: the
/// switch actuates, then the trailing focus effect holds stale without
/// dispatch and without being marked spent.
///
/// HONEST DIAGNOSTIC OF THE CURRENT BLOCKER. This test pins the failure
/// mode behind the live return-focus blocker (the switch's own SHOW/HIDE
/// WinEvent queue surfaces as a same-pass external invalidation, so the
/// remembered-focus effect the core emits for a workspace return never
/// reaches the setter). It passes by asserting the hold exactly. Do not
/// read it as approval of the outcome: a retry or re-issue policy needs
/// a separate product decision, and at-most-once semantics stay unchanged.
/// In particular this FakeNative consumes the pending flag on poll while
/// the production probe (`hook_edges_pending`) is non-consuming: see
/// `nonconsuming_pending_stops_until_drained` for the production-contract
/// shape, where no same-pass retry can succeed and only drain-before-apply
/// ordering restores progress.
struct InvalidateAfterViewSwitch {
    inner: FakeNative,
}

impl NativeOps for InvalidateAfterViewSwitch {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.inner.read_view(output)
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.inner.gesture_barrier()
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.inner.read_dpi(token)
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        self.inner.write_frame(token, rect)
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        let result = self.inner.switch_view(output, logical);
        if result == Ok(true) {
            self.inner.pending_invalidation = true;
        }
        result
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.inner.focus_window(token)
    }
    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.inner.read_cursor()
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        self.inner.move_cursor(x, y)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
    }
    fn foreground_state(&mut self) -> ForegroundState {
        self.inner.foreground_state()
    }
}

/// Shared base for the same-pass view/focus stale-hold proofs: the
/// observation preimage hides the target while the live target reads
/// visible with the view shown. Asserts nothing about behavior on its own.
fn view_return_base() -> (Rig, HostDiff, FreshHost, FakeNative) {
    let rig = rig();
    let (diff, mut fresh, mut native) = converted(&rig);
    if let Some(base) = fresh.baseline.get_mut("w1") {
        base.hidden = true;
        base.frame = None;
        base.observed_frame = None;
    }
    if let Some(live) = native.windows.get_mut("w1") {
        live.hidden = false;
        live.iconic = false;
        live.frame = Some(rect(0, 0, 800, 600));
        live.observed_frame = Some(rect(0, 0, 800, 600));
    }
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    (rig, diff, fresh, native)
}

fn view_return_reply(rig: &Rig) -> WorldReply {
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .transfers
        .push(view_transfer("op-v", &rig.out, &rig.ws));
    reply.effects.push(OneShotEffect {
        id: "fx-9".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 1,
    });
    reply
}

#[test]
fn same_pass_view_invalidation_holds_focus_stale_without_consuming_effect() {
    // Honest blocker regression: the switch's own SHOW/HIDE edges surface
    // as a same-pass external invalidation, so the trailing remembered-
    // focus effect holds stale without dispatch and without being marked
    // spent. No exemption is applied here: a proven-causality distinction
    // is not available at this gate, `LiveNative::focus_window` would
    // refuse on a pending queue anyway and burn the one-shot effect, and
    // proof of a shown view does not prove the edges are self-caused.
    let (rig, mut diff, fresh, native) = view_return_base();
    let mut probe = InvalidateAfterViewSwitch { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    // The switch itself actuated; the trailing focus never dispatched.
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert!(!probe.inner.is_foreground("w1"));
    assert!(stats.stopped_mid_apply);
    assert!(stats.partial);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert_eq!(row.id, "fx-9");
    assert_eq!(row.target, "w1");
    assert!(!row.dispatched);
    assert_eq!(row.setter, "skipped");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("stale"));
    // The effect was dropped without being marked spent: re-marking
    // succeeds because apply never consumed it. Production carries
    // one-shot effects on one reply only, so nothing retries it there.
    assert!(diff.effects.mark_effect_spent("fx-9"));
}

/// Production-contract pending probe: unlike `FakeNative` (which consumes
/// the flag on poll), the production probe (`hook_edges_pending`) is
/// non-consuming, and `LiveNative::focus_window` additionally refuses while
/// any edge waits. This wrapper models that contract: a pending flag
/// persists across polls, so no same-pass re-poll can clear it and only an
/// explicit drain (the loop's `drain_hook_edges` before apply) restores
/// progress.
struct ProdContractNative {
    inner: FakeNative,
}

impl NativeOps for ProdContractNative {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.inner.read_view(output)
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.inner.gesture_barrier()
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.inner.read_dpi(token)
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        self.inner.write_frame(token, rect)
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.inner.switch_view(output, logical)
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.inner.focus_window(token)
    }
    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.inner.read_cursor()
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        self.inner.move_cursor(x, y)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.pending_invalidation
    }
    fn foreground_state(&mut self) -> ForegroundState {
        self.inner.foreground_state()
    }
}

impl ProdContractNative {
    /// Explicit queue drain: the only modeled way pending edges clear,
    /// matching the loop's `drain_hook_edges` before apply.
    fn drain_pending(&mut self) {
        self.inner.pending_invalidation = false;
    }
}

#[test]
fn nonconsuming_pending_stops_until_drained() {
    // Production contract: pending edges do not clear on poll, so with
    // edges waiting at pass entry the view switch never dispatches and
    // the trailing focus never runs; re-applying without a drain stops
    // the same way (no same-pass retry can succeed). After an explicit
    // drain (the loop order: drain before apply) the same reply actuates
    // the view and dispatches the remembered focus through the normal
    // live-reveal path with the effect spent exactly once.
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.pending_invalidation = true;
    // A real switch always has an entry foreground; the offline seed
    // leaves none, so plant one for the guard baseline.
    native.foreground = Some("w2".to_owned());
    let mut probe = ProdContractNative { inner: native };
    let reply = view_return_reply(&rig);
    let run =
        |diff: &mut HostDiff, fresh: &FreshHost, probe: &mut ProdContractNative| -> ApplyStats {
            apply_world_reply(
                diff,
                &reply,
                fresh,
                probe,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut Vec::new(),
                0,
            )
        };
    // Leg 1: no drain. The pass stops at the view switch; the focus
    // effect never runs and stays unspent.
    let stats = run(&mut diff, &fresh, &mut probe);
    assert_eq!(stats.view_writes, 0);
    assert_eq!(stats.effects, 0);
    assert!(stats.stopped_mid_apply);
    assert!(stats.focus_dispatch.is_empty());
    assert!(diff.effects.mark_effect_spent("fx-9"));
    // Leg 2: still no drain, fresh gate. The persistent flag stops the
    // pass identically: polling again cannot clear production edges.
    let mut diff = diff_at(7);
    let stats = run(&mut diff, &fresh, &mut probe);
    assert_eq!(stats.view_writes, 0);
    assert_eq!(stats.effects, 0);
    assert!(stats.stopped_mid_apply);
    assert!(stats.focus_dispatch.is_empty());
    assert!(diff.effects.mark_effect_spent("fx-9"));
    // Leg 3: drained, fresh gate. Normal live-reveal path dispatches the
    // view and the remembered focus exactly once.
    probe.drain_pending();
    let mut diff = diff_at(7);
    let stats = run(&mut diff, &fresh, &mut probe);
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 1);
    assert!(probe.inner.is_foreground("w1"));
    assert!(!stats.stopped_mid_apply);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "accepted");
    assert!(row.foreground);
    assert_eq!(row.hold, None);
}

/// Pumped switch arms a queue edge; the nonactivating variant does not.
struct PumpedVsNonactivating {
    inner: FakeNative,
}

impl NativeOps for PumpedVsNonactivating {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.inner.read_view(output)
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.inner.gesture_barrier()
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.inner.read_dpi(token)
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        self.inner.write_frame(token, rect)
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        let result = self.inner.switch_view(output, logical);
        if result == Ok(true) {
            self.inner.pending_invalidation = true;
        }
        result
    }
    fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.inner.switch_view(output, logical)
    }
    fn foreground_state(&mut self) -> ForegroundState {
        self.inner.foreground_state()
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.inner.focus_window(token)
    }
    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.inner.read_cursor()
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        self.inner.move_cursor(x, y)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
    }
}

/// Nonactivating switch moves native foreground to an unmanaged window.
struct ForeignDuringNonactivating {
    inner: FakeNative,
}

impl NativeOps for ForeignDuringNonactivating {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.inner.read_view(output)
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.inner.gesture_barrier()
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.inner.read_dpi(token)
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        self.inner.write_frame(token, rect)
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.inner.switch_view(output, logical)
    }
    fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        let result = self.inner.switch_view(output, logical);
        if result == Ok(true) {
            self.inner.foreground = Some("foreign".to_owned());
        }
        result
    }
    fn foreground_state(&mut self) -> ForegroundState {
        self.inner.foreground_state()
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.inner.focus_window(token)
    }
    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.inner.read_cursor()
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        self.inner.move_cursor(x, y)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
    }
}

#[test]
fn remembered_focus_pure_view_nonactivating_dispatches_without_queue() {
    // Desired: a remembered-focus pure-view switch takes the nonactivating
    // path (no pump, no queue), so the trailing focus dispatches even
    // though the legacy pumped path would hold the same reply stale.
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    let mut probe = PumpedVsNonactivating { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 1);
    assert!(probe.inner.is_foreground("w1"));
    assert!(!stats.stopped_mid_apply);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "accepted");
    assert!(row.foreground);
    assert_eq!(row.hold, None);
    assert!(!diff.effects.mark_effect_spent("fx-9"));
}

#[test]
fn remembered_focus_pure_view_foreign_foreground_vetoes_with_empty_queue() {
    // Desired: a genuine foreign foreground move during a pump-free switch
    // wins even with no callbacks delivered. The focus holds without
    // dispatch and without being marked spent.
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    let mut probe = ForeignDuringNonactivating { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert!(!probe.inner.is_foreground("w1"));
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(!row.dispatched);
    assert_eq!(row.setter, "skipped");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("foreground-changed"));
    assert!(diff.effects.mark_effect_spent("fx-9"));
    assert!(!probe.inner.is_foreground("w1"));
}

/// Veto rows carry both foreground identities: the entry baseline (the
/// managed w2 member) versus the post-switch live read (the foreign
/// competitor). A later join against `member-admitted` rows (same
/// hwnd/pid/creation fields) tells a product-caused prior landing
/// (managed triple) from a genuine foreign competitor without logging
/// titles or content.
#[test]
fn remembered_focus_veto_carries_baseline_and_live_identity() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    let mut probe = ForeignDuringNonactivating { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert_eq!(row.hold, Some("foreground-changed"));
    let baseline = row.fg_baseline.as_deref().expect("veto baseline identity");
    let live = row.fg_live.as_deref().expect("veto live identity");
    // Baseline is the managed w2 member; live is the foreign competitor.
    assert!(
        baseline.contains("proc-2") && baseline.contains("2002"),
        "baseline joins managed w2: {baseline}"
    );
    assert!(
        live.contains("foreign") && !live.contains("proc-1") && !live.contains("proc-2"),
        "live joins no managed member: {live}"
    );
    assert_ne!(baseline, live);
}

/// Accepted setter whose async activation never lands: models an
/// `Ok(true)` focus call with no foreground change (stolen activation).
/// The adapter spends the one-shot on dispatch and the immediate read
/// proves nothing.
struct AcceptedNeverLands {
    inner: FakeNative,
}

impl NativeOps for AcceptedNeverLands {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.inner.read_view(output)
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.inner.gesture_barrier()
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.inner.read_dpi(token)
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        self.inner.write_frame(token, rect)
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.inner.switch_view(output, logical)
    }
    fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.inner.switch_view(output, logical)
    }
    fn foreground_state(&mut self) -> ForegroundState {
        self.inner.foreground_state()
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        let _ = token;
        Ok(true)
    }
    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.inner.read_cursor()
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        self.inner.move_cursor(x, y)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
    }
}

/// Blocker regression (no convergence fix yet): a dispatched-but-unproven
/// focus whose async landing never arrives stays lost. Leg 1 spends the
/// one-shot on an accepted setter with no immediate foreground; leg 2 is
/// the observation-only follow-up (the core mints no focus effects on
/// observations by design), so nothing can retry and the shown workspace
/// keeps the wrong focus. Genuine supersession must still win, so no
/// generic replay is applied here; convergence needs a causal
/// product-vs-competitor distinction first.
#[test]
fn remembered_focus_unproven_without_landing_loses_final_focus() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    let mut probe = AcceptedNeverLands { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "accepted");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("foreground-unproven"));
    // Guarded scope: only veto rows carry identities.
    assert!(row.fg_baseline.is_none() && row.fg_live.is_none());
    assert!(!diff.effects.mark_effect_spent("fx-9"));
    assert!(!probe.inner.is_foreground("w1"));
    // Observation-only follow-up: no minted effects, no retry possible.
    let mut diff2 = diff_at(7);
    let followup = reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]);
    let stats2 = apply_world_reply(
        &mut diff2,
        &followup,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert!(stats2.focus_dispatch.is_empty());
    assert_eq!(stats2.effects, 0);
    assert!(!probe.inner.is_foreground("w1"));
}

/// Late landing after an unproven setter: leg 1 accepts a focus for `w1`
/// that never lands (spent); leg 2 returns the view nonactivating while
/// the pending activation lands mid-switch on managed `w1`, and the
/// trailing focus targets `w2`. The foreground bracket excuses the exact
/// in-flight landing and dispatches the final focus, which proves at once.
#[test]
fn remembered_focus_managed_late_landing_proceeds_to_final_focus() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 1, "op-v1", "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "accepted");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("foreground-unproven"));
    assert!(!diff.effects.mark_effect_spent("fx-9"));
    // The accepted setter is remembered by exact identity plus lifetime.
    assert!(
        diff.in_flight
            .as_ref()
            .is_some_and(|flight| flight.token == "w1"),
        "in-flight remembers the unproven target"
    );
    // Between passes the user switches away; the pending activation is
    // still in flight and lands mid-switch on the next return.
    probe
        .inner
        .views
        .insert(rig.out.clone(), "ws-other".to_owned());
    probe.land_setter = true;
    probe.mid_switch_land = Some("w1".to_owned());
    let stats2 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 2, "op-v2", "fx-10", "w2"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.view_writes, 1);
    assert_eq!(stats2.effects, 1);
    assert_eq!(stats2.focus_dispatch.len(), 1);
    let row2 = &stats2.focus_dispatch[0];
    assert_eq!(row2.id, "fx-10");
    assert!(row2.dispatched);
    assert_eq!(row2.setter, "accepted");
    assert!(row2.foreground);
    assert_eq!(row2.hold, None);
    // Landing clears the in-flight; the final target proves at once.
    assert!(diff.in_flight.is_none());
    assert!(probe.inner.is_foreground("w2"));
}

/// Blocker regression (no convergence fix yet): a vetoed view-return
/// focus stays lost. Leg 1 switches the view but the trailing focus vetoes
/// on a changed foreground without spending; leg 2 is the observation-only
/// follow-up (the core mints no focus effects on observations by design),
/// so the shown workspace keeps the wrong focus with no retry. When the
/// veto saw a genuine foreign competitor this hold is correct; when it saw
/// a product-caused prior landing it is final-focus loss with identical
/// evidence. The new veto identity labels exist to tell the two apart
/// before any retry is attempted.
#[test]
fn remembered_focus_veto_without_retry_loses_final_focus() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    let mut probe = ForeignDuringNonactivating { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert_eq!(probe.inner.views.get(&rig.out), Some(&rig.ws));
    assert!(!probe.inner.is_foreground("w1"));
    assert!(diff.effects.mark_effect_spent("fx-9"));
    // Observation-only follow-up: view already shown, no minted effects.
    let mut diff2 = diff_at(7);
    let followup = reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]);
    let stats2 = apply_world_reply(
        &mut diff2,
        &followup,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert!(stats2.focus_dispatch.is_empty());
    assert_eq!(stats2.effects, 0);
    assert_eq!(probe.inner.views.get(&rig.out), Some(&rig.ws));
    assert!(!probe.inner.is_foreground("w1"));
}

/// Zero-baseline foreground guard: verifiably no foreground at entry and
/// at dispatch proves nothing about the switch, so the focus holds unless
/// the target already holds foreground.
#[test]
fn remembered_focus_double_zero_foreground_fails_closed() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = None;
    native.pending_invalidation = false;
    let reply = view_return_reply(&rig);
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
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(!row.dispatched);
    assert_eq!(row.hold, Some("foreground-changed"));
    assert!(diff.effects.mark_effect_spent("fx-9"));
}

/// Nonactivating view proof still required: a throwing nonactivating
/// switch proves nothing, so the trailing focus holds for the view
/// without dispatch and without spending.
#[test]
fn remembered_focus_nonactivating_failed_reveal_holds_for_view() {
    struct ThrowingNonactivating {
        inner: FakeNative,
    }
    impl NativeOps for ThrowingNonactivating {
        fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
            self.inner.read_window(token)
        }
        fn read_frame(&mut self, token: &str) -> Option<Rect> {
            self.inner.read_frame(token)
        }
        fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
            self.inner.read_homing(token)
        }
        fn read_view(&mut self, output: &str) -> Option<String> {
            self.inner.read_view(output)
        }
        fn is_foreground(&mut self, token: &str) -> bool {
            self.inner.is_foreground(token)
        }
        fn gesture_barrier(&mut self) -> bool {
            self.inner.gesture_barrier()
        }
        fn read_dpi(&mut self, token: &str) -> Option<u32> {
            self.inner.read_dpi(token)
        }
        fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
            self.inner.write_frame(token, rect)
        }
        fn move_membership(
            &mut self,
            token: &str,
            out: &str,
            ws: &str,
            frame: Option<Rect>,
        ) -> Result<bool, ()> {
            self.inner.move_membership(token, out, ws, frame)
        }
        fn switch_view(&mut self, _o: &str, _l: &str) -> Result<bool, ()> {
            self.inner.switch_view(_o, _l)
        }
        fn switch_view_nonactivating(&mut self, _o: &str, _l: &str) -> Result<bool, ()> {
            Err(())
        }
        fn foreground_state(&mut self) -> ForegroundState {
            self.inner.foreground_state()
        }
        fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
            self.inner.focus_window(token)
        }
        fn read_cursor(&mut self) -> Option<(i32, i32)> {
            self.inner.read_cursor()
        }
        fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
            self.inner.move_cursor(x, y)
        }
        fn poll_external_invalidation(&mut self) -> bool {
            self.inner.poll_external_invalidation()
        }
    }
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    let mut probe = ThrowingNonactivating { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 0);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(!row.dispatched);
    assert_eq!(row.hold, Some("view-not-shown"));
    assert!(diff.effects.mark_effect_spent("fx-9"));
}

/// Recycled HWND still stops: a live key drift fails the pre-setter fence
/// before any foreground guard, so the focus holds without dispatch and
/// without spending.
#[test]
fn remembered_focus_nonactivating_recycled_hwnd_stops() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    if let Some(live) = native.windows.get_mut("w1") {
        live.key.hwnd = 9999;
    }
    let mut probe = PumpedVsNonactivating { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(!row.dispatched);
    assert_eq!(row.hold, Some("pre-setter-stop"));
    assert!(stats.stopped_mid_apply);
    assert!(diff.effects.mark_effect_spent("fx-9"));
}

/// Refused focus setter still reports dispatched-but-unproven: the
/// foreground guard passes (no foreign move), the setter runs, the proof
/// fails, and the one-shot spends exactly once.
#[test]
fn remembered_focus_nonactivating_refused_focus_spends_once() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.fail_focus.insert("w1".to_owned());
    let mut probe = PumpedVsNonactivating { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "refused");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("foreground-unproven"));
    assert!(!diff.effects.mark_effect_spent("fx-9"));
}

/// Duplicate focus delivery never re-dispatches: the second pass holds
/// already-spent with no new view write.
#[test]
fn remembered_focus_nonactivating_duplicate_holds_spent() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    let mut probe = PumpedVsNonactivating { inner: native };
    let reply = view_return_reply(&rig);
    let first = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(first.effects, 1);
    let second = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(second.view_writes, 0);
    assert_eq!(second.effects, 0);
    assert_eq!(second.focus_dispatch.len(), 1);
    let row = &second.focus_dispatch[0];
    assert!(!row.dispatched);
    assert_eq!(row.hold, Some("already-spent"));
}

/// Lane-counting switch: distinguishes pumped vs nonactivating view calls.
struct CountingViews {
    inner: FakeNative,
    pumped: usize,
    na: usize,
}

impl NativeOps for CountingViews {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.inner.read_view(output)
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.inner.gesture_barrier()
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.inner.read_dpi(token)
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        self.inner.write_frame(token, rect)
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.pumped += 1;
        self.inner.switch_view(output, logical)
    }
    fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.na += 1;
        self.inner.switch_view(output, logical)
    }
    fn foreground_state(&mut self) -> ForegroundState {
        self.inner.foreground_state()
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.inner.focus_window(token)
    }
    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.inner.read_cursor()
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        self.inner.move_cursor(x, y)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
    }
}

#[test]
fn remembered_focus_unrelated_view_stays_pumped() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    let out2 = format!("{}-2", rig.out);
    native.views.insert(out2.clone(), "ws-other2".to_owned());
    let mut probe = CountingViews {
        inner: native,
        pumped: 0,
        na: 0,
    };
    let mut reply = reply_shell(
        7,
        1,
        &[(rig.out.as_str(), rig.ws.as_str()), (out2.as_str(), "ws-x")],
    );
    reply
        .transfers
        .push(view_transfer("op-v", &rig.out, &rig.ws));
    reply.transfers.push(view_transfer("op-v2", &out2, "ws-x"));
    reply.effects.push(OneShotEffect {
        id: "fx-9".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 1,
    });
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 2);
    assert_eq!(probe.na, 1);
    assert_eq!(probe.pumped, 1);
    assert_eq!(stats.effects, 1);
    assert!(probe.inner.is_foreground("w1"));
}

#[test]
fn remembered_focus_recycled_same_token_foreground_vetoes() {
    struct RecycleLeaving {
        inner: FakeNative,
    }
    impl NativeOps for RecycleLeaving {
        fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
            self.inner.read_window(token)
        }
        fn read_frame(&mut self, token: &str) -> Option<Rect> {
            self.inner.read_frame(token)
        }
        fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
            self.inner.read_homing(token)
        }
        fn read_view(&mut self, output: &str) -> Option<String> {
            self.inner.read_view(output)
        }
        fn is_foreground(&mut self, token: &str) -> bool {
            self.inner.is_foreground(token)
        }
        fn gesture_barrier(&mut self) -> bool {
            self.inner.gesture_barrier()
        }
        fn read_dpi(&mut self, token: &str) -> Option<u32> {
            self.inner.read_dpi(token)
        }
        fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
            self.inner.write_frame(token, rect)
        }
        fn move_membership(
            &mut self,
            token: &str,
            out: &str,
            ws: &str,
            frame: Option<Rect>,
        ) -> Result<bool, ()> {
            self.inner.move_membership(token, out, ws, frame)
        }
        fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
            self.inner.switch_view(output, logical)
        }
        fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
            let result = self.inner.switch_view(output, logical);
            if result == Ok(true)
                && let Some(live) = self.inner.windows.get_mut("w2")
            {
                live.key.pid = live.key.pid.wrapping_add(1);
                live.key.creation = format!("{}-recycled", live.key.creation);
            }
            result
        }
        fn foreground_state(&mut self) -> ForegroundState {
            self.inner.foreground_state()
        }
        fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
            self.inner.focus_window(token)
        }
        fn read_cursor(&mut self) -> Option<(i32, i32)> {
            self.inner.read_cursor()
        }
        fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
            self.inner.move_cursor(x, y)
        }
        fn poll_external_invalidation(&mut self) -> bool {
            self.inner.poll_external_invalidation()
        }
    }
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    let mut probe = RecycleLeaving { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(!row.dispatched);
    assert_eq!(row.hold, Some("foreground-changed"));
    assert!(diff.effects.mark_effect_spent("fx-9"));
}

#[test]
fn remembered_focus_move_size_start_vetoes_with_empty_queue() {
    struct MoveSizeDuringSwitch {
        inner: FakeNative,
    }
    impl NativeOps for MoveSizeDuringSwitch {
        fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
            self.inner.read_window(token)
        }
        fn read_frame(&mut self, token: &str) -> Option<Rect> {
            self.inner.read_frame(token)
        }
        fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
            self.inner.read_homing(token)
        }
        fn read_view(&mut self, output: &str) -> Option<String> {
            self.inner.read_view(output)
        }
        fn is_foreground(&mut self, token: &str) -> bool {
            self.inner.is_foreground(token)
        }
        fn gesture_barrier(&mut self) -> bool {
            self.inner.gesture_barrier()
        }
        fn read_dpi(&mut self, token: &str) -> Option<u32> {
            self.inner.read_dpi(token)
        }
        fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
            self.inner.write_frame(token, rect)
        }
        fn move_membership(
            &mut self,
            token: &str,
            out: &str,
            ws: &str,
            frame: Option<Rect>,
        ) -> Result<bool, ()> {
            self.inner.move_membership(token, out, ws, frame)
        }
        fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
            self.inner.switch_view(output, logical)
        }
        fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
            let result = self.inner.switch_view(output, logical);
            if result == Ok(true) {
                self.inner.move_size.insert("w1".to_owned());
            }
            result
        }
        fn foreground_state(&mut self) -> ForegroundState {
            self.inner.foreground_state()
        }
        fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
            self.inner.focus_window(token)
        }
        fn read_cursor(&mut self) -> Option<(i32, i32)> {
            self.inner.read_cursor()
        }
        fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
            self.inner.move_cursor(x, y)
        }
        fn poll_external_invalidation(&mut self) -> bool {
            self.inner.poll_external_invalidation()
        }
    }
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    let mut probe = MoveSizeDuringSwitch { inner: native };
    let reply = view_return_reply(&rig);
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 1);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "refused");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("foreground-unproven"));
    assert!(!diff.effects.mark_effect_spent("fx-9"));
}

#[test]
fn write_plus_hold_reports_partial_truthfully() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w2").unwrap().hidden = true;
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w2", rect(0, 610, 700, 300)));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(stats.held, 1);
    assert!(stats.partial);
}

// --- diff tables ---

#[test]
fn prune_bounds_generation_scoped_tables() {
    let rig = rig();
    let ws_target = "ws2".to_owned();
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(captured.inventory_complete);
    let mut fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    fresh.views.insert(rig.out.clone(), rig.ws.clone());
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = diff_at(7);
    diff.effects
        .attempted_members
        .insert(("op-stale".to_owned(), "w-stale".to_owned()));
    diff.effects.mark_member_attempted("op-1", "w1");
    diff.effects.mark_effect_spent("fx-stale");
    diff.effects.mark_float_spent("op-stale-float");
    diff.effects.mark_float_spent("op-float-live");
    diff.effects.mark_view_spent("op-stale-view");
    native.throw_membership.insert("w1".to_owned());
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_target.as_str()),
        ],
    );
    reply.windows.push(member_row(&rig.out, &rig.ws, "w1"));
    let mut float_holder = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
    float_holder.frame_authority = Auth::Untouched;
    float_holder.frame_operation = Some("op-float-live".to_owned());
    float_holder.effective_frame = None;
    reply.windows.push(float_holder);
    reply.transfers.push(window_transfer(
        "op-1", "w1", &rig.out, &ws_target, false, false,
    ));
    reply
        .transfers
        .push(view_transfer("op-view-live", &rig.out, &rig.ws));
    let _ = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(diff.effects.is_member_attempted("op-1", "w1"));
    assert!(!diff.effects.is_member_attempted("op-stale", "w-stale"));
    assert!(diff.effects.mark_float_spent("op-stale-float"));
    assert!(!diff.effects.mark_float_spent("op-float-live"));
    assert!(diff.effects.mark_view_spent("op-stale-view"));
    assert!(diff.effects.mark_effect_spent("fx-stale"));
}

#[test]
fn generation_reset_clears_scoped_state() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    diff.effects.mark_member_attempted("op-1", "w1");
    assert!(diff.effects.is_member_attempted("op-1", "w1"));
    diff.effects.mark_effect_spent("fx-1");
    diff.gate
        .note_applied(&reply_shell(7, 5, &[(rig.out.as_str(), rig.ws.as_str())]));
    diff.reset_generation("gen-2");
    assert!(!diff.effects.is_member_attempted("op-1", "w1"));
    assert!(diff.effects.mark_effect_spent("fx-1"));
    assert_eq!(diff.gate.current_obs, 0);
    assert!(!diff.gate.has_accepted);
    let mut reply = reply_shell(0, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.core_generation = "gen-2".to_owned();
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(!stats.stopped_mid_apply);
}

// --- transfers, follow, effects, migration ---

#[test]
fn transfer_throw_holds_ambiguous_without_repeat() {
    let rig = rig();
    let ws_target = "ws2".to_owned();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.throw_membership.insert("w1".to_owned());
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_target.as_str()),
        ],
    );
    reply.windows.push(member_row(&rig.out, &rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-1", "w1", &rig.out, &ws_target, false, false,
    ));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(members.len(), 1);
    let attempt = &members[0];
    assert_eq!(attempt.operation_id, "op-1");
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.target_output.0, rig.out);
    assert_eq!(attempt.setter, FrameSetter::Threw);
    assert!(views.is_empty());
    assert!(diff.effects.is_member_attempted("op-1", "w1"));
    let before = native.membership_writes;
    let (stats2, members2, views2) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(native.membership_writes, before);
    assert_eq!(stats2.membership_writes, 0);
    assert!(members2.is_empty() && views2.is_empty());
}

#[test]
fn transfer_refused_proven_and_third_location_supersedes() {
    let rig = rig();
    let ws_target = "ws2".to_owned();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.fail_membership.insert("w1".to_owned());
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_target.as_str()),
        ],
    );
    reply.windows.push(member_row(&rig.out, &rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-2", "w1", &rig.out, &ws_target, false, false,
    ));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(stats.held, 1);
    assert_eq!(members.len(), 1);
    let attempt = &members[0];
    assert_eq!(attempt.operation_id, "op-2");
    assert_eq!(attempt.setter, FrameSetter::Refused);
    assert_eq!(
        attempt.pre,
        Some((OutputId(rig.out.clone()), rig.ws.clone()))
    );
    assert_eq!(attempt.pre, attempt.post);
    assert!(views.is_empty());

    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().output = "o9".to_owned();
    native.windows.get_mut("w1").unwrap().workspace = "ws9".to_owned();
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_target.as_str()),
        ],
    );
    reply.windows.push(member_row(&rig.out, &rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-3", "w1", &rig.out, &ws_target, false, false,
    ));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(stats.held, 1);
    // Third-location homing reports nothing: the core derives external
    // supersession from staged observation, never from a verdict here.
    assert!(members.is_empty() && views.is_empty());
}

#[test]
fn follow_success_applies_follow_failure_holds_without_view_claim() {
    let rig = rig();
    let ws_target = "ws2".to_owned();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().workspace = ws_target.clone();
    native.views.insert(rig.out.clone(), ws_target.clone());
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_target.as_str()),
        ],
    );
    reply.windows.push(member_row(&rig.out, &rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-4", "w1", &rig.out, &ws_target, true, true,
    ));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    // Pre-proven arrival reports nothing: the core settles from staged
    // observation, and the already-shown follow view needs no dispatch.
    assert!(members.is_empty() && views.is_empty());
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(ws_target.as_str())
    );
    assert_eq!(stats.view_writes, 0);

    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().workspace = ws_target.clone();
    native.views.insert(rig.out.clone(), rig.ws.clone());
    native
        .fail_view
        .insert(format!("{}/{}", rig.out, ws_target));
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_target.as_str()),
        ],
    );
    reply.windows.push(member_row(&rig.out, &rig.ws, "w1"));
    reply.transfers.push(window_transfer(
        "op-5", "w1", &rig.out, &ws_target, true, true,
    ));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    // Membership is pre-proven so no member row rides; the missed follow
    // view reports one raw attempt under the parent id (never a view
    // outcome on the window id, never a refused window).
    assert!(members.is_empty());
    assert_eq!(views.len(), 1);
    let follow = &views[0];
    assert_eq!(follow.operation_id, "op-5");
    assert_eq!(follow.setter, FrameSetter::Refused);
    assert_eq!(follow.post.as_deref(), Some(rig.ws.as_str()));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    // The spent follow-view phase never repeats.
    assert!(members.is_empty() && views.is_empty());
}

#[test]
fn effect_needs_exact_live_view_and_spends_once() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().workspace = rig.ws.clone();
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.effects.push(OneShotEffect {
        id: "fx-1".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 1,
    });
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));

    native.views.insert(rig.out.clone(), rig.ws.clone());
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 1);
    assert!(native.is_foreground("w1"));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);

    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().workspace = rig.ws.clone();
    native.views.insert(rig.out.clone(), rig.ws.clone());
    diff.gate.current_obs = 9;
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));
    assert!(stats.stopped_mid_apply);
}

#[test]
fn focus_dispatches_after_same_pass_view_reveal() {
    // View-return race: the observation preimage still hides the target
    // (the view switch has not been observed yet), but section 3 reveals
    // entering members synchronously, so the live re-read is visible
    // with the exact view shown. The effect must dispatch in the same
    // pass; a still-hidden live target must hold without dispatch.
    let rig = rig();
    let (mut diff, mut fresh, mut native) = converted(&rig);
    if let Some(base) = fresh.baseline.get_mut("w1") {
        base.hidden = true;
        base.frame = None;
        base.observed_frame = None;
    }
    if let Some(live) = native.windows.get_mut("w1") {
        live.hidden = false;
        live.iconic = false;
        live.frame = Some(rect(0, 0, 800, 600));
        live.observed_frame = Some(rect(0, 0, 800, 600));
    }
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.effects.push(OneShotEffect {
        id: "fx-9".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 1,
    });
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 1);
    assert!(native.is_foreground("w1"));
    assert!(!stats.stopped_mid_apply);

    // Still-hidden live target holds without dispatch.
    let (mut diff, mut fresh, mut native) = converted(&rig);
    if let Some(base) = fresh.baseline.get_mut("w1") {
        base.hidden = true;
        base.frame = None;
        base.observed_frame = None;
    }
    if let Some(live) = native.windows.get_mut("w1") {
        live.hidden = true;
        live.frame = None;
        live.observed_frame = None;
    }
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.effects.push(OneShotEffect {
        id: "fx-9".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 1,
    });
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));
    assert!(stats.held >= 1);
}

#[test]
fn same_pass_reveal_recycled_key_stops_focus_without_trusting_new_baseline() {
    // Identity-gate regression: the observation preimage hides the target
    // while the live reveal reads visible with the SAME HWND but a
    // recycled lifetime (changed PID/creation) and a matching/spoofed old
    // tag. The pre-setter identity check must keep the original pre-pass
    // baseline: trusting the fresh reveal as the baseline would compare
    // the recycled read against itself and dispatch focus. The pass must
    // stop with no dispatch and the effect unspent.
    let rig = rig();
    let (mut diff, mut fresh, mut native) = converted(&rig);
    if let Some(base) = fresh.baseline.get_mut("w1") {
        base.hidden = true;
        base.frame = None;
        base.observed_frame = None;
    }
    if let Some(live) = native.windows.get_mut("w1") {
        live.hidden = false;
        live.iconic = false;
        live.frame = Some(rect(0, 0, 800, 600));
        live.observed_frame = Some(rect(0, 0, 800, 600));
        // Recycled HWND: same handle, new lifetime, spoofed old tag.
        live.key = win_key(1001, 9999, "proc-recycled");
        live.live_tag = Some("member-tag-1".to_owned());
    }
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.effects.push(OneShotEffect {
        id: "fx-9".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 1,
    });
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));
    assert!(stats.stopped_mid_apply);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert_eq!(row.id, "fx-9");
    assert!(!row.dispatched);
    assert_eq!(row.setter, "skipped");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("pre-setter-stop"));
    assert!(diff.effects.mark_effect_spent("fx-9"));
}

// --- stage 4b cursor warp: focus-correlated mechanical application ---
//
// Every warp test below carries the focus effect the point rides with:
// the adapter warps only while that exact target holds the observed
// foreground. An effect-less point never moves the cursor.

fn focus_effect(id: &str, target: &str) -> OneShotEffect {
    OneShotEffect {
        id: id.to_owned(),
        kind: EffectKind::Focus,
        target: WindowId(target.to_owned()),
        issued_revision: 1,
    }
}

/// Rig with the exact live view shown and the cursor readable: the
/// producer shape every warp pass needs before it may move anything.
fn warp_rig(rig: &Rig) -> (HostDiff, FreshHost, FakeNative) {
    let (diff, fresh, mut native) = converted(rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    native.cursor = Some((10, 10));
    (diff, fresh, native)
}

fn warp_reply(rig: &Rig, effects: Vec<OneShotEffect>, point: (i32, i32)) -> WorldReply {
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.effects = effects;
    reply.cursor_warp = Some(point);
    reply
}

#[test]
fn warp_applies_once_per_observation() {
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    let reply = warp_reply(&rig, vec![focus_effect("fx-1", "w1")], (900, 300));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 1);
    assert!(native.is_foreground("w1"));
    assert_eq!(stats.cursor_warps, 1);
    assert_eq!(native.warp_calls, vec![(900, 300)]);
    assert_eq!(native.cursor, Some((900, 300)));
    // Duplicate delivery of the same observation never moves twice.
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.cursor_warps, 0);
    assert_eq!(native.warp_calls, vec![(900, 300)]);
    assert!(!stats.stopped_mid_apply);
}

#[test]
fn warp_skips_write_when_already_there() {
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    native.cursor = Some((900, 300));
    let reply = warp_reply(&rig, vec![focus_effect("fx-1", "w1")], (900, 300));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 1);
    assert_eq!(stats.cursor_warps, 0);
    assert!(native.warp_calls.is_empty());
    assert!(!stats.stopped_mid_apply);
}

#[test]
fn warp_needs_its_focus_effect() {
    // A point with no focus effect is stale coordinates, never an order:
    // consumed without motion so a duplicate never acts on it either.
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.cursor_warp = Some((900, 300));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.cursor_warps, 0);
    assert!(native.warp_calls.is_empty());
    assert_eq!(native.cursor, Some((10, 10)));
    assert!(!stats.stopped_mid_apply);
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(native.warp_calls.is_empty());
    assert_eq!(stats.cursor_warps, 0);
}

#[test]
fn warp_vetoed_when_focus_view_not_shown() {
    // The focus holds (live view not shown), so the warp must not ride
    // ahead of it: held focus means no cursor motion.
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.cursor = Some((10, 10));
    let reply = warp_reply(&rig, vec![focus_effect("fx-1", "w1")], (900, 300));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));
    assert_eq!(stats.cursor_warps, 0);
    assert!(native.warp_calls.is_empty());
    assert!(stats.held >= 1);
}

#[test]
fn warp_vetoed_when_last_focus_failed() {
    // The focus setter refuses, so the point dies with it: no warp on a
    // failed focus, and the observation still consumes.
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    native.fail_focus.insert("w1".to_owned());
    let reply = warp_reply(&rig, vec![focus_effect("fx-1", "w1")], (900, 300));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));
    assert_eq!(stats.cursor_warps, 0);
    assert!(native.warp_calls.is_empty());
    assert!(stats.held >= 1);
    assert_eq!(native.cursor, Some((10, 10)));
}

#[test]
fn warp_follows_last_focus_effect_not_first() {
    // Two focus effects apply in order; the warp correlates with the last
    // one only. When the last holds the foreground the warp proceeds even
    // though the first targeted elsewhere.
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    let reply = warp_reply(
        &rig,
        vec![focus_effect("fx-1", "w2"), focus_effect("fx-2", "w1")],
        (900, 300),
    );
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 2);
    assert!(native.is_foreground("w1"));
    assert_eq!(stats.cursor_warps, 1);
    assert_eq!(native.warp_calls, vec![(900, 300)]);
}

#[test]
fn warp_vetoed_when_last_focus_drifted() {
    // Same order, but the last focus refuses: the earlier effect lands,
    // yet the warp must not follow it. The point correlates with the
    // last effect alone, never the last cross-output one.
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    native.fail_focus.insert("w1".to_owned());
    let reply = warp_reply(
        &rig,
        vec![focus_effect("fx-1", "w2"), focus_effect("fx-2", "w1")],
        (900, 300),
    );
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 1);
    assert!(native.is_foreground("w2"));
    assert!(!native.is_foreground("w1"));
    assert_eq!(stats.cursor_warps, 0);
    assert!(native.warp_calls.is_empty());
    assert!(stats.held >= 1);
}

#[test]
fn warp_refusal_holds_without_retry() {
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    native.fail_warp = true;
    let reply = warp_reply(&rig, vec![focus_effect("fx-1", "w1")], (900, 300));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    // Explicit refusal: honestly held, no cursor motion, and the
    // observation is consumed so a duplicate delivery never retries a
    // stale point.
    assert_eq!(stats.cursor_warps, 0);
    assert_eq!(stats.held, 1);
    assert!(native.warp_calls.is_empty());
    assert_eq!(native.cursor, Some((10, 10)));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(native.warp_calls.is_empty());
    assert_eq!(stats.held, 0);
    assert!(!stats.stopped_mid_apply);
}

#[test]
fn warp_throw_stops_the_pass() {
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    native.throw_warp = true;
    let reply = warp_reply(&rig, vec![focus_effect("fx-1", "w1")], (900, 300));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.cursor_warps, 0);
    assert!(stats.stopped_mid_apply);
}

#[test]
fn ffc_effect_focuses_hover_target_like_explicit_focus() {
    // The core's `ffc-<window>` hover id rides the standard one-shot
    // focus path: exact live view proves, the spent fence dedupes.
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.effects.push(OneShotEffect {
        id: "ffc-w1".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("w1".to_owned()),
        issued_revision: 1,
    });
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 1);
    assert!(native.is_foreground("w1"));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
}

// --- stage 4b cursor hit: native hit plus exact managed binding ---

/// Scripted owner step for hit fixtures: child 5001 belongs to frame
/// 1001, owned dialog 7001 belongs to owner 1002, everything else is
/// top-level (0) like the production parent/owner fallback on unknown
/// handles.
fn hit_step(hwnd: u64) -> u64 {
    match hwnd {
        5001 => 1001,
        7001 => 1002,
        _ => 0,
    }
}

#[test]
fn cursor_hit_resolves_exact_managed_binding() {
    let rig = rig();
    let inputs = rig.state.inputs();
    let bound = |hwnd: u64| {
        resolve_cursor_hit(
            hwnd,
            &rig.observed,
            &rig.retained,
            inputs.member_tokens,
            &hit_step,
        )
    };
    let name = |hit: Option<WindowId>| hit.map(|id| id.0);
    assert_eq!(name(bound(1001)), Some("w1".to_owned()));
    assert_eq!(name(bound(1002)), Some("w2".to_owned()));
    // Child and owned-dialog hits walk to the managed frame/owner.
    assert_eq!(name(bound(5001)), Some("w1".to_owned()));
    assert_eq!(name(bound(7001)), Some("w2".to_owned()));
    // Foreign pixels, zero, and unbounded chains never bind.
    assert_eq!(bound(9999), None);
    assert_eq!(bound(0), None);
}

#[test]
fn cursor_hit_rejects_recycled_and_unbound_identities() {
    let rig = rig();
    let inputs = rig.state.inputs();
    // Recycled HWND (same number, new lifetime): the stored token no
    // longer matches, so the hit binds to nothing.
    let mut recycled = rig.observed.clone();
    recycled[0].token = "w1-recycled".to_owned();
    assert_eq!(
        resolve_cursor_hit(
            1001,
            &recycled,
            &rig.retained,
            inputs.member_tokens,
            &hit_step
        ),
        None
    );
    // A never-member observed window is not a managed lifetime either.
    let foreign_key = win_key(9999, 2999, "proc-9");
    let mut foreign = rig.observed.clone();
    foreign.push(observed_window(
        &foreign_key,
        "w9",
        rect(0, 0, 100, 100),
        plain_facts(),
    ));
    assert_eq!(
        resolve_cursor_hit(
            9999,
            &foreign,
            &rig.retained,
            inputs.member_tokens,
            &hit_step
        ),
        None
    );
}

#[test]
fn capture_carries_cursor_hit_with_its_cursor() {
    let rig = rig();
    // Native hit on a managed frame resolves to the bound lifetime and
    // rides the sequencer envelope unchanged.
    let mut inputs = rig.state.inputs();
    inputs.cursor = Some((10, 10));
    inputs.hit_hwnd = Some(1001);
    let captured = capture_world(
        &inputs,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert_eq!(
        captured.cursor_hit.as_ref().map(|id| id.0.as_str()),
        Some("w1")
    );
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    let report = seq.assemble(&captured, 7, None, None, 0);
    assert_eq!(
        report.cursor_hit.as_ref().map(|id| id.0.as_str()),
        Some("w1")
    );
    // A hit without its cursor never travels (game lane, unreadable).
    let mut inputs = rig.state.inputs();
    inputs.cursor = None;
    inputs.hit_hwnd = Some(1001);
    let captured = capture_world(
        &inputs,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert_eq!(captured.cursor_hit, None);
    // A foreign hit resolves to nothing even with a cursor.
    let mut inputs = rig.state.inputs();
    inputs.cursor = Some((10, 10));
    inputs.hit_hwnd = Some(9999);
    let captured = capture_world(
        &inputs,
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert_eq!(captured.cursor_hit, None);
}

#[test]
fn migration_partial_holds_then_completes_with_refill() {
    let rig = rig();
    let ws_target = "ws2".to_owned();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.throw_membership.insert("w2".to_owned());
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mig = "op-mig";
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_target.as_str()),
        ],
    );
    for token in ["w1", "w2"] {
        reply.windows.push(WindowDesire {
            lifetime: WindowId(token.to_owned()),
            output: OutputId(rig.out.clone()),
            workspace: ws_target.clone(),
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
    reply.transfers.push(TransferTarget {
        operation_id: mig.to_owned(),
        kind: TransferKind::Migration,
        target_output: OutputId(rig.out.clone()),
        target_workspace: ws_target.clone(),
        follow: false,
        held: false,
    });
    reply.transfers.push(TransferTarget {
        operation_id: format!("{mig}-refill"),
        kind: TransferKind::View,
        target_output: OutputId(rig.out.clone()),
        target_workspace: rig.ws.clone(),
        follow: false,
        held: false,
    });
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 1);
    assert_eq!(stats.view_writes, 0);
    // Both members report raw attempts (never verdicts): w1 proves arrival
    // while the throwing member holds ambiguous. The target view waits for
    // full arrival.
    assert_eq!(members.len(), 2);
    let w1 = members
        .iter()
        .find(|a| a.window.0 == "w1")
        .expect("w1 attempt");
    assert_eq!(w1.setter, FrameSetter::Accepted);
    assert_eq!(
        w1.post,
        Some((OutputId(rig.out.clone()), ws_target.clone()))
    );
    let w2 = members
        .iter()
        .find(|a| a.window.0 == "w2")
        .expect("w2 attempt");
    assert_eq!(w2.operation_id, mig);
    assert_eq!(w2.setter, FrameSetter::Threw);
    assert!(views.is_empty());

    native.throw_membership.clear();
    native.windows.get_mut("w2").unwrap().workspace = ws_target.clone();
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(stats.view_writes, 2);
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(rig.ws.as_str())
    );
    // Both views report raw attempts: the migration target and the
    // paired refill. No verdict rides either row.
    assert!(members.is_empty());
    assert_eq!(views.len(), 2);
    assert!(views.iter().any(|v| v.operation_id == mig));
    assert!(
        views
            .iter()
            .any(|v| v.operation_id == format!("{mig}-refill"))
    );
}

#[test]
fn migration_held_refill_never_actuates() {
    let rig = rig();
    let ws_target = "ws2".to_owned();
    let (mut diff, fresh, mut native) = converted(&rig);
    for token in ["w1", "w2"] {
        native.windows.get_mut(token).unwrap().workspace = ws_target.clone();
    }
    native
        .views
        .insert(rig.out.clone(), "ws-elsewhere".to_owned());
    let mig = "op-mig-held-refill";
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_target.as_str()),
        ],
    );
    for token in ["w1", "w2"] {
        reply.windows.push(WindowDesire {
            lifetime: WindowId(token.to_owned()),
            output: OutputId(rig.out.clone()),
            workspace: ws_target.clone(),
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
    reply.transfers.push(TransferTarget {
        operation_id: mig.to_owned(),
        kind: TransferKind::Migration,
        target_output: OutputId(rig.out.clone()),
        target_workspace: ws_target.clone(),
        follow: false,
        held: false,
    });
    reply.transfers.push(TransferTarget {
        operation_id: format!("{mig}-refill"),
        kind: TransferKind::View,
        target_output: OutputId(rig.out.clone()),
        target_workspace: rig.ws.clone(),
        follow: false,
        held: true,
    });
    let (_, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(ws_target.as_str())
    );
}

#[test]
fn held_scope_blocks_geometry() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[]);
    reply.scopes.push(ScopeStatus {
        scope: format!("{}/{}", rig.out, rig.ws),
        authority: Auth::Hold(CoreHold::GestureBarrier),
    });
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
}

// --- float placements: raw attempts only, never verdicts ---

#[test]
fn float_token_places_once_fences_and_reports_raw_attempt() {
    use tiler_core::world_report::FrameAttempt;
    let rig = rig();
    // Baseline live frame for the prewrite/readback oracles below.
    let baseline = rect(0, 0, 800, 600);
    // A successful placement writes once and reports the mechanical facts
    // with exact operation correlation: no outcome verdict is emitted, and
    // the token plus core observation decide.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().floating = true;
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Untouched;
    row.frame_operation = Some("op-9".to_owned());
    reply.windows.push(row);
    let (stats, frames, members, views) =
        apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
    assert_eq!(stats.geometry_writes, 1);
    assert!(members.is_empty() && views.is_empty());
    assert_eq!(frames.len(), 1, "one raw attempt: {frames:?}");
    let attempt: &FrameAttempt = &frames[0];
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.target, rect(10, 10, 400, 300));
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.operation_id.as_deref(), Some("op-9"));
    assert_eq!(attempt.prewrite, Some(baseline));
    assert_eq!(attempt.readback, Some(rect(10, 10, 400, 300)));
    assert_eq!(attempt.attempt_ms, 1000);
    // Second dispatch of the same reply never replays: strictly
    // at-most-once transport while the core holds the retained target.
    let writes = native.frame_writes;
    let (stats, second_frames, second_members, second_views) =
        apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
    assert_eq!(native.frame_writes, writes);
    assert_eq!(stats.geometry_writes, 0);
    assert!(second_members.is_empty() && second_views.is_empty());
    assert!(second_frames.is_empty());

    // Live sticky with an explicit retained placement actuates exactly
    // like a float (restored half-snap lane): the `frame_operation` token
    // is the only authority. Fit-excluded still never takes float writes
    // and emits no attempt at all: the gate holds before any setter.
    {
        let (mut diff, fresh, mut native) = converted(&rig);
        native.windows.get_mut("w1").unwrap().floating = true;
        native.windows.get_mut("w1").unwrap().sticky = true;
        let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
        let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
        row.frame_authority = Auth::Untouched;
        row.frame_operation = Some("op-float-sticky".to_owned());
        reply.windows.push(row);
        let (stats, frames, members, views) =
            apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
        assert_eq!(stats.geometry_writes, 1, "sticky placement actuates");
        assert!(members.is_empty() && views.is_empty());
        assert_eq!(frames.len(), 1, "one raw attempt: {frames:?}");
        assert_eq!(frames[0].operation_id.as_deref(), Some("op-float-sticky"));
        assert_eq!(frames[0].setter, FrameSetter::Accepted);
    }
    {
        let (mut diff, fresh, mut native) = converted(&rig);
        native.windows.get_mut("w1").unwrap().floating = true;
        native.windows.get_mut("w1").unwrap().fit_excluded = true;
        let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
        let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
        row.frame_authority = Auth::Untouched;
        row.frame_operation = Some("op-float-excluded".to_owned());
        reply.windows.push(row);
        let (stats, frames, members, views) =
            apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
        assert_eq!(stats.geometry_writes, 0, "live excluded took a float write");
        assert!(stats.held >= 1);
        assert!(members.is_empty() && views.is_empty());
        assert!(frames.is_empty(), "gated lane emits no attempt");
    }

    // Clamp reports accepted-with-mismatch plus the readable preimage and
    // never repeats the setter; refusal, throw, and unreadable proof ride
    // the same raw lane with no verdict.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().floating = true;
    native.clamp_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Untouched;
    row.frame_operation = Some("op-float-clamp".to_owned());
    reply.windows.push(row);
    let (stats, first_frames, _, _) = apply_frames(&mut diff, &reply, &fresh, &mut native, 2000);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(first_frames.len(), 1);
    assert_eq!(
        first_frames[0].operation_id.as_deref(),
        Some("op-float-clamp")
    );
    assert_eq!(first_frames[0].setter, FrameSetter::Accepted);
    assert_eq!(first_frames[0].prewrite, Some(baseline));
    assert_eq!(first_frames[0].readback, Some(baseline));
    let writes = native.frame_writes;
    let (stats, second_frames, second_members, second_views) =
        apply_frames(&mut diff, &reply, &fresh, &mut native, 2000);
    assert_eq!(native.frame_writes, writes);
    assert_eq!(stats.geometry_writes, 0);
    assert!(second_members.is_empty() && second_views.is_empty());
    assert!(second_frames.is_empty());

    // Throw reports the ambiguous setter with the preimage and readback.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().floating = true;
    native.throw_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Untouched;
    row.frame_operation = Some("op-float-throw".to_owned());
    reply.windows.push(row);
    let (stats, first_frames, _, _) = apply_frames(&mut diff, &reply, &fresh, &mut native, 3000);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(first_frames.len(), 1);
    assert_eq!(
        first_frames[0].operation_id.as_deref(),
        Some("op-float-throw")
    );
    assert_eq!(first_frames[0].setter, FrameSetter::Threw);
    assert_eq!(first_frames[0].prewrite, Some(baseline));
    assert_eq!(first_frames[0].readback, Some(baseline));
    let writes = native.frame_writes;
    let (_, second_frames, second_members, second_views) =
        apply_frames(&mut diff, &reply, &fresh, &mut native, 3000);
    assert_eq!(native.frame_writes, writes);
    assert!(second_members.is_empty() && second_views.is_empty());
    assert!(second_frames.is_empty());
}

// --- view truthfulness ---

struct ViewProbe {
    inner: FakeNative,
    switches: usize,
    result: Option<Result<bool, ()>>,
    read_override: Option<Option<String>>,
}

impl NativeOps for ViewProbe {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        if let Some(over) = self.read_override.clone() {
            return over;
        }
        self.inner.read_view(output)
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.inner.gesture_barrier()
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.inner.read_dpi(token)
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        self.inner.write_frame(token, rect)
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.switches += 1;
        if let Some(result) = self.result {
            if result.is_ok() {
                let _ = self.inner.switch_view(output, logical);
            }
            return result;
        }
        self.inner.switch_view(output, logical)
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.inner.focus_window(token)
    }
    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.inner.read_cursor()
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        self.inner.move_cursor(x, y)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
    }
    fn foreground_state(&mut self) -> ForegroundState {
        self.inner.foreground_state()
    }
}

#[test]
fn view_ok_true_with_unreadable_proof_is_not_refused_proven() {
    let rig = rig();
    let (mut diff, fresh, native) = converted(&rig);
    let mut probe = ViewProbe {
        inner: native,
        switches: 0,
        result: Some(Ok(true)),
        read_override: Some(None),
    };
    probe.inner.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), "ws-target"),
        ],
    );
    reply
        .transfers
        .push(view_transfer("op-view-async", &rig.out, "ws-target"));
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 0);
    // Accepted setter with unreadable proof reports raw facts (never a
    // verdict): the setter boolean is never proof, and an unreadable
    // post can never prove refusal. The core holds ambiguous.
    assert!(members.is_empty());
    assert_eq!(views.len(), 1);
    let got = &views[0];
    assert_eq!(got.operation_id, "op-view-async");
    assert_eq!(got.setter, FrameSetter::Accepted);
    assert_eq!(got.post, None);
}

#[test]
fn view_ok_true_with_third_location_is_not_refused_proven() {
    let rig = rig();
    let (mut diff, fresh, native) = converted(&rig);
    let mut probe = ViewProbe {
        inner: native,
        switches: 0,
        result: Some(Ok(true)),
        read_override: Some(Some("ws-other".to_owned())),
    };
    probe.inner.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), "ws-target"),
        ],
    );
    reply
        .transfers
        .push(view_transfer("op-view-third", &rig.out, "ws-target"));
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.view_writes, 0);
    // Accepted setter with third-location proof reports raw facts (never
    // a refusal verdict): the core derives supersession from the readable
    // post that is neither the target nor the baseline.
    assert!(members.is_empty());
    assert_eq!(views.len(), 1);
    let got = &views[0];
    assert_eq!(got.setter, FrameSetter::Accepted);
    assert_eq!(got.post.as_deref(), Some("ws-other"));
    assert_ne!(got.post.as_deref(), Some("ws-target"));
}

#[test]
fn stale_reply_must_not_retire_ambiguous_view_fence() {
    let rig = rig();
    let (mut diff, mut fresh, native) = converted(&rig);
    fresh.views.insert(rig.out.clone(), rig.ws.clone());
    let mut probe = ViewProbe {
        inner: native,
        switches: 0,
        result: None,
        read_override: None,
    };
    probe.inner.views.insert(rig.out.clone(), rig.ws.clone());
    probe
        .inner
        .throw_view
        .insert(format!("{}/ws-target", rig.out));
    let op = "op-view-stale-prune";
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), "ws-target"),
        ],
    );
    reply
        .transfers
        .push(view_transfer(op, &rig.out, "ws-target"));
    let mut members = Vec::new();
    let mut views = Vec::new();
    let first_stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert_eq!(first_stats.held, 1);
    // The throwing switch reports one raw attempt; the spent phase fence
    // holds without repeating.
    assert!(members.is_empty());
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].operation_id, op);
    assert_eq!(views[0].setter, FrameSetter::Threw);
    assert_eq!(probe.switches, 1);
    assert!(diff.effects.is_view_spent(op));

    let mut stale = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), "ws-target"),
        ],
    );
    stale.core_generation = "gen-stale".to_owned();
    let stale_stats = apply_world_reply(
        &mut diff,
        &stale,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert!(stale_stats.stopped_mid_apply);
    assert_eq!(probe.switches, 1);
    assert!(diff.effects.is_view_spent(op));

    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut probe,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.held, 1);
    assert_eq!(probe.switches, 1);
    assert!(members.is_empty() && views.is_empty());
}

// --- core-owned frame loop: attempt -> report -> desire -> obeyed ---

pub(super) fn seq_deliver(seq: &mut WorldSequencer, report: &WorldReport) -> WorldReply {
    match seq.deliver(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

pub(super) fn seq_ack(seq: &mut WorldSequencer, reply: &WorldReply) {
    match seq.ack(reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
}

pub(super) fn dispatch_obs(
    seq: &mut WorldSequencer,
    rig: &Rig,
    foreground: u64,
    ms: u64,
) -> WorldReport {
    seq.try_dispatch(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        foreground,
        ms,
        None,
    )
    .expect("dispatch")
}

#[test]
fn production_sequencer_reports_opt_out_of_automatic_birth_clear() {
    // Non-opt-in production route pin: the real capture/sequencer path
    // reports the capability off, so the core never mints an autonomous
    // birth clear beside the retained preamble-owned birth route.
    let rig = rig();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let report = dispatch_obs(&mut seq, &rig, 1001, 7);
    assert!(
        !report.birth_clear,
        "Windows production stays opted out of automatic birth clears"
    );
}

/// Model truthful observation: the next capture reads what the native
/// desktop actually holds after the last apply pass.
pub(super) fn sync_observed_from_native(rig: &mut Rig, native: &FakeNative) {
    for window in &mut rig.observed {
        if let Some(entry) = native.windows.get(&window.token)
            && let Some(frame) = entry.frame
        {
            window.visible = frame;
            window.outer = Rect {
                x: frame.x - 8,
                y: frame.y - 8,
                w: frame.w + 16,
                h: frame.h + 16,
            };
            window.insets = crate::tiling::FrameInsets::measure(window.outer, frame);
        }
    }
}

pub(super) fn tiled_desire(reply: &WorldReply, window: &str) -> (Rect, Auth) {
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("no row for {window}"));
    (
        row.effective_frame.expect("tiled desire"),
        row.frame_authority,
    )
}

/// Full clamp loop through the real core: a lying setter (accepted write,
/// mismatched readback) reports raw facts; the next desire holds the pair
/// and the seam obeys without another setter (never desired-as-proof); an
/// external observation change rearms and the write resumes.
#[test]
fn frame_loop_clamp_suppresses_and_rearms_end_to_end() {
    let mut rig = rig();
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    let report = dispatch_obs(&mut seq, &rig, 1001, 1000);
    let reply = seq_deliver(&mut seq, &report);
    seq_ack(&mut seq, &reply);
    let (desired, first_auth) = tiled_desire(&reply, "w1");
    assert_eq!(first_auth, Auth::Controlled);

    // Tick 1 apply under the lying setter.
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native.clamp_frame.insert("w1".to_owned());
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    let mut attempts = Vec::new();
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut attempts,
        &mut members,
        &mut views,
        &mut Vec::new(),
        1000,
    );
    assert_eq!(stats.geometry_writes, 2);
    assert!(members.is_empty() && views.is_empty());
    assert_eq!(attempts.len(), 2);
    let lying = attempts
        .iter()
        .find(|attempt| attempt.window.0 == "w1")
        .expect("lying attempt rides");
    assert_eq!(lying.target, desired);
    assert_eq!(lying.setter, FrameSetter::Accepted);
    assert!(lying.readback.is_some_and(|readback| readback != desired));
    for attempt in &attempts {
        seq.note_frame_attempt(attempt.clone());
    }
    assert_eq!(seq.pending_frames_len(), 2);
    diff.gate.note_applied(&reply);
    sync_observed_from_native(&mut rig, &native);

    // Tick 2: the attempt rides the report; the core holds the pair.
    seq.invalidate();
    let report = dispatch_obs(&mut seq, &rig, 1001, 2000);
    assert!(
        report.frames.iter().any(|attempt| attempt.window.0 == "w1"),
        "raw facts ride, got {:?}",
        report.frames
    );
    let reply2 = seq_deliver(&mut seq, &report);
    let (desired2, held_auth) = tiled_desire(&reply2, "w1");
    assert_eq!(desired2, desired);
    assert_eq!(held_auth, Auth::Hold(CoreHold::ConstraintClamp));
    seq_ack(&mut seq, &reply2);
    assert_eq!(seq.pending_frames_len(), 0);

    // Tick 2 apply obeys the hold: no setter, no new facts, still held.
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    diff.gate.current_obs = seq.current_obs();
    let writes = native.frame_writes;
    let mut attempts = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply2,
        &fresh,
        &mut native,
        &mut attempts,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        2000,
    );
    assert_eq!(native.frame_writes, writes);
    assert_eq!(stats.geometry_writes, 0);
    assert!(attempts.is_empty());
    diff.gate.note_applied(&reply2);

    // Tick 3: an external observation change rearms; the write resumes.
    let moved = Rect {
        x: desired.x + 5,
        y: desired.y,
        w: desired.w,
        h: desired.h,
    };
    native.windows.get_mut("w1").expect("live").frame = Some(moved);
    sync_observed_from_native(&mut rig, &native);
    seq.invalidate();
    let report = dispatch_obs(&mut seq, &rig, 1001, 3000);
    let reply3 = seq_deliver(&mut seq, &report);
    assert_eq!(tiled_desire(&reply3, "w1").1, Auth::Controlled);
    seq_ack(&mut seq, &reply3);
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    diff.gate.current_obs = seq.current_obs();
    let mut attempts = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply3,
        &fresh,
        &mut native,
        &mut attempts,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        3000,
    );
    assert_eq!(native.frame_writes, writes + 1);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(attempts.len(), 1);
}

/// Full transient loop: a refused setter holds inside backoff with no
/// renewed facts (lost-ack redelivery cannot extend it) and retries after.
#[test]
fn frame_loop_transient_backoff_and_retry_end_to_end() {
    let mut rig = rig();
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    let report = dispatch_obs(&mut seq, &rig, 1001, 1000);
    let reply = seq_deliver(&mut seq, &report);
    seq_ack(&mut seq, &reply);
    let (desired, _) = tiled_desire(&reply, "w1");

    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native.fail_frame.insert("w1".to_owned());
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
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
        1000,
    );
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(stats.held, 1);
    let refused = attempts
        .iter()
        .find(|attempt| attempt.window.0 == "w1")
        .expect("refusal rides");
    assert_eq!(refused.setter, FrameSetter::Refused);
    assert_eq!(refused.target, desired);
    for attempt in &attempts {
        seq.note_frame_attempt(attempt.clone());
    }
    diff.gate.note_applied(&reply);
    sync_observed_from_native(&mut rig, &native);

    // Inside the 500ms backoff the desire holds and the seam emits no new
    // facts (nothing to renew the window).
    seq.invalidate();
    let report = dispatch_obs(&mut seq, &rig, 1001, 1100);
    let reply2 = seq_deliver(&mut seq, &report);
    assert_eq!(
        tiled_desire(&reply2, "w1"),
        (desired, Auth::Hold(CoreHold::ConstraintClamp))
    );
    seq_ack(&mut seq, &reply2);
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    diff.gate.current_obs = seq.current_obs();
    let writes = native.frame_writes;
    let mut attempts = Vec::new();
    apply_world_reply(
        &mut diff,
        &reply2,
        &fresh,
        &mut native,
        &mut attempts,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1100,
    );
    assert_eq!(native.frame_writes, writes);
    assert!(attempts.is_empty());
    diff.gate.note_applied(&reply2);

    // Past the backoff the adapter retries the setter.
    seq.invalidate();
    let report = dispatch_obs(&mut seq, &rig, 1001, 2000);
    let reply3 = seq_deliver(&mut seq, &report);
    assert_eq!(tiled_desire(&reply3, "w1").1, Auth::Controlled);
    seq_ack(&mut seq, &reply3);
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    diff.gate.current_obs = seq.current_obs();
    let mut attempts = Vec::new();
    let tick3_stats = apply_world_reply(
        &mut diff,
        &reply3,
        &fresh,
        &mut native,
        &mut attempts,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        2000,
    );
    // The refused setter retries (a failed write never counts as a frame
    // write, so the new stamped attempt is the retry proof).
    assert_eq!(tick3_stats.held, 1);
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].attempt_ms, 2000);
    assert_eq!(attempts[0].setter, FrameSetter::Refused);
}

/// DPI invalidation loop: a write whose basis flips under the call never
/// becomes a stable clamp; the next desire stays controlled for retry.
#[test]
fn frame_loop_dpi_change_never_clamps() {
    let mut rig = rig();
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    let report = dispatch_obs(&mut seq, &rig, 1001, 1000);
    let reply = seq_deliver(&mut seq, &report);
    seq_ack(&mut seq, &reply);
    let (desired, _) = tiled_desire(&reply, "w1");

    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native.clamp_frame.insert("w1".to_owned());
    native.dpi.insert("w1".to_owned(), 96);
    native.dpi_flip.insert("w1".to_owned());
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    let mut attempts = Vec::new();
    apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut attempts,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    let flipped = attempts
        .iter()
        .find(|attempt| attempt.window.0 == "w1")
        .expect("flipped attempt rides");
    assert_eq!(flipped.dpi_before, Some(96));
    assert_eq!(flipped.dpi_after, Some(120));
    for attempt in &attempts {
        seq.note_frame_attempt(attempt.clone());
    }
    diff.gate.note_applied(&reply);
    sync_observed_from_native(&mut rig, &native);

    seq.invalidate();
    let report = dispatch_obs(&mut seq, &rig, 1001, 2000);
    let reply2 = seq_deliver(&mut seq, &report);
    assert_eq!(tiled_desire(&reply2, "w1"), (desired, Auth::Controlled));
}

// --- sequencer ordering, attempts, closes ---

fn send_rig() -> Rig {
    rig()
}

#[test]
fn send_roundtrip_actuates_attempts_and_converges_without_remove() {
    let mut rig = send_rig();
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    let report = seq
        .try_dispatch(
            &rig.state.inputs(),
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            0,
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
    // Logical authority: the send target is a core-minted workspace id
    // from the accepted reply (same output, not the source home), never
    // an adapter-invented name.
    let source_ws = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "w1")
        .map(|w| w.workspace.clone())
        .expect("w1 homed");
    let ws2 = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == rig.out && row.id != source_ws)
        .map(|row| row.id.clone())
        .expect("second workspace");
    let send = WorldCommand::Send {
        window: WindowId("w1".to_owned()),
        target_output: OutputId(rig.out.clone()),
        target_workspace: ws2.clone(),
        follow: false,
    };
    let report = seq
        .try_dispatch(
            &rig.state.inputs(),
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            0,
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

    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 1);
    // The dispatch reports one raw membership attempt (never a verdict):
    // accepted setter with post proof at the target.
    assert_eq!(members.len(), 1);
    let attempt = &members[0];
    assert_eq!(attempt.operation_id, op);
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.post, Some((OutputId(rig.out.clone()), ws2.clone())));
    assert!(views.is_empty());
    diff.gate.note_applied(&reply);
    let writes = native.membership_writes;
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(stats.stopped_mid_apply);
    assert_eq!(native.membership_writes, writes);
    // Model the native effect end-to-end: the next capture receipts the
    // arrival and the core can settle on evidence instead of holding
    // against a stale observation.
    rig.state
        .applied_homing
        .insert("w1".to_owned(), (rig.out.clone(), ws2.clone()));
    let before = fingerprint(&rig.state);
    seq.invalidate();
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(seq.is_dirty(&captured));
    for item in &members {
        seq.note_member_attempt(item.clone());
    }
    let report = seq
        .try_dispatch(
            &rig.state.inputs(),
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            0,
            None,
        )
        .expect("attempt dispatch");
    assert!(report.member_attempts.iter().any(|f| f.operation_id == op));
    let reply2 = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply2) {
        AckOutcome::Applied(receipt) => {
            assert!(
                receipt
                    .members_acked
                    .contains(&(op.clone(), "w1".to_owned()))
            );
        }
        other => panic!("expected applied, got {other:?}"),
    }
    diff.gate.current_obs = seq.current_obs();
    let (stats, members, views) = apply(&mut diff, &reply2, &fresh, &mut native);
    // The arrival receipt settled the transfer: w1 rides homed at the
    // target with no retained transfer, while w2 converges to its grown
    // post-departure slot with one frame write. Nothing outstanding means
    // no re-proof and no fabricated verdict.
    assert!(reply2.transfers.iter().all(|t| t.operation_id != op));
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(stats.view_writes, 0);
    assert_eq!(stats.geometry_writes, 1);
    assert!(!stats.stopped_mid_apply);
    assert!(
        members.is_empty() && views.is_empty(),
        "settled flight re-proves nothing"
    );
    assert!(!stats.partial);
    assert_eq!(fingerprint(&rig.state), before);
}

#[test]
fn rejection_correlates_to_exact_flight_and_fences_catchup() {
    let rig = rig();
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    // Rejection correlates to the exact flight: a stale id is a no-op.
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &rig.state.inputs(),
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            0,
            None,
        )
        .expect("flight");
    let req = report.request_id.clone();
    assert!(!seq.note_rejected_or_lost(GENERATION, "req-nope"));
    assert!(seq.has_flight());
    assert!(seq.note_rejected_or_lost(GENERATION, &req));
    assert!(!seq.has_flight());
    assert!(seq.in_catchup());
    // While catch-up is pending a command is refused without consumption.
    let before = seq.next_request_seq();
    let send = WorldCommand::Send {
        window: WindowId("w1".to_owned()),
        target_output: OutputId(rig.out.clone()),
        target_workspace: rig.ws.clone(),
        follow: false,
    };
    assert!(matches!(
        seq.try_dispatch(
            &rig.state.inputs(),
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            0,
            Some(send)
        ),
        Err(DispatchError::CatchupRequired)
    ));
    assert_eq!(seq.next_request_seq(), before);
}

#[test]
fn close_receipt_prunes_exact_pairs_only() {
    let mut rig = rig();
    let homing = PendingClosed {
        output: DEV1.to_owned(),
        workspace: rig.ws.clone(),
    };
    rig.state
        .pending_closed
        .insert("w9".to_owned(), homing.clone());
    rig.state.pending_closed.insert(
        "w-orphan".to_owned(),
        PendingClosed {
            output: "NOPE".to_owned(),
            workspace: "ws-x".to_owned(),
        },
    );
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert_eq!(
        seq.outstanding_closes(&captured).len(),
        2,
        "carried plus unhomed closes stay dirty"
    );
    assert!(seq.is_dirty(&captured));
    let report = seq
        .try_dispatch(
            &rig.state.inputs(),
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            0,
            None,
        )
        .expect("dispatch");
    assert!(report.closed.iter().any(|id| id.0 == "w9"));
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let receipt = match seq.ack(&reply) {
        AckOutcome::Applied(receipt) => receipt,
        other => panic!("expected applied, got {other:?}"),
    };
    assert_eq!(
        receipt.closed_acked,
        vec![("w9".to_owned(), homing.clone())]
    );
    let mut pending: BTreeMap<String, PendingClosed> = rig.state.pending_closed.clone();
    assert_eq!(
        seq.apply_close_receipt(&mut pending, &receipt),
        vec!["w9".to_owned()]
    );
    // A re-homed entry under the same token no longer matches the receipt.
    pending.insert(
        "w9".to_owned(),
        PendingClosed {
            output: DEV1.to_owned(),
            workspace: "ws-elsewhere".to_owned(),
        },
    );
    assert!(seq.apply_close_receipt(&mut pending, &receipt).is_empty());
    assert!(pending.contains_key("w9"));
    // A receipt held across a generation reset prunes nothing.
    assert!(seq.reset_generation("gen-2"));
    let mut pending: BTreeMap<String, PendingClosed> = rig.state.pending_closed.clone();
    assert!(seq.apply_close_receipt(&mut pending, &receipt).is_empty());
    assert!(pending.contains_key("w9"));
}

#[test]
fn consumed_command_reconciles_without_replay() {
    let rig = send_rig();
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    let report = seq
        .try_dispatch(
            &rig.state.inputs(),
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            0,
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
    // Logical authority: switch to a core-minted workspace id from the
    // accepted reply (same output, not the current view), never an
    // adapter-invented name.
    let current = reply
        .outputs
        .iter()
        .find(|o| o.output.0 == rig.out)
        .and_then(|o| o.logical_view.clone())
        .expect("current view");
    let ws2 = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == rig.out && row.id != current)
        .map(|row| row.id.clone())
        .expect("second workspace");
    let switch = WorldCommand::SwitchView {
        output: OutputId(rig.out.clone()),
        workspace: ws2.clone(),
    };
    let report = seq
        .try_dispatch(
            &rig.state.inputs(),
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            0,
            Some(switch.clone()),
        )
        .expect("view dispatch");
    assert_eq!(report.command_seq, Some(1));
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted view, got {other:?}"),
    };
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::View)),
        "core retains the view transfer"
    );
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    // The same consumed command under a fresh request reconciles the
    // observation without re-executing: no new transfer, no replay.
    seq.invalidate();
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let resent = seq.assemble(&captured, seq.next_request_seq(), Some(1), Some(switch), 0);
    match seq.deliver(&resent) {
        WorldOutcome::CommandConsumed(_) => {}
        other => panic!("expected consumed, got {other:?}"),
    }
}

// --- stage 3b2 production float path: command -> diff -> core, no verdicts ---
//
// A real `ToggleFloat` command through the production sequencer mints the
// retained placement token; the real diff writes through the native seam
// and reports the mechanical attempt with exact operation correlation plus
// the immediate pre-write frame; the sequencer carries it into the core,
// which holds the token on clamp ambiguity. No float outcome verdict is
// ever emitted, and the held placement never re-actuates.

#[test]
fn float_production_seam_reports_raw_attempt_and_core_holds_token() {
    use tiler_core::boundary::CoreCommand;
    let mut rig = rig();
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    let report = dispatch_obs(&mut seq, &rig, 1001, 1000);
    let reply = seq_deliver(&mut seq, &report);
    seq_ack(&mut seq, &reply);
    // Real command: explicit float placement for w1 under a clamping setter.
    let target = rect(10, 10, 400, 300);
    let baseline = rect(0, 0, 800, 600);
    seq.invalidate();
    let command = WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: "w1".to_owned(),
        float_rect: Some(target),
    });
    let report = seq
        .try_dispatch_with_command(
            &rig.state.inputs(),
            Some(&rig.observed),
            &rig.retained,
            &rig.areas,
            1001,
            2000,
            command,
        )
        .expect("float dispatch");
    assert_eq!(report.command_seq, Some(1));
    let reply = seq_deliver(&mut seq, &report);
    let token = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .and_then(|row| row.frame_operation.clone())
        .expect("core mints the placement token");
    assert_eq!(token, "op-1");
    seq_ack(&mut seq, &reply);
    // Real actuation under a throwing setter: the write may have executed,
    // so the seam reports the ambiguous attempt with the readable preimage
    // and readback but no verdict. The transient backoff lapses by the next
    // report, so the held token stays published while the core holds.
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native.throw_frame.insert("w1".to_owned());
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = seq.current_obs();
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
        3000,
    );
    // One write: the floated sibling reflows; the throwing placement writes
    // nothing itself.
    assert_eq!(stats.geometry_writes, 1);
    let attempt = attempts
        .iter()
        .find(|attempt| attempt.window.0 == "w1")
        .expect("raw float attempt rides: {attempts:?}");
    assert_eq!(attempt.target, target);
    assert_eq!(attempt.setter, FrameSetter::Threw);
    assert_eq!(attempt.operation_id.as_deref(), Some(token.as_str()));
    assert_eq!(attempt.prewrite, Some(baseline));
    assert_eq!(attempt.readback, Some(baseline));
    assert_eq!(attempt.attempt_ms, 3000);
    for attempt in &attempts {
        seq.note_frame_attempt(attempt.clone());
    }
    diff.gate.note_applied(&reply);
    sync_observed_from_native(&mut rig, &native);
    // The correlated attempt rides the next report; the core holds the token
    // with no verdict and stages no replay.
    seq.invalidate();
    let report2 = dispatch_obs(&mut seq, &rig, 1001, 4000);
    let carried = report2
        .frames
        .iter()
        .find(|attempt| attempt.window.0 == "w1")
        .expect("correlated attempt rides");
    assert_eq!(carried.target, target);
    assert_eq!(carried.operation_id.as_deref(), Some(token.as_str()));
    assert_eq!(carried.prewrite, Some(baseline));
    let reply2 = seq_deliver(&mut seq, &report2);
    let row = reply2
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("second row");
    assert_eq!(
        row.frame_operation.as_deref(),
        Some(token.as_str()),
        "ambiguous hold keeps the token"
    );
    assert_eq!(reply2.command_result, None, "held placement never replays");
    seq_ack(&mut seq, &reply2);
    // After the acknowledgement the held placement still rides the token
    // with no new attempt and no verdict.
    seq.invalidate();
    let report3 = dispatch_obs(&mut seq, &rig, 1001, 5000);
    assert!(
        report3.frames.is_empty(),
        "consumed attempt never redelivers: {:?}",
        report3.frames
    );
    let reply3 = seq_deliver(&mut seq, &report3);
    assert_eq!(
        reply3
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "w1")
            .and_then(|row| row.frame_operation.clone())
            .as_deref(),
        Some(token.as_str()),
        "hold survives the ack"
    );
    seq_ack(&mut seq, &reply3);
}

fn clear_row(out: &str, ws: &str, token: &str, op: &str) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(token.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: ws.to_owned(),
        allocation: None,
        effective_frame: None,
        frame_authority: Auth::Untouched,
        membership_authority: Auth::Untouched,
        floating: true,
        float_origin: None,
        frame_operation: None,
        clear_operation: Some(op.to_owned()),
    }
}

fn maximized_facts() -> WindowFacts {
    WindowFacts {
        maximized: true,
        ..plain_facts()
    }
}

/// Fresh maximized rig: w1 reads maximized live, so the clear lane sees a
/// maximized preimage under full identity and lifetime proof.
fn maximized_rig() -> (Rig, FreshHost) {
    let mut rig = rig();
    rig.observed[0].facts = maximized_facts();
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    assert!(captured.inventory_complete);
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    assert!(
        fresh
            .baseline
            .get("w1")
            .and_then(|base| base.facts.map(|facts| facts.maximized))
            .unwrap_or(false),
        "maximized preimage rides: {fresh:?}"
    );
    (rig, fresh)
}

#[test]
fn clear_lane_tiled_row_dispatches_once_from_published_desire() {
    // Tiled production seam: a tiled row carrying the published
    // `clear_operation` token (allocation + effective frame + token, as the
    // core now publishes for eligible tiled rows) dispatches the native
    // clear exactly once via the common lane and reports the correlated
    // attempt. No fake proof: the attempt comes from the desire itself.
    use tiler_core::geometry::Rect;
    let (rig, fresh) = maximized_rig();
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = diff_at(7);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let slot = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 500,
    };
    reply.windows.push(WindowDesire {
        lifetime: WindowId("w1".to_owned()),
        output: OutputId(rig.out.clone()),
        workspace: rig.ws.clone(),
        allocation: Some(slot),
        effective_frame: Some(slot),
        frame_authority: Auth::Controlled,
        membership_authority: Auth::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
        clear_operation: Some("op-clear-tiled-1".to_owned()),
    });
    let mut clear_out = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    assert_eq!(clear_out[0].operation_id, "op-clear-tiled-1");
    assert_eq!(clear_out[0].setter, FrameSetter::Accepted);
    assert_eq!(clear_out[0].pre_maximized, Some(true));
    assert_eq!(clear_out[0].post_maximized, Some(false));
    // Second apply never re-dispatches (spent phase).
    let mut clear_again = Vec::new();
    apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_again,
        1000,
    );
    assert!(clear_again.is_empty(), "{clear_again:?}");
}

#[test]
fn clear_lane_dispatches_once_and_reports_accepted_attempt() {
    // The published `clear_operation` desire dispatches the native clear
    // exactly once and reports the mechanical attempt (setter plus
    // pre/post maximized states); the live maximized bit flips so the
    // next re-read observes it.
    let (rig, fresh) = maximized_rig();
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = diff_at(7);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(clear_row(&rig.out, &rig.ws, "w1", "op-clear-1"));
    let mut clear_out = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    let attempt = &clear_out[0];
    assert_eq!(attempt.operation_id, "op-clear-1");
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.pre_maximized, Some(true));
    assert_eq!(attempt.post_maximized, Some(false));
    assert_eq!(attempt.attempt_ms, 1000);
    assert!(
        !native
            .windows
            .get("w1")
            .and_then(|window| window.facts.map(|facts| facts.maximized))
            .unwrap_or(true),
        "live maximized flips"
    );
    // A second apply of the same reply never re-dispatches: the spent
    // clear phase holds silently.
    let mut clear_again = Vec::new();
    let restats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_again,
        1000,
    );
    assert!(clear_again.is_empty(), "{clear_again:?}");
    assert_eq!(native.clear_calls, vec!["w1".to_owned()]);
    assert_eq!(restats.clear_writes, 0);
}

#[test]
fn clear_lane_veto_reports_refused_and_spends() {
    // Fullscreen subjects veto: the lane reports exactly one correlated
    // refused attempt with the unchanged maximized preimage and spends the
    // phase, so a vetoed desire never sits silently unattempted and never
    // re-actuates.
    let (mut rig, _) = maximized_rig();
    rig.observed[0].facts.captionless_fullscreen = true;
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = diff_at(7);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(clear_row(&rig.out, &rig.ws, "w1", "op-clear-2"));
    let mut clear_out = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 0, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    let attempt = &clear_out[0];
    assert_eq!(attempt.setter, FrameSetter::Refused);
    assert_eq!(attempt.pre_maximized, Some(true));
    assert_eq!(attempt.post_maximized, Some(true));
    assert!(native.clear_calls.is_empty(), "vetoes never dispatch");
    // The spent veto never re-reports.
    let mut clear_again = Vec::new();
    apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_again,
        1000,
    );
    assert!(clear_again.is_empty(), "{clear_again:?}");
}

#[test]
fn clear_lane_throw_reports_ambiguous_without_retry() {
    // A throwing setter reports the ambiguous attempt with a readable
    // preimage; the spent phase never retries, so the core holds without
    // re-actuation until truthful later evidence reconciles.
    let (rig, fresh) = maximized_rig();
    let mut native = FakeNative::seeded(&fresh);
    native.throw_clear.insert("w1".to_owned());
    let mut diff = diff_at(7);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(clear_row(&rig.out, &rig.ws, "w1", "op-clear-3"));
    let mut clear_out = Vec::new();
    apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(clear_out.len(), 1);
    assert_eq!(clear_out[0].setter, FrameSetter::Threw);
    assert_eq!(clear_out[0].pre_maximized, Some(true));
    let mut clear_again = Vec::new();
    apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_again,
        1000,
    );
    assert!(clear_again.is_empty(), "{clear_again:?}");
    assert_eq!(native.clear_calls.len(), 1);
}

#[test]
fn clear_lane_already_unmaximized_skips_without_attempt() {
    // An already-unmaximized subject takes no write and reports no
    // attempt: the desire is stale (an external clear without a correlated
    // attempt abandons core-side), and nothing is spent.
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(clear_row(&rig.out, &rig.ws, "w1", "op-clear-4"));
    let mut clear_out = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert!(clear_out.is_empty(), "{clear_out:?}");
    assert_eq!(stats.clear_writes, 0);
    assert!(native.clear_calls.is_empty());
}

#[test]
fn clear_lane_sticky_actuates_like_float() {
    // Sticky subjects clear through the same lane: the explicit
    // `clear_operation` token is the only authority, while ordinary
    // unsolicited sticky frame writes stay prohibited elsewhere.
    let (mut rig, _) = maximized_rig();
    rig.state.sticky.insert(win_key(1001, 2001, "proc-1"), true);
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    assert!(
        native.windows.get("w1").is_some_and(|window| window.sticky),
        "sticky rides the baseline"
    );
    let mut diff = diff_at(7);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(clear_row(&rig.out, &rig.ws, "w1", "op-clear-5"));
    let mut clear_out = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    assert_eq!(clear_out[0].setter, FrameSetter::Accepted);
}

#[test]
fn clear_lane_held_scope_vetoes_with_refusal() {
    // A desire for an uncontrolled scope vetoes with one refused attempt
    // carrying the best-known maximized preimage, never a silent desire.
    let (rig, fresh) = maximized_rig();
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = diff_at(7);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(clear_row(&rig.out, "ws-elsewhere", "w1", "op-clear-6"));
    let mut clear_out = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 0, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    assert_eq!(clear_out[0].setter, FrameSetter::Refused);
    assert_eq!(clear_out[0].pre_maximized, Some(true));
    assert!(native.clear_calls.is_empty(), "vetoes never dispatch");
}

#[test]
fn capture_projects_retained_maximized_float_after_observed() {
    // Retained maximized floats (never eligible-observed) still ride
    // capture with their maximized flag and last-known slot, AFTER the
    // observed facts: float-focus candidates keep encounter order
    // (observed then retained), so axis ties break deterministically.
    let mut rig = rig();
    let slot = rect(300, 100, 200, 200);
    rig.state.member_rects.insert("w2".to_owned(), slot);
    rig.state.floated.insert(win_key(1002, 2002, "proc-2"));
    rig.observed.retain(|window| window.token == "w1");
    let retained = vec![RetainedRow {
        key: win_key(1002, 2002, "proc-2"),
        token: "w2".to_owned(),
        rect: None,
        maximized: true,
        fullscreen: false,
        facts: Some(maximized_facts()),
    }];
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &retained,
        &rig.areas,
        1001,
    );
    let order: Vec<&str> = captured
        .windows
        .iter()
        .map(|fact| fact.window.0.as_str())
        .collect();
    assert_eq!(
        order,
        vec!["w1", "w2"],
        "encounter order: observed then retained, got {order:?}"
    );
    let retained_fact = captured
        .windows
        .iter()
        .find(|fact| fact.window.0 == "w2")
        .expect("retained maximized float rides");
    assert!(retained_fact.maximized, "{retained_fact:?}");
    assert!(retained_fact.floating, "{retained_fact:?}");
    assert_eq!(retained_fact.rect, Some(slot), "{retained_fact:?}");
}

#[test]
fn attempted_members_survive_observation_gaps() {
    // At-most-once dispatch fences survive absence/readability gaps and ack
    // loss: pruning retains attempted membership on the live operation
    // alone, never erasing the fence when the fresh baseline misses the
    // lifetime. A possibly-executed write never repeats.
    let rig = rig();
    let captured = capture_world(
        &rig.state.inputs(),
        Some(&rig.observed),
        &rig.retained,
        &rig.areas,
        1001,
    );
    let mut fresh =
        FreshHost::from_sources(&rig.state.inputs(), &rig.observed, &rig.retained, &captured);
    let mut diff = diff_at(7);
    diff.effects.mark_member_attempted("op-1", "w1");
    assert!(diff.effects.is_member_attempted("op-1", "w1"));
    // Gap: fresh baseline misses the lifetime (unreadable/incomplete), but
    // the reply still carries the live operation.
    fresh.baseline.remove("w1");
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.transfers.push(window_transfer(
        "op-1",
        "w1",
        &rig.out,
        "ws-target",
        false,
        false,
    ));
    let mut native = FakeNative::seeded(&fresh);
    let _ = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(
        diff.effects.is_member_attempted("op-1", "w1"),
        "fence survives baseline gap"
    );
}

#[test]
fn hover_trace_changed_tracks_emission_cursor() {
    // Trace-only hover-change oracle: no row has been emitted yet, so the
    // first observation always counts as changed. Recording an emission
    // quiets repeats of the same triple; any hit/focused/FFC transition
    // re-arms. Pure comparison over already-captured values: no native
    // reads, no policy input.
    let mut tracker = super::world_diff::EffectTracker::default();
    assert!(tracker.hover_trace_changed(Some("w1"), "w1", true));
    tracker.note_hover_traced(Some("w1".to_owned()), "w1".to_owned(), true);
    assert!(!tracker.hover_trace_changed(Some("w1"), "w1", true));
    // Cursor moved off managed pixels: absent hit is a change, so the
    // no-effect observation still emits and stays distinguishable from
    // idle quiet.
    assert!(tracker.hover_trace_changed(None, "w1", true));
    tracker.note_hover_traced(None, "w1".to_owned(), true);
    assert!(!tracker.hover_trace_changed(None, "w1", true));
    // Focus moved without a hit change, and FFC toggled without either:
    // both re-arm the row.
    assert!(tracker.hover_trace_changed(None, "w2", true));
    assert!(tracker.hover_trace_changed(None, "w1", false));
}

// --- stage 4b FFC focus oracle: dispatched-but-unproven vs no setter ---
//
// The trace oracle must distinguish a focus setter that ran without
// immediate foreground proof from a focus that never dispatched. Both
// read as `effects == 0` with `held > 0`; only the per-effect dispatch
// row separates them. These are offline predicate tests: the FakeNative
// setter dispositions stand in for programmatic refusal, never for live
// foreground behavior.

fn focus_reply(rig: &Rig, id: &str, target: &str) -> WorldReply {
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.effects.push(focus_effect(id, target));
    reply
}

#[test]
fn focus_diag_records_dispatched_but_unproven_with_foreground_false() {
    // Setter refuses yet the immediate foreground read still runs: the row
    // records dispatched=true, setter=refused, foreground=false, so the
    // trace proves the setter ran without proof. Static-hover retry policy
    // is unchanged: at-most-once still holds the effect.
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    native.fail_focus.insert("w1".to_owned());
    let reply = focus_reply(&rig, "ffc-w1", "w1");
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert_eq!(row.id, "ffc-w1");
    assert_eq!(row.target, "w1");
    assert_eq!(row.kind, "focus");
    assert!(row.dispatched);
    assert_eq!(row.setter, "refused");
    // Immediate foreground proof rides even when false: absence of proof
    // is explicit, never an omitted field.
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("foreground-unproven"));
}

#[test]
fn focus_diag_records_throwing_setter_without_foreground_proof() {
    // A throwing setter may still have executed: the row records the throw
    // with foreground=false, never a fabricated refusal or success.
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    native.throw_focus.insert("w1".to_owned());
    let reply = focus_reply(&rig, "ffc-w1", "w1");
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "threw");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("foreground-unproven"));
}

#[test]
fn focus_diag_distinguishes_no_setter_hold_from_dispatched() {
    // Held scope never dispatches: dispatched=false with the individual
    // hold reason, so the oracle separates it from the dispatched rows
    // above despite identical aggregate counts.
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    // No live view proof for the target output: switch-before-focus holds
    // before any setter runs.
    let reply = focus_reply(&rig, "ffc-w1", "w1");
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(!row.dispatched);
    assert_eq!(row.setter, "skipped");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("view-not-shown"));
}

#[test]
fn focus_diag_proven_arrival_carries_no_hold() {
    // The control: a proven focus records dispatched=true, setter=accepted,
    // foreground=true and no hold, so all three oracle states differ.
    let rig = rig();
    let (mut diff, fresh, mut native) = warp_rig(&rig);
    let reply = focus_reply(&rig, "ffc-w1", "w1");
    let (stats, _, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 1);
    assert!(native.is_foreground("w1"));
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "accepted");
    assert!(row.foreground);
    assert_eq!(row.hold, None);
}

// --- floating-scope standalone views: actuation into Untouched targets ---

/// Production-faithful output row for the view tests below: the core
/// always publishes one row per screen, so the seam's output-hold gate
/// reads a real row (never a missing one).
fn output_row(
    out: &str,
    view: &str,
    hold: Option<CoreHold>,
) -> tiler_core::world_report::OutputDesire {
    tiler_core::world_report::OutputDesire {
        output: OutputId(out.to_owned()),
        logical_view: Some(view.to_owned()),
        tiling_mode: tiler_core::world_report::TilingMode::Floating,
        hold,
    }
}

/// Standalone view switches into a floating (Untouched) target must still
/// actuate: the core publishes Untouched scope authority for floating
/// workspaces (never frame writes), but the output view still needs the
/// native hide/reveal with truthful read-back proof. The core owns
/// completion over the reported raw attempt plus the staged echo; the seam
/// reports mechanical facts only, never a verdict.
#[test]
fn standalone_view_actuates_into_untouched_floating_target() {
    let rig = rig();
    let ws_float = "ws-float".to_owned();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_float.as_str()),
        ],
    );
    // Core publishes the floating target scope Untouched (never Controlled).
    for status in &mut reply.scopes {
        if status.scope == format!("{}/{}", rig.out, ws_float) {
            status.authority = Auth::Untouched;
        }
    }
    reply.outputs.push(output_row(&rig.out, &ws_float, None));
    reply
        .transfers
        .push(view_transfer("op-float-view", &rig.out, &ws_float));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.view_writes, 1);
    assert!(members.is_empty());
    assert_eq!(views.len(), 1, "one raw attempt rides: {views:?}");
    let attempt = &views[0];
    assert_eq!(attempt.operation_id, "op-float-view");
    assert_eq!(attempt.target_output.0, rig.out);
    assert_eq!(attempt.target_workspace, ws_float);
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.pre.as_deref(), Some(rig.ws.as_str()));
    assert_eq!(attempt.post.as_deref(), Some(ws_float.as_str()));
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(ws_float.as_str())
    );
    // Already shown: no write, no attempt. The core settles from the echo.
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.view_writes, 0);
    assert!(members.is_empty() && views.is_empty());
}

#[test]
fn standalone_view_into_floating_target_keeps_hold_and_refusal_evidence() {
    let rig = rig();
    let ws_float = "ws-float".to_owned();
    // Held scope still blocks: no setter, no attempt, truthful hold.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_float.as_str()),
        ],
    );
    for status in &mut reply.scopes {
        if status.scope == format!("{}/{}", rig.out, ws_float) {
            status.authority = Auth::Hold(CoreHold::Unreadable);
        }
    }
    reply
        .transfers
        .push(view_transfer("op-float-held", &rig.out, &ws_float));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.view_writes, 0);
    assert!(members.is_empty() && views.is_empty());
    assert!(stats.held >= 1);
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(rig.ws.as_str())
    );
    // Held output row still blocks an Untouched scope: incomplete screens
    // never take view switches.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_float.as_str()),
        ],
    );
    for status in &mut reply.scopes {
        if status.scope == format!("{}/{}", rig.out, ws_float) {
            status.authority = Auth::Untouched;
        }
    }
    reply.outputs.push(output_row(
        &rig.out,
        &ws_float,
        Some(CoreHold::IncompleteInventory),
    ));
    reply
        .transfers
        .push(view_transfer("op-float-outheld", &rig.out, &ws_float));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.view_writes, 0);
    assert!(members.is_empty() && views.is_empty());
    assert!(stats.held >= 1);
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(rig.ws.as_str())
    );
    // Gesture barrier still blocks an Untouched scope: no setter mid-gesture.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    native.gesture = true;
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_float.as_str()),
        ],
    );
    for status in &mut reply.scopes {
        if status.scope == format!("{}/{}", rig.out, ws_float) {
            status.authority = Auth::Untouched;
        }
    }
    reply.outputs.push(output_row(&rig.out, &ws_float, None));
    reply
        .transfers
        .push(view_transfer("op-float-gesture", &rig.out, &ws_float));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.view_writes, 0);
    assert!(members.is_empty() && views.is_empty());
    assert!(stats.held >= 1);
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(rig.ws.as_str())
    );
    // Explicit refusal reports raw facts (never a verdict) and never
    // repeats: the core holds the parent ambiguous on the miss.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    native.fail_view.insert(format!("{}/{}", rig.out, ws_float));
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.out.as_str(), rig.ws.as_str()),
            (rig.out.as_str(), ws_float.as_str()),
        ],
    );
    for status in &mut reply.scopes {
        if status.scope == format!("{}/{}", rig.out, ws_float) {
            status.authority = Auth::Untouched;
        }
    }
    reply.outputs.push(output_row(&rig.out, &ws_float, None));
    reply
        .transfers
        .push(view_transfer("op-float-refused", &rig.out, &ws_float));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.view_writes, 0);
    assert!(members.is_empty());
    assert_eq!(views.len(), 1);
    let attempt = &views[0];
    assert_eq!(attempt.operation_id, "op-float-refused");
    assert_eq!(attempt.setter, FrameSetter::Refused);
    assert_eq!(attempt.pre.as_deref(), Some(rig.ws.as_str()));
    assert_eq!(attempt.post.as_deref(), Some(rig.ws.as_str()));
    assert!(stats.held >= 1);
    let writes = stats.view_writes;
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.view_writes, writes);
    assert!(members.is_empty() && views.is_empty());
}

// --- floating migration/refill views: actuation into Untouched targets ---

/// Migration member rows shared by the floating-target tests below:
/// membership-only desires at the target homing (never allocation, never
/// a frame write), exactly as the core publishes them.
fn migration_member_row(out: &str, ws: &str, token: &str) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(token.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: ws.to_owned(),
        allocation: None,
        effective_frame: None,
        frame_authority: Auth::Untouched,
        membership_authority: Auth::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
        clear_operation: None,
    }
}

fn migration_reply(out: &str, ws: &str, refill_ws: &str, mig: &str) -> WorldReply {
    let mut reply = reply_shell(7, 1, &[(out, ws), (out, refill_ws)]);
    reply.transfers.push(TransferTarget {
        operation_id: mig.to_owned(),
        kind: TransferKind::Migration,
        target_output: OutputId(out.to_owned()),
        target_workspace: ws.to_owned(),
        follow: false,
        held: false,
    });
    reply.transfers.push(TransferTarget {
        operation_id: format!("{mig}-refill"),
        kind: TransferKind::View,
        target_output: OutputId(out.to_owned()),
        target_workspace: refill_ws.to_owned(),
        follow: false,
        held: false,
    });
    reply
}

/// A floating migration target must not stall the whole migration: with
/// an unheld output and no gesture barrier, members actuate their
/// membership-only moves (never frame writes: `frame_out` stays empty)
/// and the target view actuates, each reporting raw attempts for the
/// core to settle. A held migration still dispatches nothing.
#[test]
fn floating_migration_target_actuates_members_and_view() {
    let rig = rig();
    let ws_target = "ws-float".to_owned();
    let mig = "op-float-mig";
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = migration_reply(&rig.out, &ws_target, &rig.ws, mig);
    for status in &mut reply.scopes {
        if status.scope == format!("{}/{}", rig.out, ws_target) {
            status.authority = Auth::Untouched;
        }
    }
    reply.outputs.push(output_row(&rig.out, &ws_target, None));
    for token in ["w1", "w2"] {
        reply
            .windows
            .push(migration_member_row(&rig.out, &ws_target, token));
    }
    let (_stats, frames, members, views) =
        apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
    assert!(frames.is_empty(), "no frame writes ride: {frames:?}");
    assert_eq!(members.len(), 2, "both members dispatch: {members:?}");
    for attempt in &members {
        assert_eq!(attempt.operation_id, mig);
        assert_eq!(attempt.setter, FrameSetter::Accepted);
        assert_eq!(
            attempt.post,
            Some((OutputId(rig.out.clone()), ws_target.clone()))
        );
    }
    assert_eq!(
        views.len(),
        2,
        "target view actuates, then the refill follows: {views:?}"
    );
    let target_view = views
        .iter()
        .find(|v| v.operation_id == mig)
        .expect("target attempt");
    assert_eq!(target_view.target_workspace, ws_target);
    assert_eq!(target_view.setter, FrameSetter::Accepted);
    assert_eq!(target_view.post.as_deref(), Some(ws_target.as_str()));
    assert!(
        views
            .iter()
            .any(|v| v.operation_id == format!("{mig}-refill")),
        "proven target releases the refill in the same pass: {views:?}"
    );
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(rig.ws.as_str()),
        "refill follows the proven target in the same pass"
    );
    // A held migration into the same Untouched target dispatches nothing
    // and reports nothing: the at-most-once fence holds before any setter.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut reply = migration_reply(&rig.out, &ws_target, &rig.ws, mig);
    for status in &mut reply.scopes {
        if status.scope == format!("{}/{}", rig.out, ws_target) {
            status.authority = Auth::Untouched;
        }
    }
    reply.outputs.push(output_row(&rig.out, &ws_target, None));
    for transfer in &mut reply.transfers {
        if matches!(transfer.kind, TransferKind::Migration) {
            transfer.held = true;
        }
    }
    for token in ["w1", "w2"] {
        reply
            .windows
            .push(migration_member_row(&rig.out, &ws_target, token));
    }
    let (stats, frames, members, views) =
        apply_frames(&mut diff, &reply, &fresh, &mut native, 2000);
    assert!(frames.is_empty());
    assert!(members.is_empty() && views.is_empty());
    assert!(stats.held >= 1);
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(rig.ws.as_str())
    );
}

/// A floating source refill actuates only after the migration target view
/// proves: with the target already shown, the Untouched refill dispatches
/// and reports its raw attempt under its own refill id. While the target
/// view is still unproven (refused here), no refill attempt rides.
#[test]
fn floating_refill_actuates_after_target_view_proof() {
    let rig = rig();
    let ws_target = "ws-float-tgt".to_owned();
    let mig = "op-float-refill-mig";
    // Target already shown: the refill is the only remaining view work.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), ws_target.clone());
    let mut reply = migration_reply(&rig.out, &ws_target, &rig.ws, mig);
    for status in &mut reply.scopes {
        if status.scope == format!("{}/{}", rig.out, rig.ws) {
            status.authority = Auth::Untouched;
        }
    }
    reply.outputs.push(output_row(&rig.out, &rig.ws, None));
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(members.is_empty());
    assert_eq!(views.len(), 1, "refill attempt rides: {views:?}");
    let attempt = &views[0];
    assert_eq!(attempt.operation_id, format!("{mig}-refill"));
    assert_eq!(attempt.target_workspace, rig.ws);
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.pre.as_deref(), Some(ws_target.as_str()));
    assert_eq!(attempt.post.as_deref(), Some(rig.ws.as_str()));
    assert_eq!(stats.view_writes, 1);
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(rig.ws.as_str())
    );
    // Target refused: the refill waits behind the unproven target view
    // and reports nothing under its own id. The live view names a third
    // workspace so the refill is genuinely unshown (not already-shown).
    let (mut diff, fresh, mut native) = converted(&rig);
    native
        .views
        .insert(rig.out.clone(), "ws-elsewhere".to_owned());
    native
        .fail_view
        .insert(format!("{}/{}", rig.out, ws_target));
    let mut reply = migration_reply(&rig.out, &ws_target, &rig.ws, mig);
    for status in &mut reply.scopes {
        if status.scope == format!("{}/{}", rig.out, rig.ws) {
            status.authority = Auth::Untouched;
        }
    }
    reply.outputs.push(output_row(&rig.out, &ws_target, None));
    reply.outputs.push(output_row(&rig.out, &rig.ws, None));
    for token in ["w1", "w2"] {
        reply
            .windows
            .push(migration_member_row(&rig.out, &ws_target, token));
    }
    let (stats, members, views) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(members.len(), 2);
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].operation_id, mig);
    assert_eq!(views[0].setter, FrameSetter::Refused);
    assert!(
        views
            .iter()
            .all(|v| v.operation_id != format!("{mig}-refill")),
        "refill waits for target proof: {views:?}"
    );
    assert!(stats.held >= 1);
}

/// Trace-only hold storage bound: attributions past
/// [`MAX_CLEAR_HOLD_DIAG`] keep counting in `clear_holds_total` while
/// storage holds only the first rows in order. This is the regression for
/// the reviewed defect where the omitted count was derived from the
/// already-truncated storage and always reported zero past the cap.
#[test]
fn clear_hold_attribution_bounds_storage_but_keeps_total() {
    let mut stats = ApplyStats::default();
    for n in 0..MAX_CLEAR_HOLD_DIAG + 2 {
        stats.note_clear_hold(&format!("op-{n}"), "w1", "frame-race");
    }
    assert_eq!(stats.clear_holds.len(), MAX_CLEAR_HOLD_DIAG);
    assert_eq!(stats.clear_holds_total, MAX_CLEAR_HOLD_DIAG + 2);
    assert_eq!(stats.clear_holds[0].op, "op-0");
    assert_eq!(
        stats.clear_holds[MAX_CLEAR_HOLD_DIAG - 1].op,
        format!("op-{}", MAX_CLEAR_HOLD_DIAG - 1)
    );
    assert!(
        stats
            .clear_holds
            .iter()
            .all(|row| row.window == "w1" && row.hold == "frame-race"),
        "stored rows keep op/window/hold, got {:?}",
        stats.clear_holds
    );
}

// --- event-driven in-flight focus: a prior accepted activation landing
// mid-switch is the product's own landing, never a foreign supersession ---
//
// Desired behavior (user 2026-10-26): after an accepted focus setter the
// adapter remembers the exact full native target identity including
// lifetime as in-flight until EVENT_SYSTEM_FOREGROUND arrives. During a
// later switch the foreground bracket excuses an exact change to this
// in-flight target (own landing) while any other change still vetoes.
// Minimal clearing: landing, other-foreground supersession, target
// lifetime end, and a newer setter. No blocking, timers, timeouts,
// retry, or replay; the core alone decides the target and the existing
// fullscreen/game, lifetime, and at-most-once gates stay unchanged.
//
// Host ordering modeled below: switch N posts an accepted focus for w1
// whose async activation has not landed; switch N+1 brackets its view
// switch while that activation lands mid-switch, then focuses the final
// target. The own-landing legs fail on the completion-barrier path (the
// bracket vetoes the product's own landing); the foreign, recycled, and
// same-PID legs must keep vetoing after the change.

/// Mid-switch setup for [`AsyncFocusProbe`]: plain landing versus
/// lifetime-hostile variants that must never qualify as in-flight.
#[derive(Debug, Default)]
enum MidMutate {
    #[default]
    None,
    /// Recycle w1 under its HWND (new creation plus new lifetime tag)
    /// before landing on it: same number, new lifetime.
    RecycleW1,
    /// Insert a same-PID new window w9 (different HWND and creation)
    /// before landing on it.
    SamePidNewW9,
}

/// Accepted-but-async focus plus scripted mid-switch landings: the
/// offline shape of real host ordering where a prior SetForegroundWindow
/// completes during a later nonactivating view switch.
struct AsyncFocusProbe {
    inner: FakeNative,
    /// Land accepted focus setters immediately when true; when false the
    /// setter reports accepted while foreground stays put (the async
    /// request is still in flight and is never retried here).
    land_setter: bool,
    /// Foreground token to land during the next nonactivating view
    /// switch (the prior async activation completing mid-switch).
    /// One-shot: consumed by the first switch that runs.
    mid_switch_land: Option<String>,
    mutate: MidMutate,
    /// Deliver the mid-switch landing through the real WinEvent sink as
    /// EVENT_SYSTEM_FOREGROUND before flipping the poll, so the landing
    /// exercises the actual hook plumbing and not a parallel seam.
    winevent: bool,
}

impl NativeOps for AsyncFocusProbe {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.inner.read_window(token)
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.inner.read_frame(token)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.inner.read_homing(token)
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.inner.read_view(output)
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.inner.is_foreground(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.inner.gesture_barrier()
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.inner.read_dpi(token)
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        self.inner.write_frame(token, rect)
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.inner.switch_view(output, logical)
    }
    fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        use crate::ffi::WinEventSink as _;
        let result = self.inner.switch_view(output, logical);
        if result == Ok(true) {
            match self.mutate {
                MidMutate::None => {}
                MidMutate::RecycleW1 => {
                    if let Some(window) = self.inner.windows.get_mut("w1") {
                        window.key.creation = "proc-9".to_owned();
                        window.live_tag = Some("member-tag-9".to_owned());
                    }
                }
                MidMutate::SamePidNewW9 => {
                    if let Some(template) = self.inner.windows.get("w1").cloned() {
                        let mut w9 = template;
                        w9.key = win_key(1009, 2001, "proc-9");
                        w9.token = "w9".to_owned();
                        w9.live_tag = Some("member-tag-9".to_owned());
                        self.inner.windows.insert("w9".to_owned(), w9);
                    }
                }
            }
            if let Some(token) = self.mid_switch_land.take() {
                if self.winevent {
                    let hwnd = self
                        .inner
                        .windows
                        .get(&token)
                        .map(|window| window.key.hwnd)
                        .unwrap_or(1001);
                    super::observe::LoopEventSink.on_win_event(crate::ffi::WinEvent {
                        event: windows_sys::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_FOREGROUND,
                        hwnd,
                        id_object: 0,
                        thread: 7,
                        time: 9,
                    });
                }
                self.inner.foreground = Some(token);
            }
        }
        result
    }
    fn foreground_state(&mut self) -> ForegroundState {
        self.inner.foreground_state()
    }
    fn foreground_identity_of(&mut self, hwnd: u64) -> ForegroundState {
        self.inner.foreground_identity_of(hwnd)
    }
    fn gui_focus_of(&mut self, hwnd: u64) -> GuiFocusState {
        self.inner.gui_focus_of(hwnd)
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        if self.land_setter {
            self.inner.focus_window(token)
        } else {
            Ok(true)
        }
    }
    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.inner.read_cursor()
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        self.inner.move_cursor(x, y)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
    }
}

/// View-switch plus remembered-focus reply with caller-chosen operation,
/// effect, and target ids: every rapid-switch leg needs a distinct view
/// operation so the at-most-once fence never mistakes a later leg for a
/// repeat of an earlier one.
fn inflight_view_reply(rig: &Rig, rev: u64, view_op: &str, fx: &str, target: &str) -> WorldReply {
    let mut reply = reply_shell(7, rev, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .transfers
        .push(view_transfer(view_op, &rig.out, &rig.ws));
    reply.effects.push(focus_effect(fx, target));
    reply
}

fn apply_probe(
    diff: &mut HostDiff,
    reply: &WorldReply,
    fresh: &FreshHost,
    probe: &mut AsyncFocusProbe,
) -> ApplyStats {
    apply_world_reply(
        diff,
        reply,
        fresh,
        probe,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    )
}

/// Switch N posts an accepted focus for w1 whose async activation has not
/// landed; switch N+1 lands it mid-switch and must still focus the final
/// target w2. The barrier path vetoes the product's own landing here.
#[test]
fn inflight_prior_activation_landing_mid_switch_focuses_final_target() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 1, "op-v1", "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.view_writes, 1);
    assert_eq!(stats1.effects, 0);
    assert_eq!(stats1.focus_dispatch.len(), 1);
    assert_eq!(stats1.focus_dispatch[0].hold, Some("foreground-unproven"));
    probe
        .inner
        .views
        .insert(rig.out.clone(), "ws-other".to_owned());
    probe.land_setter = true;
    probe.mid_switch_land = Some("w1".to_owned());
    let stats2 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 2, "op-v2", "fx-10", "w2"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.view_writes, 1);
    assert_eq!(stats2.effects, 1, "own landing must not veto: {stats2:?}");
    assert!(probe.inner.is_foreground("w2"));
    assert_eq!(stats2.focus_dispatch.len(), 1);
    let row = &stats2.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "accepted");
    assert!(row.foreground);
    assert_eq!(row.hold, None);
}

/// A real foreign foreground move mid-switch still vetoes: the effect
/// stays unspent and nothing dispatches.
#[test]
fn inflight_foreign_foreground_mid_switch_still_vetoes() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 1, "op-v1", "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    probe
        .inner
        .views
        .insert(rig.out.clone(), "ws-other".to_owned());
    probe.land_setter = true;
    probe.mid_switch_land = Some("foreign".to_owned());
    let stats2 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 2, "op-v2", "fx-10", "w2"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.view_writes, 1);
    assert_eq!(stats2.effects, 0);
    assert_eq!(stats2.focus_dispatch.len(), 1);
    let row = &stats2.focus_dispatch[0];
    assert!(!row.dispatched);
    assert_eq!(row.setter, "skipped");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("foreground-changed"));
    assert!(diff.effects.mark_effect_spent("fx-10"));
    assert!(!probe.inner.is_foreground("w2"));
}

/// A recycled HWND (same number, new creation and lifetime tag) never
/// qualifies as the in-flight target: the bracket still vetoes.
#[test]
fn inflight_recycled_lifetime_never_qualifies_as_own_landing() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 1, "op-v1", "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    probe
        .inner
        .views
        .insert(rig.out.clone(), "ws-other".to_owned());
    probe.land_setter = true;
    probe.mutate = MidMutate::RecycleW1;
    probe.mid_switch_land = Some("w1".to_owned());
    let stats2 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 2, "op-v2", "fx-10", "w2"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.effects, 0);
    assert_eq!(stats2.focus_dispatch.len(), 1);
    assert_eq!(stats2.focus_dispatch[0].hold, Some("foreground-changed"));
    assert!(diff.effects.mark_effect_spent("fx-10"));
}

/// A same-PID new window (different HWND and creation) never qualifies
/// as the in-flight target either.
#[test]
fn inflight_same_pid_new_window_never_qualifies_as_own_landing() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 1, "op-v1", "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    probe
        .inner
        .views
        .insert(rig.out.clone(), "ws-other".to_owned());
    probe.land_setter = true;
    probe.mutate = MidMutate::SamePidNewW9;
    probe.mid_switch_land = Some("w9".to_owned());
    let stats2 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 2, "op-v2", "fx-10", "w2"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.effects, 0);
    assert_eq!(stats2.focus_dispatch.len(), 1);
    assert_eq!(stats2.focus_dispatch[0].hold, Some("foreground-changed"));
    assert!(diff.effects.mark_effect_spent("fx-10"));
}

/// Event-before-event-loop-delivery: the mid-switch landing arrives as a
/// real EVENT_SYSTEM_FOREGROUND edge through the owner sink before the
/// loop drains it, and the poll the bracket reads agrees. The edge must
/// classify as a foreground wake for the in-flight HWND through the
/// production `wake_cause`/`window_edge_wakes` rules.
#[test]
fn inflight_landing_delivery_rides_real_foreground_winevent() {
    use super::observe::{HookCause, HookEvent, hook_queue, wake_cause, window_edge_wakes};
    use windows_sys::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_FOREGROUND;
    if let Ok(mut queue) = hook_queue().lock() {
        queue.clear();
    }
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: true,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 1, "op-v1", "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    assert!(
        window_edge_wakes(EVENT_SYSTEM_FOREGROUND, 0),
        "foreground window edges must wake through the production rule"
    );
    assert_eq!(wake_cause(EVENT_SYSTEM_FOREGROUND), HookCause::Foreground);
    probe
        .inner
        .views
        .insert(rig.out.clone(), "ws-other".to_owned());
    probe.land_setter = true;
    probe.mid_switch_land = Some("w1".to_owned());
    let stats2 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 2, "op-v2", "fx-10", "w2"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.effects, 1, "own landing must not veto: {stats2:?}");
    assert!(probe.inner.is_foreground("w2"));
    let hwnd = probe
        .inner
        .windows
        .get("w1")
        .map(|window| window.key.hwnd as isize)
        .unwrap_or(1001);
    let edges: Vec<super::observe::WakeEdge> = hook_queue()
        .lock()
        .map(|queue| {
            queue
                .iter()
                .filter_map(|event| match event {
                    HookEvent::Wake(edge) => Some(edge.clone()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(edges.len(), 1, "one landing edge was delivered: {edges:?}");
    assert_eq!(edges[0].hwnd, hwnd);
    assert_eq!(edges[0].cause, HookCause::Foreground);
    assert_eq!(wake_cause(edges[0].event), HookCause::Foreground);
    if let Ok(mut queue) = hook_queue().lock() {
        queue.clear();
    }
}

/// A newer accepted setter supersedes the prior in-flight target: a later
/// landing on the stale target vetoes instead of proceeding.
#[test]
fn inflight_newer_setter_supersedes_prior_inflight() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 1, "op-v1", "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.view_writes, 1);
    assert_eq!(stats1.effects, 0);
    // The accepted setter is remembered by exact identity plus lifetime.
    assert!(
        diff.in_flight
            .as_ref()
            .is_some_and(|flight| flight.token == "w1"),
        "in-flight remembers the unproven target"
    );
    // Newer setter for w2 with no view work: no bracket, lands at once,
    // superseding (replacing then clearing on landing) the prior in-flight.
    probe.land_setter = true;
    let mut reply2 = reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply2.effects.push(focus_effect("fx-10", "w2"));
    let stats2 = apply_probe(&mut diff, &reply2, &fresh, &mut probe);
    assert_eq!(stats2.effects, 1);
    assert!(probe.inner.is_foreground("w2"));
    assert!(
        diff.in_flight.is_none(),
        "newer setter supersedes the prior in-flight"
    );
    // The stale w1 activation landing now is superseded, not own: veto.
    probe
        .inner
        .views
        .insert(rig.out.clone(), "ws-other".to_owned());
    probe.mid_switch_land = Some("w1".to_owned());
    let stats3 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 3, "op-v3", "fx-11", "w2"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats3.view_writes, 1);
    assert_eq!(stats3.effects, 0);
    assert_eq!(stats3.focus_dispatch.len(), 1);
    assert_eq!(stats3.focus_dispatch[0].hold, Some("foreground-changed"));
    assert!(diff.effects.mark_effect_spent("fx-11"));
}

/// Target lifetime end clears the in-flight at the next entry: a departed
/// baseline can never excuse a later bracket.
#[test]
fn inflight_cleared_when_target_lifetime_ends() {
    let (rig, mut diff, mut fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), "ws-other".to_owned());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_view_reply(&rig, 1, "op-v1", "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    assert!(
        diff.in_flight
            .as_ref()
            .is_some_and(|flight| flight.token == "w1"),
        "in-flight remembers the unproven target"
    );
    // The w1 lifetime ends: the next baseline drops the member.
    fresh.baseline.remove("w1");
    let followup = reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]);
    let stats2 = apply_probe(&mut diff, &followup, &fresh, &mut probe);
    assert!(stats2.focus_dispatch.is_empty());
    assert!(
        diff.in_flight.is_none(),
        "ended lifetime clears the in-flight"
    );
}

// --- event-driven in-flight resolution through the real delivery/drain
// lifecycle ---
//
// The owner loop drains hook callbacks before every apply with no message
// pump between that drain and the focus setter, so a drained edge always
// predates the pass's setter. Entry processing therefore resolves each
// drained `EVENT_SYSTEM_FOREGROUND` against the in-flight remembered by
// the previous setter: an exact full-identity-plus-lifetime match is the
// product's own landing (cleared), any other readable foreground is a
// supersession (cleared), and an unreadable edge keeps the in-flight for
// the bracket to fail closed on. Delivery-time snapshots never decide:
// only live reads plus the stored lifetime qualify, so a recycled HWND, a
// changed tag, or a same-PID new window never matches.

/// Drain collected foreground HWNDs through the REAL owner drain: edges
/// delivered through the real WinEvent sink ride the production queue,
/// classification, own-overlay exclusion, and delivery-order preservation.
fn drain_foreground_via_real_loop(hwnds: &[u64]) -> Vec<u64> {
    use super::observe::{HookCause, hook_queue};
    use super::run_loop_wait::drain_hook_edges;
    use crate::ffi::WinEventSink as _;
    use windows_sys::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_FOREGROUND;
    if let Ok(mut queue) = hook_queue().lock() {
        queue.clear();
    }
    for hwnd in hwnds {
        super::observe::LoopEventSink.on_win_event(crate::ffi::WinEvent {
            event: EVENT_SYSTEM_FOREGROUND,
            hwnd: *hwnd,
            id_object: 0,
            thread: 7,
            time: 9,
        });
    }
    // The production wake rule classifies the delivered edge before any
    // state transition is claimed from it.
    assert!(super::observe::window_edge_wakes(
        EVENT_SYSTEM_FOREGROUND,
        0
    ));
    assert_eq!(
        super::observe::wake_cause(EVENT_SYSTEM_FOREGROUND),
        HookCause::Foreground
    );
    let mut scratch = super::rmax03_adapter_tests::test_state();
    let me = process_identity(&win_key(1001, 2001, "proc-1"));
    let log = scratch.log_path.clone();
    let drain = drain_hook_edges(&mut scratch, &me, &log);
    assert!(drain.woke);
    if let Ok(mut queue) = hook_queue().lock() {
        queue.clear();
    }
    drain.foreground
}

/// Focus-only async setter: no view work, so no bracket runs and the
/// unproven target is remembered as in-flight.
fn inflight_focus_only_reply(rig: &Rig, rev: u64, fx: &str, target: &str) -> WorldReply {
    let mut reply = reply_shell(7, rev, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.effects.push(focus_effect(fx, target));
    reply
}

/// Own landing observed purely through the event lifecycle: the accepted
/// setter never lands in the poll, the real sink delivers the foreground
/// edge, the real drain collects it, and the next apply - with no
/// workspace switch at all - clears the in-flight on the exact match.
#[test]
fn inflight_clears_on_foreground_event_without_switch() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_focus_only_reply(&rig, 1, "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    assert!(
        diff.in_flight
            .as_ref()
            .is_some_and(|flight| flight.token == "w1"),
        "in-flight remembers the unproven target"
    );
    // The async activation lands outside any switch: the real sink delivers
    // the edge and the real drain collects the HWND in delivery order.
    let hwnds = drain_foreground_via_real_loop(&[1001]);
    assert_eq!(hwnds, vec![1001]);
    diff.push_foreground_events(hwnds);
    // No workspace switch runs here: the event alone resolves the landing.
    let stats2 = apply_probe(
        &mut diff,
        &reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.effects, 0);
    assert!(stats2.focus_dispatch.is_empty());
    assert!(
        diff.in_flight.is_none(),
        "own-landing event clears the in-flight with no switch"
    );
}

/// Owed-pass resolution for the key-up stranding incident, pinned at the
/// diff seam: an accepted-but-unproven focus setter arms the flight, the
/// own foreground edge drains through the real sink/drain lifecycle, and
/// an observation-only apply with no switch and no command (the exact
/// shape of the owed reconcile pass) clears it as `landed-own` with the
/// full target identity, drains the pending queue, and dispatches nothing
/// further. Scheduling half:
/// `keyup_only_turn_owes_observation_for_pending_foreground` in
/// `world_route_tests.rs` proves the key-up turn owes this pass; the
/// superseded-foreign half rides
/// `inflight_transient_foreign_then_return_clears`.
#[test]
fn keyup_owed_pass_resolves_armed_flight_as_own_landing() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    // The select tick's accepted setter never lands in the poll: the
    // unproven target is remembered as in-flight.
    let stats1 = apply_probe(
        &mut diff,
        &inflight_focus_only_reply(&rig, 1, "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    assert!(
        diff.in_flight
            .as_ref()
            .is_some_and(|flight| flight.token == "w1"),
        "in-flight remembers the unproven target"
    );
    // The own edge drains through the real sink/drain, coalesced on the
    // key-up turn with no command of its own.
    let hwnds = drain_foreground_via_real_loop(&[1001]);
    assert_eq!(hwnds, vec![1001]);
    diff.push_foreground_events(hwnds);
    // The owed observation-only pass: no switch, no command, no effects.
    let stats2 = apply_probe(
        &mut diff,
        &reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.effects, 0);
    assert!(stats2.focus_dispatch.is_empty());
    assert!(
        diff.in_flight.is_none(),
        "own-landing event clears the in-flight with no switch"
    );
    assert!(
        diff.pending_foreground.is_empty(),
        "observation consumed the drained edge"
    );
    assert_eq!(stats2.inflight_events.len(), 1);
    assert_eq!(stats2.inflight_total, 1);
    let row = &stats2.inflight_events[0];
    assert_eq!(row.cause, "landed-own");
    assert_eq!(row.effect, "fx-9");
    assert_eq!(row.token, "w1");
    assert_eq!(row.hwnd, Some(1001));
    assert_eq!(row.pid, Some(2001));
    assert_eq!(row.creation.as_deref(), Some("proc-1"));
}

/// Transient foreign foreground followed by a return to the target still
/// supersedes: the first readable non-match clears, and the return finds
/// no in-flight to confirm.
#[test]
fn inflight_transient_foreign_then_return_clears() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_focus_only_reply(&rig, 1, "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    assert!(diff.in_flight.is_some());
    // A foreign competitor takes foreground and the target returns, all
    // outside any switch: delivery order is preserved end to end.
    let hwnds = drain_foreground_via_real_loop(&[9999, 1001]);
    assert_eq!(hwnds, vec![9999, 1001]);
    diff.push_foreground_events(hwnds);
    let stats2 = apply_probe(
        &mut diff,
        &reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.effects, 0);
    assert!(
        diff.in_flight.is_none(),
        "transient foreign supersession clears even though foreground returned"
    );
}

/// A stale queued edge (delivered before the newer setter, drained in
/// production order ahead of it) resolves against the OLD in-flight and
/// can never clear the newer setter: drain-before-apply ordering plus the
/// setter-side buffer clear keep delivery order exact.
#[test]
fn inflight_stale_queued_event_cannot_clear_newer_setter() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_focus_only_reply(&rig, 1, "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    assert!(diff.in_flight.is_some());
    // Production order: the w1 landing edge drains BEFORE the newer w2
    // setter runs, so entry processing confirms the old landing first and
    // the newer setter then owns the in-flight alone. The poll names a
    // foreign window so the newer setter stays async-unproven.
    let hwnds = drain_foreground_via_real_loop(&[1001]);
    diff.push_foreground_events(hwnds);
    probe.inner.foreground = Some("foreign".to_owned());
    let stats2 = apply_probe(
        &mut diff,
        &inflight_focus_only_reply(&rig, 2, "fx-10", "w2"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.effects, 0);
    assert!(
        diff.in_flight
            .as_ref()
            .is_some_and(|flight| flight.token == "w2"),
        "stale pre-setter edge resolves the old landing; the newer setter still owns in-flight"
    );
    assert!(
        diff.pending_foreground.is_empty(),
        "setter consumes the drained edges; nothing stale survives it"
    );
}

/// A recycled lifetime never matches as an own landing: the drained edge
/// names the HWND but live creation/tag proof fails, so the event clears
/// as supersession instead of confirming.
#[test]
fn inflight_recycled_event_never_matches_new_lifetime() {
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: false,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats1 = apply_probe(
        &mut diff,
        &inflight_focus_only_reply(&rig, 1, "fx-9", "w1"),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats1.effects, 0);
    assert!(diff.in_flight.is_some());
    // The HWND is recycled under the queued edge: same number, new
    // creation and lifetime tag.
    if let Some(window) = probe.inner.windows.get_mut("w1") {
        window.key.creation = "proc-9".to_owned();
        window.live_tag = Some("member-tag-9".to_owned());
    }
    let hwnds = drain_foreground_via_real_loop(&[1001]);
    diff.push_foreground_events(hwnds);
    let stats2 = apply_probe(
        &mut diff,
        &reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats2.effects, 0);
    assert!(
        diff.in_flight.is_none(),
        "recycled lifetime clears as supersession, never as an own landing"
    );
}

// --- foreground metadata sidecar bounds ---
//
// The drained-edge metadata sidecar is diagnostic only: over-cap edges
// fold into an honest omitted count while the production HWND queue stays
// whole, and the next apply drains that count into its stats.

/// Over-cap metadata folds into the honest omission counter while every
/// drained HWND still resolves: the production queue never loses an edge
/// to the diagnostic cap.
#[test]
fn foreground_meta_sidecar_caps_with_honest_omission() {
    use super::observe::{DeliverySnapshot, ForegroundEdgeMeta};
    use super::world_diff::MAX_PENDING_FOREGROUND_META;
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    let extra: usize = 8;
    let total = MAX_PENDING_FOREGROUND_META + extra;
    let hwnds: Vec<u64> = (0..total).map(|n| 0x2_0000 + n as u64).collect();
    let metas: Vec<ForegroundEdgeMeta> = hwnds
        .iter()
        .map(|hwnd| ForegroundEdgeMeta {
            hwnd: *hwnd,
            event_ms: 9,
            thread: 7,
            queued_mono_ms: 11,
            drain_mono_ms: 13,
            delivery: DeliverySnapshot::Unknown,
        })
        .collect();
    diff.push_foreground_drain(hwnds.clone(), metas);
    assert_eq!(
        diff.pending_foreground, hwnds,
        "production HWND queue stays whole past the sidecar cap"
    );
    assert_eq!(
        diff.pending_foreground_meta.len(),
        MAX_PENDING_FOREGROUND_META
    );
    assert_eq!(diff.pending_foreground_omitted, extra as u64);
}

/// The sidecar omission drains into the next pass's stats with the queue
/// itself, so the `world-apply` row reports the fold instead of dropping
/// it silently. Foreign edges resolve as no-ops here (no in-flight), which
/// is exactly the point: the count rides the stats regardless.
#[test]
fn foreground_meta_omission_drains_into_apply_stats() {
    use super::observe::{DeliverySnapshot, ForegroundEdgeMeta};
    use super::world_diff::MAX_PENDING_FOREGROUND_META;
    let (rig, mut diff, fresh, mut native) = view_return_base();
    native.foreground = Some("w2".to_owned());
    native.pending_invalidation = false;
    native.views.insert(rig.out.clone(), rig.ws.clone());
    let extra: usize = 5;
    let total = MAX_PENDING_FOREGROUND_META + extra;
    let hwnds: Vec<u64> = (0..total).map(|n| 0x3_0000 + n as u64).collect();
    let metas: Vec<ForegroundEdgeMeta> = hwnds
        .iter()
        .map(|hwnd| ForegroundEdgeMeta {
            hwnd: *hwnd,
            event_ms: 9,
            thread: 7,
            queued_mono_ms: 11,
            drain_mono_ms: 13,
            delivery: DeliverySnapshot::Unknown,
        })
        .collect();
    diff.push_foreground_drain(hwnds, metas);
    let mut probe = AsyncFocusProbe {
        inner: native,
        land_setter: true,
        mid_switch_land: None,
        mutate: MidMutate::None,
        winevent: false,
    };
    let stats = apply_probe(
        &mut diff,
        &reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]),
        &fresh,
        &mut probe,
    );
    assert_eq!(stats.foreground_meta_omitted, extra as u64);
    assert_eq!(
        diff.pending_foreground_omitted, 0,
        "omission counter drains with the queue, never accumulates across passes"
    );
    assert!(diff.pending_foreground.is_empty());
    assert!(diff.pending_foreground_meta.is_empty());
}

/// Generation reset clears the queue and its omission counter together: a
/// reset must never leak a stale fold into a later pass's stats.
#[test]
fn generation_reset_clears_foreground_omission_counter() {
    use super::observe::{DeliverySnapshot, ForegroundEdgeMeta};
    use super::world_diff::MAX_PENDING_FOREGROUND_META;
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    let total = MAX_PENDING_FOREGROUND_META + 3;
    let hwnds: Vec<u64> = (0..total).map(|n| 0x4_0000 + n as u64).collect();
    let metas: Vec<ForegroundEdgeMeta> = hwnds
        .iter()
        .map(|hwnd| ForegroundEdgeMeta {
            hwnd: *hwnd,
            event_ms: 9,
            thread: 7,
            queued_mono_ms: 11,
            drain_mono_ms: 13,
            delivery: DeliverySnapshot::Unknown,
        })
        .collect();
    diff.push_foreground_drain(hwnds, metas);
    assert_eq!(diff.pending_foreground_omitted, 3);
    diff.reset_generation("gen-2");
    assert!(diff.pending_foreground.is_empty());
    assert!(diff.pending_foreground_meta.is_empty());
    assert_eq!(diff.pending_foreground_omitted, 0);
}

/// Successive drains never join later detail onto an earlier production
/// HWND by bare index: after a 40-edge drain (32 retained, 8 omitted) one
/// more edge with detail leaves the sidecar at its 32-row prefix, counts
/// the honest 9 omitted, and resolves the gapped slots as unknown.
#[test]
fn foreground_meta_sidecar_keeps_prefix_across_drains() {
    use super::observe::{DeliverySnapshot, ForegroundEdgeMeta};
    use super::world_diff::MAX_PENDING_FOREGROUND_META;
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    let first: Vec<u64> = (0..40).map(|n| 0x5_0000 + n as u64).collect();
    let first_meta: Vec<ForegroundEdgeMeta> = first
        .iter()
        .take(MAX_PENDING_FOREGROUND_META)
        .map(|hwnd| ForegroundEdgeMeta {
            hwnd: *hwnd,
            event_ms: 9,
            thread: 7,
            queued_mono_ms: 11,
            drain_mono_ms: 13,
            delivery: DeliverySnapshot::Unknown,
        })
        .collect();
    diff.push_foreground_drain_traced(first.clone(), first_meta, 8);
    assert_eq!(diff.pending_foreground.len(), 40);
    assert_eq!(diff.pending_foreground_meta.len(), 32);
    assert_eq!(diff.pending_foreground_omitted, 8);
    let later = vec![0x5_1000u64];
    let later_meta = vec![ForegroundEdgeMeta {
        hwnd: later[0],
        event_ms: 10,
        thread: 7,
        queued_mono_ms: 12,
        drain_mono_ms: 14,
        delivery: DeliverySnapshot::Unknown,
    }];
    diff.push_foreground_drain_traced(later.clone(), later_meta, 0);
    assert_eq!(
        diff.pending_foreground,
        first
            .iter()
            .copied()
            .chain(later.iter().copied())
            .collect::<Vec<_>>(),
        "production queue stays whole and ordered"
    );
    assert_eq!(
        diff.pending_foreground_meta.len(),
        MAX_PENDING_FOREGROUND_META,
        "later detail never joins the earlier gapped HWND at index 32"
    );
    assert!(
        diff.pending_foreground_meta
            .iter()
            .all(|m| m.hwnd != later[0]),
        "later metadata is omitted, not misassigned"
    );
    assert_eq!(diff.pending_foreground_omitted, 9);
}

/// In-flight clearing attribution stays bounded with an honest total: past
/// the row cap every clearing still counts, so the tick diag reports the
/// excess instead of a false small number.
#[test]
fn inflight_clearing_attribution_counts_past_row_cap() {
    use super::world_diff::{InFlightEventDiag, MAX_INFLIGHT_DIAG};
    let mut stats = ApplyStats::default();
    for n in 0..(MAX_INFLIGHT_DIAG + 4) {
        stats.note_inflight(InFlightEventDiag {
            token: format!("w{n}"),
            effect: format!("fx-{n}"),
            cause: "superseded-foreign",
            edge_event_ms: None,
            edge_queued_mono_ms: None,
            edge_drain_mono_ms: None,
            resolve_mono_ms: 21,
            gui: Some("unknown".to_owned()),
            hwnd: None,
            pid: None,
            creation: None,
        });
    }
    assert_eq!(stats.inflight_events.len(), MAX_INFLIGHT_DIAG);
    assert_eq!(stats.inflight_total, MAX_INFLIGHT_DIAG + 4);
}

/// Executed visibility-post attribution stays bounded with an honest
/// total: past the row cap every post still counts, so the tick diag
/// reports the excess instead of a false small number.
#[test]
fn visibility_post_attribution_counts_past_row_cap() {
    use super::world_diff::MAX_VISIBILITY_DIAG;
    use crate::product_hide::sys::{VIEW_POST_API_POS, VisibilityPostDiag};
    let mut stats = ApplyStats::default();
    for n in 0..(MAX_VISIBILITY_DIAG + 3) {
        stats.note_visibility_post(VisibilityPostDiag {
            hwnd: 1000 + n as u64,
            hide: n % 2 == 0,
            api: VIEW_POST_API_POS,
            flags: Some(crate::product_hide::sys::NA_POS_HIDE_FLAGS),
            cmd: None,
            posted: true,
            call_mono_ms: 11,
            ret_mono_ms: 13,
        });
    }
    assert_eq!(stats.visibility_posts.len(), MAX_VISIBILITY_DIAG);
    assert_eq!(stats.visibility_total, MAX_VISIBILITY_DIAG + 3);
    assert_eq!(stats.visibility_posts[0].hwnd, 1000);
}

// --- per-native-call focus diagnostics (B1-B4, B8) ---
//
// The adapter dispatch and the native primitive are distinct facts: the
// row reports whether `SetForegroundWindow` actually ran, with stamps
// strictly around the primitive and the pre-call GUI read, plus the full
// target identity for the `member-admitted` join.

/// Plain focus reply with no view transfer: the target already lives on
/// the shown view, so the effect dispatches through the setter lane.
fn plain_focus_reply(rig: &Rig, rev: u64, id: &str, target: &str) -> WorldReply {
    let mut reply = reply_shell(7, rev, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.effects.push(OneShotEffect {
        id: id.to_owned(),
        kind: EffectKind::Focus,
        target: WindowId(target.to_owned()),
        issued_revision: 1,
    });
    reply
}

fn apply_plain(
    diff: &mut HostDiff,
    reply: &WorldReply,
    fresh: &FreshHost,
    native: &mut FakeNative,
) -> ApplyStats {
    apply_world_reply(
        diff,
        reply,
        fresh,
        native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    )
}

/// An actual primitive call reports the full record: `primitive_called`
/// with the exact API, call/return stamps in order, the pre-call GUI read
/// (dispatch) distinct from the read-back GUI (landing), and the full
/// target identity.
#[test]
fn focus_diag_records_actual_primitive_call() {
    use super::world_diff::FOCUS_SETTER_API;
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.set_diagnostics_trace(true);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    native.gui_state.insert(
        1001,
        GuiFocusState::Known {
            thread: 7,
            active: 0,
            focus: 0,
        },
    );
    let stats = apply_plain(
        &mut diff,
        &plain_focus_reply(&rig, 1, "fx-1", "w1"),
        &fresh,
        &mut native,
    );
    assert_eq!(stats.effects, 1);
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched);
    assert_eq!(row.setter, "accepted");
    assert!(row.foreground);
    assert!(row.primitive_called);
    assert_eq!(row.native_return, Some(true));
    assert_eq!(row.api, Some(FOCUS_SETTER_API));
    let (call, ret) = (
        row.call_mono_ms.expect("actual call stamp"),
        row.ret_mono_ms.expect("actual return stamp"),
    );
    assert!(call <= ret, "primitive window orders call before return");
    // Pre-call GUI read keeps the pre value; the landing read observes the
    // activation the primitive caused.
    assert_eq!(
        row.gui_dispatch.as_deref(),
        Some("thread:7:active:0:focus:0")
    );
    assert_eq!(
        row.gui_landing.as_deref(),
        Some("thread:7:active:1001:focus:1001")
    );
    assert_eq!(row.hwnd, Some(1001));
    assert_eq!(row.pid, Some(2001));
    assert_eq!(row.creation.as_deref(), Some("proc-1"));
    assert_eq!(native.gui_calls, 2, "pre-call plus landing reads only");
}

/// A pre-primitive veto dispatches but never runs the native setter:
/// `primitive_called` is false with no API and no stamps, and no GUI read
/// issues.
#[test]
fn focus_diag_veto_reports_no_primitive() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.set_diagnostics_trace(true);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    native.move_size.insert("w1".to_owned());
    let stats = apply_plain(
        &mut diff,
        &plain_focus_reply(&rig, 1, "fx-1", "w1"),
        &fresh,
        &mut native,
    );
    assert_eq!(stats.focus_dispatch.len(), 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.dispatched, "adapter attempted the setter");
    assert_eq!(row.setter, "refused");
    assert!(!row.foreground);
    assert_eq!(row.hold, Some("foreground-unproven"));
    assert!(!row.primitive_called);
    assert_eq!(row.native_return, None);
    assert!(row.api.is_none());
    assert!(row.call_mono_ms.is_none());
    assert!(row.ret_mono_ms.is_none());
    assert!(row.gui_dispatch.is_none());
    assert!(row.gui_landing.is_none());
    assert_eq!(row.hwnd, Some(1001));
    assert_eq!(row.pid, Some(2001));
    assert_eq!(row.creation.as_deref(), Some("proc-1"));
    assert_eq!(native.gui_calls, 0, "vetoed primitive issues no GUI read");
}

/// Trace off suppresses every diagnostics GUI read: the rows carry no GUI
/// labels and the seam counts zero reads, while the behavior (proven
/// focus) and the primitive record stay intact.
#[test]
fn focus_diag_trace_off_suppresses_gui_reads() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    native.gui_state.insert(
        1001,
        GuiFocusState::Known {
            thread: 7,
            active: 0,
            focus: 0,
        },
    );
    assert!(!native.diagnostics_trace());
    let stats = apply_plain(
        &mut diff,
        &plain_focus_reply(&rig, 1, "fx-1", "w1"),
        &fresh,
        &mut native,
    );
    assert_eq!(stats.effects, 1);
    let row = &stats.focus_dispatch[0];
    assert!(row.primitive_called);
    assert_eq!(row.native_return, Some(true));
    assert!(row.call_mono_ms.is_some());
    assert!(row.ret_mono_ms.is_some());
    assert!(row.gui_dispatch.is_none());
    assert!(row.gui_landing.is_none());
    assert_eq!(native.gui_calls, 0);
}

/// A drained foreign edge clearing a remembered flight reports the edge's
/// delivery/owner stamps plus the remembered target identity, with the GUI
/// sampled at resolution time.
#[test]
fn inflight_clearing_carries_edge_stamps_and_target_identity() {
    use super::observe::{DeliverySnapshot, ForegroundEdgeMeta};
    use super::world_diff::InFlightFocus;
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.set_diagnostics_trace(true);
    native.views.insert(rig.out.clone(), rig.ws.clone());
    diff.in_flight = Some(InFlightFocus {
        token: "w1".to_owned(),
        key: win_key(1001, 2001, "proc-1"),
        tag: "member-tag-1".to_owned(),
        effect: "fx-1".to_owned(),
        set_mono_ms: 5,
    });
    diff.push_foreground_drain(
        vec![0x9_9999],
        vec![ForegroundEdgeMeta {
            hwnd: 0x9_9999,
            event_ms: 31,
            thread: 3,
            queued_mono_ms: 11,
            drain_mono_ms: 13,
            delivery: DeliverySnapshot::Unknown,
        }],
    );
    let stats = apply_plain(
        &mut diff,
        &reply_shell(7, 2, &[(rig.out.as_str(), rig.ws.as_str())]),
        &fresh,
        &mut native,
    );
    assert!(diff.in_flight.is_none());
    assert_eq!(stats.inflight_events.len(), 1);
    let row = &stats.inflight_events[0];
    assert_eq!(row.cause, "superseded-foreign");
    assert_eq!(row.edge_event_ms, Some(31));
    assert_eq!(row.edge_queued_mono_ms, Some(11));
    assert_eq!(row.edge_drain_mono_ms, Some(13));
    assert_eq!(row.hwnd, Some(1001));
    assert_eq!(row.pid, Some(2001));
    assert_eq!(row.creation.as_deref(), Some("proc-1"));
    assert!(
        row.gui.is_some(),
        "resolution-time GUI sampled while traced"
    );
}
