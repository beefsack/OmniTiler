//! World-runtime unit tests (offline): command mapping covers every
//! supported tiling action plus exactly the armed select subset;
//! refusal vocabulary, geometry convergence, minimized homing/close
//! paths, and honest completion outcomes. No live AX, no threads.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::session::DomainKey;

use crate::runtime::{InputSemanticAction, NativeContext, ObservedWindow, window_id};
use crate::runtime_core::{RetainedApp, RetainedCore, RosterEntry};
use crate::runtime_thread::{EngineJob, Inbox};
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
        (InputSemanticAction::ToggleMaximize, OP_TOGGLE_MAXIMIZE),
    ] {
        let (command, token) = operation_command(&action, &mut retained, None, &[])
            .unwrap_or_else(|| panic!("{action:?} has an operation route"));
        assert_eq!(token, op);
        assert!(
            tiler_core::world_report::valid_command(&command),
            "{action:?} carries a valid world command"
        );
        // Every operation shape names the execution-time core subject; no
        // observed foreground is substituted adapter-side.
        match &command {
            tiler_core::world_report::WorldCommand::FocusDirectional {
                window: tiler_core::world_report::FocusDirectionalSubject::CurrentFocus,
                ..
            }
            | tiler_core::world_report::WorldCommand::MoveDirectional {
                window: tiler_core::directional::WindowSubject::CurrentFocus,
                ..
            }
            | tiler_core::world_report::WorldCommand::Operation(
                tiler_core::boundary::CoreCommand::Resize {
                    window: tiler_core::directional::WindowSubject::CurrentFocus,
                    ..
                }
                | tiler_core::boundary::CoreCommand::ToggleFloat {
                    window: tiler_core::directional::WindowSubject::CurrentFocus,
                    ..
                }
                | tiler_core::boundary::CoreCommand::ToggleOrientation {
                    window: tiler_core::directional::WindowSubject::CurrentFocus,
                },
            ) => {}
            tiler_core::world_report::WorldCommand::ToggleMaximize {
                window: tiler_core::directional::WindowSubject::CurrentFocus,
            } => {}
            other => panic!("{action:?} must carry the current-focus subject, got {other:?}"),
        }
    }
    // Neutral current-workspace tiling toggle: the adapter resolves the
    // focused window's retained NATIVE home and translates it through the
    // published native→logical binding rows; the command names exactly
    // that one domain and the core flips the retained mode at execution.
    let mut toggle_retained = retained_with_window(4242, 7, &domain_key());
    toggle_retained.observed_focus = Some(WindowId(window_id(4242, 7)));
    let toggle_ctx = NativeContext {
        revision: 9,
        domains: vec![toggle_native_domain(11, true, true)],
        skipped: Vec::new(),
    };
    let native_key = domain_key();
    let bindings = [crate::pager::PagerWorkspaceBinding {
        output: native_key.output.0.clone(),
        workspace: "ws-11".to_owned(),
        native: Some(native_key.workspace.0.clone()),
    }];
    let (command, token) = operation_command(
        &InputSemanticAction::ToggleWorkspaceTiling,
        &mut toggle_retained,
        Some(&toggle_ctx),
        &bindings,
    )
    .expect("workspace toggle has an operation route");
    assert_eq!(token, OP_TOGGLE_WORKSPACE_TILING);
    assert_eq!(
        command,
        tiler_core::world_report::WorldCommand::ToggleWorkspaceTiling {
            output: native_key.output,
            workspace: WorkspaceId("ws-11".to_owned()),
        }
    );
    assert!(
        tiler_core::world_report::valid_command(&command),
        "toggle carries a valid world command"
    );
    // No binding row: honest refusal, never a native id or a substituted
    // domain (the live topology still corroborates the home).
    assert!(
        operation_command(
            &InputSemanticAction::ToggleWorkspaceTiling,
            &mut toggle_retained,
            Some(&toggle_ctx),
            &[],
        )
        .is_none(),
        "unbound native home never rides the operation route"
    );
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
            operation_command(&action, &mut retained, None, &[]).is_none(),
            "{action:?} is never an operation"
        );
    }
}

#[test]
fn workspace_tiling_toggle_resolver_prefers_the_focused_homed_domain() {
    // Focused native home wins over the topology fallback and translates
    // through the published binding rows (native `space-N` -> core logical
    // `ws-N`). The live topology must corroborate that home as current and
    // readable; the first binding row names a different domain to prove it
    // never leaks into the resolution.
    let ctx = NativeContext {
        revision: 9,
        domains: vec![toggle_native_domain(11, true, true)],
        skipped: Vec::new(),
    };
    let mut retained = retained_with_window(4242, 7, &domain_key());
    retained.observed_focus = Some(WindowId(window_id(4242, 7)));
    let bindings = [
        crate::pager::PagerWorkspaceBinding {
            output: "macos-display-1".to_owned(),
            workspace: "ws-other".to_owned(),
            native: Some("space-99".to_owned()),
        },
        crate::pager::PagerWorkspaceBinding {
            output: "macos-display-1".to_owned(),
            workspace: "ws-11".to_owned(),
            native: Some("space-11".to_owned()),
        },
    ];
    let command = toggle_workspace_tiling_command(&retained, Some(&ctx), &bindings)
        .expect("focused current readable home resolves");
    assert_eq!(
        command,
        tiler_core::world_report::WorldCommand::ToggleWorkspaceTiling {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("ws-11".to_owned()),
        }
    );
    assert!(tiler_core::world_report::valid_command(&command));
}

#[test]
fn workspace_tiling_toggle_resolver_refuses_an_unbound_focused_home() {
    // A focused window whose home is current and readable but has no
    // binding row is an honest translation refusal: the resolver never
    // falls through to another domain and never sends the native id.
    let ctx = NativeContext {
        revision: 9,
        domains: vec![toggle_native_domain(11, true, true)],
        skipped: Vec::new(),
    };
    let mut retained = retained_with_window(4242, 7, &domain_key());
    retained.observed_focus = Some(WindowId(window_id(4242, 7)));
    let command = toggle_workspace_tiling_command(&retained, Some(&ctx), &[]);
    assert_eq!(command, None);
}

#[test]
fn workspace_tiling_toggle_resolver_refuses_a_focused_home_without_context() {
    // No live native topology: there is no current workspace to prove, so
    // even a retained focused home and a matching binding row refuse.
    let mut retained = retained_with_window(4242, 7, &domain_key());
    retained.observed_focus = Some(WindowId(window_id(4242, 7)));
    let bindings = [crate::pager::PagerWorkspaceBinding {
        output: "macos-display-1".to_owned(),
        workspace: "ws-11".to_owned(),
        native: Some("space-11".to_owned()),
    }];
    assert_eq!(
        toggle_workspace_tiling_command(&retained, None, &bindings),
        None
    );
}

#[test]
fn workspace_tiling_toggle_resolver_refuses_a_hidden_focused_home() {
    // The focused window's retained home survives a Space switch in
    // `window_domains`, but the live topology reports it hidden: a
    // stale/hidden home never toggles (and never borrows another current
    // domain).
    let ctx = NativeContext {
        revision: 10,
        domains: vec![
            toggle_native_domain(11, false, true),
            toggle_native_domain(12, true, true),
        ],
        skipped: Vec::new(),
    };
    let mut retained = retained_with_window(4242, 7, &domain_key());
    retained.observed_focus = Some(WindowId(window_id(4242, 7)));
    let bindings = [
        crate::pager::PagerWorkspaceBinding {
            output: "macos-display-1".to_owned(),
            workspace: "ws-11".to_owned(),
            native: Some("space-11".to_owned()),
        },
        crate::pager::PagerWorkspaceBinding {
            output: "macos-display-1".to_owned(),
            workspace: "ws-12".to_owned(),
            native: Some("space-12".to_owned()),
        },
    ];
    assert_eq!(
        toggle_workspace_tiling_command(&retained, Some(&ctx), &bindings),
        None,
        "a stale/hidden focused home never toggles other workspace"
    );
}

