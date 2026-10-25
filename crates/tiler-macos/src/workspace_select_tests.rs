#![cfg(test)]
use crate::runtime_types::InputSemanticAction as Action;
use crate::workspace_input::TRAILING_CAPABILITY_ENABLED;
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
            // Append a fresh SID per leg (21 first, then 22, ...): a second
            // trailing on an occupied append proves one create per leg
            // without duplicating the first SID.
            let next = if !self.row.spaces.contains(&21) {
                21
            } else if !self.row.spaces.contains(&22) {
                22
            } else {
                self.row.spaces.iter().max().copied().unwrap_or(22) + 1
            };
            self.row.spaces.push(next);
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
fn trailing_failed_select_still_reports_created_for_lifecycle_adoption() {
    // The append is verified (exact same-display append) but the follow-up
    // select fails: the Space persists, so `take_trailing_proof` still reports the
    // exact-created SID for lifecycle ownership (the tile adopts on both
    // outcomes). The drift here breaks the pre-select topology check after
    // the verified append — the single append already ran exactly once.
    struct FailSelect {
        inner: Fake,
    }
    impl SelectionBackend for FailSelect {
        fn display_count(&mut self) -> Result<usize, String> {
            self.inner.display_count()
        }
        fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
            self.inner.snapshot()
        }
        fn revision(&mut self) -> Option<u64> {
            self.inner.revision()
        }
        fn empty(&mut self, sid: u64) -> Result<bool, String> {
            self.inner.empty(sid)
        }
        fn create(&mut self, before: &SelectionSnapshot) -> Result<(), String> {
            self.inner.create(before)
        }
        fn select(&mut self, _: &SelectionSnapshot, _: u64) -> Result<(), String> {
            Err("goto failed".to_owned())
        }
    }
    let mut backend = FailSelect {
        inner: Fake {
            row: SelectionSnapshot {
                display: "display-a".to_owned(),
                spaces: vec![1, 277],
                current: 1,
            },
            empty: Ok(false),
            ..Fake::default()
        },
    };
    let mut state = SelectionState::default();
    let err = state
        .execute(&mut backend, Action::WorkspaceSelect { index: 0 })
        .expect_err("failed goto refuses");
    assert!(err.contains("goto failed"), "{err}");
    assert_eq!(
        backend.inner.creates, 1,
        "exactly one append, never replayed"
    );
    // Ownership capture even though the effect failed: the created Space
    // is real and must enter lifecycle tracking, and its exact-append
    // proof must still bind the engine token.
    let proof = state.take_trailing_proof().expect("proof captured");
    assert_eq!(proof.created, 21);
    assert_eq!(proof.display_id, "display-a");
    assert_eq!(proof.before, vec![1, 277]);
    assert_eq!(proof.after, vec![1, 277, 21]);
    assert!(
        state.take_trailing_proof().is_none(),
        "taking clears the slot"
    );
}

#[test]
fn trailing_reuse_current_empty_is_same_current_noop() {
    // Producer-shaped native row `[1, 277]` current on the trailing empty:
    // selecting trailing reuses last with no create, no dispatch, unchanged.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 277,
        },
        empty: Ok(true),
        ..Fake::default()
    };
    let effect = SelectionState::default()
        .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
        .unwrap();
    assert_eq!(effect.target, 277);
    assert_eq!(effect.created, None);
    assert!(!effect.changed);
    assert_eq!(fake.creates, 0);
    assert!(fake.selects.is_empty(), "same-current never dispatches");
}

#[test]
fn trailing_reuse_noncurrent_empty_selects_without_create() {
    // `[1, 277]` current 1, last proven empty: reuse 277 with one dispatch.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        empty: Ok(true),
        ..Fake::default()
    };
    let effect = SelectionState::default()
        .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
        .unwrap();
    assert_eq!(effect.target, 277);
    assert_eq!(effect.created, None);
    assert!(effect.changed);
    assert_eq!(fake.creates, 0);
    assert_eq!(fake.selects, vec![277], "single dispatch, never replayed");
}

#[test]
fn trailing_occupied_appends_once_then_subsequent_occupied_appends_again() {
    // First trailing on occupied `[1, 277]` appends exactly one SID.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        empty: Ok(false),
        ..Fake::default()
    };
    let first = SelectionState::default()
        .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
        .unwrap();
    assert_eq!(first.created, Some(21));
    assert_eq!(first.target, 21);
    assert_eq!(fake.creates, 1);
    // The appended trailing is now occupied too: a second trailing appends
    // again (exact append, never a guess), proving no double-create per leg.
    fake.empty = Ok(false);
    let second = SelectionState::default()
        .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
        .unwrap();
    assert_eq!(fake.creates, 2, "one create per trailing leg");
    assert_eq!(second.target, *fake.row.spaces.last().unwrap());
    assert!(second.created.is_some());
}

