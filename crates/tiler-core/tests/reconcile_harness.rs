#![forbid(unsafe_code)]
//! Test-only headless fake compositor/adapter harness for the reconciler.
//!
//! This is not a production runtime abstraction: it owns fake snapshots,
//! capabilities, and correlation ids in-test and drives the actual
//! [`tiler_core::reconcile::Reconciler`] API. Every scenario asserts
//! real reconciler state transitions (`StatusView`, `Commit`, typed
//! `DivergenceKind`) rather than reimplementing reconciler logic.

use tiler_core::contract::DivergenceKind;
use tiler_core::contract::{AckOutcome, AdapterAck, Observation, PostObservation};
use tiler_core::directional::{
    Axis, Capabilities, Direction, MoveIntent, MoveOperation, MoveOutcome, MovePlan, Node, NodeId,
    Output, OutputId, Precondition, Rule, Snapshot, WindowId, WindowLink, WorkspaceId, plan_move,
};
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::reconcile::{
    AckApplied, AckError, ProposeError, Reconciler, StateKind, VerifyError,
};

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid owner")
}

fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid generation")
}

fn correlation_id(value: &str) -> CorrelationId {
    CorrelationId::parse(value).unwrap_or_else(|| panic!("valid correlation {value}"))
}

/// Fake headless compositor: owns the snapshot the real planner runs
/// against, plus the adapter-contract view (owner/generation/revision,
/// capabilities, correlation counter).
struct FakeCompositor {
    reconciler: Reconciler,
    capabilities: Capabilities,
    snapshot: Snapshot,
    intent: MoveIntent,
    next_correlation: u64,
}

impl FakeCompositor {
    fn new() -> Self {
        let snapshot = r2a_snapshot();
        let intent = MoveIntent {
            source_output: OutputId::from("source"),
            focused_leaf: NodeId::from("A"),
            focused_window: WindowId::from("w-A"),
            direction: Direction::Right,
            same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        };
        Self {
            reconciler: Reconciler::new(owner(), generation(), 0, 11).expect("valid seed"),
            capabilities: Capabilities::full(),
            snapshot,
            intent,
            next_correlation: 1,
        }
    }

    /// Injectable fault surface: swap the capability set the harness offers
    /// at `propose` time (e.g. `Capabilities::none()` for refusal).
    fn with_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    fn observation(&self, revision: u64) -> Observation {
        Observation::new(owner(), generation(), revision, 11)
    }

    /// Real planner plan against the owned fake snapshot.
    fn real_plan(&self) -> MovePlan {
        match plan_move(&self.snapshot, &self.intent) {
            MoveOutcome::Planned(plan) => plan,
            other => panic!("expected real planner plan, got {other:?}"),
        }
    }

    fn correlation(&mut self) -> CorrelationId {
        let id = format!("corr-{}", self.next_correlation);
        self.next_correlation += 1;
        correlation_id(&id)
    }

    fn ack(&self, correlation: &CorrelationId, base: u64, outcome: AckOutcome) -> AdapterAck {
        AdapterAck::new(correlation.clone(), owner(), generation(), base, outcome)
    }

    fn ack_str(&self, correlation: &str, base: u64, outcome: AckOutcome) -> AdapterAck {
        self.ack(&correlation_id(correlation), base, outcome)
    }

    fn post(
        &self,
        correlation: &CorrelationId,
        revision: u64,
        verified: bool,
        verified_preconditions: Vec<Precondition>,
        verified_operation: MoveOperation,
    ) -> PostObservation {
        PostObservation::new(
            Observation::new(owner(), generation(), revision, 22),
            correlation.clone(),
            verified,
            verified_preconditions,
            verified_operation,
        )
    }

    fn post_for_plan(
        &self,
        correlation: &CorrelationId,
        revision: u64,
        verified: bool,
        plan: &MovePlan,
    ) -> PostObservation {
        self.post(
            correlation,
            revision,
            verified,
            plan.preconditions.clone(),
            plan.operation.clone(),
        )
    }
}

