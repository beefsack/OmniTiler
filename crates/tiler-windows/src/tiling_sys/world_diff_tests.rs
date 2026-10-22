#![forbid(unsafe_code)]
//! Offline capture/sequencer/diff safety regressions (source-first).
//!
//! Every rig builds the real owner tables plus real per-tick products and
//! runs them through [`capture_world`](super::world_capture::capture_world)
//! and [`FreshHost::from_sources`](super::world_diff::FreshHost::from_sources).
//! Fixtures diverge only the live [`FakeNative`] facts per fence; feedback
//! uses only core-consumed operation ids. Native workspace
//! creation/removal/binding shapes are intentionally absent: the logical
//! core owns workspaces outright, so there is nothing to actuate there.

use super::rmax03_adapter_tests::test_state;
use super::world_capture::capture_world;
use super::world_diff::{
    ApplyStats, FakeNative, FreshHost, FreshWindow, HostDiff, NativeOps, apply_world_reply,
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
    AttemptFeedback, AttemptKind, AttemptOutcome, EffectKind, FrameSetter, HoldReason as CoreHold,
    OneShotEffect, ScopeAuthority as Auth, ScopeStatus, TransferKind, TransferTarget,
    WORLD_CONTRACT_VERSION, WindowDesire, WorldCommand, WorldReply, WorldReport,
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
            // Generic fixture-style identity: the proof-only helper tag is
            // empty; managed membership rides `member_tag` below.
            tag: String::new(),
        },
        facts,
        member_tag: Some("member-tag-1".to_owned()),
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

fn admit(state: &mut TileLoop, key: &WindowKey, _dev: &str, _ws: &str, token: &str, _hidden: bool) {
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

struct Rig {
    state: TileLoop,
    observed: Vec<ObservedWindow>,
    retained: Vec<RetainedRow>,
    areas: Vec<MonitorArea>,
    out: String,
    ws: String,
}

fn rig() -> Rig {
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
        presentation_bytes: 0,
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
) -> (ApplyStats, Vec<AttemptFeedback>) {
    apply_world_reply(diff, reply, fresh, native, &mut Vec::new(), 0)
}

/// Diff helper that keeps the mechanical frame attempts: the seam-level
/// proof that writes report raw facts (never verdicts) for the core.
fn apply_frames(
    diff: &mut HostDiff,
    reply: &WorldReply,
    fresh: &FreshHost,
    native: &mut FakeNative,
    attempt_ms: u64,
) -> (
    ApplyStats,
    Vec<AttemptFeedback>,
    Vec<tiler_core::world_report::FrameAttempt>,
) {
    let mut frames = Vec::new();
    let (stats, feedback) = apply_world_reply(diff, reply, fresh, native, &mut frames, attempt_ms);
    (stats, feedback, frames)
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
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(feedback.is_empty());
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
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(stats.held, 0);
    assert!(feedback.is_empty());
    assert!(!stats.partial);

    let (mut diff, fresh, mut native) = converted(&rig);
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
    row.effective_frame = None;
    reply.windows.push(row);
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(feedback.is_empty());
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
    let (stats, feedback, frames) = apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 0);
    assert!(feedback.is_empty());
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
    let (stats, feedback, frames) = apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(stats.held, 1);
    assert!(feedback.is_empty());
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
    let (stats, _, held_frames) = apply_frames(&mut diff, &held, &fresh, &mut native, 2000);
    assert_eq!(native.frame_writes, writes);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(held_frames.is_empty());
    let mut moved = reply_shell(7, 3, &[(rig.out.as_str(), rig.ws.as_str())]);
    moved
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(20, 20, 400, 300)));
    let (stats, _, _) = apply_frames(&mut diff, &moved, &fresh, &mut native, 3000);
    assert_eq!(stats.geometry_writes, 1);
}

