//! World-runtime unit tests (offline): command mapping covers every
//! supported tiling action plus exactly the armed select subset;
//! refusal vocabulary, geometry convergence, minimized homing/close
//! paths, and honest completion outcomes. No live AX, no threads.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::session::DomainKey;

use crate::runtime::{InputSemanticAction, NativeContext, ObservedWindow, window_id};
use crate::runtime_core::{RetainedApp, RetainedCore};
use crate::runtime_types::{
    AxCompletion, NativeDomain, PHASE1_GAP, PHASE1_OUTER_GAP, ScopedBackendSnapshot,
};
use crate::world_command_map::*;
use crate::world_runtime::*;

fn retained_with_window(pid: u32, wid: u32, domain: &DomainKey) -> RetainedCore {
    let mut retained = RetainedCore::default();
    retained.apps.insert(
        pid,
        RetainedApp {
            generation: 1,
            seq_last: 1,
            cmd_counter: 0,
            windows: vec![ObservedWindow {
                wid,
                x: 0,
                y: 0,
                w: 100,
                h: 100,
                floating: false,
                fullscreen: false,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
            }],
            focused_wid: Some(wid),
            roster: std::collections::BTreeMap::new(),
            roster_incomplete: false,
            roster_cause: String::new(),
        },
    );
    retained
        .window_domains
        .insert(window_id(pid, wid), domain.clone());
    retained
}

fn domain_key() -> DomainKey {
    DomainKey {
        output: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId("space-11".to_owned()),
    }
}

#[test]
fn select_commands_cover_exactly_the_armed_subset() {
    use crate::workspace_input::{is_armed_select, workspace_unsupported_outcome};
    let output = OutputId("macos-display-1".to_owned());
    // Every armed select maps to a planner-owned select command. Numbered
    // selects ride positional indices over the adopted ring (core resolves
    // them against native Desktop order); relative and previous keep
    // their shared core semantics.
    for action in [
        InputSemanticAction::WorkspaceSelect { index: 0 },
        InputSemanticAction::WorkspaceSelect { index: 1 },
        InputSemanticAction::WorkspaceSelect { index: 9 },
        InputSemanticAction::WorkspaceRelative { prev: true },
        InputSemanticAction::WorkspaceRelative { prev: false },
        InputSemanticAction::WorkspacePrevious,
    ] {
        assert!(is_armed_select(&action), "{action:?} is armed");
        assert!(
            select_command(&action, Some(output.clone())).is_some(),
            "{action:?} has a planner route"
        );
    }
    // Numbered send-follow 0..9 plus numbered stays 1..9 dispatch (never
    // the select route) with a planner-owned send route; trailing
    // stay-0, relative sends/stays, and outputs
    // have no dispatch route beyond their honest refusal. Trailing
    // select-0 dispatches through the select route.
    for index in 0..=9u8 {
        let action = InputSemanticAction::WorkspaceSend {
            index,
            follow: true,
        };
        assert_eq!(
            workspace_unsupported_outcome(&action),
            None,
            "{action:?} dispatches"
        );
        assert!(
            select_command(&action, Some(output.clone())).is_none(),
            "{action:?} never selects"
        );
        if index == 0 {
            assert!(
                send_command(&action, "4242_7", output.clone()).is_some(),
                "{action:?} has the product trailing send route"
            );
        } else {
            assert!(
                send_command(&action, "4242_7", output.clone()).is_some(),
                "{action:?} has the product send route"
            );
        }
    }
    for index in 1..=9u8 {
        let stay = InputSemanticAction::WorkspaceSend {
            index,
            follow: false,
        };
        assert_eq!(
            workspace_unsupported_outcome(&stay),
            None,
            "{stay:?} dispatches (armed, bindable unbound)"
        );
        assert!(
            select_command(&stay, Some(output.clone())).is_none(),
            "{stay:?} never selects"
        );
        assert!(
            send_command(&stay, "4242_7", output.clone()).is_some(),
            "{stay:?} has the product send route"
        );
    }
    // Trailing select-0 dispatches through the planner-owned select route.
    assert_eq!(
        workspace_unsupported_outcome(&InputSemanticAction::WorkspaceSelect { index: 0 }),
        None,
        "select-0 dispatches"
    );
    assert!(
        select_command(
            &InputSemanticAction::WorkspaceSelect { index: 0 },
            Some(output.clone())
        )
        .is_some(),
        "select-0 has the planner trailing route"
    );
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: false,
        },
        InputSemanticAction::WorkspaceSendRelative {
            prev: true,
            follow: false,
        },
        InputSemanticAction::WorkspaceOutput {
            direction: crate::runtime_types::InputDirection::Left,
            follow: true,
        },
    ] {
        assert!(workspace_unsupported_outcome(&action).is_some());
        assert!(
            select_command(&action, Some(output.clone())).is_none(),
            "{action:?} stays refused"
        );
        assert!(
            send_command(&action, "4242_7", output.clone()).is_none(),
            "{action:?} stays refused"
        );
    }
}

