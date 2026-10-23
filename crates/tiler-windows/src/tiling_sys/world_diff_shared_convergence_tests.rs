//! Shared frame-convergence through the production seam (stage 3b).
//!
//! Consumes the SAME `test-fixtures/frame-convergence.json` as the core
//! runners, but every growth/arrival fact is produced by the real native
//! seam and only emitted raw attempts ride the sequencer, never a handbuilt
//! echo. Rig helpers come from the parent diff-test module; no public API,
//! no test features.

use super::rmax03_adapter_tests::test_state;
use super::world_capture::capture_world;
use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
use super::world_diff_tests::{
    ADAPTER, DEV1, GENERATION, admit, dispatch_obs, observed_window, plain_facts, process_identity,
    rect, rig, seq_ack, seq_deliver, sync_observed_from_native, tiled_desire, win_key,
};
use super::world_sequencer::{AckOutcome, WorldSequencer};
use super::{HiddenRecord, MonitorArea, ObservedWindow, PendingClosed, TileLoop};
use crate::model::{WindowClaimKind, WindowIdentity, WindowShowState};
use tiler_core::directional::OutputId;
use tiler_core::geometry::Rect;
use tiler_core::world_report::{
    FrameSetter, HoldReason as CoreHold, ScopeAuthority as Auth, TransferKind, WorldCommand,
};

// --- stage 3b shared frame-convergence through the real seam ---
//
// Consumes the SAME `test-fixtures/frame-convergence.json` as the core
// runner, but every growth fact is produced by the real native seam: the
// prior live frame grows while the requested target stays smaller, the
// faulted setter (clamp/fail/throw) freezes the prewrite frame so the
// write returns growth, and the test feeds the EMITTED raw `FrameAttempt`
// (never a handbuilt echo) into the production `WorldSequencer`. The learn
// cases prove the target grows; the
// max-contradiction case proves the declared cap wins and the stable pair
// still suppresses; the refused/threw cases prove the transient holds
// inside the backoff with no learning and retries controlled past it; the
// reapply case proves exact re-observation converges with the floor kept.
// Oracles match the core runner on whole response frames, not width alone.
// Offline-only predicates (DPI rescale, fixed-size) stay in direct core
// tests; incomplete/hidden/unreadable observations and replacement/
// settlement ride the dedicated shared sections below through the same
// real seam (scripted capture conditions, emitted attempts only).