#[test]
fn explicit_refusal_reports_transient_facts_without_feedback() {
    // A refused setter reports transient facts (never a verdict) with no
    // operation feedback: ordinary geometry proves by read-back. The core
    // times the retry; the seam retries on the next controlled desire.
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.fail_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, feedback, frames) = apply_frames(&mut diff, &reply, &fresh, &mut native, 1000);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
    assert!(feedback.is_empty());
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].setter, FrameSetter::Refused);
    assert_eq!(frames[0].target, rect(10, 10, 400, 300));
    // A controlled re-desire retries the setter (no local backoff memory).
    let writes = native.frame_writes;
    native.fail_frame.remove("w1");
    let (stats, _, _) = apply_frames(&mut diff, &reply, &fresh, &mut native, 2000);
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
        let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
        assert_eq!(stats.geometry_writes, 0, "flag {name} wrote");
        assert!(
            stats.held >= 1 || stats.stopped_mid_apply,
            "flag {name} passed"
        );
        assert!(feedback.is_empty(), "flag {name} invented feedback");
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
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
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
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);

    let (mut diff, fresh, mut native) = converted(&rig);
    diff.gate.current_obs = 8;
    let reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert!(stats.stopped_mid_apply);

    let (mut diff, fresh, mut native) = converted(&rig);
    diff.gate
        .note_applied(&reply_shell(7, 3, &[(rig.out.as_str(), rig.ws.as_str())]));
    for rev in [2, 3] {
        let stale = reply_shell(7, rev, &[(rig.out.as_str(), rig.ws.as_str())]);
        let (stats, _) = apply(&mut diff, &stale, &fresh, &mut native);
        assert!(stats.stopped_mid_apply, "rev {rev} authorized");
    }

    let (mut diff, fresh, mut native) = converted(&rig);
    let mut foreign = reply_shell(7, 4, &[(rig.out.as_str(), rig.ws.as_str())]);
    foreign.core_generation = "gen-9".to_owned();
    let (stats, _) = apply(&mut diff, &foreign, &fresh, &mut native);
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
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
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
        remap: bool,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame, remap)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.inner.switch_view(output, logical)
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.inner.focus_window(token)
    }
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
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
    let (stats, _) = apply_world_reply(&mut diff, &reply, &fresh, &mut probe, &mut Vec::new(), 0);
    assert_eq!(stats.geometry_writes, 1);
    assert!(stats.stopped_mid_apply);
    assert!(stats.partial);
    assert_eq!(diff.gate.current_obs, 8);
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
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
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
    diff.effects.arm("op-stale", "ctx".to_owned());
    diff.effects.arm("op-1", "ctx".to_owned());
    diff.effects.mark_effect_spent("fx-stale");
    diff.effects.mark_float_spent("op-stale-float");
    diff.effects.mark_float_spent("op-float-live");
    diff.effects.mark_view_spent("op-stale-view");
    diff.effects.applied_window.insert(
        "w-gone".to_owned(),
        ("op-1".to_owned(), rig.out.clone(), rig.ws.clone()),
    );
    diff.effects.applied_float.insert(
        "w-gone".to_owned(),
        ("op-float-live".to_owned(), rect(1, 1, 10, 10)),
    );
    diff.effects
        .applied_view
        .insert(rig.out.clone(), ("op-stale".to_owned(), rig.ws.clone()));
    diff.effects
        .applied_view
        .insert("o9".to_owned(), ("op-9".to_owned(), "ws-gone".to_owned()));
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
    assert!(diff.effects.is_ambiguous("op-1"));
    assert!(!diff.effects.is_ambiguous("op-stale"));
    assert!(diff.effects.mark_float_spent("op-stale-float"));
    assert!(!diff.effects.mark_float_spent("op-float-live"));
    assert!(diff.effects.mark_view_spent("op-stale-view"));
    assert!(diff.effects.mark_effect_spent("fx-stale"));
    assert!(!diff.effects.applied_window.contains_key("w-gone"));
    assert!(!diff.effects.applied_float.contains_key("w-gone"));
    assert_eq!(
        diff.effects.applied_view.get(&rig.out),
        Some(&("op-view-live".to_owned(), rig.ws.clone()))
    );
    assert!(!diff.effects.applied_view.contains_key("o9"));
}

