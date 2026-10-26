#![cfg(test)]
use crate::runtime_types::{ClaimedSelect, InputSemanticAction as Action};
use crate::workspace_select::*;
use tiler_core::world_report::FrameSetter;

struct Fake {
    row: SelectionSnapshot,
    displays: usize,
    revision: Option<u64>,
    output: String,
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
            output: "macos-display-1".to_owned(),
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
    fn check_output(&mut self, output: &str) -> Result<(), String> {
        if output == self.output {
            Ok(())
        } else {
            Err(format!(
                "product select refuses: reply target output {output} is not the live display {}; no actuation",
                self.output
            ))
        }
    }
    fn empty(&mut self, _: u64) -> Result<bool, String> {
        self.empty.clone()
    }
    fn create(
        &mut self,
        _: &SelectionSnapshot,
    ) -> Result<(), crate::workspace_select::DispatchError> {
        self.creates += 1;
        if self.effect {
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
    fn select(
        &mut self,
        _: &SelectionSnapshot,
        sid: u64,
    ) -> Result<(), crate::workspace_select::DispatchError> {
        self.selects.push(sid);
        if self.effect {
            self.row.current = sid;
        }
        Ok(())
    }
}

fn open_gates() -> SelectGates {
    SelectGates {
        session_unlocked: true,
        secure_active: false,
        lock_screen: false,
    }
}

/// Reply-plan claim for tests: the engine would bind these from the
/// accepted reply (retained target, live binding or creation desire,
/// baseline). Tests inject them directly; production binds them in
/// `bind_select_plan` and rebinds them from fresh replies only.
fn plan_claim(
    action: Action,
    op: &str,
    output: &str,
    workspace: &str,
    binding: Option<&str>,
    creation: Option<(&str, &str, &str)>,
    baseline: Option<u64>,
) -> ClaimedSelect {
    let mut claim = ClaimedSelect::tap(action);
    claim.operation_id = Some(op.to_owned());
    claim.select_target_output = Some(output.to_owned());
    claim.select_target_workspace = Some(workspace.to_owned());
    claim.select_native_binding = binding.map(str::to_owned);
    if let Some((cop, logical, out)) = creation {
        claim.select_creation_operation = Some(cop.to_owned());
        claim.select_creation_logical = Some(logical.to_owned());
        claim.select_creation_output = Some(out.to_owned());
    }
    claim.select_baseline_sid = baseline;
    claim
}

#[test]
fn reply_reuse_selects_bound_target_with_baseline() {
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 2 },
        "op-1",
        "macos-display-1",
        "ws-2",
        Some("space-3"),
        None,
        Some(20),
    );
    let success = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect("bound reuse selects");
    assert_eq!(success.setter, FrameSetter::Accepted);
    assert_eq!((success.effect.from, success.effect.target), (20, 3));
    assert_eq!(success.effect.created, None);
    assert!(success.effect.changed);
    assert_eq!(success.effect.baseline, Some(20));
    assert_eq!((success.pre, success.post), (Some(20), Some(3)));
    assert_eq!(fake.selects, vec![3]);
    assert!(state.take_trailing_proof().is_none());
    assert!(success.creation.is_none());
}

#[test]
fn reply_already_there_is_noop_with_accepted_reads() {
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![20, 3],
            current: 3,
        },
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 2 },
        "op-1",
        "macos-display-1",
        "ws-2",
        Some("space-3"),
        None,
        Some(3),
    );
    let success = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect("already-there noop");
    assert_eq!(success.setter, FrameSetter::Accepted);
    assert!(!success.effect.changed);
    assert_eq!((success.pre, success.post), (Some(3), Some(3)));
    assert!(fake.selects.is_empty());
}

#[test]
fn reply_stale_binding_refuses_with_unchanged_reads_and_no_replay() {
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    // Binding names a SID absent from the live row (reordered/vanished).
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 2 },
        "op-1",
        "macos-display-1",
        "ws-2",
        Some("space-99"),
        None,
        Some(20),
    );
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("stale binding refuses");
    assert!(failure.reason.contains("stale"), "{}", failure.reason);
    // Untouched desktop, genuinely re-read: proven refusal shape.
    assert_eq!(failure.setter, FrameSetter::Refused);
    assert_eq!((failure.pre, failure.post), (Some(20), Some(20)));
    assert_eq!(failure.baseline, Some(20));
    assert!(failure.creation.is_none());
    assert!(fake.selects.is_empty());
    assert_eq!(fake.creates, 0);
}