#[test]
fn trailing_unknown_emptiness_refuses_without_create_or_dispatch() {
    for empty in [
        Err("unreadable".to_owned()),
        Err("trailing emptiness unknown: zero-area/unreadable candidate WID7/PID77".to_owned()),
    ] {
        let mut fake = Fake {
            row: SelectionSnapshot {
                display: "display-a".to_owned(),
                spaces: vec![1, 277],
                current: 1,
            },
            empty,
            ..Fake::default()
        };
        let err = SelectionState::default()
            .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
            .expect_err("unknown refuses");
        assert!(
            err.contains("unreadable") || err.contains("unknown"),
            "{err}"
        );
        assert_eq!(fake.creates, 0, "unknown never appends");
        assert!(fake.selects.is_empty(), "unknown never dispatches");
    }
}

#[test]
fn trailing_lifecycle_owns_only_created_proven_empty_never_baseline() {
    // Baseline `[1, 277]` (Space277 preexisting keep); runtime creates 21.
    let mut life = TrailingLifecycle::new(&[1, 277]);
    assert!(life.adopt_created(1).is_err(), "baseline never owned");
    assert!(
        life.adopt_created(277).is_err(),
        "preexisting 277 never owned"
    );
    assert!(life.adopt_created(0).is_err(), "zero never adopted");
    life.adopt_created(21).expect("created adopts");
    assert!(life.adopt_created(21).is_err(), "double adopt refuses");
    assert_eq!(life.owned(), &[21]);
    // Proven empty, non-current: eligible.
    let (remove, residue) = life.cleanup_plan(&[1, 277, 21], 1, &|_| Ok(true));
    assert_eq!(remove, vec![21]);
    assert!(residue.is_empty(), "{residue:?}");
    // Occupied owned: residue, never removed.
    let (remove, residue) = life.cleanup_plan(&[1, 277, 21], 1, &|_| Ok(false));
    assert!(remove.is_empty());
    assert_eq!(residue.len(), 1, "foreign/occupied chrome blocks");
    // Unknown emptiness (foreign chrome ambiguity): residue, never removed.
    let (remove, residue) =
        life.cleanup_plan(&[1, 277, 21], 1, &|_| Err("members unreadable".to_owned()));
    assert!(remove.is_empty());
    assert!(residue[0].1.contains("unknown"), "{residue:?}");
    // Current-owned: never removed even when proven empty.
    let (remove, residue) = life.cleanup_plan(&[1, 277, 21], 21, &|_| Ok(true));
    assert!(remove.is_empty());
    assert!(residue[0].1.contains("current"), "{residue:?}");
    // Non-owned SID in the row: never touched (no removal, honest residue
    // for the absent owned SID).
    let (remove, residue) = life.cleanup_plan(&[1, 277, 99], 1, &|_| Ok(true));
    assert!(remove.is_empty(), "non-owned never removed");
    assert_eq!(
        residue.len(),
        1,
        "absent owned reports residue: {residue:?}"
    );
    assert!(residue[0].1.contains("absent"), "{residue:?}");
    // Below-floor protection: owned empty but removal would breach MIN.
    let small = TrailingLifecycle::new(&[1]);
    let mut small = small;
    small.adopt_created(21).unwrap();
    let (remove, residue) = small.cleanup_plan(&[1, 21], 1, &|_| Ok(true));
    assert!(remove.is_empty(), "MIN floor holds");
    assert!(!residue.is_empty());
    // Release after verified removal forgets narrowly; unknown refuses.
    life.release_removed(21).expect("release owned");
    assert!(life.owned().is_empty());
    assert!(life.release_removed(21).is_err(), "double release refuses");
    assert!(life.release_removed(99).is_err(), "non-owned refuses");
}