#[test]
fn workspace_tiling_toggle_resolver_refuses_an_unhomed_focused_window() {
    // A focused window with no retained homing is an honest refusal even
    // when the topology offers exactly one current readable domain: the
    // fallback applies only when NO window is focused.
    let ctx = NativeContext {
        revision: 9,
        domains: vec![toggle_native_domain(11, true, true)],
        skipped: Vec::new(),
    };
    let bindings = [crate::pager::PagerWorkspaceBinding {
        output: "macos-display-1".to_owned(),
        workspace: "ws-11".to_owned(),
        native: Some("space-11".to_owned()),
    }];
    let retained = RetainedCore {
        observed_focus: Some(WindowId(window_id(4242, 7))),
        ..RetainedCore::default()
    };
    assert_eq!(
        toggle_workspace_tiling_command(&retained, Some(&ctx), &bindings),
        None,
        "an unhomed focused window never borrows the current-domain fallback"
    );
}

#[test]
fn workspace_tiling_toggle_resolver_falls_back_to_the_unique_current_readable_domain() {
    // No observed focus: exactly one current AND readable native domain
    // resolves (hidden and unreadable rows never qualify), translated
    // through the binding rows.
    let ctx = NativeContext {
        revision: 9,
        domains: vec![
            toggle_native_domain(11, true, true),
            toggle_native_domain(12, false, true),
            toggle_native_domain(13, true, false),
        ],
        skipped: Vec::new(),
    };
    let bindings = [crate::pager::PagerWorkspaceBinding {
        output: "macos-display-1".to_owned(),
        workspace: "ws-11".to_owned(),
        native: Some("space-11".to_owned()),
    }];
    let command = toggle_workspace_tiling_command(&RetainedCore::default(), Some(&ctx), &bindings)
        .expect("unique current readable domain resolves");
    assert_eq!(
        command,
        tiler_core::world_report::WorldCommand::ToggleWorkspaceTiling {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("ws-11".to_owned()),
        }
    );
    assert!(tiler_core::world_report::valid_command(&command));
}

#[test]
fn workspace_tiling_toggle_resolver_refuses_zero_current_readable_domains() {
    let ctx = NativeContext {
        revision: 9,
        domains: vec![
            toggle_native_domain(12, false, true),
            toggle_native_domain(13, true, false),
        ],
        skipped: Vec::new(),
    };
    let bindings = [crate::pager::PagerWorkspaceBinding {
        output: "macos-display-1".to_owned(),
        workspace: "ws-12".to_owned(),
        native: Some("space-12".to_owned()),
    }];
    assert_eq!(
        toggle_workspace_tiling_command(&RetainedCore::default(), Some(&ctx), &bindings),
        None
    );
}

#[test]
fn workspace_tiling_toggle_resolver_refuses_ambiguous_current_domains() {
    let ctx = NativeContext {
        revision: 9,
        domains: vec![
            toggle_native_domain(11, true, true),
            toggle_native_domain(12, true, true),
        ],
        skipped: Vec::new(),
    };
    let bindings = [
        crate::pager::PagerWorkspaceBinding {
            output: "macos-display-1".to_owned(),
            workspace: "ws-11".to_owned(),
            native: Some("space-11".to_owned()),
        },
        crate::pager::PagerWorkspaceBinding {
            output: "macos-display-1".to_owned(),
            workspace: "ws-12".to_owned(),
            native: Some("space-12".to_owned()),
        },
    ];
    assert_eq!(
        toggle_workspace_tiling_command(&RetainedCore::default(), Some(&ctx), &bindings),
        None,
        "two current readable domains never guess"
    );
}

#[test]
fn workspace_tiling_toggle_resolver_refuses_without_context_or_focus() {
    let bindings = [crate::pager::PagerWorkspaceBinding {
        output: "macos-display-1".to_owned(),
        workspace: "ws-11".to_owned(),
        native: Some("space-11".to_owned()),
    }];
    assert_eq!(
        toggle_workspace_tiling_command(&RetainedCore::default(), None, &bindings),
        None
    );
}

