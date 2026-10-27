//! World route smoke test (offline only): the test-only Windows route
//! captures source-first observations, envelopes a native-free report,
//! and sequences it through the core engine. Receipts echo proven state
//! only (never desire): a fresh bootstrap proves nothing, a proven view
//! echoes, and a commanded-but-unapplied target flight persists. The
//! offline facade actuates the desired snapshot against fresh proof.
//! No HWNDs, hooks, or live effects.

use super::observe::OwnerWaitCounts;
use super::owner_work::OwnerWorkGuard;
use super::pending_seams::{
    clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
    inject_pending_observe, inject_pending_suspend,
};
use super::rmax03_adapter_tests::test_state;
use super::run_loop_tick::{TickFlow, run_active_tick};
use super::world_capture::capture_world;
use super::world_diff::{FakeNative, FreshHost, HostDiff, NativeOps, apply_world_reply};
use super::world_sequencer::{AckOutcome, WorldSequencer};
use super::{MonitorArea, ObservedTarget, ObservedWindow, RetainedRow};
use crate::snapkey::{QueuedWorkspaceIntent, SnapEdge, WorkspaceOp};
use tiler_core::geometry::Rect;
use tiler_core::world_engine::WorldOutcome;

fn area() -> MonitorArea {
    MonitorArea {
        device: "DEV1".to_owned(),
        work: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        },
        full: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
    }
}

