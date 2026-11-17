//! Typed synchronous fitted admission over the sole retained `WorldLayout`.
//!
//! Fresh single-domain, normal-only fit from one complete carried observation.
//! The caller owns raw command decoding and the convergence/seeding/focus
//! sync: it enters here only when the domain is genuinely absent, there is no
//! explicit placement and the admitted window is the focused anchor. This
//! operation fits through the generation-shared allocator, validates the
//! staged tree/bindings/observation, and installs one fresh `DomainLayout`
//! atomically. A decline leaves retained state untouched except for the shared
//! allocator, whose group serials burn on the fit. No session, pending plan,
//! correlation, fingerprint, handshake or synthetic protocol is built here.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Node, NodeId, WindowId, WindowLink};
use crate::seed::{
    EngineWindow, FitDeclineReason, observed_window_from_engine,
    try_recursive_cut_fit_with_centre_count_and_alloc,
};
use crate::session::{ObservedWindow, OutputDomain};
use crate::size_hints::{
    FixedSizePredicate, WindowSizeHints, is_fixed_size_with, project_with_hints,
};

use super::focus::settle_domain_focus_tail;
use super::observation::valid_observed_shapes;
use super::outcome::{AdmissionOutcome, FittedAdmissionDecline, FittedAdmissionReason};
use super::projection::project_borrowed_geometry;
use super::state::{DomainLayout, WorldLayout};
use super::topology::validate_domain_topology;

#[cfg(test)]
#[path = "fit_tests.rs"]
mod tests;

impl WorldLayout {
    /// Fit and admit `window` into a brand-new single domain from a complete
    /// carried observation. `window` is the focused anchor; `observed` must
    /// cover exactly the clean fitted roster.
    ///
    /// On success the fresh domain's accepted revision advances from zero to
    /// one, the whole roster shares the current admission high-water (which
    /// advances once), and the focused anchor settles into MRU/last-active
    /// with no accepted focus history. On any decline no retained state is
    /// installed or advanced; allocator burns survive through the shared
    /// counter.
    pub(crate) fn try_fitted_admission(
        &mut self,
        domain: &OutputDomain,
        outer_gap: i32,
        window: &WindowId,
        observed: &[EngineWindow],
    ) -> Result<AdmissionOutcome, FittedAdmissionDecline> {
        let key = domain.key();

        // Caller preconditions fail closed before any fitter work: the domain
        // must be genuinely absent and the descriptor valid. No fit runs, so
        // no serials burn here.
        if self.domains.contains_key(&key) || !domain.validate() || outer_gap < 0 {
            return Err(commit_failed());
        }

        // A fixed-size candidate never joins the fitted topology: it floats
        // at admission. Decline with the fit-excluded token before any fitter
        // burn, matching the Engine's fit-excluded gate.
        let predicate = self.config.fixed_size_predicate;
        if self.config.fixed_size_admission
            && observed
                .iter()
                .any(|entry| is_fixed_candidate(entry, predicate))
        {
            return Err(fit(FitDeclineReason::FitExcluded, 0));
        }

        // The fitter draws from the generation-shared allocator; its burns
        // survive every later decline through the Arc-backed counter.
        let (tree, links, centre_splits) = match try_recursive_cut_fit_with_centre_count_and_alloc(
            domain,
            observed,
            &self.group_alloc,
        ) {
            Ok(fitted) => fitted,
            Err(reason) => return Err(fit(reason, 0)),
        };

        // Fits needing a centre split decline to the sequential seed.
        if centre_splits > 0 {
            return Err(fit(FitDeclineReason::CentreSplit, centre_splits));
        }

        // A clean but minimum-infeasible fitted topology declines too. A
        // projection failure is not infeasibility and falls through to the
        // normal validation gates.
        if fitted_topology_is_min_infeasible(domain, &tree, &links, observed) {
            return Err(fit(FitDeclineReason::MinInfeasible, 0));
        }

        // Validate the fresh fitted domain/bindings/normal observation/focus
        // exactly as the old propose_fitted_admit did. Every semantic or
        // validation failure collapses to CommitFailed, since the old fit
        // route fell back on any failed commit.
        if links.len() < 2 {
            return Err(commit_failed());
        }

        // Semantic observations are derived only from carried data via the
        // existing conversion; no protocol or invented observation is built.
        let semantic: Vec<ObservedWindow> =
            observed.iter().map(observed_window_from_engine).collect();
        if !valid_observed_shapes(&semantic) {
            return Err(commit_failed());
        }
        for entry in &semantic {
            if entry.output != key.output || entry.workspace != key.workspace {
                return Err(commit_failed());
            }
            if entry.flags().any() {
                return Err(commit_failed());
            }
        }

        let mut windows: BTreeMap<WindowId, WindowLink> = BTreeMap::new();
        for link in links {
            if link.window.0.is_empty()
                || link.leaf.0.is_empty()
                || link.output != key.output
                || link.workspace != key.workspace
            {
                return Err(commit_failed());
            }
            if windows.contains_key(&link.window) {
                return Err(commit_failed());
            }
            windows.insert(link.window.clone(), link);
        }

        let focus_leaf = windows
            .get(window)
            .map(|link| link.leaf.clone())
            .ok_or_else(commit_failed)?;

        let observed_ids: BTreeSet<&WindowId> = semantic.iter().map(|w| &w.window).collect();
        let known_ids: BTreeSet<&WindowId> = windows.keys().collect();
        if observed_ids != known_ids {
            return Err(commit_failed());
        }

        let hints: BTreeMap<WindowId, WindowSizeHints> = observed
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect();

        // One candidate, built with the existing constructor semantics; no
        // output/workspace identity is copied from links into stored state.
        let mut candidate = DomainLayout::new(domain.bounds, domain.gap, outer_gap);
        candidate.tree = Some(tree);
        for (id, link) in &windows {
            candidate.leaves.insert(id.clone(), link.leaf.clone());
        }
        if !validate_domain_topology(&key, &candidate) {
            return Err(commit_failed());
        }

        let geometry = project_borrowed_geometry(
            &key,
            candidate.tree.as_ref().expect("tree set above"),
            &candidate.leaves,
            candidate.bounds,
            candidate.gap,
            &hints,
        )
        .ok_or_else(commit_failed)?;
        if geometry.is_empty() {
            return Err(commit_failed());
        }

        // The clean-fit roster shares one admission sequence number: the
        // current high-water is stamped on every new member and advances
        // exactly once (saturating), never once per member.
        let mut counter = self.admission_seq_next;
        let mut fresh = false;
        for id in windows.keys() {
            if !candidate.admission_order.contains_key(id) {
                candidate.admission_order.insert(id.clone(), counter);
                fresh = true;
            }
        }
        if fresh {
            counter = counter.saturating_add(1);
        }

        // Exact verify_lifecycle commit tail: settle the focused leaf as MRU
        // and last-active, prune accepted history without appending it, prune
        // fixed/capacity provenance, then advance the accepted revision 0 -> 1.
        settle_domain_focus_tail(&mut candidate, &[], Some(focus_leaf.clone()), None);
        WorldLayout::prune_domain_fixed_size_state(&mut candidate);
        WorldLayout::prune_domain_capacity_state(&mut candidate);
        candidate.accepted_revision = 1;

        // Install state and high-water only after every gate passed.
        self.domains.insert(key, candidate);
        self.admission_seq_next = counter;

        Ok(AdmissionOutcome {
            base_revision: 0,
            revision: 1,
            admitted_leaf: Some(focus_leaf),
            geometry,
        })
    }
}