#[test]
fn shared_frame_convergence_learns_through_production_sequencing() {
    use tiler_core::world_report::FrameSetter;
    let raw = include_str!("../../../../test-fixtures/frame-convergence.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("valid fixture");
    assert_eq!(fixture["version"], serde_json::Value::from(1));
    let cases = fixture["cases"].as_array().expect("cases array");
    assert_eq!(cases.len(), 5);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let window = case["window"].as_str().expect("window");
        let extra_w = case["extra_w"].as_i64().expect("extra_w") as i32;
        let extra_h = case["extra_h"].as_i64().expect("extra_h") as i32;
        let setter = match case["setter"].as_str().unwrap_or("accepted") {
            "accepted" => FrameSetter::Accepted,
            "refused" => FrameSetter::Refused,
            "threw" => FrameSetter::Threw,
            other => panic!("{name}: unknown setter {other}"),
        };
        let expect_retry = case["expect_retry"].as_bool().unwrap_or(false);
        let expect_reapply = case["expect_reapply"].as_bool().unwrap_or(false);
        let mut rig = rig();
        let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
        let report = dispatch_obs(&mut seq, &rig, 1001, 1000);
        let reply = seq_deliver(&mut seq, &report);
        seq_ack(&mut seq, &reply);
        let (baseline, _) = tiled_desire(&reply, window);
        // The racing enlargement lands as the live native frame first: same
        // origin, larger extent, against the smaller requested target.
        let grown = Rect {
            x: baseline.x,
            y: baseline.y,
            w: baseline.w + extra_w,
            h: baseline.h + extra_h,
        };
        let slot = rig
            .observed
            .iter()
            .position(|live| live.token == window)
            .expect("window observed");
        rig.observed[slot].visible = grown;
        rig.observed[slot].outer = Rect {
            x: grown.x - 8,
            y: grown.y - 8,
            w: grown.w + 16,
            h: grown.h + 16,
        };
        rig.observed[slot].insets =
            crate::tiling::FrameInsets::measure(rig.observed[slot].outer, grown);
        if case["declared_max"].as_str() == Some("baseline-w") {
            rig.state.hint_logged.insert(
                window.to_owned(),
                tiler_core::size_hints::WindowSizeHints {
                    min_w: None,
                    min_h: None,
                    max_w: Some(baseline.w),
                    max_h: None,
                },
            );
        }
        // Real actuation: the faulted setter freezes the grown prewrite
        // frame, so the write returns growth as its readback with the
        // mechanical setter (clamp accepts, fail refuses, throw throws).
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
        match setter {
            FrameSetter::Accepted => {
                native.clamp_frame.insert(window.to_owned());
            }
            FrameSetter::Refused => {
                native.fail_frame.insert(window.to_owned());
            }
            FrameSetter::Threw => {
                native.throw_frame.insert(window.to_owned());
            }
        }
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
        let emitted = attempts
            .iter()
            .find(|attempt| attempt.window.0 == window)
            .expect("seam emits the raw growth attempt");
        assert_eq!(
            emitted.target, baseline,
            "{name}: attempt targets the desire"
        );
        assert_eq!(emitted.setter, setter, "{name}: mechanical setter rides");
        assert_eq!(
            emitted.readback,
            Some(grown),
            "{name}: readback is the growth"
        );
        for attempt in &attempts {
            seq.note_frame_attempt(attempt.clone());
        }
        diff.gate.note_applied(&reply);
        sync_observed_from_native(&mut rig, &native);
        seq.invalidate();
        // Inside the 500ms transient backoff for refusal lanes; the stable
        // clamp lane holds at any stamp.
        let second_ms = if expect_retry { 1200 } else { 2000 };
        let report2 = dispatch_obs(&mut seq, &rig, 1001, second_ms);
        assert!(
            report2
                .frames
                .iter()
                .any(|attempt| attempt.window.0 == window),
            "{name}: raw facts ride, got {:?}",
            report2.frames
        );
        let reply2 = seq_deliver(&mut seq, &report2);
        let row = reply2
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .expect("second row");
        if case["expect_grows"].as_bool().expect("expect_grows") {
            let next = row.effective_frame.expect("tiled next");
            assert!(
                next.w >= grown.w && next.h == grown.h,
                "{name}: production target {next:?} covers {grown:?}"
            );
            assert_eq!(
                row.frame_authority,
                Auth::Controlled,
                "{name}: learned target actionable"
            );
            seq_ack(&mut seq, &reply2);
            if expect_reapply {
                // Exact convergence: the app catches up to the learned
                // target, so the next observation settles controlled with
                // the floor kept.
                for live in &mut rig.observed {
                    let desired = reply2
                        .windows
                        .iter()
                        .find(|row| row.lifetime.0 == live.token)
                        .and_then(|row| row.effective_frame)
                        .expect("tiled next");
                    live.visible = desired;
                    live.outer = Rect {
                        x: desired.x - 8,
                        y: desired.y - 8,
                        w: desired.w + 16,
                        h: desired.h + 16,
                    };
                    live.insets = crate::tiling::FrameInsets::measure(live.outer, desired);
                }
                seq.invalidate();
                let report3 = dispatch_obs(&mut seq, &rig, 1001, 3000);
                assert!(
                    report3.frames.is_empty(),
                    "{name}: converged tick carries no attempts: {:?}",
                    report3.frames
                );
                let reply3 = seq_deliver(&mut seq, &report3);
                let settled = reply3
                    .windows
                    .iter()
                    .find(|row| row.lifetime.0 == window)
                    .expect("settled row");
                assert_eq!(
                    settled.effective_frame.expect("tiled"),
                    next,
                    "{name}: converged target keeps the floor"
                );
                assert_eq!(
                    settled.frame_authority,
                    Auth::Controlled,
                    "{name}: converged target actionable"
                );
                seq_ack(&mut seq, &reply3);
            }
        } else if expect_retry {
            assert_eq!(
                row.frame_authority,
                Auth::Hold(CoreHold::ConstraintClamp),
                "{name}: transient holds inside backoff"
            );
            seq_ack(&mut seq, &reply2);
            // Past the backoff with no new attempt: the adapter may retry
            // controlled, learning nothing.
            seq.invalidate();
            let report3 = dispatch_obs(&mut seq, &rig, 1001, 3000);
            assert!(
                report3.frames.is_empty(),
                "{name}: no redelivery without a write: {:?}",
                report3.frames
            );
            let reply3 = seq_deliver(&mut seq, &report3);
            assert_eq!(
                reply3
                    .windows
                    .iter()
                    .find(|row| row.lifetime.0 == window)
                    .expect("retry row")
                    .frame_authority,
                Auth::Controlled,
                "{name}: past backoff retries controlled"
            );
            seq_ack(&mut seq, &reply3);
        } else {
            assert_eq!(
                row.effective_frame.expect("tiled"),
                baseline,
                "{name}: contradictory max keeps target"
            );
            assert_eq!(
                row.frame_authority,
                Auth::Hold(CoreHold::ConstraintClamp),
                "{name}: stable pair still suppresses"
            );
            seq_ack(&mut seq, &reply2);
        }
    }
}