#[test]
fn generation_reset_clears_scoped_state() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    diff.effects.arm("op-1", "o1/ws".to_owned());
    assert!(diff.effects.is_ambiguous("op-1"));
    diff.effects.mark_effect_spent("fx-1");
    diff.gate
        .note_applied(&reply_shell(7, 5, &[(rig.out.as_str(), rig.ws.as_str())]));
    diff.reset_generation("gen-2");
    assert!(!diff.effects.is_ambiguous("op-1"));
    assert!(diff.effects.mark_effect_spent("fx-1"));
    assert_eq!(diff.gate.current_obs, 0);
    assert!(!diff.gate.has_accepted);
    let mut reply = reply_shell(0, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    reply.core_generation = "gen-2".to_owned();
    reply
        .windows
        .push(tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300)));
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
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
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-1" && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
    assert!(diff.effects.is_ambiguous("op-1"));
    let before = native.membership_writes;
    let (stats2, second) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(native.membership_writes, before);
    assert_eq!(stats2.membership_writes, 0);
    assert!(second.is_empty());
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
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(feedback.iter().any(|f| f.operation_id == "op-2"
        && f.outcome == AttemptOutcome::RefusedProven
        && f.hold == Some(CoreHold::RefusedTarget)));

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
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-3" && f.outcome == AttemptOutcome::SupersededExternal)
    );
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
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-4" && f.outcome == AttemptOutcome::Applied)
    );
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
    let (stats, first) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(
        first
            .iter()
            .any(|f| f.operation_id == "op-5" && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
    assert!(
        first
            .iter()
            .all(|f| f.operation_id != "op-5" || f.kind == AttemptKind::Transfer)
    );
    let (stats, second) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert!(second.is_empty());
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
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));

    native.views.insert(rig.out.clone(), rig.ws.clone());
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 1);
    assert!(native.is_foreground("w1"));
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);

    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().workspace = rig.ws.clone();
    native.views.insert(rig.out.clone(), rig.ws.clone());
    diff.gate.current_obs = 9;
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.effects, 0);
    assert!(!native.is_foreground("w1"));
    assert!(stats.stopped_mid_apply);
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
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 1);
    assert_eq!(stats.view_writes, 0);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == mig && f.outcome == AttemptOutcome::PossiblyExecuted)
    );

    native.throw_membership.clear();
    native.windows.get_mut("w2").unwrap().workspace = ws_target.clone();
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(stats.view_writes, 2);
    assert_eq!(
        native.views.get(&rig.out).map(String::as_str),
        Some(rig.ws.as_str())
    );
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
    let (_, _) = apply(&mut diff, &reply, &fresh, &mut native);
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
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 0);
    assert_eq!(stats.held, 1);
}

// --- float placements ---