#[test]
fn reply_missing_binding_without_desire_never_creates() {
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 0 },
        "op-9",
        "macos-display-1",
        "ws-new",
        None,
        None,
        Some(20),
    );
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("missing binding never creates");
    assert!(
        failure.reason.contains("never creates") || failure.reason.contains("no creation"),
        "{}",
        failure.reason
    );
    assert_eq!(failure.setter, FrameSetter::Refused);
    assert_eq!(fake.creates, 0);
    assert!(fake.selects.is_empty());
}

#[test]
fn reply_creation_authorizes_single_append_with_proof_and_baseline() {
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 0 },
        "op-9",
        "macos-display-1",
        "ws-new",
        None,
        Some(("op-native-1", "ws-new", "macos-display-1")),
        Some(1),
    );
    let success = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect("creation desire appends once");
    assert_eq!(success.setter, FrameSetter::Accepted);
    assert_eq!(success.effect.created, Some(21));
    assert_eq!(success.effect.target, 21);
    assert!(success.effect.changed);
    assert_eq!(success.effect.baseline, Some(1));
    assert_eq!((success.pre, success.post), (Some(1), Some(21)));
    assert_eq!(fake.creates, 1);
    assert_eq!(fake.selects, vec![21]);
    let creation = success.creation.expect("creation report rides success");
    assert_eq!(creation.operation_id, "op-native-1");
    assert_eq!(creation.setter, FrameSetter::Accepted);
    assert_eq!(creation.order_before, vec![1, 277]);
    assert_eq!(creation.order_after, vec![1, 277, 21]);
    assert_eq!(creation.created, Some(21));
    let proof = state.take_trailing_proof().expect("proof captured");
    assert_eq!(proof.created, 21);
    assert_eq!(proof.before, vec![1, 277]);
    assert_eq!(proof.after, vec![1, 277, 21]);
    assert!(state.take_trailing_proof().is_none());
}

#[test]
fn double_drain_of_same_claim_creates_only_once() {
    // One authorization creates at most once: the second drain of the
    // identical claim refuses without touching native - no replay, no
    // second Space, no second proof.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let claim = || {
        plan_claim(
            Action::WorkspaceSelect { index: 0 },
            "op-9",
            "macos-display-1",
            "ws-new",
            None,
            Some(("op-native-1", "ws-new", "macos-display-1")),
            Some(1),
        )
    };
    let first = drain_claimed_select_with(&mut state, &mut fake, claim(), &open_gates(), true)
        .expect("first drain creates");
    assert_eq!(first.effect.created, Some(21));
    assert!(state.take_trailing_proof().is_some());
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim(), &open_gates(), true)
        .expect_err("second drain refuses without replay");
    assert!(failure.reason.contains("spent"), "{}", failure.reason);
    assert_eq!(failure.setter, FrameSetter::Refused);
    assert_eq!(fake.creates, 1, "exactly one append, never replayed");
    assert_eq!(fake.selects, vec![21], "exactly one select, never replayed");
    assert!(state.take_trailing_proof().is_none());
}

#[test]
fn reply_creation_mismatch_refuses_without_append() {
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    // Desire names a different logical/output than the retained target.
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 0 },
        "op-9",
        "macos-display-1",
        "ws-new",
        None,
        Some(("op-native-1", "ws-other", "macos-display-1")),
        Some(1),
    );
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("mismatched desire refuses");
    assert!(failure.reason.contains("mismatch"), "{}", failure.reason);
    assert_eq!(fake.creates, 0);
    assert!(failure.creation.is_none());
}

