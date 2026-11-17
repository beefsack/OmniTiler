use std::collections::BTreeMap;

use crate::directional::{Direction, NodeId, OutputId, WindowId};
use crate::geometry::Rect;
use crate::session::{DomainKey, ExceptionFlags};

use super::projection::project_borrowed_geometry;
use super::state::WorldLayout;
use super::topology::collect_leaves;

impl WorldLayout {
    /// Retained fixed-size admission predicate, read directly for tests.
    #[must_use]
    pub(crate) fn fixed_size_predicate(&self) -> crate::size_hints::FixedSizePredicate {
        self.config.fixed_size_predicate
    }

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

    /// Whether `window` carries capacity-float provenance in `key` only.
    #[must_use]
    pub(crate) fn is_capacity_float(&self, key: &DomainKey, window: &WindowId) -> bool {
        self.domain(key)
            .is_some_and(|domain| domain.capacity_floats.contains(window))
    }

    /// Retained admission sequence for a window in one domain, if any.
    #[must_use]
    pub(crate) fn admission_sequence(&self, key: &DomainKey, window: &WindowId) -> Option<u64> {
        self.domain(key)
            .and_then(|domain| domain.admission_order.get(window))
            .copied()
    }

    /// Retained output home for a window from live tiled membership: the
    /// first retained domain (in key order) linking the window names its
    /// output. Port of `Engine::retained_home_any`; the key is the sole home
    /// of every tiled link. `None` means unhomed with no authority.
    #[must_use]
    pub(crate) fn retained_home_any(&self, window: &WindowId) -> Option<OutputId> {
        self.domains
            .iter()
            .find(|(_, domain)| domain.leaves.contains_key(window))
            .map(|(key, _)| key.output.clone())
    }

    /// Tiled window/leaf bindings in one domain, in window-id order. The
    /// inverse leaf-to-window map is derived by callers; no mutable view or
    /// map is exposed.
    #[must_use]
    pub(crate) fn bindings(&self, key: &DomainKey) -> impl Iterator<Item = (&WindowId, &NodeId)> {
        self.domain(key)
            .into_iter()
            .flat_map(|domain| domain.leaves.iter())
    }

    /// Retained bounds and inner gap for one domain, if it exists.
    #[must_use]
    pub(crate) fn domain_geometry(&self, key: &DomainKey) -> Option<(Rect, i32)> {
        self.domain(key).map(|domain| (domain.bounds, domain.gap))
    }

    /// Retained outer gap for one domain, if it exists.
    #[must_use]
    pub(crate) fn domain_outer_gap(&self, key: &DomainKey) -> Option<i32> {
        self.domain(key).map(|domain| domain.outer_gap)
    }

    /// Whether `window` is marked as an automatic fixed-size float in `key`.
    #[must_use]
    pub(crate) fn is_automatic_fixed_float(&self, key: &DomainKey, window: &WindowId) -> bool {
        self.domain(key)
            .is_some_and(|domain| domain.automatic_fixed.contains(window))
    }

