//! Core-internal v4 presentation producer.
//!
//! [`WorldEngine::presentation_for_publish_v4`] builds the genuine staged
//! [`WorldPresentationV4`](crate::world_report::WorldPresentationV4)
//! directly from the staged v4 facts. No generic-over-report engine, no
//! v4-to-legacy ingress conversion.

use std::collections::{BTreeMap, BTreeSet};

use crate::active_group::describe_active_group_with_hints;
use crate::directional::{Node, NodeId, OutputId, WindowId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::settings_catalog::{
    DEFAULT_CATALOG_BORDER_GAP, DEFAULT_CATALOG_BORDER_RADIUS, DEFAULT_CATALOG_BORDER_WIDTH,
    DEFAULT_CATALOG_UNDERLAY_EXTENSION, FIELD_BORDER_ENABLED, FIELD_BORDER_GAP,
    FIELD_BORDER_RADIUS, FIELD_BORDER_WIDTH, FIELD_UNDERLAY_ENABLED, FIELD_UNDERLAY_EXTENSION,
};
use crate::settings_state::EffectiveSetting;
use crate::size_hints::WindowSizeHints;
use crate::visual::{group_underlay_effective_extension, group_underlay_outer_rect_px};
use crate::world_engine::WorldEngine;
use crate::world_presentation::{
    GroupHideReason, PagerScreen, PagerWindow, PagerWorkspace, PresentationUnderlay,
    PresentationUnderlayFound, pager_workspace_label,
};
use crate::world_report::{
    BorderHideReasonV4, BorderPresentationV4, CommandRejectKind, HoldReason, PresentationIngressV4,
    PresentationRowV4, RenderTarget, RendererInput, WorldPresentationV4,
};

impl WorldEngine {
    fn style_bool_v4(&self, field: &str, default: bool) -> bool {
        match self.settings.effective.get(field) {
            Some(EffectiveSetting::Bool(value)) => *value,
            _ => default,
        }
    }

    fn style_float_v4(&self, field: &str, default: f64) -> f64 {
        match self.settings.effective.get(field) {
            Some(EffectiveSetting::Float(value)) => *value,
            _ => default,
        }
    }

    /// Observed foreground output: the raw `foregroundOut` lane when it
    /// agrees with the staged fact affinity, else fail closed. `None` on
    /// genuine contradiction (both present and different), on missing
    /// output, or on empty foreground.
    fn presentation_fg_out_v4(&self, ingress: &PresentationIngressV4) -> Option<OutputId> {
        let raw = ingress.foreground_out.clone();
        let fact_out = if ingress.foreground.is_empty() {
            None
        } else {
            self.staged
                .facts
                .get(&WindowId(ingress.foreground.clone()))
                .and_then(|fact| fact.output.clone())
        };
        match (raw, fact_out) {
            (None, None) => None,
            (Some(raw), None) => Some(raw),
            (None, Some(fact)) => Some(fact),
            (Some(raw), Some(fact)) if raw == fact => Some(raw),
            (Some(_), Some(_)) => None,
        }
    }

    /// Effective foreground maximize verdict (v4).
    ///
    /// Native route (`caps.maximize`): the verbatim ingress lane, where
    /// `None` stays unknown and fail-closes downstream. Composed route:
    /// there is no native support so native null is valid and the verdict
    /// is the owned `composed_maximized` bit; a non-null native claim is
    /// conflicting input and fail-closes as unknown (row grading owns the
    /// hold; the producer never fabricates a bool).
    fn effective_maximized_v4(&self, ingress: &PresentationIngressV4) -> Option<bool> {
        if self.capabilities().is_some_and(|caps| caps.maximize) {
            return ingress.foreground_maximized;
        }
        if ingress.foreground_maximized.is_some() {
            return None;
        }
        if ingress.foreground.is_empty() {
            return None;
        }
        Some(
            self.composed_maximized
                .contains(&WindowId(ingress.foreground.clone())),
        )
    }

    fn presentation_scale_v4(&self, output: &OutputId) -> Option<f64> {
        self.staged
            .screens
            .iter()
            .find(|screen| &screen.output == output)
            .map(|screen| screen.scale)
    }

    /// Narrow presentation style hold for the staged foreground output:
    /// `Some` when the valid ingress names a foreground whose scaled style
    /// is unrepresentable (non-finite, overflow, or above the existing
    /// [`crate::world_report::MAX_PRESENTATION_STYLE_PX`] cap). Internal
    /// signal only: sub-7 assembly must apply the [`HoldReason::Unreadable`]
    /// hold before publication. No wire lane is minted here.
    // allow(dead_code): pending publish assembly owns this signal; tests drive it.
    #[allow(dead_code)]
    pub(crate) fn presentation_style_hold_v4(&self) -> Option<(OutputId, HoldReason)> {
        let ingress = match &self.staged.presentation_v4 {
            Some(PresentationRowV4::Valid(ingress)) => ingress,
            _ => return None,
        };
        if ingress.foreground.is_empty() {
            return None;
        }
        let output = self.presentation_fg_out_v4(ingress)?;
        let scale = self.presentation_scale_v4(&output)?;
        let width_logical = self.style_float_v4(FIELD_BORDER_WIDTH, DEFAULT_CATALOG_BORDER_WIDTH);
        let gap_logical = self.style_float_v4(FIELD_BORDER_GAP, DEFAULT_CATALOG_BORDER_GAP);
        let radius_logical =
            self.style_float_v4(FIELD_BORDER_RADIUS, DEFAULT_CATALOG_BORDER_RADIUS);
        let extension_logical =
            self.style_float_v4(FIELD_UNDERLAY_EXTENSION, DEFAULT_CATALOG_UNDERLAY_EXTENSION);
        let width = Self::scale_float_once(width_logical, scale);
        let gap = Self::scale_float_once(gap_logical, scale);
        let radius = Self::scale_float_once(radius_logical, scale);
        let extension = Self::scale_float_once(
            group_underlay_effective_extension(extension_logical, width_logical),
            scale,
        );
        if width.is_none() || gap.is_none() || radius.is_none() || extension.is_none() {
            return Some((output, HoldReason::Unreadable));
        }
        let (Some(gap), Some(width)) = (gap, width) else {
            return Some((output, HoldReason::Unreadable));
        };
        if gap.checked_add(width).is_none() {
            return Some((output, HoldReason::Unreadable));
        }
        None
    }

    /// Unconditional neutral focused-fullscreen suspension verdict (v4).
    // allow(dead_code): pending sub-7 publish assembly; tests drive this.
    #[allow(dead_code)]
    pub(crate) fn focused_fullscreen_suspended_output_v4(&self) -> Option<OutputId> {
        let focus = self.staged.observation.focused_global.clone();
        if focus.is_empty() {
            return None;
        }
        if let Some(fact) = self.staged.facts.get(&WindowId(focus.clone())) {
            if fact.fullscreen != Some(true) {
                return None;
            }
            match &self.staged.presentation_v4 {
                Some(PresentationRowV4::Valid(ingress)) if ingress.foreground == focus => {
                    // Truthful fact output wins: a raw/staged out conflict
                    // fail-closes the border/style path only, never the
                    // unconditional fullscreen pager suspension.
                    if let Some(output) = fact.output.clone() {
                        return Some(output);
                    }
                    return self.presentation_fg_out_v4(ingress);
                }
                _ => return fact.output.clone(),
            }
        }
        if let Some(PresentationRowV4::Valid(ingress)) = &self.staged.presentation_v4
            && ingress.foreground == focus
            && ingress.foreground_fullscreen == Some(true)
        {
            return ingress.foreground_out.clone();
        }
        None
    }

    fn hidden_border_v4(reason: Option<BorderHideReasonV4>) -> BorderPresentationV4 {
        BorderPresentationV4 {
            visible: false,
            reason,
            subject: None,
            outsets: None,
            width: None,
            radius: None,
            witness: None,
        }
    }

    // allow(dead_code): pending sub-7 publish assembly; tests drive this.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn presentation_border_v4(
        &self,
        ingress: &PresentationIngressV4,
        suspended: Option<&OutputId>,
    ) -> BorderPresentationV4 {
        if let Some(output) = suspended
            && !ingress.foreground.is_empty()
            && self.presentation_fg_out_v4(ingress).as_ref() == Some(output)
        {
            return Self::hidden_border_v4(Some(BorderHideReasonV4::Suspended));
        }
        if !self.style_bool_v4(FIELD_BORDER_ENABLED, true) {
            return Self::hidden_border_v4(Some(BorderHideReasonV4::Disabled));
        }
        if ingress.foreground.is_empty() {
            return Self::hidden_border_v4(Some(BorderHideReasonV4::NoTarget));
        }
        if ingress.foreground_minimized == Some(true) {
            return Self::hidden_border_v4(Some(BorderHideReasonV4::Minimized));
        }
        if ingress.foreground_fullscreen == Some(true) {
            return Self::hidden_border_v4(Some(BorderHideReasonV4::Fullscreen));
        }
        let effective_maximized = self.effective_maximized_v4(ingress);
        if effective_maximized == Some(true) {
            return Self::hidden_border_v4(Some(BorderHideReasonV4::Maximized));
        }
        if ingress.foreground_host_hidden == Some(true) {
            return Self::hidden_border_v4(Some(BorderHideReasonV4::HostHidden));
        }
        if ingress.foreground_hidden_workspace == Some(true) {
            return Self::hidden_border_v4(Some(BorderHideReasonV4::HiddenWorkspace));
        }
        if ingress.foreground_minimized.is_none()
            || effective_maximized.is_none()
            || ingress.foreground_fullscreen.is_none()
            || ingress.foreground_host_hidden.is_none()
            || ingress.foreground_hidden_workspace.is_none()
        {
            return Self::hidden_border_v4(None);
        }
        let foreground = WindowId(ingress.foreground.clone());
        match self.staged.derived.get(&foreground) {
            Some(classified) if classified.border_only => {
                return Self::hidden_border_v4(Some(BorderHideReasonV4::Shell));
            }
            Some(_) => {}
            None => return Self::hidden_border_v4(None),
        }
        let joined = ingress.witness.as_ref().is_some_and(|witness| {
            witness.win.0 == ingress.foreground && witness.foreground.0 == ingress.foreground
        });
        if !joined {
            return Self::hidden_border_v4(None);
        }
        if ingress.foreground_frame.is_none() {
            return Self::hidden_border_v4(Some(BorderHideReasonV4::NoTarget));
        }
        let scale = self
            .presentation_fg_out_v4(ingress)
            .and_then(|output| self.presentation_scale_v4(&output));
        let Some(scale) = scale else {
            return Self::hidden_border_v4(None);
        };
        let width = Self::scale_float_once(
            self.style_float_v4(FIELD_BORDER_WIDTH, DEFAULT_CATALOG_BORDER_WIDTH),
            scale,
        );
        let radius = Self::scale_float_once(
            self.style_float_v4(FIELD_BORDER_RADIUS, DEFAULT_CATALOG_BORDER_RADIUS),
            scale,
        );
        let gap = Self::scale_float_once(
            self.style_float_v4(FIELD_BORDER_GAP, DEFAULT_CATALOG_BORDER_GAP),
            scale,
        );
        let (Some(width), Some(radius), Some(gap)) = (width, radius, gap) else {
            return Self::hidden_border_v4(None);
        };
        if width <= 0 || radius < 0 || gap < 0 {
            return Self::hidden_border_v4(None);
        }
        let Some(outset) = gap.checked_add(width) else {
            return Self::hidden_border_v4(None);
        };
        BorderPresentationV4 {
            visible: true,
            reason: None,
            subject: Some(foreground),
            outsets: Some([outset, outset, outset, outset]),
            width: Some(width),
            radius: Some(radius),
            witness: ingress.witness.clone(),
        }
    }

    // allow(dead_code): pending sub-7 publish assembly; tests drive this.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn presentation_underlay_v4(
        &self,
        ingress: &PresentationIngressV4,
        suspended: Option<&OutputId>,
    ) -> PresentationUnderlay {
        if let Some(output) = suspended
            && !ingress.foreground.is_empty()
            && self.presentation_fg_out_v4(ingress).as_ref() == Some(output)
        {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Suspended,
            };
        }
        if !self.style_bool_v4(FIELD_UNDERLAY_ENABLED, true) {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Disabled,
            };
        }
        if ingress.foreground.is_empty() {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::NoGroup,
            };
        }
        let foreground = WindowId(ingress.foreground.clone());
        let move_active =
            !ingress.move_window.is_empty() && ingress.move_window == ingress.foreground;
        if !(ingress.chord_held || move_active) {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Idle,
            };
        }
        if ingress.foreground_minimized == Some(true) {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Minimized,
            };
        }
        let effective_maximized = self.effective_maximized_v4(ingress);
        if effective_maximized == Some(true) {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Maximized,
            };
        }
        if ingress.foreground_fullscreen == Some(true) {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Fullscreen,
            };
        }
        if ingress.foreground_minimized.is_none()
            || effective_maximized.is_none()
            || ingress.foreground_fullscreen.is_none()
        {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::NoGroup,
            };
        }
        let Some(home) = self.core_homing(&foreground) else {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            };
        };
        if self
            .tiling
            .get(&home)
            .copied()
            .unwrap_or(crate::world_report::TilingMode::Tiled)
            != crate::world_report::TilingMode::Tiled
        {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            };
        }
        if self
            .members
            .get(&foreground)
            .is_some_and(|state| state.floating)
        {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            };
        }
        let Some(session) = self.engine.session(&home) else {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::NoGroup,
            };
        };
        let base = session.accepted_revision();
        let hidden = |reason| PresentationUnderlay::Hidden {
            base_revision: Some(base),
            reason,
        };
        if session.divergence().is_some() {
            return hidden(GroupHideReason::Diverged);
        }
        if session.has_pending() || session.has_pending_desired() || session.has_drag() {
            return hidden(GroupHideReason::Pending);
        }
        if self.hold_reasons.contains_key(&home) {
            return hidden(GroupHideReason::Pending);
        }
        if self
            .retained
            .iter()
            .any(|transfer| match &transfer.target.kind {
                crate::world_report::TransferKind::Window(window) => window == &foreground,
                crate::world_report::TransferKind::View
                | crate::world_report::TransferKind::Migration => false,
            })
        {
            return hidden(GroupHideReason::Pending);
        }
        let (focus_domain, focus_leaf) = session.focus();
        let (Some(focus_domain), Some(focus_leaf)) = (focus_domain, focus_leaf) else {
            return hidden(GroupHideReason::FocusChanged);
        };
        if focus_domain != home {
            return hidden(GroupHideReason::FocusChanged);
        }
        let snapshot = session.snapshot();
        let Some(tree) = snapshot
            .domains
            .into_iter()
            .find(|domain| domain.output == home.output && domain.workspace == home.workspace)
            .and_then(|domain| domain.tree)
        else {
            return hidden(GroupHideReason::NoGroup);
        };
        let leaf_to_window: BTreeMap<NodeId, WindowId> = snapshot
            .windows
            .into_iter()
            .filter(|link| link.output == home.output && link.workspace == home.workspace)
            .map(|link| (link.leaf, link.window))
            .collect();
        match leaf_to_window.get(&focus_leaf) {
            Some(window) if *window == foreground => {}
            _ => return hidden(GroupHideReason::FocusChanged),
        }
        let staged_windows: Vec<crate::seed::EngineWindow> = self
            .staged
            .domains
            .iter()
            .find(|domain| domain.key == home)
            .map(|domain| self.shielded_windows(&home, &domain.windows))
            .unwrap_or_default();
        let carried: BTreeMap<&WindowId, WindowSizeHints> = staged_windows
            .iter()
            .map(|entry| (&entry.window, entry.hints))
            .collect();
        let resolve = |leaf: &NodeId| {
            leaf_to_window
                .get(leaf)
                .and_then(|window| carried.get(window))
                .copied()
                .unwrap_or_else(WindowSizeHints::none)
        };
        let Some(group) = (|| {
            let retained = self
                .engine
                .session(&home)?
                .domains()
                .iter()
                .find(|domain| domain.key() == home)?;
            describe_active_group_with_hints(
                &tree,
                retained.bounds,
                retained.gap,
                &focus_leaf,
                &leaf_to_window,
                &resolve,
            )
        })() else {
            return hidden(GroupHideReason::RootLeaf);
        };
        let scale = self
            .presentation_fg_out_v4(ingress)
            .and_then(|output| self.presentation_scale_v4(&output));
        let Some(scale) = scale else {
            return hidden(GroupHideReason::NoGroup);
        };
        let gap = Self::scale_float_once(
            self.style_float_v4(FIELD_BORDER_GAP, DEFAULT_CATALOG_BORDER_GAP),
            scale,
        );
        let width_logical = self.style_float_v4(FIELD_BORDER_WIDTH, DEFAULT_CATALOG_BORDER_WIDTH);
        let width = Self::scale_float_once(width_logical, scale);
        let (Some(gap), Some(width)) = (gap, width) else {
            return hidden(GroupHideReason::NoGroup);
        };
        if gap < 0 || width < 0 {
            return hidden(GroupHideReason::NoGroup);
        }
        let extension_logical =
            self.style_float_v4(FIELD_UNDERLAY_EXTENSION, DEFAULT_CATALOG_UNDERLAY_EXTENSION);
        let extension = Self::scale_float_once(
            group_underlay_effective_extension(extension_logical, width_logical),
            scale,
        );
        let Some(extension) = extension else {
            return hidden(GroupHideReason::NoGroup);
        };
        let Some(outer) = group_underlay_outer_rect_px(group.bounds, gap, width, extension) else {
            return hidden(GroupHideReason::NoGroup);
        };
        PresentationUnderlay::Found(PresentationUnderlayFound {
            base_revision: base,
            home,
            group: group.group,
            focused_leaf: focus_leaf,
            focused_window: foreground,
            members: group.members,
            union_rect: group.bounds,
            outer_rect: outer,
        })
    }

    fn departed_v4(&self, window: &WindowId) -> bool {
        if self.staged.closes.contains(window) {
            return true;
        }
        if !self.staged.observation.inventory_complete {
            return false;
        }
        if self
            .staged
            .observation
            .raw_windows
            .iter()
            .any(|fact| &fact.window == window)
        {
            return false;
        }
        if self
            .staged
            .held_windows
            .iter()
            .any(|held| &held.window == window)
        {
            return false;
        }
        true
    }

    /// Retain v4 pager facts from the currently staged observation and
    /// presentation row. Reads `staged` only, never a caller report copy.
    // allow(dead_code): pending sub-7 publish assembly; tests drive this.
    #[allow(dead_code)]
    pub(crate) fn retain_pager_presentation_v4(&mut self) {
        let (fresh_order, fresh_frames) = match &self.staged.presentation_v4 {
            Some(PresentationRowV4::Valid(ingress)) => (
                ingress.pager_order.clone(),
                ingress
                    .pager_frames
                    .iter()
                    .filter_map(|row| row.live.map(|rect| (row.window.clone(), rect)))
                    .collect::<Vec<_>>(),
            ),
            _ => (Vec::new(), Vec::new()),
        };
        let mut roster: BTreeSet<WindowId> = self.members.keys().cloned().collect();
        roster.extend(fresh_order.iter().cloned());
        for (window, _) in &fresh_frames {
            roster.insert(window.clone());
        }
        roster.extend(
            self.staged
                .observation
                .raw_windows
                .iter()
                .map(|fact| fact.window.clone()),
        );
        roster.extend(
            self.staged
                .held_windows
                .iter()
                .map(|held| held.window.clone()),
        );
        roster.retain(|window| !self.departed_v4(window));
        let mut seen: BTreeSet<WindowId> = BTreeSet::new();
        let mut next: Vec<WindowId> = Vec::with_capacity(roster.len());
        for window in &fresh_order {
            if roster.contains(window) && seen.insert(window.clone()) {
                next.push(window.clone());
            }
        }
        for window in self.presentation_order_v4.clone() {
            if roster.contains(&window) && seen.insert(window.clone()) {
                next.push(window.clone());
            }
        }
        for window in &roster {
            if seen.insert(window.clone()) {
                next.push(window.clone());
            }
        }
        self.presentation_order_v4 = next;
        let mut fresh: BTreeMap<WindowId, Rect> = BTreeMap::new();
        for (window, rect) in fresh_frames {
            if roster.contains(&window) {
                fresh.entry(window).or_insert(rect);
            }
        }
        for (window, rect) in fresh {
            self.presentation_live_v4.insert(window, rect);
        }
        let departed: Vec<WindowId> = self
            .presentation_live_v4
            .keys()
            .filter(|window| !roster.contains(*window))
            .cloned()
            .collect();
        for window in departed {
            self.presentation_live_v4.remove(&window);
        }
        let raw_focus = self.staged.observation.focused_global.clone();
        self.presentation_focused_v4 = if raw_focus.is_empty() {
            None
        } else {
            let window = WindowId(raw_focus);
            if self.departed_v4(&window) {
                None
            } else {
                Some(window)
            }
        };
    }

    fn pager_frame_v4(&self, window: &WindowId) -> Option<Rect> {
        if !self.members.contains_key(window) {
            return None;
        }
        self.presentation_live_v4.get(window).copied()
    }

    // allow(dead_code): pending sub-7 publish assembly; tests drive this.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn presentation_pager_v4(&self, suspended: Option<&OutputId>) -> Vec<PagerScreen> {
        let focused_home = self
            .presentation_focused_v4
            .as_ref()
            .and_then(|window| self.adapter_homing(window));
        let mut screens = Vec::new();
        for output in self.logical_outputs() {
            if suspended.is_some_and(|suspended| suspended.0 == output) {
                continue;
            }
            let ring = self.logical_ring(&output);
            let marked = self.trailing_marked_id(&output);
            let view = self.logical_view(&output);
            let current_id = view
                .filter(|view| ring.iter().any(|id| id == view))
                .or_else(|| ring.first().cloned());
            let order: BTreeSet<&WindowId> = self.presentation_order_v4.iter().collect();
            let mut workspaces = Vec::with_capacity(ring.len());
            for (index, id) in ring.iter().enumerate() {
                let homed = self.logical_members(&output, id);
                let occupied = !homed.is_empty();
                let mut ordered: Vec<WindowId> = homed
                    .iter()
                    .filter(|window| !order.contains(*window))
                    .cloned()
                    .collect();
                ordered.extend(
                    self.presentation_order_v4
                        .iter()
                        .filter(|window| homed.contains(window))
                        .rev()
                        .cloned(),
                );
                let mut windows = Vec::new();
                for window in ordered {
                    if let Some(frame) = self.pager_frame_v4(&window) {
                        windows.push(PagerWindow { window, frame });
                    }
                }
                let ordinal = index as u32 + 1;
                let current = current_id.as_deref() == Some(id.as_str());
                let trailing = marked.as_deref() == Some(id.as_str());
                workspaces.push(PagerWorkspace {
                    id: id.clone(),
                    ordinal,
                    label: pager_workspace_label(ordinal),
                    current,
                    occupied,
                    trailing,
                    contains_focused: current
                        && focused_home.as_ref().is_some_and(
                            |(focused_output, focused_workspace)| {
                                focused_output == &output && focused_workspace == id
                            },
                        ),
                    windows,
                });
            }
            screens.push(PagerScreen { output, workspaces });
        }
        screens
    }

    // allow(dead_code): pending sub-7 publish assembly; tests drive this.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn presentation_for_publish_v4(&self) -> WorldPresentationV4 {
        let suspended = self.focused_fullscreen_suspended_output_v4();
        match &self.staged.presentation_v4 {
            Some(PresentationRowV4::Valid(ingress)) => WorldPresentationV4 {
                foreground: ingress.foreground.clone(),
                rev: 0,
                suspended_output: suspended.clone(),
                border: self.presentation_border_v4(ingress, suspended.as_ref()),
                underlay: self.presentation_underlay_v4(ingress, suspended.as_ref()),
                pager: self.presentation_pager_v4(suspended.as_ref()),
            },
            _ => WorldPresentationV4 {
                foreground: String::new(),
                rev: 0,
                suspended_output: suspended.clone(),
                border: Self::hidden_border_v4(None),
                underlay: PresentationUnderlay::Hidden {
                    base_revision: None,
                    reason: GroupHideReason::NoGroup,
                },
                pager: self.presentation_pager_v4(suspended.as_ref()),
            },
        }
    }

    /// Validate one staged v4 renderer input against current core
    /// identity/home state (sub-7 command-handling gate, not a publish
    /// hold). `latest_published_presentation_rev` is the latest PUBLISHED
    /// presentation revision passed explicitly by the caller (sub-7 wires
    /// the stamp and dispatch); the staged v4 producer revision stays a
    /// placeholder and is never read here. `None` input is valid (an
    /// ordinary command carries no renderer row). A drawn revision newer
    /// than the latest published refuses as
    /// [`CommandRejectKind::StaleRenderTarget`] even for an otherwise
    /// exact row; older/equal rows proceed to the row check. Row equality
    /// is identity only: a workspace is valid iff its id currently
    /// belongs on the output, a window iff the exact lifetime is homed
    /// there, a group iff the exact group id exists in the live session
    /// tree there. No ordinal, trailing, binding, label, occupancy, or
    /// member comparison, and no rendered-row history. Returns `None`
    /// when valid, otherwise the rejection kind.
    // allow(dead_code): pending sub-7 stamp+dispatch wiring; tests drive this.
    #[allow(dead_code)]
    pub(crate) fn validate_renderer_input_v4(
        &self,
        input: Option<&RendererInput>,
        latest_published_presentation_rev: u64,
    ) -> Option<CommandRejectKind> {
        let input = input?;
        if input.rev > latest_published_presentation_rev {
            return Some(CommandRejectKind::StaleRenderTarget);
        }
        let valid = match &input.target {
            RenderTarget::Workspace { out, ws } => self.known_workspace(out, &ws.0),
            RenderTarget::Window { out, ws, win } => self
                .core_homing(win)
                .is_some_and(|home| home.output == *out && home.workspace == *ws),
            RenderTarget::Group { out, ws, group } => {
                if !self.known_workspace(out, &ws.0) {
                    return Some(CommandRejectKind::StaleRenderTarget);
                }
                let key = DomainKey {
                    output: out.clone(),
                    workspace: ws.clone(),
                };
                self.engine
                    .session(&key)
                    .and_then(|session| session.tree_for(&key))
                    .is_some_and(|tree| Self::tree_has_group_v4(tree, group))
            }
        };
        if valid {
            None
        } else {
            Some(CommandRejectKind::StaleRenderTarget)
        }
    }

    /// Whether `id` names a live group node in `tree`. Leaves share the
    /// id namespace but never count: only `Group` arms match.
    fn tree_has_group_v4(tree: &Node, id: &NodeId) -> bool {
        match tree {
            Node::Leaf { .. } => false,
            Node::Group {
                id: gid, children, ..
            } => {
                gid == id
                    || children
                        .iter()
                        .any(|child| Self::tree_has_group_v4(child, id))
            }
        }
    }
}