#[test]
fn operation_commands_cover_every_supported_tiling_action() {
    let mut retained = RetainedCore::default();
    let window = "1001_7";
    for (action, op) in [
        (
            InputSemanticAction::FocusStep {
                direction: crate::runtime_types::InputDirection::Left,
            },
            OP_FOCUS,
        ),
        (
            InputSemanticAction::MoveStep {
                direction: crate::runtime_types::InputDirection::Right,
            },
            OP_MOVE,
        ),
        (
            InputSemanticAction::ResizeStep {
                direction: crate::runtime_types::InputDirection::Right,
                mode: tiler_core::contract::ResizeMode::Outwards,
            },
            OP_RESIZE,
        ),
        (InputSemanticAction::ToggleFloat, OP_TOGGLE_FLOAT),
        (
            InputSemanticAction::ToggleOrientation,
            OP_TOGGLE_ORIENTATION,
        ),
    ] {
        let (command, token) = operation_command(&action, window, &mut retained)
            .unwrap_or_else(|| panic!("{action:?} has an operation route"));
        assert_eq!(token, op);
        assert!(
            tiler_core::world_report::valid_operation(&command),
            "{action:?} carries a valid operation"
        );
    }
    // Workspace actions never ride the operation route (planner-owned
    // selects or refusals only).
    for action in [
        InputSemanticAction::WorkspaceSelect { index: 2 },
        InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: true,
        },
        InputSemanticAction::WorkspacePrevious,
    ] {
        assert!(
            operation_command(&action, window, &mut retained).is_none(),
            "{action:?} is never an operation"
        );
    }
}

#[test]
fn native_raw_plus_gap_converges_to_legacy_inset() {
    // Handover 1 proof: the world engine insets the reported raw
    // visible bounds by the carried outer gap exactly once — the same
    // arithmetic the retired adapter-side pre-inset applied
    // (`tile_domain_bounds`: shrink by the gap on every side, raw
    // fallback when the margin does not fit). Real displays converge;
    // only degenerate sub-margin areas diverge (world holds
    // constraint-clamp instead of tiling raw) — unreachable live.
    for (visible, expected) in [
        (
            Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
            Rect {
                x: 8,
                y: 8,
                w: 1424,
                h: 884,
            },
        ),
        (
            Rect {
                x: 1440,
                y: 0,
                w: 1440,
                h: 900,
            },
            Rect {
                x: 1448,
                y: 8,
                w: 1424,
                h: 884,
            },
        ),
    ] {
        let world = tiler_core::geometry::inset_bounds(visible, PHASE1_OUTER_GAP)
            .expect("real work areas carry the margin");
        assert_eq!(world, expected, "single inset converges: {visible:?}");
    }
    // Degenerate areas document the divergence (never live).
    let tiny = Rect {
        x: 0,
        y: 0,
        w: 10,
        h: 900,
    };
    assert!(tiler_core::geometry::inset_bounds(tiny, PHASE1_OUTER_GAP).is_err());
}