/// Minimal R2a fixture: single output `source`, root group `[A, B]`, moving
/// focus `A` right swaps with neighbor `B`.
fn r2a_snapshot() -> Snapshot {
    let tree = Node::Group {
        id: NodeId::from("root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                id: NodeId::from("A"),
            },
            Node::Leaf {
                id: NodeId::from("B"),
            },
        ],
        shares: vec![1, 1],
    };
    let output = Output {
        id: OutputId::from("source"),
        workspace: WorkspaceId::from("workspace-1"),
        tree: Some(tree),
        adjacent: Default::default(),
    };
    let windows = vec![
        WindowLink {
            window: WindowId::from("w-A"),
            leaf: NodeId::from("A"),
            output: OutputId::from("source"),
            workspace: WorkspaceId::from("workspace-1"),
        },
        WindowLink {
            window: WindowId::from("w-B"),
            leaf: NodeId::from("B"),
            output: OutputId::from("source"),
            workspace: WorkspaceId::from("workspace-1"),
        },
    ];
    Snapshot {
        outputs: vec![output],
        windows,
    }
}

#[test]
fn converges_with_real_planner_plan() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    assert_eq!(plan.rule, Rule::R2a);
    let correlation = fake.correlation();
    let base = fake.reconciler.verified_revision();

    let dispatch = fake
        .reconciler
        .propose(
            &plan,
            &fake.observation(base),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose real plan");
    assert_eq!(dispatch.base_revision, base);
    assert_eq!(dispatch.correlation_id.as_str(), correlation.as_str());
    assert_eq!(dispatch.owner.as_str(), "owner-1");
    assert_eq!(dispatch.generation.as_str(), "gen-1");
    assert_eq!(dispatch.operation, plan.operation);
    assert_eq!(dispatch.intent, plan.intent);
    assert_eq!(dispatch.preconditions, plan.preconditions);
    assert_eq!(fake.reconciler.status().state, StateKind::PendingUnacked);

    assert_eq!(
        fake.reconciler
            .acknowledge(&fake.ack(&correlation, base, AckOutcome::Accepted)),
        Ok(AckApplied::Accepted)
    );
    assert_eq!(fake.reconciler.status().state, StateKind::PendingAcked);

    let commit = fake
        .reconciler
        .verify(&fake.post_for_plan(&correlation, base, true, &plan))
        .expect("verified post commits");
    assert_eq!(commit.revision, base + 1);
    assert_eq!(fake.reconciler.verified_revision(), base + 1);
    let status = fake.reconciler.status();
    assert_eq!(status.state, StateKind::Verified);
    assert!(!status.pending);
    assert_eq!(status.divergence, None);
}

#[test]
fn dispatch_carries_executable_semantic_operation() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    let dispatch = fake
        .reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");

    // The future adapter executes from this payload alone: semantic operation
    // plus its bound rule/capability/preconditions, with no geometry or native
    // handles anywhere in the envelope.
    assert_eq!(dispatch.base_revision, 0);
    assert_eq!(dispatch.correlation_id.as_str(), correlation.as_str());
    assert_eq!(dispatch.owner.as_str(), "owner-1");
    assert_eq!(dispatch.generation.as_str(), "gen-1");
    assert_eq!(dispatch.operation, plan.operation);
    assert_eq!(
        dispatch.operation,
        MoveOperation::SwapNeighbor {
            rule: Rule::R2a,
            container: NodeId::from("root"),
            neighbor: NodeId::from("B"),
        }
    );
    assert_eq!(dispatch.rule, Rule::R2a);
    assert_eq!(dispatch.required_capability, plan.required_capability);
    assert_eq!(dispatch.preconditions, plan.preconditions);
    assert_eq!(dispatch.intent, plan.intent);
    // Operation carries only opaque structural ids: the container/neighbor it
    // names must be the planner's ids, and the debug rendering must not leak
    // geometry or native-handle concepts.
    let rendered = format!("{:?}", dispatch.operation);
    assert!(rendered.contains("root"));
    assert!(rendered.contains("B"));
    assert!(!rendered.contains("pixel"));
    assert!(!rendered.contains("geometry"));
    assert!(!rendered.contains("handle"));
    assert!(!rendered.contains("fd"));
}