#[test]
fn trailing_drain_armed_offline_proves_gates_display_and_arrival() {
    // Capability-disabled pin (explicit `false`): trailing refuses with
    // no backend touch. Production is armed (gate `true`): the same drain
    // reuses the proven-empty last below.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        empty: Ok(true),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let err = drain_claimed_select_with(
        &mut state,
        &mut fake,
        crate::runtime_types::ClaimedSelect::tap(Action::WorkspaceSelect { index: 0 }),
        &open_gates(),
        false,
    )
    .expect_err("capability-disabled trailing refuses");
    assert!(err.contains("unarmed"), "{err}");
    assert_eq!(fake.creates, 0);
    assert!(fake.selects.is_empty());
    // Production admission (gate const): armed trailing drains through
    // the same route.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        empty: Ok(true),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let effect = drain_claimed_select_with(
        &mut state,
        &mut fake,
        crate::runtime_types::ClaimedSelect::tap(Action::WorkspaceSelect { index: 0 }),
        &open_gates(),
        TRAILING_CAPABILITY_ENABLED,
    )
    .expect("production trailing armed");
    assert_eq!(effect.target, 277);
    assert_eq!(fake.selects, vec![277]);
    // Explicit local capability through the same production drain: reuse
    // path drains with verified arrival.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        empty: Ok(true),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let effect = drain_claimed_select_with(
        &mut state,
        &mut fake,
        crate::runtime_types::ClaimedSelect::tap(Action::WorkspaceSelect { index: 0 }),
        &open_gates(),
        true,
    )
    .expect("armed trailing drains");
    assert_eq!(effect.target, 277);
    assert_eq!(fake.selects, vec![277]);
    // Single-display gate still applies when armed: 2 displays refuse.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        displays: 2,
        empty: Ok(true),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let err = drain_claimed_select_with(
        &mut state,
        &mut fake,
        crate::runtime_types::ClaimedSelect::tap(Action::WorkspaceSelect { index: 0 }),
        &open_gates(),
        true,
    )
    .expect_err("multi-display refuses even when armed");
    assert!(err.contains("exactly one native display"), "{err}");
    assert!(fake.selects.is_empty());
}

#[test]
fn trailing_ownership_adopts_appends_and_releases_verified_removal() {
    // Full production-path orchestration with fakes: armed drain appends
    // exactly once on occupied trailing, the lifecycle adopts the created
    // SID, and the cleanup plan removes it once proven empty and releases
    // it — never touching the preexisting baseline.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        empty: Ok(false),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let effect = drain_claimed_select_with(
        &mut state,
        &mut fake,
        crate::runtime_types::ClaimedSelect::tap(Action::WorkspaceSelect { index: 0 }),
        &open_gates(),
        true,
    )
    .expect("occupied trailing appends");
    let created = effect.created.expect("exact append recorded");
    let mut life = TrailingLifecycle::new(&[1, 277]);
    assert!(note_effect_created(&mut life, &effect).expect("adopts"));
    assert_eq!(life.owned(), &[created]);
    // Occupied while current work continues: residue, never removed.
    let (remove, _) = life.cleanup_plan(&fake.row.spaces, 1, &|_| Ok(false));
    assert!(remove.is_empty());
    // Proven empty and inactive: the single owned SID removes.
    let (remove, residue) = life.cleanup_plan(&fake.row.spaces, 1, &|_| Ok(true));
    assert_eq!(remove, vec![created]);
    assert!(residue.is_empty(), "{residue:?}");
    life.release_removed(created)
        .expect("release verified removal");
    assert!(life.owned().is_empty());
    // Baseline SIDs are never adopted through the effect path.
    let reuse = SelectionEffect {
        from: 1,
        target: 277,
        created: None,
        changed: true,
    };
    assert!(!note_effect_created(&mut life, &reuse).expect("no-op"));
    assert!(life.owned().is_empty());
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
    // Product subset: exactly the 19 armed rows (numbered 1-9, trailing
    // select-0, the 8
    // relative rows, previous). Never a send chord here.
    let bindings = bindings();
    assert_eq!(bindings.len(), 19);
    assert_eq!(
        bindings
            .iter()
            .filter(|b| matches!(b.action, Action::WorkspaceSelect { index: 0 }))
            .count(),
        1,
        "trailing select-0 armed in product"
    );
    assert!(bindings.iter().all(|b| !b.shift && !b.alt));
    for b in &bindings {
        assert_eq!(b.aux, !matches!(b.action, Action::WorkspaceSelect { .. }));
    }
    // Every armed binding resolves through the armed-select gate.
    for b in &bindings {
        assert!(crate::workspace_input::is_armed_select(&b.action));
    }
    // Capability-disabled subset drops exactly the trailing select-0 row.
    let unarmed = bindings_with(false);
    assert_eq!(unarmed.len(), 18);
    assert!(
        unarmed
            .iter()
            .all(|b| !matches!(b.action, Action::WorkspaceSelect { index: 0 })),
        "capability-disabled trailing select-0 unarmed"
    );
    // Capability-enabled subset adds exactly the trailing select-0 row.
    let armed = bindings_with(true);
    assert_eq!(armed.len(), 19);
    assert_eq!(
        armed
            .iter()
            .filter(|b| matches!(b.action, Action::WorkspaceSelect { index: 0 }))
            .count(),
        1
    );
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

