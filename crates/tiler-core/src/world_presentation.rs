//! Uniform transport-facing world presentation (stage 5).
//!
//! The sole presentation contract a remote consumer (for example a KDE D-Bus
//! script) can use: every accepted report carries the normalized
//! [`PresentationIngress`] facts and every accepted reply carries the
//! computed [`WorldPresentation`] shapes and content. Windows consumes only
//! the reply: no in-process engine presentation query remains.
//!
//! The adapter keeps only native facts (proof-verified foreground identity,
//! liveness, scope/proof gates, host-scaled style numbers, z-anchor/topmost,
//! drawing, and final freshness vetoes). Eligibility, reasons, and complete
//! bounds live here: given fresh genuine foreground facts plus retained
//! state, derive what the renderer should show without running a world
//! command, actuating, or touching history, revision, mapping, membership,
//! or the gesture lifecycle.
//!
//! The group query never adopts the foreground, admits windows, advances the
//! revision, or fabricates a group on retained focus mismatch: a retained
//! focus naming any other lifetime hides as focus-changed. Held scopes,
//! outstanding transfers, and session pending/drag/divergence residue all
//! fail closed. An open native gesture never blocks the highlight on its
//! own - like the retired world-command route, only the Engine session
//! state behind it can - so fast per-loop-wake event following never needs
//! repeated world commands for presentation.
//!
//! The focused-fullscreen-game lane pauses presentation work: a report with
//! `paused` set returns hidden border/underlay and an empty pager without
//! projecting, so no hidden projection cost runs under the game.

use std::collections::{BTreeMap, BTreeSet};

use crate::active_group::{ActiveGroupMember, describe_active_group_with_hints};
use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::visual::{
    BorderEligibilityFacts, BorderHideReason, BorderPresentation, border_eligibility,
    border_geometry, group_underlay_outer_rect_px,
};
use crate::world_engine::WorldEngine;
use crate::world_report::{PresentationIngress, TilingMode, WorldReport};

/// Closed hide-reason vocabulary for the group highlight. Tokens match the
/// retired world-command consumer so log lines are unchanged, plus the
/// transport-only `idle` (untriggered) and `suspended` (game lane) hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupHideReason {
    Disabled,
    Minimized,
    Maximized,
    Fullscreen,
    Floating,
    Pending,
    Diverged,
    RootLeaf,
    FocusChanged,
    #[default]
    NoGroup,
    Idle,
    Suspended,
}

impl GroupHideReason {
    /// Closed log vocabulary for the hide reason.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Minimized => "minimized",
            Self::Maximized => "maximized",
            Self::Fullscreen => "fullscreen",
            Self::Floating => "floating",
            Self::Pending => "pending",
            Self::Diverged => "diverged",
            Self::RootLeaf => "root-leaf",
            Self::FocusChanged => "focus-changed",
            Self::NoGroup => "no-group",
            Self::Idle => "idle",
            Self::Suspended => "suspended",
        }
    }
}

/// Resolved group highlight with everything the renderer needs: the
/// immediate-parent group with its projected members, their checked union,
/// and the host-scaled outer fill rect, all in adapter units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationUnderlayFound {
    pub base_revision: u64,
    pub home: DomainKey,
    pub group: NodeId,
    pub focused_leaf: NodeId,
    pub focused_window: WindowId,
    pub members: Vec<ActiveGroupMember>,
    pub union_rect: Rect,
    pub outer_rect: Rect,
}

/// Read-only underlay presentation: either the resolved highlight or a
/// classified hide with the retained base revision when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresentationUnderlay {
    Found(PresentationUnderlayFound),
    Hidden {
        base_revision: Option<u64>,
        reason: GroupHideReason,
    },
}

/// Uniform per-reply presentation: the tick-time foreground token the shapes
/// were computed for (the renderer revalidates the live foreground against
/// it and never paints stale shapes), the border shape, the underlay shape,
/// and the pager content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldPresentation {
    pub foreground: String,
    pub border: BorderPresentation,
    pub underlay: PresentationUnderlay,
    pub pager: Vec<PagerScreen>,
}

impl Default for PresentationUnderlay {
    fn default() -> Self {
        Self::Hidden {
            base_revision: None,
            reason: GroupHideReason::NoGroup,
        }
    }
}

impl Default for BorderPresentation {
    fn default() -> Self {
        Self {
            visible: false,
            reason: Some(BorderHideReason::NoTarget),
            inner: None,
            outer: None,
        }
    }
}

