//! Serde-free near-layout fitting and seed ordering for session rebuilds.
//!
//! Portable typed boundary over adapter-normalized integer geometry: no
//! transport, JSON, or platform imports. [`EngineWindow`] is the smallest
//! coherent observation carrier (window/output/workspace identity plus the
//! carried frame rectangle and fit flags). Ordering, revision-adjacent seed
//! sequence identity (`fit-l{i}`/`fit-g0`), focus-last handling, and
//! outer-gap handling (via the already-inset [`OutputDomain::bounds`]) match
//! the planner protocol behavior exactly.

use crate::bounds::{rect_contained, valid_carried_rect};
use crate::contract::{
    AckOutcome, AdapterAck, LifecycleCapabilities, LifecyclePostObservation, Observation,
};
use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WindowLink, WorkspaceId};
use crate::geometry::{Rect, project};
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::session::{
    DesiredGeometry, DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, Session,
    SessionCommand, SessionObservation,
};

/// Serde-free observed window for seed ordering and recursive-cut fitting.
///
/// Carries the ephemeral client size hints (AR12) alongside the frame: seed
/// ordering and fitting ignore hints (topology derives from rectangles
/// only), while [`observed_window_from_engine`] propagates them so later
/// projection and clamp assessment see the same advisory input. Pre/post-image
/// matching ignores hints (identity is window/output/workspace/rect/flags).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineWindow {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub rect: Rect,
    pub floating: bool,
    pub fit_excluded: bool,
    /// Native fullscreen overlay (Q2 fixed-size admission, D5): born
    /// fullscreen bypasses the fixed classifier. Defaults false so
    /// existing carriers behave exactly as before.
    pub fullscreen: bool,
    /// Native maximized overlay (R-WS-12 migration gate): carried so the
    /// whole-workspace refusal detects maximized floats independently of
    /// the `fit_excluded` proxy. Defaults false so existing carriers
    /// behave exactly as before.
    pub maximized: bool,
    /// Native sticky state (Q2 fixed-size admission, D3/D6): sticky
    /// floats are intentional, never automatic. Defaults false.
    pub sticky: bool,
    /// Adapter-asserted automatic fixed-float origin (Q2, D6): the
    /// adapter classified this window automatic. Never inferred in core
    /// from hints alone. Defaults false.
    pub fixed_auto: bool,
    /// Adapter-asserted user tile win (Q2, D3): the adapter retains an
    /// explicit tile override for this live client across hide, domain
    /// release, and workspace re-adoption. Defaults false.
    pub fixed_suppress: bool,
    pub hints: crate::size_hints::WindowSizeHints,
}

/// Bounded near-layout fit decline reason for correlated logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitDeclineReason {
    SingleWindow,
    FitExcluded,
    InvalidGeometry,
    NoCut,
    ProjectionInvalid,
    /// Fit needing a centre split declines to the sequential seed.
    CentreSplit,
    /// Clean but minimum-infeasible fitted topology declines to the same seed.
    MinInfeasible,
}

impl FitDeclineReason {
    /// Stable token for this decline reason.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SingleWindow => "single_window",
            Self::FitExcluded => "fit_excluded",
            Self::InvalidGeometry => "invalid_geometry",
            Self::NoCut => "no_cut",
            Self::ProjectionInvalid => "projection_invalid",
            Self::CentreSplit => "centre_split",
            Self::MinInfeasible => "min_infeasible",
        }
    }
}

/// Fit one geometry-ordered recursive-cut tree, or decline to normal seeding.
/// Collect all viable cuts on an axis into an N-ary group, then recurse on the
/// orthogonal axis. Cuts allow at most `max(inner gap, 3% of domain extent)`
/// crossing per window; the finished tree projects with configured gaps.
pub fn try_recursive_cut_fit(
    domain: &OutputDomain,
    windows: &[EngineWindow],
) -> Result<(Node, Vec<WindowLink>), FitDeclineReason> {
    try_recursive_cut_fit_with_centre_count(domain, windows).map(|(tree, links, _)| (tree, links))
}

