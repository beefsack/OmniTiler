//! World-owned directional policy (stage 5): the ordered refusal lattice,
//! sticky/current-view routing, float-focus candidates/target, constrained
//! half-snap geometry, and maximize-clear need for directional focus/move.
//!
//! Pure over staged [`EngineWindow`](crate::seed::EngineWindow) carriers,
//! screens, and views: no native calls, no Engine mutation. The Windows
//! adapter dispatches [`WorldCommand::FocusDirectional`]/[`MoveDirectional`](crate::world_report::WorldCommand)
//! for every origin (tiled and float alike) and mechanically actuates the
//! returned effect/result; this module owns every logical decision behind
//! those replies. Native exact-subject, input/foreground/view/freshness,
//! lifetime/scope, suspension/elevation, and fullscreen last-moment vetoes
//! stay in the adapter.

use std::collections::BTreeMap;

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::world_report::TilingMode;

/// Ordered directional refusal / authorization verdict. The `Outcome` token
/// is the exact snap vocabulary the adapter logs (e.g.
/// `focus-refused-floating`); the adapter maps it 1:1 with no re-decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectionalVerdict {
    /// Logical refusal: no mutation, no actuation. The adapter logs
    /// `outcome` (with `cause` when present) and settles the intent.
    Refused {
        outcome: DirectionalOutcome,
        cause: Option<String>,
    },
    /// Move subject is maximized (non-fullscreen) on a tiled workspace:
    /// the core binds the original intent and publishes a one-shot
    /// `clear_operation` desire under the originating op; the adapter
    /// dispatches the native clear at most once and reports a correlated
    /// `ClearAttempt`; the commandless core settlement resumes the original
    /// intent on proof. Never a refusal by itself, never a second command.
    NeedClear { window: WindowId },
}

/// Fixed directional outcome vocabulary (snap parity). `as_str` is the exact
/// log token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionalOutcome {
    FocusRefusedFloating,
    MoveRefusedFloating,
    FocusRefusedWorkspaceFloating,
    MoveRefusedWorkspaceFloating,
    MoveRefusedFullscreen,
    FocusRefusedMaximize,
    FocusFloatRetained,
    FocusFloatRefusedStale,
    MoveFloatRefusedStale,
    MoveFloatRefusedInvalidGeometry,
    MoveFloatRefusedConstraints,
    MoveFloatRefusedUnknownOutput,
    FocusFloatApplied,
    MoveFloatApplied,
    NeedClear,
}

impl DirectionalOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FocusRefusedFloating => "focus-refused-floating",
            Self::MoveRefusedFloating => "move-refused-floating",
            Self::FocusRefusedWorkspaceFloating => "focus-refused-workspace-floating",
            Self::MoveRefusedWorkspaceFloating => "move-refused-workspace-floating",
            Self::MoveRefusedFullscreen => "move-refused-fullscreen",
            Self::FocusRefusedMaximize => "focus-refused-maximize",
            Self::FocusFloatRetained => "focus-float-retained",
            Self::FocusFloatRefusedStale => "focus-float-refused-stale",
            Self::MoveFloatRefusedStale => "move-float-refused-stale",
            Self::MoveFloatRefusedInvalidGeometry => "move-float-refused-invalid-geometry",
            Self::MoveFloatRefusedConstraints => "move-float-refused-constraints",
            Self::MoveFloatRefusedUnknownOutput => "move-float-unknown-output",
            Self::FocusFloatApplied => "focus-float-applied",
            Self::MoveFloatApplied => "move-float-applied",
            Self::NeedClear => "move-maximize-clear",
        }
    }
}

/// Effective routing domain for one directional subject: sticky subjects
/// show on every workspace of their output, so they route in the displayed
/// (current) view when homed elsewhere; ordinary windows keep strict
/// home-domain routing. `home` is the core logical homing, `sticky` the
/// staged sticky flag, `view` the current logical view of `home.output`.
#[must_use]
pub fn effective_domain(home: &DomainKey, sticky: bool, view: Option<&str>) -> DomainKey {
    if sticky
        && let Some(active) = view
        && !active.is_empty()
        && active != home.workspace.0
    {
        return DomainKey {
            output: home.output.clone(),
            workspace: crate::directional::WorkspaceId(active.to_owned()),
        };
    }
    home.clone()
}