impl WorldEngine {
    /// Read-only border shape for one report ingress: eligibility first,
    /// then the complete bounds. Eligible but uncomputable bounds hide with
    /// no reason (the adapter logs its own geometry veto). A zero border
    /// width is unusable style: the ring hides with no reason, so the sole
    /// reply contract fails closed exactly where the adapter-side
    /// zero-width veto used to fire. A paused (game-lane) report hides as
    /// suspended without computing.
    #[must_use]
    pub(crate) fn presentation_border(ingress: &PresentationIngress) -> BorderPresentation {
        if ingress.paused {
            return BorderPresentation {
                visible: false,
                reason: Some(BorderHideReason::Suspended),
                inner: None,
                outer: None,
            };
        }
        if !ingress.border_enabled {
            return BorderPresentation {
                visible: false,
                reason: Some(BorderHideReason::Disabled),
                inner: None,
                outer: None,
            };
        }
        if ingress.foreground.is_empty() {
            return BorderPresentation {
                visible: false,
                reason: Some(BorderHideReason::NoTarget),
                inner: None,
                outer: None,
            };
        }
        // Fresh-flag hides mirror the eligibility priority (minimized,
        // then fullscreen, then maximized) ahead of the frame read, so a
        // known minimized/maximized/fullscreen foreground without a live
        // frame still hides with its accurate reason instead of no-target.
        // Cloaked/shell/hidden fall through to the shared eligibility
        // below, which derives the same reasons after the verdict.
        if ingress.foreground_minimized {
            return BorderPresentation {
                visible: false,
                reason: Some(BorderHideReason::Minimized),
                inner: None,
                outer: None,
            };
        }
        if ingress.foreground_fullscreen {
            return BorderPresentation {
                visible: false,
                reason: Some(BorderHideReason::Fullscreen),
                inner: None,
                outer: None,
            };
        }
        if ingress.foreground_maximized {
            return BorderPresentation {
                visible: false,
                reason: Some(BorderHideReason::Maximized),
                inner: None,
                outer: None,
            };
        }
        let Some(frame) = ingress.foreground_frame else {
            return BorderPresentation {
                visible: false,
                reason: Some(BorderHideReason::NoTarget),
                inner: None,
                outer: None,
            };
        };
        let eligibility = BorderEligibilityFacts {
            enabled: true,
            has_window: true,
            minimized: ingress.foreground_minimized,
            fullscreen: ingress.foreground_fullscreen,
            maximized: ingress.foreground_maximized,
            cloaked: ingress.foreground_cloaked,
            hidden_workspace: ingress.foreground_hidden_workspace,
            shell: ingress.foreground_shell,
        };
        if let Err(reason) = border_eligibility(&eligibility) {
            return BorderPresentation {
                visible: false,
                reason: Some(reason),
                inner: None,
                outer: None,
            };
        }
        // Fail closed on unusable style: a zero border width draws no ring,
        // so an otherwise eligible foreground hides with the geometry veto
        // (no reason) instead of publishing a degenerate inner==outer shape.
        if ingress.border_width_px == 0 {
            return BorderPresentation {
                visible: false,
                reason: None,
                inner: None,
                outer: None,
            };
        }
        match border_geometry(frame, ingress.border_gap_px, ingress.border_width_px) {
            Some((inner, outer)) => BorderPresentation {
                visible: true,
                reason: None,
                inner: Some(inner),
                outer: Some(outer),
            },
            None => BorderPresentation {
                visible: false,
                reason: None,
                inner: None,
                outer: None,
            },
        }
    }

    /// Read-only underlay presentation over retained state (`&self` only).
    ///
    /// Resolves the tick-time foreground token from the report ingress
    /// against retained homing, held scopes, outstanding transfers, and the
    /// focused Engine session, then projects the focused leaf's immediate
    /// parent group through the hints-aware projector ordinary plans use
    /// (staged carriers with retained learned minima, never carried
    /// geometry). The movement-only trigger is chord OR a matching
    /// focused-window move from the same ingress; an untriggered tick hides
    /// as idle. An open native gesture does not block: a classified focused
    /// Move is an explicit underlay trigger, and the retired world-command
    /// route likewise resolved through it, failing closed only on session
    /// pending/drag residue and divergence. A paused (game-lane) report
    /// hides as suspended without projecting, so no hidden work runs under
    /// the game.
    ///
    /// Never adopts the foreground into retained focus, admits no windows,
    /// and mutates nothing: no history, revision, mapping, membership,
    /// transfer, hold, or gesture change. A retained focus naming any other
    /// lifetime (or no mapping at all) hides as focus-changed instead of
    /// fabricating a group.
    #[must_use]
    pub(crate) fn presentation_underlay(
        &self,
        ingress: &PresentationIngress,
    ) -> PresentationUnderlay {
        if ingress.paused {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Suspended,
            };
        }
        if !ingress.underlay_enabled {
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
        if ingress.foreground_minimized {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Minimized,
            };
        }
        if ingress.foreground_maximized {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Maximized,
            };
        }
        if ingress.foreground_fullscreen {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Fullscreen,
            };
        }
        let Some(home) = self.core_homing(&foreground) else {
            return PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            };
        };
        if self.tiling.get(&home).copied().unwrap_or(TilingMode::Tiled) != TilingMode::Tiled {
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
        // Hints-aware projection over retained topology: staged carriers
        // (shielded exactly like ordinary plans, with retained learned
        // minima) supply advisory minimums; carried geometry is never
        // consulted and overlays simply never match a retained tiled leaf.
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
            let retained = session
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
        // Host-scaled outer fill in adapter units: the projected union
        // expanded by the gap, border width, and resolved extension
        // from the same ingress. Uncomputable shapes fail closed with the
        // retained revision, never a partial fill.
        let outer = group_underlay_outer_rect_px(
            group.bounds,
            ingress.border_gap_px,
            ingress.border_width_px,
            ingress.underlay_extension_px,
        );
        let Some(outer) = outer else {
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
}

/// One pager window: the member lifetime plus its resolved miniature source
/// rectangle in adapter units (the live fact frame first, then the retained
/// snapshot frame, never a guess).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerWindow {
    pub window: WindowId,
    pub frame: Rect,
}

/// One pager workspace in ring order: the stable core id, its 1-based ordinal
/// and label, view/membership flags, and the ordered paint list (bottom
/// first, so the topmost paints last). Members with no resolved frame
/// contribute nothing, never a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerWorkspace {
    pub id: String,
    pub ordinal: u32,
    pub label: String,
    pub current: bool,
    pub occupied: bool,
    pub contains_focused: bool,
    pub windows: Vec<PagerWindow>,
}