/// Fit plus the committed centre-split count (0 for clean fits).
pub fn try_recursive_cut_fit_with_centre_count(
    domain: &OutputDomain,
    windows: &[EngineWindow],
) -> Result<(Node, Vec<WindowLink>, usize), FitDeclineReason> {
    if windows.len() < 2 {
        return Err(FitDeclineReason::SingleWindow);
    }
    if windows.iter().any(|w| w.floating || w.fit_excluded) {
        return Err(FitDeclineReason::FitExcluded);
    }
    let mut items: Vec<(WindowId, Rect)> = Vec::with_capacity(windows.len());
    for entry in windows {
        let rect = entry.rect;
        if !valid_carried_rect(rect.x, rect.y, rect.w, rect.h) {
            return Err(FitDeclineReason::InvalidGeometry);
        }
        if !rect_contained(rect, domain.bounds) {
            return Err(FitDeclineReason::InvalidGeometry);
        }
        items.push((entry.window.clone(), rect));
    }
    fn tolerance(domain: &OutputDomain, axis: Axis) -> i64 {
        let gap = i64::from(domain.gap).max(0);
        let extent = match axis {
            Axis::Horizontal => i64::from(domain.bounds.w),
            Axis::Vertical => i64::from(domain.bounds.h),
        }
        .max(0);
        gap.max(extent * 3 / 100)
    }
    fn primary_start(rect: Rect, axis: Axis) -> i64 {
        match axis {
            Axis::Horizontal => i64::from(rect.x),
            Axis::Vertical => i64::from(rect.y),
        }
    }
    fn primary_end(rect: Rect, axis: Axis) -> i64 {
        match axis {
            Axis::Horizontal => i64::from(rect.x) + i64::from(rect.w),
            Axis::Vertical => i64::from(rect.y) + i64::from(rect.h),
        }
    }
    fn primary_span(rect: Rect, axis: Axis) -> i64 {
        match axis {
            Axis::Horizontal => i64::from(rect.w),
            Axis::Vertical => i64::from(rect.h),
        }
    }
    fn sort_for_axis(items: &mut [(WindowId, Rect)], axis: Axis) {
        match axis {
            Axis::Horizontal => items.sort_by_key(|a| (a.1.x, a.1.y, a.1.w, a.1.h)),
            Axis::Vertical => items.sort_by_key(|a| (a.1.y, a.1.x, a.1.h, a.1.w)),
        }
    }
    fn orthogonal(axis: Axis) -> Axis {
        match axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }
    struct Alloc {
        next_leaf: usize,
        next_group: usize,
        centre_splits: usize,
    }
    fn valid_cuts(members: &[(WindowId, Rect)], axis: Axis, tol: i64) -> Vec<usize> {
        let mut ordered: Vec<(WindowId, Rect)> = members.to_vec();
        sort_for_axis(&mut ordered, axis);
        let mut prefix = i64::MIN;
        let mut prefix_max = Vec::with_capacity(ordered.len());
        for (_, rect) in &ordered {
            prefix = prefix.max(primary_end(*rect, axis));
            prefix_max.push(prefix);
        }
        let mut floor = i64::MAX;
        let mut suffix_min = vec![0; ordered.len()];
        for (index, (_, rect)) in ordered.iter().enumerate().rev() {
            floor = floor.min(primary_start(*rect, axis));
            suffix_min[index] = floor;
        }
        (0..ordered.len().saturating_sub(1))
            .filter(|i| prefix_max[*i] - suffix_min[*i + 1] <= 2 * tol)
            .collect()
    }
    fn centre_gap(members: &[(WindowId, Rect)], axis: Axis) -> (i64, usize) {
        let mut keys: Vec<(i64, i64, i64)> = members
            .iter()
            .map(|(_, r)| {
                let s = primary_start(*r, axis);
                let span = primary_span(*r, axis);
                (2 * s + span, s, span)
            })
            .collect();
        keys.sort();
        let mut best = (0, 0);
        for i in 0..keys.len().saturating_sub(1) {
            let gap = keys[i + 1].0 - keys[i].0;
            if gap > best.0 {
                best = (gap, i);
            }
        }
        best
    }
    fn max_span(members: &[(WindowId, Rect)], axis: Axis) -> i64 {
        members
            .iter()
            .map(|(_, r)| primary_span(*r, axis))
            .max()
            .unwrap_or(0)
    }
    fn build_clean(
        domain: &OutputDomain,
        members: &[(WindowId, Rect)],
        axis: Axis,
        cuts: &[usize],
        alloc: &mut Alloc,
    ) -> Option<(Node, Vec<WindowLink>)> {
        let mut ordered: Vec<(WindowId, Rect)> = members.to_vec();
        sort_for_axis(&mut ordered, axis);
        let group = NodeId(format!("fit-g{}", alloc.next_group));
        alloc.next_group += 1;
        let mut boundaries: Vec<usize> = vec![0];
        boundaries.extend(cuts.iter().map(|c| c + 1));
        boundaries.push(ordered.len());
        let mut children = Vec::with_capacity(boundaries.len() - 1);
        let mut shares = Vec::with_capacity(boundaries.len() - 1);
        let mut links = Vec::with_capacity(ordered.len());
        for pair in boundaries.windows(2) {
            let part = &ordered[pair[0]..pair[1]];
            let mut start = i64::MAX;
            let mut end = i64::MIN;
            for (_, rect) in part {
                start = start.min(primary_start(*rect, axis));
                end = end.max(primary_end(*rect, axis));
            }
            let share = u64::try_from(end - start).ok().filter(|s| *s > 0)?;
            shares.push(share);
            if part.len() == 1 {
                let leaf = NodeId(format!("fit-l{}", alloc.next_leaf));
                alloc.next_leaf += 1;
                links.push(WindowLink {
                    window: part[0].0.clone(),
                    leaf: leaf.clone(),
                    output: domain.id.clone(),
                    workspace: domain.workspace.clone(),
                });
                children.push(Node::Leaf { id: leaf });
            } else {
                let (child, child_links) = build_region(domain, part, orthogonal(axis), alloc)?;
                links.extend(child_links);
                children.push(child);
            }
        }
        Some((
            Node::Group {
                id: group,
                axis,
                children,
                shares,
            },
            links,
        ))
    }
    fn build_region(
        domain: &OutputDomain,
        members: &[(WindowId, Rect)],
        axis: Axis,
        alloc: &mut Alloc,
    ) -> Option<(Node, Vec<WindowLink>)> {
        if members.len() < 2 {
            return None;
        }
        let cuts = valid_cuts(members, axis, tolerance(domain, axis));
        if !cuts.is_empty() {
            return build_clean(domain, members, axis, &cuts, alloc);
        }
        let other = orthogonal(axis);
        let other_cuts = valid_cuts(members, other, tolerance(domain, other));
        if !other_cuts.is_empty() {
            return build_clean(domain, members, other, &other_cuts, alloc);
        }
        let (h_gap, h_at) = centre_gap(members, Axis::Horizontal);
        let (v_gap, v_at) = centre_gap(members, Axis::Vertical);
        if h_gap <= 0 && v_gap <= 0 {
            return None;
        }
        let (split_axis, at) = if h_gap >= v_gap {
            (Axis::Horizontal, h_at)
        } else {
            (Axis::Vertical, v_at)
        };
        let mut ordered: Vec<(WindowId, Rect)> = members.to_vec();
        match split_axis {
            Axis::Horizontal => {
                ordered.sort_by_key(|a| (2 * i64::from(a.1.x) + i64::from(a.1.w), a.1.x, a.1.y))
            }
            Axis::Vertical => {
                ordered.sort_by_key(|a| (2 * i64::from(a.1.y) + i64::from(a.1.h), a.1.y, a.1.x))
            }
        }
        let (left, right) = ordered.split_at(at + 1);
        let shares = [max_span(left, split_axis), max_span(right, split_axis)];
        if shares.iter().any(|s| *s <= 0) {
            return None;
        }
        let group = NodeId(format!("fit-g{}", alloc.next_group));
        alloc.next_group += 1;
        alloc.centre_splits += 1;
        let mut links = Vec::with_capacity(ordered.len());
        let mut children = Vec::with_capacity(2);
        for part in [left, right] {
            if part.len() == 1 {
                let leaf = NodeId(format!("fit-l{}", alloc.next_leaf));
                alloc.next_leaf += 1;
                links.push(WindowLink {
                    window: part[0].0.clone(),
                    leaf: leaf.clone(),
                    output: domain.id.clone(),
                    workspace: domain.workspace.clone(),
                });
                children.push(Node::Leaf { id: leaf });
            } else {
                let (child, child_links) =
                    build_region(domain, part, orthogonal(split_axis), alloc)?;
                links.extend(child_links);
                children.push(child);
            }
        }
        Some((
            Node::Group {
                id: group,
                axis: split_axis,
                children,
                shares: shares.iter().map(|s| *s as u64).collect(),
            },
            links,
        ))
    }
    let mut alloc = Alloc {
        next_leaf: 0,
        next_group: 0,
        centre_splits: 0,
    };
    match build_region(domain, &items, Axis::Horizontal, &mut alloc) {
        Some((tree, links)) => {
            let projected = project(&tree, domain.bounds, domain.gap)
                .ok()
                .ok_or(FitDeclineReason::ProjectionInvalid)?;
            if projected.len() != items.len() {
                return Err(FitDeclineReason::ProjectionInvalid);
            }
            Ok((tree, links, alloc.centre_splits))
        }
        None => Err(FitDeclineReason::NoCut),
    }
}

