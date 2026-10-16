#![cfg(test)]
use crate::runtime_types::InputSemanticAction as Action;
use crate::workspace_select::*;

struct Fake {
    row: SelectionSnapshot,
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
            empty: Ok(true),
            creates: 0,
            selects: Vec::new(),
            effect: true,
            drift: false,
        }
    }
}
impl SelectionBackend for Fake {
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        Ok(self.row.clone())
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
fn select_subset_contains_only_approved_numbered_and_auxiliary_arms() {
    let bindings = bindings();
    assert_eq!(bindings.len(), 19);
    assert!(bindings.iter().all(|b| !b.shift && !b.alt));
    for b in bindings {
        assert_eq!(b.aux, !matches!(b.action, Action::WorkspaceSelect { .. }));
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