#[test]
fn reply_creation_refusal_reports_unknown_without_fabricated_after_order() {
    struct FailCreate {
        inner: Fake,
        post_unreadable: bool,
    }
    impl SelectionBackend for FailCreate {
        fn display_count(&mut self) -> Result<usize, String> {
            self.inner.display_count()
        }
        fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
            if self.post_unreadable {
                Err("post-order unreadable".to_owned())
            } else {
                self.inner.snapshot()
            }
        }
        fn revision(&mut self) -> Option<u64> {
            self.inner.revision()
        }
        fn check_output(&mut self, output: &str) -> Result<(), String> {
            self.inner.check_output(output)
        }
        fn empty(&mut self, sid: u64) -> Result<bool, String> {
            self.inner.empty(sid)
        }
        fn create(
            &mut self,
            _: &SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            self.post_unreadable = true;
            Err(DispatchError::threw("MC open failed mid-leg"))
        }
        fn select(
            &mut self,
            _: &SelectionSnapshot,
            _: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            unreachable!("no select follows a failed create");
        }
    }
    let mut backend = FailCreate {
        post_unreadable: false,
        inner: Fake {
            row: SelectionSnapshot {
                display: "display-a".to_owned(),
                spaces: vec![1, 277],
                current: 1,
            },
            ..Fake::default()
        },
    };
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 0 },
        "op-9",
        "macos-display-1",
        "ws-new",
        None,
        Some(("op-native-1", "ws-new", "macos-display-1")),
        Some(1),
    );
    let failure = drain_claimed_select_with(&mut state, &mut backend, claim, &open_gates(), true)
        .expect_err("failed create holds");
    // Ambiguous mid-actuation output: the typed Threw disposition is
    // preserved verbatim (a partial dispatch may have run — never a proven
    // refusal), and the unreadable after-order stages nothing fabricated.
    assert_eq!(
        failure.setter,
        FrameSetter::Threw,
        "partial dispatch may have run"
    );
    assert_eq!(failure.pre, None);
    assert_eq!(failure.baseline, Some(1));
    assert!(
        failure.creation.is_none(),
        "unknown after-order must not be substituted"
    );
    assert_eq!(state.pending_creation_count(), 1);
    assert!(state.poll_pending_creations(&mut backend).is_empty());
    backend.post_unreadable = false;
    let receipts = state.poll_pending_creations(&mut backend);
    assert_eq!(receipts.len(), 1);
    let creation = &receipts[0];
    assert_eq!(creation.operation_id, "op-native-1");
    assert_eq!(creation.setter, FrameSetter::Threw);
    assert_eq!(creation.order_before, vec![1, 277]);
    assert_eq!(
        creation.order_after,
        vec![1, 277],
        "first readable full post-order, even with no delta"
    );
    assert_eq!(creation.attempt_ms, failure.attempt_ms);
    assert_eq!(creation.created, None);
    assert!(
        state.poll_pending_creations(&mut backend).is_empty(),
        "receipt completes once"
    );
    assert!(state.take_trailing_proof().is_none());
}

#[test]
fn reply_creation_no_delta_defers_receipt_without_emission() {
    // Dispatched create with a readable row that shows no delta yet: the
    // desktop may simply not be visible, so the drain retains a pending
    // receipt (Accepted dispatch truth) and emits nothing — a no-delta
    // emission here would turn the later delta into a changed replay.
    struct NoDelta {
        row: SelectionSnapshot,
    }
    impl SelectionBackend for NoDelta {
        fn display_count(&mut self) -> Result<usize, String> {
            Ok(1)
        }
        fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
            Ok(self.row.clone())
        }
        fn revision(&mut self) -> Option<u64> {
            None
        }
        fn check_output(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn empty(&mut self, _: u64) -> Result<bool, String> {
            Ok(false)
        }
        fn create(
            &mut self,
            _: &SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            Ok(())
        }
        fn select(
            &mut self,
            _: &SelectionSnapshot,
            _: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            unreachable!("no select follows an unverified create");
        }
    }
    let row = SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![1, 277],
        current: 1,
    };
    let mut backend = NoDelta { row };
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 0 },
        "op-9",
        "macos-display-1",
        "ws-new",
        None,
        Some(("op-native-1", "ws-new", "macos-display-1")),
        Some(1),
    );
    let failure = drain_claimed_select_with(&mut state, &mut backend, claim, &open_gates(), true)
        .expect_err("unvisible create defers");
    assert!(
        failure.reason.contains("not yet visible"),
        "{}",
        failure.reason
    );
    assert_eq!(
        failure.setter,
        FrameSetter::Refused,
        "no view was attempted"
    );
    assert!(
        failure.creation.is_none(),
        "no-delta must not emit a completed receipt"
    );
    assert_eq!(state.pending_creation_count(), 1);
    // The first later delta completes the retained receipt exactly once
    // with the original dispatch stamp and no claimed identity.
    backend.row.spaces.push(21);
    let receipts = state.poll_pending_creations(&mut backend);
    assert_eq!(receipts.len(), 1);
    let creation = &receipts[0];
    assert_eq!(creation.setter, FrameSetter::Accepted);
    assert_eq!(creation.order_before, vec![1, 277]);
    assert_eq!(creation.order_after, vec![1, 277, 21]);
    assert_eq!(creation.created, None, "a later-turn delta is uncorrelated");
    assert_eq!(creation.attempt_ms, failure.attempt_ms);
}