/// Admission seed order: spatial `(y, x, h, w)` sort with the focused window
/// last.
///
/// Stable sorting preserves the adapter's observation order when carried
/// rectangles tie during admission. Admission assigns new geometry, so an
/// uninformative incoming rectangle must not reject the other members. When
/// `allow_tied_observations` is false (non-admission rebuilds reconstructing
/// existing tiled state), equal frame rectangles on non-focused windows lack
/// a safe topology signal and fail closed with `None`.
#[must_use]
pub fn order_spatial_with_focus_last(
    mut windows: Vec<EngineWindow>,
    focused: &WindowId,
    allow_tied_observations: bool,
) -> Option<Vec<EngineWindow>> {
    if !allow_tied_observations {
        // Non-admission rebuilds reconstruct existing tiled state, so equal
        // frame rectangles still lack a safe topology signal.
        for (index, left) in windows.iter().enumerate() {
            if &left.window == focused {
                continue;
            }
            if windows[index + 1..].iter().any(|right| {
                &right.window != focused
                    && left.rect.x == right.rect.x
                    && left.rect.y == right.rect.y
                    && left.rect.w == right.rect.w
                    && left.rect.h == right.rect.h
            }) {
                return None;
            }
        }
    }
    // Stable sorting preserves the adapter's observation order when carried
    // rectangles tie during admission. Admission assigns new geometry, so an
    // uninformative incoming rectangle must not reject the other members.
    windows.sort_by(
        |left, right| match (&left.window == focused, &right.window == focused) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => (left.rect.y, left.rect.x, left.rect.h, left.rect.w).cmp(&(
                right.rect.y,
                right.rect.x,
                right.rect.h,
                right.rect.w,
            )),
        },
    );
    Some(windows)
}