fn plain_facts() -> crate::tiling::WindowFacts {
    crate::tiling::WindowFacts {
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

fn observed_window() -> ObservedWindow {
    let visible = Rect {
        x: 0,
        y: 0,
        w: 200,
        h: 200,
    };
    let outer = Rect {
        x: -8,
        y: -8,
        w: 216,
        h: 216,
    };
    ObservedWindow {
        hwnd: 101,
        token: "w1".to_owned(),
        outer,
        visible,
        insets: crate::tiling::FrameInsets::measure(outer, visible),
        identity: ObservedTarget {
            hwnd: 101,
            pid: 202,
            process_creation: "proc-1".to_owned(),
            exe_path: "C:\\Apps\\app.exe".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            tag: "tag-1".to_owned(),
        },
        facts: plain_facts(),
        member_tag: Some("tag-1".to_owned()),
    }
}

#[test]
fn native_free_route_captures_sequences_actuates_and_echoes_views() {
    let mut state = test_state();
    state.ensure_device("DEV1");
    let key = crate::native_guards::WindowKey {
        hwnd: 101,
        pid: 202,
        creation: "proc-1".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "tag-1".to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: 202,
            process_creation: "proc-1".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    let observed = vec![observed_window()];
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    // Discovery hands out generation-bound bootstrap with no desired state.
    let bootstrap = seq.bootstrap_reply("req-0");
    assert!(!bootstrap.ready);
    assert!(bootstrap.workspaces.is_empty());
    assert_eq!(seq.adapter_incarnation(), "adapter-1");
    assert_eq!(seq.core_generation(), "gen-1");
    assert_eq!(seq.current_obs(), 0);
    assert_eq!(seq.next_request_seq(), 1);
    assert_eq!(seq.next_command_seq(), 1);
    assert!(!seq.has_flight());
    assert!(!seq.in_catchup());
    assert!(!seq.is_ready());

    // First dispatch: the visible member proves the active view, so the
    // report carries a truthful view receipt (never echoed desire). The
    // core bootstraps logical state from the live screens.
    let out_token = state.output_token("DEV1");
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            None,
        )
        .expect("dispatch");
    // Fresh bootstrap proves nothing yet: no view receipts (nothing
    // applied, so nothing is proven).
    assert!(report.views.is_empty(), "no receipts before proof");
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].window.0, "w1");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(reply.ready);
    assert!(!reply.workspaces.is_empty());
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    assert!(seq.is_ready());
    assert!(seq.last_desired().is_some());
    assert_eq!(seq.last_accepted_rev(), reply.state_revision);
    assert_eq!(seq.last_accepted_obs(), 0);
    assert_eq!(seq.last_accepted_req(), "req-1");
    assert_eq!(seq.flight_command_seq(), None);

    // Second dispatch still proves nothing: visibility alone is never a
    // receipt, so the report stays receipt-free instead of echoing desire.
    seq.invalidate();
    let second = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            None,
        )
        .expect("dispatch");
    assert_eq!(
        second.views, report.views,
        "no receipt before proof, got {:?}",
        second.views
    );
    let geometry_reply = match seq.deliver(&second) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match seq.ack(&geometry_reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }

    // A desired view never settles the retained flight without observed
    // application: command a selection of an INACTIVE core-minted
    // workspace, ack the desire, then prove the next report (still
    // receipt-free) keeps the flight with no replay. Nothing here echoes
    // desire: the receipts stay empty because no native arrival proved.
    let current = geometry_reply
        .outputs
        .iter()
        .find(|o| o.output.0 == state.output_token("DEV1"))
        .and_then(|o| o.logical_view.clone())
        .expect("current view");
    let target = geometry_reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == state.output_token("DEV1") && row.id != current)
        .map(|row| row.id.clone())
        .expect("second logical workspace");
    seq.invalidate();
    let commanded = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            Some(tiler_core::world_report::WorldCommand::SelectWorkspace {
                output: tiler_core::directional::OutputId(state.output_token("DEV1")),
                workspace: target.clone(),
            }),
        )
        .expect("command dispatch");
    // The command report still proves the actual (old) view, never the
    // desired one.
    assert_eq!(
        commanded.views, report.views,
        "actual view proven, not desire, got {:?}",
        commanded.views
    );
    let reply = match seq.deliver(&commanded) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        reply.transfers.iter().any(|t| t.target_workspace == target),
        "view flight retained, got {:?}",
        reply.transfers
    );
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    // The command report still proves nothing new (no native arrival
    // happened), never the desired target: the flight persists with no
    // re-actuation.
    seq.invalidate();
    let pending = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            None,
        )
        .expect("dispatch");
    assert_eq!(
        pending.views, report.views,
        "old view proven, target never echoed, got {:?}",
        pending.views
    );
    let reply = match seq.deliver(&pending) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        reply.transfers.iter().any(|t| t.target_workspace == target),
        "flight held without readback, got {:?}",
        reply.transfers
    );
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    // The proven old view (never the desired target) never replays the
    // flight and never settles it: the offline facade performs no view
    // actuation here (live view actuation is the production `LiveNative`
    // path, not invented here).
    seq.invalidate();
    let unrelated = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            None,
        )
        .expect("dispatch");
    let reply = match seq.deliver(&unrelated) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        reply.transfers.iter().any(|t| t.target_workspace == target),
        "missing proof holds flight, got {:?}",
        reply.transfers
    );
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }

    // The flight converges when the view actually actuates: apply the
    // select reply through the facade (the offline stand-in for the
    // production `LiveNative` pass), plant the view receipt that
    // actuation proves, and the next report echoes the new view while the
    // core settles the flight. Nothing here is desire: the facade really
    // switched the offline views map, and the planted receipt mirrors the
    // read-back proof `LiveNative` would have recorded (covered genuinely
    // by the world_native receipt tests).
    let captured = capture_world(&state.inputs(), Some(&observed), &retained, &areas, 0);
    let fresh = FreshHost::from_sources(&state.inputs(), &observed, &retained, &captured);
    let mut vnative = FakeNative::seeded(&fresh);
    let mut vdiff = HostDiff::new("adapter-1", "gen-1");
    vdiff.gate.current_obs = unrelated.observation_seq;
    let mut vmembers = Vec::new();
    let mut vviews = Vec::new();
    let vstats = apply_world_reply(
        &mut vdiff,
        &reply,
        &fresh,
        &mut vnative,
        &mut Vec::new(),
        &mut vmembers,
        &mut vviews,
        &mut Vec::new(),
        0,
    );
    assert_eq!(vstats.view_writes, 1, "stats: {vstats:?}");
    let view_op = reply
        .transfers
        .iter()
        .find(|t| t.target_workspace == target)
        .map(|t| t.operation_id.clone())
        .expect("view flight");
    assert!(
        vviews
            .iter()
            .any(|a| a.operation_id == view_op && a.post.as_deref() == Some(target.as_str())),
        "view actuation proves: {vviews:?}"
    );
    vdiff.gate.note_applied(&reply);
    state
        .applied_views
        .insert(out_token.clone(), target.clone());
    seq.invalidate();
    let echoed = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            None,
        )
        .expect("dispatch");
    assert_eq!(
        echoed.views.len(),
        1,
        "proven view echoes, got {:?}",
        echoed.views
    );
    assert_eq!(echoed.views[0].output.0, out_token);
    assert_eq!(echoed.views[0].view, target);
    let reply = match seq.deliver(&echoed) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        reply.transfers.iter().all(|t| t.target_workspace != target),
        "proven arrival settles the flight, got {:?}",
        reply.transfers
    );
    match seq.ack(&reply) {
        AckOutcome::Applied(_) => {}
        other => panic!("expected applied, got {other:?}"),
    }
    // Nothing outstanding: no dirty work remains.
    let captured = capture_world(&state.inputs(), Some(&observed), &retained, &areas, 0);
    assert!(seq.outstanding_closes(&captured).is_empty());
    assert!(!seq.is_dirty(&captured));
    // The offline facade actuates desired geometry against fresh proof.
    // View flights are not facade-actuated here: the geometry reply below
    // predates the selection command, so the selection flight stays
    // outstanding through the facade pass (no readback, no replay).
    let fresh = FreshHost::from_sources(&state.inputs(), &observed, &retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = second.observation_seq;
    let stats = apply_world_reply(
        &mut diff,
        &geometry_reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert_eq!(stats.held, 0, "stats: {stats:?}");
    assert!(stats.geometry_writes > 0, "stats: {stats:?}");
    diff.gate.note_applied(&geometry_reply);
    // A racing native invalidation stops the pass before any setter.
    native.pending_invalidation = true;
    let stats = apply_world_reply(
        &mut diff,
        &geometry_reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        0,
    );
    assert!(stats.stopped_mid_apply, "stats: {stats:?}");
    assert_eq!(stats.geometry_writes, 0, "stats: {stats:?}");
    // Raw attempts stage for the next report; a lost flight stays dirty.
    seq.note_frame_attempt(tiler_core::world_report::FrameAttempt {
        window: tiler_core::directional::WindowId("w1".to_owned()),
        target: Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        setter: tiler_core::world_report::FrameSetter::Accepted,
        readback: None,
        dpi_before: None,
        dpi_after: None,
        attempt_ms: 7,
        operation_id: None,
        prewrite: None,
    });
    assert_eq!(seq.pending_frames_len(), 1);
    seq.invalidate();
    let third = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            None,
        )
        .expect("dispatch");
    assert!(seq.has_flight());
    assert!(seq.note_rejected_or_lost("gen-1", &third.request_id));
    assert!(!seq.has_flight());
    assert!(seq.in_catchup());
    assert!(seq.in_catchup());
    // Close receipts prune only exact delivered pairs.
    let mut pending = std::collections::BTreeMap::new();
    let homing = crate::tiling_sys::retained::PendingClosed {
        output: "DEV1".to_owned(),
        workspace: current.clone(),
    };
    pending.insert("w9".to_owned(), homing.clone());
    let ack_receipt = super::world_sequencer::AckReceipt {
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-x".to_owned(),
        closed_acked: vec![("w9".to_owned(), homing)],
        frames_acked: Vec::new(),
        members_acked: Vec::new(),
        views_acked: Vec::new(),
        clears_acked: Vec::new(),
    };
    let pruned = seq.apply_close_receipt(&mut pending, &ack_receipt);
    assert_eq!(pruned, vec!["w9".to_owned()]);
    assert!(pending.is_empty());
    let _ = (&mut native) as &mut dyn NativeOps;
}