fn fit(reason: FitDeclineReason, centre_splits: usize) -> FittedAdmissionDecline {
    FittedAdmissionDecline {
        reason: FittedAdmissionReason::Fit(reason),
        centre_splits,
    }
}

fn commit_failed() -> FittedAdmissionDecline {
    FittedAdmissionDecline {
        reason: FittedAdmissionReason::CommitFailed,
        centre_splits: 0,
    }
}

/// Engine predicate: fixed hints under the predicate, not born fullscreen,
/// not already floating, no adapter-asserted tile win; sticky stays
/// intentional. Identical to `Engine::is_fixed_candidate_with`. Shared with
/// the fresh-adoption router for its exact `fixed_present` gate.
pub(super) fn is_fixed_candidate(entry: &EngineWindow, predicate: FixedSizePredicate) -> bool {
    !entry.floating
        && !entry.fullscreen
        && !entry.sticky
        && !entry.fixed_suppress
        && is_fixed_size_with(entry.hints, predicate)
}

/// Read-only min-infeasibility probe: the hinted projection of a clean fitted
/// topology flags at least one leaf overconstrained. A projection failure is
/// not infeasibility and reports false, matching the Engine helper.
fn fitted_topology_is_min_infeasible(
    domain: &OutputDomain,
    tree: &Node,
    links: &[WindowLink],
    observed: &[EngineWindow],
) -> bool {
    let by_window: BTreeMap<&WindowId, WindowSizeHints> =
        observed.iter().map(|w| (&w.window, w.hints)).collect();
    let by_leaf: BTreeMap<&NodeId, &WindowId> =
        links.iter().map(|l| (&l.leaf, &l.window)).collect();
    let resolve = |leaf: &NodeId| {
        by_leaf
            .get(leaf)
            .and_then(|window| by_window.get(*window))
            .copied()
            .unwrap_or_else(WindowSizeHints::none)
    };
    matches!(
        project_with_hints(tree, domain.bounds, domain.gap, &resolve),
        Ok(hinted) if !hinted.overconstrained.is_empty()
    )
}