/// Rebuild placement target for one admission step: the focused leaf's
/// projected rectangle, or the output geometry when the rebuilt domain is
/// still empty (or focus does not resolve there).
#[must_use]
pub fn seed_target_bounds(session: &Session, domain: &OutputDomain) -> Rect {
    let key = DomainKey {
        output: domain.id.clone(),
        workspace: domain.workspace.clone(),
    };
    let (focus_domain, focus_leaf) = session.focus();
    let eligible = focus_leaf
        .as_ref()
        .filter(|_| focus_domain.as_ref() == Some(&key));
    let tree = session
        .snapshot()
        .domains
        .into_iter()
        .find(|d| d.output == key.output && d.workspace == key.workspace)
        .and_then(|d| d.tree);
    crate::session::admission_placement_for(domain, tree.as_ref(), eligible)
}

/// Portable observed-window mapping: tiled seed observations carry no
/// exception flags beyond the carried floating/fullscreen/maximized/sticky
/// bits. Hints propagate unchanged (advisory only; never identity).
#[must_use]
pub fn observed_window_from_engine(entry: &EngineWindow) -> ObservedWindow {
    ObservedWindow {
        window: entry.window.clone(),
        output: entry.output.clone(),
        workspace: entry.workspace.clone(),
        floating: entry.floating,
        fullscreen: entry.fullscreen,
        maximized: entry.maximized,
        sticky: entry.sticky,
        fixed_auto: entry.fixed_auto,
        fixed_suppress: entry.fixed_suppress,
        hints: entry.hints,
    }
}

/// Portable session observation over one complete carried window set.
#[must_use]
pub fn session_observation_for(
    owner: &OwnerId,
    generation: &GenerationId,
    base: u64,
    fingerprint: u64,
    windows: &[EngineWindow],
) -> SessionObservation {
    SessionObservation {
        observation: Observation::new(owner.clone(), generation.clone(), base, fingerprint),
        windows: windows.iter().map(observed_window_from_engine).collect(),
    }
}

/// Portable two-domain workspace observation (source first, then target).
#[must_use]
pub fn workspace_observation_for(
    owner: &OwnerId,
    generation: &GenerationId,
    base: u64,
    fingerprint: u64,
    source: &[EngineWindow],
    target: &[EngineWindow],
) -> SessionObservation {
    let mut windows: Vec<ObservedWindow> = Vec::with_capacity(source.len() + target.len());
    windows.extend(source.iter().map(observed_window_from_engine));
    windows.extend(target.iter().map(observed_window_from_engine));
    SessionObservation {
        observation: Observation::new(owner.clone(), generation.clone(), base, fingerprint),
        windows,
    }
}

/// Portable post-observation match: every desired window must be carried
/// exactly once (source plus target) with the expected output, workspace,
/// and rectangle.
#[must_use]
pub fn workspace_post_matches(
    desired_geometry: &[DesiredGeometry],
    source: &[EngineWindow],
    target: &[EngineWindow],
) -> bool {
    let mut observed: std::collections::HashMap<&str, &EngineWindow> =
        std::collections::HashMap::with_capacity(source.len() + target.len());
    for entry in source.iter().chain(target.iter()) {
        if observed.insert(entry.window.0.as_str(), entry).is_some() {
            return false;
        }
    }
    if observed.len() != desired_geometry.len() {
        return false;
    }
    for desired in desired_geometry {
        let Some(entry) = observed.get(desired.window.0.as_str()) else {
            return false;
        };
        if entry.output != desired.output
            || entry.workspace != desired.workspace
            || entry.rect != desired.rect
        {
            return false;
        }
    }
    true
}

