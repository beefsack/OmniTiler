#![forbid(unsafe_code)]
/// START-frozen preview source binding: the exact mover token, source
/// output/workspace, and the core domain fingerprint captured at gesture
/// START (native MoveSizeStart drain). The preview never rebinds
/// mid-gesture: output/workspace/fingerprint drift fails
/// the whole gesture closed until settle. Source bounds stay at START.
///
/// The fingerprint covers the core logical ring plus the current view for
/// the source output: observation-stable (observation ticks never move
/// rings/views) but commit-sensitive (any selection/send/migration or
/// lifecycle change fails the fence), replacing the retired per-domain
/// Engine accepted revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreviewStartBinding {
    pub(super) token: String,
    pub(super) output: String,
    pub(super) workspace: String,
    pub(super) domain_fp: u64,
}

pub(super) struct PreviewBound {
    /// Carried sticky group-edge hover prior for the next sample and the
    /// final drop (exact 32/80 semantics live in the Engine resolver; this
    /// only carries the opaque value across samples).
    pub(super) hover_prior: Option<tiler_core::session::DragHoverPrior>,
}

/// Fresh per-sample preview facts against the START binding. Pure input for
/// [`preview_sample_gate`]; all native reads stay with the caller.
pub(super) struct PreviewFresh {
    /// This gesture already failed closed (drift/invalidation): never
    /// rebind, never sample again until settle.
    pub(super) dead: bool,
    /// Cursor still on the down/start point: no pointer journey yet.
    pub(super) zero: bool,
    /// Live HWND/PID/creation still matches the START member key.
    pub(super) identity_ok: bool,
    /// Live member-lifetime tag still matches the START tag.
    pub(super) tag_ok: bool,
    /// `member_tokens[START key]` still names the START token (no remap).
    pub(super) token_ok: bool,
    /// The START member still lives on the START output/workspace.
    pub(super) loc_ok: bool,
    /// Domain fingerprint still equals the START fingerprint.
    pub(super) revision_ok: bool,
    /// Sticky marker or Engine float exception on the mover.
    pub(super) float_hold: bool,
    /// Sample point inside the START source bounds (same-output only).
    pub(super) inside: bool,
}

/// One preview sample verdict: proceed to the Engine, hide transiently
/// (zero/outside: the gesture continues, the next moved sample may show),
/// or fail the whole gesture closed (drift/invalidation: no rebind, no
/// further samples, no stale render until settle clears).
pub(super) enum PreviewGate {
    Proceed,
    Transient(&'static str),
    Dead(&'static str),
}

/// Pure preview sample gate: invalidation dominates (fail-closed whole
/// gesture even with a static pointer), then zero/outside hide
/// transiently. Revision drift clears the carried prior via the `Dead`
/// path (the caller drops the bound, so nothing stale can render).
pub(super) fn preview_sample_gate(fresh: &PreviewFresh) -> PreviewGate {
    if fresh.dead {
        // Repeat drift can never rebind: a dead gesture stays dead.
        return PreviewGate::Dead("dead");
    }
    if !fresh.identity_ok || !fresh.tag_ok || !fresh.token_ok {
        return PreviewGate::Dead("identity-changed");
    }
    if fresh.float_hold {
        return PreviewGate::Dead("floating");
    }
    if !fresh.loc_ok {
        return PreviewGate::Dead("drift");
    }
    if !fresh.revision_ok {
        return PreviewGate::Dead("revision-drift");
    }
    if fresh.zero {
        return PreviewGate::Transient("zero");
    }
    if !fresh.inside {
        return PreviewGate::Transient("outside");
    }
    PreviewGate::Proceed
}

impl PreviewBound {
    /// Pure gate for one preview sample: real pointer movement with a live
    /// open gesture. Zero movement (cursor still on the down/start point)
    /// never shows; Esc-latched gestures never show.
    pub(super) fn sample_allowed(
        start_x: i32,
        start_y: i32,
        x: i32,
        y: i32,
        esc_latched: bool,
    ) -> bool {
        !esc_latched && (x != start_x || y != start_y)
    }
}

/// Closed bounded hover-prior trace descriptor: `none`, or the sticky
/// group-edge side (`group-edge:left`, ...). Never carries raw NodeIds,
/// tokens, or geometry: sides are a fixed vocabulary, safe for production
/// logs and sufficient to prove prior carry equality preview-to-drop.
pub(super) fn preview_prior_desc(prior: &Option<tiler_core::session::DragHoverPrior>) -> String {
    match prior {
        None => "none".to_owned(),
        Some(carried) => match &carried.prior {
            None => "none".to_owned(),
            Some(edge) => format!("group-edge:{}", edge.edge.as_str()),
        },
    }
}

/// Pure frame-grounded move gate: a native modal hold is a move iff the
/// live frame keeps the pre-gesture size in either lane (stable visible
/// DWM lane or GetWindowRect outer lane; shadow padding only shifts the
/// outer lane). A true resize moves both lanes. Review-accepted outcome:
/// keep both lanes, never the START hit test.
pub(super) fn preview_same_size(
    before_w: i32,
    before_h: i32,
    outer_w: i32,
    outer_h: i32,
    vis_w: i32,
    vis_h: i32,
) -> bool {
    (vis_w == before_w && vis_h == before_h) || (outer_w == before_w && outer_h == before_h)
}
/// Pure refusal-to-hide-reason mapping for Engine preview refusals:
/// self/centre/outside refusals show no rectangle; every other kind rides
/// through for attribution.
#[cfg(test)]
pub(super) fn preview_hide_for_refusal(kind: &str) -> &str {
    match kind {
        "unchanged" => "self",
        "unsupported-capability" => "centre",
        "cross-domain-mismatch" => "outside",
        _ => kind,
    }
}
