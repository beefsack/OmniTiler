//! Shared domain-scoped observation validation for typed layout operations.
//!
//! Callers enforce observation completeness and scope in their own refusal
//! order; these helpers check id shapes and retained window bindings.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::WindowId;
use crate::session::{DomainKey, ObservedWindow};

use super::state::DomainLayout;

/// Reject empty homing ids or duplicate observed window ids.
pub(super) fn valid_observed_shapes(observed: &[ObservedWindow]) -> bool {
    let mut seen = BTreeSet::new();
    for entry in observed {
        if entry.window.0.is_empty() || entry.output.0.is_empty() || entry.workspace.0.is_empty() {
            return false;
        }
        if !seen.insert(entry.window.clone()) {
            return false;
        }
    }
    true
}

/// Domain-scoped observed/known binding with no admission: exact homing and
/// either strict exception flags or, with fixed-size admission enabled, only
/// the portable floating flag.
pub(super) fn domain_observed_known_match(
    key: &DomainKey,
    domain: &DomainLayout,
    observed: &[ObservedWindow],
    fixed_admission: bool,
) -> bool {
    let by_id: BTreeMap<&WindowId, &ObservedWindow> =
        observed.iter().map(|w| (&w.window, w)).collect();
    for id in domain.leaves.keys() {
        let Some(entry) = by_id.get(id) else {
            return false;
        };
        if entry.output != key.output || entry.workspace != key.workspace {
            return false;
        }
        if fixed_admission {
            if entry.floating {
                return false;
            }
        } else if entry.flags().any() {
            return false;
        }
    }
    for (id, exception) in &domain.exceptions {
        let Some(entry) = by_id.get(id) else {
            return false;
        };
        if entry.output != key.output || entry.workspace != key.workspace {
            return false;
        }
        if fixed_admission {
            if entry.floating != exception.flags.floating {
                return false;
            }
        } else if entry.flags() != exception.flags {
            return false;
        }
    }
    true
}