/// One shared seed admission step: propose/ack/verify one window into its
/// domain through the retained session lifecycle path.
///
/// The retained tiled links always synthesize `floating: false` with no hints
/// (unknown here, advisory only) plus the retained exceptions. The newly
/// admitted window (`admitted`) and the pre/post fingerprints are caller
/// inputs so each caller keeps its own semantics: tiled seeds synthesize the
/// admitted window with no hints and verify with post `base`, while workspace
/// entries preserve floating/hints via [`observed_window_from_engine`] and
/// verify with the event fingerprint. Correlation identity (`seed-{index:04}`)
/// and propose/ack/verify order match the retained lifecycle path exactly.
#[allow(clippy::too_many_arguments)]
fn seed_admit_step(
    session: &mut Session,
    owner: &OwnerId,
    generation: &GenerationId,
    domain: &OutputDomain,
    admitted: ObservedWindow,
    index: usize,
    pre_fingerprint: u64,
    post_fingerprint: u64,
) -> Option<()> {
    let base = session.accepted_revision();
    let mut observed: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| ObservedWindow {
            window: l.window.clone(),
            output: l.output.clone(),
            workspace: l.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            // Seed rebuilds synthesize observations from retained links;
            // hints are unknown here, so none (advisory only).
            hints: crate::size_hints::WindowSizeHints::none(),
        })
        .collect();
    observed.extend(session.exception_observed());
    let command = SessionCommand::Admit {
        window: admitted.window.clone(),
        output: admitted.output.clone(),
        workspace: admitted.workspace.clone(),
        exceptions: ExceptionFlags::none(),
        exception_behavior: None,
        placement_bounds: seed_target_bounds(session, domain),
        suppress_fixed_float: false,
    };
    observed.push(admitted);
    let correlation = CorrelationId::parse(&format!("seed-{index:04}"))?;
    let observation = SessionObservation {
        observation: Observation::new(owner.clone(), generation.clone(), base, pre_fingerprint),
        windows: observed,
    };
    let plan = session
        .propose(
            &command,
            &observation,
            &correlation,
            &LifecycleCapabilities::full(),
        )
        .ok()?;
    let ack = AdapterAck::new(
        correlation.clone(),
        owner.clone(),
        generation.clone(),
        base,
        AckOutcome::Accepted,
    );
    session.acknowledge(&ack).ok()?;
    session
        .verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner.clone(), generation.clone(), base, post_fingerprint),
            correlation,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .ok()?;
    Some(())
}

/// Rebuild ephemeral authoritative topology from the normalized observation.
///
/// Admits the observed spatial order, with the focused window last, through
/// the retained session lifecycle path only. The split axis derives from the
/// rebuild target at each step (see [`seed_target_bounds`]); opaque window
/// ids never determine topology. Correlations are `seed-{index:04}`,
/// revisions advance one commit per member, and ack/verify follow the
/// retained lifecycle path exactly.
///
/// `seed_session` seeds from the zero counter for standalone use; the
/// Engine seeds through [`seed_session_from_counter`] with its
/// Engine-wide floor so independently seeded domains never collide once
/// their windows share a domain.
#[must_use]
pub fn seed_session(
    owner: &OwnerId,
    generation: &GenerationId,
    fingerprint: u64,
    domain: &OutputDomain,
    seed_order: &[EngineWindow],
) -> Option<Session> {
    seed_session_from_counter(owner, generation, fingerprint, domain, seed_order, 0)
}

/// [`seed_session`] starting the first-successful-tiled-admission
/// sequence at `admission_floor` instead of zero. The Engine passes its
/// Engine-wide floor so this fresh domain's serial legs order after every
/// retained domain's sequences; leg order within the roster is unchanged.
#[must_use]
pub fn seed_session_from_counter(
    owner: &OwnerId,
    generation: &GenerationId,
    fingerprint: u64,
    domain: &OutputDomain,
    seed_order: &[EngineWindow],
    admission_floor: u64,
) -> Option<Session> {
    let mut session = Session::new(
        owner.clone(),
        generation.clone(),
        0,
        fingerprint,
        vec![domain.clone()],
    )
    .ok()?;
    session.sync_admission_counter(admission_floor);
    for (index, entry) in seed_order.iter().enumerate() {
        let base = session.accepted_revision();
        // Tiled seed synthesizes the admitted observation with no hints.
        let admitted = ObservedWindow {
            window: entry.window.clone(),
            output: entry.output.clone(),
            workspace: entry.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        };
        // Post fingerprint is the pre-step base, not the event fingerprint.
        seed_admit_step(
            &mut session,
            owner,
            generation,
            domain,
            admitted,
            index,
            fingerprint,
            base,
        )?;
    }
    Some(session)
}

/// One admission step of the two-domain workspace seed: propose/ack/verify
/// one tiled window into its exact source or target domain.
pub fn seed_workspace_admit(
    session: &mut Session,
    owner: &OwnerId,
    generation: &GenerationId,
    fingerprint: u64,
    domain: &OutputDomain,
    entry: &EngineWindow,
    index: usize,
) -> Option<()> {
    // Workspace entries preserve floating/hints; post fingerprint is the
    // event fingerprint.
    seed_admit_step(
        session,
        owner,
        generation,
        domain,
        observed_window_from_engine(entry),
        index,
        fingerprint,
        fingerprint,
    )
}

