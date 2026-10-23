//! Read-only border/group desired presentation (stage 4c).
//!
//! The adapter keeps only native facts (fresh foreground identity, liveness,
//! scope/proof gates, host-scaled style numbers, z-anchor/topmost, drawing,
//! and final freshness vetoes). Eligibility, reasons, and complete bounds
//! live here and in [`crate::visual`]: given fresh genuine foreground facts
//! plus retained state, derive what the renderer should show without running
//! a world command, actuating, or touching history, revision, mapping,
//! membership, or the gesture lifecycle.
//!
//! The group query never adopts the foreground, admits windows, advances the
//! revision, or fabricates a group on retained focus mismatch: a retained
//! focus naming any other lifetime hides as focus-changed. Held scopes,
//! outstanding transfers, and session pending/drag/divergence residue all
//! fail closed. An open native gesture never blocks the highlight on its
//! own — like the retired world-command route, only the Engine session
//! state behind it can — so fast per-loop-wake event following never needs
//! repeated world commands for presentation.

use std::collections::{BTreeMap, BTreeSet};

use crate::active_group::{ActiveGroupMember, describe_active_group_with_hints};
use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::WorldEngine;
use crate::world_report::TilingMode;

/// Fresh native facts for one group-highlight refresh. The token is the
/// genuinely observed foreground lifetime (proof-verified adapter-side);
/// minimized/maximized/fullscreen are its fresh facts. Chord/move/style feed
/// the surrounding trigger/scale steps, never this query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupHighlightFacts {
    pub enabled: bool,
    pub foreground_token: WindowId,
    pub minimized: bool,
    pub maximized: bool,
    pub fullscreen: bool,
}

/// Closed hide-reason vocabulary for the group highlight. Tokens match the
/// retired world-command consumer so log lines are unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    NoGroup,
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
        }
    }
}

/// Resolved group highlight: the immediate-parent group with its projected
/// members and their checked union, in adapter units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupHighlightFound {
    pub base_revision: u64,
    pub home: DomainKey,
    pub group: NodeId,
    pub focused_leaf: NodeId,
    pub members: Vec<ActiveGroupMember>,
    pub union_rect: Rect,
}

/// Read-only desired group presentation: either the resolved highlight or a
/// classified hide with the retained base revision when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupHighlight {
    Found(GroupHighlightFound),
    Hidden {
        base_revision: Option<u64>,
        reason: GroupHideReason,
    },
}

impl WorldEngine {
    /// Read-only group-highlight query over retained state (`&self` only).
    ///
    /// Resolves the fresh foreground token against retained homing, held
    /// scopes, outstanding transfers, and the focused Engine session, then
    /// projects the focused leaf's immediate parent group through the
    /// hints-aware projector ordinary plans use (staged carriers with
    /// retained learned minima, never carried geometry). An open native
    /// gesture does not block: a classified focused Move is an explicit
    /// underlay trigger, and the retired world-command route likewise
    /// resolved through it, failing closed only on session pending/drag
    /// residue and divergence.
    ///
    /// Never adopts the foreground into retained focus, admits no windows,
    /// and mutates nothing: no history, revision, mapping, membership,
    /// transfer, hold, or gesture change. A retained focus naming any other
    /// lifetime (or no mapping at all) hides as focus-changed instead of
    /// fabricating a group.
    #[must_use]
    pub fn group_highlight(&self, facts: &GroupHighlightFacts) -> GroupHighlight {
        if !facts.enabled {
            return GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Disabled,
            };
        }
        if facts.foreground_token.0.is_empty() {
            return GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::NoGroup,
            };
        }
        if facts.minimized {
            return GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Minimized,
            };
        }
        if facts.maximized {
            return GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Maximized,
            };
        }
        if facts.fullscreen {
            return GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Fullscreen,
            };
        }
        let Some(home) = self.core_homing(&facts.foreground_token) else {
            return GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            };
        };
        if self.tiling.get(&home).copied().unwrap_or(TilingMode::Tiled) != TilingMode::Tiled {
            return GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            };
        }
        if self
            .members
            .get(&facts.foreground_token)
            .is_some_and(|state| state.floating)
        {
            return GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            };
        }
        let Some(session) = self.engine.session(&home) else {
            return GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::NoGroup,
            };
        };
        let base = session.accepted_revision();
        let hidden = |reason| GroupHighlight::Hidden {
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
                crate::world_report::TransferKind::Window(window) => {
                    window == &facts.foreground_token
                }
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
            Some(window) if *window == facts.foreground_token => {}
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
        GroupHighlight::Found(GroupHighlightFound {
            base_revision: base,
            home,
            group: group.group,
            focused_leaf: focus_leaf,
            members: group.members,
            union_rect: group.bounds,
        })
    }
}

