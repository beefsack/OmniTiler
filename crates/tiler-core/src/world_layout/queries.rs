use crate::directional::{NodeId, WindowId};
use crate::session::{DomainKey, ExceptionFlags};

use super::state::WorldLayout;

impl WorldLayout {
    /// Whether a domain slot is retained.
    #[must_use]
    pub(crate) fn contains_domain(&self, key: &DomainKey) -> bool {
        self.domains.contains_key(key)
    }

    /// Whether a window has a retained tiled leaf in this domain.
    #[must_use]
    pub(crate) fn is_tiled_window(&self, key: &DomainKey, window: &WindowId) -> bool {
        self.domain(key)
            .is_some_and(|domain| domain.leaves.contains_key(window))
    }

    /// Whether a window is a retained exception in this domain.
    #[must_use]
    pub(crate) fn is_exception(&self, key: &DomainKey, window: &WindowId) -> bool {
        self.domain(key)
            .is_some_and(|domain| domain.exceptions.contains_key(window))
    }

    /// Whether a window has a retained minimize restore slot in this domain.
    #[must_use]
    pub(crate) fn has_minimized_slot(&self, key: &DomainKey, window: &WindowId) -> bool {
        self.domain(key)
            .is_some_and(|domain| domain.minimized_slots.contains_key(window))
    }

    /// Whether any retained domain links, excepts or restore-slots a window.
    #[must_use]
    pub(crate) fn knows_window_any(&self, window: &WindowId) -> bool {
        self.domains.values().any(|domain| {
            domain.leaves.contains_key(window)
                || domain.exceptions.contains_key(window)
                || domain.minimized_slots.contains_key(window)
        })
    }

    /// Whether any retained domain links a window as a tiled leaf.
    #[must_use]
    pub(crate) fn is_tiled_window_any(&self, window: &WindowId) -> bool {
        self.domains
            .values()
            .any(|domain| domain.leaves.contains_key(window))
    }

    /// Whether any retained domain excepts a window.
    #[must_use]
    pub(crate) fn is_exception_window_any(&self, window: &WindowId) -> bool {
        self.domains
            .values()
            .any(|domain| domain.exceptions.contains_key(window))
    }

    /// Tiled window identities retained in one domain, in window-id order.
    #[must_use]
    pub(crate) fn tiled_windows(&self, key: &DomainKey) -> impl Iterator<Item = &WindowId> {
        self.domain(key)
            .into_iter()
            .flat_map(|domain| domain.leaves.keys())
    }

    /// Exception window identities retained in one domain, in window-id order.
    #[must_use]
    pub(crate) fn exception_windows(&self, key: &DomainKey) -> impl Iterator<Item = &WindowId> {
        self.domain(key)
            .into_iter()
            .flat_map(|domain| domain.exceptions.keys())
    }

    /// Retained exception flags for one window, if it is an exception here.
    #[must_use]
    pub(crate) fn exception_flags(
        &self,
        key: &DomainKey,
        window: &WindowId,
    ) -> Option<ExceptionFlags> {
        self.domain(key)
            .and_then(|domain| domain.exceptions.get(window))
            .map(|exception| exception.flags)
    }

    /// Exception identities with flags for one domain, in window-id order.
    #[must_use]
    pub(crate) fn exception_windows_with_flags(
        &self,
        key: &DomainKey,
    ) -> impl Iterator<Item = (WindowId, ExceptionFlags)> {
        self.domain(key).into_iter().flat_map(|domain| {
            domain
                .exceptions
                .iter()
                .map(|(window, exception)| (window.clone(), exception.flags))
        })
    }

    /// Minimize restore-slot identities retained in one domain.
    #[must_use]
    pub(crate) fn minimized_windows(&self, key: &DomainKey) -> impl Iterator<Item = &WindowId> {
        self.domain(key)
            .into_iter()
            .flat_map(|domain| domain.minimized_slots.keys())
    }

    /// Retained tiled counts for one domain.
    #[must_use]
    pub(crate) fn tiled_count(&self, key: &DomainKey) -> usize {
        self.domain(key).map_or(0, |domain| domain.leaves.len())
    }