#[test]
fn stale_snapshot_revision_diverges_terminal() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    // Injectable stale fault: observation revision ahead of verified state.
    let err = fake
        .reconciler
        .propose(
            &plan,
            &fake.observation(7),
            &correlation,
            &fake.capabilities,
        )
        .expect_err("stale revision must diverge");
    assert_eq!(err, ProposeError::Diverged(DivergenceKind::StaleRevision));
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
    // Terminal: no further dispatch or mutation is possible.
    assert_eq!(
        fake.reconciler.propose(
            &plan,
            &fake.observation(0),
            &correlation_id("corr-2"),
            &fake.capabilities
        ),
        Err(ProposeError::Diverged(DivergenceKind::StaleRevision))
    );
    assert_eq!(
        fake.reconciler
            .acknowledge(&fake.ack_str("corr-2", 0, AckOutcome::Accepted)),
        Err(AckError::Diverged(DivergenceKind::StaleRevision))
    );
    assert_eq!(fake.reconciler.verified_revision(), 0);
}

#[test]
fn partial_application_diverges() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    // Injectable partial-application fault from the fake adapter.
    assert_eq!(
        fake.reconciler
            .acknowledge(&fake.ack(&correlation, 0, AckOutcome::PartialApplication)),
        Err(AckError::Diverged(DivergenceKind::PartialApplication))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
    assert_eq!(
        fake.reconciler.status().divergence,
        Some(DivergenceKind::PartialApplication)
    );
}

#[test]
fn unverified_postcondition_diverges() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    fake.reconciler
        .acknowledge(&fake.ack(&correlation, 0, AckOutcome::Accepted))
        .expect("ack");
    // Injectable fault: adapter did not natively verify postconditions.
    assert_eq!(
        fake.reconciler
            .verify(&fake.post_for_plan(&correlation, 0, false, &plan)),
        Err(VerifyError::Diverged(
            DivergenceKind::PostconditionUnverified
        ))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
}

#[test]
fn mismatched_verified_preconditions_diverge_without_commit() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    let dispatch = fake
        .reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    assert_eq!(dispatch.intent, plan.intent);
    assert_eq!(dispatch.preconditions, plan.preconditions);
    fake.reconciler
        .acknowledge(&fake.ack(&correlation, 0, AckOutcome::Accepted))
        .expect("ack");
    // Injectable fault: adapter reports a verified vector that does not bind
    // exactly to the dispatched preconditions.
    let mut mismatched = plan.preconditions.clone();
    mismatched.pop();
    assert_eq!(
        fake.reconciler.verify(&fake.post(
            &correlation,
            0,
            true,
            mismatched,
            plan.operation.clone()
        )),
        Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
    assert_eq!(
        fake.reconciler.status().divergence,
        Some(DivergenceKind::PostconditionMismatch)
    );
    assert_eq!(fake.reconciler.verified_revision(), 0);
}

#[test]
fn stale_post_revision_diverges() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    fake.reconciler
        .acknowledge(&fake.ack(&correlation, 0, AckOutcome::Accepted))
        .expect("ack");
    // Injectable fault: post-observation is not at the pending base revision.
    assert_eq!(
        fake.reconciler
            .verify(&fake.post_for_plan(&correlation, 9, true, &plan)),
        Err(VerifyError::Diverged(DivergenceKind::StaleRevision))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
}