#[test]
fn reply_creation_disagree_reports_threw_with_genuine_after() {
    // Dispatched create with a readable row that disagrees (outside race
    // added a Space): the drain completes immediately as Threw with the
    // genuine after-image and no guessed SID — never a silent no-delta.
    struct Disagree {
        before: SelectionSnapshot,
        after: SelectionSnapshot,
        phase: u8,
    }
    impl SelectionBackend for Disagree {
        fn display_count(&mut self) -> Result<usize, String> {
            Ok(1)
        }
        fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
            self.phase += 1;
            Ok(if self.phase == 1 {
                self.before.clone()
            } else {
                self.after.clone()
            })
        }
        fn revision(&mut self) -> Option<u64> {
            None
        }
        fn check_output(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn empty(&mut self, _: u64) -> Result<bool, String> {
            Ok(false)
        }
        fn create(
            &mut self,
            _: &SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            Ok(())
        }
        fn select(
            &mut self,
            _: &SelectionSnapshot,
            _: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            unreachable!("no select follows a disagreeing create");
        }
    }
    let mut backend = Disagree {
        before: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
        after: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277, 99, 100],
            current: 1,
        },
        phase: 0,
    };
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 0 },
        "op-9",
        "macos-display-1",
        "ws-new",
        None,
        Some(("op-native-1", "ws-new", "macos-display-1")),
        Some(1),
    );
    let failure = drain_claimed_select_with(&mut state, &mut backend, claim, &open_gates(), true)
        .expect_err("disagreeing create holds as Threw");
    assert!(
        failure.reason.contains("not an exact same-display append"),
        "{}",
        failure.reason
    );
    assert_eq!(
        failure.setter,
        FrameSetter::Threw,
        "ambiguity holds, never a proven refusal"
    );
    let creation = failure
        .creation
        .expect("genuine after-image completes immediately");
    assert_eq!(creation.setter, FrameSetter::Threw);
    assert_eq!(creation.order_before, vec![1, 277]);
    assert_eq!(creation.order_after, vec![1, 277, 99, 100]);
    assert_eq!(creation.created, None, "a raced row names no identity");
    assert_eq!(
        state.pending_creation_count(),
        0,
        "completed receipts never pend"
    );
}

#[test]
fn deferred_creation_never_claims_causal_identity_or_changes_setter() {
    let pending = PendingCreation {
        operation_id: "op-native-1".to_owned(),
        logical: "ws-new".to_owned(),
        output: tiler_core::directional::OutputId("macos-display-1".to_owned()),
        display_id: "display-a".to_owned(),
        order_before: vec![20, 3],
        setter: FrameSetter::Accepted,
        attempt_ms: 42,
    };
    let row = SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![3, 20, 99, 100],
        current: 20,
    };
    assert!(
        pending.observe(&row, 42).is_none(),
        "same-ms capture cannot prove ordering"
    );
    let report = pending.observe(&row, 43).unwrap();
    assert_eq!(
        report.setter,
        FrameSetter::Accepted,
        "outside reorders do not change dispatch truth"
    );
    assert_eq!(report.order_after, row.spaces);
    assert_eq!(report.created, None, "later additions are uncorrelated");
    assert_eq!(report.attempt_ms, 42);
    let foreign = SelectionSnapshot {
        display: "display-b".to_owned(),
        ..row
    };
    assert!(pending.observe(&foreign, 44).is_none());
}

#[test]
fn reply_unparsable_binding_refuses() {
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 2 },
        "op-1",
        "macos-display-1",
        "ws-2",
        Some("desktop-3"),
        None,
        Some(20),
    );
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("unparsable binding refuses");
    assert!(failure.reason.contains("unparsable"), "{}", failure.reason);
    assert_eq!(failure.setter, FrameSetter::Refused);
    assert!(fake.selects.is_empty());
}

#[test]
fn reply_indicator_mismatch_refuses_stale_click_with_unchanged_reads() {
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    // Indicator rendered Space 20 but the reply binding now names Space 3
    // (generation moved under the click): refuse, never actuate the new
    // occupant under the old click. Both reads genuinely name the
    // untouched current.
    let mut claim = plan_claim(
        Action::WorkspaceSelect { index: 1 },
        "op-1",
        "macos-display-1",
        "ws-1",
        Some("space-3"),
        None,
        Some(20),
    );
    claim.expected_sid = Some(20);
    claim.topology_revision = Some(9);
    fake.revision = Some(9);
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("indicator mismatch refuses");
    assert!(
        failure.reason.contains("no longer names"),
        "{}",
        failure.reason
    );
    assert_eq!(failure.setter, FrameSetter::Refused);
    assert_eq!((failure.pre, failure.post), (Some(20), Some(20)));
    assert!(fake.selects.is_empty());
}