#[test]
fn trailing_probe_scan_reports_first_blocker_and_self_pid() {
    use crate::mc_probe::WindowMember;
    use crate::workspace_select::{TrailingExeKind, scan_trailing_windows};
    let member = |wid: u32, pid: u32, spaces: Vec<u64>| {
        (
            WindowMember {
                wid,
                pid,
                bundle: "com.example.app".to_owned(),
                layer: 0,
                onscreen: true,
                w: 480.0,
                h: 320.0,
                exe: Some("/Applications/Example.app/Contents/MacOS/Example".to_owned()),
                exe_alias: None,
                ax_windows_absent: None,
            },
            Ok(spaces),
        )
    };
    // Occupied: first candidate on the SID blocks with its membership.
    let windows = vec![member(7, 77, vec![20]), member(8, 78, vec![20])];
    let (verdict, checked, shell_skipped, blocker) =
        scan_trailing_windows(20, 999, &[20], &windows);
    assert_eq!(verdict, Ok(false));
    assert_eq!((checked, shell_skipped), (2, 0));
    let blocker = blocker.expect("occupied reports blocker");
    assert_eq!((blocker.wid, blocker.pid), (7, 77));
    assert_eq!(blocker.members, Some(vec![20]));
    assert!(!blocker.self_probe);
    assert_eq!(blocker.exe_kind, TrailingExeKind::Other);
    // Empty: candidate homed elsewhere blocks nothing.
    let windows = vec![member(7, 77, vec![3])];
    let (verdict, _, _, blocker) = scan_trailing_windows(20, 999, &[20, 3], &windows);
    assert_eq!(verdict, Ok(true));
    assert!(blocker.is_none());
    // Unknown membership aborts with the same refusal and no path.
    let windows = vec![(member(9, 77, vec![20]).0, Err("dark".to_owned()))];
    let (verdict, _, _, blocker) = scan_trailing_windows(20, 77, &[20], &windows);
    assert_eq!(verdict, Err("workspace emptiness unknown".to_owned()));
    let blocker = blocker.expect("unknown reports blocker");
    assert_eq!((blocker.wid, blocker.pid), (9, 77));
    assert!(blocker.self_probe, "candidate PID joins current probe PID");
    assert_eq!(blocker.members, None);
    assert_eq!(blocker.exe_kind, TrailingExeKind::Other);
}

#[test]
fn trailing_probe_skips_controlcenter_only_with_ax_empty_proof() {
    // Same proof, same gate: the real captured ControlCenter shape
    // (offscreen layer-25 120x30, exact owner, target-only membership)
    // skips only with a fresh readable empty owner AX inventory.
    use crate::mc_probe::{CONTROLCENTER_BUNDLE, CONTROLCENTER_EXE, WindowMember};
    use crate::workspace_select::scan_trailing_windows;
    let cc = |ax: Option<bool>| {
        (
            WindowMember {
                wid: 5559,
                pid: 656,
                bundle: CONTROLCENTER_BUNDLE.to_owned(),
                layer: 25,
                onscreen: false,
                w: 120.0,
                h: 30.0,
                exe: Some(CONTROLCENTER_EXE.to_owned()),
                exe_alias: None,
                ax_windows_absent: ax,
            },
            Ok(vec![467]),
        )
    };
    // Proven empty: skipped, temp reads empty.
    let (verdict, checked, shell_skipped, blocker) =
        scan_trailing_windows(467, 999, &[1, 277, 467], &[cc(Some(true))]);
    assert_eq!(verdict, Ok(true));
    assert_eq!((checked, shell_skipped), (0, 1));
    assert!(blocker.is_none());
    // Unknown or nonempty AX proof: the shape blocks as a target member.
    for ax in [None, Some(false)] {
        let (verdict, _, _, blocker) = scan_trailing_windows(467, 999, &[1, 277, 467], &[cc(ax)]);
        assert_eq!(verdict, Ok(false));
        let blocker = blocker.expect("unproven shape reports blocker");
        assert_eq!((blocker.wid, blocker.pid), (5559, 656));
        assert_eq!(blocker.members, Some(vec![467]));
    }
}

