#![cfg(test)]
use crate::runtime_types::InputSemanticAction as Action;
use crate::workspace_select::*;

struct Fake {
    row: SelectionSnapshot,
    displays: usize,
    revision: Option<u64>,
    empty: Result<bool, String>,
    creates: usize,
    selects: Vec<u64>,
    effect: bool,
    drift: bool,
}
impl Default for Fake {
    fn default() -> Self {
        Self {
            row: SelectionSnapshot {
                display: "display-a".to_owned(),
                spaces: vec![20, 3],
                current: 20,
            },
            displays: 1,
            revision: None,
            empty: Ok(true),
            creates: 0,
            selects: Vec::new(),
            effect: true,
            drift: false,
        }
    }
}
impl SelectionBackend for Fake {
    fn display_count(&mut self) -> Result<usize, String> {
        Ok(self.displays)
    }
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        Ok(self.row.clone())
    }
    fn revision(&mut self) -> Option<u64> {
        self.revision
    }
    fn empty(&mut self, _: u64) -> Result<bool, String> {
        self.empty.clone()
    }
    fn create(&mut self, _: &SelectionSnapshot) -> Result<(), String> {
        self.creates += 1;
        if self.effect {
            self.row.spaces.push(21);
        }
        if self.drift {
            self.row.spaces.swap(0, 1);
        }
        Ok(())
    }
    fn select(&mut self, _: &SelectionSnapshot, sid: u64) -> Result<(), String> {
        self.selects.push(sid);
        if self.effect {
            self.row.current = sid;
        }
        Ok(())
    }
}
#[test]
fn numbered_select_is_native_order_existing_only_and_previous_toggles() {
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    assert!(state.execute(&mut fake, Action::WorkspacePrevious).is_err());
    assert!(
        state
            .execute(&mut fake, Action::WorkspaceSelect { index: 9 })
            .is_err()
    );
    state
        .execute(&mut fake, Action::WorkspaceSelect { index: 2 })
        .unwrap();
    assert_eq!(fake.row.current, 3); // Not sorted numeric SIDs.
    state.execute(&mut fake, Action::WorkspacePrevious).unwrap();
    assert_eq!(fake.row.current, 20);
    state.execute(&mut fake, Action::WorkspacePrevious).unwrap();
    assert_eq!(fake.row.current, 3);
    // Selecting current never overwrites the other view.
    state
        .execute(&mut fake, Action::WorkspaceSelect { index: 2 })
        .unwrap();
    state.execute(&mut fake, Action::WorkspacePrevious).unwrap();
    assert_eq!(fake.row.current, 20);
    assert_eq!(fake.creates, 0);
}
#[test]
fn relative_wraps_without_creating() {
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    state
        .execute(&mut fake, Action::WorkspaceRelative { prev: true })
        .unwrap();
    assert_eq!(fake.row.current, 3);
    state
        .execute(&mut fake, Action::WorkspaceRelative { prev: false })
        .unwrap();
    assert_eq!(fake.row.current, 20);
    assert_eq!(fake.creates, 0);
}
#[test]
fn trailing_reuses_empty_but_appends_when_occupied_or_below_shared_floor() {
    for (spaces, empty, creates, target) in [
        (vec![20, 3], true, 0, 3),
        (vec![20, 3], false, 1, 21),
        (vec![20], true, 1, 21),
    ] {
        let mut fake = Fake {
            row: SelectionSnapshot {
                spaces,
                ..Fake::default().row
            },
            empty: Ok(empty),
            ..Fake::default()
        };
        let effect = SelectionState::default()
            .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
            .unwrap();
        assert_eq!(fake.creates, creates);
        assert_eq!(effect.target, target);
        assert_eq!(effect.created, if creates == 0 { None } else { Some(21) });
    }
}
#[test]
fn unknown_emptiness_create_drift_and_no_effect_refuse_without_replay() {
    let mut fake = Fake {
        empty: Err("unreadable".to_owned()),
        ..Fake::default()
    };
    assert!(
        SelectionState::default()
            .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
            .is_err()
    );
    assert_eq!(fake.creates, 0);
    let mut fake = Fake {
        empty: Ok(false),
        drift: true,
        ..Fake::default()
    };
    assert!(
        SelectionState::default()
            .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
            .is_err()
    );
    assert_eq!(fake.creates, 1);
    assert!(fake.selects.is_empty());
    let mut fake = Fake {
        effect: false,
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    assert!(
        state
            .execute(&mut fake, Action::WorkspaceSelect { index: 2 })
            .is_err()
    );
    assert_eq!(fake.selects, vec![3]);
    assert!(state.execute(&mut fake, Action::WorkspacePrevious).is_err());
}
#[test]
fn invalid_rows_deleted_previous_and_other_display_fail_narrowly() {
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    state
        .execute(&mut fake, Action::WorkspaceSelect { index: 2 })
        .unwrap();
    fake.row.spaces = vec![3];
    assert!(state.execute(&mut fake, Action::WorkspacePrevious).is_err());
    fake.row.display = "display-b".to_owned();
    assert!(state.execute(&mut fake, Action::WorkspacePrevious).is_err());
    fake.row.spaces = vec![3, 3];
    assert!(
        state
            .execute(&mut fake, Action::WorkspaceSelect { index: 1 })
            .is_err()
    );
}
#[test]
fn select_subset_contains_only_armed_numbered_and_auxiliary_arms() {
    // Product subset: exactly the 18 armed rows (numbered 1-9, the 8
    // relative rows, previous). The trailing select-0 stays out (no
    // automatic trailing creation/deletion this milestone).
    let bindings = bindings();
    assert_eq!(bindings.len(), 18);
    assert!(
        bindings
            .iter()
            .all(|b| !matches!(b.action, Action::WorkspaceSelect { index: 0 })),
        "trailing select-0 never armed"
    );
    assert!(bindings.iter().all(|b| !b.shift && !b.alt));
    for b in &bindings {
        assert_eq!(b.aux, !matches!(b.action, Action::WorkspaceSelect { .. }));
    }
    // Every armed binding resolves through the armed-select gate.
    for b in &bindings {
        assert!(crate::workspace_input::is_armed_select(&b.action));
    }
    let mut fake = Fake::default();
    assert!(
        SelectionState::default()
            .execute(
                &mut fake,
                Action::WorkspaceSend {
                    index: 1,
                    follow: true
                }
            )
            .is_err()
    );
    assert!(fake.selects.is_empty());
}

#[test]
fn zero_area_candidate_is_unknown_never_an_append_trigger() {
    use crate::workspace_select::candidate_occupied;
    assert!(candidate_occupied(7, 77, 0.0, 0.0, &[20], 20).is_err());
    assert!(candidate_occupied(7, 77, f64::NAN, 100.0, &[20], 20).is_err());
    assert_eq!(candidate_occupied(7, 77, 480.0, 320.0, &[20], 20), Ok(true));
    assert_eq!(candidate_occupied(7, 77, 0.0, 0.0, &[3], 20), Ok(false));
}

fn open_gates() -> SelectGates {
    SelectGates {
        session_unlocked: true,
        secure_active: false,
        lock_screen: false,
    }
}

#[test]
fn drain_gates_refuse_without_backend_touch_or_history() {
    for gates in [
        SelectGates {
            session_unlocked: false,
            ..open_gates()
        },
        SelectGates {
            secure_active: true,
            ..open_gates()
        },
        SelectGates {
            lock_screen: true,
            ..open_gates()
        },
    ] {
        let mut fake = Fake::default();
        let mut state = SelectionState::default();
        let err = drain_claimed(
            &mut state,
            &mut fake,
            Action::WorkspaceSelect { index: 2 },
            &gates,
        )
        .expect_err("gated drain refuses");
        assert!(err.contains("gated"), "{err}");
        assert!(fake.selects.is_empty());
        assert_eq!(fake.creates, 0);
        assert!(state.execute(&mut fake, Action::WorkspacePrevious).is_err());
    }
}

#[test]
fn drain_runs_single_dispatch_and_advances_history_only_on_arrival() {
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let effect = drain_claimed(
        &mut state,
        &mut fake,
        Action::WorkspaceSelect { index: 2 },
        &open_gates(),
    )
    .unwrap();
    assert!(effect.changed);
    assert_eq!(fake.selects, vec![3]);
    drain_claimed(
        &mut state,
        &mut fake,
        Action::WorkspacePrevious,
        &open_gates(),
    )
    .unwrap();
    assert_eq!(fake.row.current, 20);
    // No effect: exactly one dispatch (never a replay) and history stays
    // on the last verified arrival.
    let mut stuck = Fake {
        effect: false,
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let err = drain_claimed(
        &mut state,
        &mut stuck,
        Action::WorkspaceSelect { index: 2 },
        &open_gates(),
    )
    .expect_err("unproven effect refuses");
    assert!(err.contains("unproven"), "{err}");
    assert_eq!(stuck.selects, vec![3], "single dispatch, never replayed");
    assert!(
        state
            .execute(&mut stuck, Action::WorkspacePrevious)
            .is_err()
    );
}

#[test]
fn drain_refuses_unarmed_actions_without_backend_touch() {
    for action in [
        Action::WorkspaceSelect { index: 0 },
        Action::WorkspaceSend {
            index: 1,
            follow: true,
        },
        Action::WorkspaceSendRelative {
            prev: true,
            follow: false,
        },
        Action::WorkspaceOutput {
            direction: crate::runtime_types::InputDirection::Left,
            follow: true,
        },
    ] {
        let mut fake = Fake::default();
        let mut state = SelectionState::default();
        let err = drain_claimed(&mut state, &mut fake, action, &open_gates())
            .expect_err("unarmed drain refuses");
        assert!(err.contains("unarmed"), "{err}");
        assert!(fake.selects.is_empty());
        assert_eq!(fake.creates, 0);
    }
}

#[test]
fn drain_refuses_without_exactly_one_display_before_any_effect() {
    // Zero or multiple native display rows refuse before any backend
    // effect leg and advance no history: first-display guessing could
    // switch the wrong display, so single-display support is the rule.
    for displays in [0, 2, 3] {
        let mut fake = Fake {
            displays,
            ..Fake::default()
        };
        let mut state = SelectionState::default();
        let err = drain_claimed(
            &mut state,
            &mut fake,
            Action::WorkspaceSelect { index: 2 },
            &open_gates(),
        )
        .expect_err("multi-display drain refuses");
        assert!(
            err.contains("exactly one native display"),
            "display rule names itself: {err}"
        );
        assert!(fake.selects.is_empty());
        assert_eq!(fake.creates, 0);
        assert!(state.execute(&mut fake, Action::WorkspacePrevious).is_err());
    }
    // Exactly one row drains (control case pins the rule boundary).
    let mut fake = Fake {
        displays: 1,
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let effect = drain_claimed(
        &mut state,
        &mut fake,
        Action::WorkspaceSelect { index: 2 },
        &open_gates(),
    )
    .unwrap();
    assert!(effect.changed);
    assert_eq!(fake.selects, vec![3]);
}

use crate::runtime_types::ClaimedSelect;

fn bound_claim(index: u8, sid: u64, revision: u64) -> ClaimedSelect {
    ClaimedSelect::indicator(
        Action::WorkspaceSelect { index },
        sid,
        7, // UI generation (carried opaquely; the UI side fences it)
        revision,
    )
}

#[test]
fn bound_drain_dispatches_only_when_ordinal_still_names_its_sid() {
    // Happy path: the admitted binding (row 2 = SID 3 at revision 9)
    // still holds at drain time — single dispatch, verified arrival.
    let mut fake = Fake {
        revision: Some(9),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let effect = drain_claimed_select(&mut state, &mut fake, bound_claim(2, 3, 9), &open_gates())
        .expect("fresh binding dispatches");
    assert!(effect.changed);
    assert_eq!(fake.selects, vec![3]);
    assert_eq!(fake.row.current, 3);
}

#[test]
fn bound_drain_refuses_topology_reorder_after_admission_before_drain() {
    // The engine admitted row 2 = SID 3 at revision 9; the native order
    // then moved (SID 3 deleted, SID 41 appended) before the main-thread
    // drain. The drain must refuse instead of selecting the new occupant
    // of ordinal 2 — with no dispatch and no history advance.
    let mut fake = Fake {
        revision: Some(9),
        ..Fake::default()
    };
    let claim = bound_claim(2, 3, 9);
    fake.row.spaces = vec![20, 41];
    let mut state = SelectionState::default();
    let err = drain_claimed_select(&mut state, &mut fake, claim, &open_gates())
        .expect_err("reordered binding refuses");
    assert!(err.contains("moved"), "{err}");
    assert!(fake.selects.is_empty(), "never dispatched");
    assert!(fake.row.current == 20, "current untouched");
    assert!(
        state.execute(&mut fake, Action::WorkspacePrevious).is_err(),
        "no history advanced"
    );
}

#[test]
fn bound_drain_refuses_revision_drift_after_admission_before_drain() {
    // Same row contents, but the topology revision moved between
    // admission (9) and drain (10): the binding is stale and refuses even
    // though the ordinal still names the same SID.
    let mut fake = Fake {
        revision: Some(10),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let err = drain_claimed_select(&mut state, &mut fake, bound_claim(2, 3, 9), &open_gates())
        .expect_err("drifted revision refuses");
    assert!(err.contains("revision"), "{err}");
    assert!(fake.selects.is_empty(), "never dispatched");
}

#[test]
fn bound_drain_refuses_vanished_position_and_unbound_revision_skips_leg() {
    // Vanished ordinal refuses.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![20],
            current: 20,
        },
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let err = drain_claimed_select(&mut state, &mut fake, bound_claim(2, 3, 9), &open_gates())
        .expect_err("vanished row refuses");
    assert!(err.contains("no longer exists"), "{err}");
    assert!(fake.selects.is_empty());
    // A backend that cannot report a revision (None) still fences on the
    // SID leg: matching SID dispatches, moved SID refuses.
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let effect = drain_claimed_select(&mut state, &mut fake, bound_claim(2, 3, 9), &open_gates())
        .expect("revision-less backend still dispatches fresh bindings");
    assert!(effect.changed);
    assert_eq!(fake.selects, vec![3]);
}

/// Native-ordinal parity shared repro: a single-display synthetic snapshot
/// with the SLS row `[1, 277]` and `current` naming the startup Space.
/// Display geometry and UUID are synthetic shapes only (never a live
/// machine value). The core claims the startup current at scoped-ring
/// position 1 (observed currents claim first), so a 277-started runtime
/// holds the reversed retained ring `[space-277, space-1]` while the native
/// row stays `[1, 277]`.
#[cfg(test)]
mod ordinal_parity_repro {
    use std::collections::BTreeMap;

    use tiler_core::directional::OutputId;
    use tiler_core::session::Session;
    use tiler_core::world_engine::WorldOutcome;
    use tiler_core::world_report::{WorldCommand, validate_report};

    use crate::ffi::{SlsDisplayRow, SlsSpaceType};
    use crate::runtime_types::NativeContext;
    use crate::topology::{PointsRect, TopologySnapshot};
    use crate::world_builder::{ReportInput, build_report};

    pub const OWNER: &str = "macos-phase-1";
    pub const INCARNATION: &str = "adapter-1";
    pub const GENERATION: &str = "gen-1";
    const UUID: &str = "deadbeef-dead-4000-8000-deadbeefdead";

    pub fn ctx(startup_current: u64) -> NativeContext {
        let snapshot = TopologySnapshot {
            displays: vec![crate::topology::DisplayGeometry {
                cg_id: 1,
                uuid: Some(UUID.to_owned()),
                cg_bounds: Some(PointsRect {
                    x: 0.0,
                    y: 0.0,
                    w: 2240.0,
                    h: 1260.0,
                }),
                visible: Some(PointsRect {
                    x: 0.0,
                    y: 25.0,
                    w: 2240.0,
                    h: 1235.0,
                }),
                scale: 2.0,
            }],
            rows: vec![SlsDisplayRow {
                identifier: Some(UUID.to_owned()),
                spaces: vec![1, 277],
                current: Some(startup_current),
            }],
            separate_spaces: Some(true),
            revision: 0,
        };
        let snapshot = TopologySnapshot {
            revision: crate::topology::fingerprint_snapshot(
                &snapshot.displays,
                &snapshot.rows,
                Some(true),
            ),
            ..snapshot
        };
        let (ctx, skipped) = NativeContext::derive(&snapshot, &|_| Some(SlsSpaceType::User));
        assert!(skipped.is_empty(), "nothing skipped: {skipped:?}");
        let row: Vec<u64> = ctx.domains.iter().map(|d| d.space).collect();
        assert_eq!(row, vec![1, 277], "derive keeps SLS row order");
        ctx
    }

    pub fn output() -> OutputId {
        OutputId(crate::topology::output_key(1))
    }

    /// Drive one engine from a commandless first report (establishes the
    /// retained ring under this startup current) and return the outcome of
    /// one numbered/explicit command report plus the retained transfers.
    pub fn drive(
        ctx: &NativeContext,
        command: WorldCommand,
    ) -> (WorldOutcome, Vec<tiler_core::world_report::TransferTarget>) {
        use tiler_core::world_engine::WorldEngine;

        let empty_members = BTreeMap::new();
        let empty_sessions: BTreeMap<tiler_core::session::DomainKey, Session> = BTreeMap::new();
        let empty_focused = BTreeMap::new();
        let empty_closed = BTreeMap::new();
        let empty_unreadable = BTreeMap::new();
        let build =
            |obs: u64, req: u64, req_id: &str, seq: Option<u64>, command: Option<WorldCommand>| {
                let mut input = ReportInput::new(
                    OWNER,
                    INCARNATION,
                    GENERATION,
                    ctx,
                    obs,
                    req,
                    req_id,
                    &empty_members,
                    &empty_sessions,
                    &empty_focused,
                    &empty_closed,
                    &empty_unreadable,
                    &[],
                    &[],
                    "",
                );
                input.command_seq = seq;
                input.command = command;
                build_report(&input)
            };
        let mut engine = WorldEngine::new(GENERATION).expect("generation parses");
        let first = build(1, 1, "req-1", None, None);
        assert!(validate_report(&first));
        assert!(matches!(
            engine.handle_report(&first),
            WorldOutcome::Accepted(_)
        ));
        let second = build(2, 2, "req-2", Some(2), Some(command));
        assert!(
            validate_report(&second),
            "command report is well-formed wire input"
        );
        let outcome = engine.handle_report(&second);
        let retained = engine.retained_transfers();
        (outcome, retained)
    }
}

/// Semantic contrast for the 277-startup repair: the legacy positional
/// `SelectNumbered{index: 1}` resolves through the stable-logical scoped
/// ring (`[space-277, space-1]`) and self-targets the startup current, while
/// the fixed admission-resolved `SelectWorkspace{space-1}` against the
/// native row (`[1, 277]`) selects Desktop-1 — under identical native
/// context plus validated reports on fresh engines. Both second reports
/// validate, so the red is a semantic wrong-target route failure, never an
/// API/compile oracle.
#[test]
fn legacy_numbered_select_self_targets_under_277_startup() {
    use tiler_core::world_report::WorldCommand;

    use ordinal_parity_repro as repro;

    let ctx = repro::ctx(277);
    let output = repro::output();
    let (outcome, retained) = repro::drive(
        &ctx,
        WorldCommand::SelectNumbered {
            output: output.clone(),
            index: 1,
        },
    );
    assert!(
        matches!(outcome, tiler_core::world_engine::WorldOutcome::Accepted(_)),
        "legacy numbered select admits (it just names the wrong target)"
    );
    assert_eq!(retained.len(), 1, "one retained view");
    assert_eq!(
        retained[0].target_workspace, "space-277",
        "Desktop-1 through the stable-logical ring names the 277-started current, not native SID 1"
    );
}

/// Native Desktop-N resolves exactly once at admission: a runtime started
/// on a non-first Space still selects by native ordinal. A startup on 277
/// selects Desktop-1 to SID 1 (the retained inverse-refusal shape), and a
/// startup on 1 keeps the proven Desktop-2 path. Same-Space and unknown
/// targets keep their honest refusals, and the bound-claim drain proves the
/// pinned SID instead of interpreting the ordinal against a new generation.
#[test]
fn explicit_select_resolves_native_ordinal_once_at_admission() {
    use tiler_core::world_engine::WorldOutcome;
    use tiler_core::world_report::{TransferKind, WorldCommand};

    use crate::workspace_send::context_target_sid;
    use crate::world_command_map::select_command;
    use ordinal_parity_repro as repro;

    let select = |index: u8| Action::WorkspaceSelect { index };
    let output = repro::output();

    // Startup on 277: admission binds the native Desktop-1 SID, and the
    // production command carries it explicitly — never the ordinal.
    let ctx = repro::ctx(277);
    assert!(
        ctx.domains.iter().any(|d| d.space == 277 && d.current),
        "277 is the actuated current"
    );
    let sid = context_target_sid(&ctx, &output, 1).expect("Desktop-1 binds");
    assert_eq!(sid, 1, "Desktop-1 is native SID 1");
    let command = select_command(
        &select(1),
        output.clone(),
        &crate::topology::workspace_key(sid),
    )
    .expect("select route");
    assert_eq!(
        command,
        WorldCommand::SelectWorkspace {
            output: output.clone(),
            workspace: "space-1".to_owned(),
        },
        "select-1 carries an explicit SID target, got {command:?}"
    );
    let (outcome, retained) = repro::drive(&ctx, command);
    assert!(
        matches!(outcome, WorldOutcome::Accepted(_)),
        "Desktop-1 from a 277-started runtime admits"
    );
    assert_eq!(retained.len(), 1, "one retained view");
    assert_eq!(retained[0].kind, TransferKind::View);
    assert_eq!(retained[0].target_output, output);
    assert_eq!(retained[0].target_workspace, "space-1");
    assert_eq!(
        retained[0].operation_id, "op-2",
        "stable operation identity unchanged"
    );

    // Admission-bound tap claim pins the SID plus the live revision (the
    // shape `handle_select_action` stores): the drain proves it against the
    // fresh row instead of interpreting the ordinal against a new
    // generation.
    let mut claim = ClaimedSelect::tap(select(1));
    claim.expected_sid = Some(sid);
    claim.topology_revision = Some(ctx.revision);
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 277,
        },
        revision: Some(ctx.revision),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let effect = drain_claimed_select(&mut state, &mut fake, claim.clone(), &open_gates())
        .expect("fresh binding dispatches the pinned SID");
    assert!(effect.changed);
    assert_eq!(fake.selects, vec![1], "native effect names SID 1");
    // A reorder between admission and drain refuses instead of selecting
    // the new occupant of the ordinal.
    let mut moved = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![277, 1],
            current: 277,
        },
        revision: Some(ctx.revision),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let err = drain_claimed_select(&mut state, &mut moved, claim, &open_gates())
        .expect_err("reordered binding refuses");
    assert!(err.contains("moved"), "{err}");
    assert!(moved.selects.is_empty(), "never dispatched");

    // Startup-on-1 parity: Desktop-2 from a 1-started runtime still admits.
    let ctx = repro::ctx(1);
    let sid = context_target_sid(&ctx, &output, 2).expect("Desktop-2 binds");
    assert_eq!(sid, 277);
    let command = select_command(
        &select(2),
        output.clone(),
        &crate::topology::workspace_key(sid),
    )
    .expect("select route");
    let (outcome, retained) = repro::drive(&ctx, command);
    assert!(
        matches!(outcome, WorldOutcome::Accepted(_)),
        "Desktop-2 from a 1-started runtime admits"
    );
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].target_workspace, "space-277");

    // Bounds/missing targets refuse narrowly at admission (never guessed).
    assert_eq!(context_target_sid(&ctx, &output, 0), None);
    assert_eq!(context_target_sid(&ctx, &output, 10), None);
    assert_eq!(
        select_command(&select(0), output.clone(), "space-1"),
        None,
        "trailing select-0 stays refused"
    );

    // Unknown native targets keep their honest core refusal.
    let (outcome, retained) = repro::drive(
        &ctx,
        WorldCommand::SelectWorkspace {
            output: output.clone(),
            workspace: "space-999".to_owned(),
        },
    );
    assert!(
        matches!(
            outcome,
            WorldOutcome::Rejected {
                kind: "view-target-unknown",
                ..
            }
        ),
        "unknown target refuses, got {outcome:?}"
    );
    assert!(retained.is_empty(), "refused select retains nothing");

    // Relative/previous semantics are untouched (binding-free core paths).
    let relative = select_command(
        &Action::WorkspaceRelative { prev: true },
        output.clone(),
        "",
    )
    .expect("relative route");
    assert_eq!(
        relative,
        WorldCommand::SelectRelative {
            output: output.clone(),
            delta: -1
        }
    );
    let previous =
        select_command(&Action::WorkspacePrevious, output.clone(), "").expect("previous route");
    assert_eq!(previous, WorldCommand::SelectPrevious { output });
}