#[test]
fn float_token_places_once_fences_and_truthful_settles() {
    let rig = rig();
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().floating = true;
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
    row.frame_authority = Auth::Untouched;
    row.frame_operation = Some("op-9".to_owned());
    reply.windows.push(row);
    let (stats, feedback) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.geometry_writes, 1);
    assert!(
        feedback
            .iter()
            .any(|f| f.operation_id == "op-9" && f.outcome == AttemptOutcome::Applied)
    );
    let writes = native.frame_writes;
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(native.frame_writes, writes);
    assert_eq!(stats.geometry_writes, 0);

    // Live sticky and fit-excluded intents never take float writes.
    for lane in ["sticky", "excluded"] {
        let (mut diff, fresh, mut native) = converted(&rig);
        native.windows.get_mut("w1").unwrap().floating = true;
        if lane == "sticky" {
            native.windows.get_mut("w1").unwrap().sticky = true;
        } else {
            native.windows.get_mut("w1").unwrap().fit_excluded = true;
        }
        let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
        let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
        row.frame_authority = Auth::Untouched;
        row.frame_operation = Some(format!("op-float-{lane}"));
        reply.windows.push(row);
        let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
        assert_eq!(stats.geometry_writes, 0, "live {lane} took a float write");
        assert!(stats.held >= 1);
    }

    // Clamp settles possibly-executed without repeating the setter.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().floating = true;
    native.clamp_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
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
    assert_eq!(native.frame_writes, writes);
    assert_eq!(stats.geometry_writes, 0);
    assert!(second.is_empty());

    // Throw settles possibly-executed without repeating the setter.
    let (mut diff, fresh, mut native) = converted(&rig);
    native.windows.get_mut("w1").unwrap().floating = true;
    native.throw_frame.insert("w1".to_owned());
    let mut reply = reply_shell(7, 1, &[(rig.out.as_str(), rig.ws.as_str())]);
    let mut row = tiled_row(&rig.out, &rig.ws, "w1", rect(10, 10, 400, 300));
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
    assert_eq!(native.frame_writes, writes);
    assert!(second.is_empty());
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
        remap: bool,
    ) -> Result<bool, ()> {
        self.inner.move_membership(token, out, ws, frame, remap)
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
    fn poll_external_invalidation(&mut self) -> bool {
        self.inner.poll_external_invalidation()
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
    let (_, feedback) =
        apply_world_reply(&mut diff, &reply, &fresh, &mut probe, &mut Vec::new(), 0);
    let got = feedback
        .iter()
        .find(|f| f.operation_id == "op-view-async")
        .expect("view feedback");
    assert_eq!(got.outcome, AttemptOutcome::PossiblyExecuted);
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
    let (_, feedback) =
        apply_world_reply(&mut diff, &reply, &fresh, &mut probe, &mut Vec::new(), 0);
    let got = feedback
        .iter()
        .find(|f| f.operation_id == "op-view-third")
        .expect("view feedback");
    assert_ne!(got.outcome, AttemptOutcome::RefusedProven);
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
    let (_, first) = apply_world_reply(&mut diff, &reply, &fresh, &mut probe, &mut Vec::new(), 0);
    assert!(
        first
            .iter()
            .any(|f| f.operation_id == op && f.outcome == AttemptOutcome::PossiblyExecuted)
    );
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
    let (stale_stats, stale_feedback) =
        apply_world_reply(&mut diff, &stale, &fresh, &mut probe, &mut Vec::new(), 0);
    assert!(stale_stats.stopped_mid_apply);
    assert!(stale_feedback.is_empty());
    assert_eq!(probe.switches, 1);
    assert!(diff.effects.is_view_spent(op));

    let (_, second) = apply_world_reply(&mut diff, &reply, &fresh, &mut probe, &mut Vec::new(), 0);
    assert_eq!(probe.switches, 1);
    assert!(second.is_empty() || second.iter().all(|f| f.operation_id != op));
}

// --- core-owned frame loop: attempt -> report -> desire -> obeyed ---

fn seq_deliver(seq: &mut WorldSequencer, report: &WorldReport) -> WorldReply {
    match seq.deliver(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn seq_ack(seq: &mut WorldSequencer, reply: &WorldReply) {
    match seq.ack(reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
}

fn dispatch_obs(seq: &mut WorldSequencer, rig: &Rig, foreground: u64, ms: u64) -> WorldReport {
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

/// Model truthful observation: the next capture reads what the native
/// desktop actually holds after the last apply pass.
fn sync_observed_from_native(rig: &mut Rig, native: &FakeNative) {
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

fn tiled_desire(reply: &WorldReply, window: &str) -> (Rect, Auth) {
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
    let (stats, feedback) =
        apply_world_reply(&mut diff, &reply, &fresh, &mut native, &mut attempts, 1000);
    assert_eq!(stats.geometry_writes, 2);
    assert!(feedback.is_empty());
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
    let (stats, _) =
        apply_world_reply(&mut diff, &reply2, &fresh, &mut native, &mut attempts, 2000);
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
    let (stats, _) =
        apply_world_reply(&mut diff, &reply3, &fresh, &mut native, &mut attempts, 3000);
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
    let (stats, feedback) =
        apply_world_reply(&mut diff, &reply, &fresh, &mut native, &mut attempts, 1000);
    assert_eq!(stats.geometry_writes, 1);
    assert_eq!(stats.held, 1);
    assert!(feedback.is_empty());
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
    apply_world_reply(&mut diff, &reply2, &fresh, &mut native, &mut attempts, 1100);
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
    let (tick3_stats, _) =
        apply_world_reply(&mut diff, &reply3, &fresh, &mut native, &mut attempts, 2000);
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
    apply_world_reply(&mut diff, &reply, &fresh, &mut native, &mut attempts, 1000);
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

// --- sequencer ordering, feedback, closes ---

fn send_rig() -> Rig {
    rig()
}

#[test]
fn send_roundtrip_actuates_feedback_and_converges_without_remove() {
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
    let (stats, produced) = apply(&mut diff, &reply, &fresh, &mut native);
    assert_eq!(stats.membership_writes, 1);
    assert!(
        produced
            .iter()
            .any(|f| f.operation_id == op && f.outcome == AttemptOutcome::Applied)
    );
    diff.gate.note_applied(&reply);
    let writes = native.membership_writes;
    let (stats, _) = apply(&mut diff, &reply, &fresh, &mut native);
    assert!(stats.stopped_mid_apply);
    assert_eq!(native.membership_writes, writes);
    for item in &produced {
        seq.note_feedback(item.clone());
    }
    assert!(seq.has_pending_feedback());
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
    let (stats, produced) = apply(&mut diff, &reply2, &fresh, &mut native);
    // The arrival receipt settled the transfer: w1 rides homed at the
    // target with no retained transfer, while w2 converges to its grown
    // post-departure slot with one frame write. Nothing outstanding means
    // no re-proof and no fabricated feedback.
    assert!(reply2.transfers.iter().all(|t| t.operation_id != op));
    assert_eq!(stats.membership_writes, 0);
    assert_eq!(stats.view_writes, 0);
    assert_eq!(stats.geometry_writes, 1);
    assert!(!stats.stopped_mid_apply);
    assert!(
        produced.iter().all(|f| f.operation_id != op),
        "settled flight re-proves nothing, got {produced:?}"
    );
    assert!(!stats.partial);
    assert_eq!(fingerprint(&rig.state), before);
}

#[test]
fn feedback_exact_equality_and_catchup_fencing() {
    let rig = rig();
    let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
    let v1 = AttemptFeedback {
        operation_id: "op-a".to_owned(),
        kind: AttemptKind::Transfer,
        outcome: AttemptOutcome::Applied,
        hold: None,
    };
    seq.note_feedback(v1.clone());
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
    assert!(report.feedback.iter().any(|f| f.operation_id == "op-a"));
    // A newer update for the same operation replaces the flight value: the
    // predecessor ack must not consume it.
    let v2 = AttemptFeedback {
        operation_id: "op-a".to_owned(),
        kind: AttemptKind::Transfer,
        outcome: AttemptOutcome::PossiblyExecuted,
        hold: None,
    };
    seq.note_feedback(v2.clone());
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply) {
        AckOutcome::Applied(receipt) => {
            assert!(receipt.feedback_acked.is_empty());
        }
        other => panic!("expected applied, got {other:?}"),
    }
    assert!(seq.has_pending_feedback());
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
        .expect("feedback redispatch");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&reply) {
        AckOutcome::Applied(receipt) => {
            assert!(receipt.feedback_acked.contains(&"op-a".to_owned()));
        }
        other => panic!("expected applied, got {other:?}"),
    }
    assert!(!seq.has_pending_feedback());

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