#[test]
fn trailing_probe_line_is_metadata_only() {
    use crate::workspace_select::{
        TrailingExeKind, TrailingProbeBlocker, format_trailing_probe_line,
    };
    let blocker = TrailingProbeBlocker {
        wid: 7,
        pid: 77,
        bundle: "com.example.app".to_owned(),
        layer: 0,
        onscreen: true,
        w: 480.0,
        h: 320.0,
        members: Some(vec![20]),
        self_probe: false,
        exe_kind: TrailingExeKind::Other,
    };
    let line = format_trailing_probe_line(20, "occupied", 2, 1, Some(&blocker), None, 999);
    assert!(line.contains("sid=20"), "{line}");
    assert!(line.contains("verdict=occupied"), "{line}");
    assert!(line.contains("WID7/PID77"), "{line}");
    assert!(line.contains("members=Some([20])"), "{line}");
    assert!(!line.contains("/Applications"), "{line}");
    assert!(!line.contains("/System"), "{line}");
    let empty = format_trailing_probe_line(20, "empty", 1, 2, None, None, 999);
    assert!(empty.contains("verdict=empty"), "{empty}");
    assert!(!empty.contains("blocker="), "{empty}");
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
    // Trailing select-0 is armed in production: the capability-disabled
    // lane still refuses it with no backend touch.
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let err = drain_claimed_select_with(
        &mut state,
        &mut fake,
        crate::runtime_types::ClaimedSelect::tap(Action::WorkspaceSelect { index: 0 }),
        &open_gates(),
        false,
    )
    .expect_err("capability-disabled drain refuses");
    assert!(err.contains("unarmed"), "{err}");
    assert!(fake.selects.is_empty());
    assert_eq!(fake.creates, 0);
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
/// machine value). Under host-owned adoption the retained ring tracks
/// native order (`[space-1, space-277]`) however the runtime started, so a
/// positional `SelectNumbered{index: 1}` names native Desktop-1 (SID 1)
/// even on a 277-started runtime — no self-target, no adapter-side
/// logical mapping table.
#[cfg(test)]
mod ordinal_parity_repro {
    use std::collections::BTreeMap;

    use tiler_core::directional::OutputId;
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
    /// one positional/explicit command report plus the retained transfers.
    pub fn drive(
        ctx: &NativeContext,
        command: WorldCommand,
    ) -> (WorldOutcome, Vec<tiler_core::world_report::TransferTarget>) {
        use tiler_core::world_engine::WorldEngine;

        let empty_members = BTreeMap::new();
        let empty_memory = crate::runtime_core::LifetimeMemory::empty();
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
                    &empty_memory,
                    &[],
                    &empty_unreadable,
                    &[],
                    "",
                    false,
                );
                input.command_seq = seq;
                input.command = command;
                build_report(&mut input)
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

/// Positional selects resolve native Desktop order under adoption: a
/// runtime started on a non-first Space still selects by native ordinal.
/// `SelectNumbered{index: 1}` on a 277-started runtime names native
/// Desktop-1 (SID 1) — the adopted ring tracks native order, so positional
/// resolution never self-targets the startup current. The admission still
/// pins the native SID plus the live revision, and the drain refuses a
/// moved ordinal instead of selecting the new occupant.
#[test]
fn positional_select_resolves_native_order_under_adoption() {
    use tiler_core::world_report::WorldCommand;

    use ordinal_parity_repro as repro;

    let ctx = repro::ctx(277);
    let output = repro::output();
    let (outcome, retained) = repro::drive(
        &ctx,
        WorldCommand::SelectNumbered {
            output: Some(output.clone()),
            index: 1,
        },
    );
    assert!(
        matches!(outcome, tiler_core::world_engine::WorldOutcome::Accepted(_)),
        "positional numbered select admits"
    );
    assert_eq!(retained.len(), 1, "one retained view");
    assert_eq!(
        retained[0].target_workspace, "ws-1",
        "Desktop-1 through the adopted ring names the adopted logical for native SID 1, never the 277-started current"
    );
}

/// Native Desktop-N resolves positionally core-side under adoption: a
/// runtime started on a non-first Space still selects by native ordinal. A
/// startup on 277 selects Desktop-1 (positional index 1 over the adopted
/// native-order ring), and a startup on 1 keeps the proven Desktop-2 path.
/// Same-Space and unknown targets keep their honest refusals, and the
/// admission pins the native SID plus the live revision so the drain
/// proves the pinned ordinal instead of interpreting it against a new
/// generation.
#[test]
fn explicit_select_resolves_native_ordinal_once_at_admission() {
    use tiler_core::world_engine::WorldOutcome;
    use tiler_core::world_report::{TransferKind, WorldCommand};

    use crate::workspace_send::context_target_sid;
    use crate::world_command_map::select_command;
    use ordinal_parity_repro as repro;

    let select = |index: u8| Action::WorkspaceSelect { index };
    let output = repro::output();

    // Startup on 277: admission pins the native Desktop-1 SID, and the
    // production command carries the positional index — the core resolves
    // it against the adopted native-order ring in the same report.
    let ctx = repro::ctx(277);
    assert!(
        ctx.domains.iter().any(|d| d.space == 277 && d.current),
        "277 is the actuated current"
    );
    let sid = context_target_sid(&ctx, &output, 1).expect("Desktop-1 binds");
    assert_eq!(sid, 1, "Desktop-1 is native SID 1");
    let command = select_command(&select(1), output.clone()).expect("select route");
    assert_eq!(
        command,
        WorldCommand::SelectNumbered {
            output: Some(output.clone()),
            index: 1,
        },
        "select-1 carries a positional index, got {command:?}"
    );
    let (outcome, retained) = repro::drive(&ctx, command);
    assert!(
        matches!(outcome, WorldOutcome::Accepted(_)),
        "Desktop-1 from a 277-started runtime admits"
    );
    assert_eq!(retained.len(), 1, "one retained view");
    assert_eq!(retained[0].kind, TransferKind::View);
    assert_eq!(retained[0].target_output, output);
    assert_eq!(retained[0].target_workspace, "ws-1");
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
    let command = select_command(&select(2), output.clone()).expect("select route");
    let (outcome, retained) = repro::drive(&ctx, command);
    assert!(
        matches!(outcome, WorldOutcome::Accepted(_)),
        "Desktop-2 from a 1-started runtime admits"
    );
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].target_workspace, "ws-2");

    // Bounds/missing targets refuse narrowly at admission (never guessed).
    assert_eq!(context_target_sid(&ctx, &output, 0), None);
    assert_eq!(context_target_sid(&ctx, &output, 10), None);
    assert_eq!(
        select_command(&select(0), output.clone()),
        Some(WorldCommand::SelectTrailing {
            output: Some(output.clone())
        }),
        "trailing select-0 armed through the capability gate"
    );
    assert_eq!(
        crate::world_command_map::select_command_with_capability(&select(0), output.clone(), false),
        None,
        "capability-disabled trailing select-0 has no route"
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
    let relative = select_command(&Action::WorkspaceRelative { prev: true }, output.clone())
        .expect("relative route");
    assert_eq!(
        relative,
        WorldCommand::SelectRelative {
            output: Some(output.clone()),
            delta: -1
        }
    );
    let previous =
        select_command(&Action::WorkspacePrevious, output.clone()).expect("previous route");
    assert_eq!(
        previous,
        WorldCommand::SelectPrevious {
            output: Some(output)
        }
    );
}

#[test]
fn trailing_append_captures_exact_proof_once_and_reuse_proves_nothing() {
    // Occupied trailing appends: the effect and the single proof take
    // agree on the exact rows, and the take clears the slot.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        empty: Ok(false),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let effect = state
        .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
        .expect("occupied trailing appends and selects");
    assert_eq!(effect.created, Some(21));
    let proof = state.take_trailing_proof().expect("proof captured");
    assert_eq!(proof.created, effect.created.unwrap());
    assert_eq!(proof.display_id, "display-a");
    assert_eq!(proof.before, vec![1, 277]);
    assert_eq!(proof.after, vec![1, 277, proof.created]);
    assert!(
        state.take_trailing_proof().is_none(),
        "taking clears the slot"
    );
    // Proven-empty reuse: no append, no proof, no lifecycle adoption.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        empty: Ok(true),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let effect = state
        .execute(&mut fake, Action::WorkspaceSelect { index: 0 })
        .expect("empty trailing reuses");
    assert_eq!(effect.created, None);
    assert!(
        state.take_trailing_proof().is_none(),
        "reuse proves nothing"
    );
}