#[test]
fn reply_revision_drift_refuses_indicator() {
    let mut fake = Fake {
        revision: Some(10),
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let mut claim = plan_claim(
        Action::WorkspaceSelect { index: 1 },
        "op-1",
        "macos-display-1",
        "ws-1",
        Some("space-20"),
        None,
        Some(20),
    );
    claim.expected_sid = Some(20);
    claim.topology_revision = Some(9);
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("revision drift refuses");
    assert!(
        failure.reason.contains("revision moved"),
        "{}",
        failure.reason
    );
    assert_eq!(failure.setter, FrameSetter::Refused);
    assert!(fake.selects.is_empty());
}

#[test]
fn reply_output_fence_refuses_foreign_display_without_touch() {
    // The plan names another output while the backend proves only
    // macos-display-1: refuse before any native touch, never switch the
    // wrong display.
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 2 },
        "op-1",
        "macos-display-2",
        "ws-2",
        Some("space-3"),
        None,
        Some(20),
    );
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("foreign output refuses");
    assert!(
        failure.reason.contains("not the live display") || failure.reason.contains("output"),
        "{}",
        failure.reason
    );
    assert_eq!(failure.setter, FrameSetter::Refused);
    assert!(fake.selects.is_empty());
    assert_eq!(fake.creates, 0);
}

#[test]
fn reply_select_failure_after_append_stages_throw_with_proof() {
    // Verified append followed by a failed select: the Space persists
    // (proof adoptable) while the view attempt holds as Threw with
    // genuine reads - never a false success, never a silent loss.
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
        fn check_output(&mut self, output: &str) -> Result<(), String> {
            self.inner.check_output(output)
        }
        fn empty(&mut self, sid: u64) -> Result<bool, String> {
            self.inner.empty(sid)
        }
        fn create(
            &mut self,
            before: &SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            self.inner.create(before)
        }
        fn select(
            &mut self,
            _: &SelectionSnapshot,
            _: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            Err(DispatchError::threw("goto failed"))
        }
    }
    let mut backend = FailSelect {
        inner: Fake {
            row: SelectionSnapshot {
                display: "display-a".to_owned(),
                spaces: vec![1, 277],
                current: 1,
            },
            ..Fake::default()
        },
    };
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 0 },
        "op-9",
        "macos-display-1",
        "ws-new",
        None,
        Some(("op-native-1", "ws-new", "macos-display-1")),
        Some(1),
    );
    let failure = drain_claimed_select_with(&mut state, &mut backend, claim, &open_gates(), true)
        .expect_err("failed goto holds");
    assert!(failure.reason.contains("goto failed"), "{}", failure.reason);
    assert_eq!(failure.setter, FrameSetter::Threw);
    assert_eq!(failure.pre, Some(1));
    assert_eq!(failure.baseline, Some(1));
    // The verified append is still reported (Accepted create leg) and
    // adoptable even though the select held.
    let creation = failure.creation.expect("append proof rides the failure");
    assert_eq!(creation.setter, FrameSetter::Accepted);
    assert_eq!(creation.created, Some(21));
    assert_eq!(creation.order_after, vec![1, 277, 21]);
    let proof = state.take_trailing_proof().expect("proof captured");
    assert_eq!(proof.created, 21);
    assert!(state.take_trailing_proof().is_none());
}