// --- stage 3b shared replacement through the real seam ---
//
// Consumes the SAME `test-fixtures/frame-convergence.json`
// `replacement_cases` as the core runner: a learned floor is established
// through the real clamp seam, the token retires through the real close
// path (native close facts plus the close receipt prune the binding),
// and the token is reused by a fresh native lifetime. The reused token
// re-admits at the un-floored desire, controlled, proving no suppression
// or floor is inherited. Every attempt rides the emitted seam facts;
// close/re-admit conditions ride the real capture and receipt path.

#[test]
fn shared_replacement_reuses_retired_token_fresh_through_sequencing() {
    use tiler_core::world_report::FrameSetter;
    let raw = include_str!("../../../../test-fixtures/frame-convergence.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("valid fixture");
    let cases = fixture["replacement_cases"]
        .as_array()
        .expect("replacement_cases array");
    assert_eq!(cases.len(), 1);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let window = case["window"].as_str().expect("window");
        let extra_w = case["extra_w"].as_i64().expect("extra_w") as i32;
        let extra_h = case["extra_h"].as_i64().expect("extra_h") as i32;
        assert_eq!(window, "w1", "{name}: shared shape retires w1");
        let mut rig = rig();
        let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
        let report = dispatch_obs(&mut seq, &rig, 1001, 1000);
        let reply = seq_deliver(&mut seq, &report);
        seq_ack(&mut seq, &reply);
        let (baseline, _) = tiled_desire(&reply, window);
        // Learn a floor through the real clamp seam, as the shared runner.
        let grown = Rect {
            x: baseline.x,
            y: baseline.y,
            w: baseline.w + extra_w,
            h: baseline.h + extra_h,
        };
        let slot = rig
            .observed
            .iter()
            .position(|live| live.token == window)
            .expect("window observed");
        rig.observed[slot].visible = grown;
        rig.observed[slot].outer = Rect {
            x: grown.x - 8,
            y: grown.y - 8,
            w: grown.w + 16,
            h: grown.h + 16,
        };
        rig.observed[slot].insets =
            crate::tiling::FrameInsets::measure(rig.observed[slot].outer, grown);
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
        native.clamp_frame.insert(window.to_owned());
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
        let emitted = attempts
            .iter()
            .find(|attempt| attempt.window.0 == window)
            .expect("seam emits the raw growth attempt");
        assert_eq!(emitted.setter, FrameSetter::Accepted);
        assert_eq!(emitted.readback, Some(grown));
        for attempt in &attempts {
            seq.note_frame_attempt(attempt.clone());
        }
        diff.gate.note_applied(&reply);
        sync_observed_from_native(&mut rig, &native);
        seq.invalidate();
        let report2 = dispatch_obs(&mut seq, &rig, 1001, 2000);
        let reply2 = seq_deliver(&mut seq, &report2);
        let next = reply2
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .expect("second row")
            .effective_frame
            .expect("tiled next");
        assert!(
            next.w >= grown.w,
            "{name}: floor learned through the seam: {next:?} covers {grown:?}"
        );
        seq_ack(&mut seq, &reply2);
        // Retire the token with genuine native close facts: the window
        // leaves enumeration, the binding drops, and the close queues.
        let old_key = win_key(1001, 2001, "proc-1");
        rig.observed.retain(|live| live.token != window);
        rig.state.member_tokens.remove(&old_key);
        rig.state.member_tags.remove(&old_key);
        rig.state.member_identity.remove(&old_key);
        rig.state.applied_homing.remove(window);
        rig.state.pending_closed.insert(
            window.to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: rig.ws.clone(),
            },
        );
        seq.invalidate();
        let report3 = dispatch_obs(&mut seq, &rig, 1001, 3000);
        assert!(
            report3.closed.iter().any(|id| id.0 == window),
            "{name}: the retired token rides closed: {:?}",
            report3.closed
        );
        let reply3 = seq_deliver(&mut seq, &report3);
        let receipt = match seq.ack(&reply3) {
            AckOutcome::Applied(receipt) => receipt,
            other => panic!("{name}: expected applied, got {other:?}"),
        };
        let mut pending = rig.state.pending_closed.clone();
        assert_eq!(
            seq.apply_close_receipt(&mut pending, &receipt),
            vec![window.to_owned()],
            "{name}: receipt prunes the retired binding"
        );
        rig.state.pending_closed = pending;
        // Reuse the token for a fresh native lifetime (new key, same
        // observed facts): admission takes the ordinary path and inherits
        // nothing from the retired floor.
        let new_key = win_key(1003, 2003, "proc-3");
        admit(&mut rig.state, &new_key, DEV1, &rig.ws, window, false);
        rig.state
            .applied_homing
            .insert(window.to_owned(), (rig.out.clone(), rig.ws.clone()));
        rig.observed
            .push(observed_window(&new_key, window, grown, plain_facts()));
        seq.invalidate();
        let report4 = dispatch_obs(&mut seq, &rig, 1001, 4000);
        assert!(
            report4.closed.iter().all(|id| id.0 != window),
            "{name}: reused token rides no stale close: {:?}",
            report4.closed
        );
        let reply4 = seq_deliver(&mut seq, &report4);
        let row = reply4
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .expect("reused row");
        assert_eq!(
            row.frame_authority,
            Auth::Controlled,
            "{name}: reused token actionable"
        );
        assert_eq!(
            row.effective_frame.expect("tiled"),
            baseline,
            "{name}: reused token takes the un-floored desire"
        );
        // Nothing retained is observable at the seam: an inherited floor
        // would grow the target past the baseline and inherited
        // suppression would hold the row.
        let retained =
            row.effective_frame != Some(baseline) || row.frame_authority != Auth::Controlled;
        assert_eq!(
            case["expect_retained"].as_bool().expect("expect_retained"),
            retained,
            "{name}: reuse retains nothing"
        );
        seq_ack(&mut seq, &reply4);
    }
}