/// Rebuild the authoritative two-domain workspace topology from the observed
/// source and target spatial orders (source first, then target). Mirrors
/// [`seed_session`]; the mover is admitted into its source domain and focus is
/// synced to it by the caller before the workspace move proposes.
#[must_use]
pub fn seed_workspace_session(
    owner: &OwnerId,
    generation: &GenerationId,
    fingerprint: u64,
    source_domain: &OutputDomain,
    target_domain: &OutputDomain,
    source_order: &[EngineWindow],
    target_order: &[EngineWindow],
) -> Option<Session> {
    let mut session = Session::new(
        owner.clone(),
        generation.clone(),
        0,
        fingerprint,
        vec![source_domain.clone(), target_domain.clone()],
    )
    .ok()?;
    for (index, entry) in source_order.iter().enumerate() {
        seed_workspace_admit(
            &mut session,
            owner,
            generation,
            fingerprint,
            source_domain,
            entry,
            index,
        )?;
    }
    for (index, entry) in target_order.iter().enumerate() {
        seed_workspace_admit(
            &mut session,
            owner,
            generation,
            fingerprint,
            target_domain,
            entry,
            source_order.len() + index,
        )?;
    }
    Some(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn window(id: &str, x: i32, y: i32, w: i32, h: i32) -> EngineWindow {
        EngineWindow {
            window: WindowId(id.to_owned()),
            output: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            rect: Rect { x, y, w, h },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }
    }

    fn domain() -> OutputDomain {
        OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 300,
                h: 100,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        }
    }

    fn square_domain() -> OutputDomain {
        OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 300,
                h: 300,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        }
    }

    #[test]
    fn flat_nary_horizontal_orders_by_geometry() {
        let domain = domain();
        let windows = vec![window("b", 100, 0, 100, 100), window("a", 0, 0, 100, 100)];
        let (tree, links) =
            try_recursive_cut_fit(&domain, &windows).expect("horizontal strip fits");
        match &tree {
            Node::Group { axis, shares, .. } => {
                assert_eq!(*axis, Axis::Horizontal);
                assert_eq!(*shares, vec![100, 100]);
            }
            Node::Leaf { .. } => panic!("expected group"),
        }
        assert_eq!(links[0].window.0, "a");
        assert_eq!(links[1].window.0, "b");
    }

    #[test]
    fn flat_nary_vertical_orders_by_geometry() {
        let domain = domain();
        let windows = vec![
            window("b", 0, 60, 300, 40),
            window("a", 0, 0, 300, 30),
            window("c", 0, 30, 300, 30),
        ];
        let (tree, links) = try_recursive_cut_fit(&domain, &windows).expect("vertical strip fits");
        match &tree {
            Node::Group { axis, shares, .. } => {
                assert_eq!(*axis, Axis::Vertical);
                assert_eq!(*shares, vec![30, 30, 40]);
            }
            Node::Leaf { .. } => panic!("expected group"),
        }
        let ids: Vec<&str> = links.iter().map(|l| l.window.0.as_str()).collect();
        assert_eq!(ids, vec!["a", "c", "b"]);
    }

    #[test]
    fn resized_shares_follow_observed_spans() {
        let domain = domain();
        let windows = vec![window("b", 60, 0, 240, 100), window("a", 0, 0, 60, 100)];
        let (tree, _) = try_recursive_cut_fit(&domain, &windows).expect("resized strip fits");
        match &tree {
            Node::Group { shares, .. } => assert_eq!(*shares, vec![60, 240]),
            Node::Leaf { .. } => panic!("expected group"),
        }
    }

    #[test]
    fn nested_t_is_focus_independent_geometry_order() {
        let domain = square_domain();
        let left = window("left", 0, 0, 100, 300);
        let top = window("top", 100, 0, 200, 150);
        let bottom = window("bottom", 100, 150, 200, 150);
        let orders = vec![
            vec![left.clone(), top.clone(), bottom.clone()],
            vec![bottom.clone(), top.clone(), left.clone()],
            vec![top.clone(), left.clone(), bottom.clone()],
        ];
        let mut results = Vec::new();
        for order in &orders {
            let (tree, links) = try_recursive_cut_fit(&domain, order).expect("nested T fits");
            match &tree {
                Node::Group {
                    axis,
                    shares,
                    children,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Horizontal);
                    assert_eq!(*shares, vec![100, 200]);
                    assert_eq!(children.len(), 2);
                    match &children[1] {
                        Node::Group {
                            axis,
                            shares,
                            children,
                            ..
                        } => {
                            assert_eq!(*axis, Axis::Vertical);
                            assert_eq!(*shares, vec![150, 150]);
                            assert_eq!(children.len(), 2);
                        }
                        Node::Leaf { .. } => panic!("expected nested group"),
                    }
                }
                Node::Leaf { .. } => panic!("expected group"),
            }
            let order: Vec<&str> = links.iter().map(|l| l.window.0.as_str()).collect();
            assert_eq!(order, vec!["left", "top", "bottom"]);
            results.push((tree, links));
        }
        assert_eq!(results[0], results[1]);
        assert_eq!(results[0], results[2]);
    }

    #[test]
    fn small_overlap_within_tolerance_fits() {
        // With tolerance 9, a 16px overlap admits a midpoint cut that each
        // window crosses by 8px.
        let domain = domain();
        let windows = vec![window("a", 0, 0, 100, 100), window("b", 84, 0, 100, 100)];
        let (tree, links) = try_recursive_cut_fit(&domain, &windows).expect("small overlap fits");
        match &tree {
            Node::Group { axis, .. } => assert_eq!(*axis, Axis::Horizontal),
            Node::Leaf { .. } => panic!("expected group"),
        }
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn overlap_beyond_tolerance_centre_splits() {
        let domain = domain();
        let windows = vec![window("a", 0, 0, 100, 100), window("b", 80, 0, 100, 100)];
        let (tree, links, splits) =
            try_recursive_cut_fit_with_centre_count(&domain, &windows).expect("centre splits");
        match &tree {
            Node::Group { axis, shares, .. } => {
                assert_eq!(*axis, Axis::Horizontal);
                assert_eq!(*shares, vec![100, 100]);
            }
            Node::Leaf { .. } => panic!("expected group"),
        }
        let ids: Vec<&str> = links.iter().map(|l| l.window.0.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
        assert_eq!(splits, 1);
    }

    #[test]
    fn vertical_overlap_beyond_tolerance_centre_splits() {
        let domain = square_domain();
        let windows = vec![window("a", 0, 0, 100, 100), window("b", 0, 80, 100, 100)];
        for input in [windows.clone(), windows.into_iter().rev().collect()] {
            let (tree, links, splits) =
                try_recursive_cut_fit_with_centre_count(&domain, &input).expect("centre splits");
            assert_eq!(splits, 1);
            let projected = project(&tree, domain.bounds, domain.gap).expect("projectable");
            assert_eq!(projected.len(), 2);
            let geometry: Vec<_> = links
                .iter()
                .map(|link| {
                    let rect = projected
                        .iter()
                        .find(|p| p.leaf == link.leaf)
                        .expect("leaf")
                        .rect;
                    (link.window.0.as_str(), rect)
                })
                .collect();
            assert_eq!(
                geometry,
                vec![
                    (
                        "a",
                        Rect {
                            x: 0,
                            y: 0,
                            w: 300,
                            h: 150
                        }
                    ),
                    (
                        "b",
                        Rect {
                            x: 0,
                            y: 150,
                            w: 300,
                            h: 150
                        }
                    ),
                ]
            );
        }
    }

    #[test]
    fn configured_gap_can_allow_a_wider_cross_cut_overlap() {
        let windows = vec![window("a", 0, 0, 100, 100), window("b", 70, 0, 100, 100)];
        let (_, _, splits) =
            try_recursive_cut_fit_with_centre_count(&domain(), &windows).expect("centre splits");
        assert_eq!(splits, 1);
        let mut with_gap = domain();
        with_gap.gap = 20;
        let (_, _, gap_splits) =
            try_recursive_cut_fit_with_centre_count(&with_gap, &windows).expect("gap fits");
        assert_eq!(gap_splits, 0);
    }

    #[test]
    fn pinwheel_without_guillotine_cut_centre_splits() {
        let domain = OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        };
        let windows = vec![
            window("a", 0, 0, 60, 40),
            window("b", 60, 0, 40, 60),
            window("c", 40, 60, 60, 40),
            window("d", 0, 40, 40, 60),
        ];
        let (tree, links, splits) =
            try_recursive_cut_fit_with_centre_count(&domain, &windows).expect("centre splits");
        assert!(splits > 0);
        assert_eq!(links.len(), 4);
        match &tree {
            Node::Group { children, .. } => assert_eq!(children.len(), 2),
            Node::Leaf { .. } => panic!("expected group"),
        }
    }

    #[test]
    fn cascading_overlap_preserves_left_top_order() {
        let domain = domain();
        let windows = vec![
            window("a", 0, 0, 100, 100),
            window("b", 50, 10, 100, 80),
            window("c", 100, 20, 100, 60),
            window("d", 150, 30, 100, 50),
        ];
        let (tree, links, splits) =
            try_recursive_cut_fit_with_centre_count(&domain, &windows).expect("cascade fits");
        assert!(splits > 0);
        let ids: Vec<&str> = links.iter().map(|l| l.window.0.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c", "d"]);
        match &tree {
            Node::Group { axis, .. } => assert_eq!(*axis, Axis::Horizontal),
            Node::Leaf { .. } => panic!("expected group"),
        }
    }

    #[test]
    fn big_vs_small_overlap_yields_proportional_shares() {
        let domain = domain();
        let windows = vec![
            window("big", 0, 0, 200, 100),
            window("small", 150, 0, 60, 100),
        ];
        let (tree, links, splits) =
            try_recursive_cut_fit_with_centre_count(&domain, &windows).expect("sizes fit");
        assert_eq!(splits, 1);
        match &tree {
            Node::Group { shares, .. } => assert_eq!(*shares, vec![200, 60]),
            Node::Leaf { .. } => panic!("expected group"),
        }
        let ids: Vec<&str> = links.iter().map(|l| l.window.0.as_str()).collect();
        assert_eq!(ids, vec!["big", "small"]);
    }

    #[test]
    fn clean_fit_is_identical_with_zero_splits() {
        let domain = domain();
        let windows = vec![window("b", 100, 0, 100, 100), window("a", 0, 0, 100, 100)];
        let plain = try_recursive_cut_fit(&domain, &windows).expect("clean fits");
        let (tree, links, splits) =
            try_recursive_cut_fit_with_centre_count(&domain, &windows).expect("clean fits");
        assert_eq!((tree, links), plain);
        assert_eq!(splits, 0);
    }

    #[test]
    fn identical_centres_fallback() {
        let domain = domain();
        let windows = vec![window("a", 0, 0, 100, 100), window("b", 0, 0, 100, 100)];
        assert_eq!(
            try_recursive_cut_fit(&domain, &windows),
            Err(FitDeclineReason::NoCut)
        );
    }

    #[test]
    fn decline_reasons_use_stable_tokens() {
        let domain = domain();
        assert_eq!(
            try_recursive_cut_fit(&domain, &[]),
            Err(FitDeclineReason::SingleWindow)
        );
        assert_eq!(FitDeclineReason::SingleWindow.as_str(), "single_window");
        let mut excluded = window("a", 0, 0, 100, 100);
        excluded.fit_excluded = true;
        assert_eq!(
            try_recursive_cut_fit(&domain, &[excluded, window("b", 100, 0, 100, 100)]),
            Err(FitDeclineReason::FitExcluded)
        );
        assert_eq!(FitDeclineReason::FitExcluded.as_str(), "fit_excluded");
        assert_eq!(
            try_recursive_cut_fit(
                &domain,
                &[window("a", 0, 0, 100, 100), window("b", 500, 0, 100, 100)]
            ),
            Err(FitDeclineReason::InvalidGeometry)
        );
        assert_eq!(
            FitDeclineReason::InvalidGeometry.as_str(),
            "invalid_geometry"
        );
        assert_eq!(FitDeclineReason::NoCut.as_str(), "no_cut");
        assert_eq!(FitDeclineReason::CentreSplit.as_str(), "centre_split");
        assert_eq!(FitDeclineReason::MinInfeasible.as_str(), "min_infeasible");
        let tight = OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            gap: 6,
            adjacent: BTreeMap::new(),
        };
        let crowded = vec![
            window("a", 0, 0, 3, 10),
            window("b", 3, 0, 4, 10),
            window("c", 7, 0, 3, 10),
        ];
        assert_eq!(
            try_recursive_cut_fit(&tight, &crowded),
            Err(FitDeclineReason::ProjectionInvalid)
        );
        assert_eq!(
            FitDeclineReason::ProjectionInvalid.as_str(),
            "projection_invalid"
        );
    }

    #[test]
    fn focus_sorts_last_with_spatial_order() {
        let focused = WindowId("focus".to_owned());
        let windows = vec![
            window("focus", 0, 0, 10, 10),
            window("b", 50, 0, 10, 10),
            window("a", 20, 0, 10, 10),
        ];
        let ordered =
            order_spatial_with_focus_last(windows, &focused, true).expect("admission orders");
        let ids: Vec<&str> = ordered.iter().map(|w| w.window.0.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "focus"]);
    }

    #[test]
    fn tied_non_focused_rects_fail_closed_without_admission() {
        let focused = WindowId("focus".to_owned());
        let windows = vec![
            window("a", 0, 0, 10, 10),
            window("b", 0, 0, 10, 10),
            window("focus", 50, 0, 10, 10),
        ];
        assert!(order_spatial_with_focus_last(windows, &focused, false).is_none());
    }

    fn ids() -> (OwnerId, GenerationId) {
        (
            OwnerId::parse("owner-a").expect("valid"),
            GenerationId::parse("gen-1").expect("valid"),
        )
    }

    #[test]
    fn seed_session_advances_one_revision_per_member() {
        let (owner, generation) = ids();
        let domain = domain();
        let mut order = vec![window("a", 0, 0, 10, 10), window("b", 20, 0, 10, 10)];
        order[0].floating = true; // Tiled seed synthesizes this entry as tiled.
        let session = seed_session(&owner, &generation, 7, &domain, &order).expect("seeds");
        assert_eq!(session.accepted_revision(), 2);
        assert_eq!(session.accepted_fingerprint(), 1);
        assert_eq!(session.snapshot().windows.len(), 2);
        assert!(seed_target_bounds(&session, &domain).w > 0);
    }

    #[test]
    fn seed_workspace_session_covers_both_domains() {
        let (owner, generation) = ids();
        let source = domain();
        let target = OutputDomain {
            id: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 300,
                h: 100,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        };
        let source_win = window("a", 0, 0, 10, 10);
        let mut target_win = window("b", 0, 0, 10, 10);
        target_win.output = OutputId("out-2".to_owned());
        let session = seed_workspace_session(
            &owner,
            &generation,
            7,
            &source,
            &target,
            std::slice::from_ref(&source_win),
            std::slice::from_ref(&target_win),
        )
        .expect("seeds");
        assert_eq!(session.accepted_revision(), 2);
        assert_eq!(session.accepted_fingerprint(), 7);
        assert_eq!(session.domains().len(), 2);
        let observation = workspace_observation_for(
            &owner,
            &generation,
            2,
            7,
            std::slice::from_ref(&source_win),
            std::slice::from_ref(&target_win),
        );
        assert_eq!(observation.windows.len(), 2);
        assert!(!workspace_post_matches(&[], &[source_win], &[target_win]));
        let mut floating = window("c", 0, 0, 10, 10);
        floating.floating = true;
        assert!(
            seed_workspace_session(
                &owner,
                &generation,
                7,
                &source,
                &target,
                std::slice::from_ref(&floating),
                &[],
            )
            .is_none()
        );
    }
}
