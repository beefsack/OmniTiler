//! World route smoke test (offline only): the test-only Windows route
//! captures source-first observations, envelopes a native-free report,
//! and sequences it through the core engine. Receipts echo proven state
//! only (never desire): a fresh bootstrap proves nothing, a proven view
//! echoes, and a commanded-but-unapplied target flight persists. The
//! offline facade actuates the desired snapshot against fresh proof.
//! No HWNDs, hooks, or live effects.

use super::rmax03_adapter_tests::test_state;
use super::world_capture::capture_world;
use super::world_diff::{FakeNative, FreshHost, HostDiff, NativeOps, apply_world_reply};
use super::world_sequencer::{AckOutcome, WorldSequencer};
use super::{MonitorArea, ObservedTarget, ObservedWindow, RetainedRow};
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
    // core bootstraps logical state; the restoration ingress rides only
    // this first report: persisted rings, views, tiling, and member homing
    // from the live owner tables.
    let out_token = state.output_token("DEV1");
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            None,
        )
        .expect("dispatch");
    // Fresh bootstrap proves nothing yet: no durable restoration ingress
    // (the core mints rings/views from the live screens) and no view
    // receipts (nothing applied, so nothing is proven).
    assert!(report.views.is_empty(), "no receipts before proof");
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].window.0, "w1");
    assert!(report.restored.is_none(), "fresh bootstrap restores nothing");
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
            None,
        )
        .expect("dispatch");
    assert_eq!(
        second.views, report.views,
        "no receipt before proof, got {:?}",
        second.views
    );
    assert!(
        second.restored.is_none(),
        "restoration never fabricates"
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
    let (vstats, vproduced) = apply_world_reply(&mut vdiff, &reply, &fresh, &mut vnative);
    assert_eq!(vstats.view_writes, 1, "stats: {vstats:?}");
    let view_op = reply
        .transfers
        .iter()
        .find(|t| t.target_workspace == target)
        .map(|t| t.operation_id.clone())
        .expect("view flight");
    assert!(
        vproduced.iter().any(|f| f.operation_id == view_op
            && f.outcome == tiler_core::world_report::AttemptOutcome::Applied),
        "view actuation proves, got {vproduced:?}"
    );
    vdiff.gate.note_applied(&reply);
    state.applied_views.insert(out_token.clone(), target.clone());
    seq.invalidate();
    let echoed = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
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
    let (stats, _feedback) = apply_world_reply(&mut diff, &geometry_reply, &fresh, &mut native);
    assert_eq!(stats.held, 0, "stats: {stats:?}");
    assert!(stats.geometry_writes > 0, "stats: {stats:?}");
    diff.gate.note_applied(&geometry_reply);
    // A racing native invalidation stops the pass before any setter.
    native.pending_invalidation = true;
    let (stats, _) = apply_world_reply(&mut diff, &geometry_reply, &fresh, &mut native);
    assert!(stats.stopped_mid_apply, "stats: {stats:?}");
    assert_eq!(stats.geometry_writes, 0, "stats: {stats:?}");
    // Feedback stages for the next report; a lost flight stays dirty.
    seq.note_feedback(tiler_core::world_report::AttemptFeedback {
        operation_id: "op-9".to_owned(),
        kind: tiler_core::world_report::AttemptKind::Effect,
        outcome: tiler_core::world_report::AttemptOutcome::Applied,
        hold: None,
    });
    assert_eq!(seq.pending_feedback_len(), 1);
    assert!(seq.has_pending_feedback());
    seq.invalidate();
    let third = seq
        .try_dispatch(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
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
        feedback_acked: Vec::new(),
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
    );
    let reply = reply.expect("send converges");
    assert!(reply.ready);
    assert!(
        state.applied_homing.is_empty(),
        "unactuated send proves no homing: {:?}",
        state.applied_homing
    );
    assert!(
        reply.transfers.iter().any(|t| matches!(&t.kind, tiler_core::world_report::TransferKind::Window(w) if w.0 == "w1") && t.target_workspace == target),
        "unproven send flight stays retained, got {:?}",
        reply.transfers
    );
    let _ = std::fs::remove_dir_all(&dir);
}