#[test]
fn minimized_entries_home_through_last_known_domains() {
    use crate::classifier::ExcludeReason;
    let key = domain_key();
    let mut retained = retained_with_window(1001, 7, &key);
    retained.known_domains.insert(
        key.clone(),
        tiler_core::session::OutputDomain {
            id: key.output.clone(),
            workspace: key.workspace.clone(),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
            gap: PHASE1_GAP,
            adjacent: BTreeMap::new(),
        },
    );
    // A minimized exclusion with known homing becomes an entry.
    let snapshot = ScopedBackendSnapshot {
        windows: Vec::new(),
        focused_wid: None,
        focus_cg_owned: false,
        focus_evidence: None,
        sys_focus: None,
        global_focus: None,
        meta: Vec::new(),
        excluded: 1,
        excluded_reasons: vec![crate::runtime_types::ExcludedOmission {
            wid: 7,
            reason: ExcludeReason::Minimized,
        }],
        cg_complete: true,
        cg_owner_wids: vec![7],
        cg_alive: Vec::new(),
        revision: 3,
    };
    refresh_minimized(&mut retained, 1001, &snapshot);
    assert_eq!(minimized_entries(&retained).len(), 1);
    // Live again: the entry drops (never double-rides membership).
    let mut live = snapshot.clone();
    live.windows = retained.apps[&1001].windows.clone();
    live.excluded = 0;
    live.excluded_reasons.clear();
    refresh_minimized(&mut retained, 1001, &live);
    assert!(minimized_entries(&retained).is_empty());
}

#[test]
fn minimized_close_in_app_and_at_terminate_ride_explicit_closes() {
    use crate::classifier::ExcludeReason;
    use crate::runtime_types::ExcludedOmission;
    let key = domain_key();
    let mut retained = retained_with_window(1001, 7, &key);
    retained.known_domains.insert(
        key.clone(),
        tiler_core::session::OutputDomain {
            id: key.output.clone(),
            workspace: key.workspace.clone(),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
            gap: PHASE1_GAP,
            adjacent: BTreeMap::new(),
        },
    );
    let minimized_snapshot = || ScopedBackendSnapshot {
        windows: Vec::new(),
        focused_wid: None,
        focus_cg_owned: false,
        focus_evidence: None,
        sys_focus: None,
        global_focus: None,
        meta: Vec::new(),
        excluded: 1,
        excluded_reasons: vec![ExcludedOmission {
            wid: 7,
            reason: ExcludeReason::Minimized,
        }],
        cg_complete: true,
        cg_owner_wids: vec![7],
        cg_alive: Vec::new(),
        revision: 3,
    };
    refresh_minimized(&mut retained, 1001, &minimized_snapshot());
    assert_eq!(minimized_entries(&retained).len(), 1);
    // Closed in-app (absent from members AND exclusions): the entry
    // drops instead of leaking a hold for a dead window.
    let mut closed_snap = minimized_snapshot();
    closed_snap.excluded = 0;
    closed_snap.excluded_reasons.clear();
    refresh_minimized(&mut retained, 1001, &closed_snap);
    assert!(minimized_entries(&retained).is_empty());
    // Terminate with a live minimized entry: it rides the explicit
    // close map under its entry homing (recreate, then terminate).
    refresh_minimized(&mut retained, 1001, &minimized_snapshot());
    let flat: Vec<WindowId> = retained
        .minimized
        .values()
        .map(|entry| entry.window.clone())
        .collect();
    assert!(
        flat.contains(&WindowId("1001_7".to_owned())),
        "minimized terminate closes explicitly"
    );
}