/// One pager screen: the output wire id plus its workspaces in ring order
/// (trailing empty included, like the tray menu pager).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerScreen {
    pub output: String,
    pub workspaces: Vec<PagerWorkspace>,
}

/// Content label for a 1-based pager workspace ordinal. The tray menu pager
/// and the numeric taskbar-pager renderer share this single core choice;
/// the renderer keeps painting the bare ordinal, never a product rename.
#[must_use]
pub fn pager_workspace_label(ordinal: u32) -> String {
    format!("Workspace {ordinal}")
}

impl WorldEngine {
    /// Retain one report of pager presentation facts (stage 5): the
    /// presentation native order plus the per-window frame sources (fresh
    /// live frames, retained float slots, retained tile slots) from the
    /// normalized ingress. Presentation-only: touches nothing but the
    /// `presentation_*` facts, never mapping, views, membership, history,
    /// sessions, transfers, or frames policy.
    ///
    /// Retention, top-to-bottom:
    /// 1. native-ordered lifetimes still in the roster take the fresh
    ///    ingress sequence, so the active stack is always exact;
    /// 2. still-rostered but currently absent lifetimes (hidden, minimized)
    ///    keep their previous relative order;
    /// 3. roster lifetimes in neither (never observed) fall back to
    ///    deterministic `WindowId` order.
    ///
    /// The roster is current membership plus the fresh ingress order and
    /// frame rows. A newly verified client admitted by the same report
    /// still paints its live frame on the first publish with no second
    /// observation, while the content below filters to actual membership,
    /// so unadmitted candidates never render publicly. The next report
    /// prunes candidates that never joined and are now absent. Departed
    /// identities (no longer members and absent from the ingress order and
    /// frame rows) prune, so a recycled lifetime never inherits a stale
    /// slot. Duplicate ingress entries keep their first occurrence.
    ///
    /// All three frame maps rebuild wholesale per report (never accumulate)
    /// from the normalized ingress rows, mirroring the retired live >
    /// float > tile fallback over the same sources: `WindowFact.rect` is
    /// never pager truth (fact frames are normalized for homing/overlay
    /// policy, e.g. a retained overlay row carries its tile slot while the
    /// fresh native frame rides the live source). The content resolves live
    /// first, then float, then tile. Members with no resolved source render
    /// nothing, never a guess.
    pub(crate) fn retain_pager_presentation(&mut self, report: &WorldReport) {
        let ingress = &report.presentation;
        let mut roster: BTreeSet<WindowId> = self.members.keys().cloned().collect();
        roster.extend(ingress.pager_order.iter().cloned());
        roster.extend(ingress.pager_frames.iter().map(|row| row.window.clone()));
        let mut seen: BTreeSet<WindowId> = BTreeSet::new();
        let mut next: Vec<WindowId> = Vec::with_capacity(roster.len());
        for window in &ingress.pager_order {
            if roster.contains(window) && seen.insert(window.clone()) {
                next.push(window.clone());
            }
        }
        for window in &self.presentation_order.clone() {
            if roster.contains(window) && seen.insert(window.clone()) {
                next.push(window.clone());
            }
        }
        // `roster` iterates in `WindowId` order, and every lifetime already
        // in `seen` is skipped, so the remainder is the deterministic
        // never-observed fallback.
        for window in &roster {
            if seen.insert(window.clone()) {
                next.push(window.clone());
            }
        }
        self.presentation_order = next;
        // Frame sources rebuild wholesale per report from the normalized
        // ingress rows (first row wins per window), pruned to the roster so
        // retired slots never linger.
        let mut live: BTreeMap<WindowId, Rect> = BTreeMap::new();
        let mut floats: BTreeMap<WindowId, Rect> = BTreeMap::new();
        let mut tiles: BTreeMap<WindowId, Rect> = BTreeMap::new();
        for row in &ingress.pager_frames {
            if !roster.contains(&row.window) {
                continue;
            }
            if let Some(rect) = row.live {
                live.entry(row.window.clone()).or_insert(rect);
            }
            if let Some(rect) = row.float_frame {
                floats.entry(row.window.clone()).or_insert(rect);
            }
            if let Some(rect) = row.tile_frame {
                tiles.entry(row.window.clone()).or_insert(rect);
            }
        }
        self.presentation_live = live;
        self.presentation_float = floats;
        self.presentation_tile = tiles;
        // Pager focus: the adapter-observed focused lifetime, retained per
        // accepted report. The content below gates it on retained homing, so
        // unmanaged or departed lifetimes simply mark no workspace. Empty
        // means no focus.
        self.presentation_focused = if report.focused_global.is_empty() {
            None
        } else {
            Some(WindowId(report.focused_global.clone()))
        };
    }

    /// Resolve one member's pager source rectangle: its live frame first,
    /// then its retained float frame, then its retained tile frame.
    /// Unadmitted lifetimes resolve to nothing even when a frame is
    /// retained for them; `None` renders no miniature, never a guess.
    #[must_use]
    fn pager_frame(&self, window: &WindowId) -> Option<Rect> {
        if !self.members.contains_key(window) {
            return None;
        }
        self.presentation_live
            .get(window)
            .or_else(|| self.presentation_float.get(window))
            .or_else(|| self.presentation_tile.get(window))
            .copied()
    }