    /// Whether `window` carries an explicit fixed-tile override in `key`.
    #[must_use]
    pub(crate) fn has_fixed_tile_override(&self, key: &DomainKey, window: &WindowId) -> bool {
        self.domain(key)
            .is_some_and(|domain| domain.fixed_tile_override.contains(window))
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

    /// Forget any minimize restore slot for `window` in `key`, returning
    /// whether a slot was removed. Revision-neutral.
    pub(crate) fn forget_minimized_slot(&mut self, key: &DomainKey, window: &WindowId) -> bool {
        self.domains
            .get_mut(key)
            .is_some_and(|domain| domain.minimized_slots.remove(window).is_some())
    }

    /// Durable retained intentional-float geometry that survives unfloat. A
    /// window keeps its last floating placement here while tiled, so the next
    /// float can select it instead of recomputing the centered fallback.
    #[must_use]
    pub(crate) fn retained_float_geometry(
        &self,
        key: &DomainKey,
        window: &WindowId,
    ) -> Option<Rect> {
        self.domain(key)
            .and_then(|domain| domain.retained_float_geometry.get(window))
            .copied()
    }

    /// Updates both the live exception geometry and the durable retained
    /// placement so clear/unfloat restore the user placement exactly. Only
    /// for floating non-sticky exceptions; refuses unknown windows,
    /// non-float exceptions, and malformed rects with no mutation.
    /// Revision-neutral.
    pub(crate) fn adopt_float_placement(
        &mut self,
        key: &DomainKey,
        window: &WindowId,
        rect: Rect,
    ) -> bool {
        if rect.w <= 0
            || rect.h <= 0
            || rect.x.checked_add(rect.w).is_none()
            || rect.y.checked_add(rect.h).is_none()
        {
            return false;
        }
        let Some(domain) = self.domains.get_mut(key) else {
            return false;
        };
        let Some(record) = domain.exceptions.get_mut(window) else {
            return false;
        };
        if !record.flags.floating
            || record.flags.sticky
            || record.flags.fullscreen
            || record.flags.maximized
        {
            return false;
        }
        record.floating_geometry = Some(rect);
        domain.retained_float_geometry.insert(window.clone(), rect);
        true
    }

    /// Atomically move a floating non-sticky exception from `from` to `to`
    /// while adopting `rect` as its live and durable float placement.
    /// Revision-neutral. Returns `false` without mutating either domain when
    /// the rectangle, domains, or exception flags are invalid.
    pub(crate) fn move_exception_float_with_placement(
        &mut self,
        from: &DomainKey,
        to: &DomainKey,
        window: &WindowId,
        rect: Rect,
    ) -> bool {
        if from == to
            || rect.w <= 0
            || rect.h <= 0
            || rect.x.checked_add(rect.w).is_none()
            || rect.y.checked_add(rect.h).is_none()
        {
            return false;
        }
        let Some(source) = self.domains.get(from) else {
            return false;
        };
        let Some(target) = self.domains.get(to) else {
            return false;
        };
        let Some(record) = source.exceptions.get(window) else {
            return false;
        };
        if !record.flags.floating
            || record.flags.sticky
            || record.flags.fullscreen
            || record.flags.maximized
        {
            return false;
        }
        if target.leaves.contains_key(window) || target.exceptions.contains_key(window) {
            return false;
        }

        let mut moved_record = {
            let source_mut = self.domains.get_mut(from).expect("checked source");
            let record = source_mut
                .exceptions
                .remove(window)
                .expect("checked exception");
            source_mut.retained_float_geometry.remove(window);
            let automatic_fixed = source_mut.automatic_fixed.remove(window);
            let fixed_tile_override = source_mut.fixed_tile_override.remove(window);
            let capacity_float = source_mut.capacity_floats.remove(window);
            let admission_order = source_mut.admission_order.remove(window);
            (
                record,
                automatic_fixed,
                fixed_tile_override,
                capacity_float,
                admission_order,
            )
        };
        moved_record.0.floating_geometry = Some(rect);

        let target_mut = self.domains.get_mut(to).expect("checked target");
        target_mut.exceptions.insert(window.clone(), moved_record.0);
        target_mut
            .retained_float_geometry
            .insert(window.clone(), rect);
        if moved_record.1 {
            target_mut.automatic_fixed.insert(window.clone());
        }
        if moved_record.2 {
            target_mut.fixed_tile_override.insert(window.clone());
        }
        if moved_record.3 {
            target_mut.capacity_floats.insert(window.clone());
        }
        if let Some(order) = moved_record.4 {
            target_mut.admission_order.insert(window.clone(), order);
        }
        true
    }

    /// Live floating geometry from a retained exception record only.
    /// Does not read the durable `retained_float_geometry` map, which can
    /// survive while tiled or differ from the live exception frame.
    #[must_use]
    pub(crate) fn exception_float_geometry(
        &self,
        key: &DomainKey,
        window: &WindowId,
    ) -> Option<Rect> {
        self.domain(key)
            .and_then(|domain| domain.exceptions.get(window))
            .and_then(|record| record.floating_geometry)
    }

    /// Select the cross-output entry-edge target leaf in `target` for a
    /// directional focus crossing in `direction` with source perpendicular
    /// span `source_perp`.
    #[must_use]
    pub(crate) fn select_cross_output_edge_target(
        &self,
        target: &DomainKey,
        direction: Direction,
        source_perp: (i32, i32),
    ) -> Option<NodeId> {
        let domain = self.domains.get(target)?;
        let tree = domain.tree.as_ref()?;
        let projected = project_borrowed_geometry(
            target,
            tree,
            &domain.leaves,
            domain.bounds,
            domain.gap,
            &BTreeMap::new(),
        )?;
        let rects: BTreeMap<&NodeId, Rect> = projected
            .iter()
            .map(|entry| (&entry.leaf, entry.rect))
            .collect();
        let mut rects_dfs: Vec<(NodeId, Rect)> = Vec::new();
        for leaf in collect_leaves(tree) {
            if !domain.leaves.values().any(|bound| bound == &leaf) {
                continue;
            }
            if let Some(rect) = rects.get(&leaf) {
                rects_dfs.push((leaf, *rect));
            }
        }
        let history = self.cross_output_history_newest_first(target);
        crate::session::rank_cross_output_edge_candidates(
            &rects_dfs,
            direction,
            &history,
            source_perp,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::directional::{OutputId, WindowId, WorkspaceId};
    use crate::geometry::Rect;
    use crate::session::{DomainKey, ExceptionFlags};

    use super::super::state::{DomainException, DomainLayout, WorldLayout};

    #[test]
    fn exception_float_geometry_discriminates_live_exception_from_durable_retained() {
        let mut layout = WorldLayout::new();
        let key = DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        let win_a = WindowId("win-a".to_owned());
        let win_b = WindowId("win-b".to_owned());
        let live_rect = Rect {
            x: 10,
            y: 20,
            w: 300,
            h: 200,
        };
        let durable_rect = Rect {
            x: 50,
            y: 60,
            w: 400,
            h: 300,
        };

        let mut domain = DomainLayout::new(
            Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            0,
            0,
        );
        domain.exceptions.insert(
            win_a.clone(),
            DomainException {
                flags: ExceptionFlags {
                    floating: true,
                    sticky: false,
                    fullscreen: false,
                    maximized: false,
                },
                floating_geometry: Some(live_rect),
            },
        );
        domain
            .retained_float_geometry
            .insert(win_a.clone(), durable_rect);
        domain
            .retained_float_geometry
            .insert(win_b.clone(), durable_rect);
        layout.domains.insert(key.clone(), domain);

        assert_eq!(
            layout.exception_float_geometry(&key, &win_a),
            Some(live_rect)
        );
        assert_eq!(
            layout.retained_float_geometry(&key, &win_a),
            Some(durable_rect)
        );
        assert_eq!(layout.exception_float_geometry(&key, &win_b), None);
        assert_eq!(
            layout.retained_float_geometry(&key, &win_b),
            Some(durable_rect)
        );
    }
}