fn toggle_native_domain(space: u64, current: bool, readable: bool) -> NativeDomain {
    NativeDomain {
        cg_id: 1,
        uuid: None,
        output: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId(format!("space-{space}")),
        space,
        current,
        readable,
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
        completion_setter(&AxCompletion::Applied {
            elapsed_ms: 3,
            readback_xywh: None
        }),
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
    // A pre-call fence maps like a refusal but is short-circuited by the
    // staging path, so it never becomes a row.
    assert_eq!(
        completion_setter(&AxCompletion::FencedNoCall {
            reason: "fence refused".to_owned()
        }),
        FrameSetter::Refused
    );
    assert_eq!(
        completion_setter(&AxCompletion::Timeout { timeout_ms: 50 }),
        FrameSetter::Threw
    );
    // The immediate post-write readback rides both settled and partial
    // completions; a genuinely unreadable one reports none, never guessed.
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
        completion_readback(&AxCompletion::Applied {
            elapsed_ms: 3,
            readback_xywh: Some((1, 2, 30, 40))
        }),
        Some(tiler_core::geometry::Rect {
            x: 1,
            y: 2,
            w: 30,
            h: 40
        })
    );
    assert_eq!(
        completion_readback(&AxCompletion::Applied {
            elapsed_ms: 3,
            readback_xywh: None
        }),
        None
    );
    // A non-carriable AppKit readback (outside the wire rectangle) rides
    // as unreadable: the staging choke point must never emit a rect the
    // report validator would reject.
    assert_eq!(
        completion_readback(&AxCompletion::Applied {
            elapsed_ms: 3,
            readback_xywh: Some((0, 0, 0, 40))
        }),
        None,
        "a non-carriable readback is unreadable, never clamped"
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
fn ordinary_frame_traffic_never_occupies_placement_correlation() {
    use std::sync::Arc;

    use crate::focus_route::FocusRoute;
    use crate::world_sequencer::ObservationClock;
    // Placement rows (explicit `frame_operation` tokens) settle one-shot
    // core tokens, so they keep their own correlation space: ordinary
    // layout traffic correlates in the separate layout lane and must
    // neither insert into nor evict that space.
    let mut session = WorldSession::new(
        false,
        FocusRoute::Slps,
        Arc::new(ObservationClock::new()),
        &[],
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
        track_layout_write(
            &mut session,
            1001,
            cmd,
            "1001_2",
            tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
        );
    }
    assert!(
        session.pending_layout_writes.len() <= FRAME_CORRELATION_CAP,
        "the layout lane is bounded independently"
    );
    // The lone placement entry survives unbounded ordinary traffic. The
    // retained app generation fences stale completions: the live
    // generation stages, a relaunched PID drops.
    let retained = retained_with_window(1001, 7, &domain_key());
    let attempt_ms = Some(crate::input_eligibility::now_ms());
    assert!(
        stage_write_feedback(
            &mut session,
            &retained,
            1001,
            1,
            1,
            &AxCompletion::Applied {
                elapsed_ms: 3,
                readback_xywh: None
            },
            attempt_ms,
        ),
        "placement feedback stages from proven completion"
    );
    assert_eq!(
        session.pending_frames["1001_1"].purpose,
        tiler_core::world_report::FramePurpose::FloatPlacement {
            operation_id: "op-7".to_owned(),
            prewrite: None,
        },
        "placement lane keeps the token purpose"
    );
    // Unknown completions stage nothing (never false success).
    assert!(
        !stage_write_feedback(
            &mut session,
            &retained,
            1001,
            1,
            9_999,
            &AxCompletion::Applied {
                elapsed_ms: 3,
                readback_xywh: None
            },
            attempt_ms,
        ),
        "unknown commands leave no correlation behind"
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
            &AxCompletion::Applied {
                elapsed_ms: 3,
                readback_xywh: None
            },
            attempt_ms,
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
        false,
        FocusRoute::Slps,
        Arc::new(ObservationClock::new()),
        &[],
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

/// Host-owned native-attempt namespace (option B follow-up): staged view and
/// member attempts keep their logical target correlation but never
/// translate drain-observed native Space ids to logical pre/post receipts.
/// Native pre/post/baseline ride in the native namespace (`WorkspaceRef::Native`);
/// the logical observation fields stay out of the translation. Unknown
/// operations still drop.
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
        true,
        FocusRoute::Slps,
        Arc::new(ObservationClock::new()),
        &[],
    );
    let incarnation = session.incarnation().to_owned();
    let generation = session.sequencer().bound_generation();
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
    // Explicit synthetic ordinary captures for `1001_7`: AXWindow /
    // AXStandardWindow, non-modal, resizable, base layer, regular app,
    // host-visible. Same vocabulary as the workspace_send/managed lanes;
    // without this the builder defaults to null native admission facts and
    // the Send carrier never admits.
    let raw: BTreeMap<String, crate::runtime_types::WindowRawFacts> = BTreeMap::from([(
        "1001_7".to_owned(),
        crate::runtime_types::WindowRawFacts {
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            modal: Some(false),
            resizable: Some(true),
            layer: Some(0),
            app_id: Some("com.example.app".to_owned()),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
            host_hidden: Some(false),
        },
    )]);
    let owned_first = "req-1".to_owned();
    let mut input = crate::world_builder::ReportInput::new(
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
    input.raw = &raw;
    let first = crate::world_builder::build_report(&mut input);
    let adopted = match tiler_protocol::world_admission::handle_report_checked(
        session.engine_mut(),
        &first,
        &[],
    )
    .outcome
    {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("adopt report must accept, got {other:?}"),
    };
    assert!(
        adopted.windows.iter().any(|w| w.lifetime.0 == "1001_7"
            && w.frame_authority == tiler_core::world_report::ScopeAuthority::Controlled),
        "1001_7 admitted Controlled before send"
    );
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
    // View transfer via the v4 direct selection (`SwitchView` is refused by
    // `valid_command_v4`) to the hidden logical view retains operation
    // `op-2`; staging native Space reads ride the native keys.
    let owned_second = "req-2".to_owned();
    let mut second = crate::world_builder::ReportInput::new(
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
    second.raw = &raw;
    second.command_seq = Some(2);
    second.command = Some(tiler_core::world_report::WorldCommand::SelectWorkspace {
        output: output.clone(),
        workspace: WorkspaceId(logical_b.clone()),
    });
    let second_report = crate::world_builder::build_report(&mut second);
    match tiler_protocol::world_admission::handle_report_checked(
        session.engine_mut(),
        &second_report,
        &[],
    )
    .outcome
    {
        WorldOutcome::Accepted(_) => {}
        other => panic!("switch-view must accept, got {other:?}"),
    }
    session.stage_view_attempt("op-2", FrameSetter::Accepted, Some(11), Some(12), Some(11));
    // Window transfer via Send to the hidden logical workspace retains
    // operation `op-3`; its native pre/post ride the native keys too.
    let owned_third = "req-3".to_owned();
    let mut third = crate::world_builder::ReportInput::new(
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
    third.raw = &raw;
    third.command_seq = Some(3);
    third.command = Some(tiler_core::world_report::WorldCommand::Send {
        window: tiler_core::directional::WindowSubject::Window(WindowId("1001_7".to_owned())),
        target_output: output.clone(),
        target_workspace: WorkspaceId(logical_b.clone()),
        follow: false,
    });
    let third_report = crate::world_builder::build_report(&mut third);
    match tiler_protocol::world_admission::handle_report_checked(
        session.engine_mut(),
        &third_report,
        &[],
    )
    .outcome
    {
        WorldOutcome::Accepted(_) => {}
        other => panic!("send must accept, got {other:?}"),
    };
    assert!(
        session
            .engine_mut()
            .retained_transfers()
            .into_iter()
            .any(|t| t.operation_id == "op-3"
                && matches!(
                    t.kind,
                    tiler_core::world_report::TransferKind::Window(ref w)
                    if w.0 == "1001_7"
                )),
        "op-3 window transfer retained before staging"
    );
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
    // v4 carries the native Space reads in the native namespace (never a
    // logical translation): pre/post/baseline ride as Native refs.
    let native_pre = tiler_core::world_report::WorkspaceRef::Native {
        output: output.clone(),
        workspace: tiler_core::ids::NativeWorkspaceId::parse(&native_a)
            .expect("valid native workspace"),
    };
    let native_post = tiler_core::world_report::WorkspaceRef::Native {
        output: output.clone(),
        workspace: tiler_core::ids::NativeWorkspaceId::parse(&native_b)
            .expect("valid native workspace"),
    };
    assert_eq!(view.pre, Some(native_pre.clone()), "view pre rides native");
    assert_eq!(
        view.post,
        Some(native_post.clone()),
        "view post rides native"
    );
    assert_eq!(
        view.baseline,
        Some(native_pre.clone()),
        "view baseline rides native"
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
    assert_eq!(member.pre, Some(native_pre), "member pre rides native");
    assert_eq!(member.post, Some(native_post), "member post rides native");
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

#[test]
fn staged_appended_sid_never_claims_causal_created_identity() {
    use std::sync::Arc;

    use tiler_core::directional::OutputId;
    use tiler_core::world_report::FrameSetter;

    use crate::focus_route::FocusRoute;
    use crate::runtime_core::RetainedCore;
    use crate::world_sequencer::ObservationClock;

    // M5: an observed appended order alone is not causal proof of what the
    // create leg made. The production v4 attempt keeps `created` null even
    // when the legacy drain exposes an appended candidate SID.
    let mut session = WorldSession::new(
        true,
        FocusRoute::Slps,
        Arc::new(ObservationClock::new()),
        &[],
    );
    let output = OutputId("macos-display-1".to_owned());
    session.stage_native_creation(
        "op-9",
        "logical-9",
        output.clone(),
        FrameSetter::Accepted,
        &[11, 12],
        &[11, 12, 13],
        Some(13),
        7,
    );
    let retained = RetainedCore::default();
    let report = crate::world_runtime::build_dispatch_report(
        &mut session,
        &retained,
        &crate::world_runtime::DispatchContext::Synthetic,
        &[],
        1,
        1,
        None,
        None,
        None,
        "req-1",
    )
    .expect("synthetic report builds");
    assert_eq!(
        report.creation_attempts.len(),
        1,
        "staged creation attempt rides the report"
    );
    let row = &report.creation_attempts[0];
    assert_eq!(row.operation_id, "op-9");
    assert_eq!(row.logical_workspace, "logical-9");
    assert_eq!(row.target_output, output);
    assert_eq!(row.setter, FrameSetter::Accepted);
    assert_eq!(
        row.order_before,
        vec![
            crate::topology::workspace_key(11),
            crate::topology::workspace_key(12)
        ],
        "genuine pre-order preserved"
    );
    assert_eq!(
        row.order_after,
        vec![
            crate::topology::workspace_key(11),
            crate::topology::workspace_key(12),
            crate::topology::workspace_key(13)
        ],
        "genuine post-order preserved"
    );
    assert!(
        row.created_native.is_none(),
        "appended observed SID never claims causal created identity"
    );
    assert_eq!(row.attempt_ms, 7, "attempt identity preserved");

    // Codec seam: the production receipt rule maps `None` to explicit null
    // (never absent), and the null round-trips back to `None`.
    use tiler_protocol::world_protocol::{CreationReceiptWire, decode_report_v4, encode_report_v4};
    let decoded = crate::world_dispatch::decoded_report_for_encode(&report);
    let wire = encode_report_v4(&decoded).expect("report encodes");
    let back = decode_report_v4(&wire).expect("report decodes");
    assert_eq!(
        back.creation_receipts,
        vec![CreationReceiptWire::Null],
        "absent causal proof rides explicit null"
    );
    assert!(
        back.report.creation_attempts[0].created_native.is_none(),
        "null decodes to no causal identity"
    );
}

/// Restart fencing: two `WorldSession` constructions in one process (the
/// production restart shape: one engine thread dies, a new one starts under
/// the same PID) must mint distinct adapter/generation pairs through the
/// real handshake route, and the new session must refuse the old session's
/// reply, report, and settings. Otherwise queued replies or settings from
/// the dead session could actuate under the new one.
#[test]
fn restart_sessions_mint_distinct_tokens_and_fence_old_traffic() {
    use std::sync::Arc;

    use tiler_core::world_engine::WorldOutcome;
    use tiler_core::world_report::{WORLD_CONTRACT_VERSION_V4, WorldPresentationV4, WorldReplyV4};

    use crate::focus_route::FocusRoute;
    use crate::runtime_core::RetainedCore;
    use crate::world_sequencer::{ObservationClock, ReplyVerdict};

    fn test_session() -> WorldSession {
        WorldSession::new(
            false,
            FocusRoute::Slps,
            Arc::new(ObservationClock::new()),
            &[],
        )
    }

    fn stale_reply(
        incarnation: &str,
        generation: &str,
        request_id: &str,
        observation_seq: u64,
    ) -> WorldReplyV4 {
        WorldReplyV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: incarnation.to_owned(),
            core_generation: generation.to_owned(),
            request_id: request_id.to_owned(),
            based_on_observation_seq: observation_seq,
            state_revision: 1,
            ready: true,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            focus_request: None,
            drag_preview: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            warp_request: None,
            presentation: WorldPresentationV4 {
                foreground: String::new(),
                rev: 0,
                suspended_output: None,
                border: tiler_core::world_report::BorderPresentationV4 {
                    visible: false,
                    reason: None,
                    subject: None,
                    outsets: None,
                    width: None,
                    radius: None,
                    witness: None,
                },
                underlay: tiler_core::world_presentation::PresentationUnderlay::Hidden {
                    base_revision: None,
                    reason: tiler_core::world_presentation::GroupHideReason::NoGroup,
                },
                pager: Vec::new(),
            },
            intent_desired: Vec::new(),
            switch_settled: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    let mut old = test_session();
    let mut new = test_session();

    // Distinct identity pair across the restart.
    assert_ne!(
        old.incarnation(),
        new.incarnation(),
        "restart mints a fresh adapter incarnation"
    );
    let old_generation = old.sequencer().bound_generation();
    let new_generation = new.sequencer().bound_generation();
    assert_ne!(
        old_generation, new_generation,
        "restart mints a fresh core generation"
    );
    let old_incarnation = old.incarnation().to_owned();
    let new_incarnation = new.incarnation().to_owned();

    // The old session's reply (old tokens, new flight id) refuses on the
    // new sequencer instead of actuating.
    let ticket = new
        .sequencer()
        .try_begin_request(None)
        .expect("flight opens");
    match new.sequencer().accept_reply(&stale_reply(
        &old_incarnation,
        &old_generation,
        &ticket.request_id,
        ticket.observation_seq,
    )) {
        ReplyVerdict::Refused { .. } => {}
        ReplyVerdict::Applied(_) => panic!("old-session reply must not apply after restart"),
    }
    // Either half of the pair fences alone: the new incarnation with the
    // old generation still refuses.
    match new.sequencer().accept_reply(&stale_reply(
        &new_incarnation,
        &old_generation,
        &ticket.request_id,
        ticket.observation_seq,
    )) {
        ReplyVerdict::Refused { reason } => assert_eq!(reason, "generation-mismatch"),
        ReplyVerdict::Applied(_) => panic!("old generation must not apply under a new incarnation"),
    }
    // The new session still converges on its own tokens.
    new.sequencer().clear_flight();
    let fresh = new
        .sequencer()
        .try_begin_request(None)
        .expect("fresh flight opens");
    match new.sequencer().accept_reply(&stale_reply(
        &new_incarnation,
        &new_generation,
        &fresh.request_id,
        fresh.observation_seq,
    )) {
        ReplyVerdict::Applied(applied) => assert_eq!(applied.request_id, fresh.request_id),
        ReplyVerdict::Refused { reason } => panic!("own tokens must apply: {reason}"),
    }

    // The old session's report (built through the real dispatch route, so
    // the shape is production-valid) is admitted by its own engine but
    // rejected by the new engine.
    let old_report = build_dispatch_report(
        &mut old,
        &RetainedCore::default(),
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
        matches!(
            tiler_protocol::world_admission::handle_report_checked(
                old.engine_mut(),
                &old_report,
                &[]
            )
            .outcome,
            WorldOutcome::Accepted(_)
                | WorldOutcome::Duplicate(_)
                | WorldOutcome::CommandConsumed(_)
        ),
        "old report is well-formed for its own engine"
    );
    assert!(
        matches!(
            tiler_protocol::world_admission::handle_report_checked(
                new.engine_mut(),
                &old_report,
                &[]
            )
            .outcome,
            WorldOutcome::Rejected { .. }
        ),
        "old report must not be admitted after restart"
    );

    // The old session's settings envelope never reaches new settings
    // state, while the new session's own revision routes.
    assert!(
        crate::world_connection::apply_settings_revision(
            new.engine_mut(),
            &old_incarnation,
            &old_generation,
            2,
            &[],
        )
        .is_none(),
        "old settings envelope must not route after restart"
    );
    assert!(
        crate::world_connection::apply_settings_revision(
            new.engine_mut(),
            &new_incarnation,
            &new_generation,
            2,
            &[],
        )
        .is_some(),
        "own settings revision still routes"
    );
}

/// Same-PID restart with PID reuse: the identity pair must come solely from
/// the fresh CSPRNG nonce, never from the PID (PID reuse across process
/// restarts could otherwise reproduce the pair). Two sessions built over
/// distinct caller-supplied nonces — the deterministic stand-in for two
/// successive fresh-nonce mints under one reused PID — bind distinct pairs through the real
/// handshake route and fence each other's traffic.
#[test]
fn reused_pid_shape_with_distinct_nonces_fences_old_traffic() {
    use std::sync::Arc;

    use tiler_core::world_engine::WorldOutcome;

    use crate::focus_route::FocusRoute;
    use crate::runtime_core::RetainedCore;
    use crate::world_sequencer::ObservationClock;

    fn fixed_session(nonce: &str) -> WorldSession {
        WorldSession::new_with_tokens(
            Some((format!("macos-{nonce}"), format!("phase-1-{nonce}"))),
            false,
            FocusRoute::Slps,
            Arc::new(ObservationClock::new()),
            &[],
        )
    }

    let mut old = fixed_session("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let mut new = fixed_session("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    assert!(old.connection_bound(), "fixed tokens bind");
    assert!(new.connection_bound(), "fixed tokens bind");
    assert_ne!(
        old.incarnation(),
        new.incarnation(),
        "distinct nonces mint distinct incarnations under one reused PID"
    );
    let old_generation = old.sequencer().bound_generation();
    let new_generation = new.sequencer().bound_generation();
    assert_ne!(
        old_generation, new_generation,
        "distinct nonces mint distinct generations under one reused PID"
    );

    // The old session's report (production-valid shape) is admitted by its
    // own engine but rejected by the new engine.
    let old_report = build_dispatch_report(
        &mut old,
        &RetainedCore::default(),
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
        matches!(
            tiler_protocol::world_admission::handle_report_checked(
                old.engine_mut(),
                &old_report,
                &[]
            )
            .outcome,
            WorldOutcome::Accepted(_)
                | WorldOutcome::Duplicate(_)
                | WorldOutcome::CommandConsumed(_)
        ),
        "old report is well-formed for its own engine"
    );
    assert!(
        matches!(
            tiler_protocol::world_admission::handle_report_checked(
                new.engine_mut(),
                &old_report,
                &[]
            )
            .outcome,
            WorldOutcome::Rejected { .. }
        ),
        "old report must not be admitted after restart under a reused PID"
    );

    // The old session's settings envelope never reaches new settings state.
    let old_incarnation = old.incarnation().to_owned();
    assert!(
        crate::world_connection::apply_settings_revision(
            new.engine_mut(),
            &old_incarnation,
            &old_generation,
            2,
            &[],
        )
        .is_none(),
        "old settings envelope must not route after restart under a reused PID"
    );
}

/// CSPRNG failure stays fail-closed: the `None` token source builds an
/// unbound session (no handshake, no bind) whose own engine rejects even
/// directly built reports and whose settings route never opens. The
/// dispatch route additionally refuses via `connection_bound` before any
/// report builds (see `dispatch_report`'s `refused-no-connection` gate).
#[test]
fn entropy_failure_session_stays_unbound_and_admits_nothing() {
    use std::sync::Arc;

    use tiler_core::world_engine::WorldOutcome;

    use crate::focus_route::FocusRoute;
    use crate::runtime_core::RetainedCore;
    use crate::world_sequencer::ObservationClock;

    fn unbound_session() -> WorldSession {
        WorldSession::new_with_tokens(
            None,
            false,
            FocusRoute::Slps,
            Arc::new(ObservationClock::new()),
            &[],
        )
    }

    let mut session = unbound_session();
    assert!(!session.connection_bound(), "entropy failure never binds");
    assert!(
        session.initial_settings_reply().is_none(),
        "entropy failure carries no settings"
    );
    // A second entropy-failure session is equally unbound: sentinels never
    // bind, so sharing them claims no lifetime.
    let other = unbound_session();
    assert!(!other.connection_bound(), "entropy failure never binds");

    // No settings envelope routes on the unbound engine.
    let incarnation = session.incarnation().to_owned();
    let generation = session.sequencer().bound_generation();
    assert!(
        crate::world_connection::apply_settings_revision(
            session.engine_mut(),
            &incarnation,
            &generation,
            2,
            &[],
        )
        .is_none(),
        "unbound settings envelope must not route"
    );

    // Even a directly built report is rejected owner-side: the unbound
    // engine minted no connection owner, so nothing admits or actuates.
    let report = build_dispatch_report(
        &mut session,
        &RetainedCore::default(),
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
        matches!(
            tiler_protocol::world_admission::handle_report_checked(
                session.engine_mut(),
                &report,
                &[]
            )
            .outcome,
            WorldOutcome::Rejected { .. }
        ),
        "unbound session must reject even its own report"
    );
}

/// One current native domain matching [`domain_key`]'s output/workspace, so
/// a retained homed member is emitted as a window row.
fn cursor_ctx() -> NativeContext {
    NativeContext {
        revision: 3,
        domains: vec![NativeDomain {
            cg_id: 1,
            uuid: None,
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
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
        }],
        skipped: Vec::new(),
    }
}

fn cursor_session() -> WorldSession {
    WorldSession::new(
        true,
        crate::focus_route::FocusRoute::Slps,
        std::sync::Arc::new(crate::world_sequencer::ObservationClock::new()),
        &[],
    )
}

#[test]
fn cursor_hit_attributes_only_live_homed_members() {
    let domain = domain_key();
    let retained = retained_with_window(1001, 1, &domain);
    assert_eq!(
        attribute_cursor_hit(&retained, Some((1001, 1))),
        Some(WindowId("1001_1".to_owned())),
        "exactly homed live member attributes"
    );
    // Chrome/desk or unreadable hit: the hit test honestly returns nothing.
    assert_eq!(attribute_cursor_hit(&retained, None), None);
    // Foreign app: no live retained generation under that pid.
    assert_eq!(attribute_cursor_hit(&retained, Some((4242, 1))), None);
    // Unhomed window of a known app.
    assert_eq!(attribute_cursor_hit(&retained, Some((1001, 2))), None);
    // Stale homing without a live app row never attributes (a relaunch
    // clears retained rows, so an old (pid, wid) cannot resurrect).
    let mut stale = RetainedCore::default();
    stale.window_domains.insert(window_id(1001, 1), domain);
    assert_eq!(attribute_cursor_hit(&stale, Some((1001, 1))), None);
}

#[test]
fn stored_cursor_sample_persists_across_reports_without_claiming_focus() {
    let domain = domain_key();
    let retained = retained_with_window(1001, 1, &domain);
    let hit = WindowId("1001_1".to_owned());
    let mut session = cursor_session();
    assert!(session.note_cursor_sample(CursorSample {
        seq: 7,
        point: (40, 50),
        hit: Some(hit.clone()),
    }));
    // A repeated/older observer sequence never replaces the sample.
    assert!(!session.note_cursor_sample(CursorSample {
        seq: 7,
        point: (0, 0),
        hit: None,
    }));
    assert_eq!(
        session.cursor_sample(),
        Some(&CursorSample {
            seq: 7,
            point: (40, 50),
            hit: Some(hit.clone()),
        })
    );

    let dispatch = DispatchContext::Native(std::sync::Arc::new(cursor_ctx()));
    // Two successive reports both carry the same sample: it is never
    // consumed by a build (a resting pointer keeps its hover memory).
    for request in 1..=2 {
        let report = build_dispatch_report(
            &mut session,
            &retained,
            &dispatch,
            &[],
            request,
            request,
            None,
            None,
            None,
            &format!("req-{request}"),
        )
        .expect("native context reports");
        assert_eq!(report.cursor, Some((40, 50)));
        assert_eq!(report.cursor_hit, Some(hit.clone()));
        // Hover is observation, never a focus claim.
        assert_eq!(report.focused_global, "");
        assert!(!report.foreground_known);
        assert_eq!(report.focus_changes, None);
    }
}

#[test]
fn cursor_hit_withheld_when_the_window_is_not_emitted() {
    let domain = domain_key();
    let retained = retained_with_window(1001, 1, &domain);
    let mut session = cursor_session();
    // Attribution happened at sample time for a window that is no longer
    // part of the report's emitted rows (e.g. closed since): the pair is
    // withheld, but the stored sample survives for the next report.
    assert!(session.note_cursor_sample(CursorSample {
        seq: 3,
        point: (5, 6),
        hit: Some(WindowId("1001_2".to_owned())),
    }));
    let dispatch = DispatchContext::Native(std::sync::Arc::new(cursor_ctx()));
    let report = build_dispatch_report(
        &mut session,
        &retained,
        &dispatch,
        &[],
        1,
        1,
        None,
        None,
        None,
        "req-1",
    )
    .expect("native context reports");
    assert_eq!(report.cursor, None);
    assert_eq!(report.cursor_hit, None);
    assert!(
        session.cursor_sample().is_some(),
        "sample is never consumed"
    );
}

fn accepted_reply(
    session: &mut WorldSession,
    report: &tiler_core::world_report::WorldReportV4,
) -> tiler_core::world_report::WorldReplyV4 {
    match tiler_protocol::world_admission::handle_report_checked(session.engine_mut(), report, &[])
        .outcome
    {
        tiler_core::world_engine::WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted reply, got {other:?}"),
    }
}

/// Builds the producer report exactly as the dispatch loop does, stamping
/// the real process clock (`monotonic_ms`) the way native context does.
fn next_report(
    session: &mut WorldSession,
    retained: &RetainedCore,
    seq: u64,
) -> tiler_core::world_report::WorldReportV4 {
    build_dispatch_report(
        session,
        retained,
        &DispatchContext::Native(std::sync::Arc::new(cursor_ctx())),
        &[],
        seq,
        seq,
        None,
        None,
        None,
        &format!("req-{seq}"),
    )
    .expect("native context reports")
}

/// Records an observed frame as the worker's post-apply write did: both the
/// app-row projection and the raw roster fact.
fn observe_frame(retained: &mut RetainedCore, pid: u32, wid: u32, frame: Rect) {
    let app = retained.apps.get_mut(&pid).expect("retained app");
    let observed = app
        .windows
        .iter_mut()
        .find(|window| window.wid == wid)
        .expect("retained window");
    observed.x = frame.x;
    observed.y = frame.y;
    observed.w = frame.w;
    observed.h = frame.h;
    if let Some(entry) = app.roster.get_mut(&wid) {
        entry.observed = Some(ObservedWindow {
            wid,
            x: frame.x,
            y: frame.y,
            w: frame.w,
            h: frame.h,
            floating: false,
            fullscreen: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        });
    }
}

/// Applies one owner observation the way the scoped-observation job does:
/// the app's sequence advances and the observed frame lands, so a report
/// built after it pairs attempts with facts no older than the write.
fn apply_owner_observation(retained: &mut RetainedCore, pid: u32, wid: u32, frame: Rect) {
    observe_frame(retained, pid, wid, frame);
    retained.apps.get_mut(&pid).expect("retained app").seq_last += 1;
}

/// Adds a second tiled member to the same workspace so the tiled
/// allocation genuinely changes.
fn add_second_member(retained: &mut RetainedCore, pid: u32, wid: u32) {
    let frame = Rect {
        x: 400,
        y: 0,
        w: 100,
        h: 100,
    };
    let window = ObservedWindow {
        wid,
        x: frame.x,
        y: frame.y,
        w: frame.w,
        h: frame.h,
        floating: false,
        fullscreen: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    };
    retained
        .window_domains
        .insert(window_id(pid, wid), domain_key());
    let app = retained.apps.get_mut(&pid).expect("retained app");
    app.windows.push(window);
    app.roster.insert(
        wid,
        RosterEntry {
            observed: Some(window),
            spaces: vec![11],
            fullscreen_meta: false,
            minimized_meta: false,
            hidden_meta: false,
            excluded: None,
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            modal: Some(false),
            resizable: Some(true),
            layer: Some(0),
            app_id: Some("com.example.app".to_owned()),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        },
    );
}

/// One plain tiled window in retained truth: the app-row projection the
/// report reads membership from plus the raw neutral facts the core needs
/// to classify the row as ordinary tiled (no native inference, no policy
/// echoes).
fn retained_with_tiled(pid: u32, wid: u32, frame: Rect) -> RetainedCore {
    let window = ObservedWindow {
        wid,
        x: frame.x,
        y: frame.y,
        w: frame.w,
        h: frame.h,
        floating: false,
        fullscreen: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    };
    let mut retained = RetainedCore::default();
    retained
        .window_domains
        .insert(window_id(pid, wid), domain_key());
    retained.apps.insert(
        pid,
        RetainedApp {
            generation: 1,
            seq_last: 1,
            cmd_counter: 0,
            windows: vec![window],
            focused_wid: Some(wid),
            roster: BTreeMap::from([(
                wid,
                RosterEntry {
                    observed: Some(window),
                    spaces: vec![11],
                    fullscreen_meta: false,
                    minimized_meta: false,
                    hidden_meta: false,
                    excluded: None,
                    role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                    subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                    modal: Some(false),
                    resizable: Some(true),
                    layer: Some(0),
                    app_id: Some("com.example.app".to_owned()),
                    activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                },
            )]),
            roster_incomplete: false,
            roster_cause: String::new(),
        },
    );
    retained
}

#[test]
fn presentation_suspends_for_unhomed_native_fullscreen_focus() {
    let (pid, wid) = (1001, 1);
    let mut retained = retained_with_tiled(
        pid,
        wid,
        Rect {
            x: 0,
            y: 0,
            w: 1440,
            h: 900,
        },
    );
    let focused = WindowId(window_id(pid, wid));
    retained.window_domains.remove(&focused.0);
    retained.observed_focus = Some(focused);
    retained.observed_focus_known = true;
    let app = retained.apps.get_mut(&pid).unwrap();
    app.windows.clear();
    let entry = app.roster.get_mut(&wid).unwrap();
    entry.spaces.clear();
    entry.fullscreen_meta = true;
    entry.observed.as_mut().unwrap().fullscreen = true;

    let mut session = cursor_session();
    let report = next_report(&mut session, &retained, 1);
    assert!(matches!(
        &report.presentation,
        tiler_core::world_report::PresentationRowV4::Valid(ingress)
            if ingress.foreground_fullscreen == Some(true)
    ));
    assert_eq!(
        accepted_reply(&mut session, &report).presentation.suspended_output,
        Some(domain_key().output)
    );
}

fn desire_row<'a>(
    reply: &'a tiler_core::world_report::WorldReplyV4,
    window: &str,
) -> &'a tiler_core::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("desire row for {window}"))
}

#[test]
fn controlled_layout_write_stages_one_attempt_for_core_policy() {
    // A Controlled (tokenless) frame command is a real native call: without
    // a `FramePurpose::Layout` attempt the shared clamp memory and burst
    // budget are unreachable on macOS (core `settle_frames` only consumes
    // reported attempts), so a natively clamped app is rewritten once per
    // unrelated report for as long as the desire stands.
    let mut session = cursor_session();
    let mut retained = retained_with_tiled(
        1001,
        7,
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
    );
    let report = next_report(&mut session, &retained, 1);
    assert!(
        tiler_core::world_report_v4_checks::validate_report_v4_reason(&report).is_none(),
        "producer report validates"
    );
    assert!(report.frames.is_empty(), "no write has happened yet");
    let reply = accepted_reply(&mut session, &report);
    let row = desire_row(&reply, "1001_7");
    assert_eq!(
        row.frame_authority,
        tiler_core::world_report::ScopeAuthority::Controlled
    );
    let desired = row.effective_frame.expect("controlled frame");

    // The adapter wrote that frame exactly once; the app's own toolkit
    // clamped the requested height by one point, which the bounded
    // post-write readback proves.
    let clamped = Rect {
        h: desired.h - 1,
        ..desired
    };
    let cmd_id = 1;
    track_layout_write(&mut session, 1001, cmd_id, "1001_7", desired);
    let called_ms = crate::input_eligibility::now_ms();
    assert!(
        stage_write_feedback(
            &mut session,
            &retained,
            1001,
            1,
            cmd_id,
            &AxCompletion::Applied {
                elapsed_ms: 3,
                readback_xywh: Some((clamped.x, clamped.y, clamped.w, clamped.h)),
            },
            Some(called_ms),
        ),
        "a Controlled layout write must stage one attempt for core policy"
    );
    // The worker's post-apply observation lands after the AX result.
    observe_frame(&mut retained, 1001, 7, clamped);

    let report = next_report(&mut session, &retained, 2);
    assert!(
        tiler_core::world_report_v4_checks::validate_report_v4_reason(&report).is_none(),
        "a layout attempt with its readback still passes the real validator"
    );
    assert_eq!(report.frames.len(), 1, "one attempt row, one window");
    let attempt = &report.frames[0];
    assert_eq!(attempt.window.0, "1001_7");
    assert_eq!(attempt.target, desired, "attempt carries the desired frame");
    assert_eq!(
        attempt.readback,
        Some(clamped),
        "the accepted-but-clamped write carries real readback"
    );
    assert_eq!(
        attempt.disposition,
        tiler_core::world_report::FrameSetter::Accepted
    );
    assert_eq!(
        attempt.purpose,
        tiler_core::world_report::FramePurpose::Layout
    );
    assert_eq!(
        attempt.attempt_ms,
        Some(called_ms),
        "attempt_ms is the process clock, never the per-call duration"
    );

    // Core learns the one-point clamp: the identical (desired, observed)
    // pair is held, so an unrelated report no longer rewrites it.
    let reply = accepted_reply(&mut session, &report);
    let row = desire_row(&reply, "1001_7");
    assert_eq!(
        row.frame_authority,
        tiler_core::world_report::ScopeAuthority::Hold(
            tiler_core::world_report::HoldReason::ConstraintClamp
        ),
        "a one-point native clamp is held instead of rewritten"
    );
    assert_eq!(row.effective_frame, Some(desired), "desire stays intact");

    // Unrelated reports (cursor heartbeat, then focus truth) hold the same
    // suppressed pair: no rewrite is authorized.
    assert!(session.note_cursor_sample(CursorSample {
        seq: 1,
        point: (5, 6),
        hit: Some(WindowId("1001_7".to_owned())),
    }));
    let report = next_report(&mut session, &retained, 3);
    assert!(report.frames.is_empty(), "no attempt staged for a hold");
    let reply = accepted_reply(&mut session, &report);
    assert_eq!(
        desire_row(&reply, "1001_7").frame_authority,
        tiler_core::world_report::ScopeAuthority::Hold(
            tiler_core::world_report::HoldReason::ConstraintClamp
        ),
        "a cursor heartbeat never re-authorizes the clamped write"
    );
    retained.observed_focus = Some(WindowId("1001_7".to_owned()));
    let report = next_report(&mut session, &retained, 4);
    let reply = accepted_reply(&mut session, &report);
    assert_eq!(
        desire_row(&reply, "1001_7").frame_authority,
        tiler_core::world_report::ScopeAuthority::Hold(
            tiler_core::world_report::HoldReason::ConstraintClamp
        ),
        "a focus-only report never re-authorizes the clamped write"
    );

    // A genuine observed change (the app moves) re-arms the retry: the
    // suppressed pair is gone, so the Controlled lane authorizes again.
    let moved = Rect {
        x: clamped.x + 17,
        ..clamped
    };
    observe_frame(&mut retained, 1001, 7, moved);
    let report = next_report(&mut session, &retained, 5);
    let reply = accepted_reply(&mut session, &report);
    assert_eq!(
        desire_row(&reply, "1001_7").frame_authority,
        tiler_core::world_report::ScopeAuthority::Controlled,
        "a genuine observation change retries the write"
    );

    // A genuine desired change (a sibling joins the workspace) also
    // re-arms: the new target is a fresh pair, never the suppressed one.
    let report = next_report(&mut session, &retained, 6);
    let reply = accepted_reply(&mut session, &report);
    let before = desire_row(&reply, "1001_7")
        .effective_frame
        .expect("controlled frame");
    add_second_member(&mut retained, 1001, 8);
    let report = next_report(&mut session, &retained, 7);
    let reply = accepted_reply(&mut session, &report);
    let row = desire_row(&reply, "1001_7");
    assert_eq!(
        row.frame_authority,
        tiler_core::world_report::ScopeAuthority::Controlled,
        "a genuine desired change retries the write"
    );
    assert_ne!(
        row.effective_frame,
        Some(before),
        "a sibling changes the tiled allocation"
    );
}

#[test]
fn pair_admission_keeps_the_clamp_ahead_of_interleaved_reports() {
    // The owning worker stages `EngineJob::AxResult` right after a native
    // write, while the post-batch read that proves the write landed is
    // published later. `Inbox::try_send_observation` appends the batch's
    // completions and that read atomically (results first, then the read,
    // one slot reserved for it), so no other queued job can be processed
    // between them. Without the pair an unrelated report landing in the gap
    // drains the attempt against the pre-write rect: the core discards the
    // clamp (`tiler-core/src/world_frames.rs`), advances the attempt
    // watermark, and rewrites the natively held window on every later
    // report.
    let mut session = cursor_session();
    let mut retained = retained_with_tiled(
        1001,
        7,
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
    );
    let report = next_report(&mut session, &retained, 1);
    let reply = accepted_reply(&mut session, &report);
    let desired = desire_row(&reply, "1001_7")
        .effective_frame
        .expect("controlled frame");
    // The app clamps the requested height by one point; its readback proves
    // it and the post-batch read observes that clamped frame.
    let clamped = Rect {
        h: desired.h - 1,
        ..desired
    };
    fn observed(wid: u32, frame: Rect) -> ObservedWindow {
        ObservedWindow {
            wid,
            x: frame.x,
            y: frame.y,
            w: frame.w,
            h: frame.h,
            floating: false,
            fullscreen: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        }
    }
    fn scoped_read(seq: u64, clamped: Rect) -> EngineJob {
        EngineJob::ScopedObservation {
            pid: 1001,
            generation: 1,
            seq,
            snapshot: ScopedBackendSnapshot {
                windows: vec![observed(7, clamped)],
                focused_wid: Some(7),
                focus_cg_owned: true,
                focus_evidence: None,
                sys_focus: None,
                global_focus: None,
                meta: Vec::new(),
                excluded: 0,
                excluded_reasons: Vec::new(),
                cg_complete: true,
                cg_owner_wids: vec![7],
                cg_alive: Vec::new(),
                revision: 1,
            },
            trace_id: 0,
            intake_us: 0,
            enqueue_us: 0,
        }
    }
    fn completion(cmd_id: u64, clamped: Rect) -> EngineJob {
        EngineJob::AxResult {
            pid: 1001,
            generation: 1,
            cmd_id,
            completion: AxCompletion::Applied {
                elapsed_ms: 3,
                readback_xywh: Some((clamped.x, clamped.y, clamped.w, clamped.h)),
            },
            attempt_ms: Some(crate::input_eligibility::now_ms()),
        }
    }
    fn cursor(seq: u64, owner: Option<(u32, u32)>) -> EngineJob {
        EngineJob::CursorSample {
            seq,
            point: (5, 6),
            owner,
        }
    }

    // Capacity pressure never reorders the pair: the read keeps the slot
    // reserved for it and the excess feedback comes back to the worker.
    let tight = Inbox::new(2);
    let mut overflow = vec![completion(8, clamped), completion(9, clamped)];
    assert!(tight.try_send_observation(scoped_read(2, clamped), &mut overflow));
    assert_eq!(
        overflow.len(),
        1,
        "the post-batch read reserves its slot; excess results return",
    );

    let inbox = Inbox::new(4);
    // An unrelated report is already queued before the pair.
    assert!(inbox.try_send_data(cursor(1, Some((1001, 7)))));
    // The batch's completion and the read that proves it are admitted
    // together: results first, then the read, adjacent under one lock.
    let cmd_id = 1;
    track_layout_write(&mut session, 1001, cmd_id, "1001_7", desired);
    let mut preceding = vec![completion(cmd_id, clamped)];
    assert!(inbox.try_send_observation(scoped_read(2, clamped), &mut preceding));
    assert!(preceding.is_empty(), "the pair admits every queued result");
    // A later unrelated report follows the pair.
    assert!(inbox.try_send_data(cursor(2, None)));

    // Engine drain in strict FIFO order, through the real builder and core.
    let stop = std::sync::atomic::AtomicBool::new(false);
    assert!(matches!(
        inbox.recv(&stop),
        Some(EngineJob::CursorSample { seq: 1, .. })
    ));
    // The report queued before the pair cannot ride the attempt: its facts
    // predate the write, so the attempt has not staged at this point.
    let report = next_report(&mut session, &retained, 2);
    assert!(
        report.frames.is_empty(),
        "a report queued before the pair never sees the attempt"
    );
    let reply = accepted_reply(&mut session, &report);
    assert_eq!(
        desire_row(&reply, "1001_7").frame_authority,
        tiler_core::world_report::ScopeAuthority::Controlled,
        "nothing was learned before the pair"
    );

    // The completion stages first, exactly as the `AxResult` arm does.
    let queued = inbox.recv(&stop).expect("result queued");
    let EngineJob::AxResult {
        cmd_id: queued_cmd,
        completion,
        attempt_ms,
        ..
    } = queued
    else {
        panic!("the pair queues its results first");
    };
    assert_eq!(queued_cmd, cmd_id);
    assert!(stage_write_feedback(
        &mut session,
        &retained,
        1001,
        1,
        queued_cmd,
        &completion,
        attempt_ms,
    ));

    // The read that proves the clamp is the very next job: the engine
    // applies it and its own report drains the attempt with fresh facts.
    let queued = inbox.recv(&stop).expect("read queued");
    let EngineJob::ScopedObservation {
        seq,
        snapshot: read,
        ..
    } = queued
    else {
        panic!("the read stays adjacent to its results");
    };
    assert_eq!(seq, 2, "the pair keeps the read next to its results");
    assert_eq!(read.windows.len(), 1);
    assert_eq!(
        (
            read.windows[0].x,
            read.windows[0].y,
            read.windows[0].w,
            read.windows[0].h
        ),
        (clamped.x, clamped.y, clamped.w, clamped.h),
        "the paired read carries the app's clamped frame"
    );
    apply_owner_observation(&mut retained, 1001, 7, clamped);
    let report = next_report(&mut session, &retained, 3);
    assert_eq!(
        report.frames.len(),
        1,
        "the staged attempt rides the report the paired read produces"
    );
    assert_eq!(report.frames[0].readback, Some(clamped));
    let reply = accepted_reply(&mut session, &report);
    assert_eq!(
        desire_row(&reply, "1001_7").frame_authority,
        tiler_core::world_report::ScopeAuthority::Hold(
            tiler_core::world_report::HoldReason::ConstraintClamp
        ),
        "the one-point clamp is held after the paired read"
    );

    // The unrelated report following the pair never rewrites the held pair.
    assert!(matches!(
        inbox.recv(&stop),
        Some(EngineJob::CursorSample { seq: 2, .. })
    ));
    let report = next_report(&mut session, &retained, 4);
    assert!(
        report.frames.is_empty(),
        "a held pair re-authorizes no write"
    );
    let reply = accepted_reply(&mut session, &report);
    assert_eq!(
        desire_row(&reply, "1001_7").frame_authority,
        tiler_core::world_report::ScopeAuthority::Hold(
            tiler_core::world_report::HoldReason::ConstraintClamp
        ),
        "the hold survives the following report"
    );

    // A genuine observed change retries the write.
    let moved = Rect {
        x: clamped.x + 17,
        ..clamped
    };
    apply_owner_observation(&mut retained, 1001, 7, moved);
    let report = next_report(&mut session, &retained, 5);
    let reply = accepted_reply(&mut session, &report);
    assert_eq!(
        desire_row(&reply, "1001_7").frame_authority,
        tiler_core::world_report::ScopeAuthority::Controlled,
        "a genuine observation change retries the write"
    );
}

#[test]
fn burst_quiet_rearm_uses_the_report_clock() {
    // The burst budget shares the same process clock: two consumed
    // attempts on one unchanged desired target latch only until the core
    // can prove a >=500ms quiet gap in observed frame changes. Without a
    // real `monotonic_ms` (and a real `attempt_ms` on the same epoch) the
    // gate could never re-arm on macOS.
    let mut session = cursor_session();
    let site = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 100,
    };
    let mut retained = retained_with_tiled(1001, 7, site);
    let report = next_report(&mut session, &retained, 1);
    let reply = accepted_reply(&mut session, &report);
    let desired = desire_row(&reply, "1001_7")
        .effective_frame
        .expect("controlled frame");
    // Position-only drift: the size already matches the desire, so an
    // exact-readback write clears the clamp lane and only the burst budget
    // counts (never a size clamp; core never pins position-only drift).
    let drifted = Rect {
        x: desired.x + 1,
        ..desired
    };
    observe_frame(&mut retained, 1001, 7, drifted);
    for cmd_id in 1..=2u64 {
        track_layout_write(&mut session, 1001, cmd_id, "1001_7", desired);
        assert!(
            stage_write_feedback(
                &mut session,
                &retained,
                1001,
                1,
                cmd_id,
                &AxCompletion::Applied {
                    elapsed_ms: 1,
                    readback_xywh: Some((desired.x, desired.y, desired.w, desired.h)),
                },
                Some(crate::input_eligibility::now_ms()),
            ),
            "each write stages its attempt"
        );
        let report = next_report(&mut session, &retained, 1 + cmd_id);
        assert!(tiler_core::world_report_v4_checks::validate_report_v4_reason(&report).is_none());
        let reply = accepted_reply(&mut session, &report);
        let authority = desire_row(&reply, "1001_7").frame_authority;
        if cmd_id == 1 {
            assert_eq!(
                authority,
                tiler_core::world_report::ScopeAuthority::Controlled,
                "one burst attempt still permits the retry"
            );
        } else {
            assert_eq!(
                authority,
                tiler_core::world_report::ScopeAuthority::Hold(
                    tiler_core::world_report::HoldReason::ConstraintClamp
                ),
                "two attempts inside one burst hold the unchanged target"
            );
        }
    }
    // A real quiet gap plus a genuine observed change re-arms the budget.
    std::thread::sleep(std::time::Duration::from_millis(520));
    let shifted = Rect {
        x: desired.x + 2,
        ..desired
    };
    observe_frame(&mut retained, 1001, 7, shifted);
    let report = next_report(&mut session, &retained, 9);
    assert!(report.monotonic_ms > 0, "the report carries the real clock");
    let reply = accepted_reply(&mut session, &report);
    assert_eq!(
        desire_row(&reply, "1001_7").frame_authority,
        tiler_core::world_report::ScopeAuthority::Controlled,
        "a proven quiet gap re-arms the burst budget"
    );
}

#[test]
fn refused_layout_write_backs_off_on_the_process_clock() {
    // A refused Controlled write rides the core's transient lane. It can
    // only suppress a retry when its `attempt_ms` shares the report clock
    // epoch: under the old per-call duration (a few ms) the backoff
    // comparison was always satisfied and macOS retried every report.
    let mut session = cursor_session();
    let retained = retained_with_tiled(
        1001,
        7,
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
    );
    let report = next_report(&mut session, &retained, 1);
    let reply = accepted_reply(&mut session, &report);
    let desired = desire_row(&reply, "1001_7")
        .effective_frame
        .expect("controlled frame");
    track_layout_write(&mut session, 1001, 1, "1001_7", desired);
    assert!(stage_write_feedback(
        &mut session,
        &retained,
        1001,
        1,
        1,
        &AxCompletion::Failed {
            reason: "AXError".to_owned()
        },
        Some(crate::input_eligibility::now_ms()),
    ));
    let report = next_report(&mut session, &retained, 2);
    let reply = accepted_reply(&mut session, &report);
    assert_eq!(
        desire_row(&reply, "1001_7").frame_authority,
        tiler_core::world_report::ScopeAuthority::Hold(
            tiler_core::world_report::HoldReason::ConstraintClamp
        ),
        "a refusal inside its backoff window is not retried"
    );
}

#[test]
fn layout_completion_never_displaces_a_staged_placement_row() {
    // One window owns one frame authority per report (the wire validator
    // refuses `duplicate-frame`): a placement row settles a token the
    // adapter already spent and always wins the collision, while the
    // displaced layout row is advisory feedback the core re-learns from the
    // next observation.
    let mut session = cursor_session();
    let site = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 100,
    };
    let mut retained = retained_with_tiled(1001, 7, site);
    add_second_member(&mut retained, 1001, 8);
    let stamp = Some(crate::input_eligibility::now_ms());
    let placement_target = Rect {
        x: 10,
        y: 20,
        w: 300,
        h: 200,
    };
    track_write(
        &mut session,
        1001,
        1,
        "1001_7",
        placement_target,
        Some("op-3"),
    );
    assert!(stage_write_feedback(
        &mut session,
        &retained,
        1001,
        1,
        1,
        &AxCompletion::Applied {
            elapsed_ms: 2,
            readback_xywh: Some((10, 20, 299, 200)),
        },
        stamp,
    ));
    // An ordinary layout completion for the same window lands before the
    // next dispatch, plus one for the sibling window.
    track_layout_write(&mut session, 1001, 2, "1001_7", site);
    assert!(stage_write_feedback(
        &mut session,
        &retained,
        1001,
        1,
        2,
        &AxCompletion::Applied {
            elapsed_ms: 2,
            readback_xywh: Some((site.x, site.y, site.w, site.h)),
        },
        stamp,
    ));
    track_layout_write(&mut session, 1001, 3, "1001_8", site);
    assert!(stage_write_feedback(
        &mut session,
        &retained,
        1001,
        1,
        3,
        &AxCompletion::Applied {
            elapsed_ms: 2,
            readback_xywh: Some((site.x, site.y, site.w, site.h)),
        },
        stamp,
    ));
    let report = next_report(&mut session, &retained, 1);
    assert!(
        tiler_core::world_report_v4_checks::validate_report_v4_reason(&report).is_none(),
        "the collision merge passes the real validator"
    );
    assert_eq!(
        report.frames.len(),
        2,
        "one row per window: the placement row plus the free layout row"
    );
    let collided = report
        .frames
        .iter()
        .find(|row| row.window.0 == "1001_7")
        .expect("collided window row");
    assert_eq!(
        collided.purpose,
        tiler_core::world_report::FramePurpose::FloatPlacement {
            operation_id: "op-3".to_owned(),
            prewrite: None,
        },
        "placement evidence wins the same-window collision"
    );
    assert_eq!(collided.target, placement_target);
    assert_eq!(
        collided.readback,
        Some(Rect {
            x: 10,
            y: 20,
            w: 299,
            h: 200
        })
    );
    let free = report
        .frames
        .iter()
        .find(|row| row.window.0 == "1001_8")
        .expect("uncollided layout row");
    assert_eq!(
        free.purpose,
        tiler_core::world_report::FramePurpose::Layout,
        "layout rows ride when no placement owns the window"
    );
}

#[test]
fn pre_call_fence_stages_no_attempt_and_voids_the_correlation() {
    // A write fenced before any AX setter stages no attempt: it can never
    // spend core burst or transient budget. The correlation is consumed,
    // so a later completion for the same command cannot resurrect it.
    let mut session = cursor_session();
    let retained = retained_with_tiled(
        1001,
        7,
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
    );
    let target = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 100,
    };
    track_layout_write(&mut session, 1001, 1, "1001_7", target);
    assert!(
        !stage_write_feedback(
            &mut session,
            &retained,
            1001,
            1,
            1,
            &AxCompletion::FencedNoCall {
                reason: "topology revision moved".to_owned()
            },
            Some(crate::input_eligibility::now_ms()),
        ),
        "a pre-call fence stages nothing"
    );
    assert!(
        !stage_write_feedback(
            &mut session,
            &retained,
            1001,
            1,
            1,
            &AxCompletion::Applied {
                elapsed_ms: 1,
                readback_xywh: Some((target.x, target.y, target.w, target.h))
            },
            Some(crate::input_eligibility::now_ms()),
        ),
        "the pre-call fence consumed the correlation"
    );
    let report = next_report(&mut session, &retained, 1);
    assert!(tiler_core::world_report_v4_checks::validate_report_v4_reason(&report).is_none());
    assert!(
        report.frames.is_empty(),
        "no frame attempt rides for a write that never called AX"
    );
}