#[test]
fn plan_missing_verify_precondition_is_postcondition_mismatch() {
    let mut fake = FakeCompositor::new();
    let mut plan = fake.real_plan();
    // Injectable fault: strip the mandatory adapter-verification precondition.
    plan.preconditions
        .retain(|p| *p != tiler_core::directional::Precondition::AdapterMustVerifyPostconditions);
    assert_eq!(
        {
            let obs = fake.observation(0);
            let corr = fake.correlation();
            let caps = fake.capabilities.clone();
            fake.reconciler.propose(&plan, &obs, &corr, &caps)
        },
        Err(ProposeError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
}

#[test]
fn duplicate_accepted_acknowledgement_is_discarded() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    assert_eq!(
        fake.reconciler
            .acknowledge(&fake.ack(&correlation, 0, AckOutcome::Accepted)),
        Ok(AckApplied::Accepted)
    );
    // Duplicate replay of the identical accepted ack: discarded, no advance.
    assert_eq!(
        fake.reconciler
            .acknowledge(&fake.ack(&correlation, 0, AckOutcome::Accepted)),
        Ok(AckApplied::DuplicateDiscarded)
    );
    assert_eq!(fake.reconciler.status().state, StateKind::PendingAcked);
    assert_eq!(fake.reconciler.verified_revision(), 0);
    // The pending plan is still committable exactly once.
    let commit = fake
        .reconciler
        .verify(&fake.post_for_plan(&correlation, 0, true, &plan))
        .expect("commit after duplicate");
    assert_eq!(commit.revision, 1);
}

#[test]
fn out_of_order_ack_and_early_verify_are_rejected() {
    // Out-of-order correlation diverges fail-closed.
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    assert_eq!(
        fake.reconciler
            .acknowledge(&fake.ack_str("corr-999", 0, AckOutcome::Accepted)),
        Err(AckError::Diverged(DivergenceKind::CorrelationMismatch))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);

    // Verification before acknowledgement is rejected without divergence.
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    assert_eq!(
        fake.reconciler
            .verify(&fake.post_for_plan(&correlation, 0, true, &plan)),
        Err(VerifyError::NotAcknowledged)
    );
    assert_eq!(fake.reconciler.status().state, StateKind::PendingUnacked);

    // Acknowledgement with no pending plan is discarded without divergence.
    let mut fake = FakeCompositor::new();
    assert_eq!(
        fake.reconciler
            .acknowledge(&fake.ack_str("corr-9", 0, AckOutcome::Accepted)),
        Err(AckError::NoPending)
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Verified);
}

#[test]
fn capability_mismatch_and_refusal_diverge() {
    // Propose-time refusal: fake adapter declares no capabilities.
    let mut fake = FakeCompositor::new().with_capabilities(Capabilities::none());
    let plan = fake.real_plan();
    assert_eq!(
        {
            let obs = fake.observation(0);
            let corr = fake.correlation();
            let caps = fake.capabilities.clone();
            fake.reconciler.propose(&plan, &obs, &corr, &caps)
        },
        Err(ProposeError::Diverged(DivergenceKind::CapabilityRefused))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);

    // Ack-time refusal: adapter accepts dispatch then refuses the capability.
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    assert_eq!(
        fake.reconciler
            .acknowledge(&fake.ack(&correlation, 0, AckOutcome::RefusedCapability)),
        Err(AckError::Diverged(DivergenceKind::CapabilityRefused))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
}

#[test]
fn adapter_loss_is_terminal() {
    // Explicit loss signal with no pending plan.
    let mut fake = FakeCompositor::new();
    assert_eq!(
        fake.reconciler.note_adapter_loss(),
        DivergenceKind::AdapterLost
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
    let plan = fake.real_plan();
    assert_eq!(
        {
            let obs = fake.observation(0);
            let corr = fake.correlation();
            let caps = fake.capabilities.clone();
            fake.reconciler.propose(&plan, &obs, &corr, &caps)
        },
        Err(ProposeError::Diverged(DivergenceKind::AdapterLost))
    );

    // Loss reported as the ack outcome also diverges terminally.
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    assert_eq!(
        fake.reconciler
            .acknowledge(&fake.ack(&correlation, 0, AckOutcome::AdapterLost)),
        Err(AckError::Diverged(DivergenceKind::AdapterLost))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
}

#[test]
fn dispatch_binds_owner_generation_identity() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    let dispatch = fake
        .reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    assert_eq!(dispatch.correlation_id.as_str(), correlation.as_str());
    assert_eq!(dispatch.owner.as_str(), "owner-1");
    assert_eq!(dispatch.generation.as_str(), "gen-1");
    assert_eq!(dispatch.base_revision, 0);
}

#[test]
fn inconsistent_hand_built_plan_is_rejected_before_dispatch() {
    // Rule mismatch.
    let mut fake = FakeCompositor::new();
    let mut bad = fake.real_plan();
    bad.rule = tiler_core::directional::Rule::R1;
    let before = fake.reconciler.verified_revision();
    assert_eq!(
        {
            let obs = fake.observation(0);
            let corr = fake.correlation();
            let caps = fake.capabilities.clone();
            fake.reconciler.propose(&bad, &obs, &corr, &caps)
        },
        Err(ProposeError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
    assert_eq!(fake.reconciler.verified_revision(), before);
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
    assert!(!fake.reconciler.status().pending);

    // Capability mismatch.
    let mut fake = FakeCompositor::new();
    let mut bad = fake.real_plan();
    bad.required_capability = tiler_core::directional::Capability::WrapPerpendicular;
    assert_eq!(
        {
            let obs = fake.observation(0);
            let corr = fake.correlation();
            let caps = fake.capabilities.clone();
            fake.reconciler.propose(&bad, &obs, &corr, &caps)
        },
        Err(ProposeError::Diverged(DivergenceKind::CapabilityRefused))
    );
    assert_eq!(fake.reconciler.verified_revision(), 0);

    // Preconditions mismatch.
    let mut fake = FakeCompositor::new();
    let mut bad = fake.real_plan();
    bad.preconditions.pop();
    assert_eq!(
        {
            let obs = fake.observation(0);
            let corr = fake.correlation();
            let caps = fake.capabilities.clone();
            fake.reconciler.propose(&bad, &obs, &corr, &caps)
        },
        Err(ProposeError::Diverged(
            DivergenceKind::PostconditionMismatch
        ))
    );
    assert_eq!(fake.reconciler.verified_revision(), 0);
}

#[test]
fn mismatched_verified_operation_diverges_without_commit() {
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    fake.reconciler
        .acknowledge(&fake.ack(&correlation, 0, AckOutcome::Accepted))
        .expect("ack");
    let mut other = plan.operation.clone();
    // Mutate the attested operation so it no longer binds exactly.
    if let MoveOperation::SwapNeighbor {
        ref mut neighbor, ..
    } = other
    {
        *neighbor = NodeId::from("other-leaf");
    }
    assert_eq!(
        fake.reconciler.verify(&fake.post(
            &correlation,
            0,
            true,
            plan.preconditions.clone(),
            other
        )),
        Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
    );
    assert_eq!(fake.reconciler.status().state, StateKind::Divergent);
    assert_eq!(fake.reconciler.verified_revision(), 0);
}

#[test]
fn malformed_ack_and_verify_shapes_classify_typed() {
    // Malformed session strings cannot construct typed ids at the boundary.
    assert!(OwnerId::parse("bad owner").is_none());
    assert!(GenerationId::parse("GEN-1").is_none());
    assert!(CorrelationId::parse("bad corr").is_none());
    // Valid-but-mismatched ack owner diverges as owner mismatch.
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    let mut bad_ack = fake.ack(&correlation, 0, AckOutcome::Accepted);
    bad_ack.owner = OwnerId::parse("owner-2").expect("valid");
    assert_eq!(
        fake.reconciler.acknowledge(&bad_ack),
        Err(AckError::Diverged(DivergenceKind::OwnerMismatch))
    );

    // Valid-but-mismatched ack generation diverges as generation mismatch.
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    let mut bad_ack = fake.ack(&correlation, 0, AckOutcome::Accepted);
    bad_ack.generation = GenerationId::parse("gen-2").expect("valid");
    assert_eq!(
        fake.reconciler.acknowledge(&bad_ack),
        Err(AckError::Diverged(DivergenceKind::GenerationMismatch))
    );

    // Out-of-bounds ack revision shape diverges as stale revision.
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    let bad_ack = fake.ack(
        &correlation,
        tiler_core::contract::MAX_REVISION + 1,
        AckOutcome::Accepted,
    );
    assert_eq!(
        fake.reconciler.acknowledge(&bad_ack),
        Err(AckError::Diverged(DivergenceKind::StaleRevision))
    );

    // Mismatched verify owner diverges as owner mismatch.
    let mut fake = FakeCompositor::new();
    let plan = fake.real_plan();
    let correlation = fake.correlation();
    fake.reconciler
        .propose(
            &plan,
            &fake.observation(0),
            &correlation,
            &fake.capabilities,
        )
        .expect("propose");
    fake.reconciler
        .acknowledge(&fake.ack(&correlation, 0, AckOutcome::Accepted))
        .expect("ack");
    let mut bad_post = fake.post_for_plan(&correlation, 0, true, &plan);
    bad_post.observation.owner = OwnerId::parse("owner-2").expect("valid");
    assert_eq!(
        fake.reconciler.verify(&bad_post),
        Err(VerifyError::Diverged(DivergenceKind::OwnerMismatch))
    );
}