/// Ordered directional lattice shared by focus and move. `is_move`
/// selects the move vocabulary/fences (fullscreen refusal, maximize
/// need-clear); focus keeps geometry-free allow on fullscreen and the
/// maximized fence instead.
///
/// Order (adapter parity): automatic-float refusal, floating-workspace
/// refusal, fullscreen (move only), maximized fence (focus) / need-clear
/// (move), then float vs tiled routing. Returns `None` when no early
/// verdict applies and the caller must continue (float geometry/target
/// or the tiled engine path).
pub fn early_verdict(
    subject: &EngineWindow,
    tiling: TilingMode,
    is_move: bool,
) -> Option<DirectionalVerdict> {
    let automatic = subject.fixed_auto && !subject.fixed_suppress;
    let float_subject = subject.floating || subject.sticky;
    if automatic && float_subject {
        return Some(DirectionalVerdict::Refused {
            outcome: if is_move {
                DirectionalOutcome::MoveRefusedFloating
            } else {
                DirectionalOutcome::FocusRefusedFloating
            },
            cause: None,
        });
    }
    if !matches!(tiling, TilingMode::Tiled) {
        return Some(DirectionalVerdict::Refused {
            outcome: if is_move {
                DirectionalOutcome::MoveRefusedWorkspaceFloating
            } else {
                DirectionalOutcome::FocusRefusedWorkspaceFloating
            },
            cause: None,
        });
    }
    if is_move && subject.fullscreen {
        return Some(DirectionalVerdict::Refused {
            outcome: DirectionalOutcome::MoveRefusedFullscreen,
            cause: None,
        });
    }
    if subject.maximized && !subject.fullscreen {
        if is_move {
            return Some(DirectionalVerdict::NeedClear {
                window: subject.window.clone(),
            });
        }
        return Some(DirectionalVerdict::Refused {
            outcome: DirectionalOutcome::FocusRefusedMaximize,
            cause: None,
        });
    }
    None
}

/// One same-domain floating/sticky candidate for float-origin directional
/// focus (R-FLT-07..11, KDE `selectFloatFocusTarget` caller parity).
/// `x`/`y` are the observed visible top-left coordinates; `sticky`
/// selects the sticky layer. Encounter order with the subject excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatFocusCandidate {
    pub token: WindowId,
    pub x: i32,
    pub y: i32,
    pub sticky: bool,
}

/// Float-focus candidates from staged carriers: observed floats/stickies
/// in encounter order, subject excluded. Sticky candidates qualify
/// output-wide on the subject's output; ordinary floats keep strict
/// effective-domain membership. Automatic fixed floats never take focus
/// writes and never enter. Tiles never enter.
pub fn float_focus_candidates(
    staged: &[EngineWindow],
    homing: &BTreeMap<WindowId, DomainKey>,
    subject: &WindowId,
    output: &OutputId,
    workspace: &str,
) -> Vec<FloatFocusCandidate> {
    let mut out = Vec::new();
    for window in staged {
        if &window.window == subject {
            continue;
        }
        if !(window.floating || window.sticky) {
            continue;
        }
        if window.fixed_auto && !window.fixed_suppress {
            continue;
        }
        let Some(home) = homing.get(&window.window) else {
            continue;
        };
        let sticky_candidate = window.sticky;
        if sticky_candidate {
            if &home.output != output {
                continue;
            }
        } else {
            if &home.output != output || home.workspace.0 != workspace {
                continue;
            }
            if !window.floating {
                continue;
            }
        }
        out.push(FloatFocusCandidate {
            token: window.window.clone(),
            x: window.rect.x,
            y: window.rect.y,
            sticky: sticky_candidate,
        });
    }
    out
}

/// Float-origin directional focus target (R-FLT-07..11, KDE
/// `selectFloatFocusTarget` parity): strictly same-domain floating/sticky
/// candidates compared by top-left coordinate on the requested axis only.
/// Candidate order is the sticky layer first in encounter order, then
/// ordinary floats in encounter order through one shared distance race.
/// Up/Left admit non-positive deltas (equal included) and keep the first
/// nearest tie; Down/Right require strictly positive deltas and keep the
/// last nearest tie. Returns the winning token, or `None` for a miss or
/// an invalid direction.
#[must_use]
pub fn select_float_focus_target(
    subject_x: i32,
    subject_y: i32,
    direction: &str,
    candidates: &[FloatFocusCandidate],
) -> Option<WindowId> {
    let vertical = match direction {
        "up" | "down" => true,
        "left" | "right" => false,
        _ => return None,
    };
    let negative = matches!(direction, "up" | "left");
    let mut best: Option<&FloatFocusCandidate> = None;
    let mut best_dist: i64 = 0;
    for entry in candidates
        .iter()
        .filter(|c| c.sticky)
        .chain(candidates.iter().filter(|c| !c.sticky))
    {
        let delta: i64 = if vertical {
            i64::from(entry.y) - i64::from(subject_y)
        } else {
            i64::from(entry.x) - i64::from(subject_x)
        };
        if negative {
            if delta > 0 {
                continue;
            }
        } else if delta <= 0 {
            continue;
        }
        let dist = delta.abs();
        if best.is_none() || dist < best_dist {
            best = Some(entry);
            best_dist = dist;
        } else if dist == best_dist && !negative {
            best = Some(entry);
        }
    }
    best.map(|b| b.token.clone())
}

