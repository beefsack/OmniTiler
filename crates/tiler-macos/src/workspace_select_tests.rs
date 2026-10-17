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
