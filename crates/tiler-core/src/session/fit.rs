//! Session near-layout fit: staged fitted initial-topology admission.
//!
//! Responsibility: own [`super::Session::propose_fitted_admit`], the narrow
//! planner-fit entry that stages a complete fitted tree/links observation
//! binding through the normal lifecycle path. World state, operation
//! families, and shared projection/validation helpers stay outside; behavior
//! is unchanged.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    DomainKey, PendingDesired, ProposeError, RefusalKind, SessionObservation, SessionPlan,
};
use crate::contract::{LifecycleCapabilities, LifecycleIntent, LifecycleOperation};
use crate::directional::{Node, NodeId, OutputId, WindowId, WindowLink, WorkspaceId};
use crate::ids::CorrelationId;

impl super::Session {
    /// Propose a complete fitted initial topology through the normal
    /// lifecycle path. Fresh single-domain normal-only adoption: fails with a
    /// refusal (no mutation) unless this session has exactly one domain, no
    /// tiled windows, no exceptions, no pending/drag/divergence, and the
    /// fitted links/tree/observation/admit binding all agree. Stages
    /// `PendingDesired` exactly as normal tiled admission does
    /// (`LifecycleIntent::Admit` / `LifecycleOperation::Admit` on the focused
    /// leaf), so the caller acknowledges and `verify_lifecycle`-commits the
    /// real dispatch like any other admit. No historical MRU is modeled.
    /// Public for the planner fit path (cross-crate); narrowly named.
    #[allow(clippy::too_many_arguments)]
    pub fn propose_fitted_admit(
        &mut self,
        tree: Node,
        links: Vec<WindowLink>,
        focus_leaf: NodeId,
        admit_window: &WindowId,
        admit_output: &OutputId,
        admit_workspace: &WorkspaceId,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        if let Some(reason) = self.reconciler.divergence() {
            return Err(ProposeError::Diverged(reason));
        }
        if self.has_pending() || self.has_pending_desired() {
            return Err(ProposeError::PendingExists);
        }
        if self.drag.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if !self.validate_current_topology() {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        if self.domains.len() != 1 || !self.windows.is_empty() || !self.exceptions.is_empty() {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        if links.len() < 2 {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        if !super::valid_observed_shapes(&session_observation.windows) {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        let domain = &self.domains[0];
        let key = domain.key();
        if admit_output != &key.output || admit_workspace != &key.workspace {
            return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
        }
        for entry in &session_observation.windows {
            if entry.output != key.output || entry.workspace != key.workspace {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
            if entry.flags().any() {
                return Err(ProposeError::Refused(RefusalKind::PartialObservation));
            }
        }
        let mut windows: BTreeMap<WindowId, WindowLink> = BTreeMap::new();
        for link in links {
            if link.window.0.is_empty() || link.leaf.0.is_empty() {
                return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
            }
            if link.output != key.output || link.workspace != key.workspace {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
            if windows.contains_key(&link.window) {
                return Err(ProposeError::Refused(RefusalKind::DuplicateWindow));
            }
            windows.insert(link.window.clone(), link);
        }
        let Some(focus_link) = windows.get(admit_window) else {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        };
        if focus_link.leaf != focus_leaf {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let observed_ids: BTreeSet<&WindowId> = session_observation
            .windows
            .iter()
            .map(|w| &w.window)
            .collect();
        let known_ids: BTreeSet<&WindowId> = windows.keys().collect();
        if observed_ids != known_ids {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
        let mut trees: BTreeMap<DomainKey, Option<Node>> = BTreeMap::new();
        trees.insert(key.clone(), Some(tree));
        if !super::validate_topology(
            &self.domains,
            &trees,
            &windows,
            &self.exceptions,
            &Some(key.clone()),
            &Some(focus_leaf.clone()),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let desired_geometry = super::project_output_geometry(
            self.domain_for(admit_output, admit_workspace),
            trees.get(&key).cloned().flatten().as_ref(),
            &windows,
            &key,
            &super::hints_from_observed(&session_observation.windows),
        )
        .map_err(|_| ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if desired_geometry.is_empty() {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let intent = LifecycleIntent::Admit {
            window: admit_window.clone(),
            output: admit_output.clone(),
            workspace: admit_workspace.clone(),
        };
        let operation = LifecycleOperation::Admit {
            window: admit_window.clone(),
            leaf: focus_leaf.clone(),
            output: admit_output.clone(),
            workspace: admit_workspace.clone(),
        };
        let plan = crate::contract::LifecyclePlan::for_operation(intent, operation);
        let dispatch = match self.reconciler.propose_lifecycle(
            &plan,
            &session_observation.observation,
            correlation_id,
            capabilities,
        ) {
            Ok(dispatch) => dispatch,
            Err(crate::reconcile::ProposeError::PendingExists) => {
                return Err(ProposeError::PendingExists);
            }
            Err(crate::reconcile::ProposeError::Diverged(reason)) => {
                self.pending_desired = None;
                self.drag = None;
                return Err(ProposeError::Diverged(reason));
            }
        };
        let desired_snapshot = self.snapshot_for(&trees, &windows);
        // A clean-fit roster shares one admission sequence number, so
        // identity breaks those D7 victim ties (D8). Lifetimes already
        // tracked keep their first number.
        let mut fitted_order = self.admission_order.clone();
        let mut fitted_counter = self.admission_counter;
        let mut fitted_fresh = false;
        for link in windows.values() {
            if !fitted_order.contains_key(&link.window) {
                fitted_order.insert(link.window.clone(), fitted_counter);
                fitted_fresh = true;
            }
        }
        if fitted_fresh {
            fitted_counter = fitted_counter.saturating_add(1);
        }
        self.pending_desired = Some(PendingDesired {
            trees: trees.clone(),
            windows: windows.clone(),
            focused_domain: Some(key.clone()),
            focused_leaf: Some(focus_leaf.clone()),
            last_active: self.updated_last_active(
                &Some(key.clone()),
                &Some(focus_leaf.clone()),
                &trees,
                &windows,
            ),
            exceptions: self.exceptions.clone(),
            retained_float_geometry: self.retained_float_geometry.clone(),
            automatic_fixed: self.automatic_fixed.clone(),
            fixed_tile_override: self.fixed_tile_override.clone(),
            capacity_floats: self.capacity_floats.clone(),
            admission_order: fitted_order,
            admission_counter: fitted_counter,
            overlap_accepted: self.overlap_accepted.clone(),
        });
        Ok(SessionPlan {
            dispatch,
            desired_snapshot,
            desired_focus_domain: Some(key),
            desired_focus_leaf: Some(focus_leaf),
            desired_geometry,
        })
    }
}
