//! Uniform transport-facing world presentation (stage 5).
//!
//! The sole presentation contract a remote consumer (for example a KDE D-Bus
//! script) can use: every accepted report carries the normalized
//! [`PresentationIngressV4`](crate::world_report::PresentationIngressV4) facts and every accepted reply carries the
//! computed [`WorldPresentationV4`](crate::world_report::WorldPresentationV4) shapes and content. Windows consumes only
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

use crate::active_group::ActiveGroupMember;
use crate::directional::{NodeId, WindowId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::visual::{BorderHideReason, BorderPresentation};

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
    /// Explicit trailing-spare designation, same rule as
    /// [`crate::world_report::WorkspaceDesire::trailing`]: true only
    /// for the final ring row when it is the empty unoccupied spare.
    /// The wire omits the key unless true.
    pub trailing: bool,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directional::OutputId;
    use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
    use crate::size_hints::WindowSizeHints;
    use crate::world_engine::{WorldEngine, WorldOutcome};
    use crate::world_report::{
        BorderHideReasonV4, PresentationIngressV4, PresentationRowV4, ScreenReport,
        WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
    };
    use crate::world_sizing_v4::{reply_estimate_v4, reply_exact_bytes_v4};

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn screen(output: &str, x: i32) -> ScreenReport {
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(rect(x, 0, 800, 600)),
            work_area: Some(rect(x, 0, 800, 560)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        }
    }

    fn fact(window: &str, output: &str, x: i32) -> WindowFact {
        WindowFact {
            window: WindowId(window.to_owned()),
            output: Some(OutputId(output.to_owned())),
            rect: Some(rect(x, 0, 40, 90)),
            fullscreen: Some(false),
            maximized: Some(false),
            sticky: Some(false),
            hints: Some(WindowSizeHints::none()),
            minimized: Some(false),
            native_workspace: crate::world_report::NativeWorkspaceFact::Omitted,
            host_hidden: Some(false),
            privileged: Some(false),
            managed: Some(true),
            accepts_input: Some(true),
            activation_inhibited: Some(false),
            tool_window: Some(false),
            owned: Some(false),
            transient: Some(false),
            modal: Some(false),
            resizable: Some(true),
            above: Some(false),
            below: Some(false),
            owner: None,
            parent: None,
            role: None,
            app_id: None,
            executable: Some("synthetic.exe".to_owned()),
            process_id: Some("foreign-pid".to_owned()),
            class: Some("Synthetic".to_owned()),
            title: None,
            window_type: Some(crate::world_report::WindowType::Normal),
            activation_policy: Some(crate::world_report::ActivationPolicy::Regular),
            layer: Some(0),
            fullscreen_owner: None,
            fullscreen_exitable: None,
            suppression_available: None,
            visibility: crate::world_report::VisibilityFact {
                visible: Some(true),
                suppression_owned: None,
            },
            last_write: None,
        }
    }

    fn ingress(token: &str) -> PresentationIngressV4 {
        // v4 ingress carries only foreground/frame/chord/witness lanes;
        // style sizes live in composed settings (contract 261-272).
        PresentationIngressV4 {
            foreground: token.to_owned(),
            // Legacy bools default false via `..disabled()`
            // (known-false); v4 options carry the same basis.
            foreground_minimized: Some(false),
            foreground_maximized: Some(false),
            foreground_fullscreen: Some(false),
            foreground_host_hidden: Some(false),
            foreground_hidden_workspace: Some(false),
            foreground_frame: if token.is_empty() {
                None
            } else {
                Some(rect(0, 0, 40, 90))
            },
            chord_held: true,
            ..PresentationIngressV4::default()
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
        ingress: PresentationIngressV4,
    ) -> WorldReportV4 {
        WorldReportV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq,
            request_seq,
            request_id: format!("req-{request_seq}"),
            command_seq,
            command,
            screens: vec![screen("out-a", 0), screen("out-b", 800)],
            held_screens: Vec::new(),
            windows,
            held_windows: Vec::new(),
            closed: vec![],
            native: Vec::new(),
            held_native: Vec::new(),
            cursor: None,
            cursor_hit: None,
            focused_global: focused.to_owned(),
            foreground_known: !focused.is_empty(),
            action_foreground: None,
            action_cached: None,
            action_outputs: Vec::new(),
            primary_output: None,
            inventory_complete: true,
            gesture_barrier: false,
            gesture,
            monotonic_ms: 0,
            frames: Vec::new(),
            member_attempts: Vec::new(),
            view_attempts: Vec::new(),
            clear_attempts: Vec::new(),
            creation_attempts: Vec::new(),
            held_attempts: Vec::new(),
            state_attempts: Vec::new(),
            focus_feedback: Vec::new(),
            presentation: PresentationRowV4::Valid(ingress),
            intent_records: Vec::new(),
            renderer_input: None,
        }
    }

    fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
        match engine.handle_report(report, &[]).0 {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    fn underlay_of(reply: &WorldReplyV4) -> &PresentationUnderlay {
        &reply.presentation.underlay
    }

    fn admitted_pair() -> (WorldEngine, DomainKey) {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
                // Outer-fill geometry from retired ingress sizes has no v4
                // row (contract 1221 removes gap/width/extension ingress);
                // the settings-composed exact twin is
                // world_presentation_v4_tests::underlay_found_scaled.
                // The focused leaf maps back to the queried token.
                let focused_window = found
                    .members
                    .iter()
                    .find(|member| member.leaf == found.focused_leaf)
                    .map(|member| member.window.clone());
                assert_eq!(focused_window, Some(WindowId("win-a".to_owned())));
                assert_eq!(found.focused_window, WindowId("win-a".to_owned()));
                assert_eq!(reply.presentation.foreground, "win-a");
                // Budget: the structural estimate bounds the v4 exact
                // byte size with a populated underlay. Legacy pinned the
                // canonical wire here; v4 has no full-reply canonical
                // encoder in core (wire lives in the protocol crate),
                // and the matched `Found` arm above is the typed twin of
                // the `"status":"found"` wire pin.
                let estimate = reply_estimate_v4(&reply);
                let exact = reply_exact_bytes_v4(&reply);
                assert!(
                    estimate >= exact,
                    "estimate {estimate} bounds exact {exact}"
                );
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
        let mut run = |styled: PresentationIngressV4| {
            seq += 1;
            let reply = accepted(
                &mut engine,
                &report(seq, seq, members(), "win-a", None, None, None, styled),
            );
            reply.presentation.underlay.clone()
        };
        // Underlay enablement comes from composed settings
        // (`style_bool_v4(FIELD_UNDERLAY_ENABLED, ...)`), covered by
        // world_presentation_v4_tests::shell_and_disabled_hides.
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
            run(PresentationIngressV4 {
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
                PresentationIngressV4 {
                    foreground_minimized: Some(true),
                    ..ingress("win-a")
                },
                GroupHideReason::Minimized,
            ),
            (
                PresentationIngressV4 {
                    foreground_maximized: Some(true),
                    ..ingress("win-a")
                },
                GroupHideReason::Maximized,
            ),
            (
                PresentationIngressV4 {
                    foreground_fullscreen: Some(true),
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
        // Game-lane `paused` has no v4 ingress slot; suspension is
        // truthful-fullscreen only (contract 1225-1228).
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
                PresentationIngressV4 {
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
                PresentationIngressV4 {
                    foreground_frame: Some(rect(5, 5, 40, 90)),
                    ..ingress("win-a")
                },
            ),
        );
        assert!(matches!(
            underlay_of(&third),
            PresentationUnderlay::Found(_)
        ));
        // Inner-rect geometry from retired ingress sizes has no v4 row
        // (contract 1221; R20 anchored border uses outsets/witness per
        // world_presentation_v4_tests::genuine_descriptor_positive).
        assert_eq!(third.presentation.rev, third.state_revision);
        assert_ne!(
            third.presentation.rev, second.presentation.rev,
            "frame-only recompute advances the label, never replays"
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
        match engine
            .handle_report(
                &report(3, 3, members, "win-a", None, None, None, ingress("win-a")),
                &[],
            )
            .0
        {
            WorldOutcome::Duplicate(cached) => {
                assert_eq!(cached.state_revision, revision);
                assert_eq!(cached.presentation, first.presentation);
            }
            other => panic!("expected duplicate, got {other:?}"),
        }
    }

    #[test]
    fn presentation_rev_labels_each_accepted_publication() {
        // Every accepted publication recomputes the presentation, so its
        // issuing label is the reply's own revision: successive accepts
        // advance the label, a fresh same-observation recompute does too,
        // and a duplicate replay replays the cached bytes exactly. The
        // empty bootstrap mirrors the current revision without advancing
        // it and authorizes nothing.
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
            &report(3, 3, members(), "win-a", None, None, None, ingress("win-a")),
        );
        assert_eq!(first.presentation.rev, first.state_revision);
        let second = accepted(
            &mut engine,
            &report(4, 4, members(), "win-a", None, None, None, ingress("win-a")),
        );
        assert_eq!(second.presentation.rev, second.state_revision);
        assert_ne!(
            second.presentation.rev, first.presentation.rev,
            "successive publications label distinct revisions"
        );
        // Fresh same-observation request recomputes (chord flip) with a
        // new label, never a replay.
        let third = accepted(
            &mut engine,
            &report(
                5,
                4,
                members(),
                "win-a",
                None,
                None,
                None,
                PresentationIngressV4 {
                    chord_held: false,
                    ..ingress("win-a")
                },
            ),
        );
        assert_eq!(third.presentation.rev, third.state_revision);
        assert_ne!(
            third.presentation.rev, second.presentation.rev,
            "same-observation recompute advances the label"
        );
        // A duplicate replays the cached presentation exactly: the typed
        // row equality is the v4 twin of the legacy byte-identity pin
        // (v4 has no core canonical presentation encoder; wire lives in
        // the protocol crate).
        match engine
            .handle_report(
                &report(
                    5,
                    4,
                    members(),
                    "win-a",
                    None,
                    None,
                    None,
                    PresentationIngressV4 {
                        chord_held: false,
                        ..ingress("win-a")
                    },
                ),
                &[],
            )
            .0
        {
            WorldOutcome::Duplicate(cached) => {
                assert_eq!(cached.presentation.rev, third.presentation.rev);
                assert_eq!(
                    cached.presentation, third.presentation,
                    "duplicate replays identical presentation"
                );
            }
            other => panic!("expected duplicate, got {other:?}"),
        }
        // Pre-ready bootstrap coverage lives at
        // `world_admit_v4_tests::pre_ready_bootstrap_emits_no_authority`.
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
                PresentationIngressV4 {
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
                    PresentationIngressV4 {
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
                        transient: Some(true),
                        ..fact("win-u", "out-a", 200)
                    },
                ],
                "win-u",
                None,
                None,
                None,
                PresentationIngressV4 {
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
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
        // Ring geometry comes from composed settings + a joined witness
        // (contract 261-272; R20 anchored border, pinned by
        // world_presentation_v4_tests::genuine_descriptor_positive),
        // not ingress sizes.
        //
        // Eligible with computable bounds on the legacy lane; on the v4
        // lane the same report hides with no reason (eligible but
        // unjoined: no witness), which still proves the style-size
        // dependence — the flag-gated reasons below stay exact.
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
                PresentationIngressV4 {
                    foreground_frame: Some(rect(10, 20, 320, 200)),
                    ..ingress("win-a")
                },
            ),
        );
        let border = &reply.presentation.border;
        assert!(!border.visible);
        assert_eq!(border.reason, None);
        // Budget: the structural estimate bounds the v4 exact byte size.
        // (Legacy pinned the canonical wire here with `"visible":true`;
        // v4 has no full-reply canonical encoder in core — wire lives in
        // the protocol crate — so the typed verdict above plus this
        // bound is the faithful twin.)
        let estimate = reply_estimate_v4(&reply);
        let exact = reply_exact_bytes_v4(&reply);
        assert!(
            estimate >= exact,
            "estimate {estimate} bounds exact {exact}"
        );
        // Ineligible: hidden with the eligibility reason.
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
                PresentationIngressV4 {
                    foreground_fullscreen: Some(true),
                    ..ingress("win-a")
                },
            ),
        );
        let border = &reply.presentation.border;
        assert!(!border.visible);
        assert_eq!(border.reason, Some(BorderHideReasonV4::Fullscreen));
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
        assert_eq!(border.reason, Some(BorderHideReasonV4::NoTarget));
        // Disabled/zero-width style cases live on the v4 lane in
        // world_presentation_v4_tests::shell_and_disabled_hides and
        // zero_width_and_missing_scale_hide_empty (typed twin for the
        // zero-width veto is `assert_eq!(border.reason, None)`).
    }
}