    /// Read-only pager content over retained state (`&self` only):
    /// per-screen ordered workspace ids with labels, current/occupied/
    /// contains-focused flags, and per-workspace ordered paint lists with
    /// retained miniature source rectangles. A paused (game-lane) report
    /// publishes no screens, so no ordering work runs under the game. Never
    /// admits, never mutates.
    #[must_use]
    fn presentation_pager(&self, paused: bool) -> Vec<PagerScreen> {
        if paused {
            return Vec::new();
        }
        let focused_home = self
            .presentation_focused
            .as_ref()
            .and_then(|window| self.adapter_homing(window));
        let mut screens = Vec::new();
        for output in self.logical_outputs() {
            let ring = self.logical_ring(&output);
            let view = self.logical_view(&output);
            // Effective current workspace: the retained view when it names a
            // live ring member, else the ring head. This preserves the
            // long-standing first-cell-active paint when no view is retained
            // yet; content policy stays core-owned either way.
            let current_id = view
                .filter(|view| ring.iter().any(|id| id == view))
                .or_else(|| ring.first().cloned());
            let order: BTreeSet<&WindowId> = self.presentation_order.iter().collect();
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
                    self.presentation_order
                        .iter()
                        .filter(|window| homed.contains(window))
                        .rev()
                        .cloned(),
                );
                let mut windows = Vec::new();
                for window in ordered {
                    if let Some(frame) = self.pager_frame(&window) {
                        windows.push(PagerWindow { window, frame });
                    }
                }
                let ordinal = index as u32 + 1;
                let current = current_id.as_deref() == Some(id.as_str());
                workspaces.push(PagerWorkspace {
                    id: id.clone(),
                    ordinal,
                    label: pager_workspace_label(ordinal),
                    current,
                    occupied,
                    // Same-report focus/view epoch: the retained focus names
                    // the pre-command foreground, but the views above already
                    // moved. Only the current workspace may carry the mark;
                    // a departed workspace never shows focused. No fallback
                    // is invented: a stale or foreign focus simply marks
                    // nothing until native observation catches up.
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
}

impl WorldEngine {
    /// Compute the uniform per-reply presentation for one accepted report:
    /// the tick-time foreground token plus the border shape, the underlay
    /// shape, and the pager content. Read-only over retained state; the
    /// caller retains pager facts first (see
    /// [`WorldEngine::retain_pager_presentation`]) and stores the result on
    /// the reply.
    #[must_use]
    pub(crate) fn presentation_for_publish(&self, report: &WorldReport) -> WorldPresentation {
        let ingress = &report.presentation;
        WorldPresentation {
            foreground: ingress.foreground.clone(),
            border: Self::presentation_border(ingress),
            underlay: self.presentation_underlay(ingress),
            pager: self.presentation_pager(ingress.paused),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directional::OutputId;
    use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
    use crate::size_hints::WindowSizeHints;
    use crate::world_engine::{WorldEngine, WorldOutcome};
    use crate::world_report::{
        PresentationIngress, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand,
        WorldReply, WorldReport,
    };

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn screen(output: &str, x: i32) -> ScreenReport {
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(rect(x, 0, 800, 600)),
            work_area: Some(rect(x, 0, 800, 560)),
            gap: 0,
            outer_gap: 0,
            complete: true,
            primary: x == 0,
        }
    }

    fn fact(window: &str, output: &str, x: i32) -> WindowFact {
        WindowFact {
            window: WindowId(window.to_owned()),
            output: OutputId(output.to_owned()),
            rect: Some(rect(x, 0, 40, 90)),
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
            minimized: false,
            observed_workspace: None,
            native_workspace: None,
        }
    }

    fn ingress(token: &str) -> PresentationIngress {
        PresentationIngress {
            foreground: token.to_owned(),
            foreground_frame: if token.is_empty() {
                None
            } else {
                Some(rect(0, 0, 40, 90))
            },
            border_enabled: true,
            underlay_enabled: true,
            border_gap_px: 2,
            border_width_px: 4,
            underlay_extension_px: 3,
            chord_held: true,
            ..PresentationIngress::disabled()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn report(
        request_seq: u64,
        observation_seq: u64,
        windows: Vec<WindowFact>,
        focused: &str,
        command_seq: Option<u64>,
        command: Option<WorldCommand>,
        gesture: Option<GestureIngress>,
        ingress: PresentationIngress,
    ) -> WorldReport {
        WorldReport {
            contract_version: WORLD_CONTRACT_VERSION,
            owner: "owner-a".to_owned(),
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq,
            request_seq,
            request_id: format!("req-{request_seq}"),
            command_seq,
            command,
            workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
            screens: vec![screen("out-a", 0), screen("out-b", 800)],
            windows,
            views: Vec::new(),
            closed: vec![],
            host_owned_workspace_domains: false,
            equal_workspace_count: false,
            native: Vec::new(),
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space_result: crate::admission::NoSpaceResult::default(),
            switch_target: crate::cursor_targeting::SwitchTarget::default(),
            fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
            focus_follows_cursor: false,
            cursor_warp: false,
            cursor: None,
            cursor_hit: None,
            focused_global: focused.to_owned(),
            action_foreground: None,
            action_cached: None,
            action_outputs: Vec::new(),
            default_tiled: true,
            inventory_complete: true,
            gesture_barrier: false,
            gesture,
            monotonic_ms: 0,
            frames: Vec::new(),
            member_attempts: Vec::new(),
            view_attempts: Vec::new(),
            clear_attempts: Vec::new(),
            creation_attempts: Vec::new(),
            presentation: ingress,
        }
    }

    fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> WorldReply {
        match engine.handle_report(report) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    fn underlay_of(reply: &WorldReply) -> &PresentationUnderlay {
        &reply.presentation.underlay
    }

    fn admitted_pair() -> (WorldEngine, DomainKey) {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        // A solo window on out-b keeps the second output homed so sends
        // have a live target domain.
        let members = vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-b", 800),
        ];
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                members.clone(),
                "win-a",
                None,
                None,
                None,
                ingress("win-a"),
            ),
        );
        let view_a = first
            .outputs
            .iter()
            .find(|row| row.output.0 == "out-a")
            .and_then(|row| row.logical_view.clone())
            .expect("view a");
        // A second identical report lets managed focus latch behind the
        // newcomer before any content reads retained focus.
        accepted(
            &mut engine,
            &report(2, 2, members, "win-a", None, None, None, ingress("win-a")),
        );
        let home = DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: crate::directional::WorkspaceId(view_a),
        };
        (engine, home)
    }

    fn session_base(engine: &WorldEngine, home: &DomainKey) -> u64 {
        engine
            .engine()
            .session(home)
            .map(|session| session.accepted_revision())
            .expect("session exists")
    }

    #[test]
    fn underlay_resolves_focused_pair_union() {
        let (mut engine, home) = admitted_pair();
        let base = session_base(&engine, &home);
        let reply = accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                ingress("win-a"),
            ),
        );
        match underlay_of(&reply) {
            PresentationUnderlay::Found(found) => {
                assert_eq!(found.home, home);
                assert_eq!(found.base_revision, base);
                assert_eq!(found.members.len(), 2);
                // Hint-free fixtures project the plain share split: the
                // union covers both members exactly.
                let mut min_x = i32::MAX;
                let mut min_y = i32::MAX;
                let mut max_r = i32::MIN;
                let mut max_b = i32::MIN;
                for member in &found.members {
                    min_x = min_x.min(member.rect.x);
                    min_y = min_y.min(member.rect.y);
                    max_r = max_r.max(member.rect.x + member.rect.w);
                    max_b = max_b.max(member.rect.y + member.rect.h);
                }
                assert_eq!(
                    found.union_rect,
                    Rect {
                        x: min_x,
                        y: min_y,
                        w: max_r - min_x,
                        h: max_b - min_y,
                    }
                );
                assert!(found.union_rect.w > 0 && found.union_rect.h > 0);
                // The outer fill expands the union by gap + width +
                // extension from the same ingress.
                assert_eq!(
                    found.outer_rect,
                    Rect {
                        x: found.union_rect.x - 9,
                        y: found.union_rect.y - 9,
                        w: found.union_rect.w + 18,
                        h: found.union_rect.h + 18,
                    }
                );
                // The focused leaf maps back to the queried token.
                let focused_window = found
                    .members
                    .iter()
                    .find(|member| member.leaf == found.focused_leaf)
                    .map(|member| member.window.clone());
                assert_eq!(focused_window, Some(WindowId("win-a".to_owned())));
                assert_eq!(found.focused_window, WindowId("win-a".to_owned()));
                assert_eq!(reply.presentation.foreground, "win-a");
                // Budget: the structural estimate bounds the exact canonical
                // encoding with a populated underlay.
                let encoded = crate::world_report::encode_reply_canonical(&reply);
                let estimate = crate::world_report::reply_estimate(&reply);
                assert!(
                    estimate >= encoded.len(),
                    "estimate {estimate} bounds wire {}",
                    encoded.len()
                );
                assert!(encoded.contains("\"status\":\"found\""));
            }
            hidden => panic!("expected found, got {hidden:?}"),
        }
    }

    #[test]
    fn underlay_flag_matrix_hides_before_topology() {
        let (mut engine, _) = admitted_pair();
        // Fresh-flag hides carry no base revision; reason precedence is
        // flag order, not topology.
        let members = || {
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-b", 800),
            ]
        };
        let mut seq = 3u64;
        let mut run = |styled: PresentationIngress| {
            seq += 1;
            let reply = accepted(
                &mut engine,
                &report(seq, seq, members(), "win-a", None, None, None, styled),
            );
            reply.presentation.underlay.clone()
        };
        // Disabled underlay hides without consulting topology.
        assert_eq!(
            run(PresentationIngress {
                underlay_enabled: false,
                ..ingress("win-a")
            }),
            PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Disabled,
            }
        );
        // Empty foreground never resolves.
        assert_eq!(
            run(ingress("")),
            PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::NoGroup,
            }
        );
        // An untriggered tick hides as idle without consulting topology.
        assert_eq!(
            run(PresentationIngress {
                chord_held: false,
                ..ingress("win-a")
            }),
            PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Idle,
            }
        );
        for (mut styled, want) in [
            (
                PresentationIngress {
                    foreground_minimized: true,
                    ..ingress("win-a")
                },
                GroupHideReason::Minimized,
            ),
            (
                PresentationIngress {
                    foreground_maximized: true,
                    ..ingress("win-a")
                },
                GroupHideReason::Maximized,
            ),
            (
                PresentationIngress {
                    foreground_fullscreen: true,
                    ..ingress("win-a")
                },
                GroupHideReason::Fullscreen,
            ),
        ] {
            styled.chord_held = true;
            assert_eq!(
                run(styled),
                PresentationUnderlay::Hidden {
                    base_revision: None,
                    reason: want,
                }
            );
        }
        // Unknown lifetimes never resolve: unmanaged foregrounds float.
        assert_eq!(
            run(ingress("ghost")),
            PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            }
        );
        // The game lane suspends without projecting.
        assert_eq!(
            run(PresentationIngress {
                paused: true,
                ..ingress("win-a")
            }),
            PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Suspended,
            }
        );
    }

    #[test]
    fn underlay_never_fabricates_on_focus_mismatch() {
        let (mut engine, home) = admitted_pair();
        let base = session_base(&engine, &home);
        let members = || {
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-b", 800),
            ]
        };
        // Retained focus names win-a: querying win-b hides instead of
        // adopting the fresh token, and a later win-a query still resolves,
        // proving the mismatch adopted nothing.
        let reply = accepted(
            &mut engine,
            &report(3, 3, members(), "win-a", None, None, None, ingress("win-b")),
        );
        assert_eq!(
            *underlay_of(&reply),
            PresentationUnderlay::Hidden {
                base_revision: Some(base),
                reason: GroupHideReason::FocusChanged,
            }
        );
        let reply = accepted(
            &mut engine,
            &report(4, 4, members(), "win-a", None, None, None, ingress("win-a")),
        );
        assert!(matches!(
            underlay_of(&reply),
            PresentationUnderlay::Found(_)
        ));
    }

    #[test]
    fn presentation_only_ingress_change_is_accepted_and_recomputed() {
        // Chord-only and frame-only ingress changes ride a fresh request
        // with an unchanged observation sequence: the digest separates them
        // from the previous report, so the core accepts and recomputes the
        // presentation instead of replaying the cached reply (duplicate) or
        // refusing a reused sequence.
        let (mut engine, _) = admitted_pair();
        let members = || {
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-b", 800),
            ]
        };
        let first = accepted(
            &mut engine,
            &report(3, 2, members(), "win-a", None, None, None, ingress("win-a")),
        );
        assert!(matches!(
            underlay_of(&first),
            PresentationUnderlay::Found(_)
        ));
        // Chord-only flip, same observation sequence, fresh request: the
        // untriggered tick hides as idle without touching topology.
        let second = accepted(
            &mut engine,
            &report(
                4,
                2,
                members(),
                "win-a",
                None,
                None,
                None,
                PresentationIngress {
                    chord_held: false,
                    ..ingress("win-a")
                },
            ),
        );
        assert_ne!(first.state_revision, second.state_revision);
        assert_eq!(
            *underlay_of(&second),
            PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Idle,
            }
        );
        // Frame-only change, same observation sequence, fresh request: the
        // border recomputes from the new frame while the underlay resolves
        // again from unchanged topology.
        let third = accepted(
            &mut engine,
            &report(
                5,
                2,
                members(),
                "win-a",
                None,
                None,
                None,
                PresentationIngress {
                    foreground_frame: Some(rect(5, 5, 40, 90)),
                    ..ingress("win-a")
                },
            ),
        );
        assert!(matches!(
            underlay_of(&third),
            PresentationUnderlay::Found(_)
        ));
        // Gap 2 expands the new frame: inner (3,3,44,94), proving the shape
        // was recomputed from this report's frame, not replayed.
        assert_eq!(
            third.presentation.border.inner,
            Some(Rect {
                x: 3,
                y: 3,
                w: 44,
                h: 94
            })
        );
    }

    #[test]
    fn duplicate_report_replays_identical_presentation() {
        let (mut engine, _) = admitted_pair();
        let members = vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-b", 800),
        ];
        let first = accepted(
            &mut engine,
            &report(
                3,
                3,
                members.clone(),
                "win-a",
                None,
                None,
                None,
                ingress("win-a"),
            ),
        );
        let revision = first.state_revision;
        match engine.handle_report(&report(
            3,
            3,
            members,
            "win-a",
            None,
            None,
            None,
            ingress("win-a"),
        )) {
            WorldOutcome::Duplicate(cached) => {
                assert_eq!(cached.state_revision, revision);
                assert_eq!(cached.presentation, first.presentation);
            }
            other => panic!("expected duplicate, got {other:?}"),
        }
    }

    #[test]
    fn underlay_hides_on_outstanding_transfer() {
        let (mut engine, home) = admitted_pair();
        // Same-output send to the trailing ring workspace: the empty
        // target seeds without a session while the mover flight stays
        // outstanding.
        let ring: Vec<String> = engine.logical_ring("out-a").into_iter().collect();
        assert!(ring.len() > 1, "trailing workspace exists: {ring:?}");
        let target = ring[1].clone();
        accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                Some(11),
                Some(WorldCommand::Send {
                    window: WindowId("win-a".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: target,
                    follow: false,
                }),
                None,
                ingress("win-a"),
            ),
        );
        assert!(!engine.retained_transfers().is_empty());
        let base = session_base(&engine, &home);
        let reply = accepted(
            &mut engine,
            &report(
                4,
                4,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                ingress("win-a"),
            ),
        );
        assert_eq!(
            *underlay_of(&reply),
            PresentationUnderlay::Hidden {
                base_revision: Some(base),
                reason: GroupHideReason::Pending,
            }
        );
    }

    #[test]
    fn underlay_resolves_during_open_move_gesture() {
        // Regression: an open native Move never blocks the highlight on its
        // own - only session pending/drag residue and divergence fail
        // closed. A classified focused Move is an explicit underlay trigger,
        // so the presentation keeps resolving the source projected group
        // while the Move is open and the genuine foreground still matches
        // retained focus.
        let (mut engine, home) = admitted_pair();
        let baseline = accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                PresentationIngress {
                    move_window: "win-a".to_owned(),
                    chord_held: false,
                    ..ingress("win-a")
                },
            ),
        );
        let PresentationUnderlay::Found(baseline_found) = baseline.presentation.underlay.clone()
        else {
            panic!(
                "expected baseline found, got {:?}",
                baseline.presentation.underlay
            );
        };
        let start = rect(0, 0, 40, 90);
        let make_ingress = |phase, cursor| GestureIngress {
            id: 1,
            window: WindowId("win-a".to_owned()),
            kind: GestureKind::Move,
            cursor,
            start_cursor: Some((10, 10)),
            phase,
            esc: false,
            modal_move: true,
            start_frame: start,
        };
        // Normalized per-wake Started then Updated sequence on the focused
        // window; the Updated cursor moved past the down point.
        for (seq, phase, cursor) in [
            (4, GesturePhase::Started, Some((10, 10))),
            (5, GesturePhase::Updated, Some((150, 50))),
        ] {
            let reply = accepted(
                &mut engine,
                &report(
                    seq,
                    seq,
                    vec![
                        fact("win-a", "out-a", 0),
                        fact("win-b", "out-a", 100),
                        fact("win-c", "out-b", 800),
                    ],
                    "win-a",
                    None,
                    None,
                    Some(make_ingress(phase, cursor)),
                    PresentationIngress {
                        move_window: "win-a".to_owned(),
                        chord_held: false,
                        ..ingress("win-a")
                    },
                ),
            );
            match underlay_of(&reply) {
                PresentationUnderlay::Found(found) => {
                    assert_eq!(found.home, home);
                    assert_eq!(found.union_rect, baseline_found.union_rect);
                    assert_eq!(found.members, baseline_found.members);
                    assert_eq!(found.base_revision, baseline_found.base_revision);
                }
                hidden => panic!("open Move must keep resolving, got {hidden:?}"),
            }
        }
    }

    #[test]
    fn underlay_ignores_unrelated_retained_gesture() {
        // A retained gesture on another lifetime neither blocks the focused
        // highlight nor fabricates one for the gestured window: the
        // presentation follows retained focus exactly.
        let (mut engine, home) = admitted_pair();
        let start = rect(100, 0, 40, 90);
        accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                Some(GestureIngress {
                    id: 1,
                    window: WindowId("win-b".to_owned()),
                    kind: GestureKind::Move,
                    cursor: Some((110, 10)),
                    start_cursor: Some((110, 10)),
                    phase: GesturePhase::Started,
                    esc: false,
                    modal_move: true,
                    start_frame: start,
                }),
                ingress("win-a"),
            ),
        );
        let base = session_base(&engine, &home);
        let reply = accepted(
            &mut engine,
            &report(
                4,
                4,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                Some(GestureIngress {
                    id: 1,
                    window: WindowId("win-b".to_owned()),
                    kind: GestureKind::Move,
                    cursor: Some((110, 10)),
                    start_cursor: Some((110, 10)),
                    phase: GesturePhase::Started,
                    esc: false,
                    modal_move: true,
                    start_frame: start,
                }),
                ingress("win-a"),
            ),
        );
        match underlay_of(&reply) {
            PresentationUnderlay::Found(found) => assert_eq!(found.home, home),
            hidden => panic!("unrelated gesture must not block, got {hidden:?}"),
        }
        // The gestured-but-unfocused window still hides as focus-changed:
        // no fabrication, no adoption.
        let reply = accepted(
            &mut engine,
            &report(
                5,
                5,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                Some(GestureIngress {
                    id: 1,
                    window: WindowId("win-b".to_owned()),
                    kind: GestureKind::Move,
                    cursor: Some((110, 10)),
                    start_cursor: Some((110, 10)),
                    phase: GesturePhase::Started,
                    esc: false,
                    modal_move: true,
                    start_frame: start,
                }),
                ingress("win-b"),
            ),
        );
        assert_eq!(
            *underlay_of(&reply),
            PresentationUnderlay::Hidden {
                base_revision: Some(base),
                reason: GroupHideReason::FocusChanged,
            }
        );
        let reply = accepted(
            &mut engine,
            &report(
                6,
                6,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                ingress("win-a"),
            ),
        );
        assert!(matches!(
            underlay_of(&reply),
            PresentationUnderlay::Found(_)
        ));
    }

    #[test]
    fn underlay_float_and_released_session_hide_without_group() {
        let (mut engine, _) = admitted_pair();
        // A focused float exception has homing but no tiled membership.
        let reply = accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                    WindowFact {
                        floating: true,
                        ..fact("win-u", "out-a", 200)
                    },
                ],
                "win-u",
                None,
                None,
                None,
                PresentationIngress {
                    foreground_frame: Some(rect(200, 0, 40, 90)),
                    ..ingress("win-u")
                },
            ),
        );
        assert_eq!(
            *underlay_of(&reply),
            PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            }
        );
        // Releasing the workspace to floating drops the layout session while
        // membership persists: the presentation hides as floating on the
        // retained mode, never a stale union.
        let home = engine
            .adapter_homing(&WindowId("win-a".to_owned()))
            .map(|(output, workspace)| DomainKey {
                output: OutputId(output),
                workspace: crate::directional::WorkspaceId(workspace),
            })
            .expect("win-a homed");
        let view = home.workspace.0.clone();
        accepted(
            &mut engine,
            &report(
                4,
                4,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                Some(12),
                Some(WorldCommand::SetWorkspaceTiling {
                    output: OutputId("out-a".to_owned()),
                    workspace: view,
                    tiled: false,
                }),
                None,
                ingress("win-a"),
            ),
        );
        assert!(engine.engine().session(&home).is_none());
        let reply = accepted(
            &mut engine,
            &report(
                5,
                5,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                ingress("win-a"),
            ),
        );
        assert_eq!(
            *underlay_of(&reply),
            PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            }
        );
    }

    #[test]
    fn underlay_root_leaf_has_no_parent_group() {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                1,
                vec![fact("solo", "out-a", 0)],
                "solo",
                None,
                None,
                None,
                ingress("solo"),
            ),
        );
        accepted(
            &mut engine,
            &report(
                2,
                2,
                vec![fact("solo", "out-a", 0)],
                "solo",
                None,
                None,
                None,
                ingress("solo"),
            ),
        );
        let reply = accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![fact("solo", "out-a", 0)],
                "solo",
                None,
                None,
                None,
                ingress("solo"),
            ),
        );
        match underlay_of(&reply) {
            PresentationUnderlay::Hidden { reason, .. } => {
                assert_eq!(*reason, GroupHideReason::RootLeaf);
            }
            PresentationUnderlay::Found(found) => panic!("solo leaf must not resolve: {found:?}"),
        }
    }

    #[test]
    fn border_shapes_follow_normalized_facts() {
        let (mut engine, _) = admitted_pair();
        // Eligible with computable bounds: visible with both rects, outer
        // expanding inner by the border width on every side.
        let reply = accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                PresentationIngress {
                    foreground_frame: Some(rect(10, 20, 320, 200)),
                    border_gap_px: 2,
                    border_width_px: 4,
                    ..ingress("win-a")
                },
            ),
        );
        let border = &reply.presentation.border;
        assert!(border.visible);
        assert_eq!(border.reason, None);
        assert_eq!(
            border.inner,
            Some(Rect {
                x: 8,
                y: 18,
                w: 324,
                h: 204
            })
        );
        assert_eq!(
            border.outer,
            Some(Rect {
                x: 4,
                y: 14,
                w: 332,
                h: 212
            })
        );
        // Budget: the structural estimate bounds the exact canonical
        // encoding with a visible border.
        let encoded = crate::world_report::encode_reply_canonical(&reply);
        let estimate = crate::world_report::reply_estimate(&reply);
        assert!(
            estimate >= encoded.len(),
            "estimate {estimate} bounds wire {}",
            encoded.len()
        );
        assert!(encoded.contains("\"visible\":true"));
        // Ineligible: hidden with the eligibility reason, no bounds.
        let reply = accepted(
            &mut engine,
            &report(
                4,
                4,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                PresentationIngress {
                    foreground_fullscreen: true,
                    ..ingress("win-a")
                },
            ),
        );
        let border = &reply.presentation.border;
        assert!(!border.visible);
        assert_eq!(
            border.reason,
            Some(crate::visual::BorderHideReason::Fullscreen)
        );
        assert_eq!(border.inner, None);
        assert_eq!(border.outer, None);
        // Eligible but unreadable: hidden as no-target, never a mislabeled
        // eligibility reason.
        let reply = accepted(
            &mut engine,
            &report(
                5,
                5,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                ingress(""),
            ),
        );
        let border = &reply.presentation.border;
        assert!(!border.visible);
        assert_eq!(
            border.reason,
            Some(crate::visual::BorderHideReason::NoTarget)
        );
        assert_eq!(border.inner, None);
        assert_eq!(border.outer, None);
        // Disabled: hidden with the disabled reason.
        let reply = accepted(
            &mut engine,
            &report(
                6,
                6,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                PresentationIngress {
                    border_enabled: false,
                    ..ingress("win-a")
                },
            ),
        );
        assert_eq!(
            reply.presentation.border.reason,
            Some(crate::visual::BorderHideReason::Disabled)
        );
        // Zero border width is unusable style: an otherwise eligible
        // foreground hides with the geometry veto (no reason), never a
        // degenerate visible inner==outer ring. The wire shape carries an
        // empty reason so a remote consumer hides exactly like the native
        // renderer, which keeps its own zero-width veto as a backstop.
        let reply = accepted(
            &mut engine,
            &report(
                7,
                7,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                PresentationIngress {
                    border_width_px: 0,
                    ..ingress("win-a")
                },
            ),
        );
        let border = &reply.presentation.border;
        assert!(!border.visible);
        assert_eq!(border.reason, None);
        assert_eq!(border.inner, None);
        assert_eq!(border.outer, None);
        let encoded = crate::world_report::encode_reply_canonical(&reply);
        assert!(encoded.contains("\"border\":{\"visible\":false,\"reason\":\"\""));
        // The game lane suspends without computing.
        let reply = accepted(
            &mut engine,
            &report(
                8,
                8,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-b", 800),
                ],
                "win-a",
                None,
                None,
                None,
                PresentationIngress {
                    paused: true,
                    ..ingress("win-a")
                },
            ),
        );
        assert_eq!(
            reply.presentation.border.reason,
            Some(crate::visual::BorderHideReason::Suspended)
        );
        assert!(reply.presentation.pager.is_empty());
    }
}
