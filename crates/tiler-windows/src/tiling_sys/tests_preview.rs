#![forbid(unsafe_code)]
use super::preview::{
    PreviewBound, PreviewFresh, PreviewGate, preview_hide_for_refusal, preview_sample_gate,
};

#[test]
fn sample_gate_needs_real_unlatched_movement() {
    // Zero movement (title click, stationary-hold start point) never
    // shows; an Esc-latched hold never shows even when moved.
    assert!(!PreviewBound::sample_allowed(10, 10, 10, 10, false));
    assert!(!PreviewBound::sample_allowed(10, 10, 12, 10, true));
    assert!(!PreviewBound::sample_allowed(10, 10, 10, 10, true));
    assert!(PreviewBound::sample_allowed(10, 10, 11, 10, false));
    assert!(PreviewBound::sample_allowed(10, 10, 10, 11, false));
}

fn fresh_ok() -> PreviewFresh {
    PreviewFresh {
        dead: false,
        zero: false,
        identity_ok: true,
        tag_ok: true,
        token_ok: true,
        loc_ok: true,
        revision_ok: true,
        float_hold: false,
        inside: true,
    }
}

fn gate_outcome(fresh: &PreviewFresh) -> &'static str {
    match preview_sample_gate(fresh) {
        PreviewGate::Proceed => "proceed",
        PreviewGate::Transient(reason) => reason,
        PreviewGate::Dead(reason) => reason,
    }
}

#[test]
fn gate_proceeds_only_when_every_fence_holds() {
    assert_eq!(gate_outcome(&fresh_ok()), "proceed");
}

#[test]
fn gate_invalidation_dominates_a_static_pointer() {
    // Same pointer (zero) plus identity loss still fails closed: a
    // lingering rectangle must never survive mover death or reuse on a
    // stationary pointer.
    for fresh in [
        PreviewFresh {
            zero: true,
            identity_ok: false,
            ..fresh_ok()
        },
        PreviewFresh {
            zero: true,
            tag_ok: false,
            ..fresh_ok()
        },
        PreviewFresh {
            zero: true,
            token_ok: false,
            ..fresh_ok()
        },
    ] {
        assert_eq!(gate_outcome(&fresh), "identity-changed");
    }
    // Float/sticky capture fails closed too, even unmoved.
    assert_eq!(
        gate_outcome(&PreviewFresh {
            zero: true,
            float_hold: true,
            ..fresh_ok()
        }),
        "floating"
    );
}

#[test]
fn gate_drift_before_first_sample_fails_closed() {
    // Output/workspace drift and revision drift fail the whole gesture
    // from the very first sample (START binding is authoritative; the
    // refresh never rebinds to current membership).
    assert_eq!(
        gate_outcome(&PreviewFresh {
            loc_ok: false,
            ..fresh_ok()
        }),
        "drift"
    );
    assert_eq!(
        gate_outcome(&PreviewFresh {
            revision_ok: false,
            ..fresh_ok()
        }),
        "revision-drift"
    );
}

#[test]
fn gate_dead_repeat_can_never_rebind() {
    // A drifted gesture stays dead on repeat samples even when every
    // fresh fact reads healthy again.
    assert_eq!(
        gate_outcome(&PreviewFresh {
            dead: true,
            ..fresh_ok()
        }),
        "dead"
    );
}

#[test]
fn gate_zero_and_outside_hide_transiently() {
    // Zero movement and outside-work-area hide without killing the
    // gesture: the next moved sample may still show.
    assert_eq!(
        gate_outcome(&PreviewFresh {
            zero: true,
            ..fresh_ok()
        }),
        "zero"
    );
    assert_eq!(
        gate_outcome(&PreviewFresh {
            inside: false,
            ..fresh_ok()
        }),
        "outside"
    );
}

#[test]
fn frame_gate_matches_either_lane_and_rejects_resizes() {
    use super::preview_same_size;
    // Pure move: both lanes match (no shadows).
    assert!(preview_same_size(800, 600, 800, 600, 800, 600));
    // Shadow padding shifts only the outer lane: the visible lane
    // still matches, so a move is not misread as a resize.
    assert!(preview_same_size(800, 600, 816, 616, 800, 600));
    // Outer-fallback pre-gesture rect matches the outer lane.
    assert!(preview_same_size(816, 616, 816, 616, 800, 600));
    // Genuine resizes move both lanes: width-only, height-only, both,
    // and shrinkage all fail the move gate (preview stays hidden).
    assert!(!preview_same_size(800, 600, 900, 600, 884, 600));
    assert!(!preview_same_size(800, 600, 800, 700, 800, 684));
    assert!(!preview_same_size(800, 600, 900, 700, 884, 684));
    assert!(!preview_same_size(800, 600, 700, 500, 684, 484));
    // Degenerate live frames never match a real pre-gesture rect.
    assert!(!preview_same_size(800, 600, 0, 0, 0, 0));
}

#[test]
fn prior_descriptor_is_closed_without_raw_ids() {
    use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
    use tiler_core::session::{DomainKey, DragHoverPrior};
    // No prior at all, and a cleared (non-group-edge) hover, both
    // describe as `none`.
    assert_eq!(super::preview_prior_desc(&None), "none");
    let cleared = DragHoverPrior {
        domain: DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
        source_leaf: NodeId("leaf-w1".to_owned()),
        source_window: WindowId("w1".to_owned()),
        revision: 3,
        prior: None,
        prior_domain: DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
    };
    assert_eq!(super::preview_prior_desc(&Some(cleared)), "none");
    // A sticky group-edge carry describes only its side: the group
    // NodeId never reaches the log.
    let sticky = DragHoverPrior {
        domain: DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
        source_leaf: NodeId("leaf-w1".to_owned()),
        source_window: WindowId("w1".to_owned()),
        revision: 3,
        prior: Some(tiler_core::policy::PriorGroupEdge {
            group: NodeId("grp-secret".to_owned()),
            edge: tiler_core::contract::DragSide::Top,
        }),
        prior_domain: DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
    };
    let desc = super::preview_prior_desc(&Some(sticky));
    assert_eq!(desc, "group-edge:top");
    assert!(!desc.contains("grp-secret"));
}

#[test]
fn refusal_kinds_map_to_no_rectangle_hides() {
    // Self, centre-stack, and outside refusals show no rectangle; every
    // other kind rides through for attribution.
    assert_eq!(preview_hide_for_refusal("unchanged"), "self");
    assert_eq!(preview_hide_for_refusal("unsupported-capability"), "centre");
    assert_eq!(preview_hide_for_refusal("cross-domain-mismatch"), "outside");
    assert_eq!(
        preview_hide_for_refusal("domain-mismatch"),
        "domain-mismatch"
    );
}