fn route_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 1,
        process_creation: "owner-1".to_owned(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: "C:\\Owner\\owner.exe".to_owned(),
    }
}

fn route_dirs(
    name: &str,
) -> (
    std::path::PathBuf,
    crate::storage::LedgerStore,
    Vec<MonitorArea>,
    Vec<Rect>,
) {
    let dir = std::env::temp_dir().join(format!(
        "omnitiler-world-route-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = crate::storage::LedgerStore::open(&dir).expect("ledger temp dir");
    let areas = vec![area()];
    let fulls = vec![Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }];
    (dir, store, areas, fulls)
}

#[test]
fn production_observation_tick_proves_nothing_on_unreadable_natives() {
    // Actual production tick (`run_observation_tick`) with synthetic
    // members on fake HWNDs: admission's real native gates fail closed
    // (no bindings possible on numbers no window owns), capture still
    // facts the rows, the core still converges and homes them, but every
    // `LiveNative` read-back fails so the applied receipt maps stay empty.
    // No crash, no fabrication: the reply converges while proving nothing.
    let mut state = test_state();
    state.ensure_device("DEV1");
    let key = crate::native_guards::WindowKey {
        hwnd: 101,
        pid: 202,
        creation: "proc-1".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "tag-1".to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: 202,
            process_creation: "proc-1".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    let observed = vec![observed_window()];
    let (dir, store, areas, fulls) = route_dirs("obs-tick");
    let reply = super::world_tick::run_observation_tick(
        &mut state,
        &route_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
        false,
    )
    .expect("tick converges");
    assert!(reply.ready);
    assert!(
        state.applied_homing.is_empty(),
        "unreadable natives prove no homing: {:?}",
        state.applied_homing
    );
    assert!(
        state.applied_views.is_empty(),
        "unreadable natives prove no view: {:?}",
        state.applied_views
    );
    assert!(
        state.member_home(&key).is_some(),
        "core still homes the member"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn production_tick_captures_paired_proof_under_trace_only() {
    // Blocker shape at tick level: a quiet accept (no writes, no hover)
    // stays silent on the ordinary world log, yet captures its exact
    // report-bound proof row under trace; trace-off captures nothing.
    fn seeded_trace_state(trace: bool) -> super::TileLoop {
        let mut state = test_state();
        state.trace = trace;
        state.ensure_device("DEV1");
        let key = crate::native_guards::WindowKey {
            hwnd: 101,
            pid: 202,
            creation: "proc-1".to_owned(),
        };
        state.member_tokens.insert(key.clone(), "w1".to_owned());
        state.member_tags.insert(key.clone(), "tag-1".to_owned());
        state.member_identity.insert(
            key.clone(),
            crate::model::ProcessIdentity {
                pid: 202,
                process_creation: "proc-1".to_owned(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Apps\\app.exe".to_owned(),
            },
        );
        state
    }
    let mut state = seeded_trace_state(true);
    let observed = vec![observed_window()];
    let (dir, store, areas, fulls) = route_dirs("trace-proof");
    let me = route_me();
    let first = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
        false,
    )
    .expect("seed tick converges");
    let proof = state.world.trace_proof().expect("proof captured").clone();
    assert_eq!(proof["event"], serde_json::json!("world-apply"));
    assert_eq!(proof["revision"], serde_json::json!(first.state_revision));
    assert_eq!(
        proof["observation"],
        serde_json::json!(first.based_on_observation_seq)
    );
    assert_eq!(
        proof["request"],
        serde_json::json!(first.request_id.clone())
    );
    assert_eq!(proof["geometry_status"], serde_json::json!("paired"));
    let _ = std::fs::remove_dir_all(&dir);
    // Trace-off captures nothing, even on a fresh accept.
    let mut dark = seeded_trace_state(false);
    let (dir2, store2, areas2, fulls2) = route_dirs("trace-proof-dark");
    let _ = super::world_tick::run_observation_tick(
        &mut dark,
        &me,
        &store2,
        &dir2,
        &observed,
        &[],
        &areas2,
        0,
        &fulls2,
        None,
        false,
    )
    .expect("dark tick converges");
    assert!(dark.world.trace_proof().is_none());
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn production_select_route_reports_honest_outcome_without_proof() {
    // Actual production select route (`workspace_do_select`) through the
    // world tick with synthetic members on fake HWNDs: the command
    // dispatches, delivers, and applies against real (fail-closed)
    // natives; the view never actuates (every hide leg fails identity
    // proof), so no receipt proves and the effect stays honest (`ok`
    // means sequencer-accepted and applied, never a claim of native
    // arrival). The unproven flight stays retained in the core.
    let mut state = test_state();
    state.ensure_device("DEV1");
    let key = crate::native_guards::WindowKey {
        hwnd: 101,
        pid: 202,
        creation: "proc-1".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "tag-1".to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: 202,
            process_creation: "proc-1".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    let observed = vec![observed_window()];
    let (dir, store, areas, fulls) = route_dirs("select-route");
    let me = route_me();
    let first = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
        false,
    )
    .expect("seed tick converges");
    assert!(first.ready);
    // Logical authority: the target is a core-minted id from the live
    // engine ring, never an adapter-invented name.
    let out = state.output_token("DEV1");
    let current = state
        .world
        .engine()
        .logical_view(&out)
        .expect("current view");
    let target = state
        .world
        .engine()
        .logical_ring(&out)
        .into_iter()
        .find(|id| id != &current)
        .expect("second workspace");
    let mut observed = vec![observed_window()];
    let ctx = super::ActionCtx {
        correlation: "route-test-select".to_owned(),
        tick: 0,
        queued_at: std::time::Instant::now(),
        start: std::time::Instant::now(),
    };
    let effect = super::workspace_do_select(
        &mut state,
        &me,
        &store,
        &dir,
        &fulls,
        &areas,
        &mut observed,
        "DEV1",
        &target,
        &ctx,
        None,
        false,
        false,
    );
    assert_eq!(effect.outcome, "ok", "effect: {:?}", effect.outcome);
    assert_eq!(effect.target_workspace, target);
    assert!(
        state.applied_views.is_empty(),
        "unactuated view proves nothing: {:?}",
        state.applied_views
    );
    let retained = state
        .world
        .last_reply()
        .expect("applied reply")
        .transfers
        .iter()
        .any(|t| t.target_workspace == target);
    assert!(retained, "unproven flight stays retained");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn production_send_route_reports_honest_outcome_without_proof() {
    // Actual production send route (`run_world_command` with `Send`)
    // through the world tick: the command delivers against the
    // core-minted ring, the unactuated transfer retains, and the reply
    // converges with no receipt proving native arrival (fake-HWND natives
    // fail every read-back). The `workspace_do_send` wrapper only adds the
    // ambient foreground read plus numbered-slot resolution, covered by the
    // CLI parse tests and core resolution tests.
    let mut state = test_state();
    state.ensure_device("DEV1");
    let key = crate::native_guards::WindowKey {
        hwnd: 101,
        pid: 202,
        creation: "proc-1".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "tag-1".to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: 202,
            process_creation: "proc-1".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    let observed = vec![observed_window()];
    let (dir, store, areas, fulls) = route_dirs("send-route");
    let me = route_me();
    let first = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
        false,
    )
    .expect("seed tick converges");
    assert!(first.ready);
    // Target a core-minted workspace id different from the current view
    // (logical authority, never an adapter-invented name).
    let out = state.output_token("DEV1");
    let current = first
        .outputs
        .iter()
        .find(|o| o.output.0 == out)
        .and_then(|o| o.logical_view.clone())
        .expect("current view");
    let target = state
        .world
        .engine()
        .logical_ring(&out)
        .into_iter()
        .find(|id| id != &current)
        .expect("second workspace");
    // Production command path with a controlled foreground: the mover is
    // the focused window, so the core admits the send (the `workspace_do_send`
    // wrapper only adds the ambient foreground read plus numbered-slot
    // resolution, covered by the CLI parse tests and core resolution tests).
    let observed = vec![observed_window()];
    let reply = super::world_tick::run_world_command(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        101,
        &fulls,
        tiler_core::world_report::WorldCommand::Send {
            window: tiler_core::directional::WindowId("w1".to_owned()),
            target_output: tiler_core::directional::OutputId(out.clone()),
            target_workspace: target.clone(),
            follow: false,
        },
        None,
    );
    let reply = reply.expect("send converges");
    assert!(reply.ready);
    assert!(
        state.applied_homing.is_empty(),
        "unactuated send proves no homing: {:?}",
        state.applied_homing
    );
    assert!(
        reply.transfers.iter().any(
            |t| matches!(&t.kind, tiler_core::world_report::TransferKind::Window(w) if w.0 == "w1")
                && t.target_workspace == target
        ),
        "unproven send flight stays retained, got {:?}",
        reply.transfers
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn arrival_reply(
    out: &str,
    rows: Vec<tiler_core::world_report::WindowDesire>,
) -> tiler_core::world_report::WorldReply {
    use tiler_core::world_report::{ScopeAuthority as Auth, ScopeStatus};
    tiler_core::world_report::WorldReply {
        contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-9".to_owned(),
        based_on_observation_seq: 9,
        state_revision: 9,
        ready: true,
        command_consumed: false,
        scopes: vec![ScopeStatus {
            scope: format!("{out}/ws1"),
            authority: Auth::Controlled,
        }],
        workspaces: Vec::new(),
        outputs: Vec::new(),
        windows: rows,
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

fn arrival_row(
    out: &str,
    ws: &str,
    token: &str,
    allocation: Option<Rect>,
    effective: Option<Rect>,
    frame_auth: tiler_core::world_report::ScopeAuthority,
    floating: bool,
) -> tiler_core::world_report::WindowDesire {
    tiler_core::world_report::WindowDesire {
        lifetime: tiler_core::directional::WindowId(token.to_owned()),
        output: tiler_core::directional::OutputId(out.to_owned()),
        workspace: ws.to_owned(),
        allocation,
        effective_frame: effective,
        frame_authority: frame_auth,
        membership_authority: tiler_core::world_report::ScopeAuthority::Controlled,
        floating,
        float_origin: None,
        frame_operation: None,
        clear_operation: None,
    }
}

#[test]
fn drop_arrival_proves_frameless_floats_only_on_matching_receipts() {
    // Membership-only float lanes preserve their physical frame by design,
    // so arrival settles on the proven applied-homing receipt alone. No
    // live window is needed; a mismatched or absent receipt never proves.
    use tiler_core::world_report::ScopeAuthority as Auth;
    let mut state = test_state();
    state.ensure_device("DEV1");
    let out = state.output_token("DEV1");
    let me = route_me();
    let fulls = vec![Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }];
    let areas = vec![area()];
    let reply = arrival_reply(
        &out,
        vec![arrival_row(
            &out,
            "ws1",
            "w1",
            None,
            None,
            Auth::Untouched,
            true,
        )],
    );
    let arrived = |state: &mut super::TileLoop, expect: Option<&str>| {
        super::drop_arrival_verified(state, &me, &fulls, &areas, &reply, "w1", 9999, expect)
    };
    assert!(!arrived(&mut state, None), "no receipt proves nothing");
    state
        .applied_homing
        .insert("w1".to_owned(), (out.clone(), "ws1".to_owned()));
    assert!(arrived(&mut state, None), "matching receipt proves");
    assert!(
        !arrived(&mut state, Some("OTHER")),
        "expected-device mismatch never proves"
    );
    state
        .applied_homing
        .insert("w1".to_owned(), (out.clone(), "ws2".to_owned()));
    assert!(!arrived(&mut state, None), "wrong workspace never proves");
    state.applied_homing.remove("w1");
    assert!(!arrived(&mut state, None), "removed receipt proves nothing");
    assert!(
        !super::drop_arrival_verified(&mut state, &me, &fulls, &areas, &reply, "w9", 9999, None),
        "unknown mover never proves"
    );
}

#[test]
fn drop_arrival_never_proves_from_allocation() {
    // The canonical slot is never the write target and never arrival
    // proof: only the published effective frame (tiled rows) or a proven
    // receipt (frameless lanes) settles. Fake-HWND natives fail every
    // fresh read-back, so framed rows hold offline.
    use tiler_core::world_report::ScopeAuthority as Auth;
    let mut state = test_state();
    state.ensure_device("DEV1");
    let out = state.output_token("DEV1");
    let me = route_me();
    let fulls = vec![Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }];
    let areas = vec![area()];
    let slot = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 560,
    };
    // Fullscreen-style row: the slot is known but there is no frame
    // target and no frame authority. The slot alone never proves.
    let reply = arrival_reply(
        &out,
        vec![arrival_row(
            &out,
            "ws1",
            "w1",
            Some(slot),
            None,
            Auth::Untouched,
            false,
        )],
    );
    assert!(
        !super::drop_arrival_verified(&mut state, &me, &fulls, &areas, &reply, "w1", 9999, None),
        "allocation without a frame target never proves"
    );
    // Tiled row with a published target but no live window: the fresh
    // read-back fails closed instead of echoing the target.
    let reply = arrival_reply(
        &out,
        vec![arrival_row(
            &out,
            "ws1",
            "w1",
            Some(slot),
            Some(slot),
            Auth::Controlled,
            false,
        )],
    );
    assert!(
        !super::drop_arrival_verified(&mut state, &me, &fulls, &areas, &reply, "w1", 9999, None),
        "unreadable live frame never proves"
    );
    // Row homed on an unknown wire fails closed even with a receipt.
    let reply = arrival_reply(
        "o9",
        vec![arrival_row(
            "o9",
            "ws1",
            "w1",
            None,
            None,
            Auth::Untouched,
            true,
        )],
    );
    state
        .applied_homing
        .insert("w1".to_owned(), ("o9".to_owned(), "ws1".to_owned()));
    assert!(
        !super::drop_arrival_verified(&mut state, &me, &fulls, &areas, &reply, "w1", 9999, None),
        "unknown wire never proves"
    );
}

#[test]
fn busy_command_reports_unverified_not_cached_reply() {
    // A command that never dispatches (single-flight busy) must not
    // report the previous accepted reply as its outcome: the cached
    // reply acknowledges older work, never this command.
    let mut state = test_state();
    state.ensure_device("DEV1");
    let key = crate::native_guards::WindowKey {
        hwnd: 101,
        pid: 202,
        creation: "proc-1".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "tag-1".to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: 202,
            process_creation: "proc-1".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    let observed = vec![observed_window()];
    let (dir, store, areas, fulls) = route_dirs("busy-cmd");
    let me = route_me();
    super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
        false,
    )
    .expect("seed converges");
    assert!(
        state.world.last_reply().is_some(),
        "seed accepted a cached reply"
    );
    // Open a flight without acknowledging it: the next command dispatch
    // stays busy and the command never runs.
    {
        let super::TileLoop {
            ref mut world,
            ref member_tokens,
            ref member_identity,
            ref member_tags,
            ref member_rects,
            ref float_rects,
            ref hint_logged,
            ref sticky,
            ref floated,
            ref restart_reject_hold,
            ref born_fullscreen,
            ref born_first_exit,
            ref fixed_auto_seen,
            ref fixed_tile_overrides,
            ref fixed_origin_overrides,
            ref hidden_claims,
            ref minimized_omitted,
            ref pending_closed,
            ref scope,
            ref settings_live,
            ref output_tokens,
            ref active_output,
            ref applied_homing,
            ref applied_views,
            ref inner_gap,
            ref outer_gap,
            ref active,
            ..
        } = state;
        world.invalidate();
        let inputs = super::TileLoop::owner_inputs(
            member_tokens,
            member_identity,
            member_tags,
            member_rects,
            float_rects,
            hint_logged,
            sticky,
            floated,
            restart_reject_hold,
            born_fullscreen,
            born_first_exit,
            fixed_auto_seen,
            fixed_tile_overrides,
            fixed_origin_overrides,
            hidden_claims,
            minimized_omitted,
            pending_closed,
            scope,
            settings_live,
            output_tokens,
            active_output.as_str(),
            applied_homing,
            applied_views,
            *inner_gap,
            *outer_gap,
            active,
            None,
            None,
            None,
            None,
            false,
            String::new(),
            false,
            false,
            (0, 0, 0),
            false,
            Vec::new(),
        );
        world
            .try_dispatch_observation(&inputs, Some(&observed), &[], &areas, 0, 0)
            .expect("flight opens");
    }
    let out = state.output_token("DEV1");
    let reply = super::world_tick::run_world_command(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        tiler_core::world_report::WorldCommand::SelectTrailing {
            output: Some(tiler_core::directional::OutputId(out)),
        },
        None,
    );
    assert!(
        reply.is_none(),
        "busy command must not replay the cached reply, got {reply:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejected_command_catchup_never_strands_the_next_command() {
    use tiler_core::directional::WindowId;
    use tiler_core::world_report::{EffectKind, WorldCommand, WorldReply};
    // Production-shaped catchup-starvation probe through the real
    // owner/command/scheduling path: three tiled windows bootstrap, valid
    // Left and Up from the bottom-right origin dispatch, an edge Right
    // core-rejects (arming observation-only catch-up), then Left and Up
    // from that same origin must dispatch with no unrelated WinEvent
    // between them. The rejected command is never queued or replayed.
    // Host-shaped identity: 16-hex-digit FILETIME creation stamps, and one
    // owner-loop tick per command turn like `dispatch_directional_intent`.
    // Fake handles never actually take focus, so every command names the
    // same origin foreground.
    fn observed_at(hwnd: u64, token: &str, creation: &str, x: i32) -> ObservedWindow {
        let visible = Rect {
            x,
            y: 100,
            w: 200,
            h: 200,
        };
        let outer = Rect {
            x: x - 8,
            y: 92,
            w: 216,
            h: 216,
        };
        ObservedWindow {
            hwnd,
            token: token.to_owned(),
            outer,
            visible,
            insets: crate::tiling::FrameInsets::measure(outer, visible),
            identity: ObservedTarget {
                hwnd,
                pid: 200 + hwnd as u32,
                process_creation: creation.to_owned(),
                exe_path: "C:\\Apps\\app.exe".to_owned(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                tag: format!("tag-{token}"),
            },
            facts: plain_facts(),
            member_tag: Some(format!("tag-{token}")),
        }
    }
    /// Exactly one fresh `fx-` focus effect with the expected target;
    /// returns its id for the uniqueness proof below.
    fn fresh_focus(reply: &WorldReply, want: &str) -> String {
        let mut hits = reply
            .effects
            .iter()
            .filter(|effect| effect.kind == EffectKind::Focus && effect.id.starts_with("fx-"));
        let hit = hits.next().expect("one fresh focus effect");
        assert!(
            hits.next().is_none(),
            "exactly one fx- focus effect, got {:?}",
            reply.effects
        );
        assert_eq!(
            hit.target,
            WindowId(want.to_owned()),
            "correct focus target"
        );
        hit.id.clone()
    }
    let mut state = test_state();
    state.ensure_device("DEV1");
    for (hwnd, token, creation) in [
        (101, "w1", "01dc9a1b2c3d4e01"),
        (102, "w2", "01dc9a1b2c3d4e02"),
        (103, "w3", "01dc9a1b2c3d4e03"),
    ] {
        let key = crate::native_guards::WindowKey {
            hwnd,
            pid: 200 + hwnd as u32,
            creation: creation.to_owned(),
        };
        state.member_tokens.insert(key.clone(), token.to_owned());
        state
            .member_tags
            .insert(key.clone(), format!("tag-{token}"));
        state.member_identity.insert(
            key,
            crate::model::ProcessIdentity {
                pid: 200 + hwnd as u32,
                process_creation: creation.to_owned(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Apps\\app.exe".to_owned(),
            },
        );
    }
    let observed = vec![
        observed_at(101, "w1", "01dc9a1b2c3d4e01", 0),
        observed_at(102, "w2", "01dc9a1b2c3d4e02", 270),
        observed_at(103, "w3", "01dc9a1b2c3d4e03", 540),
    ];
    let (dir, store, areas, fulls) = route_dirs("catchup-starve");
    let me = route_me();
    state.tick += 1;
    super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        101,
        &fulls,
        None,
        false,
    )
    .expect("seed converges");
    let focus = |window: &str, direction: &str| WorldCommand::FocusDirectional {
        window: WindowId(window.to_owned()),
        direction: direction.to_owned(),
    };
    let command = |state: &mut super::TileLoop, window: &str, direction: &str| {
        // One owner-loop turn per command, like `dispatch_directional_intent`.
        state.tick += 1;
        super::world_tick::run_world_command(
            state,
            &me,
            &store,
            &dir,
            &observed,
            &[],
            &areas,
            101,
            &fulls,
            focus(window, direction),
            None,
        )
    };
    // Valid Left and Up from the bottom-right origin: w1 reaches w2 to the
    // left and w3 above (core layout tiles w2 full-height left, w3 top-right).
    let left_id = fresh_focus(
        &command(&mut state, "w1", "left").expect("left dispatches"),
        "w2",
    );
    let up_id = fresh_focus(
        &command(&mut state, "w1", "up").expect("up dispatches"),
        "w3",
    );
    // No neighbor: w1 focuses right off the edge. The core rejects
    // unchanged, which arms observation-only catch-up.
    assert!(
        command(&mut state, "w1", "right").is_none(),
        "edge right must core-reject"
    );
    // Same origin, no unrelated WinEvent between: the owed catch-up must
    // have run, so both dispatch again with their own fresh effects.
    let left_again_id = fresh_focus(
        &command(&mut state, "w1", "left").expect("owed catch-up serves left"),
        "w2",
    );
    let up_again_id = fresh_focus(
        &command(&mut state, "w1", "up").expect("owed catch-up serves up"),
        "w3",
    );
    let mut ids = vec![left_id, up_id, left_again_id, up_again_id];
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        4,
        "unique fresh effect per valid command, never a replay"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Drive one command-lane turn through the real scheduler with the given
/// workspace batch. Returns the tick delta. The injected observe fixture
/// makes the owed reconcile pass a tick-only no-op in tests; the key-up
/// and poll lanes never enumerate.
#[allow(clippy::too_many_arguments)]
fn drive_command_lane(
    state: &mut super::TileLoop,
    me: &crate::model::ProcessIdentity,
    dir: &std::path::Path,
    store: &crate::storage::LedgerStore,
    areas: &[MonitorArea],
    batch: Vec<QueuedWorkspaceIntent>,
    hook_drain_woke: bool,
) -> u64 {
    let before = state.tick;
    let mut monitor_areas = areas.to_vec();
    let mut monitor = monitor_areas[0].clone();
    let mut monitor_count = monitor_areas.len();
    let log_path = state.log_path.clone();
    let mut owner_work = OwnerWorkGuard::begin(false, &log_path, 0, before, None);
    let ow_counts = OwnerWaitCounts::default();
    let mut snap_primed = false;
    let flow = run_active_tick(
        state,
        me,
        dir,
        store,
        &mut monitor_areas,
        &mut monitor,
        &mut monitor_count,
        std::time::Instant::now(),
        false,
        false,
        true,
        hook_drain_woke,
        &log_path,
        &mut owner_work,
        &ow_counts,
        false,
        &mut snap_primed,
        &mut None,
        0,
        0,
        0,
        0,
        Vec::new(),
        batch,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &mut None,
    )
    .expect("active turn completes");
    assert!(
        matches!(flow, TickFlow::Done),
        "command lane runs to completion"
    );
    state.tick.saturating_sub(before)
}

fn keyup_intent() -> QueuedWorkspaceIntent {
    // Win+1 key-up: consumed by the hook, never announced. Settles with
    // no world pass, exactly like the incident's coalesced turn.
    QueuedWorkspaceIntent {
        op: WorkspaceOp::Select,
        index: 1,
        follow: false,
        edge: SnapEdge::Up,
        origin: None,
        consumed: true,
        announce: false,
        tick: std::time::Instant::now(),
    }
}

#[test]
fn keyup_only_turn_owes_observation_for_pending_foreground() {
    // Incident shape: the select tick's accepted setter arms in-flight
    // focus; the own foreground edge drains on the next turn coalesced
    // with a Win+1 key-up. The key-up settles with no world pass, so the
    // drained edge sits in the owner with no further wake owed. The turn
    // owes one observation-only pass instead of stranding the flight
    // until an unrelated wake (the pager message in the incident).
    // Pairing: this proves the pass is owed (delta 1); the resolution it
    // buys is pinned at the diff seam by
    // `keyup_owed_pass_resolves_armed_flight_as_own_landing`. The injected
    // reconcile short-circuit records the owed pass as a tick only.
    let mut state = test_state();
    state.ensure_device("DEV1");
    state.keyboard.takeover = true;
    let (dir, store, areas, _) = route_dirs("keyup-owed");
    let me = route_me();
    state.world.push_foreground_drain(vec![101], Vec::new(), 0);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_observe(Vec::new(), Vec::new(), false);
    let delta = drive_command_lane(
        &mut state,
        &me,
        &dir,
        &store,
        &areas,
        vec![keyup_intent()],
        false,
    );
    assert_eq!(
        delta, 1,
        "key-up-only turn with pending foreground owes one observation pass"
    );
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keyup_only_turn_without_pending_foreground_stays_quiet() {
    // No drained edges, no owed pass: the same settled-only batch must
    // not spend a pointless observation.
    let mut state = test_state();
    state.ensure_device("DEV1");
    state.keyboard.takeover = true;
    let (dir, store, areas, _) = route_dirs("keyup-quiet");
    let me = route_me();
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_observe(Vec::new(), Vec::new(), false);
    let delta = drive_command_lane(
        &mut state,
        &me,
        &dir,
        &store,
        &areas,
        vec![keyup_intent()],
        false,
    );
    assert_eq!(
        delta, 0,
        "settled-only turn with no pending foreground observes nothing"
    );
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn real_command_turn_consumes_pending_without_extra_pass() {
    // A real select turn dispatches through the owner and consumes the
    // drained edge at apply entry, so no owed pass follows: exactly one
    // tick (the command's own), never a redundant observation.
    let mut state = test_state();
    state.ensure_device("DEV1");
    state.keyboard.takeover = true;
    let key = crate::native_guards::WindowKey {
        hwnd: 101,
        pid: 202,
        creation: "proc-1".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "tag-1".to_owned());
    state.member_identity.insert(
        key,
        crate::model::ProcessIdentity {
            pid: 202,
            process_creation: "proc-1".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    let observed = vec![observed_window()];
    let (dir, store, areas, fulls) = route_dirs("real-cmd");
    let me = route_me();
    super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
        false,
    )
    .expect("seed converges");
    // The drained own edge, exactly as `run_tile_loop` pushes it after
    // the hook-edge drain and before the tick.
    state.world.push_foreground_drain(vec![101], Vec::new(), 0);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_foreground(Some(101));
    let select_down = QueuedWorkspaceIntent {
        op: WorkspaceOp::Select,
        index: 1,
        follow: false,
        edge: SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    };
    let delta = drive_command_lane(
        &mut state,
        &me,
        &dir,
        &store,
        &areas,
        vec![select_down],
        false,
    );
    assert!(
        state.world.last_reply().is_some(),
        "real select dispatches through the owner"
    );
    assert_eq!(
        delta, 1,
        "real command turn consumes the edge with no extra observation pass"
    );
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hook_drain_keyup_owes_exactly_one_observation() {
    // Non-foreground hook drain + key-up owes one observation pass.
    // (Existing quiet test covers the no-drain case.)
    let mut state = test_state();
    state.ensure_device("DEV1");
    state.keyboard.takeover = true;
    let (dir, store, areas, _) = route_dirs("hook-drain-keyup");
    let me = route_me();
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_observe(Vec::new(), Vec::new(), false);
    let delta = drive_command_lane(
        &mut state,
        &me,
        &dir,
        &store,
        &areas,
        vec![keyup_intent()],
        true,
    );
    assert_eq!(
        delta, 1,
        "hook-drain wake + key-up owes one observation pass"
    );
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn real_command_with_hook_drain_needs_no_extra_pass() {
    // A lane that entered a world tick already captured fresh inputs.
    let mut state = test_state();
    state.ensure_device("DEV1");
    state.keyboard.takeover = true;
    let key = crate::native_guards::WindowKey {
        hwnd: 101,
        pid: 202,
        creation: "proc-1".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "tag-1".to_owned());
    state.member_identity.insert(
        key,
        crate::model::ProcessIdentity {
            pid: 202,
            process_creation: "proc-1".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    let observed = vec![observed_window()];
    let (dir, store, areas, fulls) = route_dirs("real-cmd-hook-drain");
    let me = route_me();
    super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
        false,
    )
    .expect("seed converges");
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_foreground(Some(101));
    let select_down = QueuedWorkspaceIntent {
        op: WorkspaceOp::Select,
        index: 1,
        follow: false,
        edge: SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    };
    let delta = drive_command_lane(
        &mut state,
        &me,
        &dir,
        &store,
        &areas,
        vec![select_down],
        true,
    );
    assert!(
        state.world.last_reply().is_some(),
        "real select dispatches through the owner"
    );
    assert_eq!(
        delta, 1,
        "real command with hook drain needs no extra observation pass"
    );
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}