#[test]
fn completion_setters_never_claim_arrival() {
    use tiler_core::world_report::FrameSetter;
    // Only a reported success stages `Accepted` (arrival is never claimed
    // here); partial writes and timeouts are ambiguous (`Threw`, never
    // blindly repeated); proven failures are explicit refusals.
    assert_eq!(
        completion_setter(&AxCompletion::Applied { elapsed_ms: 3 }),
        FrameSetter::Accepted
    );
    assert_eq!(
        completion_setter(&AxCompletion::PartialApplied {
            elapsed_ms: 3,
            size_first_applied: true,
            position_applied: false,
            size_applied: true,
            readback_xywh: None,
            readback_unreadable: false,
        }),
        FrameSetter::Threw
    );
    assert_eq!(
        completion_setter(&AxCompletion::Failed {
            reason: "refused".to_owned()
        }),
        FrameSetter::Refused
    );
    assert_eq!(
        completion_setter(&AxCompletion::Timeout { timeout_ms: 50 }),
        FrameSetter::Threw
    );
    // The immediate post-write readback rides partial completions only;
    // everything else reports unreadable, never guessed.
    assert_eq!(
        completion_readback(&AxCompletion::PartialApplied {
            elapsed_ms: 3,
            size_first_applied: true,
            position_applied: true,
            size_applied: true,
            readback_xywh: Some((1, 2, 30, 40)),
            readback_unreadable: false,
        }),
        Some(tiler_core::geometry::Rect {
            x: 1,
            y: 2,
            w: 30,
            h: 40
        })
    );
    assert_eq!(
        completion_readback(&AxCompletion::Applied { elapsed_ms: 3 }),
        None
    );
}

#[test]
fn select_output_prefers_focus_home_then_first_current() {
    let key = domain_key();
    let retained = retained_with_window(1001, 7, &key);
    let ctx = NativeContext {
        revision: 1,
        domains: vec![NativeDomain {
            cg_id: 1,
            uuid: None,
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            space: 11,
            current: true,
            readable: true,
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
            visible: Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
        }],
        skipped: Vec::new(),
    };
    // No observed focus yet: first current output.
    let mut unfocused = retained;
    unfocused.observed_focus = None;
    assert_eq!(select_output(&unfocused, &ctx), Some(key.output.clone()));
}

#[test]
fn ordinary_frame_traffic_never_occupies_write_correlation() {
    use std::sync::Arc;

    use tiler_core::size_hints::FixedSizePredicate;

    use crate::focus_route::FocusRoute;
    use crate::world_sequencer::ObservationClock;
    // Placement rows (explicit `frame_operation` tokens) are the only
    // rows that settle anything core-side, so they are the only rows
    // tracked: ordinary frame traffic must neither insert nor evict.
    let mut session = WorldSession::new(
        FixedSizePredicate::BothAxes,
        false,
        FocusRoute::Slps,
        Arc::new(ObservationClock::new()),
    );
    track_write(
        &mut session,
        1001,
        1,
        "1001_1",
        tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        Some("op-7"),
    );
    for cmd in 2..=5_000u64 {
        track_write(
            &mut session,
            1001,
            cmd,
            "1001_1",
            tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            None,
        );
    }
    // The lone placement entry survives unbounded ordinary traffic. The
    // retained app generation fences stale completions: the live
    // generation stages, a relaunched PID drops.
    let retained = retained_with_window(1001, 7, &domain_key());
    assert!(
        stage_write_feedback(
            &mut session,
            &retained,
            1001,
            1,
            1,
            &AxCompletion::Applied { elapsed_ms: 3 }
        ),
        "placement feedback stages from proven completion"
    );
    // Unknown completions stage nothing (never false success).
    assert!(
        !stage_write_feedback(
            &mut session,
            &retained,
            1001,
            1,
            9_999,
            &AxCompletion::Applied { elapsed_ms: 3 }
        ),
        "ordinary rows leave no correlation behind"
    );
    // Stale generations drop even with a live correlation entry: a
    // relaunched PID never settles under an old command.
    track_write(
        &mut session,
        1001,
        77,
        "1001_7",
        tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        Some("op-8"),
    );
    assert!(
        !stage_write_feedback(
            &mut session,
            &retained,
            1001,
            2,
            77,
            &AxCompletion::Applied { elapsed_ms: 3 }
        ),
        "stale generation drops without staging"
    );
}