    /// Retained exception count for one domain.
    #[must_use]
    pub(crate) fn exception_count(&self, key: &DomainKey) -> usize {
        self.domain(key).map_or(0, |domain| domain.exceptions.len())
    }

    /// Retained minimize-slot count for one domain.
    #[must_use]
    pub(crate) fn minimized_count(&self, key: &DomainKey) -> usize {
        self.domain(key)
            .map_or(0, |domain| domain.minimized_slots.len())
    }

    /// Upper bound of retained layout rows: every domain's tiled links plus
    /// exceptions plus minimize slots. A union lifetime counted twice still
    /// fits the conservative sum.
    #[must_use]
    pub(crate) fn retained_bound_count(&self) -> usize {
        self.domains.values().fold(0usize, |total, domain| {
            total
                .saturating_add(domain.leaves.len())
                .saturating_add(domain.exceptions.len())
                .saturating_add(domain.minimized_slots.len())
        })
    }

    /// All retained tiled windows, in domain/window-id order.
    #[must_use]
    pub(crate) fn all_tiled_windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.domains
            .values()
            .flat_map(|domain| domain.leaves.keys().cloned())
    }

    /// All retained exception windows with their flags.
    #[must_use]
    pub(crate) fn all_exception_windows_with_flags(
        &self,
    ) -> impl Iterator<Item = (WindowId, ExceptionFlags)> + '_ {
        self.domains.values().flat_map(|domain| {
            domain
                .exceptions
                .iter()
                .map(|(window, exception)| (window.clone(), exception.flags))
        })
    }

    /// All retained exception windows.
    #[must_use]
    pub(crate) fn all_exception_windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.domains
            .values()
            .flat_map(|domain| domain.exceptions.keys().cloned())
    }

    /// All retained minimize-slot windows.
    #[must_use]
    pub(crate) fn all_minimized_windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.domains
            .values()
            .flat_map(|domain| domain.minimized_slots.keys().cloned())
    }

    /// First tiled window bound to a leaf in a domain, in window-id order.
    /// Used by stay-source focus; duplicate leaf bindings are malformed but
    /// resolve deterministically here.
    #[must_use]
    pub(crate) fn window_for_leaf(&self, key: &DomainKey, leaf: &NodeId) -> Option<WindowId> {
        self.domain(key)?
            .leaves
            .iter()
            .find_map(|(window, bound)| (bound == leaf).then(|| window.clone()))
    }

    /// Sorted tiled residents of a domain for focus fallback.
    #[must_use]
    pub(crate) fn resident_windows(&self, key: &DomainKey) -> Vec<WindowId> {
        self.tiled_windows(key).cloned().collect()
    }

    /// Whether any retained domain carries an explicit tile override.
    #[must_use]
    pub(crate) fn has_fixed_tile_override_any(&self, window: &WindowId) -> bool {
        self.domains
            .values()
            .any(|domain| domain.fixed_tile_override.contains(window))
    }

    /// Whether any retained domain carries an automatic fixed float.
    #[must_use]
    pub(crate) fn is_automatic_fixed_float_any(&self, window: &WindowId) -> bool {
        self.domains
            .values()
            .any(|domain| domain.automatic_fixed.contains(window))
    }

    /// Whether any retained domain carries capacity-float provenance.
    #[must_use]
    pub(crate) fn is_capacity_float_any(&self, window: &WindowId) -> bool {
        self.domains
            .values()
            .any(|domain| domain.capacity_floats.contains(window))
    }

    /// Retained admission sequence for a window in one domain, if any.
    #[must_use]
    pub(crate) fn admission_sequence(&self, key: &DomainKey, window: &WindowId) -> Option<u64> {
        self.domain(key)
            .and_then(|domain| domain.admission_order.get(window))
            .copied()
    }

    /// Production commit for a restored explicit tile. Records the override
    /// in every domain currently linking the window tiled; unknown windows
    /// are ignored and existing marks are kept. Revision-neutral.
    pub(crate) fn commit_fixed_tile_override(&mut self, window: &WindowId) {
        for domain in self.domains.values_mut() {
            if domain.leaves.contains_key(window) {
                domain.fixed_tile_override.insert(window.clone());
            }
        }
    }
}