/// Pager presentation observation (stage 4c): normalized per-tick frames plus
/// the native stacking order, all over opaque member lifetimes. The adapter
/// supplies verified live frames, saved float frames, and saved tile/hide-time
/// frames separately so the core applies the visible-behavior fallback
/// (live, then float, then tile); the query renders no miniature for members
/// with no frame anywhere, never a guess.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PagerPresentationObservation {
    /// Verified live frames per lifetime (visible tiled members, floats,
    /// kept overlays, slotless holds with fresh frames).
    pub live: BTreeMap<WindowId, Rect>,
    /// Saved float frames per lifetime (floats between observations).
    pub float_frames: BTreeMap<WindowId, Rect>,
    /// Saved tile-slot / hide-time snapshots per lifetime (hidden members,
    /// minimized members, slotless holds while hidden).
    pub tile_frames: BTreeMap<WindowId, Rect>,
    /// Native stacking order, top-to-bottom in the single enumeration
    /// sequence (first is topmost). May name first-paint candidates not yet
    /// admitted; unknown lifetimes never admit members.
    pub native_order_top_to_bottom: Vec<WindowId>,
}

/// One pager window: the member lifetime plus its resolved miniature source
/// rectangle in adapter units (live, else float, else tile snapshot).
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
    /// Retain one tick of pager presentation facts (stage 4c): the verified
    /// live frames, the saved float/tile snapshots, and the retained native
    /// stacking order. Presentation-only: touches nothing but the
    /// `presentation_*` facts, never mapping, views, membership, history,
    /// sessions, transfers, or frames policy.
    ///
    /// Retention, top-to-bottom:
    /// 1. native-ordered lifetimes still in the roster take the fresh native
    ///    sequence, so the active stack is always exact;
    /// 2. still-rostered but currently absent lifetimes (hidden, minimized)
    ///    keep their previous relative order;
    /// 3. roster lifetimes in neither (never observed) fall back to
    ///    deterministic `WindowId` order.
    ///
    /// The roster is current membership plus the fresh native order. A
    /// newly verified client admitted by the same report still paints its
    /// retained candidate frame on the first query with no second
    /// observation: frames retain for the whole roster, while the query
    /// filters to actual membership, so unadmitted candidates never render
    /// publicly. The next observation prunes candidates that never joined
    /// and are now absent. Departed identities (no longer members and
    /// absent from the native order) prune, so a recycled lifetime never
    /// inherits a stale slot. Duplicate native entries keep their first
    /// occurrence.
    pub fn observe_pager_presentation(&mut self, obs: PagerPresentationObservation) {
        let mut roster: BTreeSet<WindowId> = self.members.keys().cloned().collect();
        roster.extend(obs.native_order_top_to_bottom.iter().cloned());
        let mut seen: BTreeSet<WindowId> = BTreeSet::new();
        let mut next: Vec<WindowId> = Vec::with_capacity(roster.len());
        for window in &obs.native_order_top_to_bottom {
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
        // Live frames and snapshots refresh wholesale per pass, retained for
        // the whole roster (membership plus the fresh native order) so a
        // candidate verified just before admission still resolves its frame
        // on the first query with no second observation. The public query
        // below filters to actual membership, so unadmitted candidates never
        // appear; recycled identities and closed windows never linger past
        // the observation that drops them from the roster. Hidden members
        // are absent from live by construction and resolve through their
        // saved snapshots.
        self.presentation_live = obs
            .live
            .into_iter()
            .filter(|(window, _)| roster.contains(window))
            .collect();
        self.presentation_float = obs
            .float_frames
            .into_iter()
            .filter(|(window, _)| roster.contains(window))
            .collect();
        self.presentation_tile = obs
            .tile_frames
            .into_iter()
            .filter(|(window, _)| roster.contains(window))
            .collect();
    }

    /// Resolve one member's pager source rectangle: the verified live frame
    /// first, then the saved float frame, then the saved tile/hide-time
    /// snapshot. Unadmitted lifetimes resolve to nothing even when a
    /// candidate frame is retained for them; `None` renders no miniature,
    /// never a guess.
    #[must_use]
    pub fn pager_frame(&self, window: &WindowId) -> Option<Rect> {
        if !self.members.contains_key(window) {
            return None;
        }
        self.presentation_live
            .get(window)
            .or_else(|| self.presentation_float.get(window))
            .or_else(|| self.presentation_tile.get(window))
            .copied()
    }

    /// Read-only pager content query over retained state (`&self` only):
    /// per-screen ordered workspace ids with labels, current/occupied/
    /// contains-focused flags, and per-workspace ordered paint lists with
    /// resolved miniature source rectangles. Never admits, never mutates.
    #[must_use]
    pub fn pager_screens(&self) -> Vec<PagerScreen> {
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
                workspaces.push(PagerWorkspace {
                    id: id.clone(),
                    ordinal,
                    label: pager_workspace_label(ordinal),
                    current: current_id.as_deref() == Some(id.as_str()),
                    occupied,
                    contains_focused: focused_home.as_ref().is_some_and(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directional::OutputId;
    use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
    use crate::size_hints::WindowSizeHints;
    use crate::world_engine::{WorldEngine, WorldOutcome};
    use crate::world_report::{
        ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
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
            native: vec![],
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
            default_tiled: true,
            focused_global: focused.to_owned(),
            action_foreground: None,
            action_cached: None,
            action_outputs: Vec::new(),
            inventory_complete: true,
            gesture_barrier: false,
            gesture,
            feedback: vec![],
            monotonic_ms: 0,
            frames: Vec::new(),
            member_attempts: Vec::new(),
            view_attempts: Vec::new(),
            clear_attempts: Vec::new(),
        }
    }

    fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
        match engine.handle_report(report) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    fn facts(token: &str) -> GroupHighlightFacts {
        GroupHighlightFacts {
            enabled: true,
            foreground_token: WindowId(token.to_owned()),
            minimized: false,
            maximized: false,
            fullscreen: false,
        }
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
            &report(1, 1, members.clone(), "win-a", None, None, None),
        );
        let view_a = first
            .outputs
            .iter()
            .find(|row| row.output.0 == "out-a")
            .and_then(|row| row.logical_view.clone())
            .expect("view a");
        // A second identical report lets managed focus latch behind the
        // newcomer before any query reads retained focus.
        accepted(
            &mut engine,
            &report(2, 2, members, "win-a", None, None, None),
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
    fn group_query_resolves_focused_pair_union() {
        let (engine, home) = admitted_pair();
        let base = session_base(&engine, &home);
        match engine.group_highlight(&facts("win-a")) {
            GroupHighlight::Found(found) => {
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
                // The focused leaf maps back to the queried token.
                let focused_window = found
                    .members
                    .iter()
                    .find(|member| member.leaf == found.focused_leaf)
                    .map(|member| member.window.clone());
                assert_eq!(focused_window, Some(WindowId("win-a".to_owned())));
            }
            hidden => panic!("expected found, got {hidden:?}"),
        }
    }

    #[test]
    fn group_query_flag_matrix_hides_before_topology() {
        let (engine, _) = admitted_pair();
        // Fresh-flag hides never consult retained state, so they carry no
        // base revision; reason precedence is flag order, not topology.
        for (highlight_facts, want) in [
            (
                GroupHighlightFacts {
                    enabled: false,
                    ..facts("win-a")
                },
                GroupHideReason::Disabled,
            ),
            (
                GroupHighlightFacts {
                    foreground_token: WindowId(String::new()),
                    ..facts("win-a")
                },
                GroupHideReason::NoGroup,
            ),
            (
                GroupHighlightFacts {
                    minimized: true,
                    ..facts("win-a")
                },
                GroupHideReason::Minimized,
            ),
            (
                GroupHighlightFacts {
                    maximized: true,
                    ..facts("win-a")
                },
                GroupHideReason::Maximized,
            ),
            (
                GroupHighlightFacts {
                    fullscreen: true,
                    ..facts("win-a")
                },
                GroupHideReason::Fullscreen,
            ),
        ] {
            assert_eq!(
                engine.group_highlight(&highlight_facts),
                GroupHighlight::Hidden {
                    base_revision: None,
                    reason: want,
                }
            );
        }
        // Unknown lifetimes never resolve: unmanaged foregrounds float.
        assert_eq!(
            engine.group_highlight(&facts("ghost")),
            GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            }
        );
    }

    #[test]
    fn group_query_never_fabricates_on_focus_mismatch() {
        let (engine, home) = admitted_pair();
        let base = session_base(&engine, &home);
        // Retained focus names win-a: querying win-b hides instead of
        // adopting the fresh token, and a later win-a query still resolves,
        // proving the mismatch adopted nothing.
        assert_eq!(
            engine.group_highlight(&facts("win-b")),
            GroupHighlight::Hidden {
                base_revision: Some(base),
                reason: GroupHideReason::FocusChanged,
            }
        );
        assert!(matches!(
            engine.group_highlight(&facts("win-a")),
            GroupHighlight::Found(_)
        ));
    }

    #[test]
    fn group_query_never_mutates_retained_state() {
        let (engine, _) = admitted_pair();
        let before_revision = engine.state_revision();
        let before_members = engine.logical_members_all();
        let before_outputs = engine.logical_outputs();
        for token in ["win-a", "win-b", "ghost", ""] {
            let _ = engine.group_highlight(&facts(token));
        }
        assert_eq!(engine.state_revision(), before_revision);
        assert_eq!(engine.logical_members_all(), before_members);
        assert_eq!(engine.logical_outputs(), before_outputs);
    }

    #[test]
    fn group_query_hides_on_outstanding_transfer() {
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
            ),
        );
        assert!(!engine.retained_transfers().is_empty());
        let base = session_base(&engine, &home);
        assert_eq!(
            engine.group_highlight(&facts("win-a")),
            GroupHighlight::Hidden {
                base_revision: Some(base),
                reason: GroupHideReason::Pending,
            }
        );
    }

    #[test]
    fn group_query_resolves_during_open_move_gesture() {
        // Regression: the retired world-command route (`run_local_operation`
        // for `CoreCommand::ActiveGroup`) never consulted the retained stage
        // 4a gesture session — only session pending/drag residue and
        // divergence fail closed. A classified focused native Move is an
        // explicit underlay trigger, so the query must keep resolving the
        // source projected group while the Move is open and the genuine
        // foreground still matches retained focus.
        let (mut engine, home) = admitted_pair();
        let baseline = match engine.group_highlight(&facts("win-a")) {
            GroupHighlight::Found(found) => found,
            hidden => panic!("expected baseline found, got {hidden:?}"),
        };
        let start = rect(0, 0, 40, 90);
        let ingress = |phase, cursor| GestureIngress {
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
            (3, GesturePhase::Started, Some((10, 10))),
            (4, GesturePhase::Updated, Some((150, 50))),
        ] {
            accepted(
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
                    Some(ingress(phase, cursor)),
                ),
            );
            match engine.group_highlight(&facts("win-a")) {
                GroupHighlight::Found(found) => {
                    assert_eq!(found.home, home);
                    assert_eq!(found.union_rect, baseline.union_rect);
                    assert_eq!(found.members, baseline.members);
                    assert_eq!(found.base_revision, baseline.base_revision);
                }
                hidden => panic!("open Move must keep resolving, got {hidden:?}"),
            }
        }
    }

    #[test]
    fn group_query_ignores_unrelated_retained_gesture() {
        // A retained gesture on another lifetime neither blocks the focused
        // highlight nor fabricates one for the gestured window: the query
        // follows retained focus exactly, like the retired world-command
        // route, which never consulted the gesture session.
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
            ),
        );
        let base = session_base(&engine, &home);
        match engine.group_highlight(&facts("win-a")) {
            GroupHighlight::Found(found) => assert_eq!(found.home, home),
            hidden => panic!("unrelated gesture must not block, got {hidden:?}"),
        }
        // The gestured-but-unfocused window still hides as focus-changed:
        // no fabrication, no adoption.
        assert_eq!(
            engine.group_highlight(&facts("win-b")),
            GroupHighlight::Hidden {
                base_revision: Some(base),
                reason: GroupHideReason::FocusChanged,
            }
        );
        assert!(matches!(
            engine.group_highlight(&facts("win-a")),
            GroupHighlight::Found(_)
        ));
    }

    #[test]
    fn group_query_float_and_released_session_hide_without_group() {
        let (mut engine, _) = admitted_pair();
        // A focused float exception has homing but no tiled membership.
        accepted(
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
            ),
        );
        assert_eq!(
            engine.group_highlight(&facts("win-u")),
            GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            }
        );
        // Releasing the workspace to floating drops the layout session while
        // membership persists: the query hides as floating on the retained
        // mode, never a stale union.
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
            ),
        );
        assert!(engine.engine().session(&home).is_none());
        assert_eq!(
            engine.group_highlight(&facts("win-a")),
            GroupHighlight::Hidden {
                base_revision: None,
                reason: GroupHideReason::Floating,
            }
        );
    }

    #[test]
    fn group_query_root_leaf_has_no_parent_group() {
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
            ),
        );
        match engine.group_highlight(&facts("solo")) {
            GroupHighlight::Hidden { reason, .. } => {
                assert_eq!(reason, GroupHideReason::RootLeaf);
            }
            GroupHighlight::Found(found) => panic!("solo leaf must not resolve: {found:?}"),
        }
    }
}