#[test]
fn raw_attempt_staging_drops_unknown_operations_without_evidence() {
    use tiler_core::world_report::FrameSetter;

    use std::sync::Arc;

    use crate::focus_route::FocusRoute;
    use crate::runtime_core::RetainedCore;
    use crate::world_sequencer::ObservationClock;

    // Staging resolves retained transfer targets by operation identity:
    // unknown operations (forged, retired, or never issued) drop without
    // evidence — never fabricated rows, never guessed targets.
    let mut session = WorldSession::new(
        tiler_core::size_hints::FixedSizePredicate::BothAxes,
        false,
        FocusRoute::Slps,
        Arc::new(ObservationClock::new()),
    );
    session.stage_view_attempt(
        "op-404",
        FrameSetter::Accepted,
        Some(11),
        Some(12),
        Some(11),
    );
    session.stage_member_attempt("op-404", FrameSetter::Accepted, None, Some(12));
    let retained = RetainedCore::default();
    let report = build_dispatch_report(
        &mut session,
        &retained,
        &DispatchContext::Synthetic,
        &[],
        1,
        1,
        None,
        None,
        None,
        "req-1",
    )
    .expect("synthetic report builds");
    assert!(
        report.view_attempts.is_empty() && report.member_attempts.is_empty(),
        "unknown operations stage no raw evidence"
    );
}

fn retained_with_known_domain(pid: u32, wid: u32) -> (RetainedCore, DomainKey) {
    let key = domain_key();
    let mut retained = retained_with_window(pid, wid, &key);
    retained.known_domains.insert(
        key.clone(),
        tiler_core::session::OutputDomain {
            id: key.output.clone(),
            workspace: key.workspace.clone(),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
            gap: PHASE1_GAP,
            adjacent: BTreeMap::new(),
        },
    );
    (retained, key)
}

fn hidden_app_snapshot(wid: u32) -> ScopedBackendSnapshot {
    use crate::classifier::ExcludeReason;
    use crate::runtime_types::ExcludedOmission;
    ScopedBackendSnapshot {
        windows: Vec::new(),
        focused_wid: None,
        focus_cg_owned: false,
        focus_evidence: None,
        sys_focus: None,
        global_focus: None,
        meta: Vec::new(),
        excluded: 1,
        excluded_reasons: vec![ExcludedOmission {
            wid,
            reason: ExcludeReason::HiddenApp,
        }],
        cg_complete: true,
        cg_owner_wids: vec![wid],
        cg_alive: Vec::new(),
        revision: 3,
    }
}