// --- stage 3b shared observations through the real seam ---
//
// Consumes the SAME `test-fixtures/frame-convergence.json`
// `observation_cases` as the core runner. A real clamp attempt is produced
// by the seam first; then the observation condition is scripted through
// the real capture path only (failed enumeration, iconic hidden claim,
// identity drift) while attempts still come solely from seam primitives:
// the flagged apply pass must emit no new attempt for the window (no
// fabricated proof). The pending real attempt rides the flagged report as
// a truthful receipt, the core settles authority as tabled with no bounds
// learned, and the recovery observation converges controlled with an ack.

#[test]
fn shared_observations_record_no_bounds_without_proof_through_sequencing() {
    use tiler_core::world_report::FrameSetter;
    let raw = include_str!("../../../../test-fixtures/frame-convergence.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("valid fixture");
    let cases = fixture["observation_cases"]
        .as_array()
        .expect("observation_cases array");
    assert_eq!(cases.len(), 3);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let window = case["window"].as_str().expect("window");
        let extra_w = case["extra_w"].as_i64().expect("extra_w") as i32;
        let extra_h = case["extra_h"].as_i64().expect("extra_h") as i32;
        let condition = case["condition"].as_str().expect("condition");
        assert_eq!(window, "w1", "{name}: shared shape flags w1");
        let expect = match case["expect_authority"].as_str().expect("authority") {
            "controlled" => Auth::Controlled,
            "hold-clamp" => Auth::Hold(CoreHold::ConstraintClamp),
            "hold-minimized" => Auth::Hold(CoreHold::Minimized),
            "hold-unreadable" => Auth::Hold(CoreHold::Unreadable),
            "hold-incomplete" => Auth::Hold(CoreHold::IncompleteInventory),
            other => panic!("{name}: unknown expect_authority {other}"),
        };
        let recover = match case["recover_authority"].as_str().expect("authority") {
            "controlled" => Auth::Controlled,
            "hold-clamp" => Auth::Hold(CoreHold::ConstraintClamp),
            "hold-minimized" => Auth::Hold(CoreHold::Minimized),
            "hold-unreadable" => Auth::Hold(CoreHold::Unreadable),
            other => panic!("{name}: unknown recover_authority {other}"),
        };
        let suppressed = case["expect_suppressed"]
            .as_bool()
            .expect("expect_suppressed");
        let recover_rect = case["recover_rect"].as_str().expect("recover_rect");
        // Recovery is the seam-side proxy for the private pair map (the
        // seam holds no handle on it): a recorded pair either clears exact
        // (baseline recovery converges controlled) or re-suppresses (grown
        // recovery re-holds clamp); an unrecorded lane converges
        // controlled. The tabled combination must say exactly that.
        match (suppressed, recover_rect) {
            (true, "baseline") => assert_eq!(
                recover,
                Auth::Controlled,
                "{name}: exact recovery clears the recorded pair"
            ),
            (true, "grown") => assert_eq!(
                recover,
                Auth::Hold(CoreHold::ConstraintClamp),
                "{name}: grown recovery re-holds the recorded pair"
            ),
            (false, "baseline") | (false, "grown") => assert_eq!(
                recover,
                Auth::Controlled,
                "{name}: unrecorded lane recovers clean"
            ),
            (_, other) => panic!("{name}: unknown recover_rect {other}"),
        }
        let mut rig = rig();
        let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
        let report = dispatch_obs(&mut seq, &rig, 1001, 1000);
        let reply = seq_deliver(&mut seq, &report);
        seq_ack(&mut seq, &reply);
        let (baseline, _) = tiled_desire(&reply, window);
        // Real growth attempt first: the racing enlargement lands live,
        // the clamp freezes it, and the emitted raw attempt rides pending.
        let grown = Rect {
            x: baseline.x,
            y: baseline.y,
            w: baseline.w + extra_w,
            h: baseline.h + extra_h,
        };
        let slot = rig
            .observed
            .iter()
            .position(|live| live.token == window)
            .expect("window observed");
        rig.observed[slot].visible = grown;
        rig.observed[slot].outer = Rect {
            x: grown.x - 8,
            y: grown.y - 8,
            w: grown.w + 16,
            h: grown.h + 16,
        };
        rig.observed[slot].insets =
            crate::tiling::FrameInsets::measure(rig.observed[slot].outer, grown);
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
        native.clamp_frame.insert(window.to_owned());
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
        let emitted = attempts
            .iter()
            .find(|attempt| attempt.window.0 == window)
            .expect("seam emits the raw growth attempt");
        assert_eq!(emitted.setter, FrameSetter::Accepted);
        assert_eq!(emitted.readback, Some(grown));
        for attempt in &attempts {
            seq.note_frame_attempt(attempt.clone());
        }
        diff.gate.note_applied(&reply);
        sync_observed_from_native(&mut rig, &native);
        // Flag the observation through real capture inputs only. The saved
        // identity restores the unreadable lane afterwards.
        let key = win_key(1001, 2001, "proc-1");
        let saved_identity = rig.state.member_identity.get(&key).cloned();
        match condition {
            "incomplete" => {}
            "hidden" => {
                rig.state.hidden_claims.insert(
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
            }
            "unreadable" => {
                let mut drifted = process_identity(&key);
                drifted.pid = 9999;
                rig.state.member_identity.insert(key.clone(), drifted);
            }
            other => panic!("{name}: unknown condition {other}"),
        }
        // Flagged apply pass: the seam must fabricate no new attempt for
        // the flagged window (hidden/iconic/unreadable holds take no
        // writes); the incomplete lane applies nothing at all. Capture
        // truth differs per lane: iconic-hidden rides a genuine minimized
        // fact with a complete inventory, drift rides unreadable with a
        // gap, and the table pins which.
        if condition != "incomplete" {
            let flagged = capture_world(
                &rig.state.inputs(),
                Some(&rig.observed),
                &rig.retained,
                &rig.areas,
                1001,
            );
            let flagged_fact = flagged
                .windows
                .iter()
                .find(|fact| fact.window.0 == window)
                .expect("flagged fact rides");
            assert!(
                flagged_fact.rect.is_none(),
                "{name}: flagged fact is frameless"
            );
            if condition == "hidden" {
                assert!(
                    flagged_fact.minimized,
                    "{name}: iconic-hidden rides minimized: {flagged_fact:?}"
                );
            } else {
                assert!(
                    !flagged_fact.minimized,
                    "{name}: drift rides unreadable, never minimized: {flagged_fact:?}"
                );
            }
            assert_eq!(
                flagged.inventory_complete,
                case["expect_complete"].as_bool().expect("expect_complete"),
                "{name}: flagged capture completeness as tabled"
            );
            let fresh2 = FreshHost::from_sources(
                &rig.state.inputs(),
                &rig.observed,
                &rig.retained,
                &flagged,
            );
            let mut native2 = FakeNative::seeded(&fresh2);
            let mut diff2 = HostDiff::new(ADAPTER, GENERATION);
            diff2.gate.current_obs = seq.current_obs();
            let mut flagged_attempts = Vec::new();
            apply_world_reply(
                &mut diff2,
                &reply,
                &fresh2,
                &mut native2,
                &mut flagged_attempts,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut Vec::new(),
                1500,
            );
            assert!(
                flagged_attempts
                    .iter()
                    .all(|attempt| attempt.window.0 != window),
                "{name}: no fabricated proof: {flagged_attempts:?}"
            );
            for attempt in &flagged_attempts {
                seq.note_frame_attempt(attempt.clone());
            }
        }
        // Flagged report: failed enumeration for the incomplete lane,
        // flagged capture otherwise. Only the pre-flag real attempt rides.
        seq.invalidate();
        let report2 = if condition == "incomplete" {
            seq.try_dispatch(
                &rig.state.inputs(),
                None,
                &rig.retained,
                &rig.areas,
                1001,
                2000,
                None,
            )
            .expect("incomplete dispatch")
        } else {
            dispatch_obs(&mut seq, &rig, 1001, 2000)
        };
        assert_eq!(
            report2.inventory_complete,
            case["expect_complete"].as_bool().expect("expect_complete"),
            "{name}: the receipt completeness as tabled"
        );
        assert!(
            report2
                .frames
                .iter()
                .any(|attempt| attempt.window.0 == window
                    && attempt.setter == FrameSetter::Accepted
                    && attempt.readback == Some(grown)),
            "{name}: only the pre-flag real attempt rides: {:?}",
            report2.frames
        );
        let reply2 = seq_deliver(&mut seq, &report2);
        assert_eq!(
            reply2
                .windows
                .iter()
                .find(|row| row.lifetime.0 == window)
                .expect("flagged row")
                .frame_authority,
            expect,
            "{name}: flagged observation settles as tabled"
        );
        seq_ack(&mut seq, &reply2);
        // Recovery: clear the flag and converge as tabled. The incomplete
        // lane re-observes the un-floored desire exactly; the other lanes
        // keep the grown observation as a fresh deviation.
        rig.state.hidden_claims.remove(&key);
        if let Some(saved) = saved_identity {
            rig.state.member_identity.insert(key.clone(), saved);
        }
        if case["recover_rect"].as_str() == Some("baseline") {
            rig.observed[slot].visible = baseline;
            rig.observed[slot].outer = Rect {
                x: baseline.x - 8,
                y: baseline.y - 8,
                w: baseline.w + 16,
                h: baseline.h + 16,
            };
            rig.observed[slot].insets =
                crate::tiling::FrameInsets::measure(rig.observed[slot].outer, baseline);
        }
        seq.invalidate();
        let report3 = dispatch_obs(&mut seq, &rig, 1001, 3000);
        assert!(
            report3.inventory_complete,
            "{name}: recovery completes the inventory"
        );
        let reply3 = seq_deliver(&mut seq, &report3);
        assert_eq!(
            reply3
                .windows
                .iter()
                .find(|row| row.lifetime.0 == window)
                .expect("recovered row")
                .frame_authority,
            recover,
            "{name}: recovery converges as tabled"
        );
        seq_ack(&mut seq, &reply3);
    }
}

// --- stage 3b shared settlement through the real seam ---
//
// Consumes the SAME `test-fixtures/frame-convergence.json`
// `settlement_cases` as the core runner through the production
// `WorldSequencer`: a real `MigrateWorkspace` command retains the
// migration, the real diff moves one member while the faulted membership
// setter refuses the other, and only the EMITTED raw `MemberAttempt`s
// (never handbuilt verdicts) ride the next report. The partial arrival
// holds the operation; the refused member's external arrival then
// completes it to the source refill with no re-actuation behind it (held
// transfers never re-actuate by design). Observation facts for arrived
// members are scripted onto the target output through the real capture
// path. Expected retained flags and settled ids come
// from the table; runtime operation ids resolve from the reply.

#[test]
fn shared_settlement_partial_holds_then_completes_through_sequencing() {
    let raw = include_str!("../../../../test-fixtures/frame-convergence.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("valid fixture");
    let cases = fixture["settlement_cases"]
        .as_array()
        .expect("settlement_cases array");
    assert_eq!(cases.len(), 1);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let members: Vec<String> = case["members"]
            .as_array()
            .expect("members")
            .iter()
            .map(|v| v.as_str().expect("member").to_owned())
            .collect();
        assert_eq!(members.len(), 2, "{name}: shared shape is two members");
        let refuse_member = case["refuse_member"].as_str().expect("refuse_member");
        let arrive_member = members
            .iter()
            .find(|m| m.as_str() != refuse_member)
            .expect("arrive member");
        // Two-output rig: both members home on out-a, out-b empty.
        const DEV2_NAME: &str = "DEV2";
        let mut state = test_state();
        state.ensure_device(DEV1);
        state.ensure_device(DEV2_NAME);
        let out_a = state.output_token(DEV1);
        let out_b = state.output_token(DEV2_NAME);
        let ws = "ws1".to_owned();
        let k1 = win_key(1001, 2001, "proc-1");
        let k2 = win_key(1002, 2002, "proc-2");
        admit(&mut state, &k1, DEV1, &ws, &members[0], false);
        admit(&mut state, &k2, DEV1, &ws, &members[1], false);
        for token in &members {
            state
                .applied_homing
                .insert(token.clone(), (out_a.clone(), ws.clone()));
        }
        let mut live = Some(vec![
            observed_window(&k1, &members[0], rect(0, 0, 800, 600), plain_facts()),
            observed_window(&k2, &members[1], rect(0, 610, 800, 400), plain_facts()),
        ]);
        let areas = vec![
            MonitorArea {
                device: DEV1.to_owned(),
                work: rect(0, 0, 1920, 1040),
                full: rect(0, 0, 1920, 1080),
            },
            MonitorArea {
                device: DEV2_NAME.to_owned(),
                work: rect(1920, 0, 1920, 1040),
                full: rect(1920, 0, 1920, 1080),
            },
        ];
        let dispatch = |seq: &mut WorldSequencer,
                        state: &TileLoop,
                        live: &Option<Vec<ObservedWindow>>,
                        ms: u64,
                        command: Option<WorldCommand>| {
            seq.try_dispatch(
                &state.inputs(),
                live.as_ref().map(Vec::as_slice),
                &Vec::new(),
                &areas,
                1001,
                ms,
                command,
            )
            .expect("dispatch")
        };
        let mut seq = WorldSequencer::bootstrap(ADAPTER, GENERATION).expect("bootstrap");
        let report = dispatch(&mut seq, &state, &live, 1000, None);
        let reply = seq_deliver(&mut seq, &report);
        seq_ack(&mut seq, &reply);
        let view_a = reply
            .outputs
            .iter()
            .find(|row| row.output.0 == out_a)
            .and_then(|row| row.logical_view.clone())
            .expect("view a");
        // Proven homing speaks the core logical workspace: realign the
        // receipts from the rig placeholder once the core mints its view.
        for token in &members {
            state
                .applied_homing
                .insert(token.clone(), (out_a.clone(), view_a.clone()));
        }
        // Real migration command through the production sequencer.
        seq.invalidate();
        let report = dispatch(
            &mut seq,
            &state,
            &live,
            2000,
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId(out_a.clone()),
                target_output: OutputId(out_b.clone()),
                workspace: view_a.clone(),
            }),
        );
        let reply = seq_deliver(&mut seq, &report);
        let op = reply
            .transfers
            .iter()
            .find(|t| matches!(t.kind, TransferKind::Migration))
            .expect("migration retained")
            .operation_id
            .clone();
        seq_ack(&mut seq, &reply);
        // Real actuation with one faulted membership setter: the arrived
        // member proves through its post homing, the refused member holds
        // at the source with an unchanged readable preimage.
        let captured = capture_world(&state.inputs(), live.as_deref(), &[], &areas, 1001);
        let fresh = FreshHost::from_sources(
            &state.inputs(),
            live.as_ref().expect("live"),
            &[],
            &captured,
        );
        let mut native = FakeNative::seeded(&fresh);
        native.fail_membership.insert(refuse_member.to_string());
        let mut diff = HostDiff::new(ADAPTER, GENERATION);
        diff.gate.current_obs = seq.current_obs();
        let mut members_out = Vec::new();
        let mut views_out = Vec::new();
        apply_world_reply(
            &mut diff,
            &reply,
            &fresh,
            &mut native,
            &mut Vec::new(),
            &mut members_out,
            &mut views_out,
            &mut Vec::new(),
            2000,
        );
        let arrived = members_out
            .iter()
            .find(|attempt| attempt.window.0 == *arrive_member)
            .expect("arrival attempt rides");
        assert_eq!(
            arrived.post,
            Some((OutputId(out_b.clone()), view_a.clone())),
            "{name}: arrival proves: {arrived:?}"
        );
        let refused = members_out
            .iter()
            .find(|attempt| attempt.window.0 == *refuse_member)
            .expect("refusal attempt rides");
        assert_eq!(
            refused.setter,
            FrameSetter::Refused,
            "{name}: mechanical refusal rides"
        );
        assert_eq!(
            refused.post,
            Some((OutputId(out_a.clone()), view_a.clone())),
            "{name}: refusal holds the source preimage: {refused:?}"
        );
        for attempt in &members_out {
            seq.note_member_attempt(attempt.clone());
        }
        diff.gate.note_applied(&reply);
        // The arrived member lands on the target output natively.
        for live_window in live.as_mut().expect("live").iter_mut() {
            if live_window.token == *arrive_member {
                live_window.visible = rect(2000, 10, 600, 400);
                live_window.outer = Rect {
                    x: 2000 - 8,
                    y: 10 - 8,
                    w: 600 + 16,
                    h: 400 + 16,
                };
                live_window.insets =
                    crate::tiling::FrameInsets::measure(live_window.outer, live_window.visible);
            }
        }
        seq.invalidate();
        let report2 = dispatch(&mut seq, &state, &live, 3000, None);
        assert!(
            report2
                .member_attempts
                .iter()
                .any(|attempt| attempt.window.0 == *arrive_member),
            "{name}: raw member facts ride: {:?}",
            report2.member_attempts
        );
        let reply2 = seq_deliver(&mut seq, &report2);
        assert_eq!(
            case["expect_retained"].as_bool().expect("expect_retained"),
            reply2
                .transfers
                .iter()
                .any(|t| t.operation_id == op && matches!(t.kind, TransferKind::Migration)),
            "{name}: partial arrival retains the migration: {:?}",
            reply2.transfers
        );
        let arrived_row = reply2
            .windows
            .iter()
            .find(|row| row.lifetime.0 == *arrive_member)
            .expect("arrived row");
        assert_eq!(
            (
                arrived_row.output.0.as_str(),
                arrived_row.workspace.as_str()
            ),
            (out_b.as_str(), view_a.as_str()),
            "{name}: arrived member homed at target"
        );
        let expect_membership = match case["expect_authority"].as_str().expect("expect_authority") {
            "controlled" => Auth::Controlled,
            other => panic!("{name}: unknown expect_authority {other}"),
        };
        assert_eq!(
            arrived_row.membership_authority, expect_membership,
            "{name}: arrived member actionable as tabled"
        );
        seq_ack(&mut seq, &reply2);
        // Late external arrival: the held transfer never re-actuates by
        // design, so the refused member lands on the target natively with
        // no new attempt behind it; target observation wins over the
        // staged refusal and completes the migration to the refill.
        for live_window in live.as_mut().expect("live").iter_mut() {
            if live_window.token == *refuse_member {
                live_window.visible = rect(2000, 420, 600, 400);
                live_window.outer = Rect {
                    x: 2000 - 8,
                    y: 420 - 8,
                    w: 600 + 16,
                    h: 400 + 16,
                };
                live_window.insets =
                    crate::tiling::FrameInsets::measure(live_window.outer, live_window.visible);
            }
        }
        seq.invalidate();
        let report3 = dispatch(&mut seq, &state, &live, 5000, None);
        assert!(
            report3.member_attempts.is_empty(),
            "{name}: completion rides observation, no new actuation: {:?}",
            report3.member_attempts
        );
        let reply3 = seq_deliver(&mut seq, &report3);
        assert!(
            reply3
                .transfers
                .iter()
                .any(|t| t.operation_id == op && matches!(t.kind, TransferKind::Migration)),
            "{name}: full arrival keeps the migration only for the refill: {:?}",
            reply3.transfers
        );
        // The tabled settlement selects which completion shape to verify
        // against the real runtime operation ids.
        match case["expect_settled"].as_str().expect("expect_settled") {
            "refill-only" => {
                assert!(
                    reply3
                        .transfers
                        .iter()
                        .any(|t| t.operation_id == format!("{op}-refill")),
                    "{name}: the remaining work is the source refill: {:?}",
                    reply3.transfers
                );
                for member in &members {
                    let row = reply3
                        .windows
                        .iter()
                        .find(|row| row.lifetime.0 == *member)
                        .expect("settled row");
                    assert_eq!(
                        row.output.0.as_str(),
                        out_b.as_str(),
                        "{name}: {member} homed"
                    );
                }
            }
            other => panic!("{name}: unknown expect_settled {other}"),
        }
        seq_ack(&mut seq, &reply3);
    }
}