#[test]
fn reply_accepted_with_unreadable_post_stays_accepted() {
    // Successful dispatch followed by an unreadable post read stays
    // Accepted with post unknown - never an invented Threw. The core
    // holds on the genuine pre-write read instead of settling either way.
    struct UnreadablePost {
        inner: Fake,
        reads: usize,
    }
    impl SelectionBackend for UnreadablePost {
        fn display_count(&mut self) -> Result<usize, String> {
            self.inner.display_count()
        }
        fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
            self.reads += 1;
            // First read (pre) succeeds; the post-arrival readback is
            // unreadable.
            if self.reads <= 1 {
                self.inner.snapshot()
            } else {
                Err("post snapshot unreadable".to_owned())
            }
        }
        fn revision(&mut self) -> Option<u64> {
            self.inner.revision()
        }
        fn check_output(&mut self, output: &str) -> Result<(), String> {
            self.inner.check_output(output)
        }
        fn empty(&mut self, sid: u64) -> Result<bool, String> {
            self.inner.empty(sid)
        }
        fn create(
            &mut self,
            before: &SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            self.inner.create(before)
        }
        fn select(
            &mut self,
            before: &SelectionSnapshot,
            sid: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            self.inner.select(before, sid)
        }
    }
    let mut backend = UnreadablePost {
        inner: Fake::default(),
        reads: 0,
    };
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 2 },
        "op-1",
        "macos-display-1",
        "ws-2",
        Some("space-3"),
        None,
        Some(20),
    );
    let success = drain_claimed_select_with(&mut state, &mut backend, claim, &open_gates(), true)
        .expect("unreadable post still Accepted");
    assert_eq!(success.setter, FrameSetter::Accepted);
    assert_eq!(success.pre, Some(20));
    assert_eq!(success.post, None);
}

#[test]
fn drain_gates_display_and_unarmed_refuse_without_touch() {
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
        let claim = plan_claim(
            Action::WorkspaceSelect { index: 2 },
            "op-1",
            "macos-display-1",
            "ws-2",
            Some("space-3"),
            None,
            Some(20),
        );
        let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &gates, true)
            .expect_err("gated drain refuses");
        assert!(failure.reason.contains("gated"), "{}", failure.reason);
        assert_eq!(failure.setter, FrameSetter::Refused);
        assert_eq!((failure.pre, failure.post), (None, None));
        assert!(fake.selects.is_empty());
        assert_eq!(fake.creates, 0);
    }
    // Unarmed actions refuse before any backend touch.
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSend {
            index: 1,
            follow: true,
        },
        "op-1",
        "macos-display-1",
        "ws-2",
        Some("space-3"),
        None,
        Some(20),
    );
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("unarmed refuses");
    assert!(failure.reason.contains("unarmed"), "{}", failure.reason);
    assert_eq!(failure.setter, FrameSetter::Refused);
    assert!(fake.selects.is_empty());
    // Multi-display refuses before any effect.
    let mut fake = Fake {
        displays: 2,
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSelect { index: 2 },
        "op-1",
        "macos-display-1",
        "ws-2",
        Some("space-3"),
        None,
        Some(20),
    );
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("multi-display refuses");
    assert!(
        failure.reason.contains("exactly one native display"),
        "{}",
        failure.reason
    );
    // Missing operation identity never actuates.
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let mut claim = plan_claim(
        Action::WorkspaceSelect { index: 2 },
        "op-1",
        "macos-display-1",
        "ws-2",
        Some("space-3"),
        None,
        Some(20),
    );
    claim.operation_id = None;
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("unbound refuses");
    assert!(failure.reason.contains("unbound"), "{}", failure.reason);
    assert!(fake.selects.is_empty());
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
    // Unarmed send shapes never enter the select drain.
    let mut fake = Fake::default();
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspaceSend {
            index: 1,
            follow: true,
        },
        "op-1",
        "macos-display-1",
        "ws-2",
        Some("space-3"),
        None,
        Some(20),
    );
    let failure = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect_err("send refuses on select lane");
    assert!(failure.reason.contains("unarmed"), "{}", failure.reason);
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

/// Reply-driven previous toggle through a core-resolved plan (manual plan
/// standing in for the engine-bound reply): the drain actuates the bound
/// SID once with a truthful baseline and no local history.
#[test]
fn core_previous_drives_reply_drain_without_local_history() {
    // Previous target `space-20` bound by core (baseline is the
    // observation-time current 3); the live row contains both SIDs.
    let mut fake = Fake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![20, 3],
            current: 3,
        },
        ..Fake::default()
    };
    let mut state = SelectionState::default();
    let claim = plan_claim(
        Action::WorkspacePrevious,
        "op-prev",
        "macos-display-1",
        "ws-prev",
        Some("space-20"),
        None,
        Some(3),
    );
    let success = drain_claimed_select_with(&mut state, &mut fake, claim, &open_gates(), true)
        .expect("previous plan drains");
    assert_eq!(success.setter, FrameSetter::Accepted);
    assert_eq!((success.effect.from, success.effect.target), (3, 20));
    assert_eq!(success.effect.baseline, Some(3));
    assert!(success.effect.changed);
    assert_eq!(fake.selects, vec![20]);
}