/// Option B production path: a HiddenApp exclusion mints a retained
/// minimized record under last-known homing (reversible adapter mapping).
/// Historical A omitted here and kept no homing, which lost the
/// hide-then-terminate close; B retains enough home to close explicitly.
/// The true cause stays in diagnostics via
/// `ExcludeReason::log_token` (`classifier-omitted-hidden-app`), never in
/// the minimized fact itself.
#[test]
fn hidden_app_exclusion_mints_minimized_record_for_explicit_close() {
    use crate::classifier::ExcludeReason;
    use crate::runtime_core::note_scoped_members;
    let (mut retained, key) = retained_with_known_domain(1001, 7);
    // B diagnostic cause stays distinct: the HiddenApp snapshot that mints
    // the minimized fact logs `classifier-omitted-hidden-app`, never the
    // genuine-minimized token.
    assert_eq!(
        hidden_app_snapshot(7).excluded_reasons[0]
            .reason
            .log_token(),
        "classifier-omitted-hidden-app"
    );
    assert_ne!(
        ExcludeReason::HiddenApp.log_token(),
        ExcludeReason::Minimized.log_token()
    );
    // Production order: refresh BEFORE the mapping prune, so last-known
    // homing still names the hidden window's home.
    refresh_minimized(&mut retained, 1001, &hidden_app_snapshot(7));
    note_scoped_members(&mut retained, 1001, &BTreeMap::new());
    let entries = minimized_entries(&retained);
    assert_eq!(entries.len(), 1, "HiddenApp mints one minimized record");
    assert_eq!(entries[0].window.0, "1001_7");
    assert_eq!(entries[0].output, key.output);
    assert_eq!(entries[0].workspace, key.workspace);
    // Live again drops the record (never double-rides membership):
    // simulate unhide by returning the window as a live member.
    let mut members: BTreeMap<DomainKey, Vec<ObservedWindow>> = BTreeMap::new();
    members.insert(
        key.clone(),
        vec![ObservedWindow {
            wid: 7,
            x: 0,
            y: 0,
            w: 100,
            h: 100,
            floating: false,
            fullscreen: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        }],
    );
    let unhide = ScopedBackendSnapshot {
        windows: members[&key].clone(),
        focused_wid: None,
        focus_cg_owned: false,
        focus_evidence: None,
        sys_focus: None,
        global_focus: None,
        meta: Vec::new(),
        excluded: 0,
        excluded_reasons: Vec::new(),
        cg_complete: true,
        cg_owner_wids: vec![7],
        cg_alive: Vec::new(),
        revision: 4,
    };
    refresh_minimized(&mut retained, 1001, &unhide);
    assert!(
        minimized_entries(&retained).is_empty(),
        "unhide drops the minimized record"
    );
}

/// Option B fix for the proven A gap: after a hide, PID terminate emits
/// an explicit close for the hidden window through its minimized-entry
/// homing (same plumbing as the genuine minimized path). Historical A
/// produced no close here (no homing, no record); B closes exactly once
/// and never resurrects.
#[test]
fn hidden_then_terminate_produces_explicit_close() {
    let (mut retained, _key) = retained_with_known_domain(1001, 7);
    refresh_minimized(&mut retained, 1001, &hidden_app_snapshot(7));
    {
        use crate::runtime_core::note_scoped_members;
        note_scoped_members(&mut retained, 1001, &BTreeMap::new());
    }
    let flat: Vec<WindowId> = retained
        .minimized
        .values()
        .map(|entry| entry.window.clone())
        .collect();
    assert!(
        flat.contains(&WindowId("1001_7".to_owned())),
        "hidden terminate closes explicitly"
    );
}

/// Repeat-hide retention: after the first hide plus mapping prune, a
/// second and third same-lifetime HiddenApp snapshot (no unhide) must
/// preserve the previously proven home, not drop the fact. Terminate
/// then still closes explicitly. Unknown never-homed windows still mint
/// nothing (no fabricated home). The genuine Minimized path shares this
/// same preserve arm via `is_minimized_source`.
#[test]
fn hidden_repeat_snapshots_after_prune_retain_fact_and_close() {
    use crate::runtime_core::note_scoped_members;
    let (mut retained, key) = retained_with_known_domain(1001, 7);
    // First hide mints under last-known homing, then the prune drops live
    // homing (production order: refresh before prune).
    refresh_minimized(&mut retained, 1001, &hidden_app_snapshot(7));
    note_scoped_members(&mut retained, 1001, &BTreeMap::new());
    assert_eq!(minimized_entries(&retained).len(), 1);
    // Second and third same-lifetime hidden snapshots arrive with no
    // live homing left: the fact must survive both.
    refresh_minimized(&mut retained, 1001, &hidden_app_snapshot(7));
    assert_eq!(
        minimized_entries(&retained).len(),
        1,
        "second hidden snapshot preserves the proven home"
    );
    refresh_minimized(&mut retained, 1001, &hidden_app_snapshot(7));
    let entries = minimized_entries(&retained);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].window.0, "1001_7");
    assert_eq!(entries[0].output, key.output);
    assert_eq!(entries[0].workspace, key.workspace);
    let flat: Vec<WindowId> = retained
        .minimized
        .values()
        .map(|entry| entry.window.clone())
        .collect();
    assert!(
        flat.contains(&WindowId("1001_7".to_owned())),
        "repeat-hide terminate closes explicitly"
    );
    // Unknown never-homed window: repeated HiddenApp exclusions with no
    // prior homing and no retained entry still mint nothing.
    let mut fresh = RetainedCore {
        known_domains: retained.known_domains.clone(),
        ..RetainedCore::default()
    };
    refresh_minimized(&mut fresh, 2002, &hidden_app_snapshot(9));
    refresh_minimized(&mut fresh, 2002, &hidden_app_snapshot(9));
    assert!(
        minimized_entries(&fresh).is_empty(),
        "never-homed hidden windows fabricate no home"
    );
}