/// COSMIC float half-snap rectangle (stateless first step, R-FLT-10..11,
/// KDE `floatHalfSnapRect` parity): left/top origin `loc + inner`; right
/// `x = loc.x + trunc(W/2) + trunc(inner/2)`; widths
/// `trunc(W/2) - trunc(3*inner/2)`; height `H - 2*inner` (up/down
/// analogous). `bounds` is the work area in adapter units, `inner_gap`
/// the configured inner gap. Every repeated arrow requests the same half
/// again. `None` for invalid/nonpositive input or output rectangles.
#[must_use]
pub fn float_half_snap_rect(bounds: Rect, inner_gap: i32, direction: &str) -> Option<Rect> {
    if !target_rect_valid(bounds) {
        return None;
    }
    if !(0..=64).contains(&inner_gap) {
        return None;
    }
    match direction {
        "left" | "right" | "up" | "down" => {}
        _ => return None,
    }
    let inner = i64::from(inner_gap);
    let (bx, by, w, h) = (
        i64::from(bounds.x),
        i64::from(bounds.y),
        i64::from(bounds.w),
        i64::from(bounds.h),
    );
    let half_w = w / 2;
    let half_h = h / 2;
    let half_inner = inner / 2;
    let three_half_inner = (3 * inner) / 2;
    let rect = match direction {
        "left" => Rect {
            x: bounded_i32(bx + inner)?,
            y: bounded_i32(by + inner)?,
            w: bounded_i32(half_w - three_half_inner)?,
            h: bounded_i32(h - 2 * inner)?,
        },
        "right" => Rect {
            x: bounded_i32(bx + half_w + half_inner)?,
            y: bounded_i32(by + inner)?,
            w: bounded_i32(half_w - three_half_inner)?,
            h: bounded_i32(h - 2 * inner)?,
        },
        "up" => Rect {
            x: bounded_i32(bx + inner)?,
            y: bounded_i32(by + inner)?,
            w: bounded_i32(w - 2 * inner)?,
            h: bounded_i32(half_h - three_half_inner)?,
        },
        _ => Rect {
            x: bounded_i32(bx + inner)?,
            y: bounded_i32(by + half_h + half_inner)?,
            w: bounded_i32(w - 2 * inner)?,
            h: bounded_i32(half_h - three_half_inner)?,
        },
    };
    if !target_rect_valid(rect) {
        return None;
    }
    Some(rect)
}

/// Native target-rectangle validity (KDE `isTargetRect` parity).
#[must_use]
pub const fn target_rect_valid(rect: Rect) -> bool {
    rect.x >= -16384
        && rect.x <= 16384
        && rect.y >= -16384
        && rect.y <= 16384
        && rect.w >= 1
        && rect.h >= 1
        && rect.w <= 16384
        && rect.h <= 16384
}

fn bounded_i32(value: i64) -> Option<i32> {
    if value < i64::from(i32::MIN) || value > i64::from(i32::MAX) {
        None
    } else {
        Some(value as i32)
    }
}

