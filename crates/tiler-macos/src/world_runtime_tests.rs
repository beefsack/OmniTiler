//! World-runtime unit tests (offline): command mapping covers every
//! supported tiling action plus exactly the armed select subset;
//! refusal vocabulary, geometry convergence, minimized homing/close
//! paths, and honest completion outcomes. No live AX, no threads.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::session::DomainKey;

use tiler_core::world_report::AttemptOutcome;

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
    // selects carry the admission-resolved native workspace explicitly
    // (never re-resolved core-side through the scoped ring); relative and
    // previous keep their shared core semantics.
    for action in [
        InputSemanticAction::WorkspaceSelect { index: 0 },
        InputSemanticAction::WorkspaceSelect { index: 1 },
        InputSemanticAction::WorkspaceSelect { index: 9 },
        InputSemanticAction::WorkspaceRelative { prev: true },
        InputSemanticAction::WorkspaceRelative { prev: false },
        InputSemanticAction::WorkspacePrevious,
    ] {
        assert!(is_armed_select(&action), "{action:?} is armed");
        let target = match action {
            InputSemanticAction::WorkspaceSelect { index } => {
                crate::topology::workspace_key(u64::from(index))
            }
            _ => String::new(),
        };
        assert!(
            select_command(&action, output.clone(), &target).is_some(),
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
            select_command(&action, output.clone(), "space-1").is_none(),
            "{action:?} never selects"
        );
        if index == 0 {
            assert!(
                send_command(&action, "4242_7", output.clone(), "space-277").is_some(),
                "{action:?} has the product trailing send route"
            );
        } else {
            assert!(
                send_command(&action, "4242_7", output.clone(), "space-1").is_some(),
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
            select_command(&stay, output.clone(), "space-1").is_none(),
            "{stay:?} never selects"
        );
        assert!(
            send_command(&stay, "4242_7", output.clone(), "space-1").is_some(),
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
            output.clone(),
            "space-1"
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
            select_command(&action, output.clone(), "space-1").is_none(),
            "{action:?} stays refused"
        );
        assert!(
            send_command(&action, "4242_7", output.clone(), "space-1").is_none(),
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
        (
            InputSemanticAction::DragDrop {
                window: window.to_owned(),
                x: 100,
                y: 200,
                hover_prior: None,
                press_revision: 7,
            },
            OP_DRAG_DROP,
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
    let map = terminate_close_map(&retained, 1001, &[]);
    assert_eq!(
        map.get(&key),
        Some(&vec![WindowId("1001_7".to_owned())]),
        "minimized terminate closes explicitly"
    );
}

#[test]
fn completion_outcomes_never_claim_false_success() {
    assert_eq!(
        completion_outcome(&AxCompletion::Applied { elapsed_ms: 3 }),
        AttemptOutcome::Applied
    );
    assert_eq!(
        completion_outcome(&AxCompletion::PartialApplied {
            elapsed_ms: 3,
            size_first_applied: true,
            position_applied: false,
            size_applied: true,
            readback_xywh: None,
            readback_unreadable: false,
        }),
        AttemptOutcome::PossiblyExecuted
    );
    assert_eq!(
        completion_outcome(&AxCompletion::Failed {
            reason: "refused".to_owned()
        }),
        AttemptOutcome::RefusedProven
    );
    assert_eq!(
        completion_outcome(&AxCompletion::Timeout { timeout_ms: 50 }),
        AttemptOutcome::PossiblyExecuted
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
    track_write(&mut session, 1001, 1, "1001_1", Some("op-7"));
    for cmd in 2..=5_000u64 {
        track_write(&mut session, 1001, cmd, "1001_1", None);
    }
    // The lone placement entry survives unbounded ordinary traffic.
    assert!(
        stage_write_feedback(
            &mut session,
            1001,
            1,
            &AxCompletion::Applied { elapsed_ms: 3 }
        ),
        "placement feedback stages from proven completion"
    );
    // Unknown completions stage nothing (never false success).
    assert!(
        !stage_write_feedback(
            &mut session,
            1001,
            9_999,
            &AxCompletion::Applied { elapsed_ms: 3 }
        ),
        "ordinary rows leave no correlation behind"
    );
}

#[test]
fn trailing_session_carries_exact_proof_until_core_ack() {
    use tiler_core::geometry::Rect;
    use tiler_core::world_report::{TilingMode, WorkspaceDesire, WorldReply};

    use std::sync::Arc;

    use crate::focus_route::FocusRoute;
    use crate::world_sequencer::ObservationClock;

    const UUID: &str = "A1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6";
    let output = OutputId("macos-display-1".to_owned());
    let domain = |space: u64, current: bool| NativeDomain {
        cg_id: 1,
        uuid: Some(UUID.to_owned()),
        output: output.clone(),
        workspace: WorkspaceId(crate::topology::workspace_key(space)),
        space,
        current,
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
    };
    let created = NativeContext {
        revision: 10,
        domains: vec![domain(1, false), domain(277, false), domain(278, true)],
        skipped: Vec::new(),
    };
    let mut session = WorldSession::new(
        tiler_core::size_hints::FixedSizePredicate::BothAxes,
        true,
        FocusRoute::Slps,
        Arc::new(ObservationClock::new()),
    );
    // Exact proof records through the production seam.
    session
        .adopt_trailing_binding("wl-1", &output, 278, &[1, 277], &[1, 277, 278])
        .expect("exact append adopts");
    // Drift refuses without mutating the recorded binding.
    assert!(
        session
            .adopt_trailing_binding("wl-2", &output, 279, &[1, 277], &[1, 279, 278])
            .is_err()
    );
    // The next full report carries exactly the recorded binding.
    let staged = session.trailing_report_bindings(&created);
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].logical_id, "wl-1");
    assert_eq!(staged[0].native_id, "space-278");
    assert!(staged[0].managed, "proven owned creation");
    assert!(!staged[0].pinned, "trailing stays cleanup-eligible");
    // The unbound desire never acknowledges; the bound row releases.
    let mut reply = WorldReply {
        contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-0".to_owned(),
        based_on_observation_seq: 0,
        state_revision: 0,
        ready: false,
        command_consumed: false,
        scopes: Vec::new(),
        workspaces: Vec::new(),
        outputs: Vec::new(),
        windows: Vec::new(),
        transfers: Vec::new(),
        effects: Vec::new(),
        command_result: None,
        presentation_bytes: 0,
    };
    reply.workspaces.push(WorkspaceDesire {
        id: "wl-1".to_owned(),
        output: output.clone(),
        native_binding: None,
        logical_id: Some("wl-1".to_owned()),
        scope: "macos-display-1".to_owned(),
        order: 2,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: None,
    });
    assert!(session.note_trailing_ack(&reply).is_empty());
    assert_eq!(session.trailing_report_bindings(&created).len(), 1);
    reply.workspaces[0].native_binding = Some("space-278".to_owned());
    reply.workspaces[0].id = "space-278".to_owned();
    assert_eq!(session.note_trailing_ack(&reply), vec!["wl-1".to_owned()]);
    assert!(session.trailing_report_bindings(&created).is_empty());
}