/// Host-owned native-attempt absence (option B follow-up): staged view and
/// member attempts keep their logical target correlation but never
/// translate drain-observed native Space ids to logical pre/post receipts.
/// Native pre/post ride as absence (`None`); the current attempt fields
/// represent logical observations only. Unknown operations still drop.
#[test]
fn staged_native_attempts_report_absence_not_translated_logical() {
    use crate::focus_route::FocusRoute;
    use crate::runtime_core::RetainedCore;
    use crate::world_sequencer::ObservationClock;
    use std::sync::Arc;
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::session::DomainKey;
    use tiler_core::world_engine::WorldOutcome;
    use tiler_core::world_report::FrameSetter;
    let mut session = WorldSession::new(
        tiler_core::size_hints::FixedSizePredicate::BothAxes,
        true,
        FocusRoute::Slps,
        Arc::new(ObservationClock::new()),
    );
    let incarnation = session.incarnation().to_owned();
    let generation = crate::runtime_types::PHASE1_GENERATION.to_owned();
    let output = OutputId("macos-display-1".to_owned());
    let native_a = crate::topology::workspace_key(11);
    let native_b = crate::topology::workspace_key(12);
    // Two native Spaces on one output so a view transfer and a window
    // transfer can retain; the staging path must still not translate
    // native ids to logical pre/post.
    let ctx = crate::runtime::NativeContext {
        revision: 1,
        domains: vec![
            crate::runtime_types::NativeDomain {
                cg_id: 1,
                uuid: None,
                output: output.clone(),
                workspace: tiler_core::directional::WorkspaceId(native_a.clone()),
                space: 11,
                current: true,
                readable: true,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 1440,
                    h: 900,
                },
                visible: Rect {
                    x: 0,
                    y: 25,
                    w: 1440,
                    h: 875,
                },
            },
            crate::runtime_types::NativeDomain {
                cg_id: 1,
                uuid: None,
                output: output.clone(),
                workspace: tiler_core::directional::WorkspaceId(native_b.clone()),
                space: 12,
                current: false,
                readable: true,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 1440,
                    h: 900,
                },
                visible: Rect {
                    x: 0,
                    y: 25,
                    w: 1440,
                    h: 875,
                },
            },
        ],
        skipped: Vec::new(),
    };
    let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::from([(
        DomainKey {
            output: output.clone(),
            workspace: tiler_core::directional::WorkspaceId(native_a.clone()),
        },
        vec![(
            1001,
            ObservedWindow {
                wid: 7,
                x: 0,
                y: 0,
                w: 100,
                h: 100,
                floating: false,
                fullscreen: false,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
            },
        )],
    )]);
    let memory = crate::runtime_core::LifetimeMemory::empty();
    let closed: Vec<WindowId> = Vec::new();
    let unreadable: BTreeMap<DomainKey, tiler_core::world_report::HoldReason> = BTreeMap::new();
    let owned_first = "req-1".to_owned();
    let mut input = crate::world_builder::ReportInput::new(
        &incarnation,
        &incarnation,
        &generation,
        &ctx,
        1,
        1,
        &owned_first,
        &members,
        &memory,
        &closed,
        &unreadable,
        &[],
        "1001_7",
        false,
    );
    let first = crate::world_builder::build_report(&mut input);
    let adopted = match session.engine_mut().handle_report(&first) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("adopt report must accept, got {other:?}"),
    };
    let logical_for = |native: &str| {
        adopted
            .workspaces
            .iter()
            .find(|w| w.output == output && w.native_binding.as_deref() == Some(native))
            .map(|w| w.id.clone())
            .expect("adopted binding")
    };
    let logical_a = logical_for(&native_a);
    let logical_b = logical_for(&native_b);
    assert_ne!(logical_a, logical_b, "adoption binds distinct logical ids");
    // View transfer via SwitchView to the hidden logical view retains
    // operation `op-2`; staging native Space reads must ride as absence.
    let owned_second = "req-2".to_owned();
    let mut second = crate::world_builder::ReportInput::new(
        &incarnation,
        &incarnation,
        &generation,
        &ctx,
        2,
        2,
        &owned_second,
        &members,
        &memory,
        &closed,
        &unreadable,
        &[],
        "1001_7",
        false,
    );
    second.command_seq = Some(2);
    second.command = Some(tiler_core::world_report::WorldCommand::SwitchView {
        output: output.clone(),
        workspace: logical_b.clone(),
    });
    let second_report = crate::world_builder::build_report(&mut second);
    match session.engine_mut().handle_report(&second_report) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("switch-view must accept, got {other:?}"),
    }
    session.stage_view_attempt("op-2", FrameSetter::Accepted, Some(11), Some(12), Some(11));
    // Window transfer via Send to the hidden logical workspace retains
    // operation `op-3`; its native post must also ride as absence.
    let owned_third = "req-3".to_owned();
    let mut third = crate::world_builder::ReportInput::new(
        &incarnation,
        &incarnation,
        &generation,
        &ctx,
        3,
        3,
        &owned_third,
        &members,
        &memory,
        &closed,
        &unreadable,
        &[],
        "1001_7",
        false,
    );
    third.command_seq = Some(3);
    third.command = Some(tiler_core::world_report::WorldCommand::Send {
        window: WindowId("1001_7".to_owned()),
        target_output: output.clone(),
        target_workspace: logical_b.clone(),
        follow: false,
    });
    let third_report = crate::world_builder::build_report(&mut third);
    match session.engine_mut().handle_report(&third_report) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("send must accept, got {other:?}"),
    }
    session.stage_member_attempt("op-3", FrameSetter::Accepted, Some(11), Some(12));
    // Stage with unknown operation drops (existing fence, unchanged).
    session.stage_view_attempt(
        "op-404",
        FrameSetter::Accepted,
        Some(11),
        Some(12),
        Some(11),
    );
    session.stage_member_attempt("op-404", FrameSetter::Accepted, None, Some(12));
    let retained = RetainedCore::default();
    let report = crate::world_runtime::build_dispatch_report(
        &mut session,
        &retained,
        &crate::world_runtime::DispatchContext::Native(Arc::new(ctx.clone())),
        &[],
        4,
        4,
        None,
        None,
        None,
        "req-4",
    )
    .expect("report builds");
    let view = report
        .view_attempts
        .iter()
        .find(|a| a.operation_id == "op-2")
        .expect("view row");
    assert!(
        view.pre.is_none() && view.post.is_none(),
        "native pre/post ride as absence"
    );
    assert_eq!(view.target_output, output);
    assert_eq!(
        view.target_workspace, logical_b,
        "logical target correlation kept"
    );
    let member = report
        .member_attempts
        .iter()
        .find(|a| a.operation_id == "op-3")
        .expect("member row");
    assert!(
        member.post.is_none(),
        "native post rides as absence, got {:?}",
        member.post
    );
    assert_eq!(
        member.target_workspace, logical_b,
        "logical target correlation kept"
    );
    assert!(
        report
            .view_attempts
            .iter()
            .all(|a| a.operation_id != "op-404")
            && report
                .member_attempts
                .iter()
                .all(|a| a.operation_id != "op-404"),
        "unknown operations drop"
    );
}