/// Declared-size gate for one computed half-snap rectangle (R-FLT-10..11,
/// KDE `move-float-refused-constraints` parity): an incompatible half
/// never fights the client. True when the half meets every meaningful
/// declared bound; per-axis-zero bounds are unset.
#[must_use]
pub fn half_snap_constraints_ok(half: Rect, hints: crate::size_hints::WindowSizeHints) -> bool {
    if hints.meaningful_min_w().is_some_and(|min| half.w < min) {
        return false;
    }
    if hints.meaningful_min_h().is_some_and(|min| half.h < min) {
        return false;
    }
    if hints.meaningful_max_w().is_some_and(|max| max < half.w) {
        return false;
    }
    if hints.meaningful_max_h().is_some_and(|max| max < half.h) {
        return false;
    }
    true
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::directional::{OutputId, WindowId};
    use crate::geometry::Rect;
    use crate::size_hints::WindowSizeHints;
    use crate::world_clear_tests::desire;
    use crate::world_engine::{WorldEngine, WorldOutcome};
    use crate::world_report::{
        CommandResult, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
    };

    pub(crate) fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn carrier(window: &str, output: &str, workspace: &str, x: i32, y: i32) -> EngineWindow {
        EngineWindow {
            window: WindowId(window.to_owned()),
            output: OutputId(output.to_owned()),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
            rect: rect(x, y, 200, 200),
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        }
    }

    fn float_carrier(window: &str, output: &str, workspace: &str, x: i32, y: i32) -> EngineWindow {
        EngineWindow {
            floating: true,
            ..carrier(window, output, workspace, x, y)
        }
    }

    pub(crate) fn outcome_str(result: &Option<CommandResult>) -> Option<&'static str> {
        match result {
            Some(CommandResult::Directional { outcome, .. }) => Some(outcome.as_str()),
            _ => None,
        }
    }

    #[test]
    fn ordered_refusal_automatic_beats_floating_workspace() {
        // Automatic floats refuse even on a floating workspace: the
        // automatic gate runs first in the ordered lattice.
        let mut subject = float_carrier("win-a", "out-a", "ws-1", 0, 0);
        subject.fixed_auto = true;
        let verdict = early_verdict(&subject, crate::world_report::TilingMode::Floating, true)
            .expect("refuses");
        assert_eq!(
            verdict,
            DirectionalVerdict::Refused {
                outcome: DirectionalOutcome::MoveRefusedFloating,
                cause: None,
            }
        );
        let verdict = early_verdict(&subject, crate::world_report::TilingMode::Floating, false)
            .expect("refuses");
        assert_eq!(
            verdict,
            DirectionalVerdict::Refused {
                outcome: DirectionalOutcome::FocusRefusedFloating,
                cause: None,
            }
        );
    }

    #[test]
    fn ordered_refusal_floating_workspace_fullscreen_maximize() {
        // Floating workspaces refuse both ops; fullscreen refuses move
        // only; maximized fences focus and needs clear for move.
        let tiled = carrier("win-a", "out-a", "ws-1", 0, 0);
        for is_move in [true, false] {
            let verdict = early_verdict(&tiled, crate::world_report::TilingMode::Floating, is_move)
                .expect("refuses");
            assert_eq!(
                verdict,
                DirectionalVerdict::Refused {
                    outcome: if is_move {
                        DirectionalOutcome::MoveRefusedWorkspaceFloating
                    } else {
                        DirectionalOutcome::FocusRefusedWorkspaceFloating
                    },
                    cause: None,
                }
            );
        }
        let mut fullscreen = carrier("win-a", "out-a", "ws-1", 0, 0);
        fullscreen.fullscreen = true;
        assert!(
            early_verdict(&fullscreen, crate::world_report::TilingMode::Tiled, false).is_none()
        );
        assert_eq!(
            early_verdict(&fullscreen, crate::world_report::TilingMode::Tiled, true),
            Some(DirectionalVerdict::Refused {
                outcome: DirectionalOutcome::MoveRefusedFullscreen,
                cause: None,
            })
        );
        // Fullscreen wins over maximize: never needs clear.
        let mut both = carrier("win-a", "out-a", "ws-1", 0, 0);
        both.fullscreen = true;
        both.maximized = true;
        assert_eq!(
            early_verdict(&both, crate::world_report::TilingMode::Tiled, true),
            Some(DirectionalVerdict::Refused {
                outcome: DirectionalOutcome::MoveRefusedFullscreen,
                cause: None,
            })
        );
        let mut maximized = carrier("win-a", "out-a", "ws-1", 0, 0);
        maximized.maximized = true;
        assert_eq!(
            early_verdict(&maximized, crate::world_report::TilingMode::Tiled, false),
            Some(DirectionalVerdict::Refused {
                outcome: DirectionalOutcome::FocusRefusedMaximize,
                cause: None,
            })
        );
        assert_eq!(
            early_verdict(&maximized, crate::world_report::TilingMode::Tiled, true),
            Some(DirectionalVerdict::NeedClear {
                window: WindowId("win-a".to_owned()),
            })
        );
    }

    #[test]
    fn sticky_routes_in_displayed_view_ordinary_keeps_home() {
        let home = DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
        };
        // Sticky homed elsewhere routes in the displayed workspace.
        assert_eq!(
            effective_domain(&home, true, Some("ws-2")),
            DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
            }
        );
        // Sticky already displayed, unknown, or empty views keep home.
        assert_eq!(effective_domain(&home, true, Some("ws-1")), home);
        assert_eq!(effective_domain(&home, true, None), home);
        assert_eq!(effective_domain(&home, true, Some("")), home);
        // Ordinary windows keep strict home-domain routing.
        assert_eq!(effective_domain(&home, false, Some("ws-2")), home);
    }

    #[test]
    fn float_candidates_keep_output_wide_sticky_strict_floats_and_drop_automatic() {
        // Sticky qualifies output-wide; ordinary floats keep strict
        // domain; automatic floats and tiles never enter; the subject
        // is excluded.
        let mut staged = vec![
            float_carrier("sticky-a", "out-a", "ws-9", 10, 10),
            float_carrier("float-a", "out-a", "ws-1", 20, 20),
            float_carrier("float-b", "out-b", "ws-1", 30, 30),
            carrier("tile-a", "out-a", "ws-1", 40, 40),
        ];
        staged[0].sticky = true;
        staged[0].floating = false;
        let mut auto = float_carrier("auto-a", "out-a", "ws-1", 50, 50);
        auto.fixed_auto = true;
        staged.push(auto);
        let mut homing = BTreeMap::new();
        for id in [
            "sticky-a", "float-a", "float-b", "tile-a", "auto-a", "subject",
        ] {
            homing.insert(
                WindowId(id.to_owned()),
                DomainKey {
                    output: OutputId(if id == "float-b" { "out-b" } else { "out-a" }.to_owned()),
                    workspace: crate::directional::WorkspaceId(
                        if id == "sticky-a" { "ws-9" } else { "ws-1" }.to_owned(),
                    ),
                },
            );
        }
        let candidates = float_focus_candidates(
            &staged,
            &homing,
            &WindowId("subject".to_owned()),
            &OutputId("out-a".to_owned()),
            "ws-1",
        );
        let tokens: Vec<&str> = candidates.iter().map(|c| c.token.0.as_str()).collect();
        // sticky-a qualifies output-wide despite ws-9; float-a qualifies
        // strictly; float-b is cross-output and never qualifies (physical
        // output vs membership-home direction preserved).
        assert_eq!(tokens, vec!["sticky-a", "float-a"]);
        assert!(candidates[0].sticky);
        assert!(!candidates[1].sticky);
    }

    #[test]
    fn float_target_selection_races_stickies_first_with_tie_rules() {
        // A nearer ordinary entry wins over a farther sticky one through
        // the shared race; ties keep first for up/left, last for
        // down/right.
        let candidates = vec![
            FloatFocusCandidate {
                token: WindowId("sticky".to_owned()),
                x: 0,
                y: 0,
                sticky: true,
            },
            FloatFocusCandidate {
                token: WindowId("near".to_owned()),
                x: 10,
                y: 0,
                sticky: false,
            },
        ];
        assert_eq!(
            select_float_focus_target(100, 0, "left", &candidates),
            Some(WindowId("near".to_owned()))
        );
        let ties = vec![
            FloatFocusCandidate {
                token: WindowId("first".to_owned()),
                x: 90,
                y: 0,
                sticky: true,
            },
            FloatFocusCandidate {
                token: WindowId("second".to_owned()),
                x: 90,
                y: 0,
                sticky: false,
            },
        ];
        assert_eq!(
            select_float_focus_target(100, 0, "left", &ties),
            Some(WindowId("first".to_owned()))
        );
        assert_eq!(
            select_float_focus_target(80, 0, "right", &ties),
            Some(WindowId("second".to_owned()))
        );
        assert_eq!(select_float_focus_target(100, 0, "sideways", &ties), None);
        assert!(select_float_focus_target(100, 0, "right", &[]).is_none());
    }

    #[test]
    fn half_snap_geometry_matches_stateless_halves_and_bounds() {
        // Bounded constrained halves: first step requests the same half
        // again; the adapter writes it at most once per command.
        let bounds = rect(0, 0, 800, 560);
        assert_eq!(
            float_half_snap_rect(bounds, 8, "left"),
            Some(rect(8, 8, 400 - 12, 560 - 16))
        );
        assert_eq!(
            float_half_snap_rect(bounds, 8, "right"),
            Some(rect(404, 8, 400 - 12, 560 - 16))
        );
        assert_eq!(
            float_half_snap_rect(bounds, 8, "up"),
            Some(rect(8, 8, 800 - 16, 280 - 12))
        );
        assert_eq!(
            float_half_snap_rect(bounds, 8, "down"),
            Some(rect(8, 284, 800 - 16, 280 - 12))
        );
        assert_eq!(float_half_snap_rect(bounds, 65, "left"), None);
        assert_eq!(float_half_snap_rect(bounds, 8, "sideways"), None);
        assert_eq!(float_half_snap_rect(rect(0, 0, 0, 560), 8, "left"), None);
        // Declared sizes gate the write: an incompatible half never
        // fights the client.
        let half = float_half_snap_rect(bounds, 8, "left").expect("half");
        assert!(half_snap_constraints_ok(half, WindowSizeHints::none()));
        let mut oversized = WindowSizeHints::none();
        oversized.min_w = Some(half.w + 1);
        assert!(!half_snap_constraints_ok(half, oversized));
        let mut capped = WindowSizeHints::none();
        capped.max_w = Some(half.w - 1);
        assert!(!half_snap_constraints_ok(half, capped));
    }

    // --- Engine integration: world-owned directional commands. ---

    pub(crate) fn screen(output: &str, x: i32, primary: bool) -> ScreenReport {
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(rect(x, 0, 800, 600)),
            work_area: Some(rect(x, 0, 800, 560)),
            gap: 8,
            outer_gap: 0,
            complete: true,
            primary,
        }
    }

    pub(crate) fn fact(window: &str, output: &str, x: i32) -> WindowFact {
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn report_full(
        request_seq: u64,
        observation_seq: u64,
        screens: Vec<ScreenReport>,
        windows: Vec<WindowFact>,
        views: Vec<(&str, &str)>,
        focused: &str,
        command_seq: Option<u64>,
        command: Option<WorldCommand>,
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
            screens,
            windows,
            views: views
                .into_iter()
                .map(|(output, view)| crate::world_report::ScreenView {
                    output: OutputId(output.to_owned()),
                    view: view.to_owned(),
                })
                .collect(),
            closed: Vec::new(),
            host_owned_workspace_domains: false,
            equal_workspace_count: false,
            // Core regression builders opt into the automatic birth-clear
            // capability explicitly: production adapters carry their own
            // value (KDE true, Windows/macOS false) and these tests pin the
            // enabled lane. Disabled behavior is pinned per-test by
            // overriding this back to false.
            birth_clear: true,
            native: Vec::new(),
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space_result: crate::admission::NoSpaceResult::default(),
            born_maximized: crate::born_maximized::BornMaximized::default(),
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
            gesture: None,
            monotonic_ms: 0,
            frames: Vec::new(),
            member_attempts: Vec::new(),
            view_attempts: Vec::new(),
            clear_attempts: Vec::new(),
            creation_attempts: Vec::new(),
            presentation: crate::world_report::PresentationIngress::disabled(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn report(
        request_seq: u64,
        observation_seq: u64,
        screens: Vec<ScreenReport>,
        windows: Vec<WindowFact>,
        command_seq: Option<u64>,
        command: Option<WorldCommand>,
    ) -> WorldReport {
        report_full(
            request_seq,
            observation_seq,
            screens,
            windows,
            Vec::new(),
            "",
            command_seq,
            command,
        )
    }

    pub(crate) fn accepted(
        engine: &mut WorldEngine,
        report: &WorldReport,
    ) -> crate::world_report::WorldReply {
        match engine.handle_report(report) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    pub(crate) fn float_fact(window: &str, output: &str, x: i32, y: i32) -> WindowFact {
        WindowFact {
            floating: true,
            rect: Some(rect(x, y, 200, 200)),
            ..fact(window, output, x)
        }
    }

    fn bootstrap_pair(engine: &mut WorldEngine) {
        accepted(
            engine,
            &report(
                1,
                1,
                vec![screen("out-a", 0, true)],
                vec![
                    float_fact("win-subject", "out-a", 100, 100),
                    float_fact("win-target", "out-a", 300, 100),
                ],
                None,
                None,
            ),
        );
    }

    fn directional(
        engine: &mut WorldEngine,
        seq: u64,
        command: WorldCommand,
    ) -> crate::world_report::WorldReply {
        accepted(
            engine,
            &report(
                seq,
                seq,
                vec![screen("out-a", 0, true)],
                vec![
                    float_fact("win-subject", "out-a", 100, 100),
                    float_fact("win-target", "out-a", 300, 100),
                ],
                Some(10 + seq),
                Some(command),
            ),
        )
    }

    #[test]
    fn float_focus_hit_emits_effect_and_applied_result() {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        bootstrap_pair(&mut engine);
        let reply = directional(
            &mut engine,
            2,
            WorldCommand::FocusDirectional {
                window: WindowId("win-subject".to_owned()),
                direction: "right".to_owned(),
            },
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("focus-float-applied")
        );
        let target = match &reply.command_result {
            Some(CommandResult::Directional { target, .. }) => target.clone(),
            other => panic!("directional result, got {other:?}"),
        };
        assert_eq!(target, Some(WindowId("win-target".to_owned())));
        assert_eq!(
            reply.effects.len(),
            1,
            "one spent effect, never a duplicate actuation lane"
        );
        assert!(
            reply
                .effects
                .iter()
                .any(|effect| effect.target.0 == "win-target" && effect.is_explicit_focus_for(12)),
            "explicit focus effect for the core-selected target"
        );
    }

    #[test]
    fn float_focus_miss_retains_and_float_move_authorizes_bounded_half() {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        bootstrap_pair(&mut engine);
        // Left of the leftmost float: no candidate, focus retains.
        let reply = directional(
            &mut engine,
            2,
            WorldCommand::FocusDirectional {
                window: WindowId("win-subject".to_owned()),
                direction: "left".to_owned(),
            },
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("focus-float-retained")
        );
        assert!(
            reply.effects.is_empty(),
            "no focus effect on a miss, got {:?}",
            reply.effects
        );
        // Right half-snap authorizes the bounded half in work-area
        // space; the adapter applies it exactly once.
        let reply = directional(
            &mut engine,
            3,
            WorldCommand::MoveDirectional {
                window: WindowId("win-subject".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            },
        );
        let half = match &reply.command_result {
            Some(CommandResult::Directional { outcome, rect, .. }) => {
                assert_eq!(outcome.as_str(), "move-float-applied");
                rect.expect("authorized half")
            }
            other => panic!("directional result, got {other:?}"),
        };
        assert_eq!(half, rect(404, 8, 388, 544));
    }

    #[test]
    fn automatic_float_and_fullscreen_fences_refuse_without_mutation() {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                1,
                vec![screen("out-a", 0, true)],
                vec![
                    WindowFact {
                        floating: true,
                        fixed_auto: true,
                        ..fact("win-auto", "out-a", 100)
                    },
                    WindowFact {
                        floating: true,
                        fullscreen: true,
                        ..fact("win-full", "out-a", 300)
                    },
                ],
                None,
                None,
            ),
        );
        let mut report2 = |seq: u64, command: WorldCommand| {
            accepted(
                &mut engine,
                &report(
                    seq,
                    seq,
                    vec![screen("out-a", 0, true)],
                    vec![
                        WindowFact {
                            floating: true,
                            fixed_auto: true,
                            ..fact("win-auto", "out-a", 100)
                        },
                        WindowFact {
                            floating: true,
                            fullscreen: true,
                            ..fact("win-full", "out-a", 300)
                        },
                    ],
                    Some(10 + seq),
                    Some(command),
                ),
            )
        };
        let reply = report2(
            2,
            WorldCommand::FocusDirectional {
                window: WindowId("win-auto".to_owned()),
                direction: "right".to_owned(),
            },
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("focus-refused-floating")
        );
        let reply = report2(
            3,
            WorldCommand::MoveDirectional {
                window: WindowId("win-auto".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            },
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("move-refused-floating")
        );
        // Fullscreen refuses the move but leaves focus to the float
        // lattice: from the rightmost float there is no target, so
        // focus retains instead of refusing.
        let reply = report2(
            4,
            WorldCommand::MoveDirectional {
                window: WindowId("win-full".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            },
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("move-refused-fullscreen")
        );
        let reply = report2(
            5,
            WorldCommand::FocusDirectional {
                window: WindowId("win-full".to_owned()),
                direction: "right".to_owned(),
            },
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("focus-float-retained")
        );
    }

    #[test]
    fn float_candidates_keep_retained_maximized_overlays() {
        // Overlay parity with the retired adapter leg: retained maximized
        // floats/stickies (never eligible-observed) stay focus targets.
        // The adapter projects them as staged carriers with the maximized
        // flag; the core must not drop them from the candidate set.
        let mut maximized_float = float_carrier("max-float", "out-a", "ws-1", 30, 30);
        maximized_float.maximized = true;
        let mut maximized_sticky = float_carrier("max-sticky", "out-a", "ws-9", 10, 10);
        maximized_sticky.sticky = true;
        maximized_sticky.floating = false;
        maximized_sticky.maximized = true;
        let fullscreen_float = WindowFact {
            floating: true,
            fullscreen: true,
            ..fact("win-full", "out-a", 100)
        };
        let _ = fullscreen_float;
        let staged = vec![
            maximized_sticky,
            maximized_float,
            float_carrier("float-a", "out-a", "ws-1", 20, 20),
        ];
        let mut homing = BTreeMap::new();
        for (id, ws) in [
            ("max-sticky", "ws-9"),
            ("max-float", "ws-1"),
            ("float-a", "ws-1"),
            ("subject", "ws-1"),
        ] {
            homing.insert(
                WindowId(id.to_owned()),
                DomainKey {
                    output: OutputId("out-a".to_owned()),
                    workspace: crate::directional::WorkspaceId(ws.to_owned()),
                },
            );
        }
        let candidates = float_focus_candidates(
            &staged,
            &homing,
            &WindowId("subject".to_owned()),
            &OutputId("out-a".to_owned()),
            "ws-1",
        );
        let tokens: Vec<&str> = candidates.iter().map(|c| c.token.0.as_str()).collect();
        // Sticky overlay qualifies output-wide; the maximized ordinary
        // float keeps its strict domain seat alongside ordinary floats.
        assert_eq!(tokens, vec!["max-sticky", "max-float", "float-a"]);
    }

    #[test]
    fn sticky_half_snap_authorized_through_retained_lane() {
        // Sticky half-snaps ride the same uniform retained float-placement
        // lane: the authorized half publishes under the stable command
        // identity with the desire as its proof-to-come. Ordinary
        // unsolicited sticky frame writes stay prohibited: rows without a
        // `frame_operation` token authorize nothing.
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                1,
                vec![screen("out-a", 0, true)],
                vec![
                    WindowFact {
                        floating: true,
                        sticky: true,
                        ..fact("win-sticky", "out-a", 100)
                    },
                    float_fact("win-target", "out-a", 300, 100),
                ],
                None,
                None,
            ),
        );
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                vec![screen("out-a", 0, true)],
                vec![
                    WindowFact {
                        floating: true,
                        sticky: true,
                        ..fact("win-sticky", "out-a", 100)
                    },
                    float_fact("win-target", "out-a", 300, 100),
                ],
                Some(11),
                Some(WorldCommand::MoveDirectional {
                    window: WindowId("win-sticky".to_owned()),
                    direction: "right".to_owned(),
                    same_axis: crate::directional::SameAxisMove::default(),
                }),
            ),
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("move-float-applied")
        );
        let half = match &reply.command_result {
            Some(CommandResult::Directional { rect, .. }) => *rect,
            other => panic!("directional result, got {other:?}"),
        };
        assert_eq!(half, Some(rect(404, 8, 388, 544)));
        let row = desire(&reply, "win-sticky");
        assert_eq!(row.frame_operation.as_deref(), Some("op-11"));
        assert_eq!(row.effective_frame, half);
        let wire = crate::world_sizing::encode_reply_canonical(&reply);
        assert!(
            wire.contains("\"op\":\"directional\",\"outcome\":\"move-float-applied\""),
            "sticky half-snap round-trips, got {wire}"
        );
    }

    #[test]
    fn sticky_subject_routes_in_displayed_view_end_to_end() {
        // Sticky homed on one workspace but shown on another routes the
        // directional policy in the displayed (current) view: candidates
        // come from the displayed workspace, never the home one.
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                vec![screen("out-a", 0, true)],
                vec![
                    WindowFact {
                        floating: true,
                        sticky: true,
                        ..fact("win-s", "out-a", 100)
                    },
                    float_fact("win-old", "out-a", 50, 100),
                ],
                None,
                None,
            ),
        );
        let mut ids: Vec<String> = first
            .workspaces
            .iter()
            .filter(|row| row.output.0 == "out-a")
            .map(|row| row.id.clone())
            .collect();
        ids.sort();
        assert_eq!(ids.len(), 2, "bootstrap seeds head plus trailing");
        let (head, spare) = (ids[0].clone(), ids[1].clone());
        // Switch the displayed view to the spare workspace.
        accepted(
            &mut engine,
            &report_full(
                2,
                2,
                vec![screen("out-a", 0, true)],
                vec![
                    WindowFact {
                        floating: true,
                        sticky: true,
                        ..fact("win-s", "out-a", 100)
                    },
                    float_fact("win-old", "out-a", 50, 100),
                ],
                vec![("out-a", spare.as_str())],
                "",
                Some(11),
                Some(WorldCommand::SelectWorkspace {
                    output: OutputId("out-a".to_owned()),
                    workspace: spare.clone(),
                }),
            ),
        );
        // A newcomer floats on the displayed spare; the home-workspace
        // float stays behind. Focusing right from the sticky subject must
        // resolve the displayed newcomer, never the home float.
        let reply = accepted(
            &mut engine,
            &report_full(
                3,
                3,
                vec![screen("out-a", 0, true)],
                vec![
                    WindowFact {
                        floating: true,
                        sticky: true,
                        ..fact("win-s", "out-a", 100)
                    },
                    float_fact("win-old", "out-a", 50, 100),
                    float_fact("win-new", "out-a", 300, 100),
                ],
                vec![("out-a", spare.as_str())],
                "",
                Some(12),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-s".to_owned()),
                    direction: "right".to_owned(),
                }),
            ),
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("focus-float-applied"),
            "sticky routes in displayed {spare}, not home {head}"
        );
        let target = match &reply.command_result {
            Some(CommandResult::Directional { target, .. }) => target.clone(),
            other => panic!("directional result, got {other:?}"),
        };
        assert_eq!(target, Some(WindowId("win-new".to_owned())));
        let focus_effects: Vec<_> = reply
            .effects
            .iter()
            .filter(|effect| effect.kind == crate::world_report::EffectKind::Focus)
            .collect();
        // The directional decision spends exactly one effect (a newcomer
        // admission effect may coincide on the same target through the
        // shared lane; both name the core-selected window).
        let directional: Vec<_> = focus_effects
            .iter()
            .filter(|effect| effect.id.starts_with("fx-"))
            .collect();
        assert_eq!(
            directional.len(),
            1,
            "exactly one directional focus effect, got {:?}",
            reply.effects
        );
        assert_eq!(directional[0].target, WindowId("win-new".to_owned()));
        assert!(
            focus_effects
                .iter()
                .all(|effect| effect.target == WindowId("win-new".to_owned())),
            "every focus effect names the selected target"
        );
    }

    #[test]
    fn tiled_move_keeps_focus_on_mover_through_one_effect() {
        // Tiled directional moves stay world-owned with a single spent
        // follow-focus effect on the mover: the adapter verifies the
        // foreground for its snap chain and never actuates twice.
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                1,
                vec![screen("out-a", 0, true), screen("out-b", 800, false)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 900)],
                None,
                None,
            ),
        );
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                vec![screen("out-a", 0, true), screen("out-b", 800, false)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 900)],
                Some(11),
                Some(WorldCommand::MoveDirectional {
                    window: WindowId("win-a".to_owned()),
                    direction: "right".to_owned(),
                    same_axis: crate::directional::SameAxisMove::default(),
                }),
            ),
        );
        // Tiled success carries no result: the transfer plus the single
        // follow-focus effect ARE the outcome.
        assert!(
            reply.command_result.is_none(),
            "tiled success carries no result, got {:?}",
            reply.command_result
        );
        assert!(
            reply
                .transfers
                .iter()
                .any(|transfer| transfer.target_output.0 == "out-b"),
            "cross-output move retains a transfer, got {:?}",
            reply.transfers
        );
        let focus_effects: Vec<_> = reply
            .effects
            .iter()
            .filter(|effect| effect.kind == crate::world_report::EffectKind::Focus)
            .collect();
        assert_eq!(
            focus_effects.len(),
            1,
            "exactly one follow-focus effect, got {:?}",
            reply.effects
        );
        assert_eq!(focus_effects[0].target, WindowId("win-a".to_owned()));
    }

    #[test]
    fn cross_output_floats_never_match_and_tiled_paths_stay_world_owned() {
        // Physical output vs membership home preserved: a sticky on
        // out-a never matches a float homed on out-b.
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                1,
                vec![screen("out-a", 0, true), screen("out-b", 800, false)],
                vec![
                    WindowFact {
                        floating: true,
                        sticky: true,
                        ..fact("win-sticky", "out-a", 100)
                    },
                    float_fact("win-far", "out-b", 900, 100),
                ],
                None,
                None,
            ),
        );
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                vec![screen("out-a", 0, true), screen("out-b", 800, false)],
                vec![
                    WindowFact {
                        floating: true,
                        sticky: true,
                        ..fact("win-sticky", "out-a", 100)
                    },
                    float_fact("win-far", "out-b", 900, 100),
                ],
                Some(11),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-sticky".to_owned()),
                    direction: "right".to_owned(),
                }),
            ),
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("focus-float-retained")
        );
        // Unknown windows refuse stale with the preserved cause and no
        // mutation: no continuation on stale lifetime evidence, one shared
        // vocabulary instead of a bare rejection.
        let reply = accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![screen("out-a", 0, true), screen("out-b", 800, false)],
                vec![
                    WindowFact {
                        floating: true,
                        sticky: true,
                        ..fact("win-sticky", "out-a", 100)
                    },
                    float_fact("win-far", "out-b", 900, 100),
                ],
                Some(12),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-gone".to_owned()),
                    direction: "right".to_owned(),
                }),
            ),
        );
        assert_eq!(
            outcome_str(&reply.command_result),
            Some("focus-float-refused-stale")
        );
        assert_eq!(
            reply
                .command_result
                .as_ref()
                .and_then(|result| match result {
                    CommandResult::Directional { cause, .. } => cause.clone(),
                    _ => None,
                }),
            Some("unknown-window".to_owned())
        );
        assert!(
            reply.effects.is_empty(),
            "stale refusals stay atomic, got {:?}",
            reply.effects
        );
    }
}
