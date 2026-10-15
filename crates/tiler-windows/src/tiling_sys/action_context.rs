use super::*;

/// Stable per-action context: one opaque correlation for the whole select /
/// send / follow lifecycle, even when inner reconcile ticks increment. Times
/// are monotonic `Instant`s: `queued_at` is the cheap callback stamp from the
/// existing hook event (no logging/syscalls in the callback), `start` is the
/// dispatch instant on the loop thread.
pub(super) struct ActionCtx {
    pub(super) correlation: String,
    pub(super) tick: u64,
    pub(super) queued_at: Instant,
    pub(super) start: Instant,
}

/// Settled effect of one verified select transition: membership outcome
/// plus honest per-phase geometry and focus evidence. `geometry` is `None`
/// when no geometry pass ran (empty target, observation failure, early
/// refusal, non-plan Engine reply with readback not run); otherwise it
/// carries the verified verdict so `ok` never implies applied.
/// `transition_ms` covers dispatch to activation (hide+reveal+activate,
/// measured before the post-reveal observation); `observation_ms` covers the
/// post-reveal observation alone. `action_ms` on the log line is the terminal
/// dispatch-to-geometry-completion offset.
pub(super) struct SelectEffect {
    pub(super) outcome: &'static str,
    pub(super) geometry: Option<ApplySummary>,
    pub(super) focus: &'static str,
    pub(super) transition_ms: u64,
    pub(super) observation_ms: u64,
    pub(super) geometry_ms: u64,
    pub(super) hide_ms: u64,
    pub(super) reveal_ms: u64,
    pub(super) focus_ms: u64,
    pub(super) plan_ms: u64,
    pub(super) source_workspace: String,
    pub(super) target_workspace: String,
}

/// Settled effect of one send+follow action: membership outcome plus honest
/// source (pre-hide reflow) and target (post-reveal) geometry evidence.
/// `source`/`target` are `None` when that pass never ran. Plan and geometry
/// durations stay split per domain, never collapsed. Tokens are captured
/// before cleanup so a removed source still logs its opaque identity.
pub(super) struct SendEffect {
    pub(super) outcome: &'static str,
    pub(super) focus: &'static str,
    pub(super) source: Option<ApplySummary>,
    pub(super) target: Option<ApplySummary>,
    pub(super) transition_ms: u64,
    pub(super) observation_ms: u64,
    pub(super) source_geometry_ms: u64,
    pub(super) target_geometry_ms: u64,
    pub(super) hide_ms: u64,
    pub(super) reveal_ms: u64,
    pub(super) focus_ms: u64,
    pub(super) source_plan_ms: u64,
    pub(super) target_plan_ms: u64,
    pub(super) source_workspace: String,
    pub(super) target_workspace: String,
}

/// Grouped phase timings for one bounded action line: dispatch offsets plus
/// per-phase durations. Source/target plan and geometry stay distinct.
pub(super) struct ActionTimings {
    pub(super) transition_ms: u64,
    pub(super) observation_ms: u64,
    pub(super) source_plan_ms: u64,
    pub(super) target_plan_ms: u64,
    pub(super) source_geometry_ms: u64,
    pub(super) target_geometry_ms: u64,
    pub(super) hide_ms: u64,
    pub(super) reveal_ms: u64,
    pub(super) focus_ms: u64,
}

/// One bounded action summary: opaque correlation plus monotonic
/// queue/action/transition/geometry durations and per-phase
/// applied/mismatched/readback states. A phase that never ran serializes as
/// null (outcome carries empty vs deferred vs failed), never zeros implying
/// applied; a vetoed pass carries its bounded `veto` reason.
pub(super) struct ActionLine<'a> {
    pub(super) ctx: &'a ActionCtx,
    pub(super) op: &'a str,
    pub(super) index: u8,
    pub(super) edge: &'a str,
    pub(super) outcome: &'a str,
    pub(super) focus: &'a str,
    pub(super) timings: &'a ActionTimings,
    pub(super) source_workspace: &'a str,
    pub(super) target_workspace: &'a str,
    pub(super) source: Option<&'a ApplySummary>,
    pub(super) target: Option<&'a ApplySummary>,
}
